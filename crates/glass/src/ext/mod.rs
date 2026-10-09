//! Loading and dispatching external extensions.
//!
//! Extensions are opt-in. Nothing is spawned unless config names it, which is
//! what keeps the default install at zero extensions and zero extra startup
//! cost.

pub mod host;
pub mod protocol;

use std::path::Path;

use host::{Extension, HostToolBridge, SubagentFn};
use protocol::{ExtConcurrency, ToolSpec, tool_triple};

/// Every loaded extension.
pub struct Registry {
    exts: Vec<Extension>,
    failures: Vec<String>,
    sources: std::collections::BTreeMap<String, String>,
    runner_factory: Option<Box<dyn Fn() -> SubagentFn>>,
    /// The tool list declared to the model, frozen after the
    /// first turn. See [`Registry::declared_tools`].
    frozen_tools: Option<Vec<(String, String, String)>>,
    /// Whether a change against the frozen list has been
    /// noted in the transcript already.
    freeze_noted: bool,
}

/// What changed against the frozen tool list.
#[derive(Debug, Default)]
pub struct FreezeDiff {
    /// Tools the live list has that the frozen list lacks.
    pub added: Vec<String>,
    /// Tools the frozen list declares that the live list lacks.
    pub removed: Vec<String>,
}

impl Registry {
    pub fn empty() -> Self {
        Self {
            exts: Vec::new(),
            failures: Vec::new(),
            sources: std::collections::BTreeMap::new(),
            runner_factory: None,
            frozen_tools: None,
            freeze_noted: false,
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.exts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exts.is_empty()
    }

    /// Spawn each named extension, skipping any that fail.
    ///
    /// A broken extension must not stop the session: the failure is reported
    /// and the rest load. That is the difference between "you have a broken
    /// extension" and "glass will not start".
    pub fn load(names: &[String], workspace: &Path, model: &str) -> Self {
        let mut reg = Self::empty();
        for name in names {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            match reg.enable(name, workspace, model) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("glass-core: {e}, skipping");
                    reg.failures.push(format!("{name}: {e}"));
                }
            }
        }
        reg
    }

    /// Install the subagent runner on every extension.
    ///
    /// Each extension gets its own runner, built by `make`,
    /// because a `SubagentFn` is a `Box` and cannot be shared.
    /// Only the extension that declares the `subagent` tool will
    /// actually invoke it; the rest hold an unused runner, which
    /// is cheap — an `Arc` clone and a `PathBuf`.
    pub fn set_subagent(&mut self, make: impl Fn() -> SubagentFn + 'static) {
        for ext in self.exts.iter_mut() {
            ext.set_subagent(make());
        }
        self.runner_factory = Some(Box::new(make));
    }

    /// Drain durable entries queued by any extension, for the
    /// agent to append to the session log.
    pub fn drain_custom(&mut self) -> Vec<(String, Option<String>)> {
        self.exts.iter_mut().flat_map(|e| e.take_custom()).collect()
    }

    /// Seed every extension with the durable entries it owns
    /// from the session log.
    ///
    /// Called once, after the session is open: the log is the
    /// store, and an extension rebuilds the state it owns from
    /// its own namespaced entries. Kinds are `owner/kind`, so
    /// each extension receives only its own.
    pub fn seed_custom(&mut self, records: &[crate::session::Record]) {
        let mut by_owner: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, Vec<String>>,
        > = std::collections::BTreeMap::new();
        for record in records {
            if let crate::session::Record::Custom { kind, data } = record {
                let owner = kind.split('/').next().unwrap_or_default().to_string();
                by_owner
                    .entry(owner)
                    .or_default()
                    .entry(kind.clone())
                    .or_default()
                    .extend(data.clone());
            }
        }
        for e in self.exts.iter_mut() {
            let mine: Vec<(String, Vec<String>)> = by_owner
                .remove(&e.manifest.name)
                .unwrap_or_default()
                .into_iter()
                .collect();
            e.seed_custom(mine);
        }
    }

    pub fn source(&self, name: &str) -> Option<&str> {
        self.sources.get(name).map(String::as_str)
    }
    pub fn reload(&mut self, specs: &[String], workspace: &Path, model: &str) {
        self.shutdown();
        self.exts.clear();
        self.sources.clear();
        self.failures.clear();
        for spec in specs {
            if let Err(error) = self.enable(spec, workspace, model) {
                self.failures.push(format!("{spec}: {error}"));
            }
        }
    }

    pub fn failures(&self) -> &[String] {
        &self.failures
    }

    pub fn names(&self) -> Vec<&str> {
        self.exts.iter().map(|e| e.manifest.name.as_str()).collect()
    }

    /// One line per loaded extension, with version and contributed tools, for
    /// `/extensions`.
    pub fn describe(&self) -> Vec<String> {
        self.exts
            .iter()
            .map(|e| {
                let tools: Vec<&str> = e.manifest.tools.iter().map(|t| t.name.as_str()).collect();
                let mut line = format!("{} v{}", e.manifest.name, e.manifest.version);
                if e.manifest.authorization_required {
                    line.push_str(" · required authorization");
                }
                if !tools.is_empty() {
                    line.push_str(&format!(" · tools: {}", tools.join(", ")));
                }
                if !e.manifest.system_prompt.trim().is_empty() {
                    line.push_str(" · contributes prompt text");
                }
                if let Some(p) = e.manifest.cache_warmer {
                    // Showing the knobs matters: warming is a
                    // net spend, so the operator should be able
                    // to see it is on and how it is tuned.
                    line.push_str(&format!(
                        " · warms cache after {}s (prefix ≥ {}B)",
                        p.refresh_after_s, p.min_prefix_bytes
                    ));
                }
                line
            })
            .collect()
    }

    pub fn enable(&mut self, spec: &str, workspace: &Path, model: &str) -> Result<(), String> {
        self.register(spawn_one(spec, workspace, model)?, spec)
    }
    pub fn register(&mut self, mut extension: Extension, spec: &str) -> Result<(), String> {
        let tools = extension
            .manifest
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>();
        let collision = self.names().contains(&extension.manifest.name.as_str())
            || tools
                .iter()
                .any(|name| matches!(*name, "bash" | "state") || self.has_tool(name))
            || tools
                .iter()
                .enumerate()
                .any(|(i, name)| tools[..i].contains(name));
        if collision {
            extension.shutdown();
            return Err("extension name or tool conflicts with an existing capability".into());
        }
        if let Some(make) = &self.runner_factory {
            extension.set_subagent(make());
        }
        self.sources
            .insert(extension.manifest.name.clone(), spec.into());
        self.exts.push(extension);
        Ok(())
    }

    /// Stop a loaded extension for this session, including its owned processes.
    pub fn disable(&mut self, name: &str) {
        if let Some(index) = self.exts.iter().position(|e| e.manifest.name == name) {
            let mut extension = self.exts.remove(index);
            self.sources.remove(name);
            extension.shutdown();
        }
    }

    /// Scheduling class an extension declared for one of its tools.
    pub fn tool_concurrency(&self, name: &str) -> Option<ExtConcurrency> {
        self.exts.iter().find_map(|e| {
            e.manifest
                .tools
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.concurrency)
        })
    }

    /// Every tool contributed by every extension.
    ///
    /// Extension tools are appended after the built-ins and cannot shadow
    /// them, so a prompt written against `bash` and `state` keeps working.
    pub fn tools(&self) -> Vec<(String, String, String)> {
        self.exts
            .iter()
            .flat_map(|e| e.manifest.tools.iter().map(tool_triple))
            .collect()
    }

    /// The tool list to declare to the model.
    ///
    /// Frozen after the first call: the list is part of the
    /// cacheable prefix, so a mid-session change would turn
    /// every later turn into a cache miss. An extension
    /// enabled or disabled between turns is therefore noted
    /// once — the diff comes back so the caller can record it
    /// in the transcript — and applies to the next session.
    /// This is the stability Claude Code's byte-stable tool
    /// list buys (2.1.268).
    pub fn declared_tools(&mut self) -> (Vec<(String, String, String)>, FreezeDiff) {
        let live = self.tools();
        let Some(frozen) = &self.frozen_tools else {
            self.frozen_tools = Some(live.clone());
            return (live, FreezeDiff::default());
        };
        if frozen == &live {
            return (frozen.clone(), FreezeDiff::default());
        }
        let added: Vec<String> = live
            .iter()
            .map(|(n, _, _)| n.clone())
            .filter(|n| !frozen.iter().any(|(f, _, _)| f == n))
            .collect();
        let removed: Vec<String> = frozen
            .iter()
            .map(|(n, _, _)| n.clone())
            .filter(|n| !live.iter().any(|(t, _, _)| t == n))
            .collect();
        if self.freeze_noted {
            return (frozen.clone(), FreezeDiff::default());
        }
        self.freeze_noted = true;
        (frozen.clone(), FreezeDiff { added, removed })
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.exts
            .iter()
            .any(|e| e.manifest.tools.iter().any(|t| t.name == name))
    }

    /// Find an extension that offers a named tool.
    fn tool_owner(&self, name: &str) -> Option<usize> {
        self.exts
            .iter()
            .position(|e| e.manifest.tools.iter().any(|t: &ToolSpec| t.name == name))
    }

    /// `/`-prefixed commands offered by extensions, for help text.
    pub fn commands(&self) -> Vec<(String, String)> {
        self.exts
            .iter()
            .flat_map(|e| {
                e.manifest
                    .commands
                    .iter()
                    .map(|c| (c.name.clone(), c.description.clone()))
            })
            .collect()
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.exts.iter().any(|e| e.has_command(name))
    }

    /// Combined system-prompt contributions.
    /// The cache-warmer policy of the first extension that
    /// declares one, if any.
    ///
    /// One warmer is enough: two refreshing the same prefix
    /// would pay the cache-write cost twice for the same hit.
    pub fn cache_warmer(&self) -> Option<protocol::WarmerPolicy> {
        self.exts.iter().find_map(|e| e.manifest.cache_warmer)
    }

    pub fn system_prompt(&mut self) -> String {
        let mut out = String::new();
        for e in &mut self.exts {
            if let Some(t) = e.system_prompt() {
                out.push_str("\n\n");
                out.push_str(&t);
            }
        }
        out
    }

    /// Run a tool that an extension owns.
    ///
    /// `host_tools` is the bridge the extension uses to call back into the
    /// harness while it runs. Codemode depends on this: its whole value is that
    /// one script can drive many tools without the model seeing each result.
    pub fn run_tool(
        &mut self,
        name: &str,
        arguments: &str,
        host_tools: &mut HostToolBridge<'_>,
    ) -> Option<String> {
        let idx = self.tool_owner(name)?;
        self.exts[idx].run_tool(name, arguments, host_tools)
    }

    /// Give every extension a chance to rewrite a tool result.
    ///
    /// Each one sees the previous one's output, so several extensions compose
    /// into a redaction pipeline rather than fighting over the same bytes. The
    /// bridge is passed through so a rewriting extension may run host tools
    /// (auto-verify runs the project's verifier this way).
    pub fn rewrite_tool_result(
        &mut self,
        call_id: &str,
        name: &str,
        body: &mut String,
        host_tools: &mut HostToolBridge<'_>,
    ) -> bool {
        let mut changed = false;
        for e in &mut self.exts {
            if let Some(new) = e.rewrite_tool_result(call_id, name, body, &mut *host_tools) {
                *body = new;
                changed = true;
            }
        }
        changed
    }

    /// Ask every extension whether a call may proceed. First refusal wins.
    pub fn authorize(&mut self, call_id: &str, name: &str, arguments: &str) -> Result<(), String> {
        for e in &mut self.exts {
            e.authorize(call_id, name, arguments)?;
        }
        Ok(())
    }

    pub fn turn_end(&mut self, text: &str, cacheable_prefix_bytes: usize) {
        for e in &mut self.exts {
            e.turn_end(text, cacheable_prefix_bytes);
        }
    }

    /// Invoke an extension slash command by name.
    /// Invoke the extension that owns `name`.
    ///
    /// Routed by owner rather than by asking every extension in turn: an
    /// extension that does not recognise the command must not be able to stall
    /// the dispatch until its timeout.
    pub fn run_command(&mut self, name: &str, args: &str) -> Option<String> {
        let idx = self.exts.iter().position(|e| e.has_command(name))?;
        self.exts[idx].run_command(name, args)
    }

    /// Shut every extension down cleanly.
    pub fn shutdown(&mut self) {
        for e in &mut self.exts {
            e.shutdown();
        }
    }
}

fn spawn_one(spec: &str, workspace: &Path, model: &str) -> Result<Extension, String> {
    let args = host::arguments(spec)?;
    if args[0].contains('/') && !Path::new(&args[0]).is_file() {
        return Err("Extension executable does not exist".into());
    }
    let cmd = serde_json::to_string(&args).map_err(|e| e.to_string())?;
    Extension::spawn(&cmd, &workspace.to_string_lossy(), model)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::protocol::PROTOCOL_VERSION;

    /// An extension that answers correctly, used to prove the host speaks the
    /// protocol rather than only proving the types compile.
    /// Write a throwaway extension script to a unique temp dir.
    ///
    /// Tests run in parallel, so a shared path would let one test's cleanup
    /// delete another's script mid-handshake.
    fn echo_ext(body: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("glass-ext-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ext.py");
        std::fs::write(&p, body).unwrap();
        p
    }

    const HELPER: &str = r#"
import json, sys
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"t","version":"1",
              "system_prompt":"from ext",
              "tools":[{"name":"ping","description":"d","schema":{"type":"object"}}],
              "commands":[{"name":"tcmd","description":"c"}]})
    elif t=="run_tool": send({"id":i,"output":"pong"})
    elif t=="command": send({"id":i,"output":"cmd out"})
    elif t=="system_prompt": send({"id":i,"text":"from ext"})
    elif t=="authorize": send({"id":i,"allow":False,"reason":"nope"})
    elif t=="tool_result": send({"id":i,"body":"REWRITTEN"})
    elif t=="turn_end": send({"id":i})
    elif t=="goodbye": break
"#;

    fn helper_script() -> String {
        HELPER.replace("PROTO", &PROTOCOL_VERSION.to_string())
    }

    fn python() -> Option<String> {
        for c in ["python3", "python"] {
            if std::process::Command::new(c)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
            {
                return Some(c.to_string());
            }
        }
        None
    }

    #[test]
    fn extension_completes_handshake_and_serves_hooks() {
        let Some(py) = python() else { return }; // no interpreter, skip
        let p = echo_ext(&helper_script());
        let mut e = Extension::spawn(&format!("{py} {}", p.display()), "/tmp", "test-model")
            .expect("handshake must succeed");

        assert_eq!(e.manifest.name, "t");
        assert_eq!(e.manifest.version, "1");
        assert!(e.has_command("tcmd"));
        assert_eq!(e.system_prompt().as_deref(), Some("from ext"));
        let mut nohost = |_n: &str, _a: &str| String::new();
        assert_eq!(
            e.run_tool("ping", "{}", &mut nohost).as_deref(),
            Some("pong")
        );
        assert_eq!(e.run_command("tcmd", "x").as_deref(), Some("cmd out"));
        assert_eq!(
            e.rewrite_tool_result("c1", "bash", "original", &mut nohost)
                .as_deref(),
            Some("REWRITTEN")
        );
        assert_eq!(e.authorize("c1", "bash", "rm"), Err("nope".to_string()));
        e.turn_end("done", 1234);
        e.shutdown();
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn registry_loads_and_aggregates() {
        let Some(py) = python() else { return };
        let p = echo_ext(&helper_script());
        let reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        assert_eq!(reg.len(), 1);
        assert!(reg.has_tool("ping"));
        assert!(reg.has_command("tcmd"));
        assert_eq!(reg.tools().len(), 1);
        assert_eq!(reg.commands().len(), 1);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn broken_extension_does_not_stop_the_load() {
        let reg = Registry::load(
            &["/definitely/not/here".to_string()],
            std::path::Path::new("/tmp"),
            "m",
        );
        assert_eq!(reg.len(), 0, "a bad path must be skipped, not fatal");
        assert!(reg.is_empty());
    }

    #[test]
    fn empty_registry_has_no_tools() {
        let mut reg = Registry::empty();
        assert!(reg.tools().is_empty());
        assert!(!reg.has_tool("anything"));
        assert!(reg.system_prompt().is_empty());
    }

    /// An extension that delegates a two-task batch and joins the reports.
    const BATCH_HELPER: &str = r#"
import json, sys
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"batch","version":"1",
              "tools":[{"name":"batch","description":"d","schema":{"type":"object"}}]})
    elif t=="run_tool":
        send({"type":"run_subagents","tasks":[{"task":"one"},{"task":"two"}]})
        while True:
            rep=json.loads(sys.stdin.readline())
            if rep.get("type")=="subagents_result":
                send({"id":i,"output":"|".join(rep.get("outputs") or [])})
                break
    elif t=="goodbye": break
"#;

    #[test]
    fn a_batch_extension_gets_one_report_per_task_in_order() {
        let Some(py) = python() else { return }; // no interpreter, skip
        let p = echo_ext(&BATCH_HELPER.replace("PROTO", &PROTOCOL_VERSION.to_string()));
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        reg.set_subagent(|| {
            Box::new(|tasks: &[crate::ext::protocol::SubagentTask]| {
                tasks.iter().map(|t| format!("done:{}", t.task)).collect()
            })
        });
        let mut nohost = |_n: &str, _a: &str| String::new();
        let joined = reg.run_tool("batch", "{}", &mut nohost).unwrap_or_default();
        // Both reports come back, in request order, each framed
        // so its content cannot read as session instructions.
        assert!(
            joined.contains("<subagent-report>\ndone:one\n</subagent-report>"),
            "the first report must come back framed: {joined}"
        );
        assert!(
            joined.contains("<subagent-report>\ndone:two\n</subagent-report>"),
            "the second report must come back framed: {joined}"
        );
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn a_malformed_batch_is_answered_with_an_error_not_a_hang() {
        let Some(py) = python() else { return };
        let body = r#"
import json, sys
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"bad","version":"1",
              "tools":[{"name":"bad","description":"d","schema":{"type":"object"}}]})
    elif t=="run_tool":
        send({"type":"run_subagents","tasks":[]})
        rep=json.loads(sys.stdin.readline())
        send({"id":i,"output":str(rep.get("error"))})
    elif t=="goodbye": break
"#;
        let p = echo_ext(&body.replace("PROTO", &PROTOCOL_VERSION.to_string()));
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        reg.set_subagent(|| Box::new(|_: &[crate::ext::protocol::SubagentTask]| Vec::new()));
        let mut nohost = |_n: &str, _a: &str| String::new();
        let out = reg.run_tool("bad", "{}", &mut nohost).unwrap_or_default();
        assert!(
            out.contains("at least one task"),
            "an empty batch must be refused with a reason: {out}"
        );
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }
}

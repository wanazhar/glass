//! Loading and dispatching external extensions.
//!
//! Extensions are opt-in. Nothing is spawned unless config names it, which is
//! what keeps the default install at zero extensions and zero extra startup
//! cost.

pub mod host;
pub mod protocol;

use std::path::Path;

pub use host::{CallOutcome, CallScope, CallStop, Cancel, CancelPath, ShutdownOutcome};

use host::{Extension, HostInputFn, HostToolBridge, SubagentFn};
use protocol::{ExtConcurrency, ToolSpec, tool_triple};

/// Every loaded extension.
pub struct Registry {
    exts: Vec<Extension>,
    failures: Vec<String>,
    sources: std::collections::BTreeMap<String, String>,
    runner_factory: Option<Box<dyn Fn() -> SubagentFn + Send>>,
    /// The tool list declared to the model, frozen after the
    /// first turn. See [`Registry::declared_tools`].
    frozen_tools: Option<Vec<(String, String, String)>>,
    /// Whether a change against the frozen list has been
    /// noted in the transcript already.
    freeze_noted: bool,
    deferred: bool,
    scope: std::collections::BTreeSet<String>,
    /// Per-extension results from the most recent shutdown, kept so a caller
    /// that ignored the return value can still read what happened.
    last_shutdown: Option<Vec<ShutdownOutcome>>,
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
    pub fn bind_session_path(&mut self, path: std::path::PathBuf) {
        for extension in &mut self.exts {
            extension.bind_session_path(path.clone());
        }
    }
    pub fn empty() -> Self {
        Self {
            exts: Vec::new(),
            failures: Vec::new(),
            sources: std::collections::BTreeMap::new(),
            runner_factory: None,
            frozen_tools: None,
            freeze_noted: false,
            deferred: std::env::var("GC_DEFER_TOOLS").as_deref()==Ok("1"),
            scope: std::collections::BTreeSet::new(),
            last_shutdown: None,
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
    pub fn set_subagent(&mut self, make: impl Fn() -> SubagentFn + Send + 'static) {
        for ext in self.exts.iter_mut() {
            ext.set_subagent(make());
        }
        self.runner_factory = Some(Box::new(make));
    }

    /// Drain entries queued by deferred extension implementations.
    /// Custom append requests are persisted synchronously before ack.
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
        // A reload that could not stop every old extension would silently
        // leave their processes behind, so the outcomes are kept alongside
        // the new load's failures instead of being thrown away.
        let stopped = self.shutdown();
        self.failures = stopped
            .iter()
            .filter(|o| o.killed || o.error.is_some())
            .map(|o| o.describe())
            .collect();
        self.exts.clear();
        self.sources.clear();
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
                .any(|name| matches!(*name, "bash" | "state" | "read" | "edit" | "write" | "view_image") || self.has_tool(name))
            || tools
                .iter()
                .enumerate()
                .any(|(i, name)| tools[..i].contains(name));
        if collision {
            // A rejected extension must not leave a process behind, and the
            // kill is the same one a session shutdown would use.
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
        if self.deferred { return (live.into_iter().filter(|(name,_,_)|self.scope.contains(name)).collect(),FreezeDiff::default()); }
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

    pub fn clear_scope(&mut self) { if self.deferred { self.scope.clear(); } }
    pub fn capabilities(&mut self, request:&serde_json::Value)->Result<serde_json::Value,String> {
        let object=request.as_object().ok_or("capabilities must be an object")?;
        if object.keys().any(|k|!matches!(k.as_str(),"search"|"activate")) {return Err("Unknown capability operation".into());}
        if let Some(activate)=object.get("activate") {
            if !self.deferred {return Err("Request-scoped tools require GC_DEFER_TOOLS=1; the default session schema remains frozen".into());}
            let names=activate.as_array().ok_or("activate must be tool names")?;
            if names.len()>16 {return Err("Activate at most 16 capabilities".into());}
            let mut scope=std::collections::BTreeSet::new();
            for name in names {let name=name.as_str().ok_or("Capability name must be text")?;if !self.has_tool(name) {return Err(format!("Unknown configured capability {name}"));}scope.insert(name.to_owned());}
            self.scope=scope;
        }
        let query=object.get("search").map(|v|v.as_str().ok_or("search must be text")).transpose()?.unwrap_or("").to_lowercase();
        let tools=self.tools().into_iter().filter(|(n,d,_)|format!("{n} {d}").to_lowercase().contains(&query)).take(64).map(|(name,description,_)|serde_json::json!({"name":name,"description":description.chars().take(240).collect::<String>(),"active":!self.deferred||self.scope.contains(&name)})).collect::<Vec<_>>();
        Ok(serde_json::json!({"capabilities":tools,"deferred":self.deferred,"scope":"Current user turn; resets at the next user turn. Explicit activation changes tool-schema prefix; request audit records the fingerprint."}))
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
    ///
    /// This entry point carries no call scope, so the work is unattributed and
    /// only the extension's own token can cancel it. Callers that know which
    /// model tool call they are serving — which is every caller inside the
    /// Run an extension tool with no scope. Test-only convenience: live
    /// dispatch (the agent loop, ACP, headless runners) must use
    /// [`Registry::run_tool_scoped`] so the work is attributable and
    /// cancellable from the caller's token.
    #[cfg(test)]
    pub fn run_tool(
        &mut self,
        name: &str,
        arguments: &str,
        host_tools: &mut HostToolBridge<'_>,
    ) -> Option<String> {
        self.run_tool_scoped(
            &CallScope::unscoped(),
            name,
            arguments,
            host_tools,
        )
        .into_output()
    }

    /// Run an extension tool, reporting exactly how the call ended.
    ///
    /// `scope` carries the model tool call this work descends from and the
    /// token that may still cancel it. Without it the call is uncancellable
    /// from the caller's side and cannot be traced back to the call that
    /// caused it, which is the gap this closes.
    pub fn run_tool_scoped(
        &mut self,
        scope: &CallScope,
        name: &str,
        arguments: &str,
        host_tools: &mut HostToolBridge<'_>,
    ) -> CallOutcome {
        let Some(idx) = self.tool_owner(name) else {
            return CallOutcome {
                output: None,
                stop: CallStop::Unknown,
            };
        };
        self.exts[idx].run_tool_scoped(name, arguments, host_tools, scope)
    }

    /// Run an extension tool with a scoped input callback, reporting exactly
    /// how the call ended.
    pub fn run_tool_with_input_scoped(
        &mut self,
        scope: &CallScope,
        name: &str,
        arguments: &str,
        host_tools: &mut HostToolBridge<'_>,
        host_input: &mut HostInputFn<'_>,
    ) -> CallOutcome {
        let Some(idx) = self.tool_owner(name) else {
            return CallOutcome {
                output: None,
                stop: CallStop::Unknown,
            };
        };
        self.exts[idx].run_tool_with_input_scoped(name, arguments, host_tools, host_input, scope)
    }

    /// Cancel every extension currently loaded.
    ///
    /// Cooperative: each extension stops waiting and its current exchange
    /// ends as cancelled. Processes that do not stop on their own are still
    /// taken down by `Drop` and by the next shutdown, so a cancel never
    /// leaves a process the session can no longer account for.
    ///
    /// Test-only for now: the live paths cancel through the per-turn
    /// [`Cancel`] token the agent loop holds, which cancels exactly the
    /// calls that belong to the turn rather than every loaded extension
    /// indiscriminately.
    #[cfg(test)]
    pub fn cancel_all(&self) {
        for e in &self.exts {
            e.cancel();
        }
    }

    /// Cancel one extension by name. Returns whether it was loaded.
    // Consumed by the agent loop's cancel and shutdown reporting paths,
    // which another task in this wave owns.
    #[allow(dead_code)]
    pub fn cancel(&self, name: &str) -> bool {
        match self.exts.iter().find(|e| e.manifest.name == name) {
            Some(e) => {
                e.cancel();
                true
            }
            None => false,
        }
    }

    /// Stop one extension the hard way, killing its process group.
    ///
    /// The fallback for an extension that will not stop cooperatively, and
    /// the path that says so: the return value names whether the kill fired
    /// or the extension had already stopped on its own, so a caller can
    /// report which of the two happened instead of assuming.
    ///
    /// One extension being stopped this way leaves every other extension
    /// running — a wedged extension must not take the session down with it.
    // Consumed by the agent loop's cancel and shutdown reporting paths,
    // which another task in this wave owns.
    #[allow(dead_code)]
    pub fn force_stop(&mut self, name: &str) -> Option<CancelPath> {
        let index = self.exts.iter().position(|e| e.manifest.name == name)?;
        Some(self.exts[index].force_stop())
    }

    /// The token that cancels one extension, so a caller can hold it across a
    /// call it did not itself start.
    // Consumed by the agent loop's cancel and shutdown reporting paths,
    // which another task in this wave owns.
    #[allow(dead_code)]
    pub fn cancel_token(&self, name: &str) -> Option<Cancel> {
        self.exts
            .iter()
            .find(|e| e.manifest.name == name)
            .map(|e| e.cancel_token())
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
        self.run_command_scoped(name, args, &CallScope::unscoped())
            .into_output()
    }

    /// Invoke an extension slash command under an explicit scope.
    pub fn run_command_scoped(
        &mut self,
        name: &str,
        args: &str,
        scope: &CallScope,
    ) -> CallOutcome {
        let Some(idx) = self.exts.iter().position(|e| e.has_command(name)) else {
            return CallOutcome {
                output: None,
                stop: CallStop::Unknown,
            };
        };
        self.exts[idx].run_command_scoped(name, args, scope)
    }

    /// Shut every extension down, reporting each one's outcome.
    ///
    /// One extension's failure to stop is reported and the rest still shut
    /// down: disabling or misbehaving one extension must not leave the
    /// session's other processes running. Every line goes to stderr so an
    /// operator sees which path each extension took.
    ///
    /// Idempotent. A second call signals nothing — each extension is already
    /// stopped — and reports each one as `already_stopped`.
    pub fn shutdown(&mut self) -> Vec<ShutdownOutcome> {
        let outcomes: Vec<ShutdownOutcome> = self
            .exts
            .iter_mut()
            .map(|e| {
                let outcome = e.shutdown();
                if outcome.killed || outcome.error.is_some() {
                    eprintln!("glass-core: {}", outcome.describe());
                }
                outcome
            })
            .collect();
        self.last_shutdown = Some(outcomes.clone());
        outcomes
    }

    /// Stop one extension, leaving every other extension running.
    ///
    /// Returns what happened to it. The remaining extensions keep their
    /// processes, their cancel tokens and their state, so a session with one
    /// extension disabled still has the rest of its capabilities.
    pub fn disable(&mut self, name: &str) -> Option<ShutdownOutcome> {
        let index = self.exts.iter().position(|e| e.manifest.name == name)?;
        let mut extension = self.exts.remove(index);
        self.sources.remove(name);
        Some(extension.shutdown())
    }

    /// The outcome of the last [`Registry::shutdown`], one line per extension.
    ///
    /// Empty before any shutdown has run. This is what makes a per-extension
    /// result reportable after the fact: `shutdown` already returned them, but
    /// a caller that discarded the return value can still read them here.
    // Consumed by the agent loop's cancel and shutdown reporting paths,
    // which another task in this wave owns.
    #[allow(dead_code)]
    pub fn last_shutdown(&self) -> &[ShutdownOutcome] {
        self.last_shutdown.as_deref().unwrap_or(&[])
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
        let scope = CallScope::from_call("call_1", Cancel::new());
        assert_eq!(
            e.run_tool_scoped("ping", "{}", &mut nohost, &scope)
                .into_output()
                .as_deref(),
            Some("pong")
        );
        assert_eq!(
            e.run_command_scoped("tcmd", "x", &scope)
                .into_output()
                .as_deref(),
            Some("cmd out")
        );
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
    fn extension_replies_require_unsigned_correlated_request_ids() {
        let Some(py) = python() else { return };
        for invalid in ["None", "'wrong'", "-1", "1.5", "'missing'"] {
            let response = format!("\n        bad={{'output':'unsolicited'}}\n        if {invalid} != 'missing': bad['id']={invalid}\n        send(bad)\n        send({{'id':i,'output':'real'}})");
            let body = helper_script().replace("elif t==\"run_tool\": send({\"id\":i,\"output\":\"pong\"})", &format!("elif t=='run_tool':{response}"));
            let path = echo_ext(&body);
            let mut ext = Extension::spawn(&format!("{py} {}",path.display()), "/tmp", "mock").unwrap();
            let mut bridge = |_name: &str, _args: &str| String::new();
            let result = ext.run_tool_scoped("ping", "{}", &mut bridge, &CallScope::from_call("origin", Cancel::new()));
            assert!(result.output.is_none(), "accepted {invalid} reply: {result:?}");
            assert!(matches!(result.stop, CallStop::Failed(ref reason) if reason.contains("request id")));
            drop(ext);
            std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
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
    fn custom_append_ack_follows_the_actual_session_log_write() {
        let Some(py) = python() else { return };
        let script = r#"
import json,sys
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
 r=json.loads(line); t=r.get("type"); i=r.get("id")
 if t=="hello": send({"id":i,"protocol":PROTO,"name":"durable","version":"1","tools":[{"name":"durable_tool","description":"d","schema":{"type":"object"}}]})
 elif t=="run_tool":
  send({"type":"append_custom","call_id":i,"kind":"durable/state","data":"saved"})
  rep=json.loads(sys.stdin.readline())
  send({"id":i,"output":str(rep.get("ok"))})
 elif t=="goodbye": break
"#;
        let dir = std::env::temp_dir().join(format!("glass-custom-ack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("extension.py");
        std::fs::write(&file, script.replace("PROTO", &PROTOCOL_VERSION.to_string())).unwrap();
        let session = crate::session::Session::create(&dir).unwrap();
        let mut reg = Registry::load(&[format!("{py} {}", file.display())], &dir, "mock");
        reg.bind_session_path(session.path.clone());
        let mut nohost = |_name: &str, _args: &str| String::new();
        assert_eq!(reg.run_tool("durable_tool", "{}", &mut nohost).as_deref(), Some("True"));
        let records = crate::session::Session::raw_records(&session.path).unwrap();
        assert!(matches!(records.as_slice(), [crate::session::Record::Custom { kind, data: Some(value) }] if kind == "durable/state" && value == "saved"));
        reg.shutdown();
        drop(session);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_state_refuses_oversized_growth_before_durable_ack() {
        let Some(py) = python() else { return };
        let script = helper_script().replace("import json, sys", "import json, sys\nseed_count = 99").replace("\"output\":\"pong\"", "\"output\":\"pong:\"+str(seed_count)").replace("elif t==\"turn_end\":", "elif t==\"seed_custom\": seed_count = len(r.get(\"entries\", [])); send({\"id\":i})\n    elif t==\"turn_end\":");
        let file = echo_ext(&script);
        let dir = file.parent().unwrap();
        let session = crate::session::Session::create(&dir.join("sessions")).unwrap();
        let mut reg = Registry::load(&[format!("{py} {}", file.display())], dir, "mock");
        reg.bind_session_path(session.path.clone());
        let ext = &mut reg.exts[0];
        ext.append_custom("t/state".into(), Some("x".repeat(1_700_000))).unwrap();
        assert!(ext.append_custom("t/state".into(), Some("x".repeat(1_700_000))).unwrap_err().contains("not saved"));
        assert_eq!(crate::session::Session::raw_records(&session.path).unwrap().len(), 1);
        ext.seed_custom(vec![("t/legacy".into(), vec!["x".repeat(4_200_000)])]);
        let mut host = |_name: &str, _args: &str| String::new();
        assert_eq!(reg.run_tool("ping", "{}", &mut host).as_deref(), Some("pong:0"));
        reg.shutdown();
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

    /// An extension that answers `run_tool` only after a long pause, and that
    /// records how the host called it on disk.
    const SLOW_HELPER: &str = r#"
import json, sys, time
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"slow","version":"1",
              "tools":[{"name":"slow","description":"d","schema":{"type":"object"}}],
              "commands":[{"name":"slowcmd","description":"c"}]})
    elif t=="run_tool":
        with open(MARKER,"w") as f:
            json.dump({"call_id":r.get("call_id"),"lineage":r.get("lineage")},f)
        time.sleep(DELAY)
        send({"id":i,"output":"finished"})
    elif t=="command":
        time.sleep(DELAY)
        send({"id":i,"output":"cmd out"})
    elif t=="goodbye": break
"#;

    fn slow_script(marker: &str, delay_secs: f64) -> String {
        SLOW_HELPER
            .replace("PROTO", &PROTOCOL_VERSION.to_string())
            .replace("MARKER", &serde_json::to_string(marker).unwrap())
            .replace("DELAY", &delay_secs.to_string())
    }

    /// An extension that records every `run_tool` it is given, so a test can
    /// see whether the host dispatched at all.
    const LINEAGE_HELPER: &str = r#"
import json, sys
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"lin","version":"1",
              "tools":[{"name":"lin","description":"d","schema":{"type":"object"}}]})
    elif t=="run_tool":
        send({"id":i,"output":json.dumps({"call_id":r.get("call_id"),"lineage":r.get("lineage")})})
    elif t=="goodbye": break
"#;

    fn lineage_script() -> String {
        LINEAGE_HELPER.replace("PROTO", &PROTOCOL_VERSION.to_string())
    }

    fn marker_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("glass-cancel-{}-{tag}.json", std::process::id()))
    }

    /// A cancel issued from the caller's token must stop a call already in
    /// flight, promptly. Before the token reached the running exchange, this
    /// waited out the full request timeout instead.
    #[test]
    fn a_cancel_reaches_a_call_that_is_already_running() {
        let Some(py) = python() else { return };
        let marker = marker_path("running");
        let p = echo_ext(&slow_script(&marker.to_string_lossy(), 30.0));
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let scope = CallScope::from_call("call_1", Cancel::new());
        // Cancel from another thread once the extension has been dispatched,
        // so this exercises a cancel that arrives mid-exchange.
        let token = scope.cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            token.cancel();
        });
        let started = std::time::Instant::now();
        let mut nohost = |_n: &str, _a: &str| String::new();
        let outcome = reg.run_tool_scoped(&scope, "slow", "{}", &mut nohost);
        let elapsed = started.elapsed();
        assert!(
            outcome.is_cancelled(),
            "a cancelled call is reported as cancelled, not as a failure: {:?}",
            outcome.stop
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "cancelling must stop the wait promptly, took {elapsed:?}"
        );
        assert!(
            marker.exists(),
            "the call was dispatched before it was cancelled"
        );
        let _ = std::fs::remove_file(&marker);
        reg.shutdown();
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// A cancel that arrives before dispatch must not send the request. If it
    /// did, the extension would run a tool call the session has already
    /// abandoned.
    #[test]
    fn a_cancel_before_dispatch_never_reaches_the_extension() {
        let Some(py) = python() else { return };
        let marker = marker_path("predispatch");
        let p = echo_ext(&slow_script(&marker.to_string_lossy(), 0.0));
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let scope = CallScope::from_call("call_1", Cancel::new());
        scope.cancel.cancel();
        let mut nohost = |_n: &str, _a: &str| String::new();
        let outcome = reg.run_tool_scoped(&scope, "slow", "{}", &mut nohost);
        assert!(outcome.is_cancelled(), "{:?}", outcome.stop);
        assert!(
            !outcome.is_completed(),
            "an abandoned call must not carry an output"
        );
        assert!(
            !marker.exists(),
            "the extension must not be asked to run a cancelled call"
        );
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// Cancellation and failure are different facts, and a transcript that
    /// conflates them tells the model the wrong thing about its own turn.
    #[test]
    fn a_cancel_is_reported_distinctly_from_an_extension_failure() {
        let Some(py) = python() else { return };
        // This extension exits instead of answering, so the call genuinely
        // fails rather than being cancelled.
        let body = r#"
import json, sys, os
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"dies","version":"1",
              "tools":[{"name":"dies","description":"d","schema":{"type":"object"}}]})
    elif t=="run_tool": os._exit(0)
    elif t=="goodbye": break
"#
        .replace("PROTO", &PROTOCOL_VERSION.to_string());
        let p = echo_ext(&body);
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let mut nohost = |_n: &str, _a: &str| String::new();
        let scope = CallScope::unscoped();
        let failed = reg.run_tool_scoped(&scope, "dies", "{}", &mut nohost);
        assert!(
            matches!(failed.stop, CallStop::Failed(_)),
            "a dead pipe is a failure: {:?}",
            failed.stop
        );
        let failure_line = failed.describe("dies");
        assert!(
            failure_line.starts_with("error:"),
            "a failure reads as an error: {failure_line}"
        );
        assert!(
            !failure_line.contains("cancelled"),
            "a failure must not be reported as a cancellation: {failure_line}"
        );
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn the_transcript_line_says_which_path_a_cancellation_took() {
        let cooperative = CallOutcome {
            output: None,
            stop: CallStop::Cancelled(CancelPath::Cooperative),
        };
        let line = cooperative.describe("slow");
        assert!(line.starts_with("cancelled:"), "{line}");
        assert!(
            line.contains("left running"),
            "the model is told the extension was not killed: {line}"
        );

        let killed = CallOutcome {
            output: None,
            stop: CallStop::Cancelled(CancelPath::Killed),
        };
        let line = killed.describe("slow");
        assert!(line.contains("killed"), "{line}");

        // A completed call reports its output, not a cancellation, whatever
        // the extension's own state happens to be.
        let done = CallOutcome {
            output: Some("real output".into()),
            stop: CallStop::Cancelled(CancelPath::Cooperative),
        };
        assert_eq!(done.describe("slow"), "real output");
    }

    /// A tool call the extension finished before the cancel landed must be
    /// reported as finished. Reporting it as cancelled would tell the model
    /// its work did not happen when it did.
    #[test]
    fn work_that_finished_before_a_cancel_is_not_reported_as_cancelled() {
        let Some(py) = python() else { return };
        let marker = marker_path("drain");
        let p = echo_ext(&slow_script(&marker.to_string_lossy(), 0.0));
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let scope = CallScope::from_call("call_1", Cancel::new());
        // Cancel while the reply is on its way back.
        scope.cancel.cancel();
        let mut nohost = |_n: &str, _a: &str| String::new();
        let outcome = reg.run_tool_scoped(&scope, "slow", "{}", &mut nohost);
        if !outcome.is_cancelled() {
            assert_eq!(
                outcome.output.as_deref(),
                Some("finished"),
                "a completed call keeps its output"
            );
        }
        let _ = std::fs::remove_file(&marker);
        reg.shutdown();
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// The startup token must survive the handshake and still reach a tool
    /// call afterwards. It used to be replaced with a fresh, permanently
    /// clear flag, which left it dropped on the floor.
    #[test]
    fn a_startup_cancel_token_still_reaches_a_tool_call_after_the_handshake() {
        let Some(py) = python() else { return };
        let marker = marker_path("survives");
        let p = echo_ext(&slow_script(&marker.to_string_lossy(), 30.0));
        let token = Cancel::new();
        let mut reg = Registry::empty();
        let mut extension = Extension::spawn_cancellable(
            &format!("{py} {}", p.display()),
            "/tmp",
            "test-model",
            token.clone(),
        )
        .expect("handshake must succeed");
        // What `addons.rs` does after a `/extensions` enable.
        let live = extension.finish_startup();
        assert!(
            !live.is_cancelled(),
            "finishing startup must leave a usable token, not a poisoned one"
        );
        assert!(
            extension.is_live(),
            "the extension reports itself live after startup"
        );
        reg.register(extension, "spec").expect("registers");

        // Cancel through the same flag the startup caller holds.
        live.cancel();
        assert!(
            reg.cancel_token("slow").is_some_and(|t| t.is_cancelled()),
            "the registry hands out the very same token"
        );
        let scope = CallScope::from_call("call_1", Cancel::new());
        let mut nohost = |_n: &str, _a: &str| String::new();
        let outcome = reg.run_tool_scoped(&scope, "slow", "{}", &mut nohost);
        assert!(
            outcome.is_cancelled(),
            "a token set after startup still cancels: {:?}",
            outcome.stop
        );
        let _ = std::fs::remove_file(&marker);
        reg.shutdown();
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// A cancelled `/extensions` startup must still cancel — the startup
    /// path is what a user actually presses Esc on.
    #[test]
    fn a_cancelled_startup_still_cancels() {
        let Some(py) = python() else { return };
        // This extension never answers the handshake, so only the cancel can
        // end the wait.
        let body = r#"
import json, sys, time
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    r=json.loads(line)
    if r.get("type")=="hello": time.sleep(60)
    elif r.get("type")=="goodbye": break
"#;
        let p = echo_ext(body);
        let token = Cancel::new();
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watcher = flag.clone();
        let trigger = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            watcher.store(true, std::sync::atomic::Ordering::Release);
            trigger.cancel();
        });
        let started = std::time::Instant::now();
        let err = Extension::spawn_cancellable(
            &format!("{py} {}", p.display()),
            "/tmp",
            "test-model",
            token.clone(),
        )
        .err()
        .expect("a cancelled handshake must fail");
        let elapsed = started.elapsed();
        assert!(
            err.contains("cancelled"),
            "a cancelled startup must not be reported as a broken extension: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the cancel must end the handshake promptly, took {elapsed:?}"
        );
        assert!(
            flag.load(std::sync::atomic::Ordering::Acquire),
            "the test actually cancelled"
        );
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// The originating model tool call id must arrive with the request and
    /// come back unchanged, or extension work cannot be attributed to the
    /// call that caused it.
    #[test]
    fn the_originating_call_id_round_trips_to_the_extension() {
        let Some(py) = python() else { return };
        let p = echo_ext(&lineage_script());
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let mut nohost = |_n: &str, _a: &str| String::new();
        let scope = CallScope::from_call("call_round_trip", Cancel::new());
        let outcome = reg.run_tool_scoped(&scope, "lin", "{}", &mut nohost);
        let body: serde_json::Value = serde_json::from_str(
            &outcome.output.expect("the extension answered"),
        )
        .expect("the extension echoed valid JSON");
        assert_eq!(body["call_id"], "call_round_trip");
        let lineage = crate::ext::protocol::CallLineage::from_json(&body["lineage"])
            .expect("the extension received a well-formed lineage");
        assert_eq!(lineage.origin(), "call_round_trip");
        assert_eq!(lineage.depth(), 0);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// A scope already at the nesting bound must be refused before dispatch,
    /// not after the extension has started working.
    #[test]
    fn an_over_deep_scope_is_refused_before_the_extension_is_dispatched() {
        let Some(py) = python() else { return };
        let mut chain = crate::ext::protocol::CallLineage::root("call_root");
        for level in 1..=crate::ext::protocol::MAX_NESTED_CALL_DEPTH {
            chain = chain.descend(&format!("n{level}")).expect("within the bound");
        }
        assert_eq!(chain.depth(), crate::ext::protocol::MAX_NESTED_CALL_DEPTH);
        let p = echo_ext(&lineage_script());
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let scope = CallScope {
            lineage: chain,
            cancel: Cancel::new(),
        };
        let mut nohost = |_n: &str, _a: &str| String::new();
        let outcome = reg.run_tool_scoped(&scope, "lin", "{}", &mut nohost);
        assert!(
            !outcome.is_completed(),
            "an over-deep call must not run: {:?}",
            outcome.output
        );
        let line = outcome.describe("lin");
        assert!(line.contains("deep"), "the refusal says why: {line}");
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// A nested call whose lineage loops must be refused by the host instead
    /// of being dispatched, and the extension must be told rather than left
    /// waiting.
    #[test]
    fn a_cyclic_nested_lineage_is_refused_with_a_reason() {
        let Some(py) = python() else { return };
        // The extension claims the host's own root is its nested call id,
        // which would loop the chain back onto itself.
        let body = r#"
import json, sys
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"cyc","version":"1",
              "tools":[{"name":"cyc","description":"d","schema":{"type":"object"}}]})
    elif t=="run_tool":
        send({"type":"call_tool","call_id":"call_root","name":"bash","arguments":"{}",
              "lineage":{"origin":"call_root","depth":1,"path":["call_root","call_root"]}})
        rep=json.loads(sys.stdin.readline())
        send({"id":i,"output":str(rep.get("body"))})
    elif t=="goodbye": break
"#
        .replace("PROTO", &PROTOCOL_VERSION.to_string());
        let p = echo_ext(&body);
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let mut nohost = |_n: &str, _a: &str| String::new();
        let scope = CallScope::from_call("call_root", Cancel::new());
        let out = reg
            .run_tool_scoped(&scope, "cyc", "{}", &mut nohost)
            .into_output()
            .unwrap_or_default();
        assert!(
            out.contains("repeats") || out.contains("cyclic"),
            "a looping lineage must be refused with a reason: {out}"
        );
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// One extension ignoring shutdown must not hide the others' outcomes,
    /// and every extension must actually be stopped.
    #[test]
    fn shutdown_reports_each_extension_separately() {
        let Some(py) = python() else { return };
        // This one ignores `goodbye` and never exits.
        let stubborn = r#"
import json, sys, time
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
line = sys.stdin.readline()
r = json.loads(line)
send({"id": r["id"], "protocol": PROTO, "name": "stubborn", "version": "1",
      "tools": [{"name": "stub", "description": "d", "schema": {"type": "object"}}]})
# Never read stdin again and never exit. This is what forces the host to
# escalate to the process-group kill.
while True:
    time.sleep(3600)
"#
        .replace("PROTO", &PROTOCOL_VERSION.to_string());
        let well = LINEAGE_HELPER.replace("PROTO", &PROTOCOL_VERSION.to_string())
            .replace("\"lin\"", "\"polite\"")
            .replace("\"lin\"", "\"polite\"");
        let a = echo_ext(&stubborn);
        let b = echo_ext(&well);
        let mut reg = Registry::load(
            &[format!("{py} {}", a.display()), format!("{py} {}", b.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        assert_eq!(reg.len(), 2);
        let outcomes = reg.shutdown();
        assert_eq!(outcomes.len(), 2, "every extension reports an outcome");
        let by_name: std::collections::BTreeMap<&str, &ShutdownOutcome> =
            outcomes.iter().map(|o| (o.name.as_str(), o)).collect();
        assert!(
            by_name["stubborn"].killed,
            "an extension that ignores goodbye must be killed"
        );
        assert!(
            !by_name["stubborn"].error.is_some(),
            "being killed is not an error: {:?}",
            by_name["stubborn"].error
        );
        assert!(
            !by_name["polite"].killed,
            "a cooperative extension is not killed: {:?}",
            by_name["polite"]
        );
        // Both outcomes are reported by name, not collapsed into one.
        assert!(by_name["stubborn"].describe().contains("killed"));
        assert!(by_name["polite"].describe().contains("stopped on request"));
        let _ = std::fs::remove_dir_all(a.parent().unwrap());
        let _ = std::fs::remove_dir_all(b.parent().unwrap());
    }

    #[test]
    fn shutdown_is_idempotent() {
        let Some(py) = python() else { return };
        let p = echo_ext(&helper_script());
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let first = reg.shutdown();
        assert_eq!(first.len(), 1);
        assert!(!first[0].already_stopped, "the first call does the work");
        assert!(reg.last_shutdown().len() == 1, "the outcome is retained");

        let second = reg.shutdown();
        assert_eq!(second.len(), 1);
        assert!(
            second[0].already_stopped,
            "a second shutdown signals nothing: {:?}",
            second[0]
        );
        assert!(
            !second[0].killed,
            "a second shutdown must not re-kill an exited process"
        );
        // And it is idempotent all the way down to one extension.
        assert!(reg.exts[0].shutdown().already_stopped);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// Disabling one extension must leave the session's other extensions
    /// running and usable.
    #[test]
    fn disabling_one_extension_leaves_the_others_running() {
        let Some(py) = python() else { return };
        let stub = r#"
import json, sys, time
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({"id":i,"protocol":PROTO,"name":"doomed","version":"1",
              "tools":[{"name":"doomed","description":"d","schema":{"type":"object"}}]})
"#
        .replace("PROTO", &PROTOCOL_VERSION.to_string());
        let keeper = LINEAGE_HELPER.replace("PROTO", &PROTOCOL_VERSION.to_string());
        let a = echo_ext(&stub);
        let b = echo_ext(&keeper);
        let mut reg = Registry::load(
            &[format!("{py} {}", a.display()), format!("{py} {}", b.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        assert_eq!(reg.len(), 2);
        let outcome = reg.disable("doomed").expect("the extension was loaded");
        assert_eq!(outcome.name, "doomed");
        assert_eq!(reg.len(), 1, "only the named extension is removed");
        assert!(!reg.has_tool("doomed"), "its tool is gone");
        assert!(reg.has_tool("lin"), "the other extension keeps its tool");

        // And the survivor still answers.
        let mut nohost = |_n: &str, _a: &str| String::new();
        let scope = CallScope::unscoped();
        let out = reg.run_tool_scoped(&scope, "lin", "{}", &mut nohost);
        assert_eq!(out.stop, CallStop::Unknown);
        assert!(out.is_completed(), "the survivor still works");
        reg.shutdown();
        let _ = std::fs::remove_dir_all(a.parent().unwrap());
        let _ = std::fs::remove_dir_all(b.parent().unwrap());
    }

    /// Cancelling one extension must not disturb another's running call.
    #[test]
    fn cancelling_one_extension_leaves_the_others_alone() {
        let Some(py) = python() else { return };
        let p = echo_ext(&lineage_script());
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        let mut nohost = |_n: &str, _a: &str| String::new();
        let scope = CallScope::unscoped();
        assert!(reg.run_tool_scoped(&scope, "lin", "{}", &mut nohost).is_completed());
        assert!(reg.cancel("lin"), "the extension was found");
        assert!(reg.cancel_token("lin").is_some_and(|t| t.is_cancelled()));
        // Cancelling everything covers every loaded extension at once.
        let b = echo_ext(&lineage_script().replace("\"lin\"", "\"lin2\""));
        let reg2 = Registry::load(
            &[format!("{py} {}", b.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        reg2.cancel_all();
        assert!(
            reg2
                .cancel_token("lin2")
                .is_some_and(|t| t.is_cancelled()),
            "cancel_all reaches every loaded extension"
        );
        let _ = std::fs::remove_dir_all(b.parent().unwrap());
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// A hard stop says which path it took, and leaves the rest of the
    /// registry alone.
    #[test]
    fn a_hard_stop_reports_that_it_killed() {
        let Some(py) = python() else { return };
        let p = echo_ext(&lineage_script());
        let mut reg = Registry::load(
            &[format!("{py} {}", p.display())],
            std::path::Path::new("/tmp"),
            "test-model",
        );
        assert_eq!(reg.force_stop("lin"), Some(CancelPath::Killed));
        // A second hard stop finds nothing left to kill and says so.
        assert_eq!(reg.force_stop("lin"), Some(CancelPath::Cooperative));
        assert_eq!(reg.force_stop("absent"), None);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }
}

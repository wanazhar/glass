//! Loading and dispatching external extensions.
//!
//! Extensions are opt-in. Nothing is spawned unless config names it, which is
//! what keeps the default install at zero extensions and zero extra startup
//! cost.

pub mod host;
pub mod protocol;

use std::path::{Path, PathBuf};

use host::Extension;
use protocol::{tool_triple, ToolSpec};

/// Every loaded extension.
pub struct Registry {
    exts: Vec<Extension>,
}

impl Registry {
    pub fn empty() -> Self {
        Self { exts: Vec::new() }
    }

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
            match spawn_one(name, workspace, model) {
                Ok(ext) => reg.exts.push(ext),
                Err(e) => eprintln!("\x1b[33mglass: {e}, skipping\x1b[0m"),
            }
        }
        reg
    }

    /// Load every executable in a directory (used for `~/.config/glass/ext`).
    pub fn load_dir(dir: &Path, workspace: &Path, model: &str) -> Self {
        let mut reg = Self::empty();
        let Ok(entries) = std::fs::read_dir(dir) else { return reg };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        paths.sort();

        for p in paths {
            match spawn_one(&p.to_string_lossy(), workspace, model) {
                Ok(ext) => reg.exts.push(ext),
                Err(e) => eprintln!("\x1b[33mglass: {e}, skipping\x1b[0m"),
            }
        }
        reg
    }

    pub fn names(&self) -> Vec<&str> {
        self.exts.iter().map(|e| e.manifest.name.as_str()).collect()
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

    pub fn has_tool(&self, name: &str) -> bool {
        self.exts.iter().any(|e| e.manifest.tools.iter().any(|t| t.name == name))
    }

    /// Find an extension that offers a named tool.
    fn tool_owner(&self, name: &str) -> Option<usize> {
        self.exts.iter().position(|e| e.manifest.tools.iter().any(|t: &ToolSpec| t.name == name))
    }

    /// `/`-prefixed commands offered by extensions, for help text.
    pub fn commands(&self) -> Vec<(String, String)> {
        self.exts
            .iter()
            .flat_map(|e| e.manifest.commands.iter().map(|c| (c.name.clone(), c.description.clone())))
            .collect()
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.exts.iter().any(|e| e.has_command(name))
    }

    /// Combined system-prompt contributions.
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
        host_tools: &mut dyn FnMut(&str, &str) -> String,
    ) -> Option<String> {
        let idx = self.tool_owner(name)?;
        self.exts[idx].run_tool(name, arguments, host_tools)
    }

    /// Give every extension a chance to rewrite a tool result.
    ///
    /// Each one sees the previous one's output, so several extensions compose
    /// into a redaction pipeline rather than fighting over the same bytes.
    pub fn rewrite_tool_result(&mut self, call_id: &str, name: &str, body: &mut String) -> bool {
        let mut changed = false;
        for e in &mut self.exts {
            if let Some(new) = e.rewrite_tool_result(call_id, name, body) {
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

    pub fn turn_end(&mut self, text: &str) {
        for e in &mut self.exts {
            e.turn_end(text);
        }
    }

    /// Invoke an extension slash command by name.
    pub fn run_command(&mut self, name: &str, args: &str) -> Option<String> {
        for e in &mut self.exts {
            if e.has_command(name)
                && let Some(out) = e.run_command(name, args)
            {
                return Some(out);
            }
        }
        None
    }

    /// Shut every extension down cleanly.
    pub fn shutdown(&mut self) {
        for e in &mut self.exts {
            e.shutdown();
        }
    }
}

fn spawn_one(spec: &str, workspace: &Path, model: &str) -> Result<Extension, String> {
    // A spec may carry arguments, so only the program is checked for existence.
    let program = spec.split_whitespace().next().unwrap_or(spec);

    // A bare name is looked up on PATH; anything with a separator is treated
    // as a path, which is what lets `~/.config/glass/ext/foo` work.
    let cmd = if program.contains('/') || program.starts_with('~') {
        let expanded = if let Some(rest) = program.strip_prefix("~/") {
            match std::env::var("HOME") {
                Ok(h) => format!("{h}/{rest}"),
                Err(_) => program.to_string(),
            }
        } else {
            program.to_string()
        };
        if !Path::new(&expanded).exists() {
            return Err(format!("extension `{spec}` does not exist"));
        }
        // Re-assemble with the original arguments.
        spec.replacen(program, &expanded, 1)
    } else {
        spec.to_string()
    };

    Extension::spawn(&cmd, &workspace.to_string_lossy(), model)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::protocol::PROTOCOL_VERSION;

    /// An extension that answers correctly, used to prove the host speaks the
    /// protocol rather than only proving the types compile.
    fn echo_ext(body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("glass-ext-test-{}-{}", std::process::id(), body.len()));
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
    elif t=="turn_end": pass
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
        let mut e = Extension::spawn(
            &format!("{py} {}", p.display()),
            "/tmp",
            "test-model",
        )
        .expect("handshake must succeed");

        assert_eq!(e.manifest.name, "t");
        assert_eq!(e.manifest.version, "1");
        assert!(e.has_command("tcmd"));
        assert_eq!(e.system_prompt().as_deref(), Some("from ext"));
        assert_eq!(e.run_tool("ping", "{}").as_deref(), Some("pong"));
        assert_eq!(e.run_command("tcmd", "x").as_deref(), Some("cmd out"));
        assert_eq!(
            e.rewrite_tool_result("c1", "bash", "original").as_deref(),
            Some("REWRITTEN")
        );
        assert_eq!(e.authorize("c1", "bash", "rm"), Err("nope".to_string()));
        e.turn_end("done");
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
}

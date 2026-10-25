//! Mod loading.
//!
//! Mods are loaded by name from a registry, not from arbitrary dynamic
//! libraries. That is a deliberate trade: native `.so` loading means owning a
//! stable C ABI forever and accepting that a user mod can segfault the
//! harness, whereas named mods can be vetted, and the same extension needs are
//! already served by hooks, extra tools, and skills.
//!
//! Adding a mod to the registry below is a one-line change, and a user can add
//! their own `~/.config/glass-core/skills/*.md` skill without touching this file.

use std::collections::BTreeMap;

use glass_mod_api::{Mod_, Overlay, ToolCall, ToolOutput, TurnContext};

/// Registry of mods that ship in the binary, as constructors.
///
/// Factories rather than instances so every session gets its own mod, which
/// matters as soon as a mod holds per-session state.
///
/// The set is intentionally tiny. Each entry has to earn its place, because
/// every loaded mod runs on the hot path of every turn.
fn registry() -> BTreeMap<&'static str, fn() -> Box<dyn Mod_>> {
    let mut m: BTreeMap<&'static str, fn() -> Box<dyn Mod_>> = BTreeMap::new();
    m.insert("noisy", || Box::new(Noisy));
    m.insert("redact", || Box::new(Redact));
    m
}

/// Load the named mods, skipping any that are not registered.
///
/// An unknown name is a warning rather than a fatal error: a typo in config
/// should not stop someone from starting their session.
pub fn load(names: &[String]) -> Result<Vec<Box<dyn Mod_>>, String> {
    let reg = registry();
    let mut out: Vec<Box<dyn Mod_>> = Vec::new();

    for n in names {
        let n = n.trim();
        if n.is_empty() {
            continue;
        }
        match reg.get(n) {
            Some(make) => out.push(make()),
            None => eprintln!("\x1b[33mglass-core: unknown mod `{n}`, skipping\x1b[0m"),
        }
    }
    Ok(out)
}

/// A focused output-redaction mod with no extra tools or render hooks.
struct Redact;
impl Mod_ for Redact {
    fn name(&self) -> &str {
        "redact"
    }
    fn on_tool_result(&self, _call: &ToolCall, output: &mut ToolOutput) {
        let clean = redact(&output.body);
        if clean != output.body {
            output.body = clean;
            output.rewritten = true;
        }
    }
}

pub fn available() -> Vec<String> {
    registry().keys().map(|name| (*name).to_string()).collect()
}

/// Example mod, present to prove the whole hook surface compiles and runs.
///
/// It does four things, one per hook a real mod would reach for: contributes
/// prompt text, redacts secrets out of tool output, adds a tool, and paints
/// an overlay in the transcript viewer.
struct Noisy;

impl Mod_ for Noisy {
    fn name(&self) -> &str {
        "noisy"
    }

    fn system_prompt(&self, _ctx: &TurnContext) -> Option<String> {
        Some(
            "Tool output is redacted by a mod: secrets matching common patterns are \
             replaced before you see them."
                .to_string(),
        )
    }

    fn on_tool_result(&self, _call: &ToolCall, output: &mut ToolOutput) {
        let redacted = redact(&output.body);
        if redacted != output.body {
            output.body = redacted;
            output.rewritten = true;
        }
    }

    fn tools(&self) -> Vec<(String, String, String)> {
        vec![(
            "todo".to_string(),
            "Record a task list. Demonstrates a mod-supplied tool.".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": { "items": { "type": "array", "items": { "type": "string" } } }
            })
            .to_string(),
        )]
    }

    fn authorize(&self, call: &ToolCall) -> Result<(), String> {
        // This is the hook a user would use to add an approval prompt. glass
        // ships no permission system of its own; this shows where one goes.
        if call.name != "bash" {
            return Ok(());
        }
        let cmd = call.arg_str("command").unwrap_or_default();
        if cmd.contains("rm -rf /") {
            return Err("refusing a recursive delete of /".to_string());
        }
        Ok(())
    }

    fn on_render(&self, overlay: &mut Overlay, _viewport: (u16, u16)) {
        overlay
            .lines
            .push("\x1b[90mctrl-g transcript · enter collapse · f follow\x1b[0m".to_string());
    }

    fn on_turn_end(&self, _ctx: &TurnContext, output: &str) {
        if output.contains("TODO") {
            eprintln!("\x1b[33m[noisy] turn mentioned a TODO\x1b[0m");
        }
    }
}

/// Replace anything that looks like a credential with a placeholder.
///
/// Not a security control, just a demonstration of `on_tool_result`. A real
/// implementation would use proper entropy checks rather than substrings.
fn redact(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    for line in body.lines() {
        let mut hit = false;
        for marker in ["AKIA", "ghp_", "sk-", "xoxb-", "-----BEGIN"] {
            if line.contains(marker) {
                hit = true;
                break;
            }
        }
        if hit {
            out.push_str("[redacted by mod: line contained a credential marker]");
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> TurnContext<'static> {
        TurnContext {
            messages: &[],
            model: "test",
            turn_index: 0,
        }
    }

    #[test]
    fn loads_known_mods() {
        let mods = load(&["noisy".to_string()]).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name(), "noisy");
    }

    #[test]
    fn unknown_mod_is_skipped_not_fatal() {
        let mods = load(&["nope".to_string(), "noisy".to_string()]).unwrap();
        assert_eq!(mods.len(), 1);
    }

    #[test]
    fn empty_names_are_ignored() {
        assert!(load(&["".into(), "  ".into()]).unwrap().is_empty());
    }

    #[test]
    fn mod_contributes_prompt_text() {
        let m = Noisy;
        let p = m.system_prompt(&ctx()).unwrap();
        assert!(p.contains("redacted"));
    }

    #[test]
    fn mod_redacts_credentials() {
        let m = Noisy;
        let call = ToolCall {
            id: "1".into(),
            name: "bash".into(),
            arguments: "{}".into(),
        };
        let mut out = ToolOutput::new("export KEY=AKIAIOSFODNN7EXAMPLE\nsafe line");
        m.on_tool_result(&call, &mut out);
        assert!(out.rewritten);
        assert!(!out.body.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(
            out.body.contains("safe line"),
            "non-secret lines must survive"
        );
    }

    #[test]
    fn mod_leaves_clean_output_untouched() {
        let m = Noisy;
        let call = ToolCall {
            id: "1".into(),
            name: "bash".into(),
            arguments: "{}".into(),
        };
        let mut out = ToolOutput::new("all clear");
        m.on_tool_result(&call, &mut out);
        assert!(!out.rewritten);
        assert_eq!(out.body, "all clear");
    }

    #[test]
    fn mod_can_block_a_command() {
        let m = Noisy;
        let bad = ToolCall {
            id: "1".into(),
            name: "bash".into(),
            arguments: r#"{"command":"rm -rf / --no-preserve-root"}"#.into(),
        };
        assert!(m.authorize(&bad).is_err());

        let ok = ToolCall {
            id: "2".into(),
            name: "bash".into(),
            arguments: r#"{"command":"ls"}"#.into(),
        };
        assert!(m.authorize(&ok).is_ok());
    }

    #[test]
    fn mod_tool_does_not_shadow_builtins() {
        // Mods cannot shadow the compact core tool surface.
        let all = crate::provider::collect_tools(&[Box::new(Noisy) as Box<dyn Mod_>]);
        let names: Vec<_> = all.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "bash",
                "state",
                "web_search",
                "web_fetch",
                "display_image",
                "view_image",
                "read",
                "edit",
                "write",
                "todo"
            ]
        );
    }

    #[test]
    fn mod_paints_an_overlay() {
        let mut overlay = Overlay::default();
        Noisy.on_render(&mut overlay, (80, 24));
        assert_eq!(overlay.lines.len(), 1);
        assert!(overlay.lines[0].contains("transcript"));
    }

    #[test]
    fn builtin_names_include_native_files() {
        let names: Vec<String> = crate::provider::builtin_tools()
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        assert_eq!(
            names,
            vec![
                "bash",
                "state",
                "web_search",
                "web_fetch",
                "display_image",
                "view_image",
                "read",
                "edit",
                "write"
            ]
        );
    }
}

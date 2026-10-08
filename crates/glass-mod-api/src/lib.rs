//! The entire extension surface of glass.
//!
//! This crate is the stability boundary. Everything a mod can observe or
//! influence is named here. It has zero dependencies and it will stay that
//! way: if a hook ever needs a new type, the type is defined in this file.

use std::collections::BTreeMap;

/// A conversation turn as it appears in the model context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    User(String),
    Assistant(String),
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    /// Result of a tool call, keyed by the tool call id that requested it.
    ToolResult {
        call_id: String,
        name: String,
        body: String,
    },
    /// Injected by the harness, not the model. Never returned as model output.
    System(String),
}

impl Message {
    pub fn role(&self) -> &'static str {
        match self {
            Message::User(_) => "user",
            Message::Assistant(_) | Message::ToolCall { .. } => "assistant",
            Message::ToolResult { .. } => "user",
            Message::System(_) => "system",
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Message::User(s) | Message::Assistant(s) | Message::System(s) => s,
            Message::ToolResult { body, .. } => body,
            Message::ToolCall { arguments, .. } => arguments,
        }
    }
}

/// A tool invocation requested by the model.
#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON arguments as sent by the model.
    pub arguments: String,
}

impl ToolCall {
    /// Return the raw JSON text of a top-level argument, whatever its type.
    ///
    /// Returning the slice rather than a decoded value keeps this crate free
    /// of a JSON dependency, which is what lets it stay dependency-free at
    /// all. Callers that need structure hand the text to their own parser.
    pub fn arg_raw(&self, key: &str) -> Option<String> {
        let needle = format!("\"{key}\"");
        let start = self.arguments.find(&needle)? + needle.len();
        let rest = self.arguments[start..].trim_start();
        let rest = rest.strip_prefix(':')?.trim_start();
        // Scan to the end of this value, tracking nesting and string state so
        // braces, brackets, and escaped quotes inside the value are respected.
        let bytes = rest.as_bytes();
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (i, &b) in bytes.iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_string = false;
                }
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    if depth == 0 {
                        return Some(rest[..i].trim().to_string());
                    }
                    depth -= 1;
                }
                b',' if depth == 0 => return Some(rest[..i].trim().to_string()),
                _ => {}
            }
        }
        Some(rest.trim().to_string())
    }

    /// Best-effort extraction of a string argument.
    ///
    /// Handles escapes. Returns `None` when the key is absent or is not a
    /// string, so a caller can distinguish "missing" from "wrong type".
    pub fn arg_str(&self, key: &str) -> Option<String> {
        let raw = self.arg_raw(key)?;
        let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let cp = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(cp)?);
                }
                // Unknown escape: keep the escaped character verbatim.
                other => out.push(other),
            }
        }
        Some(out)
    }
}

/// Everything a mod is allowed to see when a model turn is about to start.
pub struct TurnContext<'a> {
    pub messages: &'a [Message],
    pub model: &'a str,
    pub turn_index: usize,
}

impl TurnContext<'_> {
    /// System prompt text contributed so far by other mods. A mod should
    /// prepend or append to this rather than replacing it outright.
    pub fn contributed_prompt(&self) -> String {
        String::new()
    }
}

/// The result of running a tool. A mod may rewrite this before it is sent
/// back to the model, which makes it possible to redact, truncate, or
/// re-route tool output without patching the tool itself.
pub struct ToolOutput {
    pub body: String,
    /// Set by a mod to report that the output was intentionally altered.
    pub rewritten: bool,
}

impl ToolOutput {
    pub fn new(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            rewritten: false,
        }
    }

    pub fn rewritten(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            rewritten: true,
        }
    }
}

/// Outcome of an interrupt request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Interrupt {
    /// Drop the in-flight request and keep the conversation.
    Stop,
    /// Drop the in-flight request and discard the whole session.
    Quit,
}

/// A keypress captured by the glass TUI.
///
/// The default view ignores most of these. The fullscreen transcript view
/// binds scrolling and collapsing to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Char(char),
    Enter,
    Escape,
    CtrlC,
    CtrlG,
}

/// Where a mod wants the cursor drawn in the fullscreen transcript view.
///
/// `None` hides it. The main loop ignores this entirely.
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    pub lines: Vec<String>,
    pub cursor: Option<(u16, u16)>,
}

/// A loaded mod.
///
/// Every hook has a default implementation, so the smallest useful mod is:
///
/// ```ignore
/// struct MyMod;
/// impl Mod for MyMod {
///     fn system_prompt(&self, _ctx: &TurnContext) -> Option<String> {
///         Some("Always run `cargo test` before reporting success.".into())
///     }
/// }
/// ```
///
/// The object is created once at startup and lives for the whole session.
/// Hooks are called on the main loop thread, so a slow hook stalls the UI.
pub trait Mod_: Send {
    /// Name shown in the status bar. Keep it short.
    fn name(&self) -> &str {
        "mod"
    }

    /// Text appended to the system prompt. Called once per turn, the result
    /// is cached until the next `on_turn_end`.
    fn system_prompt(&self, _ctx: &TurnContext) -> Option<String> {
        None
    }

    /// Observe a completed model turn.
    fn on_turn_end(&self, _ctx: &TurnContext, _output: &str) {}

    /// Observe every tool call after the built-in tool has produced a result
    /// but before the result is sent back to the model.
    fn on_tool_result(&self, _call: &ToolCall, _output: &mut ToolOutput) {}

    /// Rewrite the system prompt that will be sent. Return `None` to leave it
    /// alone. Use [`TurnContext::contributed_prompt`] to respect other mods.
    fn on_prompt(&self, _system: &mut String, _ctx: &TurnContext) {}

    /// Offer extra tools. Each is `(name, description, json_schema)`.
    ///
    /// The core ships `bash` and `state`. A mod may add tools but may not
    /// remove them, so a prompt written against the base surface keeps working.
    fn tools(&self) -> Vec<(String, String, String)> {
        Vec::new()
    }

    /// Decide whether a tool call runs at all. Returning `Err(reason)` blocks
    /// it and the reason is sent to the model as the tool output.
    ///
    /// This is the hook to use for approval prompts. glass itself ships no
    /// permission system, so without a mod providing this every bash call runs.
    fn authorize(&self, _call: &ToolCall) -> Result<(), String> {
        Ok(())
    }

    /// Paint the fullscreen transcript view. Called once per frame.
    fn on_render(&self, _overlay: &mut Overlay, _viewport: (u16, u16)) {}

    /// Observe keypresses in the fullscreen transcript view.
    fn on_key(&self, _key: Key) -> Key {
        _key
    }
}

/// A mod identified by name, loaded from config.
pub struct ModSlot {
    pub name: String,
    pub enabled: bool,
}

impl ModSlot {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: true,
        }
    }
}

/// Static registry used by tests and by the example mod to prove the surface.
pub type ModList = Vec<Box<dyn Mod_>>;

/// Convenience alias so downstream code can name the trait without the
/// trailing underscore.
pub type ModT = dyn Mod_;

/// Metadata a mod may advertise for `--help`-style output.
pub type Manifest = BTreeMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: &str) -> ToolCall {
        ToolCall {
            id: "t1".into(),
            name: "bash".into(),
            arguments: args.to_string(),
        }
    }

    #[test]
    fn reads_plain_string() {
        assert_eq!(
            call(r#"{"command":"ls -la"}"#)
                .arg_str("command")
                .as_deref(),
            Some("ls -la")
        );
    }

    #[test]
    fn reads_array_argument() {
        let v = call(r#"{"items":["a","b"]}"#).arg_raw("items").unwrap();
        assert_eq!(v, r#"["a","b"]"#);
    }

    #[test]
    fn reads_object_argument() {
        let v = call(r#"{"opts":{"deep":{"k":1}}}"#)
            .arg_raw("opts")
            .unwrap();
        assert_eq!(v, r#"{"deep":{"k":1}}"#);
    }

    #[test]
    fn respects_braces_inside_strings() {
        // A shell command full of braces and quotes must not be truncated at
        // the first `}` it happens to contain.
        let v = call(r#"{"command":"find . -name '*.rs' -exec cat {} \;","timeout_ms":5}"#)
            .arg_raw("command")
            .unwrap();
        assert_eq!(v, r#""find . -name '*.rs' -exec cat {} \;""#);
    }

    #[test]
    fn respects_escaped_quotes() {
        let v = call(r#"{"command":"echo \"hi\"","x":1}"#)
            .arg_raw("command")
            .unwrap();
        assert_eq!(v, r#""echo \"hi\"""#);
    }

    #[test]
    fn decodes_escapes_in_string_value() {
        let got = call(r#"{"command":"line1\nline2\ttab"}"#)
            .arg_str("command")
            .unwrap();
        assert_eq!(got, "line1\nline2\ttab");
    }

    #[test]
    fn missing_key_returns_none() {
        assert!(call(r#"{"other":1}"#).arg_str("command").is_none());
    }

    #[test]
    fn wrong_type_returns_none_for_str() {
        assert!(call(r#"{"timeout_ms":5}"#).arg_str("timeout_ms").is_none());
        assert_eq!(
            call(r#"{"timeout_ms":5}"#).arg_raw("timeout_ms").as_deref(),
            Some("5")
        );
    }

    #[test]
    fn last_argument_is_found_without_trailing_comma() {
        assert_eq!(
            call(r#"{"a":1,"command":"go"}"#)
                .arg_str("command")
                .as_deref(),
            Some("go")
        );
    }

    #[test]
    fn empty_arguments_yield_nothing() {
        assert!(call("{}").arg_str("command").is_none());
        assert!(call("").arg_str("command").is_none());
    }

    #[test]
    fn unicode_survives() {
        assert_eq!(
            call(r#"{"command":"echo 你好"}"#)
                .arg_str("command")
                .as_deref(),
            Some("echo 你好")
        );
    }
}

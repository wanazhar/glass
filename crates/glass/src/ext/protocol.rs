//! The wire protocol between glass and an external extension.
//!
//! An extension is any executable that speaks newline-delimited JSON on stdio.
//! It is spawned at startup, announces what it provides, and then receives one
//! JSON object per hook invocation, replying with one object per reply.
//!
//! Why a subprocess and not a native library:
//!
//! - The binary stays small. An extension's dependencies never enter our
//!   address space, which is the entire reason pi ships 40 MiB (QuickJS for
//!   codemode, a TypeScript loader, an image-resize worker, all bundled).
//! - There is no ABI to freeze. Extensions are decoupled from our types, so
//!   we can change the loop internals freely and old extensions keep working.
//! - A crash is contained. A segfaulting extension takes down its own process,
//!   not the session.
//! - It is language-agnostic. Anything that can print JSONL is an extension.
//!
//! The cost is latency: a hook round-trip is a pipe write plus a read, and the
//! extension's own startup happens once per session. For hooks that run per
//! token that would be fatal, so the protocol is deliberately shaped around
//! per-turn and per-tool-call boundaries instead.

use serde_json::{json, Value};

/// Protocol version. An extension whose version we do not know is rejected at
/// handshake rather than misbehaving later.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum bytes we will read from an extension in reply to one request.
///
/// An extension that hangs must not be able to wedge the harness, and one that
/// floods must not be able to exhaust our memory.
pub const MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;

/// Default time we wait for an extension to answer one request.
pub const REQUEST_TIMEOUT_MS: u64 = 5_000;

/// A tool an extension contributes.
#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments, as an object.
    pub schema: Value,
}

/// A slash command an extension contributes.
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub name: String,
    pub description: String,
}

/// What an extension announced during its handshake.
#[derive(Clone, Debug, Default)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub tools: Vec<ToolSpec>,
    pub commands: Vec<CommandSpec>,
    /// Text appended to the system prompt.
    pub system_prompt: String,
}

/// One message sent to an extension.
#[derive(Clone, Debug)]
pub enum Request {
    /// Sent once at startup. The extension replies with a manifest.
    Hello { protocol: u32, workspace: String, model: String },
    /// Ask for the system-prompt contribution. Replies with `{ "text": ... }`.
    SystemPrompt,
    /// A tool finished. May rewrite the body.
    ToolResult { call_id: String, name: String, body: String },
    /// Decide whether a tool call may run. Replies `{ "allow": bool, "reason": ... }`.
    Authorize { call_id: String, name: String, arguments: String },
    /// A turn finished. Fire-and-forget.
    TurnEnd { text: String },
    /// A slash command was invoked. Replies with `{ "output": ... }`.
    Command { name: String, args: String },
    /// Execute a tool the extension contributed. Replies with `{ "output": ... }`.
    RunTool { name: String, arguments: String },
    /// Shutdown notice before the process exits.
    Goodbye,
}

impl Request {
    /// Wire form. `id` is filled in by the caller so replies can be matched.
    pub fn to_json(&self, id: u64) -> Value {
        match self {
            Request::Hello { protocol, workspace, model } => json!({
                "type": "hello", "id": id, "protocol": protocol,
                "workspace": workspace, "model": model
            }),
            Request::SystemPrompt => json!({ "type": "system_prompt", "id": id }),
            Request::ToolResult { call_id, name, body } => json!({
                "type": "tool_result", "id": id,
                "call_id": call_id, "name": name, "body": body
            }),
            Request::Authorize { call_id, name, arguments } => json!({
                "type": "authorize", "id": id,
                "call_id": call_id, "name": name, "arguments": arguments
            }),
            Request::TurnEnd { text } => json!({ "type": "turn_end", "id": id, "text": text }),
            Request::Command { name, args } => json!({ "type": "command", "id": id, "name": name, "args": args }),
            Request::RunTool { name, arguments } => json!({
                "type": "run_tool", "id": id, "name": name, "arguments": arguments
            }),
            Request::Goodbye => json!({ "type": "goodbye", "id": id }),
        }
    }

    /// The hook name, used in log and error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Request::Hello { .. } => "hello",
            Request::SystemPrompt => "system_prompt",
            Request::ToolResult { .. } => "tool_result",
            Request::Authorize { .. } => "authorize",
            Request::TurnEnd { .. } => "turn_end",
            Request::Command { .. } => "command",
            Request::RunTool { .. } => "run_tool",
            Request::Goodbye => "goodbye",
        }
    }
}

/// Parse a handshake reply into a [`Manifest`].
pub fn parse_manifest(v: &Value) -> Result<Manifest, String> {
    let version = v.get("protocol").and_then(Value::as_u64).unwrap_or(0) as u32;
    if version != PROTOCOL_VERSION {
        return Err(format!(
            "extension speaks protocol {version}, this glass speaks {PROTOCOL_VERSION}"
        ));
    }

    let name = v
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("extension did not send a name")?
        .to_string();

    let tools = v
        .get("tools")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let n = t.get("name")?.as_str()?.to_string();
                    Some(ToolSpec {
                        description: t.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
                        schema: t.get("schema").cloned().unwrap_or_else(|| json!({ "type": "object" })),
                        name: n,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let commands = v
        .get("commands")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let n = c.get("name")?.as_str()?.to_string();
                    Some(CommandSpec {
                        name: n,
                        description: c.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Manifest {
        version: v.get("version").and_then(Value::as_str).unwrap_or("0").to_string(),
        system_prompt: v.get("system_prompt").and_then(Value::as_str).unwrap_or("").to_string(),
        name,
        tools,
        commands,
    })
}

/// Render a tool spec into the `(name, description, schema)` triple the
/// provider layer expects.
pub fn tool_triple(t: &ToolSpec) -> (String, String, String) {
    (t.name.clone(), t.description.clone(), t.schema.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> Value {
        json!({
            "protocol": 1,
            "name": "cache-warmer",
            "version": "1.2.3",
            "system_prompt": "be helpful",
            "tools": [
                { "name": "warm", "description": "warm the cache", "schema": { "type": "object" } }
            ],
            "commands": [ { "name": "cache", "description": "show cache stats" } ]
        })
    }

    #[test]
    fn parses_a_full_manifest() {
        let m = parse_manifest(&reply()).unwrap();
        assert_eq!(m.name, "cache-warmer");
        assert_eq!(m.version, "1.2.3");
        assert_eq!(m.system_prompt, "be helpful");
        assert_eq!(m.tools.len(), 1);
        assert_eq!(m.tools[0].name, "warm");
        assert_eq!(m.commands[0].name, "cache");
    }

    #[test]
    fn rejects_a_protocol_mismatch() {
        let mut v = reply();
        v["protocol"] = json!(99);
        let err = parse_manifest(&v).unwrap_err();
        assert!(err.contains("protocol"), "{err}");
    }

    #[test]
    fn rejects_a_missing_name() {
        let v = json!({ "protocol": 1 });
        assert!(parse_manifest(&v).is_err());
    }

    #[test]
    fn accepts_a_minimal_manifest() {
        let m = parse_manifest(&json!({ "protocol": 1, "name": "tiny" })).unwrap();
        assert_eq!(m.name, "tiny");
        assert!(m.tools.is_empty());
        assert!(m.commands.is_empty());
        assert!(m.system_prompt.is_empty());
    }

    #[test]
    fn skips_malformed_tool_entries() {
        // One bad entry must not lose the good ones.
        let v = json!({
            "protocol": 1, "name": "x",
            "tools": [ { "nope": 1 }, { "name": "good", "description": "d" } ]
        });
        let m = parse_manifest(&v).unwrap();
        assert_eq!(m.tools.len(), 1);
        assert_eq!(m.tools[0].name, "good");
    }

    #[test]
    fn requests_serialize_with_an_id() {
        let r = Request::ToolResult { call_id: "c1".into(), name: "bash".into(), body: "out".into() };
        let j = r.to_json(7);
        assert_eq!(j["type"], "tool_result");
        assert_eq!(j["id"], 7);
        assert_eq!(j["body"], "out");
    }

    #[test]
    fn hello_carries_workspace_and_model() {
        let j = Request::Hello { protocol: PROTOCOL_VERSION, workspace: "/tmp".into(), model: "gpt-5".into() }
            .to_json(1);
        assert_eq!(j["workspace"], "/tmp");
        assert_eq!(j["model"], "gpt-5");
    }

    #[test]
    fn run_tool_serializes_name_and_arguments() {
        let j = Request::RunTool { name: "warm".into(), arguments: "{}".into() }.to_json(3);
        assert_eq!(j["type"], "run_tool");
        assert_eq!(j["name"], "warm");
        assert_eq!(j["id"], 3);
    }

    #[test]
    fn tool_triple_renders_the_schema_as_a_string() {
        let t = ToolSpec {
            name: "warm".into(),
            description: "d".into(),
            schema: json!({ "type": "object" }),
        };
        let (n, d, s) = tool_triple(&t);
        assert_eq!(n, "warm");
        assert_eq!(d, "d");
        assert!(s.contains("object"));
    }
}
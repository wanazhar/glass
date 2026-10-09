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

use serde_json::{Value, json};

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

/// How an extension's tool may be scheduled alongside others.
///
/// This is the same three-way choice the built-in tools get, exposed so an
/// extension author does not have to guess. A tool that writes must say so, or
/// it will be serialised and quietly cost wall-clock time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExtConcurrency {
    /// Safe to run alongside anything.
    #[default]
    Parallel,
    /// Serialized against other calls touching the same key.
    Keyed,
    /// Runs alone, after everything queued before it.
    Serial,
}

impl ExtConcurrency {
    fn parse(s: &str) -> Self {
        match s {
            "keyed" | "Keyed" => ExtConcurrency::Keyed,
            "serial" | "Serial" => ExtConcurrency::Serial,
            _ => ExtConcurrency::Parallel,
        }
    }
}

/// A tool an extension contributes.
#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments, as an object.
    pub schema: Value,
    /// Scheduling class. Defaults to `Parallel`, which is wrong for a tool that
    /// writes — so a tool that mutates state should declare `serial` or
    /// `keyed`.
    pub concurrency: ExtConcurrency,
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
    /// Require a valid allow decision; opt-in for legacy compatibility.
    pub authorization_required: bool,
    pub name: String,
    pub version: String,
    pub tools: Vec<ToolSpec>,
    pub commands: Vec<CommandSpec>,
    /// Text appended to the system prompt.
    pub system_prompt: String,
    /// Cache-warming policy, if this extension declares one.
    ///
    /// The extension owns the *policy* (how long to wait, how big
    /// the prefix must be); the host owns the *mechanism*, because
    /// only the host holds the credentials and only the host is
    /// still running while the harness sits idle. A subprocess
    /// cannot warm the cache itself: it has no API key, and the
    /// host is not reading its stdout between turns.
    pub cache_warmer: Option<WarmerPolicy>,
}

/// Policy for refreshing the prompt cache across idle gaps.
///
/// The provider expires the cache `ttl_s` after the last write.
/// Refreshing at `refresh_after_s` — just before expiry — keeps the
/// next turn a cache hit instead of a full re-read. That trades a
/// cache *write* (1.25x the prefix) for a faster, cheaper turn
/// (0.1x instead of 1.0x), which is a net spend, not a saving; it
/// is worth it only when the latency of a cold prefix matters.
#[derive(Clone, Copy, Debug)]
pub struct WarmerPolicy {
    /// Seconds after a turn to refresh the cache.
    pub refresh_after_s: u64,
    /// Smallest prefix worth warming, in bytes. Below this the
    /// latency win is noise.
    pub min_prefix_bytes: usize,
}

impl WarmerPolicy {
    /// Five-minute TTL, refreshed at four, for prefixes above
    /// roughly a thousand tokens.
    pub const DEFAULT: Self = Self {
        refresh_after_s: 240,
        min_prefix_bytes: 4000,
    };
}

/// One message sent to an extension.
#[derive(Clone, Debug)]
pub enum Request {
    /// Sent once at startup. The extension replies with a manifest.
    Hello {
        protocol: u32,
        workspace: String,
        model: String,
    },
    /// Ask for the system-prompt contribution. Replies with `{ "text": ... }`.
    SystemPrompt,
    /// A tool finished. May rewrite the body.
    ToolResult {
        call_id: String,
        name: String,
        body: String,
    },
    /// Decide whether a tool call may run. Replies `{ "allow": bool, "reason": ... }`.
    Authorize {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// A turn finished. Replies are ignored.
    ///
    /// `cacheable_prefix_bytes` tells the extension how large the stable
    /// prompt prefix is, which is what a cache warmer needs to decide whether
    /// a refresh write is worth its cost.
    TurnEnd {
        text: String,
        cacheable_prefix_bytes: usize,
    },
    /// A slash command was invoked. Replies with `{ "output": ... }`.
    Command { name: String, args: String },
    /// Execute a tool the extension contributed. Replies with `{ "output": ... }`.
    RunTool { name: String, arguments: String },
    /// Execute a tool with the scoped interactive input callback enabled.
    RunToolInput { name: String, arguments: String },
    /// Shutdown notice before the process exits.
    Goodbye,
    /// Seed an extension with its existing durable entries, sent once
    /// after the session opens. The extension acks; the entries are
    /// `(kind, data)` pairs it appended in earlier sessions.
    SeedCustom {
        entries: Vec<(String, Vec<String>)>,
    },
}

impl Request {
    /// Wire form. `id` is filled in by the caller so replies can be matched.
    pub fn to_json(&self, id: u64) -> Value {
        match self {
            Request::Hello {
                protocol,
                workspace,
                model,
            } => json!({
                "type": "hello", "id": id, "protocol": protocol,
                "workspace": workspace, "model": model
            }),
            Request::SystemPrompt => json!({ "type": "system_prompt", "id": id }),
            Request::ToolResult {
                call_id,
                name,
                body,
            } => json!({
                "type": "tool_result", "id": id,
                "call_id": call_id, "name": name, "body": body
            }),
            Request::Authorize {
                call_id,
                name,
                arguments,
            } => json!({
                "type": "authorize", "id": id,
                "call_id": call_id, "name": name, "arguments": arguments
            }),
            Request::TurnEnd {
                text,
                cacheable_prefix_bytes,
            } => json!({
                "type": "turn_end", "id": id, "text": text,
                "cacheable_prefix_bytes": cacheable_prefix_bytes
            }),
            Request::Command { name, args } => {
                json!({ "type": "command", "id": id, "name": name, "args": args })
            }
            Request::RunTool { name, arguments } => json!({
                "type": "run_tool", "id": id, "name": name, "arguments": arguments
            }),
            Request::RunToolInput { name, arguments } => json!({
                "type": "run_tool", "id": id, "name": name, "arguments": arguments,
                "interactive_input": true
            }),
            Request::Goodbye => json!({ "type": "goodbye", "id": id }),
            Request::SeedCustom { entries } => {
                let entries: Vec<Value> = entries
                    .iter()
                    .map(|(kind, data)| {
                        json!({ "kind": kind, "entries": data })
                    })
                    .collect();
                json!({ "type": "seed_custom", "id": id, "entries": entries })
            }
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
                        description: t
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        schema: t
                            .get("schema")
                            .cloned()
                            .unwrap_or_else(|| json!({ "type": "object" })),
                        concurrency: t
                            .get("concurrency")
                            .and_then(Value::as_str)
                            .map(ExtConcurrency::parse)
                            .unwrap_or_default(),
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
                        description: c
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // The warmer policy is optional and deliberately conservative:
    // an extension must opt in, and both knobs fall back to sane
    // defaults rather than trusting a hostile value.
    let cache_warmer = v.get("cache_warmer").and_then(Value::as_object).map(|o| {
        let refresh_after_s = o
            .get("refresh_after_s")
            .and_then(Value::as_u64)
            .filter(|s| *s > 0 && *s <= 3600)
            .unwrap_or(WarmerPolicy::DEFAULT.refresh_after_s);
        let min_prefix_bytes = o
            .get("min_prefix_bytes")
            .and_then(Value::as_u64)
            .filter(|b| *b > 0 && *b <= 10_000_000)
            .map(|b| b as usize)
            .unwrap_or(WarmerPolicy::DEFAULT.min_prefix_bytes);
        WarmerPolicy {
            refresh_after_s,
            min_prefix_bytes,
        }
    });

    Ok(Manifest {
        authorization_required: match v.get("authorization_required") {
            None => false,
            Some(Value::Bool(required)) => *required,
            Some(_) => return Err("authorization_required must be a boolean".into()),
        },
        version: v
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("0")
            .to_string(),
        system_prompt: v
            .get("system_prompt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name,
        tools,
        commands,
        cache_warmer,
    })
}

/// Wire form of the host's answer to a nested `call_tool` request.
pub fn nested_call_result_json(call_id: &Value, body: &str) -> Value {
    json!({ "type": "call_tool_result", "call_id": call_id, "body": body })
}

/// True when `v` is a nested `call_tool` request rather than a reply.
///
/// While a `run_tool` is executing, the extension may ask the host to run a
/// tool on its behalf. The reply to that arrives before the reply to the
/// original request, so the host has to demultiplex on the message type
/// instead of assuming one reply per request.
pub fn is_nested_call(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("call_tool")
}

/// A typed request for interactive user input, separate from tool dispatch.
pub fn is_input_request(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("request_input")
}

pub fn input_result_json(call_id: &Value, response: &Value) -> Value {
    json!({ "type": "request_input_result", "call_id": call_id, "response": response })
}

/// Wire form of the host's answer to a nested `run_subagent` request.
pub fn subagent_result_json(call_id: &Value, output: &str) -> Value {
    json!({ "type": "subagent_result", "call_id": call_id, "output": output })
}

/// True when `v` is a nested `run_subagent` request.
///
/// A subagent is a delegated task the host runs in a fresh, isolated
/// context. Like a nested tool call it arrives while the extension's own
/// request is still in flight, so it is demultiplexed the same way.
pub fn is_subagent_call(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("run_subagent")
}

/// One delegated task in a batched `run_subagents` request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubagentTask {
    pub task: String,
    /// Replaces the default subagent prompt, e.g. an agent definition body.
    pub prompt: Option<String>,
    /// Model id for this task; resolved against the active connection.
    pub model: Option<String>,
    /// Run the task in a detached git worktree instead of the workspace.
    pub worktree: bool,
    /// Run with the read-only inspection tools only — no shell, no writes.
    /// Second-opinion flows (an oracle reviewing a diff) use this so a
    /// critique can never mutate the tree it is judging.
    pub read_only: bool,
}

/// Parse the `tasks` array of a batched subagent request.
///
/// The batch is validated as a whole: one malformed entry rejects the
/// request rather than silently dropping a task the model believes it
/// delegated.
pub fn parse_subagent_tasks(v: &Value) -> Result<Vec<SubagentTask>, String> {
    let tasks = v
        .get("tasks")
        .and_then(Value::as_array)
        .ok_or("run_subagents requires a tasks array")?;
    if tasks.is_empty() {
        return Err("run_subagents requires at least one task".into());
    }
    let mut out = Vec::with_capacity(tasks.len());
    for t in tasks {
        let task = t
            .get("task")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if task.is_empty() {
            return Err("every delegated task needs a non-empty task".into());
        }
        out.push(SubagentTask {
            task,
            prompt: t
                .get("prompt")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|s| !s.trim().is_empty()),
            model: t
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|s| !s.trim().is_empty()),
            worktree: t.get("worktree").and_then(Value::as_bool).unwrap_or(false),
            read_only: t.get("read_only").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    Ok(out)
}

/// Wire form of the host's answer to a batched `run_subagents` request.
///
/// `outputs` holds one report per task, in request order. `error` is set
/// when the batch itself was rejected before any task ran.
pub fn subagents_result_json(call_id: &Value, outputs: &[String], error: Option<&str>) -> Value {
    json!({
        "type": "subagents_result", "call_id": call_id,
        "outputs": outputs, "error": error
    })
}

/// Frame one delegated report so its content can never read as
/// session instructions.
///
/// A subagent report is text generated in a context the user never
/// typed into. Without an explicit envelope it is indistinguishable
/// from conversation content, so report prose could be treated as
/// instructions the user gave. The envelope is the hand-back
/// boundary Claude Code frames subagent results with.
pub fn envelope_report(output: &str) -> String {
    format!("<subagent-report>\n{output}\n</subagent-report>")
}

/// True when `v` is an extension asking to append a durable entry.
///
/// Like a nested tool call it arrives while another request is in
/// flight, so the host demultiplexes on the message type.
pub fn is_custom_append(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("append_custom")
}

/// True when `v` is an extension asking to read its durable entries.
pub fn is_custom_read(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("read_custom")
}

/// Wire form of the host's answer to `append_custom` and `read_custom`.
///
/// `entries` is set for reads; `ok`/`error` for appends.
pub fn custom_result_json(
    call_id: &Value,
    ok: bool,
    entries: &[String],
    error: Option<&str>,
) -> Value {
    json!({
        "type": "custom_result", "call_id": call_id,
        "ok": ok, "entries": entries, "error": error
    })
}

/// True when `v` is a batched `run_subagents` request.
///
/// The single-task `run_subagent` form is legacy; a batch request is how
/// fan-out reaches the host, because one extension is one serial pipe.
pub fn is_subagents_call(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("run_subagents")
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
    fn no_cache_warmer_by_default() {
        // Warming is a net spend, so it must be opted into.
        assert!(parse_manifest(&reply()).unwrap().cache_warmer.is_none());
    }

    #[test]
    fn cache_warmer_policy_is_parsed() {
        let mut r = reply();
        r["cache_warmer"] = json!({ "refresh_after_s": 240, "min_prefix_bytes": 4000 });
        let m = parse_manifest(&r).unwrap();
        let p = m.cache_warmer.expect("policy declared");
        assert_eq!(p.refresh_after_s, 240);
        assert_eq!(p.min_prefix_bytes, 4000);
    }

    #[test]
    fn cache_warmer_falls_back_to_defaults() {
        // An empty object must not disable warming, and a
        // hostile value must not make the host sleep forever.
        let mut r = reply();
        r["cache_warmer"] = json!({});
        let p = parse_manifest(&r).unwrap().cache_warmer.unwrap();
        assert_eq!(p.refresh_after_s, WarmerPolicy::DEFAULT.refresh_after_s);
        assert_eq!(p.min_prefix_bytes, WarmerPolicy::DEFAULT.min_prefix_bytes);

        r["cache_warmer"] = json!({ "refresh_after_s": 999999, "min_prefix_bytes": 0 });
        let p = parse_manifest(&r).unwrap().cache_warmer.unwrap();
        assert_eq!(p.refresh_after_s, WarmerPolicy::DEFAULT.refresh_after_s);
        assert_eq!(p.min_prefix_bytes, WarmerPolicy::DEFAULT.min_prefix_bytes);
    }

    #[test]
    fn concurrency_defaults_to_parallel() {
        let m = parse_manifest(&reply()).unwrap();
        assert_eq!(m.tools[0].concurrency, ExtConcurrency::Parallel);
    }

    #[test]
    fn concurrency_is_read_from_the_manifest() {
        let mut v = reply();
        v["tools"][0]["concurrency"] = json!("serial");
        assert_eq!(
            parse_manifest(&v).unwrap().tools[0].concurrency,
            ExtConcurrency::Serial
        );

        v["tools"][0]["concurrency"] = json!("keyed");
        assert_eq!(
            parse_manifest(&v).unwrap().tools[0].concurrency,
            ExtConcurrency::Keyed
        );
    }

    #[test]
    fn unknown_concurrency_falls_back_to_parallel() {
        let mut v = reply();
        v["tools"][0]["concurrency"] = json!("nonsense");
        assert_eq!(
            parse_manifest(&v).unwrap().tools[0].concurrency,
            ExtConcurrency::Parallel
        );
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
        let r = Request::ToolResult {
            call_id: "c1".into(),
            name: "bash".into(),
            body: "out".into(),
        };
        let j = r.to_json(7);
        assert_eq!(j["type"], "tool_result");
        assert_eq!(j["id"], 7);
        assert_eq!(j["body"], "out");
    }

    #[test]
    fn hello_carries_workspace_and_model() {
        let j = Request::Hello {
            protocol: PROTOCOL_VERSION,
            workspace: "/tmp".into(),
            model: "gpt-5".into(),
        }
        .to_json(1);
        assert_eq!(j["workspace"], "/tmp");
        assert_eq!(j["model"], "gpt-5");
    }

    #[test]
    fn turn_end_reports_the_prefix_size() {
        let j = Request::TurnEnd {
            text: "x".into(),
            cacheable_prefix_bytes: 4200,
        }
        .to_json(2);
        assert_eq!(j["cacheable_prefix_bytes"], 4200);
    }

    #[test]
    fn nested_call_is_distinguishable_from_a_reply() {
        let j = json!({ "type": "call_tool", "call_id": "c9", "name": "bash" });
        assert!(is_nested_call(&j));
        // A plain reply, and the host's answer to a nested call, must both be
        // treated as ordinary replies rather than triggering a nested dispatch.
        assert!(!is_nested_call(&json!({ "id": 4, "output": "x" })));
        assert!(!is_nested_call(&nested_call_result_json(
            &json!("c9"),
            "out"
        )));
    }

    #[test]
    fn subagent_batch_is_distinguishable_from_a_single_task() {
        let batch = json!({ "type": "run_subagents", "tasks": [{ "task": "a" }] });
        assert!(is_subagents_call(&batch));
        assert!(!is_subagent_call(&batch), "a batch is not the legacy form");
        assert!(!is_subagents_call(&json!({ "type": "run_subagent", "task": "a" })));
        assert!(!is_subagents_call(&subagents_result_json(&json!("c1"), &[], None)));
    }

    #[test]
    fn batch_tasks_parse_with_defaults() {
        let v = json!({ "tasks": [{ "task": "  do the thing  " }, { "task": "other" }] });
        let tasks = parse_subagent_tasks(&v).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].task, "do the thing", "task text is trimmed");
        assert_eq!(tasks[0].prompt, None);
        assert_eq!(tasks[0].model, None);
        assert!(!tasks[0].worktree);
    }

    #[test]
    fn batch_tasks_carry_agent_and_worktree_fields() {
        let v = json!({
            "tasks": [{
                "task": "review auth",
                "prompt": "You review security.",
                "model": "claude-opus-4-5",
                "worktree": true
            }]
        });
        let t = &parse_subagent_tasks(&v).unwrap()[0];
        assert_eq!(t.prompt.as_deref(), Some("You review security."));
        assert_eq!(t.model.as_deref(), Some("claude-opus-4-5"));
        assert!(t.worktree);
    }

    #[test]
    fn a_malformed_batch_is_rejected_whole() {
        // One bad entry must not silently drop a delegated task.
        let missing = json!({ "tasks": [{ "task": "ok" }, { "nope": 1 }] });
        assert!(parse_subagent_tasks(&missing).is_err());
        let empty = json!({ "tasks": [] });
        assert!(parse_subagent_tasks(&empty).is_err());
        let no_array = json!({ "task": "legacy shape" });
        assert!(parse_subagent_tasks(&no_array).is_err());
    }

    #[test]
    fn a_task_can_request_read_only_execution() {
        let v = json!({ "tasks": [{ "task": "review the diff", "read_only": true }] });
        assert!(parse_subagent_tasks(&v).unwrap()[0].read_only);
        let v = json!({ "tasks": [{ "task": "fix the bug" }] });
        assert!(!parse_subagent_tasks(&v).unwrap()[0].read_only);
    }

    #[test]
    fn batch_result_reports_an_error_field() {
        let outputs = vec!["a done".to_string(), "b failed".to_string()];
        let j = subagents_result_json(&json!("c1"), &outputs, Some("nope"));
        assert_eq!(j["type"], "subagents_result");
        assert_eq!(j["outputs"][1], "b failed");
        assert_eq!(j["error"], "nope");
        let ok = subagents_result_json(&json!(null), &outputs, None);
        assert!(ok["error"].is_null());
    }

    #[test]
    fn reports_are_framed_so_they_cannot_read_as_instructions() {
        let framed = envelope_report("do the thing the user asked");
        assert!(
            framed.starts_with("<subagent-report>\n"),
            "the frame opens the report: {framed}"
        );
        assert!(
            framed.ends_with("\n</subagent-report>"),
            "the frame closes the report: {framed}"
        );
        assert!(framed.contains("do the thing the user asked"));
    }

    #[test]
    fn custom_calls_are_distinguishable_from_replies() {
        let append = json!({
            "type": "append_custom", "call_id": "c1",
            "kind": "ext/state", "data": "v1"
        });
        assert!(is_custom_append(&append));
        assert!(!is_custom_read(&append));
        let read = json!({ "type": "read_custom", "call_id": "c2", "kind": "ext/state" });
        assert!(is_custom_read(&read));
        assert!(!is_custom_append(&read));
        // A plain reply must not be demultiplexed as either.
        assert!(!is_custom_append(&json!({ "id": 3, "ok": true })));
        let j = custom_result_json(&json!("c1"), true, &["v1".to_string()], None);
        assert_eq!(j["type"], "custom_result");
        assert_eq!(j["entries"][0], "v1");
        assert!(j["error"].is_null());
    }

    #[test]
    fn seed_custom_serializes_kinds_and_entries() {
        let j = Request::SeedCustom {
            entries: vec![("ext/state".into(), vec!["v1".into(), "v2".into()])],
        }
        .to_json(9);
        assert_eq!(j["type"], "seed_custom");
        assert_eq!(j["entries"][0]["kind"], "ext/state");
        assert_eq!(j["entries"][0]["entries"][1], "v2");
    }

    #[test]
    fn run_tool_serializes_name_and_arguments() {
        let j = Request::RunTool {
            name: "warm".into(),
            arguments: "{}".into(),
        }
        .to_json(3);
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
            concurrency: ExtConcurrency::default(),
        };
        let (n, d, s) = tool_triple(&t);
        assert_eq!(n, "warm");
        assert_eq!(d, "d");
        assert!(s.contains("object"));
    }
}

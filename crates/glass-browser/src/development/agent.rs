use super::{Actor, DevelopmentError, DevelopmentEventKind, DevelopmentResult, ProjectWorkspace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub mutating: bool,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentContextPacket {
    pub schema_version: String,
    pub references: Vec<String>,
    pub project: Value,
    pub diagnostics: Value,
    pub processes: Value,
    pub recent_events: Value,
    pub resolved: Value,
}

pub fn resolve_context(
    workspace: &mut ProjectWorkspace,
    prompt: &str,
) -> DevelopmentResult<AgentContextPacket> {
    if prompt.len() > 8 * 1024 {
        return Err(DevelopmentError::InvalidInput(
            "prompt exceeds the 8192 byte context limit".into(),
        ));
    }
    let mut references = prompt
        .split_whitespace()
        .filter(|token| token.starts_with('@'))
        .map(|token| {
            token
                .trim_matches(|character: char| {
                    matches!(character, ',' | '.' | '?' | '!' | ')' | ']')
                })
                .to_string()
        })
        .collect::<Vec<_>>();
    references.sort();
    references.dedup();
    if references.len() > 32 {
        return Err(DevelopmentError::InvalidInput(
            "agent context cannot contain more than 32 references".into(),
        ));
    }
    let mut resolved = serde_json::Map::new();
    for reference in &references {
        let value = match reference.as_str() {
            "@workspace" => serde_json::to_value(workspace.detection())?,
            "@diagnostic" => serde_json::to_value(workspace.diagnostics())?,
            "@run:last" => workspace
                .timeline()
                .events()
                .next_back()
                .map(serde_json::to_value)
                .transpose()?
                .unwrap_or(Value::Null),
            "@page" | "@browser" => serde_json::json!({
                "status": "requires-attached-browser-workspace",
                "defaultObservation": "structured"
            }),
            value if value.starts_with("@entity:") => {
                serde_json::to_value(workspace.graph().links_for(&value[8..]))?
            }
            value if value.starts_with("@file:") => {
                let path = &value[6..];
                let content = workspace.read_file(path)?;
                serde_json::json!({"path": path, "bytes": content.len(), "sha256": prompt_evidence(&content)["sha256"]})
            }
            value if value.starts_with("@revision:") => {
                let index = value[10..].parse::<usize>().map_err(|_| {
                    DevelopmentError::InvalidInput(format!("invalid revision reference: {value}"))
                })?;
                serde_json::to_value(workspace.replay(index, 1)?)?
            }
            "@selection" | "@file" | "@symbol" | "@workflow" | "@memory" => {
                serde_json::json!({"status": "not-selected"})
            }
            value => {
                return Err(DevelopmentError::InvalidInput(format!(
                    "unsupported agent context reference: {value}"
                )));
            }
        };
        resolved.insert(reference.clone(), value);
    }
    let processes = workspace.processes().list();
    let events = workspace
        .timeline()
        .events()
        .rev()
        .take(32)
        .collect::<Vec<_>>();
    Ok(AgentContextPacket {
        schema_version: "glass.agent-context.v1".into(),
        references,
        project: serde_json::to_value(workspace.detection())?,
        diagnostics: serde_json::to_value(workspace.diagnostics())?,
        processes: serde_json::to_value(processes)?,
        recent_events: serde_json::to_value(events)?,
        resolved: Value::Object(resolved),
    })
}

#[derive(Debug, Clone, Default)]
pub struct ToolRegistry;

impl ToolRegistry {
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        let schema = |properties: Value, required: &[&str]| serde_json::json!({"type":"object", "properties": properties, "required": required, "additionalProperties": false});
        vec![
            descriptor(
                "glass.file.read",
                "Read one bounded project file",
                schema(serde_json::json!({"path":{"type":"string"}}), &["path"]),
                false,
            ),
            descriptor(
                "glass.file.search",
                "Search files, entities, processes, events, and commands",
                schema(serde_json::json!({"query":{"type":"string"}}), &["query"]),
                false,
            ),
            descriptor(
                "glass.file.patch",
                "Replace bounded text through an attributed editor buffer",
                schema(
                    serde_json::json!({"path":{"type":"string"},"search":{"type":"string"},"replace":{"type":"string"}}),
                    &["path", "search", "replace"],
                ),
                true,
            ),
            descriptor(
                "glass.process.start",
                "Start a named PTY process",
                schema(
                    serde_json::json!({"name":{"type":"string"},"command":{"type":"string"}}),
                    &["name", "command"],
                ),
                true,
            ),
            descriptor(
                "glass.process.stop",
                "Stop a named managed process",
                schema(serde_json::json!({"name":{"type":"string"}}), &["name"]),
                true,
            ),
            descriptor(
                "glass.process.logs",
                "Read a bounded managed-process output tail",
                schema(serde_json::json!({"name":{"type":"string"}}), &["name"]),
                false,
            ),
            descriptor(
                "glass.git.status",
                "Inspect code and runtime impact",
                schema(serde_json::json!({}), &[]),
                false,
            ),
            descriptor(
                "glass.semantic.inspect",
                "Inspect source/runtime graph links",
                schema(serde_json::json!({"entity":{"type":"string"}}), &["entity"]),
                false,
            ),
            descriptor(
                "glass.test.run",
                "Run a command as attributed verification",
                schema(
                    serde_json::json!({"name":{"type":"string"},"command":{"type":"string"}}),
                    &["name", "command"],
                ),
                true,
            ),
            descriptor(
                "glass.runtime.inspect",
                "Inspect project, processes, actors, and diagnostics",
                schema(serde_json::json!({}), &[]),
                false,
            ),
            unavailable_descriptor(
                "glass.browser.observe",
                "Requires an attached Browser Workspace; structured observation remains the default",
            ),
            unavailable_descriptor(
                "glass.browser.navigate",
                "Requires an attached Browser Workspace and navigation policy",
            ),
            unavailable_descriptor(
                "glass.browser.act",
                "Requires an attached Browser Workspace and mutation lease",
            ),
            unavailable_descriptor(
                "glass.workflow.pause",
                "Requires an active browser workflow run",
            ),
            unavailable_descriptor(
                "glass.workflow.resume",
                "Requires an active browser workflow checkpoint",
            ),
            unavailable_descriptor(
                "glass.memory.retrieve",
                "Requires an attached profile-scoped semantic memory store",
            ),
        ]
    }

    pub fn execute(
        &self,
        workspace: &mut ProjectWorkspace,
        call: &ToolCall,
        actor: Actor,
    ) -> DevelopmentResult<Value> {
        let string = |name: &str| {
            call.arguments
                .get(name)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    DevelopmentError::InvalidInput(format!(
                        "{} requires string argument {name}",
                        call.name
                    ))
                })
        };
        match call.name.as_str() {
            "glass.file.read" => {
                Ok(serde_json::json!({"content": workspace.read_file(string("path")?)?}))
            }
            "glass.file.search" => Ok(serde_json::to_value(
                workspace.search(string("query")?, 64)?,
            )?),
            "glass.file.patch" => {
                let count = workspace.replace_in_buffer(
                    string("path")?,
                    string("search")?,
                    string("replace")?,
                    actor,
                )?;
                if count == 0 {
                    return Err(DevelopmentError::Conflict(
                        "patch search text was not found".into(),
                    ));
                }
                let path = string("path")?.to_string();
                let buffer = workspace.save_buffer(&path)?;
                Ok(serde_json::json!({"path": path, "replacements": count, "dirty": buffer.dirty}))
            }
            "glass.process.start" => Ok(serde_json::to_value(
                workspace.start_process(string("name")?, string("command")?)?,
            )?),
            "glass.process.stop" => Ok(serde_json::to_value(
                workspace.stop_process(string("name")?)?,
            )?),
            "glass.process.logs" => {
                Ok(serde_json::json!({"output": workspace.processes().output(string("name")?)?}))
            }
            "glass.git.status" => Ok(serde_json::to_value(workspace.diff()?)?),
            "glass.semantic.inspect" => Ok(serde_json::to_value(
                workspace.graph().links_for(string("entity")?),
            )?),
            "glass.test.run" => Ok(serde_json::to_value(workspace.run_verification(
                string("name")?,
                string("command")?,
                Duration::from_secs(120),
            )?)?),
            "glass.runtime.inspect" => Ok(serde_json::json!({
                "project": workspace.detection(),
                "processes": workspace.processes().list(),
                "actors": workspace.actors().collect::<Vec<_>>(),
                "diagnostics": workspace.diagnostics(),
                "revision": workspace.revision()
            })),
            name if self
                .descriptors()
                .iter()
                .any(|descriptor| descriptor.name == name && !descriptor.available) =>
            {
                Err(DevelopmentError::InvalidInput(format!(
                    "tool {name} is unavailable without its required runtime attachment"
                )))
            }
            _ => Err(DevelopmentError::NotFound(format!("tool {}", call.name))),
        }
    }
}

fn descriptor(
    name: &str,
    description: &str,
    input_schema: Value,
    mutating: bool,
) -> ToolDescriptor {
    ToolDescriptor {
        name: name.into(),
        description: description.into(),
        input_schema,
        mutating,
        available: true,
        unavailable_reason: None,
    }
}

fn unavailable_descriptor(name: &str, reason: &str) -> ToolDescriptor {
    ToolDescriptor {
        name: name.into(),
        description: reason.into(),
        input_schema: serde_json::json!({"type":"object"}),
        mutating: false,
        available: false,
        unavailable_reason: Some(reason.into()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HarnessRequest {
    Hello,
    Prompt { text: String },
    Steer { text: String },
    FollowUp { text: String },
    Abort,
    State,
    Models,
    SetModel { provider: String, model_id: String },
    SetThinking { level: String },
    NewSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HarnessEvent {
    Hello {
        protocol: String,
        actor: Actor,
        capabilities: Vec<String>,
    },
    State {
        state: String,
    },
    Text {
        text: String,
    },
    ToolCall(ToolCall),
    ToolResult {
        id: String,
        result: Value,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub struct LocalHarness {
    actor: Actor,
    next_tool_id: u64,
    state: String,
}

impl Default for LocalHarness {
    fn default() -> Self {
        Self {
            actor: Actor::embedded(),
            next_tool_id: 1,
            state: "idle".into(),
        }
    }
}

impl LocalHarness {
    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    pub fn handle(
        &mut self,
        workspace: &mut ProjectWorkspace,
        request: HarnessRequest,
    ) -> DevelopmentResult<Vec<HarnessEvent>> {
        match request {
            HarnessRequest::Hello => Ok(vec![HarnessEvent::Hello {
                protocol: "glass.harness.v1".into(),
                actor: self.actor.clone(),
                capabilities: vec![
                    "prompt".into(),
                    "stream".into(),
                    "tool.call".into(),
                    "steer".into(),
                    "follow_up".into(),
                    "abort".into(),
                    "session.new".into(),
                    "model.list".into(),
                ],
            }]),
            HarnessRequest::Prompt { text } => self.prompt(workspace, text),
            HarnessRequest::Steer { text } => {
                let evidence = prompt_evidence(&text);
                workspace.record_as(
                    self.actor.clone(),
                    DevelopmentEventKind::AgentSteered,
                    evidence,
                )?;
                self.state = "steered".into();
                Ok(vec![
                    HarnessEvent::State {
                        state: self.state.clone(),
                    },
                    HarnessEvent::Text {
                        text: "Steering accepted by the Glass harness.".into(),
                    },
                ])
            }
            HarnessRequest::Abort => {
                self.state = "aborted".into();
                Ok(vec![HarnessEvent::State {
                    state: self.state.clone(),
                }])
            }
            HarnessRequest::FollowUp { text } => self.prompt(workspace, text),
            HarnessRequest::State => Ok(vec![HarnessEvent::State {
                state: self.state.clone(),
            }]),
            HarnessRequest::Models => Ok(vec![HarnessEvent::Text {
                text: "The local harness has no model provider; configure the Pi adapter.".into(),
            }]),
            HarnessRequest::SetModel { .. } | HarnessRequest::SetThinking { .. } => {
                Ok(vec![HarnessEvent::Error {
                    message: "model controls require the Pi harness adapter".into(),
                }])
            }
            HarnessRequest::NewSession => {
                self.state = "idle".into();
                Ok(vec![HarnessEvent::State {
                    state: self.state.clone(),
                }])
            }
        }
    }

    fn prompt(
        &mut self,
        workspace: &mut ProjectWorkspace,
        text: String,
    ) -> DevelopmentResult<Vec<HarnessEvent>> {
        if text.trim().is_empty() || text.len() > 8 * 1024 {
            return Ok(vec![HarnessEvent::Error {
                message: "prompt must be non-empty and at most 8192 bytes".into(),
            }]);
        }
        workspace.record_as(
            self.actor.clone(),
            DevelopmentEventKind::AgentPrompt,
            prompt_evidence(&text),
        )?;
        self.state = "working".into();
        let mut events = vec![HarnessEvent::State {
            state: self.state.clone(),
        }];
        let lower = text.to_ascii_lowercase();
        if text.split_whitespace().any(|token| token.starts_with('@')) {
            let packet = resolve_context(workspace, &text)?;
            events.push(HarnessEvent::Text {
                text: serde_json::to_string(&packet)?,
            });
        } else if let Some(path) = text
            .strip_prefix("read ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let call = self.tool_call("glass.file.read", serde_json::json!({"path": path}));
            let id = call.id.clone();
            workspace.record_as(
                self.actor.clone(),
                DevelopmentEventKind::AgentToolCalled,
                serde_json::to_value(&call)?,
            )?;
            events.push(HarnessEvent::ToolCall(call));
            match workspace.read_file(path) {
                Ok(content) => {
                    let result = serde_json::json!({"path": path, "content": content});
                    workspace.record_as(
                        self.actor.clone(),
                        DevelopmentEventKind::AgentToolResult,
                        serde_json::json!({"id": id, "ok": true}),
                    )?;
                    events.push(HarnessEvent::ToolResult { id, result });
                }
                Err(error) => events.push(HarnessEvent::Error {
                    message: error.to_string(),
                }),
            }
        } else if lower == "files" || lower == "list files" {
            let call = self.tool_call("glass.file.list", serde_json::json!({}));
            let id = call.id.clone();
            events.push(HarnessEvent::ToolCall(call));
            let result = serde_json::to_value(workspace.list_files()?)?;
            events.push(HarnessEvent::ToolResult { id, result });
        } else if lower == "process list" || lower == "processes" {
            let call = self.tool_call("glass.process.list", serde_json::json!({}));
            let id = call.id.clone();
            events.push(HarnessEvent::ToolCall(call));
            let result = serde_json::to_value(workspace.processes().list())?;
            events.push(HarnessEvent::ToolResult { id, result });
        } else if lower == "diff" {
            let call = self.tool_call("glass.diff", serde_json::json!({}));
            let id = call.id.clone();
            events.push(HarnessEvent::ToolCall(call));
            let result = serde_json::to_value(workspace.diff()?)?;
            events.push(HarnessEvent::ToolResult { id, result });
        } else {
            events.push(HarnessEvent::Text {
                text: "Glass local harness is ready. Try `read <path>`, `files`, `process list`, or `diff`.".into(),
            });
        }
        self.state = "idle".into();
        events.push(HarnessEvent::State {
            state: self.state.clone(),
        });
        Ok(events)
    }

    fn tool_call(&mut self, name: &str, arguments: Value) -> ToolCall {
        let id = format!("tool-{}", self.next_tool_id);
        self.next_tool_id = self.next_tool_id.saturating_add(1);
        ToolCall {
            id,
            name: name.into(),
            arguments,
        }
    }
}

/// Long-lived adapter for Pi's strict LF-delimited JSON RPC mode.
///
/// Glass owns the stable request/event model; this type only translates that
/// model to Pi. No prompt text is persisted by this adapter.
pub struct PiHarness {
    child: Child,
    input: ChildStdin,
    output: Receiver<Value>,
    next_id: u64,
}

impl std::fmt::Debug for PiHarness {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PiHarness")
            .field("pid", &self.child.id())
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl PiHarness {
    pub fn spawn(root: &Path) -> DevelopmentResult<Self> {
        let mut child = Command::new("pi")
            .args([
                "--mode",
                "rpc",
                "--offline",
                "--no-approve",
                "--no-context-files",
                "--no-session",
            ])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| DevelopmentError::Process(format!("failed to start Pi: {error}")))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| DevelopmentError::Process("Pi stdin is unavailable".into()))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| DevelopmentError::Process("Pi stdout is unavailable".into()))?;
        let (sender, receiver) = mpsc::sync_channel(256);
        thread::Builder::new()
            .name("glass-pi-rpc".into())
            .spawn(move || {
                for line in BufReader::new(output).split(b'\n') {
                    let Ok(mut line) = line else { break };
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    if line.len() > 1024 * 1024 {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_slice::<Value>(&line)
                        && sender.send(value).is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(DevelopmentError::Io)?;
        Ok(Self {
            child,
            input,
            output: receiver,
            next_id: 1,
        })
    }

    pub fn request(&mut self, request: HarnessRequest) -> DevelopmentResult<Vec<Value>> {
        let (command, private_text) = match request {
            HarnessRequest::Hello | HarnessRequest::State => {
                (serde_json::json!({"type": "get_state"}), None)
            }
            HarnessRequest::Prompt { text } => (
                serde_json::json!({"type": "prompt", "message": text}),
                Some(()),
            ),
            HarnessRequest::Steer { text } => (
                serde_json::json!({"type": "steer", "message": text}),
                Some(()),
            ),
            HarnessRequest::FollowUp { text } => (
                serde_json::json!({"type": "follow_up", "message": text}),
                Some(()),
            ),
            HarnessRequest::Abort => (serde_json::json!({"type": "abort"}), None),
            HarnessRequest::Models => (serde_json::json!({"type": "get_available_models"}), None),
            HarnessRequest::SetModel { provider, model_id } => (
                serde_json::json!({"type": "set_model", "provider": provider, "modelId": model_id}),
                None,
            ),
            HarnessRequest::SetThinking { level } => (
                serde_json::json!({"type": "set_thinking_level", "level": level}),
                None,
            ),
            HarnessRequest::NewSession => (serde_json::json!({"type": "new_session"}), None),
        };
        let _private_text = private_text;
        self.send(command)
    }

    fn send(&mut self, mut command: Value) -> DevelopmentResult<Vec<Value>> {
        let id = format!("glass-{}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        command["id"] = Value::String(id.clone());
        serde_json::to_writer(&mut self.input, &command)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        let mut events = Vec::new();
        loop {
            let value = self
                .output
                .recv_timeout(Duration::from_secs(10))
                .map_err(|error| {
                    DevelopmentError::Process(format!("Pi RPC response timed out: {error}"))
                })?;
            let is_response = value.get("type").and_then(Value::as_str) == Some("response")
                && value.get("id").and_then(Value::as_str) == Some(id.as_str());
            let failed =
                is_response && value.get("success").and_then(Value::as_bool) == Some(false);
            events.push(value);
            if failed {
                return Err(DevelopmentError::Process(format!(
                    "Pi rejected RPC command {id}"
                )));
            }
            if is_response {
                return Ok(events);
            }
            if events.len() >= 256 {
                return Err(DevelopmentError::Process(
                    "Pi emitted too many events before its response".into(),
                ));
            }
        }
    }
}

impl Drop for PiHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn prompt_evidence(text: &str) -> Value {
    let digest = Sha256::digest(text.as_bytes());
    serde_json::json!({
        "bytes": text.len(),
        "sha256": digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn local_harness_proves_prompt_tool_result_and_steer_events() {
        let root = std::env::temp_dir().join(format!("glass-agent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "hello agent").unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let mut harness = LocalHarness::default();
        assert!(matches!(
            harness
                .handle(&mut workspace, HarnessRequest::Hello)
                .unwrap()[0],
            HarnessEvent::Hello { .. }
        ));
        let events = harness
            .handle(
                &mut workspace,
                HarnessRequest::Prompt {
                    text: "read note.txt".into(),
                },
            )
            .unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, HarnessEvent::ToolCall(_)))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, HarnessEvent::ToolResult { .. }))
        );
        assert!(matches!(
            harness
                .handle(
                    &mut workspace,
                    HarnessRequest::Steer {
                        text: "stop".into()
                    }
                )
                .unwrap()[0],
            HarnessEvent::State { .. }
        ));
        assert!(workspace.timeline().events().all(|event| {
            !event.payload.to_string().contains("read note.txt")
                && !event.payload.to_string().contains("stop")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pi_adapter_negotiates_real_rpc_state_when_pi_is_available() {
        if Command::new("pi").arg("--version").output().is_err() {
            return;
        }
        let mut harness = PiHarness::spawn(Path::new(".")).unwrap();
        let events = harness.request(HarnessRequest::Hello).unwrap();
        assert!(events.iter().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("response")
                && event.get("command").and_then(Value::as_str) == Some("get_state")
                && event.get("success").and_then(Value::as_bool) == Some(true)
        }));
    }

    #[test]
    fn tool_registry_executes_attributed_file_patch_and_fails_closed_for_unattached_browser() {
        let root = std::env::temp_dir().join(format!("glass-tools-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "before\n").unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let registry = ToolRegistry;
        let result = registry
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "tool-1".into(),
                    name: "glass.file.patch".into(),
                    arguments: serde_json::json!({"path":"note.txt","search":"before","replace":"after"}),
                },
                Actor::embedded(),
            )
            .unwrap();
        assert_eq!(result["replacements"], 1);
        assert_eq!(
            fs::read_to_string(root.join("note.txt")).unwrap(),
            "after\n"
        );
        assert!(
            registry
                .execute(
                    &mut workspace,
                    &ToolCall {
                        id: "tool-2".into(),
                        name: "glass.browser.observe".into(),
                        arguments: serde_json::json!({}),
                    },
                    Actor::embedded(),
                )
                .is_err()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn semantic_context_resolves_explicit_references_without_dumping_file_contents() {
        let root = std::env::temp_dir().join(format!("glass-context-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("secret.txt"), "do not inline this source").unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let packet =
            resolve_context(&mut workspace, "Inspect @workspace and @file:secret.txt").unwrap();
        let serialized = serde_json::to_string(&packet).unwrap();
        assert!(!serialized.contains("do not inline this source"));
        assert_eq!(packet.references.len(), 2);
    }
}

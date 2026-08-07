use super::{Actor, DevelopmentEventKind, DevelopmentResult, ProjectWorkspace};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HarnessRequest {
    Hello,
    Prompt { text: String },
    Steer { text: String },
    Abort,
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
                ],
            }]),
            HarnessRequest::Prompt { text } => self.prompt(workspace, text),
            HarnessRequest::Steer { text } => {
                workspace.record_as(
                    self.actor.clone(),
                    DevelopmentEventKind::AgentSteered,
                    serde_json::json!({"text": text}),
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
            serde_json::json!({"text": text}),
        )?;
        self.state = "working".into();
        let mut events = vec![HarnessEvent::State {
            state: self.state.clone(),
        }];
        let lower = text.to_ascii_lowercase();
        if let Some(path) = text
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
        let _ = fs::remove_dir_all(root);
    }
}

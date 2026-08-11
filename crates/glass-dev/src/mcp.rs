//! Full-product MCP tools contributed to the browser-owned transport.

use crate::{DevelopmentToolContext, DevelopmentWorkspace};
use glass_browser::development::{Actor, ToolAuthorization, ToolCall};
use glass_browser::mcp::server::{HostMcpTool, HostMcpToolBackend};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_ARGUMENT_BYTES: usize = 256 * 1024;

pub struct DevelopmentMcpBackend {
    workspace: Mutex<DevelopmentWorkspace>,
    unrestricted: bool,
    next_call: AtomicU64,
}

impl DevelopmentMcpBackend {
    pub fn open(root: impl AsRef<Path>, unrestricted: bool) -> Result<Self, String> {
        Ok(Self {
            workspace: Mutex::new(
                DevelopmentWorkspace::open(root).map_err(|error| error.to_string())?,
            ),
            unrestricted,
            next_call: AtomicU64::new(1),
        })
    }
}

impl HostMcpToolBackend for DevelopmentMcpBackend {
    fn tools(&self) -> Vec<HostMcpTool> {
        let workspace = self
            .workspace
            .lock()
            .expect("development MCP workspace poisoned");
        let mut tools = workspace
            .tool_descriptors()
            .into_iter()
            .filter(|descriptor| descriptor.available)
            .map(|descriptor| HostMcpTool {
                name: descriptor.name,
                description: descriptor.description,
                input_schema: augment_schema(descriptor.input_schema, descriptor.mutating),
            })
            .collect::<Vec<_>>();
        tools.extend(LEGACY_EXECUTION_TOOLS.iter().map(|name| HostMcpTool {
            name: (*name).into(),
            description: format!(
                "Trust-governed compatibility route for legacy development tool {name}"
            ),
            input_schema: augment_schema(json!({"type":"object"}), true),
        }));
        tools
    }

    fn call(&self, name: &str, mut arguments: Value) -> Result<Value, String> {
        if serde_json::to_vec(&arguments)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_ARGUMENT_BYTES
        {
            return Err(format!(
                "development MCP arguments exceed {MAX_ARGUMENT_BYTES} bytes"
            ));
        }
        let object = arguments
            .as_object_mut()
            .ok_or("development MCP arguments must be an object")?;
        let metadata = object.remove("_glass").unwrap_or_else(|| json!({}));
        let actor = metadata
            .get("actor")
            .and_then(Value::as_str)
            .unwrap_or("mcp");
        if actor.is_empty() || actor.len() > 128 || actor.chars().any(char::is_control) {
            return Err("_glass.actor must contain 1..=128 non-control bytes".into());
        }

        let mut workspace = self
            .workspace
            .lock()
            .map_err(|_| "development MCP workspace poisoned".to_string())?;
        let legacy_execution = LEGACY_EXECUTION_TOOLS.contains(&name);
        let descriptor = if legacy_execution {
            glass_browser::development::ToolDescriptor {
                name: name.into(),
                description: format!("Trust-governed compatibility route {name}"),
                input_schema: json!({"type":"object"}),
                mutating: true,
                available: true,
                unavailable_reason: None,
            }
        } else {
            workspace
                .tool_descriptors()
                .into_iter()
                .find(|descriptor| descriptor.name == name && descriptor.available)
                .ok_or_else(|| format!("unknown development MCP tool {name}"))?
        };
        let authorized = self.unrestricted
            || (metadata
                .get("allowMutation")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && metadata
                    .get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false));
        if descriptor.mutating && !authorized {
            return Err(format!(
                "{name} requires _glass.allowMutation=true and _glass.confirmed=true"
            ));
        }
        let (name, arguments) = if legacy_execution {
            if !workspace.trust().permits_project_execution() {
                return Err(format!(
                    "{name} is blocked until the workspace is trusted by a local user"
                ));
            }
            translate_legacy_execution(name, arguments, workspace.root())?
        } else {
            (name.to_string(), arguments)
        };
        let context = DevelopmentToolContext {
            authorization: ToolAuthorization {
                actor: Actor::external(actor),
                allow_mutation: authorized,
                confirmed: authorized,
            },
            initiator: None,
            expected_generation: metadata
                .get("expectedGeneration")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| workspace.generation()),
            expected_project_revision: metadata
                .get("expectedProjectRevision")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| workspace.project().revision()),
        };
        let call = ToolCall {
            id: format!("mcp-{}", self.next_call.fetch_add(1, Ordering::Relaxed)),
            name,
            arguments,
        };
        workspace
            .execute_tool(&call, &context)
            .map_err(|error| error.to_string())
    }
}

const LEGACY_EXECUTION_TOOLS: &[&str] = &[
    "project.edit",
    "project.mkdir",
    "project.rename",
    "project.delete",
    "project.diagnostics",
    "project.run",
    "project.process.stop",
    "project.session.detach",
    "project.capsule.save",
    "project.capsule.clear",
    "project.neovim.probe",
    "project.experiment.create",
    "project.attach",
    "project.link",
    "agent.prompt",
    "agent.steer",
];

fn translate_legacy_execution(
    name: &str,
    mut arguments: Value,
    workspace_root: &Path,
) -> Result<(String, Value), String> {
    let object = arguments
        .as_object_mut()
        .ok_or("legacy development arguments must be an object")?;
    if let Some(root) = object.remove("root") {
        let root = root
            .as_str()
            .ok_or("legacy project root must be a string")?;
        let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
        if root != workspace_root {
            return Err("legacy project tool root does not match the resident workspace".into());
        }
    }
    let mapped = match name {
        "project.edit" => "glass.file.write",
        "project.mkdir" => "glass.file.mkdir",
        "project.rename" => "glass.file.rename",
        "project.delete" => "glass.file.delete",
        "project.diagnostics" => "glass.diagnostics.run",
        "project.run" if object.remove("wait").and_then(|value| value.as_bool()) == Some(true) => {
            "glass.command.run"
        }
        "project.run" => "glass.process.start",
        "project.process.stop" => "glass.process.stop",
        _ => {
            return Err(format!(
                "{name} is trust-gated and pending migration to the Glass Dev router"
            ));
        }
    };
    Ok((mapped.into(), arguments))
}

fn augment_schema(mut schema: Value, mutating: bool) -> Value {
    if !schema.is_object() {
        schema = json!({"type":"object"});
    }
    let object = schema
        .as_object_mut()
        .expect("tool schema must be an object");
    let properties = object
        .entry("properties")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("tool schema properties must be an object");
    properties.insert(
        "_glass".into(),
        json!({
            "type":"object",
            "description":"Optional actor, authority, and stale-context guards.",
            "properties":{
                "actor":{"type":"string","maxLength":128},
                "allowMutation":{"type":"boolean","default":false},
                "confirmed":{"type":"boolean","default":false},
                "expectedGeneration":{"type":"integer","minimum":1},
                "expectedProjectRevision":{"type":"integer","minimum":0}
            },
            "additionalProperties":false,
            "x-glass-mutating":mutating
        }),
    );
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_lists_and_executes_governed_resident_tools() {
        let root = std::env::temp_dir().join(format!("glass-mcp-backend-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("note.txt"), "resident\n").unwrap();
        let backend = DevelopmentMcpBackend::open(&root, false).unwrap();
        assert!(
            backend
                .tools()
                .iter()
                .any(|tool| tool.name == "glass.file.read")
        );
        let read = backend
            .call("glass.file.read", json!({"path":"note.txt"}))
            .unwrap();
        assert_eq!(read["content"], "resident\n");
        assert!(
            backend
                .call(
                    "glass.file.write",
                    json!({"path":"denied.txt","content":"no"})
                )
                .is_err()
        );
        assert!(
            backend
                .call(
                    "glass.file.write",
                    json!({
                        "path":"allowed.txt",
                        "content":"yes",
                        "_glass":{"allowMutation":true,"confirmed":true,"actor":"external-test"}
                    }),
                )
                .unwrap_err()
                .contains("trusted")
        );
        assert!(!root.join("allowed.txt").exists());
        assert!(
            backend
                .call(
                    "project.run",
                    json!({
                        "name":"blocked",
                        "command":if cfg!(windows) { "echo no" } else { "printf no" },
                        "wait":true,
                        "_glass":{"allowMutation":true,"confirmed":true}
                    }),
                )
                .unwrap_err()
                .contains("trusted")
        );
        let status = backend
            .call("glass.workspace.trust.status", json!({}))
            .unwrap();
        assert_eq!(status["trust"], "untrusted");
        std::fs::remove_dir_all(root).unwrap();
    }
}

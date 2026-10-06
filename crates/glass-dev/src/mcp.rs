//! Full-product MCP tools contributed to the browser-owned transport.

use crate::development::{Actor, ToolAuthorization, ToolCall};
use crate::{DevelopmentToolContext, DevelopmentWorkspace};
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
        let mut workspace = DevelopmentWorkspace::open(root).map_err(|error| error.to_string())?;
        if unrestricted {
            workspace
                .enable_unrestricted_execution()
                .map_err(|error| error.to_string())?;
        }
        Ok(Self {
            workspace: Mutex::new(workspace),
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
        let project_execution_permitted = workspace.execution_trust().permits_project_execution();
        tools.extend(
            LEGACY_TOOLS
                .iter()
                .filter(|(_, mutating)| !*mutating || project_execution_permitted)
                .map(|(name, mutating)| HostMcpTool {
                    name: (*name).into(),
                    description: format!(
                        "Trust-governed compatibility route for legacy development tool {name}"
                    ),
                    input_schema: augment_schema(json!({"type":"object"}), *mutating),
                }),
        );
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
        let legacy = LEGACY_TOOLS.iter().find(|(legacy, _)| *legacy == name);
        let legacy_execution = legacy.is_some();
        let execution_trust = workspace.execution_trust();
        let descriptor = if legacy_execution {
            let mutating = legacy.expect("legacy tool checked above").1;
            crate::development::ToolDescriptor {
                name: name.into(),
                description: format!("Trust-governed compatibility route {name}"),
                input_schema: json!({"type":"object"}),
                mutating,
                available: !mutating || execution_trust.permits_project_execution(),
                unavailable_reason: (mutating && !execution_trust.permits_project_execution())
                    .then(|| format!("{name} is blocked until the workspace is trusted")),
            }
        } else {
            workspace
                .tool_descriptors()
                .into_iter()
                .find(|descriptor| descriptor.name == name)
                .ok_or_else(|| format!("unknown development MCP tool {name}"))?
        };
        let allow_mutation = metadata
            .get("allowMutation")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let confirmed = metadata
            .get("confirmed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let authorized = self.unrestricted
            || ToolAuthorization::factors_permit_mutation(allow_mutation, confirmed);
        if !descriptor.available {
            return Err(descriptor
                .unavailable_reason
                .unwrap_or_else(|| format!("{name} is unavailable")));
        }
        if descriptor.mutating && !authorized {
            return Err(format!(
                "{name} requires _glass.allowMutation=true and _glass.confirmed=true"
            ));
        }
        let (name, arguments) = if legacy_execution {
            translate_legacy_execution(name, arguments, workspace.root())?
        } else {
            (name.to_string(), arguments)
        };
        let context = DevelopmentToolContext {
            authorization: ToolAuthorization {
                actor: Actor::external(actor),
                allow_mutation: self.unrestricted || allow_mutation,
                confirmed: self.unrestricted || confirmed,
                unrestricted: self.unrestricted,
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

const LEGACY_TOOLS: &[(&str, bool)] = &[
    ("project.read", false),
    ("project.edit", true),
    ("project.mkdir", true),
    ("project.rename", true),
    ("project.delete", true),
    ("project.diagnostics", true),
    ("project.run", true),
    ("project.process.stop", true),
    ("project.session.detach", true),
    ("project.capsule.save", true),
    ("project.capsule.clear", true),
    ("project.neovim.probe", true),
    ("project.experiment.create", true),
    ("project.attach", true),
    ("project.link", true),
    ("agent.prompt", true),
    ("agent.steer", true),
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
        "project.read" => "glass.file.read",
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
    object.insert("x-glass-mutating".into(), Value::Bool(mutating));
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
            "additionalProperties":false
        }),
    );
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_extension_is_root_level_and_glass_metadata_stays_a_property() {
        let schema = augment_schema(
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
            true,
        );
        assert_eq!(schema["x-glass-mutating"], Value::Bool(true));
        assert!(schema["properties"]["x-glass-mutating"].is_null());
        assert!(schema["properties"]["_glass"].is_object());
        assert_eq!(schema["properties"]["path"]["type"], "string");
    }

    #[test]
    fn backend_lists_and_executes_governed_resident_tools() {
        let root = std::env::temp_dir().join(format!("glass-mcp-backend-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("note.txt"), "resident\n").unwrap();
        let backend = DevelopmentMcpBackend::open(&root, false).unwrap();
        let listed = backend.tools();
        assert!(listed.iter().any(|tool| tool.name == "glass.file.read"));
        assert!(
            !listed.iter().any(|tool| tool.name == "glass.file.write"),
            "untrusted listing must hide tools the execution router blocks"
        );
        assert!(
            !listed.iter().any(|tool| tool.name == "project.edit"),
            "legacy mutating routes must follow the same trust listing"
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

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
        workspace
            .tool_descriptors()
            .into_iter()
            .filter(|descriptor| descriptor.available)
            .map(|descriptor| HostMcpTool {
                name: descriptor.name,
                description: descriptor.description,
                input_schema: augment_schema(descriptor.input_schema, descriptor.mutating),
            })
            .collect()
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
        let descriptor = workspace
            .tool_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == name && descriptor.available)
            .ok_or_else(|| format!("unknown development MCP tool {name}"))?;
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
        let context = DevelopmentToolContext {
            authorization: ToolAuthorization {
                actor: Actor::external(actor),
                allow_mutation: authorized,
                confirmed: authorized,
            },
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
            name: name.to_string(),
            arguments,
        };
        workspace
            .execute_tool(&call, &context)
            .map_err(|error| error.to_string())
    }
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
        backend
            .call(
                "glass.file.write",
                json!({
                    "path":"allowed.txt",
                    "content":"yes",
                    "_glass":{"allowMutation":true,"confirmed":true,"actor":"external-test"}
                }),
            )
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("allowed.txt")).unwrap(),
            "yes"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

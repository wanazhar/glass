//! Full-product MCP tools contributed to the browser-owned transport.

use crate::development::{Actor, ToolAuthorization, ToolCall};
use crate::{DevelopmentToolContext, DevelopmentWorkspace};
use glass_browser::browser::policy::BrowserPolicy;
#[cfg(test)]
use glass_browser::browser::policy::PolicyPreset;
use glass_browser::mcp::server::{HostMcpTool, HostMcpToolBackend};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_ARGUMENT_BYTES: usize = 256 * 1024;

pub(crate) struct DevelopmentMcpBackend {
    workspace: Mutex<DevelopmentWorkspace>,
    next_call: AtomicU64,
}

impl DevelopmentMcpBackend {
    #[cfg(test)]
    pub(crate) fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        let browser_policy = BrowserPolicy::from_preset(PolicyPreset::Development, root.as_ref())
            .map_err(|error| error.to_string())?;
        Self::open_with_browser_policy(root, browser_policy)
    }

    pub(crate) fn open_with_browser_policy(
        root: impl AsRef<Path>,
        browser_policy: BrowserPolicy,
    ) -> Result<Self, String> {
        let workspace = DevelopmentWorkspace::open_with_browser_policy(root, browser_policy)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            workspace: Mutex::new(workspace),
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
        tools.extend(LEGACY_TOOLS.iter().map(|(name, mutating)| HostMcpTool {
            name: (*name).into(),
            description: format!(
                "Glass-routed compatibility operation for development tool {name}"
            ),
            input_schema: augment_schema(legacy_schema(name), *mutating),
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
        let legacy = LEGACY_TOOLS.iter().find(|(legacy, _)| *legacy == name);
        let legacy_execution = legacy.is_some();
        let descriptor = if legacy_execution {
            let mutating = legacy.expect("legacy tool checked above").1;
            crate::development::ToolDescriptor {
                name: name.into(),
                description: format!("Compatibility route for legacy development tool {name}"),
                input_schema: legacy_schema(name),
                mutating,
                available: true,
                unavailable_reason: None,
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
        let authorized = ToolAuthorization::factors_permit_mutation(allow_mutation, confirmed);
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
            authorization: ToolAuthorization::from_user_confirmation(
                Actor::external(actor),
                allow_mutation,
                confirmed,
            ),
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
];

fn legacy_schema(name: &str) -> Value {
    let string = json!({"type":"string", "minLength":1});
    let mut properties = serde_json::Map::new();
    properties.insert("root".into(), string.clone());
    let required = match name {
        "project.read" | "project.delete" | "project.mkdir" => {
            properties.insert("path".into(), string.clone());
            vec!["path"]
        }
        "project.edit" => {
            properties.insert("path".into(), string.clone());
            properties.insert("content".into(), json!({"type":"string"}));
            vec!["path", "content"]
        }
        "project.rename" => {
            properties.insert("from".into(), string.clone());
            properties.insert("to".into(), string.clone());
            vec!["from", "to"]
        }
        "project.diagnostics" => {
            properties.insert("path".into(), string.clone());
            vec!["path"]
        }
        "project.run" => {
            properties.insert("name".into(), string.clone());
            properties.insert("command".into(), string.clone());
            properties.insert("wait".into(), json!({"type":"boolean"}));
            vec!["name", "command"]
        }
        "project.process.stop" => {
            properties.insert("name".into(), string);
            vec!["name"]
        }
        _ => Vec::new(),
    };
    json!({
        "type":"object",
        "properties":properties,
        "required":required,
        "additionalProperties":false
    })
}

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
        "project.run" => {
            if object.remove("wait").and_then(|value| value.as_bool()) == Some(true) {
                "glass.command.run"
            } else {
                "glass.process.start"
            }
        }
        "project.process.stop" => "glass.process.stop",
        _ => {
            return Err(format!("{name} has no supported Glass tool mapping"));
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
    use glass_browser::browser::policy::NavigationPreflightDecision;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_POLICY_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn legacy_mcp_routes_expose_required_arguments_and_only_supported_mappings() {
        let cases = [
            ("project.read", vec!["path"]),
            ("project.edit", vec!["path", "content"]),
            ("project.mkdir", vec!["path"]),
            ("project.rename", vec!["from", "to"]),
            ("project.delete", vec!["path"]),
            ("project.diagnostics", vec!["path"]),
            ("project.run", vec!["name", "command"]),
            ("project.process.stop", vec!["name"]),
        ];
        let expected_count = cases.len();
        for (name, expected) in cases {
            let schema = legacy_schema(name);
            assert_eq!(schema["required"], json!(expected), "{name}");
        }
        assert_eq!(LEGACY_TOOLS.len(), expected_count);
        assert!(!LEGACY_TOOLS.iter().any(|(name, _)| {
            matches!(
                *name,
                "project.session.detach" | "project.capsule.save" | "agent.prompt"
            )
        }));

        let (wait_tool, wait_args) = translate_legacy_execution(
            "project.run",
            json!({"name":"verify","command":"printf ok","wait":true}),
            &std::env::current_dir().unwrap(),
        )
        .unwrap();
        assert_eq!(wait_tool, "glass.command.run");
        assert_eq!(wait_args, json!({"name":"verify","command":"printf ok"}));
        let (start_tool, start_args) = translate_legacy_execution(
            "project.run",
            json!({"name":"dev","command":"npm run dev","wait":false}),
            &std::env::current_dir().unwrap(),
        )
        .unwrap();
        assert_eq!(start_tool, "glass.process.start");
        assert_eq!(start_args, json!({"name":"dev","command":"npm run dev"}));
    }

    #[test]
    fn backend_forwards_complete_browser_policy_to_resident_workspace() {
        let sequence = NEXT_POLICY_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-mcp-policy-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        let policy = BrowserPolicy::development(&root)
            .unwrap()
            .with_host_rules(
                ["allowed.example.test".to_string()],
                ["denied.example.test".to_string()],
            )
            .unwrap();

        let backend = DevelopmentMcpBackend::open_with_browser_policy(&root, policy).unwrap();
        let workspace = backend.workspace.lock().unwrap();
        let browser = workspace.browser();
        assert_eq!(
            browser
                .navigation_preflight("https://allowed.example.test/path")
                .decision,
            NavigationPreflightDecision::Allow
        );
        assert_eq!(
            browser
                .navigation_preflight("https://denied.example.test/path")
                .decision,
            NavigationPreflightDecision::Deny
        );

        drop(workspace);
        drop(backend);
        std::fs::remove_dir_all(root).unwrap();
    }

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
        let backend = DevelopmentMcpBackend::open(&root).unwrap();
        let listed = backend.tools();
        assert!(listed.iter().any(|tool| tool.name == "glass.file.read"));
        assert!(listed.iter().any(|tool| tool.name == "glass.file.write"));
        assert!(
            listed.iter().any(|tool| tool.name == "project.edit"),
            "workspace editing tools are available on first open"
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
                .unwrap()["written"]
                .as_bool()
                .unwrap_or(false)
        );
        assert_eq!(
            std::fs::read_to_string(root.join("allowed.txt")).unwrap(),
            "yes"
        );
        backend
            .call(
                "project.run",
                json!({
                    "name":"allowed",
                    "command":if cfg!(windows) { "echo yes" } else { "printf yes" },
                    "wait":true,
                    "_glass":{"allowMutation":true,"confirmed":true}
                }),
            )
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

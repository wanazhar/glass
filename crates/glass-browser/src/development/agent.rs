use super::{Actor, DevelopmentError, DevelopmentEventKind, DevelopmentResult, ProjectWorkspace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, VecDeque},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::Duration,
};

const MAX_PI_EVENT_BYTES: usize = 512 * 1024;
const MAX_PI_BUFFERED_EVENTS: usize = 64;
const PI_EVENT_CHANNEL_CAPACITY: usize = 32;

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
    pub browser: Option<BrowserAgentContext>,
    pub authority: AgentAuthorityContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentContext {
    pub connected: bool,
    pub target_id: Option<String>,
    pub origin: Option<String>,
    pub title: String,
    pub browser_revision: u64,
    pub semantic_summary: String,
    pub workflow_state: String,
    pub memory_scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuthorityContext {
    pub project_revision: u64,
    pub browser_revision: Option<u64>,
    pub mutation_lease_required: bool,
    pub stale_context_rejected: bool,
}

pub fn resolve_context(
    workspace: &mut ProjectWorkspace,
    prompt: &str,
) -> DevelopmentResult<AgentContextPacket> {
    resolve_context_with_browser(workspace, prompt, None)
}

pub fn resolve_context_with_browser(
    workspace: &mut ProjectWorkspace,
    prompt: &str,
    browser: Option<&BrowserAgentContext>,
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
            "@page" | "@browser" => browser.map(serde_json::to_value).transpose()?.unwrap_or_else(|| serde_json::json!({
                "status": "requires-attached-browser-workspace", "defaultObservation": "structured"
            })),
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
            "@workflow" => browser.map(|value| serde_json::json!({"state":value.workflow_state,"browserRevision":value.browser_revision})).unwrap_or_else(|| serde_json::json!({"status":"not-attached"})),
            "@memory" => browser.map(|value| serde_json::json!({"scope":value.memory_scope,"authoritative":false})).unwrap_or_else(|| serde_json::json!({"status":"not-attached"})),
            "@selection" | "@file" | "@symbol" => serde_json::json!({"status": "not-selected"}),
            value => {
                return Err(DevelopmentError::InvalidInput(format!(
                    "unsupported agent context reference: {value}"
                )));
            }
        };
        resolved.insert(reference.clone(), value);
    }
    let processes = workspace.processes().list_checked()?;
    let events = workspace
        .timeline()
        .events()
        .rev()
        .take(32)
        .collect::<Vec<_>>();
    let project_revision = workspace.revision();
    Ok(AgentContextPacket {
        schema_version: "glass.agent-context.v1".into(),
        references,
        project: serde_json::to_value(workspace.detection())?,
        diagnostics: serde_json::to_value(workspace.diagnostics())?,
        processes: serde_json::to_value(processes)?,
        recent_events: serde_json::to_value(events)?,
        resolved: Value::Object(resolved),
        browser: browser.cloned(),
        authority: AgentAuthorityContext {
            project_revision,
            browser_revision: browser.map(|value| value.browser_revision),
            mutation_lease_required: true,
            stale_context_rejected: true,
        },
    })
}

#[derive(Debug, Clone, Default)]
pub struct ToolRegistry;

const MAX_TOOL_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_TOOL_RESULT_BYTES: usize = 64 * 1024;
const MAX_TOOL_CALL_ID_BYTES: usize = 128;
const MAX_TOOL_NAME_BYTES: usize = 128;

#[derive(Debug, Clone)]
pub struct ToolAuthorization {
    pub actor: Actor,
    pub allow_mutation: bool,
    pub confirmed: bool,
}

impl ToolAuthorization {
    pub fn read_only(actor: Actor) -> Self {
        Self {
            actor,
            allow_mutation: false,
            confirmed: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentToolGateway {
    registry: ToolRegistry,
    descriptors: Vec<ToolDescriptor>,
    browser_context: Option<BrowserAgentContext>,
}

impl Default for AgentToolGateway {
    fn default() -> Self {
        let registry = ToolRegistry;
        let descriptors = registry.descriptors();
        Self {
            registry,
            descriptors,
            browser_context: None,
        }
    }
}

impl AgentToolGateway {
    pub fn set_browser_context(&mut self, context: Option<BrowserAgentContext>) {
        self.browser_context = context;
        for descriptor in &mut self.descriptors {
            let available = match descriptor.name.as_str() {
                "glass.browser.observe" | "glass.memory.retrieve" => self
                    .browser_context
                    .as_ref()
                    .is_some_and(|value| value.connected),
                "glass.workflow.pause" | "glass.workflow.resume" => self
                    .browser_context
                    .as_ref()
                    .is_some_and(|value| value.connected && value.workflow_state == "active"),
                _ => continue,
            };
            descriptor.available = available;
            descriptor.unavailable_reason = (!available).then(|| match descriptor.name.as_str() {
                "glass.memory.retrieve" => {
                    "Requires an attached profile-scoped semantic memory store".into()
                }
                name if name.starts_with("glass.workflow") => {
                    "Requires an active browser workflow run".into()
                }
                _ => "Requires an attached Browser Workspace".into(),
            });
        }
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.descriptors.clone()
    }

    pub fn execute(
        &self,
        workspace: &mut ProjectWorkspace,
        call: &ToolCall,
        authorization: &ToolAuthorization,
    ) -> DevelopmentResult<Value> {
        validate_tool_call_envelope(call)?;
        let descriptor = self
            .descriptors
            .iter()
            .find(|descriptor| descriptor.name == call.name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("tool {}", call.name)))?;
        if !descriptor.available {
            return Err(DevelopmentError::InvalidInput(
                descriptor
                    .unavailable_reason
                    .clone()
                    .unwrap_or_else(|| format!("tool {} is unavailable", descriptor.name)),
            ));
        }
        let argument_evidence = validate_tool_arguments(descriptor, &call.arguments)?;
        if descriptor.mutating && (!authorization.allow_mutation || !authorization.confirmed) {
            return Err(DevelopmentError::Conflict(format!(
                "tool {} requires explicit mutation authority and confirmation",
                descriptor.name
            )));
        }
        if descriptor.mutating {
            let expected_revision =
                call.arguments["expectedRevision"].as_u64().ok_or_else(|| {
                    DevelopmentError::InvalidInput(format!(
                        "tool {} requires unsigned integer argument expectedRevision",
                        descriptor.name
                    ))
                })?;
            let current_revision = workspace.revision();
            if expected_revision != current_revision {
                return Err(DevelopmentError::Conflict(format!(
                    "tool {} was approved for stale project revision {expected_revision}; current revision is {current_revision}",
                    descriptor.name
                )));
            }
        }
        workspace.record_as(
            authorization.actor.clone(),
            DevelopmentEventKind::AgentToolCalled,
            tool_call_evidence(call, descriptor.mutating, &argument_evidence),
        )?;
        let result = self.execute_attached(call).unwrap_or_else(|| {
            self.registry
                .execute_unchecked(workspace, call, authorization.actor.clone())
        });
        let (ok, bytes) = match &result {
            Ok(value) => {
                let mut counter = BoundedJsonWriter::new(MAX_TOOL_RESULT_BYTES);
                let serialized = serde_json::to_writer(&mut counter, value);
                if counter.exceeded {
                    workspace.record_as(
                        authorization.actor.clone(),
                        DevelopmentEventKind::AgentToolResult,
                        serde_json::json!({"id": call.id, "name": call.name, "ok": false, "reason": "result-limit", "bytes": counter.bytes}),
                    )?;
                    return Err(DevelopmentError::InvalidInput(format!(
                        "tool result exceeds the {MAX_TOOL_RESULT_BYTES} byte limit"
                    )));
                }
                serialized?;
                (true, counter.bytes)
            }
            Err(_) => (false, 0),
        };
        workspace.record_as(
            authorization.actor.clone(),
            DevelopmentEventKind::AgentToolResult,
            serde_json::json!({"id": call.id, "name": call.name, "ok": ok, "bytes": bytes}),
        )?;
        result
    }

    fn execute_attached(&self, call: &ToolCall) -> Option<DevelopmentResult<Value>> {
        let context = self.browser_context.as_ref()?;
        match call.name.as_str() {
            "glass.browser.observe" => Some(Ok(serde_json::json!({
                "structured": true,
                "targetId": context.target_id,
                "origin": context.origin,
                "title": context.title,
                "browserRevision": context.browser_revision,
                "semanticSummary": context.semantic_summary,
                "freshness": "current-context-packet"
            }))),
            "glass.memory.retrieve" => Some(Ok(serde_json::json!({
                "scope": context.memory_scope,
                "authoritative": false,
                "requiresFreshSemanticEvidence": true
            }))),
            "glass.workflow.pause" | "glass.workflow.resume" => Some(Ok(serde_json::json!({
                "workflowState": context.workflow_state,
                "browserRevision": context.browser_revision,
                "requiresMutationLease": true
            }))),
            _ => None,
        }
    }
}

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
                "glass.file.list",
                "List bounded project files",
                schema(serde_json::json!({}), &[]),
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
                    serde_json::json!({"path":{"type":"string"},"search":{"type":"string"},"replace":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0}}),
                    &["path", "search", "replace", "expectedRevision"],
                ),
                true,
            ),
            descriptor(
                "glass.process.start",
                "Start a named PTY process",
                schema(
                    serde_json::json!({"name":{"type":"string"},"command":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0}}),
                    &["name", "command", "expectedRevision"],
                ),
                true,
            ),
            descriptor(
                "glass.process.stop",
                "Stop a named managed process",
                schema(
                    serde_json::json!({"name":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0}}),
                    &["name", "expectedRevision"],
                ),
                true,
            ),
            descriptor(
                "glass.process.logs",
                "Read a bounded managed-process output tail",
                schema(serde_json::json!({"name":{"type":"string"}}), &["name"]),
                false,
            ),
            descriptor(
                "glass.process.list",
                "List managed processes",
                schema(serde_json::json!({}), &[]),
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
                    serde_json::json!({"name":{"type":"string"},"command":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0}}),
                    &["name", "command", "expectedRevision"],
                ),
                true,
            ),
            descriptor(
                "glass.runtime.inspect",
                "Inspect project, processes, actors, and diagnostics",
                schema(serde_json::json!({}), &[]),
                false,
            ),
            descriptor(
                "glass.web_ir.inspect",
                "Validate and inspect one bounded Web IR document",
                schema(serde_json::json!({"ir":{"type":"object"}}), &["ir"]),
                false,
            ),
            descriptor(
                "glass.web_ir.diff",
                "Summarize a validated Web IR revision transition",
                schema(
                    serde_json::json!({"before":{"type":"object"},"after":{"type":"object"}}),
                    &["before", "after"],
                ),
                false,
            ),
            descriptor(
                "glass.web_ir.continuity",
                "Classify graph-scoped entity continuity across two Web IR revisions",
                schema(
                    serde_json::json!({"before":{"type":"object"},"after":{"type":"object"},"entityId":{"type":"string"}}),
                    &["before", "after", "entityId"],
                ),
                false,
            ),
            descriptor(
                "glass.task.plan",
                "Compile a value-free task and return a compact explanation",
                schema(
                    serde_json::json!({"task":{"type":"object"},"ir":{"type":"object"}}),
                    &["task", "ir"],
                ),
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

    fn execute_unchecked(
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
            "glass.file.list" => Ok(serde_json::to_value(workspace.list_files_result()?)?),
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
            "glass.process.list" => {
                Ok(serde_json::to_value(workspace.processes().list_checked()?)?)
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
                "processes": workspace.processes().list_checked()?,
                "actors": workspace.actors().collect::<Vec<_>>(),
                "diagnostics": workspace.diagnostics(),
                "revision": workspace.revision()
            })),
            "glass.web_ir.inspect" => {
                let ir: crate::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["ir"].clone())?;
                ir.validate().map_err(|error| {
                    DevelopmentError::InvalidInput(format!("invalid Web IR: {error}"))
                })?;
                Ok(serde_json::to_value(
                    crate::protocol::WebIrInspectionResult::from_ir(&ir),
                )?)
            }
            "glass.web_ir.diff" => {
                let before: crate::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["before"].clone())?;
                let after: crate::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["after"].clone())?;
                let diff = before.diff(&after).map_err(|error| {
                    DevelopmentError::InvalidInput(format!("invalid Web IR transition: {error}"))
                })?;
                Ok(serde_json::json!({
                    "fromRevision": diff.from_revision,
                    "toRevision": diff.to_revision,
                    "entityChanges": diff.entity_changes.len(),
                    "relationshipChanges": diff.relationship_changes.len(),
                    "coverageChanged": diff.coverage_changed,
                    "limitsChanged": diff.limits_changed
                }))
            }
            "glass.web_ir.continuity" => {
                let before: crate::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["before"].clone())?;
                let after: crate::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["after"].clone())?;
                let entity_id = string("entityId")?;
                Ok(serde_json::to_value(
                    before
                        .classify_entity_continuity(&after, entity_id)
                        .map_err(|error| {
                            DevelopmentError::InvalidInput(format!(
                                "invalid Web IR transition: {error}"
                            ))
                        })?,
                )?)
            }
            "glass.task.plan" => {
                let task: crate::task_protocol::GlassTask =
                    serde_json::from_value(call.arguments["task"].clone())?;
                let ir: crate::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["ir"].clone())?;
                let plan = crate::task_compiler::compile_task(&task, &ir).map_err(|error| {
                    DevelopmentError::InvalidInput(format!("task compilation failed: {error}"))
                })?;
                Ok(serde_json::json!({
                    "sourceRevision": plan.source_ir_revision,
                    "task": plan.task,
                    "risk": plan.risk,
                    "confirmationRequired": plan.confirmation_required,
                    "selectedEntityIds": plan.selected_entity_ids,
                    "requiredRuntimeCapabilities": plan.required_runtime_capabilities,
                    "entityEvidenceRequirements": plan.entity_evidence_requirements,
                    "steps": plan.steps,
                    "postconditions": plan.postconditions
                }))
            }
            _ => Err(DevelopmentError::NotFound(format!("tool {}", call.name))),
        }
    }
}

struct ToolArgumentEvidence {
    bytes: usize,
    sha256: String,
}

struct BoundedJsonWriter {
    limit: usize,
    bytes: usize,
    exceeded: bool,
    digest: Sha256,
}

impl BoundedJsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            bytes: 0,
            exceeded: false,
            digest: Sha256::new(),
        }
    }

    fn evidence(self) -> ToolArgumentEvidence {
        let digest = self.digest.finalize();
        ToolArgumentEvidence {
            bytes: self.bytes,
            sha256: digest_hex(&digest),
        }
    }
}

impl Write for BoundedJsonWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.bytes);
        if buffer.len() > remaining {
            self.bytes = self.bytes.saturating_add(buffer.len());
            self.exceeded = true;
            return Err(std::io::Error::other("bounded JSON limit exceeded"));
        }
        self.bytes += buffer.len();
        self.digest.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn validate_tool_call_envelope(call: &ToolCall) -> DevelopmentResult<()> {
    if call.id.is_empty()
        || call.id.len() > MAX_TOOL_CALL_ID_BYTES
        || call.id.chars().any(char::is_control)
    {
        return Err(DevelopmentError::InvalidInput(format!(
            "tool call id must contain 1 to {MAX_TOOL_CALL_ID_BYTES} non-control bytes"
        )));
    }
    if call.name.is_empty()
        || call.name.len() > MAX_TOOL_NAME_BYTES
        || !call
            .name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(DevelopmentError::InvalidInput(format!(
            "tool name must contain 1 to {MAX_TOOL_NAME_BYTES} ASCII identifier bytes"
        )));
    }
    Ok(())
}

fn validate_tool_arguments(
    descriptor: &ToolDescriptor,
    arguments: &Value,
) -> DevelopmentResult<ToolArgumentEvidence> {
    let mut writer = BoundedJsonWriter::new(MAX_TOOL_ARGUMENT_BYTES);
    let serialized = serde_json::to_writer(&mut writer, arguments);
    if writer.exceeded {
        return Err(DevelopmentError::InvalidInput(format!(
            "tool arguments exceed the {MAX_TOOL_ARGUMENT_BYTES} byte limit"
        )));
    }
    serialized?;
    let object = arguments.as_object().ok_or_else(|| {
        DevelopmentError::InvalidInput(format!("{} arguments must be an object", descriptor.name))
    })?;
    let properties = descriptor.input_schema["properties"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for required in descriptor.input_schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !object.contains_key(required) {
            return Err(DevelopmentError::InvalidInput(format!(
                "{} requires argument {required}",
                descriptor.name
            )));
        }
    }
    for (name, value) in object {
        let Some(property) = properties.get(name) else {
            return Err(DevelopmentError::InvalidInput(format!(
                "{} does not accept argument {name}",
                descriptor.name
            )));
        };
        let valid = match property["type"].as_str() {
            Some("string") => value.is_string(),
            Some("object") => value.is_object(),
            Some("integer") => value.as_u64().is_some(),
            _ => true,
        };
        if !valid {
            return Err(DevelopmentError::InvalidInput(format!(
                "{}.{} has the wrong type",
                descriptor.name, name
            )));
        }
    }
    Ok(writer.evidence())
}

fn tool_call_evidence(call: &ToolCall, mutating: bool, evidence: &ToolArgumentEvidence) -> Value {
    serde_json::json!({
        "id": call.id,
        "name": call.name,
        "mutating": mutating,
        "argumentBytes": evidence.bytes,
        "argumentSha256": evidence.sha256
    })
}

fn digest_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
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
    gateway: AgentToolGateway,
    next_tool_id: u64,
    state: String,
    browser_context: Option<BrowserAgentContext>,
}

impl Default for LocalHarness {
    fn default() -> Self {
        Self {
            actor: Actor::embedded(),
            gateway: AgentToolGateway::default(),
            next_tool_id: 1,
            state: "idle".into(),
            browser_context: None,
        }
    }
}

impl LocalHarness {
    pub fn set_browser_context(&mut self, context: Option<BrowserAgentContext>) {
        self.browser_context = context.clone();
        self.gateway.set_browser_context(context);
    }
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
            let packet =
                resolve_context_with_browser(workspace, &text, self.browser_context.as_ref())?;
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
            events.push(HarnessEvent::ToolCall(call.clone()));
            match self.gateway.execute(
                workspace,
                &call,
                &ToolAuthorization::read_only(self.actor.clone()),
            ) {
                Ok(result) => {
                    events.push(HarnessEvent::ToolResult { id, result });
                }
                Err(error) => events.push(HarnessEvent::Error {
                    message: error.to_string(),
                }),
            }
        } else if lower == "files" || lower == "list files" {
            let call = self.tool_call("glass.file.list", serde_json::json!({}));
            let id = call.id.clone();
            events.push(HarnessEvent::ToolCall(call.clone()));
            let result = self.gateway.execute(
                workspace,
                &call,
                &ToolAuthorization::read_only(self.actor.clone()),
            )?;
            events.push(HarnessEvent::ToolResult { id, result });
        } else if lower == "process list" || lower == "processes" {
            let call = self.tool_call("glass.process.list", serde_json::json!({}));
            let id = call.id.clone();
            events.push(HarnessEvent::ToolCall(call.clone()));
            let result = self.gateway.execute(
                workspace,
                &call,
                &ToolAuthorization::read_only(self.actor.clone()),
            )?;
            events.push(HarnessEvent::ToolResult { id, result });
        } else if lower == "diff" {
            let call = self.tool_call("glass.git.status", serde_json::json!({}));
            let id = call.id.clone();
            events.push(HarnessEvent::ToolCall(call.clone()));
            let result = self.gateway.execute(
                workspace,
                &call,
                &ToolAuthorization::read_only(self.actor.clone()),
            )?;
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
    extension_path: PathBuf,
    pending_ui_requests: BTreeSet<String>,
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
        static NEXT_EXTENSION_ID: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(1);
        let extension_id = NEXT_EXTENSION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let extension_path = std::env::temp_dir().join(format!(
            "glass-pi-tools-{}-{extension_id}.ts",
            std::process::id()
        ));
        std::fs::write(
            &extension_path,
            include_str!("../../assets/pi-glass-tools.ts"),
        )?;
        let broker = std::env::current_exe().map_err(DevelopmentError::Io)?;
        let mut child = Command::new("pi")
            .args([
                "--mode",
                "rpc",
                "--offline",
                "--no-approve",
                "--no-context-files",
                "--no-extensions",
                "--no-skills",
                "--no-prompt-templates",
                "--no-themes",
                "--no-session",
                "--no-builtin-tools",
                "--system-prompt",
                include_str!("../../assets/pi-glass-system.md"),
                "--extension",
            ])
            .arg(&extension_path)
            .args([
                "--tools",
                "glass_file_read,glass_file_list,glass_file_search,glass_git_status,glass_semantic_inspect,glass_web_ir_inspect,glass_web_ir_diff,glass_web_ir_continuity,glass_task_plan,glass_process_logs,glass_process_list,glass_runtime_inspect,glass_file_patch,glass_process_start,glass_process_stop,glass_test_run",
            ])
            .env("GLASS_PI_BROKER_BIN", broker)
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
        let (sender, receiver) = mpsc::sync_channel(PI_EVENT_CHANNEL_CAPACITY);
        thread::Builder::new()
            .name("glass-pi-rpc".into())
            .spawn(move || {
                for line in BufReader::new(output).split(b'\n') {
                    let Ok(mut line) = line else { break };
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    if line.len() > MAX_PI_EVENT_BYTES {
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
            extension_path,
            pending_ui_requests: BTreeSet::new(),
        })
    }

    pub fn request(&mut self, request: HarnessRequest) -> DevelopmentResult<Vec<Value>> {
        let wait_for_agent = matches!(request, HarnessRequest::Prompt { .. });
        let id = self.start_request(request)?;
        let mut events = VecDeque::with_capacity(MAX_PI_BUFFERED_EVENTS);
        let mut response_received = false;
        let mut observed = 0_usize;
        loop {
            let value = self
                .recv_event_timeout(if response_received {
                    Duration::from_secs(120)
                } else {
                    Duration::from_secs(10)
                })?
                .ok_or_else(|| {
                    DevelopmentError::Process(if response_received {
                        "Pi agent event stream timed out".into()
                    } else {
                        "Pi RPC response timed out".into()
                    })
                })?;
            observed = observed.saturating_add(1);
            let is_response = pi_response_matches(&value, &id);
            let failed =
                is_response && value.get("success").and_then(Value::as_bool) == Some(false);
            let settled = pi_agent_settled(&value);
            if let Some(request) = pi_ui_request(&value)? {
                // A one-shot/non-interactive caller has nowhere safe to ask a
                // human. Deny immediately instead of hanging or inheriting
                // ambient authority.
                self.respond_extension_ui(&request.id, false)?;
            }
            if pi_event_visible(&value) {
                if events.len() == MAX_PI_BUFFERED_EVENTS {
                    events.pop_front();
                }
                events.push_back(value);
            }
            if failed {
                return Err(DevelopmentError::Process(format!(
                    "Pi rejected RPC command {id}"
                )));
            }
            if is_response {
                response_received = true;
                if !wait_for_agent {
                    return Ok(events.into());
                }
            }
            if response_received && settled {
                return Ok(events.into());
            }
            if observed >= 4096 {
                return Err(DevelopmentError::Process(
                    "Pi emitted too many events before settling".into(),
                ));
            }
        }
    }

    pub(crate) fn start_request(&mut self, request: HarnessRequest) -> DevelopmentResult<String> {
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

    fn send(&mut self, mut command: Value) -> DevelopmentResult<String> {
        let id = format!("glass-{}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        command["id"] = Value::String(id.clone());
        let encoded = serde_json::to_vec(&command)?;
        if encoded.len() > 1024 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "Pi RPC command exceeds the 1 MiB limit".into(),
            ));
        }
        self.input.write_all(&encoded)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        Ok(id)
    }

    pub(crate) fn respond_extension_ui(
        &mut self,
        id: &str,
        confirmed: bool,
    ) -> DevelopmentResult<()> {
        if !self.pending_ui_requests.remove(id) {
            return Err(DevelopmentError::Conflict(format!(
                "stale or unknown Pi UI request: {id}"
            )));
        }
        let encoded = serde_json::to_vec(&serde_json::json!({
            "type": "extension_ui_response",
            "id": id,
            "confirmed": confirmed,
        }))?;
        self.input.write_all(&encoded)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        Ok(())
    }

    pub(crate) fn recv_event_timeout(
        &mut self,
        timeout: Duration,
    ) -> DevelopmentResult<Option<Value>> {
        match self.output.recv_timeout(timeout) {
            Ok(value) => {
                if let Some(request) = pi_ui_request(&value)?
                    && !self.pending_ui_requests.insert(request.id.clone())
                {
                    return Err(DevelopmentError::Conflict(format!(
                        "duplicate Pi UI request: {}",
                        request.id
                    )));
                }
                if self.pending_ui_requests.len() > 8 {
                    return Err(DevelopmentError::Conflict(
                        "too many pending Pi UI requests".into(),
                    ));
                }
                Ok(Some(value))
            }
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(DevelopmentError::Process(
                "Pi RPC event stream closed".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PiUiRequest {
    pub id: String,
    pub title: String,
    pub message: String,
}

pub(crate) fn pi_ui_request(value: &Value) -> DevelopmentResult<Option<PiUiRequest>> {
    if value.get("type").and_then(Value::as_str) != Some("extension_ui_request") {
        return Ok(None);
    }
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control))
        .ok_or_else(|| DevelopmentError::InvalidInput("invalid Pi UI request id".into()))?;
    let method = value.get("method").and_then(Value::as_str).unwrap_or("");
    if method != "confirm" {
        return Err(DevelopmentError::InvalidInput(format!(
            "unsupported Pi UI request method: {method}"
        )));
    }
    let bounded_field = |name: &str, limit: usize| -> DevelopmentResult<String> {
        let field = value.get(name).and_then(Value::as_str).unwrap_or("");
        if field.len() > limit || field.chars().any(|character| character == '\0') {
            return Err(DevelopmentError::InvalidInput(format!(
                "invalid Pi UI request {name}"
            )));
        }
        Ok(field.to_string())
    };
    Ok(Some(PiUiRequest {
        id: id.to_string(),
        title: bounded_field("title", 256)?,
        message: bounded_field("message", 2048)?,
    }))
}

pub(crate) fn pi_response_matches(value: &Value, id: &str) -> bool {
    value.get("type").and_then(Value::as_str) == Some("response")
        && value.get("id").and_then(Value::as_str) == Some(id)
}

pub(crate) fn pi_agent_settled(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("agent_settled")
}

pub(crate) fn pi_event_visible(value: &Value) -> bool {
    if value.get("type").and_then(Value::as_str) == Some("message_end") {
        return value
            .get("message")
            .and_then(|message| message.get("role"))
            .and_then(Value::as_str)
            == Some("assistant");
    }
    matches!(
        value.get("type").and_then(Value::as_str),
        Some(
            "response"
                | "tool_execution_start"
                | "tool_execution_end"
                | "agent_settled"
                | "extension_error"
                | "extension_ui_request"
        )
    )
}

pub(crate) fn pi_event_display(value: &Value) -> Option<String> {
    match value.get("type").and_then(Value::as_str)? {
        "message_end" => {
            let message = value.get("message")?;
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return None;
            }
            let content = message.get("content")?;
            if let Some(text) = content.as_str() {
                return (!text.is_empty()).then(|| text.to_string());
            }
            let text = content
                .as_array()?
                .iter()
                .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(text)
        }
        "tool_execution_start" => Some(format!(
            "Pi tool running: {}",
            value
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )),
        "tool_execution_end" => Some(format!(
            "Pi tool {}: {}",
            value
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            if value.get("isError").and_then(Value::as_bool) == Some(true) {
                "failed"
            } else {
                "completed"
            }
        )),
        "extension_error" => Some(format!(
            "Pi extension error: {}",
            value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
        )),
        "response" => serde_json::to_string_pretty(value).ok(),
        "agent_settled" | "extension_ui_request" => None,
        _ => None,
    }
}

impl Drop for PiHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.extension_path);
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
        assert!(
            !events.iter().any(|event| {
                event.get("type").and_then(Value::as_str) == Some("extension_error")
            })
        );
        assert!(events.iter().any(|event| {
            event.get("type").and_then(Value::as_str) == Some("response")
                && event.get("command").and_then(Value::as_str) == Some("get_state")
                && event.get("success").and_then(Value::as_bool) == Some(true)
        }));
    }

    #[test]
    fn pi_contract_is_glass_specific_approved_and_event_bounded() {
        let prompt = include_str!("../../assets/pi-glass-system.md");
        let tools = include_str!("../../assets/pi-glass-tools.ts");
        assert!(prompt.contains("embedded inside Glass Dev"));
        assert!(prompt.contains("Structured browser observation is the default"));
        assert!(prompt.contains("per-call approval"));
        for tool in [
            "glass_file_read",
            "glass_file_list",
            "glass_file_search",
            "glass_git_status",
            "glass_semantic_inspect",
            "glass_web_ir_inspect",
            "glass_web_ir_diff",
            "glass_web_ir_continuity",
            "glass_task_plan",
            "glass_process_logs",
            "glass_process_list",
            "glass_runtime_inspect",
            "glass_file_patch",
            "glass_process_start",
            "glass_process_stop",
            "glass_test_run",
        ] {
            assert!(tools.contains(tool), "missing Pi tool {tool}");
        }
        assert!(tools.contains("ctx.ui.confirm"));
        assert!(tools.contains("--allow-mutation"));
        assert!(tools.contains("exact serialized call once"));

        let response = serde_json::json!({
            "type": "response",
            "id": "glass-9",
            "command": "prompt",
            "success": true
        });
        assert!(pi_response_matches(&response, "glass-9"));
        assert!(pi_event_visible(&response));
        assert!(!pi_agent_settled(&response));
        assert!(!pi_event_visible(
            &serde_json::json!({"type":"message_update"})
        ));
        assert!(!pi_event_visible(&serde_json::json!({
            "type":"message_end",
            "message":{"role":"user","content":"private prompt"}
        })));
        assert!(!pi_agent_settled(&serde_json::json!({"type":"agent_end"})));
        assert!(pi_agent_settled(
            &serde_json::json!({"type":"agent_settled"})
        ));
        assert_eq!(
            pi_event_display(&serde_json::json!({
                "type":"message_end",
                "message":{"role":"assistant","content":[{"type":"text","text":"done"}]}
            })),
            Some("done".into())
        );
    }

    #[test]
    fn pi_ui_requests_accept_only_bounded_confirm_dialogs() {
        let request = serde_json::json!({
            "type":"extension_ui_request",
            "id":"approval-7",
            "method":"confirm",
            "title":"Approve glass.file.patch?",
            "message":"File: src/main.rs"
        });
        assert_eq!(
            pi_ui_request(&request).unwrap(),
            Some(PiUiRequest {
                id: "approval-7".into(),
                title: "Approve glass.file.patch?".into(),
                message: "File: src/main.rs".into(),
            })
        );
        assert!(
            pi_ui_request(&serde_json::json!({
                "type":"extension_ui_request", "id":"x", "method":"input"
            }))
            .is_err()
        );
        assert!(
            pi_ui_request(&serde_json::json!({
                "type":"extension_ui_request", "id":"bad\nid", "method":"confirm"
            }))
            .is_err()
        );
    }

    #[test]
    fn tool_registry_executes_attributed_file_patch_and_fails_closed_for_unattached_browser() {
        let root = std::env::temp_dir().join(format!("glass-tools-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "before\n").unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let expected_revision = workspace.revision();
        let gateway = AgentToolGateway::default();
        let result = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "tool-1".into(),
                    name: "glass.file.patch".into(),
                    arguments: serde_json::json!({"path":"note.txt","search":"before","replace":"after","expectedRevision":expected_revision}),
                },
                &ToolAuthorization {
                    actor: Actor::embedded(),
                    allow_mutation: true,
                    confirmed: true,
                },
            )
            .unwrap();
        assert_eq!(result["replacements"], 1);
        assert_eq!(
            fs::read_to_string(root.join("note.txt")).unwrap(),
            "after\n"
        );
        let stale = ToolCall {
            id: "tool-stale".into(),
            name: "glass.file.patch".into(),
            arguments: serde_json::json!({
                "path":"note.txt", "search":"after", "replace":"stale",
                "expectedRevision":expected_revision
            }),
        };
        assert!(matches!(
            gateway.execute(
                &mut workspace,
                &stale,
                &ToolAuthorization {
                    actor: Actor::embedded(),
                    allow_mutation: true,
                    confirmed: true,
                },
            ),
            Err(DevelopmentError::Conflict(_))
        ));
        assert!(
            gateway
                .execute(
                    &mut workspace,
                    &ToolCall {
                        id: "tool-2".into(),
                        name: "glass.browser.observe".into(),
                        arguments: serde_json::json!({}),
                    },
                    &ToolAuthorization::read_only(Actor::embedded()),
                )
                .is_err()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_gateway_rejects_unconfirmed_mutation_and_unknown_arguments_without_audit_leaks() {
        let root = std::env::temp_dir().join(format!("glass-tool-policy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "private-before\n").unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let expected_revision = workspace.revision();
        let gateway = AgentToolGateway::default();
        let mutating = ToolCall {
            id: "denied".into(),
            name: "glass.file.patch".into(),
            arguments: serde_json::json!({
                "path":"note.txt",
                "search":"private-before",
                "replace":"private-after",
                "expectedRevision":expected_revision
            }),
        };
        assert!(
            gateway
                .execute(
                    &mut workspace,
                    &mutating,
                    &ToolAuthorization::read_only(Actor::embedded()),
                )
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(root.join("note.txt")).unwrap(),
            "private-before\n"
        );

        let invalid = ToolCall {
            id: "invalid".into(),
            name: "glass.file.list".into(),
            arguments: serde_json::json!({"unexpected":"private-value"}),
        };
        assert!(
            gateway
                .execute(
                    &mut workspace,
                    &invalid,
                    &ToolAuthorization::read_only(Actor::embedded()),
                )
                .is_err()
        );
        let audit =
            serde_json::to_string(&workspace.timeline().events().collect::<Vec<_>>()).unwrap();
        assert!(!audit.contains("private-before"));
        assert!(!audit.contains("private-after"));
        assert!(!audit.contains("private-value"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_gateway_bounds_envelopes_and_reuses_its_descriptor_catalog() {
        let root = std::env::temp_dir().join(format!("glass-tool-envelope-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let gateway = AgentToolGateway::default();
        let descriptor_storage = gateway.descriptors.as_ptr();

        for (id, name) in [("", "glass.file.list"), ("valid", "bad name")] {
            let error = gateway
                .execute(
                    &mut workspace,
                    &ToolCall {
                        id: id.into(),
                        name: name.into(),
                        arguments: serde_json::json!({}),
                    },
                    &ToolAuthorization::read_only(Actor::embedded()),
                )
                .unwrap_err();
            assert!(matches!(error, DevelopmentError::InvalidInput(_)));
        }
        let oversized_id = "x".repeat(MAX_TOOL_CALL_ID_BYTES + 1);
        assert!(
            gateway
                .execute(
                    &mut workspace,
                    &ToolCall {
                        id: oversized_id,
                        name: "glass.file.list".into(),
                        arguments: serde_json::json!({}),
                    },
                    &ToolAuthorization::read_only(Actor::embedded()),
                )
                .is_err()
        );
        assert_eq!(descriptor_storage, gateway.descriptors.as_ptr());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn attached_browser_enables_structured_observation_without_mutation_authority() {
        let root = std::env::temp_dir().join(format!("glass-tool-browser-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let mut gateway = AgentToolGateway::default();
        gateway.set_browser_context(Some(BrowserAgentContext {
            connected: true,
            target_id: Some("page-1".into()),
            origin: Some("https://example.test".into()),
            title: "Example".into(),
            browser_revision: 42,
            semantic_summary: "button Continue".into(),
            workflow_state: "idle".into(),
            memory_scope: "profile/default".into(),
        }));
        let descriptor = gateway
            .descriptors()
            .into_iter()
            .find(|item| item.name == "glass.browser.observe")
            .unwrap();
        assert!(descriptor.available);
        let result = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "observe-1".into(),
                    name: "glass.browser.observe".into(),
                    arguments: serde_json::json!({}),
                },
                &ToolAuthorization::read_only(Actor::embedded()),
            )
            .unwrap();
        assert_eq!(result["browserRevision"], 42);
        assert_eq!(result["structured"], true);
        assert!(
            !gateway
                .descriptors()
                .into_iter()
                .find(|item| item.name == "glass.browser.act")
                .unwrap()
                .available
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_argument_evidence_is_streamed_once_and_stays_canonical() {
        let registry = ToolRegistry;
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == "glass.file.read")
            .unwrap();
        let arguments = serde_json::json!({"path":"src/lib.rs"});
        let evidence = validate_tool_arguments(&descriptor, &arguments).unwrap();
        let canonical = serde_json::to_vec(&arguments).unwrap();
        assert_eq!(evidence.bytes, canonical.len());
        assert_eq!(evidence.sha256, digest_hex(&Sha256::digest(&canonical)));

        let oversized = serde_json::json!({"path":"x".repeat(MAX_TOOL_ARGUMENT_BYTES)});
        let error = validate_tool_arguments(&descriptor, &oversized)
            .err()
            .expect("oversized arguments must fail");
        assert!(error.to_string().contains("tool arguments exceed"));
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

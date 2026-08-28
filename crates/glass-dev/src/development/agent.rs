use super::{
    Actor, ActorKind, DevelopmentError, DevelopmentEventKind, DevelopmentResult, ProjectWorkspace,
};
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
const PI_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

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
    pub url: String,
    pub title: String,
    pub browser_revision: u64,
    pub semantic_summary: String,
    pub semantic_entity_count: usize,
    pub selected_entity: Option<Value>,
    pub workflow_state: String,
    pub input_owner: String,
    pub freshness: String,
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
            "@selection" => browser
                .and_then(|value| value.selected_entity.clone())
                .unwrap_or_else(|| serde_json::json!({"status": "not-selected"})),
            "@file" | "@symbol" => serde_json::json!({"status": "not-selected"}),
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
const MAX_GREP_FILES: usize = 512;
const MAX_GREP_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ToolAuthorization {
    pub actor: Actor,
    pub allow_mutation: bool,
    pub confirmed: bool,
    /// When true, agent file tools write through to disk. The default agent
    /// write path is a pending editor proposal.
    pub unrestricted: bool,
}

impl ToolAuthorization {
    pub fn read_only(actor: Actor) -> Self {
        Self {
            actor,
            allow_mutation: false,
            confirmed: false,
            unrestricted: false,
        }
    }

    pub fn agent_writes_to_disk(&self) -> bool {
        self.unrestricted
            || matches!(
                self.actor.kind,
                ActorKind::Human | ActorKind::System | ActorKind::Observer
            )
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
    pub fn subprocess_broker() -> Self {
        let mut gateway = Self::default();
        for descriptor in &mut gateway.descriptors {
            if matches!(
                descriptor.name.as_str(),
                "glass.process.start"
                    | "glass.process.stop"
                    | "glass.process.remove"
                    | "glass.process.logs"
                    | "glass.process.list"
            ) {
                descriptor.available = false;
                descriptor.unavailable_reason =
                    Some("Requires a resident Glass Dev session that owns the managed PTY".into());
            }
        }
        gateway
    }

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
        workspace.record_as(
            authorization.actor.clone(),
            DevelopmentEventKind::AgentToolCalled,
            tool_call_evidence(call, descriptor.mutating, &argument_evidence),
        )?;
        let result = if call.name == "glass.capabilities.inspect" {
            Ok(serde_json::to_value(&self.descriptors)?)
        } else {
            self.execute_attached(call).unwrap_or_else(|| {
                self.registry
                    .execute_unchecked(workspace, call, authorization)
            })
        };
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
                "url": context.url,
                "title": context.title,
                "browserRevision": context.browser_revision,
                "semanticSummary": context.semantic_summary,
                "semanticEntityCount": context.semantic_entity_count,
                "selectedEntity": context.selected_entity,
                "workflowState": context.workflow_state,
                "inputOwner": context.input_owner,
                "freshness": context.freshness,
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
                schema(
                    serde_json::json!({"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}}),
                    &["path"],
                ),
                false,
            ),
            descriptor(
                "glass.file.list",
                "List bounded project files",
                schema(
                    serde_json::json!({"path":{"type":"string"},"limit":{"type":"integer"}}),
                    &[],
                ),
                false,
            ),
            descriptor(
                "glass.file.search",
                "Search files, entities, processes, events, and commands",
                schema(serde_json::json!({"query":{"type":"string"}}), &["query"]),
                false,
            ),
            descriptor(
                "glass.file.grep",
                "Search bounded UTF-8 project files for a literal text pattern",
                schema(
                    serde_json::json!({"pattern":{"type":"string"},"path":{"type":"string"},"glob":{"type":"string"},"ignoreCase":{"type":"boolean"},"context":{"type":"integer"},"limit":{"type":"integer"}}),
                    &["pattern"],
                ),
                false,
            ),
            descriptor(
                "glass.file.find",
                "Find bounded project paths with shell-style star and question-mark matching",
                schema(
                    serde_json::json!({"pattern":{"type":"string"},"path":{"type":"string"},"limit":{"type":"integer"}}),
                    &["pattern"],
                ),
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
                "glass.file.edit",
                "Apply one atomic set of exact non-overlapping replacements",
                schema(
                    serde_json::json!({"path":{"type":"string"},"edits":{"type":"array"}}),
                    &["path", "edits"],
                ),
                true,
            ),
            descriptor(
                "glass.file.write",
                "Create or replace one bounded project file",
                schema(
                    serde_json::json!({"path":{"type":"string"},"content":{"type":"string"}}),
                    &["path", "content"],
                ),
                true,
            ),
            descriptor(
                "glass.file.mkdir",
                "Create one workspace-confined directory tree",
                schema(serde_json::json!({"path":{"type":"string"}}), &["path"]),
                true,
            ),
            descriptor(
                "glass.file.rename",
                "Rename one workspace-confined path",
                schema(
                    serde_json::json!({"from":{"type":"string"},"to":{"type":"string"}}),
                    &["from", "to"],
                ),
                true,
            ),
            descriptor(
                "glass.file.delete",
                "Delete one file or empty directory",
                schema(serde_json::json!({"path":{"type":"string"}}), &["path"]),
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
                "glass.process.remove",
                "Remove a stopped managed process",
                schema(serde_json::json!({"name":{"type":"string"}}), &["name"]),
                true,
            ),
            descriptor(
                "glass.neovim.probe",
                "Probe the local Neovim PTY and RPC capabilities",
                schema(serde_json::json!({}), &[]),
                false,
            ),
            descriptor(
                "glass.neovim.start",
                "Start Neovim under Glass process ownership",
                schema(
                    serde_json::json!({"name":{"type":"string"},"path":{"type":"string"}}),
                    &["name"],
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
                    serde_json::json!({"name":{"type":"string"},"command":{"type":"string"}}),
                    &["name", "command"],
                ),
                true,
            ),
            descriptor(
                "glass.command.run",
                "Run one bounded command to completion as the Pi actor",
                schema(
                    serde_json::json!({"name":{"type":"string"},"command":{"type":"string"},"timeoutSeconds":{"type":"integer"}}),
                    &["name", "command"],
                ),
                true,
            ),
            descriptor(
                "glass.diagnostics.run",
                "Publish bounded rust-analyzer diagnostics for one file",
                schema(serde_json::json!({"path":{"type":"string"}}), &["path"]),
                false,
            ),
            descriptor(
                "glass.runtime.inspect",
                "Inspect project, processes, actors, and diagnostics",
                schema(serde_json::json!({}), &[]),
                false,
            ),
            descriptor(
                "glass.capabilities.inspect",
                "Inspect the current Glass agent tool inventory and unavailable reasons",
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
        authorization: &ToolAuthorization,
    ) -> DevelopmentResult<Value> {
        let actor = authorization.actor.clone();
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
            "glass.file.read" => read_tool_file(workspace, call, string("path")?),
            "glass.file.list" => list_tool_files(workspace, call),
            "glass.file.search" => Ok(serde_json::to_value(
                workspace.search(string("query")?, 64)?,
            )?),
            "glass.file.grep" => grep_tool_files(workspace, call, string("pattern")?),
            "glass.file.find" => find_tool_files(workspace, call, string("pattern")?),
            "glass.file.patch" => {
                let path = string("path")?.to_string();
                let search = string("search")?.to_string();
                let replace = string("replace")?.to_string();
                if !authorization.agent_writes_to_disk() {
                    let original = workspace.buffer_or_file_content(&path)?;
                    let count = original.matches(&search).count();
                    if count == 0 {
                        return Err(DevelopmentError::Conflict(
                            "patch search text was not found".into(),
                        ));
                    }
                    let proposed = original.replace(&search, &replace);
                    return propose_agent_file(
                        workspace,
                        &path,
                        original,
                        proposed,
                        format!("patch {count} replacement(s) in {path}"),
                        actor,
                    );
                }
                let count = workspace.replace_in_buffer(&path, &search, &replace, actor)?;
                if count == 0 {
                    return Err(DevelopmentError::Conflict(
                        "patch search text was not found".into(),
                    ));
                }
                let buffer = workspace.save_buffer(&path)?;
                Ok(serde_json::json!({"path": path, "replacements": count, "dirty": buffer.dirty}))
            }
            "glass.file.edit" => {
                execute_atomic_edits(workspace, call, actor, authorization.agent_writes_to_disk())
            }
            "glass.file.write" => {
                let path = string("path")?.to_string();
                let content = string("content")?.to_string();
                if !authorization.agent_writes_to_disk() {
                    let original = workspace.buffer_or_file_content(&path).unwrap_or_default();
                    return propose_agent_file(
                        workspace,
                        &path,
                        original,
                        content,
                        format!("write {path}"),
                        actor,
                    );
                }
                workspace.write_file(&path, &content, actor)?;
                Ok(serde_json::json!({"path":path,"written":true}))
            }
            "glass.file.mkdir" => {
                let path = string("path")?.to_string();
                workspace.create_directory(&path, actor)?;
                Ok(serde_json::json!({"path":path,"created":true}))
            }
            "glass.file.rename" => {
                let from = string("from")?.to_string();
                let to = string("to")?.to_string();
                workspace.rename_path(&from, &to, actor)?;
                Ok(serde_json::json!({"from":from,"to":to,"renamed":true}))
            }
            "glass.neovim.probe" => Ok(serde_json::to_value(super::probe_neovim()?)?),
            "glass.neovim.start" => Ok(serde_json::to_value(super::start_neovim(
                workspace.processes(),
                string("name")?,
                call.arguments
                    .get("path")
                    .and_then(Value::as_str)
                    .map(std::path::Path::new),
            )?)?),
            "glass.file.delete" => {
                let path = string("path")?.to_string();
                workspace.delete_path(&path, actor)?;
                Ok(serde_json::json!({"path":path,"deleted":true}))
            }
            "glass.process.start" => Ok(serde_json::to_value(
                workspace.start_process(string("name")?, string("command")?)?,
            )?),
            "glass.process.stop" => Ok(serde_json::to_value(
                workspace.stop_process(string("name")?)?,
            )?),
            "glass.process.remove" => Ok(serde_json::to_value(
                workspace.processes().remove(string("name")?)?,
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
            "glass.command.run" => {
                let timeout = call
                    .arguments
                    .get("timeoutSeconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(120);
                if !(1..=300).contains(&timeout) {
                    return Err(DevelopmentError::InvalidInput(
                        "glass.command.run timeoutSeconds must be between 1 and 300".into(),
                    ));
                }
                Ok(serde_json::to_value(workspace.run_command_to_completion(
                    string("name")?,
                    string("command")?,
                    Duration::from_secs(timeout),
                )?)?)
            }
            "glass.diagnostics.run" => Ok(serde_json::to_value(
                workspace.publish_rust_diagnostics(string("path")?)?,
            )?),
            "glass.runtime.inspect" => Ok(serde_json::json!({
                "project": workspace.detection(),
                "processes": workspace.processes().list_checked()?,
                "actors": workspace.actors().collect::<Vec<_>>(),
                "diagnostics": workspace.diagnostics(),
                "revision": workspace.revision()
            })),
            "glass.web_ir.inspect" => {
                let ir: glass_browser::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["ir"].clone())?;
                ir.validate().map_err(|error| {
                    DevelopmentError::InvalidInput(format!("invalid Web IR: {error}"))
                })?;
                Ok(serde_json::to_value(
                    glass_browser::protocol::WebIrInspectionResult::from_ir(&ir),
                )?)
            }
            "glass.web_ir.diff" => {
                let before: glass_browser::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["before"].clone())?;
                let after: glass_browser::web_ir::GlassWebIrV1 =
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
                let before: glass_browser::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["before"].clone())?;
                let after: glass_browser::web_ir::GlassWebIrV1 =
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
                let task: glass_browser::task_protocol::GlassTask =
                    serde_json::from_value(call.arguments["task"].clone())?;
                let ir: glass_browser::web_ir::GlassWebIrV1 =
                    serde_json::from_value(call.arguments["ir"].clone())?;
                let plan =
                    glass_browser::task_compiler::compile_task(&task, &ir).map_err(|error| {
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

fn read_tool_file(
    workspace: &mut ProjectWorkspace,
    call: &ToolCall,
    path: &str,
) -> DevelopmentResult<Value> {
    let content = workspace.read_file(path)?;
    let content_sha256 = digest_hex(&Sha256::digest(content.as_bytes()));
    let offset = call
        .arguments
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let limit = call
        .arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(500);
    if offset == 0 || !(1..=2_000).contains(&limit) {
        return Err(DevelopmentError::InvalidInput(
            "glass.file.read offset must be positive and limit must be between 1 and 2000".into(),
        ));
    }
    let total_lines = content
        .lines()
        .count()
        .max(usize::from(!content.is_empty())) as u64;
    let mut selected = content
        .split_inclusive('\n')
        .skip(offset.saturating_sub(1) as usize)
        .take(limit as usize)
        .collect::<String>();
    let selected_lines = selected.lines().count() as u64;
    let mut bytes_truncated = false;
    if selected.len() > 60 * 1024 {
        let mut boundary = 60 * 1024;
        while !selected.is_char_boundary(boundary) {
            boundary -= 1;
        }
        selected.truncate(boundary);
        bytes_truncated = true;
    }
    Ok(serde_json::json!({
        "path": path,
        "content": selected,
        "offset": offset,
        "lines": selected_lines,
        "totalLines": total_lines,
        "sha256": content_sha256,
        "truncated": bytes_truncated || offset.saturating_sub(1) + selected_lines < total_lines,
    }))
}

fn list_tool_files(workspace: &ProjectWorkspace, call: &ToolCall) -> DevelopmentResult<Value> {
    let path_prefix = optional_string_argument(call, "path")
        .map(|path| path.trim_matches('/').to_string())
        .unwrap_or_default();
    let limit = bounded_u64_argument(call, "limit", 1, 2_000, 500)? as usize;
    let tree = workspace.list_files_result()?;
    let mut entries = Vec::new();
    let mut truncated = tree.truncated;
    for entry in tree.entries {
        if !path_prefix.is_empty()
            && entry.path != path_prefix
            && !entry.path.starts_with(&format!("{path_prefix}/"))
        {
            continue;
        }
        entries.push(entry);
        if entries.len() == limit {
            truncated = true;
            break;
        }
    }
    Ok(serde_json::json!({
        "entries": entries,
        "limit": limit,
        "truncated": truncated,
        "ignoredDirectories": tree.ignored_directories,
        "skippedSymlinks": tree.skipped_symlinks,
    }))
}

fn grep_tool_files(
    workspace: &mut ProjectWorkspace,
    call: &ToolCall,
    pattern: &str,
) -> DevelopmentResult<Value> {
    if pattern.is_empty() {
        return Err(DevelopmentError::InvalidInput(
            "glass.file.grep pattern must not be empty".into(),
        ));
    }
    let path_prefix = optional_string_argument(call, "path")
        .map(|path| path.trim_matches('/').to_string())
        .unwrap_or_default();
    let glob = optional_string_argument(call, "glob");
    let ignore_case = call
        .arguments
        .get("ignoreCase")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let context = bounded_u64_argument(call, "context", 0, 10, 0)? as usize;
    let limit = bounded_u64_argument(call, "limit", 1, 500, 100)? as usize;
    let needle = ignore_case.then(|| pattern.to_lowercase());
    let entries = workspace.list_files_result()?;
    let mut matches = Vec::new();
    let mut files_searched = 0usize;
    let mut bytes_searched = 0u64;
    let mut files_skipped = 0usize;
    let mut truncated = entries.truncated;
    for entry in entries.entries {
        if entry.kind != super::project::FileKind::File
            || (!path_prefix.is_empty()
                && entry.path != path_prefix
                && !entry.path.starts_with(&format!("{path_prefix}/")))
            || glob.is_some_and(|glob| !wildcard_matches(glob, &entry.path))
        {
            continue;
        }
        let bytes = entry.bytes.unwrap_or(0);
        if files_searched == MAX_GREP_FILES || bytes_searched.saturating_add(bytes) > MAX_GREP_BYTES
        {
            truncated = true;
            break;
        }
        let Ok(content) = workspace.read_file_snapshot(&entry.path) else {
            files_skipped += 1;
            continue;
        };
        files_searched += 1;
        bytes_searched = bytes_searched.saturating_add(content.len() as u64);
        let lines = content.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let found = needle.as_ref().map_or_else(
                || line.contains(pattern),
                |needle| line.to_lowercase().contains(needle),
            );
            if !found {
                continue;
            }
            let start = index.saturating_sub(context);
            let end = (index + context + 1).min(lines.len());
            matches.push(serde_json::json!({
                "path": entry.path,
                "line": index + 1,
                "text": line,
                "contextBefore": lines[start..index],
                "contextAfter": lines[index + 1..end],
            }));
            if matches.len() == limit {
                return Ok(serde_json::json!({
                    "matches": matches,
                    "filesSearched": files_searched,
                    "bytesSearched": bytes_searched,
                    "filesSkipped": files_skipped,
                    "limit": limit,
                    "truncated": true,
                    "literal": true,
                }));
            }
        }
    }
    Ok(serde_json::json!({
        "matches": matches,
        "filesSearched": files_searched,
        "bytesSearched": bytes_searched,
        "filesSkipped": files_skipped,
        "limit": limit,
        "truncated": truncated,
        "literal": true,
    }))
}

fn find_tool_files(
    workspace: &ProjectWorkspace,
    call: &ToolCall,
    pattern: &str,
) -> DevelopmentResult<Value> {
    if pattern.is_empty() {
        return Err(DevelopmentError::InvalidInput(
            "glass.file.find pattern must not be empty".into(),
        ));
    }
    let path_prefix = optional_string_argument(call, "path")
        .map(|path| path.trim_matches('/').to_string())
        .unwrap_or_default();
    let limit = bounded_u64_argument(call, "limit", 1, 2_000, 500)? as usize;
    let tree = workspace.list_files_result()?;
    let mut paths = Vec::new();
    let mut truncated = tree.truncated;
    for entry in tree.entries {
        if (!path_prefix.is_empty()
            && entry.path != path_prefix
            && !entry.path.starts_with(&format!("{path_prefix}/")))
            || !wildcard_matches(pattern, &entry.path)
        {
            continue;
        }
        paths.push(serde_json::json!({"path":entry.path,"kind":entry.kind}));
        if paths.len() == limit {
            truncated = true;
            break;
        }
    }
    Ok(serde_json::json!({"paths":paths,"limit":limit,"truncated":truncated}))
}

fn optional_string_argument<'a>(call: &'a ToolCall, name: &str) -> Option<&'a str> {
    call.arguments.get(name).and_then(Value::as_str)
}

fn bounded_u64_argument(
    call: &ToolCall,
    name: &str,
    minimum: u64,
    maximum: u64,
    default: u64,
) -> DevelopmentResult<u64> {
    let value = call
        .arguments
        .get(name)
        .and_then(Value::as_u64)
        .unwrap_or(default);
    if !(minimum..=maximum).contains(&value) {
        return Err(DevelopmentError::InvalidInput(format!(
            "{} {name} must be between {minimum} and {maximum}",
            call.name
        )));
    }
    Ok(value)
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let value = value.chars().collect::<Vec<_>>();
    let (mut pattern_index, mut value_index) = (0usize, 0usize);
    let (mut star, mut retry) = (None, 0usize);
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry += 1;
            value_index = retry;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn propose_agent_file(
    workspace: &mut ProjectWorkspace,
    path: &str,
    original: String,
    proposed: String,
    summary: String,
    actor: Actor,
) -> DevelopmentResult<Value> {
    let _ = workspace.create_editor_checkpoint(format!("before-proposal:{path}"), actor.clone());
    if workspace.buffer(path).is_none() && workspace.open_buffer(path, actor.clone()).is_err() {
        workspace.edit_buffer(path, original.clone(), actor.clone())?;
    }
    let proposal = workspace.propose_editor_change(path, original, proposed, summary, actor)?;
    Ok(serde_json::json!({
        "path": path,
        "written": false,
        "proposed": true,
        "proposalId": proposal.id,
        "state": "pending",
    }))
}

fn execute_atomic_edits(
    workspace: &mut ProjectWorkspace,
    call: &ToolCall,
    actor: Actor,
    write_through: bool,
) -> DevelopmentResult<Value> {
    let path = call.arguments["path"]
        .as_str()
        .ok_or_else(|| DevelopmentError::InvalidInput("glass.file.edit requires path".into()))?;
    let edits = call.arguments["edits"]
        .as_array()
        .filter(|edits| !edits.is_empty() && edits.len() <= 64)
        .ok_or_else(|| {
            DevelopmentError::InvalidInput("glass.file.edit requires between 1 and 64 edits".into())
        })?;
    let original = workspace.read_file(path)?;
    let mut replacements = Vec::with_capacity(edits.len());
    for edit in edits {
        let old_text = edit.get("oldText").and_then(Value::as_str).ok_or_else(|| {
            DevelopmentError::InvalidInput("every edit requires string oldText".into())
        })?;
        let new_text = edit.get("newText").and_then(Value::as_str).ok_or_else(|| {
            DevelopmentError::InvalidInput("every edit requires string newText".into())
        })?;
        if old_text.is_empty() {
            return Err(DevelopmentError::InvalidInput(
                "glass.file.edit oldText must not be empty".into(),
            ));
        }
        let matches = original.match_indices(old_text).collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(DevelopmentError::Conflict(format!(
                "glass.file.edit oldText must match exactly once; matched {} times",
                matches.len()
            )));
        }
        let start = matches[0].0;
        replacements.push((start, start + old_text.len(), new_text));
    }
    replacements.sort_by_key(|replacement| replacement.0);
    if replacements.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(DevelopmentError::Conflict(
            "glass.file.edit replacements overlap".into(),
        ));
    }
    let mut content = original.clone();
    for (start, end, new_text) in replacements.iter().rev() {
        content.replace_range(*start..*end, new_text);
    }
    if !write_through {
        return propose_agent_file(
            workspace,
            path,
            original,
            content,
            format!("edit {} replacement(s) in {path}", replacements.len()),
            actor,
        );
    }
    workspace.edit_buffer(path, content, actor)?;
    let buffer = workspace.save_buffer(path)?;
    Ok(serde_json::json!({
        "path": path,
        "replacements": replacements.len(),
        "dirty": buffer.dirty,
    }))
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
    Compact { instructions: Option<String> },
    CloneSession,
    Fork { entry_id: String },
    SwitchSession { path: String },
    Entries { since: Option<String> },
    Messages,
    SessionStats,
    SetSessionName { name: String },
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
            HarnessRequest::Compact { .. }
            | HarnessRequest::CloneSession
            | HarnessRequest::Fork { .. }
            | HarnessRequest::SwitchSession { .. }
            | HarnessRequest::Entries { .. }
            | HarnessRequest::Messages
            | HarnessRequest::SessionStats
            | HarnessRequest::SetSessionName { .. } => Ok(vec![HarnessEvent::Error {
                message: "persistent session controls require the Pi runtime".into(),
            }]),
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
    unrestricted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PiHarnessOptions {
    pub unrestricted: bool,
    pub persist_session: bool,
    pub session_id: Option<String>,
    pub fork: Option<String>,
    pub session_dir: Option<PathBuf>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub broker_socket: Option<PathBuf>,
    pub broker_token: Option<String>,
    pub broker_workspace_id: Option<String>,
    pub additional_system_prompt: Option<String>,
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
        Self::spawn_with_unrestricted(root, false)
    }

    pub fn spawn_with_unrestricted(root: &Path, unrestricted: bool) -> DevelopmentResult<Self> {
        Self::spawn_with_options(
            root,
            PiHarnessOptions {
                unrestricted,
                persist_session: env_enabled("GLASS_PI_PERSIST_SESSION"),
                ..PiHarnessOptions::default()
            },
        )
    }

    pub fn spawn_with_options(root: &Path, options: PiHarnessOptions) -> DevelopmentResult<Self> {
        validate_pi_harness_options(&options)?;
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
        let trusted_resources = options.unrestricted || env_enabled("GLASS_PI_TRUSTED_RESOURCES");
        let mut system_prompt = if options.unrestricted {
            format!(
                "{}\n- UNRESTRICTED MODE is active. Glass will not ask for tool approval; execute requested local development work directly and report effects truthfully.",
                include_str!("../../assets/pi-glass-system.md")
            )
        } else {
            include_str!("../../assets/pi-glass-system.md").to_string()
        };
        if let Some(additional) = &options.additional_system_prompt {
            if additional.len() > 128 * 1024 || additional.contains('\0') {
                return Err(DevelopmentError::InvalidInput(
                    "additional Pi system prompt exceeds 128 KiB or contains NUL".into(),
                ));
            }
            system_prompt.push_str(additional);
        }
        let mut command = Command::new("pi");
        command.args(["--mode", "rpc", "--no-approve", "--no-builtin-tools"]);
        if !env_enabled("GLASS_PI_ONLINE_CATALOG") {
            command.arg("--offline");
        }
        if !trusted_resources {
            command.args([
                "--no-context-files",
                "--no-extensions",
                "--no-skills",
                "--no-prompt-templates",
                "--no-themes",
            ]);
        }
        if !options.persist_session {
            command.arg("--no-session");
        }
        if let Some(session_id) = &options.session_id {
            command.args(["--session-id", session_id]);
        }
        if let Some(fork) = &options.fork {
            command.args(["--fork", fork]);
        }
        if let Some(session_dir) = &options.session_dir {
            std::fs::create_dir_all(session_dir)?;
            command.arg("--session-dir").arg(session_dir);
        }
        if let Some(name) = &options.name {
            command.args(["--name", name]);
        }
        if let Some(model) = &options.model {
            command.args(["--model", model]);
        }
        if let Some(thinking) = &options.thinking {
            command.args(["--thinking", thinking]);
        }
        if let Some(socket) = &options.broker_socket {
            command.env("GLASS_DEV_DAEMON_SOCKET", socket);
        }
        if let Some(token) = &options.broker_token {
            command.env("GLASS_DEV_DAEMON_TOKEN", token);
        }
        if let Some(workspace_id) = &options.broker_workspace_id {
            command.env("GLASS_DEV_DAEMON_WORKSPACE", workspace_id);
        }
        command
            .args(["--system-prompt", &system_prompt, "--extension"])
            .arg(&extension_path);
        if !trusted_resources {
            command.arg("--tools").arg(
                [
                    "read",
                    "write",
                    "edit",
                    "bash",
                    "grep",
                    "find",
                    "ls",
                    "glass_git_status",
                    "glass_semantic_inspect",
                    "glass_web_ir_inspect",
                    "glass_web_ir_diff",
                    "glass_web_ir_continuity",
                    "glass_task_plan",
                    "glass_runtime_inspect",
                    "glass_capabilities",
                    "glass_diagnostics_run",
                    "glass_file_mkdir",
                    "glass_file_rename",
                    "glass_file_delete",
                    "glass_test_run",
                    "glass_process_list",
                    "glass_process_start",
                    "glass_process_stop",
                    "glass_process_logs",
                    "glass_process_restart",
                    "glass_process_input",
                    "glass_process_resize",
                    "glass_process_health",
                    "glass_process_ports",
                    "glass_editor_open",
                    "glass_editor_selection",
                    "glass_editor_replace",
                    "glass_editor_replace_selection",
                    "glass_editor_save",
                    "glass_editor_diff",
                    "glass_editor_buffers",
                    "glass_browser_state",
                    "glass_browser_start",
                    "glass_browser_stop",
                    "glass_browser_observe",
                    "glass_browser_targets",
                    "glass_browser_select_target",
                    "glass_browser_navigate",
                    "glass_browser_act",
                    "glass_browser_screenshot",
                    "glass_workflow_run",
                    "glass_workflow_pause",
                    "glass_workflow_resume",
                    "glass_git_diff",
                    "glass_git_stage",
                    "glass_git_unstage",
                    "glass_git_commit",
                    "glass_git_branches",
                    "glass_git_branch_create",
                    "glass_git_branch_switch",
                    "glass_git_blame",
                    "glass_git_worktrees",
                    "glass_git_worktree_create",
                    "glass_git_worktree_remove",
                    "glass_test_discover",
                    "glass_test_run_suite",
                    "glass_test_results",
                    "glass_test_cancel",
                    "glass_test_watch",
                    "glass_eval_start",
                    "glass_eval_execute",
                    "glass_eval_list",
                    "glass_eval_reset",
                    "glass_eval_stop",
                    "glass_lsp_diagnostics",
                    "glass_lsp_hover",
                    "glass_lsp_completion",
                    "glass_lsp_definition",
                    "glass_lsp_declaration",
                    "glass_lsp_implementation",
                    "glass_lsp_references",
                    "glass_lsp_document_symbols",
                    "glass_lsp_workspace_symbols",
                    "glass_lsp_signature_help",
                    "glass_lsp_code_actions",
                    "glass_lsp_formatting",
                    "glass_lsp_range_formatting",
                    "glass_lsp_semantic_tokens",
                    "glass_lsp_rename",
                    "glass_debug_start",
                    "glass_debug_launch",
                    "glass_debug_attach",
                    "glass_debug_breakpoint_set",
                    "glass_debug_continue",
                    "glass_debug_pause",
                    "glass_debug_step",
                    "glass_debug_stack",
                    "glass_debug_scopes",
                    "glass_debug_variables",
                    "glass_debug_evaluate",
                    "glass_debug_events",
                    "glass_debug_stop",
                    "glass_agent_list",
                    "glass_agent_spawn",
                    "glass_agent_prompt",
                    "glass_agent_steer",
                    "glass_agent_follow_up",
                    "glass_agent_abort",
                    "glass_agent_compact",
                    "glass_agent_model",
                    "glass_agent_thinking",
                    "glass_agent_new_session",
                    "glass_agent_clone_session",
                    "glass_agent_fork",
                    "glass_agent_switch_session",
                    "glass_agent_messages",
                    "glass_agent_entries",
                    "glass_agent_stats",
                    "glass_agent_name",
                    "glass_graph_query",
                    "glass_graph_path",
                    "glass_replay_list",
                    "glass_replay_diff",
                ]
                .join(","),
            );
        }
        let mut child = command
            .env("GLASS_PI_BROKER_BIN", broker)
            .env(
                "GLASS_PI_YOLO",
                if options.unrestricted { "1" } else { "0" },
            )
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
            unrestricted: options.unrestricted,
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
                    PI_RESPONSE_TIMEOUT
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
                if !self.unrestricted {
                    // A one-shot/non-interactive caller has nowhere safe to ask a
                    // human. Deny immediately instead of hanging or inheriting
                    // ambient authority.
                    self.respond_extension_ui(&request.id, false)?;
                }
                continue;
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

    pub fn start_request(&mut self, request: HarnessRequest) -> DevelopmentResult<String> {
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
            HarnessRequest::Compact { instructions } => (
                serde_json::json!({"type": "compact", "customInstructions": instructions}),
                None,
            ),
            HarnessRequest::CloneSession => (serde_json::json!({"type": "clone"}), None),
            HarnessRequest::Fork { entry_id } => (
                serde_json::json!({"type": "fork", "entryId": entry_id}),
                None,
            ),
            HarnessRequest::SwitchSession { path } => (
                serde_json::json!({"type": "switch_session", "sessionPath": path}),
                None,
            ),
            HarnessRequest::Entries { since } => (
                serde_json::json!({"type": "get_entries", "since": since}),
                None,
            ),
            HarnessRequest::Messages => (serde_json::json!({"type": "get_messages"}), None),
            HarnessRequest::SessionStats => {
                (serde_json::json!({"type": "get_session_stats"}), None)
            }
            HarnessRequest::SetSessionName { name } => (
                serde_json::json!({"type": "set_session_name", "name": name}),
                None,
            ),
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

    pub fn respond_extension_ui(&mut self, id: &str, confirmed: bool) -> DevelopmentResult<()> {
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

    pub fn recv_event_timeout(&mut self, timeout: Duration) -> DevelopmentResult<Option<Value>> {
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
                if self.unrestricted
                    && let Some(request) = pi_ui_request(&value)?
                {
                    self.respond_extension_ui(&request.id, true)?;
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

fn validate_pi_harness_options(options: &PiHarnessOptions) -> DevelopmentResult<()> {
    let broker_fields = [
        options.broker_socket.is_some(),
        options.broker_token.is_some(),
        options.broker_workspace_id.is_some(),
    ];
    if broker_fields.iter().any(|present| *present) && !broker_fields.iter().all(|present| *present)
    {
        return Err(DevelopmentError::InvalidInput(
            "Pi resident broker socket, token, and workspace must be configured together".into(),
        ));
    }
    if let Some(socket) = &options.broker_socket
        && (!socket.is_absolute() || socket == Path::new("/"))
    {
        return Err(DevelopmentError::InvalidInput(
            "Pi resident broker socket must be an explicit absolute path".into(),
        ));
    }
    for (description, value, limit) in [
        ("broker token", options.broker_token.as_deref(), 256),
        (
            "broker workspace",
            options.broker_workspace_id.as_deref(),
            128,
        ),
    ] {
        if let Some(value) = value
            && (value.is_empty() || value.len() > limit || value.chars().any(char::is_control))
        {
            return Err(DevelopmentError::InvalidInput(format!(
                "Pi {description} must contain 1..={limit} non-control bytes"
            )));
        }
    }
    if options.session_id.is_some() && options.fork.is_some() {
        return Err(DevelopmentError::InvalidInput(
            "Pi session ID and fork source are mutually exclusive".into(),
        ));
    }
    if (options.session_id.is_some()
        || options.fork.is_some()
        || options.session_dir.is_some()
        || options.name.is_some())
        && !options.persist_session
    {
        return Err(DevelopmentError::InvalidInput(
            "Pi session options require persistent session storage".into(),
        ));
    }
    for (description, value, limit) in [
        ("session ID", options.session_id.as_deref(), 256),
        ("fork source", options.fork.as_deref(), 4096),
        ("session name", options.name.as_deref(), 256),
        ("model", options.model.as_deref(), 512),
    ] {
        if let Some(value) = value
            && (value.is_empty()
                || value.len() > limit
                || value.chars().any(|character| character.is_control()))
        {
            return Err(DevelopmentError::InvalidInput(format!(
                "Pi {description} must contain 1..={limit} non-control bytes"
            )));
        }
    }
    if let Some(thinking) = options.thinking.as_deref()
        && !matches!(
            thinking,
            "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        )
    {
        return Err(DevelopmentError::InvalidInput(
            "Pi thinking level must be off, minimal, low, medium, high, xhigh, or max".into(),
        ));
    }
    if let Some(directory) = &options.session_dir
        && (!directory.is_absolute() || directory == Path::new("/"))
    {
        return Err(DevelopmentError::InvalidInput(
            "Pi session directory must be an explicit absolute path".into(),
        ));
    }
    Ok(())
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
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

pub fn pi_event_display(value: &Value) -> Option<String> {
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
    fn pi_session_options_require_safe_persistent_configuration() {
        let directory = std::env::temp_dir().join("glass-pi-sessions");
        assert!(
            validate_pi_harness_options(&PiHarnessOptions {
                persist_session: true,
                session_id: Some("session-1".into()),
                session_dir: Some(directory),
                thinking: Some("high".into()),
                ..PiHarnessOptions::default()
            })
            .is_ok()
        );
        assert!(
            validate_pi_harness_options(&PiHarnessOptions {
                session_id: Some("session-1".into()),
                ..PiHarnessOptions::default()
            })
            .is_err()
        );
        assert!(
            validate_pi_harness_options(&PiHarnessOptions {
                persist_session: true,
                thinking: Some("unbounded".into()),
                ..PiHarnessOptions::default()
            })
            .is_err()
        );
    }

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
        assert!(prompt.contains("glass.editor.proposal.create"));
        assert!(prompt.contains("stale proposal"));
        assert!(prompt.contains("queued, running, indeterminate, stale, or background"));
        assert!(prompt.contains("300 seconds"));
        for tool in [
            "glass_git_status",
            "glass_semantic_inspect",
            "glass_web_ir_inspect",
            "glass_web_ir_diff",
            "glass_web_ir_continuity",
            "glass_task_plan",
            "glass_runtime_inspect",
            "glass_capabilities",
            "glass_diagnostics_run",
            "glass_file_mkdir",
            "glass_file_rename",
            "glass_process_start",
            "glass_process_stop",
            "glass_editor_comments",
            "glass_editor_proposal_create",
            "glass_editor_proposal_accept",
            "glass_editor_checkpoint_create",
            "\"read\"",
            "\"write\"",
            "\"edit\"",
            "\"bash\"",
            "\"grep\"",
            "\"find\"",
            "\"ls\"",
        ] {
            assert!(tools.contains(tool), "missing Pi tool {tool}");
        }
        assert!(tools.contains("ctx.ui.confirm"));
        assert!(tools.contains("GLASS_PI_YOLO"));
        assert!(tools.contains("mutating && !unrestricted"));
        assert!(tools.contains("--allow-mutation"));
        assert!(tools.contains("exact serialized call once"));
        assert!(tools.contains("register(\"glass_process_start\", \"glass.process.start\""));
        assert!(tools.contains("register(\"glass_process_stop\", \"glass.process.stop\""));

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
        let gateway = AgentToolGateway::default();
        let result = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "tool-1".into(),
                    name: "glass.file.patch".into(),
                    arguments: serde_json::json!({"path":"note.txt","search":"before","replace":"after"}),
                },
                &ToolAuthorization {
                    actor: Actor::embedded(),
                    allow_mutation: true,
                    confirmed: true,
                    unrestricted: true,
                },
            )
            .unwrap();
        assert_eq!(result["replacements"], 1);
        assert_eq!(
            fs::read_to_string(root.join("note.txt")).unwrap(),
            "after\n"
        );
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
        let gateway = AgentToolGateway::default();
        let mutating = ToolCall {
            id: "denied".into(),
            name: "glass.file.patch".into(),
            arguments: serde_json::json!({
                "path":"note.txt",
                "search":"private-before",
                "replace":"private-after"
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
    fn confirmed_agent_file_write_creates_a_proposal_unless_unrestricted() {
        let root =
            std::env::temp_dir().join(format!("glass-proposal-write-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("note.txt"), "before\n").unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let gateway = AgentToolGateway::default();
        let confirmed = ToolAuthorization {
            actor: Actor::embedded(),
            allow_mutation: true,
            confirmed: true,
            unrestricted: false,
        };
        let result = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "propose-1".into(),
                    name: "glass.file.write".into(),
                    arguments: serde_json::json!({
                        "path":"note.txt",
                        "content":"after\n"
                    }),
                },
                &confirmed,
            )
            .unwrap();
        assert_eq!(result["proposed"], true);
        assert_eq!(result["written"], false);
        assert_eq!(
            fs::read_to_string(root.join("note.txt")).unwrap(),
            "before\n"
        );
        let proposal = workspace
            .editor_proposals()
            .into_iter()
            .find(|item| item.path == "note.txt")
            .expect("proposal");
        assert_eq!(proposal.original, "before\n");
        assert_eq!(proposal.proposed, "after\n");
        workspace
            .accept_editor_proposal(&proposal.id, Actor::local())
            .unwrap();
        assert_eq!(workspace.buffer("note.txt").unwrap().content, "after\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn coding_gateway_writes_and_applies_atomic_multi_edits() {
        let root = std::env::temp_dir().join(format!("glass-coding-tools-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let gateway = AgentToolGateway::default();
        let authority = ToolAuthorization {
            actor: Actor::external("pi"),
            allow_mutation: true,
            confirmed: true,
            unrestricted: true,
        };
        gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "write-1".into(),
                    name: "glass.file.write".into(),
                    arguments: serde_json::json!({
                        "path":"src/example.rs",
                        "content":"fn alpha() {}\nfn beta() {}\n"
                    }),
                },
                &authority,
            )
            .unwrap();
        let result = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "edit-1".into(),
                    name: "glass.file.edit".into(),
                    arguments: serde_json::json!({
                        "path":"src/example.rs",
                        "edits":[
                            {"oldText":"alpha", "newText":"first"},
                            {"oldText":"beta", "newText":"second"}
                        ]
                    }),
                },
                &authority,
            )
            .unwrap();
        assert_eq!(result["replacements"], 2);
        assert_eq!(
            fs::read_to_string(root.join("src/example.rs")).unwrap(),
            "fn first() {}\nfn second() {}\n"
        );
        let read = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "read-page".into(),
                    name: "glass.file.read".into(),
                    arguments: serde_json::json!({"path":"src/example.rs","offset":2,"limit":1}),
                },
                &ToolAuthorization::read_only(Actor::external("pi")),
            )
            .unwrap();
        assert_eq!(read["content"], "fn second() {}\n");
        assert_eq!(read["offset"], 2);
        assert!(
            read["sha256"]
                .as_str()
                .is_some_and(|value| value.len() == 64)
        );

        {
            let command = "printf command-private-value";
            let command_result = gateway
                .execute(
                    &mut workspace,
                    &ToolCall {
                        id: "command-1".into(),
                        name: "glass.command.run".into(),
                        arguments: serde_json::json!({
                            "name":"pi-command-test", "command":command, "timeoutSeconds":5
                        }),
                    },
                    &authority,
                )
                .unwrap();
            assert!(
                command_result["output"]
                    .as_str()
                    .is_some_and(|value| value.contains("command-private-value"))
            );
            assert!(
                workspace
                    .timeline()
                    .events()
                    .all(|event| !event.payload.to_string().contains(command))
            );
            assert!(workspace.timeline().events().all(|event| !matches!(
                event.kind,
                DevelopmentEventKind::TestStarted | DevelopmentEventKind::TestCompleted
            )));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn coding_gateway_lists_finds_and_greps_with_bounded_standard_semantics() {
        let root = std::env::temp_dir().join(format!("glass-coding-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/nested")).unwrap();
        fs::write(root.join("src/lib.rs"), "alpha\nNeedle target\nomega\n").unwrap();
        fs::write(root.join("src/nested/mod.rs"), "needle second\n").unwrap();
        fs::write(root.join("README.md"), "Needle outside\n").unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let gateway = AgentToolGateway::default();
        let authority = ToolAuthorization::read_only(Actor::external("pi"));

        let listed = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "list-src".into(),
                    name: "glass.file.list".into(),
                    arguments: serde_json::json!({"path":"src","limit":2}),
                },
                &authority,
            )
            .unwrap();
        assert_eq!(listed["entries"].as_array().unwrap().len(), 2);
        assert_eq!(listed["truncated"], true);
        assert!(
            listed["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["path"].as_str().unwrap().starts_with("src"))
        );

        let found = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "find-rs".into(),
                    name: "glass.file.find".into(),
                    arguments: serde_json::json!({"pattern":"src/*.rs","limit":20}),
                },
                &authority,
            )
            .unwrap();
        assert_eq!(found["paths"].as_array().unwrap().len(), 2);

        let grep = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "grep-needle".into(),
                    name: "glass.file.grep".into(),
                    arguments: serde_json::json!({
                        "pattern":"needle", "path":"src", "glob":"*.rs",
                        "ignoreCase":true, "context":1, "limit":10
                    }),
                },
                &authority,
            )
            .unwrap();
        assert_eq!(grep["matches"].as_array().unwrap().len(), 2);
        assert_eq!(grep["literal"], true);
        assert_eq!(grep["matches"][0]["contextBefore"][0], "alpha");
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
    fn subprocess_broker_reports_resident_process_tools_as_unavailable() {
        let gateway = AgentToolGateway::subprocess_broker();
        for name in [
            "glass.process.start",
            "glass.process.stop",
            "glass.process.logs",
            "glass.process.list",
        ] {
            let descriptor = gateway
                .descriptors()
                .into_iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap();
            assert!(!descriptor.available);
            assert!(
                descriptor
                    .unavailable_reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("resident Glass Dev"))
            );
        }
        let root = std::env::temp_dir().join(format!("glass-capabilities-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut workspace = ProjectWorkspace::open(&root).unwrap();
        let result = gateway
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "capabilities".into(),
                    name: "glass.capabilities.inspect".into(),
                    arguments: serde_json::json!({}),
                },
                &ToolAuthorization::read_only(Actor::external("pi")),
            )
            .unwrap();
        let processes = result
            .as_array()
            .unwrap()
            .iter()
            .filter(|descriptor| {
                descriptor["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("glass.process."))
            })
            .collect::<Vec<_>>();
        assert_eq!(processes.len(), 5);
        assert!(
            processes
                .iter()
                .all(|descriptor| descriptor["available"] == false)
        );
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
            url: "https://example.test/".into(),
            title: "Example".into(),
            browser_revision: 42,
            semantic_summary: "button Continue".into(),
            semantic_entity_count: 1,
            selected_entity: Some(serde_json::json!({"reference":"e1","name":"Continue"})),
            workflow_state: "idle".into(),
            input_owner: "Glass".into(),
            freshness: "current".into(),
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

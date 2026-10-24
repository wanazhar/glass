//! Governed tool routing into one resident development workspace.

use crate::agents::AgentSpec;
use crate::browser::BrowserStartConfig;
use crate::debugger::{DebugAdapterConfig, SourceBreakpoint};
use crate::development::{
    AgentToolGateway, DevelopmentError, DevelopmentResult, ToolAuthorization, ToolCall,
    ToolDescriptor,
};
use crate::kernels::KernelKind;
use crate::resident_harness::ResidentSessionRequest;
use crate::tasks::{TaskId, TaskSpec};
use crate::workspace::DevelopmentWorkspace;
use crate::{DevelopmentNode, DevelopmentNodeKind, ObservableEventInput};
use glass_browser::browser::session::{
    SemanticObservationLevel, VerificationPredicate, WorkflowRecordingSession,
    record_semantic_events,
};
use glass_browser::workspace::{WorkspaceId, WorkspaceStore};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

const RESULT_LIMIT: usize = 512 * 1024;

#[derive(Debug, Clone)]
pub struct DevelopmentToolContext {
    pub authorization: ToolAuthorization,
    pub initiator: Option<crate::development::Actor>,
    pub expected_generation: u64,
    pub expected_project_revision: u64,
}

/// Routes Glass Agent operations through authoritative resident services.
#[derive(Debug, Clone)]
pub(crate) struct DevelopmentToolRouter {
    core: AgentToolGateway,
    descriptors: Vec<ToolDescriptor>,
}

impl Default for DevelopmentToolRouter {
    fn default() -> Self {
        let core = AgentToolGateway::default();
        let service = service_descriptors();
        let mut descriptors = core
            .descriptors()
            .into_iter()
            .filter(|descriptor| {
                !service
                    .iter()
                    .any(|candidate| candidate.name == descriptor.name)
            })
            .collect::<Vec<_>>();
        descriptors.extend(service);
        Self { core, descriptors }
    }
}

impl DevelopmentToolRouter {
    pub(crate) fn with_customization(customization: &crate::Customization) -> Self {
        let mut router = Self::default();
        let custom = customization.descriptors();
        router
            .descriptors
            .retain(|descriptor| !custom.iter().any(|item| item.name == descriptor.name));
        router.descriptors.extend(custom);
        router
    }

    pub(crate) fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.descriptors.clone()
    }

    pub(crate) fn execute(
        &self,
        workspace: &mut DevelopmentWorkspace,
        call: &ToolCall,
        context: &DevelopmentToolContext,
    ) -> DevelopmentResult<Value> {
        if context.authorization.is_glass_core() && !is_integration_tool(&call.name) {
            return Err(DevelopmentError::NotFound(format!(
                "Glass integration {}",
                call.name
            )));
        }
        if workspace.generation() != context.expected_generation {
            return Err(DevelopmentError::Conflict(format!(
                "stale workspace generation {}; current generation is {}",
                context.expected_generation,
                workspace.generation()
            )));
        }
        if workspace.project().revision() != context.expected_project_revision {
            return Err(DevelopmentError::Conflict(format!(
                "stale project revision {}; current revision is {}",
                context.expected_project_revision,
                workspace.project().revision()
            )));
        }
        let descriptor = self
            .descriptors
            .clone()
            .into_iter()
            .find(|descriptor| descriptor.name == call.name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("tool {}", call.name)))?;
        if !descriptor.available {
            return Err(DevelopmentError::Conflict(
                descriptor
                    .unavailable_reason
                    .clone()
                    .unwrap_or_else(|| format!("tool {} is unavailable", call.name)),
            ));
        }
        if descriptor.mutating && !context.authorization.permits_mutation() {
            return Err(DevelopmentError::Conflict(format!(
                "tool {} requires mutation authority",
                call.name
            )));
        }
        let actor_id = context.authorization.actor.id.clone();
        let initiator_id = context
            .initiator
            .as_ref()
            .map(|actor| actor.id.as_str())
            .unwrap_or(actor_id.as_str());
        let project_revision = workspace.project().revision();
        let resource = format!("tool:{}", call.id);
        let argument_bytes = serde_json::to_vec(&call.arguments)?.len();
        workspace.intelligence_mut().upsert_node(DevelopmentNode {
            id: resource.clone(),
            kind: DevelopmentNodeKind::ToolCall,
            label: call.name.clone(),
            revision: project_revision,
            stale: false,
            evidence: serde_json::json!({
                "toolCallId":call.id,
                "name":call.name,
                "mutating":descriptor.mutating,
                "argumentBytes":argument_bytes
            }),
        })?;
        workspace.intelligence_mut().link(
            "repository:root",
            &resource,
            "receivedToolCall",
            project_revision,
            &actor_id,
            serde_json::json!({"name":call.name,"initiator":initiator_id,"executor":actor_id}),
        )?;
        workspace.intelligence_mut().record(ObservableEventInput {
            actor: &actor_id,
            subsystem: "tool",
            kind: "called",
            resource: Some(&resource),
            workspace_revision: project_revision,
            evidence: serde_json::json!({"name":call.name,"mutating":descriptor.mutating,"initiator":initiator_id,"executor":actor_id}),
            rationale: None,
        })?;
        workspace.customization().run_hooks(
            "tool.before",
            &serde_json::json!({"id":call.id,"name":call.name,"actor":&actor_id}),
            &actor_id,
        )?;
        let result = if service_descriptors()
            .iter()
            .any(|descriptor| descriptor.name == call.name)
        {
            self.execute_service(workspace, call, context)
        } else if workspace.customization().custom_tool(&call.name).is_some() {
            workspace.customization().execute_tool(
                &call.name,
                &call.arguments,
                &actor_id,
                &context.authorization,
            )
        } else if let Some(name) = call.name.strip_prefix("glass.command.") {
            workspace
                .customization()
                .execute_command(name, &actor_id, &context.authorization)
        } else {
            self.core
                .execute(workspace.project_mut(), call, &context.authorization)
        };
        match result {
            Ok(result) => {
                workspace.customization().run_hooks(
                    "tool.after",
                    &serde_json::json!({"id":call.id,"name":call.name,"actor":&actor_id,"ok":true}),
                    &actor_id,
                )?;
                let result_bytes = serde_json::to_vec(&result)?.len();
                if result_bytes > RESULT_LIMIT {
                    return Err(DevelopmentError::InvalidInput(format!(
                        "tool result exceeds the {RESULT_LIMIT} byte limit"
                    )));
                }
                let resulting_revision = workspace.project().revision();
                project_observable_result(
                    workspace,
                    call,
                    &actor_id,
                    project_revision,
                    resulting_revision,
                    &result,
                )?;
                workspace.intelligence_mut().record(ObservableEventInput {
                    actor: &actor_id,
                    subsystem: "tool",
                    kind: "completed",
                    resource: Some(&resource),
                    workspace_revision: resulting_revision,
                    evidence: serde_json::json!({"name":call.name,"resultBytes":result_bytes}),
                    rationale: None,
                })?;
                Ok(result)
            }
            Err(error) => {
                workspace.customization().run_hooks(
                        "tool.after",
                        &serde_json::json!({"id":call.id,"name":call.name,"actor":&actor_id,"ok":false,"error":error.to_string()}),
                        &actor_id,
                    )?;
                let resulting_revision = workspace.project().revision();
                workspace.intelligence_mut().record(ObservableEventInput {
                    actor: &actor_id,
                    subsystem: "tool",
                    kind: "failed",
                    resource: Some(&resource),
                    workspace_revision: resulting_revision,
                    evidence: serde_json::json!({"name":call.name,"error":error.to_string()}),
                    rationale: None,
                })?;
                Err(error)
            }
        }
    }

    fn execute_service(
        &self,
        workspace: &mut DevelopmentWorkspace,
        call: &ToolCall,
        context: &DevelopmentToolContext,
    ) -> DevelopmentResult<Value> {
        let string = |name: &str| required_string(call, name);
        let actor = normalized_actor_id(&context.authorization.actor.id);
        match call.name.as_str() {
            "glass.agent.delegate" => {
                let sandbox = crate::external_agents::ExternalSandbox::parse(
                    optional_string(call, "sandbox").unwrap_or("read-only"),
                )
                .map_err(DevelopmentError::InvalidInput)?;
                if sandbox == crate::external_agents::ExternalSandbox::WorkspaceWrite
                    && !context.authorization.permits_mutation()
                {
                    return Err(DevelopmentError::Conflict(
                        "workspace-write delegation requires mutation authority"
                            .into(),
                    ));
                }
                let timeout = call
                    .arguments
                    .get("timeoutSeconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(crate::external_agents::DEFAULT_TIMEOUT_SECS);
                let result = crate::external_agents::delegate(
                    crate::external_agents::ExternalAgentRequest {
                        harness: string("harness")?.into(),
                        root: workspace.root().to_path_buf(),
                        prompt: string("prompt")?.into(),
                        sandbox,
                        timeout: Duration::from_secs(timeout),
                        authorization: context.authorization.clone(),
                    },
                )
                .map_err(DevelopmentError::Process)?;
                Ok(serde_json::to_value(result)?)
            }
            "glass.editor.open" => map_service(
                workspace
                    .project_mut()
                    .open_buffer(string("path")?, context.authorization.actor.clone()),
            ),
            "glass.editor.selection" => {
                let path = string("path")?;
                let buffer = workspace
                    .project()
                    .buffer(path)
                    .cloned()
                    .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
                let selected_text = workspace.project().selected_editor_text(path)?;
                Ok(serde_json::json!({
                    "path":buffer.path,
                    "line":buffer.cursor_line,
                    "column":buffer.cursor_column,
                    "selection":buffer.selection,
                    "selectedText":selected_text,
                    "actor":buffer.actor,
                    "dirty":buffer.dirty
                }))
            }
            "glass.editor.replace_selection" => {
                let buffer = workspace.project_mut().replace_buffer_selection(
                    string("path")?,
                    string("replacement")?.to_string(),
                    context.authorization.actor.clone(),
                )?;
                Ok(serde_json::to_value(buffer)?)
            }
            "glass.editor.replace" => {
                let matches = workspace.project_mut().replace_in_buffer(
                    string("path")?,
                    string("oldText")?,
                    string("newText")?,
                    context.authorization.actor.clone(),
                )?;
                Ok(serde_json::json!({"replacements":matches}))
            }
            "glass.editor.save" => {
                map_service(workspace.project_mut().save_buffer(string("path")?))
            }
            "glass.editor.diff" => map_service(workspace.project_mut().diff()),
            "glass.editor.buffers" => Ok(serde_json::to_value(
                workspace.project().buffers().cloned().collect::<Vec<_>>(),
            )?),
            "glass.editor.comments" => Ok(serde_json::to_value(
                workspace.project().editor_comments(optional_string(call, "path")),
            )?),
            "glass.editor.proposals" => Ok(serde_json::to_value(
                workspace.project().editor_proposals(),
            )?),
            "glass.editor.fim" => {
                let provider = crate::fim::FimProvider::from_editor(
                    &workspace.customization().config().editor,
                )
                .ok_or_else(|| {
                    DevelopmentError::NotFound("FIM is disabled".into())
                })?;
                match provider.backend {
                    crate::fim::FimBackend::Stub => {
                        let text = provider.complete(
                            string("prefix")?,
                            optional_string(call, "suffix").unwrap_or(""),
                        )
                        .map_err(DevelopmentError::InvalidInput)?;
                        Ok(serde_json::json!({"text": text, "backend": "stub"}))
                    }
                    crate::fim::FimBackend::Resident => {
                        let prefix = string("prefix")?.to_string();
                        let suffix = optional_string(call, "suffix")
                            .unwrap_or("")
                            .to_string();
                        let snapshots = workspace.agents().list()?;
                        let id = snapshots
                            .into_iter()
                            .find(|item| item.status == crate::AgentStatus::Idle)
                            .map(|item| item.id)
                            .ok_or_else(|| {
                                DevelopmentError::NotFound(
                                    "no idle resident agent for FIM; start an agent first".into(),
                                )
                            })?;
                        workspace.agents().request(
                            &id,
                            crate::resident_harness::ResidentSessionRequest::Complete { prefix, suffix },
                        )?;
                        Ok(serde_json::json!({
                            "queued": true,
                            "backend": "resident",
                            "agentId": id.as_str(),
                        }))
                    }
                }
            },
            "glass.editor.checkpoints" => Ok(serde_json::to_value(
                workspace.project().editor_checkpoints(),
            )?),
            "glass.editor.comment.add" => map_service(workspace.project_mut().add_editor_comment(
                string("path")?,
                u32_value(call, "startLine")?,
                u32_value(call, "endLine")?,
                string("text")?.to_string(),
                context.authorization.actor.clone(),
            )),
            "glass.editor.comment.resolve" => map_service(
                workspace
                    .project_mut()
                    .resolve_editor_comment(string("id")?, context.authorization.actor.clone()),
            ),
            "glass.editor.proposal.create" => map_service(
                workspace.project_mut().propose_editor_change(
                    string("path")?,
                    string("original")?.to_string(),
                    string("proposed")?.to_string(),
                    string("summary")?.to_string(),
                    context.authorization.actor.clone(),
                ),
            ),
            "glass.editor.proposal.accept" => map_service(
                workspace
                    .project_mut()
                    .accept_editor_proposal(string("id")?, context.authorization.actor.clone()),
            ),
            "glass.editor.proposal.accept_pack" => map_service(
                workspace
                    .project_mut()
                    .accept_pending_editor_proposals(context.authorization.actor.clone()),
            ),
            "glass.editor.proposal.reject" => map_service(
                workspace
                    .project_mut()
                    .reject_editor_proposal(string("id")?, context.authorization.actor.clone()),
            ),
            "glass.editor.checkpoint.create" => map_service(
                workspace
                    .project_mut()
                    .create_editor_checkpoint(
                        string("name")?.to_string(),
                        context.authorization.actor.clone(),
                    ),
            ),
            "glass.editor.checkpoint.restore" => map_service(
                workspace
                    .project_mut()
                    .restore_editor_checkpoint(string("id")?, context.authorization.actor.clone()),
            ),
            "glass.process.start" => map_service(workspace.project_mut().start_process(
                string("name")?,
                required_string(call, "command")?,
            )),
            "glass.command.run" => map_service(workspace.project_mut().run_verification(
                string("name")?,
                required_string(call, "command")?,
                timeout(call, 300)?.unwrap_or(Duration::from_secs(30)),
            )),
            "glass.process.stop" => {
                map_service(workspace.project_mut().stop_process(string("name")?))
            }
            "glass.process.logs" => {
                let name = string("name")?;
                let processes = workspace.project_mut().processes();
                processes.poll()?;
                Ok(serde_json::json!({
                    "name": name,
                    "output": processes.output(name)?,
                }))
            }
            "glass.process.list" => {
                map_service(workspace.project_mut().processes().list_checked())
            }

            "glass.process.restart" => {
                map_service(workspace.project_mut().processes().restart(string("name")?))
            }
            "glass.process.input" => {
                workspace
                    .project_mut()
                    .processes()
                    .send_input(string("name")?, string("input")?)?;
                Ok(serde_json::json!({"sent":true}))
            }
            "glass.process.resize" => {
                let cols = u16_value(call, "cols")?;
                let rows = u16_value(call, "rows")?;
                workspace
                    .project_mut()
                    .processes()
                    .resize(string("name")?, cols, rows)?;
                Ok(serde_json::json!({"resized":true,"cols":cols,"rows":rows}))
            }
            "glass.process.health" => {
                let name = string("name")?;
                let snapshot = workspace
                    .project_mut()
                    .processes()
                    .list_checked()?
                    .into_iter()
                    .find(|snapshot| snapshot.name == name)
                    .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;
                Ok(serde_json::json!({
                    "name":snapshot.name,
                    "pid":snapshot.pid,
                    "state":snapshot.state,
                    "health":snapshot.health,
                    "detectedUrls":snapshot.detected_urls
                }))
            }
            "glass.process.ports" => {
                let snapshots = workspace.project_mut().processes().list_checked()?;
                Ok(serde_json::json!({
                    "processes":snapshots.into_iter().map(|snapshot| serde_json::json!({
                        "name":snapshot.name,
                        "pid":snapshot.pid,
                        "urls":snapshot.detected_urls
                    })).collect::<Vec<_>>()
                }))
            }
            "glass.browser.start" => {
                let mut config: BrowserStartConfig =
                    serde_json::from_value(call.arguments.clone())?;
                config.port_explicit = call.arguments.get("port").is_some();
                workspace.browser().start(config)
            }
            "glass.browser.stop" => workspace.browser().stop(),
            "glass.browser.attach" => {
                let mut config: BrowserStartConfig = serde_json::from_value(call.arguments.clone())?;
                config.port_explicit = call.arguments.get("port").is_some();
                config.attach = true;
                workspace.browser().start(config)
            }
            "glass.browser.reconnect" => workspace.browser().reconnect(),
            "glass.browser.state" => workspace.browser().state(),
            "glass.browser.observe" => workspace.browser().observe(),
            "glass.browser.verify" => {
                let predicate: VerificationPredicate = serde_json::from_value(
                    required_value(call, "predicate")?.clone(),
                )
                .map_err(|error| {
                    DevelopmentError::InvalidInput(format!(
                        "glass.browser.verify predicate is invalid: {error}"
                    ))
                })?;
                let timeout = timeout(call, 30)?.unwrap_or(Duration::from_secs(8));
                workspace.browser().verify_predicate(predicate, timeout)
            }
            "glass.browser.snapshot" => workspace.browser().snapshot(),
            "glass.browser.semantic" => workspace.browser().semantic(match optional_string(call, "level").unwrap_or("structured") {
                "summary" => SemanticObservationLevel::Summary,
                "interactive" => SemanticObservationLevel::Interactive,
                "structured" => SemanticObservationLevel::Structured,
                "detailed" => SemanticObservationLevel::Detailed,
                "raw" => SemanticObservationLevel::Raw,
                _ => return Err(DevelopmentError::InvalidInput("browser semantic level must be summary, interactive, structured, detailed, or raw".into())),
            }),
            "glass.browser.web_ir" => workspace.browser().web_ir(),
            "glass.browser.diff" => workspace.browser().diff(),
            "glass.browser.targets" => workspace.browser().targets(),
            "glass.browser.target.select" => workspace
                .browser()
                .select_target(string("targetId")?.into()),
            "glass.browser.navigate" => workspace.browser().navigate(
                string("url")?.into(),
                optional_unsigned(call, "browserRevision")?,
                timeout(call, 180)?.unwrap_or(Duration::from_secs(30)),
            ),
            "glass.browser.act" => {
                let revision = optional_unsigned(call, "browserRevision")?;
                match string("action")? {
                    "back" => workspace.browser().back(revision),
                    "forward" => workspace.browser().forward(revision),
                    "reload" => workspace.browser().reload(revision),
                    "stopLoading" => workspace.browser().stop_loading(revision),
                    "click" => workspace
                        .browser()
                        .click(string("target")?.into(), revision),
                    "type" => workspace.browser().type_text(
                        string("text")?.into(),
                        optional_string(call, "target").map(str::to_string),
                        revision,
                    ),
                    "scroll" => workspace.browser().scroll(
                        number(call, "dx", 0.0)?,
                        number(call, "dy", 0.0)?,
                        revision,
                    ),
                    action => Err(DevelopmentError::InvalidInput(format!(
                        "unsupported browser action {action}; expected back, forward, reload, stopLoading, click, type, or scroll"
                    ))),
                }
            }
            "glass.browser.screenshot" => workspace.browser().screenshot(),
            "glass.browser.remote-view.open" => workspace.browser().open_remote_view(),
            "glass.browser.remote-view.status" => workspace.browser().remote_view_status(),
            "glass.browser.remote-view.revoke" => workspace.browser().revoke_remote_view(),
            "glass.workflow.list" => workspace.browser().list_workflows(),
            "glass.workflow.run" => workspace.browser().run_workflow(
                required_value(call, "definition")?.clone(),
                value_map(call, "inputs")?,
            ),
            "glass.workflow.pause" => workspace.browser().pause_workflow(),
            "glass.workflow.resume" => workspace.browser().resume_workflow(
                required_value(call, "definition")?.clone(),
                value_map(call, "inputs")?,
                required_value(call, "checkpoint")?.clone(),
            ),
            "glass.workflow.cancel" => workspace.browser().cancel_workflow(),
            "glass.workflow.verify" => workspace.browser().verify_workflow(),
            "glass.workflow.record" => {
                let session: WorkflowRecordingSession = serde_json::from_value(
                    required_value(call, "session")?.clone(),
                )?;
                Ok(serde_json::to_value(record_semantic_events(session).map_err(|error| {
                    DevelopmentError::InvalidInput(error.to_string())
                })?)?)
            }
            "glass.memory.retrieve" => {
                if let Some(record_id) = optional_string(call, "recordId") {
                    Ok(serde_json::to_value(
                        workspace
                            .knowledge()
                            .get(record_id)
                            .ok_or_else(|| DevelopmentError::NotFound(format!("memory record {record_id}")))?,
                    )?)
                } else {
                    Ok(serde_json::json!({
                        "records":workspace.knowledge().records(),
                        "stats":workspace.knowledge().stats().map_err(|error| DevelopmentError::Process(error.to_string()))?
                    }))
                }
            }
            "glass.memory.explain" => {
                let record_id = string("recordId")?;
                let record = workspace
                    .knowledge()
                    .get(record_id)
                    .ok_or_else(|| DevelopmentError::NotFound(format!("memory record {record_id}")))?;
                Ok(serde_json::json!({
                    "record":record,
                    "advisory":true,
                    "authority":"fresh browser evidence remains authoritative"
                }))
            }
            "glass.memory.forget" => {
                let record_id = string("recordId")?;
                let change = workspace
                    .knowledge_mut()
                    .remove(record_id)
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                Ok(serde_json::to_value(change)?)
            }
            "glass.semantic.diff" => workspace.browser().diff(),
            "glass.semantic.links" => Ok(serde_json::json!({
                "nodes":workspace.intelligence().nodes().collect::<Vec<_>>(),
                "edges":workspace.intelligence().edges().collect::<Vec<_>>()
            })),
            "glass.github.review" => Ok(serde_json::to_value(crate::github::review(
                workspace.root(),
            )?)?),
            "glass.github.ship" => {
                let request: crate::github::GitHubShipRequest =
                    serde_json::from_value(call.arguments.clone())?;
                Ok(serde_json::to_value(crate::github::ship(
                    workspace.root(),
                    &request,
                )?)?)
            }
            "glass.git.status" => git_value(workspace.git(), |git| git.status()),
            "glass.git.diff" => git_value(workspace.git(), |git| {
                git.diff(
                    boolean(call, "staged", false),
                    optional_string(call, "path"),
                )
            }),
            "glass.git.stage" => {
                let paths = string_array(call, "paths")?;
                git_mutation(workspace.git(), |git| git.stage(&paths))?;
                Ok(serde_json::json!({"staged":paths}))
            }
            "glass.git.unstage" => {
                let paths = string_array(call, "paths")?;
                git_mutation(workspace.git(), |git| git.unstage(&paths))?;
                Ok(serde_json::json!({"unstaged":paths}))
            }
            "glass.git.discard" => {
                let paths = string_array(call, "paths")?;
                git_mutation(workspace.git(), |git| git.discard(&paths))?;
                Ok(serde_json::json!({"discarded":paths}))
            }
            "glass.git.push" => {
                git_mutation(workspace.git(), |git| {
                    git.push(optional_string(call, "remote"), optional_string(call, "branch"))
                })?;
                Ok(serde_json::json!({"pushed":true}))
            }
            "glass.git.fetch" => {
                git_mutation(workspace.git(), |git| {
                    git.fetch(optional_string(call, "remote"))
                })?;
                Ok(serde_json::json!({"fetched":true}))
            }
            "glass.git.pull" => {
                git_mutation(workspace.git(), |git| {
                    git.pull(optional_string(call, "remote"), optional_string(call, "branch"))
                })?;
                Ok(serde_json::json!({"pulled":true}))
            }
            "glass.git.merge" => {
                let branch = string("branch")?;
                git_mutation(workspace.git(), |git| git.merge(branch))?;
                Ok(serde_json::json!({"merged":branch}))
            }
            "glass.git.rebase" => {
                let onto = string("onto")?;
                git_mutation(workspace.git(), |git| git.rebase(onto))?;
                Ok(serde_json::json!({"rebased":onto}))
            }
            "glass.git.commit" => {
                let message = string("message")?;
                git_value(workspace.git(), |git| git.commit(message))
            }
            "glass.git.branches" => git_value(workspace.git(), |git| git.branches()),
            "glass.git.branch.create" => {
                let name = string("name")?;
                let start_point = optional_string(call, "startPoint");
                git_mutation(workspace.git(), |git| git.create_branch(name, start_point))?;
                Ok(serde_json::json!({"created":name}))
            }
            "glass.git.branch.switch" => {
                let name = string("name")?;
                let create = boolean(call, "create", false);
                git_mutation(workspace.git(), |git| git.switch_branch(name, create))?;
                Ok(serde_json::json!({"current":name}))
            }
            "glass.git.blame" => {
                let path = string("path")?;
                let start = unsigned(call, "startLine", 1)?;
                let end = unsigned(call, "endLine", 1)?;
                git_value(workspace.git(), |git| git.blame(path, start, end))
            }
            "glass.git.conflicts" => git_value(workspace.git(), |git| git.conflicts()),
            "glass.git.stash.list" => git_value(workspace.git(), |git| git.stash_list()),
            "glass.git.stash.push" => {
                let message = string("message")?;
                let include_untracked = boolean(call, "includeUntracked", false);
                git_mutation(workspace.git(), |git| {
                    git.stash_push(message, include_untracked)
                })?;
                Ok(serde_json::json!({"stashed":true}))
            }
            "glass.git.stash.pop" => {
                let reference = string("reference")?;
                git_mutation(workspace.git(), |git| git.stash_pop(reference))?;
                Ok(serde_json::json!({"restored":reference}))
            }
            "glass.git.worktree.list" => git_value(workspace.git(), |git| git.worktrees()),
            "glass.git.worktree.create" => {
                let path = confined_worktree_path(workspace.root(), Path::new(string("path")?))?;
                let branch = string("branch")?;
                let create_branch = boolean(call, "createBranch", true);
                git_mutation(workspace.git(), |git| {
                    git.create_worktree(&path, branch, create_branch)
                })?;
                Ok(serde_json::json!({"path":path,"created":true}))
            }
            "glass.git.worktree.remove" => {
                let path = confined_worktree_path(workspace.root(), Path::new(string("path")?))?;
                git_mutation(workspace.git(), |git| {
                    git.remove_worktree(&path, boolean(call, "force", false))
                })?;
                Ok(serde_json::json!({"path":path,"removed":true}))
            }
            "glass.test.discover" => Ok(serde_json::to_value(
                workspace.tests().suites().collect::<Vec<_>>(),
            )?),
            "glass.test.run" => {
                let revision = workspace.project().revision();
                map_service(workspace.tests_mut().start(
                    string("runId")?,
                    string("suiteId")?,
                    &actor,
                    revision,
                    timeout(call, 3_600)?,
                ))
            }
            "glass.test.run-affected" => {
                let paths = string_array(call, "changedPaths")?;
                let suite_ids = workspace
                    .tests()
                    .affected_suites(paths.iter().map(String::as_str))
                    .into_iter()
                    .map(|suite| suite.id.clone())
                    .collect::<Vec<_>>();
                let revision = workspace.project().revision();
                let prefix = optional_string(call, "runPrefix").unwrap_or("affected");
                let mut runs = Vec::new();
                for (index, suite_id) in suite_ids.into_iter().enumerate() {
                    runs.push(map_service(workspace.tests_mut().start(
                        &format!("{prefix}-{index}"),
                        &suite_id,
                        &actor,
                        revision,
                        timeout(call, 3_600)?,
                    ))?);
                }
                Ok(serde_json::to_value(runs)?)
            }
            "glass.test.cancel" => {
                map_service(workspace.tests_mut().cancel(string("runId")?))?;
                Ok(serde_json::json!({"cancelled":string("runId")?}))
            }
            "glass.test.results" => {
                let finished = map_service(workspace.tests_mut().poll())?;
                Ok(serde_json::json!({
                    "finished":finished,
                    "running":workspace.tests().running().collect::<Vec<_>>(),
                    "results":workspace.tests().results().collect::<Vec<_>>()
                }))
            }
            "glass.test.watch" => {
                let revision = workspace.project().revision();
                map_service(workspace.tests_mut().watch(string("suiteId")?, revision))?;
                Ok(serde_json::json!({"watching":string("suiteId")?}))
            }
            "glass.eval.start" => {
                let kind = parse_kernel_kind(string("kind")?)?;
                let name = string("name")?;
                let capabilities = string_array_or_empty(call, "capabilities")?;
                let requested_mutation_authority = boolean(call, "mutationAuthority", false);
                let has_mutating_capability = capabilities.iter().any(|capability| {
                    workspace
                        .tool_descriptors()
                        .iter()
                        .any(|descriptor| descriptor.name == *capability && descriptor.mutating)
                });
                if has_mutating_capability && !requested_mutation_authority {
                    return Err(DevelopmentError::Conflict(
                        "mutating kernel capabilities require mutation authority".into(),
                    ));
                }
                let mutation_authority = requested_mutation_authority
                    && context.authorization.permits_mutation();
                map_service(workspace.kernels_mut().start_governed(
                    name,
                    kind,
                    &format!("kernel:{name}"),
                    &capabilities,
                    mutation_authority,
                ))?;
                Ok(serde_json::json!({"started":name,"kind":kind,"capabilities":capabilities,"mutationAuthority":mutation_authority}))
            }
            "glass.eval.execute" => {
                map_service(workspace.execute_kernel(
                    string("name")?,
                    string("code")?,
                    &context.authorization,
                    timeout(call, 600)?,
                ))
            }
            "glass.eval.list" => Ok(serde_json::to_value(
                workspace.kernels().snapshots().collect::<Vec<_>>(),
            )?),
            "glass.eval.reset" => {
                map_service(workspace.kernels_mut().reset(string("name")?, &actor))?;
                Ok(serde_json::json!({"reset":string("name")?}))
            }
            "glass.eval.cancel" => map_service(workspace.kernels_mut().cancel(string("name")?)),
            "glass.eval.stop" => map_service(workspace.kernels_mut().stop(string("name")?)),
            "glass.lsp.start" => {
                let server = string("server")?;
                let configured = workspace.customization().config().lsp.get(server).cloned();
                let config = match (optional_string(call, "command"), configured) {
                    (Some(command), _) => crate::lsp::LanguageServerConfig {
                        command: command.to_string(),
                        arguments: string_array_or_empty(call, "arguments")?,
                    },
                    (None, Some(config)) => crate::lsp::LanguageServerConfig {
                        command: config.command,
                        arguments: config.args,
                    },
                    (None, None) => return Err(DevelopmentError::InvalidInput(
                        "lsp start requires command or a matching glass.toml lsp entry".into(),
                    )),
                };
                map_service(workspace.language().start(server, &config))?;
                Ok(serde_json::json!({"started":server}))
            }
            "glass.lsp.stop" => {
                map_service(workspace.language().stop(string("server")?))?;
                Ok(serde_json::json!({"stopped":string("server")?}))
            }
            "glass.lsp.list" => Ok(serde_json::json!({
                "servers":workspace.language().names().collect::<Vec<_>>()
            })),
            "glass.lsp.events" => Ok(serde_json::to_value(
                workspace.language().events(unsigned(call, "since", 0)?),
            )?),
            "glass.lsp.raw" => map_service(workspace.language().raw_request(
                string("server")?,
                &actor,
                string("method")?,
                call.arguments
                    .get("params")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
                timeout(call, 60)?.unwrap_or(Duration::from_secs(10)),
            )),
            "glass.lsp.diagnostics" => {
                let server = string("server")?.to_string();
                let path = string("path")?.to_string();
                let diagnostics = map_service(
                    workspace
                        .language()
                        .diagnostics(&server, &actor, &path),
                )?;
                let diagnostics: Vec<crate::development::LanguageDiagnostic> =
                    serde_json::from_value(diagnostics)?;
                workspace
                    .project_mut()
                    .publish_diagnostics(&path, diagnostics.clone())?;
                Ok(serde_json::to_value(diagnostics)?)
            }
            "glass.lsp.hover" => map_service(workspace.language().hover(
                string("server")?,
                &actor,
                string("path")?,
                unsigned(call, "line", 1)? as u32,
                unsigned(call, "character", 1)? as u32,
            )),
            "glass.lsp.completion" => map_service(workspace.language().completion(
                string("server")?,
                &actor,
                string("path")?,
                unsigned(call, "line", 1)? as u32,
                unsigned(call, "character", 1)? as u32,
            )),
            "glass.lsp.definition" => map_service(workspace.language().definition(
                string("server")?,
                &actor,
                string("path")?,
                u32_value(call, "line")?,
                u32_value(call, "character")?,
            )),
            "glass.lsp.declaration" => map_service(workspace.language().declaration(
                string("server")?,
                &actor,
                string("path")?,
                u32_value(call, "line")?,
                u32_value(call, "character")?,
            )),
            "glass.lsp.implementation" => map_service(workspace.language().implementation(
                string("server")?,
                &actor,
                string("path")?,
                u32_value(call, "line")?,
                u32_value(call, "character")?,
            )),
            "glass.lsp.references" => map_service(workspace.language().references(
                string("server")?,
                &actor,
                string("path")?,
                u32_value(call, "line")?,
                u32_value(call, "character")?,
            )),
            "glass.lsp.document_symbols" => map_service(workspace.language().document_symbols(
                string("server")?,
                &actor,
                string("path")?,
            )),
            "glass.lsp.workspace_symbols" => map_service(workspace.language().workspace_symbols(
                string("server")?,
                &actor,
                string("query")?,
            )),
            "glass.lsp.inlay_hints" => map_service(workspace.language().inlay_hints(
                string("server")?,
                &actor,
                string("path")?,
            )),
            "glass.lsp.signature_help" => map_service(workspace.language().signature_help(
                string("server")?,
                &actor,
                string("path")?,
                u32_value(call, "line")?,
                u32_value(call, "character")?,
            )),
            "glass.lsp.code_actions" => map_service(workspace.language().code_actions(
                string("server")?,
                &actor,
                string("path")?,
                position(call, "start")?,
                position(call, "end")?,
                value_array_or_empty(call, "diagnostics")?,
            )),
            "glass.lsp.formatting" => map_service(workspace.language().formatting(
                string("server")?,
                &actor,
                string("path")?,
            )),
            "glass.lsp.range_formatting" => map_service(workspace.language().range_formatting(
                string("server")?,
                &actor,
                string("path")?,
                position(call, "start")?,
                position(call, "end")?,
            )),
            "glass.lsp.semantic_tokens" => map_service(workspace.language().semantic_tokens(
                string("server")?,
                &actor,
                string("path")?,
            )),
            "glass.lsp.rename" => map_service(workspace.language().rename(
                string("server")?,
                &actor,
                string("path")?,
                u32_value(call, "line")?,
                u32_value(call, "character")?,
                string("newName")?,
            )),
            "glass.debug.start" => {
                let session = string("session")?;
                let configured = workspace.customization().config().dap.get(session).cloned();
                let (mut config, configured_tcp, configured_connect_timeout_ms) =
                    match (optional_string(call, "command"), configured) {
                    (Some(command), _) => (
                        DebugAdapterConfig::new(
                            command,
                            string_array_or_empty(call, "arguments")?,
                        ),
                        None,
                        5_000,
                    ),
                    (None, Some(config)) => (
                        DebugAdapterConfig::new(config.command, config.args),
                        config.tcp_address,
                        config.connect_timeout_ms,
                    ),
                    (None, None) => return Err(DevelopmentError::InvalidInput(
                        "debug start requires command or a matching glass.toml dap entry".into(),
                    )),
                };
                if let Some(address) = optional_string(call, "tcpAddress")
                    .map(str::to_string)
                    .or(configured_tcp)
                {
                    let address = address.parse().map_err(|_| {
                        DevelopmentError::InvalidInput(
                            "debug tcpAddress must be a loopback IP socket address".into(),
                        )
                    })?;
                    config = config.with_tcp(
                        address,
                        Duration::from_millis(unsigned(
                            call,
                            "connectTimeoutMs",
                            configured_connect_timeout_ms,
                        )?),
                    );
                }
                map_debug(workspace.start_debugger(
                    session,
                    &config,
                    Duration::from_secs(unsigned(call, "timeoutSeconds", 30)?),
                ))?;
                Ok(serde_json::json!({"started":session}))
            }
            "glass.debug.launch" => map_debug(
                debugger(workspace, string("session")?)?.launch(
                    call.arguments
                        .get("configuration")
                        .cloned()
                        .unwrap_or(Value::Null),
                ),
            ),
            "glass.debug.attach" => map_debug(
                debugger(workspace, string("session")?)?.attach(
                    call.arguments
                        .get("configuration")
                        .cloned()
                        .unwrap_or(Value::Null),
                ),
            ),
            "glass.debug.configuration_done" => {
                map_debug(debugger(workspace, string("session")?)?.configuration_done())
            }
            "glass.debug.restart" => map_debug(
                debugger(workspace, string("session")?)?.restart(
                    call.arguments
                        .get("configuration")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({})),
                ),
            ),
            "glass.debug.breakpoint.set" => {
                let lines = unsigned_array(call, "lines")?;
                let breakpoints = lines
                    .iter()
                    .copied()
                    .map(SourceBreakpoint::line)
                    .collect::<Vec<_>>();
                map_debug(
                    debugger(workspace, string("session")?)?
                        .set_source_breakpoints(&PathBuf::from(string("path")?), &breakpoints),
                )
            }
            "glass.debug.breakpoint.remove" => map_debug(
                debugger(workspace, string("session")?)?
                    .set_source_breakpoints(&PathBuf::from(string("path")?), &[]),
            ),
            "glass.debug.exception.set" => map_debug(
                debugger(workspace, string("session")?)?
                    .set_exception_breakpoints(&string_array_or_empty(call, "filters")?),
            ),
            "glass.debug.continue" => map_debug(
                debugger(workspace, string("session")?)?
                    .continue_thread(integer(call, "threadId")?),
            ),
            "glass.debug.pause" => map_debug(
                debugger(workspace, string("session")?)?.pause(integer(call, "threadId")?),
            ),
            "glass.debug.step" => {
                let debugger = debugger(workspace, string("session")?)?;
                let thread = integer(call, "threadId")?;
                match string("kind")? {
                    "over" => map_debug(debugger.next(thread)),
                    "in" => map_debug(debugger.step_in(thread)),
                    "out" => map_debug(debugger.step_out(thread)),
                    _ => Err(DevelopmentError::InvalidInput(
                        "debug step kind must be over, in, or out".into(),
                    )),
                }
            }
            "glass.debug.threads" => map_debug(debugger(workspace, string("session")?)?.threads()),
            "glass.debug.stack" => map_debug(
                debugger(workspace, string("session")?)?.stack_trace(integer(call, "threadId")?),
            ),
            "glass.debug.scopes" => map_debug(
                debugger(workspace, string("session")?)?.scopes(integer(call, "frameId")?),
            ),
            "glass.debug.variables" => map_debug(
                debugger(workspace, string("session")?)?
                    .variables(integer(call, "variablesReference")?),
            ),
            "glass.debug.evaluate" => map_debug(debugger(workspace, string("session")?)?.evaluate(
                string("expression")?,
                call.arguments.get("frameId").and_then(Value::as_i64),
                optional_string(call, "context").unwrap_or("repl"),
            )),
            "glass.debug.events" => {
                map_debug(debugger(workspace, string("session")?)?.poll_events())
            }
            "glass.debug.inspect" => {
                map_debug(debugger(workspace, string("session")?)?.snapshot())
            }
            "glass.debug.processes" => map_debug(
                debugger(workspace, string("session")?)?.debuggee_processes(),
            ),
            "glass.debug.disconnect" => map_debug(
                debugger(workspace, string("session")?)?
                    .disconnect(boolean(call, "terminateDebuggee", false)),
            ),
            "glass.debug.terminate" => map_debug(
                debugger(workspace, string("session")?)?
                    .terminate(boolean(call, "restart", false)),
            ),
            "glass.debug.stop" => {
                map_debug(workspace.stop_debugger(string("session")?))?;
                Ok(serde_json::json!({"stopped":string("session")?}))
            }
            "glass.agent.list" => map_service(workspace.agents().list()),
            "glass.agent.resume" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().resume(&id))?;
                Ok(serde_json::json!({"resuming":true,"agentId":id.as_str()}))
            }
            "glass.agent.spawn" => {
                let spec: AgentSpec =
                    serde_json::from_value(call.arguments.get("spec").cloned().ok_or_else(
                        || DevelopmentError::InvalidInput("agent spawn requires spec".into()),
                    )?)?;
                map_service(workspace.agents().create(spec))
            }
            "glass.agent.prompt" => {
                let id = agent_id(workspace, string("agentId")?)?;
                let text = string("text")?;
                map_service(workspace.agents().prompt_with_context(
                    &id,
                    text,
                    call.arguments.get("context").cloned(),
                ))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.send" => {
                let text = string("text")?;
                let context = call.arguments.get("context").cloned();
                let mode = optional_string(call, "mode").unwrap_or("follow-up");
                let expected_run_id = match call.arguments.get("expectedRunId") {
                    Some(Value::String(value)) if !value.is_empty() && value.len() <= 64 => {
                        Some(value.as_str())
                    }
                    Some(_) => {
                        return Err(DevelopmentError::InvalidInput(
                            "expectedRunId must be a bounded string".into(),
                        ));
                    }
                    None => None,
                };
                let client_message_id = match call.arguments.get("clientMessageId") {
                    Some(Value::String(value)) => Some(value.as_str()),
                    Some(_) => {
                        return Err(DevelopmentError::InvalidInput(
                            "clientMessageId must be a bounded string".into(),
                        ));
                    }
                    None => None,
                };
                let selected = optional_string(call, "agentId")
                    .map(|value| agent_id(workspace, value))
                    .transpose()?;
                let agent = selected.or_else(|| {
                    let snapshots = workspace.agents().list().ok()?;
                    default_agent_id(
                        snapshots
                            .into_iter()
                            .map(|snapshot| (snapshot.id, snapshot.status)),
                    )
                });
                let agent = match agent {
                    Some(agent) => agent,
                    None => {
                        let readiness = crate::resident_harness::resident_readiness()?;
                        if !readiness.ready {
                            return Err(DevelopmentError::Conflict(format!(
                                "{} is not ready; {}",
                                readiness.selected.label(),
                                readiness.remediation.first().cloned().unwrap_or_else(|| {
                                    "install the engine and connect a provider".into()
                                })
                            )));
                        }
                        let id = workspace
                            .agents()
                            .create(AgentSpec::new(
                                "assistant",
                                "interactive Glass Agent session",
                            ))?;
                        map_service(workspace.agents().send_with_context_and_client_message_id(
                            &id,
                            "prompt",
                            text,
                            context,
                            client_message_id,
                        ))?;
                        return Ok(serde_json::json!({"queued":true,"agentId":id}));
                    }
                };
                if let Some(expected_run_id) = expected_run_id {
                    let (_, _, actual_run_id) = workspace
                        .agents()
                        .follow_up_queue_target(&agent)?;
                    validate_expected_run_id(expected_run_id, &actual_run_id)?;
                }
                let mut status = workspace.agents().snapshot(&agent)?.status;
                let restarted = matches!(
                    status,
                    crate::AgentStatus::Failed | crate::AgentStatus::Cancelled
                );
                if restarted {
                    if expected_run_id.is_some() {
                        return Err(DevelopmentError::Conflict(
                            "follow-up destination is no longer active; review and retarget the queue item"
                                .into(),
                        ));
                    }
                    workspace.agents().restart(&agent)?;
                    status = workspace.agents().snapshot(&agent)?.status;
                }
                if expected_run_id.is_some() && mode == "steer" && status != crate::AgentStatus::Working {
                    return Err(DevelopmentError::Conflict(
                        "steering destination is not an active run".into(),
                    ));
                }
                let effective_mode = if status == crate::AgentStatus::Working
                    && matches!(mode, "steer" | "follow-up")
                {
                    mode
                } else {
                    "prompt"
                };
                map_service(workspace.agents().send_with_context_and_client_message_id(
                    &agent,
                    effective_mode,
                    text,
                    context,
                    client_message_id,
                ))?;
                let mut response = serde_json::json!({"queued":true,"agentId":agent});
                if restarted {
                    response["restarted"] = Value::Bool(true);
                }
                Ok(response)
            }
            "glass.agent.steer" => {
                let id = agent_id(workspace, string("agentId")?)?;
                let text = string("text")?;
                map_service(workspace.agents().steer_with_context(
                    &id,
                    text,
                    call.arguments.get("context").cloned(),
                ))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.follow-up" => {
                let id = agent_id(workspace, string("agentId")?)?;
                let text = string("text")?;
                map_service(workspace.agents().follow_up_with_context(
                    &id,
                    text,
                    call.arguments.get("context").cloned(),
                ))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.abort" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().cancel(&id))?;
                Ok(serde_json::json!({"cancelled":true}))
            }
            "glass.agent.compact" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().request(
                    &id,
                    ResidentSessionRequest::Compact {
                        instructions: optional_string(call, "instructions").map(str::to_string),
                    },
                ))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.model" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().request(
                    &id,
                    ResidentSessionRequest::SetModel {
                        provider: string("provider")?.into(),
                        model_id: string("modelId")?.into(),
                    },
                ))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.thinking" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().request(
                    &id,
                    ResidentSessionRequest::SetThinking {
                        level: string("level")?.into(),
                    },
                ))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.new-session" => {
                agent_request(workspace, call, ResidentSessionRequest::NewSession)
            }
            "glass.agent.clone-session" => {
                agent_request(workspace, call, ResidentSessionRequest::CloneSession)
            }
            "glass.agent.rewind" => agent_request(
                workspace,
                call,
                ResidentSessionRequest::Rewind {
                    entry_id: string("entryId")?.into(),
                },
            ),
            "glass.agent.fork" => agent_request(
                workspace,
                call,
                ResidentSessionRequest::Fork {
                    entry_id: string("entryId")?.into(),
                },
            ),
            "glass.agent.switch-session" => agent_request(
                workspace,
                call,
                ResidentSessionRequest::SwitchSession {
                    path: string("path")?.into(),
                },
            ),
            "glass.agent.sessions" => agent_request(workspace, call, ResidentSessionRequest::ListSessions),
            "glass.agent.tree" => agent_request(workspace, call, ResidentSessionRequest::Tree),
            "glass.agent.messages" => agent_request(workspace, call, ResidentSessionRequest::Messages),
            "glass.agent.entries" => agent_request(
                workspace,
                call,
                ResidentSessionRequest::Entries {
                    since: optional_string(call, "since").map(str::to_string),
                },
            ),
            "glass.agent.stats" => agent_request(workspace, call, ResidentSessionRequest::SessionStats),
            "glass.agent.name" => agent_request(
                workspace,
                call,
                ResidentSessionRequest::SetSessionName {
                    name: string("name")?.into(),
                },
            ),
            "glass.todo.list" => Ok(serde_json::to_value(workspace.todos())?),
            "glass.todo.write" => {
                let items = call
                    .arguments
                    .get("items")
                    .cloned()
                    .ok_or_else(|| DevelopmentError::InvalidInput("todo write requires items".into()))?;
                let items: Vec<crate::SessionTodo> = serde_json::from_value(items)?;
                Ok(serde_json::to_value(workspace.write_todos(items)?)?)
            }
            "glass.todo.complete" => {
                Ok(serde_json::to_value(workspace.complete_todo(string("id")?)?)?)
            }
            "glass.task.create" => {
                let spec = serde_json::from_value::<TaskSpec>(call.arguments.clone())?;
                Ok(serde_json::to_value(workspace.create_task(spec)?)?)
            }
            "glass.task.crew" => {
                let goal = string("goal")?;
                let wake = workspace.create_crew(goal)?;
                let crew = wake.tasks.clone();
                Ok(serde_json::json!({
                    "goal": goal,
                    "wake": wake,
                    "crew": crew,
                }))
            }
            "glass.task.list" => Ok(serde_json::to_value(workspace.tasks()?)?),
            "glass.task.wake" => Ok(serde_json::to_value(workspace.latest_crew_wake())?),
            "glass.task.get" | "glass.task.inspect" => {
                let id = TaskId::parse(string("taskId")?)?;
                Ok(serde_json::to_value(workspace.task(&id)?)?)
            }
            "glass.task.pause" => task_action(workspace, call, |workspace, id| {
                workspace.pause_task(id)
            }),
            "glass.task.resume" => task_action(workspace, call, |workspace, id| {
                workspace.resume_task(id)
            }),
            "glass.task.cancel" => task_action(workspace, call, |workspace, id| {
                workspace.cancel_task(id)
            }),
            "glass.task.retry" => task_action(workspace, call, |workspace, id| {
                workspace.retry_task(id)
            }),
            "glass.task.override-blocked" => task_action(workspace, call, |workspace, id| {
                workspace.override_blocked_task(id)
            }),
            "glass.task.reassign" => {
                let id = TaskId::parse(string("taskId")?)?;
                workspace.reassign_task(
                    &id,
                    string("role")?.into(),
                    optional_string(call, "model").map(str::to_string),
                    optional_string(call, "thinking").map(str::to_string),
                )?;
                Ok(serde_json::json!({"reassigned":true}))
            }
            "glass.task.evidence" | "glass.task.verify" => {
                let id = TaskId::parse(string("taskId")?)?;
                workspace.submit_task_evidence(
                    &id,
                    string("kind")?.into(),
                    context.authorization.actor.id.clone(),
                    boolean(call, "passed", false),
                    call.arguments
                        .get("details")
                        .cloned()
                        .unwrap_or(Value::Null),
                )?;
                Ok(serde_json::json!({"recorded":true}))
            }
            "glass.experiment.create" => {
                let port = call
                    .arguments
                    .get("port")
                    .and_then(Value::as_u64)
                    .map(u16::try_from)
                    .transpose()
                    .map_err(|_| DevelopmentError::InvalidInput("invalid experiment port".into()))?;
                Ok(serde_json::to_value(workspace.experiments()?.create(
                    string("id")?,
                    string("branch")?,
                    port,
                )?)?)
            }
            "glass.experiment.collect" => Ok(serde_json::to_value(
                workspace
                    .experiments()?
                    .collect_automatic(string("id")?)?,
            )?),
            "glass.experiment.list" => Ok(serde_json::to_value(
                workspace.experiments()?.snapshots(),
            )?),
            "glass.experiment.compare" => Ok(serde_json::to_value(
                workspace.experiments()?.compare(),
            )?),
            "glass.experiment.select" => Ok(serde_json::to_value(
                workspace.experiments()?.select(string("id")?)?,
            )?),
            "glass.experiment.remove" => {
                workspace
                    .experiments()?
                    .remove(string("id")?, boolean(call, "force", false))?;
                Ok(serde_json::json!({"removed":true}))
            }
            "glass.graph.query" => Ok(serde_json::to_value(
                workspace.intelligence().node(string("id")?),
            )?),
            "glass.graph.source" => {
                let line = call
                    .arguments
                    .get("line")
                    .and_then(Value::as_u64)
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| DevelopmentError::InvalidInput("invalid graph line".into()))?;
                Ok(serde_json::to_value(
                    workspace
                        .project()
                        .graph()
                        .entities_for_source(string("path")?, line),
                )?)
            }
            "glass.graph.path" | "glass.graph.explain" => Ok(serde_json::to_value(
                workspace
                    .intelligence()
                    .path(string("from")?, string("to")?)?,
            )?),
            "glass.graph.link" => {
                let revision = workspace.project().revision();
                let edge = workspace.intelligence_mut().link(
                    string("from")?,
                    string("to")?,
                    string("relation")?,
                    revision,
                    &actor,
                    call.arguments
                        .get("evidence")
                        .cloned()
                        .unwrap_or(Value::Null),
                )?;
                workspace.intelligence_mut().record(ObservableEventInput {
                    actor: &actor,
                    subsystem: "graph",
                    kind: "linked",
                    resource: None,
                    workspace_revision: revision,
                    evidence: serde_json::json!({"edgeId":edge,"from":string("from")?,"to":string("to")?,"relation":string("relation")?}),
                    rationale: None,
                })?;
                Ok(serde_json::json!({"edgeId":edge}))
            }
            "glass.replay.list" | "glass.replay.inspect" => {
                Ok(serde_json::to_value(workspace.intelligence().replay(
                    unsigned(call, "since", 0)?,
                    usize::try_from(unsigned(call, "limit", 128)?).unwrap_or(4096),
                )?)?)
            }
            "glass.replay.diff" => Ok(serde_json::to_value(
                workspace
                    .intelligence()
                    .replay_diff(unsigned(call, "from", 0)?, unsigned(call, "to", 1)?)?,
            )?),
            "glass.workspace.list" => {
                let store = WorkspaceStore::open_default()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                Ok(serde_json::to_value(store.list().map_err(|error| {
                    DevelopmentError::Process(error.to_string())
                })?)?)
            }
            "glass.workspace.inspect" => {
                let id = WorkspaceId::new(string("id")?)
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                let store = WorkspaceStore::open_default()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                Ok(serde_json::to_value(
                    store
                        .open(&id)
                        .map_err(|error| DevelopmentError::Process(error.to_string()))?,
                )?)
            }
            "glass.workspace.suspend" | "glass.workspace.resume" => {
                let id = WorkspaceId::new(string("id")?)
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                let store = WorkspaceStore::open_default()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                let workspace = if call.name.ends_with("suspend") {
                    store.suspend(&id)
                } else {
                    store.resume(&id)
                }
                .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                Ok(serde_json::to_value(workspace)?)
            }
            "glass.workspace.delete" => {
                let id = WorkspaceId::new(string("id")?)
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                let store = WorkspaceStore::open_default()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                store
                    .delete(&id)
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                Ok(serde_json::json!({"id":id,"deleted":true}))
            }
            "glass.daemon.start" => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                map_service(runtime.block_on(crate::daemon::start(None, None)))
            }
            "glass.daemon.status" | "glass.daemon.doctor" => map_service(
                crate::daemon::read_status_checked(None, None)
                    .map_err(|error| DevelopmentError::Process(error.to_string())),
            ),
            "glass.daemon.stop" => map_service(
                crate::daemon::stop(None, None)
                    .map(|_| serde_json::json!({"status":"stopped"}))
                    .map_err(|error| DevelopmentError::Process(error.to_string())),
            ),
            "glass.daemon.logs" => map_service(
                crate::daemon::log_tail(None)
                    .map_err(|error| DevelopmentError::Process(error.to_string())),
            ),
            "glass.agent.hello" | "glass.agent.models" | "glass.agent.commands" | "glass.agent.command" | "glass.agent.task-graph" | "glass.agent.task-abort" => {
                let selected = optional_string(call, "agentId")
                    .map(|value| agent_id(workspace, value))
                    .transpose()?
                    .or_else(|| active_agent_id(workspace));
                let id = match selected {
                    Some(id) => id,
                    None if call.name == "glass.agent.commands" => {
                        // The first slash menu needs a Core session before a
                        // provider turn exists. Start the idle runtime, then
                        // let the TUI request commands once it reports ready.
                        let id = workspace.agents().create(crate::agents::AgentSpec::new(
                            "assistant",
                            "interactive Glass workspace session",
                        ))?;
                        return Ok(serde_json::json!({
                            "queued": true,
                            "agentId": id.as_str(),
                            "initializing": true
                        }));
                    }
                    None => {
                        return Err(DevelopmentError::Conflict(
                            "start an interactive agent session before requesting model state"
                                .into(),
                        ));
                    }
                };
                let request = match call.name.as_str() {
                    "glass.agent.hello" => ResidentSessionRequest::Hello,
                    "glass.agent.models" => ResidentSessionRequest::Models,
                    "glass.agent.commands" => ResidentSessionRequest::CoreCommands,
                    "glass.agent.task-graph" => ResidentSessionRequest::CoreTaskGraph,
                    "glass.agent.task-abort" => ResidentSessionRequest::CoreTaskAbort {
                        task_id: required_string(call, "taskId")?.to_string(),
                        include_background: call.arguments["includeBackground"].as_bool().unwrap_or(false),
                    },
                    _ => ResidentSessionRequest::CoreCommand {
                        name: required_string(call, "name")?.to_string(),
                        arguments: optional_string(call, "arguments").map(str::to_owned),
                    },
                };
                map_service(workspace.agents().request(&id, request))?;
                Ok(serde_json::json!({"queued":true,"agentId":id.as_str()}))
            }
            "glass.project.attach" => {
                let actor = crate::development::Actor::external(string("actor")?);
                workspace.project_mut().attach_actor(actor.clone())?;
                Ok(serde_json::to_value(actor)?)
            }
            _ => Err(DevelopmentError::NotFound(format!("tool {}", call.name))),
        }
    }
}

fn service_descriptors() -> Vec<ToolDescriptor> {
    const READ: &[&str] = &[
        "glass.process.list",
        "glass.process.logs",
        "glass.github.review",
        "glass.git.status",
        "glass.git.branches",
        "glass.git.blame",
        "glass.git.conflicts",
        "glass.git.diff",
        "glass.git.stash.list",
        "glass.git.worktree.list",
        "glass.editor.selection",
        "glass.editor.diff",
        "glass.editor.buffers",
        "glass.editor.comments",
        "glass.editor.proposals",
        "glass.editor.fim",
        "glass.editor.checkpoints",
        "glass.eval.list",
        "glass.lsp.diagnostics",
        "glass.workspace.list",
        "glass.workspace.inspect",
        "glass.daemon.status",
        "glass.daemon.doctor",
        "glass.daemon.logs",
        "glass.agent.hello",
        "glass.agent.models",
        "glass.agent.commands",
        "glass.agent.command",
        "glass.agent.task-graph",
        "glass.lsp.list",
        "glass.lsp.events",
        "glass.lsp.hover",
        "glass.lsp.completion",
        "glass.lsp.definition",
        "glass.lsp.declaration",
        "glass.lsp.implementation",
        "glass.lsp.references",
        "glass.lsp.document_symbols",
        "glass.lsp.workspace_symbols",
        "glass.lsp.inlay_hints",
        "glass.lsp.signature_help",
        "glass.lsp.code_actions",
        "glass.lsp.formatting",
        "glass.lsp.range_formatting",
        "glass.lsp.semantic_tokens",
        "glass.lsp.rename",
        "glass.agent.list",
        "glass.agent.sessions",
        "glass.agent.tree",
        "glass.agent.messages",
        "glass.agent.entries",
        "glass.agent.stats",
        "glass.debug.events",
        "glass.debug.stack",
        "glass.debug.inspect",
        "glass.debug.processes",
        "glass.debug.threads",
        "glass.debug.scopes",
        "glass.debug.variables",
        "glass.debug.evaluate",
        "glass.graph.source",
        "glass.todo.list",
        "glass.task.list",
        "glass.task.wake",
        "glass.task.get",
        "glass.task.inspect",
        "glass.test.results",
        "glass.test.discover",
        "glass.experiment.list",
        "glass.experiment.compare",
        "glass.graph.query",
        "glass.graph.path",
        "glass.graph.explain",
        "glass.replay.list",
        "glass.replay.inspect",
        "glass.replay.diff",
        "glass.process.health",
        "glass.process.ports",
        "glass.browser.state",
        "glass.browser.observe",
        "glass.browser.verify",
        "glass.browser.snapshot",
        "glass.browser.semantic",
        "glass.browser.web_ir",
        "glass.browser.diff",
        "glass.browser.targets",
        "glass.browser.screenshot",
        "glass.browser.remote-view.status",
        "glass.workflow.list",
        "glass.workflow.verify",
        "glass.workflow.record",
        "glass.memory.retrieve",
        "glass.memory.explain",
        "glass.semantic.diff",
        "glass.semantic.links",
    ];
    const MUTATE: &[&str] = &[
        "glass.agent.task-abort",
        "glass.workspace.suspend",
        "glass.workspace.resume",
        "glass.workspace.delete",
        "glass.daemon.start",
        "glass.daemon.stop",
        "glass.project.attach",
        "glass.github.ship",
        "glass.git.stage",
        "glass.git.unstage",
        "glass.git.discard",
        "glass.git.push",
        "glass.git.fetch",
        "glass.git.pull",
        "glass.git.merge",
        "glass.git.rebase",
        "glass.git.commit",
        "glass.todo.write",
        "glass.todo.complete",
        "glass.git.branch.create",
        "glass.git.branch.switch",
        "glass.git.stash.push",
        "glass.git.stash.pop",
        "glass.git.worktree.create",
        "glass.git.worktree.remove",
        "glass.graph.link",
        "glass.test.run",
        "glass.test.run-affected",
        "glass.test.cancel",
        "glass.test.watch",
        "glass.eval.start",
        "glass.eval.execute",
        "glass.eval.cancel",
        "glass.eval.reset",
        "glass.eval.stop",
        "glass.debug.start",
        "glass.lsp.start",
        "glass.lsp.stop",
        "glass.lsp.raw",
        "glass.debug.launch",
        "glass.debug.attach",
        "glass.debug.configuration_done",
        "glass.debug.restart",
        "glass.debug.breakpoint.set",
        "glass.debug.breakpoint.remove",
        "glass.debug.exception.set",
        "glass.debug.continue",
        "glass.debug.pause",
        "glass.debug.step",
        "glass.debug.disconnect",
        "glass.debug.terminate",
        "glass.debug.stop",
        "glass.agent.spawn",
        "glass.agent.resume",
        "glass.agent.delegate",
        "glass.agent.prompt",
        "glass.agent.send",
        "glass.agent.steer",
        "glass.agent.follow-up",
        "glass.agent.abort",
        "glass.agent.compact",
        "glass.agent.model",
        "glass.agent.thinking",
        "glass.agent.new-session",
        "glass.agent.clone-session",
        "glass.agent.rewind",
        "glass.agent.fork",
        "glass.agent.switch-session",
        "glass.agent.name",
        "glass.task.create",
        "glass.task.crew",
        "glass.task.pause",
        "glass.task.resume",
        "glass.task.cancel",
        "glass.task.retry",
        "glass.task.reassign",
        "glass.task.override-blocked",
        "glass.task.evidence",
        "glass.task.verify",
        "glass.experiment.create",
        "glass.experiment.collect",
        "glass.experiment.select",
        "glass.experiment.remove",
        "glass.editor.open",
        "glass.editor.replace",
        "glass.editor.replace_selection",
        "glass.editor.save",
        "glass.editor.comment.add",
        "glass.editor.comment.resolve",
        "glass.editor.proposal.create",
        "glass.editor.proposal.accept",
        "glass.editor.proposal.accept_pack",
        "glass.editor.proposal.reject",
        "glass.editor.checkpoint.create",
        "glass.editor.checkpoint.restore",
        "glass.process.start",
        "glass.command.run",
        "glass.process.stop",
        "glass.process.restart",
        "glass.process.input",
        "glass.process.resize",
        "glass.browser.start",
        "glass.browser.attach",
        "glass.browser.reconnect",
        "glass.browser.stop",
        "glass.browser.target.select",
        "glass.browser.navigate",
        "glass.browser.act",
        "glass.browser.remote-view.open",
        "glass.browser.remote-view.revoke",
        "glass.workflow.run",
        "glass.workflow.pause",
        "glass.workflow.resume",
        "glass.workflow.cancel",
        "glass.memory.forget",
    ];
    READ.iter()
        .map(|name| service_descriptor(name, false))
        .chain(MUTATE.iter().map(|name| service_descriptor(name, true)))
        .collect()
}

/// Glass-owned integrations offered to Core's negotiated mixed-tool runtime.
/// Core retains its native coding catalog and executes it locally.
pub(crate) fn integration_tool_catalog() -> Vec<ToolDescriptor> {
    const PREFIXES: &[&str] = &[
        "glass.browser.",
        "glass.git.",
        "glass.task.",
        "glass.process.",
        "glass.debug.",
    ];
    const ALLOWED: &[&str] = &[
        "glass.github.review",
        "glass.project.attach",
        "glass.workspace.list",
        "glass.workspace.inspect",
        "glass.editor.open",
        "glass.editor.selection",
        "glass.editor.diff",
        "glass.editor.buffers",
        "glass.editor.comments",
        "glass.editor.proposals",
        "glass.editor.fim",
        "glass.editor.comment.add",
        "glass.editor.comment.resolve",
        "glass.editor.proposal.create",
        "glass.lsp.diagnostics",
        "glass.lsp.list",
        "glass.lsp.events",
        "glass.lsp.hover",
        "glass.lsp.completion",
        "glass.lsp.definition",
        "glass.lsp.declaration",
        "glass.lsp.implementation",
        "glass.lsp.references",
        "glass.lsp.document_symbols",
        "glass.lsp.workspace_symbols",
        "glass.lsp.inlay_hints",
        "glass.lsp.signature_help",
        "glass.lsp.semantic_tokens",
    ];
    let mut catalog = service_descriptors()
        .into_iter()
        .filter(|descriptor| {
            ALLOWED.contains(&descriptor.name.as_str())
                || PREFIXES
                    .iter()
                    .any(|prefix| descriptor.name.starts_with(prefix))
        })
        .collect::<Vec<_>>();
    // Put the common workspace routes first so a host UI or model that only
    // shows the start of a large mixed catalog still exposes the tools needed
    // to inspect and act on the current project.
    const COMMON_FIRST: &[&str] = &[
        "glass.workspace.list",
        "glass.workspace.inspect",
        "glass.browser.state",
        "glass.browser.start",
        "glass.browser.navigate",
        "glass.browser.observe",
        "glass.browser.act",
        "glass.browser.targets",
        "glass.browser.screenshot",
        "glass.git.status",
        "glass.git.diff",
        "glass.git.branches",
        "glass.git.stage",
        "glass.git.unstage",
        "glass.git.commit",
        "glass.process.list",
        "glass.process.logs",
        "glass.process.ports",
        "glass.process.health",
        "glass.process.start",
        "glass.process.stop",
        "glass.process.restart",
        "glass.process.input",
        "glass.debug.inspect",
        "glass.debug.events",
        "glass.debug.threads",
        "glass.debug.stack",
        "glass.debug.scopes",
        "glass.debug.variables",
        "glass.task.list",
        "glass.task.get",
        "glass.task.inspect",
        "glass.editor.open",
        "glass.editor.selection",
        "glass.editor.buffers",
        "glass.lsp.diagnostics",
        "glass.lsp.list",
    ];
    catalog.sort_by_key(|descriptor| {
        COMMON_FIRST
            .iter()
            .position(|name| *name == descriptor.name)
            .unwrap_or(COMMON_FIRST.len())
    });
    catalog
}

pub(crate) fn is_integration_tool(name: &str) -> bool {
    integration_tool_catalog()
        .iter()
        .any(|descriptor| descriptor.name == name)
}

pub(crate) fn tool_requires_mutation(name: &str) -> bool {
    static MUTATING: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        AgentToolGateway::default()
            .descriptors()
            .into_iter()
            .chain(service_descriptors())
            .filter(|descriptor| descriptor.mutating)
            .map(|descriptor| descriptor.name)
            .collect()
    });
    MUTATING.iter().any(|candidate| candidate == name)
}

fn confined_worktree_path(workspace_root: &Path, path: &Path) -> DevelopmentResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(DevelopmentError::InvalidInput(
            "worktree path must be a non-empty relative or workspace worktree path".into(),
        ));
    }
    let base = workspace_root.join(".glass").join("worktrees");
    std::fs::create_dir_all(&base)?;
    let canonical_base = base.canonicalize()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(DevelopmentError::PathOutsideWorkspace(path.to_path_buf()));
    } else {
        base.join(path)
    };
    let name = candidate.file_name().ok_or_else(|| {
        DevelopmentError::InvalidInput("worktree path must name a directory".into())
    })?;
    if name == "." || name == ".." {
        return Err(DevelopmentError::InvalidInput(
            "worktree path must name a directory".into(),
        ));
    }
    let parent = candidate.parent().ok_or_else(|| {
        DevelopmentError::InvalidInput("worktree path must name a directory".into())
    })?;
    let mut existing_parent = parent;
    while !existing_parent.exists() {
        existing_parent = existing_parent
            .parent()
            .ok_or_else(|| DevelopmentError::PathOutsideWorkspace(candidate.clone()))?;
    }
    let canonical_existing = existing_parent.canonicalize()?;
    if !canonical_existing.starts_with(&canonical_base) {
        return Err(DevelopmentError::PathOutsideWorkspace(candidate));
    }
    std::fs::create_dir_all(parent)?;
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_base) {
        return Err(DevelopmentError::PathOutsideWorkspace(candidate));
    }
    Ok(canonical_parent.join(name))
}

fn lsp_debug_schema(name: &str) -> Option<serde_json::Value> {
    use serde_json::json;

    let mut properties = serde_json::Map::new();
    let mut required: Vec<String> = Vec::new();
    let mut add = |key: &str, value: serde_json::Value, is_required: bool| {
        properties.insert(key.to_string(), value);
        if is_required {
            required.push(key.to_string());
        }
    };
    let string = json!({"type":"string", "minLength":1});
    let integer = json!({"type":"integer", "minimum":0});
    match name {
        "glass.lsp.start" => {
            add("server", string.clone(), true);
            add("command", string.clone(), false);
            add(
                "arguments",
                json!({"type":"array", "items":{"type":"string"}}),
                false,
            );
        }
        "glass.lsp.stop" => add("server", string.clone(), true),
        "glass.lsp.events" => add("since", integer.clone(), false),
        "glass.lsp.raw" => {
            add("server", string.clone(), true);
            add("method", string.clone(), true);
            add("params", json!({"type":"object"}), false);
            add("timeoutSeconds", integer.clone(), false);
        }
        name if name.starts_with("glass.lsp.") => {
            let operation = name.trim_start_matches("glass.lsp.");
            if matches!(operation, "list") {
                return Some(
                    json!({"type":"object", "properties":{}, "additionalProperties":false}),
                );
            }
            add("server", string.clone(), true);
            add(
                "path",
                string.clone(),
                !matches!(operation, "workspace_symbols"),
            );
            match operation {
                "workspace_symbols" => add("query", string.clone(), true),
                "hover" | "completion" => {
                    add("line", json!({"type":"integer", "minimum":1}), false);
                    add("character", json!({"type":"integer", "minimum":1}), false);
                }
                "definition" | "declaration" | "implementation" | "references"
                | "signature_help" => {
                    add("line", integer.clone(), true);
                    add("character", integer.clone(), true);
                }
                "rename" => {
                    add("line", integer.clone(), true);
                    add("character", integer.clone(), true);
                    add("newName", string.clone(), true);
                }
                "code_actions" | "range_formatting" => {
                    add(
                        "start",
                        json!({"type":"object", "required":["line","character"], "properties":{"line":integer.clone(),"character":integer.clone()}, "additionalProperties":false}),
                        true,
                    );
                    add(
                        "end",
                        json!({"type":"object", "required":["line","character"], "properties":{"line":integer.clone(),"character":integer.clone()}, "additionalProperties":false}),
                        true,
                    );
                    if operation == "code_actions" {
                        add(
                            "diagnostics",
                            json!({"type":"array", "items":{"type":"object"}}),
                            false,
                        );
                    }
                }
                _ => {}
            }
        }
        name if name.starts_with("glass.debug.") => {
            let operation = name.trim_start_matches("glass.debug.");
            if matches!(operation, "list") {
                return Some(
                    json!({"type":"object", "properties":{}, "additionalProperties":false}),
                );
            }
            add("session", string.clone(), true);
            match operation {
                "start" => {
                    add("command", string.clone(), false);
                    add(
                        "arguments",
                        json!({"type":"array", "items":{"type":"string"}}),
                        false,
                    );
                    add("tcpAddress", string.clone(), false);
                    add("connectTimeoutMs", integer.clone(), false);
                    add("timeoutSeconds", integer.clone(), false);
                }
                "launch" | "attach" | "restart" => add(
                    "configuration",
                    json!({"type":"object"}),
                    operation != "restart",
                ),
                "breakpoint.set" => {
                    add("path", string.clone(), true);
                    add(
                        "lines",
                        json!({"type":"array", "minItems":1, "items":{"type":"integer", "minimum":0}}),
                        true,
                    );
                }
                "breakpoint.remove" => add("path", string.clone(), true),
                "exception.set" => add(
                    "filters",
                    json!({"type":"array", "items":{"type":"string"}}),
                    false,
                ),
                "continue" | "pause" => add("threadId", integer.clone(), true),
                "step" => {
                    add("threadId", integer.clone(), true);
                    add(
                        "kind",
                        json!({"type":"string", "enum":["over","in","out"]}),
                        true,
                    );
                }
                "stack" => add("threadId", integer.clone(), true),
                "scopes" => add("frameId", integer.clone(), true),
                "variables" => add("variablesReference", integer.clone(), true),
                "evaluate" => {
                    add("expression", string.clone(), true);
                    add("frameId", integer.clone(), false);
                    add("context", string.clone(), false);
                }
                "disconnect" => add("terminateDebuggee", json!({"type":"boolean"}), false),
                "terminate" => add("restart", json!({"type":"boolean"}), false),
                _ => {}
            }
        }
        _ => return None,
    }
    Some(json!({
        "type":"object",
        "properties":properties,
        "required":required,
        "additionalProperties":false
    }))
}

fn service_descriptor(name: &str, mutating: bool) -> ToolDescriptor {
    let (description, input_schema) = if let Some((description, schema)) =
        browser_tool_details(name)
    {
        (description.to_string(), schema)
    } else if let Some((description, schema)) = process_tool_details(name) {
        (description.to_string(), schema)
    } else if name == "glass.agent.commands" {
        (
            "Discover the current slash-command catalogue from the selected Glass Core session"
                .into(),
            serde_json::json!({"type":"object","properties":{"agentId":{"type":"string","minLength":1,"maxLength":64}},"additionalProperties":false}),
        )
    } else if name == "glass.agent.command" {
        ("Dispatch a Glass Core slash command; the Core reports whether it runs in-session or needs its native command UI".into(), serde_json::json!({"type":"object","properties":{"agentId":{"type":"string","minLength":1,"maxLength":64},"name":{"type":"string","minLength":1,"maxLength":128},"arguments":{"type":"string","maxLength":16384}},"required":["name"],"additionalProperties":false}))
    } else if name == "glass.agent.task-graph" {
        (
            "Read Glass Core's durable live-task graph for the configured runtime storage".into(),
            serde_json::json!({"type":"object","properties":{"agentId":{"type":"string","minLength":1,"maxLength":64}},"additionalProperties":false}),
        )
    } else if name == "glass.agent.task-abort" {
        (
            "Request cancellation of a Glass Core durable task by task ID".into(),
            serde_json::json!({"type":"object","properties":{"agentId":{"type":"string","minLength":1,"maxLength":64},"taskId":{"type":"string","minLength":1,"maxLength":128},"includeBackground":{"type":"boolean"}},"required":["taskId"],"additionalProperties":false}),
        )
    } else if name == "glass.agent.resume" {
        (
            "Explicitly reattach a recovery-pending Glass thread to its stored Core session"
                .to_string(),
            serde_json::json!({
                "type":"object",
                "properties":{"agentId":{"type":"string","minLength":1,"maxLength":64}},
                "required":["agentId"],
                "additionalProperties":false
            }),
        )
    } else if name == "glass.agent.send" {
        (
            "Send a user message to the resident Glass Agent session".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "minLength": 1, "maxLength": 65536},
                    "mode": {"type": "string", "enum": ["prompt", "follow-up", "steer"]},
                    "agentId": {"type": "string", "minLength": 1, "maxLength": 64},
                    "expectedRunId": {"type": "string", "minLength": 1, "maxLength": 64},
                    "context": {"type": "object"},
                    "intent": {"type": "string", "maxLength": 2048},
                    "verify": {"type": "object"},
                    "clientMessageId": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 128,
                        "pattern": "^[A-Za-z0-9._:-]+$"
                    }
                },
                "required": ["text"],
                "additionalProperties": false
            }),
        )
    } else if name == "glass.command.run" {
        (
            "Run one bounded project command and return its terminal result".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type":"string", "minLength":1, "maxLength":128},
                    "command": {"type":"string", "minLength":1, "maxLength":8192},
                    "timeoutSeconds": {"type":"integer", "minimum":1, "maximum":300}
                },
                "required": ["name", "command"],
                "additionalProperties": false
            }),
        )
    } else if name == "glass.agent.delegate" {
        (
            "Delegate one bounded prompt to a temporary installed external agent".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "harness": {"type": "string", "enum": ["codex", "claude", "opencode"]},
                    "prompt": {"type": "string", "minLength": 1, "maxLength": 65536},
                    "sandbox": {"type": "string", "enum": ["read-only", "workspace-write"]},
                    "timeoutSeconds": {"type": "integer", "minimum": 1, "maximum": 3600}
                },
                "required": ["harness", "prompt"],
                "additionalProperties": false
            }),
        )
    } else if name == "glass.eval.start" {
        (
            "Start a persistent kernel with an explicit Glass capability allowlist".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 128,
                        "pattern": "^[A-Za-z0-9._-]+$"
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["python", "javascript", "shell", "sql"]
                    },
                    "capabilities": {
                        "type": "array",
                        "default": [],
                        "maxItems": 64,
                        "items": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": 128,
                            "pattern": "^[A-Za-z0-9._-]+$",
                            "not": {"pattern": "^glass\\.eval\\."}
                        }
                    },
                    "mutationAuthority": {
                        "type": "boolean",
                        "default": false,
                        "description": "Explicitly grants this kernel mutating capabilities. It does not replace workspace mutation authority or per-call confirmation, which the router checks independently."
                    }
                },
                "required": ["name", "kind"],
                "additionalProperties": false
            }),
        )
    } else if let Some(schema) = lsp_debug_schema(name) {
        let description = if name.starts_with("glass.debug.") {
            debug_tool_description(name)
        } else {
            format!("Resident Glass Dev language operation {name}")
        };
        (description, schema)
    } else {
        (
            format!("Resident Glass Dev operation {name}"),
            serde_json::json!({"type": "object"}),
        )
    };
    ToolDescriptor {
        name: name.into(),
        description,
        input_schema,
        mutating,
        available: true,
        unavailable_reason: None,
    }
}

fn process_tool_details(name: &str) -> Option<(&'static str, serde_json::Value)> {
    use serde_json::json;

    let text = |max_length| json!({"type":"string", "minLength":1, "maxLength":max_length});
    let named = || json!({"type":"object", "properties":{"name":text(128)}, "required":["name"], "additionalProperties":false});
    Some(match name {
        "glass.process.list" => (
            "List named long-running processes owned by this project, including state and working directory. This is the Glass workspace process manager, separate from Core's one-shot native bash tool.",
            json!({"type":"object", "properties":{}, "additionalProperties":false}),
        ),
        "glass.process.ports" => (
            "List project processes and their detected local service URLs/ports; this does not expose the service publicly.",
            json!({"type":"object", "properties":{}, "additionalProperties":false}),
        ),
        "glass.process.logs" => (
            "Read the bounded output captured from one named project process.",
            named(),
        ),
        "glass.process.health" => (
            "Inspect one named project process's state, health, and detected local URLs.",
            named(),
        ),
        "glass.process.start" => (
            "Start a named long-running project process in the project directory; use this for a development server or watcher whose state and output Glass should track. For one-shot commands, use Core's native bash tool.",
            json!({"type":"object", "properties":{"name":text(128), "command":text(8192)}, "required":["name","command"], "additionalProperties":false}),
        ),
        "glass.process.stop" => ("Stop one named project process managed by Glass.", named()),
        "glass.process.restart" => (
            "Restart one named project process managed by Glass.",
            named(),
        ),
        "glass.process.input" => (
            "Send input to the PTY of one named project process; this targets that existing process, not a new shell.",
            json!({"type":"object", "properties":{"name":text(128), "input":{"type":"string", "maxLength":8192}}, "required":["name","input"], "additionalProperties":false}),
        ),
        "glass.process.resize" => (
            "Resize the PTY attached to one named project process.",
            json!({"type":"object", "properties":{"name":text(128), "cols":{"type":"integer", "minimum":1, "maximum":65535}, "rows":{"type":"integer", "minimum":1, "maximum":65535}}, "required":["name","cols","rows"], "additionalProperties":false}),
        ),
        _ => return None,
    })
}

fn debug_tool_description(name: &str) -> String {
    let operation = name.strip_prefix("glass.debug.").unwrap_or(name);
    match operation {
        "start" => "Start a configured DAP adapter for a named debug session. Provide command/arguments or a matching glass.toml DAP entry; starting the adapter does not launch the debuggee.".into(),
        "launch" => "Launch the debuggee through the already started DAP session using its launch configuration.".into(),
        "attach" => "Attach the already started DAP session to a debuggee using its attach configuration.".into(),
        "configuration_done" => "Tell the DAP session that initial breakpoint and exception configuration is complete.".into(),
        "restart" => "Restart the debuggee in the named DAP session, optionally supplying a launch configuration.".into(),
        "breakpoint.set" => "Set source breakpoints in a project file for the named DAP session.".into(),
        "breakpoint.remove" => "Remove source breakpoints from a project file for the named DAP session.".into(),
        "exception.set" => "Configure exception filters for the named DAP session.".into(),
        "continue" => "Continue one paused debug thread in the named DAP session.".into(),
        "pause" => "Pause one running debug thread in the named DAP session.".into(),
        "step" => "Step one paused debug thread over, into, or out of the current statement.".into(),
        "threads" => "List threads in the named DAP session.".into(),
        "stack" => "Read one debug thread's stack trace in the named DAP session.".into(),
        "scopes" => "List scopes available to a debug stack frame.".into(),
        "variables" => "Read variables for a DAP variables reference.".into(),
        "evaluate" => "Evaluate an expression in the selected debug frame or REPL context.".into(),
        "events" => "Read pending events from the named DAP session.".into(),
        "inspect" => "Read the current debugger session snapshot, including state and selected resources.".into(),
        "processes" => "List debuggee processes reported by the named DAP session.".into(),
        "disconnect" => "Disconnect the named DAP session, optionally terminating its debuggee.".into(),
        "terminate" => "Terminate the debuggee for the named DAP session, optionally requesting restart.".into(),
        "stop" => "Stop the named DAP adapter session.".into(),
        _ => format!("Operate on the configured DAP debug session with {operation}."),
    }
}

fn browser_tool_details(name: &str) -> Option<(&'static str, serde_json::Value)> {
    use serde_json::json;

    let object = json!({"type":"object","additionalProperties":false});
    Some(match name {
        "glass.browser.start" => (
            "Start Glass Dev's native browser engine by default. Set backend=chromium only for a task that needs the supported Chromium/CDP backend; selection is explicit and failures never switch backends. Managed Chromium defaults to port 9222 and selects a loopback ephemeral port only if that default is occupied and port is omitted. Explicit port and attach requests stay exact. If a different backend is already connected, call glass.browser.stop first; the conflict response explains the required transition. Use glass.browser.state to confirm backend provenance.",
            json!({
                "type":"object",
                "properties":{
                    "backend":{"type":"string","enum":["native","chromium"],"default":"native"},
                    "incognito":{"type":"boolean","default":true},
                    "profile":{"type":"string","minLength":1},
                    "viewport":{"type":"array","items":{"type":"integer","minimum":1},"minItems":2,"maxItems":2},
                    "headed":{"type":"boolean"},
                    "chromePath":{"type":"string","minLength":1},
                    "port":{"type":"integer","minimum":1,"maximum":65535},
                    "interaction":{"type":"string","enum":["human","fast"]},
                    "audit":{"type":"boolean"},
                    "modalDialogs":{"type":"boolean","default":false}
                },
                "additionalProperties":false
            }),
        ),
        "glass.browser.attach" => (
            "Explicitly attach Glass Dev to an existing Chromium/CDP browser. Use only when that browser is requested; this is separate from selecting backend=chromium to launch a managed Chromium session, and neither path is an automatic native-engine fallback.",
            json!({
                "type":"object",
                "properties":{
                    "port":{"type":"integer","minimum":1,"maximum":65535},
                    "incognito":{"type":"boolean"},
                    "headed":{"type":"boolean"},
                    "profile":{"type":"string","minLength":1},
                    "chromePath":{"type":"string","minLength":1},
                    "viewport":{"type":"array","items":{"type":"integer","minimum":1},"minItems":2,"maxItems":2},
                    "interaction":{"type":"string","enum":["human","fast"]},
                    "audit":{"type":"boolean"}
                },
                "additionalProperties":false
            }),
        ),
        "glass.browser.state" => (
            "Read the resident Glass browser connection and its actual backend provenance (native or chromium).",
            object.clone(),
        ),
        "glass.browser.observe" => (
            "Observe the current page through the connected Glass browser backend and return structured page evidence plus backend provenance.",
            object.clone(),
        ),
        "glass.browser.snapshot" => (
            "Capture the current page snapshot through the connected Glass browser backend.",
            object.clone(),
        ),
        "glass.browser.semantic" => (
            "Inspect the current page with the connected backend's semantic observation support.",
            json!({"type":"object","properties":{"level":{"type":"string","enum":["summary","interactive","structured","detailed","raw"]}},"additionalProperties":false}),
        ),
        "glass.browser.web_ir" => (
            "Extract Web IR from the current page through the connected Glass browser backend.",
            object.clone(),
        ),
        "glass.browser.diff" => (
            "Return changes since the prior observation when supported by the connected backend.",
            object.clone(),
        ),
        "glass.browser.targets" => (
            "List page targets known to the connected Glass browser backend.",
            object.clone(),
        ),
        "glass.browser.target.select" => (
            "Select a page target in the connected Glass browser backend.",
            json!({"type":"object","properties":{"targetId":{"type":"string","minLength":1}},"required":["targetId"],"additionalProperties":false}),
        ),
        "glass.browser.navigate" => (
            "Navigate the connected Glass browser backend to a URL. If browserRevision is omitted, Glass uses the worker's latest observed revision; when supplied it is checked exactly, and a stale value is rejected. glass.browser.start uses the native engine; backend provenance is returned with the result.",
            json!({"type":"object","properties":{"url":{"type":"string","minLength":1},"browserRevision":{"type":"integer","minimum":0},"timeoutSeconds":{"type":"integer","minimum":1,"maximum":180}},"required":["url"],"additionalProperties":false}),
        ),
        "glass.browser.act" => (
            "Perform a supported browser action against the current revision using the connected Glass browser backend. If browserRevision is omitted, Glass uses the worker's latest observed revision; when supplied it is checked exactly, and a stale value is rejected. Unsupported native operations return an explicit error without switching backends.",
            json!({"type":"object","properties":{"action":{"type":"string","enum":["back","forward","reload","stopLoading","click","type","scroll"]},"target":{"type":"string"},"text":{"type":"string"},"dx":{"type":"number"},"dy":{"type":"number"},"browserRevision":{"type":"integer","minimum":0}},"required":["action"],"additionalProperties":false}),
        ),
        "glass.browser.screenshot" => (
            "Capture the current browser page as PNG using the connected backend.",
            object.clone(),
        ),
        "glass.browser.stop" => (
            "Stop and disconnect the resident Glass browser session.",
            object.clone(),
        ),
        "glass.browser.reconnect" => (
            "Reconnect using the last explicit Glass browser configuration and backend.",
            object.clone(),
        ),
        "glass.browser.remote-view.open" => (
            "Open a loopback-only remote view for the connected browser session.",
            object.clone(),
        ),
        "glass.browser.remote-view.revoke" => {
            ("Revoke the current remote browser view.", object.clone())
        }
        "glass.browser.remote-view.status" => {
            ("Read remote browser view availability.", object.clone())
        }
        "glass.browser.verify" => (
            "Verify a predicate against the current browser page using the connected backend.",
            json!({"type":"object","properties":{"predicate":{"type":"object"},"timeoutSeconds":{"type":"integer","minimum":1,"maximum":30}},"required":["predicate"],"additionalProperties":false}),
        ),
        _ => return None,
    })
}

fn required_string<'a>(call: &'a ToolCall, name: &str) -> DevelopmentResult<&'a str> {
    call.arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 64 * 1024 && !value.contains('\0'))
        .ok_or_else(|| {
            DevelopmentError::InvalidInput(format!("{} requires bounded string {name}", call.name))
        })
}

fn validate_expected_run_id(expected: &str, actual: &str) -> DevelopmentResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(DevelopmentError::Conflict(
            "follow-up destination changed; review the queue item before sending".into(),
        ))
    }
}

fn optional_string<'a>(call: &'a ToolCall, name: &str) -> Option<&'a str> {
    call.arguments.get(name).and_then(Value::as_str)
}

fn required_value<'a>(call: &'a ToolCall, name: &str) -> DevelopmentResult<&'a Value> {
    call.arguments
        .get(name)
        .ok_or_else(|| DevelopmentError::InvalidInput(format!("{} requires {name}", call.name)))
}

fn value_map(
    call: &ToolCall,
    name: &str,
) -> DevelopmentResult<std::collections::BTreeMap<String, Value>> {
    match call.arguments.get(name) {
        None => Ok(std::collections::BTreeMap::new()),
        Some(value) => serde_json::from_value(value.clone()).map_err(DevelopmentError::from),
    }
}

fn boolean(call: &ToolCall, name: &str, default: bool) -> bool {
    call.arguments
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn unsigned(call: &ToolCall, name: &str, default: u64) -> DevelopmentResult<u64> {
    let value = call
        .arguments
        .get(name)
        .and_then(Value::as_u64)
        .unwrap_or(default);
    (value <= i64::MAX as u64).then_some(value).ok_or_else(|| {
        DevelopmentError::InvalidInput(format!("{}.{} is out of range", call.name, name))
    })
}

fn optional_unsigned(call: &ToolCall, name: &str) -> DevelopmentResult<Option<u64>> {
    match call.arguments.get(name) {
        None => Ok(None),
        Some(value) => {
            let value = value.as_u64().ok_or_else(|| {
                DevelopmentError::InvalidInput(format!(
                    "{}.{} must be a non-negative integer",
                    call.name, name
                ))
            })?;
            (value <= i64::MAX as u64)
                .then_some(Some(value))
                .ok_or_else(|| {
                    DevelopmentError::InvalidInput(format!(
                        "{}.{} is out of range",
                        call.name, name
                    ))
                })
        }
    }
}

fn integer(call: &ToolCall, name: &str) -> DevelopmentResult<i64> {
    call.arguments
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| DevelopmentError::InvalidInput(format!("{} requires {name}", call.name)))
}

fn u32_value(call: &ToolCall, name: &str) -> DevelopmentResult<u32> {
    u32::try_from(unsigned(call, name, 0)?).map_err(|_| {
        DevelopmentError::InvalidInput(format!("{}.{} is out of range", call.name, name))
    })
}

fn u16_value(call: &ToolCall, name: &str) -> DevelopmentResult<u16> {
    let value = u16::try_from(unsigned(call, name, 0)?).map_err(|_| {
        DevelopmentError::InvalidInput(format!("{}.{} is out of range", call.name, name))
    })?;
    (value > 0).then_some(value).ok_or_else(|| {
        DevelopmentError::InvalidInput(format!("{}.{} must be non-zero", call.name, name))
    })
}

fn position(
    call: &ToolCall,
    name: &str,
) -> DevelopmentResult<crate::development::DiagnosticPosition> {
    serde_json::from_value(required_value(call, name)?.clone()).map_err(DevelopmentError::from)
}

fn value_array_or_empty(call: &ToolCall, name: &str) -> DevelopmentResult<Vec<Value>> {
    match call.arguments.get(name) {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) if values.len() <= 512 => Ok(values.clone()),
        Some(_) => Err(DevelopmentError::InvalidInput(format!(
            "{}.{} must be an array of at most 512 values",
            call.name, name
        ))),
    }
}

fn number(call: &ToolCall, name: &str, default: f64) -> DevelopmentResult<f64> {
    let value = call
        .arguments
        .get(name)
        .and_then(Value::as_f64)
        .unwrap_or(default);
    value.is_finite().then_some(value).ok_or_else(|| {
        DevelopmentError::InvalidInput(format!("{}.{} must be finite", call.name, name))
    })
}

fn string_array(call: &ToolCall, name: &str) -> DevelopmentResult<Vec<String>> {
    let values = call
        .arguments
        .get(name)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= 256)
        .ok_or_else(|| DevelopmentError::InvalidInput(format!("{} requires {name}", call.name)))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= 4096 && !value.contains('\0'))
                .map(str::to_string)
                .ok_or_else(|| DevelopmentError::InvalidInput(format!("invalid {name} item")))
        })
        .collect()
}

fn string_array_or_empty(call: &ToolCall, name: &str) -> DevelopmentResult<Vec<String>> {
    if call.arguments.get(name).is_none() {
        Ok(Vec::new())
    } else {
        string_array(call, name)
    }
}

fn unsigned_array(call: &ToolCall, name: &str) -> DevelopmentResult<Vec<u64>> {
    call.arguments
        .get(name)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= 512)
        .ok_or_else(|| DevelopmentError::InvalidInput(format!("{} requires {name}", call.name)))?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .filter(|value| *value > 0)
                .ok_or_else(|| DevelopmentError::InvalidInput(format!("invalid {name} item")))
        })
        .collect()
}

fn timeout(call: &ToolCall, maximum_seconds: u64) -> DevelopmentResult<Option<Duration>> {
    call.arguments
        .get("timeoutSeconds")
        .map(|_| unsigned(call, "timeoutSeconds", maximum_seconds))
        .transpose()?
        .map(|seconds| {
            if seconds == 0 || seconds > maximum_seconds {
                Err(DevelopmentError::InvalidInput(format!(
                    "timeoutSeconds must be 1..={maximum_seconds}"
                )))
            } else {
                Ok(Duration::from_secs(seconds))
            }
        })
        .transpose()
}

fn parse_kernel_kind(value: &str) -> DevelopmentResult<KernelKind> {
    match value {
        "python" => Ok(KernelKind::Python),
        "javascript" => Ok(KernelKind::JavaScript),
        "shell" => Ok(KernelKind::Shell),
        "sql" => Ok(KernelKind::Sql),
        _ => Err(DevelopmentError::InvalidInput(
            "kernel kind must be python, javascript, shell, or sql".into(),
        )),
    }
}

fn normalized_actor_id(value: &str) -> String {
    let normalized: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || "-_.".contains(*character))
        .take(128)
        .collect();
    if normalized.is_empty() {
        "actor".into()
    } else {
        normalized
    }
}

fn agent_id(
    workspace: &mut DevelopmentWorkspace,
    value: &str,
) -> DevelopmentResult<crate::agents::AgentId> {
    workspace
        .agents()
        .list()?
        .into_iter()
        .find(|snapshot| snapshot.id.as_str() == value)
        .map(|snapshot| snapshot.id)
        .ok_or_else(|| DevelopmentError::NotFound(format!("agent {value}")))
}
fn is_eligible_default_agent(status: crate::agents::AgentStatus) -> bool {
    matches!(
        status,
        crate::agents::AgentStatus::Starting
            | crate::agents::AgentStatus::Idle
            | crate::agents::AgentStatus::Working
            | crate::agents::AgentStatus::Waiting
    )
}

fn default_agent_id(
    candidates: impl IntoIterator<Item = (crate::agents::AgentId, crate::agents::AgentStatus)>,
) -> Option<crate::agents::AgentId> {
    candidates
        .into_iter()
        .find(|(_, status)| is_eligible_default_agent(*status))
        .map(|(id, _)| id)
}

fn active_agent_id(workspace: &mut DevelopmentWorkspace) -> Option<crate::agents::AgentId> {
    workspace
        .agents()
        .list()
        .ok()?
        .into_iter()
        .find(|snapshot| is_eligible_default_agent(snapshot.status))
        .map(|snapshot| snapshot.id)
}

fn agent_request(
    workspace: &mut DevelopmentWorkspace,
    call: &ToolCall,
    request: ResidentSessionRequest,
) -> DevelopmentResult<Value> {
    let id = agent_id(workspace, required_string(call, "agentId")?)?;
    map_service(workspace.agents().request(&id, request))?;
    Ok(serde_json::json!({"queued":true}))
}

fn task_action(
    workspace: &mut DevelopmentWorkspace,
    call: &ToolCall,
    action: impl FnOnce(&mut DevelopmentWorkspace, &TaskId) -> DevelopmentResult<()>,
) -> DevelopmentResult<Value> {
    let id = TaskId::parse(required_string(call, "taskId")?)?;
    action(workspace, &id)?;
    Ok(serde_json::json!({"updated":true}))
}

fn map_service<T: serde::Serialize, E: std::fmt::Display>(
    result: Result<T, E>,
) -> DevelopmentResult<Value> {
    result
        .map_err(|error| DevelopmentError::Process(error.to_string()))
        .and_then(|value| serde_json::to_value(value).map_err(DevelopmentError::from))
}

fn map_debug<T: serde::Serialize>(
    result: crate::debugger::DebugResult<T>,
) -> DevelopmentResult<Value> {
    map_service(result)
}

fn debugger<'a>(
    workspace: &'a mut DevelopmentWorkspace,
    name: &str,
) -> DevelopmentResult<&'a mut crate::debugger::DebuggerSession> {
    workspace
        .debugger_mut(name)
        .map_err(|error| DevelopmentError::Process(error.to_string()))
}

fn git_value<T: serde::Serialize>(
    git: Option<&crate::git::GitService>,
    operation: impl FnOnce(&crate::git::GitService) -> crate::git::GitResult<T>,
) -> DevelopmentResult<Value> {
    map_service(operation(git.ok_or_else(|| {
        DevelopmentError::NotFound("Git repository".into())
    })?))
}

fn git_mutation(
    git: Option<&crate::git::GitService>,
    operation: impl FnOnce(&crate::git::GitService) -> crate::git::GitResult<()>,
) -> DevelopmentResult<()> {
    operation(git.ok_or_else(|| DevelopmentError::NotFound("Git repository".into()))?)
        .map_err(|error| DevelopmentError::Process(error.to_string()))
}

fn project_observable_result(
    workspace: &mut DevelopmentWorkspace,
    call: &ToolCall,
    actor: &str,
    before_revision: u64,
    after_revision: u64,
    result: &Value,
) -> DevelopmentResult<()> {
    let tool_node = format!("tool:{}", call.id);
    let result_bytes = serde_json::to_vec(result)?.len();
    let summary = serde_json::json!({
        "tool":call.name,
        "callId":call.id,
        "beforeRevision":before_revision,
        "afterRevision":after_revision,
        "resultBytes":result_bytes,
    });
    let argument = |name: &str| {
        call.arguments.get(name).and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_u64().map(|value| value.to_string()))
                .or_else(|| value.as_i64().map(|value| value.to_string()))
        })
    };
    let resource_name = |preferred: &[&str], fallback: &str| {
        preferred
            .iter()
            .find_map(|name| argument(name).filter(|value| !value.is_empty()))
            .unwrap_or_else(|| fallback.to_string())
    };
    let mut observed = Vec::<(String, DevelopmentNodeKind, String, String)>::new();

    if call.name.starts_with("glass.file.") || call.name.starts_with("glass.editor.") {
        let path = resource_name(&["path", "from", "to"], &call.id);
        let file = format!("file:{}", id_component(&path));
        observed.push((
            file.clone(),
            DevelopmentNodeKind::File,
            path,
            "referencesFile".into(),
        ));
        if call.name.starts_with("glass.editor.") {
            observed.push((
                format!("editorRevision:{}", id_component(&call.id)),
                DevelopmentNodeKind::EditorRevision,
                format!("editor revision {after_revision}"),
                "producedEditorRevision".into(),
            ));
        }
    }
    if call.name.starts_with("glass.lsp.") {
        let path = resource_name(&["path"], &call.id);
        if call.name == "glass.lsp.diagnostics" || call.name == "glass.diagnostics.run" {
            observed.push((
                format!("diagnostic:{}", id_component(&call.id)),
                DevelopmentNodeKind::Diagnostic,
                path,
                "reportedDiagnostic".into(),
            ));
        } else if !matches!(
            call.name.as_str(),
            "glass.lsp.start" | "glass.lsp.stop" | "glass.lsp.list" | "glass.lsp.events"
        ) {
            observed.push((
                format!("symbol:{}", id_component(&call.id)),
                DevelopmentNodeKind::Symbol,
                path,
                "resolvedSymbol".into(),
            ));
        }
    }
    if call.name.starts_with("glass.git.") {
        let kind = if call.name == "glass.git.commit" {
            DevelopmentNodeKind::GitCommit
        } else {
            DevelopmentNodeKind::GitChange
        };
        observed.push((
            format!("gitChange:{}", id_component(&call.id)),
            kind,
            call.name.clone(),
            "observedGitChange".into(),
        ));
    }
    if call.name.starts_with("glass.process.") {
        let name = resource_name(&["name"], &call.id);
        observed.push((
            format!("process:{}", id_component(&name)),
            DevelopmentNodeKind::Process,
            name,
            "observedProcess".into(),
        ));
        if let Some(port) = find_scalar(result, &["port", "debugPort", "assignedPort"], 0) {
            observed.push((
                format!("port:{}", id_component(&port)),
                DevelopmentNodeKind::Port,
                port,
                "observedPort".into(),
            ));
        }
    }
    if call.name.starts_with("glass.debug.") {
        let session = resource_name(&["session"], &call.id);
        observed.push((
            format!("debugger:{}", id_component(&session)),
            DevelopmentNodeKind::Debugger,
            session.clone(),
            "observedDebugSession".into(),
        ));
        if call.name.contains("breakpoint") {
            observed.push((
                format!("breakpoint:{}", id_component(&call.id)),
                DevelopmentNodeKind::Breakpoint,
                resource_name(&["path"], &call.id),
                "configuredBreakpoint".into(),
            ));
        }
        if let Some(thread) = argument("threadId").or_else(|| find_scalar(result, &["threadId"], 0))
        {
            observed.push((
                format!(
                    "thread:{}:{}",
                    id_component(&session),
                    id_component(&thread)
                ),
                DevelopmentNodeKind::Thread,
                thread,
                "observedThread".into(),
            ));
        }
        if let Some(frame) =
            argument("frameId").or_else(|| find_scalar(result, &["frameId", "id"], 0))
        {
            observed.push((
                format!(
                    "stackFrame:{}:{}",
                    id_component(&session),
                    id_component(&frame)
                ),
                DevelopmentNodeKind::StackFrame,
                frame,
                "observedStackFrame".into(),
            ));
        }
    }
    if call.name.starts_with("glass.browser.") {
        let target = argument("targetId")
            .or_else(|| find_scalar(result, &["targetId", "id"], 0))
            .unwrap_or_else(|| "active".into());
        observed.push((
            format!("browserTarget:{}", id_component(&target)),
            DevelopmentNodeKind::BrowserTarget,
            target,
            "observedBrowserTarget".into(),
        ));
        if let Some(revision) = find_scalar(result, &["revision", "browserRevision"], 0) {
            observed.push((
                format!("browserRevision:{}", id_component(&revision)),
                DevelopmentNodeKind::BrowserRevision,
                revision,
                "observedBrowserRevision".into(),
            ));
        }
    }
    if call.name.starts_with("glass.semantic.")
        || call.name.starts_with("glass.web_ir.")
        || call.name == "glass.browser.web_ir"
    {
        observed.push((
            format!("webIr:{}", id_component(&call.id)),
            DevelopmentNodeKind::WebIrEntity,
            call.name.clone(),
            "observedWebIr".into(),
        ));
    }
    if call.name.starts_with("glass.workflow.") {
        let run = resource_name(&["runId", "id", "name"], &call.id);
        observed.push((
            format!("workflow:{}", id_component(&run)),
            DevelopmentNodeKind::Workflow,
            run,
            "observedWorkflow".into(),
        ));
    }
    if call.name.starts_with("glass.test.") {
        let run = resource_name(&["runId", "suiteId"], &call.id);
        observed.push((
            format!("test:{}", id_component(&run)),
            DevelopmentNodeKind::Test,
            run,
            "observedTest".into(),
        ));
    }
    if call.name.starts_with("glass.eval.") {
        let name = resource_name(&["name"], &call.id);
        observed.push((
            format!("kernel:{}", id_component(&name)),
            DevelopmentNodeKind::Kernel,
            name,
            "observedKernel".into(),
        ));
    }
    if call.name.starts_with("glass.agent.") {
        let agent = argument("agentId")
            .or_else(|| find_scalar(result, &["agentId", "id"], 0))
            .unwrap_or_else(|| call.id.clone());
        observed.push((
            format!("agent:{}", id_component(&agent)),
            DevelopmentNodeKind::Agent,
            agent,
            "observedAgent".into(),
        ));
    }
    if call.name.starts_with("glass.task.") {
        let task = resource_name(&["taskId", "id"], &call.id);
        observed.push((
            format!("task:{}", id_component(&task)),
            DevelopmentNodeKind::Task,
            task,
            "observedTask".into(),
        ));
    }
    if call.name.starts_with("glass.experiment.") {
        let experiment = resource_name(&["id"], &call.id);
        observed.push((
            format!("experiment:{}", id_component(&experiment)),
            DevelopmentNodeKind::Experiment,
            experiment,
            "observedExperiment".into(),
        ));
    }

    let mut previous: Option<String> = None;
    for (id, kind, label, relation) in observed {
        workspace.intelligence_mut().upsert_node(DevelopmentNode {
            id: id.clone(),
            kind,
            label,
            revision: after_revision,
            stale: false,
            evidence: summary.clone(),
        })?;
        workspace.intelligence_mut().link(
            previous.as_deref().unwrap_or(&tool_node),
            &id,
            &relation,
            after_revision,
            actor,
            summary.clone(),
        )?;
        workspace.intelligence_mut().record(ObservableEventInput {
            actor,
            subsystem: subsystem_for_tool(&call.name),
            kind: "observed",
            resource: Some(&id),
            workspace_revision: after_revision,
            evidence: summary.clone(),
            rationale: None,
        })?;
        previous = Some(id);
    }
    Ok(())
}

fn subsystem_for_tool(tool: &str) -> &str {
    tool.strip_prefix("glass.")
        .and_then(|value| value.split('.').next())
        .unwrap_or("project")
}

fn id_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                '_'
            } else {
                character
            }
        })
        .take(240)
        .collect()
}

fn find_scalar(value: &Value, keys: &[&str], depth: usize) -> Option<String> {
    if depth > 6 {
        return None;
    }
    match value {
        Value::Object(values) => {
            for key in keys {
                if let Some(value) = values.get(*key) {
                    if let Some(value) = value.as_str() {
                        return Some(value.to_string());
                    }
                    if let Some(value) = value.as_u64() {
                        return Some(value.to_string());
                    }
                }
            }
            values
                .values()
                .find_map(|value| find_scalar(value, keys, depth + 1))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_scalar(value, keys, depth + 1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::development::Actor;
    use serde_json::json;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn workspace() -> DevelopmentWorkspace {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-tool-router-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        DevelopmentWorkspace::open(root).unwrap()
    }

    fn context(workspace: &DevelopmentWorkspace, mutate: bool) -> DevelopmentToolContext {
        DevelopmentToolContext {
            authorization: ToolAuthorization::from_user_confirmation(
                Actor::embedded(),
                mutate,
                mutate,
            ),
            initiator: None,
            expected_generation: workspace.generation(),
            expected_project_revision: workspace.project().revision(),
        }
    }

    #[test]
    fn resident_browser_catalog_explains_native_default_and_explicit_chromium() {
        let catalog = integration_tool_catalog();
        let start = catalog
            .iter()
            .find(|tool| tool.name == "glass.browser.start")
            .unwrap();
        let attach = catalog
            .iter()
            .find(|tool| tool.name == "glass.browser.attach")
            .unwrap();
        let state = catalog
            .iter()
            .find(|tool| tool.name == "glass.browser.state")
            .unwrap();

        assert!(
            start
                .description
                .contains("native browser engine by default")
        );
        assert_eq!(
            start.input_schema["properties"]["backend"]["default"],
            "native"
        );
        assert_eq!(
            start.input_schema["properties"]["backend"]["enum"][1],
            "chromium"
        );
        assert_eq!(
            start.input_schema["properties"]["incognito"]["default"],
            true
        );
        assert!(attach.description.contains("Chromium/CDP"));
        assert!(
            attach
                .description
                .contains("automatic native-engine fallback")
        );
        assert!(state.description.contains("actual backend provenance"));
    }

    #[test]
    fn core_catalog_exposes_project_process_and_configured_debug_capabilities() {
        let catalog = integration_tool_catalog();
        let position = |name: &str| {
            catalog
                .iter()
                .position(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("missing Core host tool {name}"))
        };
        let descriptor = |name: &str| {
            catalog
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("missing Core host tool {name}"))
        };

        let process_list = descriptor("glass.process.list");
        assert!(process_list.description.contains("project"));
        let process_start = descriptor("glass.process.start");
        assert_eq!(
            process_start.input_schema["required"],
            serde_json::json!(["name", "command"])
        );
        assert!(process_start.description.contains("long-running"));
        assert!(
            process_start
                .description
                .contains("Core's native bash tool")
        );
        let process_logs = descriptor("glass.process.logs");
        assert_eq!(
            process_logs.input_schema["required"],
            serde_json::json!(["name"])
        );
        let process_input = descriptor("glass.process.input");
        assert!(process_input.description.contains("existing process"));

        let debug_start = descriptor("glass.debug.start");
        assert!(debug_start.description.contains("DAP adapter"));
        assert!(
            debug_start
                .description
                .contains("does not launch the debuggee")
        );
        assert_eq!(
            debug_start.input_schema["required"],
            serde_json::json!(["session"])
        );
        let debug_threads = descriptor("glass.debug.threads");
        assert!(debug_threads.description.contains("List threads"));

        assert!(!is_integration_tool("glass.command.run"));
        assert!(position("glass.workspace.inspect") < position("glass.browser.navigate"));
        assert!(position("glass.browser.navigate") < position("glass.git.status"));
        assert!(position("glass.git.status") < position("glass.process.list"));
        assert!(position("glass.process.list") < position("glass.debug.inspect"));
    }

    #[test]
    fn omitted_browser_revision_stays_distinct_from_explicit_zero() {
        let omitted = ToolCall {
            id: "navigate-omitted-revision".into(),
            name: "glass.browser.navigate".into(),
            arguments: json!({"url":"http://127.0.0.1/"}),
        };
        let explicit_zero = ToolCall {
            id: "navigate-explicit-zero".into(),
            name: "glass.browser.navigate".into(),
            arguments: json!({"url":"http://127.0.0.1/","browserRevision":0}),
        };
        assert_eq!(
            optional_unsigned(&omitted, "browserRevision").unwrap(),
            None
        );
        assert_eq!(
            optional_unsigned(&explicit_zero, "browserRevision").unwrap(),
            Some(0)
        );
    }

    #[test]
    fn default_agent_selection_skips_recovered_and_unavailable_threads() {
        assert!(is_eligible_default_agent(crate::AgentStatus::Starting));
        assert!(is_eligible_default_agent(crate::AgentStatus::Idle));
        assert!(is_eligible_default_agent(crate::AgentStatus::Working));
        assert!(is_eligible_default_agent(crate::AgentStatus::Waiting));
        assert!(!is_eligible_default_agent(
            crate::AgentStatus::RecoveryPending
        ));
        assert!(!is_eligible_default_agent(crate::AgentStatus::Unavailable));
        assert!(!is_eligible_default_agent(crate::AgentStatus::Queued));
        assert!(!is_eligible_default_agent(crate::AgentStatus::Completed));
        assert!(!is_eligible_default_agent(crate::AgentStatus::Failed));
        assert!(!is_eligible_default_agent(crate::AgentStatus::Cancelled));
    }

    #[test]
    fn recovery_pending_is_not_an_implicit_send_target_but_explicit_target_is_preserved() {
        let recovered = crate::AgentId::parse("agent-recovered").unwrap();
        let candidates = vec![(recovered.clone(), crate::AgentStatus::RecoveryPending)];

        // With only a recovered thread, a fresh send has no implicit target; the
        // route creates a new eligible agent rather than resuming this session.
        assert_eq!(default_agent_id(candidates.clone()), None);

        // An explicit selection remains explicit and is rejected by AgentRegistry
        // until the user deliberately recovers it. Never silently redirect or replay.
        let explicit = Some(recovered.clone());
        let target = explicit.or_else(|| default_agent_id(candidates));
        assert_eq!(target, Some(recovered));
    }

    #[test]
    fn stale_expected_run_guard_rejects_before_any_restart_action() {
        let current_run = "run-11111111111111111111111111111111";
        let expected_run = "run-22222222222222222222222222222222";
        let mut restart_called = false;

        let guard = validate_expected_run_id(expected_run, current_run);
        if guard.is_ok() {
            restart_called = true;
        }

        assert!(matches!(guard, Err(DevelopmentError::Conflict(_))));
        assert!(!restart_called);
        assert_eq!(current_run, "run-11111111111111111111111111111111");
    }

    #[test]
    fn model_listing_without_an_active_session_returns_actionable_conflict() {
        let mut workspace = workspace();
        let root = workspace.root().to_path_buf();
        let read_only = context(&workspace, false);
        let error = DevelopmentToolRouter::default()
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "models-without-session".into(),
                    name: "glass.agent.models".into(),
                    arguments: json!({}),
                },
                &read_only,
            )
            .expect_err("no resident session should be actionable error");
        assert!(
            error
                .to_string()
                .contains("start an interactive agent session"),
            "{error}"
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_resume_is_registered_and_routes_to_the_registry() {
        let descriptor = service_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == "glass.agent.resume")
            .expect("resume service is advertised");
        assert!(descriptor.mutating);
        assert!(!is_integration_tool("glass.agent.resume"));
        assert_eq!(descriptor.input_schema["required"], json!(["agentId"]));

        let mut workspace = workspace();
        let authorized = context(&workspace, true);
        let error = DevelopmentToolRouter::default()
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "resume-missing".into(),
                    name: "glass.agent.resume".into(),
                    arguments: json!({"agentId":"agent-missing"}),
                },
                &authorized,
            )
            .expect_err("unknown thread must fail through the registered dispatcher");
        assert!(error.to_string().contains("agent"), "{error}");
    }

    #[test]
    fn worktree_paths_are_confined_to_glass_worktrees() {
        let root = std::env::temp_dir().join(format!(
            "glass-worktree-confine-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();

        let relative = confined_worktree_path(&root, Path::new("feature")).unwrap();
        assert!(relative.starts_with(root.join(".glass").join("worktrees")));
        assert_eq!(relative.file_name().unwrap(), "feature");

        let nested = confined_worktree_path(&root, Path::new("nested/leaf")).unwrap();
        assert_eq!(nested.file_name().unwrap(), "leaf");
        assert!(nested.starts_with(root.join(".glass").join("worktrees").join("nested")));

        let allowed_abs = root.join(".glass").join("worktrees").join("abs");
        let confined_abs = confined_worktree_path(&root, &allowed_abs).unwrap();
        assert_eq!(confined_abs.file_name().unwrap(), "abs");

        assert!(confined_worktree_path(&root, Path::new("../escape")).is_err());
        assert!(confined_worktree_path(&root, Path::new("")).is_err());

        let outside = std::env::temp_dir().join(format!(
            "glass-worktree-outside-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let outside_child = outside.join("leaf");
        assert!(confined_worktree_path(&root, &outside_child).is_err());
        assert!(!outside.exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn git_unstage_is_registered_as_a_mutating_tool() {
        let unstage = service_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == "glass.git.unstage")
            .expect("glass.git.unstage must be advertised");
        assert!(unstage.mutating);
        assert!(unstage.available);
        assert!(
            DevelopmentToolRouter::default()
                .descriptors()
                .iter()
                .find(|descriptor| descriptor.name == "glass.lsp.raw")
                .is_some_and(|descriptor| descriptor.mutating)
        );
    }

    #[test]
    fn root_file_listing_with_empty_path_keeps_tool_successful() {
        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let call = ToolCall {
            id: "list-root".into(),
            name: "glass.file.list".into(),
            arguments: json!({"path":""}),
        };
        let read_only = context(&workspace, false);
        let result = router
            .execute(&mut workspace, &call, &read_only)
            .expect("listing the project root must succeed");
        assert!(result["entries"].is_array());
        assert_eq!(
            workspace
                .intelligence()
                .node("file:list-root")
                .unwrap()
                .label,
            "list-root"
        );
    }

    #[test]
    fn core_integration_uses_workspace_authority_without_duplicate_confirmation() {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-core-integration-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='core-integration-test'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut project_workspace = DevelopmentWorkspace::open(&root).unwrap();
        let router = DevelopmentToolRouter::default();
        let call = ToolCall {
            id: "core-review-comment".into(),
            name: "glass.editor.comment.add".into(),
            arguments: json!({
                "path":"src/main.rs",
                "startLine":1,
                "endLine":1,
                "text":"Review this line"
            }),
        };
        let mut core = context(&project_workspace, false);
        core.authorization = ToolAuthorization::glass_core();
        assert_eq!(core.authorization.actor.id, "external:glass-core");
        let mut stale = core.clone();
        stale.expected_project_revision = stale.expected_project_revision.saturating_add(1);
        assert!(
            router
                .execute(&mut project_workspace, &call, &stale)
                .is_err()
        );
        let result = router
            .execute(&mut project_workspace, &call, &core)
            .expect("Core review integration should run under Core authority");
        assert!(result["id"].is_string());

        let create_task = ToolCall {
            id: "core-task-create".into(),
            name: "glass.task.create".into(),
            arguments: json!({
                "title":"Core authorized task",
                "goal":"Verify the Glass integration route",
                "prompt":"Inspect the fixture",
                "worktree":null
            }),
        };
        core.expected_generation = project_workspace.generation();
        core.expected_project_revision = project_workspace.project().revision();
        let task = router
            .execute(&mut project_workspace, &create_task, &core)
            .expect("Core-authorized task creation must dispatch without a Glass prompt");
        assert!(task.is_string());

        let mut external = context(&project_workspace, false);
        external.authorization = ToolAuthorization::read_only(Actor::external("glass-core"));
        assert!(
            router
                .execute(&mut project_workspace, &call, &external)
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();

        let mut trusted = workspace();
        std::fs::create_dir_all(trusted.root().join("src")).unwrap();
        std::fs::write(trusted.root().join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut external = context(&trusted, false);
        external.authorization.actor = Actor::external("mcp-client");
        assert!(router.execute(&mut trusted, &call, &external).is_err());
        std::fs::remove_dir_all(trusted.root()).unwrap();
    }

    #[test]
    fn router_rejects_stale_or_unauthorized_mutations() {
        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let call = ToolCall {
            id: "write-1".into(),
            name: "glass.file.write".into(),
            arguments: serde_json::json!({"path":"src/lib.rs","content":"pub fn ok() {}\n"}),
        };
        let read_only = context(&workspace, false);
        assert!(router.execute(&mut workspace, &call, &read_only).is_err());
        let mut stale = context(&workspace, true);
        stale.expected_project_revision = stale.expected_project_revision.saturating_add(1);
        assert!(router.execute(&mut workspace, &call, &stale).is_err());
        let valid = context(&workspace, true);
        workspace.execute_tool(&call, &valid).unwrap();
        assert!(workspace.root().join("src/lib.rs").exists());
        let replay = workspace.intelligence().replay(0, 16).unwrap();
        assert_eq!(
            replay
                .iter()
                .map(|event| event.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["called", "observed", "completed"]
        );
        assert!(
            workspace
                .intelligence()
                .path("repository:root", "tool:write-1")
                .is_ok()
        );
    }

    #[test]
    fn every_public_service_mutation_route_requires_both_authorization_factors() {
        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let mutating_routes = [
            "glass.agent.spawn",
            "glass.agent.delegate",
            "glass.browser.navigate",
            "glass.browser.remote-view.open",
            "glass.workflow.run",
            "glass.test.run",
            "glass.lsp.start",
            "glass.debug.start",
            "glass.task.create",
            "glass.task.crew",
            "glass.experiment.create",
            "glass.todo.write",
            "glass.memory.forget",
            "glass.process.start",
        ];

        for name in mutating_routes {
            let descriptor = router
                .descriptors()
                .into_iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap_or_else(|| panic!("missing {name} descriptor"));
            assert!(
                descriptor.available,
                "{name} must be available in the fixture"
            );
            assert!(descriptor.mutating, "{name} must be marked mutating");

            let call = ToolCall {
                id: format!("unauthorized-{name}"),
                name: name.into(),
                arguments: json!({}),
            };
            for (allow_mutation, confirmed) in [(false, false), (false, true), (true, false)] {
                let unauthorized = DevelopmentToolContext {
                    authorization: ToolAuthorization::from_user_confirmation(
                        Actor::embedded(),
                        allow_mutation,
                        confirmed,
                    ),
                    initiator: None,
                    expected_generation: workspace.generation(),
                    expected_project_revision: workspace.project().revision(),
                };
                let error = router
                    .execute(&mut workspace, &call, &unauthorized)
                    .expect_err("a mutation route must reject an incomplete factor pair");
                assert!(
                    error.to_string().contains("requires mutation authority"),
                    "{name} rejected for the wrong reason: {error}"
                );
            }
        }
    }

    #[test]
    fn router_uses_resident_kernel_and_test_services() {
        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let start = ToolCall {
            id: "kernel-1".into(),
            name: "glass.eval.start".into(),
            arguments: serde_json::json!({
                "name":"analysis",
                "kind":"sql",
                "capabilities":["glass.file.write"],
                "mutationAuthority":true
            }),
        };
        for (allow_mutation, confirmed) in [(false, false), (false, true), (true, false)] {
            let mut unauthorized = context(&workspace, false);
            unauthorized.authorization = ToolAuthorization::from_user_confirmation(
                Actor::embedded(),
                allow_mutation,
                confirmed,
            );
            assert!(
                router
                    .execute(&mut workspace, &start, &unauthorized)
                    .is_err()
            );
        }
        assert!(workspace.kernels().snapshot("analysis").is_none());
        let mutation = context(&workspace, true);
        router.execute(&mut workspace, &start, &mutation).unwrap();
        let execute = ToolCall {
            id: "kernel-2".into(),
            name: "glass.eval.execute".into(),
            arguments: serde_json::json!({"name":"analysis","code":"SELECT 40 + 2 AS answer"}),
        };
        let result = router.execute(&mut workspace, &execute, &mutation).unwrap();
        assert_eq!(result["value"][0]["answer"], 42);
        assert!(
            router
                .descriptors()
                .iter()
                .any(|descriptor| descriptor.name == "glass.debug.launch")
        );
    }

    #[test]
    fn eval_start_descriptor_exposes_the_separate_kernel_mutation_grant() {
        let descriptor = DevelopmentToolRouter::default()
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == "glass.eval.start")
            .expect("kernel start descriptor");

        assert!(descriptor.mutating);
        assert_eq!(
            descriptor.input_schema["properties"]["kind"]["enum"],
            json!(["python", "javascript", "shell", "sql"])
        );
        assert_eq!(
            descriptor.input_schema["properties"]["capabilities"]["maxItems"],
            json!(64)
        );
        assert_eq!(
            descriptor.input_schema["properties"]["capabilities"]["items"]["not"]["pattern"],
            "^glass\\.eval\\."
        );
        assert_eq!(
            descriptor.input_schema["properties"]["mutationAuthority"]["type"],
            "boolean"
        );
        assert_eq!(
            descriptor.input_schema["properties"]["mutationAuthority"]["default"],
            false
        );
        assert!(
            descriptor.input_schema["properties"]["mutationAuthority"]["description"]
                .as_str()
                .unwrap()
                .contains("does not replace workspace mutation authority or per-call confirmation")
        );
        assert_eq!(descriptor.input_schema["required"], json!(["name", "kind"]));
        assert_eq!(descriptor.input_schema["additionalProperties"], false);
    }

    #[test]
    fn kernel_bindings_reenter_the_same_router_with_dual_provenance() {
        let mut workspace = workspace();
        std::fs::write(workspace.root().join("evidence.txt"), "governed\n").unwrap();
        let router = DevelopmentToolRouter::default();
        let mutation = context(&workspace, true);
        let start = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "kernel-binding-start".into(),
                    name: "glass.eval.start".into(),
                    arguments: serde_json::json!({
                        "name":"analysis",
                        "kind":"sql",
                        "capabilities":["glass.file.read","glass.file.write"],
                        "mutationAuthority":true
                    }),
                },
                &mutation,
            )
            .unwrap();
        assert_eq!(start["mutationAuthority"], true);
        let result = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "kernel-binding-execute".into(),
                    name: "glass.eval.execute".into(),
                    arguments: serde_json::json!({
                        "name":"analysis",
                        "code":r#"GLASS CALL {"id":"sql-1","tool":"glass.file.read","arguments":{"path":"evidence.txt"}}"#
                    }),
                },
                &mutation,
            )
            .unwrap();
        assert_eq!(result["value"]["content"], "governed\n");
        assert_eq!(result["initiatorId"], "embedded:glass-agent");
        assert_eq!(result["executorId"], "kernel:analysis");
        let replay = workspace.intelligence().replay(0, 32).unwrap();
        let nested = replay
            .iter()
            .find(|event| event.resource.as_deref() == Some("tool:kernel-analysis-sql-1"))
            .unwrap();
        assert_eq!(nested.actor, "kernel:analysis");
        assert_eq!(nested.evidence["initiator"], "embedded:glass-agent");
        assert_eq!(nested.evidence["executor"], "kernel:analysis");

        let write = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "kernel-binding-write".into(),
                    name: "glass.eval.execute".into(),
                    arguments: serde_json::json!({
                        "name":"analysis",
                        "code":r#"GLASS CALL {"id":"sql-2","tool":"glass.file.write","arguments":{"path":"allowed.txt","content":"governed write\n"}}"#
                    }),
                },
                &mutation,
            )
            .unwrap();
        assert_eq!(write["value"]["written"], true);
        assert_eq!(
            std::fs::read_to_string(workspace.root().join("allowed.txt")).unwrap(),
            "governed write\n"
        );

        let read_only = context(&workspace, false);
        let denied = router.execute(
            &mut workspace,
            &ToolCall {
                id: "kernel-binding-denied".into(),
                name: "glass.eval.execute".into(),
                arguments: serde_json::json!({
                    "name":"analysis",
                    "code":r#"GLASS CALL {"id":"sql-3","tool":"glass.file.write","arguments":{"path":"denied.txt","content":"no"}}"#
                }),
            },
            &read_only,
        );
        assert!(denied.unwrap_err().to_string().contains("mutation"));
        assert!(!workspace.root().join("denied.txt").exists());
    }

    #[test]
    fn governed_python_queries_browser_tests_and_graph_before_approved_mutation() {
        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let mutation = context(&workspace, true);
        router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "governed-python-start".into(),
                    name: "glass.eval.start".into(),
                    arguments: serde_json::json!({
                        "name":"analysis",
                        "kind":"python",
                        "capabilities":[
                            "glass.browser.state",
                            "glass.test.results",
                            "glass.graph.path"
                        ],
                        "mutationAuthority":false
                    }),
                },
                &mutation,
            )
            .unwrap();

        for (id, code, expected) in [
            (
                "governed-python-browser",
                "glass.call('glass.browser.state')",
                "'connected': False",
            ),
            (
                "governed-python-tests",
                "glass.call('glass.test.results')",
                "results",
            ),
            (
                "governed-python-graph",
                "glass.call('glass.graph.path', {'from': 'repository:root', 'to': 'tool:kernel-analysis-python-1'})",
                "repository:root",
            ),
        ] {
            let result = router
                .execute(
                    &mut workspace,
                    &ToolCall {
                        id: id.into(),
                        name: "glass.eval.execute".into(),
                        arguments: serde_json::json!({"name":"analysis","code":code}),
                    },
                    &mutation,
                )
                .unwrap();
            assert!(
                result["value"].as_str().unwrap().contains(expected),
                "{id} must return governed {expected} evidence: {result}"
            );
            assert_eq!(result["initiatorId"], "embedded:glass-agent");
            assert_eq!(result["executorId"], "kernel:analysis");
        }

        let denied = router.execute(
            &mut workspace,
            &ToolCall {
                id: "governed-python-denied".into(),
                name: "glass.eval.execute".into(),
                arguments: serde_json::json!({
                    "name":"analysis",
                    "code":"glass.call('glass.file.write', {'path': 'denied.py', 'content': 'no'})"
                }),
            },
            &mutation,
        );
        assert!(
            denied
                .unwrap_err()
                .to_string()
                .contains("kernel capability glass.file.write was not granted")
        );
        assert!(!workspace.root().join("denied.py").exists());

        router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "approved-python-start".into(),
                    name: "glass.eval.start".into(),
                    arguments: serde_json::json!({
                        "name":"approved",
                        "kind":"python",
                        "capabilities":["glass.file.write"],
                        "mutationAuthority":true
                    }),
                },
                &mutation,
            )
            .unwrap();
        let approved = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "approved-python-write".into(),
                    name: "glass.eval.execute".into(),
                    arguments: serde_json::json!({
                        "name":"approved",
                        "code":"glass.call('glass.file.write', {'path': 'approved.py', 'content': 'approved\\n'})"
                    }),
                },
                &mutation,
            )
            .unwrap();
        assert_eq!(approved["initiatorId"], "embedded:glass-agent");
        assert_eq!(approved["executorId"], "kernel:approved");
        assert_eq!(
            std::fs::read_to_string(workspace.root().join("approved.py")).unwrap(),
            "approved\n"
        );

        let replay = workspace.intelligence().replay(0, 128).unwrap();
        for resource in [
            "tool:kernel-analysis-python-1",
            "tool:kernel-analysis-python-2",
            "tool:kernel-analysis-python-3",
            "tool:kernel-approved-python-1",
        ] {
            let nested = replay
                .iter()
                .find(|event| event.resource.as_deref() == Some(resource))
                .unwrap_or_else(|| panic!("missing provenance for {resource}"));
            assert_eq!(nested.evidence["initiator"], "embedded:glass-agent");
            assert!(
                nested.evidence["executor"]
                    .as_str()
                    .unwrap()
                    .starts_with("kernel:")
            );
        }
    }

    #[test]
    fn router_exposes_shared_editor_process_browser_and_complete_lsp_tools() {
        let mut workspace = workspace();
        std::fs::create_dir_all(workspace.root().join("src")).unwrap();
        std::fs::write(workspace.root().join("src/lib.rs"), "pub fn old() {}\n").unwrap();
        let router = DevelopmentToolRouter::default();
        let descriptors = router.descriptors();
        for name in [
            "glass.editor.open",
            "glass.editor.selection",
            "glass.editor.replace",
            "glass.editor.replace_selection",
            "glass.editor.save",
            "glass.editor.comments",
            "glass.editor.proposals",
            "glass.editor.fim",
            "glass.editor.checkpoints",
            "glass.editor.comment.add",
            "glass.editor.comment.resolve",
            "glass.editor.proposal.create",
            "glass.editor.proposal.accept",
            "glass.editor.proposal.accept_pack",
            "glass.editor.proposal.reject",
            "glass.editor.checkpoint.create",
            "glass.editor.checkpoint.restore",
            "glass.process.list",
            "glass.process.logs",
            "glass.process.start",
            "glass.github.review",
            "glass.github.ship",
            "glass.process.stop",
            "glass.process.restart",
            "glass.browser.observe",
            "glass.browser.verify",
            "glass.browser.snapshot",
            "glass.browser.attach",
            "glass.browser.reconnect",
            "glass.browser.semantic",
            "glass.browser.web_ir",
            "glass.browser.diff",
            "glass.browser.navigate",
            "glass.browser.act",
            "glass.workflow.list",
            "glass.workflow.run",
            "glass.workflow.pause",
            "glass.workflow.resume",
            "glass.workflow.cancel",
            "glass.workflow.verify",
            "glass.workflow.record",
            "glass.memory.retrieve",
            "glass.memory.explain",
            "glass.memory.forget",
            "glass.semantic.inspect",
            "glass.semantic.diff",
            "glass.semantic.links",
            "glass.lsp.start",
            "glass.lsp.stop",
            "glass.lsp.list",
            "glass.lsp.events",
            "glass.lsp.definition",
            "glass.lsp.declaration",
            "glass.lsp.implementation",
            "glass.lsp.references",
            "glass.lsp.document_symbols",
            "glass.lsp.workspace_symbols",
            "glass.lsp.inlay_hints",
            "glass.lsp.signature_help",
            "glass.lsp.code_actions",
            "glass.lsp.formatting",
            "glass.lsp.range_formatting",
            "glass.lsp.semantic_tokens",
            "glass.lsp.rename",
            "glass.lsp.raw",
            "glass.debug.configuration_done",
            "glass.debug.restart",
            "glass.debug.breakpoint.remove",
            "glass.debug.exception.set",
            "glass.debug.threads",
            "glass.debug.disconnect",
            "glass.debug.terminate",
            "glass.agent.model",
            "glass.agent.thinking",
            "glass.agent.new-session",
            "glass.agent.clone-session",
            "glass.agent.rewind",
            "glass.agent.fork",
            "glass.agent.switch-session",
            "glass.agent.sessions",
            "glass.agent.tree",
            "glass.agent.messages",
            "glass.agent.entries",
            "glass.agent.stats",
            "glass.agent.name",
            "glass.todo.list",
            "glass.todo.write",
            "glass.todo.complete",
            "glass.task.create",
            "glass.task.crew",
            "glass.task.list",
            "glass.task.wake",
            "glass.task.get",
            "glass.task.inspect",
            "glass.task.pause",
            "glass.task.resume",
            "glass.task.cancel",
            "glass.task.retry",
            "glass.task.reassign",
            "glass.task.override-blocked",
            "glass.task.evidence",
            "glass.task.verify",
            "glass.experiment.create",
            "glass.experiment.collect",
            "glass.experiment.list",
            "glass.experiment.compare",
            "glass.experiment.select",
            "glass.experiment.remove",
        ] {
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap_or_else(|| panic!("missing {name}"));
            assert!(
                descriptor.available,
                "{name} must execute, not be a placeholder"
            );
        }
        let semantic_descriptors = descriptors
            .iter()
            .filter(|descriptor| descriptor.name == "glass.semantic.inspect")
            .collect::<Vec<_>>();
        assert_eq!(semantic_descriptors.len(), 1);
        assert_eq!(
            semantic_descriptors[0].input_schema["required"],
            serde_json::json!(["entity"])
        );

        for (id, name, arguments) in [
            (
                "open",
                "glass.editor.open",
                serde_json::json!({"path":"src/lib.rs"}),
            ),
            (
                "replace",
                "glass.editor.replace",
                serde_json::json!({"path":"src/lib.rs","oldText":"old","newText":"resident"}),
            ),
            (
                "save",
                "glass.editor.save",
                serde_json::json!({"path":"src/lib.rs"}),
            ),
        ] {
            let call = ToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            };
            let authorized = context(&workspace, true);
            router.execute(&mut workspace, &call, &authorized).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(workspace.root().join("src/lib.rs")).unwrap(),
            "pub fn resident() {}\n"
        );
    }
    #[test]
    fn editor_selection_tool_returns_text_and_replaces_only_that_range() {
        let mut workspace = workspace();
        std::fs::create_dir_all(workspace.root().join("src")).unwrap();
        std::fs::write(workspace.root().join("src/lib.rs"), "alpha\nbeta\n").unwrap();
        let router = DevelopmentToolRouter::default();
        let authorized = context(&workspace, true);
        router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "selection-open".into(),
                    name: "glass.editor.open".into(),
                    arguments: serde_json::json!({"path":"src/lib.rs"}),
                },
                &authorized,
            )
            .unwrap();
        workspace
            .project_mut()
            .set_buffer_selection(
                "src/lib.rs",
                Some(crate::development::TextSelection {
                    anchor: crate::development::TextPosition { line: 1, column: 2 },
                    active: crate::development::TextPosition { line: 2, column: 3 },
                }),
                crate::development::Actor::local(),
            )
            .unwrap();
        let read_only = context(&workspace, false);
        let selection = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "selection-read".into(),
                    name: "glass.editor.selection".into(),
                    arguments: serde_json::json!({"path":"src/lib.rs"}),
                },
                &read_only,
            )
            .unwrap();
        assert_eq!(selection["selectedText"], "lpha\nbe");
        let authorized = context(&workspace, true);
        router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "selection-replace".into(),
                    name: "glass.editor.replace_selection".into(),
                    arguments: serde_json::json!({
                        "path":"src/lib.rs",
                        "replacement":"X\nY"
                    }),
                },
                &authorized,
            )
            .unwrap();
        assert_eq!(
            workspace.project().buffer("src/lib.rs").unwrap().content,
            "aX\nYta\n"
        );
        assert!(
            workspace
                .project()
                .buffer("src/lib.rs")
                .unwrap()
                .selection
                .is_none()
        );
    }

    #[test]
    fn live_web_ir_route_is_read_only_and_requires_browser() {
        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let descriptor = router
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == "glass.browser.web_ir")
            .expect("live Web IR descriptor");
        assert!(descriptor.available);
        assert!(!descriptor.mutating);

        let read_only = context(&workspace, false);
        let error = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "web-ir-disconnected".into(),
                    name: "glass.browser.web_ir".into(),
                    arguments: json!({}),
                },
                &read_only,
            )
            .unwrap_err();
        assert!(error.to_string().contains("glass.browser.start"));
    }

    #[test]
    fn observable_projection_populates_every_issue_35_graph_resource_kind() {
        let mut workspace = workspace();
        let samples = vec![
            ("glass.editor.save", json!({"path":"src/lib.rs"}), json!({})),
            ("glass.lsp.hover", json!({"path":"src/lib.rs"}), json!({})),
            (
                "glass.lsp.diagnostics",
                json!({"path":"src/lib.rs"}),
                json!([]),
            ),
            ("glass.git.diff", json!({}), json!({"changed":true})),
            (
                "glass.process.ports",
                json!({"name":"web"}),
                json!({"port":8080}),
            ),
            (
                "glass.debug.stack",
                json!({"session":"rust","threadId":3}),
                json!({"stackFrames":[{"id":9}]}),
            ),
            (
                "glass.browser.state",
                json!({}),
                json!({"targetId":"page-1","revision":12}),
            ),
            ("glass.web_ir.inspect", json!({}), json!({"entities":[]})),
            (
                "glass.browser.web_ir",
                json!({}),
                json!({"targetId":"page-1","revision":12,"entityCount":3}),
            ),
            (
                "glass.workflow.verify",
                json!({"runId":"checkout"}),
                json!({"passed":false}),
            ),
            (
                "glass.test.results",
                json!({"runId":"unit"}),
                json!({"passed":true}),
            ),
            (
                "glass.eval.execute",
                json!({"name":"analysis"}),
                json!({"value":42}),
            ),
            ("glass.agent.list", json!({}), json!({"agentId":"agent-1"})),
            (
                "glass.task.get",
                json!({"taskId":"task-1"}),
                json!({"state":"failed"}),
            ),
            (
                "glass.experiment.collect",
                json!({"id":"approach-a"}),
                json!({"measured":true}),
            ),
        ];
        for (index, (name, arguments, result)) in samples.into_iter().enumerate() {
            let call = ToolCall {
                id: format!("projection-{index}"),
                name: name.into(),
                arguments,
            };
            let tool_node = format!("tool:{}", call.id);
            workspace
                .intelligence_mut()
                .upsert_node(DevelopmentNode {
                    id: tool_node.clone(),
                    kind: DevelopmentNodeKind::ToolCall,
                    label: call.name.clone(),
                    revision: 1,
                    stale: false,
                    evidence: Value::Null,
                })
                .unwrap();
            workspace
                .intelligence_mut()
                .link(
                    "repository:root",
                    &tool_node,
                    "receivedToolCall",
                    1,
                    "test",
                    Value::Null,
                )
                .unwrap();
            project_observable_result(&mut workspace, &call, "test", 1, 1, &result).unwrap();
        }
        assert!(
            workspace
                .intelligence()
                .query_kind(DevelopmentNodeKind::WebIrEntity)
                .iter()
                .any(|node| node.id == "webIr:projection-8"),
            "live browser Web IR must be projected into the graph"
        );
        for kind in [
            DevelopmentNodeKind::Repository,
            DevelopmentNodeKind::File,
            DevelopmentNodeKind::Symbol,
            DevelopmentNodeKind::EditorRevision,
            DevelopmentNodeKind::GitChange,
            DevelopmentNodeKind::Process,
            DevelopmentNodeKind::Port,
            DevelopmentNodeKind::Diagnostic,
            DevelopmentNodeKind::Debugger,
            DevelopmentNodeKind::Thread,
            DevelopmentNodeKind::StackFrame,
            DevelopmentNodeKind::BrowserTarget,
            DevelopmentNodeKind::BrowserRevision,
            DevelopmentNodeKind::WebIrEntity,
            DevelopmentNodeKind::Workflow,
            DevelopmentNodeKind::Test,
            DevelopmentNodeKind::Kernel,
            DevelopmentNodeKind::Agent,
            DevelopmentNodeKind::Task,
            DevelopmentNodeKind::Experiment,
            DevelopmentNodeKind::ToolCall,
        ] {
            assert!(
                !workspace.intelligence().query_kind(kind).is_empty(),
                "missing {kind:?}"
            );
        }
        let router = DevelopmentToolRouter::default();
        for (index, (from, to, relation)) in [
            ("file:src/lib.rs", "gitChange:projection-3", "changedBy"),
            ("gitChange:projection-3", "test:unit", "verifiedBy"),
            ("test:unit", "webIr:projection-7", "observedRegression"),
        ]
        .into_iter()
        .enumerate()
        {
            let context = context(&workspace, true);
            router
                .execute(
                    &mut workspace,
                    &ToolCall {
                        id: format!("causal-link-{index}"),
                        name: "glass.graph.link".into(),
                        arguments: json!({
                            "from":from,
                            "to":to,
                            "relation":relation,
                            "evidence":{"source":"resident-verification"}
                        }),
                    },
                    &context,
                )
                .unwrap();
        }
        let path = workspace
            .intelligence()
            .path("file:src/lib.rs", "webIr:projection-7")
            .unwrap();
        assert_eq!(
            path.edges
                .iter()
                .map(|edge| edge.relation.as_str())
                .collect::<Vec<_>>(),
            vec!["changedBy", "verifiedBy", "observedRegression"]
        );
        assert!(path.edges.iter().all(|edge| !edge.provenance.is_empty()));
        let replay = workspace.intelligence().replay(0, 128).unwrap();
        assert!(replay.iter().any(|event| event.subsystem == "task"));
        assert!(replay.iter().all(|event| event.rationale.is_none()));
    }

    #[test]
    fn external_delegate_supports_explicit_workspace_write_authority() {
        let descriptor = DevelopmentToolRouter::default()
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == "glass.agent.delegate")
            .expect("external delegate descriptor");
        assert!(descriptor.mutating);
        assert_eq!(
            descriptor.input_schema["properties"]["sandbox"]["enum"],
            json!(["read-only", "workspace-write"])
        );
    }

    #[test]
    fn core_durable_task_tools_are_registered_and_dispatch_to_selected_session() {
        let descriptors = service_descriptors();
        let graph = descriptors
            .iter()
            .find(|descriptor| descriptor.name == "glass.agent.task-graph")
            .expect("GC durable task graph tool is advertised");
        assert!(!graph.mutating);
        assert_eq!(graph.input_schema["additionalProperties"], false);
        let abort = descriptors
            .iter()
            .find(|descriptor| descriptor.name == "glass.agent.task-abort")
            .expect("GC durable task abort tool is advertised");
        assert!(abort.mutating);
        assert_eq!(abort.input_schema["required"], json!(["taskId"]));
        assert_eq!(
            abort.input_schema["properties"]["includeBackground"]["type"],
            "boolean"
        );

        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let readonly = context(&workspace, false);
        let graph_error = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "core-task-graph-dispatch".into(),
                    name: "glass.agent.task-graph".into(),
                    arguments: json!({"agentId":"agent-missing"}),
                },
                &readonly,
            )
            .expect_err("graph route should reach resident-session lookup");
        assert!(graph_error.to_string().contains("agent agent-missing"));
        let authorized = context(&workspace, true);
        let abort_error = router
            .execute(
                &mut workspace,
                &ToolCall {
                    id: "core-task-abort-dispatch".into(),
                    name: "glass.agent.task-abort".into(),
                    arguments: json!({"agentId":"agent-missing","taskId":"generation-1"}),
                },
                &authorized,
            )
            .expect_err("abort route should reach resident-session lookup");
        assert!(abort_error.to_string().contains("agent agent-missing"));
        let root = workspace.root().to_path_buf();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lsp_and_debugger_descriptors_publish_required_argument_schemas() {
        let descriptors = service_descriptors();
        let schema = |name: &str| {
            &descriptors
                .iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap_or_else(|| panic!("missing {name}"))
                .input_schema
        };
        assert_eq!(
            schema("glass.lsp.diagnostics")["required"],
            json!(["server", "path"])
        );
        assert_eq!(
            schema("glass.lsp.definition")["required"],
            json!(["server", "path", "line", "character"])
        );
        assert_eq!(schema("glass.debug.start")["required"], json!(["session"]));
        assert_eq!(
            schema("glass.debug.launch")["required"],
            json!(["session", "configuration"])
        );
        assert_eq!(
            schema("glass.debug.step")["properties"]["kind"]["enum"],
            json!(["over", "in", "out"])
        );
        assert_eq!(
            schema("glass.debug.variables")["required"],
            json!(["session", "variablesReference"])
        );
    }
}

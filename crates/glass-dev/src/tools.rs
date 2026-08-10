//! Governed tool routing into one resident development workspace.

use crate::agents::AgentSpec;
use crate::debugger::{DebugAdapterConfig, SourceBreakpoint};
use crate::kernels::KernelKind;
use crate::workspace::DevelopmentWorkspace;
use crate::{DevelopmentNode, DevelopmentNodeKind, ObservableEventInput};
use glass_browser::development::{
    AgentToolGateway, DevelopmentError, DevelopmentResult, HarnessRequest, ToolAuthorization,
    ToolCall, ToolDescriptor,
};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

const RESULT_LIMIT: usize = 512 * 1024;

#[derive(Debug, Clone)]
pub struct DevelopmentToolContext {
    pub authorization: ToolAuthorization,
    pub expected_generation: u64,
    pub expected_project_revision: u64,
}

/// Routes Glass Agent operations through authoritative resident services.
#[derive(Debug, Clone)]
pub struct DevelopmentToolRouter {
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
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.descriptors.clone()
    }

    pub fn execute(
        &self,
        workspace: &mut DevelopmentWorkspace,
        call: &ToolCall,
        context: &DevelopmentToolContext,
    ) -> DevelopmentResult<Value> {
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
            .iter()
            .find(|descriptor| descriptor.name == call.name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("tool {}", call.name)))?;
        if descriptor.mutating
            && (!context.authorization.allow_mutation || !context.authorization.confirmed)
        {
            return Err(DevelopmentError::Conflict(format!(
                "tool {} requires mutation authority and confirmation",
                call.name
            )));
        }
        let actor_id = context.authorization.actor.id.clone();
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
            serde_json::json!({"name":call.name}),
        )?;
        workspace.intelligence_mut().record(ObservableEventInput {
            actor: &actor_id,
            subsystem: "tool",
            kind: "called",
            resource: Some(&resource),
            workspace_revision: project_revision,
            evidence: serde_json::json!({"name":call.name,"mutating":descriptor.mutating}),
            rationale: None,
        })?;
        let result = if service_descriptors()
            .iter()
            .any(|descriptor| descriptor.name == call.name)
        {
            self.execute_service(workspace, call, context)
        } else {
            self.core
                .execute(workspace.project_mut(), call, &context.authorization)
        };
        match result {
            Ok(result) => {
                let result_bytes = serde_json::to_vec(&result)?.len();
                if result_bytes > RESULT_LIMIT {
                    return Err(DevelopmentError::InvalidInput(format!(
                        "tool result exceeds the {RESULT_LIMIT} byte limit"
                    )));
                }
                let resulting_revision = workspace.project().revision();
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
                let path = PathBuf::from(string("path")?);
                let branch = string("branch")?;
                let create_branch = boolean(call, "createBranch", true);
                git_mutation(workspace.git(), |git| {
                    git.create_worktree(&path, branch, create_branch)
                })?;
                Ok(serde_json::json!({"path":path,"created":true}))
            }
            "glass.git.worktree.remove" => {
                let path = PathBuf::from(string("path")?);
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
                map_service(workspace.kernels_mut().start(string("name")?, kind, &actor))?;
                Ok(serde_json::json!({"started":string("name")?,"kind":kind}))
            }
            "glass.eval.execute" => {
                let revision = workspace.project().revision();
                map_service(workspace.kernels_mut().execute(
                    string("name")?,
                    string("code")?,
                    &actor,
                    revision,
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
            "glass.eval.stop" => map_service(workspace.kernels_mut().stop(string("name")?)),
            "glass.lsp.diagnostics" => map_service(workspace.language().diagnostics(
                string("server")?,
                &actor,
                string("path")?,
            )),
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
            "glass.debug.start" => {
                let arguments = string_array_or_empty(call, "arguments")?;
                let config = DebugAdapterConfig::new(string("command")?, arguments);
                map_debug(workspace.start_debugger(
                    string("session")?,
                    &config,
                    Duration::from_secs(unsigned(call, "timeoutSeconds", 30)?),
                ))?;
                Ok(serde_json::json!({"started":string("session")?}))
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
            "glass.debug.stop" => {
                map_debug(workspace.stop_debugger(string("session")?))?;
                Ok(serde_json::json!({"stopped":string("session")?}))
            }
            "glass.agent.list" => map_service(workspace.agents().list()),
            "glass.agent.spawn" => {
                let spec: AgentSpec =
                    serde_json::from_value(call.arguments.get("spec").cloned().ok_or_else(
                        || DevelopmentError::InvalidInput("agent spawn requires spec".into()),
                    )?)?;
                map_service(workspace.agents().create(spec))
            }
            "glass.agent.prompt" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().prompt(&id, string("text")?))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.steer" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().steer(&id, string("text")?))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.agent.follow-up" => {
                let id = agent_id(workspace, string("agentId")?)?;
                map_service(workspace.agents().follow_up(&id, string("text")?))?;
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
                    HarnessRequest::Compact {
                        instructions: optional_string(call, "instructions").map(str::to_string),
                    },
                ))?;
                Ok(serde_json::json!({"queued":true}))
            }
            "glass.graph.query" => Ok(serde_json::to_value(
                workspace.intelligence().node(string("id")?),
            )?),
            "glass.graph.path" | "glass.graph.explain" => Ok(serde_json::to_value(
                workspace
                    .intelligence()
                    .path(string("from")?, string("to")?)?,
            )?),
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
            _ => Err(DevelopmentError::NotFound(format!("tool {}", call.name))),
        }
    }
}

fn service_descriptors() -> Vec<ToolDescriptor> {
    const READ: &[&str] = &[
        "glass.git.status",
        "glass.git.diff",
        "glass.git.branches",
        "glass.git.blame",
        "glass.git.conflicts",
        "glass.git.stash.list",
        "glass.git.worktree.list",
        "glass.test.discover",
        "glass.test.results",
        "glass.eval.list",
        "glass.lsp.diagnostics",
        "glass.lsp.hover",
        "glass.lsp.completion",
        "glass.debug.stack",
        "glass.debug.scopes",
        "glass.debug.variables",
        "glass.debug.evaluate",
        "glass.debug.events",
        "glass.agent.list",
        "glass.graph.query",
        "glass.graph.path",
        "glass.graph.explain",
        "glass.replay.list",
        "glass.replay.inspect",
        "glass.replay.diff",
    ];
    const MUTATE: &[&str] = &[
        "glass.git.stage",
        "glass.git.unstage",
        "glass.git.commit",
        "glass.git.branch.create",
        "glass.git.branch.switch",
        "glass.git.stash.push",
        "glass.git.stash.pop",
        "glass.git.worktree.create",
        "glass.git.worktree.remove",
        "glass.test.run",
        "glass.test.cancel",
        "glass.test.watch",
        "glass.eval.start",
        "glass.eval.execute",
        "glass.eval.reset",
        "glass.eval.stop",
        "glass.debug.start",
        "glass.debug.launch",
        "glass.debug.attach",
        "glass.debug.breakpoint.set",
        "glass.debug.continue",
        "glass.debug.pause",
        "glass.debug.step",
        "glass.debug.stop",
        "glass.agent.spawn",
        "glass.agent.prompt",
        "glass.agent.steer",
        "glass.agent.follow-up",
        "glass.agent.abort",
        "glass.agent.compact",
    ];
    READ.iter()
        .map(|name| service_descriptor(name, false))
        .chain(MUTATE.iter().map(|name| service_descriptor(name, true)))
        .collect()
}

fn service_descriptor(name: &str, mutating: bool) -> ToolDescriptor {
    ToolDescriptor {
        name: name.into(),
        description: format!("Resident Glass Dev operation {name}"),
        input_schema: serde_json::json!({"type":"object"}),
        mutating,
        available: true,
        unavailable_reason: None,
    }
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

fn optional_string<'a>(call: &'a ToolCall, name: &str) -> Option<&'a str> {
    call.arguments.get(name).and_then(Value::as_str)
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

fn integer(call: &ToolCall, name: &str) -> DevelopmentResult<i64> {
    call.arguments
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| DevelopmentError::InvalidInput(format!("{} requires {name}", call.name)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use glass_browser::development::Actor;
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
            authorization: ToolAuthorization {
                actor: Actor::embedded(),
                allow_mutation: mutate,
                confirmed: mutate,
            },
            expected_generation: workspace.generation(),
            expected_project_revision: workspace.project().revision(),
        }
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
            vec!["called", "completed"]
        );
        assert!(
            workspace
                .intelligence()
                .path("repository:root", "tool:write-1")
                .is_ok()
        );
    }

    #[test]
    fn router_uses_resident_kernel_and_test_services() {
        let mut workspace = workspace();
        let router = DevelopmentToolRouter::default();
        let start = ToolCall {
            id: "kernel-1".into(),
            name: "glass.eval.start".into(),
            arguments: serde_json::json!({"name":"analysis","kind":"sql"}),
        };
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
}

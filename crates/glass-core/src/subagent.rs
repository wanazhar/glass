//! Delegated tasks: fan-out, isolation, and the nested agent loop.
//!
//! One extension is one serial pipe, so fan-out cannot come from overlapping
//! tool calls — the host must run several nested agents inside a single
//! `run_subagents` request. That is what this module owns: the wave scheduler,
//! the per-task runner (fresh provider, fresh session, fresh context), and
//! optional git-worktree isolation.
//!
//! Delegation is deliberately coarse. A writing task gets native file tools, `bash` and `state`,
//! while a read-only task gets inspection tools, because anything else would be the parent conversation
//! leaking into a context that is supposed to be isolated.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::agent::Agent;
use crate::ext;
use crate::ext::protocol::SubagentTask;
use crate::provider::Provider;
use crate::session::{Record, Session};
use crate::tools;

/// How many nested agents run at once.
///
/// More tasks than this queue in waves. The cap exists because every
/// nested agent holds a provider request open and runs real commands;
/// an unbounded fan-out is a fork bomb with extra steps.
pub const MAX_CONCURRENT: usize = 4;

/// Run every task and return one report per task, in request order.
///
/// A task that fails — including one whose worker panics — produces an
/// error string for its own slot and never aborts its siblings. The
/// ordering guarantee matters for the same reason it does for tool calls:
/// the model reads reports against the tasks it delegated.
pub fn run_batch(
    tasks: &[SubagentTask],
    cap: usize,
    f: impl Fn(&SubagentTask) -> String + Sync,
) -> Vec<String> {
    run_batch_with_index(tasks, cap, |_, task| f(task))
}

/// Indexed variant used by the runtime child feed. Task indices are stable
/// within the extension request and reports remain in request order.
pub fn run_batch_with_index(
    tasks: &[SubagentTask],
    cap: usize,
    f: impl Fn(usize, &SubagentTask) -> String + Sync,
) -> Vec<String> {
    let cap = cap.max(1);
    let mut out = Vec::with_capacity(tasks.len());
    for (wave_index, wave) in tasks.chunks(cap).enumerate() {
        let start = wave_index * cap;
        if wave.len() == 1 {
            out.push(f(start, &wave[0]));
            continue;
        }
        let mut results: Vec<String> = std::thread::scope(|scope| {
            let f = &f;
            let handles: Vec<_> = wave
                .iter()
                .enumerate()
                .map(|(offset, task)| scope.spawn(move || f(start + offset, task)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| "subagent failed: worker panicked".into())
                })
                .collect()
        });
        out.append(&mut results);
    }
    out
}

/// Durable relationship between one delegated task and its actual child
/// session. The parent call ID comes from the extension protocol, while both
/// session IDs are assigned by Core.
#[derive(Clone, Debug)]
pub struct ChildLink {
    pub task_id: String,
    pub parent_session_id: String,
    pub parent_turn_id: serde_json::Value,
    pub parent_call_id: String,
    pub task_index: usize,
    pub task_count: usize,
    pub read_only: bool,
}

/// Event emitted for one actual child session. `data` is intentionally kept
/// opaque here; the runtime adds the stable IDs and monotonically increasing
/// sequence before placing it in the parent event stream.
pub struct ChildEvent {
    pub data: serde_json::Value,
}

pub(crate) struct ParentContext<'a> {
    pub prompt: Option<&'a str>,
    pub secrets: &'a crate::secrets::SecretVault,
}

const CHILD_LINK_DIR: &str = "subagent-links";
const MAX_CHILD_EVENT_TEXT_BYTES: usize = 4 * 1024;

#[derive(Default)]
struct ChildToolCallIds {
    next: u64,
    active: Vec<(zeroize::Zeroizing<String>, String)>,
}

impl ChildToolCallIds {
    fn start(&mut self, provider_id: &str) -> String {
        if let Some((_, public_id)) = self
            .active
            .iter()
            .find(|(raw_id, _)| raw_id.as_str() == provider_id)
        {
            return public_id.clone();
        }
        self.next = self.next.saturating_add(1);
        let public_id = format!("child-tool-{}", self.next);
        self.active.push((
            zeroize::Zeroizing::new(provider_id.to_owned()),
            public_id.clone(),
        ));
        public_id
    }

    fn finish(&mut self, provider_id: &str) -> String {
        if let Some(index) = self
            .active
            .iter()
            .position(|(raw_id, _)| raw_id.as_str() == provider_id)
        {
            return self.active.remove(index).1;
        }
        self.start(provider_id)
    }
}

/// Persist the Core-owned parent/child relationship separately from the child
/// conversation. This small sidecar keeps snapshots cheap even when a child
/// transcript has grown large; the transcript itself remains ordinary
/// session data.
pub fn persist_child_link(
    scratch: &Path,
    link: &ChildLink,
    child_session_id: &str,
    state: &str,
    started_at_ms: u64,
    finished_at_ms: Option<u64>,
    last_seq: u64,
) -> Result<(), String> {
    if !valid_session_id(child_session_id)
        || !valid_session_id(&link.task_id)
        || !valid_session_id(&link.parent_session_id)
        || link.parent_call_id.is_empty()
        || link.parent_call_id.len() > 128
        || link.parent_call_id.chars().any(char::is_control)
        || link.task_count == 0
        || link.task_index >= link.task_count
        || !matches!(state, "running" | "completed" | "failed" | "cancelled")
    {
        return Err("invalid child link identifier".into());
    }
    let directory = scratch.join(CHILD_LINK_DIR);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("cannot create child link directory: {error}"))?;
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|error| format!("cannot inspect child link directory: {error}"))?;
    if !metadata.file_type().is_dir() {
        return Err("child link path is not a real directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot protect child link directory: {error}"))?;
    }
    let value = serde_json::json!({
        "taskId":link.task_id,
        "parentSessionId":link.parent_session_id,
        "parentTurnId":link.parent_turn_id,
        "parentCallId":link.parent_call_id,
        "taskIndex":link.task_index,
        "taskCount":link.task_count,
        "childSessionId":child_session_id,
        "readOnly":link.read_only,
        "state":state,
        "startedAtMs":started_at_ms,
        "finishedAtMs":finished_at_ms,
        "lastSeq":last_seq,
    });
    let destination = directory.join(format!("{child_session_id}.json"));
    let temporary = directory.join(format!(".{}.{}.tmp", link.task_id, unique_id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("cannot create child link record: {error}"))?;
    let write_result = (|| {
        serde_json::to_writer(&mut file, &value)?;
        file.write_all(b"\n")?;
        file.sync_all()
    })();
    drop(file);
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("cannot write child link record: {error}"));
    }
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("cannot publish child link record: {error}"));
    }
    Ok(())
}

/// Return only child sessions whose durable link names `parent_session_id`.
/// The sidecar's file name and embedded child ID must agree; this
/// prevents a stale or copied link from turning into a row for another log.
pub fn list_linked_children(
    scratch: &Path,
    parent_session_id: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let links = scratch.join(CHILD_LINK_DIR);
    match std::fs::symlink_metadata(&links) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => return Err("child link path is not a real directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot inspect child link directory: {error}")),
    }
    let entries = match std::fs::read_dir(&links) {
        Ok(entries) => entries,
        Err(error) => return Err(format!("cannot list child sessions: {error}")),
    };
    let mut children = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot list child sessions: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("cannot inspect child session: {error}"))?
            .is_file()
        {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(child_id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !valid_session_id(child_id) {
            continue;
        }
        let Some(link) = read_child_link(scratch, child_id)? else {
            continue;
        };
        if link["parentSessionId"].as_str() != Some(parent_session_id)
            || link["childSessionId"].as_str() != Some(child_id)
            || !link["taskId"].as_str().is_some_and(valid_session_id)
            || !link["parentCallId"].as_str().is_some_and(|id| {
                !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
            })
            || !matches!(
                link["state"].as_str(),
                Some("running" | "completed" | "failed" | "cancelled")
            )
            || link["taskIndex"].as_u64().is_none()
            || link["taskCount"].as_u64().is_none()
            || link["taskCount"].as_u64() == Some(0)
            || link["taskIndex"].as_u64().unwrap_or(u64::MAX)
                >= link["taskCount"].as_u64().unwrap_or(0)
            || link["readOnly"].as_bool().is_none()
            || link["startedAtMs"].as_u64().is_none()
            || link["lastSeq"].as_u64().is_none()
        {
            continue;
        }
        children.push(link);
    }
    children.sort_by(|left, right| {
        left["startedAtMs"]
            .as_u64()
            .cmp(&right["startedAtMs"].as_u64())
            .then_with(|| {
                left["parentCallId"]
                    .as_str()
                    .cmp(&right["parentCallId"].as_str())
            })
            .then_with(|| left["taskIndex"].as_u64().cmp(&right["taskIndex"].as_u64()))
            .then_with(|| left["taskId"].as_str().cmp(&right["taskId"].as_str()))
    });
    if children.len() > 100 {
        children.drain(..children.len() - 100);
    }
    Ok(children)
}

fn read_child_link(
    scratch: &Path,
    child_session_id: &str,
) -> Result<Option<serde_json::Value>, String> {
    if !valid_session_id(child_session_id) {
        return Ok(None);
    }
    let directory = scratch.join(CHILD_LINK_DIR);
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => return Err("child link path is not a real directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot inspect child link directory: {error}")),
    }
    let path = scratch
        .join(CHILD_LINK_DIR)
        .join(format!("{child_session_id}.json"));
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot inspect child link: {error}")),
    };
    if metadata.len() > 16 * 1024 {
        return Err("child link record exceeds size limit".into());
    }
    let bytes = std::fs::read(&path).map_err(|error| format!("cannot read child link: {error}"))?;
    let value =
        serde_json::from_slice(&bytes).map_err(|_| "child link record is malformed".to_string())?;
    Ok(Some(value))
}

/// Open a child log only after verifying its durable parent/session/task link.
pub fn open_linked_child(
    scratch: &Path,
    parent_session_id: &str,
    parent_call_id: &str,
    task_id: &str,
    child_session_id: &str,
) -> Result<Session, String> {
    if !valid_session_id(child_session_id) {
        return Err("invalid child session ID".into());
    }
    let linked = read_child_link(scratch, child_session_id)?.is_some_and(|child| {
        child["parentSessionId"].as_str() == Some(parent_session_id)
            && child["taskId"].as_str() == Some(task_id)
            && child["parentCallId"].as_str() == Some(parent_call_id)
            && child["childSessionId"].as_str() == Some(child_session_id)
    });
    if !linked {
        return Err("child session is not linked to this parent".into());
    }
    let session_path = scratch.join(format!("{child_session_id}.jsonl"));
    match std::fs::symlink_metadata(&session_path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err("child session path is not a regular file".into()),
        Err(error) => return Err(format!("cannot inspect child session: {error}")),
    }
    Session::open_readonly(scratch, child_session_id)
        .map_err(|error| format!("cannot open child session: {error}"))
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Run one delegated task and report its real child session lifecycle and
/// child Agent progress to an observer. The returned report remains the
/// extension tool result; the observer receives only read-only feed events.
pub fn run_task_observed(
    task: &SubagentTask,
    workspace: &Path,
    scratch: &Path,
    make_provider: &MakeProvider<'_>,
    parent: ParentContext<'_>,
    link: ChildLink,
    observer: &(dyn Fn(ChildEvent) + Sync),
) -> String {
    // A parent-known value must not be copied into a child prompt/transcript.
    // Keep the task that the child actually receives as the card's assignment.
    let mut task = task.clone();
    task.task = parent.secrets.redact(&task.task);
    task.prompt = task.prompt.map(|prompt| parent.secrets.redact(&prompt));
    let parent_prompt = parent.prompt.map(|prompt| parent.secrets.redact(prompt));
    let mut session = match Session::create(scratch) {
        Ok(session) => session,
        Err(error) => return format!("subagent failed: {error}"),
    };
    session.secrets.inherit_redactors(parent.secrets);
    let child_session_id = session.id.clone();
    let now = unix_millis();
    if persist_child_link(scratch, &link, &child_session_id, "running", now, None, 1).is_err() {
        let report = "subagent failed: child session relationship could not be recorded";
        let mut data = serde_json::json!({
            "kind":"finished",
            "taskId":link.task_id,
            "parentCallId":link.parent_call_id,
            "taskIndex":link.task_index,
            "taskCount":link.task_count,
            "childSessionId":child_session_id,
            "seq":1,
            "state":"failed",
            "error":"child session relationship could not be recorded",
            "taskText":task.task,
        });
        redact_and_bound_child_event(parent.secrets, &mut data);
        observer(ChildEvent { data });
        return report.into();
    }
    let sequence = std::cell::Cell::new(0_u64);
    let emit = |kind: &'static str, mut data: serde_json::Value| {
        let seq = sequence.get().saturating_add(1);
        sequence.set(seq);
        if !data.is_object() {
            data = serde_json::json!({"value":data});
        }
        data["kind"] = serde_json::json!(kind);
        data["taskId"] = serde_json::json!(link.task_id);
        data["parentCallId"] = serde_json::json!(link.parent_call_id);
        data["taskIndex"] = serde_json::json!(link.task_index);
        data["taskCount"] = serde_json::json!(link.task_count);
        data["childSessionId"] = serde_json::json!(child_session_id);
        data["seq"] = serde_json::json!(seq);
        redact_and_bound_child_event(parent.secrets, &mut data);
        observer(ChildEvent { data });
    };
    emit(
        "started",
        serde_json::json!({
            "state":"running",
            "readOnly":link.read_only,
            "startedAtMs":now,
            "taskText":task.task,
        }),
    );
    session.append(Record::User(task.task.clone()));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (|| -> Result<(String, &'static str), String> {
            if session.write_error().is_some() {
                return Err("child task could not be recorded".into());
            }
            let provider = make_provider(task.model.as_deref())?;
            let (run_workspace, note) = if task.worktree {
                let dir = worktree(workspace, &scratch.join("worktrees"), &unique_id())?;
                let note = format!("\n\n[isolated worktree: {}]", dir.display());
                (dir, note)
            } else {
                (workspace.to_path_buf(), String::new())
            };
            let mut state = tools::State::new();
            let parent_hub=crate::team::get(&link.parent_session_id);
            let cfg=parent_hub.as_ref().and_then(|hub|hub.parent_config()).unwrap_or_default();
            let mods=crate::mods::load(&cfg.mods)?;
            let mut registry=ext::Registry::load(&cfg.extensions,&run_workspace,&cfg.model);
            if let Some(hub)=parent_hub.clone(){registry.set_subagent(move ||hub.extension_runner("main"));}
            let base = task_prompt(&task, parent_prompt.as_deref());
            let text_buffer = std::cell::RefCell::new(String::new());
            let tool_call_ids = std::cell::RefCell::new(ChildToolCallIds::default());
            let mut agent = Agent::new(
                provider.as_ref(),
                &mut session,
                &mut state,
                &mods,
                &mut registry,
                &run_workspace,
            )
            .with_task_context(base, task.read_only)
            .with_image_input(cfg.image_input)
            .with_checkpoint_tools(cfg.checkpoint_tools)
            .with_pre_tool_hook(if task.read_only {None}else{crate::hooks_shell::load_config()})
            .prime_extensions()
            .with_progress(|progress| match progress {
                crate::agent::Progress::Text(text) => {
                    let mut buffer = text_buffer.borrow_mut();
                    buffer.push_str(text);
                    *buffer = parent.secrets.redact(&buffer);
                    emit_ready_child_text(&mut buffer, parent.secrets, false, &emit);
                }
                crate::agent::Progress::ToolStart { call_id, name, .. } => {
                    let call_id = tool_call_ids.borrow_mut().start(call_id);
                    emit(
                        "tool_start",
                        serde_json::json!({"callId":call_id,"toolName":name}),
                    );
                }
                crate::agent::Progress::ToolEnd {
                    call_id,
                    summary,
                    output,
                } => {
                    let call_id = tool_call_ids.borrow_mut().finish(call_id);
                    emit(
                        "tool_end",
                        serde_json::json!({
                            "callId":call_id,
                            "summary":summary,
                            "text":output,
                            "isError":output.starts_with("error:")
                                || (output.starts_with("[exit ") && !output.starts_with("[exit 0]")),
                        }),
                    );
                }
                _ => {}
            });
            if let Some(hub)=&parent_hub {agent=hub.inherit_hosted(agent,child_session_id.clone(),task.read_only);}
            let turn_result = agent.run_turn(|_| {});
            drop(agent);
            let mut text_buffer = text_buffer.into_inner();
            emit_ready_child_text(&mut text_buffer, parent.secrets, true, &emit);
            match turn_result {
                Ok((out, crate::agent::Stop::Cancelled)) => {
                    Ok((format!("{}{note}", out.text), "cancelled"))
                }
                Ok((out, crate::agent::Stop::Finished | crate::agent::Stop::ToolBudget)) => {
                    Ok((format!("{}{note}", out.text), "completed"))
                }
                Ok((_, crate::agent::Stop::Steered)) => Err("subagent turn was interrupted".into()),
                Err(error) => Err(error.to_string()),
            }
        })()
    }))
    .unwrap_or_else(|_| Err("subagent worker panicked".into()));

    let (status, report) = match result {
        Ok((report, status)) => (status, report),
        Err(error) => ("failed", format!("subagent failed: {error}")),
    };
    let finished = unix_millis();
    let status_persisted = persist_child_link(
        scratch,
        &link,
        &child_session_id,
        status,
        now,
        Some(finished),
        sequence.get().saturating_add(1),
    )
    .is_ok();
    let report = parent.secrets.redact(&session.secrets.redact(&report));
    emit(
        "finished",
        serde_json::json!({
            "state":if status_persisted {status} else {"failed"},
            "finishedAtMs":finished,
            "error":(!status_persisted).then_some("child session status could not be recorded"),
            "result":report,
        }),
    );
    report
}

fn redact_and_bound_child_event(
    secrets: &crate::secrets::SecretVault,
    data: &mut serde_json::Value,
) {
    let Some(fields) = data.as_object_mut() else {
        return;
    };
    for (field, limit) in [
        ("taskText", MAX_CHILD_EVENT_TEXT_BYTES),
        ("result", MAX_CHILD_EVENT_TEXT_BYTES),
        ("text", MAX_CHILD_EVENT_TEXT_BYTES),
        ("callId", 128),
        ("toolName", 128),
        ("summary", 1024),
        ("error", 1024),
    ] {
        let Some(original) = fields.get(field).and_then(serde_json::Value::as_str) else {
            continue;
        };
        let redacted = secrets.redact(original);
        let (bounded, truncated) = truncate_child_event_text(&redacted, limit);
        fields.insert(field.to_string(), serde_json::Value::String(bounded));
        if truncated {
            fields.insert(format!("{field}Truncated"), serde_json::Value::Bool(true));
        }
    }
}

fn truncate_child_event_text(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

fn emit_ready_child_text(
    buffer: &mut String,
    secrets: &crate::secrets::SecretVault,
    final_chunk: bool,
    emit: &impl Fn(&'static str, serde_json::Value),
) {
    *buffer = secrets.redact(buffer);
    let safe_end = if final_chunk {
        buffer.len()
    } else {
        secrets.stream_safe_prefix_len(buffer)
    };
    let ready = buffer[..safe_end].to_string();
    buffer.drain(..safe_end);
    let mut start = 0;
    while start < ready.len() {
        let mut end = start
            .saturating_add(MAX_CHILD_EVENT_TEXT_BYTES)
            .min(ready.len());
        while !ready.is_char_boundary(end) {
            end -= 1;
        }
        emit("text", serde_json::json!({"text":&ready[start..end]}));
        start = end;
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(duration_millis_u64)
        .unwrap_or(0)
}

/// Builds one metered provider, optionally for a named model.
///
/// Boxed rather than generic so the REPL's factory can be handed to the
/// batch runner without threading type parameters through every call site.
pub type MakeProvider<'a> = dyn Fn(Option<&str>) -> Result<Box<dyn Provider>, String> + Sync + 'a;

/// Run one delegated task in a fresh context and return its report.
///
/// `make_provider` receives the task's model override; `None` means the
/// active connection's model.
pub fn run_task(
    task: &SubagentTask,
    workspace: &Path,
    scratch: &Path,
    make_provider: &MakeProvider<'_>,
    parent_prompt: Option<&str>,
) -> String {
    let result = (|| -> Result<String, String> {
        let provider = make_provider(task.model.as_deref())?;
        let (run_workspace, note) = if task.worktree {
            let dir = worktree(workspace, &scratch.join("worktrees"), &unique_id())?;
            let note = format!("\n\n[isolated worktree: {}]", dir.display());
            (dir, note)
        } else {
            (workspace.to_path_buf(), String::new())
        };
        let mut session = Session::create(scratch).map_err(|e| e.to_string())?;
        session.append(Record::User(task.task.clone()));
        let mut state = tools::State::new();
        let mods: Vec<Box<dyn glass_mod_api::Mod_>> = Vec::new();
        let mut registry = ext::Registry::empty();
        // Reuse only the raw base prompt for tasks without a persona. This
        // does not imply equal provider cache keys: tools, project context
        // and extension prompts can differ from the parent request.
        let base = task_prompt(task, parent_prompt);
        let mut agent = Agent::new(
            provider.as_ref(),
            &mut session,
            &mut state,
            &mods,
            &mut registry,
            &run_workspace,
        )
        .with_task_context(base, task.read_only);
        match agent.run_turn(|_| {}) {
            Ok((out, _)) => Ok(format!("{}{note}", out.text)),
            Err(e) => Err(e.to_string()),
        }
    })();
    result.unwrap_or_else(|e| format!("subagent failed: {e}"))
}

/// The prompt a delegated task runs with.
///
/// An agent definition body replaces the default framing; the task text
/// is always appended so a custom persona cannot lose the assignment.
fn task_prompt(task: &SubagentTask, parent: Option<&str>) -> String {
    if task
        .prompt
        .as_ref()
        .is_some_and(|prompt| !prompt.trim().is_empty())
    {
        compose_prompt(task)
    } else {
        parent
            .map(str::to_owned)
            .unwrap_or_else(|| compose_prompt(task))
    }
}

pub fn compose_prompt(task: &SubagentTask) -> String {
    let tools = if task.read_only {
        "the inspect tool with action=list/read/search/git_status/system, plus bounded teammate delegation when that tool is advertised. Read-only workers have no bash, arbitrary command execution, or file mutations. This is an enforced capability boundary, not model conservatism. Use git_status to verify repository state; a .git entry alone is insufficient. Use system for current time and host information. Never request write permission or start a desktop merely to perform inspection"
    } else {
        "read, edit, write, bash and state; prefer bounded native file operations with revision guards"
    };
    if let Some(body) = task
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
    {
        return format!(
            "{body}\n\nDelegated task: {task}\n\n\
             Do the task using {tools}. Do not ask the user questions. \
             Reply with a concise report of what you found or changed. Read relevant ranges first, avoid whole-repository dumps, and retain concise progress and paths in your report. Separate observed tool evidence from inference and unverified claims; identify unsupported checks as not performed. Never call a task verified or complete solely because a delegate returned. Output allowance includes reasoning; prioritize an actionable report.",
            task = task.task
        );
    }
    format!(
        "You are a subagent working on a delegated task.\n\n\
         Task: {task}\n\n\
         Do the task using {tools}. Do not ask the user \
         questions. When you are done, reply with a concise report \
         of what you found or changed. Read relevant ranges first, avoid whole-repository dumps, and retain concise progress and paths in your report. Separate observed tool evidence from inference and unverified claims; identify unsupported checks as not performed. Never call a task verified or complete solely because a delegate returned. Output allowance includes reasoning; prioritize an actionable report.",
        task = task.task
    )
}

/// Create a detached worktree at `root/<id>` from the workspace's HEAD.
///
/// The worktree is deliberately left in place after the task: the parent
/// needs somewhere to inspect what the isolated agent changed, and
/// silently deleting a checkout the report points at would be worse than
/// leaving a directory behind. Uncommitted parent changes are not
/// present — a worktree starts from HEAD, which is the point.
pub fn worktree(workspace: &Path, root: &Path, id: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("worktree root: {e}"))?;
    let dir = root.join(id);
    let out = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["worktree", "add", "--detach"])
        .arg(&dir)
        .arg("HEAD")
        .output()
        .map_err(|e| format!("git worktree: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "git worktree add failed (is this a git workspace?): {}",
            err.trim()
        ));
    }
    Ok(dir)
}

/// A filesystem-safe unique id, so two worktrees cannot collide.
fn duration_millis_u64(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn unique_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(duration_millis_u64)
        .unwrap_or(0);
    format!("{millis:x}-{:x}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// Allocate a Core-owned opaque task identifier for the subagent event feed.
pub fn new_task_id() -> String {
    unique_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_id_timestamp_saturates_at_u64_max() {
        assert_eq!(
            duration_millis_u64(std::time::Duration::new(u64::MAX, 999_999_999)),
            u64::MAX
        );
    }
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    fn task(text: &str) -> SubagentTask {
        SubagentTask {
            task: text.into(),
            ..SubagentTask::default()
        }
    }

    #[test]
    fn batch_preserves_request_order_when_later_tasks_finish_first() {
        let tasks: Vec<SubagentTask> = (0..4).map(|i| task(&format!("t{i}"))).collect();
        let out = run_batch(&tasks, 8, |t| {
            // Later tasks finish first; reports must still follow request order.
            let n: u64 = t.task[1..].parse().unwrap();
            std::thread::sleep(Duration::from_millis((4 - n) * 20));
            t.task.clone()
        });
        assert_eq!(out, vec!["t0", "t1", "t2", "t3"]);
    }

    #[test]
    fn indexed_batch_keeps_global_task_indices_across_waves() {
        let tasks: Vec<SubagentTask> = (0..5).map(|i| task(&format!("t{i}"))).collect();
        let out = run_batch_with_index(&tasks, 2, |index, task| format!("{index}:{}", task.task));
        assert_eq!(out, ["0:t0", "1:t1", "2:t2", "3:t3", "4:t4"]);
    }

    #[test]
    fn batch_runs_waves_concurrently_within_the_cap() {
        let tasks: Vec<SubagentTask> = (0..6).map(|i| task(&format!("t{i}"))).collect();
        let in_flight = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let _ = run_batch(&tasks, 2, |_| {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            in_flight.fetch_sub(1, Ordering::SeqCst);
            "done".into()
        });
        assert!(
            peak.load(Ordering::SeqCst) <= 2,
            "concurrency cap was exceeded: {peak:?}"
        );
    }

    #[test]
    fn batch_keeps_one_output_per_task_even_when_a_worker_panics() {
        let tasks: Vec<SubagentTask> = (0..3).map(|i| task(&format!("t{i}"))).collect();
        let out = run_batch(&tasks, 3, |t| {
            if t.task == "t1" {
                panic!("worker blew up");
            }
            format!("ok {}", t.task)
        });
        assert_eq!(out.len(), 3, "a panicking worker must not lose its slot");
        assert_eq!(out[0], "ok t0");
        assert!(out[1].contains("panicked"), "{}", out[1]);
        assert_eq!(out[2], "ok t2");
    }

    #[test]
    fn a_single_task_runs_inline() {
        let log = StdMutex::new(Vec::new());
        let tasks = vec![task("solo")];
        let out = run_batch(&tasks, 4, |t| {
            log.lock().unwrap().push(t.task.clone());
            "report".into()
        });
        assert_eq!(out, vec!["report"]);
        assert_eq!(log.into_inner().unwrap(), vec!["solo"]);
    }

    #[test]
    fn an_agent_prompt_precedes_the_task_and_a_default_names_it() {
        let mut t = task("review the parser");
        let default = compose_prompt(&t);
        assert!(default.contains("review the parser"));
        assert!(default.contains("delegated task"), "{default}");

        t.prompt = Some("You are a security reviewer.".into());
        let custom = compose_prompt(&t);
        assert!(custom.starts_with("You are a security reviewer."));
        assert!(custom.contains("review the parser"));
    }

    #[test]
    fn worktree_isolation_creates_a_detached_checkout() {
        let base = std::env::temp_dir().join(format!("gc-wt-{}", std::process::id()));
        let repo = base.join("repo");
        let root = base.join("wts");
        let _ = std::fs::remove_dir_all(&base);
        if std::fs::create_dir_all(&repo).is_err() {
            return;
        }
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
        };
        if run(&["init", "-q"]).is_err() {
            return; // no git available: skip like the python-dependent tests
        }
        std::fs::write(repo.join("a.txt"), "hi").unwrap();
        let _ = run(&["add", "a.txt"]);
        if !run(&["commit", "-qm", "init"]).unwrap().status.success() {
            return;
        }
        let dir = worktree(&repo, &root, "t1").expect("worktree");
        assert!(dir.join("a.txt").exists(), "HEAD contents must be present");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn worktree_outside_a_repo_fails_cleanly() {
        let base = std::env::temp_dir().join(format!("gc-wt-bare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let err = worktree(&base, &base.join("wts"), "x").unwrap_err();
        assert!(err.contains("worktree"), "{err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A spy provider capturing the system prompt each request saw, so the
    /// affinity test can compare prefixes without a network.
    struct SystemSpy {
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl Provider for SystemSpy {
        fn id(&self) -> &str {
            "spy"
        }
        fn stream(
            &self,
            req: &crate::provider::Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = crate::provider::Event> + '_>, crate::provider::Error>
        {
            self.seen.lock().unwrap().push(req.system.to_string());
            Ok(Box::new(
                [
                    crate::provider::Event::Text("done".into()),
                    crate::provider::Event::Done,
                ]
                .into_iter(),
            ))
        }
        fn context_window(&self) -> u32 {
            100_000
        }
    }

    struct SplitSecretProvider;

    impl Provider for SplitSecretProvider {
        fn id(&self) -> &str {
            "split-secret"
        }

        fn stream(
            &self,
            _: &crate::provider::Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = crate::provider::Event> + '_>, crate::provider::Error>
        {
            Ok(Box::new(
                [
                    crate::provider::Event::Text("before progress-".into()),
                    crate::provider::Event::Text("sent".into()),
                    crate::provider::Event::Text("inel after".into()),
                    crate::provider::Event::Reasoning("thought progress-".into()),
                    crate::provider::Event::Reasoning("sentinel".into()),
                    crate::provider::Event::ChatResponse,
                    crate::provider::Event::ChatReasoning("private progress-".into()),
                    crate::provider::Event::ChatReasoning("sentinel thought assignment-".into()),
                    crate::provider::Event::Done,
                ]
                .into_iter(),
            ))
        }

        fn context_window(&self) -> u32 {
            100_000
        }
    }

    #[test]
    fn cache_affinity_reuses_the_parent_system_prefix() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let t = task("do a thing");
        let scratch = std::env::temp_dir().join(format!("gc-affinity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let seen_for_closure = std::sync::Arc::clone(&seen);
        let make = move |_: Option<&str>| -> Result<Box<dyn Provider>, String> {
            // Each call must capture the same shared vector: clone the Arc.
            let s = SystemSpy {
                seen: std::sync::Arc::clone(&seen_for_closure),
            };
            Ok(Box::new(s))
        };
        let parent = "PARENT system prompt body";
        let report = run_task(&t, &scratch, &scratch, &make, Some(parent));
        assert!(!report.is_empty());
        let got = seen.lock().unwrap().clone();
        assert!(!got.is_empty(), "the subagent must have made a request");
        assert!(
            got.iter().all(|s| s.starts_with(parent)
                && s.matches("[GC workflow protocol ").count() == 1
                && s.matches("[GC teammates]").count() == 1),
            "every request must preserve the inherited prefix with one shared workflow protocol, got: {got:?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn observed_child_events_and_snapshots_follow_the_durable_parent_link() {
        let scratch = std::env::temp_dir().join(format!("gc-child-feed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let event_sink = std::sync::Arc::clone(&events);
        let observer = move |event: ChildEvent| {
            event_sink.lock().unwrap().push(event.data);
        };
        let make = |_: Option<&str>| -> Result<Box<dyn Provider>, String> {
            Ok(Box::new(SystemSpy {
                seen: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }))
        };
        let link = ChildLink {
            task_id: "task-1".into(),
            parent_session_id: "parent-session".into(),
            parent_turn_id: serde_json::json!("turn-1"),
            parent_call_id: "extension-call-1".into(),
            task_index: 0,
            task_count: 1,
            read_only: true,
        };
        let report = run_task_observed(
            &task("inspect the fixture"),
            &scratch,
            &scratch,
            &make,
            ParentContext {
                prompt: None,
                secrets: &crate::secrets::SecretVault::default(),
            },
            link,
            &observer,
        );
        assert!(report.contains("done"), "{report}");

        let events = events.lock().unwrap();
        assert_eq!(events.first().unwrap()["kind"], "started");
        assert_eq!(events.last().unwrap()["kind"], "finished");
        assert_eq!(events.last().unwrap()["state"], "completed");
        assert_eq!(
            events
                .iter()
                .map(|event| event["seq"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            (1..=events.len() as u64).collect::<Vec<_>>(),
        );
        assert!(events.iter().any(|event| event["kind"] == "text"));
        assert!(events.iter().all(|event| {
            event["taskId"] == "task-1"
                && event["parentCallId"] == "extension-call-1"
                && event["childSessionId"].as_str().is_some()
        }));
        let child_id = events[0]["childSessionId"].as_str().unwrap().to_string();
        drop(events);

        let children = list_linked_children(&scratch, "parent-session").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["taskId"], "task-1");
        assert_eq!(children[0]["parentCallId"], "extension-call-1");
        assert_eq!(children[0]["childSessionId"], child_id);
        assert_eq!(children[0]["state"], "completed");
        assert!(
            list_linked_children(&scratch, "another-parent")
                .unwrap()
                .is_empty()
        );

        let mut child = open_linked_child(
            &scratch,
            "parent-session",
            "extension-call-1",
            "task-1",
            &child_id,
        )
        .unwrap();
        child.append(Record::User("must not persist".into()));
        assert!(
            child.write_error().is_some(),
            "viewer snapshots are read-only"
        );
        assert!(
            open_linked_child(
                &scratch,
                "another-parent",
                "extension-call-1",
                "task-1",
                &child_id,
            )
            .is_err()
        );
        assert!(
            open_linked_child(
                &scratch,
                "parent-session",
                "different-call",
                "task-1",
                &child_id,
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn parent_secrets_are_redacted_from_child_assignment_split_progress_report_and_tool_fields() {
        let scratch = std::env::temp_dir().join(format!(
            "gc-child-parent-secret-feed-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();

        let mut parent_secrets = crate::secrets::SecretVault::default();
        let scope = crate::secrets::SecretScope {
            session_id: "parent-session".into(),
            tool_name: "web_secret_request".into(),
            operation: "create token".into(),
            destination: "https://api.example.test".into(),
        };
        parent_secrets
            .issue_zeroizing(
                scope.clone(),
                zeroize::Zeroizing::new("assignment-sentinel".into()),
            )
            .unwrap();
        parent_secrets
            .issue_zeroizing(scope, zeroize::Zeroizing::new("progress-sentinel".into()))
            .unwrap();

        let events = std::sync::Mutex::new(Vec::new());
        let observer = |event: ChildEvent| events.lock().unwrap().push(event.data);
        let make = |_: Option<&str>| -> Result<Box<dyn Provider>, String> {
            Ok(Box::new(SplitSecretProvider))
        };
        let report = run_task_observed(
            &task("inspect assignment-sentinel"),
            &scratch,
            &scratch,
            &make,
            ParentContext {
                prompt: None,
                secrets: &parent_secrets,
            },
            ChildLink {
                task_id: "task-parent-secret".into(),
                parent_session_id: "parent-session".into(),
                parent_turn_id: serde_json::json!("turn-1"),
                parent_call_id: "call-parent-secret".into(),
                task_index: 0,
                task_count: 1,
                read_only: true,
            },
            &observer,
        );

        assert!(!report.contains("assignment-sentinel"));
        assert!(!report.contains("progress-sentinel"));
        let events = events.lock().unwrap();
        let started = events
            .iter()
            .find(|event| event["kind"] == "started")
            .unwrap();
        let finished = events
            .iter()
            .find(|event| event["kind"] == "finished")
            .unwrap();
        assert_eq!(started["taskText"], "inspect [redacted secret]");
        assert_eq!(finished["result"], "before [redacted secret] after");
        assert!(events.iter().all(|event| {
            let encoded = event.to_string();
            !encoded.contains("assignment-sentinel")
                && !encoded.contains("assignment-")
                && !encoded.contains("progress-sentinel")
                && !encoded.contains("progress-")
        }));
        assert!(events.iter().any(|event| event["kind"] == "text"));
        assert!(events.iter().all(|event| {
            event["taskId"] == "task-parent-secret"
                && event["parentCallId"] == "call-parent-secret"
                && event["childSessionId"].as_str().is_some()
        }));
        let child_id = started["childSessionId"].as_str().unwrap().to_owned();
        drop(events);

        let child = Session::open_readonly(&scratch, &child_id).unwrap();
        let transcript = format!("{:?}", child.history_records());
        assert!(!transcript.contains("assignment-sentinel"));
        assert!(!transcript.contains("assignment-"));
        assert!(!transcript.contains("progress-sentinel"));
        assert!(!transcript.contains("progress-"));
        assert!(!transcript.contains("private progress-sentinel thought"));

        let mut tool_end = serde_json::json!({
            "kind":"tool_end",
            "callId":"progress-sentinel",
            "toolName":"assignment-sentinel",
            "summary":"printed progress-sentinel",
            "text":"progress-sentinel",
        });
        redact_and_bound_child_event(&parent_secrets, &mut tool_end);
        assert_eq!(tool_end["callId"], "[redacted secret]");
        assert_eq!(tool_end["toolName"], "[redacted secret]");
        assert_eq!(tool_end["summary"], "printed [redacted secret]");
        assert_eq!(tool_end["text"], "[redacted secret]");
        assert!(!tool_end.to_string().contains("progress-sentinel"));
        assert!(!tool_end.to_string().contains("assignment-sentinel"));

        let mut call_ids = ChildToolCallIds::default();
        let public_id = call_ids.start("progress-sentinel");
        assert_eq!(call_ids.finish("progress-sentinel"), public_id);
        assert!(!public_id.contains("progress-sentinel"));

        let oversized = "x".repeat(MAX_CHILD_EVENT_TEXT_BYTES + 7);
        let mut bounded = serde_json::json!({"taskText":oversized});
        redact_and_bound_child_event(&parent_secrets, &mut bounded);
        assert_eq!(
            bounded["taskText"].as_str().unwrap().len(),
            MAX_CHILD_EVENT_TEXT_BYTES
        );
        assert_eq!(bounded["taskTextTruncated"], true);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn observed_child_records_provider_setup_failure_as_a_terminal_child() {
        let scratch = std::env::temp_dir().join(format!("gc-child-failed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let events = std::sync::Mutex::new(Vec::new());
        let observer = |event: ChildEvent| events.lock().unwrap().push(event.data);
        let make = |_: Option<&str>| -> Result<Box<dyn Provider>, String> {
            Err("provider unavailable".into())
        };
        let report = run_task_observed(
            &task("offline task"),
            &scratch,
            &scratch,
            &make,
            ParentContext {
                prompt: None,
                secrets: &crate::secrets::SecretVault::default(),
            },
            ChildLink {
                task_id: "task-failed".into(),
                parent_session_id: "parent-session".into(),
                parent_turn_id: serde_json::json!("turn-1"),
                parent_call_id: "call-1".into(),
                task_index: 0,
                task_count: 1,
                read_only: true,
            },
            &observer,
        );
        assert!(report.contains("subagent failed"));
        let events = events.lock().unwrap();
        assert_eq!(events.first().unwrap()["kind"], "started");
        assert_eq!(events.last().unwrap()["state"], "failed");
        let children = list_linked_children(&scratch, "parent-session").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["state"], "failed");
        assert_eq!(children[0]["lastSeq"], events.last().unwrap()["seq"]);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn child_link_store_preserves_running_state_for_runtime_liveness_check() {
        let scratch = std::env::temp_dir().join(format!("gc-child-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let child = Session::create(&scratch).unwrap();
        let child_id = child.id.clone();
        persist_child_link(
            &scratch,
            &ChildLink {
                task_id: "task-stale".into(),
                parent_session_id: "parent-session".into(),
                parent_turn_id: serde_json::json!("turn-1"),
                parent_call_id: "call-1".into(),
                task_index: 0,
                task_count: 1,
                read_only: true,
            },
            &child_id,
            "running",
            10,
            None,
            1,
        )
        .unwrap();
        let listed = list_linked_children(&scratch, "parent-session").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["state"], "running");
        assert_eq!(listed[0]["lastSeq"], 1);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_persona_task_keeps_its_own_prefix() {
        let mut t = task("do a thing");
        t.prompt = Some("You are a custom agent.".into());
        assert!(task_prompt(&t, Some("parent prompt")).starts_with("You are a custom agent."));
        t.prompt = None;
        assert_eq!(task_prompt(&t, Some("parent prompt")), "parent prompt");
    }
}

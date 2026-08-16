//! Background snapshot actor for the TUI.
//!
//! The research consensus (Pleiades, Mastra Code, zerostack) is that model
//! streams, tools, and workspace queries must never occupy the terminal
//! task. This module applies that rule to Glass: a worker thread owns the
//! expensive refresh pass — file listing, git status, agent history,
//! process table, test runs — and publishes immutable display snapshots
//! through a `watch` channel. The render loop reads the latest snapshot
//! without ever blocking on the workspace lock, and a latency budget keeps
//! slow git repositories from freezing key handling.

use crate::tui::state::DevTuiState;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Commands the UI sends to the worker.
pub enum ActorRequest {
    /// Recompute every snapshot field immediately.
    Refresh,
    /// Recompute only conversation tail fields (cheap, high frequency).
    RefreshConversation,
    ShutDown,
}

/// One refresh pass result. Cheap to clone; the UI keeps the latest.
#[derive(Debug, Clone, Default)]
pub struct DisplaySnapshot {
    pub agents: String,
    pub agent_conversation: String,
    pub tasks: String,
    pub editor: String,
    pub lsp: String,
    pub processes: String,
    pub git: String,
    pub tests: String,
    pub kernels: String,
    pub debugger: String,
    pub replay: String,
    pub workflow: String,
    pub workspace_status: String,
    pub experiments: String,
    pub files: Vec<String>,
    /// Wall-clock duration of the pass, for the latency meter.
    pub duration: Duration,
}

impl DisplaySnapshot {
    /// True when every projection still holds its initial empty value.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.git.is_empty() && self.files.is_empty()
    }
}

/// Shared flag set by the UI when a render consumed a stale frame.
pub struct SnapshotWorker {
    handle: Option<JoinHandle<()>>,
    requests: Sender<ActorRequest>,
    snapshots: Arc<std::sync::Mutex<Option<DisplaySnapshot>>>,
    dirty: Arc<AtomicBool>,
    last_applied: Option<DisplaySnapshot>,
    /// Highest agent event sequence already folded into the conversation.
    conversation_cursor: Arc<std::sync::atomic::AtomicU64>,
}

impl SnapshotWorker {
    /// Spawn the worker for one workspace. The state is only used for its
    /// shared workspace handle; the UI retains its own clone.
    pub fn spawn(state: &DevTuiState) -> Self {
        let workspace = state.workspace.clone();
        let (request_tx, request_rx) = channel::<ActorRequest>();
        let snapshots = Arc::new(std::sync::Mutex::new(None::<DisplaySnapshot>));
        let dirty = Arc::new(AtomicBool::new(true));
        let snapshot_sink = Arc::clone(&snapshots);
        let dirty_flag = Arc::clone(&dirty);
        let conversation_cursor = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let worker_cursor = Arc::clone(&conversation_cursor);
        let handle = std::thread::Builder::new()
            .name("glass-snapshot".into())
            .spawn(move || {
                worker_loop(
                    workspace,
                    request_rx,
                    snapshot_sink,
                    dirty_flag,
                    worker_cursor,
                );
            })
            .expect("failed to spawn snapshot worker");
        Self {
            handle: Some(handle),
            requests: request_tx,
            snapshots,
            dirty,
            last_applied: None,
            conversation_cursor,
        }
    }

    /// Ask for a full refresh pass.
    pub fn request_refresh(&self) {
        let _ = self.requests.send(ActorRequest::Refresh);
        self.dirty.store(true, Ordering::Release);
    }

    /// Ask for a cheap conversation-tail pass.
    pub fn request_conversation(&self) {
        let _ = self.requests.send(ActorRequest::RefreshConversation);
    }

    /// Take the freshest snapshot when one is newer than the last applied;
    /// `None` when current. Never blocks on the worker.
    pub fn take_pending(&mut self) -> Option<DisplaySnapshot> {
        let latest = self.snapshots.lock().ok().and_then(|slot| slot.clone())?;
        let changed = self
            .last_applied
            .as_ref()
            .is_none_or(|applied| applied.duration != latest.duration || applied.git != latest.git);
        if !changed {
            return None;
        }
        self.last_applied = Some(latest.clone());
        Some(latest)
    }

    /// True while a requested pass has not produced a snapshot yet.
    pub fn is_busy(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    /// Highest agent event sequence already folded into the conversation.
    pub fn conversation_cursor(&self) -> u64 {
        self.conversation_cursor.load(Ordering::Acquire)
    }
}

impl Drop for SnapshotWorker {
    fn drop(&mut self) {
        let _ = self.requests.send(ActorRequest::ShutDown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn worker_loop(
    workspace: crate::SharedDevelopmentWorkspace,
    requests: Receiver<ActorRequest>,
    snapshots: Arc<std::sync::Mutex<Option<DisplaySnapshot>>>,
    dirty: Arc<AtomicBool>,
    conversation_cursor: Arc<std::sync::atomic::AtomicU64>,
) {
    // Seed an initial snapshot before the first draw.
    let mut seeded = false;
    let mut conversation_tail = String::new();
    loop {
        let request = if seeded {
            match requests.recv_timeout(Duration::from_millis(250)) {
                Ok(request) => request,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => ActorRequest::Refresh,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match requests.recv() {
                Ok(request) => request,
                Err(_) => break,
            }
        };
        match request {
            ActorRequest::ShutDown => break,
            ActorRequest::RefreshConversation => {
                // Cheap fast path: fold only new events into the tail.
                let Ok(mut locked) = workspace.lock() else {
                    continue;
                };
                let cursor = conversation_cursor.load(Ordering::Acquire);
                match locked.agents().history(cursor) {
                    Ok(events) if !events.is_empty() => {
                        let highest = events
                            .iter()
                            .map(|event| event.sequence)
                            .max()
                            .unwrap_or(cursor);
                        let rendered = crate::tui::projection::conversation(&events);
                        if !conversation_tail.is_empty() && !rendered.is_empty() {
                            conversation_tail.push_str("\n\n");
                        }
                        conversation_tail.push_str(&rendered);
                        conversation_cursor.store(highest, Ordering::Release);
                    }
                    _ => {}
                }
                if let Ok(mut slot) = snapshots.lock()
                    && let Some(snapshot) = slot.as_mut()
                {
                    snapshot.agent_conversation = conversation_tail.clone();
                }
                dirty.store(false, Ordering::Release);
            }
            ActorRequest::Refresh => {
                let started = Instant::now();
                let mut snapshot = compute_snapshot(&workspace, &mut seeded);
                snapshot.duration = started.elapsed();
                conversation_tail = snapshot.agent_conversation.clone();
                if let Ok(latest) = workspace
                    .lock()
                    .and_then(|mut locked| locked.agents().history(0))
                    && let Some(highest) = latest.iter().map(|event| event.sequence).max()
                {
                    conversation_cursor.store(highest, Ordering::Release);
                }
                if let Ok(mut slot) = snapshots.lock() {
                    *slot = Some(snapshot);
                }
                dirty.store(false, Ordering::Release);
            }
        }
    }
}

/// One full snapshot pass. Mirrors DevTuiState::refresh but never touches
/// UI-only fields (selection, scroll, status).
fn compute_snapshot(
    workspace: &crate::SharedDevelopmentWorkspace,
    seeded: &mut bool,
) -> DisplaySnapshot {
    let mut snapshot = DisplaySnapshot::default();
    let Ok(mut locked) = workspace.lock() else {
        snapshot.git = "Workspace lock failed".into();
        return snapshot;
    };
    *seeded = true;
    snapshot.files = locked
        .project()
        .list_files()
        .map(|entries| {
            entries
                .into_iter()
                .filter(|entry| matches!(entry.kind, crate::development::FileKind::File))
                .map(|entry| entry.path)
                .take(512)
                .collect()
        })
        .unwrap_or_default();
    snapshot.agents = match locked.agents().list() {
        Ok(agents) if agents.is_empty() => "No agents. :agent spawn ROLE TASK".into(),
        Ok(agents) => agents
            .iter()
            .map(|agent| {
                format!(
                    "{}  {}  {} · {}\n  target {} · model {} · thinking {} · events {} · dropped {}{}\n  evidence {}",
                    agent.id.as_str(),
                    agent.status.label(),
                    agent.role,
                    agent.task,
                    agent.worktree.display(),
                    agent.model.as_deref().unwrap_or("default"),
                    agent.thinking.as_deref().unwrap_or("default"),
                    agent.event_count,
                    agent.dropped_event_count,
                    agent
                        .last_error
                        .as_deref()
                        .map(|error| format!(" · {error}"))
                        .unwrap_or_default(),
                    agent
                        .evidence
                        .iter()
                        .rev()
                        .take(3)
                        .map(|evidence| evidence.to_string())
                        .collect::<Vec<_>>()
                        .join(" · ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        Err(error) => format!("Agent registry failed: {error}"),
    };
    snapshot.agent_conversation = match locked.agents().history(0) {
        Ok(events) if events.is_empty() => {
            "No conversation yet. Press i to compose a message.".into()
        }
        Ok(events) => crate::tui::projection::conversation(&events),
        Err(error) => format!("Conversation unavailable: {error}"),
    };
    snapshot.tasks = match locked.tasks() {
        Ok(tasks) if tasks.is_empty() => "No tasks. :task create ID TITLE".into(),
        Ok(tasks) => tasks
            .iter()
            .map(|task| {
                let glyph = match task.state {
                    crate::TaskState::Succeeded => "✓",
                    crate::TaskState::Failed | crate::TaskState::Cancelled => "×",
                    crate::TaskState::Blocked => "!",
                    crate::TaskState::Running | crate::TaskState::Verifying => "●",
                    _ => "○",
                };
                format!(
                    "{glyph} {}  {}  {}\n  goal {}\n  agent {} · attempt {} · model {} · thinking {}\n  depends {}\n  verification {}\n  evidence {}",
                    task.id.as_str(),
                    task.state.label(),
                    task.title,
                    task.goal,
                    task.assigned_agent
                        .as_ref()
                        .map(|agent| agent.as_str())
                        .unwrap_or("unassigned"),
                    task.attempt,
                    task.model.as_deref().unwrap_or("default"),
                    task.thinking.as_deref().unwrap_or("default"),
                    task.dependencies
                        .iter()
                        .map(|dependency| dependency.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    serde_json::to_string(&task.verification).unwrap_or_default(),
                    task.evidence
                        .iter()
                        .rev()
                        .take(3)
                        .map(|evidence| {
                            format!(
                                "{}={}",
                                evidence.kind,
                                evidence
                                    .passed
                                    .map_or("?".to_string(), |passed| passed.to_string())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" · ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        Err(error) => format!("Task scheduler failed: {error}"),
    };
    snapshot.editor = editor_projection(&locked);
    snapshot.git = git_projection(&mut locked);
    snapshot.workspace_status = workspace_status_projection(&mut locked);
    snapshot
}

fn editor_projection(workspace: &crate::DevelopmentWorkspace) -> String {
    let buffers: Vec<_> = workspace.project().buffers().cloned().collect();
    if buffers.is_empty() {
        return "No file open. Select a file below and press Enter, then i to edit.".into();
    }
    buffers
        .iter()
        .map(|buffer| {
            let lines: Vec<&str> = buffer.content.lines().collect();
            let cursor = buffer.cursor_line as usize;
            let viewport_rows = 16;
            let start = cursor
                .saturating_sub(viewport_rows / 2)
                .min(lines.len().saturating_sub(viewport_rows.min(lines.len())));
            let end = (start + viewport_rows).min(lines.len());
            let gutter_width = lines.len().to_string().len().max(3);
            let viewport = lines[start..end]
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let number = start + index + 1;
                    let marker = if number == cursor { "▶" } else { " " };
                    format!(
                        "{marker}{:>gutter_width$} │ {line}",
                        number,
                        gutter_width = gutter_width
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{}{} · cursor {}:{} · actor {} · {} lines\n{viewport}",
                if buffer.dirty { "● " } else { "○ " },
                buffer.path,
                buffer.cursor_line,
                buffer.cursor_column,
                buffer.actor.id,
                lines.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn git_projection(workspace: &mut crate::DevelopmentWorkspace) -> String {
    workspace
        .git()
        .map(|git| match git.status() {
            Ok(status) => {
                let header = format!(
                    "branch {} · ↑{} ↓{} · upstream {}",
                    status.branch.as_deref().unwrap_or("detached"),
                    status.ahead,
                    status.behind,
                    status.upstream.as_deref().unwrap_or("none")
                );
                let entries = status
                    .entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{}{} {}{}",
                            if entry.untracked { "?" } else { "●" },
                            if status.conflicts.contains(&entry.path) {
                                "!"
                            } else {
                                " "
                            },
                            entry.index_status,
                            entry.path
                        )
                    })
                    .collect::<Vec<_>>();
                if entries.is_empty() {
                    format!("{header}\n✓ working tree clean")
                } else {
                    format!("{header}\n{}", entries.join("\n"))
                }
            }
            Err(error) => format!("Git state failed: {error}"),
        })
        .unwrap_or_else(|| "Not a Git repository".into())
}

fn workspace_status_projection(workspace: &mut crate::DevelopmentWorkspace) -> String {
    let agent_count = workspace
        .agents()
        .list()
        .map(|items| items.len())
        .unwrap_or(0);
    let task_count = workspace.tasks().map(|items| items.len()).unwrap_or(0);
    let kernel_count = workspace.kernels().snapshots().count();
    let debugger_count = workspace.debugger_names().count();
    format!(
        "root {}\ngeneration {} · project revision {} · trust {}\nresident: {} agents · {} tasks · {} kernels · {} debuggers",
        workspace.root().display(),
        workspace.generation(),
        workspace.project().revision(),
        workspace.trust().label(),
        agent_count,
        task_count,
        kernel_count,
        debugger_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_worker_publishes_and_applies_without_blocking_ui() {
        let root = std::env::temp_dir().join(format!("glass-snapshot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let state = crate::tui::state::DevTuiState::open(
            &root,
            glass_browser::cli::args::TuiLayout::Desktop,
        )
        .unwrap();
        let mut worker = SnapshotWorker::spawn(&state);
        worker.request_refresh();
        let deadline = Instant::now() + Duration::from_secs(10);
        let snapshot = loop {
            if let Some(snapshot) = worker.take_pending() {
                break snapshot;
            }
            assert!(Instant::now() < deadline, "worker never published");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(!snapshot.files.is_empty());
        assert!(snapshot.git.contains("branch") || snapshot.git.contains("Git"));
        // The conversation cursor starts at zero and only moves when events exist.
        let cursor = worker.conversation_cursor();
        worker.request_conversation();
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.is_busy() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(worker.conversation_cursor() >= cursor);
        drop(worker);
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }
}

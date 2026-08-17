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
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct ToolJob {
    pub id: u64,
    pub call: crate::development::ToolCall,
    pub context: crate::tools::DevelopmentToolContext,
}

pub struct ToolJobResult {
    pub id: u64,
    pub tool: String,
    pub result: Result<serde_json::Value, String>,
}

/// Commands the UI sends to the worker.
pub enum ActorRequest {
    /// Recompute every snapshot field immediately.
    Refresh,
    /// Recompute only conversation tail fields (cheap, high frequency).
    RefreshConversation,
    Tool(Box<ToolJob>),
    ShutDown,
}

/// One refresh pass result. Cheap to clone; the UI keeps the latest.
/// Browser supervision outcome for one pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum BrowserHealth {
    #[default]
    Unknown,
    /// Connected and healthy.
    Connected,
    /// Was connected earlier; the endpoint stopped responding.
    Crashed {
        last_process_id: Option<u32>,
        last_revision: Option<u64>,
    },
}

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
    pub browser: String,
    pub trust_label: String,
    pub trust_inspection: Vec<crate::customization::CustomizationInspectionItem>,
    pub root: String,
    pub project_revision: u64,
    pub generation: u64,
    pub skills_count: usize,
    pub tools_count: usize,
    /// Wall-clock duration of the pass, for the latency meter.
    pub duration: Duration,
    /// Browser supervision verdict for this pass.
    pub browser_health: BrowserHealth,
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
    job_results: Receiver<ToolJobResult>,
    next_job_id: u64,
}

impl SnapshotWorker {
    /// Spawn the worker for one workspace. The state is only used for its
    /// shared workspace handle; the UI retains its own clone.
    pub fn spawn(state: &DevTuiState) -> Self {
        let workspace = state.workspace.clone();
        let (request_tx, request_rx) = channel::<ActorRequest>();
        let (job_result_tx, job_result_rx) = channel::<ToolJobResult>();
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
                    job_result_tx,
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
            job_results: job_result_rx,
            next_job_id: 1,
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

    /// Queue a governed tool call without occupying the terminal task.
    pub fn submit_tool(
        &mut self,
        call: crate::development::ToolCall,
        context: crate::tools::DevelopmentToolContext,
    ) -> Result<u64, String> {
        let id = self.next_job_id;
        self.next_job_id = self.next_job_id.saturating_add(1);
        self.requests
            .send(ActorRequest::Tool(Box::new(ToolJob { id, call, context })))
            .map_err(|_| "snapshot worker stopped".to_string())?;
        Ok(id)
    }

    pub fn try_job_result(&self) -> Result<Option<ToolJobResult>, String> {
        match self.job_results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("tool worker stopped".into()),
        }
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
    job_results: Sender<ToolJobResult>,
    snapshots: Arc<std::sync::Mutex<Option<DisplaySnapshot>>>,
    dirty: Arc<AtomicBool>,
    conversation_cursor: Arc<std::sync::atomic::AtomicU64>,
) {
    // Seed an initial snapshot before the first draw.
    let mut seeded = false;
    let mut conversation_tail = String::new();
    let mut last_browser: Option<(Option<u32>, Option<u64>)> = None;
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
            ActorRequest::Tool(job) => {
                let tool = job.call.name.clone();
                let result = workspace
                    .lock()
                    .and_then(|mut locked| locked.execute_tool(&job.call, &job.context))
                    .map_err(|error| error.to_string());
                let _ = job_results.send(ToolJobResult {
                    id: job.id,
                    tool,
                    result,
                });
            }
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
                // Supervise the browser endpoint between passes.
                let state = workspace
                    .lock()
                    .ok()
                    .and_then(|locked| locked.browser().state().ok());
                match state {
                    Some(state) => {
                        let connected = state
                            .pointer("/connected")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        let pid = state
                            .pointer("/browserProcessId")
                            .and_then(serde_json::Value::as_u64)
                            .map(|pid| pid as u32);
                        let revision = state
                            .pointer("/browserRevision")
                            .and_then(serde_json::Value::as_u64);
                        if connected {
                            last_browser = Some((pid, revision));
                            snapshot.browser_health = BrowserHealth::Connected;
                        } else if let Some(previous) = last_browser.take() {
                            snapshot.browser_health = BrowserHealth::Crashed {
                                last_process_id: previous.0,
                                last_revision: previous.1,
                            };
                        }
                    }
                    None => {
                        if let Some(previous) = last_browser.take() {
                            snapshot.browser_health = BrowserHealth::Crashed {
                                last_process_id: previous.0,
                                last_revision: previous.1,
                            };
                        }
                    }
                }
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
    snapshot.processes = locked
        .project_mut()
        .processes()
        .list_checked()
        .map(|items| {
            if items.is_empty() {
                "No managed terminals. Start the detected development command from More.".into()
            } else {
                items
                    .into_iter()
                    .map(|item| {
                        format!(
                            "{} {} · health {} · pid {} · {}\n  {}",
                            if matches!(item.health, crate::development::ProcessHealth::Healthy) {
                                "●"
                            } else {
                                "○"
                            },
                            item.name,
                            item.health.label(),
                            item.pid.map_or_else(|| "—".into(), |pid| pid.to_string()),
                            if item.pty { "PTY" } else { "pipes" },
                            item.detected_urls
                                .first()
                                .map_or(item.command.as_str(), String::as_str),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
        })
        .unwrap_or_else(|error| format!("Process state failed: {error}"));
    snapshot.lsp = {
        let language = locked.language();
        let servers = language.names().collect::<Vec<_>>();
        let event_count = language.events(0).len();
        if servers.is_empty() {
            "No language server active · diagnostics unavailable".into()
        } else {
            format!("● {} · {} recent events", servers.join(" · "), event_count)
        }
    };
    let _ = locked.tests_mut().poll();
    let test_runs = locked.tests().results().rev().take(32).collect::<Vec<_>>();
    snapshot.tests = if test_runs.is_empty() {
        "No test runs".into()
    } else {
        test_runs
            .iter()
            .map(|run| {
                format!(
                    "{} {} · {} · {} ms · {} cases",
                    if run.exit_code == Some(0) {
                        "✓"
                    } else {
                        "×"
                    },
                    run.suite_id,
                    run.state.label(),
                    run.duration_ms.unwrap_or_default(),
                    run.cases.len()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let kernels = locked.kernels().snapshots().cloned().collect::<Vec<_>>();
    snapshot.kernels = if kernels.is_empty() {
        "No persistent kernels".into()
    } else {
        kernels
            .iter()
            .map(|kernel| {
                format!(
                    "{} {} · {} · {} executions · rev {}",
                    if matches!(kernel.state, crate::kernels::KernelState::Ready) {
                        "●"
                    } else {
                        "○"
                    },
                    kernel.name,
                    kernel.kind.label(),
                    kernel.executions,
                    kernel.workspace_revision
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let debugger_names = locked
        .debugger_names()
        .map(str::to_string)
        .collect::<Vec<_>>();
    snapshot.debugger = if debugger_names.is_empty() {
        "No debugger sessions. :debug start NAME COMMAND [ARGS...]".into()
    } else {
        debugger_names
            .iter()
            .filter_map(|name| {
                locked
                    .debugger_mut(name)
                    .ok()
                    .and_then(|debugger| debugger.snapshot().ok())
                    .map(|value| (name, value))
            })
            .map(|(name, value)| {
                format!(
                    "● {} · {} · pid {} · {} breakpoints · {} watches · {} threads/processes",
                    name,
                    value.state.label(),
                    value.adapter_process_id,
                    value.breakpoints.values().map(Vec::len).sum::<usize>(),
                    value.watches.len(),
                    value.debuggee_processes.len()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    snapshot.replay = locked
        .intelligence()
        .replay(0, 128)
        .map(|events| {
            if events.is_empty() {
                "No observable replay events".into()
            } else {
                events
                    .iter()
                    .rev()
                    .take(24)
                    .rev()
                    .map(|event| {
                        format!(
                            "{} {} · {} · {} · rev {}",
                            event.sequence,
                            event.actor,
                            event.subsystem,
                            event.kind,
                            event.workspace_revision
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        })
        .unwrap_or_else(|error| format!("Replay failed: {error}"));
    snapshot.workflow = locked
        .browser()
        .list_workflows()
        .map(|value| crate::tui::projection::workflow(Some(&value)))
        .unwrap_or_else(|error| format!("Workflow state failed: {error}"));
    snapshot.browser = locked
        .browser()
        .state()
        .map(|value| {
            let connected = value
                .get("connected")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let revision = value
                .get("browserRevision")
                .and_then(serde_json::Value::as_u64)
                .map_or_else(|| "—".into(), |value| value.to_string());
            if connected {
                format!("● connected · revision {revision}")
            } else {
                "○ detached · browser start required".into()
            }
        })
        .unwrap_or_else(|error| format!("Browser state failed: {error}"));
    snapshot.editor = editor_projection(&locked);
    snapshot.git = git_projection(&mut locked);
    snapshot.workspace_status = workspace_status_projection(&mut locked);
    snapshot.trust_label = locked.trust().label().into();
    snapshot.trust_inspection = locked.trust_inspection();
    snapshot.root = locked.root().display().to_string();
    snapshot.project_revision = locked.project().revision();
    snapshot.generation = locked.generation();
    snapshot.skills_count = locked.customization().skills().count();
    snapshot.tools_count = locked.customization().config().tools.len();
    snapshot
}

fn editor_projection(workspace: &crate::DevelopmentWorkspace) -> String {
    let buffers: Vec<_> = workspace.project().buffers().cloned().collect();
    if buffers.is_empty() {
        return "No file open. Select a file below and press Enter, then i to edit.\n\nBUFFERS\nnone open · ] opens the next buffer".into();
    }
    let diagnostics = workspace.project().diagnostics();
    let mut body = String::new();
    for (position, buffer) in buffers.iter().enumerate() {
        let lines: Vec<&str> = buffer.content.lines().collect();
        let cursor = buffer.cursor_line as usize;
        let viewport_rows = 16;
        let start = cursor
            .saturating_sub(viewport_rows / 2)
            .min(lines.len().saturating_sub(viewport_rows.min(lines.len())));
        let end = (start + viewport_rows).min(lines.len());
        let gutter_width = lines.len().to_string().len().max(3);
        // Mark lines that carry diagnostics for this buffer.
        let file_diagnostics: Vec<_> = diagnostics
            .get(&buffer.path)
            .map(|items| items.as_slice())
            .unwrap_or(&[])
            .to_vec();
        let flagged: std::collections::BTreeSet<u32> = file_diagnostics
            .iter()
            .map(|item| item.start.line)
            .collect();
        let extension = buffer
            .path
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let spans = crate::development::editor::syntax_spans(&buffer.content, &extension);
        let viewport = lines[start..end]
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let number = start + index + 1;
                let marker = if number == cursor {
                    "▶"
                } else if flagged.contains(&(number as u32)) {
                    "!"
                } else {
                    " "
                };
                let byte_offset = lines[..start + index]
                    .iter()
                    .map(|prefix| prefix.len() + 1)
                    .sum::<usize>();
                let highlighted = highlight_line(line, byte_offset, &spans);
                format!(
                    "{marker}{:>gutter_width$} │ {highlighted}",
                    number,
                    gutter_width = gutter_width
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if position > 0 {
            body.push_str("\n\n");
        }
        let diagnostic_summary = if file_diagnostics.is_empty() {
            String::new()
        } else {
            format!(" · {} diagnostics", file_diagnostics.len())
        };
        body.push_str(&format!(
            "{}{} · cursor {}:{} · actor {} · {} lines{}\n{viewport}",
            if buffer.dirty { "● " } else { "○ " },
            buffer.path,
            buffer.cursor_line,
            buffer.cursor_column,
            buffer.actor.id,
            lines.len(),
            diagnostic_summary,
        ));
    }
    let list = buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| {
            format!(
                "{} {}{}",
                if index == 0 { "▶" } else { " " },
                buffer.path,
                if buffer.dirty { " ●" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{body}\n\nBUFFERS · ]/[ switch\n{list}")
}

/// Apply span markers to one line. Spans are byte offsets over the whole
/// buffer; only spans intersecting this line leave a marker.
fn highlight_line(
    line: &str,
    byte_offset: usize,
    spans: &[crate::development::editor::SyntaxSpan],
) -> String {
    let line_start = byte_offset;
    let line_end = byte_offset + line.len();
    let mut markers = String::new();
    let mut any = false;
    for span in spans {
        if span.end > line_start && span.start < line_end {
            any = true;
            break;
        }
    }
    if !any {
        return line.to_string();
    }
    // Mark keyword spans with guillemets so the projection stays plain text
    // while visibly distinguishing syntax kinds.
    let mut chars = line.char_indices().peekable();
    let mut output = String::new();
    while let Some((offset, character)) = chars.next() {
        let absolute = line_start + offset;
        let in_keyword = spans.iter().any(|span| {
            span.kind == crate::development::editor::SyntaxKind::Keyword
                && absolute >= span.start
                && absolute < span.end
        });
        if in_keyword {
            output.push('«');
            output.push(character);
            // consume the rest of the keyword run
            while let Some(&(next_offset, next_character)) = chars.peek() {
                let next_absolute = line_start + next_offset;
                let still = spans.iter().any(|span| {
                    span.kind == crate::development::editor::SyntaxKind::Keyword
                        && next_absolute >= span.start
                        && next_absolute < span.end
                });
                if still {
                    output.push(next_character);
                    chars.next();
                } else {
                    break;
                }
            }
            output.push('»');
        } else {
            output.push(character);
        }
    }
    markers.push_str(&output);
    markers
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

//! Native session checklist. Mutations are atomic; presentation never changes state.
use crate::{
    input::KeyReader,
    session::{Record, Session},
    tools::{Outcome, State},
};
use glass_mod_api::ToolCall;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;

pub const KIND: &str = "state/todo";
const RECEIPTS: usize = 256;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
    Abandoned,
}
impl Status {
    fn mark(&self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::InProgress => "◆",
            Self::Completed => "✓",
            Self::Blocked => "!",
            Self::Abandoned => "–",
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Abandoned => "abandoned",
        }
    }
    fn actionable(&self) -> bool {
        matches!(self, Self::Pending | Self::InProgress)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub phase: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_task: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Receipt {
    id: String,
    intent: String,
    revision: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Board {
    version: u8,
    next_id: u64,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub auto_continue: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    receipts: VecDeque<Receipt>,
}
impl Default for Board {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            tasks: Vec::new(),
            auto_continue: false,
            hidden: false,
            receipts: VecDeque::new(),
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(default)]
    ops: Vec<Op>,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Op {
    Init {
        items: Vec<NewTask>,
    },
    Add {
        items: Vec<NewTask>,
    },
    Update {
        id: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        phase: Option<String>,
        #[serde(default)]
        status: Option<Status>,
        #[serde(default)]
        blocker: Option<String>,
        #[serde(default)]
        owner: Option<String>,
        #[serde(default)]
        workspace_task: Option<u32>,
    },
    Remove {
        id: String,
    },
    Clear,
    Configure {
        #[serde(default)]
        auto_continue: Option<bool>,
        #[serde(default)]
        hidden: Option<bool>,
    },
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NewTask {
    title: String,
    #[serde(default = "default_phase")]
    phase: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    workspace_task: Option<u32>,
}
fn default_phase() -> String {
    "Tasks".into()
}
fn short(text: &str, max: usize) -> Result<String, String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.len() > max {
        return Err(format!("Checklist text must be 1..{max} bytes"));
    }
    Ok(text)
}
fn clip_bytes(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.into();
    }
    let mut end = limit.saturating_sub(3);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}
impl Board {
    fn preview(tasks: impl Iterator<Item = Task>) -> String {
        let rows = tasks
            .take(12)
            .map(|mut task| {
                task.title = format!(
                    "[{}] {}",
                    clip_bytes(&task.phase, 64),
                    clip_bytes(&task.title, 160)
                );
                task.blocker = task.blocker.map(|s| clip_bytes(&s, 160));
                task.owner = task.owner.map(|s| clip_bytes(&s, 64));
                task
            })
            .collect::<Vec<_>>();
        Self::rows(&rows)
    }
    pub fn revision(&self) -> String {
        crate::workspace::hash(
            json!([self.next_id, self.tasks, self.auto_continue, self.hidden])
                .to_string()
                .as_bytes(),
        )
    }
    pub fn record(&self) -> Record {
        Record::Custom {
            kind: KIND.into(),
            data: Some(serde_json::to_string(self).expect("checklist serialization")),
        }
    }
    pub fn restore(records: &[Record]) -> Result<Self, String> {
        for record in records.iter().rev() {
            if let Record::Custom { kind, data } = record
                && kind == KIND
            {
                let board: Self =
                    serde_json::from_str(data.as_deref().ok_or("Checklist snapshot has no data")?)
                        .map_err(|e| format!("Invalid checklist snapshot: {e}"))?;
                board.validate()?;
                return Ok(board);
            }
        }
        Ok(Self::default())
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.next_id == 0 {
            return Err("Unsupported checklist version".into());
        }
        if self.tasks.len() > 256
            || serde_json::to_vec(&self.tasks)
                .map_err(|e| e.to_string())?
                .len()
                > 131072
            || self.receipts.len() > RECEIPTS
        {
            return Err(
                "Checklist exceeds storage bounds (256 tasks, 128 KiB, 256 receipts)".into(),
            );
        }
        let mut ids = std::collections::HashSet::new();
        for task in &self.tasks {
            let id = task
                .id
                .strip_prefix('T')
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or("Invalid task ID")?;
            if id == 0 || id >= self.next_id || !ids.insert(id) {
                return Err("Duplicate or invalid task ID".into());
            }
            short(&task.title, 512)?;
            short(&task.phase, 128)?;
            if let Some(owner) = &task.owner {
                short(owner, 128)?;
            }
            if task.workspace_task == Some(0) {
                return Err("Workspace task ID must be positive".into());
            }
            if task.status == Status::Blocked
                && task.blocker.as_deref().is_none_or(|s| s.trim().is_empty())
            {
                return Err("Blocked tasks need a reason".into());
            }
            if let Some(blocker) = &task.blocker {
                short(blocker, 1024)?;
            }
        }
        Ok(())
    }
    pub fn apply(&mut self, call: &ToolCall) -> Outcome {
        match self.apply_checked(call) {
            Ok(text) => Outcome::ok(text),
            Err(error) => Outcome::err(error),
        }
    }
    fn apply_checked(&mut self, call: &ToolCall) -> Result<String, String> {
        let raw = call.arg_raw("todo").ok_or("Expected todo namespace")?;
        let request: Request =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid checklist arguments: {e}"))?;
        if request.ops.len() > 256 {
            return Err("At most 256 checklist operations per batch".into());
        }
        if request.ops.is_empty() {
            return Ok(json!({"revision":self.revision(),"tasks":self.tasks,"auto_continue":self.auto_continue,"hidden":self.hidden}).to_string());
        }
        let id = call
            .arg_raw("request_id")
            .map(|raw| serde_json::from_str::<String>(&raw).map_err(|e| e.to_string()))
            .transpose()?
            .map(|id| short(&id, 128))
            .transpose()?;
        let intent = crate::workspace::hash(
            serde_json::to_string(&request)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        );
        if let Some(id) = &id
            && let Some(receipt) = self.receipts.iter().find(|r| &r.id == id)
        {
            if receipt.intent != intent {
                return Err(
                    "Checklist request_id already applied with different operations".into(),
                );
            }
            return Ok(format!(
                "Checklist request {id} already applied; recorded revision {}. Current revision {}. Read todo for current tasks.",
                receipt.revision,
                self.revision()
            ));
        }
        if let Some(raw) = call.arg_raw("expected") {
            let expected: String = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            if expected != self.revision() {
                return Err("Stale checklist revision; read state todo before retrying".into());
            }
        }
        let mut next = self.clone();
        for op in request.ops {
            next.op(op)?;
        }
        next.validate()?;
        let changed = next
            .tasks
            .iter()
            .filter(|task| self.tasks.iter().find(|old| old.id == task.id) != Some(*task))
            .cloned()
            .collect::<Vec<_>>();
        let removed = self
            .tasks
            .iter()
            .filter(|old| !next.tasks.iter().any(|t| t.id == old.id))
            .map(|t| t.id.clone())
            .collect::<Vec<_>>();
        if let Some(id) = id {
            next.receipts.push_back(Receipt {
                id,
                intent,
                revision: next.revision(),
            });
            while next.receipts.len() > RECEIPTS {
                next.receipts.pop_front();
            }
        }
        let result = format!(
            "## {}\n\n{}{}\n\nrevision: {}\nReceipt history: {} of {RECEIPTS}, oldest first.",
            next.summary(),
            format_args!(
                "{}{}",
                Self::preview(changed.iter().cloned()),
                if changed.len() > 12 {
                    format!(
                        "\n… {} more changed tasks; read state todo:{{}} for all.",
                        changed.len() - 12
                    )
                } else {
                    String::new()
                }
            ),
            if removed.is_empty() {
                String::new()
            } else {
                format!(
                    "\nRemoved: {}{}",
                    removed
                        .iter()
                        .take(32)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    if removed.len() > 32 {
                        format!(" · {} more", removed.len() - 32)
                    } else {
                        String::new()
                    }
                )
            },
            next.revision(),
            next.receipts.len()
        );
        *self = next;
        Ok(result)
    }
    fn op(&mut self, op: Op) -> Result<(), String> {
        match op {
            Op::Init { items } => {
                self.tasks.clear();
                self.add(items)?;
                self.hidden = false;
            }
            Op::Add { items } => {
                self.add(items)?;
                self.hidden = false;
            }
            Op::Update {
                id,
                title,
                phase,
                status,
                blocker,
                owner,
                workspace_task,
            } => {
                let task = self
                    .tasks
                    .iter_mut()
                    .find(|t| t.id == id)
                    .ok_or_else(|| format!("Unknown task {id}; read todo for IDs"))?;
                if let Some(title) = title {
                    task.title = short(&title, 512)?;
                }
                if let Some(phase) = phase {
                    task.phase = short(&phase, 128)?;
                }
                if let Some(status) = status {
                    task.status = status;
                    if task.status != Status::Blocked {
                        task.blocker = None;
                    }
                }
                if let Some(blocker) = blocker {
                    task.blocker = Some(short(&blocker, 1024)?);
                }
                if let Some(owner) = owner {
                    task.owner = if owner.trim().is_empty() {
                        None
                    } else {
                        Some(short(&owner, 128)?)
                    };
                }
                if let Some(link) = workspace_task {
                    task.workspace_task = if link == 0 { None } else { Some(link) };
                }
            }
            Op::Remove { id } => {
                let index = self
                    .tasks
                    .iter()
                    .position(|t| t.id == id)
                    .ok_or_else(|| format!("Unknown task {id}"))?;
                self.tasks.remove(index);
            }
            Op::Clear => self.tasks.clear(),
            Op::Configure {
                auto_continue,
                hidden,
            } => {
                if let Some(value) = auto_continue {
                    self.auto_continue = value;
                }
                if let Some(value) = hidden {
                    self.hidden = value;
                }
            }
        }
        Ok(())
    }
    fn add(&mut self, items: Vec<NewTask>) -> Result<(), String> {
        if items.is_empty() {
            return Err(
                "Add/init needs at least one task; use clear to empty the checklist".into(),
            );
        }
        for item in items {
            let id = self.next_id;
            self.next_id = self.next_id.checked_add(1).ok_or("Task ID exhausted")?;
            self.tasks.push(Task {
                id: format!("T{id}"),
                title: short(&item.title, 512)?,
                phase: short(&item.phase, 128)?,
                status: Status::Pending,
                blocker: None,
                owner: item.owner.map(|s| short(&s, 128)).transpose()?,
                workspace_task: item.workspace_task,
            });
        }
        Ok(())
    }
    pub fn summary(&self) -> String {
        let completed = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Completed)
            .count();
        let blocked = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Blocked)
            .count();
        let abandoned = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Abandoned)
            .count();
        format!(
            "Tasks {completed}/{} complete{}{}",
            self.tasks.len(),
            if blocked > 0 {
                format!(" · {blocked} blocked")
            } else {
                String::new()
            },
            if abandoned > 0 {
                format!(" · {abandoned} abandoned")
            } else {
                String::new()
            }
        )
    }
    pub fn compact_summary(&self, width: usize) -> String {
        let done = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Completed)
            .count();
        let blocked = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Blocked)
            .count();
        let abandoned = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Abandoned)
            .count();
        let metrics = format!(
            "{done}/{}{}{}",
            self.tasks.len(),
            if blocked > 0 {
                format!(" !{blocked}")
            } else {
                String::new()
            },
            if abandoned > 0 {
                format!(" –{abandoned}")
            } else {
                String::new()
            }
        );
        let full = format!("Tasks {metrics}");
        if glass_tui::text::width(&full) <= width {
            full
        } else {
            metrics
        }
    }

    fn rows(tasks: &[Task]) -> String {
        tasks
            .iter()
            .map(|t| {
                format!(
                    "{} `{}` {} — {}{}{}{}",
                    t.status.mark(),
                    t.id,
                    t.title,
                    t.status.name(),
                    t.blocker
                        .as_ref()
                        .map(|s| format!(" · {s}"))
                        .unwrap_or_default(),
                    t.owner
                        .as_ref()
                        .map(|s| format!(" · agent {s}"))
                        .unwrap_or_default(),
                    t.workspace_task
                        .map(|id| format!(" · workspace #{id}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn markdown(&self) -> String {
        let mut out = format!("## {}\n\n", self.summary());
        let mut phases = Vec::new();
        for task in &self.tasks {
            if !phases.contains(&task.phase) {
                phases.push(task.phase.clone());
            }
        }
        for phase in phases {
            out.push_str(&format!(
                "### {}\n\n{}\n\n",
                phase,
                Self::rows(
                    &self
                        .tasks
                        .iter()
                        .filter(|t| t.phase == phase)
                        .cloned()
                        .collect::<Vec<_>>()
                )
            ));
        }
        if self.tasks.is_empty() {
            out.push_str("No tasks yet. The agent can create a checklist through state.todo.\n");
        }
        out.push_str("Task completion is reported progress; verification evidence is separate.");
        out
    }
    pub fn context(&self) -> Option<String> {
        let active = || {
            self.tasks.iter().filter(|t| {
                matches!(
                    t.status,
                    Status::Pending | Status::InProgress | Status::Blocked
                )
            })
        };
        let count = active().count();
        if count == 0 {
            None
        } else {
            Some(format!(
                "Checklist revision {} ({}). Use state.todo to update IDs; blocked work waits. Showing {} of {} actionable/blocked tasks; read todo:{{}} for all.\n{}",
                self.revision(),
                self.summary(),
                count.min(12),
                count,
                Self::preview(active().take(12).cloned())
            ))
        }
    }

    pub fn can_continue(&self, text: &str) -> bool {
        let tail = text.trim().lines().last().unwrap_or("").to_lowercase();
        self.auto_continue
            && self.tasks.iter().any(|t| t.status.actionable())
            && !tail.ends_with(['?', '？'])
            && ![
                "let me know",
                "please confirm",
                "please choose",
                "please reply",
                "waiting for your",
                "need your",
            ]
            .iter()
            .any(|s| tail.contains(s))
    }
}
thread_local! {static VISIBLE:std::cell::RefCell<Board>=std::cell::RefCell::new(Board::default());}
pub fn publish(board: &Board) {
    VISIBLE.with(|current| *current.borrow_mut() = board.clone());
    let _ = crate::surface::checklist(board);
}
pub fn view_current(keys: &KeyReader) {
    VISIBLE
        .with(|current| crate::modal::rich_info(keys, "Checklist", &current.borrow().markdown()));
}

/// All human changes use the same transaction and durable append as model changes.
pub fn apply_user(state: &mut State, session: &mut Session, ops: Value) -> Result<(), String> {
    let before = state.clone();
    let call = ToolCall {
        id: "human-checklist".into(),
        name: "state".into(),
        arguments: json!({"todo":{"ops":ops},"expected":state.todo.revision()}).to_string(),
    };
    let outcome = state.apply(&call);
    if outcome.body.starts_with("error:") {
        return Err(outcome.body);
    }
    if before.todo == state.todo {
        return Ok(());
    }
    let extra = if session
        .history_records()
        .iter()
        .any(|r| matches!(r, Record::User(_)))
    {
        vec![Record::System(format!(
            "User changed the checklist. Respect removals and current statuses. {}",
            state.todo.context().unwrap_or_else(|| state.todo.summary())
        ))]
    } else {
        Vec::new()
    };
    state.persist_with(session, before, extra)?;
    Ok(())
}
pub fn view(
    keys: &KeyReader,
    state: &mut State,
    session: &mut Session,
    workspace: &std::path::Path,
) {
    loop {
        let mut labels = vec![
            "Read full checklist".into(),
            "Add task".into(),
            format!(
                "Automatic continuation: {}",
                if state.todo.auto_continue {
                    "on"
                } else {
                    "off"
                }
            ),
            format!(
                "Composer summary: {}",
                if state.todo.hidden {
                    "hidden"
                } else {
                    "visible"
                }
            ),
            "Import legacy checklist".into(),
        ];
        labels.extend(
            state
                .todo
                .tasks
                .iter()
                .map(|t| format!("{} {} · {} · {}", t.status.mark(), t.id, t.phase, t.title)),
        );
        let Some(index) = crate::modal::choose(keys, "Checklist", &state.todo.summary(), &labels)
        else {
            return;
        };
        let ops = match index {
            0 => {
                crate::modal::rich_info(keys, "Checklist", &state.todo.markdown());
                continue;
            }
            1 => {
                let Some(title) =
                    crate::modal::field(keys, "Add task", "Task description", "", false)
                else {
                    continue;
                };
                json!([{"op":"add","items":[{"title":title}]}])
            }
            2 => json!([{"op":"configure","auto_continue":!state.todo.auto_continue}]),
            3 => json!([{"op":"configure","hidden":!state.todo.hidden}]),
            4 => match legacy_board(&session.history_records()) {
                Some(mut imported) => {
                    if !state.todo.tasks.is_empty()
                        && crate::modal::choose(
                            keys,
                            "Replace checklist?",
                            "Import replaces current tasks, with fresh IDs",
                            &["Keep current".into(), "Import legacy".into()],
                        ) != Some(1)
                    {
                        continue;
                    }
                    let before = state.clone();
                    let first = state.todo.next_id;
                    for (index, task) in imported.tasks.iter_mut().enumerate() {
                        task.id = format!("T{}", first + index as u64);
                    }
                    imported.next_id = first + imported.tasks.len() as u64;
                    imported.receipts = state.todo.receipts.clone();
                    imported.auto_continue = state.todo.auto_continue;
                    state.todo = imported;
                    if let Err(error) = state.persist(session, before) {
                        crate::modal::info(keys, "Import failed", &error);
                    }
                    continue;
                }
                None => {
                    crate::modal::info(
                        keys,
                        "Legacy checklist",
                        "No legacy todo snapshot exists in this conversation.",
                    );
                    continue;
                }
            },
            _ => {
                let task = state.todo.tasks[index - 5].clone();
                let actions = [
                    "Read details",
                    "Start",
                    "Complete",
                    "Block…",
                    "Unblock",
                    "Rename…",
                    "Assign agent…",
                    "Link workspace task…",
                    "Abandon",
                    "Remove",
                    "Read agent transcript",
                    "Read workspace evidence",
                ]
                .map(String::from);
                let Some(action) = crate::modal::choose(
                    keys,
                    &format!("{} · {}", task.id, task.title),
                    &task_details(&task),
                    &actions,
                ) else {
                    continue;
                };
                match action {
                    0 => {
                        crate::modal::rich_info(keys, "Task details", &task_details(&task));
                        continue;
                    }
                    1 => json!([{"op":"update","id":task.id,"status":"in_progress"}]),
                    2 => json!([{"op":"update","id":task.id,"status":"completed"}]),
                    3 => {
                        let Some(reason) = crate::modal::field(
                            keys,
                            "Block task",
                            "What is this waiting for?",
                            "",
                            false,
                        ) else {
                            continue;
                        };
                        json!([{"op":"update","id":task.id,"status":"blocked","blocker":reason}])
                    }
                    4 => json!([{"op":"update","id":task.id,"status":"pending"}]),
                    5 => {
                        let Some(title) = crate::modal::field(
                            keys,
                            "Rename task",
                            "Task description",
                            &task.title,
                            false,
                        ) else {
                            continue;
                        };
                        json!([{"op":"update","id":task.id,"title":title}])
                    }
                    6 => {
                        let hub = crate::team::get(&session.id);
                        let agents = hub.map(|h| h.snapshot()).unwrap_or_default();
                        let mut choices = vec!["Unassigned".into()];
                        choices.extend(agents.iter().map(|a| format!("{} · {}", a.id, a.name)));
                        let Some(i) = crate::modal::choose(
                            keys,
                            "Assign agent",
                            "Links progress only; does not start agent work",
                            &choices,
                        ) else {
                            continue;
                        };
                        json!([{"op":"update","id":task.id,"owner":if i==0{String::new()}else{agents[i-1].id.clone()}}])
                    }
                    7 => match crate::workspace::load(workspace) {
                        Ok(board) => {
                            let mut choices = vec!["Unlinked".into()];
                            choices.extend(
                                board
                                    .tasks
                                    .iter()
                                    .map(|t| format!("{} · {}", t.id, t.title)),
                            );
                            let Some(i) = crate::modal::choose(
                                keys,
                                "Link workspace task",
                                "Checklist status does not change workspace claims or checks",
                                &choices,
                            ) else {
                                continue;
                            };
                            json!([{"op":"update","id":task.id,"workspace_task":if i==0{0}else{board.tasks[i-1].id}}])
                        }
                        Err(e) => {
                            crate::modal::info(keys, "Workspace tasks", &e);
                            continue;
                        }
                    },
                    8 => json!([{"op":"update","id":task.id,"status":"abandoned"}]),
                    10 => {
                        let result = task
                            .owner
                            .as_ref()
                            .ok_or("This task has no assigned agent".to_string())
                            .and_then(|id| {
                                crate::team::get(&session.id)
                                    .ok_or("Agent hub unavailable".to_string())
                                    .and_then(|hub| {
                                        hub.request(&json!({"action":"transcript","id":id}))
                                    })
                            });
                        match result {
                            Ok(value) => crate::modal::rich_info(
                                keys,
                                "Agent transcript",
                                &crate::structured::render(&value),
                            ),
                            Err(error) => crate::modal::info(keys, "Agent transcript", &error),
                        }
                        continue;
                    }
                    11 => {
                        let result =
                            task.workspace_task
                                .ok_or("This task has no linked workspace task".to_string())
                                .and_then(|id| {
                                    crate::workspace::load(workspace).and_then(|board| {
                                        board.tasks.into_iter().find(|t| t.id == id).ok_or(
                                            "Linked workspace task is no longer present".into(),
                                        )
                                    })
                                });
                        match result {
                            Ok(link) => crate::modal::rich_info(
                                keys,
                                "Recorded workspace evidence",
                                &format!(
                                    "{}\n\nAcceptance: {}\n\nRecorded status: {}\n\nChecks recorded by the agent (not independently verified here):\n{}",
                                    link.title,
                                    link.acceptance,
                                    link.status,
                                    link.checks
                                        .iter()
                                        .map(|s| format!("- {s}"))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                ),
                            ),
                            Err(error) => crate::modal::info(keys, "Workspace evidence", &error),
                        }
                        continue;
                    }
                    _ => {
                        if crate::modal::choose(
                            keys,
                            "Remove task?",
                            &task.title,
                            &["Keep task".into(), "Remove".into()],
                        ) != Some(1)
                        {
                            continue;
                        }
                        json!([{"op":"remove","id":task.id}])
                    }
                }
            }
        };
        if let Err(error) = apply_user(state, session, ops) {
            crate::modal::info(keys, "Checklist update failed", &error);
        }
    }
}
fn task_details(task: &Task) -> String {
    format!(
        "{}\n\nPhase: {}\nStatus: {}\n{}{}{}\nCompletion is agent-reported; verify linked evidence separately.",
        task.title,
        task.phase,
        task.status.name(),
        task.blocker
            .as_ref()
            .map(|s| format!("Waiting: {s}\n"))
            .unwrap_or_default(),
        task.owner
            .as_ref()
            .map(|s| format!("Agent: {s}\n"))
            .unwrap_or_default(),
        task.workspace_task
            .map(|id| format!("Workspace task: {id}\n"))
            .unwrap_or_default()
    )
}

pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"properties":{"ops":{"type":"array","items":{"type":"object","properties":{"op":{"type":"string","enum":["init","add","update","remove","clear","configure"]},"items":{"type":"array","items":{"type":"object","properties":{"title":{"type":"string"},"phase":{"type":"string"},"owner":{"type":"string"},"workspace_task":{"type":"integer"}},"required":["title"],"additionalProperties":false}},"id":{"type":"string"},"title":{"type":"string"},"phase":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed","blocked","abandoned"]},"blocker":{"type":"string"},"owner":{"type":"string"},"workspace_task":{"type":"integer","minimum":0},"auto_continue":{"type":"boolean"},"hidden":{"type":"boolean"}},"required":["op"],"additionalProperties":false}}}})
}

fn legacy_board(records: &[Record]) -> Option<Board> {
    for record in records.iter().rev() {
        let Record::ToolResult { name, body, .. } = record else {
            continue;
        };
        if name != "todo" {
            continue;
        }
        let mut board = Board::default();
        for line in body.lines() {
            let Some((index, text)) = line.trim().split_once(". ") else {
                continue;
            };
            if index.parse::<usize>().is_err() {
                continue;
            }
            let (status, title) = if let Some(title) = text.strip_prefix("[x] ") {
                (Status::Completed, title)
            } else if let Some(title) = text.strip_prefix("[ ] ") {
                (Status::Pending, title)
            } else {
                continue;
            };
            board
                .add(vec![NewTask {
                    title: title.into(),
                    phase: default_phase(),
                    owner: None,
                    workspace_task: None,
                }])
                .ok()?;
            board.tasks.last_mut()?.status = status;
        }
        if !board.tasks.is_empty() && board.validate().is_ok() {
            return Some(board);
        }
        if body.contains("(no todos)") {
            return Some(board);
        }
    }
    None
}

pub fn plan_items(proposal: &str, task: &str) -> Value {
    let mut phase = default_phase();
    let mut items = Vec::new();
    let mut fenced = false;
    for line in proposal.lines() {
        let line = line.trim();
        if line.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if line.starts_with('#') {
            phase = line.trim_start_matches('#').trim().to_string();
            continue;
        }
        let text = line
            .strip_prefix("- [ ] ")
            .or_else(|| line.strip_prefix("* [ ] "));
        if let Some(title) = text {
            items.push(json!({"title":title,"phase":phase}));
        }
    }
    if items.is_empty() {
        items.push(json!({"title":task}));
    }
    json!(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(value: Value) -> ToolCall {
        ToolCall {
            id: "test".into(),
            name: "state".into(),
            arguments: value.to_string(),
        }
    }
    #[test]
    fn atomic_batches_ids_receipts_and_guards() {
        let mut b = Board::default();
        let initial = b.clone();
        assert!(b.apply(&call(json!({"todo":{"ops":[{"op":"add","items":[{"title":"One"}]},{"op":"update","id":"missing","status":"completed"}]}}))).body.starts_with("error:"));
        assert_eq!(b, initial);
        let init = call(
            json!({"todo":{"ops":[{"op":"add","items":[{"title":"One"},{"title":"Two"}]}]},"request_id":"init","expected":b.revision()}),
        );
        assert!(!b.apply(&init).body.starts_with("error:"));
        let snapshot = b.clone();
        assert!(b.apply(&init).body.contains("already applied"));
        assert_eq!(b, snapshot);
        assert!(
            b.apply(&call(
                json!({"todo":{"ops":[{"op":"clear"}]},"request_id":"init"})
            ))
            .body
            .starts_with("error:")
        );
        assert_eq!(b, snapshot);
        assert!(
            b.apply(&call(
                json!({"todo":{"ops":[{"op":"clear"}]},"expected":"old"})
            ))
            .body
            .starts_with("error:")
        );
        b.apply(&call(
            json!({"todo":{"ops":[{"op":"clear"},{"op":"add","items":[{"title":"Three"}]}]}}),
        ));
        assert_eq!(b.tasks[0].id, "T3");
    }
    #[test]
    fn blocked_tasks_need_a_reason_and_wait() {
        let mut b = Board::default();
        b.apply(&call(json!({"todo":{"ops":[{"op":"add","items":[{"title":"Task"}]},{"op":"configure","auto_continue":true}]}})));
        let old = b.clone();
        assert!(
            b.apply(&call(
                json!({"todo":{"ops":[{"op":"update","id":"T1","status":"blocked"}]}})
            ))
            .body
            .starts_with("error:")
        );
        assert_eq!(b, old);
        b.apply(&call(json!({"todo":{"ops":[{"op":"update","id":"T1","status":"blocked","blocker":"Waiting for user"}]}})));
        assert!(!b.can_continue("Working"));
    }
    #[test]
    fn restore_and_read_are_exact() {
        let mut b = Board::default();
        b.apply(&call(json!({"todo":{"ops":[{"op":"add","items":[{"title":"Unicode €"}]}]},"request_id":"one"})));
        assert_eq!(Board::restore(&[b.record()]).unwrap(), b);
        let old = b.clone();
        assert!(
            b.apply(&call(json!({"todo":{}})))
                .body
                .contains("Unicode €")
        );
        assert_eq!(b, old);
    }
    #[test]
    fn explicit_continuation_respects_user_questions() {
        let mut b = Board::default();
        b.apply(&call(
            json!({"todo":{"ops":[{"op":"add","items":[{"title":"Work"}]}]}}),
        ));
        assert!(!b.can_continue("Done"));
        b.auto_continue = true;
        assert!(b.can_continue("Continuing"));
        assert!(!b.can_continue("What should I do?"));
        assert!(!b.can_continue("Please confirm the choice."));
    }
    #[test]
    fn approved_plan_items_skip_code() {
        assert_eq!(
            plan_items(
                "## Build\n- [ ] First\n```\n- [ ] fake\n```\n## Verify\n- [ ] Tests",
                "task"
            )
            .as_array()
            .unwrap()
            .len(),
            2
        );
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    fn root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("gc-todo-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn add(state: &mut State, request: &str) {
        let out=state.apply(&ToolCall{id:"test".into(),name:"state".into(),arguments:json!({"todo":{"ops":[{"op":"add","items":[{"title":"Durable task","phase":"Implement"}]}]},"request_id":request}).to_string()});
        assert!(!out.body.starts_with("error:"), "{}", out.body);
    }
    #[test]
    fn restart_receipts_and_forks_are_durable() {
        let root = root("resume");
        let mut session = Session::create(&root).unwrap();
        let mut state = State::new();
        state.restore_todo(&[]).unwrap();
        let before = state.clone();
        add(&mut state, "create");
        state.persist(&mut session, before).unwrap();
        let id = session.id.clone();
        let board = state.todo.clone();
        session.append(Record::User("Work".into()));
        let fork = session.branch(&root, 1).unwrap();
        assert_eq!(Board::restore(&fork.history_records()).unwrap(), board);
        drop(fork);
        drop(session);
        let session = Session::open(&root, &id).unwrap();
        let mut restored = State::new();
        restored.restore_todo(&session.history_records()).unwrap();
        assert_eq!(restored.todo, board);
        add(&mut restored, "create");
        assert_eq!(restored.todo, board);
        drop(session);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_append_rolls_back_tasks_and_receipts() {
        let root = root("rollback");
        let session = Session::create(&root).unwrap();
        let id = session.id.clone();
        drop(session);
        let mut session = Session::open_readonly(&root, &id).unwrap();
        let mut state = State::new();
        state.restore_todo(&[]).unwrap();
        let before = state.clone();
        add(&mut state, "retriable");
        assert!(state.persist(&mut session, before.clone()).is_err());
        assert_eq!(state.todo, before.todo);
        drop(session);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn compaction_preserves_full_state_but_projects_only_actionable_rows() {
        let mut state = State::default();
        add(&mut state, "one");
        state.todo.tasks[0].status = Status::Completed;
        add(&mut state, "two");
        let mut records = vec![Record::User("Work".into()), state.todo.record()];
        for _ in 0..20 {
            records.push(Record::Assistant("older work".into()));
        }
        let rebuilt = crate::compact::rebuild(&records, &state);
        assert_eq!(Board::restore(&rebuilt).unwrap(), state.todo);
        let projected = crate::compact::project(&rebuilt);
        let contexts = projected
            .iter()
            .filter_map(|m| {
                if let glass_mod_api::Message::System(text) = m {
                    Some(text)
                } else {
                    None
                }
            })
            .filter(|text| text.starts_with("Checklist revision"))
            .collect::<Vec<_>>();
        assert_eq!(contexts.len(), 1);
        assert!(contexts[0].contains("T2"));
        assert!(!contexts[0].contains("`T1`"));
        assert!(!contexts[0].contains("receipts"));
    }
    #[test]
    fn notes_and_checklists_have_separate_revisions() {
        let mut state = State::default();
        let revision = state.revision();
        add(&mut state, "one");
        assert_eq!(state.revision(), revision);
        let revision = state.todo.revision();
        state.apply(&ToolCall {
            id: "notes".into(),
            name: "state".into(),
            arguments: json!({"items":["Fact"]}).to_string(),
        });
        assert_eq!(state.todo.revision(), revision);
    }
    #[test]
    fn legacy_import_uses_latest_snapshot() {
        let records = vec![
            Record::ToolResult {
                call_id: "one".into(),
                name: "todo".into(),
                body: "1. [ ] Earlier".into(),
            },
            Record::ToolResult {
                call_id: "two".into(),
                name: "todo".into(),
                body: "1. [x] Finished\n2. [ ] Next\n1 pending, 1 done".into(),
            },
        ];
        let board = legacy_board(&records).unwrap();
        assert_eq!(board.tasks.len(), 2);
        assert_eq!(board.tasks[0].status, Status::Completed);
        assert_eq!(board.tasks[1].title, "Next");
    }
    #[test]
    fn malformed_snapshots_are_not_silently_cleared() {
        assert!(
            Board::restore(&[Record::Custom {
                kind: KIND.into(),
                data: Some("broken".into())
            }])
            .is_err()
        );
    }
}

#[cfg(test)]
mod bounded_tests {
    use super::*;
    #[test]
    fn large_write_results_are_bounded_and_full_reads_remain_available() {
        let mut board = Board::default();
        let items = (0..100)
            .map(|i| json!({"title":format!("{i} {}","😀".repeat(120)),"phase":"Build"}))
            .collect::<Vec<_>>();
        let result = board.apply(&ToolCall {
            id: "large".into(),
            name: "state".into(),
            arguments: json!({"todo":{"ops":[{"op":"init","items":items}]}}).to_string(),
        });
        assert!(!result.body.starts_with("error:"));
        assert!(result.body.len() < 8192);
        assert!(result.body.contains("88 more changed tasks"));
        assert_eq!(board.tasks.len(), 100);
        assert!(board.context().unwrap().len() < 8192);
    }
    #[test]
    fn no_op_human_edits_do_not_append_reminders() {
        let dir = std::env::temp_dir().join(format!("gc-todo-noop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("Work".into()));
        let mut state = State::new();
        state.restore_todo(&[]).unwrap();
        let before = session.log_len();
        apply_user(
            &mut state,
            &mut session,
            json!([{"op":"configure","auto_continue":false}]),
        )
        .unwrap();
        assert_eq!(session.log_len(), before);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod branch_tests {
    use super::*;
    #[test]
    fn branching_an_earlier_turn_restores_that_turns_tasks() {
        let dir = std::env::temp_dir().join(format!("gc-todo-branch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = Session::create(&dir).unwrap();
        let mut state = State::new();
        state.restore_todo(&[]).unwrap();
        session.append(Record::User("First task".into()));
        apply_user(
            &mut state,
            &mut session,
            json!([{"op":"add","items":[{"title":"First"}]}]),
        )
        .unwrap();
        session.append(Record::Assistant("First answer".into()));
        session.append(Record::User("Second task".into()));
        apply_user(
            &mut state,
            &mut session,
            json!([{"op":"add","items":[{"title":"Second"}]}]),
        )
        .unwrap();
        let branch = session.branch(&dir, 1).unwrap();
        let restored = Board::restore(&branch.history_records()).unwrap();
        assert_eq!(restored.tasks.len(), 1);
        assert_eq!(restored.tasks[0].title, "First");
        assert_eq!(state.todo.tasks.len(), 2);
        drop(branch);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod summary_tests {
    use super::*;
    #[test]
    fn small_summary_keeps_blockers_visible() {
        let mut board = Board::default();
        board
            .add(vec![
                NewTask {
                    title: "One".into(),
                    phase: default_phase(),
                    owner: None,
                    workspace_task: None,
                },
                NewTask {
                    title: "Two".into(),
                    phase: default_phase(),
                    owner: None,
                    workspace_task: None,
                },
            ])
            .unwrap();
        board.tasks[0].status = Status::Completed;
        board.tasks[1].status = Status::Blocked;
        board.tasks[1].blocker = Some("Wait".into());
        assert_eq!(board.compact_summary(19), "Tasks 1/2 !1");
        assert!(glass_tui::text::width(&board.compact_summary(10)) <= 10);
    }
}

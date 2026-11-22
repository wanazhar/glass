//! Server-owned product state for the local Glass marble UI.

use crate::MarbleError;
use crate::control::ControlState;
use crate::memory::MemoryState;
use crate::notify::{DeviceRegistration, NotificationSettings};
use crate::presence::PresenceState;
use crate::prism::PrismState;
use crate::routine::RoutineState;
use crate::voice::VoiceSettings;
use crate::worker::now_millis;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Transcript entries one conversation keeps before its oldest are trimmed.
pub const TRANSCRIPT_PER_CONVERSATION: usize = 400;
/// Transcript entries the whole product keeps.
pub const TRANSCRIPT_LIMIT: usize = 4_000;
/// Settled tasks the product keeps; unfinished work is never trimmed.
pub const TASK_LIMIT: usize = 500;
/// Conversations the product keeps, by most recent activity.
pub const CONVERSATION_LIMIT: usize = 100;

/// What retention has trimmed, so a long-running instance stays legible.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionState {
    /// Transcript entries dropped to stay inside the bounds.
    #[serde(default)]
    pub trimmed_transcript: u64,
    /// Settled task records dropped to stay inside the bounds.
    #[serde(default)]
    pub trimmed_tasks: u64,
    /// Conversations dropped once their history was gone.
    #[serde(default)]
    pub trimmed_conversations: u64,
    /// Unix milliseconds of the last trim that removed anything.
    #[serde(default)]
    pub last_trim_at: u64,
}

impl RetentionState {
    /// Product document: the bounds plus what has been trimmed.
    pub fn document(&self) -> Value {
        json!({
            "limits": {
                "transcriptPerConversation": TRANSCRIPT_PER_CONVERSATION,
                "transcript": TRANSCRIPT_LIMIT,
                "tasks": TASK_LIMIT,
                "conversations": CONVERSATION_LIMIT,
            },
            "trimmedTranscript": self.trimmed_transcript,
            "trimmedTasks": self.trimmed_tasks,
            "trimmedConversations": self.trimmed_conversations,
            "lastTrimAt": self.last_trim_at,
        })
    }
}

/// One trim pass's result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneSummary {
    pub transcript: usize,
    pub tasks: usize,
    pub conversations: usize,
}

impl PruneSummary {
    pub fn is_empty(&self) -> bool {
        self.transcript == 0 && self.tasks == 0 && self.conversations == 0
    }
}

/// Whether a task can be trimmed without losing unfinished work.
pub(crate) fn settled(status: &str) -> bool {
    matches!(status, "completed" | "cancelled" | "failed" | "interrupted")
}

/// One durable conversation the local product knows about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub task_ids: Vec<String>,
    /// Unix milliseconds of the last update; zero for records without one.
    #[serde(default)]
    pub updated_at: u64,
    /// Engine lane that owns this conversation's session. Several chats run at
    /// once, one per lane, and a conversation always returns to the engine that
    /// admitted its first turn.
    #[serde(default)]
    pub lane: u8,
    #[serde(default)]
    pub engine_conversation_id: String,
    #[serde(default)]
    pub checkpoints: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub retry_task_id: String,
}

/// One durable generation task admitted for a conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub conversation_id: String,
    /// Named target, including turns that failed before a chat existed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub marble_id: String,
    /// `queued`, `running`, `completed`, `cancelled`, `failed`, or
    /// `interrupted` when the process stopped mid-turn.
    pub status: String,
    /// Who owns the running turn. Marble tasks are owned by the agent.
    pub owner: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub at: u64,
    /// Set when a routine, rather than a person, admitted this turn.
    #[serde(default)]
    pub routine_id: String,
}

/// One inbound runtime tool request awaiting an explicit host decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub request_id: String,
    pub call_id: String,
    pub tool: String,
    pub arguments: Value,
    /// Set when another chat also has work in this call's path, so the card can
    /// say what approving would do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<String>,
}

/// One Core-native tool the engine has actually reached for.
///
/// Core executes its own coding tools, so they never pass through Marble's
/// control layer. They still arrive as runtime events, and recording them is the
/// difference between "Marble can read files" and "Marble read these four files
/// at 14:02". The list is bounded; the newest use wins a place.
pub const ENGINE_TOOL_LIMIT: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedTool {
    pub name: String,
    #[serde(default)]
    pub runs: u64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub last_at: u64,
}

/// One durable transcript entry for a conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    #[serde(
        default,
        rename = "client_message_id",
        alias = "clientMessageId",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_message_id: Option<String>,
    #[serde(default)]
    pub id: String,
    #[serde(
        default,
        rename = "thread_id",
        alias = "threadId",
        skip_serializing_if = "Option::is_none"
    )]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<crate::threads::ThreadSummary>,
    pub conversation_id: String,
    /// `user`, `assistant`, `reasoning`, `tool`, `system`, `failure`, or `handoff`.
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub at: u64,
    /// Who said it: a marble id, `"marble"`, `"user"`, or empty.
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub speaker_name: String,
    /// One marble asking another for help, when this entry records a handoff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<HandoffEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::failure::FailureInfo>,
}

/// One marble-to-marble handoff, kept on the transcript row that tells it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffEntry {
    /// Marble id of the asker, or `"marble"` for the default assistant.
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub from_name: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub to_name: String,
    #[serde(default)]
    pub instruction: String,
    /// `asked`, `finished`, `refused`, or `failed`.
    #[serde(default)]
    pub status: String,
    /// The reply the ask brought back, once it has one.
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientMessage {
    pub id: String,
    pub entry_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientSend {
    pub message: ClientMessage,
    pub scopes: Vec<String>,
    pub status: u16,
    pub response: Option<Value>,
}

/// Whole local product state served to the UI and encoded for the client.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductState {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_sends: Vec<ClientSend>,
    #[serde(default)]
    pub threads: Vec<crate::threads::Thread>,
    #[serde(default)]
    pub task_threads: std::collections::HashMap<String, String>,
    #[serde(skip)]
    pub thread_events: crate::threads::ThreadEvents,
    /// Monotonic document revision; durable commits use it for reconciliation.
    #[serde(default)]
    pub revision: u64,
    pub conversations: Vec<ConversationRecord>,
    pub tasks: Vec<TaskRecord>,
    pub approvals: Vec<PendingApproval>,
    pub transcript: Vec<TranscriptEntry>,
    /// Marble's own control layer: posture, pause switch, rules, grants,
    /// decisions, and the admission queue.
    #[serde(default)]
    pub control: ControlState,
    /// Who answers for a turn currently running for the Everyone plane:
    /// `task:<id>` or `marble:<id>` → the speaker id to mirror under. In-memory
    /// only; a settled turn past a restart keeps nothing to mirror.
    #[serde(skip)]
    pub everyone_pending: std::collections::HashMap<String, String>,
    #[serde(skip)]
    pub everyone_orders: std::collections::HashMap<String, std::collections::VecDeque<String>>,
    #[serde(skip)]
    pub everyone_ready: std::collections::HashMap<String, TranscriptEntry>,
    /// Whether the assistant is up, what it is doing, and what it recovered.
    #[serde(default)]
    pub presence: PresenceState,
    /// Durable facts and the suggestions waiting for confirmation.
    #[serde(default)]
    pub memory: MemoryState,
    /// Saved prompts on a schedule.
    #[serde(default)]
    pub routines: RoutineState,
    /// Webhook and APNs delivery settings.
    #[serde(default)]
    pub notifications: NotificationSettings,
    /// Devices that can receive a push.
    #[serde(default)]
    pub devices: Vec<DeviceRegistration>,
    /// Local-only voice preferences.
    #[serde(default)]
    pub voice: VoiceSettings,
    /// Bounds the document keeps, and what has been trimmed from it.
    #[serde(default)]
    pub retention: RetentionState,
    /// Model requested for new conversations; empty means the engine default.
    /// Existing conversations keep the model they were created with.
    #[serde(default)]
    pub model: String,
    /// The Prism roster: several named marbles that can work at once.
    #[serde(default)]
    pub prism: PrismState,
    /// Core-native tools this host has seen the engine use.
    #[serde(default)]
    pub engine_tools: Vec<ObservedTool>,
}

impl ProductState {
    pub fn record_failure(&mut self, conversation_id: &str, task_id: &str, error: &str) {
        self.push_transcript(TranscriptEntry {
            conversation_id: conversation_id.into(),
            task_id: task_id.into(),
            kind: "failure".into(),
            text: crate::failure::MESSAGE.into(),
            failure: Some(crate::failure::FailureInfo::of(error)),
            at: crate::worker::now_millis(),
            ..Default::default()
        });
    }
    pub fn agent_tasks(&self, id: &str) -> Vec<&TaskRecord> {
        self.tasks
            .iter()
            .filter(|task| {
                let named = self.prism.marbles.iter().find(|m| {
                    !m.conversation_id.is_empty() && m.conversation_id == task.conversation_id
                });
                let owner = if !task.marble_id.is_empty() {
                    task.marble_id.as_str()
                } else if let Some(marble) = named {
                    marble.id.as_str()
                } else if task.owner == "prism" {
                    ""
                } else {
                    "marble"
                };
                task.status == "running" && owner == id
            })
            .collect()
    }

    pub fn prism_document(&self) -> Value {
        let mut document = self.prism.document();
        if let Some(marbles) = document["marbles"].as_array_mut() {
            for marble in marbles.iter_mut() {
                let tasks = self.agent_tasks(marble["id"].as_str().unwrap_or_default());
                marble["isWorking"] = serde_json::json!(!tasks.is_empty());
                marble["activeTaskIds"] =
                    serde_json::json!(tasks.iter().map(|t| &t.id).collect::<Vec<_>>());
                marble["taskId"] =
                    serde_json::json!(tasks.last().map(|t| t.id.as_str()).unwrap_or_default());
                if !tasks.is_empty() {
                    marble["state"] = serde_json::json!("working");
                } else if marble["state"] == "working" {
                    marble["state"] = serde_json::json!("idle");
                }
            }
            let working = marbles.iter().filter(|m| m["isWorking"] == true).count();
            document["working"] = serde_json::json!(working);
        }
        document
    }

    /// Encode state into the JSON shape served to and accepted from the UI.
    pub fn encode(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Decode state from the JSON shape served to and accepted from the UI.
    pub fn decode(value: &Value) -> Result<Self, MarbleError> {
        serde_json::from_value(value.clone())
            .map_err(|error| MarbleError::Protocol(format!("invalid product state: {error}")))
    }

    /// Advance the document revision after a mutation.
    pub fn bump(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    /// Find or create the conversation record for `id`.
    pub fn conversation_mut(&mut self, id: &str) -> &mut ConversationRecord {
        if let Some(index) = self
            .conversations
            .iter()
            .position(|conversation| conversation.id == id)
        {
            return &mut self.conversations[index];
        }
        self.conversations.push(ConversationRecord {
            id: id.to_string(),
            ..ConversationRecord::default()
        });
        self.conversations
            .last_mut()
            .expect("conversation record was just pushed")
    }

    /// Replace the record for one task id.
    pub fn upsert_task(&mut self, task: TaskRecord) {
        self.tasks.retain(|existing| existing.id != task.id);
        self.tasks.push(task);
    }

    /// Update one task's status; returns whether the task was known.
    pub fn set_task_status(&mut self, task_id: &str, status: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == task_id) {
            task.status = status.to_string();
            return true;
        }
        false
    }

    /// Rename one conversation; returns whether it existed.
    pub fn rename_conversation(&mut self, id: &str, title: &str) -> bool {
        let title = title.trim();
        if title.is_empty() {
            return false;
        }
        let updated = now_millis();
        let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == id)
        else {
            return false;
        };
        conversation.title = title.chars().take(120).collect();
        conversation.updated_at = updated;
        true
    }

    /// Remove one conversation and its settled history.
    ///
    /// A conversation with a queued or running turn is not removed: deleting
    /// the record would orphan work that is still using it. Stop or settle that
    /// turn first.
    pub fn delete_conversation(&mut self, id: &str) -> Result<(), String> {
        if !self
            .conversations
            .iter()
            .any(|conversation| conversation.id == id)
        {
            return Err("unknown conversation".into());
        }
        if self
            .tasks
            .iter()
            .any(|task| task.conversation_id == id && !settled(&task.status))
        {
            return Err("a turn is still running in this conversation".into());
        }
        self.conversations
            .retain(|conversation| conversation.id != id);
        // Everyone entries quoting one of this conversation's turns — the
        // mirrored reply, any handoff lines, and the person's line that
        // triggered them (stamped with the task id once the turn starts) —
        // go with the chat, or the shared plane keeps quoting a deleted turn.
        let dropped: std::collections::HashSet<String> = self
            .tasks
            .iter()
            .filter(|task| task.conversation_id == id)
            .map(|task| task.id.clone())
            .collect();
        self.tasks.retain(|task| task.conversation_id != id);
        self.transcript.retain(|entry| {
            entry.conversation_id != id
                && (entry.conversation_id != crate::everyone::EVERYONE_CONVERSATION
                    || !dropped.contains(&entry.task_id))
        });
        self.control
            .queue
            .retain(|entry| entry.conversation_id.as_deref() != Some(id));
        self.prune_threads();
        Ok(())
    }

    /// Tasks recorded against a conversation that no longer exists. A deleted
    /// conversation takes its own tasks with it, but older documents, quirk
    /// restores, and test pollution leave orphans behind — rows that render as
    /// "recent turns" for a chat nobody can open. Dropping them on load keeps
    /// the document self-consistent.
    pub fn prune_orphaned_tasks(&mut self) -> usize {
        let known: std::collections::HashSet<&str> = self
            .conversations
            .iter()
            .map(|conversation| conversation.id.as_str())
            .collect();
        let mut dropped = self.tasks.len();
        // A queued placeholder legitimately has no conversation yet — the
        // engine names the chat only when the turn starts, so keep it.
        self.tasks.retain(|task| {
            task.status == "queued" || known.contains(task.conversation_id.as_str())
        });
        dropped -= self.tasks.len();
        // Transcript rows for a chat that is gone render nowhere; notes left
        // by an older recovery carried no conversation at all.
        let before = self.transcript.len();
        self.transcript
            .retain(|entry| known.contains(entry.conversation_id.as_str()));
        dropped + before - self.transcript.len()
    }

    /// Everyone entries quoting a task that no longer exists.
    ///
    /// A mirrored reply (or the person's stamped line that triggered it)
    /// references its turn by task id; when the chat behind that turn is
    /// deleted out from under the shared plane — an older document, a
    /// quirk restore, or direct state edits — the entries would keep
    /// quoting a turn nobody can open. Unstamped lines (a busy note, or a
    /// message whose turn never started) carry no task id and stay.
    pub fn prune_everyone_entries(&mut self) -> usize {
        let tasks: std::collections::HashSet<&str> =
            self.tasks.iter().map(|task| task.id.as_str()).collect();
        let before = self.transcript.len();
        self.transcript.retain(|entry| {
            entry.conversation_id != crate::everyone::EVERYONE_CONVERSATION
                || entry.task_id.is_empty()
                || tasks.contains(entry.task_id.as_str())
                || (entry.kind == "pending" && self.everyone_pending.contains_key(&entry.task_id))
        });
        before - self.transcript.len()
    }

    /// Empty the shared Everyone timeline. The conversation record stays —
    /// the plane itself is a fixture; only what was said in it goes.
    pub fn clear_everyone(&mut self) -> usize {
        let before = self.transcript.len();
        self.transcript
            .retain(|entry| entry.conversation_id != crate::everyone::EVERYONE_CONVERSATION);
        self.everyone_pending.clear();
        self.everyone_orders.clear();
        self.everyone_ready.clear();
        self.prune_threads();
        before - self.transcript.len()
    }

    /// Newest transcript entries whose text contains `query`.
    pub fn search_transcript(&self, query: &str, limit: usize) -> Vec<&TranscriptEntry> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<&TranscriptEntry> = self
            .transcript
            .iter()
            .filter(|entry| entry.text.to_ascii_lowercase().contains(&needle))
            .collect();
        hits.sort_by_key(|entry| std::cmp::Reverse(entry.at));
        hits.truncate(limit.clamp(1, 100));
        hits
    }

    /// Note that the engine started one of its own tools.
    pub fn note_engine_tool(&mut self, name: &str) {
        let now = now_millis();
        if let Some(tool) = self.engine_tools.iter_mut().find(|tool| tool.name == name) {
            tool.runs = tool.runs.saturating_add(1);
            tool.last_at = now;
            return;
        }
        if self.engine_tools.len() >= ENGINE_TOOL_LIMIT {
            // Drop the least recently used name instead of refusing to learn a
            // new one, so a long session still reports what is being used now.
            if let Some(index) = self
                .engine_tools
                .iter()
                .enumerate()
                .min_by_key(|(_, tool)| tool.last_at)
                .map(|(index, _)| index)
            {
                self.engine_tools.remove(index);
            }
        }
        self.engine_tools.push(ObservedTool {
            name: name.to_string(),
            runs: 1,
            errors: 0,
            last_at: now,
        });
    }

    /// Note that an engine tool reported an error.
    pub fn note_engine_tool_error(&mut self, name: &str) {
        if let Some(tool) = self.engine_tools.iter_mut().find(|tool| tool.name == name) {
            tool.errors = tool.errors.saturating_add(1);
            tool.last_at = now_millis();
        }
    }

    /// Append one transcript entry.
    pub fn push_transcript(&mut self, mut entry: TranscriptEntry) {
        let pending = matches!(entry.kind.as_str(), "assistant" | "failure" | "system")
            .then(|| {
                self.transcript.iter().find(|previous| {
                    previous.kind == "pending"
                        && previous.conversation_id == entry.conversation_id
                        && previous.task_id == entry.task_id
                })
            })
            .flatten();
        let replacing_pending = pending.is_some();
        if let Some(previous) = pending {
            entry.id = previous.id.clone();
            entry.at = previous.at;
            entry.thread_id = previous.thread_id.clone();
        }
        if entry.kind == "user"
            && !entry.task_id.is_empty()
            && let Some(previous) = self.transcript.iter().find(|previous| {
                previous.kind == "user"
                    && previous.conversation_id == entry.conversation_id
                    && previous.task_id == entry.task_id
            })
        {
            entry.id = previous.id.clone();
            entry.at = previous.at;
        }
        if entry.kind == "user"
            && entry.client_message_id.is_none()
            && !entry.task_id.is_empty()
            && let Some(send) = self.client_sends.iter().find(|send| {
                !send.scopes.iter().any(|scope| scope == "everyone")
                    && send
                        .response
                        .as_ref()
                        .is_some_and(|response| response["taskId"] == entry.task_id)
            })
        {
            entry.id = send.message.entry_id.clone();
            entry.client_message_id = Some(send.message.id.clone());
        }
        if entry.id.is_empty() {
            entry.id = format!("entry-{}", crate::next_request_id());
        }
        if entry.kind != "user" && entry.speaker_name.is_empty() {
            entry.speaker_name = self.entry_sender(&entry);
        }
        if entry.thread_id.is_none() {
            entry.thread_id = self.task_threads.get(&entry.task_id).cloned();
        }
        let key = format!("task:{}", entry.task_id);
        if entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION
            && !replacing_pending
            && matches!(entry.kind.as_str(), "assistant" | "failure" | "system")
            && self
                .everyone_orders
                .values()
                .any(|order| order.contains(&key))
        {
            self.everyone_ready.insert(key, entry);
            self.flush_everyone();
            return;
        }
        let thread = entry.thread_id.clone();
        if let Some(send) = self
            .client_sends
            .iter_mut()
            .find(|send| send.message.entry_id == entry.id)
            && !entry.conversation_id.is_empty()
            && !send.scopes.contains(&entry.conversation_id)
        {
            send.scopes.push(entry.conversation_id.clone());
        }
        if let Some(existing) = self
            .transcript
            .iter_mut()
            .find(|existing| existing.id == entry.id)
        {
            *existing = entry;
        } else {
            self.transcript.push(entry);
        }
        if let Some(id) = thread {
            if let Some(thread) = self.threads.iter_mut().find(|t| t.id == id) {
                thread.last_activity_at = thread.last_activity_at.max(now_millis());
            }
            self.refresh_thread(&id);
        }
    }

    pub(crate) fn flush_everyone(&mut self) {
        let groups: Vec<_> = self.everyone_orders.keys().cloned().collect();
        for group in groups {
            while let Some(key) = self
                .everyone_orders
                .get(&group)
                .and_then(|q| q.front())
                .cloned()
            {
                if let Some(entry) = self.everyone_ready.remove(&key) {
                    self.everyone_orders.get_mut(&group).unwrap().pop_front();
                    self.push_transcript(entry);
                } else if !self.everyone_pending.contains_key(&key) {
                    self.everyone_orders.get_mut(&group).unwrap().pop_front();
                } else {
                    break;
                }
            }
        }
        self.everyone_orders.retain(|_, order| !order.is_empty());
    }

    /// Transcript entries for one conversation, newest last.
    pub fn transcript_for(&self, conversation_id: &str) -> Vec<TranscriptEntry> {
        self.transcript
            .iter()
            .filter(|entry| entry.conversation_id == conversation_id)
            .cloned()
            .collect()
    }

    /// Fold the document back inside its retention bounds.
    ///
    /// An always-on assistant runs for months, and every turn appends to the
    /// transcript, adds a task, and touches a conversation. Without a bound the
    /// durable file grows forever and every state response gets slower. Work
    /// that has not settled is always kept; only finished history is trimmed,
    /// newest first, and the counters stay visible in `/api/state`.
    pub fn prune(&mut self, now: u64) -> PruneSummary {
        let mut summary = PruneSummary::default();

        // Tasks: keep every unfinished task, plus the newest settled ones.
        if self.tasks.len() > TASK_LIMIT {
            let protected = self
                .tasks
                .iter()
                .filter(|task| !settled(&task.status))
                .count();
            let room = TASK_LIMIT.saturating_sub(protected);
            let settled_total = self.tasks.len() - protected;
            let to_drop = settled_total.saturating_sub(room);
            if to_drop > 0 {
                let mut order: Vec<usize> = self
                    .tasks
                    .iter()
                    .enumerate()
                    .filter(|(_, task)| settled(&task.status))
                    .map(|(index, _)| index)
                    .collect();
                order.sort_by_key(|index| self.tasks[*index].at);
                let mut drop = vec![false; self.tasks.len()];
                for index in order.into_iter().take(to_drop) {
                    drop[index] = true;
                }
                let before = self.tasks.len();
                let mut index = 0usize;
                self.tasks.retain(|_| {
                    let keep = !drop[index];
                    index += 1;
                    keep
                });
                summary.tasks = before - self.tasks.len();
            }
        }

        // Keep each conversation's task ids pointing at surviving tasks.
        if summary.tasks > 0 {
            let known: std::collections::HashSet<&str> =
                self.tasks.iter().map(|task| task.id.as_str()).collect();
            for conversation in &mut self.conversations {
                conversation
                    .task_ids
                    .retain(|id| known.contains(id.as_str()));
                conversation
                    .checkpoints
                    .retain(|id, _| known.contains(id.as_str()));
            }
        }

        // Conversations: newest activity wins, but anything still working stays.
        if self.conversations.len() > CONVERSATION_LIMIT {
            let mut order: Vec<usize> = (0..self.conversations.len()).collect();
            order.sort_by_key(|index| std::cmp::Reverse(self.conversations[*index].updated_at));
            let mut keep: std::collections::HashSet<usize> =
                order.into_iter().take(CONVERSATION_LIMIT).collect();
            for (index, conversation) in self.conversations.iter().enumerate() {
                if self
                    .tasks
                    .iter()
                    .any(|task| task.conversation_id == conversation.id && !settled(&task.status))
                {
                    keep.insert(index);
                }
            }
            let dropped: std::collections::HashSet<String> = self
                .conversations
                .iter()
                .enumerate()
                .filter(|(index, _)| !keep.contains(index))
                .map(|(_, conversation)| conversation.id.clone())
                .collect();
            if !dropped.is_empty() {
                summary.conversations = dropped.len();
                self.conversations
                    .retain(|conversation| !dropped.contains(&conversation.id));
                let before = self.transcript.len();
                self.transcript
                    .retain(|entry| !dropped.contains(&entry.conversation_id));
                summary.transcript += before - self.transcript.len();
            }
        }

        // Transcript: bound each conversation, then the document as a whole.
        if self.transcript.len() > TRANSCRIPT_PER_CONVERSATION {
            let mut counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            let mut keep = vec![false; self.transcript.len()];
            for (index, entry) in self.transcript.iter().enumerate().rev() {
                let count = counts.entry(entry.conversation_id.as_str()).or_default();
                if *count < TRANSCRIPT_PER_CONVERSATION {
                    keep[index] = true;
                    *count += 1;
                }
            }
            let before = self.transcript.len();
            let mut index = 0usize;
            self.transcript.retain(|_| {
                let kept = keep[index];
                index += 1;
                kept
            });
            summary.transcript += before - self.transcript.len();
        }
        if self.transcript.len() > TRANSCRIPT_LIMIT {
            let excess = self.transcript.len() - TRANSCRIPT_LIMIT;
            self.transcript.drain(..excess);
            summary.transcript += excess;
        }

        if !summary.is_empty() {
            self.prune_threads();
            self.retention.trimmed_transcript = self
                .retention
                .trimmed_transcript
                .saturating_add(summary.transcript as u64);
            self.retention.trimmed_tasks = self
                .retention
                .trimmed_tasks
                .saturating_add(summary.tasks as u64);
            self.retention.trimmed_conversations = self
                .retention
                .trimmed_conversations
                .saturating_add(summary.conversations as u64);
            self.retention.last_trim_at = now;
            self.bump();
        }
        summary
    }

    /// Mark turns that were still running when the previous process stopped, so
    /// a restart never leaves a phantom "working" task behind. Returns how many
    /// tasks were recovered.
    pub fn recover_interrupted(&mut self) -> usize {
        let now = now_millis();
        // `true` marks a turn that was running; `false` one still queued.
        let recovered: Vec<(String, String, bool)> = self
            .tasks
            .iter_mut()
            .filter(|task| task.status == "running" || task.status == "queued")
            .map(|task| {
                let was_running = task.status == "running";
                task.status = if was_running {
                    "interrupted".to_string()
                } else {
                    // The admission queue lives in the worker's channel, which
                    // is memory only: a restart drops it, so a `202 queued`
                    // answer can never be honoured. Fail the row honestly
                    // instead of interrupting a turn that never ran.
                    "failed".to_string()
                };
                (task.id.clone(), task.conversation_id.clone(), was_running)
            })
            .collect();
        for (task_id, conversation_id, was_running) in &recovered {
            // A queued placeholder can carry no conversation — the engine only
            // names the chat when the turn starts. Give it one now so the
            // failed row, its prompt, and the explanation stay visible (and so
            // the orphan prune keeps it).
            let conversation_id = if conversation_id.is_empty() {
                let synthetic = format!("recovered-{task_id}");
                let prompt = self
                    .tasks
                    .iter()
                    .find(|task| &task.id == task_id)
                    .map(|task| task.prompt.clone())
                    .unwrap_or_default();
                {
                    let conversation = self.conversation_mut(&synthetic);
                    conversation.title = prompt.chars().take(64).collect();
                    conversation.updated_at = now;
                    if !conversation.task_ids.iter().any(|id| id == task_id) {
                        conversation.task_ids.push(task_id.clone());
                    }
                }
                if let Some(task) = self.tasks.iter_mut().find(|task| &task.id == task_id) {
                    task.conversation_id = synthetic.clone();
                }
                self.push_transcript(TranscriptEntry {
                    conversation_id: synthetic.clone(),
                    kind: "user".into(),
                    text: prompt,
                    task_id: task_id.clone(),
                    at: now.saturating_sub(1),
                    ..Default::default()
                });
                synthetic
            } else {
                conversation_id.clone()
            };
            let text = if *was_running {
                "Marble restarted while this turn was running. It is marked interrupted; \
                 send the message again to continue."
                    .to_string()
            } else {
                "Marble restarted before this turn could run, and its place in line was lost. \
                 It is marked failed; retry or send the message again."
                    .to_string()
            };
            self.push_transcript(TranscriptEntry {
                conversation_id,
                kind: "system".into(),
                text,
                task_id: task_id.clone(),
                at: now,
                ..Default::default()
            });
            let outcome = if *was_running {
                "interrupted"
            } else {
                "failed"
            };
            self.routines.note_settled(task_id, outcome);
        }
        self.presence.recovered_tasks = recovered.len() as u64;
        // The roster is durable too, and a marble mid-dispatch is the case a
        // restart used to leave permanently unusable: `working` with no engine
        // behind it. Clearing it here keeps the roster honest about the crash.
        let marbles = self.prism.recover_interrupted();
        if marbles > 0 {
            tracing::warn!(marbles, "cleared prism turns interrupted by a restart");
        }
        if !recovered.is_empty() || marbles > 0 {
            self.bump();
        }
        recovered.len()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_presence_only_counts_the_owning_agents_running_tasks() {
        let mut state = super::ProductState::default();
        let ada = state
            .prism
            .add("Ada", "writer", "circle", 20, "", "")
            .unwrap()
            .id
            .clone();
        state.tasks.push(super::TaskRecord {
            id: "a".into(),
            marble_id: ada.clone(),
            owner: "prism".into(),
            status: "running".into(),
            ..Default::default()
        });
        let doc = state.prism_document();
        assert_eq!(doc["marbles"][0]["state"], "idle");
        assert_eq!(doc["marbles"][1]["state"], "working");
        assert_eq!(
            state.presence.document(&state)["agents"][&ada]["activeTaskIds"][0],
            "a"
        );
        state.tasks.push(super::TaskRecord {
            id: "d".into(),
            owner: "agent".into(),
            status: "running".into(),
            ..Default::default()
        });
        assert_eq!(state.prism_document()["marbles"][0]["state"], "working");
        state.tasks[0].status = "failed".into();
        assert_eq!(state.prism_document()["marbles"][1]["isWorking"], false);
        state.tasks.clear();
        assert_eq!(state.prism_document()["working"], 0);
    }
    use super::*;

    #[test]
    fn conversation_records_are_created_once_and_reused() {
        let mut state = ProductState::default();
        state.conversation_mut("c1").title = "first".into();
        state.conversation_mut("c1").title = "second".into();
        assert_eq!(state.conversations.len(), 1);
        assert_eq!(state.conversations[0].title, "second");
        assert_eq!(state.bump(), 1);
        assert_eq!(state.bump(), 2);
    }

    #[test]
    fn legacy_state_documents_decode_with_defaulted_fields() {
        let legacy = serde_json::json!({
            "conversations": [{"id": "c1", "taskIds": ["t1"]}],
            "tasks": [{"id": "t1", "conversationId": "c1", "status": "completed", "owner": "human"}],
            "approvals": [],
            "transcript": [{"conversationId": "c1", "kind": "assistant", "text": "hi"}]
        });
        let state = ProductState::decode(&legacy).expect("legacy state should decode");
        assert_eq!(state.revision, 0);
        assert_eq!(state.conversations[0].title, "");
        assert_eq!(state.tasks[0].at, 0);
        assert_eq!(state.tasks[0].routine_id, "");
        assert_eq!(state.tasks[0].marble_id, "");
        assert_eq!(state.transcript[0].task_id, "");
        assert!(state.memory.facts.is_empty());
        assert!(state.routines.routines.is_empty());
        assert!(!state.voice.enabled);
        assert!(state.devices.is_empty());
        assert_eq!(state.retention, RetentionState::default());
    }

    fn settled_task(id: &str, at: u64) -> TaskRecord {
        TaskRecord {
            marble_id: String::new(),
            id: id.into(),
            conversation_id: "c1".into(),
            status: "completed".into(),
            owner: "agent".into(),
            prompt: String::new(),
            at,
            routine_id: String::new(),
        }
    }

    #[test]
    fn pruning_bounds_transcript_tasks_and_conversations() {
        let mut state = ProductState::default();
        for index in 0..(TRANSCRIPT_LIMIT + 12) {
            state.push_transcript(TranscriptEntry {
                conversation_id: format!("c{}", index % 4),
                kind: "assistant".into(),
                text: format!("entry {index}"),
                task_id: String::new(),
                at: index as u64,
                ..Default::default()
            });
        }
        for index in 0..(TASK_LIMIT + 7) {
            state
                .tasks
                .push(settled_task(&format!("t{index}"), index as u64));
        }
        for index in 0..(CONVERSATION_LIMIT + 5) {
            let conversation = state.conversation_mut(&format!("old{index}"));
            conversation.updated_at = index as u64;
        }
        let summary = state.prune(9_999);
        assert!(summary.transcript > 0, "{summary:?}");
        assert_eq!(summary.tasks, TASK_LIMIT + 7 - TASK_LIMIT);
        assert!(summary.conversations > 0, "{summary:?}");
        assert!(state.transcript.len() <= TRANSCRIPT_LIMIT);
        assert!(state.tasks.len() <= TASK_LIMIT);
        assert!(state.conversations.len() <= CONVERSATION_LIMIT);
        assert_eq!(state.retention.last_trim_at, 9_999);
        assert!(state.retention.trimmed_transcript > 0);
        assert!(state.prune(10_000).is_empty(), "a second pass is a no-op");
    }

    #[test]
    fn conversations_can_be_renamed_deleted_and_searched() {
        let mut state = ProductState::default();
        state.conversation_mut("c1").title = "old".into();
        state.push_transcript(TranscriptEntry {
            conversation_id: "c1".into(),
            kind: "assistant".into(),
            text: "the deploy key is in 1Password".into(),
            task_id: "t1".into(),
            at: 5,
            ..Default::default()
        });
        state.tasks.push(settled_task("t1", 5));

        assert!(state.rename_conversation("c1", "  Deploys  "));
        assert_eq!(state.conversations[0].title, "Deploys");
        assert!(!state.rename_conversation("c1", "   "));
        assert!(!state.rename_conversation("missing", "x"));

        assert_eq!(state.search_transcript("DEPLOY KEY", 10).len(), 1);
        assert!(state.search_transcript("nothing here", 10).is_empty());

        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "t2".into(),
            conversation_id: "c1".into(),
            status: "running".into(),
            owner: "agent".into(),
            prompt: "working".into(),
            at: 6,
            routine_id: String::new(),
        });
        assert!(state.delete_conversation("c1").is_err());
        state.tasks[1].status = "completed".into();
        assert!(state.delete_conversation("c1").is_ok());
        assert!(state.conversations.is_empty());
        assert!(state.transcript.is_empty());
        assert!(state.tasks.is_empty());
        assert!(state.delete_conversation("c1").is_err());
    }

    #[test]
    fn pruning_never_drops_unfinished_work() {
        let mut state = ProductState::default();
        for index in 0..TASK_LIMIT {
            state
                .tasks
                .push(settled_task(&format!("t{index}"), index as u64));
        }
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "running".into(),
            conversation_id: "c1".into(),
            status: "running".into(),
            owner: "agent".into(),
            prompt: "working".into(),
            at: 0,
            routine_id: String::new(),
        });
        state.prune(1);
        assert!(state.tasks.iter().any(|task| task.id == "running"));
        assert!(state.tasks.len() <= TASK_LIMIT);
    }

    #[test]
    fn restarting_marks_running_turns_interrupted() {
        let mut state = ProductState::default();
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "t1".into(),
            conversation_id: "c1".into(),
            status: "running".into(),
            owner: "agent".into(),
            prompt: "tidy".into(),
            at: 1,
            routine_id: String::new(),
        });
        assert_eq!(state.recover_interrupted(), 1);
        assert_eq!(state.tasks[0].status, "interrupted");
        assert_eq!(state.presence.recovered_tasks, 1);
        let entry = state.transcript.last().expect("a recovery note");
        assert_eq!(entry.kind, "system");
        assert!(entry.text.contains("restarted"));
        assert_eq!(state.recover_interrupted(), 0);
    }

    #[test]
    fn restarting_fails_a_queued_turn_instead_of_interrupting_it() {
        // A queued placeholder is a promise kept in a memory-only channel: the
        // restart drops it, so the row fails with a reason rather than being
        // interrupted as if the turn had run.
        let mut state = ProductState::default();
        state.conversation_mut("c1").title = "chat".into();
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "queued-1".into(),
            conversation_id: "c1".into(),
            status: "queued".into(),
            owner: "agent".into(),
            prompt: "later".into(),
            at: 1,
            routine_id: String::new(),
        });
        assert_eq!(state.recover_interrupted(), 1);
        assert_eq!(state.tasks[0].status, "failed");
        let entry = state.transcript.last().expect("a recovery note");
        assert_eq!(entry.kind, "system");
        assert!(entry.text.contains("before this turn could run"));
        assert!(entry.text.contains("failed"));
    }

    #[test]
    fn restarting_gives_a_conversationless_queued_turn_a_chat() {
        // A message admitted before the engine named its conversation carried
        // no conversation id. Failing it must not orphan the row — the prompt
        // and the explanation land in a chat that stays open.
        let mut state = ProductState::default();
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "queued-9".into(),
            conversation_id: String::new(),
            status: "queued".into(),
            owner: "agent".into(),
            prompt: "what did I ask".into(),
            at: 1,
            routine_id: String::new(),
        });
        assert_eq!(state.recover_interrupted(), 1);
        let task = &state.tasks[0];
        assert_eq!(task.status, "failed");
        assert_eq!(task.conversation_id, "recovered-queued-9");
        let conversation = state
            .conversations
            .iter()
            .find(|conversation| conversation.id == "recovered-queued-9")
            .expect("a recovered chat");
        assert_eq!(conversation.title, "what did I ask");
        assert_eq!(conversation.task_ids, vec!["queued-9".to_string()]);
        let kinds: Vec<&str> = state
            .transcript
            .iter()
            .filter(|entry| entry.conversation_id == "recovered-queued-9")
            .map(|entry| entry.kind.as_str())
            .collect();
        assert_eq!(kinds, ["user", "system"]);
        // And the orphan prune agrees this is not an orphan.
        assert_eq!(state.prune_orphaned_tasks(), 0);
    }

    #[test]
    fn settled_tasks_survive_a_restart_unchanged() {
        let mut state = ProductState::default();
        state.conversation_mut("c1").title = "chat".into();
        state.tasks.push(settled_task("t-done", 1));
        state.tasks.push(settled_task("t-cancelled", 2));
        state.tasks[1].status = "cancelled".into();
        assert_eq!(state.recover_interrupted(), 0);
        assert_eq!(state.tasks[0].status, "completed");
        assert_eq!(state.tasks[1].status, "cancelled");
        assert!(state.transcript.is_empty(), "no note for settled work");
    }

    #[test]
    fn the_orphan_prune_keeps_queued_work_without_a_conversation() {
        let mut state = ProductState::default();
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "queued-3".into(),
            conversation_id: String::new(),
            status: "queued".into(),
            owner: "agent".into(),
            prompt: "waiting".into(),
            at: 1,
            routine_id: String::new(),
        });
        assert_eq!(state.prune_orphaned_tasks(), 0);
        assert_eq!(state.tasks.len(), 1);
    }

    #[test]
    fn restarting_also_clears_a_marble_that_was_working() {
        let mut state = ProductState::default();
        let marble = state
            .prism
            .add("Scout", "reads the repo", "hex", 210, "", "")
            .expect("a marble")
            .id
            .clone();
        state.prism.marbles[0].state = "working".into();
        state.prism.marbles[0].task_id = "task-1".into();
        assert_eq!(state.recover_interrupted(), 0, "no host task was running");
        let recovered = &state.prism.get(&marble).expect("the marble stays");
        assert_eq!(recovered.state, "blocked");
        assert!(recovered.last_error.contains("restarted"));
        assert!(recovered.task_id.is_empty());
    }
}

#[cfg(test)]
mod everyone_tests {
    use super::*;

    fn everyone_entry(task_id: &str, kind: &str) -> TranscriptEntry {
        TranscriptEntry {
            conversation_id: crate::everyone::EVERYONE_CONVERSATION.into(),
            kind: kind.into(),
            text: "line".into(),
            task_id: task_id.into(),
            at: 1,
            speaker: "user".into(),
            speaker_name: "You".into(),
            handoff: None,
            ..Default::default()
        }
    }

    #[test]
    fn deleting_a_conversation_takes_its_everyone_echoes() {
        let mut state = ProductState::default();
        state.conversations.push(ConversationRecord {
            id: "c-1".into(),
            title: "chat".into(),
            task_ids: vec![],
            updated_at: 0,
            lane: 0,
            ..Default::default()
        });
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "t-1".into(),
            conversation_id: "c-1".into(),
            status: "completed".into(),
            owner: "agent".into(),
            prompt: "hi".into(),
            at: 1,
            routine_id: String::new(),
        });
        // The person's stamped line, the mirrored reply, a handoff echo —
        // all tied to t-1 — plus an unstamped busy note and an unrelated
        // conversation's entry.
        state.transcript.push(everyone_entry("t-1", "user"));
        state.transcript.push(everyone_entry("t-1", "assistant"));
        state.transcript.push(everyone_entry("t-1", "handoff"));
        state.transcript.push(everyone_entry("", "system"));
        state
            .transcript
            .push(everyone_entry("t-other", "assistant"));
        state.transcript.push(TranscriptEntry {
            conversation_id: "c-1".into(),
            ..everyone_entry("t-1", "user")
        });
        state.delete_conversation("c-1").unwrap();
        let left = everyone_in(&state);
        assert_eq!(left.len(), 2, "{left:?}");
        assert!(left.iter().all(|entry| entry.task_id != "t-1"));
    }

    #[test]
    fn everyone_entries_quoting_dead_tasks_are_pruned() {
        let mut state = ProductState::default();
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "t-live".into(),
            conversation_id: "c".into(),
            status: "completed".into(),
            owner: "agent".into(),
            prompt: "hi".into(),
            at: 1,
            routine_id: String::new(),
        });
        state.transcript.push(everyone_entry("t-live", "assistant"));
        state.transcript.push(everyone_entry("t-gone", "assistant"));
        state.transcript.push(everyone_entry("t-gone", "user"));
        state.transcript.push(everyone_entry("", "system"));
        state.transcript.push(everyone_entry("", "user"));
        assert_eq!(state.prune_everyone_entries(), 2);
        let left = everyone_in(&state);
        assert_eq!(left.len(), 3, "{left:?}");
    }

    #[test]
    fn clearing_everyone_empties_the_timeline_only() {
        let mut state = ProductState::default();
        state.transcript.push(everyone_entry("", "user"));
        state.transcript.push(everyone_entry("t-1", "assistant"));
        state.transcript.push(TranscriptEntry {
            conversation_id: "chat-1".into(),
            ..everyone_entry("t-1", "assistant")
        });
        state
            .everyone_pending
            .insert("task:t-1".into(), "marble".into());
        assert_eq!(state.clear_everyone(), 2);
        assert_eq!(state.transcript.len(), 1);
        assert_eq!(state.transcript[0].conversation_id, "chat-1");
        assert!(state.everyone_pending.is_empty());
    }

    fn everyone_in(state: &ProductState) -> Vec<&TranscriptEntry> {
        state
            .transcript
            .iter()
            .filter(|entry| entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION)
            .collect()
    }
}

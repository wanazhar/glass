//! The single durable turn worker.
//!
//! One runtime subprocess serves every conversation, so turns are serialized
//! through one channel. Each command is accepted with its durable task id as
//! soon as the runtime admits it, which lets the HTTP surface answer at once
//! and stream the rest.
//!
//! The worker is also where the assistant behaves like an always-on assistant
//! rather than a request handler: it carries remembered facts into each turn,
//! notices details worth remembering, records presence, and notifies the person
//! when a turn lands or fails.

use crate::events::EventBus;
use crate::memory;
use crate::notify::{self, Notification};
use crate::state::{ProductState, TaskRecord, TranscriptEntry};
use crate::store::{self, StateStore};
use crate::{
    InboundMessage, MarbleError, MarbleTurn, TurnOptions, conversation::Conversation, to_turn_input,
};
use glass_runtime_client::{TurnInput, protocol::ContextBlock};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One message waiting for the durable turn worker.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub conversation_id: Option<String>,
    pub text: String,
    pub request_id: String,
    pub cancel: Arc<AtomicBool>,
    /// Set when a routine, not a person, admitted this turn.
    pub routine_id: Option<String>,
}

/// Durable identity of an admitted turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedTurn {
    pub conversation_id: String,
    pub task_id: String,
}

/// Queued turn plus the acknowledgement channel used by the HTTP surface.
pub struct TurnCommand {
    pub request: TurnRequest,
    pub accepted: mpsc::Sender<Result<AcceptedTurn, MarbleError>>,
}

/// Cancel flags for the turns the worker is actually running.
///
/// The worker owns this table because it is the only place that knows a task id
/// for every turn: a routine firing on its own clock and a queued message
/// dispatched by resuming admission are both running turns, and a person who
/// presses stop should not have to care which one it is.
#[derive(Debug, Clone, Default)]
pub struct CancelRegistry {
    flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl CancelRegistry {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.flags.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn register(&self, task_id: &str, flag: Arc<AtomicBool>) {
        self.map().insert(task_id.to_string(), flag);
    }

    pub fn forget(&self, task_id: &str) {
        self.map().remove(task_id);
    }

    /// Ask one running turn to stop; returns whether it was still running.
    pub fn request(&self, task_id: &str) -> bool {
        let flag = self.map().get(task_id).cloned();
        match flag {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Every running turn, so one deliberate stop can cancel all of them.
    pub fn running(&self) -> Vec<(String, Arc<AtomicBool>)> {
        self.map()
            .iter()
            .map(|(task_id, flag)| (task_id.clone(), Arc::clone(flag)))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.map().len()
    }

    pub fn is_empty(&self) -> bool {
        self.map().is_empty()
    }
}

/// Shared view of the turn worker: how much work is waiting, and what it is
/// running right now.
#[derive(Debug, Clone, Default)]
pub struct TurnQueue {
    pending: Arc<AtomicUsize>,
    cancels: CancelRegistry,
}

impl TurnQueue {
    /// Commands a surface handed over that the worker has not picked up yet.
    pub fn waiting(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }

    /// Count one command handed to the worker; returns its place in line.
    pub fn note_enqueued(&self) -> usize {
        self.pending
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1)
    }

    /// Count one command the worker started, or one that was never delivered.
    pub fn note_finished(&self) {
        // Compare-and-set rather than a plain decrement, so a mismatched call
        // can never wrap the counter around to a huge place in line.
        let mut waiting = self.pending.load(Ordering::SeqCst);
        while waiting > 0 {
            match self.pending.compare_exchange(
                waiting,
                waiting - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return,
                Err(current) => waiting = current,
            }
        }
    }

    pub fn cancels(&self) -> &CancelRegistry {
        &self.cancels
    }

    pub fn request_cancel(&self, task_id: &str) -> bool {
        self.cancels.request(task_id)
    }

    pub fn running(&self) -> Vec<(String, Arc<AtomicBool>)> {
        self.cancels.running()
    }
}

/// Current unix time in milliseconds.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn truncate(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let mut title: String = trimmed.chars().take(limit).collect();
    title.push('…');
    title
}

fn lock(state: &Mutex<ProductState>) -> std::sync::MutexGuard<'_, ProductState> {
    state.lock().unwrap_or_else(|error| error.into_inner())
}

/// Start the durable turn worker and return its command channel.
pub fn spawn_turn_worker(
    mut conversation: Conversation,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    events: EventBus,
    timeout: Duration,
    queue: TurnQueue,
) -> mpsc::Sender<TurnCommand> {
    let (sender, receiver) = mpsc::channel::<TurnCommand>();
    let spawned = std::thread::Builder::new()
        .name("glass-marble-turns".into())
        .spawn(move || {
            while let Ok(command) = receiver.recv() {
                queue.note_finished();
                run_command(
                    command,
                    &mut conversation,
                    &state,
                    store.as_deref(),
                    &events,
                    timeout,
                    &queue,
                );
            }
            let _ = conversation.shutdown();
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "could not start the marble turn worker");
    }
    sender
}

fn run_command(
    command: TurnCommand,
    conversation: &mut Conversation,
    state: &Arc<Mutex<ProductState>>,
    store: Option<&StateStore>,
    events: &EventBus,
    timeout: Duration,
    queue: &TurnQueue,
) {
    let TurnCommand { request, accepted } = command;
    let mut sender = Some(accepted);
    // A turn cannot start on an engine that stopped answering. Replacing it
    // here, before the durable task exists, means the person gets either a real
    // answer or a clear failure — never a turn that was silently lost.
    match conversation.ensure_runtime() {
        Ok(false) => {}
        Ok(true) => {
            let (snapshot, restarts) = {
                let mut guard = lock(state);
                guard.presence.runtime.note_restart();
                guard.bump();
                let restarts = guard.presence.runtime.restarts;
                (guard.clone(), restarts)
            };
            if let Some(store) = store {
                store::persist(store, &snapshot);
            }
            events.publish(
                "",
                "",
                "runtime",
                json!({"restarted": true, "restarts": restarts, "revision": snapshot.revision}),
            );
        }
        Err(error) => {
            {
                let mut guard = lock(state);
                guard.presence.runtime.note_failure(error.to_string());
                guard.bump();
            }
            events.publish(
                "",
                "",
                "runtime",
                json!({"restarted": false, "error": error.to_string()}),
            );
            if let Some(sender) = sender.take() {
                let _ = sender.send(Err(error));
            }
            return;
        }
    }
    let prompt = request.text.clone();
    // A model chosen from a surface applies to conversations created from now
    // on; Core keeps an existing session on the model it started with.
    let model = {
        let guard = lock(state);
        guard.model.trim().to_string()
    };
    conversation.set_model((!model.is_empty()).then_some(model));
    let turn = MarbleTurn {
        conversation_id: request.conversation_id.clone(),
        request_id: request.request_id.clone(),
        input: turn_input(state, &prompt),
    };
    let mut accepted_turn: Option<AcceptedTurn> = None;
    let routine_id = request.routine_id.clone();
    let cancel = Arc::clone(&request.cancel);
    let mut on_accepted = |conversation_id: &str, task_id: &str| {
        let identity = AcceptedTurn {
            conversation_id: conversation_id.to_string(),
            task_id: task_id.to_string(),
        };
        accepted_turn = Some(identity.clone());
        // The worker registers the flag, so every admitted turn is cancellable —
        // including one a routine fired or one a resume dispatched.
        queue.cancels.register(task_id, Arc::clone(&cancel));
        if let Some(sender) = sender.take() {
            let _ = sender.send(Ok(identity));
        }
        let snapshot = {
            let mut guard = lock(state);
            guard.presence.mark_turn();
            guard.bump();
            let now = now_millis();
            let title = truncate(&prompt, 64);
            let record = guard.conversation_mut(conversation_id);
            if record.title.is_empty() {
                record.title = title;
            }
            record.updated_at = now;
            if !record.task_ids.iter().any(|id| id == task_id) {
                record.task_ids.push(task_id.to_string());
            }
            guard.upsert_task(TaskRecord {
                id: task_id.to_string(),
                conversation_id: conversation_id.to_string(),
                status: "running".into(),
                owner: "agent".into(),
                prompt: prompt.clone(),
                at: now,
                routine_id: routine_id.clone().unwrap_or_default(),
            });
            guard.push_transcript(TranscriptEntry {
                conversation_id: conversation_id.to_string(),
                kind: "user".into(),
                text: prompt.clone(),
                task_id: task_id.to_string(),
                at: now,
            });
            if let Some(routine_id) = &routine_id {
                guard.routines.note_run(routine_id, task_id, now);
            }
            guard.memory.note_use();
            guard.clone()
        };
        if let Some(store) = store {
            store::persist(store, &snapshot);
        }
        events.publish(
            conversation_id,
            task_id,
            "state",
            json!({"revision": snapshot.revision, "status": "running"}),
        );
    };
    let result = crate::run_turn(
        &turn,
        conversation,
        TurnOptions {
            cancel: Some(request.cancel.as_ref()),
            timeout,
            on_accepted: Some(&mut on_accepted),
        },
    );
    match result {
        Ok(reply) => {
            queue.cancels.forget(&reply.task_id);
            let (snapshot, suggestion) = {
                let mut guard = lock(state);
                guard.presence.mark_settled();
                guard.presence.runtime.note_ok();
                guard.bump();
                let now = now_millis();
                let status = if reply.cancelled {
                    "cancelled"
                } else {
                    "completed"
                };
                guard.set_task_status(&reply.task_id, status);
                guard.conversation_mut(&reply.conversation_id).updated_at = now;
                if !reply.reasoning.trim().is_empty() {
                    guard.push_transcript(TranscriptEntry {
                        conversation_id: reply.conversation_id.clone(),
                        kind: "reasoning".into(),
                        text: reply.reasoning.clone(),
                        task_id: reply.task_id.clone(),
                        at: now,
                    });
                }
                if !reply.text.trim().is_empty() {
                    guard.push_transcript(TranscriptEntry {
                        conversation_id: reply.conversation_id.clone(),
                        kind: "assistant".into(),
                        text: reply.text.clone(),
                        task_id: reply.task_id.clone(),
                        at: now,
                    });
                }
                guard.routines.note_settled(&reply.task_id, status);
                // A durable detail is offered, never memorized silently.
                let suggestion = memory::detect(&prompt).and_then(|(text, kind, reason)| {
                    guard
                        .memory
                        .suggest(&text, &kind, &reason, &reply.conversation_id)
                        .map(|suggestion| (suggestion, kind))
                });
                (guard.clone(), suggestion)
            };
            if let Some(store) = store {
                store::persist(store, &snapshot);
            }
            events.publish(
                &reply.conversation_id,
                &reply.task_id,
                "state",
                json!({
                    "revision": snapshot.revision,
                    "status": if reply.cancelled { "cancelled" } else { "completed" },
                }),
            );
            if let Some((suggestion, kind)) = suggestion {
                events.publish(
                    &reply.conversation_id,
                    &reply.task_id,
                    "memory_suggestion",
                    json!({
                        "suggestionId": suggestion.id,
                        "text": suggestion.text,
                        "kind": kind,
                    }),
                );
                notify::spawn_delivery(
                    snapshot.notifications.clone(),
                    snapshot.devices.clone(),
                    Notification::new(
                        "memory",
                        "Remember this?",
                        format!("Marble can remember: {}", suggestion.text),
                        &reply.conversation_id,
                        &reply.task_id,
                    ),
                );
            }
            if !reply.cancelled {
                let title = snapshot
                    .conversations
                    .iter()
                    .find(|record| record.id == reply.conversation_id)
                    .map(|record| record.title.clone())
                    .unwrap_or_else(|| "Marble".into());
                notify::spawn_delivery(
                    snapshot.notifications.clone(),
                    snapshot.devices.clone(),
                    Notification::new(
                        "settled",
                        title,
                        reply.preview(220),
                        &reply.conversation_id,
                        &reply.task_id,
                    ),
                );
            }
        }
        Err(error) => {
            if let Some(sender) = sender.take() {
                let _ = sender.send(Err(error));
                return;
            }
            if let Some(identity) = &accepted_turn {
                queue.cancels.forget(&identity.task_id);
                let transport_failed = conversation.transport_failed();
                let snapshot = {
                    let mut guard = lock(state);
                    guard.presence.mark_settled();
                    if transport_failed {
                        guard.presence.runtime.note_failure(error.to_string());
                    }
                    guard.bump();
                    guard.set_task_status(&identity.task_id, "failed");
                    guard.routines.note_settled(&identity.task_id, "failed");
                    guard.push_transcript(TranscriptEntry {
                        conversation_id: identity.conversation_id.clone(),
                        kind: "system".into(),
                        text: error.to_string(),
                        task_id: identity.task_id.clone(),
                        at: now_millis(),
                    });
                    guard.clone()
                };
                if let Some(store) = store {
                    store::persist(store, &snapshot);
                }
                events.publish(
                    &identity.conversation_id,
                    &identity.task_id,
                    "error",
                    json!({"message": error.to_string(), "revision": snapshot.revision}),
                );
                notify::spawn_delivery(
                    snapshot.notifications.clone(),
                    snapshot.devices.clone(),
                    Notification::new(
                        "failed",
                        "Marble hit a problem",
                        error.to_string(),
                        &identity.conversation_id,
                        &identity.task_id,
                    ),
                );
            } else {
                events.publish("", "", "error", json!({ "message": error.to_string() }));
            }
        }
    }
}

/// Carry remembered facts into the turn as context, so memory is used rather
/// than merely stored.
fn turn_input(state: &Arc<Mutex<ProductState>>, text: &str) -> TurnInput {
    let mut input = to_turn_input(&InboundMessage::new(text));
    let block = {
        let guard = lock(state);
        guard.memory.context_block()
    };
    if let Some(block) = block {
        input.context.push(ContextBlock::Text { text: block });
    }
    input
}

/// Convenience constructor for one inbound message.
impl TurnRequest {
    pub fn new(conversation_id: Option<String>, text: impl Into<String>) -> Self {
        Self {
            conversation_id,
            text: text.into(),
            request_id: crate::next_request_id(),
            cancel: Arc::new(AtomicBool::new(false)),
            routine_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_reports_its_depth_and_cancels_what_it_runs() {
        let queue = TurnQueue::default();
        assert_eq!(queue.waiting(), 0);
        assert_eq!(queue.note_enqueued(), 1);
        assert_eq!(queue.note_enqueued(), 2);
        assert_eq!(queue.waiting(), 2);
        queue.note_finished();
        assert_eq!(queue.waiting(), 1);

        let flag = Arc::new(AtomicBool::new(false));
        queue.cancels().register("task-1", Arc::clone(&flag));
        assert_eq!(queue.running().len(), 1);
        assert!(queue.request_cancel("task-1"));
        assert!(flag.load(Ordering::Relaxed));
        assert!(!queue.request_cancel("task-2"));
        queue.cancels().forget("task-1");
        assert!(!queue.request_cancel("task-1"));
    }

    #[test]
    fn titles_are_bounded_and_trimmed() {
        assert_eq!(truncate("  hello  ", 10), "hello");
        assert_eq!(truncate("abcdefghij", 4), "abcd…");
    }

    #[test]
    fn request_ids_are_unique_per_message() {
        let first = TurnRequest::new(None, "a");
        let second = TurnRequest::new(None, "a");
        assert_ne!(first.request_id, second.request_id);
        assert!(first.routine_id.is_none());
    }

    #[test]
    fn remembered_facts_are_carried_into_the_turn() {
        let mut product = ProductState::default();
        let plain = turn_input(&Arc::new(Mutex::new(product.clone())), "hello");
        assert!(plain.context.is_empty());
        product
            .memory
            .remember("my name is Azhar", "role", "c1", true)
            .unwrap();
        let with_memory = turn_input(&Arc::new(Mutex::new(product)), "hello");
        assert_eq!(with_memory.context.len(), 1);
        assert_eq!(with_memory.text, "hello");
        match &with_memory.context[0] {
            ContextBlock::Text { text } => assert!(text.contains("my name is Azhar")),
            other => panic!("unexpected context block: {other:?}"),
        }
    }
}

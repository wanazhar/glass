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
use std::sync::atomic::AtomicBool;
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
) -> mpsc::Sender<TurnCommand> {
    let (sender, receiver) = mpsc::channel::<TurnCommand>();
    let spawned = std::thread::Builder::new()
        .name("glass-marble-turns".into())
        .spawn(move || {
            while let Ok(command) = receiver.recv() {
                run_command(
                    command,
                    &mut conversation,
                    &state,
                    store.as_deref(),
                    &events,
                    timeout,
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
) {
    let TurnCommand { request, accepted } = command;
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
    let mut sender = Some(accepted);
    let mut accepted_turn: Option<AcceptedTurn> = None;
    let routine_id = request.routine_id.clone();
    let mut on_accepted = |conversation_id: &str, task_id: &str| {
        let identity = AcceptedTurn {
            conversation_id: conversation_id.to_string(),
            task_id: task_id.to_string(),
        };
        accepted_turn = Some(identity.clone());
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
            let (snapshot, suggestion) = {
                let mut guard = lock(state);
                guard.presence.mark_settled();
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
                let snapshot = {
                    let mut guard = lock(state);
                    guard.presence.mark_settled();
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

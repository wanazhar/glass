//! The single durable turn worker.
//!
//! One runtime subprocess serves every conversation, so turns are serialized
//! through one channel. Each command is accepted with its durable task id as
//! soon as the runtime admits it, which lets the HTTP surface answer at once
//! and stream the rest.

use crate::events::EventBus;
use crate::state::{ProductState, TaskRecord, TranscriptEntry};
use crate::store::{self, StateStore};
use crate::{
    InboundMessage, MarbleError, MarbleTurn, TurnOptions, conversation::Conversation, to_turn_input,
};
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
    let turn = MarbleTurn {
        conversation_id: request.conversation_id.clone(),
        request_id: request.request_id.clone(),
        input: to_turn_input(&InboundMessage::new(&request.text)),
    };
    let mut sender = Some(accepted);
    let mut accepted_turn: Option<AcceptedTurn> = None;
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
            guard.bump();
            let now = now_millis();
            let title = truncate(&request.text, 64);
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
                prompt: request.text.clone(),
                at: now,
            });
            guard.push_transcript(TranscriptEntry {
                conversation_id: conversation_id.to_string(),
                kind: "user".into(),
                text: request.text.clone(),
                task_id: task_id.to_string(),
                at: now,
            });
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
            let snapshot = {
                let mut guard = lock(state);
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
                guard.clone()
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
        }
        Err(error) => {
            if let Some(sender) = sender.take() {
                let _ = sender.send(Err(error));
                return;
            }
            if let Some(identity) = &accepted_turn {
                let snapshot = {
                    let mut guard = lock(state);
                    guard.bump();
                    guard.set_task_status(&identity.task_id, "failed");
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
            } else {
                events.publish("", "", "error", json!({"message": error.to_string()}));
            }
        }
    }
}

/// Convenience constructor for one inbound message.
impl TurnRequest {
    pub fn new(conversation_id: Option<String>, text: impl Into<String>) -> Self {
        Self {
            conversation_id,
            text: text.into(),
            request_id: crate::next_request_id(),
            cancel: Arc::new(AtomicBool::new(false)),
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
    }
}

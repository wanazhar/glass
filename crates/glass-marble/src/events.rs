//! In-process event fan-out for live product surfaces.
//!
//! One durable turn publishes ordered events (text deltas, reasoning, tool
//! activity, usage, settlement). The web UI and the iOS client consume the
//! same stream, so a reconnecting surface replays from its own cursor instead
//! of re-reading the whole transcript.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, mpsc};

/// Events retained for cursor replay after a reconnect.
pub const EVENT_BUFFER: usize = 512;
/// Bound on queued events per live subscriber before it is dropped.
const LIVE_QUEUE: usize = 256;

/// One ordered product event for a single durable turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnEvent {
    pub sequence: u64,
    pub conversation_id: String,
    pub task_id: String,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Default)]
struct BusState {
    reply_ids: HashSet<String>,
    pending_ids: std::collections::HashMap<String, String>,
    reply_revision: u64,
    replies_initialized: bool,
    task_threads: std::collections::HashMap<String, String>,
    sequence: u64,
    recent: VecDeque<TurnEvent>,
    subscribers: Vec<mpsc::SyncSender<TurnEvent>>,
}

/// Multi-subscriber ordered event bus with bounded replay.
#[derive(Debug, Clone, Default)]
pub struct EventBus {
    state: Arc<Mutex<BusState>>,
}

impl EventBus {
    pub(crate) fn attach_replies(&self, state: &mut crate::ProductState) {
        state.ensure_entry_ids();
        let mut bus = self.lock();
        if !bus.replies_initialized || state.thread_events.0.is_none() {
            bus.reply_ids.extend(
                state
                    .transcript
                    .iter()
                    .filter(|e| matches!(e.kind.as_str(), "assistant" | "failure"))
                    .map(|e| e.id.clone()),
            );
            bus.pending_ids.extend(
                state
                    .transcript
                    .iter()
                    .filter(|e| e.kind == "pending")
                    .map(|e| (e.id.clone(), e.task_id.clone())),
            );
            bus.reply_revision = state.revision;
            bus.replies_initialized = true;
        }
        state.thread_events.0 = Some(self.clone());
    }

    pub(crate) fn publish_replies(&self, snapshot: &crate::ProductState) {
        let mut bus = self.lock();
        if !bus.replies_initialized || snapshot.revision < bus.reply_revision {
            return;
        }
        bus.reply_revision = snapshot.revision;
        let ids: HashSet<_> = snapshot.transcript.iter().map(|e| e.id.as_str()).collect();
        bus.reply_ids.retain(|id| ids.contains(id.as_str()));
        bus.pending_ids.retain(|id, _| ids.contains(id.as_str()));
        for entry in &snapshot.transcript {
            if entry.kind == "pending" {
                if bus.pending_ids.get(&entry.id) != Some(&entry.task_id) {
                    bus.pending_ids
                        .insert(entry.id.clone(), entry.task_id.clone());
                    Self::publish_locked(
                        &mut bus,
                        &entry.conversation_id,
                        &entry.task_id,
                        "entry",
                        serde_json::to_value(entry).unwrap(),
                    );
                }
                continue;
            }
            if bus.pending_ids.remove(&entry.id).is_some() && entry.kind == "system" {
                Self::publish_locked(
                    &mut bus,
                    &entry.conversation_id,
                    &entry.task_id,
                    "entry",
                    serde_json::to_value(entry).unwrap(),
                );
            }
            if !matches!(entry.kind.as_str(), "assistant" | "failure")
                || !bus.reply_ids.insert(entry.id.clone())
            {
                continue;
            }
            let Some(conversation) = snapshot.reply_conversation(entry) else {
                continue;
            };
            let mut data = json!({"id":entry.id, "kind":entry.kind, "text":entry.text, "sender":snapshot.entry_sender(entry), "task_id":entry.task_id});
            if let Some(failure) = &entry.failure {
                data["failure"] = json!(failure);
            }
            if let Some(thread) = &entry.thread_id {
                data["thread_id"] = json!(thread);
            }
            Self::publish_locked(&mut bus, conversation, &entry.task_id, "reply", data);
        }
    }

    pub fn bind_thread(&self, task: &str, thread: &str) {
        self.lock().task_threads.insert(task.into(), thread.into());
    }
    pub fn unbind_thread(&self, task: &str) {
        self.lock().task_threads.remove(task);
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, BusState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Sequence number of the newest published event; zero means "no events".
    pub fn sequence(&self) -> u64 {
        self.lock().sequence
    }

    /// Publish one event to replay history and every live subscriber.
    pub fn publish(
        &self,
        conversation_id: &str,
        task_id: &str,
        kind: &str,
        mut data: Value,
    ) -> TurnEvent {
        let mut state = self.lock();
        if let Some(thread) = state.task_threads.get(task_id) {
            if !data.is_object() {
                data = json!({"value":data});
            }
            data["thread_id"] = json!(thread);
        }
        Self::publish_locked(&mut state, conversation_id, task_id, kind, data)
    }

    fn publish_locked(
        state: &mut BusState,
        conversation_id: &str,
        task_id: &str,
        kind: &str,
        data: Value,
    ) -> TurnEvent {
        state.sequence += 1;
        let event = TurnEvent {
            sequence: state.sequence,
            conversation_id: conversation_id.to_string(),
            task_id: task_id.to_string(),
            kind: kind.to_string(),
            data,
        };
        state.recent.push_back(event.clone());
        while state.recent.len() > EVENT_BUFFER {
            state.recent.pop_front();
        }
        state.subscribers.retain(|subscriber| {
            matches!(
                subscriber.try_send(event.clone()),
                Ok(()) | Err(mpsc::TrySendError::Full(_))
            )
        });
        event
    }

    /// Events strictly after `after`, bounded by the replay buffer.
    pub fn since(&self, after: u64) -> Vec<TurnEvent> {
        self.lock()
            .recent
            .iter()
            .filter(|event| event.sequence > after)
            .cloned()
            .collect()
    }

    /// Subscribe to live events. Replay history with [`EventBus::since`] first;
    /// the subscriber queue only carries events published after this call.
    pub fn subscribe(&self) -> mpsc::Receiver<TurnEvent> {
        let (sender, receiver) = mpsc::sync_channel(LIVE_QUEUE);
        self.lock().subscribers.push(sender);
        receiver
    }

    /// Encoded JSON shape used by the server-sent event stream.
    pub fn encode(event: &TurnEvent) -> Value {
        json!({
            "sequence": event.sequence,
            "conversationId": event.conversation_id,
            "taskId": event.task_id,
            "kind": event.kind,
            "data": event.data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_events_require_a_successful_write_and_are_not_replayed_on_load() {
        use crate::{ProductState, StateStore, TranscriptEntry};
        let root = std::env::temp_dir().join(format!("marble-reply-events-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let store = StateStore::in_data_dir(&root);
        let mut state = ProductState::default();
        state.conversation_mut("chat");
        state.push_transcript(TranscriptEntry {
            conversation_id: "chat".into(),
            kind: "assistant".into(),
            text: "old history".into(),
            ..Default::default()
        });
        let bus = EventBus::default();
        bus.attach_replies(&mut state);
        store.save(&state).unwrap();
        assert!(bus.since(0).is_empty());
        for kind in ["user", "tool", "reasoning"] {
            state.push_transcript(TranscriptEntry {
                conversation_id: "chat".into(),
                kind: kind.into(),
                text: "not a reply".into(),
                ..Default::default()
            });
        }
        state.bump();
        store.save(&state).unwrap();
        assert!(bus.since(0).is_empty());
        for kind in ["assistant", "failure"] {
            state.push_transcript(TranscriptEntry {
                conversation_id: "chat".into(),
                task_id: "turn".into(),
                kind: kind.into(),
                text: "a reply".into(),
                ..Default::default()
            });
        }
        state.bump();
        let blocked = root.join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        assert!(StateStore::in_data_dir(&blocked).save(&state).is_err());
        assert!(bus.since(0).is_empty());
        store.save(&state).unwrap();
        let replies = bus.since(0);
        assert_eq!(replies.len(), 2);
        for reply in replies {
            assert_eq!(reply.kind, "reply");
            let disk = store.load().unwrap();
            assert!(disk.transcript.iter().any(|e| reply.data["id"] == e.id));
        }
        store.save(&state).unwrap();
        assert_eq!(bus.since(0).len(), 2);
        let mut stale = state.clone();
        stale.revision -= 1;
        stale.push_transcript(TranscriptEntry {
            kind: "assistant".into(),
            text: "stale unwritten reply".into(),
            ..Default::default()
        });
        store.save(&stale).unwrap();
        assert_eq!(bus.since(0).len(), 2);
        let loaded_bus = EventBus::default();
        let mut loaded = store.load().unwrap();
        loaded_bus.attach_replies(&mut loaded);
        loaded.bump();
        store.save(&loaded).unwrap();
        assert!(loaded_bus.since(0).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn everyone_thread_reply_events_use_thread_detail_visibility() {
        for kind in ["assistant", "failure"] {
            for mirrored in [false, true] {
                let mut state = crate::ProductState::default();
                crate::everyone::mirror(
                    &mut state,
                    "marble",
                    "Marble",
                    "handoff",
                    "asked Ben",
                    "lead",
                    None,
                );
                let parent = state.transcript[0].id.clone();
                let thread = state.create_thread("everyone", &parent).unwrap();
                let bus = EventBus::default();
                bus.attach_replies(&mut state);
                state.bind_thread("delegate", Some(&thread.id));
                state.push_transcript(crate::TranscriptEntry {
                    conversation_id: "private".into(),
                    task_id: "delegate".into(),
                    kind: kind.into(),
                    text: "delegate result".into(),
                    ..Default::default()
                });
                if mirrored {
                    crate::everyone::mirror(
                        &mut state,
                        "ben",
                        "Ben",
                        kind,
                        "delegate result",
                        "delegate",
                        None,
                    );
                }
                let visible = state.thread_entries(&thread);
                assert_eq!(visible.len(), 2);
                bus.publish_replies(&state);
                bus.publish_replies(&state);
                let replies: Vec<_> = bus
                    .since(0)
                    .into_iter()
                    .filter(|e| e.kind == "reply")
                    .collect();
                assert_eq!(replies.len(), 1, "{kind}, mirrored={mirrored}");
                assert_eq!(replies[0].data["id"], visible[1].id);
                assert_eq!(replies[0].data["kind"], kind);
                assert_eq!(replies[0].conversation_id, "everyone");
            }
        }
    }

    #[test]
    fn releasing_buffered_everyone_replies_emits_only_newly_visible_entries() {
        let mut state = crate::ProductState::default();
        let bus = EventBus::default();
        bus.attach_replies(&mut state);
        state.everyone_orders.insert(
            "source".into(),
            ["task:lead".into(), "task:later".into()].into(),
        );
        state
            .everyone_pending
            .insert("task:lead".into(), "marble".into());
        state
            .everyone_pending
            .insert("task:later".into(), "marble".into());
        crate::everyone::mirror_settlement(&mut state, "later", "buffered", "completed");
        bus.publish_replies(&state);
        assert!(bus.since(0).is_empty());
        crate::everyone::mirror_settlement(&mut state, "lead", "", "cancelled");
        bus.publish_replies(&state);
        bus.publish_replies(&state);
        let replies = bus.since(0);
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].conversation_id, "everyone");
        assert_eq!(replies[0].data["task_id"], "later");
        assert_eq!(replies[0].data["id"], state.transcript[1].id);
    }

    #[test]
    fn publish_orders_events_and_replays_after_cursor() {
        let bus = EventBus::default();
        bus.publish("c1", "t1", "text", json!({"text": "a"}));
        bus.publish("c1", "t1", "text", json!({"text": "b"}));
        let replayed = bus.since(1);
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].sequence, 2);
        assert_eq!(replayed[0].data["text"], json!("b"));
    }

    #[test]
    fn live_subscriber_receives_published_events() {
        let bus = EventBus::default();
        let receiver = bus.subscribe();
        bus.publish("c1", "t1", "settled", json!({"reason": "completed"}));
        let event = receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("subscriber should receive the event");
        assert_eq!(event.kind, "settled");
        assert_eq!(event.sequence, 1);
    }

    #[test]
    fn replay_buffer_stays_bounded() {
        let bus = EventBus::default();
        for index in 0..(EVENT_BUFFER + 25) {
            bus.publish("c1", "t1", "text", json!({"index": index}));
        }
        assert_eq!(bus.since(0).len(), EVENT_BUFFER);
        assert_eq!(bus.sequence() as usize, EVENT_BUFFER + 25);
    }
}

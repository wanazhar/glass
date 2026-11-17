//! In-process event fan-out for live product surfaces.
//!
//! One durable turn publishes ordered events (text deltas, reasoning, tool
//! activity, usage, settlement). The web UI and the iOS client consume the
//! same stream, so a reconnecting surface replays from its own cursor instead
//! of re-reading the whole transcript.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
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
        data: Value,
    ) -> TurnEvent {
        let mut state = self.lock();
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

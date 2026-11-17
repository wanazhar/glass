//! Always-on presence for one Marble instance.
//!
//! Marble is meant to be the assistant a person keeps running, not a tab they
//! open. That only works if the assistant can say what it is doing right now,
//! how long it has been up, and what it lost when the machine restarted. This
//! module turns the durable product state into that one small document, so the
//! WebUI, the iOS app, and `/healthz` all answer the same way.

use crate::state::ProductState;
use crate::worker::now_millis;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Durable presence counters for one instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceState {
    /// Unix milliseconds when this process started serving.
    #[serde(default)]
    pub started_at: u64,
    /// Last moment the assistant proved it was alive.
    #[serde(default)]
    pub heartbeat_at: u64,
    /// Last accepted message.
    #[serde(default)]
    pub last_turn_at: u64,
    /// Last settled turn, successful or not.
    #[serde(default)]
    pub last_settled_at: u64,
    #[serde(default)]
    pub turns_started: u64,
    #[serde(default)]
    pub turns_completed: u64,
    /// Tasks that were still running when the previous process stopped.
    #[serde(default)]
    pub recovered_tasks: u64,
    /// Machine name, so several tailnet hosts are distinguishable.
    #[serde(default)]
    pub instance: String,
}

impl PresenceState {
    /// Mark the start of a serving process.
    pub fn mark_start(&mut self) {
        let now = now_millis();
        self.started_at = now;
        self.heartbeat_at = now;
        self.recovered_tasks = 0;
        self.instance = instance_name();
    }

    /// Record that a heartbeat is still alive.
    pub fn beat(&mut self) {
        self.heartbeat_at = now_millis();
    }

    /// Record one accepted message.
    pub fn mark_turn(&mut self) {
        let now = now_millis();
        self.last_turn_at = now;
        self.heartbeat_at = now;
        self.turns_started = self.turns_started.saturating_add(1);
    }

    /// Record one settled turn.
    pub fn mark_settled(&mut self) {
        let now = now_millis();
        self.last_settled_at = now;
        self.heartbeat_at = now;
        self.turns_completed = self.turns_completed.saturating_add(1);
    }

    /// What the assistant is doing, in one word.
    pub fn status(&self, state: &ProductState) -> &'static str {
        if state.tasks.iter().any(|task| task.status == "running") {
            "working"
        } else if state.control.paused {
            "paused"
        } else if !state.approvals.is_empty() {
            "waiting"
        } else {
            "idle"
        }
    }

    /// One sentence a surface can put under the name.
    pub fn summary(&self, state: &ProductState) -> String {
        let active = state
            .tasks
            .iter()
            .rev()
            .find(|task| task.status == "running");
        if let Some(task) = active {
            return format!("working on {}", task.prompt.trim());
        }
        if state.control.paused {
            return format!(
                "paused · {} message{} held",
                state.control.queue.len(),
                plural(state.control.queue.len())
            );
        }
        if let Some(approval) = state.approvals.first() {
            return format!("waiting for you to approve {}", approval.tool);
        }
        let due = state
            .routines
            .routines
            .iter()
            .filter(|routine| routine.enabled)
            .count();
        if due == 0 {
            "ready · no routines scheduled".to_string()
        } else {
            format!("ready · {due} routine{} scheduled", plural(due))
        }
    }

    /// Presence document served by `/api/presence` and embedded in state.
    pub fn document(&self, state: &ProductState) -> Value {
        let now = now_millis();
        let active = state
            .tasks
            .iter()
            .rev()
            .find(|task| task.status == "running");
        json!({
            "status": self.status(state),
            "summary": self.summary(state),
            "instance": if self.instance.is_empty() { instance_name() } else { self.instance.clone() },
            "startedAt": self.started_at,
            "heartbeatAt": self.heartbeat_at,
            "uptimeSeconds": now.saturating_sub(self.started_at) / 1000,
            "lastTurnAt": self.last_turn_at,
            "lastSettledAt": self.last_settled_at,
            "turnsStarted": self.turns_started,
            "turnsCompleted": self.turns_completed,
            "recoveredTasks": self.recovered_tasks,
            "activeTaskId": active.map(|task| task.id.clone()),
            "activeConversationId": active.map(|task| task.conversation_id.clone()),
            "activePrompt": active.map(|task| task.prompt.clone()),
            "approvalsWaiting": state.approvals.len(),
            "queued": state.control.queue.len(),
            "paused": state.control.paused,
            "profile": state.control.profile.as_str(),
            "routines": state.routines.routines.len(),
            "routinesEnabled": state.routines.routines.iter().filter(|routine| routine.enabled).count(),
            "memories": state.memory.facts.len(),
            "notifications": state.notifications.summary(),
        })
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// Host name for the presence document, without adding a dependency for it.
fn instance_name() -> String {
    for key in ["MARBLE_INSTANCE", "HOSTNAME"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    "marble".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TaskRecord;

    fn task(status: &str) -> TaskRecord {
        TaskRecord {
            id: "t1".into(),
            conversation_id: "c1".into(),
            status: status.into(),
            owner: "agent".into(),
            prompt: "tidy the notes".into(),
            at: 1,
            routine_id: String::new(),
        }
    }

    #[test]
    fn presence_reports_what_the_assistant_is_doing() {
        let mut state = ProductState::default();
        let mut presence = PresenceState::default();
        presence.mark_start();
        assert_eq!(presence.status(&state), "idle");

        state.tasks.push(task("running"));
        assert_eq!(presence.status(&state), "working");
        assert!(
            presence.document(&state)["summary"]
                .as_str()
                .unwrap()
                .contains("tidy the notes")
        );

        state.tasks[0].status = "completed".into();
        state.control.paused = true;
        assert_eq!(presence.status(&state), "paused");

        state.control.paused = false;
        state.approvals.push(crate::state::PendingApproval {
            request_id: "r1".into(),
            call_id: "c".into(),
            tool: "glass.workspace.read".into(),
            arguments: Value::Null,
        });
        assert_eq!(presence.status(&state), "waiting");
    }

    #[test]
    fn counters_advance_and_the_document_carries_them() {
        let mut presence = PresenceState::default();
        presence.mark_start();
        presence.mark_turn();
        presence.mark_settled();
        presence.recovered_tasks = 2;
        let state = ProductState::default();
        let document = presence.document(&state);
        assert_eq!(document["turnsStarted"], json!(1));
        assert_eq!(document["turnsCompleted"], json!(1));
        assert_eq!(document["recoveredTasks"], json!(2));
        assert!(
            document["instance"]
                .as_str()
                .is_some_and(|name| !name.is_empty())
        );
        assert!(document["startedAt"].as_u64().unwrap_or_default() > 0);
    }
}

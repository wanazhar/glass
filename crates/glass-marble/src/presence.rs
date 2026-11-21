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
use std::collections::BTreeSet;

/// Whether the engine behind the assistant is answering.
///
/// Marble owns one long-lived `glass-core runtime` subprocess. Supervision is
/// only honest if a surface can see it: a runtime that died mid-turn is
/// restarted before the next turn, and what happened stays in the document
/// rather than only in a log nobody is reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    /// `ok`, `restarting`, or `failed`.
    #[serde(default = "healthy")]
    pub health: String,
    /// Times the runtime was restarted in place after the transport failed.
    #[serde(default)]
    pub restarts: u64,
    #[serde(default)]
    pub last_restart_at: u64,
    /// Last transport failure, kept after a successful restart so a flapping
    /// engine is visible rather than silently forgiven.
    #[serde(default)]
    pub last_error: String,
    #[serde(skip)]
    failed_engines: BTreeSet<u64>,
}

fn healthy() -> String {
    "ok".to_string()
}

impl Default for RuntimeHealth {
    fn default() -> Self {
        Self {
            health: healthy(),
            restarts: 0,
            last_restart_at: 0,
            last_error: String::new(),
            failed_engines: BTreeSet::new(),
        }
    }
}

impl RuntimeHealth {
    pub fn is_ok(&self) -> bool {
        self.health == "ok"
    }

    /// Record a transport failure, before or instead of a restart.
    pub fn note_failure(&mut self, message: impl Into<String>) {
        self.health = "failed".to_string();
        self.last_error = message.into();
    }

    /// Record a successful in-place restart.
    pub fn note_restart(&mut self) {
        self.note_ok();
        self.restarts = self.restarts.saturating_add(1);
        self.last_restart_at = now_millis();
    }

    /// Mark the engine as answering again without counting a restart.
    pub fn note_ok(&mut self) {
        if self.failed_engines.is_empty() {
            self.health = healthy();
        }
    }

    pub(crate) fn note_engine_failure(&mut self, engine: u64, message: impl Into<String>) {
        self.failed_engines.insert(engine);
        self.note_failure(message);
    }

    pub(crate) fn note_engine_restart(&mut self, engine: u64) {
        self.failed_engines.remove(&engine);
        self.note_restart();
    }

    /// Presence document: one small block a surface can trust.
    pub fn document(&self) -> Value {
        json!({
            "health": self.health,
            "restarts": self.restarts,
            "lastRestartAt": self.last_restart_at,
            "lastError": self.last_error,
        })
    }
}

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
    /// Liveness of the engine this process drives.
    #[serde(default)]
    pub runtime: RuntimeHealth,
}

impl PresenceState {
    /// Mark the start of a serving process.
    pub fn mark_start(&mut self) {
        let now = now_millis();
        self.started_at = now;
        self.heartbeat_at = now;
        self.recovered_tasks = 0;
        self.instance = instance_name();
        // A fresh process has a fresh engine; the restart history stays visible.
        self.runtime.failed_engines.clear();
        self.runtime.note_ok();
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
        let waiting = Waiting::of(state);
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
            // Work already handed to the worker but not admitted yet. It is a
            // durable row, so the count survives a restart, and the lanes say
            // whose turn the worker takes next.
            "waitingTurns": waiting.human + waiting.routine,
            "waiting": {
                "human": waiting.human,
                "routine": waiting.routine,
                "nextLane": waiting.next_lane(),
                "note": waiting.note(),
            },
            "paused": state.control.paused,
            "profile": state.control.profile.as_str(),
            "routines": state.routines.routines.len(),
            "routinesEnabled": state.routines.routines.iter().filter(|routine| routine.enabled).count(),
            "memories": state.memory.facts.len(),
            "notifications": state.notifications.summary(),
            "retention": state.retention.document(),
            "prism": state.prism.summary(),
            "runtime": self.runtime.document(),
        })
    }
}

/// What is waiting for the turn worker, split by lane.
///
/// The worker drains typed messages before routine firings, so these two counts
/// explain why a routine that is due has not started yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Waiting {
    pub human: usize,
    pub routine: usize,
}

impl Waiting {
    pub fn of(state: &ProductState) -> Self {
        let mut waiting = Self::default();
        for task in state.tasks.iter().filter(|task| task.status == "queued") {
            if task.routine_id.is_empty() {
                waiting.human += 1;
            } else {
                waiting.routine += 1;
            }
        }
        waiting
    }

    /// Whose turn the worker takes next, or empty when nothing is waiting.
    pub fn next_lane(&self) -> &'static str {
        if self.human > 0 {
            "human"
        } else if self.routine > 0 {
            "routine"
        } else {
            ""
        }
    }

    /// One sentence for a surface, when the wait needs explaining.
    pub fn note(&self) -> &'static str {
        if self.human > 0 && self.routine > 0 {
            "typed messages run before routine firings"
        } else if self.routine > 1 {
            "routine firings run one at a time"
        } else {
            ""
        }
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
            conflict: None,
        });
        assert_eq!(presence.status(&state), "waiting");
    }

    #[test]
    fn a_healthy_lane_cannot_clear_another_engines_failure() {
        let mut runtime = RuntimeHealth::default();
        runtime.note_engine_failure(1, "first engine closed");
        runtime.note_ok();
        assert!(!runtime.is_ok());
        runtime.note_engine_failure(2, "second engine closed");
        runtime.note_engine_restart(1);
        assert!(!runtime.is_ok());
        runtime.note_engine_restart(2);
        assert!(runtime.is_ok());
        assert_eq!(runtime.restarts, 2);
    }

    #[test]
    fn runtime_health_records_failures_and_restarts() {
        let mut presence = PresenceState::default();
        presence.mark_start();
        let state = ProductState::default();
        assert_eq!(presence.document(&state)["runtime"]["health"], json!("ok"));
        presence
            .runtime
            .note_failure("the runtime transport closed");
        let document = presence.document(&state);
        assert_eq!(document["runtime"]["health"], json!("failed"));
        assert_eq!(
            document["runtime"]["lastError"],
            json!("the runtime transport closed")
        );
        presence.runtime.note_restart();
        let document = presence.document(&state);
        assert_eq!(document["runtime"]["health"], json!("ok"));
        assert_eq!(document["runtime"]["restarts"], json!(1));
        assert!(
            document["runtime"]["lastRestartAt"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert_eq!(
            document["runtime"]["lastError"],
            json!("the runtime transport closed"),
            "a restart keeps the reason it was needed"
        );
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

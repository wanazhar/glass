use super::{DevelopmentError, DevelopmentEvent, DevelopmentResult, Timeline};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentRevision {
    pub index: usize,
    pub event_id: String,
    pub occurred_at_ms: u64,
    pub actor_id: String,
    pub event: DevelopmentEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayWindow {
    pub total_revisions: usize,
    pub start: usize,
    pub revisions: Vec<DevelopmentRevision>,
}

pub fn replay(timeline: &Timeline, start: usize, limit: usize) -> DevelopmentResult<ReplayWindow> {
    if limit == 0 || limit > 256 {
        return Err(DevelopmentError::InvalidInput(
            "replay limit must be between 1 and 256".into(),
        ));
    }
    let events = timeline.events().cloned().collect::<Vec<_>>();
    if start > events.len() {
        return Err(DevelopmentError::InvalidInput(
            "replay start is beyond the available timeline".into(),
        ));
    }
    let revisions = events
        .iter()
        .enumerate()
        .skip(start)
        .take(limit)
        .map(|(index, event)| DevelopmentRevision {
            index,
            event_id: event.id.clone(),
            occurred_at_ms: event.occurred_at_ms,
            actor_id: event.actor.id.clone(),
            event: event.clone(),
        })
        .collect();
    Ok(ReplayWindow {
        total_revisions: events.len(),
        start,
        revisions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::development::{Actor, DevelopmentEventKind};

    #[test]
    fn replay_is_revisioned_bounded_and_actor_attributed() {
        let root = std::env::temp_dir().join(format!("glass-replay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut timeline = Timeline::open(root.join("timeline.jsonl")).unwrap();
        timeline
            .record(
                Actor::external("codex"),
                DevelopmentEventKind::VerificationCompleted,
                "/tmp/project",
                serde_json::json!({"status": "passed"}),
            )
            .unwrap();
        let replay = replay(&timeline, 0, 10).unwrap();
        assert_eq!(replay.total_revisions, 1);
        assert_eq!(replay.revisions[0].actor_id, "external:codex");
        let _ = std::fs::remove_dir_all(root);
    }
}

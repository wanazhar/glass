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
    pub oldest_revision: Option<usize>,
    pub newest_revision: Option<usize>,
    pub cursor_expired: bool,
    pub revisions: Vec<DevelopmentRevision>,
}

fn event_index(event: &DevelopmentEvent) -> Option<usize> {
    event
        .id
        .rsplit_once('-')?
        .1
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

pub fn replay(timeline: &Timeline, start: usize, limit: usize) -> DevelopmentResult<ReplayWindow> {
    if limit == 0 || limit > 256 {
        return Err(DevelopmentError::InvalidInput(
            "replay limit must be between 1 and 256".into(),
        ));
    }
    let events = timeline.events().cloned().collect::<Vec<_>>();
    let oldest_revision = events.first().and_then(event_index);
    let newest_revision = events.last().and_then(event_index);
    let total_revisions = newest_revision.map_or(0, |index| index.saturating_add(1));
    if start > total_revisions {
        return Err(DevelopmentError::InvalidInput(
            "replay start is beyond the available timeline".into(),
        ));
    }
    let cursor_expired = oldest_revision.is_some_and(|oldest| start < oldest);
    let actual_start = if cursor_expired {
        oldest_revision.unwrap_or(start)
    } else {
        start
    };
    let revisions = events
        .iter()
        .filter_map(|event| Some((event_index(event)?, event)))
        .filter(|(index, _)| *index >= actual_start)
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
        total_revisions,
        start: actual_start,
        oldest_revision,
        newest_revision,
        cursor_expired,
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

    #[test]
    fn replay_reports_compacted_cursor_gap_like_event_pages() {
        let root = std::env::temp_dir().join(format!("glass-replay-gap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut timeline = Timeline::open(root.join("timeline.jsonl")).unwrap();
        for index in 0..=super::super::MAX_TIMELINE_EVENTS {
            timeline
                .record(
                    Actor::local(),
                    DevelopmentEventKind::VerificationCompleted,
                    "/tmp/project",
                    serde_json::json!({"index": index}),
                )
                .unwrap();
        }
        let window = replay(&timeline, 0, 2).unwrap();
        assert!(window.cursor_expired);
        assert_eq!(window.start, 1);
        assert_eq!(window.oldest_revision, Some(1));
        assert_eq!(window.revisions[0].index, 1);
        assert_eq!(window.revisions.len(), 2);
        let previous_latest = window.newest_revision.unwrap();
        drop(timeline);
        let mut reopened = Timeline::open(root.join("timeline.jsonl")).unwrap();
        let next = reopened
            .record(
                Actor::local(),
                DevelopmentEventKind::VerificationCompleted,
                "/tmp/project",
                serde_json::json!({"afterRestart": true}),
            )
            .unwrap();
        assert_eq!(event_index(&next), Some(previous_latest + 1));
        let _ = std::fs::remove_dir_all(root);
    }
}

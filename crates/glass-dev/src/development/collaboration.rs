use super::{Actor, DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EditAccess {
    Read,
    Write,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EditClaim {
    pub actor: Actor,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub access: EditAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationEvent {
    pub kind: String,
    pub actor: Actor,
    pub payload: Value,
}

#[derive(Debug, Default)]
pub struct CollaborationBus {
    claims: BTreeMap<String, Vec<EditClaim>>,
    subscribers: Vec<SyncSender<CollaborationEvent>>,
}

impl CollaborationBus {
    pub fn subscribe(&mut self) -> Receiver<CollaborationEvent> {
        let (sender, receiver) = mpsc::sync_channel(128);
        self.subscribers.push(sender);
        receiver
    }

    pub fn claim(&mut self, claim: EditClaim) -> DevelopmentResult<()> {
        if claim.path.is_empty()
            || claim.path.len() > 512
            || claim.start_line == 0
            || claim.end_line < claim.start_line
        {
            return Err(DevelopmentError::InvalidInput(
                "edit claim requires a bounded path and ordered one-based lines".into(),
            ));
        }
        let overlaps = |other: &EditClaim| {
            claim.start_line <= other.end_line && other.start_line <= claim.end_line
        };
        if claim.access == EditAccess::Write
            && self.claims.get(&claim.path).is_some_and(|claims| {
                claims.iter().any(|other| {
                    other.access == EditAccess::Write
                        && !(other.actor.id == claim.actor.id
                            && other.start_line == claim.start_line
                            && other.end_line == claim.end_line)
                        && overlaps(other)
                })
            })
        {
            return Err(DevelopmentError::Conflict(format!(
                "{} overlaps an active write claim",
                claim.path
            )));
        }
        let event = CollaborationEvent {
            kind: "editor.claimed".into(),
            actor: claim.actor.clone(),
            payload: serde_json::to_value(&claim)?,
        };
        let claims = self.claims.entry(claim.path.clone()).or_default();
        claims.retain(|other| {
            !(other.actor.id == claim.actor.id
                && other.start_line == claim.start_line
                && other.end_line == claim.end_line)
        });
        claims.push(claim);
        self.publish(event);
        Ok(())
    }

    pub fn release_actor(&mut self, actor_id: &str) {
        let mut released = Vec::new();
        self.claims.retain(|_, claims| {
            claims.retain(|claim| {
                if claim.actor.id == actor_id {
                    released.push(claim.clone());
                    false
                } else {
                    true
                }
            });
            !claims.is_empty()
        });
        for claim in released {
            self.publish(CollaborationEvent {
                kind: "editor.released".into(),
                actor: claim.actor.clone(),
                payload: serde_json::json!({"claim": claim}),
            });
        }
    }

    pub fn claims(&self, path: &str) -> &[EditClaim] {
        self.claims.get(path).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn publish(&mut self, event: CollaborationEvent) {
        self.subscribers
            .retain(|subscriber| match subscriber.try_send(event.clone()) {
                Ok(()) | Err(TrySendError::Full(_)) => true,
                Err(TrySendError::Disconnected(_)) => false,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(actor: Actor, start_line: u32, end_line: u32, access: EditAccess) -> EditClaim {
        EditClaim {
            actor,
            path: "src/app.rs".into(),
            start_line,
            end_line,
            access,
        }
    }

    #[test]
    fn overlapping_writers_fail_while_readers_and_events_remain_bounded() {
        let mut bus = CollaborationBus::default();
        let receiver = bus.subscribe();
        bus.claim(claim(Actor::local(), 10, 20, EditAccess::Write))
            .unwrap();
        assert!(
            bus.claim(claim(Actor::external("codex"), 15, 16, EditAccess::Write))
                .is_err()
        );
        assert_eq!(receiver.try_recv().unwrap().kind, "editor.claimed");
    }

    #[test]
    fn same_actor_overlapping_write_claims_conflict_but_exact_reclaims_replace() {
        let mut bus = CollaborationBus::default();
        let actor = Actor::local();
        bus.claim(claim(actor.clone(), 10, 20, EditAccess::Write))
            .unwrap();
        bus.claim(claim(actor.clone(), 10, 20, EditAccess::Write))
            .expect("reclaiming the same range replaces that claim");
        assert_eq!(bus.claims("src/app.rs").len(), 1);
        assert!(matches!(
            bus.claim(claim(actor, 15, 25, EditAccess::Write)),
            Err(DevelopmentError::Conflict(_))
        ));
        assert_eq!(bus.claims("src/app.rs").len(), 1);
    }

    #[test]
    fn releasing_actor_publishes_each_released_claim() {
        let mut bus = CollaborationBus::default();
        let receiver = bus.subscribe();
        let actor = Actor::external("codex");
        bus.claim(claim(actor.clone(), 10, 20, EditAccess::Write))
            .unwrap();
        bus.claim(claim(actor.clone(), 30, 40, EditAccess::Read))
            .unwrap();
        let _ = receiver.try_iter().collect::<Vec<_>>();

        bus.release_actor(&actor.id);

        assert!(bus.claims("src/app.rs").is_empty());
        let released = receiver.try_iter().collect::<Vec<_>>();
        assert_eq!(released.len(), 2);
        assert!(released.iter().all(|event| event.kind == "editor.released"));
        assert!(released.iter().all(|event| event.actor.id == actor.id));
    }
}

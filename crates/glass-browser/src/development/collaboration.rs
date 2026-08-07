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
        let claims = self.claims.entry(claim.path.clone()).or_default();
        let overlaps = |other: &EditClaim| {
            claim.start_line <= other.end_line && other.start_line <= claim.end_line
        };
        if claim.access == EditAccess::Write
            && claims.iter().any(|other| {
                other.actor.id != claim.actor.id
                    && other.access == EditAccess::Write
                    && overlaps(other)
            })
        {
            return Err(DevelopmentError::Conflict(format!(
                "{} overlaps another actor's write claim",
                claim.path
            )));
        }
        claims.retain(|other| {
            !(other.actor.id == claim.actor.id
                && other.start_line == claim.start_line
                && other.end_line == claim.end_line)
        });
        claims.push(claim.clone());
        self.publish(CollaborationEvent {
            kind: "editor.claimed".into(),
            actor: claim.actor.clone(),
            payload: serde_json::to_value(claim)?,
        });
        Ok(())
    }

    pub fn release_actor(&mut self, actor_id: &str) {
        for claims in self.claims.values_mut() {
            claims.retain(|claim| claim.actor.id != actor_id);
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

    #[test]
    fn overlapping_writers_fail_while_readers_and_events_remain_bounded() {
        let mut bus = CollaborationBus::default();
        let receiver = bus.subscribe();
        bus.claim(EditClaim {
            actor: Actor::local(),
            path: "src/app.rs".into(),
            start_line: 10,
            end_line: 20,
            access: EditAccess::Write,
        })
        .unwrap();
        assert!(
            bus.claim(EditClaim {
                actor: Actor::external("codex"),
                path: "src/app.rs".into(),
                start_line: 15,
                end_line: 16,
                access: EditAccess::Write,
            })
            .is_err()
        );
        assert_eq!(receiver.try_recv().unwrap().kind, "editor.claimed");
    }
}

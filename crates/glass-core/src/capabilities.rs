//! Single capability declaration point for host access.
//!
//! Filesystem, browser, and desktop access requests are declared here before
//! they run. One declaration resolves into one emitted decision; enforcement
//! still happens at the owning boundary.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

/// Host access capability kinds declared through this seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    FilesystemRead,
    FilesystemWrite,
    Browser,
    Desktop,
}

/// Decision emitted for one capability declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum CapabilityDecision {
    Allow,
    Deny { reason: String },
    RequireConfirmation { reason: String },
}

impl CapabilityDecision {
    pub fn allow() -> Self {
        Self::Allow
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny { reason: reason.into() }
    }

    pub fn require(reason: impl Into<String>) -> Self {
        Self::RequireConfirmation { reason: reason.into() }
    }

    pub fn permits(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// One emitted capability decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityDecisionEvent {
    pub capability: Capability,
    pub decision: CapabilityDecision,
}

/// Bounded emitted capability decisions.
const MAX_CAPABILITY_DECISIONS: usize = 256;

fn decision_store() -> &'static Mutex<VecDeque<CapabilityDecisionEvent>> {
    static STORE: OnceLock<Mutex<VecDeque<CapabilityDecisionEvent>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Emit one capability decision without changing enforcement.
pub fn emit_decision(capability: Capability, decision: &CapabilityDecision) {
    let Ok(mut events) = decision_store().lock() else {
        return;
    };
    if events.len() >= MAX_CAPABILITY_DECISIONS {
        events.pop_front();
    }
    events.push_back(CapabilityDecisionEvent {
        capability,
        decision: decision.clone(),
    });
}

/// Bounded capability decisions emitted so far.
pub fn capability_decisions() -> Vec<CapabilityDecisionEvent> {
    decision_store()
        .lock()
        .map(|events| events.iter().cloned().collect())
        .unwrap_or_default()
}

/// Drop emitted capability decisions.
pub fn clear_capability_decisions() {
    decision_store()
        .lock()
        .map(|mut events| events.clear())
        .unwrap_or_default();
}

/// The single capability declaration point for host access.
#[derive(Debug, Clone, Default)]
pub struct CapabilityGate {
    granted: Vec<Capability>,
    confirmed: Vec<Capability>,
}

impl CapabilityGate {
    /// Declare one capability granted for the session after human approval.
    pub fn grant(&mut self, capability: Capability) {
        self.granted.push(capability);
    }

    /// Confirm one granted capability for one operation.
    pub fn confirm(&mut self, capability: Capability) {
        self.confirmed.push(capability);
    }

    /// Resolve one capability declaration into the decision emitted for it.
    pub fn decide(
        &self,
        capability: Capability,
        operation: &str,
    ) -> CapabilityDecision {
        let granted = self.granted.contains(&capability);
        let confirmed = self.confirmed.contains(&capability);
        let decision = if !granted {
            CapabilityDecision::deny(format!(
                "capability {} access was not declared for {}",
                capability_name(capability),
                operation
            ))
        } else if confirmed {
            CapabilityDecision::allow()
        } else {
            CapabilityDecision::require(format!(
                "capability {} access for {} requires human confirmation",
                capability_name(capability),
                operation
            ))
        };
        emit_decision(capability, &decision);
        decision
    }
}

fn capability_name(capability: Capability) -> &'static str {
    match capability {
        Capability::FilesystemRead => "filesystem_read",
        Capability::FilesystemWrite => "filesystem_write",
        Capability::Browser => "browser",
        Capability::Desktop => "desktop",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_declaration_emits_a_decision_for_host_access() {
        clear_capability_decisions();
        let mut gate = CapabilityGate::default();
        assert!(!gate.decide(Capability::FilesystemRead, "read").permits());
        gate.grant(Capability::FilesystemRead);
        assert!(!gate.decide(Capability::FilesystemRead, "read").permits());
        gate.confirm(Capability::FilesystemRead);
        assert!(gate.decide(Capability::FilesystemRead, "read").permits());
        assert_eq!(
            capability_decisions()
                .last()
                .expect("decision was emitted")
                .decision,
            CapabilityDecision::Allow
        );
        clear_capability_decisions();
    }
}

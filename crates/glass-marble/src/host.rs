//! Host approval boundary for runtime tool requests.

use crate::{
    MarbleError,
    tools::{ToolOutcome, ToolRouter},
};
use glass_runtime_client::{RuntimeClient, protocol::ToolRequest};
use serde_json::Value;

/// Explicit host decision for one inbound tool request.
/// An explicit host decision. Deny is the default so an unanswered tool
/// request fails closed instead of falling back to an implicit allow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Approval {
    #[default]
    Deny,
    Approve,
}

/// Selectable host posture for one durable conversation.
///
/// `decision` is the standing answer when the host does not prompt. `prompts`
/// turns each request into an explicit product-level approval so the human can
/// see the exact tool and arguments before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostPolicy {
    mode: Approval,
    prompts: bool,
}

impl Default for HostPolicy {
    fn default() -> Self {
        // Fail closed while still asking: the standing answer is deny, and the
        // surface decides each request while a human is present.
        Self::asking()
    }
}

impl HostPolicy {
    /// Approve every request without prompting. Use only for headless runs the
    /// operator explicitly authorized.
    pub fn approving() -> Self {
        Self {
            mode: Approval::Approve,
            prompts: false,
        }
    }

    /// Refuse every request without prompting.
    pub fn denying() -> Self {
        Self {
            mode: Approval::Deny,
            prompts: false,
        }
    }

    /// Ask the product surface for each request; an unanswered request is denied.
    pub fn asking() -> Self {
        Self {
            mode: Approval::Deny,
            prompts: true,
        }
    }

    pub fn decision(&self) -> Approval {
        self.mode
    }

    /// Whether each tool request waits for an explicit product decision.
    pub fn prompts(&self) -> bool {
        self.prompts
    }

    /// Record an explicit Allow/Deny decision from the host surface.
    pub fn decide(&mut self, approval: Approval) {
        self.mode = approval;
    }
}

/// Answer one inbound tool request. An approved request executes through the
/// workspace router; a denied request is an error result, not permission to run.
pub fn answer_tool_request(
    client: &mut RuntimeClient,
    request_id: Value,
    call: &ToolRequest,
    approval: Approval,
    router: &ToolRouter,
) -> Result<ToolOutcome, MarbleError> {
    answer_tool_request_with_note(client, request_id, call, approval, router, None)
}

/// Answer one inbound tool request, telling Core why a denial happened so the
/// model can adapt instead of retrying blindly.
pub fn answer_tool_request_with_note(
    client: &mut RuntimeClient,
    request_id: Value,
    call: &ToolRequest,
    approval: Approval,
    router: &ToolRouter,
    note: Option<&str>,
) -> Result<ToolOutcome, MarbleError> {
    let outcome = if approval == Approval::Approve {
        router.execute(&call.name, &call.arguments)
    } else {
        ToolOutcome::error(match note.filter(|note| !note.is_empty()) {
            Some(note) => format!("marble declined {}: {note}", call.name),
            None => format!("host declined tool {}", call.name),
        })
    };
    client
        .tool_result(request_id, &outcome.text, outcome.is_error)
        .map_err(MarbleError::Io)?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_policy_defaults_to_fail_closed_deny_while_asking() {
        assert_eq!(HostPolicy::default().decision(), Approval::Deny);
        assert!(HostPolicy::default().prompts());
        assert_eq!(Approval::default(), Approval::Deny);
    }

    #[test]
    fn host_policy_decision_is_selectable() {
        let mut policy = HostPolicy::default();
        assert_eq!(policy.decision(), Approval::Deny);
        policy.decide(Approval::Approve);
        assert_eq!(policy.decision(), Approval::Approve);
        policy.decide(Approval::Deny);
        assert_eq!(policy.decision(), Approval::Deny);

        assert_eq!(HostPolicy::approving().decision(), Approval::Approve);
        assert!(!HostPolicy::approving().prompts());
        assert_eq!(HostPolicy::denying().decision(), Approval::Deny);
        assert!(!HostPolicy::denying().prompts());
        assert!(HostPolicy::asking().prompts());
    }
}

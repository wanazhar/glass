//! Host approval boundary for runtime tool requests.

use crate::MarbleError;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostPolicy {
    mode: Approval,
}

impl Default for HostPolicy {
    fn default() -> Self {
        // Fail closed: no posture may default to allow.
        Self {
            mode: Approval::Deny,
        }
    }
}

impl HostPolicy {
    pub fn approving() -> Self {
        Self {
            mode: Approval::Approve,
        }
    }
    pub fn denying() -> Self {
        Self {
            mode: Approval::Deny,
        }
    }
    pub fn decision(&self) -> Approval {
        self.mode
    }

    /// Record an explicit Allow/Deny decision from the host surface.
    pub fn decide(&mut self, approval: Approval) {
        self.mode = approval;
    }
}

/// Answer one inbound tool request with the host decision. A denied approval is
/// an error result, not permission to execute.
pub fn answer_tool_request(
    client: &mut RuntimeClient,
    request_id: Value,
    call: &ToolRequest,
    approval: Approval,
) -> Result<(), MarbleError> {
    let text = if approval == Approval::Approve {
        format!("host approved tool {}", call.name)
    } else {
        format!("host declined tool {}", call.name)
    };
    client
        .tool_result(request_id, &text, approval != Approval::Approve)
        .map_err(MarbleError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_policy_defaults_to_fail_closed_deny() {
        assert_eq!(HostPolicy::default().decision(), Approval::Deny);
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
        assert_eq!(HostPolicy::denying().decision(), Approval::Deny);
    }
}

//! Small web UI action contract for the glass-marble web surface.
//!
//! The first forward-fit slice of the web UI action contract names the actions
//! a web surface may request, keeps execution state owned by the server, records
//! an explicit Allow/Deny decision, and exposes stable test ids for those
//! actions.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Named web UI actions supported by a Glass surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebUiAction {
    Navigate,
    Click,
    Type,
    Select,
    Check,
    Submit,
    Open,
    Close,
    Confirm,
    Cancel,
    Scroll,
    Extract,
}

impl WebUiAction {
    /// Stable contract name for the named action.
    pub const fn contract_name(self) -> &'static str {
        match self {
            Self::Navigate => "navigate",
            Self::Click => "click",
            Self::Type => "type",
            Self::Select => "select",
            Self::Check => "check",
            Self::Submit => "submit",
            Self::Open => "open",
            Self::Close => "close",
            Self::Confirm => "confirm",
            Self::Cancel => "cancel",
            Self::Scroll => "scroll",
            Self::Extract => "extract",
        }
    }

    /// Stable, deterministic test id for the named action.
    pub fn test_id(self) -> String {
        format!("web-ui-action:{}", self.contract_name())
    }
}

impl fmt::Display for WebUiAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.contract_name())
    }
}

impl FromStr for WebUiAction {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let action = match name {
            "navigate" => Self::Navigate,
            "click" => Self::Click,
            "type" => Self::Type,
            "select" => Self::Select,
            "check" => Self::Check,
            "submit" => Self::Submit,
            "open" => Self::Open,
            "close" => Self::Close,
            "confirm" => Self::Confirm,
            "cancel" => Self::Cancel,
            "scroll" => Self::Scroll,
            "extract" => Self::Extract,
            _ => return Err(format!("unknown web UI action: {name}")),
        };
        Ok(action)
    }
}

/// Explicit Allow/Deny decision for one named web UI action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum WebUiActionDecision {
    Allow,
    Deny { reason: String },
    RequireConfirmation { reason: String },
}

impl WebUiActionDecision {
    /// Whether the named action may proceed under this decision.
    pub fn permits(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Server-owned state for one named web UI action.
///
/// The server owns this state. Clients submit action intents and observe the
/// resulting decision; the state never grants a capability by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebUiActionState {
    pub action: WebUiAction,
    pub status: WebUiActionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<WebUiActionDecision>,
}

/// Lifecycle state the server owns for one named web UI action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WebUiActionStatus {
    #[default]
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "allowed")]
    Allowed,
    #[serde(rename = "denied")]
    Denied,
    #[serde(rename = "requires_confirmation")]
    RequiresConfirmation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_ui_action_contract_names_actions_holds_decisions_and_stable_test_ids() {
        let action = <WebUiAction as FromStr>::from_str("click").expect("click is named");
        assert_eq!(action.contract_name(), "click");
        assert_eq!(action.test_id(), "web-ui-action:click");
        assert_eq!(action.to_string(), "click");

        let allowed = WebUiActionDecision::Allow;
        assert!(allowed.permits());
        let denied = WebUiActionDecision::Deny {
            reason: "click is not permitted".to_string(),
        };
        assert!(!denied.permits());

        let state = WebUiActionState {
            action,
            status: WebUiActionStatus::Pending,
            decision: Some(denied),
        };
        assert_eq!(state.action, WebUiAction::Click);
        assert_eq!(state.status, WebUiActionStatus::Pending);
        assert_eq!(state.decision, Some(denied));
    }
}

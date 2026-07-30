use super::*;
/// Bounded, deterministic workflow checkpoint. Input values and page content
/// are intentionally excluded; only route identity and step state are kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpoint {
    pub schema_version: u8,
    #[serde(default)]
    pub run_id: String,
    pub workflow_name: String,
    pub workflow_version: String,
    pub definition_hash: String,
    pub status: WorkflowRunStatus,
    pub next_step_index: usize,
    pub steps: Vec<WorkflowCheckpointStep>,
    pub page: WorkflowCheckpointPage,
}

/// Redacted state for one checkpointed workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpointStep {
    pub id: String,
    pub state: WorkflowStepState,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<WorkflowStepState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_ids: Vec<String>,
    #[serde(default)]
    pub dispatch_acknowledged: bool,
    #[serde(default)]
    pub effect_observed: bool,
    #[serde(default)]
    pub postcondition_verified: bool,
    #[serde(default)]
    pub retry_safe: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_decision: Option<WorkflowBranchDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_evidence: Option<WorkflowIntentEvidence>,
}

/// Bounded page identity used to reject unsafe resume attempts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpointPage {
    pub target_id: String,
    pub frame_id: String,
    pub url: String,
    pub title: String,
    pub revision: u64,
}

/// Safe next action after a checkpoint has been reconciled with the live page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowResumePlan {
    pub workflow_name: String,
    pub workflow_version: String,
    pub next_step_index: usize,
    pub current_revision: u64,
    pub reconciled: bool,
}

/// Reason a workflow checkpoint cannot be resumed safely.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowResumeError {
    SchemaVersionMismatch {
        expected: u8,
        found: u8,
    },
    DefinitionMismatch,
    RouteChanged,
    InvalidState {
        step_id: String,
        state: WorkflowStepState,
    },
    CheckpointTooLarge,
    CheckpointShape(String),
}

impl fmt::Display for WorkflowResumeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaVersionMismatch { expected, found } => {
                write!(
                    formatter,
                    "workflow checkpoint schema mismatch: expected {expected}, found {found}"
                )
            }
            Self::DefinitionMismatch => {
                formatter.write_str("workflow definition does not match checkpoint")
            }
            Self::RouteChanged => {
                formatter.write_str("workflow checkpoint route or target changed")
            }
            Self::InvalidState { step_id, state } => {
                write!(
                    formatter,
                    "workflow step {step_id:?} cannot be resumed from {state:?}"
                )
            }
            Self::CheckpointTooLarge => {
                formatter.write_str("workflow checkpoint exceeds the 8 KiB limit")
            }
            Self::CheckpointShape(message) => {
                write!(formatter, "invalid workflow checkpoint: {message}")
            }
        }
    }
}

impl std::error::Error for WorkflowResumeError {}

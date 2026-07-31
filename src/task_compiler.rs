//! Browser-free compilation from authored tasks to bounded execution plans.
//!
//! The compiler emits intent-level operations only. A later runtime may resolve
//! those operations against a validated Web IR revision and apply policy gates.

use crate::task_protocol::{
    GlassTask, TASK_PROTOCOL_SCHEMA_VERSION, TaskAmbiguityPolicy, TaskKind, TaskLimits,
    TaskPostcondition, TaskProtocolError, TaskRevisionPolicy, TaskRiskClass, TaskScope,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Version of the deterministic execution-plan contract.
pub const TASK_PLAN_SCHEMA_VERSION: u32 = 1;

/// Intent-level operations emitted by the Task Protocol compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskPlanOperation {
    ObserveScope,
    InspectForm,
    FillInputs,
    ValidateForm,
    SubmitForm,
    FollowNavigation,
    SelectTab,
    ExtractTable,
    ExtractCollection,
    ExtractRegion,
    ReadField,
    InspectDialog,
    ConfirmDialog,
    CancelDialog,
    NextPage,
    CollectPages,
}

/// One stable, typed step in an execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskPlanStep {
    pub ordinal: u16,
    pub operation: TaskPlanOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_names: Vec<String>,
    #[serde(default)]
    pub requires_confirmation: bool,
}

/// Browser-free output of Task Protocol compilation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskExecutionPlan {
    pub schema_version: u32,
    pub task_schema_version: u32,
    pub task: TaskKind,
    pub scope: TaskScope,
    pub limits: TaskLimits,
    pub risk: TaskRiskClass,
    pub ambiguity: TaskAmbiguityPolicy,
    pub revision: TaskRevisionPolicy,
    pub confirmation_required: bool,
    pub steps: Vec<TaskPlanStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postconditions: Vec<TaskPostcondition>,
}

impl TaskExecutionPlan {
    /// Validate plan invariants before handing the plan to a runtime.
    pub fn validate(&self) -> Result<(), TaskCompilationError> {
        if self.schema_version != TASK_PLAN_SCHEMA_VERSION {
            return Err(TaskCompilationError::new(
                "schemaVersion",
                "unsupported execution-plan schema version",
            ));
        }
        if self.task_schema_version != TASK_PROTOCOL_SCHEMA_VERSION {
            return Err(TaskCompilationError::new(
                "taskSchemaVersion",
                "unsupported Task Protocol schema version",
            ));
        }
        if self.steps.is_empty() {
            return Err(TaskCompilationError::new(
                "steps",
                "execution plan must contain at least one step",
            ));
        }
        for (index, step) in self.steps.iter().enumerate() {
            let expected = u16::try_from(index + 1).map_err(|_| {
                TaskCompilationError::new("steps", "execution plan has too many steps")
            })?;
            if step.ordinal != expected {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].ordinal"),
                    "step ordinals must be contiguous and one-based",
                ));
            }
            if step.requires_confirmation && !self.confirmation_required {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].requiresConfirmation"),
                    "a confirmation-gated step requires plan confirmation metadata",
                ));
            }
        }
        Ok(())
    }

    /// Serialize a validated execution plan deterministically.
    pub fn to_canonical_json(&self) -> Result<String, TaskCompilationError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| TaskCompilationError::new("$", error.to_string()))
    }
}

/// Compile an authored task without browser access or side effects.
pub fn compile_task(task: &GlassTask) -> Result<TaskExecutionPlan, TaskCompilationError> {
    task.validate().map_err(TaskCompilationError::from)?;
    let confirmation_required = matches!(
        task.risk,
        TaskRiskClass::RemoteIrreversible
            | TaskRiskClass::Authentication
            | TaskRiskClass::DataDisclosure
    ) || matches!(
        task.ambiguity,
        crate::task_protocol::TaskAmbiguityPolicy::RequireConfirmation
    );
    let mut operations = vec![TaskPlanOperation::ObserveScope];
    operations.push(match task.task {
        TaskKind::FormInspect => TaskPlanOperation::InspectForm,
        TaskKind::FormFill => TaskPlanOperation::FillInputs,
        TaskKind::FormValidate => TaskPlanOperation::ValidateForm,
        TaskKind::FormSubmit => TaskPlanOperation::SubmitForm,
        TaskKind::NavigationFollow => TaskPlanOperation::FollowNavigation,
        TaskKind::NavigationSelectTab => TaskPlanOperation::SelectTab,
        TaskKind::TableExtract => TaskPlanOperation::ExtractTable,
        TaskKind::CollectionExtract => TaskPlanOperation::ExtractCollection,
        TaskKind::RegionExtract => TaskPlanOperation::ExtractRegion,
        TaskKind::FieldRead => TaskPlanOperation::ReadField,
        TaskKind::DialogInspect => TaskPlanOperation::InspectDialog,
        TaskKind::DialogConfirm => TaskPlanOperation::ConfirmDialog,
        TaskKind::DialogCancel => TaskPlanOperation::CancelDialog,
        TaskKind::PaginationNext => TaskPlanOperation::NextPage,
        TaskKind::PaginationCollect => TaskPlanOperation::CollectPages,
    });
    let input_names = task.inputs.keys().cloned().collect::<Vec<_>>();
    let steps = operations
        .into_iter()
        .enumerate()
        .map(|(index, operation)| TaskPlanStep {
            ordinal: u16::try_from(index + 1).expect("fixed operation sequence is bounded"),
            operation,
            input_names: if operation == TaskPlanOperation::FillInputs {
                input_names.clone()
            } else {
                Vec::new()
            },
            requires_confirmation: confirmation_required
                && operation != TaskPlanOperation::ObserveScope,
        })
        .collect();
    let plan = TaskExecutionPlan {
        schema_version: TASK_PLAN_SCHEMA_VERSION,
        task_schema_version: task.schema_version,
        task: task.task,
        scope: task.scope.clone(),
        limits: task.limits,
        risk: task.risk,
        ambiguity: task.ambiguity,
        revision: task.revision,
        confirmation_required,
        steps,
        postconditions: task.postconditions.clone(),
    };
    plan.validate()?;
    Ok(plan)
}

/// Path-aware compiler or plan validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCompilationError {
    pub path: String,
    pub reason: String,
}

impl TaskCompilationError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl From<TaskProtocolError> for TaskCompilationError {
    fn from(error: TaskProtocolError) -> Self {
        Self {
            path: error.path,
            reason: error.reason,
        }
    }
}

impl Display for TaskCompilationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl Error for TaskCompilationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_protocol::{TaskAmbiguityPolicy, TaskLimits, TaskPostconditionKind, TaskScope};
    use std::collections::BTreeMap;

    fn task(kind: TaskKind, risk: TaskRiskClass) -> GlassTask {
        GlassTask {
            schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
            task: kind,
            scope: TaskScope {
                region_name: Some("Checkout".into()),
                ..TaskScope::default()
            },
            inputs: BTreeMap::from([(String::from("email"), String::from("a@example.test"))]),
            limits: TaskLimits::default(),
            risk,
            ambiguity: TaskAmbiguityPolicy::Fail,
            revision: Default::default(),
            postconditions: vec![TaskPostcondition {
                kind: TaskPostconditionKind::ValidationClear,
                expected: None,
            }],
        }
    }

    #[test]
    fn compiler_emits_stable_semantic_operations_without_values() {
        let plan = compile_task(&task(TaskKind::FormFill, TaskRiskClass::LocalMutation)).unwrap();
        assert_eq!(
            plan.steps
                .iter()
                .map(|step| step.operation)
                .collect::<Vec<_>>(),
            vec![
                TaskPlanOperation::ObserveScope,
                TaskPlanOperation::FillInputs
            ]
        );
        assert_eq!(plan.steps[1].input_names, vec!["email"]);
        assert_eq!(plan.scope.region_name.as_deref(), Some("Checkout"));
        assert_eq!(plan.limits, TaskLimits::default());
        assert_eq!(plan.ambiguity, TaskAmbiguityPolicy::Fail);
        assert_eq!(plan.revision, TaskRevisionPolicy::Exact);
        assert!(!plan.to_canonical_json().unwrap().contains("a@example.test"));
        let first = plan.to_canonical_json().unwrap();
        let second = compile_task(&task(TaskKind::FormFill, TaskRiskClass::LocalMutation))
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn compiler_preserves_non_default_execution_guards() {
        let mut authored = task(TaskKind::NavigationFollow, TaskRiskClass::RemoteReversible);
        authored.scope.region_name = Some("Shipping".into());
        authored.limits = TaskLimits {
            max_actions: 7,
            timeout_ms: 2_500,
            max_items: 9,
        };
        authored.ambiguity = TaskAmbiguityPolicy::RequireConfirmation;
        authored.revision = TaskRevisionPolicy::Compatible;

        let plan = compile_task(&authored).unwrap();

        assert_eq!(plan.scope.region_name.as_deref(), Some("Shipping"));
        assert_eq!(plan.limits, authored.limits);
        assert_eq!(plan.ambiguity, authored.ambiguity);
        assert_eq!(plan.revision, authored.revision);
    }

    #[test]
    fn irreversible_and_explicit_confirmation_tasks_are_gated() {
        let irreversible = compile_task(&task(
            TaskKind::FormSubmit,
            TaskRiskClass::RemoteIrreversible,
        ))
        .unwrap();
        assert!(irreversible.confirmation_required);
        assert!(irreversible.steps[1].requires_confirmation);
        let mut explicit = task(TaskKind::NavigationFollow, TaskRiskClass::ReadOnly);
        explicit.ambiguity = TaskAmbiguityPolicy::RequireConfirmation;
        assert!(compile_task(&explicit).unwrap().confirmation_required);
    }

    #[test]
    fn invalid_authored_tasks_fail_before_plan_emission() {
        let mut invalid = task(TaskKind::FormFill, TaskRiskClass::LocalMutation);
        invalid.inputs.clear();
        let error = compile_task(&invalid).unwrap_err();
        assert_eq!(error.path, "inputs");
    }

    #[test]
    fn plan_validation_rejects_noncontiguous_ordinals() {
        let mut plan =
            compile_task(&task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly)).unwrap();
        plan.steps[1].ordinal = 3;
        assert_eq!(plan.validate().unwrap_err().path, "steps[1].ordinal");
    }
}

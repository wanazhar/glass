//! Browser-free compilation from authored tasks to bounded execution plans.
//!
//! The compiler emits intent-level operations only. A later runtime may resolve
//! those operations against a validated Web IR revision and apply policy gates.

use crate::task_protocol::{
    GlassTask, MAX_INPUT_NAME_BYTES, MAX_INPUTS, MAX_POSTCONDITIONS, TASK_PROTOCOL_SCHEMA_VERSION,
    TaskAmbiguityPolicy, TaskKind, TaskLimits, TaskPostcondition, TaskProtocolError,
    TaskRevisionPolicy, TaskRiskClass, TaskScope, postcondition_allowed_for,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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
    OpenMenu,
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
        self.scope.validate().map_err(TaskCompilationError::from)?;
        self.scope
            .validate_for_task(self.task)
            .map_err(TaskCompilationError::from)?;
        self.limits.validate().map_err(TaskCompilationError::from)?;
        if self.postconditions.len() > MAX_POSTCONDITIONS {
            return Err(TaskCompilationError::new(
                "postconditions",
                "postcondition count exceeds the Task Protocol bound",
            ));
        }
        for (index, postcondition) in self.postconditions.iter().enumerate() {
            if !postcondition_allowed_for(self.task, postcondition.kind) {
                return Err(TaskCompilationError::new(
                    format!("postconditions[{index}].kind"),
                    "postcondition kind is incompatible with the task family",
                ));
            }
            postcondition
                .validate_at(index)
                .map_err(TaskCompilationError::from)?;
        }
        if self.task == TaskKind::FormSubmit && self.postconditions.is_empty() {
            return Err(TaskCompilationError::new(
                "postconditions",
                "form.submit requires at least one bounded postcondition",
            ));
        }
        if self.steps.is_empty() {
            return Err(TaskCompilationError::new(
                "steps",
                "execution plan must contain at least one step",
            ));
        }
        if self.steps.len() > self.limits.max_actions as usize {
            return Err(TaskCompilationError::new(
                "steps",
                "execution plan exceeds the maxActions bound",
            ));
        }
        let expected_operation = operation_for_task(self.task);
        let expected_operations = [TaskPlanOperation::ObserveScope, expected_operation];
        if self.steps.len() != expected_operations.len()
            || self
                .steps
                .iter()
                .map(|step| step.operation)
                .ne(expected_operations)
        {
            return Err(TaskCompilationError::new(
                "steps",
                "execution plan operation sequence does not exactly match the task family",
            ));
        }
        let minimum_risk = minimum_risk_for_task(self.task);
        if !risk_at_least(self.risk, minimum_risk) {
            return Err(TaskCompilationError::new(
                "risk",
                "declared risk is below the minimum risk required by the task operation",
            ));
        }
        if confirmation_required_for(self.risk, self.ambiguity) && !self.confirmation_required {
            return Err(TaskCompilationError::new(
                "confirmationRequired",
                "plan metadata must require confirmation for this task",
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
            if step.operation == TaskPlanOperation::ObserveScope && step.requires_confirmation {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].requiresConfirmation"),
                    "scope observation cannot require confirmation",
                ));
            }
            if step.requires_confirmation && !self.confirmation_required {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].requiresConfirmation"),
                    "a confirmation-gated step requires plan confirmation metadata",
                ));
            }
            if self.confirmation_required
                && step.operation != TaskPlanOperation::ObserveScope
                && !step.requires_confirmation
            {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].requiresConfirmation"),
                    "task operations must be confirmation-gated when the plan requires it",
                ));
            }
            if matches!(
                step.operation,
                TaskPlanOperation::FillInputs | TaskPlanOperation::SubmitForm
            ) && step.input_names.is_empty()
            {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].inputNames"),
                    "form operations require at least one input name",
                ));
            }
            if !matches!(
                step.operation,
                TaskPlanOperation::FillInputs | TaskPlanOperation::SubmitForm
            ) && !step.input_names.is_empty()
            {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].inputNames"),
                    "input names are only valid for form operations",
                ));
            }
            if step.operation == TaskPlanOperation::SubmitForm
                && step.input_names != ["submit".to_string()]
            {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].inputNames"),
                    "submitForm must carry exactly the submit input name",
                ));
            }
            let mut input_names = BTreeSet::new();
            if step.input_names.len() > MAX_INPUTS {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].inputNames"),
                    "input name count exceeds the Task Protocol bound",
                ));
            }
            for input_name in &step.input_names {
                if input_name.is_empty()
                    || input_name.len() > MAX_INPUT_NAME_BYTES
                    || input_name.chars().any(char::is_control)
                    || !input_names.insert(input_name)
                {
                    return Err(TaskCompilationError::new(
                        format!("steps[{index}].inputNames"),
                        "input names must be unique, bounded, and free of control characters",
                    ));
                }
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
    let effective_risk = max_risk(task.risk, minimum_risk_for_task(task.task));
    let confirmation_required = confirmation_required_for(effective_risk, task.ambiguity);
    let operations = vec![
        TaskPlanOperation::ObserveScope,
        operation_for_task(task.task),
    ];
    let input_names = if task.task == TaskKind::FormSubmit {
        vec!["submit".to_string()]
    } else {
        task.inputs.keys().cloned().collect::<Vec<_>>()
    };
    let steps = operations
        .into_iter()
        .enumerate()
        .map(|(index, operation)| TaskPlanStep {
            ordinal: u16::try_from(index + 1).expect("fixed operation sequence is bounded"),
            operation,
            input_names: if matches!(
                operation,
                TaskPlanOperation::FillInputs | TaskPlanOperation::SubmitForm
            ) {
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
        risk: effective_risk,
        ambiguity: task.ambiguity,
        revision: task.revision,
        confirmation_required,
        steps,
        postconditions: task.postconditions.clone(),
    };
    plan.validate()?;
    Ok(plan)
}

fn operation_for_task(task: TaskKind) -> TaskPlanOperation {
    match task {
        TaskKind::FormInspect => TaskPlanOperation::InspectForm,
        TaskKind::FormFill => TaskPlanOperation::FillInputs,
        TaskKind::FormValidate => TaskPlanOperation::ValidateForm,
        TaskKind::FormSubmit => TaskPlanOperation::SubmitForm,
        TaskKind::NavigationFollow => TaskPlanOperation::FollowNavigation,
        TaskKind::NavigationSelectTab => TaskPlanOperation::SelectTab,
        TaskKind::NavigationOpenMenu => TaskPlanOperation::OpenMenu,
        TaskKind::TableExtract => TaskPlanOperation::ExtractTable,
        TaskKind::CollectionExtract => TaskPlanOperation::ExtractCollection,
        TaskKind::RegionExtract => TaskPlanOperation::ExtractRegion,
        TaskKind::FieldRead => TaskPlanOperation::ReadField,
        TaskKind::DialogInspect => TaskPlanOperation::InspectDialog,
        TaskKind::DialogConfirm => TaskPlanOperation::ConfirmDialog,
        TaskKind::DialogCancel => TaskPlanOperation::CancelDialog,
        TaskKind::PaginationNext => TaskPlanOperation::NextPage,
        TaskKind::PaginationCollect => TaskPlanOperation::CollectPages,
    }
}
fn minimum_risk_for_task(task: TaskKind) -> TaskRiskClass {
    match task {
        TaskKind::FormFill => TaskRiskClass::LocalMutation,
        TaskKind::FormSubmit => TaskRiskClass::RemoteIrreversible,
        TaskKind::DialogConfirm | TaskKind::DialogCancel => TaskRiskClass::RemoteReversible,
        TaskKind::NavigationFollow
        | TaskKind::NavigationSelectTab
        | TaskKind::NavigationOpenMenu
        | TaskKind::PaginationNext
        | TaskKind::PaginationCollect => TaskRiskClass::LocalMutation,
        TaskKind::FormInspect
        | TaskKind::FormValidate
        | TaskKind::TableExtract
        | TaskKind::CollectionExtract
        | TaskKind::RegionExtract
        | TaskKind::FieldRead
        | TaskKind::DialogInspect => TaskRiskClass::ReadOnly,
    }
}

fn risk_rank(risk: TaskRiskClass) -> u8 {
    match risk {
        TaskRiskClass::ReadOnly => 0,
        TaskRiskClass::LocalMutation => 1,
        TaskRiskClass::RemoteReversible => 2,
        TaskRiskClass::RemoteIrreversible => 3,
        TaskRiskClass::Authentication => 4,
        TaskRiskClass::DataDisclosure => 5,
        TaskRiskClass::UnknownRisk => 6,
    }
}

fn risk_at_least(actual: TaskRiskClass, minimum: TaskRiskClass) -> bool {
    actual == TaskRiskClass::UnknownRisk || risk_rank(actual) >= risk_rank(minimum)
}

fn max_risk(left: TaskRiskClass, right: TaskRiskClass) -> TaskRiskClass {
    if risk_rank(left) >= risk_rank(right) {
        left
    } else {
        right
    }
}

fn confirmation_required_for(risk: TaskRiskClass, ambiguity: TaskAmbiguityPolicy) -> bool {
    matches!(
        risk,
        TaskRiskClass::RemoteIrreversible
            | TaskRiskClass::Authentication
            | TaskRiskClass::DataDisclosure
            | TaskRiskClass::UnknownRisk
    ) || matches!(ambiguity, TaskAmbiguityPolicy::RequireConfirmation)
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
        let input_name = if kind == TaskKind::FormSubmit {
            "submit"
        } else {
            "email"
        };
        let postcondition_kind = match kind {
            TaskKind::FormSubmit | TaskKind::NavigationFollow => {
                TaskPostconditionKind::NavigationOccurred
            }
            TaskKind::TableExtract
            | TaskKind::CollectionExtract
            | TaskKind::RegionExtract
            | TaskKind::PaginationNext
            | TaskKind::PaginationCollect => TaskPostconditionKind::RecordsExtracted,
            TaskKind::NavigationSelectTab | TaskKind::NavigationOpenMenu | TaskKind::FieldRead => {
                TaskPostconditionKind::PageKind
            }
            TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => {
                TaskPostconditionKind::DialogClosed
            }
            _ => TaskPostconditionKind::ValidationClear,
        };
        let expected = match postcondition_kind {
            TaskPostconditionKind::PageKind => Some("form".into()),
            TaskPostconditionKind::RecordsExtracted => Some("0".into()),
            _ => None,
        };
        GlassTask {
            schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
            task: kind,
            scope: TaskScope {
                region_name: Some("Checkout".into()),
                ..TaskScope::default()
            },
            inputs: BTreeMap::from([(String::from(input_name), String::from("a@example.test"))]),
            limits: TaskLimits::default(),
            risk,
            ambiguity: TaskAmbiguityPolicy::Fail,
            revision: Default::default(),
            postconditions: vec![TaskPostcondition {
                kind: postcondition_kind,
                expected,
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
        assert!(!plan.to_canonical_json().unwrap().contains("a@example.test"));
    }

    #[test]
    fn submit_requires_submit_input_and_confirmation() {
        let plan = compile_task(&task(TaskKind::FormSubmit, TaskRiskClass::ReadOnly)).unwrap();
        assert_eq!(plan.steps[1].input_names, vec!["submit"]);
        assert_eq!(plan.risk, TaskRiskClass::RemoteIrreversible);
        assert!(plan.confirmation_required);
    }

    #[test]
    fn plan_rejects_extra_operations_and_preserves_revision_policy() {
        let mut plan =
            compile_task(&task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly)).unwrap();
        plan.steps.push(plan.steps[1].clone());
        assert_eq!(plan.validate().unwrap_err().path, "steps");
        let mut authored = task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly);
        authored.revision = TaskRevisionPolicy::Compatible;
        let plan = compile_task(&authored).unwrap();
        assert_eq!(plan.revision, TaskRevisionPolicy::Compatible);
    }

    #[test]
    fn plan_validation_rejects_downgraded_risk() {
        let mut plan = compile_task(&task(
            TaskKind::FormSubmit,
            TaskRiskClass::RemoteIrreversible,
        ))
        .unwrap();
        plan.risk = TaskRiskClass::ReadOnly;
        assert_eq!(plan.validate().unwrap_err().path, "risk");
    }
}

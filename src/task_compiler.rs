//! Browser-free compilation from authored tasks to bounded execution plans.
//!
//! The compiler emits intent-level operations only. A later runtime may resolve
//! those operations against a validated Web IR revision and apply policy gates.

use crate::task_protocol::{
    GlassTask, MAX_INPUT_NAME_BYTES, MAX_INPUTS, MAX_POSTCONDITIONS, TASK_PROTOCOL_SCHEMA_VERSION,
    TaskAmbiguityPolicy, TaskKind, TaskLimits, TaskPostcondition, TaskProtocolError,
    TaskRevisionPolicy, TaskRiskClass, TaskScope,
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
        self.limits.validate().map_err(TaskCompilationError::from)?;
        if self.postconditions.len() > MAX_POSTCONDITIONS {
            return Err(TaskCompilationError::new(
                "postconditions",
                "postcondition count exceeds the Task Protocol bound",
            ));
        }
        for (index, postcondition) in self.postconditions.iter().enumerate() {
            postcondition
                .validate_at(index)
                .map_err(TaskCompilationError::from)?;
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
        if self.steps[0].operation != TaskPlanOperation::ObserveScope {
            return Err(TaskCompilationError::new(
                "steps[0].operation",
                "execution plan must begin with scope observation",
            ));
        }
        if self
            .steps
            .iter()
            .filter(|step| step.operation == TaskPlanOperation::ObserveScope)
            .count()
            != 1
        {
            return Err(TaskCompilationError::new(
                "steps",
                "execution plan must contain exactly one scope observation",
            ));
        }
        let expected_operation = operation_for_task(self.task);
        if self
            .steps
            .iter()
            .filter(|step| step.operation == expected_operation)
            .count()
            != 1
        {
            return Err(TaskCompilationError::new(
                "steps",
                "execution plan must contain exactly one task operation",
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
    let confirmation_required = confirmation_required_for(task.risk, task.ambiguity);
    let operations = vec![
        TaskPlanOperation::ObserveScope,
        operation_for_task(task.task),
    ];
    let input_names = task.inputs.keys().cloned().collect::<Vec<_>>();
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

fn confirmation_required_for(risk: TaskRiskClass, ambiguity: TaskAmbiguityPolicy) -> bool {
    matches!(
        risk,
        TaskRiskClass::RemoteIrreversible
            | TaskRiskClass::Authentication
            | TaskRiskClass::DataDisclosure
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
        authored
            .inputs
            .insert("url".into(), "https://example.test/shipping".into());
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
    fn submit_plans_preserve_only_input_names() {
        let plan = compile_task(&task(
            TaskKind::FormSubmit,
            TaskRiskClass::RemoteIrreversible,
        ))
        .unwrap();
        assert_eq!(plan.steps[1].operation, TaskPlanOperation::SubmitForm);
        assert_eq!(plan.steps[1].input_names, vec!["email"]);
        assert!(!plan.to_canonical_json().unwrap().contains("a@example.test"));
        assert!(plan.confirmation_required);
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
        explicit
            .inputs
            .insert("url".into(), "https://example.test/next".into());
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
    #[test]
    fn plan_validation_rejects_steps_over_action_budget() {
        let mut plan =
            compile_task(&task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly)).unwrap();
        plan.limits.max_actions = 1;
        assert_eq!(plan.validate().unwrap_err().path, "steps");
    }

    #[test]
    fn plan_validation_rejects_task_operation_mismatch() {
        let mut plan =
            compile_task(&task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly)).unwrap();
        plan.steps[1].operation = TaskPlanOperation::InspectForm;
        assert_eq!(plan.validate().unwrap_err().path, "steps");
    }

    #[test]
    fn plan_validation_rejects_empty_fill_inputs() {
        let mut plan =
            compile_task(&task(TaskKind::FormFill, TaskRiskClass::LocalMutation)).unwrap();
        plan.steps[1].input_names.clear();
        assert_eq!(plan.validate().unwrap_err().path, "steps[1].inputNames");
    }

    #[test]
    fn plan_validation_requires_confirmation_for_risky_tasks() {
        let mut plan = compile_task(&task(
            TaskKind::FormSubmit,
            TaskRiskClass::RemoteIrreversible,
        ))
        .unwrap();
        plan.confirmation_required = false;
        plan.steps[1].requires_confirmation = false;
        assert_eq!(plan.validate().unwrap_err().path, "confirmationRequired");
    }
    #[test]
    fn plan_validation_rejects_unbounded_postconditions() {
        let mut plan =
            compile_task(&task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly)).unwrap();
        plan.postconditions[0].expected = Some("\u{0007}".into());
        assert_eq!(
            plan.validate().unwrap_err().path,
            "postconditions[0].expected"
        );

        plan.postconditions = vec![plan.postconditions[0].clone(); MAX_POSTCONDITIONS + 1];
        assert_eq!(plan.validate().unwrap_err().path, "postconditions");
    }

    #[test]
    fn plan_validation_rejects_too_many_fill_inputs() {
        let mut plan =
            compile_task(&task(TaskKind::FormFill, TaskRiskClass::LocalMutation)).unwrap();
        plan.steps[1].input_names = (0..=MAX_INPUTS)
            .map(|index| format!("field-{index}"))
            .collect();
        assert_eq!(plan.validate().unwrap_err().path, "steps[1].inputNames");
    }
}

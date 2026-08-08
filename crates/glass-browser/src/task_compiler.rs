//! Deterministic compilation from authored tasks and stable Glass Web IR v1.
//!
//! Compilation is browser-free: it binds intent to bounded semantic entities,
//! emits explicit evidence and revision guards, and never carries input values.

use crate::extraction::{EvidenceQuality, EvidenceSource};
use crate::task_protocol::{
    GlassTask, MAX_INPUT_NAME_BYTES, MAX_INPUTS, MAX_POSTCONDITIONS, TASK_PROTOCOL_SCHEMA_VERSION,
    TaskAmbiguityPolicy, TaskKind, TaskLimits, TaskPostcondition, TaskPostconditionKind,
    TaskProtocolError, TaskRevisionPolicy, TaskRiskClass, TaskScope, postcondition_allowed_for,
};
use crate::web_ir::{
    GlassWebIrV1, WEB_IR_SCHEMA_VERSION, WebIrAction, WebIrEntity, WebIrEntityDetails,
    WebIrEntityKind, WebIrRelationshipKind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::browser::session::{
    KnowledgeLookupContext, KnowledgeMemoryInfluence, KnowledgeRetrievalQuery,
    KnowledgeRetrievalReport, KnowledgeStore,
};
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

/// Version of the compiler logic recorded in every execution plan.
/// Version 2 adds entity-scoped evidence, revision-bound semantic binding
/// keys, and explicit runtime capability requirements. Version-1 plans must
/// be recompiled from their authored task and current Web IR before execution.
pub const TASK_COMPILER_VERSION: u32 = 2;

/// Evidence floor required before a compiled operation may execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskEvidenceRequirements {
    pub minimum_quality: EvidenceQuality,
    pub required_sources: Vec<EvidenceSource>,
    pub require_complete: bool,
}

/// Evidence required from one selected entity. This prevents unrelated page
/// evidence from satisfying an executable operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskEntityEvidenceRequirement {
    pub entity_id: String,
    pub minimum_quality: EvidenceQuality,
    pub required_sources: Vec<EvidenceSource>,
}

/// Stable semantic key used by a live runtime to create an ephemeral browser
/// binding from the exact compiled revision. It intentionally contains no
/// browser or DOM handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskEntityBindingKey {
    pub entity_id: String,
    pub kind: WebIrEntityKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Runtime features required to interpret a compiled plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskRuntimeCapability {
    Observe,
    Read,
    Mutate,
    Navigate,
    Extract,
    Dialog,
    Pagination,
    VerifyEntityState,
}

/// Memory output is deliberately separated from executable plan fields. It
/// explains advisory ranking and never supplies a target or postcondition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskMemoryAdvisory {
    pub retrieval: KnowledgeRetrievalReport,
    pub influence: KnowledgeMemoryInfluence,
    /// Explicit proof that memory only ranked candidates and did not alter
    /// the executable plan derived from the current Web IR.
    #[serde(default = "default_executable_plan_unchanged")]
    pub executable_plan_unchanged: bool,
}

fn default_executable_plan_unchanged() -> bool {
    false
}

#[derive(Debug, Clone, Default)]
pub struct TaskCompilationOptions<'a> {
    pub knowledge_store: Option<&'a KnowledgeStore>,
    pub knowledge_context: Option<&'a KnowledgeLookupContext>,
}

/// One inspectable runtime guard emitted by compilation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TaskPlanPrecondition {
    RevisionEquals {
        revision: u64,
    },
    EntityPresent {
        entity_id: String,
    },
    EntityEnabled {
        entity_id: String,
    },
    ActionSupported {
        entity_id: String,
        action: WebIrAction,
    },
}

/// One stable, typed step in an execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskPlanStep {
    pub ordinal: u16,
    pub operation: TaskPlanOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_ids: Vec<String>,
    #[serde(default)]
    pub requires_confirmation: bool,
}

/// Browser-free output of Task Protocol compilation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskExecutionPlan {
    pub schema_version: u32,
    pub task_schema_version: u32,
    pub compiler_version: u32,
    pub source_ir_schema_version: u32,
    pub source_ir_revision: u64,
    pub task_fingerprint: String,
    pub task: TaskKind,
    pub scope: TaskScope,
    pub limits: TaskLimits,
    pub risk: TaskRiskClass,
    pub ambiguity: TaskAmbiguityPolicy,
    pub revision: TaskRevisionPolicy,
    pub confirmation_required: bool,
    pub selected_entity_ids: Vec<String>,
    pub evidence_requirements: TaskEvidenceRequirements,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_evidence_requirements: Vec<TaskEntityEvidenceRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_binding_keys: Vec<TaskEntityBindingKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_runtime_capabilities: Vec<TaskRuntimeCapability>,
    pub preconditions: Vec<TaskPlanPrecondition>,
    pub steps: Vec<TaskPlanStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_advisory: Option<TaskMemoryAdvisory>,
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
        if self.compiler_version != TASK_COMPILER_VERSION {
            return Err(TaskCompilationError::new(
                "compilerVersion",
                "unsupported task compiler version",
            ));
        }
        if self.source_ir_schema_version != WEB_IR_SCHEMA_VERSION {
            return Err(TaskCompilationError::new(
                "sourceIrSchemaVersion",
                "unsupported source Glass Web IR schema version",
            ));
        }
        if self.task_fingerprint.len() != 64
            || !self
                .task_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(TaskCompilationError::new(
                "taskFingerprint",
                "task fingerprint must be a lowercase SHA-256 digest",
            ));
        }
        let selected = self
            .selected_entity_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if selected.is_empty() || selected.len() != self.selected_entity_ids.len() {
            return Err(TaskCompilationError::new(
                "selectedEntityIds",
                "selected entity IDs must be non-empty and unique",
            ));
        }
        if self.evidence_requirements.required_sources.is_empty()
            || self
                .evidence_requirements
                .required_sources
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(TaskCompilationError::new(
                "evidenceRequirements.requiredSources",
                "required evidence sources must be non-empty, sorted, and unique",
            ));
        }
        if self.entity_evidence_requirements.len() != self.selected_entity_ids.len()
            || self
                .entity_evidence_requirements
                .iter()
                .map(|requirement| requirement.entity_id.as_str())
                .collect::<BTreeSet<_>>()
                != selected
        {
            return Err(TaskCompilationError::new(
                "entityEvidenceRequirements",
                "every selected entity must have exactly one evidence requirement",
            ));
        }
        if self.entity_binding_keys.len() != self.selected_entity_ids.len()
            || self
                .entity_binding_keys
                .iter()
                .map(|binding| binding.entity_id.as_str())
                .collect::<BTreeSet<_>>()
                != selected
        {
            return Err(TaskCompilationError::new(
                "entityBindingKeys",
                "every selected entity must have exactly one semantic binding key",
            ));
        }
        for (index, requirement) in self.entity_evidence_requirements.iter().enumerate() {
            if requirement.required_sources.is_empty()
                || requirement
                    .required_sources
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
            {
                return Err(TaskCompilationError::new(
                    format!("entityEvidenceRequirements[{index}].requiredSources"),
                    "entity evidence sources must be non-empty, sorted, and unique",
                ));
            }
        }
        if self.required_runtime_capabilities.is_empty()
            || self
                .required_runtime_capabilities
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(TaskCompilationError::new(
                "requiredRuntimeCapabilities",
                "runtime capabilities must be non-empty, sorted, and unique",
            ));
        }
        if !self.preconditions.iter().any(|precondition| {
            matches!(
                precondition,
                TaskPlanPrecondition::RevisionEquals { revision }
                    if *revision == self.source_ir_revision
            )
        }) {
            return Err(TaskCompilationError::new(
                "preconditions",
                "plan must guard its source Web IR revision",
            ));
        }
        if self.risk == TaskRiskClass::UnknownRisk {
            return Err(TaskCompilationError::new(
                "risk",
                "unknown risk cannot be represented by a fail-closed execution plan",
            ));
        }

        self.scope.validate().map_err(TaskCompilationError::from)?;
        self.scope
            .validate_for_task(self.task)
            .map_err(TaskCompilationError::from)?;
        self.limits.validate().map_err(TaskCompilationError::from)?;
        if self.postconditions.is_empty() {
            return Err(TaskCompilationError::new(
                "postconditions",
                "compiled execution plans require at least one verification postcondition",
            ));
        }
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
            if postcondition.kind == crate::task_protocol::TaskPostconditionKind::RecordsExtracted
                && let Some(expected) = postcondition.expected.as_deref()
            {
                let minimum = expected.parse::<u32>().map_err(|_| {
                    TaskCompilationError::new(
                        format!("postconditions[{index}].expected"),
                        "recordsExtracted expected must be a non-negative integer",
                    )
                })?;
                if minimum > self.limits.max_items {
                    return Err(TaskCompilationError::new(
                        format!("postconditions[{index}].expected"),
                        "recordsExtracted expected exceeds plan maxItems",
                    ));
                }
            }
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
        if confirmation_required_for(self.risk, self.ambiguity, self.revision)
            && !self.confirmation_required
        {
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
            if step.operation == TaskPlanOperation::ObserveScope && !step.entity_ids.is_empty() {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].entityIds"),
                    "scope observation cannot carry resolved entity IDs",
                ));
            }
            if step.operation != TaskPlanOperation::ObserveScope
                && (step.entity_ids.is_empty()
                    || step
                        .entity_ids
                        .iter()
                        .any(|entity_id| !selected.contains(entity_id.as_str())))
            {
                return Err(TaskCompilationError::new(
                    format!("steps[{index}].entityIds"),
                    "task operations require selected source entity IDs",
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
    /// Canonical representation of executable fields only. Advisory memory
    /// provenance is intentionally removed so enabled and disabled plans can
    /// be compared without allowing memory to alter execution.
    pub fn executable_canonical_json(&self) -> Result<String, TaskCompilationError> {
        let mut executable = self.clone();
        executable.memory_advisory = None;
        executable.to_canonical_json()
    }
}

/// Compile an authored task without browser access or side effects.
pub fn compile_task(
    task: &GlassTask,
    ir: &GlassWebIrV1,
) -> Result<TaskExecutionPlan, TaskCompilationError> {
    compile_task_with_options(task, ir, TaskCompilationOptions::default())
}

/// Compile a task while optionally consulting bounded historical knowledge.
/// Historical records can only produce advisory provenance; entity selection,
/// executable preconditions, and postconditions always come from live IR.
pub fn compile_task_with_options(
    task: &GlassTask,
    ir: &GlassWebIrV1,
    options: TaskCompilationOptions<'_>,
) -> Result<TaskExecutionPlan, TaskCompilationError> {
    task.validate().map_err(TaskCompilationError::from)?;
    ir.validate()
        .map_err(|error| TaskCompilationError::new(format!("ir.{}", error.path), error.reason))?;
    let effective_risk = max_risk(task.risk, minimum_risk_for_task(task.task));
    let confirmation_required =
        confirmation_required_for(effective_risk, task.ambiguity, task.revision);
    let require_complete = effective_risk != TaskRiskClass::ReadOnly;
    let minimum_quality = EvidenceQuality::Strong;
    let mut required_sources = if matches!(
        task.task,
        TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel
    ) {
        vec![EvidenceSource::Dialogs]
    } else {
        vec![EvidenceSource::Accessibility]
    };
    if matches!(
        task.task,
        TaskKind::FormFill | TaskKind::FormValidate | TaskKind::FormSubmit
    ) {
        required_sources.push(EvidenceSource::Forms);
    }
    required_sources.sort();
    required_sources.dedup();
    if require_complete && ir.limits.truncated {
        return Err(TaskCompilationError::new(
            "ir.limits.truncated",
            "mutating tasks require complete bounded source evidence",
        ));
    }
    for source in &required_sources {
        if ir.limits.missing_sources.contains(source) {
            return Err(TaskCompilationError::new(
                "ir.limits.missingSources",
                format!("required {source:?} evidence is unavailable"),
            ));
        }
    }

    let selected = select_entities(task, ir)?;
    for entity in &selected {
        if !quality_satisfies(entity.quality, minimum_quality) {
            return Err(TaskCompilationError::new(
                "ir.entities",
                format!(
                    "entity {:?} does not satisfy the {:?} evidence floor",
                    entity.id, minimum_quality
                ),
            ));
        }
    }
    let mut entity_evidence_requirements = Vec::with_capacity(selected.len());
    for entity in &selected {
        let mut entity_sources = required_sources_for_selected_entity(task.task, entity);
        entity_sources.sort();
        entity_sources.dedup();
        for source in &entity_sources {
            if !entity.evidence_sources.contains(source) {
                return Err(TaskCompilationError::new(
                    format!("ir.entities.{}.evidenceSources", entity.id),
                    format!("selected entity lacks required {source:?} evidence"),
                ));
            }
        }
        entity_evidence_requirements.push(TaskEntityEvidenceRequirement {
            entity_id: entity.id.clone(),
            minimum_quality,
            required_sources: entity_sources,
        });
    }
    entity_evidence_requirements.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));

    let mut selected_entity_ids = selected
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<Vec<_>>();
    selected_entity_ids.sort();
    selected_entity_ids.dedup();
    let action = action_for_task(task.task);
    let mut preconditions = vec![TaskPlanPrecondition::RevisionEquals {
        revision: ir.revision,
    }];
    let mut action_supported = false;
    for entity_id in &selected_entity_ids {
        preconditions.push(TaskPlanPrecondition::EntityPresent {
            entity_id: entity_id.clone(),
        });
        let details = ir.entity_details.get(entity_id).ok_or_else(|| {
            TaskCompilationError::new(
                format!("ir.entityDetails.{entity_id}"),
                "selected entity is missing required execution details",
            )
        })?;
        if require_complete {
            preconditions.push(TaskPlanPrecondition::EntityEnabled {
                entity_id: entity_id.clone(),
            });
        }
        if let Some(supported_action) = supported_action_for_task(task.task, details) {
            action_supported = true;
            preconditions.push(TaskPlanPrecondition::ActionSupported {
                entity_id: entity_id.clone(),
                action: supported_action,
            });
        }
    }
    if !action_supported {
        return Err(TaskCompilationError::new(
            "ir.entityDetails.supportedActions",
            format!("selected entities do not support {action:?}"),
        ));
    }

    let input_names = if task.task == TaskKind::FormSubmit {
        vec!["submit".to_string()]
    } else {
        task.inputs.keys().cloned().collect::<Vec<_>>()
    };
    let operations = [
        TaskPlanOperation::ObserveScope,
        operation_for_task(task.task),
    ];
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
            entity_ids: if operation == TaskPlanOperation::ObserveScope {
                Vec::new()
            } else {
                selected_entity_ids.clone()
            },
            requires_confirmation: confirmation_required
                && operation != TaskPlanOperation::ObserveScope,
        })
        .collect();
    let memory_advisory = match (options.knowledge_store, options.knowledge_context) {
        (Some(store), Some(context)) => {
            // A caller-provided context is only trusted when it describes the
            // exact Web IR revision being compiled. Otherwise historical
            // validation could accidentally make an old record look current.
            if context.current_revision != ir.revision {
                return Err(TaskCompilationError::new(
                    "memory.currentRevision",
                    "knowledge context revision must match the current Web IR revision",
                ));
            }
            let mut query = KnowledgeRetrievalQuery {
                page_kind: ir.document.kind.clone(),
                task_kind: Some(format!("{:?}", task.task)),
                max_results: 8,
                ..KnowledgeRetrievalQuery::default()
            };
            query.entity_roles = selected
                .iter()
                .filter_map(|entity| entity.role.clone())
                .collect();
            query.landmarks = selected
                .iter()
                .map(|entity| format!("{:?}", entity.kind))
                .collect();
            let retrieval = store.retrieve(context, &query);
            Some(TaskMemoryAdvisory {
                influence: if retrieval.selected_record_ids.is_empty() {
                    KnowledgeMemoryInfluence::None
                } else {
                    KnowledgeMemoryInfluence::RankingOnly
                },
                retrieval,
                // The executable fields above are fully derived from live
                // Web IR. Retrieval is deliberately performed only after
                // those fields are fixed and can therefore only rank.
                executable_plan_unchanged: true,
            })
        }
        _ => None,
    };
    let task_fingerprint = task_fingerprint(task, ir, &selected_entity_ids)?;
    let required_runtime_capabilities = runtime_capabilities_for(task);
    let mut entity_binding_keys = selected
        .iter()
        .map(|entity| TaskEntityBindingKey {
            entity_id: entity.id.clone(),
            kind: entity.kind,
            role: entity.role.clone(),
            name: entity.name.clone(),
        })
        .collect::<Vec<_>>();
    entity_binding_keys.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    let plan = TaskExecutionPlan {
        schema_version: TASK_PLAN_SCHEMA_VERSION,
        task_schema_version: task.schema_version,
        compiler_version: TASK_COMPILER_VERSION,
        source_ir_schema_version: ir.schema_version,
        source_ir_revision: ir.revision,
        task_fingerprint,
        task: task.task,
        scope: task.scope.clone(),
        limits: task.limits,
        risk: effective_risk,
        ambiguity: task.ambiguity,
        revision: task.revision,
        confirmation_required,
        selected_entity_ids,
        evidence_requirements: TaskEvidenceRequirements {
            minimum_quality,
            required_sources,
            require_complete,
        },
        entity_evidence_requirements,
        entity_binding_keys,
        required_runtime_capabilities,
        preconditions,
        steps,
        memory_advisory,
        postconditions: effective_postconditions(task),
    };
    plan.validate()?;
    Ok(plan)
}

fn required_sources_for_selected_entity(
    task: TaskKind,
    entity: &WebIrEntity,
) -> Vec<EvidenceSource> {
    if matches!(
        task,
        TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel
    ) {
        return vec![EvidenceSource::Dialogs];
    }
    if matches!(task, TaskKind::FormFill | TaskKind::FormValidate)
        && entity.kind == WebIrEntityKind::Field
    {
        return vec![EvidenceSource::Accessibility, EvidenceSource::Forms];
    }
    vec![EvidenceSource::Accessibility]
}

fn runtime_capabilities_for(task: &GlassTask) -> Vec<TaskRuntimeCapability> {
    let mut capabilities = vec![TaskRuntimeCapability::Observe];
    capabilities.push(match task.task {
        TaskKind::FormInspect
        | TaskKind::FormValidate
        | TaskKind::FieldRead
        | TaskKind::DialogInspect => TaskRuntimeCapability::Read,
        TaskKind::TableExtract | TaskKind::CollectionExtract | TaskKind::RegionExtract => {
            TaskRuntimeCapability::Extract
        }
        TaskKind::NavigationFollow => TaskRuntimeCapability::Navigate,
        TaskKind::DialogConfirm | TaskKind::DialogCancel => TaskRuntimeCapability::Dialog,
        TaskKind::PaginationNext | TaskKind::PaginationCollect => TaskRuntimeCapability::Pagination,
        TaskKind::FormFill
        | TaskKind::FormSubmit
        | TaskKind::NavigationSelectTab
        | TaskKind::NavigationOpenMenu => TaskRuntimeCapability::Mutate,
    });
    if task
        .postconditions
        .iter()
        .any(|postcondition| postcondition.kind == TaskPostconditionKind::EntityState)
    {
        capabilities.push(TaskRuntimeCapability::VerifyEntityState);
    }
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

/// Convenience entry point for an explicitly enabled knowledge-assisted
/// compilation.
pub fn compile_task_with_knowledge(
    task: &GlassTask,
    ir: &GlassWebIrV1,
    knowledge_store: &KnowledgeStore,
    knowledge_context: &KnowledgeLookupContext,
) -> Result<TaskExecutionPlan, TaskCompilationError> {
    compile_task_with_options(
        task,
        ir,
        TaskCompilationOptions {
            knowledge_store: Some(knowledge_store),
            knowledge_context: Some(knowledge_context),
        },
    )
}

pub(crate) fn effective_postconditions(task: &GlassTask) -> Vec<TaskPostcondition> {
    if !task.postconditions.is_empty() {
        return task.postconditions.clone();
    }
    let kind = match task.task {
        TaskKind::FormValidate => TaskPostconditionKind::ValidationClear,
        TaskKind::FormSubmit | TaskKind::NavigationFollow | TaskKind::PaginationNext => {
            TaskPostconditionKind::NavigationOccurred
        }
        TaskKind::TableExtract
        | TaskKind::CollectionExtract
        | TaskKind::RegionExtract
        | TaskKind::PaginationCollect => TaskPostconditionKind::RecordsExtracted,
        TaskKind::DialogConfirm | TaskKind::DialogCancel => TaskPostconditionKind::DialogClosed,
        TaskKind::FormInspect
        | TaskKind::FormFill
        | TaskKind::NavigationSelectTab
        | TaskKind::NavigationOpenMenu
        | TaskKind::FieldRead
        | TaskKind::DialogInspect => TaskPostconditionKind::PageKind,
    };
    vec![TaskPostcondition {
        kind,
        expected: None,
    }]
}

fn select_entities<'a>(
    task: &GlassTask,
    ir: &'a GlassWebIrV1,
) -> Result<Vec<&'a WebIrEntity>, TaskCompilationError> {
    let selector = task
        .scope
        .entity_name
        .as_deref()
        .or_else(|| task_selector_name(task));
    let mut candidates = ir
        .entities
        .iter()
        .filter(|entity| {
            task.scope.entity_kind.map_or_else(
                || entity_kind_matches_task(entity, task.task),
                |kind| entity.kind == kind,
            )
        })
        .collect::<Vec<_>>();
    if let Some(selector) = selector {
        candidates.retain(|entity| {
            entity
                .name
                .as_deref()
                .is_some_and(|name| normalized_name(name) == normalized_name(selector))
        });
    }
    candidates.sort_by_key(|entity| entity.id.as_str());
    if candidates.is_empty() {
        return Err(TaskCompilationError::new(
            "scope",
            "no compatible entity exists in the source Glass Web IR",
        ));
    }
    if candidates.len() > 1 && task.ambiguity == TaskAmbiguityPolicy::Fail {
        let mut candidate_ids = candidates
            .iter()
            .take(8)
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if candidates.len() > 8 {
            candidate_ids.push_str(", …");
        }
        return Err(TaskCompilationError::new(
            "scope",
            format!(
                "multiple compatible entities exist and ambiguity policy is fail: {candidate_ids}"
            ),
        ));
    }
    let mut selected = candidates;
    let selected_scope_ids = selected
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<BTreeSet<_>>();

    if task.task == TaskKind::FormFill {
        for input_name in task.inputs.keys() {
            let fields = ir
                .entities
                .iter()
                .filter(|entity| {
                    entity.kind == WebIrEntityKind::Field
                        && selected_scope_ids
                            .iter()
                            .any(|form_id| entity_is_scoped_to(ir, form_id, entity.id.as_str()))
                        && entity.name.as_deref().is_some_and(|name| {
                            normalized_name(name) == normalized_name(input_name)
                        })
                })
                .collect::<Vec<_>>();
            if fields.len() != 1 {
                let candidates = ir
                    .entities
                    .iter()
                    .filter(|entity| {
                        entity.kind == WebIrEntityKind::Field
                            && entity.name.as_deref().is_some_and(|name| {
                                normalized_name(name) == normalized_name(input_name)
                            })
                    })
                    .take(8)
                    .map(|entity| {
                        let parents = ir
                            .relationships
                            .iter()
                            .filter(|relationship| relationship.to == entity.id)
                            .map(|relationship| relationship.from.as_str())
                            .take(4)
                            .collect::<Vec<_>>()
                            .join("|");
                        format!("{}<-{}", entity.id, parents)
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                return Err(TaskCompilationError::new(
                    format!("inputs.{input_name}"),
                    format!(
                        "form input must resolve to exactly one source Web IR field (found {}; scope {}; candidates {})",
                        fields.len(),
                        selected_scope_ids
                            .iter()
                            .copied()
                            .collect::<Vec<_>>()
                            .join("|"),
                        candidates
                    ),
                ));
            }
            selected.push(fields[0]);
        }
    }
    if task.task == TaskKind::FormSubmit {
        let submit_name = &task.inputs["submit"];
        let submitters =
            ir.entities
                .iter()
                .filter(|entity| {
                    matches!(
                        entity.kind,
                        WebIrEntityKind::Action | WebIrEntityKind::UnknownInteractive
                    ) && selected_scope_ids
                        .iter()
                        .any(|form_id| entity_is_scoped_to(ir, form_id, entity.id.as_str()))
                        && entity.name.as_deref().is_some_and(|name| {
                            normalized_name(name) == normalized_name(submit_name)
                        })
                })
                .collect::<Vec<_>>();
        if submitters.len() != 1 {
            return Err(TaskCompilationError::new(
                "inputs.submit",
                "submit target must resolve to exactly one source Web IR action",
            ));
        }
        selected.push(submitters[0]);
    }
    selected.sort_by_key(|entity| entity.id.as_str());
    selected.dedup_by_key(|entity| entity.id.as_str());
    Ok(selected)
}

fn entity_is_scoped_to(ir: &GlassWebIrV1, scope_id: &str, entity_id: &str) -> bool {
    let mut frontier = vec![scope_id];
    let mut visited = BTreeSet::new();
    while let Some(parent) = frontier.pop() {
        if !visited.insert(parent) {
            continue;
        }
        for relationship in &ir.relationships {
            if !matches!(
                relationship.kind,
                WebIrRelationshipKind::Contains
                    | WebIrRelationshipKind::Owns
                    | WebIrRelationshipKind::Submits
                    | WebIrRelationshipKind::ScopedTo
            ) {
                continue;
            }
            if relationship.from == parent {
                if relationship.to == entity_id {
                    return true;
                }
                frontier.push(relationship.to.as_str());
            } else if relationship.kind == WebIrRelationshipKind::ScopedTo
                && relationship.to == parent
            {
                if relationship.from == entity_id {
                    return true;
                }
                frontier.push(relationship.from.as_str());
            }
        }
    }
    false
}

fn entity_kind_matches_task(entity: &WebIrEntity, task: TaskKind) -> bool {
    match task {
        TaskKind::FormInspect
        | TaskKind::FormFill
        | TaskKind::FormValidate
        | TaskKind::FormSubmit => entity.kind == WebIrEntityKind::Form,
        TaskKind::NavigationFollow => entity.kind == WebIrEntityKind::Page,
        TaskKind::NavigationSelectTab => {
            entity.kind == WebIrEntityKind::Tab || entity.role.as_deref() == Some("tab")
        }
        TaskKind::NavigationOpenMenu => matches!(
            entity.kind,
            WebIrEntityKind::NavigationItem
                | WebIrEntityKind::Action
                | WebIrEntityKind::UnknownInteractive
        ),
        TaskKind::TableExtract => entity.kind == WebIrEntityKind::Table,
        TaskKind::CollectionExtract => entity.kind == WebIrEntityKind::Collection,
        TaskKind::RegionExtract => matches!(
            entity.kind,
            WebIrEntityKind::Region
                | WebIrEntityKind::Form
                | WebIrEntityKind::Table
                | WebIrEntityKind::Collection
                | WebIrEntityKind::Dialog
        ),
        TaskKind::FieldRead => entity.kind == WebIrEntityKind::Field,
        TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => {
            entity.kind == WebIrEntityKind::Dialog
        }
        TaskKind::PaginationNext | TaskKind::PaginationCollect => {
            matches!(
                entity.kind,
                WebIrEntityKind::PaginationControl
                    | WebIrEntityKind::Action
                    | WebIrEntityKind::NavigationItem
                    | WebIrEntityKind::UnknownInteractive
            )
        }
    }
}

fn task_selector_name(task: &GlassTask) -> Option<&str> {
    match task.task {
        TaskKind::NavigationSelectTab => task.inputs.get("tab").map(String::as_str),
        TaskKind::NavigationOpenMenu => task.inputs.get("menu").map(String::as_str),
        TaskKind::FieldRead => task.inputs.get("field").map(String::as_str),
        TaskKind::PaginationNext | TaskKind::PaginationCollect => {
            task.inputs.get("next").map(String::as_str)
        }
        TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => None,
        TaskKind::NavigationFollow => None,
        _ => task.scope.region_name.as_deref(),
    }
}

fn normalized_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn quality_satisfies(actual: EvidenceQuality, minimum: EvidenceQuality) -> bool {
    fn rank(quality: EvidenceQuality) -> u8 {
        match quality {
            EvidenceQuality::Opaque => 0,
            EvidenceQuality::Conflicted => 1,
            EvidenceQuality::Inferred => 2,
            EvidenceQuality::Partial => 3,
            EvidenceQuality::Strong => 4,
            EvidenceQuality::Confirmed => 5,
        }
    }
    rank(actual) >= rank(minimum)
}

fn action_for_task(task: TaskKind) -> WebIrAction {
    match task {
        TaskKind::FormInspect | TaskKind::FormValidate | TaskKind::FieldRead => WebIrAction::Read,
        TaskKind::FormFill => WebIrAction::Type,
        TaskKind::FormSubmit => WebIrAction::Submit,
        TaskKind::NavigationFollow => WebIrAction::Navigate,
        TaskKind::NavigationSelectTab => WebIrAction::Select,
        TaskKind::NavigationOpenMenu => WebIrAction::Click,
        TaskKind::TableExtract
        | TaskKind::CollectionExtract
        | TaskKind::RegionExtract
        | TaskKind::PaginationCollect => WebIrAction::Extract,
        TaskKind::DialogInspect => WebIrAction::Read,
        TaskKind::DialogConfirm => WebIrAction::Confirm,
        TaskKind::DialogCancel => WebIrAction::Cancel,
        TaskKind::PaginationNext => WebIrAction::Paginate,
    }
}

fn supported_action_for_task(task: TaskKind, details: &WebIrEntityDetails) -> Option<WebIrAction> {
    let preferred = action_for_task(task);
    if details.supported_actions.contains(&preferred) {
        return Some(preferred);
    }
    (task == TaskKind::PaginationNext && details.supported_actions.contains(&WebIrAction::Click))
        .then_some(WebIrAction::Click)
}

fn task_fingerprint(
    task: &GlassTask,
    ir: &GlassWebIrV1,
    selected_entity_ids: &[String],
) -> Result<String, TaskCompilationError> {
    let input_names = task.inputs.keys().collect::<Vec<_>>();
    let material = serde_json::to_vec(&serde_json::json!({
        "compilerVersion": TASK_COMPILER_VERSION,
        "taskSchemaVersion": task.schema_version,
        "task": task.task,
        "scope": task.scope,
        "inputNames": input_names,
        "limits": task.limits,
        "risk": task.risk,
        "ambiguity": task.ambiguity,
        "revision": task.revision,
        "postconditions": task.postconditions,
        "sourceIrSchemaVersion": ir.schema_version,
        "sourceIrRevision": ir.revision,
        "selectedEntityIds": selected_entity_ids,
    }))
    .map_err(|error| TaskCompilationError::new("taskFingerprint", error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(material)))
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

fn confirmation_required_for(
    risk: TaskRiskClass,
    ambiguity: TaskAmbiguityPolicy,
    revision: TaskRevisionPolicy,
) -> bool {
    matches!(
        risk,
        TaskRiskClass::RemoteIrreversible
            | TaskRiskClass::Authentication
            | TaskRiskClass::DataDisclosure
            | TaskRiskClass::UnknownRisk
    ) || matches!(ambiguity, TaskAmbiguityPolicy::RequireConfirmation)
        || (revision == TaskRevisionPolicy::Reextract && risk != TaskRiskClass::ReadOnly)
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
pub(crate) fn test_compiler_ir() -> GlassWebIrV1 {
    use crate::extraction::{
        EvidenceCoverage, EvidenceFact, ExtractionEvidence, ExtractionEvidenceLimits,
        ExtractionScope,
    };
    let facts = [
        (EvidenceSource::Accessibility, "form", "Checkout"),
        (EvidenceSource::Accessibility, "textbox", "Email"),
        (EvidenceSource::Forms, "textbox", "Email"),
        (EvidenceSource::Accessibility, "button", "Submit"),
        (EvidenceSource::Forms, "button", "Submit"),
        (EvidenceSource::Accessibility, "tab", "Payment"),
        (EvidenceSource::Accessibility, "menuitem", "Products"),
        (EvidenceSource::Accessibility, "table", "Checkout"),
        (EvidenceSource::Accessibility, "list", "Checkout"),
        (EvidenceSource::Accessibility, "main", "Checkout"),
        (EvidenceSource::Dialogs, "dialog", "Checkout"),
        (EvidenceSource::Accessibility, "link", "Next"),
        (EvidenceSource::Accessibility, "button", "Next"),
    ]
    .into_iter()
    .map(|(source, role, name)| EvidenceFact {
        source,
        kind: if source == EvidenceSource::Forms {
            "control"
        } else {
            "node"
        }
        .into(),
        quality: if source == EvidenceSource::Forms {
            EvidenceQuality::Strong
        } else {
            EvidenceQuality::Confirmed
        },
        role: Some(role.into()),
        name: Some(name.into()),
        input_type: (role == "textbox").then(|| "email".into()),
        autocomplete: (role == "textbox").then(|| "email".into()),
        required: (role == "textbox").then_some(true),
        read_only: (role == "textbox").then_some(false),
        empty: (role == "textbox").then_some(true),
        checked: None,
        disabled: Some(false),
        geometry_present: None,
        parent_role: matches!(name, "Email" | "Submit").then(|| "form".into()),
        relationship_hint: None,
    })
    .collect();
    crate::web_ir::reconcile_evidence(&ExtractionEvidence {
        schema_version: crate::extraction::EXTRACTION_CONTRACT_SCHEMA_VERSION,
        revision: 7,
        scope: ExtractionScope::Document,
        sources: vec![EvidenceSource::Accessibility, EvidenceSource::Forms],
        facts,
        limits: ExtractionEvidenceLimits {
            truncated: false,
            omitted_facts: 0,
            text_bytes: 256,
            missing_sources: Vec::new(),
        },
        coverage: EvidenceCoverage {
            structural: EvidenceQuality::Strong,
            semantic: EvidenceQuality::Strong,
            interactive_entities_observed: 5,
            opaque_regions: 0,
            reasons: Vec::new(),
        },
        surface_set: None,
    })
    .unwrap()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_protocol::{TaskAmbiguityPolicy, TaskLimits, TaskPostconditionKind, TaskScope};
    use std::collections::BTreeMap;

    fn task(kind: TaskKind, risk: TaskRiskClass) -> GlassTask {
        let inputs = match kind {
            TaskKind::FormFill => BTreeMap::from([("email".into(), "a@example.test".into())]),
            TaskKind::FormSubmit => BTreeMap::from([("submit".into(), "Submit".into())]),
            TaskKind::NavigationFollow => {
                BTreeMap::from([("url".into(), "https://example.test/next".into())])
            }
            TaskKind::NavigationSelectTab => BTreeMap::from([("tab".into(), "Payment".into())]),
            TaskKind::NavigationOpenMenu => BTreeMap::from([("menu".into(), "Products".into())]),
            TaskKind::FieldRead => BTreeMap::from([("field".into(), "Email".into())]),
            TaskKind::PaginationNext | TaskKind::PaginationCollect => {
                BTreeMap::from([("next".into(), "Next".into())])
            }
            _ => BTreeMap::new(),
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
                entity_kind: (kind == TaskKind::RegionExtract).then_some(WebIrEntityKind::Region),
                ..TaskScope::default()
            },
            inputs,
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
        let plan = compile_task(
            &task(TaskKind::FormFill, TaskRiskClass::LocalMutation),
            &test_compiler_ir(),
        )
        .unwrap();
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
        let plan = compile_task(
            &task(TaskKind::FormSubmit, TaskRiskClass::ReadOnly),
            &test_compiler_ir(),
        )
        .unwrap();
        assert_eq!(plan.steps[1].input_names, vec!["submit"]);
        assert_eq!(plan.risk, TaskRiskClass::RemoteIrreversible);
        assert!(plan.confirmation_required);
    }

    #[test]
    fn navigation_compiles_against_the_document_execution_boundary() {
        let authored = task(TaskKind::NavigationFollow, TaskRiskClass::ReadOnly);

        let plan = compile_task(&authored, &test_compiler_ir()).unwrap();

        assert_eq!(plan.selected_entity_ids, ["page"]);
        assert!(plan.preconditions.iter().any(|precondition| matches!(
            precondition,
            TaskPlanPrecondition::ActionSupported {
                entity_id,
                action: WebIrAction::Navigate,
            } if entity_id == "page"
        )));
    }

    #[test]
    fn compiler_generates_verification_postconditions_from_task_intent() {
        let mut authored = task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly);
        authored.postconditions.clear();

        let plan = compile_task(&authored, &test_compiler_ir()).unwrap();

        assert_eq!(
            plan.postconditions,
            vec![TaskPostcondition {
                kind: TaskPostconditionKind::RecordsExtracted,
                expected: None,
            }]
        );
    }

    #[test]
    fn plan_rejects_extra_operations_and_preserves_revision_policy() {
        let mut plan = compile_task(
            &task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly),
            &test_compiler_ir(),
        )
        .unwrap();
        plan.steps.push(plan.steps[1].clone());
        assert_eq!(plan.validate().unwrap_err().path, "steps");
        let mut authored = task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly);
        authored.revision = TaskRevisionPolicy::Compatible;
        let plan = compile_task(&authored, &test_compiler_ir()).unwrap();
        assert_eq!(plan.revision, TaskRevisionPolicy::Compatible);
    }

    #[test]
    fn plan_validation_rejects_downgraded_risk() {
        let mut plan = compile_task(
            &task(TaskKind::FormSubmit, TaskRiskClass::RemoteIrreversible),
            &test_compiler_ir(),
        )
        .unwrap();
        plan.risk = TaskRiskClass::ReadOnly;
        assert_eq!(plan.validate().unwrap_err().path, "risk");
    }
    #[test]
    fn compilation_is_deterministic_for_identical_authored_tasks() {
        let authored = task(TaskKind::FormFill, TaskRiskClass::LocalMutation);
        let first = compile_task(&authored, &test_compiler_ir())
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let second = compile_task(&authored, &test_compiler_ir())
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn compiler_rejects_incompatible_scope_kind() {
        let mut authored = task(TaskKind::FormFill, TaskRiskClass::LocalMutation);
        authored.scope.entity_kind = Some(crate::web_ir::WebIrEntityKind::Table);
        let error = compile_task(&authored, &test_compiler_ir()).unwrap_err();
        assert_eq!(error.path, "scope.entityKind");
    }

    #[test]
    fn compiler_declares_entity_state_runtime_capability() {
        let mut authored = task(TaskKind::FormFill, TaskRiskClass::LocalMutation);
        authored.postconditions = vec![TaskPostcondition {
            kind: TaskPostconditionKind::EntityState,
            expected: Some("Email.empty=false".into()),
        }];
        let plan = compile_task(&authored, &test_compiler_ir()).unwrap();
        assert!(
            plan.required_runtime_capabilities
                .contains(&TaskRuntimeCapability::VerifyEntityState)
        );
    }

    #[test]
    fn compiler_rejects_unknown_risk_before_plan_emission() {
        let error = compile_task(
            &task(TaskKind::FormFill, TaskRiskClass::UnknownRisk),
            &test_compiler_ir(),
        )
        .unwrap_err();
        assert_eq!(error.path, "risk");
    }

    #[test]
    fn compiler_rejects_selected_entities_without_execution_details() {
        let mut ir = test_compiler_ir();
        let form_id = ir
            .entities
            .iter()
            .find(|entity| entity.kind == crate::web_ir::WebIrEntityKind::Form)
            .unwrap()
            .id
            .clone();
        ir.entity_details.remove(&form_id);

        let error =
            compile_task(&task(TaskKind::FormInspect, TaskRiskClass::ReadOnly), &ir).unwrap_err();

        assert_eq!(error.path, format!("ir.entityDetails.{form_id}"));
    }

    #[test]
    fn compiler_requires_explicit_confirmation_for_ambiguous_entities() {
        let mut ir = test_compiler_ir();
        let form = ir
            .entities
            .iter()
            .find(|entity| entity.kind == crate::web_ir::WebIrEntityKind::Form)
            .unwrap()
            .clone();
        let mut duplicate = form.clone();
        duplicate.id = "entity_form_duplicate".into();
        let mut duplicate_details = ir.entity_details[&form.id].clone();
        duplicate_details.semantic_stability_key = Some("form|duplicate|checkout".into());
        ir.entities.push(duplicate.clone());
        ir.entity_details
            .insert(duplicate.id.clone(), duplicate_details);
        ir.validate().unwrap();

        let authored = task(TaskKind::FormInspect, TaskRiskClass::ReadOnly);
        let error = compile_task(&authored, &ir).unwrap_err();
        assert_eq!(error.path, "scope");

        let mut confirmed = authored;
        confirmed.ambiguity = TaskAmbiguityPolicy::RequireConfirmation;
        let plan = compile_task(&confirmed, &ir).unwrap();
        assert!(plan.confirmation_required);
        assert_eq!(plan.selected_entity_ids.len(), 2);
    }

    #[test]
    fn compiler_does_not_fall_back_when_named_target_is_absent() {
        let mut authored = task(TaskKind::NavigationOpenMenu, TaskRiskClass::LocalMutation);
        authored.inputs.insert("menu".into(), "Missing menu".into());

        let error = compile_task(&authored, &test_compiler_ir()).unwrap_err();

        assert_eq!(error.path, "scope");
        assert_eq!(
            error.reason,
            "no compatible entity exists in the source Glass Web IR"
        );
    }

    #[test]
    fn form_compilation_scopes_duplicate_fields_to_the_selected_form() {
        let mut ir = test_compiler_ir();
        let original_form = ir
            .entities
            .iter()
            .find(|entity| entity.kind == WebIrEntityKind::Form)
            .unwrap()
            .clone();
        let original_field = ir
            .entities
            .iter()
            .find(|entity| entity.kind == WebIrEntityKind::Field)
            .unwrap()
            .clone();
        let mut other_form = original_form.clone();
        other_form.id = "other-form".into();
        other_form.name = Some("Other".into());
        let mut other_field = original_field.clone();
        other_field.id = "other-email".into();
        ir.entity_details.insert(
            other_form.id.clone(),
            ir.entity_details[&original_form.id].clone(),
        );
        ir.entity_details.insert(
            other_field.id.clone(),
            ir.entity_details[&original_field.id].clone(),
        );
        ir.entities
            .extend([other_form.clone(), other_field.clone()]);
        ir.relationships.push(crate::web_ir::WebIrRelationship {
            from: other_form.id,
            to: other_field.id,
            kind: WebIrRelationshipKind::Owns,
        });
        ir.entities.sort_by(|left, right| left.id.cmp(&right.id));
        ir.relationships.sort_by_key(|relationship| {
            (
                relationship.from.clone(),
                relationship.to.clone(),
                relationship.kind,
            )
        });
        ir.validate().unwrap();
        let mut authored = task(TaskKind::FormFill, TaskRiskClass::LocalMutation);
        authored.scope.entity_name = Some("Checkout".into());
        authored.scope.entity_kind = Some(WebIrEntityKind::Form);

        let plan = compile_task(&authored, &ir).unwrap();

        assert!(plan.selected_entity_ids.contains(&original_field.id));
        assert!(
            !plan
                .selected_entity_ids
                .contains(&"other-email".to_string())
        );
    }

    #[test]
    fn unrelated_form_evidence_cannot_satisfy_the_selected_field() {
        let mut ir = test_compiler_ir();
        let field = ir
            .entities
            .iter_mut()
            .find(|entity| entity.kind == WebIrEntityKind::Field)
            .unwrap();
        field
            .evidence_sources
            .retain(|source| *source != EvidenceSource::Forms);
        let error =
            compile_task(&task(TaskKind::FormFill, TaskRiskClass::LocalMutation), &ir).unwrap_err();
        assert!(error.path.ends_with("evidenceSources"));
        assert!(error.reason.contains("Forms"));
    }

    #[test]
    fn pagination_next_accepts_a_named_clickable_control() {
        let mut ir = test_compiler_ir();
        let control = ir
            .entities
            .iter_mut()
            .find(|entity| {
                entity.role.as_deref() == Some("button") && entity.name.as_deref() == Some("Next")
            })
            .unwrap();
        control.kind = WebIrEntityKind::Action;
        control.name = Some("No-op next".into());
        let details = ir.entity_details.get_mut(&control.id).unwrap();
        details.supported_actions = vec![WebIrAction::Click];
        details.semantic_stability_key = Some("action|button|no-op next".into());
        ir.validate().unwrap();
        let mut authored = task(TaskKind::PaginationNext, TaskRiskClass::LocalMutation);
        authored.inputs.insert("next".into(), "No-op next".into());

        let plan = compile_task(&authored, &ir).unwrap();

        assert!(plan.preconditions.iter().any(|precondition| matches!(
            precondition,
            TaskPlanPrecondition::ActionSupported {
                action: WebIrAction::Click,
                ..
            }
        )));
    }

    #[test]
    fn plan_rejects_records_postcondition_beyond_item_limit() {
        let mut plan = compile_task(
            &task(TaskKind::RegionExtract, TaskRiskClass::ReadOnly),
            &test_compiler_ir(),
        )
        .unwrap();
        plan.limits.max_items = 1;
        plan.postconditions[0].expected = Some("2".into());
        assert_eq!(
            plan.validate().unwrap_err().path,
            "postconditions[0].expected"
        );
    }

    #[test]
    fn memory_bypass_preserves_the_executable_plan_byte_for_byte() {
        let authored = task(TaskKind::FormFill, TaskRiskClass::LocalMutation);
        let ir = test_compiler_ir();
        let baseline = compile_task(&authored, &ir).unwrap();
        let bypassed = compile_task_with_options(
            &authored,
            &ir,
            TaskCompilationOptions {
                knowledge_store: None,
                knowledge_context: None,
            },
        )
        .unwrap();

        assert_eq!(baseline.steps, bypassed.steps);
        assert_eq!(baseline.preconditions, bypassed.preconditions);
        assert_eq!(baseline.selected_entity_ids, bypassed.selected_entity_ids);
        assert_eq!(
            baseline.to_canonical_json().unwrap(),
            bypassed.to_canonical_json().unwrap()
        );
        assert!(bypassed.memory_advisory.is_none());
    }
}

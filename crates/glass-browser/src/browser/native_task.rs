//! Native execution of the Glass Task Protocol.
//!
//! This adapter deliberately consumes the same revisioned semantic references
//! exposed by [`BrowserRuntimeSession`] to every native caller.  It does not
//! compile a second selector language and it never creates a Chromium/CDP
//! session.  The native engine remains the owner of target resolution,
//! actionability, DOM mutation, script state, and navigation effects.

use super::runtime::BrowserRuntimeSession;
use crate::browser::session::{
    ActAndVerifyResult, ActionFailureKind, ActionStatus, FillFieldResult, FillFormOutcome,
    IntentConstraints, IntentScope, SemanticIntentAction, SemanticIntentExecutionRequest,
    SemanticIntentRequest, SemanticObservation, SemanticRegion, SemanticRegionKind, SemanticTarget,
    StructuredExtractionLimits, StructuredExtractionProvenance, StructuredExtractionRecord,
    StructuredExtractionResult, TaskExecutionReceipt, TaskExecutionResult,
    TaskPostconditionReceipt, TaskStepResult,
};
use crate::browser_backend::PromptDecision;
use crate::extraction::{EvidenceQuality, EvidenceSource};
use crate::protocol::{RetryClassification, RetryGuidance};
use crate::task_compiler::{
    TaskEntityEvidenceRequirement, TaskPlanOperation, TaskRuntimeCapability,
};
use crate::task_protocol::{
    GlassTask, TaskKind, TaskPostcondition, TaskPostconditionKind, TaskRevisionPolicy,
    TaskRiskClass,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::Duration;

const NATIVE_TASK_MAX_CONTROLS: usize = 64;
const NATIVE_TASK_MAX_EXTRACTION_BYTES: usize = 64 * 1024;

/// Execute one validated authored task through the native browser runtime.
pub(crate) async fn execute(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    expected_revision: u64,
    confirmed: bool,
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    task.validate()?;

    let observation = session.native_observe().await?;
    let source_revision = observation.revision;
    let confirmation_required = confirmation_required(task);
    let scoped_targets = match scoped_targets(&observation, task) {
        Ok(targets) => targets,
        Err(detail) => {
            return Ok(preflight_result(
                task,
                source_revision,
                &[],
                confirmation_required,
                detail,
            ));
        }
    };

    if revision_is_stale(task, expected_revision, source_revision) {
        return Ok(preflight_result(
            task,
            source_revision,
            &scoped_targets,
            confirmation_required,
            format!(
                "source revision {source_revision} is not safe for caller revision {expected_revision} under {:?}; no native mutation was dispatched",
                task.revision
            ),
        ));
    }
    if confirmation_required && !confirmed {
        return Ok(preflight_result(
            task,
            source_revision,
            &scoped_targets,
            confirmation_required,
            "confirmation is required before this task can mutate the native browser",
        ));
    }

    let mut result = match task.task {
        TaskKind::NavigationFollow => {
            execute_navigation(session, task, source_revision, confirmation_required).await?
        }
        TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => {
            execute_dialog(session, task, source_revision, confirmation_required).await?
        }
        TaskKind::FormInspect => inspect_form(
            task,
            source_revision,
            confirmation_required,
            &scoped_targets,
        ),
        TaskKind::FormValidate => {
            execute_form_validate(session, task, source_revision, confirmation_required).await?
        }
        TaskKind::FormFill => {
            execute_form_fill(
                session,
                task,
                source_revision,
                confirmation_required,
                &scoped_targets,
            )
            .await?
        }
        TaskKind::FormSubmit => {
            execute_named_click(
                session,
                task,
                source_revision,
                confirmation_required,
                &scoped_targets,
                "submit",
                TaskPlanOperation::SubmitForm,
                SemanticIntentAction::Submit,
                "submit-verification",
            )
            .await?
        }
        TaskKind::NavigationSelectTab => {
            execute_named_click(
                session,
                task,
                source_revision,
                confirmation_required,
                &scoped_targets,
                "tab",
                TaskPlanOperation::SelectTab,
                SemanticIntentAction::Click,
                "navigation-verification",
            )
            .await?
        }
        TaskKind::NavigationOpenMenu => {
            execute_named_click(
                session,
                task,
                source_revision,
                confirmation_required,
                &scoped_targets,
                "menu",
                TaskPlanOperation::OpenMenu,
                SemanticIntentAction::Open,
                "navigation-verification",
            )
            .await?
        }
        TaskKind::PaginationNext => {
            execute_named_click(
                session,
                task,
                source_revision,
                confirmation_required,
                &scoped_targets,
                "next",
                TaskPlanOperation::NextPage,
                SemanticIntentAction::Paginate,
                "pagination-verification",
            )
            .await?
        }
        TaskKind::FieldRead => {
            execute_field_read(
                session,
                task,
                source_revision,
                confirmation_required,
                &scoped_targets,
            )
            .await?
        }
        TaskKind::TableExtract => {
            execute_extraction(
                session,
                task,
                source_revision,
                confirmation_required,
                TaskPlanOperation::ExtractTable,
                "table",
            )
            .await?
        }
        TaskKind::CollectionExtract => {
            execute_extraction(
                session,
                task,
                source_revision,
                confirmation_required,
                TaskPlanOperation::ExtractCollection,
                "collection",
            )
            .await?
        }
        TaskKind::RegionExtract => {
            execute_extraction(
                session,
                task,
                source_revision,
                confirmation_required,
                TaskPlanOperation::ExtractRegion,
                "region",
            )
            .await?
        }
        TaskKind::PaginationCollect => {
            execute_pagination_collect(
                session,
                task,
                source_revision,
                confirmation_required,
                &scoped_targets,
            )
            .await?
        }
    };

    if result.status == "succeeded" {
        result = finalize(session, task, result).await?;
    }
    Ok(result)
}

fn confirmation_required(task: &GlassTask) -> bool {
    matches!(
        task.risk,
        TaskRiskClass::RemoteIrreversible
            | TaskRiskClass::Authentication
            | TaskRiskClass::DataDisclosure
            | TaskRiskClass::UnknownRisk
    ) || matches!(
        task.ambiguity,
        crate::task_protocol::TaskAmbiguityPolicy::RequireConfirmation
    ) || (task.revision == TaskRevisionPolicy::Reextract && task.risk != TaskRiskClass::ReadOnly)
}

fn revision_is_stale(task: &GlassTask, expected: u64, current: u64) -> bool {
    current < expected
        || (current != expected
            && (task.revision == TaskRevisionPolicy::Exact || task.risk != TaskRiskClass::ReadOnly))
}

fn scoped_targets<'a>(
    observation: &'a SemanticObservation,
    task: &GlassTask,
) -> Result<Vec<&'a SemanticTarget>, String> {
    let Some(region_name) = task.scope.region_name.as_deref() else {
        return if matches!(
            task.task,
            TaskKind::NavigationFollow
                | TaskKind::DialogInspect
                | TaskKind::DialogConfirm
                | TaskKind::DialogCancel
        ) {
            Ok(Vec::new())
        } else {
            Err("browser-backed task requires a semantic region scope".into())
        };
    };
    let regions = observation
        .regions
        .iter()
        .filter(|region| {
            region.label.eq_ignore_ascii_case(region_name)
                && task
                    .scope
                    .entity_kind
                    .is_none_or(|kind| region_matches_entity(region, kind))
                && task.scope.entity_name.as_deref().is_none_or(|name| {
                    region.label.eq_ignore_ascii_case(name)
                        || region
                            .targets
                            .iter()
                            .any(|target| target.name.eq_ignore_ascii_case(name))
                })
        })
        .collect::<Vec<_>>();
    match regions.as_slice() {
        [region] => Ok(region.targets.iter().collect()),
        [] => Err(format!(
            "semantic region not found in native observation: {region_name}"
        )),
        _ => Err(format!(
            "semantic region is ambiguous in native observation: {region_name}"
        )),
    }
}

fn region_matches_entity(region: &SemanticRegion, kind: crate::web_ir::WebIrEntityKind) -> bool {
    use crate::web_ir::WebIrEntityKind;
    match kind {
        WebIrEntityKind::Page
        | WebIrEntityKind::Region
        | WebIrEntityKind::OpaqueRegion
        | WebIrEntityKind::UnknownInteractive => true,
        WebIrEntityKind::Form => region.kind == SemanticRegionKind::Form,
        WebIrEntityKind::Dialog => region.kind == SemanticRegionKind::Dialog,
        WebIrEntityKind::Table => region.kind == SemanticRegionKind::Table,
        WebIrEntityKind::Collection => region.kind == SemanticRegionKind::Collection,
        WebIrEntityKind::PaginationControl => region.kind == SemanticRegionKind::Pagination,
        WebIrEntityKind::Field => region.targets.iter().any(is_field_target),
        WebIrEntityKind::Action => region.targets.iter().any(|target| {
            matches!(
                target.role.as_str(),
                "button" | "checkbox" | "radio" | "switch" | "menuitem"
            )
        }),
        WebIrEntityKind::Link => region.targets.iter().any(|target| target.role == "link"),
        WebIrEntityKind::NavigationItem => region.kind == SemanticRegionKind::Navigation,
        WebIrEntityKind::Tab => region.targets.iter().any(|target| target.role == "tab"),
        WebIrEntityKind::Row | WebIrEntityKind::Cell | WebIrEntityKind::CollectionItem => true,
        WebIrEntityKind::Text => true,
        WebIrEntityKind::Frame | WebIrEntityKind::ShadowRoot | WebIrEntityKind::Probe => false,
    }
}

fn is_field_target(target: &SemanticTarget) -> bool {
    target.input_type.is_some()
        || matches!(target.role.as_str(), "textbox" | "combobox" | "listbox")
}

fn target_for_name<'a>(
    targets: &[&'a SemanticTarget],
    name: &str,
) -> Result<&'a SemanticTarget, String> {
    let matches = targets
        .iter()
        .copied()
        .filter(|target| {
            target.name.eq_ignore_ascii_case(name) || target.reference.eq_ignore_ascii_case(name)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [target] => Ok(target),
        [] => Err(format!("native semantic target not found: {name}")),
        _ => Err(format!("native semantic target is ambiguous: {name}")),
    }
}

fn inspect_form(
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
    targets: &[&SemanticTarget],
) -> TaskExecutionResult {
    let ids = targets
        .iter()
        .map(|target| target.reference.clone())
        .collect::<Vec<_>>();
    result(
        task,
        revision,
        revision,
        &ids,
        confirmation_required,
        TaskPlanOperation::InspectForm,
        "succeeded",
        "inspection",
        false,
        None,
        None,
        None,
        Vec::new(),
        None,
    )
}

async fn execute_navigation(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let url = task
        .inputs
        .get("url")
        .ok_or("navigation.follow requires the url input")?;
    let navigation = session
        .navigate_with_revision(crate::browser::session::normalize_url(url), revision)
        .await?;
    Ok(result(
        task,
        revision,
        navigation.revision,
        &[format!("native-page:{}", navigation.url)],
        confirmation_required,
        TaskPlanOperation::FollowNavigation,
        "succeeded",
        "navigation",
        true,
        None,
        None,
        None,
        Vec::new(),
        None,
    ))
}

async fn execute_dialog(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let pending = session.native_pending_dialog().await?;
    if task.task == TaskKind::DialogInspect {
        return Ok(result(
            task,
            revision,
            revision,
            &[format!("native-dialog:{revision}")],
            confirmation_required,
            TaskPlanOperation::InspectDialog,
            "succeeded",
            "dialog-inspection",
            false,
            None,
            None,
            pending.clone(),
            if pending.is_some() {
                vec!["dialog-pending".into()]
            } else {
                Vec::new()
            },
            None,
        ));
    }
    if pending.is_none() {
        return Ok(preflight_result(
            task,
            revision,
            &[],
            confirmation_required,
            "no pending JavaScript dialog is available",
        ));
    }
    let decision = if task.task == TaskKind::DialogConfirm {
        PromptDecision::Accept
    } else {
        PromptDecision::Dismiss
    };
    let handled = session.native_resolve_dialog(decision).await?.handled;
    let operation = if task.task == TaskKind::DialogConfirm {
        TaskPlanOperation::ConfirmDialog
    } else {
        TaskPlanOperation::CancelDialog
    };
    let after = session.native_observe().await?;
    Ok(result(
        task,
        revision,
        after.revision,
        &[format!("native-dialog:{revision}")],
        confirmation_required,
        operation,
        if handled {
            "succeeded"
        } else {
            "indeterminate"
        },
        "dialog-verification",
        true,
        None,
        None,
        None,
        Vec::new(),
        (!handled).then(|| "native dialog was not handled".into()),
    ))
}

async fn execute_form_validate(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let valid = native_form_valid(session).await?;
    let after = session.native_observe().await?;
    Ok(result(
        task,
        revision,
        after.revision,
        &[format!("native-form:{revision}")],
        confirmation_required,
        TaskPlanOperation::ValidateForm,
        if valid {
            "succeeded"
        } else {
            "verification-failed"
        },
        "validation",
        false,
        None,
        None,
        None,
        Vec::new(),
        (!valid).then(|| "native form validity or required state failed".into()),
    ))
}

async fn execute_form_fill(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
    initial_targets: &[&SemanticTarget],
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let mut current_revision = revision;
    let mut filled = 0usize;
    let mut fields = Vec::with_capacity(task.inputs.len());
    let mut last_execution_id = None;
    for (name, value) in &task.inputs {
        let observation = session.native_observe().await?;
        if revision_is_stale(task, current_revision, observation.revision) {
            fields.push(FillFieldResult {
                target: name.clone(),
                action: "none".into(),
                label: None,
                success: false,
                error: Some("native page revision changed while filling the form".into()),
            });
            continue;
        }
        let targets = if observation.revision == revision {
            initial_targets.to_vec()
        } else {
            scoped_targets(&observation, task).unwrap_or_default()
        };
        let target = match target_for_name(&targets, name) {
            Ok(target) => target,
            Err(error) => {
                fields.push(FillFieldResult {
                    target: name.clone(),
                    action: "none".into(),
                    label: None,
                    success: false,
                    error: Some(error),
                });
                continue;
            }
        };
        let (action, value_for_action) = field_action(target, value);
        if action == SemanticIntentAction::Type && target.empty == Some(false) {
            let clear_result = invoke_native_action(
                session,
                target,
                task_region_for_target(&observation, target),
                SemanticIntentAction::Clear,
                None,
                observation.revision,
                task.limits.timeout_ms,
            )
            .await;
            if let Err(error) = clear_result {
                fields.push(FillFieldResult {
                    target: name.clone(),
                    action: "clear".into(),
                    label: Some(target.name.clone()),
                    success: false,
                    error: Some(error),
                });
                continue;
            }
        }
        let observation = session.native_observe().await?;
        let targets = scoped_targets(&observation, task).unwrap_or_default();
        let target = match target_for_name(&targets, name) {
            Ok(target) => target,
            Err(error) => {
                fields.push(FillFieldResult {
                    target: name.clone(),
                    action: action_name(action).into(),
                    label: None,
                    success: false,
                    error: Some(error),
                });
                continue;
            }
        };
        let action_result = invoke_native_action(
            session,
            target,
            task_region_for_target(&observation, target),
            action,
            value_for_action.as_deref(),
            observation.revision,
            task.limits.timeout_ms,
        )
        .await;
        match action_result {
            Ok(outcome) => {
                let success = outcome.execution.status
                    == crate::browser::session::SemanticIntentExecutionStatus::Executed;
                if success {
                    filled += 1;
                    last_execution_id = outcome.execution.execution_id.clone();
                    current_revision = outcome
                        .execution
                        .action
                        .as_ref()
                        .map_or(observation.revision, |action| action.current_revision);
                }
                fields.push(FillFieldResult {
                    target: name.clone(),
                    action: action_name(action).into(),
                    label: Some(target.name.clone()),
                    success,
                    error: (!success).then(|| "native action was not executed".into()),
                });
            }
            Err(error) => fields.push(FillFieldResult {
                target: name.clone(),
                action: action_name(action).into(),
                label: Some(target.name.clone()),
                success: false,
                error: Some(error),
            }),
        }
    }
    let after = session.native_observe().await?;
    let values = read_control_values(session).await?;
    let verified = filled == task.inputs.len()
        && task.inputs.iter().all(|(name, expected)| {
            target_for_name(&scoped_targets(&after, task).unwrap_or_default(), name)
                .ok()
                .and_then(|target| control_value_matches(&values, target, expected))
                .unwrap_or(false)
        });
    let form = FillFormOutcome {
        status: if verified {
            ActionStatus::Succeeded
        } else {
            ActionStatus::CompletedWithVerificationFailure
        },
        failure_kind: (!verified).then_some(ActionFailureKind::VerificationFailed),
        execution_id: last_execution_id.unwrap_or_else(|| format!("native_task_{revision}")),
        filled,
        total: task.inputs.len(),
        fields,
        previous_revision: revision,
        current_revision: after.revision.max(current_revision),
        verification: crate::browser::session::ActionVerificationEvidence {
            revision_delta: after.revision.saturating_sub(revision),
            url_changed: false,
            title_changed: false,
            ..Default::default()
        },
    };
    Ok(result(
        task,
        revision,
        after.revision.max(current_revision),
        &task.inputs.keys().cloned().collect::<Vec<_>>(),
        confirmation_required,
        TaskPlanOperation::FillInputs,
        if verified {
            "succeeded"
        } else {
            "indeterminate"
        },
        "mutation-verification",
        filled > 0,
        Some(form),
        None,
        None,
        Vec::new(),
        (!verified).then(|| "native form values did not match requested values".into()),
    ))
}

fn field_action(target: &SemanticTarget, value: &str) -> (SemanticIntentAction, Option<String>) {
    let input_type = target.input_type.as_deref().unwrap_or_default();
    if target.role == "checkbox" || input_type.eq_ignore_ascii_case("checkbox") {
        let checked = !value.is_empty() && !matches!(value, "false" | "0" | "off");
        return (
            if checked {
                SemanticIntentAction::Check
            } else {
                SemanticIntentAction::Uncheck
            },
            None,
        );
    }
    if target.role == "radio" || input_type.eq_ignore_ascii_case("radio") {
        return (SemanticIntentAction::Click, None);
    }
    if matches!(target.role.as_str(), "combobox" | "listbox") {
        return (SemanticIntentAction::Select, Some(value.to_owned()));
    }
    (SemanticIntentAction::Type, Some(value.to_owned()))
}

#[allow(clippy::too_many_arguments)]
async fn execute_named_click(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
    targets: &[&SemanticTarget],
    input_name: &str,
    operation: TaskPlanOperation,
    action: SemanticIntentAction,
    phase: &str,
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let name = task
        .inputs
        .get(input_name)
        .ok_or_else(|| format!("task requires the {input_name} input"))?;
    let target = match target_for_name(targets, name) {
        Ok(target) => target,
        Err(error) => {
            return Ok(preflight_result(
                task,
                revision,
                targets,
                confirmation_required,
                error,
            ));
        }
    };
    let observation = session.native_observe().await?;
    let outcome = invoke_native_action(
        session,
        target,
        task_region_for_target(&observation, target),
        action,
        None,
        observation.revision,
        task.limits.timeout_ms,
    )
    .await;
    let (status, detail, mutation_possible, current_revision) = match outcome {
        Ok(outcome)
            if outcome.execution.status
                == crate::browser::session::SemanticIntentExecutionStatus::Executed =>
        {
            (
                "succeeded",
                None,
                true,
                outcome
                    .execution
                    .action
                    .as_ref()
                    .map_or(observation.revision, |action| action.current_revision),
            )
        }
        Ok(_) => (
            "preflight-failed",
            Some("native action was not executed".into()),
            false,
            observation.revision,
        ),
        Err(error) => ("preflight-failed", Some(error), false, observation.revision),
    };
    Ok(result(
        task,
        revision,
        current_revision,
        std::slice::from_ref(&target.reference),
        confirmation_required,
        operation,
        status,
        phase,
        mutation_possible,
        None,
        None,
        None,
        Vec::new(),
        detail,
    ))
}

async fn execute_field_read(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
    targets: &[&SemanticTarget],
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let name = task
        .inputs
        .get("field")
        .ok_or("field.read requires the field input")?;
    let target = match target_for_name(targets, name) {
        Ok(target) => target,
        Err(error) => {
            return Ok(preflight_result(
                task,
                revision,
                targets,
                confirmation_required,
                error,
            ));
        }
    };
    let observation = session.native_observe().await?;
    let values = read_control_values(session).await?;
    let Some(record) = control_record(&values, target) else {
        return Ok(preflight_result(
            task,
            revision,
            targets,
            confirmation_required,
            "native field was not present in the value projection",
        ));
    };
    let record = json!({
        "field": target.name,
        "reference": target.reference,
        "role": target.role,
        "inputType": target.input_type,
        "value": record.get("value").cloned().unwrap_or(Value::Null),
        "checked": record.get("checked").cloned().unwrap_or(Value::Null),
        "selectedOption": record.get("selectedOption").cloned().unwrap_or(Value::Null),
        "empty": target.empty,
        "readOnly": target.read_only,
        "required": target.required,
    });
    let extraction = StructuredExtractionResult {
        source_revision: observation.revision,
        source_route: observation.route.clone(),
        records: vec![record.clone()],
        record_items: Vec::new(),
        truncated: false,
        provenance: vec!["native.document.controls".into()],
        field_provenance: vec![StructuredExtractionProvenance {
            field: "field".into(),
            path: "$.native.document.controls".into(),
            region_id: None,
            entity_ids: vec![target.reference.clone()],
        }],
        continuation: None,
        limits: StructuredExtractionLimits {
            max_items: task.limits.max_items as usize,
            max_bytes: NATIVE_TASK_MAX_EXTRACTION_BYTES,
            observed_items: 1,
            serialized_bytes: serde_json::to_vec(&record).map_or(0, |bytes| bytes.len()),
            truncated: false,
        },
    };
    Ok(result(
        task,
        revision,
        observation.revision,
        std::slice::from_ref(&target.reference),
        confirmation_required,
        TaskPlanOperation::ReadField,
        "succeeded",
        "field-read",
        false,
        None,
        Some(extraction),
        None,
        Vec::new(),
        None,
    ))
}

async fn execute_extraction(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
    operation: TaskPlanOperation,
    shape: &str,
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let observation = session.native_observe().await?;
    let extraction = native_extraction(session, &observation, task, shape).await?;
    Ok(result(
        task,
        revision,
        extraction.source_revision,
        &[format!("native-region:{revision}")],
        confirmation_required,
        operation,
        "succeeded",
        "extraction",
        false,
        None,
        Some(extraction),
        None,
        Vec::new(),
        None,
    ))
}

async fn execute_pagination_collect(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    revision: u64,
    confirmation_required: bool,
    initial_targets: &[&SemanticTarget],
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let mut current_revision = revision;
    let mut all_records = Vec::new();
    let mut source_route = None;
    let mut truncated = false;
    let mut page_count = 0u32;
    let max_pages = task.limits.max_actions.max(1);
    while page_count < max_pages && all_records.len() < task.limits.max_items as usize {
        let observation = session.native_observe().await?;
        let extraction = native_extraction(session, &observation, task, "collection").await?;
        source_route = Some(extraction.source_route.clone());
        truncated |= extraction.truncated;
        for record in extraction.records {
            if all_records.len() >= task.limits.max_items as usize {
                truncated = true;
                break;
            }
            all_records.push(record);
        }
        current_revision = observation.revision;
        page_count += 1;
        if page_count >= max_pages || all_records.len() >= task.limits.max_items as usize {
            break;
        }
        let next_name = task
            .inputs
            .get("next")
            .ok_or("pagination.collect requires the next input")?;
        let targets = if observation.revision == revision && page_count == 1 {
            initial_targets.to_vec()
        } else {
            scoped_targets(&observation, task).unwrap_or_default()
        };
        let Ok(target) = target_for_name(&targets, next_name) else {
            break;
        };
        let outcome = invoke_native_action(
            session,
            target,
            task_region_for_target(&observation, target),
            SemanticIntentAction::Paginate,
            None,
            observation.revision,
            task.limits.timeout_ms,
        )
        .await;
        let Ok(outcome) = outcome else { break };
        if outcome.execution.status
            != crate::browser::session::SemanticIntentExecutionStatus::Executed
        {
            break;
        }
        current_revision = outcome
            .execution
            .action
            .as_ref()
            .map_or(current_revision, |action| action.current_revision);
    }
    let route = source_route.unwrap_or_else(|| {
        // The loop always observes before attempting extraction; this branch
        // is only defensive for a zero-sized caller limit rejected upstream.
        SemanticObservation {
            schema_version: crate::browser::session::SEMANTIC_OBSERVATION_SCHEMA_VERSION,
            revision,
            level: crate::browser::session::SemanticObservationLevel::Structured,
            route: crate::browser::session::SemanticRouteIdentity {
                target_id: "native".into(),
                frame_id: "native-root".into(),
                url: String::new(),
            },
            page: crate::browser::session::SemanticPage {
                kind: crate::browser::session::SemanticPageKind::Generic,
                title: String::new(),
                url: String::new(),
                target_id: "native".into(),
                frame_id: "native-root".into(),
                confidence: crate::browser::session::SemanticConfidence::Unknown,
                evidence: Vec::new(),
            },
            regions: Vec::new(),
            text: None,
            accessibility: None,
            raw_accessibility: None,
            changes: None,
            limits: Default::default(),
        }
        .route
    });
    let records = all_records;
    let observed_items = records.len();
    let record_items = records
        .iter()
        .enumerate()
        .map(|(index, value)| StructuredExtractionRecord {
            field: "items".into(),
            index,
            value: value.clone(),
            entity_ids: vec![format!("native-record-{index}")],
        })
        .collect::<Vec<_>>();
    let extraction = StructuredExtractionResult {
        source_revision: current_revision,
        source_route: route,
        records,
        record_items,
        truncated,
        provenance: vec!["native.document.pagination".into()],
        field_provenance: Vec::new(),
        continuation: None,
        limits: StructuredExtractionLimits {
            max_items: task.limits.max_items as usize,
            max_bytes: NATIVE_TASK_MAX_EXTRACTION_BYTES,
            observed_items,
            serialized_bytes: 0,
            truncated,
        },
    };
    let extraction = with_serialized_limit(extraction, task.limits.max_items as usize);
    Ok(result(
        task,
        revision,
        current_revision,
        &[format!("native-pagination:{revision}")],
        confirmation_required,
        TaskPlanOperation::CollectPages,
        "succeeded",
        "pagination-extraction",
        true,
        None,
        Some(extraction),
        None,
        Vec::new(),
        None,
    ))
}

async fn native_extraction(
    session: &BrowserRuntimeSession,
    observation: &SemanticObservation,
    task: &GlassTask,
    shape: &str,
) -> crate::browser::session::BrowserResult<StructuredExtractionResult> {
    let max_items = task.limits.max_items as usize;
    let script = format!(
        r#"(() => {{
            const text = (node) => String((node && (node.innerText || node.textContent)) || '').trim();
            const shape = {shape:?};
            let values = [];
            if (shape === 'table') {{
                const tables = Array.from(document.querySelectorAll('table'));
                const table = tables.length ? tables[0] : null;
                values = table ? Array.from(table.querySelectorAll('tr')).slice(0, {max_items}).map((row) =>
                    Array.from(row.querySelectorAll('th,td')).map((cell) => text(cell))) : [];
            }} else if (shape === 'collection') {{
                const roots = Array.from(document.querySelectorAll('[role="list"],ul,ol'));
                const root = roots.length ? roots[0] : null;
                values = root ? Array.from(root.querySelectorAll('[role="listitem"],li')).slice(0, {max_items}).map((item) => ({{ text: text(item) }})) : [];
            }} else {{
                const root = document.querySelector('main,[role="main"],form,section,article,div,body') || document.documentElement || document;
                values = root ? [{{ text: text(root) }}] : [];
            }}
            return values;
        }})()"#,
    );
    let value = session.script(script).await?.value;
    let mut records = value.as_array().cloned().unwrap_or_default();
    let mut truncated = records.len() > max_items;
    records.truncate(max_items);
    let mut result = StructuredExtractionResult {
        source_revision: observation.revision,
        source_route: observation.route.clone(),
        records,
        record_items: Vec::new(),
        truncated,
        provenance: vec![format!("native.document.{shape}")],
        field_provenance: vec![StructuredExtractionProvenance {
            field: if shape == "table" { "rows" } else { "items" }.into(),
            path: format!("$.native.document.{shape}"),
            region_id: None,
            entity_ids: Vec::new(),
        }],
        continuation: None,
        limits: StructuredExtractionLimits {
            max_items,
            max_bytes: NATIVE_TASK_MAX_EXTRACTION_BYTES,
            observed_items: 0,
            serialized_bytes: 0,
            truncated: false,
        },
    };
    result.record_items = result
        .records
        .iter()
        .enumerate()
        .map(|(index, value)| StructuredExtractionRecord {
            field: if shape == "table" { "row" } else { "item" }.into(),
            index,
            value: value.clone(),
            entity_ids: vec![format!("native-record-{index}")],
        })
        .collect();
    let serialized = serde_json::to_vec(&result).unwrap_or_default();
    if serialized.len() > NATIVE_TASK_MAX_EXTRACTION_BYTES {
        truncated = true;
        while !result.record_items.is_empty()
            && serde_json::to_vec(&result)
                .is_ok_and(|bytes| bytes.len() > NATIVE_TASK_MAX_EXTRACTION_BYTES)
        {
            result.record_items.pop();
            result.records.pop();
        }
    }
    result.truncated = truncated;
    result.limits.observed_items = result.records.len();
    result.limits.serialized_bytes = serde_json::to_vec(&result).map_or(0, |bytes| bytes.len());
    result.limits.truncated = result.truncated;
    Ok(result)
}

fn with_serialized_limit(
    mut result: StructuredExtractionResult,
    max_items: usize,
) -> StructuredExtractionResult {
    result.records.truncate(max_items);
    result.record_items.truncate(max_items);
    result.limits.observed_items = result.records.len();
    result.limits.serialized_bytes = serde_json::to_vec(&result).map_or(0, |bytes| bytes.len());
    result
}

async fn native_form_valid(
    session: &BrowserRuntimeSession,
) -> crate::browser::session::BrowserResult<bool> {
    let value = session
        .script(
            "(() => Array.from(document.querySelectorAll('input,textarea,select')).every((control) => typeof control.checkValidity !== 'function' || control.checkValidity()))()",
        )
        .await?
        .value;
    Ok(value.as_bool().unwrap_or(false))
}

async fn read_control_values(
    session: &BrowserRuntimeSession,
) -> crate::browser::session::BrowserResult<Vec<Value>> {
    let value = session
        .script(
            &format!(
                r#"(() => {{
                    const label = (control) => {{
                        const id = control.getAttribute('id');
                        if (id) {{
                            const labels = Array.from(document.querySelectorAll('label'));
                            for (const candidate of labels) {{
                                if (candidate.getAttribute('for') === id) return String(candidate.innerText || candidate.textContent || '').trim();
                            }}
                        }}
                        const parent = control.parentElement;
                        return parent && parent.tagName === 'LABEL' ? String(parent.innerText || parent.textContent || '').trim() : '';
                    }};
                    return Array.from(document.querySelectorAll('input,textarea,select')).slice(0, {NATIVE_TASK_MAX_CONTROLS}).map((control) => {{
                        const aria = control.getAttribute('aria-label') || '';
                        const semanticName = aria || label(control) || control.getAttribute('name') || control.getAttribute('id') || control.getAttribute('placeholder') || '';
                        let selectedOption = null;
                        if (control.tagName === 'SELECT' && control.selectedIndex >= 0 && control.options[control.selectedIndex]) selectedOption = String(control.options[control.selectedIndex].text);
                        return {{ name: semanticName, value: control.value === undefined ? null : String(control.value), checked: control.checked === undefined ? null : Boolean(control.checked), selectedOption }};
                    }});
                }})()"#
            ),
        )
        .await?
        .value;
    Ok(value.as_array().cloned().unwrap_or_default())
}

fn control_record<'a>(
    values: &'a [Value],
    target: &SemanticTarget,
) -> Option<&'a Map<String, Value>> {
    values.iter().find_map(|value| {
        let object = value.as_object()?;
        object
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| name.eq_ignore_ascii_case(&target.name))
            .map(|_| object)
    })
}

fn control_value_matches(
    values: &[Value],
    target: &SemanticTarget,
    expected: &str,
) -> Option<bool> {
    let record = control_record(values, target)?;
    if matches!(target.role.as_str(), "checkbox" | "radio")
        || target
            .input_type
            .as_deref()
            .is_some_and(|kind| matches!(kind, "checkbox" | "radio"))
    {
        let wanted = !expected.is_empty() && !matches!(expected, "false" | "0" | "off");
        return Some(record.get("checked").and_then(Value::as_bool) == Some(wanted));
    }
    if matches!(target.role.as_str(), "combobox" | "listbox") {
        return Some(
            record.get("value").and_then(Value::as_str) == Some(expected)
                || record.get("selectedOption").and_then(Value::as_str) == Some(expected),
        );
    }
    Some(record.get("value").and_then(Value::as_str) == Some(expected))
}

async fn invoke_native_action(
    session: &BrowserRuntimeSession,
    target: &SemanticTarget,
    region: &SemanticRegion,
    action: SemanticIntentAction,
    value: Option<&str>,
    revision: u64,
    timeout_ms: u64,
) -> Result<ActAndVerifyResult, String> {
    let preflight_action = match action {
        SemanticIntentAction::Type | SemanticIntentAction::Clear => {
            crate::browser::native_engine::NativePreflightAction::Type
        }
        SemanticIntentAction::Check | SemanticIntentAction::Uncheck => {
            crate::browser::native_engine::NativePreflightAction::Check
        }
        SemanticIntentAction::Select => {
            crate::browser::native_engine::NativePreflightAction::Select
        }
        _ => crate::browser::native_engine::NativePreflightAction::Click,
    };
    let preflight = session
        .native_preflight_target(&target.reference, preflight_action)
        .await
        .map_err(|error| error.to_string())?;
    if !preflight.unique {
        return Err(format!(
            "native target did not resolve uniquely: {:?}",
            preflight.error_kind
        ));
    }
    if preflight.actionable != Some(true) {
        return Err(format!(
            "native target is not actionable: {:?}",
            preflight.actionability_reason
        ));
    }
    let request = SemanticIntentRequest {
        schema_version: crate::browser::session::INTENT_RESOLUTION_SCHEMA_VERSION,
        intent: format!(
            "{} {}",
            action_name(action),
            if target.name.is_empty() {
                "target"
            } else {
                &target.name
            }
        ),
        action,
        scope: IntentScope {
            region_kind: Some(region.kind),
            region_id: Some(region.id.clone()),
            ..Default::default()
        },
        constraints: IntentConstraints {
            role: Some(target.role.clone()),
            name: (!target.name.is_empty()).then(|| target.name.clone()),
            ..Default::default()
        },
        resolution_policy:
            crate::browser::session::SemanticResolutionPolicy::RequireUniqueHighConfidence,
        expected_revision: Some(revision),
    };
    let found = session
        .native_find_target(&request)
        .await
        .map_err(|error| error.to_string())?;
    let candidate = found
        .candidates
        .iter()
        .find(|candidate| candidate.reference == target.reference)
        .ok_or_else(|| {
            format!(
                "native target disappeared during task resolution: {}",
                target.name
            )
        })?;
    let execution = SemanticIntentExecutionRequest {
        request,
        candidate_id: candidate.id.clone(),
        value: value.map(str::to_owned),
    };
    session
        .native_act_and_verify(&execution, None, Duration::from_millis(timeout_ms))
        .await
        .map_err(|error| error.to_string())
}

fn task_region_for_target<'a>(
    observation: &'a SemanticObservation,
    target: &SemanticTarget,
) -> &'a SemanticRegion {
    observation
        .regions
        .iter()
        .find(|region| {
            region
                .targets
                .iter()
                .any(|candidate| candidate.reference == target.reference)
        })
        .unwrap_or_else(|| &observation.regions[0])
}

fn action_name(action: SemanticIntentAction) -> &'static str {
    match action {
        SemanticIntentAction::Click => "click",
        SemanticIntentAction::Type => "type",
        SemanticIntentAction::Clear => "clear",
        SemanticIntentAction::Check => "check",
        SemanticIntentAction::Uncheck => "uncheck",
        SemanticIntentAction::Select => "select",
        SemanticIntentAction::Submit => "submit",
        SemanticIntentAction::Open => "open",
        SemanticIntentAction::Close => "close",
        SemanticIntentAction::Search => "search",
        SemanticIntentAction::Filter => "filter",
        SemanticIntentAction::Sort => "sort",
        SemanticIntentAction::Paginate => "paginate",
        SemanticIntentAction::Toggle => "toggle",
        SemanticIntentAction::Expand => "expand",
        SemanticIntentAction::Collapse => "collapse",
        SemanticIntentAction::Download => "download",
        SemanticIntentAction::Upload => "upload",
        SemanticIntentAction::Inspect => "inspect",
        SemanticIntentAction::Extract => "extract",
    }
}

async fn finalize(
    session: &BrowserRuntimeSession,
    task: &GlassTask,
    mut result: TaskExecutionResult,
) -> crate::browser::session::BrowserResult<TaskExecutionResult> {
    let observation = session.native_observe().await?;
    let mut postconditions = Vec::new();
    for postcondition in effective_postconditions(task) {
        let held = postcondition_holds(session, &postcondition, &observation, &result).await?;
        postconditions.push(TaskPostconditionReceipt {
            kind: postcondition.kind,
            held,
        });
    }
    result.receipt.postconditions = postconditions;
    result.current_revision = result.current_revision.max(observation.revision);
    if result
        .receipt
        .postconditions
        .iter()
        .any(|postcondition| !postcondition.held)
    {
        result.status = "indeterminate".into();
        result.phase = "postcondition-verification".into();
        result.retry = retry(RetryClassification::UnsafeUntilReconciled, "recover_run");
        if let Some(last) = result.steps.last_mut() {
            last.status = "indeterminate".into();
            last.detail = Some("native task postcondition did not hold".into());
        }
    }
    Ok(result)
}

fn effective_postconditions(task: &GlassTask) -> Vec<TaskPostcondition> {
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
        _ => TaskPostconditionKind::PageKind,
    };
    vec![TaskPostcondition {
        kind,
        expected: None,
    }]
}

async fn postcondition_holds(
    session: &BrowserRuntimeSession,
    postcondition: &TaskPostcondition,
    observation: &SemanticObservation,
    result: &TaskExecutionResult,
) -> crate::browser::session::BrowserResult<bool> {
    Ok(match postcondition.kind {
        TaskPostconditionKind::PageKind => {
            postcondition.expected.as_deref().is_none_or(|expected| {
                page_kind_name(observation.page.kind).eq_ignore_ascii_case(expected)
            })
        }
        TaskPostconditionKind::RegionPresent => {
            postcondition.expected.as_deref().is_some_and(|expected| {
                observation
                    .regions
                    .iter()
                    .any(|region| region.label.eq_ignore_ascii_case(expected))
            })
        }
        TaskPostconditionKind::NavigationOccurred => {
            observation.revision > result.source_revision
                || result.steps.last().is_some_and(|step| {
                    step.status == "succeeded" && result.current_revision > result.source_revision
                })
        }
        TaskPostconditionKind::DialogClosed => session.native_pending_dialog().await?.is_none(),
        TaskPostconditionKind::ValidationClear => native_form_valid(session).await?,
        TaskPostconditionKind::RecordsExtracted => {
            let count = result
                .extraction
                .as_ref()
                .map_or(0, |extraction| extraction.records.len());
            postcondition
                .expected
                .as_deref()
                .map_or(count > 0, |expected| {
                    expected
                        .parse::<usize>()
                        .is_ok_and(|minimum| count >= minimum)
                })
        }
        TaskPostconditionKind::EntityState => postcondition
            .expected
            .as_deref()
            .and_then(parse_entity_state)
            .is_some_and(|(name, state, expected)| {
                observation
                    .regions
                    .iter()
                    .flat_map(|region| region.targets.iter())
                    .find(|target| target.name.eq_ignore_ascii_case(name))
                    .and_then(|target| target_state(target, state))
                    == Some(expected)
            }),
    })
}

fn parse_entity_state(value: &str) -> Option<(&str, &str, bool)> {
    let (selector, expected) = value.rsplit_once('=')?;
    let (name, state) = selector.rsplit_once('.')?;
    Some((name, state, expected == "true"))
}

fn target_state(target: &SemanticTarget, state: &str) -> Option<bool> {
    match state {
        "disabled" => target.disabled,
        "readOnly" => target.read_only,
        "required" => target.required,
        "checked" => target.checked,
        "empty" => target.empty,
        _ => None,
    }
}

fn page_kind_name(kind: crate::browser::session::SemanticPageKind) -> &'static str {
    match kind {
        crate::browser::session::SemanticPageKind::Generic => "generic",
        crate::browser::session::SemanticPageKind::Home => "home",
        crate::browser::session::SemanticPageKind::Search => "search",
        crate::browser::session::SemanticPageKind::SearchResults => "searchResults",
        crate::browser::session::SemanticPageKind::Article => "article",
        crate::browser::session::SemanticPageKind::Documentation => "documentation",
        crate::browser::session::SemanticPageKind::Listing => "listing",
        crate::browser::session::SemanticPageKind::Detail => "detail",
        crate::browser::session::SemanticPageKind::Form => "form",
        crate::browser::session::SemanticPageKind::Authentication => "authentication",
        crate::browser::session::SemanticPageKind::Checkout => "checkout",
        crate::browser::session::SemanticPageKind::Confirmation => "confirmation",
        crate::browser::session::SemanticPageKind::Dashboard => "dashboard",
        crate::browser::session::SemanticPageKind::Settings => "settings",
        crate::browser::session::SemanticPageKind::Error => "error",
        crate::browser::session::SemanticPageKind::AccessDenied => "accessDenied",
        crate::browser::session::SemanticPageKind::Unknown => "unknown",
    }
}

#[allow(clippy::too_many_arguments)]
fn result(
    task: &GlassTask,
    source_revision: u64,
    current_revision: u64,
    references: &[String],
    confirmation_required: bool,
    operation: TaskPlanOperation,
    status: &str,
    phase: &str,
    mutation_possible: bool,
    form: Option<FillFormOutcome>,
    extraction: Option<StructuredExtractionResult>,
    dialog: Option<crate::browser::session::PendingDialog>,
    alerts: Vec<String>,
    detail: Option<String>,
) -> TaskExecutionResult {
    let receipt = native_receipt(task, source_revision, references, confirmation_required);
    TaskExecutionResult {
        task: task.task,
        status: status.into(),
        phase: phase.into(),
        mutation_possible,
        source_revision,
        current_revision,
        steps: vec![
            TaskStepResult {
                ordinal: 1,
                operation: TaskPlanOperation::ObserveScope,
                status: "succeeded".into(),
                detail: None,
            },
            TaskStepResult {
                ordinal: 2,
                operation,
                status: status.into(),
                detail,
            },
        ],
        retry: if status == "succeeded" {
            retry(RetryClassification::SafeImmediate, "inspect_page")
        } else if mutation_possible {
            retry(RetryClassification::UnsafeUntilReconciled, "recover_run")
        } else {
            retry(RetryClassification::SafeAfterReobserve, "inspect_page")
        },
        receipt,
        form,
        extraction,
        dialog,
        alerts,
    }
}

fn preflight_result(
    task: &GlassTask,
    revision: u64,
    references: &[&SemanticTarget],
    confirmation_required: bool,
    detail: impl Into<String>,
) -> TaskExecutionResult {
    let reference_values = references
        .iter()
        .map(|target| target.reference.clone())
        .collect::<Vec<_>>();
    let operation = operation_for_task(task.task);
    let mut result = result(
        task,
        revision,
        revision,
        &reference_values,
        confirmation_required,
        operation,
        "preflight-failed",
        "preflight",
        false,
        None,
        None,
        None,
        Vec::new(),
        Some(detail.into()),
    );
    result.steps[0].status = "not-run".into();
    result.steps[1].status = "not-run".into();
    result
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

fn native_receipt(
    task: &GlassTask,
    revision: u64,
    references: &[String],
    confirmation_required: bool,
) -> TaskExecutionReceipt {
    let selected_entity_ids = if references.is_empty() {
        vec![format!(
            "native-page-{}",
            digest_id(&format!("{}:{revision}", task.task as u8))
        )]
    } else {
        references
            .iter()
            .map(|reference| digest_id(reference))
            .collect()
    };
    let mut capabilities = BTreeSet::from([TaskRuntimeCapability::Observe]);
    match task.task {
        TaskKind::NavigationFollow => {
            capabilities.insert(TaskRuntimeCapability::Navigate);
        }
        TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => {
            capabilities.insert(TaskRuntimeCapability::Dialog);
        }
        TaskKind::TableExtract | TaskKind::CollectionExtract | TaskKind::RegionExtract => {
            capabilities.insert(TaskRuntimeCapability::Extract);
        }
        TaskKind::PaginationNext | TaskKind::PaginationCollect => {
            capabilities.insert(TaskRuntimeCapability::Pagination);
            capabilities.insert(TaskRuntimeCapability::Extract);
        }
        TaskKind::FormFill
        | TaskKind::FormSubmit
        | TaskKind::NavigationSelectTab
        | TaskKind::NavigationOpenMenu => {
            capabilities.insert(TaskRuntimeCapability::Mutate);
        }
        TaskKind::FormInspect | TaskKind::FormValidate | TaskKind::FieldRead => {
            capabilities.insert(TaskRuntimeCapability::Read);
        }
    }
    if task
        .postconditions
        .iter()
        .any(|postcondition| postcondition.kind == TaskPostconditionKind::EntityState)
    {
        capabilities.insert(TaskRuntimeCapability::VerifyEntityState);
    }
    let required_runtime_capabilities = capabilities.into_iter().collect::<Vec<_>>();
    let evidence_requirements = selected_entity_ids
        .iter()
        .map(|entity_id| TaskEntityEvidenceRequirement {
            entity_id: entity_id.clone(),
            minimum_quality: EvidenceQuality::Strong,
            required_sources: vec![EvidenceSource::BrowserNative],
        })
        .collect::<Vec<_>>();
    TaskExecutionReceipt {
        source_revision: revision,
        selected_entity_ids: selected_entity_ids.clone(),
        binding_candidate_entity_ids: selected_entity_ids,
        required_runtime_capabilities,
        evidence_requirements,
        confirmation_required,
        postconditions: Vec::new(),
    }
}

fn digest_id(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let encoded = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("native-{encoded}")
}

fn retry(classification: RetryClassification, operation: &str) -> RetryGuidance {
    RetryGuidance {
        classification,
        recommended_operation: operation.into(),
    }
}

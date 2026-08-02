//! Browser-backed execution for the bounded Task Protocol families.

use super::{
    BrowserResult, BrowserSession, ExtractionField, ExtractionKind, FillFormOutcome,
    InspectPageResult, PendingDialog, SemanticObservationLevel, SemanticRegion, SemanticRegionKind,
    SemanticTarget, StructuredExtractionRequest, StructuredExtractionResult,
};
use crate::protocol::{RetryClassification, RetryGuidance};
use crate::task_compiler::{TaskExecutionPlan, TaskPlanOperation, compile_task};
use crate::task_protocol::{GlassTask, TaskKind, TaskPostconditionKind};
use serde::Serialize;
use serde_json::json;
use std::future::Future;
use std::io::{Error as IoError, ErrorKind};
use std::time::Duration;

/// A bounded result for one browser-backed Task Protocol execution.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionResult {
    pub task: TaskKind,
    pub status: String,
    pub phase: String,
    pub mutation_possible: bool,
    pub source_revision: u64,
    pub current_revision: u64,
    pub steps: Vec<TaskStepResult>,
    pub retry: RetryGuidance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<FillFormOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction: Option<StructuredExtractionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog: Option<PendingDialog>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<String>,
}

/// Outcome of one plan step without exposing authored input values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStepResult {
    pub ordinal: u16,
    pub operation: TaskPlanOperation,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl BrowserSession {
    /// Execute a validated form task against one caller-observed revision.
    ///
    /// `expected_revision` is supplied by the caller's preceding semantic
    /// observation. The runtime always re-observes before mutation, resolves
    /// targets from that observation, and passes the resulting revision into
    /// the guarded action APIs.
    pub async fn execute_form_task(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let plan = compile_task(task).map_err(|error| error.to_string())?;
        if !matches!(
            task.task,
            TaskKind::FormInspect
                | TaskKind::FormFill
                | TaskKind::FormValidate
                | TaskKind::FormSubmit
                | TaskKind::FieldRead
                | TaskKind::NavigationSelectTab
                | TaskKind::PaginationNext
                | TaskKind::PaginationCollect
                | TaskKind::TableExtract
                | TaskKind::CollectionExtract
                | TaskKind::RegionExtract
        ) {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "unsupported task family; browser execution currently supports form, field, table, collection, region extraction, and pagination tasks",
            ));
        }
        if plan.confirmation_required && !confirmed {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "confirmation is required before this task can mutate the browser",
            ));
        }

        let current_revision = self
            .page_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        if current_revision != expected_revision {
            return Ok(preflight_result(
                task,
                &plan,
                current_revision,
                "source revision is stale; no browser mutation was dispatched",
            ));
        }
        let observation = bounded(self.inspect_page(), task.limits.timeout_ms).await?;
        if observation.revision != expected_revision {
            return Ok(preflight_result(
                task,
                &plan,
                observation.revision,
                "source revision changed during preflight observation; no browser mutation was dispatched",
            ));
        }

        let scoped_regions = match scoped_regions_for_observation(&observation, task) {
            Ok(regions) => regions,
            Err(error) => {
                return Ok(preflight_result(
                    task,
                    &plan,
                    observation.revision,
                    &error.to_string(),
                ));
            }
        };
        let mut steps = vec![step(
            &plan,
            TaskPlanOperation::ObserveScope,
            "succeeded",
            None,
        )];

        match task.task {
            TaskKind::NavigationSelectTab => {
                let Some(tab_name) = task.inputs.get("tab") else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "navigation.selectTab requires the semantic tab input",
                    ));
                };
                let target = match unique_target(&scoped_regions, tab_name) {
                    Ok(target) if target.role.eq_ignore_ascii_case("tab") => target,
                    Ok(_) => {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            observation.revision,
                            "navigation.selectTab target is not a semantic tab",
                        ));
                    }
                    Err(error) => {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            observation.revision,
                            &error.to_string(),
                        ));
                    }
                };
                let outcome = bounded(
                    self.click_with_revision(&target.reference, observation.revision),
                    task.limits.timeout_ms,
                )
                .await;
                let after = bounded(self.inspect_page(), task.limits.timeout_ms).await?;
                let succeeded = outcome.is_ok();
                steps.push(step(
                    &plan,
                    TaskPlanOperation::SelectTab,
                    if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    },
                    (!succeeded).then(|| "tab selection outcome was not verified".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "navigation-verification".into(),
                    mutation_possible: true,
                    source_revision: observation.revision,
                    current_revision: after.revision,
                    steps,
                    retry: if succeeded {
                        retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
                    } else {
                        retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run")
                    },
                    form: None,
                    extraction: None,
                    dialog: None,
                    alerts: alert_labels(after.regions.iter()),
                })
            }
            TaskKind::PaginationCollect => {
                let Some(next_name) = task.inputs.get("next") else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "pagination.collect requires the semantic next control input",
                    ));
                };
                let max_pages = task.limits.max_items.min(task.limits.max_actions).max(1) as usize;
                let source_revision = observation.revision;
                let mut current = observation;
                let mut completed = 0usize;
                let mut stopped = false;
                while completed < max_pages {
                    let regions = scoped_regions_for_observation(&current, task)?;
                    let region = regions
                        .first()
                        .expect("scoped_regions contains exactly one region");
                    let candidates = region
                        .targets
                        .iter()
                        .filter(|target| target.name.eq_ignore_ascii_case(next_name))
                        .collect::<Vec<_>>();
                    if candidates.is_empty() {
                        stopped = true;
                        break;
                    }
                    if candidates.len() > 1 {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            current.revision,
                            "pagination.collect next control is ambiguous",
                        ));
                    }
                    let target = candidates[0];
                    if !matches!(target.role.as_str(), "button" | "link" | "tab") {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            current.revision,
                            "pagination.collect target is not a semantic navigation control",
                        ));
                    }
                    let before_revision = current.revision;
                    let outcome = bounded(
                        self.click_with_revision(&target.reference, before_revision),
                        task.limits.timeout_ms,
                    )
                    .await;
                    let after = bounded(self.inspect_page(), task.limits.timeout_ms).await?;
                    let succeeded = outcome.is_ok();
                    steps.push(step(
                        &plan,
                        TaskPlanOperation::CollectPages,
                        if succeeded {
                            "succeeded"
                        } else {
                            "indeterminate"
                        },
                        (!succeeded).then(|| "pagination outcome was not verified".into()),
                    ));
                    if !succeeded {
                        return Ok(TaskExecutionResult {
                            task: task.task,
                            status: "indeterminate".into(),
                            phase: "pagination-collection".into(),
                            mutation_possible: true,
                            source_revision,
                            current_revision: after.revision,
                            steps,
                            retry: retry_guidance(
                                RetryClassification::UnsafeUntilReconciled,
                                "recover_run",
                            ),
                            form: None,
                            extraction: None,
                            dialog: None,
                            alerts: alert_labels(after.regions.iter()),
                        });
                    }
                    completed += 1;
                    if !semantic_page_changed(&current, &after) {
                        stopped = true;
                        current = after;
                        break;
                    }
                    current = after;
                }
                let mut alerts = alert_labels(current.regions.iter());
                if !stopped && completed == max_pages {
                    alerts.push("pagination-limit-reached".into());
                }
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "succeeded".into(),
                    phase: "pagination-collection".into(),
                    mutation_possible: completed > 0,
                    source_revision,
                    current_revision: current.revision,
                    steps,
                    retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                    form: None,
                    extraction: None,
                    dialog: None,
                    alerts,
                })
            }
            TaskKind::PaginationNext => {
                let Some(next_name) = task.inputs.get("next") else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "pagination.next requires the semantic next control input",
                    ));
                };
                let target = match unique_target(&scoped_regions, next_name) {
                    Ok(target) if matches!(target.role.as_str(), "button" | "link" | "tab") => {
                        target
                    }
                    Ok(_) => {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            observation.revision,
                            "pagination.next target is not a semantic navigation control",
                        ));
                    }
                    Err(error) => {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            observation.revision,
                            &error.to_string(),
                        ));
                    }
                };
                let outcome = bounded(
                    self.click_with_revision(&target.reference, observation.revision),
                    task.limits.timeout_ms,
                )
                .await;
                let after = bounded(self.inspect_page(), task.limits.timeout_ms).await?;
                let succeeded = outcome.is_ok();
                steps.push(step(
                    &plan,
                    TaskPlanOperation::NextPage,
                    if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    },
                    (!succeeded).then(|| "pagination outcome was not verified".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "pagination-verification".into(),
                    mutation_possible: true,
                    source_revision: observation.revision,
                    current_revision: after.revision,
                    steps,
                    retry: if succeeded {
                        retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
                    } else {
                        retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run")
                    },
                    form: None,
                    extraction: None,
                    dialog: None,
                    alerts: alert_labels(after.regions.iter()),
                })
            }
            TaskKind::FieldRead => {
                let Some(field_name) = task.inputs.get("field") else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "field.read requires the semantic field name in inputs.field",
                    ));
                };
                let target = match unique_target(&scoped_regions, field_name) {
                    Ok(target) => target,
                    Err(error) => {
                        let detail = error.to_string();
                        return Ok(preflight_result(task, &plan, observation.revision, &detail));
                    }
                };
                let semantic = bounded(
                    self.semantic_observe(SemanticObservationLevel::Structured),
                    task.limits.timeout_ms,
                )
                .await?;
                if semantic.revision != expected_revision {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        semantic.revision,
                        "source revision changed during field read preflight",
                    ));
                }
                let values =
                    bounded(self.observe_with_form_values(), task.limits.timeout_ms).await?;
                let values_revision = self
                    .page_revision
                    .load(std::sync::atomic::Ordering::Relaxed);
                if values_revision != expected_revision {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        values_revision,
                        "source revision changed while reading field values",
                    ));
                }
                let Some(control) = values
                    .accessibility
                    .interactive
                    .iter()
                    .find(|control| control.reference == target.reference)
                else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        semantic.revision,
                        "field target was not present in the bounded form-value observation",
                    ));
                };
                let record = json!({
                    "field": target.name,
                    "reference": target.reference,
                    "role": target.role,
                    "inputType": target.input_type,
                    "value": control.value,
                    "checked": control.checked,
                    "selectedOption": control.selected_option,
                    "empty": control.empty,
                    "readOnly": control.read_only,
                    "required": control.required,
                });
                steps.push(step(&plan, TaskPlanOperation::ReadField, "succeeded", None));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "succeeded".into(),
                    phase: "field-read".into(),
                    mutation_possible: false,
                    source_revision: semantic.revision,
                    current_revision: semantic.revision,
                    steps,
                    retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                    form: None,
                    extraction: Some(StructuredExtractionResult {
                        source_revision: semantic.revision,
                        source_route: semantic.route,
                        records: vec![record],
                        truncated: false,
                        provenance: vec!["$.interactive".into()],
                    }),
                    dialog: None,
                    alerts: alert_labels(scoped_regions.iter().copied()),
                })
            }
            TaskKind::TableExtract => {
                let region = scoped_regions
                    .first()
                    .expect("scoped_regions contains exactly one region");
                if region.kind != SemanticRegionKind::Table {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "table.extract scope is not a semantic table region",
                    ));
                }
                let request = StructuredExtractionRequest {
                    fields: vec![ExtractionField {
                        name: "rows".into(),
                        path: "$.targets".into(),
                        kind: ExtractionKind::Table,
                    }],
                    region_id: Some(region.id.clone()),
                    max_items: task.limits.max_items as usize,
                    max_bytes: 64 * 1024,
                };
                let extraction =
                    bounded(self.extract_structured(&request), task.limits.timeout_ms).await?;
                if !extraction_matches_observation(&extraction, &observation) {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        extraction.source_revision,
                        "source revision or route changed during table extraction",
                    ));
                }
                steps.push(step(
                    &plan,
                    TaskPlanOperation::ExtractTable,
                    "succeeded",
                    None,
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "succeeded".into(),
                    phase: "extraction".into(),
                    mutation_possible: false,
                    source_revision: observation.revision,
                    current_revision: extraction.source_revision,
                    steps,
                    retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                    form: None,
                    extraction: Some(extraction),
                    alerts: alert_labels(scoped_regions.iter().copied()),
                    dialog: None,
                })
            }
            TaskKind::CollectionExtract => {
                let region = scoped_regions
                    .first()
                    .expect("scoped_regions contains exactly one region");
                if region.kind != SemanticRegionKind::Collection {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "collection.extract scope is not a semantic collection region",
                    ));
                }
                let request = StructuredExtractionRequest {
                    fields: vec![ExtractionField {
                        name: "items".into(),
                        path: "$.targets".into(),
                        kind: ExtractionKind::RepeatedItems,
                    }],
                    region_id: Some(region.id.clone()),
                    max_items: task.limits.max_items as usize,
                    max_bytes: 64 * 1024,
                };
                let extraction =
                    bounded(self.extract_structured(&request), task.limits.timeout_ms).await?;
                if !extraction_matches_observation(&extraction, &observation) {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        extraction.source_revision,
                        "source revision or route changed during collection extraction",
                    ));
                }
                steps.push(step(
                    &plan,
                    TaskPlanOperation::ExtractCollection,
                    "succeeded",
                    None,
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "succeeded".into(),
                    phase: "extraction".into(),
                    mutation_possible: false,
                    source_revision: observation.revision,
                    current_revision: extraction.source_revision,
                    steps,
                    retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                    form: None,
                    extraction: Some(extraction),
                    alerts: alert_labels(scoped_regions.iter().copied()),
                    dialog: None,
                })
            }
            TaskKind::RegionExtract => {
                let region_id = scoped_regions
                    .first()
                    .map(|region| region.id.clone())
                    .expect("scoped_regions contains exactly one region");
                let request = StructuredExtractionRequest {
                    fields: vec![ExtractionField {
                        name: "region".into(),
                        path: "$".into(),
                        kind: ExtractionKind::Object,
                    }],
                    region_id: Some(region_id),
                    max_items: task.limits.max_items as usize,
                    max_bytes: 64 * 1024,
                };
                let extraction =
                    bounded(self.extract_structured(&request), task.limits.timeout_ms).await?;
                if !extraction_matches_observation(&extraction, &observation) {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        extraction.source_revision,
                        "source revision or route changed during region extraction",
                    ));
                }
                steps.push(step(
                    &plan,
                    TaskPlanOperation::ExtractRegion,
                    "succeeded",
                    None,
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "succeeded".into(),
                    phase: "extraction".into(),
                    mutation_possible: false,
                    source_revision: observation.revision,
                    current_revision: extraction.source_revision,
                    steps,
                    retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                    form: None,
                    extraction: Some(extraction),
                    alerts: alert_labels(scoped_regions.iter().copied()),
                    dialog: None,
                })
            }
            TaskKind::FormInspect => {
                let alerts = alert_labels(scoped_regions.iter().copied());
                steps.push(step(
                    &plan,
                    TaskPlanOperation::InspectForm,
                    "succeeded",
                    None,
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "succeeded".into(),
                    phase: "inspection".into(),
                    mutation_possible: false,
                    source_revision: observation.revision,
                    current_revision: observation.revision,
                    steps,
                    retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                    form: None,
                    extraction: None,
                    alerts,
                    dialog: None,
                })
            }
            TaskKind::FormValidate => {
                let alerts = alert_labels(scoped_regions.iter().copied());
                let valid = alerts.is_empty();
                steps.push(step(
                    &plan,
                    TaskPlanOperation::ValidateForm,
                    if valid {
                        "succeeded"
                    } else {
                        "verification-failed"
                    },
                    (!valid).then(|| "semantic alert region present".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: if valid {
                        "succeeded"
                    } else {
                        "verification-failed"
                    }
                    .into(),
                    phase: "validation".into(),
                    mutation_possible: false,
                    source_revision: observation.revision,
                    current_revision: observation.revision,
                    steps,
                    retry: if valid {
                        retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
                    } else {
                        retry_guidance(RetryClassification::RequiresUserDecision, "form.validate")
                    },
                    form: None,
                    extraction: None,
                    dialog: None,
                    alerts,
                })
            }
            TaskKind::FormFill => {
                let fields = match resolved_fields(&scoped_regions, &task.inputs) {
                    Ok(fields) => fields,
                    Err(error) => {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            observation.revision,
                            &error.to_string(),
                        ));
                    }
                };
                let borrowed = fields
                    .iter()
                    .map(|(target, value)| (target.as_str(), value.as_str()))
                    .collect::<Vec<_>>();
                let form = bounded(
                    self.fill_form_with_expected_revision(&borrowed, Some(observation.revision)),
                    task.limits.timeout_ms,
                )
                .await?;
                let after = bounded(self.inspect_page(), task.limits.timeout_ms).await?;
                let after_scoped_regions =
                    scoped_regions_for_observation(&after, task).unwrap_or_default();
                let verified = postconditions_hold(task, &after, &after_scoped_regions);
                let succeeded = verified && form.filled == form.total;
                steps.push(step(
                    &plan,
                    TaskPlanOperation::FillInputs,
                    if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    },
                    (!verified).then(|| "postcondition did not hold after mutation".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "mutation-verification".into(),
                    mutation_possible: form.filled > 0,
                    source_revision: observation.revision,
                    current_revision: after.revision,
                    steps,
                    retry: if succeeded {
                        retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
                    } else {
                        retry_guidance(RetryClassification::SafeAfterReconcile, "inspect_page")
                    },
                    form: Some(form),
                    extraction: None,
                    dialog: None,
                    alerts: alert_labels(after.regions.iter()),
                })
            }
            TaskKind::FormSubmit => {
                let Some(submit_name) = task.inputs.get("submit") else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "form.submit requires the semantic submit target in inputs.submit",
                    ));
                };
                let target = match unique_target(&scoped_regions, submit_name) {
                    Ok(target) => target,
                    Err(error) => {
                        return Ok(preflight_result(
                            task,
                            &plan,
                            observation.revision,
                            &error.to_string(),
                        ));
                    }
                };
                let outcome = bounded(
                    self.click_with_revision(&target.reference, observation.revision),
                    task.limits.timeout_ms,
                )
                .await;
                let after = bounded(self.inspect_page(), task.limits.timeout_ms).await?;
                let after_scoped_regions =
                    scoped_regions_for_observation(&after, task).unwrap_or_default();
                let verified =
                    outcome.is_ok() && postconditions_hold(task, &after, &after_scoped_regions);
                steps.push(step(
                    &plan,
                    TaskPlanOperation::SubmitForm,
                    if verified {
                        "succeeded"
                    } else {
                        "indeterminate"
                    },
                    (!verified).then(|| "submit outcome was not verified".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: if verified {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "submit-verification".into(),
                    mutation_possible: true,
                    source_revision: observation.revision,
                    current_revision: after.revision,
                    steps,
                    retry: if verified {
                        retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
                    } else {
                        retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run")
                    },
                    form: None,
                    extraction: None,
                    dialog: None,
                    alerts: alert_labels(after.regions.iter()),
                })
            }
            _ => unreachable!(),
        }
    }
}

impl BrowserSession {
    /// Execute a bounded navigation task against one caller-observed revision.
    pub async fn execute_navigation_task(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let plan = compile_task(task).map_err(|error| error.to_string())?;
        if task.task != TaskKind::NavigationFollow {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "navigation execution only supports navigation.follow tasks",
            ));
        }
        if plan.confirmation_required && !confirmed {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "confirmation is required before this task can navigate the browser",
            ));
        }
        let current_revision = self
            .page_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        if current_revision != expected_revision {
            return Ok(preflight_result(
                task,
                &plan,
                current_revision,
                "source revision is stale; no browser navigation was dispatched",
            ));
        }
        let Some(url) = task.inputs.get("url") else {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "navigation.follow requires the semantic url input",
            ));
        };
        let mut steps = vec![step(
            &plan,
            TaskPlanOperation::ObserveScope,
            "succeeded",
            None,
        )];
        match bounded(
            self.navigate_with_revision(
                url,
                Duration::from_millis(task.limits.timeout_ms),
                expected_revision,
            ),
            task.limits.timeout_ms,
        )
        .await
        {
            Ok(outcome) => {
                steps.push(step(
                    &plan,
                    TaskPlanOperation::FollowNavigation,
                    "succeeded",
                    None,
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "succeeded".into(),
                    phase: "navigation-verification".into(),
                    mutation_possible: true,
                    source_revision: expected_revision,
                    current_revision: outcome.current_revision,
                    steps,
                    retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                    form: None,
                    extraction: None,
                    alerts: Vec::new(),
                    dialog: None,
                })
            }
            Err(error) => {
                steps.push(step(
                    &plan,
                    TaskPlanOperation::FollowNavigation,
                    "indeterminate",
                    Some(error.to_string()),
                ));
                let current_revision = self
                    .page_revision
                    .load(std::sync::atomic::Ordering::Relaxed);
                Ok(TaskExecutionResult {
                    task: task.task,
                    status: "indeterminate".into(),
                    phase: "navigation-verification".into(),
                    mutation_possible: true,
                    source_revision: expected_revision,
                    current_revision,
                    steps,
                    retry: retry_guidance(
                        RetryClassification::UnsafeUntilReconciled,
                        "recover_run",
                    ),
                    form: None,
                    extraction: None,
                    alerts: Vec::new(),
                    dialog: None,
                })
            }
        }
    }
}

impl BrowserSession {
    /// Execute any currently supported browser-backed Task Protocol family.
    pub async fn execute_task(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        match task.task {
            TaskKind::NavigationFollow => {
                self.execute_navigation_task(task, expected_revision, confirmed)
                    .await
            }
            TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => {
                self.execute_dialog_task(task, expected_revision, confirmed)
                    .await
            }
            _ => {
                self.execute_form_task(task, expected_revision, confirmed)
                    .await
            }
        }
    }
}

impl BrowserSession {
    /// Inspect or resolve one pending JavaScript dialog through the Task Protocol.
    pub async fn execute_dialog_task(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let plan = compile_task(task).map_err(|error| error.to_string())?;
        if !matches!(
            task.task,
            TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel
        ) {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "dialog execution only supports dialog.inspect, dialog.confirm, and dialog.cancel tasks",
            ));
        }
        if plan.confirmation_required && !confirmed {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "confirmation is required before this dialog task can mutate the browser",
            ));
        }
        let current_revision = self
            .page_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        if current_revision != expected_revision {
            return Ok(preflight_result(
                task,
                &plan,
                current_revision,
                "source revision is stale; no dialog action was dispatched",
            ));
        }
        let pending = self.pending_dialog().await;
        let mut steps = vec![step(
            &plan,
            TaskPlanOperation::ObserveScope,
            "succeeded",
            None,
        )];
        if task.task == TaskKind::DialogInspect {
            steps.push(step(
                &plan,
                TaskPlanOperation::InspectDialog,
                "succeeded",
                None,
            ));
            return Ok(TaskExecutionResult {
                task: task.task,
                status: "succeeded".into(),
                phase: "dialog-inspection".into(),
                mutation_possible: false,
                source_revision: expected_revision,
                current_revision,
                steps,
                retry: retry_guidance(RetryClassification::SafeImmediate, "inspect_page"),
                form: None,
                extraction: None,
                dialog: pending.clone(),
                alerts: if pending.is_some() {
                    vec!["dialog-pending".into()]
                } else {
                    Vec::new()
                },
            });
        }
        if pending.is_none() {
            return Ok(preflight_result(
                task,
                &plan,
                current_revision,
                "no pending JavaScript dialog is available",
            ));
        }
        let action = match task.task {
            TaskKind::DialogConfirm => self.accept_dialog().await,
            TaskKind::DialogCancel => self.dismiss_dialog().await,
            _ => unreachable!(),
        };
        let still_pending = self.pending_dialog().await.is_some();
        let succeeded = action.is_ok() && !still_pending;
        let operation = if task.task == TaskKind::DialogConfirm {
            TaskPlanOperation::ConfirmDialog
        } else {
            TaskPlanOperation::CancelDialog
        };
        steps.push(step(
            &plan,
            operation,
            if succeeded {
                "succeeded"
            } else {
                "indeterminate"
            },
            (!succeeded).then(|| "dialog outcome was not verified".into()),
        ));
        let current_revision = self
            .page_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        Ok(TaskExecutionResult {
            task: task.task,
            status: if succeeded {
                "succeeded"
            } else {
                "indeterminate"
            }
            .into(),
            phase: "dialog-verification".into(),
            mutation_possible: true,
            source_revision: expected_revision,
            current_revision,
            steps,
            retry: if succeeded {
                retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
            } else {
                retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run")
            },
            form: None,
            extraction: None,
            dialog: None,
            alerts: Vec::new(),
        })
    }
}
fn step(
    plan: &TaskExecutionPlan,
    operation: TaskPlanOperation,
    status: &str,
    detail: Option<String>,
) -> TaskStepResult {
    let ordinal = plan
        .steps
        .iter()
        .find(|candidate| candidate.operation == operation)
        .map_or(0, |candidate| candidate.ordinal);
    TaskStepResult {
        ordinal,
        operation,
        status: status.into(),
        detail,
    }
}

fn retry_guidance(classification: RetryClassification, operation: &str) -> RetryGuidance {
    RetryGuidance {
        classification,
        recommended_operation: operation.into(),
    }
}

fn preflight_result(
    task: &GlassTask,
    plan: &TaskExecutionPlan,
    revision: u64,
    detail: &str,
) -> TaskExecutionResult {
    TaskExecutionResult {
        task: task.task,
        status: "preflight-failed".into(),
        phase: "preflight".into(),
        mutation_possible: false,
        source_revision: revision,
        current_revision: revision,
        steps: plan
            .steps
            .iter()
            .map(|step| TaskStepResult {
                ordinal: step.ordinal,
                operation: step.operation,
                status: "not-run".into(),
                detail: Some(detail.into()),
            })
            .collect(),
        retry: retry_guidance(RetryClassification::SafeAfterReobserve, "inspect_page"),
        form: None,
        alerts: Vec::new(),
        extraction: None,
        dialog: None,
    }
}

fn scoped_regions_for_observation<'a>(
    observation: &'a InspectPageResult,
    task: &GlassTask,
) -> BrowserResult<Vec<&'a SemanticRegion>> {
    let Some(region_name) = task.scope.region_name.as_deref() else {
        return Err("browser-backed task requires a semantic region scope".into());
    };
    let regions = observation
        .regions
        .iter()
        .filter(|region| region.label.eq_ignore_ascii_case(region_name))
        .collect::<Vec<_>>();
    match regions.len() {
        1 => Ok(regions),
        0 => Err(format!("semantic region not found: {region_name}").into()),
        _ => Err(format!("semantic region is ambiguous: {region_name}").into()),
    }
}

fn resolved_fields(
    regions: &[&SemanticRegion],
    inputs: &std::collections::BTreeMap<String, String>,
) -> BrowserResult<Vec<(String, String)>> {
    inputs
        .iter()
        .map(|(name, value)| {
            Ok((
                unique_target(regions, name)?.reference.clone(),
                value.clone(),
            ))
        })
        .collect()
}

fn unique_target<'a>(
    regions: &[&'a SemanticRegion],
    name: &str,
) -> BrowserResult<&'a SemanticTarget> {
    let matches = regions
        .iter()
        .flat_map(|region| region.targets.iter())
        .filter(|target| target.name.eq_ignore_ascii_case(name))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [target] => Ok(target),
        [] => Err(format!("semantic form target not found: {name}").into()),
        _ => Err(format!("semantic form target is ambiguous: {name}").into()),
    }
}

fn extraction_matches_observation(
    extraction: &StructuredExtractionResult,
    observation: &InspectPageResult,
) -> bool {
    extraction.source_revision == observation.revision
        && extraction.source_route.target_id == observation.page.target_id
        && extraction.source_route.frame_id == observation.page.frame_id
        && extraction.source_route.url == observation.page.url
}

fn semantic_page_changed(before: &InspectPageResult, after: &InspectPageResult) -> bool {
    before.page.kind != after.page.kind
        || before.page.title != after.page.title
        || before.page.url != after.page.url
        || before.page.target_id != after.page.target_id
        || before.page.frame_id != after.page.frame_id
        || before.page.confidence != after.page.confidence
        || before.page.evidence != after.page.evidence
        || semantic_regions_changed(&before.regions, &after.regions)
}

fn semantic_regions_changed(before: &[SemanticRegion], after: &[SemanticRegion]) -> bool {
    before.len() != after.len()
        || before.iter().zip(after).any(|(before, after)| {
            before.id != after.id
                || before.kind != after.kind
                || before.label != after.label
                || before.interactive_count != after.interactive_count
                || before.item_count != after.item_count
                || before.confidence != after.confidence
                || before.evidence != after.evidence
                || before.targets.len() != after.targets.len()
                || before
                    .targets
                    .iter()
                    .zip(&after.targets)
                    .any(|(before, after)| {
                        before.role != after.role
                            || before.name != after.name
                            || before.input_type != after.input_type
                    })
        })
}

fn alert_labels<'a, I>(regions: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a SemanticRegion>,
{
    regions
        .into_iter()
        .filter(|region| matches!(region.kind, super::SemanticRegionKind::Alert))
        .map(|region| region.label.clone())
        .collect()
}
fn postconditions_hold(
    task: &GlassTask,
    observation: &InspectPageResult,
    regions: &[&SemanticRegion],
) -> bool {
    task.postconditions
        .iter()
        .all(|postcondition| match postcondition.kind {
            TaskPostconditionKind::ValidationClear => {
                alert_labels(observation.regions.iter()).is_empty()
            }
            TaskPostconditionKind::RegionPresent => {
                postcondition.expected.as_ref().is_some_and(|expected| {
                    observation
                        .regions
                        .iter()
                        .any(|region| region.label.eq_ignore_ascii_case(expected))
                })
            }
            TaskPostconditionKind::NavigationOccurred => observation.revision > 0,
            TaskPostconditionKind::PageKind => {
                postcondition.expected.as_ref().is_none_or(|expected| {
                    format!("{:?}", observation.page.kind).eq_ignore_ascii_case(expected)
                })
            }
            TaskPostconditionKind::DialogClosed
            | TaskPostconditionKind::EntityState
            | TaskPostconditionKind::RecordsExtracted => false,
        })
        && !regions.is_empty()
}

async fn bounded<T, F>(future: F, timeout_ms: u64) -> BrowserResult<T>
where
    F: Future<Output = BrowserResult<T>>,
{
    tokio::time::timeout(Duration::from_millis(timeout_ms), future)
        .await
        .map_err(|_| {
            Box::new(IoError::new(
                ErrorKind::TimedOut,
                "task execution exceeded its timeout budget",
            )) as Box<dyn std::error::Error>
        })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::session::{
        SemanticConfidence, SemanticPage, SemanticPageKind, SemanticRegionKind,
    };
    use crate::task_protocol::{
        TASK_PROTOCOL_SCHEMA_VERSION, TaskAmbiguityPolicy, TaskLimits, TaskRiskClass, TaskScope,
    };
    use std::collections::BTreeMap;

    fn target(name: &str, reference: &str) -> SemanticTarget {
        SemanticTarget {
            reference: reference.into(),
            role: "textbox".into(),
            name: name.into(),
            input_type: Some("text".into()),
        }
    }

    fn region(label: &str, targets: Vec<SemanticTarget>) -> SemanticRegion {
        SemanticRegion {
            id: label.into(),
            kind: SemanticRegionKind::Form,
            label: label.into(),
            interactive_count: targets.len(),
            item_count: None,
            confidence: SemanticConfidence::Exact,
            evidence: Vec::new(),
            targets,
            expansion: None,
        }
    }

    fn task() -> GlassTask {
        GlassTask {
            schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
            task: TaskKind::FormFill,
            scope: TaskScope {
                region_name: Some("Checkout".into()),
                ..TaskScope::default()
            },
            inputs: BTreeMap::from([(String::from("Email"), String::from("a@example.test"))]),
            limits: TaskLimits::default(),
            risk: TaskRiskClass::LocalMutation,
            ambiguity: TaskAmbiguityPolicy::Fail,
            revision: Default::default(),
            postconditions: Vec::new(),
        }
    }

    #[test]
    fn form_targets_are_resolved_by_unique_semantic_name() {
        let form = region("Checkout", vec![target("Email", "target-1")]);
        let fields = resolved_fields(&[&form], &task().inputs).unwrap();
        assert_eq!(
            fields,
            vec![(String::from("target-1"), String::from("a@example.test"))]
        );
    }

    #[test]
    fn ambiguous_form_targets_fail_before_dispatch() {
        let form = region(
            "Checkout",
            vec![target("Email", "target-1"), target("Email", "target-2")],
        );
        let error = resolved_fields(&[&form], &task().inputs).unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn task_retry_guidance_uses_canonical_recovery_shape() {
        let value = serde_json::to_value(retry_guidance(
            RetryClassification::UnsafeUntilReconciled,
            "recover_run",
        ))
        .unwrap();
        assert_eq!(value["classification"], "unsafeUntilReconciled");
        assert_eq!(value["recommendedOperation"], "recover_run");
    }

    #[test]
    fn form_scope_requires_one_matching_region() {
        let observation = InspectPageResult {
            page: SemanticPage {
                kind: SemanticPageKind::Form,
                title: "Checkout".into(),
                url: "https://example.test/checkout".into(),
                target_id: "page".into(),
                frame_id: "frame".into(),
                confidence: SemanticConfidence::Exact,
                evidence: Vec::new(),
            },
            revision: 7,
            regions: vec![region("Shipping", Vec::new())],
            limits: Default::default(),
            focused_target: None,
            alerts: Vec::new(),
        };
        let error = scoped_regions_for_observation(&observation, &task()).unwrap_err();
        assert!(error.to_string().contains("not found"));
    }
}

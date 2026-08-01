//! Browser-backed execution for the bounded form Task Protocol families.

use super::{
    BrowserResult, BrowserSession, ExtractionField, ExtractionKind, FillFormOutcome,
    InspectPageResult, SemanticRegion, SemanticTarget, StructuredExtractionRequest,
    StructuredExtractionResult,
};
use crate::task_compiler::{TaskExecutionPlan, TaskPlanOperation, compile_task};
use crate::task_protocol::{GlassTask, TaskKind, TaskPostconditionKind};
use serde::Serialize;
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
    pub retry: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<FillFormOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction: Option<StructuredExtractionResult>,
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
                | TaskKind::RegionExtract
        ) {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "unsupported task family; browser execution currently supports form and region extraction tasks",
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
                    retry: "not-needed".into(),
                    form: None,
                    extraction: Some(extraction),
                    alerts: alert_labels(scoped_regions.iter().copied()),
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
                    retry: "not-needed".into(),
                    form: None,
                    extraction: None,
                    alerts,
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
                        "not-needed"
                    } else {
                        "fix-validation-errors"
                    }
                    .into(),
                    form: None,
                    alerts,
                    extraction: None,
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
                        "not-needed"
                    } else {
                        "reconcile-before-retry"
                    }
                    .into(),
                    form: Some(form),
                    alerts: alert_labels(after.regions.iter()),
                    extraction: None,
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
                        "not-needed"
                    } else {
                        "reconcile-before-retry"
                    }
                    .into(),
                    form: None,
                    alerts: alert_labels(after.regions.iter()),
                    extraction: None,
                })
            }
            _ => unreachable!(),
        }
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
        retry: "reobserve-and-correct-task".into(),
        form: None,
        alerts: Vec::new(),
        extraction: None,
    }
}

fn scoped_regions_for_observation<'a>(
    observation: &'a InspectPageResult,
    task: &GlassTask,
) -> BrowserResult<Vec<&'a SemanticRegion>> {
    let Some(region_name) = task.scope.region_name.as_deref() else {
        return Err("form task requires a semantic region scope".into());
    };
    let regions = observation
        .regions
        .iter()
        .filter(|region| region.label.eq_ignore_ascii_case(region_name))
        .collect::<Vec<_>>();
    match regions.len() {
        1 => Ok(regions),
        0 => Err(format!("semantic form region not found: {region_name}").into()),
        _ => Err(format!("semantic form region is ambiguous: {region_name}").into()),
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

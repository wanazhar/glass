//! Browser-backed execution for the bounded Task Protocol families.

use super::{
    BrowserResult, BrowserSession, ExtractionField, ExtractionKind, FillFormOutcome,
    InspectPageResult, PendingDialog, SemanticObservationLevel, SemanticRegion, SemanticRegionKind,
    StructuredExtractionLimits, StructuredExtractionProvenance, StructuredExtractionRecord,
    StructuredExtractionRequest, StructuredExtractionResult, parse_revisioned_reference,
};
use crate::browser::session::{
    KnowledgeLearningPolicy, KnowledgeLearningRequest, KnowledgeLearningResult,
    KnowledgeLookupContext, KnowledgeLookupOptions, KnowledgeRecordBuildOptions, KnowledgeStore,
    KnowledgeVerifiedWorkflowEvidence,
};
use crate::extraction::{
    EvidenceCoverage, EvidenceFact, EvidenceQuality, EvidenceSource, ExtractionBudgets,
    ExtractionEvidence, ExtractionEvidenceLimits, ExtractionRequest, ExtractionScope,
    MAX_EXTRACTION_DURATION_MS,
};
use crate::protocol::{RetryClassification, RetryGuidance};
use crate::task_compiler::{
    TaskCompilationOptions, TaskExecutionPlan, TaskPlanOperation, TaskRuntimeCapability,
    compile_task, compile_task_with_options, effective_postconditions,
};
use crate::task_protocol::{
    GlassTask, TaskKind, TaskPostconditionKind, TaskRevisionPolicy, TaskRiskClass,
};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{Error as IoError, ErrorKind};
use std::time::{Duration, Instant};

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
    pub receipt: TaskExecutionReceipt,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<FillFormOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction: Option<StructuredExtractionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog: Option<PendingDialog>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<String>,
}

/// Compact, value-free explanation of why a task was allowed and whether its
/// verification obligations held.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionReceipt {
    pub source_revision: u64,
    pub selected_entity_ids: Vec<String>,
    /// Entities for which the plan required an ephemeral, revision-bound
    /// browser reference. This is deliberately not called `bound`: a
    /// preflight-failure receipt must not imply that binding succeeded.
    pub binding_candidate_entity_ids: Vec<String>,
    pub required_runtime_capabilities: Vec<TaskRuntimeCapability>,
    pub evidence_requirements: Vec<crate::task_compiler::TaskEntityEvidenceRequirement>,
    pub confirmation_required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postconditions: Vec<TaskPostconditionReceipt>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPostconditionReceipt {
    pub kind: TaskPostconditionKind,
    pub held: bool,
}

impl TaskExecutionReceipt {
    fn from_plan(plan: &TaskExecutionPlan) -> Self {
        let binding_candidate_entity_ids = plan
            .entity_binding_keys
            .iter()
            .filter(|key| {
                matches!(
                    key.kind,
                    crate::web_ir::WebIrEntityKind::Field
                        | crate::web_ir::WebIrEntityKind::Action
                        | crate::web_ir::WebIrEntityKind::Link
                        | crate::web_ir::WebIrEntityKind::NavigationItem
                        | crate::web_ir::WebIrEntityKind::Tab
                        | crate::web_ir::WebIrEntityKind::PaginationControl
                        | crate::web_ir::WebIrEntityKind::UnknownInteractive
                )
            })
            .map(|key| key.entity_id.clone())
            .collect();
        Self {
            source_revision: plan.source_ir_revision,
            selected_entity_ids: plan.selected_entity_ids.clone(),
            binding_candidate_entity_ids,
            required_runtime_capabilities: plan.required_runtime_capabilities.clone(),
            evidence_requirements: plan.entity_evidence_requirements.clone(),
            confirmation_required: plan.confirmation_required,
            postconditions: Vec::new(),
        }
    }
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

/// Ephemeral browser references resolved from a compiled semantic key at the
/// exact source revision. These bindings never cross the process boundary.
struct LiveTaskBindings {
    revision: u64,
    references: BTreeMap<String, String>,
}

impl LiveTaskBindings {
    fn resolve(
        plan: &TaskExecutionPlan,
        regions: &[&SemanticRegion],
        revision: u64,
    ) -> Result<Self, String> {
        if revision != plan.source_ir_revision {
            return Err("cannot bind a plan against a different page revision".into());
        }
        let mut references = BTreeMap::new();
        for key in &plan.entity_binding_keys {
            if !matches!(
                key.kind,
                crate::web_ir::WebIrEntityKind::Field
                    | crate::web_ir::WebIrEntityKind::Action
                    | crate::web_ir::WebIrEntityKind::Link
                    | crate::web_ir::WebIrEntityKind::NavigationItem
                    | crate::web_ir::WebIrEntityKind::Tab
                    | crate::web_ir::WebIrEntityKind::PaginationControl
                    | crate::web_ir::WebIrEntityKind::UnknownInteractive
            ) {
                continue;
            }
            let mut candidates = regions
                .iter()
                .flat_map(|region| region.targets.iter())
                .filter(|target| {
                    key.role
                        .as_deref()
                        .is_none_or(|role| target.role.eq_ignore_ascii_case(role))
                        && key
                            .name
                            .as_deref()
                            .is_none_or(|name| target.name.eq_ignore_ascii_case(name))
                });
            let Some(target) = candidates.next() else {
                return Err(format!(
                    "selected entity {} did not resolve to exactly one revision-bound browser target",
                    key.entity_id
                ));
            };
            if candidates.next().is_some() {
                return Err(format!(
                    "selected entity {} resolved to multiple revision-bound browser targets",
                    key.entity_id
                ));
            }
            let parsed = parse_revisioned_reference(&target.reference)
                .map_err(|error| format!("invalid live binding reference: {error}"))?
                .ok_or_else(|| "live binding is not revisioned".to_string())?;
            if parsed.revision != revision {
                return Err(format!(
                    "selected entity {} resolved to a stale browser target",
                    key.entity_id
                ));
            }
            references.insert(key.entity_id.clone(), target.reference.clone());
        }
        Ok(Self {
            revision,
            references,
        })
    }

    fn reference_for_name<'a>(
        &'a self,
        plan: &TaskExecutionPlan,
        name: &str,
    ) -> Result<&'a str, String> {
        let mut candidates = plan.entity_binding_keys.iter().filter(|key| {
            key.name
                .as_deref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
                && self.references.contains_key(&key.entity_id)
        });
        let Some(key) = candidates.next() else {
            return Err(format!(
                "compiled entity named {name:?} does not have one live binding"
            ));
        };
        if candidates.next().is_some() {
            return Err(format!(
                "compiled entity named {name:?} has multiple live bindings"
            ));
        }
        self.references
            .get(&key.entity_id)
            .map(String::as_str)
            .ok_or_else(|| "live binding disappeared before dispatch".into())
    }

    fn bound_fields(
        &self,
        plan: &TaskExecutionPlan,
        inputs: &BTreeMap<String, String>,
    ) -> Result<Vec<(String, String, String)>, String> {
        if self.revision != plan.source_ir_revision {
            return Err("live field bindings are stale".into());
        }
        inputs
            .iter()
            .map(|(name, value)| {
                self.reference_for_name(plan, name)
                    .map(|reference| (reference.to_string(), name.clone(), value.clone()))
            })
            .collect()
    }
}

fn live_task_evidence_sources() -> Vec<EvidenceSource> {
    vec![
        EvidenceSource::Accessibility,
        EvidenceSource::Dom,
        EvidenceSource::Forms,
        EvidenceSource::Layout,
        EvidenceSource::Navigation,
        EvidenceSource::Tables,
        EvidenceSource::Collections,
        EvidenceSource::Dialogs,
        EvidenceSource::Frames,
        EvidenceSource::ShadowDom,
        EvidenceSource::Svg,
        EvidenceSource::CanvasDetection,
        EvidenceSource::MediaMetadata,
        EvidenceSource::EmbeddedDocument,
        EvidenceSource::Pdf,
        EvidenceSource::BrowserNative,
        EvidenceSource::Bridge,
        EvidenceSource::BoundedProbe,
    ]
}

impl BrowserSession {
    async fn compile_live_task(&self, task: &GlassTask) -> BrowserResult<TaskExecutionPlan> {
        self.compile_live_task_with_memory(task, None).await
    }

    /// Compile a task from one fresh live Web IR revision while consulting
    /// caller-owned advisory memory.
    ///
    /// Memory is explicitly opt-in. The current extraction remains
    /// authoritative: the store receives only a lookup context built from a
    /// fresh semantic observation whose revision must equal the compiled IR.
    /// Historical records can explain ranking, but cannot provide entity
    /// handles, actions, or postconditions.
    pub async fn compile_live_task_with_knowledge(
        &self,
        task: &GlassTask,
        store: &KnowledgeStore,
        options: KnowledgeLookupOptions,
    ) -> BrowserResult<TaskExecutionPlan> {
        self.compile_live_task_with_memory(task, Some((store, options)))
            .await
    }

    /// Persist one verified-learning witness after re-observing the live page.
    ///
    /// This is an explicit opt-in update path. The caller supplies a successful
    /// guarded workflow witness and privacy scope; the session supplies the
    /// fresh semantic observation, so a stale or mismatched revision cannot be
    /// promoted into persistent memory.
    pub async fn learn_verified_knowledge(
        &self,
        store: &mut KnowledgeStore,
        build: KnowledgeRecordBuildOptions,
        evidence: KnowledgeVerifiedWorkflowEvidence,
        policy: KnowledgeLearningPolicy,
    ) -> BrowserResult<KnowledgeLearningResult> {
        let observation = self
            .semantic_observe(SemanticObservationLevel::Structured)
            .await?;
        if observation.revision != evidence.guarded_revision {
            return Err(format!(
                "verified-learning witness revision {} does not match fresh observation revision {}",
                evidence.guarded_revision, observation.revision
            )
            .into());
        }
        store
            .learn_verified(KnowledgeLearningRequest {
                observation: &observation,
                build,
                evidence,
                policy,
            })
            .map_err(|error| error.to_string().into())
    }

    async fn compile_live_task_with_memory(
        &self,
        task: &GlassTask,
        memory: Option<(&KnowledgeStore, KnowledgeLookupOptions)>,
    ) -> BrowserResult<TaskExecutionPlan> {
        if matches!(
            task.task,
            TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel
        ) && self.pending_dialog().await.is_some()
        {
            let revision = self
                .page_revision
                .load(std::sync::atomic::Ordering::Relaxed);
            let evidence = ExtractionEvidence {
                schema_version: crate::extraction::EXTRACTION_CONTRACT_SCHEMA_VERSION,
                revision,
                scope: ExtractionScope::Document,
                sources: vec![EvidenceSource::Dialogs],
                facts: vec![EvidenceFact {
                    source: EvidenceSource::Dialogs,
                    kind: "semantic".into(),
                    quality: EvidenceQuality::Confirmed,
                    role: Some("dialog".into()),
                    name: Some("Pending dialog".into()),
                    input_type: None,
                    autocomplete: None,
                    required: None,
                    read_only: None,
                    empty: None,
                    checked: None,
                    disabled: None,
                    geometry_present: None,
                    parent_role: None,
                    relationship_hint: None,
                }],
                limits: ExtractionEvidenceLimits {
                    truncated: false,
                    omitted_facts: 0,
                    text_bytes: 14,
                    missing_sources: Vec::new(),
                },
                coverage: EvidenceCoverage {
                    structural: EvidenceQuality::Strong,
                    semantic: EvidenceQuality::Confirmed,
                    interactive_entities_observed: 1,
                    opaque_regions: 0,
                    reasons: Vec::new(),
                },
                surface_set: None,
            };
            let ir = crate::web_ir::reconcile_evidence(&evidence)?;
            return compile_task(task, &ir).map_err(|error| error.to_string().into());
        }
        let budgets = ExtractionBudgets {
            max_duration_ms: task.limits.timeout_ms.clamp(1, MAX_EXTRACTION_DURATION_MS),
            ..ExtractionBudgets::default()
        };
        let request = ExtractionRequest {
            schema_version: crate::extraction::EXTRACTION_CONTRACT_SCHEMA_VERSION,
            scope: ExtractionScope::Document,
            sources: live_task_evidence_sources(),
            budgets,
        };
        let ir = self.extract_web_ir(&request).await?;
        let plan = if let Some((store, options)) = memory {
            let observation = self
                .semantic_observe(SemanticObservationLevel::Structured)
                .await?;
            if observation.revision != ir.revision {
                return Err(format!(
                    "live semantic observation revision {} disagrees with extracted Web IR revision {}",
                    observation.revision, ir.revision
                )
                .into());
            }
            let context = KnowledgeLookupContext::from_observation(&observation, options)?;
            compile_task_with_options(
                task,
                &ir,
                TaskCompilationOptions {
                    knowledge_store: Some(store),
                    knowledge_context: Some(&context),
                },
            )
        } else {
            compile_task(task, &ir)
        };
        plan.map_err(|error| error.to_string().into())
    }

    /// Execute a validated form task against one caller-observed revision.
    ///
    /// `expected_revision` is supplied by the caller's preceding semantic
    /// observation. The runtime always re-observes before mutation, resolves
    /// targets from that observation, and passes the resulting revision into
    /// the guarded action APIs.
    async fn execute_form_task_unchecked(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let plan = self.compile_live_task(task).await?;
        if let Some(detail) = unsupported_runtime_capability(&plan) {
            return Ok(preflight_result(task, &plan, expected_revision, detail));
        }
        if !matches!(
            task.task,
            TaskKind::FormInspect
                | TaskKind::FormFill
                | TaskKind::FormValidate
                | TaskKind::FormSubmit
                | TaskKind::FieldRead
                | TaskKind::NavigationSelectTab
                | TaskKind::NavigationOpenMenu
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
                "unsupported task family; browser execution currently supports form, field, table, collection, region extraction, navigation menu, and pagination tasks",
            ));
        }
        if let Some(detail) = compiled_revision_mismatch(&plan, expected_revision) {
            return Ok(preflight_result(task, &plan, expected_revision, &detail));
        }
        let expected_revision = plan.source_ir_revision;
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
        let live_bindings =
            match LiveTaskBindings::resolve(&plan, &scoped_regions, observation.revision) {
                Ok(bindings) => bindings,
                Err(detail) => {
                    return Ok(preflight_result(task, &plan, observation.revision, &detail));
                }
            };
        let mut steps = vec![step(
            &plan,
            TaskPlanOperation::ObserveScope,
            "succeeded",
            None,
        )];

        match task.task {
            TaskKind::NavigationOpenMenu => {
                Box::pin(self.execute_open_menu_task(task, &plan, &observation, &live_bindings))
                    .await
            }
            TaskKind::NavigationSelectTab => {
                let Some(tab_name) = task.inputs.get("tab") else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "navigation.selectTab requires the semantic tab input",
                    ));
                };
                let target_reference = match live_bindings.reference_for_name(&plan, tab_name) {
                    Ok(reference) => reference,
                    Err(error) => {
                        return Ok(preflight_result(task, &plan, observation.revision, &error));
                    }
                };
                let outcome = bounded(
                    self.click_with_revision(target_reference, observation.revision),
                    task.limits.timeout_ms,
                )
                .await;
                let after = match bounded(self.inspect_page(), task.limits.timeout_ms).await {
                    Ok(after) => after,
                    Err(error) => {
                        let current_revision = self
                            .page_revision
                            .load(std::sync::atomic::Ordering::Relaxed);
                        return Ok(mutation_failure_result(
                            task,
                            &plan,
                            (observation.revision, current_revision),
                            steps,
                            TaskPlanOperation::SelectTab,
                            "navigation-verification",
                            format!("post-action observation failed: {error}"),
                        ));
                    }
                };
                let succeeded = outcome.is_ok()
                    && wait_for_aria_true(
                        self,
                        target_reference,
                        "aria-selected",
                        task.limits.timeout_ms,
                    )
                    .await;
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
                    status: if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "navigation-verification".into(),
                    mutation_possible: true,
                    source_revision: observation.revision,
                    current_revision: self
                        .page_revision
                        .load(std::sync::atomic::Ordering::Relaxed)
                        .max(after.revision),
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
                let mut unsafe_until_reconciled = false;
                let deadline =
                    tokio::time::Instant::now() + Duration::from_millis(task.limits.timeout_ms);
                let mut pagination_extraction: Option<StructuredExtractionResult> = None;
                while completed < max_pages {
                    let regions = match scoped_regions_for_observation(&current, task) {
                        Ok(regions) => regions,
                        Err(error) if completed == 0 => {
                            return Ok(preflight_result(
                                task,
                                &plan,
                                current.revision,
                                &error.to_string(),
                            ));
                        }
                        Err(error) => {
                            let current_revision = self
                                .page_revision
                                .load(std::sync::atomic::Ordering::Relaxed);
                            return Ok(mutation_failure_result(
                                task,
                                &plan,
                                (source_revision, current_revision),
                                steps,
                                TaskPlanOperation::CollectPages,
                                "pagination-collection",
                                format!("post-action scope reconciliation failed: {error}"),
                            ));
                        }
                    };
                    let region = regions
                        .first()
                        .expect("scoped_regions contains exactly one region");
                    let page_extraction = match bounded(
                        self.extract_structured(&StructuredExtractionRequest {
                            fields: vec![ExtractionField {
                                name: "items".into(),
                                path: "$.structuredRecords".into(),
                                kind: ExtractionKind::RepeatedItems,
                            }],
                            region_id: Some(region.id.clone()),
                            start_index: 0,
                            continuation: None,
                            max_items: task.limits.max_items as usize,
                            max_bytes: 64 * 1024,
                        }),
                        remaining_timeout(deadline),
                    )
                    .await
                    {
                        Ok(extraction) => extraction,
                        Err(error) if completed == 0 => {
                            return Ok(preflight_result(
                                task,
                                &plan,
                                current.revision,
                                &format!("pagination extraction failed: {error}"),
                            ));
                        }
                        Err(error) => {
                            let current_revision = self
                                .page_revision
                                .load(std::sync::atomic::Ordering::Relaxed);
                            return Ok(mutation_failure_result(
                                task,
                                &plan,
                                (source_revision, current_revision),
                                steps,
                                TaskPlanOperation::CollectPages,
                                "pagination-collection",
                                format!("pagination extraction failed: {error}"),
                            ));
                        }
                    };
                    merge_pagination_extraction(
                        &mut pagination_extraction,
                        page_extraction,
                        task.limits.max_items as usize,
                        64 * 1024,
                    );
                    let initial_reference = (completed == 0)
                        .then(|| live_bindings.reference_for_name(&plan, next_name))
                        .transpose();
                    let initial_reference = match initial_reference {
                        Ok(reference) => reference,
                        Err(error) => {
                            return Ok(preflight_result(task, &plan, current.revision, &error));
                        }
                    };
                    let candidates = region
                        .targets
                        .iter()
                        .filter(|target| {
                            initial_reference.map_or_else(
                                || target.name.eq_ignore_ascii_case(next_name),
                                |reference| target.reference == reference,
                            )
                        })
                        .collect::<Vec<_>>();
                    if candidates.is_empty() {
                        stopped = true;
                        break;
                    }
                    if candidates.len() > 1 {
                        if completed == 0 {
                            return Ok(preflight_result(
                                task,
                                &plan,
                                current.revision,
                                "pagination.collect next control is ambiguous",
                            ));
                        }
                        return Ok(mutation_failure_result(
                            task,
                            &plan,
                            (source_revision, current.revision),
                            steps,
                            TaskPlanOperation::CollectPages,
                            "pagination-collection",
                            "pagination.collect next control became ambiguous after mutation",
                        ));
                    }
                    let target = candidates[0];
                    if !matches!(target.role.as_str(), "button" | "link" | "tab") {
                        if completed == 0 {
                            return Ok(preflight_result(
                                task,
                                &plan,
                                current.revision,
                                "pagination.collect target is not a semantic navigation control",
                            ));
                        }
                        return Ok(mutation_failure_result(
                            task,
                            &plan,
                            (source_revision, current.revision),
                            steps,
                            TaskPlanOperation::CollectPages,
                            "pagination-collection",
                            "pagination.collect target became non-navigational after mutation",
                        ));
                    }
                    let before_revision = current.revision;
                    let outcome = bounded(
                        self.click_with_revision(&target.reference, before_revision),
                        remaining_timeout(deadline),
                    )
                    .await;
                    let after =
                        match bounded(self.inspect_page(), remaining_timeout(deadline)).await {
                            Ok(after) => after,
                            Err(error) => {
                                let current_revision = self
                                    .page_revision
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                return Ok(mutation_failure_result(
                                    task,
                                    &plan,
                                    (source_revision, current_revision),
                                    steps,
                                    TaskPlanOperation::CollectPages,
                                    "pagination-collection",
                                    format!("post-action observation failed: {error}"),
                                ));
                            }
                        };
                    let after = match Box::pin(wait_for_semantic_page_change(
                        self,
                        &current,
                        after,
                        remaining_timeout(deadline),
                    ))
                    .await
                    {
                        Ok(after) => after,
                        Err(error) => {
                            let current_revision = self
                                .page_revision
                                .load(std::sync::atomic::Ordering::Relaxed);
                            return Ok(mutation_failure_result(
                                task,
                                &plan,
                                (source_revision, current_revision),
                                steps,
                                TaskPlanOperation::CollectPages,
                                "pagination-collection",
                                format!("post-action transition observation failed: {error}"),
                            ));
                        }
                    };
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
                            receipt: TaskExecutionReceipt::from_plan(&plan),
                            status: "indeterminate".into(),
                            phase: "pagination-collection".into(),
                            mutation_possible: true,
                            source_revision,
                            current_revision: self
                                .page_revision
                                .load(std::sync::atomic::Ordering::Relaxed)
                                .max(after.revision),
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
                    let page_changed = semantic_page_changed(&current, &after);
                    current = after;
                    if !page_changed {
                        let regions = match scoped_regions_for_observation(&current, task) {
                            Ok(regions) => regions,
                            Err(error) => {
                                let current_revision = self
                                    .page_revision
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                return Ok(mutation_failure_result(
                                    task,
                                    &plan,
                                    (source_revision, current_revision),
                                    steps,
                                    TaskPlanOperation::CollectPages,
                                    "pagination-collection",
                                    format!("post-action scope reconciliation failed: {error}"),
                                ));
                            }
                        };
                        if pagination_next_is_usable(&regions, next_name) {
                            let step = steps
                                .last_mut()
                                .expect("pagination click always records one step");
                            step.status = "indeterminate".into();
                            step.detail = Some(
                                "pagination click produced no semantic page change while the next control remained available"
                                    .into(),
                            );
                            unsafe_until_reconciled = true;
                        } else {
                            stopped = true;
                        }
                        break;
                    }
                }
                let mut alerts = alert_labels(current.regions.iter());
                if !stopped && completed == max_pages {
                    alerts.push("pagination-limit-reached".into());
                }
                Ok(TaskExecutionResult {
                    task: task.task,
                    receipt: TaskExecutionReceipt::from_plan(&plan),
                    status: if unsafe_until_reconciled {
                        "indeterminate"
                    } else {
                        "succeeded"
                    }
                    .into(),
                    phase: "pagination-collection".into(),
                    mutation_possible: completed > 0,
                    source_revision,
                    current_revision: current.revision,
                    steps,
                    retry: retry_guidance(
                        if unsafe_until_reconciled {
                            RetryClassification::UnsafeUntilReconciled
                        } else {
                            RetryClassification::SafeImmediate
                        },
                        if unsafe_until_reconciled {
                            "recover_run"
                        } else {
                            "inspect_page"
                        },
                    ),
                    form: None,
                    extraction: pagination_extraction,
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
                let target_reference = match live_bindings.reference_for_name(&plan, next_name) {
                    Ok(reference) => reference,
                    Err(error) => {
                        return Ok(preflight_result(task, &plan, observation.revision, &error));
                    }
                };
                let outcome = bounded(
                    self.click_with_revision(target_reference, observation.revision),
                    task.limits.timeout_ms,
                )
                .await;
                let after = match bounded(self.inspect_page(), task.limits.timeout_ms).await {
                    Ok(after) => after,
                    Err(error) => {
                        let current_revision = self
                            .page_revision
                            .load(std::sync::atomic::Ordering::Relaxed);
                        return Ok(mutation_failure_result(
                            task,
                            &plan,
                            (observation.revision, current_revision),
                            steps,
                            TaskPlanOperation::NextPage,
                            "pagination-verification",
                            format!("post-action observation failed: {error}"),
                        ));
                    }
                };
                let after = match wait_for_semantic_page_change(
                    self,
                    &observation,
                    after,
                    task.limits.timeout_ms,
                )
                .await
                {
                    Ok(after) => after,
                    Err(error) => {
                        let current_revision = self
                            .page_revision
                            .load(std::sync::atomic::Ordering::Relaxed);
                        return Ok(mutation_failure_result(
                            task,
                            &plan,
                            (observation.revision, current_revision),
                            steps,
                            TaskPlanOperation::NextPage,
                            "pagination-verification",
                            format!("post-action transition observation failed: {error}"),
                        ));
                    }
                };
                let succeeded = outcome.is_ok() && semantic_page_changed(&observation, &after);
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
                    status: if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "pagination-verification".into(),
                    mutation_possible: true,
                    source_revision: observation.revision,
                    current_revision: self
                        .page_revision
                        .load(std::sync::atomic::Ordering::Relaxed)
                        .max(after.revision),
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
                let target_reference = match live_bindings.reference_for_name(&plan, field_name) {
                    Ok(reference) => reference,
                    Err(error) => {
                        return Ok(preflight_result(task, &plan, observation.revision, &error));
                    }
                };
                let Some(target) = scoped_regions
                    .iter()
                    .flat_map(|region| region.targets.iter())
                    .find(|target| target.reference == target_reference)
                else {
                    return Ok(preflight_result(
                        task,
                        &plan,
                        observation.revision,
                        "compiled field binding disappeared before dispatch",
                    ));
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
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
                        records: vec![record.clone()],
                        record_items: Vec::new(),
                        provenance: vec!["$.interactive".into()],
                        truncated: false,
                        continuation: None,
                        field_provenance: vec![StructuredExtractionProvenance {
                            field: "field".into(),
                            path: "$.interactive".into(),
                            region_id: scoped_regions.first().map(|region| region.id.clone()),
                            entity_ids: vec![target.reference.clone()],
                        }],
                        limits: StructuredExtractionLimits {
                            max_items: task.limits.max_items as usize,
                            max_bytes: 64 * 1024,
                            observed_items: 1,
                            serialized_bytes: serde_json::to_vec(&record)
                                .map_or(0, |bytes| bytes.len()),
                            truncated: false,
                        },
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
                        path: "$.structuredRecords".into(),
                        kind: ExtractionKind::Table,
                    }],
                    region_id: Some(region.id.clone()),
                    start_index: 0,
                    continuation: None,
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
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
                        path: "$.structuredRecords".into(),
                        kind: ExtractionKind::RepeatedItems,
                    }],
                    region_id: Some(region.id.clone()),
                    start_index: 0,
                    continuation: None,
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
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
                    start_index: 0,
                    continuation: None,
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
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
                let values =
                    match bounded(self.observe_with_form_values(), task.limits.timeout_ms).await {
                        Ok(values) => values,
                        Err(error) => {
                            steps.push(step(
                                &plan,
                                TaskPlanOperation::ValidateForm,
                                "verification-failed",
                                Some(format!("form validity observation failed: {error}")),
                            ));
                            return Ok(TaskExecutionResult {
                                task: task.task,
                                receipt: TaskExecutionReceipt::from_plan(&plan),
                                status: "verification-failed".into(),
                                phase: "validation".into(),
                                mutation_possible: false,
                                source_revision: observation.revision,
                                current_revision: self
                                    .page_revision
                                    .load(std::sync::atomic::Ordering::Relaxed),
                                steps,
                                retry: retry_guidance(
                                    RetryClassification::SafeAfterReconcile,
                                    "inspect_page",
                                ),
                                form: None,
                                extraction: None,
                                dialog: None,
                                alerts,
                            });
                        }
                    };
                let values_revision = self
                    .page_revision
                    .load(std::sync::atomic::Ordering::Relaxed);
                if values_revision != observation.revision {
                    steps.push(step(
                        &plan,
                        TaskPlanOperation::ValidateForm,
                        "stale",
                        Some("form value observation crossed revisions".into()),
                    ));
                    return Ok(TaskExecutionResult {
                        task: task.task,
                        receipt: TaskExecutionReceipt::from_plan(&plan),
                        status: "stale".into(),
                        phase: "validation".into(),
                        mutation_possible: false,
                        source_revision: observation.revision,
                        current_revision: values_revision,
                        steps,
                        retry: retry_guidance(
                            RetryClassification::SafeAfterReconcile,
                            "inspect_page",
                        ),
                        form: None,
                        extraction: None,
                        dialog: None,
                        alerts,
                    });
                }
                let validity_deadline =
                    Instant::now() + Duration::from_millis(task.limits.timeout_ms);
                let mut native_valid = true;
                for target in scoped_regions
                    .iter()
                    .flat_map(|region| region.targets.iter())
                {
                    let remaining_ms = validity_deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .min(u64::MAX as u128) as u64;
                    if remaining_ms == 0 {
                        native_valid = false;
                        break;
                    }
                    let validity = bounded(
                        async { Ok(native_control_validity(self, &target.reference).await) },
                        remaining_ms,
                    )
                    .await
                    .ok()
                    .flatten();
                    native_valid &= validity.is_some_and(|(valid, required, value_missing)| {
                        valid && !(required && value_missing)
                    });
                }
                let required_values_present = scoped_regions
                    .iter()
                    .flat_map(|region| region.targets.iter())
                    .all(|target| {
                        values
                            .accessibility
                            .interactive
                            .iter()
                            .find(|control| control.reference == target.reference)
                            .is_some_and(|control| !control.required || !control.empty)
                    });
                let valid = alerts.is_empty() && native_valid && required_values_present;
                steps.push(step(
                    &plan,
                    TaskPlanOperation::ValidateForm,
                    if valid {
                        "succeeded"
                    } else {
                        "verification-failed"
                    },
                    (!valid).then(|| "native form validity or required state failed".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    receipt: TaskExecutionReceipt::from_plan(&plan),
                    status: if valid {
                        "succeeded"
                    } else {
                        "verification-failed"
                    }
                    .into(),
                    phase: "validation".into(),
                    mutation_possible: false,
                    source_revision: observation.revision,
                    current_revision: values.accessibility.interactive.first().map_or(
                        observation.revision,
                        |_| {
                            self.page_revision
                                .load(std::sync::atomic::Ordering::Relaxed)
                        },
                    ),
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
                let fields = match live_bindings.bound_fields(&plan, &task.inputs) {
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
                    .map(|(target, _, value)| (target.as_str(), value.as_str()))
                    .collect::<Vec<_>>();
                let form = match bounded(
                    self.fill_form_with_expected_revision(&borrowed, Some(observation.revision)),
                    task.limits.timeout_ms,
                )
                .await
                {
                    Ok(form) => form,
                    Err(error) => {
                        let current_revision = self
                            .page_revision
                            .load(std::sync::atomic::Ordering::Relaxed);
                        return Ok(form_fill_failure_result(
                            task,
                            &plan,
                            observation.revision,
                            current_revision,
                            steps,
                            None,
                            error.to_string(),
                        ));
                    }
                };
                let after = match bounded(self.inspect_page(), task.limits.timeout_ms).await {
                    Ok(after) => after,
                    Err(error) => {
                        let current_revision = self
                            .page_revision
                            .load(std::sync::atomic::Ordering::Relaxed);
                        return Ok(form_fill_failure_result(
                            task,
                            &plan,
                            observation.revision,
                            current_revision,
                            steps,
                            Some(form),
                            error.to_string(),
                        ));
                    }
                };
                let values =
                    match bounded(self.observe_with_form_values(), task.limits.timeout_ms).await {
                        Ok(values) => values,
                        Err(error) => {
                            let current_revision = self
                                .page_revision
                                .load(std::sync::atomic::Ordering::Relaxed);
                            return Ok(form_fill_failure_result(
                                task,
                                &plan,
                                observation.revision,
                                current_revision,
                                steps,
                                Some(form),
                                format!("form fill verification failed: {error}"),
                            ));
                        }
                    };
                let verified = form.filled == form.total && form_values_match(&values, &fields);
                let current_revision = self
                    .page_revision
                    .load(std::sync::atomic::Ordering::Relaxed);
                steps.push(step(
                    &plan,
                    TaskPlanOperation::FillInputs,
                    if verified {
                        "succeeded"
                    } else {
                        "indeterminate"
                    },
                    (!verified).then(|| "form values did not match requested values".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    receipt: TaskExecutionReceipt::from_plan(&plan),
                    status: if verified {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "mutation-verification".into(),
                    mutation_possible: form.filled > 0,
                    source_revision: observation.revision,
                    current_revision,
                    steps,
                    retry: if verified {
                        retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
                    } else {
                        retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run")
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
                let target_reference = match live_bindings.reference_for_name(&plan, submit_name) {
                    Ok(reference) => reference,
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
                    self.click_with_revision(target_reference, observation.revision),
                    task.limits.timeout_ms,
                )
                .await;
                let after = match bounded(self.inspect_page(), task.limits.timeout_ms).await {
                    Ok(after) => after,
                    Err(error) => {
                        let current_revision = self
                            .page_revision
                            .load(std::sync::atomic::Ordering::Relaxed);
                        return Ok(mutation_failure_result(
                            task,
                            &plan,
                            (observation.revision, current_revision),
                            steps,
                            TaskPlanOperation::SubmitForm,
                            "submit-verification",
                            format!("post-action observation failed: {error}"),
                        ));
                    }
                };
                let succeeded = outcome.is_ok();
                steps.push(step(
                    &plan,
                    TaskPlanOperation::SubmitForm,
                    if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    },
                    (!succeeded).then(|| "submit action failed".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    receipt: TaskExecutionReceipt::from_plan(&plan),
                    status: if succeeded {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "submit-verification".into(),
                    mutation_possible: true,
                    source_revision: observation.revision,
                    current_revision: self
                        .page_revision
                        .load(std::sync::atomic::Ordering::Relaxed)
                        .max(after.revision),
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
            _ => unreachable!(),
        }
    }
}

impl BrowserSession {
    /// Execute a bounded navigation task against one caller-observed revision.
    async fn execute_navigation_task_unchecked(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let plan = self.compile_live_task(task).await?;
        if let Some(detail) = unsupported_runtime_capability(&plan) {
            return Ok(preflight_result(task, &plan, expected_revision, detail));
        }
        if task.task != TaskKind::NavigationFollow {
            return Ok(preflight_result(
                task,
                &plan,
                expected_revision,
                "navigation execution only supports navigation.follow tasks",
            ));
        }
        if let Some(detail) = compiled_revision_mismatch(&plan, expected_revision) {
            return Ok(preflight_result(task, &plan, expected_revision, &detail));
        }
        let expected_revision = plan.source_ir_revision;
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
                let verified = navigation_destination_matches(url, &outcome.page.url);
                steps.push(step(
                    &plan,
                    TaskPlanOperation::FollowNavigation,
                    if verified {
                        "succeeded"
                    } else {
                        "indeterminate"
                    },
                    (!verified).then(|| "navigation destination was not verified".into()),
                ));
                Ok(TaskExecutionResult {
                    task: task.task,
                    receipt: TaskExecutionReceipt::from_plan(&plan),
                    status: if verified {
                        "succeeded"
                    } else {
                        "indeterminate"
                    }
                    .into(),
                    phase: "navigation-verification".into(),
                    mutation_possible: true,
                    source_revision: expected_revision,
                    current_revision: outcome.current_revision,
                    steps,
                    retry: if verified {
                        retry_guidance(RetryClassification::SafeImmediate, "inspect_page")
                    } else {
                        retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run")
                    },
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
                    receipt: TaskExecutionReceipt::from_plan(&plan),
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
    async fn finalize_task_result(
        &self,
        task: &GlassTask,
        mut result: TaskExecutionResult,
    ) -> BrowserResult<TaskExecutionResult> {
        if result.status == "succeeded" {
            let observation = match bounded(self.inspect_page(), task.limits.timeout_ms).await {
                Ok(observation) => observation,
                Err(error) => {
                    let current_revision = self
                        .page_revision
                        .load(std::sync::atomic::Ordering::Relaxed);
                    return Ok(postcondition_failure_result(
                        result,
                        current_revision,
                        format!("postcondition observation failed: {error}"),
                    ));
                }
            };
            let regions = match scoped_regions_for_observation(&observation, task) {
                Ok(regions) => regions,
                Err(_error)
                    if matches!(
                        task.task,
                        TaskKind::NavigationFollow
                            | TaskKind::DialogInspect
                            | TaskKind::DialogConfirm
                            | TaskKind::DialogCancel
                    ) =>
                {
                    Vec::new()
                }
                Err(error) => {
                    return Ok(postcondition_failure_result(
                        result,
                        self.page_revision
                            .load(std::sync::atomic::Ordering::Relaxed)
                            .max(observation.revision),
                        format!("postcondition scope reconciliation failed: {error}"),
                    ));
                }
            };
            let pending_dialog = self.pending_dialog().await;
            result.receipt.postconditions = effective_postconditions(task)
                .iter()
                .map(|postcondition| TaskPostconditionReceipt {
                    kind: postcondition.kind,
                    held: postcondition_holds(
                        postcondition,
                        &observation,
                        &regions,
                        result.source_revision,
                        result.extraction.as_ref(),
                        pending_dialog.as_ref(),
                    ),
                })
                .collect();
            if result
                .receipt
                .postconditions
                .iter()
                .any(|postcondition| !postcondition.held)
            {
                result.status = "indeterminate".into();
                result.phase = "postcondition-verification".into();
                result.current_revision = self
                    .page_revision
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .max(observation.revision);
                result.retry =
                    retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run");
                if let Some(last) = result.steps.last_mut() {
                    last.status = "indeterminate".into();
                    last.detail = Some("compiled postcondition did not hold".into());
                }
            }
        }
        Ok(result)
    }
}

impl BrowserSession {
    /// Execute a validated form task and enforce authored postconditions.
    pub async fn execute_form_task(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let result = self
            .execute_form_task_unchecked(task, expected_revision, confirmed)
            .await?;
        self.finalize_task_result(task, result).await
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
        task.validate()?;

        let result = match task.task {
            TaskKind::NavigationFollow => {
                Box::pin(self.execute_navigation_task(task, expected_revision, confirmed)).await?
            }
            TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => {
                Box::pin(self.execute_dialog_task(task, expected_revision, confirmed)).await?
            }
            _ => Box::pin(self.execute_form_task(task, expected_revision, confirmed)).await?,
        };
        Ok(result)
    }
}

impl BrowserSession {
    /// Execute a bounded navigation task and enforce authored postconditions.
    pub async fn execute_navigation_task(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let result = self
            .execute_navigation_task_unchecked(task, expected_revision, confirmed)
            .await?;
        self.finalize_task_result(task, result).await
    }
}

impl BrowserSession {
    /// Inspect or resolve one pending JavaScript dialog through the Task Protocol.
    async fn execute_dialog_task_unchecked(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let plan = self.compile_live_task(task).await?;
        if let Some(detail) = unsupported_runtime_capability(&plan) {
            return Ok(preflight_result(task, &plan, expected_revision, detail));
        }
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
        if let Some(detail) = compiled_revision_mismatch(&plan, expected_revision) {
            return Ok(preflight_result(task, &plan, expected_revision, &detail));
        }
        let expected_revision = plan.source_ir_revision;
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
                receipt: TaskExecutionReceipt::from_plan(&plan),
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
            TaskKind::DialogConfirm => self.accept_dialog_with_revision(expected_revision).await,
            TaskKind::DialogCancel => self.dismiss_dialog_with_revision(expected_revision).await,
            _ => unreachable!(),
        };
        let still_pending = !wait_for_dialog_closed(self, task.limits.timeout_ms).await;
        let succeeded = dialog_action_succeeded(&action, still_pending);
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
            receipt: TaskExecutionReceipt::from_plan(&plan),
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

impl BrowserSession {
    /// Execute a dialog task and enforce authored postconditions.
    pub async fn execute_dialog_task(
        &self,
        task: &GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<TaskExecutionResult> {
        let result = self
            .execute_dialog_task_unchecked(task, expected_revision, confirmed)
            .await?;
        self.finalize_task_result(task, result).await
    }
}

fn dialog_action_succeeded(action: &BrowserResult<()>, still_pending: bool) -> bool {
    action.is_ok() && !still_pending
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

fn form_fill_failure_result(
    task: &GlassTask,
    plan: &TaskExecutionPlan,
    source_revision: u64,
    current_revision: u64,
    mut steps: Vec<TaskStepResult>,
    form: Option<FillFormOutcome>,
    detail: String,
) -> TaskExecutionResult {
    steps.push(step(
        plan,
        TaskPlanOperation::FillInputs,
        "indeterminate",
        Some(detail),
    ));
    TaskExecutionResult {
        task: task.task,
        receipt: TaskExecutionReceipt::from_plan(plan),
        status: "indeterminate".into(),
        phase: "mutation-verification".into(),
        mutation_possible: true,
        source_revision,
        current_revision,
        steps,
        retry: retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run"),
        form,
        extraction: None,
        dialog: None,
        alerts: Vec::new(),
    }
}
async fn wait_for_dialog_closed(session: &BrowserSession, timeout_ms: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if session.pending_dialog().await.is_none() {
            return true;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return false;
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(10))).await;
    }
}

fn mutation_failure_result(
    task: &GlassTask,
    plan: &TaskExecutionPlan,
    revisions: (u64, u64),
    mut steps: Vec<TaskStepResult>,
    operation: TaskPlanOperation,
    phase: &str,
    detail: impl Into<String>,
) -> TaskExecutionResult {
    steps.push(step(plan, operation, "indeterminate", Some(detail.into())));
    TaskExecutionResult {
        task: task.task,
        receipt: TaskExecutionReceipt::from_plan(plan),
        status: "indeterminate".into(),
        phase: phase.into(),
        mutation_possible: true,
        source_revision: revisions.0,
        current_revision: revisions.1,
        steps,
        retry: retry_guidance(RetryClassification::UnsafeUntilReconciled, "recover_run"),
        form: None,
        extraction: None,
        dialog: None,
        alerts: Vec::new(),
    }
}
fn postcondition_failure_result(
    mut result: TaskExecutionResult,
    current_revision: u64,
    detail: impl Into<String>,
) -> TaskExecutionResult {
    result.status = "indeterminate".into();
    result.phase = "postcondition-verification".into();
    result.current_revision = current_revision;
    result.retry = retry_guidance(
        if result.mutation_possible {
            RetryClassification::UnsafeUntilReconciled
        } else {
            RetryClassification::SafeAfterReconcile
        },
        if result.mutation_possible {
            "recover_run"
        } else {
            "inspect_page"
        },
    );
    if let Some(last) = result.steps.last_mut() {
        last.status = "indeterminate".into();
        last.detail = Some(detail.into());
    }
    result
}

async fn aria_boolean_state(
    session: &BrowserSession,
    reference: &str,
    attribute: &str,
) -> Option<bool> {
    let reference = parse_revisioned_reference(reference).ok().flatten()?;
    let object_id = session
        .cdp
        .resolve_node_object(None, Some(reference.backend_dom_node_id))
        .await
        .ok()?;
    let function = format!(
        "function(){{const value=this.getAttribute({attribute:?});return value === null ? null : value === 'true';}}"
    );
    let result = session
        .cdp
        .send(
            "Runtime.callFunctionOn",
            Some(json!({
                "objectId": object_id,
                "functionDeclaration": function,
                "returnByValue": true,
                "awaitPromise": false,
            })),
        )
        .await
        .ok();
    let _ = session
        .cdp
        .send(
            "Runtime.releaseObject",
            Some(json!({"objectId": object_id})),
        )
        .await;
    result
        .as_ref()
        .and_then(|value| value["result"]["value"].as_bool())
}

fn navigation_destination_matches(requested: &str, actual: &str) -> bool {
    let requested = requested.split('#').next().unwrap_or(requested);
    let actual = actual.split('#').next().unwrap_or(actual);
    requested == actual || requested.trim_end_matches('/') == actual.trim_end_matches('/')
}

async fn native_control_validity(
    session: &BrowserSession,
    reference: &str,
) -> Option<(bool, bool, bool)> {
    let reference = parse_revisioned_reference(reference).ok().flatten()?;
    let object_id = session
        .cdp
        .resolve_node_object(None, Some(reference.backend_dom_node_id))
        .await
        .ok()?;
    let result = session
        .cdp
        .send(
            "Runtime.callFunctionOn",
            Some(json!({
                "objectId": object_id,
                "functionDeclaration": "function(){const validity=this.validity;return {valid: validity ? validity.valid : true, required: !!this.required, valueMissing: !!(validity && validity.valueMissing)};}",
                "returnByValue": true,
                "awaitPromise": false,
            })),
        )
        .await
        .ok();
    let _ = session
        .cdp
        .send(
            "Runtime.releaseObject",
            Some(json!({"objectId": object_id})),
        )
        .await;
    let result = result?;
    let value = result.get("result")?.get("value")?;
    Some((
        value.get("valid")?.as_bool()?,
        value.get("required")?.as_bool()?,
        value.get("valueMissing")?.as_bool()?,
    ))
}

async fn wait_for_aria_true(
    session: &BrowserSession,
    reference: &str,
    attribute: &str,
    timeout_ms: u64,
) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if aria_boolean_state(session, reference, attribute).await == Some(true) {
            return true;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return false;
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(10))).await;
    }
}

async fn wait_for_semantic_page_change(
    session: &BrowserSession,
    before: &InspectPageResult,
    mut after: InspectPageResult,
    timeout_ms: u64,
) -> BrowserResult<InspectPageResult> {
    if semantic_page_changed(before, &after) {
        return Ok(after);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(after);
        }
        tokio::time::sleep(remaining.min(Duration::from_millis(10))).await;
        after = match tokio::time::timeout(remaining, session.inspect_page()).await {
            Ok(Ok(next)) => next,
            _ => return Ok(after),
        };
        if semantic_page_changed(before, &after) {
            return Ok(after);
        }
    }
}

fn retry_guidance(classification: RetryClassification, operation: &str) -> RetryGuidance {
    RetryGuidance {
        classification,
        recommended_operation: operation.into(),
    }
}

fn compiled_revision_mismatch(plan: &TaskExecutionPlan, caller_revision: u64) -> Option<String> {
    let regression = plan.source_ir_revision < caller_revision;
    let incompatible_drift = match plan.revision {
        TaskRevisionPolicy::Exact => plan.source_ir_revision != caller_revision,
        TaskRevisionPolicy::Compatible => {
            plan.source_ir_revision != caller_revision && plan.risk != TaskRiskClass::ReadOnly
        }
        TaskRevisionPolicy::Reextract => false,
    };
    (regression || incompatible_drift).then(|| {
        format!(
            "compiled Web IR revision {} is not safe for caller-observed revision {} under {:?} policy; no browser action was dispatched",
            plan.source_ir_revision, caller_revision, plan.revision
        )
    })
}

fn preflight_result(
    task: &GlassTask,
    plan: &TaskExecutionPlan,
    revision: u64,
    detail: &str,
) -> TaskExecutionResult {
    TaskExecutionResult {
        task: task.task,
        receipt: TaskExecutionReceipt::from_plan(plan),
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

impl BrowserSession {
    async fn execute_open_menu_task(
        &self,
        task: &GlassTask,
        plan: &TaskExecutionPlan,
        observation: &InspectPageResult,
        live_bindings: &LiveTaskBindings,
    ) -> BrowserResult<TaskExecutionResult> {
        let Some(menu_name) = task.inputs.get("menu") else {
            return Ok(preflight_result(
                task,
                plan,
                observation.revision,
                "navigation.openMenu requires the semantic menu input",
            ));
        };
        let target_reference = match live_bindings.reference_for_name(plan, menu_name) {
            Ok(reference) => reference,
            Err(error) => {
                return Ok(preflight_result(task, plan, observation.revision, &error));
            }
        };
        let outcome = bounded(
            self.click_with_revision(target_reference, observation.revision),
            task.limits.timeout_ms,
        )
        .await;
        let after = match bounded(self.inspect_page(), task.limits.timeout_ms).await {
            Ok(after) => after,
            Err(error) => {
                let current_revision = self
                    .page_revision
                    .load(std::sync::atomic::Ordering::Relaxed);
                return Ok(mutation_failure_result(
                    task,
                    plan,
                    (observation.revision, current_revision),
                    vec![step(
                        plan,
                        TaskPlanOperation::ObserveScope,
                        "succeeded",
                        None,
                    )],
                    TaskPlanOperation::OpenMenu,
                    "navigation-verification",
                    format!("post-action observation failed: {error}"),
                ));
            }
        };
        let menu_open = wait_for_aria_true(
            self,
            target_reference,
            "aria-expanded",
            task.limits.timeout_ms,
        )
        .await;
        let succeeded = outcome.is_ok() && menu_open;
        let detail = if let Err(error) = &outcome {
            Some(format!("menu control click was not dispatched: {error}"))
        } else if !menu_open {
            Some("menu expanded state was not observed after the click".into())
        } else {
            None
        };
        let steps = vec![
            step(plan, TaskPlanOperation::ObserveScope, "succeeded", None),
            step(
                plan,
                TaskPlanOperation::OpenMenu,
                if succeeded {
                    "succeeded"
                } else {
                    "indeterminate"
                },
                detail,
            ),
        ];
        Ok(TaskExecutionResult {
            task: task.task,
            receipt: TaskExecutionReceipt::from_plan(plan),
            status: if succeeded {
                "succeeded"
            } else {
                "indeterminate"
            }
            .into(),
            phase: "mutation-verification".into(),
            mutation_possible: outcome.is_ok(),
            source_revision: observation.revision,
            current_revision: self
                .page_revision
                .load(std::sync::atomic::Ordering::Relaxed)
                .max(after.revision),
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
        .filter(|region| {
            region.label.eq_ignore_ascii_case(region_name)
                && task
                    .scope
                    .entity_kind
                    .is_none_or(|kind| semantic_region_matches_entity(region, kind))
                && task.scope.entity_name.as_deref().is_none_or(|entity_name| {
                    region.label.eq_ignore_ascii_case(entity_name)
                        || region
                            .targets
                            .iter()
                            .any(|target| target.name.eq_ignore_ascii_case(entity_name))
                })
        })
        .collect::<Vec<_>>();
    match regions.len() {
        1 => Ok(regions),
        0 => Err(format!("semantic region not found: {region_name}").into()),
        _ => Err(format!("semantic region is ambiguous: {region_name}").into()),
    }
}

fn semantic_region_matches_entity(
    region: &SemanticRegion,
    kind: crate::web_ir::WebIrEntityKind,
) -> bool {
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
        WebIrEntityKind::Field => region.targets.iter().any(|target| {
            target.input_type.is_some()
                || matches!(target.role.as_str(), "textbox" | "combobox" | "listbox")
        }),
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
fn form_values_match(values: &super::PageContext, fields: &[(String, String, String)]) -> bool {
    fields.iter().all(|(reference, name, expected)| {
        let Some(control) = values.accessibility.interactive.iter().find(|control| {
            control.reference == *reference || control.name.eq_ignore_ascii_case(name)
        }) else {
            return false;
        };
        if matches!(control.role.as_str(), "checkbox" | "radio")
            || control
                .input_type
                .as_deref()
                .is_some_and(|input_type| matches!(input_type, "checkbox" | "radio"))
        {
            let expected_checked =
                !expected.is_empty() && expected != "false" && expected != "0" && expected != "off";
            control.checked == Some(expected_checked)
        } else if matches!(control.role.as_str(), "combobox" | "listbox") {
            control.selected_option.as_deref() == Some(expected.as_str())
                || control.value.as_deref() == Some(expected.as_str())
        } else {
            control.value.as_deref() == Some(expected.as_str())
        }
    })
}

fn pagination_next_is_usable(regions: &[&SemanticRegion], next_name: &str) -> bool {
    regions
        .iter()
        .flat_map(|region| region.targets.iter())
        .any(|target| {
            target.name.eq_ignore_ascii_case(next_name)
                && matches!(target.role.as_str(), "button" | "link" | "tab")
        })
}
fn merge_pagination_extraction(
    aggregate: &mut Option<StructuredExtractionResult>,
    page: StructuredExtractionResult,
    max_items: usize,
    max_bytes: usize,
) {
    let Some(result) = aggregate.as_mut() else {
        let mut initial = page;
        let observed_items = initial.limits.observed_items;
        let mut seen = std::collections::BTreeSet::new();
        let mut items = Vec::new();
        for item in initial.record_items.drain(..) {
            let duplicate = item
                .entity_ids
                .first()
                .is_some_and(|entity_id| !seen.insert(entity_id.clone()));
            if duplicate {
                continue;
            }
            items.push(item);
            if items.len() >= max_items {
                break;
            }
        }
        initial.record_items = items;
        initial.truncated |= initial.record_items.len() < observed_items;
        initial.limits.observed_items = initial.record_items.len();
        initial.limits.truncated = initial.truncated;
        initial.limits.serialized_bytes =
            serde_json::to_vec(&initial).map_or(0, |bytes| bytes.len());
        *aggregate = Some(initial);
        return;
    };
    result.truncated |= page.truncated;
    if page.continuation.is_some() {
        result.continuation = page.continuation;
    }
    result.source_revision = page.source_revision;
    result.source_route = page.source_route;
    let mut seen = result
        .record_items
        .iter()
        .filter_map(|item| item.entity_ids.first().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    for item in page.record_items {
        if result.record_items.len() >= max_items {
            result.truncated = true;
            break;
        }
        let duplicate = item
            .entity_ids
            .first()
            .is_some_and(|entity_id| !seen.insert(entity_id.clone()));
        if duplicate {
            continue;
        }
        result.record_items.push(StructuredExtractionRecord {
            index: result.record_items.len(),
            ..item
        });
        if serde_json::to_vec(result).is_ok_and(|bytes| bytes.len() > max_bytes) {
            result.record_items.pop();
            result.truncated = true;
            break;
        }
    }
    result.limits.observed_items = result.record_items.len();
    result.limits.serialized_bytes = serde_json::to_vec(result).map_or(0, |bytes| bytes.len());
    result.limits.truncated = result.truncated;
}

fn extraction_matches_observation(
    extraction: &StructuredExtractionResult,
    observation: &InspectPageResult,
) -> bool {
    extraction.source_revision == observation.revision
        && extraction.source_route.target_id == observation.page.target_id
        && extraction.source_route.frame_id == observation.page.frame_id
        && diagnostic_route_url(&extraction.source_route.url)
            == diagnostic_route_url(&observation.page.url)
}

fn diagnostic_route_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return url.split(['?', '#']).next().unwrap_or_default().to_owned();
    };
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
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
                || before.structured_records != after.structured_records
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
fn unsupported_runtime_capability(plan: &TaskExecutionPlan) -> Option<&'static str> {
    const SUPPORTED: &[TaskRuntimeCapability] = &[
        TaskRuntimeCapability::Observe,
        TaskRuntimeCapability::Read,
        TaskRuntimeCapability::Mutate,
        TaskRuntimeCapability::Navigate,
        TaskRuntimeCapability::Extract,
        TaskRuntimeCapability::Dialog,
        TaskRuntimeCapability::Pagination,
        TaskRuntimeCapability::VerifyEntityState,
    ];
    plan.required_runtime_capabilities
        .iter()
        .find(|capability| !SUPPORTED.contains(capability))
        .map(|_| "compiled plan requires a capability unsupported by this browser runtime")
}

fn postcondition_holds(
    postcondition: &crate::task_protocol::TaskPostcondition,
    observation: &InspectPageResult,
    _regions: &[&SemanticRegion],
    source_revision: u64,
    extraction: Option<&StructuredExtractionResult>,
    dialog: Option<&PendingDialog>,
) -> bool {
    match postcondition.kind {
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
        TaskPostconditionKind::NavigationOccurred => observation.revision > source_revision,
        TaskPostconditionKind::PageKind => postcondition.expected.as_ref().is_none_or(|expected| {
            format!("{:?}", observation.page.kind).eq_ignore_ascii_case(expected)
        }),
        TaskPostconditionKind::DialogClosed => dialog.is_none(),
        TaskPostconditionKind::RecordsExtracted => extraction.is_some_and(|result| {
            postcondition.expected.as_ref().is_none_or(|expected| {
                expected
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|minimum| result.record_items.len() >= minimum)
            })
        }),
        TaskPostconditionKind::EntityState => postcondition
            .expected
            .as_deref()
            .is_some_and(|expected| entity_state_holds(observation, expected)),
    }
}

fn entity_state_holds(observation: &InspectPageResult, expected: &str) -> bool {
    let Some((selector, expected_value)) = expected.rsplit_once('=') else {
        return false;
    };
    let Some((entity_name, state)) = selector.rsplit_once('.') else {
        return false;
    };
    let Some(expected_value) = (match expected_value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }) else {
        return false;
    };
    let candidates = observation
        .regions
        .iter()
        .flat_map(|region| &region.targets)
        .filter(|target| target.name.eq_ignore_ascii_case(entity_name))
        .collect::<Vec<_>>();
    let [target] = candidates.as_slice() else {
        return false;
    };
    let observed = match state {
        "disabled" => target.disabled,
        "readOnly" => target.read_only,
        "required" => target.required,
        "checked" => target.checked,
        "empty" => target.empty,
        _ => None,
    };
    observed == Some(expected_value)
}
fn remaining_timeout(deadline: tokio::time::Instant) -> u64 {
    deadline
        .saturating_duration_since(tokio::time::Instant::now())
        .as_millis()
        .clamp(1, u64::MAX as u128) as u64
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
        SemanticStructuredRecord, SemanticTarget,
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
            disabled: None,
            read_only: None,
            required: None,
            checked: None,
            empty: None,
        }
    }

    fn region(label: &str, targets: Vec<SemanticTarget>) -> SemanticRegion {
        SemanticRegion {
            id: label.into(),
            kind: SemanticRegionKind::Form,
            label: label.into(),
            interactive_count: targets.len(),
            item_count: None,
            structured_records: Vec::new(),
            confidence: SemanticConfidence::Exact,
            evidence: Vec::new(),
            targets,
            expansion: None,
        }
    }

    fn observation(revision: u64, regions: Vec<SemanticRegion>) -> InspectPageResult {
        InspectPageResult {
            page: SemanticPage {
                kind: SemanticPageKind::Form,
                title: "Checkout".into(),
                url: "https://example.test/checkout".into(),
                target_id: "page".into(),
                frame_id: "frame".into(),
                confidence: SemanticConfidence::Exact,
                evidence: Vec::new(),
            },
            revision,
            regions,
            limits: Default::default(),
            focused_target: None,
            alerts: Vec::new(),
        }
    }

    #[test]
    fn pagination_noop_with_usable_next_is_not_terminal() {
        let mut next = target("Next", "next");
        next.role = "button".into();
        let page = region("Results", vec![next]);

        assert!(pagination_next_is_usable(&[&page], "next"));
    }

    #[test]
    fn pagination_noop_with_disappeared_next_is_terminal() {
        let page = region("Results", Vec::new());

        assert!(!pagination_next_is_usable(&[&page], "Next"));
    }

    #[test]
    fn structured_record_changes_count_as_page_changes() {
        let before = region("Results", Vec::new());
        let mut after = before.clone();
        assert!(!semantic_regions_changed(
            std::slice::from_ref(&before),
            std::slice::from_ref(&after)
        ));
        after.structured_records.push(SemanticStructuredRecord {
            fields: BTreeMap::from([("Status".into(), "Ready".into())]),
        });
        assert!(semantic_regions_changed(
            std::slice::from_ref(&before),
            std::slice::from_ref(&after)
        ));
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
    fn entity_state_postcondition_is_verified_from_one_unique_target() {
        let mut authored = task();
        authored.postconditions = vec![crate::task_protocol::TaskPostcondition {
            kind: TaskPostconditionKind::EntityState,
            expected: Some("Email.checked=true".into()),
        }];
        authored.validate().unwrap();
        let mut checked = target("Email", "r7:b1");
        checked.checked = Some(true);
        let observation = observation(7, vec![region("Checkout", vec![checked])]);
        assert!(entity_state_holds(&observation, "Email.checked=true"));
        assert!(!entity_state_holds(&observation, "Email.checked=false"));
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
    fn live_bindings_fail_closed_without_collecting_duplicate_candidates() {
        let authored = task();
        let plan = compile_task(&authored, &crate::task_compiler::test_compiler_ir()).unwrap();
        let unique = region("Checkout", vec![target("Email", "r7:b1")]);
        let bindings = LiveTaskBindings::resolve(&plan, &[&unique], 7).unwrap();
        assert_eq!(
            bindings.reference_for_name(&plan, "Email").unwrap(),
            "r7:b1"
        );

        let duplicate = region(
            "Checkout",
            vec![target("Email", "r7:b1"), target("Email", "r7:b2")],
        );
        let error = LiveTaskBindings::resolve(&plan, &[&duplicate], 7)
            .err()
            .expect("duplicate binding must fail");
        assert!(error.contains("multiple revision-bound browser targets"));

        let missing = region("Checkout", Vec::new());
        let error = LiveTaskBindings::resolve(&plan, &[&missing], 7)
            .err()
            .expect("missing binding must fail");
        assert!(error.contains("did not resolve to exactly one"));
    }

    #[test]
    fn form_fill_failure_returns_recovery_result() {
        let task = task();
        let plan = compile_task(&task, &crate::task_compiler::test_compiler_ir()).unwrap();
        let result = form_fill_failure_result(
            &task,
            &plan,
            7,
            8,
            Vec::new(),
            None,
            "task execution exceeded its timeout budget".into(),
        );
        assert_eq!(result.status, "indeterminate");
        assert!(result.mutation_possible);
        assert_eq!(result.current_revision, 8);
        assert_eq!(result.steps[0].status, "indeterminate");
        assert_eq!(
            result.retry.classification,
            RetryClassification::UnsafeUntilReconciled
        );
    }

    #[test]
    fn execution_receipt_never_serializes_authored_values_or_browser_references() {
        let mut task = task();
        task.inputs
            .insert("Email".into(), "private@example.test".into());
        let plan = compile_task(&task, &crate::task_compiler::test_compiler_ir()).unwrap();
        let encoded = serde_json::to_string(&TaskExecutionReceipt::from_plan(&plan)).unwrap();
        assert!(!encoded.contains("private@example.test"));
        assert!(!encoded.contains("r7:"));
        assert!(encoded.contains("bindingCandidateEntityIds"));
    }

    #[test]
    fn stale_dialog_action_is_indeterminate_without_dialog_payload() {
        let action: BrowserResult<()> = Err("stale revision".into());
        assert!(!dialog_action_succeeded(&action, true));

        let mut task = task();
        task.task = TaskKind::DialogConfirm;
        task.inputs.clear();
        let plan = compile_task(&task, &crate::task_compiler::test_compiler_ir()).unwrap();
        let result = mutation_failure_result(
            &task,
            &plan,
            (7, 8),
            Vec::new(),
            TaskPlanOperation::ConfirmDialog,
            "dialog-verification",
            "dialog outcome was not verified",
        );
        assert_eq!(result.status, "indeterminate");
        assert!(result.mutation_possible);
        assert_eq!(result.current_revision, 8);
        assert!(result.dialog.is_none());
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("secret dialog message"));
        assert!(!serialized.contains("\"dialog\":"));
    }

    #[test]
    fn mutation_failure_returns_bounded_recovery_result() {
        let task = task();
        let plan = compile_task(&task, &crate::task_compiler::test_compiler_ir()).unwrap();
        let result = mutation_failure_result(
            &task,
            &plan,
            (7, 9),
            Vec::new(),
            TaskPlanOperation::SubmitForm,
            "submit-verification",
            "post-action observation failed",
        );
        assert_eq!(result.status, "indeterminate");
        assert_eq!(result.phase, "submit-verification");
        assert!(result.mutation_possible);
        assert_eq!(result.source_revision, 7);
        assert_eq!(result.current_revision, 9);
        assert_eq!(
            result.steps[0].detail.as_deref(),
            Some("post-action observation failed")
        );
        assert_eq!(
            result.retry.classification,
            RetryClassification::UnsafeUntilReconciled
        );
    }

    #[test]
    fn postcondition_observation_failure_preserves_recovery_guidance() {
        let task = task();
        let plan = compile_task(&task, &crate::task_compiler::test_compiler_ir()).unwrap();
        let initial = mutation_failure_result(
            &task,
            &plan,
            (7, 9),
            Vec::new(),
            TaskPlanOperation::SubmitForm,
            "submit-verification",
            "dispatch completed but verification was unavailable",
        );
        let result = postcondition_failure_result(initial, 10, "postcondition observation failed");
        assert_eq!(result.status, "indeterminate");
        assert_eq!(result.phase, "postcondition-verification");
        assert_eq!(result.current_revision, 10);
        assert_eq!(
            result.retry.classification,
            RetryClassification::UnsafeUntilReconciled
        );
        assert_eq!(
            result.steps[0].detail.as_deref(),
            Some("postcondition observation failed")
        );
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

    #[test]
    fn navigation_destination_matching_ignores_fragment_and_trailing_slash() {
        assert!(navigation_destination_matches(
            "https://example.test/account/#section",
            "https://example.test/account"
        ));
        assert!(!navigation_destination_matches(
            "https://example.test/account",
            "https://example.test/settings"
        ));
    }

    #[test]
    fn compiled_revision_policies_fail_closed_or_reconcile_explicitly() {
        let exact =
            crate::task_compiler::compile_task(&task(), &crate::task_compiler::test_compiler_ir())
                .unwrap();
        assert!(compiled_revision_mismatch(&exact, exact.source_ir_revision).is_none());
        assert!(compiled_revision_mismatch(&exact, exact.source_ir_revision + 1).is_some());

        let mut compatible_read = task();
        compatible_read.task = TaskKind::RegionExtract;
        compatible_read.scope.entity_kind = Some(crate::web_ir::WebIrEntityKind::Region);
        compatible_read.inputs.clear();
        compatible_read.risk = TaskRiskClass::ReadOnly;
        compatible_read.revision = TaskRevisionPolicy::Compatible;
        let compatible_read = crate::task_compiler::compile_task(
            &compatible_read,
            &crate::task_compiler::test_compiler_ir(),
        )
        .unwrap();
        assert!(
            compiled_revision_mismatch(&compatible_read, compatible_read.source_ir_revision - 1)
                .is_none()
        );

        let mut compatible_mutation = task();
        compatible_mutation.revision = TaskRevisionPolicy::Compatible;
        let compatible_mutation = crate::task_compiler::compile_task(
            &compatible_mutation,
            &crate::task_compiler::test_compiler_ir(),
        )
        .unwrap();
        assert!(
            compiled_revision_mismatch(
                &compatible_mutation,
                compatible_mutation.source_ir_revision - 1
            )
            .is_some()
        );

        let mut reextract = task();
        reextract.revision = TaskRevisionPolicy::Reextract;
        let reextract = crate::task_compiler::compile_task(
            &reextract,
            &crate::task_compiler::test_compiler_ir(),
        )
        .unwrap();
        assert!(reextract.confirmation_required);
        assert!(compiled_revision_mismatch(&reextract, reextract.source_ir_revision - 1).is_none());
    }
    #[test]
    fn live_task_evidence_sources_cover_boundaries_and_bridge() {
        let sources = live_task_evidence_sources();
        for source in [
            EvidenceSource::Frames,
            EvidenceSource::ShadowDom,
            EvidenceSource::Svg,
            EvidenceSource::CanvasDetection,
            EvidenceSource::MediaMetadata,
            EvidenceSource::EmbeddedDocument,
            EvidenceSource::Pdf,
            EvidenceSource::BrowserNative,
            EvidenceSource::Bridge,
            EvidenceSource::BoundedProbe,
        ] {
            assert!(sources.contains(&source), "missing live source {source:?}");
        }
        let mut sorted = sources.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), sources.len());
    }
}

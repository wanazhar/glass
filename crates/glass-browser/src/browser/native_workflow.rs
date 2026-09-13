//! Native execution of the declarative workflow contract.
//!
//! Workflow state is deliberately kept in the shared session types.  This
//! module supplies the native browser driver for that state machine so a
//! workflow can run in one local engine, including safe checkpoint resume.

use super::native_batch;
use super::runtime::BrowserRuntimeSession;
use crate::browser::policy::BrowserPolicy;
use crate::browser::session::{
    ActionOutcome, BatchMode, BatchOutcome, BatchStepOutcome, BrowserResult,
    SemanticIntentExecutionStatus, WorkflowBranchDecision, WorkflowCheckpoint,
    WorkflowCheckpointPage, WorkflowCheckpointStep, WorkflowDefinition, WorkflowIntentEvidence,
    WorkflowOutput, WorkflowOutputEvidence, WorkflowOutputSource, WorkflowResumeError,
    WorkflowRunResult, WorkflowRunStatus, WorkflowStep, WorkflowStepRecord, WorkflowStepState,
    WorkflowTrace, WorkflowValueType,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use url::Url;

const WORKFLOW_CHECKPOINT_SCHEMA_VERSION: u8 = 1;

/// Execute one validated workflow on a single native session.
pub(crate) async fn run(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    workflow: &WorkflowDefinition,
    inputs: &BTreeMap<String, Value>,
) -> BrowserResult<WorkflowRunResult> {
    workflow.validate()?;
    workflow.validate_inputs(inputs)?;
    let resolved_steps = workflow.resolve_actions(inputs)?;
    preflight_policy(policy, &resolved_steps).await?;

    let initial_revision = current_revision(session).await?;
    let run_id = format!("run_{}", session.next_native_execution_id());
    let started = Instant::now();
    let duration_budget = Duration::from_millis(workflow.budgets.max_duration_ms);
    let mut executed_steps = 0u32;
    let mut records: Vec<_> = resolved_steps
        .iter()
        .map(|step| new_record(&step.id))
        .collect();

    for predicate in &workflow.preconditions {
        if budget_expired(started, duration_budget) {
            skip_remaining(&mut records, 0);
            return Ok(budget_exhausted(
                workflow,
                run_id,
                records,
                None,
                "workflow maxDurationMs budget exhausted before a precondition",
                initial_revision,
                current_revision(session).await?,
            ));
        }
        match session.native_verify_once(predicate).await {
            Ok((true, _)) => {}
            Ok((false, state)) => {
                skip_remaining(&mut records, 0);
                return Ok(failed(
                    workflow,
                    run_id,
                    records,
                    None,
                    format!("workflow precondition failed: {state}"),
                    initial_revision,
                    current_revision(session).await?,
                ));
            }
            Err(error) => {
                skip_remaining(&mut records, 0);
                if budget_expired(started, duration_budget) {
                    return Ok(budget_exhausted(
                        workflow,
                        run_id,
                        records,
                        None,
                        "workflow maxDurationMs budget exhausted while checking a precondition",
                        initial_revision,
                        current_revision(session).await?,
                    ));
                }
                return Ok(failed(
                    workflow,
                    run_id,
                    records,
                    None,
                    format!("workflow precondition failed: {error}"),
                    initial_revision,
                    current_revision(session).await?,
                ));
            }
        }
    }

    for (index, step) in resolved_steps.iter().enumerate() {
        for repetition in 0..step.repeat {
            if executed_steps >= workflow.budgets.max_steps
                || budget_expired(started, duration_budget)
            {
                skip_remaining(&mut records, index);
                return Ok(budget_exhausted(
                    workflow,
                    run_id,
                    records,
                    Some(step.id.clone()),
                    if executed_steps >= workflow.budgets.max_steps {
                        "workflow maxSteps budget exhausted before dispatch"
                    } else {
                        "workflow maxDurationMs budget exhausted before dispatch"
                    },
                    initial_revision,
                    current_revision(session).await?,
                ));
            }
            executed_steps = executed_steps.saturating_add(1);
            if repetition > 0 {
                transition(&mut records[index], WorkflowStepState::Ready);
            }

            if let Some(predicate) = &step.when {
                if records[index].state == WorkflowStepState::Pending {
                    transition(&mut records[index], WorkflowStepState::Ready);
                }
                match session.native_verify_once(predicate).await {
                    Ok((matched, _)) => {
                        records[index].branch_decision = Some(WorkflowBranchDecision {
                            step_id: step.id.clone(),
                            predicate: predicate.clone(),
                            matched,
                        });
                        if !matched {
                            transition(&mut records[index], WorkflowStepState::Skipped);
                            break;
                        }
                    }
                    Err(error) => {
                        let message = bounded_text(&error.to_string(), 512);
                        transition(&mut records[index], WorkflowStepState::Preflight);
                        fail_record(
                            &mut records[index],
                            WorkflowStepState::FailedBeforeDispatch,
                            &message,
                        );
                        skip_remaining(&mut records, index + 1);
                        if budget_expired(started, duration_budget) {
                            return Ok(budget_exhausted(
                                workflow,
                                run_id,
                                records,
                                Some(step.id.clone()),
                                "workflow maxDurationMs budget exhausted while evaluating a branch",
                                initial_revision,
                                current_revision(session).await?,
                            ));
                        }
                        return Ok(failed(
                            workflow,
                            run_id,
                            records,
                            Some(step.id.clone()),
                            message,
                            initial_revision,
                            current_revision(session).await?,
                        ));
                    }
                }
            }

            let mut attempt_number: u32 = 0;
            let mut effect_marker_completed = false;
            let mut intent_evidence = None;
            let outcome = loop {
                let attempt_revision = current_revision(session).await?;
                if records[index].previous_revision.is_none() {
                    records[index].previous_revision = Some(attempt_revision);
                }
                records[index].retry_safe = step.transaction.permits_pre_dispatch_retry();
                if records[index].state == WorkflowStepState::Pending {
                    transition(&mut records[index], WorkflowStepState::Ready);
                }
                transition(&mut records[index], WorkflowStepState::Preflight);
                transition(&mut records[index], WorkflowStepState::Resolving);
                attempt_number = attempt_number.saturating_add(1);
                records[index].attempts = records[index].attempts.saturating_add(1);

                let result = if let Some(intent) = &step.intent {
                    match execute_intent_step(session, intent, attempt_revision).await {
                        Ok((outcome, evidence)) => {
                            intent_evidence = Some(evidence);
                            Ok(outcome)
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    native_batch::run(
                        session,
                        std::slice::from_ref(&step.action),
                        false,
                        BatchMode::Fixed,
                        Some(attempt_revision),
                    )
                    .await
                };

                match result {
                    Ok(outcome) => break outcome,
                    Err(error) => {
                        let message = bounded_text(&error.to_string(), 512);
                        let retry = attempt_number <= step.max_retries
                            && step.transaction.permits_pre_dispatch_retry();
                        records[index].current_revision = Some(current_revision(session).await?);
                        transition(&mut records[index], WorkflowStepState::NotDispatched);
                        fail_record(
                            &mut records[index],
                            WorkflowStepState::FailedBeforeDispatch,
                            &message,
                        );
                        if retry {
                            transition(&mut records[index], WorkflowStepState::Ready);
                            let marker_matches = match &step.before_retry {
                                Some(predicate) => {
                                    match session.native_verify_once(predicate).await {
                                        Ok((matched, _)) => Some(matched),
                                        Err(error) => {
                                            let marker_error = bounded_text(
                                                &format!(
                                                    "effect marker could not be evaluated: {error}"
                                                ),
                                                512,
                                            );
                                            transition(
                                                &mut records[index],
                                                WorkflowStepState::Preflight,
                                            );
                                            fail_record(
                                                &mut records[index],
                                                WorkflowStepState::FailedBeforeDispatch,
                                                &marker_error,
                                            );
                                            skip_remaining(&mut records, index + 1);
                                            return Ok(failed(
                                                workflow,
                                                run_id,
                                                records,
                                                Some(step.id.clone()),
                                                marker_error,
                                                initial_revision,
                                                current_revision(session).await?,
                                            ));
                                        }
                                    }
                                }
                                None => None,
                            };
                            if marker_matches == Some(true) {
                                effect_marker_completed = true;
                                break empty_batch(current_revision(session).await?);
                            }
                            continue;
                        }
                        let message = records[index]
                            .error
                            .clone()
                            .unwrap_or_else(|| "workflow step failed".into());
                        skip_remaining(&mut records, index + 1);
                        return Ok(failed(
                            workflow,
                            run_id,
                            records,
                            Some(step.id.clone()),
                            message,
                            initial_revision,
                            current_revision(session).await?,
                        ));
                    }
                }
            };

            if effect_marker_completed {
                commit_effect_marker(&mut records[index]);
                continue;
            }
            if !outcome.success {
                let message = outcome
                    .steps
                    .last()
                    .and_then(|step| match step {
                        BatchStepOutcome::Error { message, .. } => Some(message.as_str()),
                        BatchStepOutcome::Success { .. } => None,
                    })
                    .unwrap_or("workflow action failed after dispatch");
                records[index].dispatch_acknowledged = true;
                records[index].current_revision = Some(current_revision(session).await?);
                transition(&mut records[index], WorkflowStepState::Dispatched);
                fail_record(
                    &mut records[index],
                    WorkflowStepState::Indeterminate,
                    message,
                );
                let message = records[index]
                    .error
                    .clone()
                    .unwrap_or_else(|| "workflow step failed".into());
                skip_remaining(&mut records, index + 1);
                return Ok(resume_required(
                    workflow,
                    run_id,
                    records,
                    Some(step.id.clone()),
                    message,
                    initial_revision,
                    current_revision(session).await?,
                ));
            }

            records[index].intent_evidence = intent_evidence;
            records[index]
                .execution_ids
                .extend(outcome.steps.iter().filter_map(|step| match step {
                    BatchStepOutcome::Success {
                        execution_id: Some(id),
                        ..
                    } => Some(id.clone()),
                    _ => None,
                }));
            records[index].dispatch_acknowledged = true;
            records[index].effect_observed = true;
            records[index].current_revision = Some(current_revision(session).await?);
            transition(&mut records[index], WorkflowStepState::Dispatched);
            transition(&mut records[index], WorkflowStepState::EffectObserved);

            if let Some(predicate) = &step.expect {
                match session
                    .native_verify(predicate.clone(), remaining(started, duration_budget))
                    .await
                {
                    Ok(_) => {}
                    Err(error) => {
                        records[index].current_revision = Some(current_revision(session).await?);
                        fail_record(
                            &mut records[index],
                            WorkflowStepState::FailedAfterDispatch,
                            &error.to_string(),
                        );
                        let message = records[index]
                            .error
                            .clone()
                            .unwrap_or_else(|| "workflow verification failed".into());
                        skip_remaining(&mut records, index + 1);
                        if budget_expired(started, duration_budget) {
                            return Ok(budget_exhausted(
                                workflow,
                                run_id,
                                records,
                                Some(step.id.clone()),
                                "workflow maxDurationMs budget exhausted while verifying a step",
                                initial_revision,
                                current_revision(session).await?,
                            ));
                        }
                        return Ok(resume_required(
                            workflow,
                            run_id,
                            records,
                            Some(step.id.clone()),
                            message,
                            initial_revision,
                            current_revision(session).await?,
                        ));
                    }
                }
            }
            records[index].postcondition_verified = true;
            transition(&mut records[index], WorkflowStepState::Verified);
            transition(&mut records[index], WorkflowStepState::OutputsExtracted);
            transition(&mut records[index], WorkflowStepState::Committed);
        }
    }

    if budget_expired(started, duration_budget) {
        return Ok(budget_exhausted(
            workflow,
            run_id,
            records,
            None,
            "workflow maxDurationMs budget exhausted before terminal verification",
            initial_revision,
            current_revision(session).await?,
        ));
    }
    let terminal_proof = match session
        .native_verify(
            workflow.terminal_condition.clone(),
            remaining(started, duration_budget),
        )
        .await
    {
        Ok(outcome) => crate::browser::session::WorkflowTerminalProof {
            predicate: outcome.predicate,
            revision: current_revision(session).await?,
            state: outcome.state,
        },
        Err(error) => {
            if budget_expired(started, duration_budget) {
                return Ok(budget_exhausted(
                    workflow,
                    run_id,
                    records,
                    None,
                    "workflow maxDurationMs budget exhausted while verifying the terminal condition",
                    initial_revision,
                    current_revision(session).await?,
                ));
            }
            return Ok(failed(
                workflow,
                run_id,
                records,
                None,
                format!("workflow terminal condition was not proven: {error}"),
                initial_revision,
                current_revision(session).await?,
            ));
        }
    };
    if budget_expired(started, duration_budget) {
        return Ok(budget_exhausted(
            workflow,
            run_id,
            records,
            None,
            "workflow maxDurationMs budget exhausted before output extraction",
            initial_revision,
            current_revision(session).await?,
        ));
    }
    let outputs = match extract_outputs(session, workflow).await {
        Ok(outputs) => outputs,
        Err(error) => {
            if budget_expired(started, duration_budget) {
                return Ok(budget_exhausted(
                    workflow,
                    run_id,
                    records,
                    None,
                    "workflow maxDurationMs budget exhausted while extracting outputs",
                    initial_revision,
                    current_revision(session).await?,
                ));
            }
            return Ok(failed(
                workflow,
                run_id,
                records,
                None,
                format!("workflow output extraction failed: {error}"),
                initial_revision,
                current_revision(session).await?,
            ));
        }
    };
    let final_revision = current_revision(session).await?;
    Ok(make_result(
        workflow,
        run_id,
        WorkflowResultParts {
            status: WorkflowRunStatus::Completed,
            steps: records,
            outputs,
            terminal_proof: Some(terminal_proof),
            failed_step: None,
            failure: None,
            initial_revision,
            final_revision,
        },
    ))
}

/// Export the shared bounded checkpoint from a native run result.
pub(crate) async fn export_checkpoint(
    session: &BrowserRuntimeSession,
    workflow: &WorkflowDefinition,
    result: &WorkflowRunResult,
) -> BrowserResult<WorkflowCheckpoint> {
    workflow.validate()?;
    if result.name != workflow.name
        || result.workflow_version != workflow.workflow_version
        || result.steps.len() != workflow.steps.len()
    {
        return Err(WorkflowResumeError::CheckpointShape(
            "run result does not belong to workflow definition".into(),
        )
        .into());
    }
    let observation = session.native_observe().await?;
    let checkpoint = WorkflowCheckpoint {
        schema_version: WORKFLOW_CHECKPOINT_SCHEMA_VERSION,
        run_id: result.run_id.clone(),
        workflow_name: workflow.name.clone(),
        workflow_version: workflow.workflow_version.clone(),
        definition_hash: definition_hash(workflow)?,
        status: result.status,
        next_step_index: result
            .steps
            .iter()
            .position(|step| step.state != WorkflowStepState::Committed)
            .unwrap_or(result.steps.len()),
        steps: result.steps.iter().map(checkpoint_step).collect(),
        page: WorkflowCheckpointPage {
            target_id: bounded_text(&observation.page.target_id, 256),
            frame_id: bounded_text(&observation.page.frame_id, 256),
            url: bounded_text(&observation.page.url, 1_024),
            title: bounded_text(&observation.page.title, 1_024),
            revision: observation.revision,
        },
    };
    checkpoint.to_canonical_json()?;
    Ok(checkpoint)
}

/// Reconcile and resume only the uncommitted suffix of a native workflow.
pub(crate) async fn resume(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    workflow: &WorkflowDefinition,
    inputs: &BTreeMap<String, Value>,
    checkpoint: &WorkflowCheckpoint,
) -> BrowserResult<WorkflowRunResult> {
    workflow.validate()?;
    checkpoint.to_canonical_json()?;
    if checkpoint.workflow_name != workflow.name
        || checkpoint.workflow_version != workflow.workflow_version
        || checkpoint.definition_hash != definition_hash(workflow)?
    {
        return Err(WorkflowResumeError::DefinitionMismatch.into());
    }
    if checkpoint.steps.len() != workflow.steps.len()
        || checkpoint
            .steps
            .iter()
            .zip(&workflow.steps)
            .any(|(checkpoint_step, step)| checkpoint_step.id != step.id)
    {
        return Err(WorkflowResumeError::CheckpointShape(
            "checkpoint steps do not match workflow steps".into(),
        )
        .into());
    }
    let observation = session.native_observe().await?;
    if checkpoint.page.target_id != observation.page.target_id
        || checkpoint.page.frame_id != observation.page.frame_id
        || checkpoint.page.url != bounded_text(&observation.page.url, 1_024)
        || checkpoint.page.title != bounded_text(&observation.page.title, 1_024)
    {
        return Err(WorkflowResumeError::RouteChanged.into());
    }
    let next_step_index = checkpoint
        .steps
        .iter()
        .position(|step| step.state != WorkflowStepState::Committed)
        .unwrap_or(checkpoint.steps.len());
    if checkpoint.next_step_index != next_step_index {
        return Err(WorkflowResumeError::CheckpointShape(
            "nextStepIndex does not match step states".into(),
        )
        .into());
    }
    if checkpoint.status == WorkflowRunStatus::Completed && next_step_index != workflow.steps.len()
    {
        return Err(WorkflowResumeError::CheckpointShape(
            "completed checkpoint has uncommitted steps".into(),
        )
        .into());
    }
    if next_step_index >= workflow.steps.len() {
        return Err(WorkflowResumeError::CheckpointShape(
            "workflow checkpoint is already complete".into(),
        )
        .into());
    }
    let next_state = checkpoint.steps[next_step_index].state;
    if next_state == WorkflowStepState::FailedBeforeDispatch
        && !workflow.steps[next_step_index]
            .transaction
            .permits_pre_dispatch_retry()
    {
        return Err(WorkflowResumeError::InvalidState {
            step_id: workflow.steps[next_step_index].id.clone(),
            state: next_state,
        }
        .into());
    }
    if !matches!(
        next_state,
        WorkflowStepState::Pending | WorkflowStepState::FailedBeforeDispatch
    ) {
        return Err(WorkflowResumeError::InvalidState {
            step_id: workflow.steps[next_step_index].id.clone(),
            state: next_state,
        }
        .into());
    }
    for step in checkpoint.steps.iter().skip(next_step_index + 1) {
        if step.state != WorkflowStepState::Skipped {
            return Err(WorkflowResumeError::InvalidState {
                step_id: step.id.clone(),
                state: step.state,
            }
            .into());
        }
    }

    let mut suffix = workflow.clone();
    suffix.steps = workflow.steps[next_step_index..].to_vec();
    let mut result = Box::pin(run(session, policy, &suffix, inputs)).await?;
    let mut prefix = checkpoint.steps[..next_step_index]
        .iter()
        .map(checkpoint_record)
        .collect::<Vec<_>>();
    prefix.append(&mut result.steps);
    result.steps = prefix;
    result.trace = WorkflowTrace::from_steps(&result.steps);
    result.trace.run_id = Some(result.run_id.clone());
    Ok(result)
}

async fn preflight_policy(policy: &BrowserPolicy, steps: &[WorkflowStep]) -> BrowserResult<()> {
    for (index, step) in steps.iter().enumerate() {
        if step.intent.is_none() {
            native_batch::check_policy(policy, std::slice::from_ref(&step.action))
                .await
                .map_err(|error| {
                    format!(
                        "workflow policy denial at step {index} ({}): {error}",
                        step.id
                    )
                })?;
        }
    }
    Ok(())
}

async fn execute_intent_step(
    session: &BrowserRuntimeSession,
    intent: &crate::browser::session::WorkflowIntentStep,
    expected_revision: u64,
) -> BrowserResult<(BatchOutcome, WorkflowIntentEvidence)> {
    let mut execution = intent.execution_request("workflow.intent")?;
    execution.request.expected_revision = Some(expected_revision);
    let result = session
        .native_act_and_verify(&execution, None, Duration::from_millis(1))
        .await?;
    if result.execution.status != SemanticIntentExecutionStatus::Executed {
        return Err(result
            .execution
            .reason
            .unwrap_or_else(|| "native workflow intent was not executed".into())
            .into());
    }
    let resolution = result.execution.resolution;
    let candidate = resolution
        .candidates
        .iter()
        .find(|candidate| candidate.id == result.execution.candidate_id)
        .ok_or("native workflow intent result omitted its accepted candidate")?;
    let revision = resolution
        .revision
        .ok_or("native workflow intent result omitted revision")?;
    let action: ActionOutcome = result
        .execution
        .action
        .ok_or("native workflow intent result omitted action evidence")?;
    let execution_id = action.execution_id.clone();
    let evidence = WorkflowIntentEvidence {
        resolution_id: result.execution.resolution_id,
        candidate_id: result.execution.candidate_id,
        revision,
        resolution: resolution.resolution,
        policy_decision: resolution.policy_decision,
        confidence: candidate.confidence,
        fingerprint: candidate.fingerprint.clone(),
    };
    Ok((
        BatchOutcome {
            mode: BatchMode::Fixed,
            initial_revision: expected_revision,
            final_revision: action.current_revision,
            steps: vec![BatchStepOutcome::Success {
                index: 0,
                action: "intent".into(),
                response_bytes: Some(serde_json::to_vec(&action)?.len()),
                execution_id: Some(execution_id),
            }],
            completed: 1,
            failed: 0,
            total: 1,
            success: true,
        },
        evidence,
    ))
}

async fn extract_outputs(
    session: &BrowserRuntimeSession,
    workflow: &WorkflowDefinition,
) -> BrowserResult<BTreeMap<String, WorkflowOutput>> {
    if workflow.outputs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let observation = session.native_observe().await?;
    let visible_text = observation.text.unwrap_or_default();
    let mut outputs = BTreeMap::new();
    let mut extracted_bytes = 0usize;
    for (name, declaration) in &workflow.outputs {
        let text = match declaration.source {
            WorkflowOutputSource::PageUrl => observation.page.url.as_str(),
            WorkflowOutputSource::PageTitle => observation.page.title.as_str(),
            WorkflowOutputSource::VisibleText => visible_text.as_str(),
        };
        extracted_bytes = extracted_bytes.saturating_add(text.len());
        if extracted_bytes > workflow.budgets.max_extracted_bytes {
            return Err(format!(
                "outputs exceed maxExtractedBytes {}",
                workflow.budgets.max_extracted_bytes
            )
            .into());
        }
        let value = typed_output_value(name, declaration.value_type, text)?;
        outputs.insert(
            name.clone(),
            WorkflowOutput {
                value_type: declaration.value_type,
                value: if declaration.sensitive {
                    Value::Null
                } else {
                    value
                },
                redacted: declaration.sensitive,
                evidence: WorkflowOutputEvidence {
                    source: declaration.source,
                    revision: observation.revision,
                },
            },
        );
    }
    Ok(outputs)
}

fn typed_output_value(
    name: &str,
    value_type: WorkflowValueType,
    text: &str,
) -> BrowserResult<Value> {
    let trimmed = text.trim();
    match value_type {
        WorkflowValueType::String => Ok(Value::String(text.to_owned())),
        WorkflowValueType::Url => {
            Url::parse(trimmed).map_err(|_| format!("output {name:?} is not a valid URL"))?;
            Ok(Value::String(text.to_owned()))
        }
        WorkflowValueType::Integer => trimmed
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| format!("output {name:?} cannot be parsed as an integer").into()),
        WorkflowValueType::Number => {
            let number = trimmed
                .parse::<f64>()
                .map_err(|_| format!("output {name:?} cannot be parsed as a number"))?;
            serde_json::Number::from_f64(number)
                .map(Value::Number)
                .ok_or_else(|| format!("output {name:?} is not a finite number").into())
        }
        WorkflowValueType::Boolean => match trimmed {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("output {name:?} cannot be parsed as a boolean").into()),
        },
    }
}

fn definition_hash(workflow: &WorkflowDefinition) -> BrowserResult<String> {
    let canonical = workflow.to_canonical_json()?;
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn new_record(id: &str) -> WorkflowStepRecord {
    WorkflowStepRecord {
        id: id.to_owned(),
        state: WorkflowStepState::Pending,
        history: vec![WorkflowStepState::Pending],
        attempts: 0,
        execution_ids: Vec::new(),
        dispatch_acknowledged: false,
        effect_observed: false,
        postcondition_verified: false,
        retry_safe: false,
        previous_revision: None,
        current_revision: None,
        branch_decision: None,
        intent_evidence: None,
        error: None,
    }
}

fn transition(record: &mut WorkflowStepRecord, next: WorkflowStepState) {
    if record.state.can_transition_to(next) {
        record.state = next;
        record.history.push(next);
    }
}

fn fail_record(record: &mut WorkflowStepRecord, state: WorkflowStepState, error: &str) {
    transition(record, state);
    record.error = Some(bounded_text(error, 512));
}

fn commit_effect_marker(record: &mut WorkflowStepRecord) {
    if record.state == WorkflowStepState::Ready {
        transition(record, WorkflowStepState::Preflight);
    }
    if record.state == WorkflowStepState::Preflight {
        transition(record, WorkflowStepState::EffectObserved);
    }
    transition(record, WorkflowStepState::Verified);
    transition(record, WorkflowStepState::OutputsExtracted);
    transition(record, WorkflowStepState::Committed);
}

fn skip_remaining(records: &mut [WorkflowStepRecord], start: usize) {
    for record in records.iter_mut().skip(start) {
        if record.state == WorkflowStepState::Pending {
            transition(record, WorkflowStepState::Skipped);
        }
    }
}

fn checkpoint_step(record: &WorkflowStepRecord) -> WorkflowCheckpointStep {
    WorkflowCheckpointStep {
        id: record.id.clone(),
        state: record.state,
        attempts: record.attempts,
        history: record.history.clone(),
        execution_ids: record.execution_ids.clone(),
        dispatch_acknowledged: record.dispatch_acknowledged,
        effect_observed: record.effect_observed,
        postcondition_verified: record.postcondition_verified,
        retry_safe: record.retry_safe,
        previous_revision: record.previous_revision,
        current_revision: record.current_revision,
        branch_decision: record.branch_decision.clone(),
        intent_evidence: record.intent_evidence.clone(),
    }
}

fn checkpoint_record(step: &WorkflowCheckpointStep) -> WorkflowStepRecord {
    WorkflowStepRecord {
        id: step.id.clone(),
        state: step.state,
        history: if step.history.is_empty() {
            checkpoint_history_for_state(step.state)
        } else {
            step.history.clone()
        },
        attempts: step.attempts,
        execution_ids: step.execution_ids.clone(),
        dispatch_acknowledged: step.dispatch_acknowledged,
        effect_observed: step.effect_observed,
        postcondition_verified: step.postcondition_verified,
        retry_safe: step.retry_safe,
        previous_revision: step.previous_revision,
        current_revision: step.current_revision,
        branch_decision: step.branch_decision.clone(),
        intent_evidence: step.intent_evidence.clone(),
        error: None,
    }
}

struct WorkflowResultParts {
    status: WorkflowRunStatus,
    steps: Vec<WorkflowStepRecord>,
    outputs: BTreeMap<String, WorkflowOutput>,
    terminal_proof: Option<crate::browser::session::WorkflowTerminalProof>,
    failed_step: Option<String>,
    failure: Option<String>,
    initial_revision: u64,
    final_revision: u64,
}

fn make_result(
    workflow: &WorkflowDefinition,
    run_id: String,
    parts: WorkflowResultParts,
) -> WorkflowRunResult {
    let mut trace = WorkflowTrace::from_steps(&parts.steps);
    trace.run_id = Some(run_id.clone());
    WorkflowRunResult {
        run_id,
        name: workflow.name.clone(),
        workflow_version: workflow.workflow_version.clone(),
        status: parts.status,
        steps: parts.steps,
        trace,
        outputs: parts.outputs,
        terminal_proof: parts.terminal_proof,
        failed_step: parts.failed_step,
        failure: parts.failure.map(|value| bounded_text(&value, 512)),
        initial_revision: parts.initial_revision,
        final_revision: parts.final_revision,
    }
}

fn failed(
    workflow: &WorkflowDefinition,
    run_id: String,
    steps: Vec<WorkflowStepRecord>,
    failed_step: Option<String>,
    failure: impl Into<String>,
    initial_revision: u64,
    final_revision: u64,
) -> WorkflowRunResult {
    make_result(
        workflow,
        run_id,
        WorkflowResultParts {
            status: WorkflowRunStatus::Failed,
            steps,
            outputs: BTreeMap::new(),
            terminal_proof: None,
            failed_step,
            failure: Some(failure.into()),
            initial_revision,
            final_revision,
        },
    )
}

fn budget_exhausted(
    workflow: &WorkflowDefinition,
    run_id: String,
    steps: Vec<WorkflowStepRecord>,
    failed_step: Option<String>,
    reason: impl Into<String>,
    initial_revision: u64,
    final_revision: u64,
) -> WorkflowRunResult {
    make_result(
        workflow,
        run_id,
        WorkflowResultParts {
            status: WorkflowRunStatus::BudgetExhausted,
            steps,
            outputs: BTreeMap::new(),
            terminal_proof: None,
            failed_step,
            failure: Some(reason.into()),
            initial_revision,
            final_revision,
        },
    )
}

fn resume_required(
    workflow: &WorkflowDefinition,
    run_id: String,
    steps: Vec<WorkflowStepRecord>,
    failed_step: Option<String>,
    reason: impl Into<String>,
    initial_revision: u64,
    final_revision: u64,
) -> WorkflowRunResult {
    make_result(
        workflow,
        run_id,
        WorkflowResultParts {
            status: WorkflowRunStatus::ResumeRequired,
            steps,
            outputs: BTreeMap::new(),
            terminal_proof: None,
            failed_step,
            failure: Some(reason.into()),
            initial_revision,
            final_revision,
        },
    )
}

fn empty_batch(revision: u64) -> BatchOutcome {
    BatchOutcome {
        mode: BatchMode::Unguarded,
        initial_revision: revision,
        final_revision: revision,
        steps: Vec::new(),
        completed: 0,
        failed: 0,
        total: 0,
        success: true,
    }
}

fn checkpoint_history_for_state(state: WorkflowStepState) -> Vec<WorkflowStepState> {
    use WorkflowStepState::*;

    match state {
        Pending => vec![Pending],
        Ready => vec![Pending, Ready],
        Preflight => vec![Pending, Ready, Preflight],
        Resolving => vec![Pending, Ready, Preflight, Resolving],
        NotDispatched => vec![Pending, Ready, Preflight, Resolving, NotDispatched],
        Dispatched => vec![Pending, Ready, Preflight, Resolving, Dispatched],
        EffectObserved => vec![
            Pending,
            Ready,
            Preflight,
            Resolving,
            Dispatched,
            EffectObserved,
        ],
        Verified => vec![
            Pending,
            Ready,
            Preflight,
            Resolving,
            Dispatched,
            EffectObserved,
            Verified,
        ],
        OutputsExtracted => vec![
            Pending,
            Ready,
            Preflight,
            Resolving,
            Dispatched,
            EffectObserved,
            Verified,
            OutputsExtracted,
        ],
        Committed => vec![
            Pending,
            Ready,
            Preflight,
            Resolving,
            Dispatched,
            EffectObserved,
            Verified,
            OutputsExtracted,
            Committed,
        ],
        FailedBeforeDispatch => vec![Pending, Ready, Preflight, FailedBeforeDispatch],
        FailedAfterDispatch => vec![
            Pending,
            Ready,
            Preflight,
            Resolving,
            Dispatched,
            FailedAfterDispatch,
        ],
        Indeterminate => vec![
            Pending,
            Ready,
            Preflight,
            Resolving,
            Dispatched,
            Indeterminate,
        ],
        Skipped => vec![Pending, Skipped],
    }
}

async fn current_revision(session: &BrowserRuntimeSession) -> BrowserResult<u64> {
    Ok(session.native_observe().await?.revision)
}

fn budget_expired(started: Instant, budget: Duration) -> bool {
    started.elapsed() >= budget
}

fn remaining(started: Instant, budget: Duration) -> Duration {
    budget.saturating_sub(started.elapsed())
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes.saturating_sub(15);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated]", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::bounded_text;

    #[test]
    fn bounds_workflow_error_text_on_character_boundary() {
        assert_eq!(bounded_text("abcdef", 6), "abcdef");
        assert!(bounded_text("abcdefghijklmnopqrstuvwxyz", 16).ends_with("[truncated]"));
    }
}

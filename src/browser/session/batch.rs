//! Ordered batch execution with policy pre-flight.
//!
//! Runs up to [`MAX_BATCH_STEPS`] typed operations as a single atomic unit.
//! Every step is validated against the active [`BrowserPolicy`] before any
//! side effects are applied.

use super::*;
use crate::browser::policy::PolicyCapability;
use std::time::Duration;

impl BrowserSession {
    /// Execute an ordered batch of typed operations.
    /// Policy is pre-flighted for every step before any step executes.
    /// Fails fast on the first error; ambiguity always fails closed.
    pub async fn run_batch(&self, steps: &[BatchStep]) -> BrowserResult<BatchOutcome> {
        self.run_batch_with_options(steps, false).await
    }

    /// Execute a batch with optional atomic locator pre-resolution.
    pub async fn run_batch_with_options(
        &self,
        steps: &[BatchStep],
        atomic: bool,
    ) -> BrowserResult<BatchOutcome> {
        self.run_batch_with_mode(steps, atomic, BatchMode::Unguarded, None)
            .await
    }

    /// Execute a batch with an explicit revision policy.
    pub async fn run_batch_with_mode(
        &self,
        steps: &[BatchStep],
        atomic: bool,
        mode: BatchMode,
        expected_revision: Option<u64>,
    ) -> BrowserResult<BatchOutcome> {
        let total = steps.len();
        if total > MAX_BATCH_STEPS {
            return Err(format!(
                "batch exceeds max {} steps (received {total})",
                MAX_BATCH_STEPS
            )
            .into());
        }

        for (index, step) in steps.iter().enumerate() {
            let action = step_action_name(step);
            if let Err(policy_error) = check_batch_step_policy(self, step).await {
                return Err(format!(
                    "batch policy denial at step {index} ({action}): {policy_error}"
                )
                .into());
            }
        }

        if atomic {
            for (index, step) in steps.iter().enumerate() {
                if let Some(target) = batch_target(step) {
                    self.resolve_element(target).await.map_err(|error| {
                        format!("atomic batch target resolution failed at step {index}: {error}")
                    })?;
                }
            }
        }

        let initial_revision = self.page_revision.load(Ordering::Relaxed);
        let mut chained_revision = match mode {
            BatchMode::Unguarded => None,
            BatchMode::Fixed | BatchMode::Chain => Some(
                expected_revision.ok_or("batch mode fixed or chain requires expectedRevision")?,
            ),
        };

        let mut outcomes: Vec<BatchStepOutcome> = Vec::with_capacity(total);
        let mut completed = 0usize;
        let mut failed = 0usize;

        for (index, step) in steps.iter().enumerate() {
            let action = step_action_name(step);
            let step_revision = chained_revision;
            match execute_batch_step(self, step, step_revision).await {
                Ok((response_bytes, resulting_revision)) => {
                    outcomes.push(BatchStepOutcome::Success {
                        index,
                        action,
                        response_bytes,
                    });
                    completed += 1;
                    if mode == BatchMode::Chain {
                        chained_revision = resulting_revision
                            .or_else(|| Some(self.page_revision.load(Ordering::Relaxed)));
                    }
                }
                Err(error) => {
                    let message = bounded_batch_text(&format!("{error:#}"), 512);
                    outcomes.push(BatchStepOutcome::Error {
                        index,
                        action,
                        message,
                    });
                    failed += 1;
                    break;
                }
            }
        }

        Ok(BatchOutcome {
            mode,
            initial_revision,
            final_revision: self.page_revision.load(Ordering::Relaxed),
            success: failed == 0,
            completed,
            failed,
            total,
            steps: outcomes,
        })
    }
}

async fn check_batch_step_policy(
    session: &BrowserSession,
    step: &BatchStep,
) -> Result<(), PolicyError> {
    match step {
        BatchStep::Navigate { url, .. } => session.policy.require_url(url).await.map(|_| ()),
        BatchStep::Evaluate { .. } => session.policy.require_for_batch(PolicyCapability::Evaluate),
        BatchStep::Observe {
            include_form_values,
            ..
        } => {
            if *include_form_values {
                session
                    .policy
                    .require_for_batch(PolicyCapability::ReadFormValues)
            } else {
                Ok(())
            }
        }
        BatchStep::Screenshot => session
            .policy
            .require_for_batch(PolicyCapability::Screenshot),
        _ => Ok(()),
    }
}

fn batch_target(step: &BatchStep) -> Option<&str> {
    match step {
        BatchStep::Click { target }
        | BatchStep::Check { target }
        | BatchStep::Uncheck { target }
        | BatchStep::Select { target, .. }
        | BatchStep::Clear { target } => Some(target),
        BatchStep::Type { target, .. } => target.as_deref(),
        _ => None,
    }
}

fn bounded_batch_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes.saturating_sub(15);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated]", &value[..end])
}

async fn execute_batch_step(
    session: &BrowserSession,
    step: &BatchStep,
    expected_revision: Option<u64>,
) -> BrowserResult<(Option<usize>, Option<u64>)> {
    match step {
        BatchStep::Navigate { url, timeout_ms } => {
            if let Some(revision) = expected_revision {
                let info = session
                    .navigate_with_revision(url, Duration::from_millis(*timeout_ms), revision)
                    .await?;
                let bytes = serde_json::to_string(&info).ok().map(|s| s.len());
                Ok((bytes, Some(info.current_revision)))
            } else {
                let info = session
                    .navigate_with_deadline(url, Duration::from_millis(*timeout_ms))
                    .await?;
                let bytes = serde_json::to_string(&info).ok().map(|s| s.len());
                Ok((bytes, Some(session.page_revision.load(Ordering::Relaxed))))
            }
        }
        BatchStep::Click { target } => {
            let outcome = match expected_revision {
                Some(revision) => session.click_with_revision(target, revision).await?,
                None => session.click(target).await?,
            };
            let bytes = serde_json::to_string(&outcome).ok().map(|s| s.len());
            Ok((bytes, Some(outcome.current_revision)))
        }
        BatchStep::Type { text, target } => {
            let outcome = session
                .type_text_with_expected_revision(text, target.as_deref(), expected_revision)
                .await?;
            Ok((
                serde_json::to_string(&outcome).ok().map(|s| s.len()),
                Some(outcome.current_revision),
            ))
        }
        BatchStep::Check { target } => {
            let outcome = session
                .check_with_revision(target, expected_revision)
                .await?;
            Ok((
                serde_json::to_string(&outcome).ok().map(|s| s.len()),
                Some(outcome.current_revision),
            ))
        }
        BatchStep::Uncheck { target } => {
            let outcome = session
                .uncheck_with_revision(target, expected_revision)
                .await?;
            Ok((
                serde_json::to_string(&outcome).ok().map(|s| s.len()),
                Some(outcome.current_revision),
            ))
        }
        BatchStep::Select { target, value } => {
            let outcome = session
                .select_option_with_revision(target, value, expected_revision)
                .await?;
            Ok((
                serde_json::to_string(&outcome).ok().map(|s| s.len()),
                Some(outcome.current_revision),
            ))
        }
        BatchStep::Clear { target } => {
            let outcome = session
                .clear_with_revision(target, expected_revision)
                .await?;
            Ok((
                serde_json::to_string(&outcome).ok().map(|s| s.len()),
                Some(outcome.current_revision),
            ))
        }
        BatchStep::Scroll { dx, dy } => {
            let outcome = session
                .scroll_with_revision(*dx, *dy, expected_revision)
                .await?;
            Ok((
                serde_json::to_string(&outcome).ok().map(|s| s.len()),
                Some(outcome.current_revision),
            ))
        }
        BatchStep::Wait {
            condition,
            timeout_ms,
        } => {
            let cond = WaitCondition::parse(condition)?;
            session
                .wait(cond, Duration::from_millis(*timeout_ms))
                .await?;
            Ok((None, None))
        }
        BatchStep::Observe {
            include_dom,
            include_screenshot,
            include_form_values,
        } => {
            let context = match (*include_dom, *include_screenshot, *include_form_values) {
                (false, false, false) => session.observe().await?,
                (true, false, false) => session.observe_with_dom().await?,
                (false, true, false) => session.observe_with_screenshot().await?,
                (true, true, false) => session.observe_with_dom_and_screenshot().await?,
                (false, false, true) => session.observe_with_form_values().await?,
                _ => {
                    return Err(
                        "form values may only be combined with default compact observe".into(),
                    );
                }
            };
            let bytes = serde_json::to_string(&context).ok().map(|s| s.len());
            Ok((bytes, Some(session.page_revision.load(Ordering::Relaxed))))
        }
        BatchStep::Screenshot => {
            let data = session.screenshot_base64().await?;
            Ok((Some(data.len()), None))
        }
        BatchStep::Evaluate { expression } => {
            let result = session.evaluate(expression).await?;
            let bytes = serde_json::to_string(&result).ok().map(|s| s.len());
            Ok((bytes, None))
        }
        BatchStep::AcceptDialog => {
            session.accept_dialog().await?;
            Ok((None, None))
        }
        BatchStep::DismissDialog => {
            session.dismiss_dialog().await?;
            Ok((None, None))
        }
    }
}

fn step_action_name(step: &BatchStep) -> String {
    match step {
        BatchStep::Navigate { .. } => "navigate".to_string(),
        BatchStep::Click { .. } => "click".to_string(),
        BatchStep::Type { .. } => "type".to_string(),
        BatchStep::Check { .. } => "check".to_string(),
        BatchStep::Uncheck { .. } => "uncheck".to_string(),
        BatchStep::Select { .. } => "select".to_string(),
        BatchStep::Clear { .. } => "clear".to_string(),
        BatchStep::Scroll { .. } => "scroll".to_string(),
        BatchStep::Wait { .. } => "wait".to_string(),
        BatchStep::Observe { .. } => "observe".to_string(),
        BatchStep::Screenshot => "screenshot".to_string(),
        BatchStep::Evaluate { .. } => "evaluate".to_string(),
        BatchStep::AcceptDialog => "acceptDialog".to_string(),
        BatchStep::DismissDialog => "dismissDialog".to_string(),
    }
}

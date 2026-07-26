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

        let mut outcomes: Vec<BatchStepOutcome> = Vec::with_capacity(total);
        let mut completed = 0usize;
        let mut failed = 0usize;

        for (index, step) in steps.iter().enumerate() {
            let action = step_action_name(step);
            match execute_batch_step(self, step).await {
                Ok(response_bytes) => {
                    outcomes.push(BatchStepOutcome::Success {
                        index,
                        action,
                        response_bytes,
                    });
                    completed += 1;
                }
                Err(error) => {
                    let message = format!("{error:#}");
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
        BatchStep::Evaluate { .. } => session.policy.require(PolicyCapability::Evaluate),
        BatchStep::Observe {
            include_form_values,
            ..
        } => {
            if *include_form_values {
                session.policy.require(PolicyCapability::ReadFormValues)
            } else {
                Ok(())
            }
        }
        BatchStep::Screenshot => session.policy.require(PolicyCapability::Screenshot),
        _ => Ok(()),
    }
}

async fn execute_batch_step(
    session: &BrowserSession,
    step: &BatchStep,
) -> BrowserResult<Option<usize>> {
    match step {
        BatchStep::Navigate { url, timeout_ms } => {
            let info = session
                .navigate_with_deadline(url, Duration::from_millis(*timeout_ms))
                .await?;
            let bytes = serde_json::to_string(&info).ok().map(|s| s.len());
            Ok(bytes)
        }
        BatchStep::Click { target } => {
            let outcome = session.click(target).await?;
            let bytes = serde_json::to_string(&outcome).ok().map(|s| s.len());
            Ok(bytes)
        }
        BatchStep::Type { text, target } => {
            session.type_text(text, target.as_deref()).await?;
            Ok(None)
        }
        BatchStep::Check { target } => {
            session.check(target).await?;
            Ok(None)
        }
        BatchStep::Uncheck { target } => {
            session.uncheck(target).await?;
            Ok(None)
        }
        BatchStep::Select { target, value } => {
            session.select_option(target, value).await?;
            Ok(None)
        }
        BatchStep::Clear { target } => {
            session.clear(target).await?;
            Ok(None)
        }
        BatchStep::Scroll { dx, dy } => {
            session.scroll(*dx, *dy).await?;
            Ok(None)
        }
        BatchStep::Wait {
            condition,
            timeout_ms,
        } => {
            let cond = WaitCondition::parse(condition)?;
            session
                .wait(cond, Duration::from_millis(*timeout_ms))
                .await?;
            Ok(None)
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
            Ok(bytes)
        }
        BatchStep::Screenshot => {
            let data = session.screenshot_base64().await?;
            Ok(Some(data.len()))
        }
        BatchStep::Evaluate { expression } => {
            let result = session.evaluate(expression).await?;
            let bytes = serde_json::to_string(&result).ok().map(|s| s.len());
            Ok(bytes)
        }
        BatchStep::AcceptDialog => {
            session.accept_dialog().await?;
            Ok(None)
        }
        BatchStep::DismissDialog => {
            session.dismiss_dialog().await?;
            Ok(None)
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

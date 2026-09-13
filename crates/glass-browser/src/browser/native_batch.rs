//! Native execution of the shared ordered batch contract.
//!
//! The Chromium session has a batch implementation under `session`.  Native
//! callers use this adapter instead of translating a batch into separate CLI
//! processes, which keeps revision chaining and the persistent-session owner
//! on one browser runtime.

use super::native_engine::NativePreflightAction;
use super::runtime::BrowserRuntimeSession;
use crate::browser::policy::{BrowserPolicy, PolicyCapability};
use crate::browser::session::{
    BatchMode, BatchOutcome, BatchStep, BatchStepOutcome, BrowserResult, MAX_BATCH_STEPS,
    WaitCondition,
};
use crate::browser_backend::{PromptDecision, SemanticAction};
use base64::Engine as _;
use serde_json::json;
use std::time::Duration;

/// Preflight all policy-sensitive operations before dispatching any batch
/// step.  The check deliberately mirrors the Chromium batch policy gate.
pub(crate) async fn check_policy(policy: &BrowserPolicy, steps: &[BatchStep]) -> BrowserResult<()> {
    if steps.len() > MAX_BATCH_STEPS {
        return Err(format!(
            "batch exceeds max {MAX_BATCH_STEPS} steps (received {})",
            steps.len()
        )
        .into());
    }
    for (index, step) in steps.iter().enumerate() {
        let result = match step {
            BatchStep::Navigate { url, .. } => policy
                .require_url(&crate::browser::session::normalize_url(url))
                .await
                .map(|_| ()),
            BatchStep::Evaluate { .. } => policy.require_for_batch(PolicyCapability::Evaluate),
            BatchStep::Observe {
                include_form_values,
                ..
            } if *include_form_values => policy.require_for_batch(PolicyCapability::ReadFormValues),
            BatchStep::Screenshot => policy.require_for_batch(PolicyCapability::Screenshot),
            BatchStep::Wait { condition, .. }
                if matches!(
                    WaitCondition::parse(condition),
                    Ok(WaitCondition::JavaScript(_))
                ) =>
            {
                policy.require_for_batch(PolicyCapability::Evaluate)
            }
            _ => Ok(()),
        };
        result.map_err(|error| {
            format!(
                "batch policy denial at step {index} ({}): {error}",
                action_name(step)
            )
        })?;
    }
    Ok(())
}

/// Execute a bounded ordered native batch.
pub(crate) async fn run(
    session: &BrowserRuntimeSession,
    steps: &[BatchStep],
    atomic: bool,
    mode: BatchMode,
    expected_revision: Option<u64>,
) -> BrowserResult<BatchOutcome> {
    if steps.len() > MAX_BATCH_STEPS {
        return Err(format!(
            "batch exceeds max {MAX_BATCH_STEPS} steps (received {})",
            steps.len()
        )
        .into());
    }
    if matches!(mode, BatchMode::Fixed | BatchMode::Chain) && expected_revision.is_none() {
        return Err("batch mode fixed or chain requires expectedRevision".into());
    }

    if atomic {
        for (index, step) in steps.iter().enumerate() {
            if let Some(target) = target_for(step) {
                let preflight = session
                    .native_preflight_target(target, NativePreflightAction::Click)
                    .await?;
                if !preflight.unique {
                    return Err(format!(
                        "atomic batch target resolution failed at step {index}: {target}"
                    )
                    .into());
                }
            }
        }
    }

    let initial_revision = session.native_observe().await?.revision;
    let mut chained_revision = match mode {
        BatchMode::Unguarded => None,
        BatchMode::Fixed | BatchMode::Chain => expected_revision,
    };
    let mut outcomes = Vec::with_capacity(steps.len());
    let mut completed = 0usize;
    let mut failed = 0usize;

    for (index, step) in steps.iter().enumerate() {
        let action = action_name(step);
        let step_revision = chained_revision;
        match execute_step(session, step, step_revision).await {
            Ok((response_bytes, resulting_revision, execution_id)) => {
                outcomes.push(BatchStepOutcome::Success {
                    index,
                    action,
                    response_bytes,
                    execution_id,
                });
                completed += 1;
                if mode == BatchMode::Chain {
                    chained_revision = resulting_revision.or(chained_revision);
                }
            }
            Err(error) => {
                outcomes.push(BatchStepOutcome::Error {
                    index,
                    action,
                    message: bounded_text(&error.to_string(), 512),
                    execution_id: None,
                });
                failed += 1;
                break;
            }
        }
    }

    let final_revision = session.native_observe().await?.revision;
    Ok(BatchOutcome {
        mode,
        initial_revision,
        final_revision,
        steps: outcomes,
        completed,
        failed,
        total: steps.len(),
        success: failed == 0,
    })
}

async fn execute_step(
    session: &BrowserRuntimeSession,
    step: &BatchStep,
    expected_revision: Option<u64>,
) -> BrowserResult<(Option<usize>, Option<u64>, Option<String>)> {
    match step {
        BatchStep::Navigate { url, timeout_ms } => {
            let url = crate::browser::session::normalize_url(url);
            let navigation = async {
                match expected_revision {
                    Some(revision) => session.navigate_with_revision(url, revision).await,
                    None => session.navigate(url).await,
                }
            };
            let result = tokio::time::timeout(Duration::from_millis(*timeout_ms), navigation)
                .await
                .map_err(|_| format!("native navigation exceeded its {timeout_ms}ms deadline"))??;
            Ok((
                Some(serde_json::to_vec(&result)?.len()),
                Some(result.revision),
                Some(session.next_native_execution_id()),
            ))
        }
        BatchStep::Click { target } => {
            execute_action(
                session,
                SemanticAction::Click {
                    target: target.clone(),
                },
                expected_revision,
            )
            .await
        }
        BatchStep::Type { text, target } => {
            let target = target.clone().ok_or("native batch type requires target")?;
            execute_action(
                session,
                SemanticAction::Type {
                    target,
                    text: text.clone(),
                },
                expected_revision,
            )
            .await
        }
        BatchStep::Check { target } => {
            execute_action(
                session,
                SemanticAction::Check {
                    target: target.clone(),
                },
                expected_revision,
            )
            .await
        }
        BatchStep::Uncheck { target } => {
            execute_action(
                session,
                SemanticAction::Uncheck {
                    target: target.clone(),
                },
                expected_revision,
            )
            .await
        }
        BatchStep::Select { target, value } => {
            execute_action(
                session,
                SemanticAction::Select {
                    target: target.clone(),
                    value: value.clone(),
                },
                expected_revision,
            )
            .await
        }
        BatchStep::Clear { target } => {
            execute_action(
                session,
                SemanticAction::Clear {
                    target: target.clone(),
                },
                expected_revision,
            )
            .await
        }
        BatchStep::Scroll { dx, dy } => {
            let (delta_x, delta_y) = scroll_deltas(*dx, *dy)?;
            execute_action(
                session,
                SemanticAction::Scroll { delta_x, delta_y },
                expected_revision,
            )
            .await
        }
        BatchStep::Wait {
            condition,
            timeout_ms,
        } => {
            let condition = WaitCondition::parse(condition)?;
            let result = session
                .native_wait(condition, Duration::from_millis(*timeout_ms))
                .await?;
            Ok((Some(serde_json::to_vec(&result)?.len()), None, None))
        }
        BatchStep::Observe {
            include_dom,
            include_screenshot,
            include_form_values,
        } => {
            if *include_form_values && (*include_dom || *include_screenshot) {
                return Err("form values may only be combined with default compact observe".into());
            }
            let observation = session.native_observe().await?;
            let nodes = include_dom
                .then(|| session.native_semantic_nodes())
                .transpose()?;
            let screenshot = if *include_screenshot {
                Some(
                    base64::engine::general_purpose::STANDARD
                        .encode(session.native_capture_png_async().await?),
                )
            } else {
                None
            };
            let form_values = if *include_form_values {
                Some(session.script(NATIVE_FORM_VALUES_SCRIPT).await?.value)
            } else {
                None
            };
            let payload = json!({
                "contextId": observation.route.target_id,
                "revision": observation.revision,
                "url": observation.page.url,
                "title": observation.page.title,
                "visibleText": observation.text,
                "nodes": nodes,
                "screenshotPngBase64": screenshot,
                "formValues": form_values,
            });
            Ok((
                Some(serde_json::to_vec(&payload)?.len()),
                Some(observation.revision),
                None,
            ))
        }
        BatchStep::Screenshot => {
            let bytes = session.native_capture_png_async().await?;
            Ok((Some(bytes.len()), None, None))
        }
        BatchStep::Evaluate { expression } => {
            let result = session.script(expression).await?;
            Ok((Some(serde_json::to_vec(&result.value)?.len()), None, None))
        }
        BatchStep::AcceptDialog => {
            let result = session
                .native_resolve_dialog(PromptDecision::Accept)
                .await?;
            Ok((Some(serde_json::to_vec(&result)?.len()), None, None))
        }
        BatchStep::DismissDialog => {
            let result = session
                .native_resolve_dialog(PromptDecision::Dismiss)
                .await?;
            Ok((Some(serde_json::to_vec(&result)?.len()), None, None))
        }
    }
}

async fn execute_action(
    session: &BrowserRuntimeSession,
    action: SemanticAction,
    expected_revision: Option<u64>,
) -> BrowserResult<(Option<usize>, Option<u64>, Option<String>)> {
    let result = match expected_revision {
        Some(revision) => session.action_with_revision(action, revision).await?,
        None => session.action(action).await?,
    };
    if !result.accepted {
        return Err("native batch action was not accepted".into());
    }
    Ok((
        Some(serde_json::to_vec(&result)?.len()),
        Some(result.revision),
        Some(session.next_native_execution_id()),
    ))
}

fn target_for(step: &BatchStep) -> Option<&str> {
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

fn action_name(step: &BatchStep) -> String {
    match step {
        BatchStep::Navigate { .. } => "navigate",
        BatchStep::Click { .. } => "click",
        BatchStep::Type { .. } => "type",
        BatchStep::Check { .. } => "check",
        BatchStep::Uncheck { .. } => "uncheck",
        BatchStep::Select { .. } => "select",
        BatchStep::Clear { .. } => "clear",
        BatchStep::Scroll { .. } => "scroll",
        BatchStep::Wait { .. } => "wait",
        BatchStep::Observe { .. } => "observe",
        BatchStep::Screenshot => "screenshot",
        BatchStep::Evaluate { .. } => "evaluate",
        BatchStep::AcceptDialog => "acceptDialog",
        BatchStep::DismissDialog => "dismissDialog",
    }
    .to_owned()
}

fn scroll_deltas(dx: f64, dy: f64) -> BrowserResult<(i32, i32)> {
    if !dx.is_finite()
        || !dy.is_finite()
        || dx.fract() != 0.0
        || dy.fract() != 0.0
        || dx < f64::from(i32::MIN)
        || dx > f64::from(i32::MAX)
        || dy < f64::from(i32::MIN)
        || dy > f64::from(i32::MAX)
    {
        return Err("native scroll deltas must be finite integral 32-bit values".into());
    }
    Ok((dx as i32, dy as i32))
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

// This is intentionally a fixed, bounded browser query.  The policy gate is
// performed by the caller before the batch is dispatched; password controls
// never leave the engine in clear text.
const NATIVE_FORM_VALUES_SCRIPT: &str = r#"(() => {
  const controls = Array.from(document.querySelectorAll('input,textarea,select')).slice(0, 32);
  return controls.map((element, index) => {
    const type = String(element.type || '').toLowerCase();
    return {
      index,
      type,
      value: type === 'password' ? '[redacted]' : String(element.value || '')
    };
  });
})()"#;

#[cfg(test)]
mod tests {
    use super::scroll_deltas;

    #[test]
    fn rejects_fractional_or_non_finite_scroll_deltas() {
        assert!(scroll_deltas(1.5, 0.0).is_err());
        assert!(scroll_deltas(f64::NAN, 0.0).is_err());
        assert!(scroll_deltas(f64::from(i32::MAX) + 1.0, 0.0).is_err());
        assert_eq!(scroll_deltas(-4.0, 7.0).unwrap(), (-4, 7));
    }
}

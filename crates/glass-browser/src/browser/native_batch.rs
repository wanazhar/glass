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
use serde_json::{Value, json};
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

/// Read the bounded form-value projection used by direct observations and
/// batch observations. The caller must perform the policy capability check;
/// the sensitive variant is selected only when that caller explicitly allows
/// it.
pub(crate) async fn read_form_values(
    session: &BrowserRuntimeSession,
    allow_sensitive: bool,
) -> BrowserResult<Value> {
    let script = if allow_sensitive {
        NATIVE_SENSITIVE_FORM_VALUES_SCRIPT
    } else {
        NATIVE_FORM_VALUES_SCRIPT
    };
    Ok(session.script(script).await?.value)
}

/// Execute a bounded ordered native batch.
pub(crate) async fn run(
    session: &BrowserRuntimeSession,
    steps: &[BatchStep],
    atomic: bool,
    mode: BatchMode,
    expected_revision: Option<u64>,
) -> BrowserResult<BatchOutcome> {
    run_with_progress(session, steps, atomic, mode, expected_revision, |_| {}).await
}

/// Execute a native batch and report privacy-safe step boundaries to an
/// optional caller-owned progress sink. The event contains only the step
/// index, action name, duration, and outcome status.
pub(crate) async fn run_with_progress(
    session: &BrowserRuntimeSession,
    steps: &[BatchStep],
    atomic: bool,
    mode: BatchMode,
    expected_revision: Option<u64>,
    mut progress: impl FnMut(Value),
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
    progress(json!({"event":"batch_started","total":steps.len()}));

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
        let step_started = std::time::Instant::now();
        progress(json!({"event":"step_started","index":index,"action":action}));
        match execute_step(session, step, step_revision).await {
            Ok((response_bytes, resulting_revision, execution_id)) => {
                let elapsed_ms = step_started.elapsed().as_millis() as u64;
                outcomes.push(BatchStepOutcome::Success {
                    index,
                    action: action.clone(),
                    elapsed_ms,
                    response_bytes,
                    execution_id,
                });
                progress(
                    json!({"event":"step_finished","index":index,"action":action,"status":"ok","elapsedMs":elapsed_ms}),
                );
                completed += 1;
                if mode == BatchMode::Chain {
                    chained_revision = resulting_revision.or(chained_revision);
                }
            }
            Err(error) => {
                let elapsed_ms = step_started.elapsed().as_millis() as u64;
                outcomes.push(BatchStepOutcome::Error {
                    index,
                    action: action.clone(),
                    elapsed_ms,
                    message: bounded_text(&error.to_string(), 512),
                    execution_id: None,
                });
                progress(
                    json!({"event":"step_finished","index":index,"action":action,"status":"error","elapsedMs":elapsed_ms}),
                );
                failed += 1;
                break;
            }
        }
    }

    let final_revision = session.native_observe().await?.revision;
    let final_url = session
        .contexts()
        .await
        .ok()
        .and_then(|contexts| contexts.into_iter().find(|context| context.active))
        .map(|context| crate::browser::session::redact_diagnostic_url(&context.url));
    Ok(BatchOutcome {
        mode,
        initial_revision,
        final_revision,
        final_url,
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
        BatchStep::ClickObserved {
            role,
            name_contains,
            name_equals,
        } => {
            let (revision, target) = session
                .native_resolve_observed_target(role, name_contains, name_equals.as_deref())
                .await?;
            if expected_revision.is_some_and(|expected| expected != revision) {
                return Err("batch revision changed before observed target selection".into());
            }
            execute_action(
                session,
                SemanticAction::Click {
                    target: target.reference,
                },
                Some(revision),
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
        BatchStep::ScrollTarget { target, dx, dy } => {
            let (delta_x, delta_y) = scroll_deltas(*dx, *dy)?;
            execute_action(
                session,
                SemanticAction::ScrollTarget {
                    target: target.clone(),
                    delta_x,
                    delta_y,
                },
                expected_revision,
            )
            .await
        }
        BatchStep::ScrollTargetObserved {
            name_contains,
            tag_name,
            frame_id,
            dx,
            dy,
        } => {
            let (delta_x, delta_y) = scroll_deltas(*dx, *dy)?;
            let name_contains = name_contains
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_lowercase);
            let tag_name = tag_name
                .as_deref()
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_lowercase);
            if name_contains.is_none() && tag_name.is_none() {
                return Err("scrollTargetObserved requires nameContains or tagName".into());
            }
            if delta_x == 0 && delta_y == 0 {
                return Err("scrollTargetObserved requires a nonzero scroll delta".into());
            }
            let observation = session.native_observe().await?;
            if expected_revision.is_some_and(|expected| expected != observation.revision) {
                return Err(
                    "batch revision changed before observed scroll target selection".into(),
                );
            }
            let mut matches = observation.scroll_targets.iter().filter(|target| {
                name_contains
                    .as_ref()
                    .is_none_or(|name| target.name.to_lowercase().contains(name))
                    && tag_name
                        .as_ref()
                        .is_none_or(|tag| target.tag_name.eq_ignore_ascii_case(tag))
                    && frame_id
                        .as_deref()
                        .is_none_or(|frame| target.frame_id == frame)
                    && (delta_x == 0
                        || (delta_x > 0 && target.max_scroll_x > target.scroll_x)
                        || (delta_x < 0 && target.scroll_x > 0))
                    && (delta_y == 0
                        || (delta_y > 0 && target.max_scroll_y > target.scroll_y)
                        || (delta_y < 0 && target.scroll_y > 0))
            });
            let target = matches.next().ok_or_else(|| {
                let available = observation
                    .scroll_targets
                    .iter()
                    .take(8)
                    .map(|target| {
                        let name = target.name.chars().take(64).collect::<String>();
                        if name.is_empty() {
                            target.tag_name.clone()
                        } else {
                            format!("{}[{}]", target.tag_name, name)
                        }
                    })
                    .collect::<Vec<_>>();
                let suffix = if available.is_empty() {
                    "none".to_owned()
                } else {
                    available.join(", ")
                };
                format!(
                    "fresh observation has no matching scroll target; available targets: {suffix}"
                )
            })?;
            if matches.next().is_some() {
                return Err("fresh observation has ambiguous scroll targets".into());
            }
            execute_action(
                session,
                SemanticAction::ScrollTarget {
                    target: target.reference.clone(),
                    delta_x,
                    delta_y,
                },
                Some(observation.revision),
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
                Some(read_form_values(session, false).await?)
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
    let viewport_scroll = matches!(action, SemanticAction::Scroll { .. });
    let result = match expected_revision {
        Some(revision) => session.action_with_revision(action, revision).await?,
        None => session.action(action).await?,
    };
    if !result.accepted {
        if viewport_scroll {
            let observation = session.native_observe().await?;
            if let Some(viewport) = observation.limits.viewport {
                return Err(format!(
                    "native viewport scroll produced no movement (scrollY={}, documentHeight={}, viewportHeight={})",
                    viewport.scroll_y, viewport.document_height, viewport.height
                )
                .into());
            }
        }
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
        BatchStep::ClickObserved { .. } => "clickObserved",
        BatchStep::Type { .. } => "type",
        BatchStep::Check { .. } => "check",
        BatchStep::Uncheck { .. } => "uncheck",
        BatchStep::Select { .. } => "select",
        BatchStep::Clear { .. } => "clear",
        BatchStep::Scroll { .. } => "scroll",
        BatchStep::ScrollTarget { .. } => "scrollTarget",
        BatchStep::ScrollTargetObserved { .. } => "scrollTargetObserved",
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
  const controls = Array.from(document.querySelectorAll('input,textarea,select')).slice(0, 16);
  return controls.map((element, index) => {
    const type = String(element.type || '').toLowerCase();
    const value = type === 'password' ? '[redacted]' : String(element.value || '').slice(0, 256);
    return {
      index,
      type,
      value,
      checked: ['checkbox', 'radio'].includes(type) ? !!element.checked : undefined,
      selectedOption: type === 'select-one' && element.selectedIndex >= 0
        ? String(element.options[element.selectedIndex].text || '').slice(0, 128)
        : undefined,
      empty: !value,
      disabled: !!element.disabled,
      readOnly: !!element.readOnly,
      required: !!element.required
    };
  });
})()"#;

const NATIVE_SENSITIVE_FORM_VALUES_SCRIPT: &str = r#"(() => {
  const controls = Array.from(document.querySelectorAll('input,textarea,select')).slice(0, 16);
  return controls.map((element, index) => {
    const type = String(element.type || '').toLowerCase();
    const value = String(element.value || '').slice(0, 256);
    return {
      index,
      type,
      value,
      checked: ['checkbox', 'radio'].includes(type) ? !!element.checked : undefined,
      selectedOption: type === 'select-one' && element.selectedIndex >= 0
        ? String(element.options[element.selectedIndex].text || '').slice(0, 128)
        : undefined,
      empty: !value,
      disabled: !!element.disabled,
      readOnly: !!element.readOnly,
      required: !!element.required
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

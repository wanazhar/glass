//! Retry policies for browser operations.
//!
//! Retries are permitted only for typed failures that explicitly prove they
//! stopped before dispatch. Targeting ambiguity (element not found, ambiguous
//! selector, stale reference) and all post-dispatch or uncertain failures
//! always fail immediately.
//!
//! Glass never guesses a destructive target or repeats an operation whose
//! browser-side dispatch status is unknown.

use super::*;
use std::sync::Arc;
use std::time::Duration;

/// Predicate function for retry decisions.
pub type RetryPredicate = Arc<dyn Fn(&(dyn std::error::Error + 'static)) -> bool + Send + Sync>;

/// Policy controlling retry behaviour for browser operations.
///
/// Only typed failures that explicitly prove they stopped before dispatch
/// are eligible for retry. Targeting ambiguity and uncertain dispatch status
/// always fail immediately — Glass never guesses a destructive target.
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub backoff: Duration,
    pub predicate: Option<RetryPredicate>,
}

impl Clone for RetryPolicy {
    fn clone(&self) -> Self {
        Self {
            max_attempts: self.max_attempts,
            backoff: self.backoff,
            predicate: self.predicate.clone(),
        }
    }
}

impl std::fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryPolicy")
            .field("max_attempts", &self.max_attempts)
            .field("backoff", &self.backoff)
            .field("predicate", &self.predicate.as_ref().map(|_| ".."))
            .finish()
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: Duration::from_millis(200),
            predicate: None,
        }
    }
}

impl RetryPolicy {
    /// Create a retry policy with the given max attempts and backoff.
    ///
    /// `max_attempts` is clamped to a minimum of 1. No custom predicate
    /// is set — use the struct directly if you need one.
    pub fn new(max_attempts: usize, backoff: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            backoff,
            predicate: None,
        }
    }
}
fn is_retryable_pre_dispatch(error: &(dyn std::error::Error + 'static)) -> bool {
    if let Some(error) = error.downcast_ref::<ActionContractError>() {
        return error.phase == ActionFailurePhase::Preflight
            && error.recovery_strategy == RecoveryStrategy::RetrySafe;
    }
    if let Some(error) = error.downcast_ref::<ActionVerificationError>() {
        return matches!(
            error.phase,
            ActionFailurePhase::Preflight | ActionFailurePhase::TargetResolution
        ) && error.recovery_strategy == RecoveryStrategy::RetrySafe;
    }
    false
}

fn should_retry(policy: &RetryPolicy, error: &(dyn std::error::Error + 'static)) -> bool {
    is_retryable_pre_dispatch(error)
        && policy
            .predicate
            .as_ref()
            .is_none_or(|predicate| predicate(error))
}

impl BrowserSession {
    /// Click one element. Retries are limited to typed, pre-dispatch
    /// failures that explicitly opt into safe recovery.
    /// Targeting ambiguity and uncertain dispatch status always fail.
    pub async fn click_with_retry(
        &self,
        target: &str,
        policy: &RetryPolicy,
    ) -> BrowserResult<ActionOutcome> {
        for attempt in 1..=policy.max_attempts {
            match self.click(target).await {
                Ok(outcome) => return Ok(outcome),
                Err(error) => {
                    if !should_retry(policy, error.as_ref()) || attempt == policy.max_attempts {
                        return Err(error);
                    }
                    tracing::debug!(attempt, ?policy.backoff, "click retry");
                    tokio::time::sleep(policy.backoff).await;
                }
            }
        }
        unreachable!("loop always returns")
    }

    /// Navigate to a URL. Retries are limited to typed, pre-dispatch
    /// failures that explicitly opt into safe recovery.
    pub async fn navigate_with_retry(
        &self,
        url: &str,
        policy: &RetryPolicy,
    ) -> BrowserResult<PageInfo> {
        for attempt in 1..=policy.max_attempts {
            match self.navigate(url).await {
                Ok(page) => return Ok(page),
                Err(error) => {
                    if !should_retry(policy, error.as_ref()) || attempt == policy.max_attempts {
                        return Err(error);
                    }
                    tracing::debug!(attempt, ?policy.backoff, "navigate retry");
                    tokio::time::sleep(policy.backoff).await;
                }
            }
        }
        unreachable!("loop always returns")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_defaults_to_three_attempts() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.backoff, Duration::from_millis(200));
    }

    #[test]
    fn retry_policy_new_clamps_min_attempts() {
        let policy = RetryPolicy::new(0, Duration::from_millis(100));
        assert_eq!(policy.max_attempts, 1);
    }

    #[test]
    fn default_predicate_rejects_target_ambiguity() {
        let target_error = TargetError {
            kind: TargetErrorKind::Ambiguous,
            reason: None,
            candidates: vec![],
            recovery: None,
            diagnostics: None,
        };
        assert!(!is_retryable_pre_dispatch(&target_error));
    }

    #[test]
    fn default_predicate_rejects_stale_reference() {
        let target_error = TargetError {
            kind: TargetErrorKind::StaleReference,
            reason: None,
            candidates: vec![],
            recovery: None,
            diagnostics: None,
        };
        assert!(!is_retryable_pre_dispatch(&target_error));
    }

    #[test]
    fn default_predicate_rejects_not_actionable() {
        let target_error = TargetError {
            kind: TargetErrorKind::NotActionable,
            reason: None,
            candidates: vec![],
            recovery: None,
            diagnostics: None,
        };
        assert!(!is_retryable_pre_dispatch(&target_error));
    }

    #[test]
    fn retry_policy_supports_custom_predicate() {
        let policy = RetryPolicy {
            max_attempts: 2,
            backoff: Duration::from_millis(50),
            predicate: Some(Arc::new(|_| true)),
        };
        assert!(policy.predicate.is_some());
    }

    #[test]
    fn retry_policy_is_cloneable() {
        let policy = RetryPolicy::new(4, Duration::from_secs(1));
        let cloned = policy.clone();
        assert_eq!(cloned.max_attempts, 4);
        assert_eq!(cloned.backoff, Duration::from_secs(1));
    }

    #[test]
    fn custom_predicate_can_be_called() {
        let predicate: RetryPredicate = Arc::new(|_err: &(dyn std::error::Error + 'static)| true);
        let test_error: Box<dyn std::error::Error + 'static> =
            Box::new(std::io::Error::other("test"));
        assert!(predicate(test_error.as_ref()));

        let negative: RetryPredicate = Arc::new(|_err: &(dyn std::error::Error + 'static)| false);
        let test_error2: Box<dyn std::error::Error + 'static> =
            Box::new(std::io::Error::other("test2"));
        assert!(!negative(test_error2.as_ref()));
    }

    #[test]
    fn retry_policy_debug_output_hides_predicate_closure() {
        let policy = RetryPolicy::new(5, Duration::from_millis(10));
        let debug = format!("{:?}", policy);
        assert!(debug.contains("max_attempts: 5"));
        assert!(debug.contains("backoff: 10ms"));
        // Predicate is None, so it should not show ".."
        assert!(debug.contains("predicate: None"));
    }

    #[test]
    fn retry_policy_debug_output_indicates_custom_predicate() {
        let policy = RetryPolicy {
            max_attempts: 3,
            backoff: Duration::from_millis(100),
            predicate: Some(Arc::new(|_| true)),
        };
        let debug = format!("{:?}", policy);
        assert!(debug.contains("predicate: Some(\"..\")"));
    }
}

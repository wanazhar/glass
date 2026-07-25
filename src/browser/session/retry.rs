use super::*;
use std::sync::Arc;
use std::time::Duration;

/// Predicate function for retry decisions.
pub type RetryPredicate = Arc<dyn Fn(&(dyn std::error::Error + 'static)) -> bool + Send + Sync>;

/// Policy controlling retry behaviour for browser operations.
///
/// Transport and CDP protocol errors are retried. Targeting ambiguity
/// (element not found, ambiguous selector, stale reference) always
/// fails immediately — Glass never guesses a destructive target.
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
    pub fn new(max_attempts: usize, backoff: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            backoff,
            predicate: None,
        }
    }
}

fn is_retryable_default(error: &(dyn std::error::Error + 'static)) -> bool {
    if error.downcast_ref::<TargetError>().is_some() {
        return false;
    }
    if error
        .downcast_ref::<crate::browser::cdp::CdpError>()
        .is_some()
    {
        return true;
    }
    true
}

impl BrowserSession {
    /// Click one element with retry on transport/CDP errors.
    /// Targeting ambiguity always fails immediately.
    pub async fn click_with_retry(
        &self,
        target: &str,
        policy: &RetryPolicy,
    ) -> BrowserResult<ActionOutcome> {
        for attempt in 1..=policy.max_attempts {
            match self.click(target).await {
                Ok(outcome) => return Ok(outcome),
                Err(error) => {
                    let retryable = policy
                        .predicate
                        .as_ref()
                        .map(|pred| pred(error.as_ref()))
                        .unwrap_or_else(|| is_retryable_default(error.as_ref()));
                    if !retryable || attempt == policy.max_attempts {
                        return Err(error);
                    }
                    tracing::debug!(attempt, ?policy.backoff, "click retry");
                    tokio::time::sleep(policy.backoff).await;
                }
            }
        }
        unreachable!("loop always returns")
    }

    /// Navigate to a URL with retry on transport/CDP errors.
    pub async fn navigate_with_retry(
        &self,
        url: &str,
        policy: &RetryPolicy,
    ) -> BrowserResult<PageInfo> {
        for attempt in 1..=policy.max_attempts {
            match self.navigate(url).await {
                Ok(page) => return Ok(page),
                Err(error) => {
                    let retryable = policy
                        .predicate
                        .as_ref()
                        .map(|pred| pred(error.as_ref()))
                        .unwrap_or_else(|| is_retryable_default(error.as_ref()));
                    if !retryable || attempt == policy.max_attempts {
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
        };
        assert!(!is_retryable_default(&target_error));
    }

    #[test]
    fn default_predicate_rejects_stale_reference() {
        let target_error = TargetError {
            kind: TargetErrorKind::StaleReference,
            reason: None,
            candidates: vec![],
        };
        assert!(!is_retryable_default(&target_error));
    }

    #[test]
    fn default_predicate_rejects_not_actionable() {
        let target_error = TargetError {
            kind: TargetErrorKind::NotActionable,
            reason: None,
            candidates: vec![],
        };
        assert!(!is_retryable_default(&target_error));
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
}

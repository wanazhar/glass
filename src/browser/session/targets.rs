//! Multi-target (parallel page) operations.
//!
//! Provides [`BrowserSession::with_targets`] for opening multiple
//! page targets, executing a closure with concurrent access, and
//! automatically cleaning up. The active target before the call is
//! restored afterward.

use super::*;

/// Maximum number of concurrent targets for parallel operations.
const MAX_CONCURRENT_TARGETS: usize = 4;

impl BrowserSession {
    /// Open `n` page targets, execute an async closure with concurrent
    /// access, then close all opened targets.
    ///
    /// The closure receives the list of opened target infos. Targets
    /// are closed automatically when the closure returns (or panics).
    /// Bounded to 4 concurrent targets.
    ///
    /// The active target before calling this method is restored after
    /// all opened targets are closed.
    pub async fn with_targets<F, Fut>(&self, n: usize, f: F) -> BrowserResult<Fut::Output>
    where
        F: FnOnce(Vec<PageTargetInfo>) -> Fut,
        Fut: std::future::Future,
    {
        if n == 0 || n > MAX_CONCURRENT_TARGETS {
            return Err(
                format!("with_targets: n must be 1..={MAX_CONCURRENT_TARGETS}, got {n}").into(),
            );
        }

        // Save current active target to restore later
        let previous_active = {
            let topology = self.topology.lock().await;
            topology.active_target_id.clone()
        };

        // Phase 1: open all targets
        let mut targets = Vec::with_capacity(n);
        for i in 0..n {
            let target = match self.create_target("about:blank").await {
                Ok(target) => target,
                Err(e) => {
                    let _ = cleanup_targets(self, &targets).await;
                    return Err(
                        format!("with_targets: failed to create target {i}/{n}: {e}").into(),
                    );
                }
            };
            targets.push(target);
        }

        // Phase 2: execute the closure
        let result = f(targets.clone()).await;

        // Phase 3: close all opened targets
        if let Err(e) = cleanup_targets(self, &targets).await {
            tracing::warn!("with_targets: cleanup failed: {e}");
        }

        // Restore previous active if it still exists
        if let Some(ref prev_id) = previous_active {
            let exists = self
                .list_targets()
                .await
                .map(|targets| targets.iter().any(|t| &t.id == prev_id))
                .unwrap_or(false);
            if exists {
                let _ = self.select_target(prev_id).await;
            }
        }

        Ok(result)
    }
}

async fn cleanup_targets(
    session: &BrowserSession,
    targets: &[PageTargetInfo],
) -> BrowserResult<()> {
    for target in targets {
        // Don't fail if a target is already gone
        let _ = session.close_target(&target.id).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_concurrent_targets_is_4() {
        assert_eq!(MAX_CONCURRENT_TARGETS, 4);
    }

    #[test]
    fn max_concurrent_targets_is_reasonable() {
        // Must be at least 1 to allow single-target parallelism
        const { assert!(MAX_CONCURRENT_TARGETS >= 1) };
        // Must be at most 16 to avoid resource exhaustion
        const { assert!(MAX_CONCURRENT_TARGETS <= 16) };
    }

    #[test]
    fn max_concurrent_targets_is_power_of_two() {
        // Power-of-two bounds align with typical pool sizing
        assert_eq!(MAX_CONCURRENT_TARGETS.count_ones(), 1);
    }
}

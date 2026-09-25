use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tokio::sync::watch;

const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const COMMITTING: u8 = 2;

/// One top-level navigation's cancellation/commit race boundary.
#[derive(Clone)]
pub(crate) struct NativeNavigationCancellation {
    phase: Arc<AtomicU8>,
    signal: watch::Sender<bool>,
}

impl NativeNavigationCancellation {
    pub(crate) fn new() -> Self {
        let (signal, _) = watch::channel(false);
        Self {
            phase: Arc::new(AtomicU8::new(RUNNING)),
            signal,
        }
    }

    pub(crate) fn request_cancel(&self) -> bool {
        if self
            .phase
            .compare_exchange(RUNNING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        self.signal.send_replace(true);
        true
    }

    pub(crate) fn begin_commit(&self) -> bool {
        self.phase
            .compare_exchange(RUNNING, COMMITTING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.phase.load(Ordering::Acquire) == CANCELLED
    }

    pub(crate) async fn cancelled(&self) {
        let mut signal = self.signal.subscribe();
        loop {
            if self.is_cancelled() || *signal.borrow() {
                return;
            }
            if signal.changed().await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NativeNavigationCancellation;

    #[tokio::test]
    async fn cancellation_wins_before_commit_and_wakes_waiters() {
        let cancellation = NativeNavigationCancellation::new();
        let waiter = tokio::spawn({
            let cancellation = cancellation.clone();
            async move { cancellation.cancelled().await }
        });

        assert!(cancellation.request_cancel());
        waiter.await.unwrap();
        assert!(cancellation.is_cancelled());
        assert!(!cancellation.begin_commit());
        assert!(!cancellation.request_cancel());
    }

    #[test]
    fn commit_claim_prevents_late_cancellation() {
        let cancellation = NativeNavigationCancellation::new();

        assert!(cancellation.begin_commit());
        assert!(!cancellation.request_cancel());
        assert!(!cancellation.is_cancelled());
    }
}

//! Scoped diagnostic evidence collection.
//!
//! Produces a bounded [`DiagnosticReport`] with console messages, network
//! evidence, and page context. All secrets are redacted before output.

use super::*;

impl BrowserSession {
    /// Collect explicitly scoped, bounded, secret-redacted browser evidence.
    pub async fn diagnostics(&self, duration: Duration) -> BrowserResult<DiagnosticReport> {
        if duration.is_zero() || duration > MAX_DIAGNOSTIC_DURATION {
            return Err("diagnostic duration must be between 1 ms and 30 seconds".into());
        }
        self.cdp
            .with_current_route(async {
                let (target_id, frame_id) = self.route_identity().await?;
                let route_session_id = self.cdp.current_session_id();
                let mut events = self.cdp.subscribe_events_with_params();
                let mut guard = DiagnosticDomainGuard::acquire(
                    self.cdp.clone(),
                    Arc::clone(&self.network_wait_leases),
                    Arc::clone(&self.diagnostic_leases),
                )
                .await?;
                let started = tokio::time::Instant::now();
                let deadline = started + duration;
                let mut console = Vec::new();
                let mut network = Vec::new();
                let mut request_indexes = HashMap::new();
                let mut dropped_events = 0_u64;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        event = events.recv() => match event {
                            Ok(event) if event.session_id == route_session_id => collect_diagnostic_event(
                                &event,
                                &mut console,
                                &mut network,
                                &mut request_indexes,
                                &mut dropped_events,
                            ),
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                                dropped_events = dropped_events.saturating_add(count);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
                guard.disable().await?;
                Ok(DiagnosticReport {
                    target_id,
                    frame_id,
                    duration_ms: started.elapsed().as_millis() as u64,
                    console,
                    network,
                    dropped_events,
                })
            })
            .await
    }
}

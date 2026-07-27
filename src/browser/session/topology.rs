//! Small, bounded topology projections shared by recovery and diagnostics.

use super::*;

pub(crate) async fn trace_for(registry: &Arc<Mutex<TopologyRegistry>>) -> TopologyTrace {
    let topology = registry.lock().await;
    TopologyTrace {
        sequence: topology.sequence,
        active_target_id: topology.active_target_id.clone(),
        active_frame_id: topology.active_frame_id.clone(),
        target_count: topology.targets.len(),
        frame_count: topology.frames.len(),
        event_loss_count: topology.event_loss_count,
    }
}

//! The clock behind always-on: routines fire here.
//!
//! The scheduler is a single thread that wakes on a fixed tick, asks the
//! durable routine document what is due, and admits each due routine through
//! the same turn channel and pause queue a typed message uses. Nothing about an
//! unattended run takes a shortcut around the control layer.

use crate::events::EventBus;
use crate::notify::{self, Notification};
use crate::state::ProductState;
use crate::store::{self, StateStore};
use crate::worker::{TurnCommand, TurnRequest, now_millis};
use serde_json::json;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

/// How often the scheduler looks for due routines.
pub const TICK: Duration = Duration::from_secs(20);
/// Persist the heartbeat every this many ticks even when nothing fired.
const HEARTBEAT_TICKS: u64 = 15;

fn lock(state: &Mutex<ProductState>) -> std::sync::MutexGuard<'_, ProductState> {
    state.lock().unwrap_or_else(|error| error.into_inner())
}

/// Start the routine scheduler. The thread lives as long as the process.
pub fn spawn_scheduler(
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    events: EventBus,
    turn_tx: mpsc::Sender<TurnCommand>,
    tick: Duration,
) {
    let spawned = std::thread::Builder::new()
        .name("glass-marble-routines".into())
        .spawn(move || {
            let mut ticks = 0u64;
            loop {
                std::thread::sleep(tick);
                ticks = ticks.wrapping_add(1);
                run_tick(
                    &state,
                    store.as_deref(),
                    &events,
                    &turn_tx,
                    now_millis(),
                    ticks.is_multiple_of(HEARTBEAT_TICKS),
                );
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the routine scheduler");
    }
}

/// One scheduler wake-up: admit every due routine and persist the result.
/// Returns how many routines were admitted as turns.
pub fn run_tick(
    state: &Arc<Mutex<ProductState>>,
    store: Option<&StateStore>,
    events: &EventBus,
    turn_tx: &mpsc::Sender<TurnCommand>,
    now: u64,
    persist_heartbeat: bool,
) -> usize {
    let mut fired = 0usize;
    let mut notifications = Vec::new();
    let snapshot = {
        let mut guard = lock(state);
        guard.presence.beat();
        let due = guard.routines.due(now);
        // The heartbeat tick is where a months-long instance folds its document
        // back inside the retention bounds.
        let pruned = if persist_heartbeat {
            guard.prune(now)
        } else {
            crate::state::PruneSummary::default()
        };
        if due.is_empty() && !persist_heartbeat {
            return 0;
        }
        if !pruned.is_empty() {
            events.publish(
                "",
                "",
                "retention",
                json!({
                    "transcript": pruned.transcript,
                    "tasks": pruned.tasks,
                    "conversations": pruned.conversations,
                    "revision": guard.revision,
                }),
            );
        }
        for routine in due {
            if guard.control.paused {
                match guard
                    .control
                    .enqueue(routine.conversation_id.clone(), routine.prompt.clone())
                {
                    Ok(entry) => {
                        guard.routines.note_run(&routine.id, "", now);
                        guard.routines.note_status(&routine.id, "queued");
                        events.publish(
                            "",
                            "",
                            "routine",
                            json!({
                                "routineId": routine.id,
                                "title": routine.title,
                                "queued": true,
                                "queueId": entry.id,
                                "missed": routine.missed_last_gap,
                            }),
                        );
                        notifications.push(Notification::new(
                            "routine",
                            format!("{} is waiting", routine.title),
                            format!(
                                "Marble is paused, so this routine joined the queue: {}",
                                routine.prompt
                            ),
                            routine.conversation_id.as_deref().unwrap_or_default(),
                            "",
                        ));
                    }
                    Err(reason) => {
                        tracing::warn!(routine = %routine.id, %reason, "routine could not be queued");
                    }
                }
                continue;
            }
            let mut request =
                TurnRequest::new(routine.conversation_id.clone(), routine.prompt.clone());
            request.routine_id = Some(routine.id.clone());
            if turn_tx
                .send(TurnCommand {
                    request,
                    accepted: mpsc::channel().0,
                })
                .is_ok()
            {
                // The worker records the run when the durable task id exists, so
                // one firing is never counted twice.
                events.publish(
                    "",
                    "",
                    "routine",
                    json!({
                        "routineId": routine.id,
                        "title": routine.title,
                        "queued": false,
                        "missed": routine.missed_last_gap,
                    }),
                );
                let title = if routine.missed_last_gap > 0 {
                    format!(
                        "{} caught up after {} missed run{}",
                        routine.title,
                        routine.missed_last_gap,
                        if routine.missed_last_gap == 1 {
                            ""
                        } else {
                            "s"
                        }
                    )
                } else {
                    format!("{} started", routine.title)
                };
                notifications.push(Notification::new(
                    "routine",
                    title,
                    routine.prompt.clone(),
                    routine.conversation_id.as_deref().unwrap_or_default(),
                    "",
                ));
                fired += 1;
            } else {
                tracing::warn!(routine = %routine.id, "routine could not be admitted");
            }
        }
        guard.bump();
        guard.clone()
    };
    if let Some(store) = store {
        store::persist(store, &snapshot);
        if persist_heartbeat && let Some(path) = store.write_daily_backup(&snapshot, now) {
            tracing::info!(path = %path.display(), "wrote the daily product-state backup");
        }
    }
    for notification in notifications {
        notify::spawn_delivery(
            snapshot.notifications.clone(),
            snapshot.devices.clone(),
            notification,
        );
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_routine(schedule: &str) -> (Arc<Mutex<ProductState>>, String, u64) {
        let mut state = ProductState::default();
        let routine = state
            .routines
            .add("Tidy notes", "tidy the notes", schedule, Some("c1".into()))
            .unwrap();
        let next = routine.next_run_at;
        (Arc::new(Mutex::new(state)), routine.id, next)
    }

    #[test]
    fn a_due_routine_becomes_a_tagged_turn() {
        let (state, routine_id, next) = state_with_routine("every:10m");
        let (turn_tx, turn_rx) = mpsc::channel();
        let events = EventBus::default();
        let fired = run_tick(&state, None, &events, &turn_tx, next + 1000, false);
        assert_eq!(fired, 1);
        let command = turn_rx.try_recv().expect("the routine is admitted");
        assert_eq!(command.request.text, "tidy the notes");
        assert_eq!(command.request.conversation_id.as_deref(), Some("c1"));
        assert_eq!(
            command.request.routine_id.as_deref(),
            Some(routine_id.as_str())
        );

        let guard = state.lock().unwrap();
        assert_eq!(
            guard.routines.routines[0].runs, 0,
            "the worker counts a run when the durable task exists, not the scheduler"
        );
        assert!(guard.routines.routines[0].next_run_at > next);
        assert!(guard.presence.heartbeat_at > 0);
        assert!(events.since(0).iter().any(|event| event.kind == "routine"));
    }

    #[test]
    fn a_paused_assistant_queues_routines_instead_of_running_them() {
        let (state, routine_id, next) = state_with_routine("every:10m");
        state.lock().unwrap().control.paused = true;
        let (turn_tx, turn_rx) = mpsc::channel();
        let events = EventBus::default();
        let fired = run_tick(&state, None, &events, &turn_tx, next + 1000, false);
        assert_eq!(fired, 0);
        assert!(turn_rx.try_recv().is_err());
        let guard = state.lock().unwrap();
        assert_eq!(guard.control.queue.len(), 1);
        assert_eq!(guard.control.queue[0].text, "tidy the notes");
        assert_eq!(
            guard.routines.get(&routine_id).unwrap().last_status,
            "queued"
        );
    }

    #[test]
    fn a_tick_before_the_next_run_does_nothing() {
        let (state, _, next) = state_with_routine("every:10m");
        let (turn_tx, turn_rx) = mpsc::channel();
        let events = EventBus::default();
        assert_eq!(
            run_tick(&state, None, &events, &turn_tx, next - 30_000, false),
            0
        );
        assert!(turn_rx.try_recv().is_err());
    }
}

use super::error::NativeEngineError;

/// Deterministic logical clock used by the native engine and its tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeterministicClock {
    now_ms: u64,
}

impl DeterministicClock {
    pub const fn now_ms(self) -> u64 {
        self.now_ms
    }

    pub fn advance_by(&mut self, duration_ms: u64) -> Result<(), NativeEngineError> {
        self.now_ms =
            self.now_ms
                .checked_add(duration_ms)
                .ok_or_else(|| NativeEngineError::Scheduler {
                    reason: "logical clock overflow".into(),
                })?;
        Ok(())
    }
}

/// Typed commit tasks used by the local navigation kernel. Arbitrary callbacks
/// never cross this scheduler boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTask {
    CommitNavigation,
    CommitSameDocumentNavigation,
    TraverseHistory,
}

/// A bounded task with deterministic insertion order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledTask {
    pub id: u64,
    pub due_at_ms: u64,
    pub sequence: u64,
    pub task: NativeTask,
}

/// Ordered, bounded scheduler with a logical test clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicScheduler {
    clock: DeterministicClock,
    tasks: Vec<ScheduledTask>,
    next_id: u64,
    next_sequence: u64,
    max_tasks: usize,
}

impl DeterministicScheduler {
    pub(crate) fn new(max_tasks: usize) -> Result<Self, NativeEngineError> {
        if max_tasks == 0 {
            return Err(NativeEngineError::invalid(
                "max scheduler tasks",
                "must be greater than zero",
            ));
        }
        Ok(Self {
            clock: DeterministicClock::default(),
            tasks: Vec::new(),
            next_id: 1,
            next_sequence: 0,
            max_tasks,
        })
    }

    pub const fn clock(&self) -> DeterministicClock {
        self.clock
    }

    pub const fn pending_len(&self) -> usize {
        self.tasks.len()
    }

    pub fn advance_by(&mut self, duration_ms: u64) -> Result<(), NativeEngineError> {
        self.clock.advance_by(duration_ms)
    }

    pub fn schedule(&mut self, task: NativeTask, delay_ms: u64) -> Result<u64, NativeEngineError> {
        if self.tasks.len() >= self.max_tasks {
            return Err(NativeEngineError::limit(
                "scheduler queue",
                self.max_tasks,
                self.tasks.len().saturating_add(1),
            ));
        }
        let due_at_ms = self.clock.now_ms().checked_add(delay_ms).ok_or_else(|| {
            NativeEngineError::Scheduler {
                reason: "task deadline overflow".into(),
            }
        })?;
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "task ID space exhausted".into(),
            })?;
        let sequence = self.next_sequence;
        self.next_sequence =
            self.next_sequence
                .checked_add(1)
                .ok_or_else(|| NativeEngineError::Scheduler {
                    reason: "task sequence space exhausted".into(),
                })?;
        self.tasks.push(ScheduledTask {
            id,
            due_at_ms,
            sequence,
            task,
        });
        Ok(id)
    }

    pub fn pop_ready(&mut self) -> Option<ScheduledTask> {
        let now_ms = self.clock.now_ms();
        let index = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| task.due_at_ms <= now_ms)
            .min_by_key(|(_, task)| (task.due_at_ms, task.sequence))
            .map(|(index, _)| index)?;
        Some(self.tasks.remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_tasks_are_ordered_by_deadline_then_insertion() {
        let mut scheduler = DeterministicScheduler::new(4).unwrap();
        let delayed = scheduler
            .schedule(NativeTask::CommitNavigation, 10)
            .unwrap();
        let first = scheduler.schedule(NativeTask::CommitNavigation, 0).unwrap();
        let second = scheduler.schedule(NativeTask::CommitNavigation, 0).unwrap();

        assert_eq!(scheduler.pop_ready().unwrap().id, first);
        assert_eq!(scheduler.pop_ready().unwrap().id, second);
        assert!(scheduler.pop_ready().is_none());
        scheduler.advance_by(10).unwrap();
        assert_eq!(scheduler.pop_ready().unwrap().id, delayed);
    }

    #[test]
    fn queue_and_clock_bounds_fail_explicitly() {
        let mut scheduler = DeterministicScheduler::new(1).unwrap();
        scheduler.schedule(NativeTask::CommitNavigation, 0).unwrap();
        assert!(matches!(
            scheduler.schedule(NativeTask::CommitNavigation, 0),
            Err(NativeEngineError::LimitExceeded { resource, .. }) if resource == "scheduler queue"
        ));

        let mut clock = DeterministicClock { now_ms: u64::MAX };
        assert!(matches!(
            clock.advance_by(1),
            Err(NativeEngineError::Scheduler { .. })
        ));
    }
}

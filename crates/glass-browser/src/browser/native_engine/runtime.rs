use super::error::NativeEngineError;
use super::interaction::NativeEventKind;
use super::scheduler::{DeterministicScheduler, NativeTask, ScheduledTask};
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Maximum microtasks retained by one native runtime.
pub const MAX_NATIVE_MICROTASKS: usize = 256;
/// Maximum trace events retained by one native runtime.
pub const MAX_NATIVE_RUNTIME_TRACE: usize = 512;

/// Runtime lifecycle independent of the page/document lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRuntimeState {
    New,
    Running,
    Closed,
}

impl NativeRuntimeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Running => "running",
            Self::Closed => "closed",
        }
    }
}

/// Typed microtasks. Arbitrary callbacks and raw page data do not cross this
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMicrotask {
    DispatchEvent { kind: NativeEventKind },
    ResolvePromise { sequence: u64 },
    RunScript { script_id: u64 },
}

/// Runtime events retain ordering and IDs without retaining page source,
/// cookies, form values, or evaluated script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRuntimeTraceKind {
    Started,
    Closed,
    CancellationRequested,
    TaskScheduled,
    TaskReady,
    MicrotaskQueued,
    MicrotaskReady,
}

/// One bounded, privacy-safe runtime trace record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeRuntimeTraceEvent {
    pub sequence: u64,
    pub clock_ms: u64,
    pub kind: NativeRuntimeTraceKind,
    pub work_id: Option<u64>,
}

/// Cloneable one-shot cancellation signal for runtime work.
#[derive(Debug, Clone, Default)]
pub struct NativeCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl NativeCancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn check(&self, operation: &str) -> Result<(), NativeEngineError> {
        if self.is_cancelled() {
            return Err(NativeEngineError::Scheduler {
                reason: format!("{operation} was cancelled"),
            });
        }
        Ok(())
    }
}

/// Runtime substrate for one native engine owner.
///
/// This is intentionally synchronous and deterministic at this stage. It
/// establishes typed task/microtask ordering, cancellation, and privacy-safe
/// trace ownership before network, script, and content-process work is added.
#[derive(Debug, Clone)]
pub struct NativeRuntime {
    state: NativeRuntimeState,
    scheduler: DeterministicScheduler,
    microtasks: VecDeque<NativeMicrotask>,
    cancellation: NativeCancellationToken,
    trace: VecDeque<NativeRuntimeTraceEvent>,
    trace_truncated: bool,
    next_trace_sequence: u64,
    max_microtasks: usize,
}

impl NativeRuntime {
    pub(crate) fn new(max_tasks: usize) -> Result<Self, NativeEngineError> {
        Ok(Self {
            state: NativeRuntimeState::New,
            scheduler: DeterministicScheduler::new(max_tasks)?,
            microtasks: VecDeque::new(),
            cancellation: NativeCancellationToken::new(),
            trace: VecDeque::new(),
            trace_truncated: false,
            next_trace_sequence: 0,
            max_microtasks: MAX_NATIVE_MICROTASKS,
        })
    }

    pub const fn state(&self) -> NativeRuntimeState {
        self.state
    }

    pub const fn scheduler(&self) -> &DeterministicScheduler {
        &self.scheduler
    }

    pub fn cancellation_token(&self) -> NativeCancellationToken {
        self.cancellation.clone()
    }

    pub fn trace(&self) -> Vec<NativeRuntimeTraceEvent> {
        self.trace.iter().copied().collect()
    }

    pub const fn trace_truncated(&self) -> bool {
        self.trace_truncated
    }

    pub fn start(&mut self) -> Result<(), NativeEngineError> {
        match self.state {
            NativeRuntimeState::New => {
                self.cancellation = NativeCancellationToken::new();
                self.state = NativeRuntimeState::Running;
                self.record(NativeRuntimeTraceKind::Started, None)
            }
            NativeRuntimeState::Running => Ok(()),
            NativeRuntimeState::Closed => Err(Self::state_error(
                "start",
                "a closed native runtime cannot be reopened",
            )),
        }
    }

    pub fn close(&mut self) -> Result<(), NativeEngineError> {
        match self.state {
            NativeRuntimeState::New => Err(Self::state_error(
                "close",
                "the native runtime must be started before close",
            )),
            NativeRuntimeState::Running => {
                self.cancellation.cancel();
                self.scheduler.clear();
                self.microtasks.clear();
                self.state = NativeRuntimeState::Closed;
                self.record(NativeRuntimeTraceKind::CancellationRequested, None)?;
                self.record(NativeRuntimeTraceKind::Closed, None)
            }
            NativeRuntimeState::Closed => Err(Self::state_error(
                "close",
                "the native runtime is already closed",
            )),
        }
    }

    pub(crate) fn rollback_start(&mut self) -> Result<(), NativeEngineError> {
        if self.state != NativeRuntimeState::Running {
            return Err(Self::state_error(
                "rollback start",
                "only a running startup attempt can be rolled back",
            ));
        }
        if self.scheduler.pending_len() != 0 || !self.microtasks.is_empty() {
            return Err(NativeEngineError::Scheduler {
                reason: "cannot roll back a runtime with pending work".into(),
            });
        }
        self.state = NativeRuntimeState::New;
        self.cancellation = NativeCancellationToken::new();
        self.trace.clear();
        self.trace_truncated = false;
        self.next_trace_sequence = 0;
        Ok(())
    }

    pub fn request_cancellation(&mut self) -> Result<(), NativeEngineError> {
        self.require_running("cancel")?;
        self.cancellation.cancel();
        self.record(NativeRuntimeTraceKind::CancellationRequested, None)
    }

    pub fn schedule(&mut self, task: NativeTask, delay_ms: u64) -> Result<u64, NativeEngineError> {
        self.require_running("schedule task")?;
        self.cancellation.check("schedule task")?;
        let task_id = self.scheduler.schedule(task, delay_ms)?;
        self.record(NativeRuntimeTraceKind::TaskScheduled, Some(task_id))?;
        Ok(task_id)
    }

    pub fn pop_ready(&mut self) -> Result<Option<ScheduledTask>, NativeEngineError> {
        self.require_running("pop task")?;
        self.cancellation.check("pop task")?;
        let task = self.scheduler.pop_ready();
        if let Some(task) = task {
            self.record(NativeRuntimeTraceKind::TaskReady, Some(task.id))?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    pub fn advance_by(&mut self, duration_ms: u64) -> Result<(), NativeEngineError> {
        self.require_running("advance runtime clock")?;
        self.cancellation.check("advance runtime clock")?;
        self.scheduler.advance_by(duration_ms)
    }

    pub fn queue_microtask(&mut self, task: NativeMicrotask) -> Result<(), NativeEngineError> {
        self.require_running("queue microtask")?;
        self.cancellation.check("queue microtask")?;
        if self.microtasks.len() >= self.max_microtasks {
            return Err(NativeEngineError::limit(
                "runtime microtask queue",
                self.max_microtasks,
                self.microtasks.len().saturating_add(1),
            ));
        }
        self.microtasks.push_back(task);
        self.record(NativeRuntimeTraceKind::MicrotaskQueued, None)
    }

    pub fn pop_microtask(&mut self) -> Result<Option<NativeMicrotask>, NativeEngineError> {
        self.require_running("pop microtask")?;
        self.cancellation.check("pop microtask")?;
        let task = self.microtasks.pop_front();
        if task.is_some() {
            self.record(NativeRuntimeTraceKind::MicrotaskReady, None)?;
        }
        Ok(task)
    }

    fn require_running(&self, operation: &str) -> Result<(), NativeEngineError> {
        if self.state == NativeRuntimeState::Running {
            return Ok(());
        }
        Err(Self::state_error(
            operation,
            "the native runtime is not running",
        ))
    }

    fn state_error(operation: &str, reason: &str) -> NativeEngineError {
        NativeEngineError::Scheduler {
            reason: format!("{operation}: {reason}"),
        }
    }

    fn record(
        &mut self,
        kind: NativeRuntimeTraceKind,
        work_id: Option<u64>,
    ) -> Result<(), NativeEngineError> {
        let sequence = self.next_trace_sequence;
        self.next_trace_sequence = self.next_trace_sequence.checked_add(1).ok_or_else(|| {
            NativeEngineError::Scheduler {
                reason: "runtime trace sequence exhausted".into(),
            }
        })?;
        if self.trace.len() >= MAX_NATIVE_RUNTIME_TRACE {
            self.trace.pop_front();
            self.trace_truncated = true;
        }
        self.trace.push_back(NativeRuntimeTraceEvent {
            sequence,
            clock_ms: self.scheduler.clock().now_ms(),
            kind,
            work_id,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_orders_tasks_and_microtasks_without_raw_callbacks() {
        let mut runtime = NativeRuntime::new(4).unwrap();
        runtime.start().unwrap();
        let delayed = runtime.schedule(NativeTask::CommitNavigation, 10).unwrap();
        runtime
            .queue_microtask(NativeMicrotask::ResolvePromise { sequence: 1 })
            .unwrap();

        assert_eq!(
            runtime.pop_microtask().unwrap(),
            Some(NativeMicrotask::ResolvePromise { sequence: 1 })
        );
        assert!(runtime.pop_ready().unwrap().is_none());
        runtime.advance_by(10).unwrap();
        assert_eq!(runtime.pop_ready().unwrap().unwrap().id, delayed);

        let kinds: Vec<_> = runtime
            .trace()
            .into_iter()
            .map(|event| event.kind)
            .collect();
        assert_eq!(kinds[0], NativeRuntimeTraceKind::Started);
        assert!(kinds.contains(&NativeRuntimeTraceKind::MicrotaskReady));
        assert!(kinds.contains(&NativeRuntimeTraceKind::TaskReady));
    }

    #[test]
    fn cancellation_rejects_new_work_and_close_is_terminal() {
        let mut runtime = NativeRuntime::new(2).unwrap();
        runtime.start().unwrap();
        let token = runtime.cancellation_token();
        runtime.request_cancellation().unwrap();
        assert!(token.is_cancelled());
        assert!(matches!(
            runtime.schedule(NativeTask::CommitNavigation, 0),
            Err(NativeEngineError::Scheduler { reason }) if reason.contains("cancelled")
        ));
        runtime.close().unwrap();
        assert!(matches!(
            runtime.start(),
            Err(NativeEngineError::Scheduler { .. })
        ));
    }

    #[test]
    fn startup_rollback_restores_a_new_runtime() {
        let mut runtime = NativeRuntime::new(2).unwrap();
        runtime.start().unwrap();
        runtime.rollback_start().unwrap();
        assert_eq!(runtime.state(), NativeRuntimeState::New);
        runtime.start().unwrap();
        assert_eq!(runtime.state(), NativeRuntimeState::Running);
    }
}

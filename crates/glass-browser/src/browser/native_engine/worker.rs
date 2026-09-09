use super::error::NativeEngineError;
use super::runtime::{NativeMicrotask, NativeRuntime, NativeRuntimeState, NativeRuntimeTraceEvent};
use super::scheduler::{DeterministicScheduler, NativeTask, ScheduledTask};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot};
use tokio::task::AbortHandle;

const NATIVE_RUNTIME_WORKER_QUEUE: usize = 64;

/// Shared runtime state used by synchronous engine views and the async worker.
///
/// The worker serializes commands; the mutex keeps synchronous evidence/action
/// paths safe while an async command is in flight. The page engine remains the
/// sole owner of the state—this is not a second scheduler or a fallback path.
#[derive(Clone)]
pub(crate) struct NativeRuntimeShared {
    inner: Arc<Mutex<NativeRuntime>>,
}

impl NativeRuntimeShared {
    pub(crate) fn new(max_tasks: usize) -> Result<Self, NativeEngineError> {
        Ok(Self {
            inner: Arc::new(Mutex::new(NativeRuntime::new(max_tasks)?)),
        })
    }

    pub(crate) fn state(&self) -> NativeRuntimeState {
        self.inner
            .lock()
            .map(|runtime| runtime.state())
            .unwrap_or(NativeRuntimeState::Closed)
    }

    pub(crate) fn scheduler(&self) -> Result<DeterministicScheduler, NativeEngineError> {
        self.with_runtime("read scheduler", |runtime| Ok(runtime.scheduler().clone()))
    }

    pub(crate) fn trace(&self) -> Vec<NativeRuntimeTraceEvent> {
        self.inner
            .lock()
            .map(|runtime| runtime.trace())
            .unwrap_or_default()
    }

    pub(crate) fn start(&self) -> Result<(), NativeEngineError> {
        self.with_runtime("start", NativeRuntime::start)
    }

    pub(crate) fn rollback_start(&self) -> Result<(), NativeEngineError> {
        self.with_runtime("rollback start", NativeRuntime::rollback_start)
    }

    pub(crate) fn close(&self) -> Result<(), NativeEngineError> {
        self.with_runtime("close", NativeRuntime::close)
    }

    pub(crate) fn schedule(
        &self,
        task: NativeTask,
        delay_ms: u64,
    ) -> Result<u64, NativeEngineError> {
        self.with_runtime("schedule task", |runtime| runtime.schedule(task, delay_ms))
    }

    pub(crate) fn pop_ready(&self) -> Result<Option<ScheduledTask>, NativeEngineError> {
        self.with_runtime("pop task", NativeRuntime::pop_ready)
    }

    pub(crate) fn advance_by(&self, duration_ms: u64) -> Result<(), NativeEngineError> {
        self.with_runtime("advance runtime clock", |runtime| {
            runtime.advance_by(duration_ms)
        })
    }

    pub(crate) fn queue_microtask(&self, task: NativeMicrotask) -> Result<(), NativeEngineError> {
        self.with_runtime("queue microtask", |runtime| runtime.queue_microtask(task))
    }

    pub(crate) fn pop_microtask(&self) -> Result<Option<NativeMicrotask>, NativeEngineError> {
        self.with_runtime("pop microtask", NativeRuntime::pop_microtask)
    }

    pub(crate) fn request_cancellation(&self) -> Result<(), NativeEngineError> {
        self.with_runtime("cancel", NativeRuntime::request_cancellation)
    }

    fn with_runtime<T>(
        &self,
        operation: &str,
        action: impl FnOnce(&mut NativeRuntime) -> Result<T, NativeEngineError>,
    ) -> Result<T, NativeEngineError> {
        let mut runtime = self.inner.lock().map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "shared native runtime lock is poisoned".into(),
        })?;
        action(&mut runtime)
    }
}

/// Lifecycle state of the asynchronous runtime worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRuntimeWorkerState {
    New,
    Running,
    Closed,
    Crashed,
}

enum NativeRuntimeWorkerCommand {
    Start(oneshot::Sender<Result<(), NativeEngineError>>),
    RollbackStart(oneshot::Sender<Result<(), NativeEngineError>>),
    Close(oneshot::Sender<Result<(), NativeEngineError>>),
    Schedule {
        task: NativeTask,
        delay_ms: u64,
        reply: oneshot::Sender<Result<u64, NativeEngineError>>,
    },
    PopReady(oneshot::Sender<Result<Option<ScheduledTask>, NativeEngineError>>),
    Advance {
        duration_ms: u64,
        reply: oneshot::Sender<Result<(), NativeEngineError>>,
    },
    QueueMicrotask {
        task: NativeMicrotask,
        reply: oneshot::Sender<Result<(), NativeEngineError>>,
    },
    PopMicrotask(oneshot::Sender<Result<Option<NativeMicrotask>, NativeEngineError>>),
    Cancel(oneshot::Sender<Result<(), NativeEngineError>>),
    Trace(oneshot::Sender<Result<Vec<NativeRuntimeTraceEvent>, NativeEngineError>>),
}

struct NativeRuntimeWorkerInner {
    shared: NativeRuntimeShared,
    sender: mpsc::Sender<NativeRuntimeWorkerCommand>,
    alive: Arc<AtomicBool>,
    abort: AbortHandle,
}

impl Drop for NativeRuntimeWorkerInner {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

/// Asynchronous serialized command owner for one native runtime.
///
/// This is the first worker boundary, not the final hostile-content process
/// boundary. It prevents async navigation from running page work on the
/// caller's future, propagates cancellation through typed commands, reports a
/// crashed worker explicitly, and keeps all mutable runtime state shared with
/// the engine's synchronous projections.
#[derive(Clone)]
pub struct NativeRuntimeWorker {
    inner: Arc<NativeRuntimeWorkerInner>,
}

impl NativeRuntimeWorker {
    /// Spawn a worker with a fresh deterministic runtime.
    pub fn spawn(max_tasks: usize) -> Result<Self, NativeEngineError> {
        let shared = NativeRuntimeShared::new(max_tasks)?;
        Self::spawn_shared(shared)
    }

    pub(crate) fn spawn_shared(shared: NativeRuntimeShared) -> Result<Self, NativeEngineError> {
        tokio::runtime::Handle::try_current().map_err(|_| NativeEngineError::Worker {
            operation: "spawn".into(),
            reason: "native runtime worker requires an active Tokio runtime".into(),
        })?;
        let (sender, receiver) = mpsc::channel(NATIVE_RUNTIME_WORKER_QUEUE);
        let alive = Arc::new(AtomicBool::new(true));
        let task_alive = alive.clone();
        let task_shared = shared.clone();
        let join = tokio::spawn(async move {
            run_worker(task_shared, receiver).await;
            task_alive.store(false, Ordering::Release);
        });
        Ok(Self {
            inner: Arc::new(NativeRuntimeWorkerInner {
                shared,
                sender,
                alive,
                abort: join.abort_handle(),
            }),
        })
    }

    pub fn state(&self) -> NativeRuntimeWorkerState {
        if !self.inner.alive.load(Ordering::Acquire) {
            return NativeRuntimeWorkerState::Crashed;
        }
        match self.inner.shared.state() {
            NativeRuntimeState::New => NativeRuntimeWorkerState::New,
            NativeRuntimeState::Running => NativeRuntimeWorkerState::Running,
            NativeRuntimeState::Closed => NativeRuntimeWorkerState::Closed,
        }
    }

    pub async fn start(&self) -> Result<(), NativeEngineError> {
        self.request(|reply| NativeRuntimeWorkerCommand::Start(reply))
            .await
    }

    pub async fn rollback_start(&self) -> Result<(), NativeEngineError> {
        self.request(|reply| NativeRuntimeWorkerCommand::RollbackStart(reply))
            .await
    }

    pub async fn close(&self) -> Result<(), NativeEngineError> {
        self.request(|reply| NativeRuntimeWorkerCommand::Close(reply))
            .await
    }

    pub async fn schedule(
        &self,
        task: NativeTask,
        delay_ms: u64,
    ) -> Result<u64, NativeEngineError> {
        let (reply, response) = oneshot::channel();
        self.send_command(
            NativeRuntimeWorkerCommand::Schedule {
                task,
                delay_ms,
                reply,
            },
            "schedule task",
        )
        .await?;
        response.await.map_err(|_| self.crashed("schedule task"))?
    }

    pub async fn pop_ready(&self) -> Result<Option<ScheduledTask>, NativeEngineError> {
        self.request(|reply| NativeRuntimeWorkerCommand::PopReady(reply))
            .await
    }

    pub async fn advance_by(&self, duration_ms: u64) -> Result<(), NativeEngineError> {
        let (reply, response) = oneshot::channel();
        self.send_command(
            NativeRuntimeWorkerCommand::Advance { duration_ms, reply },
            "advance runtime clock",
        )
        .await?;
        response
            .await
            .map_err(|_| self.crashed("advance runtime clock"))?
    }

    pub async fn queue_microtask(&self, task: NativeMicrotask) -> Result<(), NativeEngineError> {
        let (reply, response) = oneshot::channel();
        self.send_command(
            NativeRuntimeWorkerCommand::QueueMicrotask { task, reply },
            "queue microtask",
        )
        .await?;
        response
            .await
            .map_err(|_| self.crashed("queue microtask"))?
    }

    pub async fn pop_microtask(&self) -> Result<Option<NativeMicrotask>, NativeEngineError> {
        self.request(|reply| NativeRuntimeWorkerCommand::PopMicrotask(reply))
            .await
    }

    pub async fn request_cancellation(&self) -> Result<(), NativeEngineError> {
        self.request(|reply| NativeRuntimeWorkerCommand::Cancel(reply))
            .await
    }

    pub async fn trace(&self) -> Result<Vec<NativeRuntimeTraceEvent>, NativeEngineError> {
        self.request(|reply| NativeRuntimeWorkerCommand::Trace(reply))
            .await
    }

    async fn request<T>(
        &self,
        command: impl FnOnce(
            oneshot::Sender<Result<T, NativeEngineError>>,
        ) -> NativeRuntimeWorkerCommand,
    ) -> Result<T, NativeEngineError> {
        let (reply, response) = oneshot::channel();
        self.send_command(command(reply), "runtime command").await?;
        response
            .await
            .map_err(|_| self.crashed("runtime command"))?
    }

    async fn send_command(
        &self,
        command: NativeRuntimeWorkerCommand,
        operation: &str,
    ) -> Result<(), NativeEngineError> {
        self.inner
            .sender
            .send(command)
            .await
            .map_err(|_| self.crashed(operation))
    }

    fn crashed(&self, operation: &str) -> NativeEngineError {
        NativeEngineError::Worker {
            operation: operation.into(),
            reason: "worker channel closed; restart is required before retry".into(),
        }
    }

    #[cfg(test)]
    fn abort_for_test(&self) {
        self.inner.alive.store(false, Ordering::Release);
        self.inner.abort.abort();
    }
}

async fn run_worker(
    shared: NativeRuntimeShared,
    mut receiver: mpsc::Receiver<NativeRuntimeWorkerCommand>,
) {
    while let Some(command) = receiver.recv().await {
        match command {
            NativeRuntimeWorkerCommand::Start(reply) => {
                let _ = reply.send(shared.start());
            }
            NativeRuntimeWorkerCommand::RollbackStart(reply) => {
                let _ = reply.send(shared.rollback_start());
            }
            NativeRuntimeWorkerCommand::Close(reply) => {
                let _ = reply.send(shared.close());
            }
            NativeRuntimeWorkerCommand::Schedule {
                task,
                delay_ms,
                reply,
            } => {
                let _ = reply.send(shared.schedule(task, delay_ms));
            }
            NativeRuntimeWorkerCommand::PopReady(reply) => {
                let _ = reply.send(shared.pop_ready());
            }
            NativeRuntimeWorkerCommand::Advance { duration_ms, reply } => {
                let _ = reply.send(shared.advance_by(duration_ms));
            }
            NativeRuntimeWorkerCommand::QueueMicrotask { task, reply } => {
                let _ = reply.send(shared.queue_microtask(task));
            }
            NativeRuntimeWorkerCommand::PopMicrotask(reply) => {
                let _ = reply.send(shared.pop_microtask());
            }
            NativeRuntimeWorkerCommand::Cancel(reply) => {
                let _ = reply.send(shared.request_cancellation());
            }
            NativeRuntimeWorkerCommand::Trace(reply) => {
                let _ = reply.send(Ok(shared.trace()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worker_serializes_typed_tasks_and_shares_trace_state() {
        let worker = NativeRuntimeWorker::spawn(4).unwrap();
        assert_eq!(worker.state(), NativeRuntimeWorkerState::New);
        worker.start().await.unwrap();
        let task_id = worker
            .schedule(NativeTask::CommitNavigation, 0)
            .await
            .unwrap();
        let task = worker.pop_ready().await.unwrap().unwrap();
        assert_eq!(task.id, task_id);
        assert_eq!(worker.state(), NativeRuntimeWorkerState::Running);
        assert!(worker
            .trace()
            .await
            .unwrap()
            .iter()
            .any(|event| event.kind == super::super::runtime::NativeRuntimeTraceKind::TaskReady));
        worker.close().await.unwrap();
        assert_eq!(worker.state(), NativeRuntimeWorkerState::Closed);
    }

    #[tokio::test]
    async fn worker_reports_cancellation_and_crash_without_page_data() {
        let worker = NativeRuntimeWorker::spawn(4).unwrap();
        worker.start().await.unwrap();
        worker.request_cancellation().await.unwrap();
        assert!(matches!(
            worker.schedule(NativeTask::CommitNavigation, 0).await,
            Err(NativeEngineError::Scheduler { reason }) if reason.contains("cancelled")
        ));
        worker.abort_for_test();
        tokio::task::yield_now().await;
        assert_eq!(worker.state(), NativeRuntimeWorkerState::Crashed);
        assert!(matches!(
            worker.trace().await,
            Err(NativeEngineError::Worker { .. })
        ));
    }
}

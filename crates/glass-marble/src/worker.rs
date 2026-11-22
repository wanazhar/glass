//! The durable turn worker.
//!
//! Work waits in one queue with two lanes: a message a person typed outranks a
//! routine firing, and a firing that is held back says so through presence.
//! A dispatcher hands each command to a lane, and each lane is a runtime
//! subprocess with one durable conversation of its own, so two chats make
//! progress at the same time instead of queueing behind the first. Within one
//! chat turns stay serial, in arrival order.
//! Each command is accepted with its durable task id as soon as the runtime
//! admits it, which lets the HTTP surface answer at once and stream the rest.
//!
//! The worker is also where the assistant behaves like an always-on assistant
//! rather than a request handler: it carries remembered facts into each turn,
//! notices details worth remembering, records presence, and notifies the person
//! when a turn lands or fails.

use crate::events::EventBus;
use crate::live::LiveStore;
use crate::memory;
use crate::notify::{self, Notification};
use crate::state::{ProductState, TaskRecord, TranscriptEntry};
use crate::store::{self, StateStore};
use crate::{
    MarbleError, MarbleTurn, TurnOptions, conversation::Conversation, conversation::SteerRequest,
};
#[cfg(test)]
use glass_runtime_client::TurnInput;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One message waiting for the durable turn worker.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub conversation_id: Option<String>,
    pub text: String,
    pub request_id: String,
    pub cancel: Arc<AtomicBool>,
    /// Set when a routine, not a person, admitted this turn.
    pub routine_id: Option<String>,
    /// Set when this firing's run was already counted while admission held it.
    /// Admitting it then records the task id without counting the run again.
    pub routine_counted: bool,
    /// Set when a person ran the routine by hand while it was disabled. The
    /// schedule is paused, but this firing was asked for, so it still runs.
    pub routine_override: bool,
}

/// Durable identity of an admitted turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedTurn {
    pub conversation_id: String,
    pub task_id: String,
}

/// Queued turn plus the acknowledgement channel used by the HTTP surface.
pub struct TurnCommand {
    pub request: TurnRequest,
    pub accepted: mpsc::Sender<Result<AcceptedTurn, MarbleError>>,
}

/// Cancel flags for the turns the worker is actually running.
///
/// The worker owns this table because it is the only place that knows a task id
/// for every turn: a routine firing on its own clock and a queued message
/// dispatched by resuming admission are both running turns, and a person who
/// presses stop should not have to care which one it is.
#[derive(Debug, Clone, Default)]
pub struct CancelRegistry {
    flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl CancelRegistry {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.flags.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn register(&self, task_id: &str, flag: Arc<AtomicBool>) {
        self.map().insert(task_id.to_string(), flag);
    }

    pub fn forget(&self, task_id: &str) {
        self.map().remove(task_id);
    }

    /// Ask one running turn to stop; returns whether it was still running.
    pub fn request(&self, task_id: &str) -> bool {
        let flag = self.map().get(task_id).cloned();
        match flag {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Every running turn, so one deliberate stop can cancel all of them.
    pub fn running(&self) -> Vec<(String, Arc<AtomicBool>)> {
        self.map()
            .iter()
            .map(|(task_id, flag)| (task_id.clone(), Arc::clone(flag)))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.map().len()
    }

    pub fn is_empty(&self) -> bool {
        self.map().is_empty()
    }
}

/// Where a correction goes: one channel per engine lane.
///
/// A correction must reach the engine that owns the running turn, and each lane
/// drains only its own channel. The HTTP surface resolves the conversation's
/// lane from the durable document and sends there.
#[derive(Debug, Clone, Default)]
pub struct SteerSink {
    lanes: Arc<Mutex<HashMap<u8, mpsc::Sender<SteerRequest>>>>,
}

impl SteerSink {
    /// One lane, for a host that runs a single engine.
    pub fn single(sender: mpsc::Sender<SteerRequest>) -> Self {
        let sink = Self::default();
        sink.add(0, sender);
        sink
    }

    /// Register the channel one engine drains.
    pub fn add(&self, lane: u8, sender: mpsc::Sender<SteerRequest>) {
        self.lanes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(lane, sender);
    }

    /// Hand one correction to a lane; false when that lane is not serving.
    pub fn send(&self, lane: u8, request: SteerRequest) -> bool {
        self.lanes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&lane)
            .map(|sender| sender.send(request).is_ok())
            .unwrap_or(false)
    }
}

/// Which lane a waiting command belongs to.
///
/// One engine lane runs one turn at a time, so work submitted now waits for the
/// running turn to settle. What it must not do is wait behind scheduled work: a
/// routine firing can run later, while a person is standing in front of the
/// assistant. Typed messages therefore drain first, and routine firings wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnLane {
    Human,
    Routine,
}

impl TurnLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Routine => "routine",
        }
    }
}

/// Commands waiting for the turn worker, one lane per priority.
struct Inbox {
    state: Mutex<InboxState>,
    ready: Condvar,
}

#[derive(Default)]
struct InboxState {
    human: VecDeque<TurnCommand>,
    routine: VecDeque<TurnCommand>,
    /// Live [`TurnSender`] handles; the last one leaving stops the worker.
    senders: usize,
    closed: bool,
}

fn lock_inbox(inbox: &Inbox) -> std::sync::MutexGuard<'_, InboxState> {
    inbox
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// Next command to run: typed messages first, then routine firings, each lane in
/// arrival order.
fn pop_next(state: &mut InboxState) -> Option<TurnCommand> {
    state
        .human
        .pop_front()
        .or_else(|| state.routine.pop_front())
}

/// The turn worker is not running, so nothing can be handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnWorkerGone;

impl fmt::Display for TurnWorkerGone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the turn worker is not running")
    }
}

impl std::error::Error for TurnWorkerGone {}

/// Hand a command to the turn worker.
///
/// Every clone keeps the worker alive, so dropping the last surface stops it
/// once its current turn settles.
type LaneQueues = Arc<Mutex<Vec<(Arc<Inbox>, Arc<AtomicUsize>)>>>;

pub struct TurnSender {
    inbox: Arc<Inbox>,
    live: bool,
    lanes: LaneQueues,
}

impl TurnSender {
    /// A sender with no worker behind it, for a surface that never dispatches.
    pub fn detached() -> Self {
        Self {
            inbox: Arc::new(Inbox {
                state: Mutex::new(InboxState::default()),
                ready: Condvar::new(),
            }),
            live: false,
            lanes: Arc::default(),
        }
    }

    /// Queue one command; the answer is its place among the commands waiting in
    /// front of it in the same lane.
    pub fn send(&self, command: TurnCommand) -> Result<usize, TurnWorkerGone> {
        self.send_owned(command).map_err(|_| TurnWorkerGone)
    }

    fn send_owned(&self, command: TurnCommand) -> Result<usize, Box<TurnCommand>> {
        if !self.live {
            return Err(Box::new(command));
        }
        let lane = command.request.lane();
        let mut state = lock_inbox(&self.inbox);
        if state.senders == 0 || state.closed {
            return Err(Box::new(command));
        }
        let position = match lane {
            TurnLane::Human => {
                state.human.push_back(command);
                state.human.len()
            }
            TurnLane::Routine => {
                state.routine.push_back(command);
                state.routine.len()
            }
        };
        drop(state);
        self.inbox.ready.notify_one();
        Ok(position)
    }

    pub(crate) fn remove_queued(&self, task_id: &str) {
        fn remove(inbox: &Inbox, task_id: &str) -> usize {
            let mut state = lock_inbox(inbox);
            let before = state.human.len() + state.routine.len();
            state
                .human
                .retain(|command| format!("queued-{}", command.request.request_id) != task_id);
            state
                .routine
                .retain(|command| format!("queued-{}", command.request.request_id) != task_id);
            before - state.human.len() - state.routine.len()
        }
        remove(&self.inbox, task_id);
        for (inbox, in_flight) in self
            .lanes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
        {
            in_flight.fetch_sub(remove(inbox, task_id), Ordering::Relaxed);
        }
    }

    /// Commands handed over that the worker has not started yet.
    pub fn waiting(&self) -> usize {
        self.waiting_in(TurnLane::Human) + self.waiting_in(TurnLane::Routine)
    }

    /// Commands waiting in one lane.
    pub fn waiting_in(&self, lane: TurnLane) -> usize {
        let state = lock_inbox(&self.inbox);
        match lane {
            TurnLane::Human => state.human.len(),
            TurnLane::Routine => state.routine.len(),
        }
    }
}

impl fmt::Debug for TurnSender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TurnSender")
            .field("live", &self.live)
            .field("waiting", &self.waiting())
            .finish()
    }
}

impl Clone for TurnSender {
    fn clone(&self) -> Self {
        if self.live {
            let mut state = lock_inbox(&self.inbox);
            state.senders = state.senders.saturating_add(1);
        }
        Self {
            inbox: Arc::clone(&self.inbox),
            live: self.live,
            lanes: self.lanes.clone(),
        }
    }
}

impl Drop for TurnSender {
    fn drop(&mut self) {
        if !self.live {
            return;
        }
        let last = {
            let mut state = lock_inbox(&self.inbox);
            state.senders = state.senders.saturating_sub(1);
            state.senders == 0
        };
        if last {
            // Nothing can send any more, so a waiting worker wakes up and stops.
            self.inbox.ready.notify_all();
        }
    }
}

/// The worker side of the turn queue.
pub struct TurnInbox {
    inbox: Arc<Inbox>,
}

impl Drop for TurnInbox {
    fn drop(&mut self) {
        lock_inbox(&self.inbox).closed = true;
    }
}

impl TurnInbox {
    /// A sender for every surface, and the worker's own receiving handle.
    pub fn channel() -> (TurnSender, Self) {
        let inbox = Arc::new(Inbox {
            state: Mutex::new(InboxState {
                human: VecDeque::new(),
                routine: VecDeque::new(),
                senders: 1,
                closed: false,
            }),
            ready: Condvar::new(),
        });
        (
            TurnSender {
                inbox: Arc::clone(&inbox),
                live: true,
                lanes: Arc::default(),
            },
            Self { inbox },
        )
    }

    /// Block until a command is waiting. `None` once every sender is gone.
    pub fn recv(&self) -> Option<TurnCommand> {
        let mut state = lock_inbox(&self.inbox);
        loop {
            if let Some(command) = pop_next(&mut state) {
                return Some(command);
            }
            if state.senders == 0 {
                return None;
            }
            state = self
                .inbox
                .ready
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    /// Wait at most `timeout` for a command.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<TurnCommand> {
        let state = lock_inbox(&self.inbox);
        let (mut state, _) = self
            .inbox
            .ready
            .wait_timeout_while(state, timeout, |state| {
                state.human.is_empty() && state.routine.is_empty() && state.senders > 0
            })
            .unwrap_or_else(|error| error.into_inner());
        pop_next(&mut state)
    }

    /// Take a command if one is already waiting.
    pub fn try_recv(&self) -> Option<TurnCommand> {
        pop_next(&mut lock_inbox(&self.inbox))
    }

    /// Commands waiting across both lanes.
    pub fn waiting(&self) -> usize {
        let state = lock_inbox(&self.inbox);
        state.human.len() + state.routine.len()
    }
}

impl fmt::Debug for TurnInbox {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TurnInbox")
            .field("waiting", &self.waiting())
            .finish()
    }
}

/// Current unix time in milliseconds.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn truncate(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let mut title: String = trimmed.chars().take(limit).collect();
    title.push('…');
    title
}

fn lock(state: &Mutex<ProductState>) -> std::sync::MutexGuard<'_, ProductState> {
    state.lock().unwrap_or_else(|error| error.into_inner())
}

/// Builds the engine session one lane runs on.
pub type LaneFactory = Arc<dyn Fn(u8) -> Result<Conversation, MarbleError> + Send + Sync>;

/// A pool of engine lanes and the channel that feeds them.
pub struct TurnPool {
    pub sender: TurnSender,
    pub steer: SteerSink,
}

/// One lane's runtime handle, as the dispatcher sees it.
struct LaneRuntime {
    sender: TurnSender,
    /// Commands handed to this lane that have not settled yet, including the
    /// one it is running. The dispatcher sends new chats to the lightest lane.
    in_flight: Arc<AtomicUsize>,
}

/// One lane's queue, created before its engine so work can be handed over while
/// that engine is still starting.
struct LaneQueue {
    sender: TurnSender,
    receiver: TurnInbox,
    steer_tx: mpsc::Sender<SteerRequest>,
    steer_rx: mpsc::Receiver<SteerRequest>,
    in_flight: Arc<AtomicUsize>,
}

/// What a lane thread needs besides its own engine session.
struct LaneParts {
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    live: LiveStore,
    events: EventBus,
    timeout: Duration,
    cancels: CancelRegistry,
}

/// Start a pool of durable turn lanes.
///
/// This is the only turn loop: a host with one engine passes one lane, and the
/// dispatcher simply sends every command to it. Lane zero starts immediately, so
/// an engine that cannot launch still fails at startup. Other lanes start on
/// first use: a host that never opens a second chat pays nothing for the
/// capacity.
#[allow(clippy::too_many_arguments)]
pub fn spawn_turn_pool(
    lanes: usize,
    factory: LaneFactory,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    live: LiveStore,
    events: EventBus,
    timeout: Duration,
    cancels: CancelRegistry,
) -> Result<TurnPool, MarbleError> {
    let lanes = lanes.max(1);
    let (sender, inbox) = TurnInbox::channel();
    let steer = SteerSink::default();
    let wiring = LaneWiring {
        queues: sender.lanes.clone(),
        factory,
        state: Arc::clone(&state),
        store,
        live,
        events,
        timeout,
        cancels,
        steer: steer.clone(),
    };
    let mut runtimes: Vec<Option<LaneRuntime>> = (0..lanes).map(|_| None).collect();
    runtimes[0] = Some(wiring.start(0)?);
    let spawned = std::thread::Builder::new()
        .name("glass-marble-turns".into())
        .spawn(move || {
            let mut next = 1;
            while let Some(command) = inbox.recv() {
                let lane = choose_lane(
                    &wiring.state,
                    command.request.conversation_id.as_deref(),
                    &runtimes,
                    &mut next,
                );
                if runtimes[lane].is_none() {
                    // Starting an engine launches a subprocess and greets it,
                    // which can take seconds. Doing that here would hold up
                    // every other chat, so the lane starts on its own thread and
                    // its queue takes the message now.
                    match wiring.spawn(lane as u8) {
                        Ok(runtime) => runtimes[lane] = Some(runtime),
                        Err(error) => {
                            undelivered(
                                &wiring.state,
                                wiring.store.as_deref(),
                                &wiring.events,
                                command,
                                error,
                            );
                            continue;
                        }
                    }
                }
                if let Some(command) = hand_over(&mut runtimes, lane, command) {
                    undelivered(
                        &wiring.state,
                        wiring.store.as_deref(),
                        &wiring.events,
                        command,
                        MarbleError::Protocol(
                            "the engine that would run this message stopped before it started, so the message was not delivered; send it again"
                                .into(),
                        ),
                    );
                }
            }
            // Dropping the lane senders ends every lane loop, which shuts its
            // engine down once the current turn has settled.
            drop(runtimes);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "could not start the marble turn dispatcher");
        return Err(MarbleError::Io(error));
    }
    Ok(TurnPool { sender, steer })
}

/// What every lane thread needs besides its own engine session.
struct LaneWiring {
    queues: LaneQueues,
    factory: LaneFactory,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    live: LiveStore,
    events: EventBus,
    timeout: Duration,
    cancels: CancelRegistry,
    steer: SteerSink,
}

impl LaneWiring {
    /// The queue one lane drains, ready before its engine is.
    fn queue(&self) -> LaneQueue {
        let (sender, receiver) = TurnInbox::channel();
        let (steer_tx, steer_rx) = mpsc::channel();
        let in_flight = Arc::new(AtomicUsize::new(0));
        self.queues
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((sender.inbox.clone(), in_flight.clone()));
        LaneQueue {
            sender,
            receiver,
            steer_tx,
            steer_rx,
            in_flight,
        }
    }

    /// Handles a lane thread needs besides its own engine.
    fn parts(&self) -> LaneParts {
        LaneParts {
            state: Arc::clone(&self.state),
            store: self.store.clone(),
            live: self.live.clone(),
            events: self.events.clone(),
            timeout: self.timeout,
            cancels: self.cancels.clone(),
        }
    }

    /// Start one lane's engine and the thread that drains its queue, waiting for
    /// the engine to answer.
    ///
    /// Lane zero starts this way, so a host whose engine cannot launch fails at
    /// startup instead of on the first message.
    fn start(&self, index: u8) -> Result<LaneRuntime, MarbleError> {
        let LaneQueue {
            sender,
            receiver,
            steer_tx,
            steer_rx,
            in_flight,
        } = self.queue();
        let conversation = (self.factory)(index)?.with_steering(steer_rx);
        self.steer.add(index, steer_tx);
        let runtime = LaneRuntime {
            sender: sender.clone(),
            in_flight: Arc::clone(&in_flight),
        };
        let parts = self.parts();
        std::thread::Builder::new()
            .name(format!("glass-marble-turn-{index}"))
            .spawn(move || drain(index, receiver, in_flight, conversation, parts))
            .map_err(MarbleError::Io)?;
        Ok(runtime)
    }

    /// Start one lane's engine off the calling thread.
    ///
    /// The queue exists at once, so the dispatcher hands over work and moves on
    /// while this engine launches. A lane whose engine cannot start keeps
    /// answering every message it is handed with that failure: nothing waits
    /// behind an engine that will never run.
    fn spawn(&self, index: u8) -> Result<LaneRuntime, MarbleError> {
        let LaneQueue {
            sender,
            receiver,
            steer_tx,
            steer_rx,
            in_flight,
        } = self.queue();
        let runtime = LaneRuntime {
            sender: sender.clone(),
            in_flight: Arc::clone(&in_flight),
        };
        drop(sender);
        let factory = Arc::clone(&self.factory);
        let steer = self.steer.clone();
        let parts = self.parts();
        std::thread::Builder::new()
            .name(format!("glass-marble-turn-{index}"))
            .spawn(move || {
                let conversation = match (factory)(index) {
                    Ok(conversation) => conversation.with_steering(steer_rx),
                    Err(error) => {
                        // This engine never came up, so this lane can never run
                        // a turn. Answer every message it is handed, now and
                        // later, instead of leaving them in a queue nothing
                        // drains.
                        let reason = error.to_string();
                        while let Some(command) = receiver.recv() {
                            in_flight.fetch_sub(1, Ordering::Relaxed);
                            undelivered(
                                &parts.state,
                                parts.store.as_deref(),
                                &parts.events,
                                command,
                                MarbleError::Protocol(reason.clone()),
                            );
                        }
                        return;
                    }
                };
                steer.add(index, steer_tx);
                drain(index, receiver, in_flight, conversation, parts);
            })
            .map_err(MarbleError::Io)?;
        Ok(runtime)
    }
}

/// Hand one command to a lane, taking a dead lane out of service.
///
/// A dead lane must not swallow the message: the command comes back so the
/// caller can be told it will not run, and the lane is retired so later chats
/// are not handed to it.
fn hand_over(
    runtimes: &mut [Option<LaneRuntime>],
    lane: usize,
    command: TurnCommand,
) -> Option<TurnCommand> {
    let Some(runtime) = runtimes[lane].as_ref() else {
        return Some(command);
    };
    runtime.in_flight.fetch_add(1, Ordering::Relaxed);
    match runtime.sender.send_owned(command) {
        Ok(_) => None,
        Err(command) => {
            Arc::clone(&runtime.in_flight).fetch_sub(1, Ordering::Relaxed);
            runtimes[lane] = None;
            Some(*command)
        }
    }
}

/// Answer work that will never run.
///
/// The surface may already have answered `202 queued` and stopped listening —
/// admission gives up long before an engine can finish starting — so the durable
/// row and the event stream carry the failure too. A message that cannot run
/// must not sit in the document looking like it is still waiting its turn.
fn undelivered(
    state: &Arc<Mutex<ProductState>>,
    store: Option<&StateStore>,
    events: &EventBus,
    command: TurnCommand,
    error: MarbleError,
) {
    let TurnCommand { request, accepted } = command;
    let queued_id = format!("queued-{}", request.request_id);
    let conversation_id = request.conversation_id.clone().unwrap_or_default();
    let message = error.to_string();
    let snapshot = {
        let mut guard = lock(state);
        if guard
            .tasks
            .iter()
            .any(|task| task.id == queued_id && task.status == "cancelled")
        {
            return;
        }
        guard.set_task_status(&queued_id, "failed");
        guard.bump();
        guard.clone()
    };
    if let Some(store) = store {
        store::persist(store, &snapshot);
    }
    // Whoever is still waiting for admission hears the failure; a surface that
    // already answered `202` learns it from the row and the stream.
    let _ = accepted.send(Err(error));
    events.publish(
        &conversation_id,
        &queued_id,
        "error",
        json!({"message": message, "revision": snapshot.revision}),
    );
}

/// Drain one lane's queue until the last sender is gone.
fn drain(
    lane: u8,
    receiver: TurnInbox,
    in_flight: Arc<AtomicUsize>,
    mut conversation: Conversation,
    parts: LaneParts,
) {
    loop {
        let Some(command) = receiver.recv_timeout(Duration::from_secs(1)) else {
            if lock_inbox(&receiver.inbox).senders == 0 {
                break;
            }
            conversation.check_idle_transport(&parts.state);
            continue;
        };
        run_command(
            command,
            lane,
            &mut conversation,
            &parts.state,
            parts.store.as_deref(),
            &parts.live,
            &parts.events,
            parts.timeout,
            &parts.cancels,
        );
        in_flight.fetch_sub(1, Ordering::Relaxed);
    }
    let _ = conversation.shutdown();
}

/// Which lane takes the next command.
///
/// A conversation belongs to the engine that created its session, so its lane
/// is recorded on the conversation and reused. New chats go to the lane with
/// the fewest turns ahead, and ties rotate so two fresh chats do not land on
/// the same engine.
fn choose_lane(
    state: &Mutex<ProductState>,
    conversation_id: Option<&str>,
    runtimes: &[Option<LaneRuntime>],
    next: &mut usize,
) -> usize {
    if runtimes.is_empty() {
        return 0;
    }
    if let Some(id) = conversation_id.filter(|id| !id.is_empty()) {
        let bound = state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .conversations
            .iter()
            .find(|record| record.id == id)
            .map(|record| usize::from(record.lane));
        if let Some(lane) = bound {
            return lane.min(runtimes.len() - 1);
        }
    }
    let mut best = *next % runtimes.len();
    let mut best_load = usize::MAX;
    for offset in 0..runtimes.len() {
        let index = (*next + offset) % runtimes.len();
        let load = runtimes[index]
            .as_ref()
            .map(|runtime| runtime.in_flight.load(Ordering::Relaxed))
            .unwrap_or(0);
        if load < best_load {
            best = index;
            best_load = load;
        }
    }
    *next = (best + 1) % runtimes.len();
    best
}

/// Whether a routine firing is still wanted at the moment it is admitted.
///
/// A firing can wait behind typed messages, behind another turn, or in the pause
/// queue, and the routine may be removed or disabled meanwhile. A removed
/// routine always drops its firing; a disabled one only does so unless a person
/// ran it by hand, which overrides the pause.
fn firing_still_wanted(
    state: &Mutex<ProductState>,
    routine_id: &str,
    override_pause: bool,
) -> bool {
    lock(state)
        .routines
        .get(routine_id)
        .is_some_and(|routine| routine.enabled || override_pause)
}

#[allow(clippy::too_many_arguments)]
fn run_command(
    command: TurnCommand,
    lane: u8,
    conversation: &mut Conversation,
    state: &Arc<Mutex<ProductState>>,
    store: Option<&StateStore>,
    live: &LiveStore,
    events: &EventBus,
    timeout: Duration,
    cancels: &CancelRegistry,
) {
    let TurnCommand { request, accepted } = command;
    let mut sender = Some(accepted);
    // The surface records a queued placeholder before handing the message over,
    // so a `202 queued` is visible in the document. The real task replaces it as
    // soon as the engine issues an id, and it is removed if the turn never
    // starts at all.
    let queued_id = format!("queued-{}", request.request_id);
    {
        let mut guard = lock(state);
        if guard
            .tasks
            .iter()
            .any(|task| task.id == queued_id && task.status == "cancelled")
        {
            return;
        }
        guard.set_task_status(&queued_id, "running");
    }
    // A firing waits in the routine lane, and the routine can be removed or
    // disabled while it waits. Running it then would be work nobody asked for
    // any more, so drop the placeholder and say why instead of starting a turn.
    if let Some(routine_id) = request.routine_id.clone()
        && !firing_still_wanted(state, &routine_id, request.routine_override)
    {
        let snapshot = {
            let mut guard = lock(state);
            guard.tasks.retain(|task| task.id != queued_id);
            guard.bump();
            guard.clone()
        };
        if let Some(store) = store {
            store::persist(store, &snapshot);
        }
        events.publish(
            "",
            "",
            "routine",
            json!({
                "routineId": routine_id,
                "skipped": true,
                "reason": "the routine was removed or disabled while its firing waited",
            }),
        );
        return;
    }
    // A turn cannot start on an engine that stopped answering. Replacing it
    // here, before the durable task exists, means the person gets either a real
    // answer or a clear failure — never a turn that was silently lost.
    match conversation.ensure_runtime() {
        Ok(false) => {}
        Ok(true) => {
            let (snapshot, restarts) = {
                let mut guard = lock(state);
                guard
                    .presence
                    .runtime
                    .note_engine_restart(conversation.engine_id());
                guard.bump();
                let restarts = guard.presence.runtime.restarts;
                (guard.clone(), restarts)
            };
            if let Some(store) = store {
                store::persist(store, &snapshot);
            }
            events.publish(
                "",
                "",
                "runtime",
                json!({"restarted": true, "restarts": restarts, "revision": snapshot.revision}),
            );
        }
        Err(error) => {
            {
                let mut guard = lock(state);
                guard
                    .presence
                    .runtime
                    .note_engine_failure(conversation.engine_id(), error.to_string());
                guard.tasks.retain(|task| task.id != queued_id);
                guard.bump();
            }
            events.publish(
                "",
                "",
                "runtime",
                json!({"restarted": false, "error": error.to_string()}),
            );
            if let Some(sender) = sender.take() {
                let _ = sender.send(Err(error));
            }
            return;
        }
    }
    let prompt = request.text.clone();
    // Journal the turn while it runs: the durable document only learns about the
    // reply when the turn settles, so a crash in between used to lose every word
    // a person had already watched arrive.
    let mut live_turn = live.begin(
        &prompt,
        "Marble",
        request.conversation_id.as_deref().unwrap_or_default(),
        &request.request_id,
    );
    // A model chosen from a surface applies to conversations created from now
    // on; Core keeps an existing session on the model it started with.
    let model = {
        let guard = lock(state);
        guard.model.trim().to_string()
    };
    conversation.set_model((!model.is_empty()).then_some(model));
    let turn = MarbleTurn {
        conversation_id: request.conversation_id.clone(),
        request_id: request.request_id.clone(),
        input: crate::normalize::engine_input(
            &lock(state),
            request.conversation_id.as_deref(),
            "marble",
            &prompt,
        ),
    };
    let mut accepted_turn: Option<AcceptedTurn> = None;
    let routine_id = request.routine_id.clone();
    let cancel = Arc::clone(&request.cancel);
    let mut on_accepted = |conversation_id: &str, task_id: &str| {
        let identity = AcceptedTurn {
            conversation_id: conversation_id.to_string(),
            task_id: task_id.to_string(),
        };
        accepted_turn = Some(identity.clone());
        // The worker registers the flag, so every admitted turn is cancellable —
        // including one a routine fired or one a resume dispatched.
        cancels.register(task_id, Arc::clone(&cancel));
        let snapshot = {
            let mut guard = lock(state);
            guard.presence.mark_turn();
            guard.bump();
            let now = now_millis();
            let title = truncate(&prompt, 64);
            let record = guard.conversation_mut(conversation_id);
            // The lane owns this session, so a later turn or a correction comes
            // back to the same engine instead of a second one that has never
            // heard of this conversation.
            record.lane = lane;
            if record.title.is_empty() {
                record.title = title;
            }
            record.updated_at = now;
            if !record.task_ids.iter().any(|id| id == task_id) {
                record.task_ids.push(task_id.to_string());
            }
            guard.tasks.retain(|task| task.id != queued_id);
            crate::everyone::claim(&mut guard, &queued_id, task_id);
            guard.upsert_task(TaskRecord {
                marble_id: String::new(),
                id: task_id.to_string(),
                conversation_id: conversation_id.to_string(),
                status: "running".into(),
                owner: "agent".into(),
                prompt: prompt.clone(),
                at: now,
                routine_id: routine_id.clone().unwrap_or_default(),
            });
            guard.push_transcript(TranscriptEntry {
                conversation_id: conversation_id.to_string(),
                kind: "user".into(),
                text: prompt.clone(),
                task_id: task_id.to_string(),
                at: now,
                ..Default::default()
            });
            if let Some(routine_id) = &routine_id {
                if request.routine_counted {
                    // The firing was counted when admission held it, so record
                    // which task carries it instead of counting it twice.
                    guard.routines.note_admitted(routine_id, task_id);
                } else {
                    guard.routines.note_run(routine_id, task_id, now);
                }
            }
            guard.memory.note_use("marble");
            guard.clone()
        };
        if let Some(store) = store {
            store::persist(store, &snapshot);
        }
        // The conversation is durable with its lane before the caller is told
        // it exists, so the very next turn or correction is routed to the
        // engine that owns the session instead of guessing lane zero.
        if let Some(sender) = sender.take() {
            let _ = sender.send(Ok(identity));
        }
        events.publish(
            conversation_id,
            task_id,
            "state",
            json!({"revision": snapshot.revision, "status": "running"}),
        );
    };
    let result = crate::run_turn(
        &turn,
        conversation,
        TurnOptions {
            cancel: Some(request.cancel.as_ref()),
            timeout,
            on_accepted: Some(&mut on_accepted),
            live: live_turn.as_mut(),
        },
    );
    match result {
        Ok(reply) => {
            cancels.forget(&reply.task_id);
            let (snapshot, suggestion) = {
                let mut guard = lock(state);
                guard.presence.mark_settled();
                guard.presence.runtime.note_ok();
                guard.bump();
                let now = now_millis();
                let status = if reply.cancelled {
                    "cancelled"
                } else {
                    "completed"
                };
                guard.set_task_status(&reply.task_id, status);
                guard.conversation_mut(&reply.conversation_id).updated_at = now;
                if !reply.reasoning.trim().is_empty() {
                    guard.push_transcript(TranscriptEntry {
                        conversation_id: reply.conversation_id.clone(),
                        kind: "reasoning".into(),
                        text: reply.reasoning.clone(),
                        task_id: reply.task_id.clone(),
                        at: now,
                        ..Default::default()
                    });
                }
                if !reply.text.trim().is_empty() {
                    guard.push_transcript(TranscriptEntry {
                        conversation_id: reply.conversation_id.clone(),
                        kind: "assistant".into(),
                        text: reply.text.clone(),
                        task_id: reply.task_id.clone(),
                        at: now,
                        ..Default::default()
                    });
                }
                guard.routines.note_settled(&reply.task_id, status);
                crate::everyone::mirror_settlement(&mut guard, &reply.task_id, &reply.text, status);
                // A durable detail is offered, never memorized silently.
                let suggestion = memory::detect(&prompt).and_then(|(text, kind, reason)| {
                    guard
                        .memory
                        .suggest(&text, &kind, &reason, &reply.conversation_id)
                        .map(|suggestion| (suggestion, kind))
                });
                (guard.clone(), suggestion)
            };
            if let Some(store) = store {
                store::persist(store, &snapshot);
            }
            events.publish(
                &reply.conversation_id,
                &reply.task_id,
                "state",
                json!({
                    "revision": snapshot.revision,
                    "status": if reply.cancelled { "cancelled" } else { "completed" },
                }),
            );
            if let Some((suggestion, kind)) = suggestion {
                events.publish(
                    &reply.conversation_id,
                    &reply.task_id,
                    "memory_suggestion",
                    json!({
                        "suggestionId": suggestion.id,
                        "text": suggestion.text,
                        "kind": kind,
                    }),
                );
                notify::spawn_delivery(
                    snapshot.notifications.clone(),
                    snapshot.devices.clone(),
                    Notification::new(
                        "memory",
                        "Remember this?",
                        format!("Marble can remember: {}", suggestion.text),
                        &reply.conversation_id,
                        &reply.task_id,
                    ),
                );
            }
            if !reply.cancelled {
                let title = snapshot
                    .conversations
                    .iter()
                    .find(|record| record.id == reply.conversation_id)
                    .map(|record| record.title.clone())
                    .unwrap_or_else(|| "Marble".into());
                notify::spawn_delivery(
                    snapshot.notifications.clone(),
                    snapshot.devices.clone(),
                    Notification::new(
                        "settled",
                        title,
                        reply.preview(220),
                        &reply.conversation_id,
                        &reply.task_id,
                    ),
                );
            }
        }
        Err(error) => {
            if conversation.transport_failed() {
                lock(state)
                    .presence
                    .runtime
                    .note_engine_failure(conversation.engine_id(), error.to_string());
            }
            if let Some(sender) = sender.take() {
                // The turn never started, so the queued placeholder must not
                // outlive it and look like work that is still waiting.
                let snapshot = {
                    let mut guard = lock(state);
                    guard.tasks.retain(|task| task.id != queued_id);
                    guard.bump();
                    guard.clone()
                };
                if let Some(store) = store {
                    store::persist(store, &snapshot);
                }
                let _ = sender.send(Err(error));
                return;
            }
            if let Some(identity) = &accepted_turn {
                cancels.forget(&identity.task_id);
                let snapshot = {
                    let mut guard = lock(state);
                    guard.presence.mark_settled();
                    guard.bump();
                    guard.set_task_status(&identity.task_id, "failed");
                    guard.routines.note_settled(&identity.task_id, "failed");
                    guard.push_transcript(TranscriptEntry {
                        conversation_id: identity.conversation_id.clone(),
                        kind: "system".into(),
                        text: error.to_string(),
                        task_id: identity.task_id.clone(),
                        at: now_millis(),
                        ..Default::default()
                    });
                    guard.clone()
                };
                if let Some(store) = store {
                    store::persist(store, &snapshot);
                }
                events.publish(
                    &identity.conversation_id,
                    &identity.task_id,
                    "error",
                    json!({"message": error.to_string(), "revision": snapshot.revision}),
                );
                notify::spawn_delivery(
                    snapshot.notifications.clone(),
                    snapshot.devices.clone(),
                    Notification::new(
                        "failed",
                        "Marble hit a problem",
                        error.to_string(),
                        &identity.conversation_id,
                        &identity.task_id,
                    ),
                );
            } else {
                events.publish("", "", "error", json!({ "message": error.to_string() }));
            }
        }
    }
}

/// Carry remembered facts into the turn as context, so memory is used rather
/// than merely stored.
#[cfg(test)]
fn turn_input(state: &Arc<Mutex<ProductState>>, text: &str) -> TurnInput {
    crate::normalize::engine_input(&lock(state), None, "marble", text)
}

/// Convenience constructor for one inbound message.
impl TurnRequest {
    pub fn new(conversation_id: Option<String>, text: impl Into<String>) -> Self {
        Self {
            conversation_id,
            text: text.into(),
            request_id: crate::next_request_id(),
            cancel: Arc::new(AtomicBool::new(false)),
            routine_id: None,
            routine_counted: false,
            routine_override: false,
        }
    }

    /// A routine firing whose run was already counted while admission held it.
    /// Resuming a paused queue hands it back in the routine lane, and admission
    /// attaches its task id so settlement still reaches the routine.
    pub fn counted_routine(mut self, routine_id: impl Into<String>) -> Self {
        self.routine_id = Some(routine_id.into());
        self.routine_counted = true;
        self
    }

    /// A turn a person typed outranks one a schedule fired.
    pub fn lane(&self) -> TurnLane {
        if self.routine_id.is_some() {
            TurnLane::Routine
        } else {
            TurnLane::Human
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane_with(load: usize) -> Option<LaneRuntime> {
        let (sender, _receiver) = TurnInbox::channel();
        Some(LaneRuntime {
            sender,
            in_flight: Arc::new(AtomicUsize::new(load)),
        })
    }

    #[test]
    fn cancellation_removes_commands_from_a_lane_queue() {
        let (sender, _) = TurnInbox::channel();
        let (lane, receiver) = TurnInbox::channel();
        let in_flight = Arc::new(AtomicUsize::new(1));
        sender
            .lanes
            .lock()
            .unwrap()
            .push((lane.inbox.clone(), in_flight.clone()));
        let request = TurnRequest::new(None, "never run");
        let id = format!("queued-{}", request.request_id);
        lane.send(TurnCommand {
            request,
            accepted: mpsc::channel().0,
        })
        .unwrap();
        sender.remove_queued(&id);
        assert!(receiver.try_recv().is_none());
        assert_eq!(in_flight.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_bound_conversation_returns_to_its_lane_and_new_work_spreads() {
        let state = Mutex::new(ProductState::default());
        {
            let mut guard = state.lock().unwrap();
            guard.conversation_mut("c-2").lane = 2;
            guard.bump();
        }
        let runtimes = vec![lane_with(0), lane_with(0), lane_with(0)];
        let mut next = 0;
        assert_eq!(choose_lane(&state, Some("c-2"), &runtimes, &mut next), 2);

        // New chats take the lightest lane, and ties rotate so two fresh chats
        // do not land on one engine.
        let busy = vec![lane_with(1), lane_with(0), lane_with(0)];
        let mut next = 0;
        assert_eq!(choose_lane(&state, None, &busy, &mut next), 1);
        assert_eq!(choose_lane(&state, None, &busy, &mut next), 2);

        // An unknown id is not allowed to strand a message: it still lands on a
        // real lane instead of failing the admission.
        let mut next = 0;
        assert_eq!(choose_lane(&state, Some("unknown"), &busy, &mut next), 1);
    }

    #[test]
    fn a_lane_whose_thread_stopped_hands_the_command_back() {
        let (sender, receiver) = TurnInbox::channel();
        // The lane's thread is gone, so its queue can never reach an engine.
        drop(receiver);
        let mut runtimes = vec![Some(LaneRuntime {
            sender,
            in_flight: Arc::new(AtomicUsize::new(0)),
        })];
        let command = TurnCommand {
            request: TurnRequest::new(None, "still here"),
            accepted: mpsc::channel().0,
        };
        let returned = hand_over(&mut runtimes, 0, command)
            .expect("a dead lane returns the command instead of swallowing it");
        assert_eq!(returned.request.text, "still here");
        assert!(
            runtimes[0].is_none(),
            "the dead lane is taken out of service"
        );
    }

    #[test]
    fn a_firing_for_a_removed_or_disabled_routine_is_not_wanted() {
        let state = Arc::new(Mutex::new(ProductState::default()));
        let routine_id = {
            let mut guard = lock(&state);
            guard
                .routines
                .add("Sweep", "sweep the notes", "every:1h", None)
                .expect("a routine")
                .id
        };
        assert!(firing_still_wanted(&state, &routine_id, false));

        lock(&state).routines.set_enabled(&routine_id, false);
        assert!(
            !firing_still_wanted(&state, &routine_id, false),
            "a disabled routine's waiting firing is skipped"
        );
        assert!(
            firing_still_wanted(&state, &routine_id, true),
            "but a firing a person ran by hand overrides the pause"
        );

        lock(&state).routines.remove(&routine_id);
        assert!(
            !firing_still_wanted(&state, &routine_id, true),
            "a removed routine's waiting firing is skipped, override or not"
        );
    }

    #[test]
    fn typed_messages_drain_before_routine_firings() {
        let (sender, inbox) = TurnInbox::channel();
        let routine = |text: &str| {
            let mut request = TurnRequest::new(None, text);
            request.routine_id = Some("routine-1".into());
            TurnCommand {
                request,
                accepted: mpsc::channel().0,
            }
        };
        assert_eq!(sender.send(routine("sweep")).expect("queued"), 1);
        assert_eq!(sender.send(routine("tidy")).expect("queued"), 2);
        let typed = TurnCommand {
            request: TurnRequest::new(None, "typed now"),
            accepted: mpsc::channel().0,
        };
        assert_eq!(
            sender.send(typed).expect("queued"),
            1,
            "a typed message counts only typed messages in front of it"
        );
        assert_eq!(sender.waiting(), 3);
        assert_eq!(sender.waiting_in(TurnLane::Routine), 2);

        let first = inbox.try_recv().expect("the typed message is next");
        assert_eq!(first.request.text, "typed now");
        assert_eq!(first.request.lane(), TurnLane::Human);
        let second = inbox.try_recv().expect("then the first firing");
        assert_eq!(second.request.text, "sweep");
        assert_eq!(second.request.lane(), TurnLane::Routine);
        assert_eq!(
            inbox
                .try_recv()
                .expect("then the second firing")
                .request
                .text,
            "tidy"
        );
        assert!(inbox.try_recv().is_none());

        drop(sender);
        assert!(
            inbox.recv().is_none(),
            "the worker stops when the last surface is gone"
        );
    }

    #[test]
    fn the_cancel_table_covers_every_running_turn() {
        let cancels = CancelRegistry::default();
        let flag = Arc::new(AtomicBool::new(false));
        cancels.register("task-1", Arc::clone(&flag));
        assert_eq!(cancels.running().len(), 1);
        assert!(cancels.request("task-1"));
        assert!(flag.load(Ordering::Relaxed));
        assert!(!cancels.request("task-2"));
        cancels.forget("task-1");
        assert!(!cancels.request("task-1"));
    }

    #[test]
    fn titles_are_bounded_and_trimmed() {
        assert_eq!(truncate("  hello  ", 10), "hello");
        assert_eq!(truncate("abcdefghij", 4), "abcd…");
    }

    #[test]
    fn request_ids_are_unique_per_message() {
        let first = TurnRequest::new(None, "a");
        let second = TurnRequest::new(None, "a");
        assert_ne!(first.request_id, second.request_id);
        assert!(first.routine_id.is_none());
    }

    #[test]
    fn remembered_facts_are_carried_into_the_turn() {
        let mut product = ProductState::default();
        let plain = turn_input(&Arc::new(Mutex::new(product.clone())), "hello");
        assert!(plain.context.is_empty());
        assert!(plain.text.contains("Current date and time:"));
        product
            .memory
            .remember("my name is Azhar", "role", "c1", true, "")
            .unwrap();
        let with_memory = turn_input(&Arc::new(Mutex::new(product)), "hello");
        assert!(with_memory.context.is_empty());
        assert!(with_memory.text.ends_with("[User message]\nhello"));
        assert!(with_memory.text.contains("my name is Azhar"));
    }
}

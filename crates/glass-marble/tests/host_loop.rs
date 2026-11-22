use glass_marble::{
    AcceptedTurn, Approval, ApprovalInbox, CancelRegistry, ControlProfile, Conversation, EventBus,
    HostPolicy, InboundMessage, LaneFactory, LiveStore, MarbleError, MarbleReply, MarbleTurn,
    PendingApproval, ProductState, StateStore, SteerRequest, TaskRecord, ToolRouter, TurnCommand,
    TurnOptions, TurnPool, TurnRequest, run_turn, spawn_turn_pool, to_turn_input,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const FAKE_RUNTIME_SOURCE: &str = include_str!("fixtures/fake_glass_core_runtime.py");

fn write_fake_runtime(root: &Path) -> PathBuf {
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, FAKE_RUNTIME_SOURCE).unwrap();
    script
}

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn workspace(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "glass-marble-{}-{}-{name}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("main")
            .replace(':', "-")
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn spawn_conversation(
    root: &Path,
    script: &Path,
    storage_dir: &Path,
    host: HostPolicy,
    approvals: ApprovalInbox,
) -> Conversation {
    let mut python = Command::new("python3");
    python.arg(script).arg("runtime");
    Conversation::spawn_with_router(
        python,
        root,
        storage_dir,
        host,
        approvals,
        ToolRouter::new(root).unwrap(),
        EventBus::default(),
    )
    .unwrap()
}

fn turn(text: &str, conversation_id: Option<String>) -> MarbleTurn {
    MarbleTurn {
        conversation_id,
        request_id: glass_marble::next_request_id(),
        input: to_turn_input(&InboundMessage::new(text)),
    }
}

fn run_one(
    root: &Path,
    host: HostPolicy,
    text: &str,
    conversation_id: Option<String>,
) -> MarbleReply {
    let script = write_fake_runtime(root);
    let storage_dir = root.join("sessions");
    let mut conversation =
        spawn_conversation(root, &script, &storage_dir, host, ApprovalInbox::default());
    let reply = run_turn(
        &turn(text, conversation_id),
        &mut conversation,
        TurnOptions::default(),
    )
    .unwrap();
    let _ = conversation.shutdown();
    reply
}

#[test]
fn pre_tool_text_is_not_a_visible_delta_and_is_the_empty_answer_fallback() {
    if !python3_available() {
        return;
    }
    for answer in ["The answer.", ""] {
        let root = workspace(if answer.is_empty() {
            "step-empty"
        } else {
            "step-final"
        });
        let script = write_fake_runtime(&root);
        let events = EventBus::default();
        let mut command = Command::new("python3");
        command
            .arg(script)
            .arg("runtime")
            .arg("--pre-tool-text")
            .arg("Research info.")
            .arg("--text-chunks")
            .arg(serde_json::json!([answer]).to_string());
        let mut conversation = Conversation::spawn_with_router(
            command,
            &root,
            &root.join("sessions"),
            HostPolicy::approving(),
            ApprovalInbox::default(),
            ToolRouter::new(&root).unwrap(),
            events.clone(),
        )
        .unwrap();
        let reply = run_turn(
            &turn("research", None),
            &mut conversation,
            TurnOptions::default(),
        )
        .unwrap();
        assert_eq!(reply.reasoning, "Research info.");
        assert_eq!(
            reply.text,
            if answer.is_empty() {
                "Research info."
            } else {
                answer
            }
        );
        let frames = events.since(0);
        let visible: String = frames
            .iter()
            .filter(|e| e.kind == "text")
            .map(|e| e.data["text"].as_str().unwrap())
            .collect();
        assert_eq!(visible, reply.text);
        conversation.shutdown().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}

/// How one lane's fake engine is built for a test.
#[test]
fn default_tool_rows_survive_success_and_refusal_reload() {
    if !python3_available() {
        return;
    }
    for (host, status) in [
        (HostPolicy::approving(), "ok"),
        (HostPolicy::denying(), "failed"),
    ] {
        let root = workspace(&format!("tool-row-{status}"));
        let script = write_fake_runtime(&root);
        let product = Arc::new(Mutex::new(ProductState::default()));
        let mut conversation = spawn_conversation(
            &root,
            &script,
            &root.join("sessions"),
            host,
            ApprovalInbox::default(),
        )
        .with_product(product.clone());
        let reply = run_turn(
            &turn("inspect", None),
            &mut conversation,
            TurnOptions::default(),
        )
        .unwrap();
        let store = StateStore::in_data_dir(&root);
        store.save(&product.lock().unwrap()).unwrap();
        assert!(
            store
                .load()
                .unwrap()
                .transcript
                .iter()
                .any(|entry| entry.task_id == reply.task_id
                    && entry.kind == "tool"
                    && entry.text == format!("glass.workspace.list · {status}"))
        );
        conversation.shutdown().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}

/// How one lane's fake engine is built for a test.
#[derive(Clone)]
struct LaneSpec {
    delay_ms: u64,
    tool: &'static str,
    path: Option<&'static str>,
    content: Option<&'static str>,
    /// Ask the person for each governed call instead of approving it.
    prompt: bool,
    /// Die as soon as a turn asks the engine to run.
    die_on_run: bool,
    /// Fail to build this engine at all.
    fail: bool,
}

impl Default for LaneSpec {
    fn default() -> Self {
        Self {
            delay_ms: 0,
            tool: "glass.workspace.list",
            path: None,
            content: None,
            prompt: false,
            die_on_run: false,
            fail: false,
        }
    }
}

/// One engine per lane, each with its own session store and its own id
/// namespace. The names are how a test tells which lane a turn reached.
fn lane_factory(
    root: &Path,
    script: &Path,
    specs: Vec<LaneSpec>,
    approvals: ApprovalInbox,
    started: Arc<Mutex<Vec<u8>>>,
) -> LaneFactory {
    let root = root.to_path_buf();
    let script = script.to_path_buf();
    let specs = Arc::new(specs);
    Arc::new(move |lane: u8| {
        started
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(lane);
        let spec = specs.get(usize::from(lane)).cloned().unwrap_or_default();
        if spec.fail {
            return Err(MarbleError::Protocol(
                "this engine cannot start in the test".into(),
            ));
        }
        let mut python = Command::new("python3");
        python
            .arg(&script)
            .arg("runtime")
            .arg("--delay-ms")
            .arg(spec.delay_ms.to_string())
            .arg("--id-prefix")
            .arg(format!("lane-{lane}-"))
            .arg("--tool")
            .arg(spec.tool);
        if let Some(path) = spec.path {
            python.arg("--path").arg(path);
        }
        if let Some(content) = spec.content {
            python.arg("--content").arg(content);
        }
        if spec.die_on_run {
            python.arg("--die-on-run");
        }
        let host = if spec.prompt {
            HostPolicy::asking()
        } else {
            HostPolicy::approving()
        };
        Conversation::spawn_with_router(
            python,
            &root,
            &root.join("sessions").join(format!("lane-{lane}")),
            host,
            approvals.clone(),
            ToolRouter::new(&root)?,
            EventBus::default(),
        )
        .map(|conversation| conversation.with_approval_timeout(Duration::from_secs(30)))
    })
}

/// A two-lane pool wired the way `main` wires the served product.
fn lanes(
    root: &Path,
    script: &Path,
    specs: Vec<LaneSpec>,
    approvals: ApprovalInbox,
    state: &Arc<Mutex<ProductState>>,
    started: &Arc<Mutex<Vec<u8>>>,
    cancels: CancelRegistry,
) -> TurnPool {
    spawn_turn_pool(
        2,
        lane_factory(root, script, specs, approvals, Arc::clone(started)),
        Arc::clone(state),
        None,
        LiveStore::in_data_dir(root),
        EventBus::default(),
        Duration::from_secs(30),
        cancels,
    )
    .expect("two lanes")
}

fn started_lanes(started: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    started
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn submit(pool: &TurnPool, text: &str, conversation_id: Option<String>) -> AcceptedTurn {
    let (accepted, answer) = mpsc::channel();
    pool.sender
        .send(TurnCommand {
            request: TurnRequest::new(conversation_id, text),
            accepted,
        })
        .expect("the pool takes the command");
    answer
        .recv_timeout(Duration::from_secs(30))
        .expect("the engine admits the turn in time")
        .expect("the turn is accepted")
}

fn task_status(state: &Mutex<ProductState>, task_id: &str) -> String {
    state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .map(|task| task.status.clone())
        .unwrap_or_default()
}

fn running_tasks(state: &Mutex<ProductState>) -> usize {
    state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .tasks
        .iter()
        .filter(|task| task.status == "running")
        .count()
}

fn wait_for_settled_as(state: &Mutex<ProductState>, task_id: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let status = task_status(state, task_id);
        if matches!(
            status.as_str(),
            "completed" | "cancelled" | "failed" | "interrupted"
        ) {
            return status;
        }
        assert!(Instant::now() < deadline, "turn {task_id} never settled");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_settled(state: &Mutex<ProductState>, task_id: &str) {
    let status = wait_for_settled_as(state, task_id);
    assert_eq!(status, "completed", "turn {task_id} settled as {status}");
}

fn wait_until(what: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Wait for the approval card of one path and return it.
fn wait_for_approval(approvals: &ApprovalInbox, path: &str) -> PendingApproval {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(pending) = approvals.pending().into_iter().find(|pending| {
            pending
                .arguments
                .get("path")
                .and_then(|value| value.as_str())
                == Some(path)
        }) {
            return pending;
        }
        assert!(
            Instant::now() < deadline,
            "no approval for {path} reached the surface"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn lane_of(state: &Mutex<ProductState>, conversation_id: &str) -> u8 {
    state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .conversations
        .iter()
        .find(|record| record.id == conversation_id)
        .map(|record| record.lane)
        .unwrap_or(u8::MAX)
}

/// Two chats submitted back to back must be worked on by two engines at once.
///
/// The fake runtime holds each turn for a moment after admitting it, so both
/// durable tasks are visibly running together: a single loop would show one
/// running task and the other still waiting.
#[test]
fn two_chats_run_at_the_same_time_on_separate_lanes() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-concurrency");
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let pool = spawn_turn_pool(
        2,
        lane_factory(
            &root,
            &script,
            vec![
                LaneSpec {
                    delay_ms: 1_500,
                    ..LaneSpec::default()
                };
                2
            ],
            ApprovalInbox::default(),
            Arc::clone(&started),
        ),
        Arc::clone(&state),
        None,
        LiveStore::in_data_dir(&root),
        EventBus::default(),
        Duration::from_secs(30),
        CancelRegistry::default(),
    )
    .expect("two lanes");

    let first = submit(&pool, "first chat", None);
    let second = submit(&pool, "second chat", None);
    assert_ne!(
        first.conversation_id, second.conversation_id,
        "each engine keeps its own conversation"
    );

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut together = false;
    while Instant::now() < deadline {
        if running_tasks(&state) == 2 {
            together = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        together,
        "two chats should be running on two engines at the same time"
    );

    wait_for_settled(&state, &first.task_id);
    wait_for_settled(&state, &second.task_id);
    assert_eq!(
        started_lanes(&started),
        vec![0, 1],
        "each chat really got an engine of its own"
    );
    let first_lane = lane_of(&state, &first.conversation_id);
    let second_lane = lane_of(&state, &second.conversation_id);
    assert_ne!(
        first_lane, second_lane,
        "each chat is bound to an engine of its own"
    );
    let mut lanes = vec![first_lane, second_lane];
    lanes.sort_unstable();
    assert_eq!(lanes, vec![0, 1], "both lanes are in use");

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn everyone_waiting_for_its_first_conversation_stays_on_one_engine() {
    if !python3_available() {
        return;
    }
    let root = workspace("everyone-first-lane");
    let script = write_fake_runtime(&root);
    let state = Arc::new(Mutex::new(ProductState::default()));
    let (release, waiting) = mpsc::channel();
    let gate = Arc::new(Mutex::new(Some(waiting)));
    let begun = Arc::new(AtomicBool::new(false));
    let factory: LaneFactory = {
        let root = root.clone();
        let state = state.clone();
        let begun = begun.clone();
        Arc::new(move |lane| {
            if lane == 1 {
                begun.store(true, Ordering::SeqCst);
                if let Some(waiting) = gate.lock().unwrap().take() {
                    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
                }
            }
            let mut command = Command::new("python3");
            command
                .arg(&script)
                .arg("runtime")
                .arg("--strict-conversations")
                .arg("--id-prefix")
                .arg(format!("lane-{lane}-"));
            Conversation::spawn_with_router(
                command,
                &root,
                &root.join(format!("sessions-{lane}")),
                HostPolicy::approving(),
                ApprovalInbox::default(),
                ToolRouter::new(&root)?,
                EventBus::default(),
            )
            .map(|conversation| conversation.with_product(state.clone()))
        })
    };
    let pool = spawn_turn_pool(
        2,
        factory,
        state.clone(),
        None,
        LiveStore::in_data_dir(&root),
        EventBus::default(),
        Duration::from_secs(30),
        CancelRegistry::default(),
    )
    .unwrap();
    let send = |text: &str| {
        let request = TurnRequest::new(None, text);
        state
            .lock()
            .unwrap()
            .everyone_pending
            .insert(format!("queued-{}", request.request_id), "marble".into());
        let (accepted, answer) = mpsc::channel();
        pool.sender.send(TurnCommand { request, accepted }).unwrap();
        answer
    };
    let first = send("first Everyone turn");
    wait_until("Everyone engine startup", || begun.load(Ordering::SeqCst));
    let second = send("second Everyone turn");
    release.send(()).unwrap();
    let first = first
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let second = second
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    assert_eq!(first.conversation_id, second.conversation_id);
    wait_for_settled(&state, &first.task_id);
    wait_for_settled(&state, &second.task_id);
    assert_eq!(state.lock().unwrap().tasks.len(), 2);
    drop(pool);
}

#[test]
fn everyone_rebinds_to_a_surviving_chats_engine_after_deletion() {
    if !python3_available() {
        return;
    }
    let root = workspace("everyone-rebind-lane");
    let script = write_fake_runtime(&root);
    let state = Arc::new(Mutex::new(ProductState::default()));
    let pool = lanes(
        &root,
        &script,
        vec![LaneSpec::default(); 2],
        ApprovalInbox::default(),
        &state,
        &Arc::default(),
        CancelRegistry::default(),
    );
    let everyone = |text: &str, conversation_id: Option<String>| {
        let request = TurnRequest::new(conversation_id, text);
        state
            .lock()
            .unwrap()
            .everyone_pending
            .insert(format!("queued-{}", request.request_id), "marble".into());
        let (accepted, answer) = mpsc::channel();
        pool.sender.send(TurnCommand { request, accepted }).unwrap();
        let accepted = answer
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        wait_for_settled(&state, &accepted.task_id);
        accepted
    };
    let original = everyone("Everyone original", None);
    let survivor = submit(&pool, "private history that must survive", None);
    wait_for_settled(&state, &survivor.task_id);
    assert_ne!(
        lane_of(&state, &original.conversation_id),
        lane_of(&state, &survivor.conversation_id)
    );
    {
        let mut state = state.lock().unwrap();
        state
            .conversations
            .retain(|c| c.id != original.conversation_id);
        state
            .tasks
            .retain(|t| t.conversation_id != original.conversation_id);
        state.transcript.retain(|e| {
            e.conversation_id != original.conversation_id && e.task_id != original.task_id
        });
    }
    let resumed = everyone(
        "Everyone resumes survivor",
        Some(survivor.conversation_id.clone()),
    );
    assert_eq!(resumed.conversation_id, survivor.conversation_id);
    let followup = everyone("Everyone deferred followup", None);
    assert_eq!(followup.conversation_id, survivor.conversation_id);
    drop(pool);
}

/// The lane a conversation was admitted on is durable, so a restarted process
/// rebuilds that lane instead of opening the chat somewhere else.
#[test]
fn a_conversation_returns_to_its_lane_after_a_restart() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-restart");
    let script = write_fake_runtime(&root);
    let store = Arc::new(StateStore::in_data_dir(&root));
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(store.load().expect("an empty document")));
    let pool = spawn_turn_pool(
        2,
        lane_factory(
            &root,
            &script,
            vec![LaneSpec::default(); 2],
            ApprovalInbox::default(),
            Arc::clone(&started),
        ),
        Arc::clone(&state),
        Some(Arc::clone(&store)),
        LiveStore::in_data_dir(&root),
        EventBus::default(),
        Duration::from_secs(30),
        CancelRegistry::default(),
    )
    .expect("two lanes");

    // The dispatcher rotates, so a fresh chat lands on a lane above zero.
    // Only such a lane proves that a restart rebuilt it instead of reusing the
    // engine lane zero starts on its own.
    let first = submit(&pool, "first chat", None);
    wait_for_settled(&state, &first.task_id);
    let lane = lane_of(&state, &first.conversation_id);
    assert_ne!(lane, 0, "a chat can be admitted on a lane above zero");

    // A restart leaves only the document on disk behind.
    drop(pool);
    std::thread::sleep(Duration::from_millis(200));
    let restored = store.load().expect("the document a restart would read");
    let restored = Arc::new(Mutex::new(restored));
    assert_eq!(
        lane_of(&restored, &first.conversation_id),
        lane,
        "the lane is part of the durable conversation"
    );

    let started = Arc::new(Mutex::new(Vec::new()));
    let pool = spawn_turn_pool(
        2,
        lane_factory(
            &root,
            &script,
            vec![LaneSpec::default(); 2],
            ApprovalInbox::default(),
            Arc::clone(&started),
        ),
        Arc::clone(&restored),
        Some(Arc::clone(&store)),
        LiveStore::in_data_dir(&root),
        EventBus::default(),
        Duration::from_secs(30),
        CancelRegistry::default(),
    )
    .expect("two lanes");
    assert_eq!(
        started_lanes(&started),
        vec![0],
        "only lane zero starts before it is needed"
    );

    let again = submit(&pool, "back again", Some(first.conversation_id.clone()));
    wait_for_settled(&restored, &again.task_id);
    assert_eq!(again.conversation_id, first.conversation_id);
    assert_eq!(
        started_lanes(&started),
        vec![0, lane],
        "the restarted pool rebuilt the lane this conversation owns"
    );
    assert_eq!(
        lane_of(&restored, &first.conversation_id),
        lane,
        "the conversation stays on its own lane"
    );

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

/// A lane whose engine cannot start fails only the chat it was chosen for.
#[test]
fn a_lane_that_cannot_start_does_not_disturb_another_chat() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-fails");
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let specs = vec![
        LaneSpec::default(),
        LaneSpec {
            fail: true,
            ..LaneSpec::default()
        },
    ];
    let pool = lanes(
        &root,
        &script,
        specs,
        ApprovalInbox::default(),
        &state,
        &started,
        CancelRegistry::default(),
    );

    // The dispatcher rotates, so the first fresh chat needs the broken lane.
    let (accepted, answer) = mpsc::channel();
    pool.sender
        .send(TurnCommand {
            request: TurnRequest::new(None, "doomed chat"),
            accepted,
        })
        .expect("the pool takes the command");
    let refused = answer
        .recv_timeout(Duration::from_secs(20))
        .expect("the dispatcher answers every command")
        .expect_err("the broken lane must refuse the turn");
    assert!(refused.to_string().contains("cannot start"), "{refused}");

    let healthy = submit(&pool, "healthy chat", None);
    wait_for_settled(&state, &healthy.task_id);
    assert_eq!(
        lane_of(&state, &healthy.conversation_id),
        0,
        "the healthy chat still has its own engine"
    );
    assert!(
        started_lanes(&started).contains(&1),
        "the failing lane was tried before it refused"
    );
    {
        let state = state.lock().unwrap();
        let failed = state
            .tasks
            .iter()
            .find(|t| t.prompt == "doomed chat")
            .unwrap();
        assert_eq!(failed.status, "failed");
        assert!(
            state
                .conversations
                .iter()
                .any(|c| c.id == failed.conversation_id)
        );
        let entry = state
            .transcript
            .iter()
            .find(|e| e.task_id == failed.id && e.kind == "failure")
            .unwrap();
        assert_eq!(entry.text, glass_marble::failure::MESSAGE);
        assert!(entry.failure.as_ref().unwrap().retryable);
    }

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

/// Launching an engine means starting a subprocess and greeting it, which takes
/// long enough that doing it in the dispatcher would hold up every other chat.
///
/// The second lane's engine is held inside its factory, so the first chat is
/// still starting when the second chat is submitted. That chat lands on lane
/// zero and must be admitted while the first lane is still blocked.
#[test]
fn a_slow_lane_start_does_not_hold_up_another_chat() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-start-blocking");
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let gate = Arc::new(Mutex::new(Some(release_rx)));
    let begun = Arc::new(AtomicBool::new(false));
    let inner = lane_factory(
        &root,
        &script,
        vec![LaneSpec::default(); 2],
        ApprovalInbox::default(),
        Arc::clone(&started),
    );
    let factory: LaneFactory = {
        let gate = Arc::clone(&gate);
        let begun = Arc::clone(&begun);
        Arc::new(move |lane: u8| {
            if lane == 1 {
                begun.store(true, Ordering::SeqCst);
                let waiting = gate
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take();
                if let Some(waiting) = waiting {
                    let _ = waiting.recv_timeout(Duration::from_secs(60));
                }
            }
            inner(lane)
        })
    };
    let pool = spawn_turn_pool(
        2,
        factory,
        Arc::clone(&state),
        None,
        LiveStore::in_data_dir(&root),
        EventBus::default(),
        Duration::from_secs(30),
        CancelRegistry::default(),
    )
    .expect("two lanes");

    // The dispatcher rotates, so this chat needs lane one, whose engine is held.
    let (held_accepted, held_answer) = mpsc::channel();
    pool.sender
        .send(TurnCommand {
            request: TurnRequest::new(None, "held chat"),
            accepted: held_accepted,
        })
        .expect("the pool takes the command");
    wait_until("the second lane to start", || begun.load(Ordering::SeqCst));

    // A chat that lands on lane zero is admitted while lane one still starts.
    let (free_accepted, free_answer) = mpsc::channel();
    pool.sender
        .send(TurnCommand {
            request: TurnRequest::new(None, "free chat"),
            accepted: free_accepted,
        })
        .expect("the pool takes the command");
    let free = free_answer
        .recv_timeout(Duration::from_secs(20))
        .expect("a lane starting must not hold up another chat")
        .expect("the free lane admits the chat");
    wait_for_settled(&state, &free.task_id);
    assert_eq!(
        lane_of(&state, &free.conversation_id),
        0,
        "the free chat ran on the idle lane"
    );

    // Let the held engine start; the first chat then runs on its own lane.
    release_tx.send(()).expect("the gate opens");
    let held = held_answer
        .recv_timeout(Duration::from_secs(60))
        .expect("the held lane answers once its engine is up")
        .expect("the held chat is accepted");
    wait_for_settled(&state, &held.task_id);
    assert_eq!(lane_of(&state, &held.conversation_id), 1);

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

/// Work handed to a lane whose engine never started must not be left looking
/// queued in the durable document.
///
/// The surface answers `202 queued` as soon as admission takes longer than its
/// short wait, so the worker cannot rely on the caller still listening. The
/// durable row is what a person sees, and it has to say the message failed
/// instead of promising a turn that can never run.
#[test]
fn work_on_a_lane_that_never_started_fails_instead_of_staying_queued() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-never-started");
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let pool = lanes(
        &root,
        &script,
        vec![
            LaneSpec::default(),
            LaneSpec {
                fail: true,
                ..LaneSpec::default()
            },
        ],
        ApprovalInbox::default(),
        &state,
        &started,
        CancelRegistry::default(),
    );

    // The surface records the queued row before it hands the message over.
    let request = TurnRequest::new(None, "doomed chat");
    let queued_id = format!("queued-{}", request.request_id);
    {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        guard.upsert_task(TaskRecord {
            marble_id: String::new(),
            id: queued_id.clone(),
            conversation_id: String::new(),
            status: "queued".into(),
            owner: "agent".into(),
            prompt: "doomed chat".into(),
            at: 0,
            routine_id: String::new(),
        });
        guard.bump();
    }
    let (accepted, answer) = mpsc::channel();
    // The surface already gave up waiting and answered `202 queued`.
    drop(answer);
    pool.sender
        .send(TurnCommand { request, accepted })
        .expect("the pool takes the command");

    wait_until("the doomed message to fail", || {
        task_status(&state, &queued_id) == "failed"
    });
    assert_eq!(
        task_status(&state, &queued_id),
        "failed",
        "a message that can never run must not stay queued"
    );

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

/// An engine that dies mid-turn fails only its own chat; the other engine keeps
/// working, including on the turn after the death.
#[test]
fn a_lane_that_dies_mid_turn_does_not_disturb_another_chat() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-dies");
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let specs = vec![
        LaneSpec::default(),
        LaneSpec {
            die_on_run: true,
            ..LaneSpec::default()
        },
    ];
    let pool = lanes(
        &root,
        &script,
        specs,
        ApprovalInbox::default(),
        &state,
        &started,
        CancelRegistry::default(),
    );

    let doomed = submit(&pool, "doomed chat", None);
    let healthy = submit(&pool, "healthy chat", None);
    assert_eq!(
        wait_for_settled_as(&state, &doomed.task_id),
        "failed",
        "the dying engine fails its own turn"
    );
    wait_for_settled(&state, &healthy.task_id);
    assert_eq!(lane_of(&state, &healthy.conversation_id), 0);

    // The surviving chat keeps working after the other engine died.
    let again = submit(
        &pool,
        "healthy chat again",
        Some(healthy.conversation_id.clone()),
    );
    wait_for_settled(&state, &again.task_id);
    assert_eq!(again.conversation_id, healthy.conversation_id);

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

/// A stop ends one chat's turn and leaves the other chat's turn running to a
/// normal completion.
#[test]
fn a_stop_in_one_chat_does_not_disturb_another_chat() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-stop");
    let fifo = root.join("slow.txt");
    let made = Command::new("python3")
        .arg("-c")
        .arg("import os,sys; os.mkfifo(sys.argv[1])")
        .arg(&fifo)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !made {
        let _ = std::fs::remove_dir_all(&root);
        return;
    }
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let cancels = CancelRegistry::default();
    let specs = vec![
        LaneSpec {
            tool: "glass.workspace.read",
            path: Some("slow.txt"),
            ..LaneSpec::default()
        },
        LaneSpec {
            delay_ms: 500,
            ..LaneSpec::default()
        },
    ];
    let pool = lanes(
        &root,
        &script,
        specs,
        ApprovalInbox::default(),
        &state,
        &started,
        cancels.clone(),
    );

    let healthy = submit(&pool, "healthy chat", None);
    let blocked = submit(&pool, "blocked chat", None);
    wait_until("both chats to run", || running_tasks(&state) == 2);
    assert!(
        cancels.request(&blocked.task_id),
        "the running turn is cancellable"
    );
    assert_eq!(
        wait_for_settled_as(&state, &blocked.task_id),
        "cancelled",
        "a stop settles only its own turn"
    );
    wait_for_settled(&state, &healthy.task_id);
    assert_eq!(lane_of(&state, &healthy.conversation_id), 1);

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

/// A correction is delivered to the engine that owns the chat, and the other
/// chat's engine never sees it.
#[test]
fn a_steer_reaches_only_the_chat_it_names() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-steer");
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let specs = vec![
        LaneSpec {
            delay_ms: 1_500,
            ..LaneSpec::default()
        };
        2
    ];
    let pool = lanes(
        &root,
        &script,
        specs,
        ApprovalInbox::default(),
        &state,
        &started,
        CancelRegistry::default(),
    );

    let first = submit(&pool, "first chat", None);
    let second = submit(&pool, "second chat", None);
    wait_until("both chats to run", || running_tasks(&state) == 2);

    let target_lane = lane_of(&state, &first.conversation_id);
    let other_lane = lane_of(&state, &second.conversation_id);
    assert_ne!(target_lane, other_lane, "the chats sit on different lanes");
    assert!(
        pool.steer.send(
            target_lane,
            SteerRequest {
                conversation_id: first.conversation_id.clone(),
                task_id: String::new(),
                text: "use the other file".into(),
            }
        ),
        "the lane serving the chat takes the correction"
    );

    wait_for_settled(&state, &first.task_id);
    wait_for_settled(&state, &second.task_id);

    let steer_file = |lane: u8| {
        root.join("sessions")
            .join(format!("lane-{lane}"))
            .join("fixture-steer.json")
    };
    wait_until("the correction to reach its engine", || {
        steer_file(target_lane).exists()
    });
    let recorded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(steer_file(target_lane)).unwrap()).unwrap();
    assert_eq!(
        recorded["text"],
        serde_json::json!("use the other file"),
        "the owning engine received the correction"
    );
    assert!(
        !steer_file(other_lane).exists(),
        "the other chat's engine never saw the correction"
    );

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

/// A write that would replace another chat's file is flagged on the approval
/// card and refused at execution, so the other chat's content survives.
#[test]
fn a_second_chat_is_warned_before_replacing_another_chats_write() {
    if !python3_available() {
        return;
    }
    let root = workspace("lane-conflict");
    let script = write_fake_runtime(&root);
    let started = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(ProductState::default()));
    let approvals = ApprovalInbox::default();
    let specs = vec![
        LaneSpec {
            prompt: true,
            tool: "glass.workspace.write",
            path: Some("notes.md"),
            content: Some("from lane zero"),
            ..LaneSpec::default()
        },
        LaneSpec {
            prompt: true,
            tool: "glass.workspace.write",
            path: Some("notes.md"),
            content: Some("from lane one"),
            ..LaneSpec::default()
        },
    ];
    let pool = lanes(
        &root,
        &script,
        specs,
        approvals.clone(),
        &state,
        &started,
        CancelRegistry::default(),
    );

    let first = submit(&pool, "first chat", None);
    let first_card = wait_for_approval(&approvals, "notes.md");
    assert!(
        first_card.conflict.is_none(),
        "the first write has nothing to conflict with"
    );
    assert!(approvals.resolve(&first_card.request_id, Approval::Approve));
    wait_for_settled(&state, &first.task_id);
    assert_eq!(
        std::fs::read_to_string(root.join("notes.md")).unwrap(),
        "from lane one"
    );

    let second = submit(&pool, "second chat", None);
    let second_card = wait_for_approval(&approvals, "notes.md");
    let conflict = second_card
        .conflict
        .expect("the second card names the other chat's write");
    assert!(
        conflict.contains("Another chat") && conflict.contains("notes.md"),
        "{conflict}"
    );
    assert!(approvals.resolve(&second_card.request_id, Approval::Approve));
    wait_for_settled(&state, &second.task_id);

    // Approving did not let the write lose the first chat's change.
    assert_eq!(
        std::fs::read_to_string(root.join("notes.md")).unwrap(),
        "from lane one",
        "the first chat's write is still on disk"
    );
    let outcome: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("sessions/lane-0/fixture-tool-outcome.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(outcome["isError"], serde_json::json!(true));
    let text = outcome["text"].as_str().unwrap();
    assert!(text.contains("re-read"), "{text}");

    drop(pool);
    let _ = std::fs::remove_dir_all(&root);
}

fn tool_outcome(root: &Path) -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(root.join("sessions/fixture-tool-outcome.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn approved_tool_executes_against_the_workspace() {
    if !python3_available() {
        return;
    }
    let root = workspace("approve");
    let reply = run_one(
        &root,
        HostPolicy::approving(),
        "please list the workspace",
        None,
    );
    assert_eq!(reply.conversation_id, "durable-fixture-1");
    assert_eq!(reply.task_id, "generation-fixture-1");
    assert_eq!(reply.text, "handled please list the workspace");
    assert!(!reply.cancelled);

    let outcome = tool_outcome(&root);
    assert_eq!(outcome["isError"], serde_json::json!(false));
    let text = outcome["text"].as_str().unwrap();
    assert!(
        text.contains("fake_glass_core_runtime.py"),
        "the real workspace listing should reach the model: {text}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn denied_tool_fails_closed_without_running() {
    if !python3_available() {
        return;
    }
    let root = workspace("deny");
    let reply = run_one(
        &root,
        HostPolicy::denying(),
        "please list the workspace",
        None,
    );
    assert_eq!(reply.conversation_id, "durable-fixture-1");

    let outcome = tool_outcome(&root);
    assert_eq!(outcome["isError"], serde_json::json!(true));
    let text = outcome["text"].as_str().unwrap();
    assert!(
        text.contains("declined glass.workspace.list") && text.contains("host policy"),
        "a denial should name the tool and its reason: {text}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn each_message_in_one_conversation_gets_its_own_task() {
    if !python3_available() {
        return;
    }
    let root = workspace("distinct-tasks");
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let mut conversation = spawn_conversation(
        &root,
        &script,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
    );

    let first = run_turn(
        &turn("first message", None),
        &mut conversation,
        TurnOptions::default(),
    )
    .unwrap();
    let second = run_turn(
        &turn("second message", Some(first.conversation_id.clone())),
        &mut conversation,
        TurnOptions::default(),
    )
    .unwrap();

    assert_eq!(second.conversation_id, first.conversation_id);
    assert_ne!(
        second.task_id, first.task_id,
        "durable dedupe must not drop a second message in the same conversation"
    );
    assert_eq!(second.text, "handled second message");

    let _ = conversation.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn asking_policy_waits_for_an_explicit_inbox_decision() {
    if !python3_available() {
        return;
    }
    let root = workspace("ask");
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let approvals = ApprovalInbox::default();
    let mut conversation = spawn_conversation(
        &root,
        &script,
        &storage_dir,
        HostPolicy::asking(),
        approvals.clone(),
    );

    let worker = std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            run_turn(
                &turn("please list the workspace", None),
                &mut conversation,
                TurnOptions::default(),
            )
            .unwrap()
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(pending) = approvals.pending().first() {
                assert_eq!(pending.tool, "glass.workspace.list");
                approvals.resolve(&pending.request_id, glass_marble::Approval::Approve);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for a pending approval"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        handle.join().unwrap()
    });

    assert_eq!(worker.text, "handled please list the workspace");
    assert_eq!(tool_outcome(&root)["isError"], serde_json::json!(false));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_stopped_turn_does_not_wait_out_a_pending_approval() {
    if !python3_available() {
        return;
    }
    let root = workspace("cancel-approval");
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let approvals = ApprovalInbox::default();
    let mut conversation = spawn_conversation(
        &root,
        &script,
        &storage_dir,
        HostPolicy::asking(),
        approvals.clone(),
    )
    .with_approval_timeout(std::time::Duration::from_secs(60));

    let cancelled = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&cancelled);
    let watched = approvals.clone();
    let started = std::time::Instant::now();
    let reply = std::thread::scope(|scope| {
        scope.spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while watched.pending().is_empty() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the turn never parked on an approval"
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            stop.store(true, Ordering::Relaxed);
        });
        run_turn(
            &turn("please list the workspace", None),
            &mut conversation,
            TurnOptions {
                cancel: Some(&cancelled),
                ..TurnOptions::default()
            },
        )
        .unwrap()
    });
    let elapsed = started.elapsed();

    assert!(reply.cancelled, "a stopped turn reports the cancellation");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "a stopped turn must not wait out its approval timeout: {elapsed:?}"
    );
    assert!(
        approvals.pending().is_empty(),
        "a stopped approval leaves nothing pending"
    );
    let outcome = tool_outcome(&root);
    assert_eq!(outcome["isError"], serde_json::json!(true));
    let text = outcome["text"].as_str().unwrap();
    assert!(
        text.contains("cancelled"),
        "a stopped call should say it never ran: {text}"
    );

    let _ = conversation.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn marble_control_refuses_a_write_the_host_would_have_approved() {
    if !python3_available() {
        return;
    }
    let root = workspace("control-observe");
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let mut state = ProductState::default();
    state.control.profile = ControlProfile::Observe;
    let product = Arc::new(Mutex::new(state));
    let mut python = Command::new("python3");
    python
        .arg(&script)
        .arg("runtime")
        .arg("--tool")
        .arg("glass.workspace.write")
        .arg("--path")
        .arg("notes.md")
        .arg("--content")
        .arg("rewritten");
    let mut conversation = Conversation::spawn_with_router(
        python,
        &root,
        &storage_dir,
        // The host posture would approve, so only marble's own layer can refuse.
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        EventBus::default(),
    )
    .unwrap()
    .with_product(Arc::clone(&product));

    let reply = run_turn(
        &turn("rewrite the notes", None),
        &mut conversation,
        TurnOptions::default(),
    )
    .unwrap();
    assert_eq!(reply.text, "handled rewrite the notes");

    let outcome = tool_outcome(&root);
    assert_eq!(outcome["isError"], serde_json::json!(true));
    assert!(
        outcome["text"].as_str().unwrap().contains("read-only"),
        "a refusal should name its rule: {outcome}"
    );
    assert!(
        !root.join("notes.md").exists(),
        "the observe profile must not change the workspace"
    );

    let guard = product.lock().unwrap();
    let entry = guard
        .control
        .ledger
        .last()
        .expect("every governed decision is journalled");
    assert_eq!(entry.decision, "denied");
    assert_eq!(entry.tool, "glass.workspace.write");
    assert!(entry.reason.contains("read-only"), "{}", entry.reason);
    drop(guard);

    let _ = conversation.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_restarted_engine_keeps_the_durable_conversation() {
    if !python3_available() {
        return;
    }
    let root = workspace("restart");
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime");
    // The same recipe the served product holds, so a dead engine can be
    // replaced without restarting the assistant itself.
    let respawn_script = script.clone();
    let mut conversation = Conversation::spawn_with_router(
        python,
        &root,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        EventBus::default(),
    )
    .unwrap()
    .with_respawn(move || {
        let mut command = Command::new("python3");
        command.arg(&respawn_script).arg("runtime");
        command
    });

    let first = run_turn(
        &turn("first message", None),
        &mut conversation,
        TurnOptions::default(),
    )
    .unwrap();
    assert_eq!(first.text, "handled first message");
    assert!(
        !conversation.ensure_runtime().unwrap(),
        "a healthy engine is not replaced"
    );

    conversation.restart().unwrap();
    assert_eq!(conversation.restarts(), 1);
    assert!(!conversation.transport_failed());

    let second = run_turn(
        &turn("second message", Some(first.conversation_id.clone())),
        &mut conversation,
        TurnOptions::default(),
    )
    .unwrap();
    assert_eq!(second.conversation_id, first.conversation_id);
    assert_eq!(second.text, "handled second message");

    let _ = conversation.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_cancelled_turn_leaves_no_stale_reply_for_the_next_turn() {
    if !python3_available() {
        return;
    }
    let root = workspace("cancel-late-ack");
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime").arg("--late-abort-ack");
    let mut conversation = Conversation::spawn_with_router(
        python,
        &root,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        EventBus::default(),
    )
    .unwrap();

    let cancelled = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&cancelled);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            stop.store(true, Ordering::Relaxed);
        });
        let first = run_turn(
            &turn("first message", None),
            &mut conversation,
            TurnOptions {
                cancel: Some(&cancelled),
                ..TurnOptions::default()
            },
        )
        .unwrap();
        assert!(first.cancelled, "a cancelled turn reports the cancellation");
    });

    // The abort acknowledgement arrived after the run reply it was racing, so
    // the next turn has to recognise it as a reply from the previous turn.
    let second = run_turn(
        &turn("second message", None),
        &mut conversation,
        TurnOptions::default(),
    )
    .unwrap();
    assert_eq!(second.text, "handled second message");

    let _ = conversation.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// A stop pressed while a governed tool is already executing. The read below
/// blocks on a FIFO nobody writes to, so only an interruptible call lets the turn
/// settle without waiting for the tool to report back.
#[test]
fn a_stop_request_interrupts_a_governed_tool_that_is_already_running() {
    if !python3_available() {
        return;
    }
    let root = workspace("stop-running-tool");
    let fifo = root.join("slow.txt");
    let made = Command::new("python3")
        .arg("-c")
        .arg("import os,sys; os.mkfifo(sys.argv[1])")
        .arg(&fifo)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !made {
        let _ = std::fs::remove_dir_all(&root);
        return;
    }
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let mut python = Command::new("python3");
    python
        .arg(&script)
        .arg("runtime")
        .arg("--tool")
        .arg("glass.workspace.read")
        .arg("--path")
        .arg("slow.txt");
    let mut conversation = Conversation::spawn_with_router(
        python,
        &root,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        EventBus::default(),
    )
    .unwrap();

    let cancelled = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&cancelled);
    let started = std::time::Instant::now();
    let reply = std::thread::scope(|scope| {
        scope.spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            stop.store(true, Ordering::Relaxed);
        });
        run_turn(
            &turn("read the slow file", None),
            &mut conversation,
            TurnOptions {
                cancel: Some(&cancelled),
                timeout: std::time::Duration::from_secs(20),
                ..TurnOptions::default()
            },
        )
        .unwrap()
    });
    assert!(reply.cancelled, "the stopped turn reports the cancellation");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "the turn must settle on the stop request, not on the blocked read: {:?}",
        started.elapsed()
    );

    let outcome = tool_outcome(&root);
    assert_eq!(outcome["isError"], serde_json::json!(true));
    assert!(
        outcome["text"]
            .as_str()
            .unwrap()
            .contains("cancelled while this tool call was running"),
        "the runtime learns the running call was stopped: {outcome}"
    );

    let _ = conversation.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_runtime_without_mixed_capability_is_rejected() {
    if !python3_available() {
        return;
    }
    let root = workspace("no-mixed");
    let script = root.join("stale_runtime.py");
    std::fs::write(
        &script,
        "import json,sys\n\
         for line in sys.stdin:\n\
         \x20   message = json.loads(line)\n\
         \x20   if message.get('method') == 'initialize':\n\
         \x20       print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':\
         {'protocolVersion':1,'capabilities':{'durable':True,'resume':True}}}), flush=True)\n",
    )
    .unwrap();
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime");
    let spawned = Conversation::spawn_with_router(
        python,
        &root,
        &root.join("sessions"),
        HostPolicy::denying(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        EventBus::default(),
    );
    assert!(spawned.is_err(), "a v1 runtime must not be adopted");
    let _ = std::fs::remove_dir_all(&root);
}

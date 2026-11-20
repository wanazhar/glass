use glass_marble::{
    AcceptedTurn, ApprovalInbox, CancelRegistry, ControlProfile, Conversation, EventBus,
    HostPolicy, InboundMessage, LaneFactory, LiveStore, MarbleReply, MarbleTurn, ProductState,
    StateStore, ToolRouter, TurnCommand, TurnOptions, TurnPool, TurnRequest, run_turn,
    spawn_turn_pool, to_turn_input,
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

/// One engine per lane, each with its own session store and its own id
/// namespace. The names are how a test tells which lane a turn reached.
fn lane_factory(
    root: &Path,
    script: &Path,
    delay_ms: u64,
    started: Arc<Mutex<Vec<u8>>>,
) -> LaneFactory {
    let root = root.to_path_buf();
    let script = script.to_path_buf();
    Arc::new(move |lane: u8| {
        started
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(lane);
        let mut python = Command::new("python3");
        python
            .arg(&script)
            .arg("runtime")
            .arg("--delay-ms")
            .arg(delay_ms.to_string())
            .arg("--id-prefix")
            .arg(format!("lane-{lane}-"));
        Conversation::spawn_with_router(
            python,
            &root,
            &root.join("sessions").join(format!("lane-{lane}")),
            HostPolicy::approving(),
            ApprovalInbox::default(),
            ToolRouter::new(&root)?,
            EventBus::default(),
        )
    })
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

fn wait_for_settled(state: &Mutex<ProductState>, task_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match task_status(state, task_id).as_str() {
            "completed" => return,
            "failed" | "cancelled" => {
                panic!("turn {task_id} settled as {}", task_status(state, task_id))
            }
            _ => {}
        }
        assert!(Instant::now() < deadline, "turn {task_id} never settled");
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
        lane_factory(&root, &script, 1_500, Arc::clone(&started)),
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
        lane_factory(&root, &script, 0, Arc::clone(&started)),
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
        lane_factory(&root, &script, 0, Arc::clone(&started)),
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

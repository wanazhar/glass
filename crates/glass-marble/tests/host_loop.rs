use glass_marble::{
    ApprovalInbox, Conversation, EventBus, HostPolicy, InboundMessage, MarbleReply, MarbleTurn,
    ToolRouter, TurnOptions, run_turn, to_turn_input,
};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    assert!(outcome["text"].as_str().unwrap().contains("host declined"));

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

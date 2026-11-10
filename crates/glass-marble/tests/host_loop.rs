use glass_marble::{
    ApprovalInbox, Conversation, HostPolicy, InboundMessage, MarbleReply, MarbleTurn, run_turn,
    to_turn_input,
};
use std::path::PathBuf;
use std::process::Command;

const FAKE_RUNTIME_SOURCE: &str = include_str!("fixtures/fake_glass_core_runtime.py");

fn write_fake_runtime(root: &std::path::Path) -> PathBuf {
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, FAKE_RUNTIME_SOURCE).unwrap();
    script
}

fn python3_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn turn(request_id: &str, text: &str, conversation_id: Option<String>) -> MarbleTurn {
    MarbleTurn {
        conversation_id,
        request_id: request_id.into(),
        input: to_turn_input(&InboundMessage::new(text)),
    }
}

fn run_marble_turn(
    root: &std::path::Path,
    script: &std::path::Path,
    storage_dir: &std::path::Path,
    host: HostPolicy,
    request_id: &str,
    text: &str,
    conversation_id: Option<String>,
) -> MarbleReply {
    let mut python = Command::new("python3");
    python.arg(script).arg("runtime");
    let mut conversation =
        Conversation::spawn_with_policy(python, root, storage_dir, host, ApprovalInbox::default())
            .unwrap();
    run_turn(&turn(request_id, text, conversation_id), &mut conversation).unwrap()
}

fn run_marble_turn_new_conversation(
    root: &std::path::Path,
    script: &std::path::Path,
    storage_dir: &std::path::Path,
    host: HostPolicy,
    request_id: &str,
    text: &str,
) -> MarbleReply {
    run_marble_turn(root, script, storage_dir, host, request_id, text, None)
}

#[test]
fn host_loop_proves_durable_create_approval_and_final_text() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "glass-marble-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        "host-loop"
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");

    let __start = std::time::Instant::now();
    eprintln!("T0 {:?}", __start.elapsed());
    let reply = run_marble_turn_new_conversation(
        &root,
        &script,
        &storage_dir,
        HostPolicy::approving(),
        "marble-turn-1",
        "please list the workspace",
    );
    eprintln!("T1 {:?}", __start.elapsed());
    eprintln!("TURN_ELAPSED {:?}", __start.elapsed());
    assert_eq!(reply.conversation_id, "durable-fixture-1");
    assert_eq!(reply.task_id, "generation-fixture-1");
    assert_eq!(reply.text, "handled please list the workspace");

    let outcome: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(storage_dir.join("fixture-tool-outcome.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(outcome["isError"], serde_json::json!(false));
    assert!(outcome["text"].as_str().unwrap().contains("host approved"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn host_loop_proves_durable_resume_reuses_conversation_and_task() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "glass-marble-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        "host-resume"
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");

    let first = run_marble_turn_new_conversation(
        &root,
        &script,
        &storage_dir,
        HostPolicy::approving(),
        "marble-resume-1",
        "please list the workspace",
    );
    assert_eq!(first.conversation_id, "durable-fixture-1");
    assert_eq!(first.task_id, "generation-fixture-1");

    let resumed = run_marble_turn(
        &root,
        &script,
        &storage_dir,
        HostPolicy::approving(),
        "marble-resume-1",
        "please list the workspace",
        Some(first.conversation_id.clone()),
    );
    assert_eq!(resumed.conversation_id, first.conversation_id);
    assert_eq!(resumed.task_id, first.task_id);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn host_loop_proves_host_deny_fails_closed() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "glass-marble-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        "host-deny"
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");

    let reply = run_marble_turn_new_conversation(
        &root,
        &script,
        &storage_dir,
        HostPolicy::denying(),
        "marble-deny-1",
        "please list the workspace",
    );
    assert_eq!(reply.conversation_id, "durable-fixture-1");
    assert_eq!(reply.task_id, "generation-fixture-1");

    let outcome: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(storage_dir.join("fixture-tool-outcome.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(outcome["isError"], serde_json::json!(true));
    assert!(outcome["text"].as_str().unwrap().contains("host declined"));

    let _ = std::fs::remove_dir_all(&root);
}

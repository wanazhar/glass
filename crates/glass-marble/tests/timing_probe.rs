use glass_marble::{ApprovalInbox, Conversation, InboundMessage, to_turn_input};
use std::process::Command;

const FAKE_RUNTIME_SOURCE: &str = include_str!("fixtures/fake_glass_core_runtime.py");

fn write_fake_runtime(root: &std::path::Path) -> std::path::PathBuf {
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

#[test]
fn timing_probe() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "glass-marble-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        "timing-probe"
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = write_fake_runtime(&root);
    let storage_dir = root.join("sessions");
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime");
    let mut conversation =
        Conversation::spawn(python, &root, &storage_dir, ApprovalInbox::default()).unwrap();
    eprintln!("PHASE spawn");
    let conversation_id = conversation.create().unwrap();
    eprintln!("PHASE create id={conversation_id}");
    let task_id = conversation
        .submit(
            &conversation_id,
            "probe-1",
            &to_turn_input(&InboundMessage::new("hi")),
        )
        .unwrap();
    eprintln!("PHASE submit task={task_id}");
    let run_id = conversation.start(&task_id).unwrap();
    eprintln!("PHASE run id={run_id}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(40);
    loop {
        let incoming = conversation
            .recv_typed_timeout(std::time::Duration::from_secs(40))
            .unwrap()
            .unwrap();
        eprintln!("INCOMING {incoming:?}");
        match incoming {
            glass_runtime_client::Incoming::Response { id, result } => {
                eprintln!("RESPONSE id={id:?} ok={}", result.is_ok());
                break;
            }
            glass_runtime_client::Incoming::Event(event) => {
                eprintln!("EVENT kind={} data={}", event.kind, event.data);
            }
            glass_runtime_client::Incoming::Tool { request_id, call } => {
                eprintln!("TOOL request_id={request_id:?} call={call:?}");
                conversation.approvals().resolve(
                    request_id.as_str().unwrap_or_default(),
                    glass_marble::Approval::Approve,
                );
                glass_marble::host::answer_tool_request(
                    conversation.client_mut(),
                    request_id,
                    &call,
                    glass_marble::Approval::Approve,
                )
                .unwrap();
            }
            glass_runtime_client::Incoming::Unknown(_) => {}
        }
        if std::time::Instant::now() > deadline {
            panic!("probe timed out");
        }
    }
    eprintln!("PHASE response");
    conversation.shutdown().unwrap();
    eprintln!("PHASE shutdown");
    let _ = std::fs::remove_dir_all(&root);
}

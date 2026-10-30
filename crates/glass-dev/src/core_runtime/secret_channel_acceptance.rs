use super::{drain, python3, spawn_mode};
use crate::resident_harness::{
    ResidentSecretInputAnswer, ResidentSecretInputRequest, ResidentSessionRequest,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const SENTINEL: &str = "offline-secret-sentinel";

fn test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "glass-core-secret-channel-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create secret channel fixture root");
    root
}

fn next_event(runtime: &mut super::super::GlassCoreRuntime) -> Value {
    runtime
        .recv_event_timeout(Duration::from_secs(5))
        .expect("read fixture event")
        .expect("fixture event before timeout")
}

fn secret_request(
    runtime: &mut super::super::GlassCoreRuntime,
) -> (Value, ResidentSecretInputRequest) {
    loop {
        let event = next_event(runtime);
        if event["type"] == "human_input_request" {
            let request = serde_json::from_value(event["request"].clone())
                .expect("typed bounded secret request metadata");
            return (event, request);
        }
    }
}

fn start_secret_turn(runtime: &mut super::super::GlassCoreRuntime) {
    runtime
        .start_request(ResidentSessionRequest::Prompt {
            text: "request a test credential".into(),
            context: None,
        })
        .expect("start fixture prompt");
}

fn fixture_secret_results(root: &Path) -> Vec<Value> {
    let path = root.join("sessions/fixture-secret-results.jsonl");
    fs::read_to_string(path)
        .expect("read sanitized fixture response record")
        .lines()
        .map(|line| serde_json::from_str(line).expect("parse sanitized response record"))
        .collect()
}

fn assert_secret_absent_from_files(root: &Path) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("read fixture directory") {
            let path = entry.expect("fixture directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(contents) = fs::read_to_string(&path) {
                assert!(
                    !contents.contains(SENTINEL),
                    "secret sentinel reached fixture file {}",
                    path.display()
                );
            }
        }
    }
}

fn assert_secret_capability_record(root: &Path, advertised: bool) {
    let record: Value = serde_json::from_slice(
        &fs::read(root.join("sessions/fixture-secret-capabilities.json"))
            .expect("read capability negotiation record"),
    )
    .expect("parse capability negotiation record");
    let required = record["requiredCapabilities"]
        .as_array()
        .expect("Core required capabilities");
    assert!(required.iter().any(|value| value == "mixedTools"));
    assert!(required.iter().any(|value| value == "maskedSecretInput"));
    assert_eq!(record["maskedSecretInputAdvertised"], advertised);
}

#[test]
fn masked_secret_capability_is_required_during_runtime_negotiation() {
    if python3().is_none() {
        return;
    }
    let root = test_root("capability");
    let error = spawn_mode(&root, None, "missing-masked-secret-input")
        .expect_err("runtime without masked secret support must be rejected");
    assert!(
        error
            .to_string()
            .contains("does not support the masked secret input channel"),
        "{error}"
    );
    assert_secret_capability_record(&root, false);
    let requests =
        fs::read_to_string(root.join("sessions/fixture-requests.jsonl")).unwrap_or_default();
    assert!(!requests.contains("session/new"));
    assert!(!requests.contains("turn/prompt"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn secret_request_binds_exact_id_session_and_turn_and_never_enters_persisted_state() {
    if python3().is_none() {
        return;
    }
    let root = test_root("answer");
    let mut runtime = spawn_mode(&root, None, "secret-input").expect("start secret fixture");
    assert_secret_capability_record(&root, true);
    start_secret_turn(&mut runtime);
    let (request_event, request) = secret_request(&mut runtime);

    assert_eq!(request.request_id, "human-secret-fixture");
    assert_eq!(request.session_id, runtime.session_id());
    assert_eq!(request.turn_id, "turn-1");
    assert_eq!(request.purpose, "fixture login");
    assert_eq!(request.destination, "https://example.test");
    assert_eq!(request.tool, "web_secret_request");
    assert_eq!(request.operation, "fixture");
    assert_eq!(request.max_bytes, 4096);
    let now_ms = super::super::unix_time_ms();
    assert!(request.expires_at_ms > now_ms);
    assert!(request.expires_at_ms <= now_ms + 120_000);

    let mut wrong_id = request.clone();
    wrong_id.request_id = "another-request".into();
    assert!(
        runtime
            .answer_secret_input(&wrong_id, ResidentSecretInputAnswer::Declined)
            .is_err()
    );

    let mut wrong_session = request.clone();
    wrong_session.session_id = "another-session".into();
    assert!(
        runtime
            .answer_secret_input(&wrong_session, ResidentSecretInputAnswer::Declined)
            .is_err()
    );

    let mut wrong_turn = request.clone();
    wrong_turn.turn_id = "another-turn".into();
    assert!(
        runtime
            .answer_secret_input(&wrong_turn, ResidentSecretInputAnswer::Declined)
            .is_err()
    );

    for invalid in [
        String::new(),
        "\u{7}".into(),
        "x".repeat(request.max_bytes + 1),
    ] {
        assert!(
            runtime
                .answer_secret_input(
                    &request,
                    ResidentSecretInputAnswer::Provided(Zeroizing::new(invalid)),
                )
                .is_err()
        );
    }

    let answer = ResidentSecretInputAnswer::Provided(Zeroizing::new(SENTINEL.into()));
    assert!(!format!("{answer:?}").contains(SENTINEL));
    runtime
        .answer_secret_input(&request, answer)
        .expect("answer the exact pending request");
    assert!(
        runtime
            .answer_secret_input(&request, ResidentSecretInputAnswer::Declined)
            .is_err()
    );

    let events = drain(&mut runtime, Duration::from_secs(3));
    assert!(events.iter().any(|event| event["type"] == "agent_settled"));
    assert!(!format!("{request_event:?}").contains(SENTINEL));
    assert!(!format!("{events:?}").contains(SENTINEL));
    assert!(!format!("{runtime:?}").contains(SENTINEL));
    let results = fixture_secret_results(&root);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["requestId"], request.request_id);
    assert_eq!(results[0]["status"], "answered");
    assert_eq!(results[0]["secretMatchesSentinel"], true);
    assert_secret_absent_from_files(&root);

    drop(runtime);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn secret_cancel_and_expiry_are_one_shot_cancelled_responses() {
    if python3().is_none() {
        return;
    }
    for mode in ["secret-input-cancel", "secret-input-expiring"] {
        let root = test_root(mode);
        let mut runtime = spawn_mode(&root, None, mode).expect("start secret fixture");
        start_secret_turn(&mut runtime);
        let (request_event, request) = secret_request(&mut runtime);
        if mode == "secret-input-expiring" {
            let remaining_ms = request
                .expires_at_ms
                .saturating_sub(super::super::unix_time_ms());
            thread::sleep(Duration::from_millis(
                u64::try_from(remaining_ms).unwrap_or(0).saturating_add(30),
            ));
            let error = runtime
                .answer_secret_input(
                    &request,
                    ResidentSecretInputAnswer::Provided(Zeroizing::new(SENTINEL.into())),
                )
                .expect_err("expired request must not accept a secret");
            assert!(error.to_string().contains("expired"), "{error}");
        } else {
            runtime
                .cancel_pending_secret_inputs()
                .expect("cancel pending prompt");
        }
        assert!(
            runtime
                .answer_secret_input(&request, ResidentSecretInputAnswer::Declined)
                .is_err()
        );
        let events = drain(&mut runtime, Duration::from_secs(3));
        assert!(events.iter().any(|event| event["type"] == "agent_settled"));
        assert!(!format!("{request_event:?}{events:?}{runtime:?}").contains(SENTINEL));
        let results = fixture_secret_results(&root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["requestId"], request.request_id);
        assert_eq!(results[0]["status"], "cancelled");
        assert_eq!(results[0]["secretMatchesSentinel"], false);
        assert_secret_absent_from_files(&root);
        drop(runtime);
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn core_cancel_notification_requires_exact_session_and_request_id() {
    if python3().is_none() {
        return;
    }
    let root = test_root("core-cancel");
    let mut runtime =
        spawn_mode(&root, None, "secret-input-core-cancel").expect("start secret cancel fixture");
    start_secret_turn(&mut runtime);
    let (request_event, request) = secret_request(&mut runtime);

    let first_probe = next_event(&mut runtime);
    assert_eq!(first_probe["type"], "message_update");
    assert!(
        first_probe
            .to_string()
            .contains("wrong-session cancellation ignored")
    );
    let second_probe = next_event(&mut runtime);
    assert_eq!(second_probe["type"], "message_update");
    assert!(
        second_probe
            .to_string()
            .contains("wrong-id cancellation ignored")
    );
    let cancellation = next_event(&mut runtime);
    assert_eq!(cancellation["type"], "human_input_cancelled");
    assert_eq!(cancellation["requestId"], request.request_id);
    assert!(
        runtime
            .answer_secret_input(&request, ResidentSecretInputAnswer::Declined)
            .is_err()
    );

    let events = drain(&mut runtime, Duration::from_secs(3));
    assert!(events.iter().any(|event| event["type"] == "agent_settled"));
    assert!(
        !format!(
            "{request_event:?}{first_probe:?}{second_probe:?}{cancellation:?}{events:?}{runtime:?}"
        )
        .contains(SENTINEL)
    );
    drop(runtime);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn secret_request_parser_requires_canonical_https_origin_and_bounded_metadata() {
    let now = super::super::unix_time_ms();
    let base = json!({
        "sessionId":"fixture-session",
        "turnId":"turn-1",
        "kind":"secret",
        "purpose":"fixture login",
        "destination":"https://example.test",
        "tool":"web_secret_request",
        "operation":"fixture",
        "expiresAtMs":now + 60_000,
        "maxBytes":4096,
    });
    assert!(
        super::super::parse_secret_input_request("request-1", &base, "fixture-session").is_ok()
    );

    for (field, value) in [
        ("sessionId", json!("other-session")),
        ("kind", json!("text")),
        ("purpose", json!("invalid\npurpose")),
        ("destination", json!("http://example.test")),
        ("destination", json!("https://example.test/path")),
        ("destination", json!("https://user@example.test")),
        ("destination", json!("https://example.test?token=x")),
        ("tool", json!("web_open")),
        ("expiresAtMs", json!(now + 125_000)),
        ("maxBytes", json!(2048)),
    ] {
        let mut invalid = base.clone();
        invalid[field] = value;
        assert!(
            super::super::parse_secret_input_request("request-1", &invalid, "fixture-session")
                .is_err(),
            "accepted invalid secret request field {field}: {invalid}"
        );
    }
}

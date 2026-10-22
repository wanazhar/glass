use glass_runtime_client::{Incoming, RuntimeClient, SessionOptions, json};
use std::time::Duration;
fn response(client: &RuntimeClient, id: &str) -> serde_json::Value {
    match client
        .recv_typed_timeout(Duration::from_secs(10))
        .unwrap()
        .expect("response timeout")
    {
        Incoming::Response { id: got, result } if got == id => result.unwrap(),
        other => panic!("unexpected frame: {other:?}"),
    }
}
#[test]
#[ignore = "requires GLASS_CORE_TEST_BINARY pointing to a built release"]
fn actual_subprocess_session_contract() {
    let binary = std::env::var("GLASS_CORE_TEST_BINARY").expect("set GLASS_CORE_TEST_BINARY");
    let root = std::env::temp_dir().join(format!("gc-rust-host-{}", std::process::id()));
    std::fs::create_dir_all(root.join("config/glass-core")).unwrap();
    std::fs::write(root.join("config/glass-core/config.toml"),"provider=\"openai\"\nmodel=\"mock\"\napi_key_env=\"\"\nbase_url=\"http://127.0.0.1:1/v1\"\n").unwrap();
    let mut command = std::process::Command::new(binary);
    command
        .arg("runtime")
        .current_dir(&root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"));
    let mut client = RuntimeClient::spawn_command(command).unwrap();
    let id = client.initialize(root.join("sessions")).unwrap();
    assert_eq!(response(&client, &id)["protocolVersion"], 1);
    let id = client
        .new_session(&SessionOptions::host(&root, vec![]))
        .unwrap();
    let session = response(&client, &id)["sessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    let id = client.entries(&session, 0, 10).unwrap();
    assert_eq!(response(&client, &id)["entries"], json!([]));
    let id = client
        .request(
            "session/name",
            json!({"sessionId":session,"name":"Rust SDK"}),
        )
        .unwrap();
    response(&client, &id);
    let id = client
        .request("session/close", json!({"sessionId":session}))
        .unwrap();
    response(&client, &id);
    let id = client
        .resume_session(&session, glass_runtime_client::ToolMode::Host)
        .unwrap();
    assert_eq!(response(&client, &id)["sessionId"], session);
    let id = client
        .request("session/describe", json!({"sessionId":session}))
        .unwrap();
    assert_eq!(response(&client, &id)["name"], "Rust SDK");
    let id = client.request("shutdown", json!({})).unwrap();
    response(&client, &id);
    drop(client);
    std::fs::remove_dir_all(root).unwrap();
}

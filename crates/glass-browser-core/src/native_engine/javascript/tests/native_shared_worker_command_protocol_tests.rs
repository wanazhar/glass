use super::{NativeMessagePortTransfer, NativeScriptCommand};

#[test]
fn shared_worker_create_serialization_has_no_cookie_profile() {
    let command = NativeScriptCommand::SharedWorkerCreate {
        connection_id: 1,
        href: "https://example.test/worker.js".into(),
        name: "worker".into(),
        worker_type: "module".into(),
        credentials: "same-origin".into(),
        extended_lifetime: false,
        referrer_policy: None,
        constructor_storage_key: None,
        transfer_port: NativeMessagePortTransfer {
            bridge_key: "bridge-1".into(),
            port_id: 1,
            peer_id: 2,
            hidden: false,
        },
    };

    let payload = serde_json::to_value(command).expect("SharedWorker command serializes");
    assert_eq!(payload["kind"], "sharedWorkerCreate");
    assert!(payload.get("cookie_profile").is_none());
}

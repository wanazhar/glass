use super::*;
use serde_json::json;

#[test]
fn malformed_service_worker_message_dispatches_extendable_messageerror() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("sw-messageerror-dispatch")
        .expect("native JavaScript runtime is available");
    let worker_url = "https://message-events.example/sw.js";
    let source = json!({
        "clientId": "message-client",
        "clientUrl": "https://message-events.example/page",
        "clientType": "window",
        "frameType": "top-level",
        "visibilityState": "visible",
        "focused": true,
        "controlled": true,
    });
    runtime.set_service_worker_clients(vec![source.clone()]);
    runtime
        .evaluate_service_worker_source(
            7,
            worker_url,
            None,
            r#"
globalThis.messageCount = 0;
globalThis.messageErrorObservation = null;
addEventListener('message', () => { messageCount += 1; });
addEventListener('messageerror', event => {
  globalThis.messageErrorObservation = {
    type: event.type,
    isTrusted: event.isTrusted,
    isEvent: event instanceof Event,
    isExtendableEvent: event instanceof ExtendableEvent,
    isExtendableMessageEvent: event instanceof ExtendableMessageEvent,
    isMessageEvent: event instanceof MessageEvent,
    dataIsNull: event.data === null,
    origin: event.origin,
    lastEventId: event.lastEventId,
    sourceId: event.source.id,
    sourceUrl: event.source.url,
    portsLength: event.ports.length,
    portsFrozen: Object.isFrozen(event.ports),
  };
  event.waitUntil(Promise.resolve());
  event.source.postMessage({ kind: 'messageerror-reply' });
});
"#,
            &BTreeMap::new(),
        )
        .expect("Service Worker source evaluates");
    let malformed = json!({
        "__glassMessageClone": "glass-native-structured-clone-v1",
        "root": { "ref": 0 },
        "nodes": [{ "type": "invalid-node" }],
    });
    let evaluation = runtime
        .dispatch_service_worker_message(NativeServiceWorkerMessageEvent {
            worker_id: 7,
            worker_url,
            data: &malformed,
            transfer_ports: &[],
            source: &source,
            source_origin: "https://message-events.example",
            is_module: false,
        })
        .expect("malformed clone is delivered as messageerror");
    assert!(!evaluation.top_level_await_pending);
    assert!(evaluation.commands.iter().any(|command| matches!(
        command,
        NativeScriptCommand::ServiceWorkerClientPostMessage {
            worker_id: 7,
            client_id,
            ..
        } if client_id == "message-client"
    )));

    let bootstrap = service_worker_bootstrap(
        7,
        worker_url,
        runtime.now_ms(),
        &BTreeMap::new(),
        false,
        runtime.worker_global_referrer_policy,
    )
    .expect("Service Worker bootstrap is valid");
    let observation = runtime
        .evaluate_worker_source_with_bootstrap(
            super::runtime::NativeWorkerSourceIdentity {
                worker_id: 7,
                worker_url,
                module_name: None,
            },
            "({ messageCount, messageErrorObservation })",
            bootstrap,
            super::runtime::NativeWorkerExecutionMode {
                service_worker: true,
                await_promise: false,
            },
        )
        .expect("Service Worker event result remains inspectable")
        .value;
    assert_eq!(observation["messageCount"], 0);
    assert_eq!(
        observation["messageErrorObservation"],
        json!({
            "type": "messageerror",
            "isTrusted": true,
            "isEvent": true,
            "isExtendableEvent": true,
            "isExtendableMessageEvent": true,
            "isMessageEvent": false,
            "dataIsNull": true,
            "origin": "https://message-events.example",
            "lastEventId": "",
            "sourceId": "message-client",
            "sourceUrl": "https://message-events.example/page",
            "portsLength": 0,
            "portsFrozen": true,
        })
    );
}

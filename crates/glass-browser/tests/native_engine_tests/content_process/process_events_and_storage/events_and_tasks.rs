use super::*;

#[tokio::test]
async fn native_content_process_message_port_decode_recovery_and_bridge_close_both_directions() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.portEvents = [];
globalThis.portReady = false;
globalThis.workerEvents = [];
globalThis.workerErrors = [];
globalThis.localPortEvents = [];
globalThis.localCloseEvents = [];
globalThis.currentLocalCloseEvent = null;
globalThis.initiatorCloseCount = 0;
globalThis.pageBridgeCloseState = { event: null, records: [] };
globalThis.reverseBridgeCloseState = { event: null, records: [] };
globalThis.reversePortReady = false;
globalThis.independentMessages = [];
globalThis.independentChannel = new MessageChannel();
independentChannel.port1.addEventListener('message', event => independentMessages.push(event.data.value));
independentChannel.port1.start();
const recordClose = (state, kind, port, event, thisValue) => {
  if (state.event === null) state.event = event;
  state.records.push({
    kind,
    sameEvent: state.event === event,
    type: event.type,
    isEvent: event instanceof Event,
    targetIsPort: event.target === port,
    currentTargetIsPort: event.currentTarget === port,
    eventPhase: event.eventPhase,
    thisIsPort: thisValue === port,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
  });
};
const observeClose = (port, state) => {
  port.onclose = function(event) { recordClose(state, 'handler', port, event, this); };
  port.addEventListener('close', function(event) { recordClose(state, 'listener', port, event, this); });
};
const closeStateReset = state => state.event !== null
  && state.event.currentTarget === null && state.event.eventPhase === 0;
globalThis.localChannel = new MessageChannel();
globalThis.localPort = localChannel.port1;
globalThis.lastLocalPortEvent = null;
localPort.onclose = () => initiatorCloseCount++;
localChannel.port2.onclose = function(event) {
  if (currentLocalCloseEvent === null) currentLocalCloseEvent = event;
  localCloseEvents.push({
    kind: 'handler',
    sameEvent: currentLocalCloseEvent === event,
    type: event.type,
    isEvent: event instanceof Event,
    targetIsPeer: event.target === localChannel.port2,
    currentTargetIsPeer: event.currentTarget === localChannel.port2,
    eventPhase: event.eventPhase,
    thisIsPeer: this === localChannel.port2,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
  });
};
localChannel.port2.addEventListener('close', function(event) {
  if (currentLocalCloseEvent === null) currentLocalCloseEvent = event;
  localCloseEvents.push({
    kind: 'listener',
    sameEvent: currentLocalCloseEvent === event,
    type: event.type,
    isEvent: event instanceof Event,
    targetIsPeer: event.target === localChannel.port2,
    currentTargetIsPeer: event.currentTarget === localChannel.port2,
    eventPhase: event.eventPhase,
    thisIsPeer: this === localChannel.port2,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
  });
});
const recordPortEvent = (kind, message, target, events) => {
  events.push({
    kind,
    type: message.type,
    isMessageEvent: message instanceof MessageEvent,
    targetIsPort: message.target === target,
    currentTargetIsPort: message.currentTarget === target,
    dataIsNull: message.data === null,
    portCount: message.ports.length,
    bubbles: message.bubbles,
    cancelable: message.cancelable,
    value: message.data && message.data.value,
  });
};
localPort.addEventListener('message', message => {
  globalThis.lastLocalPortEvent = message;
  recordPortEvent('message', message, localPort, localPortEvents);
});
localPort.addEventListener('messageerror', message => {
  globalThis.lastLocalPortEvent = message;
  recordPortEvent('messageerror', message, localPort, localPortEvents);
});
localPort.start();
globalThis.worker = new Worker('/message-port-decode-worker.js');
worker.onmessage = event => {
  workerEvents.push({ data: event.data, portCount: event.ports.length });
  if (event.data.kind === 'port-ready') {
    globalThis.receivingPort = event.ports[0];
    const observe = (kind, message) => {
      globalThis.lastPortEvent = message;
      recordPortEvent(kind, message, receivingPort, portEvents);
    };
    receivingPort.addEventListener('message', message => observe('message', message));
    receivingPort.addEventListener('messageerror', message => observe('messageerror', message));
    observeClose(receivingPort, pageBridgeCloseState);
    receivingPort.start();
    globalThis.portReady = true;
  } else if (event.data.kind === 'reverse-port') {
    globalThis.reverseReceivingPort = event.ports[0];
    globalThis.reverseBridgeKey = reverseReceivingPort.__glassMessagePortBridgeKey;
    observeClose(reverseReceivingPort, reverseBridgeCloseState);
    reverseReceivingPort.addEventListener('message', message => {
      portEvents.push({ kind: 'unexpected-reverse-message', value: message.data.value });
    });
    reverseReceivingPort.start();
    globalThis.reversePortReady = true;
  }
};
worker.onerror = event => workerErrors.push(event.message);
</script><main>MessagePort recovery and bridge close</main>"#;
    let worker_script = r#"
const channel = new MessageChannel();
const initialCloseState = { event: null, records: [] };
const reverseCloseState = { event: null, records: [] };
const recordClose = (state, kind, port, event, thisValue) => {
  if (state.event === null) state.event = event;
  state.records.push({
    kind,
    sameEvent: state.event === event,
    type: event.type,
    isEvent: event instanceof Event,
    targetIsPort: event.target === port,
    currentTargetIsPort: event.currentTarget === port,
    eventPhase: event.eventPhase,
    thisIsPort: thisValue === port,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
  });
};
const observeClose = (port, state, afterClose) => {
  port.onclose = function(event) { recordClose(state, 'handler', port, event, this); afterClose(); };
  port.addEventListener('close', function(event) { recordClose(state, 'listener', port, event, this); });
};
const closeReport = (port, state) => ({
  records: state.records,
  eventStateReset: state.event === null
    || (state.event.currentTarget === null && state.event.eventPhase === 0),
  portOpen: !port.__glassMessageClosed,
  bridgeKeyCleared: port.__glassMessagePortBridgeKey === null,
  peerCleared: port.__glassMessagePortPeer === null,
});
observeClose(channel.port2, initialCloseState, () => {
  channel.port2.postMessage({ value: 'must-not-cross-after-close' });
  Promise.resolve().then(() => postMessage({
    kind: 'page-closed-worker-peer',
    report: closeReport(channel.port2, initialCloseState),
  }));
});
channel.port2.addEventListener('message', event => {
  channel.port2.postMessage({ value: event.data.value + 1 });
});
channel.port2.start();
postMessage({ kind: 'port-ready' }, [channel.port1]);
let reverseChannel = null;
self.onmessage = event => {
  if (event.data.kind === 'make-reverse') {
    reverseChannel = new MessageChannel();
    observeClose(reverseChannel.port2, reverseCloseState, () => {});
    reverseChannel.port2.start();
    postMessage({ kind: 'reverse-port' }, [reverseChannel.port1]);
  } else if (event.data.kind === 'close-reverse') {
    reverseChannel.port2.postMessage({ value: 'must-be-purged-before-close' });
    reverseChannel.port2.close();
    reverseChannel.port2.close();
    postMessage({
      kind: 'worker-closed-initiator',
      report: closeReport(reverseChannel.port2, reverseCloseState),
    });
  }
};
"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/message-port-decode-page", "text/html", page),
            (
                "/message-port-decode-worker.js",
                "text/javascript",
                worker_script,
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/message-port-decode-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let mut port_ready = false;
    for _ in 0..8 {
        port_ready = engine
            .evaluate_async("portReady")
            .await
            .unwrap()
            .as_bool()
            .unwrap_or(false);
        if port_ready {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert!(
        port_ready,
        "worker-transferred MessagePort should be available: {}",
        engine
            .evaluate_async("({ workerEvents, workerErrors })")
            .await
            .unwrap()
    );

    assert_eq!(
        engine
            .evaluate_async(
                r#"const malformed = {
  data: {
    __glassMessageClone: 'glass-native-structured-clone-v1',
    root: { ref: 0 },
    nodes: [{ type: 'invalid-node' }],
  },
  transfer_ports: [
    { bridge_key: 'decode-failure-provisional-port', port_id: 700001, peer_id: 700002 },
    { bridge_key: 'decode-failure-retained-port', port_id: 700003, peer_id: 700004 },
  ],
  object_urls: [],
};
const retainedBridgeKey = 'decode-failure-retained-port';
const provisionalBridgeKey = 'decode-failure-provisional-port';
__glassDispatchMessagePortById(localChannel.port2.__glassMessagePortId, {
  data: {
    __glassMessageClone: 'glass-native-structured-clone-v1',
    root: { type: 'string', value: 'retain this endpoint' },
    nodes: [],
  },
  transfer_ports: [malformed.transfer_ports[1]],
  object_urls: [],
});
const retainedBridge = __glassMessageBridgeRegistry.get(retainedBridgeKey);
const portRegistryCountBeforeFailure = __glassMessagePortRegistry.size;
const bridgeRegistryCountBeforeFailure = __glassMessageBridgeRegistry.size;
__glassDispatchMessagePortById(localPort.__glassMessagePortId, malformed);
__glassDispatchMessagePortByBridge(receivingPort.__glassMessagePortBridgeKey, malformed);
await Promise.resolve();
({
  localPortEvents,
  portEvents,
  eventStateReset: lastLocalPortEvent.currentTarget === null
    && lastLocalPortEvent.eventPhase === 0
    && lastPortEvent.currentTarget === null
    && lastPortEvent.eventPhase === 0,
  provisionalBridgeRemoved: !__glassMessageBridgeRegistry.has(provisionalBridgeKey),
  retainedBridgePreserved: __glassMessageBridgeRegistry.get(retainedBridgeKey) === retainedBridge
    && !retainedBridge.__glassMessageClosed,
  registrySizesStable: __glassMessagePortRegistry.size === portRegistryCountBeforeFailure
    && __glassMessageBridgeRegistry.size === bridgeRegistryCountBeforeFailure,
  portsOpen: !localPort.__glassMessageClosed && !receivingPort.__glassMessageClosed,
})"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "localPortEvents": [{
                "kind": "messageerror",
                "type": "messageerror",
                "isMessageEvent": true,
                "targetIsPort": true,
                "currentTargetIsPort": true,
                "dataIsNull": true,
                "portCount": 0,
                "bubbles": false,
                "cancelable": false,
                "value": null,
            }],
            "portEvents": [{
                "kind": "messageerror",
                "type": "messageerror",
                "isMessageEvent": true,
                "targetIsPort": true,
                "currentTargetIsPort": true,
                "dataIsNull": true,
                "portCount": 0,
                "bubbles": false,
                "cancelable": false,
                "value": null,
            }],
            "eventStateReset": true,
            "provisionalBridgeRemoved": true,
            "retainedBridgePreserved": true,
            "registrySizesStable": true,
            "portsOpen": true,
        })
    );

    assert_eq!(
        engine
            .evaluate_async("receivingPort.postMessage({ value: 41 }); localChannel.port2.postMessage({ value: 8 }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut recovered = serde_json::Value::Null;
    for _ in 0..8 {
        recovered = engine
            .evaluate_async("portEvents.length === 2 && localPortEvents.length === 2 ? ({ bridge: portEvents[1], local: localPortEvents[1] }) : null")
            .await
            .unwrap();
        if !recovered.is_null() {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        recovered,
        serde_json::json!({
            "bridge": {
                "kind": "message",
                "type": "message",
                "isMessageEvent": true,
                "targetIsPort": true,
                "currentTargetIsPort": true,
                "dataIsNull": false,
                "portCount": 0,
                "bubbles": false,
                "cancelable": false,
                "value": 42,
            },
            "local": {
                "kind": "message",
                "type": "message",
                "isMessageEvent": true,
                "targetIsPort": true,
                "currentTargetIsPort": true,
                "dataIsNull": false,
                "portCount": 0,
                "bubbles": false,
                "cancelable": false,
                "value": 8,
            },
        })
    );

    assert_eq!(
        engine
            .evaluate_async(
                r#"globalThis.initialBridgeKey = receivingPort.__glassMessagePortBridgeKey;
globalThis.initialPortId = receivingPort.__glassMessagePortId;
receivingPort.close();
receivingPort.close();
let postCloseError = null;
try { receivingPort.postMessage({ value: 'closed' }); }
catch (error) { postCloseError = error.name; }
({
  initiatorClosed: receivingPort.__glassMessageClosed,
  initiatorBridgeKeyCleared: receivingPort.__glassMessagePortBridgeKey === null,
  initiatorMapEntryRemoved: !__glassMessageBridgeRegistry.has(initialBridgeKey),
  initiatorIdEntryRemoved: !__glassMessagePortRegistry.has(initialPortId),
  initiatorCloseEvents: pageBridgeCloseState.records.length,
  postCloseError,
})"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "initiatorClosed": true,
            "initiatorBridgeKeyCleared": true,
            "initiatorMapEntryRemoved": true,
            "initiatorIdEntryRemoved": true,
            "initiatorCloseEvents": 0,
            "postCloseError": "InvalidStateError",
        })
    );

    let mut page_close_report = serde_json::Value::Null;
    for _ in 0..8 {
        page_close_report = engine
            .evaluate_async(
                "((workerEvents.find(event => event.data.kind === 'page-closed-worker-peer') || { data: {} }).data || {}).report || null",
            )
            .await
            .unwrap();
        if !page_close_report.is_null() {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        page_close_report,
        serde_json::json!({
            "records": [
                {
                    "kind": "handler", "sameEvent": true,
                    "type": "close", "isEvent": true,
                    "targetIsPort": true, "currentTargetIsPort": true,
                    "eventPhase": 2, "thisIsPort": true,
                    "bubbles": false, "cancelable": false,
                },
                {
                    "kind": "listener", "sameEvent": true,
                    "type": "close", "isEvent": true,
                    "targetIsPort": true, "currentTargetIsPort": true,
                    "eventPhase": 2, "thisIsPort": true,
                    "bubbles": false, "cancelable": false,
                },
            ],
            "eventStateReset": true,
            "portOpen": true,
            "bridgeKeyCleared": true,
            "peerCleared": true,
        })
    );
    assert_eq!(
        engine.evaluate_async("portEvents.length").await.unwrap(),
        serde_json::json!(2),
        "the closed bridge must not deliver post-close messages"
    );

    assert_eq!(
        engine
            .evaluate_async("worker.postMessage({ kind: 'make-reverse' }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut reverse_port_ready = false;
    for _ in 0..8 {
        reverse_port_ready = engine
            .evaluate_async("reversePortReady")
            .await
            .unwrap()
            .as_bool()
            .unwrap_or(false);
        if reverse_port_ready {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert!(
        reverse_port_ready,
        "Worker should transfer the reverse endpoint"
    );

    assert_eq!(
        engine
            .evaluate_async("worker.postMessage({ kind: 'close-reverse' }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut reverse_close_state = serde_json::Value::Null;
    for _ in 0..8 {
        reverse_close_state = engine
            .evaluate_async(
                "({ pageRecords: reverseBridgeCloseState.records, pageEventStateReset: closeStateReset(reverseBridgeCloseState), pagePeerOpen: !reverseReceivingPort.__glassMessageClosed, pageBridgeKeyCleared: reverseReceivingPort.__glassMessagePortBridgeKey === null, pageBridgeMapEntryRemoved: !__glassMessageBridgeRegistry.has(reverseBridgeKey), unexpectedMessages: portEvents.filter(event => event.kind === 'unexpected-reverse-message'), workerInitiatorReport: ((workerEvents.find(event => event.data.kind === 'worker-closed-initiator') || { data: {} }).data || {}).report || null })",
            )
            .await
            .unwrap();
        if reverse_close_state["pageRecords"]
            .as_array()
            .map_or(0, Vec::len)
            == 2
            && !reverse_close_state["workerInitiatorReport"].is_null()
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        reverse_close_state,
        serde_json::json!({
            "pageRecords": [
                {
                    "kind": "handler", "sameEvent": true,
                    "type": "close", "isEvent": true,
                    "targetIsPort": true, "currentTargetIsPort": true,
                    "eventPhase": 2, "thisIsPort": true,
                    "bubbles": false, "cancelable": false,
                },
                {
                    "kind": "listener", "sameEvent": true,
                    "type": "close", "isEvent": true,
                    "targetIsPort": true, "currentTargetIsPort": true,
                    "eventPhase": 2, "thisIsPort": true,
                    "bubbles": false, "cancelable": false,
                },
            ],
            "pageEventStateReset": true,
            "pagePeerOpen": true,
            "pageBridgeKeyCleared": true,
            "pageBridgeMapEntryRemoved": true,
            "unexpectedMessages": [],
            "workerInitiatorReport": {
                "records": [],
                "eventStateReset": true,
                "portOpen": false,
                "bridgeKeyCleared": true,
                "peerCleared": true,
            },
        })
    );

    engine
        .evaluate_async("independentChannel.port2.postMessage({ value: 73 }); true")
        .await
        .unwrap();
    let mut independent_messages = serde_json::Value::Null;
    for _ in 0..8 {
        independent_messages = engine.evaluate_async("independentMessages").await.unwrap();
        if independent_messages
            .as_array()
            .is_some_and(|messages| !messages.is_empty())
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(independent_messages, serde_json::json!([73]));

    assert_eq!(
        engine
            .evaluate_async(
                r#"localPort.close();
localPort.close();
localChannel.port2.postMessage({ value: 999 });
({
  localCloseEvents,
  closeHandlerReadback: typeof localChannel.port2.onclose === 'function',
  closeEventStateReset: currentLocalCloseEvent.currentTarget === null
    && currentLocalCloseEvent.eventPhase === 0,
  initiatorClosed: localPort.__glassMessageClosed,
  peerOpen: !localChannel.port2.__glassMessageClosed,
  pairDetached: localPort.__glassMessagePortPeer === null
    && localChannel.port2.__glassMessagePortPeer === null,
  initiatorCloseCount,
  noPostCloseDelivery: localPortEvents.length === 2,
})"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "localCloseEvents": [
                {
                    "kind": "handler", "sameEvent": true,
                    "type": "close", "isEvent": true,
                    "targetIsPeer": true, "currentTargetIsPeer": true,
                    "eventPhase": 2, "thisIsPeer": true,
                    "bubbles": false, "cancelable": false,
                },
                {
                    "kind": "listener", "sameEvent": true,
                    "type": "close", "isEvent": true,
                    "targetIsPeer": true, "currentTargetIsPeer": true,
                    "eventPhase": 2, "thisIsPeer": true,
                    "bubbles": false, "cancelable": false,
                },
            ],
            "closeHandlerReadback": true,
            "closeEventStateReset": true,
            "initiatorClosed": true,
            "peerOpen": true,
            "pairDetached": true,
            "initiatorCloseCount": 0,
            "noPostCloseDelivery": true,
        })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_http_post_message_transfers_blob_urls_between_content_processes() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/blob-message-parent" => {
                    "<title>HTTP Blob message parent</title><script>globalThis.received = []; addEventListener('message', event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); received.push([event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]); } catch (error) { received.push(['error', String(error)]); } }); globalThis.popup = window.open('/blob-message-child', 'http-blob-message-child'); const blob = new Blob(['http parent to child'], { type: 'text/http-parent' }); popup.postMessage({ kind: 'http-parent', url: URL.createObjectURL(blob) }, '*');</script><p>parent</p>"
                }
                "/blob-message-child" => {
                    "<title>HTTP Blob message child</title><script>globalThis.received = null; addEventListener('message', event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); globalThis.received = [event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]; const blob = new Blob(['http child to parent'], { type: 'text/http-child' }); event.source.postMessage({ kind: 'http-child', url: URL.createObjectURL(blob) }, '*'); } catch (error) { globalThis.received = ['error', String(error)]; } });</script><p>child</p>"
                }
                other => panic!("unexpected postMessage request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/blob-message-parent")),
    )
    .await
    .unwrap();
    let child = session
        .native_list_targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| !target.active)
        .unwrap();
    session.native_select_target(&child.id).await.unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([
            "http-parent",
            "http parent to child",
            "text/http-parent",
            format!("http://{address}")
        ])
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([[
            "http-child",
            "http child to parent",
            "text/http-child",
            format!("http://{address}")
        ]])
    );

    session.close().await.unwrap();
    server.await.unwrap();
        });
    });
}
#[tokio::test]
async fn native_content_process_delivers_report_only_csp_violation_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/csp-report-only"));
        let body = r#"<head><script>globalThis.cspReports = []; document.addEventListener('securitypolicyviolation', event => cspReports.push({ type: event.type, directive: event.effectiveDirective, alias: event.violatedDirective, disposition: event.disposition, blocked: event.blockedURI, sample: event.sample, policy: event.originalPolicy, bubbles: event.bubbles }));</script><script>globalThis.reportOnlyScript = true;</script><style>body { color: red; }</style></head><body>Report only</body>"#;
        let policy = "script-src 'none'; style-src 'none'";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'; style-src 'unsafe-inline'\r\nContent-Security-Policy-Report-Only: {policy}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/csp-report-only")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("({ script: reportOnlyScript, reports: cspReports })")
            .await
            .unwrap(),
        serde_json::json!({
            "script": true,
            "reports": [
                {
                    "type": "securitypolicyviolation",
                    "directive": "style-src",
                    "alias": "style-src",
                    "disposition": "report",
                    "blocked": "inline",
                    "sample": "body { color: red; }",
                    "policy": "script-src 'none'; style-src 'none'",
                    "bubbles": true,
                },
                {
                    "type": "securitypolicyviolation",
                    "directive": "script-src",
                    "alias": "script-src",
                    "disposition": "report",
                    "blocked": "inline",
                    "sample": "globalThis.cspReports = []; document.add",
                    "policy": "script-src 'none'; style-src 'none'",
                    "bubbles": true,
                },
                {
                    "type": "securitypolicyviolation",
                    "directive": "script-src",
                    "alias": "script-src",
                    "disposition": "report",
                    "blocked": "inline",
                    "sample": "globalThis.reportOnlyScript = true;",
                    "policy": "script-src 'none'; style-src 'none'",
                    "bubbles": true,
                },
            ],
        })
    );
    engine
        .evaluate_async("document.querySelector('style').textContent = 'body { color: blue; }'")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "cspReports.filter(event => event.blocked === 'inline').map(event => event.sample)"
            )
            .await
            .unwrap(),
        serde_json::json!([
            "body { color: red; }",
            "globalThis.cspReports = []; document.add",
            "globalThis.reportOnlyScript = true;",
            "body { color: blue; }",
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_delivers_report_only_events_for_dynamic_scripts() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/csp-dynamic-report")
        );
        let body = r#"<script>globalThis.cspReports = []; addEventListener('securitypolicyviolation', event => cspReports.push([event.effectiveDirective, event.blockedURI, event.sample]));</script><body>Dynamic report only</body>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'\r\nContent-Security-Policy-Report-Only: script-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/csp-dynamic-report")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const script = document.createElement('script'); script.textContent = 'globalThis.dynamicReportScript = true'; document.body.appendChild(script); return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({ ran: dynamicReportScript, reports: cspReports })")
            .await
            .unwrap(),
        serde_json::json!({
            "ran": true,
            "reports": [
                ["script-src", "inline", "globalThis.cspReports = []; addEventList"],
                ["script-src", "inline", "globalThis.dynamicReportScript = true"],
            ],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_delayed_timer_waits_for_due_host_turn() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<script>globalThis.taskOrder = 'script'; setTimeout(() => { globalThis.taskOrder += '-timer'; }, 2000);</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.taskOrder").await.unwrap(),
        serde_json::json!("script")
    );
    tokio::time::sleep(Duration::from_millis(2020)).await;
    assert_eq!(
        engine.evaluate_async("globalThis.taskOrder").await.unwrap(),
        serde_json::json!("script-timer")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_dispatches_resource_load_events_before_dom_content_loaded() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/style.css", "/app.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = match expected_path {
                "/page" => (
                    "text/html",
                    "<link id='theme' rel='stylesheet' href='/style.css'><script src='/app.js'></script><script>globalThis.resourceEvents = []; document.getElementById('theme').addEventListener('load', () => resourceEvents.push('style:' + document.readyState)); document.getElementsByTagName('script')[0].addEventListener('load', () => resourceEvents.push('script:' + document.readyState)); document.addEventListener('DOMContentLoaded', () => resourceEvents.push('dom:' + document.readyState));</script>",
                ),
                "/style.css" => ("text/css", "body { color: red; }"),
                _ => ("application/javascript", "globalThis.appLoaded = true;"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ appLoaded: globalThis.appLoaded, events: globalThis.resourceEvents })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "appLoaded": true,
            "events": ["style:loading", "script:loading", "dom:interactive"]
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_upload_bridges_file_objects_and_dom_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let body = "<form id='form'><input id='upload' type='file' name='asset' multiple></form><script>globalThis.uploadEvents = []; const input = document.getElementById('upload'); input.addEventListener('input', event => uploadEvents.push(event.type + ':' + event.target.id)); input.addEventListener('change', event => uploadEvents.push(event.type + ':' + event.target.id));</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/upload")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let uploaded = engine
        .action_async(NativeAction::Upload {
            target: "id=upload".into(),
            files: vec![NativeFile {
                name: "worker.txt".into(),
                media_type: "text/plain".into(),
                last_modified: 5678,
                bytes: b"worker upload".to_vec(),
            }],
        })
        .await
        .unwrap();
    assert!(uploaded.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const input = document.getElementById('upload'); const file = input.files[0]; const formFile = new FormData(document.getElementById('form')).get('asset'); return { length: input.files.length, name: file.name, type: file.type, size: file.size, modified: file.lastModified, text: await file.text(), formFile: [formFile.name, await formFile.text()], value: input.value, events: globalThis.uploadEvents }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "length": 1,
            "name": "worker.txt",
            "type": "text/plain",
            "size": 13,
            "modified": 5678,
            "text": "worker upload",
            "formFile": ["worker.txt", "worker upload"],
            "value": r"C:\fakepath\worker.txt",
            "events": ["input:upload", "change:upload"],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_drives_timer_await_and_publishes_mutation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<input id='result' value='pending'>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await new Promise(resolve => setTimeout(() => { document.getElementById('result').value = 'timer-done'; resolve('timer-done'); }, 5))",
            )
            .await
            .unwrap(),
        serde_json::json!("timer-done")
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('result').value")
            .await
            .unwrap(),
        serde_json::json!("timer-done")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

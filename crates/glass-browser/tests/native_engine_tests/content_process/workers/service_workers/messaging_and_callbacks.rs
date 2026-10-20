use super::*;

#[tokio::test]
async fn native_content_process_service_worker_transfers_message_port_round_trip() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/page",
                "text/html",
                "<script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' }); globalThis.swEvents = [];</script><main>service worker message port</main>",
            ),
            (
                "/sw.js",
                "application/javascript",
                "self.addEventListener('message', event => { const port = event.ports[0]; if (!port) return; port.onmessage = message => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', message.data.url, false); xhr.send(); const blob = new Blob(['sw-port-body'], { type: 'text/sw-port' }); port.postMessage({ kind: 'reply', value: Number(message.data.value) + 1, sourceText: xhr.responseText, sourceType: xhr.getResponseHeader('content-type'), url: URL.createObjectURL(blob) }); } catch (error) { port.postMessage({ kind: 'error', error: String(error) }); } }; port.start(); port.postMessage({ kind: 'ready', value: 1, eventPortsFrozen: Object.isFrozen(event.ports) }); });",
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
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
                "await registrationPromise.then(reg => { globalThis.channel = new MessageChannel(); channel.port2.onmessage = event => { const result = { kind: event.data.kind, value: event.data.value, portCount: event.ports.length }; if (event.data.eventPortsFrozen !== undefined) result.eventPortsFrozen = event.data.eventPortsFrozen; if (event.data.url) { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); result.responseText = xhr.responseText; result.responseType = xhr.getResponseHeader('content-type'); } catch (error) { result.error = String(error); } } if (event.data.sourceText) { result.sourceText = event.data.sourceText; result.sourceType = event.data.sourceType; } swEvents.push(result); }; channel.port2.start(); reg.active.postMessage({ kind: 'connect' }, [channel.port1]); let detachedError = ''; try { channel.port1.postMessage({}); } catch (error) { detachedError = error.name; } return { detachedError, port2: channel.port2 instanceof MessagePort }; })",
            )
            .await
            .unwrap(),
        serde_json::json!({"detachedError": "InvalidStateError", "port2": true})
    );
    assert_eq!(
        engine.evaluate_async("({ swEvents })").await.unwrap(),
        serde_json::json!({
            "swEvents": [{"kind": "ready", "value": 1, "portCount": 0, "eventPortsFrozen": true}],
        })
    );
    assert_eq!(
        engine
            .evaluate_async("const blob = new Blob(['page-sw-port-body'], { type: 'text/page-sw-port' }); channel.port2.postMessage({ value: 4, url: URL.createObjectURL(blob) }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("({ swEvents })").await.unwrap(),
        serde_json::json!({
            "swEvents": [
                {"kind": "ready", "value": 1, "portCount": 0, "eventPortsFrozen": true},
                {
                    "kind": "reply",
                    "value": 5,
                    "portCount": 0,
                    "responseText": "sw-port-body",
                    "responseType": "text/sw-port",
                    "sourceText": "page-sw-port-body",
                    "sourceType": "text/page-sw-port",
                },
            ],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_message_port_bridge_close_both_directions() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.pageMessages = [];
globalThis.pageCloseTrace = [];
globalThis.pageCloseEvents = Object.create(null);
globalThis.pageCloseReports = [];
globalThis.pagePorts = Object.create(null);
globalThis.pagePortCloseLast = Object.create(null);
globalThis.pageInitiatorPostError = null;
globalThis.pageRemotePostError = null;
navigator.serviceWorker.addEventListener('message', event => {
  if (event.data && event.data.kind === 'close-report') pageCloseReports.push(event.data);
});
</script><main>Service Worker MessagePort bridge close</main>"#;
    let worker_script = r#"
const ports = new Map();
const states = new Map();
const closeSnapshot = (phase, event, port, receiverThis, previous) => ({
  phase,
  sameEvent: previous === event,
  type: event.type,
  genericEvent: event instanceof Event && !(event instanceof MessageEvent),
  targetIsPort: event.target === port,
  currentTargetIsPort: event.currentTarget === port,
  eventPhase: event.eventPhase,
  bubbles: event.bubbles,
  cancelable: event.cancelable,
  thisIsPort: receiverThis === port,
});
self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('message', event => {
  if (!event.ports.length) {
    if (event.data && event.data.kind === 'close-report') {
      const state = states.get(event.data.id);
      event.source.postMessage({
        kind: 'close-report',
        id: event.data.id,
        trace: state ? state.trace : [],
        eventStateReset: !state || !state.lastClose
          || (state.lastClose.currentTarget === null && state.lastClose.eventPhase === 0),
        postCloseError: state ? state.postCloseError : null,
        postAfterRemoteCloseError: state ? state.postAfterRemoteCloseError : null,
      });
    }
    return;
  }
  const id = event.data.id;
  const port = event.ports[0];
  const state = {
    trace: [], lastClose: null, postCloseError: null,
    postAfterRemoteCloseError: null,
  };
  ports.set(id, port);
  states.set(id, state);
  port.onclose = function(closeEvent) {
    state.lastClose = closeEvent;
    state.trace.push(closeSnapshot('handler', closeEvent, port, this, closeEvent));
    try { port.postMessage({ kind: 'must-not-cross-after-peer-close', id }); }
    catch (error) { state.postAfterRemoteCloseError = error.name; }
  };
  port.addEventListener('close', function(closeEvent) {
    state.trace.push(closeSnapshot('listener', closeEvent, port, this, state.lastClose));
  });
  port.addEventListener('message', event => {
    if (event.data.kind === 'close-worker') {
      port.postMessage({ kind: 'queued-before-close', id });
      port.close();
      port.close();
      try { port.postMessage({ kind: 'must-not-cross-after-local-close', id }); }
      catch (error) { state.postCloseError = error.name; }
    } else if (event.data.kind === 'ping') {
      port.postMessage({ kind: 'pong', id });
    }
  });
  port.start();
  port.postMessage({ kind: 'ready', id });
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/service-port-close-page", "text/html", page),
            (
                "/service-port-close-sw.js",
                "application/javascript",
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
            .with_initial_url(format!("http://{address}/service-port-close-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
  const registration = await navigator.serviceWorker.register('/service-port-close-sw.js', { scope: '/' });
  globalThis.activeCloseWorker = registration.active;
  const snapshot = (id, phase, event, port, receiverThis, previous) => ({
    id,
    phase,
    sameEvent: previous === event,
    type: event.type,
    genericEvent: event instanceof Event && !(event instanceof MessageEvent),
    targetIsPort: event.target === port,
    currentTargetIsPort: event.currentTarget === port,
    eventPhase: event.eventPhase,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
    thisIsPort: receiverThis === port,
  });
  for (const id of ['page-closes', 'worker-closes', 'unrelated']) {
    const channel = new MessageChannel();
    const port = channel.port2;
    pagePorts[id] = port;
    port.onmessage = event => pageMessages.push(event.data);
    port.onclose = function(event) {
      pageCloseEvents[id] = event;
      pageCloseTrace.push(snapshot(id, 'handler', event, port, this, event));
    };
    port.addEventListener('close', function(event) {
      pageCloseTrace.push(snapshot(id, 'listener', event, port, this, pageCloseEvents[id]));
    });
    port.start();
    registration.active.postMessage({ id }, [channel.port1]);
  }
  return registration.active.state;
})()"#,
            )
            .await
            .unwrap(),
        serde_json::json!("activated")
    );

    let mut ready_messages = serde_json::Value::Null;
    for _ in 0..8 {
        ready_messages = engine
            .evaluate_async(
                "pageMessages.filter(message => message.kind === 'ready').map(message => message.id)",
            )
            .await
            .unwrap();
        if ready_messages
            .as_array()
            .is_some_and(|messages| messages.len() == 3)
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        ready_messages,
        serde_json::json!(["page-closes", "worker-closes", "unrelated"])
    );

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
  const initiator = pagePorts['page-closes'];
  initiator.close();
  initiator.close();
  try { initiator.postMessage({ kind: 'must-not-cross-after-local-close' }); }
  catch (error) { pageInitiatorPostError = error.name; }
  pagePorts['worker-closes'].postMessage({ kind: 'close-worker' });
  pagePorts.unrelated.postMessage({ kind: 'ping' });
  return true;
})()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );

    let mut post_close_state = serde_json::Value::Null;
    for _ in 0..8 {
        post_close_state = engine
            .evaluate_async(
                "({ pageCloseTrace, pageMessages, pageInitiatorPostError, workerCloseSeen: pageCloseTrace.some(event => event.phase === 'handler' && event.targetIsPort) })",
            )
            .await
            .unwrap();
        if post_close_state["pageMessages"]
            .as_array()
            .is_some_and(|messages| messages.iter().any(|message| message["kind"] == "pong"))
            && post_close_state["workerCloseSeen"] == true
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        post_close_state["pageInitiatorPostError"],
        "InvalidStateError"
    );
    assert_eq!(
        post_close_state["pageCloseTrace"].as_array().unwrap().len(),
        2
    );
    assert!(
        post_close_state["pageMessages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["kind"] == "pong" && message["id"] == "unrelated")
    );
    assert!(
        !post_close_state["pageMessages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["kind"] == "queued-before-close"
                || message["kind"] == "must-not-cross-after-peer-close"
                || message["kind"] == "must-not-cross-after-local-close")
    );

    engine
        .evaluate_async(
            "activeCloseWorker.postMessage({ kind: 'close-report', id: 'page-closes' }); activeCloseWorker.postMessage({ kind: 'close-report', id: 'worker-closes' }); true",
        )
        .await
        .unwrap();
    let mut close_reports = serde_json::Value::Null;
    for _ in 0..8 {
        close_reports = engine
            .evaluate_async("pageCloseReports.slice().sort((a, b) => a.id.localeCompare(b.id))")
            .await
            .unwrap();
        if close_reports
            .as_array()
            .is_some_and(|reports| reports.len() == 2)
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }

    assert_eq!(
        close_reports,
        serde_json::json!([
            {
                "kind": "close-report",
                "id": "page-closes",
                "trace": [
                    { "phase": "handler", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
                    { "phase": "listener", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
                ],
                "eventStateReset": true,
                "postCloseError": null,
                "postAfterRemoteCloseError": null,
            },
            {
                "kind": "close-report",
                "id": "worker-closes",
                "trace": [],
                "eventStateReset": true,
                "postCloseError": "InvalidStateError",
                "postAfterRemoteCloseError": null,
            },
        ])
    );

    assert_eq!(
        engine
            .evaluate_async(
                "({ pageCloseTrace, pageCloseStateReset: pageCloseEvents['worker-closes'].currentTarget === null && pageCloseEvents['worker-closes'].eventPhase === 0, remotePostError: (() => { try { pagePorts['worker-closes'].postMessage({ kind: 'must-not-cross-after-remote-close' }); return null; } catch (error) { return error.name; } })(), messages: pageMessages })",
            )
            .await
            .unwrap()["pageCloseStateReset"],
        serde_json::json!(true)
    );
    let final_state = engine
        .evaluate_async(
            "({ pageCloseTrace, remotePostError: (() => { try { pagePorts['worker-closes'].postMessage({ kind: 'must-not-cross-after-remote-close' }); return null; } catch (error) { return error.name; } })(), messages: pageMessages })",
        )
        .await
        .unwrap();
    assert_eq!(final_state["remotePostError"], serde_json::Value::Null);
    assert_eq!(
        final_state["pageCloseTrace"],
        serde_json::json!([
            { "id": "worker-closes", "phase": "handler", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
            { "id": "worker-closes", "phase": "listener", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
        ])
    );
    assert!(
        !final_state["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["kind"] == "must-not-cross-after-remote-close")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_client_messageerror_recovers() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.clientMessageErrors = [];
globalThis.clientMessages = [];
globalThis.currentClientMessageError = null;
globalThis.clientErrorHandlerThisIsContainer = false;
globalThis.registrationPromise = navigator.serviceWorker.register('/client-messageerror-sw.js', { scope: '/' });
globalThis.registrationPromise.then(registration => globalThis.clientRegistration = registration);
const observeMessageError = (kind, event) => {
  if (currentClientMessageError === null) currentClientMessageError = event;
  clientMessageErrors.push({
    kind,
    sameEvent: currentClientMessageError === event,
    type: event.type,
    isMessageEvent: event instanceof MessageEvent,
    targetIsContainer: event.target === navigator.serviceWorker,
    currentTargetIsContainer: event.currentTarget === navigator.serviceWorker,
    eventPhase: event.eventPhase,
    dataIsNull: event.data === null,
    portCount: event.ports.length,
    origin: event.origin,
    sourceIsActive: event.source === clientRegistration.active,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
  });
};
navigator.serviceWorker.addEventListener('messageerror', event => observeMessageError('listener', event));
navigator.serviceWorker.onmessageerror = function(event) {
  clientErrorHandlerThisIsContainer = this === navigator.serviceWorker;
  observeMessageError('handler', event);
};
navigator.serviceWorker.addEventListener('message', event => clientMessages.push({
  type: event.type,
  isMessageEvent: event instanceof MessageEvent,
  targetIsContainer: event.target === navigator.serviceWorker,
  currentTargetIsContainer: event.currentTarget === navigator.serviceWorker,
  eventPhase: event.eventPhase,
  origin: event.origin,
  sourceIsActive: event.source === clientRegistration.active,
  data: event.data,
}));
</script><main>Service Worker client messageerror</main>"#;
    let worker_script = r#"
self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('message', event => {
  event.source.postMessage({ kind: 'valid', value: event.data.value + 1 });
});
"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/client-messageerror-page", "text/html", page),
            (
                "/client-messageerror-sw.js",
                "application/javascript",
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
            .with_initial_url(format!("http://{address}/client-messageerror-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"await registrationPromise;
const malformed = {
  data: {
    __glassMessageClone: 'glass-native-structured-clone-v1',
    root: { ref: 0 },
    nodes: [{ type: 'invalid-node' }],
  },
  transfer_ports: [],
  object_urls: [],
};
__glassDispatchServiceWorkerClientMessage(malformed);
await Promise.resolve();
({
  clientMessageErrors,
  clientMessages,
  handlerReadback: typeof navigator.serviceWorker.onmessageerror === 'function',
  clientErrorHandlerThisIsContainer,
  eventStateReset: currentClientMessageError.currentTarget === null
    && currentClientMessageError.eventPhase === 0,
})"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "clientMessageErrors": [
                {
                    "kind": "listener", "sameEvent": true,
                    "type": "messageerror", "isMessageEvent": true,
                    "targetIsContainer": true, "currentTargetIsContainer": true,
                    "eventPhase": 2, "dataIsNull": true, "portCount": 0,
                    "origin": format!("http://{address}"), "sourceIsActive": true,
                    "bubbles": false, "cancelable": false,
                },
                {
                    "kind": "handler", "sameEvent": true,
                    "type": "messageerror", "isMessageEvent": true,
                    "targetIsContainer": true, "currentTargetIsContainer": true,
                    "eventPhase": 2, "dataIsNull": true, "portCount": 0,
                    "origin": format!("http://{address}"), "sourceIsActive": true,
                    "bubbles": false, "cancelable": false,
                },
            ],
            "clientMessages": [],
            "handlerReadback": true,
            "clientErrorHandlerThisIsContainer": true,
            "eventStateReset": true,
        })
    );

    assert_eq!(
        engine
            .evaluate_async("clientRegistration.active.postMessage({ value: 41 }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut valid_message = serde_json::Value::Null;
    for _ in 0..8 {
        valid_message = engine
            .evaluate_async("clientMessages.length > 0 ? clientMessages[0] : null")
            .await
            .unwrap();
        if !valid_message.is_null() {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        valid_message,
        serde_json::json!({
            "type": "message",
            "isMessageEvent": true,
            "targetIsContainer": true,
            "currentTargetIsContainer": true,
            "eventPhase": 2,
            "origin": format!("http://{address}"),
            "sourceIsActive": true,
            "data": { "kind": "valid", "value": 42 },
        })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_message_port_callback_errors_stay_global() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.swEvents = [];
globalThis.clientErrors = [];
navigator.serviceWorker.addEventListener('error', event => clientErrors.push(['container', event.message]));
globalThis.registrationPromise = navigator.serviceWorker.register('/sw-port-errors.js', { scope: '/' });
</script><main>service worker MessagePort callback errors</main>"#;
    let worker_script = r#"const reports = [];
const callbackTrace = [];
let messageReportStart = 0;
let messageTraceStart = 0;
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'handled service port error';
  reports.push({ phase: 'onerror', message, cancel, errorIsError: error instanceof Error });
  callbackTrace.push('global:onerror:' + message);
  return cancel;
};
addEventListener('error', event => {
  reports.push({
    phase: 'error-listener', message: event.message, defaultPrevented: event.defaultPrevented,
    cancelable: event.cancelable, targetIsGlobal: event.target === self,
    currentTargetIsGlobal: event.currentTarget === self,
  });
  callbackTrace.push('global:error:' + event.message);
});
addEventListener('message', event => {
  const port = event.ports[0];
  if (!port) return;
  port.onmessage = message => {
    const kind = message.data.kind;
    messageReportStart = reports.length;
    messageTraceStart = callbackTrace.length;
    callbackTrace.push('port:onmessage:' + kind);
    if (kind === 'handled') throw new Error('handled service port error');
  };
  port.addEventListener('message', message => {
    const kind = message.data.kind;
    callbackTrace.push('port:listener-one:' + kind);
    if (kind === 'unhandled') throw 'unhandled service port error';
  });
  port.addEventListener('message', message => {
    const kind = message.data.kind;
    callbackTrace.push('port:listener-two:' + kind);
    port.postMessage({
      kind: 'reply', request: kind,
      trace: callbackTrace.slice(messageTraceStart),
      reports: reports.slice(messageReportStart),
    });
  });
  port.start();
  port.postMessage({ kind: 'ready' });
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/service-port-error-page", "text/html", page),
            (
                "/sw-port-errors.js",
                "application/javascript",
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
            .with_initial_url(format!("http://{address}/service-port-error-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => { globalThis.activeServiceWorker = reg.active; reg.active.addEventListener('error', event => clientErrors.push(['worker', event.message])); globalThis.channel = new MessageChannel(); channel.port2.onmessage = event => swEvents.push(event.data); channel.port2.start(); reg.active.postMessage({ kind: 'connect' }, [channel.port1]); return reg.active.state; })",
            )
            .await
            .unwrap(),
        serde_json::json!("activated")
    );
    assert_eq!(
        engine
            .evaluate_async("({ swEvents, clientErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "swEvents": [{ "kind": "ready" }],
            "clientErrors": [],
        })
    );

    for (kind, trace, reports) in [
        (
            "handled",
            vec![
                "port:onmessage:handled",
                "global:onerror:handled service port error",
                "global:error:handled service port error",
                "port:listener-one:handled",
                "port:listener-two:handled",
            ],
            serde_json::json!([
                { "phase": "onerror", "message": "handled service port error", "cancel": true, "errorIsError": true },
                {
                    "phase": "error-listener", "message": "handled service port error",
                    "defaultPrevented": true, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ]),
        ),
        (
            "unhandled",
            vec![
                "port:onmessage:unhandled",
                "port:listener-one:unhandled",
                "global:onerror:unhandled service port error",
                "global:error:unhandled service port error",
                "port:listener-two:unhandled",
            ],
            serde_json::json!([
                { "phase": "onerror", "message": "unhandled service port error", "cancel": false, "errorIsError": false },
                {
                    "phase": "error-listener", "message": "unhandled service port error",
                    "defaultPrevented": false, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ]),
        ),
        (
            "alive",
            vec![
                "port:onmessage:alive",
                "port:listener-one:alive",
                "port:listener-two:alive",
            ],
            serde_json::json!([]),
        ),
    ] {
        assert_eq!(
            engine
                .evaluate_async(&format!(
                    "channel.port2.postMessage({{ kind: '{kind}' }}); true"
                ))
                .await
                .unwrap(),
            serde_json::json!(true),
            "page-to-Service-Worker port send for {kind}",
        );
        let page_state = engine
            .evaluate_async("({ swEvents, clientErrors, workerState: activeServiceWorker.state })")
            .await
            .unwrap();
        let service_worker_events = page_state["swEvents"].as_array().unwrap();
        assert_eq!(
            service_worker_events.last(),
            Some(&serde_json::json!({
                "kind": "reply", "request": kind, "trace": trace, "reports": reports,
            })),
            "reply after Service Worker MessagePort callback {kind}",
        );
        assert_eq!(page_state["clientErrors"], serde_json::json!([]));
        assert_eq!(page_state["workerState"], serde_json::json!("activated"));
    }
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_message_callback_errors_stay_global() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.swEvents = [];
globalThis.clientErrors = [];
navigator.serviceWorker.addEventListener('message', event => swEvents.push(event.data));
navigator.serviceWorker.addEventListener('error', event => clientErrors.push(['container', event.message]));
globalThis.registrationPromise = navigator.serviceWorker.register('/sw-global-message-errors.js', { scope: '/' });
</script><main>service worker global message errors</main>"#;
    let worker_script = r#"const trace = [];
const reports = [];
let traceStart = 0;
let reportStart = 0;
const messageErrorTrace = [];
let currentMessageErrorEvent = null;
let messageErrorResult = null;
let messageErrorHandlerThisIsGlobal = false;
let messageErrorReactivatedThisIsGlobal = false;
let reassignMessageErrorDuringDispatch = false;
let snapshotReactivationHandler = null;
const malformedMessageErrorEnvelope = {
  data: {
    __glassMessageClone: 'glass-native-structured-clone-v1',
    root: { ref: 0 },
    nodes: [{ type: 'invalid-node' }],
  },
  transfer_ports: [],
};
const observeMessageError = (kind, event) => {
  if (currentMessageErrorEvent === null) currentMessageErrorEvent = event;
  messageErrorTrace.push({
    kind,
    sameEvent: currentMessageErrorEvent === event,
    targetIsGlobal: event.target === self,
    currentTargetIsGlobal: event.currentTarget === self,
    eventPhase: event.eventPhase,
    type: event.type,
    dataIsNull: event.data === null,
    portsLength: event.ports.length,
    origin: event.origin,
    sourceIsNull: event.source === null,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
  });
};
const messageErrorInitiallyNull = onmessageerror === null;
addEventListener('messageerror', event => {
  observeMessageError('listener-one', event);
  if (reassignMessageErrorDuringDispatch) {
    reassignMessageErrorDuringDispatch = false;
    onmessageerror = null;
    onmessageerror = snapshotReactivationHandler;
  }
});
const nonCallableMessageErrorHandler = {};
onmessageerror = nonCallableMessageErrorHandler;
const nonCallableMessageErrorHandlerRetained = onmessageerror === nonCallableMessageErrorHandler;
addEventListener('install', event => event.waitUntil(self.skipWaiting()));
addEventListener('activate', event => event.waitUntil(self.clients.claim()));
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'handled service worker message error';
  reports.push({ phase: 'onerror', message, cancel, errorIsError: error instanceof Error });
  trace.push('global:onerror:' + message);
  return cancel;
};
addEventListener('error', event => {
  reports.push({
    phase: 'error-listener', message: event.message, defaultPrevented: event.defaultPrevented,
    cancelable: event.cancelable, targetIsGlobal: event.target === self,
    currentTargetIsGlobal: event.currentTarget === self,
  });
  trace.push('global:error:' + event.message);
});
onmessage = event => {
  const kind = event.data.kind;
  traceStart = trace.length;
  reportStart = reports.length;
  trace.push('global:onmessage:' + kind);
  if (kind === 'handled') throw new Error('handled service worker message error');
  if (kind === 'decode') {
    self.__glassDispatchWorkerMessage(malformedMessageErrorEnvelope);
    const first = messageErrorTrace.splice(0);
    currentMessageErrorEvent = null;

    onmessageerror = 1;
    const primitiveAssignmentCleared = onmessageerror === null;
    onmessageerror = function oldMessageErrorHandler(event) {
      observeMessageError('replaced-handler', event);
    };
    addEventListener('messageerror', event => observeMessageError('listener-two', event));
    const replacementHandler = function messageErrorHandler(event) {
      messageErrorHandlerThisIsGlobal = this === self;
      observeMessageError('handler', event);
      throw new Error('messageerror callback failure');
    };
    onmessageerror = replacementHandler;
    const replacementReadback = onmessageerror === replacementHandler;
    self.__glassDispatchWorkerMessage(malformedMessageErrorEnvelope);
    const second = messageErrorTrace.splice(0);
    currentMessageErrorEvent = null;

    onmessageerror = null;
    const nullAssignmentCleared = onmessageerror === null;
    const reactivatedHandler = function reactivatedMessageErrorHandler(event) {
      messageErrorReactivatedThisIsGlobal = this === self;
      observeMessageError('reactivated-handler', event);
    };
    onmessageerror = reactivatedHandler;
    const reactivatedReadback = onmessageerror === reactivatedHandler;
    self.__glassDispatchWorkerMessage(malformedMessageErrorEnvelope);
    const third = messageErrorTrace.splice(0);
    currentMessageErrorEvent = null;
    snapshotReactivationHandler = function snapshotReactivatedMessageErrorHandler(event) {
      observeMessageError('snapshot-reactivated-handler', event);
    };
    reassignMessageErrorDuringDispatch = true;
    self.__glassDispatchWorkerMessage(malformedMessageErrorEnvelope);
    const fourth = messageErrorTrace.splice(0);
    currentMessageErrorEvent = null;
    messageErrorResult = {
      initiallyNull: messageErrorInitiallyNull,
      nonCallableRetained: nonCallableMessageErrorHandlerRetained,
      primitiveAssignmentCleared,
      replacementReadback,
      nullAssignmentCleared,
      reactivatedReadback,
      handlerThisIsGlobal: messageErrorHandlerThisIsGlobal,
      reactivatedThisIsGlobal: messageErrorReactivatedThisIsGlobal,
      snapshotReactivationReadback: onmessageerror === snapshotReactivationHandler,
      first,
      second,
      third,
      fourth,
    };
  }
};
addEventListener('message', event => {
  const kind = event.data.kind;
  trace.push('message:listener-one:' + kind);
  if (kind === 'unhandled') throw 'unhandled service worker message error';
});
addEventListener('message', async event => {
  const kind = event.data.kind;
  trace.push('message:listener-two:' + kind);
  const clients = await self.clients.matchAll({ includeUncontrolled: true, type: 'window' });
  const client = clients.find(candidate => candidate.url.endsWith('/service-global-message-errors'));
  const reply = {
    kind: 'reply', request: kind,
    trace: trace.slice(traceStart), reports: reports.slice(reportStart),
  };
  if (kind === 'decode') reply.messageError = messageErrorResult;
  client?.postMessage(reply);
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/service-global-message-errors", "text/html", page),
            (
                "/sw-global-message-errors.js",
                "application/javascript",
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
            .with_initial_url(format!("http://{address}/service-global-message-errors")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => { globalThis.activeServiceWorker = reg.active; reg.active.addEventListener('error', event => clientErrors.push(['service-worker', event.message])); return [reg.active.state, navigator.serviceWorker.controller !== null]; })",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );

    let message_error_observation = |kind| {
        serde_json::json!({
            "kind": kind, "sameEvent": true,
            "targetIsGlobal": true, "currentTargetIsGlobal": true,
            "eventPhase": 2, "type": "messageerror", "dataIsNull": true,
            "portsLength": 0, "origin": "", "sourceIsNull": true,
            "bubbles": false, "cancelable": false,
        })
    };
    for (kind, trace, reports) in [
        (
            "handled",
            vec![
                "global:onmessage:handled",
                "global:onerror:handled service worker message error",
                "global:error:handled service worker message error",
                "message:listener-one:handled",
                "message:listener-two:handled",
            ],
            serde_json::json!([
                { "phase": "onerror", "message": "handled service worker message error", "cancel": true, "errorIsError": true },
                {
                    "phase": "error-listener", "message": "handled service worker message error",
                    "defaultPrevented": true, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ]),
        ),
        (
            "unhandled",
            vec![
                "global:onmessage:unhandled",
                "message:listener-one:unhandled",
                "global:onerror:unhandled service worker message error",
                "global:error:unhandled service worker message error",
                "message:listener-two:unhandled",
            ],
            serde_json::json!([
                { "phase": "onerror", "message": "unhandled service worker message error", "cancel": false, "errorIsError": false },
                {
                    "phase": "error-listener", "message": "unhandled service worker message error",
                    "defaultPrevented": false, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ]),
        ),
        (
            "decode",
            vec![
                "global:onmessage:decode",
                "global:onerror:messageerror callback failure",
                "global:error:messageerror callback failure",
                "message:listener-one:decode",
                "message:listener-two:decode",
            ],
            serde_json::json!([
                { "phase": "onerror", "message": "messageerror callback failure", "cancel": false, "errorIsError": true },
                {
                    "phase": "error-listener", "message": "messageerror callback failure",
                    "defaultPrevented": false, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ]),
        ),
        (
            "alive",
            vec![
                "global:onmessage:alive",
                "message:listener-one:alive",
                "message:listener-two:alive",
            ],
            serde_json::json!([]),
        ),
    ] {
        assert_eq!(
            engine
                .evaluate_async(&format!(
                    "navigator.serviceWorker.controller.postMessage({{ kind: '{kind}' }}); true"
                ))
                .await
                .unwrap(),
            serde_json::json!(true),
        );
        let mut response = None;
        for _ in 0..8 {
            let page_state = engine
                .evaluate_async(
                    "({ swEvents, clientErrors, workerState: activeServiceWorker.state })",
                )
                .await
                .unwrap();
            response = page_state["swEvents"]
                .as_array()
                .and_then(|events| {
                    events
                        .iter()
                        .find(|event| event["request"] == serde_json::json!(kind))
                })
                .cloned();
            if response.is_some() {
                break;
            }
            engine
                .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
                .await
                .unwrap();
        }
        let response = response.expect("Service Worker client reply arrives in bounded page turns");
        let mut expected_response = serde_json::json!({
            "kind": "reply", "request": kind, "trace": trace, "reports": reports,
        });
        if kind == "decode" {
            expected_response["messageError"] = serde_json::json!({
                "initiallyNull": true,
                "nonCallableRetained": true,
                "primitiveAssignmentCleared": true,
                "replacementReadback": true,
                "nullAssignmentCleared": true,
                "reactivatedReadback": true,
                "handlerThisIsGlobal": true,
                "reactivatedThisIsGlobal": true,
                "snapshotReactivationReadback": true,
                "first": [message_error_observation("listener-one")],
                "second": [
                    message_error_observation("listener-one"),
                    message_error_observation("handler"),
                    message_error_observation("listener-two"),
                ],
                "third": [
                    message_error_observation("listener-one"),
                    message_error_observation("listener-two"),
                    message_error_observation("reactivated-handler"),
                ],
                "fourth": [message_error_observation("listener-one"),
                    message_error_observation("listener-two")],
            });
        }
        assert_eq!(
            response, expected_response,
            "Service Worker global callback recovery for {kind}",
        );
        let page_state = engine
            .evaluate_async("({ swEvents, clientErrors, workerState: activeServiceWorker.state })")
            .await
            .unwrap();
        assert_eq!(
            page_state["swEvents"].as_array().unwrap().last(),
            Some(&response)
        );
        assert_eq!(page_state["clientErrors"], serde_json::json!([]));
        assert_eq!(page_state["workerState"], serde_json::json!("activated"));
    }

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_fetch_callback_errors_continue_and_fallback() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.clientErrors = [];
navigator.serviceWorker.addEventListener('error', event => clientErrors.push(['container', event.message]));
globalThis.registrationPromise = navigator.serviceWorker.register('/sw-fetch-callback-errors.js', { scope: '/' });
</script><main>service worker fetch callback errors</main>"#;
    let worker_script = r#"const callbackTrace = [];
const reports = [];
let traceStart = 0;
let reportStart = 0;
addEventListener('install', event => event.waitUntil(self.skipWaiting()));
addEventListener('activate', event => event.waitUntil(self.clients.claim()));
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'handled service worker onfetch error';
  reports.push({ phase: 'onerror', message, cancel, errorIsError: error instanceof Error });
  callbackTrace.push('global:onerror:' + message);
  return cancel;
};
addEventListener('error', event => {
  reports.push({
    phase: 'error-listener', message: event.message, defaultPrevented: event.defaultPrevented,
    cancelable: event.cancelable, targetIsGlobal: event.target === self,
    currentTargetIsGlobal: event.currentTarget === self,
  });
  callbackTrace.push('global:error:' + event.message);
});
onfetch = event => {
  const path = new URL(event.request.url).pathname;
  traceStart = callbackTrace.length;
  reportStart = reports.length;
  callbackTrace.push('onfetch:' + path);
  if (path === '/handler-error') throw new Error('handled service worker onfetch error');
  if (path === '/network-fallback') throw 'unhandled service worker fallback error';
};
addEventListener('fetch', event => {
  const path = new URL(event.request.url).pathname;
  callbackTrace.push('listener-one:' + path);
  if (path === '/listener-error') throw 'unhandled service worker fetch listener error';
});
addEventListener('fetch', event => {
  const path = new URL(event.request.url).pathname;
  callbackTrace.push('listener-two:' + path);
  if (path === '/handler-error' || path === '/listener-error') {
    event.respondWith(new Response(JSON.stringify({
      trace: callbackTrace.slice(traceStart), reports: reports.slice(reportStart),
    }), { headers: { 'Content-Type': 'application/json' } }));
  } else if (path === '/healthy') {
    event.respondWith(new Response('healthy after callback failure'));
  }
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/service-fetch-callback-errors", "text/html", page),
            (
                "/sw-fetch-callback-errors.js",
                "application/javascript",
                worker_script,
            ),
            (
                "/network-fallback",
                "text/plain",
                "ordinary network fallback",
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
            .with_initial_url(format!("http://{address}/service-fetch-callback-errors")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => { globalThis.activeServiceWorker = reg.active; reg.active.addEventListener('error', event => clientErrors.push(['service-worker', event.message])); return [reg.active.state, navigator.serviceWorker.controller !== null]; })",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );

    assert_eq!(
        engine
            .evaluate_async("await fetch('/handler-error').then(response => response.json())")
            .await
            .unwrap(),
        serde_json::json!({
            "trace": [
                "onfetch:/handler-error",
                "global:onerror:handled service worker onfetch error",
                "global:error:handled service worker onfetch error",
                "listener-one:/handler-error",
                "listener-two:/handler-error",
            ],
            "reports": [
                { "phase": "onerror", "message": "handled service worker onfetch error", "cancel": true, "errorIsError": true },
                {
                    "phase": "error-listener", "message": "handled service worker onfetch error",
                    "defaultPrevented": true, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ],
        })
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/listener-error').then(response => response.json())")
            .await
            .unwrap(),
        serde_json::json!({
            "trace": [
                "onfetch:/listener-error",
                "listener-one:/listener-error",
                "global:onerror:unhandled service worker fetch listener error",
                "global:error:unhandled service worker fetch listener error",
                "listener-two:/listener-error",
            ],
            "reports": [
                { "phase": "onerror", "message": "unhandled service worker fetch listener error", "cancel": false, "errorIsError": false },
                {
                    "phase": "error-listener", "message": "unhandled service worker fetch listener error",
                    "defaultPrevented": false, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/network-fallback').then(async response => [response.status, await response.text()])",
            )
            .await
            .unwrap(),
        serde_json::json!([200, "ordinary network fallback"])
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/healthy').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("healthy after callback failure")
    );
    assert_eq!(
        engine
            .evaluate_async("({ clientErrors, workerState: activeServiceWorker.state })")
            .await
            .unwrap(),
        serde_json::json!({ "clientErrors": [], "workerState": "activated" })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_lifecycle_callback_errors_continue() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.lifecycleMessages = [];
globalThis.clientErrors = [];
navigator.serviceWorker.addEventListener('message', event => lifecycleMessages.push(event.data));
navigator.serviceWorker.addEventListener('error', event => clientErrors.push(['container', event.message]));
globalThis.registrationPromise = navigator.serviceWorker.register('/sw-lifecycle-callback-errors.js', { scope: '/' });
</script><main>service worker lifecycle callback errors</main>"#;
    let worker_script = r#"const callbackTrace = [];
const reports = [];
let traceStart = 0;
let reportStart = 0;
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'handled service worker install callback error';
  reports.push({ phase: 'onerror', message, cancel, errorIsError: error instanceof Error });
  callbackTrace.push('global:onerror:' + message);
  return cancel;
};
addEventListener('error', event => {
  reports.push({
    phase: 'error-listener', message: event.message, defaultPrevented: event.defaultPrevented,
    cancelable: event.cancelable, targetIsGlobal: event.target === self,
    currentTargetIsGlobal: event.currentTarget === self,
  });
  callbackTrace.push('global:error:' + event.message);
});
addEventListener('install', event => {
  traceStart = callbackTrace.length;
  reportStart = reports.length;
  callbackTrace.push('install:throwing');
  throw new Error('handled service worker install callback error');
});
addEventListener('install', event => {
  callbackTrace.push('install:later');
  event.waitUntil(self.skipWaiting());
  event.waitUntil(self.clients.matchAll({ includeUncontrolled: true, type: 'window' }).then(clients => {
    const client = clients.find(candidate => candidate.url.endsWith('/service-worker-lifecycle-callback-errors'));
    client?.postMessage({ phase: 'install', trace: callbackTrace.slice(traceStart), reports: reports.slice(reportStart) });
  }));
});
addEventListener('activate', event => {
  traceStart = callbackTrace.length;
  reportStart = reports.length;
  callbackTrace.push('activate:throwing');
  throw 'unhandled service worker activate callback error';
});
addEventListener('activate', event => {
  callbackTrace.push('activate:later');
  event.waitUntil(self.clients.claim().then(() => self.clients.matchAll({ includeUncontrolled: true, type: 'window' })).then(clients => {
    const client = clients.find(candidate => candidate.url.endsWith('/service-worker-lifecycle-callback-errors'));
    client?.postMessage({ phase: 'activate', trace: callbackTrace.slice(traceStart), reports: reports.slice(reportStart) });
  }));
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/service-worker-lifecycle-callback-errors",
                "text/html",
                page,
            ),
            (
                "/sw-lifecycle-callback-errors.js",
                "application/javascript",
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

    let mut engine = NativeEngine::new(NativeEngineConfig::default().with_initial_url(format!(
        "http://{address}/service-worker-lifecycle-callback-errors"
    )))
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => { globalThis.activeServiceWorker = reg.active; reg.active.addEventListener('error', event => clientErrors.push(['service-worker', event.message])); return [reg.active.state, navigator.serviceWorker.controller !== null]; })",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );

    for _ in 0..12 {
        let page_state = engine
            .evaluate_async(
                "({ lifecycleMessages, clientErrors, workerState: activeServiceWorker.state, controlled: navigator.serviceWorker.controller !== null })",
            )
            .await
            .unwrap();
        if page_state["lifecycleMessages"]
            .as_array()
            .is_some_and(|messages| {
                messages.iter().any(|message| message["phase"] == "install")
                    && messages
                        .iter()
                        .any(|message| message["phase"] == "activate")
            })
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    let page_state = engine
        .evaluate_async(
            "({ lifecycleMessages, clientErrors, workerState: activeServiceWorker.state, controlled: navigator.serviceWorker.controller !== null })",
        )
        .await
        .unwrap();
    assert_eq!(page_state["clientErrors"], serde_json::json!([]));
    assert_eq!(page_state["workerState"], serde_json::json!("activated"));
    assert_eq!(page_state["controlled"], serde_json::json!(true));
    assert_eq!(
        page_state["lifecycleMessages"],
        serde_json::json!([
            {
                "phase": "install",
                "trace": [
                    "install:throwing",
                    "global:onerror:handled service worker install callback error",
                    "global:error:handled service worker install callback error",
                    "install:later",
                ],
                "reports": [
                    { "phase": "onerror", "message": "handled service worker install callback error", "cancel": true, "errorIsError": true },
                    {
                        "phase": "error-listener", "message": "handled service worker install callback error",
                        "defaultPrevented": true, "cancelable": true,
                        "targetIsGlobal": true, "currentTargetIsGlobal": true,
                    },
                ],
            },
            {
                "phase": "activate",
                "trace": [
                    "activate:throwing",
                    "global:onerror:unhandled service worker activate callback error",
                    "global:error:unhandled service worker activate callback error",
                    "activate:later",
                ],
                "reports": [
                    { "phase": "onerror", "message": "unhandled service worker activate callback error", "cancel": false, "errorIsError": false },
                    {
                        "phase": "error-listener", "message": "unhandled service worker activate callback error",
                        "defaultPrevented": false, "cancelable": true,
                        "targetIsGlobal": true, "currentTargetIsGlobal": true,
                    },
                ],
            },
        ])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_lifecycle_handler_properties_follow_listener_order()
{
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.lifecycleMessages = [];
globalThis.clientErrors = [];
navigator.serviceWorker.addEventListener('message', event => lifecycleMessages.push(event.data));
navigator.serviceWorker.addEventListener('error', event => clientErrors.push(['container', event.message]));
globalThis.registrationPromise = navigator.serviceWorker.register('/sw-lifecycle-handler-properties.js', { scope: '/' });
</script><main>service worker lifecycle handler properties</main>"#;
    let worker_script = r#"const callbackTrace = [];
const reports = [];
let traceStart = 0;
let reportStart = 0;
const callbackObservations = {};
const propertyChecks = {
  initiallyNull: oninstall === null && onactivate === null,
};
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'handled service worker oninstall property error';
  reports.push({ phase: 'onerror', message, cancel, errorIsError: error instanceof Error });
  callbackTrace.push('global:onerror:' + message);
  return cancel;
};
addEventListener('error', event => {
  reports.push({
    phase: 'error-listener', message: event.message, defaultPrevented: event.defaultPrevented,
    cancelable: event.cancelable, targetIsGlobal: event.target === self,
    currentTargetIsGlobal: event.currentTarget === self,
  });
  callbackTrace.push('global:error:' + event.message);
});
oninstall = 7;
propertyChecks.primitiveAssignmentCleared = oninstall === null;
const nonCallableInstallHandler = {};
oninstall = nonCallableInstallHandler;
propertyChecks.nonCallableObjectRetained = oninstall === nonCallableInstallHandler;
const objectProbeTraceStart = callbackTrace.length;
const objectProbeReportStart = reports.length;
globalThis.objectHandlerProbe = __glassDispatchServiceWorkerLifecycle('install');
propertyChecks.nonCallableObjectNoOp = callbackTrace.length === objectProbeTraceStart
  && reports.length === objectProbeReportStart;
oninstall = null;
addEventListener('install', event => {
  traceStart = callbackTrace.length;
  reportStart = reports.length;
  callbackTrace.push('install:before');
});
oninstall = function(event) { callbackTrace.push('install:superseded'); };
addEventListener('install', event => callbackTrace.push('install:between'));
oninstall = function(event) {
  callbackTrace.push('install:handler');
  callbackObservations.install = { thisIsGlobal: this === self, type: event.type };
  event.waitUntil(self.skipWaiting());
  throw new Error('handled service worker oninstall property error');
};
propertyChecks.installHandlerReadback = typeof oninstall === 'function';
addEventListener('install', event => {
  callbackTrace.push('install:after');
  event.waitUntil(self.clients.matchAll({ includeUncontrolled: true, type: 'window' }).then(clients => {
    const client = clients.find(candidate => candidate.url.endsWith('/service-worker-lifecycle-handler-properties'));
    client?.postMessage({
      phase: 'install', trace: callbackTrace.slice(traceStart), reports: reports.slice(reportStart),
      propertyChecks, callbackObservations,
    });
  }));
});
addEventListener('activate', event => {
  traceStart = callbackTrace.length;
  reportStart = reports.length;
  callbackTrace.push('activate:before-clear');
});
const supersededActivateHandler = event => callbackTrace.push('activate:superseded');
onactivate = supersededActivateHandler;
onactivate = undefined;
propertyChecks.undefinedAssignmentCleared = onactivate === null;
addEventListener('activate', event => callbackTrace.push('activate:after-clear'));
onactivate = function(event) {
  callbackTrace.push('activate:handler');
  callbackObservations.activate = { thisIsGlobal: this === self, type: event.type };
  event.waitUntil(self.clients.claim());
};
propertyChecks.activateHandlerReadback = typeof onactivate === 'function';
addEventListener('activate', event => {
  callbackTrace.push('activate:after-handler');
  event.waitUntil(self.clients.matchAll({ includeUncontrolled: true, type: 'window' }).then(clients => {
    const client = clients.find(candidate => candidate.url.endsWith('/service-worker-lifecycle-handler-properties'));
    client?.postMessage({
      phase: 'activate', trace: callbackTrace.slice(traceStart), reports: reports.slice(reportStart),
      propertyChecks, callbackObservations,
    });
  }));
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/service-worker-lifecycle-handler-properties",
                "text/html",
                page,
            ),
            (
                "/sw-lifecycle-handler-properties.js",
                "application/javascript",
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

    let mut engine = NativeEngine::new(NativeEngineConfig::default().with_initial_url(format!(
        "http://{address}/service-worker-lifecycle-handler-properties"
    )))
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => { globalThis.activeServiceWorker = reg.active; reg.active.addEventListener('error', event => clientErrors.push(['service-worker', event.message])); return [reg.active.state, navigator.serviceWorker.controller !== null]; })",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );

    for _ in 0..12 {
        let page_state = engine.evaluate_async("lifecycleMessages").await.unwrap();
        if page_state.as_array().is_some_and(|messages| {
            messages.iter().any(|message| message["phase"] == "install")
                && messages
                    .iter()
                    .any(|message| message["phase"] == "activate")
        }) {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    let page_state = engine
        .evaluate_async(
            "({ lifecycleMessages, clientErrors, workerState: activeServiceWorker.state, controlled: navigator.serviceWorker.controller !== null })",
        )
        .await
        .unwrap();
    assert_eq!(page_state["clientErrors"], serde_json::json!([]));
    assert_eq!(page_state["workerState"], serde_json::json!("activated"));
    assert_eq!(page_state["controlled"], serde_json::json!(true));
    assert_eq!(
        page_state["lifecycleMessages"],
        serde_json::json!([
            {
                "phase": "install",
                "trace": [
                    "install:before",
                    "install:handler",
                    "global:onerror:handled service worker oninstall property error",
                    "global:error:handled service worker oninstall property error",
                    "install:between",
                    "install:after",
                ],
                "reports": [
                    { "phase": "onerror", "message": "handled service worker oninstall property error", "cancel": true, "errorIsError": true },
                    {
                        "phase": "error-listener", "message": "handled service worker oninstall property error",
                        "defaultPrevented": true, "cancelable": true,
                        "targetIsGlobal": true, "currentTargetIsGlobal": true,
                    },
                ],
                "propertyChecks": {
                    "initiallyNull": true,
                    "primitiveAssignmentCleared": true,
                    "nonCallableObjectRetained": true,
                    "nonCallableObjectNoOp": true,
                    "undefinedAssignmentCleared": true,
                    "installHandlerReadback": true,
                    "activateHandlerReadback": true,
                },
                "callbackObservations": {
                    "install": { "thisIsGlobal": true, "type": "install" },
                },
            },
            {
                "phase": "activate",
                "trace": [
                    "activate:before-clear",
                    "activate:after-clear",
                    "activate:handler",
                    "activate:after-handler",
                ],
                "reports": [],
                "propertyChecks": {
                    "initiallyNull": true,
                    "primitiveAssignmentCleared": true,
                    "nonCallableObjectRetained": true,
                    "nonCallableObjectNoOp": true,
                    "undefinedAssignmentCleared": true,
                    "installHandlerReadback": true,
                    "activateHandlerReadback": true,
                },
                "callbackObservations": {
                    "install": { "thisIsGlobal": true, "type": "install" },
                    "activate": { "thisIsGlobal": true, "type": "activate" },
                },
            },
        ])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_runs_service_worker_timer_on_next_page_turn() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/page",
                "text/html",
                "<script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' }); globalThis.swEvents = []; navigator.serviceWorker.addEventListener('message', event => swEvents.push(event.data));</script><main>service worker timer</main>",
            ),
            (
                "/sw.js",
                "application/javascript",
                "self.addEventListener('message', event => { setTimeout(() => clients.matchAll({ includeUncontrolled: true }).then(clients => clients[0] && clients[0].postMessage({ kind: 'timer', value: event.data.value })), 0); });",
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
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
                "await registrationPromise.then(reg => { reg.active.postMessage({ value: 7 }); return true; })",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("swEvents").await.unwrap(),
        serde_json::json!([{ "kind": "timer", "value": 7 }])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_message_events_extend_lifetime() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.swEvents = [];
globalThis.pageErrors = [];
navigator.serviceWorker.addEventListener('message', event => swEvents.push(event.data));
navigator.serviceWorker.addEventListener('error', event => pageErrors.push(event.message));
globalThis.registrationPromise = navigator.serviceWorker.register('/sw-message-events.js', { scope: '/' });
</script><main>Service Worker extendable message events</main>"#;
    let worker_script = r#"
addEventListener('install', event => event.waitUntil(self.skipWaiting()));
addEventListener('activate', event => event.waitUntil(self.clients.claim()));
globalThis.workerErrors = [];
addEventListener('error', event => workerErrors.push(event.message));
const syntheticMessage = new ExtendableMessageEvent('message', { data: 'synthetic' });
let syntheticWaitError = '';
try { syntheticMessage.waitUntil(Promise.resolve()); } catch (error) { syntheticWaitError = error.name; }
let syntheticBaseWaitError = '';
try { new ExtendableEvent('install').waitUntil(Promise.resolve()); }
catch (error) { syntheticBaseWaitError = error.name; }
globalThis.syntheticChecks = {
  dispatcherHidden: typeof globalThis.__glassDispatchServiceWorkerMessage === 'undefined',
  isEvent: syntheticMessage instanceof Event,
  isExtendableEvent: syntheticMessage instanceof ExtendableEvent,
  isExtendableMessageEvent: syntheticMessage instanceof ExtendableMessageEvent,
  isMessageEvent: syntheticMessage instanceof MessageEvent,
  isTrusted: syntheticMessage.isTrusted,
  defaultData: syntheticMessage.data,
  defaultOrigin: syntheticMessage.origin,
  defaultLastEventId: syntheticMessage.lastEventId,
  defaultSource: syntheticMessage.source,
  frozenPorts: Object.isFrozen(syntheticMessage.ports),
  syntheticWaitError,
  syntheticBaseWaitError,
};
globalThis.savedMessageEvent = null;
addEventListener('message', event => {
  if (event.data.kind === 'exercise') {
    globalThis.savedMessageEvent = event;
    const observation = {
      isEvent: event instanceof Event,
      isExtendableEvent: event instanceof ExtendableEvent,
      isExtendableMessageEvent: event instanceof ExtendableMessageEvent,
      isMessageEvent: event instanceof MessageEvent,
      isTrusted: event.isTrusted,
      origin: event.origin,
      lastEventId: event.lastEventId,
      dataKind: event.data.kind,
      portsLength: event.ports.length,
      portsFrozen: Object.isFrozen(event.ports),
      sourceId: event.source.id,
      sourceUrl: event.source.url,
      sourceFrozen: Object.isFrozen(event.source),
      targetIsGlobal: event.target === self,
      currentTargetIsGlobal: event.currentTarget === self,
      eventPhase: event.eventPhase,
      bubbles: event.bubbles,
      cancelable: event.cancelable,
      syntheticChecks,
    };
    event.source.postMessage({ kind: 'immediate', observation });
    event.waitUntil(fetch('/message-work').then(response => response.text()).then(body => {
      event.waitUntil(Promise.resolve().then(() => {
        event.source.postMessage({ kind: 'extended', body });
      }));
    }));
    return;
  }
  if (event.data.kind === 'late') {
    let waitError = '';
    try { savedMessageEvent.waitUntil(Promise.resolve()); }
    catch (error) { waitError = error.name; }
    event.source.postMessage({ kind: 'late', waitError });
    return;
  }
  if (event.data.kind === 'reject') {
    event.waitUntil(Promise.reject(new Error('drained lifetime rejection')));
    return;
  }
  if (event.data.kind === 'alive') {
    event.source.postMessage({
      kind: 'alive', trusted: event.isTrusted, workerErrors: workerErrors.slice(),
    });
  }
});
"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/message-page", "text/html", page),
            (
                "/sw-message-events.js",
                "application/javascript",
                worker_script,
            ),
            ("/message-work", "text/plain", "wait-until-finished"),
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

    let origin = format!("http://{address}");
    let page_url = format!("{origin}/message-page");
    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(page_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => { globalThis.activeServiceWorker = reg.active; reg.active.addEventListener('error', event => pageErrors.push('worker:' + event.message)); return [reg.active.state, navigator.serviceWorker.controller !== null]; })",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true]),
    );

    assert_eq!(
        engine
            .evaluate_async(
                "navigator.serviceWorker.controller.postMessage({ kind: 'exercise' }); true"
            )
            .await
            .unwrap(),
        serde_json::json!(true),
    );
    let events = engine.evaluate_async("swEvents").await.unwrap();
    assert_eq!(events.as_array().unwrap().len(), 2);
    let immediate = &events[0];
    assert_eq!(immediate["kind"], "immediate");
    let observation = &immediate["observation"];
    assert_eq!(observation["isEvent"], true);
    assert_eq!(observation["isExtendableEvent"], true);
    assert_eq!(observation["isExtendableMessageEvent"], true);
    assert_eq!(observation["isMessageEvent"], false);
    assert_eq!(observation["isTrusted"], true);
    assert_eq!(observation["origin"], origin);
    assert_eq!(observation["lastEventId"], "");
    assert_eq!(observation["dataKind"], "exercise");
    assert_eq!(observation["portsLength"], 0);
    assert_eq!(observation["portsFrozen"], true);
    assert_ne!(observation["sourceId"], "");
    assert_eq!(observation["sourceUrl"], page_url);
    assert_eq!(observation["sourceFrozen"], true);
    assert_eq!(observation["targetIsGlobal"], true);
    assert_eq!(observation["currentTargetIsGlobal"], true);
    assert_eq!(observation["eventPhase"], 2);
    assert_eq!(observation["bubbles"], false);
    assert_eq!(observation["cancelable"], false);
    assert_eq!(
        observation["syntheticChecks"],
        serde_json::json!({
            "dispatcherHidden": true,
            "isEvent": true,
            "isExtendableEvent": true,
            "isExtendableMessageEvent": true,
            "isMessageEvent": false,
            "isTrusted": false,
            "defaultData": "synthetic",
            "defaultOrigin": "",
            "defaultLastEventId": "",
            "defaultSource": null,
            "frozenPorts": true,
            "syntheticWaitError": "InvalidStateError",
            "syntheticBaseWaitError": "InvalidStateError",
        })
    );
    assert_eq!(
        events[1],
        serde_json::json!({
            "kind": "extended", "body": "wait-until-finished",
        })
    );

    assert_eq!(
        engine
            .evaluate_async(
                "navigator.serviceWorker.controller.postMessage({ kind: 'late' }); true"
            )
            .await
            .unwrap(),
        serde_json::json!(true),
    );
    assert_eq!(
        engine.evaluate_async("swEvents[2]").await.unwrap(),
        serde_json::json!({ "kind": "late", "waitError": "InvalidStateError" }),
    );

    assert_eq!(
        engine
            .evaluate_async(
                "navigator.serviceWorker.controller.postMessage({ kind: 'reject' }); true"
            )
            .await
            .unwrap(),
        serde_json::json!(true),
    );
    assert_eq!(
        engine
            .evaluate_async(
                "navigator.serviceWorker.controller.postMessage({ kind: 'alive' }); true"
            )
            .await
            .unwrap(),
        serde_json::json!(true),
    );
    assert_eq!(
        engine.evaluate_async("swEvents").await.unwrap()[3],
        serde_json::json!({ "kind": "alive", "trusted": true, "workerErrors": [] }),
    );
    let final_state = engine
        .evaluate_async("({ pageErrors, state: activeServiceWorker.state })")
        .await
        .unwrap();
    assert_eq!(final_state["pageErrors"], serde_json::json!([]));
    assert_eq!(final_state["state"], "activated");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

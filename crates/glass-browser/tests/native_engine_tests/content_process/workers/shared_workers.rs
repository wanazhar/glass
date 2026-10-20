use super::*;

#[tokio::test]
async fn native_content_process_shared_worker_reuses_named_runtime_and_ports() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/shared-page",
                "text/html",
                "<script>globalThis.sharedEvents = []; globalThis.shared = new SharedWorker('/shared.js', { name: 'glass-shared' }); globalThis.sharedAgain = new SharedWorker('/shared.js', { name: 'glass-shared' }); shared.port.onmessage = event => sharedEvents.push({ port: 'one', kind: event.data.kind, value: event.data.value, connections: event.data.connections }); sharedAgain.port.onmessage = event => sharedEvents.push({ port: 'two', kind: event.data.kind, value: event.data.value, connections: event.data.connections }); shared.port.start(); sharedAgain.port.start();</script><main>shared worker</main>",
            ),
            (
                "/shared.js",
                "text/javascript",
                "let connections = 0; onconnect = event => { const port = event.ports[0]; const connection = ++connections; port.onmessage = message => port.postMessage({ kind: 'reply', value: Number(message.data.value) + connection }); port.start(); port.postMessage({ kind: 'ready', connections: connection }); };",
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
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/shared-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("sharedEvents").await.unwrap(),
        serde_json::json!([
            {"port": "one", "kind": "ready", "connections": 1},
            {"port": "two", "kind": "ready", "connections": 2},
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("shared.port.postMessage({ value: 4 }); sharedAgain.port.postMessage({ value: 7 }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("sharedEvents").await.unwrap(),
        serde_json::json!([
            {"port": "one", "kind": "ready", "connections": 1},
            {"port": "two", "kind": "ready", "connections": 2},
            {"port": "one", "kind": "reply", "value": 5},
            {"port": "two", "kind": "reply", "value": 9},
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_shared_worker_message_port_bridge_close_both_directions() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.pageMessages = [];
globalThis.pageCloseTrace = [];
globalThis.pageCloseEvents = Object.create(null);
globalThis.pageCloseReports = [];
globalThis.pagePorts = Object.create(null);
globalThis.pageInitiatorPostError = null;
</script><main>SharedWorker MessagePort bridge close</main>"#;
    let worker_script = r#"
let connectionCount = 0;
const states = new Map();
const closeSnapshot = (id, phase, event, port, receiverThis, previous) => ({
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
onconnect = event => {
  const port = event.ports[0];
  const state = {
    id: null, connection: ++connectionCount, trace: [], lastClose: null,
    postCloseError: null, postAfterRemoteCloseError: null,
  };
  port.onclose = function(closeEvent) {
    state.lastClose = closeEvent;
    state.trace.push(closeSnapshot(state.id, 'handler', closeEvent, port, this, closeEvent));
    try { port.postMessage({ kind: 'must-not-cross-after-peer-close', id: state.id }); }
    catch (error) { state.postAfterRemoteCloseError = error.name; }
  };
  port.addEventListener('close', function(closeEvent) {
    state.trace.push(closeSnapshot(state.id, 'listener', closeEvent, port, this, state.lastClose));
  });
  port.addEventListener('message', message => {
    if (message.data.kind === 'register') {
      state.id = message.data.id;
      states.set(state.id, state);
      port.postMessage({ kind: 'ready', id: state.id, connection: state.connection });
    } else if (message.data.kind === 'close-worker') {
      port.postMessage({ kind: 'queued-before-close', id: state.id });
      port.close();
      port.close();
      try { port.postMessage({ kind: 'must-not-cross-after-local-close', id: state.id }); }
      catch (error) { state.postCloseError = error.name; }
    } else if (message.data.kind === 'ping') {
      port.postMessage({ kind: 'pong', id: state.id });
    } else if (message.data.kind === 'report') {
      const target = states.get(message.data.targetId);
      port.postMessage({
        kind: 'close-report',
        id: message.data.targetId,
        trace: target ? target.trace : [],
        eventStateReset: !target || !target.lastClose
          || (target.lastClose.currentTarget === null && target.lastClose.eventPhase === 0),
        postCloseError: target ? target.postCloseError : null,
        postAfterRemoteCloseError: target ? target.postAfterRemoteCloseError : null,
      });
    }
  });
  port.start();
};"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/shared-port-close-page", "text/html", page),
            ("/shared-port-close.js", "text/javascript", worker_script),
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
            .with_initial_url(format!("http://{address}/shared-port-close-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
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
    const shared = new SharedWorker('/shared-port-close.js', { name: 'glass-shared-port-close' });
    const port = shared.port;
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
    port.postMessage({ kind: 'register', id });
  }
  return true;
})()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );

    let mut ready_state = serde_json::Value::Null;
    for _ in 0..8 {
        ready_state = engine
            .evaluate_async(
                "(() => { const ready = pageMessages.filter(message => message.kind === 'ready'); return { ids: ready.map(message => message.id).sort(), connections: ready.map(message => message.connection).sort((a, b) => a - b) }; })()",
            )
            .await
            .unwrap();
        if ready_state["ids"]
            .as_array()
            .is_some_and(|ids| ids.len() == 3)
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        ready_state,
        serde_json::json!({
            "ids": ["page-closes", "unrelated", "worker-closes"],
            "connections": [1, 2, 3],
        })
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
            .evaluate_async("({ pageCloseTrace, pageMessages, pageInitiatorPostError })")
            .await
            .unwrap();
        if post_close_state["pageCloseTrace"]
            .as_array()
            .is_some_and(|trace| trace.len() == 2)
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

    assert_eq!(
        engine
            .evaluate_async("pagePorts.unrelated.postMessage({ kind: 'ping' }); true",)
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut unrelated_reply = serde_json::Value::Null;
    for _ in 0..8 {
        unrelated_reply = engine
            .evaluate_async(
                "pageMessages.filter(message => message.kind === 'pong' && message.id === 'unrelated')",
            )
            .await
            .unwrap();
        if unrelated_reply
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
    assert_eq!(unrelated_reply.as_array().unwrap().len(), 1);
    let post_close_messages = engine.evaluate_async("pageMessages").await.unwrap();
    assert!(
        !post_close_messages
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["kind"] == "queued-before-close"
                || message["kind"] == "must-not-cross-after-peer-close"
                || message["kind"] == "must-not-cross-after-local-close")
    );

    engine
        .evaluate_async(
            "pagePorts.unrelated.postMessage({ kind: 'report', targetId: 'page-closes' }); pagePorts.unrelated.postMessage({ kind: 'report', targetId: 'worker-closes' }); true",
        )
        .await
        .unwrap();
    let mut close_reports = serde_json::Value::Null;
    for _ in 0..8 {
        close_reports = engine
            .evaluate_async(
                "pageMessages.filter(message => message.kind === 'close-report').slice().sort((a, b) => a.id.localeCompare(b.id))",
            )
            .await
            .unwrap();
        let page_initiated_close_delivered = close_reports
            .as_array()
            .and_then(|reports| reports.iter().find(|report| report["id"] == "page-closes"))
            .and_then(|report| report["trace"].as_array())
            .is_some_and(|trace| trace.len() == 2);
        if close_reports
            .as_array()
            .is_some_and(|reports| reports.len() == 2)
            && page_initiated_close_delivered
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
                    { "id": "page-closes", "phase": "handler", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
                    { "id": "page-closes", "phase": "listener", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
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
            .evaluate_async("pagePorts.unrelated.postMessage({ kind: 'ping' }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut post_both_closes_replies = serde_json::Value::Null;
    for _ in 0..8 {
        post_both_closes_replies = engine
            .evaluate_async(
                "pageMessages.filter(message => message.kind === 'pong' && message.id === 'unrelated')",
            )
            .await
            .unwrap();
        if post_both_closes_replies
            .as_array()
            .is_some_and(|messages| messages.len() == 2)
        {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(post_both_closes_replies.as_array().unwrap().len(), 2);

    let final_state = engine
        .evaluate_async(
            "({ pageCloseTrace, pageCloseStateReset: pageCloseEvents['worker-closes'].currentTarget === null && pageCloseEvents['worker-closes'].eventPhase === 0, remotePostError: (() => { try { pagePorts['worker-closes'].postMessage({ kind: 'must-not-cross-after-remote-close' }); return null; } catch (error) { return error.name; } })(), pageMessages })",
        )
        .await
        .unwrap();
    assert_eq!(final_state["pageCloseStateReset"], true);
    assert_eq!(final_state["remotePostError"], serde_json::Value::Null);
    assert_eq!(
        final_state["pageCloseTrace"],
        serde_json::json!([
            { "id": "worker-closes", "phase": "handler", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
            { "id": "worker-closes", "phase": "listener", "sameEvent": true, "type": "close", "genericEvent": true, "targetIsPort": true, "currentTargetIsPort": true, "eventPhase": 2, "bubbles": false, "cancelable": false, "thisIsPort": true },
        ])
    );
    assert!(
        !final_state["pageMessages"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |message| message["kind"] == "must-not-cross-after-remote-close"
                    || message["kind"] == "queued-before-close"
                    || message["kind"] == "must-not-cross-after-peer-close"
                    || message["kind"] == "must-not-cross-after-local-close"
            )
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_reports_shared_worker_connect_callback_errors_at_global() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.sharedEvents = [];
globalThis.ownerErrors = [];
const connectShared = () => {
  const worker = new SharedWorker('/shared-connect-errors.js', { name: 'glass-shared-connect-errors' });
  worker.onerror = event => ownerErrors.push('handler:' + event.message);
  worker.addEventListener('error', event => ownerErrors.push('listener:' + event.message));
  worker.port.onmessage = event => sharedEvents.push(event.data);
  worker.port.start();
  return worker;
};
globalThis.shared = connectShared();
globalThis.sharedAgain = connectShared();
</script><main>shared worker connect errors</main>"#;
    let worker_script = r#"let connection = 0;
let reportStart = 0;
const reports = [];
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'onconnect failure';
  reports.push({ phase: 'onerror', message, filename, line, column, errorIsError: error instanceof Error, cancel });
  return cancel;
};
addEventListener('error', event => reports.push({
  phase: 'error-listener', message: event.message, defaultPrevented: event.defaultPrevented,
  cancelable: event.cancelable, targetIsGlobal: event.target === globalThis,
  currentTargetIsGlobal: event.currentTarget === globalThis,
}));
onconnect = () => {
  connection += 1;
  reportStart = reports.length;
  if (connection === 1) throw new Error('onconnect failure');
};
addEventListener('connect', () => {
  if (connection === 2) throw 'connect listener failure';
});
addEventListener('connect', event => {
  const port = event.ports[0];
  port.start();
  port.postMessage({
    kind: 'connected', connection, connectDefaultPrevented: event.defaultPrevented,
    reports: reports.slice(reportStart),
  });
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/shared-page", "text/html", page),
            (
                "/shared-connect-errors.js",
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
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/shared-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let worker_url = format!("http://{address}/shared-connect-errors.js");
    assert_eq!(
        engine.evaluate_async("sharedEvents").await.unwrap(),
        serde_json::json!([
            {
                "kind": "connected",
                "connection": 1,
                "connectDefaultPrevented": false,
                "reports": [
                    {
                        "phase": "onerror",
                        "message": "onconnect failure",
                        "filename": worker_url,
                        "line": 0,
                        "column": 0,
                        "errorIsError": true,
                        "cancel": true,
                    },
                    {
                        "phase": "error-listener",
                        "message": "onconnect failure",
                        "defaultPrevented": true,
                        "cancelable": true,
                        "targetIsGlobal": true,
                        "currentTargetIsGlobal": true,
                    },
                ],
            },
            {
                "kind": "connected",
                "connection": 2,
                "connectDefaultPrevented": false,
                "reports": [
                    {
                        "phase": "onerror",
                        "message": "connect listener failure",
                        "filename": worker_url,
                        "line": 0,
                        "column": 0,
                        "errorIsError": false,
                        "cancel": false,
                    },
                    {
                        "phase": "error-listener",
                        "message": "connect listener failure",
                        "defaultPrevented": false,
                        "cancelable": true,
                        "targetIsGlobal": true,
                        "currentTargetIsGlobal": true,
                    },
                ],
            },
        ])
    );
    assert_eq!(
        engine.evaluate_async("ownerErrors").await.unwrap(),
        serde_json::json!([])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_shared_worker_message_port_callback_errors_recover() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.sharedEvents = [];
globalThis.ownerErrors = [];
globalThis.shared = new SharedWorker('/shared-port-errors.js', { name: 'glass-shared-port-errors' });
shared.onerror = event => ownerErrors.push('handler:' + event.message);
shared.addEventListener('error', event => ownerErrors.push('listener:' + event.message));
shared.port.onmessage = event => sharedEvents.push(event.data);
shared.port.start();
</script><main>shared worker message port callback errors</main>"#;
    let worker_script = r#"const reports = [];
const callbackTrace = [];
let connectReportStart = 0;
let messageReportStart = 0;
let messageTraceStart = 0;
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'port onmessage failure';
  reports.push({ phase: 'onerror', message, cancel, errorIsError: error instanceof Error });
  callbackTrace.push('onerror:' + message);
  return cancel;
};
addEventListener('error', event => {
  reports.push({
    phase: 'error-listener', message: event.message, defaultPrevented: event.defaultPrevented,
    cancelable: event.cancelable, targetIsGlobal: event.target === globalThis,
    currentTargetIsGlobal: event.currentTarget === globalThis,
  });
  callbackTrace.push('error-listener:' + event.message);
});
onconnect = () => {
  connectReportStart = reports.length;
  throw new Error('connect callback failure');
};
addEventListener('connect', event => {
  const port = event.ports[0];
  port.onmessage = message => {
    messageReportStart = reports.length;
    messageTraceStart = callbackTrace.length;
    callbackTrace.push('onmessage:' + message.data.kind);
    if (message.data.kind === 'throw-handler') throw new Error('port onmessage failure');
  };
  port.addEventListener('message', message => {
    const kind = message.data.kind;
    callbackTrace.push('listener-one:' + kind);
    if (kind === 'throw-listener') throw 'port listener failure';
  });
  port.addEventListener('message', message => {
    const kind = message.data.kind;
    callbackTrace.push('listener-two:' + kind);
    port.postMessage({
      kind: 'reply', request: kind, trace: callbackTrace.slice(messageTraceStart),
      reports: reports.slice(messageReportStart),
    });
  });
  port.start();
  port.postMessage({ kind: 'ready', reports: reports.slice(connectReportStart) });
});"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/shared-port-page", "text/html", page),
            ("/shared-port-errors.js", "text/javascript", worker_script),
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
            .with_initial_url(format!("http://{address}/shared-port-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let ready_event = serde_json::json!({
        "kind": "ready",
        "reports": [
            { "phase": "onerror", "message": "connect callback failure", "cancel": false, "errorIsError": true },
            {
                "phase": "error-listener", "message": "connect callback failure",
                "defaultPrevented": false, "cancelable": true,
                "targetIsGlobal": true, "currentTargetIsGlobal": true,
            },
        ],
    });
    assert_eq!(
        engine.evaluate_async("sharedEvents").await.unwrap(),
        serde_json::json!([ready_event.clone()])
    );
    let mut expected_events = vec![ready_event];

    for (kind, expected_report, trace) in [
        (
            "throw-handler",
            serde_json::json!([
                { "phase": "onerror", "message": "port onmessage failure", "cancel": true, "errorIsError": true },
                {
                    "phase": "error-listener", "message": "port onmessage failure",
                    "defaultPrevented": true, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ]),
            vec![
                "onmessage:throw-handler",
                "onerror:port onmessage failure",
                "error-listener:port onmessage failure",
                "listener-one:throw-handler",
                "listener-two:throw-handler",
            ],
        ),
        (
            "throw-listener",
            serde_json::json!([
                { "phase": "onerror", "message": "port listener failure", "cancel": false, "errorIsError": false },
                {
                    "phase": "error-listener", "message": "port listener failure",
                    "defaultPrevented": false, "cancelable": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                },
            ]),
            vec![
                "onmessage:throw-listener",
                "listener-one:throw-listener",
                "onerror:port listener failure",
                "error-listener:port listener failure",
                "listener-two:throw-listener",
            ],
        ),
        (
            "alive",
            serde_json::json!([]),
            vec![
                "onmessage:alive",
                "listener-one:alive",
                "listener-two:alive",
            ],
        ),
    ] {
        assert_eq!(
            engine
                .evaluate_async(&format!(
                    "shared.port.postMessage({{ kind: '{kind}' }}); true"
                ))
                .await
                .unwrap(),
            serde_json::json!(true),
            "page-to-worker MessagePort delivery for {kind}",
        );
        expected_events.push(serde_json::json!({
            "kind": "reply", "request": kind, "trace": trace,
            "reports": expected_report,
        }));
        assert_eq!(
            engine.evaluate_async("sharedEvents").await.unwrap(),
            serde_json::Value::Array(expected_events.clone()),
            "worker reply and callback recovery after {kind}",
        );
    }
    assert_eq!(
        engine.evaluate_async("ownerErrors").await.unwrap(),
        serde_json::json!([]),
        "SharedWorker-global callback errors must not fan out to page owners",
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_resolves_runtime_shared_worker_module_imports() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(25), async {
            let mut requests = Vec::new();
            for _ in 0..8 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("SharedWorker module request includes a URL")
                    .to_owned();
                let (status, content_type, body) = match path.as_str() {
                    "/page" => (
                        "200 OK",
                        "text/html",
                        "<script>globalThis.sharedWorkerMessages = []; const listen = worker => { worker.port.onmessage = event => sharedWorkerMessages.push(event.data); worker.port.start(); }; globalThis.classicShared = new SharedWorker('/classic-shared.js', { name: 'dynamic-classic' }); listen(classicShared); globalThis.moduleShared = new SharedWorker('/module-shared.js', { name: 'dynamic-module', type: 'module' }); listen(moduleShared);</script>",
                    ),
                    "/classic-shared.js" => (
                        "200 OK",
                        "application/javascript",
                        "globalThis.onconnect = event => { const port = event.ports[0]; const target = './classic-dep.js?runtime=classic'; import(target).then(async module => { port.postMessage({ kind: 'classic', value: module.value, nested: await module.loadNested() }); }, error => port.postMessage({ kind: 'classic-error', message: String(error) })); const missing = './classic-missing.js'; import(missing).then(() => port.postMessage({ kind: 'unexpected' }), error => port.postMessage({ kind: 'rejected', type: error instanceof TypeError })); };",
                    ),
                    "/classic-dep.js?runtime=classic" => (
                        "200 OK",
                        "application/javascript",
                        "export const value = 'classic-shared'; export async function loadNested() { const nested = './classic-nested.js?runtime=nested'; return (await import(nested)).value; }",
                    ),
                    "/classic-nested.js?runtime=nested" => (
                        "200 OK",
                        "application/javascript",
                        "export const value = 'classic-nested';",
                    ),
                    "/classic-missing.js" => (
                        "404 Not Found",
                        "application/javascript",
                        "not found",
                    ),
                    "/module-shared.js" => (
                        "200 OK",
                        "application/javascript",
                        "globalThis.onconnect = event => { const port = event.ports[0]; const target = './module-dep.js?runtime=module'; import(target).then(async module => { port.postMessage({ kind: 'module', value: module.value, nested: await module.loadNested() }); }, error => port.postMessage({ kind: 'module-error', message: String(error) })); };",
                    ),
                    "/module-dep.js?runtime=module" => (
                        "200 OK",
                        "application/javascript",
                        "export const value = 'module-shared'; export async function loadNested() { const nested = './module-nested.js?runtime=nested'; return (await import(nested)).value; }",
                    ),
                    "/module-nested.js?runtime=nested" => (
                        "200 OK",
                        "application/javascript",
                        "export const value = 'module-nested';",
                    ),
                    other => panic!("unexpected SharedWorker dynamic-module request: {other}"),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push(path);
            }
            requests.sort();
            requests
        })
        .await
        .expect("SharedWorker dynamic-module requests stay within their time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    for _ in 0..4 {
        let count = engine
            .evaluate_async("sharedWorkerMessages.length")
            .await
            .unwrap();
        if count.as_u64() == Some(3) {
            break;
        }
    }
    let result = engine
        .evaluate_async(
            "sharedWorkerMessages.slice().sort((a, b) => String(a.kind).localeCompare(String(b.kind)))",
        )
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        result,
        serde_json::json!([
            {"kind": "classic", "value": "classic-shared", "nested": "classic-nested"},
            {"kind": "module", "value": "module-shared", "nested": "module-nested"},
            {"kind": "rejected", "type": true},
        ]),
        "classic and module SharedWorker imports must settle into their connected MessagePorts, including nested imports and rejections"
    );
    assert_eq!(
        requests,
        vec![
            "/classic-dep.js?runtime=classic",
            "/classic-missing.js",
            "/classic-nested.js?runtime=nested",
            "/classic-shared.js",
            "/module-dep.js?runtime=module",
            "/module-nested.js?runtime=nested",
            "/module-shared.js",
            "/page",
        ]
    );
}
#[tokio::test]
async fn native_content_process_shared_worker_fetch_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..9 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "SharedWorker Fetch API request idle timeout after paths {:?}",
                        requests
                            .iter()
                            .map(|(path, _): &(String, Option<String>)| path)
                            .collect::<Vec<_>>()
                    )
                })
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("SharedWorker Fetch API request includes a URL")
                .to_owned();
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (extra_headers, content_type, body): (&str, &str, &str) = match path.as_str() {
                "/page" => (
                    concat!(
                        "Set-Cookie: worker_fetch=initial; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: worker_fetch_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: worker_fetch_removed=temporary; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: worker_fetch_removed=; Max-Age=0; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<script>globalThis.sharedFetchMessages = []; globalThis.sharedFetchWorker = new SharedWorker('/shared-fetch.js', { type: 'module', name: 'direct-fetch' }); sharedFetchWorker.port.onmessage = event => sharedFetchMessages.push(event.data); sharedFetchWorker.port.start();</script>",
                ),
                "/shared-fetch.js" => (
                    "Set-Cookie: worker_entry_secret=entry; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    "import { value as moduleValue } from './shared-fetch-dependency.mjs'; globalThis.onconnect = event => { const port = event.ports[0]; port.postMessage({ kind: 'ready', module: moduleValue }); fetch('/startup', { credentials: 'include' }).then(async response => port.postMessage({ kind: 'startup', text: await response.text() })).catch(error => port.postMessage({ kind: 'error', message: String(error) })); port.onmessage = async message => { if (message.data !== 'fetch') return; try { const runtimeModule = await import('./shared-fetch-runtime.mjs'); const runtimeDependency = await runtimeModule.load(); const included = await fetch('/include', { credentials: 'include' }); const includeBody = await included.text(); const omitted = await fetch('/omit', { credentials: 'omit' }); const omitBody = await omitted.text(); const after = await fetch('/after', { credentials: 'include' }); const afterBody = await after.text(); port.postMessage({ kind: 'complete', runtime: runtimeDependency.value, values: [includeBody, omitBody, afterBody], statuses: [included.status, omitted.status, after.status] }); } catch (error) { port.postMessage({ kind: 'error', message: String(error) }); } }; };",
                ),
                "/shared-fetch-dependency.mjs" => (
                    "Set-Cookie: worker_module_secret=module; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    "export const value = 'module dependency loaded';",
                ),
                "/shared-fetch-runtime.mjs" => (
                    "Set-Cookie: worker_runtime_secret=runtime; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    "export function load() { return import('./shared-fetch-runtime-dependency.mjs'); }",
                ),
                "/shared-fetch-runtime-dependency.mjs" => (
                    "Set-Cookie: worker_runtime_dependency_secret=runtime-dependency; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    "export const value = 'runtime dependency loaded';",
                ),
                "/startup" => (
                    "Set-Cookie: worker_startup_secret=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "startup complete",
                ),
                "/include" => (
                    concat!(
                        "Set-Cookie: worker_fetch=latest; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: worker_fetch_secret=latest-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: worker_fetch_removed=; Max-Age=0; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/plain",
                    "included",
                ),
                "/omit" => ("", "text/plain", "omitted"),
                "/after" => ("", "text/plain", "after"),
                other => panic!("unexpected SharedWorker Fetch API request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie));
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    for _ in 0..8 {
        let messages = engine.evaluate_async("sharedFetchMessages").await.unwrap();
        if messages.as_array().is_some_and(|messages| {
            messages.iter().any(|message| message["kind"] == "ready")
                && messages.iter().any(|message| message["kind"] == "startup")
        }) {
            break;
        }
    }
    assert_eq!(
        engine.evaluate_async("sharedFetchMessages").await.unwrap(),
        serde_json::json!([
            { "kind": "ready", "module": "module dependency loaded" },
            { "kind": "startup", "text": "startup complete" },
        ])
    );
    engine
        .evaluate_async("sharedFetchWorker.port.postMessage('fetch')")
        .await
        .unwrap();
    for _ in 0..12 {
        let complete = engine
            .evaluate_async("sharedFetchMessages.some(message => message.kind === 'complete' || message.kind === 'error')")
            .await
            .unwrap();
        if complete == serde_json::json!(true) {
            break;
        }
    }
    let messages = engine.evaluate_async("sharedFetchMessages").await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        messages,
        serde_json::json!([
            { "kind": "ready", "module": "module dependency loaded" },
            { "kind": "startup", "text": "startup complete" },
            { "kind": "complete", "runtime": "runtime dependency loaded", "values": ["included", "omitted", "after"], "statuses": [200, 200, 200] },
        ]),
        "a connected SharedWorker Fetch API must resolve response bodies and report the later request"
    );
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .unwrap_or_else(|| panic!("missing SharedWorker fetch request {path}"))
            .1
            .as_deref()
            .unwrap_or("")
            .to_owned()
    };
    let included_cookie = cookie_for("/include");
    assert!(included_cookie.contains("worker_fetch=initial"));
    assert!(included_cookie.contains("worker_fetch_secret=initial-secret"));
    assert!(included_cookie.contains("worker_entry_secret=entry"));
    assert!(included_cookie.contains("worker_module_secret=module"));
    assert!(included_cookie.contains("worker_runtime_secret=runtime"));
    assert!(included_cookie.contains("worker_runtime_dependency_secret=runtime-dependency"));
    assert!(included_cookie.contains("worker_startup_secret=accepted"));
    assert!(!included_cookie.contains("worker_fetch_removed="));
    assert!(cookie_for("/omit").is_empty());
    let after_cookie = cookie_for("/after");
    assert!(after_cookie.contains("worker_fetch=latest"));
    assert!(after_cookie.contains("worker_fetch_secret=latest-secret"));
    assert!(after_cookie.contains("worker_entry_secret=entry"));
    assert!(after_cookie.contains("worker_module_secret=module"));
    assert!(after_cookie.contains("worker_runtime_secret=runtime"));
    assert!(after_cookie.contains("worker_runtime_dependency_secret=runtime-dependency"));
    assert!(after_cookie.contains("worker_startup_secret=accepted"));
    assert!(!after_cookie.contains("worker_fetch_removed="));
    assert!(cookie_for("/startup").contains("worker_fetch_secret=initial-secret"));
    assert!(cookie_for("/shared-fetch.js").contains("worker_fetch_secret=initial-secret"));
    let dependency_cookie = cookie_for("/shared-fetch-dependency.mjs");
    assert!(dependency_cookie.contains("worker_fetch_secret=initial-secret"));
    assert!(dependency_cookie.contains("worker_entry_secret=entry"));
    assert!(cookie_for("/startup").contains("worker_entry_secret=entry"));
    assert!(cookie_for("/startup").contains("worker_module_secret=module"));
    assert!(cookie_for("/shared-fetch-runtime.mjs").contains("worker_module_secret=module"));
    assert!(
        cookie_for("/shared-fetch-runtime-dependency.mjs")
            .contains("worker_runtime_secret=runtime")
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("worker_fetch=latest")
    );
    let cookies = engine.cookies_async().await.unwrap();
    assert!(
        cookies
            .iter()
            .any(|cookie| { cookie.name == "worker_startup_secret" && cookie.value == "accepted" })
    );
    for (name, value) in [
        ("worker_entry_secret", "entry"),
        ("worker_module_secret", "module"),
        ("worker_runtime_secret", "runtime"),
        ("worker_runtime_dependency_secret", "runtime-dependency"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.value == value && cookie.http_only),
            "parent cookie jar is missing HttpOnly {name}"
        );
    }
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_content_process_shared_worker_fetch_credentials_modes_follow_redirects() {
    let _guard = native_content_process_test_lock().lock().await;
    let primary_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let primary_address = primary_listener.local_addr().unwrap();
    let cross_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cross_address = cross_listener.local_addr().unwrap();
    let page_origin = format!("http://{primary_address}");
    let cross_origin = format!("http://{cross_address}");
    let worker_cross_origin = cross_origin.clone();

    let primary_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(45), async move {
            let mut requests = Vec::new();
            let worker_script = format!(
                r#"globalThis.onconnect = event => {{
                  const port = event.ports[0];
                  port.postMessage({{ kind: "ready" }});
                  port.onmessage = async message => {{
                    if (message.data !== "fetch") return;
                    try {{
                      const crossOrigin = "{worker_cross_origin}";
                      const values = [];
                      const defaultResponse = await fetch("/same-default");
                      values.push(await defaultResponse.text());
                      const request = new Request("/same-request");
                      const requestResponse = await fetch(request);
                      values.push(await requestResponse.text());
                      const explicit = await fetch("/same-explicit", {{ credentials: "same-origin" }});
                      values.push(await explicit.text());
                      const sameOrigin = await fetch(crossOrigin + "/same-origin");
                      values.push(await sameOrigin.text());
                      const afterSameOrigin = await fetch(crossOrigin + "/after-same-origin");
                      values.push(await afterSameOrigin.text());
                      const redirected = await fetch("/redirect-to-cross");
                      values.push(await redirected.text());
                      const afterRedirect = await fetch("/after-redirect");
                      values.push(await afterRedirect.text());
                      const included = await fetch(crossOrigin + "/include", {{ credentials: "include" }});
                      values.push(await included.text());
                      const includeAfter = await fetch(crossOrigin + "/include-after", {{ credentials: "include" }});
                      values.push(await includeAfter.text());
                      const omitted = await fetch(crossOrigin + "/omit", {{ credentials: "omit" }});
                      values.push(await omitted.text());
                      const afterOmit = await fetch(crossOrigin + "/after-omit", {{ credentials: "include" }});
                      values.push(await afterOmit.text());
                      const invalidRejected = await fetch(crossOrigin + "/invalid", {{ credentials: "invalid" }})
                        .then(() => false, error => error instanceof TypeError);
                      port.postMessage({{
                        kind: "complete",
                        values,
                        statuses: [defaultResponse.status, requestResponse.status, explicit.status,
                          sameOrigin.status, afterSameOrigin.status, redirected.status,
                          afterRedirect.status, included.status, includeAfter.status,
                          omitted.status, afterOmit.status],
                        requestCredentials: request.credentials,
                        redirected: redirected.redirected,
                        invalidRejected,
                      }});
                    }} catch (error) {{
                      port.postMessage({{ kind: "error", message: String(error) }});
                    }}
                  }};
                }};"#
            );
            for _ in 0..7 {
                let (mut stream, _) = primary_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("primary Fetch request includes a URL")
                    .to_owned();
                let cookie = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim().to_owned());
                let (status, extra_headers, content_type, body): (&str, &str, &str, &str) =
                    match path.as_str() {
                        "/page" => (
                            "200 OK",
                            "Set-Cookie: root=seed; Path=/; SameSite=Lax\r\n",
                            "text/html",
                            "<script>globalThis.sharedFetchMessages = []; globalThis.sharedFetchWorker = new SharedWorker('/shared-fetch-credentials.js', { type: 'module', name: 'fetch-credentials' }); sharedFetchWorker.port.onmessage = event => sharedFetchMessages.push(event.data); sharedFetchWorker.port.start();</script>",
                        ),
                        "/shared-fetch-credentials.js" => (
                            "200 OK",
                            "",
                            "application/javascript",
                            worker_script.as_str(),
                        ),
                        "/same-default" => (
                            "200 OK",
                            "Set-Cookie: default=accepted; Path=/; SameSite=Lax\r\n",
                            "text/plain",
                            "default",
                        ),
                        "/same-request" => ("200 OK", "", "text/plain", "request"),
                        "/same-explicit" => ("200 OK", "", "text/plain", "explicit"),
                        "/redirect-to-cross" => (
                            "302 Found",
                            "",
                            "text/plain",
                            "redirecting",
                        ),
                        "/after-redirect" => ("200 OK", "", "text/plain", "after-redirect"),
                        other => panic!("unexpected primary Fetch request: {other}"),
                    };
                let extra_headers = if path == "/redirect-to-cross" {
                    format!(
                        "Location: {}/redirect-target\r\nSet-Cookie: hop=accepted; Path=/; SameSite=Lax\r\n",
                        worker_cross_origin
                    )
                } else {
                    extra_headers.to_owned()
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((path, cookie));
            }
            requests
        })
        .await
        .expect("primary SharedWorker credential requests stay within the time bound")
    });

    let cross_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(45), async move {
            let mut requests = Vec::new();
            for _ in 0..7 {
                let (mut stream, _) = cross_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("cross-origin Fetch request includes a URL")
                    .to_owned();
                let cookie = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim().to_owned());
                let (extra_headers, body) = match path.as_str() {
                    "/same-origin" => (
                        "Access-Control-Allow-Origin: *\r\nSet-Cookie: cross_same=ignored; Path=/; SameSite=Lax\r\n".to_owned(),
                        "same-origin",
                    ),
                    "/after-same-origin" => {
                        ("Access-Control-Allow-Origin: *\r\n".to_owned(), "after-same-origin")
                    }
                    "/redirect-target" => (
                        "Access-Control-Allow-Origin: *\r\nSet-Cookie: cross_redirect=ignored; Path=/; SameSite=Lax\r\n".to_owned(),
                        "redirect-target",
                    ),
                    "/include" => (
                        format!(
                            "Access-Control-Allow-Origin: {page_origin}\r\nAccess-Control-Allow-Credentials: true\r\nSet-Cookie: cross=accepted; Path=/; SameSite=Lax\r\n"
                        ),
                        "include",
                    ),
                    "/include-after" => (
                        format!(
                            "Access-Control-Allow-Origin: {page_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                        ),
                        "include-after",
                    ),
                    "/omit" => (
                        "Access-Control-Allow-Origin: *\r\nSet-Cookie: omitted=ignored; Path=/; SameSite=Lax\r\n".to_owned(),
                        "omit",
                    ),
                    "/after-omit" => (
                        format!(
                            "Access-Control-Allow-Origin: {page_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                        ),
                        "after-omit",
                    ),
                    other => panic!("unexpected cross-origin Fetch request: {other}"),
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((path, cookie));
            }
            requests
        })
        .await
        .expect("cross-origin SharedWorker credential requests stay within the time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{primary_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine.evaluate_async("sharedFetchMessages").await.unwrap();
            if messages
                .as_array()
                .is_some_and(|messages| messages.iter().any(|message| message["kind"] == "ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(ready.is_ok(), "module SharedWorker must announce readiness");
    engine
        .evaluate_async("sharedFetchWorker.port.postMessage('fetch')")
        .await
        .unwrap();
    let messages = tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let messages = engine.evaluate_async("sharedFetchMessages").await.unwrap();
            if messages.as_array().is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message["kind"] == "complete" || message["kind"] == "error")
            }) {
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("worker credential requests must settle");
    engine.close_async().await.unwrap();
    let primary_requests = primary_server.await.unwrap();
    let cross_requests = cross_server.await.unwrap();

    assert_eq!(
        messages,
        serde_json::json!([
            { "kind": "ready" },
            {
                "kind": "complete",
                "values": ["default", "request", "explicit", "same-origin", "after-same-origin", "redirect-target", "after-redirect", "include", "include-after", "omit", "after-omit"],
                "statuses": [200, 200, 200, 200, 200, 200, 200, 200, 200, 200, 200],
                "requestCredentials": "same-origin",
                "redirected": true,
                "invalidRejected": true,
            },
        ]),
        "worker Fetch must preserve same-origin credentials, CORS, redirect, and invalid-mode semantics"
    );
    let cookie_for = |requests: &[(String, Option<String>)], path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .unwrap_or_else(|| panic!("missing Fetch request {path}"))
            .1
            .as_deref()
            .unwrap_or("")
            .to_owned()
    };
    let default_cookie = cookie_for(&primary_requests, "/same-default");
    assert!(default_cookie.contains("root=seed"));
    let request_cookie = cookie_for(&primary_requests, "/same-request");
    assert!(request_cookie.contains("default=accepted"));
    let explicit_cookie = cookie_for(&primary_requests, "/same-explicit");
    assert!(explicit_cookie.contains("default=accepted"));
    let same_origin_cookie = cookie_for(&cross_requests, "/same-origin");
    assert!(same_origin_cookie.is_empty());
    assert!(cookie_for(&cross_requests, "/after-same-origin").is_empty());
    assert!(cookie_for(&cross_requests, "/redirect-target").is_empty());
    let after_redirect_cookie = cookie_for(&primary_requests, "/after-redirect");
    assert!(after_redirect_cookie.contains("hop=accepted"));
    assert!(!after_redirect_cookie.contains("cross_redirect="));
    let include_cookie = cookie_for(&cross_requests, "/include");
    assert!(include_cookie.contains("root=seed"));
    assert!(include_cookie.contains("default=accepted"));
    assert!(!include_cookie.contains("cross_same="));
    assert!(!include_cookie.contains("cross_redirect="));
    let include_after_cookie = cookie_for(&cross_requests, "/include-after");
    assert!(include_after_cookie.contains("cross=accepted"));
    assert!(cookie_for(&cross_requests, "/omit").is_empty());
    let after_omit_cookie = cookie_for(&cross_requests, "/after-omit");
    assert!(after_omit_cookie.contains("cross=accepted"));
    assert!(!after_omit_cookie.contains("omitted="));
}

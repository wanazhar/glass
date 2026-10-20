use super::*;

#[tokio::test]
async fn native_content_process_blob_object_urls_navigate_across_the_worker_boundary() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/start"));
        let body = "<script>addEventListener('load', () => { const url = URL.createObjectURL(new Blob(['<title>Remote Blob</title><p>Loaded in the native content process</p>'], { type: 'text/html' })); location.assign(url); });</script><title>Start</title><p>Initial</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/start")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert!(snapshot.url.starts_with("blob:http://127.0.0.1:"));
    assert_eq!(
        snapshot.origin,
        NativeOrigin::Tuple {
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port: address.port(),
        }
    );
    assert_eq!(snapshot.title, "Remote Blob");
    assert_eq!(
        snapshot.visible_text,
        "Loaded in the native content process"
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_runs_worker_created_during_page_load() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, body) in [
            (
                "/worker-page",
                "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker.js'); worker.onmessage = event => workerMessages.push(event.data);</script><main>Native</main>",
            ),
            (
                "/worker.js",
                "self.onmessage = event => postMessage({ kind: 'reply', value: event.data + 1, href: self.location.href }); setTimeout(() => postMessage({ kind: 'timer' }), 0); postMessage({ kind: 'ready' });",
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(path));
            let content_type = if path.ends_with(".js") {
                "text/javascript"
            } else {
                "text/html"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/worker-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("worker.postMessage(4); workerMessages")
            .await
            .unwrap(),
        serde_json::json!([{"kind": "ready"}, {"kind": "timer"}])
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "kind": "ready"
        }, {
            "kind": "timer"
        }, {
            "kind": "reply",
            "value": 5,
            "href": format!("http://{address}/worker.js"),
        }])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_message_delivery_preserves_large_payloads() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/large-worker-page",
                "text/html",
                "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/large-worker.js'); worker.onmessage = event => workerMessages.push(event.data);</script><main>Native</main>",
            ),
            (
                "/large-worker.js",
                "text/javascript",
                "self.onmessage = event => postMessage(event.data);",
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
            .with_initial_url(format!("http://{address}/large-worker-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("worker.postMessage('x'.repeat(20000)); true")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("workerMessages[0].length")
            .await
            .unwrap(),
        serde_json::json!(20000)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_dispatches_messageerror_and_recovers() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.workerMessages = [];
globalThis.workerErrors = [];
globalThis.worker = new Worker('/worker-messageerror.js');
worker.onmessage = event => workerMessages.push(event.data);
worker.onerror = event => { workerErrors.push(event.message); event.preventDefault(); };
</script><main>worker messageerror</main>"#;
    let worker_script = r#"
const messageErrorTrace = [];
let currentMessageErrorEvent = null;
const malformedEnvelope = {
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
const messageErrorHandler = function(event) {
  observeMessageError('handler', event);
  globalThis.messageErrorThisIsGlobal = this === self;
};
onmessageerror = messageErrorHandler;
onmessage = event => {
  if (event.data.kind === 'malformed') {
    self.__glassDispatchWorkerMessage(malformedEnvelope);
    postMessage({
      kind: 'decode',
      handlerReadback: onmessageerror === messageErrorHandler,
      handlerThisIsGlobal: messageErrorThisIsGlobal,
      events: messageErrorTrace.splice(0),
    });
    currentMessageErrorEvent = null;
    return;
  }
  postMessage({ kind: 'valid', data: event.data });
};
addEventListener('messageerror', event => observeMessageError('listener', event));
"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/worker-messageerror-page", "text/html", page),
            ("/worker-messageerror.js", "text/javascript", worker_script),
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
            .with_initial_url(format!("http://{address}/worker-messageerror-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("worker.postMessage({ kind: 'malformed' }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut decode_result = None;
    for _ in 0..8 {
        decode_result = engine
            .evaluate_async("workerMessages.find(message => message.kind === 'decode')")
            .await
            .unwrap()
            .as_object()
            .cloned();
        if decode_result.is_some() {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        decode_result.map(serde_json::Value::Object),
        Some(serde_json::json!({
            "kind": "decode",
            "handlerReadback": true,
            "handlerThisIsGlobal": true,
            "events": [
                {
                    "kind": "handler", "sameEvent": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                    "eventPhase": 2, "type": "messageerror", "dataIsNull": true,
                    "portsLength": 0, "origin": "", "sourceIsNull": true,
                    "bubbles": false, "cancelable": false,
                },
                {
                    "kind": "listener", "sameEvent": true,
                    "targetIsGlobal": true, "currentTargetIsGlobal": true,
                    "eventPhase": 2, "type": "messageerror", "dataIsNull": true,
                    "portsLength": 0, "origin": "", "sourceIsNull": true,
                    "bubbles": false, "cancelable": false,
                },
            ],
        }))
    );
    assert_eq!(
        engine.evaluate_async("workerErrors").await.unwrap(),
        serde_json::json!([])
    );

    assert_eq!(
        engine
            .evaluate_async("worker.postMessage({ kind: 'valid-after-error' }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut valid_result = None;
    for _ in 0..8 {
        valid_result = engine
            .evaluate_async("workerMessages.find(message => message.kind === 'valid')")
            .await
            .unwrap()
            .as_object()
            .cloned();
        if valid_result.is_some() {
            break;
        }
        engine
            .evaluate_async("await new Promise(resolve => setTimeout(resolve, 0)); true")
            .await
            .unwrap();
    }
    assert_eq!(
        valid_result.map(serde_json::Value::Object),
        Some(serde_json::json!({
            "kind": "valid", "data": { "kind": "valid-after-error" },
        }))
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_proxy_dispatches_messageerror_and_recovers() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.workerEvents = [];
globalThis.workerMessages = [];
globalThis.workerErrors = [];
globalThis.currentProxyMessageError = null;
globalThis.handlerThisIsWorker = false;
globalThis.worker = new Worker('/worker-proxy-messageerror.js');
const observeMessageError = (kind, event) => {
  if (currentProxyMessageError === null) currentProxyMessageError = event;
  workerEvents.push({
    kind,
    sameEvent: currentProxyMessageError === event,
    type: event.type,
    isMessageEvent: event instanceof MessageEvent,
    targetIsWorker: event.target === worker,
    currentTargetIsWorker: event.currentTarget === worker,
    eventPhase: event.eventPhase,
    dataIsNull: event.data === null,
    portCount: event.ports.length,
    origin: event.origin,
    sourceIsNull: event.source === null,
    bubbles: event.bubbles,
    cancelable: event.cancelable,
  });
};
const messageErrorHandler = function(event) {
  handlerThisIsWorker = this === worker;
  observeMessageError('handler', event);
};
worker.onmessageerror = messageErrorHandler;
worker.addEventListener('messageerror', event => observeMessageError('listener', event));
worker.onmessage = event => workerMessages.push({
  type: event.type,
  isMessageEvent: event instanceof MessageEvent,
  targetIsWorker: event.target === worker,
  currentTargetIsWorker: event.currentTarget === worker,
  eventPhase: event.eventPhase,
  data: event.data,
});
worker.onerror = event => workerErrors.push(event.message);
</script><main>Worker proxy messageerror</main>"#;
    let worker_script = r#"self.onmessage = event => {
  postMessage({ kind: 'valid', value: event.data.value + 1 });
};"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/worker-proxy-messageerror-page", "text/html", page),
            (
                "/worker-proxy-messageerror.js",
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
            .with_initial_url(format!("http://{address}/worker-proxy-messageerror-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"const malformed = {
  data: {
    __glassMessageClone: 'glass-native-structured-clone-v1',
    root: { ref: 0 },
    nodes: [{ type: 'invalid-node' }],
  },
  transfer_ports: [],
  object_urls: [],
  origin: 'https://sender.example',
};
__glassDispatchWorkerMessage(worker.__glassWorkerId, malformed);
({
  workerEvents,
  workerErrors,
  handlerReadback: worker.onmessageerror === messageErrorHandler,
  handlerThisIsWorker,
  eventStateReset: currentProxyMessageError.currentTarget === null
    && currentProxyMessageError.eventPhase === 0,
  workerAlive: !worker.__glassTerminated,
})"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "workerEvents": [
                {
                    "kind": "handler", "sameEvent": true,
                    "type": "messageerror", "isMessageEvent": true,
                    "targetIsWorker": true, "currentTargetIsWorker": true,
                    "eventPhase": 2, "dataIsNull": true, "portCount": 0,
                    "origin": "https://sender.example", "sourceIsNull": true,
                    "bubbles": false, "cancelable": false,
                },
                {
                    "kind": "listener", "sameEvent": true,
                    "type": "messageerror", "isMessageEvent": true,
                    "targetIsWorker": true, "currentTargetIsWorker": true,
                    "eventPhase": 2, "dataIsNull": true, "portCount": 0,
                    "origin": "https://sender.example", "sourceIsNull": true,
                    "bubbles": false, "cancelable": false,
                },
            ],
            "workerErrors": [],
            "handlerReadback": true,
            "handlerThisIsWorker": true,
            "eventStateReset": true,
            "workerAlive": true,
        })
    );

    assert_eq!(
        engine
            .evaluate_async("worker.postMessage({ value: 41 }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut valid_message = serde_json::Value::Null;
    for _ in 0..8 {
        valid_message = engine
            .evaluate_async("workerMessages.length > 0 ? workerMessages[0] : null")
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
            "targetIsWorker": true,
            "currentTargetIsWorker": true,
            "eventPhase": 2,
            "data": { "kind": "valid", "value": 42 },
        })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_blob_object_urls_cross_realm_messages() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/cross-realm-blob-page",
                "text/html",
                "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/cross-realm-blob-worker.js'); worker.onmessage = event => { workerMessages.push(event.data); if (event.data.kind === 'worker-url') { const request = new XMLHttpRequest(); request.open('GET', event.data.url, false); request.send(); workerMessages.push({ kind: 'page-read', text: request.responseText }); } }; const blob = new Blob(['http page to worker'], { type: 'text/page' }); worker.postMessage({ url: URL.createObjectURL(blob) });</script><main>Native</main>",
            ),
            (
                "/cross-realm-blob-worker.js",
                "text/javascript",
                "self.onmessage = async event => { try { const response = await fetch(event.data.url); postMessage({ kind: 'page-to-worker', text: await response.text(), contentType: response.headers.get('content-type') }); const blob = new Blob(['http worker to page'], { type: 'text/worker' }); postMessage({ kind: 'worker-url', url: URL.createObjectURL(blob) }); } catch (error) { postMessage({ error: String(error) }); } };",
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
            .with_initial_url(format!("http://{address}/cross-realm-blob-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let expected_worker_url = format!("blob:http://{address}/glass-worker-1");
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([
            {
                "kind": "page-to-worker",
                "text": "http page to worker",
                "contentType": "text/page"
            },
            {
                "kind": "worker-url",
                "url": expected_worker_url
            },
            {
                "kind": "page-read",
                "text": "http worker to page"
            }
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_transfers_message_ports_between_page_and_worker_realms() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/transferred-port-page",
                "text/html",
                "<script>globalThis.portEvents = []; globalThis.workerMessages = []; globalThis.returnPort = null; globalThis.channel = new MessageChannel(); channel.port2.onmessage = event => portEvents.push({ kind: event.data.kind, value: event.data.value, portCount: event.ports.length }); channel.port2.start(); globalThis.worker = new Worker('/transferred-port-worker.js'); worker.onmessage = event => { workerMessages.push(event.data); if (event.data.kind === 'worker-port') { returnPort = event.ports[0]; returnPort.onmessage = message => portEvents.push({ kind: message.data.kind, value: message.data.value, portCount: message.ports.length }); returnPort.start(); returnPort.postMessage({ value: 8 }); } }; worker.postMessage({ kind: 'connect' }, [channel.port1]);</script><main>Native</main>",
            ),
            (
                "/transferred-port-worker.js",
                "text/javascript",
                "self.onmessage = event => { const port = event.ports[0]; port.onmessage = message => { postMessage({ kind: 'worker-received', value: message.data.value }); port.postMessage({ kind: 'worker-reply', value: message.data.value + 1 }); }; port.start(); port.postMessage({ kind: 'worker-ready', value: 1 }); const returnChannel = new MessageChannel(); returnChannel.port2.onmessage = message => { postMessage({ kind: 'worker-port-received', value: message.data.value }); returnChannel.port2.postMessage({ kind: 'worker-port-reply', value: message.data.value + 1 }); }; returnChannel.port2.start(); postMessage({ kind: 'worker-port', value: 2 }, [returnChannel.port1]); };",
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
            .with_initial_url(format!("http://{address}/transferred-port-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("({ portEvents, workerMessages })")
            .await
            .unwrap(),
        serde_json::json!({
            "portEvents": [{"kind": "worker-ready", "value": 1, "portCount": 0}],
            "workerMessages": [{"kind": "worker-port", "value": 2}],
        })
    );
    assert_eq!(
        engine
            .evaluate_async("channel.port2.postMessage({ value: 4 }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ portEvents, workerMessages })")
            .await
            .unwrap(),
        serde_json::json!({
            "portEvents": [
                {"kind": "worker-ready", "value": 1, "portCount": 0},
                {"kind": "worker-port-reply", "value": 9, "portCount": 0},
                {"kind": "worker-reply", "value": 5, "portCount": 0},
            ],
            "workerMessages": [
                {"kind": "worker-port", "value": 2},
                {"kind": "worker-port-received", "value": 8},
                {"kind": "worker-received", "value": 4},
            ],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_message_ports_transfer_blob_urls_between_page_and_worker() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/message-port-blob-page",
                "text/html",
                "<script>globalThis.portEvents = []; globalThis.channel = new MessageChannel(); channel.port2.onmessage = event => { if (event.data.kind === 'worker-ready') { portEvents.push([event.data.kind]); return; } if (event.data.kind !== 'worker-read') { portEvents.push([event.data.kind, event.data.error]); return; } try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); portEvents.push([event.data.kind, event.data.text, event.data.contentType, xhr.responseText, xhr.getResponseHeader('content-type')]); } catch (error) { portEvents.push(['page-error', String(error)]); } }; channel.port2.start(); globalThis.worker = new Worker('/message-port-blob-worker.js'); worker.postMessage({ kind: 'connect' }, [channel.port1]);</script><main>Native</main>",
            ),
            (
                "/message-port-blob-worker.js",
                "text/javascript",
                "self.onmessage = event => { const port = event.ports[0]; port.onmessage = message => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', message.data.url, false); xhr.send(); const blob = new Blob(['http-worker-port-body'], { type: 'text/http-worker-port' }); port.postMessage({ kind: 'worker-read', text: xhr.responseText, contentType: xhr.getResponseHeader('content-type'), url: URL.createObjectURL(blob) }); } catch (error) { port.postMessage({ kind: 'worker-error', error: String(error) }); } }; port.start(); port.postMessage({ kind: 'worker-ready' }); };",
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
            .with_initial_url(format!("http://{address}/message-port-blob-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("portEvents").await.unwrap(),
        serde_json::json!([["worker-ready"]])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "const blob = new Blob(['http-page-port-body'], { type: 'text/http-page-port' }); channel.port2.postMessage({ url: URL.createObjectURL(blob) }); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("portEvents").await.unwrap(),
        serde_json::json!([
            ["worker-ready",],
            [
                "worker-read",
                "http-page-port-body",
                "text/http-page-port",
                "http-worker-port-body",
                "text/http-worker-port",
            ]
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_transfers_array_buffers_between_page_and_worker_realms() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/array-buffer-transfer-page",
                "text/html",
                "<script>globalThis.senderState = []; globalThis.workerMessages = []; globalThis.worker = new Worker('/array-buffer-transfer-worker.js'); worker.onmessage = event => workerMessages.push(event.data); const buffer = new ArrayBuffer(4); new Uint8Array(buffer).set([4, 5, 6, 7]); const view = new Uint8Array(buffer, 1, 2); worker.postMessage({ buffer, view }, [buffer]); senderState = [buffer.byteLength, view.byteLength];</script><main>Native</main>",
            ),
            (
                "/array-buffer-transfer-worker.js",
                "text/javascript",
                "self.onmessage = event => { const buffer = event.data.buffer; const view = event.data.view; postMessage({ buffer: [buffer instanceof ArrayBuffer, buffer.byteLength, Array.from(new Uint8Array(buffer))], view: [view instanceof Uint8Array, view.byteLength, Array.from(view)], shared: view.buffer === buffer }); };",
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
            .with_initial_url(format!("http://{address}/array-buffer-transfer-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("({ senderState, workerMessages })")
            .await
            .unwrap(),
        serde_json::json!({
            "senderState": [0, 0],
            "workerMessages": [{
                "buffer": [true, 4, [4, 5, 6, 7]],
                "view": [true, 2, [5, 6]],
                "shared": true,
            }],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_initial_worker_startup_fetch_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (path, content_type, body, response_headers) in [
            (
                "/worker-fetch-page",
                "text/html",
                "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-fetch.js'); worker.onmessage = event => workerMessages.push(event.data);</script>",
                concat!(
                    "Set-Cookie: worker_startup_visible=present; Path=/; SameSite=Lax\r\n",
                    "Set-Cookie: worker_startup_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                ),
            ),
            (
                "/worker-fetch.js",
                "text/javascript",
                "fetch('/worker-data').then(async response => postMessage({ kind: 'fetched', text: await response.text(), status: response.status })).catch(error => postMessage({ kind: 'error', message: String(error) })); self.onmessage = async event => { if (event.data !== 'follow-up') return; const after = await fetch('/worker-data-after'); postMessage({ kind: 'follow-up', text: await after.text() }); };",
                "",
            ),
            (
                "/worker-data",
                "text/plain",
                "hello from worker fetch",
                "Set-Cookie: worker_startup_secret=rotated-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
            ),
            ("/worker-data-after", "text/plain", "follow-up received", ""),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(path));
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let response = format!(
                "HTTP/1.1 200 OK\r\n{response_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path.to_owned(), cookie));
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-fetch-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "kind": "fetched",
            "text": "hello from worker fetch",
            "status": 200,
        }])
    );
    engine
        .evaluate_async("worker.postMessage('follow-up')")
        .await
        .unwrap();
    let mut worker_messages = serde_json::Value::Null;
    for _ in 0..8 {
        worker_messages = engine.evaluate_async("workerMessages").await.unwrap();
        if worker_messages.as_array().is_some_and(|messages| {
            messages
                .iter()
                .any(|message| message["kind"] == "follow-up")
        }) {
            break;
        }
    }
    assert_eq!(
        worker_messages,
        serde_json::json!([
            {"kind": "fetched", "text": "hello from worker fetch", "status": 200},
            {"kind": "follow-up", "text": "follow-up received"},
        ])
    );
    let cookies = engine.cookies_async().await.unwrap();
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "worker_startup_secret" && cookie.value == "rotated-secret"
    }));
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("worker_startup_visible=present")
    );
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .unwrap_or_else(|| panic!("missing initial Worker request {path}"))
            .1
            .as_deref()
            .unwrap_or("")
            .to_owned()
    };
    for path in ["/worker-fetch.js", "/worker-data"] {
        assert!(
            cookie_for(path).contains("worker_startup_secret=initial-secret"),
            "parent-selected HttpOnly cookie missing from {path}"
        );
    }
    assert!(
        cookie_for("/worker-data-after").contains("worker_startup_secret=rotated-secret"),
        "parent did not select the startup response cookie for the follow-up request: {requests:?}"
    );
}
#[tokio::test]
async fn native_content_process_worker_fetch_preserves_large_response_payload() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/worker-large-page",
            "/worker-large.js",
            "/worker-large-data",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = match expected_path {
                "/worker-large-page" => (
                    "text/html",
                    "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-large.js'); worker.onmessage = event => workerMessages.push(event.data);</script>".to_owned(),
                ),
                "/worker-large.js" => (
                    "text/javascript",
                    "fetch('/worker-large-data').then(response => response.text()).then(text => postMessage({ length: text.length })).catch(error => postMessage({ error: String(error) }));".to_owned(),
                ),
                "/worker-large-data" => ("text/plain", "x".repeat(20_000)),
                _ => unreachable!(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-large-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{"length": 20_000}])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_fetch_preserves_binary_request_and_response_bodies() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_bytes(&mut stream).await;
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .unwrap();
            let request_text = String::from_utf8_lossy(&request[..header_end]);
            let path = request_text.split_whitespace().nth(1).unwrap();
            match path {
                "/worker-binary-page" => {
                    let body = "<script>globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('/worker-binary.js'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message);</script>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/worker-binary.js" => {
                    let body = r#"(async () => {
  try {
    const bytes = new Uint8Array([0, 255, 1, 254]);
    const requestBody = new File([bytes], 'payload.bin', { type: 'application/octet-stream' });
    const request = new Request('/worker-binary-echo', { method: 'POST', body: requestBody });
    const requestClone = request.clone();
    const response = await fetch(request);
    const bufferResponse = response.clone();
    const blobResponse = response.clone();
    const streamResponse = response.clone();
    const byobResponse = response.clone();
    const cancelResponse = response.clone();
    const iteratorResponse = response.clone();
    const responseBytes = await response.bytes();
    const responseBuffer = await bufferResponse.arrayBuffer();
    const responseBlob = await blobResponse.blob();
    const blobBytes = await responseBlob.bytes();
    const stream = streamResponse.body;
    const reader = stream.getReader();
    const streamFirst = await reader.read();
    const streamEnd = await reader.read();
    reader.releaseLock();
    const byobReader = byobResponse.body.getReader({ mode: 'byob' });
    const byobFirst = await byobReader.read(new Uint8Array(8));
    const byobEnd = await byobReader.read(new Uint8Array(2));
    byobReader.releaseLock();
    const cancelReader = cancelResponse.body.getReader();
    const cancelFirst = await cancelReader.read();
    await cancelReader.cancel('stop');
    const cancelAfter = await cancelReader.read();
    cancelReader.releaseLock();
    const iterator = iteratorResponse.body[Symbol.asyncIterator]();
    const iteratorFirst = await iterator.next();
    const iteratorEnd = await iterator.next();
    const requestCloneBytes = await requestClone.bytes();
    const responseObject = new Response(new Uint8Array([9, 8, 7]), { status: 201, headers: { 'X-Worker-Response': 'yes' } });
    const responseClone = responseObject.clone();
    const responseObjectBytes = await responseClone.bytes();
    const makeStream = () => new ReadableStream({
      start(controller) {
        controller.enqueue('worker-');
      },
      async pull(controller) {
        await Promise.resolve();
        controller.enqueue('stream');
        controller.close();
      }
    });
    const streamRequest = new Request('/worker-local-stream', { method: 'POST', body: makeStream() });
    const streamRequestClone = streamRequest.clone();
    const streamRequestInitial = [streamRequest.bodyUsed, streamRequestClone.bodyUsed];
    const streamRequestText = await streamRequest.text();
    const streamRequestCloneText = await streamRequestClone.text();
    const constructedResponse = new Response(makeStream(), { status: 202 });
    const constructedResponseClone = constructedResponse.clone();
    const constructedResponseInitial = [constructedResponse.bodyUsed, constructedResponseClone.bodyUsed];
    const constructedResponseText = await constructedResponse.text();
    const constructedResponseCloneText = await constructedResponseClone.text();
    const streamedFetchRequest = new Request('/worker-stream-echo', { method: 'POST', body: makeStream() });
    const streamedFetchResponse = await fetch(streamedFetchRequest);
    const streamedOptionsResponse = await fetch('/worker-stream-echo', { method: 'POST', body: makeStream() });
    postMessage({
      kind: 'binary',
      requestBytes: Array.from(bytes),
      responseBytes: Array.from(responseBytes),
      responseBuffer: Array.from(new Uint8Array(responseBuffer)),
      blobBytes: Array.from(blobBytes),
      blobSize: responseBlob.size,
      blobType: responseBlob.type,
      streamBody: stream instanceof ReadableStream,
      streamFirst: Array.from(streamFirst.value),
      streamFirstDone: streamFirst.done,
      streamEndDone: streamEnd.done,
      streamBodyUsed: streamResponse.bodyUsed,
      byobFirst: Array.from(byobFirst.value),
      byobFirstDone: byobFirst.done,
      byobEndDone: byobEnd.done,
      byobBodyUsed: byobResponse.bodyUsed,
      cancelFirst: Array.from(cancelFirst.value),
      cancelFirstDone: cancelFirst.done,
      cancelAfterDone: cancelAfter.done,
      cancelBodyUsed: cancelResponse.bodyUsed,
      iteratorFirst: Array.from(iteratorFirst.value),
      iteratorFirstDone: iteratorFirst.done,
      iteratorEndDone: iteratorEnd.done,
      iteratorBodyUsed: iteratorResponse.bodyUsed,
      requestIdentity: [request instanceof Request, request.headers instanceof Headers, request.method, request.headers.get('content-type'), request.bodyUsed, Array.from(requestCloneBytes), requestClone.bodyUsed],
      responseIdentity: [responseObject instanceof Response, responseObject.headers instanceof Headers, responseObject.status, responseObject.headers.get('x-worker-response'), Array.from(responseObjectBytes), responseObject.bodyUsed, responseClone.bodyUsed],
      streamRequest: [streamRequestInitial, streamRequestText, streamRequestCloneText, streamRequest.bodyUsed, streamRequestClone.bodyUsed],
      streamResponse: [constructedResponseInitial, constructedResponseText, constructedResponseCloneText, constructedResponse.bodyUsed, constructedResponseClone.bodyUsed],
      streamFetch: [await streamedFetchResponse.text(), await streamedOptionsResponse.text(), streamedFetchRequest.bodyUsed],
      bodyUsed: response.bodyUsed,
      bufferBodyUsed: bufferResponse.bodyUsed,
      blobBodyUsed: blobResponse.bodyUsed,
    });
  } catch (error) {
    postMessage({ kind: 'error', message: String(error) });
  }
})();"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/worker-binary-echo" => {
                    assert_eq!(&request[header_end..], &[0, 255, 1, 254]);
                    let body = [0_u8, 255, 1, 254];
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.write_all(&body).await.unwrap();
                }
                "/worker-stream-echo" => {
                    assert!(
                        request_text
                            .lines()
                            .any(|line| line.trim().eq_ignore_ascii_case("content-length: 13"))
                    );
                    assert_eq!(&request[header_end..], b"worker-stream");
                    let body = b"worker-stream";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.write_all(body).await.unwrap();
                }
                path => panic!("unexpected native Worker binary request path: {path}"),
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-binary-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let worker_errors = engine.evaluate_async("workerErrors").await.unwrap();
    assert_eq!(worker_errors, serde_json::json!([]));
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "kind": "binary",
            "requestBytes": [0, 255, 1, 254],
            "responseBytes": [0, 255, 1, 254],
            "responseBuffer": [0, 255, 1, 254],
            "blobBytes": [0, 255, 1, 254],
            "blobSize": 4,
            "blobType": "application/octet-stream",
            "streamBody": true,
            "streamFirst": [0, 255, 1, 254],
            "streamFirstDone": false,
            "streamEndDone": true,
            "streamBodyUsed": true,
            "byobFirst": [0, 255, 1, 254],
            "byobFirstDone": false,
            "byobEndDone": true,
            "byobBodyUsed": true,
            "cancelFirst": [0, 255, 1, 254],
            "cancelFirstDone": false,
            "cancelAfterDone": true,
            "cancelBodyUsed": true,
            "iteratorFirst": [0, 255, 1, 254],
            "iteratorFirstDone": false,
            "iteratorEndDone": true,
            "iteratorBodyUsed": true,
            "requestIdentity": [true, true, "POST", "application/octet-stream", true, [0, 255, 1, 254], true],
            "responseIdentity": [true, true, 201, "yes", [9, 8, 7], false, true],
            "streamRequest": [[false, false], "worker-stream", "worker-stream", true, true],
            "streamResponse": [[false, false], "worker-stream", "worker-stream", true, true],
            "streamFetch": ["worker-stream", "worker-stream", true],
            "bodyUsed": true,
            "bufferBodyUsed": true,
            "blobBodyUsed": true,
        }])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_location_navigation_crosses_worker_boundary() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, body) in [
            ("/start", "<title>Start</title><p>Initial</p>"),
            ("/destination", "<title>Destination</title><p>Arrived</p>"),
            ("/replaced", "<title>Replaced</title><p>Rewritten</p>"),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/start")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(engine.history().len(), 1);
    assert_eq!(
        engine
            .evaluate_async("[location.href, location.origin, location.pathname]")
            .await
            .unwrap(),
        serde_json::json!([
            format!("http://{address}/start"),
            format!("http://{address}"),
            "/start"
        ])
    );

    engine
        .evaluate_async("location.assign('/destination')")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/destination")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Destination");
    assert_eq!(engine.history().len(), 2);

    engine
        .evaluate_async("location.replace('/replaced')")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/replaced")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Replaced");
    assert_eq!(engine.history().len(), 2);
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_delivers_report_only_connect_events_for_websocket() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-csp-websocket-page")
        );
        let body = "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-csp-websocket.js'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerMessages.push(['worker-error', String(event.message || '')]);</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-csp-websocket.js")
        );
        let body = format!(
            "const reports = []; addEventListener('securitypolicyviolation', event => reports.push([event.effectiveDirective, event.blockedURI, event.disposition])); const socket = new WebSocket('ws://{address}/socket'); socket.onmessage = event => {{ postMessage([event.data, reports]); socket.close(); }}; socket.onerror = event => postMessage(['error', reports, String(event.message || '')]);"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Security-Policy-Report-Only: connect-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        websocket
            .send(Message::Text("worker-websocket-report-ok".into()))
            .await
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), websocket.next()).await;
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-csp-websocket-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut messages = serde_json::Value::Null;
    for _ in 0..20 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages
            == serde_json::json!([[
                "worker-websocket-report-ok",
                [["connect-src", format!("ws://{address}/socket"), "report"]]
            ]])
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        messages,
        serde_json::json!([[
            "worker-websocket-report-ok",
            [["connect-src", format!("ws://{address}/socket"), "report"]]
        ]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_delivers_report_only_connect_events_for_event_source() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/worker-csp-page"));
        let body = "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-csp.js'); worker.onmessage = event => workerMessages.push(event.data);</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/worker-csp.js"));
        let body = "const reports = []; addEventListener('securitypolicyviolation', event => reports.push([event.effectiveDirective, event.blockedURI, event.disposition])); const source = new EventSource('/worker-events'); source.onmessage = event => { postMessage([event.data, reports]); source.close(); }; source.onerror = event => postMessage(['error', String(event.message || '')]);";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Security-Policy-Report-Only: connect-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/worker-events"));
        let body = "data: worker-event-source-ok\r\n\r\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/worker-csp-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut messages = serde_json::Value::Null;
    for _ in 0..20 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages
            == serde_json::json!([[
                "worker-event-source-ok",
                [[
                    "connect-src",
                    format!("http://{address}/worker-events"),
                    "report"
                ]]
            ]])
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        messages,
        serde_json::json!([[
            "worker-event-source-ok",
            [[
                "connect-src",
                format!("http://{address}/worker-events"),
                "report"
            ]]
        ]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_message_fetch_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..5 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("worker parent Fetch should reach the local server")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("request includes a path")
                .to_owned();
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (headers, content_type, body) = match path.as_str() {
                "/page" => (
                    concat!(
                        "Set-Cookie: parent_session=initial; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: parent_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<p>page</p>",
                ),
                "/worker.js" => (
                    "",
                    "text/javascript",
                    "self.onmessage = async event => { if (event.ports.length) { const port = event.ports[0]; port.onmessage = async () => { const response = await fetch('/port-data'); port.postMessage(await response.text()); }; port.start(); return; } const response = await fetch('/worker-data'); postMessage({ kind: 'message', text: await response.text() }); }; fetch('/worker-start-data').then(async response => postMessage({ kind: 'started', text: await response.text() }));",
                ),
                "/worker-start-data" => (
                    "Set-Cookie: worker_parent=updated; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "startup response",
                ),
                "/worker-data" => ("", "text/plain", "worker response"),
                "/port-data" => ("", "text/plain", "port response"),
                other => panic!("unexpected parent-owned worker Fetch path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async(
            "globalThis.workerMessages = []; document.cookie = 'same_turn=present; Path=/'; globalThis.worker = new Worker('/worker.js'); worker.onmessage = event => workerMessages.push(event.data); true",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.cookie = 'message_turn=present; Path=/'; worker.postMessage('fetch'); true",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "globalThis.portMessages = []; globalThis.channel = new MessageChannel(); channel.port2.onmessage = event => portMessages.push(event.data); channel.port2.start(); worker.postMessage('connect', [channel.port1]); true",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.cookie = 'port_turn=present; Path=/'; channel.port2.postMessage('fetch'); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([
            {"kind": "started", "text": "startup response"},
            {"kind": "message", "text": "worker response"}
        ])
    );
    assert_eq!(
        engine.evaluate_async("portMessages").await.unwrap(),
        serde_json::json!(["port response"])
    );
    let cookies = engine.cookies_async().await.unwrap();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "worker_parent" && cookie.value == "updated")
    );
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        [
            "/page",
            "/worker.js",
            "/worker-start-data",
            "/worker-data",
            "/port-data"
        ]
    );
    let startup_fetch_cookie = requests[2].1.as_deref().unwrap_or_default();
    assert!(startup_fetch_cookie.contains("parent_session=initial"));
    assert!(startup_fetch_cookie.contains("parent_secret=initial-secret"));
    assert!(startup_fetch_cookie.contains("same_turn=present"));
    let message_fetch_cookie = requests[3].1.as_deref().unwrap_or_default();
    assert!(message_fetch_cookie.contains("parent_session=initial"));
    assert!(message_fetch_cookie.contains("parent_secret=initial-secret"));
    assert!(message_fetch_cookie.contains("worker_parent=updated"));
    assert!(message_fetch_cookie.contains("message_turn=present"));
    let port_fetch_cookie = requests[4].1.as_deref().unwrap_or_default();
    assert!(port_fetch_cookie.contains("parent_session=initial"));
    assert!(port_fetch_cookie.contains("parent_secret=initial-secret"));
    assert!(port_fetch_cookie.contains("worker_parent=updated"));
    assert!(port_fetch_cookie.contains("message_turn=present"));
    assert!(port_fetch_cookie.contains("port_turn=present"));
}
#[tokio::test]
async fn native_content_process_worker_stream_upload_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for expected_path in [
            "/page",
            "/worker-stream.js",
            "/worker-upload",
            "/after-upload",
        ] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("parent-brokered Worker upload should reach the local server")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            if expected_path == "/worker-upload" {
                assert!(request.starts_with("POST "));
                assert!(request.ends_with("worker-pull-body"));
                assert!(
                    cookie
                        .as_deref()
                        .unwrap_or_default()
                        .contains("worker_seed=initial")
                );
            }
            let (headers, content_type, body) = match expected_path {
                "/page" => (
                    "Set-Cookie: worker_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    "<p>page</p>",
                ),
                "/worker-stream.js" => (
                    "",
                    "text/javascript",
                    r#"(async () => {
  const stream = new ReadableStream({
    pull(controller) {
      controller.enqueue('worker-pull-body');
      controller.close();
    },
  });
  const uploaded = await fetch('/worker-upload', {
    method: 'POST',
    headers: { 'Content-Type': 'text/plain' },
    body: stream,
  });
  const uploadedText = await uploaded.text();
  const followup = await fetch('/after-upload');
  postMessage([uploadedText, await followup.text()]);
})().catch(error => postMessage({ error: String(error) }));"#,
                ),
                "/worker-upload" => (
                    "Set-Cookie: worker_parent=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "uploaded",
                ),
                "/after-upload" => ("", "text/plain", "after"),
                _ => unreachable!(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((expected_path, cookie));
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "document.cookie = 'worker_turn=sent; Path=/'; globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-stream.js'); worker.onmessage = event => workerMessages.push(event.data); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([["uploaded", "after"]])
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();
    let upload_cookie = requests[2].1.as_deref().unwrap_or_default();
    assert!(upload_cookie.contains("worker_seed=initial"));
    assert!(
        upload_cookie.contains("worker_turn=sent"),
        "stream upload did not apply the explicit page turn's parent-owned cookie write: {upload_cookie}"
    );
    let after_upload_cookie = requests[3].1.as_deref().unwrap_or_default();
    assert!(
        after_upload_cookie.contains("worker_parent=accepted"),
        "follow-up Worker Fetch did not see the parent Set-Cookie update: {after_upload_cookie}"
    );
    assert!(after_upload_cookie.contains("worker_turn=sent"));
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "worker_parent" && cookie.value == "accepted")
    );
}
#[test]
fn native_content_process_page_and_worker_websocket_failed_handshakes_keep_parent_cookies() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-websocket-failed-handshake-cookies-{}-profile.json",
                std::process::id()
            ));
            let cookie_profile_path =
                std::path::PathBuf::from(format!("{}.cookies", profile_path.display()));
            let profile_lock_path = profile_path.with_extension("lock");
            let cookie_profile_lock_path = cookie_profile_path.with_extension("lock");
            let worker_clients_path = profile_path.with_extension("clients");
            let worker_events_path = profile_path.with_extension("events");
            let worker_readers_path = profile_path.with_extension("readers");
            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &worker_clients_path,
                &worker_events_path,
                &worker_readers_path,
            ] {
                let _ = fs::remove_file(path);
            }

            let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let page_address = page_listener.local_addr().unwrap();
            let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let api_address = api_listener.local_addr().unwrap();
            let page_origin = format!("http://{page_address}");
            let api_origin = format!("http://{api_address}");
            let page_api_origin = api_origin.replacen("http://", "ws://", 1);
            let expected_origin = page_origin.clone();
            let response_origin = page_origin.clone();
            let handshake_response_sent = Arc::new(AtomicBool::new(false));
            let handshake_response_sent_by_server = Arc::clone(&handshake_response_sent);
            let worker_handshake_response_sent = Arc::new(AtomicBool::new(false));
            let worker_handshake_response_sent_by_server =
                Arc::clone(&worker_handshake_response_sent);
            let worker_script_served = Arc::new(AtomicBool::new(false));
            let worker_script_served_by_server = Arc::clone(&worker_script_served);

            let page_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(30), async move {
                    let (mut stream, _) = page_listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    assert_eq!(request.split_whitespace().next(), Some("GET"));
                    assert_eq!(request.split_whitespace().nth(1), Some("/page"));
                    let body = format!(
                        r#"<script>
globalThis.websocketEvents = [];
globalThis.workerEvents = [];
globalThis.socket = new WebSocket("{page_api_origin}/reject");
socket.addEventListener('open', () => websocketEvents.push('open'));
socket.addEventListener('error', () => {{
  websocketEvents.push('error');
globalThis.worker = new Worker('/worker.js');
  worker.addEventListener('message', event => workerEvents.push(event.data));
  worker.addEventListener('error', () => workerEvents.push('worker-error'));
}});
</script>"#
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: websocket_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();

                    let (mut worker_script_stream, _) = tokio::time::timeout(
                        Duration::from_secs(20),
                        page_listener.accept(),
                    )
                    .await
                    .expect("DedicatedWorker script request reaches page origin")
                    .unwrap();
                    let worker_script_request = read_http_request(&mut worker_script_stream).await;
                    assert_eq!(worker_script_request.split_whitespace().next(), Some("GET"));
                    assert_eq!(worker_script_request.split_whitespace().nth(1), Some("/worker.js"));
                    let worker_script = format!(
                        "const socket = new WebSocket('{page_api_origin}/reject-worker'); socket.addEventListener('open', () => postMessage('open')); socket.addEventListener('error', () => postMessage('error'));"
                    );
                    let worker_script_response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{worker_script}",
                        worker_script.len()
                    );
                    worker_script_stream
                        .write_all(worker_script_response.as_bytes())
                        .await
                        .unwrap();
                    worker_script_served_by_server.store(true, Ordering::SeqCst);
                })
                .await
                .expect("page and DedicatedWorker script responses stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    let (mut handshake_stream, _) = tokio::time::timeout(
                        Duration::from_secs(30),
                        api_listener.accept(),
                    )
                    .await
                    .expect("parent WebSocket handshake reaches API")
                    .unwrap();
                    let handshake = read_http_request(&mut handshake_stream).await;
                    assert_eq!(handshake.split_whitespace().next(), Some("GET"));
                    assert_eq!(handshake.split_whitespace().nth(1), Some("/reject"));
                    assert_eq!(header_value(&handshake, "origin"), Some(expected_origin.as_str()));
                    assert!(header_value(&handshake, "upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket")));
                    assert!(header_value(&handshake, "sec-websocket-key").is_some());
                    let handshake_cookie = header_value(&handshake, "cookie").unwrap_or_default();
                    assert!(handshake_cookie.contains("websocket_seed=initial"), "WebSocket handshake missed parent seed: {handshake_cookie}");
                    assert!(!handshake_cookie.contains("websocket_page_rejected=retained"));
                    assert!(!handshake_cookie.contains("websocket_worker_rejected=retained"));
                    let rejected_body = "handshake denied";
                    let rejected_response = format!(
                        "HTTP/1.1 403 Forbidden\r\nSet-Cookie: websocket_page_rejected=retained; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{rejected_body}",
                        rejected_body.len()
                    );
                    handshake_stream
                        .write_all(rejected_response.as_bytes())
                        .await
                        .unwrap();
                    handshake_response_sent_by_server.store(true, Ordering::SeqCst);

                    let (mut worker_handshake_stream, _) = tokio::time::timeout(
                        Duration::from_secs(30),
                        api_listener.accept(),
                    )
                    .await
                    .expect("DedicatedWorker WebSocket handshake reaches API")
                    .unwrap();
                    let worker_handshake = read_http_request(&mut worker_handshake_stream).await;
                    assert_eq!(worker_handshake.split_whitespace().next(), Some("GET"));
                    assert_eq!(worker_handshake.split_whitespace().nth(1), Some("/reject-worker"));
                    assert_eq!(header_value(&worker_handshake, "origin"), Some(expected_origin.as_str()));
                    assert!(header_value(&worker_handshake, "upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket")));
                    assert!(header_value(&worker_handshake, "sec-websocket-key").is_some());
                    let worker_handshake_cookie =
                        header_value(&worker_handshake, "cookie").unwrap_or_default();
                    for name in [
                        "websocket_seed=initial",
                        "websocket_page_rejected=retained",
                    ] {
                        assert!(worker_handshake_cookie.contains(name), "DedicatedWorker handshake missed parent cookie {name}: {worker_handshake_cookie}");
                    }
                    assert!(!worker_handshake_cookie.contains("websocket_worker_rejected=retained"));
                    let rejected_body = "worker handshake denied";
                    let rejected_response = format!(
                        "HTTP/1.1 403 Forbidden\r\nSet-Cookie: websocket_worker_rejected=retained; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{rejected_body}",
                        rejected_body.len()
                    );
                    worker_handshake_stream
                        .write_all(rejected_response.as_bytes())
                        .await
                        .unwrap();
                    worker_handshake_response_sent_by_server.store(true, Ordering::SeqCst);

                    let (mut verify_stream, _) = tokio::time::timeout(
                        Duration::from_secs(30),
                        api_listener.accept(),
                    )
                    .await
                    .expect("later authorized page Fetch reaches API")
                    .unwrap();
                    let verify = read_http_request(&mut verify_stream).await;
                    assert_eq!(verify.split_whitespace().next(), Some("GET"));
                    assert_eq!(verify.split_whitespace().nth(1), Some("/verify"));
                    assert_eq!(header_value(&verify, "origin"), Some(expected_origin.as_str()));
                    let verify_cookie = header_value(&verify, "cookie").unwrap_or_default();
                    for name in [
                        "websocket_seed=initial",
                        "websocket_page_rejected=retained",
                        "websocket_worker_rejected=retained",
                    ] {
                        assert!(verify_cookie.contains(name), "authorized Fetch missed parent cookie {name}: {verify_cookie}");
                    }
                    let body = "authorized";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    verify_stream.write_all(response.as_bytes()).await.unwrap();

                    match tokio::time::timeout(Duration::from_millis(1000), api_listener.accept())
                        .await
                    {
                        Err(_) => {}
                        Ok(Ok((mut stream, _))) => {
                            let request = read_http_request(&mut stream).await;
                            panic!("unexpected WebSocket retry or redirect: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("page and Worker failed WebSocket handshake cookie flow stays bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if handshake_response_sent.load(Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("API sends the rejected page WebSocket response");
            let mut socket_state = serde_json::Value::Null;
            let worker_event_result = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    socket_state = session
                        .script("({readyState: socket.readyState, events: websocketEvents, workerEvents})")
                        .await
                        .unwrap()
                        .value;
                    if socket_state["readyState"] == serde_json::json!(3)
                        && socket_state["workerEvents"]
                            .as_array()
                            .is_some_and(|events| events.contains(&serde_json::json!("error")))
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await;
            assert!(
                worker_event_result.is_ok(),
                "page/Worker WebSocket failure events timed out: state={socket_state:?}, worker_script_served={}, worker_response_sent={}, api_server_finished={}",
                worker_script_served.load(Ordering::SeqCst),
                worker_handshake_response_sent.load(Ordering::SeqCst),
                api_server.is_finished()
            );
            assert!(
                worker_handshake_response_sent.load(Ordering::SeqCst),
                "DedicatedWorker error arrived without an HTTP handshake response"
            );
            let events = socket_state["events"]
                .as_array()
                .expect("WebSocket events are an array");
            assert!(events.iter().any(|event| event == "error"));
            assert!(!events.iter().any(|event| event == "open"));
            let worker_events = socket_state["workerEvents"]
                .as_array()
                .expect("DedicatedWorker WebSocket events are an array");
            assert!(worker_events.contains(&serde_json::json!("error")));
            assert!(!worker_events.contains(&serde_json::json!("open")));
            assert_eq!(
                session
                    .script(&format!(
                        "await fetch('{api_origin}/verify', {{ credentials: 'include' }}).then(response => response.text())"
                    ))
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("authorized")
            );
            assert_eq!(
                session.script("document.cookie").await.unwrap().value,
                serde_json::json!("")
            );
            let cookies = session.native_cookies().await.unwrap();
            session.close().await.unwrap();
            page_server.await.unwrap();
            api_server.await.unwrap();
            for (name, value) in [
                ("websocket_seed", "initial"),
                ("websocket_page_rejected", "retained"),
                ("websocket_worker_rejected", "retained"),
            ] {
                assert!(cookies.iter().any(|cookie| {
                    cookie.name == name && cookie.value == value && cookie.http_only
                }));
            }

            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &worker_clients_path,
                &worker_events_path,
                &worker_readers_path,
            ] {
                let _ = fs::remove_file(path);
            }
        });
    });
}
#[tokio::test]
async fn native_content_process_classic_import_scripts_use_worker_policy_and_root_url() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut requests = Vec::new();
            for _ in 0..10 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("classic Worker importScripts request includes a URL")
                    .to_owned();
                let referer = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("referer")
                        .then(|| value.trim().to_owned())
                });
                let (extra_headers, content_type, body) = match path.as_str() {
                    "/page" => (
                        "",
                        "text/html",
                        "<script>globalThis.workerResults = {}; globalThis.workerErrors = []; globalThis.registrationState = 'pending'; const dedicated = new Worker('/workers/dedicated/root.js'); dedicated.onmessage = event => workerResults.dedicated = event.data; dedicated.onerror = event => workerErrors.push(String(event.message)); const shared = new SharedWorker('/workers/shared/root.js'); shared.port.onmessage = event => workerResults.shared = event.data; shared.port.start(); navigator.serviceWorker.register('/workers/service/root.js', { scope: '/workers/service/' }).then(() => registrationState = 'ready', error => registrationState = String(error));</script>",
                    ),
                    "/workers/dedicated/root.js" => (
                        "Referrer-Policy: no-referrer\r\n",
                        "application/javascript",
                        "importScripts('bridge/bridge.js'); postMessage({ kind: 'dedicated', value: globalThis.dedicatedValue });",
                    ),
                    "/workers/dedicated/bridge/bridge.js" => (
                        "Referrer-Policy: unsafe-url\r\n",
                        "application/javascript",
                        "importScripts('leaf.js'); globalThis.dedicatedBridge = true;",
                    ),
                    "/workers/dedicated/leaf.js" => (
                        "",
                        "application/javascript",
                        "globalThis.dedicatedValue = 'dedicated-loaded';",
                    ),
                    "/workers/shared/root.js" => (
                        "Referrer-Policy: origin\r\n",
                        "application/javascript",
                        "importScripts('bridge/bridge.js'); onconnect = event => event.ports[0].postMessage({ kind: 'shared', value: globalThis.sharedValue });",
                    ),
                    "/workers/shared/bridge/bridge.js" => (
                        "Referrer-Policy: unsafe-url\r\n",
                        "application/javascript",
                        "importScripts('leaf.js'); globalThis.sharedBridge = true;",
                    ),
                    "/workers/shared/leaf.js" => (
                        "",
                        "application/javascript",
                        "globalThis.sharedValue = 'shared-loaded';",
                    ),
                    "/workers/service/root.js" => (
                        "Referrer-Policy: future-policy\r\n",
                        "application/javascript",
                        "importScripts('bridge/bridge.js'); self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));",
                    ),
                    "/workers/service/bridge/bridge.js" => (
                        "Referrer-Policy: unsafe-url\r\n",
                        "application/javascript",
                        "importScripts('leaf.js'); globalThis.serviceBridge = true;",
                    ),
                    "/workers/service/leaf.js" => (
                        "",
                        "application/javascript",
                        "globalThis.serviceValue = 'service-loaded';",
                    ),
                    other => panic!("unexpected classic importScripts request: {other}"),
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((path, referer));
            }
            requests.sort();
            requests
        })
        .await
        .expect("classic Worker importScripts policy requests stay within their time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut result = serde_json::Value::Null;
    for _ in 0..12 {
        result = engine
            .evaluate_async(
                "({ results: Object.values(workerResults).sort((a, b) => String(a.kind).localeCompare(String(b.kind))), registration: registrationState, errors: workerErrors })",
            )
            .await
            .unwrap();
        if result["results"]
            .as_array()
            .is_some_and(|results| results.len() == 2)
            && result["registration"] == "ready"
        {
            break;
        }
    }
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        result,
        serde_json::json!({
            "results": [
                {"kind": "dedicated", "value": "dedicated-loaded"},
                {"kind": "shared", "value": "shared-loaded"},
            ],
            "registration": "ready",
            "errors": [],
        }),
        "all classic worker kinds must execute their fully preloaded nested importScripts graph"
    );
    let expected_paths = [
        "/page",
        "/workers/dedicated/root.js",
        "/workers/dedicated/bridge/bridge.js",
        "/workers/dedicated/leaf.js",
        "/workers/shared/root.js",
        "/workers/shared/bridge/bridge.js",
        "/workers/shared/leaf.js",
        "/workers/service/root.js",
        "/workers/service/bridge/bridge.js",
        "/workers/service/leaf.js",
    ];
    let mut actual_paths = requests
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    actual_paths.sort_unstable();
    let mut expected_paths = expected_paths.to_vec();
    expected_paths.sort_unstable();
    assert_eq!(actual_paths, expected_paths);

    let origin = format!("http://{address}");
    let referer_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .unwrap_or_else(|| panic!("missing request {path}"))
            .1
            .as_deref()
            .map(str::to_owned)
    };
    for path in [
        "/workers/dedicated/bridge/bridge.js",
        "/workers/dedicated/leaf.js",
    ] {
        assert_eq!(referer_for(path), None, "no-referrer policy for {path}");
    }
    for path in [
        "/workers/shared/bridge/bridge.js",
        "/workers/shared/leaf.js",
    ] {
        assert_eq!(
            referer_for(path),
            Some(format!("{origin}/")),
            "the SharedWorker root's origin policy must persist through nested imports for {path}"
        );
    }
    let service_worker_referrer = format!("{origin}/workers/service/root.js");
    for path in [
        "/workers/service/bridge/bridge.js",
        "/workers/service/leaf.js",
    ] {
        assert_eq!(
            referer_for(path),
            Some(service_worker_referrer.clone()),
            "unknown root policy falls back to strict-origin-when-cross-origin, and imported response policy does not replace it for {path}"
        );
    }
}
#[tokio::test]
async fn native_content_process_recovers_after_worker_exit_during_startup() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<title>Recovered</title><p>Fresh child</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let previous_worker = std::env::var_os("GLASS_NATIVE_CONTENT_WORKER");
    // `/bin/false` exits before answering the first ping. This is an isolated
    // startup-failure injection; the test restores the process environment
    // before asking the still-New engine to initialize again.
    unsafe {
        std::env::set_var("GLASS_NATIVE_CONTENT_WORKER", "/bin/false");
    }
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/recover")),
    )
    .unwrap();
    let first_error = engine.initialize_async().await.unwrap_err();
    assert!(matches!(
        first_error,
        NativeEngineError::WorkerFailure {
            kind: NativeWorkerFailureKind::Exited | NativeWorkerFailureKind::Transport,
            ..
        }
    ));
    unsafe {
        if let Some(previous_worker) = previous_worker {
            std::env::set_var("GLASS_NATIVE_CONTENT_WORKER", previous_worker);
        } else {
            std::env::remove_var("GLASS_NATIVE_CONTENT_WORKER");
        }
    }

    engine.initialize_async().await.unwrap();
    let evidence = engine.snapshot().unwrap();
    assert_eq!(evidence.title, "Recovered");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_indexed_db_round_trips_through_worker_ipc() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-content.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/indexed-db"));
        let body = "<p>IndexedDB content owner</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/indexed-db")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('content', 1); request.onupgradeneeded = event => { const store = event.target.result.createObjectStore('records'); store.createIndex('byStatus', 'status'); }; const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const tx = db.transaction('records', 'readwrite'); tx.objectStore('records').put({ ready: true, status: 'ready', buffer: new Uint8Array([3, 5, 8]).buffer, bytes: new Uint16Array([258, 513]) }, 'status'); await new Promise((resolve, reject) => { tx.oncomplete = resolve; tx.onerror = () => reject(tx.error); }); const records = db.transaction('records', 'readonly').objectStore('records'); const read = records.index('byStatus').get('ready'); return await new Promise((resolve, reject) => { read.onsuccess = () => { const value = read.result; resolve({ version: db.version, stores: records.indexNames, value: { ready: value.ready, status: value.status }, buffer: Array.from(new Uint8Array(value.buffer)), typedArray: Array.from(value.bytes) }); }; read.onerror = () => reject(read.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "version": 1,
            "stores": ["byStatus"],
            "value": {"ready": true, "status": "ready"},
            "buffer": [3, 5, 8],
            "typedArray": [258, 513],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();

    let profile = fs::read_to_string(&profile_path).unwrap();
    assert!(profile.contains("indexed_db"));
    assert!(profile.contains("status"));
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_content_process_indexed_db_journal_merges_live_workers() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-content-journal.json",
        std::process::id()
    ));
    let observer_path = profile_path
        .parent()
        .unwrap()
        .join(".")
        .join(profile_path.file_name().unwrap());
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [
        profile_path.clone(),
        lock_path.clone(),
        events_path.clone(),
        readers_path.clone(),
    ] {
        let _ = fs::remove_file(path);
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/page"));
            let body = "<p>Content IndexedDB journal</p>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let url = format!("http://{address}/page");
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_initial_url(url.clone());
    let mut first = NativeEngine::new(config.clone()).unwrap();
    let mut second = NativeEngine::new(config.with_storage_path(observer_path)).unwrap();
    first.initialize_async().await.unwrap();
    second.initialize_async().await.unwrap();

    first
        .evaluate_async(
            "await (async () => { const request = indexedDB.open('alpha', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const write = db.transaction('records', 'readwrite').objectStore('records').put({ owner: 'first' }, 'one'); return await new Promise((resolve, reject) => { write.onsuccess = () => resolve(true); write.onerror = () => reject(write.error); }); })()",
        )
        .await
        .unwrap();
    second
        .evaluate_async(
            "await (async () => { const request = indexedDB.open('beta', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const write = db.transaction('records', 'readwrite').objectStore('records').put({ owner: 'second' }, 'two'); return await new Promise((resolve, reject) => { write.onsuccess = () => resolve(true); write.onerror = () => reject(write.error); }); })()",
        )
        .await
        .unwrap();
    assert_eq!(
        first
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('beta'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const read = db.transaction('records', 'readonly').objectStore('records').get('two'); return await new Promise((resolve, reject) => { read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!({"owner": "second"})
    );

    second.close_async().await.unwrap();
    first.close_async().await.unwrap();
    server.await.unwrap();
    for path in [profile_path, lock_path, events_path, readers_path] {
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
// tungstenite fixes the handshake error to its full HTTP response type.
#[allow(clippy::result_large_err)]
async fn native_content_process_worker_drives_websocket_text_binary_and_close_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let websocket_cookie_seen = Arc::new(AtomicBool::new(false));
    let websocket_cookie_seen_by_server = Arc::clone(&websocket_cookie_seen);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-websocket-page")
        );
        let body = "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-websocket.js'); worker.onmessage = event => workerMessages.push(event.data);</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: worker-page-cookie=worker-secret; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-websocket.js")
        );
        let body = format!(
            "const values = []; const socket = new WebSocket('ws://{address}/socket'); socket.binaryType = 'arraybuffer'; socket.onopen = () => {{ socket.send('worker-text'); socket.send(new Uint8Array([1, 2, 3])); }}; socket.onmessage = event => {{ values.push(typeof event.data === 'string' ? event.data : Array.from(new Uint8Array(event.data)).join(',')); if (values.length === 2) {{ postMessage(values); socket.close(1000, 'done'); }} }}; socket.onerror = () => postMessage(['error']);"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let websocket_cookie_seen = Arc::clone(&websocket_cookie_seen_by_server);
        let mut websocket = accept_hdr_async(
            stream,
            move |request: &WebSocketRequest, response: WebSocketResponse| {
                let cookie = request
                    .headers()
                    .get(COOKIE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default();
                websocket_cookie_seen.store(
                    cookie.contains("worker-page-cookie=worker-secret"),
                    Ordering::SeqCst,
                );
                Ok(response)
            },
        )
        .await
        .unwrap();
        websocket
            .send(Message::Ping(b"worker-keepalive".to_vec().into()))
            .await
            .unwrap();
        websocket
            .send(Message::Text("server-text".into()))
            .await
            .unwrap();
        websocket
            .send(Message::Binary(vec![7_u8, 8, 255].into()))
            .await
            .unwrap();
        let mut received = Vec::new();
        let mut pong_received = false;
        while received.len() < 2 || !pong_received {
            match tokio::time::timeout(Duration::from_secs(2), websocket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Message::Text(value) => received.push(("text", value.to_string().into_bytes())),
                Message::Binary(value) => received.push(("binary", value.to_vec())),
                Message::Close(_) => break,
                Message::Pong(value) => {
                    assert_eq!(value.as_ref(), b"worker-keepalive");
                    pong_received = true;
                }
                Message::Ping(_) => {}
                _ => {}
            }
        }
        assert_eq!(
            received,
            vec![("text", b"worker-text".to_vec()), ("binary", vec![1, 2, 3]),]
        );
        assert!(pong_received);
        let close = tokio::time::timeout(Duration::from_secs(2), websocket.next())
            .await
            .expect("worker WebSocket close timed out")
            .expect("worker WebSocket closed without a frame")
            .expect("worker WebSocket close frame failed");
        assert!(matches!(
            close,
            Message::Close(Some(frame))
                if u16::from(frame.code) == 1000 && frame.reason == "done"
        ));
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-websocket-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut messages = serde_json::Value::Null;
    for _ in 0..20 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages == serde_json::json!([["server-text", "7,8,255"]]) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(messages, serde_json::json!([["server-text", "7,8,255"]]));
    engine.close_async().await.unwrap();
    server.await.unwrap();
    assert!(websocket_cookie_seen.load(Ordering::SeqCst));
}
#[tokio::test]
async fn native_content_process_worker_dispatches_large_websocket_event_as_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-large-websocket-page")
        );
        let body = "<script>globalThis.workerLength = 0; globalThis.worker = new Worker('/worker-large-websocket.js'); worker.onmessage = event => { workerLength = event.data; };</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-large-websocket.js")
        );
        let body = format!(
            "const socket = new WebSocket('ws://{address}/socket'); socket.onmessage = event => {{ postMessage(typeof event.data === 'string' ? event.data.length : -1); socket.close(); }}; socket.onerror = () => postMessage(-1);"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        websocket
            .send(Message::Text("x".repeat(20_000).into()))
            .await
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), websocket.next()).await;
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-large-websocket-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut length = serde_json::Value::Null;
    for _ in 0..20 {
        length = engine.evaluate_async("workerLength").await.unwrap();
        if length == serde_json::json!(20_000) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(length, serde_json::json!(20_000));
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_drives_event_source_named_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            (
                "/worker-eventsource-page",
                "text/html",
                "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-eventsource.js'); worker.onmessage = event => workerMessages.push(event.data);</script>",
            ),
            (
                "/worker-eventsource.js",
                "text/javascript",
                "const source = new EventSource('/worker-events'); const events = []; source.addEventListener('open', () => events.push(['open', source.readyState])); source.addEventListener('greeting', event => { events.push([event.type, event.data, event.lastEventId, event.origin]); postMessage(events); source.close(); }); source.onerror = event => postMessage(['error', String(event.message || '')]);",
            ),
            (
                "/worker-events",
                "text/event-stream",
                "retry: 5\r\nid: 42\r\nevent: greeting\r\ndata: hello\r\ndata: worker\r\n\r\n",
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(path));
            if path == "/worker-events" {
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("accept") && value.trim() == "text/event-stream"
                    })
                }));
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("cookie")
                            && value
                                .trim()
                                .split(';')
                                .any(|cookie| cookie.trim() == "worker_parent=seed")
                    })
                }));
            }
            let set_cookie = match path {
                "/worker-eventsource-page" => {
                    "Set-Cookie: worker_parent=seed; HttpOnly; Path=/\r\n"
                }
                "/worker-events" => "Set-Cookie: worker_sse=accepted; HttpOnly; Path=/\r\n",
                _ => "",
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-eventsource-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut messages = serde_json::Value::Null;
    for _ in 0..20 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages
            == serde_json::json!([[
                ["open", 1],
                [
                    "greeting",
                    "hello\nworker",
                    "42",
                    format!("http://{address}")
                ]
            ]])
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        messages,
        serde_json::json!([[
            ["open", 1],
            [
                "greeting",
                "hello\nworker",
                "42",
                format!("http://{address}")
            ]
        ]])
    );
    let cookies = engine.cookies_async().await.unwrap();
    for name in ["worker_parent", "worker_sse"] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only),
            "parent cookie jar should retain Worker EventSource cookie {name} as HttpOnly"
        );
    }
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_dispatches_large_event_source_event_as_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-large-eventsource-page")
        );
        let body = "<script>globalThis.workerLength = 0; globalThis.worker = new Worker('/worker-large-eventsource.js'); worker.onmessage = event => { workerLength = event.data; };</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-large-eventsource.js")
        );
        let body = "const source = new EventSource('/worker-large-events'); source.onmessage = event => { postMessage(event.data.length); source.close(); }; source.onerror = () => postMessage(-1);";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/worker-large-events")
        );
        let body = format!("data: {}\r\n\r\n", "x".repeat(20_000));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-large-eventsource-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut length = serde_json::Value::Null;
    for _ in 0..20 {
        length = engine.evaluate_async("workerLength").await.unwrap();
        if length == serde_json::json!(20_000) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(length, serde_json::json!(20_000));
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

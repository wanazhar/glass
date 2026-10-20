use super::*;

#[tokio::test]
async fn native_json_module_file_graph_loads_rooted_imports() {
    let root = std::env::temp_dir().join(format!(
        "glass-native-file-json-modules-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("index.html"),
        "<script type='module' src='./app.js'></script>",
    )
    .unwrap();
    fs::write(
        root.join("app.js"),
        "import data from './data.json' with { type: 'json' }; globalThis.fileJsonModule = [data.name, Object.prototype.hasOwnProperty.call(data, '__proto__'), data.__proto__.polluted];",
    )
    .unwrap();
    fs::write(
        root.join("data.json"),
        r#"{"name":"file-json","__proto__":{"polluted":true}}"#,
    )
    .unwrap();

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(native_test_file_url(&root.join("index.html")))
            .with_allowed_file_root(root.clone()),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.fileJsonModule")
            .await
            .unwrap(),
        serde_json::json!(["file-json", true, true])
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_message_transport_preserves_structured_clone_values() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://structured-clone-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://structured-clone-worker",
            r#"self.onmessage = event => {
                const value = event.data;
                const mapKey = Array.from(value.map.keys())[0];
                postMessage({
                    date: [value.date instanceof Date, value.date.getTime()],
                    regexp: [value.regexp instanceof RegExp, value.regexp.source, value.regexp.flags, value.regexp.lastIndex],
                    map: [value.map instanceof Map, value.map.get(mapKey).nested, value.map.get(mapKey) === value.alias],
                    set: [value.set instanceof Set, Array.from(value.set)],
                    buffer: [value.buffer instanceof ArrayBuffer, Array.from(new Uint8Array(value.buffer))],
                    view: [value.view instanceof Uint16Array, Array.from(value.view), value.view.buffer === value.buffer],
                    blob: [value.blob instanceof Blob, value.blob.type, value.blob.size],
                    cycle: value.cycle.self === value.cycle,
                    undefined: value.undefinedValue === undefined,
                    numbers: [Number.isNaN(value.nan), Object.is(value.negativeZero, -0), value.infinity === Infinity],
                    bigint: typeof value.bigint === 'bigint' ? [typeof value.bigint, String(value.bigint)] : [typeof value.bigint, null],
                });
            }"#,
        )
        .unwrap()
        .with_initial_url("fixture://structured-clone-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const shared = { nested: true };
                    const map = new Map([[shared, shared]]);
                    const buffer = new ArrayBuffer(6);
                    new Uint8Array(buffer).set([1, 2, 3, 4, 5, 6]);
                    const payload = {
                        date: new Date(1700000000123),
                        regexp: /glass+/gi,
                        map,
                        set: new Set([1, 'two']),
                        buffer,
                        view: new Uint16Array(buffer, 2, 2),
                        alias: shared,
                        blob: new Blob(['native'], { type: 'text/plain' }),
                        cycle: {},
                        undefinedValue: undefined,
                        nan: NaN,
                        negativeZero: -0,
                        infinity: Infinity,
                        bigint: typeof BigInt === 'function' ? BigInt('9007199254740993') : undefined,
                    };
                    payload.cycle.self = payload.cycle;
                    payload.regexp.lastIndex = 2;
                    globalThis.localStructuredClone = structuredClone(payload);
                    globalThis.localStructured = null;
                    const channel = new MessageChannel();
                    channel.port2.onmessage = event => {
                        localStructured = {
                            date: [event.data.date instanceof Date, event.data.date.getTime()],
                            map: [event.data.map instanceof Map, event.data.map.get(event.data.alias) === event.data.alias],
                            buffer: [event.data.buffer instanceof ArrayBuffer, Array.from(new Uint8Array(event.data.buffer))],
                            view: [event.data.view instanceof Uint16Array, Array.from(event.data.view), event.data.view.buffer === event.data.buffer],
                            blob: [event.data.blob instanceof Blob, event.data.blob.type, event.data.blob.size],
                            cycle: event.data.cycle.self === event.data.cycle,
                            numbers: [Number.isNaN(event.data.nan), Object.is(event.data.negativeZero, -0), event.data.infinity === Infinity],
                        };
                    };
                    channel.port1.postMessage(payload);
                    globalThis.workerMessages = [];
                    globalThis.worker = new Worker('fixture://structured-clone-worker');
                    worker.onmessage = event => workerMessages.push(event.data);
                    worker.postMessage(payload);
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ localStructuredClone: [localStructuredClone.date instanceof Date, localStructuredClone.date.getTime(), localStructuredClone.map instanceof Map, localStructuredClone.map.get(localStructuredClone.alias) === localStructuredClone.alias, localStructuredClone.buffer instanceof ArrayBuffer, Array.from(new Uint8Array(localStructuredClone.buffer)), localStructuredClone.view instanceof Uint16Array, Array.from(localStructuredClone.view), localStructuredClone.view.buffer === localStructuredClone.buffer, localStructuredClone.blob instanceof Blob, localStructuredClone.blob.type, localStructuredClone.blob.size, localStructuredClone.cycle.self === localStructuredClone.cycle, Number.isNaN(localStructuredClone.nan), Object.is(localStructuredClone.negativeZero, -0), localStructuredClone.infinity === Infinity], localStructured, workerMessages })")
            .await
            .unwrap(),
        serde_json::json!({
            "localStructuredClone": [
                true,
                1700000000123_i64,
                true,
                true,
                true,
                [1, 2, 3, 4, 5, 6],
                true,
                [1027, 1541],
                true,
                true,
                "text/plain",
                6,
                true,
                true,
                true,
                true,
            ],
            "localStructured": {
                "date": [true, 1700000000123_i64],
                "map": [true, true],
                "buffer": [true, [1, 2, 3, 4, 5, 6]],
                "view": [true, [1027, 1541], true],
                "blob": [true, "text/plain", 6],
                "cycle": true,
                "numbers": [true, true, true],
            },
            "workerMessages": [{
                "date": [true, 1700000000123_i64],
                "regexp": [true, "glass+", "gi", 2],
                "map": [true, true, true],
                "set": [true, [1, "two"]],
                "buffer": [true, [1, 2, 3, 4, 5, 6]],
                "view": [true, [1027, 1541], true],
                "blob": [true, "text/plain", 6],
                "cycle": true,
                "undefined": true,
                "numbers": [true, true, true],
                "bigint": ["bigint", "9007199254740993"],
            }],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_message_ports_transfer_between_page_and_worker_realms() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://transferred-message-port-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://transferred-message-port-worker",
            "self.onmessage = event => { const port = event.ports[0]; port.onmessage = message => { postMessage({ kind: 'worker-received', value: message.data.value }); port.postMessage({ kind: 'worker-reply', value: message.data.value + 1 }); }; port.start(); port.postMessage({ kind: 'worker-ready', value: 1 }); const returnChannel = new MessageChannel(); returnChannel.port2.onmessage = message => { postMessage({ kind: 'worker-port-received', value: message.data.value }); returnChannel.port2.postMessage({ kind: 'worker-port-reply', value: message.data.value + 1 }); }; returnChannel.port2.start(); postMessage({ kind: 'worker-port', value: 2 }, [returnChannel.port1]); };",
        )
        .unwrap()
        .with_initial_url("fixture://transferred-message-port-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.portEvents = [];
                    globalThis.workerMessages = [];
                    globalThis.returnPort = null;
                    globalThis.channel = new MessageChannel();
                    globalThis.rollbackChannel = new MessageChannel();
                    globalThis.invalidTransferChannel = new MessageChannel();
                    channel.port2.onmessage = event => portEvents.push({
                        kind: event.data.kind,
                        value: event.data.value,
                        portCount: event.ports.length,
                    });
                    channel.port2.start();
                    globalThis.worker = new Worker('fixture://transferred-message-port-worker');
                    worker.onmessage = event => {
                        workerMessages.push(event.data);
                        if (event.data.kind === 'worker-port') {
                            returnPort = event.ports[0];
                            returnPort.onmessage = message => portEvents.push({
                                kind: message.data.kind,
                                value: message.data.value,
                                portCount: message.ports.length,
                            });
                            returnPort.start();
                            returnPort.postMessage({ value: 8 });
                        }
                    };
                    worker.postMessage({ kind: 'connect' }, [channel.port1]);
                    let detachedError = '';
                    try { channel.port1.postMessage({ value: 0 }); }
                    catch (error) { detachedError = error.name; }
                    const uncloneable = { callback: () => {} };
                    let uncloneableError = '';
                    try { worker.postMessage(uncloneable, [rollbackChannel.port1]); }
                    catch (error) { uncloneableError = error.name; }
                    let rollbackPortError = '';
                    try { rollbackChannel.port1.postMessage({ value: 1 }); }
                    catch (error) { rollbackPortError = error.name; }
                    let invalidTransferError = '';
                    try { worker.postMessage({ value: 2 }, [invalidTransferChannel.port1, {}]); }
                    catch (error) { invalidTransferError = error.name; }
                    let invalidTransferPortError = '';
                    try { invalidTransferChannel.port1.postMessage({ value: 2 }); }
                    catch (error) { invalidTransferPortError = error.name; }
                    return [detachedError, channel.port2 instanceof MessagePort, uncloneableError, rollbackPortError, invalidTransferError, invalidTransferPortError];
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(["InvalidStateError", true, "DataCloneError", "", "DataCloneError", ""])
    );
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
}
#[tokio::test]
async fn native_message_ports_transfer_blob_urls_between_page_and_worker_realms() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://message-port-blob-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://message-port-blob-worker",
            "self.onmessage = event => { const port = event.ports[0]; port.onmessage = message => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', message.data.url, false); xhr.send(); const blob = new Blob(['worker-port-body'], { type: 'text/worker-port' }); port.postMessage({ kind: 'worker-read', text: xhr.responseText, contentType: xhr.getResponseHeader('content-type'), url: URL.createObjectURL(blob) }); } catch (error) { port.postMessage({ kind: 'worker-error', error: String(error) }); } }; port.start(); port.postMessage({ kind: 'worker-ready' }); };",
        )
        .unwrap()
        .with_initial_url("fixture://message-port-blob-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    globalThis.portEvents = [];
                    globalThis.channel = new MessageChannel();
                    channel.port2.onmessage = event => {
                        if (event.data.kind === 'worker-ready') {
                            portEvents.push([event.data.kind]);
                            return;
                        }
                        if (event.data.kind !== 'worker-read') {
                            portEvents.push([event.data.kind, event.data.error]);
                            return;
                        }
                        try {
                            const xhr = new XMLHttpRequest();
                            xhr.open('GET', event.data.url, false);
                            xhr.send();
                            portEvents.push([event.data.kind, event.data.text,
                                event.data.contentType, xhr.responseText,
                                xhr.getResponseHeader('content-type')]);
                        } catch (error) {
                            portEvents.push(['page-error', String(error)]);
                        }
                    };
                    channel.port2.start();
                    globalThis.worker = new Worker('fixture://message-port-blob-worker');
                    worker.postMessage({ kind: 'connect' }, [channel.port1]);
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("portEvents").await.unwrap(),
        serde_json::json!([["worker-ready"]])
    );

    assert_eq!(
        engine
            .evaluate_async(
                "const blob = new Blob(['page-port-body'], { type: 'text/page-port' }); channel.port2.postMessage({ url: URL.createObjectURL(blob) }); true",
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
                "page-port-body",
                "text/page-port",
                "worker-port-body",
                "text/worker-port",
            ]
        ])
    );
    engine.close_async().await.unwrap();
}
#[test]
fn native_json_module_worker_graphs_support_dedicated_and_shared_workers() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_json_module_worker_graphs_support_dedicated_and_shared_workers_inner(),
        );
    });
}

async fn native_json_module_worker_graphs_support_dedicated_and_shared_workers_inner() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (content_type, body) = match path {
                "/module-worker-page" => (
                    "text/html",
                    "<!doctype html><html><body><main id='output'>Native</main></body></html>",
                ),
                "/module-worker-root.js" => (
                    "text/javascript",
                    "import { workerValue } from './module-worker-dependency.js'; import workerConfig from './module-worker.json' with { type: 'json' }; const dynamicConfigPromise = import('./module-worker-dynamic.json', { with: { type: 'json' } }).then(module => module.default.kind); let importScriptsRejected = false; try { importScripts('./module-worker-dependency.js'); } catch (error) { importScriptsRejected = error instanceof TypeError; } self.onmessage = event => postMessage({ kind: 'reply', value: workerValue + event.data, module: typeof importScripts, hasDocument: typeof document !== 'undefined' }); dynamicConfigPromise.then(dynamicJson => postMessage({ kind: 'ready', value: workerValue, importScriptsRejected, json: workerConfig.kind, dynamicJson }));",
                ),
                "/module-worker-dependency.js" => {
                    ("text/javascript", "export const workerValue = 7;")
                }
                "/module-worker.json" => ("application/json", r#"{"kind":"worker-json"}"#),
                "/module-worker-dynamic.json" => {
                    ("application/json", r#"{"kind":"dynamic-worker-json"}"#)
                }
                "/module-shared-worker-root.js" => (
                    "text/javascript",
                    "import { sharedValue } from './module-shared-worker-dependency.js#first'; import { sharedValue as second } from './module-shared-worker-dependency.js#second'; import sharedConfig from './module-shared-worker.json' with { type: 'json' }; const dynamicConfigPromise = import('./module-shared-worker-dynamic.json', { with: { type: 'json' } }).then(module => module.default.kind); onconnect = event => { const port = event.ports[0]; dynamicConfigPromise.then(dynamicJson => { port.postMessage({ kind: 'shared-ready', value: sharedValue, executions: globalThis.sharedModuleExecutions, json: sharedConfig.kind, dynamicJson }); port.onmessage = message => port.postMessage({ kind: 'shared-reply', value: sharedValue + Number(message.data) }); port.start(); }); };",
                ),
                "/module-shared-worker-dependency.js" => (
                    "text/javascript",
                    "globalThis.sharedModuleExecutions = (globalThis.sharedModuleExecutions || 0) + 1; export const sharedValue = 5;",
                ),
                "/module-shared-worker.json" => ("application/json", r#"{"kind":"shared-json"}"#),
                "/module-shared-worker-dynamic.json" => {
                    ("application/json", r#"{"kind":"dynamic-shared-json"}"#)
                }
                unexpected => panic!("unexpected native module worker request: {unexpected:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let page_url = format!("http://{address}/module-worker-page");
    let worker_url = format!("http://{address}/module-worker-root.js");
    let shared_worker_url = format!("http://{address}/module-shared-worker-root.js");
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(page_url),
    )
    .await
    .unwrap();

    assert_eq!(
        session
            .script(
                "globalThis.workerMessages = []; globalThis.sharedMessages = []; globalThis.worker = new Worker('__WORKER_URL__', { type: 'module' }); worker.onmessage = event => workerMessages.push(event.data); globalThis.shared = new SharedWorker('__SHARED_WORKER_URL__', { name: 'module-shared', type: 'module' }); shared.port.onmessage = event => sharedMessages.push(event.data); shared.port.start(); [worker instanceof Worker, worker.url, shared instanceof SharedWorker, shared.url]"
                    .replace("__WORKER_URL__", &worker_url)
                    .replace("__SHARED_WORKER_URL__", &shared_worker_url),
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([
            true,
            worker_url,
            true,
            shared_worker_url,
        ])
    );
    session
        .native_wait(
            WaitCondition::JavaScript(
                "workerMessages.length === 1 && sharedMessages.length === 1".into(),
            ),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script("[workerMessages, sharedMessages]")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            [{"kind": "ready", "value": 7, "importScriptsRejected": true, "json": "worker-json", "dynamicJson": "dynamic-worker-json"}],
            [{"kind": "shared-ready", "value": 5, "executions": 2, "json": "shared-json", "dynamicJson": "dynamic-shared-json"}],
        ])
    );
    assert_eq!(
        session
            .script(
                "worker.postMessage(4); shared.port.postMessage(3); [workerMessages, sharedMessages]",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([
            [{"kind": "ready", "value": 7, "importScriptsRejected": true, "json": "worker-json", "dynamicJson": "dynamic-worker-json"}],
            [{"kind": "shared-ready", "value": 5, "executions": 2, "json": "shared-json", "dynamicJson": "dynamic-shared-json"}],
        ])
    );
    session
        .native_wait(
            WaitCondition::JavaScript(
                "workerMessages.length === 2 && sharedMessages.length === 2".into(),
            ),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script("[workerMessages, sharedMessages]")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            [
                {"kind": "ready", "value": 7, "importScriptsRejected": true, "json": "worker-json", "dynamicJson": "dynamic-worker-json"},
                {
                    "kind": "reply",
                    "value": 11,
                    "module": "function",
                    "hasDocument": false,
                },
            ],
            [
                {"kind": "shared-ready", "value": 5, "executions": 2, "json": "shared-json", "dynamicJson": "dynamic-shared-json"},
                {"kind": "shared-reply", "value": 8},
            ],
        ])
    );
    session.close().await.unwrap();
    server.abort();
    server
        .await
        .expect_err("the bounded local fixture server should stop after the test");
}
#[tokio::test]
async fn native_json_module_service_worker_static_imports() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let font_bytes = native_test_color_font_bytes();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/register",
                "text/html",
                Cow::Borrowed(
                    "<!doctype html><html><body><script>globalThis.controllerChanges = 0; navigator.serviceWorker.addEventListener('controllerchange', () => controllerChanges++); globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/', type: 'module' });</script><main>registration page</main></body></html>",
                ),
            ),
            (
                "/sw.js",
                "application/javascript",
                Cow::Borrowed(
                    r#"import { execution as first } from './sw-dep.js#first';
import { execution as second } from './sw-dep.js#second';
import swConfiguration from './sw-config.json' with { type: 'json' };
const moduleExecutionCount = first + second;
self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
    const path = new URL(event.request.url).pathname;
  if (path === '/font.ttf') {
    event.respondWith(event.request.destination === 'font'
      ? fetch('/font-upstream.ttf')
      : new Response('wrong destination', { status: 400 }));
    return;
  }
  if (path === '/api') {
    event.respondWith((async () => {
      const matched = await clients.matchAll();
      const all = await clients.matchAll({ includeUncontrolled: true, type: 'all' });
      const workers = await clients.matchAll({ type: 'worker' });
      const client = matched[0];
      const nested = await fetch('/service-worker-large-data');
      const nestedLength = (await nested.text()).length;
      const channel = new MessageChannel();
      const blob = new Blob(['service-worker-client-body'], { type: 'text/service-worker-client' });
      const blobUrl = URL.createObjectURL(blob);
      client.postMessage({ kind: 'fetch-client', url: event.request.url, blobUrl }, {
        transfer: [channel.port1],
      });
      return new Response(JSON.stringify({
        matched: matched.length,
        all: all.length,
        workers: workers.length,
        nestedLength,
        moduleExecutionCount,
        serviceWorkerJson: swConfiguration.kind,
        clientIdPresent: Boolean(client && client.id),
        eventClientIdPresent: Boolean(event.clientId),
        clientUrl: client && client.url,
        clientType: client && client.type,
        frameType: client && client.frameType,
        visibilityState: client && client.visibilityState,
        focused: client && client.focused,
      }), {
      status: 201,
      headers: { 'Content-Type': 'application/json', 'X-Native-Worker': 'yes' },
      });
    })());
  } else {
    event.respondWith(new Response('<!doctype html><html><body><main id="controlled">controlled by native service worker</main></body></html>', {
      headers: { 'Content-Type': 'text/html' },
    }));
  }
});"#,
                ),
            ),
            (
                "/sw-dep.js",
                "application/javascript",
                Cow::Borrowed(
                    "globalThis.serviceWorkerModuleExecutions = (globalThis.serviceWorkerModuleExecutions || 0) + 1; export const execution = globalThis.serviceWorkerModuleExecutions;",
                ),
            ),
            (
                "/sw-dep.js",
                "application/javascript",
                Cow::Borrowed(
                    "globalThis.serviceWorkerModuleExecutions = (globalThis.serviceWorkerModuleExecutions || 0) + 1; export const execution = globalThis.serviceWorkerModuleExecutions;",
                ),
            ),
            (
                "/sw-config.json",
                "application/vnd.glass+json; charset=utf-8",
                Cow::Borrowed(r#"{"kind":"service-worker-json"}"#),
            ),
            (
                "/service-worker-large-data",
                "text/plain",
                Cow::Owned("x".repeat(20_000)),
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body.as_ref()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/font-upstream.ttf")
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: font/ttf\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            font_bytes.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.write_all(&font_bytes).await.unwrap();
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/network"));
        let body =
            "<!doctype html><html><body><main id='network'>network fallback</main></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/register")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async reg => [reg.scope, reg.active.state, reg.active.scriptURL, navigator.serviceWorker.controller !== null, controllerChanges, await navigator.serviceWorker.getRegistrations().then(items => items.length)])",
            )
            .await
            .unwrap(),
        serde_json::json!([
            format!("http://{address}/"),
            "activated",
            format!("http://{address}/sw.js"),
            true,
            1,
            1,
        ])
    );

    engine
        .navigate_async(format!("http://{address}/app/page"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ text: document.body.innerText, controlled: navigator.serviceWorker.controller !== null, scriptURL: navigator.serviceWorker.controller && navigator.serviceWorker.controller.scriptURL })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "text": "controlled by native service worker",
            "controlled": true,
            "scriptURL": format!("http://{address}/sw.js"),
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.swClientMessages = []; navigator.serviceWorker.addEventListener('message', event => swClientMessages.push({ data: event.data, ports: event.ports.length, source: event.source && event.source.scriptURL })); undefined",
            )
            .await
            .unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/api').then(async response => { const message = swClientMessages[0]; const blobResponse = await fetch(message.data.blobUrl); return { status: response.status, worker: response.headers.get('x-native-worker'), body: await response.json(), messages: swClientMessages, blob: { text: await blobResponse.text(), type: blobResponse.headers.get('content-type') } }; })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "status": 201,
            "worker": "yes",
            "body": {
                "matched": 1,
                "all": 1,
                "workers": 0,
                "nestedLength": 20_000,
                "moduleExecutionCount": 3,
                "serviceWorkerJson": "service-worker-json",
                "clientIdPresent": true,
                "eventClientIdPresent": true,
                "clientUrl": format!("http://{address}/app/page"),
                "clientType": "window",
                "frameType": "top-level",
                "visibilityState": "visible",
                "focused": true,
            },
            "messages": [{
                "data": {
                    "kind": "fetch-client",
                    "url": format!("http://{address}/api"),
                    "blobUrl": format!("blob:http://{address}/glass-worker-1"),
                },
                "ports": 1,
                "source": format!("http://{address}/sw.js"),
            }],
            "blob": {
                "text": "service-worker-client-body",
                "type": "text/service-worker-client",
            },
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                  const face = new FontFace("SW Face", "url(/font.ttf)");
                  document.fonts.add(face);
                  await face.load();
                  return [face.status, face.family, document.fonts.check("16px \"SW Face\"")];
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(["loaded", "SW Face", true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await navigator.serviceWorker.getRegistration('/app/page').then(registration => registration.unregister())",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(await navigator.serviceWorker.getRegistration('/app/page')) === undefined && (await navigator.serviceWorker.getRegistrations()).length === 0",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine
        .navigate_async(format!("http://{address}/network"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.innerText")
            .await
            .unwrap(),
        serde_json::json!("network fallback")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_post_message_dispatch_keeps_large_data_out_of_script_source() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_post_message_dispatch_keeps_large_data_out_of_script_source_inner());
    });
}

async fn native_post_message_dispatch_keeps_large_data_out_of_script_source_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://large-message-parent",
            "<script>globalThis.popup = window.open('fixture://large-message-child', 'large-message-child'); popup.postMessage({ kind: 'large', value: 'x'.repeat(20000) }, '*');</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://large-message-child",
            "<script>addEventListener('message', event => { globalThis.received = [event.data.kind, event.data.value.length, event.origin, Boolean(event.source)]; });</script><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://large-message-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();
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
        serde_json::json!(["large", 20000, "null", true])
    );
    session.close().await.unwrap();
}
#[test]
fn canonical_browser_session_stop_loading_cancels_and_restarts_worker() {
    std::thread::Builder::new()
        .name("canonical-browser-stop-loading-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("canonical stop-loading runtime should build");
            runtime.block_on(
                canonical_browser_session_stop_loading_cancels_and_restarts_worker_inner(),
            );
        })
        .expect("canonical stop-loading test thread should start")
        .join()
        .expect("canonical stop-loading test thread should complete");
}
#[tokio::test]
async fn native_json_module_page_graph_uses_module_type_identity() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let expected_paths = ["/page", "/app.js", "/data.json", "/data.json", "/dep.js"];
        let mut requests = Vec::new();
        let mut data_module_accepts_json = Vec::new();
        for expected_path in expected_paths {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("HTTP request includes a path")
                .to_owned();
            assert_eq!(path, expected_path);
            let accepts_json = request
                .to_ascii_lowercase()
                .lines()
                .any(|line| line.starts_with("accept:") && line.contains("application/json"));
            if path == "/data.json" {
                data_module_accepts_json.push(accepts_json);
            }
            let (content_type, body) = match path.as_str() {
                "/page" => (
                    "text/html",
                    r#"<script type='module' src='/app.js'></script><script>globalThis.nativeModuleEvents = []; globalThis.nativeWindowErrors = []; document.querySelector("script[type='module']").addEventListener('error', event => nativeModuleEvents.push({ type: event.type, message: event.message })); window.addEventListener('error', event => nativeWindowErrors.push({ message: event.message, filename: event.filename }));</script>"#,
                ),
                "/app.js" => (
                    "application/javascript",
                    "import data from './data.json' with { type: 'json' }; import * as dataModule from './data.json' with { type: 'json' }; import { suffix as typedSuffix } from './data.json'; import { suffix } from './dep.js'; globalThis.nativeJsonModule = [Object.keys(dataModule).join(','), data.name, Object.prototype.hasOwnProperty.call(data, '__proto__'), Object.getPrototypeOf(data) === Object.prototype, data.__proto__.polluted, suffix, typedSuffix];",
                ),
                "/data.json" if accepts_json => (
                    "application/vnd.glass+json; charset=utf-8",
                    r#"{"name":"first","name":"json","__proto__":{"polluted":true}}"#,
                ),
                "/data.json" => (
                    "application/javascript",
                    "export const suffix = 'same-url-js';",
                ),
                _ => ("application/javascript", "export const suffix = 'dep';"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push(path);
        }
        (requests, data_module_accepts_json)
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async(
            "({ value: globalThis.nativeJsonModule, moduleEvents: globalThis.nativeModuleEvents, windowErrors: globalThis.nativeWindowErrors })",
        )
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let (requests, data_module_accepts_json) = server.await.unwrap();
    assert_eq!(
        requests,
        vec![
            "/page".to_owned(),
            "/app.js".to_owned(),
            "/data.json".to_owned(),
            "/data.json".to_owned(),
            "/dep.js".to_owned(),
        ]
    );
    assert_eq!(data_module_accepts_json, [true, false]);
    assert_eq!(
        result,
        serde_json::json!({
            "value": ["default", "json", true, true, true, "dep", "same-url-js"],
            "moduleEvents": [],
            "windowErrors": [],
        }),
        "module-type identity failure details: paths={requests:?}, JSON accepts={data_module_accepts_json:?}"
    );
}
#[tokio::test]
async fn native_dynamic_json_import_options_are_demand_loaded_and_type_keyed() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let expected_paths = ["/page", "/app.js", "/dynamic.json", "/dynamic.json"];
        let mut requests = Vec::new();
        let mut accepts_json = Vec::new();
        for expected_path in expected_paths {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("dynamic module request server timed out")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("dynamic module request includes a path")
                .to_owned();
            assert_eq!(path, expected_path);
            let request_accepts_json = request
                .to_ascii_lowercase()
                .lines()
                .any(|line| line.starts_with("accept:") && line.contains("application/json"));
            if path == "/dynamic.json" {
                accepts_json.push(request_accepts_json);
            }
            let (content_type, body) = match path.as_str() {
                "/page" => ("text/html", "<script type='module' src='/app.js'></script>"),
                "/app.js" => (
                    "application/javascript",
                    "globalThis.dynamicImportOrder = []; const jsonSpecifier = () => { dynamicImportOrder.push('specifier'); return { [Symbol.toPrimitive](hint) { dynamicImportOrder.push(hint); dynamicImportOrder.push('coerce'); return './dynamic.json'; } }; }; const jsonOptions = () => { dynamicImportOrder.push('options'); return { get with() { dynamicImportOrder.push('with'); return { get type() { dynamicImportOrder.push('type'); return 'json'; } }; } }; }; const states = { json: 'pending', javascript: 'pending', invalid: 'pending', boxedSymbol: 'pending', order: dynamicImportOrder }; globalThis.dynamicImportResults = states; import(jsonSpecifier(), jsonOptions()).then(module => { states.json = module.default.kind; }, error => { states.json = `rejected:${error.name}:${error.message}`; }); import('./dynamic.json').then(module => { states.javascript = module.kind; }, error => { states.javascript = `rejected:${error.name}:${error.message}`; }); import('./invalid.json', { with: { unsupported: 'value' } }).then(() => { states.invalid = 'fulfilled'; }, error => { states.invalid = error.name; }); import(Object(Symbol())).then(() => { states.boxedSymbol = 'fulfilled'; }, error => { states.boxedSymbol = error.name; }); if (false) import('./never-requested.js');",
                ),
                "/dynamic.json" if request_accepts_json => {
                    ("application/json", r#"{"kind":"json-dynamic"}"#)
                }
                "/dynamic.json" => (
                    "application/javascript",
                    "export const kind = 'javascript-dynamic';",
                ),
                _ => panic!("unexpected dynamic module request: {path}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push(path);
        }
        (requests, accepts_json)
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async("globalThis.dynamicImportResults")
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let (requests, accepts_json) = server.await.unwrap();

    assert_eq!(
        requests,
        ["/page", "/app.js", "/dynamic.json", "/dynamic.json"]
    );
    assert_eq!(accepts_json, [true, false]);
    assert_eq!(
        result,
        serde_json::json!({
            "json": "json-dynamic",
            "javascript": "javascript-dynamic",
            "invalid": "TypeError",
            "boxedSymbol": "TypeError",
            "order": ["specifier", "options", "string", "coerce", "with", "type"]
        }),
        "dynamic import type identity or options evaluation order was incorrect: paths={requests:?}, JSON accepts={accepts_json:?}"
    );
}
#[tokio::test]
async fn native_json_module_rejects_wrong_mime_and_invalid_json() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let expected_paths = [
            "/mime-page",
            "/mime-app.js",
            "/wrong.json",
            "/invalid-page",
            "/invalid-app.js",
            "/invalid.json",
        ];
        let mut requests = Vec::new();
        for expected_path in expected_paths {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("HTTP request includes a path")
                .to_owned();
            assert_eq!(path, expected_path);
            let (content_type, body) = match path.as_str() {
                "/mime-page" => (
                    "text/html",
                    "<script type='module' src='/mime-app.js'></script>",
                ),
                "/invalid-page" => (
                    "text/html",
                    "<script type='module' src='/invalid-app.js'></script>",
                ),
                "/mime-app.js" => (
                    "application/javascript",
                    "import value from './wrong.json' with { type: 'json' }; globalThis.wrongMimeAppExecuted = true;",
                ),
                "/wrong.json" => (
                    "text/javascript",
                    "globalThis.wrongMimeResourceExecuted = true; export default {};",
                ),
                "/invalid-app.js" => (
                    "application/javascript",
                    "import value from './invalid.json' with { type: 'json' }; globalThis.invalidJsonAppExecuted = true;",
                ),
                _ => ("application/json", r#"{"value":"unterminated}"#),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push(path);
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/mime-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("[typeof wrongMimeAppExecuted, typeof wrongMimeResourceExecuted]")
            .await
            .unwrap(),
        serde_json::json!(["undefined", "undefined"]),
        "a JavaScript response must not be executed as a JSON module"
    );
    engine
        .navigate_async(format!("http://{address}/invalid-page"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("typeof invalidJsonAppExecuted")
            .await
            .unwrap(),
        serde_json::json!("undefined"),
        "invalid JSON must prevent the importing module from evaluating"
    );
    engine.close_async().await.unwrap();
    assert_eq!(
        server.await.unwrap(),
        vec![
            "/mime-page".to_owned(),
            "/mime-app.js".to_owned(),
            "/wrong.json".to_owned(),
            "/invalid-page".to_owned(),
            "/invalid-app.js".to_owned(),
            "/invalid.json".to_owned(),
        ]
    );
}
#[tokio::test]
async fn native_json_module_worker_graph_uses_redirected_response_base() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async {
            let expected_paths = [
                "/page",
                    "/entry.js?version=one",
                    "/workers/entry-real.js?version=two",
                    "/workers/dep.js?variant=one",
                    "/workers/dep.js?variant=one",
                    "/workers/data.json",
                    "/workers/dep.js?variant=two",
            ];
            let mut requests = Vec::new();
            for expected_path in expected_paths {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("worker module request includes a URL")
                    .to_owned();
                assert_eq!(path, expected_path);
                let response = match path.as_str() {
                    "/page" => {
                        let body = "<script>globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('/entry.js?version=one#root', { type: 'module' }); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message);</script>";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    "/entry.js?version=one" => {
                        "HTTP/1.1 302 Found\r\nLocation: /workers/entry-real.js?version=two\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
                    }
                    "/workers/entry-real.js?version=two" => {
                        let body = "import './dep.js?variant=one#first'; import './dep.js?variant=one#second'; import workerConfig from './data.json' with { type: 'json' }; import('./dep.js?variant=' + 'two').then(() => postMessage({ kind: 'ready', executions: globalThis.workerFragmentExecutions, json: workerConfig.kind }));";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    "/workers/dep.js?variant=one" | "/workers/dep.js?variant=two" => {
                        let body = "globalThis.workerFragmentExecutions = (globalThis.workerFragmentExecutions || 0) + 1; export const execution = globalThis.workerFragmentExecutions;";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    "/workers/data.json" => {
                        let body = r#"{"kind":"redirected-worker-json"}"#;
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/problem+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    other => panic!("unexpected worker module request: {other}"),
                };
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push(path);
            }
            requests
        })
        .await
        .expect("worker module graph stays within its time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        result,
        serde_json::json!({
            "messages": [{"kind": "ready", "executions": 3, "json": "redirected-worker-json"}],
            "errors": [],
        }),
        "worker modules must keep fragment-distinct records and resolve from the redirected response URL"
    );
    assert_eq!(
        requests,
        [
            "/page",
            "/entry.js?version=one",
            "/workers/entry-real.js?version=two",
            "/workers/dep.js?variant=one",
            "/workers/dep.js?variant=one",
            "/workers/data.json",
            "/workers/dep.js?variant=two",
        ]
    );
}
#[tokio::test]
async fn native_readable_stream_transfer_preserves_pull_order_and_hides_transport_port() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 8,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://readable-stream-transfer-page",
            "<html><body>stream transfer</body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://readable-stream-transfer-worker",
            r#"self.onmessage = async event => {
                if (event.data.kind === 'create') {
                    const events = [];
                    let pulls = 0;
                    const stream = new ReadableStream({
                        start(controller) {
                            events.push('start');
                            controller.enqueue('first');
                        },
                        pull(controller) {
                            events.push('pull');
                            if (pulls++ === 0) {
                                controller.enqueue('second');
                                controller.close();
                            }
                        },
                        cancel(reason) {
                            events.push('cancel:' + reason);
                        },
                    });
                    const reader = stream.getReader();
                    Promise.resolve((async () => {
                        const first = await reader.read();
                        const second = await reader.read();
                        const done = await reader.read();
                        const locked = stream.locked;
                        reader.releaseLock();
                        postMessage({
                            kind: 'created',
                            identity: [stream instanceof ReadableStream, locked, stream.locked],
                            events,
                            values: [first.value, second.value],
                            done: done.done,
                        });
                    })()).catch(error => postMessage({ error: error.name }));
                    return;
                }
                const stream = event.data.stream;
                if (event.data.kind === 'tee') {
                    try {
                        const [left, right] = stream.tee();
                        const leftReader = left.getReader();
                        const rightReader = right.getReader();
                        const leftFirst = await leftReader.read();
                        const rightFirst = await rightReader.read();
                        const leftSecond = await leftReader.read();
                        const rightSecond = await rightReader.read();
                        const leftDone = await leftReader.read();
                        const rightDone = await rightReader.read();
                        leftReader.releaseLock();
                        rightReader.releaseLock();
                        postMessage({
                            kind: 'tee',
                            values: [
                                Array.from(leftFirst.value),
                                Array.from(leftSecond.value),
                                leftDone.done,
                                Array.from(rightFirst.value),
                                Array.from(rightSecond.value),
                                rightDone.done,
                            ],
                        });
                    } catch (error) {
                        postMessage({ error: error.name });
                    }
                    return;
                }
                const reader = stream.getReader();
                reader.read().then(first => reader.read().then(second => reader.read().then(third => {
                    postMessage({
                        kind: event.data.kind,
                        identity: [stream instanceof ReadableStream, stream.locked, event.ports.length],
                        chunks: event.data.kind === 'bytes'
                            ? [Array.from(first.value), Array.from(second.value)]
                            : [first.value, second.value],
                        done: third.done,
                    });
                }))).catch(error => postMessage({ error: error.name }));
            }"#,
        )
        .unwrap()
        .with_initial_url("fixture://readable-stream-transfer-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const localSource = new ReadableStream({
                        type: 'bytes',
                        start(controller) {
                            controller.enqueue(new Uint8Array([9, 8]));
                            controller.enqueue(new Uint8Array([7, 6, 5]));
                            controller.close();
                        },
                    });
                    const localClone = structuredClone(localSource, { transfer: [localSource] });
                    const localReader = localClone.getReader();
                    const localFirst = await localReader.read();
                    const localSecond = await localReader.read();
                    const localDone = await localReader.read();
                    globalThis.localReadableTransfer = {
                        identity: [localSource.locked, localClone instanceof ReadableStream, localClone.locked],
                        chunks: [Array.from(localFirst.value), Array.from(localSecond.value)],
                        done: localDone.done,
                    };
                    globalThis.workerMessages = [];
                    globalThis.worker = new Worker('fixture://readable-stream-transfer-worker');
                    worker.onmessage = event => workerMessages.push(event.data);
                    globalThis.workerSource = new ReadableStream({
                        type: 'bytes',
                        start(controller) {
                            controller.enqueue(new Uint8Array([1, 2]));
                            controller.enqueue(new Uint8Array([3, 4, 5]));
                            controller.close();
                        },
                    });
                    globalThis.workerDefaultSource = new ReadableStream({
                        start(controller) {
                            controller.enqueue({ kind: 'object', value: 42 });
                            controller.enqueue('tail');
                            controller.close();
                        },
                    });
                    globalThis.teeSource = new ReadableStream({
                        type: 'bytes',
                        start(controller) {
                            controller.enqueue(new Uint8Array([6, 5]));
                            controller.enqueue(new Uint8Array([4, 3]));
                            controller.close();
                        },
                    });
                    return localReadableTransfer;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "identity": [true, true, true],
            "chunks": [[9, 8], [7, 6, 5]],
            "done": true,
        })
    );
    assert_eq!(
        engine
            .evaluate_async("worker.postMessage({ kind: 'create' }); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let expected_created = serde_json::json!([{
        "kind": "created",
        "identity": [true, true, false],
        "events": ["start", "pull"],
        "values": ["first", "second"],
        "done": true,
    }]);
    let mut worker_messages = serde_json::json!([]);
    for _ in 0..16 {
        worker_messages = engine.evaluate_async("workerMessages").await.unwrap();
        if worker_messages == expected_created {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(worker_messages, expected_created);
    assert_eq!(
        engine
            .evaluate_async(
                "worker.postMessage({ kind: 'bytes', stream: workerSource }, [workerSource]); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let expected_worker_messages = serde_json::json!([{
        "kind": "created",
        "identity": [true, true, false],
        "events": ["start", "pull"],
        "values": ["first", "second"],
        "done": true,
    }, {
        "kind": "bytes",
        "identity": [true, true, 0],
        "chunks": [[1, 2], [3, 4, 5]],
        "done": true,
    }]);
    for _ in 0..16 {
        worker_messages = engine.evaluate_async("workerMessages").await.unwrap();
        if worker_messages == expected_worker_messages {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(worker_messages, expected_worker_messages);
    assert_eq!(
        engine
            .evaluate_async(
                "worker.postMessage({ kind: 'values', stream: workerDefaultSource }, [workerDefaultSource]); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let expected_worker_messages = serde_json::json!([
        {
            "kind": "created",
            "identity": [true, true, false],
            "events": ["start", "pull"],
            "values": ["first", "second"],
            "done": true,
        },
        {
            "kind": "bytes",
            "identity": [true, true, 0],
            "chunks": [[1, 2], [3, 4, 5]],
            "done": true,
        },
        {
            "kind": "values",
            "identity": [true, true, 0],
            "chunks": [{ "kind": "object", "value": 42 }, "tail"],
            "done": true,
        },
    ]);
    for _ in 0..16 {
        worker_messages = engine.evaluate_async("workerMessages").await.unwrap();
        if worker_messages == expected_worker_messages {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(worker_messages, expected_worker_messages);
    assert_eq!(
        engine
            .evaluate_async(
                "worker.postMessage({ kind: 'tee', stream: teeSource }, [teeSource]); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let expected_tee = serde_json::json!([{
        "kind": "created",
        "identity": [true, true, false],
        "events": ["start", "pull"],
        "values": ["first", "second"],
        "done": true,
    }, {
        "kind": "bytes",
        "identity": [true, true, 0],
        "chunks": [[1, 2], [3, 4, 5]],
        "done": true,
    }, {
        "kind": "values",
        "identity": [true, true, 0],
        "chunks": [{ "kind": "object", "value": 42 }, "tail"],
        "done": true,
    }, {
        "kind": "tee",
        "values": [[6, 5], [4, 3], true, [6, 5], [4, 3], true],
    }]);
    for _ in 0..16 {
        worker_messages = engine.evaluate_async("workerMessages").await.unwrap();
        if worker_messages == expected_tee {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(worker_messages, expected_tee);
    engine.close_async().await.unwrap();
}
#[test]
fn native_physical_border_color_important_priority_reaches_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #early { border-color:red !important; }
                #rollback { border-color:red !important; }
                #rollback { border-color:revert-layer !important; }
                #normal { border-color:red; }
                #mixed { border-color:red !important; }
                #sides { border-color:red !important;border-top-color:blue !important;border-right-color:green !important; }
                #sides { border-bottom-color:blue;border-left-color:green; }
                #invalid { border-color:red !important;border-color:invalid !important; }
            }
            @layer theme {
                #early { border-color:blue !important; }
                #rollback { border-color:green !important; }
                #normal { border-color:blue; }
                #sides { border-color:green !important; }
            }
            #early { border-color:green !important; }
            #normal { border-color:green; }
            #mixed { border-color:green; }
            #sides { border-color:green !important; }
            #inline { border-color:red !important; }
        </style>
        <div id='early' class='box' role='button'>Early</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='sides' class='box' role='button'>Sides</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='inline' class='box' role='button' style='border-color:green !important'>Inline</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 100,
        device_scale_factor_milli: 1000,
    };
    let early = document.resolve_target("id=early").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [early, rollback, normal, mixed, sides, invalid, inline];
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let layout = document.layout(viewport).unwrap();
    for node_id in targets {
        let rectangle = layout.box_for(node_id).expect("border-color layout box");
        assert_eq!(rectangle.width, 20);
        assert_eq!(rectangle.height, 12);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
        );
    }

    let list = document.display_list(viewport).unwrap();
    let border_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::BorderRect {
                node_id: command_node_id,
                borders,
                ..
            } if *command_node_id == node_id => Some(*borders),
            _ => None,
        })
    };
    for (node_id, expected) in [
        (early, [NativeColor::RED; 4]),
        (rollback, [green; 4]),
        (normal, [green; 4]),
        (mixed, [NativeColor::RED; 4]),
        (sides, [blue, green, NativeColor::RED, NativeColor::RED]),
        (invalid, [NativeColor::RED; 4]),
        (inline, [green; 4]),
    ] {
        let borders = border_for(node_id).expect("border-color command");
        assert_eq!(
            [
                borders.top.color,
                borders.right.color,
                borders.bottom.color,
                borders.left.color,
            ],
            expected,
            "border colors for {node_id:?}"
        );
        assert_eq!(borders.top.width, 2);
        assert_eq!(borders.top.style, NativeBorderStyle::Solid);
    }
    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in targets.windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-color"
    }));
    let early_box = layout.box_for(early).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(early_box.x + 10, early_box.y),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_physical_border_width_important_priority_reaches_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #early { border-width:1px !important; }
                #rollback { border-width:1px !important; }
                #rollback { border-width:revert-layer !important; }
                #normal { border-width:1px; }
                #mixed { border-top-width:1px !important; }
                #sides { border-width:1px !important;border-top-width:2px !important;border-right-width:3px !important; }
                #invalid { border-width:1px !important;border-width:invalid !important; }
            }
            @layer theme {
                #early { border-width:2px !important; }
                #rollback { border-width:3px !important; }
                #normal { border-width:2px; }
                #sides { border-width:4px !important; }
            }
            #early { border-width:3px !important; }
            #normal { border-width:3px; }
            #mixed { border-width:4px; }
            #sides { border-width:5px !important; }
            #invalid { border-width:5px; }
        </style>
        <div id='early' class='box' role='button'>Early</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='sides' class='box' role='button'>Sides</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='inline' class='box' role='button' style='border-width:4px !important'>Inline</div>
        <div id='width-only' style='display:block;width:8px;height:6px;border-width:3px !important'>Only</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let early = document.resolve_target("id=early").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let width_only = document.resolve_target("id=width-only").unwrap();
    let targets = [early, rollback, normal, mixed, sides, invalid, inline];
    let layout = document.layout(viewport).unwrap();
    for (node_id, (expected_width, expected_height)) in [
        (early, (18, 10)),
        (rollback, (22, 14)),
        (normal, (22, 14)),
        (mixed, (24, 13)),
        (sides, (20, 11)),
        (invalid, (18, 10)),
        (inline, (24, 16)),
    ] {
        let rectangle = layout.box_for(node_id).expect("border-width layout box");
        assert_eq!(rectangle.width, expected_width);
        assert_eq!(rectangle.height, expected_height);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
        );
    }
    let width_only_box = layout.box_for(width_only).expect("width-only layout box");
    assert_eq!(width_only_box.width, 8);
    assert_eq!(width_only_box.height, 6);

    let list = document.display_list(viewport).unwrap();
    let border_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::BorderRect {
                node_id: command_node_id,
                borders,
                ..
            } if *command_node_id == node_id => Some(*borders),
            _ => None,
        })
    };
    for (node_id, expected_widths) in [
        (early, [1, 1, 1, 1]),
        (rollback, [3, 3, 3, 3]),
        (normal, [3, 3, 3, 3]),
        (mixed, [1, 4, 4, 4]),
        (sides, [2, 3, 1, 1]),
        (invalid, [1, 1, 1, 1]),
        (inline, [4, 4, 4, 4]),
    ] {
        let borders = border_for(node_id).expect("border-width command");
        assert_eq!(
            [
                borders.top.width,
                borders.right.width,
                borders.bottom.width,
                borders.left.width,
            ],
            expected_widths,
            "border widths for {node_id:?}"
        );
        assert_eq!(
            [
                borders.top.color,
                borders.right.color,
                borders.bottom.color,
                borders.left.color,
            ],
            [NativeColor::BLACK; 4]
        );
        assert_eq!(borders.top.style, NativeBorderStyle::Solid);
    }
    assert!(border_for(width_only).is_none());
    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in targets.windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-width"
    }));
    let early_box = layout.box_for(early).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(early_box.x + 10, early_box.y),
        Some([0, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_physical_border_style_important_priority_reaches_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #early { border-style:solid !important; }
                #rollback { border-style:dashed !important; }
                #rollback { border-style:revert-layer !important; }
                #normal { border-style:dotted; }
                #mixed { border-top-style:hidden !important; }
                #sides { border-style:solid !important;border-top-style:none !important;border-right-style:hidden !important; }
                #invalid { border-style:double !important;border-style:invalid !important; }
            }
            @layer theme {
                #early { border-style:dashed !important; }
                #rollback { border-style:groove !important; }
                #normal { border-style:double; }
                #sides { border-style:ridge !important; }
            }
            #early { border-style:dotted !important; }
            #normal { border-style:outset; }
            #mixed { border-style:double; }
            #sides { border-style:inset !important; }
            #invalid { border-style:dashed; }
        </style>
        <div id='early' class='box' role='button'>Early</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='sides' class='box' role='button'>Sides</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='inline' class='box' role='button' style='border-style:hidden !important'>Inline</div>
        <div id='style-only' style='display:block;width:8px;height:6px;border-style:solid !important'>Only</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let early = document.resolve_target("id=early").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let style_only = document.resolve_target("id=style-only").unwrap();
    let targets = [early, rollback, normal, mixed, sides, invalid, inline];
    let layout = document.layout(viewport).unwrap();
    for (node_id, (expected_width, expected_height)) in [
        (early, (20, 12)),
        (rollback, (20, 12)),
        (normal, (20, 12)),
        (mixed, (20, 10)),
        (sides, (18, 10)),
        (invalid, (20, 12)),
        (inline, (16, 8)),
    ] {
        let rectangle = layout.box_for(node_id).expect("border-style layout box");
        assert_eq!(rectangle.width, expected_width);
        assert_eq!(rectangle.height, expected_height);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
        );
    }
    let style_only_box = layout.box_for(style_only).expect("style-only layout box");
    assert_eq!((style_only_box.width, style_only_box.height), (8, 6));

    let list = document.display_list(viewport).unwrap();
    let border_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::BorderRect {
                node_id: command_node_id,
                borders,
                ..
            } if *command_node_id == node_id => Some(*borders),
            _ => None,
        })
    };
    for (node_id, expected_style) in [
        (early, NativeBorderStyle::Solid),
        (rollback, NativeBorderStyle::Groove),
        (normal, NativeBorderStyle::Outset),
        (invalid, NativeBorderStyle::Double),
    ] {
        let borders = border_for(node_id).expect("border-style command");
        assert_eq!(
            [
                borders.top.style,
                borders.right.style,
                borders.bottom.style,
                borders.left.style,
            ],
            [expected_style; 4],
            "border styles for {node_id:?}"
        );
        assert_eq!(
            [
                borders.top.width,
                borders.right.width,
                borders.bottom.width,
                borders.left.width,
            ],
            [2; 4]
        );
        assert_eq!(
            [
                borders.top.color,
                borders.right.color,
                borders.bottom.color,
                borders.left.color,
            ],
            [NativeColor::BLACK; 4]
        );
    }
    let mixed_border = border_for(mixed).expect("mixed border-style command");
    assert_eq!(mixed_border.top.width, 0);
    assert_eq!(mixed_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(mixed_border.right.style, NativeBorderStyle::Double);
    assert_eq!(mixed_border.bottom.style, NativeBorderStyle::Double);
    assert_eq!(mixed_border.left.style, NativeBorderStyle::Double);
    assert_eq!(mixed_border.right.width, 2);
    let sides_border = border_for(sides).expect("side border-style command");
    assert_eq!(sides_border.top.width, 0);
    assert_eq!(sides_border.right.width, 0);
    assert_eq!(sides_border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.left.style, NativeBorderStyle::Solid);
    assert!(border_for(inline).is_none());
    assert!(border_for(style_only).is_none());

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in targets.windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-style"
    }));
    let early_box = layout.box_for(early).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(early_box.x + 10, early_box.y),
        Some([0, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_physical_border_shorthand_important_priority_reaches_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #early { border:1px solid red !important; }
                #rollback { border:1px solid blue !important; }
                #rollback { border:revert-layer !important; }
                #normal { border:1px solid red; }
                #mixed { border-top:1px dashed blue !important; }
                #sides { border:1px solid red !important;border-top:2px dashed green !important;border-right:3px dotted blue !important; }
                #invalid { border:1px solid red !important;border:invalid !important; }
                #none { border:2px solid red !important;border-top:none !important; }
                #hidden { border:2px solid red !important;border-right:hidden !important; }
            }
            @layer theme {
                #early { border:2px dashed blue !important; }
                #rollback { border:3px groove green !important; }
                #normal { border:2px dashed blue; }
                #sides { border:4px groove green !important; }
            }
            #early { border:3px dotted green !important; }
            #normal { border:3px double green; }
            #mixed { border:4px double black; }
            #sides { border:5px ridge red !important; }
            #invalid { border:5px outset green; }
            #none { border-top:4px solid blue; }
            #hidden { border-right:4px solid blue; }
        </style>
        <div id='early' class='box' role='button'>Early</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='sides' class='box' role='button'>Sides</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='none' class='box' role='button'>None</div>
        <div id='hidden' class='box' role='button'>Hidden</div>
        <div id='inline' class='box' role='button' style='border:6px groove blue !important'>Inline</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let early = document.resolve_target("id=early").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let none = document.resolve_target("id=none").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [
        early, rollback, normal, mixed, sides, invalid, none, hidden, inline,
    ];
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let layout = document.layout(viewport).unwrap();
    for (node_id, (expected_width, expected_height)) in [
        (early, (18, 10)),
        (rollback, (22, 14)),
        (normal, (22, 14)),
        (mixed, (24, 13)),
        (sides, (20, 11)),
        (invalid, (18, 10)),
        (none, (20, 10)),
        (hidden, (18, 12)),
        (inline, (28, 20)),
    ] {
        let rectangle = layout
            .box_for(node_id)
            .expect("border-shorthand layout box");
        assert_eq!(rectangle.width, expected_width);
        assert_eq!(rectangle.height, expected_height);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
        );
    }

    let list = document.display_list(viewport).unwrap();
    let border_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::BorderRect {
                node_id: command_node_id,
                borders,
                ..
            } if *command_node_id == node_id => Some(*borders),
            _ => None,
        })
    };
    let early_border = border_for(early).expect("early border-shorthand command");
    assert_eq!(
        [
            early_border.top.width,
            early_border.right.width,
            early_border.bottom.width,
            early_border.left.width,
        ],
        [1; 4]
    );
    assert_eq!(
        [
            early_border.top.style,
            early_border.right.style,
            early_border.bottom.style,
            early_border.left.style,
        ],
        [NativeBorderStyle::Solid; 4]
    );
    assert_eq!(
        [
            early_border.top.color,
            early_border.right.color,
            early_border.bottom.color,
            early_border.left.color,
        ],
        [NativeColor::RED; 4]
    );

    let rollback_border = border_for(rollback).expect("rollback border-shorthand command");
    assert_eq!(rollback_border.top.width, 3);
    assert_eq!(rollback_border.top.style, NativeBorderStyle::Groove);
    assert_eq!(rollback_border.top.color, green);

    let mixed_border = border_for(mixed).expect("mixed border-shorthand command");
    assert_eq!(mixed_border.top.width, 1);
    assert_eq!(mixed_border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(mixed_border.top.color, blue);
    assert_eq!(mixed_border.right.width, 4);
    assert_eq!(mixed_border.right.style, NativeBorderStyle::Double);
    assert_eq!(mixed_border.right.color, NativeColor::BLACK);

    let sides_border = border_for(sides).expect("side border-shorthand command");
    assert_eq!(
        [
            sides_border.top.width,
            sides_border.right.width,
            sides_border.bottom.width,
            sides_border.left.width,
        ],
        [2, 3, 1, 1]
    );
    assert_eq!(sides_border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(sides_border.right.style, NativeBorderStyle::Dotted);
    assert_eq!(sides_border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.left.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.top.color, green);
    assert_eq!(sides_border.right.color, blue);
    assert_eq!(sides_border.bottom.color, NativeColor::RED);
    assert_eq!(sides_border.left.color, NativeColor::RED);

    let invalid_border = border_for(invalid).expect("invalid border-shorthand command");
    assert_eq!(invalid_border.top.width, 1);
    assert_eq!(invalid_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(invalid_border.top.color, NativeColor::RED);

    let none_border = border_for(none).expect("none border-shorthand command");
    assert_eq!(none_border.top.width, 0);
    assert_eq!(none_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(none_border.bottom.width, 2);
    assert_eq!(none_border.bottom.style, NativeBorderStyle::Solid);
    let hidden_border = border_for(hidden).expect("hidden border-shorthand command");
    assert_eq!(hidden_border.right.width, 0);
    assert_eq!(hidden_border.right.style, NativeBorderStyle::Solid);
    assert_eq!(hidden_border.left.width, 2);
    assert_eq!(hidden_border.left.style, NativeBorderStyle::Solid);

    let inline_border = border_for(inline).expect("inline border-shorthand command");
    assert_eq!(inline_border.top.width, 6);
    assert_eq!(inline_border.top.style, NativeBorderStyle::Groove);
    assert_eq!(inline_border.top.color, blue);

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in targets.windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border"
    }));
    let early_box = layout.box_for(early).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(early_box.x + 8, early_box.y),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn unsupported_resources_fail_without_state_mutation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(unsupported_resources_fail_without_state_mutation_inner());
    });
}

async fn unsupported_resources_fail_without_state_mutation_inner() {
    let backend = NativeEngineBackend::new(NativeEngineConfig::default()).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let before = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();

    let network = dispatcher
        .navigate(NavigationRequest {
            url: "ftp://example.com".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        network,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "navigation URL"
    ));
    let after = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(after, before);

    let script = dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "1 + 1".into(),
        })
        .await
        .unwrap();
    assert_eq!(script.value, serde_json::json!(2));
    dispatcher.close().await.unwrap();
}
#[test]
fn native_documents_import_nodes_into_the_target_document() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_documents_import_nodes_into_the_target_document_inner());
    });
}

async fn native_documents_import_nodes_into_the_target_document_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native import-node request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<!doctype html><template id='top-template'><span id='top-inside'>top</span><template id='top-nested'><b>top-nested</b></template></template><div id='top-target'></div><iframe id='child' src='/child'></iframe>"
                }
                Some("/child") => {
                    "<!doctype html><template id='frame-template'><span id='frame-inside'>frame</span><template id='frame-nested'><i>frame-nested</i></template></template><div id='frame-target'></div>"
                }
                other => panic!("unexpected native import-node request path: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let imported = session
        .script(
            r#"(() => {
                const childDocument = document.getElementById('child').contentDocument;
                const topTemplate = document.getElementById('top-template');
                const frameTemplate = childDocument.getElementById('frame-template');
                const topInertDocument = topTemplate.content.ownerDocument;
                const frameInertDocument = frameTemplate.content.ownerDocument;
                const importedIntoTop = document.importNode(frameTemplate, true);
                const importedIntoFrame = childDocument.importNode(topTemplate, {});
                const shallowDefault = document.importNode(frameTemplate);
                const shallowDictionary = childDocument.importNode(topTemplate, { selfOnly: true });
                const templateCopies = {
                  topRootOwner: importedIntoTop.ownerDocument === document,
                  topContentDifferent: importedIntoTop.content !== frameTemplate.content,
                  topContentOwner: importedIntoTop.content.ownerDocument === topInertDocument,
                  topChildOwner: importedIntoTop.content.querySelector('#frame-inside').ownerDocument === topInertDocument,
                  topNestedOwner: importedIntoTop.content.querySelector('#frame-nested').content.ownerDocument === topInertDocument,
                  topNestedChildOwner: importedIntoTop.content.querySelector('#frame-nested').content.querySelector('i').ownerDocument === topInertDocument,
                  frameRootOwner: importedIntoFrame.ownerDocument === childDocument,
                  frameContentDifferent: importedIntoFrame.content !== topTemplate.content,
                  frameContentOwner: importedIntoFrame.content.ownerDocument === frameInertDocument,
                  frameChildOwner: importedIntoFrame.content.querySelector('#top-inside').ownerDocument === frameInertDocument,
                  frameNestedChildOwner: importedIntoFrame.content.querySelector('#top-nested').content.querySelector('b').ownerDocument === frameInertDocument,
                  topShallowEmpty: shallowDefault.content.childNodes.length === 0,
                  frameShallowEmpty: shallowDictionary.content.childNodes.length === 0,
                  sourceUnchanged: topTemplate.content.querySelector('#top-inside').textContent === 'top'
                    && frameTemplate.content.querySelector('#frame-inside').textContent === 'frame',
                  detached: importedIntoTop.parentNode === null && importedIntoFrame.parentNode === null,
                };
                const source = document.createElement('article');
                source.setAttribute('data-origin', 'top');
                const sourceChild = document.createElement('strong');
                sourceChild.textContent = 'child';
                source.appendChild(sourceChild);
                const defaultShallow = document.importNode(source);
                const booleanShallow = document.importNode(source, false);
                const booleanDeep = document.importNode(source, true);
                const dictionaryDeep = childDocument.importNode(source, {});
                const shallowOptions = document.importNode(source, { selfOnly: true });
                const attr = document.createAttributeNS('http://www.w3.org/1999/xlink', 'xlink:title');
                attr.value = 'imported';
                const importedAttr = childDocument.importNode(attr);
                const type = document.implementation.createDocumentType('html', 'public', 'system');
                const importedType = childDocument.importNode(type);
                const fragment = document.createDocumentFragment();
                const fragmentChild = document.createElement('em');
                fragmentChild.textContent = 'fragment';
                fragment.appendChild(fragmentChild);
                const importedFragment = childDocument.importNode(fragment, true);
                const inertCopy = topInertDocument.importNode(frameTemplate, true);
                const frameInertCopy = frameInertDocument.importNode(topTemplate, true);
                const errors = {
                  document: (() => { try { document.importNode(document); return null; } catch (error) { return error.name; } })(),
                  invalidNode: (() => { try { document.importNode({ nodeType: 1 }); return null; } catch (error) { return error.name; } })(),
                  invalidOptions: (() => { try { document.importNode(source, 1); return null; } catch (error) { return error.name; } })(),
                  customRegistry: (() => { try { document.importNode(source, { customElementRegistry: {} }); return null; } catch (error) { return error.name; } })(),
                  nullCustomRegistry: (() => { try { document.importNode(source, { customElementRegistry: null }); return null; } catch (error) { return error.name; } })(),
                  inertCustomRegistry: (() => { try { topInertDocument.importNode(source, { customElementRegistry: customElements }); return null; } catch (error) { return error.name; } })(),
                };
                document.getElementById('top-target').appendChild(importedIntoTop);
                document.getElementById('top-target').appendChild(booleanDeep);
                childDocument.getElementById('frame-target').appendChild(importedIntoFrame);
                childDocument.getElementById('frame-target').appendChild(dictionaryDeep);
                return {
                  templateCopies,
                  ordinary: {
                    dynamicNodeBrand: source instanceof Node,
                    dynamicElementBrand: source instanceof Element,
                    defaultShallow: defaultShallow.childNodes.length === 0,
                    booleanShallow: booleanShallow.childNodes.length === 0,
                    optionsShallow: shallowOptions.childNodes.length === 0,
                    booleanDeep: booleanDeep.firstChild !== sourceChild
                      && booleanDeep.firstChild.ownerDocument === document,
                    dictionaryDeep: dictionaryDeep.firstChild !== sourceChild
                      && dictionaryDeep.firstChild.ownerDocument === childDocument
                      && dictionaryDeep.ownerDocument === childDocument,
                    sourceUnchanged: source.firstChild === sourceChild && sourceChild.ownerDocument === document,
                    importedDetached: defaultShallow.parentNode === null && dictionaryDeep.parentNode === childDocument.getElementById('frame-target'),
                  },
                  attribute: [importedAttr instanceof Attr, importedAttr.ownerDocument === childDocument,
                    importedAttr.ownerElement === null, importedAttr.namespaceURI, importedAttr.prefix,
                    importedAttr.localName, importedAttr.value],
                  documentType: [importedType instanceof DocumentType, importedType.ownerDocument === childDocument,
                    importedType.name, importedType.publicId, importedType.systemId],
                  fragment: [importedFragment instanceof DocumentFragment,
                    importedFragment.ownerDocument === childDocument,
                    importedFragment !== fragment, importedFragment.firstChild !== fragmentChild,
                    importedFragment.firstChild.parentNode === importedFragment,
                    importedFragment.firstChild.ownerDocument === childDocument,
                    fragment.firstChild === fragmentChild],
                  inert: [inertCopy.ownerDocument === topInertDocument,
                    inertCopy.content.ownerDocument === topInertDocument,
                    inertCopy.content.querySelector('#frame-inside').ownerDocument === topInertDocument,
                    frameInertCopy.ownerDocument === frameInertDocument,
                    frameInertCopy.content.ownerDocument === frameInertDocument,
                    frameInertCopy.content.querySelector('#top-inside').ownerDocument === frameInertDocument],
                  errors,
                };
              })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        imported.value,
        serde_json::json!({
            "templateCopies": {
                "topRootOwner": true,
                "topContentDifferent": true,
                "topContentOwner": true,
                "topChildOwner": true,
                "topNestedOwner": true,
                "topNestedChildOwner": true,
                "frameRootOwner": true,
                "frameContentDifferent": true,
                "frameContentOwner": true,
                "frameChildOwner": true,
                "frameNestedChildOwner": true,
                "topShallowEmpty": true,
                "frameShallowEmpty": true,
                "sourceUnchanged": true,
                "detached": true,
            },
            "ordinary": {
                "dynamicNodeBrand": true,
                "dynamicElementBrand": true,
                "defaultShallow": true,
                "booleanShallow": true,
                "optionsShallow": true,
                "booleanDeep": true,
                "dictionaryDeep": true,
                "sourceUnchanged": true,
                "importedDetached": true,
            },
            "attribute": [true, true, true, "http://www.w3.org/1999/xlink", "xlink", "title", "imported"],
            "documentType": [true, true, "html", "public", "system"],
            "fragment": [true, true, true, true, true, true, true],
            "inert": [true, true, true, true, true, true],
            "errors": {
                "document": "NotSupportedError",
                "invalidNode": "TypeError",
                "invalidOptions": "TypeError",
                "customRegistry": "TypeError",
                "nullCustomRegistry": "TypeError",
                "inertCustomRegistry": "NotSupportedError",
            },
        })
    );

    let persisted = session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; const topImported = document.querySelector('#top-target template'); const frameImported = childDocument.querySelector('#frame-target template'); return { topContent: topImported.content.querySelector('#frame-inside')?.textContent, topNested: topImported.content.querySelector('#frame-nested')?.content.querySelector('i')?.textContent, frameContent: frameImported.content.querySelector('#top-inside')?.textContent, frameNested: frameImported.content.querySelector('#top-nested')?.content.querySelector('b')?.textContent, topInertOwner: topImported.content.ownerDocument === document.getElementById('top-template').content.ownerDocument, frameInertOwner: frameImported.content.ownerDocument === childDocument.getElementById('frame-template').content.ownerDocument, topOrdinary: document.getElementById('top-target').querySelector('article strong')?.textContent, frameOrdinary: childDocument.getElementById('frame-target').querySelector('article strong')?.textContent }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        persisted.value,
        serde_json::json!({
            "topContent": "frame",
            "topNested": "frame-nested",
            "frameContent": "top",
            "frameNested": "top-nested",
            "topInertOwner": true,
            "frameInertOwner": true,
            "topOrdinary": "child",
            "frameOrdinary": "child",
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}

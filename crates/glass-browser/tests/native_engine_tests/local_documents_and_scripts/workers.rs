use super::*;

#[tokio::test]
async fn native_local_script_runs_dedicated_worker_and_delivers_messages() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-page",
            "<html><body><main id='output'>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-script",
            "self.onmessage = event => postMessage({ kind: 'reply', value: event.data + 1, href: self.location.href, hasDocument: typeof document !== 'undefined' }); postMessage({ kind: 'ready' });",
        )
        .unwrap()
        .with_initial_url("fixture://worker-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://worker-script'); worker.onmessage = event => workerMessages.push(event.data); [worker instanceof Worker, worker.url]",
            )
            .await
            .unwrap(),
        serde_json::json!([true, "fixture://worker-script"])
    );
    assert_eq!(
        engine
            .evaluate_async("worker.postMessage(4); workerMessages",)
            .await
            .unwrap(),
        serde_json::json!([{"kind": "ready"}])
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "kind": "ready"
        }, {
            "kind": "reply",
            "value": 5,
            "href": "fixture://worker-script",
            "hasDocument": false,
        }])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.dynamicWorkerMessages = []; globalThis.dynamicScript = document.createElement('script'); dynamicScript.textContent = \"globalThis.dynamicWorker = new Worker('fixture://worker-script'); dynamicWorker.onmessage = event => dynamicWorkerMessages.push(event.data);\"; document.documentElement.appendChild(dynamicScript); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("dynamicWorkerMessages")
            .await
            .unwrap(),
        serde_json::json!([{"kind": "ready"}])
    );
    assert_eq!(
        engine
            .evaluate_async("worker.terminate(); worker.postMessage(99); workerMessages.length",)
            .await
            .unwrap(),
        serde_json::json!(2)
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dedicated_worker_onmessage_false_cancels_before_listeners() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-handler-page",
            "<html><body><main>Worker event handler</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-handler-script",
            "onmessage = function(event) { globalThis.workerHandlerThis = this === self; return event.data === 'cancel' ? false : 0; }; addEventListener('message', () => false); addEventListener('message', event => postMessage([event.data, event instanceof MessageEvent, event.cancelable, event.defaultPrevented, event.target === self, event.currentTarget === self, globalThis.workerHandlerThis]));",
        )
        .unwrap()
        .with_initial_url("fixture://worker-handler-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://worker-handler-script'); worker.onmessage = event => workerMessages.push(event.data); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("worker.postMessage('cancel'); worker.postMessage('continue'); true")
            .await
            .unwrap(),
        serde_json::json!(true)
    );

    let expected = serde_json::json!([
        ["cancel", true, false, true, true, true, true],
        ["continue", true, false, false, true, true, true],
    ]);
    let mut messages = serde_json::Value::Null;
    for _ in 0..8 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages == expected {
            break;
        }
    }
    assert_eq!(messages, expected);
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_message_delivery_preserves_large_payloads() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://large-worker-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://large-worker-script",
            "self.onmessage = event => postMessage(event.data);",
        )
        .unwrap()
        .with_initial_url("fixture://large-worker-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(
            "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://large-worker-script'); worker.onmessage = event => workerMessages.push(event.data); true",
        )
        .await
        .unwrap();
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
}
#[tokio::test]
async fn native_local_message_channels_deliver_events_in_page_and_worker_realms() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://message-channel-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://message-channel-worker",
            "const channel = new MessageChannel(); channel.port2.onmessage = event => postMessage({ kind: 'port', value: event.data.value, target: event.target === channel.port2, currentTarget: event.currentTarget === channel.port2, messageEvent: event instanceof MessageEvent }); channel.port1.postMessage({ value: 7 });",
        )
        .unwrap()
        .with_initial_url("fixture://message-channel-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const channel = new MessageChannel();
                    const events = [];
                    channel.port2.onmessage = event => events.push([
                        event.data.value,
                        event.target === channel.port2,
                        event.currentTarget === channel.port2,
                        event instanceof MessageEvent,
                        event.ports.length,
                    ]);
                    const payload = { value: 1, nested: { ok: true } };
                    channel.port1.postMessage(payload);
                    payload.value = 9;
                    channel.port1.postMessage({ value: 2 });
                    const sender = new BroadcastChannel('native-message-channel');
                    const receiver = new BroadcastChannel('native-message-channel');
                    const broadcasts = [];
                    receiver.addEventListener('message', event => broadcasts.push([
                        event.data.value,
                        event.target === receiver,
                        event instanceof MessageEvent,
                    ]));
                    sender.postMessage({ value: 3 });
                    globalThis.messageChannelState = {
                        events,
                        broadcasts,
                        constructors: [
                            typeof MessageChannel,
                            typeof MessagePort,
                            typeof BroadcastChannel,
                            channel.port1 instanceof MessagePort,
                            channel.port1 instanceof EventTarget,
                        ],
                        senderCount: 0,
                    };
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("messageChannelState").await.unwrap(),
        serde_json::json!({
            "events": [[1, true, true, true, 0], [2, true, true, true, 0]],
            "broadcasts": [[3, true, true]],
            "constructors": ["function", "function", "function", true, true],
            "senderCount": 0,
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://message-channel-worker'); worker.onmessage = event => workerMessages.push(event.data); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "kind": "port",
            "value": 7,
            "target": true,
            "currentTarget": true,
            "messageEvent": true,
        }])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_timers_run_on_the_next_page_turn() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-timer-page",
            "<html><body><main id='output'>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-timer-script",
            "const interval = setInterval(() => postMessage({ kind: 'interval' }), 0); clearInterval(interval); setTimeout(() => postMessage({ kind: 'timeout', finite: Number.isFinite(performance.now()) }), 0); postMessage({ kind: 'ready' });",
        )
        .unwrap()
        .with_initial_url("fixture://worker-timer-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://worker-timer-script'); worker.onmessage = event => workerMessages.push(event.data); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([
            {"kind": "ready"},
            {"kind": "timeout", "finite": true},
        ])
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([
            {"kind": "ready"},
            {"kind": "timeout", "finite": true},
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_timers_round_robin_across_workers() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-timer-round-robin-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-timer-round-robin-script",
            "setTimeout(() => postMessage('timer'), 0);",
        )
        .unwrap()
        .with_initial_url("fixture://worker-timer-round-robin-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.first = new Worker('fixture://worker-timer-round-robin-script'); first.onmessage = event => workerMessages.push('first'); globalThis.second = new Worker('fixture://worker-timer-round-robin-script'); second.onmessage = event => workerMessages.push('second'); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!(["first"])
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!(["first", "second"])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_preloads_import_scripts_dependencies() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-import-page",
            "<html><body><main id='output'>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import-root",
            "importScripts('fixture://worker-import-dependency'); postMessage({ kind: 'loaded', value: globalThis.importedWorkerValue });",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import-dependency",
            "globalThis.importedWorkerValue = 7;",
        )
        .unwrap()
        .with_initial_url("fixture://worker-import-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://worker-import-root'); worker.onmessage = event => workerMessages.push(event.data); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{"kind": "loaded", "value": 7}])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_import_scripts_dynamic_import_uses_source_url() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-import.test/page.html",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import.test/workers/root.js",
            "importScripts('/scripts/bridge.js');",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import.test/scripts/bridge.js",
            "const target = './computed.js?source=bridge'; const configTarget = './settings.json'; Promise.all([import(target), import(configTarget, { with: { type: 'json' } })]).then(async ([module, config]) => postMessage({ kind: 'loaded', value: module.value, nested: await module.loadNested(), json: config.default.kind }), error => postMessage({ kind: 'error', message: String(error) }));",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import.test/scripts/computed.js?source=bridge",
            "export const value = 'local-computed'; export async function loadNested() { const target = './nested.js?source=computed'; return (await import(target)).value; }",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import.test/scripts/nested.js?source=computed",
            "export const value = 'local-nested';",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import.test/scripts/settings.json",
            r#"{"kind":"classic-worker-json"}"#,
        )
        .unwrap()
        .with_initial_url("fixture://worker-import.test/page.html");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('/workers/root.js'); worker.onmessage = event => workerMessages.push(event.data); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let mut messages = serde_json::Value::Null;
    for _ in 0..5 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages
            .as_array()
            .is_some_and(|messages| !messages.is_empty())
        {
            break;
        }
    }
    engine.close_async().await.unwrap();

    assert_eq!(
        messages,
        serde_json::json!([{
            "kind": "loaded",
            "value": "local-computed",
            "nested": "local-nested",
            "json": "classic-worker-json",
        }]),
        "fixture Worker imports must resolve from the imported classic script URL, including nested module imports"
    );
}
#[tokio::test]
async fn native_local_worker_import_scripts_nested_urls_use_worker_root_url() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-import-base.test/page.html",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import-base.test/workers/root.js",
            "importScripts('bridge/bridge.js'); postMessage(globalThis.nestedImportValue);",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import-base.test/workers/bridge/bridge.js",
            "importScripts('nested.js');",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-import-base.test/workers/nested.js",
            "globalThis.nestedImportValue = 'root-based';",
        )
        .unwrap()
        .with_initial_url("fixture://worker-import-base.test/page.html");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(
            "globalThis.workerMessages = []; globalThis.worker = new Worker('/workers/root.js'); worker.onmessage = event => workerMessages.push(event.data); true",
        )
        .await
        .unwrap();
    let mut messages = serde_json::Value::Null;
    for _ in 0..5 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages
            .as_array()
            .is_some_and(|messages| !messages.is_empty())
        {
            break;
        }
    }
    engine.close_async().await.unwrap();

    assert_eq!(
        messages,
        serde_json::json!(["root-based"]),
        "nested importScripts() specifiers resolve against the WorkerGlobalScope URL, not the imported script response URL"
    );
}
#[tokio::test]
async fn native_local_worker_exposes_url_search_params_and_navigator() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-runtime-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-runtime-script",
            "const url = new URL('https://example.test:8443/data?x=1&x=2#frag'); const relative = new URL('../asset?mode=1', 'https://example.test:8443/path/worker.js'); const params = new URLSearchParams('b=2&a=1&a=3'); params.sort(); params.append('space', 'hello world'); params.set('b', '4'); params.delete('missing'); postMessage({ sameGlobal: self === globalThis, hasDocument: typeof document !== 'undefined', location: [location.href, location.protocol, location.host, location.hostname, location.port, location.pathname], url: [url.href, url.origin, url.host, url.hostname, url.port, url.pathname, url.search, url.hash, url.searchParams.getAll('x')], relative: [relative.href, relative.pathname, relative.search], params: [params.toString(), params.getAll('a'), params.get('b'), params.has('space', 'hello world'), params.has('space', 'wrong'), params.size, typeof URL, typeof URLSearchParams], navigator: [navigator.userAgent, navigator.language, navigator.languages[0], navigator.onLine, navigator.cookieEnabled, navigator.hardwareConcurrency] });",
        )
        .unwrap()
        .with_initial_url("fixture://worker-runtime-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
        "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://worker-runtime-script'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "sameGlobal": true,
                "hasDocument": false,
                "location": [
                    "fixture://worker-runtime-script",
                "fixture:",
                "worker-runtime-script",
                "worker-runtime-script",
                "",
                "",
            ],
            "url": [
                    "https://example.test:8443/data?x=1&x=2#frag",
                    "https://example.test:8443",
                    "example.test:8443",
                    "example.test",
                    "8443",
                    "/data",
                    "?x=1&x=2",
                    "#frag",
                    ["1", "2"],
                ],
                "relative": [
                    "https://example.test:8443/asset?mode=1",
                    "/asset",
                    "?mode=1",
                ],
                "params": [
                    "a=1&a=3&b=4&space=hello+world",
                    ["1", "3"],
                    "4",
                    true,
                    false,
                    4,
                    "function",
                    "function",
                ],
                "navigator": ["GlassNative", "en-US", "en-US", true, true, 1],
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dedicated_worker_startup_errors_report_and_forward() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-script-error-page",
            r#"<script>
                globalThis.workerMessages = [];
                globalThis.ownerErrors = [];
                const captureMessage = (worker, event) => workerMessages.push(Object.assign({ worker }, event.data));
                globalThis.handledWorker = new Worker('fixture://worker-script-error-handled');
                handledWorker.onmessage = event => captureMessage('handled', event);
                handledWorker.onerror = event => ownerErrors.push({ kind: 'unexpected-handled-owner-error', message: event.message });
                globalThis.unhandledWorker = new Worker('fixture://worker-script-error-unhandled');
                unhandledWorker.onmessage = event => captureMessage('unhandled', event);
                unhandledWorker.onerror = function(event) {
                  ownerErrors.push({
                    kind: 'handler',
                    state: [event instanceof ErrorEvent, event.message, event.filename === this.url,
                      event.error === null, event.cancelable, event.defaultPrevented,
                      event.target === this, event.currentTarget === this, event.eventPhase]
                  });
                  return false;
                };
                unhandledWorker.addEventListener('error', event => ownerErrors.push({
                  kind: 'listener',
                  state: [event instanceof ErrorEvent, event.defaultPrevented,
                    event.target === unhandledWorker, event.currentTarget === unhandledWorker,
                    event.eventPhase]
                }));
                globalThis.syntaxWorker = new Worker('fixture://worker-script-error-syntax');
                syntaxWorker.onerror = event => ownerErrors.push({
                  kind: 'syntax-handler',
                  state: [event instanceof ErrorEvent, event.filename === syntaxWorker.url,
                    event.message.includes('script compilation failed'), event.defaultPrevented,
                    event.target === syntaxWorker, event.currentTarget === syntaxWorker,
                    event.eventPhase]
                });
                syntaxWorker.addEventListener('error', event => ownerErrors.push({
                  kind: 'syntax-listener', state: [event.defaultPrevented, event.eventPhase]
                }));
            </script><main>worker errors</main>"#,
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-script-error-handled",
            r#"globalThis.errorTrace = [];
                onmessage = event => postMessage({ kind: 'alive', value: event.data });
                onerror = function(message, filename, line, column, error) {
                  errorTrace.push(['handler', arguments.length, message.includes('handled-boom'),
                    filename === location.href, line, column, error instanceof Error]);
                  return true;
                };
                addEventListener('error', event => {
                  errorTrace.push(['listener', event instanceof ErrorEvent, event.type,
                    event.message.includes('handled-boom'), event.filename === location.href,
                    event.lineno, event.colno, event.error instanceof Error,
                    event.target === self, event.currentTarget === self,
                    event.cancelable, event.defaultPrevented, event.eventPhase]);
                  postMessage({ kind: 'global-error', trace: errorTrace.slice() });
                });
                throw new Error('handled-boom');"#,
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-script-error-unhandled",
            r#"globalThis.errorTrace = [];
                onmessage = event => postMessage({ kind: 'alive', value: event.data });
                onerror = function(message, filename, line, column, error) {
                  errorTrace.push(['handler', arguments.length, message.includes('unhandled-boom'),
                    filename === location.href, line, column, error instanceof Error]);
                  return false;
                };
                addEventListener('error', event => {
                  errorTrace.push(['listener', event instanceof ErrorEvent, event.type,
                    event.message.includes('unhandled-boom'), event.filename === location.href,
                    event.lineno, event.colno, event.error instanceof Error,
                    event.target === self, event.currentTarget === self,
                    event.cancelable, event.defaultPrevented, event.eventPhase]);
                  postMessage({ kind: 'global-error', trace: errorTrace.slice() });
                });
                throw new Error('unhandled-boom');"#,
        )
        .unwrap()
        .with_fixture("fixture://worker-script-error-syntax", "function {")
        .unwrap()
        .with_initial_url("fixture://worker-script-error-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(
            "handledWorker.postMessage('still-alive'); unhandledWorker.postMessage('still-alive'); true",
        )
        .await
        .unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"({
                    handled: workerMessages.filter(message => message.worker === 'handled').map(({ worker, ...message }) => message),
                    unhandled: workerMessages.filter(message => message.worker === 'unhandled').map(({ worker, ...message }) => message),
                    ownerErrors,
                })"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "handled": [
                {
                    "kind": "global-error",
                    "trace": [
                        ["handler", 5, true, true, 0, 0, true],
                        ["listener", true, "error", true, true, 0, 0, true, true, true, true, true, 2],
                    ],
                },
                {"kind": "alive", "value": "still-alive"},
            ],
            "unhandled": [
                {
                    "kind": "global-error",
                    "trace": [
                        ["handler", 5, true, true, 0, 0, true],
                        ["listener", true, "error", true, true, 0, 0, true, true, true, true, false, 2],
                    ],
                },
                {"kind": "alive", "value": "still-alive"},
            ],
            "ownerErrors": [
                {
                    "kind": "handler",
                    "state": [true, "unhandled-boom", true, true, true, false, true, true, 2],
                },
                {
                    "kind": "listener",
                    "state": [true, true, true, true, 2],
                },
                {
                    "kind": "syntax-handler",
                    "state": [true, true, true, false, true, true, 2],
                },
                {
                    "kind": "syntax-listener",
                    "state": [false, 2],
                },
            ],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dedicated_module_worker_startup_errors_report_and_forward() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://module-worker-script-error-page",
            r#"<script>
                globalThis.workerMessages = [];
                globalThis.ownerErrors = [];
                const captureMessage = (worker, event) => workerMessages.push(Object.assign({ worker }, event.data));
                globalThis.handledWorker = new Worker('fixture://module-worker-script-error-handled', { type: 'module' });
                handledWorker.onmessage = event => captureMessage('handled', event);
                handledWorker.onerror = event => ownerErrors.push({ kind: 'unexpected-handled-owner-error', message: event.message });
                globalThis.unhandledWorker = new Worker('fixture://module-worker-script-error-unhandled', { type: 'module' });
                unhandledWorker.onmessage = event => captureMessage('unhandled', event);
                unhandledWorker.onerror = function(event) {
                  ownerErrors.push({ kind: 'unhandled-handler', state: [
                    event instanceof ErrorEvent, event.message.includes('unhandled-module-boom'),
                    event.filename === this.url, event.error === null, event.cancelable,
                    event.defaultPrevented, event.target === this, event.currentTarget === this,
                    event.eventPhase,
                  ] });
                  return false;
                };
                unhandledWorker.addEventListener('error', event => ownerErrors.push({
                  kind: 'unhandled-listener', state: [event instanceof ErrorEvent,
                    event.defaultPrevented, event.target === unhandledWorker,
                    event.currentTarget === unhandledWorker, event.eventPhase]
                }));
                globalThis.syntaxWorker = new Worker('fixture://module-worker-script-error-syntax', { type: 'module' });
                syntaxWorker.onerror = event => ownerErrors.push({ kind: 'syntax-handler', state: [
                  event instanceof ErrorEvent, event.message.length > 0,
                  event.filename === syntaxWorker.url, event.error === null,
                  event.cancelable, event.defaultPrevented, event.target === syntaxWorker,
                  event.currentTarget === syntaxWorker, event.eventPhase,
                ] });
                syntaxWorker.addEventListener('error', event => ownerErrors.push({
                  kind: 'syntax-listener', state: [event.defaultPrevented, event.eventPhase]
                }));
                globalThis.graphWorker = new Worker('fixture://module-worker-script-error-graph', { type: 'module' });
                graphWorker.onerror = event => ownerErrors.push({ kind: 'graph-handler', state: [
                  event instanceof ErrorEvent, event.message.length > 0,
                  event.filename === graphWorker.url, event.error === null,
                  event.cancelable, event.defaultPrevented, event.target === graphWorker,
                  event.currentTarget === graphWorker, event.eventPhase,
                ] });
            </script><main>module worker errors</main>"#,
        )
        .unwrap()
        .with_fixture(
            "fixture://module-worker-script-error-handled",
            r#"globalThis.errorTrace = [];
                globalThis.onmessage = event => postMessage({ kind: 'alive', value: event.data });
                globalThis.onerror = function(message, filename, line, column, error) {
                  errorTrace.push(['handler', arguments.length, message.includes('handled-module-boom'),
                    filename === location.href, line, column, error instanceof Error]);
                  return true;
                };
                globalThis.addEventListener('error', event => {
                  errorTrace.push(['listener', event instanceof ErrorEvent, event.type,
                    event.message.includes('handled-module-boom'), event.filename === location.href,
                    event.lineno, event.colno, event.error instanceof Error,
                    event.target === self, event.currentTarget === self,
                    event.cancelable, event.defaultPrevented, event.eventPhase]);
                  postMessage({ kind: 'global-error', trace: errorTrace.slice() });
                });
                throw new Error('handled-module-boom');"#,
        )
        .unwrap()
        .with_fixture(
            "fixture://module-worker-script-error-unhandled",
            r#"globalThis.errorTrace = [];
                globalThis.onmessage = event => postMessage({ kind: 'alive', value: event.data });
                globalThis.onerror = function(message, filename, line, column, error) {
                  errorTrace.push(['handler', arguments.length, message.includes('unhandled-module-boom'),
                    filename === location.href, line, column, error instanceof Error]);
                  return false;
                };
                globalThis.addEventListener('error', event => {
                  errorTrace.push(['listener', event instanceof ErrorEvent, event.type,
                    event.message.includes('unhandled-module-boom'), event.filename === location.href,
                    event.lineno, event.colno, event.error instanceof Error,
                    event.target === self, event.currentTarget === self,
                    event.cancelable, event.defaultPrevented, event.eventPhase]);
                  postMessage({ kind: 'global-error', trace: errorTrace.slice() });
                });
                throw new Error('unhandled-module-boom');"#,
        )
        .unwrap()
        .with_fixture("fixture://module-worker-script-error-syntax", "function {")
        .unwrap()
        .with_fixture(
            "fixture://module-worker-script-error-graph",
            "import { missingValue } from 'fixture://module-worker-script-error-dependency'; globalThis.unusedMissingValue = missingValue;",
        )
        .unwrap()
        .with_fixture(
            "fixture://module-worker-script-error-dependency",
            "export const existingValue = 1;",
        )
        .unwrap()
        .with_initial_url("fixture://module-worker-script-error-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(
            "handledWorker.postMessage('still-alive'); unhandledWorker.postMessage('still-alive'); true",
        )
        .await
        .unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"({
                    handled: workerMessages.filter(message => message.worker === 'handled').map(({ worker, ...message }) => message),
                    unhandled: workerMessages.filter(message => message.worker === 'unhandled').map(({ worker, ...message }) => message),
                    ownerErrors,
                })"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "handled": [
                {
                    "kind": "global-error",
                    "trace": [
                        ["handler", 5, true, true, 0, 0, true],
                        ["listener", true, "error", true, true, 0, 0, true, true, true, true, true, 2],
                    ],
                },
                {"kind": "alive", "value": "still-alive"},
            ],
            "unhandled": [
                {
                    "kind": "global-error",
                    "trace": [
                        ["handler", 5, true, true, 0, 0, true],
                        ["listener", true, "error", true, true, 0, 0, true, true, true, true, false, 2],
                    ],
                },
                {"kind": "alive", "value": "still-alive"},
            ],
            "ownerErrors": [
                {
                    "kind": "unhandled-handler",
                    "state": [true, true, true, true, true, false, true, true, 2],
                },
                {
                    "kind": "unhandled-listener",
                    "state": [true, true, true, true, 2],
                },
                {
                    "kind": "syntax-handler",
                    "state": [true, true, true, true, true, false, true, true, 2],
                },
                {"kind": "syntax-listener", "state": [false, 2]},
                {
                    "kind": "graph-handler",
                    "state": [true, true, true, true, true, false, true, true, 2],
                },
            ],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_dedicated_worker_message_callback_errors_report_and_forward_in_order() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-message-callback-errors-page",
            r#"<script>
                globalThis.ownerEvents = [];
                const watch = (name, worker) => {
                  worker.onmessage = event => ownerEvents.push([name, "message", event.data]);
                  worker.onerror = event => ownerEvents.push([
                    name, "owner-error-handler", event.message,
                    event.filename === worker.url, event.error === null,
                    event.cancelable, event.defaultPrevented,
                    event.target === worker, event.currentTarget === worker,
                    event.eventPhase,
                  ]);
                  worker.addEventListener("error", event => ownerEvents.push([
                    name, "owner-error-listener", event.message,
                    event.defaultPrevented, event.target === worker,
                    event.currentTarget === worker, event.eventPhase,
                  ]));
                };
                globalThis.classicWorker = new Worker("fixture://worker-message-callback-errors-classic");
                watch("classic", classicWorker);
                globalThis.moduleWorker = new Worker(
                  "fixture://worker-message-callback-errors-module", { type: "module" });
                watch("module", moduleWorker);
            </script><main>worker message callback errors</main>"#,
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-message-callback-errors-classic",
            r#"globalThis.workerTrace = [];
                globalThis.onerror = function(message, filename, line, column, error) {
                  workerTrace.push(["global-handler", message, arguments.length,
                    filename === location.href, line, column, error instanceof Error]);
                  return true;
                };
                globalThis.addEventListener("error", event => workerTrace.push([
                  "global-listener", event.message, event instanceof ErrorEvent,
                  event.defaultPrevented, event.target === self,
                  event.currentTarget === self, event.eventPhase,
                ]));
                globalThis.onmessage = event => {
                  if (event.data === "trigger") {
                    workerTrace.push(["message-handler", "trigger"]);
                    postMessage({ kind: "before" });
                    throw new Error("classic-handler-boom");
                  }
                  workerTrace.push(["message-handler", event.data]);
                  postMessage({ kind: "alive", value: event.data });
                };
                globalThis.addEventListener("message", event => {
                  if (event.data === "trigger") {
                    workerTrace.push(["message-listener-one", "trigger"]);
                    throw new Error("classic-listener-boom");
                  }
                  workerTrace.push(["message-listener-one", event.data]);
                });
                globalThis.addEventListener("message", event => {
                  if (event.data === "trigger") {
                    workerTrace.push(["message-listener-two", event.defaultPrevented]);
                    postMessage({ kind: "trace", trace: workerTrace.slice() });
                  } else {
                    workerTrace.push(["message-listener-two", event.data]);
                  }
                });"#,
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-message-callback-errors-module",
            r#"globalThis.workerTrace = [];
                globalThis.onerror = function(message, filename, line, column, error) {
                  workerTrace.push(["global-handler", message, arguments.length,
                    filename === location.href, line, column, error instanceof Error]);
                  return false;
                };
                globalThis.addEventListener("error", event => workerTrace.push([
                  "global-listener", event.message, event instanceof ErrorEvent,
                  event.defaultPrevented, event.target === self,
                  event.currentTarget === self, event.eventPhase,
                ]));
                globalThis.onmessage = event => {
                  if (event.data === "trigger") {
                    workerTrace.push(["message-handler", "trigger"]);
                    postMessage({ kind: "before" });
                    throw new Error("module-handler-boom");
                  }
                  workerTrace.push(["message-handler", event.data]);
                  postMessage({ kind: "alive", value: event.data });
                };
                globalThis.addEventListener("message", event => {
                  if (event.data === "trigger") {
                    workerTrace.push(["message-listener-one", "trigger"]);
                    throw new Error("module-listener-boom");
                  }
                  workerTrace.push(["message-listener-one", event.data]);
                });
                globalThis.addEventListener("message", event => {
                  if (event.data === "trigger") {
                    workerTrace.push(["message-listener-two", event.defaultPrevented]);
                    postMessage({ kind: "trace", trace: workerTrace.slice() });
                  } else {
                    workerTrace.push(["message-listener-two", event.data]);
                  }
                });"#,
        )
        .unwrap()
        .with_initial_url("fixture://worker-message-callback-errors-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async("classicWorker.postMessage('trigger'); true")
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("ownerEvents").await.unwrap(),
        serde_json::json!([
            ["classic", "message", {"kind": "before"}],
            ["classic", "message", {
                "kind": "trace",
                "trace": [
                    ["message-handler", "trigger"],
                    ["global-handler", "classic-handler-boom", 5, true, 0, 0, true],
                    ["global-listener", "classic-handler-boom", true, true, true, true, 2],
                    ["message-listener-one", "trigger"],
                    ["global-handler", "classic-listener-boom", 5, true, 0, 0, true],
                    ["global-listener", "classic-listener-boom", true, true, true, true, 2],
                    ["message-listener-two", false],
                ],
            }],
        ])
    );

    engine
        .evaluate_async("moduleWorker.postMessage('trigger'); true")
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("ownerEvents").await.unwrap(),
        serde_json::json!([
            ["classic", "message", {"kind": "before"}],
            ["classic", "message", {
                "kind": "trace",
                "trace": [
                    ["message-handler", "trigger"],
                    ["global-handler", "classic-handler-boom", 5, true, 0, 0, true],
                    ["global-listener", "classic-handler-boom", true, true, true, true, 2],
                    ["message-listener-one", "trigger"],
                    ["global-handler", "classic-listener-boom", 5, true, 0, 0, true],
                    ["global-listener", "classic-listener-boom", true, true, true, true, 2],
                    ["message-listener-two", false],
                ],
            }],
            ["module", "message", {"kind": "before"}],
            ["module", "owner-error-handler", "module-handler-boom",
                true, true, true, false, true, true, 2],
            ["module", "owner-error-listener", "module-handler-boom",
                false, true, true, 2],
            ["module", "owner-error-handler", "module-listener-boom",
                true, true, true, false, true, true, 2],
            ["module", "owner-error-listener", "module-listener-boom",
                false, true, true, 2],
            ["module", "message", {
                "kind": "trace",
                "trace": [
                    ["message-handler", "trigger"],
                    ["global-handler", "module-handler-boom", 5, true, 0, 0, true],
                    ["global-listener", "module-handler-boom", true, false, true, true, 2],
                    ["message-listener-one", "trigger"],
                    ["global-handler", "module-listener-boom", 5, true, 0, 0, true],
                    ["global-listener", "module-listener-boom", true, false, true, true, 2],
                    ["message-listener-two", false],
                ],
            }],
        ])
    );

    engine
        .evaluate_async(
            "classicWorker.postMessage('after'); moduleWorker.postMessage('after'); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("ownerEvents.slice(-2)")
            .await
            .unwrap(),
        serde_json::json!([
            ["classic", "message", {"kind": "alive", "value": "after"}],
            ["module", "message", {"kind": "alive", "value": "after"}],
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_url_objects_are_mutable_and_search_params_are_live() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-url-mutation-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-url-mutation-script",
            "const url = new URL('/start?old=one', 'https://example.test:8443/base/page'); url.pathname = '/changed/../final'; url.searchParams.set('old', 'two words'); url.searchParams.append('z', '3'); url.hash = 'new fragment'; let invalid = ''; try { url.protocol = 'ftp'; } catch (error) { invalid = error.name; } postMessage({ instance: url instanceof URL, values: [url.href, url.origin, url.pathname, url.search, url.hash, url.searchParams.get('old'), url.searchParams.get('z')], invalid });",
        )
        .unwrap()
        .with_initial_url("fixture://worker-url-mutation-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://worker-url-mutation-script'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "instance": true,
                "values": [
                    "https://example.test:8443/final?old=two+words&z=3#new%20fragment",
                    "https://example.test:8443",
                    "/final",
                    "?old=two+words&z=3",
                    "#new%20fragment",
                    "two words",
                    "3",
                ],
                "invalid": "TypeError",
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_blob_object_urls_feed_fetch_and_xhr() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-blob-object-url-page",
            "<p>Worker Blob object URL</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-blob-object-url-script",
            r#"(async () => {
                const blob = new Blob(['worker object URL'], { type: 'text/custom' });
                const url = URL.createObjectURL(blob);
                const response = await fetch(url);
                const head = await fetch(url, { method: 'HEAD' });
                const xhr = await new Promise((resolve, reject) => {
                    const request = new XMLHttpRequest();
                    request.onload = () => resolve([request.status, request.responseURL, request.responseText]);
                    request.onerror = () => reject(new Error('worker object URL XHR failed'));
                    request.open('GET', url);
                    request.send();
                });
                const fetched = await response.text();
                URL.revokeObjectURL(url);
                const revoked = await fetch(url).then(() => false, () => true);
                postMessage({
                    url: url.startsWith('blob:'),
                    response: [response.status, response.url, response.headers.get('content-type'), fetched],
                    head: [head.status, head.body === null, head.headers.get('content-length')],
                    xhr,
                    revoked,
                });
            })().catch(error => postMessage({ error: String(error) }));"#,
        )
        .unwrap()
        .with_initial_url("fixture://worker-blob-object-url-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://worker-blob-object-url-script'); worker.onmessage = event => workerMessages.push(event.data); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "url": true,
            "response": [200, "blob:null/glass-worker-1", "text/custom", "worker object URL"],
            "head": [200, true, "17"],
            "xhr": [200, "blob:null/glass-worker-1", "worker object URL"],
            "revoked": true,
        }])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_blob_object_urls_cross_realm_messages() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-cross-realm-blob-page",
            "<p>Cross-realm Worker Blob URL</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-cross-realm-blob-script",
            r#"self.onmessage = async event => {
                try {
                    const response = await fetch(event.data.pageUrl);
                    const text = await response.text();
                    postMessage({
                        direction: 'page-to-worker',
                        text,
                        contentType: response.headers.get('content-type'),
                    });
                    const blob = new Blob(['worker to page'], { type: 'text/worker' });
                    postMessage({ direction: 'worker-to-page', url: URL.createObjectURL(blob) });
                } catch (error) {
                    postMessage({ error: String(error) });
                }
            };"#,
        )
        .unwrap()
        .with_initial_url("fixture://worker-cross-realm-blob-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"globalThis.workerMessages = [];
                globalThis.worker = new Worker('fixture://worker-cross-realm-blob-script');
                worker.onmessage = event => {
                    workerMessages.push(event.data);
                    if (event.data.direction === 'worker-to-page') {
                        const request = new XMLHttpRequest();
                        request.open('GET', event.data.url, false);
                        request.send();
                        workerMessages.push({ direction: 'worker-to-page-read', text: request.responseText });
                    }
                };
                globalThis.pageBlob = new Blob(['page to worker'], { type: 'text/page' });
                globalThis.pageBlobUrl = URL.createObjectURL(pageBlob);
                worker.postMessage({ pageUrl: pageBlobUrl });
                true"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([
            {
                "direction": "page-to-worker",
                "text": "page to worker",
                "contentType": "text/page"
            },
            {
                "direction": "worker-to-page",
                "url": "blob:null/glass-worker-1"
            },
            {
                "direction": "worker-to-page-read",
                "text": "worker to page"
            }
        ])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_fetch_honors_abort_signal() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-abort-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-abort-script",
            "const controller = new AbortController(); const events = []; controller.signal.onabort = event => events.push(event.type); const reason = new Error('cancelled'); controller.abort(reason); const combined = AbortSignal.any([controller.signal, new AbortController().signal]); fetch('fixture://worker-abort-page', { signal: controller.signal }).then(() => postMessage({ kind: 'unexpected' })).catch(error => postMessage({ kind: 'fetch', name: error.name, sameReason: error === reason, aborted: controller.signal.aborted })); postMessage({ kind: 'state', constructors: [typeof AbortController, typeof AbortSignal, typeof AbortSignal.timeout, typeof AbortSignal.any], state: [controller.signal.aborted, controller.signal.reason === reason, events, combined.aborted, combined.reason === reason, new Request('fixture://worker-abort-page').signal.aborted] });",
        )
        .unwrap()
        .with_initial_url("fixture://worker-abort-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://worker-abort-script'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [
                {
                    "kind": "state",
                    "constructors": ["function", "function", "function", "function"],
                    "state": [true, true, ["abort"], true, true, false],
                },
                {
                    "kind": "fetch",
                    "name": "Error",
                    "sameReason": true,
                    "aborted": true,
                },
            ],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_fetch_accepts_buffered_and_streamed_fixture_request_bodies() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-body.test/page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-body.test/script",
            "(async () => { const response = await fetch('/response', { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: 'fixture-body' }); const stream = new ReadableStream({ start(controller) { controller.enqueue('fixture-'); controller.enqueue('stream'); controller.close(); } }); const streamed = await fetch('/response', { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: stream }); const custom = await fetch('/response', { method: 'REPORT', body: 'custom-body' }); postMessage({ status: response.status, text: await response.text(), streamedStatus: streamed.status, streamedText: await streamed.text(), customStatus: custom.status, customText: await custom.text() }); })().catch(error => postMessage({ kind: 'error', name: error.name, message: error.message }));",
        )
        .unwrap()
        .with_fixture("fixture://worker-body.test/response", "fixture response")
        .unwrap()
        .with_initial_url("fixture://worker-body.test/page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://worker-body.test/script'); worker.onmessage = event => workerMessages.push(event.data); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "status": 200,
            "text": "fixture response",
            "streamedStatus": 200,
            "streamedText": "fixture response",
            "customStatus": 200,
            "customText": "fixture response",
        }])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_fetch_streams_fixture_response_body() {
    let response_body = format!("{}💩-tail", "x".repeat(8190));
    let expected_length = response_body.len();
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-stream.test/page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-stream.test/script",
            "(async () => { const response = await fetch('/response'); const clone = response.clone(); const reader = response.body.getReader(); const chunks = []; let total = 0; for (;;) { const part = await reader.read(); if (part.done) break; chunks.push(part.value.byteLength); total += part.value.byteLength; } reader.releaseLock(); const text = await clone.text(); postMessage({ status: response.status, url: response.url, chunks, total, textLength: text.length, prefix: text.slice(0, 3), suffix: text.slice(-5), containsEmoji: text.includes('💩'), bodyUsed: [response.bodyUsed, clone.bodyUsed] }); })().catch(error => postMessage({ kind: 'error', name: error.name, message: error.message }));",
        )
        .unwrap()
        .with_fixture("fixture://worker-stream.test/response", &response_body)
        .unwrap()
        .with_initial_url("fixture://worker-stream.test/page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.worker = new Worker('fixture://worker-stream.test/script'); worker.onmessage = event => workerMessages.push(event.data); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );

    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "status": 200,
            "url": "fixture://worker-stream.test/response",
            "chunks": [8192, expected_length - 8192],
            "total": expected_length,
            "textLength": response_body.encode_utf16().count(),
            "prefix": "xxx",
            "suffix": "-tail",
            "containsEmoji": true,
            "bodyUsed": [true, true],
        }])
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_exposes_standard_runtime_primitives() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-primitives-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-primitives-script",
            "const bytes = new TextEncoder().encode('hé'); const decoded = new TextDecoder().decode(bytes); const destination = new Uint8Array(4); const encoded = new TextEncoder().encodeInto('hi', destination); const target = new EventTarget(); const seen = []; target.addEventListener('ping', event => { seen.push([event.type, event.detail.ok, event.target === target, event.currentTarget === target]); event.preventDefault(); }); const dispatched = target.dispatchEvent(new CustomEvent('ping', { detail: { ok: true }, cancelable: true })); const clone = structuredClone({ nested: ['ok'] }); const message = new MessageEvent('message', { data: { value: 4 }, origin: 'https://example.test', lastEventId: 'event-1' }); const error = new ErrorEvent('error', { message: 'bad', filename: 'worker.js', lineno: 3, colno: 4 }); const exception = new DOMException('cancelled', 'AbortError'); const base64 = btoa('hello'); queueMicrotask(() => postMessage({ kind: 'microtask' })); postMessage({ kind: 'primitives', bytes: Array.from(bytes), decoded, encoded: [encoded.read, encoded.written, Array.from(destination)], base64: [base64, atob(base64)], events: [seen, dispatched], clone: clone.nested[0], message: [message.data.value, message.origin, message.lastEventId, message instanceof MessageEvent], error: [error.message, error.filename, error.lineno, error.colno, error instanceof ErrorEvent], exception: [exception.name, exception.message, exception.toString(), exception instanceof DOMException], constructors: [typeof TextEncoder, typeof TextDecoder, typeof EventTarget, typeof Event, typeof CustomEvent, typeof MessageEvent, typeof ErrorEvent, typeof DOMException, typeof structuredClone, typeof queueMicrotask] });",
        )
        .unwrap()
        .with_initial_url("fixture://worker-primitives-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://worker-primitives-script'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [
                {
                    "kind": "primitives",
                    "bytes": [104, 195, 169],
                    "decoded": "hé",
                    "encoded": [2, 2, [104, 105, 0, 0]],
                    "base64": ["aGVsbG8=", "hello"],
                    "events": [[["ping", true, true, true]], false],
                    "clone": "ok",
                    "message": [4, "https://example.test", "event-1", true],
                    "error": ["bad", "worker.js", 3, 4, true],
                    "exception": ["AbortError", "cancelled", "AbortError: cancelled", true],
                    "constructors": ["function", "function", "function", "function", "function", "function", "function", "function", "function", "function"],
                },
                { "kind": "microtask" },
            ],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_streams_compose_through_bounded_pipes() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-stream-composition-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-stream-composition-script",
            r#"(async () => {
                const pipeEvents = [];
                const sink = new WritableStream({
                    write(value) { pipeEvents.push('write:' + value); },
                    close() { pipeEvents.push('close'); },
                });
                const source = new ReadableStream({
                    start(controller) {
                        controller.enqueue('one');
                        controller.enqueue('two');
                        controller.close();
                    },
                });
                await source.pipeTo(sink);
                const writer = sink.getWriter();
                const writerClosed = writer.closed.then(() => 'resolved', error => 'rejected:' + error.name);
                const afterPipe = [source.locked, sink.locked, writer.desiredSize];
                const closedState = await writerClosed;
                writer.releaseLock();

                const failureEvents = [];
                const failingSource = new ReadableStream({
                    start(controller) { controller.enqueue('bad'); },
                    cancel(reason) { failureEvents.push('cancel:' + reason.message); },
                });
                const failingSink = new WritableStream({
                    write() { throw new Error('sink-failure'); },
                    abort(reason) { failureEvents.push('abort:' + reason.message); },
                });
                const failure = await failingSource.pipeTo(failingSink).then(
                    () => 'resolved',
                    error => error.message,
                );

                const transformEvents = [];
                const transform = new TransformStream({
                    start() { transformEvents.push('start'); },
                    transform(value, controller) {
                        transformEvents.push('transform:' + value);
                        controller.enqueue(value.toUpperCase());
                    },
                    flush(controller) {
                        transformEvents.push('flush');
                        controller.enqueue('tail');
                    },
                });
                const transformSource = new ReadableStream({
                    start(controller) {
                        controller.enqueue('one');
                        controller.enqueue('two');
                        controller.close();
                    },
                });
                const output = transformSource.pipeThrough(transform);
                const reader = output.getReader();
                const first = await reader.read();
                const second = await reader.read();
                const tail = await reader.read();
                const end = await reader.read();
                reader.releaseLock();
                postMessage({
                    kind: 'composition',
                    constructors: [typeof ReadableStream, typeof WritableStream, typeof TransformStream],
                    pipe: {
                        events: pipeEvents,
                        afterPipe,
                        writerClosed: closedState,
                        failure,
                        failureEvents,
                        sourceLocked: source.locked,
                        sinkLocked: sink.locked,
                        failingSourceLocked: failingSource.locked,
                        failingSinkLocked: failingSink.locked,
                    },
                    transform: {
                        events: transformEvents,
                        values: [first.value, second.value, tail.value, end.done],
                        identities: [output === transform.readable, transform.writable instanceof WritableStream, output instanceof ReadableStream],
                        sourceLocked: transformSource.locked,
                        outputLocked: output.locked,
                    },
                });
            })().catch(error => postMessage({ kind: 'error', name: error.name, message: error.message }));"#,
        )
        .unwrap()
        .with_initial_url("fixture://worker-stream-composition-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://worker-stream-composition-script'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    let expected = serde_json::json!({
        "messages": [{
            "kind": "composition",
            "constructors": ["function", "function", "function"],
            "pipe": {
                "events": ["write:one", "write:two", "close"],
                "afterPipe": [false, true, serde_json::Value::Null],
                "writerClosed": "resolved",
                "failure": "sink-failure",
                "failureEvents": ["abort:sink-failure", "cancel:sink-failure"],
                "sourceLocked": false,
                "sinkLocked": false,
                "failingSourceLocked": false,
                "failingSinkLocked": false,
            },
            "transform": {
                "events": ["start", "transform:one", "transform:two", "flush"],
                "values": ["ONE", "TWO", "tail", true],
                "identities": [true, true, true],
                "sourceLocked": false,
                "outputLocked": false,
            },
        }],
        "errors": [],
    });
    let mut messages = serde_json::json!({ "messages": [], "errors": [] });
    for _ in 0..16 {
        messages = engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap();
        if messages == expected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(messages, expected);
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_worker_exposes_os_seeded_crypto() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://worker-crypto-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://worker-crypto-script",
            "const values = new Uint8Array(8); crypto.getRandomValues(values); const uuid = crypto.randomUUID(); const bulk = new Uint8Array(16 * 1024); crypto.getRandomValues(bulk); const refill = new Uint8Array(16); crypto.getRandomValues(refill); let floatRejected = false; try { crypto.getRandomValues(new Float32Array(1)); } catch (error) { floatRejected = error instanceof TypeError; } postMessage({ byteLength: values.byteLength, bulkByteLength: bulk.byteLength, refillByteLength: refill.byteLength, uuidValid: /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(uuid), uuidVersion: uuid[14], uuidVariant: ['8', '9', 'a', 'b'].includes(uuid[19]), floatRejected, cryptoIdentity: crypto === globalThis.crypto });",
        )
        .unwrap()
        .with_initial_url("fixture://worker-crypto-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://worker-crypto-script'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "byteLength": 8,
                "bulkByteLength": 16384,
                "refillByteLength": 16,
                "uuidValid": true,
                "uuidVersion": "4",
                "uuidVariant": true,
                "floatRejected": true,
                "cryptoIdentity": true,
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_expose_subtle_digest() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://subtle-digest-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://subtle-digest-worker",
            "(async () => { const digest = await crypto.subtle.digest({ name: 'SHA-256' }, new TextEncoder().encode('hello')); postMessage({ digest: Array.from(new Uint8Array(digest)).map(value => value.toString(16).padStart(2, '0')).join('') }); })().catch(error => postMessage({ error: error.name }));",
        )
        .unwrap()
        .with_initial_url("fixture://subtle-digest-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const hex = bytes => Array.from(new Uint8Array(bytes))
                        .map(value => value.toString(16).padStart(2, '0')).join('');
                    const results = {};
                    for (const algorithm of ['SHA-1', 'SHA-256', 'SHA-384', 'SHA-512']) {
                        const digest = await crypto.subtle.digest(algorithm, new TextEncoder().encode('hello'));
                        results[algorithm] = hex(digest);
                    }
                    const objectAlgorithm = await crypto.subtle.digest(
                        { name: 'sha-256' }, new TextEncoder().encode('hello')
                    );
                    let unsupported = '';
                    try { await crypto.subtle.digest('MD5', new Uint8Array()); } catch (error) {
                        unsupported = error.name;
                    }
                    globalThis.subtleDigestState = {
                        results,
                        objectAlgorithm: hex(objectAlgorithm),
                        unsupported,
                        identity: crypto.subtle === globalThis.crypto.subtle,
                    };
                    return globalThis.subtleDigestState;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "results": {
                "SHA-1": "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d",
                "SHA-256": "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
                "SHA-384": "59e1748777448c69de6b800d7a33bbfb9ff1b463e44354c3553bcdb9c666fa90125a3c79f90397bdf5f6a13de828684f",
                "SHA-512": "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043",
            },
            "objectAlgorithm": "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "unsupported": "NotSupportedError",
            "identity": true,
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://subtle-digest-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "digest": "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_expose_hmac_crypto_keys() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://hmac-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://hmac-worker",
            "(async () => { const key = await crypto.subtle.importKey('raw', new Uint8Array([107, 101, 121]), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']); const signature = await crypto.subtle.sign('HMAC', key, new TextEncoder().encode('The quick brown fox jumps over the lazy dog')); postMessage({ signature: Array.from(new Uint8Array(signature)).map(value => value.toString(16).padStart(2, '0')).join(''), extractable: key.extractable, type: key.type, usage: key.usages[0] }); })().catch(error => postMessage({ error: error.name }));",
        )
        .unwrap()
        .with_initial_url("fixture://hmac-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const hex = bytes => Array.from(new Uint8Array(bytes))
                        .map(value => value.toString(16).padStart(2, '0')).join('');
                    const keyBytes = new Uint8Array([0, 107, 101, 121, 0]);
                    const keyView = new Uint8Array(keyBytes.buffer, 1, 3);
                    const messageBytes = new TextEncoder().encode('The quick brown fox jumps over the lazy dog');
                    const messageStorage = new Uint8Array(messageBytes.length + 2);
                    messageStorage.set(messageBytes, 1);
                    const messageView = new Uint8Array(messageStorage.buffer, 1, messageBytes.length);
                    const results = {};
                    for (const hash of ['SHA-1', 'SHA-256', 'SHA-384', 'SHA-512']) {
                        const key = await crypto.subtle.importKey(
                            'raw', keyView, { name: 'HMAC', hash }, true, ['sign', 'verify']
                        );
                        const signature = await crypto.subtle.sign({ name: 'HMAC' }, key, messageView);
                        const exported = await crypto.subtle.exportKey('raw', key);
                        const verified = await crypto.subtle.verify('HMAC', key, signature, messageView);
                        const tampered = new Uint8Array(signature).slice();
                        tampered[0] ^= 1;
                        const rejected = await crypto.subtle.verify('HMAC', key, tampered, messageView);
                        results[hash] = {
                            signature: hex(signature),
                            exported: Array.from(new Uint8Array(exported)),
                            verified,
                            rejected,
                            identity: key instanceof CryptoKey,
                            algorithm: [key.algorithm.name, key.algorithm.hash.name, key.algorithm.length],
                            usages: Array.from(key.usages),
                        };
                    }
                    const locked = await crypto.subtle.importKey(
                        'raw', keyView, { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']
                    );
                    let exportError = '';
                    try { await crypto.subtle.exportKey('raw', locked); } catch (error) { exportError = error.name; }
                    let usageError = '';
                    try { await crypto.subtle.verify('HMAC', locked, new Uint8Array(32), messageView); } catch (error) { usageError = error.name; }
                    globalThis.hmacKey = locked;
                    globalThis.hmacState = { results, exportError, usageError, cryptoKey: typeof CryptoKey };
                    return globalThis.hmacState;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "results": {
                "SHA-1": {
                    "signature": "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9",
                    "exported": [107, 101, 121],
                    "verified": true,
                    "rejected": false,
                    "identity": true,
                    "algorithm": ["HMAC", "SHA-1", 24],
                    "usages": ["sign", "verify"],
                },
                "SHA-256": {
                    "signature": "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
                    "exported": [107, 101, 121],
                    "verified": true,
                    "rejected": false,
                    "identity": true,
                    "algorithm": ["HMAC", "SHA-256", 24],
                    "usages": ["sign", "verify"],
                },
                "SHA-384": {
                    "signature": "d7f4727e2c0b39ae0f1e40cc96f60242d5b7801841cea6fc592c5d3e1ae50700582a96cf35e1e554995fe4e03381c237",
                    "exported": [107, 101, 121],
                    "verified": true,
                    "rejected": false,
                    "identity": true,
                    "algorithm": ["HMAC", "SHA-384", 24],
                    "usages": ["sign", "verify"],
                },
                "SHA-512": {
                    "signature": "b42af09057bac1e2d41708e48a902e09b5ff7f12ab428a4fe86653c73dd248fb82f948a549f7b791a5b41915ee4d1ec3935357e4e2317250d0372afa2ebeeb3a",
                    "exported": [107, 101, 121],
                    "verified": true,
                    "rejected": false,
                    "identity": true,
                    "algorithm": ["HMAC", "SHA-512", 24],
                    "usages": ["sign", "verify"],
                },
            },
            "exportError": "InvalidAccessError",
            "usageError": "InvalidAccessError",
            "cryptoKey": "function",
        })
    );
    assert_eq!(
        engine
            .evaluate_async("({ sameKey: hmacKey === globalThis.hmacKey, locked: hmacKey.extractable === false })")
            .await
            .unwrap(),
        serde_json::json!({ "sameKey": true, "locked": true })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://hmac-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "signature": "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
                "extractable": false,
                "type": "secret",
                "usage": "sign",
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_expose_aes_gcm_crypto_keys() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://aes-gcm-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://aes-gcm-worker",
            "(async () => { const key = await crypto.subtle.importKey('raw', new Uint8Array(32).fill(0x42), { name: 'AES-GCM' }, false, ['encrypt', 'decrypt']); const iv = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]); const aad = new TextEncoder().encode('worker-aad'); const plaintext = new TextEncoder().encode('worker plaintext'); const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: aad }, key, plaintext); const decrypted = await crypto.subtle.decrypt({ name: 'AES-GCM', iv, additionalData: aad }, key, ciphertext); const tampered = new Uint8Array(ciphertext).slice(); tampered[tampered.length - 1] ^= 1; let tamperError = ''; try { await crypto.subtle.decrypt({ name: 'AES-GCM', iv, additionalData: aad }, key, tampered); } catch (error) { tamperError = error.name; } postMessage({ plaintext: new TextDecoder().decode(decrypted), ciphertextLength: ciphertext.byteLength, tamperError, algorithm: [key.algorithm.name, key.algorithm.length], extractable: key.extractable, usages: Array.from(key.usages) }); })().catch(error => postMessage({ error: error.name }));",
        )
        .unwrap()
        .with_initial_url("fixture://aes-gcm-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const hex = bytes => Array.from(new Uint8Array(bytes))
                        .map(value => value.toString(16).padStart(2, '0')).join('');
                    const ivStorage = new Uint8Array(14);
                    ivStorage.set(new Uint8Array([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), 1);
                    const ivView = new Uint8Array(ivStorage.buffer, 1, 12);
                    const aadStorage = new TextEncoder().encode('xGlass AADy');
                    const aadView = new Uint8Array(aadStorage.buffer, 1, 9);
                    const plaintextStorage = new TextEncoder().encode('xNative plaintexty');
                    const plaintextView = new Uint8Array(plaintextStorage.buffer, 1, 16);
                    const results = {};
                    for (const length of [16, 24, 32]) {
                        const rawStorage = new Uint8Array(length + 2);
                        const raw = new Uint8Array(rawStorage.buffer, 1, length);
                        for (let index = 0; index < raw.length; index += 1) raw[index] = length === 16 ? 0 : index + 1;
                        const key = await crypto.subtle.importKey(
                            'raw', raw, { name: 'AES-GCM' }, true, ['encrypt', 'decrypt']
                        );
                        const algorithm = { name: 'AES-GCM', iv: ivView, additionalData: length === 16 ? undefined : aadView };
                        const encrypted = await crypto.subtle.encrypt(algorithm, key, length === 16 ? new Uint8Array(16) : plaintextView);
                        const decrypted = await crypto.subtle.decrypt(algorithm, key, encrypted);
                        const exported = await crypto.subtle.exportKey('raw', key);
                        const tampered = new Uint8Array(encrypted).slice();
                        tampered[tampered.length - 1] ^= 1;
                        let tamperError = '';
                        try { await crypto.subtle.decrypt(algorithm, key, tampered); } catch (error) { tamperError = error.name; }
                        results[length * 8] = {
                            algorithm: [key.algorithm.name, key.algorithm.length],
                            encrypted: hex(encrypted),
                            decrypted: hex(decrypted),
                            exported: Array.from(new Uint8Array(exported)),
                            identity: key instanceof CryptoKey,
                            tamperError,
                            usages: Array.from(key.usages),
                        };
                        if (length === 16) globalThis.aesKey = key;
                    }
                    const locked = await crypto.subtle.importKey(
                        'raw', new Uint8Array(16), { name: 'AES-GCM' }, false, ['encrypt']
                    );
                    let exportError = '';
                    try { await crypto.subtle.exportKey('raw', locked); } catch (error) { exportError = error.name; }
                    let ivError = '';
                    try { await crypto.subtle.encrypt({ name: 'AES-GCM', iv: new Uint8Array(11) }, locked, new Uint8Array()); } catch (error) { ivError = error.name; }
                    let tagError = '';
                    try { await crypto.subtle.encrypt({ name: 'AES-GCM', iv: new Uint8Array(12), tagLength: 96 }, locked, new Uint8Array()); } catch (error) { tagError = error.name; }
                    let keyError = '';
                    try { await crypto.subtle.importKey('raw', new Uint8Array(15), { name: 'AES-GCM' }, true, ['encrypt']); } catch (error) { keyError = error.name; }
                    globalThis.aesState = { results, exportError, ivError, tagError, keyError, cryptoKey: typeof CryptoKey };
                    return globalThis.aesState;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "results": {
                "128": {
                    "algorithm": ["AES-GCM", 128],
                    "encrypted": "0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bddf",
                    "decrypted": "00000000000000000000000000000000",
                    "exported": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                    "identity": true,
                    "tamperError": "OperationError",
                    "usages": ["encrypt", "decrypt"],
                },
                "192": {
                    "algorithm": ["AES-GCM", 192],
                    "encrypted": "358dffa808b8ec9efa7089120c69d985ebbdaa8ad8fe205b56615be0c60401cd",
                    "decrypted": "4e617469766520706c61696e74657874",
                    "exported": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24],
                    "identity": true,
                    "tamperError": "OperationError",
                    "usages": ["encrypt", "decrypt"],
                },
                "256": {
                    "algorithm": ["AES-GCM", 256],
                    "encrypted": "8818af70cb303595734b8d620355d28e23843642613f33d22a7d5810feb4d67b",
                    "decrypted": "4e617469766520706c61696e74657874",
                    "exported": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
                    "identity": true,
                    "tamperError": "OperationError",
                    "usages": ["encrypt", "decrypt"],
                },
            },
            "exportError": "InvalidAccessError",
            "ivError": "DataError",
            "tagError": "NotSupportedError",
            "keyError": "DataError",
            "cryptoKey": "function",
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "({ sameKey: aesKey === globalThis.aesKey, length: aesKey.algorithm.length })"
            )
            .await
            .unwrap(),
        serde_json::json!({ "sameKey": true, "length": 128 })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://aes-gcm-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "plaintext": "worker plaintext",
                "ciphertextLength": 32,
                "tamperError": "OperationError",
                "algorithm": ["AES-GCM", 256],
                "extractable": false,
                "usages": ["encrypt", "decrypt"],
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_generate_crypto_keys() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://generated-key-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://generated-key-worker",
            "(async () => { const hmac = await crypto.subtle.generateKey({ name: 'HMAC', hash: 'SHA-512', length: 256 }, false, ['sign']); const aes = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 192 }, false, ['encrypt', 'decrypt']); const iv = new Uint8Array(12); const data = new TextEncoder().encode('worker generated'); const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, aes, data); const plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, aes, ciphertext); const signature = await crypto.subtle.sign('HMAC', hmac, data); postMessage({ hmac: [hmac.algorithm.name, hmac.algorithm.hash.name, hmac.algorithm.length], aes: [aes.algorithm.name, aes.algorithm.length], plaintext: new TextDecoder().decode(plaintext), signatureLength: signature.byteLength, identity: hmac instanceof CryptoKey && aes instanceof CryptoKey, usages: Array.from(aes.usages) }); })().catch(error => postMessage({ error: error.name }));",
        )
        .unwrap()
        .with_initial_url("fixture://generated-key-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const data = new TextEncoder().encode('generated native key');
                    const hmacDefault = await crypto.subtle.generateKey(
                        { name: 'HMAC', hash: 'SHA-256' }, true, ['sign', 'verify']
                    );
                    const hmacCustom = await crypto.subtle.generateKey(
                        { name: 'HMAC', hash: { name: 'SHA-512' }, length: 256 }, true, ['sign']
                    );
                    const defaultSignature = await crypto.subtle.sign('HMAC', hmacDefault, data);
                    const defaultVerified = await crypto.subtle.verify('HMAC', hmacDefault, defaultSignature, data);
                    const defaultExport = await crypto.subtle.exportKey('raw', hmacDefault);
                    const customExport = await crypto.subtle.exportKey('raw', hmacCustom);
                    const iv = new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
                    const aes = {};
                    for (const length of [128, 192, 256]) {
                        const key = await crypto.subtle.generateKey({ name: 'AES-GCM', length }, true, ['encrypt', 'decrypt']);
                        const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, data);
                        const plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, key, ciphertext);
                        const exported = await crypto.subtle.exportKey('raw', key);
                        aes[length] = {
                            algorithm: [key.algorithm.name, key.algorithm.length],
                            keyBytes: exported.byteLength,
                            plaintext: new TextDecoder().decode(plaintext),
                            identity: key instanceof CryptoKey,
                            usages: Array.from(key.usages),
                        };
                    }
                    let hmacLengthError = '';
                    try { await crypto.subtle.generateKey({ name: 'HMAC', hash: 'SHA-256', length: 7 }, true, ['sign']); } catch (error) { hmacLengthError = error.name; }
                    let aesLengthError = '';
                    try { await crypto.subtle.generateKey({ name: 'AES-GCM', length: 160 }, true, ['encrypt']); } catch (error) { aesLengthError = error.name; }
                    let usageError = '';
                    try { await crypto.subtle.generateKey({ name: 'HMAC', hash: 'SHA-256' }, true, ['encrypt']); } catch (error) { usageError = error.name; }
                    let algorithmError = '';
                    try { await crypto.subtle.generateKey({ name: 'RSA-PSS' }, true, ['sign']); } catch (error) { algorithmError = error.name; }
                    globalThis.generatedHmac = hmacDefault;
                    globalThis.generatedState = {
                        hmacDefault: {
                            algorithm: [hmacDefault.algorithm.name, hmacDefault.algorithm.hash.name, hmacDefault.algorithm.length],
                            keyBytes: defaultExport.byteLength,
                            customKeyBytes: customExport.byteLength,
                            verified: defaultVerified,
                            identity: hmacDefault instanceof CryptoKey,
                            usages: Array.from(hmacDefault.usages),
                        },
                        aes,
                        hmacLengthError,
                        aesLengthError,
                        usageError,
                        algorithmError,
                    };
                    return globalThis.generatedState;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "hmacDefault": {
                "algorithm": ["HMAC", "SHA-256", 512],
                "keyBytes": 64,
                "customKeyBytes": 32,
                "verified": true,
                "identity": true,
                "usages": ["sign", "verify"],
            },
            "aes": {
                "128": {
                    "algorithm": ["AES-GCM", 128],
                    "keyBytes": 16,
                    "plaintext": "generated native key",
                    "identity": true,
                    "usages": ["encrypt", "decrypt"],
                },
                "192": {
                    "algorithm": ["AES-GCM", 192],
                    "keyBytes": 24,
                    "plaintext": "generated native key",
                    "identity": true,
                    "usages": ["encrypt", "decrypt"],
                },
                "256": {
                    "algorithm": ["AES-GCM", 256],
                    "keyBytes": 32,
                    "plaintext": "generated native key",
                    "identity": true,
                    "usages": ["encrypt", "decrypt"],
                },
            },
            "hmacLengthError": "DataError",
            "aesLengthError": "DataError",
            "usageError": "SyntaxError",
            "algorithmError": "NotSupportedError",
        })
    );
    assert_eq!(
        engine
            .evaluate_async("({ sameKey: generatedHmac === globalThis.generatedHmac, length: generatedHmac.algorithm.length })")
            .await
            .unwrap(),
        serde_json::json!({ "sameKey": true, "length": 512 })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://generated-key-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "hmac": ["HMAC", "SHA-512", 256],
                "aes": ["AES-GCM", 192],
                "plaintext": "worker generated",
                "signatureLength": 64,
                "identity": true,
                "usages": ["encrypt", "decrypt"],
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_derive_crypto_keys() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://derive-crypto-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://derive-crypto-worker",
            "(async () => { const hex = bytes => Array.from(new Uint8Array(bytes)).map(value => value.toString(16).padStart(2, '0')).join(''); const base = await crypto.subtle.importKey('raw', new Uint8Array([11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11]), { name: 'HKDF' }, false, ['deriveBits', 'deriveKey']); const algorithm = { name: 'HKDF', hash: 'SHA-256', salt: new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]), info: new Uint8Array([240, 241, 242, 243, 244, 245, 246, 247, 248, 249]) }; const bits = await crypto.subtle.deriveBits(algorithm, base, 336); const aes = await crypto.subtle.deriveKey(algorithm, base, { name: 'AES-GCM', length: 192 }, false, ['encrypt', 'decrypt']); const iv = new Uint8Array(12); const ciphertext = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, aes, new TextEncoder().encode('worker derived')); const plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, aes, ciphertext); postMessage({ bits: hex(bits), plaintext: new TextDecoder().decode(plaintext), ciphertextLength: ciphertext.byteLength, algorithm: [aes.algorithm.name, aes.algorithm.length], extractable: aes.extractable, usages: Array.from(aes.usages), baseAlgorithm: base.algorithm.name, baseExtractable: base.extractable }); })().catch(error => postMessage({ error: error.name }));",
        )
        .unwrap()
        .with_initial_url("fixture://derive-crypto-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const hex = bytes => Array.from(new Uint8Array(bytes))
                        .map(value => value.toString(16).padStart(2, '0')).join('');
                    const hkdfBase = await crypto.subtle.importKey(
                        'raw',
                        new Uint8Array([
                            11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11,
                            11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11,
                        ]),
                        { name: 'HKDF' },
                        false,
                        ['deriveBits', 'deriveKey'],
                    );
                    const hkdfAlgorithm = {
                        name: 'HKDF',
                        hash: 'SHA-256',
                        salt: new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
                        info: new Uint8Array([240, 241, 242, 243, 244, 245, 246, 247, 248, 249]),
                    };
                    const hkdfBits = await crypto.subtle.deriveBits(hkdfAlgorithm, hkdfBase, 336);
                    const derivedAes = await crypto.subtle.deriveKey(
                        hkdfAlgorithm,
                        hkdfBase,
                        { name: 'AES-GCM', length: 128 },
                        true,
                        ['encrypt', 'decrypt'],
                    );
                    const iv = new Uint8Array(12);
                    const encrypted = await crypto.subtle.encrypt(
                        { name: 'AES-GCM', iv },
                        derivedAes,
                        new TextEncoder().encode('derived AES'),
                    );
                    const decrypted = await crypto.subtle.decrypt(
                        { name: 'AES-GCM', iv },
                        derivedAes,
                        encrypted,
                    );
                    const passwordBase = await crypto.subtle.importKey(
                        'raw',
                        new TextEncoder().encode('password'),
                        { name: 'PBKDF2' },
                        false,
                        ['deriveBits', 'deriveKey'],
                    );
                    const pbkdf2One = await crypto.subtle.deriveBits(
                        { name: 'PBKDF2', hash: 'SHA-1', salt: new TextEncoder().encode('salt'), iterations: 1 },
                        passwordBase,
                        160,
                    );
                    const pbkdf2Two = await crypto.subtle.deriveBits(
                        { name: 'PBKDF2', hash: 'SHA-1', salt: new TextEncoder().encode('salt'), iterations: 2 },
                        passwordBase,
                        160,
                    );
                    const derivedHmac = await crypto.subtle.deriveKey(
                        { name: 'PBKDF2', hash: 'SHA-1', salt: new TextEncoder().encode('salt'), iterations: 2 },
                        passwordBase,
                        { name: 'HMAC', hash: 'SHA-256', length: 256 },
                        false,
                        ['sign', 'verify'],
                    );
                    const data = new TextEncoder().encode('derived HMAC');
                    const signature = await crypto.subtle.sign('HMAC', derivedHmac, data);
                    const verified = await crypto.subtle.verify('HMAC', derivedHmac, signature, data);
                    let baseExportError = '';
                    try { await crypto.subtle.exportKey('raw', hkdfBase); } catch (error) { baseExportError = error.name; }
                    let mismatchError = '';
                    try {
                        await crypto.subtle.deriveBits(
                            { name: 'PBKDF2', hash: 'SHA-1', salt: new Uint8Array(), iterations: 1 },
                            hkdfBase,
                            8,
                        );
                    } catch (error) { mismatchError = error.name; }
                    let lengthError = '';
                    try { await crypto.subtle.deriveBits(hkdfAlgorithm, hkdfBase, 7); } catch (error) { lengthError = error.name; }
                    let iterationsError = '';
                    try {
                        await crypto.subtle.deriveBits(
                            { name: 'PBKDF2', hash: 'SHA-1', salt: new Uint8Array(), iterations: 0 },
                            passwordBase,
                            8,
                        );
                    } catch (error) { iterationsError = error.name; }
                    globalThis.derivedCryptoState = {
                        hkdf: {
                            bits: hex(hkdfBits),
                            baseAlgorithm: hkdfBase.algorithm.name,
                            baseExtractable: hkdfBase.extractable,
                            baseUsages: Array.from(hkdfBase.usages),
                        },
                        aes: {
                            algorithm: [derivedAes.algorithm.name, derivedAes.algorithm.length],
                            extractable: derivedAes.extractable,
                            plaintext: new TextDecoder().decode(decrypted),
                            ciphertextLength: encrypted.byteLength,
                        },
                        pbkdf2: {
                            one: hex(pbkdf2One),
                            two: hex(pbkdf2Two),
                            hmacAlgorithm: [derivedHmac.algorithm.name, derivedHmac.algorithm.hash.name, derivedHmac.algorithm.length],
                            hmacExtractable: derivedHmac.extractable,
                            signatureLength: signature.byteLength,
                            verified,
                        },
                        errors: { baseExportError, mismatchError, lengthError, iterationsError },
                    };
                    return globalThis.derivedCryptoState;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "hkdf": {
                "bits": "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
                "baseAlgorithm": "HKDF",
                "baseExtractable": false,
                "baseUsages": ["deriveBits", "deriveKey"],
            },
            "aes": {
                "algorithm": ["AES-GCM", 128],
                "extractable": true,
                "plaintext": "derived AES",
                "ciphertextLength": 27,
            },
            "pbkdf2": {
                "one": "0c60c80f961f0e71f3a9b524af6012062fe037a6",
                "two": "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957",
                "hmacAlgorithm": ["HMAC", "SHA-256", 256],
                "hmacExtractable": false,
                "signatureLength": 32,
                "verified": true,
            },
            "errors": {
                "baseExportError": "InvalidAccessError",
                "mismatchError": "InvalidAccessError",
                "lengthError": "DataError",
                "iterationsError": "DataError",
            },
        })
    );
    assert_eq!(
        engine
            .evaluate_async("({ sameBase: derivedCryptoState.hkdf.baseAlgorithm === 'HKDF', aesLength: derivedCryptoState.aes.algorithm[1] })")
            .await
            .unwrap(),
        serde_json::json!({ "sameBase": true, "aesLength": 128 })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://derive-crypto-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "bits": "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
                "plaintext": "worker derived",
                "ciphertextLength": 30,
                "algorithm": ["AES-GCM", 192],
                "extractable": false,
                "usages": ["encrypt", "decrypt"],
                "baseAlgorithm": "HKDF",
                "baseExtractable": false,
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_wrap_crypto_keys() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://wrap-crypto-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://wrap-crypto-worker",
            "(async () => { const wrapper = await crypto.subtle.importKey('raw', new Uint8Array(16).fill(0x33), { name: 'AES-GCM' }, false, ['encrypt', 'decrypt']); const source = await crypto.subtle.importKey('raw', new Uint8Array([9, 8, 7, 6]), { name: 'HMAC', hash: 'SHA-256' }, true, ['sign', 'verify']); const iv = new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]); const wrapped = await crypto.subtle.wrapKey('raw', source, wrapper, { name: 'AES-GCM', iv }); const restored = await crypto.subtle.unwrapKey('raw', wrapped, { name: 'HMAC', hash: 'SHA-256' }, wrapper, { name: 'AES-GCM', iv }, true, ['sign', 'verify']); const data = new TextEncoder().encode('worker wrapped'); const signature = await crypto.subtle.sign('HMAC', restored, data); postMessage({ wrappedLength: wrapped.byteLength, exported: Array.from(new Uint8Array(await crypto.subtle.exportKey('raw', restored))), algorithm: [restored.algorithm.name, restored.algorithm.hash.name, restored.algorithm.length], verified: await crypto.subtle.verify('HMAC', restored, signature, data), extractable: restored.extractable }); })().catch(error => postMessage({ error: error.name }));",
        )
        .unwrap()
        .with_initial_url("fixture://wrap-crypto-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const wrappingKey = await crypto.subtle.importKey(
                        'raw', new Uint8Array(32).fill(0x5a), { name: 'AES-GCM' }, false,
                        ['encrypt', 'decrypt'],
                    );
                    const sourceBytes = new Uint8Array(32);
                    for (let index = 0; index < sourceBytes.length; index += 1) sourceBytes[index] = index + 1;
                    const sourceHmac = await crypto.subtle.importKey(
                        'raw', sourceBytes, { name: 'HMAC', hash: 'SHA-256' }, true,
                        ['sign', 'verify'],
                    );
                    const wrapAlgorithm = {
                        name: 'AES-GCM',
                        iv: new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
                        additionalData: new Uint8Array([21, 22, 23]),
                    };
                    const wrappedHmac = await crypto.subtle.wrapKey(
                        'raw', sourceHmac, wrappingKey, wrapAlgorithm,
                    );
                    const restoredHmac = await crypto.subtle.unwrapKey(
                        'raw', wrappedHmac, { name: 'HMAC', hash: 'SHA-256' },
                        wrappingKey, wrapAlgorithm, true, ['sign', 'verify'],
                    );
                    const hmacData = new TextEncoder().encode('wrapped HMAC');
                    const hmacSignature = await crypto.subtle.sign('HMAC', restoredHmac, hmacData);
                    const restoredHmacExport = await crypto.subtle.exportKey('raw', restoredHmac);
                    const sourceAes = await crypto.subtle.importKey(
                        'raw', new Uint8Array([16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]),
                        { name: 'AES-GCM' }, true, ['encrypt', 'decrypt'],
                    );
                    const wrappedAes = await crypto.subtle.wrapKey(
                        'raw', sourceAes, wrappingKey, { name: 'AES-GCM', iv: wrapAlgorithm.iv },
                    );
                    const restoredAes = await crypto.subtle.unwrapKey(
                        'raw', wrappedAes, { name: 'AES-GCM', length: 128 },
                        wrappingKey, { name: 'AES-GCM', iv: wrapAlgorithm.iv }, true,
                        ['encrypt', 'decrypt'],
                    );
                    const encrypted = await crypto.subtle.encrypt(
                        { name: 'AES-GCM', iv: new Uint8Array(12) },
                        restoredAes,
                        new TextEncoder().encode('wrapped AES'),
                    );
                    const decrypted = await crypto.subtle.decrypt(
                        { name: 'AES-GCM', iv: new Uint8Array(12) }, restoredAes, encrypted,
                    );
                    const tampered = new Uint8Array(wrappedHmac).slice();
                    tampered[tampered.length - 1] ^= 1;
                    let tamperError = '';
                    try {
                        await crypto.subtle.unwrapKey(
                            'raw', tampered, { name: 'HMAC', hash: 'SHA-256' },
                            wrappingKey, wrapAlgorithm, false, ['sign'],
                        );
                    } catch (error) { tamperError = error.name; }
                    const lockedHmac = await crypto.subtle.importKey(
                        'raw', new Uint8Array([1, 2, 3]), { name: 'HMAC', hash: 'SHA-256' }, false,
                        ['sign'],
                    );
                    let lockedError = '';
                    try { await crypto.subtle.wrapKey('raw', lockedHmac, wrappingKey, wrapAlgorithm); } catch (error) { lockedError = error.name; }
                    const decryptOnlyWrapper = await crypto.subtle.importKey(
                        'raw', new Uint8Array(16).fill(0x44), { name: 'AES-GCM' }, false, ['decrypt'],
                    );
                    let usageError = '';
                    try { await crypto.subtle.wrapKey('raw', sourceHmac, decryptOnlyWrapper, wrapAlgorithm); } catch (error) { usageError = error.name; }
                    let formatError = '';
                    try { await crypto.subtle.wrapKey('jwk', sourceHmac, wrappingKey, wrapAlgorithm); } catch (error) { formatError = error.name; }
                    globalThis.wrappedCryptoState = {
                        hmac: {
                            wrappedLength: wrappedHmac.byteLength,
                            exported: Array.from(new Uint8Array(restoredHmacExport)),
                            algorithm: [restoredHmac.algorithm.name, restoredHmac.algorithm.hash.name, restoredHmac.algorithm.length],
                            verified: await crypto.subtle.verify('HMAC', restoredHmac, hmacSignature, hmacData),
                            extractable: restoredHmac.extractable,
                            signatureLength: hmacSignature.byteLength,
                        },
                        aes: {
                            wrappedLength: wrappedAes.byteLength,
                            algorithm: [restoredAes.algorithm.name, restoredAes.algorithm.length],
                            plaintext: new TextDecoder().decode(decrypted),
                            extractable: restoredAes.extractable,
                            ciphertextLength: encrypted.byteLength,
                        },
                        errors: { tamperError, lockedError, usageError, formatError },
                    };
                    return globalThis.wrappedCryptoState;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "hmac": {
                "wrappedLength": 48,
                "exported": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
                "algorithm": ["HMAC", "SHA-256", 256],
                "verified": true,
                "extractable": true,
                "signatureLength": 32,
            },
            "aes": {
                "wrappedLength": 32,
                "algorithm": ["AES-GCM", 128],
                "plaintext": "wrapped AES",
                "extractable": true,
                "ciphertextLength": 27,
            },
            "errors": {
                "tamperError": "OperationError",
                "lockedError": "InvalidAccessError",
                "usageError": "InvalidAccessError",
                "formatError": "NotSupportedError",
            },
        })
    );
    assert_eq!(
        engine
            .evaluate_async("({ hmacLength: wrappedCryptoState.hmac.algorithm[2], aesLength: wrappedCryptoState.aes.algorithm[1] })")
            .await
            .unwrap(),
        serde_json::json!({ "hmacLength": 256, "aesLength": 128 })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://wrap-crypto-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "wrappedLength": 20,
                "exported": [9, 8, 7, 6],
                "algorithm": ["HMAC", "SHA-256", 32],
                "verified": true,
                "extractable": true,
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_jwk_crypto_keys() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://jwk-crypto-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://jwk-crypto-worker",
            "(async () => { const key = await crypto.subtle.importKey('jwk', { kty: 'oct', k: 'CQgHBg', alg: 'HS256', ext: true, key_ops: ['sign', 'verify'] }, { name: 'HMAC', hash: 'SHA-256' }, true, ['sign', 'verify']); const exported = await crypto.subtle.exportKey('jwk', key); const data = new TextEncoder().encode('worker JWK'); const signature = await crypto.subtle.sign('HMAC', key, data); postMessage({ jwk: [exported.kty, exported.k, exported.alg, exported.ext, Array.from(exported.key_ops)], verified: await crypto.subtle.verify('HMAC', key, signature, data), signatureLength: signature.byteLength, identity: key instanceof CryptoKey }); })().catch(error => postMessage({ error: error.name }));",
        )
        .unwrap()
        .with_initial_url("fixture://jwk-crypto-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const hmac = await crypto.subtle.importKey(
                        'jwk',
                        { kty: 'oct', k: 'AQIDBA', alg: 'HS256', ext: true, key_ops: ['sign', 'verify'] },
                        { name: 'HMAC', hash: 'SHA-256' },
                        true,
                        ['sign', 'verify'],
                    );
                    const exportedHmac = await crypto.subtle.exportKey('jwk', hmac);
                    const hmacData = new TextEncoder().encode('JWK HMAC');
                    const hmacSignature = await crypto.subtle.sign('HMAC', hmac, hmacData);
                    const aes = await crypto.subtle.importKey(
                        'jwk',
                        { kty: 'oct', k: 'EBESExQVFhcYGRobHB0eHw', alg: 'A128GCM', ext: true, key_ops: ['encrypt', 'decrypt'] },
                        { name: 'AES-GCM' },
                        true,
                        ['encrypt', 'decrypt'],
                    );
                    const exportedAes = await crypto.subtle.exportKey('jwk', aes);
                    const iv = new Uint8Array(12);
                    const ciphertext = await crypto.subtle.encrypt(
                        { name: 'AES-GCM', iv }, aes, new TextEncoder().encode('JWK AES'),
                    );
                    const plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, aes, ciphertext);
                    const locked = await crypto.subtle.importKey(
                        'jwk',
                        { kty: 'oct', k: 'AQIDBA', alg: 'HS256', ext: false, key_ops: ['sign'] },
                        { name: 'HMAC', hash: 'SHA-256' },
                        false,
                        ['sign'],
                    );
                    let lockedExportError = '';
                    try { await crypto.subtle.exportKey('jwk', locked); } catch (error) { lockedExportError = error.name; }
                    let algorithmError = '';
                    try {
                        await crypto.subtle.importKey(
                            'jwk', { kty: 'oct', k: 'AQIDBA', alg: 'HS512' },
                            { name: 'HMAC', hash: 'SHA-256' }, true, ['sign'],
                        );
                    } catch (error) { algorithmError = error.name; }
                    let keyOpsError = '';
                    try {
                        await crypto.subtle.importKey(
                            'jwk', { kty: 'oct', k: 'AQIDBA', key_ops: ['sign'] },
                            { name: 'HMAC', hash: 'SHA-256' }, true, ['verify'],
                        );
                    } catch (error) { keyOpsError = error.name; }
                    let ktyError = '';
                    try {
                        await crypto.subtle.importKey(
                            'jwk', { kty: 'RSA', k: 'AQIDBA' },
                            { name: 'HMAC', hash: 'SHA-256' }, true, ['sign'],
                        );
                    } catch (error) { ktyError = error.name; }
                    let extError = '';
                    try {
                        await crypto.subtle.importKey(
                            'jwk', { kty: 'oct', k: 'AQIDBA', ext: false },
                            { name: 'HMAC', hash: 'SHA-256' }, true, ['sign'],
                        );
                    } catch (error) { extError = error.name; }
                    globalThis.jwkCryptoState = {
                        hmac: {
                            exported: [exportedHmac.kty, exportedHmac.k, exportedHmac.alg, exportedHmac.ext, Array.from(exportedHmac.key_ops)],
                            algorithm: [hmac.algorithm.name, hmac.algorithm.hash.name, hmac.algorithm.length],
                            verified: await crypto.subtle.verify('HMAC', hmac, hmacSignature, hmacData),
                            signatureLength: hmacSignature.byteLength,
                        },
                        aes: {
                            exported: [exportedAes.kty, exportedAes.k, exportedAes.alg, exportedAes.ext, Array.from(exportedAes.key_ops)],
                            algorithm: [aes.algorithm.name, aes.algorithm.length],
                            plaintext: new TextDecoder().decode(plaintext),
                            ciphertextLength: ciphertext.byteLength,
                        },
                        errors: { lockedExportError, algorithmError, keyOpsError, ktyError, extError },
                    };
                    return globalThis.jwkCryptoState;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "hmac": {
                "exported": ["oct", "AQIDBA", "HS256", true, ["sign", "verify"]],
                "algorithm": ["HMAC", "SHA-256", 32],
                "verified": true,
                "signatureLength": 32,
            },
            "aes": {
                "exported": ["oct", "EBESExQVFhcYGRobHB0eHw", "A128GCM", true, ["encrypt", "decrypt"]],
                "algorithm": ["AES-GCM", 128],
                "plaintext": "JWK AES",
                "ciphertextLength": 23,
            },
            "errors": {
                "lockedExportError": "InvalidAccessError",
                "algorithmError": "DataError",
                "keyOpsError": "DataError",
                "ktyError": "DataError",
                "extError": "DataError",
            },
        })
    );
    assert_eq!(
        engine
            .evaluate_async("({ hmacKey: jwkCryptoState.hmac.exported[1], aesKey: jwkCryptoState.aes.exported[1] })")
            .await
            .unwrap(),
        serde_json::json!({ "hmacKey": "AQIDBA", "aesKey": "EBESExQVFhcYGRobHB0eHw" })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://jwk-crypto-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "jwk": ["oct", "CQgHBg", "HS256", true, ["sign", "verify"]],
                "verified": true,
                "signatureLength": 32,
                "identity": true,
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_aes_block_crypto() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://aes-block-crypto-page",
            "<html><body><main>Native</main></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://aes-block-crypto-worker",
            r#"(async () => {
                const bytes = value => Uint8Array.from(value.match(/../g), pair => parseInt(pair, 16));
                const hex = value => Array.from(new Uint8Array(value)).map(byte => byte.toString(16).padStart(2, '0')).join('');
                const keyBytes = bytes('2b7e151628aed2a6abf7158809cf4f3c');
                const cbcPlaintext = bytes('6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710');
                const cbcKey = await crypto.subtle.importKey('raw', keyBytes, { name: 'AES-CBC' }, false, ['encrypt', 'decrypt']);
                const cbcAlgorithm = { name: 'AES-CBC', iv: bytes('000102030405060708090a0b0c0d0e0f') };
                const cbcCiphertext = await crypto.subtle.encrypt(cbcAlgorithm, cbcKey, cbcPlaintext);
                const cbcPlain = await crypto.subtle.decrypt(cbcAlgorithm, cbcKey, cbcCiphertext);
                const tampered = new Uint8Array(cbcCiphertext).slice();
                tampered[tampered.length - 32] ^= 1;
                let cbcPaddingError = '';
                try { await crypto.subtle.decrypt(cbcAlgorithm, cbcKey, tampered); } catch (error) { cbcPaddingError = error.name; }
                const ctrKey = await crypto.subtle.importKey('raw', keyBytes, { name: 'AES-CTR' }, false, ['encrypt', 'decrypt']);
                const ctrAlgorithm = { name: 'AES-CTR', counter: bytes('f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff'), length: 128 };
                const ctrCiphertext = await crypto.subtle.encrypt(ctrAlgorithm, ctrKey, cbcPlaintext);
                const ctrPlain = await crypto.subtle.decrypt(ctrAlgorithm, ctrKey, ctrCiphertext);
                const generated = await Promise.all([128, 192, 256].map(length => crypto.subtle.generateKey({ name: 'AES-CTR', length }, false, ['encrypt', 'decrypt'])));
                postMessage({
                    cbcPrefix: hex(new Uint8Array(cbcCiphertext).subarray(0, 64)),
                    cbcLength: cbcCiphertext.byteLength,
                    cbcPlaintext: hex(cbcPlain),
                    cbcPaddingError,
                    ctrCiphertext: hex(ctrCiphertext),
                    ctrPlaintext: hex(ctrPlain),
                    generated: generated.map(key => [key.algorithm.name, key.algorithm.length]),
                });
            })().catch(error => postMessage({ error: error.name }));"#,
        )
        .unwrap()
        .with_initial_url("fixture://aes-block-crypto-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const bytes = value => Uint8Array.from(value.match(/../g), pair => parseInt(pair, 16));
                    const hex = value => Array.from(new Uint8Array(value)).map(byte => byte.toString(16).padStart(2, '0')).join('');
                    const keyBytes = bytes('2b7e151628aed2a6abf7158809cf4f3c');
                    const cbcPlaintext = bytes('6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710');
                    const cbcKey = await crypto.subtle.importKey('raw', keyBytes, { name: 'AES-CBC' }, false, ['encrypt', 'decrypt']);
                    const cbcAlgorithm = { name: 'AES-CBC', iv: bytes('000102030405060708090a0b0c0d0e0f') };
                    const cbcCiphertext = await crypto.subtle.encrypt(cbcAlgorithm, cbcKey, cbcPlaintext);
                    const cbcPlain = await crypto.subtle.decrypt(cbcAlgorithm, cbcKey, cbcCiphertext);
                    const tampered = new Uint8Array(cbcCiphertext).slice();
                    tampered[tampered.length - 32] ^= 1;
                    let cbcPaddingError = '';
                    try { await crypto.subtle.decrypt(cbcAlgorithm, cbcKey, tampered); } catch (error) { cbcPaddingError = error.name; }
                    let cbcIvError = '';
                    try { await crypto.subtle.encrypt({ name: 'AES-CBC', iv: new Uint8Array(15) }, cbcKey, cbcPlaintext); } catch (error) { cbcIvError = error.name; }
                    const ctrKey = await crypto.subtle.importKey('raw', keyBytes, { name: 'AES-CTR' }, false, ['encrypt', 'decrypt']);
                    const ctrAlgorithm = { name: 'AES-CTR', counter: bytes('f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff'), length: 128 };
                    const ctrCiphertext = await crypto.subtle.encrypt(ctrAlgorithm, ctrKey, cbcPlaintext);
                    const ctrPlain = await crypto.subtle.decrypt(ctrAlgorithm, ctrKey, ctrCiphertext);
                    let ctrLengthError = '';
                    try { await crypto.subtle.encrypt({ ...ctrAlgorithm, length: 0 }, ctrKey, cbcPlaintext); } catch (error) { ctrLengthError = error.name; }
                    const generated = await Promise.all([128, 192, 256].map(length => crypto.subtle.generateKey({ name: 'AES-CBC', length }, false, ['encrypt', 'decrypt'])));
                    const deriveBase = await crypto.subtle.importKey('raw', new Uint8Array(22).fill(7), { name: 'HKDF' }, false, ['deriveKey']);
                    const deriveAlgorithm = { name: 'HKDF', hash: 'SHA-256', salt: bytes('000102030405060708090a0b0c'), info: bytes('f0f1f2f3') };
                    const derivedCbc = await crypto.subtle.deriveKey(deriveAlgorithm, deriveBase, { name: 'AES-CBC', length: 128 }, false, ['encrypt', 'decrypt']);
                    const derivedCtr = await crypto.subtle.deriveKey(deriveAlgorithm, deriveBase, { name: 'AES-CTR', length: 256 }, false, ['encrypt', 'decrypt']);
                    return {
                        cbcPrefix: hex(new Uint8Array(cbcCiphertext).subarray(0, 64)),
                        cbcLength: cbcCiphertext.byteLength,
                        cbcPlaintext: hex(cbcPlain),
                        cbcPaddingError,
                        cbcIvError,
                        ctrCiphertext: hex(ctrCiphertext),
                        ctrPlaintext: hex(ctrPlain),
                        ctrLengthError,
                        generated: generated.map(key => [key.algorithm.name, key.algorithm.length]),
                        derived: [[derivedCbc.algorithm.name, derivedCbc.algorithm.length], [derivedCtr.algorithm.name, derivedCtr.algorithm.length]],
                    };
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "cbcPrefix": "7649abac8119b246cee98e9b12e9197d5086cb9b507219ee95db113a917678b273bed6b8e3c1743b7116e69e222295163ff1caa1681fac09120eca307586e1a7",
            "cbcLength": 80,
            "cbcPlaintext": "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
            "cbcPaddingError": "OperationError",
            "cbcIvError": "DataError",
            "ctrCiphertext": "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff5ae4df3edbd5d35e5b4f09020db03eab1e031dda2fbe03d1792170a0f3009cee",
            "ctrPlaintext": "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
            "ctrLengthError": "DataError",
            "generated": [["AES-CBC", 128], ["AES-CBC", 192], ["AES-CBC", 256]],
            "derived": [["AES-CBC", 128], ["AES-CTR", 256]],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://aes-block-crypto-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "cbcPrefix": "7649abac8119b246cee98e9b12e9197d5086cb9b507219ee95db113a917678b273bed6b8e3c1743b7116e69e222295163ff1caa1681fac09120eca307586e1a7",
                "cbcLength": 80,
                "cbcPlaintext": "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
                "cbcPaddingError": "OperationError",
                "ctrCiphertext": "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff5ae4df3edbd5d35e5b4f09020db03eab1e031dda2fbe03d1792170a0f3009cee",
                "ctrPlaintext": "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
                "generated": [["AES-CTR", 128], ["AES-CTR", 192], ["AES-CTR", 256]],
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_local_page_and_worker_ed25519_crypto_keys() {
    let worker_source = r#"(async () => {
        try {
            const privateJwk = {
                kty: 'OKP', crv: 'Ed25519',
                x: '11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo',
                d: 'nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A',
                alg: 'EdDSA', ext: true, key_ops: ['sign'],
            };
            const publicJwk = {
                kty: 'OKP', crv: 'Ed25519',
                x: privateJwk.x, alg: 'EdDSA', ext: true, key_ops: ['verify'],
            };
            const privateKey = await crypto.subtle.importKey(
                'jwk', privateJwk, { name: 'Ed25519' }, true, ['sign']
            );
            const publicKey = await crypto.subtle.importKey(
                'jwk', publicJwk, { name: 'Ed25519' }, true, ['verify']
            );
            const data = new Uint8Array();
            const signature = await crypto.subtle.sign('Ed25519', privateKey, data);
            const expected = new Uint8Array(signature);
            const rfcSignature = 'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b';
            const signatureHex = Array.from(expected).map(byte => byte.toString(16).padStart(2, '0')).join('');
            const verified = await crypto.subtle.verify('Ed25519', publicKey, expected, data);
            expected[0] ^= 1;
            const tampered = await crypto.subtle.verify('Ed25519', publicKey, expected, data);
            const generated = await crypto.subtle.generateKey({ name: 'Ed25519' }, false, ['sign', 'verify']);
            const generatedSignature = await crypto.subtle.sign({ name: 'Ed25519' }, generated.privateKey, new TextEncoder().encode('worker'));
            const generatedVerified = await crypto.subtle.verify({ name: 'Ed25519' }, generated.publicKey, generatedSignature, new TextEncoder().encode('worker'));
            const exportedPrivate = await crypto.subtle.exportKey('jwk', privateKey);
            const exportedPublic = await crypto.subtle.exportKey('jwk', publicKey);
            postMessage({
                signatureHex, rfcSignature, verified, tampered, generatedVerified,
                private: [privateKey.type, privateKey.algorithm.name, privateKey.extractable, Array.from(privateKey.usages)],
                public: [publicKey.type, publicKey.algorithm.name, publicKey.extractable, Array.from(publicKey.usages)],
                exported: [exportedPrivate.kty, exportedPrivate.crv, exportedPrivate.x, exportedPrivate.d, exportedPublic.x],
                raw: Array.from(new Uint8Array(await crypto.subtle.exportKey('raw', publicKey))),
            });
        } catch (error) {
            postMessage({ error: error.name, message: String(error.message) });
        }
    })();"#;
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://ed25519-crypto-page",
                "<html><body><main>Native</main></body></html>",
            )
            .unwrap()
            .with_fixture("fixture://ed25519-crypto-worker", worker_source)
            .unwrap()
            .with_initial_url("fixture://ed25519-crypto-page"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const privateJwk = {
                        kty: 'OKP', crv: 'Ed25519',
                        x: '11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo',
                        d: 'nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A',
                        alg: 'EdDSA', ext: true, key_ops: ['sign'],
                    };
                    const publicJwk = {
                        kty: 'OKP', crv: 'Ed25519',
                        x: privateJwk.x, alg: 'EdDSA', ext: true, key_ops: ['verify'],
                    };
                    const privateKey = await crypto.subtle.importKey(
                        'jwk', privateJwk, { name: 'Ed25519' }, true, ['sign']
                    );
                    const publicKey = await crypto.subtle.importKey(
                        'jwk', publicJwk, { name: 'Ed25519' }, true, ['verify']
                    );
                    const data = new Uint8Array();
                    const signature = await crypto.subtle.sign('Ed25519', privateKey, data);
                    const signatureBytes = new Uint8Array(signature);
                    const signatureHex = Array.from(signatureBytes).map(byte => byte.toString(16).padStart(2, '0')).join('');
                    const rfcSignature = 'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b';
                    const verified = await crypto.subtle.verify('Ed25519', publicKey, signatureBytes, data);
                    signatureBytes[0] ^= 1;
                    const tampered = await crypto.subtle.verify('Ed25519', publicKey, signatureBytes, data);
                    const generated = await crypto.subtle.generateKey({ name: 'Ed25519' }, false, ['sign', 'verify']);
                    const generatedData = new TextEncoder().encode('page');
                    const generatedSignature = await crypto.subtle.sign({ name: 'Ed25519' }, generated.privateKey, generatedData);
                    const generatedVerified = await crypto.subtle.verify({ name: 'Ed25519' }, generated.publicKey, generatedSignature, generatedData);
                    const exportedPrivate = await crypto.subtle.exportKey('jwk', privateKey);
                    const exportedPublic = await crypto.subtle.exportKey('jwk', publicKey);
                    let badUsage = '';
                    try { await crypto.subtle.importKey('raw', new Uint8Array(32), { name: 'Ed25519' }, true, ['sign']); } catch (error) { badUsage = error.name; }
                    let mismatch = '';
                    try { await crypto.subtle.importKey('jwk', { ...privateJwk, x: publicJwk.x.slice(0, -1) + 'A' }, { name: 'Ed25519' }, true, ['sign']); } catch (error) { mismatch = error.name; }
                    return {
                        signatureHex, rfcSignature, verified,
                        tampered, generatedVerified, badUsage, mismatch,
                        private: [privateKey.type, privateKey.algorithm.name, privateKey.extractable, Array.from(privateKey.usages)],
                        public: [publicKey.type, publicKey.algorithm.name, publicKey.extractable, Array.from(publicKey.usages)],
                        generated: [generated.privateKey.type, generated.publicKey.type, generated.publicKey.extractable],
                        exported: [exportedPrivate.kty, exportedPrivate.crv, exportedPrivate.x, exportedPrivate.d, exportedPublic.x],
                        raw: Array.from(new Uint8Array(await crypto.subtle.exportKey('raw', publicKey))),
                    };
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!({
            "signatureHex": "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            "rfcSignature": "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            "verified": true,
            "tampered": false,
            "generatedVerified": true,
            "badUsage": "SyntaxError",
            "mismatch": "DataError",
            "private": ["private", "Ed25519", true, ["sign"]],
            "public": ["public", "Ed25519", true, ["verify"]],
            "generated": ["private", "public", true],
            "exported": ["OKP", "Ed25519", "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo", "nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A", "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"],
            "raw": [215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114, 243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.workerMessages = []; globalThis.workerErrors = []; globalThis.worker = new Worker('fixture://ed25519-crypto-worker'); worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ messages: workerMessages, errors: workerErrors })")
            .await
            .unwrap(),
        serde_json::json!({
            "messages": [{
                "signatureHex": "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
                "rfcSignature": "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
                "verified": true,
                "tampered": false,
                "generatedVerified": true,
                "private": ["private", "Ed25519", true, ["sign"]],
                "public": ["public", "Ed25519", true, ["verify"]],
                "exported": ["OKP", "Ed25519", "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo", "nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A", "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"],
                "raw": [215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114, 243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26],
            }],
            "errors": [],
        })
    );
    engine.close_async().await.unwrap();
}
#[test]
fn native_local_shared_worker_reuses_named_runtime_and_ports() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = "<script>globalThis.sharedEvents = []; const capture = (port, event) => sharedEvents.push(Object.assign({ port }, event.data)); globalThis.shared = new SharedWorker('/shared-worker-script', { name: 'glass-shared' }); globalThis.sharedAgain = new SharedWorker('/shared-worker-script', { name: 'glass-shared' }); shared.port.onmessage = event => capture('one', event); sharedAgain.port.onmessage = event => capture('two', event); shared.port.start(); sharedAgain.port.start();</script><main>shared worker</main>";
    let worker = "let connections = 0; onconnect = function(event) { const port = event.ports[0]; const connection = ++connections; globalThis.sharedConnectNumber = connection; globalThis.sharedConnectHandlerThis = this === self; port.onmessage = message => port.postMessage({ kind: 'reply', value: Number(message.data.value) + connection }); port.start(); return connection === 1 ? false : 0; }; addEventListener('connect', event => { const port = event.source; port.postMessage({ kind: 'ready', connections: globalThis.sharedConnectNumber, eventType: event.type, messageEvent: event instanceof MessageEvent, data: event.data, sourceMatches: event.source === port, portsMatch: event.ports.length === 1 && event.ports[0] === port, portsFrozen: Object.isFrozen(event.ports), bubbles: event.bubbles, cancelable: event.cancelable, defaultPrevented: event.defaultPrevented, targetIsWorker: event.target === self, currentTargetIsWorker: event.currentTarget === self, handlerThis: globalThis.sharedConnectHandlerThis }); return false; });";
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("page and shared worker should request their scripts")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (content_type, body) = match path {
                "/shared-worker-page" => ("text/html", page),
                "/shared-worker-script" => ("text/javascript", worker),
                other => panic!("unexpected SharedWorker fixture request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let page_url = format!("http://{address}/shared-worker-page");
    let config = NativeEngineConfig::default().with_initial_url(page_url);
    // SharedWorker storage identity belongs to the browser owner. Exercise the
    // worker through the runtime session adapter that supplies this context.
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();
    assert_eq!(
        session.script("sharedEvents").await.unwrap().value,
        serde_json::json!([
            {
                "port": "one", "kind": "ready", "connections": 1,
                "eventType": "connect", "messageEvent": true, "data": "",
                "sourceMatches": true, "portsMatch": true, "portsFrozen": true,
                "bubbles": false, "cancelable": false, "defaultPrevented": true,
                "targetIsWorker": true, "currentTargetIsWorker": true, "handlerThis": true,
            },
            {
                "port": "two", "kind": "ready", "connections": 2,
                "eventType": "connect", "messageEvent": true, "data": "",
                "sourceMatches": true, "portsMatch": true, "portsFrozen": true,
                "bubbles": false, "cancelable": false, "defaultPrevented": false,
                "targetIsWorker": true, "currentTargetIsWorker": true, "handlerThis": true,
            },
        ])
    );
    assert_eq!(
        session
            .script("shared.port.postMessage({ value: 4 }); sharedAgain.port.postMessage({ value: 7 }); true")
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
    );
    assert_eq!(
        session.script("sharedEvents").await.unwrap().value,
        serde_json::json!([
            {
                "port": "one", "kind": "ready", "connections": 1,
                "eventType": "connect", "messageEvent": true, "data": "",
                "sourceMatches": true, "portsMatch": true, "portsFrozen": true,
                "bubbles": false, "cancelable": false, "defaultPrevented": true,
                "targetIsWorker": true, "currentTargetIsWorker": true, "handlerThis": true,
            },
            {
                "port": "two", "kind": "ready", "connections": 2,
                "eventType": "connect", "messageEvent": true, "data": "",
                "sourceMatches": true, "portsMatch": true, "portsFrozen": true,
                "bubbles": false, "cancelable": false, "defaultPrevented": false,
                "targetIsWorker": true, "currentTargetIsWorker": true, "handlerThis": true,
            },
            {"port": "one", "kind": "reply", "value": 5},
            {"port": "two", "kind": "reply", "value": 9},
        ])
    );
    session.close().await.unwrap();
    server.await.unwrap();
        });
    });
}

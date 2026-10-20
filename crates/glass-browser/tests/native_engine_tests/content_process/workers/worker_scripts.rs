use super::*;

#[tokio::test]
async fn native_content_process_dedicated_worker_message_port_callback_errors_report_and_forward() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = r#"<script>
globalThis.portEvents = [];
globalThis.ownerEvents = [];
globalThis.worker = new Worker('/dedicated-port-errors.js');
worker.onerror = event => {
  ownerEvents.push([
    'handler', event.message, event.filename === worker.url, event.error === null,
    event.cancelable, event.defaultPrevented, event.target === worker,
    event.currentTarget === worker, event.eventPhase,
  ]);
};
worker.addEventListener('error', event => {
  ownerEvents.push([
    'listener', event.message, event.defaultPrevented, event.target === worker,
    event.currentTarget === worker, event.eventPhase,
  ]);
});
globalThis.channel = new MessageChannel();
channel.port2.onmessage = event => portEvents.push(event.data);
channel.port2.start();
worker.postMessage({ kind: 'connect' }, [channel.port1]);
</script><main>dedicated worker MessagePort callback errors</main>"#;
    let worker_script = r#"const reports = [];
const callbackTrace = [];
let messageReportStart = 0;
let messageTraceStart = 0;
onerror = (message, filename, line, column, error) => {
  const cancel = message === 'handled port error';
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
onmessage = event => {
  const port = event.ports[0];
  port.onmessage = message => {
    const kind = message.data.kind;
    messageReportStart = reports.length;
    messageTraceStart = callbackTrace.length;
    callbackTrace.push('port:onmessage:' + kind);
    if (kind === 'handled') throw new Error('handled port error');
  };
  port.addEventListener('message', message => {
    const kind = message.data.kind;
    callbackTrace.push('port:listener-one:' + kind);
    if (kind === 'unhandled') throw 'unhandled port error';
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
};"#;
    let server = tokio::spawn(async move {
        for (path, content_type, body) in [
            ("/dedicated-port-error-page", "text/html", page),
            (
                "/dedicated-port-errors.js",
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
            .with_initial_url(format!("http://{address}/dedicated-port-error-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("({ portEvents, ownerEvents })")
            .await
            .unwrap(),
        serde_json::json!({
            "portEvents": [{ "kind": "ready" }],
            "ownerEvents": [],
        })
    );

    for (kind, trace, reports) in [
        (
            "handled",
            vec![
                "port:onmessage:handled",
                "global:onerror:handled port error",
                "global:error:handled port error",
                "port:listener-one:handled",
                "port:listener-two:handled",
            ],
            serde_json::json!([
                { "phase": "onerror", "message": "handled port error", "cancel": true, "errorIsError": true },
                {
                    "phase": "error-listener", "message": "handled port error",
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
                "global:onerror:unhandled port error",
                "global:error:unhandled port error",
                "port:listener-two:unhandled",
            ],
            serde_json::json!([
                { "phase": "onerror", "message": "unhandled port error", "cancel": false, "errorIsError": false },
                {
                    "phase": "error-listener", "message": "unhandled port error",
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
            "page-to-worker port send for {kind}",
        );
        let page_state = engine
            .evaluate_async("({ portEvents, ownerEvents })")
            .await
            .unwrap();
        let port_events = page_state["portEvents"].as_array().unwrap();
        assert_eq!(
            port_events.last(),
            Some(&serde_json::json!({
                "kind": "reply", "request": kind, "trace": trace, "reports": reports,
            })),
            "reply after dedicated-worker MessagePort callback {kind}",
        );
        let expected_owner_events = if kind == "handled" {
            serde_json::json!([])
        } else {
            serde_json::json!([
                [
                    "handler",
                    "unhandled port error",
                    true,
                    true,
                    true,
                    false,
                    true,
                    true,
                    2
                ],
                ["listener", "unhandled port error", false, true, true, 2],
            ])
        };
        assert_eq!(
            page_state["ownerEvents"], expected_owner_events,
            "handled errors stay local, while one unhandled error remains recorded in handler/listener order",
        );
    }
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_module_worker_graph_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests: Vec<(String, Option<String>)> = Vec::new();
        for _ in 0..7 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "module Worker graph request idle timeout after paths {:?}",
                        requests.iter().map(|(path, _)| path).collect::<Vec<_>>()
                    )
                })
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("module Worker graph request includes a URL")
                .to_owned();
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (response_headers, content_type, body): (&str, &str, &str) = match path.as_str() {
                "/module-worker-page" => (
                    "Set-Cookie: module_page_secret=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    "<script>globalThis.moduleWorkerMessages = []; globalThis.moduleWorker = new Worker('/module-worker-entry.mjs', { type: 'module' }); moduleWorker.onmessage = event => moduleWorkerMessages.push(event.data);</script>",
                ),
                "/module-worker-entry.mjs" => (
                    "Set-Cookie: module_entry_secret=entry; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "import { value } from './module-worker-dependency.mjs'; self.onmessage = async event => { if (event.data !== 'after') return; const runtime = await import('./module-worker-runtime.mjs'); const dependency = await runtime.load(); const response = await fetch('/module-worker-after'); postMessage({ kind: 'after', runtime: dependency.value, text: await response.text() }); }; fetch('/module-worker-data').then(async response => postMessage({ kind: 'startup', module: value, text: await response.text() })).catch(error => postMessage({ kind: 'error', message: String(error) }));",
                ),
                "/module-worker-dependency.mjs" => (
                    "Set-Cookie: module_dependency_secret=dependency; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "export const value = 'module dependency loaded';",
                ),
                "/module-worker-data" => (
                    "Set-Cookie: module_data_secret=data; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "module worker startup fetch",
                ),
                "/module-worker-runtime.mjs" => (
                    "Set-Cookie: module_runtime_secret=runtime; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "export function load() { return import('./module-worker-runtime-dependency.mjs'); }",
                ),
                "/module-worker-runtime-dependency.mjs" => (
                    "Set-Cookie: module_runtime_dependency_secret=runtime-dependency; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "export const value = 'runtime module dependency loaded';",
                ),
                "/module-worker-after" => ("", "text/plain", "module worker follow-up fetch"),
                other => panic!("unexpected module Worker graph request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{response_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie));
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/module-worker-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut messages = serde_json::Value::Null;
    for _ in 0..8 {
        messages = engine.evaluate_async("moduleWorkerMessages").await.unwrap();
        if messages.as_array().is_some_and(|messages| {
            messages
                .iter()
                .any(|message| message["kind"] == "startup" || message["kind"] == "error")
        }) {
            break;
        }
    }
    assert_eq!(
        messages,
        serde_json::json!([{
            "kind": "startup",
            "module": "module dependency loaded",
            "text": "module worker startup fetch",
        }])
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );

    engine
        .evaluate_async("moduleWorker.postMessage('after')")
        .await
        .unwrap();
    for _ in 0..8 {
        messages = engine.evaluate_async("moduleWorkerMessages").await.unwrap();
        if messages.as_array().is_some_and(|messages| {
            messages
                .iter()
                .any(|message| message["kind"] == "after" || message["kind"] == "error")
        }) {
            break;
        }
    }
    assert_eq!(
        messages,
        serde_json::json!([
            {
                "kind": "startup",
                "module": "module dependency loaded",
                "text": "module worker startup fetch",
            },
            {
                "kind": "after",
                "runtime": "runtime module dependency loaded",
                "text": "module worker follow-up fetch",
            },
        ])
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    for (name, value) in [
        ("module_page_secret", "seed"),
        ("module_entry_secret", "entry"),
        ("module_dependency_secret", "dependency"),
        ("module_data_secret", "data"),
        ("module_runtime_secret", "runtime"),
        ("module_runtime_dependency_secret", "runtime-dependency"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == name && cookie.value == value && cookie.http_only }),
            "parent cookie jar is missing {name}"
        );
    }
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .unwrap_or_else(|| panic!("missing module Worker request {path}"))
            .1
            .as_deref()
            .unwrap_or("")
            .to_owned()
    };
    assert!(cookie_for("/module-worker-entry.mjs").contains("module_page_secret=seed"));
    for name in ["module_page_secret=seed", "module_entry_secret=entry"] {
        assert!(
            cookie_for("/module-worker-dependency.mjs").contains(name),
            "parent did not apply the module entry response cookie before loading its dependency"
        );
    }
    for name in [
        "module_page_secret=seed",
        "module_entry_secret=entry",
        "module_dependency_secret=dependency",
    ] {
        assert!(
            cookie_for("/module-worker-data").contains(name),
            "parent did not select {name} for the module Worker startup Fetch"
        );
    }
    for name in [
        "module_page_secret=seed",
        "module_entry_secret=entry",
        "module_dependency_secret=dependency",
        "module_data_secret=data",
        "module_runtime_secret=runtime",
        "module_runtime_dependency_secret=runtime-dependency",
    ] {
        assert!(
            cookie_for("/module-worker-after").contains(name),
            "parent did not select {name} for the follow-up Worker Fetch"
        );
    }
    for name in [
        "module_page_secret=seed",
        "module_entry_secret=entry",
        "module_dependency_secret=dependency",
        "module_data_secret=data",
    ] {
        assert!(
            cookie_for("/module-worker-runtime.mjs").contains(name),
            "parent did not select {name} for the runtime module root"
        );
    }
    for name in [
        "module_page_secret=seed",
        "module_entry_secret=entry",
        "module_dependency_secret=dependency",
        "module_data_secret=data",
        "module_runtime_secret=runtime",
    ] {
        assert!(
            cookie_for("/module-worker-runtime-dependency.mjs").contains(name),
            "parent did not select {name} for the runtime module dependency"
        );
    }
}
#[tokio::test]
async fn native_content_process_uses_script_csp_for_worker_import_scripts() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut paths = Vec::new();
        for _ in 0..5 {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(Duration::from_secs(10), listener.accept()).await
            else {
                break;
            };
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            paths.push(path.clone());
            let (content_type, policy_header, body) = match path.as_str() {
                "/worker-csp-destinations" => (
                    "text/html",
                    "Content-Security-Policy: script-src 'unsafe-inline'; worker-src 'self'\r\n",
                    "<script>globalThis.workerErrors=[]; globalThis.workerMessages=[]; const denied=new Worker('/worker-csp-denied-root.js'); denied.onmessage=e=>workerMessages.push(['denied',e.data]); denied.onerror=e=>{e.preventDefault();workerErrors.push(String(e.message||''));}; const reportOnly=new Worker('/worker-csp-report-root.js'); reportOnly.onmessage=e=>workerMessages.push(['report',e.data]); reportOnly.onerror=e=>workerErrors.push('report-worker-error');</script>",
                ),
                "/worker-csp-denied-root.js" => (
                    "text/javascript",
                    "Content-Security-Policy: script-src 'none'; worker-src 'self'\r\n",
                    "importScripts('/worker-csp-denied-dependency.js'); postMessage('denied-root-ran');",
                ),
                "/worker-csp-report-root.js" => (
                    "text/javascript",
                    "Content-Security-Policy-Report-Only: script-src 'none'; worker-src 'self'\r\n",
                    "importScripts('/worker-csp-report-dependency.js'); postMessage('report-only-root-ran');",
                ),
                "/worker-csp-denied-dependency.js" => (
                    "text/javascript",
                    "",
                    "globalThis.deniedDependencyLoaded=true;",
                ),
                "/worker-csp-report-dependency.js" => (
                    "text/javascript",
                    "",
                    "globalThis.reportDependencyLoaded=true;",
                ),
                _ => ("text/plain", "", "unexpected resource"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{policy_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            if path == "/worker-csp-report-dependency.js" {
                break;
            }
        }
        paths
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-csp-destinations")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let mut observations = serde_json::Value::Null;
    for _ in 0..50 {
        observations = engine
            .evaluate_async("[workerErrors, workerMessages]")
            .await
            .unwrap();
        if observations[1].as_array().is_some_and(|messages| {
            messages.contains(&serde_json::json!(["report", "report-only-root-ran"]))
        }) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    engine.close_async().await.unwrap();
    let paths = server.await.unwrap();
    assert_eq!(
        observations[1],
        serde_json::json!([["report", "report-only-root-ran"]]),
        "worker observations: {observations:?}; HTTP paths: {paths:?}"
    );
    assert_eq!(observations[0].as_array().map(Vec::len), Some(1));
    assert!(paths.contains(&"/worker-csp-denied-root.js".to_owned()));
    assert!(paths.contains(&"/worker-csp-report-dependency.js".to_owned()));
    assert!(!paths.contains(&"/worker-csp-denied-dependency.js".to_owned()));
}
#[tokio::test]
async fn native_content_process_resolves_runtime_worker_module_imports() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut requests = Vec::new();
            for _ in 0..6 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("worker module request includes a URL")
                    .to_owned();
                let body = match path.as_str() {
                    "/page" => "<script>globalThis.workerMessages = []; globalThis.workerErrors = []; const listen = worker => { worker.onmessage = event => workerMessages.push(event.data); worker.onerror = event => workerErrors.push(event.message); }; globalThis.moduleWorker = new Worker('/module-worker.js', { type: 'module' }); listen(moduleWorker); globalThis.classicWorker = new Worker('/classic-worker.js'); listen(classicWorker);</script>",
                    "/module-worker.js" => "const target = './module-dep.js?request=runtime'; import(target).then(async module => postMessage({ kind: 'module', value: module.value, nested: await module.loadNested() }), error => postMessage({ kind: 'module-error', message: String(error) }));",
                    "/module-dep.js?request=runtime" => "export const value = 'module-worker'; export async function loadNested() { const nestedTarget = './module-nested.js?query=nested'; return (await import(nestedTarget)).value; }",
                    "/module-nested.js?query=nested" => "export const value = 'nested-worker-module';",
                    "/classic-worker.js" => "const target = './classic-dep.js'; import(target).then(module => postMessage({ kind: 'classic', value: module.value }), error => postMessage({ kind: 'classic-error', message: String(error) }));",
                    "/classic-dep.js" => "export const value = 'classic-worker';",
                    other => panic!("unexpected worker dynamic-module request: {other}"),
                };
                let content_type = if path == "/page" {
                    "text/html"
                } else {
                    "application/javascript"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push(path);
            }
            requests.sort();
            requests
        })
        .await
        .expect("runtime worker module requests stay within their time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async("({ messages: workerMessages.slice().sort((a, b) => String(a.kind).localeCompare(String(b.kind))), errors: workerErrors })")
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        result,
        serde_json::json!({
            "messages": [
                {"kind": "classic", "value": "classic-worker"},
                {"kind": "module", "value": "module-worker", "nested": "nested-worker-module"},
            ],
            "errors": [],
        }),
        "computed dynamic imports in dedicated classic/module workers and a nested worker module must evaluate without worker errors"
    );
    assert_eq!(
        requests,
        vec![
            "/classic-dep.js",
            "/classic-worker.js",
            "/module-dep.js?request=runtime",
            "/module-nested.js?query=nested",
            "/module-worker.js",
            "/page",
        ]
    );
}
#[tokio::test]
async fn native_content_process_inherits_referrer_policy_through_module_workers() {
    fn request_header(request: &str, name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let document_url = format!("http://{address}/page?secret=token");
    let origin = format!("http://{address}/");
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            let mut observed = BTreeMap::new();
            for _ in 0..11 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("module Worker request includes a URL")
                    .to_owned();
                let referer = request_header(&request, "referer");
                let (status, extra_headers, body) = match path.as_str() {
                    "/page?secret=token" => (
                        "200 OK",
                        "Content-Type: text/html\r\nReferrer-Policy: unsafe-url\r\n",
                        "<meta name='referrer' content='unsafe-url'><script>document.querySelector('meta').content='no-referrer';globalThis.workerResults=[];globalThis.sharedResults=[];const worker=new Worker('/dedicated-entry.js',{type:'module'});worker.onmessage=event=>workerResults.push(event.data);const shared=new SharedWorker('/shared-entry.js',{type:'module',name:'slice-833'});shared.port.onmessage=event=>sharedResults.push(event.data);shared.port.start();</script>",
                    ),
                    "/dedicated-entry.js" => (
                        "302 Found",
                        "Location: /dedicated/root.js?final=1\r\nReferrer-Policy: origin\r\n",
                        "",
                    ),
                    "/shared-entry.js" => (
                        "302 Found",
                        "Location: /shared/root.js?final=1\r\nReferrer-Policy: origin\r\n",
                        "",
                    ),
                    "/dedicated/root.js?final=1" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\nReferrer-Policy: origin\r\n",
                        "import { value as staticValue } from './static.js'; const dynamic = await import('./dynamic.js'); postMessage({ staticValue, dynamicValue: dynamic.value, nestedValue: await dynamic.nested });",
                    ),
                    "/shared/root.js?final=1" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\nReferrer-Policy: origin\r\n",
                        "import { value as staticValue } from './static.js'; const dynamic = await import('./dynamic.js'); const result = { staticValue, dynamicValue: dynamic.value, nestedValue: await dynamic.nested }; self.onconnect = event => event.ports[0].postMessage(result);",
                    ),
                    "/dedicated/static.js" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\n",
                        "export const value = 'dedicated-static';",
                    ),
                    "/shared/static.js" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\n",
                        "export const value = 'shared-static';",
                    ),
                    "/dedicated/dynamic.js" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\nReferrer-Policy: no-referrer\r\n",
                        "export const value = 'dedicated-dynamic'; export const nested = import('./nested.js').then(module => module.value);",
                    ),
                    "/shared/dynamic.js" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\nReferrer-Policy: no-referrer\r\n",
                        "export const value = 'shared-dynamic'; export const nested = import('./nested.js').then(module => module.value);",
                    ),
                    "/dedicated/nested.js" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\n",
                        "export const value = 'dedicated-nested';",
                    ),
                    "/shared/nested.js" => (
                        "200 OK",
                        "Content-Type: application/javascript\r\n",
                        "export const value = 'shared-nested';",
                    ),
                    other => panic!("unexpected module Worker request: {other}"),
                };
                let content_type = if extra_headers.contains("Content-Type") {
                    ""
                } else {
                    "Content-Type: application/javascript\r\n"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\n{content_type}{extra_headers}Cache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                observed.insert(path, referer);
            }
            observed
        })
        .await
        .expect("module Worker graph requests stay within their time bound")
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(document_url)).unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async("({ dedicated: workerResults, shared: sharedResults })")
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let observed = server.await.unwrap();

    assert_eq!(
        result,
        serde_json::json!({
            "dedicated": [{
                "staticValue": "dedicated-static",
                "dynamicValue": "dedicated-dynamic",
                "nestedValue": "dedicated-nested",
            }],
            "shared": [{
                "staticValue": "shared-static",
                "dynamicValue": "shared-dynamic",
                "nestedValue": "shared-nested",
            }],
        })
    );
    for path in ["/dedicated-entry.js", "/shared-entry.js"] {
        assert_eq!(
            observed.get(path).and_then(Option::as_deref),
            None,
            "{path}"
        );
    }
    for path in [
        "/dedicated/root.js?final=1",
        "/shared/root.js?final=1",
        "/dedicated/static.js",
        "/shared/static.js",
        "/dedicated/dynamic.js",
        "/shared/dynamic.js",
    ] {
        assert_eq!(
            observed.get(path).and_then(Option::as_deref),
            Some(origin.as_str()),
            "{path}"
        );
    }
    for path in ["/dedicated/nested.js", "/shared/nested.js"] {
        assert_eq!(
            observed.get(path).and_then(Option::as_deref),
            None,
            "{path}"
        );
    }
}

use super::*;

#[tokio::test]
async fn native_content_process_service_worker_dynamic_imports_reject_without_fetch() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut request_paths = Vec::new();
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default().to_owned();
                    request_paths.push(path.clone());
                    let (content_type, body) = match path.as_str() {
                        "/classic/register" => (
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/classic/sw.js', { scope: '/classic/' });</script><main>classic registration</main>",
                        ),
                        "/classic/sw.js" => (
                            "application/javascript",
                            r#"importScripts('/classic/dep.js');
try {
  Object.defineProperty(globalThis, '__glassRejectDynamicImport', {
    value: () => Promise.resolve('overridden'), configurable: true,
  });
} catch {}
globalThis.__classicResults.rootPromise = import((
  globalThis.__classicResults.specifierEvaluations++, './root-dynamic.js'
), (globalThis.__classicResults.specifierEvaluations++, { with: { type: 'json' } })).then(
  () => { globalThis.__classicResults.root = 'fulfilled'; },
  error => { globalThis.__classicResults.root = error.name; }
);
self.addEventListener('install', event => event.waitUntil(Promise.all([
  globalThis.__classicResults.rootPromise,
  globalThis.__classicResults.depPromise,
]).then(() => self.skipWaiting())));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname !== '/classic/probe') return;
  event.respondWith(Promise.all([
    globalThis.__classicResults.rootPromise,
    globalThis.__classicResults.depPromise,
  ]).then(() => new Response(JSON.stringify({
    root: globalThis.__classicResults.root,
    dependency: globalThis.__classicResults.dependency,
    static: globalThis.__classicResults.static,
    specifierEvaluations: globalThis.__classicResults.specifierEvaluations,
  }), { headers: { 'Content-Type': 'application/json' } })));
});"#,
                        ),
                        "/classic/dep.js" => (
                            "application/javascript",
                            r#"globalThis.__classicResults = { static: 'importScripts dependency', specifierEvaluations: 0 };
globalThis.__classicResults.depPromise = import((
  globalThis.__classicResults.specifierEvaluations++, './dependency-dynamic.js'
), (globalThis.__classicResults.specifierEvaluations++, { with: { type: 'json' } })).then(
  () => { globalThis.__classicResults.dependency = 'fulfilled'; },
  error => { globalThis.__classicResults.dependency = error.name; }
);"#,
                        ),
                        "/module/register" => (
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/module/sw.js', { scope: '/module/', type: 'module' });</script><main>module registration</main>",
                        ),
                        "/module/sw.js" => (
                            "application/javascript",
                            r#"import { staticValue } from './dep.js';
try {
  Object.defineProperty(globalThis, '__glassRejectDynamicImport', {
    value: () => Promise.resolve('overridden'), configurable: true,
  });
} catch {}
globalThis.__moduleResults.rootPromise = import((
  globalThis.__moduleResults.specifierEvaluations++, './dep.js'
), (globalThis.__moduleResults.specifierEvaluations++, { with: { type: 'json' } })).then(
  () => { globalThis.__moduleResults.root = 'fulfilled'; },
  error => { globalThis.__moduleResults.root = error.name; }
);
globalThis.__moduleResults.rootUncachedPromise = import((
  globalThis.__moduleResults.specifierEvaluations++, './root-dynamic.js'
), (globalThis.__moduleResults.specifierEvaluations++, { with: { type: 'json' } })).then(
  () => { globalThis.__moduleResults.rootUncached = 'fulfilled'; },
  error => { globalThis.__moduleResults.rootUncached = error.name; }
);
self.addEventListener('install', event => event.waitUntil(Promise.all([
  globalThis.__moduleResults.rootPromise,
  globalThis.__moduleResults.rootUncachedPromise,
  globalThis.__moduleResults.dependencyPromise,
]).then(() => self.skipWaiting())));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname !== '/module/probe') return;
  event.respondWith(Promise.all([
    globalThis.__moduleResults.rootPromise,
    globalThis.__moduleResults.rootUncachedPromise,
    globalThis.__moduleResults.dependencyPromise,
  ]).then(() => new Response(JSON.stringify({
    root: globalThis.__moduleResults.root,
    rootUncached: globalThis.__moduleResults.rootUncached,
    dependency: globalThis.__moduleResults.dependency,
    static: staticValue,
    specifierEvaluations: globalThis.__moduleResults.specifierEvaluations,
  }), { headers: { 'Content-Type': 'application/json' } })));
});"#,
                        ),
                        "/module/dep.js" => (
                            "application/javascript",
                            r#"globalThis.__moduleResults = globalThis.__moduleResults || { specifierEvaluations: 0 };
globalThis.__moduleResults.dependencyPromise = import((
  globalThis.__moduleResults.specifierEvaluations++, './dependency-dynamic.js'
), (globalThis.__moduleResults.specifierEvaluations++, { with: { type: 'json' } })).then(
  () => { globalThis.__moduleResults.dependency = 'fulfilled'; },
  error => { globalThis.__moduleResults.dependency = error.name; }
);
export const staticValue = 'module static dependency';"#,
                        ),
                        _ => (
                            "application/javascript",
                            "export const unexpectedDynamicFetch = true;",
                        ),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
        request_paths
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/classic/register")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(registration => registration.active.state)"
            )
            .await
            .unwrap(),
        serde_json::json!("activated")
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/classic/probe').then(response => response.json())")
            .await
            .unwrap(),
        serde_json::json!({
            "root": "TypeError",
            "dependency": "TypeError",
            "static": "importScripts dependency",
            "specifierEvaluations": 4,
        })
    );

    engine
        .navigate_async(format!("http://{address}/module/register"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(registration => registration.active.state)"
            )
            .await
            .unwrap(),
        serde_json::json!("activated")
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/module/probe').then(response => response.json())")
            .await
            .unwrap(),
        serde_json::json!({
            "root": "TypeError",
            "rootUncached": "TypeError",
            "dependency": "TypeError",
            "static": "module static dependency",
            "specifierEvaluations": 6,
        })
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    engine.close_async().await.unwrap();
    let _ = shutdown_sender.send(());
    assert_eq!(
        server.await.unwrap(),
        [
            "/classic/register",
            "/classic/sw.js",
            "/classic/dep.js",
            "/module/register",
            "/module/sw.js",
            "/module/dep.js",
        ]
    );
}
#[tokio::test]
async fn native_content_process_propagates_service_worker_document_csp_to_controlled_fetch() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let blocked_request = Arc::new(AtomicBool::new(false));
    let blocked_request_server = blocked_request.clone();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, headers, body) in [
            (
                "/register",
                "text/html",
                "Content-Security-Policy: script-src 'unsafe-inline'; worker-src 'self'\r\n",
                "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script>",
            ),
            (
                "/sw.js",
                "application/javascript",
                "",
                r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
    const path = new URL(event.request.url).pathname;
    if (path === '/controlled') {
        event.respondWith(new Response("<!doctype html><script>globalThis.fetchResultPromise = fetch('/blocked').then(response => response.text()).then(text => 'unexpected:' + text).catch(() => 'blocked');</script>", {
            headers: {
                'Content-Type': 'text/html',
                'Content-Security-Policy': "default-src 'self'; script-src 'unsafe-inline'; connect-src 'none'",
            },
        }));
    } else if (path === '/controlled-report') {
        event.respondWith(new Response("<!doctype html><script>globalThis.reportEvents = []; addEventListener('securitypolicyviolation', event => reportEvents.push([event.effectiveDirective, event.blockedURI, event.disposition])); globalThis.reportFetchPromise = fetch('/report-only-blocked').then(response => response.text());</script>", {
            headers: {
                'Content-Type': 'text/html',
                'Content-Security-Policy': "default-src 'self'; script-src 'unsafe-inline'; connect-src 'self'",
                'Content-Security-Policy-Report-Only': "connect-src 'none'",
            },
        }));
    } else if (path === '/blocked') {
        event.respondWith(new Response('service worker unexpectedly ran', { headers: { 'Content-Type': 'text/plain' } }));
    } else if (path === '/report-only-blocked') {
        event.respondWith(new Response('worker report response', { headers: { 'Content-Type': 'text/plain' } }));
    }
});"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        if let Ok((mut stream, _)) = listener.accept().await {
            let request = read_http_request(&mut stream).await;
            if request.split_whitespace().nth(1) == Some("/blocked") {
                blocked_request_server.store(true, Ordering::SeqCst);
            }
            let body = "unexpected network request";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/register")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async registration => [registration.active.state, navigator.serviceWorker.controller !== null])",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );

    engine
        .navigate_async(format!("http://{address}/controlled"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await fetchResultPromise")
            .await
            .unwrap(),
        serde_json::json!("blocked")
    );
    assert!(!blocked_request.load(Ordering::SeqCst));

    engine
        .navigate_async(format!("http://{address}/controlled-report"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await reportFetchPromise")
            .await
            .unwrap(),
        serde_json::json!("worker report response")
    );
    assert_eq!(
        engine.evaluate_async("reportEvents").await.unwrap(),
        serde_json::json!([[
            "connect-src",
            format!("http://{address}/report-only-blocked"),
            "report"
        ]])
    );
    assert!(!blocked_request.load(Ordering::SeqCst));

    engine.close_async().await.unwrap();
    server.abort();
    let _ = server.await;
}
#[tokio::test]
async fn native_content_process_service_worker_dispatches_large_fetch_request_as_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/register",
                "text/html",
                "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>large request</main>",
            ),
            (
                "/sw.js",
                "application/javascript",
                "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim())); self.addEventListener('fetch', event => { if (new URL(event.request.url).pathname === '/echo') event.respondWith(event.request.text().then(body => new Response(String(body.length), { headers: { 'Content-Type': 'text/plain' } }))); });",
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
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/register")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async registration => ({ state: registration.active.state, controlled: navigator.serviceWorker.controller !== null, echoed: await fetch('/echo', { method: 'POST', body: 'x'.repeat(20000) }).then(response => response.text()) }))",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "state": "activated",
            "controlled": true,
            "echoed": "20000",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_replays_cloned_request_body() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/register",
                "text/html",
                "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>replay</main>",
            ),
            (
                "/sw.js",
                "application/javascript",
                "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim())); self.addEventListener('fetch', event => { const path = new URL(event.request.url).pathname; if (path === '/echo') event.respondWith((async () => { const replay = event.request.clone(); const observed = await event.request.text(); const upstream = await fetch(replay); return new Response(JSON.stringify({ observed, upstream: await upstream.text(), replayUsed: replay.bodyUsed }), { headers: { 'Content-Type': 'application/json' } }); })()); if (path === '/stream-trigger') event.respondWith((async () => { const stream = new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array([110, 97, 116, 105, 118, 101, 45])); controller.enqueue(new Uint8Array([115, 119, 45, 115, 116, 114, 101, 97, 109])); controller.close(); } }); const upstream = await fetch(new Request('/stream-echo', { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: stream })); return new Response(await upstream.text(), { headers: { 'Content-Type': 'text/plain' } }); })()); if (path === '/controlled-stream') event.respondWith((async () => new Response(await event.request.text(), { headers: { 'Content-Type': 'text/plain' } }))()); });",
            ),
            ("/echo", "text/plain", "replayed"),
            ("/stream-echo", "text/plain", "native-sw-stream"),
            ("/unhandled-stream", "text/plain", "parent-fallback"),
            ("/cookie-check", "text/plain", "cookie-confirmed"),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_bytes(&mut stream).await;
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .unwrap();
            let request_path = request[..header_end]
                .split(|byte| *byte == b' ')
                .nth(1)
                .unwrap();
            assert_eq!(request_path, expected_path.as_bytes());
            if expected_path == "/echo" {
                assert_eq!(
                    request[..header_end].split(|byte| *byte == b' ').next(),
                    Some(&b"POST"[..])
                );
                assert!(
                    String::from_utf8_lossy(&request[..header_end])
                        .lines()
                        .any(|line| {
                            line.split_once(':').is_some_and(|(name, value)| {
                                name.eq_ignore_ascii_case("content-type")
                                    && value.trim() == "text/plain"
                            })
                        })
                );
                assert_eq!(&request[header_end..], b"native-replay");
            } else if expected_path == "/stream-echo" {
                assert_eq!(
                    request[..header_end].split(|byte| *byte == b' ').next(),
                    Some(&b"POST"[..])
                );
                assert!(
                    String::from_utf8_lossy(&request[..header_end])
                        .lines()
                        .any(|line| line.to_ascii_lowercase().starts_with("content-length: 16"))
                );
                assert_eq!(&request[header_end..], b"native-sw-stream");
            } else if expected_path == "/unhandled-stream" {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                assert!(headers.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("cookie")
                            && value.contains("sw_upload_turn=from-page")
                    })
                }));
                assert_eq!(&request[header_end..], b"page-unhandled-stream");
            } else if expected_path == "/cookie-check" {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                assert!(headers.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("cookie") && value.contains("sw_parent=accepted")
                    })
                }));
            }
            let set_cookie = if expected_path == "/unhandled-stream" {
                "Set-Cookie: sw_parent=accepted; HttpOnly; Path=/\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/register")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    // Keep each network scenario in its own bounded content-process turn while
    // preserving the same live worker, registration, and page cache state.
    let replay = engine
        .evaluate_async(
            "await registrationPromise.then(() => fetch('/echo', { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: 'native-replay' }).then(response => response.json()))",
        )
        .await
        .unwrap();
    assert_eq!(
        replay,
        serde_json::json!({
            "observed": "native-replay",
            "upstream": "replayed",
            "replayUsed": true,
        })
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/stream-trigger').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("native-sw-stream")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/controlled-stream', { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array([112, 97, 103, 101, 45])); controller.enqueue(new Uint8Array([115, 116, 114, 101, 97, 109])); controller.close(); } }) }).then(response => response.text())",
            )
            .await
            .unwrap(),
        serde_json::json!("page-stream")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.cookie = 'sw_upload_turn=from-page; Path=/'; await fetch('/unhandled-stream', { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array([112, 97, 103, 101, 45, 117, 110, 104, 97, 110, 100, 108, 101, 100, 45, 115, 116, 114, 101, 97, 109])); controller.close(); } }) }).then(response => response.text())",
            )
            .await
            .unwrap(),
        serde_json::json!("parent-fallback")
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/cookie-check').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("cookie-confirmed")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

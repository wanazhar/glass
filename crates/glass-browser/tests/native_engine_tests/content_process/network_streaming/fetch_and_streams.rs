use super::*;

#[tokio::test]
async fn native_content_process_does_not_intercept_uncontrolled_client_fetch() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/page",
                "text/html",
                "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>uncontrolled page</main></body></html>",
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => event.waitUntil(Promise.resolve()));
self.addEventListener('activate', event => event.waitUntil(Promise.resolve()));
self.addEventListener('fetch', event => {
  const path = new URL(event.request.url).pathname;
  event.respondWith(path === '/controlled'
    ? new Response('<!doctype html><html><body>controlled navigation</body></html>', { headers: { 'Content-Type': 'text/html' } })
    : new Response('service worker response'));
});"#,
            ),
            ("/data", "text/plain", "network response"),
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
                "await registrationPromise.then(async () => ({ controlled: navigator.serviceWorker.controller !== null, fetch: await fetch('/data').then(async response => ({ status: response.status, body: await response.text() })) }))",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "controlled": false,
            "fetch": {"status": 200, "body": "network response"},
        })
    );

    engine
        .navigate_async(format!("http://{address}/controlled"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ body: document.body.innerText, controlled: navigator.serviceWorker.controller !== null, fetch: await fetch('/data').then(response => response.text()) })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "body": "controlled navigation",
            "controlled": true,
            "fetch": "service worker response",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_idle_timer_stylesheet_events_broker_callback_fetches() {
    std::thread::Builder::new()
        .name("native-timer-stylesheet-events-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(4 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("timer stylesheet event test runtime should build");
            runtime.block_on(
                native_content_process_idle_timer_stylesheet_events_broker_callback_fetches_inner(),
            );
        })
        .expect("timer stylesheet event test thread should start")
        .join()
        .expect("timer stylesheet event test thread should complete");
}
#[test]
fn native_content_process_idle_timer_image_media_events_broker_callback_fetches() {
    std::thread::Builder::new()
        .name("native-timer-image-media-events-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(4 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("timer image/media event test runtime should build");
            runtime.block_on(
                native_content_process_idle_timer_image_media_events_broker_callback_fetches_inner(
                ),
            );
        })
        .expect("timer image/media event test thread should start")
        .join()
        .expect("timer image/media event test thread should complete");
}
#[tokio::test]
async fn native_content_process_resolves_dynamic_script_fetch() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (content_type, body) = match path {
                "/page" => (
                    "text/html",
                    "<html><body><p>Native content</p></body></html>",
                ),
                "/parent.js" => (
                    "application/javascript",
                    "fetch('/data').then(response => response.text()).then(value => document.body.setAttribute('data-dynamic-fetch', value));",
                ),
                "/data" => ("text/plain", "dynamic-fetch-done"),
                other => panic!("unexpected request path: {other}"),
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
                "globalThis.dynamicFetchScript = document.createElement('script'); dynamicFetchScript.src = '/parent.js'; document.body.appendChild(dynamicFetchScript); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("document.body.getAttribute('data-dynamic-fetch')")
            .await
            .unwrap(),
        serde_json::json!("dynamic-fetch-done")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_delivers_report_only_connect_events_before_fetch_resolution() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<script>globalThis.cspReports = []; addEventListener('securitypolicyviolation', event => cspReports.push([event.effectiveDirective, event.blockedURI, event.disposition]));</script><body>Fetch report</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy-Report-Only: connect-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/connect"));
        let body = "fetch-ok";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                "await fetch('/connect').then(async response => [await response.text(), cspReports])",
            )
            .await
            .unwrap(),
        serde_json::json!(["fetch-ok", [["connect-src", format!("http://{address}/connect"), "report"]]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_prefetches_static_module_graphs() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/app.js", "/dep.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = match expected_path {
                "/page" => ("text/html", "<script type='module' src='/app.js'></script>"),
                "/app.js" => (
                    "application/javascript",
                    "import { suffix } from './dep.js'; globalThis.moduleOrder = 'app-' + suffix; export const ready = true;",
                ),
                _ => ("application/javascript", "export const suffix = 'dep';"),
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
            .evaluate_async("globalThis.moduleOrder")
            .await
            .unwrap(),
        serde_json::json!("app-dep")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_script_fetch_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..3 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("parent-owned script Fetch should reach the local server")
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
                        "Set-Cookie: script_session=initial; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: script_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<p>page</p>",
                ),
                "/set" => (
                    concat!(
                        "Set-Cookie: script_session=latest; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: script_secret=latest-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/plain",
                    "updated",
                ),
                "/inspect" => ("", "text/plain", "inspected"),
                other => panic!("unexpected parent-owned script Fetch path: {other}"),
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
    let updated = engine
        .evaluate_async(
            "document.cookie = 'same_turn=present; Path=/'; await fetch('/set').then(response => response.text())",
        )
        .await
        .unwrap();
    assert_eq!(updated, serde_json::json!("updated"));
    let visible_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    assert!(
        visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("script_session=latest"))
    );
    assert!(
        visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("same_turn=present"))
    );
    engine
        .evaluate_async("document.cookie = 'standalone=owned; Path=/'")
        .await
        .unwrap();
    let parent_cookies = engine.cookies_async().await.unwrap();
    assert!(
        parent_cookies
            .iter()
            .any(|cookie| cookie.name == "standalone" && cookie.value == "owned")
    );
    let inspected = engine
        .evaluate_async("await fetch('/inspect').then(response => response.text())")
        .await
        .unwrap();
    assert_eq!(inspected, serde_json::json!("inspected"));
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/page", "/set", "/inspect"]
    );
    let update_cookie = requests[1].1.as_deref().unwrap_or_default();
    assert!(
        update_cookie.contains("script_session=initial"),
        "the first brokered request did not carry the parent cookie: {requests:?}"
    );
    assert!(update_cookie.contains("script_secret=initial-secret"));
    assert!(update_cookie.contains("same_turn=present"));
    let inspect_cookie = requests[2].1.as_deref().unwrap_or_default();
    assert!(inspect_cookie.contains("script_session=latest"));
    assert!(inspect_cookie.contains("script_secret=latest-secret"));
    assert!(inspect_cookie.contains("same_turn=present"));
    assert!(inspect_cookie.contains("standalone=owned"));
}
#[tokio::test]
async fn native_content_process_inherits_document_referrer_policy_for_page_fetch() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let target_origin = format!("http://{target_address}");

    let (stop_page_server, mut stop_page_server_rx) = tokio::sync::oneshot::channel();
    let page_server = tokio::spawn(async move {
        let mut seen = Vec::new();
        let mut reuse_responses = 0;
        loop {
            let (mut stream, _) = tokio::select! {
                _ = &mut stop_page_server_rx => break,
                accepted = page_listener.accept() => accepted.unwrap(),
            };
            let request = read_http_request(&mut stream).await;
            let target = request.split_whitespace().nth(1).unwrap_or_default();
            let path = target.split('?').next().unwrap_or_default();
            let (referrer_policy, body) = match path {
                "/policy" => (
                    "Referrer-Policy: origin, no-referrer, invalid\r\n",
                    "policy".to_owned(),
                ),
                "/invalid" => (
                    "Referrer-Policy: invalid, unknown\r\n",
                    "invalid".to_owned(),
                ),
                "/meta" => (
                    "Referrer-Policy: no-referrer\r\n",
                    "<!doctype html><html><head><meta name=\"REFERRER\" content=\"origin-when-crossorigin\"></head><body></body></html>".to_owned(),
                ),
                "/reuse" => {
                    reuse_responses += 1;
                    let policy = if reuse_responses == 1 {
                        "Referrer-Policy: no-referrer\r\n"
                    } else {
                        ""
                    };
                    (policy, format!("reuse-{reuse_responses}"))
                }
                _ => ("", "page".to_owned()),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: no-store\r\n{referrer_policy}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            seen.push(path.to_owned());
        }
        seen
    });

    let (stop_target_server, mut stop_target_server_rx) = tokio::sync::oneshot::channel();
    let cors_page_origin = page_origin.clone();
    let target_server = tokio::spawn(async move {
        let mut seen = Vec::new();
        loop {
            let (mut stream, _) = tokio::select! {
                _ = &mut stop_target_server_rx => break,
                accepted = target_listener.accept() => accepted.unwrap(),
            };
            let request = read_http_request(&mut stream).await;
            let target = request
                .split_whitespace()
                .nth(1)
                .expect("page Fetch request includes a target")
                .to_owned();
            let referer = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let case = target
                .split_once("case=")
                .map(|(_, value)| value)
                .unwrap_or_default()
                .to_owned();
            let cors = format!("Access-Control-Allow-Origin: {cors_page_origin}\r\n");
            let response = format!(
                "HTTP/1.1 200 OK\r\n{cors}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{case}",
                case.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            seen.push((target, referer));
        }
        seen
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("{page_origin}/policy?private=1#fragment")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let inherited_and_override = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const request = new Request('{target_origin}/record?case=inherited');
              const inherited = await fetch(request).then(response => response.text());
              const overridden = await fetch('{target_origin}/record?case=override', {{
                referrerPolicy: 'unsafe-url'
              }}).then(response => response.text());
              return [request.referrerPolicy, inherited, overridden];
            }})()"#
        ))
        .await
        .unwrap();

    engine
        .navigate_async(format!("{page_origin}/invalid?private=2#fragment"))
        .await
        .unwrap();
    let invalid_header = engine
        .evaluate_async(&format!(
            "await fetch('{target_origin}/record?case=invalid-header').then(response => response.text())"
        ))
        .await
        .unwrap();

    let reuse_url = format!("{page_origin}/reuse?private=3#fragment");
    engine.navigate_async(reuse_url.clone()).await.unwrap();
    let reuse_with_policy = engine
        .evaluate_async(&format!(
            "await fetch('{target_origin}/record?case=reuse-before').then(response => response.text())"
        ))
        .await
        .unwrap();
    engine.navigate_async(reuse_url).await.unwrap();
    let reuse_without_policy = engine
        .evaluate_async(&format!(
            "await fetch('{target_origin}/record?case=reuse-after').then(response => response.text())"
        ))
        .await
        .unwrap();

    let meta_page_url = format!("{page_origin}/meta?private=4");
    engine
        .navigate_async(format!("{meta_page_url}#fragment"))
        .await
        .unwrap();
    let meta_initial = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const request = new Request('{target_origin}/record?case=meta-parsed');
              const parsed = await fetch(request).then(response => response.text());
              return [request.referrerPolicy, parsed];
            }})()"#
        ))
        .await
        .unwrap();
    let meta_insertion_and_content = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const container = document.createElement('div');
              const dynamic = document.createElement('meta');
              dynamic.setAttribute('name', 'referrer');
              dynamic.setAttribute('content', 'always');
              container.appendChild(dynamic);
              document.head.appendChild(container);
              const inserted = await fetch('{target_origin}/record?case=meta-inserted').then(response => response.text());
              const currentDynamic = document.querySelectorAll('meta')[1];
              currentDynamic.setAttribute('content', 'No-Referrer');
              const contentChanged = await fetch('{target_origin}/record?case=meta-content').then(response => response.text());
              return [inserted, contentChanged];
            }})()"#
        ))
        .await
        .unwrap();
    let meta_name_ignored = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const dynamic = document.querySelectorAll('meta')[1];
              dynamic.setAttribute('name', 'other');
              dynamic.setAttribute('content', 'origin');
              const nameIgnored = await fetch('{target_origin}/record?case=meta-name-ignored').then(response => response.text());
              return nameIgnored;
            }})()"#
        ))
        .await
        .unwrap();
    let meta_name_changed = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const currentDynamic = document.querySelectorAll('meta')[1];
              currentDynamic.setAttribute('name', 'REFERRER');
              const nameChanged = await fetch('{target_origin}/record?case=meta-name').then(response => response.text());
              return [nameChanged,
                currentDynamic.getAttribute('name'), currentDynamic.getAttribute('content')];
            }})()"#
        ))
        .await
        .unwrap();
    let meta_empty_and_untrimmed = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const dynamic = document.querySelectorAll('meta')[1];
              dynamic.setAttribute('content', '');
              const emptyContent = await fetch('{target_origin}/record?case=meta-empty').then(response => response.text());
              const currentDynamic = document.querySelectorAll('meta')[1];
              currentDynamic.setAttribute('content', ' no-referrer');
              const untrimmedContent = await fetch('{target_origin}/record?case=meta-untrimmed').then(response => response.text());
              return [emptyContent, untrimmedContent];
            }})()"#
        ))
        .await
        .unwrap();
    let meta_tree_order_and_removal = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const earlier = document.createElement('meta');
              earlier.setAttribute('name', 'referrer');
              earlier.setAttribute('content', 'never');
              document.head.insertBefore(earlier, document.head.firstChild);
              const treeOrder = await fetch('{target_origin}/record?case=meta-tree-order').then(response => response.text());
              document.querySelectorAll('meta')[0].remove();
              const removed = await fetch('{target_origin}/record?case=meta-removed').then(response => response.text());
              return [treeOrder, removed];
            }})()"#
        ))
        .await
        .unwrap();
    let meta_invalid_and_default = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const dynamic = document.querySelectorAll('meta')[1];
              dynamic.setAttribute('content', 'not-a-policy');
              const invalid = await fetch('{target_origin}/record?case=meta-invalid').then(response => response.text());
              const currentDynamic = document.querySelectorAll('meta')[1];
              currentDynamic.setAttribute('content', 'default');
              const defaultAlias = await fetch('{target_origin}/record?case=meta-default').then(response => response.text());
              return [invalid, defaultAlias];
            }})()"#
        ))
        .await
        .unwrap();
    let meta_explicit_override = engine
        .evaluate_async(&format!(
            r#"await (async () => {{
              const overridden = await fetch('{target_origin}/record?case=meta-override', {{
                referrerPolicy: 'unsafe-url',
              }}).then(response => response.text());
              return overridden;
            }})()"#
        ))
        .await
        .unwrap();
    let live_meta_results = serde_json::Value::Array(vec![
        meta_initial,
        meta_insertion_and_content,
        meta_name_ignored,
        meta_name_changed,
        meta_empty_and_untrimmed,
        meta_tree_order_and_removal,
        meta_invalid_and_default,
        meta_explicit_override,
    ]);

    engine.close_async().await.unwrap();
    let _ = stop_page_server.send(());
    let _ = stop_target_server.send(());
    let page_requests = page_server.await.unwrap();
    let target_requests = target_server.await.unwrap();

    assert_eq!(
        inherited_and_override,
        serde_json::json!(["", "inherited", "override"])
    );
    assert_eq!(invalid_header.as_str(), Some("invalid-header"));
    assert_eq!(reuse_with_policy.as_str(), Some("reuse-before"));
    assert_eq!(reuse_without_policy.as_str(), Some("reuse-after"));
    assert_eq!(
        live_meta_results,
        serde_json::json!([
            ["", "meta-parsed"],
            ["meta-inserted", "meta-content"],
            "meta-name-ignored",
            ["meta-name", "REFERRER", "origin"],
            ["meta-empty", "meta-untrimmed"],
            ["meta-tree-order", "meta-removed"],
            ["meta-invalid", "meta-default"],
            "meta-override"
        ])
    );
    assert_eq!(
        page_requests,
        ["/policy", "/invalid", "/reuse", "/reuse", "/meta"]
    );

    let referer_for = |case: &str| {
        target_requests
            .iter()
            .find(|(target, _)| {
                target
                    .split_once("case=")
                    .is_some_and(|(_, request_case)| request_case == case)
            })
            .unwrap_or_else(|| panic!("missing page Fetch request {case}"))
            .1
            .clone()
    };
    assert_eq!(referer_for("inherited"), None);
    assert_eq!(
        referer_for("override"),
        Some(format!("{page_origin}/policy?private=1"))
    );
    for case in ["invalid-header", "reuse-after"] {
        assert_eq!(
            referer_for(case),
            Some(format!("{page_origin}/")),
            "default policy after {case}"
        );
    }
    assert_eq!(referer_for("reuse-before"), None);
    let meta_page_referrer = format!("{page_origin}/meta?private=4");
    for case in [
        "meta-parsed",
        "meta-name",
        "meta-empty",
        "meta-untrimmed",
        "meta-default",
    ] {
        assert_eq!(
            referer_for(case),
            Some(format!("{page_origin}/")),
            "meta policy {case}"
        );
    }
    for case in ["meta-inserted", "meta-override"] {
        assert_eq!(
            referer_for(case),
            Some(meta_page_referrer.clone()),
            "meta policy {case}"
        );
    }
    for case in [
        "meta-content",
        "meta-name-ignored",
        "meta-tree-order",
        "meta-removed",
        "meta-invalid",
    ] {
        assert_eq!(referer_for(case), None, "meta policy {case}");
    }
    assert_eq!(target_requests.len(), 17);
}
#[tokio::test]
async fn native_content_process_selected_file_reaches_fetch_and_form_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/form", "/fetch", "/arraybuffer", "/result"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_bytes(&mut stream).await;
            let request_text = String::from_utf8_lossy(&request);
            assert_eq!(request_text.split_whitespace().nth(1), Some(expected_path));
            if matches!(expected_path, "/fetch" | "/result") {
                assert_eq!(request_text.split_whitespace().next(), Some("POST"));
                let content_type = request_text.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            .then_some(value.trim().to_owned())
                    })
                });
                let boundary = content_type
                    .as_deref()
                    .and_then(|value| value.strip_prefix("multipart/form-data; boundary="))
                    .expect("selected-file multipart boundary");
                let header_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .expect("selected-file request headers")
                    + 4;
                let body = &request[header_end..];
                assert!(
                    body.windows(b"name=\"asset\"; filename=\"selected.bin\"".len())
                        .any(|window| { window == b"name=\"asset\"; filename=\"selected.bin\"" })
                );
                assert!(
                    body.windows([0_u8, 255, 128, b'F'].len())
                        .any(|window| window == [0_u8, 255, 128, b'F'])
                );
                assert!(body.ends_with(format!("--{boundary}--\r\n").as_bytes()));
            } else if expected_path == "/arraybuffer" {
                assert_eq!(request_text.split_whitespace().next(), Some("POST"));
                let header_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .expect("arraybuffer request headers")
                    + 4;
                assert_eq!(&request[header_end..], &[0_u8, 255, 128, 70]);
            }
            let (content_type, body) = match expected_path {
                "/form" => (
                    "text/html",
                    "<form id='form' method='post' enctype='multipart/form-data' action='/result'><input id='upload' type='file' name='asset'></form>",
                ),
                "/fetch" => ("text/plain", "fetch-uploaded"),
                "/arraybuffer" => ("text/plain", "arraybuffer-uploaded"),
                _ => ("text/html", "<title>Submitted</title><p>form-uploaded</p>"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let uploaded = engine
        .action_async(NativeAction::Upload {
            target: "id=upload".into(),
            files: vec![NativeFile {
                name: "selected.bin".into(),
                media_type: "application/octet-stream".into(),
                last_modified: 9012,
                bytes: vec![0, 255, 128, b'F'],
            }],
        })
        .await
        .unwrap();
    assert!(uploaded.accepted);
    engine
        .evaluate_async(
            "fetch('/fetch', { method: 'POST', body: new FormData(document.getElementById('form')) }).then(response => response.text()).then(text => { globalThis.selectedFetchResult = text; })",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.selectedFetchResult")
            .await
            .unwrap(),
        serde_json::json!("fetch-uploaded")
    );
    engine
        .evaluate_async(
            "fetch('/arraybuffer', { method: 'POST', body: new Uint8Array([0, 255, 128, 70]) }).then(response => response.text()).then(text => { globalThis.arrayBufferResult = text; })",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.arrayBufferResult")
            .await
            .unwrap(),
        serde_json::json!("arraybuffer-uploaded")
    );
    engine
        .evaluate_async("document.getElementById('form').submit()")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/result")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetches_cross_origin_post_after_cors_preflight() {
    let _guard = native_content_process_test_lock().lock().await;
    let document_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document_address = document_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let document_origin = format!("http://{document_address}");
    let expected_origin = document_origin.clone();
    let response_origin = document_origin.clone();

    let document_server = tokio::spawn(async move {
        let (mut stream, _) = document_listener.accept().await.unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<title>Fetch owner</title><p>Native fetch</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let api_server = tokio::spawn(async move {
        let (mut stream, _) = api_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
        assert_eq!(request.split_whitespace().nth(1), Some("/submit"));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("origin") && value.trim() == expected_origin
            })
        }));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("access-control-request-method") && value.trim() == "POST"
            })
        }));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("access-control-request-headers")
                    && value
                        .trim()
                        .eq_ignore_ascii_case("content-type, x-glass-token")
            })
        }));
        let response = format!(
            "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type, x-glass-token\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = api_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("POST"));
        assert_eq!(request.split_whitespace().nth(1), Some("/submit"));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("origin") && value.trim() == expected_origin
            })
        }));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("x-glass-token") && value.trim() == "alpha"
            })
        }));
        let body = request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .unwrap_or_default();
        assert_eq!(body, r#"{"name":"glass"}"#);
        let body = "{\"value\":\"cors-posted\"}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: {response_origin}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{document_origin}/index.html")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(&format!(
            "fetch('http://{api_address}/submit', {{ method: 'POST', credentials: 'omit', body: '{{\"name\":\"glass\"}}', headers: {{ 'Content-Type': 'application/json', 'X-Glass-Token': 'alpha' }} }}).then(response => response.json()).then(data => {{ globalThis.corsPostFetchValue = data.value; }});"
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.corsPostFetchValue")
            .await
            .unwrap(),
        serde_json::json!("cors-posted")
    );
    engine.close_async().await.unwrap();
    document_server.await.unwrap();
    api_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetches_cross_origin_simple_post_without_preflight() {
    let _guard = native_content_process_test_lock().lock().await;
    let document_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document_address = document_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let document_origin = format!("http://{document_address}");
    let expected_origin = document_origin.clone();
    let response_origin = document_origin.clone();

    let document_server = tokio::spawn(async move {
        let (mut stream, _) = document_listener.accept().await.unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<title>Fetch owner</title><p>Native fetch</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let api_server = tokio::spawn(async move {
        let (mut stream, _) = api_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("POST"));
        assert_eq!(request.split_whitespace().nth(1), Some("/submit"));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("origin") && value.trim() == expected_origin
            })
        }));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("content-type") && value.trim() == "text/plain"
            })
        }));
        let body = request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .unwrap_or_default();
        assert_eq!(body, "hello");
        let body = "{\"value\":\"simple-posted\"}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: {response_origin}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{document_origin}/index.html")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(&format!(
            "fetch('http://{api_address}/submit', {{ method: 'POST', credentials: 'omit', body: 'hello', headers: {{ 'Content-Type': 'text/plain' }} }}).then(response => response.json()).then(data => {{ globalThis.simplePostFetchValue = data.value; }});"
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.simplePostFetchValue")
            .await
            .unwrap(),
        serde_json::json!("simple-posted")
    );
    engine.close_async().await.unwrap();
    document_server.await.unwrap();
    api_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetches_common_http_methods_with_cors_preflight() {
    let _guard = native_content_process_test_lock().lock().await;
    let document_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document_address = document_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let document_origin = format!("http://{document_address}");
    let response_origin = document_origin.clone();

    let document_server = tokio::spawn(async move {
        let (mut stream, _) = document_listener.accept().await.unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<title>Fetch methods</title><p>Native fetch</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let api_server = tokio::spawn(async move {
        for _ in 0..11 {
            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let method = request.split_whitespace().next().unwrap_or_default();
            let is_preflight = method == "OPTIONS"
                && request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, _)| {
                        name.eq_ignore_ascii_case("access-control-request-method")
                    })
                });
            if is_preflight {
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("access-control-request-method")
                            && ["PUT", "PATCH", "DELETE", "OPTIONS", "REPORT"]
                                .contains(&value.trim())
                    })
                }));
                let response = format!(
                    "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Methods: HEAD, PUT, PATCH, DELETE, OPTIONS, REPORT\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                continue;
            }
            assert_eq!(request.split_whitespace().nth(1), Some("/methods"));
            assert!(matches!(
                method,
                "HEAD" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "REPORT"
            ));
            let request_body = request
                .split_once("\r\n\r\n")
                .map(|(_, body)| body)
                .unwrap_or_default();
            if method == "HEAD" {
                assert!(request_body.is_empty());
            } else {
                assert_eq!(request_body, method);
            }
            let body = if method == "HEAD" { "" } else { method };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: {response_origin}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            if !body.is_empty() {
                stream.write_all(body.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{document_origin}/index.html")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    for method in ["HEAD", "PUT", "PATCH", "DELETE", "OPTIONS", "REPORT"] {
        let body = if method == "HEAD" {
            "undefined".to_owned()
        } else {
            format!("'{method}'")
        };
        let result = engine
            .evaluate_async(&format!(
                "await (async () => {{ const response = await fetch('http://{api_address}/methods', {{ method: '{method}', credentials: 'omit', body: {body} }}); return [response.status, await response.text()]; }})()"
            ))
            .await
            .unwrap();
        assert_eq!(
            result,
            serde_json::json!([200, if method == "HEAD" { "" } else { method }])
        );
    }
    engine.close_async().await.unwrap();
    document_server.await.unwrap();
    api_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetches_bounded_redirect_modes() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let cross_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cross_address = cross_listener.local_addr().unwrap();
    let cross_server = tokio::spawn(async move {
        let accepted =
            tokio::time::timeout(Duration::from_millis(500), cross_listener.accept()).await;
        assert!(
            accepted.is_err(),
            "same-origin redirect reached the new origin"
        );
    });
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/follow", "/final", "/cross", "/error", "/manual"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = match expected_path {
                "/page" => {
                    let body = "<title>Fetch owner</title><p>Native fetch</p>";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/follow" => {
                    "HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned()
                }
                "/error" => {
                    "HTTP/1.1 302 Found\r\nLocation: /error-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned()
                }
                "/cross" => format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{cross_address}/final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                "/manual" => {
                    "HTTP/1.1 302 Found\r\nLocation: /manual-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned()
                }
                "/final" => {
                    let body = "redirected-final";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                _ => unreachable!(),
            };
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "fetch('/not-requested', { redirect: 'invalid' }).then(() => { globalThis.invalidRedirectValue = ['fulfilled', false]; }, error => { globalThis.invalidRedirectValue = [error.name, error.message.includes('redirect mode')]; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidRedirectValue")
            .await
            .unwrap(),
        serde_json::json!(["TypeError", true])
    );
    engine
        .evaluate_async(
            "fetch('/follow').then(async response => { globalThis.followRedirectValue = [response.type, response.redirected, response.status, response.url, await response.text()]; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.followRedirectValue")
            .await
            .unwrap(),
        serde_json::json!([
            "basic",
            true,
            200,
            format!("http://{address}/final"),
            "redirected-final"
        ])
    );
    engine
        .evaluate_async(
            "fetch('/cross', { mode: 'same-origin' }).then(() => { globalThis.crossOriginRedirectValue = ['fulfilled', false]; }, error => { globalThis.crossOriginRedirectValue = [error.name, error.message.includes('different origin')]; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.crossOriginRedirectValue")
            .await
            .unwrap(),
        serde_json::json!(["TypeError", true])
    );
    engine
        .evaluate_async(
            "fetch('/error', { redirect: 'error' }).then(() => { globalThis.errorRedirectValue = ['fulfilled', false]; }, error => { globalThis.errorRedirectValue = [error.name, error.message.includes('disallowed')]; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.errorRedirectValue")
            .await
            .unwrap(),
        serde_json::json!(["TypeError", true])
    );
    engine
        .evaluate_async(
            "fetch('/manual', { redirect: 'manual' }).then(async response => { const body = await response.text().then(() => 'readable', error => error.name); globalThis.manualRedirectValue = [response.type, response.redirected, response.status, response.url, Array.from(response.headers.entries()), body]; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.manualRedirectValue")
            .await
            .unwrap(),
        serde_json::json!(["opaqueredirect", false, 0, "", [], "TypeError"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
    cross_server.await.unwrap();
}
#[test]
fn native_content_process_exposes_bounded_script_fetch_promises() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_exposes_bounded_script_fetch_promises_inner());
    });
}

async fn native_content_process_exposes_bounded_script_fetch_promises_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/headers",
            "/data",
            "/data",
            "/binary",
            "/upload",
            "/constructed-upload",
            "/file-upload",
            "/form-upload",
            "/data",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request_bytes = read_http_request_bytes(&mut stream).await;
            let request = String::from_utf8_lossy(&request_bytes);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/data" {
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("x-glass-token") && value.trim() == "alpha"
                    })
                }));
            }
            if expected_path == "/headers" {
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("x-glass-token") && value.trim() == "alpha, beta"
                    })
                }));
            }
            if matches!(
                expected_path,
                "/upload" | "/constructed-upload" | "/file-upload"
            ) {
                let header_end = request_bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            && value.trim() == "application/octet-stream"
                    })
                }));
                assert_eq!(&request_bytes[header_end..], [0_u8, 0xff, 0x80, b'A']);
            }
            if expected_path == "/form-upload" {
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            && value
                                .trim()
                                .starts_with("multipart/form-data; boundary=----GlassNativeForm")
                    })
                }));
                let header_end = request_bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let body = &request_bytes[header_end..];
                assert!(
                    body.windows(b"name=\"name\"".len())
                        .any(|window| window == b"name=\"name\"")
                );
                assert!(
                    body.windows(b"name=\"payload\"".len())
                        .any(|window| window == b"name=\"payload\"")
                );
                assert!(
                    body.windows([0_u8, 0xff, 0x80, b'A'].len())
                        .any(|window| window == [0_u8, 0xff, 0x80, b'A'])
                );
            }
            if expected_path == "/binary" {
                let body = [0_u8, 0xff, 0x80, b'A'];
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
                continue;
            }
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<input id='result' value='pending'>")
            } else if expected_path == "/headers" {
                ("text/plain", "headers-ok")
            } else if expected_path.ends_with("upload") {
                ("text/plain", "uploaded")
            } else {
                ("application/json", "{\"value\":\"fetched\"}")
            };
            let extra_headers = if expected_path == "/data" {
                "X-Glass-Response: one\r\nX-Glass-Response: two\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async(
            "(() => { const headers = new Headers([['X-Glass-Token', 'alpha']]); headers.append('x-glass-token', 'beta'); headers.set('X-Glass-Extra', 'gamma'); headers.delete('x-glass-extra'); const endpoint = new URL('/headers', location.href); const query = new URL('?source=request#part', endpoint.href); const nested = new URL('../target?x=one#frag', 'http://example.test/a/b/page'); const protocolRelative = new URL('//api.example.test/v1', endpoint.href); const mutable = new URL('/headers?old=one', location.href); mutable.pathname = '/changed/../final'; mutable.searchParams.set('old', 'two'); mutable.searchParams.append('z', '3'); mutable.hash = 'changed'; const replacement = new URL('/headers?reset=yes#next', location.href); replacement.href = '/headers?reset=updated#final'; const authority = new URL('http://user:pass@api.example.test:8080/start', location.href); authority.protocol = 'https'; authority.hostname = 'other.example.test'; authority.port = '8443'; authority.username = 'next user'; authority.password = 'secret'; authority.host = 'final.example.test:9443'; const authorityBeforeInvalid = authority.href; const authorityErrors = []; try { authority.protocol = 'ftp'; } catch (error) { authorityErrors.push(error.name); } try { authority.host = 'bad/path'; } catch (error) { authorityErrors.push(error.name); } try { authority.port = '65536'; } catch (error) { authorityErrors.push(error.name); } globalThis.urlAuthorityView = [authority.protocol, authority.username, authority.password, authority.host, authority.hostname, authority.port, authority.href, authority.origin]; globalThis.urlAuthorityInvalid = [authorityErrors, authority.href === authorityBeforeInvalid]; const request = new Request(endpoint, { headers }); const relativeRequest = new Request('data'); const requestClone = request.clone(); globalThis.urlObjectView = [endpoint instanceof URL, endpoint.origin === location.origin, endpoint.pathname, endpoint.search, endpoint.hash, query.searchParams.get('source'), query.hash, nested.pathname, nested.searchParams.get('x'), nested.hash, protocolRelative.origin, protocolRelative.pathname, mutable.pathname, mutable.search, mutable.searchParams.get('old'), mutable.href.endsWith('/final?old=two&z=3#changed'), replacement.href.endsWith('/headers?reset=updated#final'), replacement.searchParams.get('reset'), replacement.hash]; globalThis.requestObjectView = [request instanceof Request, request.method, request.url === endpoint.href, relativeRequest.url.endsWith('/data'), request.mode, request.redirect, request.headers.get('x-glass-token'), requestClone.headers.get('x-glass-token'), requestClone !== request]; globalThis.requestHeaderView = [headers.get('x-glass-token'), headers.has('X-Glass-Token'), Array.from(headers.entries()), Array.from(headers.keys()), Array.from(headers.values()), headers.size]; fetch(requestClone).then(response => response.text()).then(value => { globalThis.headerFetch = value; }); })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.requestHeaderView")
            .await
            .unwrap(),
        serde_json::json!([
            "alpha, beta",
            true,
            [["x-glass-token", "alpha, beta"]],
            ["x-glass-token"],
            ["alpha, beta"],
            1
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.requestObjectView")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            "GET",
            true,
            true,
            "cors",
            "follow",
            "alpha, beta",
            "alpha, beta",
            true,
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.urlObjectView")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            true,
            "/headers",
            "",
            "",
            "request",
            "#part",
            "/a/target",
            "one",
            "#frag",
            "http://api.example.test",
            "/v1",
            "/final",
            "?old=two&z=3",
            "two",
            true,
            true,
            "updated",
            "#final",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.urlAuthorityView")
            .await
            .unwrap(),
        serde_json::json!([
            "https:",
            "next%20user",
            "secret",
            "final.example.test:9443",
            "final.example.test",
            "9443",
            "https://next%20user:secret@final.example.test:9443/start",
            "https://final.example.test:9443",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.urlAuthorityInvalid")
            .await
            .unwrap(),
        serde_json::json!([["TypeError", "TypeError", "TypeError"], true])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.headerFetch")
            .await
            .unwrap(),
        serde_json::json!("headers-ok")
    );
    engine
        .evaluate_async(
            "fetch('data', { headers: { 'X-Glass-Token': 'alpha' } }).then(async response => { globalThis.relativeFetchView = [response.url, await response.text()]; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.relativeFetchView")
            .await
            .unwrap(),
        serde_json::json!([format!("http://{address}/data"), "{\"value\":\"fetched\"}"])
    );
    engine
        .evaluate_async(
            "fetch('/data', { headers: { 'X-Glass-Token': 'alpha' } }).then(async response => { const headerEvents = []; response.headers.forEach((value, name) => headerEvents.push(name + '=' + value)); const clonedResponse = response.clone(); const jsonResponse = response.clone(); const textResponse = response.clone(); const blobResponse = response.clone(); const bufferResponse = response.clone(); const bytesResponse = response.clone(); const stream = response.body; const streamReader = stream.getReader(); const streamFirst = await streamReader.read(); const streamSecond = await streamReader.read(); const streamLocked = stream.locked; streamReader.releaseLock(); const releasedStreamRead = await streamReader.read().then(() => 'readable', error => error.name); const clonedText = await clonedResponse.text(); globalThis.fetchClone = [clonedText, clonedResponse.body instanceof ReadableStream, clonedResponse.body !== response.body]; globalThis.fetchStream = [response.body instanceof ReadableStream, streamLocked, stream.locked, streamFirst.done, Array.from(streamFirst.value), streamSecond.done, releasedStreamRead]; globalThis.fetchHeaders = [response.headers.get('CONTENT-TYPE'), response.headers.has('content-type'), Array.from(response.headers.entries()), Array.from(response.headers.keys()), Array.from(response.headers.values()), Array.from(response.headers), headerEvents]; return Promise.all([jsonResponse.json(), textResponse.text(), blobResponse.blob(), bufferResponse.arrayBuffer(), bytesResponse.bytes()]); }).then(async ([data, text, blob, buffer, bytes]) => { globalThis.fetchValue = data.value; globalThis.fetchBody = [text, blob instanceof Blob, buffer instanceof ArrayBuffer && Array.from(new Uint8Array(buffer)), Array.from(bytes), await blob.text()]; const binaryResponse = await fetch('/binary'); const binaryBufferResponse = binaryResponse.clone(); const binaryBytesResponse = binaryResponse.clone(); const binaryBlobResponse = binaryResponse.clone(); const binaryStream = binaryResponse.body; const binaryReader = binaryStream.getReader(); const binaryLocked = binaryStream.locked; await binaryReader.cancel(); binaryReader.releaseLock(); const canceledReader = binaryStream.getReader(); const canceledChunk = await canceledReader.read(); canceledReader.releaseLock(); globalThis.fetchStream = globalThis.fetchStream.concat([binaryResponse.body instanceof ReadableStream, binaryLocked, canceledChunk.done, binaryStream.locked]); const binaryBuffer = await binaryBufferResponse.arrayBuffer(); const binaryBytes = await binaryBytesResponse.bytes(); const binaryBlob = await binaryBlobResponse.blob(); globalThis.binaryFetchBody = [Array.from(new Uint8Array(binaryBuffer)), Array.from(binaryBytes), binaryBlob.size, Array.from(await binaryBlob.slice(1, 3).bytes()), await binaryBlob.text()]; const uploadResponse = await fetch('/upload', { method: 'POST', body: binaryBlob, headers: { 'Content-Type': 'application/octet-stream' } }); globalThis.binaryUpload = await uploadResponse.text(); const payload = new Uint8Array([0, 255, 128, 65]); const constructedBlob = new Blob([payload], { type: 'application/octet-stream' }); const constructedFile = new File([payload.buffer], 'payload.bin', { type: 'application/octet-stream' }); globalThis.binaryConstruction = [constructedBlob.size, Array.from(await constructedBlob.bytes()), constructedFile.size, Array.from(await constructedFile.bytes())]; const constructedUpload = await fetch('/constructed-upload', { method: 'POST', body: constructedBlob }); const fileUpload = await fetch('/file-upload', { method: 'POST', body: constructedFile }); globalThis.binaryUploads = [await constructedUpload.text(), await fileUpload.text()]; const form = new FormData(); form.append('name', 'glass'); form.append('payload', binaryBlob, 'payload.bin'); const formUpload = await fetch('/form-upload', { method: 'POST', body: form }); globalThis.formUpload = await formUpload.text(); document.getElementById('result').value = data.value; const controller = new AbortController(); const events = []; controller.signal.addEventListener('abort', () => events.push('listener')); controller.signal.onabort = () => events.push('property'); const request = fetch('/data', { signal: controller.signal, headers: { 'X-Glass-Token': 'alpha' } }); controller.abort(); controller.abort(); request.catch(error => { globalThis.abortValue = [error.name, controller.signal.aborted, controller.signal.reason.name, events]; }); });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.fetchValue")
            .await
            .unwrap(),
        serde_json::json!("fetched")
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.abortValue")
            .await
            .unwrap(),
        serde_json::json!(["AbortError", true, "AbortError", ["listener", "property"]])
    );
    assert_eq!(
        engine.evaluate_async("globalThis.fetchBody").await.unwrap(),
        serde_json::json!([
            "{\"value\":\"fetched\"}",
            true,
            [
                123, 34, 118, 97, 108, 117, 101, 34, 58, 34, 102, 101, 116, 99, 104, 101, 100, 34,
                125
            ],
            [
                123, 34, 118, 97, 108, 117, 101, 34, 58, 34, 102, 101, 116, 99, 104, 101, 100, 34,
                125
            ],
            "{\"value\":\"fetched\"}",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.fetchHeaders")
            .await
            .unwrap(),
        serde_json::json!([
            "application/json",
            true,
            [
                ["connection", "close"],
                ["content-length", "19"],
                ["content-type", "application/json"],
                ["x-glass-response", "one, two"]
            ],
            [
                "connection",
                "content-length",
                "content-type",
                "x-glass-response"
            ],
            ["close", "19", "application/json", "one, two"],
            [
                ["connection", "close"],
                ["content-length", "19"],
                ["content-type", "application/json"],
                ["x-glass-response", "one, two"]
            ],
            [
                "connection=close",
                "content-length=19",
                "content-type=application/json",
                "x-glass-response=one, two"
            ],
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.fetchStream")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            true,
            false,
            false,
            [
                123, 34, 118, 97, 108, 117, 101, 34, 58, 34, 102, 101, 116, 99, 104, 101, 100, 34,
                125
            ],
            true,
            "TypeError",
            true,
            true,
            true,
            false,
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.fetchClone")
            .await
            .unwrap(),
        serde_json::json!(["{\"value\":\"fetched\"}", true, true])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.binaryFetchBody")
            .await
            .unwrap(),
        serde_json::json!([
            [0, 255, 128, 65],
            [0, 255, 128, 65],
            4,
            [255, 128],
            "\u{0}\u{fffd}\u{fffd}A",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.binaryUpload")
            .await
            .unwrap(),
        serde_json::json!("uploaded")
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.binaryConstruction")
            .await
            .unwrap(),
        serde_json::json!([4, [0, 255, 128, 65], 4, [0, 255, 128, 65]])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.binaryUploads")
            .await
            .unwrap(),
        serde_json::json!(["uploaded", "uploaded"])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.formUpload")
            .await
            .unwrap(),
        serde_json::json!("uploaded")
    );
    engine
        .evaluate_async(
            "(() => { try { fetch('/data', { headers: { Cookie: 'secret' } }); } catch (error) { globalThis.forbiddenFetchHeader = [error.name, error.message]; } })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.forbiddenFetchHeader")
            .await
            .unwrap(),
        serde_json::json!(["TypeError", "native fetch header is forbidden"])
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('result').value")
            .await
            .unwrap(),
        serde_json::json!("fetched")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetch_cors_errors_keep_parent_cookies() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let api_origin = format!("http://{api_address}");
    let expected_origin = page_origin.clone();
    let worker_api_origin = api_origin.clone();
    let response_origin = page_origin.clone();
    let worker_script = format!(
        r#"self.onmessage = async event => {{
  if (event.data !== "run") return;
  try {{
    await fetch("{worker_api_origin}/worker-failed", {{ credentials: "include" }});
    self.postMessage(["fulfilled"]);
  }} catch (error) {{
    self.postMessage(["rejected", error.name, error instanceof TypeError]);
  }}
}};
self.postMessage(["ready"]);"#
    );

    let page_server = tokio::spawn(async move {
        for expected_path in ["/page", "/fetch-cookie-worker.js"] {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
                    .await
                    .expect("page and Worker requests stay bounded")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("GET"));
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, set_cookie, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n".to_owned(),
                    "<title>Fetch CORS cookie owner</title>".to_owned(),
                )
            } else {
                assert!(
                    header_value(&request, "cookie")
                        .unwrap_or_default()
                        .contains("root=seed"),
                    "same-origin Worker script request should use the parent cookie"
                );
                (
                    "application/javascript",
                    String::new(),
                    worker_script.clone(),
                )
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let api_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            for (path, required_cookies, set_cookie) in [
                (
                    "/page-failed",
                    vec!["root=seed"],
                    "page_fetch_failed=accepted",
                ),
                (
                    "/worker-failed",
                    vec!["root=seed", "page_fetch_failed=accepted"],
                    "worker_fetch_failed=accepted",
                ),
            ] {
                let (mut stream, _) = api_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().next(), Some("GET"));
                assert_eq!(request.split_whitespace().nth(1), Some(path));
                assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                assert!(header_value(&request, "access-control-request-method").is_none());
                let cookie = header_value(&request, "cookie").unwrap_or_default();
                for expected in required_cookies {
                    assert!(cookie.contains(expected), "missing {expected}: {cookie}");
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\nSet-Cookie: {set_cookie}; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 6\r\nConnection: close\r\n\r\nhidden"
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }

            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("GET"));
            assert_eq!(request.split_whitespace().nth(1), Some("/verify"));
            assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
            let cookie = header_value(&request, "cookie").unwrap_or_default();
            for expected in [
                "root=seed",
                "page_fetch_failed=accepted",
                "worker_fetch_failed=accepted",
            ] {
                assert!(cookie.contains(expected), "missing {expected}: {cookie}");
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nContent-Length: 8\r\nConnection: close\r\n\r\nverified"
            );
            stream.write_all(response.as_bytes()).await.unwrap();

            match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                Err(_) => {}
                Ok(Ok((mut stream, _))) => {
                    let request = read_http_request(&mut stream).await;
                    panic!("unexpected extra Fetch cookie request: {request}");
                }
                Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
            }
        })
        .await
        .expect("Fetch CORS-cookie request sequence stays bounded")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let page_failure = tokio::time::timeout(
        Duration::from_secs(30),
        engine.evaluate_async(format!(
            r#"await (async () => {{
              const pageFailure = await fetch("{api_origin}/page-failed", {{ credentials: "include" }})
                .then(() => ["fulfilled"], error => ["rejected", error.name, error instanceof TypeError]);
              globalThis.fetchCookieWorkerMessages = [];
              globalThis.fetchCookieWorker = new Worker("/fetch-cookie-worker.js");
              fetchCookieWorker.onmessage = event => fetchCookieWorkerMessages.push(event.data);
              return pageFailure;
            }})()"#
        )),
    )
    .await
    .expect("page Fetch CORS failure settles through the parent broker")
    .unwrap();
    assert_eq!(
        page_failure,
        serde_json::json!(["rejected", "TypeError", true])
    );

    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let ready = engine
                .evaluate_async("fetchCookieWorkerMessages.some(message => message[0] === 'ready')")
                .await
                .unwrap();
            if ready == serde_json::Value::Bool(true) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker Fetch fixture is ready");
    engine
        .evaluate_async("fetchCookieWorker.postMessage('run'); true")
        .await
        .unwrap();
    let worker_failure = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let message = engine
                .evaluate_async(
                    "fetchCookieWorkerMessages.find(message => message[0] === 'rejected')",
                )
                .await
                .unwrap();
            if !message.is_null() {
                break message;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker Fetch CORS failure settles through the parent broker");
    assert_eq!(
        worker_failure,
        serde_json::json!(["rejected", "TypeError", true])
    );
    let verified = engine
        .evaluate_async(format!(
            r#"await fetch("{api_origin}/verify", {{ credentials: "include" }}).then(response => response.text())"#
        ))
        .await
        .unwrap();
    assert_eq!(verified, serde_json::json!("verified"));
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    api_server.await.unwrap();
    for (name, value) in [
        ("root", "seed"),
        ("page_fetch_failed", "accepted"),
        ("worker_fetch_failed", "accepted"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == name && cookie.value == value && cookie.http_only }),
            "parent cookie jar is missing {name}={value}"
        );
    }
}
#[tokio::test]
async fn native_content_process_resolves_page_script_fetch_before_publish() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/data"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "<input id='result' value='pending'><script>fetch('/data').then(response => response.json()).then(data => { globalThis.pageFetchValue = data.value; document.getElementById('result').value = data.value; });</script>",
                )
            } else {
                ("application/json", "{\"value\":\"published\"}")
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
            .evaluate_async("globalThis.pageFetchValue")
            .await
            .unwrap(),
        serde_json::json!("published")
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('result').value")
            .await
            .unwrap(),
        serde_json::json!("published")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_drives_timer_started_fetch_continuation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/data"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<input id='result' value='pending'>")
            } else {
                ("text/plain", "network-done")
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
                "await new Promise(resolve => setTimeout(() => fetch('/data').then(response => response.text()).then(value => { document.getElementById('result').value = value; resolve(value); }), 5))",
            )
            .await
            .unwrap(),
        serde_json::json!("network-done")
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('result').value")
            .await
            .unwrap(),
        serde_json::json!("network-done")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_exposes_fetch_response_body_as_a_stream() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Fetch stream owner</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/stream"));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
        for chunk in [&[1_u8, 2, 3][..], &[4_u8, 5][..]] {
            stream
                .write_all(format!("{:X}\r\n", chunk.len()).as_bytes())
                .await
                .unwrap();
            stream.write_all(chunk).await.unwrap();
            stream.write_all(b"\r\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        stream.write_all(b"0\r\n\r\n").await.unwrap();
    });
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(engine.evaluate_async("await (async () => { const response = await fetch('/stream'); const reader = response.body.getReader(); const bytes = []; while (true) { const item = await reader.read(); if (item.done) break; bytes.push(...item.value); } return [response.status, response.body instanceof ReadableStream, bytes]; })()").await.unwrap(), serde_json::json!([200, true, [1, 2, 3, 4, 5]]));
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_dispatches_large_fetch_stream_chunk_as_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Large Fetch stream</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/stream"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let first = vec![7_u8; 20_000];
        stream
            .write_all(format!("{:X}\r\n", first.len()).as_bytes())
            .await
            .unwrap();
        stream.write_all(&first).await.unwrap();
        stream.write_all(b"\r\n0\r\n\r\n").await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/stream').then(async response => { const reader = response.body.getReader(); let total = 0; for (;;) { const part = await reader.read(); if (part.value) total += part.value.length; if (part.done) return [total, response.body instanceof ReadableStream]; } })",
            )
            .await
            .unwrap(),
        serde_json::json!([20_000, true])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_cancels_buffered_parent_fetch_stream() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Fetch cancellation owner</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/stream-cancel"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        stream.write_all(b"5\r\nfirst\r\n0\r\n\r\n").await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const response = await fetch('/stream-cancel'); const reader = response.body.getReader(); const first = await reader.read(); await reader.cancel(); const after = await reader.read(); reader.releaseLock(); return [Array.from(first.value), first.done, after.done, response.body.locked]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([[102, 105, 114, 115, 116], false, true, false])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_tracks_fetch_body_disturbance_and_clone_ownership() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/body", "/body"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<body>Fetch body owner</body>")
            } else {
                ("text/plain", "body-used")
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
                "await (async () => { const response = await fetch('/body'); const clone = response.clone(); const initial = [response.bodyUsed, clone.bodyUsed]; const text = await response.text(); const responseAfter = [response.bodyUsed, await response.text().then(() => 'readable', error => error.name), (() => { try { response.clone(); return 'cloneable'; } catch (error) { return error.name; } })()]; const cloneText = await clone.text(); const cloneAfter = [clone.bodyUsed, await clone.text().then(() => 'readable', error => error.name), (() => { try { clone.clone(); return 'cloneable'; } catch (error) { return error.name; } })()]; const locked = await fetch('/body'); const reader = locked.body.getReader(); const lockState = [locked.bodyUsed, (() => { try { locked.clone(); return 'cloneable'; } catch (error) { return error.name; } })()]; const first = await reader.read(); const disturbed = locked.bodyUsed; await reader.cancel(); reader.releaseLock(); const canceled = [locked.bodyUsed, (() => { try { locked.clone(); return 'cloneable'; } catch (error) { return error.name; } })()]; return { initial, text, responseAfter, cloneText, cloneAfter, lockState, first: [Array.from(first.value), first.done], disturbed, canceled }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "initial": [false, false],
            "text": "body-used",
            "responseAfter": [true, "TypeError", "TypeError"],
            "cloneText": "body-used",
            "cloneAfter": [true, "TypeError", "TypeError"],
            "lockState": [false, "TypeError"],
            "first": [[98, 111, 100, 121, 45, 117, 115, 101, 100], false],
            "disturbed": true,
            "canceled": [true, "TypeError"]
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetches_stream_request_bodies_and_clones_them() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/upload", "/upload", "/options-upload"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path != "/page" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert_eq!(body, "stream-body");
            }
            let content_type = if expected_path == "/page" {
                "text/html"
            } else {
                "text/plain"
            };
            let body = if expected_path == "/page" {
                "<body>Stream request body</body>"
            } else {
                "ok"
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
    let text_state = engine
        .evaluate_async(
            "const makeStream = () => new ReadableStream({ start(controller) { controller.enqueue('stream-'); controller.enqueue('body'); controller.close(); } }); const textRequest = new Request('/text', { method: 'POST', body: makeStream(), headers: { 'Content-Type': 'text/plain' } }); const text = await textRequest.text(); globalThis.makeUploadStream = makeStream; globalThis.requestBodyTextState = [text, textRequest.bodyUsed, textRequest.body.locked]; globalThis.uploadRequest = new Request('/upload', { method: 'POST', body: makeStream(), headers: { 'Content-Type': 'text/plain' } }); globalThis.uploadClone = uploadRequest.clone(); [uploadRequest.bodyUsed, uploadClone.bodyUsed]",
        )
        .await
        .unwrap();
    assert_eq!(text_state, serde_json::json!([false, false]));
    assert_eq!(
        engine
            .evaluate_async("[requestBodyTextState, uploadRequest.bodyUsed, uploadClone.bodyUsed]")
            .await
            .unwrap(),
        serde_json::json!([["stream-body", true, false], false, false])
    );
    assert_eq!(
        engine
            .evaluate_async("await (async () => { const response = await fetch(uploadRequest); globalThis.firstUploadText = await response.text(); return [uploadRequest.bodyUsed, uploadClone.bodyUsed]; })()")
            .await
            .unwrap(),
        serde_json::json!([true, false])
    );
    assert_eq!(
        engine
            .evaluate_async("await (async () => { const response = await fetch(uploadClone); globalThis.secondUploadText = await response.text(); return [uploadRequest.bodyUsed, uploadClone.bodyUsed]; })()")
            .await
            .unwrap(),
        serde_json::json!([true, true])
    );
    assert_eq!(
        engine
            .evaluate_async("await (async () => { const response = await fetch('/options-upload', { method: 'POST', body: makeUploadStream(), headers: { 'Content-Type': 'text/plain' } }); return await response.text(); })()")
            .await
            .unwrap(),
        serde_json::json!("ok")
    );
    assert_eq!(
        engine
            .evaluate_async("[firstUploadText, secondUploadText]")
            .await
            .unwrap(),
        serde_json::json!(["ok", "ok"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_does_not_replay_stream_request_bodies_across_redirects() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/switch", "/redirect-final", "/replay"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_bytes(&mut stream).await;
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .unwrap()
                + 4;
            let request_text = String::from_utf8_lossy(&request[..header_end]);
            assert_eq!(request_text.split_whitespace().nth(1), Some(expected_path));
            if matches!(expected_path, "/switch" | "/replay") {
                assert_eq!(request_text.split_whitespace().next(), Some("POST"));
                assert_eq!(&request[header_end..], b"redirect-body");
            }
            let response = match expected_path {
                "/page" => {
                    let body = "<body>Stream redirect owner</body>";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/switch" => {
                    "HTTP/1.1 302 Found\r\nLocation: /redirect-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
                }
                "/redirect-final" => {
                    let body = "switched";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/replay" => {
                    "HTTP/1.1 307 Temporary Redirect\r\nLocation: /replay-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned()
                }
                _ => unreachable!(),
            };
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
                r#"await (async () => {
                    const makeStream = () => new ReadableStream({
                        start(controller) {
                            controller.enqueue('redirect-');
                            controller.enqueue('body');
                            controller.close();
                        },
                    });
                    const followed = await fetch('/switch', { method: 'POST', body: makeStream() })
                        .then(async response => [response.status, await response.text()]);
                    const replay = await fetch('/replay', { method: 'POST', body: makeStream() })
                        .then(() => 'fulfilled', error => [error.name, error.message.includes('cannot be replayed')]);
                    return [followed, replay];
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!([[200, "switched"], ["TypeError", true]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_tees_buffered_fetch_bodies_for_clone_readers() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Fetch tee owner</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/tee"));
        // Child-facing Fetch streams over parent-brokered responses begin once
        // the bounded HTTP body is complete; transport cancellation is covered
        // directly by the core fetch-stream test.
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\ntee\r\n0\r\n\r\n",
            )
            .await
            .unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const response = await fetch('/tee'); const clone = response.clone(); const left = response.body.getReader(); const right = clone.body.getReader(); const leftFirst = await left.read(); const rightFirst = await right.read(); await left.cancel(); await right.cancel(); left.releaseLock(); right.releaseLock(); return [Array.from(leftFirst.value), leftFirst.done, Array.from(rightFirst.value), rightFirst.done, response.bodyUsed, clone.bodyUsed]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([[116, 101, 101], false, [116, 101, 101], false, true, true])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_exposes_bounded_same_origin_post_fetch() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/submit"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/submit" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            && value.trim() == "application/json"
                    })
                }));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert_eq!(body, r#"{"name":"glass"}"#);
            }
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<p>Fetch owner</p>")
            } else {
                ("application/json", "{\"value\":\"posted\"}")
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
    engine
        .evaluate_async(
            "fetch('/submit', { method: 'POST', body: '{\"name\":\"glass\"}', headers: { 'Content-Type': 'application/json' } }).then(response => response.json()).then(data => { globalThis.postFetchValue = data.value; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.postFetchValue")
            .await
            .unwrap(),
        serde_json::json!("posted")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetch_response_delivery_preserves_large_payloads() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let large_body = "x".repeat(20_000);
    let expected_body = large_body.clone();
    let server = tokio::spawn(async move {
        let responses = vec![
            (
                "/page",
                "text/html".to_owned(),
                "<p>Fetch owner</p>".to_owned(),
            ),
            ("/large-fetch", "text/plain".to_owned(), large_body),
        ];
        for (expected_path, content_type, body) in responses {
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
            .evaluate_async("await fetch('/large-fetch').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!(expected_body)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetches_bounded_text_form_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/formdata"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/formdata" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                let content_type = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            .then_some(value.trim().to_owned())
                    })
                });
                assert!(content_type.is_some_and(|value| {
                    value.starts_with("multipart/form-data; boundary=----GlassNativeForm")
                }));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert!(
                    body.contains("Content-Disposition: form-data; name=\"name\"\r\n\r\nGlass\r\n"),
                    "unexpected FormData body: {body:?}"
                );
                assert!(
                    body.contains("Content-Disposition: form-data; name=\"tag\"\r\n\r\nengine\r\n"),
                    "unexpected FormData body: {body:?}"
                );
            }
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<p>FormData owner</p>")
            } else {
                ("text/plain", "formdata-accepted")
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
    engine
        .evaluate_async(
            r#"(() => {
                const formData = new FormData();
                formData.append('name', 'Glass');
                formData.append('tag', 'old');
                formData.set('tag', 'engine');
                const iterator = formData.entries();
                const first = iterator.next();
                const second = iterator.next();
                const done = iterator.next();
                const visited = [];
                formData.forEach((value, name) => visited.push(name + '=' + value));
                globalThis.formDataIterator = [
                    iterator[Symbol.iterator]() === iterator,
                    first.value,
                    second.value,
                    done.done,
                    Array.from(formData.keys()),
                    Array.from(formData.values()),
                    Array.from(formData).map(entry => entry.join('=')),
                    visited,
                ];
                fetch('/formdata', { method: 'POST', body: formData })
                    .then(response => response.text())
                    .then(text => { globalThis.formDataResponse = text; });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.formDataResponse")
            .await
            .unwrap(),
        serde_json::json!("formdata-accepted")
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.formDataIterator")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            ["name", "Glass"],
            ["tag", "engine"],
            true,
            ["name", "tag"],
            ["Glass", "engine"],
            ["name=Glass", "tag=engine"],
            ["name=Glass", "tag=engine"]
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fetches_bounded_url_search_params() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/params"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/params" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            && value.trim() == "application/x-www-form-urlencoded;charset=UTF-8"
                    })
                }));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert_eq!(body, "name=Glass&tag=engine");
            }
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<p>URLSearchParams owner</p>")
            } else {
                ("text/plain", "params-accepted")
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
    engine
        .evaluate_async(
            r#"(() => {
                const params = new URLSearchParams();
                params.append('name', 'Glass');
                params.append('tag', 'old');
                params.set('tag', 'engine');
                const api = new URLSearchParams([['z', 'last'], ['a', 'first'], ['a', 'second']]);
                api.sort();
                const visited = [];
                api.forEach((value, name) => visited.push(name + '=' + value));
                const mapIterable = new URLSearchParams(new Map([['b', 'two'], ['a', 'one']]));
                const setIterable = new URLSearchParams(new Set([['x', 'one'], ['y', 'two']]));
                globalThis.paramsApi = { size: api.size, entries: Array.from(api.entries()), keys: Array.from(api.keys()), values: Array.from(api.values()), visited, record: new URLSearchParams({ b: 'two', a: 'one' }).toString(), mapIterable: mapIterable.toString(), setIterable: setIterable.toString() };
                fetch('/params', { method: 'POST', body: params })
                    .then(response => response.text())
                    .then(text => { globalThis.paramsResponse = text; });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.paramsResponse")
            .await
            .unwrap(),
        serde_json::json!("params-accepted")
    );
    assert_eq!(
        engine.evaluate_async("globalThis.paramsApi").await.unwrap(),
        serde_json::json!({
            "size": 3,
            "entries": [["a", "first"], ["a", "second"], ["z", "last"]],
            "keys": ["a", "a", "z"],
            "values": ["first", "second", "last"],
            "visited": ["a=first", "a=second", "z=last"],
            "record": "b=two&a=one",
            "mapIterable": "b=two&a=one",
            "setIterable": "x=one&y=two",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

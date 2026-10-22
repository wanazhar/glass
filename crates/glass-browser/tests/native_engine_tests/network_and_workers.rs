use super::*;

#[tokio::test]
async fn native_service_worker_cache_matching_options_filter_entries() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/page",
                "text/html",
                "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>cache matching page</main></body></html>",
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => event.waitUntil(caches.open('matching').then(async cache => {
  await cache.put(new Request('/asset?lang=en', { headers: { 'X-Variant': 'en' } }), new Response('x'.repeat(20000), {
    headers: { 'Vary': 'X-Variant', 'X-Value': 'en' },
  }));
  await cache.put(new Request('/asset?lang=fr', { headers: { 'X-Variant': 'fr' } }), new Response('french', {
    headers: { 'Vary': 'X-Variant', 'X-Value': 'fr' },
  }));
  await cache.put('/method', new Response('get-only'));
  return self.skipWaiting();
})));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname !== '/probe') return;
  event.respondWith((async () => {
    const cache = await caches.open('matching');
    const exact = await cache.match(new Request('/asset?lang=en', { headers: { 'X-Variant': 'en' } }));
    const queryless = await cache.match(new Request('/asset?lang=de', { headers: { 'X-Variant': 'en' } }), { ignoreSearch: true });
    const varyMiss = await cache.match(new Request('/asset?lang=en', { headers: { 'X-Variant': 'fr' } }));
    const varyIgnored = await cache.match(new Request('/asset?lang=en', { headers: { 'X-Variant': 'fr' } }), { ignoreVary: true });
    const headDefault = await cache.match(new Request('/method', { method: 'HEAD' }));
    const headIgnored = await cache.match(new Request('/method', { method: 'HEAD' }), { ignoreMethod: true });
    const keys = await cache.keys(new Request('/asset?lang=de', { headers: { 'X-Variant': 'en' } }), { ignoreSearch: true });
    const deleted = await cache.delete(new Request('/asset?lang=en', { headers: { 'X-Variant': 'fr' } }), { ignoreVary: true });
    const afterDelete = await cache.match(new Request('/asset?lang=en', { headers: { 'X-Variant': 'en' } }), { ignoreVary: true });
    return new Response(JSON.stringify({
      exact: exact && (await exact.text()).length,
      queryless: queryless && (await queryless.text()).length,
      varyMiss: varyMiss ? await varyMiss.text() : null,
      varyIgnored: varyIgnored && (await varyIgnored.text()).length,
      headDefault: Boolean(headDefault),
      headIgnored: headIgnored && await headIgnored.text(),
      keys: keys.map(request => request.url),
      deleted,
      afterDelete: Boolean(afterDelete),
    }), { headers: { 'Content-Type': 'application/json' } });
  })());
});"#,
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
                "await registrationPromise.then(reg => [reg.active.state, reg.active.scriptURL])"
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", format!("http://{address}/sw.js")])
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/probe').then(response => response.json())")
            .await
            .unwrap(),
        serde_json::json!({
            "exact": 20000,
            "queryless": 20000,
            "varyMiss": null,
            "varyIgnored": 20000,
            "headDefault": false,
            "headIgnored": "get-only",
            "keys": [format!("http://{address}/asset?lang=en")],
            "deleted": true,
            "afterDelete": false,
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn native_service_worker_cache_rejects_partial_and_commits_add_all_atomically() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (status, reason, content_type, body) = match path {
                "/page" => (
                    200,
                    "OK",
                    "text/html",
                    "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>cache conformance page</main></body></html>",
                ),
                "/sw.js" => (
                    200,
                    "OK",
                    "application/javascript",
                    r#"self.addEventListener('install', event => event.waitUntil((async () => {
  const cache = await caches.open('v1');
  await cache.put('/stable', new Response('stable'));
  let partialRejected = false;
  try { await cache.put('/partial', new Response('partial', { status: 206 })); } catch (_) { partialRejected = true; }
  let varyRejected = false;
  try { await cache.put('/vary', new Response('vary', { headers: { 'Vary': '*' } })); } catch (_) { varyRejected = true; }
  await cache.addAll(['/good']);
  let addAllRejected = false;
  try { await cache.addAll(['/good-again', '/partial']); } catch (_) { addAllRejected = true; }
  const keys = await cache.keys();
  await cache.put('/result', new Response(JSON.stringify({
    partialRejected,
    varyRejected,
    addAllRejected,
    keyUrls: keys.map(request => request.url),
  }), { headers: { 'Content-Type': 'application/json' } }));
  return self.skipWaiting();
})()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => event.respondWith(caches.match(event.request).then(cached => cached || fetch(event.request))));"#,
                ),
                "/good" | "/good-again" => (200, "OK", "text/plain", "good"),
                "/partial" => (206, "Partial Content", "text/plain", "partial"),
                _ => panic!("unexpected cache conformance request: {path}"),
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                "await registrationPromise.then(reg => [reg.active.state, reg.active.scriptURL])",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", format!("http://{address}/sw.js")])
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/result').then(response => response.json())")
            .await
            .unwrap(),
        serde_json::json!({
            "partialRejected": true,
            "varyRejected": true,
            "addAllRejected": true,
            "keyUrls": [
                format!("http://{address}/good"),
                format!("http://{address}/stable"),
            ],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn native_service_worker_fetch_event_navigation_request_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-navigation-fetch-cookies-{}.json",
        std::process::id()
    ));
    let cookie_profile_path =
        std::path::PathBuf::from(format!("{}.cookies", profile_path.display()));
    let lock_path = profile_path.with_extension("lock");
    let cookie_lock_path = cookie_profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [
        &profile_path,
        &cookie_profile_path,
        &lock_path,
        &cookie_lock_path,
        &events_path,
        &readers_path,
    ] {
        let _ = fs::remove_file(path);
    }

    let register_page = "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>service worker navigation fetch</main></body></html>";
    let worker_script = r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => event.respondWith(fetch(event.request)));"#;
    let server = tokio::spawn(async move {
        for expected_path in ["/register", "/sw.js", "/controlled", "/after"] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("Service Worker navigation Fetch should reach the parent network")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned())
                .unwrap_or_default();
            let (set_cookie, content_type, body) = match expected_path {
                "/register" => (
                    "Set-Cookie: sw_fetch_seed=parent; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    register_page,
                ),
                "/sw.js" => {
                    assert!(cookie.contains("sw_fetch_seed=parent"));
                    ("", "text/javascript", worker_script)
                }
                "/controlled" => {
                    assert!(cookie.contains("sw_fetch_seed=parent"));
                    (
                        "Set-Cookie: sw_fetch_rotation=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/html",
                        "<!doctype html><html><body><main>parent-brokered navigation</main></body></html>",
                    )
                }
                "/after" => {
                    assert!(cookie.contains("sw_fetch_seed=parent"));
                    assert!(cookie.contains("sw_fetch_rotation=accepted"));
                    ("", "text/plain", "parent-cookie-ok")
                }
                _ => unreachable!(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{set_cookie}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/register"))
            .with_storage_path(profile_path.clone()),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => [reg.active.state, reg.active.scriptURL])"
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", format!("http://{address}/sw.js")])
    );
    engine
        .navigate_async(format!("http://{address}/controlled"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await fetch('/after').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("parent-cookie-ok")
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    for expected_name in ["sw_fetch_seed", "sw_fetch_rotation"] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == expected_name && cookie.http_only })
        );
    }
    engine.close_async().await.unwrap();
    server.await.unwrap();

    for path in [
        &profile_path,
        &cookie_profile_path,
        &lock_path,
        &cookie_lock_path,
        &events_path,
        &readers_path,
    ] {
        let _ = fs::remove_file(path);
    }
}

#[tokio::test]
async fn native_service_worker_navigation_preload_sends_header_and_reuses_response() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let register_page = "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>preload registration</main></body></html>";
    let worker_script = r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  const pathname = new URL(event.request.url).pathname;
  if (pathname === '/handled') {
    event.respondWith(event.preloadResponse.then(async response => {
      if (!response) return new Response('missing preload', { status: 503 });
      return new Response('<!doctype html><html><body>' + [
        response.status,
        new URL(response.url).pathname,
        response.headers.get('x-preload-marker'),
        await response.text(),
      ].join('/') + '</body></html>', {
        headers: { 'Content-Type': 'text/html' },
      });
    }));
  } else if (pathname === '/failed') {
    event.respondWith(event.preloadResponse.then(
      () => new Response('unexpected preload response'),
      error => new Response('<!doctype html><html><body>preload-' + error.name + '</body></html>', {
        headers: { 'Content-Type': 'text/html' },
      }),
    ));
  } else if (pathname === '/request-referrer') {
    const observed = JSON.stringify({
      referrer: event.request.referrer,
      referrerPolicy: event.request.referrerPolicy,
    });
    event.respondWith(Promise.resolve(new Response(
      '<!doctype html><html><body>' + observed + '</body></html>',
      { headers: { 'Content-Type': 'text/html' } },
    )));
  }
});"#;
    let server = tokio::spawn(async move {
        for (expected_path, expected_preload_header, content_type, body) in [
            ("/register", None, "text/html", register_page),
            ("/sw.js", None, "application/javascript", worker_script),
            (
                "/controlled",
                Some("glass-nav-preload"),
                "text/html",
                "<!doctype html><html><body>unhandled preload body</body></html>",
            ),
            (
                "/request-referrer",
                Some("glass-nav-preload"),
                "text/html",
                "<!doctype html><html><body>unused preload body</body></html>",
            ),
            (
                "/handled",
                Some("glass-nav-preload"),
                "text/plain",
                "handled preload body",
            ),
            ("/failed", Some("glass-nav-preload"), "text/html", ""),
            ("/redirect", Some("glass-nav-preload"), "", ""),
            (
                "/redirected",
                Some("glass-nav-preload"),
                "text/html",
                "<!doctype html><html><body>redirected preload final</body></html>",
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            if expected_path == "/request-referrer" && request.is_empty() {
                // This handler returns its own Response without awaiting
                // event.preloadResponse. The engine cancels the speculative
                // preload if it has not reached the HTTP server yet, so the
                // accepted connection can close before sending request bytes.
                continue;
            }
            assert_eq!(
                request.split_whitespace().next(),
                Some("GET"),
                "expected a GET for {expected_path}, received: {request:?}"
            );
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if let Some(expected_value) = expected_preload_header {
                assert!(
                    request.to_ascii_lowercase().contains(&format!(
                        "service-worker-navigation-preload: {expected_value}"
                    )),
                    "navigation preload header missing from {expected_path}: {request}"
                );
            } else {
                assert!(
                    !request
                        .to_ascii_lowercase()
                        .contains("service-worker-navigation-preload:")
                );
            }
            let has_referer = request
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("referer:"));
            if matches!(
                expected_path,
                "/sw.js" | "/controlled" | "/request-referrer" | "/redirected"
            ) {
                assert!(
                    !has_referer,
                    "Referrer-Policy should suppress Referer for {expected_path}: {request}"
                );
            }
            if expected_path == "/redirect" {
                assert!(
                    has_referer,
                    "the initial same-origin redirect should retain its Referer: {request}"
                );
            }
            if expected_path == "/failed" {
                drop(stream);
                continue;
            }
            if expected_path == "/redirect" {
                stream
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /redirected\r\nReferrer-Policy: no-referrer\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
                continue;
            }
            let referrer_policy_header = if matches!(expected_path, "/register" | "/controlled") {
                "Referrer-Policy: no-referrer\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{referrer_policy_header}X-Preload-Marker: upstream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                "await registrationPromise.then(async reg => { await reg.navigationPreload.enable(); await reg.navigationPreload.setHeaderValue('glass-nav-preload'); return reg.navigationPreload.getState(); })",
            )
            .await
            .unwrap(),
        serde_json::json!({"enabled": true, "headerValue": "glass-nav-preload"})
    );

    engine
        .navigate_async(format!("http://{address}/controlled"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.innerText")
            .await
            .unwrap(),
        serde_json::json!("unhandled preload body")
    );

    engine
        .navigate_async(format!("http://{address}/request-referrer"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.innerText")
            .await
            .unwrap(),
        serde_json::json!(r#"{"referrer":"","referrerPolicy":"no-referrer"}"#)
    );

    engine
        .navigate_async(format!("http://{address}/handled"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.innerText")
            .await
            .unwrap(),
        serde_json::json!("200//handled/upstream/handled preload body")
    );

    engine
        .navigate_async(format!("http://{address}/failed"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.innerText")
            .await
            .unwrap(),
        serde_json::json!("preload-TypeError")
    );

    engine
        .navigate_async(format!("http://{address}/redirect"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.innerText")
            .await
            .unwrap(),
        serde_json::json!("redirected preload final")
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}

#[test]
fn native_service_worker_navigation_preload_does_not_delay_independent_timer_response() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_service_worker_navigation_preload_timer_response_inner());
    });
}

#[test]
fn native_service_worker_wait_until_fetch_does_not_delay_response() {
    std::thread::Builder::new()
        .name("native-service-worker-lifetime-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(4 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("Service Worker lifetime test runtime should build");
            runtime
                .block_on(native_service_worker_wait_until_fetch_does_not_delay_response_inner());
        })
        .expect("Service Worker lifetime test thread should start")
        .join()
        .expect("Service Worker lifetime test thread should complete");
}

#[test]
fn native_service_worker_navigation_preload_is_cancelled_with_navigation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (preload_seen_tx, preload_seen_rx) = oneshot::channel();
    let (preload_closed_tx, preload_closed_rx) = oneshot::channel();
    let registration_page = "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>preload cancellation</main></body></html>";
    let worker_script = r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/slow')
    event.respondWith(event.preloadResponse.then(response => response || new Response('missing preload')));
});"#;
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            match path {
                "/register" => {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: parent-session=seed; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{registration_page}",
                        registration_page.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/sw.js" => {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{worker_script}",
                        worker_script.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/slow" => {
                    assert_eq!(request.split_whitespace().next(), Some("GET"));
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains("service-worker-navigation-preload: true")
                    );
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains("cookie: parent-session=seed"),
                        "the parent must attach its HttpOnly cookie to the preload request"
                    );
                    let _ = preload_seen_tx.send(());
                    let mut byte = [0_u8; 1];
                    let closed =
                        tokio::time::timeout(Duration::from_secs(5), stream.read(&mut byte))
                            .await
                            .is_ok_and(|result| !matches!(result, Ok(read) if read > 0));
                    let _ = preload_closed_tx.send(closed);
                    return;
                }
                other => panic!("unexpected navigation preload request: {other}: {request}"),
            }
        }
    });

    let session = BrowserSession::start_default().await.unwrap();
    session
        .navigate(format!("http://{address}/register"))
        .await
        .unwrap();
    session
        .script("await navigator.serviceWorker.ready.then(async registration => { await registration.navigationPreload.enable(); if (!registration.active) throw new Error('active ServiceWorker is missing'); })")
        .await
        .unwrap();
    let initial = session.evidence(EvidenceLevel::Compact).await.unwrap();
    {
        let navigation = session.navigate(format!("http://{address}/slow"));
        tokio::pin!(navigation);
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::select! {
                result = &mut navigation => panic!("preload navigation completed before cancellation: {result:?}"),
                seen = preload_seen_rx => seen.expect("server observed the navigation preload"),
            }
        })
        .await
        .expect("navigation preload did not reach the fixture server");

        let stopped = tokio::time::timeout(
            Duration::from_secs(5),
            session.stop_loading_with_revision(initial.revision),
        )
        .await
        .expect("stop-loading did not settle promptly")
        .unwrap();
        assert_eq!(stopped.action, "stopLoading");
        assert_eq!(stopped.current_revision, initial.revision);
        let error = tokio::time::timeout(Duration::from_secs(5), &mut navigation)
            .await
            .expect("cancelled navigation did not settle")
            .expect_err("cancelled navigation must not commit");
        assert!(matches!(
            error.downcast_ref::<BrowserBackendError>(),
            Some(BrowserBackendError::Lifecycle { state, .. }) if state == "cancelled"
        ));
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(5), preload_closed_rx)
            .await
            .expect("cancelled preload connection did not close")
            .expect("fixture server reports preload connection state"),
        "navigation cancellation must close the in-flight preload request"
    );
    let after_cancel = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(after_cancel.url, initial.url);
    assert_eq!(after_cancel.revision, initial.revision);
    session.close().await.unwrap();
    server.await.unwrap();
        });
    });
}

#[test]
fn native_http_image_map_keyboard_focus_and_same_origin_frame_activation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let (content_type, extra_headers, body) = match path {
                "/parent" => (
                    "text/html",
                    "",
                    "<img id='parent-image' usemap='#parent-map' style='display:block;width:100px;height:100px'><map name='parent-map'><area id='parent-area' shape='default' href='/parent-destination' target='_blank' alt='Parent destination'><area id='parent-download' shape='rect' coords='0,0,10,10' href='/download' download='keyboard-map.txt' alt='Download area'></map><iframe id='child' src='/child'></iframe>",
                ),
                "/child" => (
                    "text/html",
                    "",
                    "<img id='frame-image' usemap='#frame-map' style='display:block;width:100px;height:100px'><map name='frame-map'><area id='frame-area' shape='default' href='/frame-destination' alt='Frame destination'></map><script>globalThis.__frameKeyboardTrace = []; globalThis.__cancelFrameClick = true; const image = document.getElementById('frame-image'); const area = document.getElementById('frame-area'); image.addEventListener('keydown', event => __frameKeyboardTrace.push('keydown:' + event.target.id)); image.addEventListener('keyup', event => __frameKeyboardTrace.push('keyup:' + event.target.id)); area.addEventListener('click', event => { __frameKeyboardTrace.push('click:' + event.target.id); if (__cancelFrameClick) event.preventDefault(); });</script>",
                ),
                "/parent-destination" => ("text/html", "", "<title>Parent popup</title>"),
                "/frame-destination" => (
                    "text/html",
                    "",
                    "<title>Frame destination</title><p id='frame-destination'>loaded in same-origin frame</p>",
                ),
                "/download" => (
                    "application/octet-stream",
                    "Content-Disposition: attachment; filename=server-name.txt\r\n",
                    "image map keyboard download bytes",
                ),
                other => panic!("unexpected image-map keyboard request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .unwrap()
        .id
        .clone();

    session
        .action(SemanticAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("parent-image")
    );
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert!(targets.iter().any(|target| {
        !target.active && target.url == format!("http://{address}/parent-destination")
    }));

    let download_directory = std::env::temp_dir().join(format!(
        "glass-native-image-map-keyboard-download-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&download_directory).unwrap();
    session
        .script("document.getElementById('parent-download').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    let download = session
        .native_wait_for_download(&download_directory, Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(download.suggested_filename, "keyboard-map.txt");
    assert_eq!(
        fs::read(download_directory.join("keyboard-map.txt")).unwrap(),
        b"image map keyboard download bytes"
    );
    fs::remove_dir_all(&download_directory).unwrap();

    session.native_select_frame(&child_id).await.unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("frame-image")
    );
    session
        .script("globalThis.__frameKeyboardTrace = []; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/child")
    );
    assert_eq!(
        session
            .script("globalThis.__frameKeyboardTrace")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            "keydown:frame-image",
            "click:frame-area",
            "keyup:frame-image"
        ])
    );
    session
        .script("globalThis.__frameKeyboardTrace = []; globalThis.__cancelFrameClick = false; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/frame-destination")
    );

    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[test]
fn native_http_window_open_transfers_blob_document_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/blob-popup-parent")
        );
        let body = "<title>Remote Blob opener</title><script>window.open(URL.createObjectURL(new Blob(['<title>Remote Blob popup</title><p>remote popup body</p>'], { type: 'text/html' })), 'blob-report');</script><p>parent</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/blob-popup-parent")),
    )
    .await
    .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].opener_id.as_deref(), Some("native-context"));
    assert!(targets[1].url.starts_with("blob:http://"));
    assert_eq!(targets[1].title, "Remote Blob popup");

    session.native_select_target(&targets[1].id).await.unwrap();
    let evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(evidence.visible_text, "remote popup body");
    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[test]
fn native_http_window_open_crosses_content_worker_and_reuses_name() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/window-open-parent" => {
                    "<title>HTTP opener</title><script>window.open('/window-open-child', 'report');</script><p>parent</p>"
                }
                "/window-open-child" => "<title>HTTP first report</title><p>first</p>",
                "/window-open-child-next" => "<title>HTTP second report</title><p>second</p>",
                other => panic!("unexpected window.open request path: {other}"),
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
            .with_initial_url(format!("http://{address}/window-open-parent")),
    )
    .await
    .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].title, "HTTP first report");

    session
        .script("window.open('/window-open-child-next', 'report');")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].id, "native-target-1");
    assert_eq!(targets[1].title, "HTTP second report");
    assert!(targets[0].active);
    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[test]
fn native_http_post_message_crosses_content_worker_and_replies() {
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
                "/message-parent" => {
                    "<title>HTTP message parent</title><script>addEventListener('message', event => { globalThis.reply = event.data.reply; }); globalThis.portReply = []; const channel = new MessageChannel(); channel.port2.addEventListener('message', event => { portReply = [event.data.kind, event.data.value]; }); channel.port2.start(); const child = window.open('/message-child', 'message-child'); child.postMessage({ kind: 'worker-greeting', payload: 'x'.repeat(20000), port: channel.port1 }, '*', [channel.port1]);</script><p>parent</p>"
                }
                "/message-child" => {
                    "<script>addEventListener('message', event => { globalThis.received = [event.data.kind, event.data.payload.length, event.origin, Boolean(event.source), event.ports.length]; event.ports[0].postMessage({ kind: 'port-reply', value: 9 }); event.source.postMessage({ reply: event.data.kind }, '*'); });</script><title>HTTP message child</title><p>child</p>"
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
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/message-parent")),
    )
    .await
    .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child = targets.iter().find(|target| !target.active).unwrap();
    session.native_select_target(&child.id).await.unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([
            "worker-greeting",
            20000,
            format!("http://{address}"),
            true,
            1
        ])
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session
            .script("globalThis.reply || null")
            .await
            .unwrap()
            .value,
        serde_json::json!("worker-greeting")
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.portReply").await.unwrap().value,
        serde_json::json!(["port-reply", 9])
    );

    session.close().await.unwrap();
    server.await.unwrap();
        });
    });
}

#[test]
fn native_http_window_proxy_message_port_transfers_blob_urls_between_page_realms() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, body) in [
            (
                "/window-port-blob-parent",
                "<title>HTTP window port Blob parent</title><script>globalThis.replies = []; const channel = new MessageChannel(); channel.port2.onmessage = event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); replies.push([event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]); } catch (error) { replies.push(['error', String(error)]); } }; channel.port2.start(); globalThis.child = window.open('/window-port-blob-child', 'http-window-port-blob-child'); const blob = new Blob(['http-parent-window-port-body'], { type: 'text/http-window-parent' }); child.postMessage({ kind: 'http-parent-to-child', url: URL.createObjectURL(blob) }, '*', [channel.port1]);</script><p>parent</p>",
            ),
            (
                "/window-port-blob-child",
                "<title>HTTP window port Blob child</title><script>globalThis.received = []; addEventListener('message', event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); received.push([event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]); const blob = new Blob(['http-child-window-port-body'], { type: 'text/http-window-child' }); event.ports[0].postMessage({ kind: 'http-child-to-parent', url: URL.createObjectURL(blob) }); } catch (error) { received.push(['error', String(error)]); } });</script><p>child</p>",
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/window-port-blob-parent")),
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
        serde_json::json!([[
            "http-parent-to-child",
            "http-parent-window-port-body",
            "text/http-window-parent",
            format!("http://{address}"),
        ]])
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.replies").await.unwrap().value,
        serde_json::json!([[
            "http-child-to-parent",
            "http-child-window-port-body",
            "text/http-window-child",
            "",
        ]])
    );
    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[test]
fn native_http_window_proxy_identity_tracks_navigation_and_close() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/proxy-identity-parent" => {
                    "<title>HTTP proxy identity parent</title><script>globalThis.popup = window.open('/proxy-identity-child', 'proxy-identity-target');</script><p>parent</p>"
                }
                "/proxy-identity-child" => {
                    "<title>HTTP proxy identity child</title><script>globalThis.initialOpener = opener.location.href;</script><p>child</p>"
                }
                "/proxy-identity-parent-next" => {
                    "<title>HTTP proxy identity parent next</title><p>parent next</p>"
                }
                other => panic!("unexpected WindowProxy identity request path: {other}"),
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
            .with_initial_url(format!("http://{address}/proxy-identity-parent")),
    )
    .await
    .unwrap();
    let child_id = session
        .native_list_targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| !target.active)
        .unwrap()
        .id;
    session
        .navigate(format!("http://{address}/proxy-identity-parent-next"))
        .await
        .unwrap();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session
            .script("[opener.location.href, opener.closed]")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            format!("http://{address}/proxy-identity-parent-next"),
            false
        ])
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    session.native_close_target("native-context").await.unwrap();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session.script("opener.closed").await.unwrap().value,
        serde_json::json!(true)
    );
    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[test]
fn native_http_window_identity_crosses_content_worker() {
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
                "/identity-parent" => {
                    "<title>HTTP identity parent</title><script>globalThis.received = []; addEventListener('message', event => { globalThis.received.push([event.data.kind, Boolean(event.source), event.origin]); }); window.name = 'http-parent'; window.open('/identity-child', 'report');</script><p>parent</p>"
                }
                "/identity-child" => {
                    "<title>HTTP identity child</title><script>globalThis.observed = [window.name, window.opener !== null, window.opener.name]; window.opener.postMessage({ kind: 'child-ready' }, '*');</script><p>child</p>"
                }
                other => panic!("unexpected window identity request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/identity-parent")),
    )
    .await
    .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child_id = targets
        .iter()
        .find(|target| !target.active)
        .unwrap()
        .id
        .clone();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session.script("globalThis.observed").await.unwrap().value,
        serde_json::json!(["report", true, "http-parent"])
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([["child-ready", true, format!("http://{address}")]])
    );
    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[tokio::test]
async fn native_network_quiet_wait_reports_completed_request_activity() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/network-quiet"));
        let body = "<title>Quiet</title><p>request complete</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/network-quiet")),
    )
    .await
    .unwrap();
    server.await.unwrap();

    let outcome = session
        .native_wait(
            WaitCondition::NetworkQuiet(Duration::from_millis(25)),
            Duration::from_millis(200),
        )
        .await
        .unwrap();
    assert_eq!(outcome.condition, "network_quiet");
    assert!(outcome.last_state.contains("inFlight=0"));
    assert!(outcome.last_state.contains("completed=1"));
    session.close().await.unwrap();
}

#[tokio::test]
async fn native_webp_data_images_expose_intrinsic_dimensions_and_paint() {
    let source = native_test_small_webp_data_url();
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://webp-image/page")
        .with_fixture(
            "fixture://webp-image/page",
            format!("<img id='image' src='{source}' style='width:2px;height:2px'>"),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, 2, source])
    );

    let document = NativeDocument::parse(
        &format!(
            "<html><body><img id='image' src='{source}' style='width:2px;height:2px'></body></html>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let image_id = document.resolve_target("id=image").unwrap();
    let list = document
        .display_list(Viewport {
            width: 2,
            height: 2,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image {
                node_id,
                source_width: 2,
                source_height: 2,
                pixels,
                ..
            } if *node_id == image_id && pixels.len() == 2 * 2 * 4
        )
    }));
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn native_cookie_profile_survives_native_process_restart() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-cookie-restart.json",
        std::process::id()
    ));
    let cookie_profile_path =
        std::path::PathBuf::from(format!("{}.cookies", profile_path.display()));
    let lock_path = profile_path.with_extension("lock");
    let cookie_lock_path = cookie_profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&cookie_profile_path);
    let _ = fs::remove_file(&lock_path);
    let _ = fs::remove_file(&cookie_lock_path);
    let _ = fs::remove_file(&events_path);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for request_index in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/page"));
            let cookie_header = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            });
            if request_index == 0 {
                assert_eq!(cookie_header, None);
            } else {
                assert_eq!(cookie_header, Some("session=one; theme=dark"));
            }
            let body = "<p>Cookie profile</p>";
            let set_cookie = if request_index == 0 {
                "Set-Cookie: theme=dark; Path=/\r\n"
            } else {
                Default::default()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let url = format!("http://{address}/page");
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_initial_url(url);

    {
        let mut engine = NativeEngine::new(config.clone()).unwrap();
        engine.initialize_async().await.unwrap();
        assert_eq!(
            engine.evaluate_async("document.cookie").await.unwrap(),
            serde_json::json!("theme=dark")
        );
        engine
            .evaluate_async("document.cookie = 'session=one; Path=/'; true")
            .await
            .unwrap();
        engine.close_async().await.unwrap();
    }

    let storage_profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&profile_path).unwrap()).unwrap();
    let cookie_profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&cookie_profile_path).unwrap()).unwrap();
    assert_eq!(storage_profile["cookies"], serde_json::json!([]));
    assert_eq!(cookie_profile["cookies"].as_array().map(Vec::len), Some(2));

    {
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize_async().await.unwrap();
        assert_eq!(
            engine.evaluate_async("document.cookie").await.unwrap(),
            serde_json::json!("session=one; theme=dark")
        );
        engine.close_async().await.unwrap();
    }

    server.await.unwrap();
    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(cookie_profile_path);
    let _ = fs::remove_file(lock_path);
    let _ = fs::remove_file(cookie_lock_path);
    let _ = fs::remove_file(events_path);
}

#[test]
fn native_web_storage_profile_rejects_an_active_writer() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-active-writer.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&lock_path);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    lock.lock_exclusive().unwrap();

    let error = match NativeEngine::new(
        NativeEngineConfig::default().with_storage_path(profile_path.clone()),
    ) {
        Ok(_) => panic!("an active profile writer must reject a second reader"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        NativeEngineError::StorageProfileLocked { path } if path == profile_path.to_string_lossy()
    ));

    drop(lock);
    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(lock_path);
}

#[tokio::test]
async fn native_xhr_abort_is_observable_and_ignores_late_callbacks() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<p>XHR abort</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let config = NativeEngineConfig::default().with_initial_url(format!("http://{address}/page"));
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                const xhr = new XMLHttpRequest();
                const events = [];
                xhr.onreadystatechange = () => events.push(`state:${xhr.readyState}`);
                xhr.onload = () => events.push('load');
                xhr.onerror = () => events.push('error');
                xhr.onabort = event => events.push(`${event.type}:${xhr.readyState}`);
                xhr.open('GET', 'data:text/plain,never');
                xhr.send();
                xhr.abort();
                globalThis.xhrAbortResult = [xhr.readyState, xhr.status, xhr.responseText, xhr.responseURL, events];
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.xhrAbortResult")
            .await
            .unwrap(),
        serde_json::json!([0, 0, "", "", ["state:1", "state:0", "abort:0"]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn native_xhr_timeout_is_bounded_and_observable() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/slow"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/slow" {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let body = if expected_path == "/page" {
                "<p>XHR timeout</p>"
            } else {
                "late-response"
            };
            let content_type = if expected_path == "/page" {
                "text/html"
            } else {
                "text/plain"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes()).await;
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
                const xhr = new XMLHttpRequest();
                const events = [];
                xhr.onreadystatechange = () => events.push(`state:${xhr.readyState}`);
                xhr.onerror = () => events.push('error');
                xhr.ontimeout = event => {
                    events.push(`${event.type}:${xhr.readyState}`);
                    globalThis.xhrTimeoutResult = [xhr.timeout, xhr.readyState, xhr.status, xhr.responseText, xhr.responseURL, events];
                };
                xhr.open('GET', '/slow');
                xhr.timeout = 20;
                xhr.send();
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.xhrTimeoutResult")
            .await
            .unwrap(),
        serde_json::json!([20, 4, 0, "", "", ["state:1", "state:4", "timeout:4"]])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { try { const xhr = new XMLHttpRequest(); xhr.timeout = 4001; return 'accepted'; } catch (error) { return [error.name, error.message]; } })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["RangeError", "native XMLHttpRequest timeout is outside the bounded range"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

#[test]
fn native_http_generic_explicit_tabindex_focus_routes_through_same_origin_frame() {
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
                "/parent" => {
                    "<div id='parent-two' tabindex='2'></div><div id='parent-one' tabindex='1'></div><h2 id='parent-zero' tabindex='0'>Parent heading</h2><iframe id='child' src='/child'></iframe>"
                }
                "/child" => {
                    "<div id='frame-two' tabindex='2'></div><section id='frame-zero' tabindex='0'></section><div id='frame-negative' tabindex='-1'></div><div id='frame-invalid' tabindex='bad'></div><div id='frame-hidden' hidden tabindex='0'></div>"
                }
                other => panic!("unexpected generic tabindex request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    for expected in ["parent-one", "parent-two", "parent-zero"] {
        session
            .action(SemanticAction::Shortcut {
                shortcut: "Tab".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            session
                .script("document.activeElement.id")
                .await
                .unwrap()
                .value,
            serde_json::json!(expected)
        );
    }

    let frames = session.native_list_frames().await.unwrap();
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin child frame")
        .id
        .clone();
    session.native_select_frame(&child_id).await.unwrap();
    session
        .script(
            "globalThis.__genericFrameTrace = []; const target = document.getElementById('frame-negative'); for (const type of ['focus', 'keydown', 'keyup', 'click']) target.addEventListener(type, event => __genericFrameTrace.push(type + ':' + event.target.id + ':' + (event.key || ''))); target.focus(); document.activeElement.id",
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("frame-negative")
    );
    for expected in ["frame-two", "frame-zero"] {
        session
            .action(SemanticAction::Shortcut {
                shortcut: "Tab".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            session
                .script("document.activeElement.id")
                .await
                .unwrap()
                .value,
            serde_json::json!(expected)
        );
    }
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("frame-two")
    );
    session
        .script("globalThis.__genericFrameTrace = []; document.getElementById('frame-negative').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("globalThis.__genericFrameTrace")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            "focus:frame-negative:",
            "keydown:frame-negative:Enter",
            "keyup:frame-negative:Enter",
        ])
    );
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("frame-negative")
    );
    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[test]
fn native_http_form_associated_disabled_focus_state_persists_in_frame_owner() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let markup = native_form_associated_focus_markup();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native form-associated focus document request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = if path == "/parent" {
                "<iframe id='child' src='/child'></iframe>"
            } else if path == "/child" {
                markup
            } else {
                panic!("unexpected native form-associated focus request: {path}");
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
    let frames = session.native_list_frames().await.unwrap();
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin child frame")
        .id
        .clone();
    session.native_select_frame(&child_id).await.unwrap();
    session
        .script(native_form_associated_focus_definition())
        .await
        .unwrap();
    assert_eq!(
        session
            .script(
                "document.getElementById('zero-first').focus(); document.getElementById('own-disabled').focus(); document.getElementById('fieldset-disabled').focus(); document.activeElement.id",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!("zero-first")
    );
    for expected in ["enabled", "legend-enabled"] {
        if expected == "legend-enabled" {
            session
                .script("document.getElementById('enabled').setAttribute('disabled', '')")
                .await
                .unwrap();
        }
        session
            .action(SemanticAction::Shortcut {
                shortcut: "Tab".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            session
                .script("document.activeElement.id")
                .await
                .unwrap()
                .value,
            serde_json::json!(expected)
        );
    }
    session
        .script("document.getElementById('enabled').removeAttribute('disabled')")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Shift+Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("enabled")
    );
    for expected in ["legend-enabled", "ordinary-disabled"] {
        session
            .action(SemanticAction::Shortcut {
                shortcut: "Tab".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            session
                .script("document.activeElement.id")
                .await
                .unwrap()
                .value,
            serde_json::json!(expected)
        );
    }
    session.close().await.unwrap();
    server.await.unwrap();
        });
    });
}

#[test]
fn native_http_inert_focus_state_persists_in_same_origin_frame() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let markup = native_inert_focus_markup();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native inert-focus document request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = if path == "/parent" {
                "<iframe id='child' src='/child'></iframe>"
            } else if path == "/child" {
                markup
            } else {
                panic!("unexpected native inert-focus request: {path}");
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
    let frames = session.native_list_frames().await.unwrap();
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin child frame")
        .id
        .clone();
    session.native_select_frame(&child_id).await.unwrap();
    assert_eq!(
        session
            .script(
                "(() => { const panel = document.getElementById('panel'); const explicit = document.getElementById('explicit-blocked'); const implicit = document.getElementById('implicit-blocked'); document.getElementById('first').focus(); explicit.focus(); implicit.focus(); const initial = [panel.inert, panel.hasAttribute('inert'), explicit.inert, document.activeElement.id]; panel.inert = false; const removed = [panel.inert, panel.hasAttribute('inert')]; document.getElementById('first').focus(); return { initial, removed }; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "initial": [true, true, false, "first"],
            "removed": [false, false],
        })
    );
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("explicit-blocked")
    );
    assert_eq!(
        session
            .script(
                "(() => { const panel = document.getElementById('panel'); panel.inert = true; document.getElementById('explicit-blocked').focus(); document.getElementById('implicit-blocked').focus(); return [panel.inert, panel.hasAttribute('inert'), document.activeElement.id]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([true, true, "explicit-blocked"])
    );
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("first")
    );
    session.close().await.unwrap();
    server.await.unwrap();

        });
    });
}

#[test]
fn native_http_focusin_focusout_persist_in_same_origin_frame() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_http_focusin_focusout_persist_in_same_origin_frame_inner());
    });
}

async fn native_http_focusin_focusout_persist_in_same_origin_frame_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let markup = native_focus_event_markup();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native focus-event document request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = if path == "/parent" {
                "<iframe id='child' src='/child'></iframe>"
            } else if path == "/child" {
                markup
            } else {
                panic!("unexpected native focus-event request: {path}");
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
    let frames = session.native_list_frames().await.unwrap();
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin child frame")
        .id
        .clone();
    assert_eq!(
        session
            .script(&native_window_document_event_handler_surface_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([[], [], [], [], [], [], [], [], [], [true, true]])
    );
    assert_eq!(
        session
            .script(native_projected_window_document_event_handler_dispatch_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([
            [
                ["frame-body-load", true, true, true],
                ["frame-document-visibility", true, true, true],
            ],
            true,
            true,
            true,
            [true, true, [[true, true]], true],
        ])
    );
    assert_eq!(
        session
            .script(&native_html_global_event_handler_surface_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([[], [], []])
    );
    session
        .script(native_projected_focus_handler_setup_script())
        .await
        .unwrap();
    session.native_select_frame(&child_id).await.unwrap();
    assert_eq!(
        session
            .script(&native_window_document_event_handler_surface_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([[], [], [], [], [], [], [], [], [], [true, true]])
    );
    assert_eq!(
        session
            .script(native_window_document_event_handler_dispatch_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([
            [
                ["window-message", true, true, true],
                ["document-ready", true, true, true],
                ["document-visibility", true, true, true],
                ["body-load", true, true, true],
                ["frameset-afterprint", true, true, true],
                ["body-click", true, true, true],
            ],
            true,
            true,
            true,
            true,
        ])
    );
    assert_eq!(
        session
            .script(&native_html_global_event_handler_surface_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([[], [], []])
    );
    assert_eq!(
        session
            .script(native_global_event_handler_dispatch_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([[["click", true, true], ["input", true, true]], null, null,])
    );
    assert_eq!(
        session
            .script(native_focus_event_listener_script())
            .await
            .unwrap()
            .value,
        native_focus_transition_events()
    );
    assert_eq!(
        session.script("idlFocusEvents").await.unwrap().value,
        serde_json::json!([
            ["focus-first", true, true],
            ["blur-first", true, true],
            ["focus-second", true, true],
        ])
    );
    assert_eq!(
        session.script("clearedFocusHandlers").await.unwrap().value,
        serde_json::json!([null, null])
    );
    assert_eq!(
        session.script("idlFocusOrder").await.unwrap().value,
        serde_json::json!(["before", "handler", "after"])
    );
    session
        .native_select_frame("native-context:main")
        .await
        .unwrap();
    assert_eq!(
        session
            .script(native_projected_focus_handler_clear_script())
            .await
            .unwrap()
            .value,
        serde_json::json!([null, null])
    );
    session.native_select_frame(&child_id).await.unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session.script("focusEvents").await.unwrap().value,
        serde_json::json!([
            ["focus", "first", null, true],
            ["focusin", "first", null, true],
            ["blur", "first", "second", true],
            ["focusout", "first", "second", true],
            ["focus", "second", "first", true],
            ["focusin", "second", "first", true],
            ["blur", "second", "first", true],
            ["focusout", "second", "first", true],
            ["focus", "first", "second", true],
            ["focusin", "first", "second", true],
        ])
    );
    session
        .native_select_frame("native-context:main")
        .await
        .unwrap();
    assert_eq!(
        session.script("projectedFocusEvents").await.unwrap().value,
        serde_json::json!([
            ["focus", "first", null, true],
            ["focusin", "first", null, true],
            ["blur", "first", "second", true],
            ["focusout", "first", "second", true],
            ["focus", "second", "first", true],
            ["focusin", "second", "first", true],
            ["blur", "second", "first", true],
            ["focusout", "second", "first", true],
            ["focus", "first", "second", true],
            ["focusin", "first", "second", true],
        ])
    );
    assert_eq!(
        session
            .script("projectedIdlFocusEvents")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            ["focus-first", true, true],
            ["blur-first", true, true],
            ["focus-second", true, true],
        ])
    );
    assert_eq!(
        session
            .script(native_projected_global_handler_dispatch_script())
            .await
            .unwrap()
            .value,
        serde_json::json!(["function", null, [[true, true]],])
    );
    session.native_select_frame(&child_id).await.unwrap();
    assert_eq!(
        session.script("idlFocusOrder").await.unwrap().value,
        serde_json::json!(["before", "handler", "after", "before", "after"])
    );
    session.close().await.unwrap();
    server.await.unwrap();
}

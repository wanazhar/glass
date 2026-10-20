use super::*;

#[tokio::test]
async fn native_content_process_updates_service_worker_registration() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/page",
                "text/html",
                "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>update page</main></body></html>",
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  event.respondWith(new Response('worker v1', { status: 200, headers: { 'X-Worker-Version': 'v1' } }));
});"#,
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  event.respondWith(new Response('worker v2', { status: 201, headers: { 'X-Worker-Version': 'v2' } }));
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
                "await registrationPromise.then(reg => [reg.active.state, reg.active.scriptURL])",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", format!("http://{address}/sw.js")])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => reg.update().then(updated => [updated === undefined, reg.active.state, reg.active.scriptURL]))",
            )
            .await
            .unwrap(),
        serde_json::json!([true, "activated", format!("http://{address}/sw.js")])
    );
    engine
        .navigate_async(format!("http://{address}/controlled"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/version').then(async response => ({ status: response.status, version: response.headers.get('x-worker-version'), body: await response.text() }))",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "status": 201,
            "version": "v2",
            "body": "worker v2",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_observes_service_worker_lifecycle_update() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/page",
                "text/html",
                "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>lifecycle page</main></body></html>",
            ),
            (
                "/sw.js",
                "application/javascript",
                "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim())); self.addEventListener('fetch', event => event.respondWith(new Response('worker v1')));",
            ),
            (
                "/sw.js",
                "application/javascript",
                "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim())); self.addEventListener('fetch', event => event.respondWith(new Response('worker v2')));",
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
                "await registrationPromise.then(async reg => { const events = []; let controllerChanges = 0; navigator.serviceWorker.addEventListener('controllerchange', () => controllerChanges++); const oldActive = reg.active; oldActive.addEventListener('statechange', () => events.push('active:' + oldActive.state)); reg.addEventListener('updatefound', () => { const installing = reg.installing; events.push('updatefound:' + installing.state); installing.addEventListener('statechange', () => events.push('installing:' + installing.state)); }); const result = await reg.update(); return { updateUndefined: result === undefined, active: reg.active && reg.active.state, waiting: reg.waiting, installing: reg.installing, events, controllerChanges }; })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "updateUndefined": true,
            "active": "activated",
            "waiting": null,
            "installing": null,
            "events": [
                "updatefound:installing",
                "installing:installed",
                "installing:activating",
                "installing:activated",
                "active:redundant",
            ],
            "controllerChanges": 1,
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_keeps_service_worker_update_waiting_until_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/page",
                "text/html",
                "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>waiting page</main></body></html>",
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => { const path = new URL(event.request.url).pathname; event.respondWith(path === '/next' ? new Response('<!doctype html><html><body>worker v1</body></html>', { headers: { 'Content-Type': 'text/html', 'X-Worker-Version': 'v1' } }) : new Response('worker v1', { headers: { 'X-Worker-Version': 'v1' } })); });
"#,
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => event.waitUntil(Promise.resolve()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => { const path = new URL(event.request.url).pathname; event.respondWith(path === '/next' ? new Response('<!doctype html><html><body>worker v2</body></html>', { headers: { 'Content-Type': 'text/html', 'X-Worker-Version': 'v2' } }) : new Response('worker v2', { headers: { 'X-Worker-Version': 'v2' } })); });
"#,
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
                "await registrationPromise.then(async reg => { const events = []; reg.addEventListener('updatefound', () => { const installing = reg.installing; events.push('updatefound:' + installing.state); installing.addEventListener('statechange', () => events.push('installing:' + installing.state)); }); const result = await reg.update(); return { updateUndefined: result === undefined, active: reg.active && reg.active.state, waiting: reg.waiting && reg.waiting.state, installing: reg.installing, registrations: (await navigator.serviceWorker.getRegistrations()).length, events }; })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "updateUndefined": true,
            "active": "activated",
            "waiting": "installed",
            "installing": null,
            "registrations": 1,
            "events": ["updatefound:installing", "installing:installed"],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/version').then(async response => ({ version: response.headers.get('x-worker-version'), body: await response.text() }))",
            )
            .await
            .unwrap(),
        serde_json::json!({"version":"v1","body":"worker v1"})
    );
    engine
        .navigate_async(format!("http://{address}/next"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ body: document.body.innerText, controlled: navigator.serviceWorker.controller !== null, active: navigator.serviceWorker.controller && navigator.serviceWorker.controller.state, version: await fetch('/version').then(async response => ({ version: response.headers.get('x-worker-version'), body: await response.text() })) })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "body": "worker v2",
            "controlled": true,
            "active": "activated",
            "version": {"version":"v2","body":"worker v2"},
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_persists_service_worker_cache_across_restart() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-cache-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }

    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            (
                "/register",
                "text/html",
                "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>cache registration page</main></body></html>",
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => {
  event.waitUntil(caches.open('v1').then(async cache => {
    await cache.put('/durable', new Response('persisted cache value', {
      status: 200,
      headers: { 'Content-Type': 'text/plain', 'X-Cache': 'durable' },
    }));
    const keys = await cache.keys();
    const has = await caches.has('v1');
    await cache.put('/cache-diagnostics', new Response(JSON.stringify({ keys: keys.length, has }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }));
    return self.skipWaiting();
  }));
});
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  event.respondWith(caches.match(event.request).then(async cached => {
    if (cached) return cached;
    if (event.request.mode === 'navigate') {
      const preload = await event.preloadResponse;
      if (preload) return preload;
    }
    return fetch(event.request);
  }));
});"#,
            ),
            (
                "/controlled",
                "text/html",
                "<!doctype html><html><body><main>controlled navigation</main></body></html>",
            ),
            (
                "/sw.js",
                "application/javascript",
                r#"self.addEventListener('install', event => {
  event.waitUntil(caches.open('v1').then(async cache => {
    await cache.put('/durable', new Response('persisted cache value', {
      status: 200,
      headers: { 'Content-Type': 'text/plain', 'X-Cache': 'durable' },
    }));
    const keys = await cache.keys();
    const has = await caches.has('v1');
    await cache.put('/cache-diagnostics', new Response(JSON.stringify({ keys: keys.length, has }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }));
    return self.skipWaiting();
  }));
});
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  event.respondWith(caches.match(event.request).then(async cached => {
    if (cached) return cached;
    if (event.request.mode === 'navigate') {
      const preload = await event.preloadResponse;
      if (preload) return preload;
    }
    return fetch(event.request);
  }));
});"#,
            ),
            (
                "/reopen",
                "text/html",
                "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.ready; globalThis.restoredController = navigator.serviceWorker.controller !== null;</script><main>cache reopen page</main></body></html>",
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
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async reg => { await reg.navigationPreload.enable(); await reg.navigationPreload.setHeaderValue('cache-preview'); return reg.navigationPreload.getState(); })",
            )
            .await
            .unwrap(),
        serde_json::json!({"enabled": true, "headerValue": "cache-preview"})
    );
    engine
        .navigate_async(format!("http://{address}/controlled"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await fetch('/cache-diagnostics').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!(r#"{"keys":1,"has":true}"#)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/durable').then(async response => ({ status: response.status, cache: response.headers.get('x-cache'), body: await response.text() }))",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "status": 200,
            "cache": "durable",
            "body": "persisted cache value",
        })
    );
    engine.close_async().await.unwrap();
    let profile = fs::read_to_string(&profile_path).unwrap();
    assert!(profile.contains("service_worker_caches"));
    assert!(profile.contains("service_worker_registrations"));
    assert!(profile.contains("navigation_preload_enabled"));
    assert!(profile.contains("cache-preview"));

    let mut reopened = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/reopen"))
            .with_storage_path(profile_path.clone()),
    )
    .unwrap();
    reopened.initialize_async().await.unwrap();
    assert_eq!(
        reopened
            .evaluate_async(
                "await registrationPromise.then(async reg => [restoredController, reg.active.state, reg.active.scriptURL, await reg.navigationPreload.getState()])",
            )
            .await
            .unwrap(),
        serde_json::json!([
            true,
            "activated",
            format!("http://{address}/sw.js"),
            {"enabled": true, "headerValue": "cache-preview"}
        ])
    );
    assert_eq!(
        reopened
            .evaluate_async(
                "await fetch('/durable').then(async response => ({ status: response.status, cache: response.headers.get('x-cache'), body: await response.text() }))",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "status": 200,
            "cache": "durable",
            "body": "persisted cache value",
        })
    );
    reopened.close_async().await.unwrap();
    server.await.unwrap();

    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_content_process_persists_waiting_service_worker_across_restart() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-waiting-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }

    let register_page = "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/app/' });</script><main>waiting persistence</main></body></html>";
    let reopen_page = "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.getRegistration('/app/');</script><main>reopened waiting persistence</main></body></html>";
    let active_script = r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => event.respondWith(new Response('worker-v1')));"#;
    let waiting_script = r#"self.addEventListener('install', event => event.waitUntil(Promise.resolve()));
self.addEventListener('activate', event => event.waitUntil(Promise.resolve()));
self.addEventListener('fetch', event => { const path = new URL(event.request.url).pathname; event.respondWith(new Response(path === '/app/next' ? '<!doctype html><html><body>worker-v2</body></html>' : 'worker-v2', { headers: { 'Content-Type': path === '/app/next' ? 'text/html' : 'text/plain', 'X-Worker-Version': 'v2' } })); });"#;
    let server = tokio::spawn(async move {
        for (expected_path, content_type, body) in [
            ("/register", "text/html", register_page),
            ("/sw.js", "application/javascript", active_script),
            ("/sw.js", "application/javascript", waiting_script),
            ("/sw.js", "application/javascript", active_script),
            ("/sw.js", "application/javascript", waiting_script),
            ("/reopen", "text/html", reopen_page),
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
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/register"))
            .with_storage_path(profile_path.clone()),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async reg => { await reg.update(); return { active: reg.active.scriptURL, waiting: reg.waiting && reg.waiting.scriptURL }; })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "active": format!("http://{address}/sw.js"),
            "waiting": format!("http://{address}/sw.js"),
        })
    );
    engine.close_async().await.unwrap();
    let profile = fs::read_to_string(&profile_path).unwrap();
    assert!(profile.contains("\"waiting\""));

    let mut reopened = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/reopen"))
            .with_storage_path(profile_path.clone()),
    )
    .unwrap();
    reopened.initialize_async().await.unwrap();
    assert_eq!(
        reopened
            .evaluate_async(
                "await registrationPromise.then(reg => ({ active: reg.active && reg.active.scriptURL, activeState: reg.active && reg.active.state, waiting: reg.waiting && reg.waiting.scriptURL, waitingState: reg.waiting && reg.waiting.state, controlled: navigator.serviceWorker.controller !== null }))",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "active": format!("http://{address}/sw.js"),
            "activeState": "activated",
            "waiting": format!("http://{address}/sw.js"),
            "waitingState": "installed",
            "controlled": false,
        })
    );
    reopened
        .navigate_async(format!("http://{address}/app/next"))
        .await
        .unwrap();
    assert_eq!(
        reopened
            .evaluate_async(
                "await (async () => ({ body: document.body.innerText, controller: navigator.serviceWorker.controller && navigator.serviceWorker.controller.state, waiting: await navigator.serviceWorker.getRegistration('/app/').then(reg => reg && reg.waiting) }))()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "body": "worker-v2",
            "controller": "activated",
            "waiting": null,
        })
    );
    reopened.close_async().await.unwrap();
    server.await.unwrap();

    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}

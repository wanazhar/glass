use super::*;

#[tokio::test]
async fn native_content_process_service_worker_module_registration_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (index, expected_path) in ["/page", "/sw.js", "/sw-dep.js", "/sw.js", "/sw-dep.js"]
            .into_iter()
            .enumerate()
        {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(45), listener.accept())
                .await
                .expect("Service Worker script request should reach parent-owned network")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            });
            let expected_cookie = match index {
                0 => None,
                1 => Some("a_page=seed"),
                2 => Some("a_page=seed; b_entry=loaded"),
                3 => Some("a_page=seed; b_entry=loaded; c_dependency=loaded"),
                4 => Some("a_page=seed; b_entry=loaded; c_dependency=loaded"),
                _ => unreachable!(),
            };
            assert_eq!(cookie, expected_cookie, "cookie on {expected_path}");
            let (content_type, set_cookie, body) = match expected_path {
                "/page" => (
                    "text/html",
                    "Set-Cookie: a_page=seed; HttpOnly; Path=/\r\n",
                    "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/', type: 'module' }); globalThis.visibleAtRegistration = document.cookie;</script><main>parent cookies</main>",
                ),
                "/sw.js" => (
                    "application/javascript",
                    "Set-Cookie: b_entry=loaded; HttpOnly; Path=/\r\n",
                    "import { dependency } from './sw-dep.js'; self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('fetch', event => {});",
                ),
                "/sw-dep.js" => (
                    "application/javascript",
                    "Set-Cookie: c_dependency=loaded; HttpOnly; Path=/\r\n",
                    "export const dependency = 'loaded';",
                ),
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
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("await globalThis.registrationPromise.then(() => 'registered')")
            .await
            .unwrap(),
        serde_json::json!("registered")
    );
    assert_eq!(
        engine
            .evaluate_async("await globalThis.registrationPromise.then(registration => registration.update()).then(() => 'updated')")
            .await
            .unwrap(),
        serde_json::json!("updated")
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.visibleAtRegistration")
            .await
            .unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    for name in ["a_page", "b_entry", "c_dependency"] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only)
        );
    }
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_service_worker_lifecycle_fetches_use_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/sw.js",
            "/install-fetch",
            "/activate-fetch",
            "/after",
        ] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("Service Worker lifecycle Fetch should reach the parent network")
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
                "/page" => (
                    "Set-Cookie: sw_lifecycle_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    "<!doctype html><script>document.cookie = 'sw_lifecycle_turn=present; Path=/'; globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>lifecycle cookies</main>",
                ),
                "/sw.js" => {
                    assert!(cookie.contains("sw_lifecycle_seed=initial"));
                    assert!(cookie.contains("sw_lifecycle_turn=present"));
                    (
                        "",
                        "text/javascript",
                        r#"self.addEventListener('install', event => {
  event.waitUntil(fetch('/install-fetch').then(response => response.text()));
  event.waitUntil(self.skipWaiting());
});
self.addEventListener('activate', event => {
  event.waitUntil(fetch('/activate-fetch').then(response => response.text()));
  event.waitUntil(self.clients.claim());
});"#,
                    )
                }
                "/install-fetch" => {
                    assert!(cookie.contains("sw_lifecycle_seed=initial"));
                    assert!(cookie.contains("sw_lifecycle_turn=present"));
                    (
                        "Set-Cookie: sw_lifecycle_install=done; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/plain",
                        "installed",
                    )
                }
                "/activate-fetch" => {
                    assert!(cookie.contains("sw_lifecycle_seed=initial"));
                    assert!(cookie.contains("sw_lifecycle_turn=present"));
                    assert!(cookie.contains("sw_lifecycle_install=done"));
                    (
                        "Set-Cookie: sw_lifecycle_activate=done; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/plain",
                        "activated",
                    )
                }
                "/after" => {
                    assert!(cookie.contains("sw_lifecycle_seed=initial"));
                    assert!(cookie.contains("sw_lifecycle_turn=present"));
                    assert!(cookie.contains("sw_lifecycle_install=done"));
                    assert!(cookie.contains("sw_lifecycle_activate=done"));
                    ("", "text/plain", "after")
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
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async registration => ({ active: registration.active.state, controlled: navigator.serviceWorker.controller !== null }))",
            )
            .await
            .unwrap(),
        serde_json::json!({"active":"activated","controlled":true})
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/after').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("after")
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("sw_lifecycle_turn=present")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    server.await.unwrap();
    for name in [
        "sw_lifecycle_seed",
        "sw_lifecycle_install",
        "sw_lifecycle_activate",
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only)
        );
    }
}
#[tokio::test]
async fn native_content_process_waiting_service_worker_navigation_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-waiting-service-worker-navigation-cookies-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
    let origin = format!("http://{address}");
    let profile = serde_json::json!({
        "version": 1,
        "revision": 0,
        "local": {},
        "cookies": [{
            "name": "sw_nav_seed",
            "value": "initial",
            "domain": "127.0.0.1",
            "path": "/",
            "host_only": true,
            "secure": false,
            "http_only": true,
            "same_site": "Lax",
            "priority": null,
            "expires_at_unix_seconds": null
        }],
        "service_worker_registrations": [{
            "script_url": format!("{origin}/sw-active.js"),
            "scope": format!("{origin}/destination/"),
            "worker_type": "classic",
            "navigation_preload_enabled": true,
            "navigation_preload_header_value": "glass-nav-preload",
            "waiting": {
                "script_url": format!("{origin}/sw-next.js"),
                "worker_type": "classic"
            }
        }]
    });
    fs::write(&profile_path, serde_json::to_vec(&profile).unwrap()).unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/sw-active.js",
            "/sw-next.js",
            "/page",
            "/flush",
            "/activate-fetch",
            "/destination/page",
            "/nav-fetch",
            "/after",
        ] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("navigation Service Worker Fetch should reach the parent network")
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
                "/sw-active.js" => {
                    assert!(cookie.contains("sw_nav_seed=initial"));
                    (
                        "",
                        "text/javascript",
                        "self.addEventListener('fetch', event => {});",
                    )
                }
                "/sw-next.js" => {
                    assert!(cookie.contains("sw_nav_seed=initial"));
                    (
                        "",
                        "text/javascript",
                        r#"self.addEventListener('activate', event => {
  event.waitUntil(fetch('/activate-fetch').then(response => response.text()));
  event.waitUntil(self.clients.claim());
});
self.addEventListener('fetch', event => {
  if (event.request.mode !== 'navigate') return;
  event.respondWith((async () => {
    const preload = await event.preloadResponse;
    const preloadText = preload ? await preload.text() : 'missing-preload';
    const response = await fetch('/nav-fetch');
    const fetchText = await response.text();
    return new Response(`<!doctype html><main>${preloadText}:${fetchText}</main>`, {
      headers: {'Content-Type': 'text/html'}
    });
  })());
});"#,
                    )
                }
                "/page" => {
                    assert!(cookie.contains("sw_nav_seed=initial"));
                    (
                        "",
                        "text/html",
                        "<!doctype html><script>document.cookie = 'sw_nav_turn=present; Path=/';</script><main>navigation cookies</main>",
                    )
                }
                "/flush" => {
                    assert!(cookie.contains("sw_nav_seed=initial"));
                    assert!(cookie.contains("sw_nav_turn=present"));
                    ("", "text/plain", "flushed")
                }
                "/activate-fetch" => {
                    assert!(cookie.contains("sw_nav_seed=initial"));
                    assert!(cookie.contains("sw_nav_turn=present"));
                    (
                        "Set-Cookie: sw_nav_activate=done; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/plain",
                        "activated",
                    )
                }
                "/destination/page" => {
                    assert!(cookie.contains("sw_nav_seed=initial"));
                    assert!(cookie.contains("sw_nav_turn=present"));
                    assert!(cookie.contains("sw_nav_activate=done"));
                    assert!(request.lines().any(|line| {
                        line.to_ascii_lowercase()
                            .starts_with("service-worker-navigation-preload: glass-nav-preload")
                    }));
                    (
                        "Set-Cookie: sw_nav_preload=done; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/plain",
                        "preloaded",
                    )
                }
                "/nav-fetch" => {
                    for expected_cookie in [
                        "sw_nav_seed=initial",
                        "sw_nav_turn=present",
                        "sw_nav_activate=done",
                        "sw_nav_preload=done",
                    ] {
                        assert!(
                            cookie.contains(expected_cookie),
                            "missing {expected_cookie}"
                        );
                    }
                    (
                        "Set-Cookie: sw_nav_fetch=done; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/plain",
                        "fetched",
                    )
                }
                "/after" => {
                    for expected_cookie in [
                        "sw_nav_seed=initial",
                        "sw_nav_turn=present",
                        "sw_nav_activate=done",
                        "sw_nav_preload=done",
                        "sw_nav_fetch=done",
                    ] {
                        assert!(
                            cookie.contains(expected_cookie),
                            "missing {expected_cookie}"
                        );
                    }
                    ("", "text/plain", "after")
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
            .with_initial_url(format!("http://{address}/page"))
            .with_storage_path(profile_path.clone()),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("await fetch('/flush').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("flushed")
    );
    engine
        .navigate_async(format!("http://{address}/destination/page"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.innerText")
            .await
            .unwrap(),
        serde_json::json!("preloaded:fetched")
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("sw_nav_turn=present")
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/after').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("after")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    server.await.unwrap();
    for name in [
        "sw_nav_seed",
        "sw_nav_activate",
        "sw_nav_preload",
        "sw_nav_fetch",
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only),
            "parent cookie jar should retain {name} as HttpOnly"
        );
    }
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_content_process_service_worker_restoration_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-restoration-cookies-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }

    let server = tokio::spawn(async move {
        let worker_script = "import { dependency } from './sw-dep.js'; self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('fetch', event => {});";
        let responses = [
            (
                "/register",
                None,
                "text/html",
                "Set-Cookie: a_page=seed; HttpOnly; Path=/\r\n",
                "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/', type: 'module' });</script><main>registration</main>",
            ),
            (
                "/sw.js",
                Some("a_page=seed"),
                "application/javascript",
                "Set-Cookie: b_entry=loaded; HttpOnly; Path=/\r\n",
                worker_script,
            ),
            (
                "/sw-dep.js",
                Some("a_page=seed; b_entry=loaded"),
                "application/javascript",
                "Set-Cookie: c_dependency=loaded; HttpOnly; Path=/\r\n",
                "export const dependency = 'loaded';",
            ),
            (
                "/sw.js",
                Some("a_page=seed; b_entry=loaded; c_dependency=loaded"),
                "application/javascript",
                "Set-Cookie: d_restore_entry=loaded; HttpOnly; Path=/\r\n",
                worker_script,
            ),
            (
                "/sw-dep.js",
                Some("a_page=seed; b_entry=loaded; c_dependency=loaded; d_restore_entry=loaded"),
                "application/javascript",
                "Set-Cookie: e_restore_dependency=loaded; HttpOnly; Path=/\r\n",
                "export const dependency = 'restored';",
            ),
            (
                "/reopen",
                Some(
                    "a_page=seed; b_entry=loaded; c_dependency=loaded; d_restore_entry=loaded; e_restore_dependency=loaded",
                ),
                "text/html",
                "",
                "<!doctype html><main>restored</main>",
            ),
        ];
        for (expected_path, expected_cookie, content_type, set_cookie, body) in responses {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(45), listener.accept())
                .await
                .expect("persisted Service Worker resources should reach the parent")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            });
            assert_eq!(cookie, expected_cookie, "cookie on {expected_path}");
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
            .evaluate_async("await registrationPromise.then(() => 'registered')")
            .await
            .unwrap(),
        serde_json::json!("registered")
    );
    engine.close_async().await.unwrap();

    let mut reopened = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/reopen"))
            .with_storage_path(profile_path.clone()),
    )
    .unwrap();
    reopened.initialize_async().await.unwrap();
    assert_eq!(
        reopened.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = reopened.cookies_async().await.unwrap();
    for name in [
        "a_page",
        "b_entry",
        "c_dependency",
        "d_restore_entry",
        "e_restore_dependency",
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only),
            "parent cookie jar should contain {name}"
        );
    }
    reopened.close_async().await.unwrap();
    server.await.unwrap();

    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_content_process_rejects_service_worker_csp_failures_without_exiting() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut paths = Vec::new();
        let mut update_script_requests = 0usize;
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default().to_owned();
                    paths.push(path.clone());
                    let (content_type, policy_header, body) = match path.as_str() {
                        "/service-worker-csp" => (
                            "text/html",
                            "Content-Security-Policy: script-src 'unsafe-inline'; worker-src 'self'\r\n",
                            "<script>globalThis.registrationState='pending';globalThis.updateState='pending';globalThis.updatePreserved=false;globalThis.pageAlive=42;navigator.serviceWorker.register('/blocked/sw.js',{scope:'/blocked/',type:'module'}).then(()=>registrationState='resolved',error=>registrationState='rejected:'+error.message);navigator.serviceWorker.register('/update/sw.js',{scope:'/update/'}).then(registration=>{globalThis.updateState='registered';registration.update().then(()=>updateState='updated',error=>{globalThis.updatePreserved=Boolean(registration.active&&registration.active.scriptURL===new URL('/update/sw.js',location.href).href&&registration.active.state==='activated');globalThis.updateState='rejected:'+error.message;});},error=>updateState='register-rejected:'+error.message);</script><main>worker policy test</main>",
                        ),
                        "/blocked/sw.js" => (
                            "application/javascript",
                            "Content-Security-Policy: worker-src 'self'; script-src 'none'\r\n",
                            "import { blocked } from './dependency.js'; self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));",
                        ),
                        "/blocked/dependency.js" => (
                            "application/javascript",
                            "",
                            "export const blocked = true;",
                        ),
                        "/update/sw.js" if update_script_requests == 0 => {
                            update_script_requests += 1;
                            (
                                "application/javascript",
                                "",
                                "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));",
                            )
                        }
                        "/update/sw.js" => {
                            update_script_requests += 1;
                            (
                                "application/javascript",
                                "Content-Security-Policy: worker-src 'self'; script-src 'none'\r\n",
                                "importScripts('/update/denied.js'); self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));",
                            )
                        }
                        "/update/denied.js" => (
                            "application/javascript",
                            "",
                            "globalThis.deniedUpdateDependencyLoaded = true;",
                        ),
                        _ => ("text/plain", "", "unexpected resource"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{policy_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
        paths
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/service-worker-csp")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let mut observations = serde_json::Value::Null;
    for _ in 0..500 {
        observations = engine
            .evaluate_async("[registrationState,updateState,updatePreserved,pageAlive]")
            .await
            .unwrap();
        let registration_finished = observations[0]
            .as_str()
            .is_some_and(|state| state.starts_with("rejected:"));
        let update_finished = observations[1]
            .as_str()
            .is_some_and(|state| state.starts_with("rejected:"));
        if registration_finished && update_finished {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    engine.close_async().await.unwrap();
    shutdown_sender.send(()).unwrap();
    let paths = server.await.unwrap();

    assert!(
        observations[0]
            .as_str()
            .is_some_and(|state| state.starts_with("rejected:")),
        "blocked module Service Worker registration must reject: {observations:?}; HTTP paths: {paths:?}"
    );
    assert!(
        observations[1]
            .as_str()
            .is_some_and(|state| state.starts_with("rejected:")),
        "blocked Service Worker update must reject: {observations:?}; HTTP paths: {paths:?}"
    );
    assert_eq!(
        observations[2], true,
        "the old active worker must survive the failed update"
    );
    assert_eq!(
        observations[3], 42,
        "the page content process must remain usable"
    );
    assert!(paths.contains(&"/blocked/sw.js".to_owned()));
    assert!(!paths.contains(&"/blocked/dependency.js".to_owned()));
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_str() == "/update/sw.js")
            .count(),
        2,
        "registration plus failed update must both load the script: {paths:?}"
    );
    assert!(!paths.contains(&"/update/denied.js".to_owned()));
}
#[tokio::test]
async fn native_content_process_service_worker_stream_upload_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/sw.js", "/upload", "/after"] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("parent-brokered Service Worker upload should reach the local server")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned())
                .unwrap_or_default();
            let (headers, content_type, body) = match expected_path {
                "/page" => (
                    "Set-Cookie: sw_parent_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>Service Worker upload cookies</main>",
                ),
                "/sw.js" => {
                    assert!(cookie.contains("sw_parent_seed=initial"));
                    (
                        "",
                        "text/javascript",
                        r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/trigger') {
    event.respondWith((async () => {
      const body = new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('sw-owned-stream'));
          controller.close();
        },
      });
      const uploaded = await fetch('/upload', {
        method: 'POST',
        headers: { 'Content-Type': 'text/plain' },
        body,
      });
      return new Response(await uploaded.text());
    })());
  }
});"#,
                    )
                }
                "/upload" => {
                    assert!(request.starts_with("POST "));
                    assert!(request.ends_with("sw-owned-stream"));
                    assert!(cookie.contains("sw_parent_seed=initial"));
                    assert!(cookie.contains("sw_turn=present"));
                    (
                        "Set-Cookie: sw_parent_rotated=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/plain",
                        "uploaded",
                    )
                }
                "/after" => {
                    assert!(cookie.contains("sw_parent_seed=initial"));
                    assert!(cookie.contains("sw_parent_rotated=accepted"));
                    assert!(cookie.contains("sw_turn=present"));
                    ("", "text/plain", "after")
                }
                _ => unreachable!(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                "await registrationPromise.then(async () => { document.cookie = 'sw_turn=present; Path=/'; const uploaded = await fetch('/trigger').then(response => response.text()); const visible = document.cookie; const after = await fetch('/after').then(response => response.text()); return { controlled: navigator.serviceWorker.controller !== null, uploaded, visible, after }; })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "controlled": true,
            "uploaded": "uploaded",
            "visible": "sw_turn=present",
            "after": "after",
        })
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    server.await.unwrap();
    for (name, value) in [
        ("sw_parent_seed", "initial"),
        ("sw_parent_rotated", "accepted"),
        ("sw_turn", "present"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.value == value)
        );
    }
}

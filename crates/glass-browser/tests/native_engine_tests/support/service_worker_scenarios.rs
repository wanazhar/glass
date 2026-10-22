use super::*;

pub(super) async fn native_runtime_service_worker_fetch_open_window_resumes_navigation_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let observed_requests = Arc::new(Mutex::new(Vec::new()));
    let server_observed_requests = Arc::clone(&observed_requests);
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default();
                    let cookie = request
                        .lines()
                        .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                        .and_then(|line| line.split_once(':'))
                        .map(|(_, value)| value.trim().to_owned());
                    server_observed_requests
                        .lock()
                        .await
                        .push((path.to_owned(), cookie));
                    let (headers, content_type, body) = match path {
                        "/register" => (
                            concat!(
                                "Set-Cookie: parent_session=initial; Path=/; SameSite=Lax\r\n",
                                "Set-Cookie: parent_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                            ),
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>register</main>",
                        ),
                        "/sw.js" => (
                            "",
                            "application/javascript",
                            r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/suspended') {
    event.respondWith((async () => {
      const client = await clients.openWindow('/opened');
      const response = await fetch('/message-fetch', { credentials: 'include' });
      const text = await response.text();
      return new Response('<!doctype html><html><body><main id="resumed">' + (client ? client.url : 'missing') + ':' + text + '</main></body></html>', {
        headers: { 'Content-Type': 'text/html' },
      });
    })());
  }
});"#,
                        ),
                        "/message-fetch" => (
                            "Set-Cookie: resumed_worker=parent-owned; HttpOnly; Path=/; SameSite=Lax\r\n",
                            "text/plain",
                            "resumed fetch response",
                        ),
                        "/after" => ("", "text/plain", "after"),
                        "/opened" => (
                            "",
                            "text/html",
                            "<!doctype html><html><body><main>opened window</main></body></html>",
                        ),
                        "/suspended" => (
                            "",
                            "text/plain",
                            "network fallback (service worker did not resume)",
                        ),
                        _ => ("", "text/plain", "unexpected native service worker request"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/register")),
    )
    .await
    .unwrap();
    assert_eq!(
        session
            .script("await registrationPromise.then(reg => [reg.active.state, navigator.serviceWorker.controller !== null])")
            .await
            .unwrap()
            .value,
        serde_json::json!(["activated", true])
    );

    let navigation = session
        .navigate(format!("http://{address}/suspended"))
        .await
        .unwrap();
    assert_eq!(navigation.url, format!("http://{address}/suspended"));
    let resumed = session
        .script("({ href: location.href, body: document.body && document.body.innerText, html: document.documentElement && document.documentElement.outerHTML })")
        .await
        .unwrap()
        .value;
    assert_eq!(
        resumed,
        serde_json::json!({
            "href": format!("http://{address}/suspended"),
            "body": format!("http://{address}/opened:resumed fetch response"),
            "html": format!("<html><head></head><body><main id=\"resumed\">http://{address}/opened:resumed fetch response</main></body></html>"),
        })
    );
    assert_eq!(
        session
            .script("await fetch('/after').then(response => response.text())")
            .await
            .unwrap()
            .value,
        serde_json::json!("after")
    );
    let visible_cookies = session.script("document.cookie").await.unwrap().value;
    assert!(
        !visible_cookies
            .as_str()
            .unwrap_or_default()
            .contains("resumed_worker")
    );
    let cookies = session.native_cookies().await.unwrap();
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "resumed_worker" && cookie.value == "parent-owned" && cookie.http_only
    }));
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .any(|target| { target.url == format!("http://{address}/opened") && !target.active })
    );

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    let requests = observed_requests.lock().await;
    let resumed_fetch = requests
        .iter()
        .find(|(path, _)| path == "/message-fetch")
        .expect("resumed ServiceWorker FetchEvent request should reach the server");
    let resumed_cookie = resumed_fetch.1.as_deref().unwrap_or_default();
    assert!(
        resumed_cookie.contains("parent_session=initial"),
        "{requests:?}"
    );
    assert!(resumed_cookie.contains("parent_secret=initial-secret"));
    let followup_fetch = requests
        .iter()
        .find(|(path, _)| path == "/after")
        .expect("follow-up page request should reach the server");
    assert!(
        followup_fetch
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("resumed_worker=parent-owned"),
        "{requests:?}"
    );
}

pub(super) async fn native_service_worker_navigation_preload_timer_response_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (preload_seen_tx, preload_seen_rx) = oneshot::channel();
    let (release_preload_tx, release_preload_rx) = oneshot::channel();
    let (preload_cancelled_tx, preload_cancelled_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let registration_page = "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>preload overlap</main></body></html>";
    let worker_script = r#"self.addEventListener('install', event => event.waitUntil(
  caches.open('preload').then(cache => cache.put('/cached-response', new Response(
    '<!doctype html><html><body>ServiceWorker cached timer response before preload</body></html>',
    { headers: { 'Content-Type': 'text/html' } },
  ))).then(() => self.skipWaiting())
));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  const pathname = new URL(event.request.url).pathname;
  if (pathname === '/early-response') {
    event.respondWith(caches.match('/cached-response').then(cached => new Promise(resolve => setTimeout(
      () => resolve(cached || new Response('cache miss', { status: 503 })),
      40,
    ))));
  } else if (pathname === '/early-response-no-preload') {
    event.respondWith(new Promise(resolve => setTimeout(() => resolve(new Response(
      '<!doctype html><html><body>ServiceWorker timer response without preload</body></html>',
      { headers: { 'Content-Type': 'text/html' } },
    )), 40)));
  }
});"#;
    let server = tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        for (expected_path, content_type, body) in [
            ("/register", "text/html", registration_page),
            ("/sw.js", "application/javascript", worker_script),
        ] {
            let accepted = tokio::select! {
                biased;
                _ = &mut shutdown_rx => return,
                accepted = listener.accept() => accepted.unwrap(),
            };
            let (mut stream, _) = accepted;
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("GET"));
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let set_cookie = if expected_path == "/register" {
                "Set-Cookie: parent-session=seed; Path=/; HttpOnly\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }

        let accepted = tokio::select! {
            biased;
            _ = &mut shutdown_rx => return,
            accepted = listener.accept() => accepted.unwrap(),
        };
        let (mut stream, _) = accepted;
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("GET"));
        assert_eq!(request.split_whitespace().nth(1), Some("/early-response"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("service-worker-navigation-preload: true")
        );
        let _ = preload_seen_tx.send(());
        let _ = release_preload_rx.await;
        let mut probe = [0u8; 1];
        let preload_cancelled = matches!(
            tokio::time::timeout(Duration::from_secs(2), stream.read(&mut probe)).await,
            Ok(Ok(0))
        );
        let _ = preload_cancelled_tx.send(preload_cancelled);
        let body = "<!doctype html><html><body>unused delayed preload</body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    let session = BrowserSession::start_default().await.unwrap();
    session
        .navigate(format!("http://{address}/register"))
        .await
        .unwrap();
    session
        .script("await navigator.serviceWorker.ready.then(registration => { if (!registration.active) throw new Error('active ServiceWorker is missing'); })")
        .await
        .unwrap();
    assert_eq!(
        session.script("document.cookie").await.unwrap().value,
        serde_json::json!("")
    );

    session
        .navigate(format!("http://{address}/early-response-no-preload"))
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.body.innerText")
            .await
            .unwrap()
            .value,
        serde_json::json!("ServiceWorker timer response without preload")
    );
    session
        .script("await navigator.serviceWorker.getRegistration().then(registration => registration.navigationPreload.enable())")
        .await
        .unwrap();

    let mut navigation = Box::pin(session.navigate(format!("http://{address}/early-response")));
    let mut preload_seen = Box::pin(preload_seen_rx);
    let mut preload_was_seen = false;
    let mut navigation_result = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::select! {
            biased;
            seen = preload_seen.as_mut() => {
                seen.expect("server observed navigation preload");
                preload_was_seen = true;
                None
            },
            result = navigation.as_mut() => Some(result),
        }
    })
    .await
    .ok()
    .flatten();
    let completed_before_preload_response = if navigation_result.is_some() {
        true
    } else if let Ok(result) =
        tokio::time::timeout(Duration::from_secs(5), navigation.as_mut()).await
    {
        navigation_result = Some(result);
        true
    } else {
        false
    };
    let response_body = if matches!(navigation_result.as_ref(), Some(Ok(_))) {
        session
            .script("document.body.innerText")
            .await
            .ok()
            .map(|result| result.value)
    } else {
        None
    };

    let _ = release_preload_tx.send(());
    let _ = shutdown_tx.send(());
    if navigation_result.is_none() {
        navigation_result = tokio::time::timeout(Duration::from_secs(5), navigation.as_mut())
            .await
            .ok();
    }
    drop(navigation);
    session.close().await.unwrap();
    server.await.unwrap();

    assert!(
        preload_was_seen,
        "the enabled preload request must reach the server before the test can prove its response is still pending"
    );
    assert!(
        completed_before_preload_response,
        "an independent ServiceWorker response must commit without waiting for the preload body"
    );
    navigation_result
        .expect("ServiceWorker navigation settles after preload release")
        .expect("ServiceWorker navigation succeeds");
    assert!(
        preload_cancelled_rx.await.unwrap_or(false),
        "the parent-brokered preload should be cancelled when the independent response commits"
    );
    assert_eq!(
        response_body,
        Some(serde_json::json!(
            "ServiceWorker cached timer response before preload"
        ))
    );
}

pub(super) async fn native_service_worker_wait_until_fetch_does_not_delay_response_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (lifetime_seen_tx, lifetime_seen_rx) = oneshot::channel();
    let (cookie_check_tx, cookie_check_rx) = oneshot::channel();
    let (release_lifetime_tx, release_lifetime_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let registration_page = "<!doctype html><html><body><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>waitUntil fetch</main></body></html>";
    let worker_script = r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  const path = new URL(event.request.url).pathname;
  if (path === '/start') {
    event.waitUntil(fetch('/lifetime').then(response => response.text()).then(text => {
      globalThis.__nativeLifetimeResult = text;
    }));
    event.respondWith(new Response('fast response', {
      headers: { 'Content-Type': 'text/plain' },
    }));
  }
});"#;
    let server = tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        let mut release_lifetime_rx = release_lifetime_rx;
        let mut lifetime_seen_tx = Some(lifetime_seen_tx);
        let mut cookie_check_tx = Some(cookie_check_tx);
        loop {
            let accepted = tokio::select! {
                biased;
                _ = &mut shutdown_rx => return,
                accepted = listener.accept() => accepted.unwrap(),
            };
            let (mut stream, _) = accepted;
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (content_type, body, response_headers) = match path {
                "/register" => (
                    "text/html",
                    registration_page,
                    "Set-Cookie: lifetime_seed=secret; Path=/; HttpOnly; SameSite=Lax\r\n",
                ),
                "/sw.js" => ("application/javascript", worker_script, ""),
                "/lifetime" => {
                    if let Some(sender) = lifetime_seen_tx.take() {
                        let _ = sender.send(request.clone());
                    }
                    tokio::select! {
                        biased;
                        _ = &mut shutdown_rx => return,
                        _ = &mut release_lifetime_rx => {}
                    }
                    (
                        "text/plain",
                        "lifetime finished",
                        "Set-Cookie: lifetime_rotated=complete; Path=/; HttpOnly; SameSite=Lax\r\n",
                    )
                }
                "/cookie-check" => {
                    if let Some(sender) = cookie_check_tx.take() {
                        let _ = sender.send(request.clone());
                    }
                    ("text/plain", "cookie check", "")
                }
                _ => ("text/plain", "unexpected network request", ""),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{response_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    let session = BrowserSession::start_default().await.unwrap();
    session
        .navigate(format!("http://{address}/register"))
        .await
        .unwrap();
    session
        .script("await navigator.serviceWorker.ready")
        .await
        .unwrap();

    let start_url = format!("http://{address}/start");
    let mut navigation = Box::pin(session.navigate(start_url.clone()));
    let mut lifetime_seen = Box::pin(lifetime_seen_rx);
    let mut navigation_result = None;
    let first_event = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::select! {
            biased;
            seen = lifetime_seen.as_mut() => {
                Ok(seen.expect("server observed the waitUntil fetch"))
            },
            result = navigation.as_mut() => {
                navigation_result = Some(result);
                Err(())
            },
        }
    })
    .await
    .ok()
    .and_then(Result::ok);
    let mut lifetime_request = first_event;
    if lifetime_request.is_none() && navigation_result.is_some() {
        lifetime_request = tokio::time::timeout(Duration::from_secs(5), lifetime_seen.as_mut())
            .await
            .ok()
            .and_then(Result::ok);
    }
    let lifetime_fetch_reached_server = lifetime_request.is_some();
    let navigation_before_lifetime_release = if let Some(result) = navigation_result.as_ref() {
        result.as_ref().is_ok_and(|result| result.url == start_url)
    } else if lifetime_fetch_reached_server {
        match tokio::time::timeout(Duration::from_secs(5), navigation.as_mut()).await {
            Ok(result) => {
                let completed = result.as_ref().is_ok_and(|result| result.url == start_url);
                navigation_result = Some(result);
                completed
            }
            Err(_) => false,
        }
    } else {
        false
    };

    if navigation_result.is_none() {
        navigation_result = tokio::time::timeout(Duration::from_secs(5), navigation.as_mut())
            .await
            .ok();
    }
    if navigation_result
        .as_ref()
        .is_some_and(|result| result.is_ok())
    {
        // The asynchronous owner turn must outlive the ordinary 5-second page-script deadline.
        tokio::time::sleep(Duration::from_secs(6)).await;
    }
    let _ = release_lifetime_tx.send(());
    drop(navigation);
    if let Some(error) = navigation_result
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string)
    {
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
        server.await.unwrap();
        panic!(
            "navigation failed: {error}; lifetime Fetch reached server: {lifetime_fetch_reached_server}; independent response completed before release: {navigation_before_lifetime_release}; lifetime request: {lifetime_request:?}"
        );
    }
    let navigation_result = navigation_result
        .expect("navigation settles after the lifetime request is released")
        .expect("navigation succeeds");
    assert_eq!(navigation_result.url, start_url);

    let cookies = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let cookies = session
                .native_cookies()
                .await
                .expect("read parent cookie jar");
            if cookies
                .iter()
                .any(|cookie| cookie.name == "lifetime_rotated" && cookie.value == "complete")
            {
                break cookies;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("parent accepts the lifetime response cookie");
    for name in ["lifetime_seed", "lifetime_rotated"] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only)
        );
    }

    assert_eq!(
        session
            .script("await fetch('/cookie-check').then(response => response.status)")
            .await
            .unwrap()
            .value,
        serde_json::json!(200)
    );
    let cookie_check_request = tokio::time::timeout(Duration::from_secs(5), cookie_check_rx)
        .await
        .expect("follow-up request reaches the server")
        .expect("server records the follow-up request");
    assert!(
        cookie_check_request.contains("lifetime_seed=secret"),
        "parent omitted the original HttpOnly cookie from the follow-up request"
    );
    assert!(
        cookie_check_request.contains("lifetime_rotated=complete"),
        "parent did not retain the lifetime response cookie"
    );
    let visible_cookies = session.script("document.cookie").await.unwrap().value;
    let visible_cookies = visible_cookies.as_str().unwrap_or_default();
    assert!(!visible_cookies.contains("lifetime_seed"));
    assert!(!visible_cookies.contains("lifetime_rotated"));
    let _ = shutdown_tx.send(());
    session.close().await.unwrap();
    server.await.unwrap();

    assert!(
        lifetime_fetch_reached_server,
        "the worker lifetime fetch must reach the HTTP server"
    );
    assert!(
        lifetime_request
            .as_deref()
            .is_some_and(|request| request.contains("lifetime_seed=secret")),
        "parent must select the HttpOnly cookie for the waitUntil Fetch"
    );
    assert!(
        navigation_before_lifetime_release,
        "the FetchEvent navigation response must commit before the waitUntil fetch is released"
    );
}

pub(super) async fn native_content_process_idle_page_timer_brokers_dynamic_stylesheet_cookies_inner()
 {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stylesheet_seen_tx, stylesheet_seen_rx) = oneshot::channel();
    let (import_seen_tx, import_seen_rx) = oneshot::channel();
    let observed_paths = Arc::new(Mutex::new(Vec::new()));
    let server_observed_paths = Arc::clone(&observed_paths);
    let server = tokio::spawn(async move {
        let mut stylesheet_seen_tx = Some(stylesheet_seen_tx);
        let mut import_seen_tx = Some(import_seen_tx);
        let mut requests = Vec::new();
        for _ in 0..3 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("idle page timer's parent-brokered request should reach the server")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            server_observed_paths.lock().await.push(path.to_owned());
            let response = match path {
                "/page" => {
                    let body = "<!doctype html><html><head></head><body><script>setTimeout(() => { globalThis.timerCallbackRan = true; const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = '/timer.css'; document.head.appendChild(link); }, 25);</script><main>idle timer</main></body></html>";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: timer_seed=secret; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/timer.css" => {
                    if let Some(sender) = stylesheet_seen_tx.take() {
                        let _ = sender.send(request.clone());
                    }
                    let body = "@import url('/timer-import.css');";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: timer_root=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/timer-import.css" => {
                    if let Some(sender) = import_seen_tx.take() {
                        let _ = sender.send(request.clone());
                    }
                    let body = "body { color: rgb(1, 2, 3); }";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: timer_import=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                other => panic!("unexpected idle timer request: {other}"),
            };
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push(request);
        }
        requests
    });

    let session = BrowserSession::start_default().await.unwrap();
    session
        .navigate(format!("http://{address}/page"))
        .await
        .unwrap();
    let timer_requests = tokio::time::timeout(Duration::from_secs(15), async {
        let stylesheet = stylesheet_seen_rx
            .await
            .expect("timer-created stylesheet request should be observed");
        let import = import_seen_rx
            .await
            .expect("timer-created CSS import should be observed");
        (stylesheet, import)
    })
    .await;
    let (stylesheet_request, import_request) = match timer_requests {
        Ok(requests) => requests,
        Err(error) => {
            let paths_before_manual_turn = observed_paths.lock().await.clone();
            let manual_timer_turn = tokio::time::timeout(
                Duration::from_secs(10),
                session.script(
                    "globalThis.__glassRunTimers(performance.now()); globalThis.timerCallbackRan",
                ),
            )
            .await
            .map(|result| result.map(|result| result.value));
            tokio::time::sleep(Duration::from_millis(250)).await;
            let paths_after_manual_turn = observed_paths.lock().await.clone();
            let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            panic!(
                "idle page timer did not complete without another session call ({error}); paths before manual turn: {paths_before_manual_turn:?}; manual timer turn: {manual_timer_turn:?}; paths after manual turn: {paths_after_manual_turn:?}"
            );
        }
    };

    assert!(stylesheet_request.starts_with("GET /timer.css HTTP/1.1\r\n"));
    assert!(stylesheet_request.contains("timer_seed=secret"));
    assert!(import_request.starts_with("GET /timer-import.css HTTP/1.1\r\n"));
    assert!(import_request.contains("timer_seed=secret"));
    assert!(import_request.contains("timer_root=accepted"));
    assert_eq!(
        session.script("document.cookie").await.unwrap().value,
        serde_json::json!("")
    );
    let cookies = session.native_cookies().await.unwrap();
    for name in ["timer_seed", "timer_root", "timer_import"] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only)
        );
    }

    session.close().await.unwrap();
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 3);
}

pub(super) async fn native_content_process_idle_timer_stylesheet_events_broker_callback_fetches_inner()
 {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let blocked_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    blocked_listener.set_nonblocking(true).unwrap();
    let blocked_address = blocked_listener.local_addr().unwrap();
    let page = r#"<!doctype html><html><head><script>
globalThis.resourceEventTrace = [];
document.addEventListener('load', event => {
  resourceEventTrace.push(event.type + ':' + event.target.tagName);
}, true);
document.addEventListener('error', event => {
  resourceEventTrace.push(event.type + ':' + event.target.tagName);
}, true);
setTimeout(() => {
  globalThis.stylesheetLoadEventCount = 0;
  globalThis.stylesheetErrorEventCount = 0;
  globalThis.blockedLoadEventCount = 0;
  globalThis.blockedErrorEventCount = 0;
  let resolveStylesheetLoadFetch;
  globalThis.stylesheetLoadFetchSettled = new Promise(resolve => {
    resolveStylesheetLoadFetch = resolve;
  });
  const stylesheet = document.createElement('link');
  stylesheet.rel = 'stylesheet';
  stylesheet.href = '/timer.css';
  stylesheet.addEventListener('load', () => {
    globalThis.stylesheetLoadEventCount += 1;
    fetch('/load-check').then(response => response.text()).then(value => {
      globalThis.stylesheetLoadFetch = value;
      resolveStylesheetLoadFetch();
    });
  });
  stylesheet.addEventListener('error', () => { globalThis.stylesheetErrorEventCount += 1; });
  document.head.appendChild(stylesheet);

  const blocked = document.createElement('link');
  blocked.rel = 'stylesheet';
  blocked.href = 'http://127.0.0.1:BLOCKED_PORT/blocked.css';
  blocked.addEventListener('load', () => { globalThis.blockedLoadEventCount += 1; });
  blocked.addEventListener('error', () => {
    globalThis.blockedErrorEventCount += 1;
    globalThis.stylesheetLoadFetchSettled
      .then(() => fetch('/error-check'))
      .then(response => response.text())
      .then(value => {
        globalThis.stylesheetErrorFetch = value;
        return fetch('/chain-complete');
      })
      .then(response => response.text())
      .then(value => {
        globalThis.stylesheetChainComplete = value;
      });
  });
  document.head.appendChild(blocked);
}, 25);
</script></head><body><main>timer stylesheet event handoff</main></body></html>"#
        .replace("BLOCKED_PORT", &blocked_address.port().to_string());
    let observed_requests = Arc::new(Mutex::new(Vec::new()));
    let server_observed_requests = Arc::clone(&observed_requests);
    let mut server = tokio::spawn(async move {
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            server_observed_requests.lock().await.push(request.clone());
            let response = match path {
                "/page" => format!(
                    "HTTP/1.1 200 OK\r\nSet-Cookie: timer_seed=secret; Path=/; HttpOnly; SameSite=Lax\r\nContent-Security-Policy: style-src 'self'\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                ),
                "/timer.css" => {
                    assert!(request.contains("timer_seed=secret"));
                    let body = "@import url('/timer-import.css');";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: timer_root=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/timer-import.css" => {
                    assert!(request.contains("timer_seed=secret"));
                    assert!(request.contains("timer_root=accepted"));
                    let body = "body { color: rgb(1, 2, 3); }";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: timer_import=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/load-check" => {
                    for cookie in [
                        "timer_seed=secret",
                        "timer_root=accepted",
                        "timer_import=accepted",
                    ] {
                        assert!(request.contains(cookie), "missing {cookie} in {request:?}");
                    }
                    let body = "load-ok";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: timer_load=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/error-check" => {
                    for cookie in [
                        "timer_seed=secret",
                        "timer_root=accepted",
                        "timer_import=accepted",
                        "timer_load=accepted",
                    ] {
                        assert!(request.contains(cookie), "missing {cookie} in {request:?}");
                    }
                    let body = "error-ok";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: timer_error=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/chain-complete" => {
                    for cookie in [
                        "timer_seed=secret",
                        "timer_root=accepted",
                        "timer_import=accepted",
                        "timer_load=accepted",
                        "timer_error=accepted",
                    ] {
                        assert!(request.contains(cookie), "missing {cookie} in {request:?}");
                    }
                    let body = "chain-ok";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                other => panic!("unexpected stylesheet event request: {other}: {request}"),
            };
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserSession::start_default().await.unwrap();
    session
        .navigate(format!("http://{address}/page"))
        .await
        .unwrap();

    let server_result = tokio::time::timeout(Duration::from_secs(20), &mut server).await;
    match server_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("stylesheet event fixture server failed: {error}"),
        Err(error) => {
            let observed = observed_requests.lock().await.clone();
            server.abort();
            let _ = server.await;
            let callback_state = tokio::time::timeout(
                Duration::from_secs(5),
                session.script(
                    "[[globalThis.stylesheetLoadEventCount, globalThis.stylesheetErrorEventCount, globalThis.blockedLoadEventCount, globalThis.blockedErrorEventCount], globalThis.resourceEventTrace, globalThis.__glassLastHostEventResults]",
                ),
            )
            .await
            .map(|result| result.map(|result| result.value));
            let blocked_request_accepted = blocked_listener.accept().is_ok();
            let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            panic!(
                "timer stylesheet event callbacks did not complete their parent-brokered Fetches ({error}); observed requests: {observed:?}; callback state: {callback_state:?}; blocked stylesheet reached transport: {blocked_request_accepted}"
            );
        }
    }

    let requests = observed_requests.lock().await.clone();
    let paths = requests
        .iter()
        .map(|request| request.split_whitespace().nth(1).unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "/page",
            "/timer.css",
            "/timer-import.css",
            "/load-check",
            "/error-check",
            "/chain-complete",
        ]
    );
    assert!(
        matches!(
            blocked_listener.accept(),
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "CSP-blocked stylesheet must not reach child or parent network transport"
    );

    let event_state = session
        .script(
            "[globalThis.stylesheetLoadEventCount, globalThis.stylesheetErrorEventCount, globalThis.blockedLoadEventCount, globalThis.blockedErrorEventCount, globalThis.stylesheetLoadFetch, globalThis.stylesheetErrorFetch, globalThis.stylesheetChainComplete, document.cookie, globalThis.resourceEventTrace]",
        )
        .await
        .unwrap()
        .value;
    assert_eq!(
        event_state,
        serde_json::json!([
            1,
            0,
            0,
            1,
            "load-ok",
            "error-ok",
            "chain-ok",
            "",
            ["load:LINK", "error:LINK"]
        ])
    );
    let cookies = session.native_cookies().await.unwrap();
    for name in [
        "timer_seed",
        "timer_root",
        "timer_import",
        "timer_load",
        "timer_error",
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only),
            "parent cookie jar should retain HttpOnly {name}"
        );
    }
    session.close().await.unwrap();
}

pub(super) async fn native_content_process_idle_timer_image_media_events_broker_callback_fetches_inner()
 {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let blocked_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    blocked_listener.set_nonblocking(true).unwrap();
    let blocked_address = blocked_listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let page = r#"<!doctype html><html><head><script>
globalThis.resourceEventTrace = [];
document.addEventListener('load', event => resourceEventTrace.push(event.type + ':' + event.target.tagName), true);
document.addEventListener('error', event => resourceEventTrace.push(event.type + ':' + event.target.tagName), true);
setTimeout(() => {
  globalThis.imageLoadEventCount = 0;
  globalThis.imageErrorEventCount = 0;
  globalThis.blockedImageLoadEventCount = 0;
  globalThis.blockedImageErrorEventCount = 0;
  globalThis.mediaLoadEventCount = 0;
  let resolveImageFetch;
  let resolveErrorFetch;
  let resolveMediaFetch;
  globalThis.imageFetchSettled = new Promise(resolve => { resolveImageFetch = resolve; });
  globalThis.errorFetchSettled = new Promise(resolve => { resolveErrorFetch = resolve; });
  globalThis.mediaFetchSettled = new Promise(resolve => { resolveMediaFetch = resolve; });

  const image = document.createElement('img');
  image.width = 2;
  image.height = 2;
  image.addEventListener('load', () => {
    imageLoadEventCount += 1;
    fetch('/image-check').then(response => response.text()).then(value => {
      globalThis.imageCallback = value;
      resolveImageFetch();
    });
  });
  image.addEventListener('error', () => { imageErrorEventCount += 1; });
  image.src = '/timer.png';
  document.body.appendChild(image);

  const blocked = document.createElement('img');
  blocked.width = 2;
  blocked.height = 2;
  blocked.onload = () => { blockedImageLoadEventCount += 1; };
  blocked.onerror = () => {
    blockedImageErrorEventCount += 1;
    imageFetchSettled.then(() => fetch('/error-check')).then(response => response.text()).then(value => {
      globalThis.errorCallback = value;
      resolveErrorFetch();
    });
  };
  blocked.src = 'http://127.0.0.1:BLOCKED_PORT/blocked.png';
  document.body.appendChild(blocked);

  const audio = document.createElement('audio');
  // Error and media callbacks are independent after the successful image
  // callback. Let both parent-brokered Fetches proceed instead of serializing
  // unrelated work through one content-process turn.
  audio.addEventListener('load', () => {
    mediaLoadEventCount += 1;
    imageFetchSettled.then(() => fetch('/media-check')).then(response => response.text()).then(value => {
      globalThis.mediaCallback = value;
      resolveMediaFetch();
    });
  });
  audio.addEventListener('error', () => {});
  audio.src = '/timer.webm';
  document.body.appendChild(audio);
}, 25);
</script></head><body><main>timer image and media event handoff</main></body></html>"#
        .replace("BLOCKED_PORT", &blocked_address.port().to_string());
    let observed_requests = Arc::new(Mutex::new(Vec::new()));
    let server_observed_requests = Arc::clone(&observed_requests);
    let mut server = tokio::spawn(async move {
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            server_observed_requests.lock().await.push(request.clone());
            let response = match path {
                "/page" => format!(
                    "HTTP/1.1 200 OK\r\nSet-Cookie: event_seed=secret; Path=/; HttpOnly; SameSite=Lax\r\nContent-Security-Policy: script-src 'unsafe-inline'; img-src 'self'; media-src 'self'; connect-src 'self'\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                )
                .into_bytes(),
                "/timer.png" => {
                    assert!(request.contains("event_seed=secret"));
                    let mut response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_image=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    )
                    .into_bytes();
                    response.extend_from_slice(&png);
                    response
                }
                "/timer.webm" => {
                    assert!(request.contains("event_seed=secret"));
                    assert!(request.contains("event_image=accepted"));
                    let body = b"\x1a\x45\xdf\xa3";
                    let mut response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_media=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: audio/webm\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    response.extend_from_slice(body);
                    response
                }
                "/image-check" => {
                    for cookie in ["event_seed=secret", "event_image=accepted", "event_media=accepted"] {
                        assert!(request.contains(cookie), "missing {cookie} in {request:?}");
                    }
                    let body = "image-ok";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_image_callback=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .into_bytes()
                }
                "/error-check" => {
                    for cookie in ["event_seed=secret", "event_image=accepted", "event_media=accepted", "event_image_callback=accepted"] {
                        assert!(request.contains(cookie), "missing {cookie} in {request:?}");
                    }
                    let body = "error-ok";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_error_callback=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .into_bytes()
                }
                "/media-check" => {
                    for cookie in ["event_seed=secret", "event_image=accepted", "event_media=accepted", "event_image_callback=accepted"] {
                        assert!(request.contains(cookie), "missing {cookie} in {request:?}");
                    }
                    let body = "media-ok";
                    format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_media_callback=accepted; Path=/; HttpOnly; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .into_bytes()
                }
                other => panic!("unexpected resource-event request: {other}: {request}"),
            };
            stream.write_all(&response).await.unwrap();
        }
    });

    let session = BrowserSession::start_default().await.unwrap();
    if let Err(error) = session.navigate(format!("http://{address}/page")).await {
        let observed = observed_requests.lock().await.clone();
        let paths = observed
            .iter()
            .map(|request| request.split_whitespace().nth(1).unwrap_or_default())
            .collect::<Vec<_>>();
        server.abort();
        let _ = server.await;
        let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
        panic!("image/media event navigation failed: {error}; observed paths: {paths:?}");
    }

    let server_result = tokio::time::timeout(Duration::from_secs(20), &mut server).await;
    match server_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("image/media event fixture server failed: {error}"),
        Err(error) => {
            let observed = observed_requests.lock().await.clone();
            server.abort();
            let _ = server.await;
            let callback_state = tokio::time::timeout(
                Duration::from_secs(5),
                session.script(
                    "[[globalThis.imageLoadEventCount, globalThis.blockedImageErrorEventCount, globalThis.mediaLoadEventCount, globalThis.imageCallback, globalThis.errorCallback, globalThis.mediaCallback, globalThis.resourceEventTrace], globalThis.__glassLastHostEventResults]",
                ),
            )
            .await
            .map(|result| result.map(|result| result.value));
            let blocked_request_accepted = blocked_listener.accept().is_ok();
            let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            panic!(
                "image/media event callbacks did not complete their parent-brokered Fetches ({error}); observed requests: {observed:?}; callback state: {callback_state:?}; blocked image reached transport: {blocked_request_accepted}"
            );
        }
    }

    let requests = observed_requests.lock().await.clone();
    let paths = requests
        .iter()
        .map(|request| request.split_whitespace().nth(1).unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(&paths[..3], ["/page", "/timer.png", "/timer.webm"]);
    let mut callback_paths = paths[3..].to_vec();
    callback_paths.sort_unstable();
    assert_eq!(
        callback_paths,
        ["/error-check", "/image-check", "/media-check"]
    );
    assert!(
        matches!(
            blocked_listener.accept(),
            Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "CSP-blocked image must not reach child or parent network transport"
    );

    let event_state = session
        .script(
            "[globalThis.imageLoadEventCount, globalThis.imageErrorEventCount, globalThis.blockedImageLoadEventCount, globalThis.blockedImageErrorEventCount, globalThis.mediaLoadEventCount, globalThis.imageCallback, globalThis.errorCallback, globalThis.mediaCallback, document.cookie, globalThis.resourceEventTrace]",
        )
        .await
        .unwrap()
        .value;
    assert_eq!(
        event_state,
        serde_json::json!([
            1,
            0,
            0,
            1,
            1,
            "image-ok",
            "error-ok",
            "media-ok",
            "",
            ["load:IMG", "error:IMG", "load:AUDIO"]
        ])
    );
    let cookies = session.native_cookies().await.unwrap();
    for name in [
        "event_seed",
        "event_image",
        "event_media",
        "event_image_callback",
        "event_error_callback",
        "event_media_callback",
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only),
            "parent cookie jar should retain HttpOnly {name}"
        );
    }
    session.close().await.unwrap();
}

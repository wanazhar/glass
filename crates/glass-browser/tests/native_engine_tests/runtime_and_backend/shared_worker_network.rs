use super::*;

#[test]
fn native_runtime_shared_worker_fetch_cors_errors_keep_parent_cookies() {
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
                "glass-native-shared-worker-fetch-cors-{}-profile.json",
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
            let shared_worker_api_origin = api_origin.clone();
            let expected_origin = page_origin.clone();
            let response_origin = page_origin.clone();

            let page_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for expected_path in ["/page", "/shared-fetch-worker.js"] {
                        let (mut stream, _) = page_listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
                        let cookie = header_value(&request, "cookie").unwrap_or_default();
                        let (content_type, set_cookie, body) = if expected_path == "/page" {
                            (
                                "text/html",
                                "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                                r#"<script>
globalThis.sharedFetchMessages = [];
globalThis.sharedFetchWorker = new SharedWorker('/shared-fetch-worker.js', {
  name: 'parent-owned-cors-cookie', credentials: 'include',
});
sharedFetchWorker.port.addEventListener('message', event => sharedFetchMessages.push(event.data));
sharedFetchWorker.port.start();
</script>"#
                                    .to_owned(),
                            )
                        } else {
                            assert!(cookie.contains("root=seed"), "Worker script missed seed cookie: {cookie}");
                            (
                                "text/javascript",
                                "Set-Cookie: worker_entry=shared; HttpOnly; Path=/; SameSite=Lax\r\n",
                                format!(
                                    r#"const apiOrigin = "{shared_worker_api_origin}";
globalThis.onconnect = event => {{
  const port = event.ports[0];
  port.postMessage(["ready"]);
  port.onmessage = async message => {{
    if (message.data !== "run") return;
    try {{
      const response = await fetch(apiOrigin + "/cors-failed", {{ credentials: "include" }});
      port.postMessage(["unexpected-response", response.status, await response.text()]);
    }} catch (error) {{
      port.postMessage(["rejected", error.name, error instanceof TypeError]);
    }}
    try {{
      const response = await fetch(apiOrigin + "/worker-verify", {{ credentials: "include" }});
      port.postMessage(["worker-verified", await response.text()]);
    }} catch (error) {{
      port.postMessage(["worker-verify-error", error.name, error.message]);
    }}
  }};
}};"#
                                ),
                            )
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{set_cookie}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    }
                })
                .await
                .expect("SharedWorker parent-owned script requests stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for (path, required_cookies) in [
                        ("/cors-failed", vec!["root=seed", "worker_entry=shared"]),
                        (
                            "/worker-verify",
                            vec!["root=seed", "worker_entry=shared", "shared_fetch_failed=accepted"],
                        ),
                        (
                            "/page-verify",
                            vec!["root=seed", "worker_entry=shared", "shared_fetch_failed=accepted"],
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
                            assert!(cookie.contains(expected), "{path} missed {expected}: {cookie}");
                        }
                        let (cors_headers, set_cookie, body) = if path == "/cors-failed" {
                            (
                                "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\n",
                                "Set-Cookie: shared_fetch_failed=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "hidden",
                            )
                        } else {
                            (
                                "Access-Control-Allow-Credentials: true\r\n",
                                "",
                                if path == "/worker-verify" { "worker-ok" } else { "page-ok" },
                            )
                        };
                        let origin_header = if path == "/cors-failed" {
                            String::new()
                        } else {
                            format!("Access-Control-Allow-Origin: {response_origin}\r\n")
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{cors_headers}{origin_header}{set_cookie}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    }
                    match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                        Err(_) => {}
                        Ok(Ok((mut stream, _))) => {
                            let request = read_http_request(&mut stream).await;
                            panic!("unexpected SharedWorker cookie request: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("SharedWorker CORS-cookie requests stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let ready = session
                        .script("sharedFetchMessages.some(message => message[0] === 'ready')")
                        .await
                        .unwrap()
                        .value;
                    if ready == serde_json::Value::Bool(true) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("browser-owned SharedWorker is connected");
            session
                .script("sharedFetchWorker.port.postMessage('run'); true")
                .await
                .unwrap();
            let messages = tokio::time::timeout(Duration::from_secs(25), async {
                loop {
                    let messages = session
                        .script("sharedFetchMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| values.len() == 3) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("SharedWorker CORS failure and follow-up Fetch settle");
            assert_eq!(
                messages,
                serde_json::json!([
                    ["ready"],
                    ["rejected", "TypeError", true],
                    ["worker-verified", "worker-ok"],
                ])
            );
            let page_verify_script = format!(
                r#"await fetch("{api_origin}/page-verify", {{ credentials: "include" }})
                    .then(response => response.text())"#
            );
            assert_eq!(
                session
                    .script(page_verify_script)
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("page-ok")
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
                ("root", "seed"),
                ("worker_entry", "shared"),
                ("shared_fetch_failed", "accepted"),
            ] {
                assert!(
                    cookies
                        .iter()
                        .any(|cookie| { cookie.name == name && cookie.value == value && cookie.http_only }),
                    "parent cookie jar is missing {name}={value}"
                );
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

#[test]
fn native_runtime_shared_worker_event_source_cors_errors_keep_parent_cookies() {
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
                "glass-native-shared-worker-eventsource-cors-{}-profile.json",
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
            let page_api_origin = api_origin.clone();
            let expected_origin = page_origin.clone();
            let response_origin = page_origin.clone();

            let page_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(40), async move {
                    for expected_path in ["/page", "/shared-eventsource.js"] {
                        let (mut stream, _) = page_listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
                        let (set_cookie, content_type, body) = if expected_path == "/page" {
                            (
                                "Set-Cookie: event_seed=page; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/html",
                                r#"<script>
globalThis.sharedEventSourceMessages = [];
globalThis.startSharedEventSource = () => {
  const worker = new SharedWorker('/shared-eventsource.js', {
    name: 'shared-eventsource-cors', credentials: 'include',
  });
  worker.port.addEventListener('message', event => sharedEventSourceMessages.push(event.data));
  worker.port.start();
};
</script>"#
                                    .to_owned(),
                            )
                        } else {
                            let body = format!(
                                r#"onconnect = event => {{
  const port = event.ports[0];
  const source = new EventSource("{page_api_origin}/events", {{ withCredentials: true }});
  source.addEventListener('open', () => port.postMessage('open'));
  source.addEventListener('message', message => port.postMessage(['message', message.data]));
  source.addEventListener('error', () => {{ port.postMessage('error'); source.close(); }});
}};"#
                            );
                            (
                                "Set-Cookie: shared_entry=worker; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/javascript",
                                body,
                            )
                        };
                        if expected_path == "/shared-eventsource.js" {
                            assert!(header_value(&request, "cookie")
                                .unwrap_or_default()
                                .contains("event_seed=page"));
                        }
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{set_cookie}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    }
                })
                .await
                .expect("SharedWorker page and entry requests stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for (path, expected_cookie) in [
                        ("/events", "event_seed=page"),
                        ("/verify", "cors_event_cookie=accepted"),
                    ] {
                        let (mut stream, _) = tokio::time::timeout(
                            Duration::from_secs(30),
                            api_listener.accept(),
                        )
                        .await
                        .expect("SharedWorker EventSource and authorized page Fetch arrive")
                        .unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(path));
                        assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                        assert!(header_value(&request, "access-control-request-method").is_none());
                        let cookie = header_value(&request, "cookie").unwrap_or_default();
                        assert!(cookie.contains(expected_cookie), "{path} missed {expected_cookie}: {cookie}");

                        let (cors_headers, set_cookie, content_type, body) = if path == "/events" {
                            assert!(cookie.contains("shared_entry=worker"));
                            assert_eq!(header_value(&request, "accept"), Some("text/event-stream"));
                            (
                                "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\n",
                                "Set-Cookie: cors_event_cookie=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/event-stream",
                                "retry: 30000\r\ndata: hidden\r\n\r\n",
                            )
                        } else {
                            assert!(cookie.contains("event_seed=page"));
                            assert!(cookie.contains("shared_entry=worker"));
                            assert!(cookie.contains("cors_event_cookie=accepted"));
                            (
                                "",
                                "",
                                "text/plain",
                                "authorized",
                            )
                        };
                        let cors_headers = if path == "/events" {
                            cors_headers.to_owned()
                        } else {
                            format!(
                                "Access-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                            )
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{cors_headers}{set_cookie}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    }

                    match tokio::time::timeout(Duration::from_millis(1500), api_listener.accept())
                        .await
                    {
                        Err(_) => {}
                        Ok(Ok((mut stream, _))) => {
                            let request = read_http_request(&mut stream).await;
                            panic!("unexpected SharedWorker EventSource reconnect: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("SharedWorker EventSource CORS-cookie requests stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            session
                .script("startSharedEventSource(); true")
                .await
                .unwrap();
            let messages = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let messages = session
                        .script("sharedEventSourceMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages == serde_json::json!(["error"]) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("CORS failure reaches SharedWorker without open or message data");
            assert_eq!(messages, serde_json::json!(["error"]));
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
                ("event_seed", "page"),
                ("shared_entry", "worker"),
                ("cors_event_cookie", "accepted"),
            ] {
                assert!(
                    cookies.iter().any(|cookie| {
                        cookie.name == name && cookie.value == value && cookie.http_only
                    }),
                    "parent cookie jar is missing HttpOnly {name}={value}"
                );
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

#[test]
fn native_runtime_shared_worker_event_source_uses_parent_cookie_authority() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-shared-worker-eventsource-{}-profile.json",
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

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(60), async move {
                    let mut requests = Vec::new();
                    for _ in 0..5 {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .expect("SharedWorker EventSource request has a path")
                            .to_owned();
                        let cookie = request
                            .lines()
                            .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                            .and_then(|line| line.split_once(':'))
                            .map(|(_, value)| value.trim().to_owned());
                        let (headers, content_type, body): (&str, &str, &str) = match path.as_str()
                        {
                            "/page" => (
                                "Set-Cookie: parent_seed=page; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/html",
                                r#"<script>
globalThis.eventSourceMessages = [];
globalThis.startSharedWorkerEventSource = () => {
  const worker = new SharedWorker('/shared-eventsource.js', {
    name: 'parent-cookie-eventsource', credentials: 'include',
  });
  worker.port.addEventListener('message', event => eventSourceMessages.push(event.data));
  worker.port.start();
};
</script>"#,
                            ),
                            "/shared-eventsource.js" => (
                                "Set-Cookie: worker_entry=shared; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/javascript",
                                "onconnect = event => { const port = event.ports[0]; const source = new EventSource('/events', { withCredentials: true }); source.addEventListener('open', () => port.postMessage(['open', source.readyState])); source.addEventListener('greeting', message => { port.postMessage([message.type, message.data, message.lastEventId, message.origin]); source.close(); }); source.onerror = error => port.postMessage(['error', String(error.message || '')]); };",
                            ),
                            "/events" => (
                                "Set-Cookie: event_stream=sse; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/event-stream",
                                "retry: 5\r\nid: 42\r\nevent: greeting\r\ndata: hello\r\ndata: worker\r\n\r\n",
                            ),
                            "/after" => ("", "text/plain", "after"),
                            "/reopen" => ("", "text/html", "<p>reopened</p>"),
                            other => panic!("unexpected SharedWorker EventSource request: {other}"),
                        };
                        if path == "/shared-eventsource.js" {
                            assert!(cookie
                                .as_deref()
                                .is_some_and(|value| value.contains("parent_seed=page")));
                        }
                        if path == "/events" {
                            assert!(cookie
                                .as_deref()
                                .is_some_and(|value| value.contains("parent_seed=page")));
                            assert!(cookie
                                .as_deref()
                                .is_some_and(|value| value.contains("worker_entry=shared")));
                        }
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                        requests.push((path, cookie));
                    }
                    requests
                })
                .await
                .expect("SharedWorker EventSource requests stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/page")),
            )
            .await
            .unwrap();
            session
                .script("startSharedWorkerEventSource(); true")
                .await
                .unwrap();
            let messages = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let messages = session
                        .script("eventSourceMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| values.len() == 2) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("parent EventSource stream events reach the SharedWorker");
            assert_eq!(
                messages,
                serde_json::json!([
                    ["open", 1],
                    [
                        "greeting",
                        "hello\nworker",
                        "42",
                        format!("http://{address}")
                    ]
                ])
            );
            assert_eq!(
                session.script("document.cookie").await.unwrap().value,
                serde_json::json!("")
            );
            assert_eq!(
                session
                    .script("await fetch('/after').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("after")
            );
            session.close().await.unwrap();

            let reopened = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/reopen")),
            )
            .await
            .unwrap();
            assert_eq!(
                reopened.script("document.cookie").await.unwrap().value,
                serde_json::json!("")
            );
            reopened.close().await.unwrap();
            let requests = server.await.unwrap();
            let cookie_for = |path: &str| {
                requests
                    .iter()
                    .find(|(request_path, _)| request_path == path)
                    .unwrap_or_else(|| panic!("missing SharedWorker EventSource request {path}"))
                    .1
                    .as_deref()
                    .unwrap_or("")
                    .to_owned()
            };
            for path in ["/after", "/reopen"] {
                let cookie = cookie_for(path);
                for name in ["parent_seed=page", "worker_entry=shared", "event_stream=sse"] {
                    assert!(cookie.contains(name), "{path} must send parent cookie {name}");
                }
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

#[test]
// tungstenite's handshake callback API requires its unboxed HTTP response error.
#[allow(clippy::result_large_err)]
fn native_runtime_shared_worker_websocket_uses_parent_cookie_authority() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-shared-worker-websocket-{}-profile.json",
                std::process::id()
            ));
            let isolated_profile_path = std::env::temp_dir().join(format!(
                "glass-native-shared-worker-websocket-{}-isolated.json",
                std::process::id()
            ));
            let cookie_profile_path =
                std::path::PathBuf::from(format!("{}.cookies", profile_path.display()));
            let isolated_cookie_profile_path = std::path::PathBuf::from(format!(
                "{}.cookies",
                isolated_profile_path.display()
            ));
            let profile_lock_path = profile_path.with_extension("lock");
            let cookie_profile_lock_path = cookie_profile_path.with_extension("lock");
            let isolated_lock_path = isolated_profile_path.with_extension("lock");
            let isolated_cookie_profile_lock_path =
                isolated_cookie_profile_path.with_extension("lock");
            let worker_clients_path = profile_path.with_extension("clients");
            let worker_events_path = profile_path.with_extension("events");
            let worker_readers_path = profile_path.with_extension("readers");
            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &isolated_profile_path,
                &isolated_cookie_profile_path,
                &isolated_lock_path,
                &isolated_cookie_profile_lock_path,
                &worker_clients_path,
                &worker_events_path,
                &worker_readers_path,
            ] {
                let _ = fs::remove_file(path);
            }

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let handshake_cookie_valid = Arc::new(AtomicBool::new(false));
            let handshake_cookie_valid_by_server = Arc::clone(&handshake_cookie_valid);
            let explicit_close_seen = Arc::new(AtomicBool::new(false));
            let explicit_close_seen_by_server = Arc::clone(&explicit_close_seen);
            let teardown_close_seen = Arc::new(AtomicBool::new(false));
            let teardown_close_seen_by_server = Arc::clone(&teardown_close_seen);
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(60), async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    assert_eq!(request.split_whitespace().nth(1), Some("/page"));
                    let body = r#"<script>globalThis.websocketMessages=[]; globalThis.startSharedWorkerWebSocket=()=>{const worker=new SharedWorker('/shared-websocket.js',{name:'parent-cookie-shared-websocket',credentials:'include'}); worker.port.addEventListener('message',event=>websocketMessages.push(event.data)); worker.port.start();};</script>"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: parent_seed=page; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();

                    let (mut stream, _) = listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    assert_eq!(
                        request.split_whitespace().nth(1),
                        Some("/shared-websocket.js")
                    );
                    let body = format!("onconnect=event=>{{const port=event.ports[0]; const socket=new WebSocket('ws://{address}/socket','glass-protocol'); const held=new WebSocket('ws://{address}/held'); socket.binaryType='arraybuffer'; socket.addEventListener('open',()=>{{port.postMessage(['open',socket.protocol]); socket.send('client-frame'); socket.send(new Uint8Array([4,5,6]));}}); socket.addEventListener('message',event=>{{if(typeof event.data==='string') port.postMessage(['message',event.data,event.origin]); else {{port.postMessage(['binary',Array.from(new Uint8Array(event.data)),event.origin]); socket.close(3001,'worker-close');}}}}); socket.addEventListener('close',event=>port.postMessage(['close',event.code,event.reason,event.wasClean])); socket.addEventListener('error',()=>port.postMessage(['error'])); held.addEventListener('open',()=>port.postMessage(['held-open'])); held.addEventListener('error',()=>port.postMessage(['held-error']));}};");
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: worker_entry=shared; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();

                    let (stream, _) = listener.accept().await.unwrap();
                    let handshake_cookie_valid =
                        Arc::clone(&handshake_cookie_valid_by_server);
                    let websocket = accept_hdr_async(
                        stream,
                        move |request: &WebSocketRequest, mut response: WebSocketResponse| {
                            let cookie = request
                                .headers()
                                .get(COOKIE)
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default();
                            handshake_cookie_valid.store(
                                cookie.contains("parent_seed=page")
                                    && cookie.contains("worker_entry=shared"),
                                Ordering::SeqCst,
                            );
                            response.headers_mut().insert(
                                SET_COOKIE,
                                HeaderValue::from_static(
                                    "socket_cookie=from-handshake; HttpOnly; Path=/; SameSite=Lax",
                                ),
                            );
                            response.headers_mut().insert(
                                tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL,
                                HeaderValue::from_static("glass-protocol"),
                            );
                            Ok(response)
                        },
                    )
                    .await
                    .unwrap();
                    let explicit_close_seen = Arc::clone(&explicit_close_seen_by_server);
                    let explicit_websocket_task = tokio::spawn(async move {
                        let (mut writer, mut reader) = websocket.split();
                        match tokio::time::timeout(Duration::from_secs(15), reader.next())
                            .await
                            .unwrap()
                            .unwrap()
                            .unwrap()
                        {
                            Message::Text(data) => assert_eq!(data, "client-frame"),
                            other => panic!("unexpected SharedWorker WebSocket frame: {other:?}"),
                        }
                        match tokio::time::timeout(Duration::from_secs(15), reader.next())
                            .await
                            .unwrap()
                            .unwrap()
                            .unwrap()
                        {
                            Message::Binary(data) => assert_eq!(data.as_ref(), &[4, 5, 6]),
                            other => panic!("unexpected SharedWorker WebSocket frame: {other:?}"),
                        }
                        writer
                            .send(Message::Text("parent-response".into()))
                            .await
                            .unwrap();
                        writer
                            .send(Message::Binary(vec![1_u8, 2, 255].into()))
                            .await
                            .unwrap();
                        let close = tokio::time::timeout(Duration::from_secs(15), reader.next())
                            .await
                            .unwrap()
                            .unwrap()
                            .unwrap();
                        match close {
                            Message::Close(Some(frame)) => {
                                explicit_close_seen.store(
                                    u16::from(frame.code) == 3001
                                        && frame.reason == "worker-close",
                                    Ordering::SeqCst,
                                );
                                writer.flush().await.unwrap();
                            }
                            other => panic!("expected explicit SharedWorker close, got {other:?}"),
                        }
                    });

                    let (stream, _) = listener.accept().await.unwrap();
                    let handshake_cookie_valid = Arc::clone(&handshake_cookie_valid_by_server);
                    let held_websocket = accept_hdr_async(
                        stream,
                        move |request: &WebSocketRequest, mut response: WebSocketResponse| {
                            let cookie = request
                                .headers()
                                .get(COOKIE)
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default();
                            handshake_cookie_valid.store(
                                cookie.contains("parent_seed=page")
                                    && cookie.contains("worker_entry=shared"),
                                Ordering::SeqCst,
                            );
                            response.headers_mut().insert(
                                SET_COOKIE,
                                HeaderValue::from_static(
                                    "socket_cookie=from-handshake; HttpOnly; Path=/; SameSite=Lax",
                                ),
                            );
                            Ok(response)
                        },
                    )
                    .await
                    .unwrap();
                    let teardown_close_seen = Arc::clone(&teardown_close_seen_by_server);
                    let held_websocket_task = tokio::spawn(async move {
                        let (mut writer, mut reader) = held_websocket.split();
                        match tokio::time::timeout(Duration::from_secs(30), reader.next()).await {
                            Ok(Some(Ok(Message::Close(Some(frame))))) => {
                                teardown_close_seen.store(
                                    u16::from(frame.code) == 1001,
                                    Ordering::SeqCst,
                                );
                                let _ = writer.send(Message::Close(Some(frame))).await;
                            }
                            Ok(Some(Ok(Message::Close(None)))) => {
                                teardown_close_seen.store(true, Ordering::SeqCst);
                            }
                            Ok(None) => teardown_close_seen.store(true, Ordering::SeqCst),
                            other => panic!("SharedWorker teardown did not close WebSocket: {other:?}"),
                        }
                    });

                    let mut requests = Vec::new();
                    for _ in 0..5 {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .expect("SharedWorker WebSocket test request has a path")
                            .to_owned();
                        let cookie = request
                            .lines()
                            .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                            .and_then(|line| line.split_once(':'))
                            .map(|(_, value)| value.trim().to_owned());
                        let (content_type, body) = match path.as_str() {
                            "/after" | "/reopened-profile" | "/isolated-observe" => {
                                ("text/plain", "observed")
                            }
                            "/reopen" => ("text/html", "<script>globalThis.reopenResult=fetch('/reopened-profile').then(r=>r.text());</script>"),
                            "/isolated" => ("text/html", "<script>globalThis.isolatedResult=fetch('/isolated-observe').then(r=>r.text());</script>"),
                            other => panic!("unexpected SharedWorker WebSocket HTTP request: {other}"),
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                        requests.push((path, cookie));
                    }
                    explicit_websocket_task.await.unwrap();
                    held_websocket_task.await.unwrap();
                    requests
                })
                .await
                .expect("SharedWorker WebSocket requests and teardown stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/page")),
            )
            .await
            .unwrap();
            session
                .script("startSharedWorkerWebSocket(); true")
                .await
                .unwrap();
            let messages = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let messages = session
                        .script("websocketMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| values.len() >= 5) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("parent WebSocket open, message, and explicit close reach the SharedWorker");
            let events = messages.as_array().expect("worker messages should be an array");
            for expected in [
                serde_json::json!(["open", "glass-protocol"]),
                serde_json::json!(["held-open"]),
                serde_json::json!([
                    "message",
                    "parent-response",
                    format!("http://{address}")
                ]),
                serde_json::json!(["binary", [1, 2, 255], format!("http://{address}")]),
                serde_json::json!(["close", 3001, "worker-close", true]),
            ] {
                assert!(events.contains(&expected), "missing worker event {expected}: {events:?}");
            }
            assert_eq!(
                session.script("document.cookie").await.unwrap().value,
                serde_json::json!("")
            );
            assert_eq!(
                session
                    .script("await fetch('/after').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("observed")
            );
            session.close().await.unwrap();

            let reopened = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/reopen")),
            )
            .await
            .unwrap();
            assert_eq!(
                reopened
                    .script("await reopenResult")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("observed")
            );
            reopened.close().await.unwrap();

            let isolated = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(isolated_profile_path.clone())
                    .with_initial_url(format!("http://{address}/isolated")),
            )
            .await
            .unwrap();
            assert_eq!(
                isolated
                    .script("await isolatedResult")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("observed")
            );
            isolated.close().await.unwrap();

            let requests = server.await.unwrap();
            let cookie_for = |path: &str| {
                requests
                    .iter()
                    .find(|(request_path, _)| request_path == path)
                    .unwrap_or_else(|| panic!("missing SharedWorker WebSocket request {path}"))
                    .1
                    .as_deref()
                    .unwrap_or("")
                    .to_owned()
            };
            assert!(handshake_cookie_valid.load(Ordering::SeqCst));
            assert!(explicit_close_seen.load(Ordering::SeqCst));
            assert!(teardown_close_seen.load(Ordering::SeqCst));
            for path in ["/after", "/reopened-profile"] {
                let cookie = cookie_for(path);
                for name in [
                    "parent_seed=page",
                    "worker_entry=shared",
                    "socket_cookie=from-handshake",
                ] {
                    assert!(cookie.contains(name), "{path} must send parent cookie {name}");
                }
            }
            assert!(!cookie_for("/isolated-observe").contains("socket_cookie="));

            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &isolated_profile_path,
                &isolated_cookie_profile_path,
                &isolated_lock_path,
                &isolated_cookie_profile_lock_path,
                &worker_clients_path,
                &worker_events_path,
                &worker_readers_path,
            ] {
                let _ = fs::remove_file(path);
            }
        });
    });
}

#[test]
fn native_runtime_shared_worker_websocket_failed_handshake_keeps_parent_cookies() {
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
                "glass-native-shared-worker-websocket-failed-handshake-{}-profile.json",
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

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let origin = format!("http://{address}");
            let expected_origin = origin.clone();
            let handshake_response_sent = Arc::new(AtomicBool::new(false));
            let handshake_response_sent_by_server = Arc::clone(&handshake_response_sent);
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(60), async move {
                    let (mut page_stream, _) = listener.accept().await.unwrap();
                    let page_request = read_http_request(&mut page_stream).await;
                    assert_eq!(page_request.split_whitespace().nth(1), Some("/page"));
                    let page_body = "<script>globalThis.sharedWebSocketEvents=[]; globalThis.startSharedWebSocket=()=>{const worker=new SharedWorker('/shared-websocket.js',{name:'failed-handshake-parent-cookie',credentials:'include'}); worker.port.addEventListener('message',event=>sharedWebSocketEvents.push(event.data)); worker.port.start();};</script>";
                    let page_response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: shared_parent_seed=page; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page_body}",
                        page_body.len()
                    );
                    page_stream.write_all(page_response.as_bytes()).await.unwrap();

                    let (mut worker_stream, _) = listener.accept().await.unwrap();
                    let worker_request = read_http_request(&mut worker_stream).await;
                    assert_eq!(worker_request.split_whitespace().nth(1), Some("/shared-websocket.js"));
                    let worker_body = format!(
                        r#"onconnect = event => {{ const port = event.ports[0]; const socket = new WebSocket("ws://{address}/reject"); socket.addEventListener("open", () => port.postMessage(["open"])); socket.addEventListener("error", () => port.postMessage(["error"])); socket.addEventListener("close", event => port.postMessage(["close", event.code, event.wasClean])); }};"#
                    );
                    let worker_response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: shared_worker_entry=worker; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{worker_body}",
                        worker_body.len()
                    );
                    worker_stream.write_all(worker_response.as_bytes()).await.unwrap();

                    let (mut handshake_stream, _) = listener.accept().await.unwrap();
                    let handshake = read_http_request(&mut handshake_stream).await;
                    assert_eq!(handshake.split_whitespace().next(), Some("GET"));
                    assert_eq!(handshake.split_whitespace().nth(1), Some("/reject"));
                    assert_eq!(header_value(&handshake, "origin"), Some(expected_origin.as_str()));
                    assert!(header_value(&handshake, "upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket")));
                    let handshake_cookie = header_value(&handshake, "cookie").unwrap_or_default();
                    for expected in ["shared_parent_seed=page", "shared_worker_entry=worker"] {
                        assert!(handshake_cookie.contains(expected), "SharedWorker handshake missed {expected}: {handshake_cookie}");
                    }
                    assert!(!handshake_cookie.contains("shared_rejected=accepted"));
                    let rejected_body = "handshake denied";
                    let rejected_response = format!(
                        "HTTP/1.1 403 Forbidden\r\nSet-Cookie: shared_rejected=accepted; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{rejected_body}",
                        rejected_body.len()
                    );
                    handshake_stream.write_all(rejected_response.as_bytes()).await.unwrap();
                    handshake_response_sent_by_server.store(true, Ordering::SeqCst);

                    let (mut verify_stream, _) = tokio::time::timeout(
                        Duration::from_secs(30),
                        listener.accept(),
                    )
                    .await
                    .expect("later page Fetch reaches the API")
                    .unwrap();
                    let verify = read_http_request(&mut verify_stream).await;
                    assert_eq!(verify.split_whitespace().next(), Some("GET"));
                    assert_eq!(verify.split_whitespace().nth(1), Some("/verify"));
                    let verify_cookie = header_value(&verify, "cookie").unwrap_or_default();
                    for expected in [
                        "shared_parent_seed=page",
                        "shared_worker_entry=worker",
                        "shared_rejected=accepted",
                    ] {
                        assert!(verify_cookie.contains(expected), "later page Fetch missed parent cookie {expected}: {verify_cookie}");
                    }
                    let body = "authorized";
                    let verify_response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    verify_stream.write_all(verify_response.as_bytes()).await.unwrap();

                    match tokio::time::timeout(Duration::from_millis(1000), listener.accept()).await {
                        Err(_) => {}
                        Ok(Ok((mut stream, _))) => {
                            let request = read_http_request(&mut stream).await;
                            panic!("unexpected SharedWorker WebSocket retry or redirect: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("SharedWorker failed-handshake cookie flow stays bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/page")),
            )
            .await
            .unwrap();
            session.script("startSharedWebSocket(); true").await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if handshake_response_sent.load(Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("API sends the rejected SharedWorker WebSocket response");
            let messages = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let messages = session
                        .script("sharedWebSocketEvents")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| values.len() >= 2) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("SharedWorker receives WebSocket error and close events");
            let events = messages.as_array().expect("SharedWorker events are an array");
            assert!(events.contains(&serde_json::json!(["error"])));
            assert!(events.contains(&serde_json::json!(["close", 1006, false])));
            assert!(!events.contains(&serde_json::json!(["open"])));
            assert_eq!(
                session
                    .script("await fetch('/verify').then(response => response.text())")
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
            server.await.unwrap();
            for (name, value) in [
                ("shared_parent_seed", "page"),
                ("shared_worker_entry", "worker"),
                ("shared_rejected", "accepted"),
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

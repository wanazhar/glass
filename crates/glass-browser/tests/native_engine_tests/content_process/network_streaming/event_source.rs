use super::*;

#[tokio::test]
async fn native_content_process_delivers_report_only_connect_events_for_event_source() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<script>globalThis.cspReports = []; addEventListener('securitypolicyviolation', event => cspReports.push([event.effectiveDirective, event.blockedURI, event.disposition])); globalThis.source = new EventSource('/events');</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy-Report-Only: connect-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/events"));
        let body = "data: event-source-ok\r\n\r\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                "await new Promise((resolve, reject) => { source.onmessage = event => { source.close(); resolve([event.data, cspReports]); }; source.onerror = event => reject(new Error(String(event.message || 'event source failed'))); })",
            )
            .await
            .unwrap(),
        serde_json::json!(["event-source-ok", [["connect-src", format!("http://{address}/events"), "report"]]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_page_event_source_redirect_cookies_use_parent_authority() {
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
                "glass-native-eventsource-redirect-cookies-{}-profile.json",
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
                tokio::time::timeout(Duration::from_secs(30), async move {
                    let (mut stream, _) = page_listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    assert_eq!(request.split_whitespace().next(), Some("GET"));
                    assert_eq!(request.split_whitespace().nth(1), Some("/page"));
                    let body = format!(
                        r#"<script>
globalThis.eventSourceMessages = [];
globalThis.eventSource = new EventSource("{page_api_origin}/redirect", {{ withCredentials: true }});
eventSource.addEventListener('open', () => eventSourceMessages.push('open'));
eventSource.addEventListener('message', event => {{
  eventSourceMessages.push(['message', event.data]);
  eventSource.close();
}});
eventSource.addEventListener('error', () => eventSourceMessages.push('error'));
</script>"#
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                })
                .await
                .expect("page response and redirected EventSource stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for (path, expected_cookie) in [
                        ("/redirect", "event_seed=initial"),
                        ("/events", "event_redirect=accepted"),
                        ("/verify", "event_redirect=accepted"),
                    ] {
                        let (mut stream, _) = tokio::time::timeout(
                            Duration::from_secs(30),
                            api_listener.accept(),
                        )
                        .await
                        .expect("EventSource redirect and authorized page requests arrive")
                        .unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(path));
                        assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                        assert!(header_value(&request, "access-control-request-method").is_none());
                        if path != "/verify" {
                            assert_eq!(
                                header_value(&request, "accept"),
                                Some("text/event-stream")
                            );
                        }
                        let cookie = header_value(&request, "cookie").unwrap_or_default();
                        assert!(cookie.contains("event_seed=initial"), "{path} missed seed cookie: {cookie}");
                        if path == "/redirect" {
                            assert!(!cookie.contains("event_redirect=accepted"));
                        } else {
                            assert!(cookie.contains(expected_cookie), "{path} missed redirect cookie: {cookie}");
                        }

                        let (status, extra_headers, content_type, body) = match path {
                            "/redirect" => (
                                "302 Found",
                                format!(
                                    "Location: /events\r\nSet-Cookie: event_redirect=accepted; HttpOnly; Path=/; SameSite=Lax\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                                ),
                                "text/plain",
                                "redirect",
                            ),
                            "/events" => (
                                "200 OK",
                                format!(
                                    "Access-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                                ),
                                "text/event-stream",
                                "data: redirected\r\n\r\n",
                            ),
                            _ => (
                                "200 OK",
                                format!(
                                    "Access-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                                ),
                                "text/plain",
                                "authorized",
                            ),
                        };
                        let response = format!(
                            "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                            panic!("unexpected EventSource redirect reconnect: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("EventSource redirect-cookie requests stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            let messages = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let messages = session
                        .script("eventSourceMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages == serde_json::json!(["open", ["message", "redirected"]]) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("redirected EventSource delivers authorized stream data");
            assert_eq!(messages, serde_json::json!(["open", ["message", "redirected"]]));
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
                ("event_seed", "initial"),
                ("event_redirect", "accepted"),
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
#[test]
fn native_content_process_page_event_source_omits_cross_origin_cookies_by_default() {
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
                "glass-native-eventsource-default-credentials-{}-profile.json",
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
                tokio::time::timeout(Duration::from_secs(30), async move {
                    let (mut stream, _) = page_listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    assert_eq!(request.split_whitespace().next(), Some("GET"));
                    assert_eq!(request.split_whitespace().nth(1), Some("/page"));
                    let body = format!(
                        r#"<script>
globalThis.eventSourceMessages = [];
globalThis.eventSource = new EventSource("{page_api_origin}/events");
globalThis.eventSourceCredentials = eventSource.withCredentials;
eventSource.addEventListener('open', () => eventSourceMessages.push('open'));
eventSource.addEventListener('message', event => {{
  eventSourceMessages.push(['message', event.data]);
  eventSource.close();
}});
eventSource.addEventListener('error', () => eventSourceMessages.push('error'));
</script>"#
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                })
                .await
                .expect("page response and default EventSource stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for (path, expected_cookie) in [
                        ("/events", None),
                        ("/verify", Some("event_seed=initial")),
                    ] {
                        let (mut stream, _) = tokio::time::timeout(
                            Duration::from_secs(30),
                            api_listener.accept(),
                        )
                        .await
                        .expect("EventSource and authorized page requests reach parent network")
                        .unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(path));
                        assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                        assert!(header_value(&request, "access-control-request-method").is_none());
                        let cookie = header_value(&request, "cookie");
                        match expected_cookie {
                            None => assert!(cookie.is_none(), "default cross-origin EventSource sent cookies: {cookie:?}"),
                            Some(expected) => {
                                let cookie = cookie.unwrap_or_default();
                                assert!(cookie.contains(expected), "authorized Fetch missed {expected}: {cookie}");
                                assert!(!cookie.contains("eventsource_uncredentialed=accepted"), "unauthorized EventSource cookie reached parent request: {cookie}");
                            }
                        }

                        let (cors_headers, set_cookie, content_type, body) = if path == "/events" {
                            assert_eq!(header_value(&request, "accept"), Some("text/event-stream"));
                            (
                                "Access-Control-Allow-Origin: *\r\n",
                                "Set-Cookie: eventsource_uncredentialed=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/event-stream",
                                "data: public\r\n\r\n",
                            )
                        } else {
                            assert_eq!(cookie, Some("event_seed=initial"));
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
                            panic!("unexpected default EventSource reconnect: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("default EventSource credential checks stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            let messages = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let messages = session
                        .script("eventSourceMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages == serde_json::json!(["open", ["message", "public"]]) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("CORS permits the default-credential EventSource stream");
            assert_eq!(messages, serde_json::json!(["open", ["message", "public"]]));
            assert_eq!(
                session.script("eventSourceCredentials").await.unwrap().value,
                serde_json::json!(false)
            );
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
            assert!(cookies.iter().any(|cookie| {
                cookie.name == "event_seed" && cookie.value == "initial" && cookie.http_only
            }));
            assert!(!cookies
                .iter()
                .any(|cookie| cookie.name == "eventsource_uncredentialed"));

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
fn native_content_process_page_event_source_omits_redirect_cookies_by_default() {
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
                "glass-native-eventsource-default-redirect-credentials-{}-profile.json",
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
                tokio::time::timeout(Duration::from_secs(30), async move {
                    let (mut stream, _) = page_listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    assert_eq!(request.split_whitespace().next(), Some("GET"));
                    assert_eq!(request.split_whitespace().nth(1), Some("/page"));
                    let body = format!(
                        r#"<script>
globalThis.eventSourceMessages = [];
globalThis.eventSource = new EventSource("{page_api_origin}/redirect");
globalThis.eventSourceCredentials = eventSource.withCredentials;
eventSource.addEventListener('open', () => eventSourceMessages.push('open'));
eventSource.addEventListener('message', event => {{
  eventSourceMessages.push(['message', event.data]);
  eventSource.close();
}});
eventSource.addEventListener('error', () => eventSourceMessages.push('error'));
</script>"#
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                })
                .await
                .expect("page response and default EventSource stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for path in ["/redirect", "/events", "/verify"] {
                        let (mut stream, _) = tokio::time::timeout(
                            Duration::from_secs(30),
                            api_listener.accept(),
                        )
                        .await
                        .expect("redirect, stream, and authorized page requests arrive")
                        .unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(path));
                        assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                        assert!(header_value(&request, "access-control-request-method").is_none());
                        let cookie = header_value(&request, "cookie");
                        if path == "/verify" {
                            let cookie = cookie.unwrap_or_default();
                            assert!(cookie.contains("event_seed=initial"), "authorized Fetch missed seed: {cookie}");
                            assert!(!cookie.contains("eventsource_redirect=must-not-store"), "uncredentialed redirect cookie reached parent request: {cookie}");
                        } else {
                            assert!(cookie.is_none(), "default cross-origin EventSource sent cookies to {path}: {cookie:?}");
                            assert_eq!(header_value(&request, "accept"), Some("text/event-stream"));
                        }

                        let (status, headers, content_type, body) = match path {
                            "/redirect" => (
                                "302 Found",
                                "Location: /events\r\nAccess-Control-Allow-Origin: *\r\nSet-Cookie: eventsource_redirect=must-not-store; HttpOnly; Path=/; SameSite=Lax\r\n".to_owned(),
                                "text/plain",
                                "redirect",
                            ),
                            "/events" => (
                                "200 OK",
                                "Access-Control-Allow-Origin: *\r\n".to_owned(),
                                "text/event-stream",
                                "data: public\r\n\r\n",
                            ),
                            _ => (
                                "200 OK",
                                format!("Access-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"),
                                "text/plain",
                                "authorized",
                            ),
                        };
                        let response = format!(
                            "HTTP/1.1 {status}\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                            panic!("unexpected default EventSource reconnect: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("default EventSource redirect credential checks stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            let messages = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let messages = session
                        .script("eventSourceMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages == serde_json::json!(["open", ["message", "public"]]) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("CORS permits the default-credential redirected stream");
            assert_eq!(messages, serde_json::json!(["open", ["message", "public"]]));
            assert_eq!(
                session.script("eventSourceCredentials").await.unwrap().value,
                serde_json::json!(false)
            );
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
            assert!(cookies.iter().any(|cookie| {
                cookie.name == "event_seed" && cookie.value == "initial" && cookie.http_only
            }));
            assert!(!cookies
                .iter()
                .any(|cookie| cookie.name == "eventsource_redirect"));

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
fn native_content_process_page_event_source_cors_errors_keep_parent_cookies() {
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
                "glass-native-eventsource-cors-{}-profile.json",
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
                tokio::time::timeout(Duration::from_secs(30), async move {
                    let (mut stream, _) = page_listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    assert_eq!(request.split_whitespace().next(), Some("GET"));
                    assert_eq!(request.split_whitespace().nth(1), Some("/page"));
                    let body = format!(
                        r#"<script>
globalThis.eventSourceMessages = [];
globalThis.eventSource = new EventSource("{page_api_origin}/events", {{ withCredentials: true }});
eventSource.addEventListener('open', () => eventSourceMessages.push('open'));
eventSource.addEventListener('message', event => eventSourceMessages.push(['message', event.data]));
eventSource.addEventListener('error', () => {{
  eventSourceMessages.push('error');
  eventSource.close();
}});
</script>"#
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: event_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                })
                .await
                .expect("page response and EventSource initialization stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for (path, expected_cookie) in [
                        ("/events", "event_seed=initial"),
                        ("/verify", "event_cors_rejected=accepted"),
                    ] {
                        let (mut stream, _) = tokio::time::timeout(
                            Duration::from_secs(30),
                            api_listener.accept(),
                        )
                        .await
                        .expect("EventSource and authorized page requests reach parent network")
                        .unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(path));
                        assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                        assert!(header_value(&request, "access-control-request-method").is_none());
                        let cookie = header_value(&request, "cookie").unwrap_or_default();
                        assert!(cookie.contains(expected_cookie), "{path} missed {expected_cookie}: {cookie}");

                        let (cors_headers, set_cookie, content_type, body) = if path == "/events" {
                            assert_eq!(
                                header_value(&request, "accept"),
                                Some("text/event-stream")
                            );
                            (
                                "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\n",
                                "Set-Cookie: event_cors_rejected=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "text/event-stream",
                                "retry: 30000\r\ndata: secret\r\n\r\n",
                            )
                        } else {
                            assert!(cookie.contains("event_seed=initial"));
                            assert!(cookie.contains("event_cors_rejected=accepted"));
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
                            panic!("unexpected EventSource reconnect or extra request: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("EventSource CORS-cookie requests stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            let event_source_messages = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let messages = session
                        .script("eventSourceMessages")
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
            .expect("CORS failure dispatches EventSource error without open or message");
            assert_eq!(event_source_messages, serde_json::json!(["error"]));
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
                ("event_seed", "initial"),
                ("event_cors_rejected", "accepted"),
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
#[tokio::test]
async fn native_content_process_drives_event_source_named_multiline_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-eventsource-cookie-{}.json",
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
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body></body><script>globalThis.pageCookie = document.cookie; globalThis.pageSource = new EventSource('/events');</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: session=page; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("GET"));
        assert_eq!(request.split_whitespace().nth(1), Some("/events"));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("accept") && value.trim() == "text/event-stream"
            })
        }));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("cookie")
                    && value
                        .split(';')
                        .any(|cookie| cookie.trim() == "session=page")
            })
        }));
        let first = b"retry: 5\rid: 42\revent: greeting\rdata: hello\r";
        let second = b"\ndata: world\r\r";
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nSet-Cookie: sse=connected; Path=/\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n",
            )
            .await
            .unwrap();
        stream
            .write_all(format!("{:X}\r\n", first.len()).as_bytes())
            .await
            .unwrap();
        stream.write_all(first).await.unwrap();
        stream.write_all(b"\r\n").await.unwrap();
        stream
            .write_all(format!("{:X}\r\n", second.len()).as_bytes())
            .await
            .unwrap();
        stream.write_all(second).await.unwrap();
        stream.write_all(b"\r\n").await.unwrap();
        let mut close_probe = [0_u8; 1];
        let _ =
            tokio::time::timeout(Duration::from_millis(100), stream.read(&mut close_probe)).await;

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/check"));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("cookie")
                    && value
                        .split(';')
                        .any(|cookie| cookie.trim() == "sse=connected")
            })
        }));
        let body = "cookie-ok";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await new Promise((resolve, reject) => { const source = globalThis.pageSource; source.addEventListener('greeting', event => { document.body.setAttribute('data-event', event.type); document.body.setAttribute('data-last-id', event.lastEventId); document.body.setAttribute('data-message', event.data); source.close(); resolve([event.type, event.data, event.lastEventId, source.readyState]); }); source.onerror = event => reject(new Error('event source failed: ' + event.message)); })",
            )
            .await
            .unwrap(),
        serde_json::json!(["greeting", "hello\nworld", "42", 2])
    );
    assert!(
        engine
            .cookies_async()
            .await
            .unwrap()
            .iter()
            .any(|cookie| cookie.name == "sse" && cookie.value == "connected")
    );
    assert_eq!(
        engine
            .evaluate_async("[document.body.getAttribute('data-event'), document.body.getAttribute('data-last-id'), document.body.getAttribute('data-message')]")
            .await
            .unwrap(),
        serde_json::json!(["greeting", "42", "hello\nworld"])
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/check').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("cookie-ok")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
    let storage_profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&profile_path).unwrap()).unwrap();
    let cookie_profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&cookie_profile_path).unwrap()).unwrap();
    assert_eq!(storage_profile["cookies"], serde_json::json!([]));
    assert!(cookie_profile["cookies"].as_array().is_some_and(|cookies| {
        cookies
            .iter()
            .any(|cookie| cookie["name"] == "sse" && cookie["value"] == "connected")
    }));
    for path in [
        profile_path,
        cookie_profile_path,
        lock_path,
        cookie_lock_path,
        events_path,
        readers_path,
    ] {
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_content_process_dispatches_large_event_source_event_as_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = r#"<script>globalThis.largeLength = 0; globalThis.largeEvent = new Promise((resolve, reject) => { globalThis.source = new EventSource('/events'); source.onmessage = event => { largeLength = event.data.length; source.close(); resolve(largeLength); }; source.onerror = () => { largeLength = -1; reject(new Error('large EventSource event failed')); }; });</script>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/events"));
        let body = format!("data: {}\r\n\r\n", "x".repeat(20_000));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
        engine.evaluate_async("await largeEvent").await.unwrap(),
        serde_json::json!(20_000)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

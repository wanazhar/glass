use super::*;

#[test]
fn native_runtime_page_response_cookie_changes_reach_all_live_profile_contexts() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-page-cookie-fanout-{}-profile.json",
                std::process::id()
            ));
            let isolated_profile_path = std::env::temp_dir().join(format!(
                "glass-native-page-cookie-fanout-{}-isolated.json",
                std::process::id()
            ));
            let profile_lock_path = profile_path.with_extension("lock");
            let isolated_lock_path = isolated_profile_path.with_extension("lock");
            for path in [
                &profile_path,
                &profile_lock_path,
                &isolated_profile_path,
                &isolated_lock_path,
            ] {
                let _ = fs::remove_file(path);
            }

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let mut requests: Vec<(String, Option<String>)> = Vec::new();
                for _ in 0..12 {
                    let (mut stream, _) = tokio::time::timeout(
                        Duration::from_secs(30),
                        listener.accept(),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "page-cookie request idle timeout after paths {:?}",
                            requests
                                .iter()
                                .map(|(path, _)| path.as_str())
                                .collect::<Vec<_>>()
                        )
                    })
                    .unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request
                        .split_whitespace()
                        .nth(1)
                        .expect("page-cookie request includes a URL")
                        .to_owned();
                    let cookie = request
                        .lines()
                        .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                        .and_then(|line| line.split_once(':'))
                        .map(|(_, value)| value.trim().to_owned());
                    let (extra_headers, content_type, body): (&str, &str, &str) =
                        match path.as_str() {
                            "/writer-page" => (
                                "",
                                "text/html",
                                r#"<script>
globalThis.pageCookieWorkerMessages = [];
globalThis.pageCookieWorker = new SharedWorker('/cookie-observer.js', {
  name: 'page-response-cookie-observer', type: 'module', credentials: 'include',
});
pageCookieWorker.port.addEventListener('message', event => pageCookieWorkerMessages.push(event.data));
pageCookieWorker.port.start();
</script>"#,
                            ),
                            "/cookie-observer.js" => (
                                "",
                                "application/javascript",
                                "globalThis.onconnect = event => { const port = event.ports[0]; port.addEventListener('message', message => { if (message.data === 'check') { port.postMessage({ stage: 'received' }); return; } if (message.data !== 'load') return; import('/observe-shared.js').then(module => port.postMessage({ body: module.cookieProbeResult }), error => port.postMessage({ error: String(error) })); }); port.start(); port.postMessage('ready'); };",
                            ),
                            "/peer-page" => {
                                ("", "text/html", "<iframe src='/peer-frame'></iframe>")
                            }
                            "/peer-frame" => ("", "text/html", "<p>live frame</p>"),
                            "/isolated-page" => ("", "text/html", "<p>isolated</p>"),
                            "/reload-page" => ("", "text/html", "<p>profile reload</p>"),
                            "/set-cookie" => (
                                concat!(
                                    "Set-Cookie: native_http_only=secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                                    "Set-Cookie: native_visible=first; Path=/; SameSite=Lax\r\n",
                                    "Set-Cookie: native_visible=final; Path=/; SameSite=Lax\r\n",
                                    "Set-Cookie: native_deleted=temporary; Path=/; SameSite=Lax\r\n",
                                    "Set-Cookie: native_deleted=; Max-Age=0; Path=/; SameSite=Lax\r\n",
                                ),
                                "text/plain",
                                "set",
                            ),
                            "/observe-shared.js" => (
                                "",
                                "application/javascript",
                                "export const cookieProbeResult = 'shared-observed';",
                            ),
                            "/observe-peer" => ("", "text/plain", "peer-observed"),
                            "/observe-frame" => ("", "text/plain", "frame-observed"),
                            "/observe-isolated" => ("", "text/plain", "isolated-observed"),
                            "/observe-reload" => ("", "text/plain", "reload-observed"),
                            other => panic!("unexpected page-cookie request: {other}"),
                        };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    requests.push((path, cookie));
                }
                requests
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/writer-page")),
            )
            .await
            .unwrap();
            let peer = session
                .native_create_target(&format!("http://{address}/peer-page"))
                .await
                .unwrap();
            session.native_select_target(&peer.id).await.unwrap();
            let peer_frames = session.native_list_frames().await.unwrap();
            let child_frame = peer_frames
                .iter()
                .find(|frame| frame.parent_id.is_some())
                .expect("the peer page has a live child frame")
                .id
                .clone();
            session.native_select_frame(&child_frame).await.unwrap();

            let isolated = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(isolated_profile_path.clone())
                    .with_initial_url(format!("http://{address}/isolated-page")),
            )
            .await
            .unwrap();
            session.native_select_target("native-context").await.unwrap();
            let ready = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let messages = session
                        .script("pageCookieWorkerMessages.slice()")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| {
                        values.first() == Some(&serde_json::json!("ready"))
                    }) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the live SharedWorker cookie observer connects");
            assert_eq!(ready, serde_json::json!(["ready"]));
            assert_eq!(
                session
                    .script("await fetch('/set-cookie').then(response => response.text())")
                .await
                .unwrap()
                .value,
                serde_json::json!("set")
            );
            session
                .script("pageCookieWorker.port.postMessage('check'); true")
                .await
                .unwrap();
            let worker_acknowledgement = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let messages = session
                        .script("pageCookieWorkerMessages.slice()")
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
            .expect("the already-running SharedWorker receives the page message");
            assert_eq!(
                worker_acknowledgement,
                serde_json::json!(["ready", {"stage": "received"}])
            );
            session
                .script("pageCookieWorker.port.postMessage('load'); true")
                .await
                .unwrap();
            let shared_response = tokio::time::timeout(Duration::from_secs(45), async {
                loop {
                    let messages = session
                        .script("pageCookieWorkerMessages.slice()")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| values.len() == 3) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await;
            let shared_response = shared_response
                .expect("the live SharedWorker loads a script with the updated cookie jar");
            assert_eq!(
                shared_response,
                serde_json::json!([
                    "ready",
                    {"stage": "received"},
                    {"body": "shared-observed"}
                ])
            );

            session.native_select_target(&peer.id).await.unwrap();
            let peer_state = session
                .script(
                    "await fetch('/observe-peer').then(async response => ({ cookie: document.cookie, body: await response.text() }))",
                )
                .await
                .unwrap()
                .value;
            assert_eq!(peer_state["body"], "peer-observed");
            assert!(peer_state["cookie"]
                .as_str()
                .unwrap()
                .contains("native_visible=final"));
            assert!(!peer_state["cookie"]
                .as_str()
                .unwrap()
                .contains("native_http_only="));
            assert!(!peer_state["cookie"]
                .as_str()
                .unwrap()
                .contains("native_deleted="));

            session.native_select_frame(&child_frame).await.unwrap();
            let frame_state = session
                .script(
                    "await fetch('/observe-frame').then(async response => ({ cookie: document.cookie, body: await response.text() }))",
                )
                .await
                .unwrap()
                .value;
            assert_eq!(frame_state["body"], "frame-observed");
            assert!(frame_state["cookie"]
                .as_str()
                .unwrap()
                .contains("native_visible=final"));
            assert!(!frame_state["cookie"]
                .as_str()
                .unwrap()
                .contains("native_http_only="));
            assert!(!frame_state["cookie"]
                .as_str()
                .unwrap()
                .contains("native_deleted="));

            assert_eq!(
                isolated
                    .script("await fetch('/observe-isolated').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("isolated-observed")
            );
            assert_eq!(
                isolated.script("document.cookie").await.unwrap().value,
                serde_json::json!("")
            );

            session.close().await.unwrap();
            isolated.close().await.unwrap();
            let reloaded = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/reload-page")),
            )
            .await
            .unwrap();
            let reloaded_visible_cookies = reloaded.script("document.cookie").await.unwrap().value;
            assert!(reloaded_visible_cookies
                .as_str()
                .unwrap()
                .contains("native_visible=final"));
            assert!(!reloaded_visible_cookies
                .as_str()
                .unwrap()
                .contains("native_deleted="));
            assert_eq!(
                reloaded
                    .script("await fetch('/observe-reload').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("reload-observed")
            );
            reloaded.close().await.unwrap();
            let requests = server.await.unwrap();
            let cookie_for = |path: &str| {
                requests
                    .iter()
                    .find(|(request_path, _)| request_path == path)
                    .unwrap_or_else(|| panic!("missing request {path}"))
                    .1
                    .as_deref()
                    .unwrap_or("")
                    .to_owned()
            };
            for path in [
                "/observe-shared.js",
                "/observe-peer",
                "/observe-frame",
                "/observe-reload",
            ] {
                let cookie = cookie_for(path);
                assert!(cookie.contains("native_http_only=secret"));
                assert!(cookie.contains("native_visible=final"));
                assert!(!cookie.contains("native_deleted="));
            }
            let reload_navigation_cookie = cookie_for("/reload-page");
            assert!(reload_navigation_cookie.contains("native_http_only=secret"));
            assert!(reload_navigation_cookie.contains("native_visible=final"));
            assert!(!reload_navigation_cookie.contains("native_deleted="));
            assert!(!cookie_for("/observe-isolated").contains("native_"));

            for path in [
                &profile_path,
                &profile_lock_path,
                &isolated_profile_path,
                &isolated_lock_path,
            ] {
                let _ = fs::remove_file(path);
            }
        });
    });
}

#[test]
fn native_runtime_cookie_changes_sync_between_live_profile_sessions() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-cross-session-cookie-{}-profile.json",
                std::process::id()
            ));
            let isolated_profile_path = std::env::temp_dir().join(format!(
                "glass-native-cross-session-cookie-{}-isolated.json",
                std::process::id()
            ));
            let profile_sidecars = [
                profile_path.clone(),
                profile_path.with_extension("lock"),
                profile_path.with_extension("events"),
                profile_path.with_extension("readers"),
                profile_path.with_extension("clients"),
                isolated_profile_path.clone(),
                isolated_profile_path.with_extension("lock"),
                isolated_profile_path.with_extension("events"),
                isolated_profile_path.with_extension("readers"),
                isolated_profile_path.with_extension("clients"),
            ];
            for path in &profile_sidecars {
                let _ = fs::remove_file(path);
            }

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let mut requests = Vec::new();
                for _ in 0..8 {
                    let (mut stream, _) = tokio::time::timeout(
                        Duration::from_secs(30),
                        listener.accept(),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "cross-session cookie request idle timeout after paths {:?}",
                            requests
                                .iter()
                                .map(|(path, _): &(String, Option<String>)| path.as_str())
                                .collect::<Vec<_>>()
                        )
                    })
                    .unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request
                        .split_whitespace()
                        .nth(1)
                        .expect("cross-session request includes a URL")
                        .to_owned();
                    let cookie = request
                        .lines()
                        .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                        .and_then(|line| line.split_once(':'))
                        .map(|(_, value)| value.trim().to_owned());
                    let (extra_headers, content_type, body): (&str, &str, &str) = match path.as_str()
                    {
                        "/writer" | "/reader" | "/isolated" => ("", "text/html", "<p>ready</p>"),
                        "/set" => (
                            concat!(
                                "Set-Cookie: cross_session=first; Path=/; SameSite=Lax\r\n",
                                "Set-Cookie: cross_session=latest; Path=/; SameSite=Lax\r\n",
                                "Set-Cookie: cross_http_only=secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "Set-Cookie: cross_removed=temporary; Path=/; SameSite=Lax\r\n",
                                "Set-Cookie: cross_removed=; Max-Age=0; Path=/; SameSite=Lax\r\n",
                            ),
                            "text/plain",
                            "set",
                        ),
                        "/check-shared" => ("", "text/plain", "shared"),
                        "/check-isolated" => ("", "text/plain", "isolated"),
                        "/check-import" => ("", "text/plain", "imported"),
                        "/check-clear" => ("", "text/plain", "cleared"),
                        other => panic!("unexpected cross-session cookie request: {other}"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    requests.push((path, cookie));
                }
                requests
            });

            let writer = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/writer")),
            )
            .await
            .unwrap();
            let reader = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/reader")),
            )
            .await
            .unwrap();
            let isolated = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(isolated_profile_path.clone())
                    .with_initial_url(format!("http://{address}/isolated")),
            )
            .await
            .unwrap();

            assert_eq!(
                writer
                    .script("await fetch('/set').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("set")
            );
            assert_eq!(
                reader
                    .script("await fetch('/check-shared').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("shared")
            );
            assert_eq!(
                isolated
                    .script("await fetch('/check-isolated').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("isolated")
            );
            writer
                .native_set_cookies(&[Cookie {
                    name: "api_import".into(),
                    value: "visible".into(),
                    domain: address.ip().to_string(),
                    path: "/".into(),
                    expires: 0.0,
                    http_only: true,
                    secure: false,
                    same_site: Some("Lax".into()),
                    is_session: true,
                    size: None,
                    priority: None,
                }])
                .await
                .unwrap();
            assert_eq!(
                reader
                    .script("await fetch('/check-import').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("imported")
            );
            reader
                .native_set_cookies(&[Cookie {
                    name: "peer_added".into(),
                    value: "peer_value".into(),
                    domain: address.ip().to_string(),
                    path: "/".into(),
                    expires: 0.0,
                    http_only: false,
                    secure: false,
                    same_site: Some("Lax".into()),
                    is_session: true,
                    size: None,
                    priority: None,
                }])
                .await
                .unwrap();
            writer.native_clear_cookies().await.unwrap();
            assert_eq!(
                reader
                    .script("await fetch('/check-clear').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("cleared")
            );

            writer.close().await.unwrap();
            reader.close().await.unwrap();
            isolated.close().await.unwrap();
            let requests = server.await.unwrap();
            let cookie_for = |path: &str| {
                requests
                    .iter()
                    .find(|(request_path, _)| request_path == path)
                    .unwrap_or_else(|| panic!("missing request {path}"))
                    .1
                    .as_deref()
                    .unwrap_or("")
                    .to_owned()
            };
            let shared_cookie = cookie_for("/check-shared");
            assert!(shared_cookie.contains("cross_session=latest"));
            assert!(shared_cookie.contains("cross_http_only=secret"));
            assert!(!shared_cookie.contains("cross_removed="));
            let imported_cookie = cookie_for("/check-import");
            assert!(imported_cookie.contains("cross_session=latest"));
            assert!(imported_cookie.contains("cross_http_only=secret"));
            assert!(
                imported_cookie.contains("api_import=visible"),
                "API import was not delivered; request cookies={imported_cookie:?}"
            );
            assert!(!imported_cookie.contains("cross_removed="));
            let cleared_cookie = cookie_for("/check-clear");
            assert!(
                cleared_cookie.is_empty(),
                "cookie clear was not delivered; request cookies={cleared_cookie:?}"
            );
            let isolated_cookie = cookie_for("/check-isolated");
            assert!(!isolated_cookie.contains("cross_session="));
            assert!(!isolated_cookie.contains("cross_http_only="));

            for path in &profile_sidecars {
                let _ = fs::remove_file(path);
            }
        });
    });
}

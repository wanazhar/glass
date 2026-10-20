use super::*;

#[test]
fn native_runtime_shared_worker_module_credentials_cover_redirects_and_graph_cookies() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let cross_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let cross_address = cross_listener.local_addr().unwrap();
            let page_origin = format!("http://{address}");
            let cross_origin = format!("http://{cross_address}");

            let primary_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(40), async move {
                    let mut requests = Vec::new();
                    for _ in 0..10 {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .expect("SharedWorker request includes a URL")
                            .to_owned();
                        let cookie = request
                            .lines()
                            .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                            .and_then(|line| line.split_once(':'))
                            .map(|(_, value)| value.trim().to_owned());
                        let (status, extra_headers, content_type, body):
                            (&str, String, &str, String) = if path == "/page" {
                            (
                                "200 OK",
                                "Set-Cookie: root=seed; Path=/; SameSite=Lax\r\n".to_owned(),
                                "text/html",
                                r#"<script>
globalThis.sharedWorkerResults = [];
globalThis.sharedWorkerErrors = [];
for (const mode of ['omit', 'same-origin', 'include']) {
  const worker = new SharedWorker(`/worker-redirect-${mode}.js`, {
    name: `credentials-${mode}`, type: 'module', credentials: mode,
  });
  worker.addEventListener('error', event => sharedWorkerErrors.push(event.message));
  worker.port.addEventListener('message', event => sharedWorkerResults.push(event.data));
  worker.port.start();
}
const recoverOptions = {
  name: 'credentials-cors-recovery', type: 'module', credentials: 'include',
};
const rejected = new SharedWorker('/worker-final-recover.js', recoverOptions);
rejected.addEventListener('error', () => {
  sharedWorkerErrors.push('cors');
  const retry = new SharedWorker('/worker-final-recover.js', recoverOptions);
  retry.port.addEventListener('message', event => sharedWorkerResults.push(event.data));
  retry.port.start();
});
globalThis.classicWorkerResults = [];
const classic = new SharedWorker('/classic-worker.js', {
  name: 'classic-credentials-option-is-ignored', credentials: 'omit',
});
classic.port.addEventListener('message', event => classicWorkerResults.push(event.data));
classic.port.start();
</script>"#
                                    .to_owned(),
                            )
                        } else if let Some(mode) = path
                            .strip_prefix("/worker-redirect-")
                            .and_then(|path| path.strip_suffix(".js"))
                        {
                            let cookie_name = mode.replace('-', "_");
                            (
                                "302 Found",
                                format!("Location: /worker-final-{mode}.js\r\nSet-Cookie: hop_{cookie_name}=accepted; Path=/; SameSite=Lax\r\n"),
                                "text/plain",
                                "redirecting".to_owned(),
                            )
                        } else if let Some(mode) = path
                            .strip_prefix("/worker-final-")
                            .and_then(|path| path.strip_suffix(".js"))
                        {
                            let body = format!(
                                "import {{ first }} from \"{cross_origin}/first.js?mode={mode}\";\nimport {{ later }} from \"{cross_origin}/later.js?mode={mode}\";\nglobalThis.onconnect = event => {{ const port = event.ports[0]; port.postMessage({{ mode: \"{mode}\", kind: 'static', first, later }}); import(\"{cross_origin}/dynamic.js?mode={mode}\").then(module => port.postMessage({{ mode: \"{mode}\", kind: 'dynamic', dynamic: module.dynamic }})); }};"
                            );
                            ("200 OK", String::new(), "application/javascript", body)
                        } else if path == "/classic-worker.js" {
                            (
                                "200 OK",
                                String::new(),
                                "application/javascript",
                                "globalThis.onconnect = event => event.ports[0].postMessage({ kind: 'classic-ready' });".to_owned(),
                            )
                        } else {
                            panic!("unexpected credentials-test request: {path}");
                        };
                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                        requests.push((path, cookie));
                    }
                    requests
                })
                .await
                .expect("SharedWorker root and redirect requests stay within their time bound")
            });

            let cross_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(40), async move {
                    let mut requests = Vec::new();
                    let mut recover_first_attempts = 0;
                    for _ in 0..13 {
                        let (mut stream, _) = cross_listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .expect("cross-origin module request includes a URL")
                            .to_owned();
                        let cookie = request
                            .lines()
                            .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                            .and_then(|line| line.split_once(':'))
                            .map(|(_, value)| value.trim().to_owned());
                        let mode = path
                            .split_once("mode=")
                            .map(|(_, mode)| mode)
                            .expect("module dependency request carries its credentials mode");
                        let deny_recovery_first = mode == "recover"
                            && path.starts_with("/first.js?")
                            && {
                                recover_first_attempts += 1;
                                recover_first_attempts == 1
                            };
                        let cors = if deny_recovery_first {
                            String::new()
                        } else if mode == "include" || mode == "recover" {
                            format!(
                                "Access-Control-Allow-Origin: {page_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                            )
                        } else {
                            "Access-Control-Allow-Origin: *\r\n".to_owned()
                        };
                        let (extra_headers, body) = if path.starts_with("/first.js?") {
                            let set_cookie = if mode == "include" || mode == "recover" {
                                if mode == "recover" {
                                    "Set-Cookie: cross=from-recover; Path=/; SameSite=Lax\r\n"
                                } else {
                                    "Set-Cookie: cross=from-include; Path=/; SameSite=Lax\r\n"
                                }
                            } else {
                                "Set-Cookie: cross=must-not-stick; Path=/; SameSite=Lax\r\n"
                            };
                            (set_cookie, "export const first = 'loaded';")
                        } else if path.starts_with("/later.js?") {
                            ("", "export const later = 'checked';")
                        } else if path.starts_with("/dynamic.js?") {
                            ("", "export const dynamic = 'loaded';")
                        } else {
                            panic!("unexpected cross-origin module request: {path}");
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\n{cors}{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                        requests.push((path, cookie));
                    }
                    requests
                })
                .await
                .expect("SharedWorker module-graph requests stay within their time bound")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let shared = session
                        .script("sharedWorkerResults.slice().sort((a, b) => a.mode.localeCompare(b.mode) || a.kind.localeCompare(b.kind))")
                        .await
                        .unwrap()
                        .value;
                    let classic = session
                        .script("classicWorkerResults")
                        .await
                        .unwrap()
                        .value;
                    let errors = session
                        .script("sharedWorkerErrors")
                        .await
                        .unwrap()
                        .value;
                    if shared.as_array().is_some_and(|values| values.len() == 8)
                        && classic.as_array().is_some_and(|values| values.len() == 1)
                        && errors.as_array().is_some_and(|values| values.len() == 1)
                    {
                        break (shared, classic);
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("all module modes and the classic SharedWorker must connect");
            let results = session
                .script("sharedWorkerResults.slice().sort((a, b) => a.mode.localeCompare(b.mode) || a.kind.localeCompare(b.kind))")
                .await
                .unwrap()
                .value;
            let classic_results = session
                .script("classicWorkerResults")
                .await
                .unwrap()
                .value;
            let errors = session
                .script("sharedWorkerErrors")
                .await
                .unwrap()
                .value;
            session.close().await.unwrap();

            let mut primary_requests = primary_server.await.unwrap();
            let mut cross_requests = cross_server.await.unwrap();
            primary_requests.sort_by(|left, right| left.0.cmp(&right.0));
            cross_requests.sort_by(|left, right| left.0.cmp(&right.0));

            assert_eq!(
                results,
                serde_json::json!([
                    {"mode": "include", "kind": "dynamic", "dynamic": "loaded"},
                    {"mode": "include", "kind": "static", "first": "loaded", "later": "checked"},
                    {"mode": "omit", "kind": "dynamic", "dynamic": "loaded"},
                    {"mode": "omit", "kind": "static", "first": "loaded", "later": "checked"},
                    {"mode": "recover", "kind": "dynamic", "dynamic": "loaded"},
                    {"mode": "recover", "kind": "static", "first": "loaded", "later": "checked"},
                    {"mode": "same-origin", "kind": "dynamic", "dynamic": "loaded"},
                    {"mode": "same-origin", "kind": "static", "first": "loaded", "later": "checked"},
                ])
            );
            assert_eq!(classic_results, serde_json::json!([{"kind": "classic-ready"}]));
            assert_eq!(errors, serde_json::json!(["cors"]));

            let primary_cookie = |path: &str| {
                primary_requests
                    .iter()
                    .find(|(request_path, _)| request_path == path)
                    .unwrap_or_else(|| panic!("missing primary request {path}"))
                    .1
                    .as_deref()
                    .unwrap_or("")
                    .to_owned()
            };
            assert!(!primary_cookie("/worker-redirect-omit.js").contains("root=seed"));
            assert!(!primary_cookie("/worker-final-omit.js").contains("root=seed"));
            for mode in ["same-origin", "include"] {
                assert!(primary_cookie(&format!("/worker-redirect-{mode}.js")).contains("root=seed"));
                let final_cookie = primary_cookie(&format!("/worker-final-{mode}.js"));
                assert!(final_cookie.contains("root=seed"));
                assert!(final_cookie.contains(&format!("hop_{}=accepted", mode.replace('-', "_"))));
            }
            assert!(primary_cookie("/classic-worker.js").contains("root=seed"));

            let cross_cookie = |path: &str| {
                cross_requests
                    .iter()
                    .find(|(request_path, _)| request_path == path)
                    .unwrap_or_else(|| panic!("missing cross-origin request {path}"))
                    .1
                    .as_deref()
                    .unwrap_or("")
                    .to_owned()
            };
            for mode in ["omit", "same-origin"] {
                assert!(!cross_cookie(&format!("/first.js?mode={mode}")).contains("root=seed"));
                assert!(!cross_cookie(&format!("/later.js?mode={mode}")).contains("cross=from-include"));
                assert!(!cross_cookie(&format!("/dynamic.js?mode={mode}")).contains("cross=from-include"));
            }
            let include_first = cross_cookie("/first.js?mode=include");
            assert!(include_first.contains("root=seed"));
            assert!(include_first.contains("hop_include=accepted"));
            assert!(cross_cookie("/later.js?mode=include").contains("cross=from-include"));
            assert!(cross_cookie("/dynamic.js?mode=include").contains("cross=from-include"));

            let recover_first_requests = cross_requests
                .iter()
                .filter(|(path, _)| path == "/first.js?mode=recover")
                .collect::<Vec<_>>();
            assert_eq!(recover_first_requests.len(), 2);
            assert!(recover_first_requests
                .iter()
                .all(|(_, cookie)| cookie.as_deref().unwrap_or("").contains("root=seed")));
            assert!(cross_cookie("/later.js?mode=recover").contains("cross=from-recover"));
            assert!(cross_cookie("/dynamic.js?mode=recover").contains("cross=from-recover"));
        });
    });
}

#[test]
fn native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-shared-worker-stale-cookie-{}.json",
                std::process::id()
            ));
            let lock_path = profile_path.with_extension("lock");
            for path in [&profile_path, &lock_path] {
                let _ = fs::remove_file(path);
            }
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
            let server = tokio::spawn(async move {
                let mut requests = Vec::new();
                loop {
                    let accepted = tokio::select! {
                        _ = &mut shutdown_receiver => break,
                        accepted = listener.accept() => accepted.unwrap(),
                    };
                    let (mut stream, _) = accepted;
                    let request = read_http_request(&mut stream).await;
                    let path = request
                        .split_whitespace()
                        .nth(1)
                        .expect("SharedWorker request includes a URL")
                        .to_owned();
                    let cookie = request
                        .lines()
                        .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                        .and_then(|line| line.split_once(':'))
                        .map(|(_, value)| value.trim().to_owned());
                    let (status, extra_headers, content_type, body):
                        (&str, &str, &str, &str) = match path.as_str() {
                        "/first" => (
                            "200 OK",
                            "Set-Cookie: worker_state=page-stale; Path=/; SameSite=Lax\r\n",
                            "text/html",
                            r#"<script>
globalThis.cookieWorkerMessages = [];
globalThis.connectCookieWorker = (path, name) => {
  const worker = new SharedWorker(path, {
    name, type: 'module', credentials: 'include',
  });
  worker.port.addEventListener('message', event => cookieWorkerMessages.push(event.data));
  worker.port.start();
};
</script>"#,
                        ),
                        "/second" => (
                            "200 OK",
                            "",
                            "text/html",
                            r#"<script>
globalThis.cookieWorkerMessages = [];
globalThis.connectCookieWorker = (path, name) => {
  const worker = new SharedWorker(path, {
    name, type: 'module', credentials: 'include',
  });
  worker.port.addEventListener('message', event => cookieWorkerMessages.push(event.data));
  worker.port.start();
};
</script>"#,
                        ),
                        "/writer.js" => (
                            "200 OK",
                            "Set-Cookie: worker_state=from-worker; Path=/; SameSite=Lax\r\n",
                            "application/javascript",
                            "globalThis.onconnect = event => event.ports[0].postMessage('writer-ready');",
                        ),
                        "/observe-before.js" => (
                            "200 OK",
                            "",
                            "application/javascript",
                            "globalThis.onconnect = event => event.ports[0].postMessage('before-ready');",
                        ),
                        "/delete.js" => (
                            "200 OK",
                            "Set-Cookie: worker_state=; Max-Age=0; Path=/; SameSite=Lax\r\n",
                            "application/javascript",
                            "globalThis.onconnect = event => event.ports[0].postMessage('delete-ready');",
                        ),
                        "/observe-after.js" => (
                            "200 OK",
                            "",
                            "application/javascript",
                            "globalThis.onconnect = event => event.ports[0].postMessage('after-ready');",
                        ),
                        other => panic!("unexpected SharedWorker cookie request: {other}"),
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                    .with_initial_url(format!("http://{address}/first")),
            )
            .await
            .unwrap();
            let second = session
                .native_create_target(&format!("http://{address}/second"))
                .await
                .unwrap();
            session.native_select_target("native-context").await.unwrap();
            session
                .script("connectCookieWorker('/writer.js', 'cookie-writer'); true")
                .await
                .unwrap();
            let first_messages = tokio::time::timeout(Duration::from_secs(25), async {
                loop {
                    let messages = session
                        .script("cookieWorkerMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| values.len() == 1) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the first SharedWorker writes its response cookie");
            assert_eq!(first_messages, serde_json::json!(["writer-ready"]));

            session.native_select_target(&second.id).await.unwrap();
            session
                .script("connectCookieWorker('/observe-before.js', 'cookie-observer-before'); true")
                .await
                .unwrap();
            let second_messages = tokio::time::timeout(Duration::from_secs(25), async {
                loop {
                    let messages = session
                        .script("cookieWorkerMessages")
                        .await
                        .unwrap()
                        .value;
                    if messages.as_array().is_some_and(|values| values.len() == 1) {
                        break messages;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("a second page with a stale snapshot sees the session update");
            assert_eq!(second_messages, serde_json::json!(["before-ready"]));
            session
                .script("connectCookieWorker('/delete.js', 'cookie-deleter'); true")
                .await
                .unwrap();
            let second_messages = tokio::time::timeout(Duration::from_secs(25), async {
                loop {
                    let messages = session
                        .script("cookieWorkerMessages")
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
            .expect("the second page accepts the SharedWorker cookie deletion");

            session.native_select_target("native-context").await.unwrap();
            session
                .script("connectCookieWorker('/observe-after.js', 'cookie-observer-after'); true")
                .await
                .unwrap();
            let first_messages = tokio::time::timeout(Duration::from_secs(25), async {
                loop {
                    let messages = session
                        .script("cookieWorkerMessages")
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
            .expect("a stale page snapshot does not resurrect the deleted cookie");
            session.close().await.unwrap();
            let _ = shutdown_sender.send(());

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
            assert_eq!(
                first_messages,
                serde_json::json!(["writer-ready", "after-ready"])
            );
            assert_eq!(
                second_messages,
                serde_json::json!(["before-ready", "delete-ready"])
            );
            assert!(cookie_for("/second").contains("worker_state=page-stale"));
            assert!(cookie_for("/writer.js").contains("worker_state=page-stale"));
            assert!(cookie_for("/observe-before.js").contains("worker_state=from-worker"));
            assert!(cookie_for("/delete.js").contains("worker_state=from-worker"));
            assert!(!cookie_for("/observe-after.js").contains("worker_state="));
            for path in [&profile_path, &lock_path] {
                let _ = fs::remove_file(path);
            }
        });
    });
}

#[test]
fn native_runtime_shared_worker_cookie_changes_reach_page_and_profile() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-shared-worker-cookie-sync-{}-profile.json",
                std::process::id()
            ));
            let lock_path = profile_path.with_extension("lock");
            for path in [&profile_path, &lock_path] {
                let _ = fs::remove_file(path);
            }
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    let mut requests = Vec::new();
                    for _ in 0..9 {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .expect("cookie synchronization request includes a URL")
                            .to_owned();
                        let cookie = request
                            .lines()
                            .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                            .and_then(|line| line.split_once(':'))
                            .map(|(_, value)| value.trim().to_owned());
                        let (extra_headers, content_type, body): (&str, &str, &str) =
                            match path.as_str() {
                                "/page" => (
                                    "",
                                    "text/html",
                                    r#"<script>
globalThis.cookieSyncChecks = [];
globalThis.httpOnlyHidden = !document.cookie.includes('durable=');
const worker = new SharedWorker('/writer.js', {
  name: 'profile-cookie-writer', type: 'module', credentials: 'include',
});
worker.port.addEventListener('message', async () => {
  httpOnlyHidden = !document.cookie.includes('durable=');
  const response = await fetch('/observe-live');
  cookieSyncChecks.push(await response.text());
});
worker.port.start();
</script>"#,
                                ),
                                "/writer.js" => (
                                    "Set-Cookie: durable=from-worker; HttpOnly; Path=/; SameSite=Lax\r\n",
                                    "application/javascript",
                                    "globalThis.onconnect = event => event.ports[0].postMessage('ready');",
                                ),
                                "/reload" => (
                                    "",
                                    "text/html",
                                    r#"<script>
globalThis.cookieSyncChecks = [];
globalThis.httpOnlyHidden = !document.cookie.includes('durable=');
fetch('/observe-reload').then(async response => {
  cookieSyncChecks.push(await response.text());
  const worker = new SharedWorker('/delete.js', {
    name: 'profile-cookie-deleter', type: 'module', credentials: 'include',
  });
  worker.port.addEventListener('message', async () => {
    httpOnlyHidden = !document.cookie.includes('durable=');
    const afterDelete = await fetch('/observe-delete');
    cookieSyncChecks.push(await afterDelete.text());
  });
  worker.port.start();
});
</script>"#,
                                ),
                                "/delete.js" => (
                                    "Set-Cookie: durable=; Max-Age=0; HttpOnly; Path=/; SameSite=Lax\r\n",
                                    "application/javascript",
                                    "globalThis.onconnect = event => event.ports[0].postMessage('deleted');",
                                ),
                                "/reload-after-delete" => (
                                    "",
                                    "text/html",
                                    "<script>globalThis.cookieSyncChecks = []; fetch('/observe-final').then(async response => cookieSyncChecks.push(await response.text()));</script>",
                                ),
                                "/observe-live"
                                | "/observe-reload"
                                | "/observe-delete"
                                | "/observe-final" => ("", "text/plain", "observed"),
                                other => panic!("unexpected SharedWorker profile request: {other}"),
                            };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                        requests.push((path, cookie));
                    }
                    requests
                })
                .await
                .expect("SharedWorker cookie profile synchronization stays bounded")
            });

            let first = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/page")),
            )
            .await
            .unwrap();
            let first_checks = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let checks = first
                        .script("cookieSyncChecks")
                        .await
                        .unwrap()
                        .value;
                    if checks.as_array().is_some_and(|values| values.len() == 1) {
                        break checks;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the owning page fetches after its worker connection");
            let first_cookie_hidden = first
                .script("httpOnlyHidden")
                .await
                .unwrap()
                .value;
            first.close().await.unwrap();

            let second = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/reload")),
            )
            .await
            .unwrap();
            let second_checks = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let checks = second
                        .script("cookieSyncChecks")
                        .await
                        .unwrap()
                        .value;
                    if checks.as_array().is_some_and(|values| values.len() == 2) {
                        break checks;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("profile reload and worker deletion complete");
            let second_cookie_hidden = second
                .script("httpOnlyHidden")
                .await
                .unwrap()
                .value;
            second.close().await.unwrap();

            let third = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/reload-after-delete")),
            )
            .await
            .unwrap();
            let final_checks = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let checks = third
                        .script("cookieSyncChecks")
                        .await
                        .unwrap()
                        .value;
                    if checks.as_array().is_some_and(|values| values.len() == 1) {
                        break checks;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("deleted cookie remains absent after a profile reload");
            third.close().await.unwrap();

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
            assert_eq!(first_checks, serde_json::json!(["observed"]));
            assert_eq!(
                second_checks,
                serde_json::json!(["observed", "observed"])
            );
            assert_eq!(final_checks, serde_json::json!(["observed"]));
            assert_eq!(first_cookie_hidden, serde_json::json!(true));
            assert_eq!(second_cookie_hidden, serde_json::json!(true));
            assert!(cookie_for("/observe-live").contains("durable=from-worker"));
            assert!(cookie_for("/observe-reload").contains("durable=from-worker"));
            assert!(!cookie_for("/observe-delete").contains("durable="));
            assert!(!cookie_for("/observe-final").contains("durable="));
            for path in [&profile_path, &lock_path] {
                let _ = fs::remove_file(path);
            }
        });
    });
}

#[test]
fn native_runtime_shared_worker_cookie_changes_reach_all_live_profile_contexts() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-shared-worker-cookie-fanout-{}-profile.json",
                std::process::id()
            ));
            let isolated_profile_path = std::env::temp_dir().join(format!(
                "glass-native-shared-worker-cookie-fanout-{}-isolated.json",
                std::process::id()
            ));
            let cookie_profile_path = std::path::PathBuf::from(format!(
                "{}.cookies",
                profile_path.display()
            ));
            let isolated_cookie_profile_path = std::path::PathBuf::from(format!(
                "{}.cookies",
                isolated_profile_path.display()
            ));
            let profile_lock_path = profile_path.with_extension("lock");
            let isolated_lock_path = isolated_profile_path.with_extension("lock");
            let cookie_profile_lock_path = cookie_profile_path.with_extension("lock");
            let isolated_cookie_profile_lock_path = isolated_cookie_profile_path.with_extension("lock");
            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &isolated_profile_path,
                &isolated_cookie_profile_path,
                &isolated_lock_path,
                &isolated_cookie_profile_lock_path,
            ] {
                let _ = fs::remove_file(path);
            }

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(60), async move {
                    let mut requests = Vec::new();
                    for _ in 0..10 {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .expect("live-context cookie request includes a URL")
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
globalThis.cookieFanoutMessages = [];
globalThis.startCookieFanout = () => {
  const worker = new SharedWorker('/cookie-writer.js', {
    name: 'live-profile-cookie-writer', type: 'module', credentials: 'include',
  });
  worker.port.addEventListener('message', event => cookieFanoutMessages.push(event.data));
  worker.port.start();
};
</script>"#,
                                ),
                                "/cookie-writer.js" => (
                                    "Set-Cookie: fanout=from-worker; Path=/; SameSite=Lax\r\n",
                                    "application/javascript",
                                    "globalThis.onconnect = event => { const port = event.ports[0]; port.postMessage('ready'); fetch('/cookie-fetch', { credentials: 'include' }).then(response => response.text()).then(body => port.postMessage(body), error => port.postMessage(String(error))); };",
                                ),
                                "/cookie-fetch" => (
                                    "Set-Cookie: fanout-fetch=from-fetch; HttpOnly; Path=/; SameSite=Lax\r\n",
                                    "text/plain",
                                    "fetch-observed",
                                ),
                                "/peer-page" => (
                                    "",
                                    "text/html",
                                    "<iframe src='/peer-frame'></iframe>",
                                ),
                                "/peer-frame" => (
                                    "",
                                    "text/html",
                                    "<p>live frame</p>",
                                ),
                                "/isolated-page" => ("", "text/html", "<p>isolated</p>"),
                                "/observe-peer" => ("", "text/plain", "peer-observed"),
                                "/observe-frame" => ("", "text/plain", "frame-observed"),
                                "/observe-isolated" => {
                                    ("", "text/plain", "isolated-observed")
                                }
                                "/reopened-profile" => {
                                    ("", "text/html", "<p>reopened profile</p>")
                                }
                                other => panic!("unexpected live-context cookie request: {other}"),
                            };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                        requests.push((path, cookie));
                    }
                    requests
                })
                .await
                .expect("live-context cookie fan-out remains bounded")
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
            assert_eq!(
                isolated.script("document.cookie").await.unwrap().value,
                serde_json::json!("")
            );

            session.native_select_target("native-context").await.unwrap();
            session
                .script("startCookieFanout(); true")
                .await
                .unwrap();
            let writer_messages = tokio::time::timeout(Duration::from_secs(25), async {
                loop {
                    let messages = session
                        .script("cookieFanoutMessages")
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
            .expect("the SharedWorker response cookie is accepted");
            assert_eq!(writer_messages, serde_json::json!(["ready", "fetch-observed"]));

            session.native_select_target(&peer.id).await.unwrap();
            let peer_observation = session
                .script(
                    "await fetch('/observe-peer').then(async response => ({ cookie: document.cookie, response: await response.text() }))",
                )
                .await
                .unwrap()
                .value;
            assert!(peer_observation["cookie"]
                .as_str()
                .unwrap()
                .contains("fanout=from-worker"));
            assert!(!peer_observation["cookie"]
                .as_str()
                .unwrap()
                .contains("fanout-fetch=from-fetch"));
            assert_eq!(peer_observation["response"], "peer-observed");

            session.native_select_frame(&child_frame).await.unwrap();
            let frame_observation = session
                .script(
                    "await fetch('/observe-frame').then(async response => ({ cookie: document.cookie, response: await response.text() }))",
                )
                .await
                .unwrap()
                .value;
            assert!(frame_observation["cookie"]
                .as_str()
                .unwrap()
                .contains("fanout=from-worker"));
            assert!(!frame_observation["cookie"]
                .as_str()
                .unwrap()
                .contains("fanout-fetch=from-fetch"));
            assert_eq!(frame_observation["response"], "frame-observed");

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

            let reopened = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("http://{address}/reopened-profile")),
            )
            .await
            .unwrap();
            let reopened_document_cookie = reopened.script("document.cookie").await.unwrap().value;
            assert!(reopened_document_cookie
                .as_str()
                .unwrap()
                .contains("fanout=from-worker"));
            assert!(!reopened_document_cookie
                .as_str()
                .unwrap()
                .contains("fanout-fetch=from-fetch"));
            reopened.close().await.unwrap();

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
            assert!(cookie_for("/observe-peer").contains("fanout=from-worker"));
            assert!(cookie_for("/cookie-fetch").contains("fanout=from-worker"));
            assert!(cookie_for("/observe-frame").contains("fanout=from-worker"));
            assert!(cookie_for("/observe-peer").contains("fanout-fetch=from-fetch"));
            assert!(cookie_for("/observe-frame").contains("fanout-fetch=from-fetch"));
            assert!(!cookie_for("/observe-isolated").contains("fanout="));
            assert!(cookie_for("/reopened-profile").contains("fanout=from-worker"));
            assert!(cookie_for("/reopened-profile").contains("fanout-fetch=from-fetch"));

            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &isolated_profile_path,
                &isolated_cookie_profile_path,
                &isolated_lock_path,
                &isolated_cookie_profile_lock_path,
            ] {
                let _ = fs::remove_file(path);
            }
        });
    });
}

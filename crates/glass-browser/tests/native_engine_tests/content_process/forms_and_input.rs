use super::*;

#[test]
fn native_content_process_form_post_target_preserves_payload() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_form_post_target_preserves_payload_inner());
    });
}

async fn native_content_process_form_post_target_preserves_payload_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            if path == "/form" {
                assert!(request.starts_with("GET /form HTTP/1.1"));
            } else {
                assert!(request.starts_with("POST /result HTTP/1.1"));
                assert!(request.lines().any(|line| {
                    line.eq_ignore_ascii_case("content-type: application/x-www-form-urlencoded")
                }));
                assert_eq!(request.split("\r\n\r\n").nth(1), Some("query=hello"));
            }
            let body = if path == "/form" {
                "<title>HTTP form opener</title><form id='form' action='/result' method='post' target='_blank'><input name='query' value='hello'><button id='go' type='submit'>Submit</button></form>"
            } else {
                "<title>HTTP form target</title><p>posted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .await
    .unwrap();
    session
        .script("document.getElementById('form').submit()")
        .await
        .unwrap();
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "HTTP form opener"
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let popup = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(popup.title, "HTTP form target");
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    session.close().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("form request fixture did not stop")
        .unwrap();
}
#[test]
fn native_content_process_multipart_form_post_target_preserves_binary_body() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_multipart_form_post_target_preserves_binary_body_inner(),
        );
    });
}

async fn native_content_process_multipart_form_post_target_preserves_binary_body_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_bytes(&mut stream).await;
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .unwrap();
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let body = &request[header_end..];
            if headers.starts_with("GET /form HTTP/1.1") {
                assert!(
                    !headers
                        .lines()
                        .any(|line| line.to_ascii_lowercase().starts_with("content-type:"))
                );
            } else {
                assert!(headers.starts_with("POST /upload HTTP/1.1"));
                assert!(headers.lines().any(|line| {
                    line.to_ascii_lowercase()
                        .starts_with("content-type: multipart/form-data; boundary=")
                }));
                assert!(
                    body.windows(b"name=\"query\"".len())
                        .any(|window| { window == b"name=\"query\"" })
                );
                assert!(
                    body.windows(b"\r\nhello\r\n".len())
                        .any(|window| { window == b"\r\nhello\r\n" })
                );
                assert!(body.ends_with(b"--\r\n"));
            }
            let body = if headers.starts_with("GET /form HTTP/1.1") {
                "<title>Multipart form opener</title><form id='form' action='/upload' method='post' enctype='multipart/form-data' target='_blank'><input name='query' value='hello'></form>"
            } else {
                "<title>Multipart form target</title><p>uploaded</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .await
    .unwrap();
    session
        .script("document.getElementById('form').submit()")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let popup = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(popup.title, "Multipart form target");
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_modifier_click_opens_background_target_and_exposes_flags() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_modifier_click_opens_background_target_and_exposes_flags_inner(),
        );
    });
}

async fn native_content_process_modifier_click_opens_background_target_and_exposes_flags_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("modifier-click navigation request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1);
            let (content_type, extra_headers, body) = match path {
                Some("/parent") => (
                    "text/html",
                    "",
                    "<title>HTTP modifier opener</title><a id='open' href='/child'>Open child</a><a id='download' download='modifier.txt' href='/download'>Download</a><script>globalThis.__modifierFlags = null; document.getElementById('open').addEventListener('click', event => { globalThis.__modifierFlags = [event.altKey, event.ctrlKey, event.metaKey, event.shiftKey]; });</script>",
                ),
                Some("/child") => (
                    "text/html",
                    "",
                    "<title>HTTP modifier child</title><p>background target</p>",
                ),
                Some("/download") => (
                    "application/octet-stream",
                    "Content-Disposition: attachment; filename=modifier.txt\r\n",
                    "native modifier download bytes",
                ),
                other => panic!("unexpected modifier-click request path: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    session
        .action(SemanticAction::ClickWithModifiers {
            target: "id=open".into(),
            modifiers: ClickModifiers {
                control: true,
                shift: true,
                ..ClickModifiers::default()
            },
        })
        .await
        .unwrap();

    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        format!("http://{address}/parent")
    );
    assert_eq!(
        session
            .script("globalThis.__modifierFlags")
            .await
            .unwrap()
            .value,
        serde_json::json!([false, true, false, true])
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(child.title, "HTTP modifier child");
    assert_eq!(child.opener_id.as_deref(), Some("native-context"));

    let before_download_targets = targets.len();
    session
        .action(SemanticAction::ClickWithModifiers {
            target: "id=download".into(),
            modifiers: ClickModifiers {
                control: true,
                ..ClickModifiers::default()
            },
        })
        .await
        .unwrap();
    assert_eq!(
        session.native_list_targets().await.unwrap().len(),
        before_download_targets,
        "the explicit download default must take precedence over modifier target creation"
    );
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let download_directory = std::env::temp_dir().join(format!(
        "glass-native-modifier-download-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&download_directory).unwrap();
    let destination = download_directory.join("modifier.txt");
    session
        .native_wait_for_download(&download_directory, Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"native modifier download bytes"
    );
    std::fs::remove_file(destination).unwrap();
    std::fs::remove_dir(download_directory).unwrap();
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_keyboard_link_default_actions_open_blank_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_keyboard_link_default_actions_open_blank_target_inner(),
        );
    });
}

async fn native_content_process_keyboard_link_default_actions_open_blank_target_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("keyboard popup request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/popup-parent") => {
                    "<title>HTTP keyboard opener</title><a id='open' target='_blank' href='/popup-child'>Open popup</a><script>globalThis.cancelPopup = true; document.getElementById('open').addEventListener('click', event => { if (cancelPopup) event.preventDefault(); });</script>"
                }
                Some("/popup-child") => "<title>HTTP keyboard popup</title><p>keyboard popup</p>",
                other => panic!("unexpected keyboard popup request path: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/popup-parent")),
    )
    .await
    .unwrap();
    session
        .script("document.getElementById('open').focus(); true")
        .await
        .unwrap();
    let canceled = session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert!(canceled.accepted);
    assert_eq!(session.native_list_targets().await.unwrap().len(), 1);
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        format!("http://{address}/popup-parent")
    );

    session
        .script("globalThis.cancelPopup = false; document.getElementById('open').focus(); true")
        .await
        .unwrap();
    let opened = session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert!(opened.accepted);
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let opener = targets.iter().find(|target| target.active).unwrap();
    assert_eq!(opener.url, format!("http://{address}/popup-parent"));
    assert_eq!(opener.title, "HTTP keyboard opener");
    let popup = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(popup.url, format!("http://{address}/popup-child"));
    assert_eq!(popup.title, "HTTP keyboard popup");
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_form_target_creates_popup_and_keeps_opener() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_form_target_creates_popup_and_keeps_opener_inner());
    });
}

async fn native_content_process_form_target_creates_popup_and_keeps_opener_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/form", "/result?query=hello"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/form" {
                "<title>HTTP form opener</title><form action='/result' target='_blank'><input name='query' value='hello'><button id='go' type='submit'>Submit</button></form>"
            } else {
                "<title>HTTP form target</title><p>submitted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .await
    .unwrap();
    session
        .action(SemanticAction::Click {
            target: "id=go".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "HTTP form opener"
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let popup = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(popup.title, "HTTP form target");
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_named_form_post_reuses_target_without_get() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_named_form_post_reuses_target_without_get_inner());
    });
}

async fn native_content_process_named_form_post_reuses_target_without_get_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for index in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            if index == 0 {
                assert!(request.starts_with("GET /form HTTP/1.1"));
            } else {
                assert!(request.starts_with("POST /result HTTP/1.1"));
                assert_eq!(request.split("\r\n\r\n").nth(1), Some("query=hello"));
            }
            let body = if index == 0 {
                "<title>Named form opener</title><form action='/result' method='post' target='report'><input name='query' value='hello'><button id='go' type='submit'>Submit</button></form>"
            } else {
                "<title>Named form target</title><p>posted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .await
    .unwrap();
    for _ in 0..2 {
        session
            .action(SemanticAction::Click {
                target: "id=go".into(),
            })
            .await
            .unwrap();
        assert_eq!(session.native_list_targets().await.unwrap().len(), 2);
    }
    let target = session
        .native_list_targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| !target.active)
        .unwrap();
    assert_eq!(target.title, "Named form target");
    session.close().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_data_constructor_collects_text_controls() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/form"));
        let body = "<form id='search'><input name='first' value='one'><textarea name='notes'>hello</textarea><select name='single'><option value='a'>A</option><option value='b' selected>B</option></select><select id='many' name='many' multiple><option value='x' selected>X</option><option selected>Y</option><option value='z'>Z</option></select><input name='unchecked' type='checkbox' value='no'><input name='checked' type='checkbox' value='yes' checked><button type='submit'>Go</button></form><input form='search' name='outside' value='two'>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const many = document.getElementById('many'); many.options[2].selected = true; const data = new FormData(document.getElementById('search')); return { entries: Array.from(data.entries()), first: data.get('first'), checked: data.get('checked'), unchecked: data.has('unchecked'), outside: data.get('outside'), selected: many.selectedOptions.map(option => option.value) }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "entries": [["first", "one"], ["notes", "hello"], ["single", "b"], ["many", "x"], ["many", "Y"], ["many", "z"], ["checked", "yes"], ["outside", "two"]],
            "first": "one",
            "checked": "yes",
            "unchecked": false,
            "outside": "two",
            "selected": ["x", "Y", "z"],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_keyboard_actions_preserve_event_and_modifier_contract() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/keyboard"));
        let body =
            "<input id='name' type='text' value='ab'><input id='next' type='text' value='next'>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/keyboard")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); globalThis.contentKeyEvents = []; field.addEventListener('keydown', event => { globalThis.contentKeyEvents.push(['down', event.key, event.code, event.altKey, event.ctrlKey, event.metaKey, event.shiftKey]); if (event.key === 'x') event.preventDefault(); }); field.addEventListener('input', event => globalThis.contentKeyEvents.push(['input', field.value])); field.addEventListener('keyup', event => globalThis.contentKeyEvents.push(['up', event.key, event.code, event.altKey, event.ctrlKey, event.metaKey, event.shiftKey])); })()",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=name".into(),
        })
        .await
        .unwrap();

    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); field.setSelectionRange(0, 1, 'forward'); return [field.selectionStart, field.selectionEnd, field.selectionDirection]; })()",
        )
        .await
        .unwrap();
    let replaced = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "!".into(),
        })
        .await
        .unwrap();
    assert!(replaced.accepted);

    let before_shortcut = engine.revision();
    let shortcut = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Ctrl+a".into(),
        })
        .await
        .unwrap();
    assert!(shortcut.accepted);
    assert_eq!(shortcut.revision, before_shortcut + 1);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, selectionStart: document.getElementById('name').selectionStart, selectionEnd: document.getElementById('name').selectionEnd, selectionDirection: document.getElementById('name').selectionDirection })",
            )
            .await
            .unwrap(),
        serde_json::json!({"value": "!b", "selectionStart": 0, "selectionEnd": 2, "selectionDirection": "forward"})
    );
    let canceled = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "x".into(),
        })
        .await
        .unwrap();
    assert!(canceled.accepted);
    assert_eq!(canceled.revision, before_shortcut + 2);
    let deleted = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Backspace".into(),
        })
        .await
        .unwrap();
    assert!(deleted.accepted);
    assert_eq!(deleted.revision, before_shortcut + 3);
    let tabbed = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert!(tabbed.accepted);
    assert_eq!(tabbed.revision, before_shortcut + 4);
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("next")
    );
    let reverse_tabbed = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Shift+Tab".into(),
        })
        .await
        .unwrap();
    assert!(reverse_tabbed.accepted);
    assert_eq!(reverse_tabbed.revision, before_shortcut + 5);
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("name")
    );
    let keydown = engine
        .action_async(NativeAction::KeyDown {
            key: "Enter".into(),
        })
        .await
        .unwrap();
    assert!(keydown.accepted);
    let keyup = engine
        .action_async(NativeAction::KeyUp {
            key: "Enter".into(),
        })
        .await
        .unwrap();
    assert!(keyup.accepted);

    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, events: globalThis.contentKeyEvents })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "value": "",
            "events": [
                ["down", "!", "!", false, false, false, false],
                ["input", "!b"],
                ["up", "!", "!", false, false, false, false],
                ["down", "a", "KeyA", false, true, false, false],
                ["up", "a", "KeyA", false, true, false, false],
                ["down", "x", "KeyX", false, false, false, false],
                ["up", "x", "KeyX", false, false, false, false],
                ["down", "Backspace", "Backspace", false, false, false, false],
                ["input", ""],
                ["up", "Backspace", "Backspace", false, false, false, false],
                ["down", "Tab", "Tab", false, false, false, false],
                ["up", "Tab", "Tab", false, false, false, true],
                ["down", "Enter", "Enter", false, false, false, false],
                ["up", "Enter", "Enter", false, false, false, false],
            ],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_external_link_click_honors_content_process_cancellation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/start"));
        let body = "<a id='next' href='/destination'>Next</a><script>document.getElementById('next').addEventListener('click', event => event.preventDefault());</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/start")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let initial_revision = engine.revision();
    engine
        .action_async(NativeAction::Click {
            target: "id=next".into(),
        })
        .await
        .unwrap();
    assert_eq!(engine.revision(), initial_revision + 1);
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/start")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_attribute_associates_external_controls() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/form",
            "/result?before=one&inside=two&after=three&action=submit",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/form" {
                "<input form='search' name='before' value='one'><form id='search' action='/result'><input name='inside' value='two'></form><input form='search' name='after' value='three'><button id='go' form='search' name='action' value='submit' type='submit'>Submit</button>"
            } else {
                "<title>External form result</title><p>Submitted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/result?before=one&inside=two&after=three&action=submit")
    );
    assert_eq!(engine.snapshot().unwrap().title, "External form result");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_submission_supports_multipart_and_text_plain() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/multipart", "/multipart-result", "/plain", "/plain-result"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path.ends_with("-result") {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                if expected_path == "/multipart-result" {
                    let content_type = request.lines().find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-type")
                                .then_some(value.trim())
                        })
                    });
                    let boundary = content_type
                        .and_then(|value| value.strip_prefix("multipart/form-data; boundary="))
                        .expect("multipart boundary");
                    assert!(body.contains(&format!("--{boundary}\r\n")));
                    assert!(body.contains("Content-Disposition: form-data; name=\"alpha\""));
                    assert!(body.contains("\r\n\r\none\r\n"));
                    assert!(body.ends_with(&format!("--{boundary}--\r\n")));
                } else {
                    assert!(
                        request
                            .lines()
                            .any(|line| { line.eq_ignore_ascii_case("content-type: text/plain") })
                    );
                    assert!(body.ends_with("alpha=one\r\n"));
                }
            }
            let body = match expected_path {
                "/multipart" => {
                    "<form id='form' method='post' enctype='multipart/form-data' action='/multipart-result'><input name='alpha' value='one'></form>"
                }
                "/plain" => {
                    "<form id='form' method='post' enctype='text/plain' action='/plain-result'><input name='alpha' value='one'></form>"
                }
                _ => "<title>Form result</title><p>Submitted</p>",
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut multipart = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/multipart")),
    )
    .unwrap();
    multipart.initialize_async().await.unwrap();
    multipart
        .evaluate_async("document.getElementById('form').submit()")
        .await
        .unwrap();
    assert_eq!(
        multipart.snapshot().unwrap().url,
        format!("http://{address}/multipart-result")
    );
    multipart.close_async().await.unwrap();

    let mut text_plain = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/plain")),
    )
    .unwrap();
    text_plain.initialize_async().await.unwrap();
    text_plain
        .evaluate_async("document.getElementById('form').submit()")
        .await
        .unwrap();
    assert_eq!(
        text_plain.snapshot().unwrap().url,
        format!("http://{address}/plain-result")
    );
    text_plain.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_submitter_overrides_form_action_method_and_encoding() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/form", "/result"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/form" {
                assert_eq!(request.split_whitespace().next(), Some("GET"));
            } else {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                assert!(
                    request
                        .lines()
                        .any(|line| { line.eq_ignore_ascii_case("content-type: text/plain") })
                );
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert_eq!(body, "query=hello\r\naction=search\r\n");
            }
            let body = if expected_path == "/form" {
                "<form id='search' action='/wrong' enctype='application/x-www-form-urlencoded'><input name='query' value='hello'><button id='go' type='submit' formmethod='post' formaction='/result' formenctype='text/plain' name='action' value='search'>Go</button></form>"
            } else {
                "<title>Override result</title><p>Submitted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/result")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Override result");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_script_form_submit_navigates_with_get_controls() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/form", "/result?query=go&action=search"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/form" {
                "<form id='search' action='/result'><input name='query' value='hello'><button id='go' name='action' value='search' type='submit'>Go</button></form>"
            } else {
                "<title>Result</title><p>Submitted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async(
            "(() => { const form = document.getElementById('search'); form.addEventListener('submit', event => { document.querySelector('input').value = event.submitter.id; }); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('go').click()")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/result?query=go&action=search")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Result");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_validation_blocks_submit() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<form id='search' action='/result'><input id='query' name='query' required><button id='go' type='submit'>Go</button></form>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let initial_url = format!("http://{address}/form");
    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const form = document.getElementById('search'); const input = document.getElementById('query'); globalThis.invalidEvents = []; globalThis.submitEvents = []; input.addEventListener('invalid', event => invalidEvents.push(event.target.id)); form.addEventListener('submit', event => submitEvents.push(event.submitter && event.submitter.id)); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["query"])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.submitEvents")
            .await
            .unwrap(),
        serde_json::json!([])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_action_blocks_click_and_script_submit() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial_url = format!("http://{address}/form");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/form"));
        let body = "<form id='search' action='/result'><input name='query' value='hello'><button id='go' type='submit'>Go</button></form>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; form-action 'none'\r\nContent-Security-Policy-Report-Only: default-src *; form-action 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_millis(300), listener.accept())
            .await
            .expect_err("form-action blocked submission must not issue a second request");
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.formReports = []; addEventListener('securitypolicyviolation', event => formReports.push([event.effectiveDirective, event.blockedURI, event.disposition]))",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=go".into(),
        })
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    let result_url = format!("http://{address}/result?query=hello");
    assert_eq!(
        engine.evaluate_async("formReports").await.unwrap(),
        serde_json::json!([["form-action", result_url, "report"]])
    );
    engine
        .evaluate_async("document.getElementById('search').submit()")
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    assert_eq!(
        engine.evaluate_async("formReports").await.unwrap(),
        serde_json::json!([
            [
                "form-action",
                format!("http://{address}/result?query=hello"),
                "report"
            ],
            [
                "form-action",
                format!("http://{address}/result?query=hello"),
                "report"
            ]
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_reports_form_action_before_allowed_click_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial_url = format!("http://{address}/form");
    let result_url = format!("http://{address}/result?query=hello");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/form"));
        let body = "<form id='search' action='/result'><input name='query' value='hello'><button id='go' type='submit'>Go</button></form>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src *\r\nContent-Security-Policy-Report-Only: default-src *; form-action 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/result?query=hello")
        );
        let body = "<title>Submitted</title>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url)).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "sessionStorage.setItem('formReports', '[]'); addEventListener('securitypolicyviolation', event => { const reports = JSON.parse(sessionStorage.getItem('formReports') || '[]'); reports.push([event.effectiveDirective, event.blockedURI, event.disposition]); sessionStorage.setItem('formReports', JSON.stringify(reports)); })",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=go".into(),
        })
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, result_url);
    assert_eq!(
        engine
            .evaluate_async("JSON.parse(sessionStorage.getItem('formReports'))")
            .await
            .unwrap(),
        serde_json::json!([["form-action", result_url, "report"]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_validation_covers_common_constraints() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/form", "/result?email=user%40example.com"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/form" {
                "<form id='search' action='/result'><input id='email' name='email' type='email' value='invalid'><button id='go' type='submit'>Go</button></form>"
            } else {
                "<title>Validated result</title><p>Submitted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async(
            "(() => { globalThis.invalidEvents = []; document.getElementById('email').addEventListener('invalid', event => invalidEvents.push(event.target.id)); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/form")
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["email"])
    );
    engine
        .evaluate_async("document.getElementById('email').value = 'user@example.com'")
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/result?email=user%40example.com")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Validated result");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_validation_covers_pattern_constraints() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/form"));
        let body = "<form id='search'><input id='code' name='code' pattern='[A-Z]{3}' value='AB1'><button id='go' type='submit'>Go</button></form>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const code = document.getElementById('code'); return { mismatch: code.validity.patternMismatch, message: code.validationMessage }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "mismatch": true,
            "message": "Please match the requested format.",
        })
    );
    engine
        .evaluate_async("document.getElementById('code').value = 'ABC'")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('code').checkValidity()")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_form_validation_api_exposes_validity_and_custom_errors() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/form"));
        let body = "<form id='search' novalidate><input id='email' name='email' type='email' value='invalid'><input id='custom' name='custom' value='ready'><button id='go' type='submit'>Go</button></form>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { globalThis.invalidEvents = []; ['email', 'custom'].forEach(id => document.getElementById(id).addEventListener('invalid', event => invalidEvents.push(event.target.id))); })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const email = document.getElementById('email'); const form = document.getElementById('search'); return { form: form.checkValidity(), valid: email.validity.valid, mismatch: email.validity.typeMismatch, message: email.validationMessage }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "form": false,
            "valid": false,
            "mismatch": true,
            "message": "Please enter a valid value.",
        })
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["email"])
    );
    engine
        .evaluate_async("document.getElementById('email').value = 'user@example.com'")
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('custom').setCustomValidity('blocked by policy')")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.invalidEvents = []; const custom = document.getElementById('custom'); return { result: custom.reportValidity(), custom: custom.validity.customError, message: custom.validationMessage }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "result": false,
            "custom": true,
            "message": "blocked by policy",
        })
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["custom"])
    );
    engine
        .evaluate_async("document.getElementById('custom').setCustomValidity('')")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.invalidEvents = []; const form = document.getElementById('search'); return { check: form.checkValidity(), report: form.reportValidity(), invalid: invalidEvents }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({ "check": true, "report": true, "invalid": [] })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_script_form_submit_sends_post_controls() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/form", "/result"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/result" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                assert!(request.lines().any(|line| {
                    line.eq_ignore_ascii_case("content-type: application/x-www-form-urlencoded")
                }));
                assert!(request.ends_with("query=hello&action=search"));
            } else {
                assert_eq!(request.split_whitespace().next(), Some("GET"));
            }
            let body = if expected_path == "/form" {
                "<form id='search' method='post' action='/result'><input name='query' value='hello'><button id='go' name='action' value='search' type='submit'>Go</button></form>"
            } else {
                "<title>Post result</title><p>Submitted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/result")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Post result");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_keyboard_button_activation_submits_form() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/form", "/result?query=hello"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/form" {
                "<form action='/result'><input name='query' value='hello'><button id='go' type='submit' style='width:80px;height:20px'>Go</button></form>"
            } else {
                "<title>Result</title><p>Submitted</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    engine
        .evaluate_async("globalThis.__keyboardSubmitTrace = []; const record = value => { globalThis.__keyboardSubmitTrace.push(value); sessionStorage.setItem('native-keyboard-submit-trace', JSON.stringify(globalThis.__keyboardSubmitTrace)); }; const button = document.getElementById('go'); button.focus(); button.addEventListener('keydown', event => record('keydown:' + event.key)); button.addEventListener('click', () => record('click')); button.addEventListener('keyup', event => record('keyup:' + event.key)); document.querySelector('form').addEventListener('submit', () => record('submit')); document.activeElement.id")
        .await
        .map(|active_element| assert_eq!(active_element, serde_json::json!("go")))
        .unwrap();
    let result = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert!(result.accepted);
    assert_eq!(
        engine
            .evaluate_async("sessionStorage.getItem('native-keyboard-submit-trace')")
            .await
            .unwrap(),
        serde_json::json!(r#"["keydown:Enter","click","submit","keyup:Enter"]"#)
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/result?query=hello")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Result");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_commits_recovered_malformed_html() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_commits_recovered_malformed_html_inner());
    });
}

async fn native_content_process_commits_recovered_malformed_html_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request.split_whitespace().nth(1).unwrap_or("/");
            let body = if path.starts_with("/broken") {
                "<p title='unfinished>"
            } else {
                "<title>Stable</title><p>Current document</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let stable_url = format!("http://{address}/stable");
    let broken_url = format!("http://{address}/broken");
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(stable_url),
    )
    .await
    .unwrap();
    let before = session.evidence(EvidenceLevel::Compact).await.unwrap();
    let navigation = session.navigate(broken_url.clone()).await.unwrap();
    assert_eq!(navigation.url, broken_url);
    assert_eq!(navigation.revision, before.revision + 1);
    let after = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(after.url, navigation.url);
    assert!(after.title.is_empty());
    // HTML tokenization discards a start tag left in an unterminated quoted
    // attribute state at EOF, so it contributes no visible text.
    assert!(after.visible_text.is_empty());
    assert!(after.complete);
    session.close().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_owns_external_form_mutations_and_effects() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<input id='toggle' type='checkbox' style='width:80px;height:20px'><input id='name' type='text' style='width:120px;height:20px'>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let initial_revision = engine.revision();

    let clicked = engine
        .action_async(NativeAction::Click {
            target: "id=toggle".into(),
        })
        .await
        .unwrap();
    assert!(clicked.accepted);
    assert_eq!(clicked.revision, initial_revision + 1);
    let toggle = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.input_type.as_deref() == Some("checkbox"))
        .unwrap();
    assert_eq!(toggle.checked, Some(true));

    let typed = engine
        .action_async(NativeAction::Type {
            target: "id=name".into(),
            text: "secret-value".into(),
        })
        .await
        .unwrap();
    assert!(typed.accepted);
    assert_eq!(typed.revision, clicked.revision + 1);
    let input = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.input_type.as_deref() == Some("text"))
        .unwrap();
    assert_eq!(input.empty, Some(false));
    assert!(input.focused);

    let effects = engine.effects_since(initial_revision).unwrap();
    assert_eq!(effects.revision, typed.revision);
    assert!(effects.changed);
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::Click)
    );
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::Input)
    );
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::Change)
    );
    let encoded_effects = format!("{:?}", effects.effects);
    assert!(!encoded_effects.contains("secret-value"));

    let scripted = engine
        .evaluate_async(
            "const toggle = document.getElementById('toggle'); const input = document.getElementById('name'); toggle.click(); input.value = 'child-value'; input.setAttribute('data-source', 'script'); ({ checked: toggle.checked, value: input.value, source: input.getAttribute('data-source') })",
        )
        .await
        .unwrap();
    assert_eq!(
        scripted,
        serde_json::json!({
            "checked": false,
            "value": "child-value",
            "source": "script",
        })
    );
    assert_eq!(engine.revision(), typed.revision + 1);
    assert_eq!(
        engine
            .evaluate_async(
                "({ checked: document.getElementById('toggle').checked, value: document.getElementById('name').value, source: document.getElementById('name').getAttribute('data-source') })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "checked": false,
            "value": "child-value",
            "source": "script",
        })
    );

    let listener_revision = engine.revision();
    let prevented = engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); globalThis.childEvents = []; globalThis.childClick = (event) => { globalThis.childEvents.push(event.type + ':' + event.target.id); document.getElementById('name').value = 'event-child'; event.preventDefault(); }; toggle.addEventListener('click', globalThis.childClick); toggle.click(); return { checked: toggle.checked, value: document.getElementById('name').value, events: globalThis.childEvents }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        prevented,
        serde_json::json!({
            "checked": false,
            "value": "event-child",
            "events": ["click:toggle"],
        })
    );
    assert_eq!(engine.revision(), listener_revision + 1);

    let accepted = engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); toggle.removeEventListener('click', globalThis.childClick); toggle.click(); return toggle.checked; })()",
        )
        .await
        .unwrap();
    assert_eq!(accepted, serde_json::json!(true));
    assert_eq!(engine.revision(), listener_revision + 2);

    let before_action = engine.revision();
    engine
        .evaluate_async(
            "(() => { globalThis.onChildAction = (event) => { document.getElementById('name').value = 'action-child'; }; document.getElementById('toggle').addEventListener('click', globalThis.onChildAction); })()",
        )
        .await
        .unwrap();
    let action = engine
        .action_async(NativeAction::Click {
            target: "id=toggle".into(),
        })
        .await
        .unwrap();
    assert!(action.accepted);
    assert_eq!(action.revision, before_action + 1);
    assert_eq!(engine.revision(), before_action + 1);
    assert_eq!(
        engine
            .evaluate_async(
                "({ checked: document.getElementById('toggle').checked, value: document.getElementById('name').value })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "checked": false,
            "value": "action-child",
        })
    );
    let before_cancel = engine.revision();
    engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); toggle.removeEventListener('click', globalThis.onChildAction); globalThis.onChildCancel = (event) => event.preventDefault(); toggle.addEventListener('click', globalThis.onChildCancel); })()",
        )
        .await
        .unwrap();
    let canceled = engine
        .action_async(NativeAction::Click {
            target: "id=toggle".into(),
        })
        .await
        .unwrap();
    assert!(canceled.accepted);
    assert_eq!(canceled.revision, before_cancel + 1);
    assert_eq!(engine.revision(), before_cancel + 1);
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('toggle').checked")
            .await
            .unwrap(),
        serde_json::json!(false)
    );
    let before_type = engine.revision();
    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); globalThis.childTypeEvents = []; field.addEventListener('focus', (event) => globalThis.childTypeEvents.push(event.type)); field.addEventListener('input', (event) => { globalThis.childTypeEvents.push(event.type); field.setAttribute('data-input', 'seen'); }); field.addEventListener('change', (event) => globalThis.childTypeEvents.push(event.type)); })()",
        )
        .await
        .unwrap();
    let typed = engine
        .action_async(NativeAction::Type {
            target: "id=name".into(),
            text: "typed-by-child-action".into(),
        })
        .await
        .unwrap();
    assert!(typed.accepted);
    assert_eq!(typed.revision, before_type + 1);
    assert_eq!(engine.revision(), before_type + 1);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, marker: document.getElementById('name').getAttribute('data-input'), events: globalThis.childTypeEvents })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "value": "typed-by-child-action",
            "marker": "seen",
            "events": ["focus", "input", "change"],
        })
    );

    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); globalThis.childKeyEvents = []; field.addEventListener('keydown', event => globalThis.childKeyEvents.push(event.type + ':' + event.key)); field.addEventListener('input', event => globalThis.childKeyEvents.push(event.type + ':' + field.value)); field.addEventListener('keyup', event => globalThis.childKeyEvents.push(event.type + ':' + event.key)); })()",
        )
        .await
        .unwrap();
    let keyed = engine
        .action_async(NativeAction::KeyPress { key: "!".into() })
        .await
        .unwrap();
    assert!(keyed.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, events: globalThis.childKeyEvents })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "value": "typed-by-child-action!",
            "events": ["keydown:!", "input:typed-by-child-action!", "keyup:!"],
        })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_owns_clear_and_select_form_actions() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_owns_clear_and_select_form_actions_inner());
    });
}

async fn native_content_process_owns_clear_and_select_form_actions_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<input id='name' type='text' value='loaded' style='width:120px;height:20px'><input id='remember' type='checkbox' style='width:20px;height:20px'><select id='country' style='width:120px;height:20px'><option value='one'>One</option><option value='two'>Two</option></select>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form-actions")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let cleared = engine
        .action_async(NativeAction::Clear {
            target: "id=name".into(),
        })
        .await
        .unwrap();
    assert!(cleared.accepted);
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('name').value")
            .await
            .unwrap(),
        serde_json::json!("")
    );

    let checked = engine
        .action_async(NativeAction::Check {
            target: "id=remember".into(),
        })
        .await
        .unwrap();
    assert!(checked.accepted);
    let unchecked = engine
        .action_async(NativeAction::Uncheck {
            target: "id=remember".into(),
        })
        .await
        .unwrap();
    assert!(unchecked.accepted);

    let selected = engine
        .action_async(NativeAction::Select {
            target: "id=country".into(),
            value: "two".into(),
        })
        .await
        .unwrap();
    assert!(selected.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "({name: document.getElementById('name').value, remember: document.getElementById('remember').checked, country: document.getElementById('country').value})",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "name": "",
            "remember": false,
            "country": "two",
        })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_pointer_actions_bridge_dom_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let body = "<button id='button' style='width:100px;height:24px'>Button</button><div id='source' draggable='true' style='width:100px;height:24px'>Source</div><div id='destination' style='width:100px;height:24px'>Destination</div><script>globalThis.pointerEvents = []; globalThis.doubleClicks = 0; const button = document.getElementById('button'); const source = document.getElementById('source'); const destination = document.getElementById('destination'); button.addEventListener('click', () => globalThis.doubleClicks += 1); button.addEventListener('mouseover', event => pointerEvents.push(event.type + ':' + event.target.id)); button.addEventListener('mouseenter', event => pointerEvents.push(event.type + ':' + event.target.id)); source.addEventListener('dragstart', event => pointerEvents.push(event.type + ':' + event.target.id)); destination.addEventListener('dragenter', event => pointerEvents.push(event.type + ':' + event.target.id)); destination.addEventListener('dragover', event => pointerEvents.push(event.type + ':' + event.target.id)); destination.addEventListener('drop', event => pointerEvents.push(event.type + ':' + event.target.id)); source.addEventListener('dragend', event => pointerEvents.push(event.type + ':' + event.target.id));</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/pointer-actions")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let double_clicked = engine
        .action_async(NativeAction::DoubleClick {
            target: "id=button".into(),
        })
        .await
        .unwrap();
    assert!(double_clicked.accepted);
    let hovered = engine
        .action_async(NativeAction::Hover {
            target: "id=button".into(),
        })
        .await
        .unwrap();
    assert!(hovered.accepted);
    let dragged = engine
        .action_async(NativeAction::Drag {
            source: "id=source".into(),
            destination: "id=destination".into(),
        })
        .await
        .unwrap();
    assert!(dragged.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "({ clicks: globalThis.doubleClicks, events: globalThis.pointerEvents })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "clicks": 2,
            "events": [
                "mouseover:button",
                "mouseenter:button",
                "dragstart:source",
                "dragenter:destination",
                "dragover:destination",
                "drop:destination",
                "dragend:source",
            ],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_selects_picture_source_and_reloads_img_fallback() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/wide.png", "/fallback-set.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path != "/page" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&png).await.unwrap();
            } else {
                let body = "<picture><source id='avif' media='(min-width: 500px)' type='image/avif' srcset='/unsupported.avif 1x'><source id='wide' media='(min-width: 500px)' type='image/png' srcset='/wide.png 1x'><source id='narrow' media='(max-width: 499px)' type='image/png' srcset='/narrow.png 1x'><img id='image' src='/fallback.png' srcset='/fallback-set.png 1x' alt='responsive image'></picture>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 600,
                height: 400,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, format!("http://{address}/wide.png")])
    );

    engine
        .evaluate_async(
            "(() => { document.getElementById('wide').media = '(max-width: 500px)'; return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, format!("http://{address}/fallback-set.png")])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_reloads_the_selected_srcset_candidate_after_sizes_mutation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/large.png", "/small.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path != "/page" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&png).await.unwrap();
            } else {
                let body = "<img id='image' src='/fallback.png' srcset='/small.png 400w, /large.png 800w' sizes='600px'>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 600,
                height: 400,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, format!("http://{address}/large.png")])
    );

    engine
        .evaluate_async(
            "(() => { const image = document.getElementById('image'); image.sizes = '300px'; return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, format!("http://{address}/small.png")])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_uploads_bounded_file_blob_form_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/upload", "/blob"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/upload" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                let content_type = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            .then_some(value.trim())
                    })
                });
                let boundary = content_type
                    .and_then(|value| value.strip_prefix("multipart/form-data; boundary="))
                    .expect("multipart boundary");
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert!(body.contains(&format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"blob\"; filename=\"blob\"\r\nContent-Type: text/plain\r\n\r\nhello!\r\n"
                )));
                assert!(body.contains(&format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"note.txt\"\r\nContent-Type: text/plain\r\n\r\nworld\r\n"
                )));
                assert!(body.ends_with(&format!("--{boundary}--\r\n")));
            } else if expected_path == "/blob" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                let content_type = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            .then_some(value.trim())
                    })
                });
                assert_eq!(content_type, Some("text/plain"));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert_eq!(body, "hello!");
            }
            let body = if expected_path == "/page" {
                "<p>Upload owner</p>"
            } else if expected_path == "/blob" {
                "blob-uploaded"
            } else {
                "uploaded"
            };
            let content_type = if expected_path == "/page" {
                "text/html"
            } else {
                "text/plain"
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
            "fetch('/upload', { method: 'POST', body: (() => { const data = new FormData(); data.append('blob', new Blob(['hello!'], { type: 'text/plain' })); data.append('file', new File(['world'], 'note.txt', { type: 'text/plain' })); return data; })() }).then(response => response.text()).then(value => fetch('/blob', { method: 'POST', body: new Blob(['hello!'], { type: 'text/plain' }) }).then(response => response.text()).then(blobValue => { globalThis.uploadStatus = value + ':' + blobValue; }));",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.uploadStatus")
            .await
            .unwrap(),
        serde_json::json!("uploaded:blob-uploaded")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_parses_request_form_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Request form data</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                "await (async () => { const encoded = new Request('/encoded', { method: 'POST', body: 'name=glass&tag=one&tag=two', headers: { 'Content-Type': 'application/x-www-form-urlencoded' } }); const encodedForm = await encoded.formData(); const encodedView = [encodedForm instanceof FormData, Array.from(encodedForm.entries()), encoded.bodyUsed]; const source = new FormData(); source.append('name', 'glass'); source.append('file', new File([new Uint8Array([0, 255, 128, 70])], 'data.bin', { type: 'application/octet-stream' })); const multipart = new Request('/multipart', { method: 'POST', body: source }); const multipartForm = await multipart.formData(); const file = multipartForm.get('file'); const multipartView = [Array.from(multipartForm.entries()).map(entry => [entry[0], entry[1] instanceof File ? 'file' : entry[1]]), file instanceof File, file.name, file.type, file.size, Array.from(await file.bytes()), multipart.bodyUsed]; const unsupported = new Request('/unsupported', { method: 'POST', body: 'raw' }); const unsupportedResult = await unsupported.formData().then(() => 'readable', error => error.name); return { encodedView, multipartView, unsupported: [unsupportedResult, unsupported.bodyUsed] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "encodedView": [
                true,
                [["name", "glass"], ["tag", "one"], ["tag", "two"]],
                true
            ],
            "multipartView": [
                [["name", "glass"], ["file", "file"]],
                true,
                "data.bin",
                "application/octet-stream",
                4,
                [0, 255, 128, 70],
                true
            ],
            "unsupported": ["TypeError", true]
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_collects_form_associated_custom_element_values() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_collects_form_associated_custom_element_values_inner(),
        );
    });
}

async fn native_content_process_collects_form_associated_custom_element_values_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native form-associated custom-element request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<!doctype html><html><body><form id='main-form'><x-face-lifecycle id='face-owner'></x-face-lifecycle><x-scalar-field id='scalar' name='scalar'></x-scalar-field><input name='builtin' value='native'><x-pair-field name='ignored'></x-pair-field><x-empty-field name='empty'></x-empty-field><x-disabled-field name='disabled' disabled></x-disabled-field><x-unnamed-field></x-unnamed-field><fieldset id='face-fieldset' disabled><legend><x-legend-field name='legend'></x-legend-field><x-face-lifecycle id='face-legend'></x-face-lifecycle></legend><x-face-lifecycle id='face-disabled'></x-face-lifecycle><x-fieldset-disabled name='fieldset-disabled'></x-fieldset-disabled></fieldset></form><form id='external-form'></form><x-external-field id='external-field' form='external-form' name='outside'></x-external-field><x-face-lifecycle id='face-explicit' form='external-form'></x-face-lifecycle></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/face")),
    )
    .await
    .unwrap();
    let result = session
        .script(native_form_associated_elements_script())
        .await
        .unwrap();
    assert_native_form_associated_elements_result(&result.value);

    let persisted = session
        .script(native_form_associated_entries_after_script_refresh())
        .await
        .unwrap();
    assert_eq!(
        persisted.value,
        serde_json::json!({
            "entries": [
                ["scalar", "one"],
                ["builtin", "native"],
                ["first", "alpha"],
                ["second", "beta"],
                ["proof", ["file", "proof.txt", "text/plain"]],
                ["legend", "legend"],
            ],
            "internalsBrand": true,
            "formOwner": true,
            "lifecycleEventCount": 19,
            "explicitLifecycleOwner": "main-form",
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_resets_forms_and_runs_face_reset_reactions() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_resets_forms_and_runs_face_reset_reactions_inner());
    });
}

async fn native_content_process_resets_forms_and_runs_face_reset_reactions_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = native_form_reset_fixture();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native form-reset request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/reset")),
    )
    .await
    .unwrap();
    session
        .native_upload_files(
            "id=reset-file",
            vec![NativeFile {
                name: "reset.bin".into(),
                media_type: "application/octet-stream".into(),
                last_modified: 17,
                bytes: vec![0, 127, 255],
            }],
            None,
        )
        .await
        .unwrap();
    let result = session.script(native_form_reset_script()).await.unwrap();
    assert_native_form_reset_result(&result.value);
    assert_eq!(
        session
            .script(native_form_reset_after_script_refresh())
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "text": "cancelled-value",
            "checked": true,
            "textarea": "default-notes",
            "select": "default-choice",
            "options": [false, true],
            "fallbackSelect": "fallback-enabled",
            "fallbackOptions": [false, true],
            "fileCount": 0,
            "fileValue": "",
            "customError": true,
            "unnamedRadios": [true, true],
            "namedRadios": [false, true],
            "external": "external-default",
            "faceValue": "after",
            "callbackCount": 6,
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_resets_same_origin_frame_forms() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_resets_same_origin_frame_forms_inner());
    });
}

async fn native_content_process_resets_same_origin_frame_forms_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("same-origin frame reset request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<!doctype html><body><iframe id='child' src='/child'></iframe></body>"
                }
                Some("/child") => {
                    "<!doctype html><body><form id='frame-form'><input id='frame-input' value='frame-default'></form></body>"
                }
                other => panic!("unexpected same-origin frame reset path: {other:?}"),
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
    let result = session
        .script(
            r##"(() => {
              const frame = document.getElementById('child');
              const child = frame.contentDocument;
              const form = child.getElementById('frame-form');
              const input = child.getElementById('frame-input');
              let events = 0;
              form.addEventListener('reset', () => { events += 1; input.value = 'frame-listener'; });
              input.value = 'frame-dirty';
              form.reset();
              return { sameOrigin: child !== null, value: input.value, events };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!({"sameOrigin": true, "value": "frame-default", "events": 1})
    );

    let child_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin child frame")
        .id;
    session.select_frame(&child_id).await.unwrap();
    assert_eq!(
        session
            .script("document.getElementById('frame-input').value")
            .await
            .unwrap()
            .value,
        serde_json::json!("frame-default")
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_keyboard_checkable_activation_and_frame_routing() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_keyboard_checkable_activation_and_frame_routing_inner(),
        );
    });
}

async fn native_content_process_keyboard_checkable_activation_and_frame_routing_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let controls = "<form id='form'><input id='checkbox' type='checkbox' checked><input id='radio-a' type='radio' name='group' checked><input id='radio-b' type='radio' name='group'></form><form id='other-form'></form><input id='other' type='checkbox'>";
    let parent_body =
        format!("<!doctype html><body>{controls}<iframe id='child' src='/child'></iframe></body>");
    let child_body = format!("<!doctype html><body>{controls}</body>");
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native checkable-control request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/page") => &parent_body,
                Some("/child") => &child_body,
                other => panic!("unexpected native checkable-control path: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .await
    .unwrap();
    let setup_script = "globalThis.events = []; globalThis.cancelClickId = ''; globalThis.changeRadioFormOnClick = false; document.addEventListener('keydown', event => events.push('keydown:' + event.target.id + ':' + (event.key === ' ' ? 'Space' : event.key))); document.addEventListener('keyup', event => events.push('keyup:' + event.target.id + ':' + (event.key === ' ' ? 'Space' : event.key))); document.addEventListener('click', event => { const target = event.target; const state = target.id === 'checkbox' ? target.checked + ':' + target.indeterminate : document.getElementById('radio-a').checked + ':' + document.getElementById('radio-b').checked; events.push('click:' + target.id + ':' + state + ':' + event.cancelable); if (changeRadioFormOnClick && target.id === 'radio-b') { target.setAttribute('form', 'other-form'); event.preventDefault(); } else if (cancelClickId === target.id) event.preventDefault(); }); document.addEventListener('input', event => events.push('input:' + event.target.id)); document.addEventListener('change', event => events.push('change:' + event.target.id)); true";
    session.script(setup_script).await.unwrap();

    session
        .script("document.getElementById('checkbox').indeterminate = true; document.getElementById('checkbox').focus(); globalThis.cancelClickId = 'checkbox'; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({checked: document.getElementById('checkbox').checked, indeterminate: document.getElementById('checkbox').indeterminate, events: globalThis.events})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "checked": true,
            "indeterminate": true,
            "events": [
                "keydown:checkbox:Space",
                "keyup:checkbox:Space",
                "click:checkbox:false:false:true",
            ],
        })
    );

    session
        .script("globalThis.events = []; globalThis.cancelClickId = ''; document.getElementById('checkbox').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({checked: document.getElementById('checkbox').checked, indeterminate: document.getElementById('checkbox').indeterminate, events: globalThis.events})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "checked": false,
            "indeterminate": false,
            "events": [
                "keydown:checkbox:Space",
                "keyup:checkbox:Space",
                "click:checkbox:false:false:true",
                "input:checkbox",
                "change:checkbox",
            ],
        })
    );

    session
        .script("globalThis.events = []; globalThis.cancelClickId = 'radio-b'; document.getElementById('radio-b').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({a: document.getElementById('radio-a').checked, b: document.getElementById('radio-b').checked, events: globalThis.events})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "a": true,
            "b": false,
            "events": [
                "keydown:radio-b:Space",
                "keyup:radio-b:Space",
                "click:radio-b:false:true:true",
            ],
        })
    );

    session
        .script("globalThis.events = []; globalThis.cancelClickId = ''; globalThis.changeRadioFormOnClick = true; const radioA = document.getElementById('radio-a'); const radioB = document.getElementById('radio-b'); radioA.checked = true; radioB.checked = false; radioB.focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({a: document.getElementById('radio-a').checked, b: document.getElementById('radio-b').checked, events: globalThis.events})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "a": false,
            "b": false,
            "events": [
                "keydown:radio-b:Space",
                "keyup:radio-b:Space",
                "click:radio-b:false:true:true",
            ],
        })
    );

    session
        .script("globalThis.events = []; globalThis.cancelClickId = ''; globalThis.changeRadioFormOnClick = false; document.getElementById('radio-b').removeAttribute('form'); document.getElementById('radio-a').checked = true; document.getElementById('radio-b').checked = false; document.getElementById('radio-b').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    let accepted_radio = serde_json::json!({
        "a": false,
        "b": true,
        "events": [
            "keydown:radio-b:Space",
            "keyup:radio-b:Space",
            "click:radio-b:false:true:true",
            "input:radio-b",
            "change:radio-b",
        ],
    });
    assert_eq!(
        session
            .script("({a: document.getElementById('radio-a').checked, b: document.getElementById('radio-b').checked, events: globalThis.events})")
            .await
            .unwrap()
            .value,
        accepted_radio
    );

    session
        .script("globalThis.events = []; document.getElementById('radio-b').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({a: document.getElementById('radio-a').checked, b: document.getElementById('radio-b').checked, events: globalThis.events})")
            .await
            .unwrap()
            .value,
        accepted_radio
    );

    session
        .script("globalThis.events = []; document.getElementById('checkbox').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::KeyDown { key: " ".into() })
        .await
        .unwrap();
    session
        .script("document.getElementById('other').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::KeyUp { key: " ".into() })
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.events").await.unwrap().value,
        serde_json::json!(["keydown:checkbox:Space", "keyup:other:Space"])
    );

    session
        .script("const checkboxForInvalidation = document.getElementById('checkbox'); checkboxForInvalidation.type = 'checkbox'; checkboxForInvalidation.disabled = false; checkboxForInvalidation.focus(); globalThis.events = []; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::KeyDown { key: " ".into() })
        .await
        .unwrap();
    session
        .script("document.getElementById('checkbox').type = 'radio'; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::KeyUp { key: " ".into() })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({checked: document.getElementById('checkbox').checked, activated: globalThis.events.some(event => event.startsWith('click:checkbox:'))})")
            .await
            .unwrap()
            .value,
        serde_json::json!({"checked": false, "activated": false})
    );

    session
        .script("document.getElementById('checkbox').type = 'checkbox'; document.getElementById('checkbox').focus(); globalThis.events = []; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::KeyDown { key: " ".into() })
        .await
        .unwrap();
    session
        .script("document.getElementById('checkbox').disabled = true; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::KeyUp { key: " ".into() })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({checked: document.getElementById('checkbox').checked, activated: globalThis.events.some(event => event.startsWith('click:'))})")
            .await
            .unwrap()
            .value,
        serde_json::json!({"checked": false, "activated": false})
    );

    let child_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin checkable-control child frame")
        .id;
    session.select_frame(&child_id).await.unwrap();
    session.script(setup_script).await.unwrap();
    session
        .script("document.getElementById('checkbox').checked = false; document.getElementById('checkbox').indeterminate = false; document.getElementById('checkbox').focus(); globalThis.events = []; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({checked: document.getElementById('checkbox').checked, events: globalThis.events})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "checked": true,
            "events": [
                "keydown:checkbox:Space",
                "keyup:checkbox:Space",
                "click:checkbox:true:false:true",
                "input:checkbox",
                "change:checkbox",
            ],
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_keyboard_button_activation_resets_forms_and_honors_cancellation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_keyboard_button_activation_resets_forms_and_honors_cancellation_inner(),
        );
    });
}

async fn native_content_process_keyboard_button_activation_resets_forms_and_honors_cancellation_inner()
 {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = native_reset_button_fixture();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native reset-button request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/form")),
    )
    .await
    .unwrap();
    session
        .script(native_reset_button_setup_script())
        .await
        .unwrap();
    assert_eq!(
        session
            .script(native_reset_button_state_script())
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "mainValue": "main-default",
            "otherValue": "other-default",
            "mainReset": 1,
            "otherReset": 0,
            "submit": 0,
            "invalid": 0,
            "sameHref": true,
        })
    );

    session
        .script("document.getElementById('main-value').value = 'dirty-input'; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Click {
            target: "id=input-reset".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script(native_reset_button_state_script())
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "mainValue": "main-default",
            "otherValue": "other-default",
            "mainReset": 2,
            "otherReset": 0,
            "submit": 0,
            "invalid": 0,
            "sameHref": true,
        })
    );

    session
        .script("document.getElementById('main-value').value = 'dirty-click-cancel'; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Click {
            target: "id=click-cancel".into(),
        })
        .await
        .unwrap();
    session
        .script("document.getElementById('other-value').value = 'dirty-other'; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Click {
            target: "id=promote-reset".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script(native_reset_button_state_script())
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "mainValue": "dirty-click-cancel",
            "otherValue": "dirty-other",
            "mainReset": 2,
            "otherReset": 1,
            "submit": 0,
            "invalid": 0,
            "sameHref": true,
        })
    );

    session
        .script("document.getElementById('main-value').value = 'dirty-enter'; document.getElementById('input-reset').focus(); globalThis.__glassResetButtonEvents.length = 0; true")
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
            .script("({value: document.getElementById('main-value').value, resets: globalThis.__glassResetButtonTrace.mainReset, events: globalThis.__glassResetButtonEvents})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "value": "main-default",
            "resets": 3,
            "events": [
                "keydown:input-reset:Enter",
                "click:input-reset",
                "keyup:input-reset:Enter",
            ],
        })
    );

    session
        .script("document.getElementById('main-value').value = 'dirty-space'; document.getElementById('button-reset').focus(); globalThis.__glassResetButtonEvents.length = 0; true")
        .await
        .unwrap();
    session
        .action(SemanticAction::KeyDown { key: " ".into() })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({value: document.getElementById('main-value').value, resets: globalThis.__glassResetButtonTrace.mainReset, events: globalThis.__glassResetButtonEvents})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "value": "dirty-space",
            "resets": 3,
            "events": ["keydown:button-reset: "],
        })
    );
    session
        .action(SemanticAction::KeyUp { key: " ".into() })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({value: document.getElementById('main-value').value, resets: globalThis.__glassResetButtonTrace.mainReset, events: globalThis.__glassResetButtonEvents})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "value": "main-default",
            "resets": 4,
            "events": [
                "keydown:button-reset: ",
                "keyup:button-reset: ",
                "click:button-reset",
            ],
        })
    );

    session
        .script("document.getElementById('main-value').value = 'dirty-canceled-key'; document.getElementById('input-reset').focus(); globalThis.__glassCancelKeyboardEnter = true; globalThis.__glassResetButtonEvents.length = 0; true")
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
            .script("({value: document.getElementById('main-value').value, resets: globalThis.__glassResetButtonTrace.mainReset, events: globalThis.__glassResetButtonEvents})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "value": "dirty-canceled-key",
            "resets": 4,
            "events": ["keydown:input-reset:Enter", "keyup:input-reset:Enter"],
        })
    );

    session
        .script("globalThis.__glassCancelKeyboardEnter = false; document.getElementById('click-cancel').focus(); globalThis.__glassResetButtonEvents.length = 0; true")
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
            .script("({value: document.getElementById('main-value').value, resets: globalThis.__glassResetButtonTrace.mainReset, events: globalThis.__glassResetButtonEvents})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "value": "dirty-canceled-key",
            "resets": 4,
            "events": [
                "keydown:click-cancel:Enter",
                "click:click-cancel",
                "keyup:click-cancel:Enter",
            ],
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_keyboard_link_activation_uses_live_href_and_event_order() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_keyboard_link_activation_uses_live_href_and_event_order_inner(),
        );
    });
}

async fn native_content_process_keyboard_link_activation_uses_live_href_and_event_order_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("keyboard-link navigation request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/page") => {
                    "<!doctype html><body><a id='link' href='/wrong-destination'>Open</a><script>window.name = ''; globalThis.events = []; globalThis.cancelKeydown = false; globalThis.cancelClick = false; globalThis.mutateHref = false; const link = document.getElementById('link'); link.addEventListener('keydown', event => { events.push('keydown:' + event.key); if (globalThis.cancelKeydown && event.key === 'Enter') event.preventDefault(); window.name = JSON.stringify(events); }); link.addEventListener('click', event => { events.push('click'); if (globalThis.cancelClick) event.preventDefault(); events.push('prevented:' + event.defaultPrevented + ':' + globalThis.cancelClick); if (globalThis.mutateHref) link.setAttribute('href', '/keyboard-link-destination'); window.name = JSON.stringify(events); }); link.addEventListener('keyup', event => { events.push('keyup:' + event.key); window.name = JSON.stringify(events); });</script>"
                }
                Some("/keyboard-link-destination") => {
                    "<!doctype html><body><p id='destination'>Keyboard destination</p></body>"
                }
                Some("/wrong-destination") => {
                    "<!doctype html><body><p id='wrong'>Wrong destination</p></body>"
                }
                other => panic!("unexpected keyboard-link request: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .await
    .unwrap();
    session
        .script("document.getElementById('link').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("JSON.parse(window.name)")
            .await
            .unwrap()
            .value,
        serde_json::json!(["keydown: ", "keyup: "])
    );

    session
        .script("globalThis.events = []; window.name = ''; globalThis.cancelKeydown = true; true")
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
        serde_json::json!("/page")
    );
    assert_eq!(
        session
            .script("JSON.parse(window.name)")
            .await
            .unwrap()
            .value,
        serde_json::json!(["keydown:Enter", "keyup:Enter"])
    );

    session
        .script("globalThis.events = []; window.name = ''; globalThis.cancelKeydown = false; globalThis.cancelClick = true; true")
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
            .script("JSON.parse(window.name)")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            "keydown:Enter",
            "click",
            "prevented:true:true",
            "keyup:Enter",
        ])
    );
    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/page")
    );
    session
        .script("globalThis.events = []; window.name = ''; globalThis.cancelClick = false; globalThis.mutateHref = true; document.getElementById('link').focus(); true")
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
            .script("({url: location.href, events: JSON.parse(window.name), text: document.body.textContent.trim()})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "url": format!("http://{address}/keyboard-link-destination"),
            "events": [
                "keydown:Enter",
                "click",
                "prevented:false:false",
                "keyup:Enter",
            ],
            "text": "Keyboard destination",
        })
    );

    session.close().await.unwrap();
    server.abort();
    let _ = server.await;
}
#[test]
fn native_content_process_keyboard_link_activation_in_same_origin_frames() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_keyboard_link_activation_in_same_origin_frames_inner(),
        );
    });
}

async fn native_content_process_keyboard_link_activation_in_same_origin_frames_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("same-origin reset-button request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<!doctype html><body><iframe id='child' src='/child'></iframe></body>"
                }
                Some("/child") => {
                    "<!doctype html><body><form id='frame-form'><input id='frame-value' value='frame-default'><button id='frame-reset' type='reset'>Reset</button></form><a id='frame-link' href='/frame-destination'>Open child destination</a></body>"
                }
                Some("/frame-destination") => {
                    "<!doctype html><body><p id='frame-destination'>Child destination</p></body>"
                }
                other => panic!("unexpected same-origin reset-button path: {other:?}"),
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
    session
        .script(
            "(() => { const child = document.getElementById('child').contentDocument; child.getElementById('frame-value').value = 'frame-dirty'; child.getElementById('frame-reset').click(); return child !== null; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.getElementById('child').contentDocument.getElementById('frame-value').value")
            .await
            .unwrap()
            .value,
        serde_json::json!("frame-default")
    );

    let child_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin reset-button child frame")
        .id;
    session.select_frame(&child_id).await.unwrap();
    assert_eq!(
        session
            .script("document.getElementById('frame-value').value")
            .await
            .unwrap()
            .value,
        serde_json::json!("frame-default")
    );

    session
        .script("globalThis.__frameKeyboardTrace = []; const input = document.getElementById('frame-value'); const button = document.getElementById('frame-reset'); button.addEventListener('keydown', event => __frameKeyboardTrace.push('keydown:' + event.key)); button.addEventListener('keyup', event => __frameKeyboardTrace.push('keyup:' + event.key)); button.addEventListener('click', () => __frameKeyboardTrace.push('click')); input.value = 'frame-space-dirty'; button.focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({value: document.getElementById('frame-value').value, trace: globalThis.__frameKeyboardTrace})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "value": "frame-default",
            "trace": ["keydown: ", "keyup: ", "click"],
        })
    );

    session
        .script("document.getElementById('frame-link').focus(); true")
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
            .script("({url: location.href, text: document.body.textContent.trim()})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "url": format!("http://{address}/frame-destination"),
            "text": "Child destination",
        })
    );
    session.select_frame("native-context:main").await.unwrap();
    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/parent")
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_modifier_link_click_routes_through_same_origin_frame() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_modifier_link_click_routes_through_same_origin_frame_inner(),
        );
    });
}

async fn native_content_process_modifier_link_click_routes_through_same_origin_frame_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("same-origin modifier-click request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<title>Frame modifier opener</title><iframe id='child' src='/child'></iframe>"
                }
                Some("/child") => {
                    "<style>body{margin:0px}</style><title>Frame source</title><a id='open' href='/popup'>Open popup</a><script>globalThis.__modifierFlags = null; document.getElementById('open').addEventListener('click', event => { globalThis.__modifierFlags = [event.altKey, event.ctrlKey, event.metaKey, event.shiftKey]; });</script>"
                }
                Some("/popup") => {
                    "<title>Frame modifier popup</title><p>popup opened from frame</p>"
                }
                other => panic!("unexpected same-origin modifier-click path: {other:?}"),
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
    let child_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin modifier-click frame")
        .id;
    session.select_frame(&child_id).await.unwrap();
    session
        .action(SemanticAction::ClickWithModifiers {
            target: "id=open".into(),
            modifiers: ClickModifiers {
                meta: true,
                ..ClickModifiers::default()
            },
        })
        .await
        .unwrap();

    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/child")
    );
    assert_eq!(
        session
            .script("globalThis.__modifierFlags")
            .await
            .unwrap()
            .value,
        serde_json::json!([false, false, true, false])
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let popup = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(popup.title, "Frame modifier popup");
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    session.select_frame("native-context:main").await.unwrap();
    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/parent")
    );
    session.close().await.unwrap();
    server.await.unwrap();
}

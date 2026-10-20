use super::*;

pub(super) async fn native_cross_origin_parent_security_and_cookie_authority_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let child_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let child_address = child_listener.local_addr().unwrap();
    let child_server = tokio::spawn(async move {
        let (mut stream, _) = child_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/child"));
        let cookie = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
            .and_then(|line| line.split_once(':'))
            .map(|(_, value)| value.trim().to_owned());
        let body = "<html><head><title>Frame loaded</title></head><body><p>cross-origin child</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nSet-Cookie: child_frame_secret=accepted; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        cookie
    });

    let parent_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let parent_address = parent_listener.local_addr().unwrap();
    let parent_server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for expected_path in ["/parent", "/after"] {
            let (mut stream, _) = parent_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (headers, content_type, body) = match expected_path {
                "/parent" => (
                    "Set-Cookie: parent_frame_secret=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    format!(
                        "<html><body><iframe id='child' src='http://{child_address}/child'></iframe><p>parent</p></body></html>"
                    ),
                ),
                "/after" => ("", "text/plain", "parent observed frame cookie".to_owned()),
                other => panic!("unexpected parent-frame request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((expected_path.to_owned(), cookie));
        }
        requests
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{parent_address}/parent")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("the cross-origin child frame is present")
        .id
        .clone();

    let parent_cookie_observation = session
        .script(
            "await fetch('/after').then(async response => ({ body: await response.text(), cookie: document.cookie }))",
        )
        .await
        .unwrap()
        .value;
    assert_eq!(
        parent_cookie_observation,
        serde_json::json!({
            "body": "parent observed frame cookie",
            "cookie": "",
        })
    );

    let parent_view = session
        .script(
            "(() => { const childWindow = document.getElementById('child').contentWindow; let documentError; let historyError; try { childWindow.document; } catch (error) { documentError = [error instanceof DOMException, error.name, error.code]; } try { childWindow.history; } catch (error) { historyError = [error instanceof DOMException, error.name, error.code]; } return { contentDocument: document.getElementById('child').contentDocument, frameElement: childWindow.frameElement, documentError, historyError, length: childWindow.length, parent: childWindow.parent === window, location: childWindow.location.href }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        parent_view.value,
        serde_json::json!({
            "contentDocument": null,
            "frameElement": null,
            "documentError": [true, "SecurityError", 18],
            "historyError": [true, "SecurityError", 18],
            "length": 0,
            "parent": true,
            "location": format!("http://{child_address}/child"),
        })
    );

    session.native_select_frame(&child_id).await.unwrap();
    let child_view = session
        .script(
            "(() => { let parentDocumentError; let parentHistoryError; try { window.parent.document; } catch (error) { parentDocumentError = [error instanceof DOMException, error.name, error.code]; } try { window.parent.history; } catch (error) { parentHistoryError = [error instanceof DOMException, error.name, error.code]; } return { frameElement: window.frameElement, parentDocumentError, parentHistoryError, parentLocation: window.parent.location.href, top: window.top === window.parent }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        child_view.value,
        serde_json::json!({
            "frameElement": null,
            "parentDocumentError": [true, "SecurityError", 18],
            "parentHistoryError": [true, "SecurityError", 18],
            "parentLocation": format!("http://{parent_address}/parent"),
            "top": true,
        })
    );

    session.close().await.unwrap();
    let child_request_cookie = child_server.await.unwrap();
    assert!(
        child_request_cookie
            .as_deref()
            .is_some_and(|value| value.contains("parent_frame_secret=seed")),
        "parent did not select its HttpOnly cookie for the cross-origin child frame: {child_request_cookie:?}"
    );
    let parent_requests = parent_server.await.unwrap();
    assert_eq!(
        parent_requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/parent", "/after"]
    );
    let after_request_cookie = parent_requests
        .iter()
        .find(|(path, _)| path == "/after")
        .and_then(|(_, cookie)| cookie.as_deref());
    for name in ["parent_frame_secret=seed", "child_frame_secret=accepted"] {
        assert!(
            after_request_cookie.is_some_and(|value| value.contains(name)),
            "parent did not select {name} after the frame response: {after_request_cookie:?}"
        );
    }
}

pub(super) async fn native_content_process_form_post_uses_parent_cookie_authority_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for expected_path in ["/form", "/result", "/after"] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("parent-owned form navigation request should reach the server")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("form navigation request includes a path")
                .to_owned();
            assert_eq!(path, expected_path);
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (method, headers, content_type, body) = match path.as_str() {
                "/form" => (
                    "GET",
                    "Set-Cookie: form_page_secret=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    "<!doctype html><form id='form' action='/result' method='post'><input name='query' value='hello'></form>",
                ),
                "/result" => {
                    assert!(
                        cookie
                            .as_deref()
                            .is_some_and(|value| value.contains("form_page_secret=seed")),
                        "parent did not select the form page cookie for POST: {request}"
                    );
                    assert!(request.starts_with("POST /result HTTP/1.1\r\n"));
                    assert!(request.lines().any(|line| {
                        line.eq_ignore_ascii_case("content-type: application/x-www-form-urlencoded")
                    }));
                    assert_eq!(request.split("\r\n\r\n").nth(1), Some("query=hello"));
                    (
                        "POST",
                        "Set-Cookie: form_result_secret=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                        "text/html",
                        "<!doctype html><title>Form result</title><p>posted</p>",
                    )
                }
                "/after" => {
                    for name in ["form_page_secret=seed", "form_result_secret=accepted"] {
                        assert!(
                            cookie.as_deref().is_some_and(|value| value.contains(name)),
                            "parent did not select {name} for the later Fetch: {request}"
                        );
                    }
                    ("GET", "", "text/plain", "form response cookie retained")
                }
                other => panic!("unexpected form navigation path: {other}"),
            };
            assert!(request.starts_with(&format!("{method} {expected_path} HTTP/1.1")));
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie));
        }
        requests
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
        "Form result"
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
        serde_json::json!("form response cookie retained")
    );
    session.close().await.unwrap();
    let requests = server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/form", "/result", "/after"]
    );
}

pub(super) async fn canonical_browser_session_exposes_history_controls_inner() {
    let session = BrowserSession::start(
        NativeEngineConfig::default()
            .with_fixture("fixture://runtime-history-first", "<p>First</p>")
            .unwrap()
            .with_fixture("fixture://runtime-history-second", "<p>Second</p>")
            .unwrap()
            .with_initial_url("fixture://runtime-history-first"),
    )
    .await
    .unwrap();
    session
        .navigate("fixture://runtime-history-second")
        .await
        .unwrap();
    let second = session.observe().await.unwrap();

    let back = session
        .go_back_with_revision(second.revision)
        .await
        .unwrap();
    assert_eq!(back.action, "back");
    assert_eq!(back.previous_revision, second.revision);
    assert!(back.current_revision > back.previous_revision);
    assert!(
        session
            .go_forward_with_revision(second.revision)
            .await
            .is_err(),
        "history traversal must reject a stale observation"
    );
    let first = session.observe().await.unwrap();
    assert_eq!(first.revision, back.current_revision);
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .visible_text,
        "First"
    );

    let forward = session.go_forward().await.unwrap();
    assert_eq!(forward.action, "forward");
    assert_eq!(forward.previous_revision, first.revision);
    assert!(forward.current_revision > forward.previous_revision);
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .visible_text,
        "Second"
    );

    let back = session.go_back().await.unwrap();
    assert_eq!(back.action, "back");
    assert_eq!(back.previous_revision, forward.current_revision);
    assert!(back.current_revision > back.previous_revision);
    let first_again = session.observe().await.unwrap();
    let forward = session
        .go_forward_with_revision(first_again.revision)
        .await
        .unwrap();
    assert_eq!(forward.action, "forward");
    assert_eq!(forward.previous_revision, first_again.revision);
    assert!(forward.current_revision > forward.previous_revision);

    let reload = session
        .reload_with_revision(forward.current_revision)
        .await
        .unwrap();
    assert_eq!(reload.action, "reload");
    assert_eq!(reload.previous_revision, forward.current_revision);
    assert!(reload.current_revision > reload.previous_revision);
    assert!(
        session
            .reload_with_revision(forward.current_revision)
            .await
            .is_err(),
        "reload must reject a stale observation"
    );
    let after_reload = session.observe().await.unwrap();
    assert_eq!(after_reload.revision, reload.current_revision);
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .visible_text,
        "Second"
    );
    session.close().await.unwrap();
}

pub(super) async fn canonical_browser_session_exposes_guarded_and_unguarded_recovery_inner() {
    let session = BrowserSession::start(
        NativeEngineConfig::default()
            .with_fixture("fixture://canonical-recovery", "<p>Recovery target</p>")
            .unwrap()
            .with_initial_url("fixture://canonical-recovery"),
    )
    .await
    .unwrap();

    let revision = session.observe().await.unwrap().revision;
    assert!(session.recover_with_revision(revision + 1).await.is_err());
    assert_eq!(session.observe().await.unwrap().revision, revision);

    let guarded = session.recover_with_revision(revision).await.unwrap();
    assert_eq!(guarded.action, "recover");
    assert_eq!(guarded.previous_revision, revision);
    assert!(guarded.current_revision > guarded.previous_revision);

    let unguarded = session.recover().await.unwrap();
    assert_eq!(unguarded.action, "recover");
    assert_eq!(unguarded.previous_revision, guarded.current_revision);
    assert!(unguarded.current_revision > unguarded.previous_revision);
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        "fixture://canonical-recovery"
    );

    session.close().await.unwrap();
}

pub(super) async fn canonical_browser_session_stop_loading_cancels_and_restarts_worker_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request_seen_tx, request_seen_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let slow_server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/slow"));
        let _ = request_seen_tx.send(());
        let _ = release_rx.await;
        let body = "<title>Must not commit</title><p>late response</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    let session = BrowserSession::start_default().await.unwrap();
    let initial = session.evidence(EvidenceLevel::Compact).await.unwrap();
    let idle = session
        .stop_loading_with_revision(initial.revision)
        .await
        .unwrap();
    assert_eq!(idle.current_revision, initial.revision);

    {
        let navigation = session.navigate(format!("http://{address}/slow"));
        tokio::pin!(navigation);
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::select! {
                result = &mut navigation => match result {
                    Ok(_) => panic!("delayed navigation completed before its response was released"),
                    Err(error) => panic!("delayed navigation failed before stop-loading: {error}"),
                },
                request_seen = request_seen_rx => request_seen.unwrap(),
            }
        })
        .await
        .expect("native navigation did not reach the delayed response");

        assert!(
            session
                .stop_loading_with_revision(initial.revision.saturating_add(1))
                .await
                .is_err(),
            "a stale stop request must not cancel the active navigation"
        );
        let stopped = tokio::time::timeout(
            Duration::from_secs(5),
            session.stop_loading_with_revision(initial.revision),
        )
        .await
        .expect("stop-loading did not settle promptly")
        .unwrap();
        assert_eq!(stopped.action, "stopLoading");
        assert_eq!(stopped.current_revision, initial.revision);

        let navigation_error = tokio::time::timeout(Duration::from_secs(5), &mut navigation)
            .await
            .expect("cancelled navigation did not return")
            .expect_err("the cancelled navigation must not report success");
        assert!(matches!(
            navigation_error.downcast_ref::<BrowserBackendError>(),
            Some(BrowserBackendError::Lifecycle { state, .. }) if state == "cancelled"
        ));
    }
    let _ = release_tx.send(());
    tokio::time::timeout(Duration::from_secs(5), slow_server)
        .await
        .expect("delayed fixture server did not stop")
        .unwrap();

    let after_stop = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(after_stop.url, initial.url);
    assert_eq!(after_stop.revision, initial.revision);

    let next_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let next_address = next_listener.local_addr().unwrap();
    let next_server = tokio::spawn(async move {
        let (mut stream, _) = next_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/after-stop"));
        let body = "<title>Worker restarted</title><p>ready</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let next = session
        .navigate(format!("http://{next_address}/after-stop"))
        .await
        .expect("native content worker should restart after cancellation");
    assert_eq!(next.url, format!("http://{next_address}/after-stop"));
    assert!(next.revision > initial.revision);
    tokio::time::timeout(Duration::from_secs(5), next_server)
        .await
        .expect("post-cancellation fixture server did not stop")
        .unwrap();
    session.close().await.unwrap();
}

#[cfg(unix)]
pub(super) async fn native_backend_captures_all_visual_formats_without_mutating_revision_inner() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 24,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://capture",
            "<html><body><style>#card { width: 8px; height: 8px; background-color: red; }</style><div id='card'>A</div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://capture");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let before = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();

    let capture = dispatcher
        .capture(CaptureRequest {
            context_id: "native-context".into(),
            format: CaptureFormat::Png,
        })
        .await
        .unwrap();
    assert_eq!(capture.format, CaptureFormat::Png);
    assert_eq!(&capture.bytes[..8], b"\x89PNG\r\n\x1a\n");
    let decoder = png::Decoder::new(Cursor::new(capture.bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (24, 24));
    let red_pixel = 9 * output.line_size + 9 * 4;
    assert_eq!(&decoded[red_pixel..red_pixel + 4], &[255, 0, 0, 255]);

    let after = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(after.revision, before.revision);

    let jpeg = dispatcher
        .capture(CaptureRequest {
            context_id: "native-context".into(),
            format: CaptureFormat::Jpeg,
        })
        .await
        .unwrap();
    assert_eq!(jpeg.format, CaptureFormat::Jpeg);
    assert_eq!(&jpeg.bytes[..2], &[0xff, 0xd8]);
    let jpeg_size = imagesize::blob_size(&jpeg.bytes).unwrap();
    assert_eq!((jpeg_size.width, jpeg_size.height), (24, 24));

    let webp = backend
        .capture_visual(&VisualCaptureOptions {
            format: VisualFormat::Webp,
            quality: Some(75),
            ..VisualCaptureOptions::default()
        })
        .await
        .unwrap();
    assert_eq!(webp.metadata.format, VisualFormat::Webp);
    let webp_bytes = base64::engine::general_purpose::STANDARD
        .decode(webp.data.as_bytes())
        .unwrap();
    assert_eq!(&webp_bytes[..4], b"RIFF");
    let webp_size = imagesize::blob_size(&webp_bytes).unwrap();
    assert_eq!((webp_size.width, webp_size.height), (24, 24));
    dispatcher.close().await.unwrap();
}

pub(super) async fn native_backend_composes_nested_frame_surfaces_into_capture_inner() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 64,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://frame-composite-parent",
            "<html><body><iframe id='child' style='display:block;width:40px;height:32px' src='fixture://frame-composite-child'></iframe></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://frame-composite-child",
            "<html><body><iframe id='grand' style='display:block;width:24px;height:16px' src='fixture://frame-composite-grand'></iframe><div style='display:block;width:8px;height:6px;background-color:blue'></div></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://frame-composite-grand",
            "<html><body><div style='display:block;width:4px;height:3px;background-color:red'></div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://frame-composite-parent");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let capture = dispatcher
        .capture(CaptureRequest {
            context_id: "native-context".into(),
            format: CaptureFormat::Png,
        })
        .await
        .unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture.bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (64, 64));
    let pixel = |x: usize, y: usize| {
        let index = y
            .saturating_mul(output.line_size)
            .saturating_add(x.saturating_mul(4));
        &decoded[index..index + 4]
    };
    assert_eq!(pixel(25, 25), &[255, 0, 0, 255]);
    assert_eq!(pixel(17, 33), &[0, 0, 255, 255]);
    assert_eq!(pixel(50, 1), &[255, 255, 255, 255]);

    let child = backend
        .list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.is_some())
        .unwrap();
    backend.select_frame(&child.id).await.unwrap();
    assert_eq!(
        backend.inspection_snapshot().unwrap().layout.viewport,
        Viewport {
            width: 40,
            height: 32,
            device_scale_factor_milli: 1000,
        }
    );
    dispatcher.close().await.unwrap();
}

pub(super) async fn native_backend_routes_point_clicks_into_nested_frame_content_inner() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 64,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://frame-click-parent",
            "<html><body><iframe id='child' style='display:block;width:40px;height:32px' src='fixture://frame-click-child'></iframe></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://frame-click-child",
            "<html><body><iframe id='grand' style='display:block;width:32px;height:24px' src='fixture://frame-click-grand'></iframe></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://frame-click-grand",
            "<html><body><input id='inside' style='display:block;width:8px;height:2px' value='before'><input id='next' style='display:block;width:8px;height:2px' value='after'></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://frame-click-parent");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let clicked = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "point=25,25".into(),
            },
        })
        .await
        .unwrap();
    assert!(clicked.accepted);
    assert_eq!(clicked.revision, 2);

    let typed = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::KeyPress { key: "a".into() },
        })
        .await
        .unwrap();
    assert!(typed.accepted);
    assert_eq!(typed.revision, 3);

    let tabbed = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Shortcut {
                shortcut: "Tab".into(),
            },
        })
        .await
        .unwrap();
    assert!(tabbed.accepted);
    assert_eq!(tabbed.revision, 4);

    let reverse_tabbed = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Shortcut {
                shortcut: "Shift+Tab".into(),
            },
        })
        .await
        .unwrap();
    assert!(reverse_tabbed.accepted);
    assert_eq!(reverse_tabbed.revision, 5);

    let located = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "id=next".into(),
            },
        })
        .await
        .unwrap();
    assert!(located.accepted);
    assert_eq!(located.revision, 6);

    let frames = backend.list_frames().await.unwrap();
    let child = frames
        .iter()
        .find(|frame| frame.url == "fixture://frame-click-grand")
        .unwrap();
    backend.select_frame(&child.id).await.unwrap();
    let value = dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "({ active: document.activeElement.id, value: document.getElementById('inside').value })".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        value.value,
        serde_json::json!({"active": "next", "value": "beforea"})
    );
    dispatcher.close().await.unwrap();
}

pub(super) async fn native_backend_dispatches_vertical_scroll_into_capture_inner() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 80,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://scroll-dispatch",
            "<html><body><div style='display:block;height:24px;background-color:red'>Top</div><button id='bottom' style='display:block;height:24px;background-color:blue'>Bottom</button></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://scroll-dispatch");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let action = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Scroll {
                delta_x: 0,
                delta_y: i32::MAX,
            },
        })
        .await
        .unwrap();
    assert_eq!(action.revision, 2);
    assert!(action.accepted);
    let capture = dispatcher
        .capture(CaptureRequest {
            context_id: "native-context".into(),
            format: CaptureFormat::Png,
        })
        .await
        .unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture.bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (80, 24));
    assert!(
        decoded
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 255, 255]),
        "scrolled button should be visible"
    );

    let horizontal_edge = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Scroll {
                delta_x: 1,
                delta_y: 0,
            },
        })
        .await
        .unwrap();
    assert!(!horizontal_edge.accepted);
    dispatcher.close().await.unwrap();
}

pub(super) async fn native_backend_dispatches_horizontal_scroll_into_capture_inner() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 64,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://wide-dispatch",
            "<html><body><div style='white-space:nowrap;background-color:red'>AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA</div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://wide-dispatch");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let action = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Scroll {
                delta_x: i32::MAX,
                delta_y: 0,
            },
        })
        .await
        .unwrap();
    assert_eq!(action.revision, 2);
    assert!(action.accepted);
    assert!(
        backend
            .inspection_snapshot()
            .unwrap()
            .layout
            .scroll_offset
            .x
            > 0
    );

    let capture = dispatcher
        .capture(CaptureRequest {
            context_id: "native-context".into(),
            format: CaptureFormat::Png,
        })
        .await
        .unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture.bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (64, 24));
    let pixel_index = 9 * output.line_size + 9 * 4;
    assert_eq!(
        &decoded[pixel_index..pixel_index + 4],
        &[255, 255, 255, 255]
    );

    let edge = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Scroll {
                delta_x: i32::MAX,
                delta_y: 0,
            },
        })
        .await
        .unwrap();
    assert_eq!(edge.revision, 2);
    assert!(!edge.accepted);
    dispatcher.close().await.unwrap();
}

pub(super) async fn fixture_navigation_projects_through_the_real_backend_dispatcher_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://welcome",
            "<!doctype html><html><head><title>Welcome</title><script>ignored()</script></head><body><h1>Hello &amp; Glass</h1><p>Native fixture</p></body></html>",
        )
        .unwrap();
    let backend = NativeEngineBackend::new(config).unwrap();
    assert_eq!(
        backend.profile().identity.backend_id,
        NATIVE_ENGINE_BACKEND_ID
    );
    assert_eq!(
        backend.profile().identity.certification.level,
        CertificationLevel::Partial
    );
    assert_eq!(
        backend
            .profile()
            .capability(BrowserCapability::Action)
            .level,
        SupportLevel::Available
    );
    assert_eq!(
        backend
            .profile()
            .capability(BrowserCapability::Effects)
            .level,
        SupportLevel::Available
    );
    assert_eq!(
        backend
            .profile()
            .capability(BrowserCapability::Capture)
            .level,
        SupportLevel::Available
    );
    assert!(
        !backend
            .profile()
            .capability(BrowserCapability::Action)
            .limitations
            .iter()
            .any(|limitation| limitation.contains("nested scrolling remain open"))
    );
    assert!(
        !backend
            .profile()
            .capability(BrowserCapability::Storage)
            .limitations
            .iter()
            .any(|limitation| limitation.contains("IndexedDB remains open"))
    );
    assert!(
        !backend
            .profile()
            .identity
            .certification
            .limitations
            .iter()
            .any(|limitation| limitation.contains("IndexedDB unavailable"))
    );

    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let initial = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(initial.url, "about:blank");
    assert_eq!(initial.revision, 1);

    let navigation = dispatcher
        .navigate(NavigationRequest {
            url: "fixture://welcome".into(),
        })
        .await
        .unwrap();
    assert_eq!(navigation.url, "fixture://welcome");
    assert_eq!(navigation.revision, 2);

    let contexts = dispatcher
        .contexts(glass_browser::browser_backend::ContextRequest {
            include_background: false,
        })
        .await
        .unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].context_id, "native-context");
    assert!(contexts[0].active);

    let evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(evidence.title, "Welcome");
    assert_eq!(evidence.visible_text, "Hello & Glass Native fixture");
    assert!(evidence.complete);
    dispatcher.close().await.unwrap();
}

pub(super) async fn data_url_and_limits_are_bounded_inner() {
    let limits = NativeEngineLimits {
        max_document_bytes: 128,
        ..NativeEngineLimits::default()
    };
    let config = NativeEngineConfig::default().with_limits(limits);
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let navigation = dispatcher
        .navigate(NavigationRequest {
            url: "data:text/html,%3Ctitle%3EData%3C%2Ftitle%3E%3Cp%3Eloaded%3C%2Fp%3E".into(),
        })
        .await
        .unwrap();
    assert_eq!(navigation.revision, 2);
    let evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(evidence.title, "Data");
    assert_eq!(evidence.visible_text, "loaded");

    let base64_url = "data:text/html;base64,PHRpdGxlPkRhdGE8L3RpdGxlPjxwPmxvYWRlZDwvcD4=";
    let navigation = dispatcher
        .navigate(NavigationRequest {
            url: base64_url.into(),
        })
        .await
        .unwrap();
    assert_eq!(navigation.url, base64_url);
    let base64_evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(base64_evidence.url, base64_url);
    assert_eq!(base64_evidence.title, "Data");
    assert_eq!(base64_evidence.visible_text, "loaded");
    dispatcher.close().await.unwrap();

    let mut engine = NativeEngine::new(NativeEngineConfig::default()).unwrap();
    engine.initialize().unwrap();
    assert!(matches!(
        engine.navigate("data:text/html;base64,not-base64"),
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));
    assert!(matches!(
        engine.navigate("data:text/html;base64,PHRpdGxlPkRhdGE8L3RpdGxlPj%3D"),
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));
    assert!(matches!(
        engine.navigate("data:text/plain;base64,SGk="),
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));
    assert!(matches!(
        engine.navigate("data:text/html;base64,/w=="),
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));

    let small_limits = NativeEngineLimits {
        max_document_bytes: 8,
        ..NativeEngineLimits::default()
    };
    let mut small_engine =
        NativeEngine::new(NativeEngineConfig::default().with_limits(small_limits)).unwrap();
    small_engine.initialize().unwrap();
    assert!(matches!(
        small_engine.navigate("data:text/html;base64,PHRpdGxlPkRhdGE8L3RpdGxlPg=="),
        Err(NativeEngineError::LimitExceeded { .. })
    ));

    let tiny_limits = NativeEngineLimits {
        max_document_bytes: 3,
        ..NativeEngineLimits::default()
    };
    let mut tiny_engine =
        NativeEngine::new(NativeEngineConfig::default().with_limits(tiny_limits)).unwrap();
    tiny_engine.initialize().unwrap();
    assert!(matches!(
        tiny_engine.navigate("data:text/html;base64,AAAAAA"),
        Err(NativeEngineError::LimitExceeded { .. })
    ));
}

pub(super) async fn local_fragment_navigation_and_history_traversal_preserve_bounded_state_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://history",
            "<button id='toggle'>Toggle</button><p>History body</p>",
        )
        .unwrap()
        .with_fixture("fixture://other", "<p>Other document</p>")
        .unwrap()
        .with_fixture("fixture://replacement", "<p>Replacement</p>")
        .unwrap()
        .with_initial_url("fixture://history#top");

    let backend = NativeEngineBackend::new(config.clone()).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let fragment = dispatcher
        .navigate(NavigationRequest {
            url: "fixture://history#middle".into(),
        })
        .await
        .unwrap();
    assert_eq!(fragment.url, "fixture://history#middle");
    assert_eq!(fragment.revision, 2);
    let fragment_evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(fragment_evidence.url, "fixture://history#middle");
    assert_eq!(fragment_evidence.visible_text, "Toggle History body");
    dispatcher.close().await.unwrap();

    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let initial_nodes = engine.semantic_nodes().unwrap();
    let initial_ids = initial_nodes
        .iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    let initial_layout = engine.layout().unwrap();
    let initial_scroll = engine.scroll_offset();
    assert_eq!(engine.history().len(), 1);
    assert!(!engine.history().can_go_back());
    assert!(!engine.history().can_go_forward());

    let middle = engine.navigate("fixture://history#middle").unwrap();
    assert_eq!(middle.url, "fixture://history#middle");
    assert_eq!(middle.revision, 2);
    assert_eq!(engine.history().len(), 2);
    assert!(engine.history().can_go_back());
    assert!(!engine.history().can_go_forward());
    assert_eq!(
        engine
            .semantic_nodes()
            .unwrap()
            .iter()
            .map(|node| node.node_id)
            .collect::<Vec<_>>(),
        initial_ids
    );
    let mut middle_layout = engine.layout().unwrap();
    assert_eq!(middle_layout.revision, 2);
    middle_layout.revision = initial_layout.revision;
    assert_eq!(middle_layout, initial_layout);
    assert_eq!(engine.scroll_offset(), initial_scroll);

    let top = engine.go_back().unwrap().unwrap();
    assert_eq!(top.url, "fixture://history#top");
    assert_eq!(top.revision, 3);
    assert_eq!(
        engine.history().current().unwrap().url,
        "fixture://history#top"
    );
    assert_eq!(engine.history().current().unwrap().revision, 3);
    assert_eq!(
        engine
            .semantic_nodes()
            .unwrap()
            .iter()
            .map(|node| node.node_id)
            .collect::<Vec<_>>(),
        initial_ids
    );
    let boundary = engine.snapshot().unwrap();
    assert!(engine.go_back().unwrap().is_none());
    assert_eq!(engine.snapshot().unwrap(), boundary);

    let middle_again = engine.go_forward().unwrap().unwrap();
    assert_eq!(middle_again.url, "fixture://history#middle");
    assert_eq!(middle_again.revision, 4);
    assert_eq!(engine.history().current().unwrap().revision, 4);

    let other = engine.navigate("fixture://other").unwrap();
    assert_eq!(other.visible_text, "Other document");
    assert_eq!(other.revision, 5);
    assert_eq!(engine.history().len(), 3);
    assert!(!engine.history().can_go_forward());
    let restored_middle = engine.go_back().unwrap().unwrap();
    assert_eq!(restored_middle.url, "fixture://history#middle");
    assert_eq!(restored_middle.visible_text, "Toggle History body");
    assert_eq!(restored_middle.revision, 6);
    assert_ne!(
        engine
            .semantic_nodes()
            .unwrap()
            .first()
            .map(|node| node.node_id.generation()),
        initial_nodes.first().map(|node| node.node_id.generation())
    );

    let restored_top = engine.go_back().unwrap().unwrap();
    assert_eq!(restored_top.url, "fixture://history#top");
    assert_eq!(restored_top.revision, 7);
    assert_eq!(engine.history().current().unwrap().revision, 7);
    let replacement = engine.navigate("fixture://replacement").unwrap();
    assert_eq!(replacement.visible_text, "Replacement");
    assert_eq!(engine.history().len(), 2);
    assert!(engine.go_forward().unwrap().is_none());

    let before_failed = engine.snapshot().unwrap();
    let before_history = engine.history().clone();
    assert!(matches!(
        engine.navigate("fixture://missing#fragment"),
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));
    assert_eq!(engine.snapshot().unwrap(), before_failed);
    assert_eq!(engine.history(), &before_history);

    let mut about_engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url("about:blank#intro"))
            .unwrap();
    about_engine.initialize().unwrap();
    assert_eq!(about_engine.snapshot().unwrap().url, "about:blank#intro");

    let mut data_engine = NativeEngine::new(NativeEngineConfig::default()).unwrap();
    data_engine.initialize().unwrap();
    let data = data_engine
        .navigate("data:text/html,%3Cp%3Ehash%23value%3C%2Fp%3E#part")
        .unwrap();
    assert_eq!(
        data.url,
        "data:text/html,%3Cp%3Ehash%23value%3C%2Fp%3E#part"
    );
    assert_eq!(data.visible_text, "hash#value");
}

pub(super) async fn local_link_activation_uses_bounded_navigation_default_action_inner() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 160,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://links",
            native_html_without_body_margin("<a id='fragment' href='#part'>Part</a><a id='other' href='fixture://other'>Other</a><a id='remote' href='https://example.com'>Remote</a><a id='relative' href='next#target'>Next</a><a id='missing' href='missing'>Missing</a><a id='host-change' href='//other.test/path'>Host change</a><p>one two three four five six seven eight nine ten</p><p>eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one</p>"),
        )
        .unwrap()
        .with_fixture("fixture://other", "<p>Other document</p>")
        .unwrap()
        .with_fixture(
            "fixture://links/next",
            "<p id='target'>Relative document</p>",
        )
        .unwrap()
        .with_initial_url("fixture://links#top");

    let backend = NativeEngineBackend::new(config.clone()).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let before_remote = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    let remote_error = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "id=remote".into(),
            },
        })
        .await
        .unwrap_err();
    assert!(remote_error.to_string().contains("invalid navigation URL"));
    assert!(!remote_error.to_string().contains("example.com"));
    assert_eq!(
        dispatcher
            .evidence(EvidenceRequest {
                context_id: "native-context".into(),
                level: EvidenceLevel::Compact,
            })
            .await
            .unwrap(),
        before_remote
    );

    let fragment = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "id=fragment".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(fragment.revision, 2);
    assert!(fragment.accepted);
    let fragment_evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(fragment_evidence.url, "fixture://links#part");
    assert!(fragment_evidence.visible_text.contains("Part"));

    let other = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "id=other".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(other.revision, 3);
    let other_evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(other_evidence.url, "fixture://other");
    assert_eq!(other_evidence.visible_text, "Other document");
    dispatcher.close().await.unwrap();

    let relative_backend = NativeEngineBackend::new(config.clone()).unwrap();
    let relative_dispatcher = BrowserBackendDispatcher::new(&relative_backend);
    relative_dispatcher.initialize().await.unwrap();
    let relative_dispatch = relative_dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "id=relative".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(relative_dispatch.revision, 2);
    assert!(relative_dispatch.accepted);
    let relative_evidence = relative_dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(relative_evidence.url, "fixture://links/next#target");
    assert_eq!(relative_evidence.visible_text, "Relative document");
    relative_dispatcher.close().await.unwrap();

    let mut engine = NativeEngine::new(config.clone()).unwrap();
    engine.initialize().unwrap();
    engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 1,
        })
        .unwrap();
    let scroll_before = engine.scroll_offset();
    assert!(scroll_before.y > 0);
    let ids_before = engine
        .semantic_nodes()
        .unwrap()
        .iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    let fragment_id = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Part")
        .map(|node| node.node_id)
        .unwrap();
    let fragment = engine
        .action(NativeAction::Click {
            target: "id=fragment".into(),
        })
        .unwrap();
    assert!(fragment.accepted);
    assert_eq!(fragment.revision, 3);
    assert_eq!(engine.snapshot().unwrap().url, "fixture://links#part");
    assert_eq!(engine.history().len(), 2);
    assert_eq!(engine.scroll_offset(), scroll_before);
    assert_eq!(
        engine
            .semantic_nodes()
            .unwrap()
            .iter()
            .map(|node| node.node_id)
            .collect::<Vec<_>>(),
        ids_before
    );
    assert!(
        engine
            .semantic_nodes()
            .unwrap()
            .iter()
            .any(|node| node.node_id == fragment_id && node.focused)
    );

    let mut relative_engine = NativeEngine::new(config.clone()).unwrap();
    relative_engine.initialize().unwrap();
    let relative = relative_engine
        .action(NativeAction::Click {
            target: "id=relative".into(),
        })
        .unwrap();
    assert!(relative.accepted);
    assert_eq!(relative.revision, 2);
    assert_eq!(
        relative_engine.snapshot().unwrap().url,
        "fixture://links/next#target"
    );
    assert_eq!(
        relative_engine.snapshot().unwrap().visible_text,
        "Relative document"
    );
    assert_eq!(relative_engine.history().len(), 2);

    for (target, forbidden_text) in [("id=missing", "missing"), ("id=host-change", "other.test")] {
        let mut failed_engine = NativeEngine::new(config.clone()).unwrap();
        failed_engine.initialize().unwrap();
        let before_failed = failed_engine.snapshot().unwrap();
        let before_failed_history = failed_engine.history().clone();
        let error = failed_engine
            .action(NativeAction::Click {
                target: target.into(),
            })
            .unwrap_err();
        assert!(
            matches!(error, NativeEngineError::UnsupportedUrl { .. }),
            "{target}: {error}"
        );
        assert!(!error.to_string().contains(forbidden_text));
        assert_eq!(failed_engine.snapshot().unwrap(), before_failed);
        assert_eq!(failed_engine.history(), &before_failed_history);
    }

    let malformed_config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://malformed",
            "<a id='malformed' href='//['>Malformed</a>",
        )
        .unwrap()
        .with_initial_url("fixture://malformed");
    let mut malformed_engine = NativeEngine::new(malformed_config).unwrap();
    malformed_engine.initialize().unwrap();
    let before_malformed = malformed_engine.snapshot().unwrap();
    let before_malformed_history = malformed_engine.history().clone();
    let malformed_error = malformed_engine
        .action(NativeAction::Click {
            target: "id=malformed".into(),
        })
        .unwrap_err();
    assert!(
        matches!(malformed_error, NativeEngineError::UnsupportedUrl { .. }),
        "{malformed_error}"
    );
    assert!(!malformed_error.to_string().contains("//["));
    assert_eq!(malformed_engine.snapshot().unwrap(), before_malformed);
    assert_eq!(malformed_engine.history(), &before_malformed_history);

    let empty_config = NativeEngineConfig::default()
        .with_fixture("fixture://empty", "<a id='empty' href=''>Empty</a>")
        .unwrap()
        .with_initial_url("fixture://empty");
    let mut empty_engine = NativeEngine::new(empty_config).unwrap();
    empty_engine.initialize().unwrap();
    let empty = empty_engine
        .action(NativeAction::Click {
            target: "id=empty".into(),
        })
        .unwrap();
    assert!(empty.accepted);
    assert_eq!(empty.revision, 2);
    assert_eq!(empty_engine.snapshot().unwrap().url, "fixture://empty");
    assert_eq!(empty_engine.history().len(), 1);
}

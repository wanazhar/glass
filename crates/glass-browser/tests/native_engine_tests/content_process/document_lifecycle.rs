use super::*;

#[test]
fn native_content_process_history_beforeunload_waits_before_loading_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_history_beforeunload_waits_before_loading_target_inner(),
        );
    });
}

async fn native_content_process_history_beforeunload_waits_before_loading_target_inner() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let _guard = native_content_process_test_lock().lock().await;
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (additional_request_tx, mut additional_request_rx) = oneshot::channel();
            let (server_stop_tx, mut server_stop_rx) = oneshot::channel();
            let server = tokio::spawn(async move {
                let mut additional_request_tx = Some(additional_request_tx);
                for expected_path in ["/first", "/second"] {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let requested_path = request.split_whitespace().nth(1).unwrap_or_default();
                    assert_eq!(requested_path, expected_path);
                    let body = if expected_path == "/first" {
                        "<title>First</title><button id='activate'>Activate</button>"
                    } else {
                        "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'must not be shown'; globalThis.beforeunloadRan = 'yes'; }); addEventListener('pagehide', () => globalThis.pagehideRan = 'yes'); addEventListener('unload', () => globalThis.unloadRan = 'yes');</script><title>Second</title><button id='activate'>Activate</button>"
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }

                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut stream, _) = accepted.unwrap();
                        let request = read_http_request(&mut stream).await;
                        let requested_path = request.split_whitespace().nth(1).unwrap_or_default();
                        if let Some(sender) = additional_request_tx.take() {
                            let _ = sender.send(requested_path.to_owned());
                        }
                        let body = "<title>First</title><button id='activate'>Activate</button>";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    }
                    _ = &mut server_stop_rx => {}
                }
            });

            let session = Arc::new(
                BrowserRuntimeSession::connect_native_with_modal_dialogs(
                    NativeEngineConfig::default(),
                )
                .await
                .unwrap(),
            );
            let controller = session.native_dialog_controller().unwrap();
            let first_url = format!("http://{address}/first");
            let second_url = format!("http://{address}/second");
            session.navigate(first_url.clone()).await.unwrap();
            session.navigate(second_url.clone()).await.unwrap();
            session
                .action(SemanticAction::Click {
                    target: "id=activate".into(),
                })
                .await
                .unwrap();

            let first_back_session = Arc::clone(&session);
            let first_back = tokio::task::spawn_local(async move { first_back_session.go_back().await });
            let first_prompt = tokio::select! {
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => prompt,
                request = &mut additional_request_rx => panic!("history target was requested before the user decision: {request:?}"),
            };
            assert!(first_prompt.dialog.message.is_empty());
            assert!(!first_back.is_finished());
            controller
                .resolve_dialog(
                    &first_prompt.id,
                    NativeDialogResolution {
                        accepted: false,
                        prompt_value: None,
                    },
                )
                .unwrap();
            let dismissed = first_back.await.unwrap().unwrap();
            assert_eq!(dismissed.action, "back");
            assert_eq!(
                session.evidence(EvidenceLevel::Compact).await.unwrap().url,
                second_url
            );
            assert_eq!(
                session
                    .script("[globalThis.beforeunloadRan || null, globalThis.pagehideRan || null, globalThis.unloadRan || null]")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!(["yes", null, null])
            );
            assert!(tokio::time::timeout(Duration::from_millis(50), &mut additional_request_rx)
                .await
                .is_err());

            let second_back_session = Arc::clone(&session);
            let second_back = tokio::task::spawn_local(async move { second_back_session.go_back().await });
            let second_prompt = tokio::select! {
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => prompt,
                request = &mut additional_request_rx => panic!("history target was requested before the user decision: {request:?}"),
            };
            assert_ne!(first_prompt.id, second_prompt.id);
            controller
                .resolve_dialog(
                    &second_prompt.id,
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: None,
                    },
                )
                .unwrap();
            let traversed = tokio::time::timeout(Duration::from_secs(10), second_back)
                .await
                .expect("accepted history traversal did not finish")
                .unwrap()
                .unwrap();
            assert_eq!(traversed.action, "back");
            assert_eq!(
                session.evidence(EvidenceLevel::Compact).await.unwrap().url,
                first_url,
                "accepted history traversal returned without activating its target"
            );
            if let Ok(Ok(requested_path)) = tokio::time::timeout(
                Duration::from_millis(50),
                &mut additional_request_rx,
            )
            .await
            {
                assert_eq!(requested_path, "/first");
            }

            let _ = server_stop_tx.send(());
            Arc::try_unwrap(session)
                .unwrap_or_else(|_| panic!("native session still has unexpected owners"))
                .close()
                .await
                .unwrap();
            server.await.unwrap();
        })
        .await;
}
#[tokio::test]
async fn native_content_process_forwards_page_dialogs_without_chromium() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/dialogs"));
        let body = "<script>alert('loaded');</script><p>worker dialogs</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/dialogs")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let pending = engine.pending_dialog().unwrap().unwrap();
    assert_eq!(pending.dialog_type, "alert");
    assert_eq!(pending.message, "loaded");
    assert_eq!(pending.url, format!("http://{address}/dialogs"));
    assert!(
        engine
            .resolve_dialog(PromptDecision::Accept)
            .unwrap()
            .handled
    );

    assert_eq!(
        engine
            .evaluate_async("prompt('worker', 'default')")
            .await
            .unwrap(),
        serde_json::Value::Null
    );
    let pending = engine.pending_dialog().unwrap().unwrap();
    assert_eq!(pending.dialog_type, "prompt");
    assert_eq!(pending.default_value.as_deref(), Some("default"));
    assert!(
        engine
            .resolve_dialog(PromptDecision::Dismiss)
            .unwrap()
            .handled
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_http_tab_index_property_persists_in_content_process_and_frame() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_http_tab_index_property_persists_in_content_process_and_frame_inner());
    });
}

async fn native_http_tab_index_property_persists_in_content_process_and_frame_inner() {
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
                    "<div id='parent-property'></div><button id='parent-button'>Parent</button><iframe id='child' src='/child'></iframe>"
                }
                "/child" => {
                    "<div id='frame-property'></div><button id='frame-button'>Frame</button>"
                }
                other => panic!("unexpected tabIndex request path: {other}"),
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
    assert_eq!(
        session
            .script(
                "const target = document.getElementById('parent-property'); const before = target.tabIndex; target.tabIndex = 3.8; [before, target.tabIndex, target.getAttribute('tabindex')]",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([-1, 3, "3"])
    );
    assert_eq!(
        session
            .script("[document.getElementById('parent-property').tabIndex, document.getElementById('parent-property').getAttribute('tabindex')]")
            .await
            .unwrap()
            .value,
        serde_json::json!([3, "3"])
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
        serde_json::json!("parent-property")
    );

    let frames = session.native_list_frames().await.unwrap();
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin tabIndex frame")
        .id
        .clone();
    session.native_select_frame(&child_id).await.unwrap();
    assert_eq!(
        session
            .script(
                "const target = document.getElementById('frame-property'); target.tabIndex = 1.9; [target.tabIndex, target.getAttribute('tabindex')]",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([1, "1"])
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
        serde_json::json!("frame-property")
    );
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_http_window_proxy_location_navigates_content_process_target() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_http_window_proxy_location_navigates_content_process_target_inner());
    });
}

async fn native_http_window_proxy_location_navigates_content_process_target_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/location-parent" => {
                    "<title>HTTP location parent</title><script>globalThis.popup = window.open('/location-child', 'location-target');</script><p>parent</p>"
                }
                "/location-child" => "<title>HTTP location child</title><p>child</p>",
                "/location-child-next" => "<title>HTTP location child next</title><p>next</p>",
                other => panic!("unexpected WindowProxy.location request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/location-parent")),
    )
    .await
    .unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 2);
    session
        .script("popup.location.replace('/location-child-next'); true")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    let child = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(child.url, format!("http://{address}/location-child-next"));
    assert_eq!(child.title, "HTTP location child next");
    session.close().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_async_http_history_traversal_reloads_through_content_process() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/first", "/second"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (title, body) = if expected_path == "/first" {
                ("First", "One")
            } else {
                ("Second", "Two")
            };
            let body = format!("<title>{title}</title><p>{body}</p>");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let first_url = format!("http://{address}/first");
    let second_url = format!("http://{address}/second");
    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(first_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    let second = engine.navigate_async(second_url.clone()).await.unwrap();
    assert_eq!(second.title, "Second");

    let first = engine.go_back_async().await.unwrap().unwrap();
    assert_eq!(first.url, first_url);
    assert_eq!(first.title, "First");
    let second = engine.go_forward_async().await.unwrap().unwrap();
    assert_eq!(second.url, second_url);
    assert_eq!(second.title, "Second");

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_script_exposes_same_document_history_api() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/app"));
        let body = "<html><body><div id='scroller' style='width:40px;height:30px;overflow:auto'><div style='width:80px;height:90px'></div></div><div style='height:140px'></div></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let initial_url = format!("http://{address}/app");
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 120,
                height: 100,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(initial_url.clone()),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ length: history.length, state: history.state, url: location.href })"
            )
            .await
            .unwrap(),
        serde_json::json!({"length": 1, "state": null, "url": initial_url})
    );

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo({ left: 20, top: 25 }); window.scrollTo({ top: 40 }); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 40])
    );

    let pushed_url = format!("http://{address}/app?screen=settings");
    engine
        .evaluate_async(
            "history.pushState({ screen: 'settings' }, '', '?screen=settings'); ({ length: history.length, state: history.state, url: location.href })",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo(0, 0); window.scrollTo(0, 0); return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, pushed_url);
    assert_eq!(engine.history().len(), 2);
    engine.evaluate_async("history.back(); true").await.unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    assert_eq!(
        engine
            .evaluate_async(
                "({ length: history.length, state: history.state, url: location.href })"
            )
            .await
            .unwrap(),
        serde_json::json!({"length": 2, "state": null, "url": initial_url})
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 40])
    );
    engine
        .evaluate_async("history.forward(); true")
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, pushed_url);
    assert_eq!(
        engine
            .evaluate_async("({ state: history.state, url: location.href })")
            .await
            .unwrap(),
        serde_json::json!({"state": {"screen": "settings"}, "url": pushed_url})
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([0, 0, 0])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_history_restores_nested_scroll_offsets_across_resource_navigation()
{
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (server_stop_tx, mut server_stop_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                _ = &mut server_stop_rx => break,
                accepted = listener.accept() => accepted,
            };
            let Ok((mut stream, _)) = accepted else {
                break;
            };
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let body = match path {
                "/a" => {
                    "<html><body><div id='scroller' style='width:40px;height:30px;overflow:auto'><div style='width:80px;height:90px'></div></div><div style='height:140px'></div></body></html>"
                }
                "/b" => "<html><body><p>Second document</p></body></html>",
                _ => "<html><body>Not found</body></html>",
            };
            let status = if matches!(path, "/a" | "/b") {
                "200 OK"
            } else {
                "404 Not Found"
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let initial_url = format!("http://{address}/a");
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 120,
                height: 100,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(initial_url.clone()),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); scroller.scrollTo({ left: 20, top: 25 }); window.scrollTo({ top: 40 }); return true; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 40 });
    assert_eq!(
        engine.history().entries()[0].scroll_offset,
        NativePoint { x: 0, y: 40 }
    );

    engine
        .navigate_async(format!("http://{address}/b"))
        .await
        .unwrap();
    assert_eq!(
        engine.history().entries()[0].scroll_offset,
        NativePoint { x: 0, y: 40 }
    );
    engine.go_back_async().await.unwrap().unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const scroller = document.getElementById('scroller'); return [scroller.scrollLeft, scroller.scrollTop, window.scrollY]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([20, 25, 40])
    );
    assert_eq!(
        engine
            .history()
            .current()
            .unwrap()
            .nested_scroll_offsets
            .len(),
        1
    );

    engine.close_async().await.unwrap();
    let _ = server_stop_tx.send(());
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_action_event_preserves_history_api() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/app"));
        let body = "<button id='push'>Push</button><script>document.getElementById('push').addEventListener('click', () => history.pushState({ view: 'next' }, '', '?screen=next'));</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let initial_url = format!("http://{address}/app");
    let pushed_url = format!("http://{address}/app?screen=next");
    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=push".into(),
        })
        .await
        .unwrap();

    assert_eq!(engine.snapshot().unwrap().url, pushed_url);
    assert_eq!(
        engine
            .evaluate_async(
                "({ length: history.length, state: history.state, url: location.href })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "length": 2,
            "state": {"view": "next"},
            "url": pushed_url,
        })
    );
    engine.evaluate_async("history.back(); true").await.unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    assert_eq!(
        engine
            .evaluate_async("({ state: history.state, url: location.href })")
            .await
            .unwrap(),
        serde_json::json!({"state": null, "url": initial_url})
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_document_fragments_cross_the_http_boundary() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/fragment"));
        let body = "<html><body><div id='mount'></div></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/fragment")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const mount = document.getElementById('mount'); const fragment = document.createDocumentFragment(); const heading = document.createElement('h1'); heading.textContent = 'Native'; fragment.append('before-', heading); const detached = document.createElement('aside'); fragment.prepend(detached); const staged = [fragment instanceof DocumentFragment, fragment.ownerDocument === document, fragment.childNodes.length, detached.parentNode === fragment, detached.parentElement === null]; mount.appendChild(fragment); const text = document.createTextNode('-after'); heading.after(text); return [staged, fragment.childNodes.length, mount.innerHTML, mount.textContent, heading.parentElement === mount, text.parentNode === heading.parentNode, mount.childNodes.length]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([[
            true, true, 3, true, true
        ], 0, "<aside></aside>before-<h1>Native</h1>-after", "before-Native-after", true, true, 4])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const fragment = document.createDocumentFragment(); fragment.innerHTML = '<section data-source=\"http\"><em>Parsed &amp; ready</em> tail</section><hr>'; const section = fragment.firstElementChild; const em = fragment.querySelector('em'); const before = [fragment.innerHTML, fragment.childNodes.length, section.getAttribute('data-source'), em.textContent, section.parentNode === fragment]; const mount = document.getElementById('mount'); mount.appendChild(fragment); return [before, fragment.innerHTML, mount.querySelector('em') === em, mount.innerHTML]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            [
                "<section data-source=\"http\"><em>Parsed &amp; ready</em> tail</section><hr>",
                2,
                "http",
                "Parsed & ready",
                true,
            ],
            "",
            true,
            "<aside></aside>before-<h1>Native</h1>-after<section data-source=\"http\"><em>Parsed &amp; ready</em> tail</section><hr>",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const fragment = document.createDocumentFragment(); fragment.innerHTML = '<!doctype html><!--ignored--><div title=\"a > b\">before<script>if (a < b) x = \"</div>\";</script><textarea>&lt;em&gt;safe&lt;/em&gt;</textarea></div>'; const div = fragment.firstElementChild; const script = div.querySelector('script'); const textarea = div.querySelector('textarea'); return [fragment.childNodes.length, div.getAttribute('title'), script.textContent, textarea.textContent, div.querySelector('em') === null]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([2, "a > b", "if (a < b) x = \"</div>\";", "<em>safe</em>", true])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const source = document.createElement('section'); source.setAttribute('id', 'moving-source'); const moved = document.createElement('b'); source.appendChild(moved); document.body.appendChild(source); const fragment = document.createDocumentFragment(); fragment.appendChild(moved); const stable = document.createElement('i'); fragment.appendChild(stable); const records = []; const observer = new MutationObserver(entries => records.push(entries.map(record => ({ type: record.type, target: record.target === fragment, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName), previous: record.previousSibling && (record.previousSibling.localName || record.previousSibling.nodeName), next: record.nextSibling && (record.nextSibling.localName || record.nextSibling.nodeName) })))); observer.observe(fragment, { childList: true, subtree: true }); const inserted = document.createElement('em'); fragment.insertBefore(inserted, stable); stable.remove(); globalThis.fragmentMutationLog = records; return [moved.parentNode === fragment, source.childNodes.length, fragment.childNodes.length]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 0, 2])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.fragmentMutationLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "type": "childList",
                "target": true,
                "added": ["em"],
                "removed": [],
                "previous": "b",
                "next": "i",
            },
            {
                "type": "childList",
                "target": true,
                "added": [],
                "removed": ["i"],
                "previous": "em",
                "next": null,
            },
        ]])
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('moving-source').childNodes.length")
            .await
            .unwrap(),
        serde_json::json!(0)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const reflected = document.createElement('label'); reflected.id = 'http-property-id'; reflected.className = 'http alpha'; document.body.appendChild(reflected); const assigned = [reflected.id, reflected.className, reflected.getAttribute('id'), reflected.getAttribute('class'), document.getElementById('http-property-id') === reflected, document.querySelector('.alpha') === reflected]; reflected.setAttribute('id', 'http-attribute-id'); reflected.removeAttribute('class'); reflected.title = 'HTTP title'; reflected.name = 'http-field'; reflected.required = true; reflected.readOnly = true; reflected.type = 'BUTTON'; const form = [reflected.title, reflected.name, reflected.required, reflected.readOnly, reflected.type, reflected.getAttribute('title'), reflected.getAttribute('name'), reflected.getAttribute('required'), reflected.getAttribute('readonly'), reflected.getAttribute('type')]; reflected.required = false; reflected.readOnly = false; return { assigned, method: [reflected.id, reflected.className, reflected.getAttribute('id'), reflected.getAttribute('class'), reflected.hasAttribute('class'), document.getElementById('http-attribute-id') === reflected], form, cleared: [reflected.required, reflected.readOnly, reflected.hasAttribute('required'), reflected.hasAttribute('readonly')] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "assigned": ["http-property-id", "http alpha", "http-property-id", "http alpha", true, true],
            "method": ["http-attribute-id", "", "http-attribute-id", null, false, true],
            "form": ["HTTP title", "http-field", true, true, "button", "HTTP title", "http-field", "", "", "BUTTON"],
            "cleared": [false, false, false, false],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const reflected = document.getElementById('http-attribute-id'); return [reflected.id, reflected.className, reflected.getAttribute('id'), reflected.getAttribute('class'), document.querySelector('#http-attribute-id') === reflected, reflected.title, reflected.name, reflected.required, reflected.readOnly, reflected.type]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["http-attribute-id", "", "http-attribute-id", null, true, "HTTP title", "http-field", false, false, "button"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const target = document.createElement('article'); document.body.appendChild(target); const log = []; const observer = new MutationObserver(records => log.push(records.map(record => ({ type: record.type, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName) })))); observer.observe(target, { childList: true }); target.innerHTML = '<div id=inner><span>Hi &amp; there</span></div>tail'; globalThis.httpElementInnerHtmlMutationLog = log; return [target.innerHTML, target.textContent, target.querySelector('#inner') !== null, target.querySelector('span').textContent, document.getElementById('inner') !== null, target.childNodes.length]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["<div id=\"inner\"><span>Hi &amp; there</span></div>tail", "Hi & theretail", true, "Hi & there", true, 2])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.httpElementInnerHtmlMutationLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "type": "childList",
                "added": ["div", "#text"],
                "removed": [],
            }
        ]])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const target = document.createElement('p'); document.body.appendChild(target); const log = []; const observer = new MutationObserver(records => log.push(records.map(record => ({ type: record.type, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName) })))); observer.observe(target, { childList: true }); target.textContent = 'http text'; globalThis.httpElementTextContentMutationLog = log; return [target.textContent, target.firstChild.nodeType, target.firstChild.nodeValue, target.childNodes.length, target.firstChild.parentNode === target]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["http text", 3, "http text", 1, true])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.httpElementTextContentMutationLog")
            .await
            .unwrap(),
        serde_json::json!([[
            {
                "type": "childList",
                "added": ["#text"],
                "removed": [],
            }
        ]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_page_load_location_handoffs_preserve_history() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, body) in [
            ("/origin", "<title>Origin</title><p>Origin</p>"),
            (
                "/start",
                "<script>addEventListener('load', () => location.replace('/middle'))</script><title>Start</title><p>Transient</p>",
            ),
            (
                "/middle",
                "<script>addEventListener('load', () => location.assign('/final'))</script><title>Middle</title><p>Intermediate</p>",
            ),
            ("/final", "<title>Final</title><p>Published</p>"),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/origin")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async(format!("http://{address}/start"))
        .await
        .unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/final")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Final");
    assert_eq!(engine.snapshot().unwrap().visible_text, "Published");
    assert_eq!(engine.history().len(), 2);
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_runs_inline_page_scripts_in_persistent_realm() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<script>globalThis.inlineCount = (globalThis.inlineCount || 0) + 1; globalThis.lifecycle = []; document.addEventListener('readystatechange', () => lifecycle.push('ready:' + document.readyState)); document.addEventListener('DOMContentLoaded', () => lifecycle.push('dom:' + document.readyState)); addEventListener('load', () => lifecycle.push('load:' + document.readyState));</script><script>globalThis.inlineCount += 1; document.getElementById('name').value = 'loaded';</script><input id='name' type='text'>";
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
            .evaluate_async("globalThis.inlineCount")
            .await
            .unwrap(),
        serde_json::json!(2)
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('name').value")
            .await
            .unwrap(),
        serde_json::json!("loaded")
    );
    assert_eq!(
        engine.evaluate_async("globalThis.lifecycle").await.unwrap(),
        serde_json::json!([
            "ready:interactive",
            "dom:interactive",
            "ready:complete",
            "load:complete"
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_routes_body_and_frameset_handlers_through_window_slot() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial_source = "globalThis.windowHandlerAttrOrder.push('body-initial');";
    let dynamic_source = "globalThis.windowHandlerAttrOrder.push('body-dynamic');";
    let frameset_source = "globalThis.windowHandlerAttrOrder.push('frameset');";
    let click_source = "globalThis.windowHandlerAttrClicks.push([this === document.body, event.currentTarget === document.body]);";
    let blocked_source = "globalThis.windowHandlerAttrOrder.push('blocked');";
    let hash = |source: &str| {
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(source.as_bytes()))
    };
    let allowed_hashes = [
        initial_source,
        dynamic_source,
        frameset_source,
        click_source,
    ]
    .into_iter()
    .map(|source| format!("'sha256-{}'", hash(source)))
    .collect::<Vec<_>>()
    .join(" ");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/window-handler-attrs")
        );
        let body =
            format!("<!doctype html><html><body onmessage=\"{initial_source}\"></body></html>");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; script-src-attr 'unsafe-hashes' {allowed_hashes}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/window-handler-attrs")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let dynamic_json = serde_json::to_string(dynamic_source).unwrap();
    let blocked_json = serde_json::to_string(blocked_source).unwrap();
    let frameset_json = serde_json::to_string(frameset_source).unwrap();
    let click_json = serde_json::to_string(click_source).unwrap();
    let setup_script = format!(
        r#"(() => {{
          const order = globalThis.windowHandlerAttrOrder = [];
          const body = document.body;
          const initialAlias = typeof body.onmessage === 'function' && body.onmessage === window.onmessage;
          const tail = () => order.push('tail');
          globalThis.windowHandlerAttrTail = tail;
          window.addEventListener('message', tail);
          window.onmessage = () => order.push('idl');
          window.dispatchEvent(new Event('message'));
          const idlOrder = order.slice();
          body.setAttribute('onmessage', {dynamic_json});
          const dynamicAlias = typeof body.onmessage === 'function' && body.onmessage === window.onmessage;
          return [initialAlias, idlOrder, dynamicAlias];
        }})()"#
    );
    assert_eq!(
        engine.evaluate_async(&setup_script).await.unwrap(),
        serde_json::json!([true, ["idl", "tail"], true])
    );
    let script = format!(
        r#"(() => {{
          const order = globalThis.windowHandlerAttrOrder;
          const body = document.body;
          window.dispatchEvent(new Event('message'));
          const afterRefreshOrder = order.slice();
          const dynamicHandler = body.onmessage;
          body.setAttribute('onmessage', {blocked_json});
          const blockedPreserved = body.onmessage === dynamicHandler && window.onmessage === dynamicHandler;
          window.dispatchEvent(new Event('message'));
          const blockedOrder = order.slice();
          body.removeAttribute('onmessage');
          const bodyCleared = body.onmessage === null && window.onmessage === null;
          window.dispatchEvent(new Event('message'));
          const afterRemovalOrder = order.slice();
          const frameset = document.createElement('frameset');
          frameset.setAttribute('onmessage', {frameset_json});
          const framesetAlias = frameset.onmessage === window.onmessage;
          window.dispatchEvent(new Event('message'));
          const framesetOrder = order.slice();
          frameset.removeAttribute('onmessage');
          const framesetCleared = frameset.onmessage === null && window.onmessage === null;
          const clicks = globalThis.windowHandlerAttrClicks = [];
          body.setAttribute('onclick', {click_json});
          body.dispatchEvent(new Event('click'));
          body.removeAttribute('onclick');
          window.removeEventListener('message', globalThis.windowHandlerAttrTail);
          return [afterRefreshOrder, blockedPreserved, blockedOrder, bodyCleared, afterRemovalOrder,
            framesetAlias, framesetOrder, framesetCleared, clicks];
        }})()"#
    );
    // A CSP-blocked replacement content attribute clears the prior handler;
    // later dispatches still reach independent listeners on the shared window slot.
    assert_eq!(
        engine.evaluate_async(&script).await.unwrap(),
        serde_json::json!([
            ["idl", "tail", "body-dynamic", "tail"],
            false,
            ["idl", "tail", "body-dynamic", "tail", "tail"],
            true,
            ["idl", "tail", "body-dynamic", "tail", "tail", "tail"],
            true,
            [
                "idl",
                "tail",
                "body-dynamic",
                "tail",
                "tail",
                "tail",
                "tail",
                "frameset"
            ],
            true,
            [[true, true]],
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_orders_navigation_lifecycle_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/old", "/next"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/old" {
                "<script>addEventListener('pagehide', () => globalThis.oldPagehide = true); addEventListener('unload', () => globalThis.oldUnload = true);</script><p>Old</p>"
            } else {
                "<script>globalThis.lifecycle = []; addEventListener('pageshow', event => lifecycle.push([document.readyState, event.persisted]));</script><p>Next</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/old")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let before_navigation = engine.revision();
    engine
        .navigate_async(format!("http://{address}/next"))
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.lifecycle").await.unwrap(),
        serde_json::json!([["complete", false]])
    );
    let lifecycle_effects = engine
        .effects_since(before_navigation)
        .unwrap()
        .effects
        .into_iter()
        .map(|effect| effect.kind)
        .filter(|kind| {
            matches!(
                kind,
                NativeEventKind::PageHide | NativeEventKind::Unload | NativeEventKind::PageShow
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle_effects,
        vec![
            NativeEventKind::PageHide,
            NativeEventKind::Unload,
            NativeEventKind::PageShow
        ]
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_beforeunload_without_activation_does_not_block_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (path, body) in [
            (
                "/start",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'page-authored warning'; });</script><button id='activate'>Activate</button><title>Start</title>",
            ),
            ("/next", "<title>Next</title>"),
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

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/start")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('activate').click()")
        .await
        .unwrap();
    engine
        .navigate_async(format!("http://{address}/next"))
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/next")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Next");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_outgoing_lifecycle_can_replace_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, body) in [
            (
                "/start",
                "<script>addEventListener('beforeunload', () => location.replace('/final')); addEventListener('pagehide', () => globalThis.pagehideSeen = true); addEventListener('unload', () => globalThis.unloadSeen = true);</script><title>Start</title>",
            ),
            ("/final", "<title>Final</title><p>Published</p>"),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/start")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async(format!("http://{address}/requested"))
        .await
        .unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/final")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Final");
    assert_eq!(engine.history().len(), 1);
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_fragment_navigation_dispatches_hashchange_in_place() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<script>globalThis.hashEvents = []; addEventListener('hashchange', (event) => hashEvents.push([event.oldURL.length, event.newURL.length, event.oldURL.endsWith('a'), event.newURL.endsWith('b')]));</script><p id='target'>Hash target</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let initial_fragment = "a".repeat(8200);
    let next_fragment = format!("{}b", "a".repeat(8199));
    let initial_url = format!("http://{address}/page#{initial_fragment}");
    let next_url = format!("http://{address}/page#{next_fragment}");
    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    let before_navigation = engine.revision();
    engine.navigate_async(next_url.clone()).await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.hashEvents")
            .await
            .unwrap(),
        serde_json::json!([[initial_url.len(), next_url.len(), true, true]])
    );
    assert_eq!(engine.snapshot().unwrap().url, next_url);
    let effects = engine.effects_since(before_navigation).unwrap().effects;
    assert!(
        effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::HashChange)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_hashchange_can_reenter_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected_path, body) in [
            (
                "/page",
                "<script>addEventListener('hashchange', () => { if (location.hash === '#two') location.assign('/final'); });</script><title>Start</title><p>Start</p>",
            ),
            ("/final", "<title>Final</title><p>Published</p>"),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page#one")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async(format!("http://{address}/page#two"))
        .await
        .unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/final")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Final");
    assert_eq!(engine.history().len(), 3);
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_classic_external_scripts_in_document_order() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/app.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "<script type='text/x-test' src='/ignored-too.js'></script><script src='/app.js'></script><script>globalThis.order += '-inline';</script>",
                )
            } else {
                ("application/javascript", "globalThis.order = 'external';")
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
    assert_eq!(
        engine.evaluate_async("globalThis.order").await.unwrap(),
        serde_json::json!("external-inline")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_keeps_document_alive_when_optional_resources_fail() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/missing.css", "/missing.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/page" {
                let body = "<link id='missing-style' rel='stylesheet' href='/missing.css'><script id='missing-script' src='/missing.js'></script><title>Still loaded</title><p id='content'>The document survives optional resource errors.</p>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            } else {
                stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, format!("http://{address}/page"));
    assert_eq!(snapshot.title, "Still loaded");
    assert_eq!(
        snapshot.visible_text,
        "The document survives optional resource errors."
    );
    assert_eq!(
        engine
            .effects_since(0)
            .unwrap()
            .effects
            .iter()
            .filter(|effect| effect.kind == NativeEventKind::Error)
            .count(),
        2
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_external_modules_in_document_order() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/module.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "<script type='module' src='/module.js'></script><script type='module'>globalThis.moduleOrder += '-inline';</script>",
                )
            } else {
                (
                    "application/javascript",
                    "globalThis.moduleOrder = 'external'; export const ready = true;",
                )
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
    assert_eq!(
        engine
            .evaluate_async("globalThis.moduleOrder")
            .await
            .unwrap(),
        serde_json::json!("external-inline")
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_reports_external_module_evaluation_failure_without_aborting_document()
 {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/app.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "<script id='module' type='module' src='/app.js'></script><script>globalThis.moduleEvents = []; const moduleScript = document.getElementById('module'); moduleScript.addEventListener('load', () => moduleEvents.push('load')); moduleScript.addEventListener('error', () => moduleEvents.push('error'));</script><title>Module error is isolated</title><p>Document committed</p>",
                )
            } else {
                ("application/javascript", "throw new Error('module boom');")
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
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, format!("http://{address}/page"));
    assert_eq!(snapshot.title, "Module error is isolated");
    assert_eq!(snapshot.visible_text, "Document committed");
    assert_eq!(
        engine
            .evaluate_async("globalThis.moduleEvents")
            .await
            .unwrap(),
        serde_json::json!(["error"])
    );
    assert_eq!(
        engine
            .evaluate_async("Boolean(globalThis.moduleRan)")
            .await
            .unwrap(),
        serde_json::json!(false)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_navigate_to_blocks_link_and_location_navigation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_navigate_to_blocks_link_and_location_navigation_inner(),
        );
    });
}

async fn native_content_process_navigate_to_blocks_link_and_location_navigation_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial_url = format!("http://{address}/start");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/start"));
        let body = "<a id='go' href='/result'>Go</a>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src *; navigate-to 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_millis(300), listener.accept())
            .await
            .expect_err("navigate-to blocked navigation must not issue a second request");
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=go".into(),
        })
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    engine
        .evaluate_async("location.href = '/result'")
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, initial_url);
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_propagates_dynamic_navigate_to_to_parent_navigation_owner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial_url = format!("http://{address}/start");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/start"));
        let body = "<head></head><body><a id='go' href='/result'>Go</a></body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_millis(300), listener.accept())
            .await
            .expect_err("dynamic navigate-to blocked navigation must not issue a request");
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const meta = document.createElement('meta'); meta.setAttribute('http-equiv', 'Content-Security-Policy'); meta.setAttribute('content', \"default-src *; navigate-to 'none'\"); document.head.appendChild(meta); return true; })()",
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
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_reports_navigate_to_preflight_before_blocking_navigation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_reports_navigate_to_preflight_before_blocking_navigation_inner(),
        );
    });
}

async fn native_content_process_reports_navigate_to_preflight_before_blocking_navigation_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial_url = format!("http://{address}/start");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/start"));
        let body = r#"<a id='go' href='/blocked'>Blocked</a>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src *; navigate-to 'none'\r\nContent-Security-Policy-Report-Only: default-src *; navigate-to 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_millis(300), listener.accept())
            .await
            .expect_err("blocked navigate-to preflight must not issue a second request");
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(initial_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.navigationReports = []; addEventListener('securitypolicyviolation', event => navigationReports.push([event.effectiveDirective, event.blockedURI, event.disposition]))",
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
    assert_eq!(
        engine.evaluate_async("navigationReports").await.unwrap(),
        serde_json::json!([["navigate-to", format!("http://{address}/blocked"), "report"]])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_blocks_csp_disallowed_frame_before_request() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_blocks_csp_disallowed_frame_before_request_inner());
    });
}

async fn native_content_process_blocks_csp_disallowed_frame_before_request_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body =
            "<title>Parent</title><iframe src='/blocked'>fallback</iframe><p>Parent page</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: frame-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child = frames
        .iter()
        .find(|frame| frame.id.ends_with(":frame-1"))
        .unwrap();
    assert_eq!(child.url, "about:blank");
    session.native_select_frame(&child.id).await.unwrap();
    session
        .navigate(format!("http://{address}/blocked"))
        .await
        .unwrap();
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        "about:blank"
    );
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_blocks_frame_from_head_meta_csp() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_blocks_frame_from_head_meta_csp_inner());
    });
}

async fn native_content_process_blocks_frame_from_head_meta_csp_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<head><meta http-equiv='Content-Security-Policy' content=\"frame-src 'none'\"></head><body><iframe src='/child'>fallback</iframe><p>Parent page</p></body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: frame-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child = frames
        .iter()
        .find(|frame| frame.id.ends_with(":frame-1"))
        .unwrap();
    assert_eq!(child.url, "about:blank");
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_content_process_allows_csp_same_origin_frame() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_content_process_allows_csp_same_origin_frame_inner());
    });
}

async fn native_content_process_allows_csp_same_origin_frame_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/child"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (extra_headers, body) = if expected_path == "/page" {
                (
                    "Content-Security-Policy: frame-src 'self'\r\n",
                    "<title>Parent</title><iframe src='/child'>fallback</iframe>",
                )
            } else {
                ("", "<title>Child</title><p>Child page</p>")
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child = frames
        .iter()
        .find(|frame| frame.id.ends_with(":frame-1"))
        .unwrap();
    assert_eq!(child.url, format!("http://{address}/child"));
    session.native_select_frame(&child.id).await.unwrap();
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "Child"
    );
    session.close().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_reuses_bounded_http_cache_for_fragment_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert_eq!(request.split_whitespace().nth(1), Some("/cached"));
        let body = "<title>Cached</title><p>One network response</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let base_url = format!("http://{address}");
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{base_url}/cached")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async(format!("{base_url}/cached#section"))
        .await
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, format!("{base_url}/cached#section"));
    assert_eq!(snapshot.title, "Cached");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_enforces_child_owned_document_limit() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "x".repeat(128);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let limits = NativeEngineLimits {
        max_document_bytes: 32,
        ..NativeEngineLimits::default()
    };
    let error = match BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_limits(limits)
            .with_initial_url(format!("http://{address}/oversized")),
    )
    .await
    {
        Ok(_) => panic!("oversized content unexpectedly initialized"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("HTTP document exceeds limit 32"), "{error}");
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_external_png_through_document_wire() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/image.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/image.png" {
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("referer")
                            && value.trim() == format!("http://{address}/page")
                    })
                }));
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&png).await.unwrap();
            } else {
                let body = "<style>body{margin:0px}</style><title>External image</title><div style='width:16px'><img id='image' src='/image.png' width='8'></div><script>globalThis.imageLoaded = 0; document.getElementById('image').addEventListener('load', () => { globalThis.imageLoaded = 1; });</script>".to_string();
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
                width: 16,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let list = engine.display_list().unwrap();
    let (image_id, image_rect) = list
        .commands
        .iter()
        .find_map(|command| match command {
            NativeDisplayCommand::Image { node_id, rect, .. } => Some((*node_id, *rect)),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        engine.layout().unwrap().box_for(image_id),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        })
    );
    assert_eq!(image_rect.width, 8);
    assert_eq!(
        list.rasterize().unwrap().pixel(1, 1),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.imageLoaded")
            .await
            .unwrap(),
        serde_json::json!(1)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, 2, format!("http://{address}/image.png")])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_file_content_process_settles_page_dynamic_module_graphs() {
    let _guard = native_content_process_test_lock().lock().await;
    let root = std::env::temp_dir().join(format!(
        "glass-native-page-dynamic-module-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let page_path = root.join("index.html");
    let entry_path = root.join("entry.js");
    let dynamic_path = root.join("dynamic.js");
    let static_dependency_path = root.join("static-dep.js");
    let nested_path = root.join("nested.js");
    let module_path = root.join("inline-dynamic.js");
    fs::write(
        &entry_path,
        "import('./dynamic.js').then(() => { globalThis.dynamicEntrySettled = true; });",
    )
    .unwrap();
    fs::write(
        &dynamic_path,
        "import './static-dep.js'; import('./nested.js').then(() => { globalThis.nestedImportSettled = true; }); globalThis.dynamicModuleEvaluated = true;",
    )
    .unwrap();
    fs::write(
        &static_dependency_path,
        "globalThis.staticModuleDependencyLoaded = true;",
    )
    .unwrap();
    fs::write(&nested_path, "globalThis.nestedDynamicModuleLoaded = true;").unwrap();
    fs::write(&module_path, "globalThis.inlineDynamicModuleLoaded = true;").unwrap();
    fs::write(
        &page_path,
        "<html><body><script type='module' src='entry.js' referrerpolicy='unsafe-url'></script><script type='module' referrerpolicy='origin'>globalThis.inlineDynamicImportError = null; import('./inline-dynamic.js').then(() => { globalThis.inlineDynamicImportSettled = true; }, error => { globalThis.inlineDynamicImportError = String(error); globalThis.inlineDynamicImportSettled = true; });</script></body></html>",
    )
    .unwrap();
    let config = NativeEngineConfig::default()
        .with_initial_url(native_test_file_url(&page_path))
        .with_allowed_file_root(root.clone());
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let result = engine
        .evaluate_async(
            "[globalThis.dynamicEntrySettled, globalThis.dynamicModuleEvaluated, globalThis.staticModuleDependencyLoaded, globalThis.nestedDynamicModuleLoaded, globalThis.nestedImportSettled, globalThis.inlineDynamicModuleLoaded, globalThis.inlineDynamicImportSettled, globalThis.inlineDynamicImportError, globalThis.__glassFetchRequests.size]",
        )
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    fs::remove_dir_all(root).unwrap();

    assert_eq!(
        result,
        serde_json::json!([true, true, true, true, true, true, true, null, 0])
    );
}
#[tokio::test]
async fn native_content_process_loads_external_background_png_through_document_wire() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/image.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/image.png" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&png).await.unwrap();
            } else {
                let body = "<style>body{margin:0px}</style><div id='surface' style=\"width:8px;height:8px;background:url('/image.png') no-repeat 0 0 / 100% 100%\"></div>";
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
                width: 8,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let list = engine.display_list().unwrap();
    assert_eq!(
        list.commands
            .iter()
            .filter(|command| matches!(command, NativeDisplayCommand::Image { .. }))
            .count(),
        1
    );
    assert_eq!(
        list.rasterize().unwrap().pixel(1, 1),
        Some([255, 0, 0, 255])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_exposes_page_web_storage_realm() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<p>Storage owner</p>";
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
                "localStorage.setItem('theme', 'dark'); sessionStorage.setItem('tab', 'one'); ({ local: localStorage.getItem('theme'), session: sessionStorage.getItem('tab') })",
            )
            .await
            .unwrap(),
        serde_json::json!({"local":"dark","session":"one"})
    );
    assert_eq!(
        engine
            .evaluate_async("[localStorage.length, sessionStorage.length, localStorage.key(0), sessionStorage.key(0)]")
            .await
            .unwrap(),
        serde_json::json!([1, 1, "theme", "tab"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_delivers_local_storage_events_between_documents() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-content-events.json",
        std::process::id()
    ));
    let observer_path = profile_path
        .parent()
        .unwrap()
        .join(".")
        .join(profile_path.file_name().unwrap());
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&lock_path);
    let _ = fs::remove_file(&events_path);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/page"));
            let body = "<p>Content storage owner</p>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let url = format!("http://{address}/page");
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_initial_url(url.clone());
    let mut owner = NativeEngine::new(config.clone()).unwrap();
    let mut observer = NativeEngine::new(config.with_storage_path(observer_path)).unwrap();
    owner.initialize_async().await.unwrap();
    observer.initialize_async().await.unwrap();

    for engine in [&mut owner, &mut observer] {
        engine
            .evaluate_async(
                "globalThis.storageEvents = []; addEventListener('storage', event => storageEvents.push({ key: event.key, oldValue: event.oldValue, newValue: event.newValue, url: event.url, local: event.storageArea === localStorage })); true",
            )
            .await
            .unwrap();
    }

    owner
        .evaluate_async("localStorage.setItem('theme', 'dark'); true")
        .await
        .unwrap();
    assert_eq!(
        observer
            .evaluate_async(
                "({ events: storageEvents.splice(0), value: localStorage.getItem('theme') })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": [{
                "key": "theme",
                "oldValue": null,
                "newValue": "dark",
                "url": url,
                "local": true,
            }],
            "value": "dark",
        })
    );
    assert_eq!(
        owner.evaluate_async("storageEvents").await.unwrap(),
        serde_json::json!([])
    );

    observer.close_async().await.unwrap();
    owner.close_async().await.unwrap();
    server.await.unwrap();
    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(lock_path);
    let _ = fs::remove_file(events_path);
}
#[tokio::test]
async fn native_content_process_preserves_origin_keyed_web_storage_across_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/next"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/page" {
                "<p>First page</p>"
            } else {
                "<p>Second page</p>"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                "localStorage.setItem('theme', 'dark'); sessionStorage.setItem('tab', 'one'); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine
        .navigate_async(format!("http://{address}/next"))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ local: localStorage.getItem('theme'), session: sessionStorage.getItem('tab'), localLength: localStorage.length, sessionLength: sessionStorage.length })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "local": "dark",
            "session": "one",
            "localLength": 1,
            "sessionLength": 1,
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_commits_page_work_from_stream_upload_pull() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for expected_path in ["/page", "/upload", "/after-upload"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            if expected_path == "/upload" {
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert_eq!(body, "pull-body");
                assert!(
                    cookie
                        .as_deref()
                        .unwrap_or_default()
                        .contains("stream_seed=initial")
                );
            } else if expected_path == "/after-upload" {
                assert!(
                    cookie
                        .as_deref()
                        .unwrap_or_default()
                        .contains("stream_parent=accepted"),
                    "same-turn follow-up did not see the parent Set-Cookie update: {cookie:?}"
                );
            }
            let (content_type, set_cookie, body) = match expected_path {
                "/page" => (
                    "text/html",
                    "Set-Cookie: stream_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "<title>Initial</title><body>Upload pull</body>",
                ),
                "/upload" => (
                    "text/plain",
                    "Set-Cookie: stream_parent=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "ok",
                ),
                "/after-upload" => ("text/plain", "", "after"),
                _ => unreachable!(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((expected_path, cookie));
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const stream = new ReadableStream({
                        pull(controller) {
                            document.title = 'Pulled';
                            controller.enqueue('pull-');
                            controller.enqueue('body');
                            controller.close();
                        },
                    });
                    const response = await fetch('/upload', { method: 'POST', body: stream });
                    const responseText = await response.text();
                    const followup = await fetch('/after-upload');
                    return [responseText, await followup.text(), document.title];
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(["ok", "after", "Pulled"])
    );
    assert!(
        engine
            .cookies_async()
            .await
            .unwrap()
            .iter()
            .any(|cookie| cookie.name == "stream_parent" && cookie.value == "accepted")
    );
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();
    assert!(
        requests[1]
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("stream_seed=initial")
    );
    assert!(
        requests[2]
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("stream_parent=accepted")
    );
}
#[test]
fn native_content_process_radio_group_arrow_navigation_covers_same_origin_frames() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_content_process_radio_group_arrow_navigation_covers_same_origin_frames_inner(),
        );
    });
}

async fn native_content_process_radio_group_arrow_navigation_covers_same_origin_frames_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let parent_body = "<!doctype html><body><form><input id='parent-a' type='radio' name='parent' checked><input id='parent-b' type='radio' name='parent'></form><iframe id='child' src='/child'></iframe></body>";
    let child_body = "<!doctype html><body><form><input id='child-a' type='radio' name='child' checked><input id='child-b' type='radio' name='child'></form></body>";
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("radio navigation document request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/page") => parent_body,
                Some("/child") => child_body,
                other => panic!("unexpected radio navigation path: {other:?}"),
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
    let setup_trace = "globalThis.__radioArrowTrace = []; document.addEventListener('keydown', event => __radioArrowTrace.push('keydown:' + event.target.id + ':' + event.key)); document.addEventListener('keyup', event => __radioArrowTrace.push('keyup:' + event.target.id + ':' + event.key)); for (const type of ['input', 'change']) document.addEventListener(type, event => __radioArrowTrace.push(type + ':' + event.target.id + ':' + event.bubbles)); true";
    session.script(setup_trace).await.unwrap();
    session
        .script("document.getElementById('parent-a').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "ArrowRight".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({active: document.activeElement.id, a: document.getElementById('parent-a').checked, b: document.getElementById('parent-b').checked, trace: __radioArrowTrace})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "active": "parent-b",
            "a": false,
            "b": true,
            "trace": [
                "keydown:parent-a:ArrowRight",
                "input:parent-b:true",
                "change:parent-b:true",
                "keyup:parent-b:ArrowRight",
            ],
        })
    );

    let child_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin radio group child frame")
        .id;
    session.select_frame(&child_id).await.unwrap();
    session.script(setup_trace).await.unwrap();
    session
        .script("document.getElementById('child-a').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "ArrowDown".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("({active: document.activeElement.id, a: document.getElementById('child-a').checked, b: document.getElementById('child-b').checked, trace: __radioArrowTrace})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "active": "child-b",
            "a": false,
            "b": true,
            "trace": [
                "keydown:child-a:ArrowDown",
                "input:child-b:true",
                "change:child-b:true",
                "keyup:child-b:ArrowDown",
            ],
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}

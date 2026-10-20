use super::*;

#[tokio::test]
async fn synchronous_process_history_beforeunload_requires_async_api() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (unexpected_request_tx, mut unexpected_request_rx) = oneshot::channel();
    let (server_stop_tx, mut server_stop_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut unexpected_request_tx = Some(unexpected_request_tx);
        for (expected_path, body) in [
            (
                "/first",
                "<title>First</title><button id='activate'>Activate</button>",
            ),
            (
                "/second",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'not shown'; globalThis.beforeunloadMutation = 'seen'; history.back(); }); addEventListener('pagehide', () => globalThis.pagehideRan = true); addEventListener('unload', () => globalThis.unloadRan = true);</script><title>Second</title><button id='activate'>Activate</button>",
            ),
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

        tokio::select! {
            accepted = listener.accept() => {
                let (mut stream, _) = accepted.unwrap();
                let request = read_http_request(&mut stream).await;
                if let Some(sender) = unexpected_request_tx.take() {
                    let path = request.split_whitespace().nth(1).unwrap_or_default().to_owned();
                    let _ = sender.send(path);
                }
                let body = "<title>Unexpected traversal</title>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            _ = &mut server_stop_rx => {}
        }
    });

    let first_url = format!("http://{address}/first");
    let second_url = format!("http://{address}/second");
    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(first_url)).unwrap();
    engine.initialize_async().await.unwrap();
    engine.navigate_async(second_url.clone()).await.unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=activate".into(),
        })
        .await
        .unwrap();
    assert!(engine.history().can_go_back());
    assert!(!engine.history().can_go_forward());
    let revision_before_sync_attempt = engine.revision();

    let synchronous_error = engine.go_back().unwrap_err();

    assert!(
        synchronous_error
            .to_string()
            .contains("requires the asynchronous native history API")
    );
    assert_eq!(engine.snapshot().unwrap().url, second_url);
    assert!(engine.history().can_go_back());
    assert!(!engine.history().can_go_forward());
    assert_eq!(engine.revision(), revision_before_sync_attempt);
    assert_eq!(
        engine
            .evaluate_async("globalThis.beforeunloadMutation || null")
            .await
            .unwrap(),
        serde_json::Value::Null
    );

    let reentry_error = engine.go_back_async().await.unwrap_err();

    assert!(
        reentry_error
            .to_string()
            .contains("cannot re-enter an outgoing lifecycle callback")
    );
    assert_eq!(engine.snapshot().unwrap().url, second_url);
    assert!(engine.history().can_go_back());
    assert!(!engine.history().can_go_forward());
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.beforeunloadMutation || null, globalThis.pagehideRan || null, globalThis.unloadRan || null]",
            )
            .await
            .unwrap(),
        serde_json::json!(["seen", null, null])
    );

    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut unexpected_request_rx)
            .await
            .is_err()
    );

    engine.close_async().await.unwrap();
    let _ = server_stop_tx.send(());
    server.await.unwrap();
}
#[tokio::test]
async fn native_async_history_traversal_uses_the_runtime_owner() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://history-async", "<p>First</p>")
        .unwrap()
        .with_fixture("fixture://history-async-next", "<p>Second</p>")
        .unwrap()
        .with_initial_url("fixture://history-async");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let first = engine.snapshot().unwrap();
    let second = engine
        .navigate_async("fixture://history-async-next")
        .await
        .unwrap();
    assert_eq!(first.visible_text, "First");
    assert_eq!(second.visible_text, "Second");

    let restored = engine.go_back_async().await.unwrap().unwrap();
    assert_eq!(restored.url, "fixture://history-async");
    assert_eq!(restored.visible_text, "First");
    assert_eq!(
        engine.history().current().unwrap().url,
        "fixture://history-async"
    );

    let forward = engine.go_forward_async().await.unwrap().unwrap();
    assert_eq!(forward.url, "fixture://history-async-next");
    assert_eq!(forward.visible_text, "Second");
    assert!(engine.go_forward_async().await.unwrap().is_none());
}
#[test]
fn canonical_browser_session_exposes_history_controls() {
    std::thread::Builder::new()
        .name("canonical-browser-history-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("canonical history runtime should build");
            runtime.block_on(canonical_browser_session_exposes_history_controls_inner());
        })
        .expect("canonical history test thread should start")
        .join()
        .expect("canonical history test thread should complete");
}
#[test]
fn canonical_browser_session_exposes_guarded_and_unguarded_recovery() {
    std::thread::Builder::new()
        .name("canonical-browser-recovery-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("canonical recovery runtime should build");
            runtime
                .block_on(canonical_browser_session_exposes_guarded_and_unguarded_recovery_inner());
        })
        .expect("canonical recovery test thread should start")
        .join()
        .expect("canonical recovery test thread should complete");
}
#[tokio::test]
async fn native_visual_capture_honors_clip_scale_full_page_and_element_options() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 8,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://visual-options",
            native_html_without_body_margin("<div id='top' style='display:block;width:8px;height:4px;background-color:red'></div><div id='bottom' style='display:block;width:8px;height:4px;background-color:blue'></div>"),
        )
        .unwrap()
        .with_initial_url("fixture://visual-options");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let clipped = backend
        .capture_visual(&VisualCaptureOptions {
            clip: Some(VisualClip {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 2.0,
            }),
            scale: 2.0,
            ..VisualCaptureOptions::default()
        })
        .await
        .unwrap();
    assert_eq!((clipped.metadata.width, clipped.metadata.height), (8, 4));
    let clipped_bytes = base64::engine::general_purpose::STANDARD
        .decode(clipped.data.as_bytes())
        .unwrap();
    let decoder = png::Decoder::new(Cursor::new(clipped_bytes));
    let reader = decoder.read_info().unwrap();
    assert_eq!((reader.info().width, reader.info().height), (8, 4));

    let full_page = backend
        .capture_visual(&VisualCaptureOptions {
            full_page: true,
            ..VisualCaptureOptions::default()
        })
        .await
        .unwrap();
    assert!(full_page.metadata.full_page);
    // The parsed HTML document includes its implicit html/body boxes, so the
    // full-page capture follows the resulting 20px document content height.
    assert_eq!(
        (full_page.metadata.width, full_page.metadata.height),
        (8, 20)
    );

    let element = backend
        .capture_visual(&VisualCaptureOptions {
            target: Some("id=bottom".into()),
            ..VisualCaptureOptions::default()
        })
        .await
        .unwrap();
    assert_eq!((element.metadata.width, element.metadata.height), (8, 4));
    assert_eq!(element.metadata.clip.unwrap().y, 4.0);

    dispatcher.close().await.unwrap();
}
#[test]
fn fixture_navigation_projects_through_the_real_backend_dispatcher() {
    std::thread::Builder::new()
        .name("native-fixture-dispatcher-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("fixture dispatcher runtime should build");
            runtime
                .block_on(fixture_navigation_projects_through_the_real_backend_dispatcher_inner());
        })
        .expect("fixture dispatcher test thread should start")
        .join()
        .expect("fixture dispatcher test thread should complete");
}
#[test]
fn local_fragment_navigation_and_history_traversal_preserve_bounded_state() {
    std::thread::Builder::new()
        .name("native-fragment-history-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("fragment history runtime should build");
            runtime.block_on(
                local_fragment_navigation_and_history_traversal_preserve_bounded_state_inner(),
            );
        })
        .expect("fragment history test thread should start")
        .join()
        .expect("fragment history test thread should complete");
}
#[tokio::test]
async fn native_session_storage_routes_events_by_browsing_context() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-session-context-events.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&lock_path);
    let owner_config = NativeEngineConfig::default()
        .with_context_id("tab-one")
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://session-storage-events", "<p>Session storage</p>")
        .unwrap()
        .with_initial_url("fixture://session-storage-events");
    let same_context_config = owner_config.clone();
    let other_context_config = owner_config.clone().with_context_id("tab-two");

    let mut owner = NativeEngine::new(owner_config).unwrap();
    let mut same_context = NativeEngine::new(same_context_config).unwrap();
    let mut other_context = NativeEngine::new(other_context_config).unwrap();
    owner.initialize().unwrap();
    same_context.initialize().unwrap();
    other_context.initialize().unwrap();

    for engine in [&mut owner, &mut same_context, &mut other_context] {
        engine
            .evaluate_async(
                "globalThis.storageEvents = []; addEventListener('storage', event => storageEvents.push({ key: event.key, oldValue: event.oldValue, newValue: event.newValue, url: event.url, session: event.storageArea === sessionStorage })); true",
            )
            .await
            .unwrap();
    }

    owner
        .evaluate_async("sessionStorage.setItem('tab', 'one'); true")
        .await
        .unwrap();
    assert_eq!(
        same_context
            .evaluate_async(
                "({ events: storageEvents.splice(0), value: sessionStorage.getItem('tab') })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": [{
                "key": "tab",
                "oldValue": null,
                "newValue": "one",
                "url": "fixture://session-storage-events",
                "session": true,
            }],
            "value": "one",
        })
    );
    assert_eq!(
        other_context
            .evaluate_async(
                "({ events: storageEvents.splice(0), value: sessionStorage.getItem('tab') })",
            )
            .await
            .unwrap(),
        serde_json::json!({"events": [], "value": null})
    );
    assert_eq!(
        owner.evaluate_async("storageEvents").await.unwrap(),
        serde_json::json!([])
    );

    other_context.close().unwrap();
    same_context.close().unwrap();
    owner.close().unwrap();
    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(lock_path);
}

use super::*;

#[test]
fn native_engine_display_list_revision_tracks_accepted_actions() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://paint", "<button id='save'>Save</button>")
        .unwrap()
        .with_initial_url("fixture://paint");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial = engine.display_list().unwrap();
    assert_eq!(initial.revision, 1);
    engine
        .action(NativeAction::Click {
            target: "id=save".into(),
        })
        .unwrap();
    let after_action = engine.display_list().unwrap();
    assert_eq!(after_action.revision, 2);
    assert_ne!(initial, after_action);
}

#[test]
fn native_engine_raster_surface_is_bounded_and_does_not_mutate_revision() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 24,
            device_scale_factor_milli: 2000,
        })
        .with_fixture(
            "fixture://surface",
            "<html><body><style>#card { width: 8px; height: 8px; background-color: red; color: blue; }</style><div id='card'>A</div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://surface");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let before = engine.revision();
    let surface: NativeSurface = engine.rasterize().unwrap();
    assert_eq!(engine.revision(), before);
    assert_eq!(surface.width(), 32);
    assert_eq!(surface.height(), 24);
    assert_eq!(surface.rgba().len(), 32 * 24 * 4);
    assert_eq!(surface.pixel(15, 15), Some([255, 0, 0, 255]));
    assert!(
        surface
            .rgba()
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 255, 255])
    );
}

#[test]
fn native_backend_captures_all_visual_formats_without_mutating_revision() {
    std::thread::Builder::new()
        .name("native-visual-formats-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("visual formats runtime should build");
            runtime.block_on(
                native_backend_captures_all_visual_formats_without_mutating_revision_inner(),
            );
        })
        .expect("visual formats test thread should start")
        .join()
        .expect("visual formats test thread should complete");
}

#[test]
fn native_backend_composes_nested_frame_surfaces_into_capture() {
    std::thread::Builder::new()
        .name("native-nested-frame-capture-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("nested frame capture runtime should build");
            runtime.block_on(native_backend_composes_nested_frame_surfaces_into_capture_inner());
        })
        .expect("nested frame capture test thread should start")
        .join()
        .expect("nested frame capture test thread should complete");
}

#[test]
fn native_backend_routes_point_clicks_into_nested_frame_content() {
    std::thread::Builder::new()
        .name("native-nested-frame-click-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("nested frame click runtime should build");
            runtime.block_on(native_backend_routes_point_clicks_into_nested_frame_content_inner());
        })
        .expect("nested frame click test thread should start")
        .join()
        .expect("nested frame click test thread should complete");
}

#[test]
fn native_backend_dispatches_vertical_scroll_into_capture() {
    std::thread::Builder::new()
        .name("native-vertical-scroll-capture-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("vertical scroll capture runtime should build");
            runtime.block_on(native_backend_dispatches_vertical_scroll_into_capture_inner());
        })
        .expect("vertical scroll capture test thread should start")
        .join()
        .expect("vertical scroll capture test thread should complete");
}

#[test]
fn native_backend_dispatches_horizontal_scroll_into_capture() {
    std::thread::Builder::new()
        .name("native-horizontal-scroll-capture-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("horizontal scroll capture runtime should build");
            runtime.block_on(native_backend_dispatches_horizontal_scroll_into_capture_inner());
        })
        .expect("horizontal scroll capture test thread should start")
        .join()
        .expect("horizontal scroll capture test thread should complete");
}

#[tokio::test]
async fn native_backend_uses_configured_context_identity() {
    let backend = NativeEngineBackend::new(
        NativeEngineConfig::default()
            .with_context_id("tab-one")
            .with_initial_url("data:text/html,%3Ctitle%3ETab%3C%2Ftitle%3E"),
    )
    .unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let contexts = dispatcher
        .contexts(glass_browser::browser_backend::ContextRequest {
            include_background: false,
        })
        .await
        .unwrap();
    assert_eq!(contexts[0].context_id, "tab-one");
    let evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "tab-one".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(evidence.context_id, "tab-one");

    let wrong_context = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        wrong_context,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "context id"
    ));
    dispatcher.close().await.unwrap();
}

#[test]
fn native_backend_is_selected_without_explicit_factory_preference() {
    let automatic = BackendSelectionRequest {
        schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
        glass_version: env!("CARGO_PKG_VERSION").into(),
        preferred_backend_id: None,
        browser_family: None,
        browser_version: None,
        required_capabilities: vec![],
        minimum_certification: CertificationLevel::Partial,
    };
    let native = BackendFactory::native(NativeEngineConfig::default()).unwrap();
    let started = BackendFactory::start(&automatic, vec![native]).unwrap();
    assert_eq!(
        started.profile().identity.backend_id,
        NATIVE_ENGINE_BACKEND_ID
    );
}

#[tokio::test]
async fn native_engine_parent_fetches_cors_authorized_cross_origin_get() {
    let _guard = native_content_process_test_lock().lock().await;
    let document_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document_address = document_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let document_origin = format!("http://{document_address}");
    let expected_origin = document_origin.clone();
    let response_origin = document_origin.clone();

    let document_server = tokio::spawn(async move {
        let (mut stream, _) = document_listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<title>Fetch owner</title><p>Native fetch</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let api_server = tokio::spawn(async move {
        let (mut stream, _) = api_listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert_eq!(request.split_whitespace().nth(1), Some("/data"));
        assert!(
            request.lines().any(|line| {
                line.split_once(':').is_some_and(|(name, value)| {
                    name.eq_ignore_ascii_case("origin") && value.trim() == expected_origin
                })
            }),
            "cross-origin fetch omitted its Origin header: {request}"
        );
        let body = "{\"ok\":true}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Expose-Headers: X-Visible\r\nX-Visible: yes\r\nX-Hidden: no\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{document_origin}/index.html")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let response = engine
        .fetch_async(format!("http://{api_address}/data"), false)
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.content_type.as_deref(), Some("application/json"));
    let mut headers = response.headers.clone();
    headers.sort();
    assert_eq!(
        headers,
        vec![
            ("content-length".into(), "11".into()),
            ("content-type".into(), "application/json".into()),
            ("x-visible".into(), "yes".into()),
        ]
    );
    assert_eq!(response.body, br#"{"ok":true}"#);
    engine.close_async().await.unwrap();
    document_server.await.unwrap();
    api_server.await.unwrap();
}

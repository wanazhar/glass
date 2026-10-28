use super::*;

#[test]
fn native_selected_grandchild_beforeunload_gates_parked_ancestor_navigation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(tokio::task::LocalSet::new().run_until(async {
            let config = NativeEngineConfig::default()
                .with_fixture(
                    "fixture://frame-unload.test/parent",
                    "<title>Parent</title><iframe src='/child'></iframe>",
                )
                .unwrap()
                .with_fixture(
                    "fixture://frame-unload.test/child",
                    native_html_without_body_margin("<title>Child</title><script>globalThis.beforeCount = 0; addEventListener('beforeunload', event => { globalThis.beforeCount++; event.preventDefault(); event.returnValue = 'secret'; }); addEventListener('pagehide', () => globalThis.pagehideSeen = true); addEventListener('unload', () => globalThis.unloadSeen = true);</script><iframe src='/grand'></iframe><button id='activate-child'>Activate child</button>"),
                )
                .unwrap()
                .with_fixture(
                    "fixture://frame-unload.test/grand",
                    native_html_without_body_margin("<title>Grandchild</title><script>globalThis.beforeCount = 0; addEventListener('beforeunload', event => { globalThis.beforeCount++; event.preventDefault(); event.returnValue = 'secret'; }); addEventListener('pagehide', () => globalThis.pagehideSeen = true); addEventListener('unload', () => globalThis.unloadSeen = true);</script><button id='activate-grandchild'>Activate grandchild</button>"),
                )
                .unwrap()
                .with_fixture(
                    "fixture://frame-unload.test/next",
                    "<title>Next</title><p>New parent document</p>",
                )
                .unwrap()
                .with_initial_url("fixture://frame-unload.test/parent");
            let session = Arc::new(
                BrowserRuntimeSession::connect_native_with_modal_dialogs(config)
                    .await
                    .unwrap(),
            );
            let controller = session.native_dialog_controller().unwrap();
            let frames = session.list_frames().await.unwrap();
            let root_id = frames[0].id.clone();
            let child_id = frames
                .iter()
                .find(|frame| frame.parent_id.as_deref() == Some(root_id.as_str()))
                .unwrap()
                .id
                .clone();
            let grandchild_id = frames
                .iter()
                .find(|frame| frame.parent_id.as_deref() == Some(child_id.as_str()))
                .unwrap()
                .id
                .clone();

            session.select_frame(&child_id).await.unwrap();
            session
                .action(SemanticAction::Click {
                    target: "id=activate-child".into(),
                })
                .await
                .unwrap();
            session.select_frame(&grandchild_id).await.unwrap();
            session
                .action(SemanticAction::Click {
                    target: "id=activate-grandchild".into(),
                })
                .await
                .unwrap();

            let first_session = Arc::clone(&session);
            let mut first_navigation = tokio::task::spawn_local(async move {
                first_session
                    .script("window.top.location.assign('fixture://frame-unload.test/next'); true")
                    .await
            });
            let first_prompt = tokio::select! {
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => prompt,
                result = &mut first_navigation => panic!("ancestor navigation bypassed an activated descendant prompt: {result:?}"),
            };
            assert_eq!(first_prompt.frame_id, child_id);
            controller
                .resolve_dialog(
                    &first_prompt.id,
                    NativeDialogResolution {
                        accepted: false,
                        prompt_value: None,
                    },
                )
                .unwrap();
            first_navigation.await.unwrap().unwrap();

            let retained_frames = session.list_frames().await.unwrap();
            assert_eq!(retained_frames.len(), 3);
            assert!(retained_frames.iter().any(|frame| {
                frame.id == root_id && frame.url == "fixture://frame-unload.test/parent"
            }));
            assert!(retained_frames
                .iter()
                .any(|frame| frame.id == grandchild_id && frame.active));
            session.select_frame(&child_id).await.unwrap();
            assert_eq!(
                session
                    .script("[beforeCount, globalThis.pagehideSeen || null, globalThis.unloadSeen || null]")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!([1, null, null])
            );
            session.select_frame(&grandchild_id).await.unwrap();
            assert_eq!(
                session
                    .script("[beforeCount, globalThis.pagehideSeen || null, globalThis.unloadSeen || null]")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!([1, null, null])
            );

            session.select_frame(&root_id).await.unwrap();
            let reload_revision = session
                .evidence(EvidenceLevel::Compact)
                .await
                .unwrap()
                .revision;
            let reload_session = Arc::clone(&session);
            let mut reload = tokio::task::spawn_local(async move {
                reload_session
                    .native_reload_with_revision(reload_revision)
                    .await
            });
            let reload_prompt = tokio::select! {
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => prompt,
                result = &mut reload => panic!("reload bypassed an activated descendant beforeunload: {result:?}"),
            };
            assert_ne!(first_prompt.id, reload_prompt.id);
            assert_eq!(reload_prompt.frame_id, child_id);
            controller
                .resolve_dialog(
                    &reload_prompt.id,
                    NativeDialogResolution {
                        accepted: false,
                        prompt_value: None,
                    },
                )
                .unwrap();
            let reload_outcome = reload.await.unwrap().unwrap();
            assert_eq!(reload_outcome.action, "reload");
            let retained_frames = session.list_frames().await.unwrap();
            assert_eq!(retained_frames.len(), 3);
            assert!(retained_frames.iter().any(|frame| {
                frame.id == root_id && frame.url == "fixture://frame-unload.test/parent"
            }));
            assert!(retained_frames
                .iter()
                .any(|frame| frame.id == root_id && frame.active));
            session.select_frame(&child_id).await.unwrap();
            assert_eq!(
                session
                    .script("[beforeCount, globalThis.pagehideSeen || null, globalThis.unloadSeen || null]")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!([2, null, null])
            );
            session.select_frame(&grandchild_id).await.unwrap();
            assert_eq!(
                session
                    .script("[beforeCount, globalThis.pagehideSeen || null, globalThis.unloadSeen || null]")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!([2, null, null])
            );

            let second_session = Arc::clone(&session);
            let mut second_navigation = tokio::task::spawn_local(async move {
                second_session
                    .script("window.top.location.assign('fixture://frame-unload.test/next'); true")
                    .await
            });
            let second_prompt = tokio::select! {
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => prompt,
                result = &mut second_navigation => panic!("accepted ancestor navigation never paused for beforeunload: {result:?}"),
            };
            assert_ne!(first_prompt.id, second_prompt.id);
            assert_eq!(second_prompt.frame_id, child_id);
            controller
                .resolve_dialog(
                    &second_prompt.id,
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: None,
                    },
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), second_navigation)
                .await
                .expect("accepted ancestor navigation did not complete")
                .unwrap()
                .unwrap();
            let final_frames = session.list_frames().await.unwrap();
            assert_eq!(final_frames.len(), 1);
            assert_eq!(final_frames[0].id, root_id);
            assert_eq!(final_frames[0].url, "fixture://frame-unload.test/next");

            Arc::try_unwrap(session)
                .unwrap_or_else(|_| panic!("native session still has unexpected owners"))
                .close()
                .await
                .unwrap();
        }));
    });
}
#[tokio::test]
async fn native_child_script_click_owns_external_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/start", "/destination"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/start" {
                "<title>Start</title><a id='next' href='/destination'>Next</a>".to_owned()
            } else {
                "<title>Destination</title><p>Arrived</p>".to_owned()
            };
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
    let initial_revision = engine.revision();
    engine
        .evaluate_async("document.getElementById('next').click()")
        .await
        .unwrap();
    assert_eq!(engine.revision(), initial_revision + 2);
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{address}/destination")
    );
    assert_eq!(engine.snapshot().unwrap().title, "Destination");
    assert_eq!(engine.snapshot().unwrap().visible_text, "Arrived");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_direct_click_owns_external_link_navigation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/start", "/destination"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/start" {
                "<title>Start</title><a id='next' href='/destination'>Next</a>"
            } else {
                "<title>Destination</title><p>Arrived</p>"
            };
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
    let initial_revision = engine.revision();
    engine
        .action_async(NativeAction::Click {
            target: "id=next".into(),
        })
        .await
        .unwrap();
    // A direct navigation owns three visible revisions: the click event, its
    // beforeunload lifecycle dispatch, and the destination document commit.
    assert_eq!(engine.revision(), initial_revision + 3);
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.url, format!("http://{address}/destination"));
    assert_eq!(snapshot.title, "Destination");
    assert_eq!(snapshot.visible_text, "Arrived");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_external_keyboard_link_default_actions_preserve_download() {
    let _guard = native_content_process_test_lock().lock().await;
    let start_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let start_address = start_listener.local_addr().unwrap();
    let download_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let download_address = download_listener.local_addr().unwrap();
    let start_server = tokio::spawn(async move {
        let (mut stream, _) = start_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/start"));
        let body = format!(
            "<title>Download</title><a id='click-file' href='http://{download_address}/asset.bin' download='click-report.bin'>Click download</a><a id='keyboard-file' href='http://{download_address}/asset.bin' download='keyboard-report.bin'>Keyboard download</a>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let download_server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = download_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/asset.bin"));
            let body = b"native-download-bytes";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{start_address}/start")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let initial_revision = engine.revision();
    engine
        .action_async(NativeAction::Click {
            target: "id=click-file".into(),
        })
        .await
        .unwrap();
    assert_eq!(engine.revision(), initial_revision + 1);
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{start_address}/start")
    );
    engine
        .evaluate_async("document.getElementById('keyboard-file').focus()")
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        format!("http://{start_address}/start")
    );
    let queued = engine.download_ids().unwrap();
    assert_eq!(queued, vec!["native-download-1", "native-download-2"]);

    let download_directory = std::env::temp_dir().join(format!(
        "glass-native-download-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&download_directory).unwrap();
    let outcome = engine
        .wait_for_download_async(&download_directory, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(outcome.guid, "native-download-1");
    assert_eq!(outcome.suggested_filename, "click-report.bin");
    assert_eq!(outcome.state, "completed");
    assert_eq!(
        outcome.received_bytes,
        b"native-download-bytes".len() as u64
    );
    assert!(outcome.sha256.is_some());
    assert_eq!(
        fs::read(download_directory.join("click-report.bin")).unwrap(),
        b"native-download-bytes"
    );
    let keyboard_outcome = engine
        .wait_for_download_async(&download_directory, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(keyboard_outcome.guid, "native-download-2");
    assert_eq!(keyboard_outcome.suggested_filename, "keyboard-report.bin");
    assert_eq!(keyboard_outcome.state, "completed");
    assert_eq!(
        keyboard_outcome.received_bytes,
        b"native-download-bytes".len() as u64
    );
    assert_eq!(
        fs::read(download_directory.join("keyboard-report.bin")).unwrap(),
        b"native-download-bytes"
    );
    assert_eq!(engine.completed_download_count().unwrap(), 2);
    assert_eq!(
        engine.download_ids().unwrap(),
        vec!["native-download-1", "native-download-2"]
    );
    engine.close_async().await.unwrap();
    fs::remove_dir_all(&download_directory).unwrap();
    start_server.await.unwrap();
    download_server.await.unwrap();
}
#[test]
fn native_nowrap_collapses_whitespace_without_soft_wrap_and_reaches_root_scroll() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://nowrap",
            native_html_without_body_margin("<style>#flow { white-space: nowrap; line-height: 24px; }</style><div id='flow'> A   B\n<span id='inline' style='display:inline; white-space:nowrap'> C   D E</span><span id='hidden' style='display:none;white-space:nowrap'> hidden hidden hidden</span></div>"),
        )
        .unwrap()
        .with_initial_url("fixture://nowrap");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial = engine.layout().unwrap();
    assert!(initial.content_width > initial.viewport.width);
    assert!(initial.max_scroll_offset().x > 0);
    assert_eq!(initial.max_scroll_offset().y, 0);
    assert!(initial.text_runs.iter().all(|run| run.origin.y == 0));
    assert!(initial.text_runs.iter().any(|run| run.text == "A B"));
    assert!(initial.text_runs.iter().any(|run| run.text == "C D E"));
    assert!(
        initial
            .text_runs
            .iter()
            .all(|run| !run.text.contains("hidden"))
    );

    let unscrolled = engine.rasterize().unwrap();
    let moved = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: 0,
        })
        .unwrap();
    assert!(moved.accepted);
    assert_eq!(moved.revision, 2);
    assert_eq!(engine.scroll_offset().x, initial.max_scroll_offset().x);
    assert_eq!(engine.scroll_offset().y, 0);
    assert_eq!(
        engine.display_list().unwrap().scroll_offset.x,
        engine.scroll_offset().x
    );
    let scrolled = engine.rasterize().unwrap();
    assert_ne!(unscrolled.rgba(), scrolled.rgba());

    let no_op = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: 0,
        })
        .unwrap();
    assert!(!no_op.accepted);
    assert_eq!(no_op.revision, 2);
}
#[test]
fn native_axis_specific_overflow_clips_only_selected_axis_across_consumers() {
    let x_only = NativeDocument::parse(
        "<style>#clip { overflow-x: hidden; width: 16px; height: 10px; } #child { display: block; white-space: nowrap; background-color: red; }</style><div id='clip'><div id='child'>ABCDEFG</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let x_clip = x_only.resolve_target("id=clip").unwrap();
    let x_child = x_only.resolve_target("id=child").unwrap();
    let x_layout = x_only.layout(viewport).unwrap();

    assert_eq!(x_layout.box_for(x_clip).unwrap().height, 10);
    assert_eq!(
        x_layout.viewport_rect_for(x_child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 20,
        })
    );
    assert_eq!(x_layout.content_width, viewport.width);
    assert_eq!(x_layout.max_scroll_offset().x, 0);
    assert_eq!(x_layout.hit_test(5, 15).unwrap(), Some(x_child));
    assert_ne!(x_layout.hit_test(16, 15).unwrap(), Some(x_child));
    let x_surface = x_only.display_list(viewport).unwrap().rasterize().unwrap();
    assert_eq!(x_surface.pixel(5, 15), Some([255, 0, 0, 255]));

    let y_only = NativeDocument::parse(
        "<style>#clip { overflow-y: clip; width: 16px; height: 10px; } #child { display: block; white-space: nowrap; background-color: red; }</style><div id='clip'><div id='child'>ABCDEFG</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let y_child = y_only.resolve_target("id=child").unwrap();
    let y_layout = y_only.layout(viewport).unwrap();

    assert_eq!(
        y_layout.viewport_rect_for(y_child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(y_layout.content_width, 56);
    assert_eq!(y_layout.max_scroll_offset().x, 24);
    assert_eq!(y_layout.hit_test(5, 5).unwrap(), Some(y_child));
    assert_ne!(y_layout.hit_test(5, 15).unwrap(), Some(y_child));
    let y_surface = y_only.display_list(viewport).unwrap().rasterize().unwrap();
    assert_eq!(y_surface.pixel(5, 5), Some([255, 0, 0, 255]));
    assert_eq!(y_surface.pixel(5, 15), Some([255, 255, 255, 255]));

    assert!(!x_only.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            && matches!(diagnostic.detail.as_str(), "overflow-x" | "overflow-y")
    }));
    assert!(!y_only.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            && matches!(diagnostic.detail.as_str(), "overflow-x" | "overflow-y")
    }));
}
#[test]
fn native_point_click_uses_the_real_backend_dispatcher() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_point_click_uses_the_real_backend_dispatcher_inner());
    });
}

async fn native_point_click_uses_the_real_backend_dispatcher_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://point-dispatch",
            native_html_without_body_margin("<button id='save'>Save</button>"),
        )
        .unwrap()
        .with_initial_url("fixture://point-dispatch");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let clicked = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "point=1,1".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(clicked.revision, 2);
    assert!(clicked.accepted);

    let outside = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "point=319,199".into(),
            },
        })
        .await
        .unwrap_err();
    assert!(matches!(
        outside,
        glass_browser::browser_backend::BrowserBackendError::UnsupportedOperation {
            operation, ..
        } if operation == "action"
    ));
    let evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(evidence.revision, 2);
    dispatcher.close().await.unwrap();
}
#[test]
fn local_link_activation_uses_bounded_navigation_default_action() {
    std::thread::Builder::new()
        .name("native-link-activation-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("link activation runtime should build");
            runtime.block_on(local_link_activation_uses_bounded_navigation_default_action_inner());
        })
        .expect("link activation test thread should start")
        .join()
        .expect("link activation test thread should complete");
}
#[tokio::test]
async fn fragment_targets_scroll_and_restore_bounded_history_offsets() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 160,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://anchors",
            "<a id='jump' href='#target'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><p>twenty-seven twenty-eight twenty-nine thirty thirty-one thirty-two thirty-three thirty-four thirty-five</p><p>thirty-six thirty-seven thirty-eight thirty-nine forty forty-one forty-two forty-three forty-four forty-five</p><button id='target'>Target</button><p id='hidden' style='display:none'>Hidden</p><p id='duplicate'>First</p><p id='duplicate'>Second</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://other",
            "<p>other one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><button id='other-target'>Other target</button>",
        )
        .unwrap()
        .with_initial_url("fixture://anchors");

    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let target_id = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.tag_name == "button" && node.name == "Target")
        .map(|node| node.node_id)
        .unwrap();
    let target_box = engine.layout().unwrap().box_for(target_id).unwrap();
    assert!(target_box.y > 40);
    let max_scroll = engine.layout().unwrap().max_scroll_offset().y;
    let target_scroll = NativePoint {
        x: 0,
        y: target_box.y.min(max_scroll),
    };
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    assert_eq!(
        engine.history().current().unwrap().scroll_offset,
        NativePoint { x: 0, y: 0 }
    );

    let jump = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jump.accepted);
    assert_eq!(engine.snapshot().unwrap().url, "fixture://anchors#target");
    assert_eq!(engine.scroll_offset(), target_scroll);
    assert_eq!(
        engine.history().current().unwrap().scroll_offset,
        target_scroll
    );

    let missing = engine.navigate("fixture://anchors#missing").unwrap();
    assert_eq!(missing.url, "fixture://anchors#missing");
    assert_eq!(engine.scroll_offset(), target_scroll);
    assert_eq!(
        engine.history().current().unwrap().scroll_offset,
        target_scroll
    );

    let target = engine.go_back().unwrap().unwrap();
    assert_eq!(target.url, "fixture://anchors#target");
    assert_eq!(engine.scroll_offset(), target_scroll);
    let initial = engine.go_back().unwrap().unwrap();
    assert_eq!(initial.url, "fixture://anchors");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    assert_eq!(
        engine.history().current().unwrap().scroll_offset,
        NativePoint { x: 0, y: 0 }
    );
    let target_again = engine.go_forward().unwrap().unwrap();
    assert_eq!(target_again.url, "fixture://anchors#target");
    assert_eq!(engine.scroll_offset(), target_scroll);

    engine.go_back().unwrap().unwrap();
    let scrolled_initial = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: 1,
        })
        .unwrap();
    assert!(scrolled_initial.accepted);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 1 });
    assert_eq!(
        engine.history().current().unwrap().scroll_offset,
        NativePoint { x: 0, y: 1 }
    );

    let other = engine.navigate("fixture://other#other-target").unwrap();
    assert_eq!(other.url, "fixture://other#other-target");
    let other_target_id = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.tag_name == "button" && node.name == "Other target")
        .map(|node| node.node_id)
        .unwrap();
    let other_target_box = engine.layout().unwrap().box_for(other_target_id).unwrap();
    let other_max_scroll = engine.layout().unwrap().max_scroll_offset().y;
    assert_eq!(
        engine.scroll_offset(),
        NativePoint {
            x: 0,
            y: other_target_box.y.min(other_max_scroll),
        }
    );

    let restored = engine.go_back().unwrap().unwrap();
    assert_eq!(restored.url, "fixture://anchors");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 1 });

    let hidden = engine.navigate("fixture://anchors#hidden").unwrap();
    assert_eq!(hidden.url, "fixture://anchors#hidden");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 1 });
    let duplicate = engine.navigate("fixture://anchors#duplicate").unwrap();
    assert_eq!(duplicate.url, "fixture://anchors#duplicate");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 1 });
}
#[tokio::test]
async fn legacy_name_fragments_use_decoded_fallback_and_preserve_scroll_safety() {
    let html = native_html_without_body_margin(
        "<a id='jump' href='#legacy%20plan'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><a name='legacy plan'>Legacy target</a><a name='café'>UTF-8 legacy target</a><a name='same'>Name should lose</a><a name='hidden' style='display:none'>Hidden legacy target</a><a name='zero' style='display:contents'></a><a name='duplicate'>First duplicate</a><a name='duplicate'>Second duplicate</a><div name='not-anchor'>Not a target</div><p id='same'>ID target</p>",
    );
    let viewport = Viewport {
        width: 160,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let legacy = find_element_with_attribute(&expected_document, "a", "name", "legacy plan");
    let legacy_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(legacy)
            .expect("legacy anchor layout")
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(legacy_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://name-fragments", html)
        .unwrap()
        .with_initial_url("fixture://name-fragments");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let jumped = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jumped.accepted);
    assert_eq!(engine.scroll_offset(), legacy_scroll);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://name-fragments#legacy%20plan"
    );

    let initial = engine.go_back().unwrap().unwrap();
    assert_eq!(initial.url, "fixture://name-fragments");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    let restored = engine.go_forward().unwrap().unwrap();
    assert_eq!(restored.url, "fixture://name-fragments#legacy%20plan");
    assert_eq!(engine.scroll_offset(), legacy_scroll);

    engine
        .navigate("fixture://name-fragments#caf%C3%A9")
        .unwrap();
    let utf8_target = find_element_with_attribute(&expected_document, "a", "name", "café");
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(utf8_target)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );

    engine.navigate("fixture://name-fragments#same").unwrap();
    let id_target = expected_document.resolve_target("id=same").unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(id_target)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );
    for fragment in [
        "duplicate",
        "not-anchor",
        "DUPLICATE",
        "hidden",
        "zero",
        "missing",
    ] {
        let before = engine.scroll_offset();
        engine
            .navigate(format!("fixture://name-fragments#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), before, "fragment={fragment}");
    }
}
#[test]
fn semantic_actions_and_effects_use_the_backend_contract() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(semantic_actions_and_effects_use_the_backend_contract_inner());
    });
}

async fn semantic_actions_and_effects_use_the_backend_contract_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://controls",
            "<label for='name'>Name</label><input id='name' type='text'><button id='save'>Save</button><input id='remember' type='checkbox'>",
        )
        .unwrap()
        .with_initial_url("fixture://controls");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let clicked = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "id=remember".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(clicked.revision, 2);
    assert!(clicked.accepted);

    let click_effects = dispatcher
        .effects(EffectsRequest {
            context_id: "native-context".into(),
            since_revision: 1,
        })
        .await
        .unwrap();
    assert_eq!(click_effects.revision, 2);
    assert!(click_effects.changed);

    let typed = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Type {
                target: "id=name".into(),
                text: "Glass".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(typed.revision, 3);

    let unchanged = dispatcher
        .effects(EffectsRequest {
            context_id: "native-context".into(),
            since_revision: 3,
        })
        .await
        .unwrap();
    assert_eq!(unchanged.revision, 3);
    assert!(!unchanged.changed);

    let keypress = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::KeyPress { key: "!".into() },
        })
        .await
        .unwrap();
    assert_eq!(keypress.revision, 4);
    assert!(keypress.accepted);

    let future = dispatcher
        .effects(EffectsRequest {
            context_id: "native-context".into(),
            since_revision: 5,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        future,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "since revision"
    ));
    dispatcher.close().await.unwrap();
}
#[test]
fn native_form_actions_clear_check_and_select_controls() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://form-actions",
            "<input id='name' type='text' value='loaded'><input id='remember' type='checkbox'><select id='country'><option value='one'>One</option><option value='two'>Two</option></select>",
        )
        .unwrap()
        .with_initial_url("fixture://form-actions");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let initial_revision = engine.revision();

    let cleared = engine
        .action(NativeAction::Clear {
            target: "id=name".into(),
        })
        .unwrap();
    assert_eq!(cleared.revision, initial_revision + 1);
    assert_eq!(
        engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .find(|node| node.input_type.as_deref() == Some("text"))
            .and_then(|node| node.empty),
        Some(true)
    );

    let checked = engine
        .action(NativeAction::Check {
            target: "id=remember".into(),
        })
        .unwrap();
    assert_eq!(checked.revision, cleared.revision + 1);
    assert_eq!(
        engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .find(|node| node.input_type.as_deref() == Some("checkbox"))
            .and_then(|node| node.checked),
        Some(true)
    );

    let unchecked = engine
        .action(NativeAction::Uncheck {
            target: "id=remember".into(),
        })
        .unwrap();
    assert_eq!(unchecked.revision, checked.revision + 1);
    assert_eq!(
        engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .find(|node| node.input_type.as_deref() == Some("checkbox"))
            .and_then(|node| node.checked),
        Some(false)
    );

    let selected = engine
        .action(NativeAction::Select {
            target: "id=country".into(),
            value: "two".into(),
        })
        .unwrap();
    assert_eq!(selected.revision, unchecked.revision + 1);
    let options = engine.semantic_nodes().unwrap();
    assert_eq!(
        options
            .iter()
            .find(|node| node.name == "One")
            .and_then(|node| node.selected),
        Some(false)
    );
    assert_eq!(
        options
            .iter()
            .find(|node| node.name == "Two")
            .and_then(|node| node.selected),
        Some(true)
    );

    let no_op_revision = engine.revision();
    let already_unchecked = engine
        .action(NativeAction::Uncheck {
            target: "id=remember".into(),
        })
        .unwrap();
    assert!(already_unchecked.accepted);
    assert_eq!(already_unchecked.revision, no_op_revision);
}
#[test]
fn visibility_projection_and_actionability_are_consistent() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://visibility",
            "<button id='visible'>Visible</button><button id='hidden' hidden>Hidden</button><button id='aria' aria-hidden='true'>Aria</button><button id='display' style='display: none'>Display</button><button id='visibility' style='VISIBILITY : HIDDEN'>Visibility</button><div hidden><button id='nested'>Nested</button></div><input id='hidden-input' type='text' style='display:none'><button id='opaque' style='opacity:0'>Opaque</button>",
        )
        .unwrap()
        .with_initial_url("fixture://visibility");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.visible_text, "Visible Opaque");
    assert!(!snapshot.text_truncated);

    let nodes = engine.semantic_nodes().unwrap();
    let visible = nodes.iter().find(|node| node.name == "Visible").unwrap();
    assert!(!visible.hidden);
    let opaque = nodes.iter().find(|node| node.name == "Opaque").unwrap();
    assert!(!opaque.hidden);
    for name in ["Hidden", "Aria", "Display", "Visibility", "Nested"] {
        assert!(
            nodes
                .iter()
                .find(|node| node.name == name)
                .is_some_and(|node| node.hidden),
            "expected {name} to be hidden"
        );
    }
    assert!(
        nodes
            .iter()
            .find(|node| node.role == "textbox" && node.name.is_empty())
            .is_some_and(|node| node.hidden)
    );

    let revision_before_rejections = engine.revision();
    for target in ["hidden", "aria", "display", "visibility", "nested"] {
        let result = engine.action(NativeAction::Click {
            target: format!("id={target}"),
        });
        assert!(matches!(
            result,
            Err(NativeEngineError::TargetNotActionable { reason })
                if reason == "hidden targets are not actionable"
        ));
    }
    let result = engine.action(NativeAction::Type {
        target: "id=hidden-input".into(),
        text: "secret".into(),
    });
    assert!(matches!(
        result,
        Err(NativeEngineError::TargetNotActionable { reason })
            if reason == "hidden targets are not actionable"
    ));
    assert_eq!(engine.revision(), revision_before_rejections);
    assert!(
        engine
            .effects_since(revision_before_rejections)
            .unwrap()
            .effects
            .is_empty()
    );
    assert!(
        !engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .any(|node| node.focused)
    );
}
#[test]
fn stylesheet_presentation_state_feeds_text_and_actionability() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://styles",
            "<style>/* bounded rules */ .gone, [data-mode=off] { display: none; } #covered { visibility: hidden; } button { display: none; }</style><button id='gone' class='gone'>Gone</button><button id='covered'>Covered</button><button id='attribute' data-mode='off'>Attribute</button><button id='override' style='display: block'>Override</button>",
        )
        .unwrap()
        .with_initial_url("fixture://styles");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.visible_text, "Override");

    let nodes = engine.semantic_nodes().unwrap();
    for name in ["Gone", "Covered", "Attribute"] {
        assert!(
            nodes
                .iter()
                .find(|node| node.name == name)
                .is_some_and(|node| node.hidden),
            "expected {name} to be hidden by stylesheet"
        );
    }
    assert!(
        nodes
            .iter()
            .find(|node| node.name == "Override")
            .is_some_and(|node| !node.hidden)
    );

    let revision_before_rejections = engine.revision();
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=gone".into(),
        }),
        Err(NativeEngineError::TargetNotActionable { .. })
    ));
    assert_eq!(engine.revision(), revision_before_rejections);
    let click = engine
        .action(NativeAction::Click {
            target: "id=override".into(),
        })
        .unwrap();
    assert_eq!(click.revision, revision_before_rejections + 1);
}

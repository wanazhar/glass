use super::*;

#[test]
fn native_image_map_point_actions_route_dead_and_linked_areas() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_image_map_point_actions_route_dead_and_linked_areas_inner());
    });
}

async fn native_image_map_point_actions_route_dead_and_linked_areas_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://image-map-parent",
            "<img id='mapped' usemap='#places' style='display:block;width:100px;height:100px'><map name='places'><area id='dead' coords='10,10,40,40'><area id='chosen' shape='rect' coords='0,0,70,70' href='fixture://map-original' alt='Chosen area'><area id='cancel' shape='rect' coords='70,0,99,30' href='fixture://map-canceled' alt='Canceled area'></map><script>globalThis.__mapClicks = []; document.getElementById('dead').addEventListener('click', event => __mapClicks.push([event.target.id, event.ctrlKey])); document.getElementById('chosen').addEventListener('click', event => { __mapClicks.push([event.target.id, event.ctrlKey]); event.currentTarget.setAttribute('href', 'fixture://map-current'); event.currentTarget.setAttribute('target', '_blank'); }); document.getElementById('cancel').addEventListener('click', event => { __mapClicks.push([event.target.id, event.defaultPrevented]); event.preventDefault(); });</script>",
        )
        .unwrap()
        .with_fixture("fixture://map-current", "<title>Current area destination</title>")
        .unwrap()
        .with_fixture("fixture://map-original", "<title>Stale area destination</title>")
        .unwrap()
        .with_fixture("fixture://map-canceled", "<title>Canceled area destination</title>")
        .unwrap()
        .with_initial_url("fixture://image-map-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let initial = session
        .semantic_observe(SemanticObservationLevel::Interactive)
        .await
        .unwrap();
    assert!(
        initial
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .any(|target| target.role == "link" && target.name == "Chosen area")
    );
    assert_eq!(
        initial
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .filter(|target| target.role == "link")
            .count(),
        2
    );

    session
        .action(SemanticAction::Click {
            target: "point=20,20".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session.script("location.href").await.unwrap().value,
        serde_json::json!("fixture://image-map-parent")
    );
    assert_eq!(
        session
            .script("globalThis.__mapClicks")
            .await
            .unwrap()
            .value,
        serde_json::json!([["dead", false]])
    );

    session
        .action(SemanticAction::ClickWithModifiers {
            target: "point=50,50".into(),
            modifiers: ClickModifiers {
                control: true,
                ..ClickModifiers::default()
            },
        })
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(targets.iter().any(|target| {
        !target.active
            && target.url == "fixture://map-current"
            && target.opener_id.as_deref() == Some("native-context")
    }));
    assert_eq!(
        session
            .script("globalThis.__mapClicks")
            .await
            .unwrap()
            .value,
        serde_json::json!([["dead", false], ["chosen", true]])
    );

    let refreshed = session
        .semantic_observe(SemanticObservationLevel::Interactive)
        .await
        .unwrap();
    let chosen = refreshed
        .regions
        .iter()
        .flat_map(|region| region.targets.iter())
        .find(|target| target.role == "link" && target.name == "Chosen area")
        .expect("linked image-map area remains addressable after live attribute mutation");
    session
        .action(SemanticAction::Click {
            target: chosen.reference.clone(),
        })
        .await
        .unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 3);
    assert!(
        session
            .native_list_targets()
            .await
            .unwrap()
            .iter()
            .any(|target| !target.active && target.url == "fixture://map-current")
    );

    session
        .action(SemanticAction::Click {
            target: "point=80,20".into(),
        })
        .await
        .unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 3);
    assert_eq!(
        session.script("location.href").await.unwrap().value,
        serde_json::json!("fixture://image-map-parent")
    );
    session.close().await.unwrap();
}
#[test]
fn native_image_map_keyboard_focus_and_enter_activation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_image_map_keyboard_focus_and_enter_activation_inner());
    });
}

async fn native_image_map_keyboard_focus_and_enter_activation_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://image-map-keyboard",
            "<a id='before' href='fixture://before'>Before</a><img id='first-image' usemap='#keyboard-map' style='display:block;width:100px;height:100px'><img id='second-image' usemap='#keyboard-map' style='display:block;width:100px;height:100px'><map name='keyboard-map'><area id='dead-area' coords='0,0,10,10'><area id='negative-area' tabindex='-1' shape='default' href='fixture://negative' alt='Skipped'><area id='empty-area' shape='rect' coords='0,0,5' href='fixture://empty' alt='Empty'><area id='keyboard-area' tabindex='2' shape='rect' coords='0,0,99,99' href='fixture://keyboard-stale' alt='Keyboard destination'><area id='zero-area' tabindex='0' shape='default' href='#zero' alt='Natural order destination'></map><a id='after' href='fixture://after'>After</a><script>globalThis.__areaKeyboardTrace = []; globalThis.__cancelAreaKeydown = false; globalThis.__cancelAreaClick = false; globalThis.__mutateAreaHref = false; const firstImage = document.getElementById('first-image'); const area = document.getElementById('keyboard-area'); firstImage.addEventListener('keydown', event => { __areaKeyboardTrace.push('image-keydown:' + event.key + ':' + event.target.id); if (__cancelAreaKeydown && event.key === 'Enter') event.preventDefault(); }); firstImage.addEventListener('keyup', event => __areaKeyboardTrace.push('image-keyup:' + event.key + ':' + event.target.id)); area.addEventListener('keydown', () => __areaKeyboardTrace.push('area-keydown')); area.addEventListener('click', event => { __areaKeyboardTrace.push('area-click:' + event.target.id); if (__cancelAreaClick) event.preventDefault(); if (__mutateAreaHref) { area.setAttribute('href', 'fixture://image-map-keyboard-destination'); area.setAttribute('target', '_blank'); } });</script>",
        )
        .unwrap()
        .with_fixture(
            "fixture://image-map-keyboard-destination",
            "<title>Keyboard map destination</title>",
        )
        .unwrap()
        .with_fixture("fixture://keyboard-stale", "<title>Stale area target</title>")
        .unwrap()
        .with_fixture("fixture://negative", "<title>Negative area</title>")
        .unwrap()
        .with_fixture("fixture://empty", "<title>Empty area</title>")
        .unwrap()
        .with_fixture("fixture://before", "<title>Before</title>")
        .unwrap()
        .with_fixture("fixture://after", "<title>After</title>")
        .unwrap()
        .with_initial_url("fixture://image-map-keyboard");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

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
        serde_json::json!("first-image")
    );
    assert_eq!(
        session
            .script("document.getElementById('keyboard-area').matches(':focus')")
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
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
        serde_json::json!("second-image")
    );
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Shift+Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("first-image")
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
        serde_json::json!("second-image")
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
        serde_json::json!("before")
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
        serde_json::json!("first-image")
    );
    assert_eq!(
        session
            .script("document.getElementById('zero-area').matches(':focus')")
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
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
        serde_json::json!("second-image")
    );
    assert_eq!(
        session
            .script("document.getElementById('zero-area').matches(':focus')")
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
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
        serde_json::json!("after")
    );

    session
        .script("globalThis.__areaKeyboardTrace = []; globalThis.__cancelAreaKeydown = true; document.getElementById('keyboard-area').focus(); true")
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
            .script("globalThis.__areaKeyboardTrace")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            "image-keydown:Enter:first-image",
            "image-keyup:Enter:first-image"
        ])
    );
    assert_eq!(
        session
            .script("document.activeElement.id")
            .await
            .unwrap()
            .value,
        serde_json::json!("first-image")
    );

    session
        .script("globalThis.__areaKeyboardTrace = []; globalThis.__cancelAreaKeydown = false; globalThis.__cancelAreaClick = true; document.getElementById('keyboard-area').focus(); true")
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
            .script("globalThis.__areaKeyboardTrace")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            "image-keydown:Enter:first-image",
            "area-click:keyboard-area",
            "image-keyup:Enter:first-image",
        ])
    );
    assert_eq!(session.native_list_targets().await.unwrap().len(), 1);

    session
        .script("globalThis.__areaKeyboardTrace = []; globalThis.__cancelAreaClick = false; globalThis.__mutateAreaHref = true; document.getElementById('keyboard-area').focus(); true")
        .await
        .unwrap();
    session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(targets.iter().any(|target| {
        !target.active
            && target.url == "fixture://image-map-keyboard-destination"
            && target.opener_id.as_deref() == Some("native-context")
    }));
    assert_eq!(
        session
            .script("globalThis.__areaKeyboardTrace")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            "image-keydown:Enter:first-image",
            "area-click:keyboard-area",
            "image-keyup:Enter:first-image",
        ])
    );

    session
        .script("globalThis.__areaKeyboardTrace = []; globalThis.__mutateAreaHref = false; document.getElementById('keyboard-area').focus(); true")
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
            .script("globalThis.__areaKeyboardTrace")
            .await
            .unwrap()
            .value,
        serde_json::json!(["image-keydown: :first-image", "image-keyup: :first-image"])
    );
    session.close().await.unwrap();
}
#[tokio::test]
async fn native_image_bitmap_transfer_preserves_pixels_across_worker_realm() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 8,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://image-bitmap-transfer-page",
            "<html><body><canvas id='canvas' width='2' height='1' style='display:block;width:2px;height:1px'></canvas></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://image-bitmap-transfer-worker",
            r#"self.onmessage = event => {
                const bitmap = event.data.bitmap;
                postMessage({
                    bitmap,
                    identity: [bitmap instanceof ImageBitmap, bitmap.width, bitmap.height],
                }, [bitmap]);
            }"#,
        )
        .unwrap()
        .with_initial_url("fixture://image-bitmap-transfer-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"await (async () => {
                    const canvas = document.getElementById('canvas');
                    const context = canvas.getContext('2d');
                    context.fillStyle = '#ff0000';
                    context.fillRect(0, 0, 1, 1);
                    context.fillStyle = '#0000ff';
                    context.fillRect(1, 0, 1, 1);
                    const localBitmap = await createImageBitmap(canvas);
                    const localClone = structuredClone(localBitmap, { transfer: [localBitmap] });
                    globalThis.localBitmapTransfer = [
                        localBitmap.width,
                        localClone instanceof ImageBitmap,
                        localClone.width,
                        localClone.height,
                    ];
                    const closedBitmap = await createImageBitmap(canvas);
                    closedBitmap.close();
                    let closedTransferError = '';
                    try { structuredClone(closedBitmap, { transfer: [closedBitmap] }); }
                    catch (error) { closedTransferError = error.name; }
                    globalThis.bitmap = await createImageBitmap(canvas);
                    globalThis.workerMessages = [];
                    globalThis.worker = new Worker('fixture://image-bitmap-transfer-worker');
                    worker.onmessage = event => {
                        const output = document.createElement('canvas');
                        output.width = 2;
                        output.height = 1;
                        document.body.appendChild(output);
                        const outputContext = output.getContext('2d');
                        outputContext.drawImage(event.data.bitmap, 0, 0);
                        workerMessages.push({
                            identity: event.data.identity,
                            pixels: Array.from(outputContext.getImageData(0, 0, 2, 1).data),
                        });
                    };
                    globalThis.closedTransferError = closedTransferError;
                    globalThis.senderBitmap = null;
                    return true;
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "worker.postMessage({ bitmap }, [bitmap]); senderBitmap = [bitmap.width, bitmap.height]; true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "({ localBitmapTransfer, closedTransferError, senderBitmap, workerMessages })"
            )
            .await
            .unwrap(),
        serde_json::json!({
            "localBitmapTransfer": [0, true, 2, 1],
            "closedTransferError": "DataCloneError",
            "senderBitmap": [0, 0],
            "workerMessages": [{
                "identity": [true, 2, 1],
                "pixels": [255, 0, 0, 255, 0, 0, 255, 255],
            }],
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_canvas_image_source_preserves_animated_frames() {
    let source = format!(
        "data:image/gif;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_animated_gif_bytes())
    );
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 4,
            height: 2,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://animated-canvas-image",
            format!(
                "<canvas id='canvas' width='1' height='1' style='display:block;width:1px;height:1px'></canvas><img id='source' src='{source}' style='display:none'>"
            ),
        )
        .unwrap()
        .with_initial_url("fixture://animated-canvas-image");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let result = engine
        .evaluate_async(
            "(() => { const image = document.getElementById('source'); const canvas = document.getElementById('canvas'); const context = canvas.getContext('2d'); const resource = globalThis.__glassCanvasImageResources.get(image.nodeIndex); const pixel = () => Array.from(context.getImageData(0, 0, 1, 1).data); resource.animationOriginNowMs = Number.MAX_SAFE_INTEGER; context.drawImage(image, 0, 0); const first = pixel(); resource.loopCount = 0; resource.animationOriginNowMs = -Number.MAX_SAFE_INTEGER; context.clearRect(0, 0, 1, 1); context.drawImage(image, 0, 0); const last = pixel(); return { frames: resource.frames.length, delays: resource.frames.map(frame => frame.delayMs), first, last, complete: image.complete, dimensions: [image.naturalWidth, image.naturalHeight] }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "frames": 2,
            "delays": [10, 10],
            "first": [255, 0, 0, 255],
            "last": [0, 255, 0, 255],
            "complete": true,
            "dimensions": [1, 1]
        })
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_canvas_cross_origin_image_taints_readback() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let image_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let image_address = image_listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = format!(
            "<img id='source' src='http://{image_address}/image.png'><div id='host'></div>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let image_server = tokio::spawn(async move {
        let (mut stream, _) = image_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/image.png"));
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            png.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(&png).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 8,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{page_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let result = engine
        .evaluate_async(
            "(() => { const source = document.getElementById('source'); const canvas = document.createElement('canvas'); canvas.width = 2; canvas.height = 2; document.getElementById('host').appendChild(canvas); const context = canvas.getContext('2d'); context.drawImage(source, 0, 0); const readback = (() => { try { context.getImageData(0, 0, 1, 1); return 'readable'; } catch (error) { return error.name; } })(); const exported = (() => { try { canvas.toDataURL(); return 'readable'; } catch (error) { return error.name; } })(); return [source.complete, source.naturalWidth, readback, exported]; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!([true, 2, "SecurityError", "SecurityError"])
    );

    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    image_server.await.unwrap();
}
#[test]
fn native_svg_strokes_replay_bounded_rect_and_ellipse_geometry() {
    let document = NativeDocument::parse(
        "<div style='width:32px;height:24px'><svg width='28' height='20'><rect id='rect' x='2' y='2' width='8' height='6' fill='red' stroke='black' stroke-width='2'></rect><circle id='circle' cx='20' cy='10' r='4' fill='none' stroke='green' stroke-width='2'></circle><ellipse id='current' cx='25' cy='5' rx='2' ry='2' fill='none' stroke='currentColor' style='color:blue;stroke-width:2px'></ellipse></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let rect = document.resolve_target("id=rect").unwrap();
    let circle = document.resolve_target("id=circle").unwrap();
    let current = document.resolve_target("id=current").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();

    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgStroke {
                node_id,
                shape: NativeSvgStrokeShape::Rect,
                rect: shape,
                width,
                color,
                ..
            } if *node_id == rect
                && *shape == NativeRect { x: 2, y: 2, width: 8, height: 6 }
                && *width == 2
                && *color == NativeColor::BLACK
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgStroke {
                node_id,
                shape: NativeSvgStrokeShape::Ellipse,
                width,
                color,
                ..
            } if *node_id == circle
                && *width == 2
                && *color == NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgStroke { node_id, width, color, .. }
                if *node_id == current
                    && *width == 2
                    && *color == NativeColor {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    }
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(2, 2), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(4, 4), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(20, 6), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(20, 10), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(25, 5), Some([0, 0, 255, 255]));
}
#[test]
fn native_svg_straight_paths_share_relative_geometry_and_fill_stroke_paint() {
    let document = NativeDocument::parse(
        "<div style='width:24px;height:16px'><svg width='20' height='12'><path id='path' d='M2 2 h8 v6 l-8 0 z' fill='blue' stroke='green' stroke-width='1'></path></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let path = document.resolve_target("id=path").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(path).unwrap(),
        NativeRect {
            x: 2,
            y: 2,
            width: 9,
            height: 7,
        }
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPathFill {
                node_id,
                subpaths,
                color,
                ..
            } if *node_id == path
                && subpaths.len() == 1
                && subpaths[0].closed
                && subpaths[0].points.len() == 4
                && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPathStroke {
                node_id,
                subpaths,
                width,
                color,
                ..
            } if *node_id == path
                && subpaths.len() == 1
                && subpaths[0].closed
                && *width == 1
                && *color == NativeColor { red: 0, green: 128, blue: 0, alpha: 255 }
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 2), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(5, 4), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(5, 8), Some([0, 128, 0, 255]));
    assert_eq!(layout.hit_test(5, 4).unwrap(), Some(path));
}
#[test]
fn native_svg_curve_paths_flatten_quadratic_and_cubic_segments() {
    let document = NativeDocument::parse(
        "<div style='width:24px;height:16px'><svg width='24' height='12'><path id='cubic' d='M2 8 C2 2 10 2 10 8 Z' fill='blue' stroke='green' stroke-width='1'></path><path id='quadratic' d='M12 8 Q16 2 20 8 Z' fill='blue' stroke='green' stroke-width='1'></path></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let cubic = document.resolve_target("id=cubic").unwrap();
    let quadratic = document.resolve_target("id=quadratic").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert!(layout.box_for(cubic).unwrap().width >= 9);
    assert!(layout.box_for(quadratic).unwrap().width >= 9);

    let list = document.display_list(viewport).unwrap();
    for node_id in [cubic, quadratic] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::SvgPathFill { node_id: command_node, subpaths, .. }
                    if *command_node == node_id
                        && subpaths.len() == 1
                        && subpaths[0].closed
                        && subpaths[0].points.len() > 8
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::SvgPathStroke { node_id: command_node, subpaths, .. }
                    if *command_node == node_id
                        && subpaths.len() == 1
                        && subpaths[0].closed
                        && subpaths[0].points.len() > 8
            )
        }));
    }

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 6), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(6, 8), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(16, 6), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(16, 8), Some([0, 128, 0, 255]));
    assert_eq!(layout.hit_test(5, 6).unwrap(), Some(cubic));
    assert_eq!(layout.hit_test(16, 6).unwrap(), Some(quadratic));
}
#[test]
fn native_svg_smooth_and_arc_paths_share_bounded_geometry() {
    let document = NativeDocument::parse(
        "<div style='width:40px;height:24px'><svg width='36' height='20'><path id='smooth' d='M2 10 C2 4 6 4 8 10 S14 16 16 10 Q12 2 8 10 T2 10 Z' fill='blue' stroke='green' stroke-width='1'></path><path id='arc' d='M24 10 A5 5 0 1 1 30 10 A5 5 0 1 1 24 10 Z' fill='blue' stroke='green' stroke-width='1'></path></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let smooth = document.resolve_target("id=smooth").unwrap();
    let arc = document.resolve_target("id=arc").unwrap();
    let viewport = Viewport {
        width: 40,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert!(layout.box_for(smooth).unwrap().width >= 15);
    assert!(layout.box_for(arc).unwrap().width >= 7);

    let list = document.display_list(viewport).unwrap();
    for (node_id, minimum_points) in [(smooth, 48), (arc, 16)] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::SvgPathFill {
                    node_id: command_node,
                    subpaths,
                    color,
                    ..
                } if *command_node == node_id
                    && subpaths.len() == 1
                    && subpaths[0].closed
                    && subpaths[0].points.len() >= minimum_points
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::SvgPathStroke {
                    node_id: command_node,
                    subpaths,
                    width,
                    color,
                    ..
                } if *command_node == node_id
                    && subpaths.len() == 1
                    && subpaths[0].closed
                    && subpaths[0].points.len() >= minimum_points
                    && *width == 1
                    && *color == NativeColor { red: 0, green: 128, blue: 0, alpha: 255 }
            )
        }));
    }

    let surface = list.rasterize().unwrap();
    assert!(
        surface
            .rgba()
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 0, 255, 255])
    );
    assert!(
        surface
            .rgba()
            .chunks_exact(4)
            .any(|pixel| pixel == [0, 128, 0, 255])
    );
    assert_eq!(layout.hit_test(5, 8).unwrap(), Some(smooth));
    assert_eq!(layout.hit_test(27, 10).unwrap(), Some(arc));
}
#[test]
fn native_svg_transforms_share_nested_layout_paint_and_hit_geometry() {
    let document = NativeDocument::parse(
        "<div style='width:48px;height:24px'><svg width='40' height='20'><g transform='translate(10 4)'><rect id='rect' x='1' y='1' width='4' height='3' fill='blue' stroke='green' stroke-width='1'></rect><path id='path' d='M20 2 L24 2 L24 6 Z' transform='rotate(90 22 4)' fill='red' stroke='black' stroke-width='1'></path></g></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let rect = document.resolve_target("id=rect").unwrap();
    let path = document.resolve_target("id=path").unwrap();
    let viewport = Viewport {
        width: 48,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(rect).unwrap(),
        NativeRect {
            x: 11,
            y: 5,
            width: 5,
            height: 4,
        }
    );
    assert_eq!(
        layout.box_for(path).unwrap(),
        NativeRect {
            x: 30,
            y: 6,
            width: 5,
            height: 5,
        }
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolygonFill { node_id, rect: command_rect, points, color, .. }
                if *node_id == rect
                    && *command_rect == NativeRect { x: 11, y: 5, width: 5, height: 4 }
                    && points == &vec![
                        NativePoint { x: 11, y: 5 },
                        NativePoint { x: 15, y: 5 },
                        NativePoint { x: 15, y: 8 },
                        NativePoint { x: 11, y: 8 },
                    ]
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolyline { node_id, rect: command_rect, points, closed, color, .. }
                if *node_id == rect
                    && *command_rect == NativeRect { x: 11, y: 5, width: 5, height: 4 }
                    && *closed
                    && points == &vec![
                        NativePoint { x: 11, y: 5 },
                        NativePoint { x: 15, y: 5 },
                        NativePoint { x: 15, y: 8 },
                        NativePoint { x: 11, y: 8 },
                    ]
                    && *color == NativeColor { red: 0, green: 128, blue: 0, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPathFill { node_id, rect: command_rect, subpaths, color, .. }
                if *node_id == path
                    && *command_rect == NativeRect { x: 30, y: 6, width: 5, height: 5 }
                    && subpaths.len() == 1
                    && subpaths[0].closed
                    && subpaths[0].points == vec![
                        NativePoint { x: 34, y: 6 },
                        NativePoint { x: 34, y: 10 },
                        NativePoint { x: 30, y: 10 },
                    ]
                    && *color == NativeColor { red: 255, green: 0, blue: 0, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPathStroke { node_id, rect: command_rect, subpaths, width, color, .. }
                if *node_id == path
                    && *command_rect == NativeRect { x: 30, y: 6, width: 5, height: 5 }
                    && subpaths.len() == 1
                    && subpaths[0].closed
                    && subpaths[0].points == vec![
                        NativePoint { x: 34, y: 6 },
                        NativePoint { x: 34, y: 10 },
                        NativePoint { x: 30, y: 10 },
                    ]
                    && *width == 1
                    && *color == NativeColor { red: 0, green: 0, blue: 0, alpha: 255 }
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(12, 6), Some([0, 0, 255, 255]));
    assert!(
        surface
            .rgba()
            .chunks_exact(4)
            .any(|pixel| pixel == [255, 0, 0, 255])
    );
    assert_eq!(layout.hit_test(12, 6).unwrap(), Some(rect));
    assert_eq!(layout.hit_test(33, 8).unwrap(), Some(path));
}
#[test]
fn native_svg_transform_variants_bound_shared_geometry() {
    let document = NativeDocument::parse(
        "<div style='width:64px;height:24px'><svg width='60' height='20'><rect id='scale' x='1' y='1' width='2' height='2' transform='scale(2 3)' fill='blue'></rect><polygon id='matrix' points='1,1 3,1 3,3' transform='matrix(1 0 0 1 8 2)' fill='red'></polygon><line id='skew-x' x1='1' y1='2' x2='5' y2='2' transform='skewX(45)' stroke='black' fill='none'></line><line id='skew-y' x1='1' y1='2' x2='1' y2='5' transform='skewY(45)' stroke='black' fill='none'></line></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let scale = document.resolve_target("id=scale").unwrap();
    let matrix = document.resolve_target("id=matrix").unwrap();
    let skew_x = document.resolve_target("id=skew-x").unwrap();
    let skew_y = document.resolve_target("id=skew-y").unwrap();
    let viewport = Viewport {
        width: 64,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(scale).unwrap(),
        NativeRect {
            x: 2,
            y: 3,
            width: 5,
            height: 7,
        }
    );
    assert_eq!(
        layout.box_for(matrix).unwrap(),
        NativeRect {
            x: 9,
            y: 3,
            width: 3,
            height: 3,
        }
    );
    assert_eq!(
        layout.box_for(skew_x).unwrap(),
        NativeRect {
            x: 3,
            y: 2,
            width: 5,
            height: 1,
        }
    );
    assert_eq!(
        layout.box_for(skew_y).unwrap(),
        NativeRect {
            x: 1,
            y: 3,
            width: 1,
            height: 4,
        }
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolygonFill { node_id, points, color, .. }
                if *node_id == scale
                    && points == &vec![
                        NativePoint { x: 2, y: 3 },
                        NativePoint { x: 6, y: 3 },
                        NativePoint { x: 6, y: 9 },
                        NativePoint { x: 2, y: 9 },
                    ]
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolyline { node_id, points, color, .. }
                if *node_id == skew_x
                    && points == &vec![
                        NativePoint { x: 3, y: 2 },
                        NativePoint { x: 7, y: 2 },
                    ]
                    && *color == NativeColor { red: 0, green: 0, blue: 0, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolyline { node_id, points, color, .. }
                if *node_id == skew_y
                    && points == &vec![
                        NativePoint { x: 1, y: 3 },
                        NativePoint { x: 1, y: 6 },
                    ]
                    && *color == NativeColor { red: 0, green: 0, blue: 0, alpha: 255 }
        )
    }));
}
#[test]
fn native_svg_viewport_clips_layout_paint_and_hit_testing() {
    let document = NativeDocument::parse(
        "<div style='width:24px;height:16px'><svg width='10' height='10' viewBox='0 0 10 10'><rect id='shape' x='0' y='0' width='20' height='10' fill='blue'></rect></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let shape = document.resolve_target("id=shape").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(shape).unwrap(),
        NativeRect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        }
    );
    assert_eq!(
        layout.viewport_rect_for(shape),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        })
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, clip: Some(clip), .. }
                if *node_id == shape
                    && *rect == NativeRect { x: 0, y: 0, width: 20, height: 10 }
                    && *clip == NativeRect { x: 0, y: 0, width: 10, height: 10 }
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 5), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(10, 5), Some([255, 255, 255, 255]));
    assert_eq!(layout.hit_test(5, 5).unwrap(), Some(shape));
    assert_ne!(layout.hit_test(10, 5).unwrap(), Some(shape));
}
#[test]
fn native_css_background_images_repeat_and_crop_tiles() {
    let source = native_test_png_data_url();
    let markup = format!(
        "<div id='surface' style='width:6px;height:4px;background-image:url(\"{source}\")'></div>"
    );
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
    let surface_id = document.resolve_target("id=surface").unwrap();
    let list = document
        .display_list(Viewport {
            width: 6,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let images = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::Image {
                node_id,
                rect,
                source_rect,
                source_width,
                source_height,
                ..
            } if *node_id == surface_id => {
                Some((*rect, *source_rect, *source_width, *source_height))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(images.len(), 6);
    assert!(images.iter().all(|(rect, source_rect, width, height)| {
        *rect
            == NativeRect {
                x: rect.x,
                y: rect.y,
                width: 2,
                height: 2,
            }
            && *source_rect
                == NativeRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                }
            && *width == 2
            && *height == 2
    }));
    let painted = list.rasterize().unwrap();
    assert_eq!(painted.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(painted.pixel(2, 0), Some([255, 0, 0, 255]));
    assert_eq!(painted.pixel(4, 3), Some([0, 0, 255, 255]));
}
#[tokio::test]
async fn native_animated_gif_data_images_paint_multiple_frames() {
    let source = format!(
        "data:image/gif;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_animated_gif_bytes())
    );
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://animated-gif-image/page")
        .with_fixture(
            "fixture://animated-gif-image/page",
            format!("<img id='image' src='{source}' style='width:1px;height:1px'>"),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let image_pixels = |engine: &NativeEngine| {
        engine
            .display_list()
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
                _ => None,
            })
            .unwrap()
    };
    let initial = image_pixels(&engine);
    let mut changed = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        if image_pixels(&engine) != initial {
            changed = true;
            break;
        }
    }
    assert!(changed, "animated GIF did not advance to another frame");
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_animated_webp_data_images_paint_multiple_frames() {
    let source = format!(
        "data:image/webp;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_animated_webp_bytes())
    );
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://animated-webp-image/page")
        .with_fixture(
            "fixture://animated-webp-image/page",
            format!("<img id='image' src='{source}' style='width:1px;height:1px'>"),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let image_pixels = |engine: &NativeEngine| {
        engine
            .display_list()
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
                _ => None,
            })
            .unwrap()
    };
    let initial = image_pixels(&engine);
    let mut changed = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        if image_pixels(&engine) != initial {
            changed = true;
            break;
        }
    }
    assert!(changed, "animated WebP did not advance to another frame");
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_animated_apng_data_images_paint_multiple_frames() {
    let source = format!(
        "data:image/apng;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(native_test_animated_apng_bytes())
    );
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://animated-apng-image/page")
        .with_fixture(
            "fixture://animated-apng-image/page",
            format!("<img id='image' src='{source}' style='width:1px;height:1px'>"),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let image_pixels = |engine: &NativeEngine| {
        engine
            .display_list()
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
                _ => None,
            })
            .unwrap()
    };
    let initial = image_pixels(&engine);
    let mut changed = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        if image_pixels(&engine) != initial {
            changed = true;
            break;
        }
    }
    assert!(changed, "animated APNG did not advance to another frame");
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_images_expose_accessible_role_and_alt_name() {
    let source = native_test_png_data_url();
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://image-semantics/page")
        .with_fixture(
            "fixture://image-semantics/page",
            format!(
                "<img id='logo' alt='Glass logo' src='{source}'><img id='decorative' alt='' src='{source}'>"
            ),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let nodes = engine.semantic_nodes().unwrap();
    let logo = nodes
        .iter()
        .find(|node| node.role == "img" && node.name == "Glass logo");
    assert_eq!(logo.map(|node| node.role.as_str()), Some("img"));
    assert_eq!(logo.map(|node| node.name.as_str()), Some("Glass logo"));
    let decorative = nodes
        .iter()
        .find(|node| node.role == "img" && node.name.is_empty());
    assert_eq!(decorative.map(|node| node.role.as_str()), Some("img"));
    assert_eq!(decorative.map(|node| node.name.as_str()), Some(""));
    engine.close_async().await.unwrap();
}
#[test]
fn native_image_map_hit_testing_normalizes_shapes_and_uses_displayed_content_pixels() {
    let document = NativeDocument::parse(
        "<img id='mapped' usemap='prefix#places' style='display:block;width:120px;height:80px;padding:4px;border:2px solid black'><map name='places'><area id='dead' coords='11,11,19,19'><area id='reversed' shape='rect' coords='40,40,30,30' href='fixture://rectangle' alt='Reversed rectangle'><area id='circle' shape='circle' coords='60 20 10 99' href='fixture://circle' alt='Circle'><area id='polygon' shape='poly' coords='70,10,90,10,80,30,999' href='fixture://polygon' alt='Polygon'><area id='invalid-shape' shape='ellipse' coords='100,40,119,60' href='fixture://invalid-shape' alt='Invalid shape defaults to rectangle'><area id='parsed-list' shape='rect' coords='ignored,50;50,60;60' href='fixture://parsed-list' alt='Recovered coordinate list'><area id='malformed-number' shape='rect' coords='0,0,12oops,10' href='fixture://malformed-number' alt='Malformed coordinate becomes zero'><area id='empty-rect' shape='rect' coords='0,0,10'><area id='empty-circle' shape='circle' coords='5,5,0'><area id='empty-fallback' coords='0,0,10,10' href='fixture://empty-fallback' alt='Empty shape fallback'><area id='default' shape='default' coords='not numeric' href='fixture://default' alt='Default area'></map><img id='unmapped' style='display:block;width:40px;height:30px'>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 300,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let image_id = find_element_with_attribute(&document, "img", "id", "mapped");
    let image_rect = layout.viewport_rect_for(image_id).unwrap();
    let image_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == image_id)
        .unwrap();
    let inset_x = image_box.content_rect.x - image_box.rect.x;
    let inset_y = image_box.content_rect.y - image_box.rect.y;
    let hit_local = |x: u32, y: u32| {
        document
            .hit_test(
                viewport,
                i64::from(image_rect.x + inset_x + x),
                i64::from(image_rect.y + inset_y + y),
            )
            .unwrap()
            .unwrap()
    };
    let node_id = |id: &str| find_element_with_attribute(&document, "area", "id", id);

    assert_eq!(hit_local(15, 15), node_id("dead"));
    assert_eq!(hit_local(35, 35), node_id("reversed"));
    assert_eq!(hit_local(60, 20), node_id("circle"));
    assert_eq!(hit_local(80, 20), node_id("polygon"));
    assert_eq!(hit_local(110, 50), node_id("invalid-shape"));
    assert_eq!(hit_local(55, 55), node_id("parsed-list"));
    assert_eq!(hit_local(0, 5), node_id("malformed-number"));
    assert_eq!(hit_local(5, 5), node_id("empty-fallback"));
    assert_eq!(hit_local(110, 70), node_id("default"));

    let border_hit = document
        .hit_test(
            viewport,
            i64::from(image_rect.x + 2),
            i64::from(image_rect.y + 2),
        )
        .unwrap();
    assert_eq!(border_hit, Some(image_id));
    let unmapped = find_element_with_attribute(&document, "img", "id", "unmapped");
    let unmapped_rect = layout.viewport_rect_for(unmapped).unwrap();
    assert_eq!(
        document
            .hit_test(
                viewport,
                i64::from(unmapped_rect.x + 5),
                i64::from(unmapped_rect.y + 5),
            )
            .unwrap(),
        Some(unmapped)
    );

    let semantic_links = document
        .semantic_nodes()
        .into_iter()
        .filter(|node| node.role == "link")
        .collect::<Vec<_>>();
    assert!(
        semantic_links
            .iter()
            .any(|node| node.node_id == node_id("reversed") && node.name == "Reversed rectangle")
    );
    assert!(
        !semantic_links
            .iter()
            .any(|node| node.node_id == node_id("dead"))
    );
}
#[tokio::test]
async fn native_image_map_hit_testing_tracks_live_dom_mutations() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://live-image-map")
        .with_fixture(
            "fixture://live-image-map",
            "<img id='image' usemap='#first-map' style='display:block;width:100px;height:100px'><map name='first-map'><area id='first' shape='rect' coords='0,0,100,100' href='fixture://first' alt='First'><area id='second' shape='rect' coords='0,0,100,100' href='fixture://second' alt='Second'></map><map name='second-map'><area id='third' shape='rect' coords='40,40,60,60' href='fixture://third' alt='Third'></map>",
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let image_id = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.tag_name == "img")
        .unwrap()
        .node_id;
    let image_rect = engine
        .layout()
        .unwrap()
        .viewport_rect_for(image_id)
        .unwrap();
    let point = (i64::from(image_rect.x + 50), i64::from(image_rect.y + 50));
    let area_id = |engine: &NativeEngine, name: &str| {
        engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .find(|node| node.tag_name == "area" && node.name == name)
            .unwrap()
            .node_id
    };
    assert_eq!(
        engine.hit_test(point.0, point.1).unwrap(),
        Some(area_id(&engine, "First"))
    );

    engine
        .evaluate_async(
            "(() => { const map = document.querySelector('map[name=first-map]'); map.insertBefore(document.getElementById('second'), document.getElementById('first')); })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.hit_test(point.0, point.1).unwrap(),
        Some(area_id(&engine, "Second"))
    );

    engine
        .evaluate_async("document.getElementById('image').setAttribute('usemap', '#second-map')")
        .await
        .unwrap();
    assert_eq!(
        engine.hit_test(point.0, point.1).unwrap(),
        Some(area_id(&engine, "Third"))
    );

    engine
        .evaluate_async("document.getElementById('third').setAttribute('coords', '0,0,10,10')")
        .await
        .unwrap();
    assert_eq!(engine.hit_test(point.0, point.1).unwrap(), Some(image_id));

    engine
        .evaluate_async(
            "(() => { document.querySelector('map[name=second-map]').setAttribute('name', 'renamed-map'); document.getElementById('image').setAttribute('usemap', '#renamed-map'); document.getElementById('third').setAttribute('coords', '40,40,60,60'); })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.hit_test(point.0, point.1).unwrap(),
        Some(area_id(&engine, "Third"))
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_images_select_srcset_candidates_from_viewport_and_density() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://image-srcset/page")
        .with_viewport(Viewport {
            width: 600,
            height: 400,
            device_scale_factor_milli: 2000,
        })
        .with_fixture(
            "fixture://image-srcset/page",
            "<img id='width' src='/fallback.png' srcset='/small.png 400w, /large.png 800w' sizes='600px'><img id='density' src='/fallback-density.png' srcset='/one.png 1x, /two.png 2x, /three.png 3x'>",
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const width = document.getElementById('width'); const density = document.getElementById('density'); return [width.srcset, width.sizes, width.currentSrc, density.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "/small.png 400w, /large.png 800w",
            "600px",
            "fixture://image-srcset/large.png",
            "fixture://image-srcset/two.png"
        ])
    );

    engine.close_async().await.unwrap();
}

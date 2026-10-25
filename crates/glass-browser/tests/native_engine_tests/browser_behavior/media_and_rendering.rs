use super::*;

#[tokio::test]
async fn native_video_reports_decoder_absence_and_does_not_claim_playback() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://video-capability.test/")
        .with_fixture(
            "fixture://video-capability.test/",
            "<video id='video' controls><source src='clip.mp4' type='video/mp4'></video>",
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const video = document.getElementById('video'); return [video.canPlayType('video/mp4'), video.readyState, video.currentTime]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["", 0, 0])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "globalThis.nativeVideoPlayResult = 'pending'; document.getElementById('video').play().then(() => { nativeVideoPlayResult = 'playing'; }, error => { nativeVideoPlayResult = error.name; }); true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.nativeVideoPlayResult")
            .await
            .unwrap(),
        serde_json::json!("NotSupportedError")
    );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn native_offscreen_canvas_transfer_preserves_worker_raster_and_detaches_source() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 8,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://offscreen-transfer-page",
            "<html><body><canvas id='placeholder' width='2' height='1' style='display:block;width:2px;height:1px'></canvas></body></html>",
        )
        .unwrap()
        .with_fixture(
            "fixture://offscreen-transfer-worker",
            r#"self.onmessage = event => {
                const canvas = event.data.canvas;
                const context = canvas.getContext('2d');
                context.fillStyle = '#00ff00';
                context.fillRect(0, 0, canvas.width, canvas.height);
                postMessage({
                    canvas,
                    identity: [canvas instanceof OffscreenCanvas, canvas.width, canvas.height],
                    pixel: Array.from(context.getImageData(0, 0, 1, 1).data),
                }, [canvas]);
            }"#,
        )
        .unwrap()
        .with_initial_url("fixture://offscreen-transfer-page");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                r#"(() => {
                    const localCanvas = new OffscreenCanvas(2, 1);
                    const localContext = localCanvas.getContext('2d');
                    localContext.fillStyle = '#ff0000';
                    localContext.fillRect(0, 0, 2, 1);
                    const localClone = structuredClone(localCanvas, { transfer: [localCanvas] });
                    let localSourceError = '';
                    try { localCanvas.width; } catch (error) { localSourceError = error.name; }
                    globalThis.localOffscreenTransfer = [
                        localSourceError,
                        localClone instanceof OffscreenCanvas,
                        localClone.width,
                        localClone.height,
                        Array.from(localClone.getContext('2d').getImageData(0, 0, 1, 1).data),
                    ];
                    const placeholder = document.getElementById('placeholder');
                    globalThis.offscreen = placeholder.transferControlToOffscreen();
                    globalThis.workerMessages = [];
                    globalThis.worker = new Worker('fixture://offscreen-transfer-worker');
                    worker.onmessage = event => {
                        const output = document.createElement('canvas');
                        output.width = 2;
                        output.height = 1;
                        document.body.appendChild(output);
                        const outputContext = output.getContext('2d');
                        outputContext.drawImage(event.data.canvas, 0, 0);
                        workerMessages.push({
                            identity: event.data.identity,
                            pixel: event.data.pixel,
                            outputPixel: Array.from(outputContext.getImageData(0, 0, 1, 1).data),
                        });
                    };
                    globalThis.sourceState = null;
                    globalThis.placeholder = placeholder;
                    return [offscreen instanceof OffscreenCanvas, offscreen.width, offscreen.height];
                })()"#,
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, 1])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "worker.postMessage({ canvas: offscreen }, [offscreen]); let offscreenError = ''; try { offscreen.width; } catch (error) { offscreenError = error.name; } let placeholderError = ''; try { placeholder.getContext('2d'); } catch (error) { placeholderError = error.name; } sourceState = [offscreenError, placeholderError]; true",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async("({ localOffscreenTransfer, sourceState, workerMessages })")
            .await
            .unwrap(),
        serde_json::json!({
            "localOffscreenTransfer": ["InvalidStateError", true, 2, 1, [255, 0, 0, 255]],
            "sourceState": ["InvalidStateError", "InvalidStateError"],
            "workerMessages": [{
                "identity": [true, 2, 1],
                "pixel": [0, 255, 0, 255],
                "outputPixel": [0, 255, 0, 255],
            }],
        })
    );
    engine.close_async().await.unwrap();
}
#[test]
fn native_real_font_metrics_feed_layout_and_glyph_paint_when_available() {
    let limits = NativeEngineLimits::default();
    let viewport = Viewport {
        width: 320,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let narrow = NativeDocument::parse(
        "<style>#text { font-family: sans-serif; font-size: 12px; }</style><div id='text'>Glass</div>",
        &limits,
    )
    .unwrap();
    let wide = NativeDocument::parse(
        "<style>#text { font-family: sans-serif; font-size: 24px; }</style><div id='text'>Glass</div>",
        &limits,
    )
    .unwrap();

    let glyph_width = |document: &NativeDocument| {
        document
            .display_list(viewport)
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::GlyphRun { run, .. } => Some(run.width),
                NativeDisplayCommand::TextRun { .. }
                | NativeDisplayCommand::BeginOpacityGroup { .. }
                | NativeDisplayCommand::Clear { .. }
                | NativeDisplayCommand::EndOpacityGroup { .. }
                | NativeDisplayCommand::FillRect { .. }
                | NativeDisplayCommand::SetNestedScrollOffset { .. }
                | NativeDisplayCommand::Image { .. }
                | NativeDisplayCommand::SvgImage { .. }
                | NativeDisplayCommand::SvgStroke { .. }
                | NativeDisplayCommand::SvgPolygonFill { .. }
                | NativeDisplayCommand::SvgPolyline { .. }
                | NativeDisplayCommand::SvgPathFill { .. }
                | NativeDisplayCommand::SvgPathStroke { .. }
                | NativeDisplayCommand::BorderRect { .. } => None,
            })
    };

    let narrow_width = glyph_width(&narrow);
    let wide_width = glyph_width(&wide);
    if let (Some(narrow_width), Some(wide_width)) = (narrow_width, wide_width) {
        assert!(wide_width > narrow_width);
        assert!(wide_width <= 320);
        let surface = wide.rasterize(viewport).unwrap();
        assert!(
            (0..120).any(|y| {
                (0..wide_width.min(320)).any(|x| {
                    surface
                        .pixel(x, y)
                        .is_some_and(|pixel| pixel != [255, 255, 255, 255])
                })
            }),
            "real font run should produce non-white coverage"
        );
    }
}
#[test]
fn native_side_specific_borders_feed_box_model_and_paint() {
    let document = NativeDocument::parse(
        "<div id='card' style='width:20px;height:10px;padding:1px;border-top:1px solid red;border-right:2px solid green;border-bottom:3px solid blue;border-left:4px solid black'>Card</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 20,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(card),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 28,
            height: 16,
        })
    );
    let card_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == card)
        .unwrap();
    assert_eq!(
        card_box.content_rect,
        NativeRect {
            x: 5,
            y: 2,
            width: 20,
            height: 10,
        }
    );

    let list = document.display_list(viewport).unwrap();
    let border = list
        .commands
        .iter()
        .find_map(|command| match command {
            NativeDisplayCommand::BorderRect { borders, .. } => Some(*borders),
            _ => None,
        })
        .unwrap();
    assert_eq!(border.top.width, 1);
    assert_eq!(border.right.width, 2);
    assert_eq!(border.bottom.width, 3);
    assert_eq!(border.left.width, 4);
    assert_eq!(border.top.color, NativeColor::RED);
    assert_eq!(
        border.right.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255
        }
    );
    assert_eq!(
        border.bottom.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255
        }
    );
    assert_eq!(border.left.color, NativeColor::BLACK);

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(10, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(27, 10), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(10, 14), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(0, 5), Some([0, 0, 0, 255]));
}
#[test]
fn native_png_capture_matches_complete_logical_pixel_golden() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 8,
            height: 6,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://pixel-golden",
            native_html_without_body_margin(
                "<div style='width:4px;height:3px;background-color:red'></div>",
            ),
        )
        .unwrap()
        .with_initial_url("fixture://pixel-golden");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let white = [255, 255, 255, 255];
    let red = [255, 0, 0, 255];
    let expected = [
        [red, red, red, red, white, white, white, white],
        [red, red, red, red, white, white, white, white],
        [red, red, red, red, white, white, white, white],
        [white, white, white, white, white, white, white, white],
        [white, white, white, white, white, white, white, white],
        [white, white, white, white, white, white, white, white],
    ];
    let expected_rgba: Vec<u8> = expected.into_iter().flatten().flatten().collect();

    let before = engine.revision();
    let surface = engine.rasterize().unwrap();
    assert_eq!(surface.rgba(), expected_rgba.as_slice());

    let capture = engine.capture_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (8, 6));
    assert_eq!(&decoded[..expected_rgba.len()], expected_rgba.as_slice());
    assert_eq!(engine.revision(), before);
}
#[test]
fn native_inline_svg_shapes_share_layout_paint_and_hit_test_geometry() {
    let document = NativeDocument::parse(
        "<!doctype html><div id='host' style='width:40px;height:30px'><svg id='icon' width='24' height='18'><rect id='rect' x='2' y='3' width='8' height='6' fill='red'></rect><circle id='circle' cx='18' cy='9' r='4' style='fill:blue'></circle></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let svg = document.resolve_target("id=icon").unwrap();
    let rect = document.resolve_target("id=rect").unwrap();
    let circle = document.resolve_target("id=circle").unwrap();
    assert!(
        document.diagnostics().is_empty(),
        "unexpected SVG diagnostics: {:?}",
        document.diagnostics()
    );
    assert_eq!(
        document.node(svg).unwrap().namespace_uri(),
        Some("http://www.w3.org/2000/svg")
    );
    assert_eq!(
        document.node(rect).unwrap().namespace_uri(),
        Some("http://www.w3.org/2000/svg")
    );
    let viewport = Viewport {
        width: 40,
        height: 30,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(svg).unwrap(),
        NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 18,
        }
    );
    assert_eq!(
        layout.box_for(rect).unwrap(),
        NativeRect {
            x: 2,
            y: 3,
            width: 8,
            height: 6,
        }
    );
    assert_eq!(
        layout.box_for(circle).unwrap(),
        NativeRect {
            x: 14,
            y: 5,
            width: 8,
            height: 8,
        }
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect: shape, color, .. }
                if *node_id == rect
                    && *shape == NativeRect { x: 2, y: 3, width: 8, height: 6 }
                    && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == circle && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(3, 4), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(18, 9), Some([0, 0, 255, 255]));
    assert_eq!(layout.hit_test(3, 4).unwrap(), Some(rect));
    assert_eq!(layout.hit_test(18, 9).unwrap(), Some(circle));
}
#[test]
fn native_inline_png_images_share_intrinsic_layout_paint_and_capture() {
    let source = native_test_png_data_url();
    let markup = format!("<div style='width:16px'><img id='image' src='{source}' width='8'></div>");
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
    let image = document.resolve_target("id=image").unwrap();
    let viewport = Viewport {
        width: 16,
        height: 8,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(image).unwrap(),
        NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        }
    );

    let list = document.display_list(viewport).unwrap();
    let command = list
        .commands
        .iter()
        .find(|command| matches!(command, NativeDisplayCommand::Image { node_id, .. } if *node_id == image))
        .unwrap();
    assert!(matches!(
        command,
        NativeDisplayCommand::Image {
            rect,
            source_width: 2,
            source_height: 2,
            pixels,
            clip: None,
            ..
        } if *rect == NativeRect { x: 0, y: 0, width: 8, height: 8 } && pixels.len() == 16
    ));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 1), Some([127, 255, 127, 255]));
    assert_eq!(surface.pixel(1, 5), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(5, 5), Some([255, 255, 255, 255]));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(image));
    assert_eq!(layout.hit_test(5, 5).unwrap(), Some(image));

    let png = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(png));
    let reader = decoder.read_info().unwrap();
    assert_eq!(reader.info().width, 16);
    assert_eq!(reader.info().height, 8);
}
#[test]
fn native_inline_png_background_images_share_css_paint_and_capture() {
    let source = native_test_png_data_url();
    let markup = format!(
        "<div id='surface' style='width:8px;height:8px;background-image:url(\"{source}\");background-repeat:no-repeat;background-size:100% 100%'></div>"
    );
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
    let surface_id = document.resolve_target("id=surface").unwrap();
    let viewport = Viewport {
        width: 8,
        height: 8,
        device_scale_factor_milli: 1000,
    };

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image {
                node_id,
                rect,
                source_width: 2,
                source_height: 2,
                pixels,
                clip: None,
                ..
            } if *node_id == surface_id
                && *rect == NativeRect { x: 0, y: 0, width: 8, height: 8 }
                && pixels.len() == 16
        )
    }));
    let painted = list.rasterize().unwrap();
    assert_eq!(painted.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(painted.pixel(5, 1), Some([127, 255, 127, 255]));
    assert_eq!(painted.pixel(1, 5), Some([0, 0, 255, 255]));
    assert_eq!(painted.pixel(5, 5), Some([255, 255, 255, 255]));
}
#[test]
fn native_stylesheet_background_geometry_cascades_into_paint() {
    let source = native_test_png_data_url();
    let markup = format!(
        "<style>#surface {{ background-image:url(\"{source}\"); background-repeat:repeat-x; background-size:3px 2px; background-position:100% 0; }}</style><div id='surface' style='width:7px;height:4px'></div>"
    );
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
    let surface_id = document.resolve_target("id=surface").unwrap();
    let list = document
        .display_list(Viewport {
            width: 7,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let images = list
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                NativeDisplayCommand::Image { node_id, .. } if *node_id == surface_id
            )
        })
        .count();
    assert_eq!(images, 3);
    let painted = list.rasterize().unwrap();
    assert_eq!(painted.pixel(4, 0), Some([255, 0, 0, 255]));
    assert_eq!(painted.pixel(0, 2), Some([255, 255, 255, 255]));
}
#[tokio::test]
async fn native_jpeg_data_images_expose_intrinsic_dimensions_and_paint() {
    let source = native_test_small_jpeg_data_url();
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://jpeg-image/page")
        .with_fixture(
            "fixture://jpeg-image/page",
            format!("<img id='image' src='{source}' style='width:2px;height:2px'>"),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, 2, source])
    );

    let document = NativeDocument::parse(
        &format!("<img id='image' src='{source}' style='width:2px;height:2px'>"),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let image_id = document.resolve_target("id=image").unwrap();
    let list = document.display_list(Viewport {
        width: 2,
        height: 2,
        device_scale_factor_milli: 1000,
    });
    let list = list.unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image {
                node_id,
                source_width: 2,
                source_height: 2,
                pixels,
                ..
            } if *node_id == image_id && pixels.len() == 2 * 2 * 4
        )
    }));
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_gif_data_images_expose_intrinsic_dimensions_and_paint() {
    let source = native_test_small_gif_data_url();
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://gif-image/page")
        .with_fixture(
            "fixture://gif-image/page",
            format!("<img id='image' src='{source}' style='width:2px;height:2px'>"),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 1, 1, source])
    );

    let document = NativeDocument::parse(
        &format!("<img id='image' src='{source}' style='width:2px;height:2px'>"),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let image_id = document.resolve_target("id=image").unwrap();
    let list = document
        .display_list(Viewport {
            width: 2,
            height: 2,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image {
                node_id,
                source_width: 1,
                source_height: 1,
                pixels,
                ..
            } if *node_id == image_id && pixels.len() == 4
        )
    }));
    engine.close_async().await.unwrap();
}
#[test]
fn native_descendant_styles_flow_through_visibility_layout_and_paint() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://descendant-selectors",
            "<style>main .panel button { display: none; } main .panel .shown { display: block; width: 8px; height: 4px; background-color: red; } main button { display: block; }</style><main><section class='panel'><button id='hidden'>Hidden</button><button id='shown' class='shown'>Shown</button></section></main>",
        )
        .unwrap()
        .with_initial_url("fixture://descendant-selectors");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    assert_eq!(engine.snapshot().unwrap().visible_text, "Shown");
    let hidden = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Hidden")
        .expect("hidden descendant node");
    let shown = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Shown")
        .expect("shown descendant node");
    assert!(hidden.hidden);
    assert!(!shown.hidden);

    let layout = engine.layout().unwrap();
    assert_eq!(layout.box_for(hidden.node_id), None);
    let shown_rect = layout.box_for(shown.node_id).expect("shown layout box");
    let list = engine.display_list().unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect {
                node_id,
                color,
                ..
            } if *node_id == shown.node_id && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        engine
            .rasterize()
            .unwrap()
            .pixel(shown_rect.x, shown_rect.y),
        Some([255, 0, 0, 255])
    );
}

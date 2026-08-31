#![cfg(feature = "native-engine")]

use glass_browser::browser::native_backend::NATIVE_ENGINE_BACKEND_ID;
use glass_browser::browser::native_engine::{
    MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES, MAX_NATIVE_DIAGNOSTICS, NativeAction, NativeBorderRadius,
    NativeBorderStyle, NativeColor, NativeDiagnosticCode, NativeDiagnosticSource,
    NativeDisplayCommand, NativeDocument, NativeEngine, NativeEngineConfig, NativeEngineError,
    NativeEngineLimits, NativeEventKind, NativeLifecycleState, NativePoint, NativeRect,
    NativeSurface, Viewport,
};
use glass_browser::browser_backend::{
    ActionRequest, BROWSER_BACKEND_SCHEMA_VERSION, BackendSelectionRequest,
    BrowserBackendDispatcher, BrowserCapability, CaptureFormat, CaptureRequest, CertificationLevel,
    EffectsRequest, EvidenceLevel, EvidenceRequest, NavigationRequest, ScriptRequest,
    SemanticAction, SupportLevel,
};
use glass_browser::{BackendFactory, BrowserRuntime, BrowserRuntimeSession, NativeEngineBackend};
use std::io::Cursor;

#[tokio::test]
async fn native_runtime_session_uses_explicit_local_constructor() {
    let session =
        BrowserRuntimeSession::connect_native(NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3ERuntime%3C%2Ftitle%3E%3Cp%3ENative%20session%3C%2Fp%3E",
        ))
        .await
        .unwrap();
    assert_eq!(session.runtime(), BrowserRuntime::Native);
    assert_eq!(
        session.profile().identity.backend_id,
        NATIVE_ENGINE_BACKEND_ID
    );

    let evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(
        evidence.url,
        "data:text/html,%3Ctitle%3ERuntime%3C%2Ftitle%3E%3Cp%3ENative%20session%3C%2Fp%3E"
    );
    assert_eq!(evidence.title, "Runtime");
    assert_eq!(evidence.visible_text, "Native session");

    let script_error = session.script("1 + 1").await.unwrap_err().to_string();
    assert!(script_error.contains("capability"));
    session.close().await.unwrap();
}

#[test]
fn native_layout_is_deterministic_and_uses_bounded_pixel_dimensions() {
    let document = NativeDocument::parse(
        "<style>button { width: 120px; height: 32px; }</style><main><button id='first'>First</button><button id='second' style='display:block;width:140px'>Second</button><p id='below'>Below</p></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let below = document.resolve_target("id=below").unwrap();
    let expected_first = NativeRect {
        x: 0,
        y: 0,
        width: 120,
        height: 32,
    };
    let expected_second = NativeRect {
        x: 0,
        y: 32,
        width: 140,
        height: 32,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.revision, 1);
    assert_eq!(layout.box_for(first), Some(expected_first));
    assert_eq!(layout.box_for(second), Some(expected_second));
    assert_eq!(
        layout.box_for(below),
        Some(NativeRect {
            x: 0,
            y: 64,
            width: 320,
            height: 20,
        })
    );
    assert_eq!(layout, document.layout(viewport).unwrap());
}

#[test]
fn native_layout_excludes_hidden_boxes_and_hit_testing_is_viewport_bound() {
    let document = NativeDocument::parse(
        "<style>.gone { display:none } .also-gone { visibility:hidden }</style><main><button class='gone' id='hidden'>Hidden</button><button class='also-gone' id='also-hidden'>Also hidden</button><button id='shown'>Shown</button></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(document.resolve_target("id=hidden").unwrap()),
        None
    );
    assert_eq!(
        layout.box_for(document.resolve_target("id=also-hidden").unwrap()),
        None
    );
    let shown = document.resolve_target("id=shown").unwrap();
    let shown_rect = layout.box_for(shown).unwrap();
    assert_eq!(
        layout
            .hit_test(i64::from(shown_rect.x), i64::from(shown_rect.y))
            .unwrap(),
        Some(shown)
    );
    assert!(layout.hit_test(-1, 0).is_err());
    assert!(layout.hit_test(320, 0).is_err());
}

#[test]
fn native_vertical_scroll_maps_layout_hit_testing_and_raster_output() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 80,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://scroll",
            "<div id='top' style='display:block;height:24px;background-color:red'>Top</div><button id='bottom' style='display:block;height:24px;background-color:blue'>Bottom</button>",
        )
        .unwrap()
        .with_initial_url("fixture://scroll");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let bottom = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Bottom")
        .unwrap();
    let initial = engine.layout().unwrap();
    assert_eq!(initial.scroll_offset, NativePoint { x: 0, y: 0 });
    assert!(initial.content_height > initial.viewport.height);
    assert_eq!(initial.viewport_rect_for(bottom.node_id), None);

    let scrolled = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(scrolled.accepted);
    assert_eq!(scrolled.revision, 2);
    let layout = engine.layout().unwrap();
    assert_eq!(layout.scroll_offset, layout.max_scroll_offset());
    assert_eq!(layout.viewport_rect_for(bottom.node_id).unwrap().y, 0);
    assert_eq!(engine.hit_test(1, 1).unwrap(), Some(bottom.node_id));
    assert_eq!(
        engine
            .effects_since(1)
            .unwrap()
            .effects
            .iter()
            .map(|effect| effect.kind)
            .collect::<Vec<_>>(),
        vec![NativeEventKind::Scroll]
    );

    let list = engine.display_list().unwrap();
    assert_eq!(list.scroll_offset, layout.scroll_offset);
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([0, 0, 255, 255]));

    let clicked = engine
        .action(NativeAction::Click {
            target: "id=bottom".into(),
        })
        .unwrap();
    assert!(clicked.accepted);
    assert_eq!(clicked.revision, 3);

    let no_op = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(!no_op.accepted);
    assert_eq!(no_op.revision, 3);
    assert_eq!(engine.scroll_offset(), layout.scroll_offset);

    let horizontal = engine
        .action(NativeAction::Scroll {
            delta_x: 1,
            delta_y: 0,
        })
        .unwrap_err();
    assert!(matches!(
        horizontal,
        NativeEngineError::InvalidConfiguration { field, .. }
            if field == "native scroll action"
    ));
    assert_eq!(engine.revision(), 3);
}

#[test]
fn native_display_list_is_revisioned_deterministic_and_visibility_aware() {
    let document = NativeDocument::parse(
        "<style>#card { background-color: #102030; color: rgb(1, 2, 3); }</style><main><div id='card'>Hello <span>child</span></div><div id='hidden' style='display:none;background-color:red'>Hidden</div></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();

    let list = document.display_list(viewport).unwrap();
    assert_eq!(list.revision, document.revision());
    assert_eq!(list.viewport, viewport);
    assert!(matches!(
        list.commands.first(),
        Some(NativeDisplayCommand::Clear { color }) if *color == NativeColor::WHITE
    ));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == card
                    && *color == NativeColor { red: 16, green: 32, blue: 48, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { text, color, .. }
                if text == "Hello"
                    && *color == NativeColor { red: 1, green: 2, blue: 3, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { text, color, .. }
                if text == "child"
                    && *color == NativeColor { red: 1, green: 2, blue: 3, alpha: 255 }
        )
    }));
    assert!(!list.commands.iter().any(|command| match command {
        NativeDisplayCommand::FillRect { node_id, .. }
        | NativeDisplayCommand::BorderRect { node_id, .. }
        | NativeDisplayCommand::TextRun { node_id, .. } => *node_id == hidden,
        NativeDisplayCommand::Clear { .. } => false,
    }));
    assert_eq!(list, document.display_list(viewport).unwrap());
}

#[test]
fn native_display_list_inherits_text_color_and_preserves_transparent_override() {
    let document = NativeDocument::parse(
        "<style>#parent { color: blue; }</style><div id='parent'><span id='child'>Child</span><span id='transparent' style='color:transparent'>Clear</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let child = document.resolve_target("id=child").unwrap();
    let transparent = document.resolve_target("id=transparent").unwrap();
    let list = document.display_list(viewport).unwrap();

    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == child
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == transparent
                    && *color == NativeColor { red: 0, green: 0, blue: 0, alpha: 0 }
        )
    }));
}

#[test]
fn native_display_list_emits_uniform_border_after_box_model_layout() {
    let document = NativeDocument::parse(
        "<div id='card' style='width: 8px; height: 8px; border: 2px solid blue'>Card</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 12,
        height: 12,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(card),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 12,
            height: 12,
        })
    );

    let list = document.display_list(viewport).unwrap();
    let border_index = list
        .commands
        .iter()
        .position(|command| matches!(command, NativeDisplayCommand::BorderRect { .. }))
        .unwrap();
    let text_index = list
        .commands
        .iter()
        .position(|command| matches!(command, NativeDisplayCommand::TextRun { .. }))
        .unwrap();
    assert!(border_index < text_index);
    assert!(matches!(
        &list.commands[border_index],
        NativeDisplayCommand::BorderRect {
            node_id,
            rect,
            radius,
            borders,
            clip: None,
        } if *node_id == card
            && *rect == NativeRect { x: 0, y: 0, width: 12, height: 12 }
            && *radius == NativeBorderRadius::default()
            && borders.top.width == 2
            && borders.right.width == 2
            && borders.bottom.width == 2
            && borders.left.width == 2
            && borders.top.color == (NativeColor { red: 0, green: 0, blue: 255, alpha: 255 })
            && borders.right.color == borders.top.color
            && borders.bottom.color == borders.top.color
            && borders.left.color == borders.top.color
    ));
    assert!(matches!(
        &list.commands[text_index],
        NativeDisplayCommand::TextRun {
            node_id,
            origin,
            ..
        } if *node_id == card && *origin == glass_browser::browser::native_engine::NativePoint { x: 2, y: 2 }
    ));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 0), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(1, 1), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(2, 2), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(2, 5), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(11, 11), Some([0, 0, 255, 255]));
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
fn native_pattern_border_styles_feed_display_list_and_surface() {
    let document = NativeDocument::parse(
        "<div id='card' style='width:8px;height:6px;border-top:1px dashed red;border-right:1px dotted green;border-bottom:1px solid blue'></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 12,
        height: 8,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let border = list
        .commands
        .iter()
        .find_map(|command| match command {
            NativeDisplayCommand::BorderRect { borders, .. } => Some(*borders),
            _ => None,
        })
        .unwrap();
    assert_eq!(border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(border.right.style, NativeBorderStyle::Dotted);
    assert_eq!(border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(border.left.width, 0);

    let mut scrolled_list = list.clone();
    let unscrolled = list.rasterize().unwrap();
    assert_eq!(unscrolled.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(2, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(3, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(4, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(5, 0), Some([255, 0, 0, 255]));
    assert_eq!(unscrolled.pixel(8, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(8, 1), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(8, 2), Some([0, 128, 0, 255]));

    scrolled_list.scroll_offset = NativePoint { x: 0, y: 1 };
    let scrolled = scrolled_list.rasterize().unwrap();
    assert_eq!(scrolled.pixel(8, 0), unscrolled.pixel(8, 1));
    assert_eq!(scrolled.pixel(8, 1), unscrolled.pixel(8, 2));
}

#[test]
fn native_border_radius_feeds_layout_hit_testing_and_rounded_replay() {
    let document = NativeDocument::parse(
        "<div id='card' style='width:12px;height:10px;border:2px solid blue;background-color:red;border-radius:4px 2px 3px 1px'>Card</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 20,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let layout = document.layout(viewport).unwrap();
    let card_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == card)
        .unwrap();
    assert_eq!(
        card_box.border_radius,
        NativeBorderRadius {
            top_left: 4,
            top_right: 2,
            bottom_right: 3,
            bottom_left: 1,
        }
    );
    assert_eq!(layout.hit_test(0, 0).unwrap(), None);
    assert_eq!(layout.hit_test(4, 4).unwrap(), Some(card));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, radius, .. }
                if *node_id == card && *radius == card_box.border_radius
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BorderRect { node_id, radius, .. }
                if *node_id == card && *radius == card_box.border_radius
        )
    }));

    let unscrolled = list.rasterize().unwrap();
    assert_eq!(unscrolled.pixel(0, 0), Some([255, 255, 255, 255]));
    assert_eq!(unscrolled.pixel(3, 0), Some([0, 0, 255, 255]));
    assert_eq!(unscrolled.pixel(4, 4), Some([255, 0, 0, 255]));

    let mut scrolled_list = list;
    scrolled_list.scroll_offset = NativePoint { x: 0, y: 1 };
    let scrolled = scrolled_list.rasterize().unwrap();
    assert_eq!(scrolled.pixel(3, 0), unscrolled.pixel(3, 1));
}

#[test]
fn native_inline_boxes_wrap_before_layout_materializes_geometry() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:16px'><span id='first' style='display:inline;width:6px;height:4px;padding:1px;margin:1px;background-color:red'>A</span><span id='second' style='display:inline;width:6px;height:4px;padding:1px;margin:1px;background-color:blue'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(container),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 40,
        })
    );
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 1,
            y: 1,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 1,
            y: 21,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(layout.hit_test(2, 2).unwrap(), Some(first));
    assert_eq!(layout.hit_test(2, 22).unwrap(), Some(second));

    let list = document.display_list(viewport).unwrap();
    let text_origins = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id, origin, ..
            } => Some((*node_id, *origin)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(text_origins.contains(&(first, NativePoint { x: 2, y: 2 })));
    assert!(text_origins.contains(&(second, NativePoint { x: 2, y: 22 })));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == first
                    && *rect == NativeRect { x: 1, y: 1, width: 8, height: 6 }
        )
    }));
}

#[test]
fn native_fixed_line_height_controls_wrapped_flow_and_preserves_explicit_height() {
    let document = NativeDocument::parse(
        "<style>#container { width:12px; line-height:28px; } #first { line-height:32px; }</style><div id='container'><span id='first' style='display:inline;width:8px'>A</span><span id='second' style='display:inline;width:8px;height:4px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(container),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 12,
            height: 60,
        })
    );
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 32,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 0,
            y: 32,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(first));
    assert_eq!(layout.hit_test(1, 33).unwrap(), Some(second));

    let list = document.display_list(viewport).unwrap();
    let text_origins = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id, origin, ..
            } => Some((*node_id, *origin)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(text_origins.contains(&(first, NativePoint { x: 0, y: 0 })));
    assert!(text_origins.contains(&(second, NativePoint { x: 0, y: 32 })));
}

#[test]
fn native_text_fragments_follow_inline_flow_and_source_order() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:24px'>AB<span id='middle' style='display:inline;width:8px;color:red'>C</span>DE</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let middle = document.resolve_target("id=middle").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(container),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 40,
        })
    );
    assert_eq!(layout.text_runs.len(), 3);
    assert_eq!(layout.text_runs[0].node_id, container);
    assert_eq!(layout.text_runs[0].origin, NativePoint { x: 0, y: 0 });
    assert_eq!(layout.text_runs[0].text, "AB");
    assert_eq!(layout.text_runs[1].node_id, middle);
    assert_eq!(layout.text_runs[1].origin, NativePoint { x: 16, y: 0 });
    assert_eq!(layout.text_runs[1].text, "C");
    assert_eq!(layout.text_runs[2].node_id, container);
    assert_eq!(layout.text_runs[2].origin, NativePoint { x: 0, y: 20 });
    assert_eq!(layout.text_runs[2].text, "DE");

    let list = document.display_list(viewport).unwrap();
    let runs = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                truncated,
                ..
            } => Some((*node_id, *origin, text.as_str(), *truncated)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![
            (container, NativePoint { x: 0, y: 0 }, "AB", false),
            (middle, NativePoint { x: 16, y: 0 }, "C", false),
            (container, NativePoint { x: 0, y: 20 }, "DE", false),
        ]
    );

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(17, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 20), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_fragments_use_collapsed_bounded_text_for_width_and_paint() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:16px'> A   B </div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 40);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "A", false),
            (NativePoint { x: 0, y: 20 }, "B", false),
        ]
    );
}

#[test]
fn native_text_fragments_wrap_words_and_split_only_wide_words() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:24px'>AB CD EFGHI</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 80);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "AB"),
            (NativePoint { x: 0, y: 20 }, "CD"),
            (NativePoint { x: 0, y: 40 }, "EFG"),
            (NativePoint { x: 0, y: 60 }, "HI"),
        ]
    );
}

#[test]
fn native_text_fragments_preserve_only_source_whitespace_boundaries() {
    let adjacent = NativeDocument::parse(
        "<div id='container' style='width:16px'>A<span style='display:contents'></span>B</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let container = adjacent.resolve_target("id=container").unwrap();
    let layout = adjacent.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 20);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "A"),
            (NativePoint { x: 8, y: 0 }, "B"),
        ]
    );

    let separated = NativeDocument::parse(
        "<div id='container' style='width:40px'>A <span id='middle' style='display:inline'>B</span> C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let container = separated.resolve_target("id=container").unwrap();
    let middle = separated.resolve_target("id=middle").unwrap();
    let layout = separated
        .layout(Viewport {
            width: 48,
            height: 48,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 20);
    assert_eq!(
        layout.box_for(middle),
        Some(NativeRect {
            x: 16,
            y: 0,
            width: 8,
            height: 20,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (container, NativePoint { x: 0, y: 0 }, "A"),
            (container, NativePoint { x: 8, y: 0 }, " "),
            (middle, NativePoint { x: 16, y: 0 }, "B"),
            (container, NativePoint { x: 24, y: 0 }, " C"),
        ]
    );
}

#[test]
fn native_br_elements_create_bounded_hard_breaks_without_layout_nodes() {
    let document = NativeDocument::parse(
        "<div id='flow'><br id='leading'>A<br id='middle-a'><br id='middle-b'>B<br id='trailing'></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 100,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let leading = document.resolve_target("id=leading").unwrap();
    let middle_a = document.resolve_target("id=middle-a").unwrap();
    let middle_b = document.resolve_target("id=middle-b").unwrap();
    let trailing = document.resolve_target("id=trailing").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(leading), None);
    assert_eq!(layout.box_for(middle_a), None);
    assert_eq!(layout.box_for(middle_b), None);
    assert_eq!(layout.box_for(trailing), None);
    assert_eq!(layout.box_for(flow).unwrap().height, 100);
    assert_eq!(layout.text_runs.len(), 2);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 20 }, "A"),
            (flow, NativePoint { x: 0, y: 60 }, "B"),
        ]
    );

    let list = document.display_list(viewport).unwrap();
    let runs = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                ..
            } => Some((*node_id, *origin, text.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![
            (flow, NativePoint { x: 0, y: 20 }, "A"),
            (flow, NativePoint { x: 0, y: 60 }, "B"),
        ]
    );
    assert!(list.commands.iter().all(|command| match command {
        NativeDisplayCommand::FillRect { node_id, .. }
        | NativeDisplayCommand::BorderRect { node_id, .. }
        | NativeDisplayCommand::TextRun { node_id, .. } => {
            *node_id != leading
                && *node_id != middle_a
                && *node_id != middle_b
                && *node_id != trailing
        }
        NativeDisplayCommand::Clear { .. } => true,
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 20), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 60), Some([0, 0, 0, 255]));

    let hidden = NativeDocument::parse(
        "<div id='flow'>A<br id='hidden' hidden>B<br id='aria-hidden' aria-hidden='true'>C<br id='display-none' style='display:none'>D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let hidden_flow = hidden.resolve_target("id=flow").unwrap();
    let hidden_break = hidden.resolve_target("id=hidden").unwrap();
    let aria_hidden_break = hidden.resolve_target("id=aria-hidden").unwrap();
    let display_none_break = hidden.resolve_target("id=display-none").unwrap();
    let hidden_layout = hidden.layout(viewport).unwrap();
    assert_eq!(hidden_layout.box_for(hidden_break), None);
    assert_eq!(hidden_layout.box_for(aria_hidden_break), None);
    assert_eq!(hidden_layout.box_for(display_none_break), None);
    assert_eq!(hidden_layout.box_for(hidden_flow).unwrap().height, 20);
    assert!(hidden_layout.text_runs.iter().all(|run| run.origin.y == 0));
}

#[test]
fn native_pre_line_preserves_bounded_source_breaks_and_inherits_through_contents() {
    let document = NativeDocument::parse(
        "<style>#flow { white-space: pre-line; line-height: 24px; }</style><div id='flow'>\nA\n\n<span id='contents' style='display:contents'>B\r\nC</span>\n</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 144,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let contents = document.resolve_target("id=contents").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(contents), None);
    assert_eq!(layout.box_for(flow).unwrap().height, 144);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 24 }, "A"),
            (contents, NativePoint { x: 0, y: 72 }, "B"),
            (contents, NativePoint { x: 0, y: 96 }, "C"),
        ]
    );

    let normal = NativeDocument::parse(
        "<div id='normal'>A\nB</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let normal_flow = normal.resolve_target("id=normal").unwrap();
    let normal_layout = normal
        .layout(Viewport {
            width: 32,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(normal_layout.box_for(normal_flow).unwrap().height, 20);
    assert_eq!(normal_layout.text_runs.len(), 2);
    assert_eq!(
        normal_layout
            .text_runs
            .iter()
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "A"),
            (NativePoint { x: 8, y: 0 }, " B"),
        ]
    );
}

#[test]
fn native_pre_preserves_bounded_whitespace_without_soft_wrap() {
    let document = NativeDocument::parse(
        "<style>#flow { white-space: pre; line-height: 24px; width: 32px; }</style><div id='flow'> A  B\n<span id='inline' style='display:inline'>\tCDEFG</span>\r\nD </div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(flow).unwrap().height, 72);
    assert_eq!(
        layout.box_for(inline),
        Some(NativeRect {
            x: 0,
            y: 24,
            width: 32,
            height: 20,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 0 }, " A  B"),
            (inline, NativePoint { x: 0, y: 24 }, "\tCDEFG"),
            (flow, NativePoint { x: 0, y: 48 }, "D "),
        ]
    );
    assert_eq!(
        document.visible_text(100),
        ("A B CDEFG D".to_owned(), false)
    );

    let list = document.display_list(viewport).unwrap();
    assert_eq!(
        list.commands
            .iter()
            .filter_map(|command| match command {
                NativeDisplayCommand::TextRun { origin, text, .. } =>
                    Some((*origin, text.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, " A  B"),
            (NativePoint { x: 0, y: 24 }, "\tCDEFG"),
            (NativePoint { x: 0, y: 48 }, "D "),
        ]
    );
}

#[test]
fn native_pre_wrap_preserves_whitespace_and_soft_wraps_at_fixed_cell_capacity() {
    let document = NativeDocument::parse(
        "<style>#flow { white-space: pre-wrap; line-height: 24px; width: 32px; }</style><div id='flow'>AB C D\n<span id='inline' style='display:inline; width:32px'> \tDE</span>\r\nF </div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 128,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(flow).unwrap().height, 96);
    assert_eq!(
        layout.box_for(inline),
        Some(NativeRect {
            x: 0,
            y: 48,
            width: 32,
            height: 20,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (flow, NativePoint { x: 0, y: 0 }, "AB C"),
            (flow, NativePoint { x: 0, y: 24 }, " D"),
            (inline, NativePoint { x: 0, y: 48 }, " \tDE"),
            (flow, NativePoint { x: 0, y: 72 }, "F "),
        ]
    );
    assert_eq!(
        document.visible_text(100),
        ("AB C D DE F".to_owned(), false)
    );

    let list = document.display_list(viewport).unwrap();
    assert_eq!(
        list.commands
            .iter()
            .filter_map(|command| match command {
                NativeDisplayCommand::TextRun { origin, text, .. } =>
                    Some((*origin, text.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "AB C"),
            (NativePoint { x: 0, y: 24 }, " D"),
            (NativePoint { x: 0, y: 48 }, " \tDE"),
            (NativePoint { x: 0, y: 72 }, "F "),
        ]
    );
}

#[test]
fn native_text_boundary_separator_drops_when_inline_item_wraps() {
    let document = NativeDocument::parse(
        "<div id='container' style='width:16px'>A <span id='middle' style='display:inline'>B</span> C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let container = document.resolve_target("id=container").unwrap();
    let middle = document.resolve_target("id=middle").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(container).unwrap().height, 60);
    assert_eq!(
        layout.box_for(middle),
        Some(NativeRect {
            x: 0,
            y: 20,
            width: 8,
            height: 20,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| (run.node_id, run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (container, NativePoint { x: 0, y: 0 }, "A"),
            (middle, NativePoint { x: 0, y: 20 }, "B"),
            (container, NativePoint { x: 0, y: 40 }, "C"),
        ]
    );
}

#[test]
fn native_box_model_lays_out_content_padding_border_and_margin() {
    let document = NativeDocument::parse(
        "<style>#outer { width: 20px; height: 10px; padding: 2px; border: 1px solid red; } #child { display: block; width: 4px; height: 4px; margin: 3px; } #fixed { width: 20px; height: 10px; padding: 2px; border: 1px solid blue; box-sizing: border-box; }</style><div id='outer'><div id='child'>A</div></div><div id='fixed'>B</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let outer = document.resolve_target("id=outer").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fixed = document.resolve_target("id=fixed").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(outer),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 26,
            height: 16,
        })
    );
    let outer_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == outer)
        .unwrap();
    assert_eq!(
        outer_box.content_rect,
        NativeRect {
            x: 3,
            y: 3,
            width: 20,
            height: 10,
        }
    );
    assert_eq!(
        layout.box_for(child),
        Some(NativeRect {
            x: 6,
            y: 6,
            width: 4,
            height: 4,
        })
    );
    assert_eq!(
        layout.box_for(fixed),
        Some(NativeRect {
            x: 0,
            y: 16,
            width: 20,
            height: 10,
        })
    );
    let fixed_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == fixed)
        .unwrap();
    assert_eq!(
        fixed_box.content_rect,
        NativeRect {
            x: 3,
            y: 19,
            width: 14,
            height: 4,
        }
    );
}

#[test]
fn native_physical_box_edges_feed_content_origins_and_flow_margins() {
    let document = NativeDocument::parse(
        "<div id='outer' style='width:20px;height:10px;padding:1px 2px 3px 4px'><div id='child' style='display:block;width:4px;height:4px;margin:1px 2px 3px 4px'>A</div></div><div id='next' style='display:block;width:8px;height:4px;margin:2px 3px 4px 5px'>B</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let outer = document.resolve_target("id=outer").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let next = document.resolve_target("id=next").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(outer),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 26,
            height: 14,
        })
    );
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == outer)
            .unwrap()
            .content_rect,
        NativeRect {
            x: 4,
            y: 1,
            width: 20,
            height: 10,
        }
    );
    assert_eq!(
        layout.box_for(child),
        Some(NativeRect {
            x: 8,
            y: 2,
            width: 4,
            height: 4,
        })
    );
    assert_eq!(
        layout.box_for(next),
        Some(NativeRect {
            x: 5,
            y: 16,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == child)
            .map(|run| (run.origin, run.text.as_str())),
        Some((NativePoint { x: 8, y: 2 }, "A"))
    );
}

#[test]
fn native_paint_clips_overflow_hidden_descendants() {
    let document = NativeDocument::parse(
        "<style>#clip { overflow: hidden; width: 10px; height: 10px; } #child { display: block; width: 20px; height: 20px; background-color: red; }</style><div id='clip'><div id='child'>Child</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let clip = document.resolve_target("id=clip").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let list = document.display_list(viewport).unwrap();
    let child_fill = list.commands.iter().find_map(|command| match command {
        NativeDisplayCommand::FillRect {
            node_id,
            rect,
            clip: paint_clip,
            ..
        } if *node_id == child => Some((*rect, *paint_clip)),
        _ => None,
    });
    assert_eq!(
        child_fill,
        Some((
            NativeRect {
                x: 0,
                y: 0,
                width: 10,
                height: 20,
            },
            Some(NativeRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            })
        ))
    );
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, .. } if *node_id == child
        )
    }));
    assert!(document.layout(viewport).unwrap().box_for(clip).is_some());

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 5), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 15), Some([255, 255, 255, 255]));
}

#[test]
fn native_overflow_hidden_clips_hit_testing_and_viewport_projection() {
    let document = NativeDocument::parse(
        "<style>#outer { overflow: hidden; width: 12px; height: 12px; } #clip { overflow: hidden; width: 10px; height: 10px; } #child { display: block; width: 10px; height: 20px; }</style><div id='outer'><div id='clip'><div id='child'>Child</div></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let outer = document.resolve_target("id=outer").unwrap();
    let clip = document.resolve_target("id=clip").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(outer).unwrap().height, 12);
    assert_eq!(layout.box_for(clip).unwrap().height, 10);
    assert_eq!(
        layout.box_for(child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 10,
            height: 20,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        })
    );
    assert_eq!(layout.hit_test(5, 5).unwrap(), Some(child));
    assert_eq!(layout.hit_test(5, 10).unwrap(), Some(outer));
}

#[test]
fn native_overflow_clip_reuses_rectangular_clip_without_scroll() {
    let document = NativeDocument::parse(
        "<style>#clip { overflow: clip; width: 10px; height: 10px; } #child { display: block; width: 20px; height: 20px; background-color: red; }</style><div id='clip'><div id='child'>Child</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let clip = document.resolve_target("id=clip").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "overflow"
    }));
    assert_eq!(layout.scroll_offset, NativePoint { x: 0, y: 0 });
    assert_eq!(layout.max_scroll_offset(), NativePoint { x: 0, y: 0 });
    assert_eq!(layout.box_for(clip).unwrap().height, 10);
    assert_eq!(
        layout.viewport_rect_for(child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        })
    );
    assert_eq!(layout.hit_test(5, 5).unwrap(), Some(child));
    assert_eq!(layout.hit_test(5, 10).unwrap(), None);

    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 5), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 15), Some([255, 255, 255, 255]));
}

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
            "<style>#card { background-color: red; color: blue; }</style><div id='card'>A</div>",
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
    assert_eq!(surface.pixel(31, 19), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 255, 255]));
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
            "<div style='width:4px;height:3px;background-color:red'></div>",
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

#[tokio::test]
async fn native_backend_captures_png_without_mutating_revision_and_denies_other_formats() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 8,
            height: 8,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://capture",
            "<style>#card { width: 8px; height: 8px; background-color: red; }</style><div id='card'>A</div>",
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
    assert_eq!((output.width, output.height), (8, 8));
    assert_eq!(&decoded[..4], &[255, 0, 0, 255]);

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
        .unwrap_err();
    assert!(matches!(
        jpeg,
        glass_browser::browser_backend::BrowserBackendError::UnsupportedOperation {
            operation, ..
        } if operation == "capture"
    ));
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn native_backend_dispatches_vertical_scroll_into_capture() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 80,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://scroll-dispatch",
            "<div style='display:block;height:24px;background-color:red'>Top</div><button id='bottom' style='display:block;height:24px;background-color:blue'>Bottom</button>",
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
    let pixel_index = output.line_size + 4;
    assert_eq!(&decoded[pixel_index..pixel_index + 4], &[0, 0, 255, 255]);

    let horizontal = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Scroll {
                delta_x: 1,
                delta_y: 0,
            },
        })
        .await
        .unwrap_err();
    assert!(matches!(
        horizontal,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "native scroll action"
    ));
    dispatcher.close().await.unwrap();
}

#[test]
fn native_point_click_hits_nested_content_and_preserves_state_on_rejection() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://layout",
            "<main><button id='save'><span>Save</span></button><p>Other</p></main>",
        )
        .unwrap()
        .with_initial_url("fixture://layout");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let button = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Save")
        .unwrap();
    let rect = engine.layout().unwrap().box_for(button.node_id).unwrap();
    let point_target = format!("point={},{}", rect.x + 1, rect.y + 1);
    let clicked = engine
        .action(NativeAction::Click {
            target: point_target,
        })
        .unwrap();
    assert_eq!(clicked.revision, 2);
    assert!(
        engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .find(|node| node.node_id == button.node_id)
            .unwrap()
            .focused
    );

    let rejected = engine
        .action(NativeAction::Click {
            target: "point=319,199".into(),
        })
        .unwrap_err();
    assert!(matches!(
        rejected,
        NativeEngineError::TargetNotActionable { .. }
    ));
    assert_eq!(engine.revision(), 2);
}

#[tokio::test]
async fn native_point_click_uses_the_real_backend_dispatcher() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://point-dispatch",
            "<button id='save'>Save</button>",
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

#[tokio::test]
async fn fixture_navigation_projects_through_the_real_backend_dispatcher() {
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
        CertificationLevel::Experimental
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

#[tokio::test]
async fn unsupported_resources_and_capabilities_fail_without_state_mutation() {
    let backend = NativeEngineBackend::new(NativeEngineConfig::default()).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let before = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();

    let network = dispatcher
        .navigate(NavigationRequest {
            url: "https://example.com".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        network,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "navigation URL"
    ));
    let after = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(after, before);

    let script = dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "1 + 1".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        script,
        glass_browser::browser_backend::BrowserBackendError::CapabilityUnavailable {
            capability: BrowserCapability::Script,
            actual: SupportLevel::Unavailable,
            declared: false,
            ..
        }
    ));
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn parse_failure_preserves_the_current_document_and_revision() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://broken", "<p title='unfinished>")
        .unwrap();
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

    let error = dispatcher
        .navigate(NavigationRequest {
            url: "fixture://broken".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "native HTML document"
    ));
    let after = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(after, before);
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn data_url_and_limits_are_bounded() {
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

#[tokio::test]
async fn semantic_actions_and_effects_use_the_backend_contract() {
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
            action: SemanticAction::KeyPress {
                key: "Enter".into(),
            },
        })
        .await
        .unwrap_err();
    assert!(matches!(
        keypress,
        glass_browser::browser_backend::BrowserBackendError::UnsupportedOperation {
            operation, ..
        } if operation == "action"
    ));

    let future = dispatcher
        .effects(EffectsRequest {
            context_id: "native-context".into(),
            since_revision: 4,
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
fn native_backend_requires_explicit_factory_selection() {
    let automatic = BackendSelectionRequest {
        schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
        glass_version: env!("CARGO_PKG_VERSION").into(),
        preferred_backend_id: None,
        browser_family: None,
        browser_version: None,
        required_capabilities: vec![],
        minimum_certification: CertificationLevel::Experimental,
    };
    let native = BackendFactory::native(NativeEngineConfig::default()).unwrap();
    assert!(BackendFactory::start(&automatic, vec![native]).is_err());

    let explicit = BackendSelectionRequest {
        preferred_backend_id: Some(NATIVE_ENGINE_BACKEND_ID.into()),
        ..automatic
    };
    let native = BackendFactory::native(NativeEngineConfig::default()).unwrap();
    let started = BackendFactory::start(&explicit, vec![native]).unwrap();
    assert_eq!(
        started.profile().identity.backend_id,
        NATIVE_ENGINE_BACKEND_ID
    );
}

#[test]
fn lifecycle_state_is_terminal_after_close() {
    let mut engine = glass_browser::NativeEngine::new(NativeEngineConfig::default()).unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::New);
    engine.initialize().unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::Running);
    engine.close().unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::Closed);
    assert!(engine.initialize().is_err());
    assert!(engine.close().is_err());
}

#[test]
fn semantic_dom_references_and_locators_are_revision_bound() {
    let document = NativeDocument::parse(
        "<label for='query'>Query</label><input id='query' type='search'><button id='go'>Go</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let nodes = document.semantic_nodes();
    let query = nodes.iter().find(|node| node.role == "textbox").unwrap();
    assert_eq!(query.name, "Query");
    assert_eq!(document.resolve_target("id=query"), Ok(query.node_id));
    assert_eq!(
        document.resolve_target("role=textbox[name=Query]"),
        Ok(query.node_id)
    );
    assert_eq!(
        query.reference,
        format!("ref=r1:n{}", query.node_id.index())
    );
    assert!(document.resolve_target(&query.reference).is_ok());
}

#[test]
fn native_actions_update_state_and_reject_unsafe_targets_before_mutation() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://controls",
            "<label for='name'>Name</label><input id='name' type='text'><input id='remember' type='checkbox'><input id='first' type='radio' name='choice'><input id='second' type='radio' name='choice' checked><input id='disabled' type='text' disabled><input id='readonly' type='text' readonly><button id='save'>Save</button><button id='other'>Other</button><p id='plain'>Plain</p><select id='country'><option id='one' value='one'>One</option><option id='two' value='two'>Two</option></select><select id='many' multiple><option id='many-one'>Many One</option></select>",
        )
        .unwrap()
        .with_initial_url("fixture://controls");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial_nodes = engine.semantic_nodes().unwrap();
    let remember_reference = initial_nodes
        .iter()
        .find(|node| node.role == "checkbox")
        .map(|node| node.reference.clone())
        .unwrap();
    assert_eq!(engine.revision(), 1);

    let click = engine
        .action(NativeAction::Click {
            target: "id=remember".into(),
        })
        .unwrap();
    assert_eq!(click.revision, 2);
    let after_click = engine.semantic_nodes().unwrap();
    let remember = after_click
        .iter()
        .find(|node| node.role == "checkbox")
        .unwrap();
    assert_eq!(remember.checked, Some(true));
    assert!(remember.focused);
    let click_effects = engine.effects_since(1).unwrap();
    assert_eq!(
        click_effects
            .effects
            .iter()
            .map(|effect| effect.kind)
            .collect::<Vec<_>>(),
        vec![
            NativeEventKind::Focus,
            NativeEventKind::Click,
            NativeEventKind::Change
        ]
    );

    let typed = engine
        .action(NativeAction::Type {
            target: "id=name".into(),
            text: "Glass".into(),
        })
        .unwrap();
    assert_eq!(typed.revision, 3);
    let after_type = engine.semantic_nodes().unwrap();
    let name = after_type.iter().find(|node| node.name == "Name").unwrap();
    let remember = after_type
        .iter()
        .find(|node| node.role == "checkbox")
        .unwrap();
    assert_eq!(name.empty, Some(false));
    assert!(name.focused);
    assert!(!remember.focused);

    engine
        .action(NativeAction::Click {
            target: "id=first".into(),
        })
        .unwrap();
    let radios = engine.semantic_nodes().unwrap();
    assert_eq!(
        radios
            .iter()
            .find(|node| node.role == "radio")
            .and_then(|node| node.checked),
        Some(true)
    );
    assert_eq!(
        radios
            .iter()
            .filter(|node| node.role == "radio")
            .nth(1)
            .and_then(|node| node.checked),
        Some(false)
    );

    let options = engine.semantic_nodes().unwrap();
    assert_eq!(
        options
            .iter()
            .find(|node| node.name == "One")
            .and_then(|node| node.selected),
        Some(true)
    );
    assert_eq!(
        options
            .iter()
            .find(|node| node.name == "Two")
            .and_then(|node| node.selected),
        Some(false)
    );
    assert_eq!(
        options
            .iter()
            .find(|node| node.role == "combobox" && node.tag_name == "select")
            .and_then(|node| node.empty),
        Some(false)
    );
    let selected = engine
        .action(NativeAction::Click {
            target: "id=two".into(),
        })
        .unwrap();
    assert_eq!(selected.revision, 5);
    let selected_options = engine.semantic_nodes().unwrap();
    assert_eq!(
        selected_options
            .iter()
            .find(|node| node.name == "One")
            .and_then(|node| node.selected),
        Some(false)
    );
    assert_eq!(
        selected_options
            .iter()
            .find(|node| node.name == "Two")
            .and_then(|node| node.selected),
        Some(true)
    );

    let revision_before_rejections = engine.revision();
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "role=button".into(),
        }),
        Err(glass_browser::NativeEngineError::AmbiguousTarget { matches: 2 })
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=plain".into(),
        }),
        Err(glass_browser::NativeEngineError::TargetNotActionable { .. })
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=many-one".into(),
        }),
        Err(glass_browser::NativeEngineError::TargetNotActionable { .. })
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: remember_reference,
        }),
        Err(glass_browser::NativeEngineError::DetachedTarget)
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=disabled".into(),
        }),
        Err(glass_browser::NativeEngineError::DisabledTarget)
    ));
    assert!(matches!(
        engine.action(NativeAction::Type {
            target: "id=readonly".into(),
            text: "nope".into(),
        }),
        Err(glass_browser::NativeEngineError::ReadOnlyTarget)
    ));
    assert_eq!(engine.revision(), revision_before_rejections);
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

#[test]
fn native_css_diagnostics_identify_unsupported_input_without_raw_echo() {
    let document = NativeDocument::parse(
        "<style>button:hover, main > button, #ok { color: red; width: 10%; display: flex; overflow: visible; white-space: break-spaces; custom-property: url(secret); broken; }</style><style>.unclosed { color: blue; </style><button id='ok' style='background-image: url(secret); padding: -1px'>OK</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    let diagnostics = document.diagnostics();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssSelector
            && diagnostic.detail == "pseudo-selector"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssSelector
            && diagnostic.detail == "selector-combinator"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            && diagnostic.detail == "custom-property"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            && diagnostic.detail == "background-image"
            && matches!(
                diagnostic.source,
                NativeDiagnosticSource::InlineStyle { .. }
            )
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "width"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "display"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "overflow"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "white-space"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "padding"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::MalformedCss
            && diagnostic.detail == "missing-colon"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::MalformedCss
            && diagnostic.detail == "unclosed-rule"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.source,
            NativeDiagnosticSource::Stylesheet { index: 0 }
        )
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.source,
            NativeDiagnosticSource::InlineStyle { .. }
        )
    }));
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.detail.len() <= MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES)
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.detail.contains("secret"))
    );
}

#[test]
fn native_css_diagnostics_are_bounded() {
    let mut source = String::from("<style>");
    for index in 0..MAX_NATIVE_DIAGNOSTICS.saturating_add(8) {
        source.push_str(&format!(".item{index} {{ unknown-{index}: value; }}"));
    }
    source.push_str("</style><div class='item0'>Item</div>");

    let document = NativeDocument::parse(&source, &NativeEngineLimits::default()).unwrap();
    assert_eq!(document.diagnostics().len(), MAX_NATIVE_DIAGNOSTICS);
    assert!(document.diagnostics_truncated());
}

#[test]
fn native_css_diagnostics_replace_atomically_with_navigation() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://diagnostic-bad",
            "<style>#card { width: 50%; }</style><div id='card'>Bad</div>",
        )
        .unwrap()
        .with_fixture("fixture://diagnostic-good", "<div id='card'>Good</div>")
        .unwrap()
        .with_initial_url("fixture://diagnostic-bad");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial = engine.diagnostics().unwrap();
    assert_eq!(initial.revision, 1);
    assert_eq!(initial.diagnostics.len(), 1);
    assert!(!initial.truncated);

    let failed = engine.navigate("fixture://missing");
    assert!(matches!(
        failed,
        Err(NativeEngineError::UnsupportedUrl { .. })
    ));
    let after_failure = engine.diagnostics().unwrap();
    assert_eq!(after_failure, initial);

    engine.navigate("fixture://diagnostic-good").unwrap();
    let after_success = engine.diagnostics().unwrap();
    assert_eq!(after_success.revision, 2);
    assert!(after_success.diagnostics.is_empty());
    assert!(!after_success.truncated);
}

#![cfg(feature = "native-engine")]

use glass_browser::browser::native_backend::NATIVE_ENGINE_BACKEND_ID;
use glass_browser::browser::native_engine::{
    MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES, MAX_NATIVE_DIAGNOSTICS, NativeAction, NativeBorderRadius,
    NativeBorderStyle, NativeColor, NativeDiagnosticCode, NativeDiagnosticSource,
    NativeDisplayCommand, NativeDocument, NativeEngine, NativeEngineConfig, NativeEngineError,
    NativeEngineLimits, NativeEventKind, NativeLifecycleState, NativeNodeId, NativePoint,
    NativeRect, NativeSurface, Viewport,
};
use glass_browser::browser_backend::{
    ActionRequest, BROWSER_BACKEND_SCHEMA_VERSION, BackendSelectionRequest,
    BrowserBackendDispatcher, BrowserCapability, CaptureFormat, CaptureRequest, CertificationLevel,
    EffectsRequest, EvidenceLevel, EvidenceRequest, NavigationRequest, ScriptRequest,
    SemanticAction, SupportLevel,
};
use glass_browser::{BackendFactory, BrowserRuntime, BrowserRuntimeSession, NativeEngineBackend};
use std::io::Cursor;

fn find_element_with_attribute(
    document: &NativeDocument,
    element_name: &str,
    attribute_name: &str,
    attribute_value: &str,
) -> NativeNodeId {
    let mut pending = vec![document.root()];
    while let Some(id) = pending.pop() {
        let node = document.node(id).unwrap();
        if node.element_name() == Some(element_name)
            && node.attribute(attribute_name) == Some(attribute_value)
        {
            return id;
        }
        pending.extend(node.children().iter().copied());
    }
    panic!("missing {element_name} element with {attribute_name}={attribute_value:?}");
}

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

    let horizontal_edge = engine
        .action(NativeAction::Scroll {
            delta_x: 1,
            delta_y: 0,
        })
        .unwrap();
    assert!(!horizontal_edge.accepted);
    assert_eq!(engine.revision(), 3);
}

#[test]
fn native_horizontal_scroll_projects_wide_preformatted_content_and_history() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 16,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://wide",
            "<pre id='wide' style='white-space:pre'>A A</pre><div id='below' style='display:block;height:40px'>Below</div><pre style='display:none;white-space:pre'>Hidden hidden hidden</pre><script>Invisible invisible invisible</script>",
        )
        .unwrap()
        .with_fixture("fixture://narrow", "<p>Narrow</p>")
        .unwrap()
        .with_initial_url("fixture://wide");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial = engine.layout().unwrap();
    let wide_text = initial
        .text_runs
        .iter()
        .find(|run| run.text == "A A")
        .unwrap();
    let wide = wide_text.node_id;
    assert_eq!(initial.content_width, 24);
    assert_eq!(initial.max_scroll_offset(), NativePoint { x: 8, y: 36 });
    assert_eq!(initial.scroll_offset, NativePoint { x: 0, y: 0 });
    assert_eq!(wide_text.origin, NativePoint { x: 0, y: 0 });
    assert!(!wide_text.truncated);
    assert_eq!(initial.viewport_rect_for(wide).unwrap().width, 16);

    let unscrolled = engine.rasterize().unwrap();
    assert_eq!(unscrolled.pixel(1, 0), Some([0, 0, 0, 255]));

    let horizontal = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: 0,
        })
        .unwrap();
    assert!(horizontal.accepted);
    assert_eq!(horizontal.revision, 2);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 8, y: 0 });
    let horizontal_layout = engine.layout().unwrap();
    assert_eq!(
        horizontal_layout.max_scroll_offset(),
        NativePoint { x: 8, y: 36 }
    );
    assert_eq!(
        horizontal_layout.viewport_rect_for(wide),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 20,
        })
    );
    assert_eq!(engine.hit_test(1, 0).unwrap(), Some(wide));
    assert_eq!(engine.display_list().unwrap().scroll_offset.x, 8);
    assert_eq!(
        engine.rasterize().unwrap().pixel(1, 0),
        Some([255, 255, 255, 255])
    );

    let diagonal = engine
        .action(NativeAction::Scroll {
            delta_x: -4,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(diagonal.accepted);
    assert_eq!(diagonal.revision, 3);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 4, y: 36 });

    let edge = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(edge.accepted);
    assert_eq!(edge.revision, 4);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 8, y: 36 });

    let no_op = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(!no_op.accepted);
    assert_eq!(no_op.revision, 4);

    let saved = engine.scroll_offset();
    engine.navigate("fixture://narrow").unwrap();
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    engine.go_back().unwrap().unwrap();
    assert_eq!(engine.scroll_offset(), saved);
}

#[test]
fn native_root_overflow_ignores_fully_clipped_text_but_keeps_visible_text_extent() {
    let document = NativeDocument::parse(
        "<div id='clipped' style='display:block;width:16px;overflow:hidden;white-space:nowrap'>ABCDEFG</div><div id='visible' style='display:block;white-space:nowrap'>ABCD</div><div id='clip' style='display:block;width:16px;overflow:clip;white-space:nowrap'>HIJKLMN</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let clipped = document.resolve_target("id=clipped").unwrap();
    let visible = document.resolve_target("id=visible").unwrap();
    let clip = document.resolve_target("id=clip").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(clipped).unwrap().width, 16);
    assert_eq!(layout.box_for(clip).unwrap().width, 16);
    assert_eq!(layout.box_for(visible).unwrap().width, 24);
    assert_eq!(layout.content_width, 32);
    assert_eq!(layout.max_scroll_offset().x, 8);
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.node_id == clipped && run.text == "ABCDEFG")
    );
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.node_id == clip && run.text == "HIJKLMN")
    );

    let clipped_only = NativeDocument::parse(
        "<div id='clipped' style='display:block;width:16px;overflow:hidden;white-space:nowrap'>ABCDEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let clipped_only_layout = clipped_only.layout(viewport).unwrap();
    assert_eq!(clipped_only_layout.content_width, viewport.width);
    assert_eq!(clipped_only_layout.max_scroll_offset().x, 0);
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
        NativeDisplayCommand::BeginOpacityGroup { .. }
        | NativeDisplayCommand::Clear { .. }
        | NativeDisplayCommand::EndOpacityGroup { .. } => false,
    }));
    assert_eq!(list, document.display_list(viewport).unwrap());
}

#[test]
fn native_opacity_groups_composite_subtrees_and_preserve_layout_hit_testing() {
    let document = NativeDocument::parse(
        "<div id='parent' style='display:block;width:40px;height:20px;background-color:red;opacity:50%'><div id='child' style='display:block;width:20px;height:10px;background-color:blue'></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(parent),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 40,
            height: 20,
        })
    );
    assert_eq!(
        layout.box_for(child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        })
    );
    assert_eq!(layout.hit_test(4, 4).unwrap(), Some(child));

    let list = document.display_list(viewport).unwrap();
    let groups = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::BeginOpacityGroup { node_id, opacity } => {
                Some((*node_id, *opacity))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(groups, vec![(parent, 128)]);
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::EndOpacityGroup { node_id } if *node_id == parent
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(30, 5), Some([255, 127, 127, 255]));
    assert_eq!(surface.pixel(5, 5), Some([127, 127, 255, 255]));
}

#[test]
fn native_nested_opacity_and_zero_opacity_keep_geometry_but_change_pixels() {
    let document = NativeDocument::parse(
        "<div id='zero' style='display:block;width:12px;height:12px;background-color:red;opacity:0'></div><div id='contents' style='display:contents;opacity:50%'><div id='child' style='display:block;width:12px;height:12px;background-color:blue;opacity:50%'></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let zero = document.resolve_target("id=zero").unwrap();
    let contents = document.resolve_target("id=contents").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(zero).unwrap().y, 0);
    assert_eq!(layout.box_for(child).unwrap().y, 12);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(zero));
    assert_eq!(layout.hit_test(1, 13).unwrap(), Some(child));

    let list = document.display_list(viewport).unwrap();
    let groups = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::BeginOpacityGroup { node_id, opacity } => {
                Some((*node_id, *opacity))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(groups, vec![(zero, 0), (contents, 128), (child, 128)]);

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(20, 20), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(5, 13), Some([191, 191, 255, 255]));
}

#[test]
fn native_text_alignment_shifts_complete_fixed_cell_line_items() {
    let document = NativeDocument::parse(
        "<style>#center { display: block; width: 32px; text-align: center; } #right { display: block; width: 32px; text-align: right; } #inline { display: block; width: 32px; text-align: center; } #chip { display: inline-block; width: 8px; height: 8px; background-color: red; }</style><div id='center'>A B C</div><div id='right'>D</div><div id='inline'><span id='chip'></span>Q</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let center = document.resolve_target("id=center").unwrap();
    let right = document.resolve_target("id=right").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let chip = document.resolve_target("id=chip").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(center).unwrap().width, 32);
    assert_eq!(layout.box_for(center).unwrap().height, 40);
    assert_eq!(layout.box_for(right).unwrap().y, 40);
    assert_eq!(layout.box_for(inline).unwrap().y, 60);
    assert_eq!(layout.box_for(chip).unwrap().x, 8);
    assert_eq!(layout.box_for(chip).unwrap().y, 60);
    assert_eq!(layout.hit_test(9, 61).unwrap(), Some(chip));

    let center_text = layout
        .text_runs
        .iter()
        .filter(|text_run| text_run.node_id == center)
        .map(|text_run| (text_run.origin, text_run.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        center_text,
        vec![
            (NativePoint { x: 4, y: 0 }, "A"),
            (NativePoint { x: 12, y: 0 }, " B"),
            (NativePoint { x: 12, y: 20 }, "C"),
        ]
    );
    let right_text = layout
        .text_runs
        .iter()
        .find(|text_run| text_run.node_id == right)
        .unwrap();
    assert_eq!(right_text.origin, NativePoint { x: 24, y: 40 });
    let inline_text = layout
        .text_runs
        .iter()
        .find(|text_run| text_run.node_id == inline)
        .unwrap();
    assert_eq!(inline_text.origin, NativePoint { x: 16, y: 60 });

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == chip && rect.x == 8 && rect.y == 60
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, origin, .. }
                if *node_id == inline && *origin == NativePoint { x: 16, y: 60 }
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(9, 61), Some([255, 0, 0, 255]));
}

#[test]
fn native_text_decoration_inherits_through_contents_and_reaches_raster() {
    let document = NativeDocument::parse(
        "<style>#parent { display:block; width:32px; text-decoration:underline; color:rgba(0, 128, 0, 50%); } #clear { text-decoration:none; } #contents { display:contents; }</style><div id='parent'>A<span id='clear'>B</span><span id='contents'><span id='nested'>C</span></span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 20,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let contents = document.resolve_target("id=contents").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_runs = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                color,
                underline,
                ..
            } => Some((*node_id, *origin, text.as_str(), *color, *underline)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected_color = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: 128,
    };
    assert!(
        text_runs
            .iter()
            .any(|(node_id, origin, text, color, underline)| {
                *node_id == parent
                    && *origin == NativePoint { x: 0, y: 0 }
                    && *text == "A"
                    && *color == expected_color
                    && *underline
            })
    );
    assert!(
        text_runs
            .iter()
            .any(|(node_id, _, text, color, underline)| {
                *node_id == clear && *text == "B" && *color == expected_color && !*underline
            })
    );
    assert!(
        !text_runs
            .iter()
            .any(|(node_id, _, _, _, _)| *node_id == contents)
    );
    assert!(
        text_runs
            .iter()
            .any(|(node_id, _, text, color, underline)| {
                *node_id == nested && *text == "C" && *color == expected_color && *underline
            })
    );

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 7), Some([127, 191, 127, 255]));
    assert_eq!(surface.pixel(8, 7), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(16, 7), Some([127, 191, 127, 255]));
}

#[test]
fn native_functional_alpha_colors_reach_display_list_and_raster() {
    let document = NativeDocument::parse(
        "<div id='background' style='display:block;width:8px;height:8px;background-color:rgba(255, 0, 0, 0.5)'></div><div id='border' style='display:block;width:8px;height:8px;border:1px solid rgba(0, 0, 255, 50%)'></div><div id='text' style='display:block;width:8px;height:8px;color:rgba(0, 128, 0, 0.5)'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 16,
        height: 28,
        device_scale_factor_milli: 1000,
    };
    let background = document.resolve_target("id=background").unwrap();
    let border = document.resolve_target("id=border").unwrap();
    let text = document.resolve_target("id=text").unwrap();
    let list = document.display_list(viewport).unwrap();
    let background_color = NativeColor {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 128,
    };
    let border_color = NativeColor {
        red: 0,
        green: 0,
        blue: 255,
        alpha: 128,
    };
    let text_color = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: 128,
    };
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == background && *color == background_color
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BorderRect { node_id, borders, .. }
                if *node_id == border && borders.top.color == border_color
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == text && *color == text_color
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(4, 4), Some([255, 127, 127, 255]));
    assert_eq!(surface.pixel(0, 8), Some([127, 127, 255, 255]));
    assert_eq!(surface.pixel(1, 18), Some([127, 191, 127, 255]));
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
fn native_inherited_line_height_controls_nested_auto_height_and_preserves_explicit_height() {
    let document = NativeDocument::parse(
        "<style>#parent { width:24px; line-height:28px; } #explicit { line-height:32px; }</style><div id='parent'><span id='inherit' style='display:inline'>A</span><span id='explicit' style='display:inline'>B</span><span id='invalid' style='display:inline;line-height:0px;height:4px'>C</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(parent).unwrap().height, 32);
    assert_eq!(layout.box_for(inherit).unwrap().height, 28);
    assert_eq!(layout.box_for(explicit).unwrap().height, 32);
    assert_eq!(layout.box_for(invalid).unwrap().height, 4);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(inherit));
    assert_eq!(layout.hit_test(9, 1).unwrap(), Some(explicit));
    assert_eq!(layout.hit_test(17, 1).unwrap(), Some(invalid));
    assert!(layout.text_runs.iter().all(|run| run.origin.y == 0));

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
    assert!(text_origins.contains(&(inherit, NativePoint { x: 0, y: 0 })));
    assert!(text_origins.contains(&(explicit, NativePoint { x: 8, y: 0 })));
    assert!(text_origins.contains(&(invalid, NativePoint { x: 16, y: 0 })));
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
        NativeDisplayCommand::BeginOpacityGroup { .. }
        | NativeDisplayCommand::Clear { .. }
        | NativeDisplayCommand::EndOpacityGroup { .. } => true,
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
            height: 24,
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
            height: 24,
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
fn native_nowrap_collapses_whitespace_without_soft_wrap_and_reaches_root_scroll() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://nowrap",
            "<style>#flow { white-space: nowrap; line-height: 24px; }</style><div id='flow'> A   B\n<span id='inline' style='display:inline; white-space:nowrap'> C   D E</span><span id='hidden' style='display:none;white-space:nowrap'> hidden hidden hidden</span></div>",
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
    assert_eq!(x_layout.hit_test(16, 15).unwrap(), None);
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
    assert_eq!(y_layout.hit_test(5, 15).unwrap(), None);
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
fn native_min_max_dimensions_constrain_content_and_border_box_geometry() {
    let document = NativeDocument::parse(
        "<style>#min { min-width: 40px; min-height: 30px; } #max { width: 24px; height: 20px; max-width: 16px; max-height: 10px; } #border { box-sizing: border-box; width: 20px; height: 20px; min-width: 28px; max-width: 32px; min-height: 26px; max-height: 30px; padding: 2px; border: 2px solid red; }</style><div id='min'>Min</div><div id='max'>Max</div><div id='border'>Border</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let min = document.resolve_target("id=min").unwrap();
    let max = document.resolve_target("id=max").unwrap();
    let border = document.resolve_target("id=border").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(min),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 40,
            height: 30,
        })
    );
    assert_eq!(
        layout.box_for(max),
        Some(NativeRect {
            x: 0,
            y: 30,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(
        layout.box_for(border),
        Some(NativeRect {
            x: 0,
            y: 40,
            width: 28,
            height: 26,
        })
    );
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == border)
            .map(|layout_box| layout_box.content_rect),
        Some(NativeRect {
            x: 4,
            y: 44,
            width: 20,
            height: 18,
        })
    );
    assert_eq!(layout.content_width, 40);
    assert_eq!(layout.max_scroll_offset().x, 8);
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

#[tokio::test]
async fn native_backend_dispatches_horizontal_scroll_into_capture() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 16,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://wide-dispatch",
            "<div style='white-space:nowrap'>A A</div>",
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
    assert_eq!((output.width, output.height), (16, 24));
    let pixel_index = output.line_size;
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
async fn local_fragment_navigation_and_history_traversal_preserve_bounded_state() {
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

#[tokio::test]
async fn local_link_activation_uses_bounded_navigation_default_action() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 160,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://links",
            "<a id='fragment' href='#part'>Part</a><a id='other' href='fixture://other'>Other</a><a id='remote' href='https://example.com'>Remote</a><a id='relative' href='next#target'>Next</a><a id='missing' href='missing'>Missing</a><a id='host-change' href='//other.test/path'>Host change</a><p>one two three four five six seven eight nine ten</p><p>eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one</p>",
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

#[tokio::test]
async fn fixture_relative_links_normalize_paths_queries_and_reject_opaque_bases() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://relative.test/docs/index",
            "<a id='parent' href='../next#target'>Parent</a><a id='dot' href='./child/../next?mode=fast#target'>Dot</a><a id='query' href='?view=full#target'>Query</a>",
        )
        .unwrap()
        .with_fixture("fixture://relative.test/next", "<p id='target'>Parent target</p>")
        .unwrap()
        .with_fixture(
            "fixture://relative.test/docs/next?mode=fast",
            "<p id='target'>Dot target</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://relative.test/docs/index?view=full",
            "<p id='target'>Query target</p>",
        )
        .unwrap()
        .with_initial_url("fixture://relative.test/docs/index");

    for (target, expected_url, expected_text) in [
        (
            "id=parent",
            "fixture://relative.test/next#target",
            "Parent target",
        ),
        (
            "id=dot",
            "fixture://relative.test/docs/next?mode=fast#target",
            "Dot target",
        ),
        (
            "id=query",
            "fixture://relative.test/docs/index?view=full#target",
            "Query target",
        ),
    ] {
        let mut engine = NativeEngine::new(config.clone()).unwrap();
        engine.initialize().unwrap();
        let result = engine
            .action(NativeAction::Click {
                target: target.into(),
            })
            .unwrap();
        assert!(result.accepted);
        assert_eq!(engine.snapshot().unwrap().url, expected_url);
        assert_eq!(engine.snapshot().unwrap().visible_text, expected_text);
    }

    let data_config = NativeEngineConfig::default().with_initial_url(
        "data:text/html,%3Ca%20id%3D%27relative%27%20href%3D%27next%27%3ERelative%3C%2Fa%3E",
    );
    let mut data_engine = NativeEngine::new(data_config).unwrap();
    data_engine.initialize().unwrap();
    let before_data = data_engine.snapshot().unwrap();
    let before_data_history = data_engine.history().clone();
    let data_error = data_engine
        .action(NativeAction::Click {
            target: "id=relative".into(),
        })
        .unwrap_err();
    assert!(matches!(
        data_error,
        NativeEngineError::UnsupportedUrl { .. }
    ));
    assert!(!data_error.to_string().contains("next"));
    assert_eq!(data_engine.snapshot().unwrap(), before_data);
    assert_eq!(data_engine.history(), &before_data_history);
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
async fn percent_decoded_fragments_match_utf8_ids_and_fail_closed() {
    let html = "<a id='jump' href='#pricing%20plan'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><button id='pricing plan'>Spaced target</button><button id='café'>UTF-8 target</button><button id='a+b'>Plus target</button><p id='duplicate name'>First duplicate</p><p id='duplicate name'>Second duplicate</p>";
    let viewport = Viewport {
        width: 160,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let spaced_id = expected_document.resolve_target("id=pricing plan").unwrap();
    let spaced_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(spaced_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(spaced_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://encoded-fragments", html)
        .unwrap()
        .with_initial_url("fixture://encoded-fragments");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let jumped = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jumped.accepted);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://encoded-fragments#pricing%20plan"
    );
    assert_eq!(engine.scroll_offset(), spaced_scroll);

    let utf8_id = NativeDocument::parse(html, &NativeEngineLimits::default())
        .unwrap()
        .resolve_target("id=café")
        .unwrap();
    let utf8_scroll = NativeDocument::parse(html, &NativeEngineLimits::default())
        .unwrap()
        .layout(viewport)
        .unwrap()
        .box_for(utf8_id)
        .unwrap()
        .y;
    engine
        .navigate("fixture://encoded-fragments#caf%C3%A9")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        utf8_scroll.min(expected_layout.max_scroll_offset().y)
    );

    engine.navigate("fixture://encoded-fragments#a+b").unwrap();
    let plus_scroll = engine.scroll_offset();
    assert!(plus_scroll.y > 0);
    engine
        .navigate("fixture://encoded-fragments#a%20b")
        .unwrap();
    assert_eq!(engine.scroll_offset(), plus_scroll);

    for fragment in ["duplicate%20name", "bad%ZZ", "bad%C3%28"] {
        let before = engine.scroll_offset();
        engine
            .navigate(format!("fixture://encoded-fragments#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), before, "fragment={fragment}");
    }
}

#[tokio::test]
async fn legacy_name_fragments_use_decoded_fallback_and_preserve_scroll_safety() {
    let html = "<a id='jump' href='#legacy%20plan'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><a name='legacy plan'>Legacy target</a><a name='café'>UTF-8 legacy target</a><a name='same'>Name should lose</a><a name='hidden' style='display:none'>Hidden legacy target</a><a name='zero' style='display:contents'></a><a name='duplicate'>First duplicate</a><a name='duplicate'>Second duplicate</a><div name='not-anchor'>Not a target</div><p id='same'>ID target</p>";
    let viewport = Viewport {
        width: 160,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
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

#[tokio::test]
async fn text_fragments_match_bounded_visible_runs_and_fail_closed() {
    let html = "<a id='jump' href='#:~:text=target%20phrase,anchor'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><p style='white-space:pre'>target phrase anchor and more</p><p style='white-space:pre'>comma, target phrase</p><p style='white-space:pre'>repeat target</p><p style='white-space:pre'>repeat target</p><p>cross <span>run target</span></p><p style='display:none'>hidden target</p><p style='white-space:pre'>plus a+b</p>";
    let viewport = Viewport {
        width: 240,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let range_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("target phrase") && run.text.contains("anchor"))
        .unwrap();
    let range_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(range_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(range_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://text-fragments", html)
        .unwrap()
        .with_initial_url("fixture://text-fragments");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let jumped = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jumped.accepted);
    assert_eq!(engine.scroll_offset(), range_scroll);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://text-fragments#:~:text=target%20phrase,anchor"
    );

    let initial = engine.go_back().unwrap().unwrap();
    assert_eq!(initial.url, "fixture://text-fragments");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    let restored = engine.go_forward().unwrap().unwrap();
    assert_eq!(
        restored.url,
        "fixture://text-fragments#:~:text=target%20phrase,anchor"
    );
    assert_eq!(engine.scroll_offset(), range_scroll);

    let comma_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("comma, target phrase"))
        .unwrap();
    engine
        .navigate("fixture://text-fragments#:~:text=comma%2C%20target%20phrase")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(comma_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );

    let plus_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("plus a+b"))
        .unwrap();
    engine
        .navigate("fixture://text-fragments#:~:text=plus%20a%2Bb")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(plus_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );

    let first_repeat = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("repeat target"))
        .unwrap();
    engine
        .navigate("fixture://text-fragments#:~:text=repeat%20target")
        .unwrap();
    assert_eq!(
        engine.scroll_offset().y,
        expected_layout
            .box_for(first_repeat.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y)
    );

    for fragment in [
        ":~:text=hidden%20target",
        ":~:text=cross,target",
        ":~:text=anchor,target",
        ":~:text=prefix-,target",
        ":~:text=target,-suffix",
        ":~:text=target,end,extra",
        ":~:text=bad%ZZ",
        ":~:text=bad%C3%28",
        ":~:text=missing",
    ] {
        let before = engine.scroll_offset();
        engine
            .navigate(format!("fixture://text-fragments#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), before, "fragment={fragment}");
    }
}

#[tokio::test]
async fn text_fragment_affixes_match_adjacent_terms_and_fail_closed() {
    let html = "<a id='jump' href='#:~:text=prefix%20-,target%20phrase,anchor,-%20suffix'>Jump</a><p>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen</p><p>seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six</p><p style='white-space:pre'>prefix target phrase anchor suffix</p><p style='white-space:pre'>prefix target phrase mismatch</p><p style='white-space:pre'>target phrase anchor suffix</p><p>cross <span>target phrase</span></p><p style='display:none'>prefix target phrase anchor suffix</p><p style='white-space:pre'>target phrase anchor</p>";
    let viewport = Viewport {
        width: 240,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let affix_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text == "prefix target phrase anchor suffix")
        .unwrap();
    let affix_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(affix_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(affix_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://text-affixes", html)
        .unwrap()
        .with_initial_url("fixture://text-affixes");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let jumped = engine
        .action(NativeAction::Click {
            target: "id=jump".into(),
        })
        .unwrap();
    assert!(jumped.accepted);
    assert_eq!(engine.scroll_offset(), affix_scroll);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://text-affixes#:~:text=prefix%20-,target%20phrase,anchor,-%20suffix"
    );

    let initial = engine.go_back().unwrap().unwrap();
    assert_eq!(initial.url, "fixture://text-affixes");
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 0 });
    let restored = engine.go_forward().unwrap().unwrap();
    assert_eq!(
        restored.url,
        "fixture://text-affixes#:~:text=prefix%20-,target%20phrase,anchor,-%20suffix"
    );
    assert_eq!(engine.scroll_offset(), affix_scroll);

    for fragment in [
        ":~:text=target%20phrase",
        ":~:text=target%20phrase,anchor",
        ":~:text=prefix%20-,target%20phrase",
        ":~:text=target%20phrase,-%20suffix",
        ":~:text=prefix%20-,target%20phrase,-%20suffix",
        ":~:text=prefix%20-,target%20phrase,anchor",
        ":~:text=target%20phrase,anchor,-%20suffix",
        ":~:text=prefix%20-,target%20phrase,anchor,-%20suffix",
    ] {
        engine
            .navigate(format!("fixture://text-affixes#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), affix_scroll, "fragment={fragment}");
    }

    for fragment in [
        ":~:text=prefix-,target%20phrase",
        ":~:text=target%20phrase,-suffix",
        ":~:text=wrong%20-,target%20phrase",
        ":~:text=target%20phrase,-%20wrong",
        ":~:text=anchor,target%20phrase",
        ":~:text=cross,target",
        ":~:text=hidden%20-,target%20phrase",
        ":~:text=target%20phrase,anchor,extra",
        ":~:text=prefix%20-,target%20phrase,anchor,extra",
        ":~:text=prefix%20-,target%20phrase,-%20suffix,extra",
        ":~:text=target%20phrase,anchor,-",
        ":~:text=bad%ZZ,target",
    ] {
        let before = engine.scroll_offset();
        engine
            .navigate(format!("fixture://text-affixes#{fragment}"))
            .unwrap();
        assert_eq!(engine.scroll_offset(), before, "fragment={fragment}");
    }
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
        "<style>button:hover, main > button, #ok { color: red; width: 10%; display: flex; opacity: 1.1; text-align: justify; text-decoration: overline; overflow: visible; white-space: break-spaces; custom-property: url(secret); broken; }</style><style>.unclosed { color: blue; </style><button id='ok' style='background-image: url(secret); padding: -1px'>OK</button>",
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
            && diagnostic.detail == "opacity"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-align"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration"
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

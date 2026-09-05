#![cfg(feature = "native-engine")]

use glass_browser::browser::native_backend::NATIVE_ENGINE_BACKEND_ID;
use glass_browser::browser::native_engine::{
    MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES, MAX_NATIVE_DIAGNOSTICS, NativeAction, NativeBorderRadius,
    NativeBorderStyle, NativeColor, NativeDiagnosticCode, NativeDiagnosticSource,
    NativeDisplayCommand, NativeDocument, NativeEngine, NativeEngineConfig, NativeEngineError,
    NativeEngineLimits, NativeEventKind, NativeLifecycleState, NativeNodeId, NativePoint,
    NativeRect, NativeSurface, NativeTextDecorationSkipInk, NativeTextDecorationStyle, Viewport,
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
fn native_text_alignment_maps_logical_edges_through_direction_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#ltr-start,#ltr-end,#rtl-start,#rtl-end,#physical-left,#physical-right,#inline { display:block; width:32px; height:20px; } #ltr-start { direction:ltr; text-align:start; } #ltr-end { direction:ltr; text-align:end; } #rtl-start { text-align:start; } #rtl-end { text-align:end; } #physical-left { text-align:left; } #physical-right { text-align:right; } #rtl-wrapped { display:block; width:32px; text-align:start; } #inline { text-align:start; } #chip { display:inline-block; width:8px; height:8px; background-color:red; }</style><div id='root' style='direction:rtl'><div id='ltr-start'>L</div><div id='ltr-end'>E</div><div id='rtl-start'>S</div><div id='rtl-end'>N</div><div id='physical-left'>P</div><div id='physical-right'>R</div><div id='rtl-wrapped'>A B C</div><div id='inline'><span id='chip'></span>Q</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let expected_text_x = [
        ("ltr-start", 0),
        ("ltr-end", 24),
        ("rtl-start", 24),
        ("rtl-end", 0),
        ("physical-left", 0),
        ("physical-right", 24),
    ];
    for (target, expected_x) in expected_text_x {
        let node = document.resolve_target(&format!("id={target}")).unwrap();
        let text_run = layout
            .text_runs
            .iter()
            .find(|text_run| text_run.node_id == node)
            .unwrap();
        assert_eq!(text_run.origin.x, expected_x, "{target}");
    }

    let inline = document.resolve_target("id=inline").unwrap();
    let chip = document.resolve_target("id=chip").unwrap();
    let rtl_wrapped = document.resolve_target("id=rtl-wrapped").unwrap();
    let inline_rect = layout.box_for(inline).unwrap();
    let chip_rect = layout.box_for(chip).unwrap();
    assert_eq!(chip_rect.x, 16);
    assert_eq!(chip_rect.y, inline_rect.y);
    let inline_text = layout
        .text_runs
        .iter()
        .find(|text_run| text_run.node_id == inline)
        .unwrap();
    assert_eq!(
        inline_text.origin,
        NativePoint {
            x: 24,
            y: inline_rect.y
        }
    );
    assert_eq!(
        layout.hit_test(17, i64::from(inline_rect.y + 1)),
        Ok(Some(chip))
    );

    let wrapped_rect = layout.box_for(rtl_wrapped).unwrap();
    let wrapped_text = layout
        .text_runs
        .iter()
        .filter(|text_run| text_run.node_id == rtl_wrapped)
        .map(|text_run| (text_run.origin, text_run.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        wrapped_text,
        vec![
            (
                NativePoint {
                    x: 8,
                    y: wrapped_rect.y
                },
                "A"
            ),
            (
                NativePoint {
                    x: 16,
                    y: wrapped_rect.y
                },
                " B"
            ),
            (
                NativePoint {
                    x: 24,
                    y: wrapped_rect.y + 20
                },
                "C"
            ),
        ]
    );

    let display_list = document.display_list(viewport).unwrap();
    assert!(display_list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == chip && rect.x == 16 && rect.y == inline_rect.y
        )
    }));
    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(17, inline_rect.y + 1), Some([255, 0, 0, 255]));

    let visible = document.visible_text(1024).0;
    let mut search_start = 0;
    for token in ["L", "E", "S", "N", "P", "R", "A", "B", "C", "Q"] {
        let offset = visible[search_start..].find(token).unwrap();
        search_start = search_start.saturating_add(offset + token.len());
    }
}

#[test]
fn native_justified_text_expands_soft_wrapped_spaces_through_shared_artifacts() {
    let document = NativeDocument::parse(
        "<style>#justify { display:block; width:45px; text-align:justify; } #hard { display:block; width:45px; text-align:justify; white-space:pre-line; } #pre { display:block; width:45px; text-align:justify; white-space:pre-wrap; } #break { display:block; width:45px; text-align:justify; word-break:break-all; }</style><div id='justify'>A B C D</div><div id='hard'>A B C<br>D E F</div><div id='pre'>A B C D</div><div id='break'>ABCDEFGHIJ</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let justify = document.resolve_target("id=justify").unwrap();
    let hard = document.resolve_target("id=hard").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let break_all = document.resolve_target("id=break").unwrap();
    let layout = document.layout(viewport).unwrap();

    let justify_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == justify)
        .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
        .collect::<Vec<_>>();
    assert_eq!(
        justify_runs,
        vec![
            (NativePoint { x: 0, y: 0 }, "A", 0),
            (NativePoint { x: 8, y: 0 }, " B", 3),
            (NativePoint { x: 27, y: 0 }, " C", 2),
            (NativePoint { x: 0, y: 20 }, "D", 0),
        ]
    );
    assert_eq!(layout.box_for(justify).unwrap().height, 40);
    assert_eq!(layout.content_width, 45);

    let hard_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == hard)
        .collect::<Vec<_>>();
    assert!(hard_runs.iter().all(|run| run.justify_spacing == 0));

    let pre_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == pre)
        .collect::<Vec<_>>();
    assert!(pre_runs.iter().all(|run| run.justify_spacing == 0));
    let break_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == break_all)
        .collect::<Vec<_>>();
    assert!(break_runs.iter().all(|run| run.justify_spacing == 0));

    let list = document.display_list(viewport).unwrap();
    let justify_commands = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                justify_spacing,
                ..
            } if *node_id == justify => Some((*origin, text.as_str(), *justify_spacing)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(justify_commands, justify_runs);

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(19, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(16, 0), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(37, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 20), Some([0, 0, 0, 255]));
}

#[test]
fn native_justification_composes_authored_word_spacing() {
    let document = NativeDocument::parse(
        "<style>#target { display:block; width:45px; text-align:justify; word-spacing:2px; }</style><div id='target'>A B C D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let target = document.resolve_target("id=target").unwrap();
    let layout = document
        .layout(Viewport {
            width: 45,
            height: 80,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    let runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == target)
        .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![
            (NativePoint { x: 0, y: 0 }, "A", 0),
            (NativePoint { x: 8, y: 0 }, " B", 1),
            (NativePoint { x: 27, y: 0 }, " C", 0),
            (NativePoint { x: 0, y: 20 }, "D", 0),
        ]
    );

    let list = document
        .display_list(Viewport {
            width: 45,
            height: 80,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                text,
                word_spacing: 2,
                justify_spacing: 1,
                ..
            } if *node_id == target && text == " B"
        )
    }));
}

#[test]
fn native_text_align_last_resolves_final_lines_through_shared_artifacts() {
    let document = NativeDocument::parse(
        "<style>.block { display:block; width:45px; text-align:justify; } #center { text-align-last:center; } #right { text-align-last:right; } #start-rtl { direction:rtl; text-align-last:start; } #end-rtl { direction:rtl; text-align-last:end; } #auto { text-align-last:auto; } #hard { text-align-last:right; }</style><div id='center' class='block'>A B C D</div><div id='right' class='block'>A B C D</div><div id='start-rtl' class='block'>A B C D</div><div id='end-rtl' class='block'>A B C D</div><div id='auto' class='block'>A B C D</div><div id='hard' class='block'>A B C<br>D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 280,
        device_scale_factor_milli: 1000,
    };
    let center = document.resolve_target("id=center").unwrap();
    let right = document.resolve_target("id=right").unwrap();
    let start_rtl = document.resolve_target("id=start-rtl").unwrap();
    let end_rtl = document.resolve_target("id=end-rtl").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let hard = document.resolve_target("id=hard").unwrap();
    let layout = document.layout(viewport).unwrap();

    let final_origin = |node_id| {
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id && run.text == "D" && run.origin.y > 0)
            .map(|run| run.origin)
            .unwrap()
    };
    assert_eq!(final_origin(center), NativePoint { x: 18, y: 20 });
    assert_eq!(final_origin(right), NativePoint { x: 37, y: 60 });
    assert_eq!(final_origin(start_rtl), NativePoint { x: 37, y: 100 });
    assert_eq!(final_origin(end_rtl), NativePoint { x: 0, y: 140 });
    assert_eq!(final_origin(auto), NativePoint { x: 0, y: 180 });
    assert_eq!(final_origin(hard), NativePoint { x: 37, y: 220 });

    let hard_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == hard)
        .collect::<Vec<_>>();
    assert!(hard_runs.iter().all(|run| run.justify_spacing == 0));
    assert_eq!(layout.box_for(center).unwrap().height, 40);

    let display_list = document.display_list(viewport).unwrap();
    assert!(display_list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                origin: NativePoint { x: 18, y: 20 },
                text,
                ..
            } if *node_id == center && text == "D"
        )
    }));
    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(18, 20), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_align_last_justifies_final_lines_without_changing_other_paths() {
    let document = NativeDocument::parse(
        "<style>.block { display:block; width:45px; text-align-last:justify; } #tight { width:40px; } #pre { white-space:pre; } #pre-wrap { white-space:pre-wrap; } #break { word-break:break-all; } #spacing { word-spacing:2px; }</style><div id='target' class='block'>A B C</div><div id='tight' class='block'>A B C</div><div id='hard' class='block'>A B<br>C D</div><div id='pre' class='block'>A B C</div><div id='pre-wrap' class='block'>A B C</div><div id='break' class='block'>A B C</div><div id='spacing' class='block'>A B C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let target = document.resolve_target("id=target").unwrap();
    let tight = document.resolve_target("id=tight").unwrap();
    let hard = document.resolve_target("id=hard").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let pre_wrap = document.resolve_target("id=pre-wrap").unwrap();
    let break_all = document.resolve_target("id=break").unwrap();
    let spacing = document.resolve_target("id=spacing").unwrap();
    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        runs_for(target),
        vec![
            (NativePoint { x: 0, y: 0 }, "A", 0),
            (NativePoint { x: 8, y: 0 }, " B", 3),
            (NativePoint { x: 27, y: 0 }, " C", 2),
        ]
    );
    assert_eq!(
        runs_for(tight),
        vec![
            (NativePoint { x: 0, y: 20 }, "A", 0),
            (NativePoint { x: 8, y: 20 }, " B", 0),
            (NativePoint { x: 24, y: 20 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(hard),
        vec![
            (NativePoint { x: 0, y: 40 }, "A", 0),
            (NativePoint { x: 8, y: 40 }, " B", 0),
            (NativePoint { x: 0, y: 60 }, "C", 0),
            (NativePoint { x: 8, y: 60 }, " D", 21),
        ]
    );
    assert_eq!(
        runs_for(pre),
        vec![(NativePoint { x: 0, y: 80 }, "A B C", 0)]
    );
    assert_eq!(
        runs_for(pre_wrap),
        vec![(NativePoint { x: 0, y: 100 }, "A B C", 0)]
    );
    assert_eq!(
        runs_for(break_all),
        vec![
            (NativePoint { x: 0, y: 120 }, "A", 0),
            (NativePoint { x: 8, y: 120 }, " B", 0),
            (NativePoint { x: 24, y: 120 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(spacing),
        vec![
            (NativePoint { x: 0, y: 140 }, "A", 0),
            (NativePoint { x: 8, y: 140 }, " B", 1),
            (NativePoint { x: 27, y: 140 }, " C", 0),
        ]
    );
    assert_eq!(layout.box_for(target).unwrap().height, 20);

    let display_list = document.display_list(viewport).unwrap();
    let target_commands = display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                justify_spacing,
                ..
            } if *node_id == target => Some((*origin, text.as_str(), *justify_spacing)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(target_commands, runs_for(target));
    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_justify_controls_both_justification_flushes() {
    let document = NativeDocument::parse(
        "<style>.soft { display:block; width:45px; text-align:justify; } #none-soft { text-justify:none; } #inter-word-soft { text-justify:inter-word; } .final { display:block; width:45px; text-align:left; text-align-last:justify; } #none-final { text-justify:none; } #alone { display:block; width:45px; text-align:left; text-justify:inter-word; } #spacing-none { display:block; width:45px; text-align:justify; word-spacing:2px; text-justify:none; }</style><div id='auto-soft' class='soft'>A B C D</div><div id='none-soft' class='soft'>A B C D</div><div id='inter-word-soft' class='soft'>A B C D</div><div id='auto-final' class='final'>A B C</div><div id='none-final' class='final'>A B C</div><div id='alone' >A B C</div><div id='spacing-none'>A B C</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 45,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let auto_soft = document.resolve_target("id=auto-soft").unwrap();
    let none_soft = document.resolve_target("id=none-soft").unwrap();
    let inter_word_soft = document.resolve_target("id=inter-word-soft").unwrap();
    let auto_final = document.resolve_target("id=auto-final").unwrap();
    let none_final = document.resolve_target("id=none-final").unwrap();
    let alone = document.resolve_target("id=alone").unwrap();
    let spacing_none = document.resolve_target("id=spacing-none").unwrap();
    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str(), run.justify_spacing))
            .collect::<Vec<_>>()
    };
    let first_line = |y: u32, spacing: [u32; 2]| {
        vec![
            (NativePoint { x: 0, y }, "A", 0),
            (NativePoint { x: 8, y }, " B", spacing[0]),
            (NativePoint { x: 27, y }, " C", spacing[1]),
            (NativePoint { x: 0, y: y + 20 }, "D", 0),
        ]
    };

    assert_eq!(runs_for(auto_soft), first_line(0, [3, 2]));
    assert_eq!(
        runs_for(none_soft),
        vec![
            (NativePoint { x: 0, y: 40 }, "A", 0),
            (NativePoint { x: 8, y: 40 }, " B", 0),
            (NativePoint { x: 24, y: 40 }, " C", 0),
            (NativePoint { x: 0, y: 60 }, "D", 0),
        ]
    );
    assert_eq!(runs_for(inter_word_soft), first_line(80, [3, 2]));
    assert_eq!(
        runs_for(auto_final),
        first_line(120, [3, 2])
            .into_iter()
            .take(3)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        runs_for(none_final),
        vec![
            (NativePoint { x: 0, y: 140 }, "A", 0),
            (NativePoint { x: 8, y: 140 }, " B", 0),
            (NativePoint { x: 24, y: 140 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(alone),
        vec![
            (NativePoint { x: 0, y: 160 }, "A", 0),
            (NativePoint { x: 8, y: 160 }, " B", 0),
            (NativePoint { x: 24, y: 160 }, " C", 0),
        ]
    );
    assert_eq!(
        runs_for(spacing_none),
        vec![
            (NativePoint { x: 0, y: 180 }, "A", 0),
            (NativePoint { x: 8, y: 180 }, " B", 0),
            (NativePoint { x: 26, y: 180 }, " C", 0),
        ]
    );

    let display_list = document.display_list(viewport).unwrap();
    let none_commands = display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                justify_spacing,
                ..
            } if *node_id == none_final => Some((*origin, text.as_str(), *justify_spacing)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(none_commands, runs_for(none_final));

    let surface = display_list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_decoration_inherits_through_contents_and_reaches_raster() {
    let document = NativeDocument::parse(
        "<style>#parent { display:block; width:32px; text-decoration:underline overline; color:rgba(0, 128, 0, 50%); } #clear { text-decoration:none; } #contents { display:contents; }</style><div id='parent'>A<span id='clear'>B</span><span id='contents'><span id='nested'>C</span></span></div>",
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
                overline,
                line_through,
                ..
            } => Some((
                *node_id,
                *origin,
                text.as_str(),
                *color,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected_color = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: 128,
    };
    assert!(text_runs.iter().any(
        |(node_id, origin, text, color, underline, overline, line_through)| {
            *node_id == parent
                && *origin == NativePoint { x: 0, y: 0 }
                && *text == "A"
                && *color == expected_color
                && *underline
                && *overline
                && !*line_through
        }
    ));
    assert!(text_runs.iter().any(
        |(node_id, _, text, color, underline, overline, line_through)| {
            *node_id == clear
                && *text == "B"
                && *color == expected_color
                && !*underline
                && !*overline
                && !*line_through
        }
    ));
    assert!(
        !text_runs
            .iter()
            .any(|(node_id, _, _, _, _, _, _)| *node_id == contents)
    );
    assert!(text_runs.iter().any(
        |(node_id, _, text, color, underline, overline, line_through)| {
            *node_id == nested
                && *text == "C"
                && *color == expected_color
                && *underline
                && *overline
                && !*line_through
        }
    ));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 7), Some([127, 191, 127, 255]));
    assert_eq!(surface.pixel(8, 7), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(16, 7), Some([127, 191, 127, 255]));
}

#[test]
fn native_text_decoration_line_styles_share_layout_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; } #plain { text-decoration:none; } #over { text-decoration:overline; } #through { text-decoration:line-through; } #under { text-decoration:underline; }</style><div id='plain' class='line'>A</div><div id='over' class='line'>A</div><div id='through' class='line'>A</div><div id='under' class='line'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let plain = document.resolve_target("id=plain").unwrap();
    let over = document.resolve_target("id=over").unwrap();
    let through = document.resolve_target("id=through").unwrap();
    let under = document.resolve_target("id=under").unwrap();
    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| {
                (
                    run.origin,
                    run.text.as_str(),
                    run.truncated,
                    run.justify_spacing,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs_for(plain),
        vec![(NativePoint { x: 0, y: 0 }, "A", false, 0)]
    );
    assert_eq!(
        runs_for(over),
        vec![(NativePoint { x: 0, y: 20 }, "A", false, 0)]
    );
    assert_eq!(
        runs_for(through),
        vec![(NativePoint { x: 0, y: 40 }, "A", false, 0)]
    );
    assert_eq!(
        runs_for(under),
        vec![(NativePoint { x: 0, y: 60 }, "A", false, 0)]
    );

    let list = document.display_list(viewport).unwrap();
    let decoration_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((*underline, *overline, *line_through)),
            _ => None,
        })
    };
    assert_eq!(decoration_for(plain), Some((false, false, false)));
    assert_eq!(decoration_for(over), Some((false, true, false)));
    assert_eq!(decoration_for(through), Some((false, false, true)));
    assert_eq!(decoration_for(under), Some((true, false, false)));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 0), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(5, 19), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 43), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 67), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(6, 19), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(6, 43), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(6, 67), Some([255, 255, 255, 255]));
}

#[test]
fn native_text_decoration_combinations_share_layout_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>.spacer { display:block; height:20px; } .line { display:block; width:24px; height:20px; line-height:20px; color:black; } #combo { text-decoration:underline overline; } #all { text-decoration:line-through underline overline; }</style><div class='spacer'></div><div id='combo' class='line'>A</div><div id='all' class='line'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 60,
        device_scale_factor_milli: 1000,
    };
    let combo = document.resolve_target("id=combo").unwrap();
    let all = document.resolve_target("id=all").unwrap();
    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs_for(combo),
        vec![(NativePoint { x: 0, y: 20 }, "A", false)]
    );
    assert_eq!(
        runs_for(all),
        vec![(NativePoint { x: 0, y: 40 }, "A", false)]
    );

    let list = document.display_list(viewport).unwrap();
    let decoration_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((*underline, *overline, *line_through)),
            _ => None,
        })
    };
    assert_eq!(decoration_for(combo), Some((true, true, false)));
    assert_eq!(decoration_for(all), Some((true, true, true)));

    let surface = list.rasterize().unwrap();
    for y in [19, 27, 39, 43, 47] {
        assert_eq!(surface.pixel(5, y), Some([0, 0, 0, 255]));
        assert_eq!(surface.pixel(6, y), Some([255, 255, 255, 255]));
    }
}

#[test]
fn native_text_decoration_color_separates_glyph_and_line_paint() {
    let document = NativeDocument::parse(
        "<style>#target { display:block; width:24px; height:20px; line-height:20px; color:red; text-decoration:underline overline; text-decoration-color:blue; }</style><div id='target'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 20,
        device_scale_factor_milli: 1000,
    };
    let target = document.resolve_target("id=target").unwrap();
    let list = document.display_list(viewport).unwrap();
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let text_command = list.commands.iter().find_map(|command| match command {
        NativeDisplayCommand::TextRun {
            node_id,
            color,
            decoration_color,
            underline,
            overline,
            line_through,
            ..
        } if *node_id == target => Some((
            *color,
            *decoration_color,
            *underline,
            *overline,
            *line_through,
        )),
        _ => None,
    });
    assert_eq!(
        text_command,
        Some((NativeColor::RED, blue, true, true, false))
    );

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 7), Some([0, 0, 255, 255]));
}

#[test]
fn native_text_decoration_line_longhand_reuses_color_and_geometry() {
    let document = NativeDocument::parse(
        "<style>#target { display:block; width:24px; height:20px; line-height:20px; color:red; text-decoration-line:overline line-through; text-decoration-color:blue; }</style><div id='target'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 20,
        device_scale_factor_milli: 1000,
    };
    let target = document.resolve_target("id=target").unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = list.commands.iter().find_map(|command| match command {
        NativeDisplayCommand::TextRun {
            node_id,
            color,
            decoration_color,
            underline,
            overline,
            line_through,
            ..
        } if *node_id == target => Some((
            *color,
            *decoration_color,
            *underline,
            *overline,
            *line_through,
        )),
        _ => None,
    });
    assert_eq!(
        text_command,
        Some((
            NativeColor::RED,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255
            },
            false,
            true,
            true
        ))
    );

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 3), Some([0, 0, 255, 255]));
}

#[test]
fn native_text_decoration_style_patterns_share_command_and_geometry() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; text-decoration:underline; } #solid { text-decoration-style:solid; } #dashed { text-decoration-style:dashed; } #dotted { text-decoration-style:dotted; } #parent { text-decoration-style:dashed; } #override { text-decoration-style:solid; }</style><div id='solid' class='line'>A</div><div id='dashed' class='line'>A</div><div id='dotted' class='line'>A</div><div id='parent' class='line'><span id='inherited'>A</span><span id='override'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 84,
        device_scale_factor_milli: 1000,
    };
    let solid = document.resolve_target("id=solid").unwrap();
    let dashed = document.resolve_target("id=dashed").unwrap();
    let dotted = document.resolve_target("id=dotted").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let override_node = document.resolve_target("id=override").unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                underline,
                ..
            } if *command_node_id == node_id => Some((*origin, *decoration_style, *underline)),
            _ => None,
        })
    };
    let solid_command = text_command(solid).expect("solid text command");
    let dashed_command = text_command(dashed).expect("dashed text command");
    let dotted_command = text_command(dotted).expect("dotted text command");
    let inherited_command = text_command(inherited).expect("inherited text command");
    let override_command = text_command(override_node).expect("override text command");
    assert_eq!(solid_command.1, NativeTextDecorationStyle::Solid);
    assert_eq!(dashed_command.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(dotted_command.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(inherited_command.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(override_command.1, NativeTextDecorationStyle::Solid);
    assert!(
        solid_command.2
            && dashed_command.2
            && dotted_command.2
            && inherited_command.2
            && override_command.2
    );

    let surface = list.rasterize().unwrap();
    let line_pixel = |command: (NativePoint, NativeTextDecorationStyle, bool), offset: u32| {
        surface.pixel(
            command.0.x.saturating_add(offset),
            command.0.y.saturating_add(7),
        )
    };
    assert_eq!(line_pixel(solid_command, 0), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(solid_command, 5), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(dashed_command, 0), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(dashed_command, 2), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(dashed_command, 3), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(dashed_command, 4), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(dashed_command, 5), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(dotted_command, 0), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(dotted_command, 1), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(dotted_command, 2), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(dotted_command, 3), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(inherited_command, 0), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(inherited_command, 3), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(override_command, 0), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(override_command, 5), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_decoration_skip_ink_cascades_to_display_commands() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; text-decoration:underline; text-decoration-style:wavy; } #none { text-decoration-skip-ink:none; } #parent { text-decoration-skip-ink:auto; } #invalid { text-decoration-skip-ink:all; }</style><div id='auto' class='line'>A</div><div id='none' class='line'>A</div><div id='parent' class='line'><span id='inherited'>A</span></div><div id='invalid' class='line'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let none = document.resolve_target("id=none").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let list = document
        .display_list(Viewport {
            width: 32,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let skip_ink_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                decoration_skip_ink,
                ..
            } if *command_node_id == node_id => Some(*decoration_skip_ink),
            _ => None,
        })
    };

    assert_eq!(skip_ink_for(auto), Some(NativeTextDecorationSkipInk::Auto));
    assert_eq!(skip_ink_for(none), Some(NativeTextDecorationSkipInk::None));
    assert_eq!(
        skip_ink_for(inherited),
        Some(NativeTextDecorationSkipInk::Auto)
    );
    assert_eq!(
        skip_ink_for(invalid),
        Some(NativeTextDecorationSkipInk::Auto)
    );
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-ink"
    }));
}

#[test]
fn native_text_decoration_thickness_shares_style_and_line_geometry() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; text-decoration:underline; } #one { text-decoration-style:solid; text-decoration-thickness:1px; } #two { text-decoration-style:dashed; text-decoration-thickness:2px; } #three { text-decoration-style:dotted; text-decoration-thickness:3px; } #four { text-decoration-style:solid; text-decoration-thickness:4px; } #parent { text-decoration-style:dotted; text-decoration-thickness:3px; }</style><div id='one' class='line'>AB</div><div id='two' class='line'>AB</div><div id='three' class='line'>AB</div><div id='four' class='line'>AB</div><div id='parent' class='line'><span id='inherited'>A</span><span id='override' style='text-decoration-thickness:1px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let one = document.resolve_target("id=one").unwrap();
    let two = document.resolve_target("id=two").unwrap();
    let three = document.resolve_target("id=three").unwrap();
    let four = document.resolve_target("id=four").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let override_node = document.resolve_target("id=override").unwrap();
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                decoration_thickness,
                underline,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *decoration_style,
                *decoration_thickness,
                *underline,
            )),
            _ => None,
        })
    };
    let one_command = text_command(one).expect("one-pixel text command");
    let two_command = text_command(two).expect("two-pixel text command");
    let three_command = text_command(three).expect("three-pixel text command");
    let four_command = text_command(four).expect("four-pixel text command");
    let inherited_command = text_command(inherited).expect("inherited text command");
    let override_command = text_command(override_node).expect("override text command");
    assert_eq!(one_command.1, NativeTextDecorationStyle::Solid);
    assert_eq!(one_command.2, 1);
    assert_eq!(two_command.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(two_command.2, 2);
    assert_eq!(three_command.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(three_command.2, 3);
    assert_eq!(four_command.1, NativeTextDecorationStyle::Solid);
    assert_eq!(four_command.2, 4);
    assert_eq!(inherited_command.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(inherited_command.2, 3);
    assert_eq!(override_command.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(override_command.2, 1);
    assert!(one_command.3 && two_command.3 && three_command.3 && four_command.3);
    assert_eq!(
        layout.box_for(one),
        layout.box_for(two).map(|rect| NativeRect {
            y: rect.y.saturating_sub(20),
            ..rect
        })
    );

    let surface = list.rasterize().unwrap();
    let line_pixel =
        |command: (NativePoint, NativeTextDecorationStyle, u32, bool), x: u32, y: u32| {
            surface.pixel(command.0.x.saturating_add(x), command.0.y.saturating_add(y))
        };
    for y in 7..8 {
        assert_eq!(line_pixel(one_command, 0, y), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(one_command, 7, y), Some([0, 0, 0, 255]));
    }
    for y in 7..9 {
        assert_eq!(line_pixel(two_command, 0, y), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(two_command, 5, y), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(two_command, 6, y), Some([255, 255, 255, 255]));
    }
    for y in 7..10 {
        assert_eq!(line_pixel(three_command, 0, y), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(three_command, 2, y), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(three_command, 3, y), Some([255, 255, 255, 255]));
        assert_eq!(line_pixel(three_command, 6, y), Some([0, 0, 0, 255]));
    }
    for y in 7..11 {
        assert_eq!(line_pixel(four_command, 0, y), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(four_command, 7, y), Some([0, 0, 0, 255]));
    }
    for y in 7..10 {
        assert_eq!(
            line_pixel(inherited_command, 3, y),
            Some([255, 255, 255, 255])
        );
    }
    assert_eq!(line_pixel(override_command, 0, 7), Some([0, 0, 0, 255]));
    assert_eq!(
        line_pixel(override_command, 0, 8),
        Some([255, 255, 255, 255])
    );
}

#[test]
fn native_text_underline_offset_moves_only_underlines_through_shared_artifacts() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; text-decoration:underline overline line-through; text-decoration-style:dashed; text-decoration-thickness:2px; } #negative { text-underline-offset:-2px; } #zero { text-underline-offset:0px; } #positive { text-underline-offset:3px; } #parent { text-decoration-style:dotted; text-underline-offset:-3px; }</style><div id='negative' class='line'>AB</div><div id='zero' class='line'>AB</div><div id='positive' class='line'>AB</div><div id='parent' class='line'><span id='inherited'>A</span><span id='override' style='text-underline-offset:2px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 100,
        device_scale_factor_milli: 1000,
    };
    let negative = document.resolve_target("id=negative").unwrap();
    let zero = document.resolve_target("id=zero").unwrap();
    let positive = document.resolve_target("id=positive").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let override_node = document.resolve_target("id=override").unwrap();
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                decoration_thickness,
                underline_offset,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *decoration_style,
                *decoration_thickness,
                *underline_offset,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let negative_command = text_command(negative).expect("negative-offset text command");
    let zero_command = text_command(zero).expect("zero-offset text command");
    let positive_command = text_command(positive).expect("positive-offset text command");
    let inherited_command = text_command(inherited).expect("inherited-offset text command");
    let override_command = text_command(override_node).expect("override-offset text command");

    assert_eq!(negative_command.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(zero_command.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(positive_command.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(inherited_command.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(override_command.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(negative_command.2, 2);
    assert_eq!(zero_command.2, 2);
    assert_eq!(positive_command.2, 2);
    assert_eq!(inherited_command.2, 2);
    assert_eq!(override_command.2, 2);
    assert_eq!(negative_command.3, -2);
    assert_eq!(zero_command.3, 0);
    assert_eq!(positive_command.3, 3);
    assert_eq!(inherited_command.3, -3);
    assert_eq!(override_command.3, 2);
    for command in [
        negative_command,
        zero_command,
        positive_command,
        inherited_command,
        override_command,
    ] {
        assert!(command.4 && command.5 && command.6);
    }
    assert_eq!(
        layout.box_for(negative),
        layout.box_for(zero).map(|rect| NativeRect {
            y: rect.y.saturating_sub(20),
            ..rect
        })
    );
    assert_eq!(
        layout.box_for(positive),
        layout.box_for(zero).map(|rect| NativeRect {
            y: rect.y.saturating_add(20),
            ..rect
        })
    );

    let surface = list.rasterize().unwrap();
    let line_pixel =
        |command: (
            NativePoint,
            NativeTextDecorationStyle,
            u32,
            i32,
            bool,
            bool,
            bool,
        ),
         x: u32,
         y: u32| { surface.pixel(command.0.x.saturating_add(x), y) };
    for y in [5, 6] {
        assert_eq!(line_pixel(negative_command, 2, y), Some([0, 0, 0, 255]));
    }
    for y in [27, 28] {
        assert_eq!(line_pixel(zero_command, 2, y), Some([0, 0, 0, 255]));
    }
    for y in [50, 51] {
        assert_eq!(line_pixel(positive_command, 2, y), Some([0, 0, 0, 255]));
    }
    for y in [19, 23, 39, 43] {
        assert_eq!(surface.pixel(5, y), Some([0, 0, 0, 255]));
    }
    for (x, y) in [(7, 4), (0, 7), (5, 26), (0, 29), (0, 49), (0, 52)] {
        assert_eq!(
            surface.pixel(x, y),
            Some([255, 255, 255, 255]),
            "gap at x={x}, y={y}"
        );
    }
    assert_eq!(
        surface.pixel(inherited_command.0.x, 64),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(override_command.0.x, 69),
        Some([0, 0, 0, 255])
    );

    let mut scrolled_list = list.clone();
    scrolled_list.scroll_offset = NativePoint { x: 0, y: 10 };
    let scrolled_surface = scrolled_list.rasterize().unwrap();
    assert_eq!(scrolled_surface.pixel(0, 17), Some([0, 0, 0, 255]));
    assert_eq!(scrolled_surface.pixel(0, 40), Some([0, 0, 0, 255]));
}

#[test]
fn native_text_double_decoration_preserves_style_inheritance_and_geometry() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:24px; line-height:20px; color:black; text-decoration:underline overline line-through; text-decoration-style:double; text-decoration-thickness:2px; text-underline-offset:1px; } #one { text-decoration-style:DoUbLe; text-decoration-thickness:1px; text-underline-offset:0px; } #two { text-decoration-style:double; text-decoration-thickness:2px; text-underline-offset:2px; } #parent { text-decoration-style:double; text-decoration-thickness:2px; }</style><div id='one' class='line'>AB</div><div id='two' class='line'>AB</div><div id='parent' class='line'><span id='inherited'>A</span><span id='override' style='text-decoration-style:solid'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let one = document.resolve_target("id=one").unwrap();
    let two = document.resolve_target("id=two").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let override_node = document.resolve_target("id=override").unwrap();
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                decoration_thickness,
                underline_offset,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *decoration_style,
                *decoration_thickness,
                *underline_offset,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let one_command = text_command(one).expect("one double-decoration command");
    let two_command = text_command(two).expect("two double-decoration command");
    let inherited_command = text_command(inherited).expect("inherited double-decoration command");
    let override_command = text_command(override_node).expect("override decoration command");

    assert_eq!(one_command.1, NativeTextDecorationStyle::Double);
    assert_eq!(one_command.2, 1);
    assert_eq!(one_command.3, 0);
    assert!(one_command.4 && one_command.5 && one_command.6);
    assert_eq!(two_command.1, NativeTextDecorationStyle::Double);
    assert_eq!(two_command.2, 2);
    assert_eq!(two_command.3, 2);
    assert!(two_command.4 && two_command.5 && two_command.6);
    assert_eq!(inherited_command.1, NativeTextDecorationStyle::Double);
    assert_eq!(inherited_command.2, 2);
    assert_eq!(inherited_command.3, 1);
    assert!(inherited_command.4 && inherited_command.5 && inherited_command.6);
    assert_eq!(override_command.1, NativeTextDecorationStyle::Solid);
    assert_eq!(override_command.2, 2);
    assert_eq!(override_command.3, 1);
    assert!(override_command.4 && override_command.5 && override_command.6);

    assert_eq!(
        layout.box_for(one),
        layout.box_for(two).map(|rect| NativeRect {
            y: rect.y.saturating_sub(24),
            ..rect
        })
    );
    let surface = list.rasterize().unwrap();
    let line_pixel =
        |command: (
            NativePoint,
            NativeTextDecorationStyle,
            u32,
            i32,
            bool,
            bool,
            bool,
        ),
         y: u32| surface.pixel(command.0.x, command.0.y.saturating_add(y));
    assert_eq!(line_pixel(one_command, 7), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(one_command, 8), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(one_command, 9), Some([0, 0, 0, 255]));
    for y in [9, 10, 12, 13] {
        assert_eq!(line_pixel(two_command, y), Some([0, 0, 0, 255]));
    }
    assert_eq!(line_pixel(two_command, 11), Some([255, 255, 255, 255]));
    assert_eq!(line_pixel(override_command, 10), Some([255, 255, 255, 255]));
}

#[test]
fn native_text_wavy_decoration_preserves_phase_inheritance_and_geometry() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:24px; line-height:20px; color:black; white-space:pre; text-decoration:underline; text-decoration-style:wavy; text-decoration-thickness:2px; text-underline-offset:1px; } #one { text-decoration-style:WaVy; text-decoration-thickness:1px; text-underline-offset:0px; } #two { text-decoration-style:wavy; text-decoration-thickness:2px; text-underline-offset:2px; } #parent { text-decoration-style:wavy; text-decoration-thickness:2px; }</style><div id='one' class='line'>        </div><div id='two' class='line'>        </div><div id='parent' class='line'><span id='inherited'>        </span><span id='override' style='text-decoration-style:solid'>        </span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let one = document.resolve_target("id=one").unwrap();
    let two = document.resolve_target("id=two").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let override_node = document.resolve_target("id=override").unwrap();
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                decoration_thickness,
                underline_offset,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *decoration_style,
                *decoration_thickness,
                *underline_offset,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let one_command = text_command(one).expect("one wavy-decoration command");
    let two_command = text_command(two).expect("two wavy-decoration command");
    let inherited_command = text_command(inherited).expect("inherited wavy-decoration command");
    let override_command = text_command(override_node).expect("override decoration command");

    for command in [
        one_command,
        two_command,
        inherited_command,
        override_command,
    ] {
        assert!(!command.5 && !command.6);
        assert!(command.4);
    }
    assert_eq!(one_command.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(one_command.2, 1);
    assert_eq!(one_command.3, 0);
    assert_eq!(two_command.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(two_command.2, 2);
    assert_eq!(two_command.3, 2);
    assert_eq!(inherited_command.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(inherited_command.2, 2);
    assert_eq!(inherited_command.3, 1);
    assert_eq!(override_command.1, NativeTextDecorationStyle::Solid);
    assert_eq!(override_command.2, 2);
    assert_eq!(override_command.3, 1);

    assert_eq!(
        layout.box_for(one),
        layout.box_for(two).map(|rect| NativeRect {
            y: rect.y.saturating_sub(24),
            ..rect
        })
    );
    let surface = list.rasterize().unwrap();
    let line_pixel = |command: (
        NativePoint,
        NativeTextDecorationStyle,
        u32,
        i32,
        bool,
        bool,
        bool,
    ),
                      x: u32,
                      y: u32| {
        surface.pixel(command.0.x.saturating_add(x), command.0.y.saturating_add(y))
    };
    for (x, y) in [
        (0, 7),
        (1, 8),
        (2, 9),
        (3, 8),
        (4, 7),
        (5, 6),
        (6, 5),
        (7, 6),
    ] {
        assert_eq!(line_pixel(one_command, x, y), Some([0, 0, 0, 255]));
    }
    for (x, top) in [
        (0, 9),
        (1, 10),
        (2, 11),
        (3, 10),
        (4, 9),
        (5, 8),
        (6, 7),
        (7, 8),
    ] {
        assert_eq!(line_pixel(two_command, x, top), Some([0, 0, 0, 255]));
        assert_eq!(line_pixel(two_command, x, top + 1), Some([0, 0, 0, 255]));
    }
    assert_eq!(line_pixel(inherited_command, 0, 8), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(inherited_command, 0, 9), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(override_command, 0, 8), Some([0, 0, 0, 255]));
    assert_eq!(line_pixel(override_command, 2, 8), Some([0, 0, 0, 255]));
    assert_eq!(
        line_pixel(override_command, 2, 7),
        Some([255, 255, 255, 255])
    );
}

#[test]
fn native_text_transform_aligns_layout_and_display_with_source_text_preserved() {
    let document = NativeDocument::parse(
        "<style>#upper { display:block; width:80px; text-transform:uppercase; } #lower { display:block; width:80px; text-transform:lowercase; } #parent { display:block; width:80px; text-transform:uppercase; } #clear { text-transform:none; } #contents { display:contents; text-transform:lowercase; } #pre { display:block; width:80px; white-space:pre-wrap; text-transform:uppercase; } #nowrap { display:block; text-transform:uppercase; white-space:nowrap; }</style><button id='upper'>aBc dEf</button><div id='lower'>aBc dEf</div><div id='parent'>One <span id='clear'>aBc</span> <span id='contents'><span id='nested'>aBc</span></span></div><div id='pre'>aB\ncD</div><div id='nowrap'>aB cD</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 140,
        device_scale_factor_milli: 1000,
    };
    let upper = document.resolve_target("id=upper").unwrap();
    let lower = document.resolve_target("id=lower").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let nowrap = document.resolve_target("id=nowrap").unwrap();

    let layout = document.layout(viewport).unwrap();
    let run_text = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(run_text(upper), vec!["ABC", " DEF"]);
    assert_eq!(run_text(lower), vec!["abc", " def"]);
    assert!(run_text(parent).contains(&"ONE"));
    assert_eq!(run_text(clear), vec!["aBc"]);
    assert_eq!(run_text(nested), vec!["abc"]);
    assert_eq!(run_text(pre), vec!["AB", "CD"]);
    assert_eq!(run_text(nowrap), vec!["AB CD"]);

    let (source_text, truncated) = document.visible_text(1024);
    assert!(!truncated);
    assert!(source_text.contains("aBc dEf"));
    assert!(!source_text.contains("ABC DEF"));
    assert!(document.resolve_target("text=aBc dEf").is_ok());

    let list = document.display_list(viewport).unwrap();
    for run in &layout.text_runs {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::TextRun { node_id, origin, text, .. }
                    if *node_id == run.node_id && *origin == run.origin && text == &run.text
            )
        }));
    }
}

#[test]
fn native_text_transform_feeds_text_fragments_without_mutating_evidence() {
    let html = "<div style='display:block;height:40px'>one</div><div id='target' style='display:block;height:40px;text-transform:uppercase;white-space:pre'>target phrase</div>";
    let viewport = Viewport {
        width: 240,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let expected_document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let expected_layout = expected_document.layout(viewport).unwrap();
    let target_run = expected_layout
        .text_runs
        .iter()
        .find(|run| run.text == "TARGET PHRASE")
        .unwrap();
    let expected_scroll = NativePoint {
        x: 0,
        y: expected_layout
            .box_for(target_run.node_id)
            .unwrap()
            .y
            .min(expected_layout.max_scroll_offset().y),
    };
    assert!(expected_scroll.y > 0);

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://text-transform-fragment", html)
        .unwrap()
        .with_initial_url("fixture://text-transform-fragment");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(engine.snapshot().unwrap().visible_text, "one target phrase");

    engine
        .navigate("fixture://text-transform-fragment#:~:text=TARGET%20PHRASE")
        .unwrap();
    assert_eq!(engine.scroll_offset(), expected_scroll);
    assert_eq!(engine.snapshot().unwrap().visible_text, "one target phrase");
}

#[test]
fn native_text_indent_shifts_only_block_first_lines_and_preserves_shared_consumers() {
    let document = NativeDocument::parse(
        "<style>#indented { display:block; width:48px; text-indent:16px; } #broken { display:block; width:48px; text-indent:16px; white-space:pre; } #inline-parent { display:block; width:48px; } #wide { display:block; width:48px; text-indent:16px; white-space:nowrap; }</style><div id='indented'>AB CD EF</div><div id='broken'>AB\nCD</div><div id='inline-parent'><span id='inline' style='text-indent:16px'>GH</span> IJ</div><div id='wide'>ABCDEFGH</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 100,
        device_scale_factor_milli: 1000,
    };
    let indented = document.resolve_target("id=indented").unwrap();
    let broken = document.resolve_target("id=broken").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let wide = document.resolve_target("id=wide").unwrap();
    let layout = document.layout(viewport).unwrap();

    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs_for(indented),
        vec![
            (NativePoint { x: 16, y: 0 }, "AB"),
            (NativePoint { x: 0, y: 20 }, "CD"),
            (NativePoint { x: 16, y: 20 }, " EF"),
        ]
    );
    assert_eq!(
        runs_for(broken),
        vec![
            (NativePoint { x: 16, y: 40 }, "AB"),
            (NativePoint { x: 0, y: 60 }, "CD"),
        ]
    );
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.node_id == inline && run.origin == NativePoint { x: 0, y: 80 })
    );
    assert_eq!(
        runs_for(wide),
        vec![(NativePoint { x: 16, y: 100 }, "ABCDEFGH")]
    );
    assert!(layout.content_width >= 80);
    assert!(layout.max_scroll_offset().x > 0);

    let list = document.display_list(viewport).unwrap();
    for run in &layout.text_runs {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::TextRun { node_id, origin, text, .. }
                    if *node_id == run.node_id && *origin == run.origin && text == &run.text
            )
        }));
    }
    assert_eq!(layout.hit_test(17, 1).unwrap(), Some(indented));
}

#[test]
fn native_word_spacing_shares_width_across_flow_paint_and_overflow() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:40px; word-spacing:4px; } #pre { display:block; width:32px; word-spacing:4px; white-space:pre-wrap; } #wide { display:block; width:32px; word-spacing:4px; white-space:nowrap; } #align { display:block; width:40px; word-spacing:4px; text-align:right; white-space:nowrap; } #parent { display:block; width:72px; word-spacing:4px; } #override { word-spacing:8px; }</style><div id='normal'>A   B C</div><div id='pre'>A  B</div><div id='wide'>A B C D E F G H</div><div id='align'>A B</div><div id='parent'>A <span id='override'>B C</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let wide = document.resolve_target("id=wide").unwrap();
    let align = document.resolve_target("id=align").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let override_id = document.resolve_target("id=override").unwrap();

    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs_for(normal),
        vec![
            (NativePoint { x: 0, y: 0 }, "A"),
            (NativePoint { x: 8, y: 0 }, " B"),
            (NativePoint { x: 0, y: 20 }, "C")
        ]
    );
    assert_eq!(
        runs_for(pre),
        vec![
            (NativePoint { x: 0, y: 40 }, "A  "),
            (NativePoint { x: 0, y: 60 }, "B")
        ]
    );

    let wide_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == wide)
        .unwrap();
    assert_eq!(wide_run.origin.x, 0);
    assert_eq!(wide_run.text, "A B C D E F G H");
    assert_eq!(layout.content_width, 148);
    assert_eq!(layout.max_scroll_offset().x, 68);

    let align_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == align)
        .unwrap();
    assert_eq!(align_run.origin.x, 12);
    assert_eq!(align_run.text, "A B");

    let parent_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == parent)
        .unwrap();
    let override_runs = layout
        .text_runs
        .iter()
        .filter(|run| run.node_id == override_id)
        .map(|run| (run.origin, run.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(parent_run.text, "A");
    assert_eq!(
        override_runs,
        vec![
            (NativePoint { x: 20, y: 120 }, "B"),
            (NativePoint { x: 28, y: 120 }, " C"),
        ]
    );

    let list = document.display_list(viewport).unwrap();
    let spacing_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                word_spacing,
                ..
            } if *command_node == node_id => Some(*word_spacing),
            _ => None,
        })
    };
    assert_eq!(spacing_for(normal), Some(4));
    assert_eq!(spacing_for(wide), Some(4));
    assert_eq!(spacing_for(override_id), Some(8));

    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(wide_run.origin.x + 16, wide_run.origin.y),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(wide_run.origin.x + 6, wide_run.origin.y),
        Some([255, 255, 255, 255])
    );
}

#[test]
fn native_letter_spacing_composes_with_word_spacing_across_consumers() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:48px; letter-spacing:2px; word-spacing:4px; } #pre { display:block; width:36px; letter-spacing:2px; word-spacing:4px; white-space:pre-wrap; } #wide { display:block; width:32px; letter-spacing:2px; word-spacing:4px; white-space:nowrap; } #align { display:block; width:48px; letter-spacing:2px; word-spacing:4px; text-align:right; white-space:nowrap; } #parent { display:block; width:80px; letter-spacing:2px; word-spacing:4px; } #override { letter-spacing:4px; }</style><div id='normal'>A B</div><div id='pre'>A  B</div><div id='wide'>A B C D E F G H</div><div id='align'>A B</div><div id='parent'>A <span id='override'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let pre = document.resolve_target("id=pre").unwrap();
    let wide = document.resolve_target("id=wide").unwrap();
    let align = document.resolve_target("id=align").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let override_id = document.resolve_target("id=override").unwrap();

    let layout = document.layout(viewport).unwrap();
    let runs_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs_for(normal),
        vec![
            (NativePoint { x: 0, y: 0 }, "A"),
            (NativePoint { x: 10, y: 0 }, " B")
        ]
    );
    assert_eq!(
        runs_for(pre),
        vec![
            (NativePoint { x: 0, y: 20 }, "A "),
            (NativePoint { x: 0, y: 40 }, " B")
        ]
    );

    let wide_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == wide)
        .unwrap();
    assert_eq!(wide_run.origin, NativePoint { x: 0, y: 60 });
    assert_eq!(wide_run.text, "A B C D E F G H");
    assert_eq!(layout.content_width, 178);
    assert_eq!(layout.max_scroll_offset().x, 98);

    let align_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == align)
        .unwrap();
    assert_eq!(align_run.origin, NativePoint { x: 14, y: 80 });
    assert_eq!(align_run.text, "A B");

    let parent_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == parent)
        .unwrap();
    let override_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == override_id)
        .unwrap();
    assert_eq!(parent_run.origin, NativePoint { x: 0, y: 100 });
    assert_eq!(parent_run.text, "A");
    assert_eq!(override_run.origin, NativePoint { x: 24, y: 100 });
    assert_eq!(override_run.text, "B");

    let list = document.display_list(viewport).unwrap();
    let spacing_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                letter_spacing,
                word_spacing,
                ..
            } if *command_node == node_id => Some((*letter_spacing, *word_spacing)),
            _ => None,
        })
    };
    assert_eq!(spacing_for(normal), Some((2, 4)));
    assert_eq!(spacing_for(wide), Some((2, 4)));
    assert_eq!(spacing_for(override_id), Some((4, 4)));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(24, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(10, 0), Some([255, 255, 255, 255]));
}

#[test]
fn native_font_weight_inherits_and_changes_only_fixed_cell_raster() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:16px; font-weight:400; } #bold { display:block; width:16px; font-weight:700; } #parent { display:block; width:48px; font-weight:bold; } #clear { font-weight:normal; } #numeric { font-weight:700; } #invalid { font-weight:500; }</style><div id='normal'>A</div><div id='bold'>A</div><div id='parent'>A<span id='clear'>B</span><span id='numeric'>C</span><span id='invalid'>D</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let bold = document.resolve_target("id=bold").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let numeric = document.resolve_target("id=numeric").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(normal).unwrap().width, 16);
    assert_eq!(layout.box_for(bold).unwrap().width, 16);
    assert_eq!(layout.box_for(normal).unwrap().height, 20);
    assert_eq!(layout.box_for(bold).unwrap().height, 20);
    assert_eq!(layout.box_for(normal).unwrap().y, 0);
    assert_eq!(layout.box_for(bold).unwrap().y, 20);
    assert_eq!(layout.box_for(parent).unwrap().y, 40);

    let list = document.display_list(viewport).unwrap();
    let weight_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                bold,
                ..
            } if *command_node == node_id => Some(*bold),
            _ => None,
        })
    };
    assert_eq!(weight_for(normal), Some(false));
    assert_eq!(weight_for(bold), Some(true));
    assert_eq!(weight_for(parent), Some(true));
    assert_eq!(weight_for(clear), Some(false));
    assert_eq!(weight_for(numeric), Some(true));
    assert_eq!(weight_for(invalid), Some(true));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(1, 21), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 21), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(6, 21), Some([255, 255, 255, 255]));
    assert_eq!(layout.max_scroll_offset().x, 0);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(normal));
    assert_eq!(layout.hit_test(1, 21).unwrap(), Some(bold));
}

#[test]
fn native_font_style_inherits_and_shears_only_fixed_cell_raster() {
    let document = NativeDocument::parse(
        "<style>#normal { display:block; width:16px; font-style:normal; } #italic { display:block; width:16px; font-style:italic; } #parent { display:block; width:48px; font-style:italic; } #clear { font-style:normal; } #invalid { font-style:oblique; }</style><div id='normal'>A</div><div id='italic'>A</div><div id='parent'>A<span id='clear'>B</span><span id='invalid'>C</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let italic = document.resolve_target("id=italic").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    let layout = document.layout(viewport).unwrap();
    let normal_box = layout.box_for(normal).unwrap();
    let italic_box = layout.box_for(italic).unwrap();
    assert_eq!(normal_box.width, italic_box.width);
    assert_eq!(normal_box.height, italic_box.height);
    assert_eq!(normal_box.x, italic_box.x);
    assert_eq!(layout.max_scroll_offset().x, 0);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(normal));
    assert_eq!(layout.hit_test(1, 21).unwrap(), Some(italic));

    let list = document.display_list(viewport).unwrap();
    let style_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                bold,
                italic,
                ..
            } if *command_node == node_id => Some((*bold, *italic)),
            _ => None,
        })
    };
    assert_eq!(style_for(normal), Some((false, false)));
    assert_eq!(style_for(italic), Some((false, true)));
    assert_eq!(style_for(parent), Some((false, true)));
    assert_eq!(style_for(clear), Some((false, false)));
    assert_eq!(style_for(invalid), Some((false, true)));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 20), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(3, 20), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 26), Some([0, 0, 0, 255]));
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
fn native_word_break_break_all_splits_collapsed_words_without_changing_semantics() {
    let document = NativeDocument::parse(
        "<div id='normal' style='display:block;width:40px;word-break:normal'>ABC DEFG</div><div id='break' style='display:block;width:40px;word-break:break-all'>ABC DEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let normal = document.resolve_target("id=normal").unwrap();
    let break_all = document.resolve_target("id=break").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(normal).unwrap().height, 40);
    assert_eq!(layout.box_for(break_all).unwrap().height, 40);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == normal)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 0 }, "ABC"),
            (NativePoint { x: 0, y: 20 }, "DEFG"),
        ]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == break_all)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (NativePoint { x: 0, y: 40 }, "ABC"),
            (NativePoint { x: 24, y: 40 }, " D"),
            (NativePoint { x: 0, y: 60 }, "EFG"),
        ]
    );

    let (visible_text, truncated) = document.visible_text(1024);
    assert!(!truncated);
    assert!(visible_text.contains("ABC DEFG"));

    let modes = NativeDocument::parse(
        "<div id='pre' style='display:block;width:24px;white-space:pre;word-break:break-all'>ABC DEFG</div><div id='nowrap' style='display:block;width:24px;white-space:nowrap;word-break:break-all'>ABC DEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let mode_layout = modes.layout(viewport).unwrap();
    for (id, y) in [("pre", 0), ("nowrap", 20)] {
        let node_id = modes.resolve_target(&format!("id={id}")).unwrap();
        assert_eq!(mode_layout.box_for(node_id).unwrap().height, 20);
        assert_eq!(
            mode_layout
                .text_runs
                .iter()
                .filter(|run| run.node_id == node_id)
                .map(|run| (run.origin, run.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(NativePoint { x: 0, y }, "ABC DEFG")]
        );
    }
}

#[test]
fn native_text_overflow_ellipsis_truncates_only_eligible_clipped_nowrap_text() {
    let document = NativeDocument::parse(
        "<div id='clip' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:clip'>ABCDEFG</div><div id='ellipsis' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'>ABCDEFG</div><div id='fit' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'>ABC</div><div id='narrow' style='display:block;width:16px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'>ABCDEFG</div><div id='nested' style='display:block;width:40px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis'><span id='nested-child'>ABCDEFG</span></div><div id='styled' style='display:block;width:56px;white-space:nowrap;overflow-x:clip;text-overflow:ellipsis;text-transform:uppercase;word-spacing:4px;letter-spacing:2px;font-weight:bold;font-style:italic;text-decoration:underline'>ab cd ef</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 128,
        device_scale_factor_milli: 1000,
    };
    let clip = document.resolve_target("id=clip").unwrap();
    let ellipsis = document.resolve_target("id=ellipsis").unwrap();
    let fit = document.resolve_target("id=fit").unwrap();
    let narrow = document.resolve_target("id=narrow").unwrap();
    let nested_child = document.resolve_target("id=nested-child").unwrap();
    let styled = document.resolve_target("id=styled").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(clip).unwrap().height, 20);
    assert_eq!(layout.box_for(ellipsis).unwrap().height, 20);
    assert_eq!(layout.box_for(fit).unwrap().height, 20);
    assert_eq!(layout.box_for(narrow).unwrap().height, 20);
    assert_eq!(layout.max_scroll_offset().x, 0);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == clip)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 0 }, "ABCDEFG", false)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == ellipsis)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 20 }, "AB...", true)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == fit)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 40 }, "ABC", false)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == narrow)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 60 }, "AB", true)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == nested_child)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 80 }, "ABCDEFG", false)]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == styled)
            .map(|run| (run.origin, run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 100 }, "AB...", true)]
    );

    let (visible_text, truncated) = document.visible_text(1024);
    assert!(!truncated);
    assert_eq!(visible_text, "ABCDEFG ABCDEFG ABC ABCDEFG ABCDEFG ab cd ef");

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                origin,
                text,
                truncated,
                ..
            } if *node_id == ellipsis
                && *origin == NativePoint { x: 0, y: 20 }
                && text == "AB..."
                && *truncated
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun {
                node_id,
                text,
                truncated,
                underline,
                bold,
                italic,
                word_spacing,
                letter_spacing,
                ..
            } if *node_id == styled
                && text == "AB..."
                && *truncated
                && *underline
                && *bold
                && *italic
                && *word_spacing == 4
                && *letter_spacing == 2
        )
    }));

    let visible_overflow = NativeDocument::parse(
        "<div id='visible' style='display:block;width:40px;white-space:nowrap;text-overflow:ellipsis'>ABCDEFG</div><div id='normal' style='display:block;width:40px;text-overflow:ellipsis'>ABC DEFG</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let visible_layout = visible_overflow.layout(viewport).unwrap();
    let visible = visible_overflow.resolve_target("id=visible").unwrap();
    let normal = visible_overflow.resolve_target("id=normal").unwrap();
    assert_eq!(
        visible_layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == visible)
            .map(|run| (run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![("ABCDEFG", false)]
    );
    assert_eq!(
        visible_layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == normal)
            .map(|run| (run.text.as_str(), run.truncated))
            .collect::<Vec<_>>(),
        vec![("ABC", false), ("DEFG", false)]
    );
}

#[test]
fn native_vertical_align_moves_inline_items_and_text_within_fixed_line_box() {
    let document = NativeDocument::parse(
        "<style>#flow { display:block; width:80px; line-height:40px; } .item { display:inline-block; width:8px; height:10px; } #base { vertical-align:baseline; } #top { vertical-align:top; } #middle { vertical-align:middle; } #bottom { vertical-align:bottom; } #text-bottom { vertical-align:bottom; }</style><div id='flow'><span id='base' class='item'></span><span id='top' class='item'></span><span id='middle' class='item'></span><span id='bottom' class='item'></span><span id='text-bottom' class='item'>B</span></div><div id='direct' style='line-height:40px'>D</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 96,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let flow = document.resolve_target("id=flow").unwrap();
    let base = document.resolve_target("id=base").unwrap();
    let top = document.resolve_target("id=top").unwrap();
    let middle = document.resolve_target("id=middle").unwrap();
    let bottom = document.resolve_target("id=bottom").unwrap();
    let text_bottom = document.resolve_target("id=text-bottom").unwrap();
    let direct = document.resolve_target("id=direct").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(flow).unwrap().height, 40);
    for (node_id, x, y) in [
        (base, 0, 0),
        (top, 8, 0),
        (middle, 16, 15),
        (bottom, 24, 30),
        (text_bottom, 32, 30),
    ] {
        assert_eq!(
            layout
                .box_for(node_id)
                .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
            Some((x, y, 8, 10))
        );
    }
    assert_eq!(layout.box_for(direct).unwrap().y, 40);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == text_bottom)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 32, y: 30 }, "B")]
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == direct)
            .map(|run| (run.origin, run.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(NativePoint { x: 0, y: 40 }, "D")]
    );
}

#[test]
fn native_flex_row_places_eligible_element_children_in_source_order() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:40px;height:20px'> \n<div id='first' style='display:block;width:8px;height:6px;margin:1px;background-color:red'>A</div> \n<button id='second' style='display:block;width:12px;height:10px;margin:2px;background-color:blue'>B</button> \n<span id='third' style='display:inline-block;width:8px;height:8px;margin:1px;background-color:green'>C</span> \n</div><div id='below' style='height:8px'>Below</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let below = document.resolve_target("id=below").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().height, 20);
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
            x: 12,
            y: 2,
            width: 12,
            height: 10,
        })
    );
    assert_eq!(
        layout.box_for(third),
        Some(NativeRect {
            x: 27,
            y: 1,
            width: 8,
            height: 8,
        })
    );
    assert_eq!(layout.box_for(below).unwrap().y, 20);
    assert_eq!(layout.hit_test(13, 3).unwrap(), Some(second));

    let text_origins = layout
        .text_runs
        .iter()
        .map(|run| (run.node_id, run.origin, run.text.as_str()))
        .collect::<Vec<_>>();
    assert!(text_origins.contains(&(first, NativePoint { x: 1, y: 1 }, "A")));
    assert!(text_origins.contains(&(second, NativePoint { x: 12, y: 2 }, "B")));
    assert!(text_origins.contains(&(third, NativePoint { x: 27, y: 1 }, "C")));
}

#[test]
fn native_flex_row_auto_margins_distribute_main_and_cross_space_with_artifacts() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:41px;height:20px;justify-content:space-between;align-items:flex-end'><div id='first' style='width:8px;height:6px;background-color:red'></div><div id='second' style='width:8px;height:8px;margin-left:auto;background-color:blue'></div><div id='third' style='width:8px;height:4px;margin:auto;background-color:green'><span id='nested' style='display:block;height:2px'></span></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(row),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 41,
            height: 20
        })
    );
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 0,
            y: 14,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 14,
            y: 12,
            width: 8,
            height: 8,
        })
    );
    assert_eq!(
        layout.box_for(third),
        Some(NativeRect {
            x: 28,
            y: 8,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(
        layout.box_for(nested),
        Some(NativeRect {
            x: 28,
            y: 8,
            width: 8,
            height: 2,
        })
    );
    assert_eq!(layout.hit_test(29, 9).unwrap(), Some(nested));

    let list = document.display_list(viewport).unwrap();
    for (node_id, color, x, y) in [
        (first, NativeColor::RED, 0, 14),
        (
            second,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
            14,
            12,
        ),
        (
            third,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },
            28,
            8,
        ),
    ] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id: painted_id, rect, color: painted_color, .. }
                    if *painted_id == node_id
                        && rect.x == x
                        && rect.y == y
                        && *painted_color == color
            )
        }));
    }
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(7, 19), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(21, 19), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(35, 11), Some([0, 128, 0, 255]));
}

#[test]
fn native_flex_row_reverse_auto_margins_keep_physical_edges_and_order() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:41px;height:12px;flex-direction:row-reverse;justify-content:flex-start;align-items:flex-start'><div id='first' style='width:8px;height:6px;margin-right:auto;background-color:red'></div><div id='second' style='width:8px;height:6px;margin-left:auto;background-color:blue'></div><div id='third' style='width:8px;height:6px;margin:auto;background-color:green'><span id='nested' style='display:block;height:2px'></span></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 28,
            y: 0,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 20,
            y: 0,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(third),
        Some(NativeRect {
            x: 4,
            y: 3,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(nested).map(|rect| (rect.x, rect.y)),
        Some((4, 3))
    );
    assert_eq!(layout.hit_test(5, 4).unwrap(), Some(nested));
}

#[test]
fn native_flex_column_auto_margins_map_main_cross_axes_and_reverse() {
    let viewport = Viewport {
        width: 36,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    for (direction, second_margin, expected) in [
        (
            "column",
            "margin-top:auto",
            [(24, 0, 6, 8), (22, 15, 8, 8), (12, 30, 6, 4)],
        ),
        (
            "column-reverse",
            "margin-bottom:auto",
            [(24, 33, 6, 8), (22, 18, 8, 8), (12, 7, 6, 4)],
        ),
    ] {
        let html = format!(
            "<div id='column' style='display:flex;width:30px;height:41px;flex-direction:{direction};justify-content:space-between;align-items:flex-end'><div id='first' style='width:6px;height:8px;background-color:red'></div><div id='second' style='width:8px;height:8px;{second_margin};background-color:blue'></div><div id='third' style='width:6px;height:4px;margin:auto;background-color:green'><span id='nested' style='display:block;height:2px'></span></div></div>"
        );
        let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
        let column = document.resolve_target("id=column").unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(
            layout.box_for(column),
            Some(NativeRect {
                x: 0,
                y: 0,
                width: 30,
                height: 41
            })
        );
        for (node_id, (x, y, width, height)) in [
            (first, expected[0]),
            (second, expected[1]),
            (third, expected[2]),
        ] {
            assert_eq!(
                layout.box_for(node_id),
                Some(NativeRect {
                    x,
                    y,
                    width,
                    height,
                })
            );
        }
        assert_eq!(
            layout.box_for(nested).map(|rect| (rect.x, rect.y)),
            Some((expected[2].0, expected[2].1))
        );
        assert_eq!(
            layout
                .hit_test(i64::from(expected[2].0 + 1), i64::from(expected[2].1 + 1))
                .unwrap(),
            Some(nested)
        );

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == third
                        && rect.x == expected[2].0
                        && rect.y == expected[2].1
                        && *color == NativeColor { red: 0, green: 128, blue: 0, alpha: 255 }
            )
        }));
    }
}

#[test]
fn native_flex_auto_margins_preserve_overflow_and_resolve_wrapped_lines() {
    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:10px;height:8px'><div id='first' style='width:8px;height:4px;flex-shrink:0'></div><div id='second' style='width:8px;height:4px;margin-left:auto;flex-shrink:0'></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = overflow.resolve_target("id=first").unwrap();
    let second = overflow.resolve_target("id=second").unwrap();
    let overflow_layout = overflow
        .layout(Viewport {
            width: 10,
            height: 12,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(overflow_layout.box_for(first).unwrap().x, 0);
    assert_eq!(overflow_layout.box_for(second).unwrap().x, 8);
    assert_eq!(overflow_layout.content_width, 16);
    assert_eq!(overflow_layout.max_scroll_offset().x, 6);

    let with_auto = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;flex-wrap:wrap'><div id='first' style='width:8px;height:4px'>A</div><div id='second' style='width:8px;height:4px;margin-left:auto'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let without_auto = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;flex-wrap:wrap'><div id='first' style='width:8px;height:4px'>A</div><div id='second' style='width:8px;height:4px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let with_layout = with_auto
        .layout(Viewport {
            width: 16,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let without_layout = without_auto
        .layout(Viewport {
            width: 16,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let with_first = with_auto.resolve_target("id=first").unwrap();
    let with_second = with_auto.resolve_target("id=second").unwrap();
    let without_first = without_auto.resolve_target("id=first").unwrap();
    let without_second = without_auto.resolve_target("id=second").unwrap();
    assert_eq!(with_layout.box_for(with_first).unwrap().x, 0);
    assert_eq!(with_layout.box_for(with_second).unwrap().x, 4);
    assert_eq!(without_layout.box_for(without_first).unwrap().x, 0);
    assert_eq!(without_layout.box_for(without_second).unwrap().x, 0);
    assert_eq!(with_layout.box_for(with_second).unwrap().y, 4);
    assert_eq!(without_layout.box_for(without_second).unwrap().y, 4);
}

#[test]
fn native_flex_wrapped_row_auto_margins_resolve_per_line_and_wrap_reverse() {
    let viewport = Viewport {
        width: 32,
        height: 36,
        device_scale_factor_milli: 1000,
    };

    for direction in ["row", "row-reverse"] {
        for wrap in ["wrap", "wrap-reverse"] {
            let html = format!(
                "<div id='row' style='display:flex;width:21px;height:31px;gap:2px;flex-direction:{direction};flex-wrap:{wrap};align-items:flex-start;align-content:stretch'><div id='first' style='width:8px;height:6px;background-color:red'>A</div><button id='second' style='width:8px;height:6px;margin-left:auto;margin-top:auto;background-color:blue'><span id='nested' style='display:block;height:2px'>B</span></button><div id='third' style='width:8px;height:4px;margin-left:auto;margin-right:auto;background-color:green'>C</div></div>"
            );
            let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
            let row = document.resolve_target("id=row").unwrap();
            let first = document.resolve_target("id=first").unwrap();
            let second = document.resolve_target("id=second").unwrap();
            let nested = document.resolve_target("id=nested").unwrap();
            let third = document.resolve_target("id=third").unwrap();
            let layout = document.layout(viewport).unwrap();

            let (first_x, second_x) = if direction == "row" { (0, 13) } else { (13, 3) };
            let third_x = if direction == "row" { 7 } else { 6 };
            let (first_y, second_y, third_y) = if wrap == "wrap" {
                (0, 10, 18)
            } else {
                (15, 25, 0)
            };
            assert_eq!(
                layout.box_for(row),
                Some(NativeRect {
                    x: 0,
                    y: 0,
                    width: 21,
                    height: 31,
                })
            );
            assert_eq!(
                layout.box_for(first),
                Some(NativeRect {
                    x: first_x,
                    y: first_y,
                    width: 8,
                    height: 6,
                })
            );
            assert_eq!(
                layout.box_for(second),
                Some(NativeRect {
                    x: second_x,
                    y: second_y,
                    width: 8,
                    height: 6,
                })
            );
            assert_eq!(
                layout.box_for(third),
                Some(NativeRect {
                    x: third_x,
                    y: third_y,
                    width: 8,
                    height: 4,
                })
            );
            assert_eq!(
                layout.box_for(nested).map(|rect| (rect.x, rect.y)),
                Some((second_x, second_y))
            );
            assert_eq!(
                layout
                    .hit_test(i64::from(second_x + 1), i64::from(second_y + 1))
                    .unwrap(),
                Some(nested)
            );

            let list = document.display_list(viewport).unwrap();
            assert!(list.commands.iter().any(|command| {
                matches!(
                    command,
                    NativeDisplayCommand::FillRect {
                        node_id,
                        rect,
                        color,
                        ..
                    } if *node_id == second
                        && rect.x == second_x
                        && rect.y == second_y
                        && *color == NativeColor {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        }
                )
            }));
            let surface = list.rasterize().unwrap();
            assert_eq!(
                surface.pixel(second_x + 1, second_y + 1),
                Some([0, 0, 255, 255])
            );
        }
    }

    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:10px;flex-wrap:wrap'><div id='wide' style='width:16px;height:4px;margin-left:auto;flex-shrink:0;background-color:red'>W</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let wide = overflow.resolve_target("id=wide").unwrap();
    let overflow_layout = overflow
        .layout(Viewport {
            width: 10,
            height: 12,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(overflow_layout.box_for(wide).unwrap().x, 0);
    assert_eq!(overflow_layout.content_width, 16);
    assert_eq!(overflow_layout.max_scroll_offset().x, 6);
}

#[test]
fn native_flex_wrapped_column_auto_margins_map_lines_reverse_and_artifacts() {
    let viewport = Viewport {
        width: 36,
        height: 24,
        device_scale_factor_milli: 1000,
    };

    for direction in ["column", "column-reverse"] {
        for wrap in ["wrap", "wrap-reverse"] {
            let html = format!(
                "<div id='column' style='display:flex;width:31px;height:21px;flex-direction:{direction};flex-wrap:{wrap};row-gap:2px;column-gap:3px;align-items:flex-start;align-content:stretch'><div id='first' style='width:6px;height:8px;background-color:red'>A</div><button id='second' style='width:8px;height:8px;margin-top:auto;margin-left:auto;background-color:blue'><span id='nested' style='display:block;height:2px'>B</span></button><div id='third' style='width:10px;height:6px;margin:auto;background-color:green'>C</div></div>"
            );
            let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
            let column = document.resolve_target("id=column").unwrap();
            let first = document.resolve_target("id=first").unwrap();
            let second = document.resolve_target("id=second").unwrap();
            let nested = document.resolve_target("id=nested").unwrap();
            let third = document.resolve_target("id=third").unwrap();
            let layout = document.layout(viewport).unwrap();

            let line_one_x = if wrap == "wrap" { 0 } else { 18 };
            let line_two_x = if wrap == "wrap" { 16 } else { 0 };
            let first_x = if wrap == "wrap" {
                line_one_x
            } else {
                line_one_x + 7
            };
            let second_x = line_one_x + 5;
            let third_x = line_two_x + 3;
            let (first_y, second_y, third_y) = if direction == "column" {
                (0, 13, 8)
            } else {
                (13, 3, 7)
            };
            assert_eq!(
                layout.box_for(column),
                Some(NativeRect {
                    x: 0,
                    y: 0,
                    width: 31,
                    height: 21,
                })
            );
            assert_eq!(
                layout.box_for(first),
                Some(NativeRect {
                    x: first_x,
                    y: first_y,
                    width: 6,
                    height: 8,
                })
            );
            assert_eq!(
                layout.box_for(second),
                Some(NativeRect {
                    x: second_x,
                    y: second_y,
                    width: 8,
                    height: 8,
                })
            );
            assert_eq!(
                layout.box_for(third),
                Some(NativeRect {
                    x: third_x,
                    y: third_y,
                    width: 10,
                    height: 6,
                })
            );
            assert_eq!(
                layout.box_for(nested).map(|rect| (rect.x, rect.y)),
                Some((second_x, second_y))
            );
            assert_eq!(
                layout
                    .hit_test(i64::from(second_x + 1), i64::from(second_y + 1))
                    .unwrap(),
                Some(nested)
            );

            let list = document.display_list(viewport).unwrap();
            assert!(list.commands.iter().any(|command| {
                matches!(
                    command,
                    NativeDisplayCommand::FillRect {
                        node_id,
                        rect,
                        color,
                        ..
                    } if *node_id == third
                        && rect.x == third_x
                        && rect.y == third_y
                        && *color == NativeColor { red: 0, green: 128, blue: 0, alpha: 255 }
                )
            }));
            let surface = list.rasterize().unwrap();
            assert_eq!(
                surface.pixel(second_x + 1, second_y + 1),
                Some([0, 0, 255, 255])
            );
        }
    }

    let auto_height = NativeDocument::parse(
        "<div id='column' style='display:flex;width:20px;flex-direction:column;flex-wrap:wrap'><div id='item' style='width:8px;height:4px;margin-left:auto'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let item = auto_height.resolve_target("id=item").unwrap();
    assert_eq!(
        auto_height
            .layout(viewport)
            .unwrap()
            .box_for(item)
            .unwrap()
            .x,
        0
    );
}

#[test]
fn native_flex_direction_maps_rows_without_reordering_or_split_artifacts() {
    let viewport = Viewport {
        width: 36,
        height: 20,
        device_scale_factor_milli: 1000,
    };

    for direction in ["ltr", "rtl"] {
        for flex_direction in ["row", "row-reverse"] {
            let html = format!(
                "<div id='row' style='display:flex;width:30px;height:12px;direction:{direction};flex-direction:{flex_direction};justify-content:flex-start;align-items:flex-start'><div id='first' style='width:8px;height:6px;background-color:red'><span id='nested' style='display:block;width:2px;height:2px'>A</span></div><div id='second' style='width:8px;height:6px;background-color:blue'>B</div></div>"
            );
            let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
            let first = document.resolve_target("id=first").unwrap();
            let second = document.resolve_target("id=second").unwrap();
            let nested = document.resolve_target("id=nested").unwrap();
            let layout = document.layout(viewport).unwrap();
            let physical_reverse = (direction == "rtl") ^ (flex_direction == "row-reverse");
            let first_x = if physical_reverse { 22 } else { 0 };
            let second_x = if physical_reverse { 14 } else { 8 };

            assert_eq!(layout.box_for(first).unwrap().x, first_x);
            assert_eq!(layout.box_for(second).unwrap().x, second_x);
            assert_eq!(
                layout.box_for(nested).map(|rect| (rect.x, rect.y)),
                Some((first_x, 0))
            );
            assert_eq!(
                layout.hit_test(i64::from(first_x + 1), 1).unwrap(),
                Some(nested)
            );

            let list = document.display_list(viewport).unwrap();
            assert!(list.commands.iter().any(|command| {
                matches!(
                    command,
                    NativeDisplayCommand::FillRect {
                        node_id,
                        rect,
                        color,
                        ..
                    } if *node_id == first
                        && rect.x == first_x
                        && rect.y == 0
                        && *color == NativeColor { red: 255, green: 0, blue: 0, alpha: 255 }
                )
            }));
            let surface = list.rasterize().unwrap();
            assert_eq!(surface.pixel(first_x + 1, 1), Some([255, 0, 0, 255]));
            assert_eq!(document.visible_text(1024).0, "A B");
        }
    }
}

#[test]
fn native_flex_direction_maps_physical_auto_margins_with_rows() {
    for direction in ["ltr", "rtl"] {
        let html = format!(
            "<div id='row' style='display:flex;width:20px;height:10px;direction:{direction};flex-direction:row;justify-content:flex-start'><div id='first' style='width:6px;height:6px;margin-left:auto;flex-shrink:0'>A</div><div id='second' style='width:6px;height:6px;flex-shrink:0'>B</div></div>"
        );
        let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let layout = document
            .layout(Viewport {
                width: 24,
                height: 16,
                device_scale_factor_milli: 1000,
            })
            .unwrap();
        let expected = if direction == "ltr" { (8, 14) } else { (14, 0) };
        assert_eq!(layout.box_for(first).unwrap().x, expected.0);
        assert_eq!(layout.box_for(second).unwrap().x, expected.1);
    }
}

#[test]
fn native_flex_direction_maps_column_cross_start_and_wrapped_line_stacking() {
    let viewport = Viewport {
        width: 32,
        height: 24,
        device_scale_factor_milli: 1000,
    };

    for direction in ["ltr", "rtl"] {
        for flex_direction in ["column", "column-reverse"] {
            let html = format!(
                "<div id='column' style='display:flex;width:20px;height:20px;direction:{direction};flex-direction:{flex_direction};align-items:flex-start'><div id='first' style='width:6px;height:6px;background-color:red'>A</div><div id='second' style='width:8px;height:6px;background-color:blue'>B</div></div>"
            );
            let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
            let first = document.resolve_target("id=first").unwrap();
            let second = document.resolve_target("id=second").unwrap();
            let layout = document.layout(viewport).unwrap();
            let expected_x = if direction == "rtl" { (14, 12) } else { (0, 0) };
            let expected_y = if flex_direction == "column-reverse" {
                (14, 8)
            } else {
                (0, 6)
            };
            assert_eq!(layout.box_for(first).unwrap().x, expected_x.0);
            assert_eq!(layout.box_for(second).unwrap().x, expected_x.1);
            assert_eq!(layout.box_for(first).unwrap().y, expected_y.0);
            assert_eq!(layout.box_for(second).unwrap().y, expected_y.1);
        }

        for wrap in ["wrap", "wrap-reverse"] {
            let html = format!(
                "<div id='column' style='display:flex;width:24px;height:12px;direction:{direction};flex-direction:column;flex-wrap:{wrap};row-gap:2px;column-gap:3px;align-items:flex-start;align-content:flex-start'><div id='first' style='width:4px;height:6px;background-color:red'>A</div><div id='second' style='width:4px;height:6px;background-color:blue'>B</div><div id='third' style='width:4px;height:6px;background-color:green'>C</div></div>"
            );
            let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
            let first = document.resolve_target("id=first").unwrap();
            let second = document.resolve_target("id=second").unwrap();
            let third = document.resolve_target("id=third").unwrap();
            let layout = document.layout(viewport).unwrap();
            let reflected = (direction == "rtl") ^ (wrap == "wrap-reverse");
            let expected_x = if reflected { [20, 13, 6] } else { [0, 7, 14] };
            assert_eq!(layout.box_for(first).unwrap().x, expected_x[0]);
            assert_eq!(layout.box_for(second).unwrap().x, expected_x[1]);
            assert_eq!(layout.box_for(third).unwrap().x, expected_x[2]);
            assert_eq!(document.visible_text(1024).0, "A B C");
        }
    }
}

#[test]
fn native_direction_keeps_non_flex_flow_and_source_text_order_bounded() {
    let viewport = Viewport {
        width: 32,
        height: 20,
        device_scale_factor_milli: 1000,
    };
    let ltr = NativeDocument::parse(
        "<div id='root' style='width:20px;height:12px'><div id='first' style='width:6px;height:4px;background-color:red'>A</div><div id='second' style='width:8px;height:4px;background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let rtl = NativeDocument::parse(
        "<div id='root' style='direction:rtl;width:20px;height:12px'><div id='first' style='width:6px;height:4px;background-color:red'>A</div><div id='second' style='width:8px;height:4px;background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let ltr_first = ltr.resolve_target("id=first").unwrap();
    let ltr_second = ltr.resolve_target("id=second").unwrap();
    let rtl_first = rtl.resolve_target("id=first").unwrap();
    let rtl_second = rtl.resolve_target("id=second").unwrap();
    let ltr_layout = ltr.layout(viewport).unwrap();
    let rtl_layout = rtl.layout(viewport).unwrap();

    assert_eq!(ltr_layout.box_for(ltr_first), rtl_layout.box_for(rtl_first));
    assert_eq!(
        ltr_layout.box_for(ltr_second),
        rtl_layout.box_for(rtl_second)
    );
    assert_eq!(ltr.visible_text(1024).0, "A B");
    assert_eq!(rtl.visible_text(1024).0, "A B");
}

#[test]
fn native_flex_row_preserves_fixed_width_overflow_and_fallback_content() {
    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:16px'><div id='first' style='width:12px;height:8px;flex-shrink:0'>A</div><div id='second' style='width:12px;height:8px;flex-shrink:0'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 16,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let second = overflow.resolve_target("id=second").unwrap();
    let layout = overflow.layout(viewport).unwrap();
    assert_eq!(layout.box_for(second).unwrap().x, 12);
    assert_eq!(layout.content_width, 24);
    assert_eq!(layout.max_scroll_offset().x, 8);

    let fallback = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px'><span id='item' style='display:block;width:8px;height:8px'>A</span> meaningful text</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let fallback_layout = fallback
        .layout(Viewport {
            width: 48,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let item = fallback.resolve_target("id=item").unwrap();
    assert_eq!(fallback_layout.box_for(item).unwrap().y, 0);
    assert!(
        fallback_layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("mean"))
    );
    assert!(
        fallback_layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("text"))
    );
}

#[test]
fn native_flex_row_applies_gap_between_rendered_items_and_ignores_fallback_rows() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:16px;gap:4px'><div id='first' style='width:8px;height:8px;flex-shrink:0'>A</div><div id='hidden' style='display:none;width:8px;height:8px'>H</div><div id='second' style='width:8px;height:8px;flex-shrink:0'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 16,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let first = document.resolve_target("id=first").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(first).unwrap().x, 0);
    assert_eq!(layout.box_for(second).unwrap().x, 12);
    assert_eq!(layout.box_for(hidden), None);
    assert_eq!(layout.content_width, 20);
    assert_eq!(layout.max_scroll_offset().x, 4);
    assert_eq!(layout.hit_test(13, 1).unwrap(), Some(second));

    let with_gap = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px;gap:8px'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let without_gap = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px;gap:0px'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let with_gap_first = with_gap.resolve_target("id=first").unwrap();
    let with_gap_second = with_gap.resolve_target("id=second").unwrap();
    let without_gap_first = without_gap.resolve_target("id=first").unwrap();
    let without_gap_second = without_gap.resolve_target("id=second").unwrap();
    let with_gap_layout = with_gap
        .layout(Viewport {
            width: 48,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let without_gap_layout = without_gap
        .layout(Viewport {
            width: 48,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(
        with_gap_layout.box_for(with_gap_first),
        without_gap_layout.box_for(without_gap_first)
    );
    assert_eq!(
        with_gap_layout.box_for(with_gap_second),
        without_gap_layout.box_for(without_gap_second)
    );
    assert!(
        with_gap_layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("mean"))
    );
    assert!(
        with_gap_layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("text"))
    );
}

#[test]
fn native_flex_row_gap_separates_wrapped_lines_before_alignment() {
    let source = |align_content: &str| {
        format!(
            "<div id='row' style='display:flex;width:20px;height:60px;gap:2px;row-gap:4px;flex-wrap:wrap;align-items:center;align-content:{align_content}'><div id='first' style='width:8px;height:6px;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></div><div id='second' style='width:8px;height:10px;background-color:green'>S</div><div id='third' style='width:8px;height:14px;background-color:blue'>T</div></div>"
        )
    };
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };

    for (align_content, expected) in [
        ("flex-start", (2, 0, 14)),
        ("center", (18, 16, 30)),
        ("flex-end", (34, 32, 46)),
        ("space-between", (2, 0, 46)),
        ("space-around", (10, 8, 38)),
        ("space-evenly", (12, 10, 35)),
        ("stretch", (10, 8, 38)),
        ("normal", (10, 8, 38)),
    ] {
        let document =
            NativeDocument::parse(&source(align_content), &NativeEngineLimits::default()).unwrap();
        let row = document.resolve_target("id=row").unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(layout.box_for(row).unwrap().height, 60);
        assert_eq!(layout.box_for(first).unwrap().y, expected.0);
        assert_eq!(layout.box_for(nested).unwrap().y, expected.0);
        assert_eq!(layout.box_for(second).unwrap().y, expected.1);
        assert_eq!(layout.box_for(third).unwrap().y, expected.2);
        assert_eq!(layout.content_height, 64);
        assert_eq!(layout.max_scroll_offset(), NativePoint { x: 0, y: 0 });
        assert_eq!(
            layout.hit_test(1, (expected.0 + 1).into()).unwrap(),
            Some(nested)
        );

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == first && rect.y == expected.0 && *color == NativeColor::RED
            )
        }));
    }
}

#[test]
fn native_flex_row_gap_reflects_with_wrap_reverse_and_preserves_single_line_fallback() {
    let source = |align_content: &str| {
        format!(
            "<div id='row' style='display:flex;width:20px;height:60px;gap:2px;row-gap:4px;flex-wrap:wrap-reverse;align-items:center;align-content:{align_content}'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>"
        )
    };
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };

    for (align_content, expected) in [
        ("flex-start", (52, 50, 32)),
        ("stretch", (44, 42, 8)),
        ("normal", (44, 42, 8)),
    ] {
        let document =
            NativeDocument::parse(&source(align_content), &NativeEngineLimits::default()).unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(layout.box_for(first).unwrap().y, expected.0);
        assert_eq!(layout.box_for(second).unwrap().y, expected.1);
        assert_eq!(layout.box_for(third).unwrap().y, expected.2);
        assert_eq!(
            layout.hit_test(1, (expected.2 + 1).into()).unwrap(),
            Some(third)
        );
    }

    let single_with_gap = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;height:60px;row-gap:9px;flex-wrap:wrap;align-content:center'><div id='item' style='width:8px;height:6px'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let single_without_gap = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;height:60px;flex-wrap:wrap;align-content:center'><div id='item' style='width:8px;height:6px'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let with_gap_item = single_with_gap.resolve_target("id=item").unwrap();
    let without_gap_item = single_without_gap.resolve_target("id=item").unwrap();
    assert_eq!(
        single_with_gap
            .layout(viewport)
            .unwrap()
            .box_for(with_gap_item),
        single_without_gap
            .layout(viewport)
            .unwrap()
            .box_for(without_gap_item)
    );
}

#[test]
fn native_flex_row_gap_contributes_to_auto_and_undersized_cross_axis_overflow() {
    let auto = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;row-gap:4px;flex-wrap:wrap'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let auto_row = auto.resolve_target("id=row").unwrap();
    let auto_third = auto.resolve_target("id=third").unwrap();
    let auto_layout = auto
        .layout(Viewport {
            width: 24,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(auto_layout.box_for(auto_row).unwrap().height, 28);
    assert_eq!(auto_layout.box_for(auto_third).unwrap().y, 14);

    let small = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;height:12px;row-gap:4px;flex-wrap:wrap;align-content:space-evenly'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let small_row = small.resolve_target("id=row").unwrap();
    let small_third = small.resolve_target("id=third").unwrap();
    let small_layout = small
        .layout(Viewport {
            width: 24,
            height: 12,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(small_layout.box_for(small_row).unwrap().height, 12);
    assert_eq!(small_layout.box_for(small_third).unwrap().y, 14);
    assert_eq!(
        small_layout.max_scroll_offset(),
        NativePoint { x: 0, y: 16 }
    );
}

#[test]
fn native_flex_row_justifies_fixed_items_and_distributes_space_deterministically() {
    let positions = |justify: &str| {
        let source = format!(
            "<div id='row' style='display:flex;width:40px;gap:2px;justify-content:{justify}'><div id='first' style='width:6px;height:8px'>A</div><div id='second' style='width:8px;height:8px'>B</div></div>"
        );
        let document = NativeDocument::parse(&source, &NativeEngineLimits::default()).unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let layout = document
            .layout(Viewport {
                width: 40,
                height: 32,
                device_scale_factor_milli: 1000,
            })
            .unwrap();
        (
            layout.box_for(first).unwrap().x,
            layout.box_for(second).unwrap().x,
        )
    };

    assert_eq!(positions("flex-start"), (0, 8));
    assert_eq!(positions("center"), (12, 20));
    assert_eq!(positions("flex-end"), (24, 32));
    assert_eq!(positions("space-between"), (0, 32));
    assert_eq!(positions("space-around"), (6, 26));
    assert_eq!(positions("space-evenly"), (8, 24));
    assert_eq!(positions("normal"), (0, 8));
    assert_eq!(positions("stretch"), (0, 8));

    let remainder = NativeDocument::parse(
        "<div id='row' style='display:flex;width:41px;gap:1px;justify-content:space-between'><div id='first' style='width:6px;height:8px'>A</div><div id='second' style='width:6px;height:8px'>B</div><div id='third' style='width:6px;height:8px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = remainder.resolve_target("id=first").unwrap();
    let second = remainder.resolve_target("id=second").unwrap();
    let third = remainder.resolve_target("id=third").unwrap();
    let layout = remainder
        .layout(Viewport {
            width: 41,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(layout.box_for(first).unwrap().x, 0);
    assert_eq!(layout.box_for(second).unwrap().x, 18);
    assert_eq!(layout.box_for(third).unwrap().x, 35);
}

#[test]
fn native_flex_row_justify_content_aliases_reuse_flex_start_geometry_and_artifacts() {
    let source = |direction: &str, justify: &str| {
        format!(
            "<div id='row' style='display:flex;width:40px;height:20px;gap:2px;flex-direction:{direction};justify-content:{justify}'><button id='low' style='width:6px;height:8px;background-color:red'><span id='nested' style='display:block;height:4px'>L</span></button><button id='middle' style='width:8px;height:8px;background-color:green'>M</button><button id='high' style='width:4px;height:8px;background-color:blue'>H</button></div>"
        )
    };
    let viewport = Viewport {
        width: 48,
        height: 32,
        device_scale_factor_milli: 1000,
    };

    for justify in ["normal", "stretch"] {
        for (direction, expected) in [
            ("row", [(0, 6), (8, 8), (18, 4)]),
            ("row-reverse", [(34, 6), (24, 8), (18, 4)]),
        ] {
            let document =
                NativeDocument::parse(&source(direction, justify), &NativeEngineLimits::default())
                    .unwrap();
            let row = document.resolve_target("id=row").unwrap();
            let low = document.resolve_target("id=low").unwrap();
            let nested = document.resolve_target("id=nested").unwrap();
            let middle = document.resolve_target("id=middle").unwrap();
            let high = document.resolve_target("id=high").unwrap();
            let layout = document.layout(viewport).unwrap();

            assert_eq!(layout.box_for(row).unwrap().width, 40);
            for (node_id, (x, width)) in [
                (low, expected[0]),
                (middle, expected[1]),
                (high, expected[2]),
            ] {
                assert_eq!(
                    layout.box_for(node_id).map(|rect| (rect.x, rect.width)),
                    Some((x, width))
                );
            }
            assert_eq!(layout.box_for(nested).unwrap().x, expected[0].0);
            assert_eq!(
                layout.hit_test((expected[0].0 + 1).into(), 1).unwrap(),
                Some(nested)
            );

            let list = document.display_list(viewport).unwrap();
            assert!(list.commands.iter().any(|command| {
                matches!(
                    command,
                    NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                        if *node_id == low && rect.x == expected[0].0 && *color == NativeColor::RED
                )
            }));
        }
    }
}

#[test]
fn native_flex_column_maps_vertical_justification_and_cross_axis_artifacts() {
    let source = |direction: &str, justify: &str| {
        format!(
            "<div id='column' style='display:flex;width:30px;height:40px;row-gap:2px;flex-direction:{direction};justify-content:{justify};align-items:center'><button id='first' style='width:6px;height:6px;background-color:red'><span id='nested' style='display:block;height:4px'>F</span></button><button id='second' style='width:8px;height:8px;background-color:blue'>S</button></div>"
        )
    };
    let viewport = Viewport {
        width: 40,
        height: 48,
        device_scale_factor_milli: 1000,
    };

    for justify in [
        "flex-start",
        "center",
        "flex-end",
        "space-between",
        "space-around",
        "space-evenly",
        "normal",
        "stretch",
    ] {
        let expected = match justify {
            "center" => [(12, 6), (20, 8)],
            "flex-end" => [(24, 6), (32, 8)],
            "space-between" => [(0, 6), (32, 8)],
            "space-around" => [(6, 6), (26, 8)],
            "space-evenly" => [(8, 6), (24, 8)],
            _ => [(0, 6), (8, 8)],
        };
        for (direction, expected) in [
            ("column", expected),
            (
                "column-reverse",
                [
                    (40 - expected[0].0 - expected[0].1, expected[0].1),
                    (40 - expected[1].0 - expected[1].1, expected[1].1),
                ],
            ),
        ] {
            let document =
                NativeDocument::parse(&source(direction, justify), &NativeEngineLimits::default())
                    .unwrap();
            let column = document.resolve_target("id=column").unwrap();
            let first = document.resolve_target("id=first").unwrap();
            let nested = document.resolve_target("id=nested").unwrap();
            let second = document.resolve_target("id=second").unwrap();
            let layout = document.layout(viewport).unwrap();

            assert_eq!(
                layout.box_for(column).map(|rect| (rect.width, rect.height)),
                Some((30, 40))
            );
            assert_eq!(
                layout
                    .box_for(first)
                    .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
                Some((12, expected[0].0, expected[0].1, expected[0].1))
            );
            assert_eq!(
                layout
                    .box_for(second)
                    .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
                Some((11, expected[1].0, expected[1].1, expected[1].1))
            );
            assert_eq!(
                layout.box_for(nested).map(|rect| (rect.x, rect.y)),
                Some((12, expected[0].0))
            );
            assert_eq!(
                layout
                    .hit_test((12 + 1).into(), (expected[0].0 + 1).into())
                    .unwrap(),
                Some(nested)
            );

            let list = document.display_list(viewport).unwrap();
            assert!(list.commands.iter().any(|command| {
                matches!(
                    command,
                    NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                        if *node_id == first
                            && rect.x == 12
                            && rect.y == expected[0].0
                            && *color == NativeColor::RED
                )
            }));
        }
    }
}

#[test]
fn native_flex_column_applies_vertical_flex_sizing_before_justification() {
    let grown = NativeDocument::parse(
        "<div id='column' style='display:flex;width:20px;height:30px;row-gap:2px;flex-direction:column'><div id='first' style='width:6px;height:6px;flex-grow:1'>A</div><div id='second' style='width:6px;height:6px;flex-grow:2'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = grown.resolve_target("id=first").unwrap();
    let second = grown.resolve_target("id=second").unwrap();
    let layout = grown
        .layout(Viewport {
            width: 24,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(
        layout.box_for(first).map(|rect| (rect.y, rect.height)),
        Some((0, 11))
    );
    assert_eq!(
        layout.box_for(second).map(|rect| (rect.y, rect.height)),
        Some((13, 17))
    );

    let shrunk = NativeDocument::parse(
        "<div id='column' style='display:flex;width:20px;height:10px;row-gap:2px;flex-direction:column'><div id='first' style='width:6px;height:8px'>A</div><div id='second' style='width:6px;height:8px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = shrunk.resolve_target("id=first").unwrap();
    let second = shrunk.resolve_target("id=second").unwrap();
    let layout = shrunk
        .layout(Viewport {
            width: 24,
            height: 20,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(
        layout.box_for(first).map(|rect| (rect.y, rect.height)),
        Some((0, 4))
    );
    assert_eq!(
        layout.box_for(second).map(|rect| (rect.y, rect.height)),
        Some((6, 4))
    );
}

#[test]
fn native_flex_column_wrap_forms_vertical_lines_and_maps_cross_axis_spacing() {
    let source = |direction: &str| {
        format!(
            "<div id='column' style='display:flex;width:26px;height:20px;flex-direction:{direction};flex-wrap:wrap;row-gap:2px;column-gap:3px;align-items:center;align-content:space-between'><div id='first' style='width:4px;height:8px;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></div><div id='second' style='width:8px;height:7px;background-color:green'>S</div><div id='third' style='width:6px;height:6px;background-color:blue'>T</div></div>"
        )
    };
    let viewport = Viewport {
        width: 32,
        height: 24,
        device_scale_factor_milli: 1000,
    };

    for (direction, expected) in [
        ("column", [(2, 0, 4, 8), (0, 10, 8, 7), (20, 0, 6, 6)]),
        (
            "column-reverse",
            [(2, 12, 4, 8), (0, 3, 8, 7), (20, 14, 6, 6)],
        ),
    ] {
        let document =
            NativeDocument::parse(&source(direction), &NativeEngineLimits::default()).unwrap();
        let column = document.resolve_target("id=column").unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(
            layout
                .box_for(column)
                .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
            Some((0, 0, 26, 20))
        );
        for (node_id, expected_rect) in [
            (first, expected[0]),
            (second, expected[1]),
            (third, expected[2]),
        ] {
            assert_eq!(
                layout
                    .box_for(node_id)
                    .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
                Some(expected_rect)
            );
        }
        assert_eq!(
            layout.box_for(nested).map(|rect| (rect.x, rect.y)),
            Some((expected[0].0, expected[0].1))
        );
        assert_eq!(
            layout.hit_test(3, i64::from(expected[0].1 + 1)).unwrap(),
            Some(nested)
        );

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == first
                        && rect.x == expected[0].0
                        && rect.y == expected[0].1
                        && *color == NativeColor::RED
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == third
                        && rect.x == expected[2].0
                        && rect.y == expected[2].1
                        && *color
                            == (NativeColor {
                                red: 0,
                                green: 0,
                                blue: u8::MAX,
                                alpha: u8::MAX,
                            })
            )
        }));
    }
}

#[test]
fn native_flex_column_wrap_sizes_each_line_and_supports_wrap_reverse() {
    let wrapped = NativeDocument::parse(
        "<div id='column' style='display:flex;width:20px;height:20px;flex-direction:column;flex-wrap:wrap;row-gap:2px;column-gap:1px;align-content:flex-start'><div id='first' style='width:5px;height:6px;flex-grow:1'>A</div><div id='second' style='width:5px;height:6px;flex-grow:1'>B</div><div id='third' style='width:5px;height:6px;flex-grow:1'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = wrapped.resolve_target("id=first").unwrap();
    let second = wrapped.resolve_target("id=second").unwrap();
    let third = wrapped.resolve_target("id=third").unwrap();
    let layout = wrapped
        .layout(Viewport {
            width: 24,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(
        layout
            .box_for(first)
            .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((0, 0, 5, 9))
    );
    assert_eq!(
        layout
            .box_for(second)
            .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((0, 11, 5, 9))
    );
    assert_eq!(
        layout
            .box_for(third)
            .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((6, 0, 5, 20))
    );

    let wrap_reverse = NativeDocument::parse(
        "<div id='column' style='display:flex;width:20px;height:20px;flex-direction:column;flex-wrap:wrap-reverse;row-gap:2px;column-gap:1px;align-items:flex-start;align-content:flex-start'><div id='first' style='display:block;width:5px;height:6px'>A</div><div id='second' style='display:block;width:5px;height:6px'>B</div><div id='third' style='display:block;width:5px;height:6px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let wrap_reverse_first = wrap_reverse.resolve_target("id=first").unwrap();
    let wrap_reverse_second = wrap_reverse.resolve_target("id=second").unwrap();
    let wrap_reverse_third = wrap_reverse.resolve_target("id=third").unwrap();
    let wrap_reverse_layout = wrap_reverse
        .layout(Viewport {
            width: 24,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(
        wrap_reverse_layout.box_for(wrap_reverse_first).map(|rect| (
            rect.x,
            rect.y,
            rect.width,
            rect.height
        )),
        Some((15, 0, 5, 6))
    );
    assert_eq!(
        wrap_reverse_layout
            .box_for(wrap_reverse_second)
            .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((15, 8, 5, 6))
    );
    assert_eq!(
        wrap_reverse_layout.box_for(wrap_reverse_third).map(|rect| (
            rect.x,
            rect.y,
            rect.width,
            rect.height
        )),
        Some((9, 0, 5, 6))
    );
}

#[test]
fn native_flex_column_wrap_reverse_reflects_cross_axis_alignment_and_artifacts() {
    let source = |direction: &str| {
        format!(
            "<div id='column' style='display:flex;width:26px;height:20px;flex-direction:{direction};flex-wrap:wrap-reverse;row-gap:2px;column-gap:3px;align-items:flex-start;align-content:space-between'><button id='first' style='width:4px;height:8px;align-self:flex-end;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></button><button id='second' style='width:8px;height:7px;background-color:green'>S</button><button id='third' style='width:6px;height:6px;background-color:blue'>T</button></div>"
        )
    };
    let viewport = Viewport {
        width: 32,
        height: 28,
        device_scale_factor_milli: 1000,
    };

    for (direction, expected) in [
        ("column", [(18, 0, 4, 8), (18, 10, 8, 7), (0, 0, 6, 6)]),
        (
            "column-reverse",
            [(18, 12, 4, 8), (18, 3, 8, 7), (0, 14, 6, 6)],
        ),
    ] {
        let document =
            NativeDocument::parse(&source(direction), &NativeEngineLimits::default()).unwrap();
        let column = document.resolve_target("id=column").unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(
            layout.box_for(column).map(|rect| (rect.width, rect.height)),
            Some((26, 20))
        );
        for (node_id, (x, y, width, height)) in [
            (first, expected[0]),
            (second, expected[1]),
            (third, expected[2]),
        ] {
            assert_eq!(
                layout
                    .box_for(node_id)
                    .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
                Some((x, y, width, height))
            );
        }
        assert_eq!(
            layout.box_for(nested).map(|rect| (rect.x, rect.y)),
            Some((18, expected[0].1))
        );
        assert_eq!(
            layout.hit_test(19.into(), (expected[0].1 + 1).into()),
            Ok(Some(nested))
        );

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, .. } if *node_id == first
            )
        }));
        let surface = document.rasterize(viewport).unwrap();
        assert_eq!(surface.width(), 32);
        assert_eq!(surface.height(), 28);
        assert_eq!(surface.pixel(18, expected[0].1 + 7), Some([255, 0, 0, 255]));
    }
}

#[test]
fn native_flex_column_wrap_reverse_ineligible_content_keeps_fallback() {
    let wrapped = NativeDocument::parse(
        "<div id='column' style='display:flex;width:20px;height:20px;flex-direction:column;flex-wrap:wrap-reverse'><div id='first' style='display:block;width:5px'>Alpha</div><div id='second' style='display:block;width:5px'>Beta</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let block = NativeDocument::parse(
        "<div id='column' style='display:block;width:20px;height:20px'><div id='first' style='display:block;width:5px'>Alpha</div><div id='second' style='display:block;width:5px'>Beta</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    let viewport = Viewport {
        width: 24,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let wrapped_layout = wrapped.layout(viewport).unwrap();
    let block_layout = block.layout(viewport).unwrap();
    assert_eq!(wrapped_layout.boxes, block_layout.boxes);
    assert_eq!(wrapped_layout.text_runs, block_layout.text_runs);
}

#[test]
fn native_flex_row_space_around_rounds_and_mirrors_complete_item_geometry() {
    let source = |direction: &str| {
        format!(
            "<div id='row' style='display:flex;width:40px;height:20px;gap:2px;flex-direction:{direction};justify-content:space-around'><button id='low' style='width:6px;height:8px;background-color:red'><span id='nested' style='display:block;height:4px'>L</span></button><button id='middle' style='width:8px;height:8px;background-color:green'>M</button><button id='high' style='width:4px;height:8px;background-color:blue'>H</button></div>"
        )
    };
    let viewport = Viewport {
        width: 48,
        height: 32,
        device_scale_factor_milli: 1000,
    };

    for (direction, expected) in [
        ("row", [(3, 6), (17, 8), (33, 4)]),
        ("row-reverse", [(31, 6), (15, 8), (3, 4)]),
    ] {
        let document =
            NativeDocument::parse(&source(direction), &NativeEngineLimits::default()).unwrap();
        let row = document.resolve_target("id=row").unwrap();
        let low = document.resolve_target("id=low").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let middle = document.resolve_target("id=middle").unwrap();
        let high = document.resolve_target("id=high").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(layout.box_for(row).unwrap().width, 40);
        for (node_id, (x, width)) in [
            (low, expected[0]),
            (middle, expected[1]),
            (high, expected[2]),
        ] {
            assert_eq!(
                layout.box_for(node_id).map(|rect| (rect.x, rect.width)),
                Some((x, width))
            );
        }
        assert_eq!(layout.box_for(nested).unwrap().x, expected[0].0);
        assert_eq!(
            layout.hit_test((expected[0].0 + 1).into(), 1).unwrap(),
            Some(nested)
        );

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == low && rect.x == expected[0].0 && *color == NativeColor::RED
            )
        }));
    }

    let wrapped = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px;flex-wrap:wrap;justify-content:space-around'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:6px'>B</div><div id='third' style='width:8px;height:6px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = wrapped.resolve_target("id=first").unwrap();
    let second = wrapped.resolve_target("id=second").unwrap();
    let third = wrapped.resolve_target("id=third").unwrap();
    let wrapped_layout = wrapped.layout(viewport).unwrap();
    assert_eq!(wrapped_layout.box_for(first).unwrap().x, 0);
    assert_eq!(wrapped_layout.box_for(second).unwrap().x, 11);
    assert_eq!(wrapped_layout.box_for(third).unwrap().x, 6);

    let one = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;justify-content:space-around'><div id='item' style='width:8px;height:6px'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let one_item = one.resolve_target("id=item").unwrap();
    assert_eq!(
        one.layout(viewport).unwrap().box_for(one_item).unwrap().x,
        6
    );

    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:8px;justify-content:space-around'><div id='item' style='width:12px;height:6px;flex-shrink:0'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overflow_item = overflow.resolve_target("id=item").unwrap();
    assert_eq!(
        overflow
            .layout(Viewport {
                width: 16,
                height: 32,
                device_scale_factor_milli: 1000,
            })
            .unwrap()
            .box_for(overflow_item)
            .unwrap()
            .x,
        0
    );
}

#[test]
fn native_flex_row_space_evenly_rounds_and_mirrors_complete_item_geometry() {
    let source = |direction: &str| {
        format!(
            "<div id='row' style='display:flex;width:40px;height:20px;gap:2px;flex-direction:{direction};justify-content:space-evenly'><button id='low' style='width:6px;height:8px;background-color:red'><span id='nested' style='display:block;height:4px'>L</span></button><button id='middle' style='width:8px;height:8px;background-color:green'>M</button><button id='high' style='width:4px;height:8px;background-color:blue'>H</button></div>"
        )
    };
    let viewport = Viewport {
        width: 48,
        height: 32,
        device_scale_factor_milli: 1000,
    };

    for (direction, expected) in [
        ("row", [(4, 6), (17, 8), (31, 4)]),
        ("row-reverse", [(30, 6), (15, 8), (5, 4)]),
    ] {
        let document =
            NativeDocument::parse(&source(direction), &NativeEngineLimits::default()).unwrap();
        let row = document.resolve_target("id=row").unwrap();
        let low = document.resolve_target("id=low").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let middle = document.resolve_target("id=middle").unwrap();
        let high = document.resolve_target("id=high").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(layout.box_for(row).unwrap().width, 40);
        for (node_id, (x, width)) in [
            (low, expected[0]),
            (middle, expected[1]),
            (high, expected[2]),
        ] {
            assert_eq!(
                layout.box_for(node_id).map(|rect| (rect.x, rect.width)),
                Some((x, width))
            );
        }
        assert_eq!(layout.box_for(nested).unwrap().x, expected[0].0);
        assert_eq!(
            layout.hit_test((expected[0].0 + 1).into(), 1).unwrap(),
            Some(nested)
        );

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == low && rect.x == expected[0].0 && *color == NativeColor::RED
            )
        }));
    }

    let wrapped = NativeDocument::parse(
        "<div id='row' style='display:flex;width:21px;gap:2px;flex-wrap:wrap;justify-content:space-evenly'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:6px'>B</div><div id='third' style='width:8px;height:6px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = wrapped.resolve_target("id=first").unwrap();
    let second = wrapped.resolve_target("id=second").unwrap();
    let third = wrapped.resolve_target("id=third").unwrap();
    let wrapped_layout = wrapped.layout(viewport).unwrap();
    assert_eq!(wrapped_layout.box_for(first).unwrap().x, 1);
    assert_eq!(wrapped_layout.box_for(second).unwrap().x, 12);
    assert_eq!(wrapped_layout.box_for(third).unwrap().x, 6);

    let one = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;justify-content:space-evenly'><div id='item' style='width:8px;height:6px'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let one_item = one.resolve_target("id=item").unwrap();
    assert_eq!(
        one.layout(viewport).unwrap().box_for(one_item).unwrap().x,
        6
    );

    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:8px;justify-content:space-evenly'><div id='item' style='width:12px;height:6px;flex-shrink:0'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overflow_item = overflow.resolve_target("id=item").unwrap();
    assert_eq!(
        overflow
            .layout(Viewport {
                width: 16,
                height: 32,
                device_scale_factor_milli: 1000,
            })
            .unwrap()
            .box_for(overflow_item)
            .unwrap()
            .x,
        0
    );
}

#[test]
fn native_flex_row_justification_preserves_overflow_and_ignores_fallback_rows() {
    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;gap:2px;justify-content:center'><div id='first' style='width:10px;height:8px;flex-shrink:0'>A</div><div id='second' style='width:10px;height:8px;flex-shrink:0'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = overflow.resolve_target("id=first").unwrap();
    let second = overflow.resolve_target("id=second").unwrap();
    let layout = overflow
        .layout(Viewport {
            width: 12,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(layout.box_for(first).unwrap().x, 0);
    assert_eq!(layout.box_for(second).unwrap().x, 12);
    assert_eq!(layout.content_width, 22);
    assert_eq!(layout.max_scroll_offset().x, 10);

    let with_justify = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px;justify-content:flex-end'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let without_justify = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px;justify-content:flex-start'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let with_first = with_justify.resolve_target("id=first").unwrap();
    let with_second = with_justify.resolve_target("id=second").unwrap();
    let without_first = without_justify.resolve_target("id=first").unwrap();
    let without_second = without_justify.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 48,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let with_layout = with_justify.layout(viewport).unwrap();
    let without_layout = without_justify.layout(viewport).unwrap();
    assert_eq!(
        with_layout.box_for(with_first),
        without_layout.box_for(without_first)
    );
    assert_eq!(
        with_layout.box_for(with_second),
        without_layout.box_for(without_second)
    );
    assert!(
        with_layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("mean"))
    );
}

#[test]
fn native_flex_item_order_reorders_visual_items_and_preserves_source_ties() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:40px;gap:2px'><button id='source-first' style='order:2;width:6px;height:8px;background-color:red'>A</button><button id='tied-first' style='order:-1;width:6px;height:8px;background-color:green'>B</button><button id='tied-second' style='order:-1;width:6px;height:8px;background-color:blue'>C</button><button id='default' style='width:6px;height:8px;background-color:black'>D</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let source_first = document.resolve_target("id=source-first").unwrap();
    let tied_first = document.resolve_target("id=tied-first").unwrap();
    let tied_second = document.resolve_target("id=tied-second").unwrap();
    let default_item = document.resolve_target("id=default").unwrap();
    let layout = document
        .layout(Viewport {
            width: 40,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(layout.box_for(tied_first).unwrap().x, 0);
    assert_eq!(layout.box_for(tied_second).unwrap().x, 8);
    assert_eq!(layout.box_for(default_item).unwrap().x, 16);
    assert_eq!(layout.box_for(source_first).unwrap().x, 24);

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    assert_eq!(
        semantic_ids,
        vec![source_first, tied_first, tied_second, default_item]
    );

    let painted_ids = document
        .display_list(Viewport {
            width: 40,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap()
        .commands
        .into_iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::FillRect { node_id, .. } => Some(node_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        painted_ids,
        vec![tied_first, tied_second, default_item, source_first]
    );
}

#[test]
fn native_flex_item_order_filters_hidden_items_and_ignores_fallback_rows() {
    let ordered = NativeDocument::parse(
        "<div id='row' style='display:flex;width:24px;gap:2px'><div id='first' style='order:2;width:8px;height:8px'>A</div><div id='hidden' style='display:none;order:-1024;width:8px;height:8px'>Hidden</div><div id='second' style='order:-1;width:8px;height:8px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = ordered.resolve_target("id=first").unwrap();
    let hidden = ordered.resolve_target("id=hidden").unwrap();
    let second = ordered.resolve_target("id=second").unwrap();
    let layout = ordered
        .layout(Viewport {
            width: 24,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(layout.box_for(second).unwrap().x, 0);
    assert_eq!(layout.box_for(first).unwrap().x, 10);
    assert_eq!(layout.box_for(hidden), None);

    let with_order = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px'><span id='first' style='display:block;order:2;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;order:-2;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let without_order = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let with_first = with_order.resolve_target("id=first").unwrap();
    let with_second = with_order.resolve_target("id=second").unwrap();
    let without_first = without_order.resolve_target("id=first").unwrap();
    let without_second = without_order.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 48,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let with_layout = with_order.layout(viewport).unwrap();
    let without_layout = without_order.layout(viewport).unwrap();
    assert_eq!(
        with_layout.box_for(with_first),
        without_layout.box_for(without_first)
    );
    assert_eq!(
        with_layout.box_for(with_second),
        without_layout.box_for(without_second)
    );
    assert!(
        with_layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("mean"))
    );
}

#[test]
fn native_flex_align_items_moves_complete_subtrees_and_paint_artifacts() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:40px;height:31px;gap:2px;align-items:center'><button id='short' style='width:6px;height:8px;background-color:red'><span id='nested' style='display:block;height:4px'>A</span></button><button id='tall' style='width:6px;height:20px;background-color:blue'>B</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let short = document.resolve_target("id=short").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let tall = document.resolve_target("id=tall").unwrap();
    let viewport = Viewport {
        width: 40,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let row_rect = layout.box_for(row).unwrap();
    let short_rect = layout.box_for(short).unwrap();
    let nested_rect = layout.box_for(nested).unwrap();
    let tall_rect = layout.box_for(tall).unwrap();

    assert_eq!(row_rect.height, 31);
    assert_eq!(short_rect.x, 0);
    assert_eq!(short_rect.y, 11);
    assert_eq!(tall_rect.x, 8);
    assert_eq!(tall_rect.y, 5);
    assert_eq!(nested_rect.y, short_rect.y);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .find(|run| run.text == "A")
            .map(|run| run.origin.y),
        Some(short_rect.y)
    );
    assert_eq!(
        layout.hit_test(1, short_rect.y as i64).unwrap(),
        Some(nested)
    );
    assert_eq!(layout.hit_test(9, tall_rect.y as i64).unwrap(), Some(tall));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == short && rect.y == short_rect.y
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == tall && rect.y == tall_rect.y
        )
    }));
}

#[test]
fn native_flex_align_self_overrides_parent_and_moves_complete_subtrees() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:40px;height:31px;gap:2px;align-items:center'><button id='auto' style='width:6px;height:8px;background-color:red'><span id='nested' style='display:block;height:4px'>A</span></button><button id='start' style='width:6px;height:8px;align-self:flex-start;background-color:green'>S</button><button id='end' style='width:6px;height:8px;align-self:flex-end;background-color:blue'>E</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let start = document.resolve_target("id=start").unwrap();
    let end = document.resolve_target("id=end").unwrap();
    let viewport = Viewport {
        width: 40,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().height, 31);
    assert_eq!(layout.box_for(auto).unwrap().y, 11);
    assert_eq!(layout.box_for(nested).unwrap().y, 11);
    assert_eq!(layout.box_for(start).unwrap().y, 0);
    assert_eq!(layout.box_for(end).unwrap().y, 23);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == auto && rect.y == 11 && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == start
                    && rect.y == 0
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: u8::MAX,
                        })
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == end
                    && rect.y == 23
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 0,
                            blue: u8::MAX,
                            alpha: u8::MAX,
                        })
        )
    }));
    assert_eq!(layout.hit_test(1, 12).unwrap(), Some(nested));
    assert_eq!(layout.hit_test(9, 1).unwrap(), Some(start));
    assert_eq!(layout.hit_test(17, 24).unwrap(), Some(end));
}

#[test]
fn native_flex_place_content_reuses_axis_distribution_and_artifacts() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:24px;height:60px;gap:2px;flex-wrap:wrap;align-items:center;place-content:center space-between'><div id='first' style='width:8px;height:6px;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></div><div id='second' style='width:8px;height:10px;background-color:green'>S</div><div id='third' style='width:8px;height:14px;background-color:blue'>T</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().height, 60);
    assert_eq!(layout.box_for(first).unwrap().x, 0);
    assert_eq!(layout.box_for(second).unwrap().x, 16);
    assert_eq!(layout.box_for(third).unwrap().x, 0);
    assert_eq!(layout.box_for(first).unwrap().y, 19);
    assert_eq!(layout.box_for(second).unwrap().y, 17);
    assert_eq!(layout.box_for(third).unwrap().y, 29);
    assert_eq!(layout.box_for(nested).unwrap().y, 19);
    assert_eq!(layout.content_height, 64);
    assert_eq!(layout.max_scroll_offset(), NativePoint { x: 0, y: 0 });

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first && rect.x == 0 && rect.y == 19 && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == second
                    && rect.x == 16
                    && rect.y == 17
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: u8::MAX,
                        })
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == third
                    && rect.x == 0
                    && rect.y == 29
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 0,
                            blue: u8::MAX,
                            alpha: u8::MAX,
                        })
        )
    }));
    assert_eq!(layout.hit_test(1, 20).unwrap(), Some(nested));
    assert_eq!(layout.hit_test(17, 18).unwrap(), Some(second));
    assert_eq!(layout.hit_test(1, 30).unwrap(), Some(third));
}

#[test]
fn native_flex_align_self_stretch_fills_auto_height_and_preserves_explicit_size() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:30px;height:32px;gap:2px;align-items:center'><div id='stretched' style='width:8px;align-self:stretch;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></div><div id='explicit' style='width:8px;height:8px;align-self:stretch;background-color:green'>E</div><div id='inset' style='width:8px;box-sizing:border-box;padding:1px;border:1px solid blue;max-height:24px;align-self:stretch;background-color:blue'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let stretched = document.resolve_target("id=stretched").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let inset = document.resolve_target("id=inset").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().height, 32);
    assert_eq!(layout.box_for(stretched).unwrap().x, 0);
    assert_eq!(layout.box_for(stretched).unwrap().y, 0);
    assert_eq!(layout.box_for(stretched).unwrap().height, 32);
    assert_eq!(layout.box_for(nested).unwrap().y, 0);
    assert_eq!(layout.box_for(explicit).unwrap().x, 10);
    assert_eq!(layout.box_for(explicit).unwrap().y, 0);
    assert_eq!(layout.box_for(explicit).unwrap().height, 8);
    assert_eq!(layout.box_for(inset).unwrap().x, 20);
    assert_eq!(layout.box_for(inset).unwrap().y, 0);
    assert_eq!(layout.box_for(inset).unwrap().height, 24);
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == inset)
            .unwrap()
            .content_rect
            .height,
        20
    );
    assert_eq!(layout.content_height, 40);
    assert_eq!(layout.max_scroll_offset(), NativePoint { x: 0, y: 0 });

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == stretched && rect.x == 0 && rect.y == 0 && rect.height == 32 && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == explicit && rect.x == 10 && rect.y == 0 && rect.height == 8
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: u8::MAX,
                        })
        )
    }));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(nested));
    assert_eq!(layout.hit_test(1, 31).unwrap(), Some(stretched));
    assert_eq!(layout.hit_test(11, 1).unwrap(), Some(explicit));
    assert_eq!(layout.hit_test(21, 23).unwrap(), Some(inset));
}

#[test]
fn native_flex_align_items_stretch_reuses_item_path_and_respects_overrides() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:48px;height:32px;gap:2px;align-items:stretch'><div id='stretched' style='width:8px;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></div><div id='explicit' style='width:8px;height:8px;background-color:green'>E</div><div id='center' style='width:8px;height:8px;align-self:center;background-color:blue'>C</div><div id='inset' style='width:8px;box-sizing:border-box;padding:1px;border:1px solid blue;max-height:24px;background-color:blue'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let stretched = document.resolve_target("id=stretched").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let center = document.resolve_target("id=center").unwrap();
    let inset = document.resolve_target("id=inset").unwrap();
    let viewport = Viewport {
        width: 56,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().height, 32);
    assert_eq!(layout.box_for(stretched).unwrap().x, 0);
    assert_eq!(layout.box_for(stretched).unwrap().y, 0);
    assert_eq!(layout.box_for(stretched).unwrap().height, 32);
    assert_eq!(layout.box_for(nested).unwrap().y, 0);
    assert_eq!(layout.box_for(explicit).unwrap().x, 10);
    assert_eq!(layout.box_for(explicit).unwrap().y, 0);
    assert_eq!(layout.box_for(explicit).unwrap().height, 8);
    assert_eq!(layout.box_for(center).unwrap().x, 20);
    assert_eq!(layout.box_for(center).unwrap().y, 12);
    assert_eq!(layout.box_for(center).unwrap().height, 8);
    assert_eq!(layout.box_for(inset).unwrap().x, 30);
    assert_eq!(layout.box_for(inset).unwrap().y, 0);
    assert_eq!(layout.box_for(inset).unwrap().height, 24);
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == inset)
            .unwrap()
            .content_rect
            .height,
        20
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == stretched
                    && rect.x == 0
                    && rect.y == 0
                    && rect.height == 32
                    && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == center
                    && rect.x == 20
                    && rect.y == 12
                    && rect.height == 8
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 0,
                            blue: u8::MAX,
                            alpha: u8::MAX,
                        })
        )
    }));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(nested));
    assert_eq!(layout.hit_test(1, 31).unwrap(), Some(stretched));
    assert_eq!(layout.hit_test(21, 13).unwrap(), Some(center));
    assert_eq!(layout.hit_test(31, 23).unwrap(), Some(inset));
}

#[test]
fn native_flex_align_items_stretch_uses_the_formed_wrapped_line_size() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;height:60px;gap:2px;align-items:stretch;align-content:stretch;flex-wrap:wrap'><div id='first' style='width:8px;background-color:red'><span id='first-nested' style='display:block;height:2px'>F</span></div><div id='explicit' style='width:8px;height:12px;background-color:green'>E</div><div id='second' style='width:8px;background-color:blue'><span id='second-nested' style='display:block;height:4px'>S</span></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let first_nested = document.resolve_target("id=first-nested").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let second_nested = document.resolve_target("id=second-nested").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 68,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().height, 60);
    assert_eq!(layout.box_for(first).unwrap().x, 0);
    assert_eq!(layout.box_for(first).unwrap().y, 0);
    assert_eq!(layout.box_for(first).unwrap().height, 29);
    assert_eq!(layout.box_for(first_nested).unwrap().y, 0);
    assert_eq!(layout.box_for(explicit).unwrap().x, 10);
    assert_eq!(layout.box_for(explicit).unwrap().y, 0);
    assert_eq!(layout.box_for(explicit).unwrap().height, 12);
    assert_eq!(layout.box_for(second).unwrap().x, 0);
    assert_eq!(layout.box_for(second).unwrap().y, 31);
    assert_eq!(layout.box_for(second).unwrap().height, 29);
    assert_eq!(layout.box_for(second_nested).unwrap().y, 31);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first
                    && rect.x == 0
                    && rect.y == 0
                    && rect.height == 29
                    && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == second
                    && rect.x == 0
                    && rect.y == 31
                    && rect.height == 29
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 0,
                            blue: u8::MAX,
                            alpha: u8::MAX,
                        })
        )
    }));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(first_nested));
    assert_eq!(layout.hit_test(1, 28).unwrap(), Some(first));
    assert_eq!(layout.hit_test(1, 32).unwrap(), Some(second_nested));
    assert_eq!(layout.hit_test(1, 59).unwrap(), Some(second));
}

#[test]
fn native_flex_align_items_normal_reuses_stretch_and_preserves_overrides() {
    let normal = NativeDocument::parse(
        "<div id='row' style='display:flex;width:48px;height:32px;gap:2px;align-items:normal'><div id='auto' style='width:8px;background-color:red'><span id='nested' style='display:block;height:2px'>N</span></div><div id='explicit' style='width:8px;height:8px;background-color:green'>E</div><div id='center' style='width:8px;height:8px;align-self:center;background-color:blue'>C</div><div id='inset' style='width:8px;box-sizing:border-box;padding:1px;border:1px solid blue;max-height:24px;background-color:blue'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let stretch = NativeDocument::parse(
        "<div id='row' style='display:flex;width:48px;height:32px;gap:2px;align-items:stretch'><div id='auto' style='width:8px;background-color:red'><span id='nested' style='display:block;height:2px'>N</span></div><div id='explicit' style='width:8px;height:8px;background-color:green'>E</div><div id='center' style='width:8px;height:8px;align-self:center;background-color:blue'>C</div><div id='inset' style='width:8px;box-sizing:border-box;padding:1px;border:1px solid blue;max-height:24px;background-color:blue'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let normal_auto = normal.resolve_target("id=auto").unwrap();
    let normal_nested = normal.resolve_target("id=nested").unwrap();
    let normal_center = normal.resolve_target("id=center").unwrap();
    let viewport = Viewport {
        width: 56,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let normal_layout = normal.layout(viewport).unwrap();
    let stretch_layout = stretch.layout(viewport).unwrap();

    assert_eq!(normal_layout.boxes, stretch_layout.boxes);
    assert_eq!(normal_layout.text_runs, stretch_layout.text_runs);
    assert_eq!(normal_layout.box_for(normal_auto).unwrap().height, 32);
    assert_eq!(normal_layout.box_for(normal_nested).unwrap().y, 0);
    assert_eq!(normal_layout.box_for(normal_center).unwrap().y, 12);

    let list = normal.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == normal_auto
                    && rect.y == 0
                    && rect.height == 32
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(normal_layout.hit_test(1, 1).unwrap(), Some(normal_nested));
    assert_eq!(normal_layout.hit_test(21, 13).unwrap(), Some(normal_center));
}

#[test]
fn native_flex_align_self_normal_reuses_stretch_and_overrides_parent() {
    let normal = NativeDocument::parse(
        "<div id='row' style='display:flex;width:48px;height:32px;gap:2px;align-items:center'><div id='normal' style='width:8px;align-self:normal;background-color:red'><span id='nested' style='display:block;height:2px'>N</span></div><div id='auto' style='width:8px;background-color:green'>A</div><div id='explicit' style='width:8px;height:8px;align-self:normal;background-color:blue'>E</div><div id='center' style='width:8px;height:8px;align-self:center;background-color:blue'>C</div><div id='inset' style='width:8px;box-sizing:border-box;padding:1px;border:1px solid blue;max-height:24px;align-self:normal;background-color:blue'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let stretch = NativeDocument::parse(
        "<div id='row' style='display:flex;width:48px;height:32px;gap:2px;align-items:center'><div id='normal' style='width:8px;align-self:stretch;background-color:red'><span id='nested' style='display:block;height:2px'>N</span></div><div id='auto' style='width:8px;background-color:green'>A</div><div id='explicit' style='width:8px;height:8px;align-self:stretch;background-color:blue'>E</div><div id='center' style='width:8px;height:8px;align-self:center;background-color:blue'>C</div><div id='inset' style='width:8px;box-sizing:border-box;padding:1px;border:1px solid blue;max-height:24px;align-self:stretch;background-color:blue'>I</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let normal_item = normal.resolve_target("id=normal").unwrap();
    let normal_nested = normal.resolve_target("id=nested").unwrap();
    let normal_auto = normal.resolve_target("id=auto").unwrap();
    let normal_explicit = normal.resolve_target("id=explicit").unwrap();
    let normal_center = normal.resolve_target("id=center").unwrap();
    let normal_inset = normal.resolve_target("id=inset").unwrap();
    let viewport = Viewport {
        width: 56,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let normal_layout = normal.layout(viewport).unwrap();
    let stretch_layout = stretch.layout(viewport).unwrap();

    assert_eq!(normal_layout.boxes, stretch_layout.boxes);
    assert_eq!(normal_layout.text_runs, stretch_layout.text_runs);
    assert_eq!(normal_layout.box_for(normal_item).unwrap().height, 32);
    assert_eq!(normal_layout.box_for(normal_item).unwrap().y, 0);
    assert_eq!(normal_layout.box_for(normal_nested).unwrap().y, 0);
    assert_eq!(normal_layout.box_for(normal_auto).unwrap().y, 6);
    assert_eq!(normal_layout.box_for(normal_explicit).unwrap().y, 0);
    assert_eq!(normal_layout.box_for(normal_explicit).unwrap().height, 8);
    assert_eq!(normal_layout.box_for(normal_center).unwrap().y, 12);
    assert_eq!(normal_layout.box_for(normal_inset).unwrap().height, 24);
    assert_eq!(normal_layout.box_for(normal_inset).unwrap().y, 0);

    let list = normal.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == normal_item
                    && rect.y == 0
                    && rect.height == 32
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(normal_layout.hit_test(1, 1).unwrap(), Some(normal_nested));
    assert_eq!(normal_layout.hit_test(11, 16).unwrap(), Some(normal_auto));
    assert_eq!(normal_layout.hit_test(31, 13).unwrap(), Some(normal_center));
}

#[test]
fn native_flex_align_items_handles_auto_lines_box_sizing_overflow_and_fallback() {
    let auto = NativeDocument::parse(
        "<div id='row' style='display:flex;width:32px;align-items:center'><div id='short' style='width:8px;height:8px'>A</div><div id='tall' style='width:8px;height:24px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let auto_row = auto.resolve_target("id=row").unwrap();
    let auto_short = auto.resolve_target("id=short").unwrap();
    let auto_tall = auto.resolve_target("id=tall").unwrap();
    let viewport = Viewport {
        width: 48,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let auto_layout = auto.layout(viewport).unwrap();
    assert_eq!(auto_layout.box_for(auto_row).unwrap().height, 24);
    assert_eq!(auto_layout.box_for(auto_short).unwrap().y, 8);
    assert_eq!(auto_layout.box_for(auto_tall).unwrap().y, 0);

    let bordered = NativeDocument::parse(
        "<div id='row' style='display:flex;width:32px;height:30px;box-sizing:border-box;padding:2px;border:1px solid black;align-items:flex-end'><div id='short' style='width:8px;height:8px'>A</div><div id='tall' style='width:8px;height:20px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let bordered_row = bordered.resolve_target("id=row").unwrap();
    let bordered_short = bordered.resolve_target("id=short").unwrap();
    let bordered_tall = bordered.resolve_target("id=tall").unwrap();
    let bordered_layout = bordered.layout(viewport).unwrap();
    assert_eq!(bordered_layout.box_for(bordered_row).unwrap().height, 30);
    assert_eq!(bordered_layout.box_for(bordered_short).unwrap().y, 19);
    assert_eq!(bordered_layout.box_for(bordered_tall).unwrap().y, 7);

    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;height:8px;align-items:flex-end'><div id='item' style='width:10px;height:20px'>A</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overflow_item = overflow.resolve_target("id=item").unwrap();
    let overflow_layout = overflow
        .layout(Viewport {
            width: 12,
            height: 8,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(overflow_layout.box_for(overflow_item).unwrap().y, 0);
    assert_eq!(overflow_layout.max_scroll_offset().y, 12);

    let with_alignment = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px;align-items:flex-end'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let without_alignment = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let with_first = with_alignment.resolve_target("id=first").unwrap();
    let with_second = with_alignment.resolve_target("id=second").unwrap();
    let without_first = without_alignment.resolve_target("id=first").unwrap();
    let without_second = without_alignment.resolve_target("id=second").unwrap();
    let with_layout = with_alignment.layout(viewport).unwrap();
    let without_layout = without_alignment.layout(viewport).unwrap();
    assert_eq!(
        with_layout.box_for(with_first),
        without_layout.box_for(without_first)
    );
    assert_eq!(
        with_layout.box_for(with_second),
        without_layout.box_for(without_second)
    );
    assert_eq!(with_layout.text_runs, without_layout.text_runs);
}

#[test]
fn native_flex_direction_row_is_equivalent_and_row_reverse_maps_justification() {
    let source = |direction: &str, justify: &str| {
        format!(
            "<div id='row' style='display:flex;width:40px;height:20px;gap:2px;flex-direction:{direction};justify-content:{justify};align-items:center'><button id='low' style='order:-1;width:6px;height:8px;background-color:red'><span id='nested' style='display:block;height:4px'>L</span></button><button id='middle' style='order:0;width:8px;height:12px;background-color:green'>M</button><button id='high' style='order:2;width:4px;height:4px;background-color:blue'>H</button></div>"
        )
    };
    let viewport = Viewport {
        width: 48,
        height: 32,
        device_scale_factor_milli: 1000,
    };

    let default_source = source("row", "flex-start").replace("flex-direction:row;", "");
    let default_document =
        NativeDocument::parse(&default_source, &NativeEngineLimits::default()).unwrap();
    let explicit_row =
        NativeDocument::parse(&source("row", "flex-start"), &NativeEngineLimits::default())
            .unwrap();
    let default_layout = default_document.layout(viewport).unwrap();
    let explicit_row_layout = explicit_row.layout(viewport).unwrap();
    assert_eq!(default_layout.boxes, explicit_row_layout.boxes);
    assert_eq!(default_layout.text_runs, explicit_row_layout.text_runs);

    for (justify, expected) in [
        ("flex-start", (34, 24, 18)),
        ("center", (25, 15, 9)),
        ("flex-end", (16, 6, 0)),
        ("space-between", (34, 15, 0)),
    ] {
        let document = NativeDocument::parse(
            &source("row-reverse", justify),
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let low = document.resolve_target("id=low").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let middle = document.resolve_target("id=middle").unwrap();
        let high = document.resolve_target("id=high").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(layout.box_for(low).unwrap().x, expected.0);
        assert_eq!(layout.box_for(middle).unwrap().x, expected.1);
        assert_eq!(layout.box_for(high).unwrap().x, expected.2);
        assert_eq!(layout.box_for(nested).unwrap().x, expected.0);
        assert_eq!(
            layout.box_for(nested).unwrap().y,
            layout.box_for(low).unwrap().y
        );
        assert_eq!(
            layout.hit_test((expected.0 + 1).into(), 6).unwrap(),
            Some(nested)
        );
        assert_eq!(
            layout.hit_test((expected.2 + 1).into(), 8).unwrap(),
            Some(high)
        );

        let painted_ids = document
            .display_list(viewport)
            .unwrap()
            .commands
            .into_iter()
            .filter_map(|command| match command {
                NativeDisplayCommand::FillRect { node_id, .. } => Some(node_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(painted_ids, vec![low, middle, high]);
    }
}

#[test]
fn native_flex_direction_row_reverse_keeps_margins_reachable_and_fallback_content() {
    let html = "<div id='row' style='display:flex;width:16px;height:20px;gap:2px;flex-direction:row-reverse;justify-content:flex-start;align-items:center'><div id='low' style='order:-1;width:10px;height:8px;margin:1px;flex-shrink:0;background-color:red'><span id='nested' style='display:block;height:4px'>L</span></div><div id='high' style='order:1;width:10px;height:12px;margin:2px;flex-shrink:0;background-color:blue'>H</div><div id='hidden' style='display:none;order:-1024;width:10px;height:8px'>Hidden</div></div>";
    let document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let low = document.resolve_target("id=low").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let high = document.resolve_target("id=high").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();
    let viewport = Viewport {
        width: 16,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(high).unwrap().x, 2);
    assert_eq!(layout.box_for(low).unwrap().x, 17);
    assert_eq!(layout.box_for(nested).unwrap().x, 17);
    assert_eq!(layout.box_for(high).unwrap().y, 4);
    assert_eq!(layout.box_for(low).unwrap().y, 6);
    assert_eq!(layout.box_for(hidden), None);
    assert_eq!(layout.max_scroll_offset().x, 11);
    assert_eq!(layout.hit_test(3, 5).unwrap(), Some(high));

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://row-reverse-overflow", html)
        .unwrap()
        .with_initial_url("fixture://row-reverse-overflow");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let scroll = engine
        .action(NativeAction::Scroll {
            delta_x: 11,
            delta_y: 0,
        })
        .unwrap();
    assert!(scroll.accepted);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 11, y: 0 });
    assert_eq!(engine.hit_test(7, 6).unwrap(), Some(nested));

    let with_direction = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px;flex-direction:row-reverse'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let without_direction = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let with_first = with_direction.resolve_target("id=first").unwrap();
    let with_second = with_direction.resolve_target("id=second").unwrap();
    let without_first = without_direction.resolve_target("id=first").unwrap();
    let without_second = without_direction.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 48,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let with_layout = with_direction.layout(viewport).unwrap();
    let without_layout = without_direction.layout(viewport).unwrap();
    assert_eq!(
        with_layout.box_for(with_first),
        without_layout.box_for(without_first)
    );
    assert_eq!(
        with_layout.box_for(with_second),
        without_layout.box_for(without_second)
    );
    assert_eq!(with_layout.text_runs, without_layout.text_runs);
}

#[test]
fn native_flex_wrap_forms_lines_and_reuses_alignment_scroll_and_artifacts() {
    let html = "<div id='row' style='display:flex;width:20px;gap:2px;flex-wrap:wrap;justify-content:space-between;align-items:center'><button id='first' style='order:2;width:8px;height:12px;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></button><button id='second' style='order:-1;width:6px;height:14px;background-color:green'>S</button><button id='third' style='order:0;width:8px;height:8px;background-color:blue'>T</button></div>";
    let document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let viewport = Viewport {
        width: 20,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().height, 28);
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 6,
            height: 14,
        })
    );
    assert_eq!(
        layout.box_for(third),
        Some(NativeRect {
            x: 12,
            y: 3,
            width: 8,
            height: 8,
        })
    );
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 0,
            y: 16,
            width: 8,
            height: 12,
        })
    );
    assert_eq!(layout.box_for(nested).unwrap().y, 16);
    assert_eq!(
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == nested)
            .map(|run| (run.origin, run.text.as_str())),
        Some((NativePoint { x: 0, y: 16 }, "F"))
    );
    assert_eq!(layout.content_height, 28);
    assert_eq!(layout.max_scroll_offset(), NativePoint { x: 0, y: 12 });
    assert_eq!(layout.hit_test(13, 4).unwrap(), Some(third));

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    let source_positions = [first, second, third]
        .into_iter()
        .map(|node_id| semantic_ids.iter().position(|id| *id == node_id).unwrap())
        .collect::<Vec<_>>();
    assert!(source_positions[0] < source_positions[1]);
    assert!(source_positions[1] < source_positions[2]);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == first && rect.y == 16 && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == third
                    && rect.y == 3
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 0,
                            blue: u8::MAX,
                            alpha: u8::MAX,
                        })
        )
    }));

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://flex-wrap", html)
        .unwrap()
        .with_initial_url("fixture://flex-wrap");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let scroll = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(scroll.accepted);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 12 });
    assert_eq!(
        engine.layout().unwrap().viewport_rect_for(first),
        Some(NativeRect {
            x: 0,
            y: 4,
            width: 8,
            height: 12,
        })
    );
    assert_eq!(engine.hit_test(1, 6).unwrap(), Some(first));
}

#[test]
fn native_flex_gap_shorthand_resolves_row_and_column_axes() {
    let html = "<div id='row' style='display:flex;width:24px;gap:3px 4px;flex-wrap:wrap'><div id='first' style='width:8px;height:6px;background-color:red'>A</div><div id='second' style='width:8px;height:10px;background-color:green'>B</div><div id='third' style='width:8px;height:14px;background-color:blue'>C</div></div>";
    let document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(row).unwrap().height, 27);
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 12,
            y: 0,
            width: 8,
            height: 10,
        })
    );
    assert_eq!(
        layout.box_for(third),
        Some(NativeRect {
            x: 0,
            y: 13,
            width: 8,
            height: 14,
        })
    );
    assert_eq!(layout.content_height, 32);
    assert_eq!(layout.hit_test(1, 14).unwrap(), Some(third));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == third
                    && rect.y == 13
                    && *color
                        == (NativeColor {
                            red: 0,
                            green: 0,
                            blue: u8::MAX,
                            alpha: u8::MAX,
                        })
        )
    }));
}

#[test]
fn native_flex_wrap_applies_row_reverse_per_line_and_keeps_wide_items_non_negative() {
    let html = "<div id='row' style='display:flex;width:12px;gap:2px;flex-wrap:wrap;flex-direction:row-reverse;justify-content:flex-start'><div id='wide' style='width:20px;height:6px;margin:1px;flex-shrink:0;background-color:red'>W</div><div id='next' style='width:4px;height:6px;margin:1px;flex-shrink:0;background-color:blue'>N</div></div>";
    let document = NativeDocument::parse(html, &NativeEngineLimits::default()).unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let wide = document.resolve_target("id=wide").unwrap();
    let next = document.resolve_target("id=next").unwrap();
    let viewport = Viewport {
        width: 12,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(wide),
        Some(NativeRect {
            x: 1,
            y: 1,
            width: 20,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(next),
        Some(NativeRect {
            x: 7,
            y: 11,
            width: 4,
            height: 6,
        })
    );
    assert_eq!(layout.content_width, 21);
    assert_eq!(layout.max_scroll_offset(), NativePoint { x: 9, y: 0 });
    assert_eq!(layout.hit_test(8, 2).unwrap(), Some(wide));

    assert_eq!(document.node(row).unwrap().children(), &[wide, next]);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == wide && rect.x == 1 && *color == NativeColor::RED
        )
    }));

    let config = NativeEngineConfig::default()
        .with_viewport(viewport)
        .with_fixture("fixture://flex-wrap-reverse", html)
        .unwrap()
        .with_initial_url("fixture://flex-wrap-reverse");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let scroll = engine
        .action(NativeAction::Scroll {
            delta_x: i32::MAX,
            delta_y: 0,
        })
        .unwrap();
    assert!(scroll.accepted);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 9, y: 0 });
    assert_eq!(engine.hit_test(1, 2).unwrap(), Some(wide));
}

#[test]
fn native_flex_wrap_no_wrap_and_ineligible_content_keep_existing_fallback() {
    let nowrap = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px;flex-wrap:nowrap'><div id='first' style='width:8px;height:8px'>A</div><div id='second' style='width:8px;height:8px'>B</div><div id='third' style='width:8px;height:8px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let default = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px'><div id='first' style='width:8px;height:8px'>A</div><div id='second' style='width:8px;height:8px'>B</div><div id='third' style='width:8px;height:8px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let nowrap_layout = nowrap.layout(viewport).unwrap();
    let default_layout = default.layout(viewport).unwrap();
    assert_eq!(nowrap_layout.boxes, default_layout.boxes);
    assert_eq!(nowrap_layout.text_runs, default_layout.text_runs);

    let wrapped_fallback = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px;flex-wrap:wrap'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let default_fallback = NativeDocument::parse(
        "<div id='fallback' style='display:flex;width:32px'><span id='first' style='display:block;width:8px;height:8px'>A</span> meaningful text <span id='second' style='display:block;width:8px;height:8px'>B</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let wrapped_first = wrapped_fallback.resolve_target("id=first").unwrap();
    let wrapped_second = wrapped_fallback.resolve_target("id=second").unwrap();
    let default_first = default_fallback.resolve_target("id=first").unwrap();
    let default_second = default_fallback.resolve_target("id=second").unwrap();
    let wrapped_layout = wrapped_fallback.layout(viewport).unwrap();
    let default_layout = default_fallback.layout(viewport).unwrap();
    assert_eq!(
        wrapped_layout.box_for(wrapped_first),
        default_layout.box_for(default_first)
    );
    assert_eq!(
        wrapped_layout.box_for(wrapped_second),
        default_layout.box_for(default_second)
    );
    assert_eq!(wrapped_layout.text_runs, default_layout.text_runs);
}

#[test]
fn native_flex_align_content_distributes_explicit_cross_axis_space() {
    let source = |align_content: &str| {
        format!(
            "<div id='row' style='display:flex;width:20px;height:60px;gap:2px;flex-wrap:wrap;align-items:center;align-content:{align_content}'><div id='first' style='width:8px;height:6px;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></div><div id='second' style='width:8px;height:10px;background-color:green'>S</div><div id='third' style='width:8px;height:14px;background-color:blue'>T</div></div>"
        )
    };
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };

    for (align_content, expected) in [
        ("flex-start", (2, 0, 12)),
        ("center", (19, 17, 29)),
        ("flex-end", (36, 34, 46)),
        ("space-between", (2, 0, 46)),
        ("space-around", (10, 8, 37)),
        ("space-evenly", (13, 11, 34)),
        ("stretch", (10, 8, 37)),
        ("normal", (10, 8, 37)),
    ] {
        let document =
            NativeDocument::parse(&source(align_content), &NativeEngineLimits::default()).unwrap();
        let row = document.resolve_target("id=row").unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(layout.box_for(row).unwrap().height, 60);
        assert_eq!(layout.box_for(first).unwrap().y, expected.0);
        assert_eq!(layout.box_for(second).unwrap().y, expected.1);
        assert_eq!(layout.box_for(third).unwrap().y, expected.2);
        assert_eq!(layout.box_for(nested).unwrap().y, expected.0);
        assert_eq!(layout.content_height, 64);
        assert_eq!(layout.max_scroll_offset(), NativePoint { x: 0, y: 0 });

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == first && rect.y == expected.0 && *color == NativeColor::RED
            )
        }));
        assert_eq!(
            layout.hit_test(1, (expected.0 + 1).into()).unwrap(),
            Some(nested)
        );

        let one_line = NativeDocument::parse(
            &format!(
                "<div id='row' style='display:flex;width:20px;height:60px;flex-wrap:wrap;align-content:{align_content}'><div id='item' style='width:8px;height:6px'>I</div></div>"
            ),
            &NativeEngineLimits::default(),
        )
        .unwrap();
        let one_line_item = one_line.resolve_target("id=item").unwrap();
        let one_line_layout = one_line
            .layout(Viewport {
                width: 24,
                height: 64,
                device_scale_factor_milli: 1000,
            })
            .unwrap();
        let one_line_y = match align_content {
            "center" | "space-around" | "space-evenly" => 27,
            "flex-end" => 54,
            "flex-start" | "space-between" | "stretch" | "normal" => 0,
            _ => unreachable!(),
        };
        assert_eq!(
            one_line_layout.box_for(one_line_item).unwrap().y,
            one_line_y
        );
    }
}

#[test]
fn native_flex_align_content_stretch_assigns_remainder_to_first_formed_line() {
    let source = |wrap: &str| {
        format!(
            "<div id='row' style='display:flex;width:20px;height:61px;gap:2px;flex-wrap:{wrap};align-items:center;align-content:stretch'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>"
        )
    };

    for (wrap, expected) in [("wrap", (11, 9, 38)), ("wrap-reverse", (44, 42, 8))] {
        let document =
            NativeDocument::parse(&source(wrap), &NativeEngineLimits::default()).unwrap();
        let row = document.resolve_target("id=row").unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let layout = document
            .layout(Viewport {
                width: 24,
                height: 64,
                device_scale_factor_milli: 1000,
            })
            .unwrap();

        assert_eq!(layout.box_for(row).unwrap().height, 61);
        assert_eq!(layout.box_for(first).unwrap().y, expected.0);
        assert_eq!(layout.box_for(second).unwrap().y, expected.1);
        assert_eq!(layout.box_for(third).unwrap().y, expected.2);
    }
}

#[test]
fn native_flex_align_content_normal_reuses_stretch_and_keeps_omitted_fallback() {
    let source = |align_content: &str, wrap: &str| {
        format!(
            "<div id='row' style='display:flex;width:20px;height:61px;gap:2px;flex-wrap:{wrap};align-items:center;align-content:{align_content}'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>"
        )
    };
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };

    for wrap in ["wrap", "wrap-reverse"] {
        let normal =
            NativeDocument::parse(&source("normal", wrap), &NativeEngineLimits::default()).unwrap();
        let stretch =
            NativeDocument::parse(&source("stretch", wrap), &NativeEngineLimits::default())
                .unwrap();
        assert_eq!(
            normal.layout(viewport).unwrap(),
            stretch.layout(viewport).unwrap()
        );
        assert_eq!(
            normal.display_list(viewport).unwrap().commands,
            stretch.display_list(viewport).unwrap().commands
        );
    }

    let omitted_html = source("normal", "wrap").replace(";align-content:normal", "");
    let omitted = NativeDocument::parse(&omitted_html, &NativeEngineLimits::default()).unwrap();
    let omitted_first = omitted.resolve_target("id=first").unwrap();
    let omitted_second = omitted.resolve_target("id=second").unwrap();
    let omitted_third = omitted.resolve_target("id=third").unwrap();
    let omitted_layout = omitted.layout(viewport).unwrap();
    assert_eq!(omitted_layout.box_for(omitted_first).unwrap().y, 2);
    assert_eq!(omitted_layout.box_for(omitted_second).unwrap().y, 0);
    assert_eq!(omitted_layout.box_for(omitted_third).unwrap().y, 12);
}

#[test]
fn native_flex_align_content_preserves_auto_small_and_nowrap_geometry() {
    let auto = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px;flex-wrap:wrap;align-content:space-evenly'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let auto_row = auto.resolve_target("id=row").unwrap();
    let auto_third = auto.resolve_target("id=third").unwrap();
    let auto_layout = auto
        .layout(Viewport {
            width: 24,
            height: 64,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(auto_layout.box_for(auto_third).unwrap().y, 12);
    assert_eq!(auto_layout.box_for(auto_row).unwrap().height, 26);

    let small = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;height:12px;gap:2px;flex-wrap:wrap;align-content:space-evenly'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let small_row = small.resolve_target("id=row").unwrap();
    let small_third = small.resolve_target("id=third").unwrap();
    let small_layout = small
        .layout(Viewport {
            width: 24,
            height: 12,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(small_layout.box_for(small_row).unwrap().height, 12);
    assert_eq!(small_layout.box_for(small_third).unwrap().y, 12);
    assert_eq!(
        small_layout.max_scroll_offset(),
        NativePoint { x: 0, y: 14 }
    );

    let nowrap = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;height:60px;gap:2px;flex-wrap:nowrap;align-content:space-evenly'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let default = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;height:60px;gap:2px;flex-wrap:nowrap'><div id='first' style='width:8px;height:6px'>A</div><div id='second' style='width:8px;height:10px'>B</div><div id='third' style='width:8px;height:14px'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let nowrap_layout = nowrap.layout(viewport).unwrap();
    let default_layout = default.layout(viewport).unwrap();
    assert_eq!(nowrap_layout.boxes, default_layout.boxes);
    assert_eq!(nowrap_layout.text_runs, default_layout.text_runs);
}

#[test]
fn native_flex_wrap_reverse_reflects_lines_and_shared_artifacts() {
    let source = |align_content: &str| {
        format!(
            "<div id='row' style='display:flex;width:20px;height:60px;gap:2px;flex-wrap:wrap-reverse;align-items:center;align-content:{align_content}'><div id='first' style='width:8px;height:6px;background-color:red'><span id='nested' style='display:block;height:2px'>F</span></div><div id='second' style='width:8px;height:10px;background-color:green'>S</div><div id='third' style='width:8px;height:14px;background-color:blue'>T</div></div>"
        )
    };
    let viewport = Viewport {
        width: 24,
        height: 64,
        device_scale_factor_milli: 1000,
    };

    for (align_content, expected) in [
        ("flex-start", (52, 50, 34)),
        ("center", (35, 33, 17)),
        ("flex-end", (18, 16, 0)),
        ("space-between", (52, 50, 0)),
        ("space-around", (44, 42, 9)),
        ("space-evenly", (41, 39, 12)),
        ("normal", (43, 41, 8)),
    ] {
        let html = source(align_content);
        let document = NativeDocument::parse(&html, &NativeEngineLimits::default()).unwrap();
        let row = document.resolve_target("id=row").unwrap();
        let first = document.resolve_target("id=first").unwrap();
        let nested = document.resolve_target("id=nested").unwrap();
        let second = document.resolve_target("id=second").unwrap();
        let third = document.resolve_target("id=third").unwrap();
        let layout = document.layout(viewport).unwrap();

        assert_eq!(layout.box_for(row).unwrap().height, 60);
        assert_eq!(layout.box_for(first).unwrap().y, expected.0);
        assert_eq!(layout.box_for(second).unwrap().y, expected.1);
        assert_eq!(layout.box_for(third).unwrap().y, expected.2);
        assert_eq!(layout.box_for(nested).unwrap().y, expected.0);
        assert_eq!(layout.content_height, 64);
        assert_eq!(layout.max_scroll_offset(), NativePoint { x: 0, y: 0 });

        assert_eq!(
            document.node(row).unwrap().children(),
            &[first, second, third]
        );

        let list = document.display_list(viewport).unwrap();
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                    if *node_id == first && rect.y == expected.0 && *color == NativeColor::RED
            )
        }));
        assert_eq!(
            layout.hit_test(1, (expected.0 + 1).into()).unwrap(),
            Some(nested)
        );
    }

    let auto = NativeDocument::parse(
        &source("space-evenly").replace("height:60px;", ""),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let auto_row = auto.resolve_target("id=row").unwrap();
    let auto_third = auto.resolve_target("id=third").unwrap();
    let auto_layout = auto.layout(viewport).unwrap();
    assert_eq!(auto_layout.box_for(auto_row).unwrap().height, 26);
    assert_eq!(auto_layout.box_for(auto_third).unwrap().y, 0);

    let small = NativeDocument::parse(
        &source("space-evenly").replace("height:60px;", "height:12px;"),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let small_row = small.resolve_target("id=row").unwrap();
    let small_third = small.resolve_target("id=third").unwrap();
    let small_layout = small
        .layout(Viewport {
            width: 24,
            height: 12,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(small_layout.box_for(small_row).unwrap().height, 12);
    assert_eq!(small_layout.box_for(small_third).unwrap().y, 0);
    assert_eq!(small_layout.max_scroll_offset(), NativePoint { x: 0, y: 2 });

    let stretch_auto = NativeDocument::parse(
        &source("stretch").replace("height:60px;", ""),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let stretch_auto_row = stretch_auto.resolve_target("id=row").unwrap();
    let stretch_auto_third = stretch_auto.resolve_target("id=third").unwrap();
    let stretch_auto_layout = stretch_auto.layout(viewport).unwrap();
    assert_eq!(
        stretch_auto_layout
            .box_for(stretch_auto_row)
            .unwrap()
            .height,
        26
    );
    assert_eq!(
        stretch_auto_layout.box_for(stretch_auto_third).unwrap().y,
        0
    );

    let stretch_small = NativeDocument::parse(
        &source("stretch").replace("height:60px;", "height:12px;"),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let stretch_small_row = stretch_small.resolve_target("id=row").unwrap();
    let stretch_small_third = stretch_small.resolve_target("id=third").unwrap();
    let stretch_small_layout = stretch_small
        .layout(Viewport {
            width: 24,
            height: 12,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(
        stretch_small_layout
            .box_for(stretch_small_row)
            .unwrap()
            .height,
        12
    );
    assert_eq!(
        stretch_small_layout.box_for(stretch_small_third).unwrap().y,
        0
    );
    assert_eq!(
        stretch_small_layout.max_scroll_offset(),
        NativePoint { x: 0, y: 2 }
    );

    let html = source("flex-start");
    let third = NativeDocument::parse(&html, &NativeEngineLimits::default())
        .unwrap()
        .resolve_target("id=third")
        .unwrap();
    let engine_viewport = Viewport {
        width: 24,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let config = NativeEngineConfig::default()
        .with_viewport(engine_viewport)
        .with_fixture("fixture://wrap-reverse-scroll", &html)
        .unwrap()
        .with_initial_url("fixture://wrap-reverse-scroll");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let scroll = engine
        .action(NativeAction::Scroll {
            delta_x: 0,
            delta_y: i32::MAX,
        })
        .unwrap();
    assert!(scroll.accepted);
    assert_eq!(engine.scroll_offset(), NativePoint { x: 0, y: 36 });
    assert_eq!(
        engine.layout().unwrap().viewport_rect_for(third),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 12,
        })
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

#[test]
fn native_flex_grow_allocates_weighted_space_before_justification() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:40px;gap:2px;justify-content:space-between'><div id='first' style='width:6px;height:8px;flex-grow:1;background-color:red'><span id='nested' style='display:block;height:4px'>A</span></div><div id='second' style='width:8px;height:8px;flex-grow:2;background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 48,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().width, 40);
    assert_eq!(layout.box_for(first).unwrap().x, 0);
    assert_eq!(layout.box_for(first).unwrap().width, 14);
    assert_eq!(layout.box_for(nested).unwrap().x, 0);
    assert_eq!(layout.box_for(nested).unwrap().width, 14);
    assert_eq!(layout.box_for(second).unwrap().x, 16);
    assert_eq!(layout.box_for(second).unwrap().width, 24);
    assert_eq!(layout.content_width, 48);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first && rect.width == 14 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(13, 6).unwrap(), Some(first));
    assert_eq!(layout.hit_test(16, 1).unwrap(), Some(second));
}

#[test]
fn native_flex_grow_handles_wrap_reverse_overflow_and_max_width_reallocation() {
    let wrapped = NativeDocument::parse(
        "<div id='row' style='display:flex;width:30px;gap:2px;flex-wrap:wrap-reverse'><div id='first' style='width:14px;height:6px;flex-grow:1'>A</div><div id='second' style='width:14px;height:8px;flex-grow:1'>B</div><div id='third' style='width:14px;height:10px;flex-grow:2'>C</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = wrapped.resolve_target("id=first").unwrap();
    let second = wrapped.resolve_target("id=second").unwrap();
    let third = wrapped.resolve_target("id=third").unwrap();
    let wrapped_layout = wrapped
        .layout(Viewport {
            width: 32,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(wrapped_layout.box_for(first).unwrap().width, 14);
    assert_eq!(wrapped_layout.box_for(second).unwrap().width, 14);
    assert_eq!(wrapped_layout.box_for(third).unwrap().width, 30);
    assert_eq!(wrapped_layout.box_for(first).unwrap().y, 12);
    assert_eq!(wrapped_layout.box_for(second).unwrap().y, 12);
    assert_eq!(wrapped_layout.box_for(third).unwrap().y, 0);
    assert_eq!(wrapped_layout.content_width, 32);

    let maxed = NativeDocument::parse(
        "<div id='row' style='display:flex;width:40px;gap:2px'><div id='capped' style='width:8px;height:8px;max-width:10px;flex-grow:1'>A</div><div id='receiver' style='width:8px;height:8px;flex-grow:1'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let capped = maxed.resolve_target("id=capped").unwrap();
    let receiver = maxed.resolve_target("id=receiver").unwrap();
    let maxed_layout = maxed
        .layout(Viewport {
            width: 40,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(maxed_layout.box_for(capped).unwrap().width, 10);
    assert_eq!(maxed_layout.box_for(receiver).unwrap().x, 12);
    assert_eq!(maxed_layout.box_for(receiver).unwrap().width, 28);

    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;gap:2px;flex-direction:row-reverse'><div id='first' style='width:8px;height:8px;flex-grow:1;flex-shrink:0'>A</div><div id='second' style='width:8px;height:8px;flex-grow:2;flex-shrink:0'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overflow_first = overflow.resolve_target("id=first").unwrap();
    let overflow_second = overflow.resolve_target("id=second").unwrap();
    let overflow_layout = overflow
        .layout(Viewport {
            width: 12,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(overflow_layout.box_for(overflow_first).unwrap().width, 8);
    assert_eq!(overflow_layout.box_for(overflow_second).unwrap().width, 8);
    assert_eq!(overflow_layout.box_for(overflow_first).unwrap().x, 10);
    assert_eq!(overflow_layout.box_for(overflow_second).unwrap().x, 0);
}

#[test]
fn native_flex_shrink_allocates_base_weighted_deficit_before_justification() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px;justify-content:space-between'><div id='first' style='width:12px;height:8px;flex-shrink:1;background-color:red'><span id='nested' style='display:block;height:4px'>A</span></div><div id='second' style='width:12px;height:8px;flex-shrink:2;background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().width, 20);
    assert_eq!(layout.box_for(first).unwrap().width, 10);
    assert_eq!(layout.box_for(nested).unwrap().width, 10);
    assert_eq!(layout.box_for(second).unwrap().x, 12);
    assert_eq!(layout.box_for(second).unwrap().width, 8);
    assert_eq!(layout.content_width, 24);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first && rect.width == 10 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(9, 6).unwrap(), Some(first));
    assert_eq!(layout.hit_test(12, 1).unwrap(), Some(second));
}

#[test]
fn native_flex_shrink_freezes_minimums_and_preserves_wrap_and_zero_overflow() {
    let minimum = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px'><div id='capped' style='width:12px;height:8px;min-width:10px;flex-shrink:1'>A</div><div id='receiver' style='width:12px;height:8px;flex-shrink:1'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let capped = minimum.resolve_target("id=capped").unwrap();
    let receiver = minimum.resolve_target("id=receiver").unwrap();
    let minimum_layout = minimum
        .layout(Viewport {
            width: 20,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(minimum_layout.box_for(capped).unwrap().width, 10);
    assert_eq!(minimum_layout.box_for(receiver).unwrap().x, 12);
    assert_eq!(minimum_layout.box_for(receiver).unwrap().width, 8);

    let wrapped = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;gap:2px;flex-wrap:wrap'><div id='first' style='width:8px;height:8px'>A</div><div id='second' style='width:8px;height:8px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let wrapped_first = wrapped.resolve_target("id=first").unwrap();
    let wrapped_second = wrapped.resolve_target("id=second").unwrap();
    let wrapped_layout = wrapped
        .layout(Viewport {
            width: 16,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(wrapped_layout.box_for(wrapped_first).unwrap().width, 8);
    assert_eq!(wrapped_layout.box_for(wrapped_second).unwrap().width, 8);
    assert_eq!(wrapped_layout.box_for(wrapped_first).unwrap().y, 0);
    assert_eq!(wrapped_layout.box_for(wrapped_second).unwrap().y, 10);

    let zero = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;gap:2px;flex-direction:row-reverse'><div id='first' style='width:8px;height:8px;flex-shrink:0'>A</div><div id='second' style='width:8px;height:8px;flex-shrink:0'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let zero_first = zero.resolve_target("id=first").unwrap();
    let zero_second = zero.resolve_target("id=second").unwrap();
    let zero_layout = zero
        .layout(Viewport {
            width: 12,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(zero_layout.box_for(zero_first).unwrap().width, 8);
    assert_eq!(zero_layout.box_for(zero_second).unwrap().width, 8);
    assert_eq!(zero_layout.box_for(zero_first).unwrap().x, 10);
    assert_eq!(zero_layout.box_for(zero_second).unwrap().x, 0);
}

#[test]
fn native_flex_basis_overrides_width_and_feeds_growth_and_descendants() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:30px;gap:2px;justify-content:space-between'><div id='first' style='width:4px;flex-basis:10px;flex-grow:1;height:8px;background-color:red'><span id='nested' style='display:block;height:4px'>A</span></div><div id='second' style='width:20px;flex-basis:6px;flex-grow:2;height:8px;background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().width, 30);
    assert_eq!(layout.box_for(first).unwrap().width, 14);
    assert_eq!(layout.box_for(nested).unwrap().width, 14);
    assert_eq!(layout.box_for(second).unwrap().x, 16);
    assert_eq!(layout.box_for(second).unwrap().width, 14);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first && rect.width == 14 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(13, 6).unwrap(), Some(first));
    assert_eq!(layout.hit_test(16, 1).unwrap(), Some(second));
}

#[test]
fn native_flex_basis_controls_wrap_constraints_and_explicit_overflow() {
    let wrapped = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;gap:2px;flex-wrap:wrap'><div id='first' style='width:8px;flex-basis:4px;height:8px'>A</div><div id='second' style='width:8px;flex-basis:4px;height:8px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let wrapped_first = wrapped.resolve_target("id=first").unwrap();
    let wrapped_second = wrapped.resolve_target("id=second").unwrap();
    let wrapped_layout = wrapped
        .layout(Viewport {
            width: 16,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(wrapped_layout.box_for(wrapped_first).unwrap().width, 4);
    assert_eq!(wrapped_layout.box_for(wrapped_first).unwrap().x, 0);
    assert_eq!(wrapped_layout.box_for(wrapped_second).unwrap().width, 4);
    assert_eq!(wrapped_layout.box_for(wrapped_second).unwrap().x, 6);
    assert_eq!(wrapped_layout.box_for(wrapped_second).unwrap().y, 0);

    let constrained = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px'><div id='bordered' style='width:2px;flex-basis:4px;min-width:10px;padding:1px;border:1px solid red;flex-shrink:0;height:8px'>A</div><div id='receiver' style='width:40px;flex-basis:4px;flex-shrink:0;height:8px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let bordered = constrained.resolve_target("id=bordered").unwrap();
    let receiver = constrained.resolve_target("id=receiver").unwrap();
    let constrained_layout = constrained
        .layout(Viewport {
            width: 20,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(constrained_layout.box_for(bordered).unwrap().width, 14);
    assert_eq!(constrained_layout.box_for(receiver).unwrap().x, 16);
    assert_eq!(constrained_layout.box_for(receiver).unwrap().width, 4);
    assert_eq!(
        constrained_layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == bordered)
            .map(|layout_box| layout_box.content_rect.width),
        Some(10)
    );

    let overflow = NativeDocument::parse(
        "<div id='row' style='display:flex;width:16px;gap:2px'><div id='first' style='width:4px;flex-basis:12px;flex-shrink:0;height:8px'>A</div><div id='second' style='width:4px;flex-basis:12px;flex-shrink:0;height:8px'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overflow_first = overflow.resolve_target("id=first").unwrap();
    let overflow_second = overflow.resolve_target("id=second").unwrap();
    let overflow_layout = overflow
        .layout(Viewport {
            width: 16,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(overflow_layout.box_for(overflow_first).unwrap().width, 12);
    assert_eq!(overflow_layout.box_for(overflow_second).unwrap().x, 14);
    assert_eq!(overflow_layout.box_for(overflow_second).unwrap().width, 12);
    assert_eq!(overflow_layout.content_width, 26);
    assert_eq!(overflow_layout.max_scroll_offset().x, 10);
}

#[test]
fn native_flex_shorthand_expands_zero_basis_and_preserves_shared_consumers() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:20px;gap:2px'><div id='first' style='width:100px;height:8px;flex:1;background-color:red'><span id='nested' style='display:block;height:4px'>A</span></div><div id='second' style='width:100px;height:8px;flex:1;background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().width, 20);
    assert_eq!(layout.box_for(first).unwrap().x, 0);
    assert_eq!(layout.box_for(first).unwrap().width, 9);
    assert_eq!(layout.box_for(nested).unwrap().width, 9);
    assert_eq!(layout.box_for(second).unwrap().x, 11);
    assert_eq!(layout.box_for(second).unwrap().width, 9);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first && rect.width == 9 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(8, 6).unwrap(), Some(first));
    assert_eq!(layout.hit_test(11, 1).unwrap(), Some(second));
}

#[test]
fn native_flex_flow_shorthand_reuses_reverse_and_wrap_consumers() {
    let document = NativeDocument::parse(
        "<div id='row' style='display:flex;width:12px;gap:2px;flex-flow:row-reverse wrap'><div id='first' style='width:8px;height:6px;margin:1px;flex-shrink:0;background-color:red'>A</div><div id='second' style='width:4px;height:6px;margin:1px;flex-shrink:0;background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 16,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(row).unwrap().width, 12);
    assert_eq!(
        layout.box_for(first),
        Some(NativeRect {
            x: 3,
            y: 1,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(second),
        Some(NativeRect {
            x: 7,
            y: 11,
            width: 4,
            height: 6,
        })
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first && rect.x == 3 && rect.y == 1 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(4, 2).unwrap(), Some(first));
    assert_eq!(layout.hit_test(8, 12).unwrap(), Some(second));
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
        "<style>button:hover, main > button, #ok { color: red; width: 10%; display: grid; opacity: 1.1; text-align: start; text-align: match-parent; text-align-last: match-parent; justify-content: safe center; order: 1025; flex: 1.5 1 8px; flex-flow: column wrap wrap; flex-grow: 1.5; flex-shrink: 1.5; flex-basis: 1.5px; align-items: baseline; align-self: baseline; align-content: safe center; place-content: stretch stretch stretch; flex-direction: column reverse; direction: vertical-rl; flex-wrap: wrap reverse; text-decoration: blink; text-decoration-line: blink; text-decoration-style: zigzag; text-decoration-skip-ink: all; text-decoration-thickness: 5px; text-underline-offset: 5px; text-decoration-color: currentColor; text-transform: capitalize; font-weight: 500; font-style: oblique; word-break: keep-all; text-overflow: fade; overflow: visible; white-space: break-spaces; gap: 1px 2px 3px; row-gap: 4px 5px; column-gap: 5px 6px; custom-property: url(secret); broken; }</style><style>.unclosed { color: blue; </style><button id='ok' style='background-image: url(secret); padding: -1px'>OK</button>",
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
            && diagnostic.detail == "text-align-last"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-grow"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "flex"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-flow"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-shrink"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-basis"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-line"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-style"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-ink"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-thickness"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-underline-offset"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-color"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-transform"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "font-weight"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "font-style"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "word-break"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-overflow"
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
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "gap"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "justify-content"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "order"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-items"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-self"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-content"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "place-content"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-direction"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "direction"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-wrap"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "row-gap"
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "column-gap"
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

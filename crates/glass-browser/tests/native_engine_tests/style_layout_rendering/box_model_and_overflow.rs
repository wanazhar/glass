use super::*;

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
fn native_dimension_important_priority_reaches_geometry_and_artifact_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
        .box { display:block; margin:0px; padding:0; }
        @layer base {
          #named { width:24px !important; height:12px !important; min-width:4px !important; max-width:80px !important; min-height:6px !important; max-height:90px !important; background-color:red; }
          #rollback { width:30px !important; height:18px !important; min-width:8px !important; max-width:70px !important; min-height:10px !important; max-height:60px !important; background-color:green; }
          #rollback { width:revert-layer !important; height:revert-layer !important; min-width:revert-layer !important; max-width:revert-layer !important; min-height:revert-layer !important; max-height:revert-layer !important; }
          #invalid { width:40px !important; width:bad !important; height:20px !important; height:1px 2px !important; min-width:12px !important; min-width:-1px !important; max-width:60px !important; max-width:50% !important; min-height:14px !important; min-height:auto !important; max-height:50px !important; max-height:revert-layer 1px !important; background-color:blue; }
          #clamped { width:80px !important; height:60px !important; min-width:24px !important; max-width:40px !important; min-height:18px !important; max-height:30px !important; background-color:green; }
          #scroll { width:16px !important; height:12px !important; background-color:red; overflow:hidden; }
          #scroll-child { width:40px !important; height:4px !important; background-color:blue; }
          #normal { width:10px; height:10px; min-width:2px; max-width:20px; min-height:2px; max-height:20px; }
        }
        @layer theme {
          #named { width:48px !important; height:20px !important; min-width:16px !important; max-width:100px !important; min-height:10px !important; max-height:110px !important; }
          #rollback { width:36px !important; height:22px !important; min-width:12px !important; max-width:90px !important; min-height:8px !important; max-height:80px !important; }
          #clamped { width:100px !important; height:70px !important; min-width:32px !important; max-width:50px !important; min-height:20px !important; max-height:40px !important; }
          #scroll { width:30px !important; height:20px !important; }
          #scroll-child { width:50px !important; height:6px !important; }
          #normal { width:20px; height:20px; min-width:4px; max-width:40px; min-height:4px; max-height:40px; }
        }
        #named { width:64px !important; height:28px !important; min-width:24px !important; max-width:120px !important; min-height:14px !important; max-height:130px !important; }
        #rollback { width:72px !important; height:32px !important; min-width:28px !important; max-width:140px !important; min-height:16px !important; max-height:150px !important; }
        #normal { width:32px; height:32px; min-width:6px; max-width:60px; min-height:6px; max-height:60px; background-color:blue; }
        #inline { width:64px !important; height:28px !important; min-width:24px !important; max-width:120px !important; min-height:14px !important; max-height:130px !important; background-color:red; }
        </style>
        <div id='named' class='box'>Named</div>
        <div id='rollback' class='box'>Rollback</div>
        <div id='invalid' class='box'>Invalid</div>
        <div id='normal' class='box'>Normal</div>
        <div id='clamped' class='box'>Clamped</div>
        <div id='inline' class='box' style='width:91px !important;height:33px !important;min-width:31px !important;max-width:141px !important;min-height:17px !important;max-height:151px !important'>Inline</div>
        <div id='scroll' class='box'><div id='scroll-child' class='box'>Child</div></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(
        !document
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty)
    );
    for property in [
        "width",
        "height",
        "min-width",
        "max-width",
        "min-height",
        "max-height",
    ] {
        assert!(document.diagnostics().iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == property
        }));
    }

    let viewport = Viewport {
        width: 120,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let named = document.resolve_target("id=named").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let clamped = document.resolve_target("id=clamped").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let scroll = document.resolve_target("id=scroll").unwrap();
    let scroll_child = document.resolve_target("id=scroll-child").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(named),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 12,
        })
    );
    assert_eq!(
        layout.box_for(rollback),
        Some(NativeRect {
            x: 0,
            y: 12,
            width: 36,
            height: 22,
        })
    );
    assert_eq!(
        layout.box_for(invalid),
        Some(NativeRect {
            x: 0,
            y: 34,
            width: 40,
            height: 20,
        })
    );
    assert_eq!(
        layout.box_for(normal),
        Some(NativeRect {
            x: 0,
            y: 54,
            width: 32,
            height: 32,
        })
    );
    assert_eq!(
        layout.box_for(clamped),
        Some(NativeRect {
            x: 0,
            y: 86,
            width: 40,
            height: 30,
        })
    );
    assert_eq!(
        layout.box_for(inline),
        Some(NativeRect {
            x: 0,
            y: 116,
            width: 91,
            height: 33,
        })
    );
    assert_eq!(
        layout.box_for(scroll),
        Some(NativeRect {
            x: 0,
            y: 149,
            width: 16,
            height: 12,
        })
    );
    assert_eq!(
        layout.box_for(scroll_child),
        Some(NativeRect {
            x: 0,
            y: 149,
            width: 16,
            height: 4,
        })
    );
    assert_eq!(layout.hit_test(23, 1), Ok(Some(named)));
    assert_eq!(layout.hit_test(35, 13), Ok(Some(rollback)));
    assert!(!matches!(
        layout.hit_test(20, 150),
        Ok(Some(hit))
            if [named, rollback, invalid, normal, clamped, inline, scroll, scroll_child]
                .contains(&hit)
    ));

    let list = document.display_list(viewport).unwrap();
    for (node_id, rect, color) in [
        (
            named,
            NativeRect {
                x: 0,
                y: 0,
                width: 24,
                height: 12,
            },
            NativeColor::RED,
        ),
        (
            rollback,
            NativeRect {
                x: 0,
                y: 12,
                width: 36,
                height: 22,
            },
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },
        ),
        (
            clamped,
            NativeRect {
                x: 0,
                y: 86,
                width: 40,
                height: 30,
            },
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },
        ),
    ] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect {
                    node_id: painted_node,
                    rect: painted_rect,
                    color: painted_color,
                    ..
                } if *painted_node == node_id
                    && *painted_rect == rect
                    && *painted_color == color
            )
        }));
    }
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(23, 11), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(35, 33), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(39, 115), Some([0, 128, 0, 255]));
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (120, 180));
}
#[test]
fn native_box_model_important_priority_reaches_geometry_and_artifact_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
        .box { display:block; margin:0px; padding:0px; }
        @layer base {
          #named { width:12px; height:6px; box-sizing:content-box !important; padding:1px 2px 3px 4px !important; margin:1px 2px 3px 4px !important; background-color:red; }
          #border { width:20px; height:12px; box-sizing:border-box !important; padding:2px 3px 4px 5px !important; border:1px solid blue; background-color:green; }
          #rollback { width:12px; height:8px; box-sizing:content-box !important; padding:1px !important; margin:1px !important; background-color:blue; }
          #rollback { box-sizing:revert-layer !important; padding:revert-layer !important; margin:revert-layer !important; }
          #flex { display:flex; width:40px; height:12px; }
          #first { width:8px; height:6px; background-color:blue; }
          #auto { width:8px; height:6px; margin:1px 1px 2px auto !important; background-color:red; }
          #clip { width:10px; height:8px; overflow:hidden; background-color:green; }
          #clip-child { width:24px; height:4px; background-color:blue; }
        }
        @layer theme {
          #rollback { box-sizing:border-box !important; padding:2px 3px 4px 5px !important; margin:2px 3px 4px 5px !important; }
          #auto { margin-left:4px !important; margin-top:4px !important; }
        }
        #named { box-sizing:border-box; padding:9px; margin:9px; }
        #border { box-sizing:content-box; padding:9px; margin:9px; }
        #rollback { box-sizing:content-box; padding:9px; margin:9px; }
        #auto { margin-left:0px; margin-top:0px; }
        </style>
        <button id='named' class='box'>Named</button>
        <button id='border' class='box'>Border</button>
        <button id='rollback' class='box'>Rollback</button>
        <div id='flex'><button id='first'>First</button><button id='auto'>Auto</button></div>
        <div id='clip'><div id='clip-child'>Child</div></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            || (diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && matches!(
                    diagnostic.detail.as_str(),
                    "box-sizing" | "padding" | "padding-left" | "margin" | "margin-left"
                ))
    }));

    let named = document.resolve_target("id=named").unwrap();
    let border = document.resolve_target("id=border").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let flex = document.resolve_target("id=flex").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let clip = document.resolve_target("id=clip").unwrap();
    let clip_child = document.resolve_target("id=clip-child").unwrap();

    let viewport = Viewport {
        width: 64,
        height: 128,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let named_rect = layout.box_for(named).unwrap();
    assert_eq!((named_rect.width, named_rect.height), (18, 10));
    let border_rect = layout.box_for(border).unwrap();
    assert_eq!((border_rect.width, border_rect.height), (20, 12));
    let border_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == border)
        .unwrap();
    assert_eq!(
        border_box.content_rect,
        NativeRect {
            x: border_rect.x + 6,
            y: border_rect.y + 3,
            width: 10,
            height: 4,
        }
    );
    let rollback_rect = layout.box_for(rollback).unwrap();
    assert_eq!((rollback_rect.width, rollback_rect.height), (12, 8));
    let rollback_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == rollback)
        .unwrap();
    assert_eq!(
        rollback_box.content_rect,
        NativeRect {
            x: rollback_rect.x + 5,
            y: rollback_rect.y + 2,
            width: 4,
            height: 2,
        }
    );

    let flex_rect = layout.box_for(flex).unwrap();
    let first_rect = layout.box_for(first).unwrap();
    let auto_rect = layout.box_for(auto).unwrap();
    assert_eq!((flex_rect.width, flex_rect.height), (40, 12));
    assert_eq!(
        (first_rect.x - flex_rect.x, first_rect.y - flex_rect.y),
        (0, 0)
    );
    assert_eq!(
        (auto_rect.x - flex_rect.x, auto_rect.y - flex_rect.y),
        (31, 1)
    );
    assert_eq!(
        layout.hit_test((auto_rect.x + 1).into(), (auto_rect.y + 1).into()),
        Ok(Some(auto))
    );

    let clip_rect = layout.box_for(clip).unwrap();
    assert_eq!((clip_rect.width, clip_rect.height), (10, 8));
    assert_eq!(layout.box_for(clip_child).unwrap().width, 10);
    assert_eq!(layout.max_scroll_offset().x, 0);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|semantic_node| semantic_node.node_id == named)
            < semantic_nodes
                .iter()
                .position(|semantic_node| semantic_node.node_id == auto)
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == named
                    && *rect == named_rect
                    && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == auto
                    && *rect == auto_rect
                    && *color == NativeColor::RED
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(auto_rect.x + 1, auto_rect.y + 1),
        Some([255, 0, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (64, 128));
}
#[test]
fn native_logical_box_edges_map_and_feed_all_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
        @layer base {
          #ltr { display:block; width:20px; height:10px; padding-block:1px 2px; padding-inline:3px 4px; margin-block:1px 2px; margin-inline:3px 4px; background-color:red; }
          #rtl { display:block; direction:rtl; width:20px; height:10px; padding-block:1px 2px; padding-inline:3px 4px; margin-block:1px 2px; margin-inline:3px 4px; background-color:blue; }
          #priority { display:block; direction:ltr; width:10px; height:6px; padding:1px; padding-inline-start:5px !important; }
          #flex { display:flex; width:40px; height:12px; background-color:blue; }
          #auto { width:8px; height:6px; margin-inline:auto; background-color:green; }
          #clip { display:block; width:10px; height:8px; padding-block:1px; padding-inline:2px 3px; overflow:hidden; background-color:green; }
          #clip-child { display:block; width:24px; height:4px; background-color:red; }
        }
        @layer theme {
          #priority { padding-inline-start:7px !important; }
        }
        #priority { padding-left:9px !important; }
        </style>
        <div id='ltr' role='button'>Ltr</div>
        <div id='rtl' role='button'>Rtl</div>
        <div id='priority' role='button'>Priority</div>
        <div id='flex'><div id='auto'>Auto</div></div>
        <div id='clip' role='button'><div id='clip-child'>Child</div></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
            || (diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && matches!(
                    diagnostic.detail.as_str(),
                    "padding-block"
                        | "padding-block-start"
                        | "padding-inline"
                        | "padding-inline-start"
                        | "margin-block"
                        | "margin-inline"
                ))
    }));

    let viewport = Viewport {
        width: 64,
        height: 128,
        device_scale_factor_milli: 1000,
    };
    let ltr = document.resolve_target("id=ltr").unwrap();
    let rtl = document.resolve_target("id=rtl").unwrap();
    let priority = document.resolve_target("id=priority").unwrap();
    let flex = document.resolve_target("id=flex").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let clip = document.resolve_target("id=clip").unwrap();
    let clip_child = document.resolve_target("id=clip-child").unwrap();
    let layout = document.layout(viewport).unwrap();

    let ltr_rect = layout.box_for(ltr).unwrap();
    assert_eq!((ltr_rect.width, ltr_rect.height), (27, 13));
    let ltr_box = layout
        .boxes
        .iter()
        .find(|item| item.node_id == ltr)
        .unwrap();
    assert_eq!(
        ltr_box.content_rect,
        NativeRect {
            x: ltr_rect.x + 3,
            y: ltr_rect.y + 1,
            width: 20,
            height: 10,
        }
    );

    let rtl_rect = layout.box_for(rtl).unwrap();
    assert_eq!((rtl_rect.width, rtl_rect.height), (27, 13));
    let rtl_box = layout
        .boxes
        .iter()
        .find(|item| item.node_id == rtl)
        .unwrap();
    assert_eq!(
        rtl_box.content_rect,
        NativeRect {
            x: rtl_rect.x + 4,
            y: rtl_rect.y + 1,
            width: 20,
            height: 10,
        }
    );

    let priority_rect = layout.box_for(priority).unwrap();
    assert_eq!((priority_rect.width, priority_rect.height), (16, 8));
    let priority_box = layout
        .boxes
        .iter()
        .find(|item| item.node_id == priority)
        .unwrap();
    assert_eq!(priority_box.content_rect.x, priority_rect.x + 5);

    let flex_rect = layout.box_for(flex).unwrap();
    let auto_rect = layout.box_for(auto).unwrap();
    assert_eq!((flex_rect.width, flex_rect.height), (40, 12));
    assert_eq!(
        (auto_rect.x - flex_rect.x, auto_rect.y - flex_rect.y),
        (16, 0)
    );
    assert_eq!(
        layout.hit_test((auto_rect.x + 1).into(), (auto_rect.y + 1).into()),
        Ok(Some(auto))
    );

    let clip_rect = layout.box_for(clip).unwrap();
    assert_eq!((clip_rect.width, clip_rect.height), (15, 10));
    assert_eq!(
        layout.viewport_rect_for(clip_child),
        Some(NativeRect {
            x: clip_rect.x + 2,
            y: clip_rect.y + 1,
            width: 10,
            height: 4,
        })
    );
    assert_eq!(
        layout.hit_test((clip_rect.x + 14).into(), (clip_rect.y + 2).into()),
        Ok(Some(clip))
    );

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [ltr, rtl, priority, clip].windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == ltr && *rect == ltr_rect && *color == NativeColor::RED
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(ltr_rect.x + 1, ltr_rect.y + 1),
        Some([255, 0, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (64, 128));
}
#[test]
fn native_box_model_inherit_preserves_parent_values_and_artifact_owners() {
    let document = NativeDocument::parse(
        r#"<style>
        #parent { display:block; width:20px; height:10px; padding:2px 3px 4px 5px; margin:1px 2px 3px 4px; box-sizing:border-box; background-color:red; }
        #child { display:block; width:8px; height:4px; padding:inherit; margin:inherit; box-sizing:inherit; background-color:blue; }
        #logical-parent { display:block; direction:ltr; width:12px; height:8px; padding-inline:3px 5px; margin-inline:auto 7px; background-color:green; }
        #logical-child { display:block; direction:rtl; width:4px; height:4px; padding-inline-start:inherit; padding-inline-end:inherit; margin-inline-start:inherit; margin-inline-end:inherit; background-color:blue; }
        </style>
        <div id='parent' role='button'><div id='child' role='button'>Child</div></div>
        <div id='logical-parent' role='button'><div id='logical-child' role='button'>Logical</div></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !matches!(
            diagnostic.code,
            NativeDiagnosticCode::UnsupportedCssProperty
                | NativeDiagnosticCode::UnsupportedCssValue
        )
    }));

    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let logical_parent = document.resolve_target("id=logical-parent").unwrap();
    let logical_child = document.resolve_target("id=logical-child").unwrap();

    let viewport = Viewport {
        width: 96,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let parent_rect = layout.box_for(parent).unwrap();
    let child_rect = layout.box_for(child).unwrap();
    assert!(child_rect.x >= parent_rect.x);
    assert!(child_rect.y >= parent_rect.y);
    assert!(child_rect.width > 0 && child_rect.width <= 8);
    assert_eq!(child_rect.height, 4);
    assert!(layout.box_for(logical_parent).is_some());
    assert!(
        layout
            .box_for(logical_child)
            .is_some_and(|rect| rect.width > 0)
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == parent && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == child
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(child_rect.x + 1, child_rect.y + 1),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        layout.hit_test((child_rect.x + 1).into(), (child_rect.y + 1).into()),
        Ok(Some(child))
    );

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [parent, child, logical_parent, logical_child].windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }

    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (96, 96));
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
    assert_ne!(layout.hit_test(5, 10).unwrap(), Some(child));

    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 5), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 15), Some([255, 255, 255, 255]));
}
#[test]
fn native_overflow_inherit_reaches_axis_clips_scroll_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#clip-parent{display:block;width:32px;height:20px;overflow-x:hidden;overflow-y:visible;}#visible-parent{display:block;width:32px;height:10px;overflow:visible;}#inherit,#axis,#omitted,#invalid{display:block;width:16px;height:10px;margin:0px;padding:0;border:0;}#inherit{overflow:inherit;background-color:red;}#axis{overflow-x:inherit;overflow-y:visible;background-color:green;}#omitted{background-color:green;}#invalid{overflow:hidden;overflow:inherit hidden;background-color:blue;}.wide{display:block;width:56px;min-width:56px;height:10px;background-color:blue;}</style><div id='clip-parent'><button id='inherit'><span id='inherit-child' class='wide'></span></button><button id='axis'><span id='axis-child' class='wide'></span></button></div><div id='visible-parent'><button id='omitted'><span id='omitted-child' class='wide'></span></button></div><button id='invalid'><span id='invalid-child' class='wide'></span></button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let inherit_child = document.resolve_target("id=inherit-child").unwrap();
    let axis = document.resolve_target("id=axis").unwrap();
    let axis_child = document.resolve_target("id=axis-child").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let omitted_child = document.resolve_target("id=omitted-child").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let invalid_child = document.resolve_target("id=invalid-child").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(inherit_child).unwrap().width, 56);
    assert_eq!(layout.box_for(axis_child).unwrap().width, 56);
    assert_eq!(layout.box_for(omitted_child).unwrap().width, 56);
    assert_eq!(layout.box_for(invalid_child).unwrap().width, 56);
    assert_eq!(
        layout.viewport_rect_for(inherit_child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(axis_child),
        Some(NativeRect {
            x: 0,
            y: 10,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(omitted_child),
        Some(NativeRect {
            x: 0,
            y: 20,
            width: 32,
            height: 10,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(invalid_child),
        Some(NativeRect {
            x: 0,
            y: 30,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(layout.content_width, 56);
    assert_eq!(layout.max_scroll_offset().x, 24);
    assert_eq!(layout.hit_test(8, 5).unwrap(), Some(inherit_child));
    assert_ne!(layout.hit_test(20, 5).unwrap(), Some(inherit_child));
    assert_eq!(layout.hit_test(20, 25).unwrap(), Some(omitted_child));
    assert_ne!(layout.hit_test(20, 35).unwrap(), Some(invalid_child));

    let semantic_nodes = document.semantic_nodes();
    let source_position = |node_id| {
        semantic_nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .unwrap()
    };
    assert!(source_position(inherit) < source_position(axis));
    assert!(source_position(axis) < source_position(omitted));
    assert!(source_position(omitted) < source_position(invalid));

    let list = document.display_list(viewport).unwrap();
    let fill = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                rect,
                clip,
                ..
            } if *command_node_id == node_id => Some((*rect, *clip)),
            _ => None,
        })
    };
    let (inherit_fill_rect, inherit_fill_clip) = fill(inherit_child).unwrap();
    assert_eq!(
        inherit_fill_rect,
        NativeRect {
            x: 0,
            y: 0,
            width: 56,
            height: 10,
        }
    );
    assert_eq!(
        inherit_fill_clip.map(|clip| (clip.x, clip.y, clip.width, clip.height)),
        Some((0, 0, 16, u32::MAX))
    );
    let (axis_fill_rect, axis_fill_clip) = fill(axis_child).unwrap();
    assert_eq!(
        axis_fill_rect,
        NativeRect {
            x: 0,
            y: 10,
            width: 56,
            height: 10,
        }
    );
    assert_eq!(
        axis_fill_clip.map(|clip| (clip.x, clip.y, clip.width, clip.height)),
        Some((0, 0, 16, u32::MAX))
    );
    assert_eq!(
        fill(omitted_child),
        Some((layout.box_for(omitted_child).unwrap(), None))
    );
    let (invalid_fill_rect, invalid_fill_clip) = fill(invalid_child).unwrap();
    assert_eq!(
        invalid_fill_rect,
        NativeRect {
            x: 0,
            y: 30,
            width: 56,
            height: 10,
        }
    );
    assert_eq!(
        invalid_fill_clip,
        Some(NativeRect {
            x: 0,
            y: 30,
            width: 16,
            height: 10,
        })
    );

    let surface = list.rasterize().unwrap();
    assert_ne!(surface.pixel(20, 5), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(20, 25), Some([0, 0, 255, 255]));
    assert_ne!(surface.pixel(20, 35), Some([0, 0, 255, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "overflow"
    }));
}
#[test]
fn native_overflow_css_wide_resets_reach_axis_clips_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#initial,#x-reset,#y-reset,#revert,#invalid,#important,#important-reset{display:block;width:16px;height:10px;margin:0px;padding:0;border:0;}#initial{overflow:hidden;overflow:InItIaL;}#x-reset{overflow-x:hidden;overflow-x:INITIAL;overflow-y:clip;}#y-reset{overflow-x:clip;overflow-y:hidden;overflow-y:unset;}#revert{overflow:clip;overflow:ReVeRt;}#invalid{overflow:hidden;overflow:initial hidden;}#important{overflow:hidden !important;}#important{overflow:initial;}#important-reset{overflow:hidden;}#important-reset{overflow:initial !important;}.wide{display:block;width:56px;min-width:56px;height:10px;background-color:blue;}</style><button id='initial'><span id='initial-child' class='wide'></span></button><button id='x-reset'><span id='x-reset-child' class='wide'></span></button><button id='y-reset'><span id='y-reset-child' class='wide'></span></button><button id='revert'><span id='revert-child' class='wide'></span></button><button id='invalid'><span id='invalid-child' class='wide'></span></button><button id='important'><span id='important-child' class='wide'></span></button><button id='important-reset'><span id='important-reset-child' class='wide'></span></button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let initial_child = document.resolve_target("id=initial-child").unwrap();
    let x_reset = document.resolve_target("id=x-reset").unwrap();
    let x_reset_child = document.resolve_target("id=x-reset-child").unwrap();
    let y_reset = document.resolve_target("id=y-reset").unwrap();
    let y_reset_child = document.resolve_target("id=y-reset-child").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let revert_child = document.resolve_target("id=revert-child").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let invalid_child = document.resolve_target("id=invalid-child").unwrap();
    let important = document.resolve_target("id=important").unwrap();
    let important_child = document.resolve_target("id=important-child").unwrap();
    let important_reset = document.resolve_target("id=important-reset").unwrap();
    let important_reset_child = document.resolve_target("id=important-reset-child").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 80,
        device_scale_factor_milli: 1000,
    };

    let layout = document.layout(viewport).unwrap();
    let expected_visible = |y| {
        Some(NativeRect {
            x: 0,
            y,
            width: 32,
            height: 10,
        })
    };
    let expected_x_clip = |y| {
        Some(NativeRect {
            x: 0,
            y,
            width: 16,
            height: 10,
        })
    };
    assert_eq!(layout.viewport_rect_for(initial_child), expected_visible(0));
    assert_eq!(
        layout.viewport_rect_for(x_reset_child),
        expected_visible(10)
    );
    assert_eq!(layout.viewport_rect_for(y_reset_child), expected_x_clip(20));
    assert_eq!(layout.viewport_rect_for(revert_child), expected_visible(30));
    assert_eq!(layout.viewport_rect_for(invalid_child), expected_x_clip(40));
    assert_eq!(
        layout.viewport_rect_for(important_child),
        expected_x_clip(50)
    );
    assert_eq!(
        layout.viewport_rect_for(important_reset_child),
        expected_visible(60)
    );
    assert_eq!(layout.content_width, 56);
    assert_eq!(layout.max_scroll_offset().x, 24);
    assert_eq!(layout.hit_test(20, 5).unwrap(), Some(initial_child));
    assert_eq!(layout.hit_test(20, 15).unwrap(), Some(x_reset_child));
    assert_ne!(layout.hit_test(20, 25).unwrap(), Some(y_reset_child));
    assert_eq!(layout.hit_test(20, 35).unwrap(), Some(revert_child));
    assert_ne!(layout.hit_test(20, 45).unwrap(), Some(invalid_child));
    assert_ne!(layout.hit_test(20, 55).unwrap(), Some(important_child));
    assert_eq!(
        layout.hit_test(20, 65).unwrap(),
        Some(important_reset_child)
    );

    let semantic_nodes = document.semantic_nodes();
    let source_position = |node_id| {
        semantic_nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .unwrap()
    };
    assert!(source_position(initial) < source_position(x_reset));
    assert!(source_position(x_reset) < source_position(y_reset));
    assert!(source_position(y_reset) < source_position(revert));
    assert!(source_position(revert) < source_position(invalid));
    assert!(source_position(invalid) < source_position(important));
    assert!(source_position(important) < source_position(important_reset));

    let list = document.display_list(viewport).unwrap();
    let fill = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                rect,
                clip,
                ..
            } if *command_node_id == node_id => Some((*rect, *clip)),
            _ => None,
        })
    };
    for child in [initial_child, revert_child, important_reset_child] {
        let (rect, clip) = fill(child).unwrap();
        assert_eq!(rect.width, 56);
        assert_eq!(clip, None);
    }
    for child in [invalid_child, important_child] {
        let (rect, clip) = fill(child).unwrap();
        assert_eq!(rect.width, 56);
        assert!(clip.is_some());
    }

    let surface = list.rasterize().unwrap();
    for (x, y) in [(20, 5), (20, 15), (20, 35), (20, 65)] {
        assert_eq!(surface.pixel(x, y), Some([0, 0, 255, 255]));
    }
    for y in [25, 45, 55] {
        assert_ne!(surface.pixel(20, y), Some([0, 0, 255, 255]));
    }
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "overflow"
    }));
}
#[test]
fn native_overflow_revert_layer_preserves_axis_clips_across_consumers() {
    let document = NativeDocument::parse(
        "<style>@layer base { #named { overflow: hidden; width: 16px; height: 10px; white-space: nowrap; } #inline { overflow: hidden; width: 16px; height: 10px; white-space: nowrap; } #axis { overflow-y: clip; width: 16px; height: 10px; white-space: nowrap; } } @layer theme { #named { overflow-x: clip; overflow-y: revert-layer; } #axis { overflow-x: hidden; } } @layer top { #named { overflow: revert-layer; } #axis { overflow-x: revert-layer; } } #named { overflow: revert-layer; } #fallback { overflow: revert-layer; width: 16px; height: 10px; white-space: nowrap; }</style><div id='named'><div id='named-child' style='display:inline-block;width:56px;height:20px;background-color:red'>Named</div></div><div id='fallback'><div id='fallback-child' style='display:inline-block;width:56px;height:20px;background-color:red'>Fallback</div></div><div id='inline' style='overflow:ReVeRt-LaYeR'><div id='inline-child' style='display:inline-block;width:56px;height:20px;background-color:red'>Inline</div></div><div id='axis'><div id='axis-child' style='display:inline-block;width:56px;height:20px;background-color:red'>Axis</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let named_child = document.resolve_target("id=named-child").unwrap();
    let fallback_child = document.resolve_target("id=fallback-child").unwrap();
    let inline_child = document.resolve_target("id=inline-child").unwrap();
    let axis_child = document.resolve_target("id=axis-child").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.viewport_rect_for(named_child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(fallback_child),
        Some(NativeRect {
            x: 0,
            y: 10,
            width: 16,
            height: 20,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(inline_child),
        Some(NativeRect {
            x: 0,
            y: 20,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(axis_child),
        Some(NativeRect {
            x: 0,
            y: 30,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(layout.content_width, 64);
    assert_eq!(layout.max_scroll_offset().x, 32);
    assert_eq!(layout.hit_test(5, 15).unwrap(), Some(fallback_child));
    assert_eq!(layout.hit_test(5, 35).unwrap(), Some(axis_child));
    assert_eq!(layout.hit_test(5, 40).unwrap(), None);

    let list = document.display_list(viewport).unwrap();
    let text_clip_for = |node_id| {
        list.commands.iter().find_map(|command| {
            if let NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                clip,
                ..
            } = command
                && *command_node_id == node_id
            {
                Some(*clip)
            } else {
                None
            }
        })
    };
    assert_eq!(
        text_clip_for(named_child),
        Some(Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 10,
        }))
    );
    assert_eq!(text_clip_for(fallback_child), Some(None));
    assert_eq!(
        text_clip_for(inline_child),
        Some(Some(NativeRect {
            x: 0,
            y: 20,
            width: 16,
            height: 10,
        }))
    );
    assert_eq!(
        text_clip_for(axis_child),
        Some(Some(NativeRect {
            x: 0,
            y: 30,
            width: 16,
            height: 10,
        }))
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(15, 5), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(16, 5), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(5, 15), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(15, 25), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(16, 25), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(5, 40), Some([255, 255, 255, 255]));

    assert!(!document.diagnostics().iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            NativeDiagnosticCode::UnsupportedCssProperty
                | NativeDiagnosticCode::UnsupportedCssValue
        ) && matches!(
            diagnostic.detail.as_str(),
            "overflow" | "overflow-x" | "overflow-y"
        )
    }));
}
#[test]
fn native_overflow_important_priority_reaches_projection_and_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
        .box { display:block; width:16px; height:10px; margin:0px; background-color:blue; }
        .child { display:block; width:56px; min-width:56px; height:20px; background-color:red; }
        @layer base {
          #named { overflow-x:hidden !important; }
          #rollback { overflow:hidden !important; overflow:revert-layer !important; }
        }
        @layer theme {
          #named { overflow-x:clip !important; }
          #rollback { overflow:clip !important; }
        }
        #named { overflow-x:clip; }
        #inline { overflow:revert-layer !important; }
        </style>
        <button id='named' class='box'><span id='named-child' class='child'></span></button>
        <button id='rollback' class='box'><span id='rollback-child' class='child'></span></button>
        <button id='normal' class='box'><span id='normal-child' class='child'></span></button>
        <button id='inline' class='box' style='overflow:hidden !important'><span id='inline-child' class='child'></span></button>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overflow_details = ["overflow", "overflow-x", "overflow-y"];
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            NativeDiagnosticCode::UnsupportedCssProperty
                | NativeDiagnosticCode::UnsupportedCssValue
        ) && overflow_details.contains(&diagnostic.detail.as_str())
    }));

    let named = document.resolve_target("id=named").unwrap();
    let named_child = document.resolve_target("id=named-child").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let rollback_child = document.resolve_target("id=rollback-child").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let normal_child = document.resolve_target("id=normal-child").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let inline_child = document.resolve_target("id=inline-child").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.viewport_rect_for(named_child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 16,
            height: 20,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(rollback_child),
        Some(NativeRect {
            x: 0,
            y: 10,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(normal_child),
        Some(NativeRect {
            x: 0,
            y: 20,
            width: 32,
            height: 20,
        })
    );
    assert_eq!(
        layout.viewport_rect_for(inline_child),
        Some(NativeRect {
            x: 0,
            y: 30,
            width: 16,
            height: 10,
        })
    );
    assert_eq!(layout.hit_test(5, 5).unwrap(), Some(named_child));
    assert_ne!(layout.hit_test(16, 5).unwrap(), Some(named_child));
    assert_eq!(layout.hit_test(5, 15).unwrap(), Some(rollback_child));
    assert_eq!(layout.hit_test(5, 25).unwrap(), Some(normal_child));

    let list = document.display_list(viewport).unwrap();
    let child_clip_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                clip,
                ..
            } if *command_node_id == node_id => Some(*clip),
            _ => None,
        })
    };
    assert!(child_clip_for(named_child).is_some());
    assert!(child_clip_for(rollback_child).is_some());
    assert_eq!(child_clip_for(normal_child), Some(None));
    assert!(child_clip_for(inline_child).is_some());

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(5, 5), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(16, 5), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(5, 15), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 20), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(30, 25), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 35), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 40), Some([255, 255, 255, 255]));
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (32, 64));

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [named, rollback, normal, inline].windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }

    let scroll_document = NativeDocument::parse(
        r#"<style>
        @layer base { #scroll { overflow-x:visible !important; overflow-y:hidden !important; } }
        @layer theme { #scroll { overflow-x:hidden !important; overflow-y:visible !important; } }
        #scroll { overflow:clip; }
        </style>
        <button id='scroll' style='display:block;width:16px;height:10px;margin:0px'><span id='scroll-child' style='display:block;width:56px;min-width:56px;height:20px;background-color:red'></span></button>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let scroll_child = scroll_document.resolve_target("id=scroll-child").unwrap();
    let scroll_layout = scroll_document
        .layout(Viewport {
            width: 32,
            height: 32,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(scroll_layout.content_width, 56);
    assert_eq!(
        scroll_layout.max_scroll_offset(),
        NativePoint { x: 24, y: 0 }
    );
    assert_eq!(
        scroll_layout.viewport_rect_for(scroll_child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 32,
            height: 10,
        })
    );
}
#[test]
fn native_dimension_css_wide_resets_reach_layout_and_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
        #parent { width:24px; height:12px; background-color:blue; }
        #reset-child { display:block; width:4px; height:4px; min-width:20px; max-width:22px; min-height:6px; max-height:8px; background-color:red; }
        #reset-child { width:initial; height:unset; min-width:revert; max-width:initial; min-height:unset; max-height:revert; }
        </style>
        <div id='parent'><button id='reset-child'>Reset</button></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=reset-child").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(parent),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 12,
        })
    );
    assert_eq!(
        layout.box_for(child),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 24,
            height: 40,
        })
    );
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(child));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, .. } if *node_id == child
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(
        document
            .semantic_nodes()
            .iter()
            .any(|semantic_node| semantic_node.node_id == child)
    );
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height"
            )
    }));
}
#[test]
fn native_box_model_revert_layer_preserves_geometry_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #box { display:block; width:32px; height:20px; padding:2px; border:1px solid red; box-sizing:content-box; margin:1px 2px 3px 4px; background-color:red; } } @layer theme { #box { padding:4px 5px 6px 7px; box-sizing:border-box; margin:5px 6px 7px 8px; } } #box { padding:ReVeRt-LaYeR; box-sizing:revert-layer; margin:revert-layer; }</style><button id='box'>Box</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let box_id = document.resolve_target("id=box").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(box_id),
        Some(NativeRect {
            x: 8,
            y: 5,
            width: 32,
            height: 20,
        })
    );
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == box_id)
            .unwrap()
            .content_rect,
        NativeRect {
            x: 16,
            y: 10,
            width: 18,
            height: 8,
        }
    );
    assert_eq!(layout.hit_test(16, 10).unwrap(), Some(box_id));
    assert!(
        document
            .semantic_nodes()
            .iter()
            .any(|semantic_node| semantic_node.node_id == box_id)
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == box_id
                    && *rect == (NativeRect { x: 8, y: 5, width: 32, height: 20 })
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(12, 8), Some([255, 0, 0, 255]));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "box-sizing" | "padding" | "margin"
            )
    }));
}
#[test]
fn native_svg_viewbox_preserve_aspect_ratio_maps_shared_geometry() {
    let render = |viewbox: &str, preserve: Option<&str>, shape: &str| {
        let preserve_attribute = preserve
            .map(|value| format!("preserveAspectRatio='{value}'"))
            .unwrap_or_default();
        let markup = format!(
            "<div style='width:48px;height:28px'><svg width='40' height='20' viewBox='{viewbox}' {preserve_attribute}>{shape}</svg></div>"
        );
        let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
        let node_id = document.resolve_target("id=shape").unwrap();
        let viewport = Viewport {
            width: 48,
            height: 28,
            device_scale_factor_milli: 1000,
        };
        let layout = document.layout(viewport).unwrap();
        let bounds = layout.box_for(node_id).unwrap();
        let list = document.display_list(viewport).unwrap();
        let points = list
            .commands
            .iter()
            .find_map(|command| {
                matches!(command, NativeDisplayCommand::SvgPolygonFill { node_id: command_node, .. }
                    if *command_node == node_id)
                .then(|| match command {
                    NativeDisplayCommand::SvgPolygonFill { points, .. } => points.clone(),
                    _ => unreachable!(),
                })
            })
            .unwrap();
        (bounds, points)
    };

    let (scaled_bounds, scaled_points) = render(
        "0 0 20 10",
        None,
        "<rect id='shape' x='1' y='1' width='4' height='3' fill='blue'></rect>",
    );
    assert_eq!(
        scaled_bounds,
        NativeRect {
            x: 2,
            y: 2,
            width: 9,
            height: 7,
        }
    );
    assert_eq!(
        scaled_points,
        vec![
            NativePoint { x: 2, y: 2 },
            NativePoint { x: 10, y: 2 },
            NativePoint { x: 10, y: 8 },
            NativePoint { x: 2, y: 8 },
        ]
    );

    let (meet_bounds, meet_points) = render(
        "0 0 10 20",
        Some("xMidYMid meet"),
        "<rect id='shape' x='0' y='0' width='10' height='20' fill='red'></rect>",
    );
    assert_eq!(
        meet_bounds,
        NativeRect {
            x: 15,
            y: 0,
            width: 11,
            height: 21,
        }
    );
    assert_eq!(
        meet_points,
        vec![
            NativePoint { x: 15, y: 0 },
            NativePoint { x: 25, y: 0 },
            NativePoint { x: 25, y: 20 },
            NativePoint { x: 15, y: 20 },
        ]
    );

    let (none_bounds, none_points) = render(
        "0 0 10 20",
        Some("none"),
        "<rect id='shape' x='0' y='0' width='10' height='20' fill='green'></rect>",
    );
    assert_eq!(
        none_bounds,
        NativeRect {
            x: 0,
            y: 0,
            width: 41,
            height: 21,
        }
    );
    assert_eq!(
        none_points,
        vec![
            NativePoint { x: 0, y: 0 },
            NativePoint { x: 40, y: 0 },
            NativePoint { x: 40, y: 20 },
            NativePoint { x: 0, y: 20 },
        ]
    );

    let (slice_bounds, slice_points) = render(
        "0 0 10 20",
        Some("xMaxYMax slice"),
        "<rect id='shape' x='0' y='15' width='10' height='5' fill='yellow'></rect>",
    );
    assert_eq!(
        slice_bounds,
        NativeRect {
            x: 0,
            y: 0,
            width: 41,
            height: 21,
        }
    );
    assert_eq!(
        slice_points,
        vec![
            NativePoint { x: 0, y: 0 },
            NativePoint { x: 40, y: 0 },
            NativePoint { x: 40, y: 20 },
            NativePoint { x: 0, y: 20 },
        ]
    );
}
#[test]
fn native_css_absolute_position_uses_nearest_positioned_padding_box() {
    let document = NativeDocument::parse(
        "<div style='position:relative;width:30px;height:30px;padding:5px;box-sizing:border-box'><div id='overlay' style='position:absolute;left:3px;top:4px;width:8px;height:8px;background:blue'></div><div id='flow' style='width:5px;height:5px'></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overlay = document.resolve_target("id=overlay").unwrap();
    let flow = document.resolve_target("id=flow").unwrap();
    let layout = document
        .layout(Viewport {
            width: 40,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(
        layout.box_for(overlay),
        Some(NativeRect {
            x: 3,
            y: 4,
            width: 8,
            height: 8,
        })
    );
    assert_eq!(layout.box_for(flow).unwrap().x, 5);
    assert_eq!(layout.box_for(flow).unwrap().y, 5);
}
#[tokio::test]
async fn native_image_element_exposes_complete_intrinsic_dimensions_and_current_src() {
    let source = native_test_png_data_url();
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://image-properties/page")
        .with_fixture(
            "fixture://image-properties/page",
            format!(
                "<img id='image' src='{source}'><img id='empty'><script>globalThis.imageProperties = () => {{ const image = document.getElementById('image'); const empty = document.getElementById('empty'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc, empty.complete, empty.naturalWidth, empty.naturalHeight, empty.currentSrc]; }};</script>"
            ),
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("globalThis.imageProperties()")
            .await
            .unwrap(),
        serde_json::json!([true, 2, 2, source, true, 0, 0, ""])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); image.src = ''; return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 0, 0, ""])
    );

    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_svg_data_images_expose_intrinsic_dimensions_and_paint() {
    let source = native_test_svg_data_url();
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://svg-image/page")
        .with_fixture(
            "fixture://svg-image/page",
            format!(
                "<img id='image' src='{source}'><div id='background' style='width:4px;height:3px;background-image:url({source})'></div>"
            ),
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
        serde_json::json!([true, 4, 3, source])
    );

    let document = NativeDocument::parse(
        &format!(
            "<img id='image' src='{source}'><div id='background' style='width:4px;height:3px;background-image:url({source})'></div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let image_id = document.resolve_target("id=image").unwrap();
    let background_id = document.resolve_target("id=background").unwrap();
    let list = document
        .display_list(Viewport {
            width: 8,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image {
                node_id,
                source_width: 4,
                source_height: 3,
                pixels,
                ..
            } if (*node_id == image_id || *node_id == background_id)
                && pixels.len() == 4 * 3 * 4
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 20), Some([255, 0, 0, 255]));
    engine.close_async().await.unwrap();
}
#[test]
fn native_custom_element_reaction_queue_overflow_is_bounded_and_recovers() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_custom_element_reaction_queue_overflow_is_bounded_and_recovers_inner(),
        );
    });
}

async fn native_custom_element_reaction_queue_overflow_is_bounded_and_recovers_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("native custom-element queue-bound request")
            .unwrap();
        let _request = read_http_request(&mut stream).await;
        let attributes = (0..256)
            .map(|index| format!(" a{index}='value'"))
            .collect::<String>();
        let mut body = String::from("<!doctype html><html><body>");
        for index in 0..17 {
            body.push_str(&format!(
                "<x-reaction-bound id='bound-{index}'{attributes}></x-reaction-bound>"
            ));
        }
        body.push_str("</body></html>");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/custom-element-queue-bound")),
    )
    .await
    .unwrap();
    let result = session
        .script(
            r##"(() => {
              const observedAttributes = Array.from(
                { length: 256 },
                (_, index) => `a${index}`,
              );
              let callbackCount = 0;
              let overflowReportCount = 0;
              let unexpectedReportCount = 0;
              globalThis.reportError = error => {
                if (error instanceof RangeError
                    && String(error.message).includes('reaction queue exceeded its limit')) {
                  overflowReportCount += 1;
                } else {
                  unexpectedReportCount += 1;
                }
              };
              class QueueBoundProbe extends HTMLElement {
                static get observedAttributes() { return observedAttributes; }
                attributeChangedCallback() { callbackCount += 1; }
              }
              customElements.define('x-reaction-bound', QueueBoundProbe);
              const nodes = Array.from(document.querySelectorAll('x-reaction-bound'));

              let postOverflowCallbackCount = 0;
              class PostOverflowProbe extends HTMLElement {
                static get observedAttributes() { return ['data-trigger']; }
                attributeChangedCallback() { postOverflowCallbackCount += 1; }
              }
              customElements.define('x-after-overflow', PostOverflowProbe);
              document.createElement('x-after-overflow').setAttribute('data-trigger', 'recovered');

              return {
                nodeCount: nodes.length,
                upgradedCount: nodes.filter(node => node instanceof QueueBoundProbe).length,
                callbackCount,
                overflowReportCount,
                unexpectedReportCount,
                postOverflowCallbackCount,
              };
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!({
            "nodeCount": 17,
            "upgradedCount": 17,
            "callbackCount": 4096,
            "overflowReportCount": 256,
            "unexpectedReportCount": 0,
            "postOverflowCallbackCount": 1,
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}

#[test]
fn native_logical_borders_map_direction_and_share_physical_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:20px;height:10px;margin:1px; }
            #ltr { direction:ltr;border-block:1px solid red;border-inline:2px solid green; }
            #rtl { direction:rtl;border-inline-width:3px 4px;border-inline-style:solid;border-inline-color:blue; }
            #side { border-block-start:5px dashed black;border-inline-end:6px dotted red; }
            #components { border:1px solid black;border-inline-color:red blue;border-block-style:double; }
            #precedence-a { border-inline:2px solid blue;border-left:7px solid red; }
            #precedence-b { border-left:7px solid red;border-inline:2px solid blue; }
            @layer base { #rollback { border-inline:2px solid green; } }
            @layer theme { #rollback { border-inline-start:revert-layer;border-inline-end:4px solid blue; } }
            #invalid { border:1px solid black;border-inline-width:inherit 1px;border-inline:1px solid red blue; }
            #parent { direction:rtl;border-inline-width:2px 3px;border-inline-style:solid;border-inline-color:red; }
            #child { border-inline:inherit; }
        </style>
        <div id='ltr' class='box' role='button'>LTR</div>
        <div id='rtl' class='box' role='button'>RTL</div>
        <div id='side' class='box' role='button'>Side</div>
        <div id='components' class='box' role='button'>Components</div>
        <div id='precedence-a' class='box' role='button'>Physical later</div>
        <div id='precedence-b' class='box' role='button'>Logical later</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='invalid' class='box' role='button'>Invalid keeps physical</div>
        <div id='parent' class='box' role='button'><div id='child' class='box' role='button'>Inherited RTL</div></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 96,
        height: 420,
        device_scale_factor_milli: 1000,
    };
    let id = |value: &str| document.resolve_target(&format!("id={value}")).unwrap();
    let ltr = id("ltr");
    let rtl = id("rtl");
    let side = id("side");
    let components = id("components");
    let precedence_a = id("precedence-a");
    let precedence_b = id("precedence-b");
    let rollback = id("rollback");
    let invalid = id("invalid");
    let parent = id("parent");
    let child = id("child");
    let targets = [
        ltr,
        rtl,
        side,
        components,
        precedence_a,
        precedence_b,
        rollback,
        invalid,
        parent,
        child,
    ];

    let layout = document.layout(viewport).unwrap();
    for node_id in targets {
        let rectangle = layout.box_for(node_id).expect("logical-border layout box");
        assert!(
            rectangle.width > 0,
            "logical-border width for {node_id:?}: {rectangle:?}"
        );
        assert!(
            rectangle.height >= 10,
            "logical-border height for {node_id:?}: {rectangle:?}"
        );
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id),
            "logical-border hit target {node_id:?}"
        );
    }

    let list = document.display_list(viewport).unwrap();
    let border_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::BorderRect {
                node_id: command_node_id,
                borders,
                ..
            } if *command_node_id == node_id => Some(*borders),
            _ => None,
        })
    };
    let ltr_border = border_for(ltr).expect("ltr logical border command");
    assert_eq!(ltr_border.top.width, 1);
    assert_eq!(ltr_border.bottom.width, 1);
    assert_eq!(ltr_border.left.width, 2);
    assert_eq!(ltr_border.right.width, 2);
    assert_eq!(ltr_border.top.color, NativeColor::RED);
    assert_eq!(
        ltr_border.left.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }
    );

    let rtl_border = border_for(rtl).expect("rtl logical border command");
    assert_eq!(rtl_border.top.width, 0);
    assert_eq!(rtl_border.bottom.width, 0);
    assert_eq!(rtl_border.right.width, 3);
    assert_eq!(rtl_border.left.width, 4);
    assert_eq!(
        rtl_border.right.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );

    let side_border = border_for(side).expect("logical side border command");
    assert_eq!(side_border.top.width, 5);
    assert_eq!(side_border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(side_border.right.width, 6);
    assert_eq!(side_border.right.style, NativeBorderStyle::Dotted);
    assert_eq!(side_border.bottom.width, 0);
    assert_eq!(side_border.left.width, 0);

    let component_border = border_for(components).expect("logical component border command");
    assert_eq!(component_border.top.style, NativeBorderStyle::Double);
    assert_eq!(component_border.bottom.style, NativeBorderStyle::Double);
    assert_eq!(component_border.left.color, NativeColor::RED);
    assert_eq!(
        component_border.right.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );

    let precedence_a_border = border_for(precedence_a).expect("physical-later border command");
    assert_eq!(precedence_a_border.left.width, 7);
    assert_eq!(precedence_a_border.left.color, NativeColor::RED);
    assert_eq!(precedence_a_border.right.width, 2);
    let precedence_b_border = border_for(precedence_b).expect("logical-later border command");
    assert_eq!(precedence_b_border.left.width, 2);
    assert_eq!(
        precedence_b_border.left.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );

    let rollback_border = border_for(rollback).expect("logical rollback border command");
    assert_eq!(rollback_border.left.width, 2);
    assert_eq!(
        rollback_border.left.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }
    );
    assert_eq!(rollback_border.right.width, 4);
    assert_eq!(
        rollback_border.right.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );

    let invalid_border = border_for(invalid).expect("invalid logical declarations preserve border");
    assert_eq!(
        [
            invalid_border.top.width,
            invalid_border.right.width,
            invalid_border.bottom.width,
            invalid_border.left.width,
        ],
        [1, 1, 1, 1]
    );
    let parent_border = border_for(parent).expect("parent logical border command");
    let child_border = border_for(child).expect("inherited logical border command");
    assert_eq!(parent_border.right.width, 2);
    assert_eq!(parent_border.left.width, 3);
    assert_eq!(child_border.right.width, 2);
    assert_eq!(child_border.left.width, 3);

    let semantic_nodes = document.semantic_nodes();
    for node_id in targets {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "logical-border semantic target {node_id:?} should remain visible"
        );
    }
    let surface = list.rasterize().unwrap();
    let ltr_box = layout.box_for(ltr).unwrap();
    assert_eq!(
        surface.pixel(ltr_box.x + ltr_box.width / 2, ltr_box.y),
        Some([255, 0, 0, 255])
    );
    let rtl_box = layout.box_for(rtl).unwrap();
    assert_eq!(
        surface.pixel(
            rtl_box.x + rtl_box.width - 1,
            rtl_box.y + rtl_box.height / 2
        ),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());

    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-inline-width"
    }));
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-inline"
    }));
    for property in [
        "border-block",
        "border-block-start",
        "border-block-end",
        "border-inline",
        "border-inline-start",
        "border-inline-end",
        "border-block-width",
        "border-inline-width",
        "border-block-style",
        "border-inline-style",
        "border-block-color",
        "border-inline-color",
    ] {
        assert!(
            !document.diagnostics().iter().any(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssProperty
                    && diagnostic.detail == property
            }),
            "valid logical property {property} must be recognized"
        );
    }
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

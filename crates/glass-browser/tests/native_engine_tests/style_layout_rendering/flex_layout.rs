use super::*;

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
fn native_direction_revert_layer_preserves_text_flex_and_artifact_owners() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:8px; text-align:start; } @layer base { #named { direction:ltr; } #repeated { direction:ltr; } #inline { direction:ltr; } #parent { direction:rtl; } #row { display:flex; width:20px; height:8px; direction:ltr; flex-direction:row; align-items:flex-start; } #column { display:flex; width:20px; height:12px; direction:ltr; flex-direction:column; align-items:flex-start; } #wrapped { display:flex; width:24px; height:12px; direction:ltr; flex-direction:column; flex-wrap:wrap; row-gap:2px; column-gap:3px; align-items:flex-start; align-content:flex-start; } } @layer theme { #named { direction:rtl; } #repeated { direction:revert-layer; } #inline { direction:rtl; } #row { direction:rtl; } #column { direction:rtl; } #wrapped { direction:rtl; } } @layer top { #repeated { direction:revert-layer; } } #named { direction:revert-layer; } #repeated { direction:revert-layer; } #row { direction:revert-layer; } #column { direction:revert-layer; } #wrapped { direction:revert-layer; } #fallback { direction:revert-layer; } #child { display:block; width:24px; height:8px; text-align:start; }</style><div id='named' class='line'>N</div><div id='repeated' class='line'>R</div><div id='inline' class='line' style='direction:REVERT-LAYER'>I</div><div id='fallback' class='line'>F</div><div id='parent' class='line'><div id='child' style='direction:revert-layer'>C</div></div><div id='row'><div id='row-first' style='width:6px;height:8px;flex-shrink:0;background-color:red'>A</div><div id='row-second' style='width:8px;height:8px;flex-shrink:0;background-color:blue'>B</div></div><div id='column'><div id='column-first' style='width:6px;height:6px;flex-shrink:0;background-color:red'>D</div><div id='column-second' style='width:8px;height:6px;flex-shrink:0;background-color:blue'>E</div></div><div id='wrapped'><div id='wrapped-first' style='width:4px;height:6px;flex-shrink:0;background-color:red'>G</div><div id='wrapped-second' style='width:4px;height:6px;flex-shrink:0;background-color:blue'>H</div><div id='wrapped-third' style='width:4px;height:6px;flex-shrink:0;background-color:green'>J</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "direction")
    }));

    let viewport = Viewport {
        width: 64,
        height: 256,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let text_origin = |id: &str| {
        let node = document.resolve_target(&format!("id={id}")).unwrap();
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node)
            .map(|run| run.origin)
            .unwrap()
    };

    // Unlayered rollback exposes the highest named candidate, while repeated
    // rollback reaches the base layer and a candidate-free root is ltr.
    assert_eq!(text_origin("named").x, 24);
    assert_eq!(text_origin("repeated").x, 0);
    assert_eq!(text_origin("inline").x, 24);
    assert_eq!(text_origin("fallback").x, 0);
    // A child rollback with no local concrete candidate keeps its rtl parent.
    assert_eq!(text_origin("child").x, 16);

    let row = document.resolve_target("id=row").unwrap();
    let row_first = document.resolve_target("id=row-first").unwrap();
    let row_second = document.resolve_target("id=row-second").unwrap();
    let column_first = document.resolve_target("id=column-first").unwrap();
    let column_second = document.resolve_target("id=column-second").unwrap();
    let wrapped_first = document.resolve_target("id=wrapped-first").unwrap();
    let wrapped_second = document.resolve_target("id=wrapped-second").unwrap();
    let wrapped_third = document.resolve_target("id=wrapped-third").unwrap();
    assert_eq!(layout.box_for(row_first).unwrap().x, 14);
    assert_eq!(layout.box_for(row_second).unwrap().x, 6);
    assert_eq!(layout.box_for(column_first).unwrap().x, 14);
    assert_eq!(layout.box_for(column_second).unwrap().x, 12);
    assert_eq!(layout.box_for(wrapped_first).unwrap().x, 20);
    assert_eq!(layout.box_for(wrapped_second).unwrap().x, 13);
    assert_eq!(layout.box_for(wrapped_third).unwrap().x, 6);

    let row_order = layout
        .text_runs
        .iter()
        .filter_map(|run| {
            if run.node_id == row_first {
                Some("first")
            } else if run.node_id == row_second {
                Some("second")
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(row_order, vec!["first", "second"]);
    let row_rect = layout.box_for(row).unwrap();
    assert_eq!(
        layout.hit_test(i64::from(row_rect.x + 15), i64::from(row_rect.y + 1),),
        Ok(Some(row_first))
    );

    let list = document.display_list(viewport).unwrap();
    let row_first_rect = layout.box_for(row_first).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == row_first && *rect == row_first_rect
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(row_first_rect.x + 1, row_first_rect.y + 1),
        Some([255, 0, 0, 255])
    );
}
#[test]
fn native_flex_direction_revert_layer_preserves_non_inherited_flex_owners() {
    let document = NativeDocument::parse(
        "<style>@layer base { #named { display:flex; width:20px; height:8px; flex-direction:row; align-items:flex-start; } #repeated { display:flex; width:20px; height:8px; flex-direction:column; align-items:flex-start; } #inline { display:flex; width:20px; height:12px; flex-direction:row; align-items:flex-start; } #parent { display:flex; width:20px; height:12px; flex-direction:column; align-items:flex-start; } #wrapped { display:flex; width:14px; height:8px; flex-direction:column; flex-wrap:wrap; align-items:flex-start; align-content:flex-start; } } @layer theme { #named { flex-direction:row-reverse; } #repeated { flex-direction:row; } #inline { flex-direction:column; } #wrapped { flex-direction:row; } } @layer top { #repeated { flex-direction:revert-layer; } } #named { flex-direction:revert-layer; } #repeated { flex-direction:revert-layer; } #fallback { display:flex; width:20px; height:8px; flex-direction:revert-layer; align-items:flex-start; }</style><div id='named'><div id='named-first' style='width:6px;height:8px;flex-shrink:0;background-color:red'>A</div><div id='named-second' style='width:8px;height:8px;flex-shrink:0;background-color:blue'>B</div></div><div id='repeated'><div id='repeated-first' style='width:6px;height:8px;flex-shrink:0;background-color:red'>C</div><div id='repeated-second' style='width:8px;height:8px;flex-shrink:0;background-color:blue'>D</div></div><div id='inline' style='flex-direction:REVERT-LAYER'><div id='inline-first' style='width:6px;height:6px;flex-shrink:0;background-color:red'>E</div><div id='inline-second' style='width:8px;height:6px;flex-shrink:0;background-color:blue'>F</div></div><div id='fallback'><div id='fallback-first' style='width:6px;height:8px;flex-shrink:0;background-color:red'>G</div><div id='fallback-second' style='width:8px;height:8px;flex-shrink:0;background-color:blue'>H</div></div><div id='parent'><div id='child' style='display:flex;width:20px;height:8px;flex-direction:revert-layer;align-items:flex-start'><div id='child-first' style='width:6px;height:8px;flex-shrink:0;background-color:red'>I</div><div id='child-second' style='width:8px;height:8px;flex-shrink:0;background-color:blue'>J</div></div></div><div id='wrapped'><div id='wrapped-first' style='width:8px;height:4px;flex-shrink:0;background-color:red'>K</div><div id='wrapped-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>L</div><div id='wrapped-third' style='width:8px;height:4px;flex-shrink:0;background-color:green'>M</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-direction")
    }));

    let viewport = Viewport {
        width: 64,
        height: 256,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let box_for = |id: &str| {
        let node = document.resolve_target(&format!("id={id}")).unwrap();
        layout.box_for(node).unwrap()
    };

    // Unlayered rollback exposes the named theme candidate, while two
    // rollback layers reach the lower named candidate. A local inline rollback
    // falls through to the named column candidate.
    let named = box_for("named");
    assert_eq!(box_for("named-first").x, named.x + 14);
    assert_eq!(box_for("named-second").x, named.x + 6);
    let repeated = box_for("repeated");
    assert_eq!(box_for("repeated-first").x, repeated.x);
    assert_eq!(box_for("repeated-second").x, repeated.x + 6);
    let inline = box_for("inline");
    assert_eq!(box_for("inline-first").y, inline.y);
    assert_eq!(box_for("inline-second").y, inline.y + 6);

    // No concrete candidate leaves the non-inherited root fallback at row.
    let fallback = box_for("fallback");
    assert_eq!(box_for("fallback-first").x, fallback.x);
    assert_eq!(box_for("fallback-second").x, fallback.x + 6);

    // The parent is column, but a child with no local concrete flex-direction
    // candidate still resolves its own non-inherited row fallback.
    let child = box_for("child");
    assert_eq!(box_for("child-first").x, child.x);
    assert_eq!(box_for("child-second").x, child.x + 6);
    assert_eq!(box_for("child-first").y, box_for("child-second").y);

    // Rolling back from base column to theme row keeps source order while
    // changing wrapped-line formation from horizontal columns to row lines.
    let wrapped = box_for("wrapped");
    assert_eq!(box_for("wrapped-first").x, wrapped.x);
    assert_eq!(box_for("wrapped-second").x, wrapped.x);
    assert_eq!(box_for("wrapped-third").x, wrapped.x);
    assert_eq!(box_for("wrapped-second").y, box_for("wrapped-first").y + 4);
    assert_eq!(box_for("wrapped-third").y, box_for("wrapped-first").y + 8);

    let named_first = document.resolve_target("id=named-first").unwrap();
    let named_second = document.resolve_target("id=named-second").unwrap();
    let row_order = layout
        .text_runs
        .iter()
        .filter_map(|run| {
            if run.node_id == named_first {
                Some("first")
            } else if run.node_id == named_second {
                Some("second")
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(row_order, vec!["first", "second"]);

    let named_first_rect = box_for("named-first");
    assert_eq!(
        layout.hit_test(
            i64::from(named_first_rect.x + 1),
            i64::from(named_first_rect.y + 1),
        ),
        Ok(Some(named_first))
    );
    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == named_first && *rect == named_first_rect
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(named_first_rect.x + 1, named_first_rect.y + 1),
        Some([255, 0, 0, 255])
    );
}
#[test]
fn native_flexbox_revert_layer_family_preserves_non_inherited_owners() {
    let document = NativeDocument::parse(
        "<style>@layer base { #justify { display:flex; width:20px; height:12px; justify-content:flex-start; align-items:flex-start; } #repeat { display:flex; width:20px; height:12px; justify-content:flex-start; align-items:flex-start; } #items { display:flex; width:20px; height:12px; align-items:flex-start; } #self { display:flex; width:20px; height:12px; align-items:flex-start; } #content { display:flex; width:14px; height:20px; flex-wrap:wrap; align-items:flex-start; align-content:flex-start; } #wrap { display:flex; width:14px; height:14px; flex-wrap:nowrap; align-items:flex-start; align-content:flex-start; } #parent { display:flex; width:20px; height:8px; justify-content:flex-end; align-items:flex-start; } } @layer theme { #justify { justify-content:flex-end; } #repeat { justify-content:center; } #items { align-items:flex-end; } #self-first { align-self:flex-end; } #content { align-content:flex-end; } #wrap { flex-wrap:wrap; } } @layer top { #repeat { justify-content:revert-layer; } } #justify { justify-content:revert-layer; } #repeat { justify-content:revert-layer; } #self-first { align-self:revert-layer; } #content { align-content:revert-layer; } #wrap { flex-wrap:revert-layer; } #fallback { display:flex; width:20px; height:12px; flex-wrap:revert-layer; justify-content:revert-layer; align-items:revert-layer; align-content:revert-layer; }</style><div id='justify'><div id='justify-first' style='width:6px;height:4px;flex-shrink:0;background-color:red'>A</div><div id='justify-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>B</div></div><div id='repeat'><div id='repeat-first' style='width:6px;height:4px;flex-shrink:0;background-color:red'>C</div><div id='repeat-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>D</div></div><div id='items' style='align-items:REVERT-LAYER'><div id='items-first' style='width:6px;height:4px;flex-shrink:0;background-color:red'>E</div><div id='items-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>F</div></div><div id='self'><div id='self-first' style='width:6px;height:4px;flex-shrink:0;background-color:red'>G</div><div id='self-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>H</div></div><div id='content'><div id='content-first' style='width:8px;height:4px;flex-shrink:0;background-color:red'>I</div><div id='content-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>J</div><div id='content-third' style='width:8px;height:4px;flex-shrink:0;background-color:green'>K</div></div><div id='wrap'><div id='wrap-first' style='width:8px;height:4px;flex-shrink:0;background-color:red'>L</div><div id='wrap-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>M</div><div id='wrap-third' style='width:8px;height:4px;flex-shrink:0;background-color:green'>N</div></div><div id='parent'><div id='nested' style='display:flex;width:14px;height:8px;align-items:flex-start'><div id='nested-first' style='width:6px;height:4px;flex-shrink:0;background-color:red'>O</div><div id='nested-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>P</div></div></div><div id='fallback'><div id='fallback-first' style='width:6px;height:4px;flex-shrink:0;background-color:red'>Q</div><div id='fallback-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>R</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "flex-wrap" | "justify-content" | "align-items" | "align-self" | "align-content"
            ))
    }));

    let viewport = Viewport {
        width: 64,
        height: 256,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let box_for = |id: &str| {
        let node = document.resolve_target(&format!("id={id}")).unwrap();
        layout.box_for(node).unwrap()
    };

    // Unlayered rollback exposes the theme value, while two rollback layers
    // expose the lower concrete candidate.
    let justify = box_for("justify");
    assert_eq!(box_for("justify-first").x, justify.x + 6);
    assert_eq!(box_for("justify-second").x, justify.x + 12);
    let repeat = box_for("repeat");
    assert_eq!(box_for("repeat-first").x, repeat.x + 3);
    assert_eq!(box_for("repeat-second").x, repeat.x + 9);

    // Inline rollback reaches the named cross-axis value, and item rollback
    // reaches its named override inside the existing parent alignment path.
    let items = box_for("items");
    assert_eq!(box_for("items-first").y, items.y + 8);
    let self_container = box_for("self");
    assert_eq!(box_for("self-first").y, self_container.y + 8);
    assert_eq!(box_for("self-second").y, self_container.y);

    // Line-level and item-level rollback retain distinct geometry owners.
    let content = box_for("content");
    assert_eq!(box_for("content-first").x, content.x);
    assert_eq!(box_for("content-first").y, content.y + 8);
    assert_eq!(box_for("content-second").y, content.y + 12);
    assert_eq!(box_for("content-third").y, content.y + 16);
    let wrap = box_for("wrap");
    assert_eq!(box_for("wrap-first").x, wrap.x);
    assert_eq!(box_for("wrap-second").x, wrap.x);
    assert_eq!(box_for("wrap-third").x, wrap.x);
    assert_eq!(box_for("wrap-second").y, box_for("wrap-first").y + 4);
    assert_eq!(box_for("wrap-third").y, box_for("wrap-first").y + 8);

    // Non-inherited defaults are local: the parent justifies its child, but
    // the nested flex container uses its own flex-start fallback.
    let nested = box_for("nested");
    assert_eq!(box_for("nested-first").x, nested.x);
    assert_eq!(box_for("nested-second").x, nested.x + 6);
    let fallback = box_for("fallback");
    assert_eq!(box_for("fallback-first").x, fallback.x);
    assert_eq!(box_for("fallback-second").x, fallback.x + 6);
    assert_eq!(box_for("fallback-first").y, box_for("fallback-second").y);

    let justify_first = document.resolve_target("id=justify-first").unwrap();
    let justify_second = document.resolve_target("id=justify-second").unwrap();
    let source_order = layout
        .text_runs
        .iter()
        .filter_map(|run| {
            if run.node_id == justify_first {
                Some("first")
            } else if run.node_id == justify_second {
                Some("second")
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(source_order, vec!["first", "second"]);

    let justify_first_rect = box_for("justify-first");
    assert_eq!(
        layout.hit_test(
            i64::from(justify_first_rect.x + 1),
            i64::from(justify_first_rect.y + 1),
        ),
        Ok(Some(justify_first))
    );
    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == justify_first && *rect == justify_first_rect
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(justify_first_rect.x + 1, justify_first_rect.y + 1),
        Some([255, 0, 0, 255])
    );
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
fn native_flex_item_order_sizing_revert_layer_preserves_layout_and_artifacts() {
    let order_document = NativeDocument::parse(
        "<style>@layer base { #row { display:flex;width:24px;height:8px;gap:2px; } #first { order:0;width:8px;height:8px;background-color:red; } #second { order:0;width:8px;height:8px;background-color:blue; } } @layer theme { #first { order:2; } #second { order:-1; } } #first { order:revert-layer; }</style><div id='row'><button id='first'>A</button><button id='second'>B</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = order_document.resolve_target("id=first").unwrap();
    let second = order_document.resolve_target("id=second").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let order_layout = order_document.layout(viewport).unwrap();
    assert_eq!(order_layout.box_for(second).unwrap().x, 0);
    assert_eq!(order_layout.box_for(first).unwrap().x, 10);
    assert_eq!(order_layout.hit_test(1, 1).unwrap(), Some(second));
    assert_eq!(order_layout.hit_test(10, 1).unwrap(), Some(first));
    assert_eq!(
        order_document
            .semantic_nodes()
            .into_iter()
            .map(|node| node.node_id)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    let order_list = order_document.display_list(viewport).unwrap();
    assert_eq!(
        order_list
            .commands
            .iter()
            .filter_map(|command| match command {
                NativeDisplayCommand::FillRect { node_id, .. } => Some(*node_id),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![second, first]
    );
    let order_surface = order_list.rasterize().unwrap();
    assert_eq!(order_surface.pixel(1, 1), Some([0, 0, 255, 255]));
    assert_eq!(order_surface.pixel(11, 1), Some([255, 0, 0, 255]));

    let grow_document = NativeDocument::parse(
        "<style>@layer base { #row { display:flex;width:20px;height:8px;gap:2px; } #first { width:6px;height:8px;flex-grow:0;background-color:red; } #second { width:6px;height:8px;flex-grow:0;background-color:blue; } } @layer theme { #first { flex-grow:1; } } #first { flex-grow:revert-layer; }</style><div id='row'><div id='first'>A</div><div id='second'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let grow_first = grow_document.resolve_target("id=first").unwrap();
    let grow_second = grow_document.resolve_target("id=second").unwrap();
    let grow_layout = grow_document.layout(viewport).unwrap();
    assert_eq!(grow_layout.box_for(grow_first).unwrap().width, 12);
    assert_eq!(grow_layout.box_for(grow_second).unwrap().x, 14);
    assert_eq!(grow_layout.box_for(grow_second).unwrap().width, 6);

    let shrink_document = NativeDocument::parse(
        "<style>@layer base { #row { display:flex;width:12px;height:8px;gap:2px; } #first { width:8px;height:8px;flex-shrink:1;background-color:red; } #second { width:8px;height:8px;flex-shrink:1;background-color:blue; } } @layer theme { #first { flex-shrink:0; } } #first { flex-shrink:revert-layer; }</style><div id='row'><div id='first'>A</div><div id='second'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let shrink_first = shrink_document.resolve_target("id=first").unwrap();
    let shrink_second = shrink_document.resolve_target("id=second").unwrap();
    let shrink_layout = shrink_document.layout(viewport).unwrap();
    assert_eq!(shrink_layout.box_for(shrink_first).unwrap().width, 8);
    assert_eq!(shrink_layout.box_for(shrink_second).unwrap().x, 10);
    assert_eq!(shrink_layout.box_for(shrink_second).unwrap().width, 2);

    let basis_document = NativeDocument::parse(
        "<style>@layer base { #row { display:flex;width:20px;height:8px;gap:2px; } #first { width:4px;height:8px;flex-basis:4px;background-color:red; } #second { width:4px;height:8px;background-color:blue; } } @layer theme { #first { flex-basis:10px; } } #first { flex-basis:revert-layer; }</style><div id='row'><div id='first'>A</div><div id='second'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let basis_first = basis_document.resolve_target("id=first").unwrap();
    let basis_second = basis_document.resolve_target("id=second").unwrap();
    let basis_layout = basis_document.layout(viewport).unwrap();
    assert_eq!(basis_layout.box_for(basis_first).unwrap().width, 10);
    assert_eq!(basis_layout.box_for(basis_second).unwrap().x, 12);
    assert_eq!(basis_layout.box_for(basis_second).unwrap().width, 4);

    for document in [
        &order_document,
        &grow_document,
        &shrink_document,
        &basis_document,
    ] {
        assert!(!document.diagnostics().iter().any(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && matches!(
                    diagnostic.detail.as_str(),
                    "order" | "flex-grow" | "flex-shrink" | "flex-basis"
                )
        }));
    }
}
#[test]
fn native_flex_item_order_css_wide_resets_reach_visual_order_and_raster() {
    let document = NativeDocument::parse(
        "<style>.row { display:flex; width:32px; height:8px; gap:2px; align-items:flex-start; } #reset { order:-2; background-color:red; } #reset { order:INITIAL; } #unset { order:-3; background-color:green; } #unset { order:UNSET; } #revert { order:-4; background-color:blue; } #revert { order:ReVeRt; } #invalid { order:-5; background-color:black; } #invalid { order:inherit 1; } #important { order:-6 !important; background-color:#102030; } #important { order:initial; } .item { width:4px; height:4px; flex-shrink:0; }</style><div id='row' class='row'><button id='reset' class='item'>R</button><button id='unset' class='item'>U</button><button id='revert' class='item'>V</button><button id='invalid' class='item'>I</button><button id='important' class='item'>P</button><button id='default' class='item' style='background-color:#804000'>D</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let row = document.resolve_target("id=row").unwrap();
    let reset = document.resolve_target("id=reset").unwrap();
    let unset = document.resolve_target("id=unset").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let important = document.resolve_target("id=important").unwrap();
    let default_item = document.resolve_target("id=default").unwrap();

    let layout = document.layout(viewport).unwrap();
    let rect = |node_id| layout.box_for(node_id).expect("layout rectangle");
    assert_eq!(rect(row).width, 32);
    assert_eq!(rect(important).x, 0);
    assert_eq!(rect(invalid).x, 6);
    assert_eq!(rect(reset).x, 12);
    assert_eq!(rect(unset).x, 18);
    assert_eq!(rect(revert).x, 24);
    assert_eq!(rect(default_item).x, 30);

    let semantic_nodes = document.semantic_nodes();
    assert_eq!(
        semantic_nodes
            .iter()
            .map(|node| node.node_id)
            .collect::<Vec<_>>(),
        vec![reset, unset, revert, invalid, important, default_item]
    );

    let list = document.display_list(viewport).unwrap();
    let painted_ids = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::FillRect { node_id, .. } => Some(*node_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        painted_ids,
        vec![important, invalid, reset, unset, revert, default_item]
    );

    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(important));
    assert_eq!(layout.hit_test(13, 1).unwrap(), Some(reset));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([16, 32, 48, 255]));
    assert_eq!(surface.pixel(13, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(19, 1), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(25, 1), Some([0, 0, 255, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "order"
    }));
}
#[test]
fn native_flex_shorthand_revert_layer_preserves_components_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #row { display:flex;width:24px;height:8px;gap:2px; } #first { flex:0 1 4px; width:4px;height:8px;background-color:red; } #second { flex:2 3 5px; width:5px;height:8px;background-color:blue; } #inline { flex:2 3 4px; } #repeated { flex:2 3 7px; } #mixed { flex:2 3 6px; } } @layer theme { #first { flex:1 0 10px; } #second { flex:0 0 8px; } #inline { flex:5 0 9px; } #repeated { flex:4 2 9px; } #mixed { flex:revert-layer; flex-grow:5; } } @layer top { #second { flex:revert-layer; } #repeated { flex:revert-layer; } } #first { flex:revert-layer; } #second { flex:revert-layer; } #inline { flex:revert-layer; } #repeated { flex:revert-layer; } #mixed { flex:revert-layer; } #fallback { flex:revert-layer; } #same-before { flex:2 3 6px; flex-grow:4; } #same-after { flex-grow:4; flex:2 3 6px; } #invalid { flex:2 3 6px; flex:1 2 3%; }</style><div id='row'><div id='first'>A</div><div id='second'>B</div></div><div id='inline' style='flex:REVERT-LAYER'>Inline</div><div id='repeated'>Repeated</div><div id='mixed'>Mixed</div><div id='fallback'>Fallback</div><div id='same-before'>Before</div><div id='same-after'>After</div><div id='invalid'>Invalid</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();

    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "flex"
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "revert-layer"
    }));

    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let row = document.resolve_target("id=row").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let first_rect = layout.box_for(first).unwrap();
    let second_rect = layout.box_for(second).unwrap();
    assert_eq!(first_rect.width, 14);
    assert_eq!(second_rect.x, 16);
    assert_eq!(second_rect.width, 8);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(first));
    assert_eq!(layout.hit_test(17, 1).unwrap(), Some(second));
    assert_eq!(layout.box_for(row).unwrap().width, 24);

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == first && *rect == first_rect
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(17, 1), Some([0, 0, 255, 255]));
}
#[test]
fn native_flex_direction_inherit_reaches_nested_layout_and_artifacts() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#parent{flex-direction:column-reverse;}",
            ".row{display:flex;width:24px;height:20px;gap:2px;align-items:flex-start;}",
            "#inherited{flex-direction:InHeRiT;}",
            "#invalid{flex-direction:row;flex-direction:inherit column;}",
            "#flow{flex-flow:row wrap;flex-direction:inherit;}",
            ".item{width:6px;height:4px;flex-shrink:0;}",
            "</style>",
            "<div id='parent'>",
            "<div id='inherited' class='row'><button id='inherited-first' class='item' style='background-color:red'><span id='nested'>A</span></button><button id='inherited-second' class='item'>B</button></div>",
            "<div id='omitted' class='row'><button id='omitted-first' class='item'>C</button><button id='omitted-second' class='item'>D</button></div>",
            "<div id='invalid' class='row'><button id='invalid-first' class='item'>E</button><button id='invalid-second' class='item'>F</button></div>",
            "<div id='flow' class='row'><button id='flow-first' class='item'>G</button><button id='flow-second' class='item'>H</button></div>",
            "</div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 96,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let inherited = target("inherited");
    let inherited_first = target("inherited-first");
    let inherited_second = target("inherited-second");
    let nested = target("nested");
    let omitted = target("omitted");
    let omitted_first = target("omitted-first");
    let omitted_second = target("omitted-second");
    let invalid = target("invalid");
    let invalid_first = target("invalid-first");
    let flow = target("flow");
    let flow_first = target("flow-first");

    let layout = document.layout(viewport).unwrap();
    let inherited_box = layout.box_for(inherited).unwrap();
    let inherited_first_box = layout.box_for(inherited_first).unwrap();
    let inherited_second_box = layout.box_for(inherited_second).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let omitted_first_box = layout.box_for(omitted_first).unwrap();
    let omitted_second_box = layout.box_for(omitted_second).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    let invalid_first_box = layout.box_for(invalid_first).unwrap();
    let flow_box = layout.box_for(flow).unwrap();
    let flow_first_box = layout.box_for(flow_first).unwrap();
    assert_eq!(inherited_first_box.x, inherited_box.x);
    assert!(inherited_first_box.y > inherited_second_box.y);
    assert!(inherited_first_box.y > inherited_box.y);
    assert_eq!(omitted_first_box.y, omitted_box.y);
    assert_eq!(omitted_second_box.y, omitted_box.y);
    assert!(omitted_second_box.x > omitted_first_box.x);
    assert_eq!(invalid_first_box.x, invalid_box.x);
    assert_eq!(invalid_first_box.y, invalid_box.y);
    assert_eq!(flow_first_box.x, flow_box.x);
    assert!(flow_first_box.y > flow_box.y);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == inherited_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == inherited_second)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == inherited_first
                    && rect.x == inherited_first_box.x
                    && rect.y == inherited_first_box.y
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(
                i64::from(inherited_first_box.x + 1),
                i64::from(inherited_first_box.y + 1),
            )
            .unwrap(),
        Some(nested)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(inherited_first_box.x + 1, inherited_first_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-direction"
    }));
}
#[test]
fn native_flex_flow_inherit_reaches_both_flex_components_and_artifacts() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#parent{flex-direction:column;flex-wrap:wrap-reverse;}",
            ".column{display:flex;width:20px;height:20px;row-gap:2px;column-gap:1px;align-items:flex-start;align-content:flex-start;}",
            "#inherited{flex-flow:InHeRiT;}",
            "#invalid{flex-flow:column wrap;flex-flow:inherit column;}",
            ".item{width:5px;height:6px;flex-shrink:0;}",
            "</style>",
            "<div id='parent'>",
            "<div id='inherited' class='column'><button id='inherited-first' class='item' style='background-color:red'><span id='nested'>A</span></button><button id='inherited-second' class='item'>B</button><button id='inherited-third' class='item'>C</button></div>",
            "<div id='omitted' class='column'><button id='omitted-first' class='item'>D</button><button id='omitted-second' class='item'>E</button><button id='omitted-third' class='item'>F</button></div>",
            "<div id='invalid' class='column'><button id='invalid-first' class='item'>G</button><button id='invalid-second' class='item'>H</button><button id='invalid-third' class='item'>I</button></div>",
            "</div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 72,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let inherited = target("inherited");
    let inherited_first = target("inherited-first");
    let inherited_second = target("inherited-second");
    let inherited_third = target("inherited-third");
    let nested = target("nested");
    let omitted = target("omitted");
    let omitted_first = target("omitted-first");
    let omitted_second = target("omitted-second");
    let omitted_third = target("omitted-third");
    let invalid = target("invalid");
    let invalid_first = target("invalid-first");
    let invalid_second = target("invalid-second");
    let invalid_third = target("invalid-third");

    let layout = document.layout(viewport).unwrap();
    let inherited_box = layout.box_for(inherited).unwrap();
    let inherited_first_box = layout.box_for(inherited_first).unwrap();
    let inherited_second_box = layout.box_for(inherited_second).unwrap();
    let inherited_third_box = layout.box_for(inherited_third).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let omitted_first_box = layout.box_for(omitted_first).unwrap();
    let omitted_second_box = layout.box_for(omitted_second).unwrap();
    let omitted_third_box = layout.box_for(omitted_third).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    let invalid_first_box = layout.box_for(invalid_first).unwrap();
    let invalid_second_box = layout.box_for(invalid_second).unwrap();
    let invalid_third_box = layout.box_for(invalid_third).unwrap();

    assert_eq!(inherited_box.width, 20);
    assert_eq!(inherited_box.height, 20);
    assert_eq!(inherited_first_box.x, inherited_box.x + 15);
    assert_eq!(inherited_first_box.y, inherited_box.y);
    assert_eq!(inherited_second_box.x, inherited_box.x + 15);
    assert_eq!(inherited_second_box.y, inherited_box.y + 8);
    assert_eq!(inherited_third_box.x, inherited_box.x + 9);
    assert_eq!(inherited_third_box.y, inherited_box.y);
    assert_eq!(omitted_first_box.y, omitted_box.y);
    assert_eq!(omitted_second_box.y, omitted_box.y);
    assert_eq!(omitted_third_box.y, omitted_box.y);
    assert_eq!(omitted_second_box.x, omitted_first_box.x + 6);
    assert_eq!(omitted_third_box.x, omitted_first_box.x + 12);
    assert_eq!(invalid_first_box.x, invalid_box.x);
    assert_eq!(invalid_first_box.y, invalid_box.y);
    assert_eq!(invalid_second_box.x, invalid_box.x);
    assert_eq!(invalid_second_box.y, invalid_box.y + 8);
    assert_eq!(invalid_third_box.x, invalid_box.x + 6);
    assert_eq!(invalid_third_box.y, invalid_box.y);

    assert_eq!(
        document.node(inherited).unwrap().children(),
        &[inherited_first, inherited_second, inherited_third]
    );
    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == inherited_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == inherited_second)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == inherited_first
                    && rect.x == inherited_first_box.x
                    && rect.y == inherited_first_box.y
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(
                i64::from(inherited_first_box.x + 1),
                i64::from(inherited_first_box.y + 1),
            )
            .unwrap(),
        Some(nested)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(inherited_first_box.x + 1, inherited_first_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-flow"
    }));
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

    let engine_viewport = Viewport {
        width: viewport.width + 16,
        height: viewport.height + 16,
        ..viewport
    };
    let config = NativeEngineConfig::default()
        .with_viewport(engine_viewport)
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
    assert_eq!(engine.scroll_offset(), NativePoint { x: 3, y: 0 });
    assert_eq!(engine.hit_test(31, 15).unwrap(), Some(nested));

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
fn native_box_model_revert_layer_reaches_flex_auto_margin_and_source_order() {
    let document = NativeDocument::parse(
        "<style>@layer base { #flex { display:flex; width:40px; height:12px; } #first { width:8px; height:6px; background-color:blue; } #auto { width:8px; height:6px; margin:1px 1px 2px auto; background-color:red; } } @layer theme { #auto { margin-left:revert-layer; margin-top:4px; } } @layer top { #auto { margin-left:revert-layer; margin-top:revert-layer; } } #auto { margin-left:revert-layer; margin-top:revert-layer; }</style><div id='flex'><button id='first'>First</button><button id='auto'>Auto</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let first = document.resolve_target("id=first").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(auto),
        Some(NativeRect {
            x: 31,
            y: 4,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(layout.hit_test(32, 5).unwrap(), Some(auto));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(first));
    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|semantic_node| semantic_node.node_id == first)
            < semantic_nodes
                .iter()
                .position(|semantic_node| semantic_node.node_id == auto)
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == auto
                    && *rect == (NativeRect { x: 31, y: 4, width: 8, height: 6 })
        )
    }));
    assert_eq!(
        list.rasterize().unwrap().pixel(32, 5),
        Some([255, 0, 0, 255])
    );
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "margin" | "margin-left" | "margin-top"
            )
    }));
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
fn native_flex_inherit_reaches_sizing_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#outer{display:flex;width:30px;height:8px;gap:2px;align-items:flex-start;flex:2 3 12px;}#inherited{flex:InHeRiT;height:8px;background-color:red;}#omitted{width:4px;height:8px;background-color:green;}#invalid{width:4px;height:8px;flex:1 1 4px;flex:inherit 1 auto;background-color:blue;}</style><div id='outer'><button id='inherited'><span id='nested' style='display:block;height:4px'>A</span></button><button id='omitted'>B</button><button id='invalid'>C</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let outer = document.resolve_target("id=outer").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let viewport = Viewport {
        width: 36,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let outer_box = layout.box_for(outer).unwrap();
    let inherited_box = layout.box_for(inherited).unwrap();
    let nested_box = layout.box_for(nested).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    assert_eq!(outer_box.width, 30);
    assert!(inherited_box.width > 12);
    assert_eq!(nested_box.width, inherited_box.width);
    assert_eq!(omitted_box.width, 4);
    assert!(invalid_box.width > 4);
    assert_eq!(inherited_box.x, outer_box.x);
    assert_eq!(omitted_box.x, inherited_box.x + inherited_box.width + 2);
    assert_eq!(invalid_box.x, omitted_box.x + omitted_box.width + 2);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == inherited)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == omitted)
                .unwrap()
    );
    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == inherited
                    && rect.width == inherited_box.width
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(
                i64::from(inherited_box.x + 1),
                i64::from(inherited_box.y + 1),
            )
            .unwrap(),
        Some(nested)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(inherited_box.x + 1, inherited_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "flex"
    }));
}
#[test]
fn native_flex_grow_inherit_reaches_sizing_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#outer{display:flex;width:24px;height:8px;gap:2px;align-items:flex-start;flex-grow:2;}#direct{flex-grow:InHeRiT;width:4px;height:8px;background-color:red;}#omitted{width:4px;height:8px;background-color:green;}#invalid{width:4px;height:8px;flex-grow:3;flex-grow:inherit 1;background-color:blue;}</style><div id='outer'><button id='direct'><span id='nested' style='display:block;height:4px'>A</span></button><button id='omitted'>B</button><button id='invalid'>C</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let outer = document.resolve_target("id=outer").unwrap();
    let direct = document.resolve_target("id=direct").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let viewport = Viewport {
        width: 30,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let outer_box = layout.box_for(outer).unwrap();
    let direct_box = layout.box_for(direct).unwrap();
    let nested_box = layout.box_for(nested).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    assert_eq!(outer_box.width, 24);
    assert!(direct_box.width > 4);
    assert_eq!(nested_box.width, direct_box.width);
    assert_eq!(omitted_box.width, 4);
    assert!(invalid_box.width > 4);
    assert_eq!(direct_box.x, outer_box.x);
    assert_eq!(omitted_box.x, direct_box.x + direct_box.width + 2);
    assert_eq!(invalid_box.x, omitted_box.x + omitted_box.width + 2);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == direct)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == omitted)
                .unwrap()
    );
    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == direct
                    && rect.width == direct_box.width
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(i64::from(direct_box.x + 1), i64::from(direct_box.y + 1),)
            .unwrap(),
        Some(nested)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(direct_box.x + 1, direct_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-grow"
    }));
}
#[test]
fn native_flex_shrink_inherit_reaches_sizing_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#outer{display:flex;width:20px;height:8px;gap:2px;align-items:flex-start;flex-shrink:2;}#direct{flex-shrink:InHeRiT;width:12px;height:8px;background-color:red;}#omitted{width:12px;height:8px;background-color:green;}#invalid{display:none;flex-shrink:3;flex-shrink:inherit 1;}</style><div id='outer'><button id='direct'><span id='nested' style='display:block;height:4px'>A</span></button><button id='omitted'>B</button><button id='invalid'>C</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let outer = document.resolve_target("id=outer").unwrap();
    let direct = document.resolve_target("id=direct").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let outer_box = layout.box_for(outer).unwrap();
    let direct_box = layout.box_for(direct).unwrap();
    let nested_box = layout.box_for(nested).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    assert_eq!(outer_box.width, 20);
    assert_eq!(direct_box.width, 8);
    assert_eq!(nested_box.width, direct_box.width);
    assert_eq!(omitted_box.width, 10);
    assert!(direct_box.width < omitted_box.width);
    assert_eq!(direct_box.x, outer_box.x);
    assert_eq!(omitted_box.x, direct_box.x + direct_box.width + 2);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == direct)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == omitted)
                .unwrap()
    );
    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == direct
                    && rect.width == direct_box.width
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(i64::from(direct_box.x + 1), i64::from(direct_box.y + 1),)
            .unwrap(),
        Some(nested)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(direct_box.x + 1, direct_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-shrink"
    }));
}
#[test]
fn native_flex_basis_inherit_reaches_sizing_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>#outer{display:flex;width:24px;height:8px;gap:2px;align-items:flex-start;flex-basis:12px;}#direct{flex-basis:InHeRiT;width:4px;height:8px;flex-grow:0;flex-shrink:0;background-color:red;}#omitted{width:4px;height:8px;flex-grow:0;flex-shrink:0;background-color:green;}#invalid{display:none;flex-basis:8px;flex-basis:inherit 1px;}</style><div id='outer'><button id='direct'><span id='nested' style='display:block;height:4px'>A</span></button><button id='omitted'>B</button><button id='invalid'>C</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let outer = document.resolve_target("id=outer").unwrap();
    let direct = document.resolve_target("id=direct").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let viewport = Viewport {
        width: 30,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let outer_box = layout.box_for(outer).unwrap();
    let direct_box = layout.box_for(direct).unwrap();
    let nested_box = layout.box_for(nested).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    assert_eq!(outer_box.width, 24);
    assert_eq!(direct_box.width, 12);
    assert_eq!(nested_box.width, direct_box.width);
    assert_eq!(omitted_box.width, 4);
    assert_eq!(direct_box.x, outer_box.x);
    assert_eq!(omitted_box.x, direct_box.x + direct_box.width + 2);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == direct)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == omitted)
                .unwrap()
    );
    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == direct
                    && rect.width == direct_box.width
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(i64::from(direct_box.x + 1), i64::from(direct_box.y + 1),)
            .unwrap(),
        Some(nested)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(direct_box.x + 1, direct_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-basis"
    }));
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
fn native_css_absolute_position_is_filtered_from_flex_and_grid_items() {
    let document = NativeDocument::parse(
        "<div id='flex' style='display:flex;width:30px;height:10px'><div id='flex-overlay' style='position:absolute;left:2px;top:1px;width:4px;height:4px'></div><div id='flex-flow' style='width:6px;height:4px'></div></div><div id='grid' style='display:grid;width:30px;grid-template-columns:10px 10px'><div id='grid-overlay' style='position:absolute;left:12px;top:1px;width:4px;height:4px'></div><div id='grid-flow' style='height:4px'></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let flex_overlay = document.resolve_target("id=flex-overlay").unwrap();
    let flex_flow = document.resolve_target("id=flex-flow").unwrap();
    let grid_overlay = document.resolve_target("id=grid-overlay").unwrap();
    let grid_flow = document.resolve_target("id=grid-flow").unwrap();
    let layout = document
        .layout(Viewport {
            width: 40,
            height: 40,
            device_scale_factor_milli: 1000,
        })
        .unwrap();

    assert_eq!(layout.box_for(flex_overlay).unwrap().x, 2);
    assert_eq!(layout.box_for(flex_overlay).unwrap().y, 1);
    assert_eq!(layout.box_for(flex_flow).unwrap().x, 0);
    assert_eq!(layout.box_for(flex_flow).unwrap().y, 0);
    assert_eq!(layout.box_for(grid_overlay).unwrap().x, 12);
    assert_eq!(layout.box_for(grid_overlay).unwrap().y, 11);
    assert_eq!(layout.box_for(grid_flow).unwrap().x, 0);
    assert_eq!(layout.box_for(grid_flow).unwrap().y, 10);
}
#[test]
fn native_flex_sizing_css_wide_resets_reach_layout_and_raster() {
    let document = NativeDocument::parse(
        "<style>.row { display:flex; width:32px; height:8px; gap:2px; align-items:flex-start; } .item { width:4px; height:4px; flex-shrink:0; } #reset { flex:4 5 12px; background-color:red; } #reset { flex:InItIaL; } #longhands { flex-grow:4; flex-shrink:5; flex-basis:12px; background-color:green; } #longhands { flex-grow:UnSeT; } #longhands { flex-shrink:ReVeRt; flex-basis:INITIAL; } #invalid { flex:2 3 8px; background-color:blue; } #invalid { flex:inherit 1 auto; } #important { flex:5 6 10px !important; background-color:black; } #important { flex:initial; }</style><div id='row' class='row'><div id='reset' class='item'>A</div><div id='longhands' class='item'>B</div><div id='invalid' class='item'>C</div><div id='important' class='item'>D</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let row = document.resolve_target("id=row").unwrap();
    let reset = document.resolve_target("id=reset").unwrap();
    let longhands = document.resolve_target("id=longhands").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let important = document.resolve_target("id=important").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(row).unwrap().width, 32);
    let rect = |node_id| layout.box_for(node_id).expect("layout rectangle");
    let reset_rect = rect(reset);
    let longhands_rect = rect(longhands);
    let invalid_rect = rect(invalid);
    let important_rect = rect(important);
    assert_eq!(
        reset_rect,
        NativeRect {
            x: 0,
            y: 0,
            width: 4,
            height: 4
        }
    );
    assert_eq!(
        longhands_rect,
        NativeRect {
            x: 6,
            y: 0,
            width: 4,
            height: 4
        }
    );
    assert_eq!(
        invalid_rect,
        NativeRect {
            x: 12,
            y: 0,
            width: 8,
            height: 4
        }
    );
    assert_eq!(
        important_rect,
        NativeRect {
            x: 22,
            y: 0,
            width: 10,
            height: 4
        }
    );

    let list = document.display_list(viewport).unwrap();
    for (node_id, rect, color) in [
        (reset, reset_rect, NativeColor::RED),
        (
            longhands,
            longhands_rect,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: u8::MAX,
            },
        ),
        (
            invalid,
            invalid_rect,
            NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            },
        ),
        (important, important_rect, NativeColor::BLACK),
    ] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id: command_node, rect: command_rect, color: command_color, .. }
                    if *command_node == node_id && *command_rect == rect && *command_color == color
            )
        }));
    }
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(reset));
    assert_eq!(layout.hit_test(23, 1).unwrap(), Some(important));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(23, 1), Some([0, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "flex"
    }));
}
#[test]
fn native_flex_flow_css_wide_resets_reach_layout_and_raster() {
    let document = NativeDocument::parse(
        "<style>.flex { display:flex; width:8px; height:8px; gap:2px; align-items:flex-start; } .item { width:4px; height:4px; flex-shrink:0; } #direction { flex-direction:column; } #direction { flex-direction:INITIAL; } #wrap { flex-wrap:wrap; } #wrap { flex-wrap:UNSET; } #flow { flex-flow:column wrap; } #flow { flex-flow:ReVeRt; } #invalid { flex-flow:row-reverse wrap; } #invalid { flex-flow:inherit row; } #important { flex-flow:column wrap !important; } #important { flex-flow:initial; }</style><div id='direction' class='flex'><button id='direction-first' class='item' style='background-color:red'>A</button><button id='direction-second' class='item' style='background-color:blue'>B</button></div><div id='wrap' class='flex'><div id='wrap-first' class='item' style='background-color:red'>A</div><div id='wrap-second' class='item' style='background-color:blue'>B</div></div><div id='flow' class='flex'><div id='flow-first' class='item' style='background-color:red'>A</div><div id='flow-second' class='item' style='background-color:blue'>B</div></div><div id='invalid' class='flex'><div id='invalid-first' class='item' style='background-color:green'>A</div><div id='invalid-second' class='item' style='background-color:black'>B</div></div><div id='important' class='flex'><div id='important-first' class='item' style='background-color:red'>A</div><div id='important-second' class='item' style='background-color:blue'>B</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let direction = document.resolve_target("id=direction").unwrap();
    let direction_first = document.resolve_target("id=direction-first").unwrap();
    let direction_second = document.resolve_target("id=direction-second").unwrap();
    let wrap = document.resolve_target("id=wrap").unwrap();
    let wrap_first = document.resolve_target("id=wrap-first").unwrap();
    let wrap_second = document.resolve_target("id=wrap-second").unwrap();
    let flow = document.resolve_target("id=flow").unwrap();
    let flow_first = document.resolve_target("id=flow-first").unwrap();
    let flow_second = document.resolve_target("id=flow-second").unwrap();
    let invalid_first = document.resolve_target("id=invalid-first").unwrap();
    let invalid_second = document.resolve_target("id=invalid-second").unwrap();
    let important = document.resolve_target("id=important").unwrap();
    let important_first = document.resolve_target("id=important-first").unwrap();
    let important_second = document.resolve_target("id=important-second").unwrap();

    let layout = document.layout(viewport).unwrap();
    let rect = |node_id| layout.box_for(node_id).expect("layout rectangle");
    let direction_rect = rect(direction);
    let direction_first_rect = rect(direction_first);
    let direction_second_rect = rect(direction_second);
    assert_eq!(direction_rect.width, 8);
    assert_eq!(direction_rect.height, 8);
    assert_eq!(direction_first_rect.x, direction_rect.x);
    assert_eq!(direction_second_rect.x, direction_rect.x + 6);
    assert_eq!(direction_second_rect.y, direction_first_rect.y);

    let wrap_rect = rect(wrap);
    let wrap_first_rect = rect(wrap_first);
    let wrap_second_rect = rect(wrap_second);
    assert_eq!(wrap_second_rect.x, wrap_rect.x + 6);
    assert_eq!(wrap_second_rect.y, wrap_first_rect.y);

    let flow_rect = rect(flow);
    let flow_first_rect = rect(flow_first);
    let flow_second_rect = rect(flow_second);
    assert_eq!(flow_second_rect.x, flow_rect.x + 6);
    assert_eq!(flow_second_rect.y, flow_first_rect.y);

    let invalid_first_rect = rect(invalid_first);
    let invalid_second_rect = rect(invalid_second);
    assert!(invalid_second_rect.y > invalid_first_rect.y);

    let important_rect = rect(important);
    let important_first_rect = rect(important_first);
    let important_second_rect = rect(important_second);
    assert_eq!(important_second_rect.x, important_rect.x + 6);
    assert_eq!(important_second_rect.y, important_first_rect.y);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|semantic_node| semantic_node.node_id == direction_first)
            < semantic_nodes
                .iter()
                .position(|semantic_node| semantic_node.node_id == direction_second)
    );

    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    for (node_id, node_rect, color) in [
        (direction_first, direction_first_rect, [255, 0, 0, 255]),
        (direction_second, direction_second_rect, [0, 0, 255, 255]),
        (invalid_first, invalid_first_rect, [0, 128, 0, 255]),
        (invalid_second, invalid_second_rect, [0, 0, 0, 255]),
    ] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id: command_node, rect: command_rect, .. }
                    if *command_node == node_id && *command_rect == node_rect
            )
        }));
        assert_eq!(surface.pixel(node_rect.x + 1, node_rect.y + 1), Some(color));
    }
    assert_eq!(
        layout
            .hit_test(
                i64::from(direction_first_rect.x + 1),
                i64::from(direction_first_rect.y + 1),
            )
            .unwrap(),
        Some(direction_first)
    );
    assert_eq!(
        layout
            .hit_test(
                i64::from(important_second_rect.x + 1),
                i64::from(important_second_rect.y + 1),
            )
            .unwrap(),
        Some(important_second)
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "flex-flow"
    }));
}

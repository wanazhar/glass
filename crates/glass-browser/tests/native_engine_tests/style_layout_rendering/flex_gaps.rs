use super::*;

#[test]
fn native_flex_gap_important_priority_reaches_layout_and_artifact_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
        @layer base {
          #row { flex-flow:row nowrap !important; justify-content:flex-start !important; align-items:center !important; gap:4px !important; }
          #row-second { order:-1 !important; }
          #column { flex-flow:column nowrap !important; justify-content:flex-start !important; align-items:center !important; gap:3px !important; }
          #column-second { align-self:flex-end !important; }
          #wrap { flex-flow:row wrap !important; align-items:flex-start !important; align-content:flex-start !important; gap:2px !important; }
          #overflow { flex-flow:row nowrap !important; gap:2px !important; }
          #overflow-first { flex-shrink:0 !important; }
          #overflow-second { flex-shrink:0 !important; }
        }
        @layer theme {
          #row { flex-flow:column wrap !important; justify-content:flex-end !important; align-items:flex-end !important; gap:1px !important; }
          #row-second { order:1 !important; }
          #column { flex-flow:row wrap !important; justify-content:flex-end !important; align-items:flex-end !important; gap:1px !important; }
          #column-second { align-self:flex-start !important; }
          #wrap { flex-flow:row nowrap !important; align-items:flex-end !important; align-content:flex-end !important; gap:1px !important; }
          #overflow { flex-flow:column nowrap !important; gap:1px !important; }
          #overflow-first { flex-shrink:1 !important; }
          #overflow-second { flex-shrink:1 !important; }
        }
        #row { flex-flow:column wrap; justify-content:flex-end; align-items:flex-end; gap:1px; }
        #row-second { order:1; }
        #column { flex-flow:row wrap; justify-content:flex-end; align-items:flex-end; gap:1px; }
        #column-second { align-self:flex-start; }
        #wrap { flex-flow:row nowrap; align-items:flex-end; align-content:flex-end; gap:1px; }
        #overflow { flex-flow:column nowrap; gap:1px; }
        #overflow-first { flex-shrink:1; }
        #overflow-second { flex-shrink:1; }
        </style>
        <div id='row' style='display:flex;width:20px;height:12px'>
          <div id='row-first' style='width:6px;height:4px;background-color:red'></div>
          <div id='row-second' style='width:6px;height:4px;background-color:blue'></div>
        </div>
        <div id='column' style='display:flex;width:12px;height:20px'>
          <div id='column-first' style='width:4px;height:5px;background-color:green'></div>
          <div id='column-second' style='width:4px;height:5px;background-color:blue'></div>
        </div>
        <div id='wrap' style='display:flex;width:14px;height:14px'>
          <div id='wrap-first' style='width:6px;height:4px;background-color:red'></div>
          <div id='wrap-second' style='width:6px;height:4px;background-color:blue'></div>
          <div id='wrap-third' style='width:6px;height:4px;background-color:green'></div>
        </div>
        <div id='overflow' style='display:flex;width:10px;height:8px'>
          <div id='overflow-first' style='width:8px;height:4px;background-color:red'></div>
          <div id='overflow-second' style='width:8px;height:4px;background-color:blue'></div>
        </div>"#,
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

    let viewport = Viewport {
        width: 32,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let row = document.resolve_target("id=row").unwrap();
    let row_first = document.resolve_target("id=row-first").unwrap();
    let row_second = document.resolve_target("id=row-second").unwrap();
    let column = document.resolve_target("id=column").unwrap();
    let column_first = document.resolve_target("id=column-first").unwrap();
    let column_second = document.resolve_target("id=column-second").unwrap();
    let wrap = document.resolve_target("id=wrap").unwrap();
    let wrap_first = document.resolve_target("id=wrap-first").unwrap();
    let wrap_second = document.resolve_target("id=wrap-second").unwrap();
    let wrap_third = document.resolve_target("id=wrap-third").unwrap();
    let layout = document.layout(viewport).unwrap();

    let row_rect = layout.box_for(row).unwrap();
    let row_first_rect = layout.box_for(row_first).unwrap();
    let row_second_rect = layout.box_for(row_second).unwrap();
    assert_eq!(row_rect.height, 12);
    assert_eq!(row_second_rect.x, row_rect.x);
    assert_eq!(row_first_rect.x, row_second_rect.right() + 4);
    assert_eq!(row_first_rect.y, row_rect.y + 4);
    assert_eq!(row_second_rect.y, row_rect.y + 4);
    assert_eq!(
        layout.hit_test(
            (row_second_rect.x + 1).into(),
            (row_second_rect.y + 1).into(),
        ),
        Ok(Some(row_second))
    );

    let column_rect = layout.box_for(column).unwrap();
    let column_first_rect = layout.box_for(column_first).unwrap();
    let column_second_rect = layout.box_for(column_second).unwrap();
    assert_eq!(column_rect.height, 20);
    assert_eq!(column_first_rect.x, column_rect.x + 4);
    assert_eq!(column_second_rect.x, column_rect.right() - 4);
    assert_eq!(column_second_rect.y, column_first_rect.bottom() + 3);

    let wrap_rect = layout.box_for(wrap).unwrap();
    let wrap_first_rect = layout.box_for(wrap_first).unwrap();
    let wrap_second_rect = layout.box_for(wrap_second).unwrap();
    let wrap_third_rect = layout.box_for(wrap_third).unwrap();
    assert_eq!(wrap_rect.height, 14);
    assert_eq!(wrap_first_rect.x, wrap_rect.x);
    assert_eq!(wrap_second_rect.x, wrap_first_rect.right() + 2);
    assert_eq!(wrap_third_rect.x, wrap_rect.x);
    assert_eq!(wrap_third_rect.y, wrap_first_rect.bottom() + 2);

    let list = document.display_list(viewport).unwrap();
    for (node_id, rect, color) in [
        (row_first, row_first_rect, NativeColor::RED),
        (
            row_second,
            row_second_rect,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
        ),
        (
            column_first,
            column_first_rect,
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
    assert_eq!(
        surface.pixel(row_second_rect.x + 1, row_second_rect.y + 1),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        surface.pixel(column_first_rect.x + 1, column_first_rect.y + 1),
        Some([0, 128, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (32, 64));

    let overflow_document = NativeDocument::parse(
        r#"<style>
        @layer base { #overflow { flex-flow:row nowrap !important; gap:2px !important; } #first { flex-shrink:0 !important; } #second { flex-shrink:0 !important; } }
        @layer theme { #overflow { flex-flow:column nowrap !important; gap:1px !important; } #first { flex-shrink:1 !important; } #second { flex-shrink:1 !important; } }
        #overflow { flex-flow:column nowrap; gap:1px; }
        </style><div id='overflow' style='display:flex;width:10px;height:8px'><div id='first' style='width:8px;height:4px'></div><div id='second' style='width:8px;height:4px'></div></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let first = overflow_document.resolve_target("id=first").unwrap();
    let second = overflow_document.resolve_target("id=second").unwrap();
    let overflow_layout = overflow_document
        .layout(Viewport {
            width: 10,
            height: 12,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert_eq!(overflow_layout.box_for(first).unwrap().x, 0);
    assert_eq!(overflow_layout.box_for(second).unwrap().x, 10);
    assert_eq!(overflow_layout.content_width, 18);
    assert_eq!(overflow_layout.max_scroll_offset().x, 8);
    assert_eq!(overflow_layout.hit_test(1, 1), Ok(Some(first)));
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

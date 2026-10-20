use super::*;

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
fn native_flex_wrap_inherit_reaches_wrapped_layout_and_artifacts() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#parent{flex-wrap:wrap-reverse;}",
            ".row{display:flex;width:20px;height:40px;gap:2px;align-items:flex-start;align-content:flex-start;}",
            "#inherited{flex-wrap:InHeRiT;}",
            "#invalid{flex-wrap:wrap;flex-wrap:inherit wrap;}",
            "#flow{flex-flow:row wrap;flex-wrap:inherit;}",
            ".item{width:8px;height:6px;flex-shrink:0;}",
            "</style>",
            "<div id='parent'>",
            "<div id='inherited' class='row'><button id='inherited-first' class='item' style='background-color:red'><span id='nested'>A</span></button><button id='inherited-second' class='item'>B</button><button id='inherited-third' class='item'>C</button></div>",
            "<div id='omitted' class='row'><button id='omitted-first' class='item'>D</button><button id='omitted-second' class='item'>E</button><button id='omitted-third' class='item'>F</button></div>",
            "<div id='invalid' class='row'><button id='invalid-first' class='item'>G</button><button id='invalid-second' class='item'>H</button><button id='invalid-third' class='item'>I</button></div>",
            "<div id='flow' class='row'><button id='flow-first' class='item'>J</button><button id='flow-second' class='item'>K</button><button id='flow-third' class='item'>L</button></div>",
            "</div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 180,
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
    let invalid_third = target("invalid-third");
    let flow = target("flow");
    let flow_first = target("flow-first");
    let flow_third = target("flow-third");

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
    let invalid_third_box = layout.box_for(invalid_third).unwrap();
    let flow_box = layout.box_for(flow).unwrap();
    let flow_first_box = layout.box_for(flow_first).unwrap();
    let flow_third_box = layout.box_for(flow_third).unwrap();

    assert_eq!(inherited_first_box.x, inherited_box.x);
    assert_eq!(inherited_second_box.x, inherited_box.x + 10);
    assert_eq!(inherited_first_box.y, inherited_box.y + 34);
    assert_eq!(inherited_second_box.y, inherited_box.y + 34);
    assert_eq!(inherited_third_box.y, inherited_box.y + 26);
    assert_eq!(omitted_first_box.y, omitted_box.y);
    assert_eq!(omitted_second_box.y, omitted_box.y);
    assert_eq!(omitted_third_box.y, omitted_box.y);
    assert!(omitted_third_box.x > omitted_second_box.x);
    assert_eq!(invalid_first_box.y, invalid_box.y);
    assert_eq!(invalid_third_box.y, invalid_box.y + 8);
    assert_eq!(flow_first_box.y, flow_box.y + 34);
    assert_eq!(flow_third_box.y, flow_box.y + 26);

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
            && diagnostic.detail == "flex-wrap"
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
    assert_eq!(engine.scroll_offset(), NativePoint { x: 17, y: 0 });
    assert_eq!(engine.hit_test(1, 10).unwrap(), Some(wide));
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
    assert_eq!(small_layout.max_scroll_offset(), NativePoint { x: 0, y: 8 });

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
        NativePoint { x: 0, y: 8 }
    );

    let html = source("flex-start");
    let third = NativeDocument::parse(&html, &NativeEngineLimits::default())
        .unwrap()
        .resolve_target("id=third")
        .unwrap();
    let engine_viewport = Viewport {
        width: 40,
        height: 40,
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
            x: 8,
            y: 6,
            width: 8,
            height: 14,
        })
    );
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

use super::*;

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
fn native_flex_justify_content_css_wide_resets_reach_layout_and_raster() {
    let document = NativeDocument::parse(
        "<style>.row { display:flex; width:32px; height:8px; gap:2px; } #initial { justify-content:center; } #initial { justify-content:INITIAL; } #unset { justify-content:flex-end; } #unset { justify-content:UNSET; } #revert { justify-content:space-between; } #revert { justify-content:ReVeRt; } #invalid { justify-content:center; } #invalid { justify-content:inherit; } #important { justify-content:flex-end !important; } #important { justify-content:initial; } #layered { justify-content:flex-start; } @layer theme { #layered { justify-content:flex-end; } } #layered { justify-content:revert-layer; } .item { width:4px; height:4px; flex-shrink:0; }</style><div id='initial' class='row'><button id='initial-first' class='item' style='background-color:red'>A</button><button id='initial-second' class='item' style='background-color:blue'>B</button></div><div id='unset' class='row'><button id='unset-first' class='item' style='background-color:red'>C</button><button id='unset-second' class='item' style='background-color:blue'>D</button></div><div id='revert' class='row'><button id='revert-first' class='item' style='background-color:red'>E</button><button id='revert-second' class='item' style='background-color:blue'>F</button></div><div id='invalid' class='row'><button id='invalid-first' class='item' style='background-color:red'>G</button><button id='invalid-second' class='item' style='background-color:blue'>H</button></div><div id='important' class='row'><button id='important-first' class='item' style='background-color:red'>I</button><button id='important-second' class='item' style='background-color:blue'>J</button></div><div id='layered' class='row'><button id='layered-first' class='item' style='background-color:red'>K</button><button id='layered-second' class='item' style='background-color:blue'>L</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 56,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let initial_first = target("initial-first");
    let initial_second = target("initial-second");
    let unset_first = target("unset-first");
    let unset_second = target("unset-second");
    let revert_first = target("revert-first");
    let revert_second = target("revert-second");
    let invalid_first = target("invalid-first");
    let invalid_second = target("invalid-second");
    let important_first = target("important-first");
    let important_second = target("important-second");
    let layered_first = target("layered-first");
    let layered_second = target("layered-second");

    let layout = document.layout(viewport).unwrap();
    let x_pair = |first, second| {
        (
            layout.box_for(first).unwrap().x,
            layout.box_for(second).unwrap().x,
        )
    };
    assert_eq!(x_pair(initial_first, initial_second), (0, 6));
    assert_eq!(x_pair(unset_first, unset_second), (0, 6));
    assert_eq!(x_pair(revert_first, revert_second), (0, 6));
    assert_eq!(x_pair(invalid_first, invalid_second), (0, 6));
    assert_eq!(x_pair(important_first, important_second), (22, 28));
    assert_eq!(x_pair(layered_first, layered_second), (22, 28));

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == initial_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == initial_second)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == initial_first && rect.x == 0 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(initial_first));
    assert_eq!(layout.hit_test(1, 25).unwrap(), Some(invalid_first));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 25), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    let unsupported = NativeDocument::parse(
        "<div style='justify-content:inherit center'>Unsupported</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(unsupported.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "justify-content"
    }));
}
#[test]
fn native_flex_justify_content_inherit_reaches_nested_layout_and_raster() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#parent{display:flex;flex-direction:column;align-items:flex-start;justify-content:center;width:40px;height:40px;}",
            "#child{display:flex;width:32px;height:8px;gap:2px;justify-content:inherit;flex-shrink:0;}",
            "#omitted{display:flex;width:32px;height:8px;gap:2px;flex-shrink:0;}",
            "#invalid{display:flex;width:32px;height:8px;gap:2px;justify-content:center;justify-content:inherit center;flex-shrink:0;}",
            ".item{width:4px;height:4px;flex-shrink:0;}",
            "</style>",
            "<div id='parent'>",
            "<div id='child'><button id='child-first' class='item' style='background-color:red'>A</button><button id='child-second' class='item'>B</button></div>",
            "<div id='omitted'><button id='omitted-first' class='item'>C</button><button id='omitted-second' class='item'>D</button></div>",
            "<div id='invalid'><button id='invalid-first' class='item'>E</button><button id='invalid-second' class='item'>F</button></div>",
            "</div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let child = target("child");
    let child_first = target("child-first");
    let omitted = target("omitted");
    let omitted_first = target("omitted-first");
    let invalid = target("invalid");
    let invalid_first = target("invalid-first");

    let layout = document.layout(viewport).unwrap();
    let child_box = layout.box_for(child).unwrap();
    let child_first_box = layout.box_for(child_first).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let omitted_first_box = layout.box_for(omitted_first).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    let invalid_first_box = layout.box_for(invalid_first).unwrap();
    assert!(child_first_box.x > child_box.x);
    assert_eq!(omitted_first_box.x, omitted_box.x);
    assert!(invalid_first_box.x > invalid_box.x);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == child_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == omitted_first)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == child_first && rect.x == child_first_box.x && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(
                i64::from(child_first_box.x + 1),
                i64::from(child_first_box.y + 1),
            )
            .unwrap(),
        Some(child_first)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(child_first_box.x + 1, child_first_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    let unsupported = NativeDocument::parse(
        "<div style='justify-content:inherit center'>Unsupported</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(unsupported.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "justify-content"
    }));
}
#[test]
fn native_flex_align_items_css_wide_resets_reach_layout_and_raster() {
    let document = NativeDocument::parse(
        "<style>.row { display:flex; width:12px; height:32px; gap:2px; } #initial { align-items:center; } #initial { align-items:INITIAL; } #unset { align-items:flex-end; } #unset { align-items:UNSET; } #revert { align-items:center; } #revert { align-items:ReVeRt; } #invalid { align-items:center; } #invalid { align-items:inherit center; } #important { align-items:flex-end !important; } #important { align-items:initial; } #layered { align-items:flex-start; } @layer theme { #layered { align-items:flex-end; } } #layered { align-items:revert-layer; } .item { width:4px; height:4px; flex-shrink:0; }</style><div id='initial' class='row'><button id='initial-first' class='item' style='background-color:red'>A</button><button id='initial-second' class='item' style='background-color:blue'>B</button></div><div id='unset' class='row'><button id='unset-first' class='item' style='background-color:red'>C</button><button id='unset-second' class='item' style='background-color:blue'>D</button></div><div id='revert' class='row'><button id='revert-first' class='item' style='background-color:red'>E</button><button id='revert-second' class='item' style='background-color:blue'>F</button></div><div id='invalid' class='row'><button id='invalid-first' class='item' style='background-color:red'>G</button><button id='invalid-second' class='item' style='background-color:blue'>H</button></div><div id='important' class='row'><button id='important-first' class='item' style='background-color:red'>I</button><button id='important-second' class='item' style='background-color:blue'>J</button></div><div id='layered' class='row'><button id='layered-first' class='item' style='background-color:red'>K</button><button id='layered-second' class='item' style='background-color:blue'>L</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let initial_first = target("initial-first");
    let initial_second = target("initial-second");
    let unset_first = target("unset-first");
    let unset_second = target("unset-second");
    let revert_first = target("revert-first");
    let revert_second = target("revert-second");
    let invalid_first = target("invalid-first");
    let invalid_second = target("invalid-second");
    let important_first = target("important-first");
    let important_second = target("important-second");
    let layered_first = target("layered-first");
    let layered_second = target("layered-second");

    let layout = document.layout(viewport).unwrap();
    let y_pair = |first, second| {
        (
            layout.box_for(first).unwrap().y,
            layout.box_for(second).unwrap().y,
        )
    };
    assert_eq!(y_pair(initial_first, initial_second), (0, 0));
    assert_eq!(y_pair(unset_first, unset_second), (32, 32));
    assert_eq!(y_pair(revert_first, revert_second), (64, 64));
    assert_eq!(y_pair(invalid_first, invalid_second), (110, 110));
    assert_eq!(y_pair(important_first, important_second), (156, 156));
    assert_eq!(y_pair(layered_first, layered_second), (188, 188));
    assert_eq!(layout.box_for(initial_first).unwrap().x, 0);
    assert_eq!(layout.box_for(initial_second).unwrap().x, 6);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == initial_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == initial_second)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == initial_first && rect.y == 0 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(initial_first));
    assert_eq!(layout.hit_test(1, 111).unwrap(), Some(invalid_first));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 111), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-items"
    }));
}
#[test]
fn native_flex_align_items_inherit_reaches_nested_layout_and_raster() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#parent{display:flex;flex-direction:column;align-items:flex-end;width:40px;height:40px;}",
            "#child{display:flex;width:32px;height:8px;gap:2px;align-items:inherit;flex-shrink:0;}",
            "#omitted{display:flex;width:32px;height:8px;gap:2px;flex-shrink:0;}",
            "#invalid{display:flex;width:32px;height:8px;gap:2px;align-items:flex-start;align-items:inherit center;flex-shrink:0;}",
            ".item{width:4px;height:4px;flex-shrink:0;}",
            "</style>",
            "<div id='parent'>",
            "<div id='child'><button id='child-first' class='item' style='background-color:red'>A</button><button id='child-second' class='item'>B</button></div>",
            "<div id='omitted'><button id='omitted-first' class='item'>C</button><button id='omitted-second' class='item'>D</button></div>",
            "<div id='invalid'><button id='invalid-first' class='item'>E</button><button id='invalid-second' class='item'>F</button></div>",
            "</div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 48,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let child = target("child");
    let child_first = target("child-first");
    let omitted = target("omitted");
    let omitted_first = target("omitted-first");
    let invalid = target("invalid");
    let invalid_first = target("invalid-first");

    let layout = document.layout(viewport).unwrap();
    let child_box = layout.box_for(child).unwrap();
    let child_first_box = layout.box_for(child_first).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let omitted_first_box = layout.box_for(omitted_first).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    let invalid_first_box = layout.box_for(invalid_first).unwrap();
    assert!(child_first_box.y > child_box.y);
    assert_eq!(omitted_first_box.y, omitted_box.y);
    assert_eq!(invalid_first_box.y, invalid_box.y);
    assert!(child_box.x > 0);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == child_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == omitted_first)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == child_first
                    && rect.y == child_first_box.y
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(
                i64::from(child_first_box.x + 1),
                i64::from(child_first_box.y + 1),
            )
            .unwrap(),
        Some(child_first)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(child_first_box.x + 1, child_first_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    let unsupported = NativeDocument::parse(
        "<div style='align-items:inherit center'>Unsupported</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(unsupported.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-items"
    }));
}
#[test]
fn native_flex_align_self_css_wide_resets_delegate_and_move_artifacts() {
    let document = NativeDocument::parse(
        "<style>.row { display:flex; width:12px; height:32px; gap:2px; align-items:center; } #initial-first { align-self:flex-end; } #initial-first { align-self:INITIAL; } #unset-first { align-self:flex-start; } #unset-first { align-self:UNSET; } #revert-first { align-self:flex-end; } #revert-first { align-self:ReVeRt; } #invalid-first { align-self:flex-end; } #invalid-first { align-self:inherit center; } #important-first { align-self:flex-start !important; } #important-first { align-self:initial; } #layered-first { align-self:flex-start; } @layer theme { #layered-first { align-self:flex-end; } } #layered-first { align-self:revert-layer; } .item { width:4px; height:4px; flex-shrink:0; } .nested { display:block; height:2px; }</style><div id='initial' class='row'><button id='initial-first' class='item' style='background-color:red'><span id='nested' class='nested'>A</span></button><button id='initial-second' class='item' style='background-color:blue'>B</button></div><div id='unset' class='row'><button id='unset-first' class='item' style='background-color:red'>C</button><button id='unset-second' class='item' style='background-color:blue'>D</button></div><div id='revert' class='row'><button id='revert-first' class='item' style='background-color:red'>E</button><button id='revert-second' class='item' style='background-color:blue'>F</button></div><div id='invalid' class='row'><button id='invalid-first' class='item' style='background-color:red'>G</button><button id='invalid-second' class='item' style='background-color:blue'>H</button></div><div id='important' class='row'><button id='important-first' class='item' style='background-color:red'>I</button><button id='important-second' class='item' style='background-color:blue'>J</button></div><div id='layered' class='row'><button id='layered-first' class='item' style='background-color:red'>K</button><button id='layered-second' class='item' style='background-color:blue'>L</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let initial_first = target("initial-first");
    let initial_second = target("initial-second");
    let nested = target("nested");
    let unset_first = target("unset-first");
    let unset_second = target("unset-second");
    let revert_first = target("revert-first");
    let revert_second = target("revert-second");
    let invalid_first = target("invalid-first");
    let invalid_second = target("invalid-second");
    let important_first = target("important-first");
    let important_second = target("important-second");
    let layered_first = target("layered-first");
    let layered_second = target("layered-second");

    let layout = document.layout(viewport).unwrap();
    let y_pair = |first, second| {
        (
            layout.box_for(first).unwrap().y,
            layout.box_for(second).unwrap().y,
        )
    };
    assert_eq!(y_pair(initial_first, initial_second), (14, 14));
    assert_eq!(y_pair(unset_first, unset_second), (46, 46));
    assert_eq!(y_pair(revert_first, revert_second), (78, 78));
    assert_eq!(y_pair(invalid_first, invalid_second), (124, 110));
    assert_eq!(y_pair(important_first, important_second), (128, 142));
    assert_eq!(y_pair(layered_first, layered_second), (188, 174));
    assert_eq!(layout.box_for(nested).unwrap().y, 14);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == initial_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == initial_second)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == initial_first && rect.y == 14 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(1, 15).unwrap(), Some(nested));
    assert_eq!(layout.hit_test(1, 125).unwrap(), Some(invalid_first));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 15), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 125), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-self"
    }));
}
#[test]
fn native_flex_align_self_inherit_reaches_three_level_layout_and_raster() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#grand{display:flex;width:48px;height:48px;align-items:flex-start;}",
            "#parent{display:flex;flex-direction:column;align-items:center;align-self:flex-end;width:32px;height:32px;flex-shrink:0;}",
            "#child{display:block;width:24px;height:8px;align-self:inherit;}",
            "#auto{display:block;width:24px;height:8px;align-self:auto;}",
            "#omitted{display:block;width:24px;height:8px;}",
            "#invalid{display:block;width:24px;height:8px;align-self:flex-start;align-self:inherit center;}",
            "</style>",
            "<div id='grand'><div id='parent'>",
            "<button id='child' style='background-color:red'>Inherited</button>",
            "<button id='auto'>Auto</button>",
            "<button id='omitted'>Omitted</button>",
            "<button id='invalid'>Invalid</button>",
            "</div></div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 56,
        height: 56,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let parent = target("parent");
    let child = target("child");
    let auto = target("auto");
    let omitted = target("omitted");
    let invalid = target("invalid");

    let layout = document.layout(viewport).unwrap();
    let parent_box = layout.box_for(parent).unwrap();
    let child_box = layout.box_for(child).unwrap();
    let auto_box = layout.box_for(auto).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    assert!(parent_box.y > 0);
    assert!(child_box.x > auto_box.x);
    assert_eq!(auto_box.x, omitted_box.x);
    assert!(invalid_box.x < auto_box.x);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == child)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == invalid)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == child && rect.x == child_box.x && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(i64::from(child_box.x + 1), i64::from(child_box.y + 1),)
            .unwrap(),
        Some(child)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(child_box.x + 1, child_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    let unsupported = NativeDocument::parse(
        "<div style='align-self:inherit center'>Unsupported</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(unsupported.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-self"
    }));
}
#[test]
fn native_flex_align_content_css_wide_resets_reach_layout_and_raster() {
    let document = NativeDocument::parse(
        "<style>.row { display:flex; width:12px; height:32px; gap:2px; flex-wrap:wrap; align-items:center; } #initial { align-content:center; } #initial { align-content:INITIAL; } #unset { align-content:flex-end; } #unset { align-content:UNSET; } #revert { align-content:center; } #revert { align-content:ReVeRt; } #terminal { align-content:center; align-content:initial; } #invalid { align-content:center; align-content:initial; align-content:inherit; } #important { align-content:flex-end !important; } #important { align-content:initial; } #layered { align-content:flex-start; } @layer theme { #layered { align-content:flex-end; } } #layered { align-content:revert-layer; } .item { width:4px; height:4px; flex-shrink:0; } .nested { display:block; height:2px; }</style><div id='initial' class='row'><button id='initial-first' class='item' style='background-color:red'><span id='nested' class='nested'>A</span></button><button id='initial-second' class='item' style='background-color:blue'>B</button><button id='initial-third' class='item' style='background-color:green'>C</button></div><div id='unset' class='row'><button id='unset-first' class='item' style='background-color:red'>D</button><button id='unset-second' class='item' style='background-color:blue'>E</button><button id='unset-third' class='item' style='background-color:green'>F</button></div><div id='revert' class='row'><button id='revert-first' class='item' style='background-color:red'>G</button><button id='revert-second' class='item' style='background-color:blue'>H</button><button id='revert-third' class='item' style='background-color:green'>I</button></div><div id='terminal' class='row'><button id='terminal-first' class='item' style='background-color:red'>J</button><button id='terminal-second' class='item' style='background-color:blue'>K</button><button id='terminal-third' class='item' style='background-color:green'>L</button></div><div id='invalid' class='row'><button id='invalid-first' class='item' style='background-color:red'>M</button><button id='invalid-second' class='item' style='background-color:blue'>N</button><button id='invalid-third' class='item' style='background-color:green'>O</button></div><div id='important' class='row'><button id='important-first' class='item' style='background-color:red'>P</button><button id='important-second' class='item' style='background-color:blue'>Q</button><button id='important-third' class='item' style='background-color:green'>R</button></div><div id='layered' class='row'><button id='layered-first' class='item' style='background-color:red'>S</button><button id='layered-second' class='item' style='background-color:blue'>T</button><button id='layered-third' class='item' style='background-color:green'>U</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let initial_first = target("initial-first");
    let initial_second = target("initial-second");
    let unset_first = target("unset-first");
    let unset_second = target("unset-second");
    let revert_first = target("revert-first");
    let revert_second = target("revert-second");
    let terminal_first = target("terminal-first");
    let terminal_second = target("terminal-second");
    let invalid_first = target("invalid-first");
    let invalid_second = target("invalid-second");
    let important_first = target("important-first");
    let important_second = target("important-second");
    let layered_first = target("layered-first");
    let layered_second = target("layered-second");
    let nested = target("nested");

    let layout = document.layout(viewport).unwrap();
    let y_pair = |first, second| {
        (
            layout.box_for(first).unwrap().y,
            layout.box_for(second).unwrap().y,
        )
    };
    assert_eq!(y_pair(initial_first, initial_second), (0, 0));
    assert_eq!(y_pair(unset_first, unset_second), (32, 32));
    assert_eq!(y_pair(revert_first, revert_second), (64, 64));
    assert_eq!(y_pair(terminal_first, terminal_second), (96, 96));
    assert_eq!(y_pair(invalid_first, invalid_second), (128, 128));
    assert_eq!(y_pair(important_first, important_second), (182, 182));
    assert_eq!(y_pair(layered_first, layered_second), (214, 214));
    assert_eq!(layout.box_for(nested).unwrap().y, 0);
    assert_eq!(layout.box_for(initial_first).unwrap().height, 4);

    let semantic_nodes = document.semantic_nodes();
    for pair in [
        initial_first,
        initial_second,
        unset_first,
        unset_second,
        revert_first,
        revert_second,
        terminal_first,
        terminal_second,
        invalid_first,
        invalid_second,
        important_first,
        important_second,
        layered_first,
        layered_second,
    ]
    .windows(2)
    {
        assert!(
            semantic_nodes
                .iter()
                .position(|node| node.node_id == pair[0])
                < semantic_nodes
                    .iter()
                    .position(|node| node.node_id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == initial_first && rect.y == 0 && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == important_first && rect.y == 182 && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(nested));
    assert_eq!(layout.hit_test(1, 183).unwrap(), Some(important_first));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 183), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    let unsupported = NativeDocument::parse(
        "<div style='align-content:initial center'>Unsupported</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(unsupported.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-content"
    }));
}
#[test]
fn native_flex_align_content_inherit_reaches_nested_layout_and_raster() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#parent{display:flex;flex-direction:column;align-items:flex-start;align-content:center;width:20px;height:120px;}",
            "#child{display:flex;width:12px;height:32px;flex-wrap:wrap;gap:2px;align-content:inherit;flex-shrink:0;}",
            "#omitted{display:flex;width:12px;height:32px;flex-wrap:wrap;gap:2px;flex-shrink:0;}",
            "#invalid{display:flex;width:12px;height:32px;flex-wrap:wrap;gap:2px;align-content:center;align-content:inherit center;flex-shrink:0;}",
            ".item{width:4px;height:4px;flex-shrink:0;}",
            "</style>",
            "<div id='parent'>",
            "<div id='child'><button id='child-first' class='item' style='background-color:red'>A</button><button id='child-second' class='item'>B</button><button id='child-third' class='item'>C</button></div>",
            "<div id='omitted'><button id='omitted-first' class='item'>D</button><button id='omitted-second' class='item'>E</button><button id='omitted-third' class='item'>F</button></div>",
            "<div id='invalid'><button id='invalid-first' class='item'>G</button><button id='invalid-second' class='item'>H</button><button id='invalid-third' class='item'>I</button></div>",
            "</div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let child = target("child");
    let child_first = target("child-first");
    let omitted = target("omitted");
    let omitted_first = target("omitted-first");
    let invalid = target("invalid");
    let invalid_first = target("invalid-first");

    let layout = document.layout(viewport).unwrap();
    let child_box = layout.box_for(child).unwrap();
    let child_first_box = layout.box_for(child_first).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let omitted_first_box = layout.box_for(omitted_first).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    let invalid_first_box = layout.box_for(invalid_first).unwrap();
    assert!(child_first_box.y > child_box.y);
    assert_eq!(omitted_first_box.y, omitted_box.y);
    assert!(invalid_first_box.y > invalid_box.y);

    let semantic_nodes = document.semantic_nodes();
    assert!(
        semantic_nodes
            .iter()
            .position(|node| node.node_id == child_first)
            .unwrap()
            < semantic_nodes
                .iter()
                .position(|node| node.node_id == omitted_first)
                .unwrap()
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == child_first && rect.y == child_first_box.y && *color == NativeColor::RED
        )
    }));
    assert_eq!(
        layout
            .hit_test(
                i64::from(child_first_box.x + 1),
                i64::from(child_first_box.y + 1),
            )
            .unwrap(),
        Some(child_first)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(child_first_box.x + 1, child_first_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    let unsupported = NativeDocument::parse(
        "<div style='align-content:inherit center'>Unsupported</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(unsupported.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "align-content"
    }));
}
#[test]
fn native_flex_flow_place_content_revert_layer_preserves_layout_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #flow { display:flex;width:14px;height:8px;flex-flow:row nowrap;align-items:flex-start;align-content:flex-start; } #content { display:flex;width:14px;height:16px;flex-flow:row wrap;align-items:flex-start;place-content:flex-start; } } @layer theme { #flow { flex-flow:column wrap; } #content { place-content:space-between flex-end; } } @layer top { #flow { flex-flow:column-reverse wrap; flex-flow:revert-layer; } #content { place-content:center center; place-content:revert-layer; } } #flow { flex-flow:row-reverse nowrap; flex-flow:revert-layer; } #content { place-content:flex-start flex-start; place-content:revert-layer; }</style><div id='flow'><div id='flow-first' style='width:4px;height:4px;flex-shrink:0;background-color:red'>A</div><div id='flow-second' style='width:4px;height:4px;flex-shrink:0;background-color:blue'>B</div><div id='flow-third' style='width:4px;height:4px;flex-shrink:0;background-color:green'>C</div></div><div id='content'><div id='content-first' style='width:8px;height:4px;flex-shrink:0;background-color:red'>D</div><div id='content-second' style='width:8px;height:4px;flex-shrink:0;background-color:blue'>E</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(diagnostic.detail.as_str(), "flex-flow" | "place-content"))
    }));

    let flow = document.resolve_target("id=flow").unwrap();
    let flow_first = document.resolve_target("id=flow-first").unwrap();
    let flow_second = document.resolve_target("id=flow-second").unwrap();
    let flow_third = document.resolve_target("id=flow-third").unwrap();
    let content = document.resolve_target("id=content").unwrap();
    let content_first = document.resolve_target("id=content-first").unwrap();
    let content_second = document.resolve_target("id=content-second").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let flow_rect = layout.box_for(flow).unwrap();
    let content_rect = layout.box_for(content).unwrap();
    let flow_first_rect = layout.box_for(flow_first).unwrap();
    let flow_second_rect = layout.box_for(flow_second).unwrap();
    let flow_third_rect = layout.box_for(flow_third).unwrap();
    assert_eq!(flow_first_rect.x, flow_rect.x);
    assert_eq!(flow_first_rect.y, flow_rect.y);
    assert_eq!(flow_second_rect.x, flow_rect.x);
    assert_eq!(flow_second_rect.y, flow_rect.y + 4);
    assert_eq!(flow_third_rect.x, flow_rect.x + 4);
    assert_eq!(flow_third_rect.y, flow_rect.y);

    let content_first_rect = layout.box_for(content_first).unwrap();
    let content_second_rect = layout.box_for(content_second).unwrap();
    assert_eq!(content_first_rect.x, content_rect.x + 6);
    assert_eq!(content_first_rect.y, content_rect.y);
    assert_eq!(content_second_rect.x, content_rect.x + 6);
    assert_eq!(content_second_rect.y, content_rect.y + 12);
    assert_eq!(
        layout.hit_test(
            i64::from(flow_first_rect.x + 1),
            i64::from(flow_first_rect.y + 1),
        ),
        Ok(Some(flow_first))
    );
    assert_eq!(
        layout.hit_test(
            i64::from(content_second_rect.x + 1),
            i64::from(content_second_rect.y + 1),
        ),
        Ok(Some(content_second))
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, .. }
                if *node_id == flow_first && *rect == flow_first_rect
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(flow_first_rect.x + 1, flow_first_rect.y + 1),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(content_second_rect.x + 1, content_second_rect.y + 1),
        Some([0, 0, 255, 255])
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
fn native_flex_place_content_css_wide_resets_reach_both_axes_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>.row { display:flex; width:32px; height:32px; gap:2px; flex-wrap:wrap; align-items:center; } #initial { place-content:center space-between; } #initial { place-content:INITIAL; } #unset { place-content:flex-end center; } #unset { place-content:UNSET; } #revert { place-content:center flex-end; } #revert { place-content:ReVeRt; } #terminal { place-content:center flex-end; place-content:initial; } #invalid { place-content:center space-between; place-content:initial center; } #important { place-content:flex-end center !important; } #important { place-content:initial; } #layered { place-content:flex-start flex-start; } @layer theme { #layered { place-content:flex-end flex-end; } } #layered { place-content:revert-layer; } .item { width:8px; height:4px; flex-shrink:0; } .nested { display:block; height:2px; }</style><div id='initial' class='row'><button id='initial-first' class='item' style='background-color:red'><span id='nested' class='nested'>A</span></button><button id='initial-second' class='item' style='background-color:blue'>B</button><button id='initial-third' class='item' style='background-color:green'>C</button><button id='initial-fourth' class='item' style='background-color:black'>D</button></div><div id='unset' class='row'><button id='unset-first' class='item' style='background-color:red'>E</button><button id='unset-second' class='item' style='background-color:blue'>F</button><button id='unset-third' class='item' style='background-color:green'>G</button><button id='unset-fourth' class='item' style='background-color:black'>H</button></div><div id='revert' class='row'><button id='revert-first' class='item' style='background-color:red'>I</button><button id='revert-second' class='item' style='background-color:blue'>J</button><button id='revert-third' class='item' style='background-color:green'>K</button><button id='revert-fourth' class='item' style='background-color:black'>L</button></div><div id='terminal' class='row'><button id='terminal-first' class='item' style='background-color:red'>M</button><button id='terminal-second' class='item' style='background-color:blue'>N</button><button id='terminal-third' class='item' style='background-color:green'>O</button><button id='terminal-fourth' class='item' style='background-color:black'>P</button></div><div id='invalid' class='row'><button id='invalid-first' class='item' style='background-color:red'>Q</button><button id='invalid-second' class='item' style='background-color:blue'>R</button><button id='invalid-third' class='item' style='background-color:green'>S</button><button id='invalid-fourth' class='item' style='background-color:black'>T</button></div><div id='important' class='row'><button id='important-first' class='item' style='background-color:red'>U</button><button id='important-second' class='item' style='background-color:blue'>V</button><button id='important-third' class='item' style='background-color:green'>W</button><button id='important-fourth' class='item' style='background-color:black'>X</button></div><div id='layered' class='row'><button id='layered-first' class='item' style='background-color:red'>Y</button><button id='layered-second' class='item' style='background-color:blue'>Z</button><button id='layered-third' class='item' style='background-color:green'>AA</button><button id='layered-fourth' class='item' style='background-color:black'>AB</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let initial_first = target("initial-first");
    let initial_second = target("initial-second");
    let unset_first = target("unset-first");
    let unset_second = target("unset-second");
    let revert_first = target("revert-first");
    let revert_second = target("revert-second");
    let terminal_first = target("terminal-first");
    let terminal_second = target("terminal-second");
    let invalid_first = target("invalid-first");
    let invalid_second = target("invalid-second");
    let important_first = target("important-first");
    let important_second = target("important-second");
    let layered_first = target("layered-first");
    let layered_second = target("layered-second");
    let nested = target("nested");

    let layout = document.layout(viewport).unwrap();
    let rect = |node_id| layout.box_for(node_id).expect("layout rectangle");
    assert_eq!(rect(initial_first).x, 0);
    assert_eq!(rect(initial_second).x, 10);
    assert_eq!(rect(initial_first).y, 0);
    assert_eq!(rect(unset_first).x, 0);
    assert_eq!(rect(unset_second).x, 10);
    assert_eq!(rect(unset_first).y, 32);
    assert_eq!(rect(revert_first).x, 0);
    assert_eq!(rect(revert_second).x, 10);
    assert_eq!(rect(revert_first).y, 64);
    assert_eq!(rect(terminal_first).x, 0);
    assert_eq!(rect(terminal_second).x, 10);
    assert_eq!(rect(terminal_first).y, 96);
    assert_eq!(rect(invalid_first).x, 0);
    assert_eq!(rect(invalid_second).x, 12);
    assert_eq!(rect(invalid_first).y, 139);
    assert_eq!(rect(important_first).x, 2);
    assert_eq!(rect(important_second).x, 12);
    assert_eq!(rect(important_first).y, 182);
    assert_eq!(rect(layered_first).x, 4);
    assert_eq!(rect(layered_second).x, 14);
    assert_eq!(rect(layered_first).y, 214);
    assert_eq!(rect(nested).y, 0);

    let semantic_nodes = document.semantic_nodes();
    for pair in [
        initial_first,
        initial_second,
        unset_first,
        unset_second,
        revert_first,
        revert_second,
        terminal_first,
        terminal_second,
        invalid_first,
        invalid_second,
        important_first,
        important_second,
        layered_first,
        layered_second,
    ]
    .windows(2)
    {
        assert!(
            semantic_nodes
                .iter()
                .position(|node| node.node_id == pair[0])
                < semantic_nodes
                    .iter()
                    .position(|node| node.node_id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == initial_first
                    && rect.x == 0
                    && rect.y == 0
                    && *color == NativeColor::RED
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == important_first
                    && rect.x == 2
                    && rect.y == 182
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(nested));
    assert_eq!(layout.hit_test(3, 183).unwrap(), Some(important_first));
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(3, 183), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "place-content"
    }));
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
fn native_flex_place_content_inherit_projects_both_axes_and_artifacts() {
    let document = NativeDocument::parse(
        concat!(
            "<style>",
            "#parent{width:48px;height:100px;align-content:space-around;justify-content:flex-end;}",
            ".row{display:flex;width:32px;height:24px;gap:2px;flex-wrap:wrap;}",
            "#inherited{place-content:InHeRiT;}",
            "#invalid{place-content:center;place-content:inherit center;}",
            ".item{width:10px;height:4px;flex-shrink:0;}",
            "</style>",
            "<div id='parent'>",
            "<div id='inherited' class='row'><button id='inherited-first' class='item' style='background-color:red'>A</button><button id='inherited-second' class='item'>B</button><button id='inherited-third' class='item'>C</button></div>",
            "<div id='omitted' class='row'><button id='omitted-first' class='item'>D</button><button id='omitted-second' class='item'>E</button><button id='omitted-third' class='item'>F</button></div>",
            "<div id='invalid' class='row'><button id='invalid-first' class='item'>G</button><button id='invalid-second' class='item'>H</button><button id='invalid-third' class='item'>I</button></div>",
            "</div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 52,
        height: 100,
        device_scale_factor_milli: 1000,
    };
    let target = |id: &str| document.resolve_target(&format!("id={id}")).unwrap();
    let inherited = target("inherited");
    let inherited_first = target("inherited-first");
    let omitted = target("omitted");
    let omitted_first = target("omitted-first");
    let invalid = target("invalid");
    let invalid_first = target("invalid-first");

    let layout = document.layout(viewport).unwrap();
    let inherited_box = layout.box_for(inherited).unwrap();
    let inherited_first_box = layout.box_for(inherited_first).unwrap();
    let omitted_box = layout.box_for(omitted).unwrap();
    let omitted_first_box = layout.box_for(omitted_first).unwrap();
    let invalid_box = layout.box_for(invalid).unwrap();
    let invalid_first_box = layout.box_for(invalid_first).unwrap();
    assert!(inherited_first_box.x > inherited_box.x);
    assert!(inherited_first_box.y > inherited_box.y);
    assert_eq!(omitted_first_box.x, omitted_box.x);
    assert_eq!(omitted_first_box.y, omitted_box.y);
    assert!(invalid_first_box.x > invalid_box.x);
    assert!(invalid_first_box.y > invalid_box.y);
    assert_ne!(inherited_first_box.x, invalid_first_box.x);
    assert_ne!(inherited_first_box.y, invalid_first_box.y);

    let semantic_nodes = document.semantic_nodes();
    for pair in [
        [inherited_first, target("inherited-second")],
        [omitted_first, target("omitted-second")],
        [invalid_first, target("invalid-second")],
    ] {
        assert!(
            semantic_nodes
                .iter()
                .position(|node| node.node_id == pair[0])
                .unwrap()
                < semantic_nodes
                    .iter()
                    .position(|node| node.node_id == pair[1])
                    .unwrap()
        );
    }

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
        Some(inherited_first)
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(inherited_first_box.x + 1, inherited_first_box.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "place-content"
    }));
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

    let engine_viewport = Viewport {
        width: viewport.width + 16,
        height: viewport.height + 16,
        ..viewport
    };
    let config = NativeEngineConfig::default()
        .with_viewport(engine_viewport)
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
            x: 8,
            y: 12,
            width: 8,
            height: 12,
        })
    );
    assert_eq!(engine.hit_test(9, 14).unwrap(), Some(first));
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
fn native_grid_tracks_place_children_through_layout_paint_and_hit_test() {
    let document = NativeDocument::parse(
        "<style>#grid { grid-template-columns: REPEAT(2, 1FR); grid-template-rows: 8px 8px; gap: 2px; align-items: center; }</style><div id='grid' style='display:grid;width:24px;height:18px'><div id='first' style='height:4px;background-color:red'>A</div><div id='second' style='height:4px;background-color:green'>B</div><div id='third' style='height:4px;background-color:blue'>C</div><div id='fourth' style='height:4px;background-color:black'>D</div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let grid = document.resolve_target("id=grid").unwrap();
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let third = document.resolve_target("id=third").unwrap();
    let fourth = document.resolve_target("id=fourth").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(grid).unwrap().width, 24);
    assert_eq!(layout.box_for(grid).unwrap().height, 18);
    assert_eq!(
        layout.box_for(first).unwrap(),
        NativeRect {
            x: 0,
            y: 2,
            width: 11,
            height: 4,
        }
    );
    assert_eq!(
        layout.box_for(second).unwrap(),
        NativeRect {
            x: 13,
            y: 2,
            width: 11,
            height: 4,
        }
    );
    assert_eq!(
        layout.box_for(third).unwrap(),
        NativeRect {
            x: 0,
            y: 12,
            width: 11,
            height: 4,
        }
    );
    assert_eq!(
        layout.box_for(fourth).unwrap(),
        NativeRect {
            x: 13,
            y: 12,
            width: 11,
            height: 4,
        }
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == first && *rect == NativeRect { x: 0, y: 2, width: 11, height: 4 }
                    && *color == NativeColor::RED
        )
    }));
    assert_eq!(layout.hit_test(1, 3).unwrap(), Some(first));
    assert_eq!(layout.hit_test(14, 13).unwrap(), Some(fourth));
}

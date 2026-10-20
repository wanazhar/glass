use super::*;

#[test]
fn native_paint_color_important_priority_reaches_fill_and_text_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:48px;height:20px;line-height:20px;border:1px solid blue;text-decoration:underline; }
            @layer base {
                #early { background-color:red !important;color:red !important;text-decoration-color:red !important; }
                #rollback { background-color:red !important;background-color:revert-layer !important;color:red !important;color:revert-layer !important;text-decoration-color:red !important;text-decoration-color:revert-layer !important; }
                #normal { background-color:red;color:red;text-decoration-color:red; }
                #mixed { background-color:red !important;color:red !important;text-decoration-color:red !important; }
                #invalid { background-color:red !important;background-color:invalid !important;color:red !important;color:invalid !important;text-decoration-color:red !important;text-decoration-color:invalid !important; }
            }
            @layer theme {
                #early { background-color:blue !important;color:blue !important;text-decoration-color:blue !important; }
                #rollback { background-color:green !important;color:green !important;text-decoration-color:green !important; }
                #normal { background-color:blue;color:blue;text-decoration-color:blue; }
            }
            #early { background-color:green !important;color:green !important;text-decoration-color:green !important; }
            #normal { background-color:green;color:green;text-decoration-color:green; }
            #mixed { background-color:green;color:green;text-decoration-color:green; }
            #inline { background-color:red !important;color:red !important;text-decoration-color:red !important; }
        </style>
        <div id='early' class='box' role='button'>Early</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='inline' class='box' role='button' style='background-color:green !important;color:green !important;text-decoration-color:green !important'>Inline</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 56,
        height: 140,
        device_scale_factor_milli: 1000,
    };
    let early = document.resolve_target("id=early").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [early, rollback, normal, mixed, invalid, inline];
    let red = NativeColor::RED;
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let layout = document.layout(viewport).unwrap();
    for node_id in targets {
        assert!(
            layout.box_for(node_id).is_some(),
            "missing paint box for {node_id:?}"
        );
    }

    let list = document.display_list(viewport).unwrap();
    let fill_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                color,
                ..
            } if *command_node_id == node_id => Some(*color),
            _ => None,
        })
    };
    let text_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                color,
                decoration_color,
                ..
            } if *command_node_id == node_id => Some((*color, *decoration_color)),
            _ => None,
        })
    };
    for (node_id, expected) in [
        (early, red),
        (rollback, green),
        (normal, green),
        (mixed, red),
        (invalid, red),
        (inline, green),
    ] {
        assert_eq!(fill_for(node_id), Some(expected), "fill for {node_id:?}");
        assert_eq!(
            text_for(node_id),
            Some((expected, expected)),
            "text for {node_id:?}"
        );
    }
    assert_eq!(layout.hit_test(10, 10).unwrap(), Some(early));
    let early_box = layout.box_for(early).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(early_box.x + early_box.width - 5, early_box.y + 10),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in targets.windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }
    for detail in ["background-color", "color", "text-decoration-color"] {
        assert!(
            document.diagnostics().iter().any(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == detail
            }),
            "missing diagnostic for {detail}"
        );
    }
}

#[test]
fn native_inherited_text_spacing_revert_layer_preserves_consumers() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:48px; line-height:20px; } @layer base { #named { word-spacing:2px; letter-spacing:1px; } #repeated { word-spacing:4px; letter-spacing:2px; } #parent { word-spacing:3px; letter-spacing:1px; } } @layer theme { #named { word-spacing:6px; letter-spacing:3px; } #repeated { word-spacing:revert-layer; letter-spacing:revert-layer; } #parent { word-spacing:8px; letter-spacing:4px; } } @layer top { #repeated { word-spacing:revert-layer; letter-spacing:revert-layer; } } #named { word-spacing:ReVeRt-LaYeR; letter-spacing:revert-layer; } #parent { word-spacing:revert-layer; letter-spacing:revert-layer; }</style><div id='named' class='line'>A B</div><div id='repeated' class='line'>A B</div><div id='parent' class='line'>A <span id='child' style='word-spacing:revert-layer;letter-spacing:REVERT-LAYER'>B</span></div><div id='fallback' class='line' style='word-spacing:revert-layer;letter-spacing:revert-layer'>A B</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && (diagnostic.detail == "word-spacing" || diagnostic.detail == "letter-spacing"))
    }));
    let named = document.resolve_target("id=named").unwrap();
    let repeated = document.resolve_target("id=repeated").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let viewport = Viewport {
        width: 64,
        height: 100,
        device_scale_factor_milli: 1000,
    };

    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let spacing_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                letter_spacing,
                word_spacing,
                ..
            } if *command_node == node_id => Some((*word_spacing, *letter_spacing)),
            _ => None,
        })
    };
    assert_eq!(spacing_for(named), Some((6, 3)));
    assert_eq!(spacing_for(repeated), Some((4, 2)));
    assert_eq!(spacing_for(parent), Some((8, 4)));
    assert_eq!(spacing_for(child), Some((8, 4)));
    assert_eq!(spacing_for(fallback), Some((0, 0)));

    let named_run = layout
        .text_runs
        .iter()
        .find(|run| run.node_id == named)
        .unwrap();
    let surface = list.rasterize().unwrap();
    let has_black_pixel = |origin: NativePoint| {
        (0..8).any(|x| {
            (0..12).any(|y| surface.pixel(origin.x + x, origin.y + y) == Some([0, 0, 0, 255]))
        })
    };
    assert!(has_black_pixel(named_run.origin));
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(named));
}

#[test]
fn native_inherited_text_css_wide_resets_reach_layout_and_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
        #parent { display:block; width:64px; line-height:24px; white-space:pre; text-transform:uppercase; font-weight:bold; font-style:italic; word-break:break-all; vertical-align:middle; word-spacing:4px; letter-spacing:2px; }
        .probe { display:block; width:64px; }
        #inherit { white-space:inherit; line-height:inherit; text-transform:inherit; font-weight:inherit; font-style:inherit; word-break:inherit; vertical-align:inherit; word-spacing:inherit; letter-spacing:inherit; background-color:#ff0000; }
        #unset { white-space:unset; line-height:unset; text-transform:unset; font-weight:unset; font-style:unset; word-break:unset; vertical-align:unset; word-spacing:unset; letter-spacing:unset; background-color:#00ff00; }
        #revert { white-space:revert; line-height:revert; text-transform:revert; font-weight:revert; font-style:revert; word-break:revert; vertical-align:revert; word-spacing:revert; letter-spacing:revert; background-color:#0000ff; }
        #initial { white-space:initial; line-height:initial; text-transform:initial; font-weight:initial; font-style:initial; word-break:initial; vertical-align:initial; word-spacing:initial; letter-spacing:initial; background-color:#ffff00; }
        #terminal { white-space:pre; white-space:initial; line-height:24px; line-height:initial; text-transform:uppercase; text-transform:initial; font-weight:bold; font-weight:initial; font-style:italic; font-style:initial; word-break:break-all; word-break:initial; vertical-align:middle; vertical-align:initial; word-spacing:4px; word-spacing:initial; letter-spacing:2px; letter-spacing:initial; background-color:#ff00ff; }
        </style>
        <div id='parent'><button id='inherit' class='probe'>inherit</button><button id='unset' class='probe'>unset</button><button id='revert' class='probe'>revert</button><button id='initial' class='probe'>initial</button><button id='terminal' class='probe'>terminal</button></div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 192,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let unset = document.resolve_target("id=unset").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let terminal = document.resolve_target("id=terminal").unwrap();
    let targets = [inherit, unset, revert, initial, terminal];

    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "white-space"
                    | "line-height"
                    | "text-transform"
                    | "font-weight"
                    | "font-style"
                    | "word-break"
                    | "vertical-align"
                    | "word-spacing"
                    | "letter-spacing"
            ))
    }));

    let layout = document.layout(viewport).unwrap();
    assert!(layout.box_for(parent).is_some());
    for target in targets {
        assert!(layout.box_for(target).is_some());
        assert!(layout.text_runs.iter().any(|run| run.node_id == target));
    }
    let text_for = |node_id| {
        layout
            .text_runs
            .iter()
            .filter(|run| run.node_id == node_id)
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(text_for(inherit), vec!["INHERIT"]);
    assert_eq!(text_for(unset), vec!["UNSET"]);
    assert_eq!(text_for(revert), vec!["REVERT"]);
    assert_eq!(text_for(initial), vec!["initial"]);
    assert_eq!(text_for(terminal), vec!["terminal"]);
    assert_eq!(layout.box_for(inherit).unwrap().height, 24);
    assert_eq!(layout.box_for(unset).unwrap().height, 24);
    assert_eq!(layout.box_for(revert).unwrap().height, 24);
    assert_eq!(layout.box_for(initial).unwrap().height, 20);
    assert_eq!(layout.box_for(terminal).unwrap().height, 20);

    let list = document.display_list(viewport).unwrap();
    let text_style_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node,
                bold,
                italic,
                word_spacing,
                letter_spacing,
                ..
            } if *command_node == node_id => Some((*bold, *italic, *word_spacing, *letter_spacing)),
            _ => None,
        })
    };
    assert_eq!(text_style_for(inherit), Some((true, true, 4, 2)));
    assert_eq!(text_style_for(unset), Some((true, true, 4, 2)));
    assert_eq!(text_style_for(revert), Some((true, true, 4, 2)));
    assert_eq!(text_style_for(initial), Some((false, false, 0, 0)));
    assert_eq!(text_style_for(terminal), Some((false, false, 0, 0)));

    let surface = list.rasterize().unwrap();
    for (target, color) in [
        (inherit, [255, 0, 0, 255]),
        (unset, [0, 255, 0, 255]),
        (revert, [0, 0, 255, 255]),
        (initial, [255, 255, 0, 255]),
        (terminal, [255, 0, 255, 255]),
    ] {
        let rect = layout.box_for(target).unwrap();
        assert_eq!(
            layout
                .hit_test(i64::from(rect.x + 1), i64::from(rect.y + 1))
                .unwrap(),
            Some(target)
        );
        assert_eq!(surface.pixel(rect.x + 1, rect.y + 1), Some(color));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id, .. } if *node_id == target
            )
        }));
    }
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(
        document
            .semantic_nodes()
            .iter()
            .filter(|semantic_node| targets.contains(&semantic_node.node_id))
            .count()
            >= targets.len()
    );
}

#[test]
fn native_svg_lines_and_polygons_share_layout_paint_and_hit_test_geometry() {
    let document = NativeDocument::parse(
        "<div style='width:32px;height:24px'><svg width='28' height='20'><line id='line' x1='2' y1='2' x2='10' y2='2' stroke='black' stroke-width='2'></line><polyline id='polyline' points='2,8 8,12 14,8' fill='none' stroke='red' stroke-width='2'></polyline><polygon id='polygon' points='18,2 26,2 22,8' fill='blue' stroke='green' stroke-width='1'></polygon></svg></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let line = document.resolve_target("id=line").unwrap();
    let polyline = document.resolve_target("id=polyline").unwrap();
    let polygon = document.resolve_target("id=polygon").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(line).unwrap(),
        NativeRect {
            x: 2,
            y: 2,
            width: 9,
            height: 1,
        }
    );
    assert_eq!(
        layout.box_for(polyline).unwrap(),
        NativeRect {
            x: 2,
            y: 8,
            width: 13,
            height: 5,
        }
    );
    assert_eq!(
        layout.box_for(polygon).unwrap(),
        NativeRect {
            x: 18,
            y: 2,
            width: 9,
            height: 7,
        }
    );

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolyline {
                node_id,
                rect,
                points,
                closed,
                width,
                color,
                ..
            } if *node_id == line
                && *rect == NativeRect { x: 2, y: 2, width: 9, height: 1 }
                && points == &[NativePoint { x: 2, y: 2 }, NativePoint { x: 10, y: 2 }]
                && !*closed
                && *width == 2
                && *color == NativeColor::BLACK
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolyline { node_id, closed, width, color, .. }
                if *node_id == polyline
                    && !*closed
                    && *width == 2
                    && *color == NativeColor { red: 255, green: 0, blue: 0, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolygonFill { node_id, points, color, .. }
                if *node_id == polygon
                    && points.len() == 3
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::SvgPolyline { node_id, closed, width, color, .. }
                if *node_id == polygon
                    && *closed
                    && *width == 1
                    && *color == NativeColor { red: 0, green: 128, blue: 0, alpha: 255 }
        )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(6, 2), Some([0, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 10), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(22, 2), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(22, 4), Some([0, 0, 255, 255]));
    assert_eq!(layout.hit_test(6, 2).unwrap(), Some(line));
    assert_eq!(layout.hit_test(22, 4).unwrap(), Some(polygon));
}

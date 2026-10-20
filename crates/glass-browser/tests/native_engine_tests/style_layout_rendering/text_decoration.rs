use super::*;

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
fn native_text_decoration_current_color_reaches_separate_text_artifacts() {
    let document = NativeDocument::parse(
        "<style>.line { display:block;width:64px;height:20px;line-height:20px;text-decoration:underline overline line-through; } @layer base { #local { color:red;text-decoration-color:currentColor; } #parent { display:block;width:64px;height:20px;color:green;text-decoration:underline overline line-through;text-decoration-color:red; } #black { text-decoration-color:CuRrEnTcOlOr; } #named { color:green;text-decoration-color:red; } #rollback { color:blue;text-decoration-color:red; } #invalid { color:blue;text-decoration-color:red; } #transparent { color:green;text-decoration-color:transparent; } #omitted { color:green; } } @layer theme { #named { text-decoration-color:blue; } #rollback { text-decoration-color:currentColor; } #invalid { text-decoration-color:currentColor;text-decoration-color:invalid; } } #named { text-decoration-color:revert-layer; } #rollback { text-decoration-color:revert-layer; }</style><button id='local' class='line'>Local</button><div id='parent'><button id='inherited' class='line'>Inherited</button></div><button id='black' class='line'>Black</button><button id='named' class='line'>Named</button><button id='rollback' class='line'>Rollback</button><button id='invalid' class='line'>Invalid</button><button id='inline' class='line' style='color:red;text-decoration-color:currentColor;opacity:50%'>Inline</button><button id='transparent' class='line'>Transparent</button><button id='omitted' class='line'>Omitted</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 220,
        device_scale_factor_milli: 1000,
    };
    let local = document.resolve_target("id=local").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let black = document.resolve_target("id=black").unwrap();
    let named = document.resolve_target("id=named").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let transparent = document.resolve_target("id=transparent").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();

    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let transparent_color = NativeColor {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    };

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(local));
    assert_eq!(layout.hit_test(1, 21).unwrap(), Some(inherited));

    let list = document.display_list(viewport).unwrap();
    let command_for = |node_id| {
        list.commands
            .iter()
            .find_map(|command| match command {
                NativeDisplayCommand::TextRun {
                    node_id: command_node_id,
                    origin,
                    color,
                    decoration_color,
                    underline,
                    overline,
                    line_through,
                    clip,
                    ..
                } if *command_node_id == node_id => Some((
                    *origin,
                    *color,
                    *decoration_color,
                    *underline,
                    *overline,
                    *line_through,
                    *clip,
                )),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text command for {node_id:?}"))
    };
    assert_eq!(command_for(local).2, NativeColor::RED);
    assert_eq!(command_for(inherited).2, green);
    assert_eq!(command_for(black).2, NativeColor::BLACK);
    assert_eq!(command_for(named).2, blue);
    assert_eq!(command_for(rollback).2, blue);
    assert_eq!(command_for(invalid).2, blue);
    assert_eq!(command_for(inline).1, NativeColor::RED);
    assert_eq!(command_for(inline).2, NativeColor::RED);
    assert_eq!(command_for(transparent).2, transparent_color);
    assert_eq!(command_for(omitted).2, green);
    for node_id in [
        local,
        inherited,
        black,
        named,
        rollback,
        invalid,
        inline,
        transparent,
        omitted,
    ] {
        let command = command_for(node_id);
        assert!(command.3 && command.4 && command.5);
        assert_eq!(command.6, None);
    }
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BeginOpacityGroup { node_id, opacity }
                if *node_id == inline && *opacity == 128
        )
    }));

    let inline_command = command_for(inline);
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(command_for(local).0.x, command_for(local).0.y + 7),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(command_for(named).0.x, command_for(named).0.y + 7),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        surface.pixel(inline_command.0.x, inline_command.0.y + 7),
        Some([255, 127, 127, 255]),
        "inline command: {inline_command:?}"
    );
    assert_eq!(
        surface.pixel(
            command_for(transparent).0.x,
            command_for(transparent).0.y + 7
        ),
        Some([255, 255, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [
        (local, inherited),
        (inherited, black),
        (black, named),
        (named, rollback),
        (rollback, invalid),
        (invalid, inline),
        (inline, transparent),
        (transparent, omitted),
    ] {
        assert!(
            semantic_ids.iter().position(|id| *id == pair.0)
                < semantic_ids.iter().position(|id| *id == pair.1),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-color"
    }));
}
#[test]
fn native_text_decoration_color_css_wide_keywords_preserve_effective_parent_and_reset_colors() {
    let document = NativeDocument::parse(
        "<style>.line { display:block;width:80px;height:20px;line-height:20px;text-decoration:underline overline line-through; } #parent { display:block;width:96px;color:green;text-decoration:underline overline line-through;text-decoration-color:red; } #invalid { text-decoration-color:red;text-decoration-color:InHeRiT;text-decoration-color:invalid; } #inline { text-decoration-color:red; } @layer base { #named { text-decoration-color:red; } #rollback { text-decoration-color:red; } } @layer theme { .named { text-decoration-color:blue; } #rollback { text-decoration-color:blue; } } #named { text-decoration-color:revert-layer; } #rollback { text-decoration-color:revert-layer; }</style><div id='parent'><button id='inherit' class='line' style='text-decoration-color:inherit'>Inherit</button><button id='unset' class='line' style='text-decoration-color:UnSeT'>Unset</button><button id='initial' class='line' style='text-decoration-color:initial'>Initial</button><button id='revert' class='line' style='text-decoration-color:revert'>Revert</button><button id='current' class='line' style='text-decoration-color:CuRrEnTcOlOr'>Current</button><button id='invalid' class='line'>Invalid</button><button id='omitted' class='line'>Omitted</button><button id='precedence' class='line' style='text-decoration-color:inherit'>Precedence</button><button id='inline' class='line' style='color:green;text-decoration-color:blue'>Inline</button><button id='named' class='line named'>Named</button><button id='rollback' class='line'>Rollback</button></div><button id='root-inherit' class='line' style='color:red;text-decoration-color:inherit'>Root</button><div id='blue-parent' style='display:block;width:96px;color:blue;text-decoration:underline'><button id='blue-inherit' class='line' style='text-decoration-color:inherit'>Blue</button></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 112,
        height: 320,
        device_scale_factor_milli: 1000,
    };
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
    let command_for = |id| {
        let node_id = node_for(id);
        document
            .display_list(viewport)
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::TextRun {
                    node_id: command_node_id,
                    origin,
                    color,
                    decoration_color,
                    underline,
                    overline,
                    line_through,
                    ..
                } if command_node_id == node_id => Some((
                    origin,
                    color,
                    decoration_color,
                    underline,
                    overline,
                    line_through,
                )),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text command for {id}"))
    };

    assert_eq!(command_for("inherit").1, green);
    assert_eq!(command_for("inherit").2, NativeColor::RED);
    for id in ["unset", "initial", "revert", "current", "omitted"] {
        assert_eq!(command_for(id).1, green, "glyph color for {id}");
        assert_eq!(command_for(id).2, green, "decoration color for {id}");
    }
    assert_eq!(command_for("precedence").1, green);
    assert_eq!(command_for("precedence").2, NativeColor::RED);
    assert_eq!(command_for("invalid").2, NativeColor::RED);
    assert_eq!(command_for("inline").1, green);
    assert_eq!(command_for("inline").2, blue);
    assert_eq!(command_for("named").2, blue);
    assert_eq!(command_for("rollback").2, blue);
    assert_eq!(command_for("root-inherit").1, NativeColor::RED);
    assert_eq!(command_for("root-inherit").2, NativeColor::BLACK);
    assert_eq!(command_for("blue-inherit").1, blue);
    assert_eq!(command_for("blue-inherit").2, blue);
    for id in [
        "inherit",
        "unset",
        "initial",
        "revert",
        "current",
        "invalid",
        "omitted",
        "precedence",
        "inline",
        "named",
        "rollback",
        "root-inherit",
        "blue-inherit",
    ] {
        let command = command_for(id);
        assert!(
            command.3 && command.4 && command.5,
            "decoration flags for {id}"
        );
    }

    let layout = document.layout(viewport).unwrap();
    for id in ["inherit", "root-inherit", "blue-inherit"] {
        let node_id = node_for(id);
        let rect = layout.box_for(node_id).unwrap();
        assert_eq!(
            layout.hit_test(i64::from(rect.x) + 1, i64::from(rect.y) + 1),
            Ok(Some(node_id))
        );
    }
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let inherit = command_for("inherit");
    let named = command_for("named");
    let root_inherit = command_for("root-inherit");
    assert_eq!(
        surface.pixel(inherit.0.x, inherit.0.y + 7),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(named.0.x, named.0.y + 7),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        surface.pixel(root_inherit.0.x, root_inherit.0.y + 7),
        Some([0, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [
        ("inherit", "unset"),
        ("unset", "initial"),
        ("initial", "revert"),
        ("revert", "current"),
        ("current", "invalid"),
        ("invalid", "omitted"),
        ("omitted", "precedence"),
        ("precedence", "inline"),
        ("inline", "named"),
        ("named", "rollback"),
    ] {
        let first = node_for(pair.0);
        let second = node_for(pair.1);
        assert!(
            semantic_ids.iter().position(|id| *id == first)
                < semantic_ids.iter().position(|id| *id == second),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == "text-decoration-color"
            })
            .count(),
        1
    );
}
#[test]
fn native_text_decoration_color_revert_layer_preserves_local_fallback_and_glyph_paint() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:green; text-decoration:underline overline line-through; } @layer base { #named { text-decoration-color:red; } #rollback { text-decoration-color:red; } #repeated { text-decoration-color:red; } #unlayered { text-decoration-color:red; } #inline { text-decoration-color:red; } #fallback { text-decoration-color:revert-layer; } } @layer theme { .named { text-decoration-color:blue; } #rollback { text-decoration-color:blue; } #repeated { text-decoration-color:revert-layer; } #unlayered { text-decoration-color:blue; } #inline { text-decoration-color:blue; } } @layer top { #repeated { text-decoration-color:revert-layer; } } #named { text-decoration-color:revert-layer; } #unlayered { text-decoration-color:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='text-decoration-color:revert-layer'>A</div><div id='fallback' class='line'>A</div><div id='parent' class='line' style='text-decoration-color:red'><span id='child'>A</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let command_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands
            .iter()
            .find_map(|command| match command {
                NativeDisplayCommand::TextRun {
                    node_id: command_node_id,
                    origin,
                    color,
                    decoration_color,
                    underline,
                    overline,
                    line_through,
                    ..
                } if *command_node_id == node_id => Some((
                    *origin,
                    *color,
                    *decoration_color,
                    *underline,
                    *overline,
                    *line_through,
                )),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text command for {id}"))
    };
    let named = command_for("named");
    let rollback = command_for("rollback");
    let repeated = command_for("repeated");
    let unlayered = command_for("unlayered");
    let inline = command_for("inline");
    let fallback = command_for("fallback");
    let child = command_for("child");

    assert_eq!(named.1, green);
    assert_eq!(named.2, blue);
    assert_eq!(rollback.2, blue);
    assert_eq!(repeated.2, NativeColor::RED);
    assert_eq!(unlayered.2, blue);
    assert_eq!(inline.2, blue);
    assert_eq!(fallback.2, green);
    assert_eq!(child.2, green);
    for command in [
        named, rollback, repeated, unlayered, inline, fallback, child,
    ] {
        assert!(command.3 && command.4 && command.5);
        assert_eq!(
            surface.pixel(command.0.x.saturating_add(1), command.0.y),
            Some([0, 128, 0, 255])
        );
    }
    assert_eq!(
        surface.pixel(named.0.x, named.0.y.saturating_add(7)),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        surface.pixel(repeated.0.x, repeated.0.y.saturating_add(7)),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(fallback.0.x, fallback.0.y.saturating_add(7)),
        Some([0, 128, 0, 255])
    );
}
#[test]
fn native_text_decoration_line_revert_layer_preserves_shared_line_owner() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:green; text-decoration-style:dashed; text-decoration-thickness:2px; text-underline-offset:1px; text-decoration-color:blue; } @layer base { #named { text-decoration-line:underline; } #rollback { text-decoration:underline; } #repeated { text-decoration:underline; } #unlayered { text-decoration:underline; } #inline { text-decoration:underline; } #fallback { text-decoration-line:revert-layer; } } @layer theme { .named { text-decoration-line:overline; } #rollback { text-decoration-line:revert-layer; } #repeated { text-decoration-line:revert-layer; } #unlayered { text-decoration-line:overline; } #inline { text-decoration-line:overline; } } @layer top { #repeated { text-decoration-line:revert-layer; } } #named { text-decoration-line:revert-layer; } #unlayered { text-decoration-line:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='text-decoration-line:revert-layer'>A</div><div id='fallback' class='line'>A</div><div id='parent' class='line' style='text-decoration:line-through'><span id='child' style='text-decoration-line:revert-layer'>A</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let blue = [0, 0, 255, 255];
    let green = [0, 128, 0, 255];
    let white = [255, 255, 255, 255];
    let command_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands
            .iter()
            .find_map(|command| match command {
                NativeDisplayCommand::TextRun {
                    node_id: command_node_id,
                    origin,
                    color,
                    decoration_color,
                    decoration_style,
                    decoration_thickness,
                    underline_offset,
                    underline,
                    overline,
                    line_through,
                    ..
                } if *command_node_id == node_id => Some((
                    *origin,
                    *color,
                    *decoration_color,
                    *decoration_style,
                    *decoration_thickness,
                    *underline_offset,
                    *underline,
                    *overline,
                    *line_through,
                )),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text command for {id}"))
    };
    let named = command_for("named");
    let rollback = command_for("rollback");
    let repeated = command_for("repeated");
    let unlayered = command_for("unlayered");
    let inline = command_for("inline");
    let fallback = command_for("fallback");
    let child = command_for("child");

    assert_eq!(
        (named.6, named.7, named.8),
        (false, true, false),
        "named candidate rolls back from unlayered to theme"
    );
    assert_eq!((rollback.6, rollback.7, rollback.8), (true, false, false));
    assert_eq!((repeated.6, repeated.7, repeated.8), (true, false, false));
    assert_eq!(
        (unlayered.6, unlayered.7, unlayered.8),
        (false, true, false)
    );
    assert_eq!((inline.6, inline.7, inline.8), (false, true, false));
    assert_eq!((fallback.6, fallback.7, fallback.8), (false, false, false));
    assert_eq!((child.6, child.7, child.8), (false, false, true));

    for command in [named, rollback, repeated, unlayered, inline, fallback] {
        assert_eq!(
            command.1,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            }
        );
        assert_eq!(
            command.2,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            }
        );
        assert_eq!(command.3, NativeTextDecorationStyle::Dashed);
        assert_eq!(command.4, 2);
        assert_eq!(command.5, 1);
        assert_eq!(
            surface.pixel(command.0.x.saturating_add(1), command.0.y),
            Some(green)
        );
    }
    assert_eq!(
        child.1,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }
    );
    assert_eq!(
        child.2,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }
    );
    assert_eq!(child.3, NativeTextDecorationStyle::Dashed);
    assert_eq!(child.4, 2);
    assert_eq!(child.5, 1);
    assert_eq!(
        surface.pixel(
            rollback.0.x.saturating_add(2),
            rollback.0.y.saturating_add(8)
        ),
        Some(blue)
    );
    assert_eq!(
        surface.pixel(
            repeated.0.x.saturating_add(2),
            repeated.0.y.saturating_add(8)
        ),
        Some(blue)
    );
    assert_eq!(
        surface.pixel(inline.0.x.saturating_add(2), inline.0.y.saturating_sub(1)),
        Some(blue)
    );
    assert_eq!(
        surface.pixel(
            fallback.0.x.saturating_add(2),
            fallback.0.y.saturating_add(8)
        ),
        Some(white)
    );
    assert_eq!(
        surface.pixel(
            unlayered.0.x.saturating_add(2),
            unlayered.0.y.saturating_sub(1)
        ),
        Some(blue)
    );
    assert_eq!(
        surface.pixel(child.0.x.saturating_add(2), child.0.y.saturating_add(3)),
        Some(green)
    );
}
#[test]
fn native_text_decoration_line_css_wide_resets_reach_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:72px; height:20px; line-height:20px; color:black; text-decoration-style:solid; text-decoration-thickness:1px; } #parent { text-decoration-line:underline overline line-through; } #initial { text-decoration-line:initial; } #inherit { text-decoration-line:inherit; } #unset { text-decoration-line:unset; } #revert { text-decoration-line:revert; } #invalid { text-decoration-line:overline; text-decoration-line:invalid; } #important { text-decoration-line:underline !important; } #important { text-decoration-line:none; } #shorthand-inherit { text-decoration:inherit; } #shorthand-initial { text-decoration:initial; }</style><div id='parent' class='line'>P<span id='initial' class='line'>A</span><span id='inherit' class='line'>B</span><span id='unset' class='line'>C</span><span id='revert' class='line'>D</span><span id='invalid' class='line'>E</span><span id='important' class='line'>F</span><span id='shorthand-inherit' class='line'>G</span><span id='shorthand-initial' class='line'>H</span></div><div id='root-initial' class='line' style='text-decoration-line:initial'>Root</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 96,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let command_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands
            .iter()
            .find_map(|command| match command {
                NativeDisplayCommand::TextRun {
                    node_id: command_node_id,
                    origin,
                    underline,
                    overline,
                    line_through,
                    ..
                } if *command_node_id == node_id => {
                    Some((*origin, *underline, *overline, *line_through))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text command for {id}"))
    };
    let parent = command_for("parent");
    assert_eq!((parent.1, parent.2, parent.3), (true, true, true));
    for id in ["inherit", "unset", "revert", "shorthand-inherit"] {
        assert_eq!(
            (command_for(id).1, command_for(id).2, command_for(id).3),
            (true, true, true),
            "inherited line flags for {id}"
        );
    }
    for id in ["initial", "shorthand-initial", "root-initial"] {
        assert_eq!(
            (command_for(id).1, command_for(id).2, command_for(id).3),
            (false, false, false),
            "initial line flags for {id}"
        );
    }
    assert_eq!(
        (
            command_for("invalid").1,
            command_for("invalid").2,
            command_for("invalid").3
        ),
        (false, true, false)
    );
    assert_eq!(
        (
            command_for("important").1,
            command_for("important").2,
            command_for("important").3
        ),
        (true, false, false)
    );

    let surface = list.rasterize().unwrap();
    assert!(
        surface
            .pixel(parent.0.x, parent.0.y.saturating_add(7))
            .is_some()
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-line"
    }));
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
fn native_text_decoration_style_revert_layer_reaches_all_pattern_owners() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; text-decoration:underline; text-decoration-thickness:1px; } @layer base { #named { text-decoration-style:dashed; } #rollback { text-decoration-style:solid; } #repeated { text-decoration-style:dashed; } #unlayered { text-decoration-style:dotted; } #inline { text-decoration-style:solid; } #fallback { text-decoration-style:revert-layer; } } @layer theme { .named { text-decoration-style:dotted; } #rollback { text-decoration-style:double; } #repeated { text-decoration-style:revert-layer; } #unlayered { text-decoration-style:wavy; } #inline { text-decoration-style:dotted; } } @layer top { #repeated { text-decoration-style:revert-layer; } } #named { text-decoration-style:revert-layer; } #unlayered { text-decoration-style:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='text-decoration-style:revert-layer'>A</div><div id='fallback' class='line'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 132,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
    let command_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_style,
                ..
            } if *command_node_id == node_id => Some((*origin, *decoration_style)),
            _ => None,
        })
    };
    let named = command_for(node_for("named")).expect("named text command");
    let rollback = command_for(node_for("rollback")).expect("rollback text command");
    let repeated = command_for(node_for("repeated")).expect("repeated text command");
    let unlayered = command_for(node_for("unlayered")).expect("unlayered text command");
    let inline = command_for(node_for("inline")).expect("inline text command");
    let fallback = command_for(node_for("fallback")).expect("fallback text command");
    assert_eq!(named.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(rollback.1, NativeTextDecorationStyle::Double);
    assert_eq!(repeated.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(unlayered.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(inline.1, NativeTextDecorationStyle::Dotted);
    assert_eq!(fallback.1, NativeTextDecorationStyle::Solid);

    assert_eq!(
        surface.pixel(named.0.x, named.0.y + 7),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(named.0.x + 1, named.0.y + 7),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(rollback.0.x, rollback.0.y + 7),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(rollback.0.x, rollback.0.y + 9),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(repeated.0.x + 3, repeated.0.y + 7),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(unlayered.0.x + 2, unlayered.0.y + 7),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(inline.0.x + 1, inline.0.y + 7),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(fallback.0.x, fallback.0.y + 7),
        Some([0, 0, 0, 255])
    );
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-style"
    }));
}
#[test]
fn native_text_decoration_style_css_wide_resets_reach_inheritance_and_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; text-decoration:underline; text-decoration-style:dotted; } #parent { text-decoration-style:dashed; } #initial { text-decoration-style:initial; } #inherit { text-decoration-style:inherit; } #unset { text-decoration-style:unset; } #revert { text-decoration-style:revert; } #invalid { text-decoration-style:wavy; text-decoration-style:zigzag; } #important { text-decoration-style:dashed !important; } #important { text-decoration-style:solid; } </style><div id='parent' class='line'><span id='initial' class='line'>A</span><span id='inherit' class='line'>B</span><span id='unset' class='line'>C</span><span id='revert' class='line'>D</span><span id='invalid' class='line'>E</span><span id='important' class='line'>F</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let command_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
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
    let initial = command_for("initial").expect("initial text command");
    let inherited = command_for("inherit").expect("inherit text command");
    let unset = command_for("unset").expect("unset text command");
    let revert = command_for("revert").expect("revert text command");
    let invalid = command_for("invalid").expect("invalid text command");
    let important = command_for("important").expect("important text command");
    assert_eq!(initial.1, NativeTextDecorationStyle::Solid);
    assert_eq!(inherited.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(unset.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(revert.1, NativeTextDecorationStyle::Dashed);
    assert_eq!(invalid.1, NativeTextDecorationStyle::Wavy);
    assert_eq!(important.1, NativeTextDecorationStyle::Dashed);
    assert!(initial.2 && inherited.2 && important.2);

    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(initial.0.x, initial.0.y + 7),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(inherited.0.x + 3, inherited.0.y + 7),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(important.0.x + 3, important.0.y + 7),
        Some([255, 255, 255, 255])
    );
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-style"
    }));
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
fn native_text_decoration_skip_ink_css_wide_resets_reach_intersection_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; text-decoration-color:red; text-decoration:underline; text-decoration-style:wavy; text-underline-offset:-1px; font-weight:bold; font-style:italic; } #parent { text-decoration-skip-ink:none; } #initial { text-decoration-skip-ink:initial; } #inherit { text-decoration-skip-ink:inherit; } #unset { text-decoration-skip-ink:unset; } #revert { text-decoration-skip-ink:revert; } #invalid { text-decoration-skip-ink:none; text-decoration-skip-ink:all; } #important { text-decoration-skip-ink:none !important; } #important { text-decoration-skip-ink:initial; }</style><div id='parent' class='line'><span id='initial' class='line'>A</span><span id='inherit' class='line'>B</span><span id='unset' class='line'>C</span><span id='revert' class='line'>D</span><span id='invalid' class='line'>E</span><span id='important' class='line'>F</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let command_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_skip_ink,
                ..
            } if *command_node_id == node_id => Some((*origin, *decoration_skip_ink)),
            _ => None,
        })
    };
    let initial = command_for("initial").expect("initial skip-ink command");
    let inherited = command_for("inherit").expect("inherit skip-ink command");
    let unset = command_for("unset").expect("unset skip-ink command");
    let revert = command_for("revert").expect("revert skip-ink command");
    let invalid = command_for("invalid").expect("invalid skip-ink command");
    let important = command_for("important").expect("important skip-ink command");
    assert_eq!(initial.1, NativeTextDecorationSkipInk::Auto);
    assert_eq!(inherited.1, NativeTextDecorationSkipInk::None);
    assert_eq!(unset.1, NativeTextDecorationSkipInk::None);
    assert_eq!(revert.1, NativeTextDecorationSkipInk::None);
    assert_eq!(invalid.1, NativeTextDecorationSkipInk::None);
    assert_eq!(important.1, NativeTextDecorationSkipInk::None);

    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(initial.0.x + 4, initial.0.y + 6),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(inherited.0.x + 4, inherited.0.y + 6),
        Some([255, 0, 0, 255])
    );
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-ink"
    }));
}
#[test]
fn native_text_decoration_skip_ink_revert_layer_reuses_layers_and_reaches_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:black; text-decoration-color:red; text-decoration:underline; text-decoration-style:wavy; text-underline-offset:-1px; font-weight:bold; font-style:italic; } @layer base { #named { text-decoration-skip-ink:none; } #rollback { text-decoration-skip-ink:auto; } #inline { text-decoration-skip-ink:auto; } } @layer theme { .named { text-decoration-skip-ink:auto; } #rollback { text-decoration-skip-ink:none; } #inline { text-decoration-skip-ink:none; } } #named { text-decoration-skip-ink:revert-layer; } #rollback { text-decoration-skip-ink:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='inline' class='line' style='text-decoration-skip-ink:revert-layer'>A</div><div id='fallback' class='line'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let list = document
        .display_list(Viewport {
            width: 32,
            height: 100,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
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
    let named = node_for("named");
    let rollback = node_for("rollback");
    let inline = node_for("inline");
    let fallback = node_for("fallback");
    assert_eq!(skip_ink_for(named), Some(NativeTextDecorationSkipInk::Auto));
    assert_eq!(
        skip_ink_for(rollback),
        Some(NativeTextDecorationSkipInk::None)
    );
    assert_eq!(
        skip_ink_for(inline),
        Some(NativeTextDecorationSkipInk::None)
    );
    assert_eq!(
        skip_ink_for(fallback),
        Some(NativeTextDecorationSkipInk::Auto)
    );

    let surface = list.rasterize().unwrap();
    let run_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                ..
            } if *command_node_id == node_id => Some(*origin),
            _ => None,
        })
    };
    let auto_origin = run_for(named).unwrap();
    let none_origin = run_for(rollback).unwrap();
    assert_eq!(
        surface.pixel(auto_origin.x + 4, auto_origin.y + 6),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(none_origin.x + 4, none_origin.y + 6),
        Some([255, 0, 0, 255])
    );
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-ink"
    }));
}
#[test]
fn native_text_decoration_skip_spaces_cascades_to_display_commands() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; text-decoration:underline; } #all { text-decoration-skip-spaces:ALL; } #parent { text-decoration-skip-spaces:none; } #invalid { text-decoration-skip-spaces:start start; }</style><div id='default' class='line'>A B</div><div id='all' class='line'>A B</div><div id='parent' class='line'><span id='inherited'>A B</span></div><div id='invalid' class='line'>A B</div><div id='inline' class='line' style='text-decoration-skip-spaces:all'>A B</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let default = document.resolve_target("id=default").unwrap();
    let all = document.resolve_target("id=all").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let list = document
        .display_list(Viewport {
            width: 40,
            height: 120,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let skip_spaces_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                decoration_skip_spaces,
                ..
            } if *command_node_id == node_id => Some(*decoration_skip_spaces),
            _ => None,
        })
    };

    assert_eq!(
        skip_spaces_for(default),
        Some(NativeTextDecorationSkipSpaces::None)
    );
    assert_eq!(
        skip_spaces_for(all),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(inherited),
        Some(NativeTextDecorationSkipSpaces::None)
    );
    assert_eq!(
        skip_spaces_for(invalid),
        Some(NativeTextDecorationSkipSpaces::None)
    );
    assert_eq!(
        skip_spaces_for(inline),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-spaces"
    }));
}
#[test]
fn native_text_decoration_skip_spaces_marks_line_edges_in_layout_and_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; white-space:pre; text-decoration:underline overline line-through; text-decoration-skip-ink:none; } #start { text-decoration-skip-spaces:start; } #end { text-decoration-skip-spaces:end; } #both { text-decoration-skip-spaces:start end; } #none { text-decoration-skip-spaces:none; } #all { text-decoration-skip-spaces:all; }</style><div id='start' class='line'> A </div><div id='end' class='line'> A </div><div id='both' class='line'> A </div><div id='none' class='line'> A </div><div id='all' class='line'> A </div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let white = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    let black = [0, 0, 0, u8::MAX];
    let run_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
    };
    let start = run_for("start");
    let end = run_for("end");
    let both = run_for("both");
    let none = run_for("none");
    let all = run_for("all");
    assert!(start.starts_line && start.ends_line);
    assert!(end.starts_line && end.ends_line);
    assert!(both.starts_line && both.ends_line);
    assert!(none.starts_line && none.ends_line);
    assert!(all.starts_line && all.ends_line);

    let assert_mode = |run: &glass_browser::browser::native_engine::NativeTextLayout,
                       expected: [bool; 3]| {
        for (label, row) in [
            ("overline", run.origin.y.checked_sub(1)),
            ("line-through", run.origin.y.checked_add(3)),
            ("underline", run.origin.y.checked_add(7)),
        ] {
            let Some(row) = row else { continue };
            for (x, expected_space) in [(0, expected[0]), (6, expected[1]), (12, expected[2])] {
                let pixel = surface.pixel(x, row).unwrap();
                let expected_color = if expected_space { white } else { black };
                assert_eq!(pixel, expected_color, "{label}, x {x}");
            }
        }
    };
    assert_mode(start, [true, false, false]);
    assert_mode(end, [false, false, true]);
    assert_mode(both, [true, false, true]);
    assert_mode(none, [false, false, false]);
    assert_mode(all, [true, false, true]);
}
#[test]
fn native_text_decoration_skip_spaces_uses_unicode_whitespace_at_line_edges() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; white-space:pre; text-decoration:underline overline line-through; text-decoration-skip-ink:none; } #start { text-decoration-skip-spaces:start; } #end { text-decoration-skip-spaces:end; } #both { text-decoration-skip-spaces:start end; } #none { text-decoration-skip-spaces:none; } #all { text-decoration-skip-spaces:all; }</style><div id='start' class='line'>\tA\u{00a0}</div><div id='end' class='line'>\tA\u{00a0}</div><div id='both' class='line'>\tA\u{00a0}</div><div id='none' class='line'>\tA\u{00a0}</div><div id='all' class='line'>\tA\u{00a0}</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let white = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    let black = [0, 0, 0, u8::MAX];
    let run_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
    };
    let start = run_for("start");
    let end = run_for("end");
    let both = run_for("both");
    let none = run_for("none");
    let all = run_for("all");

    for run in [start, end, both, none, all] {
        assert_eq!(run.text, "\tA\u{00a0}");
        assert!(run.starts_line && run.ends_line);
    }
    let assert_mode = |run: &glass_browser::browser::native_engine::NativeTextLayout,
                       expected: [bool; 2]| {
        for row in [run.origin.y + 3, run.origin.y + 7] {
            for (x, expected_space) in [(0, expected[0]), (8, false), (16, expected[1])] {
                let pixel = surface.pixel(x, row).unwrap();
                let expected_color = if expected_space { white } else { black };
                assert_eq!(pixel, expected_color, "x {x}, row {row}");
            }
        }
    };
    assert_mode(start, [true, false]);
    assert_mode(end, [false, true]);
    assert_mode(both, [true, true]);
    assert_mode(none, [false, false]);
    assert_mode(all, [true, true]);
}
#[test]
fn native_text_decoration_skip_spaces_initial_resets_to_both_edges() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; white-space:pre; text-decoration:underline overline line-through; text-decoration-skip-ink:none; } #parent { text-decoration-skip-spaces:all; } #initial { text-decoration-skip-spaces:INITIAL; } #none { text-decoration-skip-spaces:none; } #unset { text-decoration-skip-spaces:unset; } #revert { text-decoration-skip-spaces:revert; } #revert-layer { text-decoration-skip-spaces:revert-layer; }</style><div id='parent' class='line'><span id='inherited'>\tA\u{00a0}</span><span id='inline' style='text-decoration-skip-spaces:initial'>\tA\u{00a0}</span></div><div id='initial' class='line'>\tA\u{00a0}</div><div id='none' class='line'>\tA\u{00a0}</div><div id='unset' class='line'>\tA\u{00a0}</div><div id='revert' class='line'>\tA\u{00a0}</div><div id='revert-layer' class='line'>\tA\u{00a0}</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let white = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    let black = [0, 0, 0, u8::MAX];
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
    let run_for = |id| {
        let node_id = node_for(id);
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
    };
    let inherited = run_for("inherited");
    let inline = run_for("inline");
    let initial = run_for("initial");
    let none = run_for("none");
    let unset = run_for("unset");
    let revert = run_for("revert");
    let revert_layer = run_for("revert-layer");

    for run in [initial, none, unset, revert, revert_layer] {
        assert_eq!(run.text, "\tA\u{00a0}");
        assert!(run.starts_line && run.ends_line);
    }
    assert_eq!(inherited.text, "\tA\u{00a0}");
    assert_eq!(inline.text, "\tA\u{00a0}");
    let skip_spaces_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                decoration_skip_spaces,
                ..
            } if *command_node_id == node_id => Some(*decoration_skip_spaces),
            _ => None,
        })
    };
    assert_eq!(
        skip_spaces_for(node_for("inherited")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(node_for("inline")),
        Some(NativeTextDecorationSkipSpaces::StartAndEnd)
    );
    assert_eq!(
        skip_spaces_for(node_for("initial")),
        Some(NativeTextDecorationSkipSpaces::StartAndEnd)
    );
    assert_eq!(
        skip_spaces_for(node_for("none")),
        Some(NativeTextDecorationSkipSpaces::None)
    );
    assert_eq!(
        skip_spaces_for(node_for("unset")),
        Some(NativeTextDecorationSkipSpaces::None)
    );

    let assert_edges = |run: &glass_browser::browser::native_engine::NativeTextLayout,
                        expected_edges: [bool; 2]| {
        for row in [run.origin.y + 3, run.origin.y + 7] {
            for (x, expected_space) in [(0, expected_edges[0]), (8, false), (16, expected_edges[1])]
            {
                let pixel = surface.pixel(run.origin.x + x, row).unwrap();
                let expected_color = if expected_space { white } else { black };
                assert_eq!(pixel, expected_color, "x {}, row {row}", run.origin.x + x);
            }
        }
    };
    assert_edges(initial, [true, true]);
    assert_edges(none, [false, false]);
    assert_edges(unset, [false, false]);
    assert_edges(revert, [false, false]);
    assert_edges(revert_layer, [false, false]);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-spaces"
    }));
}
#[test]
fn native_text_decoration_skip_spaces_inherit_resolves_parent_value() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; white-space:pre; text-decoration:underline overline line-through; text-decoration-skip-ink:none; } .child { display:block; } #parent { text-decoration-skip-spaces:all; } #stylesheet-inherit { text-decoration-skip-spaces:INHERIT; } #inline-inherit { text-decoration-skip-spaces:none; }</style><div id='parent' class='line'><span id='stylesheet-inherit' class='child'>\tA\u{00a0}</span><span id='inline-inherit' class='child' style='text-decoration-skip-spaces:inherit'>\tA\u{00a0}</span><span id='explicit-none' class='child' style='text-decoration-skip-spaces:none'>\tA\u{00a0}</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let white = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    let black = [0, 0, 0, u8::MAX];
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
    let run_for = |id| {
        let node_id = node_for(id);
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
    };
    let stylesheet_inherit = run_for("stylesheet-inherit");
    let inline_inherit = run_for("inline-inherit");
    let explicit_none = run_for("explicit-none");
    for run in [stylesheet_inherit, inline_inherit, explicit_none] {
        assert_eq!(run.text, "\tA\u{00a0}");
        assert!(run.starts_line && run.ends_line);
    }
    let skip_spaces_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                decoration_skip_spaces,
                ..
            } if *command_node_id == node_id => Some(*decoration_skip_spaces),
            _ => None,
        })
    };
    assert_eq!(
        skip_spaces_for(node_for("stylesheet-inherit")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(node_for("inline-inherit")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(node_for("explicit-none")),
        Some(NativeTextDecorationSkipSpaces::None)
    );

    for run in [stylesheet_inherit, inline_inherit] {
        for row in [run.origin.y + 3, run.origin.y + 7] {
            assert_eq!(surface.pixel(run.origin.x, row), Some(white));
            assert_eq!(surface.pixel(run.origin.x + 8, row), Some(black));
            assert_eq!(surface.pixel(run.origin.x + 16, row), Some(white));
        }
    }
    for row in [explicit_none.origin.y + 3, explicit_none.origin.y + 7] {
        assert_eq!(surface.pixel(explicit_none.origin.x, row), Some(black));
        assert_eq!(surface.pixel(explicit_none.origin.x + 16, row), Some(black));
    }
}
#[test]
fn native_text_decoration_skip_spaces_unset_resolves_parent_value() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; white-space:pre; text-decoration:underline overline line-through; text-decoration-skip-ink:none; } .child { display:block; } #parent { text-decoration-skip-spaces:all; } #stylesheet-unset { text-decoration-skip-spaces:UNSET; } #inline-unset { text-decoration-skip-spaces:none; }</style><div id='parent' class='line'><span id='stylesheet-unset' class='child'>\tA\u{00a0}</span><span id='inline-unset' class='child' style='text-decoration-skip-spaces:unset'>\tA\u{00a0}</span><span id='explicit-none' class='child' style='text-decoration-skip-spaces:none'>\tA\u{00a0}</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
    let run_for = |id| {
        let node_id = node_for(id);
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
    };
    let stylesheet_unset = run_for("stylesheet-unset");
    let inline_unset = run_for("inline-unset");
    let explicit_none = run_for("explicit-none");
    for run in [stylesheet_unset, inline_unset, explicit_none] {
        assert_eq!(run.text, "\tA\u{00a0}");
        assert!(run.starts_line && run.ends_line);
    }
    let skip_spaces_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                decoration_skip_spaces,
                ..
            } if *command_node_id == node_id => Some(*decoration_skip_spaces),
            _ => None,
        })
    };
    assert_eq!(
        skip_spaces_for(node_for("stylesheet-unset")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(node_for("inline-unset")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(node_for("explicit-none")),
        Some(NativeTextDecorationSkipSpaces::None)
    );

    let white = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    let black = [0, 0, 0, u8::MAX];
    for run in [stylesheet_unset, inline_unset] {
        for row in [run.origin.y + 3, run.origin.y + 7] {
            assert_eq!(surface.pixel(run.origin.x, row), Some(white));
            assert_eq!(surface.pixel(run.origin.x + 8, row), Some(black));
            assert_eq!(surface.pixel(run.origin.x + 16, row), Some(white));
        }
    }
    for row in [explicit_none.origin.y + 3, explicit_none.origin.y + 7] {
        assert_eq!(surface.pixel(explicit_none.origin.x, row), Some(black));
        assert_eq!(surface.pixel(explicit_none.origin.x + 16, row), Some(black));
    }
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-spaces"
    }));
}
#[test]
fn native_text_decoration_skip_spaces_revert_resolves_parent_value() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; white-space:pre; text-decoration:underline overline line-through; text-decoration-skip-ink:none; } .child { display:block; } #parent { text-decoration-skip-spaces:all; } #stylesheet-revert { text-decoration-skip-spaces:ReVeRt; } #inline-revert { text-decoration-skip-spaces:none; }</style><div id='parent' class='line'><span id='stylesheet-revert' class='child'>\tA\u{00a0}</span><span id='inline-revert' class='child' style='text-decoration-skip-spaces:revert'>\tA\u{00a0}</span><span id='explicit-none' class='child' style='text-decoration-skip-spaces:none'>\tA\u{00a0}</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
    let run_for = |id| {
        let node_id = node_for(id);
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
    };
    let stylesheet_revert = run_for("stylesheet-revert");
    let inline_revert = run_for("inline-revert");
    let explicit_none = run_for("explicit-none");
    for run in [stylesheet_revert, inline_revert, explicit_none] {
        assert_eq!(run.text, "\tA\u{00a0}");
        assert!(run.starts_line && run.ends_line);
    }
    let skip_spaces_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                decoration_skip_spaces,
                ..
            } if *command_node_id == node_id => Some(*decoration_skip_spaces),
            _ => None,
        })
    };
    assert_eq!(
        skip_spaces_for(node_for("stylesheet-revert")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(node_for("inline-revert")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    assert_eq!(
        skip_spaces_for(node_for("explicit-none")),
        Some(NativeTextDecorationSkipSpaces::None)
    );

    let white = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    let black = [0, 0, 0, u8::MAX];
    for run in [stylesheet_revert, inline_revert] {
        for row in [run.origin.y + 3, run.origin.y + 7] {
            assert_eq!(surface.pixel(run.origin.x, row), Some(white));
            assert_eq!(surface.pixel(run.origin.x + 8, row), Some(black));
            assert_eq!(surface.pixel(run.origin.x + 16, row), Some(white));
        }
    }
    for row in [explicit_none.origin.y + 3, explicit_none.origin.y + 7] {
        assert_eq!(surface.pixel(explicit_none.origin.x, row), Some(black));
        assert_eq!(surface.pixel(explicit_none.origin.x + 16, row), Some(black));
    }
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-spaces"
    }));
}
#[test]
fn native_text_decoration_skip_spaces_revert_layer_rolls_back_layers_and_reaches_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; white-space:pre; text-decoration:underline overline line-through; text-decoration-skip-ink:none; } @layer base { #named { text-decoration-skip-spaces:all; } #repeated { text-decoration-skip-spaces:all; } #unlayered { text-decoration-skip-spaces:all; } #inline { text-decoration-skip-spaces:all; } } @layer theme { #named { text-decoration-skip-spaces:revert-layer; } #repeated { text-decoration-skip-spaces:end; } #unlayered { text-decoration-skip-spaces:end; } #inline { text-decoration-skip-spaces:end; } } @layer top { #repeated { text-decoration-skip-spaces:revert-layer; } } #unlayered { text-decoration-skip-spaces:revert-layer; }</style><div id='named' class='line'>\tA\u{00a0}</div><div id='repeated' class='line'>\tA\u{00a0}</div><div id='unlayered' class='line'>\tA\u{00a0}</div><div id='inline' class='line' style='text-decoration-skip-spaces:revert-layer'>\tA\u{00a0}</div><div id='fallback' class='line'>\tA\u{00a0}</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    let list = document.display_list(viewport).unwrap();
    let surface = list.rasterize().unwrap();
    let node_for = |id| document.resolve_target(&format!("id={id}")).unwrap();
    let run_for = |id| {
        let node_id = node_for(id);
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == node_id)
            .unwrap()
    };
    let skip_spaces_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                decoration_skip_spaces,
                ..
            } if *command_node_id == node_id => Some(*decoration_skip_spaces),
            _ => None,
        })
    };
    assert_eq!(
        skip_spaces_for(node_for("named")),
        Some(NativeTextDecorationSkipSpaces::All)
    );
    for id in ["repeated", "unlayered", "inline"] {
        assert_eq!(
            skip_spaces_for(node_for(id)),
            Some(NativeTextDecorationSkipSpaces::End)
        );
    }
    assert_eq!(
        skip_spaces_for(node_for("fallback")),
        Some(NativeTextDecorationSkipSpaces::None)
    );

    let white = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    let black = [0, 0, 0, u8::MAX];
    let assert_edges = |id, expected_edges: ([u8; 4], [u8; 4])| {
        let run = run_for(id);
        for row in [run.origin.y + 3, run.origin.y + 7] {
            assert_eq!(surface.pixel(run.origin.x, row), Some(expected_edges.0));
            assert_eq!(
                surface.pixel(run.origin.x + 16, row),
                Some(expected_edges.1)
            );
        }
    };
    assert_edges("named", (white, white));
    for id in ["repeated", "unlayered", "inline"] {
        let run = run_for(id);
        for row in [run.origin.y + 3, run.origin.y + 7] {
            assert_eq!(surface.pixel(run.origin.x, row), Some(black));
            assert_eq!(surface.pixel(run.origin.x + 16, row), Some(white));
        }
    }
    let fallback = run_for("fallback");
    for row in [fallback.origin.y + 3, fallback.origin.y + 7] {
        assert_eq!(surface.pixel(fallback.origin.x, row), Some(black));
        assert_eq!(surface.pixel(fallback.origin.x + 16, row), Some(black));
    }
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-skip-spaces"
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
fn native_text_decoration_thickness_revert_layer_reaches_all_line_owners() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; text-decoration:underline overline line-through; text-decoration-style:solid; text-decoration-skip-ink:none; } @layer base { #named { text-decoration-thickness:1px; } #rollback { text-decoration-thickness:1px; } #repeated { text-decoration-thickness:1px; } #unlayered { text-decoration-thickness:2px; } #inline { text-decoration-thickness:1px; } #fallback { text-decoration-thickness:revert-layer; } } @layer theme { .named { text-decoration-thickness:2px; } #rollback { text-decoration-thickness:3px; } #repeated { text-decoration-thickness:revert-layer; } #unlayered { text-decoration-thickness:4px; } #inline { text-decoration-thickness:2px; } } @layer top { #repeated { text-decoration-thickness:revert-layer; } } #named { text-decoration-thickness:revert-layer; } #unlayered { text-decoration-thickness:revert-layer; }</style><div id='named' class='line named'>AB</div><div id='rollback' class='line'>AB</div><div id='repeated' class='line'>AB</div><div id='unlayered' class='line'>AB</div><div id='inline' class='line' style='text-decoration-thickness:revert-layer'>AB</div><div id='fallback' class='line'>AB</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 140,
        device_scale_factor_milli: 1000,
    };
    let named = document.resolve_target("id=named").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let repeated = document.resolve_target("id=repeated").unwrap();
    let unlayered = document.resolve_target("id=unlayered").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_thickness,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *decoration_thickness,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let commands = [
        (text_command(named).expect("named thickness command"), 2),
        (
            text_command(rollback).expect("rollback thickness command"),
            3,
        ),
        (
            text_command(repeated).expect("repeated thickness command"),
            1,
        ),
        (
            text_command(unlayered).expect("unlayered thickness command"),
            4,
        ),
        (text_command(inline).expect("inline thickness command"), 2),
        (
            text_command(fallback).expect("fallback thickness command"),
            1,
        ),
    ];
    for (command, expected_thickness) in commands {
        assert_eq!(command.1, expected_thickness);
        assert!(command.2 && command.3 && command.4);
    }

    let surface = list.rasterize().unwrap();
    for (command, expected_thickness) in commands {
        let pixel = |y: u32| surface.pixel(command.0.x.saturating_add(2), y);
        let overline_start = command.0.y.saturating_sub(1);
        let line_through_start = command.0.y.saturating_add(3);
        let underline_start = command.0.y.saturating_add(7);
        if command.0.y > 0 {
            for offset in 0..expected_thickness {
                assert_eq!(
                    pixel(overline_start.saturating_add(offset)),
                    Some([0, 0, 0, 255]),
                    "overline band at y={}",
                    overline_start.saturating_add(offset)
                );
            }
        }
        for start in [line_through_start, underline_start] {
            for offset in 0..expected_thickness {
                assert_eq!(
                    pixel(start.saturating_add(offset)),
                    Some([0, 0, 0, 255]),
                    "decoration band at y={}",
                    start.saturating_add(offset)
                );
            }
        }
    }
}
#[test]
fn native_text_decoration_thickness_css_wide_resets_reach_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; text-decoration:underline; text-decoration-style:solid; text-decoration-thickness:1px; } #parent { text-decoration-thickness:3px; } #initial { text-decoration-thickness:initial; } #inherit { text-decoration-thickness:inherit; } #unset { text-decoration-thickness:unset; } #revert { text-decoration-thickness:revert; } #invalid { text-decoration-thickness:4px; text-decoration-thickness:5px; } #important { text-decoration-thickness:3px !important; } #important { text-decoration-thickness:1px; }</style><div id='parent' class='line'>P<span id='initial' class='line'>A</span><span id='inherit' class='line'>B</span><span id='unset' class='line'>C</span><span id='revert' class='line'>D</span><span id='invalid' class='line'>E</span><span id='important' class='line'>F</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let command_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                decoration_thickness,
                underline,
                ..
            } if *command_node_id == node_id => Some((*origin, *decoration_thickness, *underline)),
            _ => None,
        })
    };
    let initial = command_for("initial").expect("initial thickness command");
    let inherited = command_for("inherit").expect("inherit thickness command");
    let unset = command_for("unset").expect("unset thickness command");
    let revert = command_for("revert").expect("revert thickness command");
    let invalid = command_for("invalid").expect("invalid thickness command");
    let important = command_for("important").expect("important thickness command");
    assert_eq!(initial.1, 1);
    assert_eq!(inherited.1, 3);
    assert_eq!(unset.1, 3);
    assert_eq!(revert.1, 3);
    assert_eq!(invalid.1, 4);
    assert_eq!(important.1, 3);
    assert!(initial.2 && inherited.2 && important.2);

    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(initial.0.x, initial.0.y + 7),
        Some([0, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(initial.0.x, initial.0.y + 8),
        Some([255, 255, 255, 255])
    );
    for offset in 0..3 {
        assert_eq!(
            surface.pixel(inherited.0.x, inherited.0.y + 7 + offset),
            Some([0, 0, 0, 255])
        );
    }
    assert_eq!(
        surface.pixel(inherited.0.x, inherited.0.y + 10),
        Some([255, 255, 255, 255])
    );
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-decoration-thickness"
    }));
}
#[test]
fn native_text_underline_offset_css_wide_resets_reach_raster() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:40px; height:20px; line-height:20px; color:black; text-decoration:underline overline line-through; text-decoration-style:solid; text-decoration-thickness:1px; } #parent { text-underline-offset:-3px; } #initial { text-underline-offset:InItIaL; } #inherit { text-underline-offset:InHeRiT; } #unset { text-underline-offset:UnSeT; } #revert { text-underline-offset:ReVeRt; } #invalid { text-underline-offset:2px; text-underline-offset:5px; } #important { text-underline-offset:-2px !important; } #important { text-underline-offset:0px; }</style><div id='parent' class='line'>P<span id='initial' class='line'>A</span><span id='inherit' class='line'>B</span><span id='unset' class='line'>C</span><span id='revert' class='line'>D</span><span id='invalid' class='line'>E</span><span id='important' class='line'>F</span></div><div id='root-initial' class='line' style='text-underline-offset:initial'>Root</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 240,
        device_scale_factor_milli: 1000,
    };
    let list = document.display_list(viewport).unwrap();
    let command_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                underline_offset,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *underline_offset,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let parent = command_for("parent").expect("parent offset command");
    let initial = command_for("initial").expect("initial offset command");
    let inherited = command_for("inherit").expect("inherit offset command");
    let unset = command_for("unset").expect("unset offset command");
    let revert = command_for("revert").expect("revert offset command");
    let invalid = command_for("invalid").expect("invalid offset command");
    let important = command_for("important").expect("important offset command");
    let root_initial = command_for("root-initial").expect("root initial offset command");
    assert_eq!(parent.1, -3);
    assert_eq!(initial.1, 0);
    assert_eq!(inherited.1, -3);
    assert_eq!(unset.1, -3);
    assert_eq!(revert.1, -3);
    assert_eq!(invalid.1, 2);
    assert_eq!(important.1, -2);
    assert_eq!(root_initial.1, 0);
    for command in [
        parent,
        initial,
        inherited,
        unset,
        revert,
        invalid,
        important,
        root_initial,
    ] {
        assert!(command.2 && command.3 && command.4);
    }

    let surface = list.rasterize().unwrap();
    let underline_y = |command: (NativePoint, i32, bool, bool, bool)| {
        u32::try_from(i64::from(command.0.y) + 7 + i64::from(command.1))
            .expect("bounded underline coordinate")
    };
    for command in [
        parent,
        initial,
        inherited,
        unset,
        revert,
        invalid,
        important,
        root_initial,
    ] {
        assert_eq!(
            surface.pixel(command.0.x.saturating_add(2), underline_y(command)),
            Some([0, 0, 0, 255]),
            "underline raster for offset {}",
            command.1
        );
        assert_eq!(
            surface.pixel(command.0.x.saturating_add(2), command.0.y.saturating_add(3)),
            Some([0, 0, 0, 255]),
            "line-through raster for offset {}",
            command.1
        );
    }
    assert!(initial.0.y > 0);
    assert_eq!(
        surface.pixel(initial.0.x.saturating_add(2), initial.0.y - 1),
        Some([0, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "text-underline-offset"
    }));
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
fn native_text_underline_offset_revert_layer_moves_only_underlines() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:32px; height:20px; line-height:20px; color:black; text-decoration:underline overline line-through; text-decoration-style:solid; text-decoration-thickness:2px; } @layer base { #named { text-underline-offset:-1px; } #rollback { text-underline-offset:-1px; } #repeated { text-underline-offset:-1px; } #unlayered { text-underline-offset:-1px; } #inline { text-underline-offset:-1px; } #fallback { text-underline-offset:revert-layer; } } @layer theme { .named { text-underline-offset:-2px; } #rollback { text-underline-offset:revert-layer; } #repeated { text-underline-offset:revert-layer; } #unlayered { text-underline-offset:4px; } #inline { text-underline-offset:2px; } } @layer top { #repeated { text-underline-offset:revert-layer; } } #named { text-underline-offset:revert-layer; } #unlayered { text-underline-offset:revert-layer; }</style><div id='named' class='line named'>AB</div><div id='rollback' class='line'>AB</div><div id='repeated' class='line'>AB</div><div id='unlayered' class='line'>AB</div><div id='inline' class='line' style='text-underline-offset:revert-layer'>AB</div><div id='fallback' class='line'>AB</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 140,
        device_scale_factor_milli: 1000,
    };
    let named = document.resolve_target("id=named").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let repeated = document.resolve_target("id=repeated").unwrap();
    let unlayered = document.resolve_target("id=unlayered").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let list = document.display_list(viewport).unwrap();
    let text_command = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::TextRun {
                node_id: command_node_id,
                origin,
                underline_offset,
                underline,
                overline,
                line_through,
                ..
            } if *command_node_id == node_id => Some((
                *origin,
                *underline_offset,
                *underline,
                *overline,
                *line_through,
            )),
            _ => None,
        })
    };
    let commands = [
        (text_command(named).expect("named offset command"), -2),
        (text_command(rollback).expect("rollback offset command"), -1),
        (text_command(repeated).expect("repeated offset command"), -1),
        (
            text_command(unlayered).expect("unlayered offset command"),
            4,
        ),
        (text_command(inline).expect("inline offset command"), 2),
        (text_command(fallback).expect("fallback offset command"), 0),
    ];
    for (command, expected_offset) in commands {
        assert_eq!(command.1, expected_offset);
        assert!(command.2 && command.3 && command.4);
    }

    let surface = list.rasterize().unwrap();
    for (command, expected_offset) in commands {
        let pixel = |y: u32| surface.pixel(command.0.x.saturating_add(2), y);
        let line_through_start = command.0.y.saturating_add(3);
        let underline_start =
            u32::try_from(i64::from(command.0.y) + 7 + i64::from(expected_offset))
                .expect("bounded underline coordinate");
        for offset in 0..2 {
            assert_eq!(
                pixel(line_through_start.saturating_add(offset)),
                Some([0, 0, 0, 255]),
                "line-through band at y={}",
                line_through_start.saturating_add(offset)
            );
            assert_eq!(
                pixel(underline_start.saturating_add(offset)),
                Some([0, 0, 0, 255]),
                "underline band at y={}",
                underline_start.saturating_add(offset)
            );
        }
        if command.0.y > 0 {
            let overline_start = command.0.y.saturating_sub(1);
            for offset in 0..2 {
                assert_eq!(
                    pixel(overline_start.saturating_add(offset)),
                    Some([0, 0, 0, 255]),
                    "overline band at y={}",
                    overline_start.saturating_add(offset)
                );
            }
        }
    }
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
fn native_line_height_revert_layer_preserves_flow_inheritance_and_explicit_height() {
    let document = NativeDocument::parse(
        "<style>.line { display:block; width:16px; } @layer base { #named { line-height:20px; } #repeated { line-height:24px; } #inline { line-height:28px; } #parent { line-height:28px; } #explicit { line-height:20px; } } @layer theme { #named { line-height:32px; } #repeated { line-height:revert-layer; } #inline { line-height:36px; } #parent { line-height:32px; } #explicit { line-height:32px; } } @layer top { #repeated { line-height:revert-layer; } } #named { line-height:revert-layer; } #repeated { line-height:revert-layer; } #parent { line-height:revert-layer; } #explicit { line-height:revert-layer; }</style><div id='named' class='line'>A</div><div id='repeated' class='line'>A</div><div id='inline' class='line' style='line-height:ReVeRt-LaYeR'>A</div><div id='parent' class='line'><span id='child'>A</span></div><div id='fallback' class='line' style='line-height:revert-layer'>A</div><div id='explicit' class='line' style='line-height:revert-layer;height:4px'>A</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().iter().all(|diagnostic| {
        !(diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "line-height")
    }));
    let viewport = Viewport {
        width: 32,
        height: 256,
        device_scale_factor_milli: 1000,
    };
    let named = document.resolve_target("id=named").unwrap();
    let repeated = document.resolve_target("id=repeated").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.box_for(named).unwrap().height, 32);
    assert_eq!(layout.box_for(repeated).unwrap().height, 24);
    assert_eq!(layout.box_for(inline).unwrap().height, 36);
    assert_eq!(layout.box_for(parent).unwrap().height, 32);
    assert_eq!(layout.box_for(child).unwrap().height, 32);
    assert_eq!(layout.box_for(fallback).unwrap().height, 20);
    assert_eq!(layout.box_for(explicit).unwrap().height, 4);
    assert_eq!(layout.box_for(named).unwrap().y, 0);
    assert_eq!(layout.box_for(repeated).unwrap().y, 32);
    assert_eq!(layout.box_for(inline).unwrap().y, 56);
    assert_eq!(layout.box_for(parent).unwrap().y, 92);
    assert_eq!(layout.box_for(child).unwrap().y, 92);
    assert_eq!(layout.box_for(fallback).unwrap().y, 124);
    assert_eq!(layout.box_for(explicit).unwrap().y, 144);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(named));
    assert_eq!(layout.hit_test(1, 33).unwrap(), Some(repeated));
    assert_eq!(layout.hit_test(1, 57).unwrap(), Some(inline));
    assert_eq!(layout.hit_test(1, 125).unwrap(), Some(fallback));
    assert_eq!(layout.hit_test(1, 145).unwrap(), Some(explicit));

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
    assert!(text_origins.contains(&(named, NativePoint { x: 0, y: 0 })));
    assert!(text_origins.contains(&(repeated, NativePoint { x: 0, y: 32 })));
    assert!(text_origins.contains(&(inline, NativePoint { x: 0, y: 56 })));
    assert!(text_origins.contains(&(child, NativePoint { x: 0, y: 92 })));
    assert!(text_origins.contains(&(fallback, NativePoint { x: 0, y: 124 })));
    assert!(text_origins.contains(&(explicit, NativePoint { x: 0, y: 144 })));
}

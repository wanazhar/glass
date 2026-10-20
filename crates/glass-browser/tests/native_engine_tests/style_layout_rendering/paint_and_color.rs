use super::*;

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
fn native_opacity_revert_layer_preserves_group_compositing_and_geometry() {
    let document = NativeDocument::parse(
        "<style>@layer base { #group { display:block;width:20px;height:12px;background-color:red;opacity:50%; } #zero { display:block;width:20px;height:12px;background-color:blue;opacity:0; } #fallback { display:block;width:20px;height:12px;background-color:green;opacity:revert-layer; } } @layer theme { #group { opacity:75%; } #group { opacity:revert-layer; } } @layer top { #group { opacity:revert-layer; } } #group { opacity:revert-layer; }</style><button id='group'>Group</button><button id='zero'>Zero</button><button id='fallback'>Fallback</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let group = document.resolve_target("id=group").unwrap();
    let zero = document.resolve_target("id=zero").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(group),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 20,
            height: 12,
        })
    );
    assert_eq!(layout.box_for(zero).unwrap().y, 12);
    assert_eq!(layout.box_for(fallback).unwrap().y, 24);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(group));
    assert_eq!(layout.hit_test(1, 13).unwrap(), Some(zero));
    assert_eq!(layout.hit_test(1, 25).unwrap(), Some(fallback));

    let semantic_nodes = document.semantic_nodes();
    for node_id in [group, zero, fallback] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| { node.node_id == node_id && !node.hidden })
        );
    }

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
    assert_eq!(groups, vec![(group, 128), (zero, 0)]);
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, .. } if *node_id == fallback
        )
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "opacity"
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 127, 127, 255]));
    assert_eq!(surface.pixel(1, 13), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(1, 25), Some([0, 128, 0, 255]));
}
#[test]
fn native_background_current_color_resolves_through_fill_and_raster_owners() {
    let document = NativeDocument::parse(
        "<style>@layer base { #local { display:block;width:8px;height:8px;color:blue;background-color:red;background-color:currentColor; } #parent { display:block;width:8px;height:8px;color:green; } #inherited { display:block;width:8px;height:8px;background-color:CURRENTcolor; } #black { display:block;width:8px;height:8px;background-color:CuRrEnTcOlOr; } #rollback { display:block;width:8px;height:8px;color:blue;background-color:red; } #invalid { display:block;width:8px;height:8px;color:blue;background-color:red; } #transparent { display:block;width:8px;height:8px;background-color:transparent; } #empty { display:block;width:8px;height:8px; } } @layer theme { #rollback { background-color:currentColor; } #invalid { background-color:currentColor;background-color:invalid; } } #rollback { background-color:revert-layer; }</style><div id='parent'><button id='inherited'>Inherited</button></div><button id='local'>Local</button><button id='black'>Black</button><button id='rollback'>Rollback</button><button id='invalid'>Invalid</button><button id='inline' style='display:block;width:8px;height:8px;color:green;background-color:currentColor;opacity:50%'>Inline</button><button id='transparent'>Transparent</button><button id='empty'>Empty</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 16,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let local = document.resolve_target("id=local").unwrap();
    let black = document.resolve_target("id=black").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let transparent = document.resolve_target("id=transparent").unwrap();
    let empty = document.resolve_target("id=empty").unwrap();

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
    for node_id in [
        parent,
        inherited,
        local,
        black,
        rollback,
        invalid,
        inline,
        transparent,
        empty,
    ] {
        assert!(
            layout.box_for(node_id).is_some(),
            "missing layout box for {node_id:?}"
        );
    }
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(inherited));

    let list = document.display_list(viewport).unwrap();
    let fill_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                rect,
                color,
                clip,
                ..
            } if *command_node_id == node_id => Some((*rect, *color, *clip)),
            _ => None,
        })
    };
    assert_eq!(fill_for(parent), None);
    assert_eq!(fill_for(inherited).unwrap().1, green);
    assert_eq!(fill_for(local).unwrap().1, blue);
    assert_eq!(fill_for(black).unwrap().1, NativeColor::BLACK);
    assert_eq!(fill_for(rollback).unwrap().1, blue);
    assert_eq!(fill_for(invalid).unwrap().1, blue);
    assert_eq!(fill_for(inline).unwrap().1, green);
    assert_eq!(fill_for(transparent).unwrap().1, transparent_color);
    assert_eq!(fill_for(empty), None);
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BeginOpacityGroup { node_id, opacity }
                if *node_id == inline && *opacity == 128
        )
    }));

    let inline_fill = fill_for(inline).unwrap();
    assert_eq!(inline_fill.2, None);
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(1, inline_fill.0.y),
        Some([127, 191, 127, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());

    let semantic_nodes = document.semantic_nodes();
    let semantic_ids = semantic_nodes
        .iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [
        (inherited, local),
        (local, black),
        (black, rollback),
        (rollback, invalid),
        (invalid, inline),
        (inline, transparent),
        (transparent, empty),
    ] {
        assert!(
            semantic_ids.iter().position(|id| *id == pair.0)
                < semantic_ids.iter().position(|id| *id == pair.1),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "background-color"
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "color"
    }));
}
#[test]
fn native_color_current_color_resolves_from_inherited_owner_without_recursion() {
    let document = NativeDocument::parse(
        "<style>.surface { display:block;width:64px;height:20px;line-height:20px;background-color:currentColor;border:1px solid currentColor;text-decoration:underline;text-decoration-color:currentColor; } @layer base { #parent { color:green; } #surface { color:currentColor; } #rollback { color:red; } #invalid { color:red;color:currentColor;color:invalid; } } @layer theme { #rollback { color:currentColor; } } #rollback { color:revert-layer; } #concrete { color:blue; }</style><div id='parent' role='button'><div id='surface' class='surface' role='button'>Surface</div><div id='rollback' class='surface' role='button'>Rollback</div><div id='invalid' class='surface' role='button'>Invalid</div><div id='inline' class='surface' style='color:currentColor'>Inline</div></div><div id='root-current' class='surface'>Root</div><div id='concrete' class='surface' style='color:currentColor;color:blue'>Concrete</div><div id='omitted' class='surface'>Omitted</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 220,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let surface = document.resolve_target("id=surface").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let root_current = document.resolve_target("id=root-current").unwrap();
    let concrete = document.resolve_target("id=concrete").unwrap();
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

    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout
            .hit_test(
                (layout.box_for(surface).unwrap().x + 1).into(),
                (layout.box_for(surface).unwrap().y + 1).into(),
            )
            .unwrap(),
        Some(surface)
    );
    assert!(layout.box_for(root_current).is_some());
    assert!(layout.box_for(omitted).is_some());

    let list = document.display_list(viewport).unwrap();
    let text_for = |node_id| {
        list.commands
            .iter()
            .find_map(|command| match command {
                NativeDisplayCommand::TextRun {
                    node_id: command_node_id,
                    origin,
                    color,
                    decoration_color,
                    ..
                } if *command_node_id == node_id => Some((*origin, *color, *decoration_color)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text command for {node_id:?}"))
    };
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
    for node_id in [surface, rollback, invalid, inline] {
        let text = text_for(node_id);
        assert_eq!(text.1, green);
        assert_eq!(text.2, green);
        assert_eq!(fill_for(node_id), Some(green));
        assert_eq!(border_for(node_id).unwrap().top.color, green);
    }
    assert_eq!(text_for(root_current).1, NativeColor::BLACK);
    assert_eq!(text_for(root_current).2, NativeColor::BLACK);
    assert_eq!(fill_for(root_current), Some(NativeColor::BLACK));
    assert_eq!(
        border_for(root_current).unwrap().top.color,
        NativeColor::BLACK
    );
    assert_eq!(text_for(concrete).1, blue);
    assert_eq!(text_for(concrete).2, blue);
    assert_eq!(fill_for(concrete), Some(blue));
    assert_eq!(border_for(concrete).unwrap().top.color, blue);
    assert_eq!(text_for(omitted).1, NativeColor::BLACK);
    assert_eq!(text_for(omitted).2, NativeColor::BLACK);

    let surface_text = text_for(surface);
    let raster = list.rasterize().unwrap();
    assert_eq!(
        raster.pixel(surface_text.0.x + 1, surface_text.0.y),
        Some([0, 128, 0, 255])
    );
    assert!(!raster.to_png().unwrap().is_empty());

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [(parent, surface), (surface, rollback), (rollback, invalid)] {
        assert!(
            semantic_ids.iter().position(|id| *id == pair.0)
                < semantic_ids.iter().position(|id| *id == pair.1),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == "color"
            })
            .count(),
        1
    );
}
#[test]
fn native_paint_color_revert_layer_preserves_fill_glyph_and_inheritance_owners() {
    let document = NativeDocument::parse(
        "<!doctype html><style>.line { display:block; width:24px; height:20px; line-height:20px; } @layer base { #named { color:red; background-color:red; } #rollback { color:red; background-color:red; } #repeated { color:red; background-color:red; } #unlayered { color:red; background-color:red; } #inline { color:red; background-color:red; } #independent { color:red; background-color:red; } #parent { color:red; } } @layer theme { .named { color:blue; background-color:blue; } #rollback { color:revert-layer; background-color:revert-layer; } #repeated { color:revert-layer; background-color:revert-layer; } #unlayered { color:blue; background-color:blue; } #inline { color:blue; background-color:blue; } #independent { color:revert-layer; background-color:blue; } #parent { color:blue; } } @layer top { #repeated { color:revert-layer; background-color:revert-layer; } } #named { color:revert-layer; background-color:revert-layer; } #unlayered { color:revert-layer; background-color:revert-layer; } #parent { color:revert-layer; } #fallback { color:revert-layer; background-color:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='color:revert-layer;background-color:revert-layer'>A</div><div id='independent' class='line'>A</div><div id='parent' class='line'>P<span id='child'>C</span></div><div id='fallback' class='line'>F</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    assert!(document.diagnostics().is_empty());
    let viewport = Viewport {
        width: 32,
        height: 180,
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

    let text_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands
            .iter()
            .find_map(|command| match command {
                NativeDisplayCommand::TextRun {
                    node_id: command_node_id,
                    origin,
                    color,
                    ..
                } if *command_node_id == node_id => Some((*origin, *color)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text command for {id}"))
    };
    let fill_for = |id| {
        let node_id = document.resolve_target(&format!("id={id}")).unwrap();
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                rect,
                color,
                ..
            } if *command_node_id == node_id => Some((*rect, *color)),
            _ => None,
        })
    };

    assert_eq!(text_for("named").1, blue);
    assert_eq!(fill_for("named").unwrap().1, blue);
    assert_eq!(text_for("rollback").1, NativeColor::RED);
    assert_eq!(fill_for("rollback").unwrap().1, NativeColor::RED);
    assert_eq!(text_for("repeated").1, NativeColor::RED);
    assert_eq!(fill_for("repeated").unwrap().1, NativeColor::RED);
    assert_eq!(text_for("unlayered").1, blue);
    assert_eq!(fill_for("unlayered").unwrap().1, blue);
    assert_eq!(text_for("inline").1, blue);
    assert_eq!(fill_for("inline").unwrap().1, blue);
    assert_eq!(text_for("independent").1, NativeColor::RED);
    assert_eq!(fill_for("independent").unwrap().1, blue);
    assert_eq!(text_for("child").1, blue);
    assert_eq!(text_for("fallback").1, NativeColor::BLACK);
    assert!(fill_for("fallback").is_none());

    let independent_text = text_for("independent");
    let independent_fill = fill_for("independent").unwrap();
    assert_eq!(
        surface.pixel(independent_fill.0.x, independent_fill.0.y),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        surface.pixel(independent_text.0.x.saturating_add(1), independent_text.0.y,),
        Some([255, 0, 0, 255])
    );
}
#[test]
fn native_css_background_size_and_position_place_one_scaled_tile() {
    let source = native_test_png_data_url();
    let markup = format!(
        "<div id='surface' style='width:8px;height:8px;background-image:url(\"{source}\");background-repeat:no-repeat;background-size:4px 2px;background-position:right bottom'></div>"
    );
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
    let surface_id = document.resolve_target("id=surface").unwrap();
    let list = document
        .display_list(Viewport {
            width: 8,
            height: 8,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let command = list
        .commands
        .iter()
        .find(|command| {
            matches!(
                command,
                NativeDisplayCommand::Image { node_id, .. } if *node_id == surface_id
            )
        })
        .unwrap();
    assert!(matches!(
        command,
        NativeDisplayCommand::Image {
            rect,
            source_rect,
            source_width: 2,
            source_height: 2,
            ..
        } if *rect == NativeRect { x: 4, y: 6, width: 4, height: 2 }
            && *source_rect == NativeRect { x: 0, y: 0, width: 2, height: 2 }
    ));
    let painted = list.rasterize().unwrap();
    assert_eq!(painted.pixel(3, 6), Some([255, 255, 255, 255]));
    assert_eq!(painted.pixel(4, 6), Some([255, 0, 0, 255]));
    assert_eq!(painted.pixel(7, 7), Some([255, 255, 255, 255]));
}
#[test]
fn native_css_background_partial_tile_uses_the_matching_source_crop() {
    let source = native_test_png_data_url();
    let markup = format!(
        "<div id='surface' style='width:1px;height:2px;background-image:url(\"{source}\");background-repeat:no-repeat;background-size:4px 2px;background-position:-3px 0'></div>"
    );
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
    let surface_id = document.resolve_target("id=surface").unwrap();
    let list = document
        .display_list(Viewport {
            width: 1,
            height: 2,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    let command = list
        .commands
        .iter()
        .find(|command| {
            matches!(
                command,
                NativeDisplayCommand::Image { node_id, .. } if *node_id == surface_id
            )
        })
        .unwrap();
    assert!(matches!(
        command,
        NativeDisplayCommand::Image {
            rect,
            source_rect,
            ..
        } if *rect == NativeRect { x: 0, y: 0, width: 1, height: 2 }
            && *source_rect == NativeRect { x: 1, y: 0, width: 1, height: 2 }
    ));
    assert_eq!(
        list.rasterize().unwrap().pixel(0, 0),
        Some([127, 255, 127, 255])
    );
}
#[test]
fn native_css_background_cover_and_contain_keep_intrinsic_ratio() {
    let source = native_test_png_data_url();
    let cover = NativeDocument::parse(
        &format!(
            "<div id='surface' style='width:8px;height:4px;background-image:url(\"{source}\");background-repeat:no-repeat;background-size:cover'></div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let cover_id = cover.resolve_target("id=surface").unwrap();
    let cover_list = cover
        .display_list(Viewport {
            width: 8,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(cover_list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image {
                node_id,
                rect,
                source_rect,
                ..
            }
                if *node_id == cover_id
                    && *rect == NativeRect { x: 0, y: 0, width: 8, height: 4 }
                    && *source_rect == NativeRect { x: 0, y: 0, width: 2, height: 1 }
        )
    }));

    let contain = NativeDocument::parse(
        &format!(
            "<div id='surface' style='width:8px;height:4px;background-image:url(\"{source}\");background-repeat:no-repeat;background-size:contain'></div>"
        ),
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let contain_id = contain.resolve_target("id=surface").unwrap();
    let contain_list = contain
        .display_list(Viewport {
            width: 8,
            height: 4,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(contain_list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image { node_id, rect, .. }
                if *node_id == contain_id
                    && *rect == NativeRect { x: 0, y: 0, width: 4, height: 4 }
        )
    }));
    let painted = contain_list.rasterize().unwrap();
    assert_eq!(painted.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(painted.pixel(5, 0), Some([255, 255, 255, 255]));
}
#[test]
fn native_css_background_shorthand_expands_into_one_positioned_layer() {
    let source = native_test_png_data_url();
    let markup = format!(
        "<div id='surface' style='width:8px;height:8px;background:red url(\"{source}\") no-repeat right bottom / 4px 2px'></div>"
    );
    let document = NativeDocument::parse(&markup, &NativeEngineLimits::default()).unwrap();
    let surface_id = document.resolve_target("id=surface").unwrap();
    let list = document
        .display_list(Viewport {
            width: 8,
            height: 8,
            device_scale_factor_milli: 1000,
        })
        .unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::Image { node_id, rect, .. }
                if *node_id == surface_id
                    && *rect == NativeRect { x: 4, y: 6, width: 4, height: 2 }
        )
    }));
    let painted = list.rasterize().unwrap();
    assert_eq!(painted.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(painted.pixel(4, 6), Some([255, 0, 0, 255]));
}
#[test]
fn native_css_absolute_position_is_out_of_flow_and_paints_after_flow() {
    let document = NativeDocument::parse(
        "<div id='overlay' style='position:absolute;left:6px;top:4px;width:8px;height:8px;background:red'></div><div id='flow' style='width:10px;height:8px;background:blue'></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let overlay = document.resolve_target("id=overlay").unwrap();
    let flow = document.resolve_target("id=flow").unwrap();
    let viewport = Viewport {
        width: 32,
        height: 24,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(overlay),
        Some(NativeRect {
            x: 6,
            y: 4,
            width: 8,
            height: 8,
        })
    );
    assert_eq!(layout.box_for(flow).unwrap().y, 0);
    assert_eq!(layout.hit_test(7, 5).unwrap(), Some(overlay));

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == overlay
                    && *rect == NativeRect { x: 6, y: 4, width: 8, height: 8 }
                    && *color == NativeColor::RED
        )
    }));
}
#[test]
fn native_css_z_index_orders_positioned_overlays_for_paint_and_hit_testing() {
    let document = NativeDocument::parse(
        "<button id='high' style='display:block;position:absolute;left:2px;top:2px;width:12px;height:8px;background:blue;z-index:2'>high</button><button id='low' style='display:block;position:absolute;left:2px;top:2px;width:12px;height:8px;background:red;z-index:1'>low</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let high = document.resolve_target("id=high").unwrap();
    let low = document.resolve_target("id=low").unwrap();
    let viewport = Viewport {
        width: 24,
        height: 16,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();

    assert_eq!(layout.hit_test(3, 3).unwrap(), Some(high));
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == high)
            .map(|layout_box| layout_box.z_index),
        Some(2)
    );
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == low)
            .map(|layout_box| layout_box.z_index),
        Some(1)
    );

    let list = document.display_list(viewport).unwrap();
    let fills = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::FillRect { node_id, .. } => Some(*node_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fills, vec![low, high]);
    assert_eq!(
        list.rasterize().unwrap().pixel(3, 3),
        Some([0, 0, 255, 255])
    );
}

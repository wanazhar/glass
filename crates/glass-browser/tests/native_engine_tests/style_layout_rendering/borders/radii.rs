use super::*;

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
    assert_ne!(layout.hit_test(0, 0).unwrap(), Some(card));
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
fn native_border_radius_corner_longhands_reach_all_rounded_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                #card { display:block;width:24px;height:20px;border:2px solid blue;background-color:red;border-radius:1px; border-top-left-radius:7px; border-top-right-radius:8px; border-bottom-right-radius:9px; border-bottom-left-radius:10px; }
                #inherited { display:block;width:8px;height:6px;border:1px solid blue;background-color:blue;border-top-left-radius:inherit; border-bottom-right-radius:inherit; }
                #invalid { display:block;width:8px;height:6px;border:1px solid blue;background-color:green;border-radius:5px; border-top-right-radius:50%; }
            }
        </style>
        <div id='card' role='button'>Card<div id='inherited' role='button'>Inherited</div></div>
        <div id='invalid' role='button'>Invalid</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let layout = document.layout(viewport).unwrap();
    let layout_box_for = |node_id| {
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == node_id)
            .unwrap()
    };
    let card_box = layout_box_for(card);
    assert_eq!(
        card_box.border_radius,
        NativeBorderRadius {
            top_left: 7,
            top_right: 8,
            bottom_right: 9,
            bottom_left: 10,
        }
    );
    assert_eq!(
        layout_box_for(inherited).border_radius,
        NativeBorderRadius {
            top_left: 7,
            top_right: 0,
            bottom_right: 9,
            bottom_left: 0,
        }
    );
    assert_eq!(
        layout_box_for(invalid).border_radius,
        NativeBorderRadius {
            top_left: 5,
            top_right: 5,
            bottom_right: 5,
            bottom_left: 5,
        }
    );
    assert_eq!(
        layout
            .hit_test(
                (card_box.rect.x + card_box.rect.width / 2).into(),
                (card_box.rect.y + card_box.rect.height / 2).into(),
            )
            .unwrap(),
        Some(card)
    );
    assert_ne!(
        layout
            .hit_test(card_box.rect.x.into(), card_box.rect.y.into())
            .unwrap(),
        Some(card)
    );

    let semantic_nodes = document.semantic_nodes();
    for node_id in [card, inherited, invalid] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected corner-radius fixture node {node_id:?} to remain visible"
        );
    }

    let list = document.display_list(viewport).unwrap();
    for node_id in [card, inherited, invalid] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id: command_node_id, radius, .. }
                    if *command_node_id == node_id
                        && *radius == layout_box_for(node_id).border_radius
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::BorderRect { node_id: command_node_id, radius, .. }
                    if *command_node_id == node_id
                        && *radius == layout_box_for(node_id).border_radius
            )
        }));
    }
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(
            card_box.rect.x + card_box.rect.width / 2,
            card_box.rect.y + card_box.rect.height / 2,
        ),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-top-right-radius"
    }));
}
#[test]
fn native_logical_border_radius_corner_longhands_map_all_rounded_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                #ltr { display:block;width:24px;height:20px;border:2px solid blue;background-color:red;direction:ltr;border-start-start-radius:7px;border-start-end-radius:8px;border-end-start-radius:10px;border-end-end-radius:9px; }
                #rtl { display:block;width:24px;height:20px;border:2px solid blue;background-color:green;direction:rtl;border-start-start-radius:7px;border-start-end-radius:8px;border-end-start-radius:10px;border-end-end-radius:9px; }
                #parent { display:block;width:28px;height:20px;border:1px solid blue;background-color:blue;direction:rtl;border-radius:3px 4px 5px 6px; }
                #inherited { display:block;width:8px;height:6px;border:1px solid blue;background-color:blue;border-start-start-radius:inherit;border-end-end-radius:inherit; }
                #invalid { display:block;width:8px;height:6px;border:1px solid blue;background-color:red;direction:ltr;border-radius:5px;border-start-end-radius:50%; }
            }
        </style>
        <div id='ltr' role='button'>LTR</div>
        <div id='rtl' role='button'>RTL</div>
        <div id='parent' role='button'><div id='inherited' role='button'>Inherited</div></div>
        <div id='invalid' role='button'>Invalid</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 40,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let ltr = document.resolve_target("id=ltr").unwrap();
    let rtl = document.resolve_target("id=rtl").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let layout = document.layout(viewport).unwrap();
    let layout_box_for = |node_id| {
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == node_id)
            .unwrap()
    };
    let ltr_box = layout_box_for(ltr);
    assert_eq!(
        ltr_box.border_radius,
        NativeBorderRadius {
            top_left: 7,
            top_right: 8,
            bottom_right: 9,
            bottom_left: 10,
        }
    );
    assert_eq!(
        layout_box_for(rtl).border_radius,
        NativeBorderRadius {
            top_left: 8,
            top_right: 7,
            bottom_right: 10,
            bottom_left: 9,
        }
    );
    assert_eq!(
        layout_box_for(inherited).border_radius,
        NativeBorderRadius {
            top_left: 0,
            top_right: 4,
            bottom_right: 0,
            bottom_left: 6,
        }
    );
    assert_eq!(
        layout_box_for(invalid).border_radius,
        NativeBorderRadius {
            top_left: 5,
            top_right: 5,
            bottom_right: 5,
            bottom_left: 5,
        }
    );
    assert_eq!(
        layout
            .hit_test(
                (ltr_box.rect.x + ltr_box.rect.width / 2).into(),
                (ltr_box.rect.y + ltr_box.rect.height / 2).into(),
            )
            .unwrap(),
        Some(ltr)
    );
    assert_ne!(
        layout
            .hit_test(ltr_box.rect.x.into(), ltr_box.rect.y.into())
            .unwrap(),
        Some(ltr)
    );

    let semantic_nodes = document.semantic_nodes();
    for node_id in [ltr, rtl, parent, inherited, invalid] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected logical corner-radius fixture node {node_id:?} to remain visible"
        );
    }

    let list = document.display_list(viewport).unwrap();
    for (node_id, expected_radius) in [
        (ltr, ltr_box.border_radius),
        (rtl, layout_box_for(rtl).border_radius),
        (inherited, layout_box_for(inherited).border_radius),
        (invalid, layout_box_for(invalid).border_radius),
    ] {
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id: command_node_id, radius, .. }
                    if *command_node_id == node_id && *radius == expected_radius
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::BorderRect { node_id: command_node_id, radius, .. }
                    if *command_node_id == node_id && *radius == expected_radius
            )
        }));
    }
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(
            ltr_box.rect.x + ltr_box.rect.width / 2,
            ltr_box.rect.y + ltr_box.rect.height / 2,
        ),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-start-end-radius"
    }));
}
#[test]
fn native_border_radius_important_priority_reaches_rounded_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                .box { display:block;width:20px;height:12px;border:2px solid blue; }
                #early { background-color:red;border-radius:4px !important; }
                #rollback { background-color:green;border-radius:6px !important;border-top-left-radius:revert-layer !important; }
                #logical { background-color:blue;direction:rtl;border-radius:2px;border-start-start-radius:8px !important;border-start-end-radius:9px !important; }
                #invalid { background-color:red;border-radius:5px;border-top-right-radius:50% !important; }
                #normal { background-color:green;border-radius:1px; }
                #mixed { background-color:blue;border-radius:1px !important; }
            }
            @layer theme {
                #early { border-radius:7px !important; }
                #rollback { border-radius:9px !important; }
                #normal { border-radius:2px; }
            }
            #normal { border-radius:3px; }
            #mixed { border-radius:9px; }
            #inline { background-color:green;border-radius:3px !important; }
        </style>
        <div id='early' class='box' role='button' aria-label='Early'></div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='logical' class='box' role='button'>Logical</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='inline' class='box' role='button' style='border-radius:10px !important'>Inline</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let early = document.resolve_target("id=early").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let logical = document.resolve_target("id=logical").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [early, rollback, logical, invalid, normal, mixed, inline];
    let layout = document.layout(viewport).unwrap();
    let layout_box_for = |node_id| {
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == node_id)
            .expect("important-radius layout box")
    };
    assert_eq!(
        layout_box_for(early).border_radius,
        NativeBorderRadius {
            top_left: 4,
            top_right: 4,
            bottom_right: 4,
            bottom_left: 4,
        }
    );
    assert_eq!(
        layout_box_for(rollback).border_radius,
        NativeBorderRadius {
            top_left: 9,
            top_right: 6,
            bottom_right: 6,
            bottom_left: 6,
        }
    );
    assert_eq!(
        layout_box_for(logical).border_radius,
        NativeBorderRadius {
            top_left: 9,
            top_right: 8,
            bottom_right: 2,
            bottom_left: 2,
        }
    );
    for node_id in [invalid, normal, mixed, inline] {
        let expected = match node_id {
            node_id if node_id == invalid => 5,
            node_id if node_id == normal => 3,
            node_id if node_id == mixed => 1,
            _ => 10,
        };
        assert_eq!(
            layout_box_for(node_id).border_radius,
            NativeBorderRadius {
                top_left: expected,
                top_right: expected,
                bottom_right: expected,
                bottom_left: expected,
            },
            "radius for {node_id:?}"
        );
    }

    assert_ne!(layout.hit_test(0, 0).unwrap(), Some(early));
    let early_box = layout_box_for(early);
    assert_eq!(
        layout
            .hit_test(
                (early_box.rect.x + early_box.rect.width / 2).into(),
                (early_box.rect.y + early_box.rect.height / 2).into(),
            )
            .unwrap(),
        Some(early)
    );

    let list = document.display_list(viewport).unwrap();
    for node_id in targets {
        let expected_radius = layout_box_for(node_id).border_radius;
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::FillRect { node_id: command_node_id, radius, .. }
                    if *command_node_id == node_id && *radius == expected_radius
            )
        }));
        assert!(list.commands.iter().any(|command| {
            matches!(
                command,
                NativeDisplayCommand::BorderRect { node_id: command_node_id, radius, .. }
                    if *command_node_id == node_id && *radius == expected_radius
            )
        }));
    }
    let surface = list.rasterize().unwrap();
    let early_box = layout_box_for(early);
    assert_eq!(
        surface.pixel(
            early_box.rect.x + early_box.rect.width / 2,
            early_box.rect.y + early_box.rect.height / 2,
        ),
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
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-top-right-radius"
    }));
}
#[test]
fn native_border_radius_revert_layer_preserves_rounded_consumers() {
    let document = NativeDocument::parse(
        "<style>@layer base { #card { width:12px;height:10px;border:2px solid blue;background-color:red;border-radius:4px; } } @layer theme { #card { border-radius:6px; } #card { border-radius: revert-layer; } } @layer top { #card { border-radius: revert-layer; } } #card { border-radius: revert-layer; } #fallback { width:12px;height:10px;border:2px solid blue;background-color:red;border-radius: revert-layer; }</style><div id='card'>Card</div><div id='fallback'>Fallback</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let layout = document.layout(viewport).unwrap();
    let card_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == card)
        .unwrap();
    let fallback_box = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id == fallback)
        .unwrap();
    assert_eq!(
        card_box.border_radius,
        NativeBorderRadius {
            top_left: 4,
            top_right: 4,
            bottom_right: 4,
            bottom_left: 4,
        }
    );
    assert_eq!(fallback_box.border_radius, NativeBorderRadius::default());
    assert_ne!(layout.hit_test(0, 0).unwrap(), Some(card));
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

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 0), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(3, 0), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(4, 4), Some([255, 0, 0, 255]));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-radius"
    }));
}

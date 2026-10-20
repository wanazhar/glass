use super::*;

#[test]
fn native_border_width_revert_layer_preserves_component_geometry_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #shorthand { display:block;width:8px;height:6px;border:1px solid red; } #sides { display:block;width:8px;height:6px;border:2px dashed red; } #repeat { display:block;width:8px;height:6px;border:1px solid red; } #fallback { display:block;width:8px;height:6px;border:1px solid red; } #same { display:block;width:8px;height:6px;border:1px solid red;border-width:revert-layer; } #order-a { display:block;width:8px;height:6px;border-width:3px;border:1px solid red; } #order-b { display:block;width:8px;height:6px;border:1px solid red;border-width:3px; } #width-only { display:block;width:8px;height:6px;border-width:4px; } #inline { display:block;width:8px;height:6px;border:1px solid red; } } @layer theme { #shorthand { border-width:5px 6px 7px 8px; } #shorthand { border-top-width:revert-layer; } #sides { border-top-width:4px;border-right-width:revert-layer;border-bottom-width:6px;border-left-width:8px; } #repeat { border-width:4px; } #repeat { border-width:revert-layer; } #fallback { border-width:4px; } #fallback { border-width:revert-layer; } } @layer top { #repeat { border-width:revert-layer; } #fallback { border-width:revert-layer; } } #shorthand { border-width:revert-layer; } #repeat { border-width:revert-layer; } #fallback { border-width:revert-layer; }</style><button id='shorthand'>Shorthand</button><button id='sides'>Sides</button><button id='repeat'>Repeat</button><button id='fallback'>Fallback</button><button id='same'>Same</button><button id='order-a'>Order A</button><button id='order-b'>Order B</button><button id='width-only'>Width only</button><button id='inline' style='border-width:4px'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let shorthand = document.resolve_target("id=shorthand").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let same = document.resolve_target("id=same").unwrap();
    let order_a = document.resolve_target("id=order-a").unwrap();
    let order_b = document.resolve_target("id=order-b").unwrap();
    let width_only = document.resolve_target("id=width-only").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(shorthand),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 22,
            height: 14,
        })
    );
    assert_eq!(layout.box_for(sides).unwrap().width, 18);
    assert_eq!(layout.box_for(sides).unwrap().height, 16);
    assert_eq!(layout.box_for(repeat).unwrap().width, 10);
    assert_eq!(layout.box_for(fallback).unwrap().width, 10);
    assert_eq!(layout.box_for(same).unwrap().width, 8);
    assert_eq!(layout.box_for(same).unwrap().height, 6);
    assert_eq!(layout.box_for(order_a).unwrap().width, 10);
    assert_eq!(layout.box_for(order_b).unwrap().width, 14);
    assert_eq!(layout.box_for(width_only).unwrap().width, 8);
    assert_eq!(layout.box_for(inline).unwrap().width, 16);
    for node_id in [
        shorthand, sides, repeat, fallback, same, order_a, order_b, width_only, inline,
    ] {
        let rectangle = layout.box_for(node_id).expect("border-width layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    for node_id in [
        shorthand, sides, repeat, fallback, same, order_a, order_b, width_only, inline,
    ] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected border-width fixture node {node_id:?} to remain visible"
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
    let shorthand_border = border_for(shorthand).expect("shorthand border command");
    assert_eq!(shorthand_border.top.width, 1);
    assert_eq!(shorthand_border.right.width, 6);
    assert_eq!(shorthand_border.bottom.width, 7);
    assert_eq!(shorthand_border.left.width, 8);
    assert_eq!(shorthand_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(shorthand_border.top.color, NativeColor::RED);

    let sides_border = border_for(sides).expect("physical width longhand command");
    assert_eq!(sides_border.top.width, 4);
    assert_eq!(sides_border.right.width, 2);
    assert_eq!(sides_border.bottom.width, 6);
    assert_eq!(sides_border.left.width, 8);
    assert_eq!(sides_border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(sides_border.top.color, NativeColor::RED);

    assert_eq!(border_for(repeat).unwrap().top.width, 1);
    assert_eq!(border_for(fallback).unwrap().top.width, 1);
    assert!(border_for(same).is_none());
    assert_eq!(border_for(order_a).unwrap().top.width, 1);
    assert_eq!(border_for(order_b).unwrap().top.width, 3);
    assert!(border_for(width_only).is_none());
    assert_eq!(border_for(inline).unwrap().top.width, 4);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border-width"
                    | "border-top-width"
                    | "border-right-width"
                    | "border-bottom-width"
                    | "border-left-width"
            )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 7), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_border_style_revert_layer_composes_components_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #shorthand { display:block;width:8px;height:6px;border:1px solid red; } #sides { display:block;width:8px;height:6px;border:2px dashed red; } #repeat { display:block;width:8px;height:6px;border:1px solid red; } #fallback { display:block;width:8px;height:6px;border:1px solid red; } #same { display:block;width:8px;height:6px;border:1px solid red;border-style:revert-layer; } #order-a { display:block;width:8px;height:6px;border-style:dotted;border:1px solid red; } #order-b { display:block;width:8px;height:6px;border:1px solid red;border-style:dashed; } #combined { display:block;width:8px;height:6px;border-width:4px;border-color:green; } #style-only { display:block;width:8px;height:6px;border-style:dashed; } #inline { display:block;width:8px;height:6px;border:1px solid red; } } @layer theme { #shorthand { border-style:dashed dotted solid dashed; } #shorthand { border-top-style:revert-layer; } #sides { border-top-style:solid;border-right-style:revert-layer;border-bottom-style:dotted;border-left-style:solid; } #repeat { border-style:dotted; } #repeat { border-style:revert-layer; } #fallback { border-style:dashed; } #fallback { border-style:revert-layer; } #combined { border-style:dashed; } } @layer top { #repeat { border-style:revert-layer; } #fallback { border-style:revert-layer; } } #shorthand { border-style:revert-layer; } #repeat { border-style:revert-layer; } #fallback { border-style:revert-layer; }</style><button id='shorthand'>Shorthand</button><button id='sides'>Sides</button><button id='repeat'>Repeat</button><button id='fallback'>Fallback</button><button id='same'>Same</button><button id='order-a'>Order A</button><button id='order-b'>Order B</button><button id='combined'>Combined</button><button id='style-only'>Style only</button><button id='inline' style='border-style:dotted'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let shorthand = document.resolve_target("id=shorthand").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let same = document.resolve_target("id=same").unwrap();
    let order_a = document.resolve_target("id=order-a").unwrap();
    let order_b = document.resolve_target("id=order-b").unwrap();
    let combined = document.resolve_target("id=combined").unwrap();
    let style_only = document.resolve_target("id=style-only").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(shorthand).unwrap().width, 10);
    assert_eq!(layout.box_for(shorthand).unwrap().height, 8);
    assert_eq!(layout.box_for(sides).unwrap().width, 12);
    assert_eq!(layout.box_for(sides).unwrap().height, 10);
    assert_eq!(layout.box_for(repeat).unwrap().width, 10);
    assert_eq!(layout.box_for(fallback).unwrap().width, 10);
    assert_eq!(layout.box_for(same).unwrap().width, 8);
    assert_eq!(layout.box_for(same).unwrap().height, 6);
    assert_eq!(layout.box_for(order_a).unwrap().width, 10);
    assert_eq!(layout.box_for(order_b).unwrap().width, 10);
    assert_eq!(layout.box_for(combined).unwrap().width, 16);
    assert_eq!(layout.box_for(combined).unwrap().height, 14);
    assert_eq!(layout.box_for(style_only).unwrap().width, 8);
    assert_eq!(layout.box_for(inline).unwrap().width, 10);
    for node_id in [
        shorthand, sides, repeat, fallback, same, order_a, order_b, combined, style_only, inline,
    ] {
        let rectangle = layout.box_for(node_id).expect("border-style layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    for node_id in [
        shorthand, sides, repeat, fallback, same, order_a, order_b, combined, style_only, inline,
    ] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected border-style fixture node {node_id:?} to remain visible"
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
    let shorthand_border = border_for(shorthand).expect("shorthand border command");
    assert_eq!(shorthand_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(shorthand_border.right.style, NativeBorderStyle::Dotted);
    assert_eq!(shorthand_border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(shorthand_border.left.style, NativeBorderStyle::Dashed);
    assert_eq!(shorthand_border.top.width, 1);
    assert_eq!(shorthand_border.top.color, NativeColor::RED);

    let sides_border = border_for(sides).expect("physical style longhand command");
    assert_eq!(sides_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.right.style, NativeBorderStyle::Dashed);
    assert_eq!(sides_border.bottom.style, NativeBorderStyle::Dotted);
    assert_eq!(sides_border.left.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.top.width, 2);

    assert_eq!(
        border_for(repeat).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert_eq!(
        border_for(fallback).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert!(border_for(same).is_none());
    assert_eq!(
        border_for(order_a).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert_eq!(
        border_for(order_b).unwrap().top.style,
        NativeBorderStyle::Dashed
    );
    let combined_border = border_for(combined).expect("separate component border command");
    assert_eq!(combined_border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(combined_border.top.width, 4);
    assert_eq!(
        combined_border.top.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }
    );
    assert!(border_for(style_only).is_none());
    assert_eq!(
        border_for(inline).unwrap().top.style,
        NativeBorderStyle::Dotted
    );
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border-style"
                    | "border-top-style"
                    | "border-right-style"
                    | "border-bottom-style"
                    | "border-left-style"
            )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 7), Some([255, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_border_style_none_blocks_or_reveals_components_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #mixed { display:block;width:8px;height:6px;border:2px solid red; } #blocked { display:block;width:8px;height:6px;border:2px solid red; } #exposed { display:block;width:8px;height:6px;border:2px solid red; } #repeated { display:block;width:8px;height:6px;border:2px solid red; } #invalid { display:block;width:8px;height:6px;border:2px solid red; } #physical { display:block;width:8px;height:6px;border:2px solid red; } #style-only { display:block;width:8px;height:6px;border-style:none; } #inline { display:block;width:8px;height:6px;border:2px solid red; } #inline-paint { display:block;width:8px;height:6px;border:2px solid red;border-style:none; } } @layer theme { #mixed { border-style:none dashed dotted none; } #blocked { border-style:none; } #exposed { border-style:none;border-style:revert-layer; } #repeated { border-style:none;border-style:revert-layer;border-style:revert-layer; } #invalid { border-style:none;border-style:invalid; } #physical { border-top-style:none;border-right-style:dashed;border-bottom-style:dotted;border-left-style:solid; } #inline-paint { border-style:none; } } #blocked { border-style:revert-layer; }</style><button id='mixed'>Mixed</button><button id='blocked'>Blocked</button><button id='exposed'>Exposed</button><button id='repeated'>Repeated</button><button id='invalid'>Invalid</button><button id='physical'>Physical</button><button id='style-only'>Style only</button><button id='inline' style='border-style:none'>Inline</button><button id='inline-paint' style='border-style:solid'>Inline paint</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let mixed = document.resolve_target("id=mixed").unwrap();
    let blocked = document.resolve_target("id=blocked").unwrap();
    let exposed = document.resolve_target("id=exposed").unwrap();
    let repeated = document.resolve_target("id=repeated").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let style_only = document.resolve_target("id=style-only").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let inline_paint = document.resolve_target("id=inline-paint").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(mixed).unwrap().width, 10);
    assert_eq!(layout.box_for(mixed).unwrap().height, 8);
    assert_eq!(layout.box_for(blocked).unwrap().width, 8);
    assert_eq!(layout.box_for(blocked).unwrap().height, 6);
    assert_eq!(layout.box_for(exposed).unwrap().width, 12);
    assert_eq!(layout.box_for(exposed).unwrap().height, 10);
    assert_eq!(layout.box_for(repeated).unwrap().width, 12);
    assert_eq!(layout.box_for(repeated).unwrap().height, 10);
    assert_eq!(layout.box_for(invalid).unwrap().width, 8);
    assert_eq!(layout.box_for(invalid).unwrap().height, 6);
    assert_eq!(layout.box_for(physical).unwrap().width, 12);
    assert_eq!(layout.box_for(physical).unwrap().height, 8);
    assert_eq!(layout.box_for(style_only).unwrap().width, 8);
    assert_eq!(layout.box_for(style_only).unwrap().height, 6);
    assert_eq!(layout.box_for(inline).unwrap().width, 8);
    assert_eq!(layout.box_for(inline).unwrap().height, 6);
    assert_eq!(layout.box_for(inline_paint).unwrap().width, 12);
    assert_eq!(layout.box_for(inline_paint).unwrap().height, 10);
    for node_id in [
        mixed,
        blocked,
        exposed,
        repeated,
        invalid,
        physical,
        style_only,
        inline,
        inline_paint,
    ] {
        let rectangle = layout
            .box_for(node_id)
            .expect("border-style:none layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    for node_id in [
        mixed,
        blocked,
        exposed,
        repeated,
        invalid,
        physical,
        style_only,
        inline,
        inline_paint,
    ] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected border-style:none fixture node {node_id:?} to remain visible"
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
    let mixed_border = border_for(mixed).expect("mixed border command");
    assert_eq!(mixed_border.top.width, 0);
    assert_eq!(mixed_border.right.width, 2);
    assert_eq!(mixed_border.right.style, NativeBorderStyle::Dashed);
    assert_eq!(mixed_border.bottom.width, 2);
    assert_eq!(mixed_border.bottom.style, NativeBorderStyle::Dotted);
    assert_eq!(mixed_border.left.width, 0);
    assert!(border_for(blocked).is_none());
    assert!(border_for(invalid).is_none());
    assert!(border_for(style_only).is_none());
    assert!(border_for(inline).is_none());
    assert!(border_for(exposed).is_some());
    assert!(border_for(repeated).is_some());
    let physical_border = border_for(physical).expect("physical border command");
    assert_eq!(physical_border.top.width, 0);
    assert_eq!(physical_border.right.style, NativeBorderStyle::Dashed);
    assert_eq!(physical_border.bottom.style, NativeBorderStyle::Dotted);
    assert_eq!(physical_border.left.style, NativeBorderStyle::Solid);
    let inline_paint_border = border_for(inline_paint).expect("inline border command");
    assert_eq!(inline_paint_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(inline_paint_border.top.width, 2);

    let style_diagnostic_count = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border-style"
        })
        .count();
    assert_eq!(style_diagnostic_count, 1);

    let surface = list.rasterize().unwrap();
    let physical_rect = layout.box_for(physical).unwrap();
    assert_eq!(
        surface.pixel(physical_rect.x, physical_rect.y + 1),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_border_style_hidden_blocks_or_reveals_components_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #mixed { display:block;width:8px;height:6px;border:2px solid red; } #blocked { display:block;width:8px;height:6px;border:2px solid red; } #exposed { display:block;width:8px;height:6px;border:2px solid red; } #repeated { display:block;width:8px;height:6px;border:2px solid red; } #invalid { display:block;width:8px;height:6px;border:2px solid red; } #physical { display:block;width:8px;height:6px;border:2px solid red; } #style-only { display:block;width:8px;height:6px;border-style:hidden; } #components { display:block;width:8px;height:6px;border:2px solid red; } #order-solid { display:block;width:8px;height:6px;border:2px solid red; border-style:hidden; border-style:solid; } #order-hidden { display:block;width:8px;height:6px;border:2px solid red; border-style:solid; border-style:hidden; } #inline { display:block;width:8px;height:6px;border:2px solid red; } #inline-paint { display:block;width:8px;height:6px;border:2px solid red; } } @layer theme { #mixed { border-style:hidden dashed dotted hidden; } #blocked { border-style:hidden; } #exposed { border-style:hidden;border-style:revert-layer; } #repeated { border-style:hidden;border-style:revert-layer;border-style:revert-layer; } #invalid { border-style:hidden;border-style:invalid; } #physical { border-top-style:hidden;border-right-style:dashed;border-bottom-style:dotted;border-left-style:solid; } #components { border-style:hidden; border-width:4px; border-color:blue; } } #blocked { border-style:revert-layer; }</style><button id='mixed'>Mixed</button><button id='blocked'>Blocked</button><button id='exposed'>Exposed</button><button id='repeated'>Repeated</button><button id='invalid'>Invalid</button><button id='physical'>Physical</button><button id='style-only'>Style only</button><button id='components'>Components</button><button id='order-solid'>Order solid</button><button id='order-hidden'>Order hidden</button><button id='inline' style='border-style:hidden'>Inline</button><button id='inline-paint' style='border-style:solid'>Inline paint</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 220,
        device_scale_factor_milli: 1000,
    };
    let ids = [
        "mixed",
        "blocked",
        "exposed",
        "repeated",
        "invalid",
        "physical",
        "style-only",
        "components",
        "order-solid",
        "order-hidden",
        "inline",
        "inline-paint",
    ];
    let nodes = ids.map(|id| document.resolve_target(&format!("id={id}")).unwrap());
    let layout = document.layout(viewport).unwrap();
    let expected_sizes = [
        (10, 8),
        (8, 6),
        (12, 10),
        (12, 10),
        (8, 6),
        (12, 8),
        (8, 6),
        (8, 6),
        (12, 10),
        (8, 6),
        (8, 6),
        (12, 10),
    ];
    for (node_id, (width, height)) in nodes.iter().zip(expected_sizes) {
        let rectangle = layout.box_for(*node_id).expect("hidden border layout box");
        assert_eq!((rectangle.width, rectangle.height), (width, height));
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(*node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    let positions: Vec<_> = nodes
        .iter()
        .map(|node_id| {
            semantic_nodes
                .iter()
                .position(|node| node.node_id == *node_id && !node.hidden)
                .expect("visible hidden-border semantic node")
        })
        .collect();
    assert!(positions.windows(2).all(|window| window[0] < window[1]));

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
    let mixed_border = border_for(nodes[0]).expect("mixed hidden border command");
    assert_eq!(mixed_border.top.width, 0);
    assert_eq!(mixed_border.right.style, NativeBorderStyle::Dashed);
    assert_eq!(mixed_border.bottom.style, NativeBorderStyle::Dotted);
    assert_eq!(mixed_border.left.width, 0);
    for index in [1, 4, 6, 7, 9, 10] {
        assert!(
            border_for(nodes[index]).is_none(),
            "unexpected hidden border command {index}"
        );
    }
    for index in [2, 3, 8, 11] {
        assert!(
            border_for(nodes[index]).is_some(),
            "missing revealed border command {index}"
        );
    }
    let physical_border = border_for(nodes[5]).expect("physical hidden border command");
    assert_eq!(physical_border.top.width, 0);
    assert_eq!(physical_border.right.style, NativeBorderStyle::Dashed);
    assert_eq!(physical_border.bottom.style, NativeBorderStyle::Dotted);
    assert_eq!(physical_border.left.style, NativeBorderStyle::Solid);
    assert_eq!(
        border_for(nodes[11]).unwrap().top.style,
        NativeBorderStyle::Solid
    );

    let invalid_diagnostics = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border-style"
        })
        .count();
    assert_eq!(invalid_diagnostics, 1);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail.contains("hidden")
    }));

    let surface = list.rasterize().unwrap();
    let physical_rect = layout.box_for(nodes[5]).unwrap();
    assert_eq!(
        surface.pixel(physical_rect.x, physical_rect.y + 1),
        Some([255, 0, 0, 255])
    );
    assert_ne!(
        surface.pixel(physical_rect.x + 3, physical_rect.y),
        Some([255, 0, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (48, 220));
}
#[test]
fn native_painted_border_style_variants_replay_and_cascade() {
    let document = NativeDocument::parse(
        "<style>@layer base { #double { display:block;width:8px;height:6px;border:6px solid red; } #groove { display:block;width:8px;height:6px;border:6px solid red; } #ridge { display:block;width:8px;height:6px;border:6px solid red; } #inset { display:block;width:8px;height:6px;border:6px solid red; } #outset { display:block;width:8px;height:6px;border:6px solid red; } #longhands { display:block;width:8px;height:6px;border:6px solid red; } #complete { display:block;width:8px;height:6px; } #rollback { display:block;width:8px;height:6px;border:6px solid red; } #repeated { display:block;width:8px;height:6px;border:6px solid red; } #invalid { display:block;width:8px;height:6px;border:6px solid red; } #inline { display:block;width:8px;height:6px;border:6px solid red; } } @layer theme { #double { border-style:double; } #groove { border-style:groove; } #ridge { border-style:RIDGE; } #inset { border-style:inset; } #outset { border-style:OuTsEt; } #longhands { border-style:double groove ridge inset; } #complete { border:6px double blue; } #rollback { border-style:double;border-style:revert-layer; } #repeated { border-style:groove;border-style:revert-layer;border-style:revert-layer; } #invalid { border-style:double;border-style:invalid; } } #inline { border-style:outset; }</style><button id='double'>Double</button><button id='groove'>Groove</button><button id='ridge'>Ridge</button><button id='inset'>Inset</button><button id='outset'>Outset</button><button id='longhands'>Longhands</button><button id='complete'>Complete</button><button id='rollback'>Rollback</button><button id='repeated'>Repeated</button><button id='invalid'>Invalid</button><button id='inline'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 300,
        device_scale_factor_milli: 1000,
    };
    let ids = [
        "double",
        "groove",
        "ridge",
        "inset",
        "outset",
        "longhands",
        "complete",
        "rollback",
        "repeated",
        "invalid",
        "inline",
    ];
    let nodes = ids.map(|id| document.resolve_target(&format!("id={id}")).unwrap());
    let layout = document.layout(viewport).unwrap();
    for node_id in nodes {
        let rectangle = layout.box_for(node_id).expect("painted border layout box");
        assert_eq!((rectangle.width, rectangle.height), (20, 18));
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    let positions: Vec<_> = nodes
        .iter()
        .map(|node_id| {
            semantic_nodes
                .iter()
                .position(|node| node.node_id == *node_id && !node.hidden)
                .expect("visible painted-border semantic node")
        })
        .collect();
    assert!(positions.windows(2).all(|window| window[0] < window[1]));

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
    assert_eq!(
        border_for(nodes[0]).unwrap().top.style,
        NativeBorderStyle::Double
    );
    assert_eq!(
        border_for(nodes[1]).unwrap().top.style,
        NativeBorderStyle::Groove
    );
    assert_eq!(
        border_for(nodes[2]).unwrap().top.style,
        NativeBorderStyle::Ridge
    );
    assert_eq!(
        border_for(nodes[3]).unwrap().top.style,
        NativeBorderStyle::Inset
    );
    assert_eq!(
        border_for(nodes[4]).unwrap().top.style,
        NativeBorderStyle::Outset
    );
    let longhands = border_for(nodes[5]).unwrap();
    assert_eq!(longhands.top.style, NativeBorderStyle::Double);
    assert_eq!(longhands.right.style, NativeBorderStyle::Groove);
    assert_eq!(longhands.bottom.style, NativeBorderStyle::Ridge);
    assert_eq!(longhands.left.style, NativeBorderStyle::Inset);
    assert_eq!(
        border_for(nodes[6]).unwrap().top.style,
        NativeBorderStyle::Double
    );
    assert_eq!(
        border_for(nodes[7]).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert_eq!(
        border_for(nodes[8]).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert_eq!(
        border_for(nodes[9]).unwrap().top.style,
        NativeBorderStyle::Double
    );
    assert_eq!(
        border_for(nodes[10]).unwrap().top.style,
        NativeBorderStyle::Outset
    );
    assert_eq!(
        border_for(nodes[6]).unwrap().top.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );

    let invalid_diagnostics = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border-style"
        })
        .count();
    assert_eq!(invalid_diagnostics, 1);

    let surface = list.rasterize().unwrap();
    let double_rect = layout.box_for(nodes[0]).unwrap();
    assert_eq!(
        surface.pixel(double_rect.x + 10, double_rect.y),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(double_rect.x + 10, double_rect.y + 2),
        Some([255, 255, 255, 255])
    );
    let groove_rect = layout.box_for(nodes[1]).unwrap();
    assert_eq!(
        surface.pixel(groove_rect.x + 10, groove_rect.y),
        Some([127, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(groove_rect.x + 10, groove_rect.y + 4),
        Some([255, 127, 127, 255])
    );
    let ridge_rect = layout.box_for(nodes[2]).unwrap();
    assert_eq!(
        surface.pixel(ridge_rect.x + 10, ridge_rect.y),
        Some([255, 127, 127, 255])
    );
    let inset_rect = layout.box_for(nodes[3]).unwrap();
    assert_eq!(
        surface.pixel(inset_rect.x + 10, inset_rect.y),
        Some([127, 0, 0, 255])
    );
    let outset_rect = layout.box_for(nodes[4]).unwrap();
    assert_eq!(
        surface.pixel(outset_rect.x + 10, outset_rect.y),
        Some([255, 127, 127, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (64, 300));
}
#[test]
fn native_border_none_shorthand_blocks_or_reveals_components_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #shorthand { display:block;width:8px;height:6px;border:2px solid red; } #physical { display:block;width:8px;height:6px;border:2px solid red; } #components { display:block;width:8px;height:6px;border:2px solid red; } #exposed { display:block;width:8px;height:6px;border:2px solid red; } #repeated { display:block;width:8px;height:6px;border:2px solid red; } #invalid { display:block;width:8px;height:6px;border:2px solid red; } #order-a { display:block;width:8px;height:6px;border:2px solid red; } #order-b { display:block;width:8px;height:6px;border:2px solid red; } #inline { display:block;width:8px;height:6px;border:2px solid red; } } @layer theme { #shorthand { border: NoNe; } #physical { border-top: none; border-right: NONE; } #components { border: none; border-width: 4px; border-color: blue; } #exposed { border: none; } #exposed { border: revert-layer; } #repeated { border: none; } #repeated { border: revert-layer; } #repeated { border: revert-layer; } #invalid { border: none; } #invalid { border: unsupported; } #order-a { border: none; border-style: solid; } #order-b { border-style: solid; border: none; } } @layer top { #repeated { border: revert-layer; } } #inline { border: NONE; }</style><button id='shorthand'>Shorthand</button><button id='physical'></button><button id='components'>Components</button><button id='exposed'>Exposed</button><button id='repeated'>Repeated</button><button id='invalid'>Invalid</button><button id='order-a'>Order A</button><button id='order-b'>Order B</button><button id='inline'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let ids = [
        "shorthand",
        "physical",
        "components",
        "exposed",
        "repeated",
        "invalid",
        "order-a",
        "order-b",
        "inline",
    ];
    let nodes = ids.map(|id| document.resolve_target(&format!("id={id}")).unwrap());
    let layout = document.layout(viewport).unwrap();
    let expected_sizes = [
        (8, 6),
        (10, 8),
        (8, 6),
        (12, 10),
        (12, 10),
        (8, 6),
        (12, 10),
        (8, 6),
        (8, 6),
    ];
    for (node_id, (width, height)) in nodes.iter().zip(expected_sizes) {
        let rectangle = layout.box_for(*node_id).expect("border:none layout box");
        assert_eq!((rectangle.width, rectangle.height), (width, height));
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(*node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    let positions: Vec<_> = nodes
        .iter()
        .map(|node_id| {
            semantic_nodes
                .iter()
                .position(|node| node.node_id == *node_id && !node.hidden)
                .expect("visible border:none semantic node")
        })
        .collect();
    assert!(positions.windows(2).all(|window| window[0] < window[1]));

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
    for index in [0, 2, 5, 7, 8] {
        assert!(
            border_for(nodes[index]).is_none(),
            "unexpected border command {index}"
        );
    }
    for index in [3, 4, 6] {
        assert!(
            border_for(nodes[index]).is_some(),
            "missing border command {index}"
        );
    }
    let physical = border_for(nodes[1]).expect("physical border command");
    assert_eq!(physical.top.width, 0);
    assert_eq!(physical.right.width, 0);
    assert_eq!(physical.bottom.width, 2);
    assert_eq!(physical.left.width, 2);
    assert_eq!(
        border_for(nodes[3]).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert_eq!(
        border_for(nodes[6]).unwrap().top.style,
        NativeBorderStyle::Solid
    );

    let border_diagnostics = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border"
        })
        .count();
    assert_eq!(border_diagnostics, 1);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail.contains("none")
    }));

    let surface = list.rasterize().unwrap();
    let physical_rect = layout.box_for(nodes[1]).unwrap();
    assert_eq!(
        surface.pixel(physical_rect.x + physical_rect.width / 2, physical_rect.y),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(
            physical_rect.x + physical_rect.width / 2,
            physical_rect.y + physical_rect.height - 1,
        ),
        Some([255, 0, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (48, 180));
}
#[test]
fn native_border_hidden_shorthand_blocks_or_reveals_components_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #shorthand { display:block;width:8px;height:6px;border:2px solid red; } #physical { display:block;width:8px;height:6px;border:2px solid red; } #components { display:block;width:8px;height:6px;border:2px solid red; } #exposed { display:block;width:8px;height:6px;border:2px solid red; } #repeated { display:block;width:8px;height:6px;border:2px solid red; } #invalid { display:block;width:8px;height:6px;border:2px solid red; } #order-a { display:block;width:8px;height:6px;border:2px solid red; } #order-b { display:block;width:8px;height:6px;border:2px solid red; } #inline { display:block;width:8px;height:6px;border:2px solid red; } } @layer theme { #shorthand { border: HiDdEn; } #physical { border-top: hidden; border-right: HIDDEN; } #components { border: hidden; border-width: 4px; border-color: blue; } #exposed { border: hidden; } #exposed { border: revert-layer; } #repeated { border: hidden; } #repeated { border: revert-layer; } #repeated { border: revert-layer; } #invalid { border: hidden; } #invalid { border: unsupported; } #order-a { border: hidden; border-style: solid; } #order-b { border-style: solid; border: hidden; } } @layer top { #repeated { border: revert-layer; } } #inline { border: HIDDEN; }</style><button id='shorthand'>Shorthand</button><button id='physical'></button><button id='components'>Components</button><button id='exposed'>Exposed</button><button id='repeated'>Repeated</button><button id='invalid'>Invalid</button><button id='order-a'>Order A</button><button id='order-b'>Order B</button><button id='inline'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let ids = [
        "shorthand",
        "physical",
        "components",
        "exposed",
        "repeated",
        "invalid",
        "order-a",
        "order-b",
        "inline",
    ];
    let nodes = ids.map(|id| document.resolve_target(&format!("id={id}")).unwrap());
    let layout = document.layout(viewport).unwrap();
    let expected_sizes = [
        (8, 6),
        (10, 8),
        (8, 6),
        (12, 10),
        (12, 10),
        (8, 6),
        (12, 10),
        (8, 6),
        (8, 6),
    ];
    for (node_id, (width, height)) in nodes.iter().zip(expected_sizes) {
        let rectangle = layout.box_for(*node_id).expect("border:hidden layout box");
        assert_eq!((rectangle.width, rectangle.height), (width, height));
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(*node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    let positions: Vec<_> = nodes
        .iter()
        .map(|node_id| {
            semantic_nodes
                .iter()
                .position(|node| node.node_id == *node_id && !node.hidden)
                .expect("visible border:hidden semantic node")
        })
        .collect();
    assert!(positions.windows(2).all(|window| window[0] < window[1]));

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
    for index in [0, 2, 5, 7, 8] {
        assert!(
            border_for(nodes[index]).is_none(),
            "unexpected border command {index}"
        );
    }
    for index in [3, 4, 6] {
        assert!(
            border_for(nodes[index]).is_some(),
            "missing border command {index}"
        );
    }
    let physical = border_for(nodes[1]).expect("physical border command");
    assert_eq!(physical.top.width, 0);
    assert_eq!(physical.right.width, 0);
    assert_eq!(physical.bottom.width, 2);
    assert_eq!(physical.left.width, 2);
    assert_eq!(
        border_for(nodes[3]).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert_eq!(
        border_for(nodes[6]).unwrap().top.style,
        NativeBorderStyle::Solid
    );

    let border_diagnostics = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border"
        })
        .count();
    assert_eq!(border_diagnostics, 1);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail.contains("hidden")
    }));

    let surface = list.rasterize().unwrap();
    let physical_rect = layout.box_for(nodes[1]).unwrap();
    assert_eq!(
        surface.pixel(physical_rect.x + physical_rect.width / 2, physical_rect.y),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(
            physical_rect.x + physical_rect.width / 2,
            physical_rect.y + physical_rect.height - 1,
        ),
        Some([255, 0, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (48, 180));
}
#[test]
fn native_complete_border_hidden_preserves_components_and_suppresses_paint() {
    let document = NativeDocument::parse(
        "<style>@layer base { #shorthand { display:block;width:8px;height:6px;border:2px solid red; } #physical { display:block;width:8px;height:6px;border:2px solid red; } #components { display:block;width:8px;height:6px;border:2px solid red; } #exposed { display:block;width:8px;height:6px;border:2px solid red; } #repeated { display:block;width:8px;height:6px;border:2px solid red; } #invalid { display:block;width:8px;height:6px;border:2px solid red; } #order-a { display:block;width:8px;height:6px;border:2px solid red; } #order-b { display:block;width:8px;height:6px;border:2px solid red; } #inline { display:block;width:8px;height:6px;border:2px solid red; } } @layer theme { #shorthand { border: 4px HiDdEn blue; } #physical { border-top: 3px hidden green; border-right: 4px HIDDEN blue; } #components { border: 4px hidden blue; border-width: 5px; border-color: green; } #exposed { border: 4px hidden blue; border-style: revert-layer; } #repeated { border: 4px hidden blue; } #repeated { border-style: revert-layer; } #repeated { border-style: revert-layer; } #invalid { border: 4px hidden blue; } #invalid { border: unsupported; } #order-a { border: 4px hidden blue; border-style: solid; } #order-b { border-style: solid; border: 4px hidden blue; } } @layer top { #repeated { border-style: revert-layer; } } #inline { border: 4px hidden blue; }</style><button id='shorthand'>Shorthand</button><button id='physical'></button><button id='components'>Components</button><button id='exposed'>Exposed</button><button id='repeated'>Repeated</button><button id='invalid'>Invalid</button><button id='order-a'>Order A</button><button id='order-b'>Order B</button><button id='inline'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let ids = [
        "shorthand",
        "physical",
        "components",
        "exposed",
        "repeated",
        "invalid",
        "order-a",
        "order-b",
        "inline",
    ];
    let nodes = ids.map(|id| document.resolve_target(&format!("id={id}")).unwrap());
    let layout = document.layout(viewport).unwrap();
    let expected_sizes = [
        (8, 6),
        (10, 8),
        (8, 6),
        (16, 14),
        (16, 14),
        (8, 6),
        (16, 14),
        (8, 6),
        (8, 6),
    ];
    for (node_id, (width, height)) in nodes.iter().zip(expected_sizes) {
        let rectangle = layout
            .box_for(*node_id)
            .expect("complete hidden layout box");
        assert_eq!((rectangle.width, rectangle.height), (width, height));
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(*node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    let positions: Vec<_> = nodes
        .iter()
        .map(|node_id| {
            semantic_nodes
                .iter()
                .position(|node| node.node_id == *node_id && !node.hidden)
                .expect("visible complete hidden semantic node")
        })
        .collect();
    assert!(positions.windows(2).all(|window| window[0] < window[1]));

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
    for index in [0, 2, 5, 7, 8] {
        assert!(
            border_for(nodes[index]).is_none(),
            "unexpected border command {index}"
        );
    }
    for index in [1, 3, 4, 6] {
        assert!(
            border_for(nodes[index]).is_some(),
            "missing border command {index}"
        );
    }
    let physical = border_for(nodes[1]).expect("physical border command");
    assert_eq!(physical.top.width, 0);
    assert_eq!(physical.right.width, 0);
    assert_eq!(physical.bottom.width, 2);
    assert_eq!(physical.left.width, 2);
    let exposed = border_for(nodes[3]).expect("reopened complete hidden border");
    assert_eq!(exposed.top.width, 4);
    assert_eq!(exposed.top.style, NativeBorderStyle::Solid);
    assert_eq!(
        exposed.top.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );
    let repeated = border_for(nodes[4]).expect("repeated rollback border");
    assert_eq!(repeated.top.width, 4);
    assert_eq!(repeated.top.style, NativeBorderStyle::Solid);
    assert_eq!(repeated.top.color, exposed.top.color);
    let order_a = border_for(nodes[6]).expect("same-block style override border");
    assert_eq!(order_a.top.width, 4);
    assert_eq!(order_a.top.style, NativeBorderStyle::Solid);
    assert_eq!(order_a.top.color, exposed.top.color);

    let border_diagnostics = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border"
        })
        .count();
    assert_eq!(border_diagnostics, 1);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail.contains("hidden")
    }));

    let surface = list.rasterize().unwrap();
    let physical_rect = layout.box_for(nodes[1]).unwrap();
    assert_eq!(
        surface.pixel(physical_rect.x + physical_rect.width / 2, physical_rect.y),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(
            physical_rect.x + physical_rect.width / 2,
            physical_rect.y + physical_rect.height - 1,
        ),
        Some([255, 0, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (48, 180));
}
#[test]
fn native_complete_border_none_preserves_components_and_suppresses_paint() {
    let document = NativeDocument::parse(
        "<style>@layer base { #shorthand { display:block;width:8px;height:6px;border:2px solid red; } #physical { display:block;width:8px;height:6px;border:2px solid red; } #components { display:block;width:8px;height:6px;border:2px solid red; } #exposed { display:block;width:8px;height:6px;border:2px solid red; } #repeated { display:block;width:8px;height:6px;border:2px solid red; } #invalid { display:block;width:8px;height:6px;border:2px solid red; } #order-a { display:block;width:8px;height:6px;border:2px solid red; } #order-b { display:block;width:8px;height:6px;border:2px solid red; } #inline { display:block;width:8px;height:6px;border:2px solid red; } } @layer theme { #shorthand { border: 4px NoNe blue; } #physical { border-top: 3px none green; border-right: 4px NoNe blue; } #components { border: 4px none blue; border-width: 5px; border-color: green; } #exposed { border: 4px none blue; border-style: revert-layer; } #repeated { border: 4px none blue; } #repeated { border-style: revert-layer; } #repeated { border-style: revert-layer; } #invalid { border: 4px none blue; } #invalid { border: unsupported; } #order-a { border: 4px none blue; border-style: solid; } #order-b { border-style: solid; border: 4px none blue; } } @layer top { #repeated { border: revert-layer; } } #inline { border: 4px none blue; }</style><button id='shorthand'>Shorthand</button><button id='physical'></button><button id='components'>Components</button><button id='exposed'>Exposed</button><button id='repeated'>Repeated</button><button id='invalid'>Invalid</button><button id='order-a'>Order A</button><button id='order-b'>Order B</button><button id='inline'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 48,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let ids = [
        "shorthand",
        "physical",
        "components",
        "exposed",
        "repeated",
        "invalid",
        "order-a",
        "order-b",
        "inline",
    ];
    let nodes = ids.map(|id| document.resolve_target(&format!("id={id}")).unwrap());
    let layout = document.layout(viewport).unwrap();
    let expected_sizes = [
        (8, 6),
        (10, 8),
        (8, 6),
        (16, 14),
        (16, 14),
        (8, 6),
        (16, 14),
        (8, 6),
        (8, 6),
    ];
    for (node_id, (width, height)) in nodes.iter().zip(expected_sizes) {
        let rectangle = layout.box_for(*node_id).expect("complete none layout box");
        assert_eq!((rectangle.width, rectangle.height), (width, height));
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(*node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    let positions: Vec<_> = nodes
        .iter()
        .map(|node_id| {
            semantic_nodes
                .iter()
                .position(|node| node.node_id == *node_id && !node.hidden)
                .expect("visible complete none semantic node")
        })
        .collect();
    assert!(positions.windows(2).all(|window| window[0] < window[1]));

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
    for index in [0, 2, 5, 7, 8] {
        assert!(
            border_for(nodes[index]).is_none(),
            "unexpected border command {index}"
        );
    }
    for index in [1, 3, 4, 6] {
        assert!(
            border_for(nodes[index]).is_some(),
            "missing border command {index}"
        );
    }
    let physical = border_for(nodes[1]).expect("physical border command");
    assert_eq!(physical.top.width, 0);
    assert_eq!(physical.right.width, 0);
    assert_eq!(physical.bottom.width, 2);
    assert_eq!(physical.left.width, 2);
    let exposed = border_for(nodes[3]).expect("reopened complete none border");
    assert_eq!(exposed.top.width, 4);
    assert_eq!(exposed.top.style, NativeBorderStyle::Solid);
    assert_eq!(
        exposed.top.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );
    let repeated = border_for(nodes[4]).expect("repeated rollback border");
    assert_eq!(repeated.top.width, 4);
    assert_eq!(repeated.top.style, NativeBorderStyle::Solid);
    assert_eq!(repeated.top.color, exposed.top.color);
    let order_a = border_for(nodes[6]).expect("same-block style override border");
    assert_eq!(order_a.top.width, 4);
    assert_eq!(order_a.top.style, NativeBorderStyle::Solid);
    assert_eq!(order_a.top.color, exposed.top.color);

    let border_diagnostics = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                && diagnostic.detail == "border"
        })
        .count();
    assert_eq!(border_diagnostics, 1);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail.contains("none")
    }));

    let surface = list.rasterize().unwrap();
    let physical_rect = layout.box_for(nodes[1]).unwrap();
    assert_eq!(
        surface.pixel(physical_rect.x + physical_rect.width / 2, physical_rect.y),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(
            physical_rect.x + physical_rect.width / 2,
            physical_rect.y + physical_rect.height - 1,
        ),
        Some([255, 0, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (48, 180));
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

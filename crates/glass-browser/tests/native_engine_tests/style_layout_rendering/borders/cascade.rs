use super::*;

#[test]
fn native_border_revert_layer_preserves_side_geometry_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #card { display:block;width:8px;height:6px;border:1px solid red; } #sides { display:block;width:8px;height:6px;border-top:1px solid red;border-right:2px solid green;border-bottom:3px solid blue;border-left:4px solid black; } #fallback { display:block;width:8px;height:6px;border:revert-layer;background-color:yellow; } #inline { display:block;width:8px;height:6px;border:1px solid red; } } @layer theme { #card { border:3px dashed blue; } #card { border:revert-layer; } #sides { border-top:5px dotted green;border-right:6px solid blue;border-bottom:7px dashed black;border-left:8px solid red; } #sides { border-top:revert-layer;border-bottom:revert-layer; } #inline { border:3px solid green; } } #card { border:revert-layer; } #sides { border-top:revert-layer;border-right:revert-layer;border-left:revert-layer; }</style><button id='card'>Card</button><button id='sides'>Sides</button><button id='fallback'>Fallback</button><button id='inline' style='border:ReVeRt-LaYeR'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 40,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(card),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 10,
            height: 8,
        })
    );
    assert_eq!(
        layout.box_for(sides),
        Some(NativeRect {
            x: 0,
            y: 8,
            width: 22,
            height: 10,
        })
    );
    assert_eq!(
        layout.box_for(fallback),
        Some(NativeRect {
            x: 0,
            y: 18,
            width: 8,
            height: 6,
        })
    );
    assert_eq!(
        layout.box_for(inline),
        Some(NativeRect {
            x: 0,
            y: 24,
            width: 14,
            height: 12,
        })
    );
    assert_eq!(layout.hit_test(2, 2).unwrap(), Some(card));
    assert_eq!(layout.hit_test(10, 12).unwrap(), Some(sides));
    assert_eq!(layout.hit_test(1, 19).unwrap(), Some(fallback));
    assert_eq!(layout.hit_test(5, 28).unwrap(), Some(inline));

    let semantic_nodes = document.semantic_nodes();
    for node_id in [card, sides, fallback, inline] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected border fixture node {node_id:?} to remain visible"
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
    let card_border = border_for(card).expect("card border command");
    assert_eq!(card_border.top.width, 1);
    assert_eq!(card_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(card_border.top.color, NativeColor::RED);
    assert_eq!(card_border.right, card_border.top);
    assert_eq!(card_border.bottom, card_border.top);
    assert_eq!(card_border.left, card_border.top);

    let sides_border = border_for(sides).expect("side border command");
    assert_eq!(sides_border.top.width, 1);
    assert_eq!(sides_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.top.color, NativeColor::RED);
    assert_eq!(sides_border.right.width, 6);
    assert_eq!(sides_border.right.style, NativeBorderStyle::Solid);
    assert_eq!(
        sides_border.right.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );
    assert_eq!(sides_border.bottom.width, 3);
    assert_eq!(sides_border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(
        sides_border.bottom.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );
    assert_eq!(sides_border.left.width, 8);
    assert_eq!(sides_border.left.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.left.color, NativeColor::RED);
    assert!(border_for(fallback).is_none());

    let inline_border = border_for(inline).expect("inline border command");
    assert_eq!(inline_border.top.width, 3);
    assert_eq!(inline_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(
        inline_border.top.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }
    );
    assert_eq!(inline_border.right, inline_border.top);
    assert_eq!(inline_border.bottom, inline_border.top);
    assert_eq!(inline_border.left, inline_border.top);
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border" | "border-top" | "border-right" | "border-bottom" | "border-left"
            )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(10, 8), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(21, 12), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(10, 17), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(0, 12), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(5, 24), Some([0, 128, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_border_color_revert_layer_preserves_component_geometry_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>@layer base { #shorthand { display:block;width:8px;height:6px;border:1px solid red; } #sides { display:block;width:8px;height:6px;border:2px dashed red; } #repeat { display:block;width:8px;height:6px;border:1px solid red; } #fallback { display:block;width:8px;height:6px;border:1px solid red; } #same { display:block;width:8px;height:6px;border:1px solid red;border-color:revert-layer; } #inline { display:block;width:8px;height:6px;border:1px solid red; } #order-a { display:block;width:8px;height:6px;border-color:blue;border:1px solid red; } #order-b { display:block;width:8px;height:6px;border:1px solid red;border-color:blue; } } @layer theme { #shorthand { border-color:blue green blue black; } #sides { border-top-color:blue;border-right-color:green;border-bottom-color:blue;border-left-color:black; } #sides { border-top-color:revert-layer;border-bottom-color:revert-layer; } #repeat { border-color:blue; } #repeat { border-color:revert-layer; } #fallback { border-color:revert-layer; } #inline { border-color:blue; } } @layer top { #repeat { border-color:revert-layer; } } #shorthand { border-color:revert-layer; } #repeat { border-color:revert-layer; }</style><button id='shorthand'>Shorthand</button><button id='sides'>Sides</button><button id='repeat'>Repeat</button><button id='fallback'>Fallback</button><button id='same'>Same</button><button id='inline' style='border-color:revert-layer'>Inline</button><button id='order-a'>Order A</button><button id='order-b'>Order B</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 72,
        device_scale_factor_milli: 1000,
    };
    let shorthand = document.resolve_target("id=shorthand").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let same = document.resolve_target("id=same").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let order_a = document.resolve_target("id=order-a").unwrap();
    let order_b = document.resolve_target("id=order-b").unwrap();

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(shorthand).unwrap().width, 10);
    assert_eq!(layout.box_for(shorthand).unwrap().height, 8);
    assert_eq!(layout.box_for(sides).unwrap().width, 12);
    assert_eq!(layout.box_for(sides).unwrap().height, 10);
    for node_id in [
        shorthand, sides, repeat, fallback, same, inline, order_a, order_b,
    ] {
        let rectangle = layout.box_for(node_id).expect("border-color layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }

    let semantic_nodes = document.semantic_nodes();
    for node_id in [
        shorthand, sides, repeat, fallback, same, inline, order_a, order_b,
    ] {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected border-color fixture node {node_id:?} to remain visible"
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
    assert_eq!(
        shorthand_border.top.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255
        }
    );
    assert_eq!(
        shorthand_border.right.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255
        }
    );
    assert_eq!(
        shorthand_border.bottom.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255
        }
    );
    assert_eq!(shorthand_border.left.color, NativeColor::BLACK);
    assert_eq!(shorthand_border.top.width, 1);
    assert_eq!(shorthand_border.top.style, NativeBorderStyle::Solid);

    let sides_border = border_for(sides).expect("physical color longhand command");
    assert_eq!(sides_border.top.color, NativeColor::RED);
    assert_eq!(
        sides_border.right.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255
        }
    );
    assert_eq!(sides_border.bottom.color, NativeColor::RED);
    assert_eq!(sides_border.left.color, NativeColor::BLACK);
    assert_eq!(sides_border.top.width, 2);
    assert_eq!(sides_border.top.style, NativeBorderStyle::Dashed);

    assert_eq!(border_for(repeat).unwrap().top.color, NativeColor::RED);
    assert_eq!(border_for(fallback).unwrap().top.color, NativeColor::RED);
    assert_eq!(border_for(same).unwrap().top.color, NativeColor::BLACK);
    assert_eq!(
        border_for(inline).unwrap().top.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255
        }
    );
    assert_eq!(border_for(order_a).unwrap().top.color, NativeColor::RED);
    assert_eq!(
        border_for(order_b).unwrap().top.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255
        }
    );
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border-color"
                    | "border-top-color"
                    | "border-right-color"
                    | "border-bottom-color"
                    | "border-left-color"
            )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(0, 0), Some([0, 0, 255, 255]));
    assert_eq!(surface.pixel(9, 2), Some([0, 128, 0, 255]));
    assert_eq!(surface.pixel(0, 2), Some([0, 0, 0, 255]));
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_border_color_current_color_resolves_after_cascade_and_inheritance() {
    let document = NativeDocument::parse(
        "<style>@layer base { #parent { color:green; } #shorthand { display:block;width:8px;height:6px;color:blue;border:1px solid red;border-color:CURRENTcolor green currentColor black; } #sides { display:block;width:8px;height:6px;color:blue;border:2px dashed red;border-top-color:currentColor;border-right-color:CuRrEnTcOlOr;border-bottom-color:CURRENTCOLOR;border-left-color:currentColor; } #inherited { display:block;width:8px;height:6px;border:1px solid red;border-color:currentColor; } #rollback { display:block;width:8px;height:6px;color:red;border:1px solid red; } #repeat { display:block;width:8px;height:6px;color:red;border:1px solid red; } #same { display:block;width:8px;height:6px;color:green;border:1px solid red;border-color:blue;border-color:currentColor; } #invalid { display:block;width:8px;height:6px;color:blue;border:1px solid red;border-color:green;border-color:invalid; } #complete { display:block;width:8px;height:6px;color:blue;border:1px solid red;border:2px solid currentColor; } #black { display:block;width:8px;height:6px;border:1px solid red;border-color:currentColor; } #inline { display:block;width:8px;height:6px;color:blue;border:1px solid red; } } @layer theme { #rollback { color:blue;border-color:currentColor; } #repeat { color:blue;border-color:currentColor; } #same { border-color:revert-layer; } } @layer top { #rollback { border-color:revert-layer; } #repeat { border-color:currentColor;border-color:revert-layer; } } #inline { color:green;border-color:currentColor; }</style><div id='parent'><button id='shorthand'>Shorthand</button><button id='sides'>Sides</button><button id='inherited'>Inherited</button><button id='rollback'>Rollback</button><button id='repeat'>Repeat</button><button id='same'>Same</button><button id='invalid'>Invalid</button><button id='complete'>Complete</button></div><button id='black'>Black</button><button id='inline'>Inline</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 110,
        device_scale_factor_milli: 1000,
    };
    let shorthand = document.resolve_target("id=shorthand").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let same = document.resolve_target("id=same").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let complete = document.resolve_target("id=complete").unwrap();
    let black = document.resolve_target("id=black").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [
        shorthand, sides, inherited, rollback, repeat, same, invalid, complete, black, inline,
    ];
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: 255,
        alpha: 255,
    };
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: 255,
    };

    let layout = document.layout(viewport).unwrap();
    for node_id in targets {
        let rectangle = layout.box_for(node_id).expect("currentColor layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }
    let semantic_nodes = document.semantic_nodes();
    for node_id in targets {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected currentColor fixture node {node_id:?} to remain visible"
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
    let shorthand_border = border_for(shorthand).expect("currentColor shorthand border command");
    assert_eq!(shorthand_border.top.color, blue);
    assert_eq!(shorthand_border.right.color, green);
    assert_eq!(shorthand_border.bottom.color, blue);
    assert_eq!(shorthand_border.left.color, NativeColor::BLACK);
    let sides_border = border_for(sides).expect("currentColor physical border command");
    assert_eq!(sides_border.top.color, blue);
    assert_eq!(sides_border.right.color, blue);
    assert_eq!(sides_border.bottom.color, blue);
    assert_eq!(sides_border.left.color, blue);
    assert_eq!(border_for(inherited).unwrap().top.color, green);
    assert_eq!(border_for(rollback).unwrap().top.color, blue);
    assert_eq!(border_for(repeat).unwrap().top.color, blue);
    assert_eq!(border_for(same).unwrap().top.color, green);
    assert_eq!(border_for(invalid).unwrap().top.color, green);
    assert_eq!(border_for(complete).unwrap().top.width, 2);
    assert_eq!(
        border_for(complete).unwrap().top.style,
        NativeBorderStyle::Solid
    );
    assert_eq!(border_for(complete).unwrap().top.color, blue);
    assert_eq!(border_for(black).unwrap().top.color, NativeColor::BLACK);
    assert_eq!(border_for(inline).unwrap().top.color, green);
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-color"
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "color"
    }));

    let shorthand_box = layout.box_for(shorthand).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(shorthand_box.x, shorthand_box.y),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_logical_border_width_important_priority_reaches_projected_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #ltr { direction:ltr;border-inline-start-width:1px !important; }
                #rtl { direction:rtl;border-inline-start-width:1px !important; }
                #pair { border-block-width:1px 2px !important; }
                #rollback { border-block-width:1px !important; }
                #rollback { border-block-width:revert-layer !important; }
                #normal { border-block-width:1px; }
                #mixed { border-inline-width:1px !important; }
                #invalid { border-inline-width:1px !important;border-inline-width:invalid !important; }
            }
            @layer theme {
                #ltr { border-inline-start-width:2px !important; }
                #rtl { border-inline-start-width:2px !important; }
                #pair { border-block-width:3px 4px !important; }
                #rollback { border-block-width:5px !important; }
                #normal { border-block-width:2px; }
            }
            #ltr { border-inline-start-width:3px !important; }
            #rtl { border-inline-start-width:3px !important; }
            #normal { border-block-width:3px; }
            #mixed { border-inline-width:4px; }
            #invalid { border-inline-width:4px; }
        </style>
        <div id='ltr' class='box' role='button'>Ltr</div>
        <div id='rtl' class='box' role='button'>Rtl</div>
        <div id='pair' class='box' role='button'>Pair</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='inline' class='box' role='button' style='border-inline-start-width:4px !important'>Inline</div>
        <div id='width-only' style='display:block;width:8px;height:6px;border-inline-width:3px !important'>Only</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 180,
        device_scale_factor_milli: 1000,
    };
    let ltr = document.resolve_target("id=ltr").unwrap();
    let rtl = document.resolve_target("id=rtl").unwrap();
    let pair = document.resolve_target("id=pair").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let width_only = document.resolve_target("id=width-only").unwrap();
    let targets = [ltr, rtl, pair, rollback, normal, mixed, invalid, inline];
    let layout = document.layout(viewport).unwrap();
    for (node_id, (expected_width, expected_height)) in [
        (ltr, (19, 12)),
        (rtl, (19, 12)),
        (pair, (20, 11)),
        (rollback, (20, 18)),
        (normal, (20, 14)),
        (mixed, (18, 12)),
        (invalid, (18, 12)),
        (inline, (22, 12)),
    ] {
        let rectangle = layout
            .box_for(node_id)
            .expect("logical border-width layout box");
        assert_eq!(rectangle.width, expected_width);
        assert_eq!(rectangle.height, expected_height);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
        );
    }
    let width_only_box = layout.box_for(width_only).expect("width-only layout box");
    assert_eq!((width_only_box.width, width_only_box.height), (8, 6));

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
    for (node_id, expected_widths) in [
        (ltr, [2, 2, 2, 1]),
        (rtl, [2, 1, 2, 2]),
        (pair, [1, 2, 2, 2]),
        (rollback, [5, 2, 5, 2]),
        (normal, [3, 2, 3, 2]),
        (mixed, [2, 1, 2, 1]),
        (invalid, [2, 1, 2, 1]),
        (inline, [2, 2, 2, 4]),
    ] {
        let borders = border_for(node_id).expect("logical border-width command");
        assert_eq!(
            [
                borders.top.width,
                borders.right.width,
                borders.bottom.width,
                borders.left.width,
            ],
            expected_widths,
            "logical border widths for {node_id:?}"
        );
        assert_eq!(
            [
                borders.top.style,
                borders.right.style,
                borders.bottom.style,
                borders.left.style,
            ],
            [NativeBorderStyle::Solid; 4]
        );
        assert_eq!(
            [
                borders.top.color,
                borders.right.color,
                borders.bottom.color,
                borders.left.color,
            ],
            [NativeColor::BLACK; 4]
        );
    }
    assert!(border_for(width_only).is_none());
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
            && diagnostic.detail == "border-inline-width"
    }));
    let ltr_box = layout.box_for(ltr).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(ltr_box.x + ltr_box.width / 2, ltr_box.y),
        Some([0, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_logical_border_style_important_priority_reaches_projected_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #ltr { direction:ltr;border-inline-start-style:dashed !important; }
                #rtl { direction:rtl;border-inline-start-style:dashed !important; }
                #pair { border-block-style:double dotted !important; }
                #rollback { border-block-style:groove !important; }
                #rollback { border-block-style:revert-layer !important; }
                #normal { border-block-style:dotted; }
                #mixed { border-inline-style:hidden !important; }
                #invalid { border-inline-style:double !important;border-inline-style:invalid !important; }
                #none { border-block-style:none !important; }
            }
            @layer theme {
                #ltr { border-inline-start-style:groove !important; }
                #rtl { border-inline-start-style:groove !important; }
                #pair { border-block-style:ridge outset !important; }
                #rollback { border-block-style:inset !important; }
                #normal { border-block-style:double; }
            }
            #ltr { border-inline-start-style:solid !important; }
            #rtl { border-inline-start-style:solid !important; }
            #normal { border-block-style:outset; }
            #mixed { border-inline-style:double; }
            #invalid { border-inline-style:solid; }
            #none { border-block-style:solid; }
        </style>
        <div id='ltr' class='box' role='button'>Ltr</div>
        <div id='rtl' class='box' role='button'>Rtl</div>
        <div id='pair' class='box' role='button'>Pair</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='none' class='box' role='button'>None</div>
        <div id='inline' class='box' role='button' style='border-inline-start-style:hidden !important'>Inline</div>
        <div id='style-only' style='display:block;width:8px;height:6px;border-style:solid !important'>Only</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let ltr = document.resolve_target("id=ltr").unwrap();
    let rtl = document.resolve_target("id=rtl").unwrap();
    let pair = document.resolve_target("id=pair").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let none = document.resolve_target("id=none").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let style_only = document.resolve_target("id=style-only").unwrap();
    let targets = [
        ltr, rtl, pair, rollback, normal, mixed, invalid, none, inline,
    ];
    let layout = document.layout(viewport).unwrap();
    for (node_id, (expected_width, expected_height)) in [
        (ltr, (20, 12)),
        (rtl, (20, 12)),
        (pair, (20, 12)),
        (rollback, (20, 12)),
        (normal, (20, 12)),
        (mixed, (16, 12)),
        (invalid, (20, 12)),
        (none, (20, 8)),
        (inline, (18, 12)),
    ] {
        let rectangle = layout
            .box_for(node_id)
            .expect("logical border-style layout box");
        assert_eq!(rectangle.width, expected_width);
        assert_eq!(rectangle.height, expected_height);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
        );
    }
    let style_only_box = layout.box_for(style_only).expect("style-only layout box");
    assert_eq!((style_only_box.width, style_only_box.height), (8, 6));

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
    for (node_id, expected_styles) in [
        (
            ltr,
            [
                NativeBorderStyle::Solid,
                NativeBorderStyle::Solid,
                NativeBorderStyle::Solid,
                NativeBorderStyle::Dashed,
            ],
        ),
        (
            rtl,
            [
                NativeBorderStyle::Solid,
                NativeBorderStyle::Dashed,
                NativeBorderStyle::Solid,
                NativeBorderStyle::Solid,
            ],
        ),
        (
            pair,
            [
                NativeBorderStyle::Double,
                NativeBorderStyle::Solid,
                NativeBorderStyle::Dotted,
                NativeBorderStyle::Solid,
            ],
        ),
        (
            rollback,
            [
                NativeBorderStyle::Inset,
                NativeBorderStyle::Solid,
                NativeBorderStyle::Inset,
                NativeBorderStyle::Solid,
            ],
        ),
        (
            normal,
            [
                NativeBorderStyle::Outset,
                NativeBorderStyle::Solid,
                NativeBorderStyle::Outset,
                NativeBorderStyle::Solid,
            ],
        ),
        (
            invalid,
            [
                NativeBorderStyle::Solid,
                NativeBorderStyle::Double,
                NativeBorderStyle::Solid,
                NativeBorderStyle::Double,
            ],
        ),
    ] {
        let borders = border_for(node_id).expect("logical border-style command");
        assert_eq!(
            [
                borders.top.style,
                borders.right.style,
                borders.bottom.style,
                borders.left.style,
            ],
            expected_styles,
            "logical border styles for {node_id:?}"
        );
        assert_eq!(
            [
                borders.top.width,
                borders.right.width,
                borders.bottom.width,
                borders.left.width,
            ],
            [2; 4]
        );
        assert_eq!(
            [
                borders.top.color,
                borders.right.color,
                borders.bottom.color,
                borders.left.color,
            ],
            [NativeColor::BLACK; 4]
        );
    }
    let mixed_border = border_for(mixed).expect("mixed logical border-style command");
    assert_eq!(
        [
            mixed_border.top.width,
            mixed_border.right.width,
            mixed_border.bottom.width,
            mixed_border.left.width,
        ],
        [2, 0, 2, 0]
    );
    let none_border = border_for(none).expect("none logical border-style command");
    assert_eq!(
        [
            none_border.top.width,
            none_border.right.width,
            none_border.bottom.width,
            none_border.left.width,
        ],
        [0, 2, 0, 2]
    );
    let inline_border = border_for(inline).expect("inline logical border-style command");
    assert_eq!(
        [
            inline_border.top.width,
            inline_border.right.width,
            inline_border.bottom.width,
            inline_border.left.width,
        ],
        [2, 2, 2, 0]
    );
    assert!(border_for(style_only).is_none());

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
            && diagnostic.detail == "border-inline-style"
    }));
    let ltr_box = layout.box_for(ltr).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(ltr_box.x + ltr_box.width / 2, ltr_box.y),
        Some([0, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_logical_border_shorthand_important_priority_reaches_projected_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #ltr { direction:ltr;border-inline-start:1px solid red !important; }
                #rtl { direction:rtl;border-inline-start:1px solid red !important; }
                #pair { border-block:1px solid red !important;border-inline:2px dashed blue !important; }
                #rollback { border-block:1px solid red !important; }
                #rollback { border-block:revert-layer !important; }
                #normal { border-block:1px solid red; }
                #mixed { border-inline-start:1px solid blue !important; }
                #sides { border-block:1px solid red !important;border-block-start:2px dashed green !important;border-inline-end:3px dotted blue !important; }
                #invalid { border-block:1px solid red !important;border-block:invalid !important; }
            }
            @layer theme {
                #ltr { border-inline-start:2px dashed blue !important; }
                #rtl { border-inline-start:2px dashed blue !important; }
                #rollback { border-block:3px groove green !important; }
                #normal { border-block:2px dashed blue; }
                #sides { border-block:4px groove green !important; }
            }
            #ltr { border-inline-start:3px dotted green !important; }
            #rtl { border-inline-start:3px dotted green !important; }
            #normal { border-block:3px double green; }
            #mixed { border-inline:4px double black; }
            #sides { border-inline-end:5px ridge red !important; }
            #invalid { border-block:4px outset green; }
        </style>
        <div id='ltr' class='box' role='button'>Ltr</div>
        <div id='rtl' class='box' role='button'>Rtl</div>
        <div id='pair' class='box' role='button'>Pair</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='sides' class='box' role='button'>Sides</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='inline' class='box' role='button' style='border-inline-start:6px groove blue !important'>Inline</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 220,
        device_scale_factor_milli: 1000,
    };
    let ltr = document.resolve_target("id=ltr").unwrap();
    let rtl = document.resolve_target("id=rtl").unwrap();
    let pair = document.resolve_target("id=pair").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let sides = document.resolve_target("id=sides").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [
        ltr, rtl, pair, rollback, normal, mixed, sides, invalid, inline,
    ];
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
    let layout = document.layout(viewport).unwrap();
    for (node_id, (expected_width, expected_height)) in [
        (ltr, (19, 12)),
        (rtl, (19, 12)),
        (pair, (20, 10)),
        (rollback, (20, 14)),
        (normal, (20, 14)),
        (mixed, (21, 12)),
        (sides, (21, 11)),
        (invalid, (20, 10)),
        (inline, (24, 12)),
    ] {
        let rectangle = layout
            .box_for(node_id)
            .expect("logical border-shorthand layout box");
        assert_eq!(rectangle.width, expected_width);
        assert_eq!(rectangle.height, expected_height);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
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
    let ltr_border = border_for(ltr).expect("ltr logical border-shorthand command");
    assert_eq!(
        [
            ltr_border.top.width,
            ltr_border.right.width,
            ltr_border.bottom.width,
            ltr_border.left.width,
        ],
        [2, 2, 2, 1]
    );
    assert_eq!(ltr_border.top.color, NativeColor::BLACK);
    assert_eq!(ltr_border.left.color, NativeColor::RED);
    let rtl_border = border_for(rtl).expect("rtl logical border-shorthand command");
    assert_eq!(
        [
            rtl_border.top.width,
            rtl_border.right.width,
            rtl_border.bottom.width,
            rtl_border.left.width,
        ],
        [2, 1, 2, 2]
    );
    assert_eq!(rtl_border.right.color, NativeColor::RED);

    let pair_border = border_for(pair).expect("pair logical border-shorthand command");
    assert_eq!(
        [
            pair_border.top.width,
            pair_border.right.width,
            pair_border.bottom.width,
            pair_border.left.width,
        ],
        [1, 2, 1, 2]
    );
    assert_eq!(pair_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(pair_border.right.style, NativeBorderStyle::Dashed);
    assert_eq!(pair_border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(pair_border.left.style, NativeBorderStyle::Dashed);
    assert_eq!(pair_border.top.color, NativeColor::RED);
    assert_eq!(pair_border.right.color, blue);
    assert_eq!(pair_border.bottom.color, NativeColor::RED);
    assert_eq!(pair_border.left.color, blue);

    let rollback_border = border_for(rollback).expect("rollback logical border-shorthand command");
    assert_eq!(
        [
            rollback_border.top.width,
            rollback_border.right.width,
            rollback_border.bottom.width,
            rollback_border.left.width,
        ],
        [3, 2, 3, 2]
    );
    assert_eq!(rollback_border.top.style, NativeBorderStyle::Groove);
    assert_eq!(rollback_border.bottom.style, NativeBorderStyle::Groove);
    assert_eq!(rollback_border.top.color, green);

    let normal_border = border_for(normal).expect("normal logical border-shorthand command");
    assert_eq!(normal_border.top.width, 3);
    assert_eq!(normal_border.top.style, NativeBorderStyle::Double);
    assert_eq!(normal_border.top.color, green);
    assert_eq!(normal_border.right.width, 2);
    assert_eq!(normal_border.right.color, NativeColor::BLACK);

    let mixed_border = border_for(mixed).expect("mixed logical border-shorthand command");
    assert_eq!(
        [
            mixed_border.top.width,
            mixed_border.right.width,
            mixed_border.bottom.width,
            mixed_border.left.width,
        ],
        [2, 4, 2, 1]
    );
    assert_eq!(mixed_border.right.style, NativeBorderStyle::Double);
    assert_eq!(mixed_border.left.style, NativeBorderStyle::Solid);
    assert_eq!(mixed_border.left.color, blue);

    let sides_border = border_for(sides).expect("side logical border-shorthand command");
    assert_eq!(
        [
            sides_border.top.width,
            sides_border.right.width,
            sides_border.bottom.width,
            sides_border.left.width,
        ],
        [2, 3, 1, 2]
    );
    assert_eq!(sides_border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(sides_border.right.style, NativeBorderStyle::Dotted);
    assert_eq!(sides_border.bottom.style, NativeBorderStyle::Solid);
    assert_eq!(sides_border.top.color, green);
    assert_eq!(sides_border.right.color, blue);
    assert_eq!(sides_border.bottom.color, NativeColor::RED);

    let invalid_border = border_for(invalid).expect("invalid logical border-shorthand command");
    assert_eq!(
        [
            invalid_border.top.width,
            invalid_border.right.width,
            invalid_border.bottom.width,
            invalid_border.left.width,
        ],
        [1, 2, 1, 2]
    );
    assert_eq!(invalid_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(invalid_border.top.color, NativeColor::RED);

    let inline_border = border_for(inline).expect("inline logical border-shorthand command");
    assert_eq!(inline_border.left.width, 6);
    assert_eq!(inline_border.left.style, NativeBorderStyle::Groove);
    assert_eq!(inline_border.left.color, blue);

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
            && diagnostic.detail == "border-block"
    }));
    let pair_box = layout.box_for(pair).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(pair_box.x + 10, pair_box.y),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_logical_border_color_important_priority_reaches_projected_border_artifacts() {
    let document = NativeDocument::parse(
        r#"<style>
            .box { display:block;width:16px;height:8px;border:2px solid black; }
            @layer base {
                #ltr { direction:ltr;border-inline-start-color:red !important; }
                #rtl { direction:rtl;border-inline-start-color:red !important; }
                #pair { border-block-color:red blue !important; }
                #rollback { border-block-color:red !important; }
                #rollback { border-block-color:revert-layer !important; }
                #normal { border-block-color:red; }
                #mixed { border-inline-color:red !important; }
                #invalid { border-inline-color:red !important;border-inline-color:invalid !important; }
            }
            @layer theme {
                #ltr { border-inline-start-color:blue !important; }
                #rtl { border-inline-start-color:blue !important; }
                #pair { border-block-color:green green !important; }
                #rollback { border-block-color:green !important; }
                #normal { border-block-color:blue; }
            }
            #ltr { border-inline-start-color:green !important; }
            #rtl { border-inline-start-color:green !important; }
            #pair { border-block-color:green green !important; }
            #normal { border-block-color:green; }
            #mixed { border-inline-color:green; }
            #invalid { border-inline-color:green; }
        </style>
        <div id='ltr' class='box' role='button'>Ltr</div>
        <div id='rtl' class='box' role='button'>Rtl</div>
        <div id='pair' class='box' role='button'>Pair</div>
        <div id='rollback' class='box' role='button'>Rollback</div>
        <div id='normal' class='box' role='button'>Normal</div>
        <div id='mixed' class='box' role='button'>Mixed</div>
        <div id='invalid' class='box' role='button'>Invalid</div>
        <div id='inline' class='box' role='button' style='border-inline-start-color:green !important'>Inline</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 32,
        height: 120,
        device_scale_factor_milli: 1000,
    };
    let ltr = document.resolve_target("id=ltr").unwrap();
    let rtl = document.resolve_target("id=rtl").unwrap();
    let pair = document.resolve_target("id=pair").unwrap();
    let rollback = document.resolve_target("id=rollback").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [ltr, rtl, pair, rollback, normal, mixed, invalid, inline];
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
    let layout = document.layout(viewport).unwrap();
    for node_id in targets {
        let rectangle = layout
            .box_for(node_id)
            .expect("logical border-color layout box");
        assert_eq!(rectangle.width, 20);
        assert_eq!(rectangle.height, 12);
        assert_eq!(
            layout
                .hit_test((rectangle.x + 3).into(), (rectangle.y + 3).into())
                .unwrap(),
            Some(node_id)
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
    for (node_id, expected) in [
        (
            ltr,
            [
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::RED,
            ],
        ),
        (
            rtl,
            [
                NativeColor::BLACK,
                NativeColor::RED,
                NativeColor::BLACK,
                NativeColor::BLACK,
            ],
        ),
        (
            pair,
            [
                NativeColor::RED,
                NativeColor::BLACK,
                blue,
                NativeColor::BLACK,
            ],
        ),
        (
            rollback,
            [green, NativeColor::BLACK, green, NativeColor::BLACK],
        ),
        (
            normal,
            [green, NativeColor::BLACK, green, NativeColor::BLACK],
        ),
        (
            mixed,
            [
                NativeColor::BLACK,
                NativeColor::RED,
                NativeColor::BLACK,
                NativeColor::RED,
            ],
        ),
        (
            invalid,
            [
                NativeColor::BLACK,
                NativeColor::RED,
                NativeColor::BLACK,
                NativeColor::RED,
            ],
        ),
        (
            inline,
            [
                NativeColor::BLACK,
                NativeColor::BLACK,
                NativeColor::BLACK,
                green,
            ],
        ),
    ] {
        let borders = border_for(node_id).expect("logical border-color command");
        assert_eq!(
            [
                borders.top.color,
                borders.right.color,
                borders.bottom.color,
                borders.left.color,
            ],
            expected,
            "border colors for {node_id:?}"
        );
        assert_eq!(borders.top.width, 2);
        assert_eq!(borders.top.style, NativeBorderStyle::Solid);
    }
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
            && diagnostic.detail == "border-inline-color"
    }));
    let pair_box = layout.box_for(pair).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(pair_box.x + 10, pair_box.y),
        Some([255, 0, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}
#[test]
fn native_complete_border_current_color_projects_all_physical_forms() {
    let document = NativeDocument::parse(
        r#"<style>@layer base {
            #painted { display:block;width:8px;height:6px;color:blue;border:1px solid red; }
            #physical { display:block;width:8px;height:6px;color:blue;border:1px solid red;border-top:2px dashed currentColor;border-right:3px double CURRENTcolor;border-bottom:4px dotted CuRrEnTcOlOr;border-left:5px groove currentCOLOR; }
            #none { display:block;width:8px;height:6px;color:green;border:3px none currentColor; }
            #hidden { display:block;width:8px;height:6px;color:green;border:4px hidden CURRENTCOLOR; }
        } @layer theme {
            #painted { border:2px solid currentColor; }
        }</style><button id='painted'>Painted</button><button id='physical'>Physical</button><button id='none'>None</button><button id='hidden'>Hidden</button>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 80,
        device_scale_factor_milli: 1000,
    };
    let painted = document.resolve_target("id=painted").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let none = document.resolve_target("id=none").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();
    let nodes = [painted, physical, none, hidden];
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: 255,
        alpha: 255,
    };

    let layout = document.layout(viewport).unwrap();
    for node_id in nodes {
        let rectangle = layout
            .box_for(node_id)
            .expect("complete currentColor layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
        assert!(
            document
                .semantic_nodes()
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden)
        );
    }
    assert_eq!(
        (
            layout.box_for(none).unwrap().width,
            layout.box_for(none).unwrap().height
        ),
        (8, 6)
    );
    assert_eq!(
        (
            layout.box_for(hidden).unwrap().width,
            layout.box_for(hidden).unwrap().height
        ),
        (8, 6)
    );

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
    let painted_border = border_for(painted).expect("painted complete currentColor border");
    assert_eq!(painted_border.top.width, 2);
    assert_eq!(painted_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(painted_border.top.color, blue);

    let physical_border = border_for(physical).expect("physical complete currentColor borders");
    assert_eq!(physical_border.top.width, 2);
    assert_eq!(physical_border.top.style, NativeBorderStyle::Dashed);
    assert_eq!(physical_border.top.color, blue);
    assert_eq!(physical_border.right.width, 3);
    assert_eq!(physical_border.right.style, NativeBorderStyle::Double);
    assert_eq!(physical_border.right.color, blue);
    assert_eq!(physical_border.bottom.width, 4);
    assert_eq!(physical_border.bottom.style, NativeBorderStyle::Dotted);
    assert_eq!(physical_border.bottom.color, blue);
    assert_eq!(physical_border.left.width, 5);
    assert_eq!(physical_border.left.style, NativeBorderStyle::Groove);
    assert_eq!(physical_border.left.color, blue);
    assert!(border_for(none).is_none());
    assert!(border_for(hidden).is_none());
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border" | "border-top" | "border-right" | "border-bottom" | "border-left"
            )
    }));

    let painted_box = layout.box_for(painted).unwrap();
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(painted_box.x, painted_box.y),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}

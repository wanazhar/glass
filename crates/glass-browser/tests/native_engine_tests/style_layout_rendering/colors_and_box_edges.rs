use super::*;

#[test]
fn native_border_color_css_wide_keywords_preserve_physical_paint_and_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                #parent { display:block;width:32px;height:120px;color:green;border:2px solid red;border-color:red green blue black; }
                .box { display:block;width:8px;height:6px;border-width:1px;border-style:solid; }
                #inherit { color:blue;border-color:inherit; }
                #physical { color:blue;border-top-color:inherit;border-right-color:unset;border-bottom-color:initial;border-left-color:revert; }
                #reset { color:blue;border-color:unset;opacity:0.5; }
                #initial { color:blue;border-color:initial; }
                #revert { color:blue;border-color:revert; }
                #omitted { color:blue; }
                #invalid { color:blue;border-color:inherit;border-color:invalid; }
                #mixed { color:blue;border-color:inherit red; }
                #layered { color:blue;border-color:green; }
                #inline { color:blue;border-color:blue; }
            }
            @layer theme {
                #layered { border-color:inherit; }
            }
            #layered { border-color:revert-layer; }
        </style>
        <div id='parent' role='button'>
            <div id='inherit' class='box' role='button'>Inherit</div>
            <div id='physical' class='box' role='button'>Physical</div>
            <div id='reset' class='box' role='button'>Reset</div>
            <div id='initial' class='box' role='button'>Initial</div>
            <div id='revert' class='box' role='button'>Revert</div>
            <div id='omitted' class='box' role='button'>Omitted</div>
            <div id='invalid' class='box' role='button'>Invalid</div>
            <div id='mixed' class='box' role='button'>Mixed</div>
            <div id='layered' class='box' role='button'>Layered</div>
            <div id='inline' class='box' role='button' style='border-color:inherit'>Inline</div>
        </div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 160,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let reset = document.resolve_target("id=reset").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let layered = document.resolve_target("id=layered").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let targets = [
        parent, inherit, physical, reset, initial, revert, omitted, invalid, mixed, layered, inline,
    ];
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
    let parent_colors = [NativeColor::RED, green, blue, NativeColor::BLACK];

    let layout = document.layout(viewport).unwrap();
    for node_id in targets {
        let rectangle = layout.box_for(node_id).expect("border-color layout box");
        assert_eq!(rectangle.width, if node_id == parent { 36 } else { 10 });
        assert_eq!(rectangle.height, if node_id == parent { 124 } else { 8 });
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
    let parent_border = border_for(parent).expect("parent border command");
    assert_eq!(
        [
            parent_border.top.color,
            parent_border.right.color,
            parent_border.bottom.color,
            parent_border.left.color,
        ],
        parent_colors
    );
    assert_eq!(border_for(inherit).unwrap().top.color, NativeColor::RED);
    assert_eq!(border_for(inherit).unwrap().right.color, green);
    assert_eq!(border_for(inherit).unwrap().bottom.color, blue);
    assert_eq!(border_for(inherit).unwrap().left.color, NativeColor::BLACK);
    let physical_border = border_for(physical).expect("physical border command");
    assert_eq!(physical_border.top.color, NativeColor::RED);
    assert_eq!(physical_border.right.color, blue);
    assert_eq!(physical_border.bottom.color, blue);
    assert_eq!(physical_border.left.color, blue);
    for node_id in [reset, initial, revert] {
        let borders = border_for(node_id).expect("CSS-wide reset border command");
        assert_eq!(
            [
                borders.top.color,
                borders.right.color,
                borders.bottom.color,
                borders.left.color
            ],
            [blue; 4]
        );
        assert_eq!(borders.top.width, 1);
        assert_eq!(borders.top.style, NativeBorderStyle::Solid);
    }
    assert_eq!(
        [
            border_for(omitted).unwrap().top.color,
            border_for(omitted).unwrap().right.color,
            border_for(omitted).unwrap().bottom.color,
            border_for(omitted).unwrap().left.color,
        ],
        [NativeColor::BLACK; 4]
    );
    assert_eq!(border_for(invalid).unwrap().top.color, NativeColor::RED);
    assert_eq!(border_for(mixed).unwrap().top.color, NativeColor::BLACK);
    assert_eq!(border_for(layered).unwrap().top.color, NativeColor::RED);
    assert_eq!(border_for(inline).unwrap().top.color, NativeColor::RED);
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BeginOpacityGroup {
                node_id,
                opacity: 128
            } if *node_id == reset
        )
    }));
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-color"
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border-top-color"
                    | "border-right-color"
                    | "border-bottom-color"
                    | "border-left-color"
            )
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(
            layout.box_for(parent).unwrap().x,
            layout.box_for(parent).unwrap().y
        ),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(
            layout.box_for(reset).unwrap().x,
            layout.box_for(reset).unwrap().y
        ),
        Some([127, 127, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}

#[test]
fn native_border_width_css_wide_keywords_preserve_physical_geometry_and_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                #parent { display:block;width:32px;height:180px;border-width:2px 3px 4px 5px;border-style:solid;border-color:red; }
                .box { display:block;width:8px;height:6px;border-style:solid;border-color:blue; }
                #inherit { border-width:inherit; }
                #physical { border-top-width:inherit;border-right-width:unset;border-bottom-width:initial;border-left-width:revert; }
                #reset { border-width:unset;opacity:0.5; }
                #initial { border-width:initial; }
                #revert { border-width:revert; }
                #omitted { }
                #invalid { border-width:inherit;border-width:invalid; }
                #mixed { border-width:inherit 1px; }
                #layered { border-width:1px; }
                #unpainted { display:block;width:32px;height:10px;border-width:6px 7px 8px 9px; }
                #unpainted-child { border-width:inherit; }
                #zero-parent { display:block;width:32px;height:10px;border-width:0;border-style:solid;border-color:red; }
                #zero-child { border-width:inherit; }
            }
            @layer theme {
                #layered { border-width:inherit; }
            }
            #layered { border-width:revert-layer; }
        </style>
        <div id='parent' role='button'>
            <div id='inherit' class='box' role='button'>Inherit</div>
            <div id='physical' class='box' role='button'>Physical</div>
            <div id='reset' class='box' role='button'>Reset</div>
            <div id='initial' class='box' role='button'>Initial</div>
            <div id='revert' class='box' role='button'>Revert</div>
            <div id='omitted' class='box' role='button'>Omitted</div>
            <div id='invalid' class='box' role='button'>Invalid</div>
            <div id='mixed' class='box' role='button'>Mixed</div>
            <div id='layered' class='box' role='button'>Layered</div>
            <div id='inline' class='box' role='button' style='border-width:inherit'>Inline</div>
        </div>
        <div id='unpainted' role='button'>
            <div id='unpainted-child' class='box' role='button'>Unpainted child</div>
        </div>
        <div id='zero-parent' role='button'>
            <div id='zero-child' class='box' role='button'>Zero child</div>
        </div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 400,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let reset = document.resolve_target("id=reset").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let layered = document.resolve_target("id=layered").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let unpainted = document.resolve_target("id=unpainted").unwrap();
    let unpainted_child = document.resolve_target("id=unpainted-child").unwrap();
    let zero_parent = document.resolve_target("id=zero-parent").unwrap();
    let zero_child = document.resolve_target("id=zero-child").unwrap();
    let targets = [
        parent,
        inherit,
        physical,
        reset,
        initial,
        revert,
        omitted,
        invalid,
        mixed,
        layered,
        inline,
        unpainted,
        unpainted_child,
        zero_parent,
        zero_child,
    ];
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(parent).unwrap().width, 40);
    assert_eq!(layout.box_for(parent).unwrap().height, 186);
    assert_eq!(layout.box_for(inherit).unwrap().width, 16);
    assert_eq!(layout.box_for(inherit).unwrap().height, 12);
    assert_eq!(layout.box_for(physical).unwrap().width, 8);
    assert_eq!(layout.box_for(physical).unwrap().height, 8);
    for node_id in [reset, initial, revert, omitted, mixed, zero_child] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 8, "id={node_id:?}");
        assert_eq!(layout.box_for(node_id).unwrap().height, 6, "id={node_id:?}");
    }
    for node_id in [invalid, layered, inline] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 16, "id={node_id:?}");
        assert_eq!(
            layout.box_for(node_id).unwrap().height,
            12,
            "id={node_id:?}"
        );
    }
    assert_eq!(layout.box_for(unpainted).unwrap().width, 32);
    assert_eq!(layout.box_for(unpainted).unwrap().height, 10);
    assert_eq!(layout.box_for(unpainted_child).unwrap().width, 24);
    assert_eq!(layout.box_for(unpainted_child).unwrap().height, 20);
    assert_eq!(layout.box_for(zero_parent).unwrap().width, 32);
    assert_eq!(layout.box_for(zero_parent).unwrap().height, 10);
    for node_id in [
        parent,
        inherit,
        physical,
        reset,
        initial,
        revert,
        omitted,
        invalid,
        mixed,
        layered,
        inline,
        unpainted_child,
        zero_child,
    ] {
        let rectangle = layout.box_for(node_id).expect("border-width layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }
    let unpainted_box = layout.box_for(unpainted).unwrap();
    assert_eq!(
        layout
            .hit_test((unpainted_box.x + 30).into(), (unpainted_box.y + 1).into())
            .unwrap(),
        Some(unpainted)
    );
    let zero_parent_box = layout.box_for(zero_parent).unwrap();
    assert_eq!(
        layout
            .hit_test(
                (zero_parent_box.x + 30).into(),
                (zero_parent_box.y + 1).into()
            )
            .unwrap(),
        Some(zero_parent)
    );

    let semantic_nodes = document.semantic_nodes();
    for node_id in targets {
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
    let parent_border = border_for(parent).expect("parent border command");
    assert_eq!(parent_border.top.width, 2);
    assert_eq!(parent_border.right.width, 3);
    assert_eq!(parent_border.bottom.width, 4);
    assert_eq!(parent_border.left.width, 5);
    assert_eq!(parent_border.top.color, NativeColor::RED);

    let inherited_border = border_for(inherit).expect("inherited border command");
    assert_eq!(inherited_border.top.width, 2);
    assert_eq!(inherited_border.right.width, 3);
    assert_eq!(inherited_border.bottom.width, 4);
    assert_eq!(inherited_border.left.width, 5);
    assert_eq!(inherited_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(inherited_border.top.color, blue);

    let physical_border = border_for(physical).expect("physical border command");
    assert_eq!(physical_border.top.width, 2);
    assert_eq!(physical_border.right.width, 0);
    assert_eq!(physical_border.bottom.width, 0);
    assert_eq!(physical_border.left.width, 0);
    assert_eq!(physical_border.top.color, blue);
    for node_id in [reset, initial, revert, omitted, mixed, zero_child] {
        assert!(
            border_for(node_id).is_none(),
            "expected no border paint for {node_id:?}"
        );
    }
    for node_id in [invalid, layered, inline] {
        let borders = border_for(node_id).expect("inherited width border command");
        assert_eq!(
            [
                borders.top.width,
                borders.right.width,
                borders.bottom.width,
                borders.left.width,
            ],
            [2, 3, 4, 5]
        );
        assert_eq!(borders.top.color, blue);
    }
    let unpainted_border = border_for(unpainted_child).expect("unpainted-parent inherit command");
    assert_eq!(
        [
            unpainted_border.top.width,
            unpainted_border.right.width,
            unpainted_border.bottom.width,
            unpainted_border.left.width,
        ],
        [6, 7, 8, 9]
    );
    assert_eq!(unpainted_border.top.color, blue);
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BeginOpacityGroup {
                node_id,
                opacity: 128
            } if *node_id == reset
        )
    }));
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-width"
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border-top-width"
                    | "border-right-width"
                    | "border-bottom-width"
                    | "border-left-width"
            )
    }));

    let surface = list.rasterize().unwrap();
    let parent_box = layout.box_for(parent).unwrap();
    assert_eq!(
        surface.pixel(parent_box.x, parent_box.y),
        Some([255, 0, 0, 255])
    );
    let inherit_box = layout.box_for(inherit).unwrap();
    assert_eq!(
        surface.pixel(inherit_box.x, inherit_box.y),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}

#[test]
fn native_border_style_css_wide_keywords_preserve_physical_geometry_and_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                #parent { display:block;width:32px;height:180px;border-width:2px 3px 4px 5px;border-style:solid dashed dotted double;border-color:red; }
                .box { display:block;width:8px;height:6px;border-width:1px;border-color:blue; }
                #inherit { border-style:inherit; }
                #physical { border-top-style:inherit;border-right-style:unset;border-bottom-style:initial;border-left-style:revert; }
                #reset { border-style:unset;opacity:0.5; }
                #initial { border-style:initial; }
                #revert { border-style:revert; }
                #omitted { }
                #invalid { border-style:inherit;border-style:invalid; }
                #mixed { border-style:inherit solid; }
                #layered { border-style:solid; }
                #unpainted { display:block;width:32px;height:10px;border-style:dashed; }
                #unpainted-child { border-style:inherit; }
                #zero-parent { display:block;width:32px;height:10px;border-width:0;border-style:solid;border-color:red; }
                #zero-child { border-style:inherit; }
                #hidden-parent { display:block;width:32px;height:10px;border-width:3px;border-style:hidden;border-color:red; }
                #hidden-child { border-style:inherit; }
            }
            @layer theme {
                #layered { border-style:inherit; }
            }
            #layered { border-style:revert-layer; }
        </style>
        <div id='parent' role='button'>
            <div id='inherit' class='box' role='button'>Inherit</div>
            <div id='physical' class='box' role='button'>Physical</div>
            <div id='reset' class='box' role='button'>Reset</div>
            <div id='initial' class='box' role='button'>Initial</div>
            <div id='revert' class='box' role='button'>Revert</div>
            <div id='omitted' class='box' role='button'>Omitted</div>
            <div id='invalid' class='box' role='button'>Invalid</div>
            <div id='mixed' class='box' role='button'>Mixed</div>
            <div id='layered' class='box' role='button'>Layered</div>
            <div id='inline' class='box' role='button' style='border-style:inherit'>Inline</div>
        </div>
        <div id='unpainted' role='button'>
            <div id='unpainted-child' class='box' role='button'>Unpainted child</div>
        </div>
        <div id='zero-parent' role='button'>
            <div id='zero-child' class='box' role='button'>Zero child</div>
        </div>
        <div id='hidden-parent' role='button'>
            <div id='hidden-child' class='box' role='button'>Hidden child</div>
        </div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 600,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let reset = document.resolve_target("id=reset").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let layered = document.resolve_target("id=layered").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let unpainted = document.resolve_target("id=unpainted").unwrap();
    let unpainted_child = document.resolve_target("id=unpainted-child").unwrap();
    let zero_parent = document.resolve_target("id=zero-parent").unwrap();
    let zero_child = document.resolve_target("id=zero-child").unwrap();
    let hidden_parent = document.resolve_target("id=hidden-parent").unwrap();
    let hidden_child = document.resolve_target("id=hidden-child").unwrap();
    let targets = [
        parent,
        inherit,
        physical,
        reset,
        initial,
        revert,
        omitted,
        invalid,
        mixed,
        layered,
        inline,
        unpainted,
        unpainted_child,
        zero_parent,
        zero_child,
        hidden_parent,
        hidden_child,
    ];
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(parent).unwrap().width, 40);
    assert_eq!(layout.box_for(parent).unwrap().height, 186);
    assert_eq!(layout.box_for(inherit).unwrap().width, 10);
    assert_eq!(layout.box_for(inherit).unwrap().height, 8);
    assert_eq!(layout.box_for(physical).unwrap().width, 8);
    assert_eq!(layout.box_for(physical).unwrap().height, 7);
    for node_id in [reset, initial, revert, omitted, mixed, hidden_child] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 8, "id={node_id:?}");
        assert_eq!(layout.box_for(node_id).unwrap().height, 6, "id={node_id:?}");
    }
    for node_id in [invalid, layered, inline] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 10, "id={node_id:?}");
        assert_eq!(layout.box_for(node_id).unwrap().height, 8, "id={node_id:?}");
    }
    assert_eq!(layout.box_for(unpainted).unwrap().width, 32);
    assert_eq!(layout.box_for(unpainted).unwrap().height, 10);
    assert_eq!(layout.box_for(unpainted_child).unwrap().width, 10);
    assert_eq!(layout.box_for(unpainted_child).unwrap().height, 8);
    assert_eq!(layout.box_for(zero_parent).unwrap().width, 32);
    assert_eq!(layout.box_for(zero_parent).unwrap().height, 10);
    assert_eq!(layout.box_for(zero_child).unwrap().width, 10);
    assert_eq!(layout.box_for(zero_child).unwrap().height, 8);
    assert_eq!(layout.box_for(hidden_parent).unwrap().width, 32);
    assert_eq!(layout.box_for(hidden_parent).unwrap().height, 10);

    for node_id in [
        parent,
        inherit,
        physical,
        reset,
        initial,
        revert,
        omitted,
        invalid,
        mixed,
        layered,
        inline,
        unpainted_child,
        zero_child,
        hidden_child,
    ] {
        let rectangle = layout.box_for(node_id).expect("border-style layout box");
        assert_eq!(
            layout
                .hit_test((rectangle.x + 1).into(), (rectangle.y + 1).into())
                .unwrap(),
            Some(node_id)
        );
    }
    for node_id in [unpainted, zero_parent, hidden_parent] {
        let rectangle = layout.box_for(node_id).unwrap();
        assert_eq!(
            layout
                .hit_test((rectangle.x + 30).into(), (rectangle.y + 1).into())
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
    let parent_border = border_for(parent).expect("parent border command");
    assert_eq!(
        [
            parent_border.top.width,
            parent_border.right.width,
            parent_border.bottom.width,
            parent_border.left.width,
        ],
        [2, 3, 4, 5]
    );
    assert_eq!(
        [
            parent_border.top.style,
            parent_border.right.style,
            parent_border.bottom.style,
            parent_border.left.style,
        ],
        [
            NativeBorderStyle::Solid,
            NativeBorderStyle::Dashed,
            NativeBorderStyle::Dotted,
            NativeBorderStyle::Double,
        ]
    );
    assert_eq!(parent_border.top.color, NativeColor::RED);

    let inherited_border = border_for(inherit).expect("inherited border command");
    assert_eq!(
        [
            inherited_border.top.style,
            inherited_border.right.style,
            inherited_border.bottom.style,
            inherited_border.left.style,
        ],
        [
            NativeBorderStyle::Solid,
            NativeBorderStyle::Dashed,
            NativeBorderStyle::Dotted,
            NativeBorderStyle::Double,
        ]
    );
    assert_eq!(
        [
            inherited_border.top.width,
            inherited_border.right.width,
            inherited_border.bottom.width,
            inherited_border.left.width,
        ],
        [1, 1, 1, 1]
    );
    assert_eq!(inherited_border.top.color, blue);

    let physical_border = border_for(physical).expect("physical style longhand command");
    assert_eq!(physical_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(physical_border.top.width, 1);
    assert_eq!(physical_border.right.width, 0);
    assert_eq!(physical_border.bottom.width, 0);
    assert_eq!(physical_border.left.width, 0);
    assert_eq!(physical_border.top.color, blue);
    for node_id in [reset, initial, revert, omitted, mixed, hidden_child] {
        assert!(
            border_for(node_id).is_none(),
            "expected no border paint for {node_id:?}"
        );
    }
    for node_id in [invalid, layered, inline] {
        let borders = border_for(node_id).expect("inherited style border command");
        assert_eq!(borders.top.style, NativeBorderStyle::Solid);
        assert_eq!(borders.right.style, NativeBorderStyle::Dashed);
        assert_eq!(borders.bottom.style, NativeBorderStyle::Dotted);
        assert_eq!(borders.left.style, NativeBorderStyle::Double);
        assert_eq!(borders.top.color, blue);
    }
    assert_eq!(
        border_for(unpainted_child)
            .expect("unpainted-parent inherit command")
            .top
            .style,
        NativeBorderStyle::Dashed
    );
    assert_eq!(
        border_for(zero_child)
            .expect("zero-parent inherit command")
            .top
            .style,
        NativeBorderStyle::Solid
    );
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::BeginOpacityGroup {
                node_id,
                opacity: 128
            } if *node_id == reset
        )
    }));
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-style"
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border-top-style"
                    | "border-right-style"
                    | "border-bottom-style"
                    | "border-left-style"
            )
    }));

    let surface = list.rasterize().unwrap();
    let parent_box = layout.box_for(parent).unwrap();
    assert_eq!(
        surface.pixel(parent_box.x, parent_box.y),
        Some([255, 0, 0, 255])
    );
    let inherit_box = layout.box_for(inherit).unwrap();
    assert_eq!(
        surface.pixel(inherit_box.x, inherit_box.y),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}

#[test]
fn native_box_model_reset_keywords_preserve_geometry_and_artifact_owners() {
    let document = NativeDocument::parse(
        r#"<style>
        @layer base {
          #physical { display:block; width:10px; height:6px; padding:4px; margin:3px; box-sizing:border-box; background-color:red; }
          #logical { display:block; direction:rtl; width:10px; height:6px; padding-block:2px 3px; padding-inline:4px 5px; margin-block:1px 2px; margin-inline:3px 4px; box-sizing:border-box; background-color:blue; }
          #important { display:block; width:30px; height:20px; background-color:green; }
          #important { padding:6px !important; margin:7px !important; box-sizing:border-box !important; }
          #important { padding:initial; margin:unset; box-sizing:initial; }
        }
        @layer theme {
          #physical { padding:initial; margin:unset; box-sizing:revert; }
          #logical { padding-inline-start:initial; margin-inline-end:revert; box-sizing:unset; }
        }
        </style>
        <div id='physical' role='button'>Physical</div>
        <div id='logical' role='button'>Logical</div>
        <div id='important' role='button'>Important</div>"#,
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
        width: 64,
        height: 128,
        device_scale_factor_milli: 1000,
    };
    let physical = document.resolve_target("id=physical").unwrap();
    let logical = document.resolve_target("id=logical").unwrap();
    let important = document.resolve_target("id=important").unwrap();
    let layout = document.layout(viewport).unwrap();

    let physical_rect = layout.box_for(physical).unwrap();
    assert_eq!((physical_rect.width, physical_rect.height), (10, 6));
    let physical_box = layout
        .boxes
        .iter()
        .find(|item| item.node_id == physical)
        .unwrap();
    assert_eq!(
        physical_box.content_rect,
        NativeRect {
            x: physical_rect.x,
            y: physical_rect.y,
            width: 10,
            height: 6,
        }
    );

    let logical_rect = layout.box_for(logical).unwrap();
    assert_eq!((logical_rect.width, logical_rect.height), (15, 11));
    let logical_box = layout
        .boxes
        .iter()
        .find(|item| item.node_id == logical)
        .unwrap();
    assert_eq!(
        logical_box.content_rect,
        NativeRect {
            x: logical_rect.x + 5,
            y: logical_rect.y + 2,
            width: 10,
            height: 6,
        }
    );

    let important_rect = layout.box_for(important).unwrap();
    assert_eq!((important_rect.width, important_rect.height), (30, 20));
    let important_box = layout
        .boxes
        .iter()
        .find(|item| item.node_id == important)
        .unwrap();
    assert_eq!(
        important_box.content_rect,
        NativeRect {
            x: important_rect.x + 6,
            y: important_rect.y + 6,
            width: 18,
            height: 8,
        }
    );
    assert_eq!(
        layout.hit_test((important_rect.x + 1).into(), (important_rect.y + 1).into()),
        Ok(Some(important))
    );

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [physical, logical, important].windows(2) {
        assert!(
            semantic_ids.iter().position(|id| *id == pair[0])
                < semantic_ids.iter().position(|id| *id == pair[1]),
            "semantic/source order lost for {pair:?}"
        );
    }

    let list = document.display_list(viewport).unwrap();
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, rect, color, .. }
                if *node_id == physical && *rect == physical_rect && *color == NativeColor::RED
        )
    }));
    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(important_rect.x + 1, important_rect.y + 1),
        Some([0, 128, 0, 255])
    );
    let capture = surface.to_png().unwrap();
    let decoder = png::Decoder::new(Cursor::new(capture));
    let mut reader = decoder.read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((output.width, output.height), (64, 128));
}

#[test]
fn native_color_css_wide_keywords_preserve_inheritance_and_artifacts() {
    let document = NativeDocument::parse(
        "<style>.surface { display:block;width:64px;height:20px;line-height:20px;background-color:currentColor;border:1px solid currentColor;text-decoration:underline;text-decoration-color:currentColor; } @layer base { #parent { color:green; } #inherit { color:inherit; } #unset { color:unset; } #revert { color:revert; } #initial { color:initial; } #invalid { color:red;color:inherit;color:invalid; } #precedence { color:red; } #root-inherit { color:inherit; } #root-unset { color:unset; } #root-revert { color:revert; } #root-initial { color:initial; } } @layer theme { #precedence { color:blue; } #invalid-mixed { color:inherit red; } } #precedence { color:InHeRiT; } #inline { color:initial; }</style><div id='parent' role='button'><button id='inherit' class='surface'>Inherit</button><button id='unset' class='surface'>Unset</button><button id='revert' class='surface'>Revert</button><button id='initial' class='surface'>Initial</button><button id='invalid' class='surface'>Invalid</button><button id='precedence' class='surface'>Precedence</button><button id='inline' class='surface' style='color:unset'>Inline</button></div><button id='root-inherit' class='surface'>Root inherit</button><button id='root-unset' class='surface'>Root unset</button><button id='root-revert' class='surface'>Root revert</button><button id='root-initial' class='surface'>Root initial</button><button id='invalid-mixed' class='surface'>Mixed</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 260,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let unset = document.resolve_target("id=unset").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let precedence = document.resolve_target("id=precedence").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let root_inherit = document.resolve_target("id=root-inherit").unwrap();
    let root_unset = document.resolve_target("id=root-unset").unwrap();
    let root_revert = document.resolve_target("id=root-revert").unwrap();
    let root_initial = document.resolve_target("id=root-initial").unwrap();
    let invalid_mixed = document.resolve_target("id=invalid-mixed").unwrap();
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };

    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout
            .hit_test(
                (layout.box_for(inherit).unwrap().x + 1).into(),
                (layout.box_for(inherit).unwrap().y + 1).into(),
            )
            .unwrap(),
        Some(inherit)
    );
    for node_id in [
        parent,
        inherit,
        unset,
        revert,
        initial,
        invalid,
        precedence,
        inline,
        root_inherit,
        root_unset,
        root_revert,
        root_initial,
        invalid_mixed,
    ] {
        assert!(
            layout.box_for(node_id).is_some(),
            "missing layout for {node_id:?}"
        );
    }

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
    for node_id in [inherit, unset, revert, invalid, precedence] {
        let text = text_for(node_id);
        assert_eq!(text.1, green);
        assert_eq!(text.2, green);
        assert_eq!(fill_for(node_id), Some(green));
        assert_eq!(border_for(node_id).unwrap().top.color, green);
    }
    assert_eq!(text_for(inline).1, green);
    assert_eq!(text_for(inline).2, green);
    assert_eq!(fill_for(inline), Some(green));
    assert_eq!(border_for(inline).unwrap().top.color, green);
    for node_id in [initial, root_inherit, root_unset, root_revert, root_initial] {
        let text = text_for(node_id);
        assert_eq!(text.1, NativeColor::BLACK);
        assert_eq!(text.2, NativeColor::BLACK);
        assert_eq!(fill_for(node_id), Some(NativeColor::BLACK));
        assert_eq!(border_for(node_id).unwrap().top.color, NativeColor::BLACK);
    }

    let inherited_text = text_for(inherit);
    let raster = list.rasterize().unwrap();
    assert_eq!(
        raster.pixel(inherited_text.0.x + 1, inherited_text.0.y),
        Some([0, 128, 0, 255])
    );
    assert!(!raster.to_png().unwrap().is_empty());

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [
        (parent, inherit),
        (inherit, unset),
        (unset, revert),
        (revert, initial),
        (initial, invalid),
        (invalid, precedence),
        (precedence, inline),
    ] {
        assert!(
            semantic_ids.iter().position(|id| *id == pair.0)
                < semantic_ids.iter().position(|id| *id == pair.1),
            "semantic/source order lost for {pair:?}"
        );
    }
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "color"
    }));
    assert_eq!(
        document
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == "color"
            })
            .count(),
        2
    );
}

#[test]
fn native_background_color_css_wide_keywords_preserve_optional_fill_owners() {
    let document = NativeDocument::parse(
        "<style>.surface { display:block;width:64px;height:20px; } @layer base { #parent { color:green;background-color:blue; } #inherit { background-color:inherit; } #current { background-color:currentColor; } #unset { background-color:unset; } #initial { background-color:initial; } #revert { background-color:revert; } #invalid { background-color:red;background-color:inherit;background-color:invalid; } #precedence { background-color:red; } #inline { background-color:blue; } #root-inherit { background-color:inherit; } #root-unset { background-color:unset; } #root-revert { background-color:revert; } #root-initial { background-color:initial; } #empty-parent { display:block;width:64px;height:20px; } #inherit-empty { background-color:inherit; } } @layer theme { #precedence { background-color:blue; } #invalid-mixed { background-color:inherit red; } } #precedence { background-color:InHeRiT; }</style><div id='parent' role='button'><button id='inherit' class='surface' role='button'>Inherit</button><button id='current' class='surface' role='button'>Current</button><button id='unset' class='surface' role='button'>Unset</button><button id='initial' class='surface' role='button'>Initial</button><button id='revert' class='surface' role='button'>Revert</button><button id='invalid' class='surface' role='button'>Invalid</button><button id='precedence' class='surface' role='button'>Precedence</button><button id='inline' class='surface' role='button' style='background-color:initial'>Inline</button></div><div id='empty-parent' role='button'><button id='inherit-empty' class='surface' role='button' style='background-color:inherit'>Empty</button></div><button id='root-inherit' class='surface' role='button'>Root inherit</button><button id='root-unset' class='surface' role='button'>Root unset</button><button id='root-revert' class='surface' role='button'>Root revert</button><button id='root-initial' class='surface' role='button'>Root initial</button><button id='invalid-mixed' class='surface' role='button'>Mixed</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 80,
        height: 300,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let current = document.resolve_target("id=current").unwrap();
    let unset = document.resolve_target("id=unset").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let precedence = document.resolve_target("id=precedence").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let empty_parent = document.resolve_target("id=empty-parent").unwrap();
    let inherit_empty = document.resolve_target("id=inherit-empty").unwrap();
    let root_inherit = document.resolve_target("id=root-inherit").unwrap();
    let root_unset = document.resolve_target("id=root-unset").unwrap();
    let root_revert = document.resolve_target("id=root-revert").unwrap();
    let root_initial = document.resolve_target("id=root-initial").unwrap();
    let invalid_mixed = document.resolve_target("id=invalid-mixed").unwrap();
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
    assert_eq!(
        layout
            .hit_test(
                (layout.box_for(inherit).unwrap().x + 1).into(),
                (layout.box_for(inherit).unwrap().y + 1).into(),
            )
            .unwrap(),
        Some(inherit)
    );
    for node_id in [
        parent,
        inherit,
        current,
        unset,
        initial,
        revert,
        invalid,
        precedence,
        inline,
        empty_parent,
        inherit_empty,
        root_inherit,
        root_unset,
        root_revert,
        root_initial,
        invalid_mixed,
    ] {
        assert!(
            layout.box_for(node_id).is_some(),
            "missing layout for {node_id:?}"
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
    assert_eq!(fill_for(parent), Some(blue));
    for node_id in [inherit, invalid, precedence] {
        assert_eq!(fill_for(node_id), Some(blue));
    }
    assert_eq!(fill_for(current), Some(green));
    for node_id in [
        unset,
        initial,
        revert,
        inline,
        inherit_empty,
        root_inherit,
        root_unset,
        root_revert,
        root_initial,
        invalid_mixed,
    ] {
        assert_eq!(fill_for(node_id), None, "unexpected fill for {node_id:?}");
    }

    let raster = list.rasterize().unwrap();
    let inherit_box = layout.box_for(inherit).unwrap();
    assert_eq!(
        raster.pixel(inherit_box.x + 1, inherit_box.y + 1),
        Some([0, 0, 255, 255])
    );
    assert!(!raster.to_png().unwrap().is_empty());

    let semantic_ids = document
        .semantic_nodes()
        .into_iter()
        .map(|node| node.node_id)
        .collect::<Vec<_>>();
    for pair in [
        (parent, inherit),
        (inherit, current),
        (current, unset),
        (unset, initial),
        (initial, revert),
        (revert, invalid),
        (invalid, precedence),
        (precedence, inline),
        (inline, empty_parent),
        (empty_parent, inherit_empty),
        (inherit_empty, root_inherit),
        (root_initial, invalid_mixed),
    ] {
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
                    && diagnostic.detail == "background-color"
            })
            .count(),
        2
    );
}

#[test]
fn native_display_list_inherits_text_color_and_preserves_transparent_override() {
    let document = NativeDocument::parse(
        "<style>#parent { color: blue; }</style><div id='parent'><span id='child'>Child</span><span id='transparent' style='color:transparent'>Clear</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let child = document.resolve_target("id=child").unwrap();
    let transparent = document.resolve_target("id=transparent").unwrap();
    let list = document.display_list(viewport).unwrap();

    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == child
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == transparent
                    && *color == NativeColor { red: 0, green: 0, blue: 0, alpha: 0 }
        )
    }));
}

#[test]
fn native_border_radius_css_wide_keywords_preserve_rounded_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                #parent { display:block;width:32px;height:260px;border-width:2px 3px 4px 5px;border-style:solid;border-color:red;background-color:red;border-radius:4px 3px 2px 1px; }
                .box { display:block;width:8px;height:6px;border:1px solid blue;background-color:blue; }
                #inherit { border-radius:inherit; }
                #physical { border-radius:1px 2px 3px 4px; }
                #reset { border-radius:unset; }
                #initial { border-radius:initial; }
                #revert { border-radius:revert; }
                #omitted { }
                #invalid { border-radius:inherit;border-radius:invalid; }
                #mixed { border-radius:inherit 1px; }
                #layered { border-radius:1px; }
                #unpainted { display:block;width:32px;height:10px;border-radius:5px; }
                #unpainted-child { border-radius:inherit; }
                #zero-parent { display:block;width:32px;height:10px;border-width:0;border-style:solid;border-color:red;border-radius:7px; }
                #zero-child { border-radius:inherit; }
            }
            @layer theme {
                #layered { border-radius:inherit; }
            }
            #layered { border-radius:revert-layer; }
        </style>
        <div id='parent' role='button'>
            <div id='inherit' class='box' role='button'>Inherit</div>
            <div id='physical' class='box' role='button'>Physical</div>
            <div id='reset' class='box' role='button'>Reset</div>
            <div id='initial' class='box' role='button'>Initial</div>
            <div id='revert' class='box' role='button'>Revert</div>
            <div id='omitted' class='box' role='button'>Omitted</div>
            <div id='invalid' class='box' role='button'>Invalid</div>
            <div id='mixed' class='box' role='button'>Mixed</div>
            <div id='layered' class='box' role='button'>Layered</div>
            <div id='inline' class='box' role='button' style='border-radius:inherit'>Inline</div>
        </div>
        <div id='unpainted' role='button'>
            <div id='unpainted-child' class='box' role='button'>Unpainted child</div>
        </div>
        <div id='zero-parent' role='button'>
            <div id='zero-child' class='box' role='button'>Zero child</div>
        </div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 600,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let reset = document.resolve_target("id=reset").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let layered = document.resolve_target("id=layered").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let unpainted = document.resolve_target("id=unpainted").unwrap();
    let unpainted_child = document.resolve_target("id=unpainted-child").unwrap();
    let zero_parent = document.resolve_target("id=zero-parent").unwrap();
    let zero_child = document.resolve_target("id=zero-child").unwrap();
    let targets = [
        parent,
        inherit,
        physical,
        reset,
        initial,
        revert,
        omitted,
        invalid,
        mixed,
        layered,
        inline,
        unpainted,
        unpainted_child,
        zero_parent,
        zero_child,
    ];
    let parent_radius = NativeBorderRadius {
        top_left: 4,
        top_right: 3,
        bottom_right: 2,
        bottom_left: 1,
    };
    let physical_radius = NativeBorderRadius {
        top_left: 1,
        top_right: 2,
        bottom_right: 3,
        bottom_left: 4,
    };

    let layout = document.layout(viewport).unwrap();
    let layout_box_for = |node_id| {
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == node_id)
            .expect("border-radius layout box")
    };
    assert_eq!(layout.box_for(parent).unwrap().width, 40);
    assert_eq!(layout.box_for(parent).unwrap().height, 266);
    for node_id in [
        inherit, physical, reset, initial, revert, omitted, invalid, mixed, layered, inline,
    ] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 10, "id={node_id:?}");
        assert_eq!(layout.box_for(node_id).unwrap().height, 8, "id={node_id:?}");
    }
    assert_eq!(layout.box_for(unpainted).unwrap().width, 32);
    assert_eq!(layout.box_for(unpainted).unwrap().height, 10);
    assert_eq!(layout.box_for(unpainted_child).unwrap().width, 10);
    assert_eq!(layout.box_for(unpainted_child).unwrap().height, 8);
    assert_eq!(layout.box_for(zero_parent).unwrap().width, 32);
    assert_eq!(layout.box_for(zero_parent).unwrap().height, 10);
    assert_eq!(layout.box_for(zero_child).unwrap().width, 10);
    assert_eq!(layout.box_for(zero_child).unwrap().height, 8);

    let parent_box = layout.box_for(parent).unwrap();
    assert_eq!(layout_box_for(parent).border_radius, parent_radius);
    assert_eq!(layout_box_for(inherit).border_radius, parent_radius);
    assert_eq!(layout_box_for(physical).border_radius, physical_radius);
    for node_id in [reset, initial, revert, omitted, mixed] {
        assert_eq!(
            layout_box_for(node_id).border_radius,
            NativeBorderRadius::default(),
            "id={node_id:?}"
        );
    }
    for node_id in [invalid, layered, inline] {
        assert_eq!(
            layout_box_for(node_id).border_radius,
            parent_radius,
            "id={node_id:?}"
        );
    }
    assert_eq!(
        layout_box_for(unpainted).border_radius,
        NativeBorderRadius {
            top_left: 5,
            top_right: 5,
            bottom_right: 5,
            bottom_left: 5,
        }
    );
    assert_eq!(
        layout_box_for(unpainted_child).border_radius,
        NativeBorderRadius {
            top_left: 5,
            top_right: 5,
            bottom_right: 5,
            bottom_left: 5,
        }
    );
    assert_eq!(
        layout_box_for(zero_parent).border_radius,
        NativeBorderRadius {
            top_left: 7,
            top_right: 7,
            bottom_right: 7,
            bottom_left: 7,
        }
    );
    assert_eq!(
        layout_box_for(zero_child).border_radius,
        NativeBorderRadius {
            top_left: 7,
            top_right: 7,
            bottom_right: 7,
            bottom_left: 7,
        }
    );

    for node_id in targets {
        let rectangle = layout.box_for(node_id).expect("border-radius layout box");
        assert_eq!(
            layout
                .hit_test(
                    (rectangle.x + rectangle.width / 2).into(),
                    (rectangle.y + rectangle.height / 2).into(),
                )
                .unwrap(),
            Some(node_id),
            "id={node_id:?}"
        );
    }
    assert_ne!(
        layout
            .hit_test(parent_box.x.into(), parent_box.y.into())
            .unwrap(),
        Some(parent),
        "the rounded outer corner cannot target its parent"
    );
    let inherit_box = layout.box_for(inherit).unwrap();
    assert_ne!(
        layout
            .hit_test(inherit_box.x.into(), inherit_box.y.into())
            .unwrap(),
        Some(inherit)
    );

    let semantic_nodes = document.semantic_nodes();
    for node_id in targets {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected border-radius fixture node {node_id:?} to remain visible"
        );
    }

    let list = document.display_list(viewport).unwrap();
    let fill_radius_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::FillRect {
                node_id: command_node_id,
                radius,
                ..
            } if *command_node_id == node_id => Some(*radius),
            _ => None,
        })
    };
    let border_radius_for = |node_id| {
        list.commands.iter().find_map(|command| match command {
            NativeDisplayCommand::BorderRect {
                node_id: command_node_id,
                radius,
                ..
            } if *command_node_id == node_id => Some(*radius),
            _ => None,
        })
    };
    for node_id in [
        parent,
        inherit,
        physical,
        reset,
        initial,
        revert,
        omitted,
        invalid,
        mixed,
        layered,
        inline,
        unpainted_child,
        zero_child,
    ] {
        assert!(
            fill_radius_for(node_id).is_some(),
            "missing fill command for {node_id:?}"
        );
        assert!(
            border_radius_for(node_id).is_some(),
            "missing border command for {node_id:?}"
        );
    }
    assert_eq!(fill_radius_for(parent), Some(parent_radius));
    assert_eq!(border_radius_for(parent), Some(parent_radius));
    for node_id in [inherit, invalid, layered, inline] {
        assert_eq!(
            fill_radius_for(node_id),
            Some(parent_radius),
            "id={node_id:?}"
        );
        assert_eq!(
            border_radius_for(node_id),
            Some(parent_radius),
            "id={node_id:?}"
        );
    }
    assert_eq!(fill_radius_for(physical), Some(physical_radius));
    assert_eq!(border_radius_for(physical), Some(physical_radius));
    for node_id in [reset, initial, revert, omitted, mixed] {
        assert_eq!(
            fill_radius_for(node_id),
            Some(NativeBorderRadius::default())
        );
        assert_eq!(
            border_radius_for(node_id),
            Some(NativeBorderRadius::default())
        );
    }
    assert_eq!(
        fill_radius_for(unpainted_child),
        Some(NativeBorderRadius {
            top_left: 5,
            top_right: 5,
            bottom_right: 5,
            bottom_left: 5,
        })
    );
    assert_eq!(
        fill_radius_for(zero_child),
        Some(NativeBorderRadius {
            top_left: 7,
            top_right: 7,
            bottom_right: 7,
            bottom_left: 7,
        })
    );
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border-radius"
    }));

    let surface = list.rasterize().unwrap();
    assert_eq!(
        surface.pixel(parent_box.x, parent_box.y),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        surface.pixel(parent_box.x + parent_box.width / 2, parent_box.y),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        surface.pixel(inherit_box.x + inherit_box.width / 2, inherit_box.y),
        Some([0, 0, 255, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}

#[test]
fn native_complete_border_css_wide_keywords_preserve_physical_consumers() {
    let document = NativeDocument::parse(
        r#"<style>
            @layer base {
                #parent { display:block;width:32px;height:180px;border-width:2px 3px 4px 5px;border-style:solid dashed dotted double;border-color:red green blue black;background-color:red; }
                .box { display:block;width:8px;height:6px; }
                .fallback { border:1px solid blue; }
                #inherit { border:inherit; }
                #physical { border-top:inherit;border-right:unset;border-bottom:initial;border-left:revert; }
                #reset { border:unset;opacity:0.5; }
                #initial { border:initial; }
                #revert { border:revert; }
                #omitted { }
                #invalid { border:inherit;border:invalid; }
                #mixed { border:inherit solid red; }
                #layered { border:1px solid green; }
                #compose { color:green;border:unset;border-width:2px;border-style:solid; }
                #unpainted { display:block;width:32px;height:10px; }
                #unpainted-child { border:inherit; }
                #zero-parent { display:block;width:32px;height:10px;border-width:0;border-style:solid;border-color:red; }
                #zero-child { border:inherit; }
                #hidden-parent { display:block;width:32px;height:10px;border-width:3px;border-style:hidden;border-color:red; }
                #hidden-child { border:inherit; }
            }
            @layer theme {
                #layered { border:inherit; }
            }
            #layered { border:revert-layer; }
        </style>
        <div id='parent' role='button'>
            <div id='inherit' class='box fallback' role='button'>Inherit</div>
            <div id='physical' class='box fallback' role='button'>Physical</div>
            <div id='reset' class='box fallback' role='button'>Reset</div>
            <div id='initial' class='box fallback' role='button'>Initial</div>
            <div id='revert' class='box fallback' role='button'>Revert</div>
            <div id='omitted' class='box' role='button'>Omitted</div>
            <div id='invalid' class='box fallback' role='button'>Invalid</div>
            <div id='mixed' class='box fallback' role='button'>Mixed</div>
            <div id='layered' class='box fallback' role='button'>Layered</div>
            <div id='compose' class='box' role='button'>Compose</div>
            <div id='inline' class='box fallback' role='button' style='border:inherit'>Inline</div>
        </div>
        <div id='unpainted' role='button'>
            <div id='unpainted-child' class='box fallback' role='button'>Unpainted child</div>
        </div>
        <div id='zero-parent' role='button'>
            <div id='zero-child' class='box fallback' role='button'>Zero child</div>
        </div>
        <div id='hidden-parent' role='button'>
            <div id='hidden-child' class='box fallback' role='button'>Hidden child</div>
        </div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 500,
        device_scale_factor_milli: 1000,
    };
    let parent = document.resolve_target("id=parent").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let reset = document.resolve_target("id=reset").unwrap();
    let initial = document.resolve_target("id=initial").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let omitted = document.resolve_target("id=omitted").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let mixed = document.resolve_target("id=mixed").unwrap();
    let layered = document.resolve_target("id=layered").unwrap();
    let compose = document.resolve_target("id=compose").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let unpainted = document.resolve_target("id=unpainted").unwrap();
    let unpainted_child = document.resolve_target("id=unpainted-child").unwrap();
    let zero_parent = document.resolve_target("id=zero-parent").unwrap();
    let zero_child = document.resolve_target("id=zero-child").unwrap();
    let hidden_parent = document.resolve_target("id=hidden-parent").unwrap();
    let hidden_child = document.resolve_target("id=hidden-child").unwrap();
    let targets = [
        parent,
        inherit,
        physical,
        reset,
        initial,
        revert,
        omitted,
        invalid,
        mixed,
        layered,
        compose,
        inline,
        unpainted,
        unpainted_child,
        zero_parent,
        zero_child,
        hidden_parent,
        hidden_child,
    ];

    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(parent).unwrap().width, 40);
    assert_eq!(layout.box_for(parent).unwrap().height, 186);
    for node_id in [inherit, invalid, layered, inline] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 16, "id={node_id:?}");
        assert_eq!(
            layout.box_for(node_id).unwrap().height,
            12,
            "id={node_id:?}"
        );
    }
    assert_eq!(layout.box_for(physical).unwrap().width, 8);
    assert_eq!(layout.box_for(physical).unwrap().height, 8);
    assert_eq!(layout.box_for(compose).unwrap().width, 12);
    assert_eq!(layout.box_for(compose).unwrap().height, 10);
    assert_eq!(layout.box_for(mixed).unwrap().width, 10);
    assert_eq!(layout.box_for(mixed).unwrap().height, 8);
    for node_id in [
        reset,
        initial,
        revert,
        omitted,
        unpainted_child,
        zero_child,
        hidden_child,
    ] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 8, "id={node_id:?}");
        assert_eq!(layout.box_for(node_id).unwrap().height, 6, "id={node_id:?}");
    }
    for node_id in [unpainted, zero_parent, hidden_parent] {
        assert_eq!(layout.box_for(node_id).unwrap().width, 32, "id={node_id:?}");
        assert_eq!(
            layout.box_for(node_id).unwrap().height,
            10,
            "id={node_id:?}"
        );
    }

    for (name, node_id) in [
        ("parent", parent),
        ("inherit", inherit),
        ("physical", physical),
        ("reset", reset),
        ("initial", initial),
        ("revert", revert),
        ("omitted", omitted),
        ("invalid", invalid),
        ("mixed", mixed),
        ("layered", layered),
        ("compose", compose),
        ("inline", inline),
        ("unpainted", unpainted),
        ("unpainted-child", unpainted_child),
        ("zero-parent", zero_parent),
        ("zero-child", zero_child),
        ("hidden-parent", hidden_parent),
        ("hidden-child", hidden_child),
    ] {
        let rectangle = layout.box_for(node_id).expect("complete-border layout box");
        let x_offset = if matches!(name, "unpainted" | "zero-parent" | "hidden-parent") {
            30
        } else {
            1
        };
        assert_eq!(
            layout
                .hit_test((rectangle.x + x_offset).into(), (rectangle.y + 1).into(),)
                .unwrap(),
            Some(node_id),
            "id={name} node={node_id:?}"
        );
    }

    let semantic_nodes = document.semantic_nodes();
    for node_id in targets {
        assert!(
            semantic_nodes
                .iter()
                .any(|node| node.node_id == node_id && !node.hidden),
            "expected complete-border fixture node {node_id:?} to remain visible"
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
    let parent_border = border_for(parent).expect("parent complete-border command");
    assert_eq!(
        [
            parent_border.top.width,
            parent_border.right.width,
            parent_border.bottom.width,
            parent_border.left.width,
        ],
        [2, 3, 4, 5]
    );
    assert_eq!(
        [
            parent_border.top.style,
            parent_border.right.style,
            parent_border.bottom.style,
            parent_border.left.style,
        ],
        [
            NativeBorderStyle::Solid,
            NativeBorderStyle::Dashed,
            NativeBorderStyle::Dotted,
            NativeBorderStyle::Double,
        ]
    );
    assert_eq!(
        [
            parent_border.top.color,
            parent_border.right.color,
            parent_border.bottom.color,
            parent_border.left.color,
        ],
        [
            NativeColor::RED,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
            NativeColor::BLACK,
        ]
    );

    let inherited_border = border_for(inherit).expect("inherited complete-border command");
    assert_eq!(
        [
            inherited_border.top.width,
            inherited_border.right.width,
            inherited_border.bottom.width,
            inherited_border.left.width,
        ],
        [2, 3, 4, 5]
    );
    assert_eq!(
        [
            inherited_border.top.style,
            inherited_border.right.style,
            inherited_border.bottom.style,
            inherited_border.left.style,
        ],
        [
            NativeBorderStyle::Solid,
            NativeBorderStyle::Dashed,
            NativeBorderStyle::Dotted,
            NativeBorderStyle::Double,
        ]
    );
    assert_eq!(inherited_border.top.color, NativeColor::RED);

    let physical_border = border_for(physical).expect("physical complete-border command");
    assert_eq!(physical_border.top.width, 2);
    assert_eq!(physical_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(physical_border.top.color, NativeColor::RED);
    assert_eq!(physical_border.right.width, 0);
    assert_eq!(physical_border.bottom.width, 0);
    assert_eq!(physical_border.left.width, 0);

    for node_id in [
        reset,
        initial,
        revert,
        omitted,
        unpainted_child,
        zero_child,
        hidden_child,
    ] {
        assert!(
            border_for(node_id).is_none(),
            "expected no painted complete border for {node_id:?}"
        );
    }
    let mixed_border = border_for(mixed).expect("invalid mixed value keeps lower fallback");
    assert_eq!(mixed_border.top.width, 1);
    assert_eq!(mixed_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(
        mixed_border.top.color,
        NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        }
    );
    let composed_border = border_for(compose).expect("component declarations compose");
    assert_eq!(composed_border.top.width, 2);
    assert_eq!(composed_border.top.style, NativeBorderStyle::Solid);
    assert_eq!(
        composed_border.top.color,
        NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        }
    );
    for node_id in [invalid, layered, inline] {
        let borders = border_for(node_id).expect("inherited complete-border command");
        assert_eq!(
            [
                borders.top.width,
                borders.right.width,
                borders.bottom.width,
                borders.left.width,
            ],
            [2, 3, 4, 5]
        );
    }

    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border"
    }));
    assert!(!document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && matches!(
                diagnostic.detail.as_str(),
                "border-top" | "border-right" | "border-bottom" | "border-left"
            )
    }));

    let surface = list.rasterize().unwrap();
    let parent_box = layout.box_for(parent).unwrap();
    assert_eq!(
        surface.pixel(parent_box.x + parent_box.width / 2, parent_box.y),
        Some([255, 0, 0, 255])
    );
    let inherit_box = layout.box_for(inherit).unwrap();
    assert_eq!(
        surface.pixel(inherit_box.x + inherit_box.width / 2, inherit_box.y),
        Some([255, 0, 0, 255])
    );
    let compose_box = layout.box_for(compose).unwrap();
    assert_eq!(
        surface.pixel(compose_box.x + compose_box.width / 2, compose_box.y),
        Some([0, 128, 0, 255])
    );
    assert!(!surface.to_png().unwrap().is_empty());
}

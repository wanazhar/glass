use super::*;

#[test]
fn physical_border_color_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border-color: red !IMPORTANT; border-top-color: blue !important; border-right-color: invalid !important; border-right-color: green;",
    );
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
    assert_eq!(
        declarations.border_color,
        [
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(blue)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(green)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(NativeColor::RED)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(NativeColor::RED)
            )),
        ]
    );
    assert_eq!(
        declarations.border_color_important,
        [true, false, true, true]
    );
}

#[test]
fn stylesheet_cascade_resolves_physical_border_color_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { border-color: red !important; } #rollback { border-color: red !important; } #rollback { border-color: revert-layer !important; } #normal { border-color: red; } #mixed { border-color: red !important; } #sides { border-color: red !important; border-top-color: blue !important; border-right-color: green !important; } #sides { border-bottom-color: blue; border-left-color: green; } #invalid { border-color: red !important; border-color: invalid !important; } } @layer theme { #layered { border-color: blue !important; } #rollback { border-color: green !important; } #normal { border-color: blue; } #sides { border-color: green !important; } } #layered { border-color: green !important; } #normal { border-color: green; } #mixed { border-color: green; } #sides { border-color: green !important; } #inline { border-color: red !important; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        stylesheet
            .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
            .border_colors()
    };
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

    assert_eq!(computed("layered"), [NativeColor::RED; 4]);
    assert_eq!(computed("rollback"), [green; 4]);
    assert_eq!(computed("normal"), [green; 4]);
    assert_eq!(computed("mixed"), [NativeColor::RED; 4]);
    assert_eq!(
        computed("sides"),
        [blue, green, NativeColor::RED, NativeColor::RED]
    );
    assert_eq!(computed("invalid"), [NativeColor::RED; 4]);

    let inline = node("<div id='inline' style='border-color:green !important'>Inline</div>");
    assert_eq!(stylesheet.computed_for(&inline).border_colors(), [green; 4]);
}

#[test]
fn physical_border_width_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border-width: 1px !IMPORTANT; border-top-width: 2px !important; border-right-width: invalid !important; border-right-width: 3px;",
    );
    assert_eq!(
        declarations.border_width,
        [
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(2))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(3))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(1))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(1))
            )),
        ]
    );
    assert_eq!(
        declarations.border_width_important,
        [true, false, true, true]
    );
}

#[test]
fn stylesheet_cascade_resolves_physical_border_width_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #early { border-width: 1px !important; } #rollback { border-width: 1px !important; } #rollback { border-width: revert-layer !important; } #normal { border-width: 1px; } #mixed { border-top-width: 1px !important; } #sides { border-width: 1px !important; border-top-width: 2px !important; border-right-width: 3px !important; } #invalid { border-width: 1px !important; border-width: invalid !important; } } @layer theme { #early { border-width: 2px !important; } #rollback { border-width: 3px !important; } #normal { border-width: 2px; } #sides { border-width: 4px !important; } } #early { border-width: 3px !important; } #normal { border-width: 3px; } #mixed { border-width: 4px; } #sides { border-width: 5px !important; } #invalid { border-width: 5px; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        stylesheet
            .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
            .border_widths()
    };

    assert_eq!(computed("early"), [1; 4]);
    assert_eq!(computed("rollback"), [3; 4]);
    assert_eq!(computed("normal"), [3; 4]);
    assert_eq!(computed("mixed"), [1, 4, 4, 4]);
    assert_eq!(computed("sides"), [2, 3, 1, 1]);
    assert_eq!(computed("invalid"), [1; 4]);

    let inline = node("<div id='inline' style='border-width:4px !important'>Inline</div>");
    assert_eq!(stylesheet.computed_for(&inline).border_widths(), [4; 4]);
}

#[test]
fn physical_border_style_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border-style: solid !IMPORTANT; border-top-style: dashed !important; border-right-style: invalid !important; border-right-style: dotted;",
    );
    assert_eq!(
        declarations.border_style,
        [
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dashed
                ))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dotted
                ))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid
                ))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid
                ))
            )),
        ]
    );
    assert_eq!(
        declarations.border_style_important,
        [true, false, true, true]
    );
}

#[test]
fn stylesheet_cascade_resolves_physical_border_style_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #early { border-style: solid !important; } #rollback { border-style: dashed !important; } #rollback { border-style: revert-layer !important; } #normal { border-style: dotted; } #mixed { border-top-style: hidden !important; } #sides { border-style: solid !important; border-top-style: none !important; border-right-style: hidden !important; } #invalid { border-style: double !important; border-style: invalid !important; } } @layer theme { #early { border-style: dashed !important; } #rollback { border-style: groove !important; } #normal { border-style: double; } #sides { border-style: ridge !important; } } #early { border-style: dotted !important; } #normal { border-style: outset; } #mixed { border-style: double; } #sides { border-style: inset !important; } #invalid { border-style: dashed; } #wide { border-style: inherit !important; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        stylesheet
            .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
            .border_styles()
    };

    assert_eq!(
        computed("early"),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Solid); 4]
    );
    assert_eq!(
        computed("rollback"),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Groove); 4]
    );
    assert_eq!(
        computed("normal"),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Outset); 4]
    );
    assert_eq!(
        computed("mixed"),
        [
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
        ]
    );
    assert_eq!(
        computed("sides"),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        ]
    );
    assert_eq!(
        computed("invalid"),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Double); 4]
    );

    let inline = node("<div id='inline' style='border-style:hidden !important'>Inline</div>");
    assert_eq!(
        stylesheet.computed_for(&inline).border_styles(),
        [NativeBorderStyleValue::Hidden; 4]
    );
    let inherited_style = [NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted); 4];
    let wide = node("<div id='wide'>Wide</div>");
    assert_eq!(
        stylesheet
            .computed_for_with_matcher(
                &wide,
                NativeInheritedStyle {
                    border_style: inherited_style,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&wide),
            )
            .border_styles(),
        inherited_style
    );
}

#[test]
fn physical_border_shorthand_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border: 1px solid red !IMPORTANT; border-top: 2px dashed blue !important; border-right: invalid !important; border-right: 3px dotted green;",
    );
    assert_eq!(declarations.border_important, [true, false, true, true]);
}

#[test]
fn stylesheet_cascade_resolves_physical_border_shorthand_important_priority_and_composition() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #early { border:1px solid red !important; } #rollback { border:1px solid blue !important; } #rollback { border:revert-layer !important; } #normal { border:1px solid red; } #mixed { border-top:1px solid blue !important; } #sides { border:1px solid red !important; border-top:2px dashed green !important; border-right:3px dotted blue !important; } #invalid { border:1px solid red !important; border:invalid !important; } #none { border:2px solid red !important; border-top:none !important; } #hidden { border:2px solid red !important; border-right:hidden !important; } #wide { border:inherit !important; } } @layer theme { #early { border:2px dashed blue !important; } #rollback { border:3px groove green !important; } #normal { border:2px dashed blue; } #sides { border:4px groove green !important; } } #early { border:3px dotted green !important; } #normal { border:3px double green; } #mixed { border:4px double black; } #sides { border:5px ridge red !important; } #invalid { border:5px outset green; } #none { border-top:4px solid blue; } #hidden { border-right:4px solid blue; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| stylesheet.computed_for(&node(&format!("<div id='{id}'>Text</div>")));
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

    assert_eq!(computed("early").border_widths(), [1; 4]);
    assert_eq!(
        computed("early").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Solid); 4]
    );
    assert_eq!(computed("early").border_colors(), [NativeColor::RED; 4]);

    assert_eq!(computed("rollback").border_widths(), [3; 4]);
    assert_eq!(
        computed("rollback").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Groove); 4]
    );
    assert_eq!(computed("rollback").border_colors(), [green; 4]);

    assert_eq!(computed("normal").border_widths(), [3; 4]);
    assert_eq!(
        computed("normal").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Double); 4]
    );
    assert_eq!(computed("normal").border_colors(), [green; 4]);

    assert_eq!(computed("mixed").border_widths(), [1, 4, 4, 4]);
    assert_eq!(
        computed("mixed").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
        ]
    );
    assert_eq!(
        computed("mixed").border_colors(),
        [
            blue,
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::BLACK
        ]
    );

    assert_eq!(computed("sides").border_widths(), [2, 3, 1, 1]);
    assert_eq!(
        computed("sides").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        ]
    );
    assert_eq!(
        computed("sides").border_colors(),
        [green, blue, NativeColor::RED, NativeColor::RED]
    );

    assert_eq!(computed("invalid").border_widths(), [1; 4]);
    assert_eq!(
        computed("invalid").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Solid); 4]
    );
    assert_eq!(computed("invalid").border_colors(), [NativeColor::RED; 4]);

    assert_eq!(computed("none").border_widths(), [4, 2, 2, 2]);
    assert_eq!(
        computed("none").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        ]
    );
    assert_eq!(
        computed("none").border_colors(),
        [blue, NativeColor::RED, NativeColor::RED, NativeColor::RED]
    );
    assert_eq!(computed("hidden").border_widths(), [2, 4, 2, 2]);
    assert_eq!(
        computed("hidden").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        ]
    );
    assert_eq!(
        computed("hidden").border_colors(),
        [NativeColor::RED, blue, NativeColor::RED, NativeColor::RED]
    );

    let inherited = node("<div id='wide'>Wide</div>");
    let inherited_width = [9, 8, 7, 6];
    let inherited_style = [
        NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Ridge),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
    ];
    let inherited_color = [NativeColor::RED, blue, green, NativeColor::BLACK];
    let wide = stylesheet.computed_for_with_matcher(
        &inherited,
        NativeInheritedStyle {
            border_width: inherited_width,
            border_style: inherited_style,
            border_color: inherited_color,
            ..NativeInheritedStyle::default()
        },
        |selector| selector.matches(&inherited),
    );
    assert_eq!(wide.border_widths(), inherited_width);
    assert_eq!(wide.border_styles(), inherited_style);
    assert_eq!(wide.border_colors(), inherited_color);

    let inline = node("<div id='inline' style='border:6px groove blue !important'>Inline</div>");
    let inline_style = stylesheet.computed_for(&inline);
    assert_eq!(inline_style.border_widths(), [6; 4]);
    assert_eq!(
        inline_style.border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Groove); 4]
    );
    assert_eq!(inline_style.border_colors(), [blue; 4]);
}

#[test]
fn logical_border_color_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border-block-color: red !IMPORTANT; border-inline-start-color: blue !important; border-inline-end-color: invalid !important; border-inline-end-color: green;",
    );
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
    assert_eq!(
        declarations.logical_border.color,
        [
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(NativeColor::RED)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(NativeColor::RED)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(blue)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::Color(green)
            )),
        ]
    );
    assert_eq!(
        declarations.logical_border.color_important,
        [true, true, true, false]
    );
}

#[test]
fn stylesheet_cascade_resolves_logical_border_color_important_priority_and_projection() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #start { direction:ltr; border-inline-start-color:red !important; } #rtl { direction:rtl; border-inline-start-color:red !important; } #rollback { border-block-color:red !important; } #rollback { border-block-color:revert-layer !important; } #normal { border-block-color:red; } #mixed { border-inline-color:red !important; } #pair { border-block-color:red blue !important; } } @layer theme { #start { border-inline-start-color:blue !important; } #rtl { border-inline-start-color:blue !important; } #rollback { border-block-color:green !important; } #normal { border-block-color:blue; } #pair { border-block-color:green green !important; } } #start { border-inline-start-color:green !important; } #rtl { border-inline-start-color:green !important; } #normal { border-block-color:green; } #mixed { border-inline-color:green; } #pair { border-block-color:green green !important; } #inline { border-inline-start-color:red !important; } #rollback { border-block-color:green !important; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        stylesheet
            .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
            .border_colors()
    };
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

    assert_eq!(
        computed("start"),
        [
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::RED
        ]
    );
    assert_eq!(
        computed("rtl"),
        [
            NativeColor::BLACK,
            NativeColor::RED,
            NativeColor::BLACK,
            NativeColor::BLACK
        ]
    );
    assert_eq!(
        computed("rollback"),
        [green, NativeColor::BLACK, green, NativeColor::BLACK]
    );
    assert_eq!(
        computed("normal"),
        [green, NativeColor::BLACK, green, NativeColor::BLACK]
    );
    assert_eq!(
        computed("mixed"),
        [
            NativeColor::BLACK,
            NativeColor::RED,
            NativeColor::BLACK,
            NativeColor::RED
        ]
    );
    assert_eq!(
        computed("pair"),
        [
            NativeColor::RED,
            NativeColor::BLACK,
            blue,
            NativeColor::BLACK
        ]
    );

    let inline =
        node("<div id='inline' style='border-inline-start-color:green !important'>Inline</div>");
    assert_eq!(
        stylesheet.computed_for(&inline).border_colors(),
        [
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::BLACK,
            green
        ]
    );
}

#[test]
fn logical_border_width_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border-block-width: 1px 2px !IMPORTANT; border-inline-start-width: 3px !important; border-inline-end-width: invalid !important; border-inline-end-width: 4px;",
    );
    assert_eq!(
        declarations.logical_border.width,
        [
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(1)),
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(2)),
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(3)),
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(4)),
            )),
        ]
    );
    assert_eq!(
        declarations.logical_border.width_important,
        [true, true, true, false]
    );
}

#[test]
fn stylesheet_cascade_resolves_logical_border_width_important_priority_and_projection() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; border-inline-start-width:1px !important; } #rtl { direction:rtl; border-inline-start-width:1px !important; } #pair { border-block-width:1px 2px !important; } #rollback { border-block-width:1px !important; } #rollback { border-block-width:revert-layer !important; } #normal { border-block-width:1px; } #mixed { border-inline-width:1px !important; } #invalid { border-inline-width:1px !important; border-inline-width:invalid !important; } } @layer theme { #ltr { border-inline-start-width:2px !important; } #rtl { border-inline-start-width:2px !important; } #pair { border-block-width:3px 4px !important; } #rollback { border-block-width:5px !important; } #normal { border-block-width:2px; } } #ltr { border-inline-start-width:3px !important; } #rtl { border-inline-start-width:3px !important; } #normal { border-block-width:3px; } #mixed { border-inline-width:4px; } #invalid { border-inline-width:4px; } #wide { border-block-width:inherit !important; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        stylesheet
            .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
            .border_widths()
    };

    assert_eq!(computed("ltr"), [0, 0, 0, 1]);
    assert_eq!(computed("rtl"), [0, 1, 0, 0]);
    assert_eq!(computed("pair"), [1, 0, 2, 0]);
    assert_eq!(computed("rollback"), [5, 0, 5, 0]);
    assert_eq!(computed("normal"), [3, 0, 3, 0]);
    assert_eq!(computed("mixed"), [0, 1, 0, 1]);
    assert_eq!(computed("invalid"), [0, 1, 0, 1]);

    let inline =
        node("<div id='inline' style='border-inline-start-width:4px !important'>Inline</div>");
    assert_eq!(
        stylesheet.computed_for(&inline).border_widths(),
        [0, 0, 0, 4]
    );

    let inherited_width = [2, 3, 4, 5];
    let wide = node("<div id='wide'>Wide</div>");
    let wide_style = stylesheet.computed_for_with_matcher(
        &wide,
        NativeInheritedStyle {
            border_width: inherited_width,
            ..NativeInheritedStyle::default()
        },
        |selector| selector.matches(&wide),
    );
    assert_eq!(wide_style.border_widths(), [2, 0, 4, 0]);
}

#[test]
fn logical_border_style_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border-block-style: solid !IMPORTANT; border-inline-start-style: hidden !important; border-inline-end-style: invalid !important; border-inline-end-style: dotted;",
    );
    assert_eq!(
        declarations.logical_border.style,
        [
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid,
                )),
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Solid,
                )),
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Hidden),
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dotted,
                )),
            )),
        ]
    );
    assert_eq!(
        declarations.logical_border.style_important,
        [true, true, true, false]
    );
}

#[test]
fn stylesheet_cascade_resolves_logical_border_style_important_priority_and_projection() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; border-inline-start-style:dashed !important; } #rtl { direction:rtl; border-inline-start-style:dashed !important; } #pair { border-block-style:double dotted !important; } #rollback { border-block-style:groove !important; } #rollback { border-block-style:revert-layer !important; } #normal { border-block-style:dotted; } #mixed { border-inline-style:hidden !important; } #invalid { border-inline-style:double !important; border-inline-style:invalid !important; } #none { border-block-style:none !important; } } @layer theme { #ltr { border-inline-start-style:groove !important; } #rtl { border-inline-start-style:groove !important; } #pair { border-block-style:ridge outset !important; } #rollback { border-block-style:inset !important; } #normal { border-block-style:double; } } #ltr { border-inline-start-style:solid !important; } #rtl { border-inline-start-style:solid !important; } #normal { border-block-style:outset; } #mixed { border-inline-style:double; } #invalid { border-inline-style:solid; } #none { border-block-style:solid; } #wide { border-block-style:inherit !important; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        stylesheet
            .computed_for(&node(&format!("<div id='{id}'>Text</div>")))
            .border_styles()
    };

    assert_eq!(
        computed("ltr"),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
        ]
    );
    assert_eq!(
        computed("rtl"),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        computed("pair"),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        computed("rollback"),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        computed("normal"),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Outset),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Outset),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        computed("mixed"),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
        ]
    );
    assert_eq!(
        computed("invalid"),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
        ]
    );
    assert_eq!(
        computed("none"),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
        ]
    );

    let inline =
        node("<div id='inline' style='border-inline-start-style:hidden !important'>Inline</div>");
    assert_eq!(
        stylesheet.computed_for(&inline).border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
        ]
    );
    let inherited_style = [NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted); 4];
    let wide = node("<div id='wide'>Wide</div>");
    assert_eq!(
        stylesheet
            .computed_for_with_matcher(
                &wide,
                NativeInheritedStyle {
                    border_style: inherited_style,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&wide),
            )
            .border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::None,
        ]
    );
}

#[test]
fn logical_border_shorthand_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "border-block: 1px solid red !IMPORTANT; border-inline-start: 2px dashed blue !important; border-inline-end: invalid !important; border-inline-end: 3px dotted green;",
    );
    assert_eq!(
        declarations.logical_border.border_important,
        [true, true, true, false]
    );
}

#[test]
fn stylesheet_cascade_resolves_logical_border_shorthand_important_priority_and_projection() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; border-inline-start:1px solid red !important; } #rtl { direction:rtl; border-inline-start:1px solid red !important; } #pair { border-block:1px solid red !important; border-inline:2px dashed blue !important; } #rollback { border-block:1px solid red !important; } #rollback { border-block:revert-layer !important; } #normal { border-block:1px solid red; } #mixed { border-inline-start:1px solid blue !important; } #sides { border-block:1px solid red !important; border-block-start:2px dashed green !important; border-inline-end:3px dotted blue !important; } #invalid { border-block:1px solid red !important; border-block:invalid !important; } #none { border-block:2px solid red !important; border-block-start:none !important; } #hidden { border-inline:2px solid red !important; border-inline-end:hidden !important; } #wide { border-block:inherit !important; } } @layer theme { #ltr { border-inline-start:2px dashed blue !important; } #rtl { border-inline-start:2px dashed blue !important; } #rollback { border-block:3px groove green !important; } #normal { border-block:2px dashed blue; } #sides { border-block:4px groove green !important; } } #ltr { border-inline-start:3px dotted green !important; } #rtl { border-inline-start:3px dotted green !important; } #normal { border-block:3px double green; } #mixed { border-inline:4px double black; } #sides { border-inline:5px ridge red !important; } #none { border-block-start:4px solid blue; } #hidden { border-inline-end:4px solid blue; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| stylesheet.computed_for(&node(&format!("<div id='{id}'>Text</div>")));
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

    assert_eq!(computed("ltr").border_widths(), [0, 0, 0, 1]);
    assert_eq!(
        computed("ltr").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        ]
    );
    assert_eq!(
        computed("ltr").border_colors(),
        [
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::RED
        ]
    );
    assert_eq!(computed("rtl").border_widths(), [0, 1, 0, 0]);

    assert_eq!(computed("pair").border_widths(), [1, 2, 1, 2]);
    assert_eq!(
        computed("pair").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
        ]
    );
    assert_eq!(
        computed("pair").border_colors(),
        [NativeColor::RED, blue, NativeColor::RED, blue]
    );

    assert_eq!(computed("rollback").border_widths(), [3, 0, 3, 0]);
    assert_eq!(
        computed("rollback").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        computed("rollback").border_colors(),
        [green, NativeColor::BLACK, green, NativeColor::BLACK]
    );

    assert_eq!(computed("normal").border_widths(), [3, 0, 3, 0]);
    assert_eq!(
        computed("normal").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        computed("normal").border_colors(),
        [green, NativeColor::BLACK, green, NativeColor::BLACK]
    );

    assert_eq!(computed("mixed").border_widths(), [0, 4, 0, 1]);
    assert_eq!(
        computed("mixed").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        ]
    );
    assert_eq!(
        computed("mixed").border_colors(),
        [
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::BLACK,
            blue
        ]
    );

    assert_eq!(computed("sides").border_widths(), [2, 3, 1, 5]);
    assert_eq!(
        computed("sides").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Ridge),
        ]
    );
    assert_eq!(
        computed("sides").border_colors(),
        [green, blue, NativeColor::RED, NativeColor::RED]
    );

    assert_eq!(computed("invalid").border_widths(), [1, 0, 1, 0]);
    assert_eq!(
        computed("invalid").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::None,
        ]
    );

    assert_eq!(computed("none").border_widths(), [4, 0, 2, 0]);
    assert_eq!(
        computed("none").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        computed("none").border_colors(),
        [
            blue,
            NativeColor::BLACK,
            NativeColor::RED,
            NativeColor::BLACK
        ]
    );

    assert_eq!(computed("hidden").border_widths(), [0, 4, 0, 2]);
    assert_eq!(
        computed("hidden").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        ]
    );
    assert_eq!(
        computed("hidden").border_colors(),
        [
            NativeColor::BLACK,
            blue,
            NativeColor::BLACK,
            NativeColor::RED
        ]
    );

    let inherited = node("<div id='wide'>Wide</div>");
    let inherited_width = [9, 8, 7, 6];
    let inherited_style = [
        NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Ridge),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Inset),
    ];
    let inherited_color = [NativeColor::RED, blue, green, NativeColor::BLACK];
    let wide = stylesheet.computed_for_with_matcher(
        &inherited,
        NativeInheritedStyle {
            border_width: inherited_width,
            border_style: inherited_style,
            border_color: inherited_color,
            ..NativeInheritedStyle::default()
        },
        |selector| selector.matches(&inherited),
    );
    assert_eq!(wide.border_widths(), [9, 0, 7, 0]);
    assert_eq!(
        wide.border_styles(),
        [
            inherited_style[0],
            NativeBorderStyleValue::None,
            inherited_style[2],
            NativeBorderStyleValue::None
        ]
    );
    assert_eq!(
        wide.border_colors(),
        [
            inherited_color[0],
            NativeColor::BLACK,
            inherited_color[2],
            NativeColor::BLACK
        ]
    );

    let inline = node(
        "<div id='inline' style='border-inline-start:6px groove blue !important'>Inline</div>",
    );
    let inline_style = stylesheet.computed_for(&inline);
    assert_eq!(inline_style.border_widths(), [0, 0, 0, 6]);
    assert_eq!(
        inline_style.border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Groove),
        ]
    );
    assert_eq!(
        inline_style.border_colors(),
        [
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::BLACK,
            blue
        ]
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_color_candidates() {
    let document = NativeDocument::parse(
            "<style>.line { display:block; width:24px; height:20px; line-height:20px; color:green; text-decoration:underline; } @layer base { #named { text-decoration-color:red; } #rollback { text-decoration-color:red; } #repeated { text-decoration-color:red; } #unlayered { text-decoration-color:red; } #inline { text-decoration-color:red; } #fallback { text-decoration-color:revert-layer; } } @layer theme { .named { text-decoration-color:blue; } #rollback { text-decoration-color:blue; } #repeated { text-decoration-color:revert-layer; } #unlayered { text-decoration-color:blue; } #inline { text-decoration-color:blue; } } @layer top { #repeated { text-decoration-color:revert-layer; } } #named { text-decoration-color:revert-layer; } #unlayered { text-decoration-color:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='text-decoration-color:revert-layer'>A</div><div id='fallback' class='line'>A</div><div id='inherited' class='line' style='text-decoration-color:red'><span id='child'>A</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let color_for = |id| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_decoration_color()
    };
    assert_eq!(color_for("named"), Some(blue));
    assert_eq!(color_for("rollback"), Some(blue));
    assert_eq!(color_for("repeated"), Some(NativeColor::RED));
    assert_eq!(color_for("unlayered"), Some(blue));
    assert_eq!(color_for("inline"), Some(blue));
    assert_eq!(color_for("fallback"), None);
    assert_eq!(color_for("child"), None);
}

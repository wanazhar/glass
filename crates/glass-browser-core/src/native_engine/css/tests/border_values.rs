use super::*;

#[test]
fn border_revert_layer_rolls_back_independent_physical_sides() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #card { border: 1px solid red; } #sides { border-top: 1px solid red; border-right: 2px solid green; border-bottom: 3px solid blue; border-left: 4px solid black; } #fallback { border: revert-layer; } #inline { border: 1px solid red; } } @layer theme { #card { border: 3px dashed blue; } #card { border: revert-layer; } #sides { border-top: 5px dotted green; border-right: 6px solid blue; border-bottom: 7px dashed black; border-left: 8px solid red; } #sides { border-top: revert-layer; border-bottom: revert-layer; } #inline { border: 3px solid green; } } #card { border: revert-layer; } #sides { border-top: revert-layer; border-right: revert-layer; border-left: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let card = node("<div id='card'>Card</div>");
    let sides = node("<div id='sides'>Sides</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let inline = node("<div id='inline' style='border:ReVeRt-LaYeR'>Inline</div>");

    let card_border = stylesheet.computed_for(&card).border.unwrap();
    assert_eq!(card_border.top(), border_side(1, NativeColor::RED));
    assert_eq!(card_border.right(), border_side(1, NativeColor::RED));
    assert_eq!(card_border.bottom(), border_side(1, NativeColor::RED));
    assert_eq!(card_border.left(), border_side(1, NativeColor::RED));

    let sides_border = stylesheet.computed_for(&sides).border.unwrap();
    assert_eq!(sides_border.top(), border_side(1, NativeColor::RED));
    assert_eq!(
        sides_border.right(),
        border_side(
            6,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
        )
    );
    assert_eq!(
        sides_border.bottom(),
        border_side(
            3,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
        )
    );
    assert_eq!(sides_border.left(), border_side(8, NativeColor::RED));
    assert_eq!(stylesheet.computed_for(&fallback).border, None);

    let inline_border = stylesheet.computed_for(&inline).border.unwrap();
    assert_eq!(
        inline_border.top(),
        border_side(
            3,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },
        )
    );

    let declarations = parse_declarations(
        "border: 2px solid red; border: 1px wavy blue; border-top: ReVeRt-LaYeR; border-right: 3px dotted green; border-right: 1px dotted invalid",
    );
    assert_eq!(
        declarations.border,
        [
            Some(LocalCascadeDeclaration::RevertLayer),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Complete(styled_border_side(
                    3,
                    NativeBorderStyle::Dotted,
                    NativeColor {
                        red: 0,
                        green: 128,
                        blue: 0,
                        alpha: 255,
                    },
                )),
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Complete(border_side(2, NativeColor::RED)),
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Complete(border_side(2, NativeColor::RED)),
            )),
        ]
    );
    assert_eq!(
        parse_border_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_border_declaration("NoNe"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::None
        ))
    );
    assert_eq!(
            parse_declarations("border: none; border-top: NONE; border-right: none; border-bottom: none; border-left: NoNe").border,
            [Some(LocalCascadeDeclaration::Value(NativeBorderDeclaration::None)); 4]
        );
    assert_eq!(
        parse_border_declaration("HiDdEn"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Hidden
        ))
    );
    assert_eq!(
            parse_declarations("border: hidden; border-top: HIDDEN; border-right: hidden; border-bottom: Hidden; border-left: hIdDeN").border,
            [Some(LocalCascadeDeclaration::Value(NativeBorderDeclaration::Hidden)); 4]
        );
    assert_eq!(
        parse_border_declaration("2px solid currentColor"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteCurrentColor {
                width: 2,
                style: NativeBorderStyle::Solid,
            }
        ))
    );
    assert_eq!(
        parse_border_declaration("2px HiDdEn red"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteHidden {
                width: 2,
                color: NativeColor::RED,
            }
        ))
    );
    assert_eq!(
        parse_border_declaration("2px NoNe red"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CompleteNone {
                width: 2,
                color: NativeColor::RED,
            }
        ))
    );
    assert_eq!(
            parse_declarations(
                "border: 2px hidden red; border-top: 3px HIDDEN blue; border-right: 4px hidden green; border-bottom: 5px Hidden black; border-left: 6px hIdDeN white"
            )
            .border,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 3,
                        color: NativeColor {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 4,
                        color: NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 5,
                        color: NativeColor::BLACK,
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteHidden {
                        width: 6,
                        color: NativeColor::WHITE,
                    }
                )),
            ]
        );
    assert_eq!(
            parse_declarations(
                "border: 2px none red; border-top: 3px NoNe blue; border-right: 4px none green; border-bottom: 5px NONE black; border-left: 6px nOnE white"
            )
            .border,
            [
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 3,
                        color: NativeColor {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 4,
                        color: NativeColor {
                            red: 0,
                            green: 128,
                            blue: 0,
                            alpha: 255,
                        },
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 5,
                        color: NativeColor::BLACK,
                    }
                )),
                Some(LocalCascadeDeclaration::Value(
                    NativeBorderDeclaration::CompleteNone {
                        width: 6,
                        color: NativeColor::WHITE,
                    }
                )),
            ]
        );
    assert_eq!(
        parse_border_declaration("ReVeRt"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Revert
        ))
    );
    for (value, declaration) in [
        ("InHeRiT", NativeBorderDeclaration::Inherit),
        ("UNSET", NativeBorderDeclaration::Unset),
        ("initial", NativeBorderDeclaration::Initial),
    ] {
        assert_eq!(
            parse_border_declaration(value),
            Some(LocalCascadeDeclaration::Value(declaration)),
            "value={value}"
        );
    }
    assert_eq!(
        parse_declarations("border: inherit; border: invalid").border,
        [Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Inherit
        )); 4]
    );
    for value in [
        "inherit solid red",
        "2px inherit red",
        "2px solid inherit",
        "inherit 1px",
    ] {
        assert_eq!(parse_border_declaration(value), None, "value={value}");
    }
    assert_eq!(parse_border_declaration("none solid"), None);
    assert_eq!(parse_border_declaration("hidden solid"), None);
    assert_eq!(parse_border_declaration("none red"), None);
    assert_eq!(parse_border_declaration("2px none"), None);
    assert_eq!(parse_border_declaration("2px none red solid"), None);
    assert_eq!(parse_border_declaration("1px solid red dashed"), None);

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
            vec![
                "#valid { border: ReVeRt-LaYeR; border-top: 1px solid red; } #invalid { border-right: 1px wavy red; border-left: 1px solid; }"
                    .into(),
            ],
            &mut diagnostics,
        )
        .unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == "border-right"
            })
            .count(),
        1
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
                    && diagnostic.detail == "border-left"
            })
            .count(),
        1
    );
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "border"
    }));
}
#[test]
fn border_parser_accepts_custom_property_aliases_and_fallbacks() {
    let border = parse_custom_property_name("--border").unwrap();
    assert_eq!(
        parse_border_declaration("var(--border)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CustomProperty(border)
        ))
    );
    assert_eq!(
        parse_border_declaration("var(--border, 2px solid red)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::CustomPropertyFallback(
                border,
                border_side(2, NativeColor::RED)
            )
        ))
    );
    assert_eq!(
        parse_declarations("border: var(--border); border-left: var(--border, 3px dashed white)")
            .border,
        [
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CustomProperty(border)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CustomProperty(border)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CustomProperty(border)
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CustomPropertyFallback(
                    border,
                    styled_border_side(3, NativeBorderStyle::Dashed, NativeColor::WHITE)
                )
            )),
        ]
    );
    assert_eq!(
        parse_border_declaration("var(--border, 2px solid currentColor)"),
        None
    );
    assert_eq!(parse_border_declaration("var(--border, none)"), None);
}

#[test]
fn border_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent { --border: 1px solid red; --alias: var(--border); border: var(--alias); }
            #child { border: var(--alias); }
            #fallback { border: var(--missing, 5px dashed black); }
            #invalid { --bad: unsupported; border: var(--bad, 7px dotted white); }
            #cycle { --cycle: var(--cycle); border: var(--cycle, 8px double red); }
            #reset { --reset: initial; border: var(--reset, 9px solid red); }
            #none { --none: none; border: var(--none); }
            #longhand { --side: 10px solid white; border-left: var(--side); }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='none'>None</div>
            <div id='longhand'>Longhand</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(style("parent").border_widths(), [1; 4]);
    assert_eq!(
        style("parent").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Solid); 4]
    );
    assert_eq!(style("parent").border_colors(), [NativeColor::RED; 4]);
    assert_eq!(style("child").border_widths(), [1; 4]);
    assert_eq!(style("fallback").border_widths(), [5; 4]);
    assert_eq!(
        style("fallback").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed); 4]
    );
    assert_eq!(style("invalid").border_widths(), [7; 4]);
    assert_eq!(
        style("cycle").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Double); 4]
    );
    assert_eq!(style("reset").border_widths(), [0; 4]);
    assert_eq!(style("none").border_widths(), [0; 4]);
    assert_eq!(style("longhand").border_widths(), [0, 0, 0, 10]);
    assert_eq!(
        style("longhand").border_colors(),
        [
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::BLACK,
            NativeColor::WHITE
        ]
    );
}

#[test]
fn border_color_shorthand_custom_properties_resolve_with_fallbacks() {
    let blue = parse_color("blue").unwrap();
    let green = parse_color("green").unwrap();
    let yellow = parse_color("rgb(255,255,0)").unwrap();
    let document = NativeDocument::parse(
        r#"<style>
            #parent {
                --palette: red blue;
                border-color: var(--palette);
            }
            #child {
                --alias: var(--palette);
                border-color: var(--alias);
            }
            #fallback {
                border-color: var(--missing, green rgb(255,255,0));
            }
            #invalid {
                --bad: nonsense;
                border-color: var(--bad, white red);
            }
            #cycle {
                --cycle: var(--cycle);
                border-color: var(--cycle, black white);
            }
            #ordered {
                border-color: red;
                border-color: var(--missing, blue green);
            }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='ordered'>Ordered</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(
        style("parent").border_colors(),
        [NativeColor::RED, blue, NativeColor::RED, blue]
    );
    assert_eq!(
        style("child").border_colors(),
        style("parent").border_colors()
    );
    assert_eq!(
        style("fallback").border_colors(),
        [green, yellow, green, yellow]
    );
    assert_eq!(
        style("invalid").border_colors(),
        [
            NativeColor::WHITE,
            NativeColor::RED,
            NativeColor::WHITE,
            NativeColor::RED
        ]
    );
    assert_eq!(
        style("cycle").border_colors(),
        [
            NativeColor::BLACK,
            NativeColor::WHITE,
            NativeColor::BLACK,
            NativeColor::WHITE
        ]
    );
    assert_eq!(style("ordered").border_colors(), [blue, green, blue, green]);
}

#[test]
fn complete_border_current_color_parser_covers_physical_styles_and_rejections() {
    let declarations = parse_declarations(
        "border: 1px solid CURRENTcolor; border-top: 2px DASHED currentColor; border-right: 3px none CuRrEnTcOlOr; border-bottom: 4px HIDDEN CURRENTCOLOR; border-left: 5px double currentColor",
    );
    assert_eq!(
        declarations.border,
        [
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CompleteCurrentColor {
                    width: 2,
                    style: NativeBorderStyle::Dashed,
                }
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CompleteNoneCurrentColor { width: 3 }
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CompleteHiddenCurrentColor { width: 4 }
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::CompleteCurrentColor {
                    width: 5,
                    style: NativeBorderStyle::Double,
                }
            )),
        ]
    );
    assert_eq!(
        parse_border_declaration("2px solid red"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderDeclaration::Complete(border_side(2, NativeColor::RED))
        ))
    );

    for value in [
        "2px solid",
        "2px solid currentColor red",
        "2px wavy currentColor",
        "2px solid revert-layer",
        "2px solid inherit",
        "2px solid hsl(0 100% 50%)",
        "2px none",
        "2px hidden",
    ] {
        assert_eq!(parse_border_declaration(value), None, "{value}");
    }
}

#[test]
fn complete_border_css_wide_values_project_through_component_cascade() {
    let inherited_width = [2, 3, 4, 5];
    let inherited_style = [
        NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
        NativeBorderStyleValue::Paint(NativeBorderStyle::Double),
    ];
    let inherited_color = [
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
    ];
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border: inherit; } #physical { border-top: inherit; border-right: unset; border-bottom: initial; border-left: revert; } #reset { border: unset; } #initial { border: initial; } #revert { border: revert; } #omitted {} #invalid { border: inherit; border: invalid; } #mixed { border: inherit solid red; } #compose { color: green; border: unset; border-width: 2px; border-style: solid; } #root { border: inherit; }"
                .into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        let element = node(&format!("<div id='{id}'>Border</div>"));
        stylesheet.computed_for_with_matcher(
            &element,
            NativeInheritedStyle {
                color: Some(NativeColor::RED),
                border_color: inherited_color,
                border_width: inherited_width,
                border_style: inherited_style,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(&element),
        )
    };

    let inherited = computed("inherit").border().unwrap();
    assert_eq!(inherited.top(), border_side(2, NativeColor::RED));
    assert_eq!(inherited.right().width(), 3);
    assert_eq!(inherited.right().style(), NativeBorderStyle::Dashed);
    assert_eq!(inherited.right().color(), inherited_color[1]);
    assert_eq!(inherited.bottom().width(), 4);
    assert_eq!(inherited.bottom().style(), NativeBorderStyle::Dotted);
    assert_eq!(inherited.left().width(), 5);
    assert_eq!(inherited.left().style(), NativeBorderStyle::Double);

    let physical = computed("physical").border().unwrap();
    assert_eq!(physical.top(), border_side(2, NativeColor::RED));
    for side in [physical.right(), physical.bottom(), physical.left()] {
        assert_eq!(side.width(), 0);
        assert_eq!(side.style(), NativeBorderStyle::Solid);
    }
    for id in ["reset", "initial", "revert", "omitted", "mixed"] {
        assert!(computed(id).border().is_none(), "id={id}");
    }
    let invalid = computed("invalid").border().unwrap();
    assert_eq!(invalid.top(), border_side(2, NativeColor::RED));
    let composed = computed("compose").border().unwrap();
    assert_eq!(
        composed.top(),
        border_side(
            2,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            }
        )
    );
    assert_eq!(computed("root").border(), Some(inherited));
}

#[test]
fn logical_border_parsers_expand_pairs_and_preserve_css_wide_rules() {
    assert_eq!(
        parse_logical_border_width_pair("1px 2px"),
        Some([
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(
                NativeBorderWidthValue::Width(1),
            )),
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(
                NativeBorderWidthValue::Width(2),
            )),
        ])
    );
    assert_eq!(
        parse_logical_border_style_pair("NONE hidden"),
        Some([
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::None,
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Hidden,
            )),
        ])
    );
    assert_eq!(
        parse_logical_border_color_pair("currentColor #123456"),
        Some([
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CurrentColor),
            LocalCascadeDeclaration::Value(NativeBorderColorValue::Color(NativeColor {
                red: 0x12,
                green: 0x34,
                blue: 0x56,
                alpha: 255,
            })),
        ])
    );
    assert_eq!(
        parse_logical_border_width_pair("ReVeRt-LaYeR"),
        Some([LocalCascadeDeclaration::RevertLayer; 2])
    );
    for value in ["inherit 1px", "1px unset", "1px 2px 3px", "50%"] {
        assert_eq!(
            parse_logical_border_width_pair(value),
            None,
            "value={value}"
        );
    }

    let declarations = parse_declarations(
        "border-inline: 1px solid red; border-block-width: 2px 3px; border-inline-start-style: hidden; border-block-end-color: currentColor",
    );
    assert_eq!(
        declarations.logical_border.border,
        [
            None,
            None,
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Complete(border_side(1, NativeColor::RED))
            )),
            Some(LocalCascadeDeclaration::Value(
                NativeBorderDeclaration::Complete(border_side(1, NativeColor::RED))
            )),
        ]
    );
    assert_eq!(
        declarations.logical_border.width,
        [
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(2)),
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(3)),
            )),
            None,
            None,
        ]
    );
    assert_eq!(
        declarations.logical_border.style,
        [
            None,
            None,
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Hidden),
            )),
            None,
        ]
    );
    assert_eq!(
        declarations.logical_border.color,
        [
            None,
            Some(LocalCascadeDeclaration::Value(
                NativeBorderColorValue::CurrentColor
            )),
            None,
            None,
        ]
    );
}

#[test]
fn logical_border_color_parser_accepts_custom_property_aliases_and_fallbacks() {
    let blue = parse_color("blue").unwrap();
    let color = custom_property_hash("--accent");
    assert_eq!(
        parse_logical_border_color_pair("var(--accent)"),
        Some([
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyPair(color, 0)),
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyPair(color, 1)),
        ])
    );
    assert_eq!(
        parse_logical_border_color_pair("VAR(--accent, red blue)"),
        Some([
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyPairFallback(
                color,
                0,
                NativeColor::RED
            )),
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyPairFallback(
                color, 1, blue
            )),
        ])
    );
    assert!(parse_logical_border_color_pair("var(--accent, var(--other))").is_none());
    assert!(parse_logical_border_color_pair("var(--accent, inherit red)").is_none());
}

#[test]
fn border_color_shorthand_parser_accepts_custom_property_aliases_and_fallbacks() {
    let blue = parse_color("blue").unwrap();
    let color = custom_property_hash("--palette");
    assert_eq!(
        parse_border_color_declaration("var(--palette)"),
        Some([
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyShorthand(
                color, 0
            )),
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyShorthand(
                color, 1
            )),
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyShorthand(
                color, 2
            )),
            LocalCascadeDeclaration::Value(NativeBorderColorValue::CustomPropertyShorthand(
                color, 3
            )),
        ])
    );
    assert_eq!(
        parse_border_color_declaration("VAR(--palette, red blue)"),
        Some([
            LocalCascadeDeclaration::Value(
                NativeBorderColorValue::CustomPropertyShorthandFallback(color, 0, NativeColor::RED)
            ),
            LocalCascadeDeclaration::Value(
                NativeBorderColorValue::CustomPropertyShorthandFallback(color, 1, blue)
            ),
            LocalCascadeDeclaration::Value(
                NativeBorderColorValue::CustomPropertyShorthandFallback(color, 2, NativeColor::RED)
            ),
            LocalCascadeDeclaration::Value(
                NativeBorderColorValue::CustomPropertyShorthandFallback(color, 3, blue)
            ),
        ])
    );
    assert!(parse_border_color_declaration("var(--palette, var(--other))").is_none());
    assert!(parse_border_color_declaration("var(--palette, inherit red)").is_none());
}

#[test]
fn border_color_parser_expands_physical_values_and_rejects_mixed_forms() {
    let declarations = parse_declarations(
        "border-color: red green rgb(1, 2, 3) #123456; border-top-color: transparent; border-right-color: RGBA(4, 5, 6, 0.5); border-bottom-color: ReVeRt-LaYeR; border-left-color: #abcdef",
    );
    assert_eq!(
        declarations.border_color,
        [
            Some(LocalCascadeDeclaration::Value(border_color_value(
                NativeColor {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                }
            ))),
            Some(LocalCascadeDeclaration::Value(border_color_value(
                NativeColor {
                    red: 4,
                    green: 5,
                    blue: 6,
                    alpha: 128,
                }
            ))),
            Some(LocalCascadeDeclaration::RevertLayer),
            Some(LocalCascadeDeclaration::Value(border_color_value(
                NativeColor {
                    red: 171,
                    green: 205,
                    blue: 239,
                    alpha: 255,
                }
            ))),
        ]
    );
    assert_eq!(declarations.border_color_order, [1, 2, 3, 4]);
    assert_eq!(
        parse_border_color("red green blue black"),
        Some([
            border_color_value(NativeColor::RED),
            border_color_value(NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },),
            border_color_value(NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },),
            border_color_value(NativeColor::BLACK),
        ])
    );
    assert_eq!(
        parse_border_color("CURRENTcolor red"),
        Some([
            NativeBorderColorValue::CurrentColor,
            border_color_value(NativeColor::RED),
            NativeBorderColorValue::CurrentColor,
            border_color_value(NativeColor::RED),
        ])
    );
    assert_eq!(
        parse_border_color_declaration("revert-layer"),
        Some([LocalCascadeDeclaration::RevertLayer; 4])
    );
    for value in [
        "revert-layer red",
        "red revert-layer",
        "red green blue black white",
        "rgb(1, 2, 3",
    ] {
        assert_eq!(parse_border_color_declaration(value), None, "{value}");
    }
    assert_eq!(
        parse_border_color_side_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_border_color_side_declaration("CURRENTcolor"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CurrentColor
        ))
    );
    assert_eq!(
        parse_border_color_side_declaration("var(--accent)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CustomProperty(custom_property_hash("--accent"))
        ))
    );
    assert_eq!(
        parse_border_color_side_declaration("VAR(--accent, red)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBorderColorValue::CustomPropertyFallback(
                custom_property_hash("--accent"),
                NativeColor::RED
            )
        ))
    );
    assert_eq!(
        parse_border_color_side_declaration("var(--accent, var(--other))"),
        None
    );
    assert_eq!(
        parse_border_color("var(--accent)"),
        Some([NativeBorderColorValue::CustomProperty(custom_property_hash("--accent")); 4])
    );

    for (value, expected) in [
        ("InHeRiT", NativeBorderColorValue::Inherit),
        ("UnSeT", NativeBorderColorValue::Unset),
        ("InItIaL", NativeBorderColorValue::Initial),
        ("ReVeRt", NativeBorderColorValue::Revert),
    ] {
        assert_eq!(
            parse_border_color(value),
            Some([expected; 4]),
            "value={value}"
        );
        assert_eq!(
            parse_border_color_side_declaration(value),
            Some(LocalCascadeDeclaration::Value(expected)),
            "value={value}"
        );
    }
    for value in [
        "inherit red",
        "red inherit",
        "unset blue",
        "initial currentColor",
        "revert green",
        "inherit unset",
    ] {
        assert_eq!(parse_border_color_declaration(value), None, "{value}");
    }
    assert_eq!(parse_border_color_side_declaration("red blue"), None);
}
#[test]
fn inherited_border_color_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            "<style>#parent { --accent: rgb(1, 2, 3); --alias: var(--accent); --cycle: var(--cycle); color: green; border-color: var(--accent); } #child { border-color: var(--alias); } #fallback { border-color: var(--missing, red); } #invalid { --bad: nonsense; border-color: var(--bad, white); } #cycle { border-color: var(--cycle, white); }</style><div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let cycle = document.resolve_target("id=cycle").unwrap();
    let accent = NativeColor {
        red: 1,
        green: 2,
        blue: 3,
        alpha: u8::MAX,
    };

    assert_eq!(
        document.computed_style_for_layout(parent).border_colors(),
        [accent; 4]
    );
    assert_eq!(
        document.computed_style_for_layout(child).border_colors(),
        [accent; 4]
    );
    assert_eq!(
        document.computed_style_for_layout(fallback).border_colors(),
        [NativeColor::RED; 4]
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).border_colors(),
        [NativeColor::WHITE; 4]
    );
    assert_eq!(
        document.computed_style_for_layout(cycle).border_colors(),
        [NativeColor::WHITE; 4]
    );
}

#[test]
fn logical_border_color_custom_properties_resolve_with_fallbacks() {
    let yellow = parse_color("rgb(255,255,0)").unwrap();
    let blue = parse_color("blue").unwrap();
    let green = parse_color("green").unwrap();
    let document = NativeDocument::parse(
        r#"<style>
            #parent {
                --accent: red blue;
                border-block-color: var(--accent);
            }
            #child {
                --alias: var(--accent);
                border-block-color: var(--alias);
            }
            #fallback {
                border-inline-color: var(--missing, green rgb(255,255,0));
            }
            #invalid {
                --bad: nonsense;
                border-block-color: var(--bad, red blue);
            }
            #cycle {
                --cycle: var(--cycle);
                border-inline-color: var(--cycle, black white);
            }
            #ordered {
                border-block-color: red green;
                border-block-color: var(--missing, blue rgb(255,255,0));
            }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='ordered'>Ordered</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(
        style("parent").border_colors(),
        [
            NativeColor::RED,
            NativeColor::BLACK,
            blue,
            NativeColor::BLACK
        ]
    );
    assert_eq!(
        style("child").border_colors(),
        style("parent").border_colors()
    );
    assert_eq!(
        style("fallback").border_colors(),
        [NativeColor::BLACK, yellow, NativeColor::BLACK, green]
    );
    assert_eq!(
        style("invalid").border_colors(),
        [
            NativeColor::RED,
            NativeColor::BLACK,
            blue,
            NativeColor::BLACK
        ]
    );
    assert_eq!(
        style("cycle").border_colors(),
        [
            NativeColor::BLACK,
            NativeColor::WHITE,
            NativeColor::BLACK,
            NativeColor::BLACK
        ]
    );
    assert_eq!(
        style("ordered").border_colors(),
        [blue, NativeColor::BLACK, yellow, NativeColor::BLACK]
    );
}

#[test]
fn border_color_css_wide_keywords_preserve_local_fallback_and_parent_effective_colors() {
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let inherited_border_colors = [NativeColor::RED, green, NativeColor::BLACK, green];
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { color: blue; border: 2px solid red; border-color: inherit; } #unset { color: blue; border: 2px solid red; border-color: unset; } #initial { color: blue; border: 2px solid red; border-color: initial; } #revert { color: blue; border: 2px solid red; border-color: revert; } #current { color: blue; border: 2px solid red; border-color: currentColor; } #omitted { color: blue; border-width: 2px; border-style: solid; } #invalid { color: blue; border: 2px solid red; border-color: inherit; border-color: invalid; }"
                .into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        let element = node(&format!("<div id='{id}'>Border</div>"));
        stylesheet.computed_for_with_matcher(
            &element,
            NativeInheritedStyle {
                color: Some(NativeColor::RED),
                border_color: inherited_border_colors,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(&element),
        )
    };
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    assert_eq!(computed("inherit").border_colors(), inherited_border_colors);
    assert_eq!(computed("invalid").border_colors(), inherited_border_colors);
    for id in ["unset", "initial", "revert", "current"] {
        assert_eq!(computed(id).border_colors(), [blue; 4], "id={id}");
    }
    assert_eq!(computed("omitted").border_colors(), [NativeColor::BLACK; 4]);
    assert_eq!(
        computed("inherit").border().unwrap().top().color(),
        NativeColor::RED
    );
    assert_eq!(computed("inherit").border().unwrap().right().color(), green);
    assert_eq!(computed("unset").border().unwrap().top().color(), blue);
    assert_eq!(computed("invalid").border().unwrap().left().color(), green);
}

#[test]
fn border_color_revert_layer_resolves_component_candidates_in_order() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border: 2px solid red; border-color: red green blue black; } #repeat { border: 1px solid red; } #fallback { border: 1px solid red; } #same { border: 1px solid red; border-color: revert-layer; } #order-a { border-color: blue; border: 1px solid red; } #order-b { border: 1px solid red; border-color: blue; } } @layer theme { #named { border-color: blue green blue black; } #named { border-top-color: revert-layer; } #repeat { border-color: blue; } #repeat { border-color: revert-layer; } #fallback { border-color: revert-layer; } } @layer top { #repeat { border-color: revert-layer; } } #named { border-color: revert-layer; } #repeat { border-color: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let repeat = node("<div id='repeat'>Repeat</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let same = node("<div id='same'>Same</div>");
    let order_a = node("<div id='order-a'>Order A</div>");
    let order_b = node("<div id='order-b'>Order B</div>");

    let named_border = stylesheet.computed_for(&named).border.unwrap();
    assert_eq!(named_border.top(), border_side(2, NativeColor::RED));
    assert_eq!(
        named_border.right(),
        border_side(
            2,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },
        )
    );
    assert_eq!(
        named_border.bottom(),
        border_side(
            2,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
        )
    );
    assert_eq!(named_border.left(), border_side(2, NativeColor::BLACK));

    let repeat_border = stylesheet.computed_for(&repeat).border.unwrap();
    assert_eq!(repeat_border, uniform_border(1, NativeColor::RED));
    let fallback_border = stylesheet.computed_for(&fallback).border.unwrap();
    assert_eq!(fallback_border, uniform_border(1, NativeColor::RED));
    assert_eq!(
        stylesheet.computed_for(&same).border.unwrap(),
        uniform_border(1, NativeColor::BLACK)
    );
    assert_eq!(
        stylesheet.computed_for(&order_a).border.unwrap(),
        uniform_border(1, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&order_b).border.unwrap(),
        uniform_border(
            1,
            NativeColor {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
        )
    );
}

#[test]
fn border_width_parser_expands_physical_values_and_rejects_mixed_forms() {
    let declarations = parse_declarations(
        "border-width: 1px 2px 3px 4px; border-top-width: 5px; border-right-width: ReVeRt-LaYeR; border-bottom-width: 6px; border-left-width: 7px; border-left-width: invalid",
    );
    assert_eq!(
        declarations.border_width,
        [
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(5))
            )),
            Some(LocalCascadeDeclaration::RevertLayer),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(6))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Width(7))
            )),
        ]
    );
    assert_eq!(declarations.border_width_order, [1, 2, 3, 4]);
    assert_eq!(
        parse_border_width_declaration("1px 2px 3px 4px"),
        Some([
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(
                NativeBorderWidthValue::Width(1)
            )),
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(
                NativeBorderWidthValue::Width(2)
            )),
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(
                NativeBorderWidthValue::Width(3)
            )),
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(
                NativeBorderWidthValue::Width(4)
            )),
        ])
    );
    assert_eq!(
        parse_border_width_declaration("revert-layer"),
        Some([LocalCascadeDeclaration::RevertLayer; 4])
    );
    for value in [
        "revert-layer 1px",
        "1px revert-layer",
        "1px 2px 3px 4px 5px",
        "1px -1px",
        "1.5px",
        "50%",
        "medium",
        "1px solid red",
    ] {
        assert_eq!(parse_border_width_declaration(value), None, "{value}");
    }
    for (value, expected) in [
        ("InHeRiT", NativeBorderWidthValue::Inherit),
        ("UnSeT", NativeBorderWidthValue::Unset),
        ("InItIaL", NativeBorderWidthValue::Initial),
        ("ReVeRt", NativeBorderWidthValue::Revert),
    ] {
        assert_eq!(
            parse_border_width_value(value),
            Some(expected),
            "value={value}"
        );
        assert_eq!(
            parse_border_width_declaration(value),
            Some([LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::Value(expected)); 4]),
            "shorthand={value}"
        );
        assert_eq!(
            parse_local_physical_border_width_side_declaration(value),
            Some(LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::Value(expected),
            )),
            "longhand={value}"
        );
    }
    for value in ["inherit 1px", "1px unset", "initial 2px 3px", "4px revert"] {
        assert_eq!(parse_border_width_declaration(value), None, "mixed={value}");
    }
    assert_eq!(
        parse_local_physical_border_width_side_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_local_physical_border_width_side_declaration("1px 2px"),
        None
    );
}
#[test]
fn border_width_parser_accepts_custom_property_aliases_and_fallbacks() {
    let width = parse_custom_property_name("--width").unwrap();
    assert_eq!(
        parse_border_width_property("2px"),
        Some(BorderWidthDeclarationValue::Value(
            NativeBorderWidthValue::Width(2)
        ))
    );
    assert_eq!(
        parse_border_width_declaration("var(--width)"),
        Some([
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::CustomPropertyShorthand(
                width, 0
            )),
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::CustomPropertyShorthand(
                width, 1
            )),
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::CustomPropertyShorthand(
                width, 2
            )),
            LocalCascadeDeclaration::Value(BorderWidthDeclarationValue::CustomPropertyShorthand(
                width, 3
            )),
        ])
    );
    assert_eq!(
        parse_border_width_declaration("var(--width, 1px 2px)"),
        Some([
            LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::CustomPropertyShorthandFallback(
                    width,
                    0,
                    NativeBorderWidthValue::Width(1)
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::CustomPropertyShorthandFallback(
                    width,
                    1,
                    NativeBorderWidthValue::Width(2)
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::CustomPropertyShorthandFallback(
                    width,
                    2,
                    NativeBorderWidthValue::Width(1)
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::CustomPropertyShorthandFallback(
                    width,
                    3,
                    NativeBorderWidthValue::Width(2)
                )
            ),
        ])
    );
    assert_eq!(
        parse_local_physical_border_width_side_declaration("var(--width)"),
        Some(LocalCascadeDeclaration::Value(
            BorderWidthDeclarationValue::CustomProperty(width)
        ))
    );
    assert_eq!(
        parse_local_physical_border_width_side_declaration("var(--width, 3px)"),
        Some(LocalCascadeDeclaration::Value(
            BorderWidthDeclarationValue::CustomPropertyFallback(
                width,
                NativeBorderWidthValue::Width(3)
            )
        ))
    );
    assert_eq!(
        parse_border_width_declaration("var(--width, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_physical_border_width_side_declaration("var(--width, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_physical_border_width_side_declaration("initial"),
        Some(LocalCascadeDeclaration::Value(
            BorderWidthDeclarationValue::Value(NativeBorderWidthValue::Initial)
        ))
    );
    assert_eq!(
        parse_local_physical_border_width_side_declaration("revert-layer"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
}

#[test]
fn border_width_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --width: 1px 2px 3px 4px; --alias: var(--width); border-width: var(--alias); border-style: solid; border-color: red; }
            #child { border-width: var(--alias); border-style: solid; border-color: red; }
            #fallback { border-width: var(--missing, 5px 6px); border-style: solid; border-color: red; }
            #invalid { --bad: unsupported; border-width: var(--bad, 7px); border-style: solid; border-color: red; }
            #cycle { --cycle: var(--cycle); border-width: var(--cycle, 8px); border-style: solid; border-color: red; }
            #reset { --reset: initial; border-width: var(--reset, 9px); border-style: solid; border-color: red; }
            #longhand { --side: 10px; border-left-width: var(--side); border-style: solid; border-color: red; }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='longhand'>Longhand</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(style("parent").border_widths(), [1, 2, 3, 4]);
    assert_eq!(style("child").border_widths(), [1, 2, 3, 4]);
    assert_eq!(style("fallback").border_widths(), [5, 6, 5, 6]);
    assert_eq!(style("invalid").border_widths(), [7; 4]);
    assert_eq!(style("cycle").border_widths(), [8; 4]);
    assert_eq!(style("reset").border_widths(), [0; 4]);
    assert_eq!(style("longhand").border_widths(), [0, 0, 0, 10]);
}

#[test]
fn logical_border_width_parser_accepts_custom_property_aliases_and_fallbacks() {
    let width = parse_custom_property_name("--width").unwrap();
    assert_eq!(
        parse_logical_border_width_pair("var(--width, 1px 2px)"),
        Some([
            LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::CustomPropertyPairFallback(
                    width,
                    0,
                    NativeBorderWidthValue::Width(1),
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderWidthDeclarationValue::CustomPropertyPairFallback(
                    width,
                    1,
                    NativeBorderWidthValue::Width(2),
                )
            ),
        ])
    );
    assert_eq!(
        parse_local_physical_border_width_side_declaration("var(--width, 3px)"),
        Some(LocalCascadeDeclaration::Value(
            BorderWidthDeclarationValue::CustomPropertyFallback(
                width,
                NativeBorderWidthValue::Width(3),
            )
        ))
    );
    assert_eq!(
        parse_logical_border_width_pair("var(--width, var(--other))"),
        None
    );
    assert_eq!(parse_logical_border_width_pair("inherit 1px"), None);
    assert_eq!(
        parse_logical_border_width_pair("revert-layer"),
        Some([LocalCascadeDeclaration::RevertLayer; 2])
    );
}

#[test]
fn logical_border_width_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --width: 1px 2px; --alias: var(--width); border-block-width: var(--alias); border-style: solid; border-color: red; }
            #child { border-block-width: var(--alias); border-style: solid; border-color: red; }
            #fallback { border-block-width: var(--missing, 3px 4px); border-style: solid; border-color: red; }
            #invalid { --bad: unsupported; border-block-width: var(--bad, 5px); border-style: solid; border-color: red; }
            #cycle { --cycle: var(--cycle); border-block-width: var(--cycle, 6px); border-style: solid; border-color: red; }
            #reset { --reset: initial; border-block-width: var(--reset, 7px); border-style: solid; border-color: red; }
            #longhand { --side: 8px; border-inline-start-width: var(--side); border-style: solid; border-color: red; }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='longhand'>Longhand</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(style("parent").border_widths(), [1, 0, 2, 0]);
    assert_eq!(style("child").border_widths(), [1, 0, 2, 0]);
    assert_eq!(style("fallback").border_widths(), [3, 0, 4, 0]);
    assert_eq!(style("invalid").border_widths(), [5, 0, 5, 0]);
    assert_eq!(style("cycle").border_widths(), [6, 0, 6, 0]);
    assert_eq!(style("reset").border_widths(), [0; 4]);
    assert_eq!(style("longhand").border_widths(), [0, 0, 0, 8]);
}

#[test]
fn border_width_css_wide_keywords_resolve_effective_inherited_widths() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border-width: inherit; border-style: solid; border-color: red; } #physical { border-top-width: inherit; border-right-width: unset; border-bottom-width: initial; border-left-width: revert; border-style: solid; border-color: red; } #unset { border-width: unset; border-style: solid; border-color: red; } #initial { border-width: initial; border-style: solid; border-color: red; } #revert { border-width: revert; border-style: solid; border-color: red; } #omitted { border-style: solid; border-color: red; } #invalid { border-width: inherit; border-width: invalid; border-style: solid; border-color: red; }"
                .into(),
        ])
        .unwrap();
    let inherited_width = [2, 3, 4, 5];
    let computed = |id: &str| {
        let element = node(&format!("<div id='{id}'>Border</div>"));
        stylesheet.computed_for_with_matcher(
            &element,
            NativeInheritedStyle {
                border_width: inherited_width,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(&element),
        )
    };

    assert_eq!(computed("inherit").border_widths(), inherited_width);
    assert_eq!(computed("physical").border_widths(), [2, 0, 0, 0]);
    for id in ["unset", "initial", "revert", "omitted"] {
        assert_eq!(computed(id).border_widths(), [0; 4], "id={id}");
    }
    assert_eq!(computed("invalid").border_widths(), inherited_width);

    let inherited_border = computed("inherit").border().unwrap();
    assert_eq!(inherited_border.top().width(), 2);
    assert_eq!(inherited_border.right().width(), 3);
    assert_eq!(inherited_border.bottom().width(), 4);
    assert_eq!(inherited_border.left().width(), 5);
    let physical_border = computed("physical").border().unwrap();
    assert_eq!(physical_border.top().width(), 2);
    assert_eq!(physical_border.right().width(), 0);
    assert_eq!(physical_border.bottom().width(), 0);
    assert_eq!(physical_border.left().width(), 0);
    for id in ["unset", "initial", "revert"] {
        assert_eq!(computed(id).border().unwrap().top().width(), 0, "id={id}");
    }
    assert!(computed("omitted").border().is_none());
}

#[test]
fn border_width_revert_layer_resolves_component_candidates_in_order() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border: 1px solid red; border-width: 2px 3px; } #fallback { border: 1px solid red; } #same { border: 1px solid red; border-width: revert-layer; } #order-a { border-width: 3px; border: 1px solid red; } #order-b { border: 1px solid red; border-width: 3px; } #width-only { border-width: 4px; } #inline { border: 1px solid red; } } @layer theme { #named { border-width: 5px 6px 7px 8px; } #named { border-top-width: revert-layer; } #fallback { border-width: 4px; } #fallback { border-width: revert-layer; } } @layer top { #fallback { border-width: revert-layer; } } #named { border-width: revert-layer; } #fallback { border-width: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let same = node("<div id='same'>Same</div>");
    let order_a = node("<div id='order-a'>Order A</div>");
    let order_b = node("<div id='order-b'>Order B</div>");
    let width_only = node("<div id='width-only'>Width only</div>");
    let inline =
        node("<div id='inline' style='border:1px solid red;border-width:4px'>Inline</div>");

    let named_border = stylesheet.computed_for(&named).border.unwrap();
    assert_eq!(named_border.top(), border_side(2, NativeColor::RED));
    assert_eq!(named_border.right(), border_side(6, NativeColor::RED));
    assert_eq!(named_border.bottom(), border_side(7, NativeColor::RED));
    assert_eq!(named_border.left(), border_side(8, NativeColor::RED));
    assert_eq!(
        stylesheet.computed_for(&fallback).border.unwrap(),
        uniform_border(1, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&same).border.unwrap(),
        uniform_border(0, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&order_a).border.unwrap(),
        uniform_border(1, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&order_b).border.unwrap(),
        uniform_border(3, NativeColor::RED)
    );
    assert_eq!(stylesheet.computed_for(&width_only).border, None);
    assert_eq!(
        stylesheet.computed_for(&inline).border.unwrap(),
        uniform_border(4, NativeColor::RED)
    );
}

#[test]
fn border_style_parser_expands_physical_values_and_rejects_mixed_forms() {
    let declarations = parse_declarations(
        "border-style: solid dashed dotted solid; border-top-style: dotted; border-right-style: ReVeRt-LaYeR; border-bottom-style: invalid; border-left-style: dashed; border-left-style: invalid",
    );
    assert_eq!(
        declarations.border_style,
        [
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dotted
                ))
            )),
            Some(LocalCascadeDeclaration::RevertLayer),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dotted
                ))
            )),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                    NativeBorderStyle::Dashed
                ))
            )),
        ]
    );
    assert_eq!(declarations.border_style_order, [1, 2, 0, 4]);
    assert_eq!(
        parse_border_style_declaration("solid dashed dotted wavy"),
        None
    );
    assert_eq!(
        parse_border_style_declaration("solid dashed dotted solid"),
        Some([
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
            )),
        ])
    );
    assert_eq!(
        parse_border_style_declaration("revert-layer"),
        Some([LocalCascadeDeclaration::RevertLayer; 4])
    );
    for value in [
        "revert-layer solid",
        "solid revert-layer",
        "solid dashed dotted solid double",
        "solid 1px",
    ] {
        assert_eq!(parse_border_style_declaration(value), None, "{value}");
    }
    assert_eq!(
        parse_local_physical_border_style_side_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_border_style_declaration("none solid dashed none"),
        Some([
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::None
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::None
            )),
        ])
    );
    assert_eq!(
        parse_border_style_declaration("hidden solid dashed hidden"),
        Some([
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Hidden
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Hidden
            )),
        ])
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("solid dashed"),
        None
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("none"),
        Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::None),
        ))
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("HiDdEn"),
        Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Hidden),
        ))
    );
    assert_eq!(
        parse_border_style_declaration("double groove ridge inset"),
        Some([
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Double)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Groove)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Ridge)
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(
                NativeBorderStyleValue::Paint(NativeBorderStyle::Inset)
            )),
        ])
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("OuTsEt"),
        Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Paint(
                NativeBorderStyle::Outset,
            )),
        ))
    );
    for (value, expected) in [
        ("InHeRiT", NativeBorderStyleValue::Inherit),
        ("UnSeT", NativeBorderStyleValue::Unset),
        ("InItIaL", NativeBorderStyleValue::Initial),
        ("ReVeRt", NativeBorderStyleValue::Revert),
    ] {
        assert_eq!(
            parse_border_style_value(value),
            Some(expected),
            "value={value}"
        );
        assert_eq!(
            parse_border_style_declaration(value),
            Some([LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::Value(expected)); 4]),
            "shorthand={value}"
        );
        assert_eq!(
            parse_local_physical_border_style_side_declaration(value),
            Some(LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::Value(expected),
            )),
            "longhand={value}"
        );
    }
    for value in [
        "inherit solid",
        "solid unset",
        "initial dashed dotted",
        "double revert",
    ] {
        assert_eq!(parse_border_style_declaration(value), None, "mixed={value}");
    }
}
#[test]
fn border_style_parser_accepts_custom_property_aliases_and_fallbacks() {
    let style = parse_custom_property_name("--style").unwrap();
    assert_eq!(
        parse_border_style_property("solid"),
        Some(BorderStyleDeclarationValue::Value(
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid)
        ))
    );
    assert_eq!(
        parse_border_style_declaration("var(--style)"),
        Some([
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::CustomPropertyShorthand(
                style, 0
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::CustomPropertyShorthand(
                style, 1
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::CustomPropertyShorthand(
                style, 2
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::CustomPropertyShorthand(
                style, 3
            )),
        ])
    );
    assert_eq!(
        parse_border_style_declaration("var(--style, none hidden)"),
        Some([
            LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::CustomPropertyShorthandFallback(
                    style,
                    0,
                    NativeBorderStyleValue::None
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::CustomPropertyShorthandFallback(
                    style,
                    1,
                    NativeBorderStyleValue::Hidden
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::CustomPropertyShorthandFallback(
                    style,
                    2,
                    NativeBorderStyleValue::None
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::CustomPropertyShorthandFallback(
                    style,
                    3,
                    NativeBorderStyleValue::Hidden
                )
            ),
        ])
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("var(--style)"),
        Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::CustomProperty(style)
        ))
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("var(--style, hidden)"),
        Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::CustomPropertyFallback(
                style,
                NativeBorderStyleValue::Hidden
            )
        ))
    );
    assert_eq!(
        parse_border_style_declaration("var(--style, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("var(--style, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("initial"),
        Some(LocalCascadeDeclaration::Value(
            BorderStyleDeclarationValue::Value(NativeBorderStyleValue::Initial)
        ))
    );
    assert_eq!(
        parse_local_physical_border_style_side_declaration("revert-layer"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
}

#[test]
fn border_style_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --style: solid dashed; --alias: var(--style); border-style: var(--alias); border-width: 1px; border-color: red; }
            #child { border-style: var(--alias); border-width: 1px; border-color: red; }
            #fallback { border-style: var(--missing, none hidden); border-width: 1px; border-color: red; }
            #invalid { --bad: unsupported; border-style: var(--bad, hidden); border-width: 1px; border-color: red; }
            #cycle { --cycle: var(--cycle); border-style: var(--cycle, dashed); border-width: 1px; border-color: red; }
            #reset { --reset: initial; border-style: var(--reset, solid); border-width: 1px; border-color: red; }
            #longhand { --side: hidden; border-left-style: var(--side); border-width: 1px; border-color: red; }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='longhand'>Longhand</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(
        style("parent").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
        ]
    );
    assert_eq!(
        style("child").border_styles(),
        style("parent").border_styles()
    );
    assert_eq!(
        style("fallback").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
        ]
    );
    assert_eq!(
        style("invalid").border_styles(),
        [NativeBorderStyleValue::Hidden; 4]
    );
    assert_eq!(
        style("cycle").border_styles(),
        [NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed); 4]
    );
    assert_eq!(
        style("reset").border_styles(),
        [NativeBorderStyleValue::None; 4]
    );
    assert_eq!(
        style("longhand").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
        ]
    );
}

#[test]
fn logical_border_style_parser_accepts_custom_property_aliases_and_fallbacks() {
    let style = parse_custom_property_name("--style").unwrap();
    assert_eq!(
        parse_logical_border_style_pair("var(--style, none hidden)"),
        Some([
            LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::CustomPropertyPairFallback(
                    style,
                    0,
                    NativeBorderStyleValue::None,
                )
            ),
            LocalCascadeDeclaration::Value(
                BorderStyleDeclarationValue::CustomPropertyPairFallback(
                    style,
                    1,
                    NativeBorderStyleValue::Hidden,
                )
            ),
        ])
    );
    assert_eq!(
        parse_logical_border_style_pair("var(--style)"),
        Some([
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::CustomPropertyPair(
                style, 0
            )),
            LocalCascadeDeclaration::Value(BorderStyleDeclarationValue::CustomPropertyPair(
                style, 1
            )),
        ])
    );
    assert_eq!(
        parse_logical_border_style_pair("var(--style, var(--other))"),
        None
    );
    assert_eq!(parse_logical_border_style_pair("inherit solid"), None);
    assert_eq!(
        parse_logical_border_style_pair("revert-layer"),
        Some([LocalCascadeDeclaration::RevertLayer; 2])
    );
}

#[test]
fn logical_border_style_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --style: solid dashed; --alias: var(--style); border-block-style: var(--alias); border-width: 1px; border-color: red; }
            #child { border-block-style: var(--alias); border-width: 1px; border-color: red; }
            #fallback { border-block-style: var(--missing, none hidden); border-width: 1px; border-color: red; }
            #invalid { --bad: unsupported; border-block-style: var(--bad, dotted); border-width: 1px; border-color: red; }
            #cycle { --cycle: var(--cycle); border-block-style: var(--cycle, dashed); border-width: 1px; border-color: red; }
            #reset { --reset: initial; border-block-style: var(--reset, solid); border-width: 1px; border-color: red; }
            #longhand { --side: hidden; border-inline-start-style: var(--side); border-width: 1px; border-color: red; }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='longhand'>Longhand</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(
        style("parent").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        style("child").border_styles(),
        style("parent").border_styles()
    );
    assert_eq!(
        style("fallback").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        style("invalid").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dotted),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        style("cycle").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
            NativeBorderStyleValue::None,
        ]
    );
    assert_eq!(
        style("reset").border_styles(),
        [NativeBorderStyleValue::None; 4]
    );
    assert_eq!(
        style("longhand").border_styles(),
        [
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::Hidden,
        ]
    );
}

#[test]
fn border_style_css_wide_keywords_resolve_effective_inherited_styles() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { border-style: inherit; border-width: 1px; border-color: red; } #physical { border-top-style: inherit; border-right-style: unset; border-bottom-style: initial; border-left-style: revert; border-width: 1px; border-color: red; } #unset { border-style: unset; border-width: 1px; border-color: red; } #initial { border-style: initial; border-width: 1px; border-color: red; } #revert { border-style: revert; border-width: 1px; border-color: red; } #omitted { border-width: 1px; border-color: red; } #invalid { border-style: inherit; border-style: invalid; border-width: 1px; border-color: red; }"
                .into(),
        ])
        .unwrap();
    let inherited_style = [
        NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
        NativeBorderStyleValue::Hidden,
        NativeBorderStyleValue::None,
        NativeBorderStyleValue::Paint(NativeBorderStyle::Dashed),
    ];
    let computed = |id: &str| {
        let element = node(&format!("<div id='{id}'>Border</div>"));
        stylesheet.computed_for_with_matcher(
            &element,
            NativeInheritedStyle {
                border_style: inherited_style,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(&element),
        )
    };

    assert_eq!(computed("inherit").border_styles(), inherited_style);
    assert_eq!(
        computed("physical").border_styles(),
        [
            NativeBorderStyleValue::Paint(NativeBorderStyle::Solid),
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
            NativeBorderStyleValue::None,
        ]
    );
    for id in ["unset", "initial", "revert", "omitted"] {
        assert_eq!(
            computed(id).border_styles(),
            [NativeBorderStyleValue::None; 4],
            "id={id}"
        );
    }
    assert_eq!(computed("invalid").border_styles(), inherited_style);

    let inherited_border = computed("inherit").border().unwrap();
    assert_eq!(inherited_border.top().style(), NativeBorderStyle::Solid);
    assert_eq!(inherited_border.top().width(), 1);
    assert_eq!(inherited_border.left().style(), NativeBorderStyle::Dashed);
    assert_eq!(inherited_border.left().width(), 1);
    let physical_border = computed("physical").border().unwrap();
    assert_eq!(physical_border.top().style(), NativeBorderStyle::Solid);
    assert_eq!(physical_border.top().width(), 1);
    assert_eq!(physical_border.right().width(), 0);
    assert!(computed("unset").border().is_none());
    assert!(computed("omitted").border().is_none());
}

#[test]
fn border_style_revert_layer_resolves_component_candidates_in_order() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border: 1px solid red; border-style: dashed dotted; } #fallback { border: 1px solid red; } #same { border: 1px solid red; border-style: revert-layer; } #order-a { border-style: dotted; border: 1px solid red; } #order-b { border: 1px solid red; border-style: dashed; } #combined { border-width: 4px; border-style: dashed; border-color: green; } #style-only { border-style: dashed; } #inline { border: 1px solid red; } } @layer theme { #named { border-style: dotted solid dashed solid; } #named { border-top-style: revert-layer; } #fallback { border-style: dashed; } #fallback { border-style: revert-layer; } } @layer top { #fallback { border-style: revert-layer; } } #named { border-style: revert-layer; } #fallback { border-style: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let same = node("<div id='same'>Same</div>");
    let order_a = node("<div id='order-a'>Order A</div>");
    let order_b = node("<div id='order-b'>Order B</div>");
    let combined = node("<div id='combined'>Combined</div>");
    let style_only = node("<div id='style-only'>Style only</div>");
    let inline = node("<div id='inline' style='border-style:dotted'>Inline</div>");

    let named_border = stylesheet.computed_for(&named).border.unwrap();
    assert_eq!(
        named_border.top(),
        styled_border_side(1, NativeBorderStyle::Dashed, NativeColor::RED)
    );
    assert_eq!(
        named_border.right(),
        styled_border_side(1, NativeBorderStyle::Solid, NativeColor::RED)
    );
    assert_eq!(
        named_border.bottom(),
        styled_border_side(1, NativeBorderStyle::Dashed, NativeColor::RED)
    );
    assert_eq!(
        named_border.left(),
        styled_border_side(1, NativeBorderStyle::Solid, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&fallback).border.unwrap(),
        uniform_border(1, NativeColor::RED)
    );
    assert_eq!(stylesheet.computed_for(&same).border, None);
    assert_eq!(
        stylesheet.computed_for(&order_a).border.unwrap(),
        uniform_border(1, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&order_b).border.unwrap(),
        uniform_styled_border(1, NativeBorderStyle::Dashed, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&combined).border.unwrap(),
        NativeBorder {
            top: styled_border_side(
                4,
                NativeBorderStyle::Dashed,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                }
            ),
            right: styled_border_side(
                4,
                NativeBorderStyle::Dashed,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                }
            ),
            bottom: styled_border_side(
                4,
                NativeBorderStyle::Dashed,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                }
            ),
            left: styled_border_side(
                4,
                NativeBorderStyle::Dashed,
                NativeColor {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                }
            ),
        }
    );
    assert_eq!(stylesheet.computed_for(&style_only).border, None);
    assert_eq!(
        stylesheet.computed_for(&inline).border.unwrap(),
        uniform_styled_border(1, NativeBorderStyle::Dotted, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&inline).border.unwrap().top().style,
        NativeBorderStyle::Dotted
    );
}

#[test]
fn border_style_none_blocks_or_reveals_lower_components_without_public_leakage() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #blocked { border: 2px solid red; } #exposed { border: 2px solid red; } #repeated { border: 2px solid red; } #invalid { border: 2px solid red; } #physical { border: 2px solid red; } #style-only { border-style: none; } #inline { border: 2px solid red; } #inline-paint { border: 2px solid red; border-style: none; } } @layer theme { #blocked { border-style: none; } #exposed { border-style: none; border-style: revert-layer; } #repeated { border-style: none; border-style: revert-layer; border-style: revert-layer; } #invalid { border-style: none; border-style: invalid; } #physical { border-style: none dashed dotted solid; } } #blocked { border-style: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let blocked = node("<div id='blocked'>Blocked</div>");
    let exposed = node("<div id='exposed'>Exposed</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let physical = node("<div id='physical'>Physical</div>");
    let style_only = node("<div id='style-only'>Style only</div>");
    let inline = node("<div id='inline' style='border-style:none'>Inline</div>");
    let inline_paint = node("<div id='inline-paint' style='border-style:solid'>Inline paint</div>");

    assert_eq!(stylesheet.computed_for(&blocked).border, None);
    assert_eq!(
        stylesheet.computed_for(&exposed).border.unwrap(),
        uniform_border(2, NativeColor::RED)
    );
    assert_eq!(
        stylesheet.computed_for(&repeated).border.unwrap(),
        uniform_border(2, NativeColor::RED)
    );
    assert_eq!(stylesheet.computed_for(&invalid).border, None);
    assert_eq!(stylesheet.computed_for(&style_only).border, None);
    assert_eq!(stylesheet.computed_for(&inline).border, None);
    assert_eq!(
        stylesheet.computed_for(&inline_paint).border.unwrap(),
        uniform_border(2, NativeColor::RED)
    );

    let physical_border = stylesheet.computed_for(&physical).border.unwrap();
    assert_eq!(physical_border.top().width(), 0);
    assert_eq!(
        physical_border.right(),
        styled_border_side(2, NativeBorderStyle::Dashed, NativeColor::RED)
    );
    assert_eq!(
        physical_border.bottom(),
        styled_border_side(2, NativeBorderStyle::Dotted, NativeColor::RED)
    );
    assert_eq!(
        physical_border.left(),
        styled_border_side(2, NativeBorderStyle::Solid, NativeColor::RED)
    );
}

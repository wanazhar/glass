use super::*;

#[test]
fn stylesheet_cascade_layers_precede_specificity_and_reopen_by_source_order() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "@layer base { #target { color: red; } } @layer theme { .target { color: blue; } }".into(),
        "@layer base { #target { color: green; } }".into(),
    ])
    .unwrap();
    let target = node("<div id='target' class='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&target).color(),
        Some(NativeColor {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        })
    );

    let reopened = NativeStylesheet::from_sources(vec![
        "@layer base { #target { color: red; } } @layer base { #target { color: green; } }".into(),
    ])
    .unwrap();
    assert_eq!(
        reopened.computed_for(&target).color(),
        Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: 255,
        })
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_independent_paint_colors() {
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
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { color: red; background-color: red; } #rollback { color: red; background-color: red; } #repeated { color: red; background-color: red; } #unlayered { color: red; background-color: red; } #inline { color: red; background-color: red; } #independent { color: red; background-color: red; } #parent { color: red; } } @layer theme { .named { color: blue; background-color: blue; } #rollback { color: revert-layer; background-color: revert-layer; } #repeated { color: revert-layer; background-color: revert-layer; } #unlayered { color: blue; background-color: blue; } #inline { color: blue; background-color: blue; } #independent { color: revert-layer; background-color: blue; } #parent { color: blue; } } @layer top { #repeated { color: revert-layer; background-color: revert-layer; } } #named { color: revert-layer; background-color: revert-layer; } #unlayered { color: revert-layer; background-color: revert-layer; } #parent { color: revert-layer; } #fallback { color: revert-layer; background-color: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let unlayered = node("<div id='unlayered'>Unlayered</div>");
    let inline = node(
        "<div id='inline' style='color:revert-layer;background-color:revert-layer'>Inline</div>",
    );
    let independent = node("<div id='independent'>Independent</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let parent = node("<div id='parent'>Parent</div>");

    let colors = |value: &NativeNode| {
        let style = stylesheet.computed_for(value);
        (style.color(), style.background_color())
    };
    assert_eq!(colors(&named), (Some(blue), Some(blue)));
    assert_eq!(
        colors(&rollback),
        (Some(NativeColor::RED), Some(NativeColor::RED))
    );
    assert_eq!(
        colors(&repeated),
        (Some(NativeColor::RED), Some(NativeColor::RED))
    );
    assert_eq!(colors(&unlayered), (Some(blue), Some(blue)));
    assert_eq!(colors(&inline), (Some(blue), Some(blue)));
    assert_eq!(colors(&independent), (Some(NativeColor::RED), Some(blue)));
    assert_eq!(colors(&fallback), (None, None));
    assert_eq!(colors(&parent), (Some(blue), None));

    let inherited = stylesheet.computed_for_with_matcher(
        &node("<div style='color:revert-layer'>Inherited</div>"),
        NativeInheritedStyle {
            color: Some(green),
            ..NativeInheritedStyle::default()
        },
        |_| false,
    );
    assert_eq!(inherited.color(), Some(green));
    assert_eq!(inherited.background_color(), None);
}

#[test]
fn background_color_css_wide_keywords_preserve_optional_fallback() {
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #inherit { background-color: inherit; } #unset { background-color: unset; } #initial { background-color: initial; } #revert { background-color: revert; } #invalid { background-color: red; background-color: inherit; background-color: invalid; } #precedence { background-color: red; } } @layer theme { #precedence { background-color: blue; } } #precedence { background-color: inherit; }"
                .into(),
        ])
        .unwrap();
    let computed = |html: &str, inherited_background| {
        let element = node(html);
        stylesheet
            .computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    background_color: inherited_background,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
            .background_color()
    };
    assert_eq!(
        parse_background_color_declaration("InHeRiT"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::Inherit
        ))
    );
    assert_eq!(
        parse_background_color_declaration("UnSeT"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::Unset
        ))
    );
    assert_eq!(
        parse_background_color_declaration("InItIaL"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::Initial
        ))
    );
    assert_eq!(
        parse_background_color_declaration("ReVeRt"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::Revert
        ))
    );
    assert_eq!(
        parse_background_color_declaration("CuRrEnTcOlOr"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::CurrentColor
        ))
    );
    assert_eq!(
        parse_background_color_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_background_color_declaration("var(--accent)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::CustomProperty(custom_property_hash("--accent"))
        ))
    );
    assert_eq!(
        parse_background_color_declaration("var(--accent, red)"),
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::CustomPropertyFallback(
                custom_property_hash("--accent"),
                NativeColor::RED
            )
        ))
    );
    assert_eq!(
        parse_background_color_declaration("var(--accent, var(--other))"),
        None
    );

    assert_eq!(
        computed("<div id='inherit'>Inherit</div>", Some(green)),
        Some(green)
    );
    assert_eq!(computed("<div id='unset'>Unset</div>", Some(green)), None);
    assert_eq!(
        computed("<div id='initial'>Initial</div>", Some(green)),
        None
    );
    assert_eq!(computed("<div id='revert'>Revert</div>", Some(green)), None);
    assert_eq!(
        computed("<div id='invalid'>Invalid</div>", Some(green)),
        Some(green)
    );
    assert_eq!(
        computed("<div id='precedence'>Precedence</div>", Some(green)),
        Some(green)
    );
    assert_eq!(computed("<div id='inherit'>Root</div>", None), None);
    assert_eq!(
        NativeStylesheet::default()
            .computed_for(&node("<div id='omitted'>Omitted</div>"))
            .background_color(),
        None
    );
}
#[test]
fn inherited_background_color_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            "<style>#parent { --accent: rgb(1, 2, 3); --alias: var(--accent); --cycle: var(--cycle); background-color: var(--accent); } #child { background-color: var(--alias); } #fallback { background-color: var(--missing, red); } #invalid { --bad: nonsense; background-color: var(--bad, white); } #cycle { background-color: var(--cycle, white); }</style><div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span></div>",
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
        document
            .computed_style_for_layout(parent)
            .background_color(),
        Some(accent)
    );
    assert_eq!(
        document.computed_style_for_layout(child).background_color(),
        Some(accent)
    );
    assert_eq!(
        document
            .computed_style_for_layout(fallback)
            .background_color(),
        Some(NativeColor::RED)
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .background_color(),
        Some(NativeColor::WHITE)
    );
    assert_eq!(
        document.computed_style_for_layout(cycle).background_color(),
        Some(NativeColor::WHITE)
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_named_and_unlayered_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-skip-spaces: all; } #unlayered { text-decoration-skip-spaces: all; } } @layer theme { .named { text-decoration-skip-spaces: end; } #unlayered { text-decoration-skip-spaces: end; } } #named { text-decoration-skip-spaces: revert-layer; } #unlayered { text-decoration-skip-spaces: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let unlayered = node(
        "<div id='unlayered' style='text-decoration-skip-spaces:revert-layer'>Unlayered</div>",
    );
    assert_eq!(
        stylesheet
            .computed_for(&named)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::End
    );
    assert_eq!(
        stylesheet
            .computed_for(&unlayered)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::End
    );

    let repeated = NativeStylesheet::from_sources(vec![
            "@layer base { #target { text-decoration-skip-spaces: all; } } @layer theme { #target { text-decoration-skip-spaces: revert-layer; } } @layer top { #target { text-decoration-skip-spaces: revert-layer; } }"
                .into(),
        ])
        .unwrap();
    let target = node("<div id='target'>Target</div>");
    assert_eq!(
        repeated.computed_for(&target).text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::All
    );
}

#[test]
fn stylesheet_cascade_layers_bound_invalid_forms_and_layer_count() {
    let mut source = String::from(
        "@layer { #anonymous { color: red; } } @layer base, theme { #comma { color: red; } } @layer outer { @layer inner { #nested { color: red; } } } @layer statement;",
    );
    for index in 0..=MAX_NATIVE_NAMED_CASCADE_LAYERS {
        source.push_str(&format!(
            " @layer limit-{index} {{ #limit-{index} {{ color: red; }} }}"
        ));
    }
    let mut diagnostics = NativeDiagnosticSink::default();
    let stylesheet =
        NativeStylesheet::from_sources_with_diagnostics(vec![source], &mut diagnostics).unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail == "layer-name")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail == "layer-prelude")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail == "nested-layer")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail == "layer-statement")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.detail == "too-many-layers")
    );
    assert_eq!(
        stylesheet.rules.len(),
        MAX_NATIVE_NAMED_CASCADE_LAYERS.saturating_sub(1)
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_skip_ink_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-skip-ink: none; } #unlayered { text-decoration-skip-ink: auto; } } @layer theme { .named { text-decoration-skip-ink: auto; } #unlayered { text-decoration-skip-ink: none; } } #named { text-decoration-skip-ink: revert-layer; } #unlayered { text-decoration-skip-ink: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let unlayered =
        node("<div id='unlayered' style='text-decoration-skip-ink:revert-layer'>Unlayered</div>");
    assert_eq!(
        stylesheet.computed_for(&named).text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );
    assert_eq!(
        stylesheet
            .computed_for(&unlayered)
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::None
    );

    let repeated = NativeStylesheet::from_sources(vec![
            "@layer base { #target { text-decoration-skip-ink: none; } } @layer theme { #target { text-decoration-skip-ink: auto; } } @layer top { #target { text-decoration-skip-ink: revert-layer; } }"
                .into(),
        ])
        .unwrap();
    let target = node("<div id='target'>Target</div>");
    assert_eq!(
        repeated.computed_for(&target).text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );

    let root_fallback = NativeStylesheet::from_sources(vec![
        "@layer base { #target { text-decoration-skip-ink: revert-layer; } }".into(),
    ])
    .unwrap();
    assert_eq!(
        root_fallback
            .computed_for(&target)
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_style_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-style: dashed; } #rollback { text-decoration-style: solid; } #repeated { text-decoration-style: solid; } #inline { text-decoration-style: dotted; } #fallback { text-decoration-style: revert-layer; } } @layer theme { .named { text-decoration-style: dotted; } #rollback { text-decoration-style: double; } #repeated { text-decoration-style: revert-layer; } #inline { text-decoration-style: wavy; } } @layer top { #repeated { text-decoration-style: revert-layer; } } #named { text-decoration-style: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let inline = node("<div id='inline' style='text-decoration-style:revert-layer'>Inline</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    assert_eq!(
        stylesheet.computed_for(&named).text_decoration_style(),
        NativeTextDecorationStyle::Dotted
    );
    assert_eq!(
        stylesheet.computed_for(&rollback).text_decoration_style(),
        NativeTextDecorationStyle::Double
    );
    assert_eq!(
        stylesheet.computed_for(&repeated).text_decoration_style(),
        NativeTextDecorationStyle::Solid
    );
    assert_eq!(
        stylesheet.computed_for(&inline).text_decoration_style(),
        NativeTextDecorationStyle::Wavy
    );
    assert_eq!(
        stylesheet.computed_for(&fallback).text_decoration_style(),
        NativeTextDecorationStyle::Solid
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_thickness_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-decoration-thickness: 1px; } #rollback { text-decoration-thickness: 1px; } #repeated { text-decoration-thickness: 1px; } #unlayered { text-decoration-thickness: 2px; } #inline { text-decoration-thickness: 1px; } #fallback { text-decoration-thickness: revert-layer; } } @layer theme { .named { text-decoration-thickness: 2px; } #rollback { text-decoration-thickness: 3px; } #repeated { text-decoration-thickness: revert-layer; } #unlayered { text-decoration-thickness: 4px; } #inline { text-decoration-thickness: 2px; } } @layer top { #repeated { text-decoration-thickness: revert-layer; } } #named { text-decoration-thickness: revert-layer; } #unlayered { text-decoration-thickness: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let unlayered = node("<div id='unlayered'>Unlayered</div>");
    let inline =
        node("<div id='inline' style='text-decoration-thickness:revert-layer'>Inline</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    assert_eq!(
        stylesheet.computed_for(&named).text_decoration_thickness(),
        2
    );
    assert_eq!(
        stylesheet
            .computed_for(&rollback)
            .text_decoration_thickness(),
        3
    );
    assert_eq!(
        stylesheet
            .computed_for(&repeated)
            .text_decoration_thickness(),
        1
    );
    assert_eq!(
        stylesheet
            .computed_for(&unlayered)
            .text_decoration_thickness(),
        4
    );
    assert_eq!(
        stylesheet.computed_for(&inline).text_decoration_thickness(),
        2
    );
    assert_eq!(
        stylesheet
            .computed_for(&fallback)
            .text_decoration_thickness(),
        1
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_text_underline_offset_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { text-underline-offset: -1px; } #rollback { text-underline-offset: -1px; } #repeated { text-underline-offset: -1px; } #unlayered { text-underline-offset: -1px; } #inline { text-underline-offset: -1px; } #fallback { text-underline-offset: revert-layer; } } @layer theme { .named { text-underline-offset: -2px; } #rollback { text-underline-offset: revert-layer; } #repeated { text-underline-offset: revert-layer; } #unlayered { text-underline-offset: 4px; } #inline { text-underline-offset: 2px; } } @layer top { #repeated { text-underline-offset: revert-layer; } } #named { text-underline-offset: revert-layer; } #unlayered { text-underline-offset: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let unlayered = node("<div id='unlayered'>Unlayered</div>");
    let inline = node("<div id='inline' style='text-underline-offset:revert-layer'>Inline</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    assert_eq!(stylesheet.computed_for(&named).text_underline_offset(), -2);
    assert_eq!(
        stylesheet.computed_for(&rollback).text_underline_offset(),
        -1
    );
    assert_eq!(
        stylesheet.computed_for(&repeated).text_underline_offset(),
        -1
    );
    assert_eq!(
        stylesheet.computed_for(&unlayered).text_underline_offset(),
        4
    );
    assert_eq!(stylesheet.computed_for(&inline).text_underline_offset(), 2);
    assert_eq!(
        stylesheet.computed_for(&fallback).text_underline_offset(),
        0
    );
}

#[test]
fn stylesheet_cascade_resolves_uniform_border_with_inline_precedence() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { border: 1px solid red; } #card { border: 2px solid blue; }".into(),
    ])
    .unwrap();
    let node = node("<div id='card' style='border: 3px solid green'>Card</div>");
    assert_eq!(
        stylesheet.computed_for(&node).border(),
        Some(uniform_border(
            3,
            NativeColor {
                red: 0,
                green: 128,
                blue: 0,
                alpha: 255,
            },
        ))
    );
}

#[test]
fn stylesheet_cascade_resolves_border_radius_with_inline_precedence() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        ".card { border-radius: 1px; } #card { border-radius: 2px 3px; }".into(),
    ])
    .unwrap();
    let node =
        node("<div id='card' class='card' style='border-radius: 4px 5px 6px 7px'>Card</div>");
    assert_eq!(
        stylesheet.computed_for(&node).border_radius(),
        NativeBorderRadius {
            top_left: 4,
            top_right: 5,
            bottom_right: 6,
            bottom_left: 7,
        }
    );
}

#[test]
fn border_radius_revert_layer_rolls_back_named_and_inline_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { border-radius: 1px; } #repeat { border-radius: 2px; } #fallback { border-radius: revert-layer; } #inline { border-radius: 3px; } } @layer theme { #named { border-radius: 4px; } #repeat { border-radius: revert-layer; } } @layer top { #repeat { border-radius: revert-layer; } } #named { border-radius: revert-layer; } #repeat { border-radius: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let repeat = node("<div id='repeat'>Repeat</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let inline = node("<div id='inline' style='border-radius:ReVeRt-LaYeR'>Inline</div>");

    let style = |node| stylesheet.computed_for(node).border_radius();
    assert_eq!(
        style(&named),
        NativeBorderRadius {
            top_left: 4,
            top_right: 4,
            bottom_right: 4,
            bottom_left: 4,
        }
    );
    assert_eq!(
        style(&repeat),
        NativeBorderRadius {
            top_left: 2,
            top_right: 2,
            bottom_right: 2,
            bottom_left: 2,
        }
    );
    assert_eq!(style(&fallback), NativeBorderRadius::default());
    assert_eq!(
        style(&inline),
        NativeBorderRadius {
            top_left: 3,
            top_right: 3,
            bottom_right: 3,
            bottom_left: 3,
        }
    );

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
        vec![
            "#target { border-radius: ReVeRt-LaYeR; } #other { border-radius: 1px revert-layer; }"
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
                    && diagnostic.detail == "border-radius"
            })
            .count(),
        1
    );
}

#[test]
fn stylesheet_cascade_resolves_box_edges_per_physical_side() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { padding: 1px 2px; margin: 3px; } #card { padding-left: 5px; margin-top: 4px; }"
            .into(),
    ])
    .unwrap();
    let edge_node = node("<div id='card' style='padding-bottom:6px;margin-right:7px'>Card</div>");
    let style = stylesheet.computed_for(&edge_node);
    assert_eq!(
        style.padding(),
        NativeBoxEdges {
            top: 1,
            right: 2,
            bottom: 6,
            left: 5,
        }
    );
    assert_eq!(
        style.margin(),
        NativeBoxEdges {
            top: 4,
            right: 7,
            bottom: 3,
            left: 3,
        }
    );
    let auto_margin = style.margin_auto();
    assert!(!auto_margin.top());
    assert!(!auto_margin.right());
    assert!(!auto_margin.bottom());
    assert!(!auto_margin.left());

    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { margin: auto 2px; } #card { margin-left: 4px; }".into(),
    ])
    .unwrap();
    let auto_node = node("<div id='card' style='margin-top:3px;margin-bottom:auto'>Card</div>");
    let style = stylesheet.computed_for(&auto_node);
    assert_eq!(
        style.margin(),
        NativeBoxEdges {
            top: 3,
            right: 2,
            bottom: 0,
            left: 4,
        }
    );
    assert!(!style.margin_auto().top());
    assert!(!style.margin_auto().right());
    assert!(style.margin_auto().bottom());
    assert!(!style.margin_auto().left());
}

#[test]
fn stylesheet_cascade_resolves_line_height_with_inline_precedence() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        ".card { line-height: 16px; } #card { line-height: 20px; }".into(),
    ])
    .unwrap();
    let node = node("<div id='card' class='card' style='line-height: 28px'>Card</div>");
    assert_eq!(stylesheet.computed_for(&node).line_height(), Some(28));
}

#[test]
fn line_height_revert_layer_rolls_back_named_and_inline_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { line-height: 16px; } #repeated { line-height: 20px; } #inline { line-height: 24px; } } @layer theme { #named { line-height: 28px; } #repeated { line-height: revert-layer; } #inline { line-height: 32px; } } @layer top { #repeated { line-height: revert-layer; } } #named { line-height: revert-layer; } #repeated { line-height: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let inline = node("<div id='inline' style='line-height:ReVeRt-LaYeR'>Inline</div>");
    let fallback = node("<div id='fallback' style='line-height:revert-layer'>Fallback</div>");

    assert_eq!(stylesheet.computed_for(&named).line_height(), Some(28));
    assert_eq!(stylesheet.computed_for(&repeated).line_height(), Some(20));
    assert_eq!(stylesheet.computed_for(&inline).line_height(), Some(32));
    assert_eq!(stylesheet.computed_for(&fallback).line_height(), None);

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
        vec!["#target { line-height: revert-layer; }".into()],
        &mut diagnostics,
    )
    .unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "line-height"
    }));
}

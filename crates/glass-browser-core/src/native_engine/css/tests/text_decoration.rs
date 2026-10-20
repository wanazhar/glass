use super::*;

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_inherited_text_presentation() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #transform { text-transform: lowercase; } #weight { font-weight: normal; } #style { font-style: normal; } #break { word-break: normal; } #repeated { text-transform: lowercase; } #fallback { text-transform: revert-layer; } } @layer theme { #transform { text-transform: uppercase; } #weight { font-weight: bold; } #style { font-style: italic; } #break { word-break: break-all; } #repeated { text-transform: revert-layer; } #fallback { text-transform: revert-layer; } } @layer top { #repeated { text-transform: revert-layer; } } #transform { text-transform: revert-layer; } #weight { font-weight: revert-layer; } #style { font-style: revert-layer; } #break { word-break: revert-layer; } #fallback { text-transform: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let transform = node("<div id='transform'>Target</div>");
    let weight = node("<div id='weight'>Target</div>");
    let style = node("<div id='style'>Target</div>");
    let break_all = node("<div id='break'>Target</div>");
    let repeated = node("<div id='repeated'>Target</div>");
    let fallback = node("<div id='fallback'>Target</div>");

    assert_eq!(
        stylesheet.computed_for(&transform).text_transform(),
        TextTransformValue::Uppercase
    );
    assert_eq!(
        stylesheet.computed_for(&weight).font_weight(),
        FontWeightValue::Bold
    );
    assert_eq!(
        stylesheet.computed_for(&style).font_style(),
        FontStyleValue::Italic
    );
    assert_eq!(
        stylesheet.computed_for(&break_all).word_break(),
        WordBreakValue::BreakAll
    );
    assert_eq!(
        stylesheet.computed_for(&repeated).text_transform(),
        TextTransformValue::Lowercase
    );
    assert_eq!(
        stylesheet.computed_for(&fallback).text_transform(),
        TextTransformValue::None
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_inherited_text_spacing() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { word-spacing: 4px; letter-spacing: 5px; } #rollback { word-spacing: 6px; letter-spacing: 7px; } #repeated { word-spacing: 8px; letter-spacing: 9px; } #fallback { word-spacing: revert-layer; letter-spacing: revert-layer; } #invalid { word-spacing: 10px; letter-spacing: 11px; } } @layer theme { #named { word-spacing: 12px; letter-spacing: 13px; } #rollback { word-spacing: 14px; letter-spacing: 15px; } #repeated { word-spacing: revert-layer; letter-spacing: revert-layer; } #invalid { word-spacing: 16px; letter-spacing: 17px; } } @layer top { #repeated { word-spacing: revert-layer; letter-spacing: revert-layer; } } #named { word-spacing: revert-layer; letter-spacing: revert-layer; } #rollback { word-spacing: 18px; letter-spacing: revert-layer; } #fallback { word-spacing: revert-layer; letter-spacing: revert-layer; } #inline { word-spacing: 22px; letter-spacing: 23px; } #invalid { word-spacing: 20px; word-spacing: 1px 2px; letter-spacing: 21px; letter-spacing: normal; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let inline = node("<div id='inline'>Inline</div>");

    let assert_values = |element: &NativeNode, word: u32, letter: u32| {
        let style = stylesheet.computed_for(element);
        assert_eq!(style.word_spacing(), word);
        assert_eq!(style.letter_spacing(), letter);
    };

    assert_values(&named, 12, 13);
    assert_values(&rollback, 18, 15);
    assert_values(&repeated, 8, 9);
    assert_values(&fallback, 0, 0);
    assert_values(&invalid, 20, 21);
    assert_values(&inline, 22, 23);
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_inherited_vertical_align() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { vertical-align: top; } #rollback { vertical-align: middle; } #repeated { vertical-align: bottom; } #fallback { vertical-align: revert-layer; } #invalid { vertical-align: top; } } @layer theme { #named { vertical-align: bottom; } #rollback { vertical-align: top; } #repeated { vertical-align: revert-layer; } #invalid { vertical-align: middle; } } @layer top { #repeated { vertical-align: revert-layer; } } #named { vertical-align: revert-layer; } #rollback { vertical-align: middle; vertical-align: revert-layer; } #fallback { vertical-align: revert-layer; } #invalid { vertical-align: bottom; vertical-align: 1px; } #inline { vertical-align: top; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let inline = node("<div id='inline'>Inline</div>");

    let assert_value = |element: &NativeNode, expected: VerticalAlignValue| {
        assert_eq!(stylesheet.computed_for(element).vertical_align(), expected);
    };
    assert_value(&named, VerticalAlignValue::Bottom);
    assert_value(&rollback, VerticalAlignValue::Top);
    assert_value(&repeated, VerticalAlignValue::Bottom);
    assert_value(&fallback, VerticalAlignValue::Baseline);
    assert_value(&invalid, VerticalAlignValue::Bottom);
    assert_value(&inline, VerticalAlignValue::Top);
}

#[test]
fn text_decoration_parser_accepts_bounded_line_sets() {
    assert_eq!(
        parse_text_decoration("UNDERLINE"),
        Some(TextDecorationValue::new(true, false, false))
    );
    assert_eq!(
        parse_text_decoration("OVERLINE"),
        Some(TextDecorationValue::new(false, true, false))
    );
    assert_eq!(
        parse_text_decoration("line-through"),
        Some(TextDecorationValue::new(false, false, true))
    );
    assert_eq!(
        parse_text_decoration("none"),
        Some(TextDecorationValue::none())
    );
    assert_eq!(
        parse_text_decoration("underline overline line-through"),
        Some(TextDecorationValue::new(true, true, true))
    );
    assert_eq!(
        parse_text_decoration("LINE-THROUGH underline OVERLINE"),
        Some(TextDecorationValue::new(true, true, true))
    );
    assert_eq!(
        parse_text_decoration("underline overline"),
        Some(TextDecorationValue::new(true, true, false))
    );
    assert_eq!(parse_text_decoration("underline underline"), None);
    assert_eq!(parse_text_decoration("none underline"), None);
    assert_eq!(parse_text_decoration("underline none"), None);
    assert_eq!(parse_text_decoration("none none"), None);
    assert_eq!(parse_text_decoration("underline red"), None);
    assert_eq!(parse_text_decoration(""), None);
}

#[test]
fn text_decoration_line_longhand_shares_ordered_line_state() {
    let shorthand_then_longhand = parse_declarations(
        "text-decoration: underline; text-decoration-line: overline line-through",
    );
    assert_eq!(
        shorthand_then_longhand.text_decoration,
        Some(NativeTextDecorationDeclaration::Value(
            TextDecorationValue::new(false, true, true)
        ))
    );

    let longhand_then_shorthand = parse_declarations(
        "text-decoration-line: overline line-through; text-decoration: underline",
    );
    assert_eq!(
        longhand_then_shorthand.text_decoration,
        Some(NativeTextDecorationDeclaration::Value(
            TextDecorationValue::new(true, false, false)
        ))
    );
    assert_eq!(
        parse_declarations("text-decoration-line: none").text_decoration,
        Some(NativeTextDecorationDeclaration::Value(
            TextDecorationValue::none()
        ))
    );
    assert_eq!(
        parse_declarations("text-decoration-line: underline underline").text_decoration,
        None
    );
}

#[test]
fn text_decoration_style_parser_accepts_only_bounded_patterns() {
    assert_eq!(parse_border_style("SOLID"), Some(NativeBorderStyle::Solid));
    assert_eq!(
        parse_border_style("dashed"),
        Some(NativeBorderStyle::Dashed)
    );
    assert_eq!(
        parse_border_style("DOTTED"),
        Some(NativeBorderStyle::Dotted)
    );
    assert_eq!(
        parse_border_style("DOUBLE"),
        Some(NativeBorderStyle::Double)
    );
    assert_eq!(
        parse_border_style("groove"),
        Some(NativeBorderStyle::Groove)
    );
    assert_eq!(parse_border_style("RIDGE"), Some(NativeBorderStyle::Ridge));
    assert_eq!(parse_border_style("inset"), Some(NativeBorderStyle::Inset));
    assert_eq!(
        parse_border_style("OUTSET"),
        Some(NativeBorderStyle::Outset)
    );
    assert_eq!(parse_border_style("wavy"), None);
    assert_eq!(parse_border_style("solid dashed"), None);
    assert_eq!(parse_border_style(""), None);
    assert_eq!(
        parse_text_decoration_style("SOLID"),
        Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Solid
        ))
    );
    assert_eq!(
        parse_text_decoration_style("dashed"),
        Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Dashed
        ))
    );
    assert_eq!(
        parse_text_decoration_style("DOTTED"),
        Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Dotted
        ))
    );
    assert_eq!(
        parse_text_decoration_style("DoUbLe"),
        Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Double
        ))
    );
    assert_eq!(
        parse_text_decoration_style("WaVy"),
        Some(NativeTextDecorationStyleDeclaration::Value(
            NativeTextDecorationStyle::Wavy
        ))
    );
    assert_eq!(
        parse_text_decoration_style("ReVeRt-LaYeR"),
        Some(NativeTextDecorationStyleDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_decoration_style("InHeRiT"),
        Some(NativeTextDecorationStyleDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_decoration_style("InItIaL"),
        Some(NativeTextDecorationStyleDeclaration::Initial)
    );
    assert_eq!(
        parse_text_decoration_style("UnSeT"),
        Some(NativeTextDecorationStyleDeclaration::Unset)
    );
    assert_eq!(
        parse_text_decoration_style("ReVeRt"),
        Some(NativeTextDecorationStyleDeclaration::Revert)
    );
    assert_eq!(parse_text_decoration_style("zigzag"), None);
    assert_eq!(parse_text_decoration_style("solid double"), None);
    assert_eq!(parse_text_decoration_style("inherit initial"), None);
    assert_eq!(parse_text_decoration_style(""), None);
}

#[test]
fn text_decoration_style_is_inherited_and_cascaded() {
    let document = NativeDocument::parse(
            "<style>div { text-decoration-style: dotted; } #parent { text-decoration-style: dashed; } #explicit { text-decoration-style: solid; } #double { text-decoration-style: double; } #wavy { text-decoration-style: WaVy; } #invalid { text-decoration-style: zigzag; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='double'>Double</span><span id='wavy'>Wavy</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-style:dotted'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let double = document.resolve_target("id=double").unwrap();
    let wavy = document.resolve_target("id=wavy").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .text_decoration_style(),
        NativeTextDecorationStyle::Dashed
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherited)
            .text_decoration_style(),
        NativeTextDecorationStyle::Dashed
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .text_decoration_style(),
        NativeTextDecorationStyle::Solid
    );
    assert_eq!(
        document
            .computed_style_for_layout(double)
            .text_decoration_style(),
        NativeTextDecorationStyle::Double
    );
    assert_eq!(
        document
            .computed_style_for_layout(wavy)
            .text_decoration_style(),
        NativeTextDecorationStyle::Wavy
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_decoration_style(),
        NativeTextDecorationStyle::Dashed
    );
    assert_eq!(
        document
            .computed_style_for_layout(inline)
            .text_decoration_style(),
        NativeTextDecorationStyle::Dotted
    );
}

#[test]
fn text_decoration_skip_ink_parser_accepts_only_bounded_values() {
    assert_eq!(
        NativeInheritedStyle::default().text_decoration_skip_ink,
        NativeTextDecorationSkipInk::Auto
    );
    assert_eq!(
        parse_text_decoration_skip_ink("AUTO"),
        Some(NativeTextDecorationSkipInkDeclaration::Value(
            NativeTextDecorationSkipInk::Auto
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_ink("NoNe"),
        Some(NativeTextDecorationSkipInkDeclaration::Value(
            NativeTextDecorationSkipInk::None
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_ink("ReVeRt-LaYeR"),
        Some(NativeTextDecorationSkipInkDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_decoration_skip_ink("InHeRiT"),
        Some(NativeTextDecorationSkipInkDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_decoration_skip_ink("InItIaL"),
        Some(NativeTextDecorationSkipInkDeclaration::Initial)
    );
    assert_eq!(
        parse_text_decoration_skip_ink("UnSeT"),
        Some(NativeTextDecorationSkipInkDeclaration::Unset)
    );
    assert_eq!(
        parse_text_decoration_skip_ink("ReVeRt"),
        Some(NativeTextDecorationSkipInkDeclaration::Revert)
    );
    assert_eq!(parse_text_decoration_skip_ink("all"), None);
    assert_eq!(parse_text_decoration_skip_ink("inherit initial"), None);
    assert_eq!(parse_text_decoration_skip_ink(""), None);
    assert_eq!(
        parse_declarations("text-decoration-skip-ink: none").text_decoration_skip_ink,
        Some(NativeTextDecorationSkipInkDeclaration::Value(
            NativeTextDecorationSkipInk::None
        ))
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-ink: all").text_decoration_skip_ink,
        None
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-ink: revert-layer").text_decoration_skip_ink,
        Some(NativeTextDecorationSkipInkDeclaration::RevertLayer)
    );
}

#[test]
fn text_decoration_skip_spaces_parser_accepts_bounded_edge_values() {
    assert_eq!(
        NativeInheritedStyle::default().text_decoration_skip_spaces,
        NativeTextDecorationSkipSpaces::None
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("NONE"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::None
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("AlL"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::All
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("start"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::Start
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("END"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::End
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("start end"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::StartAndEnd
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("END start"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::StartAndEnd
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("INITIAL"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::StartAndEnd
        ))
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("InHeRiT"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("uNsEt"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Unset)
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("ReVeRt"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Revert)
    );
    assert_eq!(parse_text_decoration_skip_spaces("start start"), None);
    assert_eq!(parse_text_decoration_skip_spaces("none start"), None);
    assert_eq!(parse_text_decoration_skip_spaces("all end"), None);
    assert_eq!(
        parse_text_decoration_skip_spaces("revert"),
        Some(NativeTextDecorationSkipSpacesDeclaration::Revert)
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("ReVeRt-LaYeR"),
        Some(NativeTextDecorationSkipSpacesDeclaration::RevertLayer)
    );
    assert_eq!(parse_text_decoration_skip_spaces(""), None);
    assert_eq!(
        parse_declarations("text-decoration-skip-spaces: all").text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::All
        ))
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-spaces: start end").text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::StartAndEnd
        ))
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-spaces: initial").text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::Value(
            NativeTextDecorationSkipSpaces::StartAndEnd
        ))
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-spaces: inherit").text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::Inherit)
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-spaces: unset").text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::Unset)
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-spaces: revert").text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::Revert)
    );
    assert_eq!(
        parse_declarations("text-decoration-skip-spaces: revert-layer").text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::RevertLayer)
    );
}
#[test]
fn text_decoration_parser_accepts_custom_property_aliases_and_fallbacks() {
    let style = parse_custom_property_name("--style").unwrap();
    let skip_ink = parse_custom_property_name("--skip-ink").unwrap();
    let skip_spaces = parse_custom_property_name("--skip-spaces").unwrap();
    let thickness = parse_custom_property_name("--thickness").unwrap();
    let offset = parse_custom_property_name("--offset").unwrap();

    assert_eq!(
        parse_text_decoration_style("var(--style)"),
        Some(NativeTextDecorationStyleDeclaration::CustomProperty(style))
    );
    assert_eq!(
        parse_text_decoration_style("var(--style, wavy)"),
        Some(
            NativeTextDecorationStyleDeclaration::CustomPropertyFallback(
                style,
                NativeTextDecorationStyle::Wavy
            )
        )
    );
    assert_eq!(
        parse_text_decoration_skip_ink("var(--skip-ink, none)"),
        Some(
            NativeTextDecorationSkipInkDeclaration::CustomPropertyFallback(
                skip_ink,
                NativeTextDecorationSkipInk::None
            )
        )
    );
    assert_eq!(
        parse_text_decoration_skip_spaces("var(--skip-spaces, start end)"),
        Some(
            NativeTextDecorationSkipSpacesDeclaration::CustomPropertyFallback(
                skip_spaces,
                NativeTextDecorationSkipSpaces::StartAndEnd
            )
        )
    );
    assert_eq!(
        parse_text_decoration_thickness("var(--thickness, 4px)"),
        Some(NativeTextDecorationThicknessDeclaration::CustomPropertyFallback(thickness, 4))
    );
    assert_eq!(
        parse_text_underline_offset("var(--offset, -3px)"),
        Some(NativeTextUnderlineOffsetDeclaration::CustomPropertyFallback(offset, -3))
    );
    let missing_offset = parse_custom_property_name("--missing-offset").unwrap();
    assert_eq!(
        parse_text_underline_offset("var(--missing-offset, 4px)"),
        Some(NativeTextUnderlineOffsetDeclaration::CustomPropertyFallback(missing_offset, 4))
    );
    assert_eq!(
        parse_text_decoration_skip_ink("var(--skip-ink, var(--nested))"),
        None
    );
    assert_eq!(parse_text_decoration_style("var(--style, inherit)"), None);
    assert_eq!(
        parse_text_decoration_skip_spaces("var(--skip-spaces, start start)"),
        None
    );
    assert_eq!(
        parse_text_decoration_thickness("var(--thickness, 0px)"),
        None
    );
    assert_eq!(parse_text_underline_offset("var(--offset, 2em)"), None);
    let declarations = parse_declarations(
        "text-decoration-style: var(--style); text-decoration-skip-ink: var(--skip-ink); text-decoration-skip-spaces: var(--skip-spaces); text-decoration-thickness: var(--thickness); text-underline-offset: var(--offset)",
    );
    assert_eq!(
        declarations.text_decoration_style,
        Some(NativeTextDecorationStyleDeclaration::CustomProperty(style))
    );
    assert_eq!(
        declarations.text_decoration_skip_ink,
        Some(NativeTextDecorationSkipInkDeclaration::CustomProperty(
            skip_ink
        ))
    );
    assert_eq!(
        declarations.text_decoration_skip_spaces,
        Some(NativeTextDecorationSkipSpacesDeclaration::CustomProperty(
            skip_spaces
        ))
    );
    assert_eq!(
        declarations.text_decoration_thickness,
        Some(NativeTextDecorationThicknessDeclaration::CustomProperty(
            thickness
        ))
    );
    assert_eq!(
        declarations.text_underline_offset,
        Some(NativeTextUnderlineOffsetDeclaration::CustomProperty(offset))
    );
}

#[test]
fn text_decoration_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent {
                --style: wavy;
                --style-alias: var(--style);
                --skip-ink: none;
                --skip-spaces: start end;
                --thickness: 4px;
                --offset: -3px;
                text-decoration-style: var(--style-alias);
                text-decoration-skip-ink: var(--skip-ink);
                text-decoration-skip-spaces: var(--skip-spaces);
                text-decoration-thickness: var(--thickness);
                text-underline-offset: var(--offset);
            }
            #child {
                text-decoration-style: var(--style-alias);
                text-decoration-skip-ink: var(--skip-ink);
                text-decoration-skip-spaces: var(--skip-spaces);
                text-decoration-thickness: var(--thickness);
                text-underline-offset: var(--offset);
            }
            #fallback {
                text-decoration-style: var(--missing-style, double);
                text-decoration-skip-ink: var(--missing-ink, auto);
                text-decoration-skip-spaces: var(--missing-spaces, end);
                text-decoration-thickness: var(--missing-thickness, 2px);
                text-underline-offset: var(--missing-offset, 4px);
            }
            #invalid {
                --bad: unsupported;
                text-decoration-style: var(--bad, dotted);
                text-decoration-skip-ink: var(--bad, none);
                text-decoration-skip-spaces: var(--bad, all);
                text-decoration-thickness: var(--bad, 3px);
                text-underline-offset: var(--bad, -4px);
            }
            #cycle {
                --cycle: var(--cycle);
                text-decoration-style: var(--cycle, dashed);
                text-decoration-skip-ink: var(--cycle, none);
                text-decoration-skip-spaces: var(--cycle, start end);
                text-decoration-thickness: var(--cycle, 3px);
                text-underline-offset: var(--cycle, 3px);
            }
            #reset {
                --reset: initial;
                text-decoration-style: var(--reset, dotted);
                text-decoration-skip-ink: var(--reset, none);
                text-decoration-skip-spaces: var(--reset, end);
                text-decoration-thickness: var(--reset, 6px);
                text-underline-offset: var(--reset, 2px);
            }
            #override {
                --style: solid;
                --thickness: 2px;
                text-decoration-style: var(--style);
                text-decoration-style: dashed;
                text-decoration-thickness: var(--thickness);
                text-decoration-thickness: 4px;
            }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='override'>Override</div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    let assert_values = |id: &str,
                         decoration_style: NativeTextDecorationStyle,
                         skip_ink: NativeTextDecorationSkipInk,
                         skip_spaces: NativeTextDecorationSkipSpaces,
                         thickness: u32,
                         offset: i32| {
        let computed = style(id);
        assert_eq!(computed.text_decoration_style(), decoration_style);
        assert_eq!(computed.text_decoration_skip_ink(), skip_ink);
        assert_eq!(computed.text_decoration_skip_spaces(), skip_spaces);
        assert_eq!(computed.text_decoration_thickness(), thickness);
        assert_eq!(computed.text_underline_offset(), offset);
    };

    assert_values(
        "parent",
        NativeTextDecorationStyle::Wavy,
        NativeTextDecorationSkipInk::None,
        NativeTextDecorationSkipSpaces::StartAndEnd,
        4,
        -3,
    );
    assert_values(
        "child",
        NativeTextDecorationStyle::Wavy,
        NativeTextDecorationSkipInk::None,
        NativeTextDecorationSkipSpaces::StartAndEnd,
        4,
        -3,
    );
    assert_values(
        "fallback",
        NativeTextDecorationStyle::Double,
        NativeTextDecorationSkipInk::Auto,
        NativeTextDecorationSkipSpaces::End,
        2,
        4,
    );
    assert_values(
        "invalid",
        NativeTextDecorationStyle::Dotted,
        NativeTextDecorationSkipInk::None,
        NativeTextDecorationSkipSpaces::All,
        3,
        -4,
    );
    assert_values(
        "cycle",
        NativeTextDecorationStyle::Dashed,
        NativeTextDecorationSkipInk::None,
        NativeTextDecorationSkipSpaces::StartAndEnd,
        3,
        3,
    );
    assert_values(
        "reset",
        NativeTextDecorationStyle::Solid,
        NativeTextDecorationSkipInk::Auto,
        NativeTextDecorationSkipSpaces::StartAndEnd,
        1,
        0,
    );
    assert_values(
        "override",
        NativeTextDecorationStyle::Dashed,
        NativeTextDecorationSkipInk::Auto,
        NativeTextDecorationSkipSpaces::None,
        4,
        0,
    );
}

#[test]
fn text_decoration_skip_spaces_is_inherited_and_cascaded() {
    let document = NativeDocument::parse(
            "<style>div { text-decoration-skip-spaces: none; } #parent { text-decoration-skip-spaces: START END; } #explicit { text-decoration-skip-spaces: end; } #inherit { text-decoration-skip-spaces: INHERIT; } #unset { text-decoration-skip-spaces: UNSET; } #revert { text-decoration-skip-spaces: ReVeRt; } #inline-inherit { text-decoration-skip-spaces: end; } #invalid { text-decoration-skip-spaces: start start; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='inline-inherit' style='text-decoration-skip-spaces:inherit'>Inline inherit</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-skip-spaces:start'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let unset = document.resolve_target("id=unset").unwrap();
    let revert = document.resolve_target("id=revert").unwrap();
    let inline_inherit = document.resolve_target("id=inline-inherit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(document.root())
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::None
    );
    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::StartAndEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherited)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::StartAndEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::End
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherit)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::StartAndEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(unset)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::StartAndEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(revert)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::StartAndEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(inline_inherit)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::StartAndEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::StartAndEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(inline)
            .text_decoration_skip_spaces(),
        NativeTextDecorationSkipSpaces::Start
    );
}

#[test]
fn text_decoration_skip_ink_is_inherited_and_cascaded() {
    let document = NativeDocument::parse(
            "<style>div { text-decoration-skip-ink: none; } #parent { text-decoration-skip-ink: AUTO; } #explicit { text-decoration-skip-ink: none; } #invalid { text-decoration-skip-ink: all; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-skip-ink:none'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(document.root())
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );
    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherited)
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::None
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );
    assert_eq!(
        document
            .computed_style_for_layout(inline)
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::None
    );
}

#[test]
fn text_decoration_skip_ink_css_wide_resets_follow_parent_and_initial() {
    let document = NativeDocument::parse(
            "<style>#parent { text-decoration-skip-ink: none; } #initial { text-decoration-skip-ink: initial; } #inherit { text-decoration-skip-ink: inherit; } #unset { text-decoration-skip-ink: unset; } #revert { text-decoration-skip-ink: revert; } #invalid { text-decoration-skip-ink: none; text-decoration-skip-ink: all; } #important { text-decoration-skip-ink: none !important; } #important { text-decoration-skip-ink: initial; }</style><div id='parent'><span id='initial'>Initial</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='invalid'>Invalid</span><span id='important'>Important</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_decoration_skip_ink()
    };

    assert_eq!(
        document
            .computed_style_for_layout(document.root())
            .text_decoration_skip_ink(),
        NativeTextDecorationSkipInk::Auto
    );
    assert_eq!(style("parent"), NativeTextDecorationSkipInk::None);
    assert_eq!(style("initial"), NativeTextDecorationSkipInk::Auto);
    assert_eq!(style("inherit"), NativeTextDecorationSkipInk::None);
    assert_eq!(style("unset"), NativeTextDecorationSkipInk::None);
    assert_eq!(style("revert"), NativeTextDecorationSkipInk::None);
    assert_eq!(style("invalid"), NativeTextDecorationSkipInk::None);
    assert_eq!(style("important"), NativeTextDecorationSkipInk::None);
}

#[test]
fn text_decoration_thickness_parser_accepts_only_bounded_pixels() {
    assert_eq!(NativeInheritedStyle::default().text_decoration_thickness, 1);
    assert_eq!(
        parse_text_decoration_thickness("1px"),
        Some(NativeTextDecorationThicknessDeclaration::Value(1))
    );
    assert_eq!(
        parse_text_decoration_thickness("2PX"),
        Some(NativeTextDecorationThicknessDeclaration::Value(2))
    );
    assert_eq!(
        parse_text_decoration_thickness("4px"),
        Some(NativeTextDecorationThicknessDeclaration::Value(
            MAX_NATIVE_TEXT_DECORATION_THICKNESS
        ))
    );
    assert_eq!(
        parse_text_decoration_thickness("ReVeRt-LaYeR"),
        Some(NativeTextDecorationThicknessDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_decoration_thickness("InHeRiT"),
        Some(NativeTextDecorationThicknessDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_decoration_thickness("InItIaL"),
        Some(NativeTextDecorationThicknessDeclaration::Initial)
    );
    assert_eq!(
        parse_text_decoration_thickness("UnSeT"),
        Some(NativeTextDecorationThicknessDeclaration::Unset)
    );
    assert_eq!(
        parse_text_decoration_thickness("ReVeRt"),
        Some(NativeTextDecorationThicknessDeclaration::Revert)
    );
    assert_eq!(parse_text_decoration_thickness("0px"), None);
    assert_eq!(parse_text_decoration_thickness("5px"), None);
    assert_eq!(parse_text_decoration_thickness("auto"), None);
    assert_eq!(parse_text_decoration_thickness("1.5px"), None);
    assert_eq!(parse_text_decoration_thickness("2px 3px"), None);
    assert_eq!(parse_text_decoration_thickness("from-font"), None);
    assert_eq!(parse_text_decoration_thickness("inherit initial"), None);
    assert_eq!(parse_text_decoration_thickness("all"), None);
}

#[test]
fn text_decoration_thickness_is_inherited_and_cascaded() {
    let document = NativeDocument::parse(
            "<style>div { text-decoration-thickness: 4px; } #parent { text-decoration-thickness: 3px; } #explicit { text-decoration-thickness: 1px; } #invalid { text-decoration-thickness: 5px; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span><span id='inline' style='text-decoration-thickness:2px'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(document.root())
            .text_decoration_thickness(),
        1
    );
    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .text_decoration_thickness(),
        3
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherited)
            .text_decoration_thickness(),
        3
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .text_decoration_thickness(),
        1
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_decoration_thickness(),
        3
    );
    assert_eq!(
        document
            .computed_style_for_layout(inline)
            .text_decoration_thickness(),
        2
    );
}

#[test]
fn text_decoration_thickness_css_wide_resets_follow_parent_and_initial() {
    let document = NativeDocument::parse(
            "<style>#parent { text-decoration-thickness: 3px; } #initial { text-decoration-thickness: initial; } #inherit { text-decoration-thickness: inherit; } #unset { text-decoration-thickness: unset; } #revert { text-decoration-thickness: revert; } #invalid { text-decoration-thickness: 4px; text-decoration-thickness: 5px; } #important { text-decoration-thickness: 3px !important; } #important { text-decoration-thickness: 1px; }</style><div id='parent'><span id='initial'>Initial</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='invalid'>Invalid</span><span id='important'>Important</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_decoration_thickness()
    };

    assert_eq!(
        document
            .computed_style_for_layout(document.root())
            .text_decoration_thickness(),
        1
    );
    assert_eq!(style("parent"), 3);
    assert_eq!(style("initial"), 1);
    assert_eq!(style("inherit"), 3);
    assert_eq!(style("unset"), 3);
    assert_eq!(style("revert"), 3);
    assert_eq!(style("invalid"), 4);
    assert_eq!(style("important"), 3);
}

#[test]
fn text_underline_offset_parser_accepts_css_wide_resets_and_bounded_signed_pixels() {
    assert_eq!(NativeInheritedStyle::default().text_underline_offset, 0);
    assert_eq!(
        parse_text_underline_offset("-4px"),
        Some(NativeTextUnderlineOffsetDeclaration::Value(
            MIN_NATIVE_TEXT_UNDERLINE_OFFSET
        ))
    );
    assert_eq!(
        parse_text_underline_offset("-0PX"),
        Some(NativeTextUnderlineOffsetDeclaration::Value(0))
    );
    assert_eq!(
        parse_text_underline_offset("0px"),
        Some(NativeTextUnderlineOffsetDeclaration::Value(0))
    );
    assert_eq!(
        parse_text_underline_offset("4px"),
        Some(NativeTextUnderlineOffsetDeclaration::Value(4))
    );
    assert_eq!(
        parse_text_underline_offset("ReVeRt-LaYeR"),
        Some(NativeTextUnderlineOffsetDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_underline_offset("InHeRiT"),
        Some(NativeTextUnderlineOffsetDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_underline_offset("InItIaL"),
        Some(NativeTextUnderlineOffsetDeclaration::Initial)
    );
    assert_eq!(
        parse_text_underline_offset("UnSeT"),
        Some(NativeTextUnderlineOffsetDeclaration::Unset)
    );
    assert_eq!(
        parse_text_underline_offset("ReVeRt"),
        Some(NativeTextUnderlineOffsetDeclaration::Revert)
    );
    assert_eq!(parse_text_underline_offset("-5px"), None);
    assert_eq!(parse_text_underline_offset("5px"), None);
    assert_eq!(parse_text_underline_offset("+1px"), None);
    assert_eq!(parse_text_underline_offset("1.5px"), None);
    assert_eq!(parse_text_underline_offset("auto"), None);
    assert_eq!(parse_text_underline_offset("10%"), None);
    assert_eq!(parse_text_underline_offset("2px 3px"), None);
    assert_eq!(parse_text_underline_offset("inherit initial"), None);
}

#[test]
fn text_underline_offset_css_wide_resets_follow_parent_and_initial() {
    let document = NativeDocument::parse(
            "<style>#parent { text-underline-offset: -3px; } #initial { text-underline-offset: initial; } #inherit { text-underline-offset: inherit; } #unset { text-underline-offset: unset; } #revert { text-underline-offset: revert; } #invalid { text-underline-offset: 4px; text-underline-offset: 5px; } #important { text-underline-offset: -2px !important; } #important { text-underline-offset: 0px; }</style><div id='parent'><span id='initial'>Initial</span><span id='inherit'>Inherit</span><span id='unset'>Unset</span><span id='revert'>Revert</span><span id='invalid'>Invalid</span><span id='important'>Important</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_underline_offset()
    };

    assert_eq!(
        document
            .computed_style_for_layout(document.root())
            .text_underline_offset(),
        0
    );
    assert_eq!(style("parent"), -3);
    assert_eq!(style("initial"), 0);
    assert_eq!(style("inherit"), -3);
    assert_eq!(style("unset"), -3);
    assert_eq!(style("revert"), -3);
    assert_eq!(style("invalid"), 4);
    assert_eq!(style("important"), -2);
}

#[test]
fn text_underline_offset_is_inherited_and_cascaded() {
    let document = NativeDocument::parse(
            "<style>div { text-underline-offset: 4px; } #parent { text-underline-offset: -3px; } #explicit { text-underline-offset: 0px; } #invalid { text-underline-offset: 5px; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span><span id='inline' style='text-underline-offset:2px'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(document.root())
            .text_underline_offset(),
        0
    );
    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .text_underline_offset(),
        -3
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherited)
            .text_underline_offset(),
        -3
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .text_underline_offset(),
        0
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_underline_offset(),
        -3
    );
    assert_eq!(
        document
            .computed_style_for_layout(inline)
            .text_underline_offset(),
        2
    );
}

#[test]
fn text_decoration_color_is_local_and_uses_existing_color_values() {
    let blue = NativeColor {
        red: 0,
        green: 0,
        blue: u8::MAX,
        alpha: u8::MAX,
    };
    let transparent = NativeColor {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    };
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { text-decoration-color: red; } #target { text-decoration-color: blue; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target' style='text-decoration-color: #010203'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).text_decoration_color(),
        Some(NativeColor {
            red: 1,
            green: 2,
            blue: 3,
            alpha: u8::MAX,
        })
    );

    let document = NativeDocument::parse(
            "<style>#parent { text-decoration: underline; text-decoration-color: red; color: green; } #explicit { text-decoration-color: blue; } #transparent { text-decoration-color: transparent; } #invalid { text-decoration-color: currentColor; }</style><div id='parent'><span id='inherited'>Inherited</span><span id='explicit'>Explicit</span><span id='transparent'>Transparent</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let inherited = document.resolve_target("id=inherited").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let transparent_node = document.resolve_target("id=transparent").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .text_decoration_color(),
        Some(NativeColor::RED)
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherited)
            .text_decoration_color(),
        None
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .text_decoration_color(),
        Some(blue)
    );
    assert_eq!(
        document
            .computed_style_for_layout(transparent_node)
            .text_decoration_color(),
        Some(transparent)
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_decoration_color(),
        Some(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        })
    );
}

#[test]
fn text_decoration_color_parser_accepts_bounded_values_and_css_wide_keywords() {
    assert_eq!(
        parse_text_decoration_color("ReVeRt-LaYeR"),
        Some(NativeTextDecorationColorDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_decoration_color("blue"),
        Some(NativeTextDecorationColorDeclaration::Value(NativeColor {
            red: 0,
            green: 0,
            blue: u8::MAX,
            alpha: u8::MAX,
        }))
    );
    assert_eq!(
        parse_text_decoration_color("transparent"),
        Some(NativeTextDecorationColorDeclaration::Value(NativeColor {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        }))
    );
    assert_eq!(
        parse_text_decoration_color("CuRrEnTcOlOr"),
        Some(NativeTextDecorationColorDeclaration::CurrentColor)
    );
    for (value, expected) in [
        ("InHeRiT", NativeTextDecorationColorDeclaration::Inherit),
        ("UnSeT", NativeTextDecorationColorDeclaration::Unset),
        ("InItIaL", NativeTextDecorationColorDeclaration::Initial),
        ("ReVeRt", NativeTextDecorationColorDeclaration::Revert),
    ] {
        assert_eq!(
            parse_text_decoration_color(value),
            Some(expected),
            "value={value}"
        );
    }
    assert_eq!(
        parse_text_decoration_color("linear-gradient(red, blue)"),
        None
    );
    assert_eq!(
        parse_declarations("text-decoration-color: revert-layer").text_decoration_color,
        Some(NativeTextDecorationColorDeclaration::RevertLayer)
    );
    assert_eq!(
            parse_declarations(
                "text-decoration-color: red; text-decoration-color: currentColor; text-decoration-color: invalid"
            )
            .text_decoration_color,
            Some(NativeTextDecorationColorDeclaration::CurrentColor)
        );
    assert_eq!(
        parse_text_decoration_color("var(--accent)"),
        Some(NativeTextDecorationColorDeclaration::CustomProperty(
            custom_property_hash("--accent")
        ))
    );
    assert_eq!(
        parse_text_decoration_color("VAR(--accent, red)"),
        Some(
            NativeTextDecorationColorDeclaration::CustomPropertyFallback(
                custom_property_hash("--accent"),
                NativeColor::RED
            )
        )
    );
    assert_eq!(
        parse_text_decoration_color("var(--accent, var(--other))"),
        None
    );
}
#[test]
fn inherited_text_decoration_color_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            "<style>#parent { --accent: rgb(1, 2, 3); --alias: var(--accent); --cycle: var(--cycle); text-decoration-color: var(--accent); } #child { text-decoration-color: var(--alias); } #fallback { text-decoration-color: var(--missing, red); } #invalid { --bad: nonsense; text-decoration-color: var(--bad, white); } #cycle { text-decoration-color: var(--cycle, white); }</style><div id='parent'><span id='child'>Child</span><span id='fallback'>Fallback</span><span id='invalid'>Invalid</span><span id='cycle'>Cycle</span></div>",
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
            .text_decoration_color(),
        Some(accent)
    );
    assert_eq!(
        document
            .computed_style_for_layout(child)
            .text_decoration_color(),
        Some(accent)
    );
    assert_eq!(
        document
            .computed_style_for_layout(fallback)
            .text_decoration_color(),
        Some(NativeColor::RED)
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_decoration_color(),
        Some(NativeColor::WHITE)
    );
    assert_eq!(
        document
            .computed_style_for_layout(cycle)
            .text_decoration_color(),
        Some(NativeColor::WHITE)
    );
}

#[test]
fn text_decoration_color_css_wide_keywords_preserve_local_fallback_and_parent_effective_color() {
    let green = NativeColor {
        red: 0,
        green: 128,
        blue: 0,
        alpha: u8::MAX,
    };
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { text-decoration-color: inherit; } #unset { text-decoration-color: unset; } #initial { text-decoration-color: initial; } #revert { text-decoration-color: revert; } #current { text-decoration-color: currentColor; } #invalid { text-decoration-color: red; text-decoration-color: inherit; text-decoration-color: invalid; }".into(),
        ])
        .unwrap();
    let computed = |id: &str, inherited_decoration_color| {
        let element = node(&format!("<div id='{id}'>Text</div>"));
        stylesheet
            .computed_for_with_matcher(
                &element,
                NativeInheritedStyle {
                    color: Some(green),
                    text_decoration_color: inherited_decoration_color,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(&element),
            )
            .text_decoration_color()
    };
    assert_eq!(
        computed("inherit", NativeColor::RED),
        Some(NativeColor::RED)
    );
    for id in ["unset", "initial", "revert", "current"] {
        assert_eq!(computed(id, NativeColor::RED), Some(green), "id={id}");
    }
    assert_eq!(
        computed("invalid", NativeColor::RED),
        Some(NativeColor::RED)
    );
    let omitted = stylesheet
        .computed_for_with_matcher(
            &node("<div>Text</div>"),
            NativeInheritedStyle {
                color: Some(green),
                text_decoration_color: NativeColor::RED,
                ..NativeInheritedStyle::default()
            },
            |_| false,
        )
        .text_decoration_color();
    assert_eq!(omitted, None);
}

#[test]
fn paint_color_important_parser_tracks_valid_terminal_markers() {
    let declarations = parse_declarations(
        "background-color: red !IMPORTANT; color: blue !important; text-decoration-color: green !important; background-color: invalid !important;",
    );
    assert_eq!(
        declarations.background_color,
        Some(LocalCascadeDeclaration::Value(
            NativeBackgroundColorValue::Color(NativeColor::RED),
        ))
    );
    assert!(declarations.background_color_important);
    assert_eq!(
        declarations.color,
        Some(LocalCascadeDeclaration::Value(NativeColorValue::Color(
            NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            },
        )))
    );
    assert!(declarations.color_important);
    assert_eq!(
        declarations.text_decoration_color,
        Some(NativeTextDecorationColorDeclaration::Value(NativeColor {
            red: 0,
            green: 128,
            blue: 0,
            alpha: u8::MAX,
        }))
    );
    assert!(declarations.text_decoration_color_important);
}

#[test]
fn stylesheet_cascade_resolves_paint_color_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { background-color: red !important; color: red !important; text-decoration-color: red !important; } #rollback { background-color: red !important; background-color: revert-layer !important; color: red !important; color: revert-layer !important; text-decoration-color: red !important; text-decoration-color: revert-layer !important; } #normal { background-color: red; color: red; text-decoration-color: red; } #mixed { background-color: red !important; color: red !important; text-decoration-color: red !important; } } @layer theme { #layered { background-color: blue !important; color: blue !important; text-decoration-color: blue !important; } #rollback { background-color: green !important; color: green !important; text-decoration-color: green !important; } #normal { background-color: blue; color: blue; text-decoration-color: blue; } } #layered { background-color: green !important; color: green !important; text-decoration-color: green !important; } #normal { background-color: green; color: green; text-decoration-color: green; } #mixed { background-color: green; color: green; text-decoration-color: green; } #inline { background-color: red !important; color: red !important; text-decoration-color: red !important; }".into(),
        ])
        .unwrap();
    let computed = |id: &str| {
        let style = stylesheet.computed_for(&node(&format!("<div id='{id}'>Text</div>")));
        (
            style.background_color(),
            style.color(),
            style.text_decoration_color(),
        )
    };
    let red = NativeColor::RED;
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

    assert_eq!(computed("layered"), (Some(red), Some(red), Some(red)));
    assert_eq!(
        computed("rollback"),
        (Some(green), Some(green), Some(green))
    );
    assert_eq!(computed("normal"), (Some(green), Some(green), Some(green)));
    assert_eq!(computed("mixed"), (Some(red), Some(red), Some(red)));

    let inline = node(
        "<div id='inline' style='background-color:green !important; color:green !important; text-decoration-color:green !important'>Inline</div>",
    );
    assert_eq!(
        stylesheet.computed_for(&inline).background_color(),
        Some(green)
    );
    assert_eq!(stylesheet.computed_for(&inline).color(), Some(green));
    assert_eq!(
        stylesheet.computed_for(&inline).text_decoration_color(),
        Some(green)
    );
    assert_ne!(computed("layered").0, Some(blue));
}

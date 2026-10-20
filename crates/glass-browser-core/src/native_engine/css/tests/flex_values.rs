use super::*;

#[test]
fn flex_grow_parser_accepts_only_bounded_non_negative_integers() {
    assert_eq!(parse_flex_grow("0"), Some(0));
    assert_eq!(parse_flex_grow("1024"), Some(1024));
    assert_eq!(parse_flex_grow(" 12 "), Some(12));
    assert_eq!(parse_flex_grow("-1"), None);
    assert_eq!(parse_flex_grow("+1"), None);
    assert_eq!(parse_flex_grow("1.5"), None);
    assert_eq!(parse_flex_grow("1px"), None);
    assert_eq!(parse_flex_grow("1025"), None);
    assert_eq!(parse_flex_grow("1e2"), None);
}

#[test]
fn flex_shrink_parser_accepts_only_bounded_non_negative_integers() {
    assert_eq!(parse_flex_shrink("0"), Some(0));
    assert_eq!(parse_flex_shrink("1024"), Some(1024));
    assert_eq!(parse_flex_shrink(" 12 "), Some(12));
    assert_eq!(parse_flex_shrink("-1"), None);
    assert_eq!(parse_flex_shrink("+1"), None);
    assert_eq!(parse_flex_shrink("1.5"), None);
    assert_eq!(parse_flex_shrink("1px"), None);
    assert_eq!(parse_flex_shrink("1025"), None);
    assert_eq!(parse_flex_shrink("1e2"), None);
}

#[test]
fn flex_basis_parser_accepts_auto_and_bounded_non_negative_pixels() {
    assert_eq!(parse_flex_basis("auto"), Some(FlexBasisValue::Auto));
    assert_eq!(parse_flex_basis("AUTO"), Some(FlexBasisValue::Auto));
    assert_eq!(parse_flex_basis("0px"), Some(FlexBasisValue::Length(0)));
    assert_eq!(parse_flex_basis(" 12px "), Some(FlexBasisValue::Length(12)));
    assert_eq!(parse_flex_basis("-1px"), None);
    assert_eq!(parse_flex_basis("12"), None);
    assert_eq!(parse_flex_basis("1.5px"), None);
    assert_eq!(parse_flex_basis("2em"), None);
    assert_eq!(parse_flex_basis("50%"), None);
    assert_eq!(parse_flex_basis("calc(12px)"), None);
    assert_eq!(parse_flex_basis("content"), None);
    assert_eq!(parse_flex_basis("20000px"), None);
    assert_eq!(
        parse_declarations("flex-basis: 12px; flex-basis: 1.5px").flex_basis,
        Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(12)))
    );
}
#[test]
fn flex_sizing_parser_accepts_custom_property_aliases_and_fallbacks() {
    let grow = parse_custom_property_name("--grow").unwrap();
    let shrink = parse_custom_property_name("--shrink").unwrap();
    let basis = parse_custom_property_name("--basis").unwrap();
    assert_eq!(
        parse_flex_grow_declaration("var(--grow)"),
        Some(FlexGrowDeclaration::CustomProperty(grow))
    );
    assert_eq!(
        parse_flex_grow_declaration("var(--grow, 3)"),
        Some(FlexGrowDeclaration::CustomPropertyFallback(grow, 3))
    );
    assert_eq!(
        parse_flex_shrink_declaration("var(--shrink, 2)"),
        Some(FlexShrinkDeclaration::CustomPropertyFallback(shrink, 2))
    );
    assert_eq!(
        parse_flex_basis_declaration("var(--basis)"),
        Some(FlexBasisDeclaration::CustomProperty(basis))
    );
    assert_eq!(
        parse_flex_basis_declaration("var(--basis, auto)"),
        Some(FlexBasisDeclaration::CustomPropertyFallback(
            basis,
            FlexBasisValue::Auto
        ))
    );
    assert_eq!(
        parse_flex_basis_declaration("var(--basis, 12px)"),
        Some(FlexBasisDeclaration::CustomPropertyFallback(
            basis,
            FlexBasisValue::Length(12)
        ))
    );
    let declarations = parse_declarations(
        "flex-grow: var(--grow); flex-shrink: var(--shrink); flex-basis: var(--basis)",
    );
    assert_eq!(
        declarations.flex_grow,
        Some(FlexGrowDeclaration::CustomProperty(grow))
    );
    assert_eq!(
        declarations.flex_shrink,
        Some(FlexShrinkDeclaration::CustomProperty(shrink))
    );
    assert_eq!(
        declarations.flex_basis,
        Some(FlexBasisDeclaration::CustomProperty(basis))
    );
    assert_eq!(parse_flex_grow_declaration("var(--grow, 1.5)"), None);
    assert_eq!(
        parse_flex_basis_declaration("var(--basis, var(--other))"),
        None
    );
}

#[test]
fn flex_shorthand_parser_expands_bounded_forms_and_rejects_unsupported() {
    assert_eq!(
        parse_flex_shorthand("none"),
        Some((0, 0, FlexBasisValue::Auto))
    );
    assert_eq!(
        parse_flex_shorthand("AUTO"),
        Some((1, 1, FlexBasisValue::Auto))
    );
    assert_eq!(
        parse_flex_shorthand("2"),
        Some((2, 1, FlexBasisValue::Length(0)))
    );
    assert_eq!(
        parse_flex_shorthand("2 3"),
        Some((2, 3, FlexBasisValue::Length(0)))
    );
    assert_eq!(
        parse_flex_shorthand("2 auto"),
        Some((2, 1, FlexBasisValue::Auto))
    );
    assert_eq!(
        parse_flex_shorthand("2 12px"),
        Some((2, 1, FlexBasisValue::Length(12)))
    );
    assert_eq!(
        parse_flex_shorthand("2 3 auto"),
        Some((2, 3, FlexBasisValue::Auto))
    );
    assert_eq!(
        parse_flex_shorthand("2 3 12px"),
        Some((2, 3, FlexBasisValue::Length(12)))
    );
    assert_eq!(parse_flex_shorthand("initial"), None);
    assert_eq!(parse_flex_shorthand("inherit"), None);
    assert_eq!(parse_flex_shorthand("1.5"), None);
    assert_eq!(parse_flex_shorthand("-1"), None);
    assert_eq!(parse_flex_shorthand("1px"), None);
    assert_eq!(parse_flex_shorthand("1 2 3"), None);
    assert_eq!(parse_flex_shorthand("1 50%"), None);
    assert_eq!(parse_flex_shorthand("1 2 3%"), None);
    assert_eq!(parse_flex_shorthand("1 2 3 4px"), None);
    assert_eq!(
        parse_flex_shorthand_declaration("ReVeRt-LaYeR"),
        Some((
            FlexGrowDeclaration::RevertLayer,
            FlexShrinkDeclaration::RevertLayer,
            FlexBasisDeclaration::RevertLayer,
        ))
    );
    assert_eq!(parse_flex_shorthand_declaration("revert-layer 1"), None);
    assert_eq!(
        parse_flex_shorthand_declaration("InItIaL"),
        Some((
            FlexGrowDeclaration::Reset,
            FlexShrinkDeclaration::Reset,
            FlexBasisDeclaration::Reset,
        ))
    );
    assert_eq!(
        parse_flex_shorthand_declaration(" InHeRiT "),
        Some((
            FlexGrowDeclaration::Inherit,
            FlexShrinkDeclaration::Inherit,
            FlexBasisDeclaration::Inherit,
        ))
    );
    assert_eq!(parse_flex_shorthand_declaration("inherit 1 auto"), None);
    assert_eq!(parse_flex_shorthand_declaration("1 inherit auto"), None);
    assert_eq!(
        parse_declarations("flex: 2 3 12px; flex-grow: 4; flex-basis: auto"),
        NativeDeclarations {
            flex_grow: Some(FlexGrowDeclaration::Value(4)),
            flex_shrink: Some(FlexShrinkDeclaration::Value(3)),
            flex_basis: Some(FlexBasisDeclaration::Value(FlexBasisValue::Auto)),
            ..NativeDeclarations::default()
        }
    );
    assert_eq!(
        parse_declarations("flex: 2 3 12px; flex: revert-layer"),
        NativeDeclarations {
            flex_grow: Some(FlexGrowDeclaration::RevertLayer),
            flex_shrink: Some(FlexShrinkDeclaration::RevertLayer),
            flex_basis: Some(FlexBasisDeclaration::RevertLayer),
            ..NativeDeclarations::default()
        }
    );
    assert_eq!(
        parse_declarations("flex: revert-layer; flex-grow: 4"),
        NativeDeclarations {
            flex_grow: Some(FlexGrowDeclaration::Value(4)),
            flex_shrink: Some(FlexShrinkDeclaration::RevertLayer),
            flex_basis: Some(FlexBasisDeclaration::RevertLayer),
            ..NativeDeclarations::default()
        }
    );
}
#[test]
fn flex_shorthand_parser_accepts_custom_property_aliases_and_fallbacks() {
    let flex = parse_custom_property_name("--flex").unwrap();
    assert_eq!(
        parse_flex_shorthand_declaration("var(--flex)"),
        Some((
            FlexGrowDeclaration::CustomPropertyShorthand(flex),
            FlexShrinkDeclaration::CustomPropertyShorthand(flex),
            FlexBasisDeclaration::CustomPropertyShorthand(flex),
        ))
    );
    assert_eq!(
        parse_flex_shorthand_declaration("var(--flex, 2 3 12px)"),
        Some((
            FlexGrowDeclaration::CustomPropertyShorthandFallback(flex, 2),
            FlexShrinkDeclaration::CustomPropertyShorthandFallback(flex, 3),
            FlexBasisDeclaration::CustomPropertyShorthandFallback(flex, FlexBasisValue::Length(12)),
        ))
    );
    assert_eq!(
        parse_declarations("flex: var(--flex)"),
        NativeDeclarations {
            flex_grow: Some(FlexGrowDeclaration::CustomPropertyShorthand(flex)),
            flex_shrink: Some(FlexShrinkDeclaration::CustomPropertyShorthand(flex)),
            flex_basis: Some(FlexBasisDeclaration::CustomPropertyShorthand(flex)),
            ..NativeDeclarations::default()
        }
    );
    assert_eq!(
        parse_flex_shorthand_declaration("var(--flex, var(--other))"),
        None
    );
    assert_eq!(
        parse_flex_shorthand_declaration("var(--flex, 2 3 50%)"),
        None
    );
}

#[test]
fn flex_flow_parser_expands_bounded_direction_and_wrap_forms() {
    assert_eq!(
        parse_flex_flow("row"),
        Some((FlexDirectionValue::Row, FlexWrapValue::NoWrap))
    );
    assert_eq!(
        parse_flex_flow("row-reverse"),
        Some((FlexDirectionValue::RowReverse, FlexWrapValue::NoWrap))
    );
    assert_eq!(
        parse_flex_flow("column"),
        Some((FlexDirectionValue::Column, FlexWrapValue::NoWrap))
    );
    assert_eq!(
        parse_flex_flow("COLUMN-REVERSE"),
        Some((FlexDirectionValue::ColumnReverse, FlexWrapValue::NoWrap))
    );
    assert_eq!(
        parse_flex_flow("wrap"),
        Some((FlexDirectionValue::Row, FlexWrapValue::Wrap))
    );
    assert_eq!(
        parse_flex_flow("wrap-reverse"),
        Some((FlexDirectionValue::Row, FlexWrapValue::WrapReverse))
    );
    assert_eq!(
        parse_flex_flow("row-reverse wrap"),
        Some((FlexDirectionValue::RowReverse, FlexWrapValue::Wrap))
    );
    assert_eq!(
        parse_flex_flow("WRAP ROW"),
        Some((FlexDirectionValue::Row, FlexWrapValue::Wrap))
    );
    assert_eq!(
        parse_flex_flow("wrap-reverse row-reverse"),
        Some((FlexDirectionValue::RowReverse, FlexWrapValue::WrapReverse))
    );
    assert_eq!(parse_flex_flow(""), None);
    assert_eq!(
        parse_flex_flow("column wrap"),
        Some((FlexDirectionValue::Column, FlexWrapValue::Wrap))
    );
    assert_eq!(
        parse_flex_flow("column-reverse wrap-reverse"),
        Some((
            FlexDirectionValue::ColumnReverse,
            FlexWrapValue::WrapReverse
        ))
    );
    assert_eq!(parse_flex_flow("row column"), None);
    assert_eq!(parse_flex_flow("wrap wrap-reverse"), None);
    assert_eq!(parse_flex_flow("row row-reverse"), None);
    assert_eq!(parse_flex_flow("initial"), None);
    assert_eq!(
        parse_flex_flow_declaration("ReVeRt-LaYeR"),
        Some((
            FlexDirectionDeclaration::RevertLayer,
            FlexWrapDeclaration::RevertLayer,
        ))
    );
    assert_eq!(parse_flex_flow_declaration("revert-layer row"), None);
    assert_eq!(
        parse_declarations("flex-flow: row-reverse wrap; flex-direction: row; flex-wrap: nowrap"),
        NativeDeclarations {
            flex_direction: Some(FlexDirectionDeclaration::Value(FlexDirectionValue::Row)),
            flex_wrap: Some(FlexWrapDeclaration::Value(FlexWrapValue::NoWrap)),
            ..NativeDeclarations::default()
        }
    );
    assert_eq!(
        parse_declarations("flex-flow: revert-layer; flex-direction: column"),
        NativeDeclarations {
            flex_direction: Some(FlexDirectionDeclaration::Value(FlexDirectionValue::Column,)),
            flex_wrap: Some(FlexWrapDeclaration::RevertLayer),
            ..NativeDeclarations::default()
        }
    );
}
#[test]
fn flex_flow_parser_accepts_custom_property_aliases_and_fallbacks() {
    let direction = parse_custom_property_name("--direction").unwrap();
    let wrap = parse_custom_property_name("--wrap").unwrap();
    let flow = parse_custom_property_name("--flow").unwrap();
    assert_eq!(
        parse_flex_direction_declaration("var(--direction)"),
        Some(FlexDirectionDeclaration::CustomProperty(direction))
    );
    assert_eq!(
        parse_flex_direction_declaration("var(--direction, column)"),
        Some(FlexDirectionDeclaration::CustomPropertyFallback(
            direction,
            FlexDirectionValue::Column
        ))
    );
    assert_eq!(
        parse_flex_wrap_declaration("var(--wrap, wrap-reverse)"),
        Some(FlexWrapDeclaration::CustomPropertyFallback(
            wrap,
            FlexWrapValue::WrapReverse
        ))
    );
    assert_eq!(
        parse_flex_flow_declaration("var(--flow)"),
        Some((
            FlexDirectionDeclaration::CustomPropertyFlow(flow),
            FlexWrapDeclaration::CustomPropertyFlow(flow),
        ))
    );
    assert_eq!(
        parse_flex_flow_declaration("var(--flow, column wrap)"),
        Some((
            FlexDirectionDeclaration::CustomPropertyFlowFallback(flow, FlexDirectionValue::Column),
            FlexWrapDeclaration::CustomPropertyFlowFallback(flow, FlexWrapValue::Wrap),
        ))
    );
    let declarations = parse_declarations(
        "flex-flow: var(--flow); flex-direction: var(--direction); flex-wrap: var(--wrap)",
    );
    assert_eq!(
        declarations.flex_direction,
        Some(FlexDirectionDeclaration::CustomProperty(direction))
    );
    assert_eq!(
        declarations.flex_wrap,
        Some(FlexWrapDeclaration::CustomProperty(wrap))
    );
    assert_eq!(
        parse_flex_direction_declaration("var(--direction, row wrap)"),
        None
    );
    assert_eq!(
        parse_flex_flow_declaration("var(--flow, var(--other))"),
        None
    );
}

#[test]
fn opacity_parser_quantizes_bounded_numbers_and_percentages() {
    assert_eq!(parse_opacity("0"), Some(0));
    assert_eq!(parse_opacity("0.5"), Some(128));
    assert_eq!(parse_opacity(".5"), Some(128));
    assert_eq!(parse_opacity("1"), Some(255));
    assert_eq!(parse_opacity("50%"), Some(128));
    assert_eq!(parse_opacity("100%"), Some(255));
    assert_eq!(parse_opacity("1.001"), None);
    assert_eq!(parse_opacity("101%"), None);
    assert_eq!(parse_opacity("-0.1"), None);
    assert_eq!(parse_opacity("0.1234"), None);
    assert_eq!(parse_opacity("1e-1"), None);
    assert_eq!(
        parse_opacity_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_opacity_declaration("50%"),
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::Value(128)
        ))
    );
    for value in [
        "revert-layer 50%",
        "50% revert-layer",
        "inherit",
        "unset",
        "revert",
        "initial",
    ] {
        assert_eq!(parse_opacity_declaration(value), None, "value={value}");
    }
    assert_eq!(
        parse_declarations("opacity: 50%; opacity: 1.001").opacity,
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::Value(128)
        ))
    );
    assert_eq!(
        parse_declarations("opacity: 50% !important").opacity,
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::Value(128)
        ))
    );
}

#[test]
fn opacity_parser_accepts_custom_property_aliases_and_fallbacks() {
    let opacity = parse_custom_property_name("--opacity").unwrap();
    assert_eq!(
        parse_opacity_property("50%"),
        Some(OpacityDeclarationValue::Value(128))
    );
    assert_eq!(
        parse_opacity_declaration("var(--opacity)"),
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::CustomProperty(opacity)
        ))
    );
    assert_eq!(
        parse_opacity_declaration("var(--opacity, 50%)"),
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::CustomPropertyFallback(opacity, 128)
        ))
    );
    let numeric = parse_custom_property_name("--numeric").unwrap();
    assert_eq!(
        parse_opacity_declaration("var(--numeric, 0.25)"),
        Some(LocalCascadeDeclaration::Value(
            OpacityDeclarationValue::CustomPropertyFallback(numeric, 64)
        ))
    );
    assert_eq!(
        parse_opacity_declaration("var(--opacity, var(--other))"),
        None
    );
    assert_eq!(parse_opacity_declaration("inherit"), None);
}

#[test]
fn opacity_revert_layer_rolls_back_named_and_inline_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { opacity: 25%; } #repeat { opacity: 50%; } #fallback { opacity: revert-layer; } #inline { opacity: 75%; } } @layer theme { #named { opacity: 75%; } #repeat { opacity: revert-layer; } } @layer top { #repeat { opacity: revert-layer; } } #named { opacity: revert-layer; } #repeat { opacity: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let repeat = node("<div id='repeat'>Repeat</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let inline = node("<div id='inline' style='opacity:ReVeRt-LaYeR'>Inline</div>");

    let opacity = |node: &NativeNode| stylesheet.computed_for(node).opacity();
    assert_eq!(opacity(&named), 191);
    assert_eq!(opacity(&repeat), 128);
    assert_eq!(opacity(&fallback), u8::MAX);
    assert_eq!(opacity(&inline), 191);

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
        vec!["#target { opacity: ReVeRt-LaYeR; } #other { opacity: 50% revert-layer; }".into()],
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
                    && diagnostic.detail == "opacity"
            })
            .count(),
        1
    );
}

#[test]
fn opacity_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --opacity: 50%; --alias: var(--opacity); --cycle: var(--cycle); opacity: var(--opacity); }
            #child { opacity: var(--alias); }
            #fallback { opacity: var(--missing, 25%); }
            #invalid { --bad: 2; opacity: var(--bad, 75%); }
            #cycle { opacity: var(--cycle, 80%); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .opacity()
    };

    assert_eq!(style("parent"), 128);
    assert_eq!(style("child"), 128);
    assert_eq!(style("fallback"), 64);
    assert_eq!(style("invalid"), 191);
    assert_eq!(style("cycle"), 204);
}

#[test]
fn text_align_parser_accepts_bounded_physical_and_logical_values() {
    assert_eq!(parse_text_align("left"), Some(TextAlignValue::Left));
    assert_eq!(parse_text_align("CENTER"), Some(TextAlignValue::Center));
    assert_eq!(parse_text_align("right"), Some(TextAlignValue::Right));
    assert_eq!(parse_text_align("START"), Some(TextAlignValue::Start));
    assert_eq!(parse_text_align("end"), Some(TextAlignValue::End));
    assert_eq!(parse_text_align("JUSTIFY"), Some(TextAlignValue::Justify));
    assert_eq!(parse_text_align("match-parent"), None);
    assert_eq!(parse_text_align("start end"), None);
    let align_name = parse_custom_property_name("--align").unwrap();
    let fallback = TextAlignValue::Justify;
    assert_eq!(
        parse_text_align_property("var(--align)"),
        Some(TextAlignDeclarationValue::CustomProperty(align_name))
    );
    assert_eq!(
        parse_text_align_declaration("var(--align, justify)"),
        Some(InheritedTextDeclaration::Value(
            TextAlignDeclarationValue::CustomPropertyFallback(align_name, fallback)
        ))
    );
    assert_eq!(
        parse_text_align_property("var(--align, var(--other))"),
        None
    );
}

#[test]
fn text_align_last_parser_accepts_bounded_inherited_values() {
    assert_eq!(
        parse_text_align_last("auto"),
        Some(TextAlignLastValue::Auto)
    );
    assert_eq!(
        parse_text_align_last("LEFT"),
        Some(TextAlignLastValue::Left)
    );
    assert_eq!(
        parse_text_align_last("center"),
        Some(TextAlignLastValue::Center)
    );
    assert_eq!(
        parse_text_align_last("right"),
        Some(TextAlignLastValue::Right)
    );
    assert_eq!(
        parse_text_align_last("START"),
        Some(TextAlignLastValue::Start)
    );
    assert_eq!(parse_text_align_last("end"), Some(TextAlignLastValue::End));
    assert_eq!(
        parse_text_align_last("JUSTIFY"),
        Some(TextAlignLastValue::Justify)
    );
    assert_eq!(parse_text_align_last("match-parent"), None);
    assert_eq!(parse_text_align_last("start end"), None);
    let align_last_name = parse_custom_property_name("--align-last").unwrap();
    let fallback = TextAlignLastValue::Justify;
    assert_eq!(
        parse_text_align_last_property("var(--align-last)"),
        Some(TextAlignLastDeclarationValue::CustomProperty(
            align_last_name
        ))
    );
    assert_eq!(
        parse_text_align_last_declaration("var(--align-last, justify)"),
        Some(InheritedTextDeclaration::Value(
            TextAlignLastDeclarationValue::CustomPropertyFallback(align_last_name, fallback)
        ))
    );
    assert_eq!(
        parse_text_align_last_property("var(--align-last, var(--other))"),
        None
    );
}

#[test]
fn text_justify_parser_accepts_bounded_inherited_values() {
    assert_eq!(parse_text_justify("auto"), Some(TextJustifyValue::Auto));
    assert_eq!(parse_text_justify("NONE"), Some(TextJustifyValue::None));
    assert_eq!(
        parse_text_justify("inter-word"),
        Some(TextJustifyValue::InterWord)
    );
    assert_eq!(parse_text_justify("inter-character"), None);
    assert_eq!(parse_text_justify("distribute"), None);
    assert_eq!(parse_text_justify("auto none"), None);
    let justify_name = parse_custom_property_name("--justify").unwrap();
    let fallback = TextJustifyValue::InterWord;
    assert_eq!(
        parse_text_justify_property("var(--justify)"),
        Some(TextJustifyDeclarationValue::CustomProperty(justify_name))
    );
    assert_eq!(
        parse_text_justify_declaration("var(--justify, inter-word)"),
        Some(InheritedTextDeclaration::Value(
            TextJustifyDeclarationValue::CustomPropertyFallback(justify_name, fallback)
        ))
    );
    assert_eq!(
        parse_text_justify_property("var(--justify, var(--other))"),
        None
    );
}

#[test]
fn inherited_alignment_declaration_parser_accepts_standalone_css_wide_keywords() {
    assert_eq!(
        parse_text_align_declaration("ReVeRt-LaYeR"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_align_last_declaration(" REVERT-LAYER "),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_justify_declaration("revert-LAYER"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_align_declaration("INHERIT"),
        Some(InheritedTextDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_align_last_declaration("initial"),
        Some(InheritedTextDeclaration::Initial)
    );
    assert_eq!(
        parse_text_justify_declaration("UnSeT"),
        Some(InheritedTextDeclaration::Unset)
    );
    assert_eq!(
        parse_text_align_declaration("ReVeRt"),
        Some(InheritedTextDeclaration::Revert)
    );
    assert_eq!(
        parse_text_align_last_declaration("inherit"),
        Some(InheritedTextDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_justify_declaration("INITIAL"),
        Some(InheritedTextDeclaration::Initial)
    );
    assert_eq!(
        parse_text_align_declaration("unset"),
        Some(InheritedTextDeclaration::Unset)
    );
    assert_eq!(
        parse_text_align_last_declaration("REVERT"),
        Some(InheritedTextDeclaration::Revert)
    );
    assert_eq!(parse_text_align_declaration("center revert-layer"), None);
    assert_eq!(parse_text_justify_declaration("none revert-layer"), None);
}

#[test]
fn white_space_declaration_parser_accepts_bounded_modes_and_css_wide_keywords() {
    assert_eq!(
        parse_white_space_declaration("ReVeRt-LaYeR"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_white_space_declaration("PRE-WRAP"),
        Some(InheritedTextDeclaration::Value(
            WhiteSpaceDeclarationValue::Value(WhiteSpaceValue::PreWrap)
        ))
    );
    assert_eq!(
        parse_white_space_declaration("pre-line"),
        Some(InheritedTextDeclaration::Value(
            WhiteSpaceDeclarationValue::Value(WhiteSpaceValue::PreLine)
        ))
    );
    assert_eq!(
        parse_white_space_declaration("InHeRiT"),
        Some(InheritedTextDeclaration::Inherit)
    );
    assert_eq!(
        parse_white_space_declaration("INITIAL"),
        Some(InheritedTextDeclaration::Initial)
    );
    assert_eq!(
        parse_white_space_declaration("unset"),
        Some(InheritedTextDeclaration::Unset)
    );
    assert_eq!(
        parse_white_space_declaration("ReVeRt"),
        Some(InheritedTextDeclaration::Revert)
    );
    assert_eq!(parse_white_space_declaration("inherit pre"), None);
    assert_eq!(parse_white_space_declaration("pre wrap"), None);
    assert_eq!(parse_white_space_declaration("revert-layer pre"), None);
    let white_space_name = parse_custom_property_name("--white-space").unwrap();
    let fallback = WhiteSpaceValue::NoWrap;
    assert_eq!(
        parse_white_space_property("var(--white-space)"),
        Some(WhiteSpaceDeclarationValue::CustomProperty(white_space_name))
    );
    assert_eq!(
        parse_white_space_declaration("var(--white-space, nowrap)"),
        Some(InheritedTextDeclaration::Value(
            WhiteSpaceDeclarationValue::CustomPropertyFallback(white_space_name, fallback)
        ))
    );
    assert_eq!(
        parse_white_space_property("var(--white-space, var(--other))"),
        None
    );
}

#[test]
fn direction_parser_accepts_only_bounded_inherited_values() {
    assert_eq!(parse_direction("ltr"), Some(DirectionValue::Ltr));
    assert_eq!(parse_direction("RTL"), Some(DirectionValue::Rtl));
    assert_eq!(parse_direction("inherit"), None);
    assert_eq!(parse_direction("vertical-rl"), None);
    assert_eq!(parse_direction("rtl ltr"), None);
    let direction_name = parse_custom_property_name("--direction").unwrap();
    let fallback = DirectionValue::Rtl;
    assert_eq!(
        parse_direction_property("var(--direction)"),
        Some(DirectionDeclarationValue::CustomProperty(direction_name))
    );
    assert_eq!(
        parse_direction_declaration("var(--direction, rtl)"),
        Some(InheritedTextDeclaration::Value(
            DirectionDeclarationValue::CustomPropertyFallback(direction_name, fallback)
        ))
    );
    assert_eq!(
        parse_direction_property("var(--direction, var(--other))"),
        None
    );
}

#[test]
fn direction_declaration_parser_accepts_standalone_css_wide_keywords() {
    assert_eq!(
        parse_direction_declaration("ReVeRt-LaYeR"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_direction_declaration("RTL"),
        Some(InheritedTextDeclaration::Value(
            DirectionDeclarationValue::Value(DirectionValue::Rtl)
        ))
    );
    assert_eq!(
        parse_direction_declaration(" REVERT-LAYER "),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_direction_declaration("INHERIT"),
        Some(InheritedTextDeclaration::Inherit)
    );
    assert_eq!(
        parse_direction_declaration("initial"),
        Some(InheritedTextDeclaration::Initial)
    );
    assert_eq!(
        parse_direction_declaration("UnSeT"),
        Some(InheritedTextDeclaration::Unset)
    );
    assert_eq!(
        parse_direction_declaration("ReVeRt"),
        Some(InheritedTextDeclaration::Revert)
    );
    assert_eq!(parse_direction_declaration("vertical-rl"), None);
    assert_eq!(parse_direction_declaration("revert-layer rtl"), None);
}

#[test]
fn justify_content_parser_accepts_only_bounded_row_values() {
    assert_eq!(
        parse_justify_content("flex-start"),
        Some(JustifyContentValue::FlexStart)
    );
    assert_eq!(
        parse_justify_content("CENTER"),
        Some(JustifyContentValue::Center)
    );
    assert_eq!(
        parse_justify_content("flex-end"),
        Some(JustifyContentValue::FlexEnd)
    );
    assert_eq!(
        parse_justify_content("space-between"),
        Some(JustifyContentValue::SpaceBetween)
    );
    assert_eq!(
        parse_justify_content("SPACE-AROUND"),
        Some(JustifyContentValue::SpaceAround)
    );
    assert_eq!(
        parse_justify_content("SPACE-EVENLY"),
        Some(JustifyContentValue::SpaceEvenly)
    );
    assert_eq!(
        parse_justify_content("NORMAL"),
        Some(JustifyContentValue::Normal)
    );
    assert_eq!(
        parse_justify_content("STRETCH"),
        Some(JustifyContentValue::Stretch)
    );
    assert_eq!(parse_justify_content("safe center"), None);
    assert_eq!(parse_justify_content("start"), None);
}

#[test]
fn place_content_parser_expands_bounded_shared_and_axis_values() {
    assert_eq!(
        parse_place_content("center"),
        Some((AlignContentValue::Center, JustifyContentValue::Center))
    );
    assert_eq!(
        parse_place_content("SPACE-BETWEEN"),
        Some((
            AlignContentValue::SpaceBetween,
            JustifyContentValue::SpaceBetween
        ))
    );
    assert_eq!(
        parse_place_content("space-around flex-end"),
        Some((AlignContentValue::SpaceAround, JustifyContentValue::FlexEnd))
    );
    assert_eq!(
        parse_place_content("space-around"),
        Some((
            AlignContentValue::SpaceAround,
            JustifyContentValue::SpaceAround
        ))
    );
    assert_eq!(
        parse_place_content("space-evenly"),
        Some((
            AlignContentValue::SpaceEvenly,
            JustifyContentValue::SpaceEvenly
        ))
    );
    assert_eq!(
        parse_place_content("normal"),
        Some((AlignContentValue::Normal, JustifyContentValue::Normal))
    );
    assert_eq!(
        parse_place_content("stretch"),
        Some((AlignContentValue::Stretch, JustifyContentValue::Stretch))
    );
    assert_eq!(
        parse_place_content("normal center"),
        Some((AlignContentValue::Normal, JustifyContentValue::Center))
    );
    assert_eq!(
        parse_place_content("stretch flex-start"),
        Some((AlignContentValue::Stretch, JustifyContentValue::FlexStart))
    );
    assert_eq!(
        parse_place_content("center stretch"),
        Some((AlignContentValue::Center, JustifyContentValue::Stretch))
    );
    assert_eq!(parse_place_content("center center center"), None);
    assert_eq!(parse_place_content("start center"), None);
    assert_eq!(
        parse_place_content_declaration("InItIaL"),
        Some((
            AlignContentDeclaration::Reset,
            JustifyContentDeclaration::Reset,
        ))
    );
    assert_eq!(
        parse_place_content_declaration("UNSET"),
        Some((
            AlignContentDeclaration::Reset,
            JustifyContentDeclaration::Reset,
        ))
    );
    assert_eq!(
        parse_place_content_declaration(" ReVeRt "),
        Some((
            AlignContentDeclaration::Reset,
            JustifyContentDeclaration::Reset,
        ))
    );
    assert_eq!(
        parse_place_content_declaration("ReVeRt-LaYeR"),
        Some((
            AlignContentDeclaration::RevertLayer,
            JustifyContentDeclaration::RevertLayer,
        ))
    );
    assert_eq!(
        parse_place_content_declaration(" InHeRiT "),
        Some((
            AlignContentDeclaration::Inherit,
            JustifyContentDeclaration::Inherit,
        ))
    );
    assert_eq!(parse_place_content_declaration("revert-layer center"), None);
    assert_eq!(parse_place_content_declaration("initial center"), None);
    assert_eq!(parse_place_content_declaration("inherit center"), None);
    assert_eq!(
        parse_declarations("place-content: revert-layer; justify-content: flex-end"),
        NativeDeclarations {
            align_content: Some(AlignContentDeclaration::RevertLayer),
            justify_content: Some(JustifyContentDeclaration::Value(
                JustifyContentValue::FlexEnd,
            )),
            ..NativeDeclarations::default()
        }
    );
}
#[test]
fn place_content_parser_accepts_custom_property_aliases_and_fallbacks() {
    let place = parse_custom_property_name("--place").unwrap();
    assert_eq!(
        parse_place_content_declaration("var(--place)"),
        Some((
            AlignContentDeclaration::CustomPropertyPlaceContent(place),
            JustifyContentDeclaration::CustomPropertyPlaceContent(place),
        ))
    );
    assert_eq!(
        parse_place_content_declaration("var(--place, space-around flex-end)"),
        Some((
            AlignContentDeclaration::CustomPropertyPlaceContentFallback(
                place,
                AlignContentValue::SpaceAround
            ),
            JustifyContentDeclaration::CustomPropertyPlaceContentFallback(
                place,
                JustifyContentValue::FlexEnd
            ),
        ))
    );
    assert_eq!(
        parse_declarations("place-content: var(--place)"),
        NativeDeclarations {
            align_content: Some(AlignContentDeclaration::CustomPropertyPlaceContent(place)),
            justify_content: Some(JustifyContentDeclaration::CustomPropertyPlaceContent(place)),
            ..NativeDeclarations::default()
        }
    );
    assert_eq!(
        parse_place_content_declaration("var(--place, var(--other))"),
        None
    );
    assert_eq!(
        parse_place_content_declaration("var(--place, start center)"),
        None
    );
}

#[test]
fn align_items_parser_accepts_only_bounded_cross_axis_values() {
    assert_eq!(
        parse_align_items("flex-start"),
        Some(AlignItemsValue::FlexStart)
    );
    assert_eq!(parse_align_items("CENTER"), Some(AlignItemsValue::Center));
    assert_eq!(
        parse_align_items("flex-end"),
        Some(AlignItemsValue::FlexEnd)
    );
    assert_eq!(parse_align_items("stretch"), Some(AlignItemsValue::Stretch));
    assert_eq!(parse_align_items("NORMAL"), Some(AlignItemsValue::Normal));
    assert_eq!(parse_align_items("baseline"), None);
    assert_eq!(parse_align_items("start"), None);
    assert_eq!(parse_align_items("end"), None);
}

#[test]
fn align_self_parser_accepts_only_bounded_item_values() {
    assert_eq!(parse_align_self("auto"), Some(AlignSelfValue::Auto));
    assert_eq!(
        parse_align_self("FLEX-START"),
        Some(AlignSelfValue::FlexStart)
    );
    assert_eq!(parse_align_self("center"), Some(AlignSelfValue::Center));
    assert_eq!(parse_align_self("flex-end"), Some(AlignSelfValue::FlexEnd));
    assert_eq!(parse_align_self("stretch"), Some(AlignSelfValue::Stretch));
    assert_eq!(parse_align_self("NORMAL"), Some(AlignSelfValue::Normal));
    assert_eq!(parse_align_self("baseline"), None);
    assert_eq!(parse_align_self("safe center"), None);
    assert_eq!(parse_align_self("flex-start center"), None);
}

#[test]
fn align_content_parser_accepts_only_bounded_line_distribution_values() {
    assert_eq!(
        parse_align_content("flex-start"),
        Some(AlignContentValue::FlexStart)
    );
    assert_eq!(
        parse_align_content("CENTER"),
        Some(AlignContentValue::Center)
    );
    assert_eq!(
        parse_align_content("flex-end"),
        Some(AlignContentValue::FlexEnd)
    );
    assert_eq!(
        parse_align_content("space-between"),
        Some(AlignContentValue::SpaceBetween)
    );
    assert_eq!(
        parse_align_content("space-around"),
        Some(AlignContentValue::SpaceAround)
    );
    assert_eq!(
        parse_align_content("SPACE-EVENLY"),
        Some(AlignContentValue::SpaceEvenly)
    );
    assert_eq!(
        parse_align_content("STRETCH"),
        Some(AlignContentValue::Stretch)
    );
    assert_eq!(
        parse_align_content("NORMAL"),
        Some(AlignContentValue::Normal)
    );
    assert_eq!(parse_align_content("start"), None);
    assert_eq!(parse_align_content("safe center"), None);
}
#[test]
fn flex_alignment_parser_accepts_custom_property_aliases_and_fallbacks() {
    let justify = parse_custom_property_name("--justify").unwrap();
    let items = parse_custom_property_name("--items").unwrap();
    let self_name = parse_custom_property_name("--self").unwrap();
    let content = parse_custom_property_name("--content").unwrap();
    assert_eq!(
        parse_justify_content_declaration("var(--justify)"),
        Some(JustifyContentDeclaration::CustomProperty(justify))
    );
    assert_eq!(
        parse_justify_content_declaration("var(--justify, space-between)"),
        Some(JustifyContentDeclaration::CustomPropertyFallback(
            justify,
            JustifyContentValue::SpaceBetween
        ))
    );
    assert_eq!(
        parse_align_items_declaration("var(--items, center)"),
        Some(AlignItemsDeclaration::CustomPropertyFallback(
            items,
            AlignItemsValue::Center
        ))
    );
    assert_eq!(
        parse_align_self_declaration("var(--self)"),
        Some(AlignSelfDeclaration::CustomProperty(self_name))
    );
    assert_eq!(
        parse_align_content_declaration("var(--content, space-around)"),
        Some(AlignContentDeclaration::CustomPropertyFallback(
            content,
            AlignContentValue::SpaceAround
        ))
    );
    let declarations = parse_declarations(
        "justify-content: var(--justify); align-items: var(--items); align-self: var(--self); align-content: var(--content)",
    );
    assert_eq!(
        declarations.justify_content,
        Some(JustifyContentDeclaration::CustomProperty(justify))
    );
    assert_eq!(
        declarations.align_items,
        Some(AlignItemsDeclaration::CustomProperty(items))
    );
    assert_eq!(
        declarations.align_self,
        Some(AlignSelfDeclaration::CustomProperty(self_name))
    );
    assert_eq!(
        declarations.align_content,
        Some(AlignContentDeclaration::CustomProperty(content))
    );
    assert_eq!(
        parse_justify_content_declaration("var(--justify, safe center)"),
        None
    );
    assert_eq!(
        parse_align_items_declaration("var(--items, baseline)"),
        None
    );
    assert_eq!(
        parse_align_self_declaration("var(--self, var(--other))"),
        None
    );
}

#[test]
fn flex_direction_parser_accepts_bounded_row_and_column_values() {
    assert_eq!(parse_flex_direction("row"), Some(FlexDirectionValue::Row));
    assert_eq!(
        parse_flex_direction("ROW-REVERSE"),
        Some(FlexDirectionValue::RowReverse)
    );
    assert_eq!(
        parse_flex_direction("COLUMN"),
        Some(FlexDirectionValue::Column)
    );
    assert_eq!(
        parse_flex_direction("column-reverse"),
        Some(FlexDirectionValue::ColumnReverse)
    );
    assert_eq!(parse_flex_direction("start"), None);
    assert_eq!(parse_flex_direction("row reverse"), None);
}

#[test]
fn flex_direction_declaration_parser_accepts_css_wide_resets_and_revert_layer() {
    assert_eq!(
        parse_flex_direction_declaration("ReVeRt-LaYeR"),
        Some(FlexDirectionDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_flex_direction_declaration("COLUMN-REVERSE"),
        Some(FlexDirectionDeclaration::Value(
            FlexDirectionValue::ColumnReverse
        ))
    );
    assert_eq!(
        parse_flex_direction_declaration(" REVERT-LAYER "),
        Some(FlexDirectionDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_flex_direction_declaration("InItIaL"),
        Some(FlexDirectionDeclaration::Reset)
    );
    assert_eq!(
        parse_flex_direction_declaration("UNSET"),
        Some(FlexDirectionDeclaration::Reset)
    );
    assert_eq!(
        parse_flex_direction_declaration(" ReVeRt "),
        Some(FlexDirectionDeclaration::Reset)
    );
    assert_eq!(
        parse_flex_direction_declaration(" InHeRiT "),
        Some(FlexDirectionDeclaration::Inherit)
    );
    assert_eq!(parse_flex_direction_declaration("inherit column"), None);
    assert_eq!(parse_flex_direction_declaration("revert-layer row"), None);
    assert_eq!(parse_flex_direction_declaration("initial row"), None);
    assert_eq!(parse_flex_direction_declaration("row reverse"), None);
}

#[test]
fn flex_wrap_parser_accepts_only_bounded_line_values() {
    assert_eq!(parse_flex_wrap("nowrap"), Some(FlexWrapValue::NoWrap));
    assert_eq!(parse_flex_wrap("WRAP"), Some(FlexWrapValue::Wrap));
    assert_eq!(
        parse_flex_wrap("WRAP-REVERSE"),
        Some(FlexWrapValue::WrapReverse)
    );
    assert_eq!(parse_flex_wrap("row"), None);
    assert_eq!(parse_flex_wrap("normal"), None);
    assert_eq!(
        parse_flex_wrap_declaration(" InHeRiT "),
        Some(FlexWrapDeclaration::Inherit)
    );
    assert_eq!(parse_flex_wrap_declaration("inherit wrap"), None);
}

#[test]
fn flex_flow_declaration_parser_accepts_css_wide_resets_and_revert_layer() {
    for reset in ["initial", "UNSET", "ReVeRt"] {
        assert_eq!(
            parse_flex_wrap_declaration(reset),
            Some(FlexWrapDeclaration::Reset),
            "{reset}"
        );
        assert_eq!(
            parse_flex_flow_declaration(reset),
            Some((FlexDirectionDeclaration::Reset, FlexWrapDeclaration::Reset,)),
            "{reset}"
        );
    }
    assert_eq!(parse_flex_flow("initial"), None);
    assert_eq!(
        parse_flex_flow_declaration("ReVeRt-LaYeR"),
        Some((
            FlexDirectionDeclaration::RevertLayer,
            FlexWrapDeclaration::RevertLayer,
        ))
    );
    assert_eq!(parse_flex_flow_declaration("initial row"), None);
    assert_eq!(
        parse_flex_flow_declaration(" InHeRiT "),
        Some((
            FlexDirectionDeclaration::Inherit,
            FlexWrapDeclaration::Inherit,
        ))
    );
    assert_eq!(parse_flex_flow_declaration("inherit row"), None);
    assert_eq!(parse_flex_flow_declaration("row inherit"), None);
    assert_eq!(
        parse_declarations("flex-flow: UnSeT"),
        NativeDeclarations {
            flex_direction: Some(FlexDirectionDeclaration::Reset),
            flex_wrap: Some(FlexWrapDeclaration::Reset),
            ..NativeDeclarations::default()
        }
    );
}

#[test]
fn flexbox_declaration_parsers_accept_only_standalone_case_insensitive_revert_layer() {
    assert_eq!(
        parse_justify_content_declaration("ReVeRt-LaYeR"),
        Some(JustifyContentDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_justify_content_declaration("InItIaL"),
        Some(JustifyContentDeclaration::Reset)
    );
    assert_eq!(
        parse_justify_content_declaration("UNSET"),
        Some(JustifyContentDeclaration::Reset)
    );
    assert_eq!(
        parse_justify_content_declaration(" ReVeRt "),
        Some(JustifyContentDeclaration::Reset)
    );
    assert_eq!(
        parse_align_items_declaration(" REVERT-LAYER "),
        Some(AlignItemsDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_align_items_declaration("InItIaL"),
        Some(AlignItemsDeclaration::Reset)
    );
    assert_eq!(
        parse_align_items_declaration("UNSET"),
        Some(AlignItemsDeclaration::Reset)
    );
    assert_eq!(
        parse_align_items_declaration(" ReVeRt "),
        Some(AlignItemsDeclaration::Reset)
    );
    assert_eq!(
        parse_align_self_declaration("revert-layer"),
        Some(AlignSelfDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_align_self_declaration("InItIaL"),
        Some(AlignSelfDeclaration::Reset)
    );
    assert_eq!(
        parse_align_self_declaration("UNSET"),
        Some(AlignSelfDeclaration::Reset)
    );
    assert_eq!(
        parse_align_self_declaration(" ReVeRt "),
        Some(AlignSelfDeclaration::Reset)
    );
    assert_eq!(
        parse_align_content_declaration("ReVeRt-LaYeR"),
        Some(AlignContentDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_align_content_declaration("InItIaL"),
        Some(AlignContentDeclaration::Reset)
    );
    assert_eq!(
        parse_align_content_declaration("UNSET"),
        Some(AlignContentDeclaration::Reset)
    );
    assert_eq!(
        parse_align_content_declaration(" ReVeRt "),
        Some(AlignContentDeclaration::Reset)
    );
    assert_eq!(
        parse_flex_wrap_declaration("REVERT-LAYER"),
        Some(FlexWrapDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_flex_wrap_declaration("INITIAL"),
        Some(FlexWrapDeclaration::Reset)
    );
    assert_eq!(parse_justify_content_declaration("safe center"), None);
    assert_eq!(
        parse_justify_content_declaration("InHeRiT"),
        Some(JustifyContentDeclaration::Inherit)
    );
    assert_eq!(
        parse_justify_content_declaration("revert-layer center"),
        None
    );
    assert_eq!(
        parse_align_items_declaration("InHeRiT"),
        Some(AlignItemsDeclaration::Inherit)
    );
    assert_eq!(parse_align_items_declaration("inherit center"), None);
    assert_eq!(parse_align_items_declaration("revert-layer center"), None);
    assert_eq!(parse_align_self_declaration("center flex-end"), None);
    assert_eq!(
        parse_align_self_declaration("InHeRiT"),
        Some(AlignSelfDeclaration::Inherit)
    );
    assert_eq!(parse_align_self_declaration("inherit center"), None);
    assert_eq!(parse_align_self_declaration("revert-layer center"), None);
    assert_eq!(
        parse_align_content_declaration("InHeRiT"),
        Some(AlignContentDeclaration::Inherit)
    );
    assert_eq!(parse_align_content_declaration("revert-layer center"), None);
    assert_eq!(parse_align_content_declaration("initial center"), None);
    assert_eq!(parse_align_content_declaration("inherit center"), None);
    assert_eq!(parse_flex_wrap_declaration("wrap reverse"), None);
}

#[test]
fn flex_item_order_parser_accepts_only_bounded_signed_integers() {
    assert_eq!(
        parse_flex_item_order("-1024"),
        Some(NativeOrderValue(-1024))
    );
    assert_eq!(parse_flex_item_order("+1024"), Some(NativeOrderValue(1024)));
    assert_eq!(
        parse_flex_item_order("  -12  "),
        Some(NativeOrderValue(-12))
    );
    assert_eq!(parse_flex_item_order("1025"), None);
    assert_eq!(parse_flex_item_order("-1025"), None);
    assert_eq!(parse_flex_item_order("1.0"), None);
    assert_eq!(parse_flex_item_order("1e1"), None);
    assert_eq!(parse_flex_item_order("--1"), None);
    assert_eq!(parse_flex_item_order("+ 1"), None);
    assert_eq!(parse_flex_item_order("normal"), None);
    assert_eq!(parse_flex_item_order("1px"), None);
}
#[test]
fn flex_item_order_parser_accepts_custom_property_aliases_and_fallbacks() {
    let order = parse_custom_property_name("--order").unwrap();
    assert_eq!(
        parse_flex_item_order_declaration("var(--order)"),
        Some(FlexItemOrderDeclaration::CustomProperty(order))
    );
    assert_eq!(
        parse_flex_item_order_declaration("var(--order, -12)"),
        Some(FlexItemOrderDeclaration::CustomPropertyFallback(
            order,
            NativeOrderValue(-12)
        ))
    );
    assert_eq!(
        parse_declarations("order: var(--order)").order,
        Some(FlexItemOrderDeclaration::CustomProperty(order))
    );
    assert_eq!(
        parse_flex_item_order_declaration("var(--order, var(--other))"),
        None
    );
    assert_eq!(
        parse_flex_item_order_declaration("var(--order, 1025)"),
        None
    );
}

#[test]
fn flex_item_declaration_parsers_accept_css_wide_resets_and_revert_layer() {
    assert_eq!(
        parse_flex_item_order_declaration("ReVeRt-LaYeR"),
        Some(FlexItemOrderDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_flex_item_order_declaration("-12"),
        Some(FlexItemOrderDeclaration::Value(NativeOrderValue(-12)))
    );
    assert_eq!(
        parse_flex_grow_declaration(" REVERT-LAYER "),
        Some(FlexGrowDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_flex_grow_declaration("2"),
        Some(FlexGrowDeclaration::Value(2))
    );
    assert_eq!(
        parse_flex_shrink_declaration("revert-LAYER"),
        Some(FlexShrinkDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_flex_shrink_declaration("3"),
        Some(FlexShrinkDeclaration::Value(3))
    );
    assert_eq!(
        parse_flex_basis_declaration("REVERT-LAYER"),
        Some(FlexBasisDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_flex_basis_declaration("12px"),
        Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(12)))
    );

    for reset in ["initial", "UNSET", "ReVeRt"] {
        assert_eq!(
            parse_flex_item_order_declaration(reset),
            Some(FlexItemOrderDeclaration::Reset),
            "{reset}"
        );
    }
    assert_eq!(
        parse_flex_item_order_declaration(" InHeRiT "),
        Some(FlexItemOrderDeclaration::Inherit)
    );
    assert_eq!(parse_flex_item_order_declaration("inherit 1"), None);
    assert_eq!(
        parse_flex_grow_declaration(" InHeRiT "),
        Some(FlexGrowDeclaration::Inherit)
    );
    assert_eq!(parse_flex_grow_declaration("inherit 1"), None);
    assert_eq!(
        parse_flex_shrink_declaration(" InHeRiT "),
        Some(FlexShrinkDeclaration::Inherit)
    );
    assert_eq!(parse_flex_shrink_declaration("inherit 1"), None);
    assert_eq!(
        parse_flex_basis_declaration(" InHeRiT "),
        Some(FlexBasisDeclaration::Inherit)
    );
    assert_eq!(parse_flex_basis_declaration("inherit 1px"), None);
    assert_eq!(
        parse_flex_grow_declaration("initial"),
        Some(FlexGrowDeclaration::Reset)
    );
    assert_eq!(
        parse_flex_shrink_declaration("ReVeRt"),
        Some(FlexShrinkDeclaration::Reset)
    );
    assert_eq!(
        parse_flex_basis_declaration("UNSET"),
        Some(FlexBasisDeclaration::Reset)
    );
    assert_eq!(parse_flex_item_order_declaration("revert-layer -1"), None);
    assert_eq!(parse_flex_grow_declaration("1 revert-layer"), None);
    assert_eq!(parse_flex_shrink_declaration("1.5"), None);
    assert_eq!(parse_flex_basis_declaration("50%"), None);
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_flex_item_order_and_sizing_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { order: -1; flex-grow: 1; flex-shrink: 2; flex-basis: 4px; } #rollback { order: -2; flex-grow: 2; flex-shrink: 3; flex-basis: 6px; } #repeated { order: -3; flex-grow: 3; flex-shrink: 4; flex-basis: 8px; } #unlayered { order: -4; flex-grow: 4; flex-shrink: 5; flex-basis: 10px; } #inline { order: -5; flex-grow: 5; flex-shrink: 6; flex-basis: 12px; } #fallback { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } } @layer theme { .named { order: 1; flex-grow: 11; flex-shrink: 12; flex-basis: 14px; } #rollback { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } #repeated { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } #unlayered { order: 2; flex-grow: 13; flex-shrink: 14; flex-basis: 16px; } #inline { order: 3; flex-grow: 15; flex-shrink: 16; flex-basis: 18px; } } @layer top { #repeated { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } } #named { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; } #unlayered { order: revert-layer; flex-grow: revert-layer; flex-shrink: revert-layer; flex-basis: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let unlayered = node("<div id='unlayered'>Unlayered</div>");
    let inline = node(
        "<div id='inline' style='order:revert-layer;flex-grow:revert-layer;flex-shrink:revert-layer;flex-basis:revert-layer'>Inline</div>",
    );
    let fallback = node("<div id='fallback'>Fallback</div>");
    let parent = node(
        "<div id='parent' style='order:7;flex-grow:7;flex-shrink:7;flex-basis:20px'><span id='child'>Child</span></div>",
    );
    let child = node("<span id='child'>Child</span>");

    let assert_values =
        |element: &NativeNode, order: i32, grow: u32, shrink: u32, basis: FlexBasisValue| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.flex_item_order(), NativeOrderValue(order));
            assert_eq!(style.flex_grow(), grow);
            assert_eq!(style.flex_shrink(), shrink);
            assert_eq!(style.flex_basis(), basis);
        };

    assert_values(&named, 1, 11, 12, FlexBasisValue::Length(14));
    assert_values(&rollback, -2, 2, 3, FlexBasisValue::Length(6));
    assert_values(&repeated, -3, 3, 4, FlexBasisValue::Length(8));
    assert_values(&unlayered, 2, 13, 14, FlexBasisValue::Length(16));
    assert_values(&inline, 3, 15, 16, FlexBasisValue::Length(18));
    assert_values(&fallback, 0, 0, 1, FlexBasisValue::Auto);

    let parent_style = stylesheet.computed_for(&parent);
    assert_eq!(parent_style.flex_item_order(), NativeOrderValue(7));
    assert_eq!(parent_style.flex_grow(), 7);
    assert_eq!(parent_style.flex_shrink(), 7);
    assert_eq!(parent_style.flex_basis(), FlexBasisValue::Length(20));
    assert_values(&child, 0, 0, 1, FlexBasisValue::Auto);
}

#[test]
fn stylesheet_flex_order_css_wide_resets_and_root_inherit_use_zero() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { order: -3; } } @layer theme { #layer { order: revert-layer; } } #reset { order: -2; } #reset { order: INITIAL; } #unset { order: -4; } #unset { order: UnSeT; } #revert { order: -5; } #revert { order: ReVeRt; } #inherit { order: -6; } #inherit { order: inherit; } #important { order: -7 !important; } #important { order: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let reset = node("<div id='reset'>Reset</div>");
    let unset = node("<div id='unset'>Unset</div>");
    let revert = node("<div id='revert'>Revert</div>");
    let inherit = node("<div id='inherit'>Inherit</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_order = |element: &NativeNode, order: i32| {
        assert_eq!(
            stylesheet.computed_for(element).flex_item_order(),
            NativeOrderValue(order)
        );
    };

    assert_order(&layer, -3);
    assert_order(&reset, 0);
    assert_order(&unset, 0);
    assert_order(&revert, 0);
    assert_order(&inherit, 0);
    assert_order(&important, -7);
}

#[test]
fn stylesheet_justify_content_css_wide_resets_use_flex_start_and_preserve_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { justify-content: center; } } @layer theme { #layer { justify-content: revert-layer; } } #reset { justify-content: flex-end; } #reset { justify-content: INITIAL; } #unset { justify-content: center; } #unset { justify-content: UnSeT; } #revert { justify-content: space-between; } #revert { justify-content: ReVeRt; } #invalid { justify-content: center; } #invalid { justify-content: inherit center; } #important { justify-content: flex-end !important; } #important { justify-content: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let reset = node("<div id='reset'>Reset</div>");
    let unset = node("<div id='unset'>Unset</div>");
    let revert = node("<div id='revert'>Revert</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_justify = |element: &NativeNode, value: JustifyContentValue| {
        assert_eq!(stylesheet.computed_for(element).justify_content(), value);
    };

    assert_justify(&layer, JustifyContentValue::Center);
    assert_justify(&reset, JustifyContentValue::FlexStart);
    assert_justify(&unset, JustifyContentValue::FlexStart);
    assert_justify(&revert, JustifyContentValue::FlexStart);
    assert_justify(&invalid, JustifyContentValue::Center);
    assert_justify(&important, JustifyContentValue::FlexEnd);
}

#[test]
fn stylesheet_justify_content_explicit_inherit_uses_parent_without_implicit_inheritance() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { justify-content: inherit; } #default { color: red; } #invalid { justify-content: flex-end; justify-content: inherit center; } #terminal { justify-content: inherit; justify-content: space-between; } #important { justify-content: flex-end !important; } #important { justify-content: inherit; }"
                .into(),
        ])
        .unwrap();
    let inherited = JustifyContentValue::SpaceAround;
    let computed = |element: &NativeNode| {
        stylesheet
            .computed_for_with_matcher(
                element,
                NativeInheritedStyle {
                    justify_content: inherited,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(element),
            )
            .justify_content()
    };

    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")),
        inherited
    );
    assert_eq!(
        computed(&node("<div id='default'>Default</div>")),
        JustifyContentValue::FlexStart
    );
    assert_eq!(
        computed(&node("<div id='invalid'>Invalid</div>")),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(
        computed(&node("<div id='terminal'>Terminal</div>")),
        JustifyContentValue::SpaceBetween
    );
    assert_eq!(
        computed(&node("<div id='important'>Important</div>")),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='inherit'>Root default</div>"))
            .justify_content(),
        JustifyContentValue::FlexStart
    );
}

#[test]
fn stylesheet_align_items_css_wide_resets_use_flex_start_and_preserve_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { align-items: center; } } @layer theme { #layer { align-items: revert-layer; } } #reset { align-items: flex-end; } #reset { align-items: INITIAL; } #unset { align-items: center; } #unset { align-items: UnSeT; } #revert { align-items: flex-end; } #revert { align-items: ReVeRt; } #invalid { align-items: center; } #invalid { align-items: inherit center; } #important { align-items: flex-end !important; } #important { align-items: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let reset = node("<div id='reset'>Reset</div>");
    let unset = node("<div id='unset'>Unset</div>");
    let revert = node("<div id='revert'>Revert</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_align = |element: &NativeNode, value: AlignItemsValue| {
        assert_eq!(stylesheet.computed_for(element).align_items(), value);
    };

    assert_align(&layer, AlignItemsValue::Center);
    assert_align(&reset, AlignItemsValue::FlexStart);
    assert_align(&unset, AlignItemsValue::FlexStart);
    assert_align(&revert, AlignItemsValue::FlexStart);
    assert_align(&invalid, AlignItemsValue::Center);
    assert_align(&important, AlignItemsValue::FlexEnd);
}

#[test]
fn stylesheet_align_items_explicit_inherit_uses_parent_without_implicit_inheritance() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { align-items: inherit; } #default { color: red; } #invalid { align-items: flex-end; align-items: inherit center; } #terminal { align-items: inherit; align-items: stretch; } #important { align-items: flex-end !important; } #important { align-items: inherit; }"
                .into(),
        ])
        .unwrap();
    let inherited = AlignItemsValue::Center;
    let computed = |element: &NativeNode| {
        stylesheet
            .computed_for_with_matcher(
                element,
                NativeInheritedStyle {
                    align_items: inherited,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(element),
            )
            .align_items()
    };

    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")),
        inherited
    );
    assert_eq!(
        computed(&node("<div id='default'>Default</div>")),
        AlignItemsValue::FlexStart
    );
    assert_eq!(
        computed(&node("<div id='invalid'>Invalid</div>")),
        AlignItemsValue::FlexEnd
    );
    assert_eq!(
        computed(&node("<div id='terminal'>Terminal</div>")),
        AlignItemsValue::Stretch
    );
    assert_eq!(
        computed(&node("<div id='important'>Important</div>")),
        AlignItemsValue::FlexEnd
    );
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='inherit'>Root default</div>"))
            .align_items(),
        AlignItemsValue::FlexStart
    );
}

#[test]
fn stylesheet_align_self_css_wide_resets_use_auto_and_preserve_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { align-self: center; } } @layer theme { #layer { align-self: revert-layer; } } #reset { align-self: flex-end; } #reset { align-self: INITIAL; } #unset { align-self: center; } #unset { align-self: UnSeT; } #revert { align-self: flex-end; } #revert { align-self: ReVeRt; } #invalid { align-self: center; } #invalid { align-self: inherit center; } #important { align-self: flex-end !important; } #important { align-self: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let reset = node("<div id='reset'>Reset</div>");
    let unset = node("<div id='unset'>Unset</div>");
    let revert = node("<div id='revert'>Revert</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_align = |element: &NativeNode, value: AlignSelfValue| {
        assert_eq!(stylesheet.computed_for(element).align_self(), value);
    };

    assert_align(&layer, AlignSelfValue::Center);
    assert_align(&reset, AlignSelfValue::Auto);
    assert_align(&unset, AlignSelfValue::Auto);
    assert_align(&revert, AlignSelfValue::Auto);
    assert_align(&invalid, AlignSelfValue::Center);
    assert_align(&important, AlignSelfValue::FlexEnd);
}

#[test]
fn stylesheet_align_self_explicit_inherit_uses_parent_without_implicit_inheritance() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { align-self: inherit; } #default { color: red; } #invalid { align-self: flex-end; align-self: inherit center; } #terminal { align-self: inherit; align-self: stretch; } #important { align-self: flex-end !important; } #important { align-self: inherit; } #auto { align-self: auto; }"
                .into(),
        ])
        .unwrap();
    let inherited = AlignSelfValue::Center;
    let computed = |element: &NativeNode| {
        stylesheet
            .computed_for_with_matcher(
                element,
                NativeInheritedStyle {
                    align_self: inherited,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(element),
            )
            .align_self()
    };

    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")),
        inherited
    );
    assert_eq!(
        computed(&node("<div id='default'>Default</div>")),
        AlignSelfValue::Auto
    );
    assert_eq!(
        computed(&node("<div id='auto'>Auto</div>")),
        AlignSelfValue::Auto
    );
    assert_eq!(
        computed(&node("<div id='invalid'>Invalid</div>")),
        AlignSelfValue::FlexEnd
    );
    assert_eq!(
        computed(&node("<div id='terminal'>Terminal</div>")),
        AlignSelfValue::Stretch
    );
    assert_eq!(
        computed(&node("<div id='important'>Important</div>")),
        AlignSelfValue::FlexEnd
    );
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='inherit'>Root default</div>"))
            .align_self(),
        AlignSelfValue::Auto
    );
}

#[test]
fn stylesheet_align_content_css_wide_resets_use_flex_start_and_preserve_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { align-content: center; } } @layer theme { #layer { align-content: revert-layer; } } #reset { align-content: flex-end; } #reset { align-content: INITIAL; } #unset { align-content: center; } #unset { align-content: UnSeT; } #revert { align-content: flex-end; } #revert { align-content: ReVeRt; } #invalid { align-content: center; } #invalid { align-content: initial center; } #important { align-content: flex-end !important; } #important { align-content: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let reset = node("<div id='reset'>Reset</div>");
    let unset = node("<div id='unset'>Unset</div>");
    let revert = node("<div id='revert'>Revert</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_align = |element: &NativeNode, value: AlignContentValue| {
        assert_eq!(stylesheet.computed_for(element).align_content(), value);
    };

    assert_align(&layer, AlignContentValue::Center);
    assert_align(&reset, AlignContentValue::FlexStart);
    assert_align(&unset, AlignContentValue::FlexStart);
    assert_align(&revert, AlignContentValue::FlexStart);
    assert_align(&invalid, AlignContentValue::Center);
    assert_align(&important, AlignContentValue::FlexEnd);
}

#[test]
fn stylesheet_align_content_explicit_inherit_uses_parent_without_implicit_inheritance() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { align-content: inherit; } #default { color: red; } #invalid { align-content: flex-end; align-content: inherit center; } #terminal { align-content: inherit; align-content: space-between; } #important { align-content: flex-end !important; } #important { align-content: inherit; }"
                .into(),
        ])
        .unwrap();
    let inherited = AlignContentValue::SpaceAround;
    let computed = |element: &NativeNode| {
        stylesheet
            .computed_for_with_matcher(
                element,
                NativeInheritedStyle {
                    align_content: inherited,
                    ..NativeInheritedStyle::default()
                },
                |selector| selector.matches(element),
            )
            .align_content()
    };

    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")),
        inherited
    );
    assert_eq!(
        computed(&node("<div id='default'>Default</div>")),
        AlignContentValue::FlexStart
    );
    assert_eq!(
        computed(&node("<div id='invalid'>Invalid</div>")),
        AlignContentValue::FlexEnd
    );
    assert_eq!(
        computed(&node("<div id='terminal'>Terminal</div>")),
        AlignContentValue::SpaceBetween
    );
    assert_eq!(
        computed(&node("<div id='important'>Important</div>")),
        AlignContentValue::FlexEnd
    );
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='inherit'>Root default</div>"))
            .align_content(),
        AlignContentValue::FlexStart
    );
}

#[test]
fn stylesheet_place_content_css_wide_resets_project_component_fallbacks() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { place-content: center flex-end; } } @layer theme { #layer { place-content: revert-layer; } } #reset { place-content: flex-end center; } #reset { place-content: INITIAL; } #unset { place-content: center space-between; } #unset { place-content: UnSeT; } #revert { place-content: flex-end center; } #revert { place-content: ReVeRt; } #invalid { place-content: center space-between; } #invalid { place-content: initial center; } #important { place-content: flex-end center !important; } #important { place-content: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let reset = node("<div id='reset'>Reset</div>");
    let unset = node("<div id='unset'>Unset</div>");
    let revert = node("<div id='revert'>Revert</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_values =
        |element: &NativeNode, align_content: AlignContentValue, justify_content| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.align_content(), align_content);
            assert_eq!(style.justify_content(), justify_content);
        };

    assert_values(
        &layer,
        AlignContentValue::Center,
        JustifyContentValue::FlexEnd,
    );
    assert_values(
        &reset,
        AlignContentValue::FlexStart,
        JustifyContentValue::FlexStart,
    );
    assert_values(
        &unset,
        AlignContentValue::FlexStart,
        JustifyContentValue::FlexStart,
    );
    assert_values(
        &revert,
        AlignContentValue::FlexStart,
        JustifyContentValue::FlexStart,
    );
    assert_values(
        &invalid,
        AlignContentValue::Center,
        JustifyContentValue::SpaceBetween,
    );
    assert_values(
        &important,
        AlignContentValue::FlexEnd,
        JustifyContentValue::Center,
    );
}

#[test]
fn stylesheet_place_content_inherit_projects_both_parent_components() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { place-content: InHeRiT; } #omitted { place-content: center; } #omitted { place-content: inherit center; } #important { place-content: center !important; } #important { place-content: inherit; }"
                .into(),
        ])
        .unwrap();
    let inherited_align = AlignContentValue::SpaceAround;
    let inherited_justify = JustifyContentValue::FlexEnd;
    let computed = |element: &NativeNode| {
        stylesheet.computed_for_with_matcher(
            element,
            NativeInheritedStyle {
                align_content: inherited_align,
                justify_content: inherited_justify,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(element),
        )
    };

    let inherited = computed(&node("<div id='inherit'>Inherit</div>"));
    assert_eq!(inherited.align_content(), inherited_align);
    assert_eq!(inherited.justify_content(), inherited_justify);

    let omitted = computed(&node("<div id='omitted'>Omitted</div>"));
    assert_eq!(omitted.align_content(), AlignContentValue::Center);
    assert_eq!(omitted.justify_content(), JustifyContentValue::Center);

    let important = computed(&node("<div id='important'>Important</div>"));
    assert_eq!(important.align_content(), AlignContentValue::Center);
    assert_eq!(important.justify_content(), JustifyContentValue::Center);
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_flex_shorthand_components() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #first { flex: 0 1 4px; } #repeated { flex: 2 3 7px; } #mixed { flex: 2 3 6px; } } @layer theme { #first { flex: 1 0 10px; } #repeated { flex: 4 2 9px; } #mixed { flex: revert-layer; flex-grow: 5; } } @layer top { #repeated { flex: revert-layer; } } #first { flex: revert-layer; } #repeated { flex: revert-layer; } #mixed { flex: revert-layer; } #fallback { flex: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let first = node("<div id='first'>First</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let mixed = node("<div id='mixed'>Mixed</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let inline = node("<div id='first' style='flex:REVERT-LAYER'>Inline</div>");

    let assert_values = |element: &NativeNode, grow: u32, shrink: u32, basis: FlexBasisValue| {
        let style = stylesheet.computed_for(element);
        assert_eq!(style.flex_grow(), grow);
        assert_eq!(style.flex_shrink(), shrink);
        assert_eq!(style.flex_basis(), basis);
    };

    assert_values(&first, 1, 0, FlexBasisValue::Length(10));
    assert_values(&repeated, 4, 2, FlexBasisValue::Length(9));
    assert_values(&mixed, 5, 3, FlexBasisValue::Length(6));
    assert_values(&fallback, 0, 1, FlexBasisValue::Auto);
    assert_values(&inline, 1, 0, FlexBasisValue::Length(10));
}

#[test]
fn stylesheet_flex_sizing_css_wide_resets_use_initial_fallbacks_and_preserve_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { flex: 3 4 9px; } } @layer theme { #layer { flex: revert-layer; } } #shorthand { flex: 4 5 12px; } #shorthand { flex: InItIaL; } #longhands { flex-grow: 4; } #longhands { flex-grow: unset; flex-shrink: 5; } #longhands { flex-shrink: revert; flex-basis: 12px; } #longhands { flex-basis: INITIAL; } #invalid { flex: 2 3 8px; } #invalid { flex: inherit 1 auto; } #important { flex: 5 6 10px !important; } #important { flex: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let shorthand = node("<div id='shorthand'>Shorthand</div>");
    let longhands = node("<div id='longhands'>Longhands</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_values = |element: &NativeNode, grow: u32, shrink: u32, basis: FlexBasisValue| {
        let style = stylesheet.computed_for(element);
        assert_eq!(style.flex_grow(), grow);
        assert_eq!(style.flex_shrink(), shrink);
        assert_eq!(style.flex_basis(), basis);
    };

    assert_values(&layer, 3, 4, FlexBasisValue::Length(9));
    assert_values(&shorthand, 0, 1, FlexBasisValue::Auto);
    assert_values(&longhands, 0, 1, FlexBasisValue::Auto);
    assert_values(&invalid, 2, 3, FlexBasisValue::Length(8));
    assert_values(&important, 5, 6, FlexBasisValue::Length(10));
}

#[test]
fn stylesheet_flex_inherit_projects_parent_components() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { flex-grow: 7; } #shrink-layered { flex-shrink: 7; } #basis-layered { flex-basis: 7px; } #order-layered { order: 7; } } @layer theme { #layered { flex-grow: inherit; } #shrink-layered { flex-shrink: inherit; } #basis-layered { flex-basis: inherit; } #order-layered { order: inherit; } } #inherit { flex: InHeRiT; } #direct { flex-grow: InHeRiT; } #direct-order { flex-grow: 1; } #direct-order { flex-grow: inherit; } #direct-invalid { flex-grow: 4; flex-grow: inherit 1; } #direct-important { flex-grow: 8 !important; } #direct-important { flex-grow: inherit; } #direct-reset { flex-grow: inherit; flex-grow: initial; } #direct-longhand { flex: 1 1 4px; flex-grow: inherit; } #shrink-direct { flex-shrink: InHeRiT; } #shrink-order { flex-shrink: 1; } #shrink-order { flex-shrink: inherit; } #shrink-invalid { flex-shrink: 4; flex-shrink: inherit 1; } #shrink-important { flex-shrink: 8 !important; } #shrink-important { flex-shrink: inherit; } #shrink-reset { flex-shrink: inherit; flex-shrink: initial; } #shrink-longhand { flex: 2 3 4px; flex-shrink: inherit; } #basis-direct { flex-basis: InHeRiT; } #basis-order { flex-basis: 1px; } #basis-order { flex-basis: inherit; } #basis-invalid { flex-basis: 4px; flex-basis: inherit 1px; } #basis-important { flex-basis: 8px !important; } #basis-important { flex-basis: inherit; } #basis-reset { flex-basis: inherit; flex-basis: initial; } #basis-longhand { flex: 2 3 4px; flex-basis: inherit; } #order-direct { order: InHeRiT; } #order-order { order: 1; } #order-order { order: inherit; } #order-invalid { order: 4; order: inherit 1; } #order-important { order: 8 !important; } #order-important { order: inherit; } #order-reset { order: inherit; order: initial; } #invalid { flex: 1 1 4px; flex: inherit 1 auto; } #important { flex: 2 3 12px !important; } #important { flex: inherit; } #longhands { flex: 1 1 4px; flex-grow: 4; flex-shrink: 5; flex-basis: 12px; }"
                .into(),
        ])
        .unwrap();
    let inherited_grow = 2;
    let inherited_shrink = 3;
    let inherited_basis = FlexBasisValue::Length(12);
    let inherited_order = NativeOrderValue(-7);
    let computed = |element: &NativeNode| {
        stylesheet.computed_for_with_matcher(
            element,
            NativeInheritedStyle {
                flex_grow: inherited_grow,
                flex_shrink: inherited_shrink,
                flex_basis: inherited_basis,
                flex_item_order: inherited_order,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(element),
        )
    };

    let inherited = computed(&node("<div id='inherit'>Inherit</div>"));
    assert_eq!(inherited.flex_grow(), inherited_grow);
    assert_eq!(inherited.flex_shrink(), inherited_shrink);
    assert_eq!(inherited.flex_basis(), inherited_basis);
    let layered = computed(&node("<div id='layered'>Layered</div>"));
    assert_eq!(layered.flex_grow(), inherited_grow);
    let direct = computed(&node("<div id='direct'>Direct</div>"));
    assert_eq!(direct.flex_grow(), inherited_grow);
    let direct_order = computed(&node("<div id='direct-order'>Direct order</div>"));
    assert_eq!(direct_order.flex_grow(), inherited_grow);
    let direct_invalid = computed(&node("<div id='direct-invalid'>Direct invalid</div>"));
    assert_eq!(direct_invalid.flex_grow(), 4);
    let direct_important = computed(&node("<div id='direct-important'>Direct important</div>"));
    assert_eq!(direct_important.flex_grow(), 8);
    let direct_reset = computed(&node("<div id='direct-reset'>Direct reset</div>"));
    assert_eq!(direct_reset.flex_grow(), 0);
    let direct_longhand = computed(&node("<div id='direct-longhand'>Direct longhand</div>"));
    assert_eq!(direct_longhand.flex_grow(), inherited_grow);
    assert_eq!(direct_longhand.flex_shrink(), 1);
    assert_eq!(direct_longhand.flex_basis(), FlexBasisValue::Length(4));
    let shrink_layered = computed(&node("<div id='shrink-layered'>Shrink layered</div>"));
    assert_eq!(shrink_layered.flex_shrink(), inherited_shrink);
    let shrink_direct = computed(&node("<div id='shrink-direct'>Shrink direct</div>"));
    assert_eq!(shrink_direct.flex_shrink(), inherited_shrink);
    let shrink_order = computed(&node("<div id='shrink-order'>Shrink order</div>"));
    assert_eq!(shrink_order.flex_shrink(), inherited_shrink);
    let shrink_invalid = computed(&node("<div id='shrink-invalid'>Shrink invalid</div>"));
    assert_eq!(shrink_invalid.flex_shrink(), 4);
    let shrink_important = computed(&node("<div id='shrink-important'>Shrink important</div>"));
    assert_eq!(shrink_important.flex_shrink(), 8);
    let shrink_reset = computed(&node("<div id='shrink-reset'>Shrink reset</div>"));
    assert_eq!(shrink_reset.flex_shrink(), 1);
    let shrink_longhand = computed(&node("<div id='shrink-longhand'>Shrink longhand</div>"));
    assert_eq!(shrink_longhand.flex_grow(), 2);
    assert_eq!(shrink_longhand.flex_shrink(), inherited_shrink);
    assert_eq!(shrink_longhand.flex_basis(), FlexBasisValue::Length(4));
    let basis_layered = computed(&node("<div id='basis-layered'>Basis layered</div>"));
    assert_eq!(basis_layered.flex_basis(), inherited_basis);
    let basis_direct = computed(&node("<div id='basis-direct'>Basis direct</div>"));
    assert_eq!(basis_direct.flex_basis(), inherited_basis);
    let basis_order = computed(&node("<div id='basis-order'>Basis order</div>"));
    assert_eq!(basis_order.flex_basis(), inherited_basis);
    let basis_invalid = computed(&node("<div id='basis-invalid'>Basis invalid</div>"));
    assert_eq!(basis_invalid.flex_basis(), FlexBasisValue::Length(4));
    let basis_important = computed(&node("<div id='basis-important'>Basis important</div>"));
    assert_eq!(basis_important.flex_basis(), FlexBasisValue::Length(8));
    let basis_reset = computed(&node("<div id='basis-reset'>Basis reset</div>"));
    assert_eq!(basis_reset.flex_basis(), FlexBasisValue::Auto);
    let basis_longhand = computed(&node("<div id='basis-longhand'>Basis longhand</div>"));
    assert_eq!(basis_longhand.flex_grow(), 2);
    assert_eq!(basis_longhand.flex_shrink(), 3);
    assert_eq!(basis_longhand.flex_basis(), inherited_basis);
    let basis_auto_element = node("<div id='basis-direct'>Basis auto</div>");
    let basis_auto = stylesheet.computed_for_with_matcher(
        &basis_auto_element,
        NativeInheritedStyle {
            flex_basis: FlexBasisValue::Auto,
            ..NativeInheritedStyle::default()
        },
        |selector| selector.matches(&basis_auto_element),
    );
    assert_eq!(basis_auto.flex_basis(), FlexBasisValue::Auto);
    let order_layered = computed(&node("<div id='order-layered'>Order layered</div>"));
    assert_eq!(order_layered.flex_item_order(), inherited_order);
    let order_direct = computed(&node("<div id='order-direct'>Order direct</div>"));
    assert_eq!(order_direct.flex_item_order(), inherited_order);
    let order_order = computed(&node("<div id='order-order'>Order order</div>"));
    assert_eq!(order_order.flex_item_order(), inherited_order);
    let order_invalid = computed(&node("<div id='order-invalid'>Order invalid</div>"));
    assert_eq!(order_invalid.flex_item_order(), NativeOrderValue(4));
    let order_important = computed(&node("<div id='order-important'>Order important</div>"));
    assert_eq!(order_important.flex_item_order(), NativeOrderValue(8));
    let order_reset = computed(&node("<div id='order-reset'>Order reset</div>"));
    assert_eq!(order_reset.flex_item_order(), NativeOrderValue::default());
    let omitted = computed(&node("<div id='omitted'>Omitted</div>"));
    assert_eq!(omitted.flex_grow(), 0);
    assert_eq!(omitted.flex_shrink(), 1);
    assert_eq!(omitted.flex_basis(), FlexBasisValue::Auto);
    assert_eq!(omitted.flex_item_order(), NativeOrderValue::default());
    let invalid = computed(&node("<div id='invalid'>Invalid</div>"));
    assert_eq!(invalid.flex_grow(), 1);
    assert_eq!(invalid.flex_shrink(), 1);
    assert_eq!(invalid.flex_basis(), FlexBasisValue::Length(4));
    let important = computed(&node("<div id='important'>Important</div>"));
    assert_eq!(important.flex_grow(), 2);
    assert_eq!(important.flex_shrink(), 3);
    assert_eq!(important.flex_basis(), FlexBasisValue::Length(12));
    let longhands = computed(&node("<div id='longhands'>Longhands</div>"));
    assert_eq!(longhands.flex_grow(), 4);
    assert_eq!(longhands.flex_shrink(), 5);
    assert_eq!(longhands.flex_basis(), FlexBasisValue::Length(12));
    let root = stylesheet.computed_for(&node("<div id='root'>Root</div>"));
    assert_eq!(root.flex_grow(), 0);
    assert_eq!(root.flex_shrink(), 1);
    assert_eq!(root.flex_basis(), FlexBasisValue::Auto);
}

#[test]
fn stylesheet_flex_flow_css_wide_resets_use_initial_fallbacks_and_preserve_cascade() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layer { flex-flow: column wrap; } } @layer theme { #layer { flex-flow: revert-layer; } } #shorthand { flex-flow: column wrap; } #shorthand { flex-flow: InItIaL; } #direction { flex-direction: column; } #direction { flex-direction: unset; } #wrap { flex-wrap: wrap; } #wrap { flex-wrap: ReVeRt; } #invalid { flex-flow: row-reverse wrap; } #invalid { flex-flow: inherit row; } #important { flex-flow: column wrap !important; } #important { flex-flow: initial; }"
                .into(),
        ])
        .unwrap();
    let layer = node("<div id='layer'>Layer</div>");
    let shorthand = node("<div id='shorthand'>Shorthand</div>");
    let direction = node("<div id='direction'>Direction</div>");
    let wrap = node("<div id='wrap'>Wrap</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let important = node("<div id='important'>Important</div>");

    let assert_values =
        |element: &NativeNode, direction: FlexDirectionValue, wrap: FlexWrapValue| {
            let style = stylesheet.computed_for(element);
            assert_eq!(style.flex_direction(), direction);
            assert_eq!(style.flex_wrap(), wrap);
        };

    assert_values(&layer, FlexDirectionValue::Column, FlexWrapValue::Wrap);
    assert_values(&shorthand, FlexDirectionValue::Row, FlexWrapValue::NoWrap);
    assert_values(&direction, FlexDirectionValue::Row, FlexWrapValue::NoWrap);
    assert_values(&wrap, FlexDirectionValue::Row, FlexWrapValue::NoWrap);
    assert_values(
        &invalid,
        FlexDirectionValue::RowReverse,
        FlexWrapValue::Wrap,
    );
    assert_values(&important, FlexDirectionValue::Column, FlexWrapValue::Wrap);
}

#[test]
fn stylesheet_flex_flow_inherit_projects_both_parent_components() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { flex-flow: InHeRiT; } #invalid { flex-flow: row wrap; flex-flow: inherit row; } #important { flex-flow: column wrap !important; } #important { flex-flow: inherit; } #longhands { flex-flow: row wrap; flex-direction: column; flex-wrap: wrap-reverse; }"
                .into(),
        ])
        .unwrap();
    let inherited_direction = FlexDirectionValue::ColumnReverse;
    let inherited_wrap = FlexWrapValue::WrapReverse;
    let computed = |element: &NativeNode| {
        stylesheet.computed_for_with_matcher(
            element,
            NativeInheritedStyle {
                flex_direction: inherited_direction,
                flex_wrap: inherited_wrap,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(element),
        )
    };

    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")).flex_direction(),
        inherited_direction
    );
    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")).flex_wrap(),
        inherited_wrap
    );
    let omitted = computed(&node("<div id='omitted'>Omitted</div>"));
    assert_eq!(omitted.flex_direction(), FlexDirectionValue::Row);
    assert_eq!(omitted.flex_wrap(), FlexWrapValue::NoWrap);
    let invalid = computed(&node("<div id='invalid'>Invalid</div>"));
    assert_eq!(invalid.flex_direction(), FlexDirectionValue::Row);
    assert_eq!(invalid.flex_wrap(), FlexWrapValue::Wrap);
    let important = computed(&node("<div id='important'>Important</div>"));
    assert_eq!(important.flex_direction(), FlexDirectionValue::Column);
    assert_eq!(important.flex_wrap(), FlexWrapValue::Wrap);
    let longhands = computed(&node("<div id='longhands'>Longhands</div>"));
    assert_eq!(longhands.flex_direction(), FlexDirectionValue::Column);
    assert_eq!(longhands.flex_wrap(), FlexWrapValue::WrapReverse);
    let root = stylesheet.computed_for(&node("<div id='root'>Root</div>"));
    assert_eq!(root.flex_direction(), FlexDirectionValue::Row);
    assert_eq!(root.flex_wrap(), FlexWrapValue::NoWrap);
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_flex_flow_and_place_content_components() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #flow { flex-flow: row nowrap; } #content { place-content: flex-start; } #mixed { flex-flow: row nowrap; place-content: flex-start; } #inline { flex-flow: row nowrap; place-content: flex-start; } } @layer theme { #flow { flex-flow: column wrap; } #content { place-content: space-between flex-end; } #mixed { flex-flow: revert-layer; flex-direction: row-reverse; place-content: revert-layer; justify-content: space-around; } #inline { flex-flow: column wrap; place-content: center flex-end; } } @layer top { #flow { flex-flow: revert-layer; } #content { place-content: revert-layer; } #mixed { flex-flow: column-reverse wrap; flex-flow: revert-layer; place-content: center; place-content: revert-layer; } } #flow { flex-flow: revert-layer; } #content { place-content: revert-layer; } #mixed { flex-flow: revert-layer; place-content: revert-layer; } #fallback { flex-flow: revert-layer; place-content: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let flow = node("<div id='flow'>Flow</div>");
    let content = node("<div id='content'>Content</div>");
    let mixed = node("<div id='mixed'>Mixed</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let inline = node(
        "<div id='inline' style='flex-flow:REVERT-LAYER;place-content:REVERT-LAYER'>Inline</div>",
    );

    let assert_values = |element: &NativeNode,
                         direction: FlexDirectionValue,
                         wrap: FlexWrapValue,
                         align_content: AlignContentValue,
                         justify_content: JustifyContentValue| {
        let style = stylesheet.computed_for(element);
        assert_eq!(style.flex_direction(), direction);
        assert_eq!(style.flex_wrap(), wrap);
        assert_eq!(style.align_content(), align_content);
        assert_eq!(style.justify_content(), justify_content);
    };

    assert_values(
        &flow,
        FlexDirectionValue::Column,
        FlexWrapValue::Wrap,
        AlignContentValue::FlexStart,
        JustifyContentValue::FlexStart,
    );
    assert_values(
        &content,
        FlexDirectionValue::Row,
        FlexWrapValue::NoWrap,
        AlignContentValue::SpaceBetween,
        JustifyContentValue::FlexEnd,
    );
    assert_values(
        &mixed,
        FlexDirectionValue::RowReverse,
        FlexWrapValue::NoWrap,
        AlignContentValue::FlexStart,
        JustifyContentValue::SpaceAround,
    );
    assert_values(
        &fallback,
        FlexDirectionValue::Row,
        FlexWrapValue::NoWrap,
        AlignContentValue::FlexStart,
        JustifyContentValue::FlexStart,
    );
    assert_values(
        &inline,
        FlexDirectionValue::Column,
        FlexWrapValue::Wrap,
        AlignContentValue::Center,
        JustifyContentValue::FlexEnd,
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_gap_components() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { gap: 2px 3px; } #rollback { gap: 6px 7px; } #repeated { gap: 10px 11px; } #fallback { gap: revert-layer; } #inline { gap: 18px 19px; } #invalid { gap: 24px 25px; gap: 1px 2px 3px; row-gap: 26px; row-gap: revert-layer 1px; } #longhand { row-gap: 2px; column-gap: 3px; } #mixed { gap: 30px 31px; } } @layer theme { .named { gap: 4px 5px; } #rollback { gap: 8px 9px; } #repeated { gap: 12px 13px; } #inline { gap: 20px 21px; } #longhand { row-gap: 4px; column-gap: 5px; } #mixed { gap: 40px 41px; } } @layer top { #repeated { gap: revert-layer; } #longhand { row-gap: revert-layer; column-gap: revert-layer; } } #named { gap: revert-layer; } #rollback { gap: revert-layer; row-gap: 10px; } #repeated { gap: revert-layer; } #mixed { gap: revert-layer; column-gap: 42px; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let fallback = node("<div id='fallback'>Fallback</div>");
    let inline = node("<div id='inline' style='gap:revert-layer;column-gap:22px'>Inline</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let longhand = node("<div id='longhand'>Longhand</div>");
    let mixed = node("<div id='mixed'>Mixed</div>");

    let assert_values = |element: &NativeNode, row: u32, column: u32| {
        let style = stylesheet.computed_for(element);
        assert_eq!(style.row_gap(), row);
        assert_eq!(style.column_gap(), column);
    };

    assert_values(&named, 4, 5);
    assert_values(&rollback, 10, 9);
    assert_values(&repeated, 12, 13);
    assert_values(&fallback, 0, 0);
    assert_values(&inline, 20, 22);
    assert_values(&invalid, 26, 25);
    assert_values(&longhand, 4, 5);
    assert_values(&mixed, 40, 42);
}

#[test]
fn stylesheet_gap_inherit_projects_parent_components() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { gap: 7px 9px; } #important { gap: 8px 10px !important; } } @layer theme { #layered { gap: inherit; } } #direct { gap: InHeRiT; } #order { gap: 1px 2px; } #order { gap: inherit; } #invalid { gap: 4px 5px; gap: inherit 1px; } #important-inherit { gap: 1px 2px; gap: inherit !important; } #reset { gap: inherit; gap: initial; } #revert { gap: inherit; gap: revert; }"
                .into(),
        ])
        .unwrap();
    let inherited_row = 12;
    let inherited_column = 14;
    let computed = |element: &NativeNode| {
        stylesheet.computed_for_with_matcher(
            element,
            NativeInheritedStyle {
                row_gap: inherited_row,
                column_gap: inherited_column,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(element),
        )
    };
    let assert_values = |element: &NativeNode, row: u32, column: u32| {
        let style = computed(element);
        assert_eq!(style.row_gap(), row);
        assert_eq!(style.column_gap(), column);
    };

    assert_values(
        &node("<div id='layered'>Layered</div>"),
        inherited_row,
        inherited_column,
    );
    assert_values(
        &node("<div id='direct'>Direct</div>"),
        inherited_row,
        inherited_column,
    );
    assert_values(
        &node("<div id='order'>Order</div>"),
        inherited_row,
        inherited_column,
    );
    assert_values(&node("<div id='invalid'>Invalid</div>"), 4, 5);
    assert_values(
        &node("<div id='important-inherit'>Important inherit</div>"),
        inherited_row,
        inherited_column,
    );
    assert_values(&node("<div id='reset'>Reset</div>"), 0, 0);
    assert_values(&node("<div id='revert'>Revert</div>"), 0, 0);
    assert_values(&node("<div id='omitted'>Omitted</div>"), 0, 0);

    assert_values(&node("<div id='root'>Root</div>"), 0, 0);
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='direct'>Root default</div>"))
            .row_gap(),
        0
    );
}

#[test]
fn stylesheet_gap_longhand_inherit_projects_independent_axes() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #layered { row-gap: 7px; column-gap: 9px; } #important { row-gap: 8px !important; column-gap: 10px !important; } #shorthand { gap: 1px 2px; } } @layer theme { #layered { row-gap: inherit; column-gap: InHeRiT; } #important { row-gap: inherit; column-gap: inherit; } #shorthand { row-gap: inherit; } } #row { row-gap: InHeRiT; } #column { column-gap: inherit; } #order { row-gap: 1px; row-gap: inherit; column-gap: 2px; column-gap: inherit; } #invalid { row-gap: 4px; row-gap: inherit 1px; column-gap: 5px; column-gap: 1px inherit; } #important-inherit { row-gap: 1px; row-gap: inherit !important; column-gap: 2px; column-gap: inherit !important; } #reset { row-gap: inherit; row-gap: initial; column-gap: inherit; column-gap: unset; } #revert { row-gap: inherit; row-gap: revert; column-gap: inherit; column-gap: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let inherited_row = 12;
    let inherited_column = 14;
    let computed = |element: &NativeNode| {
        stylesheet.computed_for_with_matcher(
            element,
            NativeInheritedStyle {
                row_gap: inherited_row,
                column_gap: inherited_column,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(element),
        )
    };
    let assert_values = |element: &NativeNode, row: u32, column: u32| {
        let style = computed(element);
        assert_eq!(style.row_gap(), row);
        assert_eq!(style.column_gap(), column);
    };

    assert_values(
        &node("<div id='layered'>Layered</div>"),
        inherited_row,
        inherited_column,
    );
    assert_values(&node("<div id='important'>Important</div>"), 8, 10);
    assert_values(
        &node("<div id='shorthand'>Shorthand</div>"),
        inherited_row,
        2,
    );
    assert_values(&node("<div id='row'>Row</div>"), inherited_row, 0);
    assert_values(&node("<div id='column'>Column</div>"), 0, inherited_column);
    assert_values(
        &node("<div id='order'>Order</div>"),
        inherited_row,
        inherited_column,
    );
    assert_values(&node("<div id='invalid'>Invalid</div>"), 4, 5);
    assert_values(
        &node("<div id='important-inherit'>Important inherit</div>"),
        inherited_row,
        inherited_column,
    );
    assert_values(&node("<div id='reset'>Reset</div>"), 0, 0);
    assert_values(&node("<div id='revert'>Revert</div>"), 0, 0);
    assert_values(&node("<div id='omitted'>Omitted</div>"), 0, 0);
}

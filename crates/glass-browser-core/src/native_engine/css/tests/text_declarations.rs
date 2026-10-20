use super::*;

#[test]
fn text_decoration_declaration_parser_accepts_css_wide_resets_and_line_values() {
    assert_eq!(
        parse_text_decoration_declaration("ReVeRt-LaYeR"),
        Some(NativeTextDecorationDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_decoration_declaration("InHeRiT"),
        Some(NativeTextDecorationDeclaration::Inherit)
    );
    assert_eq!(
        parse_text_decoration_declaration("InItIaL"),
        Some(NativeTextDecorationDeclaration::Initial)
    );
    assert_eq!(
        parse_text_decoration_declaration("UnSeT"),
        Some(NativeTextDecorationDeclaration::Unset)
    );
    assert_eq!(
        parse_text_decoration_declaration("ReVeRt"),
        Some(NativeTextDecorationDeclaration::Revert)
    );
    assert_eq!(
        parse_text_decoration_declaration("underline overline line-through"),
        Some(NativeTextDecorationDeclaration::Value(
            TextDecorationValue::new(true, true, true)
        ))
    );
    assert_eq!(
        parse_text_decoration_declaration("text-decoration-line: revert-layer"),
        None
    );
    for value in [
        "all",
        "revert-layer underline",
        "underline revert-layer",
        "none underline",
        "underline underline",
        "",
    ] {
        assert_eq!(parse_text_decoration_declaration(value), None, "{value}");
    }
    assert_eq!(
        parse_declarations("text-decoration-line: revert-layer").text_decoration,
        Some(NativeTextDecorationDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_declarations("text-decoration: revert-layer").text_decoration,
        Some(NativeTextDecorationDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_declarations("text-decoration-line: inherit").text_decoration,
        Some(NativeTextDecorationDeclaration::Inherit)
    );
    assert_eq!(
        parse_declarations("text-decoration: initial").text_decoration,
        Some(NativeTextDecorationDeclaration::Initial)
    );
}
#[test]
fn text_decoration_shorthand_parser_accepts_custom_property_aliases_and_fallbacks() {
    let decoration = parse_custom_property_name("--decoration").unwrap();
    assert_eq!(
        parse_text_decoration_declaration("var(--decoration)"),
        Some(NativeTextDecorationDeclaration::CustomProperty(decoration))
    );
    assert_eq!(
        parse_text_decoration_declaration("var(--decoration, underline overline)"),
        Some(NativeTextDecorationDeclaration::CustomPropertyFallback(
            decoration,
            TextDecorationValue::new(true, true, false)
        ))
    );
    assert_eq!(
        parse_declarations("text-decoration-line: var(--decoration)").text_decoration,
        Some(NativeTextDecorationDeclaration::CustomProperty(decoration))
    );
    assert_eq!(
        parse_declarations("text-decoration: var(--decoration, none)").text_decoration,
        Some(NativeTextDecorationDeclaration::CustomPropertyFallback(
            decoration,
            TextDecorationValue::none()
        ))
    );
    assert_eq!(
        parse_text_decoration_declaration("var(--decoration, var(--nested))"),
        None
    );
    assert_eq!(
        parse_text_decoration_declaration("var(--decoration, inherit)"),
        None
    );
}

#[test]
fn text_decoration_css_wide_resets_follow_parent_and_initial() {
    let document = NativeDocument::parse(
            "<style>.line { display:block; } #parent { text-decoration: underline overline; } #initial { text-decoration-line: initial; } #inherit { text-decoration-line: inherit; } #unset { text-decoration: unset; } #revert { text-decoration: revert; } #invalid { text-decoration-line: overline; text-decoration-line: invalid; } #important { text-decoration-line: underline !important; } #important { text-decoration-line: none; } #root-initial { text-decoration-line: initial; }</style><div id='parent' class='line'><span id='initial' class='line'>Initial</span><span id='inherit' class='line'>Inherit</span><span id='unset' class='line'>Unset</span><span id='revert' class='line'>Revert</span><span id='invalid' class='line'>Invalid</span><span id='important' class='line'>Important</span></div><div id='root-initial' class='line'>Root initial</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let line_for = |id| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_decoration()
    };
    let parent = TextDecorationValue::new(true, true, false);
    assert_eq!(line_for("parent"), parent);
    assert_eq!(line_for("initial"), TextDecorationValue::none());
    assert_eq!(line_for("inherit"), parent);
    assert_eq!(line_for("unset"), parent);
    assert_eq!(line_for("revert"), parent);
    assert_eq!(
        line_for("invalid"),
        TextDecorationValue::new(false, true, false)
    );
    assert_eq!(
        line_for("important"),
        TextDecorationValue::new(true, false, false)
    );
    assert_eq!(line_for("root-initial"), TextDecorationValue::none());
}
#[test]
fn text_decoration_shorthand_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent {
                --decoration: underline overline;
                --alias: var(--decoration);
                text-decoration-line: var(--alias);
            }
            #child { text-decoration: var(--alias); }
            #fallback { text-decoration-line: var(--missing, line-through); }
            #invalid {
                --bad: unsupported;
                text-decoration-line: var(--bad, underline);
            }
            #cycle {
                --cycle: var(--cycle);
                text-decoration-line: var(--cycle, overline);
            }
            #reset {
                --reset: initial;
                text-decoration-line: var(--reset, underline overline);
            }
            #override {
                --decoration: none;
                text-decoration-line: var(--decoration);
                text-decoration-line: underline;
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
    let decoration_for = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_decoration()
    };
    assert_eq!(
        decoration_for("parent"),
        TextDecorationValue::new(true, true, false)
    );
    assert_eq!(
        decoration_for("child"),
        TextDecorationValue::new(true, true, false)
    );
    assert_eq!(
        decoration_for("fallback"),
        TextDecorationValue::new(false, false, true)
    );
    assert_eq!(
        decoration_for("invalid"),
        TextDecorationValue::new(true, false, false)
    );
    assert_eq!(
        decoration_for("cycle"),
        TextDecorationValue::new(false, true, false)
    );
    assert_eq!(decoration_for("reset"), TextDecorationValue::none());
    assert_eq!(
        decoration_for("override"),
        TextDecorationValue::new(true, false, false)
    );
}

#[test]
fn stylesheet_cascade_revert_layer_rolls_back_text_decoration_candidates() {
    let document = NativeDocument::parse(
            "<style>.line { display:block; width:24px; height:20px; line-height:20px; } @layer base { #named { text-decoration-line:underline; } #rollback { text-decoration:underline; } #repeated { text-decoration:underline; } #unlayered { text-decoration:underline; } #inline { text-decoration:underline; } #fallback { text-decoration-line:revert-layer; } } @layer theme { .named { text-decoration-line:overline; } #rollback { text-decoration-line:revert-layer; } #repeated { text-decoration-line:revert-layer; } #unlayered { text-decoration-line:overline; } #inline { text-decoration-line:overline; } } @layer top { #repeated { text-decoration-line:revert-layer; } } #named { text-decoration-line:revert-layer; } #unlayered { text-decoration-line:revert-layer; }</style><div id='named' class='line named'>A</div><div id='rollback' class='line'>A</div><div id='repeated' class='line'>A</div><div id='unlayered' class='line'>A</div><div id='inline' class='line' style='text-decoration-line:revert-layer'>A</div><div id='fallback' class='line'>A</div><div id='parent' class='line' style='text-decoration:line-through'><span id='child' style='text-decoration-line:revert-layer'>A</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let line_for = |id| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_decoration()
    };
    assert_eq!(
        line_for("named"),
        TextDecorationValue::new(false, true, false)
    );
    assert_eq!(
        line_for("rollback"),
        TextDecorationValue::new(true, false, false)
    );
    assert_eq!(
        line_for("repeated"),
        TextDecorationValue::new(true, false, false)
    );
    assert_eq!(
        line_for("unlayered"),
        TextDecorationValue::new(false, true, false)
    );
    assert_eq!(
        line_for("inline"),
        TextDecorationValue::new(false, true, false)
    );
    assert_eq!(line_for("fallback"), TextDecorationValue::none());
    assert_eq!(
        line_for("child"),
        TextDecorationValue::new(false, false, true)
    );
}

#[test]
fn text_presentation_important_parser_tracks_markers_and_invalid_preservation() {
    let declarations = parse_declarations(
        "white-space: pre !IMPORTANT; text-align: center !important; text-align-last: right !important; text-justify: inter-word !important; direction: rtl !important; text-decoration: underline !important; text-decoration-style: dashed !important; text-decoration-skip-ink: none !important; text-decoration-skip-spaces: all !important; text-decoration-thickness: 2px !important; text-underline-offset: -2px !important; text-transform: uppercase !important; font-weight: bold !important; font-style: italic !important; word-break: break-all !important; text-overflow: ellipsis !important; vertical-align: middle !important; text-indent: 4px !important; word-spacing: 2px !important; letter-spacing: 3px !important; line-height: 20px !important",
    );
    let importance = declarations.text_importance;
    assert!(importance.white_space);
    assert!(importance.text_align);
    assert!(importance.text_align_last);
    assert!(importance.text_justify);
    assert!(importance.direction);
    assert!(importance.text_decoration);
    assert!(importance.text_decoration_style);
    assert!(importance.text_decoration_skip_ink);
    assert!(importance.text_decoration_skip_spaces);
    assert!(importance.text_decoration_thickness);
    assert!(importance.text_underline_offset);
    assert!(importance.text_transform);
    assert!(importance.font_weight);
    assert!(importance.font_style);
    assert!(importance.word_break);
    assert!(importance.text_overflow);
    assert!(importance.vertical_align);
    assert!(importance.text_indent);
    assert!(importance.word_spacing);
    assert!(importance.letter_spacing);
    assert!(importance.line_height);

    let preserved = parse_declarations(
        "white-space: pre !important; white-space: invalid !important; text-decoration: underline !important; text-decoration: invalid !important; line-height: 20px !important; line-height: invalid !important",
    );
    assert_eq!(
        preserved.white_space,
        Some(InheritedTextDeclaration::Value(
            WhiteSpaceDeclarationValue::Value(WhiteSpaceValue::Pre)
        ))
    );
    assert_eq!(
        preserved.text_decoration,
        Some(NativeTextDecorationDeclaration::Value(
            TextDecorationValue::new(true, false, false)
        ))
    );
    assert_eq!(
        preserved.line_height,
        Some(InheritedTextDeclaration::Value(
            LineHeightDeclarationValue::Value(20)
        ))
    );
    assert!(preserved.text_importance.white_space);
    assert!(preserved.text_importance.text_decoration);
    assert!(preserved.text_importance.line_height);
}

#[test]
fn stylesheet_cascade_resolves_text_presentation_important_priority_and_rollback() {
    let document = NativeDocument::parse(
            "<style>.line { display:block; width:32px; } @layer base { #winner { white-space:pre !important; text-align:center !important; text-transform:uppercase !important; text-decoration:underline !important; text-decoration-style:dashed !important; line-height:24px !important; } #rollback { text-transform:uppercase !important; text-decoration-style:dashed !important; } } @layer theme { #winner { white-space:nowrap !important; text-align:right !important; text-transform:lowercase !important; text-decoration:overline !important; text-decoration-style:dotted !important; line-height:12px !important; } #rollback { text-transform:revert-layer !important; text-decoration-style:revert-layer !important; } } #winner { white-space:normal !important; text-align:left !important; text-transform:none !important; text-decoration:none !important; text-decoration-style:solid !important; line-height:8px !important; } #normal { white-space:pre; text-align:right; text-transform:lowercase; line-height:8px; } </style><div id='winner' class='line'>A B</div><div id='rollback' class='line'>A</div><div id='normal' class='line'>A</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style_for = |id| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let winner = style_for("winner");
    assert_eq!(winner.white_space(), WhiteSpaceValue::Pre);
    assert_eq!(winner.text_align(), TextAlignValue::Center);
    assert_eq!(winner.text_transform(), TextTransformValue::Uppercase);
    assert_eq!(
        winner.text_decoration(),
        TextDecorationValue::new(true, false, false)
    );
    assert_eq!(
        winner.text_decoration_style(),
        NativeTextDecorationStyle::Dashed
    );
    assert_eq!(winner.line_height(), Some(24));

    let rollback = style_for("rollback");
    assert_eq!(rollback.text_transform(), TextTransformValue::Uppercase);
    assert_eq!(
        rollback.text_decoration_style(),
        NativeTextDecorationStyle::Dashed
    );

    let normal = style_for("normal");
    assert_eq!(normal.white_space(), WhiteSpaceValue::Pre);
    assert_eq!(normal.text_align(), TextAlignValue::Right);
    assert_eq!(normal.text_transform(), TextTransformValue::Lowercase);
    assert_eq!(normal.line_height(), Some(8));
}

#[test]
fn flex_gap_important_parser_tracks_markers_and_invalid_preservation() {
    let declarations = parse_declarations(
        "justify-content: center !IMPORTANT; place-content: space-around flex-end !important; align-items: center !important; align-self: flex-end !important; align-content: space-between !important; flex-direction: column !important; flex-wrap: wrap !important; flex-flow: row-reverse wrap-reverse !important; order: -3 !important; flex: 2 3 12px !important; flex-grow: 4 !important; flex-shrink: 5 !important; flex-basis: 16px !important; gap: 2px 3px !important; row-gap: 4px !important; column-gap: 5px !important",
    );
    assert_eq!(
        declarations.justify_content,
        Some(JustifyContentDeclaration::Value(
            JustifyContentValue::FlexEnd
        ))
    );
    assert_eq!(
        declarations.align_content,
        Some(AlignContentDeclaration::Value(
            AlignContentValue::SpaceBetween
        ))
    );
    assert_eq!(
        declarations.flex_direction,
        Some(FlexDirectionDeclaration::Value(
            FlexDirectionValue::RowReverse
        ))
    );
    assert_eq!(
        declarations.flex_wrap,
        Some(FlexWrapDeclaration::Value(FlexWrapValue::WrapReverse))
    );
    assert_eq!(
        declarations.order,
        Some(FlexItemOrderDeclaration::Value(NativeOrderValue(-3)))
    );
    assert_eq!(declarations.flex_grow, Some(FlexGrowDeclaration::Value(4)));
    assert_eq!(
        declarations.flex_shrink,
        Some(FlexShrinkDeclaration::Value(5))
    );
    assert_eq!(
        declarations.flex_basis,
        Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(16)))
    );
    assert_eq!(
        declarations.gap,
        Some(GapShorthandDeclaration::Value(NativeGapValue {
            row: 2,
            column: 3,
        }))
    );
    assert_eq!(
        declarations.row_gap,
        Some(GapComponentDeclaration::Value(4))
    );
    assert_eq!(
        declarations.column_gap,
        Some(GapComponentDeclaration::Value(5))
    );
    assert!(declarations.flex_importance.justify_content);
    assert!(declarations.flex_importance.align_items);
    assert!(declarations.flex_importance.align_self);
    assert!(declarations.flex_importance.align_content);
    assert!(declarations.flex_importance.flex_direction);
    assert!(declarations.flex_importance.flex_wrap);
    assert!(declarations.flex_importance.order);
    assert!(declarations.flex_importance.flex_grow);
    assert!(declarations.flex_importance.flex_shrink);
    assert!(declarations.flex_importance.flex_basis);
    assert!(declarations.gap_important);
    assert!(declarations.row_gap_important);
    assert!(declarations.column_gap_important);

    let preserved = parse_declarations(
        "place-content: space-around flex-end !important; place-content: unsupported !important; flex-flow: column wrap !important; flex-flow: column wrap wrap !important; flex: 2 3 12px !important; flex: 1 2 3% !important; gap: 2px 3px !important; gap: 1px 2px 3px !important; row-gap: 4px !important; row-gap: 1px 2px !important; column-gap: 5px !important; column-gap: revert-layer 1px !important",
    );
    assert_eq!(
        preserved.justify_content,
        Some(JustifyContentDeclaration::Value(
            JustifyContentValue::FlexEnd
        ))
    );
    assert_eq!(
        preserved.align_content,
        Some(AlignContentDeclaration::Value(
            AlignContentValue::SpaceAround
        ))
    );
    assert_eq!(
        preserved.flex_direction,
        Some(FlexDirectionDeclaration::Value(FlexDirectionValue::Column))
    );
    assert_eq!(
        preserved.flex_wrap,
        Some(FlexWrapDeclaration::Value(FlexWrapValue::Wrap))
    );
    assert_eq!(preserved.flex_grow, Some(FlexGrowDeclaration::Value(2)));
    assert_eq!(preserved.flex_shrink, Some(FlexShrinkDeclaration::Value(3)));
    assert_eq!(
        preserved.flex_basis,
        Some(FlexBasisDeclaration::Value(FlexBasisValue::Length(12)))
    );
    assert_eq!(
        preserved.gap,
        Some(GapShorthandDeclaration::Value(NativeGapValue {
            row: 2,
            column: 3,
        }))
    );
    assert_eq!(preserved.row_gap, Some(GapComponentDeclaration::Value(4)));
    assert_eq!(
        preserved.column_gap,
        Some(GapComponentDeclaration::Value(5))
    );
    assert!(preserved.flex_importance.justify_content);
    assert!(preserved.flex_importance.align_content);
    assert!(preserved.flex_importance.flex_direction);
    assert!(preserved.flex_importance.flex_wrap);
    assert!(preserved.flex_importance.flex_grow);
    assert!(preserved.flex_importance.flex_shrink);
    assert!(preserved.flex_importance.flex_basis);
    assert!(preserved.gap_important);
    assert!(preserved.row_gap_important);
    assert!(preserved.column_gap_important);
}

#[test]
fn stylesheet_cascade_resolves_flex_gap_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { place-content: space-around flex-end !important; align-items: center !important; align-self: flex-end !important; flex-flow: column wrap !important; order: -3 !important; flex: 2 3 12px !important; gap: 4px 6px !important; }
                #rollback { place-content: revert-layer !important; align-items: revert-layer !important; align-self: revert-layer !important; flex-flow: revert-layer !important; order: revert-layer !important; flex: revert-layer !important; gap: revert-layer !important; }
                #invalid { place-content: space-around flex-end !important; align-items: center !important; align-self: flex-end !important; flex-flow: column wrap !important; order: -3 !important; flex: 2 3 12px !important; gap: 4px 6px !important; }
                #normal { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: -1; flex: 1 1 8px; gap: 1px 2px; }
                #inline-normal { place-content: space-around flex-end !important; flex-flow: column wrap !important; flex: 2 3 12px !important; gap: 4px 6px !important; }
            }
            @layer theme {
                #important { place-content: center flex-start !important; align-items: flex-end !important; align-self: flex-start !important; flex-flow: row-reverse nowrap !important; order: 8 !important; flex: 4 5 20px !important; gap: 9px 10px !important; }
                #rollback { place-content: center flex-end !important; align-items: flex-end !important; align-self: center !important; flex-flow: row-reverse wrap-reverse !important; order: 8 !important; flex: 4 5 20px !important; gap: 9px 10px !important; }
                #invalid { place-content: unsupported !important; align-items: unsupported !important; align-self: unsupported !important; flex-flow: column wrap wrap !important; order: 1025 !important; flex: 1 2 3% !important; gap: 1px 2px 3px !important; }
                #normal { place-content: center center; align-items: center; align-self: center; flex-flow: column wrap; order: 2; flex: 2 2 10px; gap: 3px 4px; }
            }
            @layer top {
                #rollback { place-content: flex-end flex-start !important; align-items: flex-start !important; align-self: flex-start !important; flex-flow: column-reverse nowrap !important; order: 12 !important; flex: 6 7 24px !important; gap: 12px 13px !important; }
            }
            #important { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: 1; flex: 1 1 1px; gap: 1px 1px; }
            #rollback { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: 1; flex: 1 1 1px; gap: 1px 1px; }
            #invalid { place-content: flex-start flex-start; align-items: flex-start; align-self: auto; flex-flow: row nowrap; order: 1; flex: 1 1 1px; gap: 1px 1px; }
            #normal { place-content: space-between flex-end; align-items: flex-end; align-self: flex-end; flex-flow: row-reverse wrap-reverse; order: 4; flex: 3 4 12px; gap: 7px 8px; }
            #inline-normal { place-content: flex-start flex-start; flex-flow: row nowrap; flex: 1 1 1px; gap: 1px 1px; }
            #inline-important { place-content: flex-end flex-end !important; flex-flow: row-reverse wrap-reverse !important; flex: 3 4 12px !important; gap: 7px 8px !important; }
        "#
        .into(),
        ])
        .unwrap();
    let important = node("<div id='important'>Important</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let normal = node("<div id='normal'>Normal</div>");
    let inline_normal = node(
        "<div id='inline-normal' style='place-content:center center;flex-flow:column wrap;flex:5 6 18px;gap:11px 12px'>Inline normal</div>",
    );
    let inline_important = node(
        "<div id='inline-important' style='place-content:space-between flex-end !important;flex-flow:column-reverse wrap-reverse !important;flex:5 6 18px !important;gap:11px 12px !important'>Inline important</div>",
    );

    let assert_values = |element: &NativeNode,
                         justify_content,
                         align_items,
                         align_self,
                         align_content,
                         flex_direction,
                         flex_wrap,
                         order,
                         flex_grow,
                         flex_shrink,
                         flex_basis,
                         row_gap,
                         column_gap| {
        let style = stylesheet.computed_for(element);
        assert_eq!(style.justify_content(), justify_content);
        assert_eq!(style.align_items(), align_items);
        assert_eq!(style.align_self(), align_self);
        assert_eq!(style.align_content(), align_content);
        assert_eq!(style.flex_direction(), flex_direction);
        assert_eq!(style.flex_wrap(), flex_wrap);
        assert_eq!(style.flex_item_order(), order);
        assert_eq!(style.flex_grow(), flex_grow);
        assert_eq!(style.flex_shrink(), flex_shrink);
        assert_eq!(style.flex_basis(), flex_basis);
        assert_eq!(style.row_gap(), row_gap);
        assert_eq!(style.column_gap(), column_gap);
    };

    assert_values(
        &important,
        JustifyContentValue::FlexEnd,
        AlignItemsValue::Center,
        AlignSelfValue::FlexEnd,
        AlignContentValue::SpaceAround,
        FlexDirectionValue::Column,
        FlexWrapValue::Wrap,
        NativeOrderValue(-3),
        2,
        3,
        FlexBasisValue::Length(12),
        4,
        6,
    );
    assert_values(
        &rollback,
        JustifyContentValue::FlexEnd,
        AlignItemsValue::FlexEnd,
        AlignSelfValue::Center,
        AlignContentValue::Center,
        FlexDirectionValue::RowReverse,
        FlexWrapValue::WrapReverse,
        NativeOrderValue(8),
        4,
        5,
        FlexBasisValue::Length(20),
        9,
        10,
    );
    assert_values(
        &invalid,
        JustifyContentValue::FlexEnd,
        AlignItemsValue::Center,
        AlignSelfValue::FlexEnd,
        AlignContentValue::SpaceAround,
        FlexDirectionValue::Column,
        FlexWrapValue::Wrap,
        NativeOrderValue(-3),
        2,
        3,
        FlexBasisValue::Length(12),
        4,
        6,
    );
    assert_values(
        &normal,
        JustifyContentValue::FlexEnd,
        AlignItemsValue::FlexEnd,
        AlignSelfValue::FlexEnd,
        AlignContentValue::SpaceBetween,
        FlexDirectionValue::RowReverse,
        FlexWrapValue::WrapReverse,
        NativeOrderValue(4),
        3,
        4,
        FlexBasisValue::Length(12),
        7,
        8,
    );
    assert_values(
        &inline_normal,
        JustifyContentValue::FlexEnd,
        AlignItemsValue::FlexStart,
        AlignSelfValue::Auto,
        AlignContentValue::SpaceAround,
        FlexDirectionValue::Column,
        FlexWrapValue::Wrap,
        NativeOrderValue(0),
        2,
        3,
        FlexBasisValue::Length(12),
        4,
        6,
    );
    assert_values(
        &inline_important,
        JustifyContentValue::FlexEnd,
        AlignItemsValue::FlexStart,
        AlignSelfValue::Auto,
        AlignContentValue::SpaceBetween,
        FlexDirectionValue::ColumnReverse,
        FlexWrapValue::WrapReverse,
        NativeOrderValue(0),
        5,
        6,
        FlexBasisValue::Length(18),
        11,
        12,
    );
}

#[test]
fn dimension_important_parser_tracks_markers_and_invalid_preservation() {
    let declarations = parse_declarations(
        "width: 24px !IMPORTANT; height: 30px !important; min-width: 8px !important; max-width: 64px !important; min-height: 10px !important; max-height: 80px !important",
    );
    assert_eq!(
        declarations.width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(24)
        ))
    );
    assert_eq!(
        declarations.height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(30)
        ))
    );
    assert_eq!(
        declarations.min_width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(8)
        ))
    );
    assert_eq!(
        declarations.max_width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(64)
        ))
    );
    assert_eq!(
        declarations.min_height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(10)
        ))
    );
    assert_eq!(
        declarations.max_height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(80)
        ))
    );
    assert_eq!(
        declarations.dimension_importance,
        NativeDimensionDeclarationImportance {
            width: true,
            height: true,
            min_width: true,
            max_width: true,
            min_height: true,
            max_height: true,
        }
    );

    let preserved = parse_declarations(
        "width:24px !important;width:bad !important;height:30px !important;height:1px 2px !important;min-width:8px !important;min-width:-1px !important;max-width:64px !important;max-width:50% !important;min-height:10px !important;min-height:auto !important;max-height:80px !important;max-height:revert-layer 1px !important",
    );
    assert_eq!(
        preserved.width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(24)
        ))
    );
    assert_eq!(
        preserved.height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(30)
        ))
    );
    assert_eq!(
        preserved.min_width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(8)
        ))
    );
    assert_eq!(
        preserved.max_width,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(64)
        ))
    );
    assert_eq!(
        preserved.min_height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(10)
        ))
    );
    assert_eq!(
        preserved.max_height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(80)
        ))
    );
    assert_eq!(
        preserved.dimension_importance,
        NativeDimensionDeclarationImportance {
            width: true,
            height: true,
            min_width: true,
            max_width: true,
            min_height: true,
            max_height: true,
        }
    );
}

#[test]
fn stylesheet_cascade_resolves_dimension_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { width:24px !important; height:12px !important; min-width:4px !important; max-width:80px !important; min-height:6px !important; max-height:90px !important; }
                #rollback { width:30px !important; height:18px !important; min-width:8px !important; max-width:70px !important; min-height:10px !important; max-height:60px !important; }
                #rollback { width:revert-layer !important; height:revert-layer !important; min-width:revert-layer !important; max-width:revert-layer !important; min-height:revert-layer !important; max-height:revert-layer !important; }
                #invalid { width:40px !important; width:bad !important; height:20px !important; height:1px 2px !important; min-width:12px !important; min-width:-1px !important; max-width:60px !important; max-width:50% !important; min-height:14px !important; min-height:auto !important; max-height:50px !important; max-height:revert-layer 1px !important; }
                #normal { width:10px; height:10px; min-width:2px; max-width:20px; min-height:2px; max-height:20px; }
            }
            @layer theme {
                #important { width:48px !important; height:20px !important; min-width:16px !important; max-width:100px !important; min-height:10px !important; max-height:110px !important; }
                #rollback { width:36px !important; height:22px !important; min-width:12px !important; max-width:90px !important; min-height:8px !important; max-height:80px !important; }
                #normal { width:20px; height:20px; min-width:4px; max-width:40px; min-height:4px; max-height:40px; }
            }
            #important { width:64px !important; height:28px !important; min-width:24px !important; max-width:120px !important; min-height:14px !important; max-height:130px !important; }
            #rollback { width:72px !important; height:32px !important; min-width:28px !important; max-width:140px !important; min-height:16px !important; max-height:150px !important; }
            #invalid { width:80px; height:40px; min-width:32px; max-width:160px; min-height:18px; max-height:170px; }
            #normal { width:32px; height:32px; min-width:6px; max-width:60px; min-height:6px; max-height:60px; }
            #inline { width:64px !important; height:28px !important; min-width:24px !important; max-width:120px !important; min-height:14px !important; max-height:130px !important; }
        "#
        .into(),
        ])
        .unwrap();
    let important = node("<div id='important'>Important</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let normal = node("<div id='normal'>Normal</div>");
    let inline = node(
        "<div id='inline' style='width:91px !important;height:33px !important;min-width:31px !important;max-width:141px !important;min-height:17px !important;max-height:151px !important'>Inline</div>",
    );

    let assert_dimensions = |element: &NativeNode, expected: [Option<u32>; 6]| {
        let style = stylesheet.computed_for(element);
        assert_eq!(
            [
                style.width(),
                style.height(),
                style.min_width(),
                style.max_width(),
                style.min_height(),
                style.max_height(),
            ],
            expected
        );
    };

    assert_dimensions(
        &important,
        [Some(24), Some(12), Some(4), Some(80), Some(6), Some(90)],
    );
    assert_dimensions(
        &rollback,
        [Some(36), Some(22), Some(12), Some(90), Some(8), Some(80)],
    );
    assert_dimensions(
        &invalid,
        [Some(40), Some(20), Some(12), Some(60), Some(14), Some(50)],
    );
    assert_dimensions(
        &normal,
        [Some(32), Some(32), Some(6), Some(60), Some(6), Some(60)],
    );
    assert_dimensions(
        &inline,
        [Some(91), Some(33), Some(31), Some(141), Some(17), Some(151)],
    );
}

#[test]
fn box_model_important_parser_tracks_markers_and_invalid_preservation() {
    let declarations = parse_declarations(
        "box-sizing: border-box !IMPORTANT; padding: 1px 2px 3px 4px !important; padding-left: 5px; margin: auto 2px 3px 4px !important; margin-bottom: auto !IMPORTANT",
    );
    assert_eq!(
        declarations.padding,
        [
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(1)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(2)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(3)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(5)
            )),
        ]
    );
    assert_eq!(
        declarations.box_model_importance.padding,
        [true, true, true, false]
    );
    assert_eq!(
        declarations.margin,
        [
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Auto)
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(2))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Auto)
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(4))
            )),
        ]
    );
    assert_eq!(
        declarations.box_model_importance.margin,
        [true, true, true, true]
    );
    assert_eq!(
        declarations.box_sizing,
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::BorderBox)
        ))
    );
    assert!(declarations.box_model_importance.box_sizing);

    let preserved = parse_declarations(
        "box-sizing:border-box !important;box-sizing:auto !important;padding:4px !important;padding:bad !important;padding-right:9px !important;padding-right:-1px !important;margin:auto 2px 3px 4px !important;margin:-1px !important;margin-top:6px !important;margin-top:50% !important",
    );
    assert_eq!(
        preserved.padding,
        [
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(4)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(9)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(4)
            )),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(4)
            )),
        ]
    );
    assert_eq!(
        preserved.box_model_importance.padding,
        [true, true, true, true]
    );
    assert_eq!(
        preserved.margin,
        [
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(6))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(2))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(3))
            )),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(4))
            )),
        ]
    );
    assert_eq!(
        preserved.box_model_importance.margin,
        [true, true, true, true]
    );
    assert_eq!(
        preserved.box_sizing,
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::BorderBox)
        ))
    );
    assert!(preserved.box_model_importance.box_sizing);
}

#[test]
fn stylesheet_cascade_resolves_box_model_important_priority_and_rollback() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            r#"@layer base {
                #important { box-sizing:content-box !important; padding:1px 2px 3px 4px !important; padding-left:5px !important; margin:auto 2px 3px 4px !important; margin-bottom:6px !important; }
                #rollback { box-sizing:content-box !important; padding:1px 2px 3px 4px !important; margin:1px 2px 3px 4px !important; }
                #rollback { box-sizing:revert-layer !important; padding:revert-layer !important; margin:revert-layer !important; }
                #invalid { box-sizing:border-box !important; box-sizing:auto !important; padding:4px !important; padding:bad !important; margin:5px !important; margin:-1px !important; }
                #normal { box-sizing:content-box; padding:1px; margin:1px; }
            }
            @layer theme {
                #important { box-sizing:border-box !important; padding:5px 6px 7px 8px !important; margin:5px 6px 7px 8px !important; }
                #rollback { box-sizing:border-box !important; padding:6px 7px 8px 9px !important; margin:6px 7px 8px 9px !important; }
                #normal { box-sizing:border-box; padding:2px; margin:2px; }
            }
            #important { box-sizing:border-box !important; padding:9px !important; margin:9px !important; }
            #rollback { box-sizing:content-box !important; padding:10px !important; margin:10px !important; }
            #normal { box-sizing:content-box; padding:3px; margin:3px; }
            #inline { box-sizing:content-box !important; padding:11px !important; margin:11px !important; }"#
            .into(),
        ])
        .unwrap();
    let important = node("<div id='important'>Important</div>");
    let rollback = node("<div id='rollback'>Rollback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");
    let normal = node("<div id='normal'>Normal</div>");
    let inline = node(
        "<div id='inline' style='box-sizing:border-box !important;padding:12px !important;margin:auto !important'>Inline</div>",
    );

    let assert_box_model = |element: &NativeNode,
                            border_box: bool,
                            padding: NativeBoxEdges,
                            margin: NativeBoxEdges,
                            margin_auto: NativeAutoEdges| {
        let style = stylesheet.computed_for(element);
        assert_eq!(style.is_border_box(), border_box);
        assert_eq!(style.padding(), padding);
        assert_eq!(style.margin(), margin);
        assert_eq!(style.margin_auto(), margin_auto);
    };

    assert_box_model(
        &important,
        false,
        NativeBoxEdges {
            top: 1,
            right: 2,
            bottom: 3,
            left: 5,
        },
        NativeBoxEdges {
            top: 0,
            right: 2,
            bottom: 6,
            left: 4,
        },
        NativeAutoEdges {
            top: true,
            right: false,
            bottom: false,
            left: false,
        },
    );
    assert_box_model(
        &rollback,
        true,
        NativeBoxEdges {
            top: 6,
            right: 7,
            bottom: 8,
            left: 9,
        },
        NativeBoxEdges {
            top: 6,
            right: 7,
            bottom: 8,
            left: 9,
        },
        NativeAutoEdges::default(),
    );
    assert_box_model(
        &invalid,
        true,
        NativeBoxEdges::from_values([Some(4); 4]),
        NativeBoxEdges::from_values([Some(5); 4]),
        NativeAutoEdges::default(),
    );
    assert_box_model(
        &normal,
        false,
        NativeBoxEdges::from_values([Some(3); 4]),
        NativeBoxEdges::from_values([Some(3); 4]),
        NativeAutoEdges::default(),
    );
    assert_box_model(
        &inline,
        true,
        NativeBoxEdges::from_values([Some(12); 4]),
        NativeBoxEdges::default(),
        NativeAutoEdges {
            top: true,
            right: true,
            bottom: true,
            left: true,
        },
    );
}

#[test]
fn stylesheet_cascade_resolves_box_model_inherit_for_physical_and_logical_edges() {
    assert_eq!(
        parse_local_box_edges("InHeRiT"),
        Some([LocalCascadeDeclaration::Inherit; 4])
    );
    assert_eq!(
        parse_local_box_edge_pair("inherit"),
        Some([LocalCascadeDeclaration::Inherit; 2])
    );
    assert_eq!(
        parse_local_padding_declaration("INHERIT"),
        Some(LocalCascadeDeclaration::Inherit)
    );
    assert_eq!(
        parse_local_margin_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
    assert_eq!(
        parse_local_box_sizing_declaration("InHeRiT"),
        Some(LocalCascadeDeclaration::Inherit)
    );
    assert_eq!(parse_local_box_edges("inherit 1px"), None);
    assert_eq!(parse_local_margin_edge_pair("auto inherit"), None);

    let document = NativeDocument::parse(
            r#"<style>
            #physical-parent { padding:2px 4px 6px 8px; margin:auto 3px 5px 7px; box-sizing:border-box; }
            #physical-child { padding:inherit; margin:INHERIT; box-sizing:InHeRiT; }
            #logical-parent { direction:ltr; padding-inline:11px 13px; margin-inline:auto 17px; }
            #logical-child { direction:rtl; padding-inline-start:inherit; padding-inline-end:INHERIT; margin-inline-start:inherit; margin-inline-end:inherit; }
            #important-parent { padding:12px 13px 14px 15px; margin:auto 16px 17px 18px; box-sizing:border-box; }
            #important-child { padding:1px; padding:inherit !important; margin:2px; margin:inherit !important; box-sizing:content-box; box-sizing:inherit !important; }
            #rollback-parent { padding:19px; margin:20px; box-sizing:border-box; }
            @layer base { #rollback-child { padding:inherit; margin:inherit; box-sizing:inherit; } }
            @layer theme { #rollback-child { padding:1px; padding:revert-layer; margin:2px; margin:revert-layer; box-sizing:content-box; box-sizing:revert-layer; } }
            #root-inherit { padding:inherit; margin:inherit; box-sizing:inherit; }
            </style>
            <div id='physical-parent'><div id='physical-child'>Physical</div></div>
            <div id='logical-parent'><div id='logical-child'>Logical</div></div>
            <div id='important-parent'><div id='important-child'>Important</div></div>
            <div id='rollback-parent'><div id='rollback-child'>Rollback</div></div>
            <div id='root-inherit'>Root</div>"#,
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

    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let physical_parent = style("physical-parent");
    let physical_child = style("physical-child");
    assert!(physical_parent.is_border_box());
    assert_eq!(physical_child.padding(), physical_parent.padding());
    assert_eq!(physical_child.margin(), physical_parent.margin());
    assert_eq!(physical_child.margin_auto(), physical_parent.margin_auto());
    assert!(physical_child.is_border_box());

    let logical_child = style("logical-child");
    assert_eq!(
        logical_child.padding(),
        NativeBoxEdges {
            top: 0,
            right: 11,
            bottom: 0,
            left: 13,
        }
    );
    assert_eq!(
        logical_child.margin(),
        NativeBoxEdges {
            top: 0,
            right: 0,
            bottom: 0,
            left: 17,
        }
    );
    assert!(logical_child.margin_auto().right());
    assert!(!logical_child.margin_auto().left());

    let important_parent = style("important-parent");
    let important_child = style("important-child");
    assert_eq!(important_child.padding(), important_parent.padding());
    assert_eq!(important_child.margin(), important_parent.margin());
    assert_eq!(
        important_child.margin_auto(),
        important_parent.margin_auto()
    );
    assert!(important_child.is_border_box());

    let rollback_parent = style("rollback-parent");
    let rollback_child = style("rollback-child");
    assert_eq!(rollback_child.padding(), rollback_parent.padding());
    assert_eq!(rollback_child.margin(), rollback_parent.margin());
    assert!(rollback_child.is_border_box());

    let root_id = document.resolve_target("id=root-inherit").unwrap();
    let root = style("root-inherit");
    let parsed_parent = document.node(root_id).unwrap().parent().unwrap();
    let parent_style = document.computed_style_for_layout(parsed_parent);
    assert_eq!(root.padding(), NativeBoxEdges::default());
    // `margin: inherit` resolves against the implicit HTML body parent. Its
    // default margin is distinct from the CSS initial value.
    assert_eq!(root.margin(), parent_style.margin());
    assert_eq!(root.margin_auto(), parent_style.margin_auto());
    assert_eq!(root.is_border_box(), parent_style.is_border_box());
}

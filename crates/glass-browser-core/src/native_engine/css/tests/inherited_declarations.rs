use super::*;

#[test]
fn word_break_parser_accepts_only_normal_and_break_all() {
    assert_eq!(parse_word_break("normal"), Some(WordBreakValue::Normal));
    assert_eq!(
        parse_word_break("BREAK-ALL"),
        Some(WordBreakValue::BreakAll)
    );
    assert_eq!(parse_word_break("keep-all"), None);
    assert_eq!(parse_word_break("break-word"), None);
    assert_eq!(parse_word_break("initial"), None);
    let word_break_name = parse_custom_property_name("--word-break").unwrap();
    assert_eq!(
        parse_word_break_property("var(--word-break)"),
        Some(WordBreakDeclarationValue::CustomProperty(word_break_name))
    );
    assert_eq!(
        parse_word_break_declaration("var(--word-break, break-all)"),
        Some(InheritedTextDeclaration::Value(
            WordBreakDeclarationValue::CustomPropertyFallback(
                word_break_name,
                WordBreakValue::BreakAll
            )
        ))
    );
    assert_eq!(
        parse_word_break_property("var(--word-break, var(--other))"),
        None
    );
}

#[test]
fn inherited_text_declaration_parsers_accept_standalone_css_wide_keywords() {
    assert_eq!(
        parse_text_transform_declaration("ReVeRt-LaYeR"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_font_weight_declaration("revert-layer"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_font_style_declaration(" REVERT-LAYER "),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_word_break_declaration("revert-layer"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_transform_declaration("revert-layer uppercase"),
        None
    );
    assert_eq!(
        parse_font_weight_declaration("revert"),
        Some(InheritedTextDeclaration::Revert)
    );
    assert_eq!(
        parse_font_style_declaration("INITIAL"),
        Some(InheritedTextDeclaration::Initial)
    );
    assert_eq!(
        parse_word_break_declaration("unset"),
        Some(InheritedTextDeclaration::Unset)
    );
    assert_eq!(
        parse_text_transform_declaration("inherit"),
        Some(InheritedTextDeclaration::Inherit)
    );
    assert_eq!(parse_text_transform_declaration("uppercase initial"), None);
    assert_eq!(parse_font_style_declaration("oblique"), None);
    assert_eq!(parse_word_break_declaration("keep-all"), None);
    assert_eq!(
        parse_text_transform_declaration("uppercase"),
        Some(InheritedTextDeclaration::Value(
            TextTransformDeclarationValue::Value(TextTransformValue::Uppercase)
        ))
    );
}

#[test]
fn inherited_text_spacing_declaration_parsers_accept_standalone_css_wide_keywords() {
    assert_eq!(
        parse_word_spacing_declaration("ReVeRt-LaYeR"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_letter_spacing_declaration(" REVERT-LAYER "),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_word_spacing_declaration("16px"),
        Some(InheritedTextDeclaration::Value(
            WordSpacingDeclarationValue::Value(16)
        ))
    );
    assert_eq!(
        parse_letter_spacing_declaration("16px"),
        Some(InheritedTextDeclaration::Value(
            LetterSpacingDeclarationValue::Value(16)
        ))
    );
    assert_eq!(parse_letter_spacing_declaration("revert-layer 2px"), None);
    assert_eq!(
        parse_word_spacing_declaration("revert"),
        Some(InheritedTextDeclaration::Revert)
    );
    assert_eq!(
        parse_letter_spacing_declaration("unset"),
        Some(InheritedTextDeclaration::Unset)
    );
    assert_eq!(parse_letter_spacing_declaration("normal"), None);
    assert_eq!(parse_word_spacing_declaration("-1px"), None);
    assert_eq!(parse_letter_spacing_declaration("1.5px"), None);
    assert_eq!(parse_word_spacing_declaration("2em"), None);
    let word_spacing_name = parse_custom_property_name("--word-spacing").unwrap();
    assert_eq!(
        parse_word_spacing_property("var(--word-spacing)"),
        Some(WordSpacingDeclarationValue::CustomProperty(
            word_spacing_name
        ))
    );
    assert_eq!(
        parse_word_spacing_declaration("var(--word-spacing, 24px)"),
        Some(InheritedTextDeclaration::Value(
            WordSpacingDeclarationValue::CustomPropertyFallback(word_spacing_name, 24)
        ))
    );
    assert_eq!(
        parse_word_spacing_property("var(--word-spacing, var(--other))"),
        None
    );
    let letter_spacing_name = parse_custom_property_name("--letter-spacing").unwrap();
    assert_eq!(
        parse_letter_spacing_property("var(--letter-spacing)"),
        Some(LetterSpacingDeclarationValue::CustomProperty(
            letter_spacing_name
        ))
    );
    assert_eq!(
        parse_letter_spacing_declaration("var(--letter-spacing, 24px)"),
        Some(InheritedTextDeclaration::Value(
            LetterSpacingDeclarationValue::CustomPropertyFallback(letter_spacing_name, 24)
        ))
    );
    assert_eq!(
        parse_letter_spacing_property("var(--letter-spacing, var(--other))"),
        None
    );
}

#[test]
fn inherited_text_declarations_preserve_valid_values_before_invalid_later_values() {
    let declarations = parse_declarations(
        "text-transform: uppercase; text-transform: capitalize; font-weight: bold; font-weight: 1001; font-style: italic; font-style: oblique; word-break: break-all; word-break: keep-all; word-spacing: 12px; word-spacing: 1px 2px; letter-spacing: 13px; letter-spacing: normal;",
    );
    assert_eq!(
        declarations.text_transform,
        Some(InheritedTextDeclaration::Value(
            TextTransformDeclarationValue::Value(TextTransformValue::Uppercase)
        ))
    );
    assert_eq!(
        declarations.font_weight,
        Some(InheritedTextDeclaration::Value(
            FontWeightDeclarationValue::Absolute(FontWeightValue::Bold),
        ))
    );
    assert_eq!(
        declarations.font_style,
        Some(InheritedTextDeclaration::Value(
            FontStyleDeclarationValue::Italic
        ))
    );
    assert_eq!(
        declarations.word_break,
        Some(InheritedTextDeclaration::Value(
            WordBreakDeclarationValue::Value(WordBreakValue::BreakAll)
        ))
    );
    assert_eq!(
        declarations.word_spacing,
        Some(InheritedTextDeclaration::Value(
            WordSpacingDeclarationValue::Value(12)
        ))
    );
    assert_eq!(
        declarations.letter_spacing,
        Some(InheritedTextDeclaration::Value(
            LetterSpacingDeclarationValue::Value(13)
        ))
    );
}

#[test]
fn inherited_text_css_wide_keywords_resolve_parent_initial_and_terminal_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { white-space:pre; line-height:28px; text-transform:uppercase; font-weight:bold; font-style:italic; word-break:break-all; vertical-align:middle; word-spacing:12px; letter-spacing:13px; }
            #inherit { white-space:inherit; line-height:inherit; text-transform:inherit; font-weight:inherit; font-style:inherit; word-break:inherit; vertical-align:inherit; word-spacing:inherit; letter-spacing:inherit; }
            #unset { white-space:unset; line-height:unset; text-transform:unset; font-weight:unset; font-style:unset; word-break:unset; vertical-align:unset; word-spacing:unset; letter-spacing:unset; }
            #revert { white-space:ReVeRt; line-height:ReVeRt; text-transform:ReVeRt; font-weight:ReVeRt; font-style:ReVeRt; word-break:ReVeRt; vertical-align:ReVeRt; word-spacing:ReVeRt; letter-spacing:ReVeRt; }
            #initial { white-space:initial; line-height:initial; text-transform:initial; font-weight:initial; font-style:initial; word-break:initial; vertical-align:initial; word-spacing:initial; letter-spacing:initial; }
            #terminal { white-space:pre; white-space:initial; line-height:28px; line-height:initial; text-transform:uppercase; text-transform:initial; font-weight:bold; font-weight:initial; font-style:italic; font-style:initial; word-break:break-all; word-break:initial; vertical-align:middle; vertical-align:initial; word-spacing:12px; word-spacing:initial; letter-spacing:13px; letter-spacing:initial; }
            #invalid { white-space:pre; white-space:break-spaces; line-height:28px; line-height:0px; text-transform:uppercase; text-transform:capitalize; font-weight:bold; font-weight:1001; font-style:italic; font-style:oblique; word-break:break-all; word-break:keep-all; vertical-align:middle; vertical-align:sub; word-spacing:12px; word-spacing:-1px; letter-spacing:13px; letter-spacing:-1px; }
            </style>
            <div id='parent'><span id='inherit'>inherit</span><span id='unset'>unset</span><span id='revert'>revert</span><span id='initial'>initial</span><span id='terminal'>terminal</span><span id='invalid'>invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let values = |id| {
        let style = document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap());
        (
            style.white_space(),
            style.line_height(),
            style.text_transform(),
            style.font_weight(),
            style.font_style(),
            style.word_break(),
            style.vertical_align(),
            style.word_spacing(),
            style.letter_spacing(),
        )
    };
    let inherited = (
        WhiteSpaceValue::Pre,
        Some(28),
        TextTransformValue::Uppercase,
        FontWeightValue::Bold,
        FontStyleValue::Italic,
        WordBreakValue::BreakAll,
        VerticalAlignValue::Middle,
        12,
        13,
    );
    let initial = (
        WhiteSpaceValue::Normal,
        None,
        TextTransformValue::None,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        WordBreakValue::Normal,
        VerticalAlignValue::Baseline,
        0,
        0,
    );

    assert_eq!(values("parent"), inherited);
    assert_eq!(values("inherit"), inherited);
    assert_eq!(values("unset"), inherited);
    assert_eq!(values("revert"), inherited);
    assert_eq!(values("initial"), initial);
    assert_eq!(values("terminal"), initial);
    assert_eq!(values("invalid"), inherited);
}

#[test]
fn inherited_alignment_css_wide_keywords_resolve_parent_initial_and_terminal_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { text-align:right; text-align-last:justify; text-justify:inter-word; direction:rtl; }
            #inherit { text-align:inherit; text-align-last:inherit; text-justify:inherit; direction:inherit; }
            #unset { text-align:unset; text-align-last:unset; text-justify:unset; direction:unset; }
            #revert { text-align:ReVeRt; text-align-last:ReVeRt; text-justify:ReVeRt; direction:ReVeRt; }
            #initial { text-align:initial; text-align-last:initial; text-justify:initial; direction:initial; }
            #terminal { text-align:center; text-align:initial; text-align-last:end; text-align-last:initial; text-justify:none; text-justify:initial; direction:ltr; direction:initial; }
            #invalid { text-align:right; text-align:match-parent; text-align-last:justify; text-align-last:match-parent; text-justify:inter-word; text-justify:inter-character; direction:rtl; direction:vertical-rl; }
            </style>
            <div id='parent'><span id='inherit'>inherit</span><span id='unset'>unset</span><span id='revert'>revert</span><span id='initial'>initial</span><span id='terminal'>terminal</span><span id='invalid'>invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let values = |id| {
        let style = document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap());
        (
            style.text_align(),
            style.text_align_last(),
            style.text_justify(),
            style.direction(),
        )
    };
    let inherited = (
        TextAlignValue::Right,
        TextAlignLastValue::Justify,
        TextJustifyValue::InterWord,
        DirectionValue::Rtl,
    );
    let initial = (
        TextAlignValue::Left,
        TextAlignLastValue::Auto,
        TextJustifyValue::Auto,
        DirectionValue::Ltr,
    );

    assert_eq!(values("parent"), inherited);
    assert_eq!(values("inherit"), inherited);
    assert_eq!(values("unset"), inherited);
    assert_eq!(values("revert"), inherited);
    assert_eq!(values("initial"), initial);
    assert_eq!(values("terminal"), initial);
    assert_eq!(values("invalid"), inherited);
}

#[test]
fn inherited_vertical_align_declaration_parser_accepts_only_standalone_revert_layer() {
    assert_eq!(
        parse_vertical_align_declaration("ReVeRt-LaYeR"),
        Some(InheritedTextDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_vertical_align_declaration("middle"),
        Some(InheritedTextDeclaration::Value(
            VerticalAlignDeclarationValue::Value(VerticalAlignValue::Middle)
        ))
    );
    assert_eq!(
        parse_vertical_align_declaration("revert-layer middle"),
        None
    );
    assert_eq!(parse_vertical_align_declaration("1px"), None);
    assert_eq!(parse_vertical_align_declaration("50%"), None);
    assert_eq!(parse_vertical_align_declaration("text-top"), None);
    let declarations = parse_declarations("vertical-align: bottom; vertical-align: 1px;");
    assert_eq!(
        declarations.vertical_align,
        Some(InheritedTextDeclaration::Value(
            VerticalAlignDeclarationValue::Value(VerticalAlignValue::Bottom)
        ))
    );
    let vertical_align_name = parse_custom_property_name("--vertical-align").unwrap();
    assert_eq!(
        parse_vertical_align_property("var(--vertical-align)"),
        Some(VerticalAlignDeclarationValue::CustomProperty(
            vertical_align_name
        ))
    );
    assert_eq!(
        parse_vertical_align_declaration("var(--vertical-align, middle)"),
        Some(InheritedTextDeclaration::Value(
            VerticalAlignDeclarationValue::CustomPropertyFallback(
                vertical_align_name,
                VerticalAlignValue::Middle
            )
        ))
    );
    assert_eq!(
        parse_vertical_align_property("var(--vertical-align, var(--other))"),
        None
    );
}

#[test]
fn local_text_declaration_parsers_accept_css_wide_resets_and_revert_layer() {
    assert_eq!(
        parse_text_overflow_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_overflow_declaration("ellipsis"),
        Some(LocalCascadeDeclaration::Value(
            TextOverflowDeclarationValue::Value(TextOverflowValue::Ellipsis)
        ))
    );
    for keyword in ["INITIAL", "UnSeT", "ReVeRt"] {
        assert_eq!(
            parse_text_overflow_declaration(keyword),
            Some(LocalCascadeDeclaration::Reset),
            "text-overflow {keyword}"
        );
    }
    assert_eq!(
        parse_text_indent_declaration(" REVERT-LAYER "),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_text_indent_declaration("16px"),
        Some(LocalCascadeDeclaration::Value(
            TextIndentDeclarationValue::Value(16)
        ))
    );
    for keyword in ["initial", "UNSET", "revert"] {
        assert_eq!(
            parse_text_indent_declaration(keyword),
            Some(LocalCascadeDeclaration::Reset),
            "text-indent {keyword}"
        );
    }
    for keyword in ["inherit", "InHeRiT"] {
        assert_eq!(
            parse_text_overflow_declaration(keyword),
            Some(LocalCascadeDeclaration::Inherit),
            "text-overflow {keyword}"
        );
    }
    assert_eq!(parse_text_overflow_declaration("inherit ellipsis"), None);
    for keyword in ["inherit", "InHeRiT"] {
        assert_eq!(
            parse_text_indent_declaration(keyword),
            Some(LocalCascadeDeclaration::Inherit),
            "text-indent {keyword}"
        );
    }
    assert_eq!(parse_text_indent_declaration("inherit 1px"), None);
    assert_eq!(
        parse_text_overflow_declaration("revert-layer ellipsis"),
        None
    );
    assert_eq!(parse_text_indent_declaration("-1px"), None);
    assert_eq!(parse_text_indent_declaration("50%"), None);
    let declarations = parse_declarations(
        "text-indent: 12px; text-indent: 1px 2px; text-overflow: ellipsis; text-overflow: fade;",
    );
    assert_eq!(
        declarations.text_indent,
        Some(LocalCascadeDeclaration::Value(
            TextIndentDeclarationValue::Value(12)
        ))
    );
    assert_eq!(
        declarations.text_overflow,
        Some(LocalCascadeDeclaration::Value(
            TextOverflowDeclarationValue::Value(TextOverflowValue::Ellipsis)
        ))
    );
    let resets = parse_declarations("text-indent: initial; text-overflow: unset;");
    assert_eq!(resets.text_indent, Some(LocalCascadeDeclaration::Reset));
    assert_eq!(resets.text_overflow, Some(LocalCascadeDeclaration::Reset));
}

#[test]
fn local_text_css_wide_resets_use_fallbacks_and_preserve_invalid_later() {
    let document = NativeDocument::parse(
            "<style>#initial { text-indent: 16px; text-indent: initial; text-overflow: ellipsis; text-overflow: initial; } #unset { text-indent: 16px; text-indent: unset; text-overflow: ellipsis; text-overflow: unset; } #revert { text-indent: 16px; text-indent: revert; text-overflow: ellipsis; text-overflow: revert; } #invalid { text-indent: 16px; text-indent: 1px 2px; text-overflow: ellipsis; text-overflow: fade; } #important { text-indent: 16px !important; text-overflow: ellipsis !important; } #important { text-indent: initial; text-overflow: initial; }</style><div id='initial'>Initial</div><div id='unset'>Unset</div><div id='revert'>Revert</div><div id='invalid'>Invalid</div><div id='important'>Important</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    for id in ["initial", "unset", "revert"] {
        assert_eq!(style(id).text_indent(), 0, "text-indent for {id}");
        assert_eq!(
            style(id).text_overflow(),
            TextOverflowValue::Clip,
            "text-overflow for {id}"
        );
    }
    assert_eq!(style("invalid").text_indent(), 16);
    assert_eq!(
        style("invalid").text_overflow(),
        TextOverflowValue::Ellipsis
    );
    assert_eq!(style("important").text_indent(), 16);
    assert_eq!(
        style("important").text_overflow(),
        TextOverflowValue::Ellipsis
    );
}

#[test]
fn local_dimension_declaration_parser_accepts_only_standalone_revert_layer() {
    assert_eq!(
        parse_local_dimension_declaration("InHeRiT"),
        Some(LocalCascadeDeclaration::Inherit)
    );
    assert_eq!(
        parse_local_dimension_declaration("INITIAL"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_local_dimension_declaration("UnSeT"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_local_dimension_declaration("ReVeRt"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_local_dimension_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_local_dimension_declaration("24px"),
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(24)
        ))
    );
    assert_eq!(parse_local_dimension_declaration("revert-layer 24px"), None);
    assert_eq!(parse_local_dimension_declaration("-1px"), None);
    assert_eq!(parse_local_dimension_declaration("50%"), None);
    assert_eq!(parse_local_dimension_declaration("auto"), None);
    assert_eq!(parse_local_dimension_declaration("inherit 24px"), None);
    assert_eq!(parse_local_dimension_declaration("initial 24px"), None);
    let declarations = parse_declarations(
        "width:24px;width:1px 2px;height:30px;height:bad;min-width:8px;min-width:-1px;max-height:40px;max-height:50%;",
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
        declarations.max_height,
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::Value(40)
        ))
    );
}

#[test]
fn dimension_parser_accepts_custom_property_aliases_and_fallbacks() {
    let dimension = parse_custom_property_name("--dimension").unwrap();
    assert_eq!(
        parse_dimension_property("12px"),
        Some(DimensionDeclarationValue::Value(12))
    );
    assert_eq!(
        parse_local_dimension_declaration("var(--dimension)"),
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::CustomProperty(dimension)
        ))
    );
    assert_eq!(
        parse_local_dimension_declaration("var(--dimension, 8px)"),
        Some(LocalCascadeDeclaration::Value(
            DimensionDeclarationValue::CustomPropertyFallback(dimension, 8)
        ))
    );
    assert_eq!(
        parse_local_dimension_declaration("var(--dimension, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_dimension_declaration("initial"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_local_dimension_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn dimensions_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --size: 24px; --alias: var(--size); --cycle: var(--cycle); --bad: unsupported; --reset: initial; width: var(--size); height: var(--alias); min-width: var(--missing, 4px); max-width: var(--cycle, 40px); min-height: var(--bad, 6px); max-height: var(--reset, 30px); }
            #child { width: var(--alias); }
            #invalid { --bad: unsupported; width: var(--bad, 8px); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='invalid'>Invalid</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        let style = document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap());
        [
            style.width(),
            style.height(),
            style.min_width(),
            style.max_width(),
            style.min_height(),
            style.max_height(),
        ]
    };

    assert_eq!(
        style("parent"),
        [Some(24), Some(24), Some(4), Some(40), Some(6), None]
    );
    assert_eq!(style("child"), [Some(24), None, None, None, None, None]);
    assert_eq!(style("invalid"), [Some(8), None, None, None, None, None]);
}

#[test]
fn stylesheet_cascade_resolves_explicit_dimension_inheritance() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { width:24px; height:18px; min-width:4px; max-width:40px; min-height:6px; max-height:30px; }
            #child { width:inherit; height:INHERIT; min-width:InHeRiT; max-width:inherit; min-height:inherit; max-height:inherit; }
            #none-child { width:inherit; height:inherit; min-width:inherit; max-width:inherit; min-height:inherit; max-height:inherit; }
            @layer base { #rollback-child { width:12px; height:13px; min-width:2px; max-width:20px; min-height:3px; max-height:21px; } }
            @layer theme { #rollback-child { width:inherit; height:inherit; min-width:inherit; max-width:inherit; min-height:inherit; max-height:inherit; } #rollback-child { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } }
            #important-child { width:4px !important; height:5px !important; min-width:1px !important; max-width:6px !important; min-height:2px !important; max-height:7px !important; }
            #important-child { width:inherit !important; height:inherit !important; min-width:inherit !important; max-width:inherit !important; min-height:inherit !important; max-height:inherit !important; }
            #root-child { width:inherit; height:inherit; min-width:inherit; max-width:inherit; min-height:inherit; max-height:inherit; }
            </style>
            <div id='parent'><div id='child'>Child</div></div>
            <div id='none-parent'><div id='none-child'>None</div></div>
            <div id='rollback-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='rollback-child'>Rollback</div></div>
            <div id='important-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='important-child'>Important</div></div>
            <div id='root-child'>Root</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let dimensions = |style: NativeComputedStyle| {
        [
            style.width(),
            style.height(),
            style.min_width(),
            style.max_width(),
            style.min_height(),
            style.max_height(),
        ]
    };

    assert_eq!(
        dimensions(style("child")),
        [Some(24), Some(18), Some(4), Some(40), Some(6), Some(30)]
    );
    assert_eq!(
        dimensions(style("none-child")),
        [None, None, None, None, None, None]
    );
    assert_eq!(
        dimensions(style("rollback-child")),
        [Some(12), Some(13), Some(2), Some(20), Some(3), Some(21)]
    );
    assert_eq!(
        dimensions(style("important-child")),
        [Some(24), Some(18), Some(4), Some(40), Some(6), Some(30)]
    );
    assert_eq!(
        dimensions(style("root-child")),
        [None, None, None, None, None, None]
    );
}

#[test]
fn stylesheet_cascade_resolves_dimension_css_wide_resets() {
    let document = NativeDocument::parse(
            r#"<style>
            #child-reset { width:INITIAL; height:unset; min-width:ReVeRt; max-width:initial; min-height:unset; max-height:revert; }
            #invalid-reset { width:initial; }
            #invalid-reset { width:bad; }
            #important-reset { width:4px !important; height:5px !important; min-width:1px !important; max-width:6px !important; min-height:2px !important; max-height:7px !important; }
            #important-reset { width:initial !important; height:unset !important; min-width:revert !important; max-width:initial !important; min-height:unset !important; max-height:revert !important; }
            @layer base { #rollback-child { width:12px; height:13px; min-width:2px; max-width:20px; min-height:3px; max-height:21px; } }
            @layer theme { #rollback-child { width:initial; height:unset; min-width:revert; max-width:initial; min-height:unset; max-height:revert; } #rollback-child { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } }
            #root-reset { width:initial; height:unset; min-width:revert; max-width:initial; min-height:unset; max-height:revert; }
            #revert-child { width:revert; height:revert; min-width:revert; max-width:revert; min-height:revert; max-height:revert; }
            </style>
            <div id='parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'>
                <div id='child-reset'>Reset</div>
                <div id='invalid-reset'>Invalid</div>
                <div id='important-reset'>Important</div>
            </div>
            <div id='rollback-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='rollback-child'>Rollback</div></div>
            <div id='revert-parent' style='width:24px;height:18px;min-width:4px;max-width:40px;min-height:6px;max-height:30px'><div id='revert-child'>Revert</div></div>
            <div id='root-reset'>Root</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();

    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let dimensions = |style: NativeComputedStyle| {
        [
            style.width(),
            style.height(),
            style.min_width(),
            style.max_width(),
            style.min_height(),
            style.max_height(),
        ]
    };
    let none = [None, None, None, None, None, None];

    assert_eq!(dimensions(style("child-reset")), none);
    assert_eq!(dimensions(style("invalid-reset")), none);
    assert_eq!(dimensions(style("important-reset")), none);
    assert_eq!(
        dimensions(style("rollback-child")),
        [Some(12), Some(13), Some(2), Some(20), Some(3), Some(21)]
    );
    assert_eq!(dimensions(style("revert-child")), none);
    assert_eq!(dimensions(style("root-reset")), none);
}

#[test]
fn local_box_model_declaration_parsers_accept_bounded_reset_and_revert_layer_forms() {
    assert_eq!(
        parse_local_box_edges(" ReVeRt-LaYeR "),
        Some([LocalCascadeDeclaration::RevertLayer; 4])
    );
    assert_eq!(
        parse_local_padding_declaration("4px"),
        Some(LocalCascadeDeclaration::Value(4))
    );
    assert_eq!(
        parse_local_padding_declaration("revert-layer"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(parse_local_box_edges("revert-layer 4px"), None);
    assert_eq!(
        parse_local_box_edges("InItIaL"),
        Some([LocalCascadeDeclaration::Value(PaddingDeclarationValue::Value(0)); 4])
    );
    assert_eq!(
        parse_local_box_edge_pair("UNSET"),
        Some([LocalCascadeDeclaration::Value(0); 2])
    );
    assert_eq!(parse_local_box_edge_pair("revert 1px"), None);
    assert_eq!(parse_local_box_edges("50%"), None);
    assert_eq!(parse_local_box_edges("-1px"), None);
    assert_eq!(parse_local_box_edges("1px 2px 3px 4px 5px"), None);
    assert_eq!(
        parse_local_margin_edges("revert-layer"),
        Some([LocalCascadeDeclaration::RevertLayer; 4])
    );
    assert_eq!(
        parse_local_margin_declaration("auto"),
        Some(LocalCascadeDeclaration::Value(NativeMarginValue::Auto))
    );
    assert_eq!(
        parse_local_margin_declaration("revert-layer"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(parse_local_margin_edges("revert-layer auto"), None);
    assert_eq!(
        parse_local_margin_declaration("ReVeRt"),
        Some(LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)))
    );
    assert_eq!(
        parse_local_margin_edge_pair("initial"),
        Some([LocalCascadeDeclaration::Value(NativeMarginValue::Length(0)); 2])
    );
    assert_eq!(parse_local_margin_edge_pair("unset auto"), None);
    assert_eq!(parse_local_margin_edges("50%"), None);
    assert_eq!(parse_local_margin_edges("-1px"), None);
    assert_eq!(
        parse_local_box_sizing_declaration("ReVeRt-LaYeR"),
        Some(LocalCascadeDeclaration::RevertLayer)
    );
    assert_eq!(
        parse_local_box_sizing_declaration("border-box"),
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::BorderBox)
        ))
    );
    assert_eq!(
        parse_local_box_sizing_declaration("UNSET"),
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::ContentBox)
        ))
    );
    assert_eq!(
        parse_local_box_sizing_declaration("revert-layer border-box"),
        None
    );
    assert_eq!(parse_local_box_sizing_declaration("auto"), None);

    let declarations = parse_declarations(
        "padding: 1px 2px 3px 4px; padding-left: revert-layer; padding-right: 6px; padding-right: revert-layer; margin: auto 2px; margin-bottom: revert-layer; box-sizing: border-box; box-sizing: invalid;",
    );
    assert_eq!(
        declarations.padding,
        [
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(1)
            )),
            Some(LocalCascadeDeclaration::RevertLayer),
            Some(LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::Value(3)
            )),
            Some(LocalCascadeDeclaration::RevertLayer),
        ]
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
            Some(LocalCascadeDeclaration::RevertLayer),
            Some(LocalCascadeDeclaration::Value(
                MarginDeclarationValue::Value(NativeMarginValue::Length(2))
            )),
        ]
    );
    assert_eq!(
        declarations.box_sizing,
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::BorderBox)
        ))
    );
}

#[test]
fn margin_parser_accepts_custom_property_aliases_and_fallbacks() {
    let margin = parse_custom_property_name("--margin").unwrap();
    assert_eq!(
        parse_margin_property("auto"),
        Some(MarginDeclarationValue::Value(NativeMarginValue::Auto))
    );
    assert_eq!(
        parse_local_physical_margin_declaration("var(--margin)"),
        Some(LocalCascadeDeclaration::Value(
            MarginDeclarationValue::CustomProperty(margin)
        ))
    );
    assert_eq!(
        parse_local_physical_margin_declaration("var(--margin, auto)"),
        Some(LocalCascadeDeclaration::Value(
            MarginDeclarationValue::CustomPropertyFallback(margin, NativeMarginValue::Auto)
        ))
    );
    assert_eq!(
        parse_local_margin_edges("var(--margin)"),
        Some([
            LocalCascadeDeclaration::Value(MarginDeclarationValue::CustomPropertyShorthand(
                margin, 0
            )),
            LocalCascadeDeclaration::Value(MarginDeclarationValue::CustomPropertyShorthand(
                margin, 1
            )),
            LocalCascadeDeclaration::Value(MarginDeclarationValue::CustomPropertyShorthand(
                margin, 2
            )),
            LocalCascadeDeclaration::Value(MarginDeclarationValue::CustomPropertyShorthand(
                margin, 3
            )),
        ])
    );
    assert_eq!(
        parse_local_margin_edges("var(--margin, auto 2px)"),
        Some([
            LocalCascadeDeclaration::Value(
                MarginDeclarationValue::CustomPropertyShorthandFallback(
                    margin,
                    0,
                    NativeMarginValue::Auto
                )
            ),
            LocalCascadeDeclaration::Value(
                MarginDeclarationValue::CustomPropertyShorthandFallback(
                    margin,
                    1,
                    NativeMarginValue::Length(2)
                )
            ),
            LocalCascadeDeclaration::Value(
                MarginDeclarationValue::CustomPropertyShorthandFallback(
                    margin,
                    2,
                    NativeMarginValue::Auto
                )
            ),
            LocalCascadeDeclaration::Value(
                MarginDeclarationValue::CustomPropertyShorthandFallback(
                    margin,
                    3,
                    NativeMarginValue::Length(2)
                )
            ),
        ])
    );
    assert_eq!(
        parse_local_physical_margin_declaration("var(--margin, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_margin_edges("var(--margin, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_physical_margin_declaration("initial"),
        Some(LocalCascadeDeclaration::Value(
            MarginDeclarationValue::Value(NativeMarginValue::Length(0))
        ))
    );
    assert_eq!(
        parse_local_physical_margin_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn margin_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent { --margin: auto 2px 3px 4px; --alias: var(--margin); margin: var(--alias); }
            #child { margin: var(--alias); }
            #fallback { margin: var(--missing, 1px auto 3px 4px); }
            #invalid { --bad: unsupported; margin: var(--bad, auto); }
            #cycle { --cycle: var(--cycle); margin: var(--cycle, 6px); }
            #reset { --reset: initial; margin: var(--reset, 7px); }
            #longhand { --side: auto; margin-left: var(--side); }
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
        style("parent").margin(),
        NativeBoxEdges::from_values([Some(0), Some(2), Some(3), Some(4)])
    );
    assert_eq!(
        style("parent").margin_auto(),
        NativeAutoEdges {
            top: true,
            right: false,
            bottom: false,
            left: false,
        }
    );
    assert_eq!(style("child").margin(), style("parent").margin());
    assert_eq!(style("child").margin_auto(), style("parent").margin_auto());
    assert_eq!(
        style("fallback").margin(),
        NativeBoxEdges::from_values([Some(1), Some(0), Some(3), Some(4)])
    );
    assert_eq!(
        style("fallback").margin_auto(),
        NativeAutoEdges {
            top: false,
            right: true,
            bottom: false,
            left: false,
        }
    );
    assert_eq!(style("invalid").margin(), NativeBoxEdges::default());
    assert_eq!(
        style("invalid").margin_auto(),
        NativeAutoEdges::from_values([Some(NativeMarginValue::Auto); 4])
    );
    assert_eq!(
        style("cycle").margin(),
        NativeBoxEdges::from_values([Some(6); 4])
    );
    assert_eq!(style("cycle").margin_auto(), NativeAutoEdges::default());
    assert_eq!(style("reset").margin(), NativeBoxEdges::default());
    assert_eq!(style("reset").margin_auto(), NativeAutoEdges::default());
    assert_eq!(style("longhand").margin(), NativeBoxEdges::default());
    assert_eq!(
        style("longhand").margin_auto(),
        NativeAutoEdges {
            top: false,
            right: false,
            bottom: false,
            left: true,
        }
    );
}

#[test]
fn padding_parser_accepts_custom_property_aliases_and_fallbacks() {
    let padding = parse_custom_property_name("--padding").unwrap();
    assert_eq!(
        parse_padding_property("4px"),
        Some(PaddingDeclarationValue::Value(4))
    );
    assert_eq!(
        parse_local_physical_padding_declaration("var(--padding)"),
        Some(LocalCascadeDeclaration::Value(
            PaddingDeclarationValue::CustomProperty(padding)
        ))
    );
    assert_eq!(
        parse_local_physical_padding_declaration("var(--padding, 8px)"),
        Some(LocalCascadeDeclaration::Value(
            PaddingDeclarationValue::CustomPropertyFallback(padding, 8)
        ))
    );
    assert_eq!(
        parse_local_box_edges("var(--padding)"),
        Some([
            LocalCascadeDeclaration::Value(PaddingDeclarationValue::CustomPropertyShorthand(
                padding, 0
            )),
            LocalCascadeDeclaration::Value(PaddingDeclarationValue::CustomPropertyShorthand(
                padding, 1
            )),
            LocalCascadeDeclaration::Value(PaddingDeclarationValue::CustomPropertyShorthand(
                padding, 2
            )),
            LocalCascadeDeclaration::Value(PaddingDeclarationValue::CustomPropertyShorthand(
                padding, 3
            )),
        ])
    );
    assert_eq!(
        parse_local_box_edges("var(--padding, 1px 2px)"),
        Some([
            LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::CustomPropertyShorthandFallback(padding, 0, 1)
            ),
            LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::CustomPropertyShorthandFallback(padding, 1, 2)
            ),
            LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::CustomPropertyShorthandFallback(padding, 2, 1)
            ),
            LocalCascadeDeclaration::Value(
                PaddingDeclarationValue::CustomPropertyShorthandFallback(padding, 3, 2)
            ),
        ])
    );
    assert_eq!(
        parse_local_physical_padding_declaration("var(--padding, var(--other))"),
        None
    );
    assert_eq!(parse_local_box_edges("var(--padding, var(--other))"), None);
    assert_eq!(
        parse_local_physical_padding_declaration("initial"),
        Some(LocalCascadeDeclaration::Value(
            PaddingDeclarationValue::Value(0)
        ))
    );
    assert_eq!(
        parse_local_physical_padding_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn padding_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent { --pad: 2px 4px 6px 8px; --alias: var(--pad); padding: var(--alias); }
            #child { padding: var(--alias); }
            #fallback { padding: var(--missing, 3px 4px); }
            #invalid { --bad: unsupported; padding: var(--bad, 5px); }
            #cycle { --cycle: var(--cycle); padding: var(--cycle, 6px); }
            #reset { --reset: initial; padding: var(--reset, 7px); }
            #longhand { --side: 9px; padding-left: var(--side); }
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
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .padding()
    };

    assert_eq!(
        style("parent"),
        NativeBoxEdges::from_values([Some(2), Some(4), Some(6), Some(8)])
    );
    assert_eq!(
        style("child"),
        NativeBoxEdges::from_values([Some(2), Some(4), Some(6), Some(8)])
    );
    assert_eq!(
        style("fallback"),
        NativeBoxEdges::from_values([Some(3), Some(4), Some(3), Some(4)])
    );
    assert_eq!(style("invalid"), NativeBoxEdges::from_values([Some(5); 4]));
    assert_eq!(style("cycle"), NativeBoxEdges::from_values([Some(6); 4]));
    assert_eq!(style("reset"), NativeBoxEdges::default());
    assert_eq!(
        style("longhand"),
        NativeBoxEdges::from_values([None, None, None, Some(9)])
    );
}

#[test]
fn box_sizing_parser_accepts_custom_property_aliases_and_fallbacks() {
    let box_sizing = parse_custom_property_name("--box-sizing").unwrap();
    assert_eq!(
        parse_box_sizing_property("border-box"),
        Some(BoxSizingDeclarationValue::Value(NativeBoxSizing::BorderBox))
    );
    assert_eq!(
        parse_local_box_sizing_declaration("var(--box-sizing)"),
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::CustomProperty(box_sizing)
        ))
    );
    assert_eq!(
        parse_local_box_sizing_declaration("var(--box-sizing, content-box)"),
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::CustomPropertyFallback(
                box_sizing,
                NativeBoxSizing::ContentBox
            )
        ))
    );
    assert_eq!(
        parse_local_box_sizing_declaration("var(--box-sizing, var(--other))"),
        None
    );
    assert_eq!(
        parse_local_box_sizing_declaration("initial"),
        Some(LocalCascadeDeclaration::Value(
            BoxSizingDeclarationValue::Value(NativeBoxSizing::ContentBox)
        ))
    );
    assert_eq!(
        parse_local_box_sizing_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn box_sizing_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --box: border-box; --alias: var(--box); --cycle: var(--cycle); --reset: initial; box-sizing: var(--box); }
            #child { box-sizing: var(--alias); }
            #fallback { box-sizing: var(--missing, border-box); }
            #invalid { --bad: unsupported; box-sizing: var(--bad, border-box); }
            #cycle { --cycle: var(--cycle); box-sizing: var(--cycle, border-box); }
            #reset { --reset: initial; box-sizing: var(--reset, border-box); }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .box_sizing()
    };

    assert_eq!(style("parent"), NativeBoxSizing::BorderBox);
    assert_eq!(style("child"), NativeBoxSizing::BorderBox);
    assert_eq!(style("fallback"), NativeBoxSizing::BorderBox);
    assert_eq!(style("invalid"), NativeBoxSizing::BorderBox);
    assert_eq!(style("cycle"), NativeBoxSizing::BorderBox);
    assert_eq!(style("reset"), NativeBoxSizing::ContentBox);
}

#[test]
fn text_overflow_parser_accepts_clip_ellipsis_and_custom_properties() {
    assert_eq!(parse_text_overflow("clip"), Some(TextOverflowValue::Clip));
    assert_eq!(
        parse_text_overflow("ELLIPSIS"),
        Some(TextOverflowValue::Ellipsis)
    );
    assert_eq!(parse_text_overflow("fade"), None);
    assert_eq!(parse_text_overflow("initial"), None);
    assert_eq!(parse_text_overflow(""), None);

    let mode = parse_custom_property_name("--mode").unwrap();
    assert_eq!(
        parse_text_overflow_property("var(--mode)"),
        Some(TextOverflowDeclarationValue::CustomProperty(mode))
    );
    assert_eq!(
        parse_text_overflow_declaration("var(--mode, ellipsis)"),
        Some(LocalCascadeDeclaration::Value(
            TextOverflowDeclarationValue::CustomPropertyFallback(mode, TextOverflowValue::Ellipsis)
        ))
    );
    assert_eq!(
        parse_text_overflow_declaration("var(--mode, var(--other))"),
        None
    );
    assert_eq!(
        parse_text_overflow_declaration("initial"),
        Some(LocalCascadeDeclaration::Reset)
    );
    assert_eq!(
        parse_text_overflow_declaration("inherit"),
        Some(LocalCascadeDeclaration::Inherit)
    );
}

#[test]
fn vertical_align_parser_accepts_only_bounded_keywords() {
    assert_eq!(
        parse_vertical_align("BASELINE"),
        Some(VerticalAlignValue::Baseline)
    );
    assert_eq!(parse_vertical_align("top"), Some(VerticalAlignValue::Top));
    assert_eq!(
        parse_vertical_align("middle"),
        Some(VerticalAlignValue::Middle)
    );
    assert_eq!(
        parse_vertical_align("BOTTOM"),
        Some(VerticalAlignValue::Bottom)
    );
    assert_eq!(parse_vertical_align("text-top"), None);
    assert_eq!(parse_vertical_align("sub"), None);
    assert_eq!(parse_vertical_align("1px"), None);
    assert_eq!(parse_vertical_align("initial"), None);
}

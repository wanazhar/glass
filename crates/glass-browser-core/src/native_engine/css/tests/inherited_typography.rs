use super::*;

#[test]
fn opacity_is_cascaded_locally_without_inheriting_to_children() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { opacity: 25%; } #target { opacity: 75%; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(stylesheet.computed_for(&node).opacity(), 191);

    let document = NativeDocument::parse(
            "<style>#parent { opacity: 50%; }</style><div id='parent'><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    assert_eq!(document.computed_style_for_layout(parent).opacity(), 128);
    assert_eq!(document.computed_style_for_layout(child).opacity(), 255);
}

#[test]
fn inherited_text_align_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --align: center; --alias: var(--align); --cycle: var(--cycle); text-align: var(--align); }
            #child { text-align: var(--alias); }
            #fallback { text-align: var(--missing, justify); }
            #invalid { --bad: match-parent; text-align: var(--bad, right); }
            #cycle { text-align: var(--cycle, left); }
            #wide-initial { --wide: initial; text-align: var(--wide); }
            #wide-inherit { --wide: inherit; text-align: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_align()
    };

    assert_eq!(style("parent"), TextAlignValue::Center);
    assert_eq!(style("child"), TextAlignValue::Center);
    assert_eq!(style("fallback"), TextAlignValue::Justify);
    assert_eq!(style("invalid"), TextAlignValue::Right);
    assert_eq!(style("cycle"), TextAlignValue::Left);
    assert_eq!(style("wide-initial"), TextAlignValue::Left);
    assert_eq!(style("wide-inherit"), TextAlignValue::Center);
}

#[test]
fn text_align_is_cascaded_and_inherited_with_child_precedence() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { text-align: left; } #target { text-align: right; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target' style='text-align: center'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).text_align(),
        TextAlignValue::Center
    );

    let document = NativeDocument::parse(
            "<style>#parent { text-align: center; } #explicit { text-align: right; } #justified { text-align: justify; } #invalid { text-align: match-parent; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='justified'>Justified</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let justified = document.resolve_target("id=justified").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).text_align(),
        TextAlignValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(child).text_align(),
        TextAlignValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).text_align(),
        TextAlignValue::Right
    );
    assert_eq!(
        document.computed_style_for_layout(justified).text_align(),
        TextAlignValue::Justify
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).text_align(),
        TextAlignValue::Center
    );
}

#[test]
fn inherited_text_align_last_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --align-last: center; --alias: var(--align-last); --cycle: var(--cycle); text-align-last: var(--align-last); }
            #child { text-align-last: var(--alias); }
            #fallback { text-align-last: var(--missing, justify); }
            #invalid { --bad: match-parent; text-align-last: var(--bad, right); }
            #cycle { text-align-last: var(--cycle, left); }
            #wide-initial { --wide: initial; text-align-last: var(--wide); }
            #wide-inherit { --wide: inherit; text-align-last: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_align_last()
    };

    assert_eq!(style("parent"), TextAlignLastValue::Center);
    assert_eq!(style("child"), TextAlignLastValue::Center);
    assert_eq!(style("fallback"), TextAlignLastValue::Justify);
    assert_eq!(style("invalid"), TextAlignLastValue::Right);
    assert_eq!(style("cycle"), TextAlignLastValue::Left);
    assert_eq!(style("wide-initial"), TextAlignLastValue::Auto);
    assert_eq!(style("wide-inherit"), TextAlignLastValue::Center);
}

#[test]
fn text_align_last_is_cascaded_and_inherited_with_child_precedence() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { text-align-last: left; } #target { text-align-last: right; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target' style='text-align-last: center'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).text_align_last(),
        TextAlignLastValue::Center
    );

    let document = NativeDocument::parse(
            "<style>#parent { text-align-last: center; } #explicit { text-align-last: end; } #justify { text-align-last: justify; } #invalid { text-align-last: match-parent; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='justify'>Justify</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let justify = document.resolve_target("id=justify").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).text_align_last(),
        TextAlignLastValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(child).text_align_last(),
        TextAlignLastValue::Center
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .text_align_last(),
        TextAlignLastValue::End
    );
    assert_eq!(
        document
            .computed_style_for_layout(justify)
            .text_align_last(),
        TextAlignLastValue::Justify
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_align_last(),
        TextAlignLastValue::Center
    );
}

#[test]
fn inherited_text_justify_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --justify: inter-word; --alias: var(--justify); --cycle: var(--cycle); text-justify: var(--justify); }
            #child { text-justify: var(--alias); }
            #fallback { text-justify: var(--missing, none); }
            #invalid { --bad: inter-character; text-justify: var(--bad, auto); }
            #cycle { text-justify: var(--cycle, none); }
            #wide-initial { --wide: initial; text-justify: var(--wide); }
            #wide-inherit { --wide: inherit; text-justify: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_justify()
    };

    assert_eq!(style("parent"), TextJustifyValue::InterWord);
    assert_eq!(style("child"), TextJustifyValue::InterWord);
    assert_eq!(style("fallback"), TextJustifyValue::None);
    assert_eq!(style("invalid"), TextJustifyValue::Auto);
    assert_eq!(style("cycle"), TextJustifyValue::None);
    assert_eq!(style("wide-initial"), TextJustifyValue::Auto);
    assert_eq!(style("wide-inherit"), TextJustifyValue::InterWord);
}

#[test]
fn text_justify_is_cascaded_and_inherited_with_child_precedence() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { text-justify: none; } #target { text-justify: inter-word; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target' style='text-justify: auto'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).text_justify(),
        TextJustifyValue::Auto
    );

    let document = NativeDocument::parse(
            "<style>#parent { text-justify: none; } #explicit { text-justify: inter-word; } #auto { text-justify: auto; } #invalid { text-justify: inter-character; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='auto'>Auto</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).text_justify(),
        TextJustifyValue::None
    );
    assert_eq!(
        document.computed_style_for_layout(child).text_justify(),
        TextJustifyValue::None
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).text_justify(),
        TextJustifyValue::InterWord
    );
    assert_eq!(
        document.computed_style_for_layout(auto).text_justify(),
        TextJustifyValue::Auto
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).text_justify(),
        TextJustifyValue::None
    );
}

#[test]
fn inherited_white_space_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --white-space: pre-wrap; --alias: var(--white-space); --cycle: var(--cycle); white-space: var(--white-space); }
            #child { white-space: var(--alias); }
            #fallback { white-space: var(--missing, nowrap); }
            #invalid { --bad: pre wrap; white-space: var(--bad, pre); }
            #cycle { white-space: var(--cycle, normal); }
            #wide-initial { --wide: initial; white-space: var(--wide); }
            #wide-inherit { --wide: inherit; white-space: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .white_space()
    };

    assert_eq!(style("parent"), WhiteSpaceValue::PreWrap);
    assert_eq!(style("child"), WhiteSpaceValue::PreWrap);
    assert_eq!(style("fallback"), WhiteSpaceValue::NoWrap);
    assert_eq!(style("invalid"), WhiteSpaceValue::Pre);
    assert_eq!(style("cycle"), WhiteSpaceValue::Normal);
    assert_eq!(style("wide-initial"), WhiteSpaceValue::Normal);
    assert_eq!(style("wide-inherit"), WhiteSpaceValue::PreWrap);
}

#[test]
fn white_space_revert_layer_rolls_back_named_and_inline_candidates() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { white-space: pre; } #repeated { white-space: pre-line; } #inline { white-space: nowrap; } } @layer theme { #named { white-space: pre-wrap; } #repeated { white-space: revert-layer; } #inline { white-space: pre; } } @layer top { #repeated { white-space: revert-layer; } } #named { white-space: revert-layer; } #repeated { white-space: revert-layer; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named'>Named</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let inline = node("<div id='inline' style='white-space:REVERT-LAYER'>Inline</div>");
    let fallback = node("<div id='fallback' style='white-space:ReVeRt-LaYeR'>Fallback</div>");

    assert_eq!(
        stylesheet.computed_for(&named).white_space(),
        WhiteSpaceValue::PreWrap
    );
    assert_eq!(
        stylesheet.computed_for(&repeated).white_space(),
        WhiteSpaceValue::PreLine
    );
    assert_eq!(
        stylesheet.computed_for(&inline).white_space(),
        WhiteSpaceValue::Pre
    );
    assert_eq!(
        stylesheet.computed_for(&fallback).white_space(),
        WhiteSpaceValue::Normal
    );

    let mut diagnostics = NativeDiagnosticSink::default();
    NativeStylesheet::from_sources_with_diagnostics(
        vec!["#target { white-space: revert-layer; }".into()],
        &mut diagnostics,
    )
    .unwrap();
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "white-space"
    }));
}

#[test]
fn logical_text_align_is_inherited_and_respects_direction_overrides() {
    let document = NativeDocument::parse(
            "<style>#parent { direction: rtl; text-align: start; } #ltr { direction: ltr; } #end { text-align: end; } #physical { text-align: right; } #invalid { text-align: match-parent; }</style><div id='parent'><span id='child'>Child</span><span id='ltr'>Ltr</span><span id='end'>End</span><span id='physical'>Physical</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let ltr = document.resolve_target("id=ltr").unwrap();
    let end = document.resolve_target("id=end").unwrap();
    let physical = document.resolve_target("id=physical").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).text_align(),
        TextAlignValue::Start
    );
    assert_eq!(
        document.computed_style_for_layout(child).text_align(),
        TextAlignValue::Start
    );
    assert_eq!(
        document.computed_style_for_layout(ltr).direction(),
        DirectionValue::Ltr
    );
    assert_eq!(
        document.computed_style_for_layout(ltr).text_align(),
        TextAlignValue::Start
    );
    assert_eq!(
        document.computed_style_for_layout(end).text_align(),
        TextAlignValue::End
    );
    assert_eq!(
        document.computed_style_for_layout(physical).text_align(),
        TextAlignValue::Right
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).text_align(),
        TextAlignValue::Start
    );
}

#[test]
fn text_decoration_is_inherited_and_child_none_clears_it() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { text-decoration: none; } #target { text-decoration: line-through; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).text_decoration(),
        TextDecorationValue::new(false, false, true)
    );

    let document = NativeDocument::parse(
            "<style>#parent { text-decoration: underline; } #clear { text-decoration: none; } #over { text-decoration: overline; } #through { text-decoration: line-through; } #longhand { text-decoration-line: overline line-through; } #invalid { text-decoration: blink; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='over'>Over</span><span id='through'>Through</span><span id='longhand'>Longhand</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let over = document.resolve_target("id=over").unwrap();
    let through = document.resolve_target("id=through").unwrap();
    let longhand = document.resolve_target("id=longhand").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).text_decoration(),
        TextDecorationValue::new(true, false, false)
    );
    assert_eq!(
        document.computed_style_for_layout(child).text_decoration(),
        TextDecorationValue::new(true, false, false)
    );
    assert_eq!(
        document.computed_style_for_layout(clear).text_decoration(),
        TextDecorationValue::none()
    );
    assert_eq!(
        document.computed_style_for_layout(over).text_decoration(),
        TextDecorationValue::new(false, true, false)
    );
    assert_eq!(
        document
            .computed_style_for_layout(through)
            .text_decoration(),
        TextDecorationValue::new(false, false, true)
    );
    assert_eq!(
        document
            .computed_style_for_layout(longhand)
            .text_decoration(),
        TextDecorationValue::new(false, true, true)
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .text_decoration(),
        TextDecorationValue::new(true, false, false)
    );
}

#[test]
fn inherited_text_transform_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --transform: uppercase; --alias: var(--transform); --cycle: var(--cycle); text-transform: var(--transform); }
            #child { text-transform: var(--alias); }
            #fallback { text-transform: var(--missing, lowercase); }
            #invalid { --bad: capitalize; text-transform: var(--bad, none); }
            #cycle { text-transform: var(--cycle, none); }
            #wide-initial { --wide: initial; text-transform: var(--wide); }
            #wide-inherit { --wide: inherit; text-transform: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_transform()
    };

    assert_eq!(style("parent"), TextTransformValue::Uppercase);
    assert_eq!(style("child"), TextTransformValue::Uppercase);
    assert_eq!(style("fallback"), TextTransformValue::Lowercase);
    assert_eq!(style("invalid"), TextTransformValue::None);
    assert_eq!(style("cycle"), TextTransformValue::None);
    assert_eq!(style("wide-initial"), TextTransformValue::None);
    assert_eq!(style("wide-inherit"), TextTransformValue::Uppercase);
}

#[test]
fn text_transform_is_inherited_and_child_none_clears_it() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { text-transform: uppercase; } #target { text-transform: lowercase; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).text_transform(),
        TextTransformValue::Lowercase
    );

    let document = NativeDocument::parse(
            "<style>#parent { text-transform: uppercase; } #clear { text-transform: none; } #invalid { text-transform: capitalize; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).text_transform(),
        TextTransformValue::Uppercase
    );
    assert_eq!(
        document.computed_style_for_layout(child).text_transform(),
        TextTransformValue::Uppercase
    );
    assert_eq!(
        document.computed_style_for_layout(clear).text_transform(),
        TextTransformValue::None
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).text_transform(),
        TextTransformValue::Uppercase
    );
}

#[test]
fn font_variant_ligatures_is_inherited_and_css_wide_resets_are_bounded() {
    let normal = NativeFontVariantLigatures::default();
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-ligatures: no-common-ligatures discretionary-ligatures; } #clear { font-variant-ligatures: initial; } #inherit { font-variant-ligatures: inherit; } #invalid { font-variant-ligatures: common-ligatures no-common-ligatures; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='inherit'>Inherit</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let disabled_common = NativeFontVariantLigatures {
        common: false,
        discretionary: true,
        historical: false,
        contextual: true,
    };

    assert_eq!(
        document
            .computed_style_for_layout(parent)
            .font_variant_ligatures(),
        disabled_common
    );
    assert_eq!(
        document
            .computed_style_for_layout(child)
            .font_variant_ligatures(),
        disabled_common
    );
    assert_eq!(
        document
            .computed_style_for_layout(clear)
            .font_variant_ligatures(),
        normal
    );
    assert_eq!(
        document
            .computed_style_for_layout(inherit)
            .font_variant_ligatures(),
        disabled_common
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .font_variant_ligatures(),
        disabled_common
    );
}

#[test]
fn inherited_font_variant_ligatures_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --ligatures: no-common-ligatures discretionary-ligatures; --alias: var(--ligatures); --cycle: var(--cycle); font-variant-ligatures: var(--ligatures); }
            #child { font-variant-ligatures: var(--alias); }
            #fallback { font-variant-ligatures: var(--missing, common-ligatures); }
            #invalid { --bad: common-ligatures no-common-ligatures; font-variant-ligatures: var(--bad, none); }
            #cycle { font-variant-ligatures: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variant-ligatures: var(--wide); }
            #wide-inherit { --wide: inherit; font-variant-ligatures: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_variant_ligatures()
    };
    let parent = NativeFontVariantLigatures {
        common: false,
        discretionary: true,
        historical: false,
        contextual: true,
    };
    let none = NativeFontVariantLigatures {
        common: false,
        discretionary: false,
        historical: false,
        contextual: false,
    };

    assert_eq!(style("parent"), parent);
    assert_eq!(style("child"), parent);
    assert_eq!(style("fallback"), NativeFontVariantLigatures::default());
    assert_eq!(style("invalid"), none);
    assert_eq!(style("cycle"), NativeFontVariantLigatures::default());
    assert_eq!(style("wide-initial"), NativeFontVariantLigatures::default());
    assert_eq!(style("wide-inherit"), parent);
}

#[test]
fn font_feature_settings_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { font-feature-settings: "liga" off, "kern" on; }
            #child { font-feature-settings: "liga" 1, "liga" 0, "dlig" on; }
            #inherit { font-feature-settings: inherit; }
            #clear { font-feature-settings: initial; }
            #invalid { font-feature-settings: "liga" 65536; }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let parent = style("parent").font_feature_settings();
    let child = style("child").font_feature_settings();
    let inherit = style("inherit").font_feature_settings();
    let clear = style("clear").font_feature_settings();
    let invalid = style("invalid").font_feature_settings();

    assert_eq!(parent.count, 2);
    assert_eq!(
        parent.values[0],
        NativeFontFeature {
            tag: *b"liga",
            value: 0
        }
    );
    assert_eq!(
        parent.values[1],
        NativeFontFeature {
            tag: *b"kern",
            value: 1
        }
    );
    assert_eq!(child.count, 2);
    assert_eq!(
        child.values[0],
        NativeFontFeature {
            tag: *b"liga",
            value: 0
        }
    );
    assert_eq!(
        child.values[1],
        NativeFontFeature {
            tag: *b"dlig",
            value: 1
        }
    );
    assert_eq!(inherit, parent);
    assert_eq!(clear, NativeFontFeatureSettings::default());
    assert_eq!(invalid, parent);
}

#[test]
fn inherited_font_feature_settings_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --features: "liga" off, "kern" on; --alias: var(--features); --cycle: var(--cycle); font-feature-settings: var(--features); }
            #child { font-feature-settings: var(--alias); }
            #fallback { font-feature-settings: var(--missing, "dlig" on); }
            #invalid { --bad: "liga" 65536; font-feature-settings: var(--bad, "kern" off); }
            #cycle { font-feature-settings: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-feature-settings: var(--wide); }
            #wide-inherit { --wide: inherit; font-feature-settings: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_feature_settings()
    };
    let parent = style("parent");
    let child = style("child");
    let fallback = style("fallback");
    let invalid = style("invalid");
    let cycle = style("cycle");

    assert_eq!(parent.count, 2);
    assert_eq!(child, parent);
    assert_eq!(fallback.count, 1);
    assert_eq!(
        fallback.values[0],
        NativeFontFeature {
            tag: *b"dlig",
            value: 1
        }
    );
    assert_eq!(invalid.count, 1);
    assert_eq!(
        invalid.values[0],
        NativeFontFeature {
            tag: *b"kern",
            value: 0
        }
    );
    assert_eq!(cycle, NativeFontFeatureSettings::default());
    assert_eq!(style("wide-initial"), NativeFontFeatureSettings::default());
    assert_eq!(style("wide-inherit"), parent);
}

#[test]
fn font_kerning_is_inherited_and_low_level_kern_wins() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { font-kerning: none; }
            #child { font-kerning: normal; font-feature-settings: "kern" off; }
            #inherit { font-kerning: inherit; }
            #clear { font-kerning: initial; }
            #invalid { font-kerning: normal none; }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(style("parent").font_kerning(), NativeFontKerning::None);
    assert_eq!(style("child").font_kerning(), NativeFontKerning::Normal);
    assert_eq!(style("inherit").font_kerning(), NativeFontKerning::None);
    assert_eq!(style("clear").font_kerning(), NativeFontKerning::Auto);
    assert_eq!(style("invalid").font_kerning(), NativeFontKerning::None);
}

#[test]
fn inherited_font_kerning_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --kerning: none; --alias: var(--kerning); --cycle: var(--cycle); font-kerning: var(--kerning); }
            #child { font-kerning: var(--alias); }
            #fallback { font-kerning: var(--missing, normal); }
            #invalid { --bad: unsupported; font-kerning: var(--bad, normal); }
            #cycle { font-kerning: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-kerning: var(--wide); }
            #wide-inherit { --wide: inherit; font-kerning: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_kerning()
    };

    assert_eq!(style("parent"), NativeFontKerning::None);
    assert_eq!(style("child"), NativeFontKerning::None);
    assert_eq!(style("fallback"), NativeFontKerning::Normal);
    assert_eq!(style("invalid"), NativeFontKerning::Normal);
    assert_eq!(style("cycle"), NativeFontKerning::Normal);
    assert_eq!(style("wide-initial"), NativeFontKerning::Auto);
    assert_eq!(style("wide-inherit"), NativeFontKerning::None);
}

#[test]
fn font_optical_sizing_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { font-optical-sizing: none; }
            #child { font-optical-sizing: auto; }
            #inherit { font-optical-sizing: inherit; }
            #clear { font-optical-sizing: initial; }
            #invalid { font-optical-sizing: auto none; }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(
        style("parent").font_optical_sizing(),
        NativeFontOpticalSizing::None
    );
    assert_eq!(
        style("child").font_optical_sizing(),
        NativeFontOpticalSizing::Auto
    );
    assert_eq!(
        style("inherit").font_optical_sizing(),
        NativeFontOpticalSizing::None
    );
    assert_eq!(
        style("clear").font_optical_sizing(),
        NativeFontOpticalSizing::Auto
    );
    assert_eq!(
        style("invalid").font_optical_sizing(),
        NativeFontOpticalSizing::None
    );
}

#[test]
fn inherited_font_optical_sizing_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --sizing: none; --alias: var(--sizing); --cycle: var(--cycle); font-optical-sizing: var(--sizing); }
            #child { font-optical-sizing: var(--alias); }
            #fallback { font-optical-sizing: var(--missing, auto); }
            #invalid { --bad: unsupported; font-optical-sizing: var(--bad, auto); }
            #cycle { font-optical-sizing: var(--cycle, auto); }
            #wide-initial { --wide: initial; font-optical-sizing: var(--wide); }
            #wide-inherit { --wide: inherit; font-optical-sizing: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_optical_sizing()
    };

    assert_eq!(style("parent"), NativeFontOpticalSizing::None);
    assert_eq!(style("child"), NativeFontOpticalSizing::None);
    assert_eq!(style("fallback"), NativeFontOpticalSizing::Auto);
    assert_eq!(style("invalid"), NativeFontOpticalSizing::Auto);
    assert_eq!(style("cycle"), NativeFontOpticalSizing::Auto);
    assert_eq!(style("wide-initial"), NativeFontOpticalSizing::Auto);
    assert_eq!(style("wide-inherit"), NativeFontOpticalSizing::None);
}

#[test]
fn font_palette_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { font-palette: light; }
            #child { font-palette: dark; }
            #inherit { font-palette: inherit; }
            #clear { font-palette: initial; }
            #unset { font-palette: unset; }
            #invalid { font-palette: palette-one; }
            </style>
            <div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='unset'>Unset</span><span id='invalid'>Invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(style("parent").font_palette(), NativeFontPalette::Light);
    assert_eq!(style("child").font_palette(), NativeFontPalette::Dark);
    assert_eq!(style("inherit").font_palette(), NativeFontPalette::Light);
    assert_eq!(style("clear").font_palette(), NativeFontPalette::Normal);
    assert_eq!(style("unset").font_palette(), NativeFontPalette::Light);
    assert_eq!(style("invalid").font_palette(), NativeFontPalette::Light);
}

#[test]
fn inherited_font_palette_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --palette: --brand; --alias: var(--palette); --cycle: var(--cycle); font-palette: var(--palette); }
            #child { font-palette: var(--alias); }
            #fallback { font-palette: var(--missing, light); }
            #invalid { --bad: unsupported; font-palette: var(--bad, dark); }
            #cycle { font-palette: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-palette: var(--wide); }
            #wide-inherit { --wide: inherit; font-palette: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_palette()
    };
    let brand = parse_font_palette("--brand").unwrap();

    assert_eq!(style("parent"), brand);
    assert_eq!(style("child"), brand);
    assert_eq!(style("fallback"), NativeFontPalette::Light);
    assert_eq!(style("invalid"), NativeFontPalette::Dark);
    assert_eq!(style("cycle"), NativeFontPalette::Normal);
    assert_eq!(style("wide-initial"), NativeFontPalette::Normal);
    assert_eq!(style("wide-inherit"), brand);
}

#[test]

fn named_font_palette_values_resolve_last_valid_base_palette() {
    let mut diagnostics = NativeDiagnosticSink::default();
    let stylesheet = NativeStylesheet::from_sources_with_diagnostics(
        [r#"
                @font-palette-values --brand { base-palette: 2; }
                @font-palette-values --brand { base-palette: 1; }
                @font-palette-values --ignored { override-colors: 0 #ffffff; }
                @font-palette-values --invalid { base-palette: 65536; }
                @font-palette-values --malformed { override-colors: 0 red 0 blue; }
            "#
        .to_owned()],
        &mut diagnostics,
    )
    .unwrap();
    let brand = parse_font_palette("--brand").unwrap();
    assert_eq!(stylesheet.font_palette_values().len(), 3);
    assert_eq!(
        stylesheet.resolve_font_palette(brand),
        NativeFontPalette::Base(1)
    );
    let ignored = parse_font_palette("--ignored").unwrap();
    assert_eq!(
        stylesheet.resolve_font_palette_overrides(ignored)[0],
        Some(NativeFontPaletteOverride {
            palette_index: 0,
            color: parse_color("#ffffff").unwrap(),
        })
    );
    let malformed = parse_font_palette("--malformed").unwrap();
    assert!(
        stylesheet
            .resolve_font_palette_overrides(malformed)
            .iter()
            .all(Option::is_none)
    );
    assert_eq!(
        stylesheet.resolve_font_palette(parse_font_palette("--missing").unwrap()),
        NativeFontPalette::Normal
    );
    let (diagnostics, truncated) = diagnostics.finish();
    assert!(!truncated);
    assert!(!diagnostics.is_empty());
}

#[test]
fn font_palette_override_parser_is_bounded_and_unique() {
    let parsed = parse_font_palette_overrides("0 red 1 rgba(1, 2, 3, 0.5)")
        .expect("bounded override descriptor must parse");
    assert_eq!(
        parsed[0],
        Some(NativeFontPaletteOverride {
            palette_index: 0,
            color: NativeColor::RED,
        })
    );
    assert_eq!(
        parsed[1],
        Some(NativeFontPaletteOverride {
            palette_index: 1,
            color: NativeColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 128,
            },
        })
    );
    assert!(parse_font_palette_overrides("0 red 0 blue").is_none());
    assert!(parse_font_palette_overrides("-1 red").is_none());
    let too_many = (0..=MAX_NATIVE_FONT_PALETTE_OVERRIDES)
        .map(|index| format!("{index} red"))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(parse_font_palette_overrides(&too_many).is_none());
}
#[test]
fn font_palette_parser_accepts_custom_identifiers_only() {
    assert!(matches!(
        parse_font_palette("--brand"),
        Some(NativeFontPalette::Named(_))
    ));
    assert!(parse_font_palette("brand").is_none());
    assert!(parse_font_palette("--").is_none());
}

#[test]
fn font_variant_caps_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-caps: all-small-caps; } #child { font-variant-caps: unicase; } #inherit { font-variant-caps: inherit; } #clear { font-variant-caps: initial; } #invalid { font-variant-caps: small-caps all-small-caps; }</style><div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(
        style("parent").font_variant_caps(),
        NativeFontVariantCaps::AllSmallCaps
    );
    assert_eq!(
        style("child").font_variant_caps(),
        NativeFontVariantCaps::Unicase
    );
    assert_eq!(
        style("inherit").font_variant_caps(),
        NativeFontVariantCaps::AllSmallCaps
    );
    assert_eq!(
        style("clear").font_variant_caps(),
        NativeFontVariantCaps::Normal
    );
    assert_eq!(
        style("invalid").font_variant_caps(),
        NativeFontVariantCaps::AllSmallCaps
    );
}

#[test]
fn inherited_font_variant_caps_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --caps: all-small-caps; --alias: var(--caps); --cycle: var(--cycle); font-variant-caps: var(--caps); }
            #child { font-variant-caps: var(--alias); }
            #fallback { font-variant-caps: var(--missing, unicase); }
            #invalid { --bad: small-caps all-small-caps; font-variant-caps: var(--bad, titling-caps); }
            #cycle { font-variant-caps: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variant-caps: var(--wide); }
            #wide-inherit { --wide: inherit; font-variant-caps: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_variant_caps()
    };

    assert_eq!(style("parent"), NativeFontVariantCaps::AllSmallCaps);
    assert_eq!(style("child"), NativeFontVariantCaps::AllSmallCaps);
    assert_eq!(style("fallback"), NativeFontVariantCaps::Unicase);
    assert_eq!(style("invalid"), NativeFontVariantCaps::TitlingCaps);
    assert_eq!(style("cycle"), NativeFontVariantCaps::Normal);
    assert_eq!(style("wide-initial"), NativeFontVariantCaps::Normal);
    assert_eq!(style("wide-inherit"), NativeFontVariantCaps::AllSmallCaps);
}

#[test]
fn inherited_font_variant_position_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --position: super; --alias: var(--position); --cycle: var(--cycle); font-variant-position: var(--position); }
            #child { font-variant-position: var(--alias); }
            #fallback { font-variant-position: var(--missing, sub); }
            #invalid { --bad: sub super; font-variant-position: var(--bad, normal); }
            #cycle { font-variant-position: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variant-position: var(--wide); }
            #wide-inherit { --wide: inherit; font-variant-position: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_variant_position()
    };

    assert_eq!(style("parent"), NativeFontVariantPosition::Super);
    assert_eq!(style("child"), NativeFontVariantPosition::Super);
    assert_eq!(style("fallback"), NativeFontVariantPosition::Sub);
    assert_eq!(style("invalid"), NativeFontVariantPosition::Normal);
    assert_eq!(style("cycle"), NativeFontVariantPosition::Normal);
    assert_eq!(style("wide-initial"), NativeFontVariantPosition::Normal);
    assert_eq!(style("wide-inherit"), NativeFontVariantPosition::Super);
}

#[test]
fn font_variant_position_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-position: super; } #child { font-variant-position: sub; } #inherit { font-variant-position: inherit; } #clear { font-variant-position: initial; } #invalid { font-variant-position: sub super; }</style><div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(
        style("parent").font_variant_position(),
        NativeFontVariantPosition::Super
    );
    assert_eq!(
        style("child").font_variant_position(),
        NativeFontVariantPosition::Sub
    );
    assert_eq!(
        style("inherit").font_variant_position(),
        NativeFontVariantPosition::Super
    );
    assert_eq!(
        style("clear").font_variant_position(),
        NativeFontVariantPosition::Normal
    );
    assert_eq!(
        style("invalid").font_variant_position(),
        NativeFontVariantPosition::Super
    );
}

#[test]
fn inherited_font_variant_alternates_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --alternates: historical-forms; --alias: var(--alternates); --cycle: var(--cycle); font-variant-alternates: var(--alternates); }
            #child { font-variant-alternates: var(--alias); }
            #fallback { font-variant-alternates: var(--missing, normal); }
            #invalid { --bad: normal historical-forms; font-variant-alternates: var(--bad, historical-forms); }
            #cycle { font-variant-alternates: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variant-alternates: var(--wide); }
            #wide-inherit { --wide: inherit; font-variant-alternates: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_variant_alternates()
    };

    assert_eq!(
        style("parent"),
        NativeFontVariantAlternates::HistoricalForms
    );
    assert_eq!(style("child"), NativeFontVariantAlternates::HistoricalForms);
    assert_eq!(style("fallback"), NativeFontVariantAlternates::Normal);
    assert_eq!(
        style("invalid"),
        NativeFontVariantAlternates::HistoricalForms
    );
    assert_eq!(style("cycle"), NativeFontVariantAlternates::Normal);
    assert_eq!(style("wide-initial"), NativeFontVariantAlternates::Normal);
    assert_eq!(
        style("wide-inherit"),
        NativeFontVariantAlternates::HistoricalForms
    );
}

#[test]
fn font_variant_alternates_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-alternates: historical-forms; } #child { font-variant-alternates: normal; } #inherit { font-variant-alternates: inherit; } #clear { font-variant-alternates: initial; } #invalid { font-variant-alternates: stylistic(foo); }</style><div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(
        style("parent").font_variant_alternates(),
        NativeFontVariantAlternates::HistoricalForms
    );
    assert_eq!(
        style("child").font_variant_alternates(),
        NativeFontVariantAlternates::Normal
    );
    assert_eq!(
        style("inherit").font_variant_alternates(),
        NativeFontVariantAlternates::HistoricalForms
    );
    assert_eq!(
        style("clear").font_variant_alternates(),
        NativeFontVariantAlternates::Normal
    );
    assert_eq!(
        style("invalid").font_variant_alternates(),
        NativeFontVariantAlternates::HistoricalForms
    );
}

#[test]
fn font_language_override_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            r#"<style>#parent { font-language-override: "ENG "; } #child { font-language-override: normal; } #inherit { font-language-override: inherit; } #clear { font-language-override: initial; } #invalid { font-language-override: "EN G"; }</style><div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let english = NativeFontLanguageOverride {
        tag: Some(*b"ENG "),
    };
    assert_eq!(style("parent").font_language_override(), english);
    assert_eq!(
        style("child").font_language_override(),
        NativeFontLanguageOverride::default()
    );
    assert_eq!(style("inherit").font_language_override(), english);
    assert_eq!(
        style("clear").font_language_override(),
        NativeFontLanguageOverride::default()
    );
    assert_eq!(style("invalid").font_language_override(), english);
}

#[test]
fn font_variation_settings_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            r#"<style>#parent { font-variation-settings: "wght" 500, "wdth" 90; } #child { font-variation-settings: normal; } #inherit { font-variation-settings: inherit; } #clear { font-variation-settings: initial; } #invalid { font-variation-settings: "wght" 700 escape; } #important { font-variation-settings: "wght" 700 !important; } #important { font-variation-settings: "wght" 300; }</style><div id="parent"><span id="child">Child</span><span id="inherit">Inherit</span><span id="clear">Clear</span><span id="invalid">Invalid</span><span id="important">Important</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let parent = NativeFontVariationSettings {
        values: [
            NativeFontVariation {
                tag: *b"wght",
                value_milli: 500_000,
            },
            NativeFontVariation {
                tag: *b"wdth",
                value_milli: 90_000,
            },
            NativeFontVariation::default(),
            NativeFontVariation::default(),
            NativeFontVariation::default(),
            NativeFontVariation::default(),
            NativeFontVariation::default(),
            NativeFontVariation::default(),
        ],
        count: 2,
    };
    assert_eq!(style("parent").font_variation_settings(), parent);
    assert_eq!(
        style("child").font_variation_settings(),
        NativeFontVariationSettings::default()
    );
    assert_eq!(style("inherit").font_variation_settings(), parent);
    assert_eq!(
        style("clear").font_variation_settings(),
        NativeFontVariationSettings::default()
    );
    assert_eq!(style("invalid").font_variation_settings(), parent);
    assert_eq!(
        style("important").font_variation_settings().values()[0].value_milli,
        700_000
    );
}

#[test]
fn inherited_font_variation_settings_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --variations: "wght" 500, "wdth" 90; --alias: var(--variations); --cycle: var(--cycle); font-variation-settings: var(--variations); }
            #child { font-variation-settings: var(--alias); }
            #fallback { font-variation-settings: var(--missing, "opsz" 12); }
            #invalid { --bad: "wght" 32768.001; font-variation-settings: var(--bad, "wdth" 80); }
            #cycle { font-variation-settings: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variation-settings: var(--wide); }
            #wide-inherit { --wide: inherit; font-variation-settings: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_variation_settings()
    };
    let parent = parse_font_variation_settings(r#""wght" 500, "wdth" 90"#).unwrap();
    let fallback = parse_font_variation_settings(r#""opsz" 12"#).unwrap();
    let invalid = parse_font_variation_settings(r#""wdth" 80"#).unwrap();

    assert_eq!(style("parent"), parent);
    assert_eq!(style("child"), parent);
    assert_eq!(style("fallback"), fallback);
    assert_eq!(style("invalid"), invalid);
    assert_eq!(style("cycle"), NativeFontVariationSettings::default());
    assert_eq!(
        style("wide-initial"),
        NativeFontVariationSettings::default()
    );
    assert_eq!(style("wide-inherit"), parent);
}

#[test]
fn inherited_font_language_override_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --language: "ENG "; --alias: var(--language); --cycle: var(--cycle); font-language-override: var(--language); }
            #child { font-language-override: var(--alias); }
            #fallback { font-language-override: var(--missing, "TRK "); }
            #invalid { --bad: "EN G"; font-language-override: var(--bad, "FRA "); }
            #cycle { font-language-override: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-language-override: var(--wide); }
            #wide-inherit { --wide: inherit; font-language-override: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_language_override()
    };
    let english = NativeFontLanguageOverride {
        tag: Some(*b"ENG "),
    };
    let turkish = NativeFontLanguageOverride {
        tag: Some(*b"TRK "),
    };
    let french = NativeFontLanguageOverride {
        tag: Some(*b"FRA "),
    };

    assert_eq!(style("parent"), english);
    assert_eq!(style("child"), english);
    assert_eq!(style("fallback"), turkish);
    assert_eq!(style("invalid"), french);
    assert_eq!(style("cycle"), NativeFontLanguageOverride::default());
    assert_eq!(style("wide-initial"), NativeFontLanguageOverride::default());
    assert_eq!(style("wide-inherit"), english);
}

#[test]
fn inherited_font_variant_east_asian_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --east-asian: jis90 full-width ruby; --alias: var(--east-asian); --cycle: var(--cycle); font-variant-east-asian: var(--east-asian); }
            #child { font-variant-east-asian: var(--alias); }
            #fallback { font-variant-east-asian: var(--missing, simplified proportional-width); }
            #invalid { --bad: jis78 jis83; font-variant-east-asian: var(--bad, traditional ruby); }
            #cycle { font-variant-east-asian: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variant-east-asian: var(--wide); }
            #wide-inherit { --wide: inherit; font-variant-east-asian: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_variant_east_asian()
    };

    let parent = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Jis90,
        width: NativeFontVariantEastAsianWidth::Full,
        ruby: true,
    };
    let fallback = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Simplified,
        width: NativeFontVariantEastAsianWidth::Proportional,
        ruby: false,
    };
    let invalid_fallback = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Traditional,
        width: NativeFontVariantEastAsianWidth::Normal,
        ruby: true,
    };
    assert_eq!(style("parent"), parent);
    assert_eq!(style("child"), parent);
    assert_eq!(style("fallback"), fallback);
    assert_eq!(style("invalid"), invalid_fallback);
    assert_eq!(style("cycle"), NativeFontVariantEastAsian::default());
    assert_eq!(style("wide-initial"), NativeFontVariantEastAsian::default());
    assert_eq!(style("wide-inherit"), parent);
}

#[test]
fn font_variant_east_asian_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-east-asian: jis90 full-width ruby; } #child { font-variant-east-asian: simplified proportional-width; } #inherit { font-variant-east-asian: inherit; } #clear { font-variant-east-asian: initial; } #invalid { font-variant-east-asian: jis78 jis83; }</style><div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let parent = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Jis90,
        width: NativeFontVariantEastAsianWidth::Full,
        ruby: true,
    };
    let child = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Simplified,
        width: NativeFontVariantEastAsianWidth::Proportional,
        ruby: false,
    };
    assert_eq!(style("parent").font_variant_east_asian(), parent);
    assert_eq!(style("child").font_variant_east_asian(), child);
    assert_eq!(style("inherit").font_variant_east_asian(), parent);
    assert_eq!(
        style("clear").font_variant_east_asian(),
        NativeFontVariantEastAsian::default()
    );
    assert_eq!(style("invalid").font_variant_east_asian(), parent);
}

#[test]
fn inherited_font_variant_shorthand_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --variant: no-common-ligatures unicase super historical-forms jis83 full-width ruby oldstyle-nums tabular-nums; --alias: var(--variant); --cycle: var(--cycle); font-variant: var(--variant); }
            #child { font-variant: var(--alias); }
            #fallback { font-variant: var(--missing, common-ligatures titling-caps sub historical-forms jis83 full-width ruby oldstyle-nums); }
            #invalid { --bad: normal unknown; font-variant: var(--bad, normal); }
            #cycle { font-variant: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variant: var(--wide); }
            #wide-inherit { --wide: inherit; font-variant: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    let parent_ligatures = NativeFontVariantLigatures {
        common: false,
        discretionary: false,
        historical: false,
        contextual: true,
    };
    let parent_caps = NativeFontVariantCaps::Unicase;
    let parent_position = NativeFontVariantPosition::Super;
    let parent_alternates = NativeFontVariantAlternates::HistoricalForms;
    let parent_east_asian = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Jis83,
        width: NativeFontVariantEastAsianWidth::Full,
        ruby: true,
    };
    let parent_numeric = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Oldstyle,
        spacing: NativeFontVariantNumericSpacing::Tabular,
        fraction: NativeFontVariantNumericFraction::Normal,
        ordinal: false,
        slashed_zero: false,
    };
    for id in ["parent", "child", "wide-inherit"] {
        assert_eq!(style(id).font_variant_ligatures(), parent_ligatures);
        assert_eq!(style(id).font_variant_caps(), parent_caps);
        assert_eq!(style(id).font_variant_position(), parent_position);
        assert_eq!(style(id).font_variant_alternates(), parent_alternates);
        assert_eq!(style(id).font_variant_east_asian(), parent_east_asian);
        assert_eq!(style(id).font_variant_numeric(), parent_numeric);
    }
    assert_eq!(
        style("fallback").font_variant_ligatures(),
        NativeFontVariantLigatures::default()
    );
    assert_eq!(
        style("fallback").font_variant_caps(),
        NativeFontVariantCaps::TitlingCaps
    );
    assert_eq!(
        style("fallback").font_variant_position(),
        NativeFontVariantPosition::Sub
    );
    assert_eq!(
        style("fallback").font_variant_alternates(),
        NativeFontVariantAlternates::HistoricalForms
    );
    assert_eq!(
        style("fallback").font_variant_east_asian(),
        NativeFontVariantEastAsian {
            form: NativeFontVariantEastAsianForm::Jis83,
            width: NativeFontVariantEastAsianWidth::Full,
            ruby: true,
        }
    );
    assert_eq!(
        style("fallback").font_variant_numeric(),
        NativeFontVariantNumeric {
            figure: NativeFontVariantNumericFigure::Oldstyle,
            spacing: NativeFontVariantNumericSpacing::Normal,
            fraction: NativeFontVariantNumericFraction::Normal,
            ordinal: false,
            slashed_zero: false,
        }
    );
    for id in ["invalid", "cycle", "wide-initial"] {
        assert_eq!(
            style(id).font_variant_ligatures(),
            NativeFontVariantLigatures::default()
        );
        assert_eq!(style(id).font_variant_caps(), NativeFontVariantCaps::Normal);
        assert_eq!(
            style(id).font_variant_position(),
            NativeFontVariantPosition::Normal
        );
        assert_eq!(
            style(id).font_variant_alternates(),
            NativeFontVariantAlternates::Normal
        );
        assert_eq!(
            style(id).font_variant_east_asian(),
            NativeFontVariantEastAsian::default()
        );
        assert_eq!(
            style(id).font_variant_numeric(),
            NativeFontVariantNumeric::default()
        );
    }
}

#[test]
fn font_variant_shorthand_resets_and_expands_with_normal_cascade() {
    let document = NativeDocument::parse(
            "<style>#parent { font-variant: no-common-ligatures unicase super historical-forms jis83 full-width ruby oldstyle-nums tabular-nums; } #child { font-variant: normal; font-variant-caps: petite-caps; } #inherit { font-variant: inherit; } #clear { font-variant: initial; } #none { font-variant: none; } #invalid { font-variant: jis78 jis83; }</style><div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='none'>None</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(
        style("parent").font_variant_ligatures(),
        NativeFontVariantLigatures {
            common: false,
            discretionary: false,
            historical: false,
            contextual: true,
        }
    );
    assert_eq!(
        style("parent").font_variant_caps(),
        NativeFontVariantCaps::Unicase
    );
    assert_eq!(
        style("parent").font_variant_position(),
        NativeFontVariantPosition::Super
    );
    assert_eq!(
        style("parent").font_variant_alternates(),
        NativeFontVariantAlternates::HistoricalForms
    );
    assert_eq!(
        style("parent").font_variant_east_asian(),
        NativeFontVariantEastAsian {
            form: NativeFontVariantEastAsianForm::Jis83,
            width: NativeFontVariantEastAsianWidth::Full,
            ruby: true,
        }
    );
    assert_eq!(
        style("parent").font_variant_numeric(),
        NativeFontVariantNumeric {
            figure: NativeFontVariantNumericFigure::Oldstyle,
            spacing: NativeFontVariantNumericSpacing::Tabular,
            fraction: NativeFontVariantNumericFraction::Normal,
            ordinal: false,
            slashed_zero: false,
        }
    );
    assert_eq!(
        style("child").font_variant_caps(),
        NativeFontVariantCaps::PetiteCaps
    );
    assert_eq!(
        style("child").font_variant_ligatures(),
        NativeFontVariantLigatures::default()
    );
    assert_eq!(
        style("inherit").font_variant_east_asian(),
        style("parent").font_variant_east_asian()
    );
    assert_eq!(
        style("clear").font_variant_numeric(),
        NativeFontVariantNumeric::default()
    );
    assert_eq!(
        style("none").font_variant_ligatures(),
        NativeFontVariantLigatures {
            common: false,
            discretionary: false,
            historical: false,
            contextual: false,
        }
    );
    assert_eq!(
        style("none").font_variant_caps(),
        NativeFontVariantCaps::Normal
    );
    assert_eq!(
        style("invalid").font_variant_position(),
        NativeFontVariantPosition::Super
    );
}

#[test]
fn inherited_font_variant_numeric_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --numeric: oldstyle-nums tabular-nums stacked-fractions ordinal slashed-zero; --alias: var(--numeric); --cycle: var(--cycle); font-variant-numeric: var(--numeric); }
            #child { font-variant-numeric: var(--alias); }
            #fallback { font-variant-numeric: var(--missing, lining-nums proportional-nums diagonal-fractions); }
            #invalid { --bad: lining-nums oldstyle-nums; font-variant-numeric: var(--bad, ordinal slashed-zero); }
            #cycle { font-variant-numeric: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-variant-numeric: var(--wide); }
            #wide-inherit { --wide: inherit; font-variant-numeric: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_variant_numeric()
    };

    let parent = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Oldstyle,
        spacing: NativeFontVariantNumericSpacing::Tabular,
        fraction: NativeFontVariantNumericFraction::Stacked,
        ordinal: true,
        slashed_zero: true,
    };
    let fallback = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Lining,
        spacing: NativeFontVariantNumericSpacing::Proportional,
        fraction: NativeFontVariantNumericFraction::Diagonal,
        ordinal: false,
        slashed_zero: false,
    };
    let invalid_fallback = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Normal,
        spacing: NativeFontVariantNumericSpacing::Normal,
        fraction: NativeFontVariantNumericFraction::Normal,
        ordinal: true,
        slashed_zero: true,
    };
    assert_eq!(style("parent"), parent);
    assert_eq!(style("child"), parent);
    assert_eq!(style("fallback"), fallback);
    assert_eq!(style("invalid"), invalid_fallback);
    assert_eq!(style("cycle"), NativeFontVariantNumeric::default());
    assert_eq!(style("wide-initial"), NativeFontVariantNumeric::default());
    assert_eq!(style("wide-inherit"), parent);
}

#[test]
fn font_variant_numeric_is_inherited_and_css_wide_resets_are_bounded() {
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-numeric: oldstyle-nums tabular-nums stacked-fractions ordinal slashed-zero; } #child { font-variant-numeric: lining-nums proportional-nums diagonal-fractions; } #inherit { font-variant-numeric: inherit; } #clear { font-variant-numeric: initial; } #invalid { font-variant-numeric: lining-nums oldstyle-nums; }</style><div id='parent'><span id='child'>Child</span><span id='inherit'>Inherit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    let parent = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Oldstyle,
        spacing: NativeFontVariantNumericSpacing::Tabular,
        fraction: NativeFontVariantNumericFraction::Stacked,
        ordinal: true,
        slashed_zero: true,
    };
    let child = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Lining,
        spacing: NativeFontVariantNumericSpacing::Proportional,
        fraction: NativeFontVariantNumericFraction::Diagonal,
        ordinal: false,
        slashed_zero: false,
    };
    assert_eq!(style("parent").font_variant_numeric(), parent);
    assert_eq!(style("child").font_variant_numeric(), child);
    assert_eq!(style("inherit").font_variant_numeric(), parent);
    assert_eq!(
        style("clear").font_variant_numeric(),
        NativeFontVariantNumeric::default()
    );
    assert_eq!(style("invalid").font_variant_numeric(), parent);
}

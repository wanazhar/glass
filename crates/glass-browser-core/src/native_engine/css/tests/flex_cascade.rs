use super::*;

#[test]
fn gap_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --gap: 8px 12px; --alias: var(--gap); --cycle: var(--cycle); gap: var(--alias); }
            #child { gap: var(--alias); }
            #fallback { gap: var(--missing, 14px 16px); }
            #invalid { --bad: unsupported; gap: var(--bad, 18px 20px); }
            #cycle { gap: var(--cycle, 22px 24px); }
            #reset { --reset: initial; gap: var(--reset, 26px 28px); }
            #longhand { --row: 30px; row-gap: var(--row); column-gap: var(--missing, 32px); }
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

    assert_eq!(style("parent").row_gap(), 8);
    assert_eq!(style("parent").column_gap(), 12);
    assert_eq!(style("child").row_gap(), 8);
    assert_eq!(style("child").column_gap(), 12);
    assert_eq!(style("fallback").row_gap(), 14);
    assert_eq!(style("fallback").column_gap(), 16);
    assert_eq!(style("invalid").row_gap(), 18);
    assert_eq!(style("invalid").column_gap(), 20);
    assert_eq!(style("cycle").row_gap(), 22);
    assert_eq!(style("cycle").column_gap(), 24);
    assert_eq!(style("reset").row_gap(), 0);
    assert_eq!(style("reset").column_gap(), 0);
    assert_eq!(style("longhand").row_gap(), 30);
    assert_eq!(style("longhand").column_gap(), 32);
}

#[test]
fn gap_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { gap: 16px 20px; } #explicit { gap: 24px; } #inherit { gap: InHeRiT; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='gap: 32px'>Explicit</span><span id='inherit'>Inherit</span><span id='invalid' style='gap: -1px'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(document.computed_style_for_layout(parent).column_gap(), 20);
    assert_eq!(document.computed_style_for_layout(parent).row_gap(), 16);
    assert_eq!(document.computed_style_for_layout(child).column_gap(), 0);
    assert_eq!(document.computed_style_for_layout(child).row_gap(), 0);
    assert_eq!(
        document.computed_style_for_layout(explicit).column_gap(),
        32
    );
    assert_eq!(document.computed_style_for_layout(explicit).row_gap(), 32);
    assert_eq!(document.computed_style_for_layout(inherit).column_gap(), 20);
    assert_eq!(document.computed_style_for_layout(inherit).row_gap(), 16);
    assert_eq!(document.computed_style_for_layout(invalid).column_gap(), 0);
    assert_eq!(document.computed_style_for_layout(invalid).row_gap(), 0);
}

#[test]
fn gap_css_wide_resets_use_zero_and_preserve_invalid_later() {
    let document = NativeDocument::parse(
            "<style>#shorthand { gap: 4px 5px; gap: initial; } #unset { gap: 4px 5px; gap: unset; } #revert { gap: 4px 5px; gap: revert; } #longhand { gap: 4px 5px; row-gap: initial; column-gap: unset; } #invalid { gap: 3px 4px; gap: inherit 1px; }</style><div id='shorthand'>Shorthand</div><div id='unset'>Unset</div><div id='revert'>Revert</div><div id='longhand'>Longhand</div><div id='invalid'>Invalid</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    for id in ["shorthand", "unset", "revert", "longhand"] {
        assert_eq!(style(id).row_gap(), 0, "row gap for {id}");
        assert_eq!(style(id).column_gap(), 0, "column gap for {id}");
    }
    assert_eq!(style("invalid").row_gap(), 3);
    assert_eq!(style("invalid").column_gap(), 4);
    assert!(document.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue && diagnostic.detail == "gap"
    }));
}

#[test]
fn row_gap_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { row-gap: 16px; } #explicit { row-gap: 24px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='row-gap: 32px'>Explicit</span><span id='invalid' style='row-gap: -1px'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(document.computed_style_for_layout(parent).row_gap(), 16);
    assert_eq!(document.computed_style_for_layout(child).row_gap(), 0);
    assert_eq!(document.computed_style_for_layout(explicit).row_gap(), 32);
    assert_eq!(document.computed_style_for_layout(invalid).row_gap(), 0);
}

#[test]
fn gap_longhands_follow_declaration_order_and_inline_precedence() {
    let document = NativeDocument::parse(
            "<style>#later-row { row-gap: 2px; gap: 4px 6px; row-gap: 8px; column-gap: 10px; } #earlier-row { gap: 4px 6px; row-gap: 8px; column-gap: 10px; } .specificity { gap: 2px 3px; row-gap: 9px; } #specificity { row-gap: 7px; column-gap: 11px; }</style><div id='later-row'>Later</div><div id='earlier-row' style='row-gap: 12px; gap: 14px 16px; column-gap: 18px'>Earlier</div><div id='specificity' class='specificity'>Specificity</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let later_row = document.resolve_target("id=later-row").unwrap();
    let earlier_row = document.resolve_target("id=earlier-row").unwrap();
    let specificity = document.resolve_target("id=specificity").unwrap();

    let later_style = document.computed_style_for_layout(later_row);
    assert_eq!(later_style.column_gap(), 10);
    assert_eq!(later_style.row_gap(), 8);

    let earlier_style = document.computed_style_for_layout(earlier_row);
    assert_eq!(earlier_style.column_gap(), 18);
    assert_eq!(earlier_style.row_gap(), 14);

    let specificity_style = document.computed_style_for_layout(specificity);
    assert_eq!(specificity_style.column_gap(), 11);
    assert_eq!(specificity_style.row_gap(), 7);
}

#[test]
fn flex_sizing_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --grow: 3; --shrink: 2; --basis: 12px; --grow-alias: var(--grow); --basis-alias: var(--basis); --cycle: var(--cycle); flex-grow: var(--grow-alias); flex-shrink: var(--shrink); flex-basis: var(--basis-alias); }
            #child { flex-grow: var(--grow-alias); flex-shrink: var(--shrink); flex-basis: var(--basis-alias); }
            #fallback { flex-grow: var(--missing, 4); flex-shrink: var(--missing-shrink, 3); flex-basis: var(--missing-basis, 14px); }
            #invalid { --bad: unsupported; flex-grow: var(--bad, 5); flex-shrink: var(--bad, 4); flex-basis: var(--bad, 16px); }
            #cycle { flex-grow: var(--cycle, 6); flex-shrink: var(--cycle, 5); flex-basis: var(--cycle, 18px); }
            #reset { --reset: initial; flex-grow: var(--reset, 7); flex-shrink: var(--reset, 6); flex-basis: var(--reset, 20px); }
            #override { --grow: 9; flex-grow: var(--grow); flex-grow: 4; --shrink: 8; flex-shrink: var(--shrink); flex-shrink: 2; --basis: 22px; flex-basis: var(--basis); flex-basis: 10px; }
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

    assert_eq!(style("parent").flex_grow(), 3);
    assert_eq!(style("parent").flex_shrink(), 2);
    assert_eq!(style("parent").flex_basis(), FlexBasisValue::Length(12));
    assert_eq!(style("child").flex_grow(), 3);
    assert_eq!(style("child").flex_shrink(), 2);
    assert_eq!(style("child").flex_basis(), FlexBasisValue::Length(12));
    assert_eq!(style("fallback").flex_grow(), 4);
    assert_eq!(style("fallback").flex_shrink(), 3);
    assert_eq!(style("fallback").flex_basis(), FlexBasisValue::Length(14));
    assert_eq!(style("invalid").flex_grow(), 5);
    assert_eq!(style("invalid").flex_shrink(), 4);
    assert_eq!(style("invalid").flex_basis(), FlexBasisValue::Length(16));
    assert_eq!(style("cycle").flex_grow(), 6);
    assert_eq!(style("cycle").flex_shrink(), 5);
    assert_eq!(style("cycle").flex_basis(), FlexBasisValue::Length(18));
    assert_eq!(style("reset").flex_grow(), 0);
    assert_eq!(style("reset").flex_shrink(), 1);

    assert_eq!(style("reset").flex_basis(), FlexBasisValue::Auto);
    assert_eq!(style("override").flex_grow(), 4);
    assert_eq!(style("override").flex_shrink(), 2);
    assert_eq!(style("override").flex_basis(), FlexBasisValue::Length(10));
}
#[test]
fn flex_alignment_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --justify: space-between; --alias: var(--justify); --items: center; --self: flex-end; --content: space-around; --cycle: var(--cycle); justify-content: var(--alias); align-items: var(--items); align-self: var(--self); align-content: var(--content); }
            #child { justify-content: var(--alias); align-items: var(--items); align-self: var(--self); align-content: var(--content); }
            #fallback { justify-content: var(--missing, center); align-items: var(--missing-items, flex-end); align-self: var(--missing-self, stretch); align-content: var(--missing-content, space-evenly); }
            #invalid { --bad: unsupported; justify-content: var(--bad, flex-end); align-items: var(--bad, normal); align-self: var(--bad, center); align-content: var(--bad, space-between); }
            #cycle { justify-content: var(--cycle, space-around); align-items: var(--cycle, stretch); align-self: var(--cycle, flex-start); align-content: var(--cycle, space-evenly); }
            #reset { --reset: initial; justify-content: var(--reset, center); align-items: var(--reset, flex-end); align-self: var(--reset, stretch); align-content: var(--reset, space-between); }
            #override { --justify: center; justify-content: var(--justify); justify-content: flex-end; --items: center; align-items: var(--items); align-items: stretch; --self: flex-start; align-self: var(--self); align-self: center; --content: center; align-content: var(--content); align-content: normal; }
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

    assert_eq!(
        style("parent").justify_content(),
        JustifyContentValue::SpaceBetween
    );
    assert_eq!(style("parent").align_items(), AlignItemsValue::Center);
    assert_eq!(style("parent").align_self(), AlignSelfValue::FlexEnd);
    assert_eq!(
        style("parent").align_content(),
        AlignContentValue::SpaceAround
    );
    assert_eq!(
        style("child").justify_content(),
        JustifyContentValue::SpaceBetween
    );
    assert_eq!(style("child").align_items(), AlignItemsValue::Center);
    assert_eq!(style("child").align_self(), AlignSelfValue::FlexEnd);
    assert_eq!(
        style("child").align_content(),
        AlignContentValue::SpaceAround
    );
    assert_eq!(
        style("fallback").justify_content(),
        JustifyContentValue::Center
    );
    assert_eq!(style("fallback").align_items(), AlignItemsValue::FlexEnd);
    assert_eq!(style("fallback").align_self(), AlignSelfValue::Stretch);
    assert_eq!(
        style("fallback").align_content(),
        AlignContentValue::SpaceEvenly
    );
    assert_eq!(
        style("invalid").justify_content(),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(style("invalid").align_items(), AlignItemsValue::Normal);
    assert_eq!(style("invalid").align_self(), AlignSelfValue::Center);
    assert_eq!(
        style("invalid").align_content(),
        AlignContentValue::SpaceBetween
    );
    assert_eq!(
        style("cycle").justify_content(),
        JustifyContentValue::SpaceAround
    );
    assert_eq!(style("cycle").align_items(), AlignItemsValue::Stretch);
    assert_eq!(style("cycle").align_self(), AlignSelfValue::FlexStart);
    assert_eq!(
        style("cycle").align_content(),
        AlignContentValue::SpaceEvenly
    );
    assert_eq!(
        style("reset").justify_content(),
        JustifyContentValue::FlexStart
    );
    assert_eq!(style("reset").align_items(), AlignItemsValue::FlexStart);
    assert_eq!(style("reset").align_self(), AlignSelfValue::Auto);
    assert_eq!(style("reset").align_content(), AlignContentValue::FlexStart);
    assert_eq!(
        style("override").justify_content(),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(style("override").align_items(), AlignItemsValue::Stretch);
    assert_eq!(style("override").align_self(), AlignSelfValue::Center);
    assert_eq!(style("override").align_content(), AlignContentValue::Normal);
}

#[test]
fn flex_grow_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>.grow { flex-grow: 1; } #grow { flex-grow: 2; } #later { flex-grow: 3; flex-grow: 4; } #invalid { flex-grow: 2; flex-grow: 1.5; } #parent { flex-grow: 9; }</style><div id='parent'><span id='grow' class='grow' style='flex-grow: 3'>Grow</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let grow = document.resolve_target("id=grow").unwrap();
    let later = document.resolve_target("id=later").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let child = document.resolve_target("id=child").unwrap();

    assert_eq!(document.computed_style_for_layout(parent).flex_grow(), 9);
    assert_eq!(document.computed_style_for_layout(grow).flex_grow(), 3);
    assert_eq!(document.computed_style_for_layout(later).flex_grow(), 4);
    assert_eq!(document.computed_style_for_layout(invalid).flex_grow(), 2);
    assert_eq!(document.computed_style_for_layout(child).flex_grow(), 0);
}

#[test]
fn flex_shrink_is_cascaded_without_inheriting_and_defaults_to_one() {
    let document = NativeDocument::parse(
            "<style>.shrink { flex-shrink: 1; } #shrink { flex-shrink: 2; } #later { flex-shrink: 3; flex-shrink: 4; } #invalid { flex-shrink: 2; flex-shrink: 1.5; } #parent { flex-shrink: 0; }</style><div id='parent'><span id='shrink' class='shrink' style='flex-shrink: 3'>Shrink</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let shrink = document.resolve_target("id=shrink").unwrap();
    let later = document.resolve_target("id=later").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let child = document.resolve_target("id=child").unwrap();

    assert_eq!(document.computed_style_for_layout(parent).flex_shrink(), 0);
    assert_eq!(document.computed_style_for_layout(shrink).flex_shrink(), 3);
    assert_eq!(document.computed_style_for_layout(later).flex_shrink(), 4);
    assert_eq!(document.computed_style_for_layout(invalid).flex_shrink(), 2);
    assert_eq!(document.computed_style_for_layout(child).flex_shrink(), 1);
}

#[test]
fn flex_basis_is_cascaded_without_inheriting_and_defaults_to_auto() {
    let document = NativeDocument::parse(
            "<style>.basis { flex-basis: 12px; } #basis { flex-basis: 20px; } #later { flex-basis: 24px; flex-basis: 28px; } #invalid { flex-basis: 18px; flex-basis: 1.5px; } #parent { flex-basis: 40px; }</style><div id='parent'><span id='basis' class='basis' style='flex-basis: 16px'>Basis</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let basis = document.resolve_target("id=basis").unwrap();
    let later = document.resolve_target("id=later").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let child = document.resolve_target("id=child").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).flex_basis(),
        FlexBasisValue::Length(40)
    );
    assert_eq!(
        document.computed_style_for_layout(basis).flex_basis(),
        FlexBasisValue::Length(16)
    );
    assert_eq!(
        document.computed_style_for_layout(later).flex_basis(),
        FlexBasisValue::Length(28)
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).flex_basis(),
        FlexBasisValue::Length(18)
    );
    assert_eq!(
        document.computed_style_for_layout(child).flex_basis(),
        FlexBasisValue::Auto
    );
}

#[test]
fn flex_shorthand_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --flex: 2 3 12px; --alias: var(--flex); --cycle: var(--cycle); flex: var(--alias); }
            #child { flex: var(--alias); }
            #fallback { flex: var(--missing, 4 5 auto); }
            #invalid { --bad: unsupported; flex: var(--bad, 6 0 16px); }
            #cycle { flex: var(--cycle, 7 1 18px); }
            #reset { --reset: initial; flex: var(--reset, 8 2 20px); }
            #override { --flex: 9 8 22px; flex: var(--flex); flex-grow: 4; flex-shrink: 3; flex-basis: 10px; }
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

    assert_eq!(style("parent").flex_grow(), 2);
    assert_eq!(style("parent").flex_shrink(), 3);
    assert_eq!(style("parent").flex_basis(), FlexBasisValue::Length(12));
    assert_eq!(style("child").flex_grow(), 2);
    assert_eq!(style("child").flex_shrink(), 3);
    assert_eq!(style("child").flex_basis(), FlexBasisValue::Length(12));
    assert_eq!(style("fallback").flex_grow(), 4);
    assert_eq!(style("fallback").flex_shrink(), 5);
    assert_eq!(style("fallback").flex_basis(), FlexBasisValue::Auto);
    assert_eq!(style("invalid").flex_grow(), 6);
    assert_eq!(style("invalid").flex_shrink(), 0);
    assert_eq!(style("invalid").flex_basis(), FlexBasisValue::Length(16));
    assert_eq!(style("cycle").flex_grow(), 7);
    assert_eq!(style("cycle").flex_shrink(), 1);
    assert_eq!(style("cycle").flex_basis(), FlexBasisValue::Length(18));
    assert_eq!(style("reset").flex_grow(), 0);
    assert_eq!(style("reset").flex_shrink(), 1);
    assert_eq!(style("reset").flex_basis(), FlexBasisValue::Auto);
    assert_eq!(style("override").flex_grow(), 4);
    assert_eq!(style("override").flex_shrink(), 3);
    assert_eq!(style("override").flex_basis(), FlexBasisValue::Length(10));
}

#[test]
fn flex_shorthand_is_cascaded_without_inheriting_and_longhands_override_components() {
    let document = NativeDocument::parse(
            "<style>.preset { flex: 2 3 10px; } #preset { flex-grow: 4; } #later { flex: 1 0 8px; flex-shrink: 2; } #invalid { flex: 2 0 6px; flex: 1 2 3%; } #parent { flex: none; }</style><div id='parent'><span id='preset' class='preset' style='flex-basis: 16px'>Preset</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let preset = document.resolve_target("id=preset").unwrap();
    let later = document.resolve_target("id=later").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let child = document.resolve_target("id=child").unwrap();

    let parent_style = document.computed_style_for_layout(parent);
    assert_eq!(parent_style.flex_grow(), 0);
    assert_eq!(parent_style.flex_shrink(), 0);
    assert_eq!(parent_style.flex_basis(), FlexBasisValue::Auto);

    let preset_style = document.computed_style_for_layout(preset);
    assert_eq!(preset_style.flex_grow(), 4);
    assert_eq!(preset_style.flex_shrink(), 3);
    assert_eq!(preset_style.flex_basis(), FlexBasisValue::Length(16));

    let later_style = document.computed_style_for_layout(later);
    assert_eq!(later_style.flex_grow(), 1);
    assert_eq!(later_style.flex_shrink(), 2);
    assert_eq!(later_style.flex_basis(), FlexBasisValue::Length(8));

    let invalid_style = document.computed_style_for_layout(invalid);
    assert_eq!(invalid_style.flex_grow(), 2);
    assert_eq!(invalid_style.flex_shrink(), 0);
    assert_eq!(invalid_style.flex_basis(), FlexBasisValue::Length(6));

    let child_style = document.computed_style_for_layout(child);
    assert_eq!(child_style.flex_grow(), 0);
    assert_eq!(child_style.flex_shrink(), 1);
    assert_eq!(child_style.flex_basis(), FlexBasisValue::Auto);
}

#[test]
fn flex_flow_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --flow: column wrap; --alias: var(--flow); --cycle: var(--cycle); flex-flow: var(--alias); }
            #child { flex-flow: var(--alias); }
            #fallback { flex-flow: var(--missing, column wrap); }
            #invalid { --bad: unsupported; flex-flow: var(--bad, row-reverse wrap); }
            #cycle { flex-flow: var(--cycle, column wrap-reverse); }
            #reset { --reset: initial; flex-flow: var(--reset, column wrap); }
            #direction { --direction: row-reverse; --wrap: wrap-reverse; flex-direction: var(--direction); flex-wrap: var(--wrap); }
            #longhand { --flow: column wrap; --direction: row-reverse; flex-flow: var(--flow); flex-direction: var(--direction, row); }
            </style>
            <div id='parent'><span id='child'>Child</span></div>
            <div id='fallback'>Fallback</div>
            <div id='invalid'>Invalid</div>
            <div id='cycle'>Cycle</div>
            <div id='reset'>Reset</div>
            <div id='direction'>Direction</div>
            <div id='longhand'>Longhand</div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(style("parent").flex_direction(), FlexDirectionValue::Column);
    assert_eq!(style("parent").flex_wrap(), FlexWrapValue::Wrap);
    assert_eq!(style("child").flex_direction(), FlexDirectionValue::Column);
    assert_eq!(style("child").flex_wrap(), FlexWrapValue::Wrap);
    assert_eq!(
        style("fallback").flex_direction(),
        FlexDirectionValue::Column
    );
    assert_eq!(style("fallback").flex_wrap(), FlexWrapValue::Wrap);
    assert_eq!(
        style("invalid").flex_direction(),
        FlexDirectionValue::RowReverse
    );
    assert_eq!(style("invalid").flex_wrap(), FlexWrapValue::Wrap);
    assert_eq!(style("cycle").flex_direction(), FlexDirectionValue::Column);
    assert_eq!(style("cycle").flex_wrap(), FlexWrapValue::WrapReverse);
    assert_eq!(style("reset").flex_direction(), FlexDirectionValue::Row);
    assert_eq!(style("reset").flex_wrap(), FlexWrapValue::NoWrap);
    assert_eq!(
        style("direction").flex_direction(),
        FlexDirectionValue::RowReverse
    );
    assert_eq!(style("direction").flex_wrap(), FlexWrapValue::WrapReverse);
    assert_eq!(
        style("longhand").flex_direction(),
        FlexDirectionValue::RowReverse
    );
    assert_eq!(style("longhand").flex_wrap(), FlexWrapValue::Wrap);
}

#[test]
fn flex_flow_is_cascaded_without_inheriting_and_longhands_override_components() {
    let document = NativeDocument::parse(
            "<style>.flow { flex-flow: row-reverse wrap; } #flow { flex-wrap: nowrap; } #either { flex-flow: wrap-reverse row; } #column { flex-flow: column wrap; } #invalid { flex-flow: row-reverse wrap; flex-flow: column wrap wrap; } #parent { flex-flow: wrap-reverse; }</style><div id='parent'><span id='flow' class='flow'>Flow</span><span id='either'>Either</span><span id='column'>Column</span><span id='invalid'>Invalid</span><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let flow = document.resolve_target("id=flow").unwrap();
    let either = document.resolve_target("id=either").unwrap();
    let column = document.resolve_target("id=column").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let child = document.resolve_target("id=child").unwrap();

    let parent_style = document.computed_style_for_layout(parent);
    assert_eq!(parent_style.flex_direction(), FlexDirectionValue::Row);
    assert_eq!(parent_style.flex_wrap(), FlexWrapValue::WrapReverse);

    let flow_style = document.computed_style_for_layout(flow);
    assert_eq!(flow_style.flex_direction(), FlexDirectionValue::RowReverse);
    assert_eq!(flow_style.flex_wrap(), FlexWrapValue::NoWrap);

    let either_style = document.computed_style_for_layout(either);
    assert_eq!(either_style.flex_direction(), FlexDirectionValue::Row);
    assert_eq!(either_style.flex_wrap(), FlexWrapValue::WrapReverse);

    let column_style = document.computed_style_for_layout(column);
    assert_eq!(column_style.flex_direction(), FlexDirectionValue::Column);
    assert_eq!(column_style.flex_wrap(), FlexWrapValue::Wrap);

    let invalid_style = document.computed_style_for_layout(invalid);
    assert_eq!(
        invalid_style.flex_direction(),
        FlexDirectionValue::RowReverse
    );
    assert_eq!(invalid_style.flex_wrap(), FlexWrapValue::Wrap);

    let child_style = document.computed_style_for_layout(child);
    assert_eq!(child_style.flex_direction(), FlexDirectionValue::Row);
    assert_eq!(child_style.flex_wrap(), FlexWrapValue::NoWrap);
}

#[test]
fn justify_content_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { justify-content: space-between; } #explicit { justify-content: flex-end; } #normal { justify-content: normal; } #stretch { justify-content: stretch; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='justify-content: center'>Explicit</span><span id='normal' style='justify-content: safe center'>Normal</span><span id='stretch' style='justify-content: safe center'>Stretch</span><span id='invalid' style='justify-content: safe center'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let stretch = document.resolve_target("id=stretch").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).justify_content(),
        JustifyContentValue::SpaceBetween
    );
    assert_eq!(
        document.computed_style_for_layout(child).justify_content(),
        JustifyContentValue::FlexStart
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .justify_content(),
        JustifyContentValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(normal).justify_content(),
        JustifyContentValue::Normal
    );
    assert_eq!(
        document
            .computed_style_for_layout(stretch)
            .justify_content(),
        JustifyContentValue::Stretch
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .justify_content(),
        JustifyContentValue::FlexStart
    );
}

#[test]
fn align_items_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { align-items: center; } #explicit { align-items: flex-end; } #normal { align-items: normal; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='align-items: flex-start'>Explicit</span><span id='invalid' style='align-items: baseline'>Invalid</span><span id='stretch' style='align-items: stretch'>Stretch</span><span id='normal'>Normal</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let stretch = document.resolve_target("id=stretch").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).align_items(),
        AlignItemsValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(child).align_items(),
        AlignItemsValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).align_items(),
        AlignItemsValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).align_items(),
        AlignItemsValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(stretch).align_items(),
        AlignItemsValue::Stretch
    );
    assert_eq!(
        document.computed_style_for_layout(normal).align_items(),
        AlignItemsValue::Normal
    );
}

#[test]
fn align_self_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { align-items: center; } .item { align-self: flex-end; } #specific { align-self: flex-start; } #later { align-self: center; align-self: flex-end; } #invalid { align-self: center; align-self: baseline; } #stretch { align-self: stretch; } #normal { align-self: normal; }</style><div id='parent'><span id='child'>Child</span><span id='item' class='item'>Item</span><span id='specific' class='item'>Specific</span><span id='later'>Later</span><span id='invalid'>Invalid</span><span id='stretch'>Stretch</span><span id='normal'>Normal</span><span id='auto' style='align-self: auto'>Auto</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let item = document.resolve_target("id=item").unwrap();
    let specific = document.resolve_target("id=specific").unwrap();
    let later = document.resolve_target("id=later").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let stretch = document.resolve_target("id=stretch").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let auto = document.resolve_target("id=auto").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).align_self(),
        AlignSelfValue::Auto
    );
    assert_eq!(
        document.computed_style_for_layout(child).align_self(),
        AlignSelfValue::Auto
    );
    assert_eq!(
        document.computed_style_for_layout(item).align_self(),
        AlignSelfValue::FlexEnd
    );
    assert_eq!(
        document.computed_style_for_layout(specific).align_self(),
        AlignSelfValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(later).align_self(),
        AlignSelfValue::FlexEnd
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).align_self(),
        AlignSelfValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(stretch).align_self(),
        AlignSelfValue::Stretch
    );
    assert_eq!(
        document.computed_style_for_layout(normal).align_self(),
        AlignSelfValue::Normal
    );
    assert_eq!(
        document.computed_style_for_layout(auto).align_self(),
        AlignSelfValue::Auto
    );
}

#[test]
fn place_content_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --place: space-around flex-end; --alias: var(--place); --cycle: var(--cycle); place-content: var(--alias); }
            #child { place-content: var(--alias); }
            #fallback { place-content: var(--missing, stretch flex-end); }
            #invalid { --bad: unsupported; place-content: var(--bad, space-between center); }
            #cycle { place-content: var(--cycle, space-evenly flex-start); }
            #reset { --reset: initial; place-content: var(--reset, center flex-end); }
            #override { --place: center; place-content: var(--place); align-content: flex-end; justify-content: space-between; }
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

    assert_eq!(
        style("parent").align_content(),
        AlignContentValue::SpaceAround
    );
    assert_eq!(
        style("parent").justify_content(),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(
        style("child").align_content(),
        AlignContentValue::SpaceAround
    );
    assert_eq!(
        style("child").justify_content(),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(
        style("fallback").align_content(),
        AlignContentValue::Stretch
    );
    assert_eq!(
        style("fallback").justify_content(),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(
        style("invalid").align_content(),
        AlignContentValue::SpaceBetween
    );
    assert_eq!(
        style("invalid").justify_content(),
        JustifyContentValue::Center
    );
    assert_eq!(
        style("cycle").align_content(),
        AlignContentValue::SpaceEvenly
    );
    assert_eq!(
        style("cycle").justify_content(),
        JustifyContentValue::FlexStart
    );
    assert_eq!(style("reset").align_content(), AlignContentValue::FlexStart);
    assert_eq!(
        style("reset").justify_content(),
        JustifyContentValue::FlexStart
    );
    assert_eq!(
        style("override").align_content(),
        AlignContentValue::FlexEnd
    );
    assert_eq!(
        style("override").justify_content(),
        JustifyContentValue::SpaceBetween
    );
}

#[test]
fn place_content_is_expanded_with_component_precedence_and_no_inheritance() {
    let document = NativeDocument::parse(
            "<style>#parent { place-content: space-around flex-end; } .shared { place-content: center; } #shared { place-content: flex-start; } #longhands { place-content: stretch center; justify-content: flex-start; align-content: flex-end; } #invalid { place-content: center center; place-content: stretch stretch stretch; } #normal { place-content: normal; } #stretch { place-content: stretch; } #order { align-content: flex-end; place-content: space-between center; justify-content: flex-end; }</style><div id='parent'><span id='child'>Child</span><span id='shared' class='shared'>Shared</span><span id='longhands'>Longhands</span><span id='invalid'>Invalid</span><span id='normal'>Normal</span><span id='stretch'>Stretch</span><span id='order'>Order</span><span id='inline' style='place-content: flex-start flex-end; place-content: center; justify-content: flex-start'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let shared = document.resolve_target("id=shared").unwrap();
    let longhands = document.resolve_target("id=longhands").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let normal = document.resolve_target("id=normal").unwrap();
    let stretch = document.resolve_target("id=stretch").unwrap();
    let order = document.resolve_target("id=order").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).align_content(),
        AlignContentValue::SpaceAround
    );
    assert_eq!(
        document.computed_style_for_layout(parent).justify_content(),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(
        document.computed_style_for_layout(child).align_content(),
        AlignContentValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(child).justify_content(),
        JustifyContentValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(shared).align_content(),
        AlignContentValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(shared).justify_content(),
        JustifyContentValue::FlexStart
    );
    assert_eq!(
        document
            .computed_style_for_layout(longhands)
            .align_content(),
        AlignContentValue::FlexEnd
    );
    assert_eq!(
        document
            .computed_style_for_layout(longhands)
            .justify_content(),
        JustifyContentValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).align_content(),
        AlignContentValue::Center
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .justify_content(),
        JustifyContentValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(normal).align_content(),
        AlignContentValue::Normal
    );
    assert_eq!(
        document.computed_style_for_layout(normal).justify_content(),
        JustifyContentValue::Normal
    );
    assert_eq!(
        document.computed_style_for_layout(stretch).align_content(),
        AlignContentValue::Stretch
    );
    assert_eq!(
        document
            .computed_style_for_layout(stretch)
            .justify_content(),
        JustifyContentValue::Stretch
    );
    assert_eq!(
        document.computed_style_for_layout(order).align_content(),
        AlignContentValue::SpaceBetween
    );
    assert_eq!(
        document.computed_style_for_layout(order).justify_content(),
        JustifyContentValue::FlexEnd
    );
    assert_eq!(
        document.computed_style_for_layout(inline).align_content(),
        AlignContentValue::Center
    );
    assert_eq!(
        document.computed_style_for_layout(inline).justify_content(),
        JustifyContentValue::FlexStart
    );
}

#[test]
fn flex_direction_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { flex-direction: row-reverse; } #column { flex-direction: column; } #reverse { flex-direction: column-reverse; } #explicit { flex-direction: row; }</style><div id='parent'><span id='child'>Child</span><span id='column'>Column</span><span id='reverse'>Reverse</span><span id='explicit' style='flex-direction: row'>Explicit</span><span id='invalid' style='flex-direction: row reverse'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let column = document.resolve_target("id=column").unwrap();
    let reverse = document.resolve_target("id=reverse").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).flex_direction(),
        FlexDirectionValue::RowReverse
    );
    assert_eq!(
        document.computed_style_for_layout(child).flex_direction(),
        FlexDirectionValue::Row
    );
    assert_eq!(
        document.computed_style_for_layout(column).flex_direction(),
        FlexDirectionValue::Column
    );
    assert_eq!(
        document.computed_style_for_layout(reverse).flex_direction(),
        FlexDirectionValue::ColumnReverse
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .flex_direction(),
        FlexDirectionValue::Row
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).flex_direction(),
        FlexDirectionValue::Row
    );
}

#[test]
fn flex_direction_explicit_inherit_uses_parent_without_implicit_inheritance() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { flex-direction: InHeRiT; } #invalid { flex-direction: column; flex-direction: inherit column; } #important { flex-direction: column !important; } #important { flex-direction: inherit; }"
                .into(),
        ])
        .unwrap();
    let inherited = FlexDirectionValue::ColumnReverse;
    let computed = |element: &NativeNode| {
        stylesheet.computed_for_with_matcher(
            element,
            NativeInheritedStyle {
                flex_direction: inherited,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(element),
        )
    };

    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")).flex_direction(),
        inherited
    );
    assert_eq!(
        computed(&node("<div id='omitted'>Omitted</div>")).flex_direction(),
        FlexDirectionValue::Row
    );
    assert_eq!(
        computed(&node("<div id='invalid'>Invalid</div>")).flex_direction(),
        FlexDirectionValue::Column
    );
    assert_eq!(
        computed(&node("<div id='important'>Important</div>")).flex_direction(),
        FlexDirectionValue::Column
    );
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='root'>Root</div>"))
            .flex_direction(),
        FlexDirectionValue::Row
    );
}

#[test]
fn inherited_direction_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --direction: rtl; --alias: var(--direction); --cycle: var(--cycle); direction: var(--direction); }
            #child { direction: var(--alias); }
            #fallback { direction: var(--missing, ltr); }
            #invalid { --bad: vertical-rl; direction: var(--bad, ltr); }
            #cycle { direction: var(--cycle, rtl); }
            #wide-initial { --wide: initial; direction: var(--wide); }
            #wide-inherit { --wide: inherit; direction: var(--wide); }
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
            .direction()
    };

    assert_eq!(style("parent"), DirectionValue::Rtl);
    assert_eq!(style("child"), DirectionValue::Rtl);
    assert_eq!(style("fallback"), DirectionValue::Ltr);
    assert_eq!(style("invalid"), DirectionValue::Ltr);
    assert_eq!(style("cycle"), DirectionValue::Rtl);
    assert_eq!(style("wide-initial"), DirectionValue::Ltr);
    assert_eq!(style("wide-inherit"), DirectionValue::Rtl);
}

#[test]
fn direction_is_inherited_and_inline_cascade_can_override_it() {
    let document = NativeDocument::parse(
            "<style>#parent { direction: rtl; } #explicit { direction: rtl; } #invalid { direction: vertical-rl; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='direction: ltr'>Explicit</span><span id='invalid'>Invalid</span><span id='inline-invalid' style='direction: inherit'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let inline_invalid = document.resolve_target("id=inline-invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).direction(),
        DirectionValue::Rtl
    );
    assert_eq!(
        document.computed_style_for_layout(child).direction(),
        DirectionValue::Rtl
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).direction(),
        DirectionValue::Ltr
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).direction(),
        DirectionValue::Rtl
    );
    assert_eq!(
        document
            .computed_style_for_layout(inline_invalid)
            .direction(),
        DirectionValue::Rtl
    );
}

#[test]
fn flex_wrap_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { flex-wrap: wrap-reverse; } #explicit { flex-wrap: nowrap; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='flex-wrap: wrap'>Explicit</span><span id='invalid' style='flex-wrap: wrap reverse'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).flex_wrap(),
        FlexWrapValue::WrapReverse
    );
    assert_eq!(
        document.computed_style_for_layout(child).flex_wrap(),
        FlexWrapValue::NoWrap
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).flex_wrap(),
        FlexWrapValue::Wrap
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).flex_wrap(),
        FlexWrapValue::NoWrap
    );
}

#[test]
fn flex_wrap_explicit_inherit_uses_parent_without_implicit_inheritance() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "#inherit { flex-wrap: InHeRiT; } #invalid { flex-wrap: wrap; flex-wrap: inherit wrap; } #important { flex-wrap: wrap !important; } #important { flex-wrap: inherit; }"
                .into(),
        ])
        .unwrap();
    let inherited = FlexWrapValue::WrapReverse;
    let computed = |element: &NativeNode| {
        stylesheet.computed_for_with_matcher(
            element,
            NativeInheritedStyle {
                flex_wrap: inherited,
                ..NativeInheritedStyle::default()
            },
            |selector| selector.matches(element),
        )
    };

    assert_eq!(
        computed(&node("<div id='inherit'>Inherit</div>")).flex_wrap(),
        inherited
    );
    assert_eq!(
        computed(&node("<div id='omitted'>Omitted</div>")).flex_wrap(),
        FlexWrapValue::NoWrap
    );
    assert_eq!(
        computed(&node("<div id='invalid'>Invalid</div>")).flex_wrap(),
        FlexWrapValue::Wrap
    );
    assert_eq!(
        computed(&node("<div id='important'>Important</div>")).flex_wrap(),
        FlexWrapValue::Wrap
    );
    assert_eq!(
        stylesheet
            .computed_for(&node("<div id='root'>Root</div>"))
            .flex_wrap(),
        FlexWrapValue::NoWrap
    );
}

#[test]
fn align_content_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { align-content: normal; } #explicit { align-content: flex-end; }</style><div id='parent'><span id='child'>Child</span><span id='explicit' style='align-content: space-evenly'>Explicit</span><span id='invalid' style='align-content: safe center'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).align_content(),
        AlignContentValue::Normal
    );
    assert_eq!(
        document.computed_style_for_layout(child).align_content(),
        AlignContentValue::FlexStart
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).align_content(),
        AlignContentValue::SpaceEvenly
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).align_content(),
        AlignContentValue::FlexStart
    );
}

#[test]
fn flex_item_order_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --order: -12; --alias: var(--order); --cycle: var(--cycle); order: var(--alias); }
            #child { order: var(--alias); }
            #fallback { order: var(--missing, 8); }
            #invalid { --bad: unsupported; order: var(--bad, -4); }
            #cycle { order: var(--cycle, 6); }
            #reset { --reset: initial; order: var(--reset, 7); }
            #override { --order: -9; order: var(--order); order: -2; }
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

    assert_eq!(style("parent").flex_item_order(), NativeOrderValue(-12));
    assert_eq!(style("child").flex_item_order(), NativeOrderValue(-12));
    assert_eq!(style("fallback").flex_item_order(), NativeOrderValue(8));
    assert_eq!(style("invalid").flex_item_order(), NativeOrderValue(-4));
    assert_eq!(style("cycle").flex_item_order(), NativeOrderValue(6));
    assert_eq!(style("reset").flex_item_order(), NativeOrderValue(0));
    assert_eq!(style("override").flex_item_order(), NativeOrderValue(-2));
}

#[test]
fn flex_item_order_is_cascaded_without_inheriting_to_children() {
    let document = NativeDocument::parse(
            "<style>#parent { order: -12; } #explicit { order: 8; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='invalid' style='order: 1025'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).flex_item_order(),
        NativeOrderValue(-12)
    );
    assert_eq!(
        document.computed_style_for_layout(child).flex_item_order(),
        NativeOrderValue(0)
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .flex_item_order(),
        NativeOrderValue(8)
    );
    assert_eq!(
        document
            .computed_style_for_layout(invalid)
            .flex_item_order(),
        NativeOrderValue(0)
    );
}

#[test]
fn color_parser_accepts_bounded_forms_only() {
    assert_eq!(
        parse_color("#abc"),
        Some(NativeColor {
            red: 170,
            green: 187,
            blue: 204,
            alpha: 255,
        })
    );
    assert_eq!(
        parse_color("#10203080"),
        Some(NativeColor {
            red: 16,
            green: 32,
            blue: 48,
            alpha: 128,
        })
    );
    assert_eq!(
        parse_color("rgb(1, 2, 3)"),
        Some(NativeColor {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        })
    );
    assert_eq!(
        parse_color("rgba(1, 2, 3, 0.5)"),
        Some(NativeColor {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 128,
        })
    );
    assert_eq!(parse_color("rgb(101%, 2, 3)"), None);
}

#[test]
fn logical_box_model_edges_parse_and_project_with_priority() {
    assert_eq!(
        parse_local_box_edge_pair("2px 3px"),
        Some([
            LocalCascadeDeclaration::Value(2),
            LocalCascadeDeclaration::Value(3),
        ])
    );
    assert_eq!(
        parse_local_box_edge_pair("ReVeRt-LaYeR"),
        Some([LocalCascadeDeclaration::RevertLayer; 2])
    );
    assert_eq!(parse_local_box_edge_pair("1px 2px 3px"), None);
    assert_eq!(parse_local_box_edge_pair("revert-layer 1px"), None);
    assert_eq!(
        parse_local_margin_edge_pair("auto 4px"),
        Some([
            LocalCascadeDeclaration::Value(NativeMarginValue::Auto),
            LocalCascadeDeclaration::Value(NativeMarginValue::Length(4)),
        ])
    );

    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #ltr { direction:ltr; padding-block:1px 2px; padding-inline:3px 4px; margin-block:auto 5px; margin-inline:6px auto; } #rtl { direction:rtl; padding-block:1px 2px; padding-inline:3px 4px; margin-block:auto 5px; margin-inline:6px auto; } } @layer theme { #ltr { padding-block-start:7px !important; margin-inline-start:9px !important; } } #ltr { padding-top:8px; padding-inline-start:10px; }".into(),
        ])
        .unwrap();
    let ltr = node("<div id='ltr'>Ltr</div>");
    let rtl = node("<div id='rtl'>Rtl</div>");
    let ltr_style = stylesheet.computed_for(&ltr);
    assert_eq!(
        ltr_style.padding(),
        NativeBoxEdges {
            top: 7,
            right: 4,
            bottom: 2,
            left: 10,
        }
    );
    assert_eq!(
        ltr_style.margin(),
        NativeBoxEdges {
            top: 0,
            right: 0,
            bottom: 5,
            left: 9,
        }
    );
    assert!(ltr_style.margin_auto().top());
    assert!(ltr_style.margin_auto().right());
    assert!(!ltr_style.margin_auto().left());

    let rtl_style = stylesheet.computed_for(&rtl);
    assert_eq!(
        rtl_style.padding(),
        NativeBoxEdges {
            top: 1,
            right: 3,
            bottom: 2,
            left: 4,
        }
    );
    assert_eq!(
        rtl_style.margin(),
        NativeBoxEdges {
            top: 0,
            right: 6,
            bottom: 5,
            left: 0,
        }
    );
    assert!(rtl_style.margin_auto().top());
    assert!(rtl_style.margin_auto().left());

    let inline = node(
        "<div style='direction:rtl;padding-top:1px;padding-block-start:2px;margin-left:3px;margin-inline-start:4px'>Inline</div>",
    );
    let inline_style = NativeStylesheet::default().computed_for(&inline);
    assert_eq!(inline_style.padding().top(), 2);
    assert_eq!(inline_style.margin().right(), 4);
}

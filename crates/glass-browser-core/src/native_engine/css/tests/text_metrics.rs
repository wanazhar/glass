use super::*;

#[test]
fn font_weight_is_inherited_and_child_normal_clears_it() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { font-weight: bold; } #target { font-weight: 400; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).font_weight(),
        FontWeightValue::Normal
    );

    let document = NativeDocument::parse(
            "<style>#parent { font-weight: 700; } #clear { font-weight: normal; } #numeric { font-weight: 500; } #invalid { font-weight: 1001; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='numeric'>Numeric</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let numeric = document.resolve_target("id=numeric").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).font_weight(),
        FontWeightValue::Bold
    );
    assert_eq!(
        document.computed_style_for_layout(child).font_weight(),
        FontWeightValue::Bold
    );
    assert_eq!(
        document.computed_style_for_layout(clear).font_weight(),
        FontWeightValue::Normal
    );
    assert_eq!(
        document.computed_style_for_layout(numeric).font_weight(),
        FontWeightValue::Numeric(500)
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).font_weight(),
        FontWeightValue::Bold
    );
}

#[test]
fn relative_font_weight_is_resolved_against_the_inherited_computed_value() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent { font-weight: 550; }
            #lighter { font-weight: lighter; }
            #bolder { font-weight: bolder; }
            #inherit { font-weight: inherit; }
            #initial { font-weight: initial; }
            #invalid { font-weight: lighter bolder; }
            #edge { font-weight: 1000; }
            #edge-bolder { font-weight: bolder; }
            #edge-lighter { font-weight: lighter; }
            @layer base { #rollback { font-weight: 350; } }
            @layer theme { #rollback { font-weight: lighter; font-weight: revert-layer; } }
            </style>
            <div id='parent'>
              <span id='lighter'>Lighter</span>
              <span id='bolder'>Bolder</span>
              <span id='inherit'>Inherit</span>
              <span id='initial'>Initial</span>
              <span id='invalid'>Invalid</span>
              <span id='rollback'>Rollback</span>
            </div>
            <div id='edge'>
              <span id='edge-bolder'>Edge bolder</span>
              <span id='edge-lighter'>Edge lighter</span>
            </div>"#,
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let style = |id: &str| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };
    assert_eq!(style("parent").font_weight().numeric(), 550);
    assert_eq!(style("lighter").font_weight().numeric(), 400);
    assert_eq!(style("bolder").font_weight().numeric(), 900);
    assert_eq!(style("inherit").font_weight().numeric(), 550);
    assert_eq!(style("initial").font_weight(), FontWeightValue::Normal);
    assert_eq!(style("invalid").font_weight().numeric(), 550);
    assert_eq!(style("rollback").font_weight().numeric(), 350);
    assert_eq!(style("edge").font_weight().numeric(), 1000);
    assert_eq!(style("edge-bolder").font_weight().numeric(), 1000);
    assert_eq!(style("edge-lighter").font_weight().numeric(), 700);
}

#[test]
fn inherited_font_weight_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --weight: 550; --alias: var(--weight); --cycle: var(--cycle); font-weight: var(--weight); }
            #child { font-weight: var(--alias); }
            #fallback { font-weight: var(--missing, 700); }
            #invalid { --bad: nonsense; font-weight: var(--bad, 700); }
            #cycle { font-weight: var(--cycle, 700); }
            #relative { --relative: bolder; font-weight: var(--relative); }
            #wide-initial { --wide: initial; font-weight: var(--wide); }
            #wide-inherit { --wide: inherit; font-weight: var(--wide); }
            </style>
            <div id='parent'>
              <span id='child'>Child</span>
              <span id='fallback'>Fallback</span>
              <span id='invalid'>Invalid</span>
              <span id='cycle'>Cycle</span>
              <span id='relative'>Relative</span>
              <span id='wide-initial'>Initial</span>
              <span id='wide-inherit'>Inherit</span>
            </div>"#,
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id: &str| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .font_weight()
    };

    assert_eq!(style("parent").numeric(), 550);
    assert_eq!(style("child").numeric(), 550);
    assert_eq!(style("fallback").numeric(), 700);
    assert_eq!(style("invalid").numeric(), 700);
    assert_eq!(style("cycle").numeric(), 700);
    assert_eq!(style("relative").numeric(), 900);
    assert_eq!(style("wide-initial"), FontWeightValue::Normal);
    assert_eq!(style("wide-inherit").numeric(), 550);
}

#[test]
fn font_style_is_inherited_and_child_normal_clears_it() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { font-style: italic; } #target { font-style: normal; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).font_style(),
        FontStyleValue::Normal
    );

    let document = NativeDocument::parse(
            "<style>#parent { font-style: italic; } #clear { font-style: normal; } #invalid { font-style: oblique; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).font_style(),
        FontStyleValue::Italic
    );
    assert_eq!(
        document.computed_style_for_layout(child).font_style(),
        FontStyleValue::Italic
    );
    assert_eq!(
        document.computed_style_for_layout(clear).font_style(),
        FontStyleValue::Normal
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).font_style(),
        FontStyleValue::Italic
    );
}

#[test]
fn inherited_font_style_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --style: italic; --alias: var(--style); --cycle: var(--cycle); font-style: var(--style); }
            #child { font-style: var(--alias); }
            #fallback { font-style: var(--missing, normal); }
            #invalid { --bad: oblique; font-style: var(--bad, normal); }
            #cycle { font-style: var(--cycle, normal); }
            #wide-initial { --wide: initial; font-style: var(--wide); }
            #wide-inherit { --wide: inherit; font-style: var(--wide); }
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
            .font_style()
    };

    assert_eq!(style("parent"), FontStyleValue::Italic);
    assert_eq!(style("child"), FontStyleValue::Italic);
    assert_eq!(style("fallback"), FontStyleValue::Normal);
    assert_eq!(style("invalid"), FontStyleValue::Normal);
    assert_eq!(style("cycle"), FontStyleValue::Normal);
    assert_eq!(style("wide-initial"), FontStyleValue::Normal);
    assert_eq!(style("wide-inherit"), FontStyleValue::Italic);
}

#[test]
fn font_stretch_is_inherited_and_css_wide_resets_are_bounded() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { font-stretch: condensed; } #target { font-stretch: 62.5%; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).font_stretch(),
        NativeFontStretchRange { min: 625, max: 625 }
    );

    let document = NativeDocument::parse(
            "<style>#parent { font-stretch: 75%; } #clear { font-stretch: initial; } #inherit { font-stretch: inherit; } #invalid { font-stretch: 62.5% 125%; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='inherit'>Inherit</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let condensed = NativeFontStretchRange { min: 750, max: 750 };
    let normal = NativeFontStretchRange::default();

    assert_eq!(
        document.computed_style_for_layout(parent).font_stretch(),
        condensed
    );
    assert_eq!(
        document.computed_style_for_layout(child).font_stretch(),
        condensed
    );
    assert_eq!(
        document.computed_style_for_layout(clear).font_stretch(),
        normal
    );
    assert_eq!(
        document.computed_style_for_layout(inherit).font_stretch(),
        condensed
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).font_stretch(),
        condensed
    );
}

#[test]
fn inherited_font_stretch_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --stretch: 75%; --alias: var(--stretch); --cycle: var(--cycle); font-stretch: var(--stretch); }
            #child { font-stretch: var(--alias); }
            #fallback { font-stretch: var(--missing, 62.5%); }
            #invalid { --bad: 201%; font-stretch: var(--bad, condensed); }
            #cycle { font-stretch: var(--cycle, 125%); }
            #wide-initial { --wide: initial; font-stretch: var(--wide); }
            #wide-inherit { --wide: inherit; font-stretch: var(--wide); }
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
            .font_stretch()
    };
    let parent = NativeFontStretchRange { min: 750, max: 750 };

    assert_eq!(style("parent"), parent);
    assert_eq!(style("child"), parent);
    assert_eq!(
        style("fallback"),
        NativeFontStretchRange { min: 625, max: 625 }
    );
    assert_eq!(style("invalid"), parent);
    assert_eq!(
        style("cycle"),
        NativeFontStretchRange {
            min: 1250,
            max: 1250
        }
    );
    assert_eq!(style("wide-initial"), NativeFontStretchRange::default());
    assert_eq!(style("wide-inherit"), parent);
}

#[test]
fn font_family_and_size_are_inherited_with_css_wide_resets() {
    let document = NativeDocument::parse(
            "<style>html { font-size: 24px; } #parent { font-family: sans-serif; font-size: 24px; } #clear { font-family: initial; font-size: initial; } #inherit { font-family: inherit; font-size: inherit; } #invalid { font-family: sans-serif,; font-size: 400px; } #absolute { font-size: 9pt; } #relative { font-size: 1.5em; } #percent { font-size: 125%; } #relative-invalid { font-size: 2000%; } #nested { font-size: 32px; } #rem { font-size: 1.5rem; } #rem-invalid { font-size: 20rem; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='inherit'>Inherit</span><span id='invalid'>Invalid</span><span id='absolute'>Absolute</span><span id='relative'>Relative</span><span id='percent'>Percent</span><span id='relative-invalid'>Relative invalid</span><span id='nested'><span id='rem'>Rem</span><span id='rem-invalid'>Rem invalid</span></span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let inherit = document.resolve_target("id=inherit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let absolute = document.resolve_target("id=absolute").unwrap();
    let relative = document.resolve_target("id=relative").unwrap();
    let percent = document.resolve_target("id=percent").unwrap();
    let relative_invalid = document.resolve_target("id=relative-invalid").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let rem = document.resolve_target("id=rem").unwrap();
    let rem_invalid = document.resolve_target("id=rem-invalid").unwrap();
    let sans = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));

    assert_eq!(
        document.computed_style_for_layout(parent).font_family(),
        sans
    );
    assert_eq!(document.computed_style_for_layout(parent).font_size(), 24);
    assert_eq!(
        document.computed_style_for_layout(child).font_family(),
        sans
    );
    assert_eq!(document.computed_style_for_layout(child).font_size(), 24);
    assert_eq!(
        document.computed_style_for_layout(clear).font_family(),
        NativeFontFamilyList::default()
    );
    assert_eq!(document.computed_style_for_layout(clear).font_size(), 16);
    assert_eq!(
        document.computed_style_for_layout(inherit).font_family(),
        sans
    );
    assert_eq!(document.computed_style_for_layout(inherit).font_size(), 24);
    assert_eq!(
        document.computed_style_for_layout(invalid).font_family(),
        sans
    );
    assert_eq!(document.computed_style_for_layout(invalid).font_size(), 24);
    assert_eq!(document.computed_style_for_layout(absolute).font_size(), 12);
    assert_eq!(document.computed_style_for_layout(relative).font_size(), 36);
    assert_eq!(document.computed_style_for_layout(percent).font_size(), 30);
    assert_eq!(
        document
            .computed_style_for_layout(relative_invalid)
            .font_size(),
        24
    );
    assert_eq!(document.computed_style_for_layout(nested).font_size(), 32);
    assert_eq!(document.computed_style_for_layout(rem).font_size(), 36);
    assert_eq!(
        document.computed_style_for_layout(rem_invalid).font_size(),
        32
    );
}

#[test]
fn inherited_font_family_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --family: "Missing Face", sans-serif; --alias: var(--family); --cycle: var(--cycle); font-family: var(--family); }
            #child { font-family: var(--alias); }
            #fallback { font-family: var(--missing, "Fallback", serif); }
            #invalid { --bad: sans-serif,; font-family: var(--bad, monospace); }
            #cycle { font-family: var(--cycle, cursive); }
            #wide-initial { --wide: initial; font-family: var(--wide); }
            #wide-inherit { --wide: inherit; font-family: var(--wide); }
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
            .font_family()
    };
    let parent = parse_font_family("\"Missing Face\", sans-serif").unwrap();
    let fallback = parse_font_family("\"Fallback\", serif").unwrap();
    let invalid = parse_font_family("monospace").unwrap();
    let cycle = parse_font_family("cursive").unwrap();

    assert_eq!(style("parent"), parent);
    assert_eq!(style("child"), parent);
    assert_eq!(style("fallback"), fallback);
    assert_eq!(style("invalid"), invalid);
    assert_eq!(style("cycle"), cycle);
    assert_eq!(style("wide-initial"), NativeFontFamilyList::default());
    assert_eq!(style("wide-inherit"), parent);
}

#[test]
fn fractional_pixel_font_sizes_round_and_preserve_bounds() {
    let document = NativeDocument::parse(
            "<style>#parent { font-size: 20px; } #half-up { font-size: 16.5px; } #down { font-size: 16.499px; } #tiny { font-size: 0.5px; } #too-large { font-size: 256.5px; }</style><div id='parent'><span id='half-up'>Half up</span><span id='down'>Down</span><span id='tiny'>Tiny</span><span id='too-large'>Too large</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let half_up = document.resolve_target("id=half-up").unwrap();
    let down = document.resolve_target("id=down").unwrap();
    let tiny = document.resolve_target("id=tiny").unwrap();
    let too_large = document.resolve_target("id=too-large").unwrap();

    assert_eq!(document.computed_style_for_layout(half_up).font_size(), 17);
    assert_eq!(document.computed_style_for_layout(down).font_size(), 16);
    assert_eq!(document.computed_style_for_layout(tiny).font_size(), 1);
    assert_eq!(
        document.computed_style_for_layout(too_large).font_size(),
        20
    );
}

#[test]
fn viewport_font_sizes_use_configured_dimensions_and_bounds() {
    let mut document = NativeDocument::parse(
            "<style>#parent { font-size: 20px; } #width { font-size: 2vw; } #height { font-size: 2vh; } #minimum { font-size: 2vmin; } #maximum { font-size: 2vmax; } #too-large { font-size: 30vw; }</style><div id='parent'><span id='width'>Width</span><span id='height'>Height</span><span id='minimum'>Minimum</span><span id='maximum'>Maximum</span><span id='too-large'>Too large</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    document
        .set_viewport(Viewport {
            width: 1_000,
            height: 800,
            device_scale_factor_milli: 1_000,
        })
        .unwrap();
    let width = document.resolve_target("id=width").unwrap();
    let height = document.resolve_target("id=height").unwrap();
    let minimum = document.resolve_target("id=minimum").unwrap();
    let maximum = document.resolve_target("id=maximum").unwrap();
    let too_large = document.resolve_target("id=too-large").unwrap();

    assert_eq!(document.computed_style_for_layout(width).font_size(), 20);
    assert_eq!(document.computed_style_for_layout(height).font_size(), 16);
    assert_eq!(document.computed_style_for_layout(minimum).font_size(), 16);
    assert_eq!(document.computed_style_for_layout(maximum).font_size(), 20);
    assert_eq!(
        document.computed_style_for_layout(too_large).font_size(),
        20
    );

    document
        .set_viewport(Viewport {
            width: 500,
            height: 400,
            device_scale_factor_milli: 1_000,
        })
        .unwrap();
    assert_eq!(document.computed_style_for_layout(width).font_size(), 10);
    assert_eq!(document.computed_style_for_layout(height).font_size(), 8);
    assert_eq!(document.computed_style_for_layout(minimum).font_size(), 8);
    assert_eq!(document.computed_style_for_layout(maximum).font_size(), 10);
    assert_eq!(
        document.computed_style_for_layout(too_large).font_size(),
        150
    );
}

#[test]
fn additive_calc_font_sizes_resolve_against_style_context() {
    let mut document = NativeDocument::parse(
            "<style>#parent { font-size: 24px; } #sum { font-size: calc(1em + 2px); } #subtract { font-size: calc(100% - 2px); } #viewport { font-size: calc(1vw + 1vh); } #negative { font-size: calc(1px - 2em); } #too-large { font-size: calc(256px + 1px); }</style><div id='parent'><span id='sum'>Sum</span><span id='subtract'>Subtract</span><span id='viewport'>Viewport</span><span id='negative'>Negative</span><span id='too-large'>Too large</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    document
        .set_viewport(Viewport {
            width: 1_000,
            height: 800,
            device_scale_factor_milli: 1_000,
        })
        .unwrap();
    let sum = document.resolve_target("id=sum").unwrap();
    let subtract = document.resolve_target("id=subtract").unwrap();
    let viewport = document.resolve_target("id=viewport").unwrap();
    let negative = document.resolve_target("id=negative").unwrap();
    let too_large = document.resolve_target("id=too-large").unwrap();

    assert_eq!(document.computed_style_for_layout(sum).font_size(), 26);
    assert_eq!(document.computed_style_for_layout(subtract).font_size(), 22);
    assert_eq!(document.computed_style_for_layout(viewport).font_size(), 18);
    assert_eq!(document.computed_style_for_layout(negative).font_size(), 24);
    assert_eq!(
        document.computed_style_for_layout(too_large).font_size(),
        24
    );
}

#[test]
fn product_and_nested_calc_font_sizes_resolve() {
    let document = NativeDocument::parse(
            "<style>#parent { font-size: 24px; } #times { font-size: calc(1em * 2); } #reverse { font-size: calc(2 * 1em); } #divide { font-size: calc(24px / 2); } #nested { font-size: calc(calc(1em + 2px) * 2); } #chain { font-size: calc(1em * 2 * 3); } #reverse-chain { font-size: calc(2 * 1em * 3); } #divide-chain { font-size: calc(24px / 2 / 2); } #nested-chain { font-size: calc(calc(1em * 2) * 3); } #invalid { font-size: calc(1px / 0); } #invalid-chain { font-size: calc(1px * 2px * 2); }</style><div id='parent'><span id='times'>Times</span><span id='reverse'>Reverse</span><span id='divide'>Divide</span><span id='nested'>Nested</span><span id='chain'>Chain</span><span id='reverse-chain'>Reverse chain</span><span id='divide-chain'>Divide chain</span><span id='nested-chain'>Nested chain</span><span id='invalid'>Invalid</span><span id='invalid-chain'>Invalid chain</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let times = document.resolve_target("id=times").unwrap();
    let reverse = document.resolve_target("id=reverse").unwrap();
    let divide = document.resolve_target("id=divide").unwrap();
    let nested = document.resolve_target("id=nested").unwrap();
    let chain = document.resolve_target("id=chain").unwrap();
    let reverse_chain = document.resolve_target("id=reverse-chain").unwrap();
    let divide_chain = document.resolve_target("id=divide-chain").unwrap();
    let nested_chain = document.resolve_target("id=nested-chain").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let invalid_chain = document.resolve_target("id=invalid-chain").unwrap();

    assert_eq!(document.computed_style_for_layout(times).font_size(), 48);
    assert_eq!(document.computed_style_for_layout(reverse).font_size(), 48);
    assert_eq!(document.computed_style_for_layout(divide).font_size(), 12);
    assert_eq!(document.computed_style_for_layout(nested).font_size(), 52);
    assert_eq!(document.computed_style_for_layout(chain).font_size(), 144);
    assert_eq!(
        document
            .computed_style_for_layout(reverse_chain)
            .font_size(),
        144
    );
    assert_eq!(
        document.computed_style_for_layout(divide_chain).font_size(),
        6
    );
    assert_eq!(
        document.computed_style_for_layout(nested_chain).font_size(),
        144
    );
    assert_eq!(document.computed_style_for_layout(invalid).font_size(), 24);
    assert_eq!(
        document
            .computed_style_for_layout(invalid_chain)
            .font_size(),
        24
    );
}

#[test]
fn extra_precision_font_sizes_round_into_native_model() {
    let document = NativeDocument::parse(
            "<style>#parent { font-size: 24px; } #direct { font-size: 16.0005px; } #relative { font-size: 1.0005em; } #tiny { font-size: 0.0005px; } #too-precise { font-size: 1.0000001em; }</style><div id='parent'><span id='direct'>Direct</span><span id='relative'>Relative</span><span id='tiny'>Tiny</span><span id='too-precise'>Too precise</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let direct = document.resolve_target("id=direct").unwrap();
    let relative = document.resolve_target("id=relative").unwrap();
    let tiny = document.resolve_target("id=tiny").unwrap();
    let too_precise = document.resolve_target("id=too-precise").unwrap();

    assert_eq!(document.computed_style_for_layout(direct).font_size(), 16);
    assert_eq!(document.computed_style_for_layout(relative).font_size(), 24);
    assert_eq!(document.computed_style_for_layout(tiny).font_size(), 24);
    assert_eq!(
        document.computed_style_for_layout(too_precise).font_size(),
        24
    );
}

#[test]
fn inherited_font_size_custom_properties_resolve_with_inline_override() {
    let document = NativeDocument::parse(
            "<style>#parent { --size: 1.5em; --nested: var(--size); --broken: var(--missing); --bad: nonsense; font-size: 24px; } #child { font-size: var(--size); } #invalid { font-size: var(--missing); } #fallback { font-size: var(--missing, 18px); } #invalid-fallback { font-size: var(--bad, 20px); } #calc-fallback { font-size: var(--missing, calc(1em + 2px)); } #composite { font-size: calc(var(--size) + 2px); } #composite-fallback { font-size: calc(var(--missing, 18px) + 2px); } #composite-invalid { font-size: calc(var(--bad, 20px) + 2px); } #composite-nested { font-size: calc(var(--nested) + 2px); } #composite-unresolved-fallback { font-size: calc(var(--broken, 20px) + 2px); }</style><div id='parent'><span id='child'>Child</span><span id='invalid'>Invalid</span><span id='fallback'>Fallback</span><span id='invalid-fallback'>Invalid fallback</span><span id='calc-fallback'>Calc fallback</span><span id='composite'>Composite</span><span id='composite-fallback'>Composite fallback</span><span id='composite-invalid'>Composite invalid</span><span id='composite-nested'>Composite nested</span><span id='composite-unresolved-fallback'>Composite unresolved fallback</span><span id='inline' style='--size: 2em; font-size: var(--size)'>Inline</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid_fallback = document.resolve_target("id=invalid-fallback").unwrap();
    let calc_fallback = document.resolve_target("id=calc-fallback").unwrap();
    let composite = document.resolve_target("id=composite").unwrap();
    let composite_fallback = document.resolve_target("id=composite-fallback").unwrap();
    let composite_invalid = document.resolve_target("id=composite-invalid").unwrap();
    let composite_nested = document.resolve_target("id=composite-nested").unwrap();
    let composite_unresolved_fallback = document
        .resolve_target("id=composite-unresolved-fallback")
        .unwrap();
    let inline = document.resolve_target("id=inline").unwrap();

    assert_eq!(document.computed_style_for_layout(parent).font_size(), 24);
    assert_eq!(document.computed_style_for_layout(child).font_size(), 36);
    assert_eq!(document.computed_style_for_layout(invalid).font_size(), 24);
    assert_eq!(document.computed_style_for_layout(fallback).font_size(), 18);
    assert_eq!(
        document
            .computed_style_for_layout(invalid_fallback)
            .font_size(),
        20
    );
    assert_eq!(
        document
            .computed_style_for_layout(calc_fallback)
            .font_size(),
        26
    );
    assert_eq!(
        document.computed_style_for_layout(composite).font_size(),
        38
    );
    assert_eq!(
        document
            .computed_style_for_layout(composite_fallback)
            .font_size(),
        20
    );
    assert_eq!(
        document
            .computed_style_for_layout(composite_invalid)
            .font_size(),
        22
    );
    assert_eq!(
        document
            .computed_style_for_layout(composite_nested)
            .font_size(),
        38
    );
    assert_eq!(
        document
            .computed_style_for_layout(composite_unresolved_fallback)
            .font_size(),
        22
    );
    assert_eq!(document.computed_style_for_layout(inline).font_size(), 48);
}

#[test]
fn word_break_is_inherited_and_child_normal_clears_it() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { word-break: break-all; } #target { word-break: normal; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).word_break(),
        WordBreakValue::Normal
    );

    let document = NativeDocument::parse(
            "<style>#parent { word-break: break-all; } #clear { word-break: normal; } #invalid { word-break: keep-all; }</style><div id='parent'><span id='child'>Child</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).word_break(),
        WordBreakValue::BreakAll
    );
    assert_eq!(
        document.computed_style_for_layout(child).word_break(),
        WordBreakValue::BreakAll
    );
    assert_eq!(
        document.computed_style_for_layout(clear).word_break(),
        WordBreakValue::Normal
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).word_break(),
        WordBreakValue::BreakAll
    );
}

#[test]
fn inherited_word_break_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --word-break: break-all; --alias: var(--word-break); --cycle: var(--cycle); word-break: var(--word-break); }
            #child { word-break: var(--alias); }
            #fallback { word-break: var(--missing, normal); }
            #invalid { --bad: keep-all; word-break: var(--bad, break-all); }
            #cycle { word-break: var(--cycle, normal); }
            #wide-initial { --wide: initial; word-break: var(--wide); }
            #wide-inherit { --wide: inherit; word-break: var(--wide); }
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
            .word_break()
    };

    assert_eq!(style("parent"), WordBreakValue::BreakAll);
    assert_eq!(style("child"), WordBreakValue::BreakAll);
    assert_eq!(style("fallback"), WordBreakValue::Normal);
    assert_eq!(style("invalid"), WordBreakValue::BreakAll);
    assert_eq!(style("cycle"), WordBreakValue::Normal);
    assert_eq!(style("wide-initial"), WordBreakValue::Normal);
    assert_eq!(style("wide-inherit"), WordBreakValue::BreakAll);
}

#[test]
fn text_overflow_is_local_and_defaults_to_clip() {
    let document = NativeDocument::parse(
            "<style>#parent { text-overflow: ellipsis; } #child { text-overflow: clip; }</style><div id='parent'><span id='child'>Child</span><span id='other'>Other</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let other = document.resolve_target("id=other").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).text_overflow(),
        TextOverflowValue::Ellipsis
    );
    assert_eq!(
        document.computed_style_for_layout(child).text_overflow(),
        TextOverflowValue::Clip
    );
    assert_eq!(
        document.computed_style_for_layout(other).text_overflow(),
        TextOverflowValue::Clip
    );

    let defaults = NativeDocument::parse(
        "<div id='default'>Default</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let default_node = defaults.resolve_target("id=default").unwrap();
    assert_eq!(
        defaults
            .computed_style_for_layout(default_node)
            .text_overflow(),
        TextOverflowValue::Clip
    );
}

#[test]
fn text_overflow_explicit_inherit_uses_parent_and_respects_cascade() {
    let document = NativeDocument::parse(
            "<style>#parent { text-overflow: ellipsis; } #inherit { text-overflow: InHeRiT; } #ordered { text-overflow: clip; text-overflow: inherit; } #invalid { text-overflow: ellipsis; text-overflow: inherit ellipsis; } #important { text-overflow: ellipsis !important; } #important { text-overflow: inherit; } #important-inherit { text-overflow: clip; } #important-inherit { text-overflow: inherit !important; } #reset { text-overflow: inherit; text-overflow: initial; } #unset { text-overflow: inherit; text-overflow: unset; } #revert { text-overflow: inherit; text-overflow: revert; }</style><div id='parent'><span id='inherit'>Inherit</span><span id='ordered'>Ordered</span><span id='invalid'>Invalid</span><span id='important'>Important</span><span id='important-inherit'>Important inherit</span><span id='reset'>Reset</span><span id='unset'>Unset</span><span id='revert'>Revert</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id| {
        document
            .computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
            .text_overflow()
    };

    assert_eq!(style("parent"), TextOverflowValue::Ellipsis);
    assert_eq!(style("inherit"), TextOverflowValue::Ellipsis);
    assert_eq!(style("ordered"), TextOverflowValue::Ellipsis);
    assert_eq!(style("invalid"), TextOverflowValue::Ellipsis);
    assert_eq!(style("important"), TextOverflowValue::Ellipsis);
    assert_eq!(style("important-inherit"), TextOverflowValue::Ellipsis);
    for id in ["reset", "unset", "revert"] {
        assert_eq!(style(id), TextOverflowValue::Clip, "text-overflow for {id}");
    }
}

#[test]
fn text_overflow_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
        r#"<style>
            #parent { --overflow: ellipsis; --cycle: var(--cycle); text-overflow: var(--overflow); }
            #child { text-overflow: var(--overflow); }
            #fallback { text-overflow: var(--missing, clip); }
            #invalid { --bad: fade; text-overflow: var(--bad, ellipsis); }
            #cycle { text-overflow: var(--cycle, ellipsis); }
            #wide-initial { --wide: initial; text-overflow: var(--wide); }
            #wide-inherit { --wide: inherit; text-overflow: var(--wide); }
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
            .text_overflow()
    };

    assert_eq!(style("parent"), TextOverflowValue::Ellipsis);
    assert_eq!(style("child"), TextOverflowValue::Ellipsis);
    assert_eq!(style("fallback"), TextOverflowValue::Clip);
    assert_eq!(style("invalid"), TextOverflowValue::Ellipsis);
    assert_eq!(style("cycle"), TextOverflowValue::Ellipsis);
    assert_eq!(style("wide-initial"), TextOverflowValue::Clip);
    assert_eq!(style("wide-inherit"), TextOverflowValue::Ellipsis);
}

#[test]
fn vertical_align_is_inherited_and_child_override_replaces_it() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { vertical-align: top; } #target { vertical-align: bottom; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(
        stylesheet.computed_for(&node).vertical_align(),
        VerticalAlignValue::Bottom
    );

    let document = NativeDocument::parse(
            "<style>#parent { vertical-align: middle; } #explicit { vertical-align: bottom; } #clear { vertical-align: baseline; } #invalid { vertical-align: sub; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='clear'>Clear</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let clear = document.resolve_target("id=clear").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).vertical_align(),
        VerticalAlignValue::Middle
    );
    assert_eq!(
        document.computed_style_for_layout(child).vertical_align(),
        VerticalAlignValue::Middle
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .vertical_align(),
        VerticalAlignValue::Bottom
    );
    assert_eq!(
        document.computed_style_for_layout(clear).vertical_align(),
        VerticalAlignValue::Baseline
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).vertical_align(),
        VerticalAlignValue::Middle
    );
}

#[test]
fn inherited_vertical_align_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --vertical-align: middle; --alias: var(--vertical-align); --cycle: var(--cycle); vertical-align: var(--vertical-align); }
            #child { vertical-align: var(--alias); }
            #fallback { vertical-align: var(--missing, top); }
            #invalid { --bad: sub; vertical-align: var(--bad, bottom); }
            #cycle { vertical-align: var(--cycle, baseline); }
            #wide-initial { --wide: initial; vertical-align: var(--wide); }
            #wide-inherit { --wide: inherit; vertical-align: var(--wide); }
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
            .vertical_align()
    };

    assert_eq!(style("parent"), VerticalAlignValue::Middle);
    assert_eq!(style("child"), VerticalAlignValue::Middle);
    assert_eq!(style("fallback"), VerticalAlignValue::Top);
    assert_eq!(style("invalid"), VerticalAlignValue::Bottom);
    assert_eq!(style("cycle"), VerticalAlignValue::Baseline);
    assert_eq!(style("wide-initial"), VerticalAlignValue::Baseline);
    assert_eq!(style("wide-inherit"), VerticalAlignValue::Middle);
}

#[test]
fn text_indent_is_local_and_does_not_inherit() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        "div { text-indent: 16px; } #target { text-indent: 24px; }".into(),
    ])
    .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(stylesheet.computed_for(&node).text_indent(), 24);

    let document = NativeDocument::parse(
            "<style>#parent { text-indent: 16px; } #explicit { text-indent: 24px; } #invalid { text-indent: -1px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(document.computed_style_for_layout(parent).text_indent(), 16);
    assert_eq!(document.computed_style_for_layout(child).text_indent(), 0);
    assert_eq!(
        document.computed_style_for_layout(explicit).text_indent(),
        24
    );
    assert_eq!(document.computed_style_for_layout(invalid).text_indent(), 0);
}

#[test]
fn text_indent_explicit_inherit_uses_parent_and_respects_cascade() {
    let document = NativeDocument::parse(
            "<style>#parent { text-indent: 24px; } #inherit { text-indent: InHeRiT; } #omitted { text-indent: 8px; text-indent: inherit; } #invalid { text-indent: 16px; text-indent: inherit 1px; } #important { text-indent: 8px !important; } #important { text-indent: inherit; } #important-inherit { text-indent: 8px; } #important-inherit { text-indent: inherit !important; } #reset { text-indent: inherit; text-indent: initial; } #revert { text-indent: inherit; text-indent: revert; }</style><div id='parent'><span id='inherit'>Inherit</span><span id='omitted'>Omitted</span><span id='invalid'>Invalid</span><span id='important'>Important</span><span id='important-inherit'>Important inherit</span><span id='reset'>Reset</span><span id='revert'>Revert</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let style = |id| {
        document.computed_style_for_layout(document.resolve_target(&format!("id={id}")).unwrap())
    };

    assert_eq!(style("parent").text_indent(), 24);
    assert_eq!(style("inherit").text_indent(), 24);
    assert_eq!(style("omitted").text_indent(), 24);
    assert_eq!(style("invalid").text_indent(), 16);
    assert_eq!(style("important").text_indent(), 8);
    assert_eq!(style("important-inherit").text_indent(), 24);
    assert_eq!(style("reset").text_indent(), 0);
    assert_eq!(style("revert").text_indent(), 0);
}

#[test]
fn text_indent_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --indent: 24px; --alias: var(--indent); --cycle: var(--cycle); text-indent: var(--indent); }
            #child { text-indent: var(--alias); }
            #fallback { text-indent: var(--missing, 8px); }
            #invalid { --bad: 1em; text-indent: var(--bad, 12px); }
            #cycle { text-indent: var(--cycle, 16px); }
            #wide-initial { --wide: initial; text-indent: var(--wide); }
            #wide-inherit { --wide: inherit; text-indent: var(--wide); }
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
            .text_indent()
    };

    assert_eq!(style("parent"), 24);
    assert_eq!(style("child"), 24);
    assert_eq!(style("fallback"), 8);
    assert_eq!(style("invalid"), 12);
    assert_eq!(style("cycle"), 16);
    assert_eq!(style("wide-initial"), 0);
    assert_eq!(style("wide-inherit"), 24);
}

#[test]
fn local_text_declarations_revert_layer_resolve_independently() {
    let document = NativeDocument::parse(
            "<style>@layer base { #named { text-indent:8px; text-overflow:clip; } #repeat { text-indent:4px; text-overflow:ellipsis; } #inline { text-indent:12px; text-overflow:ellipsis; } #fallback { text-indent:revert-layer; text-overflow:revert-layer; } #invalid { text-indent:16px; text-overflow:ellipsis; } } @layer theme { #named { text-indent:20px; text-overflow:ellipsis; } #repeat { text-indent:revert-layer; text-overflow:revert-layer; } #inline { text-indent:16px; text-overflow:clip; } } @layer top { #repeat { text-indent:revert-layer; text-overflow:revert-layer; } } #named { text-indent:revert-layer; text-overflow:revert-layer; } #repeat { text-indent:revert-layer; text-overflow:revert-layer; } #invalid { text-indent:1px 2px; text-overflow:fade; }</style><div id='named'>Named</div><div id='repeat'>Repeat</div><div id='inline' style='text-indent:revert-layer;text-overflow:revert-layer'>Inline</div><div id='fallback'>Fallback</div><div id='invalid'>Invalid</div><div id='parent' style='text-indent:20px;text-overflow:ellipsis'><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let named = document.resolve_target("id=named").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();

    let style = |node| document.computed_style_for_layout(node);
    assert_eq!(style(named).text_indent(), 20);
    assert_eq!(style(named).text_overflow(), TextOverflowValue::Ellipsis);
    assert_eq!(style(repeat).text_indent(), 4);
    assert_eq!(style(repeat).text_overflow(), TextOverflowValue::Ellipsis);
    assert_eq!(style(inline).text_indent(), 16);
    assert_eq!(style(inline).text_overflow(), TextOverflowValue::Clip);
    assert_eq!(style(fallback).text_indent(), 0);
    assert_eq!(style(fallback).text_overflow(), TextOverflowValue::Clip);
    assert_eq!(style(invalid).text_indent(), 16);
    assert_eq!(style(invalid).text_overflow(), TextOverflowValue::Ellipsis);
    assert_eq!(style(parent).text_indent(), 20);
    assert_eq!(style(parent).text_overflow(), TextOverflowValue::Ellipsis);
    assert_eq!(style(child).text_indent(), 0);
    assert_eq!(style(child).text_overflow(), TextOverflowValue::Clip);
}

#[test]
fn local_dimension_declarations_revert_layer_resolve_independently() {
    let document = NativeDocument::parse(
            "<style>@layer base { #named { width:24px; height:12px; min-width:4px; max-width:80px; min-height:6px; max-height:90px; } #repeat { width:20px; height:10px; min-width:2px; max-width:70px; min-height:4px; max-height:60px; } #fallback { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #invalid { width:30px; height:18px; min-width:8px; max-width:70px; min-height:10px; max-height:40px; } } @layer theme { #named { width:48px; height:20px; min-width:16px; max-width:100px; min-height:10px; max-height:110px; } #repeat { width:36px; height:16px; min-width:12px; max-width:90px; min-height:8px; max-height:80px; } #inline { width:32px; height:22px; min-width:14px; max-width:92px; min-height:12px; max-height:82px; } } @layer top { #repeat { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } } #named { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #repeat { width:revert-layer; height:revert-layer; min-width:revert-layer; max-width:revert-layer; min-height:revert-layer; max-height:revert-layer; } #invalid { width:1px 2px; height:1px 2px; min-width:1px 2px; max-width:1px 2px; min-height:1px 2px; max-height:50%; }</style><div id='named'>Named</div><div id='repeat'>Repeat</div><div id='inline' style='width:revert-layer;height:revert-layer;min-width:revert-layer;max-width:revert-layer;min-height:revert-layer;max-height:revert-layer'>Inline</div><div id='fallback'>Fallback</div><div id='invalid'>Invalid</div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let named = document.resolve_target("id=named").unwrap();
    let repeat = document.resolve_target("id=repeat").unwrap();
    let inline = document.resolve_target("id=inline").unwrap();
    let fallback = document.resolve_target("id=fallback").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    let style = |node| document.computed_style_for_layout(node);
    let assert_dimensions = |node, expected: [Option<u32>; 6]| {
        let computed = style(node);
        assert_eq!(
            [
                computed.width(),
                computed.height(),
                computed.min_width(),
                computed.max_width(),
                computed.min_height(),
                computed.max_height(),
            ],
            expected
        );
    };
    assert_dimensions(
        named,
        [Some(48), Some(20), Some(16), Some(100), Some(10), Some(110)],
    );
    assert_dimensions(
        repeat,
        [Some(36), Some(16), Some(12), Some(90), Some(8), Some(80)],
    );
    assert_dimensions(
        inline,
        [Some(32), Some(22), Some(14), Some(92), Some(12), Some(82)],
    );
    assert_dimensions(fallback, [None, None, None, None, None, None]);
    assert_dimensions(
        invalid,
        [Some(30), Some(18), Some(8), Some(70), Some(10), Some(40)],
    );
}

#[test]
fn local_box_model_declarations_revert_layer_resolve_independently() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "@layer base { #named { box-sizing: content-box; padding: 1px 2px 3px 4px; margin: 1px 2px 3px 4px; } #repeated { box-sizing: border-box; padding: 2px; margin: auto 2px 3px 4px; } #fallback { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } #invalid { box-sizing: border-box; padding: 4px; margin: 5px; } } @layer theme { #named { box-sizing: border-box; padding: 5px 6px 7px 8px; margin: 5px 6px 7px 8px; } #repeated { box-sizing: content-box; padding: 6px; margin: revert-layer; } #inline { box-sizing: border-box; padding: 7px; margin: 8px; } } @layer top { #repeated { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } } #named { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } #repeated { box-sizing: revert-layer; padding: revert-layer; margin: revert-layer; } #invalid { box-sizing: auto; padding: 1px 2px 3px 4px 5px; margin: -1px; }"
                .into(),
        ])
        .unwrap();
    let named = node("<div id='named' class='named'>Named</div>");
    let repeated = node("<div id='repeated'>Repeated</div>");
    let inline = node(
        "<div id='inline' style='box-sizing:revert-layer;padding:revert-layer;margin:revert-layer'>Inline</div>",
    );
    let fallback = node("<div id='fallback'>Fallback</div>");
    let invalid = node("<div id='invalid'>Invalid</div>");

    let named_style = stylesheet.computed_for(&named);
    assert!(named_style.is_border_box());
    assert_eq!(
        named_style.padding(),
        NativeBoxEdges {
            top: 5,
            right: 6,
            bottom: 7,
            left: 8,
        }
    );
    assert_eq!(
        named_style.margin(),
        NativeBoxEdges {
            top: 5,
            right: 6,
            bottom: 7,
            left: 8,
        }
    );

    let repeated_style = stylesheet.computed_for(&repeated);
    assert!(!repeated_style.is_border_box());
    assert_eq!(
        repeated_style.padding(),
        NativeBoxEdges {
            top: 6,
            right: 6,
            bottom: 6,
            left: 6,
        }
    );
    assert_eq!(
        repeated_style.margin(),
        NativeBoxEdges {
            top: 0,
            right: 2,
            bottom: 3,
            left: 4,
        }
    );
    assert!(repeated_style.margin_auto().top());
    assert!(!repeated_style.margin_auto().right());
    assert!(!repeated_style.margin_auto().bottom());
    assert!(!repeated_style.margin_auto().left());

    let inline_style = stylesheet.computed_for(&inline);
    assert!(inline_style.is_border_box());
    assert_eq!(
        inline_style.padding(),
        NativeBoxEdges::from_values([Some(7); 4])
    );
    assert_eq!(
        inline_style.margin(),
        NativeBoxEdges::from_values([Some(8); 4])
    );

    let fallback_style = stylesheet.computed_for(&fallback);
    assert!(!fallback_style.is_border_box());
    assert_eq!(fallback_style.padding(), NativeBoxEdges::default());
    assert_eq!(fallback_style.margin(), NativeBoxEdges::default());
    assert_eq!(fallback_style.margin_auto(), NativeAutoEdges::default());

    let invalid_style = stylesheet.computed_for(&invalid);
    assert!(invalid_style.is_border_box());
    assert_eq!(
        invalid_style.padding(),
        NativeBoxEdges::from_values([Some(4); 4])
    );
    assert_eq!(
        invalid_style.margin(),
        NativeBoxEdges::from_values([Some(5); 4])
    );
}

#[test]
fn word_spacing_is_inherited_and_child_override_replaces_it() {
    let stylesheet = NativeStylesheet::from_sources(vec![
            "div { word-spacing: 16px; letter-spacing: 16px; } #target { word-spacing: 24px; letter-spacing: 24px; }".into(),
        ])
        .unwrap();
    let node = node("<div id='target'>Target</div>");
    assert_eq!(stylesheet.computed_for(&node).word_spacing(), 24);
    assert_eq!(stylesheet.computed_for(&node).letter_spacing(), 24);

    let document = NativeDocument::parse(
            "<style>#parent { word-spacing: 16px; letter-spacing: 16px; } #explicit { word-spacing: 24px; letter-spacing: 24px; } #invalid { word-spacing: -1px; letter-spacing: -1px; }</style><div id='parent'><span id='child'>Child</span><span id='explicit'>Explicit</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .unwrap();
    let parent = document.resolve_target("id=parent").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let explicit = document.resolve_target("id=explicit").unwrap();
    let invalid = document.resolve_target("id=invalid").unwrap();

    assert_eq!(
        document.computed_style_for_layout(parent).word_spacing(),
        16
    );
    assert_eq!(document.computed_style_for_layout(child).word_spacing(), 16);
    assert_eq!(
        document.computed_style_for_layout(child).letter_spacing(),
        16
    );
    assert_eq!(
        document.computed_style_for_layout(explicit).word_spacing(),
        24
    );
    assert_eq!(
        document
            .computed_style_for_layout(explicit)
            .letter_spacing(),
        24
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).word_spacing(),
        16
    );
    assert_eq!(
        document.computed_style_for_layout(invalid).letter_spacing(),
        16
    );
}

#[test]
fn inherited_word_spacing_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --word-spacing: 16px; --alias: var(--word-spacing); --cycle: var(--cycle); word-spacing: var(--word-spacing); }
            #child { word-spacing: var(--alias); }
            #fallback { word-spacing: var(--missing, 24px); }
            #invalid { --bad: 1em; word-spacing: var(--bad, 20px); }
            #cycle { word-spacing: var(--cycle, 8px); }
            #wide-initial { --wide: initial; word-spacing: var(--wide); }
            #wide-inherit { --wide: inherit; word-spacing: var(--wide); }
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
            .word_spacing()
    };

    assert_eq!(style("parent"), 16);
    assert_eq!(style("child"), 16);
    assert_eq!(style("fallback"), 24);
    assert_eq!(style("invalid"), 20);
    assert_eq!(style("cycle"), 8);
    assert_eq!(style("wide-initial"), 0);
    assert_eq!(style("wide-inherit"), 16);
}

#[test]
fn inherited_letter_spacing_custom_properties_resolve_with_fallbacks() {
    let document = NativeDocument::parse(
            r#"<style>
            #parent { --letter-spacing: 16px; --alias: var(--letter-spacing); --cycle: var(--cycle); letter-spacing: var(--letter-spacing); }
            #child { letter-spacing: var(--alias); }
            #fallback { letter-spacing: var(--missing, 24px); }
            #invalid { --bad: 1em; letter-spacing: var(--bad, 20px); }
            #cycle { letter-spacing: var(--cycle, 8px); }
            #wide-initial { --wide: initial; letter-spacing: var(--wide); }
            #wide-inherit { --wide: inherit; letter-spacing: var(--wide); }
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
            .letter_spacing()
    };

    assert_eq!(style("parent"), 16);
    assert_eq!(style("child"), 16);
    assert_eq!(style("fallback"), 24);
    assert_eq!(style("invalid"), 20);
    assert_eq!(style("cycle"), 8);
    assert_eq!(style("wide-initial"), 0);
    assert_eq!(style("wide-inherit"), 16);
}

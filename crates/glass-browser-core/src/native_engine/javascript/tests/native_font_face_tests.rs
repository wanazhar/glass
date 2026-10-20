use super::super::config::NativeEngineLimits;
use super::super::css::{
    FontWeightValue, NativeFontStretchRange, NativeFontVariation, NativeFontVariationSettings,
    NativeFontWeightRange, NativeUnicodeRange,
};
use super::super::font::NativeFontBook;
use super::*;
use std::sync::{Mutex, MutexGuard};

fn local_system_font_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn document_fonts_projects_css_faces_and_supports_set_operations() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>@font-face { font-family: 'Missing Face'; src: local('Missing Face'); font-weight: 300 700; font-stretch: condensed; unicode-range: U+41-5A; }</style>",
            &NativeEngineLimits::default(),
        )
        .expect("font-face fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const cssFace = [...document.fonts][0];
                  const initial = [
                    document.fonts instanceof FontFaceSet,
                    document.fonts.status,
                    document.fonts.size,
                    cssFace.family,
                    cssFace.weight,
                    cssFace.stretch,
                    cssFace.unicodeRange,
                    cssFace.status,
                    document.fonts.check("16px 'Missing Face'"),
                    document.fonts.check("16px Arial"),
                  ];
                  const dynamicFace = new FontFace("Dynamic Face", "url(dynamic.woff2)", { weight: "700" });
                  document.fonts.add(dynamicFace);
                  const added = [document.fonts.size, dynamicFace.status, document.fonts.check("16px Dynamic Face")];
                  const deleted = document.fonts.delete(dynamicFace);
                  return [initial, added, deleted, document.fonts.size, [...document.fonts].length];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFaceSet surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            [
                true,
                "loaded",
                1,
                "Missing Face",
                "300 700",
                "condensed",
                "U+41-5A",
                "error",
                false,
                true
            ],
            [2, "unloaded", false],
            true,
            1,
            1,
        ])
    );
}

#[test]
fn computed_style_projects_font_stretch_as_a_percentage() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-stretch-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-stretch: condensed; } #child { font-stretch: 62.5%; }</style><div id='parent'><span id='child'>Child</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("font-stretch fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  return [parent.fontStretch, child.getPropertyValue("font-stretch"), child.item(0) !== ""];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-stretch surface must evaluate");
    assert_eq!(evaluation.value, serde_json::json!(["75%", "62.5%", true]));
}

#[test]
fn computed_style_projects_numeric_font_weight() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-weight-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-weight: 500; } #bold { font-weight: 700; } #reset { font-weight: normal; } #invalid { font-weight: 1001; }</style><div id='parent'><span id='bold'>Bold</span><span id='reset'>Reset</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("numeric font-weight fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const bold = getComputedStyle(document.getElementById("bold"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  const invalid = getComputedStyle(document.getElementById("invalid"));
                  return [parent.fontWeight, parent.getPropertyValue("font-weight"), bold.fontWeight, reset.fontWeight, invalid.fontWeight];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed numeric font-weight surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["500", "500", "bold", "normal", "500"])
    );
}

#[test]
fn computed_style_projects_relative_font_weight() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("computed-relative-font-weight-test")
            .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-weight: 550; } #lighter { font-weight: lighter; } #bolder { font-weight: bolder; } #inherit { font-weight: inherit; } #initial { font-weight: initial; } #invalid { font-weight: lighter bolder; }</style><div id='parent'><span id='lighter'>Lighter</span><span id='bolder'>Bolder</span><span id='inherit'>Inherit</span><span id='initial'>Initial</span><span id='invalid'>Invalid</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("relative font-weight fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const lighter = getComputedStyle(document.getElementById("lighter"));
                  const bolder = getComputedStyle(document.getElementById("bolder"));
                  const inherited = getComputedStyle(document.getElementById("inherit"));
                  const initial = getComputedStyle(document.getElementById("initial"));
                  const invalid = getComputedStyle(document.getElementById("invalid"));
                  return [lighter.fontWeight, bolder.getPropertyValue("font-weight"), inherited.fontWeight, initial.fontWeight, invalid.fontWeight];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed relative font-weight surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["normal", "900", "550", "normal", "550"])
    );
}

#[test]
fn computed_style_projects_font_variant_ligatures() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-variant-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-ligatures: none; } #child { font-variant-ligatures: discretionary-ligatures contextual; } #reset { font-variant-ligatures: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("font-variant-ligatures fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontVariantLigatures, child.getPropertyValue("font-variant-ligatures"), reset.fontVariantLigatures];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-variant-ligatures surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "none",
            "common-ligatures discretionary-ligatures no-historical-ligatures contextual",
            "normal"
        ])
    );
}

#[test]
fn computed_style_projects_font_feature_settings() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-feature-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            r#"<style>#parent { font-feature-settings: "liga" off, "kern" on; } #child { font-feature-settings: "liga" 1, "liga" 0, "dlig" on; } #reset { font-feature-settings: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .expect("font-feature-settings fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontFeatureSettings, child.getPropertyValue("font-feature-settings"), reset.fontFeatureSettings];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-feature-settings surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["\"liga\" 0, \"kern\" 1", "\"liga\" 0, \"dlig\" 1", "normal"])
    );
}

#[test]
fn computed_style_projects_font_kerning() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-kerning-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            r#"<style>#parent { font-kerning: none; } #child { font-kerning: normal; } #reset { font-kerning: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .expect("font-kerning fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontKerning, child.getPropertyValue("font-kerning"), reset.fontKerning];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-kerning surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["none", "normal", "auto"])
    );
}

#[test]
fn computed_style_projects_font_optical_sizing() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-optical-sizing-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            r#"<style>#parent { font-optical-sizing: auto; } #child { font-optical-sizing: none; } #reset { font-optical-sizing: initial; } #invalid { font-optical-sizing: auto none; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span><span id='invalid'>Invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .expect("font-optical-sizing fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  const invalid = getComputedStyle(document.getElementById("invalid"));
                  return [parent.fontOpticalSizing, child.getPropertyValue("font-optical-sizing"), reset.fontOpticalSizing, invalid.fontOpticalSizing];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-optical-sizing surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["auto", "none", "auto", "auto"])
    );
}

#[test]
fn computed_style_projects_font_palette() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-palette-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            r#"<style>#parent { font-palette: light; } #child { font-palette: dark; } #reset { font-palette: initial; } #named { font-palette: --brand; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span><span id='named'>Named</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .expect("font-palette fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  const named = getComputedStyle(document.getElementById("named"));
                  return [parent.fontPalette, child.getPropertyValue("font-palette"), reset.fontPalette, named.fontPalette];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-palette surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["light", "dark", "normal", "--brand"])
    );
}

#[test]
fn computed_style_projects_font_variant_caps() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-caps-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-caps: all-small-caps; } #child { font-variant-caps: petite-caps; } #reset { font-variant-caps: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("font-variant-caps fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontVariantCaps, child.getPropertyValue("font-variant-caps"), reset.fontVariantCaps];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-variant-caps surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["all-small-caps", "petite-caps", "normal"])
    );
}

#[test]
fn computed_style_projects_font_variant_position() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-position-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-position: super; } #child { font-variant-position: sub; } #reset { font-variant-position: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("font-variant-position fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontVariantPosition, child.getPropertyValue("font-variant-position"), reset.fontVariantPosition];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-variant-position surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["super", "sub", "normal"])
    );
}

#[test]
fn computed_style_projects_font_variant_alternates() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-alternates-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-alternates: historical-forms; } #child { font-variant-alternates: normal; } #reset { font-variant-alternates: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("font-variant-alternates fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontVariantAlternates, child.getPropertyValue("font-variant-alternates"), reset.fontVariantAlternates];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-variant-alternates surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["historical-forms", "normal", "normal"])
    );
}

#[test]
fn computed_style_projects_font_variant_east_asian() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-east-asian-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-east-asian: jis83 full-width ruby; } #child { font-variant-east-asian: simplified proportional-width; } #reset { font-variant-east-asian: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("font-variant-east-asian fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontVariantEastAsian, child.getPropertyValue("font-variant-east-asian"), reset.fontVariantEastAsian];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-variant-east-asian surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "jis83 full-width ruby",
            "simplified proportional-width",
            "normal"
        ])
    );
}

#[test]
fn computed_style_projects_font_language_override() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("computed-font-language-override-test")
            .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            r#"<style>#parent { font-language-override: "ENG "; } #reset { font-language-override: normal; }</style><div id="parent">Parent</div><span id="reset">Reset</span>"#,
            &NativeEngineLimits::default(),
        )
        .expect("font-language-override fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontLanguageOverride, parent.getPropertyValue("font-language-override"), reset.fontLanguageOverride];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-language-override surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["\"ENG \"", "\"ENG \"", "normal"])
    );
}

#[test]
fn computed_style_projects_font_variation_settings() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("computed-font-variation-settings-test")
            .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            r#"<style>#parent { font-variation-settings: "wght" 450.5, "wdth" -12.25, "wght" 700; } #inherit { font-variation-settings: inherit; } #reset { font-variation-settings: initial; } #invalid { font-variation-settings: "wght" 700 escape; }</style><div id="parent"><span id="inherit">Inherit</span><span id="reset">Reset</span><span id="invalid">Invalid</span></div>"#,
            &NativeEngineLimits::default(),
        )
        .expect("font-variation-settings fixture must parse");
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const inherited = getComputedStyle(document.getElementById("inherit"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  const invalid = getComputedStyle(document.getElementById("invalid"));
                  return [
                    parent.fontVariationSettings,
                    inherited.getPropertyValue("font-variation-settings"),
                    reset.fontVariationSettings,
                    invalid.fontVariationSettings
                  ];
                })()"#,
            &document,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("computed font-variation-settings surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "\"wght\" 700, \"wdth\" -12.25",
            "\"wght\" 700, \"wdth\" -12.25",
            "normal",
            "\"wght\" 700, \"wdth\" -12.25"
        ])
    );
}

#[test]
fn computed_style_projects_font_variant_shorthand() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("computed-font-variant-shorthand-test")
            .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-variant: small-caps super historical-forms jis83 full-width ruby lining-nums; } #reset { font-variant: normal; }</style><div id='parent'>Parent</div><span id='reset'>Reset</span>",
            &NativeEngineLimits::default(),
        )
        .expect("font-variant shorthand fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontVariant, parent.getPropertyValue("font-variant"), reset.fontVariant];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-variant shorthand surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "small-caps super historical-forms jis83 full-width ruby lining-nums",
            "small-caps super historical-forms jis83 full-width ruby lining-nums",
            "normal"
        ])
    );
}

#[test]
fn computed_style_projects_font_variant_numeric() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("computed-font-numeric-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::parse(
            "<style>#parent { font-variant-numeric: oldstyle-nums tabular-nums stacked-fractions ordinal slashed-zero; } #child { font-variant-numeric: lining-nums proportional-nums diagonal-fractions; } #reset { font-variant-numeric: initial; }</style><div id='parent'><span id='child'>Child</span><span id='reset'>Reset</span></div>",
            &NativeEngineLimits::default(),
        )
        .expect("font-variant-numeric fixture must parse");
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const parent = getComputedStyle(document.getElementById("parent"));
                  const child = getComputedStyle(document.getElementById("child"));
                  const reset = getComputedStyle(document.getElementById("reset"));
                  return [parent.fontVariantNumeric, child.getPropertyValue("font-variant-numeric"), reset.fontVariantNumeric];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("computed font-variant-numeric surface must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "oldstyle-nums tabular-nums stacked-fractions ordinal slashed-zero",
            "lining-nums proportional-nums diagonal-fractions",
            "normal"
        ])
    );
}

#[test]
fn css_font_face_refresh_dispatches_loading_error_for_new_rules() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-refresh-test")
        .expect("native JavaScript runtime must construct");
    let empty = NativeDocument::empty();
    runtime
        .evaluate(
            r#"globalThis.__fontFaceEvents = [];
                globalThis.__fontFaceHandlerEvents = [];
                document.fonts.addEventListener("loading", event => {
                  __fontFaceEvents.push(["loading", event.fontfaces.length]);
                });
                document.fonts.addEventListener("loadingerror", event => {
                  __fontFaceEvents.push(["loadingerror", event.fontfaces.length]);
                });
                document.fonts.onloading = event => {
                  __fontFaceHandlerEvents.push(["loading", event.fontfaces.length]);
                };
                document.fonts.onloadingerror = event => {
                  __fontFaceHandlerEvents.push(["loadingerror", event.fontfaces.length]);
                };
                true"#,
            &empty,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("FontFaceSet listeners must install");
    let styled = NativeDocument::parse(
            "<style>@font-face { font-family: 'Late Face'; src: local('Late Face'); font-feature-settings: \"liga\" off, \"kern\" on; font-variation-settings: \"wght\" 620; size-adjust: 62.5%; ascent-override: 80%; descent-override: 20%; line-gap-override: normal; font-display: swap; }</style>",
            &NativeEngineLimits::default(),
        )
        .expect("font-face fixture must parse");
    let evaluation = runtime
            .evaluate(
                "[document.fonts.size, document.fonts.status, __fontFaceEvents, __fontFaceHandlerEvents, [...document.fonts][0].status, [...document.fonts][0].featureSettings, [...document.fonts][0].variationSettings, [...document.fonts][0].sizeAdjust, [...document.fonts][0].ascentOverride, [...document.fonts][0].descentOverride, [...document.fonts][0].lineGapOverride, [...document.fonts][0].display]",
                &styled,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFaceSet refresh must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            1,
            "loaded",
            [["loading", 1], ["loadingerror", 1]],
            [["loading", 1], ["loadingerror", 1]],
            "error",
            "\"liga\" 0, \"kern\" 1",
            "\"wght\" 620",
            "62.5%",
            "80%",
            "20%",
            "normal",
            "swap"
        ])
    );
}

#[test]
fn font_face_set_handler_properties_replace_and_remove() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-handler-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                  const events = [];
                  const first = event => events.push(["first", event.type]);
                  const second = event => events.push(["second", event.type]);
                  document.fonts.onloading = first;
                  document.fonts.dispatchEvent(new Event("loading"));
                  document.fonts.onloading = second;
                  document.fonts.dispatchEvent(new Event("loading"));
                  const replaced = document.fonts.onloading === second;
                  document.fonts.onloading = null;
                  document.fonts.dispatchEvent(new Event("loading"));
                  return [typeof document.fonts.onloading, replaced, events];
                })()"#,
            &document,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("FontFaceSet handler properties must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            "object",
            true,
            [["first", "loading"], ["second", "loading"]]
        ])
    );
}

#[test]
fn script_font_face_display_descriptor_normalizes_and_rejects_invalid_values() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-display-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                  const face = new FontFace("Display Face", "", { display: " Swap " });
                  let constructorError = null;
                  try {
                    new FontFace("Invalid Display Face", "", { display: "blink" });
                  } catch (error) {
                    constructorError = error.name;
                  }
                  let setterError = null;
                  try {
                    face.display = "blink";
                  } catch (error) {
                    setterError = [error.name, face.display];
                  }
                  return [face.display, constructorError, setterError];
                })()"#,
            &document,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("FontFace display descriptor validation must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["swap", "SyntaxError", ["SyntaxError", "swap"]])
    );
}

#[test]
fn script_font_face_descriptor_validation_normalizes_and_rejects_invalid_values() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-descriptor-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const face = new FontFace("Descriptor Face", "", {
                    style: " ITALIC ",
                    weight: " 0300 0700 ",
                    stretch: " 62.5% 125% ",
                    unicodeRange: "u+41-5a, u+400-4ff",
                    variationSettings: '"wght" 450.5, "wdth" -12.25',
                    sizeAdjust: "62.5%",
                    ascentOverride: "80%",
                    descentOverride: "20%",
                    lineGapOverride: "normal",
                    display: " SWAP "
                  });
                  const invalid = [
                    ["style", "oblique"],
                    ["weight", "700 300"],
                    ["stretch", "40%"],
                    ["unicodeRange", "U+110000"],
                    ["variationSettings", "wght 700"],
                    ["sizeAdjust", "24.9%"],
                    ["ascentOverride", "1000.1%"]
                  ];
                  const constructorErrors = invalid.map(([name, value]) => {
                    const descriptors = {};
                    descriptors[name] = value;
                    try {
                      new FontFace("Invalid Descriptor Face", "", descriptors);
                      return [name, "accepted"];
                    } catch (error) {
                      return [name, error.name];
                    }
                  });
                  const setterResults = invalid.map(([name, value]) => {
                    const previous = face[name];
                    try {
                      face[name] = value;
                      return [name, "accepted", face[name]];
                    } catch (error) {
                      return [name, error.name, face[name] === previous, face[name]];
                    }
                  });
                  return [
                    [face.style, face.weight, face.stretch, face.unicodeRange, face.variationSettings, face.sizeAdjust, face.ascentOverride, face.descentOverride, face.lineGapOverride, face.display],
                    constructorErrors,
                    setterResults
                  ];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFace descriptor validation must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            [
                "italic",
                "300 700",
                "62.5% 125%",
                "U+41-5A, U+400-4FF",
                "\"wght\" 450.5, \"wdth\" -12.25",
                "62.5%",
                "80%",
                "20%",
                "normal",
                "swap"
            ],
            [
                ["style", "SyntaxError"],
                ["weight", "SyntaxError"],
                ["stretch", "SyntaxError"],
                ["unicodeRange", "SyntaxError"],
                ["variationSettings", "SyntaxError"],
                ["sizeAdjust", "SyntaxError"],
                ["ascentOverride", "SyntaxError"]
            ],
            [
                ["style", "SyntaxError", true, "italic"],
                ["weight", "SyntaxError", true, "300 700"],
                ["stretch", "SyntaxError", true, "62.5% 125%"],
                ["unicodeRange", "SyntaxError", true, "U+41-5A, U+400-4FF"],
                [
                    "variationSettings",
                    "SyntaxError",
                    true,
                    "\"wght\" 450.5, \"wdth\" -12.25",
                ],
                ["sizeAdjust", "SyntaxError", true, "62.5%"],
                ["ascentOverride", "SyntaxError", true, "80%"]
            ]
        ])
    );
}

#[test]
fn script_font_face_variant_and_feature_descriptors_normalize_transactionally() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-variant-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let evaluation = runtime
            .evaluate(
                r#"(() => {
                  const face = new FontFace("Variant Face", "", {
                    variant: " SMALL-CAPS proportional-nums RUBY ",
                    featureSettings: '"liga" on, \'kern\' 0000, "liga" off, "calt"'
                  });
                  document.fonts.add(face);
                  const invalid = [
                    ["variant", "normal small-caps"],
                    ["variant", "small-caps small-caps"],
                    ["variant", "made-up"],
                    ["featureSettings", "liga 1"],
                    ["featureSettings", '"liga" 65536'],
                    ["featureSettings", '"toolong" 1']
                  ];
                  const constructorErrors = invalid.map(([name, value]) => {
                    const descriptors = {};
                    descriptors[name] = value;
                    try {
                      new FontFace("Invalid Variant Face", "", descriptors);
                      return [name, "accepted"];
                    } catch (error) {
                      return [name, error.name];
                    }
                  });
                  const previous = [face.variant, face.featureSettings];
                  const setterErrors = invalid.map(([name, value]) => {
                    try {
                      face[name] = value;
                      return [name, "accepted"];
                    } catch (error) {
                      return [name, error.name];
                    }
                  });
                  return [
                    [face.variant, face.featureSettings, document.fonts.size, Array.from(document.fonts)[0] === face],
                    constructorErrors,
                    [setterErrors, face.variant === previous[0] && face.featureSettings === previous[1]]
                  ];
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFace variant descriptor validation must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!([
            [
                "small-caps proportional-nums ruby",
                "\"liga\" 0, \"kern\" 0, \"calt\" 1",
                1,
                true
            ],
            [
                ["variant", "SyntaxError"],
                ["variant", "SyntaxError"],
                ["variant", "SyntaxError"],
                ["featureSettings", "SyntaxError"],
                ["featureSettings", "SyntaxError"],
                ["featureSettings", "SyntaxError"]
            ],
            [
                [
                    ["variant", "SyntaxError"],
                    ["variant", "SyntaxError"],
                    ["variant", "SyntaxError"],
                    ["featureSettings", "SyntaxError"],
                    ["featureSettings", "SyntaxError"],
                    ["featureSettings", "SyntaxError"]
                ],
                true
            ]
        ])
    );
}

#[test]
fn script_font_face_load_installs_bounded_inline_bytes() {
    #[cfg(not(target_os = "linux"))]
    return;
    let Ok(bytes) = std::fs::read("/usr/share/fonts/truetype/noto/NotoSansLycian-Regular.ttf")
    else {
        return;
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let source = serde_json::to_string(&format!("url(data:font/ttf;base64,{encoded})"))
        .expect("font source must serialize");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-inline-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse("<body></body>", &NativeEngineLimits::default())
        .expect("font-face document must parse");
    let started = runtime
            .evaluate(
                &format!(
                    r#"(() => {{
                      const face = new FontFace("Inline Sans", {source}, {{ weight: "300 700", stretch: "condensed", unicodeRange: "U+41-5A", variant: "SMALL-CAPS", featureSettings: '"liga" off, "kern" on', variationSettings: '"wght" 620', sizeAdjust: "62.5%", ascentOverride: "80%", descentOverride: "20%", lineGapOverride: "normal", display: "optional" }});
                      globalThis.__inlineFontFace = face;
                      document.fonts.add(face);
                      face.load().then(() => {{ document.body.textContent = "accepted"; }});
                      return true;
                    }})()"#
                ),
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("inline FontFace source must evaluate");
    let pending = runtime
            .evaluate(
                "[globalThis.__inlineFontFace.status, document.fonts.status, document.body.textContent]",
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("pending FontFace state must evaluate");
    assert_eq!(pending.value, serde_json::json!(["loading", "loading", ""]));
    apply_document_commands_with_font_face_ack(
        &mut document,
        &runtime,
        "about:blank",
        &NativeOrigin::Opaque,
        Viewport::default(),
        &started.commands,
        false,
    )
    .expect("initial FontFace commands must apply and acknowledge");
    let evaluation = runtime
            .evaluate(
                "[globalThis.__inlineFontFace.status, document.fonts.status, document.fonts.size, document.body.textContent]",
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("inline FontFace completion must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["loaded", "loaded", 1, "accepted"])
    );
    let install = started
        .commands
        .iter()
        .chain(evaluation.commands.iter())
        .find(|command| matches!(command, NativeScriptCommand::FontFaceInstall { .. }))
        .expect("FontFace load must emit a native installation command");
    assert!(matches!(
        install,
        NativeScriptCommand::FontFaceInstall {
            family,
            weight,
            style,
            stretch,
            unicode_range,
            variant,
            feature_settings,
            size_adjust,
            ascent_override,
            descent_override,
            line_gap_override,
            display,
            variation_settings,
            body_base64,
            ..
        } if family == "Inline Sans"
            && weight == "300 700"
            && style == "normal"
            && stretch == "condensed"
            && unicode_range == "U+41-5A"
            && variant == "small-caps"
            && feature_settings == "\"liga\" 0, \"kern\" 1"
            && variation_settings == "\"wght\" 620"
            && size_adjust == "62.5%"
            && ascent_override == "80%"
            && descent_override == "20%"
            && line_gap_override == "normal"
            && display == "optional"
            && !body_base64.is_empty()
    ));
    let wire = document.to_content_wire();
    assert_eq!(wire.font_resources.len(), 1);
    assert_eq!(wire.font_resources[0].family, "Inline Sans");
    assert_eq!(
        wire.font_resources[0].weight_range,
        Some(NativeFontWeightRange { min: 300, max: 700 })
    );
    assert_eq!(wire.font_resources[0].weight, FontWeightValue::Numeric(500));
    assert_eq!(
        wire.font_resources[0].stretch,
        NativeFontStretchRange { min: 750, max: 750 }
    );
    assert_eq!(
        wire.font_resources[0].unicode_ranges,
        vec![NativeUnicodeRange {
            start: 0x41,
            end: 0x5A,
        }]
    );
    let mut expected_variations = NativeFontVariationSettings::default();
    expected_variations.values[0] = NativeFontVariation {
        tag: *b"wght",
        value_milli: 620_000,
    };
    expected_variations.count = 1;
    assert_eq!(
        wire.font_resources[0].variation_settings,
        expected_variations
    );
    assert_eq!(wire.font_resources[0].feature_settings.count, 3);
    assert_eq!(
        wire.font_resources[0].feature_settings.values[0].tag,
        *b"liga"
    );
    assert_eq!(wire.font_resources[0].feature_settings.values[0].value, 0);
    assert_eq!(
        wire.font_resources[0].feature_settings.values[1].tag,
        *b"kern"
    );
    assert_eq!(wire.font_resources[0].feature_settings.values[1].value, 1);
    assert_eq!(
        wire.font_resources[0].feature_settings.values[2].tag,
        *b"smcp"
    );
    assert_eq!(wire.font_resources[0].feature_settings.values[2].value, 1);
    assert_eq!(wire.font_resources[0].size_adjust, 625);
    assert_eq!(wire.font_resources[0].metric_overrides.ascent, Some(800));
    assert_eq!(wire.font_resources[0].metric_overrides.descent, Some(200));
    assert_eq!(wire.font_resources[0].metric_overrides.line_gap, None);
}

#[test]
fn script_font_face_load_accepts_buffer_source_views() {
    #[cfg(not(target_os = "linux"))]
    return;
    let Ok(font_bytes) = std::fs::read("/usr/share/fonts/truetype/noto/NotoSansLycian-Regular.ttf")
    else {
        return;
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&font_bytes);
    let mut padded = Vec::with_capacity(font_bytes.len() + 4);
    padded.extend_from_slice(&[0, 1]);
    padded.extend_from_slice(&font_bytes);
    padded.extend_from_slice(&[2, 3]);
    let values = serde_json::to_string(&padded).expect("buffer source must serialize");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-buffer-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse("<body></body>", &NativeEngineLimits::default())
        .expect("font-face document must parse");
    let started = runtime
            .evaluate(
                &format!(
                    r#"(() => {{
                      const storage = new Uint8Array({values});
                      const face = new FontFace("Buffer Sans", storage.subarray(2, storage.length - 2));
                      globalThis.__bufferFontFace = face;
                      document.fonts.add(face);
                      face.load().then(() => {{ document.body.textContent = "buffer-loaded"; }});
                      return true;
                    }})()"#
                ),
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFace BufferSource must evaluate");
    apply_document_commands_with_font_face_ack(
        &mut document,
        &runtime,
        "about:blank",
        &NativeOrigin::Opaque,
        Viewport::default(),
        &started.commands,
        false,
    )
    .expect("FontFace BufferSource must apply and acknowledge");
    let evaluation = runtime
            .evaluate(
                "[globalThis.__bufferFontFace.status, document.fonts.status, document.body.textContent]",
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFace BufferSource completion must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["loaded", "loaded", "buffer-loaded"])
    );
    let install = started
        .commands
        .iter()
        .find(|command| matches!(command, NativeScriptCommand::FontFaceInstall { .. }))
        .expect("BufferSource FontFace load must emit an installation command");
    assert!(matches!(
        install,
        NativeScriptCommand::FontFaceInstall { body_base64, .. } if body_base64 == &encoded
    ));
    assert_eq!(document.to_content_wire().font_resources.len(), 1);
}

#[test]
fn script_font_face_url_emits_private_font_destination() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-url-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                  const face = new FontFace("Remote Face", "url(https://fonts.test/face.woff2)");
                  document.fonts.add(face);
                  face.load();
                  return true;
                })()"#,
            &document,
            "https://app.test/index.html",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("URL FontFace source must evaluate");
    assert!(evaluation.commands.iter().any(|command| matches!(
        command,
        NativeScriptCommand::Fetch {
            destination: Some(destination),
            href,
            method,
            credentials: true,
            mode: Some(mode),
            redirect: Some(redirect),
            cache: Some(cache),
            ..
        } if destination == "font"
            && href == "https://fonts.test/face.woff2"
            && method == "GET"
            && mode == "cors"
            && redirect == "follow"
            && cache == "default"
    )));
}

#[test]
fn script_font_face_source_list_tries_supported_fallbacks() {
    let _font_lock = local_system_font_test_lock();
    let family = "Glass Deterministic Test Face";
    NativeFontBook::set_test_local_font_source(
        family,
        Some(include_bytes!("../../../../tests/fixtures/colr-v0.ttf").to_vec()),
    );
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-list-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse("<body></body>", &NativeEngineLimits::default())
        .expect("font-face document must parse");
    let started = runtime
            .evaluate(
                r#"(() => {
                  const face = new FontFace(
                    "Fallback Face",
                    "local('Missing Face') format('future-format'), local('Missing Face') format('woff2'), local('Glass Deterministic Test Face') tech(color-COLRv1), local('Glass Deterministic Test Face') format('truetype')",
                  );
                  globalThis.__fallbackFontFace = face;
                  document.fonts.add(face);
                  face.load().then(() => { document.body.textContent = "fallback-loaded"; });
                  return true;
                })()"#,
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFace source list must evaluate");
    apply_document_commands_with_font_face_ack(
        &mut document,
        &runtime,
        "about:blank",
        &NativeOrigin::Opaque,
        Viewport::default(),
        &started.commands,
        false,
    )
    .expect("FontFace source-list fallback must apply and acknowledge");
    let evaluation = runtime
            .evaluate(
                "[globalThis.__fallbackFontFace.status, document.fonts.status, document.body.textContent]",
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFace source-list completion must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["loaded", "loaded", "fallback-loaded"])
    );
    assert_eq!(document.to_content_wire().font_resources.len(), 1);
    NativeFontBook::set_test_local_font_source(family, None);
}

#[test]
fn script_font_face_rejection_acknowledges_admission_failure() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-reject-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::parse("<body></body>", &NativeEngineLimits::default())
        .expect("font-face document must parse");
    let started = runtime
        .evaluate(
            r#"(() => {
                  const face = new FontFace("Rejected Face", "url(data:font/ttf;base64,AA==)");
                  globalThis.__rejectedFontFace = face;
                  document.fonts.add(face);
                  face.load().then(
                    () => { document.body.textContent = "unexpected-success"; },
                    error => { document.body.textContent = `${error.name}:${face.status}`; },
                  );
                  return true;
                })()"#,
            &document,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("invalid FontFace source must evaluate");
    let mut pending_fetches = Vec::new();
    let mut websocket_commands = Vec::new();
    let mut event_source_commands = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut navigation = None;
    apply_page_script_evaluation(
        &mut document,
        &runtime,
        NativePageScriptContext {
            document_url: "about:blank",
            origin: &NativeOrigin::Opaque,
            viewport: Viewport::default(),
        },
        started,
        NativePageScriptCommandQueues {
            pending_fetches: &mut pending_fetches,
            websocket_commands: &mut websocket_commands,
            event_source_commands: &mut event_source_commands,
        },
        &mut scroll_commands,
        &mut navigation,
    )
    .expect("invalid FontFace admission must be acknowledged");
    let evaluation = runtime
            .evaluate(
                "[globalThis.__rejectedFontFace.status, document.fonts.status, document.body.textContent]",
                &document,
                "about:blank",
                &NativeOrigin::Opaque,
                Viewport::default(),
            )
            .expect("FontFace rejection state must evaluate");
    assert_eq!(
        evaluation.value,
        serde_json::json!(["error", "loaded", "NetworkError:error"])
    );
    assert!(document.to_content_wire().font_resources.is_empty());
}

#[test]
fn script_font_face_load_resolves_local_system_faces() {
    let _font_lock = local_system_font_test_lock();
    let family = "Glass Deterministic Test Face";
    NativeFontBook::set_test_local_font_source(
        family,
        Some(include_bytes!("../../../../tests/fixtures/colr-v0.ttf").to_vec()),
    );
    let runtime = NativeJavaScriptRuntime::new_with_context_id("font-face-local-test")
        .expect("native JavaScript runtime must construct");
    let mut document = NativeDocument::empty();
    let started = runtime
        .evaluate(
            r#"(() => {
                  const face = new FontFace("Local Alias", "local('Glass Deterministic Test Face')");
                  globalThis.__localFontFace = face;
                  document.fonts.add(face);
                  face.load();
                  return true;
                })()"#,
            &document,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("local FontFace source must evaluate");
    assert!(
        started
            .commands
            .iter()
            .any(|command| matches!(command, NativeScriptCommand::FontFaceInstall { .. })),
        "local FontFace load must emit an installation command"
    );
    apply_document_commands_with_font_face_ack(
        &mut document,
        &runtime,
        "about:blank",
        &NativeOrigin::Opaque,
        Viewport::default(),
        &started.commands,
        false,
    )
    .expect("local FontFace commands must apply and acknowledge");
    let evaluation = runtime
        .evaluate(
            "[globalThis.__localFontFace.status, document.fonts.status]",
            &document,
            "about:blank",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("local FontFace completion must evaluate");
    assert_eq!(evaluation.value, serde_json::json!(["loaded", "loaded"]));
    assert_eq!(document.to_content_wire().font_resources.len(), 1);
    NativeFontBook::set_test_local_font_source(family, None);
}

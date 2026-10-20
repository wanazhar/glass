use super::*;

#[test]
fn text_transform_parser_accepts_only_bounded_ascii_modes() {
    assert_eq!(
        parse_text_transform("UPPERCASE"),
        Some(TextTransformValue::Uppercase)
    );
    assert_eq!(
        parse_text_transform("lowercase"),
        Some(TextTransformValue::Lowercase)
    );
    assert_eq!(parse_text_transform("none"), Some(TextTransformValue::None));
    assert_eq!(parse_text_transform("capitalize"), None);
    assert_eq!(parse_text_transform("full-width"), None);
    assert_eq!(parse_text_transform("initial"), None);
    let transform_name = parse_custom_property_name("--transform").unwrap();
    let fallback = TextTransformValue::Lowercase;
    assert_eq!(
        parse_text_transform_property("var(--transform)"),
        Some(TextTransformDeclarationValue::CustomProperty(
            transform_name
        ))
    );
    assert_eq!(
        parse_text_transform_declaration("var(--transform, lowercase)"),
        Some(InheritedTextDeclaration::Value(
            TextTransformDeclarationValue::CustomPropertyFallback(transform_name, fallback)
        ))
    );
    assert_eq!(
        parse_text_transform_property("var(--transform, var(--other))"),
        None
    );
}

#[test]
fn font_weight_parser_accepts_the_css_numeric_range() {
    assert_eq!(parse_font_weight("normal"), Some(FontWeightValue::Normal));
    assert_eq!(parse_font_weight("400"), Some(FontWeightValue::Normal));
    assert_eq!(parse_font_weight("BOLD"), Some(FontWeightValue::Bold));
    assert_eq!(parse_font_weight("700"), Some(FontWeightValue::Bold));
    assert_eq!(
        parse_font_weight("500"),
        Some(FontWeightValue::Numeric(500))
    );
    assert_eq!(parse_font_weight("1"), Some(FontWeightValue::Numeric(1)));
    assert_eq!(
        parse_font_weight("1000"),
        Some(FontWeightValue::Numeric(1000))
    );
    assert_eq!(parse_font_weight("0"), None);
    assert_eq!(parse_font_weight("1001"), None);
    assert_eq!(parse_font_weight("500.5"), None);
    assert_eq!(parse_font_weight("lighter"), None);
    assert_eq!(parse_font_weight("700 800"), None);
    assert_eq!(parse_font_weight("initial"), None);
}

#[test]
fn font_weight_property_parser_accepts_relative_keywords_only_at_property_level() {
    assert_eq!(
        parse_font_weight_property("LIGHTER"),
        Some(FontWeightDeclarationValue::Lighter)
    );
    assert_eq!(
        parse_font_weight_property("bolder"),
        Some(FontWeightDeclarationValue::Bolder)
    );
    assert_eq!(
        parse_font_weight_property("500"),
        Some(FontWeightDeclarationValue::Absolute(
            FontWeightValue::Numeric(500)
        ))
    );
    assert_eq!(parse_font_weight_property("lighter bolder"), None);
    assert_eq!(parse_font_weight("lighter"), None);
    let weight_name = parse_custom_property_name("--weight").unwrap();
    assert_eq!(
        parse_font_weight_property("var(--weight)"),
        Some(FontWeightDeclarationValue::CustomProperty(weight_name))
    );
    assert_eq!(
        parse_font_weight_declaration("VAR(--weight, 700)"),
        Some(InheritedTextDeclaration::Value(
            FontWeightDeclarationValue::CustomPropertyFallback(weight_name, FontWeightValue::Bold)
        ))
    );
    assert_eq!(
        parse_font_weight_property("var(--weight, var(--other))"),
        None
    );
}

#[test]
fn relative_font_weight_uses_css_parent_weight_bands() {
    let cases = [
        (FontWeightValue::Numeric(50), 50, 400),
        (FontWeightValue::Numeric(100), 100, 400),
        (FontWeightValue::Numeric(349), 100, 400),
        (FontWeightValue::Numeric(350), 100, 700),
        (FontWeightValue::Numeric(549), 100, 700),
        (FontWeightValue::Numeric(550), 400, 900),
        (FontWeightValue::Numeric(749), 400, 900),
        (FontWeightValue::Numeric(750), 700, 900),
        (FontWeightValue::Numeric(899), 700, 900),
        (FontWeightValue::Numeric(900), 700, 900),
        (FontWeightValue::Numeric(1000), 700, 1000),
    ];
    for (inherited, lighter, bolder) in cases {
        assert_eq!(relative_font_weight(inherited, false).numeric(), lighter);
        assert_eq!(relative_font_weight(inherited, true).numeric(), bolder);
    }
}

#[test]
fn font_style_parser_accepts_only_normal_and_italic() {
    assert_eq!(parse_font_style("normal"), Some(FontStyleValue::Normal));
    assert_eq!(parse_font_style("ITALIC"), Some(FontStyleValue::Italic));
    assert_eq!(parse_font_style("oblique"), None);
    assert_eq!(parse_font_style("12deg"), None);
    assert_eq!(parse_font_style("initial"), None);
    let style_name = parse_custom_property_name("--style").unwrap();
    assert_eq!(
        parse_font_style_property("var(--style)"),
        Some(FontStyleDeclarationValue::CustomProperty(style_name))
    );
    assert_eq!(
        parse_font_style_declaration("var(--style, italic)"),
        Some(InheritedTextDeclaration::Value(
            FontStyleDeclarationValue::CustomPropertyFallback(style_name, FontStyleValue::Italic)
        ))
    );
    assert_eq!(
        parse_font_style_property("var(--style, var(--other))"),
        None
    );
}

#[test]
fn font_stretch_parser_normalizes_keywords_percentages_and_ranges() {
    assert_eq!(
        parse_font_stretch_range("condensed"),
        Some(NativeFontStretchRange { min: 750, max: 750 })
    );
    assert_eq!(
        parse_font_stretch_range("62.5% 125%"),
        Some(NativeFontStretchRange {
            min: 625,
            max: 1250
        })
    );
    assert_eq!(
        format_font_stretch_range(NativeFontStretchRange {
            min: 625,
            max: 1250,
        }),
        "62.5% 125%"
    );
    assert!(parse_font_stretch_range("201%").is_none());
    assert!(parse_font_stretch_range("125% 62.5%").is_none());
    assert!(parse_font_stretch_range("normal expanded extra-expanded").is_none());
    let stretch_name = parse_custom_property_name("--stretch").unwrap();
    assert_eq!(
        parse_font_stretch_property("var(--stretch)"),
        Some(FontStretchDeclarationValue::CustomProperty(stretch_name))
    );
    assert_eq!(
        parse_font_stretch_declaration("var(--stretch, 75%)"),
        Some(InheritedTextDeclaration::Value(
            FontStretchDeclarationValue::CustomPropertyFallback(
                stretch_name,
                NativeFontStretchRange { min: 750, max: 750 }
            )
        ))
    );
    assert_eq!(
        parse_font_stretch_property("var(--stretch, var(--other))"),
        None
    );
}

#[test]
fn font_variant_ligatures_parser_keeps_one_value_per_feature_group() {
    let normal = NativeFontVariantLigatures::default();
    assert_eq!(parse_font_variant_ligatures("normal"), Some(normal));
    assert_eq!(
        parse_font_variant_ligatures("none"),
        Some(NativeFontVariantLigatures {
            common: false,
            discretionary: false,
            historical: false,
            contextual: false,
        })
    );
    assert_eq!(
        parse_font_variant_ligatures(
            "no-common-ligatures discretionary-ligatures historical-ligatures no-contextual"
        ),
        Some(NativeFontVariantLigatures {
            common: false,
            discretionary: true,
            historical: true,
            contextual: false,
        })
    );
    assert!(parse_font_variant_ligatures("common-ligatures no-common-ligatures").is_none());
    assert!(parse_font_variant_ligatures("normal common-ligatures").is_none());
    assert!(parse_font_variant_ligatures("none contextual").is_none());
    assert!(parse_font_variant_ligatures("unknown").is_none());
    let ligature_name = parse_custom_property_name("--ligatures").unwrap();
    let fallback = parse_font_variant_ligatures("none").unwrap();
    assert_eq!(
        parse_font_variant_ligatures_property("var(--ligatures)"),
        Some(FontVariantLigaturesDeclarationValue::CustomProperty(
            ligature_name
        ))
    );
    assert_eq!(
        parse_font_variant_ligatures_declaration("var(--ligatures, none)"),
        Some(InheritedTextDeclaration::Value(
            FontVariantLigaturesDeclarationValue::CustomPropertyFallback(ligature_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_variant_ligatures_property("var(--ligatures, var(--other))"),
        None
    );
}

#[test]
fn font_feature_settings_parser_keeps_bounded_last_values() {
    let settings =
        parse_font_feature_settings(r#""liga" off, "kern" 0, "liga" on, "dlig""#).unwrap();
    assert_eq!(settings.count, 3);
    assert_eq!(settings.values[0].tag, *b"liga");
    assert_eq!(settings.values[0].value, 1);
    assert_eq!(settings.values[1].tag, *b"kern");
    assert_eq!(settings.values[1].value, 0);
    assert_eq!(settings.values[2].tag, *b"dlig");
    assert_eq!(settings.values[2].value, 1);
    assert_eq!(
        parse_font_feature_settings("normal"),
        Some(NativeFontFeatureSettings::default())
    );
    assert!(parse_font_feature_settings("liga 0").is_none());
    assert!(parse_font_feature_settings(r#""lig" 0"#).is_none());
    assert!(parse_font_feature_settings(r#""liga" -1"#).is_none());
    assert!(parse_font_feature_settings(r#""liga" 65536"#).is_none());
    assert!(parse_font_feature_settings(r#""liga" 1 extra"#).is_none());
    let feature_name = parse_custom_property_name("--features").unwrap();
    let fallback = parse_font_feature_settings(r#""liga" off, "kern" on"#).unwrap();
    assert_eq!(
        parse_font_feature_settings_property("var(--features)"),
        Some(FontFeatureSettingsDeclarationValue::CustomProperty(
            feature_name
        ))
    );
    assert_eq!(
        parse_font_feature_settings_declaration(r#"var(--features, "liga" off, "kern" on)"#),
        Some(InheritedTextDeclaration::Value(
            FontFeatureSettingsDeclarationValue::CustomPropertyFallback(feature_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_feature_settings_property("var(--features, var(--other))"),
        None
    );
}
#[test]
fn font_face_variant_descriptor_maps_to_bounded_features() {
    let settings = parse_font_face_variant(" SMALL-CAPS proportional-nums RUBY ").unwrap();
    assert_eq!(settings.count, 3);
    assert_eq!(
        settings.values[0],
        NativeFontFeature {
            tag: *b"smcp",
            value: 1
        }
    );
    assert_eq!(
        settings.values[1],
        NativeFontFeature {
            tag: *b"pnum",
            value: 1
        }
    );
    assert_eq!(
        settings.values[2],
        NativeFontFeature {
            tag: *b"ruby",
            value: 1
        }
    );
    let explicit = parse_font_feature_settings(r#""smcp" off"#).unwrap();
    let merged = settings.with_overrides(explicit);
    assert_eq!(merged.count, 3);
    assert_eq!(merged.values[0].tag, *b"smcp");
    assert_eq!(merged.values[0].value, 0);
    assert_eq!(
        parse_font_face_variant("normal"),
        Some(NativeFontFeatureSettings::default())
    );
    assert!(parse_font_face_variant("none").is_none());
    assert!(parse_font_face_variant("small-caps all-small-caps").is_none());
    assert!(parse_font_face_variant("small-caps made-up").is_none());
}

#[test]
fn font_variation_settings_parser_keeps_bounded_last_values() {
    let settings =
        parse_font_variation_settings(r#""wght" 450.5, "wdth" -12.25, "wght" 700"#).unwrap();
    assert_eq!(settings.count, 2);
    assert_eq!(settings.values[0].tag, *b"wght");
    assert_eq!(settings.values[0].value_milli, 700_000);
    assert_eq!(settings.values[1].tag, *b"wdth");
    assert_eq!(settings.values[1].value_milli, -12_250);
    assert_eq!(
        parse_font_variation_settings("normal"),
        Some(NativeFontVariationSettings::default())
    );
    assert!(parse_font_variation_settings(r#"wght 700"#).is_none());
    assert!(parse_font_variation_settings(r#""wgt" 700"#).is_none());
    assert!(parse_font_variation_settings(r#""wght" 32768.001"#).is_none());
    assert!(parse_font_variation_settings(r#""wght" 700 escape"#).is_none());
    assert!(parse_font_variation_settings(r#""wght" 700, "#).is_none());
    assert!(parse_font_variation_settings(r#""wg\,t" 700"#).is_none());
    assert_eq!(
        format_font_variation_settings(settings),
        r#""wght" 700, "wdth" -12.25"#
    );
    let merged = parse_font_variation_settings(r#""wght" 500, "opsz" 12"#)
        .unwrap()
        .with_overrides(settings);
    assert_eq!(
        format_font_variation_settings(merged),
        r#""wght" 700, "wdth" -12.25, "opsz" 12"#
    );
    let variation_name = parse_custom_property_name("--variations").unwrap();
    let fallback = parse_font_variation_settings(r#""wght" 550, "wdth" 90"#).unwrap();
    assert_eq!(
        parse_font_variation_settings_property("var(--variations)"),
        Some(FontVariationSettingsDeclarationValue::CustomProperty(
            variation_name
        ))
    );
    assert_eq!(
        parse_font_variation_settings_declaration(r#"var(--variations, "wght" 550, "wdth" 90)"#),
        Some(InheritedTextDeclaration::Value(
            FontVariationSettingsDeclarationValue::CustomPropertyFallback(variation_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_variation_settings_property("var(--variations, var(--other))"),
        None
    );
}

#[test]
fn font_kerning_parser_accepts_only_supported_keywords() {
    assert_eq!(parse_font_kerning("auto"), Some(NativeFontKerning::Auto));
    assert_eq!(
        parse_font_kerning("NORMAL"),
        Some(NativeFontKerning::Normal)
    );
    assert_eq!(parse_font_kerning("none"), Some(NativeFontKerning::None));
    assert!(parse_font_kerning("inherit").is_none());
    assert!(parse_font_kerning("normal none").is_none());
    let kerning_name = parse_custom_property_name("--kerning").unwrap();
    assert_eq!(
        parse_font_kerning_property("var(--kerning)"),
        Some(FontKerningDeclarationValue::CustomProperty(kerning_name))
    );
    assert_eq!(
        parse_font_kerning_declaration("var(--kerning, none)"),
        Some(InheritedTextDeclaration::Value(
            FontKerningDeclarationValue::CustomPropertyFallback(
                kerning_name,
                NativeFontKerning::None
            )
        ))
    );
    assert_eq!(
        parse_font_kerning_property("var(--kerning, var(--other))"),
        None
    );
}

#[test]
fn font_optical_sizing_parser_accepts_only_supported_keywords() {
    assert_eq!(
        parse_font_optical_sizing("auto"),
        Some(NativeFontOpticalSizing::Auto)
    );
    assert_eq!(
        parse_font_optical_sizing("NONE"),
        Some(NativeFontOpticalSizing::None)
    );
    assert!(parse_font_optical_sizing("inherit").is_none());
    assert!(parse_font_optical_sizing("auto none").is_none());
    assert!(parse_font_optical_sizing("on").is_none());
    let sizing_name = parse_custom_property_name("--sizing").unwrap();
    assert_eq!(
        parse_font_optical_sizing_property("var(--sizing)"),
        Some(FontOpticalSizingDeclarationValue::CustomProperty(
            sizing_name
        ))
    );
    assert_eq!(
        parse_font_optical_sizing_declaration("var(--sizing, none)"),
        Some(InheritedTextDeclaration::Value(
            FontOpticalSizingDeclarationValue::CustomPropertyFallback(
                sizing_name,
                NativeFontOpticalSizing::None
            )
        ))
    );
    assert_eq!(
        parse_font_optical_sizing_property("var(--sizing, var(--other))"),
        None
    );
}

#[test]
fn font_palette_parser_accepts_keywords_and_custom_names() {
    assert_eq!(
        parse_font_palette("normal"),
        Some(NativeFontPalette::Normal)
    );
    assert_eq!(parse_font_palette("LIGHT"), Some(NativeFontPalette::Light));
    assert_eq!(parse_font_palette("dark"), Some(NativeFontPalette::Dark));
    assert!(matches!(
        parse_font_palette("--brand"),
        Some(NativeFontPalette::Named(_))
    ));
    assert!(parse_font_palette("palette-one").is_none());
    assert!(parse_font_palette("normal light").is_none());
    let palette_name = parse_custom_property_name("--palette").unwrap();
    let fallback = parse_font_palette("--brand").unwrap();
    assert_eq!(
        parse_font_palette_property("var(--palette)"),
        Some(FontPaletteDeclarationValue::CustomProperty(palette_name))
    );
    assert_eq!(
        parse_font_palette_declaration("var(--palette, --brand)"),
        Some(InheritedTextDeclaration::Value(
            FontPaletteDeclarationValue::CustomPropertyFallback(palette_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_palette_property("var(--palette, var(--other))"),
        None
    );
}

#[test]
fn font_variant_caps_parser_accepts_only_supported_keywords() {
    assert_eq!(
        parse_font_variant_caps("normal"),
        Some(NativeFontVariantCaps::Normal)
    );
    assert_eq!(
        parse_font_variant_caps("ALL-SMALL-CAPS"),
        Some(NativeFontVariantCaps::AllSmallCaps)
    );
    assert_eq!(
        parse_font_variant_caps("petite-caps"),
        Some(NativeFontVariantCaps::PetiteCaps)
    );
    assert_eq!(
        parse_font_variant_caps("all-petite-caps"),
        Some(NativeFontVariantCaps::AllPetiteCaps)
    );
    assert_eq!(
        parse_font_variant_caps("unicase"),
        Some(NativeFontVariantCaps::Unicase)
    );
    assert_eq!(
        parse_font_variant_caps("titling-caps"),
        Some(NativeFontVariantCaps::TitlingCaps)
    );
    assert!(parse_font_variant_caps("inherit").is_none());
    assert!(parse_font_variant_caps("small-caps all-small-caps").is_none());
    assert!(parse_font_variant_caps("none").is_none());
    let caps_name = parse_custom_property_name("--caps").unwrap();
    let fallback = NativeFontVariantCaps::AllSmallCaps;
    assert_eq!(
        parse_font_variant_caps_property("var(--caps)"),
        Some(FontVariantCapsDeclarationValue::CustomProperty(caps_name))
    );
    assert_eq!(
        parse_font_variant_caps_declaration("var(--caps, all-small-caps)"),
        Some(InheritedTextDeclaration::Value(
            FontVariantCapsDeclarationValue::CustomPropertyFallback(caps_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_variant_caps_property("var(--caps, var(--other))"),
        None
    );
}

#[test]
fn font_variant_position_parser_accepts_only_supported_keywords() {
    assert_eq!(
        parse_font_variant_position("normal"),
        Some(NativeFontVariantPosition::Normal)
    );
    assert_eq!(
        parse_font_variant_position("SUB"),
        Some(NativeFontVariantPosition::Sub)
    );
    assert_eq!(
        parse_font_variant_position("super"),
        Some(NativeFontVariantPosition::Super)
    );
    assert!(parse_font_variant_position("inherit").is_none());
    assert!(parse_font_variant_position("sub super").is_none());
    assert!(parse_font_variant_position("none").is_none());
    let position_name = parse_custom_property_name("--position").unwrap();
    let fallback = NativeFontVariantPosition::Super;
    assert_eq!(
        parse_font_variant_position_property("var(--position)"),
        Some(FontVariantPositionDeclarationValue::CustomProperty(
            position_name
        ))
    );
    assert_eq!(
        parse_font_variant_position_declaration("var(--position, super)"),
        Some(InheritedTextDeclaration::Value(
            FontVariantPositionDeclarationValue::CustomPropertyFallback(position_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_variant_position_property("var(--position, var(--other))"),
        None
    );
}

#[test]
fn font_variant_alternates_parser_accepts_only_bounded_keywords() {
    assert_eq!(
        parse_font_variant_alternates("normal"),
        Some(NativeFontVariantAlternates::Normal)
    );
    assert_eq!(
        parse_font_variant_alternates("HISTORICAL-FORMS"),
        Some(NativeFontVariantAlternates::HistoricalForms)
    );
    assert!(parse_font_variant_alternates("inherit").is_none());
    assert!(parse_font_variant_alternates("normal historical-forms").is_none());
    assert!(parse_font_variant_alternates("stylistic(foo)").is_none());
    let alternates_name = parse_custom_property_name("--alternates").unwrap();
    let fallback = NativeFontVariantAlternates::HistoricalForms;
    assert_eq!(
        parse_font_variant_alternates_property("var(--alternates)"),
        Some(FontVariantAlternatesDeclarationValue::CustomProperty(
            alternates_name
        ))
    );
    assert_eq!(
        parse_font_variant_alternates_declaration("var(--alternates, historical-forms)"),
        Some(InheritedTextDeclaration::Value(
            FontVariantAlternatesDeclarationValue::CustomPropertyFallback(
                alternates_name,
                fallback
            )
        ))
    );
    assert_eq!(
        parse_font_variant_alternates_property("var(--alternates, var(--other))"),
        None
    );
}

#[test]
fn font_language_override_parser_accepts_padded_four_byte_tags() {
    assert_eq!(
        parse_font_language_override("normal"),
        Some(NativeFontLanguageOverride::default())
    );
    assert_eq!(
        parse_font_language_override("\"ENG \""),
        Some(NativeFontLanguageOverride {
            tag: Some(*b"ENG "),
        })
    );
    assert_eq!(
        parse_font_language_override("'TRK '"),
        Some(NativeFontLanguageOverride {
            tag: Some(*b"TRK "),
        })
    );
    assert!(parse_font_language_override("ENG ").is_none());
    assert!(parse_font_language_override("\"EN G\"").is_none());
    assert!(parse_font_language_override("\"    \"").is_none());
    assert!(parse_font_language_override_declaration("inherit").is_some());
    assert!(parse_font_language_override_declaration("revert-layer").is_some());
    let language_name = parse_custom_property_name("--language").unwrap();
    let fallback = NativeFontLanguageOverride {
        tag: Some(*b"TRK "),
    };
    assert_eq!(
        parse_font_language_override_property("var(--language)"),
        Some(FontLanguageOverrideDeclarationValue::CustomProperty(
            language_name
        ))
    );
    assert_eq!(
        parse_font_language_override_declaration("var(--language, 'TRK ')"),
        Some(InheritedTextDeclaration::Value(
            FontLanguageOverrideDeclarationValue::CustomPropertyFallback(language_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_language_override_property("var(--language, var(--other))"),
        None
    );
}

#[test]
fn font_variant_east_asian_parser_keeps_feature_groups_exclusive() {
    assert_eq!(
        parse_font_variant_east_asian("normal"),
        Some(NativeFontVariantEastAsian::default())
    );
    assert_eq!(
        parse_font_variant_east_asian("JIS83 FULL-WIDTH ruby"),
        Some(NativeFontVariantEastAsian {
            form: NativeFontVariantEastAsianForm::Jis83,
            width: NativeFontVariantEastAsianWidth::Full,
            ruby: true,
        })
    );
    assert_eq!(
        parse_font_variant_east_asian("traditional proportional-width"),
        Some(NativeFontVariantEastAsian {
            form: NativeFontVariantEastAsianForm::Traditional,
            width: NativeFontVariantEastAsianWidth::Proportional,
            ruby: false,
        })
    );
    assert!(parse_font_variant_east_asian("normal ruby").is_none());
    assert!(parse_font_variant_east_asian("jis78 jis90").is_none());
    assert!(parse_font_variant_east_asian("full-width proportional-width").is_none());
    assert!(parse_font_variant_east_asian("ruby ruby").is_none());
    assert!(parse_font_variant_east_asian("oldstyle-nums").is_none());
    let east_asian_name = parse_custom_property_name("--east-asian").unwrap();
    let fallback = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Jis90,
        width: NativeFontVariantEastAsianWidth::Full,
        ruby: true,
    };
    assert_eq!(
        parse_font_variant_east_asian_property("var(--east-asian)"),
        Some(FontVariantEastAsianDeclarationValue::CustomProperty(
            east_asian_name
        ))
    );
    assert_eq!(
        parse_font_variant_east_asian_declaration("var(--east-asian, jis90 full-width ruby)"),
        Some(InheritedTextDeclaration::Value(
            FontVariantEastAsianDeclarationValue::CustomPropertyFallback(east_asian_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_variant_east_asian_property("var(--east-asian, var(--other))"),
        None
    );
}

#[test]
fn font_variant_shorthand_expands_existing_font_variant_controls() {
    let parsed = parse_font_variant_shorthand(
            "no-common-ligatures unicase super historical-forms jis83 full-width ruby oldstyle-nums tabular-nums",
        )
        .unwrap();
    assert_eq!(
        parsed.ligatures,
        InheritedTextDeclaration::Value(NativeFontVariantLigatures {
            common: false,
            discretionary: false,
            historical: false,
            contextual: true,
        })
    );
    assert_eq!(
        parsed.caps,
        InheritedTextDeclaration::Value(NativeFontVariantCaps::Unicase)
    );
    assert_eq!(
        parsed.position,
        InheritedTextDeclaration::Value(NativeFontVariantPosition::Super)
    );
    assert_eq!(
        parsed.alternates,
        InheritedTextDeclaration::Value(NativeFontVariantAlternates::HistoricalForms)
    );
    assert_eq!(
        parsed.east_asian,
        InheritedTextDeclaration::Value(NativeFontVariantEastAsian {
            form: NativeFontVariantEastAsianForm::Jis83,
            width: NativeFontVariantEastAsianWidth::Full,
            ruby: true,
        })
    );
    assert!(matches!(
        parsed.numeric,
        InheritedTextDeclaration::Value(NativeFontVariantNumeric {
            figure: NativeFontVariantNumericFigure::Oldstyle,
            spacing: NativeFontVariantNumericSpacing::Tabular,
            fraction: NativeFontVariantNumericFraction::Normal,
            ordinal: false,
            slashed_zero: false,
        })
    ));
    assert!(matches!(
        parse_font_variant_shorthand("inherit").unwrap().caps,
        InheritedTextDeclaration::Inherit
    ));
    assert!(parse_font_variant_shorthand("normal unicase").is_none());
    assert!(parse_font_variant_shorthand("none ruby").is_none());
    assert!(parse_font_variant_shorthand("small-caps small-caps").is_none());
    assert!(parse_font_variant_shorthand("unknown").is_none());
    let variant_name = parse_custom_property_name("--variant").unwrap();
    let custom = parse_font_variant_shorthand_custom_property("var(--variant)").unwrap();
    assert!(matches!(
        custom.ligatures,
        InheritedTextDeclaration::Value(
            FontVariantLigaturesDeclarationValue::CustomProperty(name)
        ) if name == variant_name
    ));
    let fallback = parse_font_variant_shorthand_custom_property(
            "var(--variant, no-common-ligatures unicase super historical-forms jis83 full-width ruby oldstyle-nums)",
        )
        .unwrap();
    assert!(matches!(
        fallback.numeric,
        InheritedTextDeclaration::Value(
            FontVariantNumericDeclarationValue::CustomPropertyFallback(name, _)
        ) if name == variant_name
    ));
    assert!(parse_font_variant_shorthand_custom_property("var(--variant, var(--other))").is_none());
}

#[test]
fn font_variant_numeric_parser_keeps_one_value_per_group() {
    assert_eq!(
        parse_font_variant_numeric("normal"),
        Some(NativeFontVariantNumeric::default())
    );
    assert_eq!(
        parse_font_variant_numeric(
            "oldstyle-nums tabular-nums stacked-fractions ordinal slashed-zero"
        ),
        Some(NativeFontVariantNumeric {
            figure: NativeFontVariantNumericFigure::Oldstyle,
            spacing: NativeFontVariantNumericSpacing::Tabular,
            fraction: NativeFontVariantNumericFraction::Stacked,
            ordinal: true,
            slashed_zero: true,
        })
    );
    assert_eq!(
        parse_font_variant_numeric("LINING-NUMS PROPORTIONAL-NUMS DIAGONAL-FRACTIONS"),
        Some(NativeFontVariantNumeric {
            figure: NativeFontVariantNumericFigure::Lining,
            spacing: NativeFontVariantNumericSpacing::Proportional,
            fraction: NativeFontVariantNumericFraction::Diagonal,
            ordinal: false,
            slashed_zero: false,
        })
    );
    assert!(parse_font_variant_numeric("normal ordinal").is_none());
    assert!(parse_font_variant_numeric("lining-nums oldstyle-nums").is_none());
    assert!(parse_font_variant_numeric("ordinal ordinal").is_none());
    assert!(parse_font_variant_numeric("unknown").is_none());
    let numeric_name = parse_custom_property_name("--numeric").unwrap();
    let fallback = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Lining,
        spacing: NativeFontVariantNumericSpacing::Tabular,
        fraction: NativeFontVariantNumericFraction::Stacked,
        ordinal: true,
        slashed_zero: true,
    };
    assert_eq!(
        parse_font_variant_numeric_property("var(--numeric)"),
        Some(FontVariantNumericDeclarationValue::CustomProperty(
            numeric_name
        ))
    );
    assert_eq!(
        parse_font_variant_numeric_declaration(
            "var(--numeric, lining-nums tabular-nums stacked-fractions ordinal slashed-zero)"
        ),
        Some(InheritedTextDeclaration::Value(
            FontVariantNumericDeclarationValue::CustomPropertyFallback(numeric_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_variant_numeric_property("var(--numeric, var(--other))"),
        None
    );
}

#[test]
fn font_family_parser_keeps_ordered_bounded_fallbacks() {
    let families = parse_font_family("\"Missing Face\", sans-serif, monospace").unwrap();
    assert_eq!(families.len, 3);
    assert!(matches!(
        families.values[0],
        NativeFontFamilyValue::Named(_)
    ));
    assert_eq!(
        families.values[1],
        NativeFontFamilyValue::Generic(NativeGenericFontFamily::SansSerif)
    );
    assert_eq!(
        families.values[2],
        NativeFontFamilyValue::Generic(NativeGenericFontFamily::Monospace)
    );
    assert!(parse_font_family("sans-serif, ").is_none());
    assert!(parse_font_family("'unterminated").is_none());
    assert!(parse_font_family(", sans-serif").is_none());
    let family_name = parse_custom_property_name("--family").unwrap();
    let fallback = parse_font_family("\"Fallback\", serif").unwrap();
    assert_eq!(
        parse_font_family_property("var(--family)"),
        Some(FontFamilyDeclarationValue::CustomProperty(family_name))
    );
    assert_eq!(
        parse_font_family_declaration("var(--family, \"Fallback\", serif)"),
        Some(InheritedTextDeclaration::Value(
            FontFamilyDeclarationValue::CustomPropertyFallback(family_name, fallback)
        ))
    );
    assert_eq!(
        parse_font_family_property("var(--family, var(--other))"),
        None
    );
}

#[test]
fn font_face_parser_collects_named_data_sources_without_creating_style_rules() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        r#"@font-face {
                font-family: "Embedded Sans";
                src: local("missing"), url("data:font/ttf;base64,AAECAw==") format("truetype");
                font-weight: 700;
                font-style: italic;
                font-stretch: condensed;
                font-variation-settings: "wght" 450.5, "wdth" -12.25, "wght" 700;
                font-feature-settings: "liga" off, "kern" on;
                size-adjust: 62.5%;
                ascent-override: 80%;
                descent-override: 20%;
                line-gap-override: normal;
                font-display: SWAP;
            }
            #text { font-family: "Embedded Sans"; }"#
            .into(),
    ])
    .unwrap();
    assert_eq!(stylesheet.font_face_rules().len(), 1);
    let rule = &stylesheet.font_face_rules()[0];
    assert_eq!(rule.family, "Embedded Sans");
    assert_eq!(rule.family_key, font_family_hash("Embedded Sans"));
    assert_eq!(
        rule.sources,
        vec![
            NativeFontFaceSource::Local("missing".into()),
            NativeFontFaceSource::Url("data:font/ttf;base64,AAECAw==".into()),
        ]
    );
    assert_eq!(
        rule.weight,
        NativeFontWeightRange::singleton(FontWeightValue::Bold)
    );
    assert_eq!(rule.style, FontStyleValue::Italic);
    assert_eq!(rule.stretch, NativeFontStretchRange { min: 750, max: 750 });
    assert_eq!(rule.size_adjust, 625);
    assert_eq!(
        rule.metric_overrides,
        NativeFontMetricOverrides {
            ascent: Some(800),
            descent: Some(200),
            line_gap: None,
        }
    );
    assert_eq!(rule.font_display, NativeFontDisplay::Swap);
    assert_eq!(
        format_font_variation_settings(rule.variation_settings),
        r#""wght" 700, "wdth" -12.25"#
    );
    assert_eq!(
        format_font_feature_settings(rule.feature_settings),
        r#""liga" 0, "kern" 1"#
    );
    assert_eq!(format_font_size_adjust(rule.size_adjust), "62.5%");
    assert_eq!(stylesheet.rules.len(), 1);
}

#[test]
fn font_size_adjust_parser_enforces_percentage_bounds() {
    assert_eq!(parse_font_size_adjust("25%"), Some(250));
    assert_eq!(parse_font_size_adjust("62.5%"), Some(625));
    assert_eq!(parse_font_size_adjust("400%"), Some(4000));
    assert_eq!(parse_font_size_adjust("24.9%"), None);
    assert_eq!(parse_font_size_adjust("400.1%"), None);
    assert_eq!(parse_font_size_adjust("62.5"), None);
    assert_eq!(parse_font_size_adjust("62.5555%"), None);
    assert_eq!(format_font_size_adjust(250), "25%");
    assert_eq!(format_font_size_adjust(4000), "400%");
}

#[test]
fn font_metric_override_parser_accepts_normal_and_bounded_percentages() {
    assert_eq!(parse_font_metric_override("normal"), Some(None));
    assert_eq!(parse_font_metric_override("0%"), Some(Some(0)));
    assert_eq!(parse_font_metric_override("62.5%"), Some(Some(625)));
    assert_eq!(parse_font_metric_override("1000%"), Some(Some(10_000)));
    assert_eq!(parse_font_metric_override("1000.1%"), None);
    assert_eq!(parse_font_metric_override("62.5"), None);
    assert_eq!(parse_font_metric_override("62.5555%"), None);
    assert_eq!(format_font_metric_override(None), "normal");
    assert_eq!(format_font_metric_override(Some(625)), "62.5%");
    assert_eq!(format_font_metric_override(Some(10_000)), "1000%");
}

#[test]
fn font_display_parser_accepts_only_single_normalized_keywords() {
    assert_eq!(parse_font_display("AUTO"), Some(NativeFontDisplay::Auto));
    assert_eq!(parse_font_display("block"), Some(NativeFontDisplay::Block));
    assert_eq!(parse_font_display(" Swap "), Some(NativeFontDisplay::Swap));
    assert_eq!(
        parse_font_display("fallback"),
        Some(NativeFontDisplay::Fallback)
    );
    assert_eq!(
        parse_font_display("optional"),
        Some(NativeFontDisplay::Optional)
    );
    assert!(parse_font_display("swap optional").is_none());
    assert!(parse_font_display("unknown").is_none());
}

#[test]
fn font_face_weight_range_parser_accepts_absolute_singletons_and_ranges() {
    assert_eq!(
        parse_font_weight_range("normal"),
        Some(NativeFontWeightRange::singleton(FontWeightValue::Normal))
    );
    assert_eq!(
        parse_font_weight_range("300 700"),
        Some(NativeFontWeightRange { min: 300, max: 700 })
    );
    assert_eq!(
        parse_font_weight_range("BOLD 900"),
        Some(NativeFontWeightRange { min: 700, max: 900 })
    );
    assert_eq!(
        format_font_weight_range(NativeFontWeightRange { min: 300, max: 700 }),
        "300 700"
    );
    assert!(parse_font_weight_range("lighter").is_none());
    assert!(parse_font_weight_range("300 700 900").is_none());
    assert!(parse_font_weight_range("700 300").is_none());
    assert!(parse_font_weight_range("0 400").is_none());
    assert!(parse_font_weight_range("400 1001").is_none());
}

#[test]
fn font_face_parser_retains_numeric_weight_ranges() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        r#"@font-face {
                font-family: "Range Face";
                src: url("data:font/ttf;base64,AAECAw==");
                font-weight: 300 700;
            }"#
        .into(),
    ])
    .unwrap();
    assert_eq!(
        stylesheet.font_face_rules()[0].weight,
        NativeFontWeightRange { min: 300, max: 700 }
    );
    assert_eq!(
        format_font_weight_range(stylesheet.font_face_rules()[0].weight),
        "300 700"
    );
}

#[test]
fn font_face_unicode_range_parser_normalizes_hex_wildcards_and_merges() {
    let stylesheet = NativeStylesheet::from_sources(vec![
        r#"@font-face {
                font-family: "Range Face";
                src: url("data:font/ttf;base64,AAECAw==");
                unicode-range: U+0041, U+42-43, U+0044, u+4??, U+0045;
            }"#
        .into(),
    ])
    .unwrap();
    assert_eq!(
        stylesheet.font_face_rules()[0].unicode_ranges,
        vec![
            NativeUnicodeRange {
                start: 0x41,
                end: 0x45,
            },
            NativeUnicodeRange {
                start: 0x400,
                end: 0x4ff,
            },
        ]
    );
    assert_eq!(
        format_font_face_unicode_ranges(&stylesheet.font_face_rules()[0].unicode_ranges),
        "U+41-45, U+400-4FF"
    );
    assert!(parse_font_face_unicode_range("U+110000").is_none());
    assert!(parse_font_face_unicode_range("U+4?A").is_none());
    assert!(parse_font_face_unicode_range("U+0-FFFF, ").is_none());
}

#[test]
fn font_face_parser_rejects_generic_families_but_keeps_local_sources() {
    let mut diagnostics = NativeDiagnosticSink::default();
    let stylesheet = NativeStylesheet::from_sources_with_diagnostics(
        vec![
            "@font-face { font-family: sans-serif; src: url(font.ttf); }".into(),
            "@font-face { font-family: Embedded; src: url(font.ttf); }".into(),
            "@font-face { font-family: Embedded; src: local(\"missing\"); }".into(),
        ],
        &mut diagnostics,
    )
    .unwrap();
    assert_eq!(stylesheet.font_face_rules().len(), 2);
    assert_eq!(
        stylesheet.font_face_rules()[0].sources,
        vec![NativeFontFaceSource::Url("font.ttf".into())]
    );
    assert_eq!(
        stylesheet.font_face_rules()[1].sources,
        vec![NativeFontFaceSource::Local("missing".into())]
    );
    let (diagnostics, _) = diagnostics.finish();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == NativeDiagnosticCode::UnsupportedCssValue
            && diagnostic.detail == "font-face-family"
    }));
}

#[test]
fn font_size_parser_accepts_bounded_absolute_units() {
    assert_eq!(parse_font_size("16px"), Some(16));
    assert_eq!(parse_font_size("9pt"), Some(12));
    assert_eq!(parse_font_size("0.25in"), Some(24));
    assert_eq!(parse_font_size("1pc"), Some(16));
    assert_eq!(parse_font_size("1in"), Some(96));
    assert_eq!(parse_font_size("1cm"), Some(38));
    assert_eq!(parse_font_size("1mm"), Some(4));
    assert_eq!(parse_font_size("256px"), Some(256));
    assert_eq!(parse_font_size("16.5px"), Some(17));
    assert_eq!(parse_font_size("16.499px"), Some(16));
    assert_eq!(parse_font_size("0.499px"), None);
    assert_eq!(parse_font_size("0.5px"), Some(1));
    assert_eq!(parse_font_size("256.499px"), Some(256));
    assert_eq!(parse_font_size("256.5px"), None);
    assert_eq!(parse_font_size("1.0000px"), Some(1));
    assert_eq!(parse_font_size("0px"), None);
    assert_eq!(parse_font_size("257px"), None);
    assert_eq!(parse_font_size("257pt"), None);
    assert_eq!(parse_font_size("1.0000pt"), Some(2));
    assert_eq!(parse_font_size("16rem"), None);
    assert_eq!(parse_font_size("50%"), None);
    assert_eq!(parse_font_size("1.5em"), None);
}

#[test]
fn font_size_parser_accepts_extra_decimal_precision() {
    assert_eq!(parse_font_size_decimal_milli("1.000499"), Some(1_000));
    assert_eq!(parse_font_size_decimal_milli("1.0005"), Some(1_001));
    assert_eq!(parse_font_size_decimal_milli("1.0000001"), None);
    assert_eq!(
        parse_font_size_value("1.0005em"),
        Some(NativeFontSizeDeclarationValue::Relative(1_001))
    );
    assert_eq!(
        parse_font_size_value("calc(1.0005em * 2.0005)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                parent_scale_milli: 2_003,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
}

#[test]
fn font_size_parser_accepts_custom_property_fallbacks() {
    assert_eq!(
        parse_font_size_value("var(--size)"),
        Some(NativeFontSizeDeclarationValue::CustomProperty(
            custom_property_hash("--size")
        ))
    );
    assert_eq!(
        parse_font_size_value("var(--size, 16px)"),
        Some(NativeFontSizeDeclarationValue::CustomPropertyFallback(
            custom_property_hash("--size"),
            NativeFontSizeFallbackValue::Pixels(16),
        ))
    );
    assert_eq!(
        parse_font_size_value("var(--size, calc(1em + 2px))"),
        Some(NativeFontSizeDeclarationValue::CustomPropertyFallback(
            custom_property_hash("--size"),
            NativeFontSizeFallbackValue::Calculation(NativeFontSizeCalculation {
                absolute_milli: 2_000,
                parent_scale_milli: 1_000,
                ..NativeFontSizeCalculation::default()
            }),
        ))
    );
    assert_eq!(parse_font_size_value("var(--size, var(--other))"), None);
    assert_eq!(parse_font_size_value("var(--size, 16px, 18px)"), None);
    assert_eq!(parse_font_size_value("var(size)"), None);
}

#[test]
fn font_size_parser_accepts_composite_custom_property_calculations() {
    let Some(NativeFontSizeDeclarationValue::VariableCalculation(expression)) =
        parse_font_size_value("calc(var(--size) + 2px)")
    else {
        panic!("expected a bounded variable calculation");
    };
    assert_eq!(expression.as_str(), "calc(var(--size) + 2px)");
    assert!(matches!(
        parse_font_size_value("CALC(VAR(--size, 18px) + 2px)"),
        Some(NativeFontSizeDeclarationValue::VariableCalculation(_))
    ));
    assert_eq!(
        parse_font_size_value("calc(var(--size) + 2px) trailing"),
        None
    );
    assert_eq!(
        parse_font_size_value("calc(var(--size) + 2px\u{00a0})"),
        None
    );
}

#[test]
fn font_size_variable_resolution_remains_bounded_and_fail_closed() {
    let oversized = format!(
        "calc(var(--size) + {}px)",
        "1".repeat(MAX_NATIVE_FONT_SIZE_VARIABLE_EXPRESSION_BYTES)
    );
    assert_eq!(parse_font_size_value(&oversized), None);

    let mut custom_properties = BTreeMap::new();
    custom_properties.insert(custom_property_hash("--cycle"), "var(--cycle)".into());
    assert_eq!(
        expand_font_size_variables("calc(var(--cycle) + 2px)", &custom_properties, 0),
        None
    );
}

#[test]
fn font_size_parser_accepts_bounded_relative_units() {
    assert_eq!(
        parse_font_size_value("1.5em"),
        Some(NativeFontSizeDeclarationValue::Relative(1_500))
    );
    assert_eq!(
        parse_font_size_value("125%"),
        Some(NativeFontSizeDeclarationValue::Relative(1_250))
    );
    assert_eq!(parse_font_size_value("0em"), None);
    assert_eq!(parse_font_size_value("256.001em"), None);
    assert_eq!(
        parse_font_size_value("1rem"),
        Some(NativeFontSizeDeclarationValue::RootRelative(1_000))
    );
    assert_eq!(parse_font_size_value("0rem"), None);
    assert_eq!(
        parse_font_size_value("1vw"),
        Some(NativeFontSizeDeclarationValue::Viewport(
            1_000,
            NativeViewportFontSizeUnit::Width,
        ))
    );
    assert_eq!(
        parse_font_size_value("1vh"),
        Some(NativeFontSizeDeclarationValue::Viewport(
            1_000,
            NativeViewportFontSizeUnit::Height,
        ))
    );
    assert_eq!(
        parse_font_size_value("1vmin"),
        Some(NativeFontSizeDeclarationValue::Viewport(
            1_000,
            NativeViewportFontSizeUnit::Min,
        ))
    );
    assert_eq!(
        parse_font_size_value("1vmax"),
        Some(NativeFontSizeDeclarationValue::Viewport(
            1_000,
            NativeViewportFontSizeUnit::Max,
        ))
    );
    assert_eq!(parse_font_size_value("0vw"), None);
    assert_eq!(parse_font_size_value("256.001vh"), None);
    assert_eq!(
        parse_font_size_value("calc(1em + 2px)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                absolute_milli: 2_000,
                parent_scale_milli: 1_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(100% - 2px)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                absolute_milli: -2_000,
                parent_scale_milli: 1_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(1px * 2)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                absolute_milli: 2_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(2 * 1em)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                parent_scale_milli: 2_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(2px / 2)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                absolute_milli: 1_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(calc(1em + 2px) * 2)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                absolute_milli: 4_000,
                parent_scale_milli: 2_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(1px * 2 * 3)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                absolute_milli: 6_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(calc(1em * 2) * 3)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                parent_scale_milli: 6_000,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(
        parse_font_size_value("calc(2 / 3 * 1em)"),
        Some(NativeFontSizeDeclarationValue::Calculation(
            NativeFontSizeCalculation {
                parent_scale_milli: 667,
                ..NativeFontSizeCalculation::default()
            }
        ))
    );
    assert_eq!(parse_font_size_value("calc(1px * 2px)"), None);
    assert_eq!(parse_font_size_value("calc(1px / 2px / 2)"), None);
    assert_eq!(parse_font_size_value("calc(1px / 0)"), None);
    assert_eq!(parse_font_size_value("calc(-1px + 2px)"), None);
    assert_eq!(
        parse_font_size_value("calc(1px * 2 * 3 * 4 * 5 * 6 * 7 * 8 * 9)"),
        None
    );
}

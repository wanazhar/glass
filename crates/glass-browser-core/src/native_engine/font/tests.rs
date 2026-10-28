use super::*;

use base64::Engine as _;
use flate2::Compression;
#[test]
fn font_face_feature_defaults_merge_with_element_overrides() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let face_features =
        crate::native_engine::css::parse_font_feature_settings(r#""liga" off, "kern" 1"#).unwrap();
    let rule = NativeFontFaceRule {
        family: "Feature Fixture".into(),
        family_key: font_family_hash("Feature Fixture"),
        sources: Vec::new(),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        unicode_ranges: Vec::new(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: face_features,
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: crate::native_engine::css::NativeFontDisplay::Auto,
    };
    let resource = NativeFontFaceResource::from_rule(&rule, bytes.to_vec());
    let book = NativeFontBook::from_resources(&[resource]);
    let element_features =
        crate::native_engine::css::parse_font_feature_settings(r#""liga" on, "calt" 0"#).unwrap();
    let metrics = NativeTextMetrics::for_style_with_book_and_options(
        NativeFontFamilyList::single(NativeFontFamilyValue::Named(font_family_hash(
            "Feature Fixture",
        ))),
        16,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: element_features,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        &book,
    );
    let merged = metrics.effective_feature_settings(&metrics.faces[0]);
    assert_eq!(merged.count, 3);
    assert_eq!(
        merged.values[0],
        NativeFontFeature {
            tag: *b"liga",
            value: 1
        }
    );
    assert_eq!(
        merged.values[1],
        NativeFontFeature {
            tag: *b"calt",
            value: 0
        }
    );
    assert_eq!(
        merged.values[2],
        NativeFontFeature {
            tag: *b"kern",
            value: 1
        }
    );
}

#[test]
fn font_face_size_adjust_scales_metrics_and_rasterization() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let resource = |size_adjust| NativeFontFaceResource {
        family: "Size Adjust Fixture".into(),
        family_key: font_family_hash("Size Adjust Fixture"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(bytes.as_slice()),
        unicode_ranges: Vec::new(),
    };
    let normal_book = NativeFontBook::from_resources(&[resource(1000)]);
    let adjusted_book = NativeFontBook::from_resources(&[resource(500)]);
    let family = || {
        NativeFontFamilyList::single(NativeFontFamilyValue::Named(font_family_hash(
            "Size Adjust Fixture",
        )))
    };
    let normal = NativeTextMetrics::for_style_with_book(
        family(),
        64,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &normal_book,
    );
    let adjusted = NativeTextMetrics::for_style_with_book(
        family(),
        64,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &adjusted_book,
    );
    assert_eq!(adjusted.faces[0].size_adjust, 500);
    assert!(adjusted.line_height() < normal.line_height());
    assert!(adjusted.measure_text("A", 0, 0) < normal.measure_text("A", 0, 0));
    let normal_run = normal
        .rasterize("A", 0, 0, 0)
        .expect("normal font must rasterize");
    let adjusted_run = adjusted
        .rasterize("A", 0, 0, 0)
        .expect("adjusted font must rasterize");
    assert!(!normal_run.glyphs.is_empty());
    assert!(!adjusted_run.glyphs.is_empty());
    assert!(adjusted_run.glyphs[0].height < normal_run.glyphs[0].height);
}

#[test]
fn font_face_metric_overrides_replace_native_line_metrics() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let resource = |size_adjust, metric_overrides| NativeFontFaceResource {
        family: "Metric Override Fixture".into(),
        family_key: font_family_hash("Metric Override Fixture"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust,
        metric_overrides,
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(bytes.as_slice()),
        unicode_ranges: Vec::new(),
    };
    let normal_book =
        NativeFontBook::from_resources(&[resource(1000, NativeFontMetricOverrides::default())]);
    let overridden_values = NativeFontMetricOverrides {
        ascent: Some(800),
        descent: Some(200),
        line_gap: Some(0),
    };
    let overridden_book = NativeFontBook::from_resources(&[resource(1000, overridden_values)]);
    let adjusted_overridden_book =
        NativeFontBook::from_resources(&[resource(500, overridden_values)]);
    let family = || {
        NativeFontFamilyList::single(NativeFontFamilyValue::Named(font_family_hash(
            "Metric Override Fixture",
        )))
    };
    let normal = NativeTextMetrics::for_style_with_book(
        family(),
        64,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &normal_book,
    );
    let overridden = NativeTextMetrics::for_style_with_book(
        family(),
        64,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &overridden_book,
    );
    let adjusted_overridden = NativeTextMetrics::for_style_with_book(
        family(),
        64,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &adjusted_overridden_book,
    );
    assert!(overridden.ascent < normal.ascent);
    assert!(overridden.line_height() < normal.line_height());
    assert!(
        adjusted_overridden.ascent < overridden.ascent,
        "adjusted ascent {} must be below {}",
        adjusted_overridden.ascent,
        overridden.ascent
    );
    assert!(adjusted_overridden.line_height() < overridden.line_height());
}
use flate2::write::GzEncoder;

fn svg_table(glyph_id: u16, document: &[u8]) -> Vec<u8> {
    let document_offset = 20_u32;
    let mut table = Vec::with_capacity(
        usize::try_from(document_offset)
            .unwrap_or_default()
            .saturating_add(document.len()),
    );
    table.extend_from_slice(&[0, 0, 0, 0, 0, 6]);
    table.extend_from_slice(&1_u16.to_be_bytes());
    table.extend_from_slice(&glyph_id.to_be_bytes());
    table.extend_from_slice(&glyph_id.to_be_bytes());
    table.extend_from_slice(&14_u32.to_be_bytes());
    table.extend_from_slice(
        &u32::try_from(document.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    table.extend_from_slice(document);
    table
}

fn font_with_svg_table(font: &[u8], glyph_id: u16, document: &[u8]) -> Vec<u8> {
    let table_count = usize::from(read_u16_be(font, 4).expect("font table count"));
    let old_directory_end = 12_usize.saturating_add(table_count.saturating_mul(16));
    let first_table_offset = (0..table_count)
        .map(|index| {
            read_u32_be(font, 12 + index * 16 + 8)
                .expect("font table offset")
                .try_into()
                .expect("font table offset fits")
        })
        .min()
        .expect("font has tables");
    let mut output = font[..12].to_vec();
    let next_table_count = u16::try_from(table_count.saturating_add(1)).expect("table count");
    write_u16_be(&mut output, 4, next_table_count);
    let max_power = (1_usize
        << (usize::BITS - (usize::from(next_table_count)).leading_zeros() - 1))
        .min(1 << 15);
    let search_range = max_power.saturating_mul(16);
    let entry_selector = max_power.trailing_zeros();
    let range_shift = usize::from(next_table_count)
        .saturating_mul(16)
        .saturating_sub(search_range);
    write_u16_be(
        &mut output,
        6,
        u16::try_from(search_range).expect("search range"),
    );
    write_u16_be(
        &mut output,
        8,
        u16::try_from(entry_selector).expect("entry selector"),
    );
    write_u16_be(
        &mut output,
        10,
        u16::try_from(range_shift).expect("range shift"),
    );
    for index in 0..table_count {
        let start = 12 + index * 16;
        let end = start + 16;
        let mut record = font[start..end].to_vec();
        let offset = read_u32_be(&record, 8).expect("record offset");
        write_u32_be(&mut record, 8, offset.saturating_add(16));
        output.extend_from_slice(&record);
    }
    let svg_record_offset = output.len();
    output.extend_from_slice(b"SVG ");
    output.extend_from_slice(&[0; 12]);
    output.extend_from_slice(&font[old_directory_end..first_table_offset]);
    output.extend_from_slice(&font[first_table_offset..]);
    while !output.len().is_multiple_of(4) {
        output.push(0);
    }
    let svg_table_offset = output.len();
    let table = svg_table(glyph_id, document);
    output.extend_from_slice(&table);
    write_u32_be(
        &mut output,
        svg_record_offset + 8,
        u32::try_from(svg_table_offset).expect("SVG table offset"),
    );
    write_u32_be(
        &mut output,
        svg_record_offset + 12,
        u32::try_from(table.len()).expect("SVG table length"),
    );
    output
}

#[test]
fn svg_font_glyph_rasterization_precedes_outline_fallback() {
    let source = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let source_face = ttf_parser::Face::parse(source, 0).expect("fixture must parse");
    let glyph_id = source_face
        .glyph_index('A')
        .expect("fixture must map A to a glyph")
        .0;
    assert!(rasterize_svg_glyph(source, glyph_id, 32).is_none());
    let svg = br##"<svg viewBox="0 0 1000 1000"><path fill="#ff0000" d="M100 100 L900 100 L900 900 L100 900 Z"/></svg>"##;
    let bytes = font_with_svg_table(source, glyph_id, svg);
    let face = ttf_parser::Face::parse(&bytes, 0).expect("SVG fixture must parse");
    assert!(
        face.glyph_svg_image(ttf_parser::GlyphId(glyph_id))
            .is_some()
    );
    let rasterized = rasterize_svg_glyph(&bytes, glyph_id, 32).expect("SVG glyph must rasterize");
    assert_eq!((rasterized.width, rasterized.height), (16, 16));
    assert_eq!(rasterized.bitmap.as_ref().map(Vec::len), Some(16 * 16 * 4));
    assert!(rasterized.coverage.iter().any(|coverage| *coverage > 0));
    let resource = NativeFontFaceResource {
        family: "SVG Fixture".into(),
        family_key: font_family_hash("SVG Fixture"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(bytes),
        unicode_ranges: Vec::new(),
    };
    let book = NativeFontBook::from_resources(&[resource]);
    let metrics = NativeTextMetrics::for_style_with_book_and_options(
        NativeFontFamilyList::single(NativeFontFamilyValue::Named(font_family_hash(
            "SVG Fixture",
        ))),
        32,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::default(),
            variant_caps: NativeFontVariantCaps::default(),
            variant_position: NativeFontVariantPosition::default(),
            variant_numeric: NativeFontVariantNumeric::default(),
            variant_alternates: NativeFontVariantAlternates::default(),
            variant_east_asian: NativeFontVariantEastAsian::default(),
            language_override: NativeFontLanguageOverride::default(),
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        &book,
    );
    let run = metrics.rasterize("A", 0, 0, 0).expect("SVG font run");
    assert!(
        run.glyphs.iter().any(|glyph| glyph.bitmap.is_some()),
        "font-book rasterization must retain the SVG bitmap"
    );
}

#[test]
fn svg_font_glyphs_accept_svgz_and_reject_external_or_malformed_data() {
    let source = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let face = ttf_parser::Face::parse(source, 0).expect("fixture must parse");
    let glyph_id = face
        .glyph_index('A')
        .expect("fixture must map A to a glyph")
        .0;
    let svg = br##"<svg viewBox="0 0 1000 1000"><path fill="#00ff00" d="M100 100 L900 100 L500 900 Z"/></svg>"##;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(svg).expect("SVGZ payload must compress");
    let compressed = encoder.finish().expect("SVGZ payload must finish");
    let svgz_font = font_with_svg_table(source, glyph_id, &compressed);
    let rasterized =
        rasterize_svg_glyph(&svgz_font, glyph_id, 32).expect("SVGZ glyph must rasterize");
    assert!(rasterized.coverage.iter().any(|coverage| *coverage > 0));

    let external =
        br#"<svg viewBox="0 0 1000 1000"><image href="https://example.test/a.png"/></svg>"#;
    let external_font = font_with_svg_table(source, glyph_id, external);
    assert!(rasterize_svg_glyph(&external_font, glyph_id, 32).is_none());
    let malformed = br#"<svg viewBox="0 0 bad 1000"><path d="M0 0"/></svg>"#;
    let malformed_font = font_with_svg_table(source, glyph_id, malformed);
    assert!(rasterize_svg_glyph(&malformed_font, glyph_id, 32).is_none());
}

#[test]
fn svg_font_source_without_svg_keeps_existing_outline_path() {
    let source = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let face = ttf_parser::Face::parse(source, 0).expect("fixture must parse");
    let glyph_id = face
        .glyph_index('A')
        .expect("fixture must map A to a glyph")
        .0;
    assert!(rasterize_svg_glyph(source, glyph_id, 32).is_none());
    let mut builder = NativeOutlineBuilder::default();
    assert!(
        face.outline_glyph(ttf_parser::GlyphId(glyph_id), &mut builder)
            .is_some()
    );
    let contours = builder
        .finish()
        .expect("outline fallback must remain available");
    let rasterized =
        rasterize_outline_contours(&contours, 32, f32::from(face.units_per_em()), None)
            .expect("outline fallback must rasterize");
    assert!(rasterized.coverage.iter().any(|coverage| *coverage > 0));
}

#[test]
fn real_system_font_metrics_are_bounded_when_available() {
    let families = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));
    let metrics = NativeTextMetrics::for_style(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
    );
    assert!(metrics.line_height() > 0);
    assert!(metrics.measure_text("Glass", 0, 0) > 0);
    if !metrics.faces.is_empty() {
        let run = metrics.rasterize("Glass", 0, 0, 0).unwrap();
        assert!(!run.glyphs.is_empty());
        assert!(run.width > 0);
    }
}

#[test]
fn colr_palette_layers_are_rasterized_with_palette_colors() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLR fixture must parse");
    let glyph_id = face
        .glyph_index('A')
        .expect("COLR fixture must map A to a glyph");
    assert!(face.is_color_glyph(glyph_id));
    let layers = rasterize_color_glyph(
        bytes,
        glyph_id.0,
        32,
        NativeFontVariationSettings::default(),
        0,
    )
    .expect("COLR glyph must rasterize");
    assert_eq!(layers.len(), 2);
    assert!(layers.iter().any(|layer| {
        layer.color
            == Some(NativeColor {
                red: u8::MAX,
                green: 0,
                blue: 0,
                alpha: u8::MAX,
            })
    }));
    assert!(layers.iter().any(|layer| {
        layer.color
            == Some(NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            })
    }));
    assert!(
        layers
            .iter()
            .any(|layer| layer.coverage.iter().any(|coverage| *coverage > 0))
    );
}

#[test]
fn colr_v1_gradients_are_rasterized_with_palette_stops() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLRv1 fixture must parse");
    for glyph_id in [9_u16, 13, 93] {
        let glyph_id = ttf_parser::GlyphId(glyph_id);
        assert!(face.is_color_glyph(glyph_id));
        let layers = rasterize_color_glyph(
            bytes,
            glyph_id.0,
            32,
            NativeFontVariationSettings::default(),
            0,
        )
        .expect("bounded COLRv1 gradient must rasterize");
        assert!(layers.iter().any(|layer| {
            layer.gradient.is_some() && layer.coverage.iter().any(|coverage| *coverage > 0)
        }));
    }
}

#[test]
fn colr_v1_non_default_palette_changes_rasterized_layer_colors() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLRv1 fixture must parse");
    assert_eq!(face.color_palettes().map(|count| count.get()), Some(3));
    let glyph_id = ttf_parser::GlyphId(86);
    assert!(face.is_color_glyph(glyph_id));

    let default_layers = rasterize_color_glyph(
        bytes,
        glyph_id.0,
        32,
        NativeFontVariationSettings::default(),
        0,
    )
    .expect("default COLRv1 palette must rasterize");
    let alternate_layers = rasterize_color_glyph(
        bytes,
        glyph_id.0,
        32,
        NativeFontVariationSettings::default(),
        1,
    )
    .expect("alternate COLRv1 palette must rasterize");
    assert_eq!(default_layers.len(), alternate_layers.len());
    let default_colors = default_layers
        .iter()
        .map(|layer| layer.color)
        .collect::<Vec<_>>();
    let alternate_colors = alternate_layers
        .iter()
        .map(|layer| layer.color)
        .collect::<Vec<_>>();
    assert_ne!(default_colors, alternate_colors);
    assert!(
        rasterize_color_glyph(
            bytes,
            glyph_id.0,
            32,
            NativeFontVariationSettings::default(),
            3,
        )
        .is_none()
    );
}

#[test]
fn colr_palette_overrides_change_solid_and_gradient_colors() {
    let solid_bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let solid_face = ttf_parser::Face::parse(solid_bytes, 0).expect("COLRv0 fixture must parse");
    let solid_glyph = solid_face
        .glyph_index('A')
        .expect("COLRv0 fixture must map A to a glyph");
    let solid_layers = rasterize_color_glyph(
        solid_bytes,
        solid_glyph.0,
        32,
        NativeFontVariationSettings::default(),
        0,
    )
    .expect("COLRv0 glyph must rasterize");
    let solid_palette = palette_colors_for_face(solid_bytes, 0).expect("CPAL must parse");
    let solid_index = solid_palette
        .iter()
        .enumerate()
        .find(|(_, color)| {
            solid_layers
                .iter()
                .any(|layer| layer.color == Some(**color))
        })
        .map(|(index, _)| index)
        .expect("COLRv0 layer must reference a CPAL entry");
    assert!(solid_index < MAX_NATIVE_FONT_PALETTE_OVERRIDES);
    let replacement = NativeColor {
        red: 12,
        green: 34,
        blue: 56,
        alpha: u8::MAX,
    };
    let mut solid_overrides = [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES];
    solid_overrides[solid_index] = Some(NativeFontPaletteOverride {
        palette_index: u16::try_from(solid_index).unwrap(),
        color: replacement,
    });
    let overridden_solid_layers = rasterize_color_glyph_with_overrides(
        solid_bytes,
        solid_glyph.0,
        32,
        NativeFontVariationSettings::default(),
        0,
        solid_overrides,
    )
    .expect("COLRv0 override must rasterize");
    assert!(
        overridden_solid_layers
            .iter()
            .any(|layer| layer.color == Some(replacement))
    );

    let gradient_bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let gradient_palette =
        palette_colors_for_face(gradient_bytes, 0).expect("COLRv1 CPAL must parse");
    fn stops_for(gradient: &NativeGlyphGradient) -> &[NativeGradientStop] {
        match gradient {
            NativeGlyphGradient::Linear { stops, .. }
            | NativeGlyphGradient::Radial { stops, .. }
            | NativeGlyphGradient::Sweep { stops, .. } => stops,
        }
    }
    let (gradient_glyph, gradient_index) = [9_u16, 13, 93]
        .into_iter()
        .find_map(|glyph_id| {
            let layers = rasterize_color_glyph(
                gradient_bytes,
                glyph_id,
                32,
                NativeFontVariationSettings::default(),
                0,
            )?;
            let index = gradient_palette
                .iter()
                .enumerate()
                .find_map(|(index, color)| {
                    layers
                        .iter()
                        .filter_map(|layer| layer.gradient.as_ref())
                        .any(|gradient| stops_for(gradient).iter().any(|stop| stop.color == *color))
                        .then_some(index)
                })?;
            Some((glyph_id, index))
        })
        .expect("COLRv1 gradient must reference a CPAL entry");
    let mut gradient_overrides = [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES];
    gradient_overrides[gradient_index] = Some(NativeFontPaletteOverride {
        palette_index: u16::try_from(gradient_index).unwrap(),
        color: replacement,
    });
    let overridden_gradient_layers = rasterize_color_glyph_with_overrides(
        gradient_bytes,
        gradient_glyph,
        32,
        NativeFontVariationSettings::default(),
        0,
        gradient_overrides,
    )
    .expect("COLRv1 gradient override must rasterize");
    assert!(
        overridden_gradient_layers
            .iter()
            .filter_map(|layer| layer.gradient.as_ref())
            .any(|gradient| stops_for(gradient)
                .iter()
                .any(|stop| stop.color == replacement))
    );
    let mut invalid_overrides = [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES];
    invalid_overrides[0] = Some(NativeFontPaletteOverride {
        palette_index: u16::MAX,
        color: replacement,
    });
    assert!(
        rasterize_color_glyph_with_overrides(
            solid_bytes,
            solid_glyph.0,
            32,
            NativeFontVariationSettings::default(),
            0,
            invalid_overrides,
        )
        .is_none()
    );
}

#[test]
fn cpal_semantic_palette_keywords_select_advertised_palettes() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    assert_eq!(palette_index_for_face(bytes, NativeFontPalette::Normal), 0);
    assert_eq!(palette_index_for_face(bytes, NativeFontPalette::Light), 2);
    assert_eq!(palette_index_for_face(bytes, NativeFontPalette::Dark), 1);
    assert_eq!(palette_index_for_face(bytes, NativeFontPalette::Base(2)), 2);
    assert_eq!(
        palette_index_for_face(bytes, NativeFontPalette::Base(99)),
        0
    );
    assert_eq!(
        palette_index_for_face(b"not-a-font", NativeFontPalette::Dark),
        0
    );
}

#[test]
fn text_metrics_palette_keyword_changes_colr_raster_colors() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLRv1 fixture must parse");
    let glyph_id = ttf_parser::GlyphId(86);
    let character = (0..=0x10ffff)
        .filter_map(char::from_u32)
        .find(|character| face.glyph_index(*character) == Some(glyph_id))
        .expect("COLRv1 fixture must map glyph 86");
    let resource = NativeFontFaceResource {
        family: "Palette Fixture".into(),
        family_key: font_family_hash("Palette Fixture"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(bytes.as_slice()),
        unicode_ranges: Vec::new(),
    };
    let book = NativeFontBook::from_resources(&[resource]);
    let families = NativeFontFamilyList::parse("Palette Fixture").unwrap();
    let normal = NativeTextMetrics::for_style_with_book(
        families,
        32,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    )
    .with_palette(NativeFontPalette::Normal);
    let light = NativeTextMetrics::for_style_with_book(
        families,
        32,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    )
    .with_palette(NativeFontPalette::Light);
    let base = NativeTextMetrics::for_style_with_book(
        families,
        32,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    )
    .with_palette(NativeFontPalette::Base(2));
    let normal_colors = normal
        .rasterize(&character.to_string(), 0, 0, 0)
        .expect("normal palette must rasterize")
        .glyphs
        .into_iter()
        .map(|glyph| glyph.color)
        .collect::<Vec<_>>();
    let light_colors = light
        .rasterize(&character.to_string(), 0, 0, 0)
        .expect("light palette must rasterize")
        .glyphs
        .into_iter()
        .map(|glyph| glyph.color)
        .collect::<Vec<_>>();
    let base_colors = base
        .rasterize(&character.to_string(), 0, 0, 0)
        .expect("base palette must rasterize")
        .glyphs
        .into_iter()
        .map(|glyph| glyph.color)
        .collect::<Vec<_>>();
    assert_eq!(base_colors, light_colors);
    assert_ne!(normal_colors, light_colors);
}

#[test]
fn native_gradient_sampling_interpolates_and_applies_extend_modes() {
    let stops: Arc<[NativeGradientStop]> = Arc::from([
        NativeGradientStop {
            offset: 0.0,
            color: NativeColor {
                red: u8::MAX,
                green: 0,
                blue: 0,
                alpha: u8::MAX,
            },
        },
        NativeGradientStop {
            offset: 1.0,
            color: NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            },
        },
    ]);
    let gradient = |extend| NativeGlyphGradient::Linear {
        x0: 0.0,
        y0: 0.0,
        x1: 10.0,
        y1: 0.0,
        x2: 0.0,
        y2: 10.0,
        extend,
        stops: stops.clone(),
    };
    let midpoint = NativeColor {
        red: 128,
        green: 0,
        blue: 128,
        alpha: u8::MAX,
    };
    assert_eq!(
        gradient(NativeGradientExtend::Pad).color_at(5.0, 0.0),
        Some(midpoint)
    );
    assert_eq!(
        gradient(NativeGradientExtend::Pad).color_at(15.0, 0.0),
        Some(stops[1].color)
    );
    assert_eq!(
        gradient(NativeGradientExtend::Repeat).color_at(15.0, 0.0),
        Some(midpoint)
    );
    assert_eq!(
        gradient(NativeGradientExtend::Reflect).color_at(15.0, 0.0),
        Some(midpoint)
    );
    let diagonal = NativeGlyphGradient::Linear {
        x0: 0.0,
        y0: 0.0,
        x1: 10.0,
        y1: 0.0,
        x2: 10.0,
        y2: 10.0,
        extend: NativeGradientExtend::Pad,
        stops: stops.clone(),
    };
    assert_eq!(diagonal.color_at(5.0, 5.0), Some(stops[0].color));
}

#[test]
fn native_radial_gradient_solves_two_circle_geometry() {
    let stops: Arc<[NativeGradientStop]> = Arc::from([
        NativeGradientStop {
            offset: 0.0,
            color: NativeColor::RED,
        },
        NativeGradientStop {
            offset: 1.0,
            color: NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            },
        },
    ]);
    let midpoint = NativeColor {
        red: 128,
        green: 0,
        blue: 128,
        alpha: u8::MAX,
    };
    let radial = |x0, y0, r0, x1, y1, r1| NativeGlyphGradient::Radial {
        x0,
        y0,
        r0,
        x1,
        y1,
        r1,
        transform: NativeGradientTransform::identity(),
        extend: NativeGradientExtend::Pad,
        stops: stops.clone(),
    };

    assert_eq!(
        radial(0.0, 0.0, 10.0, 0.0, 0.0, 20.0).color_at(15.0, 0.0),
        Some(midpoint)
    );
    assert_eq!(
        radial(0.0, 0.0, 10.0, 0.0, 0.0, 20.0).color_at(5.0, 0.0),
        Some(NativeColor::RED)
    );
    assert_eq!(
        radial(0.0, 0.0, 2.0, 4.0, 0.0, 8.0).color_at(2.0, 5.0),
        Some(midpoint)
    );
    assert_eq!(
        radial(0.0, 0.0, 0.0, 0.0, 0.0, 10.0).color_at(0.0, 0.0),
        Some(NativeColor::RED)
    );
    assert!(
        radial(0.0, 0.0, 1.0, 4.0, 0.0, 1.0)
            .color_at(2.0, 2.0)
            .is_none()
    );
    assert!(
        radial(0.0, 0.0, 5.0, 0.0, 0.0, 5.0)
            .color_at(5.0, 0.0)
            .is_none()
    );
}

#[test]
fn radial_gradient_affine_transform_samples_ellipse_geometry() {
    let stops: Arc<[NativeGradientStop]> = Arc::from([
        NativeGradientStop {
            offset: 0.0,
            color: NativeColor::RED,
        },
        NativeGradientStop {
            offset: 1.0,
            color: NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            },
        },
    ]);
    let midpoint = NativeColor {
        red: 128,
        green: 0,
        blue: 128,
        alpha: u8::MAX,
    };
    let radial = NativeGlyphGradient::Radial {
        x0: 0.0,
        y0: 0.0,
        r0: 2.0,
        x1: 4.0,
        y1: 0.0,
        r1: 8.0,
        transform: NativeGradientTransform {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        },
        extend: NativeGradientExtend::Pad,
        stops: stops.clone(),
    };
    assert_eq!(radial.color_at(4.0, 5.0), Some(midpoint));

    let mapped = NativeColorPaint::Gradient(radial).glyph_gradient(2.0, -3, 10);
    assert_eq!(
        mapped.and_then(|gradient| gradient.color_at(11.0, 0.0)),
        Some(midpoint)
    );
}

#[test]
fn sweep_gradient_affine_transform_samples_gradient_space_angles() {
    let stops: Arc<[NativeGradientStop]> = Arc::from([
        NativeGradientStop {
            offset: 0.0,
            color: NativeColor::RED,
        },
        NativeGradientStop {
            offset: 1.0,
            color: NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            },
        },
    ]);
    let sweep = |transform| NativeGlyphGradient::Sweep {
        center_x: 0.0,
        center_y: 0.0,
        start_angle: 0.0,
        end_angle: 1.0,
        transform,
        extend: NativeGradientExtend::Pad,
        stops: stops.clone(),
    };
    let base = sweep(NativeGradientTransform::identity()).color_at(1.0, 1.0);
    assert!(base.is_some());
    let transformed = sweep(NativeGradientTransform {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    });
    assert_eq!(transformed.color_at(2.0, 1.0), base);

    let mapped = NativeColorPaint::Gradient(transformed).glyph_gradient(2.0, -3, 10);
    assert_eq!(
        mapped.and_then(|gradient| gradient.color_at(7.0, 8.0)),
        base
    );
}

#[test]
fn radial_gradient_without_cone_intersection_is_transparent() {
    let stops: Arc<[NativeGradientStop]> = Arc::from([
        NativeGradientStop {
            offset: 0.0,
            color: NativeColor::RED,
        },
        NativeGradientStop {
            offset: 1.0,
            color: NativeColor {
                red: 0,
                green: 0,
                blue: u8::MAX,
                alpha: u8::MAX,
            },
        },
    ]);
    let node_id = super::super::NativeDocument::empty().root();
    let run = super::super::NativeFontRun {
        glyphs: vec![NativeGlyph {
            x: 0,
            y: 0,
            width: 1,
            height: 3,
            advance: 1,
            color: Some(NativeColor::RED),
            composite: NativeGlyphComposite::SourceOver,
            gradient: Some(NativeGlyphGradient::Radial {
                x0: 0.0,
                y0: 0.0,
                r0: 1.0,
                x1: 4.0,
                y1: 0.0,
                r1: 1.0,
                transform: NativeGradientTransform::identity(),
                extend: NativeGradientExtend::Pad,
                stops,
            }),
            bitmap: None,
            coverage: Arc::from([u8::MAX; 3]),
        }],
        space_ranges: Vec::new(),
        width: 1,
        ascent: 3,
        line_height: 3,
    };
    let list = super::super::NativeDisplayList {
        revision: 1,
        viewport: super::super::Viewport {
            width: 1,
            height: 3,
            device_scale_factor_milli: 1_000,
        },
        scroll_offset: super::super::NativePoint { x: 0, y: 0 },
        commands: vec![
            super::super::NativeDisplayCommand::Clear {
                color: NativeColor::WHITE,
            },
            super::super::NativeDisplayCommand::GlyphRun {
                node_id,
                origin: super::super::NativePoint { x: 0, y: 0 },
                text: "A".into(),
                truncated: false,
                run,
                color: NativeColor::BLACK,
                decoration_color: NativeColor::BLACK,
                decoration_style: super::super::NativeTextDecorationStyle::Solid,
                decoration_skip_ink: super::super::NativeTextDecorationSkipInk::None,
                decoration_skip_spaces: super::super::NativeTextDecorationSkipSpaces::None,
                decoration_thickness: 1,
                underline_offset: 0,
                underline: false,
                overline: false,
                line_through: false,
                bold: false,
                italic: false,
                word_spacing: 0,
                letter_spacing: 0,
                justify_spacing: 0,
                clip: None,
            },
        ],
        text_run_boundaries: Vec::new(),
    };
    let surface = super::super::NativeSurface::from_display_list(&list).unwrap();

    assert_ne!(surface.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(0, 2), Some([255, 255, 255, 255]));
}

#[test]
fn colr_gradient_transform_geometry_allows_affine_shapes() {
    let translated = ttf_parser::Transform::new_translate(12.0, -4.0);
    assert_eq!(
        NativeColorPainter::transformed_point(translated, 3.0, 5.0),
        Some((15.0, 1.0))
    );

    let scaled = NativeGradientTransform::from_ttf(ttf_parser::Transform::new_scale(2.0, 2.0));
    assert!(scaled.is_invertible());
    assert!(scaled.inverse_point(4.0, 6.0).is_some());

    let reflected = NativeGradientTransform::from_ttf(ttf_parser::Transform::new_scale(-2.0, 2.0));
    assert!(reflected.is_invertible());
    assert!(reflected.inverse_point(-4.0, 6.0).is_some());

    let skew = NativeGradientTransform::from_ttf(ttf_parser::Transform::new_skew(0.125, 0.0));
    assert!(skew.is_invertible());
    assert!(skew.inverse_point(4.0, 5.0).is_some());
    assert!(
        !NativeGradientTransform::from_ttf(ttf_parser::Transform::new_scale(0.0, 1.0))
            .is_invertible()
    );
}

#[test]
fn transformed_color_clip_polygons_intersect_nested_masks() {
    let outer = NativeColorClipPolygon::from_bounds(
        0.0,
        0.0,
        10.0,
        10.0,
        ttf_parser::Transform {
            a: 1.0,
            b: 0.5,
            c: 0.25,
            d: 1.0,
            e: 3.0,
            f: -2.0,
        },
    )
    .expect("finite nonsingular transformed clip must be admitted");
    let inner =
        NativeColorClipPolygon::from_bounds(4.0, 1.0, 8.0, 5.0, ttf_parser::Transform::default())
            .expect("identity clip must be admitted");
    let mask = NativeColorClipMask {
        polygons: vec![outer, inner],
    };

    assert!(mask.contains(5.0, 2.0));
    assert!(!mask.contains(3.0, 2.0));
    assert!(!mask.contains(20.0, 2.0));
    assert!(
        NativeColorClipPolygon::from_bounds(
            0.0,
            0.0,
            1.0,
            1.0,
            ttf_parser::Transform::new_scale(0.0, 1.0),
        )
        .is_none()
    );

    let raster_mask = NativeColorClipMask {
        polygons: vec![
            NativeColorClipPolygon::from_bounds(
                0.0,
                0.0,
                10.0,
                10.0,
                ttf_parser::Transform::new_translate(2.0, 0.0),
            )
            .expect("translated outer clip must be admitted"),
            NativeColorClipPolygon::from_bounds(
                4.0,
                1.0,
                8.0,
                9.0,
                ttf_parser::Transform::default(),
            )
            .expect("inner clip must be admitted"),
        ],
    };
    let contours = vec![vec![
        NativeOutlinePoint { x: 0.0, y: 0.0 },
        NativeOutlinePoint { x: 10.0, y: 0.0 },
        NativeOutlinePoint { x: 10.0, y: 10.0 },
        NativeOutlinePoint { x: 0.0, y: 10.0 },
    ]];
    let rasterized = rasterize_outline_contours(&contours, 10, 10.0, Some(raster_mask))
        .expect("transformed nested clips must rasterize");
    assert_eq!(rasterized.coverage[5 * 10], 0);
    assert_eq!(rasterized.coverage[5 * 10 + 5], u8::MAX);
    assert_eq!(rasterized.coverage[5 * 10 + 8], 0);
}

#[test]
fn bitmap_glyph_formats_decode_and_scale_with_bounded_alpha() {
    let gray_data = [0x30_u8];
    let gray = ttf_parser::RasterGlyphImage {
        x: 1,
        y: -2,
        width: 2,
        height: 1,
        pixels_per_em: 16,
        format: ttf_parser::RasterImageFormat::BitmapGray2Packed,
        data: &gray_data,
    };
    let decoded = decode_bitmap_glyph_image(gray).expect("packed grayscale bitmap must decode");
    assert_eq!((decoded.width, decoded.height), (2, 1));
    assert_eq!(decoded.coverage, vec![0, u8::MAX]);
    assert!(decoded.pixels.is_none());

    let bgra_data = [0_u8, 64, 128, 128, 0, 0, 0, 0];
    let bgra = ttf_parser::RasterGlyphImage {
        x: 0,
        y: 0,
        width: 2,
        height: 1,
        pixels_per_em: 16,
        format: ttf_parser::RasterImageFormat::BitmapPremulBgra32,
        data: &bgra_data,
    };
    let decoded = decode_bitmap_glyph_image(bgra).expect("premultiplied BGRA bitmap must decode");
    assert_eq!((decoded.width, decoded.height), (2, 1));
    assert_eq!(decoded.coverage, vec![128, 0]);
    assert_eq!(
        decoded.pixels.expect("color bitmap pixels"),
        vec![255, 128, 0, 128, 0, 0, 0, 0]
    );
    let malformed_premul = ttf_parser::RasterGlyphImage {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        pixels_per_em: 16,
        format: ttf_parser::RasterImageFormat::BitmapPremulBgra32,
        data: &[u8::MAX, 0, 0, 128],
    };
    assert!(
        decode_bitmap_glyph_image(malformed_premul).is_none(),
        "non-premultiplied channel values must fall back"
    );
    assert!(
        decode_bitmap_glyph_image(ttf_parser::RasterGlyphImage {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            pixels_per_em: 16,
            format: ttf_parser::RasterImageFormat::BitmapGray8,
            data: &[],
        })
        .is_none(),
        "truncated grayscale data must fall back"
    );
    let row_aligned_gray = [0x1b_u8, 0xe4_u8];
    assert_eq!(
        decode_bitmap_coverage(&row_aligned_gray, 2, 2, 2, true),
        Some(vec![0, 85, u8::MAX, 170])
    );
    assert!(decode_bitmap_coverage(&[], 1_025, 1, 1, true).is_none());

    assert_eq!(
        scale_raster_bytes(&[1], 1, 1, 1, 2, 2).expect("bounded scale"),
        vec![1, 1, 1, 1]
    );
    assert!(scale_raster_bytes(&[1], u32::MAX, 1, 1, 1, 1).is_none());
    assert!(scale_coverage_horizontal(vec![1], u32::MAX, 1, 1, 1).is_none());
    assert!(
        scale_coverage_horizontal_bounded(vec![u8::MAX; 1_024], 1_024, 1, 2_000, 1_000).is_none()
    );
}

#[test]
fn system_color_bitmap_strike_preserves_offsets_when_available() {
    let Some(bytes) = [
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        r"C:\Windows\Fonts\seguiemj.ttf",
    ]
    .into_iter()
    .find_map(|path| std::fs::read(path).ok()) else {
        return;
    };
    let face = ttf_parser::Face::parse(&bytes, 0).ok();
    let Some(face) = face else {
        return;
    };
    let Some(glyph_id) = face.glyph_index('😀') else {
        return;
    };
    let Some(image) = face.glyph_raster_image(glyph_id, 16) else {
        return;
    };
    let decoded = decode_bitmap_glyph_image(image).expect("bitmap strike payload must decode");
    let decoded_width = decoded.width;
    let decoded_height = decoded.height;
    let rasterized =
        rasterize_bitmap_glyph(&bytes, 0, glyph_id.0, 16).expect("bitmap strike must decode");
    assert_eq!(
        rasterized.xmin,
        scale_signed(i32::from(image.x), 16, image.pixels_per_em)
    );
    assert_eq!(
        rasterized.ymin,
        scale_signed(i32::from(image.y), 16, image.pixels_per_em)
    );
    assert_eq!(
        rasterized.width,
        scale_dimension(decoded_width, 16, image.pixels_per_em)
    );
    assert_eq!(
        rasterized.height,
        scale_dimension(decoded_height, 16, image.pixels_per_em)
    );
    let pixel_count = usize::try_from(rasterized.width)
        .unwrap()
        .checked_mul(usize::try_from(rasterized.height).unwrap())
        .unwrap();
    assert_eq!(rasterized.coverage.len(), pixel_count);
    assert!(rasterized.coverage.iter().any(|coverage| *coverage > 0));
    if matches!(image.format, ttf_parser::RasterImageFormat::PNG) {
        assert_eq!(
            rasterized.bitmap.as_ref().map(|pixels| pixels.len()),
            Some(pixel_count * 4)
        );
    }
}

#[test]
fn colr_v1_unsupported_paints_fall_back_to_monochrome() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLRv1 fixture must parse");
    let glyph_id = ttf_parser::GlyphId(131);
    assert!(face.is_color_glyph(glyph_id));
    assert!(
        rasterize_color_glyph(
            bytes,
            glyph_id.0,
            32,
            NativeFontVariationSettings::default(),
            0,
        )
        .is_none()
    );
}

#[test]
fn colr_v1_solid_transforms_are_rasterized() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLRv1 fixture must parse");
    let glyph_id = ttf_parser::GlyphId(86);
    assert!(face.is_color_glyph(glyph_id));
    let layers = rasterize_color_glyph(
        bytes,
        glyph_id.0,
        32,
        NativeFontVariationSettings::default(),
        0,
    )
    .expect("bounded solid transform must rasterize");
    assert!(!layers.is_empty());
    assert!(
        layers
            .iter()
            .any(|layer| layer.coverage.iter().any(|coverage| *coverage > 0))
    );
}

#[test]
fn colr_v1_solid_composite_layers_are_rasterized() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLRv1 fixture must parse");
    let glyph_id = ttf_parser::GlyphId(84);
    assert!(face.is_color_glyph(glyph_id));
    let layers = rasterize_color_glyph(
        bytes,
        glyph_id.0,
        32,
        NativeFontVariationSettings::default(),
        0,
    )
    .expect("bounded solid composite must rasterize");
    assert_eq!(layers.len(), 2);
    assert!(
        layers
            .iter()
            .any(|layer| layer.composite == NativeGlyphComposite::DestinationOver)
    );
}

#[test]
fn colr_current_outline_clip_rejects_a_replaced_outline() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-1.ttf");
    let face = ttf_parser::Face::parse(bytes, 0).expect("COLRv1 fixture must parse");
    let mut painter = NativeColorPainter::new(
        face.clone(),
        Vec::new(),
        [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
        NativeColor::BLACK,
    );
    ttf_parser::colr::Painter::outline_glyph(&mut painter, ttf_parser::GlyphId(3));
    ttf_parser::colr::Painter::push_clip(&mut painter);
    ttf_parser::colr::Painter::outline_glyph(&mut painter, ttf_parser::GlyphId(2));
    assert!(!painter.unsupported);
    ttf_parser::colr::Painter::paint(
        &mut painter,
        ttf_parser::colr::Paint::Solid(ttf_parser::RgbaColor::new(255, 0, 0, 255)),
    );
    assert!(painter.unsupported);
}

#[test]
fn ordered_font_candidates_preserve_css_family_order() {
    let families = NativeFontFamilyList::parse("sans-serif, serif").unwrap();
    let candidates = system_font_book().faces_for(
        families,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        1000,
    );
    if candidates.len() < 2 {
        return;
    }
    assert_eq!(
        candidates[0].generic_family,
        Some(NativeGenericFontFamily::SansSerif)
    );
    assert_eq!(
        candidates[1].generic_family,
        Some(NativeGenericFontFamily::Serif)
    );
}

#[test]
fn local_font_lookup_matches_system_family_case_insensitively() {
    let Some(face) = system_font_book().faces.first() else {
        return;
    };
    let family = face.family.to_ascii_uppercase();
    let bytes = system_font_book()
        .local_font_bytes(
            &family,
            FontWeightValue::from_numeric(face.weight.nominal()).unwrap(),
            face.style,
        )
        .expect("the system font book must resolve its own first face");
    assert_eq!(bytes.as_slice(), face.font_data.as_ref());
}

#[test]
fn local_font_lookup_rejects_unknown_family() {
    assert!(
        system_font_book()
            .local_font_bytes(
                "Glass Missing Local Font",
                FontWeightValue::Normal,
                FontStyleValue::Normal
            )
            .is_none()
    );
}

#[test]
fn font_book_parses_only_faces_used_by_text_metrics() {
    let bytes = include_bytes!("../../../tests/fixtures/colr-v0.ttf");
    let mut book = NativeFontBook::default();
    for family in ["Glass First Fixture", "Glass Unused Fixture"] {
        assert!(insert_font_face(
            &mut book,
            family.into(),
            None,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            bytes.to_vec(),
            0,
        ));
    }
    assert!(book.faces.iter().all(|face| face.font.get().is_none()));
    assert!(
        book.faces
            .iter()
            .all(|face| face.shaper_data.get().is_none())
    );

    let metrics = NativeTextMetrics::for_style_with_book(
        NativeFontFamilyList::single(NativeFontFamilyValue::Named(font_family_hash(
            "Glass First Fixture",
        ))),
        16,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    );
    assert!(book.faces[0].font.get().is_some());
    assert!(book.faces[0].shaper_data.get().is_none());
    assert!(metrics.measure_text("A", 0, 0) > 0);
    assert!(book.faces[0].shaper_data.get().is_some());
    assert!(book.faces[1].font.get().is_none());
    assert!(book.faces[1].shaper_data.get().is_none());
}

#[test]
fn system_font_book_discovers_a_non_static_installed_face() {
    let static_paths = system_font_candidates()
        .iter()
        .map(|candidate| Path::new(candidate.0))
        .collect::<Vec<_>>();
    let Some((bytes, family, weight, style)) = discover_system_font_paths()
        .into_iter()
        .filter(|path| !static_paths.iter().any(|candidate| *candidate == path))
        .find_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
                return None;
            }
            let (family, _, weight, style) = discovered_font_metadata(&bytes, 0)?;
            let mut probe = NativeFontBook::default();
            insert_font_face(
                &mut probe,
                family.clone(),
                None,
                weight,
                style,
                bytes.clone(),
                0,
            )
            .then_some((bytes, family, weight, style))
        })
    else {
        return;
    };
    assert!(system_font_book().faces.iter().any(|face| {
        face.family.eq_ignore_ascii_case(&family)
            && face.weight == NativeFontWeightRange::singleton(weight)
            && face.style == style
            && face.font_data.as_ref() == bytes.as_slice()
    }));
}

fn test_woff_from_sfnt(sfnt: &[u8], compress_tables: bool) -> Option<(Vec<u8>, bool)> {
    let flavor = read_u32_be(sfnt, 0)?;
    let table_count = usize::from(read_u16_be(sfnt, 4)?);
    if table_count == 0 || table_count > MAX_WOFF_TABLES {
        return None;
    }
    let mut tables = Vec::with_capacity(table_count);
    let mut compressed_any = false;
    for index in 0..table_count {
        let entry = 12usize.checked_add(index.checked_mul(16)?)?;
        let offset = usize::try_from(read_u32_be(sfnt, entry.checked_add(8)?)?).ok()?;
        let length = usize::try_from(read_u32_be(sfnt, entry.checked_add(12)?)?).ok()?;
        let table_bytes = sfnt.get(offset..offset.checked_add(length)?)?;
        if table_bytes.is_empty() {
            return None;
        }
        let compressed = if compress_tables {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            std::io::Write::write_all(&mut encoder, table_bytes).ok()?;
            let candidate = encoder.finish().ok()?;
            if candidate.len() < table_bytes.len() {
                compressed_any = true;
                candidate
            } else {
                table_bytes.to_vec()
            }
        } else {
            table_bytes.to_vec()
        };
        tables.push((
            read_u32_be(sfnt, entry)?,
            compressed,
            length,
            read_u32_be(sfnt, entry.checked_add(4)?)?,
        ));
    }
    tables.sort_unstable_by_key(|table| table.0);
    let header_size = WOFF_HEADER_BYTES.checked_add(table_count.checked_mul(WOFF_TABLE_BYTES)?)?;
    let mut woff = vec![0; header_size];
    write_u32_be(&mut woff, 0, 0x774f_4646);
    write_u32_be(&mut woff, 4, flavor);
    write_u16_be(&mut woff, 12, u16::try_from(table_count).ok()?);
    write_u32_be(&mut woff, 16, u32::try_from(sfnt.len()).ok()?);
    write_u16_be(&mut woff, 20, 1);
    for (index, (tag, table_bytes, original_len, checksum)) in tables.iter().enumerate() {
        while woff.len() % 4 != 0 {
            woff.push(0);
        }
        let offset = woff.len();
        woff.extend_from_slice(table_bytes);
        let entry = WOFF_HEADER_BYTES.checked_add(index.checked_mul(WOFF_TABLE_BYTES)?)?;
        write_u32_be(&mut woff, entry, *tag);
        write_u32_be(
            &mut woff,
            entry.checked_add(4)?,
            u32::try_from(offset).ok()?,
        );
        write_u32_be(
            &mut woff,
            entry.checked_add(8)?,
            u32::try_from(table_bytes.len()).ok()?,
        );
        write_u32_be(
            &mut woff,
            entry.checked_add(12)?,
            u32::try_from(*original_len).ok()?,
        );
        write_u32_be(&mut woff, entry.checked_add(16)?, *checksum);
    }
    let woff_length = u32::try_from(woff.len()).ok()?;
    write_u32_be(&mut woff, 8, woff_length);
    Some((woff, compressed_any))
}

#[test]
fn woff_sources_are_normalized_before_native_font_admission() {
    let Some(sfnt) = system_font_candidates()
        .iter()
        .find_map(|candidate| std::fs::read(candidate.0).ok())
    else {
        return;
    };
    let Some((uncompressed, _)) = test_woff_from_sfnt(&sfnt, false) else {
        return;
    };
    assert!(NativeFontBook::is_parseable_font_bytes(&uncompressed));
    let Some((compressed, compressed_any)) = test_woff_from_sfnt(&sfnt, true) else {
        return;
    };
    assert!(compressed_any);
    assert!(NativeFontBook::is_parseable_font_bytes(&compressed));
    let resource = NativeFontFaceResource {
        family: "WOFF Face".into(),
        family_key: font_family_hash("WOFF Face"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(compressed),
        unicode_ranges: Vec::new(),
    };
    let book = NativeFontBook::from_resources(&[resource]);
    assert_eq!(
        book.faces.first().map(|face| face.family.as_str()),
        Some("WOFF Face")
    );
    assert_ne!(book.faces.first().unwrap().font_data.as_ref(), b"wOFF");
}

#[test]
fn woff2_sources_are_normalized_before_native_font_admission() {
    let encoded = "d09GMnR0Y2YAAAdMAA8AAAAAFDgAAAbKAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABmAWi2AAglIKhSaFFQsKAAE2AiRDEAkEIAWEfAcgCoUihH8LCgADEAWEfAABAAACCwABAAAAAQIDBAUGBwgJCgsAAQAAAAECCwwFBg0IDgobUBKzERVsHABafgzgv0zgxhDsH2ROglBAUXPRWrfTqsQSDEdrtVhjWE/7S94TH/QXd4C7xQIMwYOCP0I5KGFCaOcgqP1+e/Lui2hEpYtpJDI088YQCqGJRUJUSSQyQybB8Hn6GcT3JieFUjCdC3HQB4iTbtBt8p8/9+etmyUYJ57UuHHjF/t9Cdt2E14MrQmGbqh5U3NDNA8x+R+iU0T1ul7XUCmNUAjVS6JRGVhnodURvIl8GIPzQiqjD5CI2I9f5c3Nb9iGiLxE+qHhmvpXoZlbJFpoJJqohfhLRUtmOiERiUQSy/BaWOfSAbwDyl0TsQ4i2joQyQgiR5ekns0uaGRBzVVYL0T4oL7NH2CdIaiOxswmDwjELQVufg9RMP8+KjYa9ugMHEycsdi4ONxcnnh8XgFfQhG/WEAiGA1yCgglUVTUxDQktKTR0ZMxkMfIRMFMGQsrFRs1O00cnLRcdHHz0PMyAIyBUCY+5vgFWARZhdgSFmEX5UhMnBPGBecOgeRB8YaWACTBpKShMnyy/MnJCygIpqgkpCysIpKqmqi6WBqa4lowbXg6ugg9ZPoGKEO0kUTGJpKmUpmZS/MBLhNBylJyA2r5/oGAFUAUAwVTAleGUAkSShVaLRhYdTgNeM0QELWQtENG0UHVRdMLHUMf0yAsbAAHxIXCw4cJkAiJUGIsElKcjCAno6CkqOioaRhalo6LnoFnFGJiFlkkVlls7HIORZxcSm4VQB0Pr4ZPG7+ATlCfkLBBxChqSkzcLGFJUsoqbZOxJyvnkHemoOhScit7UlH1qvlS1/BrEmuRpK1DqkuWnj65AUWGRpTGVCbUmVae0fRnTpuFJZ0VvTVDNraMdpiya4/ZPosD1hw6YnPMnhOnHM44nXPlwiW3K0Cu3fC45XXHl3sP/B4F8uRZ0ItQXr0JexfxIZpPX2K+xfPjV8KfpH+ppH95K31U+fqp/TVIOw8o/FnSBABdohBAmqYC2LEjgbwW8lLoQHrQrV7EENMmDOMs6Cn51yoYDZ9vB1zANxBzZQu60MLYnUarSH4+QXWEZqYtuUfcQ8tZkanSI93SveKx+2RwRsqie9k1sRDtwitcy07h29uhNza3u2jTJ5KHLvZ518tvtnfmM5OWgotWpPPD6q2t2Hpjc5+Rfq5v4YrjWD2ru+LEGFeJ6jqrrN5s/tGSUVWoulBV8Zn7vnhWFlt3Fk0rO0tL1BX+J5fzeKxNXrWnSLLD6n7SJDW6jzuqG9+ByTuTJjomxn685O741pEDVfWL/c1I+JN5NHVixgcsT/02NwU3kBneAMaRK/jMIH+TXs/fCFXQRSOKSPWeh18733Wc6qHOAjIVZILEwTJTVaA4+24+3Lx4OaACk2TtWyDsM3cbhocFEp67wv1NqTLSGMUGQjatzVS1iH4wbOwBQ3yDcNO04BHc3eHPlRVPlBfrFSBWm7zPX+sbWUWXcb2VrZIaF5ddaCdUMzvtNf3NOx22cP2MNqVgyYh9P8XGtuRIC6gZif83X3b72LFrG29tPvb32OaNt7Z2PPpcHQrwLqkghlo6/iqKin2F9RlVGUnf748zAgJd/NHLG2xp7HGyBbodgKvnLRYCvN7JwD39d4E5A+BAAiDAOqP/IHiR17ir4iwYtFCroDtZEosBL05zDzOrz/NyRgS+iFJiw1cqykVLTUxzaRE2VRrC5lfZidqtk9COeVkH4VLi5bJUzHhSaspSWVoURFmYihm72ynJAfZ2ZATx1KF1ajUB0UeCztZ5nPcjtA89orP2h8EOU3m9bGHt0h6uQQ3l648h7oeiWDOKolDUiGp5JrSpje1QtUL1FoiODIEANXkMGQ61qov+OgN1Hj4jwhwG6Xg4Efr0GjZEp8+gkIBePlKD+ozox2ry4fDasqAACNR39iag3j21PAYNCXTRHAg0aY4HsE+va3I91uOFoNz9dzqoT4b6RxXFAFTxLrdHYLcIxRgopdKIMVG97/P56wMZAxndn/cGxnrfb5wY6FrxGan8MLnb9eMjGVwnqsknAro19/EDFFgqW0ATbsI5QTgdbvR/ignHl6pkD3+cvy2RcnWgsAgkaGM8YlNmIDXbKDlds1gCCk9vOgyPSef0i3c3DrMEFbCiJRMfk9qrkLHbPjfUK8JNPWny0WWb2EEVTqomZk1kpNrK0mPt2+/N1+gjBL0WUColWXT7lGmFitMZt6bkaAnn4KvAJSy7nIKW/mBeEmJGekMUYj9u3HarQ/13qNN72ZcdX3Pz6v14+4eJNzwzi/EidlsxgmBTzZJ2euYS/tiLyxGODQ9sRGF/B8gGgiPcCMMAAAA=";
    let woff2 = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("the WPT WOFF2 fixture must be valid base64");
    let normalized = normalize_font_bytes(&woff2).expect("WOFF2 must normalize to SFNT");
    assert!(supported_woff_flavor(
        read_u32_be(&normalized, 0).expect("normalized SFNT must have a flavor")
    ));
    assert!(NativeFontBook::is_parseable_font_bytes(&woff2));
    let resource = NativeFontFaceResource {
        family: "WOFF2 Face".into(),
        family_key: font_family_hash("WOFF2 Face"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(woff2),
        unicode_ranges: Vec::new(),
    };
    let book = NativeFontBook::from_resources(&[resource]);
    assert_eq!(
        book.faces.first().map(|face| face.family.as_str()),
        Some("WOFF2 Face")
    );
    assert_eq!(book.faces.first().unwrap().font_data.as_ref(), normalized);
}

#[test]
fn unicode_ranges_select_the_matching_document_font_face() {
    let Some(system_face) = system_font_book().faces.first() else {
        return;
    };
    if !has_glyph(system_face, 'A') || !has_glyph(system_face, 'B') {
        return;
    }
    let resources = ['A', 'B']
        .into_iter()
        .map(|character| NativeFontFaceResource {
            family: "Range Face".into(),
            family_key: font_family_hash("Range Face"),
            weight: NativeFontWeightRange::default(),
            style: FontStyleValue::Normal,
            stretch: NativeFontStretchRange::default(),
            variation_settings: NativeFontVariationSettings::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
            metric_overrides: NativeFontMetricOverrides::default(),
            font_display: NativeFontDisplay::Auto,
            bytes: system_face.font_data.clone(),
            unicode_ranges: vec![NativeUnicodeRange {
                start: u32::from(character),
                end: u32::from(character),
            }],
        })
        .collect::<Vec<_>>();
    let book = NativeFontBook::from_resources(&resources);
    let families = NativeFontFamilyList::parse("Range Face").unwrap();
    let metrics = NativeTextMetrics::for_style_with_book(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    );
    assert_eq!(metrics.faces.len(), 2);
    assert_eq!(metrics.face_index_for_character('A'), Some(0));
    assert_eq!(metrics.face_index_for_character('B'), Some(1));
    assert_eq!(metrics.face_index_for_character('C'), None);
}

#[test]
fn font_stretch_matches_face_ranges_and_scales_glyph_runs() {
    let Some(system_face) = system_font_book().faces.first() else {
        return;
    };
    if !has_glyph(system_face, 'A') {
        return;
    }
    let family_key = font_family_hash("Stretch Face");
    let resources = [
        NativeFontFaceResource {
            family: "Stretch Face".into(),
            family_key,
            weight: NativeFontWeightRange::default(),
            style: FontStyleValue::Normal,
            stretch: NativeFontStretchRange::default(),
            variation_settings: NativeFontVariationSettings::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
            metric_overrides: NativeFontMetricOverrides::default(),
            font_display: NativeFontDisplay::Auto,
            bytes: system_face.font_data.clone(),
            unicode_ranges: Vec::new(),
        },
        NativeFontFaceResource {
            family: "Stretch Face".into(),
            family_key,
            weight: NativeFontWeightRange::default(),
            style: FontStyleValue::Normal,
            stretch: NativeFontStretchRange { min: 750, max: 750 },
            variation_settings: NativeFontVariationSettings::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
            metric_overrides: NativeFontMetricOverrides::default(),
            font_display: NativeFontDisplay::Auto,
            bytes: system_face.font_data.clone(),
            unicode_ranges: Vec::new(),
        },
    ];
    let book = NativeFontBook::from_resources(&resources);
    let families = NativeFontFamilyList::parse("Stretch Face").unwrap();
    let normal = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        &book,
    );
    let condensed = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 750,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        &book,
    );
    let expanded = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1250,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        &book,
    );

    assert_eq!(normal.faces[0].stretch, NativeFontStretchRange::default());
    assert_eq!(condensed.faces[0].stretch.min, 750);
    assert_eq!(expanded.faces[0].stretch, NativeFontStretchRange::default());
    let normal_width = normal.measure_text("AAAA", 0, 0);
    assert_eq!(
        condensed.measure_text("AAAA", 0, 0),
        normal_width,
        "a singleton condensed face is already at its declared width"
    );
    assert!(expanded.measure_text("AAAA", 0, 0) > normal_width);
    let run = expanded.rasterize("AAAA", 0, 0, 0).unwrap();
    assert!(run.width > normal.rasterize("AAAA", 0, 0, 0).unwrap().width);
}

#[test]
fn font_weight_ranges_match_inside_and_outside_the_declared_interval() {
    let Some(system_face) = system_font_book().faces.first() else {
        return;
    };
    let family_key = font_family_hash("Weight Range Face");
    let resources = [
        NativeFontFaceResource {
            family: "Weight Range Face".into(),
            family_key,
            weight: NativeFontWeightRange { min: 300, max: 700 },
            style: FontStyleValue::Normal,
            stretch: NativeFontStretchRange::default(),
            variation_settings: NativeFontVariationSettings::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
            metric_overrides: NativeFontMetricOverrides::default(),
            font_display: NativeFontDisplay::Auto,
            bytes: system_face.font_data.clone(),
            unicode_ranges: Vec::new(),
        },
        NativeFontFaceResource {
            family: "Weight Range Face".into(),
            family_key,
            weight: NativeFontWeightRange::singleton(FontWeightValue::Numeric(800)),
            style: FontStyleValue::Normal,
            stretch: NativeFontStretchRange::default(),
            variation_settings: NativeFontVariationSettings::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
            metric_overrides: NativeFontMetricOverrides::default(),
            font_display: NativeFontDisplay::Auto,
            bytes: system_face.font_data.clone(),
            unicode_ranges: Vec::new(),
        },
    ];
    let book = NativeFontBook::from_resources(&resources);
    let families = NativeFontFamilyList::parse("Weight Range Face").unwrap();
    let inside = NativeTextMetrics::for_style_with_book(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Numeric(500),
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    );
    let singleton = NativeTextMetrics::for_style_with_book(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Numeric(800),
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    );
    assert_eq!(
        inside.faces[0].weight,
        NativeFontWeightRange { min: 300, max: 700 }
    );
    assert_eq!(
        singleton.faces[0].weight,
        NativeFontWeightRange::singleton(FontWeightValue::Numeric(800))
    );
}

#[test]
fn font_variant_ligatures_toggle_common_shaping_features() {
    let families = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));
    let default_metrics = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        system_font_book(),
    );
    let no_common_metrics = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures {
                common: false,
                ..NativeFontVariantLigatures::default()
            },
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        system_font_book(),
    );
    let Some(default_shape) = default_metrics.shape("fi", 0, 0) else {
        return;
    };
    let Some(no_common_shape) = no_common_metrics.shape("fi", 0, 0) else {
        return;
    };
    if default_shape.glyphs.len() >= 2 {
        return;
    }
    assert!(no_common_shape.glyphs.len() > default_shape.glyphs.len());

    let mut explicit_off_settings = NativeFontFeatureSettings::default();
    explicit_off_settings.values[0] = NativeFontFeature {
        tag: *b"liga",
        value: 0,
    };
    explicit_off_settings.count = 1;
    let explicit_off_metrics = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: explicit_off_settings,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        system_font_book(),
    );
    let Some(explicit_off_shape) = explicit_off_metrics.shape("fi", 0, 0) else {
        return;
    };
    assert_eq!(
        explicit_off_shape.glyphs.len(),
        no_common_shape.glyphs.len()
    );
}

#[test]
fn font_kerning_toggles_kern_feature_and_honors_low_level_override() {
    let families = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));
    let kerning_on = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::Normal,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        system_font_book(),
    );
    let kerning_off = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::None,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        system_font_book(),
    );
    let Some(on_shape) = kerning_on.shape("AV", 0, 0) else {
        return;
    };
    let Some(off_shape) = kerning_off.shape("AV", 0, 0) else {
        return;
    };
    if on_shape.width_fixed == off_shape.width_fixed {
        return;
    }

    let mut low_level_off = NativeFontFeatureSettings::default();
    low_level_off.values[0] = NativeFontFeature {
        tag: *b"kern",
        value: 0,
    };
    low_level_off.count = 1;
    let explicit_off = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: low_level_off,
            kerning: NativeFontKerning::Normal,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
        system_font_book(),
    );
    let Some(explicit_off_shape) = explicit_off.shape("AV", 0, 0) else {
        return;
    };
    assert_eq!(explicit_off_shape.width_fixed, off_shape.width_fixed);
}

#[test]
fn font_variant_caps_maps_to_bounded_opentype_feature_tags() {
    assert_eq!(
        font_variant_caps_tags(NativeFontVariantCaps::Normal),
        ([None, None], 0)
    );
    assert_eq!(
        font_variant_caps_tags(NativeFontVariantCaps::SmallCaps),
        ([Some(*b"smcp"), None], 1)
    );
    assert_eq!(
        font_variant_caps_tags(NativeFontVariantCaps::AllSmallCaps),
        ([Some(*b"c2sc"), Some(*b"smcp")], 2)
    );
    assert_eq!(
        font_variant_caps_tags(NativeFontVariantCaps::PetiteCaps),
        ([Some(*b"pcap"), None], 1)
    );
    assert_eq!(
        font_variant_caps_tags(NativeFontVariantCaps::AllPetiteCaps),
        ([Some(*b"c2pc"), Some(*b"pcap")], 2)
    );
    assert_eq!(
        font_variant_caps_tags(NativeFontVariantCaps::Unicase),
        ([Some(*b"unic"), None], 1)
    );
    assert_eq!(
        font_variant_caps_tags(NativeFontVariantCaps::TitlingCaps),
        ([Some(*b"titl"), None], 1)
    );
}

#[test]
fn font_variant_position_maps_to_bounded_opentype_feature_tags() {
    assert_eq!(
        font_variant_position_tag(NativeFontVariantPosition::Normal),
        None
    );
    assert_eq!(
        font_variant_position_tag(NativeFontVariantPosition::Sub),
        Some(*b"subs")
    );
    assert_eq!(
        font_variant_position_tag(NativeFontVariantPosition::Super),
        Some(*b"sups")
    );
}

#[test]
fn font_variant_position_respects_explicit_low_level_feature_tags() {
    let mut features = Vec::new();
    push_feature_if_not_explicit(
        &mut features,
        NativeFontFeatureSettings::default(),
        *b"subs",
        1,
    );
    assert_eq!(features.len(), 1);
    assert_eq!(features[0].tag, harfrust::Tag::new(b"subs"));
    assert_eq!(features[0].value, 1);

    let mut settings = NativeFontFeatureSettings::default();
    settings.values[0] = NativeFontFeature {
        tag: *b"subs",
        value: 0,
    };
    settings.count = 1;
    let mut overridden = Vec::new();
    push_feature_if_not_explicit(&mut overridden, settings, *b"subs", 1);
    assert!(overridden.is_empty());
}

#[test]
fn font_variant_numeric_maps_to_bounded_opentype_feature_tags() {
    assert_eq!(
        font_variant_numeric_tags(NativeFontVariantNumeric::default()),
        ([None, None, None, None, None, None, None, None], 0)
    );
    let all = NativeFontVariantNumeric {
        figure: NativeFontVariantNumericFigure::Lining,
        spacing: NativeFontVariantNumericSpacing::Tabular,
        fraction: NativeFontVariantNumericFraction::Stacked,
        ordinal: true,
        slashed_zero: true,
    };
    assert_eq!(
        font_variant_numeric_tags(all),
        (
            [
                Some(*b"lnum"),
                Some(*b"tnum"),
                Some(*b"afrc"),
                Some(*b"ordn"),
                Some(*b"zero"),
                None,
                None,
                None,
            ],
            5
        )
    );
    let mut settings = NativeFontFeatureSettings::default();
    settings.values[0] = NativeFontFeature {
        tag: *b"tnum",
        value: 0,
    };
    settings.count = 1;
    let (tags, count) = font_variant_numeric_tags(all);
    let mut admitted = Vec::new();
    for tag in tags.into_iter().take(count).flatten() {
        push_feature_if_not_explicit(&mut admitted, settings, tag, 1);
    }
    assert_eq!(admitted.len(), 4);
    assert!(!admitted.iter().any(|feature| feature.tag == *b"tnum"));
}

#[test]
fn font_variant_alternates_maps_to_hist_and_respects_explicit_features() {
    assert_eq!(
        font_variant_alternates_tag(NativeFontVariantAlternates::Normal),
        None
    );
    assert_eq!(
        font_variant_alternates_tag(NativeFontVariantAlternates::HistoricalForms),
        Some(*b"hist")
    );

    let mut settings = NativeFontFeatureSettings::default();
    settings.values[0] = NativeFontFeature {
        tag: *b"hist",
        value: 0,
    };
    settings.count = 1;
    let mut admitted = Vec::new();
    if let Some(tag) = font_variant_alternates_tag(NativeFontVariantAlternates::HistoricalForms) {
        push_feature_if_not_explicit(&mut admitted, settings, tag, 1);
    }
    assert!(admitted.is_empty());
}

#[test]
fn font_language_override_maps_padded_open_type_tag_for_harfrust() {
    assert_eq!(
        font_language_override_language(NativeFontLanguageOverride::default()),
        None
    );
    let language = font_language_override_language(NativeFontLanguageOverride {
        tag: Some(*b"ENG "),
    })
    .expect("padded OpenType language tag must map to a HarfRust language");
    assert_eq!(language.as_bytes(), b"eng");
    assert_eq!(
        font_language_override_language(NativeFontLanguageOverride {
            tag: Some(*b"EN G"),
        }),
        None
    );
}

#[test]
fn font_variation_settings_change_variable_font_shaping_when_available() {
    let Some(bytes) = [
        "/usr/share/fonts/truetype/ubuntu/Ubuntu[wdth,wght].ttf",
        "/usr/share/fonts/truetype/ubuntu/UbuntuSans[wdth,wght].ttf",
    ]
    .into_iter()
    .find_map(|path| std::fs::read(path).ok()) else {
        return;
    };
    let resource = NativeFontFaceResource {
        family: "Variable Face".into(),
        family_key: font_family_hash("Variable Face"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(bytes),
        unicode_ranges: Vec::new(),
    };
    let book = NativeFontBook::from_resources(&[resource]);
    let families = NativeFontFamilyList::parse("Variable Face").unwrap();
    let normal = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::Auto,
            variant_caps: NativeFontVariantCaps::Normal,
            variant_position: NativeFontVariantPosition::Normal,
            variant_numeric: NativeFontVariantNumeric::default(),
            variant_alternates: NativeFontVariantAlternates::Normal,
            variant_east_asian: NativeFontVariantEastAsian::default(),
            language_override: NativeFontLanguageOverride::default(),
            variation_settings: NativeFontVariationSettings::default(),
            direction: DirectionValue::Ltr,
        },
        &book,
    );
    let mut narrow_settings = NativeFontVariationSettings::default();
    narrow_settings.values[0] = NativeFontVariation {
        tag: *b"wdth",
        value_milli: 75_000,
    };
    narrow_settings.count = 1;
    let narrow = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::Auto,
            variant_caps: NativeFontVariantCaps::Normal,
            variant_position: NativeFontVariantPosition::Normal,
            variant_numeric: NativeFontVariantNumeric::default(),
            variant_alternates: NativeFontVariantAlternates::Normal,
            variant_east_asian: NativeFontVariantEastAsian::default(),
            language_override: NativeFontLanguageOverride::default(),
            variation_settings: narrow_settings,
            direction: DirectionValue::Ltr,
        },
        &book,
    );
    let normal_run = normal
        .shape("AAAA", 0, 0)
        .expect("variable font must shape the normal instance");
    let narrow_run = narrow
        .shape("AAAA", 0, 0)
        .expect("variable font must shape the requested instance");
    assert!(
        narrow_run.width_fixed < normal_run.width_fixed,
        "wdth axis must affect shaped advance: narrow={} normal={}",
        narrow_run.width_fixed,
        normal_run.width_fixed
    );
    let normal_raster = normal
        .rasterize("AAAA", 0, 0, 0)
        .expect("variable font must rasterize the normal instance");
    let narrow_raster = narrow
        .rasterize("AAAA", 0, 0, 0)
        .expect("variable font must rasterize the requested instance");
    let normal_glyph = normal_raster
        .glyphs
        .first()
        .expect("normal variable run must contain a glyph");
    let narrow_glyph = narrow_raster
        .glyphs
        .first()
        .expect("narrow variable run must contain a glyph");
    assert!(
        normal_glyph.width != narrow_glyph.width
            || normal_glyph.height != narrow_glyph.height
            || normal_glyph.coverage != narrow_glyph.coverage,
        "wdth axis must affect the rasterized outline"
    );
    let mut automatic = normal.clone();
    automatic.weight = FontWeightValue::Bold;
    automatic.stretch = 750;
    automatic.variation_settings = NativeFontVariationSettings::default();
    let automatic_face = automatic
        .faces
        .first()
        .expect("automatic-axis witness must retain a variable face");
    let automatic_settings = automatic.effective_variation_settings(automatic_face);
    assert_eq!(
        automatic_settings
            .values()
            .iter()
            .find(|variation| variation.tag == *b"wght")
            .map(|variation| variation.value_milli),
        Some(700_000)
    );
    assert_eq!(
        automatic_settings
            .values()
            .iter()
            .find(|variation| variation.tag == *b"wdth")
            .map(|variation| variation.value_milli),
        Some(75_000)
    );
}

#[test]
fn font_face_variation_defaults_are_overridden_by_authored_axes() {
    let Some(bytes) = [
        "/usr/share/fonts/truetype/ubuntu/Ubuntu[wdth,wght].ttf",
        "/usr/share/fonts/truetype/ubuntu/UbuntuSans[wdth,wght].ttf",
    ]
    .into_iter()
    .find_map(|path| std::fs::read(path).ok()) else {
        return;
    };
    let mut descriptor_settings = NativeFontVariationSettings::default();
    descriptor_settings.values[0] = NativeFontVariation {
        tag: *b"wdth",
        value_milli: 75_000,
    };
    descriptor_settings.count = 1;
    let resource = NativeFontFaceResource {
        family: "Descriptor Variable Face".into(),
        family_key: font_family_hash("Descriptor Variable Face"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: descriptor_settings,
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: Arc::from(bytes),
        unicode_ranges: Vec::new(),
    };
    let book = NativeFontBook::from_resources(&[resource]);
    let families = NativeFontFamilyList::parse("Descriptor Variable Face").unwrap();
    let descriptor_metrics = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::Auto,
            variant_caps: NativeFontVariantCaps::Normal,
            variant_position: NativeFontVariantPosition::Normal,
            variant_numeric: NativeFontVariantNumeric::default(),
            variant_alternates: NativeFontVariantAlternates::Normal,
            variant_east_asian: NativeFontVariantEastAsian::default(),
            language_override: NativeFontLanguageOverride::default(),
            variation_settings: NativeFontVariationSettings::default(),
            direction: DirectionValue::Ltr,
        },
        &book,
    );
    let mut authored_settings = NativeFontVariationSettings::default();
    authored_settings.values[0] = NativeFontVariation {
        tag: *b"wdth",
        value_milli: 100_000,
    };
    authored_settings.count = 1;
    let authored_metrics = NativeTextMetrics::for_style_with_book_and_options(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        NativeTextStyleOptions {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::Auto,
            variant_caps: NativeFontVariantCaps::Normal,
            variant_position: NativeFontVariantPosition::Normal,
            variant_numeric: NativeFontVariantNumeric::default(),
            variant_alternates: NativeFontVariantAlternates::Normal,
            variant_east_asian: NativeFontVariantEastAsian::default(),
            language_override: NativeFontLanguageOverride::default(),
            variation_settings: authored_settings,
            direction: DirectionValue::Ltr,
        },
        &book,
    );
    let descriptor_width = descriptor_metrics
        .shape("AAAA", 0, 0)
        .expect("descriptor variable face must shape")
        .width_fixed;
    let authored_width = authored_metrics
        .shape("AAAA", 0, 0)
        .expect("authored variable face must shape")
        .width_fixed;
    assert!(
        descriptor_width < authored_width,
        "authored wdth must override @font-face default: descriptor={descriptor_width} authored={authored_width}"
    );
}

#[test]
fn font_variant_east_asian_maps_to_bounded_opentype_tags() {
    assert_eq!(
        font_variant_east_asian_tags(NativeFontVariantEastAsian::default()),
        ([None, None, None], 0)
    );
    let all = NativeFontVariantEastAsian {
        form: NativeFontVariantEastAsianForm::Jis83,
        width: NativeFontVariantEastAsianWidth::Full,
        ruby: true,
    };
    assert_eq!(
        font_variant_east_asian_tags(all),
        ([Some(*b"jp83"), Some(*b"fwid"), Some(*b"ruby")], 3)
    );
    let mut settings = NativeFontFeatureSettings::default();
    settings.values[0] = NativeFontFeature {
        tag: *b"fwid",
        value: 0,
    };
    settings.count = 1;
    let (tags, count) = font_variant_east_asian_tags(all);
    let mut admitted = Vec::new();
    for tag in tags.into_iter().take(count).flatten() {
        push_feature_if_not_explicit(&mut admitted, settings, tag, 1);
    }
    assert_eq!(admitted.len(), 2);
    assert!(!admitted.iter().any(|feature| feature.tag == *b"fwid"));
}

#[test]
fn missing_glyph_uses_bounded_primary_replacement() {
    let families = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));
    let metrics = NativeTextMetrics::for_style(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
    );
    if metrics.faces.is_empty() {
        return;
    }
    let missing = char::from_u32(0x10_ffff).unwrap();
    if metrics.face_index_for_character(missing).is_some() {
        return;
    }
    let run = metrics.rasterize(&missing.to_string(), 0, 0, 0).unwrap();
    assert_eq!(run.width, FALLBACK_GLYPH_ADVANCE);
}

#[test]
fn document_font_resource_precedes_system_fallback_for_named_family() {
    let Some(system_face) = system_font_book().faces.first() else {
        return;
    };
    let resource = NativeFontFaceResource {
        family: "Embedded Sans".into(),
        family_key: font_family_hash("Embedded Sans"),
        weight: NativeFontWeightRange::default(),
        style: FontStyleValue::Normal,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: crate::native_engine::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        bytes: system_face.font_data.clone(),
        unicode_ranges: Vec::new(),
    };
    let book = NativeFontBook::from_resources(&[resource]);
    let families = NativeFontFamilyList::parse("Embedded Sans").unwrap();
    let metrics = NativeTextMetrics::for_style_with_book(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
        &book,
    );
    assert!(!metrics.faces.is_empty());
    assert!(metrics.measure_text("Glass", 0, 0) > 0);
    assert_eq!(metrics.faces[0].family, "Embedded Sans");
    assert_eq!(metrics.faces[0].generic_family, None);
}

#[test]
fn shaped_ltr_runs_keep_clusters_and_justified_spaces() {
    let families = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));
    let metrics = NativeTextMetrics::for_style(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
    );
    let Some(face) = metrics.faces.first() else {
        return;
    };
    if face.shaper_data().is_none() {
        return;
    }
    let Some(shaped) = metrics.shape("A B", 1, 2) else {
        return;
    };
    assert!(shaped.width > 0);
    assert!(
        shaped
            .glyphs
            .windows(2)
            .all(|glyphs| glyphs[0].cluster <= glyphs[1].cluster)
    );
    assert_eq!(shaped.width, metrics.measure_text("A B", 1, 2));
    assert_eq!(
        shaped.width,
        metrics.measure_text("A B", 0, 0).saturating_add(5)
    );

    let run = metrics.rasterize("A B", 1, 2, 3).unwrap();
    assert_eq!(run.width, shaped.width.saturating_add(3));
    assert!(
        run.space_ranges
            .iter()
            .any(|range| range.char_index == 1 && range.end > range.start)
    );
}

#[test]
fn large_shaped_runs_keep_cluster_positions() {
    let families = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));
    let metrics = NativeTextMetrics::for_style(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Ltr,
    );
    let Some(face) = metrics.faces.first() else {
        return;
    };
    if face.shaper_data().is_none() {
        return;
    }

    let text = "A".repeat(4_096);
    let Some(shaped) = metrics.shape(&text, 0, 0) else {
        return;
    };
    assert_eq!(shaped.glyphs.len(), text.chars().count());
    assert!(shaped.width > 0);
    assert!(
        shaped
            .glyphs
            .windows(2)
            .all(|glyphs| glyphs[0].cluster <= glyphs[1].cluster)
    );
}

#[test]
fn shaped_rtl_runs_reverse_clusters_and_visual_coordinates() {
    let families = NativeFontFamilyList::single(NativeFontFamilyValue::Generic(
        NativeGenericFontFamily::SansSerif,
    ));
    let metrics = NativeTextMetrics::for_style(
        families,
        DEFAULT_NATIVE_FONT_SIZE,
        FontWeightValue::Normal,
        FontStyleValue::Normal,
        DirectionValue::Rtl,
    );
    let Some(face) = metrics.faces.first() else {
        return;
    };
    if face.shaper_data().is_none() {
        return;
    }
    let Some(shaped) = metrics.shape("ABC", 0, 0) else {
        return;
    };
    assert!(shaped.width > 0);
    assert!(
        shaped
            .glyphs
            .windows(2)
            .all(|glyphs| glyphs[0].cluster >= glyphs[1].cluster)
    );
    let run = metrics.rasterize("ABC", 0, 0, 0).unwrap();
    assert_eq!(run.width, shaped.width);
    assert!(!run.glyphs.is_empty());
    assert!(
        run.glyphs
            .iter()
            .all(|glyph| glyph.x < i32::try_from(run.width).unwrap_or(i32::MAX))
    );
}

#[test]
fn fallback_metrics_keep_the_legacy_cell_contract() {
    let metrics = NativeTextMetrics::fallback_with_style_options(
        DEFAULT_NATIVE_FONT_SIZE,
        NativeTextStyleOptions {
            stretch: 1000,
            direction: DirectionValue::Ltr,
            ..NativeTextStyleOptions::default()
        },
    );
    assert_eq!(metrics.measure_text("A B", 0, 0), 24);
    assert_eq!(metrics.line_height(), FALLBACK_LINE_HEIGHT);
    assert!(metrics.rasterize("A", 0, 0, 0).is_none());
}

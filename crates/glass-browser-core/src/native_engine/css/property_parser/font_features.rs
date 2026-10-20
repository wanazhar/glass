use super::*;

pub(crate) fn parse_font_variation_settings(value: &str) -> Option<NativeFontVariationSettings> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("normal") {
        return Some(NativeFontVariationSettings::default());
    }
    let entries = value.split(',').map(str::trim).collect::<Vec<_>>();
    if entries.is_empty()
        || entries.len() > MAX_NATIVE_FONT_VARIATIONS
        || entries.iter().any(|entry| entry.is_empty())
    {
        return None;
    }
    let mut settings = NativeFontVariationSettings::default();
    for entry in entries {
        let variation = parse_font_variation_entry(entry)?;
        let count = usize::from(settings.count);
        if let Some(existing) = settings.values.get_mut(..count).and_then(|values| {
            values
                .iter_mut()
                .find(|existing| existing.tag == variation.tag)
        }) {
            *existing = variation;
        } else {
            *settings.values.get_mut(count)? = variation;
            settings.count = settings.count.saturating_add(1);
        }
    }
    Some(settings)
}

pub(crate) fn format_font_variation_settings(settings: NativeFontVariationSettings) -> String {
    if settings.values().is_empty() {
        return "normal".into();
    }
    settings
        .values()
        .iter()
        .map(|variation| {
            let tag = std::str::from_utf8(&variation.tag).unwrap_or("    ");
            let magnitude = i64::from(variation.value_milli).unsigned_abs();
            let whole = magnitude / 1_000;
            let fraction = magnitude % 1_000;
            let number = if fraction == 0 {
                whole.to_string()
            } else {
                let fractional = format!("{fraction:03}");
                format!("{whole}.{}", fractional.trim_end_matches('0'))
            };
            let number = if variation.value_milli < 0 {
                format!("-{number}")
            } else {
                number
            };
            format!("\"{tag}\" {number}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn parse_font_face_variant(value: &str) -> Option<NativeFontFeatureSettings> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("normal") {
        return Some(NativeFontFeatureSettings::default());
    }
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }
    parse_font_variant_shorthand(trimmed)?;
    let mut settings = NativeFontFeatureSettings::default();
    macro_rules! add_feature {
        ($tag:literal, $value:expr) => {
            if !settings.insert(NativeFontFeature {
                tag: *$tag,
                value: $value,
            }) {
                return None;
            }
        };
    }
    for token in trimmed.split_ascii_whitespace() {
        match token.to_ascii_lowercase().as_str() {
            "common-ligatures" => {
                add_feature!(b"liga", 1);
                add_feature!(b"clig", 1);
            }
            "no-common-ligatures" => {
                add_feature!(b"liga", 0);
                add_feature!(b"clig", 0);
            }
            "discretionary-ligatures" => add_feature!(b"dlig", 1),
            "no-discretionary-ligatures" => add_feature!(b"dlig", 0),
            "historical-ligatures" => add_feature!(b"hlig", 1),
            "no-historical-ligatures" => add_feature!(b"hlig", 0),
            "contextual" => add_feature!(b"calt", 1),
            "no-contextual" => add_feature!(b"calt", 0),
            "small-caps" => add_feature!(b"smcp", 1),
            "all-small-caps" => {
                add_feature!(b"c2sc", 1);
                add_feature!(b"smcp", 1);
            }
            "petite-caps" => add_feature!(b"pcap", 1),
            "all-petite-caps" => {
                add_feature!(b"c2pc", 1);
                add_feature!(b"pcap", 1);
            }
            "unicase" => add_feature!(b"unic", 1),
            "titling-caps" => add_feature!(b"titl", 1),
            "sub" => add_feature!(b"subs", 1),
            "super" => add_feature!(b"sups", 1),
            "historical-forms" => add_feature!(b"hist", 1),
            "jis78" => add_feature!(b"jp78", 1),
            "jis83" => add_feature!(b"jp83", 1),
            "jis90" => add_feature!(b"jp90", 1),
            "jis04" => add_feature!(b"jp04", 1),
            "simplified" => add_feature!(b"smpl", 1),
            "traditional" => add_feature!(b"trad", 1),
            "full-width" => add_feature!(b"fwid", 1),
            "proportional-width" => add_feature!(b"pwid", 1),
            "ruby" => add_feature!(b"ruby", 1),
            "lining-nums" => add_feature!(b"lnum", 1),
            "oldstyle-nums" => add_feature!(b"onum", 1),
            "proportional-nums" => add_feature!(b"pnum", 1),
            "tabular-nums" => add_feature!(b"tnum", 1),
            "diagonal-fractions" => add_feature!(b"frac", 1),
            "stacked-fractions" => add_feature!(b"afrc", 1),
            "ordinal" => add_feature!(b"ordn", 1),
            "slashed-zero" => add_feature!(b"zero", 1),
            _ => return None,
        }
    }
    Some(settings)
}

pub(crate) fn parse_font_feature_settings(value: &str) -> Option<NativeFontFeatureSettings> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("normal") {
        return Some(NativeFontFeatureSettings::default());
    }
    let entries = value.split(',').map(str::trim).collect::<Vec<_>>();
    if entries.is_empty()
        || entries.len() > MAX_NATIVE_FONT_FEATURES
        || entries.iter().any(|entry| entry.is_empty())
    {
        return None;
    }

    let mut settings = NativeFontFeatureSettings::default();
    for entry in entries {
        let feature = parse_font_feature_entry(entry)?;
        let count = usize::from(settings.count);
        if let Some(existing) = settings.values.get_mut(..count).and_then(|values| {
            values
                .iter_mut()
                .find(|existing| existing.tag == feature.tag)
        }) {
            *existing = feature;
        } else {
            *settings.values.get_mut(count)? = feature;
            settings.count = settings.count.saturating_add(1);
        }
    }
    Some(settings)
}

pub(crate) fn format_font_feature_settings(settings: NativeFontFeatureSettings) -> String {
    if settings.values().is_empty() {
        return "normal".into();
    }
    settings
        .values()
        .iter()
        .map(|feature| {
            let tag = std::str::from_utf8(&feature.tag).unwrap_or("    ");
            format!("\"{tag}\" {}", feature.value)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

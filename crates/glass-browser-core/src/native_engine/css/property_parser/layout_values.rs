use super::*;

pub(in crate::native_engine::css) fn parse_box_sizing(value: &str) -> Option<NativeBoxSizing> {
    match value.to_ascii_lowercase().as_str() {
        "content-box" => Some(NativeBoxSizing::ContentBox),
        "border-box" => Some(NativeBoxSizing::BorderBox),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_display(value: &str) -> Option<DisplayValue> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(DisplayValue::None),
        "block" | "flow-root" | "list-item" | "table" => Some(DisplayValue::Block),
        "inline" | "inline-block" | "inline-flex" | "inline-grid" => Some(DisplayValue::Inline),
        "contents" => Some(DisplayValue::Contents),
        "flex" => Some(DisplayValue::Flex),
        "grid" => Some(DisplayValue::Grid),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_display_custom_property(
    value: &str,
) -> Option<DisplayDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = arguments
        .split_once(',')
        .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback)
            if fallback
                .as_bytes()
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"var(")) =>
        {
            None
        }
        Some(fallback) => parse_display(fallback.trim())
            .map(|fallback| DisplayDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(DisplayDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_display_property(
    value: &str,
) -> Option<DisplayDeclarationValue> {
    parse_display(value)
        .map(DisplayDeclarationValue::Value)
        .or_else(|| parse_display_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_display_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<DisplayDeclarationValue>> {
    parse_local_cascade_declaration(value, parse_display_property)
}

pub(in crate::native_engine::css) fn supports_display_declaration(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("revert-layer")
        || parse_display_custom_property(value).is_some()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "none"
                | "block"
                | "flow-root"
                | "list-item"
                | "table"
                | "inline"
                | "inline-block"
                | "inline-flex"
                | "inline-grid"
                | "flex"
                | "grid"
                | "contents"
        )
}

pub(in crate::native_engine::css) fn parse_position(value: &str) -> Option<NativePositionValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "static" => Some(NativePositionValue::Static),
        "relative" => Some(NativePositionValue::Relative),
        "absolute" => Some(NativePositionValue::Absolute),
        "fixed" => Some(NativePositionValue::Fixed),
        "sticky" => Some(NativePositionValue::Sticky),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_position_custom_property(
    value: &str,
) -> Option<PositionDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = arguments
        .split_once(',')
        .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback)
            if fallback
                .as_bytes()
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"var(")) =>
        {
            None
        }
        Some(fallback) => parse_position(fallback)
            .map(|fallback| PositionDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(PositionDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_position_property(
    value: &str,
) -> Option<PositionDeclarationValue> {
    parse_position(value)
        .map(PositionDeclarationValue::Value)
        .or_else(|| parse_position_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_position_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<PositionDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_position_property)
}

pub(in crate::native_engine::css) fn parse_z_index(value: &str) -> Option<NativeZIndexValue> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") {
        return Some(NativeZIndexValue::Auto);
    }
    let value = value.parse::<i64>().ok()?;
    (i64::from(MIN_NATIVE_Z_INDEX)..=i64::from(MAX_NATIVE_Z_INDEX))
        .contains(&value)
        .then_some(NativeZIndexValue::Integer(i32::try_from(value).ok()?))
}

pub(in crate::native_engine::css) fn parse_z_index_custom_property(
    value: &str,
) -> Option<ZIndexDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = arguments
        .split_once(',')
        .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback)
            if fallback
                .as_bytes()
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"var(")) =>
        {
            None
        }
        Some(fallback) => parse_z_index(fallback)
            .map(|fallback| ZIndexDeclarationValue::CustomPropertyFallback(name_hash, fallback)),
        None => Some(ZIndexDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_z_index_property(
    value: &str,
) -> Option<ZIndexDeclarationValue> {
    parse_z_index(value)
        .map(ZIndexDeclarationValue::Value)
        .or_else(|| parse_z_index_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_z_index_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<ZIndexDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_z_index_property)
}

pub(in crate::native_engine::css) fn parse_pointer_events(
    value: &str,
) -> Option<NativePointerEventsValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(NativePointerEventsValue::Auto),
        "none" => Some(NativePointerEventsValue::None),
        _ => None,
    }
}

pub(in crate::native_engine::css) fn parse_pointer_events_custom_property(
    value: &str,
) -> Option<PointerEventsDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = arguments
        .split_once(',')
        .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback)
            if fallback
                .as_bytes()
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"var(")) =>
        {
            None
        }
        Some(fallback) => parse_pointer_events(fallback).map(|fallback| {
            PointerEventsDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(PointerEventsDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_pointer_events_property(
    value: &str,
) -> Option<PointerEventsDeclarationValue> {
    parse_pointer_events(value)
        .map(PointerEventsDeclarationValue::Value)
        .or_else(|| parse_pointer_events_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_pointer_events_declaration(
    value: &str,
) -> Option<InheritedTextDeclaration<PointerEventsDeclarationValue>> {
    parse_inherited_text_declaration(value, parse_pointer_events_property)
}

pub(in crate::native_engine::css) fn parse_position_offset(
    value: &str,
) -> Option<NativePositionOffset> {
    let value = value.trim().to_ascii_lowercase();
    if value == "auto" {
        return Some(NativePositionOffset::Auto);
    }
    let value = if value == "0" {
        "0"
    } else {
        value.strip_suffix("px")?.trim()
    };
    if value.is_empty() {
        return None;
    }
    let value = value.parse::<i64>().ok()?;
    (value.abs() <= i64::from(MAX_NATIVE_VIEWPORT_DIMENSION))
        .then_some(NativePositionOffset::Length(i32::try_from(value).ok()?))
}

pub(in crate::native_engine::css) fn parse_position_offset_custom_property(
    value: &str,
) -> Option<PositionOffsetDeclarationValue> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = arguments
        .split_once(',')
        .map_or((arguments, None), |(name, fallback)| (name, Some(fallback)));
    let name_hash = parse_custom_property_name(name)?;
    match fallback {
        Some(fallback)
            if fallback
                .as_bytes()
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"var(")) =>
        {
            None
        }
        Some(fallback) => parse_position_offset(fallback).map(|fallback| {
            PositionOffsetDeclarationValue::CustomPropertyFallback(name_hash, fallback)
        }),
        None => Some(PositionOffsetDeclarationValue::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_position_offset_property(
    value: &str,
) -> Option<PositionOffsetDeclarationValue> {
    parse_position_offset(value)
        .map(PositionOffsetDeclarationValue::Value)
        .or_else(|| parse_position_offset_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_position_offset_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<PositionOffsetDeclarationValue>> {
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    parse_local_reset_cascade_declaration(value, parse_position_offset_property)
}

pub(in crate::native_engine::css) fn parse_grid_track_list_declaration(
    value: &str,
) -> Option<LocalCascadeDeclaration<GridTrackListDeclaration>> {
    let value = value.trim();
    if is_inherit_keyword(value) {
        return Some(LocalCascadeDeclaration::Inherit);
    }
    if is_local_reset_keyword(value) {
        return Some(LocalCascadeDeclaration::Reset);
    }
    if value.eq_ignore_ascii_case("revert-layer") {
        return Some(LocalCascadeDeclaration::RevertLayer);
    }
    parse_grid_track_list_property(value).map(LocalCascadeDeclaration::Value)
}

pub(in crate::native_engine::css) fn parse_grid_track_list_property(
    value: &str,
) -> Option<GridTrackListDeclaration> {
    parse_grid_track_list(value)
        .map(GridTrackListDeclaration::Value)
        .or_else(|| parse_grid_track_list_custom_property(value))
}

pub(in crate::native_engine::css) fn parse_grid_track_list_custom_property(
    value: &str,
) -> Option<GridTrackListDeclaration> {
    let (name_hash, fallback) = parse_flex_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_grid_track_list(fallback)
            .map(|fallback| GridTrackListDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(GridTrackListDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_grid_track_list(
    value: &str,
) -> Option<NativeGridTrackList> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(NativeGridTrackList::default());
    }
    let tokens = split_grid_track_tokens(value)?;
    let mut tracks = NativeGridTrackList::default();
    for token in tokens {
        if let Some((count, track)) = parse_grid_repeat(token) {
            for _ in 0..count {
                if !tracks.push(track) {
                    return None;
                }
            }
        } else {
            let track = parse_grid_track(token)?;
            if !tracks.push(track) {
                return None;
            }
        }
    }
    (tracks.len() > 0).then_some(tracks)
}

pub(in crate::native_engine::css) fn split_grid_track_tokens(value: &str) -> Option<Vec<&str>> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    for (index, character) in value.char_indices() {
        match character {
            '(' => depth = depth.checked_add(1)?,
            ')' => {
                depth = depth.checked_sub(1)?;
            }
            character if character.is_ascii_whitespace() && depth == 0 => {
                if let Some(start) = start.take() {
                    tokens.push(&value[start..index]);
                }
                continue;
            }
            _ => {}
        }
        if start.is_none() {
            start = Some(index);
        }
    }
    if depth != 0 {
        return None;
    }
    if let Some(start) = start {
        tokens.push(&value[start..]);
    }
    (!tokens.is_empty()).then_some(tokens)
}

pub(in crate::native_engine::css) fn parse_grid_repeat(
    value: &str,
) -> Option<(usize, NativeGridTrack)> {
    let value = value.trim().to_ascii_lowercase();
    let inner = value.strip_prefix("repeat(")?.strip_suffix(')')?;
    let (count, track) = inner.split_once(',')?;
    let count = count.trim().parse::<usize>().ok()?;
    if count == 0 || count > MAX_NATIVE_GRID_TRACKS {
        return None;
    }
    Some((count, parse_grid_track(track.trim())?))
}

pub(in crate::native_engine::css) fn parse_grid_track(value: &str) -> Option<NativeGridTrack> {
    let value = value.trim().to_ascii_lowercase();
    if matches!(value.as_str(), "auto" | "min-content" | "max-content") {
        return Some(NativeGridTrack::Auto);
    }
    if let Some(fr) = value.strip_suffix("fr") {
        if fr.is_empty() || !fr.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        return Some(NativeGridTrack::Fr(fr.parse().ok()?));
    }
    parse_dimension(&value).map(NativeGridTrack::Length)
}

pub(in crate::native_engine::css) fn parse_dimension(value: &str) -> Option<u32> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.strip_suffix("px")?.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = value.parse::<u32>().ok()?;
    (value <= MAX_NATIVE_VIEWPORT_DIMENSION).then_some(value)
}

pub(in crate::native_engine::css) fn parse_gap(value: &str) -> Option<NativeGapValue> {
    let mut values = value.split_ascii_whitespace();
    let row = parse_dimension(values.next()?)?;
    let column = match values.next() {
        Some(value) => parse_dimension(value)?,
        None => row,
    };
    values
        .next()
        .is_none()
        .then_some(NativeGapValue { row, column })
}

pub(in crate::native_engine::css) fn parse_gap_var_arguments(
    value: &str,
) -> Option<(u64, Option<&str>)> {
    let value = value.trim();
    let arguments = value
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("var("))
        .and_then(|_| value.strip_suffix(')'))?
        .get(4..)?;
    let (name, fallback) = parse_font_size_var_arguments(arguments)?;
    Some((parse_custom_property_name(name)?, fallback))
}

pub(in crate::native_engine::css) fn parse_gap_custom_property(
    value: &str,
) -> Option<GapShorthandDeclaration> {
    let (name_hash, fallback) = parse_gap_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_gap(fallback)
            .map(|fallback| GapShorthandDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(GapShorthandDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn parse_gap_component_custom_property(
    value: &str,
) -> Option<GapComponentDeclaration> {
    let (name_hash, fallback) = parse_gap_var_arguments(value)?;
    match fallback {
        Some(fallback) => parse_dimension(fallback)
            .map(|fallback| GapComponentDeclaration::CustomPropertyFallback(name_hash, fallback)),
        None => Some(GapComponentDeclaration::CustomProperty(name_hash)),
    }
}

pub(in crate::native_engine::css) fn project_gap_shorthand_declaration(
    declaration: GapShorthandDeclaration,
    axis: u8,
) -> GapComponentDeclaration {
    match declaration {
        GapShorthandDeclaration::Value(value) => {
            GapComponentDeclaration::Value(if axis == 0 { value.row } else { value.column })
        }
        GapShorthandDeclaration::CustomProperty(name_hash) => {
            GapComponentDeclaration::CustomPropertyShorthand(name_hash, axis)
        }
        GapShorthandDeclaration::CustomPropertyFallback(name_hash, fallback) => {
            GapComponentDeclaration::CustomPropertyShorthandFallback(
                name_hash,
                axis,
                if axis == 0 {
                    fallback.row
                } else {
                    fallback.column
                },
            )
        }
        GapShorthandDeclaration::Inherit => GapComponentDeclaration::Inherit,
        GapShorthandDeclaration::Initial => GapComponentDeclaration::Initial,
        GapShorthandDeclaration::Unset => GapComponentDeclaration::Unset,
        GapShorthandDeclaration::Revert => GapComponentDeclaration::Revert,
        GapShorthandDeclaration::RevertLayer => GapComponentDeclaration::RevertLayer,
    }
}

pub(in crate::native_engine::css) fn parse_gap_declaration(
    value: &str,
) -> Option<GapShorthandDeclaration> {
    match value.trim().to_ascii_lowercase().as_str() {
        "inherit" => Some(GapShorthandDeclaration::Inherit),
        "initial" => Some(GapShorthandDeclaration::Initial),
        "unset" => Some(GapShorthandDeclaration::Unset),
        "revert" => Some(GapShorthandDeclaration::Revert),
        "revert-layer" => Some(GapShorthandDeclaration::RevertLayer),
        _ => parse_gap(value)
            .map(GapShorthandDeclaration::Value)
            .or_else(|| parse_gap_custom_property(value)),
    }
}

pub(in crate::native_engine::css) fn parse_gap_component_declaration(
    value: &str,
) -> Option<GapComponentDeclaration> {
    match value.trim().to_ascii_lowercase().as_str() {
        "inherit" => Some(GapComponentDeclaration::Inherit),
        "initial" => Some(GapComponentDeclaration::Initial),
        "unset" => Some(GapComponentDeclaration::Unset),
        "revert" => Some(GapComponentDeclaration::Revert),
        "revert-layer" => Some(GapComponentDeclaration::RevertLayer),
        _ => parse_dimension(value)
            .map(GapComponentDeclaration::Value)
            .or_else(|| parse_gap_component_custom_property(value)),
    }
}

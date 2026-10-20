use super::*;

pub(super) fn wins<T>(
    specificity: u16,
    order: usize,
    inline: bool,
    current: Option<CascadeValue<T>>,
) -> bool {
    current.is_none_or(|current| {
        (inline, specificity, order) > (current.inline, current.specificity, current.order)
    })
}

pub(super) fn cascade_layer_index(specificity: u16) -> usize {
    usize::from(specificity / CASCADE_SPECIFICITY_STRIDE).min(usize::from(UNLAYERED_CASCADE_LAYER))
}

pub(super) fn encode_cascade_specificity(specificity: u16, layer: Option<usize>) -> u16 {
    let layer = layer
        .and_then(|layer| u16::try_from(layer).ok())
        .unwrap_or(UNLAYERED_CASCADE_LAYER)
        .min(UNLAYERED_CASCADE_LAYER);
    layer
        .saturating_mul(CASCADE_SPECIFICITY_STRIDE)
        .saturating_add(specificity.min(MAX_NATIVE_SELECTOR_SPECIFICITY))
}

pub(super) fn resolve_white_space_declaration(
    declaration: InheritedTextDeclaration<WhiteSpaceDeclarationValue>,
    inherited: WhiteSpaceValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<WhiteSpaceDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_white_space_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(WhiteSpaceDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => {
            Some(WhiteSpaceDeclarationValue::Value(WhiteSpaceValue::Normal))
        }
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_white_space_value(
    value: WhiteSpaceDeclarationValue,
    inherited: WhiteSpaceValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<WhiteSpaceDeclarationValue> {
    match value {
        WhiteSpaceDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_white_space_declaration(value))
                .and_then(|declaration| {
                    resolve_white_space_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        WhiteSpaceDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_white_space_declaration(value))
                .and_then(|declaration| {
                    resolve_white_space_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(WhiteSpaceDeclarationValue::Value(fallback)))
        }
        WhiteSpaceDeclarationValue::CustomProperty(_)
        | WhiteSpaceDeclarationValue::CustomPropertyFallback(_, _) => None,
        WhiteSpaceDeclarationValue::Value(value) => Some(WhiteSpaceDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_white_space(
    candidates: NativeTextCascadeCandidates<WhiteSpaceDeclarationValue>,
    inherited: WhiteSpaceValue,
    custom_properties: &BTreeMap<u64, String>,
) -> WhiteSpaceValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        WhiteSpaceDeclarationValue::Value(inherited),
        |declaration| resolve_white_space_declaration(declaration, inherited, custom_properties, 0),
    );
    match resolved {
        WhiteSpaceDeclarationValue::Value(value) => value,
        WhiteSpaceDeclarationValue::CustomProperty(_)
        | WhiteSpaceDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_line_height_declaration(
    declaration: InheritedTextDeclaration<LineHeightDeclarationValue>,
    inherited: Option<u32>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<LineHeightDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_line_height_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(match inherited {
            Some(value) => LineHeightDeclarationValue::Value(value),
            None => LineHeightDeclarationValue::Initial,
        }),
        InheritedTextDeclaration::Initial => Some(LineHeightDeclarationValue::Initial),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_line_height_value(
    value: LineHeightDeclarationValue,
    inherited: Option<u32>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<LineHeightDeclarationValue> {
    match value {
        LineHeightDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_line_height_declaration(value))
                .and_then(|declaration| {
                    resolve_line_height_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        LineHeightDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_line_height_declaration(value))
                .and_then(|declaration| {
                    resolve_line_height_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(LineHeightDeclarationValue::Value(fallback)))
        }
        LineHeightDeclarationValue::CustomProperty(_)
        | LineHeightDeclarationValue::CustomPropertyFallback(_, _) => None,
        value => Some(value),
    }
}

pub(super) fn resolve_line_height(
    candidates: NativeTextCascadeCandidates<LineHeightDeclarationValue>,
    inherited: Option<u32>,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<u32> {
    let fallback = match inherited {
        Some(value) => LineHeightDeclarationValue::Value(value),
        None => LineHeightDeclarationValue::Initial,
    };
    let resolved = resolve_alignment_candidates(candidates, fallback, |declaration| {
        resolve_line_height_declaration(declaration, inherited, custom_properties, 0)
    });
    match resolved {
        LineHeightDeclarationValue::Value(value) => Some(value),
        LineHeightDeclarationValue::Initial => None,
        LineHeightDeclarationValue::CustomProperty(_)
        | LineHeightDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_word_break_declaration(
    declaration: InheritedTextDeclaration<WordBreakDeclarationValue>,
    inherited: WordBreakValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<WordBreakDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_word_break_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(WordBreakDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => {
            Some(WordBreakDeclarationValue::Value(WordBreakValue::Normal))
        }
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_word_break_value(
    value: WordBreakDeclarationValue,
    inherited: WordBreakValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<WordBreakDeclarationValue> {
    match value {
        WordBreakDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_word_break_declaration(value))
                .and_then(|declaration| {
                    resolve_word_break_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        WordBreakDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_word_break_declaration(value))
                .and_then(|declaration| {
                    resolve_word_break_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(WordBreakDeclarationValue::Value(fallback)))
        }
        WordBreakDeclarationValue::CustomProperty(_)
        | WordBreakDeclarationValue::CustomPropertyFallback(_, _) => None,
        value => Some(value),
    }
}

pub(super) fn resolve_word_break(
    candidates: NativeTextCascadeCandidates<WordBreakDeclarationValue>,
    inherited: WordBreakValue,
    custom_properties: &BTreeMap<u64, String>,
) -> WordBreakValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        WordBreakDeclarationValue::Value(inherited),
        |declaration| resolve_word_break_declaration(declaration, inherited, custom_properties, 0),
    );
    match resolved {
        WordBreakDeclarationValue::Value(value) => value,
        WordBreakDeclarationValue::CustomProperty(_)
        | WordBreakDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_vertical_align_declaration(
    declaration: InheritedTextDeclaration<VerticalAlignDeclarationValue>,
    inherited: VerticalAlignValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<VerticalAlignDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_vertical_align_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(VerticalAlignDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(VerticalAlignDeclarationValue::Value(
            VerticalAlignValue::Baseline,
        )),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_vertical_align_value(
    value: VerticalAlignDeclarationValue,
    inherited: VerticalAlignValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<VerticalAlignDeclarationValue> {
    match value {
        VerticalAlignDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_vertical_align_declaration(value))
                .and_then(|declaration| {
                    resolve_vertical_align_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        VerticalAlignDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_vertical_align_declaration(value))
                .and_then(|declaration| {
                    resolve_vertical_align_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(VerticalAlignDeclarationValue::Value(fallback)))
        }
        VerticalAlignDeclarationValue::CustomProperty(_)
        | VerticalAlignDeclarationValue::CustomPropertyFallback(_, _) => None,
        value => Some(value),
    }
}

pub(super) fn resolve_vertical_align(
    candidates: NativeTextCascadeCandidates<VerticalAlignDeclarationValue>,
    inherited: VerticalAlignValue,
    custom_properties: &BTreeMap<u64, String>,
) -> VerticalAlignValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        VerticalAlignDeclarationValue::Value(inherited),
        |declaration| {
            resolve_vertical_align_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        VerticalAlignDeclarationValue::Value(value) => value,
        VerticalAlignDeclarationValue::CustomProperty(_)
        | VerticalAlignDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_word_spacing_declaration(
    declaration: InheritedTextDeclaration<WordSpacingDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<WordSpacingDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_word_spacing_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(WordSpacingDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(WordSpacingDeclarationValue::Value(0)),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_word_spacing_value(
    value: WordSpacingDeclarationValue,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<WordSpacingDeclarationValue> {
    match value {
        WordSpacingDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_word_spacing_declaration(value))
                .and_then(|declaration| {
                    resolve_word_spacing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        WordSpacingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_word_spacing_declaration(value))
                .and_then(|declaration| {
                    resolve_word_spacing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(WordSpacingDeclarationValue::Value(fallback)))
        }
        WordSpacingDeclarationValue::CustomProperty(_)
        | WordSpacingDeclarationValue::CustomPropertyFallback(_, _) => None,
        value => Some(value),
    }
}

pub(super) fn resolve_word_spacing(
    candidates: NativeTextCascadeCandidates<WordSpacingDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    let resolved = resolve_alignment_candidates(
        candidates,
        WordSpacingDeclarationValue::Value(inherited),
        |declaration| {
            resolve_word_spacing_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        WordSpacingDeclarationValue::Value(value) => value,
        WordSpacingDeclarationValue::CustomProperty(_)
        | WordSpacingDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_letter_spacing_declaration(
    declaration: InheritedTextDeclaration<LetterSpacingDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<LetterSpacingDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_letter_spacing_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(LetterSpacingDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => Some(LetterSpacingDeclarationValue::Value(0)),
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_letter_spacing_value(
    value: LetterSpacingDeclarationValue,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<LetterSpacingDeclarationValue> {
    match value {
        LetterSpacingDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_letter_spacing_declaration(value))
                .and_then(|declaration| {
                    resolve_letter_spacing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        LetterSpacingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_letter_spacing_declaration(value))
                .and_then(|declaration| {
                    resolve_letter_spacing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(LetterSpacingDeclarationValue::Value(fallback)))
        }
        LetterSpacingDeclarationValue::CustomProperty(_)
        | LetterSpacingDeclarationValue::CustomPropertyFallback(_, _) => None,
        value => Some(value),
    }
}

pub(super) fn resolve_letter_spacing(
    candidates: NativeTextCascadeCandidates<LetterSpacingDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    let resolved = resolve_alignment_candidates(
        candidates,
        LetterSpacingDeclarationValue::Value(inherited),
        |declaration| {
            resolve_letter_spacing_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        LetterSpacingDeclarationValue::Value(value) => value,
        LetterSpacingDeclarationValue::CustomProperty(_)
        | LetterSpacingDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_direction_declaration(
    declaration: InheritedTextDeclaration<DirectionDeclarationValue>,
    inherited: DirectionValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<DirectionDeclarationValue> {
    match declaration {
        InheritedTextDeclaration::Value(value) => {
            resolve_direction_value(value, inherited, custom_properties, depth)
        }
        InheritedTextDeclaration::Inherit
        | InheritedTextDeclaration::Unset
        | InheritedTextDeclaration::Revert => Some(DirectionDeclarationValue::Value(inherited)),
        InheritedTextDeclaration::Initial => {
            Some(DirectionDeclarationValue::Value(DirectionValue::Ltr))
        }
        InheritedTextDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_direction_value(
    value: DirectionDeclarationValue,
    inherited: DirectionValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<DirectionDeclarationValue> {
    match value {
        DirectionDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_direction_declaration(value))
                .and_then(|declaration| {
                    resolve_direction_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        DirectionDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_direction_declaration(value))
                .and_then(|declaration| {
                    resolve_direction_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(DirectionDeclarationValue::Value(fallback)))
        }
        DirectionDeclarationValue::CustomProperty(_)
        | DirectionDeclarationValue::CustomPropertyFallback(_, _) => None,
        DirectionDeclarationValue::Value(value) => Some(DirectionDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_direction(
    candidates: NativeTextCascadeCandidates<DirectionDeclarationValue>,
    inherited: DirectionValue,
    custom_properties: &BTreeMap<u64, String>,
) -> DirectionValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        DirectionDeclarationValue::Value(inherited),
        |declaration| resolve_direction_declaration(declaration, inherited, custom_properties, 0),
    );
    match resolved {
        DirectionDeclarationValue::Value(value) => value,
        DirectionDeclarationValue::CustomProperty(_)
        | DirectionDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

mod flex;
pub(super) use flex::*;

mod typography;
pub(super) use typography::*;

pub(super) fn resolve_visibility_value(
    value: VisibilityValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<VisibilityValue> {
    match value {
        VisibilityValue::Hidden | VisibilityValue::Other => Some(value),
        VisibilityValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_visibility_declaration(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => {
                        resolve_visibility_value(value, custom_properties, depth + 1)
                    }
                    LocalCascadeDeclaration::Inherit
                    | LocalCascadeDeclaration::Reset
                    | LocalCascadeDeclaration::RevertLayer => None,
                })
        }
        VisibilityValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_visibility_declaration(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => {
                        resolve_visibility_value(value, custom_properties, depth + 1)
                    }
                    LocalCascadeDeclaration::Inherit
                    | LocalCascadeDeclaration::Reset
                    | LocalCascadeDeclaration::RevertLayer => None,
                })
                .or(Some(if fallback {
                    VisibilityValue::Hidden
                } else {
                    VisibilityValue::Other
                }))
        }
        VisibilityValue::CustomProperty(_) | VisibilityValue::CustomPropertyFallback(_, _) => None,
    }
}

pub(super) fn resolve_visibility<const N: usize>(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<VisibilityValue>>>; N],
    custom_properties: &BTreeMap<u64, String>,
) -> VisibilityValue {
    resolve_alignment_candidates(
        candidates,
        VisibilityValue::Other,
        |declaration| match declaration {
            LocalCascadeDeclaration::Value(value) => {
                resolve_visibility_value(value, custom_properties, 0)
            }
            LocalCascadeDeclaration::Inherit
            | LocalCascadeDeclaration::Reset
            | LocalCascadeDeclaration::RevertLayer => None,
        },
    )
}

pub(super) fn resolve_grid_track_list_source(
    source: &str,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeGridTrackList> {
    if depth >= MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH {
        return None;
    }
    if let Some((name_hash, fallback)) = parse_flex_var_arguments(source) {
        return custom_properties
            .get(&name_hash)
            .and_then(|value| {
                resolve_grid_track_list_source(value, custom_properties, depth.saturating_add(1))
            })
            .or_else(|| fallback.and_then(parse_grid_track_list));
    }
    parse_grid_track_list(source)
}

pub(super) fn resolve_grid_track_list_value(
    value: GridTrackListDeclaration,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeGridTrackList> {
    match value {
        GridTrackListDeclaration::Value(value) => Some(value),
        GridTrackListDeclaration::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties.get(&name_hash).and_then(|value| {
                resolve_grid_track_list_source(value, custom_properties, depth.saturating_add(1))
            })
        }
        GridTrackListDeclaration::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| {
                    resolve_grid_track_list_source(
                        value,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(fallback))
        }
        GridTrackListDeclaration::CustomProperty(_)
        | GridTrackListDeclaration::CustomPropertyFallback(_, _) => None,
    }
}

pub(super) fn resolve_grid_track_list(
    candidates: NativeLocalCascadeCandidates<GridTrackListDeclaration>,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeGridTrackList {
    resolve_alignment_candidates(candidates, NativeGridTrackList::default(), |declaration| {
        match declaration {
            LocalCascadeDeclaration::Value(value) => {
                resolve_grid_track_list_value(value, custom_properties, 0)
            }
            LocalCascadeDeclaration::Inherit
            | LocalCascadeDeclaration::Reset
            | LocalCascadeDeclaration::RevertLayer => None,
        }
    })
}

pub(super) fn resolve_display_declaration(
    declaration: LocalCascadeDeclaration<DisplayDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<DisplayDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_display_value(value, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => {
            Some(DisplayDeclarationValue::Value(DisplayValue::Auto))
        }
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_display_value(
    value: DisplayDeclarationValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<DisplayDeclarationValue> {
    match value {
        DisplayDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_display_declaration(value))
                .and_then(|declaration| {
                    resolve_display_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        DisplayDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_display_declaration(value))
                .and_then(|declaration| {
                    resolve_display_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(DisplayDeclarationValue::Value(fallback)))
        }
        DisplayDeclarationValue::CustomProperty(_)
        | DisplayDeclarationValue::CustomPropertyFallback(_, _) => None,
        DisplayDeclarationValue::Value(value) => Some(DisplayDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_display(
    candidates: NativeLocalCascadeCandidates<DisplayDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
) -> DisplayValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        DisplayDeclarationValue::Value(DisplayValue::Auto),
        |declaration| resolve_display_declaration(declaration, custom_properties, 0),
    );
    match resolved {
        DisplayDeclarationValue::Value(value) => value,
        DisplayDeclarationValue::CustomProperty(_)
        | DisplayDeclarationValue::CustomPropertyFallback(_, _) => DisplayValue::Auto,
    }
}

pub(super) fn resolve_position_declaration(
    declaration: LocalCascadeDeclaration<PositionDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PositionDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_position_value(value, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => {
            Some(PositionDeclarationValue::Value(NativePositionValue::Static))
        }
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_position_value(
    value: PositionDeclarationValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PositionDeclarationValue> {
    match value {
        PositionDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_position_declaration(value))
                .and_then(|declaration| {
                    resolve_position_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        PositionDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_position_declaration(value))
                .and_then(|declaration| {
                    resolve_position_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(PositionDeclarationValue::Value(fallback)))
        }
        PositionDeclarationValue::CustomProperty(_)
        | PositionDeclarationValue::CustomPropertyFallback(_, _) => None,
        PositionDeclarationValue::Value(value) => Some(PositionDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_position(
    candidates: NativeLocalCascadeCandidates<PositionDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
) -> NativePositionValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        PositionDeclarationValue::Value(NativePositionValue::Static),
        |declaration| resolve_position_declaration(declaration, custom_properties, 0),
    );
    match resolved {
        PositionDeclarationValue::Value(value) => value,
        PositionDeclarationValue::CustomProperty(_)
        | PositionDeclarationValue::CustomPropertyFallback(_, _) => NativePositionValue::Static,
    }
}

pub(super) fn resolve_z_index_declaration(
    declaration: LocalCascadeDeclaration<ZIndexDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<ZIndexDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_z_index_value(value, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => {
            Some(ZIndexDeclarationValue::Value(NativeZIndexValue::Auto))
        }
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_z_index_value(
    value: ZIndexDeclarationValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<ZIndexDeclarationValue> {
    match value {
        ZIndexDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_z_index_declaration(value))
                .and_then(|declaration| {
                    resolve_z_index_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        ZIndexDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_z_index_declaration(value))
                .and_then(|declaration| {
                    resolve_z_index_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(ZIndexDeclarationValue::Value(fallback)))
        }
        ZIndexDeclarationValue::CustomProperty(_)
        | ZIndexDeclarationValue::CustomPropertyFallback(_, _) => None,
        ZIndexDeclarationValue::Value(value) => Some(ZIndexDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_z_index(
    candidates: NativeLocalCascadeCandidates<ZIndexDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeZIndexValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        ZIndexDeclarationValue::Value(NativeZIndexValue::Auto),
        |declaration| resolve_z_index_declaration(declaration, custom_properties, 0),
    );
    match resolved {
        ZIndexDeclarationValue::Value(value) => value,
        ZIndexDeclarationValue::CustomProperty(_)
        | ZIndexDeclarationValue::CustomPropertyFallback(_, _) => NativeZIndexValue::Auto,
    }
}

pub(super) fn resolve_position_offset_declaration(
    declaration: LocalCascadeDeclaration<PositionOffsetDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PositionOffsetDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_position_offset_value(value, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => Some(
            PositionOffsetDeclarationValue::Value(NativePositionOffset::Auto),
        ),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_position_offset_value(
    value: PositionOffsetDeclarationValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PositionOffsetDeclarationValue> {
    match value {
        PositionOffsetDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_position_offset_declaration(value))
                .and_then(|declaration| {
                    resolve_position_offset_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        PositionOffsetDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_position_offset_declaration(value))
                .and_then(|declaration| {
                    resolve_position_offset_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(PositionOffsetDeclarationValue::Value(fallback)))
        }
        PositionOffsetDeclarationValue::CustomProperty(_)
        | PositionOffsetDeclarationValue::CustomPropertyFallback(_, _) => None,
        PositionOffsetDeclarationValue::Value(value) => {
            Some(PositionOffsetDeclarationValue::Value(value))
        }
    }
}

pub(super) fn resolve_position_offset(
    candidates: NativeLocalCascadeCandidates<PositionOffsetDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
) -> NativePositionOffset {
    let resolved = resolve_alignment_candidates(
        candidates,
        PositionOffsetDeclarationValue::Value(NativePositionOffset::Auto),
        |declaration| resolve_position_offset_declaration(declaration, custom_properties, 0),
    );
    match resolved {
        PositionOffsetDeclarationValue::Value(value) => value,
        PositionOffsetDeclarationValue::CustomProperty(_)
        | PositionOffsetDeclarationValue::CustomPropertyFallback(_, _) => {
            NativePositionOffset::Auto
        }
    }
}

pub(super) fn resolve_dimension_declaration(
    declaration: LocalCascadeDeclaration<DimensionDeclarationValue>,
    inherited: Option<u32>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<Option<u32>> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_dimension_value(value, inherited, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(inherited),
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_dimension_value(
    value: DimensionDeclarationValue,
    inherited: Option<u32>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<Option<u32>> {
    match value {
        DimensionDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_dimension_declaration(value))
                .and_then(|declaration| {
                    resolve_dimension_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        DimensionDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_dimension_declaration(value))
                .and_then(|declaration| {
                    resolve_dimension_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(Some(fallback)))
        }
        DimensionDeclarationValue::CustomProperty(_)
        | DimensionDeclarationValue::CustomPropertyFallback(_, _) => None,
        DimensionDeclarationValue::Value(value) => Some(Some(value)),
    }
}

pub(super) fn resolve_dimension(
    candidates: NativeLocalCascadeCandidates<DimensionDeclarationValue>,
    inherited: Option<u32>,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<u32> {
    resolve_alignment_candidates(candidates, None, |declaration| {
        resolve_dimension_declaration(declaration, inherited, custom_properties, 0)
    })
}

pub(super) fn resolve_box_sizing_declaration(
    declaration: LocalCascadeDeclaration<BoxSizingDeclarationValue>,
    inherited: NativeBoxSizing,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<BoxSizingDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_box_sizing_value(value, inherited, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(BoxSizingDeclarationValue::Value(inherited)),
        LocalCascadeDeclaration::Reset => Some(BoxSizingDeclarationValue::Value(
            NativeBoxSizing::ContentBox,
        )),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_box_sizing_value(
    value: BoxSizingDeclarationValue,
    inherited: NativeBoxSizing,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<BoxSizingDeclarationValue> {
    match value {
        BoxSizingDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_box_sizing_declaration(value))
                .and_then(|declaration| {
                    resolve_box_sizing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        BoxSizingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_box_sizing_declaration(value))
                .and_then(|declaration| {
                    resolve_box_sizing_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(BoxSizingDeclarationValue::Value(fallback)))
        }
        BoxSizingDeclarationValue::CustomProperty(_)
        | BoxSizingDeclarationValue::CustomPropertyFallback(_, _) => None,
        BoxSizingDeclarationValue::Value(value) => Some(BoxSizingDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_box_sizing(
    candidates: NativeLocalCascadeCandidates<BoxSizingDeclarationValue>,
    inherited: NativeBoxSizing,
    custom_properties: &BTreeMap<u64, String>,
) -> NativeBoxSizing {
    let resolved = resolve_alignment_candidates(
        candidates,
        BoxSizingDeclarationValue::Value(NativeBoxSizing::ContentBox),
        |declaration| resolve_box_sizing_declaration(declaration, inherited, custom_properties, 0),
    );
    match resolved {
        BoxSizingDeclarationValue::Value(value) => value,
        BoxSizingDeclarationValue::CustomProperty(_)
        | BoxSizingDeclarationValue::CustomPropertyFallback(_, _) => NativeBoxSizing::ContentBox,
    }
}

pub(super) fn resolve_padding_declaration(
    declaration: LocalCascadeDeclaration<PaddingDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PaddingDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_padding_value(value, inherited, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(PaddingDeclarationValue::Value(inherited)),
        LocalCascadeDeclaration::Reset => Some(PaddingDeclarationValue::Value(0)),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_padding_value(
    value: PaddingDeclarationValue,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<PaddingDeclarationValue> {
    match value {
        PaddingDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_physical_padding_declaration(value))
                .and_then(|declaration| {
                    resolve_padding_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        PaddingDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_physical_padding_declaration(value))
                .and_then(|declaration| {
                    resolve_padding_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(PaddingDeclarationValue::Value(fallback)))
        }
        PaddingDeclarationValue::CustomPropertyShorthand(name_hash, edge)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_box_edges(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_padding_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        PaddingDeclarationValue::CustomPropertyShorthandFallback(name_hash, edge, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_box_edges(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_padding_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(PaddingDeclarationValue::Value(fallback)))
        }
        PaddingDeclarationValue::CustomProperty(_)
        | PaddingDeclarationValue::CustomPropertyFallback(_, _)
        | PaddingDeclarationValue::CustomPropertyShorthand(_, _)
        | PaddingDeclarationValue::CustomPropertyShorthandFallback(_, _, _) => None,
        PaddingDeclarationValue::Value(value) => Some(PaddingDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_padding_candidates(
    candidates: NativeLocalCascadeCandidates<PaddingDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<u32> {
    resolve_alignment_candidates(candidates, None, |declaration| {
        resolve_padding_declaration(declaration, inherited, custom_properties, 0).map(|value| {
            match value {
                PaddingDeclarationValue::Value(value) => Some(value),
                PaddingDeclarationValue::CustomProperty(_)
                | PaddingDeclarationValue::CustomPropertyFallback(_, _)
                | PaddingDeclarationValue::CustomPropertyShorthand(_, _)
                | PaddingDeclarationValue::CustomPropertyShorthandFallback(_, _, _) => None,
            }
        })
    })
}

pub(super) fn resolve_margin_declaration(
    declaration: LocalCascadeDeclaration<MarginDeclarationValue>,
    inherited: NativeMarginValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<MarginDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_margin_value(value, inherited, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(MarginDeclarationValue::Value(inherited)),
        LocalCascadeDeclaration::Reset => {
            Some(MarginDeclarationValue::Value(NativeMarginValue::Length(0)))
        }
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_margin_value(
    value: MarginDeclarationValue,
    inherited: NativeMarginValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<MarginDeclarationValue> {
    match value {
        MarginDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_physical_margin_declaration(value))
                .and_then(|declaration| {
                    resolve_margin_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        MarginDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_physical_margin_declaration(value))
                .and_then(|declaration| {
                    resolve_margin_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(MarginDeclarationValue::Value(fallback)))
        }
        MarginDeclarationValue::CustomPropertyShorthand(name_hash, edge)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_margin_edges(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_margin_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        MarginDeclarationValue::CustomPropertyShorthandFallback(name_hash, edge, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_margin_edges(value))
                .and_then(|declarations| declarations.get(edge as usize).copied())
                .and_then(|declaration| {
                    resolve_margin_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(MarginDeclarationValue::Value(fallback)))
        }
        MarginDeclarationValue::CustomProperty(_)
        | MarginDeclarationValue::CustomPropertyFallback(_, _)
        | MarginDeclarationValue::CustomPropertyShorthand(_, _)
        | MarginDeclarationValue::CustomPropertyShorthandFallback(_, _, _) => None,
        MarginDeclarationValue::Value(value) => Some(MarginDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_margin_candidates(
    candidates: NativeLocalCascadeCandidates<MarginDeclarationValue>,
    inherited: NativeMarginValue,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<NativeMarginValue> {
    resolve_alignment_candidates(candidates, None, |declaration| {
        resolve_margin_declaration(declaration, inherited, custom_properties, 0).map(|value| {
            match value {
                MarginDeclarationValue::Value(value) => Some(value),
                MarginDeclarationValue::CustomProperty(_)
                | MarginDeclarationValue::CustomPropertyFallback(_, _)
                | MarginDeclarationValue::CustomPropertyShorthand(_, _)
                | MarginDeclarationValue::CustomPropertyShorthandFallback(_, _, _) => None,
            }
        })
    })
}

pub(super) fn resolve_text_indent_declaration(
    declaration: LocalCascadeDeclaration<TextIndentDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextIndentDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_text_indent_value(value, inherited, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(TextIndentDeclarationValue::Value(inherited)),
        LocalCascadeDeclaration::Reset => Some(TextIndentDeclarationValue::Value(0)),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_text_indent_value(
    value: TextIndentDeclarationValue,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextIndentDeclarationValue> {
    match value {
        TextIndentDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_indent_declaration(value))
                .and_then(|declaration| {
                    resolve_text_indent_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        TextIndentDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_indent_declaration(value))
                .and_then(|declaration| {
                    resolve_text_indent_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(TextIndentDeclarationValue::Value(fallback)))
        }
        TextIndentDeclarationValue::CustomProperty(_)
        | TextIndentDeclarationValue::CustomPropertyFallback(_, _) => None,
        TextIndentDeclarationValue::Value(value) => Some(TextIndentDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_text_indent(
    candidates: NativeTextLocalCascadeCandidates<TextIndentDeclarationValue>,
    inherited: u32,
    custom_properties: &BTreeMap<u64, String>,
) -> u32 {
    let resolved = resolve_alignment_candidates(
        candidates,
        TextIndentDeclarationValue::Value(0),
        |declaration| resolve_text_indent_declaration(declaration, inherited, custom_properties, 0),
    );
    match resolved {
        TextIndentDeclarationValue::Value(value) => value,
        TextIndentDeclarationValue::CustomProperty(_)
        | TextIndentDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_text_overflow_declaration(
    declaration: LocalCascadeDeclaration<TextOverflowDeclarationValue>,
    inherited: TextOverflowValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextOverflowDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_text_overflow_value(value, inherited, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(TextOverflowDeclarationValue::Value(inherited)),
        LocalCascadeDeclaration::Reset => {
            Some(TextOverflowDeclarationValue::Value(TextOverflowValue::Clip))
        }
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_text_overflow_value(
    value: TextOverflowDeclarationValue,
    inherited: TextOverflowValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<TextOverflowDeclarationValue> {
    match value {
        TextOverflowDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_overflow_declaration(value))
                .and_then(|declaration| {
                    resolve_text_overflow_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        TextOverflowDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_text_overflow_declaration(value))
                .and_then(|declaration| {
                    resolve_text_overflow_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(TextOverflowDeclarationValue::Value(fallback)))
        }
        TextOverflowDeclarationValue::CustomProperty(_)
        | TextOverflowDeclarationValue::CustomPropertyFallback(_, _) => None,
        TextOverflowDeclarationValue::Value(value) => {
            Some(TextOverflowDeclarationValue::Value(value))
        }
    }
}

pub(super) fn resolve_text_overflow(
    candidates: NativeTextLocalCascadeCandidates<TextOverflowDeclarationValue>,
    inherited: TextOverflowValue,
    custom_properties: &BTreeMap<u64, String>,
) -> TextOverflowValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        TextOverflowDeclarationValue::Value(TextOverflowValue::Clip),
        |declaration| {
            resolve_text_overflow_declaration(declaration, inherited, custom_properties, 0)
        },
    );
    match resolved {
        TextOverflowDeclarationValue::Value(value) => value,
        TextOverflowDeclarationValue::CustomProperty(_)
        | TextOverflowDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_overflow_declaration(
    declaration: LocalCascadeDeclaration<OverflowDeclarationValue>,
    inherited: OverflowValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<OverflowDeclarationValue> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_overflow_value(value, inherited, custom_properties, depth)
        }
        LocalCascadeDeclaration::Inherit => Some(OverflowDeclarationValue::Value(inherited)),
        LocalCascadeDeclaration::Reset => {
            Some(OverflowDeclarationValue::Value(OverflowValue::Other))
        }
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_overflow_value(
    value: OverflowDeclarationValue,
    inherited: OverflowValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<OverflowDeclarationValue> {
    match value {
        OverflowDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_overflow_declaration(value))
                .and_then(|declaration| {
                    resolve_overflow_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
        }
        OverflowDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_overflow_declaration(value))
                .and_then(|declaration| {
                    resolve_overflow_declaration(
                        declaration,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .or(Some(OverflowDeclarationValue::Value(fallback)))
        }
        OverflowDeclarationValue::CustomProperty(_)
        | OverflowDeclarationValue::CustomPropertyFallback(_, _) => None,
        OverflowDeclarationValue::Value(value) => Some(OverflowDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_overflow_axis(
    candidates: NativeLocalCascadeCandidates<OverflowDeclarationValue>,
    inherited: OverflowValue,
    custom_properties: &BTreeMap<u64, String>,
) -> OverflowValue {
    let resolved = resolve_alignment_candidates(
        candidates,
        OverflowDeclarationValue::Value(OverflowValue::Other),
        |declaration| resolve_overflow_declaration(declaration, inherited, custom_properties, 0),
    );
    match resolved {
        OverflowDeclarationValue::Value(value) => value,
        OverflowDeclarationValue::CustomProperty(_)
        | OverflowDeclarationValue::CustomPropertyFallback(_, _) => inherited,
    }
}

pub(super) fn resolve_opacity_declaration(
    declaration: LocalCascadeDeclaration<OpacityDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<Option<OpacityDeclarationValue>> {
    match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_opacity_value(value, custom_properties, depth).map(Some)
        }
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    }
}

pub(super) fn resolve_opacity_value(
    value: OpacityDeclarationValue,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<OpacityDeclarationValue> {
    match value {
        OpacityDeclarationValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_opacity_declaration(value))
                .and_then(|declaration| {
                    resolve_opacity_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .flatten()
        }
        OpacityDeclarationValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_opacity_declaration(value))
                .and_then(|declaration| {
                    resolve_opacity_declaration(
                        declaration,
                        custom_properties,
                        depth.saturating_add(1),
                    )
                })
                .flatten()
                .or(Some(OpacityDeclarationValue::Value(fallback)))
        }
        OpacityDeclarationValue::CustomProperty(_)
        | OpacityDeclarationValue::CustomPropertyFallback(_, _) => None,
        OpacityDeclarationValue::Value(value) => Some(OpacityDeclarationValue::Value(value)),
    }
}

pub(super) fn resolve_opacity(
    candidates: NativeLocalCascadeCandidates<OpacityDeclarationValue>,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<u8> {
    let resolved = resolve_alignment_candidates(candidates, None, |declaration| {
        resolve_opacity_declaration(declaration, custom_properties, 0)
    });
    match resolved {
        Some(OpacityDeclarationValue::Value(value)) => Some(value),
        Some(OpacityDeclarationValue::CustomProperty(_))
        | Some(OpacityDeclarationValue::CustomPropertyFallback(_, _))
        | None => None,
    }
}

pub(super) fn resolve_native_color_value(
    value: NativeColorValue,
    inherited: Option<NativeColor>,
    custom_properties: &BTreeMap<u64, String>,
    depth: usize,
) -> Option<NativeColor> {
    match value {
        NativeColorValue::CustomProperty(name_hash)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_color_declaration(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_color_value(
                        value,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
        }
        NativeColorValue::CustomPropertyFallback(name_hash, fallback)
            if depth < MAX_NATIVE_CUSTOM_PROPERTY_RESOLUTION_DEPTH =>
        {
            custom_properties
                .get(&name_hash)
                .and_then(|value| parse_local_color_declaration(value))
                .and_then(|declaration| match declaration {
                    LocalCascadeDeclaration::Value(value) => resolve_native_color_value(
                        value,
                        inherited,
                        custom_properties,
                        depth.saturating_add(1),
                    ),
                    LocalCascadeDeclaration::RevertLayer => None,
                    LocalCascadeDeclaration::Inherit | LocalCascadeDeclaration::Reset => None,
                })
                .or(Some(fallback))
        }
        NativeColorValue::CustomProperty(_) | NativeColorValue::CustomPropertyFallback(_, _) => {
            None
        }
        value => Some(value.resolve(inherited)),
    }
}

pub(super) fn resolve_local_color_declaration(
    candidates: [Option<CascadeValue<LocalCascadeDeclaration<NativeColorValue>>>;
        MAX_NATIVE_PAINT_CASCADE_LAYERS],
    inherited: Option<NativeColor>,
    custom_properties: &BTreeMap<u64, String>,
) -> Option<NativeColor> {
    resolve_alignment_candidates(candidates, inherited, |declaration| match declaration {
        LocalCascadeDeclaration::Value(value) => {
            resolve_native_color_value(value, inherited, custom_properties, 0).map(Some)
        }
        LocalCascadeDeclaration::Inherit => None,
        LocalCascadeDeclaration::Reset => Some(None),
        LocalCascadeDeclaration::RevertLayer => None,
    })
}

mod background;
pub(super) use background::*;

mod border;
pub(super) use border::*;

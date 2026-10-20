//! System font discovery, metric scaling, and bounded raster helpers.

use super::*;

pub(super) fn has_glyph(face: &NativeFontFace, character: char) -> bool {
    (face.unicode_ranges.is_empty()
        || face
            .unicode_ranges
            .iter()
            .copied()
            .any(|range| range.contains(character)))
        && face.font.lookup_glyph_index(character) != 0
}

pub(super) fn system_font_book() -> &'static NativeFontBook {
    static BOOK: OnceLock<NativeFontBook> = OnceLock::new();
    BOOK.get_or_init(load_system_font_book)
}

pub(super) fn load_system_font_book() -> NativeFontBook {
    let mut book = NativeFontBook::default();
    for &(path, family, generic_family, weight, style) in system_font_candidates() {
        let path = Path::new(path);
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        insert_font_face(
            &mut book,
            family.to_owned(),
            Some(generic_family),
            weight,
            style,
            bytes,
            0,
        );
    }

    let mut discovered_bytes = 0usize;
    for path in discover_system_font_paths() {
        if book.faces.len() >= MAX_NATIVE_SYSTEM_FONT_FACES {
            break;
        }
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let Ok(file_len) = usize::try_from(metadata.len()) else {
            continue;
        };
        if file_len == 0 || file_len > MAX_NATIVE_FONT_BYTES {
            continue;
        }
        if discovered_bytes.saturating_add(file_len) > MAX_NATIVE_SYSTEM_FONT_BYTES {
            break;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        discovered_bytes = discovered_bytes.saturating_add(bytes.len());
        let collection_faces = ttf_parser::fonts_in_collection(&bytes)
            .unwrap_or(1)
            .min(MAX_NATIVE_SYSTEM_COLLECTION_FACES);
        for index in 0..collection_faces {
            if book.faces.len() >= MAX_NATIVE_SYSTEM_FONT_FACES {
                break;
            }
            let Some((family, generic_family, weight, style)) =
                discovered_font_metadata(&bytes, index)
            else {
                continue;
            };
            if book.faces.iter().any(|face| {
                face.family.eq_ignore_ascii_case(&family)
                    && face.weight == NativeFontWeightRange::singleton(weight)
                    && face.style == style
            }) {
                continue;
            }
            insert_font_face(
                &mut book,
                family,
                generic_family,
                weight,
                style,
                bytes.clone(),
                index,
            );
        }
    }
    book
}

pub(super) fn insert_font_face(
    book: &mut NativeFontBook,
    family: String,
    generic_family: Option<NativeGenericFontFamily>,
    weight: FontWeightValue,
    style: FontStyleValue,
    bytes: Vec<u8>,
    collection_index: u32,
) -> bool {
    if book.faces.len() >= MAX_NATIVE_SYSTEM_FONT_FACES
        || bytes.is_empty()
        || bytes.len() > MAX_NATIVE_FONT_BYTES
    {
        return false;
    }
    let Ok(font) = fontdue::Font::from_bytes(
        bytes.clone(),
        fontdue::FontSettings {
            collection_index,
            scale: FONT_PARSE_SCALE,
            load_substitutions: true,
        },
    ) else {
        return false;
    };
    let font_data: Arc<[u8]> = Arc::from(bytes);
    if book.faces.iter().any(|face| {
        face.family.eq_ignore_ascii_case(&family)
            && face.weight == NativeFontWeightRange::singleton(weight)
            && face.style == style
            && face.font_data == font_data
    }) {
        return false;
    }
    let shaper_data = harfrust::FontRef::new(font_data.as_ref())
        .ok()
        .map(|font| Arc::new(harfrust::ShaperData::new(&font)));
    book.faces.push(NativeFontFace {
        family: family.clone(),
        family_key: font_family_hash(&family),
        generic_family,
        weight: NativeFontWeightRange::singleton(weight),
        style,
        stretch: NativeFontStretchRange::default(),
        variation_settings: NativeFontVariationSettings::default(),
        feature_settings: NativeFontFeatureSettings::default(),
        size_adjust: super::super::css::DEFAULT_NATIVE_FONT_SIZE_ADJUST,
        metric_overrides: NativeFontMetricOverrides::default(),
        font_display: NativeFontDisplay::Auto,
        font: Arc::new(font),
        font_data,
        collection_index,
        shaper_data,
        unicode_ranges: Arc::from(Vec::<NativeUnicodeRange>::new()),
    });
    true
}

pub(super) fn discovered_font_metadata(
    bytes: &[u8],
    collection_index: u32,
) -> Option<(
    String,
    Option<NativeGenericFontFamily>,
    FontWeightValue,
    FontStyleValue,
)> {
    let face = ttf_parser::Face::parse(bytes, collection_index).ok()?;
    let mut family_name: Option<(u8, String)> = None;
    for name in face.names() {
        if name.name_id != ttf_parser::name_id::FAMILY
            && name.name_id != ttf_parser::name_id::TYPOGRAPHIC_FAMILY
        {
            continue;
        }
        let Some(value) = name.to_string() else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let preferred = u8::from(name.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY);
        let english = u8::from(name.language().primary_language() == "English");
        let score = preferred.saturating_mul(2).saturating_add(english);
        if family_name
            .as_ref()
            .is_none_or(|(current_score, _)| score > *current_score)
        {
            family_name = Some((score, value.to_owned()));
        }
    }
    let family = family_name?.1;
    let generic_family = discovered_generic_family(&family);
    let weight = if face.weight().to_number() >= 600 {
        FontWeightValue::Bold
    } else {
        FontWeightValue::Normal
    };
    let style = if face.is_italic() || face.is_oblique() {
        FontStyleValue::Italic
    } else {
        FontStyleValue::Normal
    };
    Some((family, generic_family, weight, style))
}

pub(super) fn discovered_generic_family(family: &str) -> Option<NativeGenericFontFamily> {
    let family = family.to_ascii_lowercase();
    if family.contains("mono") || family.contains("courier") {
        Some(NativeGenericFontFamily::Monospace)
    } else if family.contains("serif") || family.contains("times") {
        Some(NativeGenericFontFamily::Serif)
    } else if family.contains("sans") || family.contains("arial") {
        Some(NativeGenericFontFamily::SansSerif)
    } else {
        None
    }
}

pub(super) fn system_font_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut add = |path: PathBuf| {
        if !roots.iter().any(|existing| existing == &path) {
            roots.push(path);
        }
    };
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            add(home.join(".fonts"));
            if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
                add(PathBuf::from(data_home).join("fonts"));
            } else {
                add(home.join(".local/share/fonts"));
            }
        }
        add(PathBuf::from("/usr/local/share/fonts"));
        add(PathBuf::from("/usr/share/fonts"));
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            add(home.join("Library/Fonts"));
        }
        add(PathBuf::from("/Library/Fonts"));
        add(PathBuf::from("/System/Library/Fonts"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            add(PathBuf::from(local_app_data).join("Microsoft/Windows/Fonts"));
        }
        add(PathBuf::from(r"C:\Windows\Fonts"));
    }
    roots
}

pub(super) fn discover_system_font_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in system_font_roots() {
        collect_system_font_paths(&root, &mut paths);
    }
    paths.sort_unstable();
    paths.dedup();
    paths.truncate(MAX_NATIVE_SYSTEM_FONT_FILES);
    paths
}

pub(super) fn collect_system_font_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    if paths.len() >= MAX_NATIVE_SYSTEM_FONT_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_unstable_by_key(|entry| entry.path());
    for entry in entries {
        if paths.len() >= MAX_NATIVE_SYSTEM_FONT_FILES {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_system_font_paths(&path, paths);
        } else if file_type.is_file() && is_discoverable_font_path(&path) {
            paths.push(path);
        }
    }
}

pub(super) fn is_discoverable_font_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        })
}

pub(super) fn family_matches(family: NativeFontFamilyValue, face: &NativeFontFace) -> bool {
    match family {
        NativeFontFamilyValue::Named(key) => key == face.family_key,
        NativeFontFamilyValue::Generic(generic) => face.generic_family == Some(generic),
        NativeFontFamilyValue::Fallback => false,
    }
}

pub(super) fn face_score(
    face: &NativeFontFace,
    weight: FontWeightValue,
    style: FontStyleValue,
    stretch: u16,
) -> u16 {
    let weight_score = face.weight.distance(weight.numeric()).saturating_mul(8);
    let style_score = u16::from(face.style != style);
    weight_score
        .saturating_add(style_score.saturating_mul(2000))
        .saturating_add(face.stretch.distance(stretch))
}

pub(super) fn ceil_positive(value: f32) -> u32 {
    if value.is_finite() && value > 0.0 {
        value.ceil().min(u32::MAX as f32) as u32
    } else {
        0
    }
}

pub(super) fn scale_fixed(value: i64, requested: u16, nominal: u16) -> i64 {
    if nominal == 0 {
        return value;
    }
    value
        .saturating_mul(i64::from(requested))
        .checked_div(i64::from(nominal))
        .unwrap_or_else(|| {
            if value.is_negative() {
                i64::MIN
            } else {
                i64::MAX
            }
        })
}

pub(super) fn scale_dimension(value: u32, requested: u16, nominal: u16) -> u32 {
    if value == 0 || nominal == 0 {
        return value;
    }
    let scaled = u64::from(value)
        .saturating_mul(u64::from(requested))
        .saturating_add(u64::from(nominal / 2))
        / u64::from(nominal);
    u32::try_from(scaled.max(1)).unwrap_or(u32::MAX)
}

pub(super) fn scale_signed(value: i32, requested: u16, nominal: u16) -> i32 {
    i32::try_from(scale_fixed(i64::from(value), requested, nominal)).unwrap_or(
        if value.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        },
    )
}

pub(super) fn ceil_stretched(value: f32, requested: u16, nominal: u16) -> u32 {
    if nominal == 0 {
        return ceil_positive(value);
    }
    ceil_positive(value * (f32::from(requested) / f32::from(nominal)))
}

pub(super) fn round_stretched(value: f32, requested: u16, nominal: u16) -> i32 {
    if nominal == 0 {
        return round_signed(value);
    }
    round_signed(value * (f32::from(requested) / f32::from(nominal)))
}

pub(super) fn scale_raster_bytes(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    bytes_per_pixel: usize,
    destination_width: u32,
    destination_height: u32,
) -> Option<Vec<u8>> {
    if source_width == 0
        || source_height == 0
        || destination_width == 0
        || destination_height == 0
        || bytes_per_pixel == 0
    {
        return None;
    }
    let source_width = usize::try_from(source_width).ok()?;
    let source_height = usize::try_from(source_height).ok()?;
    let destination_width = usize::try_from(destination_width).ok()?;
    let destination_height = usize::try_from(destination_height).ok()?;
    if source_width > MAX_COLOR_BITMAP_DIMENSION
        || source_height > MAX_COLOR_BITMAP_DIMENSION
        || destination_width > MAX_COLOR_BITMAP_DIMENSION
        || destination_height > MAX_COLOR_BITMAP_DIMENSION
    {
        return None;
    }
    let source_len = source_width
        .checked_mul(source_height)?
        .checked_mul(bytes_per_pixel)?;
    if source.len() != source_len || source_len > MAX_COLOR_BITMAP_BYTES {
        return None;
    }
    let destination_len = destination_width
        .checked_mul(destination_height)?
        .checked_mul(bytes_per_pixel)?;
    if destination_len > MAX_COLOR_BITMAP_BYTES {
        return None;
    }
    let mut destination = vec![0_u8; destination_len];
    for row in 0..destination_height {
        let source_row = (row
            .saturating_mul(2)
            .saturating_add(1)
            .saturating_mul(source_height)
            / destination_height.saturating_mul(2).max(1))
        .min(source_height - 1);
        for column in 0..destination_width {
            let source_column = (column
                .saturating_mul(2)
                .saturating_add(1)
                .saturating_mul(source_width)
                / destination_width.saturating_mul(2).max(1))
            .min(source_width - 1);
            let source_start = source_row
                .checked_mul(source_width)?
                .checked_add(source_column)?
                .checked_mul(bytes_per_pixel)?;
            let destination_start = row
                .checked_mul(destination_width)?
                .checked_add(column)?
                .checked_mul(bytes_per_pixel)?;
            let source_pixel =
                source.get(source_start..source_start.checked_add(bytes_per_pixel)?)?;
            destination
                .get_mut(destination_start..destination_start.checked_add(bytes_per_pixel)?)?
                .copy_from_slice(source_pixel);
        }
    }
    Some(destination)
}

pub(super) fn scale_bitmap_horizontal(
    pixels: &[u8],
    width: u32,
    height: u32,
    requested: u16,
    nominal: u16,
) -> Option<Vec<u8>> {
    scale_raster_bytes(
        pixels,
        width,
        height,
        4,
        scale_dimension(width, requested, nominal),
        height,
    )
}

pub(super) fn scale_coverage_horizontal(
    coverage: Vec<u8>,
    width: u32,
    height: u32,
    requested: u16,
    nominal: u16,
) -> Option<(u32, Arc<[u8]>)> {
    scale_coverage_horizontal_with_limit(coverage, width, height, requested, nominal, None)
}

pub(super) fn scale_coverage_horizontal_bounded(
    coverage: Vec<u8>,
    width: u32,
    height: u32,
    requested: u16,
    nominal: u16,
) -> Option<(u32, Arc<[u8]>)> {
    scale_coverage_horizontal_with_limit(
        coverage,
        width,
        height,
        requested,
        nominal,
        Some(MAX_COLOR_BITMAP_DIMENSION),
    )
}

pub(super) fn scale_coverage_horizontal_with_limit(
    coverage: Vec<u8>,
    width: u32,
    height: u32,
    requested: u16,
    nominal: u16,
    max_dimension: Option<usize>,
) -> Option<(u32, Arc<[u8]>)> {
    let source_width = usize::try_from(width).ok()?;
    let source_height = usize::try_from(height).ok()?;
    if source_width == 0
        || source_height == 0
        || max_dimension.is_some_and(|max| source_width > max || source_height > max)
    {
        return None;
    }
    let source_len = source_width.checked_mul(source_height)?;
    if source_len > MAX_COLOR_BITMAP_BYTES || coverage.len() != source_len {
        return None;
    }
    let scaled_width = scale_dimension(width, requested, nominal);
    let destination_width = usize::try_from(scaled_width).ok()?;
    if scaled_width == 0 || max_dimension.is_some_and(|max| destination_width > max) {
        return None;
    }
    if scaled_width == width {
        return Some((width, Arc::from(coverage)));
    }
    let destination_len = destination_width.checked_mul(source_height)?;
    if destination_len > MAX_COLOR_BITMAP_BYTES {
        return None;
    }
    let mut scaled = vec![0_u8; destination_len];
    for row in 0..source_height {
        let source_row = row.checked_mul(source_width)?;
        let destination_row = row.checked_mul(destination_width)?;
        for column in 0..destination_width {
            let numerator =
                (column.saturating_mul(2).saturating_add(1)).saturating_mul(source_width);
            let denominator = destination_width.saturating_mul(2).max(1);
            let source_column = (numerator / denominator).min(source_width - 1);
            scaled[destination_row + column] = coverage[source_row + source_column];
        }
    }
    Some((scaled_width, Arc::from(scaled)))
}

pub(super) fn round_signed(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

pub(super) fn round_signed_fixed(value: i64) -> i32 {
    let rounded = if value >= 0 {
        value.saturating_add(i64::from(FONT_SHAPE_SCALE / 2)) / i64::from(FONT_SHAPE_SCALE)
    } else {
        value.saturating_sub(i64::from(FONT_SHAPE_SCALE / 2)) / i64::from(FONT_SHAPE_SCALE)
    };
    i32::try_from(rounded).unwrap_or(if rounded.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    })
}

pub(super) fn round_fixed_nonnegative(value: i64) -> u32 {
    if value <= 0 {
        return 0;
    }
    let rounded =
        value.saturating_add(i64::from(FONT_SHAPE_SCALE / 2)) / i64::from(FONT_SHAPE_SCALE);
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

pub(super) fn cluster_index(value: &str, cluster: u32) -> Option<usize> {
    let byte = usize::try_from(cluster).ok()?;
    (byte <= value.len() && value.is_char_boundary(byte)).then(|| value[..byte].chars().count())
}

pub(super) fn spacing_for_characters(
    characters: &[char],
    range: std::ops::Range<usize>,
    letter_spacing: u32,
    word_spacing: u32,
) -> i64 {
    characters[range].iter().fold(0i64, |spacing, character| {
        let extra = i64::from(letter_spacing).saturating_add(if *character == ' ' {
            i64::from(word_spacing)
        } else {
            0
        });
        spacing.saturating_add(extra.saturating_mul(i64::from(FONT_SHAPE_SCALE)))
    })
}

type FontCandidate = (
    &'static str,
    &'static str,
    NativeGenericFontFamily,
    FontWeightValue,
    FontStyleValue,
);

#[cfg(target_os = "linux")]
pub(super) fn system_font_candidates() -> &'static [FontCandidate] {
    use FontStyleValue::{Italic, Normal};
    use FontWeightValue::{Bold, Normal as Regular};
    use NativeGenericFontFamily::{Monospace, SansSerif, Serif};
    &[
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "DejaVu Sans",
            SansSerif,
            Regular,
            Normal,
        ),
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "DejaVu Sans",
            SansSerif,
            Bold,
            Normal,
        ),
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
            "DejaVu Sans",
            SansSerif,
            Regular,
            Italic,
        ),
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf",
            "DejaVu Sans",
            SansSerif,
            Bold,
            Italic,
        ),
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
            "DejaVu Serif",
            Serif,
            Regular,
            Normal,
        ),
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
            "DejaVu Serif",
            Serif,
            Bold,
            Normal,
        ),
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "DejaVu Sans Mono",
            Monospace,
            Regular,
            Normal,
        ),
        (
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
            "DejaVu Sans Mono",
            Monospace,
            Bold,
            Normal,
        ),
    ]
}

#[cfg(target_os = "macos")]
pub(super) fn system_font_candidates() -> &'static [FontCandidate] {
    use FontStyleValue::{Italic, Normal};
    use FontWeightValue::{Bold, Normal as Regular};
    use NativeGenericFontFamily::{Monospace, SansSerif, Serif};
    &[
        (
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "Arial",
            SansSerif,
            Regular,
            Normal,
        ),
        (
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            "Arial",
            SansSerif,
            Bold,
            Normal,
        ),
        (
            "/System/Library/Fonts/Supplemental/Arial Italic.ttf",
            "Arial",
            SansSerif,
            Regular,
            Italic,
        ),
        (
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "Times New Roman",
            Serif,
            Regular,
            Normal,
        ),
        (
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
            "Courier New",
            Monospace,
            Regular,
            Normal,
        ),
    ]
}

#[cfg(target_os = "windows")]
pub(super) fn system_font_candidates() -> &'static [FontCandidate] {
    use FontStyleValue::{Italic, Normal};
    use FontWeightValue::{Bold, Normal as Regular};
    use NativeGenericFontFamily::{Monospace, SansSerif, Serif};
    &[
        (
            r"C:\Windows\Fonts\arial.ttf",
            "Arial",
            SansSerif,
            Regular,
            Normal,
        ),
        (
            r"C:\Windows\Fonts\arialbd.ttf",
            "Arial",
            SansSerif,
            Bold,
            Normal,
        ),
        (
            r"C:\Windows\Fonts\ariali.ttf",
            "Arial",
            SansSerif,
            Regular,
            Italic,
        ),
        (
            r"C:\Windows\Fonts\times.ttf",
            "Times New Roman",
            Serif,
            Regular,
            Normal,
        ),
        (
            r"C:\Windows\Fonts\cour.ttf",
            "Courier New",
            Monospace,
            Regular,
            Normal,
        ),
    ]
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(super) fn system_font_candidates() -> &'static [FontCandidate] {
    &[]
}

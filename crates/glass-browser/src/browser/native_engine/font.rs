use super::css::{
    DirectionValue, FontStyleValue, FontWeightValue, NativeFontFaceRule, NativeFontFamilyList,
    NativeFontFamilyValue, NativeGenericFontFamily, font_family_hash,
};
use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// The CSS initial font size used by the bounded native text fallback.
pub(crate) const DEFAULT_NATIVE_FONT_SIZE: u32 = 16;
/// Prevent a stylesheet from requesting an unbounded rasterization scale.
pub(crate) const MAX_NATIVE_FONT_SIZE: u32 = 256;
/// Limit the number of document-owned font faces crossing the native process
/// boundary in one document snapshot.
pub(crate) const MAX_NATIVE_FONT_FACES: usize = 16;
/// Bound one document-owned font payload before it reaches a font parser.
pub(crate) const MAX_NATIVE_FONT_BYTES: usize = 4 * 1024 * 1024;
/// Bound the aggregate document-owned font payload in one snapshot.
pub(crate) const MAX_NATIVE_FONT_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const FONT_PARSE_SCALE: f32 = 40.0;
const FONT_SHAPE_SCALE: u32 = 64;
const FALLBACK_GLYPH_ADVANCE: u32 = 8;
const FALLBACK_LINE_HEIGHT: u32 = 20;

/// One coverage bitmap and placement produced by the selected font face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeGlyph {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub advance: u32,
    pub coverage: Arc<[u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeSpaceRange {
    pub(crate) start: i32,
    pub(crate) end: i32,
    pub(crate) char_index: usize,
}

/// A shaped font run consumed by the native display list.
///
/// Explicit system faces use HarfRust's horizontal LTR or RTL glyph and
/// cluster positions when the font can be parsed by both shaping and
/// rasterization. The bounded character-by-character fontdue path remains
/// available for fonts that the shaper rejects; mixed bidi and writing-mode
/// selection are still outside this run's contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFontRun {
    pub glyphs: Vec<NativeGlyph>,
    pub(crate) space_ranges: Vec<NativeSpaceRange>,
    pub width: u32,
    pub ascent: u32,
    pub line_height: u32,
}

#[derive(Clone)]
struct NativeFontFace {
    family: String,
    family_key: u64,
    generic_family: Option<NativeGenericFontFamily>,
    weight: FontWeightValue,
    style: FontStyleValue,
    font: Arc<fontdue::Font>,
    font_data: Arc<[u8]>,
    shaper_data: Option<Arc<harfrust::ShaperData>>,
}

impl PartialEq for NativeFontFace {
    fn eq(&self, other: &Self) -> bool {
        self.family == other.family
            && self.family_key == other.family_key
            && self.generic_family == other.generic_family
            && self.weight == other.weight
            && self.style == other.style
            && self.font_data == other.font_data
    }
}

impl Eq for NativeFontFace {}

impl fmt::Debug for NativeFontFace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeFontFace")
            .field("family", &self.family)
            .field("weight", &self.weight)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NativeFontBook {
    faces: Vec<NativeFontFace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeFontFaceResource {
    pub(crate) family: String,
    pub(crate) family_key: u64,
    pub(crate) weight: FontWeightValue,
    pub(crate) style: FontStyleValue,
    pub(crate) bytes: Arc<[u8]>,
}

impl NativeFontFaceResource {
    pub(crate) fn from_rule(rule: &NativeFontFaceRule, bytes: Vec<u8>) -> Self {
        Self {
            family: rule.family.clone(),
            family_key: rule.family_key,
            weight: rule.weight,
            style: rule.style,
            bytes: Arc::from(bytes),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeTextMetrics {
    faces: Vec<Arc<NativeFontFace>>,
    font_size: u32,
    ascent: u32,
    line_height: u32,
    direction: DirectionValue,
}

impl NativeTextMetrics {
    pub(crate) fn fallback_with_direction(font_size: u32, direction: DirectionValue) -> Self {
        Self {
            faces: Vec::new(),
            font_size: font_size.clamp(1, MAX_NATIVE_FONT_SIZE),
            ascent: FALLBACK_LINE_HEIGHT.saturating_sub(5),
            line_height: FALLBACK_LINE_HEIGHT,
            direction,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_style(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        direction: DirectionValue,
    ) -> Self {
        Self::for_style_with_book(
            families,
            font_size,
            weight,
            style,
            direction,
            system_font_book(),
        )
    }

    pub(crate) fn for_style_with_book(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        let font_size = font_size.clamp(1, MAX_NATIVE_FONT_SIZE);
        let faces = book.faces_for(families, weight, style);
        let Some(face) = faces.first() else {
            return Self::fallback_with_direction(font_size, direction);
        };
        let line_metrics = face.font.horizontal_line_metrics(font_size as f32);
        let ascent = line_metrics
            .map(|metrics| ceil_positive(metrics.ascent))
            .unwrap_or(FALLBACK_LINE_HEIGHT.saturating_sub(5));
        let line_height = line_metrics
            .map(|metrics| ceil_positive(metrics.new_line_size))
            .unwrap_or(FALLBACK_LINE_HEIGHT)
            .max(1);
        Self {
            faces,
            font_size,
            ascent,
            line_height,
            direction,
        }
    }

    pub(crate) const fn line_height(&self) -> u32 {
        self.line_height
    }

    pub(crate) fn advance(&self, character: char, letter_spacing: u32, word_spacing: u32) -> u32 {
        let base = self
            .face_index_for_character(character)
            .and_then(|face_index| self.faces.get(face_index))
            .map_or(FALLBACK_GLYPH_ADVANCE, |face| {
                ceil_positive(
                    face.font
                        .metrics(character, self.font_size as f32)
                        .advance_width,
                )
            });
        base.saturating_add(letter_spacing)
            .saturating_add(if character == ' ' { word_spacing } else { 0 })
    }

    fn face_index_for_character(&self, character: char) -> Option<usize> {
        self.faces
            .iter()
            .position(|face| has_glyph(face, character))
    }

    pub(crate) fn measure_text(&self, value: &str, letter_spacing: u32, word_spacing: u32) -> u32 {
        if let Some(shaped) = self.shape(value, letter_spacing, word_spacing) {
            return shaped.width;
        }
        let mut width = 0u32;
        let mut previous = None;
        let mut previous_face_index = None;
        for character in value.chars() {
            let face_index = self.face_index_for_character(character);
            if let Some(face_index) = face_index
                && previous_face_index == Some(face_index)
                && let Some(face) = self.faces.get(face_index)
                && let Some(previous) = previous
            {
                let kerning = face
                    .font
                    .horizontal_kern(previous, character, self.font_size as f32)
                    .map_or(0, round_signed);
                if kerning.is_negative() {
                    width = width.saturating_sub(kerning.unsigned_abs());
                } else {
                    width = width.saturating_add(kerning as u32);
                }
            }
            width = width.saturating_add(self.advance(character, letter_spacing, word_spacing));
            previous = Some(character);
            previous_face_index = face_index;
        }
        width
    }

    pub(crate) fn rasterize(
        &self,
        value: &str,
        letter_spacing: u32,
        word_spacing: u32,
        justify_spacing: u32,
    ) -> Option<NativeFontRun> {
        if let Some(shaped) = self.shape(value, letter_spacing, word_spacing)
            && let Some(run) = self.rasterize_shaped(value, &shaped, justify_spacing)
        {
            return Some(run);
        }
        self.rasterize_character_by_character(value, letter_spacing, word_spacing, justify_spacing)
    }

    fn shape(
        &self,
        value: &str,
        letter_spacing: u32,
        word_spacing: u32,
    ) -> Option<NativeShapedRun> {
        let (face_index, face) = self
            .faces
            .iter()
            .enumerate()
            .find(|(_, face)| value.chars().all(|character| has_glyph(face, character)))?;
        self.shape_with_face(face_index, face, value, letter_spacing, word_spacing)
    }

    fn shape_with_face(
        &self,
        face_index: usize,
        face: &NativeFontFace,
        value: &str,
        letter_spacing: u32,
        word_spacing: u32,
    ) -> Option<NativeShapedRun> {
        let shaper_data = face.shaper_data.as_ref()?;
        let font = harfrust::FontRef::new(face.font_data.as_ref()).ok()?;
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(value);
        buffer.set_flags(
            harfrust::BufferFlags::BEGINNING_OF_TEXT | harfrust::BufferFlags::END_OF_TEXT,
        );
        buffer.guess_segment_properties();
        let direction = match self.direction {
            DirectionValue::Ltr => harfrust::Direction::LeftToRight,
            DirectionValue::Rtl => harfrust::Direction::RightToLeft,
        };
        buffer.set_direction(direction);
        let scale = i32::try_from(self.font_size.saturating_mul(FONT_SHAPE_SCALE)).ok()?;
        let shaper = shaper_data.shaper(&font).build();
        let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new().scale(Some(scale)));
        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();
        if infos.len() != positions.len() {
            return None;
        }

        let characters: Vec<char> = value.chars().collect();
        let character_count = characters.len();
        let cluster_indices = infos
            .iter()
            .map(|info| cluster_index(value, info.cluster))
            .collect::<Option<Vec<_>>>()?;
        let mut glyphs = Vec::with_capacity(infos.len());
        let mut pen_x = 0i64;
        let mut previous_cluster = None;
        for (index, ((info, position), &cluster)) in infos
            .iter()
            .zip(positions)
            .zip(&cluster_indices)
            .enumerate()
        {
            if cluster >= character_count && character_count != 0 {
                return None;
            }
            if previous_cluster.is_some_and(|previous| match self.direction {
                DirectionValue::Ltr => cluster < previous,
                DirectionValue::Rtl => cluster > previous,
            }) {
                return None;
            }
            let is_last_for_cluster = infos
                .get(index + 1)
                .is_none_or(|next| next.cluster != info.cluster);
            let cluster_end = cluster_indices
                .iter()
                .copied()
                .filter(|next| *next > cluster)
                .min()
                .unwrap_or(character_count);
            let cluster_spacing = if is_last_for_cluster {
                spacing_for_characters(
                    &characters,
                    cluster.min(character_count)..cluster_end.min(character_count),
                    letter_spacing,
                    word_spacing,
                )
            } else {
                0
            };
            if position.x_advance < 0 {
                return None;
            }
            glyphs.push(NativeShapedGlyph {
                glyph_id: info.glyph_id,
                x_offset: position.x_offset,
                y_offset: position.y_offset,
                x_advance: position.x_advance,
                cluster,
                cluster_spacing,
            });
            pen_x = pen_x
                .saturating_add(i64::from(position.x_advance))
                .saturating_add(cluster_spacing);
            previous_cluster = Some(cluster);
        }
        Some(NativeShapedRun {
            face_index,
            glyphs,
            width: round_fixed_nonnegative(pen_x),
            width_fixed: pen_x,
        })
    }

    fn rasterize_shaped(
        &self,
        value: &str,
        shaped: &NativeShapedRun,
        justify_spacing: u32,
    ) -> Option<NativeFontRun> {
        let face = self.faces.get(shaped.face_index)?;
        let characters: Vec<char> = value.chars().collect();
        let justify_unit = i64::from(justify_spacing).saturating_mul(i64::from(FONT_SHAPE_SCALE));
        let justified_space_count = shaped
            .glyphs
            .iter()
            .enumerate()
            .filter(|(index, shaped_glyph)| {
                shaped
                    .glyphs
                    .get(index.saturating_add(1))
                    .is_none_or(|next| next.cluster != shaped_glyph.cluster)
                    && characters
                        .get(shaped_glyph.cluster)
                        .is_some_and(|character| character.is_whitespace())
            })
            .count();
        let total_width_fixed = shaped.width_fixed.saturating_add(
            i64::try_from(justified_space_count)
                .unwrap_or(i64::MAX)
                .saturating_mul(justify_unit),
        );
        let mut glyphs = Vec::new();
        let mut space_ranges = Vec::new();
        let mut pen_x = 0i64;
        let mut current_cluster = None;
        let mut cluster_start = 0i64;
        for (index, shaped_glyph) in shaped.glyphs.iter().enumerate() {
            if current_cluster != Some(shaped_glyph.cluster) {
                current_cluster = Some(shaped_glyph.cluster);
                cluster_start = pen_x;
            }
            let is_last_for_cluster = shaped
                .glyphs
                .get(index + 1)
                .is_none_or(|next| next.cluster != shaped_glyph.cluster);
            let character = characters.get(shaped_glyph.cluster).copied();
            let justify = if is_last_for_cluster
                && character.is_some_and(|character| character.is_whitespace())
            {
                justify_unit
            } else {
                0
            };
            let glyph_id = u16::try_from(shaped_glyph.glyph_id).ok()?;
            if glyph_id >= face.font.glyph_count() {
                return None;
            }
            let (metrics, coverage) = face.font.rasterize_indexed(glyph_id, self.font_size as f32);
            let advance_fixed = i64::from(shaped_glyph.x_advance)
                .saturating_add(shaped_glyph.cluster_spacing)
                .saturating_add(justify);
            let advance = round_fixed_nonnegative(advance_fixed);
            if is_last_for_cluster && character.is_some_and(|character| character.is_whitespace()) {
                let range_end = pen_x.saturating_add(advance_fixed);
                let (range_start, range_end) = match self.direction {
                    DirectionValue::Ltr => (cluster_start, range_end),
                    DirectionValue::Rtl => (
                        total_width_fixed.saturating_sub(range_end),
                        total_width_fixed.saturating_sub(cluster_start),
                    ),
                };
                space_ranges.push(NativeSpaceRange {
                    start: round_signed_fixed(range_start),
                    end: round_signed_fixed(range_end).max(round_signed_fixed(range_start)),
                    char_index: shaped_glyph.cluster,
                });
            }
            if metrics.width > 0 && metrics.height > 0 && !coverage.is_empty() {
                let width = u32::try_from(metrics.width).ok()?;
                let height = u32::try_from(metrics.height).ok()?;
                let y = i32::try_from(self.ascent)
                    .ok()?
                    .saturating_sub(
                        i32::try_from(metrics.height)
                            .ok()?
                            .saturating_add(metrics.ymin),
                    )
                    .saturating_sub(round_signed_fixed(i64::from(shaped_glyph.y_offset)));
                let glyph_x = match self.direction {
                    DirectionValue::Ltr => pen_x.saturating_add(i64::from(shaped_glyph.x_offset)),
                    DirectionValue::Rtl => total_width_fixed
                        .saturating_sub(pen_x)
                        .saturating_sub(advance_fixed)
                        .saturating_add(i64::from(shaped_glyph.x_offset)),
                };
                glyphs.push(NativeGlyph {
                    x: round_signed_fixed(glyph_x).saturating_add(metrics.xmin),
                    y,
                    width,
                    height,
                    advance,
                    coverage: Arc::from(coverage),
                });
            }
            pen_x = pen_x
                .saturating_add(i64::from(shaped_glyph.x_advance))
                .saturating_add(shaped_glyph.cluster_spacing)
                .saturating_add(justify);
        }
        Some(NativeFontRun {
            glyphs,
            space_ranges,
            width: round_fixed_nonnegative(total_width_fixed),
            ascent: self.ascent,
            line_height: self.line_height,
        })
    }

    fn rasterize_character_by_character(
        &self,
        value: &str,
        letter_spacing: u32,
        word_spacing: u32,
        justify_spacing: u32,
    ) -> Option<NativeFontRun> {
        let mut glyphs = Vec::new();
        let mut space_ranges = Vec::new();
        let mut x = 0i32;
        let mut previous = None;
        let mut previous_face_index = None;
        for (char_index, character) in value.chars().enumerate() {
            let face_index = self.face_index_for_character(character).unwrap_or(0);
            let face = self.faces.get(face_index)?;
            if previous_face_index == Some(face_index)
                && let Some(previous) = previous
            {
                let kerning = face
                    .font
                    .horizontal_kern(previous, character, self.font_size as f32)
                    .map_or(0, round_signed);
                x = x.saturating_add(kerning);
            }
            let (metrics, coverage) = face.font.rasterize(character, self.font_size as f32);
            let advance = self
                .advance(character, letter_spacing, word_spacing)
                .saturating_add(if character == ' ' { justify_spacing } else { 0 });
            if character.is_whitespace() {
                space_ranges.push(NativeSpaceRange {
                    start: x,
                    end: x.saturating_add(i32::try_from(advance).unwrap_or(i32::MAX)),
                    char_index,
                });
            }
            if metrics.width > 0 && metrics.height > 0 && !coverage.is_empty() {
                let width = u32::try_from(metrics.width).ok()?;
                let height = u32::try_from(metrics.height).ok()?;
                let y = i32::try_from(self.ascent).ok()?.saturating_sub(
                    i32::try_from(metrics.height)
                        .ok()?
                        .saturating_add(metrics.ymin),
                );
                glyphs.push(NativeGlyph {
                    x: x.saturating_add(metrics.xmin),
                    y,
                    width,
                    height,
                    advance,
                    coverage: Arc::from(coverage),
                });
            }
            x = x.saturating_add(i32::try_from(advance).unwrap_or(i32::MAX));
            previous = Some(character);
            previous_face_index = Some(face_index);
        }
        Some(NativeFontRun {
            glyphs,
            space_ranges,
            width: u32::try_from(x.max(0)).unwrap_or(u32::MAX),
            ascent: self.ascent,
            line_height: self.line_height,
        })
    }
}

#[derive(Debug, Clone)]
struct NativeShapedGlyph {
    glyph_id: u32,
    x_offset: i32,
    y_offset: i32,
    x_advance: i32,
    cluster: usize,
    cluster_spacing: i64,
}

#[derive(Debug, Clone)]
struct NativeShapedRun {
    face_index: usize,
    glyphs: Vec<NativeShapedGlyph>,
    width: u32,
    width_fixed: i64,
}

impl NativeFontBook {
    pub(crate) fn system() -> Self {
        system_font_book().clone()
    }

    pub(crate) fn from_resources(resources: &[NativeFontFaceResource]) -> Self {
        let mut book = Self::default();
        for resource in resources.iter().take(MAX_NATIVE_FONT_FACES) {
            if resource.bytes.is_empty() || resource.bytes.len() > MAX_NATIVE_FONT_BYTES {
                continue;
            }
            let Ok(font) = fontdue::Font::from_bytes(
                resource.bytes.as_ref().to_vec(),
                fontdue::FontSettings {
                    collection_index: 0,
                    scale: FONT_PARSE_SCALE,
                    load_substitutions: true,
                },
            ) else {
                continue;
            };
            let font_data = resource.bytes.clone();
            let shaper_data = harfrust::FontRef::new(font_data.as_ref())
                .ok()
                .map(|font| Arc::new(harfrust::ShaperData::new(&font)));
            book.faces.push(NativeFontFace {
                family: resource.family.clone(),
                family_key: resource.family_key,
                generic_family: None,
                weight: resource.weight,
                style: resource.style,
                font: Arc::new(font),
                font_data,
                shaper_data,
            });
        }
        book.faces.extend(system_font_book().faces.iter().cloned());
        book
    }

    fn faces_for(
        &self,
        families: NativeFontFamilyList,
        weight: FontWeightValue,
        style: FontStyleValue,
    ) -> Vec<Arc<NativeFontFace>> {
        let mut selected = Vec::new();
        for family in families.iter() {
            let mut best = None;
            let mut best_score = u8::MAX;
            for face in &self.faces {
                if !family_matches(family, face) {
                    continue;
                }
                let score = face_score(face, weight, style);
                if score < best_score {
                    best_score = score;
                    best = Some(face);
                }
            }
            if let Some(face) = best {
                selected.push(Arc::new(face.clone()));
            }
        }
        selected
    }
}

fn has_glyph(face: &NativeFontFace, character: char) -> bool {
    face.font.lookup_glyph_index(character) != 0
}

fn system_font_book() -> &'static NativeFontBook {
    static BOOK: OnceLock<NativeFontBook> = OnceLock::new();
    BOOK.get_or_init(load_system_font_book)
}

fn load_system_font_book() -> NativeFontBook {
    let mut book = NativeFontBook::default();
    for &(path, family, generic_family, weight, style) in system_font_candidates() {
        let path = Path::new(path);
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(font) = fontdue::Font::from_bytes(
            bytes.clone(),
            fontdue::FontSettings {
                collection_index: 0,
                scale: FONT_PARSE_SCALE,
                load_substitutions: true,
            },
        ) else {
            continue;
        };
        let font_data: Arc<[u8]> = Arc::from(bytes);
        let shaper_data = harfrust::FontRef::new(font_data.as_ref())
            .ok()
            .map(|font| Arc::new(harfrust::ShaperData::new(&font)));
        book.faces.push(NativeFontFace {
            family: family.to_owned(),
            family_key: font_family_hash(family),
            generic_family: Some(generic_family),
            weight,
            style,
            font: Arc::new(font),
            font_data,
            shaper_data,
        });
    }
    book
}

fn family_matches(family: NativeFontFamilyValue, face: &NativeFontFace) -> bool {
    match family {
        NativeFontFamilyValue::Named(key) => key == face.family_key,
        NativeFontFamilyValue::Generic(generic) => face.generic_family == Some(generic),
        NativeFontFamilyValue::Fallback => false,
    }
}

fn face_score(face: &NativeFontFace, weight: FontWeightValue, style: FontStyleValue) -> u8 {
    let weight_score = u8::from(face.weight != weight);
    let style_score = u8::from(face.style != style);
    weight_score.saturating_mul(2).saturating_add(style_score)
}

fn ceil_positive(value: f32) -> u32 {
    if value.is_finite() && value > 0.0 {
        value.ceil().min(u32::MAX as f32) as u32
    } else {
        0
    }
}

fn round_signed(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn round_signed_fixed(value: i64) -> i32 {
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

fn round_fixed_nonnegative(value: i64) -> u32 {
    if value <= 0 {
        return 0;
    }
    let rounded =
        value.saturating_add(i64::from(FONT_SHAPE_SCALE / 2)) / i64::from(FONT_SHAPE_SCALE);
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

fn cluster_index(value: &str, cluster: u32) -> Option<usize> {
    let byte = usize::try_from(cluster).ok()?;
    (byte <= value.len() && value.is_char_boundary(byte)).then(|| value[..byte].chars().count())
}

fn spacing_for_characters(
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
fn system_font_candidates() -> &'static [FontCandidate] {
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
fn system_font_candidates() -> &'static [FontCandidate] {
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
fn system_font_candidates() -> &'static [FontCandidate] {
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
fn system_font_candidates() -> &'static [FontCandidate] {
    &[]
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn ordered_font_candidates_preserve_css_family_order() {
        let families = NativeFontFamilyList::parse("sans-serif, serif").unwrap();
        let candidates =
            system_font_book().faces_for(families, FontWeightValue::Normal, FontStyleValue::Normal);
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
            weight: FontWeightValue::Normal,
            style: FontStyleValue::Normal,
            bytes: system_face.font_data.clone(),
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
        if face.shaper_data.is_none() {
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
        if face.shaper_data.is_none() {
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
        let metrics = NativeTextMetrics::fallback_with_direction(
            DEFAULT_NATIVE_FONT_SIZE,
            DirectionValue::Ltr,
        );
        assert_eq!(metrics.measure_text("A B", 0, 0), 24);
        assert_eq!(metrics.line_height(), FALLBACK_LINE_HEIGHT);
        assert!(metrics.rasterize("A", 0, 0, 0).is_none());
    }
}

use super::css::{
    DirectionValue, FontStyleValue, FontWeightValue, NativeFontFaceRule, NativeFontFamilyList,
    NativeFontFamilyValue, NativeGenericFontFamily, NativeUnicodeRange, font_family_hash,
};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
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
const MAX_NATIVE_SYSTEM_FONT_FILES: usize = 512;
const MAX_NATIVE_SYSTEM_FONT_FACES: usize = 64;
const MAX_NATIVE_SYSTEM_FONT_BYTES: usize = 64 * 1024 * 1024;
const MAX_NATIVE_SYSTEM_COLLECTION_FACES: u32 = 32;
const MAX_WOFF_TABLES: usize = 256;
const WOFF_HEADER_BYTES: usize = 44;
const WOFF_TABLE_BYTES: usize = 20;

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
    unicode_ranges: Arc<[NativeUnicodeRange]>,
}

impl PartialEq for NativeFontFace {
    fn eq(&self, other: &Self) -> bool {
        self.family == other.family
            && self.family_key == other.family_key
            && self.generic_family == other.generic_family
            && self.weight == other.weight
            && self.style == other.style
            && self.font_data == other.font_data
            && self.unicode_ranges == other.unicode_ranges
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
    pub(crate) unicode_ranges: Vec<NativeUnicodeRange>,
}

impl NativeFontFaceResource {
    pub(crate) fn from_rule(rule: &NativeFontFaceRule, bytes: Vec<u8>) -> Self {
        Self {
            family: rule.family.clone(),
            family_key: rule.family_key,
            weight: rule.weight,
            style: rule.style,
            bytes: Arc::from(bytes),
            unicode_ranges: rule.unicode_ranges.clone(),
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

#[derive(Debug, Clone, Copy)]
struct WoffTable {
    tag: u32,
    offset: usize,
    compressed_len: usize,
    original_len: usize,
    checksum: u32,
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    let value = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes(value.try_into().ok()?))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    let value = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes(value.try_into().ok()?))
}

fn write_u16_be(bytes: &mut [u8], offset: usize, value: u16) {
    if let Some(slot) = bytes.get_mut(offset..offset.saturating_add(2)) {
        if slot.len() == 2 {
            slot.copy_from_slice(&value.to_be_bytes());
        }
    }
}

fn write_u32_be(bytes: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = bytes.get_mut(offset..offset.saturating_add(4)) {
        if slot.len() == 4 {
            slot.copy_from_slice(&value.to_be_bytes());
        }
    }
}

fn supported_woff_flavor(flavor: u32) -> bool {
    matches!(
        flavor,
        0x0001_0000 | 0x4f54_544f | 0x7472_7565 | 0x7479_7031 | 0x7474_6366
    )
}

fn valid_woff_range(offset: usize, length: usize, total_length: usize) -> bool {
    offset
        .checked_add(length)
        .is_some_and(|end| offset <= total_length && end <= total_length)
}

fn sfnt_checksum(bytes: &[u8]) -> u32 {
    bytes
        .chunks(4)
        .map(|chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_be_bytes(word)
        })
        .fold(0, u32::wrapping_add)
}

/// Convert a bounded WOFF 1.0 container to the SFNT bytes consumed by the
/// existing fontdue/HarfRust owners. WOFF2 and unsupported payloads remain
/// fail-closed until their decompressor and renderer paths are implemented.
fn decode_woff(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < WOFF_HEADER_BYTES || bytes.len() > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    let flavor = read_u32_be(bytes, 4)?;
    if !supported_woff_flavor(flavor) {
        return None;
    }
    let declared_length = usize::try_from(read_u32_be(bytes, 8)?).ok()?;
    if declared_length != bytes.len() {
        return None;
    }
    let table_count = usize::from(read_u16_be(bytes, 12)?);
    if table_count == 0 || table_count > MAX_WOFF_TABLES {
        return None;
    }
    let total_sfnt_size = usize::try_from(read_u32_be(bytes, 16)?).ok()?;
    if total_sfnt_size == 0 || total_sfnt_size > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    for (offset_index, length_index) in [(24, 28), (36, 40)] {
        let offset = usize::try_from(read_u32_be(bytes, offset_index)?).ok()?;
        let length = usize::try_from(read_u32_be(bytes, length_index)?).ok()?;
        if length > 0 && !valid_woff_range(offset, length, declared_length) {
            return None;
        }
    }
    let table_directory_end =
        WOFF_HEADER_BYTES.checked_add(table_count.checked_mul(WOFF_TABLE_BYTES)?)?;
    if table_directory_end > declared_length {
        return None;
    }
    let mut tables = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let entry = WOFF_HEADER_BYTES.checked_add(index.checked_mul(WOFF_TABLE_BYTES)?)?;
        let offset = usize::try_from(read_u32_be(bytes, entry.checked_add(4)?)?).ok()?;
        let compressed_len = usize::try_from(read_u32_be(bytes, entry.checked_add(8)?)?).ok()?;
        let original_len = usize::try_from(read_u32_be(bytes, entry.checked_add(12)?)?).ok()?;
        if compressed_len == 0
            || original_len == 0
            || compressed_len > original_len
            || offset < table_directory_end
            || !valid_woff_range(offset, compressed_len, declared_length)
        {
            return None;
        }
        tables.push(WoffTable {
            tag: read_u32_be(bytes, entry)?,
            offset,
            compressed_len,
            original_len,
            checksum: read_u32_be(bytes, entry.checked_add(16)?)?,
        });
    }
    let mut ranges = tables
        .iter()
        .map(|table| {
            Some((
                table.offset,
                table.offset.checked_add(table.compressed_len)?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return None;
    }
    tables.sort_unstable_by_key(|table| table.tag);

    let directory_size = 12usize.checked_add(table_count.checked_mul(16)?)?;
    if directory_size > total_sfnt_size {
        return None;
    }
    let original_table_bytes = tables
        .iter()
        .try_fold(0usize, |total, table| total.checked_add(table.original_len))?;
    if original_table_bytes > total_sfnt_size.saturating_sub(directory_size) {
        return None;
    }
    let mut max_power_of_two = 1usize;
    let mut entry_selector = 0u16;
    while max_power_of_two
        .checked_mul(2)
        .is_some_and(|value| value <= table_count)
    {
        max_power_of_two = max_power_of_two.saturating_mul(2);
        entry_selector = entry_selector.saturating_add(1);
    }
    let search_range = u16::try_from(max_power_of_two.checked_mul(16)?).ok()?;
    let range_shift = u16::try_from(
        table_count
            .checked_mul(16)?
            .saturating_sub(usize::from(search_range)),
    )
    .ok()?;
    let mut output = vec![0; directory_size];
    write_u32_be(&mut output, 0, flavor);
    write_u16_be(&mut output, 4, u16::try_from(table_count).ok()?);
    write_u16_be(&mut output, 6, search_range);
    write_u16_be(&mut output, 8, entry_selector);
    write_u16_be(&mut output, 10, range_shift);
    let mut head_offset = None;
    for (index, table) in tables.iter().enumerate() {
        let end = table.offset.checked_add(table.compressed_len)?;
        let mut decoded = Vec::with_capacity(table.original_len);
        if table.compressed_len == table.original_len {
            decoded.extend_from_slice(bytes.get(table.offset..end)?);
        } else {
            let compressed = bytes.get(table.offset..end)?;
            let decoder = flate2::read::ZlibDecoder::new(compressed);
            decoder
                .take(u64::try_from(table.original_len.saturating_add(1)).ok()?)
                .read_to_end(&mut decoded)
                .ok()?;
        }
        if decoded.len() != table.original_len {
            return None;
        }
        let aligned_length = output
            .len()
            .checked_add(3)?
            .checked_div(4)?
            .checked_mul(4)?;
        if aligned_length > total_sfnt_size {
            return None;
        }
        output.resize(aligned_length, 0);
        if decoded.len() > total_sfnt_size.saturating_sub(output.len()) {
            return None;
        }
        let data_offset = output.len();
        output.extend_from_slice(&decoded);
        if output.len() > total_sfnt_size {
            return None;
        }
        let directory_entry = 12usize.checked_add(index.checked_mul(16)?)?;
        write_u32_be(&mut output, directory_entry, table.tag);
        write_u32_be(&mut output, directory_entry.checked_add(4)?, table.checksum);
        write_u32_be(
            &mut output,
            directory_entry.checked_add(8)?,
            u32::try_from(data_offset).ok()?,
        );
        write_u32_be(
            &mut output,
            directory_entry.checked_add(12)?,
            u32::try_from(table.original_len).ok()?,
        );
        if table.tag == u32::from_be_bytes(*b"head") {
            head_offset = Some(data_offset);
        }
    }
    if output.len() != total_sfnt_size {
        return None;
    }
    if let Some(head_offset) = head_offset
        && head_offset
            .checked_add(12)
            .is_some_and(|end| end <= output.len())
    {
        write_u32_be(&mut output, head_offset.checked_add(8)?, 0);
        let adjustment = 0xb1b0_afbau32.wrapping_sub(sfnt_checksum(&output));
        write_u32_be(&mut output, head_offset.checked_add(8)?, adjustment);
    }
    Some(output)
}

fn normalize_font_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    match read_u32_be(bytes, 0)? {
        0x774f_4646 => decode_woff(bytes),
        0x774f_4632 => None,
        _ => Some(bytes.to_vec()),
    }
}

impl NativeFontBook {
    pub(crate) fn system() -> Self {
        system_font_book().clone()
    }

    /// Check whether a bounded payload can be consumed by the native font
    /// rasterizer. This is kept separate from `from_resources`, whose
    /// historical wire-snapshot behavior intentionally skips malformed
    /// entries instead of rejecting the whole document.
    pub(crate) fn is_parseable_font_bytes(bytes: &[u8]) -> bool {
        let Some(bytes) = normalize_font_bytes(bytes) else {
            return false;
        };
        fontdue::Font::from_bytes(
            bytes,
            fontdue::FontSettings {
                collection_index: 0,
                scale: FONT_PARSE_SCALE,
                load_substitutions: true,
            },
        )
        .is_ok()
    }

    pub(crate) fn local_font_bytes(
        &self,
        family: &str,
        weight: FontWeightValue,
        style: FontStyleValue,
    ) -> Option<Vec<u8>> {
        let mut best = None;
        let mut best_score = u8::MAX;
        for face in &self.faces {
            if !face.family.eq_ignore_ascii_case(family) {
                continue;
            }
            let score = face_score(face, weight, style);
            if score < best_score {
                best_score = score;
                best = Some(face);
            }
        }
        best.map(|face| face.font_data.as_ref().to_vec())
    }

    pub(crate) fn from_resources(resources: &[NativeFontFaceResource]) -> Self {
        let mut book = Self::default();
        for resource in resources.iter().take(MAX_NATIVE_FONT_FACES) {
            let Some(font_data) = normalize_font_bytes(resource.bytes.as_ref()) else {
                continue;
            };
            let Ok(font) = fontdue::Font::from_bytes(
                font_data.clone(),
                fontdue::FontSettings {
                    collection_index: 0,
                    scale: FONT_PARSE_SCALE,
                    load_substitutions: true,
                },
            ) else {
                continue;
            };
            let font_data: Arc<[u8]> = Arc::from(font_data);
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
                unicode_ranges: Arc::from(resource.unicode_ranges.clone()),
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
            let matching = self
                .faces
                .iter()
                .filter(|face| family_matches(family, face))
                .collect::<Vec<_>>();
            let Some(best_score) = matching
                .iter()
                .map(|face| face_score(face, weight, style))
                .min()
            else {
                continue;
            };
            let best = matching
                .into_iter()
                .filter(|face| face_score(face, weight, style) == best_score)
                .collect::<Vec<_>>();
            if best.iter().any(|face| !face.unicode_ranges.is_empty()) {
                selected.extend(best.into_iter().map(|face| Arc::new(face.clone())));
            } else if let Some(face) = best.into_iter().next() {
                selected.push(Arc::new(face.clone()));
            }
        }
        selected
    }
}

fn has_glyph(face: &NativeFontFace, character: char) -> bool {
    (face.unicode_ranges.is_empty()
        || face
            .unicode_ranges
            .iter()
            .copied()
            .any(|range| range.contains(character)))
        && face.font.lookup_glyph_index(character) != 0
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
                    && face.weight == weight
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

fn insert_font_face(
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
            && face.weight == weight
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
        weight,
        style,
        font: Arc::new(font),
        font_data,
        shaper_data,
        unicode_ranges: Arc::from(Vec::<NativeUnicodeRange>::new()),
    });
    true
}

fn discovered_font_metadata(
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

fn discovered_generic_family(family: &str) -> Option<NativeGenericFontFamily> {
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

fn system_font_roots() -> Vec<PathBuf> {
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

fn discover_system_font_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in system_font_roots() {
        collect_system_font_paths(&root, &mut paths);
    }
    paths.sort_unstable();
    paths.dedup();
    paths.truncate(MAX_NATIVE_SYSTEM_FONT_FILES);
    paths
}

fn collect_system_font_paths(root: &Path, paths: &mut Vec<PathBuf>) {
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

fn is_discoverable_font_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        })
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
    fn local_font_lookup_matches_system_family_case_insensitively() {
        let Some(face) = system_font_book().faces.first() else {
            return;
        };
        let family = face.family.to_ascii_uppercase();
        let bytes = system_font_book()
            .local_font_bytes(&family, face.weight, face.style)
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
                && face.weight == weight
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
        let header_size =
            WOFF_HEADER_BYTES.checked_add(table_count.checked_mul(WOFF_TABLE_BYTES)?)?;
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
            weight: FontWeightValue::Normal,
            style: FontStyleValue::Normal,
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
                weight: FontWeightValue::Normal,
                style: FontStyleValue::Normal,
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

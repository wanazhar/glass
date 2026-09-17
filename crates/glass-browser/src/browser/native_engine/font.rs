use super::css::{
    FontStyleValue, FontWeightValue, NativeFontFamilyList, NativeFontFamilyValue,
    NativeGenericFontFamily, font_family_hash,
};
use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// The CSS initial font size used by the bounded native text fallback.
pub(crate) const DEFAULT_NATIVE_FONT_SIZE: u32 = 16;
/// Prevent a stylesheet from requesting an unbounded rasterization scale.
pub(crate) const MAX_NATIVE_FONT_SIZE: u32 = 256;
const FONT_PARSE_SCALE: f32 = 40.0;
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

/// A shaped-by-character font run consumed by the native display list.
///
/// Character-by-character placement is intentional in this first real-font
/// slice. Complex-script shaping and grapheme-safe line breaking remain a
/// later HarfRust integration; the run still uses real font metrics and
/// rasterized glyph coverage rather than the legacy fixed 5x7 bitmap.
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
    generic_family: NativeGenericFontFamily,
    weight: FontWeightValue,
    style: FontStyleValue,
    font: Arc<fontdue::Font>,
}

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

#[derive(Debug, Clone, Default)]
struct NativeFontBook {
    faces: Vec<NativeFontFace>,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeTextMetrics {
    face: Option<Arc<NativeFontFace>>,
    font_size: u32,
    ascent: u32,
    line_height: u32,
}

impl NativeTextMetrics {
    pub(crate) fn fallback(font_size: u32) -> Self {
        Self {
            face: None,
            font_size: font_size.clamp(1, MAX_NATIVE_FONT_SIZE),
            ascent: FALLBACK_LINE_HEIGHT.saturating_sub(5),
            line_height: FALLBACK_LINE_HEIGHT,
        }
    }

    pub(crate) fn for_style(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
    ) -> Self {
        let font_size = font_size.clamp(1, MAX_NATIVE_FONT_SIZE);
        let book = system_font_book();
        let face = book.face_for(families, weight, style);
        let Some(face) = face else {
            return Self::fallback(font_size);
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
            face: Some(face),
            font_size,
            ascent,
            line_height,
        }
    }

    pub(crate) const fn line_height(&self) -> u32 {
        self.line_height
    }

    pub(crate) fn advance(&self, character: char, letter_spacing: u32, word_spacing: u32) -> u32 {
        let base = self.face.as_ref().map_or(FALLBACK_GLYPH_ADVANCE, |face| {
            ceil_positive(
                face.font
                    .metrics(character, self.font_size as f32)
                    .advance_width,
            )
        });
        base.saturating_add(letter_spacing)
            .saturating_add(if character == ' ' { word_spacing } else { 0 })
    }

    pub(crate) fn measure_text(&self, value: &str, letter_spacing: u32, word_spacing: u32) -> u32 {
        let mut width = 0u32;
        let mut previous = None;
        for character in value.chars() {
            if let Some(face) = &self.face
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
        let face = self.face.as_ref()?;
        let mut glyphs = Vec::new();
        let mut space_ranges = Vec::new();
        let mut x = 0i32;
        let mut previous = None;
        for (char_index, character) in value.chars().enumerate() {
            if let Some(previous) = previous {
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

impl NativeFontBook {
    fn face_for(
        &self,
        families: NativeFontFamilyList,
        weight: FontWeightValue,
        style: FontStyleValue,
    ) -> Option<Arc<NativeFontFace>> {
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
                return Some(Arc::new(face.clone()));
            }
        }
        None
    }
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
            bytes,
            fontdue::FontSettings {
                collection_index: 0,
                scale: FONT_PARSE_SCALE,
                load_substitutions: false,
            },
        ) else {
            continue;
        };
        book.faces.push(NativeFontFace {
            family: family.to_owned(),
            family_key: font_family_hash(family),
            generic_family,
            weight,
            style,
            font: Arc::new(font),
        });
    }
    book
}

fn family_matches(family: NativeFontFamilyValue, face: &NativeFontFace) -> bool {
    match family {
        NativeFontFamilyValue::Named(key) => key == face.family_key,
        NativeFontFamilyValue::Generic(generic) => generic == face.generic_family,
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
        );
        assert!(metrics.line_height() > 0);
        assert!(metrics.measure_text("Glass", 0, 0) > 0);
        if metrics.face.is_some() {
            let run = metrics.rasterize("Glass", 0, 0, 0).unwrap();
            assert!(!run.glyphs.is_empty());
            assert!(run.width > 0);
        }
    }

    #[test]
    fn fallback_metrics_keep_the_legacy_cell_contract() {
        let metrics = NativeTextMetrics::fallback(DEFAULT_NATIVE_FONT_SIZE);
        assert_eq!(metrics.measure_text("A B", 0, 0), 24);
        assert_eq!(metrics.line_height(), FALLBACK_LINE_HEIGHT);
        assert!(metrics.rasterize("A", 0, 0, 0).is_none());
    }
}

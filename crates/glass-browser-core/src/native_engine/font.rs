#[path = "font/catalog.rs"]
mod catalog;
mod color_glyphs;
mod woff;
#[cfg(test)]
use crate::native_engine::css::NativeFontFeature;
use crate::native_engine::css::NativeFontVariation;
use crate::native_engine::css::{
    DirectionValue, FontStyleValue, FontWeightValue, MAX_NATIVE_FONT_PALETTE_OVERRIDES,
    NativeColor, NativeFontDisplay, NativeFontFaceRule, NativeFontFamilyList,
    NativeFontFamilyValue, NativeFontFeatureSettings, NativeFontKerning,
    NativeFontLanguageOverride, NativeFontMetricOverrides, NativeFontOpticalSizing,
    NativeFontPalette, NativeFontPaletteOverride, NativeFontStretchRange,
    NativeFontVariantAlternates, NativeFontVariantCaps, NativeFontVariantEastAsian,
    NativeFontVariantEastAsianForm, NativeFontVariantEastAsianWidth, NativeFontVariantLigatures,
    NativeFontVariantNumeric, NativeFontVariantNumericFigure, NativeFontVariantNumericFraction,
    NativeFontVariantNumericSpacing, NativeFontVariantPosition, NativeFontVariationSettings,
    NativeFontWeightRange, NativeGenericFontFamily, NativeUnicodeRange, font_family_hash,
};
use catalog::*;
use color_glyphs::*;
use std::fmt;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use woff::normalize_font_bytes;
#[cfg(test)]
use woff::{read_u16_be, read_u32_be, supported_woff_flavor, write_u16_be, write_u32_be};
#[cfg(test)]
type TestLocalFontSource = OnceLock<std::sync::Mutex<Option<(String, Vec<u8>)>>>;
#[cfg(test)]
static TEST_LOCAL_FONT_SOURCE: TestLocalFontSource = OnceLock::new();

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
const WOFF2_HEADER_BYTES: usize = 48;
const WOFF_TABLE_BYTES: usize = 20;
const MAX_COLOR_GLYPH_LAYERS: usize = 32;
const MAX_COLOR_TRANSFORM_DEPTH: usize = 16;
const MAX_COLOR_CLIP_DEPTH: usize = 16;
const MAX_COLOR_TRANSFORM_COMPONENT: f32 = 1_000_000.0;
const MAX_COLOR_STOPS: usize = 16;
const RADIAL_EQUATION_RELATIVE_EPSILON: f64 = 1e-12;
const MAX_COLOR_BITMAP_DIMENSION: usize = 1_024;
const MAX_COLOR_BITMAP_BYTES: usize = 4 * 1024 * 1024;
const MAX_VARIABLE_OUTLINE_POINTS: usize = 8_192;
const MAX_VARIABLE_RASTER_DIMENSION: usize = 1_024;
const VARIABLE_RASTER_SAMPLES: u32 = 4;

/// One coverage bitmap and placement produced by the selected font face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeGlyph {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub advance: u32,
    pub color: Option<NativeColor>,
    pub composite: NativeGlyphComposite,
    pub gradient: Option<NativeGlyphGradient>,
    pub bitmap: Option<Arc<[u8]>>,
    pub coverage: Arc<[u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeGlyphComposite {
    #[default]
    SourceOver,
    DestinationOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeGradientExtend {
    Pad,
    Repeat,
    Reflect,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeGradientStop {
    pub offset: f32,
    pub color: NativeColor,
}

impl PartialEq for NativeGradientStop {
    fn eq(&self, other: &Self) -> bool {
        self.offset.to_bits() == other.offset.to_bits() && self.color == other.color
    }
}

impl Eq for NativeGradientStop {}

#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct NativeGradientTransform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl NativeGradientTransform {
    #[cfg(test)]
    const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn from_ttf(transform: ttf_parser::Transform) -> Self {
        Self {
            a: transform.a,
            b: transform.b,
            c: transform.c,
            d: transform.d,
            e: transform.e,
            f: transform.f,
        }
    }

    fn compose(self, inner: Self) -> Self {
        Self {
            a: self.a.mul_add(inner.a, self.c * inner.b),
            b: self.b.mul_add(inner.a, self.d * inner.b),
            c: self.a.mul_add(inner.c, self.c * inner.d),
            d: self.b.mul_add(inner.c, self.d * inner.d),
            e: self.a.mul_add(inner.e, self.c.mul_add(inner.f, self.e)),
            f: self.b.mul_add(inner.e, self.d.mul_add(inner.f, self.f)),
        }
    }

    fn scale_x(self, scale: f32) -> Self {
        Self {
            a: self.a * scale,
            b: self.b,
            c: self.c * scale,
            d: self.d,
            e: self.e * scale,
            f: self.f,
        }
    }

    fn is_bounded(self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .into_iter()
            .all(|component| {
                component.is_finite() && component.abs() <= MAX_COLOR_TRANSFORM_COMPONENT
            })
    }

    fn is_invertible(self) -> bool {
        let determinant = self.a.mul_add(self.d, -(self.b * self.c));
        self.is_bounded() && determinant.is_finite() && determinant.abs() > f32::EPSILON
    }

    fn inverse_point(self, x: f32, y: f32) -> Option<(f32, f32)> {
        if !self.is_invertible() || !x.is_finite() || !y.is_finite() {
            return None;
        }
        let determinant = self.a.mul_add(self.d, -(self.b * self.c));
        let delta_x = x - self.e;
        let delta_y = y - self.f;
        let local_x = (self.d.mul_add(delta_x, -(self.c * delta_y))) / determinant;
        let local_y = (self.a.mul_add(delta_y, -(self.b * delta_x))) / determinant;
        (local_x.is_finite()
            && local_y.is_finite()
            && local_x.abs() <= MAX_COLOR_TRANSFORM_COMPONENT
            && local_y.abs() <= MAX_COLOR_TRANSFORM_COMPONENT)
            .then_some((local_x, local_y))
    }
}

impl PartialEq for NativeGradientTransform {
    fn eq(&self, other: &Self) -> bool {
        self.a.to_bits() == other.a.to_bits()
            && self.b.to_bits() == other.b.to_bits()
            && self.c.to_bits() == other.c.to_bits()
            && self.d.to_bits() == other.d.to_bits()
            && self.e.to_bits() == other.e.to_bits()
            && self.f.to_bits() == other.f.to_bits()
    }
}

impl Eq for NativeGradientTransform {}

#[derive(Debug, Clone)]
pub enum NativeGlyphGradient {
    Linear {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        extend: NativeGradientExtend,
        stops: Arc<[NativeGradientStop]>,
    },
    Radial {
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
        transform: NativeGradientTransform,
        extend: NativeGradientExtend,
        stops: Arc<[NativeGradientStop]>,
    },
    Sweep {
        center_x: f32,
        center_y: f32,
        start_angle: f32,
        end_angle: f32,
        transform: NativeGradientTransform,
        extend: NativeGradientExtend,
        stops: Arc<[NativeGradientStop]>,
    },
}

impl PartialEq for NativeGlyphGradient {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Linear {
                    x0: left_x0,
                    y0: left_y0,
                    x1: left_x1,
                    y1: left_y1,
                    x2: left_x2,
                    y2: left_y2,
                    extend: left_extend,
                    stops: left_stops,
                },
                Self::Linear {
                    x0: right_x0,
                    y0: right_y0,
                    x1: right_x1,
                    y1: right_y1,
                    x2: right_x2,
                    y2: right_y2,
                    extend: right_extend,
                    stops: right_stops,
                },
            ) => {
                left_x0.to_bits() == right_x0.to_bits()
                    && left_y0.to_bits() == right_y0.to_bits()
                    && left_x1.to_bits() == right_x1.to_bits()
                    && left_y1.to_bits() == right_y1.to_bits()
                    && left_x2.to_bits() == right_x2.to_bits()
                    && left_y2.to_bits() == right_y2.to_bits()
                    && left_extend == right_extend
                    && left_stops.as_ref() == right_stops.as_ref()
            }
            (
                Self::Radial {
                    x0: left_x0,
                    y0: left_y0,
                    r0: left_r0,
                    x1: left_x1,
                    y1: left_y1,
                    r1: left_r1,
                    transform: left_transform,
                    extend: left_extend,
                    stops: left_stops,
                },
                Self::Radial {
                    x0: right_x0,
                    y0: right_y0,
                    r0: right_r0,
                    x1: right_x1,
                    y1: right_y1,
                    r1: right_r1,
                    transform: right_transform,
                    extend: right_extend,
                    stops: right_stops,
                },
            ) => {
                left_x0.to_bits() == right_x0.to_bits()
                    && left_y0.to_bits() == right_y0.to_bits()
                    && left_r0.to_bits() == right_r0.to_bits()
                    && left_x1.to_bits() == right_x1.to_bits()
                    && left_y1.to_bits() == right_y1.to_bits()
                    && left_r1.to_bits() == right_r1.to_bits()
                    && left_transform == right_transform
                    && left_extend == right_extend
                    && left_stops.as_ref() == right_stops.as_ref()
            }
            (
                Self::Sweep {
                    center_x: left_center_x,
                    center_y: left_center_y,
                    start_angle: left_start,
                    end_angle: left_end,
                    transform: left_transform,
                    extend: left_extend,
                    stops: left_stops,
                },
                Self::Sweep {
                    center_x: right_center_x,
                    center_y: right_center_y,
                    start_angle: right_start,
                    end_angle: right_end,
                    transform: right_transform,
                    extend: right_extend,
                    stops: right_stops,
                },
            ) => {
                left_center_x.to_bits() == right_center_x.to_bits()
                    && left_center_y.to_bits() == right_center_y.to_bits()
                    && left_start.to_bits() == right_start.to_bits()
                    && left_end.to_bits() == right_end.to_bits()
                    && left_transform == right_transform
                    && left_extend == right_extend
                    && left_stops.as_ref() == right_stops.as_ref()
            }
            _ => false,
        }
    }
}

impl Eq for NativeGlyphGradient {}

impl NativeGlyphGradient {
    fn scale_x(&self, requested: u16, nominal: u16) -> Self {
        if nominal == 0 || requested == nominal {
            return self.clone();
        }
        let scale = f32::from(requested) / f32::from(nominal);
        let scale_x = |value: f32| value * scale;
        match self {
            Self::Linear {
                x0,
                y0,
                x1,
                y1,
                x2,
                y2,
                extend,
                stops,
            } => Self::Linear {
                x0: scale_x(*x0),
                y0: *y0,
                x1: scale_x(*x1),
                y1: *y1,
                x2: scale_x(*x2),
                y2: *y2,
                extend: *extend,
                stops: stops.clone(),
            },
            Self::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
                transform,
                extend,
                stops,
            } => Self::Radial {
                x0: *x0,
                y0: *y0,
                r0: *r0,
                x1: *x1,
                y1: *y1,
                r1: *r1,
                transform: transform.scale_x(scale),
                extend: *extend,
                stops: stops.clone(),
            },
            Self::Sweep {
                center_x,
                center_y,
                start_angle,
                end_angle,
                transform,
                extend,
                stops,
            } => Self::Sweep {
                center_x: *center_x,
                center_y: *center_y,
                start_angle: *start_angle,
                end_angle: *end_angle,
                transform: transform.scale_x(scale),
                extend: *extend,
                stops: stops.clone(),
            },
        }
    }

    fn radial_parameter(
        point: (f32, f32),
        start_circle: (f32, f32, f32),
        end_circle: (f32, f32, f32),
    ) -> Option<f32> {
        let (x, y) = point;
        let (x0, y0, r0) = start_circle;
        let (x1, y1, r1) = end_circle;
        if ![x, y, x0, y0, r0, x1, y1, r1]
            .into_iter()
            .all(f32::is_finite)
        {
            return None;
        }

        let x = f64::from(x);
        let y = f64::from(y);
        let x0 = f64::from(x0);
        let y0 = f64::from(y0);
        let r0 = f64::from(r0);
        let x1 = f64::from(x1);
        let y1 = f64::from(y1);
        let r1 = f64::from(r1);
        let center_x = x1 - x0;
        let center_y = y1 - y0;
        let radius_delta = r1 - r0;
        let point_x = x - x0;
        let point_y = y - y0;
        let coefficient_a =
            center_x.mul_add(center_x, center_y * center_y) - radius_delta * radius_delta;
        let coefficient_b =
            -2.0 * center_x.mul_add(point_x, center_y.mul_add(point_y, r0 * radius_delta));
        let coefficient_c = point_x.mul_add(point_x, point_y * point_y) - r0 * r0;
        let geometry_scale = center_x
            .mul_add(center_x, center_y * center_y)
            .max(radius_delta * radius_delta)
            .max(1.0);
        let coefficient_epsilon = geometry_scale * RADIAL_EQUATION_RELATIVE_EPSILON;
        let radius_epsilon = r0.abs().max(r1.abs()).max(1.0) * RADIAL_EQUATION_RELATIVE_EPSILON;
        let mut result = None;
        let mut consider = |parameter: f64| {
            if !parameter.is_finite() {
                return;
            }
            let radius = r0 + radius_delta * parameter;
            if !radius.is_finite() || radius < -radius_epsilon {
                return;
            }
            let Some(parameter) = (parameter as f32).is_finite().then_some(parameter as f32) else {
                return;
            };
            if result.is_none_or(|current| parameter > current) {
                result = Some(parameter);
            }
        };

        if coefficient_a.abs() <= coefficient_epsilon {
            if coefficient_b.abs() > coefficient_epsilon {
                consider(-coefficient_c / coefficient_b);
            }
            return result;
        }

        let discriminant =
            coefficient_b.mul_add(coefficient_b, -4.0 * coefficient_a * coefficient_c);
        let discriminant_scale = coefficient_b
            .abs()
            .mul_add(
                coefficient_b.abs(),
                (4.0 * coefficient_a * coefficient_c).abs(),
            )
            .max(1.0);
        let discriminant_epsilon = discriminant_scale * RADIAL_EQUATION_RELATIVE_EPSILON;
        if discriminant < -discriminant_epsilon {
            return None;
        }
        let root = discriminant.max(0.0).sqrt();
        let denominator = 2.0 * coefficient_a;
        consider((-coefficient_b + root) / denominator);
        consider((-coefficient_b - root) / denominator);
        result
    }

    pub(crate) fn color_at(&self, x: f32, y: f32) -> Option<NativeColor> {
        let t = match self {
            Self::Linear {
                x0,
                y0,
                x1,
                y1,
                x2,
                y2,
                extend,
                ..
            } => {
                let dx = x1 - x0;
                let dy = y1 - y0;
                let projection_x = x2 - x0;
                let projection_y = y2 - y0;
                let denominator = dx.mul_add(projection_y, -(dy * projection_x));
                if !denominator.is_finite() || denominator.abs() <= f32::EPSILON {
                    0.0
                } else {
                    ((x - x0).mul_add(projection_y, -((y - y0) * projection_x)) / denominator)
                        .apply_gradient_extend(*extend)
                }
            }
            Self::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
                transform,
                extend,
                ..
            } => {
                let (local_x, local_y) = transform.inverse_point(x, y)?;
                Self::radial_parameter((local_x, local_y), (*x0, *y0, *r0), (*x1, *y1, *r1))?
                    .apply_gradient_extend(*extend)
            }
            Self::Sweep {
                center_x,
                center_y,
                start_angle,
                end_angle,
                transform,
                extend,
                ..
            } => {
                let (local_x, local_y) = transform.inverse_point(x, y)?;
                let angle = (local_y - center_y).atan2(local_x - center_x) / std::f32::consts::PI;
                let span = end_angle - start_angle;
                if !span.is_finite() || span.abs() <= f32::EPSILON {
                    0.0
                } else {
                    ((angle - start_angle) / span).apply_gradient_extend(*extend)
                }
            }
        };
        self.interpolate(t)
    }

    fn interpolate(&self, t: f32) -> Option<NativeColor> {
        let stops = match self {
            Self::Linear { stops, .. } | Self::Radial { stops, .. } | Self::Sweep { stops, .. } => {
                stops.as_ref()
            }
        };
        let first = stops.first()?;
        if stops.len() == 1 || t <= first.offset {
            return Some(first.color);
        }
        for pair in stops.windows(2) {
            let left = pair[0];
            let right = pair[1];
            if t <= right.offset {
                let span = right.offset - left.offset;
                let amount = if span > f32::EPSILON {
                    ((t - left.offset) / span).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                return Some(interpolate_native_color(left.color, right.color, amount));
            }
        }
        stops.last().map(|stop| stop.color)
    }
}

trait NativeGradientExtendValue {
    fn apply_gradient_extend(self, extend: NativeGradientExtend) -> f32;
}

impl NativeGradientExtendValue for f32 {
    fn apply_gradient_extend(self, extend: NativeGradientExtend) -> f32 {
        if !self.is_finite() {
            return 0.0;
        }
        match extend {
            NativeGradientExtend::Pad => self.clamp(0.0, 1.0),
            NativeGradientExtend::Repeat => self.rem_euclid(1.0),
            NativeGradientExtend::Reflect => {
                let period = self.rem_euclid(2.0);
                if period <= 1.0 { period } else { 2.0 - period }
            }
        }
    }
}

fn interpolate_native_color(left: NativeColor, right: NativeColor, amount: f32) -> NativeColor {
    let channel = |first: u8, second: u8| {
        (f32::from(first) + (f32::from(second) - f32::from(first)) * amount)
            .round()
            .clamp(0.0, f32::from(u8::MAX)) as u8
    };
    NativeColor {
        red: channel(left.red, right.red),
        green: channel(left.green, right.green),
        blue: channel(left.blue, right.blue),
        alpha: channel(left.alpha, right.alpha),
    }
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

#[derive(Debug, Clone, Copy)]
struct NativeOutlinePoint {
    x: f32,
    y: f32,
}

#[derive(Default)]
struct NativeOutlineBuilder {
    contours: Vec<Vec<NativeOutlinePoint>>,
    current: Vec<NativeOutlinePoint>,
    start: Option<NativeOutlinePoint>,
    previous: Option<NativeOutlinePoint>,
    invalid: bool,
}

impl NativeOutlineBuilder {
    fn finish_current(&mut self) {
        if self.current.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
        self.start = None;
        self.previous = None;
    }

    fn total_points(&self) -> usize {
        self.contours
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            .saturating_add(self.current.len())
    }

    fn push_point(&mut self, point: NativeOutlinePoint) {
        if !point.x.is_finite() || !point.y.is_finite() {
            self.invalid = true;
            return;
        }
        if self.total_points() >= MAX_VARIABLE_OUTLINE_POINTS {
            self.invalid = true;
            return;
        }
        if self
            .current
            .last()
            .is_some_and(|last| last.x == point.x && last.y == point.y)
        {
            return;
        }
        self.current.push(point);
        self.previous = Some(point);
    }

    fn begin(&mut self, point: NativeOutlinePoint) {
        self.finish_current();
        self.start = Some(point);
        self.push_point(point);
    }

    fn curve_steps(points: &[NativeOutlinePoint]) -> usize {
        let length = points
            .windows(2)
            .map(|pair| {
                let dx = pair[1].x - pair[0].x;
                let dy = pair[1].y - pair[0].y;
                dx.mul_add(dx, dy * dy).sqrt()
            })
            .sum::<f32>();
        if !length.is_finite() {
            return 0;
        }
        (length / 16.0).ceil().clamp(4.0, 64.0) as usize
    }

    fn finish(mut self) -> Option<Vec<Vec<NativeOutlinePoint>>> {
        self.finish_current();
        (!self.invalid && !self.contours.is_empty()).then_some(self.contours)
    }
}

impl ttf_parser::OutlineBuilder for NativeOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.begin(NativeOutlinePoint { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if self.start.is_none() {
            self.invalid = true;
            return;
        }
        self.push_point(NativeOutlinePoint { x, y });
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let Some(previous) = self.previous else {
            self.invalid = true;
            return;
        };
        let control = NativeOutlinePoint { x: x1, y: y1 };
        let end = NativeOutlinePoint { x, y };
        let steps = Self::curve_steps(&[previous, control, end]);
        if steps == 0 {
            self.invalid = true;
            return;
        }
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let inverse = 1.0 - t;
            self.push_point(NativeOutlinePoint {
                x: inverse.mul_add(
                    inverse.mul_add(previous.x, 2.0 * t * control.x),
                    t * t * end.x,
                ),
                y: inverse.mul_add(
                    inverse.mul_add(previous.y, 2.0 * t * control.y),
                    t * t * end.y,
                ),
            });
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let Some(previous) = self.previous else {
            self.invalid = true;
            return;
        };
        let first_control = NativeOutlinePoint { x: x1, y: y1 };
        let second_control = NativeOutlinePoint { x: x2, y: y2 };
        let end = NativeOutlinePoint { x, y };
        let steps = Self::curve_steps(&[previous, first_control, second_control, end]);
        if steps == 0 {
            self.invalid = true;
            return;
        }
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let inverse = 1.0 - t;
            let inverse_squared = inverse * inverse;
            let t_squared = t * t;
            self.push_point(NativeOutlinePoint {
                x: inverse_squared.mul_add(
                    inverse.mul_add(previous.x, 3.0 * t * first_control.x),
                    t_squared.mul_add(3.0 * inverse * second_control.x, t * end.x),
                ),
                y: inverse_squared.mul_add(
                    inverse.mul_add(previous.y, 3.0 * t * first_control.y),
                    t_squared.mul_add(3.0 * inverse * second_control.y, t * end.y),
                ),
            });
        }
    }

    fn close(&mut self) {
        if let Some(start) = self.start
            && self
                .previous
                .is_some_and(|previous| previous.x != start.x || previous.y != start.y)
        {
            self.push_point(start);
        }
        self.finish_current();
    }
}

#[derive(Clone)]
struct NativeFontFace {
    family: String,
    family_key: u64,
    generic_family: Option<NativeGenericFontFamily>,
    weight: NativeFontWeightRange,
    style: FontStyleValue,
    stretch: NativeFontStretchRange,
    variation_settings: NativeFontVariationSettings,
    feature_settings: NativeFontFeatureSettings,
    size_adjust: u16,
    metric_overrides: NativeFontMetricOverrides,
    font_display: NativeFontDisplay,
    font: Arc<OnceLock<Option<Arc<fontdue::Font>>>>,
    font_data: Arc<[u8]>,
    collection_index: u32,
    shaper_data: Arc<OnceLock<Option<Arc<harfrust::ShaperData>>>>,
    unicode_ranges: Arc<[NativeUnicodeRange]>,
}

impl NativeFontFace {
    fn font(&self) -> Option<&Arc<fontdue::Font>> {
        self.font
            .get_or_init(|| {
                let stage = crate::native_engine::content_process::ContentLoadStage::begin(
                    0,
                    "system_font_face_parse",
                );
                let font = fontdue::Font::from_bytes(
                    self.font_data.to_vec(),
                    fontdue::FontSettings {
                        collection_index: self.collection_index,
                        scale: FONT_PARSE_SCALE,
                        load_substitutions: true,
                    },
                )
                .ok()
                .map(Arc::new);
                stage.finish(None);
                font
            })
            .as_ref()
    }

    fn shaper_data(&self) -> Option<&Arc<harfrust::ShaperData>> {
        self.shaper_data
            .get_or_init(|| {
                let stage = crate::native_engine::content_process::ContentLoadStage::begin(
                    0,
                    "system_font_face_shaper_init",
                );
                let shaper_data = harfrust::FontRef::new(self.font_data.as_ref())
                    .ok()
                    .map(|font| Arc::new(harfrust::ShaperData::new(&font)));
                stage.finish(None);
                shaper_data
            })
            .as_ref()
    }
}

impl PartialEq for NativeFontFace {
    fn eq(&self, other: &Self) -> bool {
        self.family == other.family
            && self.family_key == other.family_key
            && self.generic_family == other.generic_family
            && self.weight == other.weight
            && self.style == other.style
            && self.stretch == other.stretch
            && self.variation_settings == other.variation_settings
            && self.feature_settings == other.feature_settings
            && self.size_adjust == other.size_adjust
            && self.metric_overrides == other.metric_overrides
            && self.font_display == other.font_display
            && self.collection_index == other.collection_index
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
            .field("stretch", &self.stretch)
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
    pub(crate) weight: NativeFontWeightRange,
    pub(crate) style: FontStyleValue,
    pub(crate) stretch: NativeFontStretchRange,
    pub(crate) variation_settings: NativeFontVariationSettings,
    pub(crate) feature_settings: NativeFontFeatureSettings,
    pub(crate) size_adjust: u16,
    pub(crate) metric_overrides: NativeFontMetricOverrides,
    pub(crate) font_display: NativeFontDisplay,
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
            stretch: rule.stretch,
            variation_settings: rule.variation_settings,
            feature_settings: rule.feature_settings,
            size_adjust: rule.size_adjust,
            metric_overrides: rule.metric_overrides,
            font_display: rule.font_display,
            bytes: Arc::from(bytes),
            unicode_ranges: rule.unicode_ranges.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeTextMetrics {
    faces: Vec<Arc<NativeFontFace>>,
    font_size: u32,
    weight: FontWeightValue,
    stretch: u16,
    ligatures: NativeFontVariantLigatures,
    variant_caps: NativeFontVariantCaps,
    variant_position: NativeFontVariantPosition,
    variant_alternates: NativeFontVariantAlternates,
    language_override: NativeFontLanguageOverride,
    variation_settings: NativeFontVariationSettings,
    variant_east_asian: NativeFontVariantEastAsian,
    variant_numeric: NativeFontVariantNumeric,
    feature_settings: NativeFontFeatureSettings,
    kerning: NativeFontKerning,
    optical_sizing: NativeFontOpticalSizing,
    palette_overrides: [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
    palette: NativeFontPalette,
    ascent: u32,
    line_height: u32,
    direction: DirectionValue,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeTextStyleOptions {
    pub(crate) stretch: u16,
    pub(crate) ligatures: NativeFontVariantLigatures,
    pub(crate) feature_settings: NativeFontFeatureSettings,
    pub(crate) kerning: NativeFontKerning,
    pub(crate) variant_caps: NativeFontVariantCaps,
    pub(crate) variant_position: NativeFontVariantPosition,
    pub(crate) variant_numeric: NativeFontVariantNumeric,
    pub(crate) variant_alternates: NativeFontVariantAlternates,
    pub(crate) variant_east_asian: NativeFontVariantEastAsian,
    pub(crate) language_override: NativeFontLanguageOverride,
    pub(crate) variation_settings: NativeFontVariationSettings,
    pub(crate) direction: DirectionValue,
}

impl Default for NativeTextStyleOptions {
    fn default() -> Self {
        Self {
            stretch: 1000,
            ligatures: NativeFontVariantLigatures::default(),
            feature_settings: NativeFontFeatureSettings::default(),
            kerning: NativeFontKerning::Auto,
            variant_caps: NativeFontVariantCaps::default(),
            variant_position: NativeFontVariantPosition::default(),
            variant_numeric: NativeFontVariantNumeric::default(),
            variant_alternates: NativeFontVariantAlternates::default(),
            variant_east_asian: NativeFontVariantEastAsian::default(),
            language_override: NativeFontLanguageOverride::default(),
            variation_settings: NativeFontVariationSettings::default(),
            direction: DirectionValue::Ltr,
        }
    }
}

fn push_feature_if_not_explicit(
    features: &mut Vec<harfrust::Feature>,
    settings: NativeFontFeatureSettings,
    tag: [u8; 4],
    value: u32,
) {
    if !settings.contains_tag(&tag) {
        features.push(harfrust::Feature::new(harfrust::Tag::new(&tag), value, ..));
    }
}

fn font_variant_caps_tags(caps: NativeFontVariantCaps) -> ([Option<[u8; 4]>; 2], usize) {
    match caps {
        NativeFontVariantCaps::Normal => ([None, None], 0),
        NativeFontVariantCaps::SmallCaps => ([Some(*b"smcp"), None], 1),
        NativeFontVariantCaps::AllSmallCaps => ([Some(*b"c2sc"), Some(*b"smcp")], 2),
        NativeFontVariantCaps::PetiteCaps => ([Some(*b"pcap"), None], 1),
        NativeFontVariantCaps::AllPetiteCaps => ([Some(*b"c2pc"), Some(*b"pcap")], 2),
        NativeFontVariantCaps::Unicase => ([Some(*b"unic"), None], 1),
        NativeFontVariantCaps::TitlingCaps => ([Some(*b"titl"), None], 1),
    }
}

fn font_variant_position_tag(position: NativeFontVariantPosition) -> Option<[u8; 4]> {
    match position {
        NativeFontVariantPosition::Normal => None,
        NativeFontVariantPosition::Sub => Some(*b"subs"),
        NativeFontVariantPosition::Super => Some(*b"sups"),
    }
}

fn font_variant_alternates_tag(alternates: NativeFontVariantAlternates) -> Option<[u8; 4]> {
    match alternates {
        NativeFontVariantAlternates::Normal => None,
        NativeFontVariantAlternates::HistoricalForms => Some(*b"hist"),
    }
}

fn font_language_override_language(
    language_override: NativeFontLanguageOverride,
) -> Option<harfrust::Language> {
    let tag = language_override.tag?;
    let end = tag
        .iter()
        .position(|byte| *byte == b' ')
        .unwrap_or(tag.len());
    if end == 0 || !tag[end..].iter().all(|byte| *byte == b' ') {
        return None;
    }
    harfrust::Language::new(&tag[..end])
}

fn font_variant_east_asian_tags(
    east_asian: NativeFontVariantEastAsian,
) -> ([Option<[u8; 4]>; 3], usize) {
    let mut tags = [None; 3];
    let mut count = 0;
    let mut push = |tag| {
        tags[count] = Some(tag);
        count += 1;
    };
    match east_asian.form {
        NativeFontVariantEastAsianForm::Normal => {}
        NativeFontVariantEastAsianForm::Jis78 => push(*b"jp78"),
        NativeFontVariantEastAsianForm::Jis83 => push(*b"jp83"),
        NativeFontVariantEastAsianForm::Jis90 => push(*b"jp90"),
        NativeFontVariantEastAsianForm::Jis04 => push(*b"jp04"),
        NativeFontVariantEastAsianForm::Simplified => push(*b"smpl"),
        NativeFontVariantEastAsianForm::Traditional => push(*b"trad"),
    }
    match east_asian.width {
        NativeFontVariantEastAsianWidth::Normal => {}
        NativeFontVariantEastAsianWidth::Full => push(*b"fwid"),
        NativeFontVariantEastAsianWidth::Proportional => push(*b"pwid"),
    }
    if east_asian.ruby {
        push(*b"ruby");
    }
    (tags, count)
}

fn font_variant_numeric_tags(numeric: NativeFontVariantNumeric) -> ([Option<[u8; 4]>; 8], usize) {
    let mut tags = [None; 8];
    let mut count = 0;
    let mut push = |tag| {
        tags[count] = Some(tag);
        count += 1;
    };
    match numeric.figure {
        NativeFontVariantNumericFigure::Normal => {}
        NativeFontVariantNumericFigure::Lining => push(*b"lnum"),
        NativeFontVariantNumericFigure::Oldstyle => push(*b"onum"),
    }
    match numeric.spacing {
        NativeFontVariantNumericSpacing::Normal => {}
        NativeFontVariantNumericSpacing::Proportional => push(*b"pnum"),
        NativeFontVariantNumericSpacing::Tabular => push(*b"tnum"),
    }
    match numeric.fraction {
        NativeFontVariantNumericFraction::Normal => {}
        NativeFontVariantNumericFraction::Diagonal => push(*b"frac"),
        NativeFontVariantNumericFraction::Stacked => push(*b"afrc"),
    }
    if numeric.ordinal {
        push(*b"ordn");
    }
    if numeric.slashed_zero {
        push(*b"zero");
    }
    (tags, count)
}

fn append_variation_if_missing(
    settings: &mut NativeFontVariationSettings,
    tag: [u8; 4],
    value_milli: i32,
) {
    if settings
        .values()
        .iter()
        .any(|variation| variation.tag == tag)
    {
        return;
    }
    let count = usize::from(settings.count);
    if count < settings.values.len() {
        settings.values[count] = NativeFontVariation { tag, value_milli };
        settings.count = settings.count.saturating_add(1);
    }
}

impl NativeTextMetrics {
    pub(crate) fn fallback_with_style_options(
        font_size: u32,
        options: NativeTextStyleOptions,
    ) -> Self {
        let NativeTextStyleOptions {
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            variant_alternates,
            variant_east_asian,
            language_override,
            variation_settings,
            direction,
        } = options;
        Self {
            faces: Vec::new(),
            font_size: font_size.clamp(1, MAX_NATIVE_FONT_SIZE),
            weight: FontWeightValue::Normal,
            stretch: stretch.clamp(500, 2000),
            ligatures,
            variant_caps,
            variant_position,
            variant_alternates,
            language_override,
            variation_settings,
            variant_east_asian,
            variant_numeric,
            feature_settings,
            kerning,
            optical_sizing: NativeFontOpticalSizing::Auto,
            palette: NativeFontPalette::Normal,
            palette_overrides: [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
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

    #[cfg(test)]
    pub(crate) fn for_style_with_book(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_options(
            families,
            font_size,
            weight,
            style,
            NativeTextStyleOptions {
                direction,
                ..NativeTextStyleOptions::default()
            },
            book,
        )
    }

    pub(crate) fn for_style_with_book_and_options(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        options: NativeTextStyleOptions,
        book: &NativeFontBook,
    ) -> Self {
        let NativeTextStyleOptions {
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            variant_alternates,
            variant_east_asian,
            language_override,
            variation_settings,
            direction,
        } = options;
        let font_size = font_size.clamp(1, MAX_NATIVE_FONT_SIZE);
        let stretch = stretch.clamp(500, 2000);
        let faces = book.faces_for(families, weight, style, stretch);
        let Some(face) = faces.first() else {
            return Self::fallback_with_style_options(
                font_size,
                NativeTextStyleOptions {
                    stretch,
                    ligatures,
                    feature_settings,
                    kerning,
                    variant_caps,
                    variant_position,
                    variant_numeric,
                    variant_alternates,
                    variant_east_asian,
                    language_override,
                    variation_settings,
                    direction,
                },
            );
        };
        let line_metrics = face.font().and_then(|font| {
            font.horizontal_line_metrics(adjusted_font_size(font_size, face.size_adjust) as f32)
        });
        let (ascent, line_height) = match line_metrics {
            Some(metrics) if face.metric_overrides == NativeFontMetricOverrides::default() => (
                ceil_positive(metrics.ascent),
                ceil_positive(metrics.new_line_size).max(1),
            ),
            Some(metrics) => {
                let metric_scale = adjusted_font_size(font_size, face.size_adjust) as f32;
                let ascent = face
                    .metric_overrides
                    .ascent
                    .map(|value| metric_scale * value as f32 / 1000.0)
                    .unwrap_or(metrics.ascent);
                let descent = face
                    .metric_overrides
                    .descent
                    .map(|value| -(metric_scale * value as f32 / 1000.0))
                    .unwrap_or(metrics.descent);
                let line_gap = face
                    .metric_overrides
                    .line_gap
                    .map(|value| metric_scale * value as f32 / 1000.0)
                    .unwrap_or(metrics.line_gap);
                (
                    ceil_positive(ascent),
                    ceil_positive(ascent - descent + line_gap).max(1),
                )
            }
            None => (FALLBACK_LINE_HEIGHT.saturating_sub(5), FALLBACK_LINE_HEIGHT),
        };
        Self {
            faces,
            font_size,
            weight,
            stretch,
            ligatures,
            variant_caps,
            variant_position,
            variant_alternates,
            language_override,
            variation_settings,
            variant_east_asian,
            variant_numeric,
            feature_settings,
            kerning,
            optical_sizing: NativeFontOpticalSizing::Auto,
            palette: NativeFontPalette::Normal,
            palette_overrides: [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
            ascent,
            line_height,
            direction,
        }
    }

    pub(crate) fn with_optical_sizing(mut self, optical_sizing: NativeFontOpticalSizing) -> Self {
        self.optical_sizing = optical_sizing;
        self
    }

    pub(crate) fn with_palette(mut self, palette: NativeFontPalette) -> Self {
        self.palette = palette;
        self
    }
    pub(crate) fn with_palette_overrides(
        mut self,
        palette_overrides: [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
    ) -> Self {
        self.palette_overrides = palette_overrides;
        self
    }

    #[cfg(test)]
    pub(crate) const fn palette(&self) -> NativeFontPalette {
        self.palette
    }
    #[cfg(test)]
    pub(crate) const fn palette_overrides(
        &self,
    ) -> [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES] {
        self.palette_overrides
    }

    pub(crate) const fn line_height(&self) -> u32 {
        self.line_height
    }

    pub(crate) fn advance(&self, character: char, letter_spacing: u32, word_spacing: u32) -> u32 {
        let base = self
            .face_index_for_character(character)
            .and_then(|face_index| self.faces.get(face_index))
            .map_or(
                scale_dimension(FALLBACK_GLYPH_ADVANCE, self.stretch, 1000),
                |face| {
                    let (_, nominal) = self.stretch_factor(face);
                    let font_size = adjusted_font_size(self.font_size, face.size_adjust);
                    face.font().map_or(FALLBACK_GLYPH_ADVANCE, |font| {
                        ceil_stretched(
                            font.metrics(character, font_size as f32).advance_width,
                            self.stretch,
                            nominal,
                        )
                    })
                },
            );
        base.saturating_add(letter_spacing)
            .saturating_add(if character == ' ' { word_spacing } else { 0 })
    }

    fn stretch_factor(&self, face: &NativeFontFace) -> (u16, u16) {
        (self.stretch, face.stretch.nominal())
    }

    fn effective_variation_settings(&self, face: &NativeFontFace) -> NativeFontVariationSettings {
        let mut settings = face
            .variation_settings
            .with_overrides(self.variation_settings);
        let Ok(parsed_face) = ttf_parser::Face::parse(face.font_data.as_ref(), 0) else {
            return settings;
        };
        for axis in parsed_face.variation_axes() {
            let tag = axis.tag.to_bytes();
            let value_milli = match &tag {
                b"wght" => Some(match self.weight {
                    FontWeightValue::Normal => 400_000,
                    FontWeightValue::Bold => 700_000,
                    FontWeightValue::Numeric(value) => i32::from(value).saturating_mul(1_000),
                }),
                b"wdth" => Some(i32::from(self.stretch).saturating_mul(100)),
                b"opsz" if matches!(self.optical_sizing, NativeFontOpticalSizing::Auto) => {
                    Some((self.font_size as i32).saturating_mul(1_000))
                }
                _ => None,
            };
            if let Some(value_milli) = value_milli {
                append_variation_if_missing(&mut settings, tag, value_milli);
            }
        }
        settings
    }
    fn effective_feature_settings(&self, face: &NativeFontFace) -> NativeFontFeatureSettings {
        face.feature_settings.with_overrides(self.feature_settings)
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
                let font_size = adjusted_font_size(self.font_size, face.size_adjust);
                let kerning = face
                    .font()
                    .and_then(|font| font.horizontal_kern(previous, character, font_size as f32))
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
        let shaper_data = face.shaper_data()?;
        let font = harfrust::FontRef::new(face.font_data.as_ref()).ok()?;
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(value);
        buffer.set_flags(
            harfrust::BufferFlags::BEGINNING_OF_TEXT | harfrust::BufferFlags::END_OF_TEXT,
        );
        buffer.guess_segment_properties();
        if let Some(language) = font_language_override_language(self.language_override) {
            buffer.set_language(language);
        }
        let direction = match self.direction {
            DirectionValue::Ltr => harfrust::Direction::LeftToRight,
            DirectionValue::Rtl => harfrust::Direction::RightToLeft,
        };
        buffer.set_direction(direction);
        let scale = i32::try_from(
            adjusted_font_size(self.font_size, face.size_adjust).saturating_mul(FONT_SHAPE_SCALE),
        )
        .ok()?;
        let variation_settings = self.effective_variation_settings(face);
        let variation_instance = if variation_settings.values().is_empty() {
            None
        } else {
            Some(harfrust::ShaperInstance::from_variations(
                &font,
                variation_settings
                    .values()
                    .iter()
                    .map(|variation| harfrust::Variation {
                        tag: harfrust::Tag::new(&variation.tag),
                        value: variation.value_milli as f32 / 1_000.0,
                    }),
            ))
        };
        let shaper = shaper_data
            .shaper(&font)
            .instance(variation_instance.as_ref())
            .build();
        let feature_settings = self.effective_feature_settings(face);
        let mut features = Vec::with_capacity(21 + feature_settings.values().len());
        for (tag, value) in [
            (*b"liga", u32::from(self.ligatures.common)),
            (*b"clig", u32::from(self.ligatures.common)),
            (*b"dlig", u32::from(self.ligatures.discretionary)),
            (*b"hlig", u32::from(self.ligatures.historical)),
            (*b"calt", u32::from(self.ligatures.contextual)),
        ] {
            if !feature_settings.contains_tag(&tag) {
                features.push(harfrust::Feature::new(harfrust::Tag::new(&tag), value, ..));
            }
        }
        if !matches!(self.kerning, NativeFontKerning::Auto)
            && !feature_settings.contains_tag(b"kern")
        {
            let value = u32::from(matches!(self.kerning, NativeFontKerning::Normal));
            features.push(harfrust::Feature::new(
                harfrust::Tag::new(b"kern"),
                value,
                ..,
            ));
        }
        let (caps_tags, caps_count) = font_variant_caps_tags(self.variant_caps);
        for tag in caps_tags.into_iter().take(caps_count).flatten() {
            push_feature_if_not_explicit(&mut features, feature_settings, tag, 1);
        }
        if let Some(tag) = font_variant_position_tag(self.variant_position) {
            push_feature_if_not_explicit(&mut features, feature_settings, tag, 1);
        }
        let (numeric_tags, numeric_count) = font_variant_numeric_tags(self.variant_numeric);
        for tag in numeric_tags.into_iter().take(numeric_count).flatten() {
            push_feature_if_not_explicit(&mut features, feature_settings, tag, 1);
        }
        if let Some(tag) = font_variant_alternates_tag(self.variant_alternates) {
            push_feature_if_not_explicit(&mut features, feature_settings, tag, 1);
        }
        let (east_asian_tags, east_asian_count) =
            font_variant_east_asian_tags(self.variant_east_asian);
        for tag in east_asian_tags.into_iter().take(east_asian_count).flatten() {
            push_feature_if_not_explicit(&mut features, feature_settings, tag, 1);
        }
        let explicit_features = feature_settings.values();
        for (index, feature) in explicit_features.iter().enumerate() {
            if explicit_features[index.saturating_add(1)..]
                .iter()
                .any(|later| later.tag == feature.tag)
            {
                continue;
            }
            features.push(harfrust::Feature::new(
                harfrust::Tag::new(&feature.tag),
                feature.value,
                ..,
            ));
        }
        let shaped = shaper.shape(
            buffer,
            harfrust::ShapeOptions::new()
                .scale(Some(scale))
                .features(&features),
        );
        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();
        if infos.len() != positions.len() {
            return None;
        }

        let characters: Vec<char> = value.chars().collect();
        let character_count = characters.len();
        let (requested_stretch, nominal_stretch) = self.stretch_factor(face);
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
            let x_advance = i32::try_from(scale_fixed(
                i64::from(position.x_advance),
                requested_stretch,
                nominal_stretch,
            ))
            .unwrap_or(i32::MAX);
            let x_offset = i32::try_from(scale_fixed(
                i64::from(position.x_offset),
                requested_stretch,
                nominal_stretch,
            ))
            .unwrap_or(if position.x_offset.is_negative() {
                i32::MIN
            } else {
                i32::MAX
            });
            glyphs.push(NativeShapedGlyph {
                glyph_id: info.glyph_id,
                x_offset,
                y_offset: position.y_offset,
                x_advance,
                cluster,
                cluster_spacing,
            });
            pen_x = pen_x
                .saturating_add(i64::from(x_advance))
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
        let (requested_stretch, nominal_stretch) = self.stretch_factor(face);
        let variation_settings = self.effective_variation_settings(face);
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
            if face
                .font()
                .is_none_or(|font| glyph_id >= font.glyph_count())
            {
                return None;
            }
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
            let glyph_x = match self.direction {
                DirectionValue::Ltr => pen_x.saturating_add(i64::from(shaped_glyph.x_offset)),
                DirectionValue::Rtl => total_width_fixed
                    .saturating_sub(pen_x)
                    .saturating_sub(advance_fixed)
                    .saturating_add(i64::from(shaped_glyph.x_offset)),
            };
            for rasterized in self.rasterize_glyph_layers(face, glyph_id, variation_settings)? {
                if rasterized.width == 0 || rasterized.height == 0 || rasterized.coverage.is_empty()
                {
                    continue;
                }
                let width = rasterized.width;
                let height = rasterized.height;
                let (width, coverage) = if rasterized.is_bitmap {
                    scale_coverage_horizontal_bounded(
                        rasterized.coverage,
                        width,
                        height,
                        requested_stretch,
                        nominal_stretch,
                    )
                } else {
                    scale_coverage_horizontal(
                        rasterized.coverage,
                        width,
                        height,
                        requested_stretch,
                        nominal_stretch,
                    )
                }?;
                let y = i32::try_from(self.ascent)
                    .ok()?
                    .saturating_sub(
                        i32::try_from(rasterized.height)
                            .ok()?
                            .saturating_add(rasterized.ymin),
                    )
                    .saturating_sub(round_signed_fixed(i64::from(shaped_glyph.y_offset)));
                glyphs.push(NativeGlyph {
                    x: round_signed_fixed(glyph_x).saturating_add(scale_signed(
                        rasterized.xmin,
                        requested_stretch,
                        nominal_stretch,
                    )),
                    y,
                    width,
                    height,
                    advance,
                    color: rasterized.color,
                    composite: rasterized.composite,
                    gradient: rasterized
                        .gradient
                        .clone()
                        .map(|gradient| gradient.scale_x(requested_stretch, nominal_stretch)),
                    bitmap: match rasterized.bitmap.as_deref() {
                        Some(bitmap) => Some(Arc::from(scale_bitmap_horizontal(
                            bitmap,
                            rasterized.width,
                            rasterized.height,
                            requested_stretch,
                            nominal_stretch,
                        )?)),
                        None => None,
                    },
                    coverage,
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

    fn rasterize_glyph_layers(
        &self,
        face: &NativeFontFace,
        glyph_id: u16,
        variation_settings: NativeFontVariationSettings,
    ) -> Option<Vec<NativeRasterizedGlyph>> {
        let font_size = adjusted_font_size(self.font_size, face.size_adjust);
        if let Some(rasterized) = rasterize_svg_glyph(face.font_data.as_ref(), glyph_id, font_size)
        {
            return Some(vec![rasterized]);
        }
        let palette_index = palette_index_for_face(face.font_data.as_ref(), self.palette);
        let palette_is_valid = !matches!(
            self.palette,
            NativeFontPalette::Base(index)
                if !palette_index_is_valid_for_face(face.font_data.as_ref(), index)
        );
        if palette_is_valid
            && let Some(layers) = rasterize_color_glyph_with_overrides(
                face.font_data.as_ref(),
                glyph_id,
                font_size,
                variation_settings,
                palette_index,
                self.palette_overrides,
            )
        {
            return Some(layers);
        }
        if let Some(rasterized) = rasterize_bitmap_glyph(
            face.font_data.as_ref(),
            face.collection_index,
            glyph_id,
            font_size,
        ) {
            let (requested_stretch, nominal_stretch) = self.stretch_factor(face);
            let stretched_width =
                scale_dimension(rasterized.width, requested_stretch, nominal_stretch);
            if stretched_width <= MAX_COLOR_BITMAP_DIMENSION as u32 {
                return Some(vec![rasterized]);
            }
        }
        if let Some(rasterized) = rasterize_variable_glyph(
            face.font_data.as_ref(),
            glyph_id,
            font_size,
            variation_settings,
        ) {
            return Some(vec![rasterized]);
        }
        let (metrics, coverage) = face.font()?.rasterize_indexed(glyph_id, font_size as f32);
        Some(vec![NativeRasterizedGlyph {
            xmin: metrics.xmin,
            ymin: metrics.ymin,
            width: u32::try_from(metrics.width).ok()?,
            height: u32::try_from(metrics.height).ok()?,
            color: None,
            composite: NativeGlyphComposite::SourceOver,
            gradient: None,
            is_bitmap: false,
            bitmap: None,
            coverage,
        }])
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
            let (requested_stretch, nominal_stretch) = self.stretch_factor(face);
            if previous_face_index == Some(face_index)
                && let Some(previous) = previous
            {
                let font_size = adjusted_font_size(self.font_size, face.size_adjust);
                let kerning = face
                    .font()
                    .and_then(|font| font.horizontal_kern(previous, character, font_size as f32))
                    .map_or(0, |value| {
                        round_stretched(value, requested_stretch, nominal_stretch)
                    });
                x = x.saturating_add(kerning);
            }
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
            for rasterized in self.rasterize_glyph_layers(
                face,
                face.font()?.lookup_glyph_index(character),
                self.effective_variation_settings(face),
            )? {
                if rasterized.width == 0 || rasterized.height == 0 || rasterized.coverage.is_empty()
                {
                    continue;
                }
                let width = rasterized.width;
                let height = rasterized.height;
                let (width, coverage) = if rasterized.is_bitmap {
                    scale_coverage_horizontal_bounded(
                        rasterized.coverage,
                        width,
                        height,
                        requested_stretch,
                        nominal_stretch,
                    )
                } else {
                    scale_coverage_horizontal(
                        rasterized.coverage,
                        width,
                        height,
                        requested_stretch,
                        nominal_stretch,
                    )
                }?;
                let y = i32::try_from(self.ascent).ok()?.saturating_sub(
                    i32::try_from(rasterized.height)
                        .ok()?
                        .saturating_add(rasterized.ymin),
                );
                glyphs.push(NativeGlyph {
                    x: x.saturating_add(scale_signed(
                        rasterized.xmin,
                        requested_stretch,
                        nominal_stretch,
                    )),
                    y,
                    width,
                    height,
                    advance,
                    color: rasterized.color,
                    composite: rasterized.composite,
                    gradient: rasterized
                        .gradient
                        .clone()
                        .map(|gradient| gradient.scale_x(requested_stretch, nominal_stretch)),
                    bitmap: match rasterized.bitmap.as_deref() {
                        Some(bitmap) => Some(Arc::from(scale_bitmap_horizontal(
                            bitmap,
                            rasterized.width,
                            rasterized.height,
                            requested_stretch,
                            nominal_stretch,
                        )?)),
                        None => None,
                    },
                    coverage,
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

fn rasterize_bitmap_glyph(
    font_data: &[u8],
    collection_index: u32,
    glyph_id: u16,
    font_size: u32,
) -> Option<NativeRasterizedGlyph> {
    let requested_ppem = u16::try_from(font_size).ok()?;
    if requested_ppem == 0 {
        return None;
    }
    let face = ttf_parser::Face::parse(font_data, collection_index).ok()?;
    let image = face.glyph_raster_image(ttf_parser::GlyphId(glyph_id), requested_ppem)?;
    let source_ppem = image.pixels_per_em;
    if source_ppem == 0 {
        return None;
    }
    let NativeDecodedBitmap {
        width,
        height,
        coverage,
        pixels: bitmap,
    } = decode_bitmap_glyph_image(image)?;
    let scaled_width = scale_dimension(width, requested_ppem, source_ppem);
    let scaled_height = scale_dimension(height, requested_ppem, source_ppem);
    if scaled_width == 0
        || scaled_height == 0
        || usize::try_from(scaled_width).ok()? > MAX_COLOR_BITMAP_DIMENSION
        || usize::try_from(scaled_height).ok()? > MAX_COLOR_BITMAP_DIMENSION
    {
        return None;
    }
    let scaled_coverage =
        scale_raster_bytes(&coverage, width, height, 1, scaled_width, scaled_height)?;
    let scaled_bitmap = match bitmap.as_deref() {
        Some(pixels) => Some(scale_raster_bytes(
            pixels,
            width,
            height,
            4,
            scaled_width,
            scaled_height,
        )?),
        None => None,
    };
    let scaled_bytes = usize::try_from(scaled_width)
        .ok()?
        .checked_mul(usize::try_from(scaled_height).ok()?)?
        .checked_mul(4)?;
    if scaled_bytes > MAX_COLOR_BITMAP_BYTES {
        return None;
    }
    Some(NativeRasterizedGlyph {
        xmin: scale_signed(i32::from(image.x), requested_ppem, source_ppem),
        ymin: scale_signed(i32::from(image.y), requested_ppem, source_ppem),
        width: scaled_width,
        height: scaled_height,
        color: None,
        composite: NativeGlyphComposite::SourceOver,
        gradient: None,
        is_bitmap: true,
        bitmap: scaled_bitmap,
        coverage: scaled_coverage,
    })
}
fn rasterize_svg_glyph(
    font_data: &[u8],
    glyph_id: u16,
    font_size: u32,
) -> Option<NativeRasterizedGlyph> {
    let face = ttf_parser::Face::parse(font_data, 0).ok()?;
    let svg_document = face.glyph_svg_image(ttf_parser::GlyphId(glyph_id))?;
    let (image, view_box) = super::image::decode_svg_font_glyph(
        svg_document.data,
        font_size,
        face.units_per_em(),
        MAX_COLOR_BITMAP_BYTES,
    )?;
    let pixel_count = usize::try_from(image.width)
        .ok()?
        .checked_mul(usize::try_from(image.height).ok()?)?;
    let expected_bytes = pixel_count.checked_mul(4)?;
    if image.pixels.len() != expected_bytes {
        return None;
    }
    let scale = f64::from(font_size) / f64::from(face.units_per_em());
    let xmin = round_signed((view_box[0] * scale) as f32);
    let ymin = round_signed((view_box[1] * scale) as f32);
    Some(NativeRasterizedGlyph {
        xmin,
        ymin,
        width: image.width,
        height: image.height,
        color: None,
        composite: NativeGlyphComposite::SourceOver,
        gradient: None,
        is_bitmap: true,
        coverage: image.pixels.chunks_exact(4).map(|pixel| pixel[3]).collect(),
        bitmap: Some(image.pixels),
    })
}

#[derive(Debug)]
struct NativeDecodedBitmap {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
    pixels: Option<Vec<u8>>,
}

fn decode_bitmap_glyph_image(
    image: ttf_parser::RasterGlyphImage<'_>,
) -> Option<NativeDecodedBitmap> {
    let width = u32::from(image.width);
    let height = u32::from(image.height);
    if width == 0
        || height == 0
        || usize::try_from(width).ok()? > MAX_COLOR_BITMAP_DIMENSION
        || usize::try_from(height).ok()? > MAX_COLOR_BITMAP_DIMENSION
    {
        return None;
    }
    match image.format {
        ttf_parser::RasterImageFormat::PNG => {
            let decoded = super::image::decode_png_bytes(image.data, MAX_COLOR_BITMAP_BYTES)?;
            let pixel_count = usize::try_from(decoded.width)
                .ok()?
                .checked_mul(usize::try_from(decoded.height).ok()?)?;
            if decoded.width == 0
                || decoded.height == 0
                || usize::try_from(decoded.width).ok()? > MAX_COLOR_BITMAP_DIMENSION
                || usize::try_from(decoded.height).ok()? > MAX_COLOR_BITMAP_DIMENSION
                || decoded.pixels.len() != pixel_count.checked_mul(4)?
            {
                return None;
            }
            let coverage = decoded
                .pixels
                .chunks_exact(4)
                .map(|pixel| pixel[3])
                .collect();
            Some(NativeDecodedBitmap {
                width: decoded.width,
                height: decoded.height,
                coverage,
                pixels: Some(decoded.pixels),
            })
        }
        ttf_parser::RasterImageFormat::BitmapPremulBgra32 => {
            let pixel_count = usize::try_from(width)
                .ok()?
                .checked_mul(usize::try_from(height).ok()?)?;
            if image.data.len() != pixel_count.checked_mul(4)? {
                return None;
            }
            let mut pixels = Vec::with_capacity(image.data.len());
            for bgra in image.data.chunks_exact(4) {
                let alpha = bgra[3];
                // A premultiplied channel cannot exceed its alpha. Reject
                // malformed pixels instead of promoting them to opaque color.
                if bgra[..3].iter().any(|channel| *channel > alpha) {
                    return None;
                }
                let alpha_u32 = u32::from(alpha);
                let unpremultiply = |channel: u8| {
                    if alpha_u32 == 0 {
                        0
                    } else {
                        let numerator = u32::from(channel)
                            .saturating_mul(u32::from(u8::MAX))
                            .saturating_add(alpha_u32 / 2);
                        u8::try_from(numerator.checked_div(alpha_u32).unwrap_or(u32::MAX))
                            .unwrap_or(u8::MAX)
                    }
                };
                pixels.extend_from_slice(&[
                    unpremultiply(bgra[2]),
                    unpremultiply(bgra[1]),
                    unpremultiply(bgra[0]),
                    bgra[3],
                ]);
            }
            let coverage = image.data.chunks_exact(4).map(|pixel| pixel[3]).collect();
            Some(NativeDecodedBitmap {
                width,
                height,
                coverage,
                pixels: Some(pixels),
            })
        }
        ttf_parser::RasterImageFormat::BitmapMono => Some(NativeDecodedBitmap {
            width,
            height,
            coverage: decode_bitmap_coverage(image.data, width, height, 1, true)?,
            pixels: None,
        }),
        ttf_parser::RasterImageFormat::BitmapMonoPacked => Some(NativeDecodedBitmap {
            width,
            height,
            coverage: decode_bitmap_coverage(image.data, width, height, 1, false)?,
            pixels: None,
        }),
        ttf_parser::RasterImageFormat::BitmapGray2 => Some(NativeDecodedBitmap {
            width,
            height,
            coverage: decode_bitmap_coverage(image.data, width, height, 2, true)?,
            pixels: None,
        }),
        ttf_parser::RasterImageFormat::BitmapGray2Packed => Some(NativeDecodedBitmap {
            width,
            height,
            coverage: decode_bitmap_coverage(image.data, width, height, 2, false)?,
            pixels: None,
        }),
        ttf_parser::RasterImageFormat::BitmapGray4 => Some(NativeDecodedBitmap {
            width,
            height,
            coverage: decode_bitmap_coverage(image.data, width, height, 4, true)?,
            pixels: None,
        }),
        ttf_parser::RasterImageFormat::BitmapGray4Packed => Some(NativeDecodedBitmap {
            width,
            height,
            coverage: decode_bitmap_coverage(image.data, width, height, 4, false)?,
            pixels: None,
        }),
        ttf_parser::RasterImageFormat::BitmapGray8 => Some(NativeDecodedBitmap {
            width,
            height,
            coverage: decode_bitmap_coverage(image.data, width, height, 8, true)?,
            pixels: None,
        }),
    }
}

fn decode_bitmap_coverage(
    data: &[u8],
    width: u32,
    height: u32,
    bits_per_pixel: u8,
    row_aligned: bool,
) -> Option<Vec<u8>> {
    if !matches!(bits_per_pixel, 1 | 2 | 4 | 8) {
        return None;
    }
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    if width == 0
        || height == 0
        || width > MAX_COLOR_BITMAP_DIMENSION
        || height > MAX_COLOR_BITMAP_DIMENSION
    {
        return None;
    }
    let pixel_count = width.checked_mul(height)?;
    if pixel_count > MAX_COLOR_BITMAP_BYTES {
        return None;
    }
    let bits_per_row = width.checked_mul(usize::from(bits_per_pixel))?;
    let row_bytes = bits_per_row.checked_add(7)?.checked_div(8)?;
    let expected_bytes = if row_aligned {
        row_bytes.checked_mul(height)?
    } else {
        bits_per_row
            .checked_mul(height)?
            .checked_add(7)?
            .checked_div(8)?
    };
    if expected_bytes > MAX_COLOR_BITMAP_BYTES || data.len() != expected_bytes {
        return None;
    }
    let mask = (1_u32 << bits_per_pixel) - 1;
    let mut coverage = Vec::with_capacity(pixel_count);
    for row in 0..height {
        let row_bit_offset = if row_aligned {
            row.checked_mul(row_bytes)?.checked_mul(8)?
        } else {
            row.checked_mul(bits_per_row)?
        };
        for column in 0..width {
            let bit_offset =
                row_bit_offset.checked_add(column.checked_mul(usize::from(bits_per_pixel))?)?;
            let byte = *data.get(bit_offset / 8)?;
            let shift = 8usize
                .checked_sub(usize::from(bits_per_pixel))?
                .checked_sub(bit_offset % 8)?;
            let value = (u32::from(byte) >> shift) & mask;
            let alpha = value
                .saturating_mul(u32::from(u8::MAX))
                .saturating_add(mask / 2)
                / mask;
            coverage.push(u8::try_from(alpha).unwrap_or(u8::MAX));
        }
    }
    Some(coverage)
}

fn rasterize_variable_glyph(
    font_data: &[u8],
    glyph_id: u16,
    font_size: u32,
    variation_settings: NativeFontVariationSettings,
) -> Option<NativeRasterizedGlyph> {
    if variation_settings.values().is_empty() {
        return None;
    }
    let mut face = ttf_parser::Face::parse(font_data, 0).ok()?;
    if face.variation_axes().is_empty() {
        return None;
    }
    for variation in variation_settings.values() {
        let _ = face.set_variation(
            ttf_parser::Tag::from_bytes(&variation.tag),
            variation.value_milli as f32 / 1_000.0,
        );
    }
    let mut builder = NativeOutlineBuilder::default();
    let _bbox = face.outline_glyph(ttf_parser::GlyphId(glyph_id), &mut builder)?;
    let contours = builder.finish()?;
    let units_per_em = f32::from(face.units_per_em());
    let mut rasterized = rasterize_outline_contours(&contours, font_size, units_per_em, None)?;
    rasterized.color = None;
    rasterized.composite = NativeGlyphComposite::SourceOver;
    rasterized.gradient = None;
    Some(rasterized)
}

const CPAL_USABLE_WITH_LIGHT_BACKGROUND: u32 = 0x0000_0001;
const CPAL_USABLE_WITH_DARK_BACKGROUND: u32 = 0x0000_0002;

fn read_be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes
            .get(offset..offset.saturating_add(2))?
            .try_into()
            .ok()?,
    ))
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes
            .get(offset..offset.saturating_add(4))?
            .try_into()
            .ok()?,
    ))
}

fn palette_index_is_valid_for_face(font_data: &[u8], palette_index: u16) -> bool {
    ttf_parser::Face::parse(font_data, 0)
        .ok()
        .and_then(|face| face.color_palettes())
        .is_some_and(|palette_count| palette_index < palette_count.get())
}

fn palette_index_for_face(font_data: &[u8], palette: NativeFontPalette) -> u16 {
    let base_palette = match palette {
        NativeFontPalette::Base(index) => Some(index),
        NativeFontPalette::Normal
        | NativeFontPalette::Named(_)
        | NativeFontPalette::Light
        | NativeFontPalette::Dark => None,
    };
    let required_flags = match palette {
        NativeFontPalette::Normal | NativeFontPalette::Named(_) | NativeFontPalette::Base(_) => {
            None
        }
        NativeFontPalette::Light => Some(CPAL_USABLE_WITH_LIGHT_BACKGROUND),
        NativeFontPalette::Dark => Some(CPAL_USABLE_WITH_DARK_BACKGROUND),
    };
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return 0;
    };
    let Some(palette_count) = face.color_palettes().map(|count| usize::from(count.get())) else {
        return 0;
    };
    if let Some(index) = base_palette {
        return if usize::from(index) < palette_count {
            index
        } else {
            0
        };
    }
    let Some(required_flags) = required_flags else {
        return 0;
    };
    let Some(cpal) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"CPAL")) else {
        return 0;
    };
    if read_be_u16(cpal, 0).is_none_or(|version| version < 1) {
        return 0;
    }
    let Some(palette_total) = read_be_u16(cpal, 4).map(usize::from) else {
        return 0;
    };
    let indices_end = 12usize.saturating_add(palette_total.saturating_mul(2));
    let Some(flags_offset) = read_be_u32(cpal, indices_end).map(|offset| offset as usize) else {
        return 0;
    };
    for palette_index in 0..palette_count.min(palette_total) {
        let Some(flags) = read_be_u32(cpal, flags_offset.saturating_add(palette_index * 4)) else {
            break;
        };
        if flags & required_flags != 0 {
            return u16::try_from(palette_index).unwrap_or(0);
        }
    }
    0
}

fn cpal_palette_colors(cpal: &[u8], palette_index: u16) -> Option<Vec<NativeColor>> {
    let palette_entries = usize::from(read_be_u16(cpal, 2)?);
    let palette_count = usize::from(read_be_u16(cpal, 4)?);
    let color_record_count = usize::from(read_be_u16(cpal, 6)?);
    if usize::from(palette_index) >= palette_count {
        return None;
    }
    let color_records_offset = usize::try_from(read_be_u32(cpal, 8)?).ok()?;
    let palette_indices_offset = 12usize.checked_add(usize::from(palette_index).checked_mul(2)?)?;
    let first_color_record = usize::from(read_be_u16(cpal, palette_indices_offset)?);
    let last_color_record = first_color_record.checked_add(palette_entries)?;
    if last_color_record > color_record_count {
        return None;
    }
    let mut colors = Vec::with_capacity(palette_entries);
    for color_record in first_color_record..last_color_record {
        let offset = color_records_offset.checked_add(color_record.checked_mul(4)?)?;
        colors.push(NativeColor {
            blue: *cpal.get(offset)?,
            green: *cpal.get(offset.checked_add(1)?)?,
            red: *cpal.get(offset.checked_add(2)?)?,
            alpha: *cpal.get(offset.checked_add(3)?)?,
        });
    }
    Some(colors)
}

fn palette_colors_for_face(font_data: &[u8], palette_index: u16) -> Option<Vec<NativeColor>> {
    let face = ttf_parser::Face::parse(font_data, 0).ok()?;
    let cpal = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CPAL"))?;
    cpal_palette_colors(cpal, palette_index)
}

fn palette_overrides_are_unambiguous(
    palette_colors: &[NativeColor],
    palette_overrides: &[Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
    foreground_color: NativeColor,
) -> bool {
    palette_overrides.iter().flatten().all(|palette_override| {
        let Some(source_color) = palette_colors.get(usize::from(palette_override.palette_index))
        else {
            return false;
        };
        *source_color != foreground_color
            && palette_colors
                .iter()
                .filter(|candidate| **candidate == *source_color)
                .count()
                == 1
    })
}

#[cfg(test)]
fn rasterize_color_glyph(
    font_data: &[u8],
    glyph_id: u16,
    font_size: u32,
    variation_settings: NativeFontVariationSettings,
    palette_index: u16,
) -> Option<Vec<NativeRasterizedGlyph>> {
    rasterize_color_glyph_with_overrides(
        font_data,
        glyph_id,
        font_size,
        variation_settings,
        palette_index,
        [None; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
    )
}

fn rasterize_color_glyph_with_overrides(
    font_data: &[u8],
    glyph_id: u16,
    font_size: u32,
    variation_settings: NativeFontVariationSettings,
    palette_index: u16,
    palette_overrides: [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
) -> Option<Vec<NativeRasterizedGlyph>> {
    let mut face = ttf_parser::Face::parse(font_data, 0).ok()?;
    for variation in variation_settings.values() {
        let _ = face.set_variation(
            ttf_parser::Tag::from_bytes(&variation.tag),
            variation.value_milli as f32 / 1_000.0,
        );
    }
    let glyph_id = ttf_parser::GlyphId(glyph_id);
    if !face.is_color_glyph(glyph_id) || palette_index >= face.color_palettes()?.get() {
        return None;
    }
    let foreground_color = NativeColor {
        red: 0,
        green: 0,
        blue: 0,
        alpha: u8::MAX,
    };
    let palette_colors = if palette_overrides.iter().any(Option::is_some) {
        let palette_colors = palette_colors_for_face(font_data, palette_index)?;
        if !palette_overrides_are_unambiguous(&palette_colors, &palette_overrides, foreground_color)
        {
            return None;
        }
        palette_colors
    } else {
        Vec::new()
    };
    let mut painter = NativeColorPainter::new(
        face.clone(),
        palette_colors,
        palette_overrides,
        foreground_color,
    );
    face.paint_color_glyph(
        glyph_id,
        palette_index,
        ttf_parser::RgbaColor::new(0, 0, 0, u8::MAX),
        &mut painter,
    )?;
    if painter.unsupported || !painter.is_balanced() || painter.layers.is_empty() {
        return None;
    }
    let units_per_em = f32::from(face.units_per_em());
    painter
        .layers
        .into_iter()
        .map(|layer| {
            let mut rasterized = rasterize_outline_contours(
                &layer.contours,
                font_size,
                units_per_em,
                layer.clip_mask,
            )?;
            rasterized.color = match &layer.paint {
                NativeColorPaint::Solid(color) => Some(*color),
                NativeColorPaint::Gradient(_) => None,
            };
            rasterized.gradient = match &layer.paint {
                NativeColorPaint::Solid(_) => None,
                NativeColorPaint::Gradient(_) => {
                    let ymax = rasterized
                        .ymin
                        .saturating_add(i32::try_from(rasterized.height).ok()?);
                    let scale = font_size as f32 / units_per_em;
                    Some(layer.paint.glyph_gradient(scale, rasterized.xmin, ymax)?)
                }
            };
            rasterized.composite = layer.composite;
            Some(rasterized)
        })
        .collect()
}

fn rasterize_outline_contours(
    contours: &[Vec<NativeOutlinePoint>],
    font_size: u32,
    units_per_em: f32,
    clip_mask: Option<NativeColorClipMask>,
) -> Option<NativeRasterizedGlyph> {
    if units_per_em <= 0.0 {
        return None;
    }
    let scale = font_size as f32 / units_per_em;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let mut points = contours.iter().flat_map(|contour| contour.iter()).copied();
    let first = points.next()?;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x, first.x, first.y, first.y);
    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    let xmin = bounded_floor_i32(min_x * scale)?;
    let xmax = bounded_ceil_i32(max_x * scale)?;
    let ymin = bounded_floor_i32(min_y * scale)?;
    let ymax = bounded_ceil_i32(max_y * scale)?;
    let width = usize::try_from(i64::from(xmax).saturating_sub(i64::from(xmin))).ok()?;
    let height = usize::try_from(i64::from(ymax).saturating_sub(i64::from(ymin))).ok()?;
    if width == 0
        || height == 0
        || width > MAX_VARIABLE_RASTER_DIMENSION
        || height > MAX_VARIABLE_RASTER_DIMENSION
    {
        return None;
    }
    let pixel_count = width.checked_mul(height)?;
    let sample_side = usize::try_from(VARIABLE_RASTER_SAMPLES).ok()?;
    let sample_count = sample_side.checked_mul(sample_side)?;
    let mut coverage = vec![0_u8; pixel_count];
    let top = ymax as f32;
    for row in 0..height {
        for column in 0..width {
            let mut hits = 0usize;
            for sample_y in 0..sample_side {
                let y = (top - row as f32 - (sample_y as f32 + 0.5) / sample_side as f32) / scale;
                for sample_x in 0..sample_side {
                    let x = (xmin as f32
                        + column as f32
                        + (sample_x as f32 + 0.5) / sample_side as f32)
                        / scale;
                    if clip_mask.as_ref().is_none_or(|clip| clip.contains(x, y))
                        && outline_contains(contours, x, y)
                    {
                        hits = hits.saturating_add(1);
                    }
                }
            }
            let value = hits.saturating_mul(255).saturating_add(sample_count / 2) / sample_count;
            coverage[row * width + column] = u8::try_from(value).unwrap_or(u8::MAX);
        }
    }
    Some(NativeRasterizedGlyph {
        xmin,
        ymin,
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        color: None,
        is_bitmap: false,
        composite: NativeGlyphComposite::SourceOver,
        gradient: None,
        bitmap: None,
        coverage,
    })
}

fn outline_contains(contours: &[Vec<NativeOutlinePoint>], x: f32, y: f32) -> bool {
    let mut winding = 0i32;
    for contour in contours {
        if contour.len() < 3 {
            continue;
        }
        for index in 0..contour.len() {
            let start = contour[index];
            let end = contour[(index + 1) % contour.len()];
            let is_left = (end.x - start.x) * (y - start.y) - (x - start.x) * (end.y - start.y);
            if start.y <= y {
                if end.y > y && is_left > 0.0 {
                    winding = winding.saturating_add(1);
                }
            } else if end.y <= y && is_left < 0.0 {
                winding = winding.saturating_sub(1);
            }
        }
    }
    winding != 0
}

fn bounded_floor_i32(value: f32) -> Option<i32> {
    let value = value.floor();
    (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
        .then_some(value as i32)
}

fn bounded_ceil_i32(value: f32) -> Option<i32> {
    let value = value.ceil();
    (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
        .then_some(value as i32)
}

impl NativeFontBook {
    pub(crate) fn system() -> Self {
        system_font_book().clone()
    }
    pub(crate) fn system_local_font_bytes(
        family: &str,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
    ) -> Option<Vec<u8>> {
        #[cfg(test)]
        if let Some(bytes) = Self::test_local_font_bytes(family) {
            return Some(bytes);
        }
        system_font_book().local_font_bytes_with_stretch(family, weight, style, stretch)
    }

    #[cfg(test)]
    pub(crate) fn set_test_local_font_source(family: &str, bytes: Option<Vec<u8>>) {
        let source = TEST_LOCAL_FONT_SOURCE.get_or_init(Default::default);
        *source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            bytes.map(|bytes| (family.to_ascii_lowercase(), bytes));
    }

    #[cfg(test)]
    fn test_local_font_bytes(family: &str) -> Option<Vec<u8>> {
        TEST_LOCAL_FONT_SOURCE
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|(name, _)| name.eq_ignore_ascii_case(family))
            .map(|(_, bytes)| bytes.clone())
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
        self.local_font_bytes_with_stretch(family, weight, style, 1000)
    }

    pub(crate) fn local_font_bytes_with_stretch(
        &self,
        family: &str,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
    ) -> Option<Vec<u8>> {
        let mut best = None;
        let mut best_score = u16::MAX;
        for face in &self.faces {
            if !face.family.eq_ignore_ascii_case(family) {
                continue;
            }
            let score = face_score(face, weight, style, stretch);
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
            if ttf_parser::Face::parse(&font_data, 0).is_err() {
                continue;
            }
            let font_data: Arc<[u8]> = Arc::from(font_data);
            book.faces.push(NativeFontFace {
                family: resource.family.clone(),
                family_key: resource.family_key,
                generic_family: None,
                weight: resource.weight,
                style: resource.style,
                stretch: resource.stretch,
                variation_settings: resource.variation_settings,
                feature_settings: resource.feature_settings,
                size_adjust: resource.size_adjust,
                metric_overrides: resource.metric_overrides,
                font_display: resource.font_display,
                font: Arc::new(OnceLock::new()),
                font_data,
                collection_index: 0,
                shaper_data: Arc::new(OnceLock::new()),
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
        stretch: u16,
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
                .map(|face| face_score(face, weight, style, stretch))
                .min()
            else {
                continue;
            };
            let best = matching
                .into_iter()
                .filter(|face| face_score(face, weight, style, stretch) == best_score)
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

#[cfg(test)]
#[path = "font/tests.rs"]
mod tests;

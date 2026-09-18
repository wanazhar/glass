#[cfg(test)]
use super::css::NativeFontFeature;
use super::css::NativeFontVariation;
use super::css::{
    DirectionValue, FontStyleValue, FontWeightValue, NativeColor, NativeFontFaceRule,
    NativeFontFamilyList, NativeFontFamilyValue, NativeFontFeatureSettings, NativeFontKerning,
    NativeFontLanguageOverride, NativeFontOpticalSizing, NativeFontStretchRange,
    NativeFontVariantAlternates, NativeFontVariantCaps, NativeFontVariantEastAsian,
    NativeFontVariantEastAsianForm, NativeFontVariantEastAsianWidth, NativeFontVariantLigatures,
    NativeFontVariantNumeric, NativeFontVariantNumericFigure, NativeFontVariantNumericFraction,
    NativeFontVariantNumericSpacing, NativeFontVariantPosition, NativeFontVariationSettings,
    NativeFontWeightRange, NativeGenericFontFamily, NativeUnicodeRange, font_family_hash,
};
use std::fmt;
use std::io::{self, Cursor, Read, Write};
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
                    extend: left_extend,
                    stops: left_stops,
                },
                Self::Sweep {
                    center_x: right_center_x,
                    center_y: right_center_y,
                    start_angle: right_start,
                    end_angle: right_end,
                    extend: right_extend,
                    stops: right_stops,
                },
            ) => {
                left_center_x.to_bits() == right_center_x.to_bits()
                    && left_center_y.to_bits() == right_center_y.to_bits()
                    && left_start.to_bits() == right_start.to_bits()
                    && left_end.to_bits() == right_end.to_bits()
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
                extend,
                stops,
            } => Self::Sweep {
                center_x: scale_x(*center_x),
                center_y: *center_y,
                start_angle: *start_angle,
                end_angle: *end_angle,
                extend: *extend,
                stops: stops.clone(),
            },
        }
    }

    fn radial_parameter(
        x: f32,
        y: f32,
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
    ) -> Option<f32> {
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
                Self::radial_parameter(local_x, local_y, *x0, *y0, *r0, *x1, *y1, *r1)?
                    .apply_gradient_extend(*extend)
            }
            Self::Sweep {
                center_x,
                center_y,
                start_angle,
                end_angle,
                extend,
                ..
            } => {
                let angle = (y - center_y).atan2(x - center_x) / std::f32::consts::PI;
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

#[derive(Debug, Clone)]
struct NativeColorLayer {
    contours: Vec<Vec<NativeOutlinePoint>>,
    paint: NativeColorPaint,
    composite: NativeGlyphComposite,
    clip_box: Option<NativeColorClipBox>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NativeColorPaint {
    Solid(NativeColor),
    Gradient(NativeGlyphGradient),
}

impl NativeColorPaint {
    fn glyph_gradient(&self, scale: f32, xmin: i32, ymax: i32) -> Option<NativeGlyphGradient> {
        let NativeColorPaint::Gradient(gradient) = self else {
            return None;
        };
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let map_x = |value: f32| value.mul_add(scale, -(xmin as f32));
        let pixel_transform = NativeGradientTransform {
            a: scale,
            b: 0.0,
            c: 0.0,
            d: -scale,
            e: -(xmin as f32),
            f: ymax as f32,
        };
        let map_y = |value: f32| (ymax as f32) - value * scale;
        let mapped = match gradient {
            NativeGlyphGradient::Linear {
                x0,
                y0,
                x1,
                y1,
                x2,
                y2,
                extend,
                stops,
            } => NativeGlyphGradient::Linear {
                x0: map_x(*x0),
                y0: map_y(*y0),
                x1: map_x(*x1),
                y1: map_y(*y1),
                x2: map_x(*x2),
                y2: map_y(*y2),
                extend: *extend,
                stops: stops.clone(),
            },
            NativeGlyphGradient::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
                transform,
                extend,
                stops,
            } => NativeGlyphGradient::Radial {
                x0: *x0,
                y0: *y0,
                r0: *r0,
                x1: *x1,
                y1: *y1,
                r1: *r1,
                transform: pixel_transform.compose(*transform),
                extend: *extend,
                stops: stops.clone(),
            },
            NativeGlyphGradient::Sweep {
                center_x,
                center_y,
                start_angle,
                end_angle,
                extend,
                stops,
            } => NativeGlyphGradient::Sweep {
                center_x: map_x(*center_x),
                center_y: map_y(*center_y),
                start_angle: *start_angle,
                end_angle: *end_angle,
                extend: *extend,
                stops: stops.clone(),
            },
        };
        Some(mapped)
    }
}

#[derive(Debug, Clone, Copy)]
struct NativeColorClipBox {
    x_min: f32,
    y_min: f32,
    x_max: f32,
    y_max: f32,
}

impl NativeColorClipBox {
    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x_min && x <= self.x_max && y >= self.y_min && y <= self.y_max
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let result = Self {
            x_min: self.x_min.max(other.x_min),
            y_min: self.y_min.max(other.y_min),
            x_max: self.x_max.min(other.x_max),
            y_max: self.y_max.min(other.y_max),
        };
        (result.x_min <= result.x_max && result.y_min <= result.y_max).then_some(result)
    }
}

#[derive(Debug, Clone, Copy)]
enum NativeColorClip {
    Outline(u64),
    Box(NativeColorClipBox),
}

struct NativeColorPainter<'a> {
    face: ttf_parser::Face<'a>,
    current: Option<Vec<Vec<NativeOutlinePoint>>>,
    layers: Vec<NativeColorLayer>,
    transform: ttf_parser::Transform,
    transform_stack: Vec<ttf_parser::Transform>,
    outline_generation: u64,
    clip_stack: Vec<NativeColorClip>,
    layer_stack: Vec<NativeGlyphComposite>,
    unsupported: bool,
}

impl<'a> NativeColorPainter<'a> {
    fn new(face: ttf_parser::Face<'a>) -> Self {
        Self {
            face,
            current: None,
            layers: Vec::new(),
            transform: ttf_parser::Transform::default(),
            transform_stack: Vec::new(),
            outline_generation: 0,
            clip_stack: Vec::new(),
            layer_stack: Vec::new(),
            unsupported: false,
        }
    }

    fn native_color(color: ttf_parser::RgbaColor) -> NativeColor {
        NativeColor {
            red: color.red,
            green: color.green,
            blue: color.blue,
            alpha: color.alpha,
        }
    }

    fn mark_unsupported(&mut self) {
        self.unsupported = true;
        self.current = None;
    }

    fn transform_is_bounded(transform: ttf_parser::Transform) -> bool {
        [
            transform.a,
            transform.b,
            transform.c,
            transform.d,
            transform.e,
            transform.f,
        ]
        .into_iter()
        .all(|component| component.is_finite() && component.abs() <= MAX_COLOR_TRANSFORM_COMPONENT)
    }

    fn is_balanced(&self) -> bool {
        self.current.is_none()
            && self.transform_stack.is_empty()
            && self.clip_stack.is_empty()
            && self.layer_stack.is_empty()
    }

    fn native_stops<I>(stops: I) -> Option<Arc<[NativeGradientStop]>>
    where
        I: Iterator<Item = ttf_parser::colr::ColorStop>,
    {
        let mut native_stops = Vec::new();
        for stop in stops {
            if native_stops.len() >= MAX_COLOR_STOPS
                || !stop.stop_offset.is_finite()
                || !(0.0..=1.0).contains(&stop.stop_offset)
            {
                return None;
            }
            native_stops.push(NativeGradientStop {
                offset: stop.stop_offset,
                color: Self::native_color(stop.color),
            });
        }
        if native_stops.is_empty() {
            return None;
        }
        native_stops.sort_by(|left, right| {
            left.offset
                .partial_cmp(&right.offset)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Some(Arc::from(native_stops))
    }

    fn native_extend(extend: ttf_parser::colr::GradientExtend) -> NativeGradientExtend {
        match extend {
            ttf_parser::colr::GradientExtend::Pad => NativeGradientExtend::Pad,
            ttf_parser::colr::GradientExtend::Repeat => NativeGradientExtend::Repeat,
            ttf_parser::colr::GradientExtend::Reflect => NativeGradientExtend::Reflect,
        }
    }

    fn finite_component(value: f32) -> bool {
        value.is_finite() && value.abs() <= MAX_COLOR_TRANSFORM_COMPONENT
    }

    fn transformed_point(transform: ttf_parser::Transform, x: f32, y: f32) -> Option<(f32, f32)> {
        let transformed_x = transform.a.mul_add(x, transform.c.mul_add(y, transform.e));
        let transformed_y = transform.b.mul_add(x, transform.d.mul_add(y, transform.f));
        (Self::finite_component(transformed_x) && Self::finite_component(transformed_y))
            .then_some((transformed_x, transformed_y))
    }

    fn conformal_scale(transform: ttf_parser::Transform) -> Option<(f32, bool)> {
        let first_length = transform.a.mul_add(transform.a, transform.b * transform.b);
        let second_length = transform.c.mul_add(transform.c, transform.d * transform.d);
        let dot = transform.a.mul_add(transform.c, transform.b * transform.d);
        let determinant = transform.a * transform.d - transform.b * transform.c;
        let tolerance = first_length.max(second_length).max(1.0) * 0.0001;
        if !first_length.is_finite()
            || !second_length.is_finite()
            || !dot.is_finite()
            || !determinant.is_finite()
            || first_length <= f32::EPSILON
            || (first_length - second_length).abs() > tolerance
            || dot.abs() > tolerance
            || determinant.abs() <= f32::EPSILON
        {
            return None;
        }
        let scale = first_length.sqrt();
        Self::finite_component(scale).then_some((scale, determinant >= 0.0))
    }

    fn linear_gradient(
        &self,
        gradient: ttf_parser::colr::LinearGradient<'a>,
    ) -> Option<NativeGlyphGradient> {
        if ![
            gradient.x0,
            gradient.y0,
            gradient.x1,
            gradient.y1,
            gradient.x2,
            gradient.y2,
        ]
        .into_iter()
        .all(Self::finite_component)
        {
            return None;
        }
        let (x0, y0) = Self::transformed_point(self.transform, gradient.x0, gradient.y0)?;
        let (x1, y1) = Self::transformed_point(self.transform, gradient.x1, gradient.y1)?;
        let (x2, y2) = Self::transformed_point(self.transform, gradient.x2, gradient.y2)?;
        let denominator = (x1 - x0).mul_add(y2 - y0, -((y1 - y0) * (x2 - x0)));
        if !denominator.is_finite() || denominator.abs() <= f32::EPSILON {
            return None;
        }
        Some(NativeGlyphGradient::Linear {
            x0,
            y0,
            x1,
            y1,
            x2,
            y2,
            extend: Self::native_extend(gradient.extend),
            stops: Self::native_stops(gradient.stops(0, self.face.variation_coordinates()))?,
        })
    }

    fn radial_gradient(
        &self,
        gradient: ttf_parser::colr::RadialGradient<'a>,
    ) -> Option<NativeGlyphGradient> {
        if ![
            gradient.x0,
            gradient.y0,
            gradient.r0,
            gradient.x1,
            gradient.y1,
            gradient.r1,
        ]
        .into_iter()
        .all(Self::finite_component)
            || gradient.r0 < 0.0
            || gradient.r1 <= 0.0
        {
            return None;
        }
        let transform = NativeGradientTransform::from_ttf(self.transform);
        if !transform.is_invertible() {
            return None;
        }
        Some(NativeGlyphGradient::Radial {
            x0: gradient.x0,
            y0: gradient.y0,
            r0: gradient.r0,
            x1: gradient.x1,
            y1: gradient.y1,
            r1: gradient.r1,
            transform,
            extend: Self::native_extend(gradient.extend),
            stops: Self::native_stops(gradient.stops(0, self.face.variation_coordinates()))?,
        })
    }

    fn sweep_gradient(
        &self,
        gradient: ttf_parser::colr::SweepGradient<'a>,
    ) -> Option<NativeGlyphGradient> {
        if ![
            gradient.center_x,
            gradient.center_y,
            gradient.start_angle,
            gradient.end_angle,
        ]
        .into_iter()
        .all(Self::finite_component)
            || (gradient.end_angle - gradient.start_angle).abs() <= f32::EPSILON
        {
            return None;
        }
        let (_, preserves_orientation) = Self::conformal_scale(self.transform)?;
        let (center_x, center_y) =
            Self::transformed_point(self.transform, gradient.center_x, gradient.center_y)?;
        let rotation = self.transform.b.atan2(self.transform.a) / std::f32::consts::PI;
        let map_angle = |angle: f32| {
            if preserves_orientation {
                angle + rotation
            } else {
                rotation - angle
            }
        };
        let start_angle = map_angle(gradient.start_angle);
        let end_angle = map_angle(gradient.end_angle);
        if !Self::finite_component(start_angle) || !Self::finite_component(end_angle) {
            return None;
        }
        Some(NativeGlyphGradient::Sweep {
            center_x,
            center_y,
            start_angle,
            end_angle,
            extend: Self::native_extend(gradient.extend),
            stops: Self::native_stops(gradient.stops(0, self.face.variation_coordinates()))?,
        })
    }

    fn active_clip_box(&self) -> Result<Option<NativeColorClipBox>, ()> {
        let mut clip_box: Option<NativeColorClipBox> = None;
        for clip in &self.clip_stack {
            match clip {
                NativeColorClip::Outline(generation) if *generation != self.outline_generation => {
                    return Err(());
                }
                NativeColorClip::Outline(_) => {}
                NativeColorClip::Box(next) => {
                    if !self.transform.is_default() {
                        return Err(());
                    }
                    clip_box = match clip_box {
                        Some(current) => Some(current.intersect(*next).ok_or(())?),
                        None => Some(*next),
                    };
                }
            }
        }
        Ok(clip_box)
    }
}

impl<'a> ttf_parser::colr::Painter<'a> for NativeColorPainter<'a> {
    fn outline_glyph(&mut self, glyph_id: ttf_parser::GlyphId) {
        if self.unsupported {
            return;
        }
        let mut builder = NativeOutlineBuilder::default();
        let Some(_) = self.face.outline_glyph(glyph_id, &mut builder) else {
            self.mark_unsupported();
            return;
        };
        let Some(mut contours) = builder.finish() else {
            self.mark_unsupported();
            return;
        };
        for contour in &mut contours {
            for point in contour {
                let x = self.transform.a * point.x + self.transform.c * point.y + self.transform.e;
                let y = self.transform.b * point.x + self.transform.d * point.y + self.transform.f;
                if !x.is_finite() || !y.is_finite() {
                    self.mark_unsupported();
                    return;
                }
                point.x = x;
                point.y = y;
            }
        }
        self.outline_generation = self.outline_generation.saturating_add(1);
        self.current = Some(contours);
    }

    fn paint(&mut self, paint: ttf_parser::colr::Paint<'a>) {
        if self.unsupported {
            return;
        }
        let Some(contours) = self.current.take() else {
            self.mark_unsupported();
            return;
        };
        let Ok(clip_box) = self.active_clip_box() else {
            self.mark_unsupported();
            return;
        };
        let paint = match paint {
            ttf_parser::colr::Paint::Solid(color) => {
                Some(NativeColorPaint::Solid(Self::native_color(color)))
            }
            ttf_parser::colr::Paint::LinearGradient(gradient) => self
                .linear_gradient(gradient)
                .map(NativeColorPaint::Gradient),
            ttf_parser::colr::Paint::RadialGradient(gradient) => self
                .radial_gradient(gradient)
                .map(NativeColorPaint::Gradient),
            ttf_parser::colr::Paint::SweepGradient(gradient) => self
                .sweep_gradient(gradient)
                .map(NativeColorPaint::Gradient),
        };
        let Some(paint) = paint else {
            self.mark_unsupported();
            return;
        };
        if self.layers.len() >= MAX_COLOR_GLYPH_LAYERS {
            self.mark_unsupported();
            return;
        }
        self.layers.push(NativeColorLayer {
            contours,
            paint,
            composite: self.layer_stack.last().copied().unwrap_or_default(),
            clip_box,
        });
    }

    fn push_clip(&mut self) {
        if self.unsupported {
            return;
        }
        if self.clip_stack.len() >= MAX_COLOR_CLIP_DEPTH || self.current.is_none() {
            self.mark_unsupported();
            return;
        }
        self.clip_stack
            .push(NativeColorClip::Outline(self.outline_generation));
    }

    fn push_clip_box(&mut self, clipbox: ttf_parser::RectF) {
        if self.unsupported
            || self.clip_stack.len() >= MAX_COLOR_CLIP_DEPTH
            || ![clipbox.x_min, clipbox.y_min, clipbox.x_max, clipbox.y_max]
                .into_iter()
                .all(Self::finite_component)
            || clipbox.x_min > clipbox.x_max
            || clipbox.y_min > clipbox.y_max
        {
            self.mark_unsupported();
            return;
        }
        self.clip_stack
            .push(NativeColorClip::Box(NativeColorClipBox {
                x_min: clipbox.x_min,
                y_min: clipbox.y_min,
                x_max: clipbox.x_max,
                y_max: clipbox.y_max,
            }));
    }

    fn pop_clip(&mut self) {
        if self.unsupported {
            return;
        }
        if self.clip_stack.pop().is_none() {
            self.mark_unsupported();
            return;
        }
    }

    fn push_layer(&mut self, mode: ttf_parser::colr::CompositeMode) {
        if self.unsupported || self.layer_stack.len() >= MAX_COLOR_TRANSFORM_DEPTH {
            self.mark_unsupported();
            return;
        }
        let Some(mode) = (match mode {
            ttf_parser::colr::CompositeMode::SourceOver => Some(NativeGlyphComposite::SourceOver),
            ttf_parser::colr::CompositeMode::DestinationOver => {
                Some(NativeGlyphComposite::DestinationOver)
            }
            _ => None,
        }) else {
            self.mark_unsupported();
            return;
        };
        self.layer_stack.push(mode);
    }

    fn pop_layer(&mut self) {
        if self.unsupported {
            return;
        }
        if self.layer_stack.pop().is_none() {
            self.mark_unsupported();
        }
    }

    fn push_transform(&mut self, transform: ttf_parser::Transform) {
        if self.unsupported
            || self.transform_stack.len() >= MAX_COLOR_TRANSFORM_DEPTH
            || !Self::transform_is_bounded(transform)
        {
            self.mark_unsupported();
            return;
        }
        let combined = ttf_parser::Transform::combine(self.transform, transform);
        if !Self::transform_is_bounded(combined) {
            self.mark_unsupported();
            return;
        }
        self.transform_stack.push(self.transform);
        self.transform = combined;
    }

    fn pop_transform(&mut self) {
        if self.unsupported {
            return;
        }
        let Some(transform) = self.transform_stack.pop() else {
            self.mark_unsupported();
            return;
        };
        self.transform = transform;
    }
}

#[derive(Debug, Clone)]
struct NativeRasterizedGlyph {
    xmin: i32,
    ymin: i32,
    width: u32,
    height: u32,
    color: Option<NativeColor>,
    composite: NativeGlyphComposite,
    gradient: Option<NativeGlyphGradient>,
    bitmap: Option<Vec<u8>>,
    is_bitmap: bool,
    coverage: Vec<u8>,
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
    font: Arc<fontdue::Font>,
    font_data: Arc<[u8]>,
    collection_index: u32,
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
            && self.stretch == other.stretch
            && self.variation_settings == other.variation_settings
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
    ascent: u32,
    line_height: u32,
    direction: DirectionValue,
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
    #[cfg(test)]
    pub(crate) fn fallback_with_stretch(
        font_size: u32,
        stretch: u16,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures(
            font_size,
            stretch,
            NativeFontVariantLigatures::default(),
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features(
            font_size,
            stretch,
            ligatures,
            NativeFontFeatureSettings::default(),
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning(
            font_size,
            stretch,
            ligatures,
            feature_settings,
            NativeFontKerning::Auto,
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps(
            font_size,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            NativeFontVariantCaps::Normal,
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position(
            font_size,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            NativeFontVariantPosition::Normal,
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric(
            font_size,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            NativeFontVariantNumeric::default(),
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates(
            font_size,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            NativeFontVariantAlternates::Normal,
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian(
            font_size,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            variant_alternates,
            NativeFontVariantEastAsian::default(),
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        variant_east_asian: NativeFontVariantEastAsian,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language(
            font_size,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            variant_alternates,
            variant_east_asian,
            NativeFontLanguageOverride::default(),
            direction,
        )
    }

    #[cfg(test)]
    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        variant_east_asian: NativeFontVariantEastAsian,
        language_override: NativeFontLanguageOverride,
        direction: DirectionValue,
    ) -> Self {
        Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
            font_size,
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
            NativeFontVariationSettings::default(),
            direction,
        )
    }

    pub(crate) fn fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
        font_size: u32,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        variant_east_asian: NativeFontVariantEastAsian,
        language_override: NativeFontLanguageOverride,
        variation_settings: NativeFontVariationSettings,
        direction: DirectionValue,
    ) -> Self {
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
        Self::for_style_with_book_and_stretch(
            families, font_size, weight, style, 1000, direction, book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures(
            families,
            font_size,
            weight,
            style,
            stretch,
            NativeFontVariantLigatures::default(),
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            NativeFontFeatureSettings::default(),
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            feature_settings,
            NativeFontKerning::Auto,
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            NativeFontVariantCaps::Normal,
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            NativeFontVariantPosition::Normal,
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            NativeFontVariantNumeric::default(),
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            NativeFontVariantAlternates::Normal,
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            variant_alternates,
            NativeFontVariantEastAsian::default(),
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        variant_east_asian: NativeFontVariantEastAsian,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language(
            families,
            font_size,
            weight,
            style,
            stretch,
            ligatures,
            feature_settings,
            kerning,
            variant_caps,
            variant_position,
            variant_numeric,
            variant_alternates,
            variant_east_asian,
            NativeFontLanguageOverride::default(),
            direction,
            book,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        variant_east_asian: NativeFontVariantEastAsian,
        language_override: NativeFontLanguageOverride,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        Self::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
            families,
            font_size,
            weight,
            style,
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
            NativeFontVariationSettings::default(),
            direction,
            book,
        )
    }

    pub(crate) fn for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
        families: NativeFontFamilyList,
        font_size: u32,
        weight: FontWeightValue,
        style: FontStyleValue,
        stretch: u16,
        ligatures: NativeFontVariantLigatures,
        feature_settings: NativeFontFeatureSettings,
        kerning: NativeFontKerning,
        variant_caps: NativeFontVariantCaps,
        variant_position: NativeFontVariantPosition,
        variant_numeric: NativeFontVariantNumeric,
        variant_alternates: NativeFontVariantAlternates,
        variant_east_asian: NativeFontVariantEastAsian,
        language_override: NativeFontLanguageOverride,
        variation_settings: NativeFontVariationSettings,
        direction: DirectionValue,
        book: &NativeFontBook,
    ) -> Self {
        let font_size = font_size.clamp(1, MAX_NATIVE_FONT_SIZE);
        let stretch = stretch.clamp(500, 2000);
        let faces = book.faces_for(families, weight, style, stretch);
        let Some(face) = faces.first() else {
            return Self::fallback_with_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
                font_size,
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
            );
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
            ascent,
            line_height,
            direction,
        }
    }

    pub(crate) fn with_optical_sizing(mut self, optical_sizing: NativeFontOpticalSizing) -> Self {
        self.optical_sizing = optical_sizing;
        self
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
                    ceil_stretched(
                        face.font
                            .metrics(character, self.font_size as f32)
                            .advance_width,
                        self.stretch,
                        nominal,
                    )
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
        if let Some(language) = font_language_override_language(self.language_override) {
            buffer.set_language(language);
        }
        let direction = match self.direction {
            DirectionValue::Ltr => harfrust::Direction::LeftToRight,
            DirectionValue::Rtl => harfrust::Direction::RightToLeft,
        };
        buffer.set_direction(direction);
        let scale = i32::try_from(self.font_size.saturating_mul(FONT_SHAPE_SCALE)).ok()?;
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
        let mut features = Vec::with_capacity(21 + self.feature_settings.values().len());
        for (tag, value) in [
            (*b"liga", u32::from(self.ligatures.common)),
            (*b"clig", u32::from(self.ligatures.common)),
            (*b"dlig", u32::from(self.ligatures.discretionary)),
            (*b"hlig", u32::from(self.ligatures.historical)),
            (*b"calt", u32::from(self.ligatures.contextual)),
        ] {
            if !self.feature_settings.contains_tag(&tag) {
                features.push(harfrust::Feature::new(harfrust::Tag::new(&tag), value, ..));
            }
        }
        if !matches!(self.kerning, NativeFontKerning::Auto)
            && !self.feature_settings.contains_tag(b"kern")
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
            push_feature_if_not_explicit(&mut features, self.feature_settings, tag, 1);
        }
        if let Some(tag) = font_variant_position_tag(self.variant_position) {
            push_feature_if_not_explicit(&mut features, self.feature_settings, tag, 1);
        }
        let (numeric_tags, numeric_count) = font_variant_numeric_tags(self.variant_numeric);
        for tag in numeric_tags.into_iter().take(numeric_count).flatten() {
            push_feature_if_not_explicit(&mut features, self.feature_settings, tag, 1);
        }
        if let Some(tag) = font_variant_alternates_tag(self.variant_alternates) {
            push_feature_if_not_explicit(&mut features, self.feature_settings, tag, 1);
        }
        let (east_asian_tags, east_asian_count) =
            font_variant_east_asian_tags(self.variant_east_asian);
        for tag in east_asian_tags.into_iter().take(east_asian_count).flatten() {
            push_feature_if_not_explicit(&mut features, self.feature_settings, tag, 1);
        }
        let explicit_features = self.feature_settings.values();
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
            if glyph_id >= face.font.glyph_count() {
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
        if let Some(layers) = rasterize_color_glyph(
            face.font_data.as_ref(),
            glyph_id,
            self.font_size,
            variation_settings,
        ) {
            return Some(layers);
        }
        if let Some(rasterized) = rasterize_bitmap_glyph(
            face.font_data.as_ref(),
            face.collection_index,
            glyph_id,
            self.font_size,
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
            self.font_size,
            variation_settings,
        ) {
            return Some(vec![rasterized]);
        }
        let (metrics, coverage) = face.font.rasterize_indexed(glyph_id, self.font_size as f32);
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
                let kerning = face
                    .font
                    .horizontal_kern(previous, character, self.font_size as f32)
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
                face.font.lookup_glyph_index(character),
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
    if face.variation_axes().len() == 0 {
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

fn rasterize_color_glyph(
    font_data: &[u8],
    glyph_id: u16,
    font_size: u32,
    variation_settings: NativeFontVariationSettings,
) -> Option<Vec<NativeRasterizedGlyph>> {
    let mut face = ttf_parser::Face::parse(font_data, 0).ok()?;
    for variation in variation_settings.values() {
        let _ = face.set_variation(
            ttf_parser::Tag::from_bytes(&variation.tag),
            variation.value_milli as f32 / 1_000.0,
        );
    }
    let glyph_id = ttf_parser::GlyphId(glyph_id);
    if !face.is_color_glyph(glyph_id) {
        return None;
    }
    let mut painter = NativeColorPainter::new(face.clone());
    face.paint_color_glyph(
        glyph_id,
        0,
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
                layer.clip_box,
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
    clip_box: Option<NativeColorClipBox>,
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
                    if clip_box.is_none_or(|clip| clip.contains(x, y))
                        && outline_contains(&contours, x, y)
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
/// existing fontdue/HarfRust owners.
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

struct BoundedBrotliWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedBrotliWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "WOFF2 Brotli output exceeds the native font limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn decode_woff2_brotli(
    compressed: &[u8],
    size_hint: usize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if size_hint == 0 || size_hint > MAX_NATIVE_FONT_BYTES {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "WOFF2 Brotli table stream exceeds the native font limit",
        )));
    }
    let mut reader = Cursor::new(compressed);
    let mut writer = BoundedBrotliWriter {
        bytes: Vec::with_capacity(size_hint),
        limit: size_hint,
    };
    brotli_decompressor::BrotliDecompress(&mut reader, &mut writer)?;
    if writer.bytes.len() != size_hint {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "WOFF2 Brotli table stream has an unexpected size",
        )));
    }
    Ok(writer.bytes)
}

/// Convert a bounded WOFF2 container to the SFNT bytes consumed by the
/// existing fontdue/HarfRust owners. Header limits are checked before Wuff's
/// table-directory parser and the Brotli callback keeps its decompressed table
/// stream within the same native font budget.
fn decode_woff2(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < WOFF2_HEADER_BYTES || bytes.len() > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    if read_u32_be(bytes, 0)? != 0x774f_4632 {
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
    let total_compressed_size = usize::try_from(read_u32_be(bytes, 20)?).ok()?;
    if total_compressed_size == 0 || total_compressed_size > bytes.len() {
        return None;
    }
    let decoded =
        wuff::decompress_woff2_with_custom_brotli(bytes, &mut decode_woff2_brotli).ok()?;
    (decoded.len() <= MAX_NATIVE_FONT_BYTES).then_some(decoded)
}

fn normalize_font_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    match read_u32_be(bytes, 0)? {
        0x774f_4646 => decode_woff(bytes),
        0x774f_4632 => decode_woff2(bytes),
        _ => Some(bytes.to_vec()),
    }
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
        system_font_book().local_font_bytes_with_stretch(family, weight, style, stretch)
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
                stretch: resource.stretch,
                variation_settings: resource.variation_settings,
                font: Arc::new(font),
                font_data,
                collection_index: 0,
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
        font: Arc::new(font),
        font_data,
        collection_index,
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

fn face_score(
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

fn ceil_positive(value: f32) -> u32 {
    if value.is_finite() && value > 0.0 {
        value.ceil().min(u32::MAX as f32) as u32
    } else {
        0
    }
}

fn scale_fixed(value: i64, requested: u16, nominal: u16) -> i64 {
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

fn scale_dimension(value: u32, requested: u16, nominal: u16) -> u32 {
    if value == 0 || nominal == 0 {
        return value;
    }
    let scaled = u64::from(value)
        .saturating_mul(u64::from(requested))
        .saturating_add(u64::from(nominal / 2))
        / u64::from(nominal);
    u32::try_from(scaled.max(1)).unwrap_or(u32::MAX)
}

fn scale_signed(value: i32, requested: u16, nominal: u16) -> i32 {
    i32::try_from(scale_fixed(i64::from(value), requested, nominal)).unwrap_or(
        if value.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        },
    )
}

fn ceil_stretched(value: f32, requested: u16, nominal: u16) -> u32 {
    if nominal == 0 {
        return ceil_positive(value);
    }
    ceil_positive(value * (f32::from(requested) / f32::from(nominal)))
}

fn round_stretched(value: f32, requested: u16, nominal: u16) -> i32 {
    if nominal == 0 {
        return round_signed(value);
    }
    round_signed(value * (f32::from(requested) / f32::from(nominal)))
}

fn scale_raster_bytes(
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

fn scale_bitmap_horizontal(
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

fn scale_coverage_horizontal(
    coverage: Vec<u8>,
    width: u32,
    height: u32,
    requested: u16,
    nominal: u16,
) -> Option<(u32, Arc<[u8]>)> {
    scale_coverage_horizontal_with_limit(coverage, width, height, requested, nominal, None)
}

fn scale_coverage_horizontal_bounded(
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

fn scale_coverage_horizontal_with_limit(
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
    use base64::Engine as _;

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
            )
            .expect("bounded COLRv1 gradient must rasterize");
            assert!(layers.iter().any(|layer| {
                layer.gradient.is_some() && layer.coverage.iter().any(|coverage| *coverage > 0)
            }));
        }
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
    fn colr_gradient_transform_geometry_allows_affine_radial_shapes() {
        let translated = ttf_parser::Transform::new_translate(12.0, -4.0);
        assert_eq!(
            NativeColorPainter::transformed_point(translated, 3.0, 5.0),
            Some((15.0, 1.0))
        );
        let scaled = ttf_parser::Transform::new_scale(2.0, 2.0);
        let (scale, preserves_orientation) =
            NativeColorPainter::conformal_scale(scaled).expect("uniform scale is conformal");
        assert!((scale - 2.0).abs() <= f32::EPSILON);
        assert!(preserves_orientation);
        let reflected = ttf_parser::Transform::new_scale(-2.0, 2.0);
        let (_, preserves_orientation) = NativeColorPainter::conformal_scale(reflected)
            .expect("uniform reflection is conformal");
        assert!(!preserves_orientation);

        let skew = NativeGradientTransform::from_ttf(ttf_parser::Transform::new_skew(0.125, 0.0));
        assert!(skew.is_invertible());
        assert!(skew.inverse_point(4.0, 5.0).is_some());
        assert!(
            !NativeGradientTransform::from_ttf(ttf_parser::Transform::new_scale(0.0, 1.0))
                .is_invertible()
        );
        assert!(
            NativeColorPainter::conformal_scale(ttf_parser::Transform::new_skew(0.125, 0.0))
                .is_none()
        );
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
        let decoded =
            decode_bitmap_glyph_image(bgra).expect("premultiplied BGRA bitmap must decode");
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
            scale_coverage_horizontal_bounded(vec![u8::MAX; 1_024], 1_024, 1, 2_000, 1_000)
                .is_none()
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
        let mut painter = NativeColorPainter::new(face.clone());
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
            weight: NativeFontWeightRange::default(),
            style: FontStyleValue::Normal,
            stretch: NativeFontStretchRange::default(),
            variation_settings: NativeFontVariationSettings::default(),
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
                bytes: system_face.font_data.clone(),
                unicode_ranges: Vec::new(),
            },
        ];
        let book = NativeFontBook::from_resources(&resources);
        let families = NativeFontFamilyList::parse("Stretch Face").unwrap();
        let normal = NativeTextMetrics::for_style_with_book_and_stretch(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1000,
            DirectionValue::Ltr,
            &book,
        );
        let condensed = NativeTextMetrics::for_style_with_book_and_stretch(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            750,
            DirectionValue::Ltr,
            &book,
        );
        let expanded = NativeTextMetrics::for_style_with_book_and_stretch(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1250,
            DirectionValue::Ltr,
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
        let default_metrics = NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1000,
            NativeFontVariantLigatures::default(),
            DirectionValue::Ltr,
            system_font_book(),
        );
        let no_common_metrics = NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1000,
            NativeFontVariantLigatures {
                common: false,
                ..NativeFontVariantLigatures::default()
            },
            DirectionValue::Ltr,
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
        let explicit_off_metrics =
            NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features(
                families,
                DEFAULT_NATIVE_FONT_SIZE,
                FontWeightValue::Normal,
                FontStyleValue::Normal,
                1000,
                NativeFontVariantLigatures::default(),
                explicit_off_settings,
                DirectionValue::Ltr,
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
        let kerning_on =
            NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning(
                families,
                DEFAULT_NATIVE_FONT_SIZE,
                FontWeightValue::Normal,
                FontStyleValue::Normal,
                1000,
                NativeFontVariantLigatures::default(),
                NativeFontFeatureSettings::default(),
                NativeFontKerning::Normal,
                DirectionValue::Ltr,
                system_font_book(),
            );
        let kerning_off =
            NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning(
                families,
                DEFAULT_NATIVE_FONT_SIZE,
                FontWeightValue::Normal,
                FontStyleValue::Normal,
                1000,
                NativeFontVariantLigatures::default(),
                NativeFontFeatureSettings::default(),
                NativeFontKerning::None,
                DirectionValue::Ltr,
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
        let explicit_off =
            NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning(
                families,
                DEFAULT_NATIVE_FONT_SIZE,
                FontWeightValue::Normal,
                FontStyleValue::Normal,
                1000,
                NativeFontVariantLigatures::default(),
                low_level_off,
                NativeFontKerning::Normal,
                DirectionValue::Ltr,
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
        if let Some(tag) = font_variant_alternates_tag(NativeFontVariantAlternates::HistoricalForms)
        {
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
            bytes: Arc::from(bytes),
            unicode_ranges: Vec::new(),
        };
        let book = NativeFontBook::from_resources(&[resource]);
        let families = NativeFontFamilyList::parse("Variable Face").unwrap();
        let normal = NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1000,
            NativeFontVariantLigatures::default(),
            NativeFontFeatureSettings::default(),
            NativeFontKerning::Auto,
            NativeFontVariantCaps::Normal,
            NativeFontVariantPosition::Normal,
            NativeFontVariantNumeric::default(),
            NativeFontVariantAlternates::Normal,
            NativeFontVariantEastAsian::default(),
            NativeFontLanguageOverride::default(),
            NativeFontVariationSettings::default(),
            DirectionValue::Ltr,
            &book,
        );
        let mut narrow_settings = NativeFontVariationSettings::default();
        narrow_settings.values[0] = NativeFontVariation {
            tag: *b"wdth",
            value_milli: 75_000,
        };
        narrow_settings.count = 1;
        let narrow = NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1000,
            NativeFontVariantLigatures::default(),
            NativeFontFeatureSettings::default(),
            NativeFontKerning::Auto,
            NativeFontVariantCaps::Normal,
            NativeFontVariantPosition::Normal,
            NativeFontVariantNumeric::default(),
            NativeFontVariantAlternates::Normal,
            NativeFontVariantEastAsian::default(),
            NativeFontLanguageOverride::default(),
            narrow_settings,
            DirectionValue::Ltr,
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
            bytes: Arc::from(bytes),
            unicode_ranges: Vec::new(),
        };
        let book = NativeFontBook::from_resources(&[resource]);
        let families = NativeFontFamilyList::parse("Descriptor Variable Face").unwrap();
        let descriptor_metrics = NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1000,
            NativeFontVariantLigatures::default(),
            NativeFontFeatureSettings::default(),
            NativeFontKerning::Auto,
            NativeFontVariantCaps::Normal,
            NativeFontVariantPosition::Normal,
            NativeFontVariantNumeric::default(),
            NativeFontVariantAlternates::Normal,
            NativeFontVariantEastAsian::default(),
            NativeFontLanguageOverride::default(),
            NativeFontVariationSettings::default(),
            DirectionValue::Ltr,
            &book,
        );
        let mut authored_settings = NativeFontVariationSettings::default();
        authored_settings.values[0] = NativeFontVariation {
            tag: *b"wdth",
            value_milli: 100_000,
        };
        authored_settings.count = 1;
        let authored_metrics = NativeTextMetrics::for_style_with_book_and_stretch_and_ligatures_and_features_and_kerning_and_variant_caps_and_position_and_numeric_and_alternates_and_east_asian_and_language_and_variations(
            families,
            DEFAULT_NATIVE_FONT_SIZE,
            FontWeightValue::Normal,
            FontStyleValue::Normal,
            1000,
            NativeFontVariantLigatures::default(),
            NativeFontFeatureSettings::default(),
            NativeFontKerning::Auto,
            NativeFontVariantCaps::Normal,
            NativeFontVariantPosition::Normal,
            NativeFontVariantNumeric::default(),
            NativeFontVariantAlternates::Normal,
            NativeFontVariantEastAsian::default(),
            NativeFontLanguageOverride::default(),
            authored_settings,
            DirectionValue::Ltr,
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
        let metrics = NativeTextMetrics::fallback_with_stretch(
            DEFAULT_NATIVE_FONT_SIZE,
            1000,
            DirectionValue::Ltr,
        );
        assert_eq!(metrics.measure_text("A B", 0, 0), 24);
        assert_eq!(metrics.line_height(), FALLBACK_LINE_HEIGHT);
        assert!(metrics.rasterize("A", 0, 0, 0).is_none());
    }
}

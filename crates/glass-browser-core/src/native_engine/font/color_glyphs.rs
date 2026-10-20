use super::*;

#[derive(Debug, Clone)]
pub(super) struct NativeColorLayer {
    pub(super) contours: Vec<Vec<NativeOutlinePoint>>,
    pub(super) paint: NativeColorPaint,
    pub(super) composite: NativeGlyphComposite,
    pub(super) clip_mask: Option<NativeColorClipMask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NativeColorPaint {
    Solid(NativeColor),
    Gradient(NativeGlyphGradient),
}

impl NativeColorPaint {
    pub(super) fn glyph_gradient(
        &self,
        scale: f32,
        xmin: i32,
        ymax: i32,
    ) -> Option<NativeGlyphGradient> {
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
                transform,
                extend,
                stops,
            } => NativeGlyphGradient::Sweep {
                center_x: *center_x,
                center_y: *center_y,
                start_angle: *start_angle,
                end_angle: *end_angle,
                transform: pixel_transform.compose(*transform),
                extend: *extend,
                stops: stops.clone(),
            },
        };
        Some(mapped)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct NativeColorClipPolygon {
    points: [NativeOutlinePoint; 4],
}

impl NativeColorClipPolygon {
    pub(super) fn from_bounds(
        x_min: f32,
        y_min: f32,
        x_max: f32,
        y_max: f32,
        transform: ttf_parser::Transform,
    ) -> Option<Self> {
        if !NativeGradientTransform::from_ttf(transform).is_invertible() {
            return None;
        }
        let transform_point = |x: f32, y: f32| {
            let transformed_x = transform.a.mul_add(x, transform.c.mul_add(y, transform.e));
            let transformed_y = transform.b.mul_add(x, transform.d.mul_add(y, transform.f));
            (transformed_x.is_finite()
                && transformed_y.is_finite()
                && transformed_x.abs() <= MAX_COLOR_TRANSFORM_COMPONENT
                && transformed_y.abs() <= MAX_COLOR_TRANSFORM_COMPONENT)
                .then_some(NativeOutlinePoint {
                    x: transformed_x,
                    y: transformed_y,
                })
        };
        let points = [
            transform_point(x_min, y_min)?,
            transform_point(x_max, y_min)?,
            transform_point(x_max, y_max)?,
            transform_point(x_min, y_max)?,
        ];
        let area = points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let next = points[(index + 1) % points.len()];
                point.x.mul_add(next.y, -(point.y * next.x))
            })
            .sum::<f32>();
        area.is_finite().then_some(Self { points })
    }

    fn contains(self, x: f32, y: f32) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let mut orientation = 0_i8;
        for index in 0..self.points.len() {
            let start = self.points[index];
            let end = self.points[(index + 1) % self.points.len()];
            let cross =
                (end.x - start.x).mul_add(y - start.y, -((end.y - start.y) * (x - start.x)));
            if !cross.is_finite() {
                return false;
            }
            if cross.abs() <= f32::EPSILON {
                continue;
            }
            let edge_orientation = if cross > 0.0 { 1 } else { -1 };
            if orientation == 0 {
                orientation = edge_orientation;
            } else if orientation != edge_orientation {
                return false;
            }
        }
        orientation != 0
    }
}

#[derive(Debug, Clone)]
pub(super) struct NativeColorClipMask {
    pub(super) polygons: Vec<NativeColorClipPolygon>,
}

impl NativeColorClipMask {
    pub(super) fn contains(&self, x: f32, y: f32) -> bool {
        self.polygons
            .iter()
            .copied()
            .all(|polygon| polygon.contains(x, y))
    }
}

#[derive(Debug, Clone, Copy)]
enum NativeColorClip {
    Outline(u64),
    Box(NativeColorClipPolygon),
}

pub(super) struct NativeColorPainter<'a> {
    face: ttf_parser::Face<'a>,
    palette_colors: Vec<NativeColor>,
    palette_overrides: [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
    foreground_color: NativeColor,
    current: Option<Vec<Vec<NativeOutlinePoint>>>,
    pub(super) layers: Vec<NativeColorLayer>,
    transform: ttf_parser::Transform,
    transform_stack: Vec<ttf_parser::Transform>,
    outline_generation: u64,
    clip_stack: Vec<NativeColorClip>,
    layer_stack: Vec<NativeGlyphComposite>,
    pub(super) unsupported: bool,
}

impl<'a> NativeColorPainter<'a> {
    pub(super) fn new(
        face: ttf_parser::Face<'a>,
        palette_colors: Vec<NativeColor>,
        palette_overrides: [Option<NativeFontPaletteOverride>; MAX_NATIVE_FONT_PALETTE_OVERRIDES],
        foreground_color: NativeColor,
    ) -> Self {
        Self {
            face,
            palette_colors,
            palette_overrides,
            foreground_color,
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

    fn color_with_overrides(&self, color: ttf_parser::RgbaColor) -> Option<NativeColor> {
        let native = Self::native_color(color);
        if self.palette_overrides.iter().all(Option::is_none) {
            return Some(native);
        }
        if native == self.foreground_color {
            return Some(native);
        }
        let mut matched_palette_color = false;
        for (palette_index, palette_color) in self.palette_colors.iter().enumerate() {
            if *palette_color != native {
                continue;
            }
            matched_palette_color = true;
            if let Some(palette_override) =
                self.palette_overrides.get(palette_index).copied().flatten()
            {
                return Some(palette_override.color);
            }
        }
        matched_palette_color.then_some(native)
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

    pub(super) fn is_balanced(&self) -> bool {
        self.current.is_none()
            && self.transform_stack.is_empty()
            && self.clip_stack.is_empty()
            && self.layer_stack.is_empty()
    }

    fn native_stops<I>(&self, stops: I) -> Option<Arc<[NativeGradientStop]>>
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
                color: self.color_with_overrides(stop.color)?,
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

    pub(super) fn transformed_point(
        transform: ttf_parser::Transform,
        x: f32,
        y: f32,
    ) -> Option<(f32, f32)> {
        let transformed_x = transform.a.mul_add(x, transform.c.mul_add(y, transform.e));
        let transformed_y = transform.b.mul_add(x, transform.d.mul_add(y, transform.f));
        (Self::finite_component(transformed_x) && Self::finite_component(transformed_y))
            .then_some((transformed_x, transformed_y))
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
            stops: self.native_stops(gradient.stops(0, self.face.variation_coordinates()))?,
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
            stops: self.native_stops(gradient.stops(0, self.face.variation_coordinates()))?,
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
        let transform = NativeGradientTransform::from_ttf(self.transform);
        if !transform.is_invertible() {
            return None;
        }
        Some(NativeGlyphGradient::Sweep {
            center_x: gradient.center_x,
            center_y: gradient.center_y,
            start_angle: gradient.start_angle,
            end_angle: gradient.end_angle,
            transform,
            extend: Self::native_extend(gradient.extend),
            stops: self.native_stops(gradient.stops(0, self.face.variation_coordinates()))?,
        })
    }

    fn active_clip_mask(&self) -> Result<Option<NativeColorClipMask>, ()> {
        let mut polygons = Vec::new();
        for clip in &self.clip_stack {
            match clip {
                NativeColorClip::Outline(generation) if *generation != self.outline_generation => {
                    return Err(());
                }
                NativeColorClip::Outline(_) => {}
                NativeColorClip::Box(polygon) => polygons.push(*polygon),
            }
        }
        Ok((!polygons.is_empty()).then_some(NativeColorClipMask { polygons }))
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
        let Ok(clip_mask) = self.active_clip_mask() else {
            self.mark_unsupported();
            return;
        };
        let paint = match paint {
            ttf_parser::colr::Paint::Solid(color) => self
                .color_with_overrides(color)
                .map(NativeColorPaint::Solid),
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
            clip_mask,
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
        let transform = NativeGradientTransform::from_ttf(self.transform);
        let Some(polygon) = transform
            .is_invertible()
            .then(|| {
                NativeColorClipPolygon::from_bounds(
                    clipbox.x_min,
                    clipbox.y_min,
                    clipbox.x_max,
                    clipbox.y_max,
                    self.transform,
                )
            })
            .flatten()
        else {
            self.mark_unsupported();
            return;
        };
        self.clip_stack.push(NativeColorClip::Box(polygon));
    }

    fn pop_clip(&mut self) {
        if self.unsupported {
            return;
        }
        if self.clip_stack.pop().is_none() {
            self.mark_unsupported();
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
pub(super) struct NativeRasterizedGlyph {
    pub(super) xmin: i32,
    pub(super) ymin: i32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) color: Option<NativeColor>,
    pub(super) composite: NativeGlyphComposite,
    pub(super) gradient: Option<NativeGlyphGradient>,
    pub(super) bitmap: Option<Vec<u8>>,
    pub(super) is_bitmap: bool,
    pub(super) coverage: Vec<u8>,
}

pub(super) fn adjusted_font_size(font_size: u32, size_adjust: u16) -> u32 {
    let adjusted = u64::from(font_size)
        .saturating_mul(u64::from(size_adjust))
        .saturating_add(500)
        / 1000;
    u32::try_from(adjusted.clamp(1, u64::from(MAX_NATIVE_FONT_SIZE).saturating_mul(4)))
        .unwrap_or(MAX_NATIVE_FONT_SIZE.saturating_mul(4))
}

use super::config::{MAX_NATIVE_DOM_DEPTH, Viewport};
use super::css::{
    AlignContentValue, AlignItemsValue, DisplayValue, FlexDirectionValue, FlexWrapValue,
    JustifyContentValue, NativeBorderRadius, NativeBoxEdges, NativeComputedStyle, TextAlignValue,
    TextOverflowValue, TextTransformValue, VerticalAlignValue, WhiteSpaceValue, WordBreakValue,
};
use super::dom::{NativeDocument, NativeNodeId, NativeNodeKind};
use super::error::NativeEngineError;

const DEFAULT_LINE_HEIGHT: u32 = 20;
const DEFAULT_CONTROL_HEIGHT: u32 = 24;
const DEFAULT_CONTROL_WIDTH: u32 = 160;
const DEFAULT_BUTTON_WIDTH: u32 = 80;
const CHARACTER_WIDTH: u32 = 8;

/// An integer-pixel point in the native viewport coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePoint {
    pub x: u32,
    pub y: u32,
}

/// A half-open integer-pixel rectangle in the native viewport coordinate
/// space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl NativeRect {
    pub const fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }

    pub const fn bottom(self) -> u32 {
        self.y.saturating_add(self.height)
    }

    pub const fn contains(self, point: NativePoint) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }
}

/// One derived layout box for a visible native element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeLayoutBox {
    pub node_id: NativeNodeId,
    pub rect: NativeRect,
    /// The content box after the bounded padding and border insets.
    pub content_rect: NativeRect,
    /// The physical circular corner radii for the outer border box.
    pub border_radius: NativeBorderRadius,
    pub depth: usize,
}

/// One bounded direct-text fragment placed by the native flow cursor.
///
/// The `node_id` identifies the containing element that owns the fragment's
/// computed style and clipping context. Text nodes themselves do not carry
/// independent CSS in this bounded model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeTextLayout {
    pub node_id: NativeNodeId,
    pub origin: NativePoint,
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeLayoutPaintOrder {
    BeginOpacityGroup { node_id: NativeNodeId, opacity: u8 },
    Box(usize),
    Text(usize),
    EndOpacityGroup { node_id: NativeNodeId },
}

/// Deterministic layout derived from one current native document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLayoutSnapshot {
    pub revision: u64,
    pub viewport: Viewport,
    /// The explicit root viewport offset applied to this projection.
    pub scroll_offset: NativePoint,
    /// The bounded document height before the viewport is translated.
    pub content_height: u32,
    /// The bounded document width before the viewport is translated.
    pub content_width: u32,
    pub boxes: Vec<NativeLayoutBox>,
    pub text_runs: Vec<NativeTextLayout>,
    pub(crate) paint_order: Vec<NativeLayoutPaintOrder>,
    overflow_clips: Vec<Option<NativeRect>>,
}

impl NativeLayoutSnapshot {
    pub(crate) fn compute(
        document: &NativeDocument,
        viewport: Viewport,
    ) -> Result<Self, NativeEngineError> {
        viewport.validate()?;
        let mut builder = LayoutBuilder {
            document,
            boxes: Vec::new(),
            text_runs: Vec::new(),
            paint_order: Vec::new(),
        };
        let flow = builder.layout_children(document.root(), 0, 0, viewport.width, 0);
        let max_box_bottom = builder
            .boxes
            .iter()
            .map(|layout_box| layout_box.rect.bottom())
            .max()
            .unwrap_or(0);
        let max_box_right = builder
            .boxes
            .iter()
            .map(|layout_box| layout_box.rect.right())
            .max()
            .unwrap_or(0);
        let max_text_right = builder
            .text_runs
            .iter()
            .filter_map(|text_run| {
                if text_run.truncated || text_run.text.is_empty() {
                    return None;
                }
                let text_rect = NativeRect {
                    x: text_run.origin.x,
                    y: text_run.origin.y,
                    width: LayoutBuilder::text_width_with_spacing(
                        &text_run.text,
                        document
                            .computed_style_for_layout(text_run.node_id)
                            .letter_spacing(),
                        document
                            .computed_style_for_layout(text_run.node_id)
                            .word_spacing(),
                    ),
                    height: DEFAULT_LINE_HEIGHT,
                };
                let visible_rect = overflow_clip_for(document, &builder.boxes, text_run.node_id)
                    .map_or(text_rect, |clip| intersect_rect(text_rect, clip));
                (visible_rect.width > 0 && visible_rect.height > 0).then_some(visible_rect.right())
            })
            .max()
            .unwrap_or(0);
        let overflow_clips = builder
            .boxes
            .iter()
            .map(|layout_box| overflow_clip_for(document, &builder.boxes, layout_box.node_id))
            .collect();
        Ok(Self {
            revision: document.revision(),
            viewport,
            scroll_offset: NativePoint { x: 0, y: 0 },
            content_height: viewport.height.max(flow.height).max(max_box_bottom),
            content_width: viewport
                .width
                .max(flow.width)
                .max(max_box_right)
                .max(max_text_right),
            boxes: builder.boxes,
            text_runs: builder.text_runs,
            paint_order: builder.paint_order,
            overflow_clips,
        })
    }

    pub fn box_for(&self, node_id: NativeNodeId) -> Option<NativeRect> {
        self.boxes
            .iter()
            .find(|layout_box| layout_box.node_id == node_id)
            .map(|layout_box| layout_box.rect)
    }

    /// Return the bounded rectangular overflow clip affecting one node.
    /// Coordinates remain in document space so root scrolling can be applied
    /// exactly once by viewport consumers.
    pub(crate) fn overflow_clip_for(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
    ) -> Option<NativeRect> {
        overflow_clip_for(document, &self.boxes, node_id)
    }

    /// Return a box projected into the current viewport, clipped at its
    /// visible viewport and bounded overflow edges. The layout box itself
    /// remains in document coordinates.
    pub fn viewport_rect_for(&self, node_id: NativeNodeId) -> Option<NativeRect> {
        let (box_index, layout_box) = self
            .boxes
            .iter()
            .enumerate()
            .find(|(_, layout_box)| layout_box.node_id == node_id)?;
        let rect = self
            .overflow_clips
            .get(box_index)
            .copied()
            .flatten()
            .map_or(layout_box.rect, |clip| {
                intersect_rect(layout_box.rect, clip)
            });
        let viewport_left = self.scroll_offset.x;
        let viewport_top = self.scroll_offset.y;
        let viewport_right = viewport_left.saturating_add(self.viewport.width);
        let viewport_bottom = viewport_top.saturating_add(self.viewport.height);
        let left = rect.x.max(viewport_left).min(viewport_right);
        let top = rect.y.max(viewport_top).min(viewport_bottom);
        let right = rect.right().min(viewport_right);
        let bottom = rect.bottom().min(viewport_bottom);
        (left < right && top < bottom).then_some(NativeRect {
            x: left.saturating_sub(viewport_left),
            y: top.saturating_sub(viewport_top),
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        })
    }

    /// Return the maximum root horizontal and vertical scroll offset for this
    /// document.
    pub const fn max_scroll_offset(&self) -> NativePoint {
        NativePoint {
            x: self.content_width.saturating_sub(self.viewport.width),
            y: self.content_height.saturating_sub(self.viewport.height),
        }
    }

    pub(crate) fn with_scroll_offset(
        mut self,
        scroll_offset: NativePoint,
    ) -> Result<Self, NativeEngineError> {
        let max_scroll = self.max_scroll_offset();
        if scroll_offset.x > max_scroll.x {
            return Err(NativeEngineError::invalid(
                "native scroll offset",
                "horizontal scroll offset exceeds document bounds",
            ));
        }
        if scroll_offset.y > max_scroll.y {
            return Err(NativeEngineError::invalid(
                "native scroll offset",
                "vertical scroll offset exceeds document bounds",
            ));
        }
        self.scroll_offset = scroll_offset;
        Ok(self)
    }

    /// Return the deepest visible element at an integer point. A point outside
    /// the viewport is rejected instead of being adjusted or scrolled into
    /// view. Later document boxes win equal-depth overlaps deterministically.
    pub fn hit_test(&self, x: i64, y: i64) -> Result<Option<NativeNodeId>, NativeEngineError> {
        if x < 0 || y < 0 {
            return Err(NativeEngineError::invalid(
                "hit-test point",
                "coordinates must be finite and non-negative",
            ));
        }
        let viewport_point = NativePoint {
            x: u32::try_from(x).map_err(|_| {
                NativeEngineError::invalid("hit-test point", "coordinates exceed integer bounds")
            })?,
            y: u32::try_from(y).map_err(|_| {
                NativeEngineError::invalid("hit-test point", "coordinates exceed integer bounds")
            })?,
        };
        if viewport_point.x >= self.viewport.width || viewport_point.y >= self.viewport.height {
            return Err(NativeEngineError::invalid(
                "hit-test point",
                "coordinates are outside the native viewport",
            ));
        }
        let point = NativePoint {
            x: viewport_point
                .x
                .checked_add(self.scroll_offset.x)
                .ok_or_else(|| {
                    NativeEngineError::invalid(
                        "hit-test point",
                        "coordinates exceed document bounds",
                    )
                })?,
            y: viewport_point
                .y
                .checked_add(self.scroll_offset.y)
                .ok_or_else(|| {
                    NativeEngineError::invalid(
                        "hit-test point",
                        "coordinates exceed document bounds",
                    )
                })?,
        };

        let mut best: Option<(usize, usize, NativeNodeId)> = None;
        for (order, layout_box) in self.boxes.iter().enumerate() {
            if let Some(clip) = self.overflow_clips.get(order).copied().flatten()
                && !clip.contains(point)
            {
                continue;
            }
            if !rounded_rect_contains(layout_box.rect, layout_box.border_radius, point) {
                continue;
            }
            let replaces = best.is_none_or(|(best_depth, best_order, _)| {
                (layout_box.depth, order) > (best_depth, best_order)
            });
            if replaces {
                best = Some((layout_box.depth, order, layout_box.node_id));
            }
        }
        Ok(best.map(|(_, _, node_id)| node_id))
    }
}

fn overflow_clip_for(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    node_id: NativeNodeId,
) -> Option<NativeRect> {
    let mut current = Some(node_id);
    let mut clip = None;
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let Some(current_id) = current else {
            break;
        };
        let style = document.computed_style_for_layout(current_id);
        if (style.overflow_clip_x() || style.overflow_clip_y())
            && let Some(rect) = boxes
                .iter()
                .find(|layout_box| layout_box.node_id == current_id)
                .map(|layout_box| layout_box.rect)
        {
            let axis_clip = NativeRect {
                x: if style.overflow_clip_x() { rect.x } else { 0 },
                y: if style.overflow_clip_y() { rect.y } else { 0 },
                width: if style.overflow_clip_x() {
                    rect.width
                } else {
                    u32::MAX
                },
                height: if style.overflow_clip_y() {
                    rect.height
                } else {
                    u32::MAX
                },
            };
            clip = Some(match clip {
                Some(existing) => intersect_rect(existing, axis_clip),
                None => axis_clip,
            });
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    clip
}

fn intersect_rect(first: NativeRect, second: NativeRect) -> NativeRect {
    let left = first.x.max(second.x);
    let top = first.y.max(second.y);
    let right = first.right().min(second.right());
    let bottom = first.bottom().min(second.bottom());
    NativeRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

fn constrain_dimension(value: u32, minimum: Option<u32>, maximum: Option<u32>) -> u32 {
    let minimum = minimum.unwrap_or_default();
    let value = value.max(minimum);
    maximum.map_or(value, |maximum| value.min(maximum.max(minimum)))
}

pub(crate) fn rounded_rect_contains(
    rect: NativeRect,
    radius: NativeBorderRadius,
    point: NativePoint,
) -> bool {
    if !rect.contains(point) {
        return false;
    }
    let radius_limit = rect.width.min(rect.height) / 2;
    let top_left = radius.top_left.min(radius_limit);
    let top_right = radius.top_right.min(radius_limit);
    let bottom_right = radius.bottom_right.min(radius_limit);
    let bottom_left = radius.bottom_left.min(radius_limit);
    let left = i64::from(rect.x);
    let top = i64::from(rect.y);
    let right = i64::from(rect.right());
    let bottom = i64::from(rect.bottom());
    let x = i64::from(point.x);
    let y = i64::from(point.y);
    let corner = if x < left.saturating_add(i64::from(top_left))
        && y < top.saturating_add(i64::from(top_left))
    {
        (
            top_left,
            left.saturating_add(i64::from(top_left)),
            top.saturating_add(i64::from(top_left)),
        )
    } else if x >= right.saturating_sub(i64::from(top_right))
        && y < top.saturating_add(i64::from(top_right))
    {
        (
            top_right,
            right.saturating_sub(i64::from(top_right)),
            top.saturating_add(i64::from(top_right)),
        )
    } else if x >= right.saturating_sub(i64::from(bottom_right))
        && y >= bottom.saturating_sub(i64::from(bottom_right))
    {
        (
            bottom_right,
            right.saturating_sub(i64::from(bottom_right)),
            bottom.saturating_sub(i64::from(bottom_right)),
        )
    } else if x < left.saturating_add(i64::from(bottom_left))
        && y >= bottom.saturating_sub(i64::from(bottom_left))
    {
        (
            bottom_left,
            left.saturating_add(i64::from(bottom_left)),
            bottom.saturating_sub(i64::from(bottom_left)),
        )
    } else {
        return true;
    };
    let (radius, center_x, center_y) = corner;
    if radius == 0 {
        return true;
    }
    let point_x = x.saturating_mul(2).saturating_add(1);
    let point_y = y.saturating_mul(2).saturating_add(1);
    let center_x = center_x.saturating_mul(2);
    let center_y = center_y.saturating_mul(2);
    let delta_x = point_x.saturating_sub(center_x);
    let delta_y = point_y.saturating_sub(center_y);
    let radius = i64::from(radius).saturating_mul(2);
    delta_x
        .saturating_mul(delta_x)
        .saturating_add(delta_y.saturating_mul(delta_y))
        <= radius.saturating_mul(radius)
}

struct LayoutBuilder<'a> {
    document: &'a NativeDocument,
    boxes: Vec<NativeLayoutBox>,
    text_runs: Vec<NativeTextLayout>,
    paint_order: Vec<NativeLayoutPaintOrder>,
}

#[derive(Debug, Clone, Copy, Default)]
struct FlowSize {
    width: u32,
    height: u32,
}

struct FlexItemPlacement {
    box_start: usize,
    box_end: usize,
    text_start: usize,
    text_end: usize,
    margin: NativeBoxEdges,
    height: u32,
}

struct FlexItem {
    child: NativeNodeId,
    margin: NativeBoxEdges,
    width: u32,
    order: i32,
    source_index: usize,
}

struct FlexLineLayout {
    width: u32,
    placements: Vec<FlexItemPlacement>,
}

struct FlexLineRecord {
    y: u32,
    provisional_y: u32,
    height: u32,
    layout: FlexLineLayout,
}

struct FlexLineContext {
    x: u32,
    y: u32,
    available_width: u32,
    gap: u32,
    justify_content: JustifyContentValue,
    reverse: bool,
    depth: usize,
}

fn flex_space_around_line_offset(free_space: u32, line_index: u32, line_count: usize) -> u32 {
    let numerator = u64::from(free_space)
        .saturating_mul(u64::from(line_index).saturating_mul(2).saturating_add(1));
    let denominator = u64::try_from(line_count)
        .unwrap_or(u64::MAX)
        .saturating_mul(2)
        .max(1);
    u32::try_from(numerator / denominator).unwrap_or(u32::MAX)
}

fn flex_space_evenly_line_offset(free_space: u32, line_index: u32, line_count: usize) -> u32 {
    let numerator = u64::from(free_space).saturating_mul(u64::from(line_index).saturating_add(1));
    let denominator = u64::try_from(line_count)
        .unwrap_or(u64::MAX)
        .saturating_add(1)
        .max(1);
    u32::try_from(numerator / denominator).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy)]
struct FlowStyle {
    minimum_line_height: u32,
    text_align: TextAlignValue,
    allow_soft_wrap: bool,
    text_indent: u32,
    word_spacing: u32,
    letter_spacing: u32,
}

#[derive(Debug, Clone, Copy)]
struct FlowItem {
    box_start: usize,
    box_end: usize,
    text_start: usize,
    text_end: usize,
    height: u32,
    vertical_align: VerticalAlignValue,
}

impl FlowItem {
    fn vertical_offset(self, line_height: u32) -> u32 {
        let remaining = line_height.saturating_sub(self.height);
        match self.vertical_align {
            VerticalAlignValue::Baseline | VerticalAlignValue::Top => 0,
            VerticalAlignValue::Middle => remaining / 2,
            VerticalAlignValue::Bottom => remaining,
        }
    }
}

struct FlowCursor {
    base_start_x: u32,
    base_available_width: u32,
    start_x: u32,
    start_y: u32,
    available_width: u32,
    x: u32,
    y: u32,
    minimum_line_height: u32,
    text_align: TextAlignValue,
    allow_soft_wrap: bool,
    line_height: u32,
    line_has_content: bool,
    pending_whitespace: bool,
    word_spacing: u32,
    letter_spacing: u32,
    line_items: Vec<FlowItem>,
    max_right: u32,
    max_bottom: u32,
}

impl FlowCursor {
    fn new(x: u32, y: u32, available_width: u32, style: FlowStyle) -> Self {
        let effective_indent = style
            .text_indent
            .min(available_width.saturating_sub(CHARACTER_WIDTH));
        Self {
            base_start_x: x,
            base_available_width: available_width,
            start_x: x.saturating_add(effective_indent),
            start_y: y,
            available_width: available_width.saturating_sub(effective_indent),
            x: x.saturating_add(effective_indent),
            y,
            minimum_line_height: style.minimum_line_height,
            text_align: style.text_align,
            allow_soft_wrap: style.allow_soft_wrap,
            line_height: 0,
            line_has_content: false,
            pending_whitespace: false,
            word_spacing: style.word_spacing,
            letter_spacing: style.letter_spacing,
            line_items: Vec::new(),
            max_right: x,
            max_bottom: y,
        }
    }

    fn flush_line(&mut self) {
        self.pending_whitespace = false;
        if self.line_has_content {
            self.y = self
                .y
                .saturating_add(self.line_height.max(self.minimum_line_height));
            self.max_bottom = self.max_bottom.max(self.y);
            self.x = self.start_x;
            self.line_height = 0;
            self.line_has_content = false;
            self.line_items.clear();
        }
        self.reset_line_position();
    }

    fn force_line_break(&mut self) {
        self.pending_whitespace = false;
        self.y = self
            .y
            .saturating_add(self.line_height.max(self.minimum_line_height));
        self.max_bottom = self
            .max_bottom
            .max(self.y.saturating_add(self.minimum_line_height));
        self.line_height = 0;
        self.line_has_content = false;
        self.line_items.clear();
        self.reset_line_position();
    }

    fn reset_line_position(&mut self) {
        self.start_x = self.base_start_x;
        self.available_width = self.base_available_width;
        self.x = self.start_x;
    }

    fn take_pending_whitespace(&mut self) -> bool {
        std::mem::take(&mut self.pending_whitespace)
    }

    fn has_pending_whitespace(&self) -> bool {
        self.pending_whitespace
    }

    fn mark_pending_whitespace(&mut self) {
        self.pending_whitespace = true;
    }

    fn would_wrap(&self, width: u32) -> bool {
        self.allow_soft_wrap
            && self.line_has_content
            && self.x.saturating_sub(self.start_x).saturating_add(width) > self.available_width
    }

    fn line_capacity_for_text(&self, value: &str) -> usize {
        let remaining_width = self
            .available_width
            .saturating_sub(self.x.saturating_sub(self.start_x));
        let mut width: u32 = 0;
        let mut count: usize = 0;
        for character in value.chars() {
            let advance =
                LayoutBuilder::character_advance(character, self.letter_spacing, self.word_spacing);
            if width.saturating_add(advance) > remaining_width {
                if count == 0 && !self.line_has_content {
                    return 1;
                }
                break;
            }
            width = width.saturating_add(advance);
            count += 1;
        }
        count
    }

    fn text_width(&self, value: &str) -> u32 {
        LayoutBuilder::text_width_with_spacing(value, self.letter_spacing, self.word_spacing)
    }

    fn character_advance(&self, character: char) -> u32 {
        LayoutBuilder::character_advance(character, self.letter_spacing, self.word_spacing)
    }

    fn place_inline(&mut self, width: u32, height: u32) -> Option<NativePoint> {
        self.place_inline_with_origin(width, height)
    }

    fn place_inline_with_origin(&mut self, mut width: u32, height: u32) -> Option<NativePoint> {
        if self.available_width == 0 {
            return None;
        }
        if self.allow_soft_wrap {
            width = width.min(self.available_width);
        }
        let origin = NativePoint {
            x: self.x,
            y: self.y,
        };
        self.x = self.x.saturating_add(width);
        self.line_height = self.line_height.max(height.max(self.minimum_line_height));
        self.line_has_content = true;
        self.max_right = self.max_right.max(self.x);
        self.max_bottom = self.max_bottom.max(self.y.saturating_add(self.line_height));
        Some(origin)
    }

    fn place_unwrapped_with_origin(&mut self, width: u32, height: u32) -> Option<NativePoint> {
        if self.available_width == 0 {
            return None;
        }
        let origin = NativePoint {
            x: self.x,
            y: self.y,
        };
        self.x = self.x.saturating_add(width);
        self.line_height = self.line_height.max(height.max(self.minimum_line_height));
        self.line_has_content = true;
        self.max_right = self.max_right.max(self.x);
        self.max_bottom = self.max_bottom.max(self.y.saturating_add(self.line_height));
        Some(origin)
    }

    fn place_block(&mut self, height: u32) {
        self.y = self.y.saturating_add(height);
        self.max_bottom = self.max_bottom.max(self.y);
    }

    fn record_item(&mut self, item: FlowItem) {
        self.line_items.push(item);
    }

    fn alignment_offset(&self) -> u32 {
        let remaining = self
            .available_width
            .saturating_sub(self.x.saturating_sub(self.start_x));
        match self.text_align {
            TextAlignValue::Left => 0,
            TextAlignValue::Center => remaining / 2,
            TextAlignValue::Right => remaining,
        }
    }

    fn take_line_items(&mut self) -> Vec<FlowItem> {
        std::mem::take(&mut self.line_items)
    }

    fn finish(mut self) -> FlowSize {
        self.flush_line();
        FlowSize {
            width: self.max_right.saturating_sub(self.start_x),
            height: self.max_bottom.saturating_sub(self.start_y),
        }
    }
}

impl<'a> LayoutBuilder<'a> {
    fn layout_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let minimum_line_height = self
            .document
            .computed_style_for_layout(parent)
            .line_height()
            .unwrap_or(DEFAULT_LINE_HEIGHT);
        let white_space = self
            .document
            .computed_style_for_layout(parent)
            .white_space();
        let style = self.document.computed_style_for_layout(parent);
        let text_align = style.text_align();
        let text_indent = if self.effective_display(parent) == DisplayValue::Block {
            style.text_indent()
        } else {
            0
        };
        let word_spacing = style.word_spacing();
        let letter_spacing = style.letter_spacing();
        let allow_soft_wrap =
            !matches!(white_space, WhiteSpaceValue::Pre | WhiteSpaceValue::NoWrap);
        let mut flow = FlowCursor::new(
            x,
            y,
            available_width,
            FlowStyle {
                minimum_line_height,
                text_align,
                allow_soft_wrap,
                text_indent,
                word_spacing,
                letter_spacing,
            },
        );
        self.process_children(parent, &mut flow, depth);
        self.flush_line(&mut flow);
        let bottom = flow.max_bottom;
        let start_y = y;
        let mut result = flow.finish();
        result.height = bottom.saturating_sub(start_y).max(result.height);
        result
    }

    fn flush_line(&mut self, flow: &mut FlowCursor) {
        if flow.line_has_content {
            let offset = flow.alignment_offset();
            let line_height = flow.line_height.max(flow.minimum_line_height);
            let line_right = flow.x.saturating_add(offset);
            flow.max_right = flow.max_right.max(line_right);
            for item in flow.take_line_items() {
                let vertical_offset = item.vertical_offset(line_height);
                let box_end = item.box_end.min(self.boxes.len());
                let box_start = item.box_start.min(box_end);
                for layout_box in &mut self.boxes[box_start..box_end] {
                    layout_box.rect.x = layout_box.rect.x.saturating_add(offset);
                    layout_box.rect.y = layout_box.rect.y.saturating_add(vertical_offset);
                    layout_box.content_rect.x = layout_box.content_rect.x.saturating_add(offset);
                    layout_box.content_rect.y =
                        layout_box.content_rect.y.saturating_add(vertical_offset);
                }

                let text_end = item.text_end.min(self.text_runs.len());
                let text_start = item.text_start.min(text_end);
                for text_run in &mut self.text_runs[text_start..text_end] {
                    text_run.origin.x = text_run.origin.x.saturating_add(offset);
                    text_run.origin.y = text_run.origin.y.saturating_add(vertical_offset);
                }
            }
        }
        flow.flush_line();
    }

    fn force_line_break(&mut self, flow: &mut FlowCursor) {
        let had_content = flow.line_has_content;
        self.flush_line(flow);
        if had_content {
            flow.max_bottom = flow
                .max_bottom
                .max(flow.y.saturating_add(flow.minimum_line_height));
        } else {
            flow.force_line_break();
        }
    }

    fn process_children(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, depth: usize) {
        let children = self
            .document
            .node(parent)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();
        for child in children {
            let Some(node) = self.document.node(child) else {
                continue;
            };
            match node.kind() {
                NativeNodeKind::Text(value) => {
                    let value = value.clone();
                    self.place_text(parent, flow, &value);
                }
                NativeNodeKind::Document => {
                    self.process_children(child, flow, depth);
                }
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(child) || self.document.is_hidden_for_layout(child) {
                        continue;
                    }
                    let display = self.effective_display(child);
                    if self
                        .document
                        .node(child)
                        .and_then(|node| node.element_name())
                        == Some("br")
                    {
                        if display != DisplayValue::None {
                            self.force_line_break(flow);
                        }
                        continue;
                    }
                    match display {
                        DisplayValue::None => {}
                        DisplayValue::Contents => {
                            let opacity = self.document.computed_style_for_layout(child).opacity();
                            let grouped = opacity < u8::MAX;
                            if grouped {
                                self.paint_order
                                    .push(NativeLayoutPaintOrder::BeginOpacityGroup {
                                        node_id: child,
                                        opacity,
                                    });
                            }
                            self.process_children(child, flow, depth + 1);
                            if grouped {
                                self.paint_order
                                    .push(NativeLayoutPaintOrder::EndOpacityGroup {
                                        node_id: child,
                                    });
                            }
                        }
                        DisplayValue::Block | DisplayValue::Flex => {
                            self.flush_line(flow);
                            let margin = self.document.computed_style_for_layout(child).margin();
                            let size = self.layout_element(
                                child,
                                flow.start_x.saturating_add(margin.left()),
                                flow.y.saturating_add(margin.top()),
                                flow.available_width.saturating_sub(margin.horizontal()),
                                depth,
                            );
                            flow.max_right = flow.max_right.max(
                                flow.start_x
                                    .saturating_add(margin.left())
                                    .saturating_add(size.width)
                                    .saturating_add(margin.right()),
                            );
                            flow.place_block(size.height.saturating_add(margin.vertical()));
                        }
                        DisplayValue::Auto | DisplayValue::Inline | DisplayValue::Other => {
                            let style = self.document.computed_style_for_layout(child);
                            let margin = style.margin();
                            let available_width =
                                flow.available_width.saturating_sub(margin.horizontal());
                            let candidate_width =
                                self.outer_width(child, style, false, available_width, true);
                            let candidate_width =
                                candidate_width.saturating_add(margin.horizontal());
                            let separator_width =
                                if flow.has_pending_whitespace() && flow.line_has_content {
                                    flow.text_width(" ")
                                } else {
                                    0
                                };
                            if flow.would_wrap(candidate_width.saturating_add(separator_width)) {
                                self.flush_line(flow);
                            } else {
                                self.place_pending_separator(parent, flow);
                            }
                            let box_start = self.boxes.len();
                            let text_start = self.text_runs.len();
                            let size = self.layout_element(
                                child,
                                flow.x.saturating_add(margin.left()),
                                flow.y.saturating_add(margin.top()),
                                available_width,
                                depth,
                            );
                            if flow
                                .place_inline(
                                    size.width.saturating_add(margin.horizontal()),
                                    size.height.saturating_add(margin.vertical()),
                                )
                                .is_some()
                            {
                                flow.record_item(FlowItem {
                                    box_start,
                                    box_end: self.boxes.len(),
                                    text_start,
                                    text_end: self.text_runs.len(),
                                    height: size.height.saturating_add(margin.vertical()),
                                    vertical_align: style.vertical_align(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn layout_element(
        &mut self,
        id: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let style = self.document.computed_style_for_layout(id);
        let display = self.effective_display(id);
        if display == DisplayValue::None
            || self.is_non_rendered(id)
            || self.document.is_hidden_for_layout(id)
        {
            return FlowSize::default();
        }
        if display == DisplayValue::Contents {
            let opacity = style.opacity();
            let grouped = opacity < u8::MAX;
            if grouped {
                self.paint_order
                    .push(NativeLayoutPaintOrder::BeginOpacityGroup {
                        node_id: id,
                        opacity,
                    });
            }
            let result = self.layout_children(id, x, y, available_width, depth);
            if grouped {
                self.paint_order
                    .push(NativeLayoutPaintOrder::EndOpacityGroup { node_id: id });
            }
            return result;
        }

        let is_block = matches!(display, DisplayValue::Block | DisplayValue::Flex);
        let opacity = style.opacity();
        let grouped = opacity < u8::MAX;
        if grouped {
            self.paint_order
                .push(NativeLayoutPaintOrder::BeginOpacityGroup {
                    node_id: id,
                    opacity,
                });
        }
        let padding = style.padding();
        let (border_top, border_right, border_bottom, border_left) =
            style.border().map_or((0, 0, 0, 0), |border| {
                (
                    border.top().width(),
                    border.right().width(),
                    border.bottom().width(),
                    border.left().width(),
                )
            });
        let left_inset = border_left.saturating_add(padding.left());
        let right_inset = border_right.saturating_add(padding.right());
        let top_inset = border_top.saturating_add(padding.top());
        let bottom_inset = border_bottom.saturating_add(padding.bottom());
        let horizontal_inset = left_inset.saturating_add(right_inset);
        let vertical_inset = top_inset.saturating_add(bottom_inset);
        let width = self.outer_width(id, style, is_block, available_width, true);
        let minimum_line_height = style.line_height().unwrap_or(DEFAULT_LINE_HEIGHT);
        let default_content_height = if is_block {
            minimum_line_height
        } else {
            self.intrinsic_inline_height(id).max(minimum_line_height)
        };
        let box_index = self.boxes.len();
        self.boxes.push(NativeLayoutBox {
            node_id: id,
            rect: NativeRect {
                x,
                y,
                width,
                height: vertical_inset.saturating_add(default_content_height),
            },
            content_rect: NativeRect {
                x: x.saturating_add(left_inset),
                y: y.saturating_add(top_inset),
                width: width.saturating_sub(horizontal_inset),
                height: default_content_height,
            },
            border_radius: style.border_radius(),
            depth,
        });
        self.paint_order
            .push(NativeLayoutPaintOrder::Box(box_index));

        let content_width = width.saturating_sub(horizontal_inset);
        let children = if display == DisplayValue::Flex && self.can_use_flex_layout(id) {
            self.layout_flex_children(
                id,
                x.saturating_add(left_inset),
                y.saturating_add(top_inset),
                content_width,
                depth + 1,
            )
        } else {
            self.layout_children(
                id,
                x.saturating_add(left_inset),
                y.saturating_add(top_inset),
                content_width,
                depth + 1,
            )
        };
        let auto_content_height = default_content_height.max(children.height);
        let height = style.height().map_or(
            vertical_inset.saturating_add(auto_content_height),
            |declared| {
                if style.is_border_box() {
                    declared
                } else {
                    declared.saturating_add(vertical_inset)
                }
            },
        );
        let min_height = style.min_height().map(|declared| {
            if style.is_border_box() {
                declared
            } else {
                declared.saturating_add(vertical_inset)
            }
        });
        let max_height = style.max_height().map(|declared| {
            if style.is_border_box() {
                declared
            } else {
                declared.saturating_add(vertical_inset)
            }
        });
        let height = constrain_dimension(height, min_height, max_height);
        let content_height = height.saturating_sub(vertical_inset);
        self.boxes[box_index].rect.height = height;
        self.boxes[box_index].content_rect = NativeRect {
            x: x.saturating_add(left_inset),
            y: y.saturating_add(top_inset),
            width: content_width,
            height: content_height,
        };
        if grouped {
            self.paint_order
                .push(NativeLayoutPaintOrder::EndOpacityGroup { node_id: id });
        }
        FlowSize { width, height }
    }

    fn explicit_content_height(style: NativeComputedStyle) -> Option<u32> {
        let padding = style.padding();
        let border_vertical = style.border().map_or(0, |border| {
            border.top().width().saturating_add(border.bottom().width())
        });
        let vertical_inset = padding.vertical().saturating_add(border_vertical);
        let declared = style.height()?;
        let outer_height = if style.is_border_box() {
            declared
        } else {
            declared.saturating_add(vertical_inset)
        };
        let min_height = style.min_height().map(|value| {
            if style.is_border_box() {
                value
            } else {
                value.saturating_add(vertical_inset)
            }
        });
        let max_height = style.max_height().map(|value| {
            if style.is_border_box() {
                value
            } else {
                value.saturating_add(vertical_inset)
            }
        });
        Some(
            constrain_dimension(outer_height, min_height, max_height)
                .saturating_sub(vertical_inset),
        )
    }

    fn shift_layout_y(
        &mut self,
        box_start: usize,
        box_end: usize,
        text_start: usize,
        text_end: usize,
        offset: i64,
    ) {
        let shift = |value: u32| {
            if offset.is_negative() {
                value.saturating_sub(u32::try_from(offset.unsigned_abs()).unwrap_or(u32::MAX))
            } else {
                value.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX))
            }
        };
        let box_end = box_end.min(self.boxes.len());
        let box_start = box_start.min(box_end);
        for layout_box in &mut self.boxes[box_start..box_end] {
            layout_box.rect.y = shift(layout_box.rect.y);
            layout_box.content_rect.y = shift(layout_box.content_rect.y);
        }

        let text_end = text_end.min(self.text_runs.len());
        let text_start = text_start.min(text_end);
        for text_run in &mut self.text_runs[text_start..text_end] {
            text_run.origin.y = shift(text_run.origin.y);
        }
    }

    fn can_use_flex_layout(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.document.node(id) else {
            return false;
        };
        node.children().iter().all(|child| {
            let Some(child_node) = self.document.node(*child) else {
                return false;
            };
            match child_node.kind() {
                NativeNodeKind::Text(value) => value.chars().all(char::is_whitespace),
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(*child) || self.document.is_hidden_for_layout(*child) {
                        return true;
                    }
                    let display = self.effective_display(*child);
                    display != DisplayValue::Contents && child_node.element_name() != Some("br")
                }
                NativeNodeKind::Document => false,
            }
        })
    }

    fn layout_flex_line(
        &mut self,
        items: Vec<FlexItem>,
        context: FlexLineContext,
    ) -> FlexLineLayout {
        let gap_count = u32::try_from(items.len().saturating_sub(1)).unwrap_or(u32::MAX);
        let item_width = items.iter().fold(0u32, |total, item| {
            total
                .saturating_add(item.margin.horizontal())
                .saturating_add(item.width)
        });
        let occupied_width = item_width.saturating_add(context.gap.saturating_mul(gap_count));
        let free_space = context.available_width.saturating_sub(occupied_width);
        let leading_offset = if items.is_empty() {
            0
        } else {
            match context.justify_content {
                JustifyContentValue::Center => free_space / 2,
                JustifyContentValue::FlexEnd => free_space,
                JustifyContentValue::FlexStart | JustifyContentValue::SpaceBetween => 0,
            }
        };
        let distributed_gap = if context.justify_content == JustifyContentValue::SpaceBetween {
            free_space.checked_div(gap_count).unwrap_or(0)
        } else {
            0
        };
        let distributed_remainder = if context.justify_content == JustifyContentValue::SpaceBetween
        {
            free_space.checked_rem(gap_count).unwrap_or(0)
        } else {
            0
        };
        let item_count = items.len();
        let mut placements = Vec::with_capacity(item_count);
        let width = if context.reverse {
            let reverse_shift = occupied_width.saturating_sub(context.available_width);
            let mut cursor_right = context
                .x
                .saturating_add(context.available_width)
                .saturating_sub(leading_offset)
                .saturating_add(reverse_shift);
            for (index, item) in items.into_iter().enumerate() {
                if index > 0 {
                    cursor_right = cursor_right.saturating_sub(context.gap);
                    if context.justify_content == JustifyContentValue::SpaceBetween {
                        cursor_right = cursor_right.saturating_sub(distributed_gap);
                        if u32::try_from(index - 1)
                            .is_ok_and(|gap_index| gap_index < distributed_remainder)
                        {
                            cursor_right = cursor_right.saturating_sub(1);
                        }
                    }
                }
                let item_x = cursor_right
                    .saturating_sub(item.margin.right())
                    .saturating_sub(item.width);
                let item_y = context.y.saturating_add(item.margin.top());
                let box_start = self.boxes.len();
                let text_start = self.text_runs.len();
                let size =
                    self.layout_element(item.child, item_x, item_y, item.width, context.depth);
                placements.push(FlexItemPlacement {
                    box_start,
                    box_end: self.boxes.len(),
                    text_start,
                    text_end: self.text_runs.len(),
                    margin: item.margin,
                    height: size.height,
                });
                cursor_right = item_x.saturating_sub(item.margin.left());
            }
            if item_count == 0 {
                0
            } else {
                occupied_width.saturating_add(
                    if context.justify_content == JustifyContentValue::SpaceBetween {
                        free_space
                    } else {
                        leading_offset
                    },
                )
            }
        } else {
            let mut cursor_x = context.x.saturating_add(leading_offset);
            for (index, item) in items.into_iter().enumerate() {
                if index > 0 {
                    cursor_x = cursor_x.saturating_add(context.gap);
                    if context.justify_content == JustifyContentValue::SpaceBetween {
                        cursor_x = cursor_x.saturating_add(distributed_gap);
                        if u32::try_from(index - 1)
                            .is_ok_and(|gap_index| gap_index < distributed_remainder)
                        {
                            cursor_x = cursor_x.saturating_add(1);
                        }
                    }
                }
                let item_x = cursor_x.saturating_add(item.margin.left());
                let item_y = context.y.saturating_add(item.margin.top());
                let box_start = self.boxes.len();
                let text_start = self.text_runs.len();
                let size =
                    self.layout_element(item.child, item_x, item_y, item.width, context.depth);
                placements.push(FlexItemPlacement {
                    box_start,
                    box_end: self.boxes.len(),
                    text_start,
                    text_end: self.text_runs.len(),
                    margin: item.margin,
                    height: size.height,
                });
                cursor_x = cursor_x
                    .saturating_add(item.margin.left())
                    .saturating_add(size.width)
                    .saturating_add(item.margin.right());
            }
            cursor_x.saturating_sub(context.x)
        };

        FlexLineLayout { width, placements }
    }

    fn layout_flex_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        if !self.can_use_flex_layout(parent) {
            return self.layout_children(parent, x, y, available_width, depth);
        }
        let parent_style = self.document.computed_style_for_layout(parent);
        let gap = parent_style.gap();
        let justify_content = parent_style.justify_content();
        let align_items = parent_style.align_items();
        let flex_wrap = parent_style.flex_wrap();
        let wrapped = matches!(flex_wrap, FlexWrapValue::Wrap | FlexWrapValue::WrapReverse);
        let wrap_reverse = flex_wrap == FlexWrapValue::WrapReverse;
        let explicit_line_height = Self::explicit_content_height(parent_style);
        let children = self
            .document
            .node(parent)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();
        let mut items = Vec::new();
        for (source_index, child) in children.into_iter().enumerate() {
            let Some(node) = self.document.node(child) else {
                continue;
            };
            match node.kind() {
                NativeNodeKind::Text(_) => {}
                NativeNodeKind::Document => {
                    return self.layout_children(parent, x, y, available_width, depth);
                }
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(child) || self.document.is_hidden_for_layout(child) {
                        continue;
                    }
                    let style = self.document.computed_style_for_layout(child);
                    if self.effective_display(child) == DisplayValue::None {
                        continue;
                    }
                    let margin = style.margin();
                    let width = self.outer_width(child, style, false, available_width, !wrapped);
                    items.push(FlexItem {
                        child,
                        margin,
                        width,
                        order: style.flex_item_order().value(),
                        source_index,
                    });
                }
            }
        }

        items.sort_by_key(|item| (item.order, item.source_index));
        let mut lines = Vec::new();
        if wrapped {
            let mut current = Vec::new();
            let mut current_width = 0u32;
            for item in items {
                let item_outer_width = item.margin.horizontal().saturating_add(item.width);
                let separator_width = if current.is_empty() { 0 } else { gap };
                if !current.is_empty()
                    && current_width
                        .saturating_add(separator_width)
                        .saturating_add(item_outer_width)
                        > available_width
                {
                    lines.push(current);
                    current = Vec::new();
                    current_width = 0;
                }
                if !current.is_empty() {
                    current_width = current_width.saturating_add(gap);
                }
                current_width = current_width.saturating_add(item_outer_width);
                current.push(item);
            }
            if !current.is_empty() {
                lines.push(current);
            }
        } else {
            lines.push(items);
        }

        let line_count = lines.len();
        let reverse = parent_style.flex_direction() == FlexDirectionValue::RowReverse;
        let mut row_width = 0u32;
        let mut line_y = y;
        let mut line_records = Vec::with_capacity(line_count);
        for line_items in lines {
            let line_layout = self.layout_flex_line(
                line_items,
                FlexLineContext {
                    x,
                    y: line_y,
                    available_width,
                    gap,
                    justify_content,
                    reverse,
                    depth,
                },
            );
            row_width = row_width.max(line_layout.width);
            let auto_line_height = line_layout
                .placements
                .iter()
                .map(|placement| placement.height.saturating_add(placement.margin.vertical()))
                .max()
                .unwrap_or(0);
            let line_height = if wrapped {
                auto_line_height
            } else {
                explicit_line_height.unwrap_or(auto_line_height)
            };
            line_records.push(FlexLineRecord {
                y: line_y,
                provisional_y: line_y,
                height: line_height,
                layout: line_layout,
            });
            if wrapped {
                line_y = line_y.saturating_add(line_height);
            }
        }

        let total_line_height = line_records
            .iter()
            .fold(0u32, |total, line| total.saturating_add(line.height));
        let line_content_height = if wrapped {
            explicit_line_height.unwrap_or(total_line_height)
        } else {
            total_line_height
        };
        let free_space = line_content_height.saturating_sub(total_line_height);
        if wrapped
            && parent_style.align_content() == AlignContentValue::Stretch
            && free_space > 0
            && line_count > 0
        {
            let line_count_u32 = u32::try_from(line_count).unwrap_or(u32::MAX).max(1);
            let per_line_extra = free_space / line_count_u32;
            let remainder = free_space % line_count_u32;
            let mut stretched_line_y = y;
            for (line_index, line) in line_records.iter_mut().enumerate() {
                let line_index = u32::try_from(line_index).unwrap_or(u32::MAX);
                let extra = per_line_extra.saturating_add(u32::from(line_index < remainder));
                line.height = line.height.saturating_add(extra);
                line.y = stretched_line_y;
                stretched_line_y = stretched_line_y.saturating_add(line.height);
            }
        }
        let line_gap_count = u32::try_from(line_count.saturating_sub(1)).unwrap_or(u32::MAX);
        let distributed_line_gap =
            if wrapped && parent_style.align_content() == AlignContentValue::SpaceBetween {
                free_space.checked_div(line_gap_count).unwrap_or(0)
            } else {
                0
            };
        let distributed_remainder =
            if wrapped && parent_style.align_content() == AlignContentValue::SpaceBetween {
                free_space.checked_rem(line_gap_count).unwrap_or(0)
            } else {
                0
            };
        let leading_line_offset = if wrapped {
            match parent_style.align_content() {
                AlignContentValue::Center => free_space / 2,
                AlignContentValue::FlexEnd => free_space,
                AlignContentValue::FlexStart
                | AlignContentValue::SpaceBetween
                | AlignContentValue::SpaceAround
                | AlignContentValue::SpaceEvenly
                | AlignContentValue::Stretch => 0,
            }
        } else {
            0
        };
        let mut max_bottom = y;
        for (line_index, line) in line_records.into_iter().enumerate() {
            let line_index = u32::try_from(line_index).unwrap_or(u32::MAX);
            let line_offset = if wrapped
                && parent_style.align_content() == AlignContentValue::SpaceAround
            {
                flex_space_around_line_offset(free_space, line_index, line_count)
            } else if wrapped && parent_style.align_content() == AlignContentValue::SpaceEvenly {
                flex_space_evenly_line_offset(free_space, line_index, line_count)
            } else {
                leading_line_offset
                    .saturating_add(distributed_line_gap.saturating_mul(line_index))
                    .saturating_add(line_index.min(distributed_remainder))
            };
            let final_line_y = if wrap_reverse {
                y.saturating_add(line_content_height)
                    .saturating_sub(line.y.saturating_sub(y).saturating_add(line.height))
                    .saturating_sub(line_offset)
            } else {
                line.y.saturating_add(line_offset)
            };
            let line_shift = i64::from(final_line_y).saturating_sub(i64::from(line.provisional_y));
            max_bottom = max_bottom.max(final_line_y.saturating_add(line.height));
            for placement in line.layout.placements {
                let item_outer_height =
                    placement.height.saturating_add(placement.margin.vertical());
                let remaining = line.height.saturating_sub(item_outer_height);
                let offset = match align_items {
                    AlignItemsValue::FlexStart => 0,
                    AlignItemsValue::Center => remaining / 2,
                    AlignItemsValue::FlexEnd => remaining,
                };
                self.shift_layout_y(
                    placement.box_start,
                    placement.box_end,
                    placement.text_start,
                    placement.text_end,
                    line_shift.saturating_add(i64::from(offset)),
                );
                max_bottom = max_bottom.max(
                    final_line_y
                        .saturating_add(placement.margin.top())
                        .saturating_add(offset)
                        .saturating_add(placement.height)
                        .saturating_add(placement.margin.bottom()),
                );
            }
        }

        let width = if wrapped && line_count > 0 {
            available_width
        } else {
            row_width
        };
        FlowSize {
            width,
            height: max_bottom.saturating_sub(y),
        }
    }

    fn outer_width(
        &self,
        id: NativeNodeId,
        style: NativeComputedStyle,
        is_block: bool,
        available_width: u32,
        constrain_to_available: bool,
    ) -> u32 {
        let padding = style.padding();
        let (border_left, border_right) = style.border().map_or((0, 0), |border| {
            (border.left().width(), border.right().width())
        });
        let horizontal_inset = border_left
            .saturating_add(padding.left())
            .saturating_add(border_right)
            .saturating_add(padding.right());
        let default_outer_width = if is_block {
            available_width
        } else {
            self.intrinsic_inline_width(id, style)
                .saturating_add(horizontal_inset)
        };
        let width = style.width().map_or(default_outer_width, |declared| {
            if style.is_border_box() {
                declared
            } else {
                declared.saturating_add(horizontal_inset)
            }
        });
        let min_width = style.min_width().map(|declared| {
            if style.is_border_box() {
                declared
            } else {
                declared.saturating_add(horizontal_inset)
            }
        });
        let max_width = style.max_width().map(|declared| {
            if style.is_border_box() {
                declared
            } else {
                declared.saturating_add(horizontal_inset)
            }
        });
        let width = constrain_dimension(width, min_width, max_width);
        if constrain_to_available {
            width.min(available_width.max(min_width.unwrap_or_default()))
        } else {
            width
        }
    }

    fn place_text(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, value: &str) {
        match self
            .document
            .computed_style_for_layout(parent)
            .white_space()
        {
            WhiteSpaceValue::Normal => self.place_text_segment(parent, flow, value),
            WhiteSpaceValue::PreLine => self.place_pre_line_text(parent, flow, value),
            WhiteSpaceValue::Pre => self.place_preformatted_text(parent, flow, value, false),
            WhiteSpaceValue::PreWrap => self.place_preformatted_text(parent, flow, value, true),
            WhiteSpaceValue::NoWrap => self.place_text_segment(parent, flow, value),
        }
    }

    fn transform_text(&self, parent: NativeNodeId, value: &str) -> String {
        match self
            .document
            .computed_style_for_layout(parent)
            .text_transform()
        {
            TextTransformValue::None => value.to_owned(),
            TextTransformValue::Uppercase => value
                .chars()
                .map(|character| character.to_ascii_uppercase())
                .collect(),
            TextTransformValue::Lowercase => value
                .chars()
                .map(|character| character.to_ascii_lowercase())
                .collect(),
        }
    }

    fn place_pre_line_text(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, value: &str) {
        let mut segment_start = 0;
        let mut offset = 0;
        while offset < value.len() {
            let byte = value.as_bytes()[offset];
            if byte == b'\n' || byte == b'\r' {
                self.place_text_segment(parent, flow, &value[segment_start..offset]);
                let break_end = if byte == b'\r' && value.as_bytes().get(offset + 1) == Some(&b'\n')
                {
                    offset + 2
                } else {
                    offset + 1
                };
                self.force_line_break(flow);
                offset = break_end;
                segment_start = break_end;
            } else {
                let character = value[offset..].chars().next().unwrap_or_default();
                offset = offset.saturating_add(character.len_utf8());
            }
        }
        self.place_text_segment(parent, flow, &value[segment_start..]);
    }

    fn place_preformatted_text(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        value: &str,
        allow_soft_wrap: bool,
    ) {
        let _ = flow.take_pending_whitespace();
        let mut segment_start = 0;
        let mut offset = 0;
        while offset < value.len() {
            let byte = value.as_bytes()[offset];
            if byte == b'\n' || byte == b'\r' {
                self.place_preformatted_segment(
                    parent,
                    flow,
                    &value[segment_start..offset],
                    allow_soft_wrap,
                );
                let break_end = if byte == b'\r' && value.as_bytes().get(offset + 1) == Some(&b'\n')
                {
                    offset + 2
                } else {
                    offset + 1
                };
                self.force_line_break(flow);
                offset = break_end;
                segment_start = break_end;
            } else {
                let character = value[offset..].chars().next().unwrap_or_default();
                offset = offset.saturating_add(character.len_utf8());
            }
        }
        self.place_preformatted_segment(parent, flow, &value[segment_start..], allow_soft_wrap);
    }

    fn place_preformatted_segment(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        value: &str,
        allow_soft_wrap: bool,
    ) {
        let value = self.transform_text(parent, value);
        if value.is_empty() {
            return;
        }
        if allow_soft_wrap {
            self.place_preformatted_wrapped_segment(parent, flow, &value);
            return;
        }
        let width = flow.text_width(&value);
        let Some(origin) = flow.place_unwrapped_with_origin(width, DEFAULT_LINE_HEIGHT) else {
            return;
        };
        let text_index = self.text_runs.len();
        self.text_runs.push(NativeTextLayout {
            node_id: parent,
            origin,
            text: value,
            truncated: false,
        });
        self.paint_order
            .push(NativeLayoutPaintOrder::Text(text_index));
        flow.record_item(FlowItem {
            box_start: self.boxes.len(),
            box_end: self.boxes.len(),
            text_start: text_index,
            text_end: text_index.saturating_add(1),
            height: DEFAULT_LINE_HEIGHT,
            vertical_align: VerticalAlignValue::Baseline,
        });
    }

    fn place_preformatted_wrapped_segment(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        value: &str,
    ) {
        if flow.available_width == 0 {
            return;
        }
        let mut offset = 0;
        while offset < value.len() {
            if flow.line_has_content && flow.line_capacity_for_text(&value[offset..]) == 0 {
                self.flush_line(flow);
            }
            let chunk_length = flow.line_capacity_for_text(&value[offset..]).max(1);
            let mut end = offset;
            for (relative_offset, character) in value[offset..].char_indices().take(chunk_length) {
                end = offset + relative_offset + character.len_utf8();
            }
            if end == offset {
                break;
            }
            self.place_text_fragment(parent, flow, &value[offset..end], false);
            offset = end;
        }
    }

    fn place_text_segment(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, value: &str) {
        let value = self.transform_text(parent, value);
        let leading_whitespace = value.chars().next().is_some_and(char::is_whitespace);
        let trailing_whitespace = value.chars().next_back().is_some_and(char::is_whitespace);
        let pending_whitespace = flow.take_pending_whitespace();
        let (text, truncated) = NativeDocument::collapse_text_for_layout(&value);
        if text.is_empty() {
            if leading_whitespace || trailing_whitespace || pending_whitespace {
                flow.mark_pending_whitespace();
            }
            return;
        }
        if flow.available_width == 0 {
            return;
        }
        if !flow.allow_soft_wrap {
            let separator = (leading_whitespace || pending_whitespace) && flow.line_has_content;
            let fragment = if separator { format!(" {text}") } else { text };
            if self.should_apply_ellipsis(parent) {
                self.place_ellipsis_text(parent, flow, &fragment, truncated);
            } else {
                self.place_text_fragment(parent, flow, &fragment, truncated);
            }
            if trailing_whitespace {
                flow.mark_pending_whitespace();
            }
            return;
        }
        if self.document.computed_style_for_layout(parent).word_break() == WordBreakValue::BreakAll
        {
            self.place_break_all_text(
                parent,
                flow,
                &text,
                leading_whitespace,
                pending_whitespace,
                truncated,
            );
            if trailing_whitespace {
                flow.mark_pending_whitespace();
            }
            return;
        }
        let words = text
            .split(' ')
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>();
        for (word_index, word) in words.iter().enumerate() {
            let is_last_word = word_index + 1 == words.len();
            let separator = if word_index == 0 {
                leading_whitespace || pending_whitespace
            } else {
                true
            };
            let word_width = flow.text_width(word);
            if flow.line_has_content {
                let remaining_width = flow
                    .available_width
                    .saturating_sub(flow.x.saturating_sub(flow.start_x));
                let separator_width = if separator { flow.text_width(" ") } else { 0 };
                if separator_width.saturating_add(word_width) <= remaining_width {
                    let fragment = if separator {
                        format!(" {word}")
                    } else {
                        (*word).to_owned()
                    };
                    self.place_text_fragment(parent, flow, &fragment, truncated && is_last_word);
                    continue;
                }
                self.flush_line(flow);
            }
            self.place_word(parent, flow, word, is_last_word, truncated);
        }
        if trailing_whitespace {
            flow.mark_pending_whitespace();
        }
    }

    fn should_apply_ellipsis(&self, parent: NativeNodeId) -> bool {
        let style = self.document.computed_style_for_layout(parent);
        if self.effective_display(parent) != DisplayValue::Block
            || style.white_space() != WhiteSpaceValue::NoWrap
            || style.text_overflow() != TextOverflowValue::Ellipsis
            || !style.overflow_clip_x()
        {
            return false;
        }
        let Some(node) = self.document.node(parent) else {
            return false;
        };
        let children = node.children();
        children.len() == 1
            && self
                .document
                .node(children[0])
                .is_some_and(|child| matches!(child.kind(), NativeNodeKind::Text(_)))
    }

    fn place_ellipsis_text(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        text: &str,
        truncated: bool,
    ) {
        let remaining_width = flow
            .available_width
            .saturating_sub(flow.x.saturating_sub(flow.start_x));
        if flow.text_width(text) <= remaining_width {
            self.place_text_fragment(parent, flow, text, truncated);
            return;
        }

        const ELLIPSIS: &str = "...";
        let ellipsis_width = flow.text_width(ELLIPSIS);
        if ellipsis_width <= remaining_width {
            let prefix = self.text_prefix_for_width(
                text,
                remaining_width.saturating_sub(ellipsis_width),
                flow,
            );
            let fragment = format!("{}{}", prefix.trim_end_matches(' '), ELLIPSIS);
            self.place_text_fragment(parent, flow, &fragment, true);
            return;
        }

        let prefix = self.text_prefix_for_width(text, remaining_width, flow);
        let prefix = prefix.trim_end_matches(' ');
        if !prefix.is_empty() {
            self.place_text_fragment(parent, flow, prefix, true);
        }
    }

    fn text_prefix_for_width(&self, text: &str, available_width: u32, flow: &FlowCursor) -> String {
        let mut width: u32 = 0;
        let mut prefix = String::new();
        for character in text.chars() {
            let advance = flow.character_advance(character);
            if width.saturating_add(advance) > available_width {
                break;
            }
            width = width.saturating_add(advance);
            prefix.push(character);
        }
        prefix
    }

    fn place_break_all_text(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        text: &str,
        leading_whitespace: bool,
        pending_whitespace: bool,
        truncated: bool,
    ) {
        let words = text
            .split(' ')
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>();
        for (word_index, word) in words.iter().enumerate() {
            let separator = if word_index == 0 {
                leading_whitespace || pending_whitespace
            } else {
                true
            };
            let is_last_word = word_index + 1 == words.len();
            self.place_break_all_word(parent, flow, word, separator, truncated && is_last_word);
        }
    }

    fn place_break_all_word(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        word: &str,
        separator: bool,
        truncated: bool,
    ) {
        let characters = word.chars().collect::<Vec<_>>();
        let mut offset = 0;
        while offset < characters.len() {
            let include_separator = offset == 0 && separator && flow.line_has_content;
            let separator_width = if include_separator {
                flow.text_width(" ")
            } else {
                0
            };
            let remaining_width = flow
                .available_width
                .saturating_sub(flow.x.saturating_sub(flow.start_x));
            let first_advance = flow.character_advance(characters[offset]);
            if include_separator && separator_width.saturating_add(first_advance) > remaining_width
            {
                self.flush_line(flow);
                continue;
            }

            let mut width = separator_width;
            let mut count = 0;
            for &character in &characters[offset..] {
                let advance = flow.character_advance(character);
                if width.saturating_add(advance) > remaining_width {
                    break;
                }
                width = width.saturating_add(advance);
                count += 1;
            }
            if count == 0 {
                if flow.line_has_content {
                    self.flush_line(flow);
                    continue;
                }
                count = 1;
            }

            let mut fragment = String::new();
            if include_separator {
                fragment.push(' ');
            }
            fragment.extend(characters[offset..offset + count].iter().copied());
            let fragment_is_last = offset + count == characters.len();
            self.place_text_fragment(parent, flow, &fragment, truncated && fragment_is_last);
            offset += count;
            if !fragment_is_last {
                self.flush_line(flow);
            }
        }
    }

    fn place_pending_separator(&mut self, parent: NativeNodeId, flow: &mut FlowCursor) {
        if !flow.take_pending_whitespace() || !flow.line_has_content {
            return;
        }
        let remaining_width = flow
            .available_width
            .saturating_sub(flow.x.saturating_sub(flow.start_x));
        let separator_width = flow.text_width(" ");
        if separator_width > remaining_width {
            if !flow.allow_soft_wrap {
                self.place_text_fragment(parent, flow, " ", false);
                return;
            }
            self.flush_line(flow);
            return;
        }
        self.place_text_fragment(parent, flow, " ", false);
    }

    fn place_word(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        word: &str,
        is_last_word: bool,
        truncated: bool,
    ) {
        let word_width = flow.text_width(word);
        if word_width <= flow.available_width {
            self.place_text_fragment(parent, flow, word, truncated && is_last_word);
            return;
        }

        let characters = word.chars().collect::<Vec<_>>();
        let mut offset = 0;
        while offset < characters.len() {
            if flow.line_has_content {
                self.flush_line(flow);
            }
            let remaining = characters[offset..].iter().collect::<String>();
            let fragment_length = flow
                .line_capacity_for_text(&remaining)
                .max(1)
                .min(characters.len() - offset);
            let fragment = characters[offset..offset + fragment_length]
                .iter()
                .collect::<String>();
            let fragment_is_last = offset + fragment_length == characters.len();
            self.place_text_fragment(
                parent,
                flow,
                &fragment,
                truncated && is_last_word && fragment_is_last,
            );
            offset += fragment_length;
            if !fragment_is_last {
                self.flush_line(flow);
            }
        }
    }

    fn place_text_fragment(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        fragment: &str,
        truncated: bool,
    ) {
        let width = flow.text_width(fragment);
        let Some(origin) = flow.place_inline_with_origin(width, DEFAULT_LINE_HEIGHT) else {
            return;
        };
        let text_index = self.text_runs.len();
        self.text_runs.push(NativeTextLayout {
            node_id: parent,
            origin,
            text: fragment.to_owned(),
            truncated,
        });
        self.paint_order
            .push(NativeLayoutPaintOrder::Text(text_index));
        flow.record_item(FlowItem {
            box_start: self.boxes.len(),
            box_end: self.boxes.len(),
            text_start: text_index,
            text_end: text_index.saturating_add(1),
            height: DEFAULT_LINE_HEIGHT,
            vertical_align: VerticalAlignValue::Baseline,
        });
    }

    fn character_advance(character: char, letter_spacing: u32, word_spacing: u32) -> u32 {
        CHARACTER_WIDTH
            .saturating_add(letter_spacing)
            .saturating_add(if character == ' ' { word_spacing } else { 0 })
    }

    fn text_width_with_spacing(value: &str, letter_spacing: u32, word_spacing: u32) -> u32 {
        value.chars().fold(0, |width, character| {
            width.saturating_add(Self::character_advance(
                character,
                letter_spacing,
                word_spacing,
            ))
        })
    }

    fn intrinsic_inline_width(&self, id: NativeNodeId, style: NativeComputedStyle) -> u32 {
        let Some(node) = self.document.node(id) else {
            return 0;
        };
        match node.element_name() {
            Some("input" | "textarea" | "select") => DEFAULT_CONTROL_WIDTH,
            Some("button") => self
                .intrinsic_text_width(id, style)
                .saturating_add(24)
                .max(DEFAULT_BUTTON_WIDTH),
            Some("option") => self
                .intrinsic_text_width(id, style)
                .saturating_add(16)
                .max(DEFAULT_BUTTON_WIDTH),
            Some(_) => self.intrinsic_text_width(id, style).max(CHARACTER_WIDTH),
            None => 0,
        }
    }

    fn intrinsic_text_width(&self, id: NativeNodeId, style: NativeComputedStyle) -> u32 {
        let Some(value) = self.document.raw_text_for_layout(id) else {
            return 0;
        };
        let value = self.transform_text(id, &value);
        match style.white_space() {
            WhiteSpaceValue::Normal | WhiteSpaceValue::NoWrap => {
                let (value, _) = NativeDocument::collapse_text_for_layout(&value);
                Self::text_width_with_spacing(&value, style.letter_spacing(), style.word_spacing())
            }
            WhiteSpaceValue::PreLine => value
                .split(['\n', '\r'])
                .map(|line| {
                    let (line, _) = NativeDocument::collapse_text_for_layout(line);
                    Self::text_width_with_spacing(
                        &line,
                        style.letter_spacing(),
                        style.word_spacing(),
                    )
                })
                .max()
                .unwrap_or(0),
            WhiteSpaceValue::Pre | WhiteSpaceValue::PreWrap => value
                .split(['\n', '\r'])
                .map(|line| {
                    Self::text_width_with_spacing(
                        line,
                        style.letter_spacing(),
                        style.word_spacing(),
                    )
                })
                .max()
                .unwrap_or(0),
        }
    }

    fn intrinsic_inline_height(&self, id: NativeNodeId) -> u32 {
        match self.document.node(id).and_then(|node| node.element_name()) {
            Some("input" | "textarea" | "select" | "button") => DEFAULT_CONTROL_HEIGHT,
            _ => DEFAULT_LINE_HEIGHT,
        }
    }

    fn effective_display(&self, id: NativeNodeId) -> DisplayValue {
        let style = self.document.computed_style_for_layout(id);
        match style.display() {
            DisplayValue::Auto | DisplayValue::Other => self.default_display(id),
            display => display,
        }
    }

    fn is_non_rendered(&self, id: NativeNodeId) -> bool {
        self.document
            .node(id)
            .and_then(|node| node.element_name())
            .is_some_and(|name| matches!(name, "head" | "script" | "style" | "template" | "title"))
    }

    fn default_display(&self, id: NativeNodeId) -> DisplayValue {
        let Some(name) = self.document.node(id).and_then(|node| node.element_name()) else {
            return DisplayValue::Inline;
        };
        if matches!(
            name,
            "html"
                | "body"
                | "main"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "nav"
                | "aside"
                | "div"
                | "p"
                | "form"
                | "ul"
                | "ol"
                | "li"
                | "dl"
                | "dt"
                | "dd"
                | "fieldset"
                | "legend"
                | "blockquote"
                | "pre"
                | "table"
                | "thead"
                | "tbody"
                | "tfoot"
                | "tr"
                | "td"
                | "th"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "hr"
        ) {
            DisplayValue::Block
        } else {
            DisplayValue::Inline
        }
    }
}

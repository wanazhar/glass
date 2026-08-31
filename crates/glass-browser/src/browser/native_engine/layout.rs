use super::config::{MAX_NATIVE_DOM_DEPTH, Viewport};
use super::css::{DisplayValue, NativeBorderRadius, NativeComputedStyle, WhiteSpaceValue};
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
    Box(usize),
    Text(usize),
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

    /// Return the maximum root vertical scroll offset for this document.
    pub const fn max_scroll_offset(&self) -> NativePoint {
        NativePoint {
            x: 0,
            y: self.content_height.saturating_sub(self.viewport.height),
        }
    }

    pub(crate) fn with_scroll_offset(
        mut self,
        scroll_offset: NativePoint,
    ) -> Result<Self, NativeEngineError> {
        let max_scroll = self.max_scroll_offset();
        if scroll_offset.x != 0 {
            return Err(NativeEngineError::invalid(
                "native scroll offset",
                "horizontal scrolling is unsupported",
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
        if style.overflow_clip()
            && let Some(rect) = boxes
                .iter()
                .find(|layout_box| layout_box.node_id == current_id)
                .map(|layout_box| layout_box.rect)
        {
            clip = Some(match clip {
                Some(existing) => intersect_rect(existing, rect),
                None => rect,
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

struct FlowCursor {
    start_x: u32,
    start_y: u32,
    available_width: u32,
    x: u32,
    y: u32,
    minimum_line_height: u32,
    allow_soft_wrap: bool,
    line_height: u32,
    line_has_content: bool,
    pending_whitespace: bool,
    max_right: u32,
    max_bottom: u32,
}

impl FlowCursor {
    fn new(
        x: u32,
        y: u32,
        available_width: u32,
        minimum_line_height: u32,
        allow_soft_wrap: bool,
    ) -> Self {
        Self {
            start_x: x,
            start_y: y,
            available_width,
            x,
            y,
            minimum_line_height,
            allow_soft_wrap,
            line_height: 0,
            line_has_content: false,
            pending_whitespace: false,
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
        }
    }

    fn force_line_break(&mut self) {
        self.pending_whitespace = false;
        self.y = self
            .y
            .saturating_add(self.line_height.max(self.minimum_line_height));
        self.max_bottom = self
            .max_bottom
            .max(self.y.saturating_add(self.minimum_line_height));
        self.x = self.start_x;
        self.line_height = 0;
        self.line_has_content = false;
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

    fn place_inline(&mut self, width: u32, height: u32) {
        let _ = self.place_inline_with_origin(width, height);
    }

    fn place_inline_with_origin(&mut self, mut width: u32, height: u32) -> Option<NativePoint> {
        if self.available_width == 0 {
            return None;
        }
        if self.allow_soft_wrap {
            width = width.min(self.available_width);
        }
        if self.would_wrap(width) {
            self.flush_line();
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
        self.flush_line();
        self.y = self.y.saturating_add(height);
        self.max_bottom = self.max_bottom.max(self.y);
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
        let allow_soft_wrap = self
            .document
            .computed_style_for_layout(parent)
            .white_space()
            != WhiteSpaceValue::Pre;
        let mut flow = FlowCursor::new(x, y, available_width, minimum_line_height, allow_soft_wrap);
        self.process_children(parent, &mut flow, depth);
        let bottom = flow.max_bottom;
        let start_y = y;
        let mut result = flow.finish();
        result.height = bottom.saturating_sub(start_y).max(result.height);
        result
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
                            flow.force_line_break();
                        }
                        continue;
                    }
                    match display {
                        DisplayValue::None => {}
                        DisplayValue::Contents => self.process_children(child, flow, depth + 1),
                        DisplayValue::Block => {
                            flow.flush_line();
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
                                self.outer_width(child, style, false, available_width);
                            let candidate_width =
                                candidate_width.saturating_add(margin.horizontal());
                            let separator_width =
                                if flow.has_pending_whitespace() && flow.line_has_content {
                                    CHARACTER_WIDTH
                                } else {
                                    0
                                };
                            if flow.would_wrap(candidate_width.saturating_add(separator_width)) {
                                flow.flush_line();
                            } else {
                                self.place_pending_separator(parent, flow);
                            }
                            let size = self.layout_element(
                                child,
                                flow.x.saturating_add(margin.left()),
                                flow.y.saturating_add(margin.top()),
                                available_width,
                                depth,
                            );
                            flow.place_inline(
                                size.width.saturating_add(margin.horizontal()),
                                size.height.saturating_add(margin.vertical()),
                            );
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
            return self.layout_children(id, x, y, available_width, depth);
        }

        let is_block = display == DisplayValue::Block;
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
        let width = self.outer_width(id, style, is_block, available_width);
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
        let children = self.layout_children(
            id,
            x.saturating_add(left_inset),
            y.saturating_add(top_inset),
            content_width,
            depth + 1,
        );
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
        let content_height = height.saturating_sub(vertical_inset);
        self.boxes[box_index].rect.height = height;
        self.boxes[box_index].content_rect = NativeRect {
            x: x.saturating_add(left_inset),
            y: y.saturating_add(top_inset),
            width: content_width,
            height: content_height,
        };
        FlowSize { width, height }
    }

    fn outer_width(
        &self,
        id: NativeNodeId,
        style: NativeComputedStyle,
        is_block: bool,
        available_width: u32,
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
            self.intrinsic_inline_width(id)
                .saturating_add(horizontal_inset)
        };
        let width = style.width().map_or(default_outer_width, |declared| {
            if style.is_border_box() {
                declared
            } else {
                declared.saturating_add(horizontal_inset)
            }
        });
        width.min(available_width)
    }

    fn place_text(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, value: &str) {
        match self
            .document
            .computed_style_for_layout(parent)
            .white_space()
        {
            WhiteSpaceValue::Normal => self.place_text_segment(parent, flow, value),
            WhiteSpaceValue::PreLine => self.place_pre_line_text(parent, flow, value),
            WhiteSpaceValue::Pre => self.place_preformatted_text(parent, flow, value),
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
                flow.force_line_break();
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
    ) {
        let mut segment_start = 0;
        let mut offset = 0;
        while offset < value.len() {
            let byte = value.as_bytes()[offset];
            if byte == b'\n' || byte == b'\r' {
                self.place_preformatted_segment(parent, flow, &value[segment_start..offset]);
                let break_end = if byte == b'\r' && value.as_bytes().get(offset + 1) == Some(&b'\n')
                {
                    offset + 2
                } else {
                    offset + 1
                };
                flow.force_line_break();
                offset = break_end;
                segment_start = break_end;
            } else {
                let character = value[offset..].chars().next().unwrap_or_default();
                offset = offset.saturating_add(character.len_utf8());
            }
        }
        self.place_preformatted_segment(parent, flow, &value[segment_start..]);
    }

    fn place_preformatted_segment(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        value: &str,
    ) {
        if value.is_empty() {
            return;
        }
        let width = Self::text_width(value);
        let Some(origin) = flow.place_unwrapped_with_origin(width, DEFAULT_LINE_HEIGHT) else {
            return;
        };
        let text_index = self.text_runs.len();
        self.text_runs.push(NativeTextLayout {
            node_id: parent,
            origin,
            text: value.to_owned(),
            truncated: false,
        });
        self.paint_order
            .push(NativeLayoutPaintOrder::Text(text_index));
    }

    fn place_text_segment(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, value: &str) {
        let leading_whitespace = value.chars().next().is_some_and(char::is_whitespace);
        let trailing_whitespace = value.chars().next_back().is_some_and(char::is_whitespace);
        let pending_whitespace = flow.take_pending_whitespace();
        let (text, truncated) = NativeDocument::collapse_text_for_layout(value);
        if text.is_empty() {
            if leading_whitespace || trailing_whitespace || pending_whitespace {
                flow.mark_pending_whitespace();
            }
            return;
        }
        if flow.available_width == 0 {
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
            let word_width = Self::text_width(word);
            if flow.line_has_content {
                let remaining_width = flow
                    .available_width
                    .saturating_sub(flow.x.saturating_sub(flow.start_x));
                let separator_width = if separator { CHARACTER_WIDTH } else { 0 };
                if separator_width.saturating_add(word_width) <= remaining_width {
                    let fragment = if separator {
                        format!(" {word}")
                    } else {
                        (*word).to_owned()
                    };
                    self.place_text_fragment(parent, flow, &fragment, truncated && is_last_word);
                    continue;
                }
                flow.flush_line();
            }
            self.place_word(parent, flow, word, is_last_word, truncated);
        }
        if trailing_whitespace {
            flow.mark_pending_whitespace();
        }
    }

    fn place_pending_separator(&mut self, parent: NativeNodeId, flow: &mut FlowCursor) {
        if !flow.take_pending_whitespace() || !flow.line_has_content {
            return;
        }
        let remaining_width = flow
            .available_width
            .saturating_sub(flow.x.saturating_sub(flow.start_x));
        if CHARACTER_WIDTH > remaining_width {
            flow.flush_line();
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
        let word_width = Self::text_width(word);
        if word_width <= flow.available_width {
            self.place_text_fragment(parent, flow, word, truncated && is_last_word);
            return;
        }

        let characters = word.chars().collect::<Vec<_>>();
        let mut offset = 0;
        while offset < characters.len() {
            if flow.line_has_content {
                flow.flush_line();
            }
            let characters_on_line = usize::try_from(flow.available_width / CHARACTER_WIDTH)
                .unwrap_or_default()
                .max(1);
            let fragment_length = characters_on_line.min(characters.len() - offset);
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
                flow.flush_line();
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
        let width = Self::text_width(fragment);
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
    }

    fn text_width(value: &str) -> u32 {
        u32::try_from(value.chars().count())
            .unwrap_or(u32::MAX)
            .saturating_mul(CHARACTER_WIDTH)
    }

    fn intrinsic_inline_width(&self, id: NativeNodeId) -> u32 {
        let Some(node) = self.document.node(id) else {
            return 0;
        };
        match node.element_name() {
            Some("input" | "textarea" | "select") => DEFAULT_CONTROL_WIDTH,
            Some("button") => self
                .document
                .layout_text_width(id)
                .saturating_add(24)
                .max(DEFAULT_BUTTON_WIDTH),
            Some("option") => self
                .document
                .layout_text_width(id)
                .saturating_add(16)
                .max(DEFAULT_BUTTON_WIDTH),
            Some(_) => self
                .document
                .layout_text_width(id)
                .saturating_mul(CHARACTER_WIDTH)
                .max(CHARACTER_WIDTH),
            None => 0,
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

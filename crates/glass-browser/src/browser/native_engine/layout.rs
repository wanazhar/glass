use super::config::{MAX_NATIVE_DOM_DEPTH, Viewport};
use super::css::{
    AlignContentValue, AlignItemsValue, AlignSelfValue, DirectionValue, DisplayValue,
    FlexBasisValue, FlexDirectionValue, FlexWrapValue, JustifyContentValue, NativeAutoEdges,
    NativeBorderRadius, NativeBoxEdges, NativeComputedStyle, NativeGridTrack, NativeGridTrackList,
    NativePositionOffset, NativePositionValue, OverflowValue, TextAlignLastValue, TextAlignValue,
    TextJustifyValue, TextOverflowValue, TextTransformValue, VerticalAlignValue, WhiteSpaceValue,
    WordBreakValue,
};
use super::dom::{NativeDocument, NativeNode, NativeNodeId, NativeNodeKind};
use super::error::NativeEngineError;
use super::image::image_dimensions_from_source;
use std::collections::BTreeMap;

const DEFAULT_LINE_HEIGHT: u32 = 20;
const DEFAULT_CONTROL_HEIGHT: u32 = 24;
const DEFAULT_CONTROL_WIDTH: u32 = 160;
const DEFAULT_BUTTON_WIDTH: u32 = 80;
const DEFAULT_SVG_WIDTH: u32 = 300;
const DEFAULT_SVG_HEIGHT: u32 = 150;
const CHARACTER_WIDTH: u32 = 8;
pub(crate) const MAX_NATIVE_SVG_POINTS: usize = 2048;
const MAX_NATIVE_SVG_CURVE_SEGMENTS: usize = 16;
const MAX_NATIVE_SVG_ARC_SEGMENTS: usize = 64;

/// An integer-pixel point in the native viewport coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePoint {
    pub x: u32,
    pub y: u32,
}

/// A bounded SVG path subpath retained as shared layout/paint geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSvgSubpath {
    pub points: Vec<NativePoint>,
    pub closed: bool,
}

/// A bounded SVG affine transform in the SVG `(a b c d e f)` form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NativeSvgTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl NativeSvgTransform {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn translation(x: f64, y: f64) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }

    fn rotation(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            ..Self::IDENTITY
        }
    }

    fn followed_by(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    fn apply(self, point: (f64, f64)) -> Option<(f64, f64)> {
        let x = self.a * point.0 + self.c * point.1 + self.e;
        let y = self.b * point.0 + self.d * point.1 + self.f;
        (x.is_finite() && y.is_finite()).then_some((x, y))
    }

    pub(crate) fn is_identity(self) -> bool {
        self == Self::IDENTITY
    }
}

/// A half-open integer-pixel rectangle in the native viewport coordinate
/// space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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
    /// Whether this box belongs to a viewport-anchored fixed subtree.
    pub fixed: bool,
    /// Whether this box belongs to a flow-preserving sticky subtree.
    pub sticky: bool,
    /// The bounded effective stacking level used by paint and hit-testing.
    pub z_index: i32,
    /// Whether pointer hit testing may target this box.
    pub pointer_events: bool,
}

/// Layout-backed scroll metrics for one element with bounded overflow.
/// Coordinates remain in document space; only the scroll offset changes the
/// projection of descendants into the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeScrollContainer {
    pub(crate) node_id: NativeNodeId,
    pub(crate) client_width: u32,
    pub(crate) client_height: u32,
    pub(crate) scroll_width: u32,
    pub(crate) scroll_height: u32,
    pub(crate) scrollable_x: bool,
    pub(crate) scrollable_y: bool,
    pub(crate) scroll_offset: NativePoint,
}

impl NativeScrollContainer {
    pub(crate) const fn max_scroll_offset(self) -> NativePoint {
        NativePoint {
            x: if self.scrollable_x {
                self.scroll_width.saturating_sub(self.client_width)
            } else {
                0
            },
            y: if self.scrollable_y {
                self.scroll_height.saturating_sub(self.client_height)
            } else {
                0
            },
        }
    }
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
    /// Whether this text run is the first text item in its flushed line.
    pub starts_line: bool,
    /// Whether this text run is the last text item in its flushed line.
    pub ends_line: bool,
    /// Extra fixed-cell advance applied to each eligible ASCII separator by
    /// `text-align:justify` or `text-align-last:justify`, when
    /// `text-justify` permits expansion.
    pub justify_spacing: u32,
    /// Whether this text run belongs to a viewport-anchored fixed subtree.
    pub fixed: bool,
    /// Whether this text run belongs to a flow-preserving sticky subtree.
    pub sticky: bool,
    /// The bounded effective stacking level used by paint.
    pub z_index: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeLayoutPaintOrder {
    BeginOpacityGroup { node_id: NativeNodeId, opacity: u8 },
    Box(usize),
    Text(usize),
    EndOpacityGroup { node_id: NativeNodeId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeStickyProjection {
    box_start: usize,
    box_end: usize,
    text_start: usize,
    text_end: usize,
    normal_rect: NativeRect,
    containing_block: NativeRect,
    top: NativePositionOffset,
    right: NativePositionOffset,
    bottom: NativePositionOffset,
    left: NativePositionOffset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeStickyRange {
    root_box: usize,
    box_end: usize,
    text_start: usize,
    text_end: usize,
    top: NativePositionOffset,
    right: NativePositionOffset,
    bottom: NativePositionOffset,
    left: NativePositionOffset,
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
    sticky_projections: Vec<NativeStickyProjection>,
    scroll_containers: Vec<NativeScrollContainer>,
    nested_scroll_offsets: BTreeMap<u32, NativePoint>,
    projected_boxes: Vec<NativeRect>,
    projected_overflow_clips: Vec<Option<NativeRect>>,
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
            sticky_ranges: Vec::new(),
            initial_containing_block: PositionedContainingBlock {
                x: 0,
                y: 0,
                width: viewport.width,
                height: viewport.height,
            },
            fixed: false,
            sticky_root: None,
            stacking_context: 0,
            containing_block: PositionedContainingBlock {
                x: 0,
                y: 0,
                width: viewport.width,
                height: viewport.height,
            },
        };
        let flow = builder.layout_children(document.root(), 0, 0, viewport.width, 0);
        let scroll_containers = scroll_containers_for(document, &builder.boxes, &builder.text_runs);
        let max_box_bottom = builder
            .boxes
            .iter()
            .filter(|layout_box| {
                !layout_box.fixed
                    && !has_overflow_clip_ancestor(document, layout_box.node_id, false, false)
            })
            .map(|layout_box| layout_box.rect.bottom())
            .max()
            .unwrap_or(0);
        let max_box_right = builder
            .boxes
            .iter()
            .filter(|layout_box| {
                !layout_box.fixed
                    && !has_overflow_clip_ancestor(document, layout_box.node_id, false, true)
            })
            .map(|layout_box| layout_box.rect.right())
            .max()
            .unwrap_or(0);
        let max_text_right = builder
            .text_runs
            .iter()
            .filter(|text_run| {
                !text_run.fixed
                    && !has_overflow_clip_ancestor(document, text_run.node_id, true, true)
            })
            .filter_map(|text_run| {
                if text_run.truncated || text_run.text.is_empty() {
                    return None;
                }
                let text_rect = NativeRect {
                    x: text_run.origin.x,
                    y: text_run.origin.y,
                    width: LayoutBuilder::text_width_with_justification(
                        &text_run.text,
                        document
                            .computed_style_for_layout(text_run.node_id)
                            .letter_spacing(),
                        document
                            .computed_style_for_layout(text_run.node_id)
                            .word_spacing(),
                        text_run.justify_spacing,
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
        let content_height = viewport.height.max(flow.height).max(max_box_bottom);
        let content_width = viewport
            .width
            .max(flow.width)
            .max(max_box_right)
            .max(max_text_right);
        let sticky_projections = builder
            .sticky_ranges
            .iter()
            .filter_map(|range| {
                let root = builder.boxes.get(range.root_box)?;
                let containing_block =
                    nearest_layout_ancestor_rect(document, &builder.boxes, root.node_id).unwrap_or(
                        NativeRect {
                            x: 0,
                            y: 0,
                            width: content_width,
                            height: content_height,
                        },
                    );
                Some(NativeStickyProjection {
                    box_start: range.root_box,
                    box_end: range.box_end,
                    text_start: range.text_start,
                    text_end: range.text_end,
                    normal_rect: root.rect,
                    containing_block,
                    top: range.top,
                    right: range.right,
                    bottom: range.bottom,
                    left: range.left,
                })
            })
            .collect();
        let projected_boxes = builder
            .boxes
            .iter()
            .map(|layout_box| layout_box.rect)
            .collect();
        let projected_overflow_clips = builder
            .boxes
            .iter()
            .map(|layout_box| overflow_clip_for(document, &builder.boxes, layout_box.node_id))
            .collect();
        Ok(Self {
            revision: document.revision(),
            viewport,
            scroll_offset: NativePoint { x: 0, y: 0 },
            content_height,
            content_width,
            boxes: builder.boxes,
            text_runs: builder.text_runs,
            paint_order: builder.paint_order,
            overflow_clips,
            sticky_projections,
            scroll_containers,
            nested_scroll_offsets: BTreeMap::new(),
            projected_boxes,
            projected_overflow_clips,
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
        _document: &NativeDocument,
        node_id: NativeNodeId,
    ) -> Option<NativeRect> {
        let box_index = self
            .boxes
            .iter()
            .position(|layout_box| layout_box.node_id == node_id)?;
        self.projected_overflow_clips
            .get(box_index)
            .copied()
            .flatten()
    }

    pub(crate) fn scroll_container_for(
        &self,
        node_id: NativeNodeId,
    ) -> Option<NativeScrollContainer> {
        self.scroll_containers
            .iter()
            .find(|container| container.node_id == node_id)
            .copied()
    }

    pub(crate) fn nested_scroll_offset_for(
        &self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        include_self: bool,
    ) -> NativePoint {
        nested_scroll_offset_for_node(document, node_id, &self.nested_scroll_offsets, include_self)
    }

    /// Return a box projected into the current viewport, clipped at its
    /// visible viewport and bounded overflow edges. The layout box itself
    /// remains in document coordinates.
    pub fn viewport_rect_for(&self, node_id: NativeNodeId) -> Option<NativeRect> {
        self.viewport_projection_for(node_id).map(|(rect, _)| rect)
    }

    /// Return the visible viewport projection and the source-pixel offset
    /// within the un-clipped projected box. The offset keeps embedded frame
    /// pixels and pointer coordinates aligned when scrolling clips an owner.
    pub(crate) fn viewport_projection_for(
        &self,
        node_id: NativeNodeId,
    ) -> Option<(NativeRect, NativePoint)> {
        let (box_index, _) = self
            .boxes
            .iter()
            .enumerate()
            .find(|(_, layout_box)| layout_box.node_id == node_id)?;
        let projected_rect = self.projected_boxes.get(box_index).copied()?;
        let rect = self
            .projected_overflow_clips
            .get(box_index)
            .copied()
            .flatten()
            .map_or(projected_rect, |clip| intersect_rect(projected_rect, clip));
        let viewport_left = self.scroll_offset.x;
        let viewport_top = self.scroll_offset.y;
        let viewport_right = viewport_left.saturating_add(self.viewport.width);
        let viewport_bottom = viewport_top.saturating_add(self.viewport.height);
        let left = rect.x.max(viewport_left).min(viewport_right);
        let top = rect.y.max(viewport_top).min(viewport_bottom);
        let right = rect.right().min(viewport_right);
        let bottom = rect.bottom().min(viewport_bottom);
        (left < right && top < bottom).then_some((
            NativeRect {
                x: left.saturating_sub(viewport_left),
                y: top.saturating_sub(viewport_top),
                width: right.saturating_sub(left),
                height: bottom.saturating_sub(top),
            },
            NativePoint {
                x: left.saturating_sub(projected_rect.x),
                y: top.saturating_sub(projected_rect.y),
            },
        ))
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
        document: &NativeDocument,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
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
        let mut resolved_nested_scroll_offsets = BTreeMap::new();
        for container in &mut self.scroll_containers {
            let requested = nested_scroll_offsets
                .get(&container.node_id.index())
                .copied()
                .unwrap_or(NativePoint { x: 0, y: 0 });
            let max_scroll = container.max_scroll_offset();
            if requested.x > max_scroll.x || requested.y > max_scroll.y {
                return Err(NativeEngineError::invalid(
                    "native nested scroll offset",
                    "scroll offset exceeds the element's scroll bounds",
                ));
            }
            container.scroll_offset = requested;
            resolved_nested_scroll_offsets.insert(container.node_id.index(), requested);
        }
        self.nested_scroll_offsets = resolved_nested_scroll_offsets;
        for layout_box in &mut self.boxes {
            if layout_box.fixed {
                layout_box.rect.x = layout_box.rect.x.saturating_add(scroll_offset.x);
                layout_box.rect.y = layout_box.rect.y.saturating_add(scroll_offset.y);
                layout_box.content_rect.x =
                    layout_box.content_rect.x.saturating_add(scroll_offset.x);
                layout_box.content_rect.y =
                    layout_box.content_rect.y.saturating_add(scroll_offset.y);
            }
        }
        for text_run in &mut self.text_runs {
            if text_run.fixed {
                text_run.origin.x = text_run.origin.x.saturating_add(scroll_offset.x);
                text_run.origin.y = text_run.origin.y.saturating_add(scroll_offset.y);
            }
        }
        for projection in &self.sticky_projections {
            let delta_x = sticky_axis_delta(
                projection.normal_rect.x,
                projection.normal_rect.width,
                projection.containing_block.x,
                projection.containing_block.width,
                scroll_offset.x,
                self.viewport.width,
                projection.left,
                projection.right,
            );
            let delta_y = sticky_axis_delta(
                projection.normal_rect.y,
                projection.normal_rect.height,
                projection.containing_block.y,
                projection.containing_block.height,
                scroll_offset.y,
                self.viewport.height,
                projection.top,
                projection.bottom,
            );
            let box_end = projection.box_end.min(self.boxes.len());
            let box_start = projection.box_start.min(box_end);
            for layout_box in &mut self.boxes[box_start..box_end] {
                if layout_box.fixed {
                    continue;
                }
                layout_box.rect.x = shift_coordinate(layout_box.rect.x, delta_x);
                layout_box.rect.y = shift_coordinate(layout_box.rect.y, delta_y);
                layout_box.content_rect.x = shift_coordinate(layout_box.content_rect.x, delta_x);
                layout_box.content_rect.y = shift_coordinate(layout_box.content_rect.y, delta_y);
            }
            let text_end = projection.text_end.min(self.text_runs.len());
            let text_start = projection.text_start.min(text_end);
            for text_run in &mut self.text_runs[text_start..text_end] {
                if text_run.fixed {
                    continue;
                }
                text_run.origin.x = shift_coordinate(text_run.origin.x, delta_x);
                text_run.origin.y = shift_coordinate(text_run.origin.y, delta_y);
            }
        }
        self.overflow_clips = self
            .boxes
            .iter()
            .map(|layout_box| overflow_clip_for(document, &self.boxes, layout_box.node_id))
            .collect();
        self.projected_boxes = self
            .boxes
            .iter()
            .map(|layout_box| {
                if layout_box.fixed {
                    layout_box.rect
                } else {
                    project_rect_for_node(
                        document,
                        &self.boxes,
                        &self.nested_scroll_offsets,
                        layout_box.node_id,
                        layout_box.rect,
                        false,
                    )
                }
            })
            .collect();
        self.projected_overflow_clips = self
            .boxes
            .iter()
            .map(|layout_box| {
                overflow_clip_for_projected(
                    document,
                    &self.boxes,
                    &self.nested_scroll_offsets,
                    layout_box.node_id,
                )
            })
            .collect();
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

        let mut best: Option<(i32, usize, usize, NativeNodeId)> = None;
        for (order, layout_box) in self.boxes.iter().enumerate() {
            if let Some(clip) = self.projected_overflow_clips.get(order).copied().flatten()
                && !clip.contains(point)
            {
                continue;
            }
            if !layout_box.pointer_events {
                continue;
            }
            let Some(rect) = self.projected_boxes.get(order).copied() else {
                continue;
            };
            if !rounded_rect_contains(rect, layout_box.border_radius, point) {
                continue;
            }
            let replaces = best.is_none_or(|(best_z_index, best_depth, best_order, _)| {
                (layout_box.z_index, layout_box.depth, order)
                    > (best_z_index, best_depth, best_order)
            });
            if replaces {
                best = Some((
                    layout_box.z_index,
                    layout_box.depth,
                    order,
                    layout_box.node_id,
                ));
            }
        }
        Ok(best.map(|(_, _, _, node_id)| node_id))
    }
}

fn is_scroll_container(document: &NativeDocument, node_id: NativeNodeId) -> bool {
    let style = document.computed_style_for_layout(node_id);
    style.overflow_clip_x() || style.overflow_clip_y()
}

fn scrollable_overflow_axis(value: OverflowValue) -> bool {
    matches!(
        value,
        OverflowValue::Hidden | OverflowValue::Auto | OverflowValue::Scroll
    )
}

fn nearest_scroll_ancestor(
    document: &NativeDocument,
    node_id: NativeNodeId,
    include_self: bool,
) -> Option<NativeNodeId> {
    let mut current = if include_self {
        Some(node_id)
    } else {
        document.node(node_id).and_then(|node| node.parent())
    };
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let current_id = current?;
        if is_scroll_container(document, current_id) {
            return Some(current_id);
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    None
}

fn has_overflow_clip_ancestor(
    document: &NativeDocument,
    node_id: NativeNodeId,
    include_self: bool,
    horizontal: bool,
) -> bool {
    let mut current = if include_self {
        Some(node_id)
    } else {
        document.node(node_id).and_then(|node| node.parent())
    };
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let Some(current_id) = current else {
            break;
        };
        let style = document.computed_style_for_layout(current_id);
        if if horizontal {
            style.overflow_clip_x()
        } else {
            style.overflow_clip_y()
        } {
            return true;
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    false
}

fn nested_scroll_offset_for_node(
    document: &NativeDocument,
    node_id: NativeNodeId,
    offsets: &BTreeMap<u32, NativePoint>,
    include_self: bool,
) -> NativePoint {
    let mut current = if include_self {
        Some(node_id)
    } else {
        document.node(node_id).and_then(|node| node.parent())
    };
    let mut offset = NativePoint { x: 0, y: 0 };
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let Some(current_id) = current else {
            break;
        };
        if is_scroll_container(document, current_id)
            && let Some(current_offset) = offsets.get(&current_id.index())
        {
            offset.x = offset.x.saturating_add(current_offset.x);
            offset.y = offset.y.saturating_add(current_offset.y);
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    offset
}

fn project_rect_for_node(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    offsets: &BTreeMap<u32, NativePoint>,
    node_id: NativeNodeId,
    rect: NativeRect,
    include_self: bool,
) -> NativeRect {
    let nested_offset = boxes
        .iter()
        .find(|layout_box| layout_box.node_id == node_id)
        .filter(|layout_box| !layout_box.fixed)
        .map_or(NativePoint { x: 0, y: 0 }, |_| {
            nested_scroll_offset_for_node(document, node_id, offsets, include_self)
        });
    let left = i64::from(rect.x).saturating_sub(i64::from(nested_offset.x));
    let top = i64::from(rect.y).saturating_sub(i64::from(nested_offset.y));
    let right = i64::from(rect.right()).saturating_sub(i64::from(nested_offset.x));
    let bottom = i64::from(rect.bottom()).saturating_sub(i64::from(nested_offset.y));
    let visible_left = left.max(0);
    let visible_top = top.max(0);
    let visible_right = right.max(0);
    let visible_bottom = bottom.max(0);
    NativeRect {
        x: u32::try_from(visible_left).unwrap_or(u32::MAX),
        y: u32::try_from(visible_top).unwrap_or(u32::MAX),
        width: u32::try_from(visible_right.saturating_sub(visible_left)).unwrap_or(u32::MAX),
        height: u32::try_from(visible_bottom.saturating_sub(visible_top)).unwrap_or(u32::MAX),
    }
}

fn scroll_containers_for(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    text_runs: &[NativeTextLayout],
) -> Vec<NativeScrollContainer> {
    boxes
        .iter()
        .filter_map(|container_box| {
            let style = document.computed_style_for_layout(container_box.node_id);
            if !is_scroll_container(document, container_box.node_id) {
                return None;
            }
            let mut scroll_width = container_box.rect.width;
            let mut scroll_height = container_box.rect.height;
            for child_box in boxes.iter().filter(|child_box| {
                child_box.node_id != container_box.node_id
                    && !child_box.fixed
                    && nearest_scroll_ancestor(document, child_box.node_id, false)
                        == Some(container_box.node_id)
            }) {
                scroll_width =
                    scroll_width.max(child_box.rect.right().saturating_sub(container_box.rect.x));
                scroll_height =
                    scroll_height.max(child_box.rect.bottom().saturating_sub(container_box.rect.y));
            }
            for text_run in text_runs.iter().filter(|text_run| {
                !text_run.fixed
                    && nearest_scroll_ancestor(document, text_run.node_id, true)
                        == Some(container_box.node_id)
            }) {
                let text_width = LayoutBuilder::text_width_with_justification(
                    &text_run.text,
                    style.letter_spacing(),
                    style.word_spacing(),
                    text_run.justify_spacing,
                );
                scroll_width = scroll_width.max(
                    text_run
                        .origin
                        .x
                        .saturating_add(text_width)
                        .saturating_sub(container_box.rect.x),
                );
                scroll_height = scroll_height.max(
                    text_run
                        .origin
                        .y
                        .saturating_add(DEFAULT_LINE_HEIGHT)
                        .saturating_sub(container_box.rect.y),
                );
            }
            Some(NativeScrollContainer {
                node_id: container_box.node_id,
                client_width: container_box.rect.width,
                client_height: container_box.rect.height,
                scroll_width,
                scroll_height,
                scrollable_x: scrollable_overflow_axis(style.overflow_x()),
                scrollable_y: scrollable_overflow_axis(style.overflow_y()),
                scroll_offset: NativePoint { x: 0, y: 0 },
            })
        })
        .collect()
}

fn fixed_layout_root(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    node_id: NativeNodeId,
) -> Option<NativeNodeId> {
    let mut current = Some(node_id);
    let mut root = None;
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let Some(current_id) = current else {
            break;
        };
        if boxes
            .iter()
            .find(|layout_box| layout_box.node_id == current_id)
            .is_some_and(|layout_box| layout_box.fixed)
        {
            root = Some(current_id);
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    root
}

fn overflow_clip_for(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    node_id: NativeNodeId,
) -> Option<NativeRect> {
    overflow_clip_for_with_offsets(document, boxes, node_id, None)
}

fn overflow_clip_for_projected(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    offsets: &BTreeMap<u32, NativePoint>,
    node_id: NativeNodeId,
) -> Option<NativeRect> {
    overflow_clip_for_with_offsets(document, boxes, node_id, Some(offsets))
}

fn overflow_clip_for_with_offsets(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    node_id: NativeNodeId,
    offsets: Option<&BTreeMap<u32, NativePoint>>,
) -> Option<NativeRect> {
    let fixed_root = fixed_layout_root(document, boxes, node_id);
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
                .map(|layout_box| {
                    offsets.map_or(layout_box.rect, |offsets| {
                        project_rect_for_node(
                            document,
                            boxes,
                            offsets,
                            current_id,
                            layout_box.rect,
                            false,
                        )
                    })
                })
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
        if fixed_root == Some(current_id) {
            break;
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    match (
        clip,
        svg_viewport_clip_for_with_offsets(document, boxes, node_id, offsets),
    ) {
        (Some(first), Some(second)) => Some(intersect_rect(first, second)),
        (first, second) => first.or(second),
    }
}

fn svg_viewport_clip_for_with_offsets(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    node_id: NativeNodeId,
    offsets: Option<&BTreeMap<u32, NativePoint>>,
) -> Option<NativeRect> {
    let mut current = document.node(node_id).and_then(|node| node.parent());
    let mut clip = None;
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let Some(current_id) = current else {
            break;
        };
        if document
            .node(current_id)
            .and_then(|node| node.element_name())
            == Some("svg")
            && let Some(rect) = boxes
                .iter()
                .find(|layout_box| layout_box.node_id == current_id)
                .map(|layout_box| {
                    offsets.map_or(layout_box.rect, |offsets| {
                        project_rect_for_node(
                            document,
                            boxes,
                            offsets,
                            current_id,
                            layout_box.rect,
                            false,
                        )
                    })
                })
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

fn constrain_dimension(value: u32, minimum: Option<u32>, maximum: Option<u32>) -> u32 {
    let minimum = minimum.unwrap_or_default();
    let value = value.max(minimum);
    maximum.map_or(value, |maximum| value.min(maximum.max(minimum)))
}

fn shift_coordinate(value: u32, offset: i64) -> u32 {
    if offset.is_negative() {
        value.saturating_sub(u32::try_from(offset.unsigned_abs()).unwrap_or(u32::MAX))
    } else {
        value.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX))
    }
}

fn sticky_axis_delta(
    normal_start: u32,
    size: u32,
    containing_start: u32,
    containing_extent: u32,
    scroll_start: u32,
    viewport_extent: u32,
    leading: NativePositionOffset,
    trailing: NativePositionOffset,
) -> i64 {
    let normal_start = i64::from(normal_start);
    let size = i64::from(size);
    let containing_start = i64::from(containing_start);
    let containing_end = containing_start.saturating_add(i64::from(containing_extent));
    let scroll_start = i64::from(scroll_start);
    let viewport_end = scroll_start.saturating_add(i64::from(viewport_extent));
    let mut lower = containing_start.saturating_sub(normal_start);
    let mut upper = containing_end
        .saturating_sub(size)
        .saturating_sub(normal_start);
    if let Some(offset) = leading.length() {
        lower = lower.max(
            scroll_start
                .saturating_add(i64::from(offset))
                .saturating_sub(normal_start),
        );
    }
    if let Some(offset) = trailing.length() {
        upper = upper.min(
            viewport_end
                .saturating_sub(i64::from(offset))
                .saturating_sub(size)
                .saturating_sub(normal_start),
        );
    }
    if lower > upper {
        upper
    } else {
        0i64.clamp(lower, upper)
    }
}

fn nearest_layout_ancestor_rect(
    document: &NativeDocument,
    boxes: &[NativeLayoutBox],
    node_id: NativeNodeId,
) -> Option<NativeRect> {
    let mut current = document.node(node_id).and_then(|node| node.parent());
    for _ in 0..=MAX_NATIVE_DOM_DEPTH {
        let current_id = current?;
        if let Some(layout_box) = boxes
            .iter()
            .find(|layout_box| layout_box.node_id == current_id)
        {
            return Some(layout_box.rect);
        }
        current = document.node(current_id).and_then(|node| node.parent());
    }
    None
}

fn relative_offset(primary: NativePositionOffset, opposite: NativePositionOffset) -> i64 {
    primary
        .length()
        .map(i64::from)
        .or_else(|| opposite.length().map(|value| -i64::from(value)))
        .unwrap_or(0)
}

fn positioned_coordinate(
    origin: u32,
    extent: u32,
    size: u32,
    primary: NativePositionOffset,
    opposite: NativePositionOffset,
    leading_margin: u32,
    trailing_margin: u32,
) -> u32 {
    if let Some(offset) = primary.length() {
        shift_coordinate(origin.saturating_add(leading_margin), i64::from(offset))
    } else if let Some(offset) = opposite.length() {
        let base = origin
            .saturating_add(extent)
            .saturating_sub(size)
            .saturating_sub(trailing_margin);
        shift_coordinate(base, -i64::from(offset))
    } else {
        origin.saturating_add(leading_margin)
    }
}

fn outer_width_inset(style: NativeComputedStyle) -> u32 {
    let padding = style.padding();
    let (border_left, border_right) = style.border().map_or((0, 0), |border| {
        (border.left().width(), border.right().width())
    });
    border_left
        .saturating_add(padding.left())
        .saturating_add(border_right)
        .saturating_add(padding.right())
}

fn outer_width_from_declared(style: NativeComputedStyle, declared: u32) -> u32 {
    if style.is_border_box() {
        declared
    } else {
        declared.saturating_add(outer_width_inset(style))
    }
}

fn outer_height_inset(style: NativeComputedStyle) -> u32 {
    let padding = style.padding();
    let border_vertical = style.border().map_or(0, |border| {
        border.top().width().saturating_add(border.bottom().width())
    });
    padding.vertical().saturating_add(border_vertical)
}

fn outer_height_from_declared(style: NativeComputedStyle, declared: u32) -> u32 {
    if style.is_border_box() {
        declared
    } else {
        declared.saturating_add(outer_height_inset(style))
    }
}

fn stretched_outer_height(
    style: NativeComputedStyle,
    line_height: u32,
    margin: NativeBoxEdges,
    natural_height: u32,
) -> Option<u32> {
    if style.height().is_some() {
        return None;
    }
    let line_height = line_height.saturating_sub(margin.vertical());
    let target = natural_height.max(line_height);
    let min_height = style
        .min_height()
        .map(|declared| outer_height_from_declared(style, declared));
    let max_height = style
        .max_height()
        .map(|declared| outer_height_from_declared(style, declared));
    Some(constrain_dimension(target, min_height, max_height))
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
    sticky_ranges: Vec<NativeStickyRange>,
    initial_containing_block: PositionedContainingBlock,
    containing_block: PositionedContainingBlock,
    fixed: bool,
    sticky_root: Option<usize>,
    stacking_context: i32,
}

#[derive(Debug, Clone, Copy)]
struct PositionedContainingBlock {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct FlowSize {
    width: u32,
    height: u32,
}

struct FlexItemPlacement {
    child: NativeNodeId,
    box_start: usize,
    box_end: usize,
    text_start: usize,
    text_end: usize,
    margin: NativeBoxEdges,
    auto_margin: NativeAutoEdges,
    height: u32,
    align_self: AlignSelfValue,
}

struct FlexItem {
    child: NativeNodeId,
    margin: NativeBoxEdges,
    auto_margin: NativeAutoEdges,
    width: u32,
    height: u32,
    flex_base_width: u32,
    flex_base_height: u32,
    min_width: u32,
    min_height: u32,
    max_width: Option<u32>,
    max_height: Option<u32>,
    flex_grow: u32,
    flex_shrink: u32,
    order: i32,
    source_index: usize,
    align_self: AlignSelfValue,
}

#[derive(Debug, Clone, Copy, Default)]
struct ForcedOuterSize {
    width: Option<u32>,
    height: Option<u32>,
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

struct GridItemPlacement {
    child: NativeNodeId,
    box_start: usize,
    box_end: usize,
    text_start: usize,
    text_end: usize,
    column: usize,
    row: usize,
    margin: NativeBoxEdges,
    align_self: AlignSelfValue,
    width: u32,
    height: u32,
}

fn auto_margin_share(free_space: u32, slot: u32, count: u32) -> u32 {
    if count == 0 {
        return 0;
    }
    free_space
        .checked_div(count)
        .unwrap_or(0)
        .saturating_add(u32::from(slot < free_space.checked_rem(count).unwrap_or(0)))
}

fn horizontal_auto_margin_count(auto_margin: NativeAutoEdges) -> u32 {
    u32::from(auto_margin.left()) + u32::from(auto_margin.right())
}

fn vertical_auto_margin_count(auto_margin: NativeAutoEdges) -> u32 {
    u32::from(auto_margin.top()) + u32::from(auto_margin.bottom())
}

fn resolve_row_main_auto_margins(items: &mut [FlexItem], free_space: u32, reverse: bool) -> bool {
    let count = items.iter().fold(0u32, |total, item| {
        total.saturating_add(horizontal_auto_margin_count(item.auto_margin))
    });
    if count == 0 || free_space == 0 {
        return false;
    }
    let mut slot = 0;
    for item in items {
        if reverse {
            if item.auto_margin.right() {
                item.margin = item
                    .margin
                    .with_right(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
            if item.auto_margin.left() {
                item.margin = item
                    .margin
                    .with_left(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
        } else {
            if item.auto_margin.left() {
                item.margin = item
                    .margin
                    .with_left(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
            if item.auto_margin.right() {
                item.margin = item
                    .margin
                    .with_right(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
        }
    }
    true
}

fn resolve_column_main_auto_margins(
    items: &mut [FlexItem],
    free_space: u32,
    reverse: bool,
) -> bool {
    let count = items.iter().fold(0u32, |total, item| {
        total.saturating_add(vertical_auto_margin_count(item.auto_margin))
    });
    if count == 0 || free_space == 0 {
        return false;
    }
    let mut slot = 0;
    for item in items {
        if reverse {
            if item.auto_margin.bottom() {
                item.margin = item
                    .margin
                    .with_bottom(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
            if item.auto_margin.top() {
                item.margin = item
                    .margin
                    .with_top(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
        } else {
            if item.auto_margin.top() {
                item.margin = item
                    .margin
                    .with_top(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
            if item.auto_margin.bottom() {
                item.margin = item
                    .margin
                    .with_bottom(auto_margin_share(free_space, slot, count));
                slot = slot.saturating_add(1);
            }
        }
    }
    true
}

fn resolve_column_cross_auto_margins(item: &mut FlexItem, available_width: u32) -> bool {
    let count = horizontal_auto_margin_count(item.auto_margin);
    let free_space =
        available_width.saturating_sub(item.width.saturating_add(item.margin.horizontal()));
    if count == 0 || free_space == 0 {
        return false;
    }
    let mut slot = 0;
    if item.auto_margin.left() {
        item.margin = item
            .margin
            .with_left(auto_margin_share(free_space, slot, count));
        slot = slot.saturating_add(1);
    }
    if item.auto_margin.right() {
        item.margin = item
            .margin
            .with_right(auto_margin_share(free_space, slot, count));
    }
    true
}

struct FlexColumnLineContext {
    x: u32,
    y: u32,
    available_width: u32,
    available_height: u32,
    gap: u32,
    justify_content: JustifyContentValue,
    align_items: AlignItemsValue,
    cross_reverse: bool,
    reverse: bool,
    depth: usize,
}

struct FlexColumnWrapLine {
    items: Vec<FlexItem>,
    provisional_x: u32,
    width: u32,
}

fn flex_occupied_width(items: &[FlexItem], gap: u32) -> u32 {
    let gap_count = u32::try_from(items.len().saturating_sub(1)).unwrap_or(u32::MAX);
    let item_width = items.iter().fold(0u32, |total, item| {
        total
            .saturating_add(item.margin.horizontal())
            .saturating_add(item.width)
    });
    item_width.saturating_add(gap.saturating_mul(gap_count))
}

fn apply_flex_growth(items: &mut [FlexItem], available_width: u32, gap: u32) {
    let occupied_width = flex_occupied_width(items, gap);
    let mut remaining = available_width.saturating_sub(occupied_width);
    if remaining == 0 {
        return;
    }

    let mut active = vec![true; items.len()];
    while remaining > 0 {
        let total_weight = items
            .iter()
            .enumerate()
            .filter(|(index, item)| active[*index] && item.flex_grow > 0)
            .fold(0u64, |total, (_, item)| {
                total.saturating_add(u64::from(item.flex_grow))
            });
        if total_weight == 0 {
            break;
        }

        let mut cumulative_weight = 0u64;
        let mut previous_target = 0u64;
        let mut applied = 0u32;
        let mut froze_item = false;
        for (index, item) in items.iter_mut().enumerate() {
            if !active[index] || item.flex_grow == 0 {
                continue;
            }
            cumulative_weight = cumulative_weight.saturating_add(u64::from(item.flex_grow));
            let target = u64::from(remaining).saturating_mul(cumulative_weight) / total_weight;
            let share = u32::try_from(target.saturating_sub(previous_target)).unwrap_or(u32::MAX);
            previous_target = target;
            let capacity = item
                .max_width
                .map_or(u32::MAX, |max_width| max_width.saturating_sub(item.width));
            let growth = share.min(capacity);
            item.width = item.width.saturating_add(growth);
            applied = applied.saturating_add(growth);
            if item
                .max_width
                .is_some_and(|max_width| item.width >= max_width)
            {
                active[index] = false;
                froze_item = true;
            }
        }

        remaining = remaining.saturating_sub(applied);
        if !froze_item {
            break;
        }
        if applied == 0 && active.iter().all(|is_active| !*is_active) {
            break;
        }
    }
}

fn apply_flex_shrink(items: &mut [FlexItem], available_width: u32, gap: u32) {
    let occupied_width = flex_occupied_width(items, gap);
    let mut remaining = occupied_width.saturating_sub(available_width);
    if remaining == 0 {
        return;
    }

    let mut active = vec![true; items.len()];
    while remaining > 0 {
        let total_weight = items
            .iter()
            .enumerate()
            .filter(|(index, item)| {
                active[*index] && item.flex_shrink > 0 && item.flex_base_width > 0
            })
            .fold(0u64, |total, (_, item)| {
                let weight =
                    u64::from(item.flex_base_width).saturating_mul(u64::from(item.flex_shrink));
                total.saturating_add(weight)
            });
        if total_weight == 0 {
            break;
        }

        let mut cumulative_weight = 0u64;
        let mut previous_target = 0u64;
        let mut applied = 0u32;
        let mut froze_item = false;
        for (index, item) in items.iter_mut().enumerate() {
            if !active[index] || item.flex_shrink == 0 || item.flex_base_width == 0 {
                continue;
            }
            let weight =
                u64::from(item.flex_base_width).saturating_mul(u64::from(item.flex_shrink));
            cumulative_weight = cumulative_weight.saturating_add(weight);
            let target = u64::from(remaining).saturating_mul(cumulative_weight) / total_weight;
            let share = u32::try_from(target.saturating_sub(previous_target)).unwrap_or(u32::MAX);
            previous_target = target;
            let capacity = item.width.saturating_sub(item.min_width);
            let reduction = share.min(capacity);
            item.width = item.width.saturating_sub(reduction);
            applied = applied.saturating_add(reduction);
            if item.width <= item.min_width {
                active[index] = false;
                froze_item = true;
            }
        }

        remaining = remaining.saturating_sub(applied);
        if !froze_item {
            break;
        }
        if applied == 0 && active.iter().all(|is_active| !*is_active) {
            break;
        }
    }
}

fn flex_occupied_height(items: &[FlexItem], gap: u32) -> u32 {
    let gap_count = u32::try_from(items.len().saturating_sub(1)).unwrap_or(u32::MAX);
    let item_height = items.iter().fold(0u32, |total, item| {
        total
            .saturating_add(item.margin.vertical())
            .saturating_add(item.height)
    });
    item_height.saturating_add(gap.saturating_mul(gap_count))
}

fn apply_flex_growth_height(items: &mut [FlexItem], available_height: u32, gap: u32) {
    let occupied_height = flex_occupied_height(items, gap);
    let mut remaining = available_height.saturating_sub(occupied_height);
    if remaining == 0 {
        return;
    }

    let mut active = vec![true; items.len()];
    while remaining > 0 {
        let total_weight = items
            .iter()
            .enumerate()
            .filter(|(index, item)| active[*index] && item.flex_grow > 0)
            .fold(0u64, |total, (_, item)| {
                total.saturating_add(u64::from(item.flex_grow))
            });
        if total_weight == 0 {
            break;
        }

        let mut cumulative_weight = 0u64;
        let mut previous_target = 0u64;
        let mut applied = 0u32;
        let mut froze_item = false;
        for (index, item) in items.iter_mut().enumerate() {
            if !active[index] || item.flex_grow == 0 {
                continue;
            }
            cumulative_weight = cumulative_weight.saturating_add(u64::from(item.flex_grow));
            let target = u64::from(remaining).saturating_mul(cumulative_weight) / total_weight;
            let share = u32::try_from(target.saturating_sub(previous_target)).unwrap_or(u32::MAX);
            previous_target = target;
            let capacity = item.max_height.map_or(u32::MAX, |max_height| {
                max_height.saturating_sub(item.height)
            });
            let growth = share.min(capacity);
            item.height = item.height.saturating_add(growth);
            applied = applied.saturating_add(growth);
            if item
                .max_height
                .is_some_and(|max_height| item.height >= max_height)
            {
                active[index] = false;
                froze_item = true;
            }
        }

        remaining = remaining.saturating_sub(applied);
        if !froze_item {
            break;
        }
        if applied == 0 && active.iter().all(|is_active| !*is_active) {
            break;
        }
    }
}

fn apply_flex_shrink_height(items: &mut [FlexItem], available_height: u32, gap: u32) {
    let occupied_height = flex_occupied_height(items, gap);
    let mut remaining = occupied_height.saturating_sub(available_height);
    if remaining == 0 {
        return;
    }

    let mut active = vec![true; items.len()];
    while remaining > 0 {
        let total_weight = items
            .iter()
            .enumerate()
            .filter(|(index, item)| {
                active[*index] && item.flex_shrink > 0 && item.flex_base_height > 0
            })
            .fold(0u64, |total, (_, item)| {
                let weight =
                    u64::from(item.flex_base_height).saturating_mul(u64::from(item.flex_shrink));
                total.saturating_add(weight)
            });
        if total_weight == 0 {
            break;
        }

        let mut cumulative_weight = 0u64;
        let mut previous_target = 0u64;
        let mut applied = 0u32;
        let mut froze_item = false;
        for (index, item) in items.iter_mut().enumerate() {
            if !active[index] || item.flex_shrink == 0 || item.flex_base_height == 0 {
                continue;
            }
            let weight =
                u64::from(item.flex_base_height).saturating_mul(u64::from(item.flex_shrink));
            cumulative_weight = cumulative_weight.saturating_add(weight);
            let target = u64::from(remaining).saturating_mul(cumulative_weight) / total_weight;
            let share = u32::try_from(target.saturating_sub(previous_target)).unwrap_or(u32::MAX);
            previous_target = target;
            let capacity = item.height.saturating_sub(item.min_height);
            let reduction = share.min(capacity);
            item.height = item.height.saturating_sub(reduction);
            applied = applied.saturating_add(reduction);
            if item.height <= item.min_height {
                active[index] = false;
                froze_item = true;
            }
        }

        remaining = remaining.saturating_sub(applied);
        if !froze_item {
            break;
        }
        if applied == 0 && active.iter().all(|is_active| !*is_active) {
            break;
        }
    }
}

fn flex_space_around_offset(free_space: u32, item_index: u32, item_count: usize) -> u32 {
    let numerator = u64::from(free_space)
        .saturating_mul(u64::from(item_index).saturating_mul(2).saturating_add(1));
    let denominator = u64::try_from(item_count)
        .unwrap_or(u64::MAX)
        .saturating_mul(2)
        .max(1);
    u32::try_from(numerator / denominator).unwrap_or(u32::MAX)
}

fn flex_space_evenly_offset(free_space: u32, index: u32, count: usize) -> u32 {
    let numerator = u64::from(free_space).saturating_mul(u64::from(index).saturating_add(1));
    let denominator = u64::try_from(count)
        .unwrap_or(u64::MAX)
        .saturating_add(1)
        .max(1);
    u32::try_from(numerator / denominator).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy)]
struct FlowStyle {
    minimum_line_height: u32,
    direction: DirectionValue,
    text_align: TextAlignValue,
    text_align_last: TextAlignLastValue,
    justify_enabled: bool,
    final_justify_enabled: bool,
    allow_soft_wrap: bool,
    text_indent: u32,
    word_spacing: u32,
    letter_spacing: u32,
    owns_line_boundaries: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowFlushReason {
    Normal,
    SoftWrap,
    Final,
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
    direction: DirectionValue,
    text_align: TextAlignValue,
    text_align_last: TextAlignLastValue,
    justify_enabled: bool,
    final_justify_enabled: bool,
    allow_soft_wrap: bool,
    line_height: u32,
    line_has_content: bool,
    pending_whitespace: bool,
    word_spacing: u32,
    letter_spacing: u32,
    owns_line_boundaries: bool,
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
            direction: style.direction,
            text_align: style.text_align,
            text_align_last: style.text_align_last,
            justify_enabled: style.justify_enabled,
            final_justify_enabled: style.final_justify_enabled,
            allow_soft_wrap: style.allow_soft_wrap,
            line_height: 0,
            line_has_content: false,
            pending_whitespace: false,
            word_spacing: style.word_spacing,
            letter_spacing: style.letter_spacing,
            owns_line_boundaries: style.owns_line_boundaries,
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

    fn alignment_offset_for(&self, text_align: TextAlignValue) -> u32 {
        let remaining = self
            .available_width
            .saturating_sub(self.x.saturating_sub(self.start_x));
        match text_align {
            TextAlignValue::Left => 0,
            TextAlignValue::Center => remaining / 2,
            TextAlignValue::Right => remaining,
            TextAlignValue::Start => {
                if self.direction == DirectionValue::Rtl {
                    remaining
                } else {
                    0
                }
            }
            TextAlignValue::End => {
                if self.direction == DirectionValue::Rtl {
                    0
                } else {
                    remaining
                }
            }
            TextAlignValue::Justify => 0,
        }
    }

    fn alignment_offset(&self) -> u32 {
        self.alignment_offset_for(self.text_align)
    }

    fn final_alignment_offset(&self) -> u32 {
        self.alignment_offset_for(self.text_align_last.resolve(self.text_align))
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
        let direction = style.direction();
        let text_align = style.text_align();
        let text_align_last = style.text_align_last();
        let text_justify = style.text_justify();
        let text_indent = if self.effective_display(parent) == DisplayValue::Block {
            style.text_indent()
        } else {
            0
        };
        let word_spacing = style.word_spacing();
        let letter_spacing = style.letter_spacing();
        let allow_soft_wrap =
            !matches!(white_space, WhiteSpaceValue::Pre | WhiteSpaceValue::NoWrap);
        let justify_enabled = text_align == TextAlignValue::Justify
            && text_justify != TextJustifyValue::None
            && matches!(
                white_space,
                WhiteSpaceValue::Normal | WhiteSpaceValue::PreLine
            )
            && style.word_break() == WordBreakValue::Normal;
        let final_justify_enabled = text_align_last == TextAlignLastValue::Justify
            && text_justify != TextJustifyValue::None
            && matches!(
                white_space,
                WhiteSpaceValue::Normal | WhiteSpaceValue::PreLine
            )
            && style.word_break() == WordBreakValue::Normal;
        let owns_line_boundaries = parent == self.document.root()
            || matches!(
                self.effective_display(parent),
                DisplayValue::Block | DisplayValue::Flex | DisplayValue::Grid
            );
        let mut flow = FlowCursor::new(
            x,
            y,
            available_width,
            FlowStyle {
                minimum_line_height,
                direction,
                text_align,
                text_align_last,
                justify_enabled,
                final_justify_enabled,
                allow_soft_wrap,
                text_indent,
                word_spacing,
                letter_spacing,
                owns_line_boundaries,
            },
        );
        let containing_block = self.containing_block;
        self.process_children(parent, &mut flow, depth, containing_block);
        self.flush_final_line(&mut flow);
        let bottom = flow.max_bottom;
        let start_y = y;
        let mut result = flow.finish();
        result.height = bottom.saturating_sub(start_y).max(result.height);
        result
    }

    fn flush_line(&mut self, flow: &mut FlowCursor) {
        self.flush_line_with_reason(flow, FlowFlushReason::Normal);
    }

    fn flush_line_after_soft_wrap(&mut self, flow: &mut FlowCursor) {
        self.flush_line_with_reason(flow, FlowFlushReason::SoftWrap);
    }

    fn flush_final_line(&mut self, flow: &mut FlowCursor) {
        self.flush_line_with_reason(flow, FlowFlushReason::Final);
    }

    fn flush_line_with_reason(&mut self, flow: &mut FlowCursor, reason: FlowFlushReason) {
        if flow.line_has_content {
            let line_items = flow.take_line_items();
            if flow.owns_line_boundaries {
                let text_indices = line_items
                    .iter()
                    .flat_map(|item| {
                        let end = item.text_end.min(self.text_runs.len());
                        let start = item.text_start.min(end);
                        start..end
                    })
                    .collect::<Vec<_>>();
                if let Some(first_text_index) = text_indices.first().copied()
                    && let Some(text_run) = self.text_runs.get_mut(first_text_index)
                {
                    text_run.starts_line = true;
                }
                if let Some(last_text_index) = text_indices.last().copied()
                    && let Some(text_run) = self.text_runs.get_mut(last_text_index)
                {
                    text_run.ends_line = true;
                }
            }
            let justification_enabled = match reason {
                FlowFlushReason::SoftWrap => flow.justify_enabled,
                FlowFlushReason::Final => flow.final_justify_enabled,
                FlowFlushReason::Normal => false,
            };
            let eligible_spaces = if justification_enabled {
                line_items
                    .iter()
                    .filter(|item| self.justifiable_text_run(**item, reason).is_some())
                    .count()
            } else {
                0
            };
            let free_space = if eligible_spaces > 0 {
                flow.available_width
                    .saturating_sub(flow.x.saturating_sub(flow.start_x))
            } else {
                0
            };
            let spaces = u32::try_from(eligible_spaces).unwrap_or(u32::MAX);
            let extra_per_space = free_space.checked_div(spaces).unwrap_or(0);
            let extra_remainder = free_space.checked_rem(spaces).unwrap_or(0);
            let offset = if reason == FlowFlushReason::Final {
                flow.final_alignment_offset()
            } else {
                flow.alignment_offset()
            };
            let line_height = flow.line_height.max(flow.minimum_line_height);
            let mut extra_width = 0;
            let mut space_index = 0;
            for item in line_items {
                let vertical_offset = item.vertical_offset(line_height);
                let item_shift = offset.saturating_add(extra_width);
                let box_end = item.box_end.min(self.boxes.len());
                let box_start = item.box_start.min(box_end);
                for layout_box in &mut self.boxes[box_start..box_end] {
                    layout_box.rect.x = layout_box.rect.x.saturating_add(item_shift);
                    layout_box.rect.y = layout_box.rect.y.saturating_add(vertical_offset);
                    layout_box.content_rect.x =
                        layout_box.content_rect.x.saturating_add(item_shift);
                    layout_box.content_rect.y =
                        layout_box.content_rect.y.saturating_add(vertical_offset);
                }

                let text_end = item.text_end.min(self.text_runs.len());
                let text_start = item.text_start.min(text_end);
                for text_run in &mut self.text_runs[text_start..text_end] {
                    text_run.origin.x = text_run.origin.x.saturating_add(item_shift);
                    text_run.origin.y = text_run.origin.y.saturating_add(vertical_offset);
                }
                let justified_text = self.justifiable_text_run(item, reason);
                if let Some(text_index) = justified_text {
                    let extra =
                        extra_per_space.saturating_add(u32::from(space_index < extra_remainder));
                    if let Some(text_run) = self.text_runs.get_mut(text_index) {
                        text_run.justify_spacing = extra;
                    }
                    extra_width = extra_width.saturating_add(extra);
                    space_index = space_index.saturating_add(1);
                }
            }
            let line_right = flow.x.saturating_add(offset).saturating_add(extra_width);
            flow.max_right = flow.max_right.max(line_right);
        }
        flow.flush_line();
    }

    fn justifiable_text_run(&self, item: FlowItem, reason: FlowFlushReason) -> Option<usize> {
        if item.box_start != item.box_end || item.text_end != item.text_start.saturating_add(1) {
            return None;
        }
        let text_index = item.text_start;
        let text_run = self.text_runs.get(text_index)?;
        if text_run.truncated
            || text_run
                .text
                .chars()
                .filter(|character| *character == ' ')
                .count()
                != 1
        {
            return None;
        }
        let style = self.document.computed_style_for_layout(text_run.node_id);
        let alignment = match reason {
            FlowFlushReason::SoftWrap => {
                style.text_align() == TextAlignValue::Justify
                    && style.text_justify() != TextJustifyValue::None
            }
            FlowFlushReason::Final => {
                style.text_align_last() == TextAlignLastValue::Justify
                    && style.text_justify() != TextJustifyValue::None
            }
            FlowFlushReason::Normal => false,
        };
        (alignment
            && matches!(
                style.white_space(),
                WhiteSpaceValue::Normal | WhiteSpaceValue::PreLine
            )
            && style.word_break() == WordBreakValue::Normal)
            .then_some(text_index)
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

    fn process_children(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        depth: usize,
        containing_block: PositionedContainingBlock,
    ) {
        let children = self
            .document
            .node(parent)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();
        let mut positioned_children = Vec::new();
        for child in children {
            let Some(node) = self.document.node(child) else {
                continue;
            };
            match node.kind() {
                NativeNodeKind::Text(value) => {
                    let value = value.clone();
                    self.place_text(parent, flow, &value);
                }
                NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
                NativeNodeKind::Document => {
                    self.process_children(child, flow, depth, containing_block);
                }
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(child) || self.document.is_hidden_for_layout(child) {
                        continue;
                    }
                    if self.is_out_of_flow(child) {
                        positioned_children.push(child);
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
                            self.process_children(child, flow, depth + 1, containing_block);
                            if grouped {
                                self.paint_order
                                    .push(NativeLayoutPaintOrder::EndOpacityGroup {
                                        node_id: child,
                                    });
                            }
                        }
                        DisplayValue::Block | DisplayValue::Flex | DisplayValue::Grid => {
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
                            let candidate_width = self.outer_width(
                                child,
                                style,
                                false,
                                available_width,
                                !self.explicit_width_can_overflow(child),
                            );
                            let candidate_width =
                                candidate_width.saturating_add(margin.horizontal());
                            let separator_width =
                                if flow.has_pending_whitespace() && flow.line_has_content {
                                    flow.text_width(" ")
                                } else {
                                    0
                                };
                            if flow.would_wrap(candidate_width.saturating_add(separator_width)) {
                                self.flush_line_after_soft_wrap(flow);
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
        self.layout_positioned_children(positioned_children, containing_block, depth);
    }

    fn layout_element(
        &mut self,
        id: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        self.layout_element_with_outer_width(
            id,
            x,
            y,
            available_width,
            depth,
            ForcedOuterSize::default(),
        )
    }

    fn is_out_of_flow(&self, id: NativeNodeId) -> bool {
        matches!(
            self.document.computed_style_for_layout(id).position(),
            NativePositionValue::Absolute | NativePositionValue::Fixed
        )
    }

    fn explicit_width_can_overflow(&self, id: NativeNodeId) -> bool {
        let mut current = self.document.node(id).and_then(|node| node.parent());
        for _ in 0..=MAX_NATIVE_DOM_DEPTH {
            let Some(current_id) = current else {
                break;
            };
            let style = self.document.computed_style_for_layout(current_id);
            if matches!(
                style.overflow_x(),
                OverflowValue::Auto | OverflowValue::Scroll
            ) {
                return true;
            }
            current = self
                .document
                .node(current_id)
                .and_then(|node| node.parent());
        }
        false
    }

    fn positioned_children(&self, parent: NativeNodeId) -> Vec<NativeNodeId> {
        self.document
            .node(parent)
            .map(|node| {
                node.children()
                    .iter()
                    .copied()
                    .filter(|child| {
                        self.document.node(*child).is_some_and(|node| {
                            matches!(node.kind(), NativeNodeKind::Element { .. })
                        }) && self.is_out_of_flow(*child)
                            && !self.is_non_rendered(*child)
                            && !self.document.is_hidden_for_layout(*child)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn layout_positioned_children(
        &mut self,
        children: Vec<NativeNodeId>,
        containing_block: PositionedContainingBlock,
        depth: usize,
    ) {
        for child in children {
            let style = self.document.computed_style_for_layout(child);
            let containing_block = if style.position() == NativePositionValue::Fixed {
                self.initial_containing_block
            } else {
                containing_block
            };
            let display = self.effective_display(child);
            if display == DisplayValue::None
                || self.is_non_rendered(child)
                || self.document.is_hidden_for_layout(child)
            {
                continue;
            }
            let is_block = matches!(
                display,
                DisplayValue::Block | DisplayValue::Flex | DisplayValue::Grid
            );
            let margin = style.margin();
            let available_width = containing_block.width.saturating_sub(margin.horizontal());
            let width = self.outer_width(
                child,
                style,
                is_block,
                available_width,
                !self.explicit_width_can_overflow(child),
            );
            let box_start = self.boxes.len();
            let text_start = self.text_runs.len();
            let initial_x = containing_block.x.saturating_add(margin.left());
            let initial_y = containing_block.y.saturating_add(margin.top());
            let size = self.layout_element_with_outer_width(
                child,
                initial_x,
                initial_y,
                available_width,
                depth,
                ForcedOuterSize {
                    width: Some(width),
                    ..ForcedOuterSize::default()
                },
            );
            let final_x = positioned_coordinate(
                containing_block.x,
                containing_block.width,
                size.width,
                style.left(),
                style.right(),
                margin.left(),
                margin.right(),
            );
            let final_y = positioned_coordinate(
                containing_block.y,
                containing_block.height,
                size.height,
                style.top(),
                style.bottom(),
                margin.top(),
                margin.bottom(),
            );
            self.shift_layout(
                box_start,
                self.boxes.len(),
                text_start,
                self.text_runs.len(),
                i64::from(final_x).saturating_sub(i64::from(initial_x)),
                i64::from(final_y).saturating_sub(i64::from(initial_y)),
            );
        }
    }

    fn layout_element_with_outer_width(
        &mut self,
        id: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
        forced_outer_size: ForcedOuterSize,
    ) -> FlowSize {
        let style = self.document.computed_style_for_layout(id);
        let display = self.effective_display(id);
        if display == DisplayValue::None
            || self.is_non_rendered(id)
            || self.document.is_hidden_for_layout(id)
        {
            return FlowSize::default();
        }
        let previous_fixed = self.fixed;
        if style.position() == NativePositionValue::Fixed {
            self.fixed = true;
        }
        let previous_stacking_context = self.stacking_context;
        if !style.z_index().is_auto() && self.z_index_applies(id, style) {
            self.stacking_context = self
                .stacking_context
                .saturating_add(style.z_index().value());
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
            self.fixed = previous_fixed;
            self.stacking_context = previous_stacking_context;
            if grouped {
                self.paint_order
                    .push(NativeLayoutPaintOrder::EndOpacityGroup { node_id: id });
            }
            return result;
        }

        let previous_sticky_root = self.sticky_root;
        let sticky_root =
            if style.position() == NativePositionValue::Sticky && previous_sticky_root.is_none() {
                Some(self.boxes.len())
            } else {
                previous_sticky_root
            };
        self.sticky_root = sticky_root;

        let is_block = matches!(
            display,
            DisplayValue::Block | DisplayValue::Flex | DisplayValue::Grid
        );
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
        let width = forced_outer_size.width.unwrap_or_else(|| {
            self.outer_width(
                id,
                style,
                is_block,
                available_width,
                !self.explicit_width_can_overflow(id),
            )
        });
        let minimum_line_height = style.line_height().unwrap_or(DEFAULT_LINE_HEIGHT);
        let default_content_height = if is_block {
            minimum_line_height
        } else if self.document.node(id).and_then(|node| node.element_name()) == Some("svg") {
            self.intrinsic_inline_height(id)
        } else if self.document.node(id).and_then(|node| node.element_name()) == Some("img") {
            self.intrinsic_inline_height(id)
        } else {
            self.intrinsic_inline_height(id).max(minimum_line_height)
        };
        let box_index = self.boxes.len();
        let text_start = self.text_runs.len();
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
            fixed: self.fixed,
            sticky: self.sticky_root.is_some(),
            z_index: self.stacking_context,
            pointer_events: style.pointer_events().allows_hit_testing(),
        });
        self.paint_order
            .push(NativeLayoutPaintOrder::Box(box_index));

        let content_x = x.saturating_add(left_inset);
        let content_y = y.saturating_add(top_inset);
        let content_width = width.saturating_sub(horizontal_inset);
        let previous_containing_block = self.containing_block;
        if style.position() != NativePositionValue::Static
            || matches!(display, DisplayValue::Flex | DisplayValue::Grid)
        {
            self.containing_block = PositionedContainingBlock {
                x: x.saturating_add(border_left),
                y: y.saturating_add(border_top),
                width: width.saturating_sub(border_left.saturating_add(border_right)),
                height: Self::explicit_content_height(style)
                    .unwrap_or(default_content_height)
                    .saturating_add(padding.vertical()),
            };
        }
        let children = if self.document.node(id).and_then(|node| node.element_name()) == Some("svg")
        {
            self.layout_svg_children(id, content_x, content_y, content_width, depth + 1)
        } else {
            match display {
                DisplayValue::Flex if self.can_use_flex_layout(id) => {
                    self.layout_flex_children(id, content_x, content_y, content_width, depth + 1)
                }
                DisplayValue::Grid => {
                    self.layout_grid_children(id, content_x, content_y, content_width, depth + 1)
                }
                _ => self.layout_children(id, content_x, content_y, content_width, depth + 1),
            }
        };
        self.containing_block = previous_containing_block;
        self.fixed = previous_fixed;
        self.stacking_context = previous_stacking_context;
        let auto_content_height = default_content_height.max(children.height);
        let height = forced_outer_size.height.unwrap_or_else(|| {
            style.height().map_or(
                vertical_inset.saturating_add(auto_content_height),
                |declared| {
                    if style.is_border_box() {
                        declared
                    } else {
                        declared.saturating_add(vertical_inset)
                    }
                },
            )
        });
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
        if style.position() == NativePositionValue::Relative {
            self.shift_layout(
                box_index,
                self.boxes.len(),
                text_start,
                self.text_runs.len(),
                relative_offset(style.left(), style.right()),
                relative_offset(style.top(), style.bottom()),
            );
        }
        if let Some(root_box) = sticky_root
            && previous_sticky_root.is_none()
            && style.position() == NativePositionValue::Sticky
        {
            self.sticky_ranges.push(NativeStickyRange {
                root_box,
                box_end: self.boxes.len(),
                text_start,
                text_end: self.text_runs.len(),
                top: style.top(),
                right: style.right(),
                bottom: style.bottom(),
                left: style.left(),
            });
        }
        self.sticky_root = previous_sticky_root;
        if grouped {
            self.paint_order
                .push(NativeLayoutPaintOrder::EndOpacityGroup { node_id: id });
        }
        FlowSize { width, height }
    }

    fn z_index_applies(&self, id: NativeNodeId, style: NativeComputedStyle) -> bool {
        style.position() != NativePositionValue::Static
            || self
                .document
                .node(id)
                .and_then(|node| node.parent())
                .is_some_and(|parent| {
                    matches!(
                        self.document.computed_style_for_layout(parent).display(),
                        DisplayValue::Flex | DisplayValue::Grid
                    )
                })
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

    fn shift_layout(
        &mut self,
        box_start: usize,
        box_end: usize,
        text_start: usize,
        text_end: usize,
        offset_x: i64,
        offset_y: i64,
    ) {
        let box_end = box_end.min(self.boxes.len());
        let box_start = box_start.min(box_end);
        for layout_box in &mut self.boxes[box_start..box_end] {
            layout_box.rect.x = shift_coordinate(layout_box.rect.x, offset_x);
            layout_box.rect.y = shift_coordinate(layout_box.rect.y, offset_y);
            layout_box.content_rect.x = shift_coordinate(layout_box.content_rect.x, offset_x);
            layout_box.content_rect.y = shift_coordinate(layout_box.content_rect.y, offset_y);
        }

        let text_end = text_end.min(self.text_runs.len());
        let text_start = text_start.min(text_end);
        for text_run in &mut self.text_runs[text_start..text_end] {
            text_run.origin.x = shift_coordinate(text_run.origin.x, offset_x);
            text_run.origin.y = shift_coordinate(text_run.origin.y, offset_y);
        }
    }

    fn layout_svg_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let children = self
            .document
            .node(parent)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();
        let mut max_right = x;
        let mut max_bottom = y;
        for child in children {
            let Some(node) = self.document.node(child) else {
                continue;
            };
            if !matches!(node.kind(), NativeNodeKind::Element { .. })
                || self.is_non_rendered(child)
                || self.document.is_hidden_for_layout(child)
                || self.effective_display(child) == DisplayValue::None
            {
                continue;
            }
            if node.element_name() == Some("g") {
                let nested = self.layout_svg_children(child, x, y, available_width, depth + 1);
                max_right = max_right.max(x.saturating_add(nested.width));
                max_bottom = max_bottom.max(y.saturating_add(nested.height));
                continue;
            }
            let Some((offset_x, offset_y, width, height)) = self.svg_shape_box(child) else {
                continue;
            };
            self.layout_element_with_outer_width(
                child,
                x.saturating_add(offset_x),
                y.saturating_add(offset_y),
                width,
                depth,
                ForcedOuterSize {
                    width: Some(width),
                    height: Some(height),
                },
            );
            max_right = max_right.max(x.saturating_add(offset_x).saturating_add(width));
            max_bottom = max_bottom.max(y.saturating_add(offset_y).saturating_add(height));
        }
        FlowSize {
            width: max_right.saturating_sub(x).max(available_width),
            height: max_bottom.saturating_sub(y),
        }
    }

    fn svg_shape_box(&self, id: NativeNodeId) -> Option<(u32, u32, u32, u32)> {
        let node = self.document.node(id)?;
        let transform = svg_transform_for_node(self.document, id)?;
        let number = |name: &str| {
            node.attribute(name)
                .and_then(|value| value.trim().parse::<u32>().ok())
                .unwrap_or(0)
        };
        if !transform.is_identity() {
            let points = if node.element_name() == Some("path") {
                svg_transformed_subpaths(node, transform)?
                    .into_iter()
                    .flat_map(|subpath| subpath.points)
                    .collect::<Vec<_>>()
            } else {
                svg_transformed_points(node, transform)?
            };
            let (min_x, min_y, max_x, max_y) = svg_points_bounds(&points)?;
            return Some((
                min_x,
                min_y,
                max_x.saturating_sub(min_x).saturating_add(1),
                max_y.saturating_sub(min_y).saturating_add(1),
            ));
        }
        match node.element_name()? {
            "rect" => Some((number("x"), number("y"), number("width"), number("height"))),
            "circle" => {
                let radius = number("r");
                let diameter = radius.saturating_mul(2);
                Some((
                    number("cx").saturating_sub(radius),
                    number("cy").saturating_sub(radius),
                    diameter,
                    diameter,
                ))
            }
            "ellipse" => {
                let radius_x = number("rx");
                let radius_y = number("ry");
                Some((
                    number("cx").saturating_sub(radius_x),
                    number("cy").saturating_sub(radius_y),
                    radius_x.saturating_mul(2),
                    radius_y.saturating_mul(2),
                ))
            }
            "line" => {
                let points = svg_line_points(node);
                let (min_x, min_y, max_x, max_y) = svg_points_bounds(&points)?;
                Some((
                    min_x,
                    min_y,
                    max_x.saturating_sub(min_x).saturating_add(1),
                    max_y.saturating_sub(min_y).saturating_add(1),
                ))
            }
            "polyline" | "polygon" => {
                let points = svg_points(node)?;
                let (min_x, min_y, max_x, max_y) = svg_points_bounds(&points)?;
                Some((
                    min_x,
                    min_y,
                    max_x.saturating_sub(min_x).saturating_add(1),
                    max_y.saturating_sub(min_y).saturating_add(1),
                ))
            }
            "path" => {
                let subpaths = svg_path_subpaths(node)?;
                let points = subpaths
                    .iter()
                    .flat_map(|subpath| subpath.points.iter().copied())
                    .collect::<Vec<_>>();
                let (min_x, min_y, max_x, max_y) = svg_points_bounds(&points)?;
                Some((
                    min_x,
                    min_y,
                    max_x.saturating_sub(min_x).saturating_add(1),
                    max_y.saturating_sub(min_y).saturating_add(1),
                ))
            }
            _ => None,
        }
    }

    fn stretch_flex_item_box(
        &mut self,
        placement: &FlexItemPlacement,
        style: NativeComputedStyle,
        height: u32,
    ) {
        let Some(layout_box) = self.boxes.get_mut(placement.box_start) else {
            return;
        };
        if layout_box.node_id != placement.child {
            return;
        }
        layout_box.rect.height = height;
        layout_box.content_rect.height = height.saturating_sub(outer_height_inset(style));
    }

    fn stretch_grid_item_box(
        &mut self,
        placement: &GridItemPlacement,
        style: NativeComputedStyle,
        height: u32,
    ) {
        let Some(layout_box) = self.boxes.get_mut(placement.box_start) else {
            return;
        };
        if layout_box.node_id != placement.child {
            return;
        }
        layout_box.rect.height = height;
        layout_box.content_rect.height = height.saturating_sub(outer_height_inset(style));
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
                NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => true,
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

    fn can_use_column_flex_layout(&self, id: NativeNodeId, style: NativeComputedStyle) -> bool {
        if !matches!(
            style.flex_direction(),
            FlexDirectionValue::Column | FlexDirectionValue::ColumnReverse
        ) || !matches!(
            style.flex_wrap(),
            FlexWrapValue::NoWrap | FlexWrapValue::Wrap | FlexWrapValue::WrapReverse
        ) || Self::explicit_content_height(style).is_none()
            || !self.can_use_flex_layout(id)
        {
            return false;
        }
        self.document.node(id).is_some_and(|node| {
            node.children().iter().all(|child| {
                let Some(child_node) = self.document.node(*child) else {
                    return false;
                };
                match child_node.kind() {
                    NativeNodeKind::Text(value) => value.chars().all(char::is_whitespace),
                    NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => true,
                    NativeNodeKind::Document => false,
                    NativeNodeKind::Element { .. } => {
                        if self.is_non_rendered(*child)
                            || self.document.is_hidden_for_layout(*child)
                            || self.effective_display(*child) == DisplayValue::None
                            || self.is_out_of_flow(*child)
                        {
                            return true;
                        }
                        let style = self.document.computed_style_for_layout(*child);
                        style.height().is_some()
                            || matches!(style.flex_basis(), FlexBasisValue::Length(_))
                    }
                }
            })
        })
    }

    fn layout_grid_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let parent_style = self.document.computed_style_for_layout(parent);
        let column_gap = parent_style.column_gap();
        let column_sizes = Self::grid_column_sizes(
            parent_style.grid_template_columns(),
            available_width,
            column_gap,
            parent_style.justify_content(),
        );
        let column_count = column_sizes.len().max(1);
        let column_free_space = available_width.saturating_sub(
            column_sizes
                .iter()
                .fold(0u32, |total, size| total.saturating_add(*size))
                .saturating_add(column_gap.saturating_mul(
                    u32::try_from(column_count.saturating_sub(1)).unwrap_or(u32::MAX),
                )),
        );
        let (column_leading, column_extra_gap) = Self::grid_justify_offsets(
            parent_style.justify_content(),
            column_free_space,
            column_count,
        );
        let mut column_positions = Vec::with_capacity(column_count);
        let mut column_x = x.saturating_add(column_leading);
        for (index, size) in column_sizes.iter().enumerate() {
            column_positions.push(column_x);
            column_x = column_x
                .saturating_add(*size)
                .saturating_add(if index + 1 < column_count {
                    column_gap.saturating_add(column_extra_gap)
                } else {
                    0
                });
        }

        let children = self
            .document
            .node(parent)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();
        let mut placements = Vec::new();
        let mut row_heights = Vec::new();
        let mut grid_index = 0usize;
        for child in children {
            let Some(node) = self.document.node(child) else {
                continue;
            };
            if !matches!(node.kind(), NativeNodeKind::Element { .. })
                || self.is_non_rendered(child)
                || self.document.is_hidden_for_layout(child)
                || self.effective_display(child) == DisplayValue::None
            {
                continue;
            }
            if self.is_out_of_flow(child) {
                continue;
            }
            let column = grid_index % column_count;
            let row = grid_index / column_count;
            grid_index = grid_index.saturating_add(1);
            if row_heights.len() <= row {
                row_heights.resize(row.saturating_add(1), 0);
            }
            let child_style = self.document.computed_style_for_layout(child);
            let margin = child_style.margin();
            let cell_width = column_sizes[column];
            let available_item_width = cell_width.saturating_sub(margin.horizontal());
            let item_width = if child_style.width().is_none() {
                available_item_width
            } else {
                self.outer_width(
                    child,
                    child_style,
                    false,
                    available_item_width,
                    !self.explicit_width_can_overflow(child),
                )
            };
            let box_start = self.boxes.len();
            let text_start = self.text_runs.len();
            let size = self.layout_element_with_outer_width(
                child,
                column_positions[column].saturating_add(margin.left()),
                y,
                item_width,
                depth,
                ForcedOuterSize {
                    width: Some(item_width),
                    ..ForcedOuterSize::default()
                },
            );
            let row_height = size.height.saturating_add(margin.vertical());
            row_heights[row] = row_heights[row].max(row_height);
            placements.push(GridItemPlacement {
                child,
                box_start,
                box_end: self.boxes.len(),
                text_start,
                text_end: self.text_runs.len(),
                column,
                row,
                margin,
                align_self: child_style.align_self(),
                width: size.width,
                height: size.height,
            });
        }

        let row_count = row_heights
            .len()
            .max(parent_style.grid_template_rows().len());
        if row_count == 0 {
            let containing_block = self.containing_block;
            self.layout_positioned_children(
                self.positioned_children(parent),
                containing_block,
                depth,
            );
            return FlowSize {
                width: available_width,
                height: 0,
            };
        }
        row_heights.resize(row_count, 0);
        let explicit_height = Self::explicit_content_height(parent_style);
        let row_sizes = Self::grid_row_sizes(
            parent_style.grid_template_rows(),
            row_count,
            explicit_height,
            parent_style.row_gap(),
            &row_heights,
            parent_style.align_content(),
        );
        let row_gap = parent_style.row_gap();
        let row_content_height = row_sizes
            .iter()
            .fold(0u32, |total, size| total.saturating_add(*size))
            .saturating_add(
                row_gap
                    .saturating_mul(u32::try_from(row_count.saturating_sub(1)).unwrap_or(u32::MAX)),
            );
        let row_free_space = explicit_height
            .unwrap_or(row_content_height)
            .saturating_sub(row_content_height);
        let (row_leading, row_extra_gap) =
            Self::grid_align_offsets(parent_style.align_content(), row_free_space, row_count);
        let mut row_positions = Vec::with_capacity(row_count);
        let mut row_y = y.saturating_add(row_leading);
        for (index, size) in row_sizes.iter().enumerate() {
            row_positions.push(row_y);
            row_y = row_y
                .saturating_add(*size)
                .saturating_add(if index + 1 < row_count {
                    row_gap.saturating_add(row_extra_gap)
                } else {
                    0
                });
        }

        let mut max_right = x;
        let mut max_bottom = y;
        for placement in placements {
            let style = self.document.computed_style_for_layout(placement.child);
            let alignment = match placement.align_self {
                AlignSelfValue::Auto => parent_style.align_items(),
                AlignSelfValue::FlexStart => AlignItemsValue::FlexStart,
                AlignSelfValue::Center => AlignItemsValue::Center,
                AlignSelfValue::FlexEnd => AlignItemsValue::FlexEnd,
                AlignSelfValue::Stretch => AlignItemsValue::Stretch,
                AlignSelfValue::Normal => AlignItemsValue::Normal,
            };
            let cell_height = row_sizes[placement.row];
            let available_item_height = cell_height.saturating_sub(placement.margin.vertical());
            let item_height = if matches!(
                alignment,
                AlignItemsValue::Stretch | AlignItemsValue::Normal
            ) && style.height().is_none()
            {
                constrain_dimension(
                    available_item_height,
                    style
                        .min_height()
                        .map(|declared| outer_height_from_declared(style, declared)),
                    style
                        .max_height()
                        .map(|declared| outer_height_from_declared(style, declared)),
                )
            } else {
                placement.height
            };
            if item_height != placement.height {
                self.stretch_grid_item_box(&placement, style, item_height);
            }
            let remaining_height = available_item_height.saturating_sub(item_height);
            let alignment_offset = match alignment {
                AlignItemsValue::Center => remaining_height / 2,
                AlignItemsValue::FlexEnd => remaining_height,
                AlignItemsValue::FlexStart | AlignItemsValue::Stretch | AlignItemsValue::Normal => {
                    0
                }
            };
            let item_y = row_positions[placement.row]
                .saturating_add(placement.margin.top())
                .saturating_add(alignment_offset);
            self.shift_layout_y(
                placement.box_start,
                placement.box_end,
                placement.text_start,
                placement.text_end,
                i64::from(item_y).saturating_sub(i64::from(y)),
            );
            let item_x = column_positions[placement.column].saturating_add(placement.margin.left());
            max_right = max_right.max(
                item_x
                    .saturating_add(placement.width)
                    .saturating_add(placement.margin.right()),
            );
            max_bottom = max_bottom.max(
                row_positions[placement.row]
                    .saturating_add(cell_height)
                    .max(
                        item_y
                            .saturating_add(item_height)
                            .saturating_add(placement.margin.bottom()),
                    ),
            );
        }
        let containing_block = self.containing_block;
        self.layout_positioned_children(self.positioned_children(parent), containing_block, depth);
        FlowSize {
            width: max_right.saturating_sub(x).max(available_width),
            height: max_bottom.saturating_sub(y),
        }
    }

    fn grid_column_sizes(
        tracks: NativeGridTrackList,
        available_width: u32,
        gap: u32,
        alignment: JustifyContentValue,
    ) -> Vec<u32> {
        if tracks.len() == 0 {
            return vec![available_width];
        }
        let track_count = tracks.len();
        let gap_total =
            gap.saturating_mul(u32::try_from(track_count.saturating_sub(1)).unwrap_or(u32::MAX));
        let mut sizes = vec![0; track_count];
        let mut auto_indices = Vec::new();
        let mut fr_total = 0u32;
        let mut fixed_total = gap_total;
        for (index, size) in sizes.iter_mut().enumerate() {
            match tracks.track(index) {
                NativeGridTrack::Length(value) => {
                    *size = value;
                    fixed_total = fixed_total.saturating_add(value);
                }
                NativeGridTrack::Fr(weight) => fr_total = fr_total.saturating_add(weight),
                NativeGridTrack::Auto => auto_indices.push(index),
            }
        }
        let remaining = available_width.saturating_sub(fixed_total);
        if fr_total > 0 {
            for (index, size) in sizes.iter_mut().enumerate() {
                if let NativeGridTrack::Fr(weight) = tracks.track(index) {
                    *size = u32::try_from(
                        u64::from(remaining)
                            .saturating_mul(u64::from(weight))
                            .checked_div(u64::from(fr_total))
                            .unwrap_or_default(),
                    )
                    .unwrap_or(u32::MAX);
                }
            }
        } else if !auto_indices.is_empty() {
            let share = remaining
                .checked_div(u32::try_from(auto_indices.len()).unwrap_or(u32::MAX))
                .unwrap_or_default();
            let remainder = remaining
                .checked_rem(u32::try_from(auto_indices.len()).unwrap_or(u32::MAX))
                .unwrap_or_default();
            for (slot, index) in auto_indices.into_iter().enumerate() {
                sizes[index] = share.saturating_add(u32::from(
                    u32::try_from(slot).unwrap_or(u32::MAX) < remainder,
                ));
            }
        }
        let used = sizes
            .iter()
            .fold(gap_total, |total, size| total.saturating_add(*size));
        if alignment == JustifyContentValue::Stretch && used < available_width {
            let auto_count = sizes
                .iter()
                .enumerate()
                .filter(|(index, _)| matches!(tracks.track(*index), NativeGridTrack::Auto))
                .count();
            if auto_count > 0 {
                let share = available_width
                    .saturating_sub(used)
                    .checked_div(u32::try_from(auto_count).unwrap_or(u32::MAX))
                    .unwrap_or_default();
                for (index, size) in sizes.iter_mut().enumerate() {
                    if matches!(tracks.track(index), NativeGridTrack::Auto) {
                        *size = size.saturating_add(share);
                    }
                }
            }
        }
        sizes
    }

    fn grid_row_sizes(
        tracks: NativeGridTrackList,
        row_count: usize,
        explicit_height: Option<u32>,
        gap: u32,
        measured: &[u32],
        alignment: AlignContentValue,
    ) -> Vec<u32> {
        let mut sizes = vec![0; row_count];
        let mut auto_indices = Vec::new();
        let mut fr_indices = Vec::new();
        let mut fr_total = 0u32;
        let mut fixed_total =
            gap.saturating_mul(u32::try_from(row_count.saturating_sub(1)).unwrap_or(u32::MAX));
        for (index, size) in sizes.iter_mut().enumerate() {
            match if index < tracks.len() {
                tracks.track(index)
            } else {
                NativeGridTrack::Auto
            } {
                NativeGridTrack::Length(value) => {
                    *size = value;
                    fixed_total = fixed_total.saturating_add(value);
                }
                NativeGridTrack::Fr(weight) => {
                    fr_total = fr_total.saturating_add(weight);
                    fr_indices.push((index, weight));
                }
                NativeGridTrack::Auto => {
                    *size = measured.get(index).copied().unwrap_or_default();
                    fixed_total = fixed_total.saturating_add(*size);
                    auto_indices.push(index);
                }
            }
        }
        let Some(explicit_height) = explicit_height else {
            for (index, _) in fr_indices {
                sizes[index] = measured.get(index).copied().unwrap_or_default();
            }
            return sizes;
        };
        let remaining = explicit_height.saturating_sub(fixed_total);
        if fr_total > 0 {
            for (index, weight) in fr_indices {
                sizes[index] = u32::try_from(
                    u64::from(remaining)
                        .saturating_mul(u64::from(weight))
                        .checked_div(u64::from(fr_total))
                        .unwrap_or_default(),
                )
                .unwrap_or(u32::MAX);
            }
        } else if matches!(
            alignment,
            AlignContentValue::Stretch | AlignContentValue::Normal
        ) && !auto_indices.is_empty()
        {
            let share = remaining
                .checked_div(u32::try_from(auto_indices.len()).unwrap_or(u32::MAX))
                .unwrap_or_default();
            for index in auto_indices {
                sizes[index] = sizes[index].saturating_add(share);
            }
        }
        sizes
    }

    fn grid_justify_offsets(
        alignment: JustifyContentValue,
        free_space: u32,
        track_count: usize,
    ) -> (u32, u32) {
        match alignment {
            JustifyContentValue::Center => (free_space / 2, 0),
            JustifyContentValue::FlexEnd => (free_space, 0),
            JustifyContentValue::SpaceBetween if track_count > 1 => (
                0,
                free_space
                    .checked_div(u32::try_from(track_count - 1).unwrap_or(u32::MAX))
                    .unwrap_or_default(),
            ),
            JustifyContentValue::FlexStart
            | JustifyContentValue::Normal
            | JustifyContentValue::Stretch
            | JustifyContentValue::SpaceBetween
            | JustifyContentValue::SpaceAround
            | JustifyContentValue::SpaceEvenly => (0, 0),
        }
    }

    fn grid_align_offsets(
        alignment: AlignContentValue,
        free_space: u32,
        row_count: usize,
    ) -> (u32, u32) {
        match alignment {
            AlignContentValue::Center => (free_space / 2, 0),
            AlignContentValue::FlexEnd => (free_space, 0),
            AlignContentValue::SpaceBetween if row_count > 1 => (
                0,
                free_space
                    .checked_div(u32::try_from(row_count - 1).unwrap_or(u32::MAX))
                    .unwrap_or_default(),
            ),
            AlignContentValue::FlexStart
            | AlignContentValue::Stretch
            | AlignContentValue::Normal
            | AlignContentValue::SpaceBetween
            | AlignContentValue::SpaceAround
            | AlignContentValue::SpaceEvenly => (0, 0),
        }
    }

    fn layout_flex_line(
        &mut self,
        items: Vec<FlexItem>,
        context: FlexLineContext,
    ) -> FlexLineLayout {
        let mut items = items;
        let base_occupied_width = flex_occupied_width(&items, context.gap);
        if base_occupied_width < context.available_width {
            apply_flex_growth(&mut items, context.available_width, context.gap);
        } else if base_occupied_width > context.available_width {
            apply_flex_shrink(&mut items, context.available_width, context.gap);
        }
        let gap_count = u32::try_from(items.len().saturating_sub(1)).unwrap_or(u32::MAX);
        let item_width = items.iter().fold(0u32, |total, item| {
            total
                .saturating_add(item.margin.horizontal())
                .saturating_add(item.width)
        });
        let occupied_width = item_width.saturating_add(context.gap.saturating_mul(gap_count));
        let free_space_before_auto = context.available_width.saturating_sub(occupied_width);
        resolve_row_main_auto_margins(&mut items, free_space_before_auto, context.reverse);
        let item_width = items.iter().fold(0u32, |total, item| {
            total
                .saturating_add(item.margin.horizontal())
                .saturating_add(item.width)
        });
        let occupied_width = item_width.saturating_add(context.gap.saturating_mul(gap_count));
        let free_space = context.available_width.saturating_sub(occupied_width);
        let item_count = items.len();
        let leading_offset = if items.is_empty() {
            0
        } else {
            match context.justify_content {
                JustifyContentValue::Center => free_space / 2,
                JustifyContentValue::FlexEnd => free_space,
                JustifyContentValue::FlexStart
                | JustifyContentValue::Normal
                | JustifyContentValue::Stretch
                | JustifyContentValue::SpaceBetween
                | JustifyContentValue::SpaceAround
                | JustifyContentValue::SpaceEvenly => 0,
            }
        };
        let initial_offset = match context.justify_content {
            JustifyContentValue::SpaceAround if item_count > 0 => {
                flex_space_around_offset(free_space, 0, item_count)
            }
            JustifyContentValue::SpaceEvenly if item_count > 0 => {
                flex_space_evenly_offset(free_space, 0, item_count)
            }
            _ => leading_offset,
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
        let mut placements = Vec::with_capacity(item_count);
        let width = if context.reverse {
            let reverse_shift = occupied_width.saturating_sub(context.available_width);
            let mut cursor_right = context
                .x
                .saturating_add(context.available_width)
                .saturating_sub(initial_offset)
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
                    } else if context.justify_content == JustifyContentValue::SpaceAround {
                        let current_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_right = cursor_right
                            .saturating_sub(current_offset.saturating_sub(previous_offset));
                    } else if context.justify_content == JustifyContentValue::SpaceEvenly {
                        let current_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_right = cursor_right
                            .saturating_sub(current_offset.saturating_sub(previous_offset));
                    }
                }
                let item_x = cursor_right
                    .saturating_sub(item.margin.right())
                    .saturating_sub(item.width);
                let item_y = context.y.saturating_add(item.margin.top());
                let box_start = self.boxes.len();
                let text_start = self.text_runs.len();
                let size = self.layout_element_with_outer_width(
                    item.child,
                    item_x,
                    item_y,
                    item.width,
                    context.depth,
                    ForcedOuterSize {
                        width: Some(item.width),
                        ..ForcedOuterSize::default()
                    },
                );
                placements.push(FlexItemPlacement {
                    child: item.child,
                    box_start,
                    box_end: self.boxes.len(),
                    text_start,
                    text_end: self.text_runs.len(),
                    margin: item.margin,
                    auto_margin: item.auto_margin,
                    height: size.height,
                    align_self: item.align_self,
                });
                cursor_right = item_x.saturating_sub(item.margin.left());
            }
            if item_count == 0 {
                0
            } else {
                occupied_width.saturating_add(
                    if context.justify_content == JustifyContentValue::SpaceBetween {
                        free_space
                    } else if context.justify_content == JustifyContentValue::SpaceAround {
                        flex_space_around_offset(
                            free_space,
                            u32::try_from(item_count.saturating_sub(1)).unwrap_or(u32::MAX),
                            item_count,
                        )
                    } else if context.justify_content == JustifyContentValue::SpaceEvenly {
                        flex_space_evenly_offset(
                            free_space,
                            u32::try_from(item_count.saturating_sub(1)).unwrap_or(u32::MAX),
                            item_count,
                        )
                    } else {
                        leading_offset
                    },
                )
            }
        } else {
            let mut cursor_x = context.x.saturating_add(initial_offset);
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
                    } else if context.justify_content == JustifyContentValue::SpaceAround {
                        let current_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_x =
                            cursor_x.saturating_add(current_offset.saturating_sub(previous_offset));
                    } else if context.justify_content == JustifyContentValue::SpaceEvenly {
                        let current_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_x =
                            cursor_x.saturating_add(current_offset.saturating_sub(previous_offset));
                    }
                }
                let item_x = cursor_x.saturating_add(item.margin.left());
                let item_y = context.y.saturating_add(item.margin.top());
                let box_start = self.boxes.len();
                let text_start = self.text_runs.len();
                let size = self.layout_element_with_outer_width(
                    item.child,
                    item_x,
                    item_y,
                    item.width,
                    context.depth,
                    ForcedOuterSize {
                        width: Some(item.width),
                        ..ForcedOuterSize::default()
                    },
                );
                placements.push(FlexItemPlacement {
                    child: item.child,
                    box_start,
                    box_end: self.boxes.len(),
                    text_start,
                    text_end: self.text_runs.len(),
                    margin: item.margin,
                    auto_margin: item.auto_margin,
                    height: size.height,
                    align_self: item.align_self,
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

    fn layout_flex_column_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let parent_style = self.document.computed_style_for_layout(parent);
        let available_height = Self::explicit_content_height(parent_style).unwrap_or_default();
        let gap = parent_style.row_gap();
        let justify_content = parent_style.justify_content();
        let align_items = parent_style.align_items();
        let reverse = parent_style.flex_direction() == FlexDirectionValue::ColumnReverse;
        let cross_reverse = parent_style.direction() == DirectionValue::Rtl;
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
                NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
                NativeNodeKind::Document => {
                    return self.layout_children(parent, x, y, available_width, depth);
                }
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(child) || self.document.is_hidden_for_layout(child) {
                        continue;
                    }
                    if self.is_out_of_flow(child) {
                        continue;
                    }
                    let style = self.document.computed_style_for_layout(child);
                    if self.effective_display(child) == DisplayValue::None {
                        continue;
                    }
                    let margin = style.margin();
                    let width = self.outer_width(
                        child,
                        style,
                        false,
                        available_width,
                        !self.explicit_width_can_overflow(child),
                    );
                    let height = self.flex_item_base_height(child, style);
                    let min_width = style
                        .min_width()
                        .map(|declared| outer_width_from_declared(style, declared))
                        .unwrap_or(0);
                    let max_width = style
                        .max_width()
                        .map(|declared| outer_width_from_declared(style, declared));
                    let min_height = style
                        .min_height()
                        .map(|declared| outer_height_from_declared(style, declared))
                        .unwrap_or(0);
                    let max_height = style
                        .max_height()
                        .map(|declared| outer_height_from_declared(style, declared));
                    items.push(FlexItem {
                        child,
                        margin,
                        auto_margin: style.margin_auto(),
                        width,
                        height,
                        flex_base_width: width,
                        flex_base_height: height,
                        min_width,
                        min_height,
                        max_width,
                        max_height,
                        flex_grow: style.flex_grow(),
                        flex_shrink: style.flex_shrink(),
                        order: style.flex_item_order().value(),
                        source_index,
                        align_self: style.align_self(),
                    });
                }
            }
        }

        items.sort_by_key(|item| (item.order, item.source_index));
        let base_occupied_height = flex_occupied_height(&items, gap);
        if base_occupied_height < available_height {
            apply_flex_growth_height(&mut items, available_height, gap);
        } else if base_occupied_height > available_height {
            apply_flex_shrink_height(&mut items, available_height, gap);
        }
        let item_count = items.len();
        let gap_count = u32::try_from(item_count.saturating_sub(1)).unwrap_or(u32::MAX);
        let occupied_height = flex_occupied_height(&items, gap);
        let free_space_before_auto = available_height.saturating_sub(occupied_height);
        resolve_column_main_auto_margins(&mut items, free_space_before_auto, reverse);
        let occupied_height = flex_occupied_height(&items, gap);
        let free_space = available_height.saturating_sub(occupied_height);
        let leading_offset = match justify_content {
            JustifyContentValue::Center => free_space / 2,
            JustifyContentValue::FlexEnd => free_space,
            JustifyContentValue::FlexStart
            | JustifyContentValue::Normal
            | JustifyContentValue::Stretch
            | JustifyContentValue::SpaceBetween
            | JustifyContentValue::SpaceAround
            | JustifyContentValue::SpaceEvenly => 0,
        };
        let initial_offset = match justify_content {
            JustifyContentValue::SpaceAround if item_count > 0 => {
                flex_space_around_offset(free_space, 0, item_count)
            }
            JustifyContentValue::SpaceEvenly if item_count > 0 => {
                flex_space_evenly_offset(free_space, 0, item_count)
            }
            _ => leading_offset,
        };
        let distributed_gap = if justify_content == JustifyContentValue::SpaceBetween {
            free_space.checked_div(gap_count).unwrap_or(0)
        } else {
            0
        };
        let distributed_remainder = if justify_content == JustifyContentValue::SpaceBetween {
            free_space.checked_rem(gap_count).unwrap_or(0)
        } else {
            0
        };
        let mut max_right = x;
        let mut max_bottom = y;
        if reverse {
            let reverse_shift = occupied_height.saturating_sub(available_height);
            let mut cursor_bottom = y
                .saturating_add(available_height)
                .saturating_sub(initial_offset)
                .saturating_add(reverse_shift);
            for (index, mut item) in items.into_iter().enumerate() {
                if index > 0 {
                    cursor_bottom = cursor_bottom.saturating_sub(gap);
                    if justify_content == JustifyContentValue::SpaceBetween {
                        cursor_bottom = cursor_bottom.saturating_sub(distributed_gap);
                        if u32::try_from(index - 1)
                            .is_ok_and(|gap_index| gap_index < distributed_remainder)
                        {
                            cursor_bottom = cursor_bottom.saturating_sub(1);
                        }
                    } else if justify_content == JustifyContentValue::SpaceAround {
                        let current_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_bottom = cursor_bottom
                            .saturating_sub(current_offset.saturating_sub(previous_offset));
                    } else if justify_content == JustifyContentValue::SpaceEvenly {
                        let current_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_bottom = cursor_bottom
                            .saturating_sub(current_offset.saturating_sub(previous_offset));
                    }
                }
                let style = self.document.computed_style_for_layout(item.child);
                let alignment = match item.align_self {
                    AlignSelfValue::Auto => align_items,
                    AlignSelfValue::FlexStart => AlignItemsValue::FlexStart,
                    AlignSelfValue::Center => AlignItemsValue::Center,
                    AlignSelfValue::FlexEnd => AlignItemsValue::FlexEnd,
                    AlignSelfValue::Stretch => AlignItemsValue::Stretch,
                    AlignSelfValue::Normal => AlignItemsValue::Normal,
                };
                let cross_auto_resolved =
                    resolve_column_cross_auto_margins(&mut item, available_width);
                let item_width = if matches!(
                    alignment,
                    AlignItemsValue::Stretch | AlignItemsValue::Normal
                ) && !cross_auto_resolved
                    && style.width().is_none()
                {
                    constrain_dimension(
                        available_width.saturating_sub(item.margin.horizontal()),
                        Some(
                            style
                                .min_width()
                                .map(|declared| outer_width_from_declared(style, declared))
                                .unwrap_or(0),
                        ),
                        style
                            .max_width()
                            .map(|declared| outer_width_from_declared(style, declared)),
                    )
                } else {
                    item.width
                };
                let remaining = available_width
                    .saturating_sub(item_width.saturating_add(item.margin.horizontal()));
                let cross_offset = if cross_auto_resolved {
                    0
                } else {
                    match alignment {
                        AlignItemsValue::FlexStart
                        | AlignItemsValue::Stretch
                        | AlignItemsValue::Normal => {
                            if cross_reverse {
                                remaining
                            } else {
                                0
                            }
                        }
                        AlignItemsValue::Center => remaining / 2,
                        AlignItemsValue::FlexEnd => {
                            if cross_reverse {
                                0
                            } else {
                                remaining
                            }
                        }
                    }
                };
                let item_x = x
                    .saturating_add(item.margin.left())
                    .saturating_add(cross_offset);
                let item_y = cursor_bottom
                    .saturating_sub(item.margin.bottom())
                    .saturating_sub(item.height);
                let size = self.layout_element_with_outer_width(
                    item.child,
                    item_x,
                    item_y,
                    item_width,
                    depth,
                    ForcedOuterSize {
                        width: Some(item_width),
                        height: Some(item.height),
                    },
                );
                max_right = max_right.max(
                    item_x
                        .saturating_add(size.width)
                        .saturating_add(item.margin.right()),
                );
                max_bottom = max_bottom.max(
                    item_y
                        .saturating_add(size.height)
                        .saturating_add(item.margin.bottom()),
                );
                cursor_bottom = item_y.saturating_sub(item.margin.top());
            }
        } else {
            let mut cursor_y = y.saturating_add(initial_offset);
            for (index, mut item) in items.into_iter().enumerate() {
                if index > 0 {
                    cursor_y = cursor_y.saturating_add(gap);
                    if justify_content == JustifyContentValue::SpaceBetween {
                        cursor_y = cursor_y.saturating_add(distributed_gap);
                        if u32::try_from(index - 1)
                            .is_ok_and(|gap_index| gap_index < distributed_remainder)
                        {
                            cursor_y = cursor_y.saturating_add(1);
                        }
                    } else if justify_content == JustifyContentValue::SpaceAround {
                        let current_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_y =
                            cursor_y.saturating_add(current_offset.saturating_sub(previous_offset));
                    } else if justify_content == JustifyContentValue::SpaceEvenly {
                        let current_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_y =
                            cursor_y.saturating_add(current_offset.saturating_sub(previous_offset));
                    }
                }
                let style = self.document.computed_style_for_layout(item.child);
                let alignment = match item.align_self {
                    AlignSelfValue::Auto => align_items,
                    AlignSelfValue::FlexStart => AlignItemsValue::FlexStart,
                    AlignSelfValue::Center => AlignItemsValue::Center,
                    AlignSelfValue::FlexEnd => AlignItemsValue::FlexEnd,
                    AlignSelfValue::Stretch => AlignItemsValue::Stretch,
                    AlignSelfValue::Normal => AlignItemsValue::Normal,
                };
                let cross_auto_resolved =
                    resolve_column_cross_auto_margins(&mut item, available_width);
                let item_width = if matches!(
                    alignment,
                    AlignItemsValue::Stretch | AlignItemsValue::Normal
                ) && !cross_auto_resolved
                    && style.width().is_none()
                {
                    constrain_dimension(
                        available_width.saturating_sub(item.margin.horizontal()),
                        Some(
                            style
                                .min_width()
                                .map(|declared| outer_width_from_declared(style, declared))
                                .unwrap_or(0),
                        ),
                        style
                            .max_width()
                            .map(|declared| outer_width_from_declared(style, declared)),
                    )
                } else {
                    item.width
                };
                let remaining = available_width
                    .saturating_sub(item_width.saturating_add(item.margin.horizontal()));
                let cross_offset = if cross_auto_resolved {
                    0
                } else {
                    match alignment {
                        AlignItemsValue::FlexStart
                        | AlignItemsValue::Stretch
                        | AlignItemsValue::Normal => {
                            if cross_reverse {
                                remaining
                            } else {
                                0
                            }
                        }
                        AlignItemsValue::Center => remaining / 2,
                        AlignItemsValue::FlexEnd => {
                            if cross_reverse {
                                0
                            } else {
                                remaining
                            }
                        }
                    }
                };
                let item_x = x
                    .saturating_add(item.margin.left())
                    .saturating_add(cross_offset);
                let item_y = cursor_y.saturating_add(item.margin.top());
                let size = self.layout_element_with_outer_width(
                    item.child,
                    item_x,
                    item_y,
                    item_width,
                    depth,
                    ForcedOuterSize {
                        width: Some(item_width),
                        height: Some(item.height),
                    },
                );
                max_right = max_right.max(
                    item_x
                        .saturating_add(size.width)
                        .saturating_add(item.margin.right()),
                );
                max_bottom = max_bottom.max(
                    item_y
                        .saturating_add(size.height)
                        .saturating_add(item.margin.bottom()),
                );
                cursor_y = item_y
                    .saturating_add(size.height)
                    .saturating_add(item.margin.bottom());
            }
        }

        let containing_block = self.containing_block;
        self.layout_positioned_children(self.positioned_children(parent), containing_block, depth);
        FlowSize {
            width: max_right.saturating_sub(x),
            height: max_bottom.saturating_sub(y),
        }
    }

    fn layout_flex_column_line(
        &mut self,
        mut items: Vec<FlexItem>,
        context: FlexColumnLineContext,
    ) -> FlowSize {
        let base_occupied_height = flex_occupied_height(&items, context.gap);
        if base_occupied_height < context.available_height {
            apply_flex_growth_height(&mut items, context.available_height, context.gap);
        } else if base_occupied_height > context.available_height {
            apply_flex_shrink_height(&mut items, context.available_height, context.gap);
        }
        let gap_count = u32::try_from(items.len().saturating_sub(1)).unwrap_or(u32::MAX);
        let item_height = items.iter().fold(0u32, |total, item| {
            total
                .saturating_add(item.margin.vertical())
                .saturating_add(item.height)
        });
        let occupied_height = item_height.saturating_add(context.gap.saturating_mul(gap_count));
        let free_space_before_auto = context.available_height.saturating_sub(occupied_height);
        resolve_column_main_auto_margins(&mut items, free_space_before_auto, context.reverse);
        let item_height = items.iter().fold(0u32, |total, item| {
            total
                .saturating_add(item.margin.vertical())
                .saturating_add(item.height)
        });
        let occupied_height = item_height.saturating_add(context.gap.saturating_mul(gap_count));
        let free_space = context.available_height.saturating_sub(occupied_height);
        let item_count = items.len();
        let leading_offset = match context.justify_content {
            JustifyContentValue::Center => free_space / 2,
            JustifyContentValue::FlexEnd => free_space,
            JustifyContentValue::FlexStart
            | JustifyContentValue::Normal
            | JustifyContentValue::Stretch
            | JustifyContentValue::SpaceBetween
            | JustifyContentValue::SpaceAround
            | JustifyContentValue::SpaceEvenly => 0,
        };
        let initial_offset = match context.justify_content {
            JustifyContentValue::SpaceAround if item_count > 0 => {
                flex_space_around_offset(free_space, 0, item_count)
            }
            JustifyContentValue::SpaceEvenly if item_count > 0 => {
                flex_space_evenly_offset(free_space, 0, item_count)
            }
            _ => leading_offset,
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
        let cross_axis_offset = |alignment: AlignItemsValue, remaining: u32| match alignment {
            AlignItemsValue::FlexStart | AlignItemsValue::Stretch | AlignItemsValue::Normal => {
                if context.cross_reverse {
                    remaining
                } else {
                    0
                }
            }
            AlignItemsValue::Center => remaining / 2,
            AlignItemsValue::FlexEnd => {
                if context.cross_reverse {
                    0
                } else {
                    remaining
                }
            }
        };
        let mut max_right = context.x.saturating_add(context.available_width);
        let mut max_bottom = context.y.saturating_add(context.available_height);
        if context.reverse {
            let reverse_shift = occupied_height.saturating_sub(context.available_height);
            let mut cursor_bottom = context
                .y
                .saturating_add(context.available_height)
                .saturating_sub(initial_offset)
                .saturating_add(reverse_shift);
            for (index, mut item) in items.into_iter().enumerate() {
                if index > 0 {
                    cursor_bottom = cursor_bottom.saturating_sub(context.gap);
                    if context.justify_content == JustifyContentValue::SpaceBetween {
                        cursor_bottom = cursor_bottom.saturating_sub(distributed_gap);
                        if u32::try_from(index - 1)
                            .is_ok_and(|gap_index| gap_index < distributed_remainder)
                        {
                            cursor_bottom = cursor_bottom.saturating_sub(1);
                        }
                    } else if context.justify_content == JustifyContentValue::SpaceAround {
                        let current_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_bottom = cursor_bottom
                            .saturating_sub(current_offset.saturating_sub(previous_offset));
                    } else if context.justify_content == JustifyContentValue::SpaceEvenly {
                        let current_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_bottom = cursor_bottom
                            .saturating_sub(current_offset.saturating_sub(previous_offset));
                    }
                }
                let style = self.document.computed_style_for_layout(item.child);
                let alignment = match item.align_self {
                    AlignSelfValue::Auto => context.align_items,
                    AlignSelfValue::FlexStart => AlignItemsValue::FlexStart,
                    AlignSelfValue::Center => AlignItemsValue::Center,
                    AlignSelfValue::FlexEnd => AlignItemsValue::FlexEnd,
                    AlignSelfValue::Stretch => AlignItemsValue::Stretch,
                    AlignSelfValue::Normal => AlignItemsValue::Normal,
                };
                let cross_auto_resolved =
                    resolve_column_cross_auto_margins(&mut item, context.available_width);
                let item_width = if matches!(
                    alignment,
                    AlignItemsValue::Stretch | AlignItemsValue::Normal
                ) && !cross_auto_resolved
                    && style.width().is_none()
                {
                    constrain_dimension(
                        context
                            .available_width
                            .saturating_sub(item.margin.horizontal()),
                        Some(
                            style
                                .min_width()
                                .map(|declared| outer_width_from_declared(style, declared))
                                .unwrap_or(0),
                        ),
                        style
                            .max_width()
                            .map(|declared| outer_width_from_declared(style, declared)),
                    )
                } else {
                    item.width
                };
                let remaining = context
                    .available_width
                    .saturating_sub(item_width.saturating_add(item.margin.horizontal()));
                let cross_offset = if cross_auto_resolved {
                    0
                } else {
                    cross_axis_offset(alignment, remaining)
                };
                let item_x = context
                    .x
                    .saturating_add(item.margin.left())
                    .saturating_add(cross_offset);
                let item_y = cursor_bottom
                    .saturating_sub(item.margin.bottom())
                    .saturating_sub(item.height);
                let size = self.layout_element_with_outer_width(
                    item.child,
                    item_x,
                    item_y,
                    item_width,
                    context.depth,
                    ForcedOuterSize {
                        width: Some(item_width),
                        height: Some(item.height),
                    },
                );
                max_right = max_right.max(
                    item_x
                        .saturating_add(size.width)
                        .saturating_add(item.margin.right()),
                );
                max_bottom = max_bottom.max(
                    item_y
                        .saturating_add(size.height)
                        .saturating_add(item.margin.bottom()),
                );
                cursor_bottom = item_y.saturating_sub(item.margin.top());
            }
        } else {
            let mut cursor_y = context.y.saturating_add(initial_offset);
            for (index, mut item) in items.into_iter().enumerate() {
                if index > 0 {
                    cursor_y = cursor_y.saturating_add(context.gap);
                    if context.justify_content == JustifyContentValue::SpaceBetween {
                        cursor_y = cursor_y.saturating_add(distributed_gap);
                        if u32::try_from(index - 1)
                            .is_ok_and(|gap_index| gap_index < distributed_remainder)
                        {
                            cursor_y = cursor_y.saturating_add(1);
                        }
                    } else if context.justify_content == JustifyContentValue::SpaceAround {
                        let current_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_around_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_y =
                            cursor_y.saturating_add(current_offset.saturating_sub(previous_offset));
                    } else if context.justify_content == JustifyContentValue::SpaceEvenly {
                        let current_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index).unwrap_or(u32::MAX),
                            item_count,
                        );
                        let previous_offset = flex_space_evenly_offset(
                            free_space,
                            u32::try_from(index - 1).unwrap_or(u32::MAX),
                            item_count,
                        );
                        cursor_y =
                            cursor_y.saturating_add(current_offset.saturating_sub(previous_offset));
                    }
                }
                let style = self.document.computed_style_for_layout(item.child);
                let alignment = match item.align_self {
                    AlignSelfValue::Auto => context.align_items,
                    AlignSelfValue::FlexStart => AlignItemsValue::FlexStart,
                    AlignSelfValue::Center => AlignItemsValue::Center,
                    AlignSelfValue::FlexEnd => AlignItemsValue::FlexEnd,
                    AlignSelfValue::Stretch => AlignItemsValue::Stretch,
                    AlignSelfValue::Normal => AlignItemsValue::Normal,
                };
                let cross_auto_resolved =
                    resolve_column_cross_auto_margins(&mut item, context.available_width);
                let item_width = if matches!(
                    alignment,
                    AlignItemsValue::Stretch | AlignItemsValue::Normal
                ) && !cross_auto_resolved
                    && style.width().is_none()
                {
                    constrain_dimension(
                        context
                            .available_width
                            .saturating_sub(item.margin.horizontal()),
                        Some(
                            style
                                .min_width()
                                .map(|declared| outer_width_from_declared(style, declared))
                                .unwrap_or(0),
                        ),
                        style
                            .max_width()
                            .map(|declared| outer_width_from_declared(style, declared)),
                    )
                } else {
                    item.width
                };
                let remaining = context
                    .available_width
                    .saturating_sub(item_width.saturating_add(item.margin.horizontal()));
                let cross_offset = if cross_auto_resolved {
                    0
                } else {
                    cross_axis_offset(alignment, remaining)
                };
                let item_x = context
                    .x
                    .saturating_add(item.margin.left())
                    .saturating_add(cross_offset);
                let item_y = cursor_y.saturating_add(item.margin.top());
                let size = self.layout_element_with_outer_width(
                    item.child,
                    item_x,
                    item_y,
                    item_width,
                    context.depth,
                    ForcedOuterSize {
                        width: Some(item_width),
                        height: Some(item.height),
                    },
                );
                max_right = max_right.max(
                    item_x
                        .saturating_add(size.width)
                        .saturating_add(item.margin.right()),
                );
                max_bottom = max_bottom.max(
                    item_y
                        .saturating_add(size.height)
                        .saturating_add(item.margin.bottom()),
                );
                cursor_y = item_y
                    .saturating_add(size.height)
                    .saturating_add(item.margin.bottom());
            }
        }

        FlowSize {
            width: max_right.saturating_sub(context.x),
            height: max_bottom.saturating_sub(context.y),
        }
    }

    fn layout_flex_column_wrap_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
        wrap_reverse: bool,
    ) -> FlowSize {
        let parent_style = self.document.computed_style_for_layout(parent);
        let available_height = Self::explicit_content_height(parent_style).unwrap_or_default();
        let row_gap = parent_style.row_gap();
        let column_gap = parent_style.column_gap();
        let justify_content = parent_style.justify_content();
        let align_items = parent_style.align_items();
        let align_content = parent_style.align_content();
        let reverse = parent_style.flex_direction() == FlexDirectionValue::ColumnReverse;
        let cross_reverse = wrap_reverse ^ (parent_style.direction() == DirectionValue::Rtl);
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
                NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
                NativeNodeKind::Document => {
                    return self.layout_children(parent, x, y, available_width, depth);
                }
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(child) || self.document.is_hidden_for_layout(child) {
                        continue;
                    }
                    if self.is_out_of_flow(child) {
                        continue;
                    }
                    let style = self.document.computed_style_for_layout(child);
                    if self.effective_display(child) == DisplayValue::None {
                        continue;
                    }
                    let margin = style.margin();
                    let width = self.outer_width(
                        child,
                        style,
                        false,
                        available_width,
                        !self.explicit_width_can_overflow(child),
                    );
                    let height = self.flex_item_base_height(child, style);
                    let min_width = style
                        .min_width()
                        .map(|declared| outer_width_from_declared(style, declared))
                        .unwrap_or(0);
                    let max_width = style
                        .max_width()
                        .map(|declared| outer_width_from_declared(style, declared));
                    let min_height = style
                        .min_height()
                        .map(|declared| outer_height_from_declared(style, declared))
                        .unwrap_or(0);
                    let max_height = style
                        .max_height()
                        .map(|declared| outer_height_from_declared(style, declared));
                    items.push(FlexItem {
                        child,
                        margin,
                        auto_margin: style.margin_auto(),
                        width,
                        height,
                        flex_base_width: width,
                        flex_base_height: height,
                        min_width,
                        min_height,
                        max_width,
                        max_height,
                        flex_grow: style.flex_grow(),
                        flex_shrink: style.flex_shrink(),
                        order: style.flex_item_order().value(),
                        source_index,
                        align_self: style.align_self(),
                    });
                }
            }
        }

        items.sort_by_key(|item| (item.order, item.source_index));
        let mut lines = Vec::new();
        let mut current = Vec::new();
        let mut current_height = 0u32;
        for item in items {
            let item_outer_height = item.margin.vertical().saturating_add(item.height);
            let separator_height = if current.is_empty() { 0 } else { row_gap };
            if !current.is_empty()
                && current_height
                    .saturating_add(separator_height)
                    .saturating_add(item_outer_height)
                    > available_height
            {
                let width = current
                    .iter()
                    .map(|item: &FlexItem| item.margin.horizontal().saturating_add(item.width))
                    .max()
                    .unwrap_or(0);
                lines.push(FlexColumnWrapLine {
                    items: current,
                    provisional_x: x,
                    width,
                });
                current = Vec::new();
                current_height = 0;
            }
            if !current.is_empty() {
                current_height = current_height.saturating_add(row_gap);
            }
            current_height = current_height.saturating_add(item_outer_height);
            current.push(item);
        }
        if !current.is_empty() {
            let width = current
                .iter()
                .map(|item: &FlexItem| item.margin.horizontal().saturating_add(item.width))
                .max()
                .unwrap_or(0);
            lines.push(FlexColumnWrapLine {
                items: current,
                provisional_x: x,
                width,
            });
        }

        let line_count = lines.len();
        let line_gap_count = u32::try_from(line_count.saturating_sub(1)).unwrap_or(u32::MAX);
        let explicit_line_gap = column_gap.saturating_mul(line_gap_count);
        let total_line_width = lines
            .iter()
            .fold(0u32, |total, line| total.saturating_add(line.width));
        let total_line_width = total_line_width.saturating_add(explicit_line_gap);
        let free_space = available_width.saturating_sub(total_line_width);
        if line_count > 0
            && matches!(
                align_content,
                AlignContentValue::Stretch | AlignContentValue::Normal
            )
            && free_space > 0
        {
            let line_count_u32 = u32::try_from(line_count).unwrap_or(u32::MAX).max(1);
            let per_line_extra = free_space / line_count_u32;
            let remainder = free_space % line_count_u32;
            for (index, line) in lines.iter_mut().enumerate() {
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                line.width = line
                    .width
                    .saturating_add(per_line_extra)
                    .saturating_add(u32::from(index < remainder));
            }
        }

        let mut provisional_x = x;
        for line in &mut lines {
            line.provisional_x = provisional_x;
            provisional_x = provisional_x
                .saturating_add(line.width)
                .saturating_add(column_gap);
        }
        let distributed_line_gap = if align_content == AlignContentValue::SpaceBetween {
            free_space.checked_div(line_gap_count).unwrap_or(0)
        } else {
            0
        };
        let distributed_remainder = if align_content == AlignContentValue::SpaceBetween {
            free_space.checked_rem(line_gap_count).unwrap_or(0)
        } else {
            0
        };
        let leading_line_offset = match align_content {
            AlignContentValue::Center => free_space / 2,
            AlignContentValue::FlexEnd => free_space,
            AlignContentValue::FlexStart
            | AlignContentValue::SpaceBetween
            | AlignContentValue::SpaceAround
            | AlignContentValue::SpaceEvenly
            | AlignContentValue::Stretch
            | AlignContentValue::Normal => 0,
        };
        let mut max_right = x;
        let mut max_bottom = y.saturating_add(available_height);
        for (index, line) in lines.into_iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            let line_offset = if align_content == AlignContentValue::SpaceAround {
                flex_space_around_offset(free_space, index, line_count)
            } else if align_content == AlignContentValue::SpaceEvenly {
                flex_space_evenly_offset(free_space, index, line_count)
            } else {
                leading_line_offset
                    .saturating_add(distributed_line_gap.saturating_mul(index))
                    .saturating_add(index.min(distributed_remainder))
            };
            let normal_line_x = line.provisional_x.saturating_add(line_offset);
            let line_x = if cross_reverse {
                let line_start = normal_line_x.saturating_sub(x);
                x.saturating_add(
                    available_width.saturating_sub(line_start.saturating_add(line.width)),
                )
            } else {
                normal_line_x
            };
            let line_layout = self.layout_flex_column_line(
                line.items,
                FlexColumnLineContext {
                    x: line_x,
                    y,
                    available_width: line.width,
                    available_height,
                    gap: row_gap,
                    justify_content,
                    align_items,
                    cross_reverse,
                    reverse,
                    depth,
                },
            );
            max_right = max_right.max(line_x.saturating_add(line_layout.width));
            max_bottom = max_bottom.max(y.saturating_add(line_layout.height));
        }

        let containing_block = self.containing_block;
        self.layout_positioned_children(self.positioned_children(parent), containing_block, depth);
        FlowSize {
            width: max_right.saturating_sub(x).max(available_width),
            height: max_bottom.saturating_sub(y),
        }
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
        if matches!(
            parent_style.flex_direction(),
            FlexDirectionValue::Column | FlexDirectionValue::ColumnReverse
        ) {
            return match parent_style.flex_wrap() {
                FlexWrapValue::NoWrap if self.can_use_column_flex_layout(parent, parent_style) => {
                    self.layout_flex_column_children(parent, x, y, available_width, depth)
                }
                FlexWrapValue::Wrap | FlexWrapValue::WrapReverse
                    if self.can_use_column_flex_layout(parent, parent_style) =>
                {
                    self.layout_flex_column_wrap_children(
                        parent,
                        x,
                        y,
                        available_width,
                        depth,
                        parent_style.flex_wrap() == FlexWrapValue::WrapReverse,
                    )
                }
                FlexWrapValue::NoWrap | FlexWrapValue::Wrap | FlexWrapValue::WrapReverse => {
                    self.layout_children(parent, x, y, available_width, depth)
                }
            };
        }
        let gap = parent_style.column_gap();
        let row_gap = parent_style.row_gap();
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
                NativeNodeKind::DocumentType { .. } | NativeNodeKind::Comment(_) => {}
                NativeNodeKind::Document => {
                    return self.layout_children(parent, x, y, available_width, depth);
                }
                NativeNodeKind::Element { .. } => {
                    if self.is_non_rendered(child) || self.document.is_hidden_for_layout(child) {
                        continue;
                    }
                    if self.is_out_of_flow(child) {
                        continue;
                    }
                    let style = self.document.computed_style_for_layout(child);
                    if self.effective_display(child) == DisplayValue::None {
                        continue;
                    }
                    let margin = style.margin();
                    let width = self.flex_item_base_width(child, style, available_width, wrapped);
                    let flex_base_width = width;
                    let min_width = style
                        .min_width()
                        .map(|declared| outer_width_from_declared(style, declared))
                        .unwrap_or(0);
                    let max_width = style
                        .max_width()
                        .map(|declared| outer_width_from_declared(style, declared));
                    items.push(FlexItem {
                        child,
                        margin,
                        auto_margin: style.margin_auto(),
                        width,
                        height: 0,
                        flex_base_width,
                        flex_base_height: 0,
                        min_width,
                        min_height: 0,
                        max_width,
                        max_height: None,
                        flex_grow: style.flex_grow(),
                        flex_shrink: style.flex_shrink(),
                        order: style.flex_item_order().value(),
                        source_index,
                        align_self: style.align_self(),
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
        let reverse = (parent_style.flex_direction() == FlexDirectionValue::RowReverse)
            ^ (parent_style.direction() == DirectionValue::Rtl);
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
                line_y = line_y.saturating_add(line_height).saturating_add(row_gap);
            }
        }

        let line_gap_count = u32::try_from(line_count.saturating_sub(1)).unwrap_or(u32::MAX);
        let explicit_line_gap = row_gap.saturating_mul(line_gap_count);
        let total_line_height = line_records
            .iter()
            .fold(0u32, |total, line| total.saturating_add(line.height));
        let total_line_height = total_line_height.saturating_add(explicit_line_gap);
        let line_content_height = if wrapped {
            explicit_line_height.unwrap_or(total_line_height)
        } else {
            total_line_height
        };
        let free_space = line_content_height.saturating_sub(total_line_height);
        if wrapped
            && matches!(
                parent_style.align_content(),
                AlignContentValue::Stretch | AlignContentValue::Normal
            )
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
                stretched_line_y = stretched_line_y
                    .saturating_add(line.height)
                    .saturating_add(row_gap);
            }
        }
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
                | AlignContentValue::Stretch
                | AlignContentValue::Normal => 0,
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
                flex_space_around_offset(free_space, line_index, line_count)
            } else if wrapped && parent_style.align_content() == AlignContentValue::SpaceEvenly {
                flex_space_evenly_offset(free_space, line_index, line_count)
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
            for mut placement in line.layout.placements {
                let style = self.document.computed_style_for_layout(placement.child);
                let alignment = match placement.align_self {
                    AlignSelfValue::Auto => align_items,
                    AlignSelfValue::FlexStart => AlignItemsValue::FlexStart,
                    AlignSelfValue::Center => AlignItemsValue::Center,
                    AlignSelfValue::FlexEnd => AlignItemsValue::FlexEnd,
                    AlignSelfValue::Stretch => AlignItemsValue::Stretch,
                    AlignSelfValue::Normal => AlignItemsValue::Normal,
                };
                let cross_auto_count = vertical_auto_margin_count(placement.auto_margin);
                let cross_free_space = line
                    .height
                    .saturating_sub(placement.height.saturating_add(placement.margin.vertical()));
                let cross_auto_resolved = cross_auto_count > 0 && cross_free_space > 0;
                if cross_auto_resolved {
                    let mut slot = 0;
                    if placement.auto_margin.top() {
                        placement.margin = placement.margin.with_top(auto_margin_share(
                            cross_free_space,
                            slot,
                            cross_auto_count,
                        ));
                        slot = slot.saturating_add(1);
                    }
                    if placement.auto_margin.bottom() {
                        placement.margin = placement.margin.with_bottom(auto_margin_share(
                            cross_free_space,
                            slot,
                            cross_auto_count,
                        ));
                    }
                    self.shift_layout_y(
                        placement.box_start,
                        placement.box_end,
                        placement.text_start,
                        placement.text_end,
                        i64::from(placement.margin.top()),
                    );
                }
                if matches!(
                    alignment,
                    AlignItemsValue::Stretch | AlignItemsValue::Normal
                ) && !cross_auto_resolved
                    && let Some(stretched_height) = stretched_outer_height(
                        style,
                        line.height,
                        placement.margin,
                        placement.height,
                    )
                    && stretched_height > placement.height
                {
                    self.stretch_flex_item_box(&placement, style, stretched_height);
                    placement.height = stretched_height;
                }
                let item_outer_height =
                    placement.height.saturating_add(placement.margin.vertical());
                let remaining = line.height.saturating_sub(item_outer_height);
                let offset = if cross_auto_resolved {
                    0
                } else {
                    match alignment {
                        AlignItemsValue::FlexStart
                        | AlignItemsValue::Stretch
                        | AlignItemsValue::Normal => 0,
                        AlignItemsValue::Center => remaining / 2,
                        AlignItemsValue::FlexEnd => remaining,
                    }
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
        let containing_block = self.containing_block;
        self.layout_positioned_children(self.positioned_children(parent), containing_block, depth);
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
        let horizontal_inset = outer_width_inset(style);
        let default_outer_width = if is_block {
            available_width
        } else {
            self.intrinsic_inline_width(id, style)
                .saturating_add(horizontal_inset)
        };
        let width = style.width().map_or(default_outer_width, |declared| {
            outer_width_from_declared(style, declared)
        });
        let min_width = style
            .min_width()
            .map(|declared| outer_width_from_declared(style, declared));
        let max_width = style
            .max_width()
            .map(|declared| outer_width_from_declared(style, declared));
        let width = constrain_dimension(width, min_width, max_width);
        if constrain_to_available {
            width.min(available_width.max(min_width.unwrap_or_default()))
        } else {
            width
        }
    }

    fn flex_item_base_width(
        &self,
        id: NativeNodeId,
        style: NativeComputedStyle,
        available_width: u32,
        wrapped: bool,
    ) -> u32 {
        match style.flex_basis() {
            FlexBasisValue::Auto => self.outer_width(
                id,
                style,
                false,
                available_width,
                !wrapped && !self.explicit_width_can_overflow(id),
            ),
            FlexBasisValue::Length(declared) => {
                let width = outer_width_from_declared(style, declared);
                let min_width = style
                    .min_width()
                    .map(|value| outer_width_from_declared(style, value));
                let max_width = style
                    .max_width()
                    .map(|value| outer_width_from_declared(style, value));
                constrain_dimension(width, min_width, max_width)
            }
        }
    }

    fn flex_item_base_height(&self, id: NativeNodeId, style: NativeComputedStyle) -> u32 {
        let height = match style.flex_basis() {
            FlexBasisValue::Auto => style
                .height()
                .map(|declared| outer_height_from_declared(style, declared))
                .unwrap_or_else(|| {
                    self.intrinsic_inline_height(id)
                        .saturating_add(outer_height_inset(style))
                }),
            FlexBasisValue::Length(declared) => outer_height_from_declared(style, declared),
        };
        let min_height = style
            .min_height()
            .map(|declared| outer_height_from_declared(style, declared));
        let max_height = style
            .max_height()
            .map(|declared| outer_height_from_declared(style, declared));
        constrain_dimension(height, min_height, max_height)
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
            starts_line: false,
            ends_line: false,
            justify_spacing: 0,
            fixed: self.fixed,
            sticky: self.sticky_root.is_some(),
            z_index: self.stacking_context,
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
                self.flush_line_after_soft_wrap(flow);
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
            self.flush_line_after_soft_wrap(flow);
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
                self.flush_line_after_soft_wrap(flow);
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
            starts_line: false,
            ends_line: false,
            justify_spacing: 0,
            fixed: self.fixed,
            sticky: self.sticky_root.is_some(),
            z_index: self.stacking_context,
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
        Self::text_width_with_justification(value, letter_spacing, word_spacing, 0)
    }

    fn text_width_with_justification(
        value: &str,
        letter_spacing: u32,
        word_spacing: u32,
        justify_spacing: u32,
    ) -> u32 {
        value.chars().fold(0, |width, character| {
            width
                .saturating_add(Self::character_advance(
                    character,
                    letter_spacing,
                    word_spacing,
                ))
                .saturating_add(if character == ' ' { justify_spacing } else { 0 })
        })
    }

    fn intrinsic_inline_width(&self, id: NativeNodeId, style: NativeComputedStyle) -> u32 {
        let Some(node) = self.document.node(id) else {
            return 0;
        };
        match node.element_name() {
            Some("input" | "textarea" | "select") => DEFAULT_CONTROL_WIDTH,
            Some("svg") => node
                .attribute("width")
                .and_then(svg_length)
                .unwrap_or(DEFAULT_SVG_WIDTH),
            Some("img") => self
                .image_dimensions(id)
                .map(|(width, _)| width)
                .unwrap_or(CHARACTER_WIDTH),
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
            Some("svg") => self
                .document
                .node(id)
                .and_then(|node| node.attribute("height"))
                .and_then(svg_length)
                .unwrap_or(DEFAULT_SVG_HEIGHT),
            Some("img") => self
                .image_dimensions(id)
                .map(|(_, height)| height)
                .unwrap_or(DEFAULT_LINE_HEIGHT),
            _ => DEFAULT_LINE_HEIGHT,
        }
    }

    fn image_dimensions(&self, id: NativeNodeId) -> Option<(u32, u32)> {
        let node = self.document.node(id)?;
        let declared_width = node
            .attribute("width")
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|value| *value > 0);
        let declared_height = node
            .attribute("height")
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|value| *value > 0);
        let intrinsic = self
            .document
            .image_resource_for_node(id)
            .map(|image| (image.width, image.height))
            .or_else(|| node.attribute("src").and_then(image_dimensions_from_source));
        match (declared_width, declared_height, intrinsic) {
            (Some(width), Some(height), _) => Some((width, height)),
            (Some(width), None, Some((intrinsic_width, intrinsic_height)))
                if intrinsic_width > 0 =>
            {
                Some((
                    width,
                    u32::try_from(
                        u64::from(intrinsic_height) * u64::from(width) / u64::from(intrinsic_width),
                    )
                    .unwrap_or(u32::MAX)
                    .max(1),
                ))
            }
            (None, Some(height), Some((intrinsic_width, intrinsic_height)))
                if intrinsic_height > 0 =>
            {
                Some((
                    u32::try_from(
                        u64::from(intrinsic_width) * u64::from(height)
                            / u64::from(intrinsic_height),
                    )
                    .unwrap_or(u32::MAX)
                    .max(1),
                    height,
                ))
            }
            (Some(width), None, _) => Some((width, DEFAULT_LINE_HEIGHT)),
            (None, Some(height), _) => Some((CHARACTER_WIDTH, height)),
            (None, None, intrinsic) => intrinsic,
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

pub(crate) fn svg_points(node: &NativeNode) -> Option<Vec<NativePoint>> {
    let value = node.attribute("points")?;
    let coordinates = value
        .split(|character: char| character == ',' || character.is_ascii_whitespace())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if coordinates.len() < 4
        || !coordinates.len().is_multiple_of(2)
        || coordinates.len() / 2 > MAX_NATIVE_SVG_POINTS
    {
        return None;
    }
    let mut points = Vec::with_capacity(coordinates.len() / 2);
    for pair in coordinates.chunks_exact(2) {
        points.push(NativePoint {
            x: pair[0].parse().ok()?,
            y: pair[1].parse().ok()?,
        });
    }
    Some(points)
}

pub(crate) fn svg_line_points(node: &NativeNode) -> Vec<NativePoint> {
    let number = |name: &str| {
        node.attribute(name)
            .and_then(|value| value.trim().parse::<u32>().ok())
            .unwrap_or(0)
    };
    vec![
        NativePoint {
            x: number("x1"),
            y: number("y1"),
        },
        NativePoint {
            x: number("x2"),
            y: number("y2"),
        },
    ]
}

pub(crate) fn svg_transform_for_node(
    document: &NativeDocument,
    node_id: NativeNodeId,
) -> Option<NativeSvgTransform> {
    let mut ancestors = Vec::new();
    let mut current = Some(node_id);
    while let Some(id) = current {
        let node = document.node(id)?;
        ancestors.push(id);
        current = node.parent();
    }
    let mut transform = NativeSvgTransform::IDENTITY;
    let mut in_svg = false;
    for id in ancestors.iter().rev() {
        let node = document.node(*id)?;
        if node.element_name() == Some("svg") {
            in_svg = true;
        }
        if in_svg {
            let mut local = NativeSvgTransform::IDENTITY;
            if node.element_name() == Some("svg")
                && let Some(value) = node.attribute("viewBox")
            {
                local = local.followed_by(svg_viewbox_transform(node, value)?);
            }
            if let Some(value) = node.attribute("transform") {
                local = local.followed_by(parse_svg_transform(value)?);
            }
            transform = local.followed_by(transform);
        }
    }
    Some(transform)
}

fn svg_viewbox_transform(node: &NativeNode, value: &str) -> Option<NativeSvgTransform> {
    let values = tokenize_svg_path(value)?
        .into_iter()
        .map(|token| match token {
            SvgPathToken::Number(value) => Some(value),
            SvgPathToken::Command(_) => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if values.len() != 4
        || !values.iter().all(|value| value.is_finite())
        || values[2] <= 0.0
        || values[3] <= 0.0
    {
        return None;
    }
    let viewport_width = node
        .attribute("width")
        .and_then(svg_length)
        .map(f64::from)
        .unwrap_or(f64::from(DEFAULT_SVG_WIDTH));
    let viewport_height = node
        .attribute("height")
        .and_then(svg_length)
        .map(f64::from)
        .unwrap_or(f64::from(DEFAULT_SVG_HEIGHT));
    if !viewport_width.is_finite()
        || viewport_width <= 0.0
        || !viewport_height.is_finite()
        || viewport_height <= 0.0
    {
        return None;
    }
    let (align_x, align_y, slice) = svg_preserve_aspect_ratio(node)?;
    let viewbox_width = values[2];
    let viewbox_height = values[3];
    let scale_x = viewport_width / viewbox_width;
    let scale_y = viewport_height / viewbox_height;
    let (scale_x, scale_y) = if align_x.is_none() {
        (scale_x, scale_y)
    } else {
        let scale = if slice {
            scale_x.max(scale_y)
        } else {
            scale_x.min(scale_y)
        };
        (scale, scale)
    };
    let rendered_width = viewbox_width * scale_x;
    let rendered_height = viewbox_height * scale_y;
    let offset_x = align_x.unwrap_or(0.0) * (viewport_width - rendered_width) - values[0] * scale_x;
    let offset_y =
        align_y.unwrap_or(0.0) * (viewport_height - rendered_height) - values[1] * scale_y;
    let transform = NativeSvgTransform {
        a: scale_x,
        d: scale_y,
        e: offset_x,
        f: offset_y,
        ..NativeSvgTransform::IDENTITY
    };
    [
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.e,
        transform.f,
    ]
    .iter()
    .all(|value| value.is_finite())
    .then_some(transform)
}

fn svg_length(value: &str) -> Option<u32> {
    let value = value.trim();
    let value = value.strip_suffix("px").map(str::trim).unwrap_or(value);
    if value.is_empty() || value.ends_with('%') {
        return None;
    }
    let value = value.parse::<f64>().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    u32::try_from(value.ceil() as u64)
        .ok()
        .filter(|value| *value > 0)
}

fn svg_preserve_aspect_ratio(node: &NativeNode) -> Option<(Option<f64>, Option<f64>, bool)> {
    let value = node
        .attribute("preserveAspectRatio")
        .unwrap_or("xMidYMid meet");
    let mut tokens = value.split_ascii_whitespace();
    let mut token = tokens.next()?.to_ascii_lowercase();
    if token == "defer" {
        token = tokens.next()?.to_ascii_lowercase();
    }
    if token == "none" {
        return tokens.next().is_none().then_some((None, None, false));
    }
    let (align_x, align_y) = match token.as_str() {
        "xminymin" => (0.0, 0.0),
        "xminymid" => (0.0, 0.5),
        "xminymax" => (0.0, 1.0),
        "xmidymin" => (0.5, 0.0),
        "xmidymid" => (0.5, 0.5),
        "xmidymax" => (0.5, 1.0),
        "xmaxymin" => (1.0, 0.0),
        "xmaxymid" => (1.0, 0.5),
        "xmaxymax" => (1.0, 1.0),
        _ => return None,
    };
    let slice = match tokens
        .next()
        .unwrap_or("meet")
        .to_ascii_lowercase()
        .as_str()
    {
        "meet" => false,
        "slice" => true,
        _ => return None,
    };
    tokens
        .next()
        .is_none()
        .then_some((Some(align_x), Some(align_y), slice))
}

pub(crate) fn svg_transformed_points(
    node: &NativeNode,
    transform: NativeSvgTransform,
) -> Option<Vec<NativePoint>> {
    let number = |name: &str| {
        node.attribute(name)
            .map(|value| value.trim().parse::<f64>().ok())
            .unwrap_or(Some(0.0))
            .filter(|value| value.is_finite())
    };
    let points = match node.element_name()? {
        "rect" => {
            let x = number("x")?;
            let y = number("y")?;
            let width = number("width")?;
            let height = number("height")?;
            if width < 0.0 || height < 0.0 {
                return None;
            }
            vec![
                (x, y),
                (x + width, y),
                (x + width, y + height),
                (x, y + height),
            ]
        }
        "circle" => {
            let center_x = number("cx")?;
            let center_y = number("cy")?;
            let radius = number("r")?;
            if radius < 0.0 {
                return None;
            }
            sampled_ellipse_points(center_x, center_y, radius, radius)
        }
        "ellipse" => {
            let center_x = number("cx")?;
            let center_y = number("cy")?;
            let radius_x = number("rx")?;
            let radius_y = number("ry")?;
            if radius_x < 0.0 || radius_y < 0.0 {
                return None;
            }
            sampled_ellipse_points(center_x, center_y, radius_x, radius_y)
        }
        "line" => svg_line_points(node)
            .into_iter()
            .map(|point| (f64::from(point.x), f64::from(point.y)))
            .collect(),
        "polyline" | "polygon" => svg_points(node)?
            .into_iter()
            .map(|point| (f64::from(point.x), f64::from(point.y)))
            .collect(),
        _ => return None,
    };
    transform_points(&points, transform)
}

pub(crate) fn svg_transformed_subpaths(
    node: &NativeNode,
    transform: NativeSvgTransform,
) -> Option<Vec<NativeSvgSubpath>> {
    svg_path_subpaths(node)?
        .into_iter()
        .map(|subpath| {
            Some(NativeSvgSubpath {
                points: transform_points(
                    &subpath
                        .points
                        .iter()
                        .map(|point| (f64::from(point.x), f64::from(point.y)))
                        .collect::<Vec<_>>(),
                    transform,
                )?,
                closed: subpath.closed,
            })
        })
        .collect()
}

const MAX_NATIVE_SVG_ELLIPSE_POINTS: usize = 32;

fn sampled_ellipse_points(
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
) -> Vec<(f64, f64)> {
    (0..MAX_NATIVE_SVG_ELLIPSE_POINTS)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / MAX_NATIVE_SVG_ELLIPSE_POINTS as f64;
            (
                center_x + radius_x * angle.cos(),
                center_y + radius_y * angle.sin(),
            )
        })
        .collect()
}

fn transform_points(
    points: &[(f64, f64)],
    transform: NativeSvgTransform,
) -> Option<Vec<NativePoint>> {
    if points.len() > MAX_NATIVE_SVG_POINTS {
        return None;
    }
    points
        .iter()
        .copied()
        .map(|point| transform.apply(point).map(svg_path_point))
        .collect()
}

fn parse_svg_transform(value: &str) -> Option<NativeSvgTransform> {
    let characters = value.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut transform = NativeSvgTransform::IDENTITY;
    while index < characters.len() {
        while index < characters.len()
            && (characters[index].is_ascii_whitespace() || characters[index] == ',')
        {
            index = index.saturating_add(1);
        }
        if index == characters.len() {
            break;
        }
        let name_start = index;
        while index < characters.len() && characters[index].is_ascii_alphabetic() {
            index = index.saturating_add(1);
        }
        if name_start == index {
            return None;
        }
        let name = characters[name_start..index]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        while index < characters.len() && characters[index].is_ascii_whitespace() {
            index = index.saturating_add(1);
        }
        if characters.get(index) != Some(&'(') {
            return None;
        }
        index = index.saturating_add(1);
        let argument_start = index;
        while index < characters.len() && characters[index] != ')' {
            if characters[index] == '(' {
                return None;
            }
            index = index.saturating_add(1);
        }
        if index == characters.len() {
            return None;
        }
        let argument_text = characters[argument_start..index].iter().collect::<String>();
        let arguments = tokenize_svg_path(&argument_text)?
            .into_iter()
            .map(|token| match token {
                SvgPathToken::Number(value) => Some(value),
                SvgPathToken::Command(_) => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let local = match name.as_str() {
            "matrix" if arguments.len() == 6 => NativeSvgTransform {
                a: arguments[0],
                b: arguments[1],
                c: arguments[2],
                d: arguments[3],
                e: arguments[4],
                f: arguments[5],
            },
            "translate" if matches!(arguments.len(), 1 | 2) => NativeSvgTransform::translation(
                arguments[0],
                arguments.get(1).copied().unwrap_or(0.0),
            ),
            "scale" if matches!(arguments.len(), 1 | 2) => NativeSvgTransform::scale(
                arguments[0],
                arguments.get(1).copied().unwrap_or(arguments[0]),
            ),
            "rotate" if matches!(arguments.len(), 1 | 3) => {
                let rotation = NativeSvgTransform::rotation(arguments[0].to_radians());
                if arguments.len() == 1 {
                    rotation
                } else {
                    NativeSvgTransform::translation(-arguments[1], -arguments[2])
                        .followed_by(rotation)
                        .followed_by(NativeSvgTransform::translation(arguments[1], arguments[2]))
                }
            }
            "skewx" if arguments.len() == 1 => {
                let tangent = arguments[0].to_radians().tan();
                NativeSvgTransform {
                    c: tangent,
                    ..NativeSvgTransform::IDENTITY
                }
            }
            "skewy" if arguments.len() == 1 => {
                let tangent = arguments[0].to_radians().tan();
                NativeSvgTransform {
                    b: tangent,
                    ..NativeSvgTransform::IDENTITY
                }
            }
            _ => return None,
        };
        if ![local.a, local.b, local.c, local.d, local.e, local.f]
            .iter()
            .all(|number| number.is_finite())
        {
            return None;
        }
        transform = transform.followed_by(local);
        index = index.saturating_add(1);
    }
    Some(transform)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SvgPathToken {
    Command(char),
    Number(f64),
}

pub(crate) fn svg_path_subpaths(node: &NativeNode) -> Option<Vec<NativeSvgSubpath>> {
    let value = node.attribute("d")?;
    let tokens = tokenize_svg_path(value)?;
    let mut subpaths: Vec<NativeSvgSubpath> = Vec::new();
    let mut current = (0.0, 0.0);
    let mut command = None;
    let mut previous_command = None;
    let mut previous_cubic_control = None;
    let mut previous_quadratic_control = None;
    let mut index = 0usize;
    while index < tokens.len() {
        if let SvgPathToken::Command(next) = tokens[index] {
            if !matches!(
                next,
                'M' | 'm'
                    | 'L'
                    | 'l'
                    | 'H'
                    | 'h'
                    | 'V'
                    | 'v'
                    | 'C'
                    | 'c'
                    | 'Q'
                    | 'q'
                    | 'S'
                    | 's'
                    | 'T'
                    | 't'
                    | 'A'
                    | 'a'
                    | 'Z'
                    | 'z'
            ) {
                return None;
            }
            index = index.saturating_add(1);
            if matches!(next, 'Z' | 'z') {
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                subpath.closed = true;
                current = subpath
                    .points
                    .first()
                    .map(|point| (f64::from(point.x), f64::from(point.y)))
                    .unwrap_or(current);
                command = None;
                previous_command = Some(next);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            } else {
                command = Some(next);
            }
            continue;
        }

        let Some(next) = command else {
            return None;
        };
        let relative = next.is_ascii_lowercase();
        let upper = next.to_ascii_uppercase();
        match upper {
            'M' | 'L' => {
                let (x, y) = svg_path_pair(&tokens, &mut index)?;
                let point = if relative {
                    (current.0 + x, current.1 + y)
                } else {
                    (x, y)
                };
                if upper == 'M' {
                    subpaths.push(NativeSvgSubpath {
                        points: vec![svg_path_point(point)],
                        closed: false,
                    });
                    if subpaths.len() > MAX_NATIVE_SVG_POINTS {
                        return None;
                    }
                    command = Some(if relative { 'l' } else { 'L' });
                } else {
                    let Some(subpath) = subpaths.last_mut() else {
                        return None;
                    };
                    subpath.points.push(svg_path_point(point));
                }
                current = point;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            'H' => {
                let x = svg_path_number(&tokens, &mut index)?;
                let point = (if relative { current.0 + x } else { x }, current.1);
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                subpath.points.push(svg_path_point(point));
                current = point;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            'V' => {
                let y = svg_path_number(&tokens, &mut index)?;
                let point = (current.0, if relative { current.1 + y } else { y });
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                subpath.points.push(svg_path_point(point));
                current = point;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            'C' => {
                let control_1 = svg_path_pair(&tokens, &mut index)?;
                let control_2 = svg_path_pair(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                let control_1 = if relative {
                    (current.0 + control_1.0, current.1 + control_1.1)
                } else {
                    control_1
                };
                let control_2 = if relative {
                    (current.0 + control_2.0, current.1 + control_2.1)
                } else {
                    control_2
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                append_cubic_curve(&mut subpath.points, current, control_1, control_2, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = Some(control_2);
                previous_quadratic_control = None;
            }
            'Q' => {
                let control = svg_path_pair(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                let control = if relative {
                    (current.0 + control.0, current.1 + control.1)
                } else {
                    control
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                append_quadratic_curve(&mut subpath.points, current, control, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = Some(control);
            }
            'S' => {
                let control_2 = svg_path_pair(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                let control_1 = if matches!(previous_command, Some('C' | 'S')) {
                    previous_cubic_control
                        .map(|control| (current.0 * 2.0 - control.0, current.1 * 2.0 - control.1))
                        .unwrap_or(current)
                } else {
                    current
                };
                let control_2 = if relative {
                    (current.0 + control_2.0, current.1 + control_2.1)
                } else {
                    control_2
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                append_cubic_curve(&mut subpath.points, current, control_1, control_2, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = Some(control_2);
                previous_quadratic_control = None;
            }
            'T' => {
                let end = svg_path_pair(&tokens, &mut index)?;
                let control = if matches!(previous_command, Some('Q' | 'T')) {
                    previous_quadratic_control
                        .map(|control| (current.0 * 2.0 - control.0, current.1 * 2.0 - control.1))
                        .unwrap_or(current)
                } else {
                    current
                };
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                append_quadratic_curve(&mut subpath.points, current, control, end);
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = Some(control);
            }
            'A' => {
                let radius_x = svg_path_number(&tokens, &mut index)?;
                let radius_y = svg_path_number(&tokens, &mut index)?;
                let rotation = svg_path_number(&tokens, &mut index)?;
                let large_arc = svg_path_number(&tokens, &mut index)?;
                let sweep = svg_path_number(&tokens, &mut index)?;
                let end = svg_path_pair(&tokens, &mut index)?;
                if !matches!(large_arc, 0.0 | 1.0) || !matches!(sweep, 0.0 | 1.0) {
                    return None;
                }
                let end = if relative {
                    (current.0 + end.0, current.1 + end.1)
                } else {
                    end
                };
                let Some(subpath) = subpaths.last_mut() else {
                    return None;
                };
                if !append_elliptical_arc(
                    &mut subpath.points,
                    current,
                    radius_x,
                    radius_y,
                    rotation,
                    large_arc == 1.0,
                    sweep == 1.0,
                    end,
                ) {
                    return None;
                }
                current = end;
                previous_command = Some(upper);
                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
            _ => return None,
        }
        let point_count = subpaths
            .iter()
            .map(|subpath| subpath.points.len())
            .sum::<usize>();
        if point_count > MAX_NATIVE_SVG_POINTS {
            return None;
        }
    }
    (!subpaths.is_empty()).then_some(subpaths)
}

fn tokenize_svg_path(value: &str) -> Option<Vec<SvgPathToken>> {
    let characters = value.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < characters.len() {
        let character = characters[index];
        if character.is_ascii_alphabetic() {
            tokens.push(SvgPathToken::Command(character));
            index = index.saturating_add(1);
            continue;
        }
        if character == ',' || character.is_ascii_whitespace() {
            index = index.saturating_add(1);
            continue;
        }
        let start = index;
        if matches!(character, '+' | '-') {
            index = index.saturating_add(1);
        }
        let mut has_digit = false;
        while index < characters.len() && characters[index].is_ascii_digit() {
            has_digit = true;
            index = index.saturating_add(1);
        }
        if index < characters.len() && characters[index] == '.' {
            index = index.saturating_add(1);
            while index < characters.len() && characters[index].is_ascii_digit() {
                has_digit = true;
                index = index.saturating_add(1);
            }
        }
        if !has_digit {
            return None;
        }
        if index < characters.len() && matches!(characters[index], 'e' | 'E') {
            let exponent = index;
            index = index.saturating_add(1);
            if index < characters.len() && matches!(characters[index], '+' | '-') {
                index = index.saturating_add(1);
            }
            let exponent_start = index;
            while index < characters.len() && characters[index].is_ascii_digit() {
                index = index.saturating_add(1);
            }
            if exponent_start == index {
                index = exponent;
            }
        }
        let number = characters[start..index].iter().collect::<String>();
        tokens.push(SvgPathToken::Number(number.parse().ok()?));
        if tokens.len() > MAX_NATIVE_SVG_POINTS.saturating_mul(4) {
            return None;
        }
    }
    Some(tokens)
}

fn svg_path_number(tokens: &[SvgPathToken], index: &mut usize) -> Option<f64> {
    let SvgPathToken::Number(value) = tokens.get(*index).copied()? else {
        return None;
    };
    *index = index.saturating_add(1);
    Some(value)
}

fn svg_path_pair(tokens: &[SvgPathToken], index: &mut usize) -> Option<(f64, f64)> {
    Some((
        svg_path_number(tokens, index)?,
        svg_path_number(tokens, index)?,
    ))
}

fn append_cubic_curve(
    points: &mut Vec<NativePoint>,
    start: (f64, f64),
    control_1: (f64, f64),
    control_2: (f64, f64),
    end: (f64, f64),
) {
    for segment in 1..=MAX_NATIVE_SVG_CURVE_SEGMENTS {
        let t = segment as f64 / MAX_NATIVE_SVG_CURVE_SEGMENTS as f64;
        let inverse = 1.0 - t;
        let inverse_squared = inverse * inverse;
        let t_squared = t * t;
        let x = inverse_squared * inverse * start.0
            + 3.0 * inverse_squared * t * control_1.0
            + 3.0 * inverse * t_squared * control_2.0
            + t_squared * t * end.0;
        let y = inverse_squared * inverse * start.1
            + 3.0 * inverse_squared * t * control_1.1
            + 3.0 * inverse * t_squared * control_2.1
            + t_squared * t * end.1;
        points.push(svg_path_point((x, y)));
    }
}

fn append_quadratic_curve(
    points: &mut Vec<NativePoint>,
    start: (f64, f64),
    control: (f64, f64),
    end: (f64, f64),
) {
    for segment in 1..=MAX_NATIVE_SVG_CURVE_SEGMENTS {
        let t = segment as f64 / MAX_NATIVE_SVG_CURVE_SEGMENTS as f64;
        let inverse = 1.0 - t;
        let x = inverse * inverse * start.0 + 2.0 * inverse * t * control.0 + t * t * end.0;
        let y = inverse * inverse * start.1 + 2.0 * inverse * t * control.1 + t * t * end.1;
        points.push(svg_path_point((x, y)));
    }
}

fn append_elliptical_arc(
    points: &mut Vec<NativePoint>,
    start: (f64, f64),
    radius_x: f64,
    radius_y: f64,
    rotation_degrees: f64,
    large_arc: bool,
    sweep: bool,
    end: (f64, f64),
) -> bool {
    if ![
        start.0,
        start.1,
        radius_x,
        radius_y,
        rotation_degrees,
        end.0,
        end.1,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        return false;
    }
    let radius_x = radius_x.abs();
    let radius_y = radius_y.abs();
    if radius_x == 0.0 || radius_y == 0.0 {
        if points.len() >= MAX_NATIVE_SVG_POINTS {
            return false;
        }
        points.push(svg_path_point(end));
        return true;
    }
    if start == end {
        return true;
    }

    let rotation = rotation_degrees.to_radians();
    let (sin_rotation, cos_rotation) = rotation.sin_cos();
    let delta_x = (start.0 - end.0) / 2.0;
    let delta_y = (start.1 - end.1) / 2.0;
    let transformed_x = cos_rotation * delta_x + sin_rotation * delta_y;
    let transformed_y = -sin_rotation * delta_x + cos_rotation * delta_y;
    let radius_x_squared = radius_x * radius_x;
    let radius_y_squared = radius_y * radius_y;
    let transformed_x_squared = transformed_x * transformed_x;
    let transformed_y_squared = transformed_y * transformed_y;
    let radii_ratio =
        transformed_x_squared / radius_x_squared + transformed_y_squared / radius_y_squared;
    if !radii_ratio.is_finite() {
        return false;
    }
    let radius_scale = radii_ratio.sqrt().max(1.0);
    let radius_x = radius_x * radius_scale;
    let radius_y = radius_y * radius_scale;
    let radius_x_squared = radius_x * radius_x;
    let radius_y_squared = radius_y * radius_y;
    let numerator = radius_x_squared * radius_y_squared
        - radius_x_squared * transformed_y_squared
        - radius_y_squared * transformed_x_squared;
    let denominator =
        radius_x_squared * transformed_y_squared + radius_y_squared * transformed_x_squared;
    if ![radius_x, radius_y, numerator, denominator]
        .iter()
        .all(|value| value.is_finite())
        || denominator == 0.0
    {
        return false;
    }
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let center_scale = sign * (numerator.max(0.0) / denominator).sqrt();
    let center_x = center_scale * radius_x * transformed_y / radius_y;
    let center_y = center_scale * -radius_y * transformed_x / radius_x;
    let center = (
        cos_rotation * center_x - sin_rotation * center_y + (start.0 + end.0) / 2.0,
        sin_rotation * center_x + cos_rotation * center_y + (start.1 + end.1) / 2.0,
    );
    let start_vector = (
        (transformed_x - center_x) / radius_x,
        (transformed_y - center_y) / radius_y,
    );
    let end_vector = (
        (-transformed_x - center_x) / radius_x,
        (-transformed_y - center_y) / radius_y,
    );
    let start_angle = start_vector.1.atan2(start_vector.0);
    let mut sweep_angle = (start_vector.0 * end_vector.1 - start_vector.1 * end_vector.0)
        .atan2(start_vector.0 * end_vector.0 + start_vector.1 * end_vector.1);
    if sweep && sweep_angle < 0.0 {
        sweep_angle += std::f64::consts::TAU;
    } else if !sweep && sweep_angle > 0.0 {
        sweep_angle -= std::f64::consts::TAU;
    }
    if ![
        center.0,
        center.1,
        start_angle,
        sweep_angle,
        radius_x,
        radius_y,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        return false;
    }
    let segment_count = ((sweep_angle.abs() / (std::f64::consts::PI / 2.0)) * 16.0)
        .ceil()
        .max(1.0)
        .min(MAX_NATIVE_SVG_ARC_SEGMENTS as f64) as usize;
    if points.len().saturating_add(segment_count) > MAX_NATIVE_SVG_POINTS {
        return false;
    }
    for segment in 1..=segment_count {
        let progress = segment as f64 / segment_count as f64;
        let angle = start_angle + sweep_angle * progress;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let point = if segment == segment_count {
            end
        } else {
            (
                center.0 + cos_rotation * radius_x * cos_angle
                    - sin_rotation * radius_y * sin_angle,
                center.1
                    + sin_rotation * radius_x * cos_angle
                    + cos_rotation * radius_y * sin_angle,
            )
        };
        if !point.0.is_finite() || !point.1.is_finite() {
            return false;
        }
        points.push(svg_path_point(point));
    }
    true
}

fn svg_path_point((x, y): (f64, f64)) -> NativePoint {
    let coordinate = |value: f64| {
        if !value.is_finite() {
            return 0;
        }
        value.round().max(0.0).min(f64::from(u32::MAX)) as u32
    };
    NativePoint {
        x: coordinate(x),
        y: coordinate(y),
    }
}

fn svg_points_bounds(points: &[NativePoint]) -> Option<(u32, u32, u32, u32)> {
    let first = points.first().copied()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x;
    let mut max_y = first.y;
    for point in &points[1..] {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    Some((min_x, min_y, max_x, max_y))
}

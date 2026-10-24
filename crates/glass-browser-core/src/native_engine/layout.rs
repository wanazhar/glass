#[path = "layout/builder.rs"]
mod builder;
#[path = "layout/svg.rs"]
mod svg;
use super::config::{MAX_NATIVE_DOM_DEPTH, Viewport};
use super::css::{
    AlignItemsValue, AlignSelfValue, DirectionValue, JustifyContentValue, NativeAutoEdges,
    NativeBorderRadius, NativeBoxEdges, NativeComputedStyle, NativePositionOffset, OverflowValue,
    TextAlignLastValue, TextAlignValue, VerticalAlignValue,
};
use super::dom::{MAX_NATIVE_CANVAS_DIMENSION, NativeDocument, NativeNode, NativeNodeId};
use super::error::NativeEngineError;
use super::font::NativeTextMetrics;
use builder::LayoutBuilder;
use std::collections::{BTreeMap, HashMap};
pub(crate) use svg::{
    svg_image_viewport, svg_line_points, svg_path_subpaths, svg_points, svg_preserve_aspect_ratio,
    svg_transform_for_node, svg_transform_points, svg_transformed_points, svg_transformed_subpaths,
};

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
        let build_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "native_layout_tree_build",
        );
        let flow = builder.layout_children(document.root(), 0, 0, viewport.width, 0);
        build_stage.finish(Some(builder.boxes.len()));
        let scroll_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "native_layout_scroll_containers",
        );
        let scroll_containers = scroll_containers_for(document, &builder.boxes, &builder.text_runs);
        scroll_stage.finish(Some(scroll_containers.len()));
        let extent_stage = crate::native_engine::content_process::ContentLoadStage::begin(
            0,
            "native_layout_extent_projection",
        );
        let mut overflow_ancestors = OverflowClipAncestorCache::with_capacity(builder.boxes.len());
        let mut max_box_bottom = 0;
        let mut max_box_right = 0;
        for layout_box in &builder.boxes {
            if layout_box.fixed {
                continue;
            }
            if !overflow_ancestors.has_clip_ancestor(document, layout_box.node_id, false, false) {
                max_box_bottom = max_box_bottom.max(layout_box.rect.bottom());
            }
            if !overflow_ancestors.has_clip_ancestor(document, layout_box.node_id, false, true) {
                max_box_right = max_box_right.max(layout_box.rect.right());
            }
        }
        let mut max_text_right = 0;
        for text_run in &builder.text_runs {
            if text_run.fixed
                || text_run.truncated
                || text_run.text.is_empty()
                || overflow_ancestors.has_clip_ancestor(document, text_run.node_id, true, true)
            {
                continue;
            }
            let style = document.computed_style_for_layout(text_run.node_id);
            let text_rect = NativeRect {
                x: text_run.origin.x,
                y: text_run.origin.y,
                width: document
                    .text_metrics_for_layout(text_run.node_id)
                    .measure_text(&text_run.text, style.letter_spacing(), style.word_spacing())
                    .saturating_add(
                        text_run
                            .text
                            .chars()
                            .filter(|character| *character == ' ')
                            .count()
                            .try_into()
                            .unwrap_or(u32::MAX)
                            .saturating_mul(text_run.justify_spacing),
                    ),
                height: document.text_line_height_for_layout(text_run.node_id),
            };
            let visible_rect = overflow_clip_for(document, &builder.boxes, text_run.node_id)
                .map_or(text_rect, |clip| intersect_rect(text_rect, clip));
            if visible_rect.width > 0 && visible_rect.height > 0 {
                max_text_right = max_text_right.max(visible_rect.right());
            }
        }
        extent_stage.finish(Some(builder.text_runs.len()));
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
    pub fn viewport_projection_for(
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

struct OverflowClipAncestorCache {
    includes_self_by_node: HashMap<NativeNodeId, (bool, bool)>,
}

impl OverflowClipAncestorCache {
    fn with_capacity(node_count: usize) -> Self {
        Self {
            includes_self_by_node: HashMap::with_capacity(node_count),
        }
    }

    fn has_clip_ancestor(
        &mut self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        include_self: bool,
        horizontal: bool,
    ) -> bool {
        let start = if include_self {
            Some(node_id)
        } else {
            document.node(node_id).and_then(|node| node.parent())
        };
        let Some(start) = start else {
            return false;
        };
        let flags = self.includes_self(document, start);
        if horizontal { flags.0 } else { flags.1 }
    }

    fn includes_self(&mut self, document: &NativeDocument, node_id: NativeNodeId) -> (bool, bool) {
        if let Some(flags) = self.includes_self_by_node.get(&node_id) {
            return *flags;
        }

        let mut path = Vec::with_capacity(MAX_NATIVE_DOM_DEPTH.min(16));
        let mut current = Some(node_id);
        let mut flags = (false, false);
        for _ in 0..=MAX_NATIVE_DOM_DEPTH {
            let Some(current_id) = current else {
                break;
            };
            if let Some(cached) = self.includes_self_by_node.get(&current_id) {
                flags = *cached;
                break;
            }
            path.push(current_id);
            current = document.node(current_id).and_then(|node| node.parent());
        }
        for current_id in path.into_iter().rev() {
            let style = document.computed_style_for_layout(current_id);
            flags.0 |= style.overflow_clip_x();
            flags.1 |= style.overflow_clip_y();
            self.includes_self_by_node.insert(current_id, flags);
        }
        flags
    }
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

#[allow(clippy::too_many_arguments)]
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

#[derive(Debug, Clone)]
struct FlowStyle {
    font: NativeTextMetrics,
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
    font: NativeTextMetrics,
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
            font: style.font,
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
            let advance = self.character_advance(character);
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
        self.font
            .measure_text(value, self.letter_spacing, self.word_spacing)
    }

    fn character_advance(&self, character: char) -> u32 {
        self.font
            .advance(character, self.letter_spacing, self.word_spacing)
    }

    fn text_line_height(&self) -> u32 {
        self.font.line_height()
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

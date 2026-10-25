use super::super::css::{
    AlignContentValue, DisplayValue, FlexBasisValue, FlexDirectionValue, FlexWrapValue,
    FontStyleValue, FontWeightValue, NativeFontFamilyList, NativeFontKerning,
    NativeFontOpticalSizing, NativeFontVariantAlternates, NativeFontVariantCaps,
    NativeFontVariantEastAsian, NativeFontVariantLigatures, NativeFontVariantNumeric,
    NativeFontVariantPosition, NativeFontVariationSettings, NativeGridTrack, NativeGridTrackList,
    NativePositionValue, TextJustifyValue, TextOverflowValue, TextTransformValue, WhiteSpaceValue,
    WordBreakValue,
};
use super::super::dom::NativeNodeKind;
use super::super::image::image_dimensions_from_source;
use super::svg::{svg_length, svg_points_bounds};
use super::*;

#[path = "builder/flex.rs"]
mod flex;
#[path = "builder/grid.rs"]
mod grid;
#[path = "builder/text.rs"]
mod text;

const MAX_LAYOUT_TEXT_WIDTH_CACHE_ENTRIES: usize = 4096;
const MAX_LAYOUT_TEXT_WIDTH_CACHE_TEXT_BYTES: usize = 128;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct NativeTextWidthCacheKey {
    font_family: NativeFontFamilyList,
    font_size: u32,
    font_weight: FontWeightValue,
    font_style: FontStyleValue,
    font_stretch: u16,
    letter_spacing: u32,
    word_spacing: u32,
}

pub(super) struct LayoutBuilder<'a> {
    pub(super) document: &'a NativeDocument,
    pub(super) boxes: Vec<NativeLayoutBox>,
    pub(super) text_runs: Vec<NativeTextLayout>,
    pub(super) paint_order: Vec<NativeLayoutPaintOrder>,
    pub(super) sticky_ranges: Vec<NativeStickyRange>,
    pub(super) initial_containing_block: PositionedContainingBlock,
    pub(super) containing_block: PositionedContainingBlock,
    pub(super) fixed: bool,
    pub(super) sticky_root: Option<usize>,
    pub(super) stacking_context: i32,
    pub(super) text_width_cache: HashMap<NativeTextWidthCacheKey, HashMap<String, u32>>,
    pub(super) text_width_cache_entries: usize,
    pub(super) text_width_cache_hits: usize,
}

impl<'a> LayoutBuilder<'a> {
    pub(super) fn layout_children(
        &mut self,
        parent: NativeNodeId,
        x: u32,
        y: u32,
        available_width: u32,
        depth: usize,
    ) -> FlowSize {
        let style = self.document.computed_style_for_layout(parent);
        let font = self.document.text_metrics_for_layout(parent);
        let minimum_line_height = style
            .line_height()
            .unwrap_or(DEFAULT_LINE_HEIGHT)
            .max(font.line_height());
        let white_space = self
            .document
            .computed_style_for_layout(parent)
            .white_space();
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
                font,
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
                NativeNodeKind::Document | NativeNodeKind::DocumentFragment => {
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
        let minimum_line_height = style
            .line_height()
            .unwrap_or(DEFAULT_LINE_HEIGHT)
            .max(self.document.text_line_height_for_layout(id));
        let default_content_height = if is_block {
            minimum_line_height
        } else if matches!(
            self.document.node(id).and_then(|node| node.element_name()),
            Some("svg" | "img")
        ) {
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
        if !transform.is_identity() || node.element_name() == Some("image") {
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
                NativeNodeKind::Document | NativeNodeKind::DocumentFragment => false,
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
                    NativeNodeKind::Document | NativeNodeKind::DocumentFragment => false,
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

    fn intrinsic_inline_width(&mut self, id: NativeNodeId, style: NativeComputedStyle) -> u32 {
        let Some(node) = self.document.node(id) else {
            return 0;
        };
        match node.element_name() {
            Some("input" | "textarea" | "select") => DEFAULT_CONTROL_WIDTH,
            Some("canvas") => self.canvas_dimensions(id).0,
            Some("svg") => node
                .attribute("width")
                .and_then(svg_length)
                .unwrap_or(DEFAULT_SVG_WIDTH),
            Some("img") => self
                .image_dimensions(id)
                .map(|(width, _)| width)
                .unwrap_or(CHARACTER_WIDTH),
            Some("video") => self
                .video_poster_dimensions(id)
                .map(|(width, _)| width)
                .unwrap_or_else(|| self.intrinsic_text_width(id, style).max(CHARACTER_WIDTH)),
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

    fn intrinsic_text_width(&mut self, id: NativeNodeId, style: NativeComputedStyle) -> u32 {
        let Some(value) = self.document.raw_text_for_layout(id) else {
            return 0;
        };
        let value = self.transform_text(id, &value);
        let metrics = self.document.text_metrics_for_layout(id);
        match style.white_space() {
            WhiteSpaceValue::Normal | WhiteSpaceValue::NoWrap => {
                let (value, _) = NativeDocument::collapse_text_for_layout(&value);
                self.measure_text_width(&metrics, style, &value)
            }
            WhiteSpaceValue::PreLine => value
                .split(['\n', '\r'])
                .map(|line| {
                    let (line, _) = NativeDocument::collapse_text_for_layout(line);
                    self.measure_text_width(&metrics, style, &line)
                })
                .max()
                .unwrap_or(0),
            WhiteSpaceValue::Pre | WhiteSpaceValue::PreWrap => value
                .split(['\n', '\r'])
                .map(|line| self.measure_text_width(&metrics, style, line))
                .max()
                .unwrap_or(0),
        }
    }

    fn measure_text_width(
        &mut self,
        metrics: &NativeTextMetrics,
        style: NativeComputedStyle,
        text: &str,
    ) -> u32 {
        let cacheable = text.len() <= MAX_LAYOUT_TEXT_WIDTH_CACHE_TEXT_BYTES
            && style.direction() == super::super::css::DirectionValue::Ltr
            && style.font_variant_ligatures() == NativeFontVariantLigatures::default()
            && style.font_variant_caps() == NativeFontVariantCaps::default()
            && style.font_variant_position() == NativeFontVariantPosition::default()
            && style.font_variant_alternates() == NativeFontVariantAlternates::default()
            && style.font_language_override() == Default::default()
            && style.font_variation_settings() == NativeFontVariationSettings::default()
            && style.font_variant_east_asian() == NativeFontVariantEastAsian::default()
            && style.font_variant_numeric() == NativeFontVariantNumeric::default()
            && style.font_feature_settings() == Default::default()
            && style.font_kerning() == NativeFontKerning::Auto
            && style.font_optical_sizing() == NativeFontOpticalSizing::Auto;
        if !cacheable {
            return metrics.measure_text(text, style.letter_spacing(), style.word_spacing());
        }
        let key = NativeTextWidthCacheKey {
            font_family: style.font_family(),
            font_size: style.font_size(),
            font_weight: style.font_weight(),
            font_style: style.font_style(),
            font_stretch: style.font_stretch().min,
            letter_spacing: style.letter_spacing(),
            word_spacing: style.word_spacing(),
        };
        if let Some(width) = self
            .text_width_cache
            .get(&key)
            .and_then(|widths| widths.get(text))
            .copied()
        {
            self.text_width_cache_hits = self.text_width_cache_hits.saturating_add(1);
            return width;
        }
        let width = metrics.measure_text(text, style.letter_spacing(), style.word_spacing());
        if self.text_width_cache_entries < MAX_LAYOUT_TEXT_WIDTH_CACHE_ENTRIES {
            self.text_width_cache
                .entry(key)
                .or_default()
                .insert(text.to_owned(), width);
            self.text_width_cache_entries += 1;
        }
        width
    }

    fn intrinsic_inline_height(&self, id: NativeNodeId) -> u32 {
        match self.document.node(id).and_then(|node| node.element_name()) {
            Some("input" | "textarea" | "select" | "button") => DEFAULT_CONTROL_HEIGHT,
            Some("canvas") => self.canvas_dimensions(id).1,
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
            Some("video") => self
                .video_poster_dimensions(id)
                .map(|(_, height)| height)
                .unwrap_or(DEFAULT_LINE_HEIGHT),
            _ => DEFAULT_LINE_HEIGHT,
        }
    }

    fn canvas_dimensions(&self, id: NativeNodeId) -> (u32, u32) {
        let Some(node) = self.document.node(id) else {
            return (300, 150);
        };
        let dimension = |name: &str, default: u32| {
            node.attribute(name)
                .and_then(|value| value.trim().parse::<u32>().ok())
                .filter(|value| *value > 0 && *value <= MAX_NATIVE_CANVAS_DIMENSION)
                .unwrap_or(default)
        };
        let width = dimension("width", 300);
        let height = dimension("height", 150);
        if usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .is_some_and(|pixels| pixels > super::super::dom::MAX_NATIVE_CANVAS_PIXELS)
        {
            (
                width,
                (super::super::dom::MAX_NATIVE_CANVAS_PIXELS / width as usize).max(1) as u32,
            )
        } else {
            (width, height)
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

    fn video_poster_dimensions(&self, id: NativeNodeId) -> Option<(u32, u32)> {
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
            .video_poster_resource_for_node(id)
            .map(|image| (image.width, image.height))
            .or_else(|| {
                node.attribute("poster")
                    .and_then(image_dimensions_from_source)
            });
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
        if matches!(name, "map" | "area") {
            return DisplayValue::None;
        }
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

#[cfg(test)]
mod text_width_cache_tests {
    use super::*;
    use crate::native_engine::NativeEngineLimits;

    fn builder(document: &NativeDocument) -> LayoutBuilder<'_> {
        let containing_block = super::super::PositionedContainingBlock {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        LayoutBuilder {
            document,
            boxes: Vec::new(),
            text_runs: Vec::new(),
            paint_order: Vec::new(),
            sticky_ranges: Vec::new(),
            initial_containing_block: containing_block,
            containing_block,
            fixed: false,
            sticky_root: None,
            stacking_context: 0,
            text_width_cache: HashMap::new(),
            text_width_cache_entries: 0,
            text_width_cache_hits: 0,
        }
    }

    fn element_with_id(document: &NativeDocument, id: &str) -> NativeNodeId {
        document
            .element_node_ids()
            .find(|node_id| {
                document
                    .node(*node_id)
                    .and_then(|node| node.attribute("id"))
                    == Some(id)
            })
            .expect("fixture element exists")
    }

    #[test]
    fn text_width_cache_reuses_only_identical_default_ltr_shaping_inputs() {
        let document = NativeDocument::parse(
            "<p id='first'>repeated label</p><p id='same'>repeated label</p><p id='spaced' style='letter-spacing:2px'>repeated label</p>",
            &NativeEngineLimits::default(),
        )
        .expect("fixture document parses");
        let first_id = element_with_id(&document, "first");
        let same_id = element_with_id(&document, "same");
        let spaced_id = element_with_id(&document, "spaced");
        let first_style = document.computed_style_for_layout(first_id);
        let same_style = document.computed_style_for_layout(same_id);
        let spaced_style = document.computed_style_for_layout(spaced_id);
        let first_metrics = document.text_metrics_for_layout(first_id);
        let spaced_metrics = document.text_metrics_for_layout(spaced_id);
        let mut builder = builder(&document);

        let expected = first_metrics.measure_text("repeated label", 0, 0);
        assert_eq!(
            builder.measure_text_width(&first_metrics, first_style, "repeated label"),
            expected
        );
        assert_eq!(
            builder.measure_text_width(&first_metrics, same_style, "repeated label"),
            expected
        );
        assert_eq!(builder.text_width_cache_hits, 1);
        assert_eq!(
            builder.measure_text_width(&spaced_metrics, spaced_style, "repeated label"),
            spaced_metrics.measure_text("repeated label", 2, 0)
        );
        assert_eq!(builder.text_width_cache_hits, 1);
        assert_eq!(builder.text_width_cache_entries, 2);
    }

    #[test]
    fn text_width_cache_stays_within_its_entry_budget() {
        let document =
            NativeDocument::parse("<p id='target'>x</p>", &NativeEngineLimits::default())
                .expect("fixture document parses");
        let target = element_with_id(&document, "target");
        let style = document.computed_style_for_layout(target);
        let metrics = document.text_metrics_for_layout(target);
        let mut builder = builder(&document);

        for index in 0..=MAX_LAYOUT_TEXT_WIDTH_CACHE_ENTRIES {
            let text = format!("label-{index}");
            builder.measure_text_width(&metrics, style, &text);
        }

        assert_eq!(
            builder.text_width_cache_entries,
            MAX_LAYOUT_TEXT_WIDTH_CACHE_ENTRIES
        );
    }

    #[test]
    fn repeated_inline_labels_reuse_cached_text_width_during_layout() {
        let mut html = String::from("<div>");
        for _ in 0..64 {
            html.push_str("<span>Common repeated navigation label</span>");
        }
        html.push_str("</div>");
        let document = NativeDocument::parse(&html, &NativeEngineLimits::default())
            .expect("repeated-label fixture parses");
        let mut builder = builder(&document);

        let flow = builder.layout_children(document.root(), 0, 0, 800, 0);

        assert!(flow.width > 0);
        assert!(builder.text_width_cache_hits >= 63);
    }
}

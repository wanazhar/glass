use super::*;

impl<'a> LayoutBuilder<'a> {
    pub(super) fn layout_flex_line(
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

    pub(super) fn layout_flex_column_children(
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
                NativeNodeKind::Document | NativeNodeKind::DocumentFragment => {
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

    pub(super) fn layout_flex_column_line(
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

    pub(super) fn layout_flex_column_wrap_children(
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
                NativeNodeKind::Document | NativeNodeKind::DocumentFragment => {
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

    pub(super) fn layout_flex_children(
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
                NativeNodeKind::Document | NativeNodeKind::DocumentFragment => {
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

    pub(super) fn outer_width(
        &mut self,
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

    pub(super) fn flex_item_base_width(
        &mut self,
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

    pub(super) fn flex_item_base_height(
        &self,
        id: NativeNodeId,
        style: NativeComputedStyle,
    ) -> u32 {
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
}

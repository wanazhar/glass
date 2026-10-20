use super::*;

impl<'a> LayoutBuilder<'a> {
    pub(super) fn layout_grid_children(
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

    pub(super) fn grid_column_sizes(
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

    pub(super) fn grid_row_sizes(
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

    pub(super) fn grid_justify_offsets(
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

    pub(super) fn grid_align_offsets(
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
}

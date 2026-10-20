use super::*;

impl<'a> LayoutBuilder<'a> {
    pub(super) fn place_text(&mut self, parent: NativeNodeId, flow: &mut FlowCursor, value: &str) {
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

    pub(super) fn transform_text(&self, parent: NativeNodeId, value: &str) -> String {
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

    pub(super) fn place_pre_line_text(
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

    pub(super) fn place_preformatted_text(
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

    pub(super) fn place_preformatted_segment(
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
        let line_height = flow.text_line_height();
        let Some(origin) = flow.place_unwrapped_with_origin(width, line_height) else {
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
            height: line_height,
            vertical_align: VerticalAlignValue::Baseline,
        });
    }

    pub(super) fn place_preformatted_wrapped_segment(
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

    pub(super) fn place_text_segment(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        value: &str,
    ) {
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

    pub(super) fn should_apply_ellipsis(&self, parent: NativeNodeId) -> bool {
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

    pub(super) fn place_ellipsis_text(
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

    pub(super) fn text_prefix_for_width(
        &self,
        text: &str,
        available_width: u32,
        flow: &FlowCursor,
    ) -> String {
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

    pub(super) fn place_break_all_text(
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

    pub(super) fn place_break_all_word(
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

    pub(super) fn place_pending_separator(&mut self, parent: NativeNodeId, flow: &mut FlowCursor) {
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

    pub(super) fn place_word(
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

    pub(super) fn place_text_fragment(
        &mut self,
        parent: NativeNodeId,
        flow: &mut FlowCursor,
        fragment: &str,
        truncated: bool,
    ) {
        let width = flow.text_width(fragment);
        let line_height = flow.text_line_height();
        let Some(origin) = flow.place_inline_with_origin(width, line_height) else {
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
            height: line_height,
            vertical_align: VerticalAlignValue::Baseline,
        });
    }

    pub(super) fn character_advance(
        character: char,
        letter_spacing: u32,
        word_spacing: u32,
    ) -> u32 {
        CHARACTER_WIDTH
            .saturating_add(letter_spacing)
            .saturating_add(if character == ' ' { word_spacing } else { 0 })
    }

    pub(in crate::native_engine::layout) fn text_width_with_justification(
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
}

//! Glass Core conversation presentation shared with the standalone Core TUI.
//!
//! Glass keeps its own project navigation, session identity, draft storage,
//! and event protocol. This adapter translates only conversation entries and
//! assistant Markdown into the reusable `glass-tui` presentation types.

use super::projection::{ConversationEntry, ConversationKind};

pub(super) fn edit_draft(
    text: &str,
    cursor: usize,
    key: glass_tui::crossterm::event::KeyEvent,
) -> (glass_tui::Edit, String, usize) {
    let mut editor = glass_tui::Editor::new();
    editor.set_text(text);
    editor.set_cursor(cursor);
    let outcome = editor.key(key);
    (outcome, editor.text().to_string(), editor.cursor())
}

/// Retained Core wrapping cache used underneath Glass's typed bubble framing.
pub(super) struct InlineLayout {
    layout: glass_tui::layout::Layout,
    source_key: Option<InlineLayoutKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InlineLayoutKey {
    pub(super) event_cursor: u64,
    pub(super) selected_agent: Option<String>,
    pub(super) entry_count: usize,
    pub(super) pending_signature: Vec<(String, super::state::ChatMessageState, Option<String>)>,
}

impl Default for InlineLayout {
    fn default() -> Self {
        Self {
            layout: glass_tui::layout::Layout::new(Vec::new()),
            source_key: None,
        }
    }
}

impl InlineLayout {
    pub(super) fn set_entries(&mut self, entries: &[ConversationEntry]) {
        self.layout.set_entries(
            entries
                .iter()
                .enumerate()
                .map(|(turn, entry)| {
                    let shared = transcript_entry(entry, turn);
                    (shared.text, shared.role, shared.turn)
                })
                .collect(),
        );
    }

    pub(super) fn set_entries_if_changed(
        &mut self,
        entries: &[ConversationEntry],
        key: InlineLayoutKey,
    ) {
        if self.source_key.as_ref() == Some(&key) {
            return;
        }
        self.set_entries(entries);
        self.source_key = Some(key);
    }

    pub(super) fn assistant_lines(
        &mut self,
        entry: usize,
        width: usize,
    ) -> Vec<glass_tui::ratatui::text::Line<'static>> {
        let width = width.clamp(1, u16::MAX as usize) as u16;
        let margin = if width >= 24 {
            2
        } else if width >= 10 {
            1
        } else {
            0
        };
        let prefix = " ".repeat(margin);
        self.layout.set_width(width);
        self.layout
            .entry_lines(entry)
            .into_iter()
            .map(|line| strip_line_prefix(line, &prefix))
            .collect()
    }
}

fn strip_line_prefix(
    line: glass_tui::ratatui::text::Line<'static>,
    prefix: &str,
) -> glass_tui::ratatui::text::Line<'static> {
    use glass_tui::ratatui::text::{Line, Span};

    let mut remaining = prefix.chars().count();
    if remaining == 0 {
        return line;
    }
    let mut spans = Vec::with_capacity(line.spans.len());
    let mut source = line.spans.into_iter();
    for span in source.by_ref() {
        let content = span.content.as_ref();
        let mut split = 0;
        for (offset, character) in content.char_indices() {
            if remaining == 0 {
                break;
            }
            if character != ' ' {
                spans.push(Span::styled(content.to_string(), span.style));
                spans
                    .extend(source.map(|rest| Span::styled(rest.content.into_owned(), rest.style)));
                return Line::from(spans);
            }
            remaining -= 1;
            split = offset + character.len_utf8();
        }
        if split < content.len() {
            spans.push(Span::styled(content[split..].to_string(), span.style));
        }
        if remaining == 0 {
            // Keep all following spans as-is once the generated margin is gone.
            spans.extend(source.map(|rest| Span::styled(rest.content.into_owned(), rest.style)));
            break;
        }
    }
    Line::from(spans)
}

pub(super) fn transcript_entry(entry: &ConversationEntry, turn: usize) -> glass_tui::Entry {
    let role = match entry.kind {
        ConversationKind::User => glass_tui::Role::User,
        ConversationKind::Assistant => glass_tui::Role::Assistant,
        ConversationKind::System | ConversationKind::Alert | ConversationKind::Error => {
            glass_tui::Role::System
        }
    };
    glass_tui::Entry {
        role,
        text: entry.text.clone(),
        turn,
        tool_card: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_glass_identity_into_shared_transcript_entries() {
        let entry = ConversationEntry {
            kind: ConversationKind::Assistant,
            text: "answer".into(),
            streaming: true,
            entry_id: Some("core-entry".into()),
            client_message_id: Some("glass-message".into()),
            tool_name: None,
        };
        let shared = transcript_entry(&entry, 7);
        assert_eq!(shared.role, glass_tui::Role::Assistant);
        assert_eq!(shared.text, "answer");
        assert_eq!(shared.turn, 7);
        assert!(shared.tool_card.is_none());
        assert_eq!(entry.entry_id.as_deref(), Some("core-entry"));
        assert_eq!(entry.client_message_id.as_deref(), Some("glass-message"));
    }

    #[test]
    fn assistant_markdown_keeps_paragraph_boundaries() {
        let mut layout = InlineLayout::default();
        let entries = [ConversationEntry {
            kind: ConversationKind::Assistant,
            text: "First paragraph.\n\nSecond **paragraph**.".into(),
            streaming: false,
            entry_id: None,
            client_message_id: None,
            tool_name: None,
        }];
        layout.set_entries(&entries);
        let lines = layout.assistant_lines(0, 60);
        assert!(lines.iter().any(|line| line.width() == 0));
        assert!(
            lines
                .iter()
                .any(|line| line.to_string().contains("Second paragraph."))
        );
        assert!(lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("paragraph")
                && span
                    .style
                    .add_modifier
                    .contains(glass_tui::ratatui::style::Modifier::BOLD)
        }));
        assert_eq!(layout.assistant_lines(0, 60), lines);
    }

    #[test]
    fn retained_layout_rebuilds_only_when_its_content_revision_changes() {
        let mut layout = InlineLayout::default();
        let key = InlineLayoutKey {
            event_cursor: 1,
            selected_agent: Some("agent-a".into()),
            entry_count: 1,
            pending_signature: Vec::new(),
        };
        let first = [ConversationEntry {
            kind: ConversationKind::Assistant,
            text: "old response".into(),
            streaming: false,
            entry_id: None,
            client_message_id: None,
            tool_name: None,
        }];
        layout.set_entries_if_changed(&first, key.clone());
        let changed_without_revision = [ConversationEntry {
            text: "new response".into(),
            ..first[0].clone()
        }];
        layout.set_entries_if_changed(&changed_without_revision, key.clone());
        assert!(
            layout.assistant_lines(0, 60)[0]
                .to_string()
                .contains("old response")
        );

        layout.set_entries_if_changed(
            &changed_without_revision,
            InlineLayoutKey {
                event_cursor: 2,
                ..key
            },
        );
        assert!(
            layout.assistant_lines(0, 60)[0]
                .to_string()
                .contains("new response")
        );
    }

    #[test]
    fn shared_editor_applies_edits_at_the_glass_persisted_cursor() {
        use glass_tui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let (outcome, text, cursor) = edit_draft(
            "abc",
            1,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        assert_eq!(outcome, glass_tui::Edit::Continue);
        assert_eq!(text, "axbc");
        assert_eq!(cursor, 2);
    }
}

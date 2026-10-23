//! Glass Core conversation presentation shared with the standalone Core TUI.
//!
//! Glass keeps its own project navigation, session identity, draft storage,
//! and event protocol. This adapter translates only conversation entries and
//! typed messages and tool cards into the reusable `glass-tui` transcript.

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

/// Glass adapter around Core's retained transcript viewport/controller.
pub(super) struct InlineTranscript {
    transcript: glass_tui::Transcript,
    source_key: Option<InlineTranscriptKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InlineTranscriptKey {
    pub(super) event_cursor: u64,
    pub(super) selected_agent: Option<String>,
    pub(super) entry_count: usize,
    pub(super) pending_signature: Vec<(String, super::state::ChatMessageState, Option<String>)>,
    pub(super) expanded_tools: bool,
}

impl Default for InlineTranscript {
    fn default() -> Self {
        let mut transcript = glass_tui::Transcript::new(Vec::new());
        transcript.set_glass_presentation(true);
        Self {
            transcript,
            source_key: None,
        }
    }
}

impl InlineTranscript {
    pub(super) fn set_entries_if_changed(
        &mut self,
        entries: &[ConversationEntry],
        key: InlineTranscriptKey,
        expanded_tools: bool,
    ) {
        if self.source_key.as_ref() == Some(&key) {
            return;
        }
        self.transcript.replace_entries(
            entries
                .iter()
                .enumerate()
                .map(|(turn, entry)| transcript_entry(entry, turn, expanded_tools))
                .collect(),
        );
        self.source_key = Some(key);
    }

    pub(super) fn visible_lines(
        &mut self,
        width: usize,
        height: usize,
        selected_entry: usize,
    ) -> Vec<glass_tui::ratatui::text::Line<'static>> {
        let width = width.clamp(1, u16::MAX as usize) as u16;
        let viewport = glass_tui::ratatui::layout::Rect::new(
            0,
            0,
            width,
            height.clamp(1, u16::MAX as usize) as u16,
        );
        self.transcript
            .visible_lines(viewport)
            .into_iter()
            .map(|visible| {
                if visible.entry_index != selected_entry {
                    return tint_tool_identity(visible.line);
                }
                let spans = visible
                    .line
                    .spans
                    .into_iter()
                    .map(|span| {
                        glass_tui::ratatui::text::Span::styled(
                            span.content.into_owned(),
                            span.style
                                .add_modifier(glass_tui::ratatui::style::Modifier::REVERSED),
                        )
                    })
                    .collect::<Vec<_>>();
                tint_tool_identity(
                    glass_tui::ratatui::text::Line::from(spans).style(visible.line.style),
                )
            })
            .collect()
    }

    pub(super) fn step(&mut self, delta: isize, height: u16) {
        self.transcript.step(delta, height);
    }

    pub(super) fn top(&mut self) {
        self.transcript.top();
    }

    pub(super) fn follow_tail(&mut self, height: u16) {
        self.transcript.follow_tail(height);
    }

    pub(super) fn focus_entry(&mut self, entry: usize, height: u16) -> bool {
        self.transcript.focus_entry(entry, height)
    }

    pub(super) fn scroll(&self) -> usize {
        self.transcript.scroll
    }

    pub(super) fn is_following(&self) -> bool {
        self.transcript.follow
    }

    pub(super) fn total_lines(&self) -> usize {
        self.transcript.total_lines()
    }

    pub(super) fn set_width(&mut self, width: u16) {
        self.transcript.set_width(width.max(1));
    }
}

fn tint_tool_identity(
    mut line: glass_tui::ratatui::text::Line<'static>,
) -> glass_tui::ratatui::text::Line<'static> {
    use glass_tui::ratatui::style::Color;
    let text = line.to_string();
    let color = if text.contains("CORE ·") {
        Some(Color::Rgb(180, 145, 230))
    } else if text.contains("GLASS ·") {
        Some(Color::Rgb(79, 214, 195))
    } else {
        None
    };
    if let Some(color) = color
        && let Some(span) = line.spans.first_mut()
    {
        span.style = span.style.fg(color);
    }
    line
}

pub(super) fn transcript_entry(
    entry: &ConversationEntry,
    turn: usize,
    expanded_tools: bool,
) -> glass_tui::Entry {
    let mut text = entry.text.clone();
    if entry.kind == ConversationKind::Assistant && entry.streaming {
        if text.is_empty() {
            text = "Thinking…".into();
        } else {
            text.push_str("\n… streaming");
        }
    }
    match entry.kind {
        ConversationKind::User => text = format!("YOU\n{text}"),
        ConversationKind::Assistant => text = format!("GLASS AGENT\n{text}"),
        ConversationKind::System => text = format!("SYSTEM\n{text}"),
        ConversationKind::Alert | ConversationKind::Error => {}
    }
    let role = if entry.tool_card.is_some() {
        if entry
            .tool_card
            .as_ref()
            .is_some_and(|card| card.command == "bash")
        {
            glass_tui::Role::Bash
        } else {
            glass_tui::Role::Tool
        }
    } else {
        match entry.kind {
            ConversationKind::User => glass_tui::Role::User,
            ConversationKind::Assistant => glass_tui::Role::Assistant,
            ConversationKind::System | ConversationKind::Alert | ConversationKind::Error => {
                glass_tui::Role::System
            }
        }
    };
    if entry.kind == ConversationKind::Alert {
        text = format!("⚠ ALERT\n{text}");
    } else if entry.kind == ConversationKind::Error && entry.tool_card.is_none() {
        text = format!("ERROR\n× {text}");
    }
    let tool_card = entry.tool_card.as_ref().map(|card| {
        let mut card = card.clone();
        card.expanded = expanded_tools;
        card
    });
    glass_tui::Entry {
        role,
        text,
        turn,
        tool_card,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: ConversationKind, text: &str) -> ConversationEntry {
        ConversationEntry {
            kind,
            text: text.into(),
            streaming: false,
            entry_id: None,
            client_message_id: None,
            tool_name: None,
            tool_card: None,
        }
    }

    fn key(event_cursor: u64, entry_count: usize) -> InlineTranscriptKey {
        InlineTranscriptKey {
            event_cursor,
            selected_agent: Some("agent-a".into()),
            entry_count,
            pending_signature: Vec::new(),
            expanded_tools: false,
        }
    }

    #[test]
    fn maps_glass_identity_into_shared_transcript_entries() {
        let entry = ConversationEntry {
            kind: ConversationKind::Assistant,
            text: "answer".into(),
            streaming: true,
            entry_id: Some("core-entry".into()),
            client_message_id: Some("glass-message".into()),
            tool_name: None,
            tool_card: None,
        };
        let shared = transcript_entry(&entry, 7, false);
        assert_eq!(shared.role, glass_tui::Role::Assistant);
        assert_eq!(shared.text, "GLASS AGENT\nanswer\n… streaming");
        assert_eq!(shared.turn, 7);
        assert!(shared.tool_card.is_none());
        assert_eq!(entry.entry_id.as_deref(), Some("core-entry"));
        assert_eq!(entry.client_message_id.as_deref(), Some("glass-message"));
    }

    #[test]
    fn maps_call_identity_and_status_into_the_shared_tool_card() {
        let mut row = entry(ConversationKind::System, "read file\nfirst output");
        row.tool_name = Some("glass.file.read".into());
        row.tool_card = Some(glass_tui::ToolCard {
            expanded: false,
            call_id: "call-stable-17".into(),
            command: "glass.file.read src/main.rs".into(),
            output: "first output".into(),
            status: "completed".into(),
            elapsed_ms: Some(37),
            spill: None,
            truncated: false,
        });

        let shared = transcript_entry(&row, 4, false);
        assert_eq!(shared.role, glass_tui::Role::Tool);
        let card = shared.tool_card.expect("shared tool card");
        assert_eq!(card.call_id, "call-stable-17");
        assert_eq!(card.status, "completed");
        assert_eq!(card.elapsed_ms, Some(37));
    }

    #[test]
    fn glass_embed_selects_compact_cards_without_changing_core_default() {
        let mut row = entry(ConversationKind::System, "");
        row.tool_name = Some("bash".into());
        row.tool_card = Some(glass_tui::ToolCard {
            expanded: false,
            call_id: "call-glass-1".into(),
            command: "bash".into(),
            output: String::new(),
            status: "completed".into(),
            elapsed_ms: None,
            spill: None,
            truncated: false,
        });
        let mut embedded = InlineTranscript::default();
        embedded.set_entries_if_changed(&[row], key(1, 1), false);
        let rendered = embedded
            .visible_lines(48, 8, usize::MAX)
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("CORE · bash · completed"));
        assert!(!rendered.contains("$ bash"));
        assert!(!rendered.contains("0.0s"));
        assert!(!rendered.contains("No output"));

        let mut core = glass_tui::Transcript::new(vec![glass_tui::Entry {
            role: glass_tui::Role::Bash,
            text: String::new(),
            turn: 0,
            tool_card: Some(glass_tui::ToolCard {
                expanded: false,
                call_id: "call-core-1".into(),
                command: "bash".into(),
                output: String::new(),
                status: "completed".into(),
                elapsed_ms: None,
                spill: None,
                truncated: false,
            }),
        }]);
        let core_lines = core.visible_lines(glass_tui::ratatui::layout::Rect::new(0, 0, 48, 8));
        let core_rendered = core_lines
            .iter()
            .map(|line| line.line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(core_rendered.contains("$ bash"));
        assert!(core_rendered.contains("0.0s"));
        assert!(core_rendered.contains("No output"));
    }

    #[test]
    fn shared_transcript_only_returns_the_bounded_viewport_and_preserves_anchor() {
        let mut transcript = InlineTranscript::default();
        let mut entries = (0..100)
            .map(|index| {
                entry(
                    if index % 2 == 0 {
                        ConversationKind::User
                    } else {
                        ConversationKind::Assistant
                    },
                    &format!("message {index} with enough detail to wrap over several lines"),
                )
            })
            .collect::<Vec<_>>();
        transcript.set_entries_if_changed(&entries, key(1, entries.len()), false);
        assert!(transcript.focus_entry(20, 8));
        let selected = transcript.visible_lines(32, 8, 20);
        assert!(selected.iter().any(|line| {
            line.to_string().contains("message 20")
                && line.spans.iter().any(|span| {
                    span.style
                        .add_modifier
                        .contains(glass_tui::ratatui::style::Modifier::REVERSED)
                })
        }));

        transcript.top();
        transcript.step(8, 8);
        let before = transcript.scroll();
        let visible = transcript.visible_lines(32, 8, usize::MAX);
        assert!(visible.len() <= 8);
        assert_eq!(transcript.scroll(), before);

        entries.push(entry(ConversationKind::User, "new tail"));
        transcript.set_entries_if_changed(&entries, key(2, entries.len()), false);
        let _ = transcript.visible_lines(32, 8, usize::MAX);
        assert_eq!(transcript.scroll(), before);
    }

    #[test]
    fn typed_alert_and_error_rows_keep_their_glass_labels() {
        let alert = transcript_entry(
            &entry(ConversationKind::Alert, "approval required"),
            0,
            false,
        );
        let error = transcript_entry(&entry(ConversationKind::Error, "tool failed"), 1, false);
        assert!(alert.text.starts_with("⚠ ALERT\napproval required"));
        assert!(error.text.starts_with("ERROR\n× tool failed"));
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

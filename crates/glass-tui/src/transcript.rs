//! Transcript renderer shared by the fullscreen viewer and inline reader.
//!
//! The viewer never owns history. It renders through [`super::layout::Layout`],
//! which wraps each entry once and hands back only the lines the viewport
//! needs, so opening the viewer on a long session cannot stall the frame.

use glass_mod_api::{Mod_, Overlay};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout as RLayout, Rect},
    style::Style,
    widgets::{Block, Borders, List, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use super::layout::Layout;
use crate::Role;

/// Viewer state: scroll position, collapse flags, and the line layout.
pub struct Transcript {
    pub layout: Layout,
    /// Current scroll offset, in wrapped lines.
    pub scroll: usize,
    pub follow: bool,
    collapsed: Vec<bool>,
    original: Vec<(String, Role, usize)>,
    inline_hint: Option<String>,
    anchor: Option<Anchor>,
    matches: Vec<usize>,
    match_index: Option<usize>,
}

#[derive(Clone)]
struct Anchor {
    text: String,
    role: Role,
    turn: usize,
    occurrence: usize,
    line: usize,
}

impl Transcript {
    pub fn new(entries: Vec<super::Entry>) -> Self {
        let cards = entries.iter().enumerate().filter_map(|(i,e)| e.tool_card.clone().map(|c| (i,c))).collect();
        let rows: Vec<(String, Role, usize)> = entries
            .into_iter()
            .map(|e| (e.text, e.role, e.turn))
            .collect();
        let collapsed = vec![false; rows.len()];
        let mut layout = Layout::new(rows.clone());
        layout.set_cards(cards);
        layout.set_width(80);
        let scroll = layout.total_lines().saturating_sub(1);
        Self {
            layout,
            scroll,
            follow: true,
            collapsed,
            original: rows,
            inline_hint: None,
            anchor: None,
            matches: Vec::new(),
            match_index: None,
        }
    }

    /// Refresh live records without resetting the reader's scroll position.
    pub fn replace_entries(&mut self, entries: Vec<super::Entry>) -> bool {
        let cards = entries.iter().enumerate().filter_map(|(i,e)| e.tool_card.clone().map(|c| (i,c))).collect();
        let cards_changed = self.layout.set_cards(cards);
        let rows: Vec<_> = entries
            .into_iter()
            .map(|e| (e.text, e.role, e.turn))
            .collect();
        if self.original == rows {
            return cards_changed;
        }
        let anchor = self.capture_anchor();
        self.collapsed.resize(rows.len(), false);
        self.original = rows;
        self.rebuild();
        self.restore_anchor(anchor);
        if !self.matches.is_empty() {
            self.refresh_search_matches();
        }
        true
    }

    pub fn len(&self) -> usize {
        self.layout.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layout.is_empty()
    }

    /// Replace the fullscreen help footer with a compact pinned-reader hint.
    pub fn set_inline_hint(&mut self, hint: Option<String>) {
        self.inline_hint = hint;
    }

    /// Set the wrapping width before applying an initial page offset.
    pub fn set_width(&mut self, width: u16) {
        let anchor = self.capture_anchor();
        self.layout.set_width(width.max(1));
        self.restore_anchor(anchor);
    }

    /// Clamp `scroll` so the viewport can always be filled.
    fn clamp(&mut self, height: u16) {
        let max = self
            .layout
            .total_lines()
            .saturating_sub(height.max(1) as usize);
        self.scroll = self.scroll.min(max);
    }

    /// Scroll by whole lines.
    pub fn step(&mut self, delta: isize, height: u16) {
        self.scroll = (self.scroll as isize + delta).max(0) as usize;
        self.clamp(height);
        self.follow = delta >= 0
            && self.scroll
                == self
                    .layout
                    .total_lines()
                    .saturating_sub(height.max(1) as usize);
    }

    /// Scroll by half a viewport for the fullscreen transcript.
    pub fn page(&mut self, delta: isize, height: u16) {
        let step = (height as isize / 2).max(1);
        self.step(delta * step, height);
    }

    /// Jump to the first line of a turn.
    pub fn jump_to_turn(&mut self, turn: usize, height: u16) {
        if let Some(line) = self.layout.line_for_turn(turn) {
            self.scroll = line;
            self.follow = false;
            self.clamp(height);
        }
    }

    pub fn top(&mut self) {
        self.scroll = 0;
        self.follow = false;
    }

    /// Search complete raw entry strings, including command and tool output.
    /// Match positions are entry anchors, so jump targets remain valid on reflow.
    pub fn search(&mut self, query: &str) -> usize {
        self.matches.clear();
        self.match_index = None;
        if query.is_empty() {
            return 0;
        }
        let needle = query.to_lowercase();
        for i in 0..self.layout.len() {
            if let Some(text) = self.layout.entry_text(i) {
                let lower = text.to_lowercase();
                if lower.contains(&needle) {
                    let role = self.original.get(i).map(|row| row.1).unwrap_or(Role::Meta);
                    let line = display_line_for_match(text, role, query, self.layout.width());
                    let boundary = usize::from(i > 0);
                    self.matches
                        .push(self.layout.line_for_entry(i).unwrap_or(0) + boundary + line);
                }
            }
        }
        self.matches.len()
    }

    pub fn search_next(&mut self, reverse: bool, height: u16) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        let next = match self.match_index {
            None => {
                if reverse {
                    self.matches.len() - 1
                } else {
                    0
                }
            }
            Some(i) if reverse => (i + self.matches.len() - 1) % self.matches.len(),
            Some(i) => (i + 1) % self.matches.len(),
        };
        self.match_index = Some(next);
        self.scroll = self.matches[next];
        self.follow = false;
        self.clamp(height);
        self.anchor = self.capture_anchor();
        true
    }

    fn refresh_search_matches(&mut self) {
        // The caller only invokes this when a search was already active.
        // The query itself is held by the reader; matching is refreshed there.
        self.matches.retain(|i| *i < self.layout.len());
    }

    fn capture_anchor(&self) -> Option<Anchor> {
        if self.layout.is_empty() {
            return None;
        }
        let entry = self.layout.entry_at_line(self.scroll);
        let (text, role, turn) = self.original.get(entry)?;
        let occurrence = self.original[..entry]
            .iter()
            .filter(|row| row.0 == *text && row.1 == *role && row.2 == *turn)
            .count();
        let start = self.layout.line_for_entry(entry).unwrap_or(0);
        Some(Anchor {
            text: text.clone(),
            role: *role,
            turn: *turn,
            occurrence,
            line: self.scroll.saturating_sub(start),
        })
    }

    fn restore_anchor(&mut self, anchor: Option<Anchor>) {
        let Some(anchor) = anchor else {
            self.clamp(1);
            return;
        };
        let mut seen = 0;
        let found = self.original.iter().enumerate().find_map(|(i, row)| {
            if row.0 == anchor.text && row.1 == anchor.role && row.2 == anchor.turn {
                let current = seen;
                seen += 1;
                (current == anchor.occurrence).then_some(i)
            } else {
                None
            }
        });
        if let Some(entry) = found {
            self.scroll = self
                .layout
                .line_for_entry(entry)
                .unwrap_or(0)
                .saturating_add(anchor.line);
            self.clamp(1);
            self.anchor = self.capture_anchor();
        } else {
            self.clamp(1);
        }
    }

    /// Pin to the newest content.
    pub fn follow_tail(&mut self, height: u16) {
        self.follow = true;
        self.scroll = self
            .layout
            .total_lines()
            .saturating_sub(height.max(1) as usize);
        self.clamp(height);
    }

    /// Toggle collapse on the entry at the top of the viewport.
    pub fn toggle_collapse(&mut self) {
        let idx = self.layout.entry_at_line(self.scroll);
        let Some(slot) = self.collapsed.get_mut(idx) else {
            return;
        };
        *slot = !*slot;
        self.rebuild();
    }

    /// Rebuild the line index after a collapse change, preserving scroll.
    fn rebuild(&mut self) {
        let keep = self.scroll;
        let width = self.layout.width();
        let rows = self
            .original
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, (text, role, turn))| {
                if self.collapsed.get(i).copied().unwrap_or(false) {
                    (
                        format!(
                            "▸ {} · {} lines",
                            crate::text::clip(text.lines().next().unwrap_or(""), 70),
                            text.lines().count()
                        ),
                        Role::Meta,
                        turn,
                    )
                } else {
                    (text, role, turn)
                }
            })
            .collect();
        let cards = self.layout.cards();
        self.layout = Layout::new(rows);
        self.layout.set_cards(cards);
        self.layout.set_width(width);
        self.scroll = keep.min(self.layout.total_lines().saturating_sub(1));
    }

    /// Actual text cells: one header row, one controls row, and a scrollbar column.
    pub fn content_rect(area: Rect) -> Rect {
        Rect::new(
            area.x,
            area.y.saturating_add(1).min(area.bottom()),
            area.width.saturating_sub(1),
            area.height.saturating_sub(2),
        )
    }

    /// Draw one frame. Only `height` lines are ever constructed.
    pub fn draw(&mut self, frame: &mut Frame, mods: &[Box<dyn Mod_>], session_label: &str) {
        let area = frame.area();
        let chunks = RLayout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let view = chunks[0];
        let content = Self::content_rect(area);
        if self.layout.width() != content.width {
            let anchor = self.capture_anchor();
            self.layout.set_width(content.width.max(1));
            self.restore_anchor(anchor);
        }
        if self.follow {
            self.follow_tail(content.height);
        }
        self.clamp(content.height);
        let items = self.layout.visible(self.scroll, content);

        let session_label = if area.width < 40 {
            "Log"
        } else {
            session_label
        };
        let title = format!(
            " {} — {} ",
            session_label,
            if self.follow {
                "following".to_string()
            } else {
                format!(
                    "line {}/{}",
                    self.scroll + 1,
                    self.layout.total_lines().max(1)
                )
            }
        );

        frame.render_widget(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::new().fg(ratatui::style::Color::DarkGray))
                .title(title),
            view,
        );
        frame.render_widget(List::new(items), content);

        let total = self.layout.total_lines();
        if total > 0 {
            // Ratatui expects the number of scroll positions, not total
            // wrapped lines. The last legal offset is total - viewport.
            let viewport = usize::from(content.height);
            let mut sb = ScrollbarState::new(total.saturating_sub(viewport) + 1)
                .viewport_content_length(viewport)
                .position(self.scroll);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                view,
                &mut sb,
            );
        }

        let controls = if let Some(hint) = &self.inline_hint {
            hint.as_str()
        } else if area.width < 40 {
            "↑↓ scroll · Esc close"
        } else {
            "↑↓ scroll · PgUp/PgDn page · Enter collapse · f follow · Esc close"
        };
        frame.render_widget(
            Paragraph::new(crate::text::clip(controls, area.width as usize)),
            chunks[1],
        );

        // Mods paint last so an overlay always wins over harness chrome.
        let mut overlay = Overlay::default();
        for m in mods {
            m.on_render(&mut overlay, (view.width, view.height));
        }
        if !overlay.lines.is_empty() {
            let h = (overlay.lines.len() as u16).min(chunks[1].height);
            let rect = Rect {
                x: view.x,
                y: chunks[1].y,
                width: view.width,
                height: h,
            };
            frame.render_widget(Paragraph::new(overlay.lines.join("\n")), rect);
        }
    }
}

fn display_line_for_match(text: &str, role: Role, query: &str, width: u16) -> usize {
    if role == Role::Assistant {
        let query=query.to_lowercase();
        return crate::markdown::lines(text,usize::from(width)).iter()
            .position(|row|row.to_string().to_lowercase().contains(&query)).unwrap_or(0);
    }
    let mut preceding = 0;
    let lines: Vec<_> = text.lines().collect();
    for (line_index, raw) in lines.iter().enumerate() {
        let content_width = if line_index == 0 && role != Role::Assistant && role != Role::User {
            usize::from(width).saturating_sub(crate::text::width(&crate::text::clip(
                &format!(" {} ", role.label()),
                usize::from(width.saturating_sub(1)),
            )))
        } else if line_index == 0 && role == Role::User {
            usize::from(width).saturating_sub(2)
        } else {
            usize::from(width)
        }
        .max(1);
        let wrapped = wrap_width(raw, content_width);
        let query = query.to_lowercase();
        let lowered = raw.to_lowercase();
        if let Some(byte) = lowered.find(&query) {
            let char_offset = lowered[..byte].chars().count();
            let mut consumed = 0;
            for (wrapped_index, part) in wrapped.iter().enumerate() {
                let part_len = part.chars().count();
                if char_offset < consumed + part_len {
                    return preceding + wrapped_index;
                }
                consumed += part_len + 1;
            }
            return preceding + wrapped.len().saturating_sub(1);
        }
        preceding += wrapped.len().max(1);
    }
    0
}

/// Wrap text to `width` columns on whitespace where possible, hard-splitting
/// only for words that cannot fit.
///
/// Respect the actual viewport, including narrow SSH terminals.
pub fn wrap_width(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for para in text.lines() {
        if para.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in para.split_whitespace() {
            if line.is_empty() {
                line.push_str(word);
            } else if crate::text::width(&line) + 1 + crate::text::width(word) <= width {
                line.push(' ');
                line.push_str(word);
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            }
            // Hard-split a token that cannot fit on any line: a long path, a
            // base64 blob, a minified bundle.
            while crate::text::width(&line) > width {
                let split = crate::text::clip(&line, width);
                if split.is_empty() {
                    line = crate::text::wrap(&line, width).join("");
                    break;
                }
                let rest = line[split.len()..].to_string();
                out.push(split);
                line = rest;
            }
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_is_one_line_on_a_narrow_screen_and_restores_original() {
        let mut viewer = Transcript::new(vec![crate::Entry {
            role: Role::Tool,
            text: "many tool output lines\n".repeat(20),
            turn: 1,
            tool_card: None,
        }]);
        viewer.layout.set_width(20);
        let original = viewer.layout.total_lines();
        viewer.top();
        viewer.toggle_collapse();
        assert_eq!(viewer.layout.total_lines(), 1);
        viewer.toggle_collapse();
        assert_eq!(viewer.layout.width(), 20);
        assert_eq!(viewer.layout.total_lines(), original);
    }
    use crate::Entry;

    fn entries(n: usize) -> Vec<Entry> {
        (0..n)
            .map(|i| Entry {
                role: Role::User,
                text: format!("message {i}"),
                turn: i,
                tool_card: None,
            })
            .collect()
    }

    #[test]
    fn rendered_tail_and_scroll_use_the_same_text_rectangle() {
        use ratatui::{Terminal, backend::TestBackend};
        for (width, height) in [(40, 19), (19, 3)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut rows = entries(100);
            rows.last_mut().unwrap().text = "TAIL".into();
            let mut transcript = Transcript::new(rows);
            let content = Transcript::content_rect(Rect::new(0, 0, width, height));
            transcript.set_width(content.width);
            transcript.top();
            transcript.step(isize::MAX, content.height);
            assert!(transcript.follow);
            terminal.draw(|frame| transcript.draw(frame, &[], "Log")).unwrap();
            let buffer = terminal.backend().buffer();
            let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
            assert!(text.contains("TAIL"), "{width}x{height}: {text}");
            assert_eq!(transcript.scroll,
                transcript.layout.total_lines().saturating_sub(content.height as usize));
        }
    }

    #[test]
    fn scrollbar_thumb_reaches_both_ends_for_each_viewport() {
        use ratatui::{Terminal, backend::TestBackend};
        for (width, height) in [(80, 24), (40, 19), (20, 8), (19, 4)] {
            for count in [1, 15, 100] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                let mut transcript = Transcript::new(entries(count));
                transcript.top();
                terminal.draw(|frame| transcript.draw(frame, &[], "Log")).unwrap();
                assert_eq!(terminal.backend().buffer()[(width - 1, 1)].symbol(), "█",
                    "top: {width}x{height}, {count} entries");
                transcript.follow = true;
                terminal.draw(|frame| transcript.draw(frame, &[], "Log")).unwrap();
                // Last track cell is above the down arrow and controls row.
                assert_eq!(terminal.backend().buffer()[(width - 1, height - 3)].symbol(), "█",
                    "bottom: {width}x{height}, {count} entries");
            }
        }
    }

    const H: u16 = 20;

    #[test]
    fn wraps_on_whitespace() {
        let out = wrap_width("the quick brown fox jumps over", 20);
        assert!(out.iter().all(|l| l.chars().count() <= 20), "{out:?}");
        assert_eq!(out, vec!["the quick brown fox", "jumps over"]);
    }

    #[test]
    fn narrow_wrap_never_loses_text_to_clipping() {
        let out = wrap_width("alpha beta gamma delta", 5);
        assert_eq!(out, vec!["alpha", "beta", "gamma", "delta"]);
    }

    #[test]
    fn hard_splits_overlong_token() {
        let out = wrap_width(&"x".repeat(45), 20);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|l| l.chars().count() <= 20));
    }

    #[test]
    fn preserves_blank_lines() {
        let out = wrap_width("one\n\ntwo", 40);
        assert_eq!(out, vec!["one", "", "two"]);
    }

    #[test]
    fn starts_at_the_tail_in_follow_mode() {
        let t = Transcript::new(entries(100));
        assert!(t.follow);
        assert!(t.scroll > 0);
    }

    #[test]
    fn step_up_stops_following() {
        let mut t = Transcript::new(entries(100));
        t.follow_tail(H);
        t.step(-1, H);
        assert!(!t.follow);
    }

    #[test]
    fn scroll_clamps_at_both_ends() {
        let mut t = Transcript::new(entries(50));
        t.step(-100_000, H);
        assert_eq!(t.scroll, 0);
        t.step(100_000, H);
        assert!(t.scroll <= t.layout.total_lines().saturating_sub(1));
    }

    #[test]
    fn page_moves_by_half_a_viewport() {
        let mut t = Transcript::new(entries(500));
        // Start from the top: a fresh transcript is pinned to the tail, where
        // paging forward is correctly clamped.
        t.top();
        let before = t.scroll;
        t.page(1, H);
        assert!(t.scroll > before, "page down must advance");
        assert!(
            t.scroll - before <= H as usize + 1,
            "page must be about half a viewport"
        );
    }

    #[test]
    fn top_jumps_to_first_line() {
        let mut t = Transcript::new(entries(100));
        t.top();
        assert_eq!(t.scroll, 0);
        assert!(!t.follow);
    }

    #[test]
    fn follow_tail_repins_to_newest() {
        let mut t = Transcript::new(entries(100));
        t.top();
        t.follow_tail(H);
        assert!(t.follow);
        assert!(t.scroll > 0);
    }

    #[test]
    fn incoming_entries_preserve_a_paused_reader_position() {
        let mut t = Transcript::new(entries(100));
        t.top();
        t.step(4, H);
        let position = t.scroll;
        t.replace_entries(entries(120));
        assert!(!t.follow);
        assert_eq!(t.scroll, position);
    }

    #[test]
    fn identical_session_sync_is_not_a_new_update() {
        let mut t = Transcript::new(entries(10));
        assert!(!t.replace_entries(entries(10)));
    }

    #[test]
    fn incoming_entries_keep_following_at_the_tail() {
        let mut t = Transcript::new(entries(100));
        t.replace_entries(entries(120));
        t.follow_tail(H);
        assert!(t.follow);
        assert_eq!(t.scroll, t.layout.total_lines().saturating_sub(H as usize));
    }

    #[test]
    fn scrolling_down_to_the_tail_resumes_following() {
        let mut t = Transcript::new(entries(100));
        t.top();
        t.page(1_000, H);
        assert!(t.follow);
        let old_tail = t.scroll;
        t.replace_entries(entries(120));
        t.follow_tail(H);
        assert!(t.scroll > old_tail);
    }

    #[test]
    fn jump_to_turn_finds_it() {
        let mut t = Transcript::new(entries(100));
        t.jump_to_turn(40, H);
        assert_eq!(t.layout.entry_at_line(t.scroll), 40);
        assert!(!t.follow);
    }

    #[test]
    fn collapse_keeps_the_layout_valid() {
        let mut t = Transcript::new(entries(20));
        t.top();
        let before = t.layout.total_lines();
        t.toggle_collapse();
        assert!(t.layout.total_lines() <= before);
        assert!(t.scroll <= t.layout.total_lines().saturating_sub(1));
    }

    #[test]
    fn empty_transcript_is_safe() {
        let mut t = Transcript::new(Vec::new());
        assert!(t.is_empty());
        t.step(5, H);
        assert_eq!(t.scroll, 0);
    }

    #[test]
    fn search_matches_full_raw_tool_output_and_navigates() {
        let mut t = Transcript::new(vec![
            Entry {
                role: Role::Tool,
                text: "command: run\n".to_string() + &"hidden output needle\n".repeat(20),
                turn: 1,
                tool_card: None,
            },
            Entry {
                role: Role::User,
                text: "needle in a later message".into(),
                turn: 2,
                tool_card: None,
            },
        ]);
        assert_eq!(t.search("needle"), 2);
        assert!(t.search_next(false, H));
        assert_eq!(t.layout.entry_at_line(t.scroll), 0);
        assert!(t.search_next(false, H));
        let second = t.matches[1];
        assert!(second >= t.scroll && second < t.scroll + H as usize);
        assert!(t.search_next(true, H));
        let first = t.matches[0];
        assert!(first >= t.scroll && first < t.scroll + H as usize);
    }

    #[test]
    fn stable_anchor_survives_reflow_and_appends() {
        let mut t = Transcript::new(entries(80));
        t.top();
        t.step(22, H);
        let text = t
            .layout
            .entry_text(t.layout.entry_at_line(t.scroll))
            .unwrap()
            .to_string();
        t.set_width(20);
        let entry = t.layout.entry_at_line(t.scroll);
        assert_eq!(t.layout.entry_text(entry), Some(text.as_str()));
        t.replace_entries(entries(100));
        assert_eq!(
            t.layout.entry_text(t.layout.entry_at_line(t.scroll)),
            Some(text.as_str())
        );
    }
}

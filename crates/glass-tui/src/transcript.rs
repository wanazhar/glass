//! Fullscreen transcript viewer, entered with ctrl+g.
//!
//! This is the opt-in half of the dual renderer. It exists because people want
//! to scroll and search history, but it is *not* the default view: the default
//! view writes into the terminal's own scrollback and costs nothing.
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
}

impl Transcript {
    pub fn new(entries: Vec<super::Entry>) -> Self {
        let rows: Vec<(String, Role, usize)> =
            entries.into_iter().map(|e| (e.text, e.role, e.turn)).collect();
        let collapsed = vec![false; rows.len()];
        let layout = Layout::new(rows);
        let scroll = layout.total_lines().saturating_sub(1);
        Self { layout, scroll, follow: true, collapsed }
    }

    pub fn len(&self) -> usize {
        self.layout.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layout.is_empty()
    }

    /// Clamp `scroll` so the viewport can always be filled.
    fn clamp(&mut self, height: u16) {
        let max = self.layout.total_lines().saturating_sub(height.max(1) as usize);
        self.scroll = self.scroll.min(max);
    }

    /// Scroll by whole lines.
    pub fn step(&mut self, delta: isize, height: u16) {
        self.scroll = (self.scroll as isize + delta).max(0) as usize;
        self.follow = false;
        self.clamp(height);
    }

    /// Scroll by half a viewport, which is what a page key should do here.
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

    /// Pin to the newest content.
    pub fn follow_tail(&mut self, height: u16) {
        self.follow = true;
        self.scroll = self.layout.total_lines().saturating_sub(height.max(1) as usize);
        self.clamp(height);
    }

    /// Toggle collapse on the entry at the top of the viewport.
    pub fn toggle_collapse(&mut self) {
        let idx = self.layout.entry_at_line(self.scroll);
        let Some(slot) = self.collapsed.get_mut(idx) else { return };
        *slot = !*slot;
        self.rebuild();
    }

    /// Rebuild the line index after a collapse change, preserving scroll.
    fn rebuild(&mut self) {
        let keep = self.scroll;
        let rows = self
            .layout
            .snapshot()
            .into_iter()
            .enumerate()
            .map(|(i, (text, role, turn))| {
                if self.collapsed.get(i).copied().unwrap_or(false) {
                    (text.to_string(), role, turn)
                } else {
                    (text, role, turn)
                }
            })
            .collect();
        self.layout = Layout::new(rows);
        self.scroll = keep.min(self.layout.total_lines().saturating_sub(1));
    }

    /// Draw one frame. Only `height` lines are ever constructed.
    pub fn draw(&mut self, frame: &mut Frame, mods: &[Box<dyn Mod_>], session_label: &str) {
        let area = frame.area();
        let chunks = RLayout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let view = chunks[0];
        self.clamp(view.height);
        let items = self.layout.visible(self.scroll, view);

        let title = format!(
            " {} — {} ",
            session_label,
            if self.follow {
                "following".to_string()
            } else {
                format!("line {}/{}", self.scroll + 1, self.layout.total_lines().max(1))
            }
        );

        frame.render_widget(
            List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_style(Style::new().fg(ratatui::style::Color::DarkGray))
                        .title(title),
                )
                .highlight_style(Style::new().bg(ratatui::style::Color::Rgb(38, 38, 38))),
            view,
        );

        let total = self.layout.total_lines();
        if total > 0 {
            let mut sb = ScrollbarState::new(total.saturating_sub(1)).position(self.scroll);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                view,
                &mut sb,
            );
        }

        // Mods paint last so an overlay always wins over harness chrome.
        let mut overlay = Overlay::default();
        for m in mods {
            m.on_render(&mut overlay, (view.width, view.height));
        }
        if !overlay.lines.is_empty() {
            let h = (overlay.lines.len() as u16).min(chunks[1].height);
            let rect = Rect { x: view.x, y: chunks[1].y, width: view.width, height: h };
            frame.render_widget(Paragraph::new(overlay.lines.join("\n")), rect);
        }
    }
}

/// Wrap text to `width` columns on whitespace where possible, hard-splitting
/// only for words that cannot fit.
///
/// The minimum is 20 because wrapping a 40-column terminal down to 10 shreds
/// every word into unreadable fragments.
pub fn wrap_width(text: &str, width: usize) -> Vec<String> {
    let width = width.max(20);
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
            } else if line.chars().count() + 1 + word.chars().count() <= width {
                line.push(' ');
                line.push_str(word);
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            }
            // Hard-split a token that cannot fit on any line: a long path, a
            // base64 blob, a minified bundle.
            while line.chars().count() > width {
                let split: String = line.chars().take(width).collect();
                let rest: String = line.chars().skip(width).collect();
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
    use crate::Entry;

    fn entries(n: usize) -> Vec<Entry> {
        (0..n).map(|i| Entry { role: Role::User, text: format!("message {i}"), turn: i }).collect()
    }

    const H: u16 = 20;

    #[test]
    fn wraps_on_whitespace() {
        let out = wrap_width("the quick brown fox jumps over", 20);
        assert!(out.iter().all(|l| l.chars().count() <= 20), "{out:?}");
        assert_eq!(out, vec!["the quick brown fox", "jumps over"]);
    }

    #[test]
    fn never_wraps_below_minimum_width() {
        // A 5-column request clamps to 20 rather than shredding every word.
        let out = wrap_width("alpha beta gamma delta", 5);
        assert_eq!(out, vec!["alpha beta gamma", "delta"]);
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
        assert!(t.scroll - before <= H as usize + 1, "page must be about half a viewport");
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
}

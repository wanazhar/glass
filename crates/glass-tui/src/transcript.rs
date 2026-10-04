//! Fullscreen transcript viewer, entered with ctrl+g.
//!
//! This is the opt-in half of the dual renderer. It exists because people
//! want to scroll and search history, but it is *not* the default view: the
//! default view writes into the terminal's own scrollback and costs nothing.
//!
//! The viewer never owns history. It renders from a borrowed snapshot owned
//! by the session log, so entering and leaving it cannot grow the heap by a
//! message's worth of text.

use glass_mod_api::{Mod_, Overlay};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

/// Which role produced a log line, used only for colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    User,
    Assistant,
    Bash,
    Tool,
    System,
    Meta,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::User => "you",
            Role::Assistant => "glass",
            Role::Bash => "bash",
            Role::Tool => "tool",
            Role::System => "system",
            Role::Meta => "",
        }
    }

    fn style(self) -> Style {
        match self {
            Role::User => Style::new().fg(ratatui::style::Color::Cyan).add_modifier(Modifier::BOLD),
            Role::Assistant => Style::new().fg(ratatui::style::Color::Green),
            Role::Bash => Style::new().fg(ratatui::style::Color::Yellow),
            Role::Tool => Style::new().fg(ratatui::style::Color::Magenta),
            Role::System => Style::new().fg(ratatui::style::Color::Red).add_modifier(Modifier::DIM),
            Role::Meta => Style::new().fg(ratatui::style::Color::DarkGray),
        }
    }
}

/// One renderable entry in the transcript.
pub struct Entry {
    pub role: Role,
    pub text: String,
    /// Index into the conversation, used by jump-to-turn.
    pub turn: usize,
}

pub struct Transcript {
    pub entries: Vec<Entry>,
    pub state: ListState,
    pub follow: bool,
    pub collapsed: Vec<bool>,
}

impl Transcript {
    pub fn new(entries: Vec<Entry>) -> Self {
        let len = entries.len();
        let mut state = ListState::default();
        if len > 0 {
            state.select(Some(len - 1));
        }
        Self { entries, state, follow: true, collapsed: vec![false; len] }
    }

    /// Scroll by a page, clamping at both ends. Returns whether it moved.
    pub fn page(&mut self, delta: isize) {
        let last = self.entries.len().saturating_sub(1) as isize;
        if last < 0 {
            return;
        }
        let cur = self.state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, last);
        self.state.select(Some(next as usize));
        self.follow = next == last;
    }

    /// Move by one entry.
    pub fn step(&mut self, delta: isize) {
        self.page(delta);
    }

    /// Jump to the turn carrying the given absolute index.
    pub fn jump_to_turn(&mut self, turn: usize) {
        if let Some(pos) = self.entries.iter().position(|e| e.turn == turn) {
            self.state.select(Some(pos));
            self.follow = false;
        }
    }

    /// Toggle collapse on the selected entry.
    pub fn toggle_collapse(&mut self) {
        let Some(i) = self.state.selected() else { return };
        if let Some(slot) = self.collapsed.get_mut(i) {
            *slot = !*slot;
        }
    }

    /// Pin to the newest entry, which is what follow-mode means.
    pub fn follow_tail(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.state.select(Some(self.entries.len() - 1));
        self.follow = true;
    }

    /// Build the ratatui list. Collapsed entries are replaced by an
    /// ellipsis line, which is the only place the stored text is skipped.
    pub fn render_items(&self, width: u16) -> Vec<ListItem<'static>> {
        let mut items = Vec::with_capacity(self.entries.len());
        for (i, e) in self.entries.iter().enumerate() {
            let collapsed = self.collapsed.get(i).copied().unwrap_or(false);
            if collapsed {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!(" ▸ {} ", e.role.label()), e.role.style()),
                    Span::styled("⋯", Style::new().fg(ratatui::style::Color::DarkGray)),
                ])));
                continue;
            }
            let mut lines: Vec<Line> = Vec::new();
            let head = Span::styled(format!(" {} ", e.role.label()), e.role.style());
            let wrapped = wrap_width(&e.text, width.saturating_sub(2) as usize);
            for (n, w) in wrapped.into_iter().enumerate() {
                if n == 0 {
                    lines.push(Line::from(vec![head.clone(), Span::raw(w)]));
                } else {
                    lines.push(Line::from(Span::raw(w)));
                }
            }
            items.push(ListItem::new(Text::from(lines)));
        }
        items
    }
}

/// Wrap text to `width` columns on whitespace where possible, hard-splitting
/// only for words that cannot fit.
fn wrap_width(text: &str, width: usize) -> Vec<String> {
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
            // Hard-split a single over-long token (a long path, a base64 blob).
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

/// Draw the fullscreen transcript.
pub fn draw(
    frame: &mut Frame,
    transcript: &Transcript,
    mods: &[Box<dyn Mod_>],
    session_label: &str,
) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let items = transcript.render_items(chunks[0].width);
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::new().fg(ratatui::style::Color::DarkGray))
                .title(format!(" {} — {} ", session_label, transcript.follow_string())),
        )
        .highlight_style(Style::new().bg(ratatui::style::Color::Rgb(38, 38, 38)))
        .scroll_padding(1);

    let mut state = transcript.state.clone();
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let sb_len = transcript.entries.len();
    if sb_len > 0 {
        let mut sb_state = ScrollbarState::new(sb_len.saturating_sub(1))
            .position(transcript.state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            chunks[0],
            &mut sb_state,
        );
    }

    // Mods paint last so a mod overlay always wins over harness chrome.
    let mut overlay = Overlay::default();
    for m in mods {
        m.on_render(&mut overlay, (chunks[0].width, chunks[0].height));
    }
    if !overlay.lines.is_empty() {
        let height = overlay.lines.len().min(chunks[1].height as usize) as u16;
        let rect = Rect { x: chunks[0].x, y: chunks[1].y, width: chunks[0].width, height };
        let p = Paragraph::new(overlay.lines.join("\n"));
        frame.render_widget(p, rect);
    }
}

impl Transcript {
    fn follow_string(&self) -> String {
        if self.follow {
            "following".to_string()
        } else {
            format!("{}/{}", self.state.selected().unwrap_or(0) + 1, self.entries.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn page_clamps_at_both_ends() {
        let entries = (0..5).map(|i| Entry { role: Role::User, text: format!("{i}"), turn: i }).collect();
        let mut t = Transcript::new(entries);
        t.page(-100);
        assert_eq!(t.state.selected(), Some(0));
        t.page(100);
        assert_eq!(t.state.selected(), Some(4));
        assert!(t.follow);
    }

    #[test]
    fn collapse_hides_body() {
        let entries = vec![Entry { role: Role::Bash, text: "rm -rf /".into(), turn: 0 }];
        let mut t = Transcript::new(entries);
        t.state.select(Some(0));
        let before = t.render_items(40).len();
        t.toggle_collapse();
        let after = t.render_items(40);
        assert!(before > 0);
        assert_eq!(after.len(), 1);
        let rendered = format!("{:?}", after[0]);
        assert!(rendered.contains('⋯'), "{rendered}");
    }

    #[test]
    fn jump_to_turn_selects_entry() {
        let entries = (0..10).map(|i| Entry { role: Role::User, text: "x".into(), turn: i / 3 }).collect();
        let mut t = Transcript::new(entries);
        t.jump_to_turn(2);
        assert_eq!(t.state.selected(), Some(6));
        assert!(!t.follow);
    }
}

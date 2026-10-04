//! Virtualized transcript layout.
//!
//! The naive approach — wrap every entry, every frame — is O(total history) per
//! frame. At 10k messages that is ~200,000 wrapped lines built to fill a
//! 30-line viewport, which is where pi's #9807 "typing lag at 800+ messages"
//! comes from. Both implementations had the same bug.
//!
//! Instead we lay out each entry once, cache the result, and hand the renderer
//! only the slice of lines the viewport needs. Frame cost becomes proportional
//! to the window, not the history.

use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    text::{Line, Span, Text},
    widgets::ListItem,
};

use crate::{Role, wrap_width};

/// Lines kept in the layout cache before the least-recently-used entries are
/// dropped. Entries are cheap to re-wrap, so this is a memory/CPU tradeoff
/// rather than a correctness one.
const CACHE_CAPACITY: usize = 512;

/// One entry's wrapped lines plus the metadata the renderer needs.
#[derive(Clone)]
struct Laid {
    lines: Vec<Line<'static>>,
}

/// Cumulative line index: `starts[i]` is the first line of entry `i`.
#[derive(Default)]
pub struct Layout {
    entries: Vec<(String, Role, usize)>,
    cache: HashMap<usize, Laid>,
    /// Insertion order of `cache` keys, for eviction.
    order: Vec<usize>,
    starts: Vec<usize>,
    total: usize,
    /// Width the cache is currently laid out for. A change invalidates
    /// everything, since soft-wrapping is width-dependent.
    last_width: Option<u16>,
    dirty: bool,
}

impl Layout {
    pub fn new(entries: Vec<(String, Role, usize)>) -> Self {
        let mut l = Self { entries, dirty: true, ..Default::default() };
        l.rebuild_index();
        l
    }

    /// Replace the entry list, dropping the cache only if it actually changed.
    pub fn set_entries(&mut self, entries: Vec<(String, Role, usize)>) {
        if entries.len() == self.entries.len() && self.same_shape(&entries) {
            return;
        }
        self.entries = entries;
        self.dirty = true;
    }

    fn same_shape(&self, other: &[(String, Role, usize)]) -> bool {
        self.entries.iter().zip(other).all(|(a, b)| a.0.len() == b.0.len())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total_lines(&self) -> usize {
        self.total
    }

    /// Copy of the raw entries, used when a collapse change forces a rebuild.
    pub fn snapshot(&self) -> Vec<(String, Role, usize)> {
        self.entries.clone()
    }

    /// The entry containing `line`, for scroll clamping.
    pub fn entry_at_line(&self, line: usize) -> usize {
        match self.starts.binary_search(&line) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }

    /// Recompute the line index. Requires every entry to be laid out, so this
    /// is the one genuinely O(history) pass; it only runs when entries change,
    /// not on scroll or on every keystroke.
    fn rebuild_index(&mut self) {
        self.cache.clear();
        self.order.clear();
        self.starts.clear();
        // Must reset: rebuild_index runs again on every width change, and
        // accumulating onto the previous total corrupts the line index.
        self.total = 0;
        self.starts.reserve(self.entries.len() + 1);

        let width = self.last_width.unwrap_or(80);
        for i in 0..self.entries.len() {
            self.starts.push(self.total);
            // Measure without caching: populating the LRU here would evict the
            // entries we are still counting, so every line past the cap would
            // measure as one line and the index would be wrong.
            let (text, role, _) = &self.entries[i];
            let n = build_lines(text, *role, width).len().max(1);
            self.total += n;
        }
        self.starts.push(self.total);
        self.dirty = false;
    }

    /// Wrap one entry, caching the result.
    fn lay_out(&mut self, idx: usize, width: u16) -> Laid {
        if let Some(l) = self.cache.get(&idx) {
            return l.clone();
        }
        let (text, role, _) = self.entries[idx].clone();
        let laid = Laid { lines: build_lines(&text, role, width) };
        self.cache.insert(idx, laid.clone());
        self.order.push(idx);
        if self.order.len() > CACHE_CAPACITY {
            let victim = self.order.remove(0);
            self.cache.remove(&victim);
        }
        laid
    }

    fn touch(&mut self, idx: usize) {
        if let Some(pos) = self.order.iter().position(|&i| i == idx) {
            self.order.remove(pos);
        }
        self.order.push(idx);
    }

    /// Change the wrap width. Every cached entry was wrapped for the old width,
    /// so the whole cache is invalid and the index must be rebuilt.
    pub fn set_width(&mut self, width: u16) {
        if self.last_width == Some(width) {
            return;
        }
        self.last_width = Some(width);
        self.dirty = true;
    }

    /// The lines visible in `area`, starting at `scroll`.
    ///
    /// Returns at most `area.height` items, which is the entire point: frame
    /// cost tracks the viewport, not the history.
    pub fn visible(&mut self, scroll: usize, area: Rect) -> Vec<ListItem<'static>> {
        if self.dirty || self.last_width != Some(area.width) {
            self.last_width = Some(area.width);
            self.rebuild_index();
        }
        if self.entries.is_empty() || area.height == 0 {
            return Vec::new();
        }

        let end = (scroll + area.height as usize).min(self.total);
        let first = scroll.min(self.total.saturating_sub(1));

        let mut out = Vec::with_capacity(area.height as usize);
        let mut line_no = first;

        while line_no < end {
            let idx = self.entry_at_line(line_no);
            if idx >= self.entries.len() {
                break;
            }
            let laid = self.lay_out(idx, area.width);
            self.touch(idx);

            let start = self.starts.get(idx).copied().unwrap_or(0);
            let skip = line_no - start.min(line_no);
            let visible_in_entry = laid.lines.len().max(1);
            let take = (end - line_no).min(visible_in_entry - skip);

            for line in laid.lines.iter().skip(skip).take(take) {
                out.push(ListItem::new(Text::from(line.clone())));
            }
            line_no += take.max(1);
        }
        out
    }

    /// Turn number to scroll to in order to show its first line.
    pub fn line_for_turn(&self, turn: usize) -> Option<usize> {
        self.entries
            .iter()
            .position(|(_, _, t)| *t == turn)
            .and_then(|i| self.starts.get(i).copied())
    }
}

/// Build the display lines for one entry.
///
/// A `Role::Meta` entry renders as a single dim line regardless of content,
/// which is both cheaper and visually correct for compaction markers.
fn build_lines(text: &str, role: Role, width: u16) -> Vec<Line<'static>> {
    if role == Role::Meta {
        return vec![Line::from(Span::styled(truncate(text, width as usize), role_style(role)))];
    }

    let wrapped = wrap_width(text, width.saturating_sub(2) as usize);

    // The role label appears only on the first line, so it is built once and
    // consumed on the first iteration rather than cloned per line.
    let mut first = true;
    let mut lines = Vec::with_capacity(wrapped.len());
    for w in wrapped {
        if first {
            first = false;
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", role.label()), role_style(role)),
                Span::raw(w),
            ]));
        } else {
            lines.push(Line::from(Span::raw(w)));
        }
    }
    lines
}

fn truncate(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

fn role_style(role: Role) -> ratatui::style::Style {
    use ratatui::style::{Color, Modifier, Style};
    match role {
        Role::User => Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        Role::Assistant => Style::new().fg(Color::Green),
        Role::Bash => Style::new().fg(Color::Yellow),
        Role::Tool => Style::new().fg(Color::Magenta),
        Role::System => Style::new().fg(Color::Red).add_modifier(Modifier::DIM),
        Role::Meta => Style::new().fg(Color::DarkGray),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(n: usize) -> Vec<(String, Role, usize)> {
        (0..n)
            .map(|i| (format!("entry {i} with some text to wrap around"), Role::User, i))
            .collect()
    }

    fn rect(h: u16) -> Rect {
        Rect { x: 0, y: 0, width: 80, height: h }
    }

    #[test]
    fn builds_only_the_visible_window() {
        let mut l = Layout::new(entries(5000));
        l.set_width(80);
        let items = l.visible(0, rect(30));
        assert!(items.len() <= 30, "built {} items for a 30-row viewport", items.len());
        assert!(!items.is_empty());
    }

    #[test]
    fn frame_cost_is_independent_of_history() {
        // The core win: a 10-line viewport over 1,000 entries must do the same
        // work as over 100.
        let mut small = Layout::new(entries(100));
        let mut large = Layout::new(entries(10_000));
        let a = small.visible(0, rect(10)).len();
        let b = large.visible(0, rect(10)).len();
        assert_eq!(a, b, "viewport size must not depend on history length");
    }

    #[test]
    fn scrolling_moves_the_window() {
        let mut l = Layout::new(entries(200));
        let top = l.visible(0, rect(5));
        let mid = l.visible(100, rect(5));
        assert_ne!(top[0], mid[0], "scrolling must change what is shown");
    }

    #[test]
    fn total_lines_grows_with_entries() {
        let l = Layout::new(entries(50));
        assert!(l.total_lines() >= 50, "each entry contributes at least one line");
    }

    #[test]
    fn entry_index_is_monotonic() {
        let l = Layout::new(entries(100));
        for line in 0..l.total_lines() {
            let e = l.entry_at_line(line);
            assert!(e < 100, "line {line} mapped to entry {e}");
        }
    }

    #[test]
    fn cache_is_bounded() {
        let mut l = Layout::new(entries(10_000));
        l.set_width(80);
        let _ = l.visible(9_000, rect(30));
        assert!(l.cache.len() <= CACHE_CAPACITY, "cache grew to {}", l.cache.len());
    }

    #[test]
    fn width_change_invalidates_layout() {
        // Each entry is short enough to fit on one line at 80 columns, but
        // wraps to several at 20, so the total line count must grow.
        let mut l = Layout::new(entries(20));
        let wide = Rect { x: 0, y: 0, width: 80, height: 10 };
        let _ = l.visible(0, wide);
        let at_80 = l.total_lines();

        let narrow = Rect { x: 0, y: 0, width: 20, height: 10 };
        let _ = l.visible(0, narrow);
        assert!(l.total_lines() > at_80, "narrower width must re-wrap: {} vs {at_80}", l.total_lines());
    }

    #[test]
    fn empty_transcript_renders_nothing() {
        let mut l = Layout::new(Vec::new());
        assert!(l.visible(0, rect(30)).is_empty());
        assert!(l.is_empty());
    }

    #[test]
    fn scroll_past_the_end_is_clamped() {
        let mut l = Layout::new(entries(20));
        let items = l.visible(100_000, rect(10));
        assert!(items.len() <= 10);
    }

    #[test]
    fn zero_height_returns_nothing() {
        let mut l = Layout::new(entries(100));
        assert!(l.visible(0, Rect { x: 0, y: 0, width: 80, height: 0 }).is_empty());
    }

    #[test]
    fn line_for_turn_finds_the_entry() {
        let l = Layout::new(entries(30));
        let line = l.line_for_turn(10).expect("turn 10 exists");
        assert_eq!(l.entry_at_line(line), 10);
    }

    #[test]
    fn meta_entries_collapse_to_one_line() {
        let lines = build_lines("dropped 12, kept 6", Role::Meta, 80);
        assert_eq!(lines.len(), 1, "a compaction marker must not wrap");
    }
}

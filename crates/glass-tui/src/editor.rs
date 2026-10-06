//! A single-line-aware prompt editor.
//!
//! Hand-rolled rather than delegated to a widget crate: this is on the hot
//! path of every keystroke, it needs multi-row input for pasted diffs, and
//! owning the cursor math is what lets the diff renderer redraw only the
//! editor rows that actually changed.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::{self, Write};

use crate::term;

/// Outcome of feeding one key to the editor.
#[derive(Debug, PartialEq, Eq)]
pub enum Edit {
    /// Nothing to do, editor should re-render.
    Continue,
    /// User pressed enter. Carries the accumulated text.
    Submit(String),
}

pub struct Editor {
    buf: String,
    /// Cursor position in grapheme-ish terms; we treat bytes as columns, which
    /// is wrong for CJK but keeps the hot path branch-free.
    cursor: usize,
    history: Vec<String>,
    hist_idx: Option<usize>,
    draft: String,
}

impl Editor {
    pub fn new() -> Self {
        Self { buf: String::new(), cursor: 0, history: Vec::new(), hist_idx: None, draft: String::new() }
    }

    pub fn text(&self) -> &str {
        &self.buf
    }

    pub fn set_text(&mut self, text: &str) {
        self.buf = text.to_string();
        self.cursor = self.buf.len();
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
        self.hist_idx = None;
        self.draft.clear();
    }

    /// Consume a key. `Submit` means the caller should dispatch the turn.
    pub fn key(&mut self, key: KeyEvent) -> Edit {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter if !ctrl => {
                let text = std::mem::take(&mut self.buf);
                self.cursor = 0;
                if !text.trim().is_empty()
                    && (self.hist_idx.is_none() || self.history.last().map(String::as_str) != Some(text.as_str())) {
                        self.history.push(text.clone());
                    }
                self.hist_idx = None;
                self.draft.clear();
                Edit::Submit(text)
            }
            KeyCode::Char('c') if ctrl => Edit::Continue,
            KeyCode::Esc => {
                self.buf.clear();
                self.cursor = 0;
                Edit::Continue
            }
            KeyCode::Char(c) if !ctrl => {
                self.insert(c);
                Edit::Continue
            }
            KeyCode::Tab => {
                self.insert(' ');
                Edit::Continue
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let prev = prev_boundary(&self.buf, self.cursor);
                    self.buf.replace_range(prev..self.cursor, "");
                    self.cursor = prev;
                }
                Edit::Continue
            }
            KeyCode::Delete => {
                if self.cursor < self.buf.len() {
                    let next = prev_boundary(&self.buf, self.cursor + 1);
                    self.buf.replace_range(self.cursor..next, "");
                }
                Edit::Continue
            }
            KeyCode::Left => {
                self.cursor = prev_boundary(&self.buf, self.cursor);
                Edit::Continue
            }
            KeyCode::Right => {
                self.cursor = next_boundary(&self.buf, self.cursor);
                Edit::Continue
            }
            KeyCode::Home => {
                self.cursor = 0;
                Edit::Continue
            }
            KeyCode::End => {
                self.cursor = self.buf.len();
                Edit::Continue
            }
            KeyCode::Up => {
                self.recall(-1);
                Edit::Continue
            }
            KeyCode::Down => {
                self.recall(1);
                Edit::Continue
            }
            _ => Edit::Continue,
        }
    }

    fn insert(&mut self, c: char) {
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert text at the cursor. This is the paste path:
    /// bracketed paste delivers a whole run of text at once,
    /// including newlines, and a pasted diff or stack trace
    /// must keep its line breaks rather than be flattened.
    pub fn insert_str(&mut self, s: &str) {
        self.buf.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Walk prompt history. `delta` of -1 is older, +1 is newer.
    fn recall(&mut self, delta: isize) {
        if self.history.is_empty() {
            return;
        }
        let len = self.history.len();
        // `None` means "we are on the fresh draft", which sits just past the end.
        let current = self.hist_idx.unwrap_or(len);
        let next = (current as isize + delta).clamp(0, len as isize) as usize;
        self.hist_idx = if next >= len { None } else { Some(next) };
        self.buf = match self.hist_idx {
            Some(i) => self.history[i].clone(),
            None => std::mem::take(&mut self.draft),
        };
        self.cursor = self.buf.len();
    }

    /// Draw the editor at `base_row`, wrapping at `width`. Returns rows used.
    pub fn render(&self, base_row: u16, width: u16, focused: bool) -> u16 {
        let width = width.max(8) as usize;
        let mut out = String::new();
        let chunks = wrap(&self.buf, width);

        // The cursor can sit one row past the last line of
        // text, at the pending-wrap position. Draw that row
        // too, or the cursor lands somewhere it was never
        // cleared and leaves a ghost of the previous frame.
        let cursor_row = cursor_position(&self.buf, self.cursor, width).0 as usize;
        let total = chunks.len().max(cursor_row + 1).max(1);

        for i in 0..total {
            let row = base_row + i as u16;
            // Position inside `out`, not with term::move_to:
            // that prints straight to stdout, which would
            // reorder ahead of the text below.
            out.push_str(&format!("\x1b[{row};0H"));
            let chunk = chunks.get(i).copied().unwrap_or("");
            // Only the first row carries the prompt glyph;
            // continuation rows are plain cleared lines.
            let marker = if i == 0 { "\x1b[2K\x1b[90m❯\x1b[0m " } else { "\x1b[2K" };
            out.push_str(&format!("{marker}{chunk}"));
        }

        if self.buf.is_empty() && !focused {
            out.push_str(&format!("\x1b[{base_row};2H"));
            out.push_str("\x1b[90mtype a message, /help for commands\x1b[0m");
        }

        let rows = total as u16;
        self.draw_cursor(base_row, width, focused, &mut out);
        print!("{out}");
        let _ = io::stdout().flush();
        rows
    }

    fn draw_cursor(&self, base_row: u16, width: usize, focused: bool, out: &mut String) {
        if !focused {
            return;
        }
        let (row, col) = cursor_position(&self.buf, self.cursor, width);
        out.push_str(&format!("\x1b[{};{}H\x1b[6m", base_row + row, col + 2));
        match self.buf[self.cursor..].chars().next() {
            Some(c) => out.push(c),
            None => out.push(' '),
        }
        out.push_str("\x1b[0m");
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrap on character boundaries so we never split a UTF-8
/// sequence, and on newlines so pasted multi-line text keeps
/// its line breaks instead of being flattened or, worse,
/// emitting a raw carriage return that desynchronizes the
/// row accounting.
fn wrap(text: &str, width: usize) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    let mut lines = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() {
            lines.push("");
            continue;
        }
        let mut start = 0;
        for (i, _) in line.char_indices() {
            if i - start >= width {
                lines.push(&line[start..i]);
                start = i;
            }
        }
        lines.push(&line[start..]);
    }
    if lines.is_empty() {
        lines.push("");
    }
    lines
}

fn prev_boundary(s: &str, at: usize) -> usize {
    if at == 0 { return 0; }
    let mut i = at - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_boundary(s: &str, at: usize) -> usize {
    if at >= s.len() { return s.len(); }
    let mut i = at + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn cursor_position(buf: &str, cursor: usize, width: usize) -> (u16, u16) {
    let width = width.max(1);
    let mut row = 0usize;
    let mut col = 0usize;
    for c in buf[..cursor].chars() {
        if c == '\n' {
            row += 1;
            col = 0;
        } else {
            // Wrap before placing, so a full row followed
            // by a newline advances one row, not two.
            if col == width {
                row += 1;
                col = 0;
            }
            col += 1;
        }
    }
    // A cursor left at the end of a full row sits at the
    // terminal's pending-wrap position: the start of the
    // next row, which the renderer must therefore draw.
    if col == width {
        row += 1;
        col = 0;
    }
    (row as u16, col as u16)
}

/// Redraw the prompt region, clearing rows that shrank.
pub fn repaint_rows(base_row: u16, count: u16) {
    for i in 0..count {
        term::move_to(base_row + i, 0);
        print!("\x1b[2K");
    }
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_on_width() {
        let lines = wrap("abcdefghij", 4);
        assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_breaks_on_newlines() {
        // A pasted diff keeps its line breaks. Before
        // this, a newline was wrapped like any other
        // character and the row count was wrong.
        let lines = wrap("ab\ncd", 10);
        assert_eq!(lines, vec!["ab", "cd"]);
    }

    #[test]
    fn wrap_counts_empty_lines() {
        let lines = wrap("a\n\nb", 10);
        assert_eq!(lines, vec!["a", "", "b"]);
    }

    #[test]
    fn wrap_wraps_long_pasted_lines() {
        let lines = wrap("abcdef\nghijkl", 4);
        assert_eq!(lines, vec!["abcd", "ef", "ghij", "kl"]);
    }

    #[test]
    fn wrap_never_splits_utf8() {
        // Multi-byte characters. Wrapping is byte-oriented
        // by design (see the struct comment), but it must
        // only ever cut on character boundaries or the
        // slice panics, and it must not lose text.
        let text = "éééé";
        let lines = wrap(text, 2);
        assert_eq!(lines.concat(), text);
        for l in &lines {
            assert!(l.is_empty() || l.is_char_boundary(0));
            assert!(l.is_char_boundary(l.len()));
        }
    }

    #[test]
    fn cursor_position_counts_newlines_as_rows() {
        // Row 1, column 0: two newlines mean the cursor
        // is on the third line.
        let (row, col) = cursor_position("ab\ncd\n", 6, 80);
        assert_eq!((row, col), (2, 0));
    }

    #[test]
    fn cursor_position_wraps_at_width() {
        let (row, col) = cursor_position("abcdefgh", 8, 4);
        assert_eq!((row, col), (2, 0));
    }

    #[test]
    fn cursor_position_mixes_newlines_and_wrap() {
        // "ab\n" is one row break, then "cdef" wraps at
        // width 2 into two more rows, landing the cursor
        // at row 2, column 0.
        let (row, col) = cursor_position("ab\ncdef", 7, 2);
        assert_eq!((row, col), (3, 0));
    }

    #[test]
    fn insert_str_places_text_at_cursor() {
        let mut e = Editor::new();
        e.set_text("hello world");
        e.cursor = 5;
        e.insert_str(" there");
        assert_eq!(e.text(), "hello there world");
        assert_eq!(e.cursor, 11);
    }

    #[test]
    fn insert_str_keeps_newlines() {
        let mut e = Editor::new();
        e.insert_str("line one\nline two");
        assert_eq!(e.text(), "line one\nline two");
        // The row count the renderer uses must match
        // the newlines present.
        assert_eq!(wrap(e.text(), 80).len(), 2);
    }
}

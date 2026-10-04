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

        for (i, chunk) in wrap(&self.buf, width).into_iter().enumerate() {
            let row = base_row + i as u16;
            term::move_to(row, 0);
            out.push_str(&format!("\x1b[2K\x1b[90m❯\x1b[0m {chunk}"));
        }

        if self.buf.is_empty() && !focused {
            term::move_to(base_row, 2);
            out.push_str("\x1b[90mtype a message, /help for commands\x1b[0m");
        }

        let rows = wrap(&self.buf, width).len().max(1) as u16;
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

/// Wrap on character boundaries so we never split a UTF-8 sequence.
fn wrap(text: &str, width: usize) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, _) in text.char_indices() {
        if i - start >= width {
            lines.push(&text[start..i]);
            start = i;
        }
    }
    lines.push(&text[start..]);
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
    let head = &buf[..cursor];
    let row = head.chars().count() / width;
    let col = head.chars().count() % width;
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

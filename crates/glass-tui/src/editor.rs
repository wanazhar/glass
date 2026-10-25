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
    /// UTF-8 byte offset of the insertion point; layout counts characters.
    cursor: usize,
    history: Vec<String>,
    hist_idx: Option<usize>,
    draft: String,
    /// The column the cursor is drifting toward across
    /// consecutive up/down moves. `None` means the next
    /// vertical move uses the column the cursor is on.
    goal_col: Option<usize>,
    painted: std::cell::Cell<usize>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            cursor: 0,
            history: Vec::new(),
            hist_idx: None,
            draft: String::new(),
            goal_col: None,
            painted: std::cell::Cell::new(0),
        }
    }

    /// Replace the recalled history, e.g. with prompts
    /// persisted from earlier sessions.
    pub fn set_history(&mut self, history: Vec<String>) {
        self.history = history;
        self.hist_idx = None;
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn text(&self) -> &str {
        &self.buf
    }

    pub fn set_text(&mut self, text: &str) {
        self.buf = text.to_string();
        self.cursor = self.buf.len();
    }

    /// Place the cursor at a UTF-8 byte offset, clamped to a valid boundary.
    /// Hosts with durable drafts can restore their own cursor without giving
    /// this editor ownership of persistence or session identity.
    pub fn set_cursor(&mut self, cursor: usize) {
        let cursor = cursor.min(self.buf.len());
        self.cursor = (0..=cursor)
            .rev()
            .find(|offset| self.buf.is_char_boundary(*offset))
            .unwrap_or(0);
        self.goal_col = None;
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
            KeyCode::Enter
                if ctrl
                    || key.modifiers.contains(KeyModifiers::SHIFT)
                    || key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.insert('\n');
                Edit::Continue
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.buf);
                self.cursor = 0;
                if !text.trim().is_empty()
                    && (self.hist_idx.is_none()
                        || self.history.last().map(String::as_str) != Some(text.as_str()))
                {
                    self.history.push(text.clone());
                }
                self.hist_idx = None;
                self.draft.clear();
                Edit::Submit(text)
            }
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
                Edit::Continue
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = self.buf.len();
                Edit::Continue
            }
            KeyCode::Char('u') if ctrl => {
                self.buf.drain(..self.cursor);
                self.cursor = 0;
                self.goal_col = None;
                Edit::Continue
            }
            KeyCode::Char('k') if ctrl => {
                self.buf.truncate(self.cursor);
                self.goal_col = None;
                Edit::Continue
            }
            KeyCode::Char('w') if ctrl => {
                let mut start = self.cursor;
                while start > 0
                    && self.buf[..start]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_whitespace)
                {
                    start = prev_boundary(&self.buf, start);
                }
                while start > 0
                    && self.buf[..start]
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_whitespace())
                {
                    start = prev_boundary(&self.buf, start);
                }
                self.buf.drain(start..self.cursor);
                self.cursor = start;
                self.goal_col = None;
                Edit::Continue
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
                    self.goal_col = None;
                }
                Edit::Continue
            }
            KeyCode::Delete => {
                if self.cursor < self.buf.len() {
                    let next = next_boundary(&self.buf, self.cursor);
                    self.buf.replace_range(self.cursor..next, "");
                    self.goal_col = None;
                }
                Edit::Continue
            }
            KeyCode::Left => {
                self.cursor = prev_boundary(&self.buf, self.cursor);
                self.goal_col = None;
                Edit::Continue
            }
            KeyCode::Right => {
                self.cursor = next_boundary(&self.buf, self.cursor);
                self.goal_col = None;
                Edit::Continue
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.goal_col = None;
                Edit::Continue
            }
            KeyCode::End => {
                self.cursor = self.buf.len();
                self.goal_col = None;
                Edit::Continue
            }
            KeyCode::Up => {
                // A multi-line draft is an editing surface:
                // up and down move the cursor between its
                // lines. Only a single-line buffer treats
                // them as history recall.
                if self.buf.contains('\n') {
                    self.move_line(-1);
                } else {
                    self.recall(-1);
                }
                Edit::Continue
            }
            KeyCode::Down => {
                if self.buf.contains('\n') {
                    self.move_line(1);
                } else {
                    self.recall(1);
                }
                Edit::Continue
            }
            _ => Edit::Continue,
        }
    }

    fn insert(&mut self, c: char) {
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.goal_col = None;
    }

    /// Insert text at the cursor. This is the paste path:
    /// bracketed paste delivers a whole run of text at once,
    /// including newlines, and a pasted diff or stack trace
    /// must keep its line breaks rather than be flattened.
    pub fn insert_str(&mut self, s: &str) {
        self.buf.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.goal_col = None;
    }

    /// Walk prompt history. `delta` of -1 is older, +1 is newer.
    fn recall(&mut self, delta: isize) {
        if self.history.is_empty() {
            return;
        }
        let len = self.history.len();
        // `None` means "we are on the fresh draft", which sits just past the end.
        let current = self.hist_idx.unwrap_or(len);
        if self.hist_idx.is_none() && delta < 0 {
            self.draft = self.buf.clone();
        }
        let current = isize::try_from(current).unwrap_or(isize::MAX);
        let signed_len = isize::try_from(len).unwrap_or(isize::MAX);
        let next =
            usize::try_from(current.saturating_add(delta).clamp(0, signed_len)).unwrap_or_default();
        self.hist_idx = if next >= len { None } else { Some(next) };
        self.buf = match self.hist_idx {
            Some(i) => self.history[i].clone(),
            None => std::mem::take(&mut self.draft),
        };
        self.cursor = self.buf.len();
        self.goal_col = None;
    }

    /// Move the cursor up (`delta` -1) or down (+1) one
    /// logical line, keeping the column.
    ///
    /// The column is remembered across consecutive moves
    /// so that stepping up and back down lands on the
    /// character you started on, even when the lines are
    /// different lengths: the goal is clamped to each
    /// line, but not overwritten by the clamped result.
    fn move_line(&mut self, delta: isize) {
        let starts = self.line_starts();
        if starts.len() < 2 {
            return;
        }
        // The line the cursor is on is the last one that
        // starts at or before it.
        let cur = starts.partition_point(|&s| s <= self.cursor) - 1;
        // Column in characters, not bytes: a byte column
        // would drift inside a multi-byte character.
        let col = self.buf[starts[cur]..self.cursor].chars().count();
        let goal = self.goal_col.unwrap_or(col);
        self.goal_col = Some(goal);

        let cur = isize::try_from(cur).unwrap_or(isize::MAX);
        let last_line = isize::try_from(starts.len() - 1).unwrap_or(isize::MAX);
        let target =
            usize::try_from(cur.saturating_add(delta).clamp(0, last_line)).unwrap_or_default();
        let line_start = starts[target];
        let line_end = if target + 1 < starts.len() {
            starts[target + 1] - 1
        } else {
            self.buf.len()
        };
        let line = &self.buf[line_start..line_end];
        let target_col = goal.min(line.chars().count());

        // Turn the character column back into a byte
        // offset inside the target line.
        let mut off = 0usize;
        for (seen, c) in line.chars().enumerate() {
            if seen == target_col {
                break;
            }
            off += c.len_utf8();
        }
        self.cursor = line_start + off;
    }

    /// Byte offset of the start of every logical line.
    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0usize];
        for (i, c) in self.buf.char_indices() {
            if c == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    /// Draw one prompt frame, then flush before waiting for another key.
    pub fn render(&self, base_row: u16, width: u16, focused: bool) -> u16 {
        let mut stdout = crate::theme::TerminalWriter::new(io::stdout().lock());
        self.render_into(&mut stdout, base_row, width, focused)
            .unwrap_or(1)
    }

    /// A single-row viewport while tools/providers own the neighboring output row.
    pub fn render_limited(&self, base_row: u16, width: u16, focused: bool, rows: usize) -> u16 {
        let mut stdout = crate::theme::TerminalWriter::new(io::stdout().lock());
        self.render_into_limited(&mut stdout, base_row, width, focused, rows)
            .unwrap_or(1)
    }

    /// Terminal cursor cell after rendering the editor in this viewport.
    pub fn cursor_cell(&self, base_row: u16, width: u16, rows: usize) -> (u16, u16) {
        let terminal_height = crossterm::terminal::size().unwrap_or((80, 24)).1.max(1);
        let base_row = base_row.max(1).min(terminal_height);
        let prefix = usize::from(width >= 3) * 2;
        let content_width = usize::from(width.max(1)).saturating_sub(prefix).max(1);
        let (cursor_row, cursor_col) = cursor_position(&self.buf, self.cursor, content_width);
        let capacity = usize::from(terminal_height - base_row + 1).clamp(1, rows.clamp(1, 5));
        let start = (cursor_row as usize).saturating_sub(capacity - 1);
        (
            base_row + cursor_row.saturating_sub(u16::try_from(start).unwrap_or(u16::MAX)),
            u16::try_from((usize::from(cursor_col) + prefix + 1).min(usize::from(width.max(1))))
                .unwrap_or(u16::MAX),
        )
    }

    pub fn render_into(
        &self,
        io: &mut impl Write,
        base_row: u16,
        width: u16,
        focused: bool,
    ) -> io::Result<u16> {
        self.render_into_limited(io, base_row, width, focused, 5)
    }
    fn render_into_limited(
        &self,
        io: &mut impl Write,
        base_row: u16,
        width: u16,
        focused: bool,
        rows: usize,
    ) -> io::Result<u16> {
        let terminal_height = crossterm::terminal::size().unwrap_or((80, 24)).1.max(1);
        let base_row = base_row.max(1).min(terminal_height);
        let prefix = usize::from(width >= 3) * 2;
        let content_width = usize::from(width.max(1)).saturating_sub(prefix).max(1);
        let chunks = crate::text::wrap(&self.buf, content_width);
        let (cursor_row, cursor_col) = cursor_position(&self.buf, self.cursor, content_width);
        let total = chunks.len().max(cursor_row as usize + 1).max(1);
        let capacity = usize::from(terminal_height - base_row + 1).clamp(1, rows.clamp(1, 5));
        let start = (cursor_row as usize).saturating_sub(capacity - 1);
        let visible = (total - start).min(capacity);
        let previous = self.painted.replace(visible).min(capacity);
        for i in 0..visible.max(previous) {
            let chunk = chunks.get(start + i).map(String::as_str).unwrap_or("");
            let marker = if prefix == 0 {
                ""
            } else if start + i == 0 {
                "❯\x1b[0m "
            } else {
                "  "
            };
            write!(
                io,
                "\x1b[{};1H\x1b[2K",
                base_row + u16::try_from(i).unwrap_or_default()
            )?;
            if i < visible {
                if start + i == 0 && prefix > 0 {
                    write!(io, "{}{marker}{chunk}", crate::theme::accent())?;
                } else {
                    write!(io, "{marker}{chunk}")?;
                }
            }
        }
        if self.buf.is_empty() && !focused {
            let hint = crate::text::clip("type a message, / for commands", content_width);
            write!(
                io,
                "\x1b[{base_row};{}H{}{hint}\x1b[0m",
                prefix + 1,
                crate::theme::ansi(crate::theme::Tone::Muted)
            )?;
        }
        if focused {
            write!(
                io,
                "\x1b[{};{}H\x1b[?25h",
                base_row + cursor_row.saturating_sub(u16::try_from(start).unwrap_or(u16::MAX)),
                (usize::from(cursor_col) + prefix + 1).min(usize::from(width.max(1)))
            )?;
        }
        io.flush()?;
        Ok(u16::try_from(visible).unwrap_or(u16::MAX))
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
#[cfg(test)]
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
        let mut cells = 0;
        for (i, c) in line.char_indices() {
            let n = crate::text::char_width(c);
            if cells + n > width {
                lines.push(&line[start..i]);
                start = i;
                cells = 0;
            }
            cells += n;
        }
        lines.push(&line[start..]);
    }
    if lines.is_empty() {
        lines.push("");
    }
    lines
}

fn prev_boundary(s: &str, at: usize) -> usize {
    if at == 0 {
        return 0;
    }
    let mut i = at - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_boundary(s: &str, at: usize) -> usize {
    if at >= s.len() {
        return s.len();
    }
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
            let cells = crate::text::char_width(c);
            if col + cells > width || col == width {
                row += 1;
                col = 0;
            }
            col += cells;
        }
    }
    // A cursor left at the end of a full row sits at the
    // terminal's pending-wrap position: the start of the
    // next row, which the renderer must therefore draw.
    if col == width {
        row += 1;
        col = 0;
    }
    (
        u16::try_from(row).unwrap_or(u16::MAX),
        u16::try_from(col).unwrap_or(u16::MAX),
    )
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
    fn set_cursor_clamps_to_utf8_boundaries() {
        let mut editor = Editor::new();
        editor.set_text("aéz");
        editor.set_cursor(2);
        assert_eq!(editor.cursor(), 1);
        editor.set_cursor(3);
        assert_eq!(editor.cursor(), 3);
    }

    #[test]
    fn typed_character_is_not_overwritten_by_a_cursor_cell() {
        let mut e = Editor::new();
        e.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let mut out = Vec::new();
        e.render_into(&mut out, 5, 80, true).unwrap();
        let frame = String::from_utf8(out).unwrap();
        assert!(frame.contains("❯\x1b[0m a"));
        assert!(frame.ends_with("\x1b[5;4H\x1b[?25h"));
        assert!(!frame.contains("\x1b[6m"), "cursor must not repaint text");
        e.key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        let mut out = Vec::new();
        e.render_into(&mut out, 5, 80, true).unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .ends_with("\x1b[5;5H\x1b[?25h")
        );
    }

    #[test]
    fn rendered_wrap_reserves_prompt_columns_and_keeps_cursor_in_bounds() {
        let mut e = Editor::new();
        e.set_text("abcdefgh");
        let mut out = Vec::new();
        assert_eq!(e.render_into(&mut out, 5, 8, true).unwrap(), 2);
        let frame = String::from_utf8(out).unwrap();
        assert!(frame.contains("abcdef"));
        assert!(frame.contains("  gh"));
        assert!(frame.ends_with("\x1b[6;5H\x1b[?25h"));
    }

    #[test]
    fn moving_cursor_within_text_does_not_write_an_extra_character() {
        let mut e = Editor::new();
        e.set_text("ab");
        e.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        let mut out = Vec::new();
        e.render_into(&mut out, 1, 80, true).unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .ends_with("\x1b[1;4H\x1b[?25h")
        );
    }

    #[test]
    fn move_line_keeps_the_column() {
        let mut e = Editor::new();
        e.insert_str("ab\ncd\nef");
        // Cursor at the end of "ef", column 2. Move up
        // twice: land on "cd" then "ab", same column.
        e.move_line(-1);
        assert_eq!(e.cursor, 5, "on 'cd' at column 2");
        e.move_line(-1);
        assert_eq!(e.cursor, 2, "on 'ab' at column 2");
    }

    #[test]
    fn move_line_clamps_to_shorter_lines() {
        let mut e = Editor::new();
        e.insert_str("abcd\nef");
        // Column 4 on "abcd"; move down to "ef" which is
        // only 2 long, so the cursor clamps to its end.
        e.cursor = 4;
        e.move_line(1);
        assert_eq!(e.cursor, 7, "clamped to the end of 'ef'");
        // The goal column survives, so moving back up
        // restores the original column.
        e.move_line(-1);
        assert_eq!(e.cursor, 4, "goal column restored");
    }

    #[test]
    fn move_line_never_leaves_the_buffer() {
        let mut e = Editor::new();
        e.insert_str("a\nb\nc");
        e.cursor = 0;
        e.move_line(-1);
        e.move_line(-1);
        assert_eq!(e.cursor, 0, "cannot move above the first line");
        e.move_line(1);
        e.move_line(1);
        e.move_line(1);
        assert_eq!(e.cursor, 4, "cannot move below the last line");
    }

    #[test]
    fn single_line_buffer_still_recalls_history() {
        let mut e = Editor::new();
        for prompt in ["one", "two", "three"] {
            e.insert_str(prompt);
            e.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        // A fresh single-line draft.
        e.insert_str("one");
        // A single-line buffer has nowhere to move between,
        // so Up must fall back to history, not line motion.
        e.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(e.text(), "three", "recalled, not moved");
    }

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
    #[test]
    fn busy_single_row_preserves_the_full_multiline_draft() {
        let mut editor = Editor::new();
        editor.set_text("first line\nsecond line\nlast line");
        let mut output = Vec::new();
        assert_eq!(
            editor
                .render_into_limited(&mut output, 6, 20, true, 1)
                .unwrap(),
            1
        );
        assert_eq!(editor.text(), "first line\nsecond line\nlast line");
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("last line"));
        assert!(!output.contains("\x1b[7;1H"));
    }
}

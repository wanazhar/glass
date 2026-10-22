//! The sole owner of primary-screen body, status and composer rows.
//! Output scrolls in a top-anchored region; cosmetic rows never enter that region.
use glass_tui::{
    Editor,
    theme::{self, Tone},
};
use std::{cell::RefCell, io::{self, Write}};
thread_local! { static PRIMARY: RefCell<Option<Surface>> = const { RefCell::new(None) }; }
struct Surface {
    width: u16,
    height: u16,
    x: usize,
    y: usize,
    overlay: bool,
    status: String,
    todo: crate::todo::Board,
    status_color: String,
    body: std::collections::VecDeque<String>,
    body_bytes: usize,
    repainting: bool,
    reading: bool,
    cursor: (u16, u16),
    reader_invalidated: bool,
}
fn size() -> (u16, u16) {
    let (w, h) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
    (w.max(1), h.max(1))
}
impl Surface {
    fn bottom(&self) -> usize {
        usize::from(self.height.saturating_sub(5).max(1))
    }
    fn status_row(&self) -> usize {
        usize::from(self.height.saturating_sub(4).max(1))
    }
    fn separator_row(&self) -> usize {
        usize::from(self.height.saturating_sub(3).max(1))
    }
    fn geometry(&mut self) -> io::Result<()> {
        if self.overlay {
            return Ok(());
        }
        let (width, height) = size();
        if (width, height) != (self.width, self.height) {
            self.width = width;
            self.height = height;
            self.repaint()?;
        }
        Ok(())
    }
    fn repaint(&mut self) -> io::Result<()> {
        self.x = 0;
        self.y = 1;
        let mut out = theme::TerminalWriter::new(io::stdout());
        write!(out, "\x1b[?2026h\x1b[r")?;
        for row in 1..=self.height {
            write!(out, "\x1b[{row};1H\x1b[2K")?;
        }
        write!(out, "\x1b[1;{}r", self.bottom())?;
        out.flush()?;
        self.repainting = true;
        let chunks: Vec<_> = self.body.iter().cloned().collect();
        let result = if self.reading {
            Ok(())
        } else {
            chunks.iter().try_for_each(|chunk| self.emit(chunk))
        };
        self.repainting = false;
        result?;
        self.chrome(&mut out)?;
        if self.reading {
            self.reader_invalidated = true;
        }
        write!(out, "\x1b[?2026l")?;
        out.flush()
    }
    fn remember(&mut self, text: &str) {
        for chunk in frames(text) {
            self.body_bytes += chunk.len();
            self.body.push_back(chunk.to_owned());
        }
        while self.body_bytes > 1_048_576 {
            if let Some(old) = self.body.pop_front() {
                self.body_bytes -= old.len();
            } else {
                break;
            }
        }
    }
    fn chrome(&self, out: &mut impl Write) -> io::Result<()> {
        let row = self.separator_row();
        write!(
            out,
            "\x1b[{row};1H\x1b[2K{}{}\x1b[0m",
            theme::ansi(if self.todo.tasks.is_empty() || self.todo.hidden {Tone::Border}else{Tone::Accent}),
            if self.todo.tasks.is_empty() || self.todo.hidden { separator_line(self.width) } else {
                let active = self.todo.tasks.iter().find(|t|t.status == crate::todo::Status::InProgress);
                let line = if self.width >= 40 { format!("{}{}",self.todo.summary(),active.map(|t|format!(" · ◆ {}",t.title)).unwrap_or_default()) } else {self.todo.compact_summary(usize::from(self.width.saturating_sub(1)))};
                glass_tui::text::clip(&glass_tui::markdown::sanitize(&line),usize::from(self.width.saturating_sub(1)))
            }
        )?;
        self.status_into(out)
    }
    fn status_into(&self, out: &mut impl Write) -> io::Result<()> {
        let row = self.status_row();
        let text = glass_tui::text::clip(&self.status, usize::from(self.width.saturating_sub(1)));
        write!(
            out,
            "\x1b[{row};1H\x1b[2K{}{}\x1b[0m",
            self.status_color, text
        )
    }
    fn emit(&mut self, text: &str) -> io::Result<()> {
        if self.overlay {
            // The ordinary body cache already retains the same bounded tail used
            // for repaint. Keep it current without drawing or spooling to disk.
            self.remember(text);
            return Ok(());
        }
        self.geometry()?;
        if !self.repainting {
            self.remember(text);
        }
        if self.reading {
            return Ok(());
        }
        let mut out = theme::TerminalWriter::new(io::stdout());
        // CUP cannot represent a terminal's pending autowrap column. Resolve it
        // before relocating the cursor, without duplicating a following CR/LF.
        if self.x >= usize::from(self.width) {
            let first = glass_tui::markdown::sanitize(text).chars().next();
            if !matches!(first, Some('\r' | '\n')) {
                write!(out, "\x1b[s\x1b[{};{}H\r\n\x1b[u", self.y, self.width)?;
                self.track("\r\n");
            } else {
                self.x = usize::from(self.width.saturating_sub(1));
            }
        }
        write!(out, "\x1b[s\x1b[{};{}H{}\x1b[u", self.y, self.x + 1, text)?;
        self.track(text);
        out.flush()
    }
    fn track(&mut self, text: &str) {
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                match chars.next() {
                    Some('[') => {
                        for c in chars.by_ref() {
                            if ('@'..='~').contains(&c) {
                                break;
                            }
                        }
                    }
                    Some(']') => {
                        let mut escape = false;
                        for c in chars.by_ref() {
                            if c == '\x07' || (escape && c == '\\') {
                                break;
                            }
                            escape = c == '\x1b';
                        }
                    }
                    _ => {}
                }
            } else if c == '\r' {
                self.x = 0;
            } else if c == '\n' {
                self.x = 0;
                self.y = (self.y + 1).min(self.bottom());
            } else if !c.is_control() {
                let cells = glass_tui::text::char_width(c);
                if self.x + cells > usize::from(self.width) {
                    self.x = 0;
                    self.y = (self.y + 1).min(self.bottom());
                }
                self.x += cells;
            }
        }
    }
    fn overlay(&mut self, active: bool) -> io::Result<()> {
        self.overlay = active;
        if !active {
            self.geometry()?;
            write!(io::stdout(), "\x1b[1;{}r", self.bottom())?;
            self.repaint()?;
        }
        Ok(())
    }
}
pub struct Guard;
pub fn enter() -> io::Result<Guard> {
    let (width, height) = size();
    let surface = Surface {
        width,
        height,
        x: 0,
        y: 1,
        overlay: false,
        status: String::new(),
        todo: crate::todo::Board::default(),
        status_color: theme::ansi(Tone::Accent),
        body: std::collections::VecDeque::new(),
        body_bytes: 0,
        repainting: false,
        reading: false,
        cursor: (height.saturating_sub(2).max(1), 3.min(width)),
        reader_invalidated: false,
    };
    let mut out = theme::TerminalWriter::new(io::stdout());
    // One alternate surface for the lifetime of the interactive application.
    // The shell's primary screen and scrollback remain untouched.
    write!(
        out,
        "\x1b[?1049h\x1b[r\x1b[2J\x1b[H\x1b[1;{}r",
        surface.bottom()
    )?;
    surface.chrome(&mut out)?;
    out.flush()?;
    PRIMARY.with(|slot| *slot.borrow_mut() = Some(surface));
    Ok(Guard)
}
impl Drop for Guard {
    fn drop(&mut self) {
        PRIMARY.with(|slot| {
            if let Some(mut state) = slot.borrow_mut().take() {
                let _ = state.overlay(false);
                let mut out = theme::TerminalWriter::new(io::stdout());
                let _ = write!(out, "\x1b[r\x1b[?1049l\x1b[?25h");
                let _ = out.flush();
            }
        });
    }
}
pub fn emit(text: &str) -> io::Result<()> {
    PRIMARY.with(|slot| match slot.borrow_mut().as_mut() {
        Some(state) => state.emit(text),
        None => {
            let mut out = theme::TerminalWriter::new(io::stdout());
            write!(out, "{text}")?;
            out.flush()
        }
    })
}
pub fn checklist(board: &crate::todo::Board) -> io::Result<()> {
    PRIMARY.with(|slot| {
        let mut slot=slot.borrow_mut();
        let Some(state)=slot.as_mut() else {return Ok(());};
        if state.todo.tasks == board.tasks && state.todo.hidden == board.hidden {return Ok(());}
        state.todo=board.clone();
        if state.overlay {return Ok(());}
        state.geometry()?;
        let mut out=theme::TerminalWriter::new(io::stdout());
        write!(out,"\x1b[s")?;state.chrome(&mut out)?;write!(out,"\x1b[u")?;out.flush()
    })
}
pub fn status(text: &str) -> io::Result<()> {
    status_colored(text, &theme::ansi(Tone::Accent))
}
pub fn status_colored(text: &str, color: &str) -> io::Result<()> {
    PRIMARY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else {
            return Ok(());
        };
        state.status = glass_tui::markdown::sanitize(text);
        state.status_color = color.to_owned();
        if state.overlay {
            return Ok(());
        }
        state.geometry()?;
        let mut out = theme::TerminalWriter::new(io::stdout());
        write!(out, "\x1b[s")?;
        state.status_into(&mut out)?;
        write!(out, "\x1b[u")?;
        out.flush()
    })
}
pub fn overlay(active: bool) -> io::Result<()> {
    PRIMARY.with(|slot| match slot.borrow_mut().as_mut() {
        Some(state) => state.overlay(active),
        None => Ok(()),
    })
}
pub fn composer(editor: &Editor, focused: bool, busy: bool) -> u16 {
    PRIMARY.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(state) = slot.as_mut() {
            if state.overlay {
                return 1;
            }
            let _ = state.geometry();
            let row = state.height.saturating_sub(2).max(1);
            let rows = if busy { 1 } else { 5 };
            state.cursor = editor.cursor_cell(row, state.width, rows);
            if busy {
                editor.render_limited(row, state.width, focused, 1)
            } else {
                editor.render(row, state.width, focused)
            }
        } else {
            let (w, h) = size();
            editor.render(h.saturating_sub(2).max(1), w, focused)
        }
    })
}

/// Paint an empty composer immediately after an idle submission. Body output is
/// recorded in the scrollable region and cannot erase the composer rows.
pub fn clear_composer() {
    PRIMARY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else {
            return;
        };
        if state.overlay {
            return;
        }
        let _ = state.geometry();
        let first_row = state.height.saturating_sub(2).max(1);
        let mut out = theme::TerminalWriter::new(io::stdout());
        for row in composer_rows(state.height) {
            let _ = out.write_all(clear_row_sequence(row).as_bytes());
        }
        let _ = out.flush();
        Editor::new().render_limited(first_row, state.width, true, 1);
        state.cursor = (first_row, 3.min(state.width));
    });
}

/// Restore the insertion cursor after a fixed-viewport transcript redraw.
pub fn restore_composer_cursor() {
    PRIMARY.with(|slot| {
        if let Some(state) = slot.borrow().as_ref() {
            let mut out = theme::TerminalWriter::new(io::stdout());
            let _ = write!(out, "\x1b[{};{}H\x1b[?25h", state.cursor.0, state.cursor.1);
            let _ = out.flush();
        }
    });
}

pub fn reader_drawn() {
    PRIMARY.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            state.reader_invalidated = false;
        }
    });
}

/// Clear only the app-owned body rectangle before restoring a reader frame
/// after an overlay has painted over it.
pub fn clear_body_rect() -> io::Result<()> {
    let (_, _, _, height) = body_rect();
    let mut stdout = io::stdout();
    for row in 0..height {
        write!(stdout, "\x1b[{};1H\x1b[2K", row.saturating_add(1))?;
    }
    stdout.flush()
}

pub fn reader_invalidated() -> bool {
    PRIMARY.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|state| state.reader_invalidated)
    })
}

fn clear_row_sequence(row: u16) -> String {
    format!("\x1b[{row};1H\x1b[2K")
}

fn separator_line(width: u16) -> String {
    "─".repeat(usize::from(width.saturating_sub(1)))
}

fn composer_rows(height: u16) -> Vec<u16> {
    (height.saturating_sub(2).max(1)..=height.max(1)).collect()
}

pub struct BodyWriter;
impl Write for BodyWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        emit(text)?;
        Ok(bytes.len())
    }
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> io::Result<()> {
        emit(&format!("{args}"))
    }
    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()
    }
}

/// Split at complete UTF-8 and escape tokens; presentation emits only short CSI.
fn frames(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut escape = false;
    for (i, c) in text.char_indices() {
        if c == '\x1b' {
            escape = true;
        } else if escape && ('@'..='~').contains(&c) && c != '[' {
            escape = false;
        }
        let end = i + c.len_utf8();
        if end - start >= 4096 && !escape {
            result.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        result.push(&text[start..]);
    }
    result
}

pub fn managed() -> bool {
    PRIMARY.with(|slot| slot.borrow().is_some())
}

/// Toggle the inline transcript while preserving body writes in the surface
/// cache. Leaving reader mode repaints the latest cached body once.
pub fn reading(active: bool) -> io::Result<()> {
    PRIMARY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else {
            return Ok(());
        };
        if state.reading == active {
            return Ok(());
        }
        state.reading = active;
        if !active {
            state.repaint()?;
        }
        Ok(())
    })
}

/// Rect of the scrollable body in terminal coordinates (zero based).
pub fn body_rect() -> (u16, u16, u16, u16) {
    PRIMARY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else {
            let (w, h) = size();
            return (0, 0, w, h.saturating_sub(5).max(1));
        };
        let _ = state.geometry();
        (0, 0, state.width, state.bottom().max(1) as u16)
    })
}

pub fn suspend() -> io::Result<()> {
    if managed() {
        write!(io::stdout(), "\x1b[r\x1b[?1049l\x1b[?25h")?;
        io::stdout().flush()?;
    }
    Ok(())
}
pub fn resume() -> io::Result<()> {
    PRIMARY.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(state) = slot.as_mut() {
            write!(io::stdout(), "\x1b[?1049h")?;
            let (w, h) = size();
            state.width = w;
            state.height = h;
            state.repaint()?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composer_clear_targets_only_its_owned_row() {
        assert_eq!(clear_row_sequence(22), "\x1b[22;1H\x1b[2K");
    }

    #[test]
    fn composer_clear_region_includes_wrapped_input_rows() {
        assert_eq!(composer_rows(24), vec![22, 23, 24]);
        assert_eq!(composer_rows(1), vec![1]);
    }

    #[test]
    fn separator_fills_the_available_width_without_wrapping() {
        for width in [20, 24, 40, 65] {
            let line = separator_line(width);
            assert_eq!(line.chars().count(), usize::from(width - 1));
            assert_eq!(line.chars().last(), Some('─'));
        }
        assert!(separator_line(1).is_empty());
    }

    #[test]
    fn cached_frames_bound_long_unicode_and_keep_escape_tokens() {
        let source = format!(
            "{}\x1b[38;5;252m{}\x1b[0m",
            "🙂".repeat(5000),
            "word ".repeat(5000)
        );
        let chunks = frames(&source);
        assert_eq!(chunks.concat(), source);
        assert!(chunks.iter().all(|c| c.len() < 8192));
        assert!(chunks.iter().all(|c| !c.ends_with('\x1b')));
    }
}

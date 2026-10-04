//! Terminal setup, raw mode, and a bounded differential backbuffer.
//!
//! The default glass view deliberately does not enter alt-screen. History
//! belongs to the terminal, which already scrolls, searches, and selects for
//! free. What we keep in memory is only enough to diff the next frame against
//! the last one, and that buffer is hard-capped.

use std::io::{self, IsTerminal, Write};

/// Bounded number of rendered lines retained for differential redraw.
///
/// pi publishes "a few hundred kilobytes" for the same idea. We cap by line
/// count because that is the thing that actually grows without bound.
const BACKBUFFER_MAX_LINES: usize = 512;

pub fn stdout_is_tty() -> bool {
    io::stdout().is_terminal()
}

pub fn stdin_is_tty() -> bool {
    io::stdin().is_terminal()
}

/// Put the terminal into raw mode with alternate screen disabled.
///
/// Alternate screen stays off on purpose: it is the single change that makes
/// the harness feel native, and it is what keeps history memory-free.
pub fn enter() -> io::Result<()> {
    use crossterm::event::EnableBracketedPaste;
    use crossterm::{execute, terminal};
    execute!(io::stdout(), EnableBracketedPaste).ok();
    terminal::enable_raw_mode()?;
    // Clear scrollback so a fresh session starts at a known point.
    execute!(io::stdout(), terminal::Clear(terminal::ClearType::All), terminal::Clear(terminal::ClearType::Purge)).ok();
    Ok(())
}

/// Leave the terminal exactly as we found it. Must run on every exit path.
pub fn leave() -> io::Result<()> {
    use crossterm::event::DisableBracketedPaste;
    use crossterm::{execute, terminal};
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), crossterm::cursor::Show);
    Ok(())
}

/// Synchronized output: batch a frame so the terminal paints it atomically.
///
/// Without this, a burst of writes during token streaming shows up as
/// visible tearing. Support is universal but harmlessly ignored elsewhere.
pub fn begin_sync() {
    print!("\x1b[?2026h");
}

pub fn end_sync() {
    print!("\x1b[?2026l");
    let _ = io::stdout().flush();
}

pub fn move_to(row: u16, col: u16) {
    print!("\x1b[{row};{col}H");
}

/// A bounded ring of previously-rendered lines.
///
/// Redraw strategy is "find the first line that differs, repaint from there",
/// which turns a full frame into a handful of byte writes when nothing moves.
pub struct BackBuffer {
    lines: Vec<String>,
    /// Index of the logical line currently at the top of the terminal viewport.
    scroll: usize,
}

impl BackBuffer {
    pub fn new() -> Self {
        Self { lines: Vec::new(), scroll: 0 }
    }

    /// Append a logical line, evicting from the front when over budget.
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
        if self.lines.len() > BACKBUFFER_MAX_LINES {
            let excess = self.lines.len() - BACKBUFFER_MAX_LINES;
            self.lines.drain(0..excess);
            self.scroll = self.scroll.saturating_sub(excess);
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Repaint from the first differing line. Returns how many lines changed.
    pub fn diff_into(&self, next: &[String]) -> usize {
        let common = self
            .lines
            .iter()
            .zip(next.iter())
            .take_while(|(a, b)| a == b)
            .count();

        if common == self.lines.len() && common == next.len() {
            return 0;
        }

        // Repaint the changed tail, clearing each line so stale styling from
        // the previous frame cannot bleed through.
        let tail = next.len().saturating_sub(common);
        begin_sync();
        for (i, line) in next.iter().enumerate().skip(common) {
            move_to(i as u16, 0);
            print!("\x1b[2K{line}");
        }
        end_sync();
        tail
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }
}

impl Default for BackBuffer {
    fn default() -> Self {
        Self::new()
    }
}

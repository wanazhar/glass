//! Terminal setup, raw mode, and a bounded differential backbuffer.
//!
//! Raw input leaves shell history untouched. The interactive application owns
//! its alternate-screen lifecycle; this module handles terminal input modes.

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

/// Put the terminal into raw mode without clearing shell output or scrollback.
pub fn enter() -> io::Result<()> {
    use crossterm::event::EnableBracketedPaste;
    use crossterm::execute;
    execute!(io::stdout(), EnableBracketedPaste).ok();
    enable_raw_input()?;
    Ok(())
}

/// Raw keyboard input with normal newline output for the scrollback renderer.
/// Crossterm clears OPOST; without restoring it, LF leaves the cursor in the
/// previous column and streamed prose walks diagonally across SSH terminals.
pub fn enable_raw_input() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    #[cfg(unix)]
    {
        let result = (|| {
            let mut attrs = std::mem::MaybeUninit::<libc::termios>::uninit();
            // SAFETY: stdout is a terminal; tcgetattr initializes attrs on success.
            if unsafe { libc::tcgetattr(libc::STDOUT_FILENO, attrs.as_mut_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut attrs = unsafe { attrs.assume_init() };
            attrs.c_oflag |= libc::OPOST | libc::ONLCR;
            attrs.c_oflag &= !(libc::OCRNL | libc::ONOCR | libc::ONLRET);
            // SAFETY: attrs is an initialized termios value for this terminal.
            if unsafe { libc::tcsetattr(libc::STDOUT_FILENO, libc::TCSANOW, &attrs) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        result?;
    }
    let _ = crossterm::execute!(io::stdout(), crossterm::event::EnableMouseCapture);
    Ok(())
}

/// Leave the terminal exactly as we found it. Must run on every exit path.
pub fn leave() -> io::Result<()> {
    use crossterm::event::{DisableBracketedPaste, DisableMouseCapture};
    use crossterm::{execute, terminal};
    let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
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
        Self {
            lines: Vec::new(),
            scroll: 0,
        }
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

//! Inline conversation reader for the owned chat body region.
use glass_tui::{
    Entry, Transcript,
    crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind},
    ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect},
    theme::TerminalWriter,
};
use std::io;

type Tui = Terminal<CrosstermBackend<TerminalWriter<io::Stdout>>>;

pub enum Action {
    Handled,
    Close,
    CloseAndPass,
    PassThrough,
}

pub struct Reader {
    terminal: Tui,
    transcript: Transcript,
    unread: usize,
}

impl Reader {
    pub fn new(entries: Vec<Entry>, page_up: bool) -> io::Result<Self> {
        let rect = body_rect();
        let terminal = make_clean_terminal(rect)?;
        let mut transcript = Transcript::new(entries);
        transcript.set_width(rect.width);
        transcript.follow_tail(rect.height);
        if page_up {
            transcript.page(-1, rect.height);
        }
        let mut reader = Self {
            terminal,
            transcript,
            unread: 0,
        };
        reader.draw()?;
        Ok(reader)
    }

    pub fn update(&mut self, entries: Vec<Entry>) -> io::Result<()> {
        let following = self.transcript.follow;
        if !self.transcript.replace_entries(entries) {
            return Ok(());
        }
        if following {
            self.unread = 0;
        } else {
            self.unread = self.unread.saturating_add(1);
        }
        self.draw()
    }

    /// Repaint after an overlay has written directly to the same body rows.
    pub fn restore_after_overlay(&mut self) -> io::Result<()> {
        self.terminal = make_clean_terminal(body_rect())?;
        self.draw()
    }

    pub fn resize(&mut self) -> io::Result<()> {
        self.terminal = make_clean_terminal(body_rect())?;
        self.draw()
    }

    pub fn event(&mut self, event: &Event) -> io::Result<Action> {
        let area = body_rect();
        let draw;
        match event {
            Event::Resize(_, _) => {
                draw = true;
            }
            Event::Key(key) => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc | KeyCode::End => return Ok(Action::Close),
                    KeyCode::Char('g' | 't') if ctrl => return Ok(Action::CloseAndPass),
                    KeyCode::Home if ctrl => self.transcript.top(),
                    KeyCode::PageUp => self.transcript.step(-(area.height as isize), area.height),
                    KeyCode::PageDown => self.transcript.step(area.height as isize, area.height),
                    KeyCode::Char('b') if ctrl => {
                        self.transcript.step(-(area.height as isize), area.height)
                    }
                    KeyCode::Char('f') if ctrl => {
                        self.transcript.step(area.height as isize, area.height)
                    }
                    _ => return Ok(Action::PassThrough),
                }
                draw = true;
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.transcript.step(-3, area.height);
                    draw = true;
                }
                MouseEventKind::ScrollDown => {
                    self.transcript.step(3, area.height);
                    draw = true;
                }
                _ => return Ok(Action::Handled),
            },
            Event::Paste(_) => return Ok(Action::PassThrough),
            _ => return Ok(Action::Handled),
        }
        if draw {
            self.draw()?;
        }
        Ok(Action::Handled)
    }

    fn draw(&mut self) -> io::Result<()> {
        let hint = if self.transcript.follow {
            "PgUp/Ctrl+B ↑ · PgDn/Ctrl+F ↓ · End/Esc bottom · Wheel".to_string()
        } else if self.unread == 0 {
            "End / Esc back to bottom".to_string()
        } else {
            format!("End / Esc bottom · {} new updates", self.unread)
        };
        self.transcript.set_inline_hint(Some(hint));
        self.terminal.draw(|frame| {
            self.transcript.draw(frame, &[], "Conversation");
        })?;
        crate::surface::restore_composer_cursor();
        crate::surface::reader_drawn();
        Ok(())
    }
}

fn body_rect() -> Rect {
    let (x, y, width, height) = crate::surface::body_rect();
    Rect::new(x, y, width.max(1), height.max(1))
}

fn make_terminal(rect: Rect) -> io::Result<Tui> {
    Terminal::with_options(
        CrosstermBackend::new(TerminalWriter(io::stdout())),
        TerminalOptions {
            viewport: Viewport::Fixed(rect),
        },
    )
}

fn make_clean_terminal(rect: Rect) -> io::Result<Tui> {
    crate::surface::clear_body_rect()?;
    make_terminal(rect)
}

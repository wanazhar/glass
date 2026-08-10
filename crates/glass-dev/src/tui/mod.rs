//! Decomposed full Glass Dev terminal application.

mod command;
mod render;
mod state;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use glass_browser::cli::args::TuiLayout;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::time::{Duration, Instant};

pub use state::{DevSurface, DevTuiState, ResponsiveClass};

pub fn run(root: impl AsRef<Path>, layout: TuiLayout) -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Glass Dev TUI requires an interactive terminal; use a CLI subcommand or --mcp for non-interactive use".into());
    }
    let mut state = DevTuiState::open(root, layout)?;
    let mut guard = TerminalGuard::enter()?;
    let mut last_refresh = Instant::now();
    loop {
        guard.terminal.draw(|frame| render::render(frame, &state))?;
        if state.quit {
            break;
        }
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if state.command_mode {
                match key.code {
                    KeyCode::Esc => state.close_palette(),
                    KeyCode::Enter => state.submit_palette(),
                    KeyCode::Backspace => {
                        state.command_input.pop();
                    }
                    KeyCode::Char(character) => state.command_input.push(character),
                    _ => {}
                }
            } else {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), _) => {
                        state.quit = true;
                    }
                    (KeyCode::Char(':'), _) => state.open_palette(),
                    (KeyCode::Up, _) | (KeyCode::Char('k'), _) => state.previous_surface(),
                    (KeyCode::Down, _) | (KeyCode::Char('j'), _) => state.next_surface(),
                    (KeyCode::Char(character), _) => state.handle_printable(character),
                    _ => {}
                }
            }
        }
        if last_refresh.elapsed() >= Duration::from_millis(250) {
            state.refresh();
            last_refresh = Instant::now();
        }
    }
    Ok(())
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

//! Decomposed full Glass Dev terminal application.

mod command;
mod render;
mod state;

use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use glass_browser::browser_workspace::BrowserWorkspaceIntent;
use glass_browser::cli::args::TuiLayout;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::time::{Duration, Instant};

pub use state::{DevSurface, DevTuiState, ProductMode, ResponsiveClass};

pub fn run(root: impl AsRef<Path>, layout: TuiLayout) -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Glass Dev TUI requires an interactive terminal; use a CLI subcommand or --mcp for non-interactive use".into());
    }
    let mut state = DevTuiState::open(root, layout)?;
    let mut guard = TerminalGuard::enter()?;
    let mut last_refresh = Instant::now();
    loop {
        let size = guard.terminal.size()?;
        state.set_terminal_size(size.width, size.height);
        guard.terminal.draw(|frame| render::render(frame, &state))?;
        if state.quit {
            break;
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if state.pending_confirmation.is_some() {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                                state.approve_confirmation();
                            }
                            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                                state.deny_confirmation();
                            }
                            _ => {}
                        }
                    } else if state.code_edit_mode {
                        match key.code {
                            KeyCode::Esc => state.close_code_edit(),
                            _ => state.edit_code_key(key.code, key.modifiers),
                        }
                    } else if state.composer_mode {
                        match key.code {
                            KeyCode::Esc => state.close_composer(),
                            KeyCode::Enter => state.submit_composer(),
                            KeyCode::Backspace => state.composer_backspace(),
                            KeyCode::Char(character) => {
                                state.insert_composer_text(&character.to_string());
                            }
                            _ => {}
                        }
                    } else if state.command_mode {
                        match key.code {
                            KeyCode::Esc => state.close_palette(),
                            KeyCode::Enter => state.submit_palette(),
                            KeyCode::Backspace => state.palette_backspace(),
                            KeyCode::Left => state.move_palette_cursor(false),
                            KeyCode::Right => state.move_palette_cursor(true),
                            KeyCode::Up => state.navigate_palette_history(true),
                            KeyCode::Down => state.navigate_palette_history(false),
                            KeyCode::Tab => state.complete_palette(),
                            KeyCode::Char(character) => state.insert_palette_char(character),
                            _ => {}
                        }
                    } else {
                        match (key.code, key.modifiers) {
                            (KeyCode::Char('c'), KeyModifiers::CONTROL)
                            | (KeyCode::Char('q'), _) => {
                                state.quit = true;
                            }
                            (KeyCode::Char(':'), _) => state.open_palette(),
                            (KeyCode::Char('H'), _) if state.surface == DevSurface::App => {
                                let _ = state
                                    .browser_workspace
                                    .reduce(BrowserWorkspaceIntent::TakeHumanControl);
                                state.browser = state.browser_workspace_summary();
                                state.status =
                                    "Human browser control acquired · agent mutation paused".into();
                            }
                            (KeyCode::Char('G'), _) if state.surface == DevSurface::App => {
                                state.browser_workspace.reconcile_takeover();
                                state.browser = state.browser_workspace_summary();
                                state.status =
                                    "Browser checkpoint reconciled · control returned to Glass"
                                        .into();
                            }
                            (KeyCode::Char('i'), _) if state.surface == DevSurface::Agent => {
                                state.open_composer();
                            }
                            (KeyCode::Char('i'), _) if state.surface == DevSurface::Code => {
                                state.enter_code_edit();
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::Code => {
                                state.open_selected_file();
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::App => {
                                state.execute_app_intent(BrowserWorkspaceIntent::ActivateSelected)
                            }
                            (KeyCode::PageUp, _) if state.surface == DevSurface::App => state
                                .execute_app_intent(BrowserWorkspaceIntent::ScrollBrowser {
                                    dx: 0.0,
                                    dy: -600.0,
                                }),
                            (KeyCode::PageDown, _) if state.surface == DevSurface::App => state
                                .execute_app_intent(BrowserWorkspaceIntent::ScrollBrowser {
                                    dx: 0.0,
                                    dy: 600.0,
                                }),
                            (KeyCode::Char('n'), _) if state.surface == DevSurface::App => {
                                state.open_palette_with("browser navigate ")
                            }
                            (KeyCode::Char('t'), _) if state.surface == DevSurface::App => {
                                state.open_palette_with("browser type ")
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _)
                                if state.surface == DevSurface::Code =>
                            {
                                state.move_file_selection(-1)
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::Code =>
                            {
                                state.move_file_selection(1)
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _)
                                if state.surface == DevSurface::App =>
                            {
                                let _ = state
                                    .browser_workspace
                                    .reduce(BrowserWorkspaceIntent::MoveSelection { delta: -1 });
                                state.browser = state.browser_workspace_summary();
                                state.highlight_app_selection();
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::App =>
                            {
                                let _ = state
                                    .browser_workspace
                                    .reduce(BrowserWorkspaceIntent::MoveSelection { delta: 1 });
                                state.browser = state.browser_workspace_summary();
                                state.highlight_app_selection();
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => state.scroll_surface(-1),
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => state.scroll_surface(1),
                            (KeyCode::Tab, modifiers)
                                if modifiers.contains(KeyModifiers::SHIFT) =>
                            {
                                state.previous_surface()
                            }
                            (KeyCode::Tab, _) => state.next_surface(),
                            (KeyCode::Char(character), _) => state.handle_printable(character),
                            _ => {}
                        }
                    }
                }
                Event::Paste(text) if state.command_mode => state.insert_palette_text(&text),
                Event::Paste(text) if state.composer_mode => state.insert_composer_text(&text),
                Event::Mouse(mouse) if state.surface == DevSurface::App => {
                    let delta = match mouse.kind {
                        crossterm::event::MouseEventKind::ScrollUp => Some(-1),
                        crossterm::event::MouseEventKind::ScrollDown => Some(1),
                        _ => None,
                    };
                    if let Some(delta) = delta {
                        let _ = state
                            .browser_workspace
                            .reduce(BrowserWorkspaceIntent::MoveSelection { delta });
                        state.browser = state.browser_workspace_summary();
                        state.highlight_app_selection();
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollUp => state.scroll_surface(-1),
                    crossterm::event::MouseEventKind::ScrollDown => state.scroll_surface(1),
                    _ => {}
                },
                Event::FocusLost => {
                    let _ = state
                        .browser_workspace
                        .reduce(BrowserWorkspaceIntent::CloseOverlay);
                }
                Event::Resize(width, height) => state.set_terminal_size(width, height),
                Event::Key(_) | Event::Paste(_) | Event::FocusGained => {}
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
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableFocusChange,
            EnableBracketedPaste
        )?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            DisableFocusChange,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

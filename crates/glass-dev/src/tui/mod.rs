//! Decomposed full Glass Dev terminal application.

mod command;
mod projection;
pub mod render;
mod snapshot;
pub mod state;

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
    let mut worker = snapshot::SnapshotWorker::spawn(&state);
    worker.request_refresh();
    let mut guard = TerminalGuard::enter()?;
    let mut last_refresh = Instant::now();
    let mut last_render = Instant::now();
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
                    // The terminal's strongest reflex works in every mode.
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        state.quit = true;
                    } else if state.menu_open {
                        match key.code {
                            KeyCode::Esc => state.close_menu(),
                            KeyCode::Enter => state.run_menu_action(),
                            KeyCode::Up | KeyCode::Char('k') => state.move_menu_selection(-1),
                            KeyCode::Down | KeyCode::Char('j') => state.move_menu_selection(1),
                            _ => {}
                        }
                    } else if state.browser_recovery.is_some() && state.surface == DevSurface::App {
                        match key.code {
                            KeyCode::Esc => {
                                state.browser_recovery = None;
                                state.status = "Recovery dismissed".into();
                            }
                            KeyCode::Char('1') => state.accept_browser_recovery(0),
                            KeyCode::Char('2') => state.accept_browser_recovery(1),
                            KeyCode::Char('3') => state.accept_browser_recovery(2),
                            _ => {}
                        }
                    } else if state.pending_confirmation.is_some() {
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
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_composer(),
                            (KeyCode::Enter, _) => state.submit_composer(),
                            (KeyCode::Backspace, _) => state.composer_backspace(),
                            (KeyCode::Char('u'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.composer_input.clear();
                                state.composer_cursor = 0;
                            }
                            (KeyCode::Char('a'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.composer_cursor = 0;
                            }
                            (KeyCode::Char('e'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.composer_cursor = state.composer_input.len();
                            }
                            (KeyCode::Char('w'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.delete_composer_word();
                            }
                            (KeyCode::Char('x'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.abort_selected_agent();
                            }
                            (KeyCode::Char('d'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.composer_steer = !state.composer_steer;
                                state.status = if state.composer_steer {
                                    "Steer mode · Enter interrupts the running agent".into()
                                } else {
                                    "Follow-up mode · Enter queues after the current turn".into()
                                };
                            }
                            (KeyCode::Left, _) => state.move_composer_cursor(false),
                            (KeyCode::Right, _) => state.move_composer_cursor(true),
                            (KeyCode::Home, _) => state.composer_cursor = 0,
                            (KeyCode::End, _) => state.composer_cursor = state.composer_input.len(),
                            (KeyCode::Char(character), _) => {
                                state.insert_composer_text(&character.to_string());
                            }
                            _ => {}
                        }
                    } else if state.command_mode {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_palette(),
                            (KeyCode::Enter, _) => state.submit_palette(),
                            (KeyCode::Backspace, _) => state.palette_backspace(),
                            (KeyCode::Char('u'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.command_input.clear();
                                state.command_cursor = 0;
                            }
                            (KeyCode::Left, _) => state.move_palette_cursor(false),
                            (KeyCode::Right, _) => state.move_palette_cursor(true),
                            (KeyCode::Up, _) => state.navigate_palette_history(true),
                            (KeyCode::Down, _) => state.navigate_palette_history(false),
                            (KeyCode::Tab, _) => state.complete_palette(),
                            (KeyCode::Char(character), _) => state.insert_palette_char(character),
                            _ => {}
                        }
                    } else {
                        match (key.code, key.modifiers) {
                            (KeyCode::Char('q'), _) => {
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
                            (KeyCode::Char('a'), _) if state.surface != DevSurface::App => {
                                state.open_menu()
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
                            (KeyCode::PageUp, _) => state.scroll_surface(-10),
                            (KeyCode::PageDown, _) => state.scroll_surface(10),
                            (KeyCode::Home, _) => state.scroll_home(),
                            (KeyCode::End, _) => state.scroll_end(),
                            (KeyCode::Char('v'), _) if state.surface == DevSurface::App => {
                                state.browser_visual_live = !state.browser_visual_live;
                                if state.browser_visual_live {
                                    state.refresh_app_visual(
                                        state.terminal_width / 3,
                                        state.terminal_height / 3,
                                    );
                                    state.status = if state.browser_pane.is_some() {
                                        "Live view on · ANSI half-block rendering · v stops".into()
                                    } else {
                                        state.browser_visual_live = false;
                                        "Live view unavailable · observe the browser first".into()
                                    };
                                } else {
                                    state.status = "Live view off".into();
                                }
                            }
                            (KeyCode::Left, modifiers)
                                if modifiers.contains(KeyModifiers::ALT)
                                    && state.surface == DevSurface::App =>
                            {
                                state.execute_app_intent(BrowserWorkspaceIntent::Back)
                            }
                            (KeyCode::Right, modifiers)
                                if modifiers.contains(KeyModifiers::ALT)
                                    && state.surface == DevSurface::App =>
                            {
                                state.execute_app_intent(BrowserWorkspaceIntent::Forward)
                            }
                            (KeyCode::Char('r'), modifiers)
                                if modifiers.contains(KeyModifiers::CONTROL)
                                    && state.surface == DevSurface::App =>
                            {
                                state.execute_app_intent(BrowserWorkspaceIntent::Reload)
                            }
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
                Event::Mouse(mouse) => match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollUp => state.scroll_surface(-3),
                    crossterm::event::MouseEventKind::ScrollDown => state.scroll_surface(3),
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        if let Some(surface) =
                            navigation_surface_at(state.terminal_width, mouse.column, mouse.row)
                        {
                            state.surface = surface;
                            state.status = format!("{} · : for actions", surface.label());
                        }
                    }
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
            worker.request_refresh();
            if state.browser_visual_live {
                state.refresh_app_visual(state.terminal_width / 3, state.terminal_height / 3);
            }
            last_refresh = Instant::now();
        } else if worker.is_busy() && last_render.elapsed() >= Duration::from_millis(100) {
            // Conversation tail keeps streaming while a full pass is in flight.
            worker.request_conversation();
            state.conversation_cursor = worker.conversation_cursor();
        }
        // Apply whatever the worker finished; never block on it.
        if let Some(snapshot) = worker.take_pending() {
            state.apply_snapshot(&snapshot);
        }
        // Redraw at most ~30fps so event polling stays responsive.
        if last_render.elapsed() >= Duration::from_millis(33) {
            last_render = Instant::now();
        }
    }
    drop(worker);
    Ok(())
}

/// Map a left-click on the desktop navigation column (columns 0..22) to the
/// surface it selects. Returns `None` for clicks elsewhere.
fn navigation_surface_at(width: u16, column: u16, row: u16) -> Option<DevSurface> {
    let (_nav_width, nav_start, list_start) = if width < 72 { (0, 0, 0) } else { (22, 2, 3) };
    if width < 72 || column >= _nav_width || row < list_start {
        return None;
    }
    // rows[1] list starts one row below the pane border; item height is 1.
    DevSurface::PRIMARY
        .into_iter()
        .nth((row - nav_start).checked_sub(1)? as usize)
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

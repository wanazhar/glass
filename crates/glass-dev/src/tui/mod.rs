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
use glass_browser::browser_workspace::{BrowserConnectionPhase, BrowserWorkspaceIntent};
use glass_browser::cli::args::{
    TuiLayout, TuiLiveBackend, TuiLiveFit, TuiLiveMode, TuiLiveQuality,
};
use glass_browser::tui::live_view::{
    VisualPath, decide_path, frame_fit, frame_interval_ms, pane_size, png_dimensions,
};
use glass_browser::tui::{HerdrEnvironment, HerdrEvent, HerdrFrame, HerdrGraphicsWorker};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::time::{Duration, Instant};

pub use state::{DevSurface, DevTuiState, ProductMode, ResponsiveClass};

#[derive(Debug, Clone, Copy)]
pub struct TuiVisualOptions {
    pub mode: TuiLiveMode,
    pub backend: TuiLiveBackend,
    pub quality: TuiLiveQuality,
    pub fit: TuiLiveFit,
}

struct VisualRuntime {
    path: VisualPath,
    live: bool,
    quality: TuiLiveQuality,
    fit: TuiLiveFit,
    herdr: Option<HerdrGraphicsWorker>,
}

impl VisualRuntime {
    fn new(options: TuiVisualOptions) -> Self {
        let herdr_available = HerdrEnvironment::from_process().is_some();
        let path = decide_path(options.mode, options.backend, herdr_available);
        let herdr = matches!(&path, VisualPath::Herdr)
            .then(|| HerdrEnvironment::from_process().map(HerdrGraphicsWorker::spawn))
            .flatten();
        let live = matches!(&path, VisualPath::Herdr | VisualPath::Ansi)
            && matches!(options.mode, TuiLiveMode::On | TuiLiveMode::Auto);
        Self {
            path,
            live,
            quality: options.quality,
            fit: options.fit,
            herdr,
        }
    }

    fn request_live(&mut self, live: bool) -> Option<String> {
        if !live {
            self.live = false;
            return None;
        }
        match &self.path {
            VisualPath::Herdr if self.herdr.is_some() => {
                self.live = true;
                None
            }
            VisualPath::Ansi => {
                self.live = true;
                None
            }
            VisualPath::SemanticOnly { reason } => {
                self.live = false;
                Some(reason.clone())
            }
            VisualPath::Herdr => {
                self.live = false;
                Some("Herdr pane graphics are unavailable in this terminal".into())
            }
        }
    }

    fn sync_state(&self, state: &mut DevTuiState) {
        state.browser_visual_live = self.live;
        let browser = state.browser_workspace.state_mut();
        if !self.live {
            browser.presentation =
                glass_browser::browser_workspace::BrowserPresentationPath::SemanticOnly;
            browser.presentation_reason = match &self.path {
                VisualPath::SemanticOnly { reason } => Some(reason.clone()),
                _ => {
                    Some("visual presentation is off; semantic inspection remains available".into())
                }
            };
            return;
        }
        match self.path {
            VisualPath::Herdr => {
                browser.presentation =
                    glass_browser::browser_workspace::BrowserPresentationPath::Herdr;
                browser.presentation_reason =
                    Some("waiting for the Herdr pane graphics stream".into());
            }
            VisualPath::Ansi => {
                browser.presentation =
                    glass_browser::browser_workspace::BrowserPresentationPath::Ansi;
                browser.presentation_reason =
                    Some("waiting for a bounded ANSI frame; semantic inspector stays live".into());
            }
            VisualPath::SemanticOnly { .. } => {}
        }
    }

    fn poll_events(&self) -> Vec<HerdrEvent> {
        let Some(herdr) = self.herdr.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        while let Some(event) = herdr.try_event() {
            events.push(event);
        }
        events
    }

    fn submit_herdr(&self, png: Vec<u8>, columns: u16, rows: u16) -> bool {
        let Some(herdr) = self.herdr.as_ref() else {
            return false;
        };
        let (image_width, image_height) = png_dimensions(&png).unwrap_or((1, 1));
        herdr.try_send(HerdrFrame {
            png,
            image_width,
            image_height,
            viewport_col: 0,
            viewport_row: 3,
            grid_cols: u32::from(columns),
            grid_rows: u32::from(rows.saturating_sub(6).max(1)),
        })
    }
}

pub fn run(
    root: impl AsRef<Path>,
    layout: TuiLayout,
    visual_options: TuiVisualOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Glass Dev TUI requires an interactive terminal; use a CLI subcommand or --mcp for non-interactive use".into());
    }
    let mut state = DevTuiState::open_for_tui(root, layout)?;
    let mut visual = VisualRuntime::new(visual_options);
    visual.sync_state(&mut state);
    let mut worker = snapshot::SnapshotWorker::spawn(&state);
    worker.request_refresh();
    let mut guard = TerminalGuard::enter()?;
    let mut last_refresh = Instant::now();
    let mut last_visual = Instant::now();
    let mut last_render = Instant::now() - Duration::from_millis(33);
    loop {
        let size = guard.terminal.size()?;
        state.set_terminal_size(size.width, size.height);
        if last_render.elapsed() >= Duration::from_millis(33) {
            guard.terminal.draw(|frame| render::render(frame, &state))?;
            last_render = Instant::now();
        }
        if state.quit {
            break;
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    // The quit modal is the strongest user-facing guard.
                    if state.quit_confirmation {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y' | 'Y') => state.confirm_quit(),
                            KeyCode::Esc | KeyCode::Char('n' | 'N') => state.cancel_quit(),
                            _ => {}
                        }
                    // Ctrl-C keeps its strongest reflex, but now asks first.
                    } else if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        state.request_quit();
                    } else if state.help_open {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('?') => state.toggle_help(),
                            KeyCode::Up | KeyCode::Char('k') => state.scroll_help(-1),
                            KeyCode::Down | KeyCode::Char('j') => state.scroll_help(1),
                            KeyCode::PageUp => state.scroll_help(-8),
                            KeyCode::PageDown => state.scroll_help(8),
                            KeyCode::Home => state.help_scroll = 0,
                            _ => {}
                        }
                    } else if key.code == KeyCode::Char('?')
                        && !state.composer_mode
                        && !state.command_mode
                        && !state.code_edit_mode
                    {
                        state.toggle_help();
                    } else if state.menu_open {
                        match key.code {
                            KeyCode::Esc => state.close_menu(),
                            KeyCode::Enter => {
                                let visual_requested = state.surface == DevSurface::App
                                    && state
                                        .surface_actions()
                                        .get(state.menu_selection)
                                        .is_some_and(|action| action.command == "v");
                                state.run_menu_action();
                                if visual_requested {
                                    if let Some(reason) = visual.request_live(true) {
                                        state.browser_visual_live = false;
                                        state.status = format!("Live view unavailable · {reason}");
                                    } else {
                                        visual.sync_state(&mut state);
                                        state.status = "Live view starting · v stops".into();
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => state.move_menu_selection(-1),
                            KeyCode::Down | KeyCode::Char('j') => state.move_menu_selection(1),
                            _ => {}
                        }
                    } else if state.browser_target_picker && state.surface == DevSurface::App {
                        match key.code {
                            KeyCode::Esc => state.close_browser_target_picker(),
                            KeyCode::Enter => state.select_browser_target(),
                            KeyCode::Up | KeyCode::Char('k') => {
                                state.move_browser_target_selection(-1)
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                state.move_browser_target_selection(1)
                            }
                            KeyCode::Backspace => state.browser_target_backspace(),
                            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.clear_browser_target_query()
                            }
                            KeyCode::Char(character) => {
                                state.insert_browser_target_query(character)
                            }
                            _ => {}
                        }
                    } else if state.browser_recovery.is_some() && state.surface == DevSurface::App {
                        match key.code {
                            KeyCode::Esc => {
                                state.browser_recovery = None;
                                state.status = "Recovery dismissed".into();
                            }
                            KeyCode::Char('1') => state.accept_browser_recovery(0, &mut worker),
                            KeyCode::Char('2') => state.accept_browser_recovery(1, &mut worker),
                            KeyCode::Char('3')
                                if state
                                    .browser_recovery
                                    .as_ref()
                                    .is_some_and(|offer| offer.compatible_endpoint) =>
                            {
                                state.accept_browser_recovery(2, &mut worker)
                            }
                            _ => {}
                        }
                    } else if state.git_diff_open && state.surface == DevSurface::Git {
                        match key.code {
                            KeyCode::Esc => state.close_git_diff(),
                            KeyCode::PageUp => state.scroll_surface(-10),
                            KeyCode::PageDown => state.scroll_surface(10),
                            KeyCode::Home => state.scroll_home(),
                            KeyCode::End => state.scroll_end(),
                            _ => {}
                        }
                    } else if state.pending_agent_approval.is_some() {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                                state.resolve_agent_approval(true, &mut worker);
                            }
                            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                                state.resolve_agent_approval(false, &mut worker);
                            }
                            _ => {}
                        }
                    } else if state.pending_confirmation.is_some() {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                                state.approve_confirmation_async(&mut worker);
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
                            (KeyCode::Enter, _) => state.submit_composer(&mut worker),
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
                                state.abort_selected_agent(&mut worker);
                            }
                            (KeyCode::Char('d'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.toggle_composer_steer();
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
                            (KeyCode::Enter, _) => state.submit_palette(&mut worker),
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
                            (KeyCode::Esc, _) if state.running_tool_job.is_some() => {
                                state.status =
                                    "Background operation is bounded · Ctrl-C opens quit confirmation"
                                        .into();
                            }
                            (KeyCode::Char('q'), _) => {
                                state.request_quit();
                            }
                            (KeyCode::Char('a'), _) => state.open_menu(),
                            (KeyCode::Char(':'), _) => state.open_palette(),
                            (KeyCode::Char('T'), _) if state.surface == DevSurface::App => {
                                state.queue_browser_targets(&mut worker)
                            }
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
                            (KeyCode::Char('s'), _) if state.surface == DevSurface::Agent => {
                                state.request_agent_setup();
                            }
                            (KeyCode::Char('u'), _) if state.surface == DevSurface::Agent => {
                                state.request_agent_update();
                            }
                            (KeyCode::Char('l'), _) if state.surface == DevSurface::Agent => {
                                let _ = state.request_agent_login();
                            }
                            (KeyCode::Char('s'), _) if state.surface == DevSurface::Terminal => {
                                state.request_detected_dev();
                            }
                            (KeyCode::Char('i'), _)
                                if state.surface == DevSurface::Agent
                                    && state.snapshot_trust_label == "untrusted" =>
                            {
                                state.surface = DevSurface::Trust;
                                state.status =
                                    "Trust this workspace before starting the Glass Agent · T or 1"
                                        .into();
                            }
                            (KeyCode::Char('i'), _)
                                if state.surface == DevSurface::Agent
                                    && !state.agent_readiness.starts_with("✓ Ready") =>
                            {
                                state.status =
                                    "Pi is not ready · press s to install, u to refresh, or l to sign in"
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
                            (KeyCode::Char('d'), _) if state.surface == DevSurface::Git => {
                                state.queue_git_diff(&mut worker)
                            }
                            (KeyCode::Char(']'), _) if state.surface == DevSurface::Code => {
                                state.cycle_editor_buffer(1)
                            }
                            (KeyCode::Char('['), _) if state.surface == DevSurface::Code => {
                                state.cycle_editor_buffer(-1)
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::App => {
                                state.queue_browser_intent(BrowserWorkspaceIntent::ActivateSelected)
                            }
                            (KeyCode::PageUp, _) => state.scroll_surface(-10),
                            (KeyCode::PageDown, _) => state.scroll_surface(10),
                            (KeyCode::Home, _) => state.scroll_home(),
                            (KeyCode::End, _) => state.scroll_end(),
                            (KeyCode::Char('v'), _) if state.surface == DevSurface::App => {
                                if visual.live {
                                    visual.request_live(false);
                                    visual.sync_state(&mut state);
                                    state.status =
                                        "Live view off · semantic inspection remains available"
                                            .into();
                                } else if let Some(reason) = visual.request_live(true) {
                                    visual.sync_state(&mut state);
                                    state.status = format!("Live view unavailable · {reason}");
                                } else {
                                    visual.sync_state(&mut state);
                                    state.status = match &visual.path {
                                        VisualPath::Herdr => {
                                            "Live view on · Herdr pane graphics · v stops".into()
                                        }
                                        VisualPath::Ansi => {
                                            if state.browser_pane.is_some() {
                                                "Live view on · ANSI half-block rendering · v stops"
                                                    .into()
                                            } else {
                                                "Live view starting · screenshot worker will update the pane"
                                                    .into()
                                            }
                                        }
                                        VisualPath::SemanticOnly { .. } => {
                                            "Live view unavailable · semantic inspection remains available"
                                                .into()
                                        }
                                    };
                                }
                            }
                            (KeyCode::Left, modifiers)
                                if modifiers.contains(KeyModifiers::ALT)
                                    && state.surface == DevSurface::App =>
                            {
                                state.queue_browser_intent(BrowserWorkspaceIntent::Back)
                            }
                            (KeyCode::Right, modifiers)
                                if modifiers.contains(KeyModifiers::ALT)
                                    && state.surface == DevSurface::App =>
                            {
                                state.queue_browser_intent(BrowserWorkspaceIntent::Forward)
                            }
                            (KeyCode::Char('r'), modifiers)
                                if modifiers.contains(KeyModifiers::CONTROL)
                                    && state.surface == DevSurface::App =>
                            {
                                state.queue_browser_intent(BrowserWorkspaceIntent::Reload)
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
                                state.status =
                                    "Selection moved · Enter activates the selected entity".into();
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::App =>
                            {
                                let _ = state
                                    .browser_workspace
                                    .reduce(BrowserWorkspaceIntent::MoveSelection { delta: 1 });
                                state.browser = state.browser_workspace_summary();
                                state.status =
                                    "Selection moved · Enter activates the selected entity".into();
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
        if state.agent_login_requested {
            state.agent_login_requested = false;
            run_agent_login(&mut state, &mut guard);
            worker.request_refresh();
        }

        for event in visual.poll_events() {
            match event {
                HerdrEvent::Connected if visual.live => {
                    state.browser_workspace.state_mut().presentation =
                        glass_browser::browser_workspace::BrowserPresentationPath::Herdr;
                    state.browser_workspace.state_mut().presentation_reason =
                        Some("Herdr pane graphics stream connected".into());
                    state.status = "Live view ready · Herdr pane graphics".into();
                }
                HerdrEvent::Failed(reason) => {
                    visual.live = false;
                    visual.sync_state(&mut state);
                    state.browser_workspace.state_mut().presentation_reason =
                        Some(format!("Herdr graphics unavailable: {reason}"));
                    state.status = format!("Live view unavailable · Herdr: {reason}");
                }
                HerdrEvent::Stopped if visual.live => {
                    visual.live = false;
                    visual.sync_state(&mut state);
                    state.browser_workspace.state_mut().presentation_reason =
                        Some("Herdr pane graphics stream stopped".into());
                    state.status =
                        "Live view stopped · semantic inspection remains available".into();
                }
                HerdrEvent::Connected | HerdrEvent::Stopped => {}
            }
        }
        if state.git_diff_requested {
            state.git_diff_requested = false;
            state.queue_git_diff(&mut worker);
        }
        if last_refresh.elapsed() >= Duration::from_millis(250) {
            worker.request_refresh();
            last_refresh = Instant::now();
        } else if worker.is_busy() && last_render.elapsed() >= Duration::from_millis(100) {
            // Conversation tail keeps streaming while a full pass is in flight.
            worker.request_conversation();
            state.conversation_cursor = worker.conversation_cursor();
        }
        if state.browser_visual_live
            && visual.live
            && matches!(
                state.browser_workspace.state().connection,
                BrowserConnectionPhase::Connected
            )
            && last_visual.elapsed() >= Duration::from_millis(frame_interval_ms(visual.quality))
        {
            let available = (
                state.terminal_width.saturating_sub(42),
                state.terminal_height.saturating_sub(12),
            );
            let (columns, rows) = pane_size(visual.quality, available);
            worker.submit_screenshot(columns, rows);
            last_visual = Instant::now();
        }
        // Apply whatever the worker finished; never block on it.
        if let Ok(Some(result)) = worker.try_job_result() {
            state.apply_tool_job_result(result);
            state.queue_browser_observe(&mut worker);
        }
        if let Ok(Some(result)) = worker.try_visual_result() {
            match &visual.path {
                VisualPath::Herdr if visual.live => match visual_png(&result) {
                    Ok(png) => {
                        if visual.submit_herdr(png, result.columns, result.rows) {
                            let browser = state.browser_workspace.state_mut();
                            browser.presentation =
                                glass_browser::browser_workspace::BrowserPresentationPath::Herdr;
                            browser.frame_revision = browser.browser_revision;
                            browser.presentation_reason =
                                Some("Herdr pane graphics frame queued".into());
                            state.status = "Live view updated · Herdr pane".into();
                        }
                    }
                    Err(error) => {
                        visual.live = false;
                        visual.sync_state(&mut state);
                        state.browser_workspace.state_mut().presentation_reason =
                            Some(error.clone());
                        state.status = format!("Live view unavailable · {error}");
                    }
                },
                VisualPath::Ansi => {
                    state.apply_visual_job_result_with_fit(result, frame_fit(visual.fit));
                }
                VisualPath::SemanticOnly { .. } => {}
                VisualPath::Herdr => {}
            }
        }
        if let Some(snapshot) = worker.take_pending() {
            state.apply_snapshot(&snapshot);
        }
        // The render gate above bounds terminal writes at roughly 30fps.
    }
    drop(worker);
    Ok(())
}

/// Map a left-click on the desktop navigation column to the surface it selects.
/// Returns `None` for clicks outside the visible list.
fn navigation_surface_at(width: u16, column: u16, row: u16) -> Option<DevSurface> {
    let (nav_width, header_height): (u16, u16) = if width < 72 { (0, 0) } else { (22, 3) };
    let first_item_row = header_height.saturating_add(1);
    if width < 72 || column >= nav_width || row < first_item_row {
        return None;
    }
    DevSurface::PRIMARY
        .into_iter()
        .nth(usize::from(row - first_item_row))
}

fn visual_png(result: &snapshot::VisualJobResult) -> Result<Vec<u8>, String> {
    let value = result.result.as_ref().map_err(|error| error.clone())?;
    let encoded = value
        .get("base64")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "screenshot payload did not contain base64 PNG data".to_string())?;
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("screenshot payload was not valid base64: {error}"))
}

fn run_agent_login(state: &mut DevTuiState, guard: &mut TerminalGuard) {
    if let Err(error) = guard.suspend() {
        state.status = format!("Could not hand the terminal to Pi: {error}");
        return;
    }
    let result = crate::pi_runtime::setup_pi_runtime(None, None, false, true);
    let resume = guard.resume();
    match (result, resume) {
        (Ok(_), Ok(())) => match state.refresh_agent_readiness() {
            Ok(true) => {
                state.status = "Pi is ready · press i to start a conversation".into();
            }
            Ok(false) => {
                state.status =
                    "Pi login finished, but readiness is incomplete · press l to retry".into();
            }
            Err(error) => state.status = format!("Pi readiness check failed: {error}"),
        },
        (Err(error), Ok(())) => {
            state.status = format!("Pi login failed: {error}");
        }
        (Ok(_), Err(error)) => {
            state.status = format!("Pi login finished, but TUI could not resume: {error}");
            state.quit = true;
        }
        (Err(error), Err(resume_error)) => {
            state.status = format!("Pi login failed: {error}; TUI resume failed: {resume_error}");
            state.quit = true;
        }
    }
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
    fn suspend(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        if let Err(error) = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            DisableFocusChange,
            DisableMouseCapture,
            LeaveAlternateScreen
        ) {
            let _ = enable_raw_mode();
            return Err(error);
        }
        self.terminal.show_cursor()
    }

    fn resume(&mut self) -> io::Result<()> {
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableFocusChange,
            EnableBracketedPaste
        )?;
        enable_raw_mode()?;
        self.terminal.clear()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_navigation_maps_rows_inside_the_visible_list() {
        assert_eq!(navigation_surface_at(120, 2, 4), Some(DevSurface::Agent));
        assert_eq!(navigation_surface_at(120, 2, 5), Some(DevSurface::Code));
        assert_eq!(navigation_surface_at(120, 2, 10), Some(DevSurface::Debug));
        assert_eq!(navigation_surface_at(120, 2, 3), None);
        assert_eq!(navigation_surface_at(120, 22, 4), None);
        assert_eq!(navigation_surface_at(71, 2, 4), None);
    }
}

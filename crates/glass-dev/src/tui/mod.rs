//! Decomposed full Glass Dev terminal application.

mod command;
mod file_view;
mod projection;
/// Rendering primitives and frame composition for the development TUI.
pub mod render;
mod snapshot;
/// Public reducer state and surface-selection types for the development TUI.
pub mod state;
mod syntax;

use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use glass_browser::browser::policy::PolicyPreset;
use glass_browser::browser_workspace::{BrowserConnectionPhase, BrowserWorkspaceIntent};
use glass_browser::cli::args::{
    TuiLayout, TuiLiveBackend, TuiLiveFit, TuiLiveMode, TuiLiveQuality,
};
use glass_browser::presentation::{
    BrowserFrame, CaptureScale, FrameDamage, FrameDropCounts, FrameEncoding,
    PRESENTATION_CONTRACT_SCHEMA_VERSION, PixelSize, TargetResourceIdentity,
};
use glass_browser::terminal_graphics::{GraphicsMode, PaneArea, TerminalGraphics};
use glass_browser::tui::live_view::{
    VisualPath, decide_path, frame_fit, frame_interval_ms, pane_size, png_dimensions,
};
use glass_browser::tui::{HerdrEnvironment, HerdrEvent, HerdrFrame, HerdrGraphicsWorker};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::time::{Duration, Instant};

/// Public TUI state, selected surface, product mode, and responsive class.
pub use state::{DevSurface, DevTuiState, ProductMode, ResponsiveClass};

/// Visual settings used to select the live browser preview path.
#[derive(Debug, Clone, Copy)]
pub struct TuiVisualOptions {
    /// Requested live-rendering mode.
    pub mode: TuiLiveMode,
    /// Preferred rendering backend.
    pub backend: TuiLiveBackend,
    /// Requested image quality.
    pub quality: TuiLiveQuality,
    /// Frame fit policy.
    pub fit: TuiLiveFit,
}

const KITTY_CLEAR: &[u8] = b"\x1b_Ga=d,d=A\x1b\\";

struct VisualRuntime {
    path: VisualPath,
    live: bool,
    quality: TuiLiveQuality,
    fit: TuiLiveFit,
    herdr: Option<HerdrGraphicsWorker>,
    kitty: Option<TerminalGraphics>,
    kitty_generation: u64,
    kitty_pane: Option<PaneArea>,
    kitty_drawn: bool,
}

impl VisualRuntime {
    fn new(
        options: TuiVisualOptions,
    ) -> Result<Self, glass_browser::terminal_graphics::GraphicsError> {
        let herdr_available = HerdrEnvironment::from_process().is_some();
        let path = decide_path(options.mode, options.backend, herdr_available);
        let herdr = matches!(&path, VisualPath::Herdr)
            .then(|| HerdrEnvironment::from_process().map(HerdrGraphicsWorker::spawn))
            .flatten();
        let kitty = if matches!(&path, VisualPath::Kitty) {
            Some(Self::new_kitty_renderer()?)
        } else {
            None
        };
        let live = matches!(
            &path,
            VisualPath::Herdr | VisualPath::Kitty | VisualPath::Ansi
        ) && matches!(options.mode, TuiLiveMode::On | TuiLiveMode::Auto);
        Ok(Self {
            path,
            live,
            quality: options.quality,
            fit: options.fit,
            herdr,
            kitty,
            kitty_generation: 0,
            kitty_pane: None,
            kitty_drawn: false,
        })
    }

    fn new_kitty_renderer()
    -> Result<TerminalGraphics, glass_browser::terminal_graphics::GraphicsError> {
        let identity = TargetResourceIdentity::new("glass-tui", Some("kitty-live".into()))?;
        TerminalGraphics::new(GraphicsMode::Kitty, identity)
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
            VisualPath::Kitty => {
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
            VisualPath::Kitty => {
                browser.presentation =
                    glass_browser::browser_workspace::BrowserPresentationPath::Kitty;
                browser.presentation_reason =
                    Some("waiting for a Kitty terminal graphics frame".into());
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

    fn sync_kitty_area(
        &mut self,
        pane: Option<PaneArea>,
        terminal: &mut TerminalGuard,
    ) -> io::Result<()> {
        if !matches!(self.path, VisualPath::Kitty) {
            return Ok(());
        }
        if self.kitty_pane != pane && self.kitty_drawn {
            terminal.write_bytes(KITTY_CLEAR)?;
            self.kitty_drawn = false;
        }
        self.kitty_pane = pane;
        Ok(())
    }

    fn submit_kitty(
        &mut self,
        png: &[u8],
        pane: PaneArea,
        browser_revision: u64,
    ) -> Result<Vec<u8>, String> {
        let (image_width, image_height) =
            png_dimensions(png).ok_or_else(|| "screenshot payload was not a PNG".to_string())?;
        let viewport = PixelSize::new(image_width, image_height);
        viewport
            .validate("screenshot")
            .map_err(|error| error.to_string())?;

        let reset = self
            .kitty
            .as_ref()
            .is_none_or(|graphics| browser_revision < graphics.browser_revision());
        if reset {
            self.kitty = Some(Self::new_kitty_renderer().map_err(|error| error.to_string())?);
            self.kitty_pane = Some(pane);
            self.kitty_drawn = false;
        }
        let graphics = self
            .kitty
            .as_mut()
            .ok_or_else(|| "Kitty renderer is unavailable".to_string())?;
        graphics
            .resize(
                pane,
                viewport,
                viewport,
                CaptureScale::FULL,
                browser_revision,
            )
            .map_err(|error| error.to_string())?;
        self.kitty_generation = self.kitty_generation.saturating_add(1).max(1);
        let frame = BrowserFrame {
            schema_version: PRESENTATION_CONTRACT_SCHEMA_VERSION,
            generation: self.kitty_generation,
            identity: TargetResourceIdentity::new("glass-tui", Some("kitty-live".into()))
                .map_err(|error| error.to_string())?,
            acquired_at_ms: self.kitty_generation,
            viewport,
            content: viewport,
            capture_scale: CaptureScale::FULL,
            encoding: FrameEncoding::Png,
            keyframe: true,
            damage: FrameDamage::Full,
            browser_revision,
            geometry_revision: graphics.geometry_revision(),
            dropped: FrameDropCounts::default(),
        };
        graphics
            .submit(frame, png)
            .map_err(|error| error.to_string())?;
        graphics
            .present_pending()
            .map_err(|error| error.to_string())?;
        let rendered = graphics
            .render_current("")
            .map_err(|error| error.to_string())?;
        if rendered.mode != GraphicsMode::Kitty {
            return Err("Kitty renderer returned a semantic frame".into());
        }
        Ok(rendered.bytes)
    }

    fn mark_kitty_drawn(&mut self) {
        self.kitty_drawn = true;
    }

    fn shutdown(&mut self) -> Vec<u8> {
        self.kitty
            .as_mut()
            .map(TerminalGraphics::shutdown)
            .unwrap_or_default()
    }
}

/// Run the interactive Glass Dev TUI until the user exits.
///
/// Requires interactive stdin and stdout. `root` is the project workspace;
/// `layout` selects the desktop/phone composition and `visual_options`
/// selects the optional browser preview path. Non-interactive callers should
pub fn run(
    root: impl AsRef<Path>,
    layout: TuiLayout,
    visual_options: TuiVisualOptions,
    yolo_mode: bool,
    policy_preset: PolicyPreset,
) -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Glass Dev TUI requires an interactive terminal; use a CLI subcommand or --mcp for non-interactive use".into());
    }
    let mut state = DevTuiState::open_for_tui_with_policy(root, layout, yolo_mode, policy_preset)?;
    let mut visual = VisualRuntime::new(visual_options)?;
    visual.sync_state(&mut state);
    let mut worker = snapshot::SnapshotWorker::spawn(&state);
    worker.request_refresh();
    let mut guard = TerminalGuard::enter()?;
    let mut last_refresh = Instant::now();
    let mut last_visual = Instant::now();
    let mut last_render = Instant::now() - Duration::from_millis(33);
    let mut previous_overlay_mask = 0_u16;
    loop {
        let size = guard.terminal.size()?;
        let overlay_mask = terminal_overlay_mask(&state);
        if overlay_mask != previous_overlay_mask {
            guard
                .terminal
                .resize(Rect::new(0, 0, size.width, size.height))?;
            previous_overlay_mask = overlay_mask;
        }
        state.set_terminal_size(size.width, size.height);
        let kitty_area =
            render::browser_visual_area(&state, Rect::new(0, 0, size.width, size.height))
                .map(|area| PaneArea::new(area.x, area.y, area.width, area.height));
        visual.sync_kitty_area(kitty_area, &mut guard)?;
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
                    } else if state.editor_exit_prompt.is_some() {
                        state.handle_editor_exit_key(key.code);
                    } else if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                        && state.code_edit_mode
                    {
                        state.request_editor_exit();
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
                                let was_live = state.browser_visual_live;
                                let visual_requested = state.surface == DevSurface::App
                                    && state
                                        .surface_actions()
                                        .get(state.menu_selection)
                                        .is_some_and(|action| action.command == "browser view");
                                state.run_menu_action();
                                if visual_requested && state.browser_visual_live != was_live {
                                    let live = state.browser_visual_live;
                                    if let Some(reason) = visual.request_live(live) {
                                        state.browser_visual_live = false;
                                        state.status = format!("Live view unavailable · {reason}");
                                    } else {
                                        visual.sync_state(&mut state);
                                        state.status = if live {
                                            "Live view starting · screenshot worker will update the pane"
                                                .into()
                                        } else {
                                            "Live view off · semantic inspection remains available"
                                                .into()
                                        };
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
                                state.pending_browser_navigation = None;
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
                            KeyCode::Up | KeyCode::Char('k') => state.scroll_surface(-1),
                            KeyCode::Down | KeyCode::Char('j') => state.scroll_surface(1),
                            KeyCode::Enter | KeyCode::Char('d') => {
                                state.queue_git_diff(&mut worker)
                            }
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
                            KeyCode::Esc => state.request_editor_exit(),
                            _ => state.edit_code_key(key.code, key.modifiers),
                        }
                    } else if state.file_picker_open {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_file_picker(),
                            (KeyCode::Char('p'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.close_file_picker();
                            }
                            (KeyCode::Enter, _) => state.submit_file_picker(),
                            (KeyCode::Backspace, _) => state.file_picker_backspace(),
                            (KeyCode::Char('u'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.file_picker_query.clear();
                                state.file_picker_cursor = 0;
                                state.file_picker_selection = 0;
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                                state.move_file_picker_selection(-1)
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                                state.move_file_picker_selection(1)
                            }
                            (KeyCode::Char(character), _) => {
                                state.insert_file_picker_char(character)
                            }
                            _ => {}
                        }
                    } else if state.composer_mode {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_composer(),
                            (KeyCode::Enter, value) if value.contains(KeyModifiers::SHIFT) => {
                                state.insert_composer_newline();
                            }
                            (KeyCode::Enter, _) => state.submit_composer(&mut worker),
                            (KeyCode::Backspace, _) => state.composer_backspace(),
                            (KeyCode::Up, _) => state.navigate_composer_history(true),
                            (KeyCode::Down, _) => state.navigate_composer_history(false),
                            (KeyCode::Char('p'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.navigate_composer_history(true);
                            }
                            (KeyCode::Char('n'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.navigate_composer_history(false);
                            }
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
                            (KeyCode::End, _) => {
                                state.composer_cursor = state.composer_input.len();
                            }
                            (KeyCode::Char(character), _) => {
                                state.insert_composer_text(&character.to_string());
                            }
                            _ => {}
                        }
                    } else if state.command_mode {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_palette(),
                            (KeyCode::Enter, _) => {
                                let was_live = state.browser_visual_live;
                                state.submit_palette(&mut worker);
                                if state.browser_visual_live != was_live {
                                    let live = state.browser_visual_live;
                                    if let Some(reason) = visual.request_live(live) {
                                        state.browser_visual_live = false;
                                        state.status = format!("Live view unavailable · {reason}");
                                    } else {
                                        visual.sync_state(&mut state);
                                        state.status = if live {
                                            "Live view starting · screenshot worker will update the pane"
                                                .into()
                                        } else {
                                            "Live view off · semantic inspection remains available"
                                                .into()
                                        };
                                    }
                                }
                            }
                            (KeyCode::Backspace, _) => state.palette_backspace(),
                            (KeyCode::Char('u'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.command_input.clear();
                                state.command_cursor = 0;
                                state.palette_error = None;
                                state.palette_scroll = 0;
                                state.palette_selection = 0;
                            }
                            (KeyCode::Char('p'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.navigate_palette_history(true);
                            }
                            (KeyCode::Char('n'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.navigate_palette_history(false);
                            }
                            (KeyCode::Left, _) => state.move_palette_cursor(false),
                            (KeyCode::Right, _) => state.move_palette_cursor(true),
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                                state.move_palette_selection(-1)
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                                state.move_palette_selection(1)
                            }
                            (KeyCode::PageUp, _) => state.scroll_palette(-1),
                            (KeyCode::PageDown, _) => state.scroll_palette(1),
                            (KeyCode::Tab, _) if state.command_input.trim().is_empty() => {
                                state.move_palette_selection(1)
                            }
                            (KeyCode::Tab, _) => state.complete_palette(),
                            (KeyCode::Char(character), _) => state.insert_palette_char(character),
                            _ => {}
                        }
                    } else {
                        match (key.code, key.modifiers) {
                            (KeyCode::Char('p'), value)
                                if value.contains(KeyModifiers::CONTROL)
                                    && value.contains(KeyModifiers::SHIFT) =>
                            {
                                state.open_palette();
                            }
                            (KeyCode::Char('p'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.open_file_picker();
                            }
                            (KeyCode::Char('k'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.open_palette();
                            }
                            (KeyCode::Esc, _) if state.running_tool_job.is_some() => {
                                state.status =
                                    "Background operation is bounded · Ctrl-C opens quit confirmation"
                                        .into();
                            }
                            (KeyCode::Char('s'), _) if state.surface == DevSurface::Terminal => {
                                state.request_detected_dev();
                            }
                            (KeyCode::Char('d'), _) if state.surface == DevSurface::Git => {
                                state.queue_git_diff(&mut worker);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::Agent => {
                                state.start_agent_interaction();
                            }
                            (KeyCode::Up | KeyCode::Char('k'), _)
                                if state.surface == DevSurface::Git =>
                            {
                                state.move_git_selection(-1);
                            }
                            (KeyCode::Down | KeyCode::Char('j'), _)
                                if state.surface == DevSurface::Git =>
                            {
                                state.move_git_selection(1);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::Git => {
                                state.queue_git_diff(&mut worker);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::Code => {
                                state.open_selected_file_for_edit();
                            }
                            (KeyCode::Char(']'), _) if state.surface == DevSurface::Agent => {
                                state.cycle_agent_selection(1)
                            }
                            (KeyCode::Char('['), _) if state.surface == DevSurface::Agent => {
                                state.cycle_agent_selection(-1)
                            }
                            (KeyCode::Char(']'), _) if state.surface == DevSurface::Code => {
                                state.cycle_editor_buffer(1)
                            }
                            (KeyCode::Char('['), _) if state.surface == DevSurface::Code => {
                                state.cycle_editor_buffer(-1)
                            }
                            (KeyCode::Char('T' | 't'), _) if state.surface == DevSurface::App => {
                                state.queue_browser_targets(&mut worker);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::App => {
                                state.queue_browser_intent(BrowserWorkspaceIntent::ActivateSelected)
                            }
                            (KeyCode::PageUp, _) => state.scroll_surface(-10),
                            (KeyCode::PageDown, _) => state.scroll_surface(10),
                            (KeyCode::Home, _) => state.scroll_home(),
                            (KeyCode::End, _) => state.scroll_end(),
                            (KeyCode::Left, modifiers)
                                if !modifiers.contains(KeyModifiers::ALT) =>
                            {
                                state.previous_surface()
                            }
                            (KeyCode::Right, modifiers)
                                if !modifiers.contains(KeyModifiers::ALT) =>
                            {
                                state.next_surface()
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
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
                        if !state.quit_confirmation
                            && state.editor_exit_prompt.is_none()
                            && !state.help_open
                            && !state.menu_open
                            && !state.command_mode
                            && !state.composer_mode
                            && !state.code_edit_mode
                            && state.pending_confirmation.is_none()
                            && state.pending_agent_approval.is_none()
                            && !state.browser_target_picker
                            && state.browser_recovery.is_none()
                            && !state.git_diff_open =>
                    {
                        let responsive =
                            state.responsive_class(state.terminal_width, state.terminal_height);
                        let header_height = if state.composer_mode { 3 } else { 2 };
                        if let Some(surface) = navigation_surface_at(
                            responsive,
                            header_height,
                            mouse.column,
                            mouse.row,
                        ) {
                            state.surface = surface;
                            state.status = format!("{} selected", surface.label());
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
        if let Some(name) = state.harness_launch_requested.take() {
            run_external_harness(&mut state, &mut guard, &name);
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
        if let Ok(Some(result)) = worker.try_job_result() {
            let browser_start = result.tool == "glass.browser.start" && result.result.is_ok();
            let browser_observe = result.tool == "glass.browser.observe" && result.result.is_ok();
            state.apply_tool_job_result(result);
            if browser_start {
                state.continue_pending_browser_navigation(&mut worker);
            } else if browser_observe && state.pending_browser_navigation.is_some() {
                state.submit_pending_browser_navigation(&mut worker);
            } else {
                state.queue_browser_observe(&mut worker);
            }
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
                VisualPath::Kitty if visual.live => match visual_png(&result) {
                    Ok(png) => {
                        let pane = render::browser_visual_area(
                            &state,
                            Rect::new(0, 0, state.terminal_width, state.terminal_height),
                        )
                        .map(|area| PaneArea::new(area.x, area.y, area.width, area.height));
                        if let Some(pane) = pane {
                            let browser_revision = state
                                .browser_workspace
                                .state()
                                .browser_revision
                                .unwrap_or(0);
                            match visual.submit_kitty(&png, pane, browser_revision) {
                                Ok(bytes) => {
                                    guard.write_bytes(&bytes)?;
                                    visual.mark_kitty_drawn();
                                    let browser = state.browser_workspace.state_mut();
                                    browser.presentation =
                                        glass_browser::browser_workspace::BrowserPresentationPath::Kitty;
                                    browser.frame_revision = browser.browser_revision;
                                    browser.presentation_reason = Some(
                                        "Kitty terminal graphics frame emitted · semantic controls remain authoritative"
                                            .into(),
                                    );
                                    state.status = "Live view updated · Kitty graphics".into();
                                }
                                Err(error) => {
                                    visual.live = false;
                                    visual.sync_state(&mut state);
                                    visual.sync_kitty_area(None, &mut guard)?;
                                    state.browser_workspace.state_mut().presentation_reason =
                                        Some(error.clone());
                                    state.status = format!("Live view unavailable · {error}");
                                }
                            }
                        }
                    }
                    Err(error) => {
                        visual.live = false;
                        visual.sync_state(&mut state);
                        visual.sync_kitty_area(None, &mut guard)?;
                        state.browser_workspace.state_mut().presentation_reason =
                            Some(error.clone());
                        state.status = format!("Live view unavailable · {error}");
                    }
                },
                VisualPath::Kitty => {}
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
    }

    let kitty_cleanup = visual.shutdown();
    guard.write_bytes(&kitty_cleanup)?;
    drop(worker);
    Ok(())
}
fn terminal_overlay_mask(state: &DevTuiState) -> u16 {
    let mut mask = 0_u16;
    if state.command_mode {
        mask |= 1 << 0;
    }
    if state.composer_mode {
        mask |= 1 << 1;
    }
    if state.menu_open {
        mask |= 1 << 2;
    }
    if state.help_open {
        mask |= 1 << 3;
    }
    if state.code_edit_mode {
        mask |= 1 << 4;
    }
    if state.browser_target_picker {
        mask |= 1 << 5;
    }
    if state.browser_recovery.is_some() {
        mask |= 1 << 6;
    }
    if state.git_diff_open {
        mask |= 1 << 7;
    }
    if state.pending_confirmation.is_some() {
        mask |= 1 << 8;
    }
    if state.pending_agent_approval.is_some() {
        mask |= 1 << 9;
    }
    if state.quit_confirmation {
        mask |= 1 << 10;
    }
    if state.editor_exit_prompt.is_some() {
        mask |= 1 << 11;
    }
    mask
}

/// Map a left-click on the desktop or compact navigation column to a surface.
/// The caller supplies the rendered header height so composer mode stays aligned.
fn navigation_surface_at(
    responsive: ResponsiveClass,
    header_height: u16,
    column: u16,
    row: u16,
) -> Option<DevSurface> {
    let nav_width = match responsive {
        ResponsiveClass::Desktop => 24,
        ResponsiveClass::Compact => 22,
        ResponsiveClass::Phone => return None,
    };
    let first_item_row = header_height.saturating_add(1);
    if column >= nav_width || row < first_item_row {
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
                state.status = "Pi is ready · press Enter or start typing to chat".into();
            }
            Ok(false) => {
                state.status =
                    "Pi needs setup · use :agent setup or :agent setup login, then Enter to chat"
                        .into();
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

fn run_external_harness(state: &mut DevTuiState, guard: &mut TerminalGuard, name: &str) {
    let resolved = match crate::harness::resolve(name) {
        Ok(resolved) => resolved,
        Err(error) => {
            state.status = format!("Harness launch unavailable · {error}");
            return;
        }
    };
    let root = state.ws().map(|workspace| workspace.root().to_path_buf());
    let root = match root {
        Ok(root) => root,
        Err(error) => {
            state.status = format!("Harness launch unavailable · {error}");
            return;
        }
    };
    if let Err(error) = guard.suspend() {
        state.status = format!(
            "Could not hand the terminal to {}: {error}",
            resolved.spec.label
        );
        return;
    }
    let result = crate::harness::launch_resolved(&resolved, &root);
    let resume = guard.resume();
    match (result, resume) {
        (Ok(status), Ok(())) if status.success() => {
            state.status = format!("{} exited · Glass workspace resumed", resolved.spec.label);
        }
        (Ok(status), Ok(())) => {
            state.status = format!(
                "{} exited with {} · Glass workspace resumed",
                resolved.spec.label,
                status
                    .code()
                    .map_or_else(|| "a signal".into(), |code| format!("status {code}"))
            );
        }
        (Err(error), Ok(())) => {
            state.status = format!("{} failed to start: {error}", resolved.spec.label);
        }
        (Ok(_), Err(error)) => {
            state.status = format!(
                "{} exited, but Glass could not resume: {error}",
                resolved.spec.label
            );
            state.quit = true;
        }
        (Err(error), Err(resume_error)) => {
            state.status = format!(
                "{} failed to start: {error}; Glass resume failed: {resume_error}",
                resolved.spec.label
            );
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

    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let backend = self.terminal.backend_mut();
        backend.write_all(bytes)?;
        backend.flush()
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
        // `Terminal::clear` queries the cursor position so it can preserve it. That query
        // is not supported by every terminal multiplexer/PTY (and can hang after a child
        // process returns), so clear the fullscreen surface directly and force Ratatui to
        // redraw both buffers instead.
        execute!(
            self.terminal.backend_mut(),
            Clear(ClearType::All),
            crossterm::cursor::MoveTo(0, 0)
        )?;
        self.terminal.current_buffer_mut().reset();
        self.terminal.swap_buffers();
        Ok(())
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
        assert_eq!(
            navigation_surface_at(ResponsiveClass::Desktop, 2, 2, 3),
            Some(DevSurface::Agent)
        );
        assert_eq!(
            navigation_surface_at(ResponsiveClass::Desktop, 2, 2, 4),
            Some(DevSurface::Code)
        );
        assert_eq!(
            navigation_surface_at(ResponsiveClass::Desktop, 2, 2, 9),
            Some(DevSurface::Debug)
        );
        assert_eq!(
            navigation_surface_at(ResponsiveClass::Desktop, 2, 2, 2),
            None
        );
        assert_eq!(
            navigation_surface_at(ResponsiveClass::Desktop, 2, 24, 3),
            None
        );
        assert_eq!(navigation_surface_at(ResponsiveClass::Phone, 2, 2, 3), None);
        assert_eq!(
            navigation_surface_at(ResponsiveClass::Compact, 3, 2, 4),
            Some(DevSurface::Agent)
        );
    }
}

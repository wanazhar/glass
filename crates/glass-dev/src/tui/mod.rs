//! Decomposed full Glass Dev terminal application.
//!
//! The event loop keeps overlays above surface keys: quit, editor exit, help,
//! menus, pickers, recovery, agent approval, mutation confirmation, the
//! full-screen editor, composer dock, then the command palette. Git workbench
//! keys stay live while a diff is open, but they do not trap confirm or
//! palette input. `Ctrl-C` opens quit confirmation from editor input; an
//! already-open unsaved-exit prompt keeps its save/discard/stay choices. Clean
//! `Esc` from NORMAL leaves the editor; unsaved work still asks.

mod bindings;
mod command;
mod editor;
mod file_view;
mod overlay;
mod parse;
mod pi_commands;
mod playbooks;
mod pointer;
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
use glass_browser::browser::policy::BrowserPolicy;
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

fn handle_untrusted_trust_shortcut(
    state: &mut DevTuiState,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> bool {
    if state.surface != DevSurface::Trust || state.trust_allows_execution() {
        return false;
    }

    match (code, modifiers) {
        (KeyCode::Char('g'), value) if value.contains(KeyModifiers::CONTROL) => {
            state.jump_to_app_keep_dock();
            true
        }
        (KeyCode::Char('p'), value)
            if value.contains(KeyModifiers::CONTROL) && value.contains(KeyModifiers::SHIFT) =>
        {
            state.open_palette();
            true
        }
        (KeyCode::Char('p'), value) if value.contains(KeyModifiers::CONTROL) => {
            state.open_file_picker();
            true
        }
        (KeyCode::Char('k'), value) if value.contains(KeyModifiers::CONTROL) => {
            state.open_palette();
            true
        }
        (KeyCode::Char('l'), value) if value.contains(KeyModifiers::CONTROL) => {
            state.focus_composer_dock();
            true
        }
        _ => false,
    }
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

    fn disable_herdr(&mut self, reason: String) -> bool {
        if !matches!(self.path, VisualPath::Herdr) {
            return false;
        }
        self.herdr.take();
        self.live = false;
        self.path = VisualPath::SemanticOnly { reason };
        true
    }
}

const VISUAL_PAUSED_STATUS: &str = "Live view paused · browser pane hidden";
const VISUAL_RESUMING_STATUS: &str = "Live view resuming · waiting for a fresh frame";
const VISUAL_PAUSED_REASON: &str = "Live view paused while the browser pane is hidden";
const VISUAL_RESUMING_REASON: &str = "Live view resuming; waiting for a fresh visible frame";

fn rendered_browser_visual_area(state: &DevTuiState) -> Option<Rect> {
    render::browser_visual_area(
        state,
        Rect::new(0, 0, state.terminal_width, state.terminal_height),
    )
}

fn visual_capture_area(state: &DevTuiState, visual: &VisualRuntime) -> Option<Rect> {
    (visual.live
        && state.browser_visual_live
        && state.browser_workspace.state().connection == BrowserConnectionPhase::Connected)
        .then(|| rendered_browser_visual_area(state))
        .flatten()
}

fn reconcile_visual_pane_visibility(
    state: &mut DevTuiState,
    visual: &VisualRuntime,
    previous_visible: &mut Option<bool>,
    has_pending_job: bool,
    pending_job_hidden: &mut bool,
) -> Option<Rect> {
    let live = visual.live && state.browser_visual_live;
    let area = live.then(|| rendered_browser_visual_area(state)).flatten();
    if !live {
        if previous_visible.take() == Some(true) {
            if has_pending_job {
                *pending_job_hidden = true;
            }
            state.browser_pane = None;
            state.browser_workspace.state_mut().frame_revision = None;
        }
        return None;
    }

    let visible = area.is_some();
    if !visible {
        if *previous_visible != Some(false) {
            if *previous_visible == Some(true) && has_pending_job {
                *pending_job_hidden = true;
            }
            let browser = state.browser_workspace.state_mut();
            browser.frame_revision = None;
            browser.presentation_reason = Some(VISUAL_PAUSED_REASON.into());
            state.browser_pane = None;
            if state.status.starts_with("Live view") {
                state.status = VISUAL_PAUSED_STATUS.into();
            }
        }
    } else if *previous_visible == Some(false) {
        let browser = state.browser_workspace.state_mut();
        if browser.presentation_reason.as_deref() == Some(VISUAL_PAUSED_REASON) {
            browser.presentation_reason = Some(VISUAL_RESUMING_REASON.into());
        }
        if state.status == VISUAL_PAUSED_STATUS {
            state.status = VISUAL_RESUMING_STATUS.into();
        }
    }
    *previous_visible = Some(visible);
    area
}

fn take_visual_result_if_current(
    result_id: u64,
    pending_job: &mut Option<u64>,
    pending_job_hidden: &mut bool,
    pane_available: bool,
) -> bool {
    let belongs_to_pending = *pending_job == Some(result_id);
    if belongs_to_pending {
        *pending_job = None;
    }
    let accepted = belongs_to_pending && !*pending_job_hidden && pane_available;
    if belongs_to_pending {
        *pending_job_hidden = false;
    }
    accepted
}

fn reconcile_browser_visual_request(state: &mut DevTuiState, visual: &mut VisualRuntime) {
    let Some(request) = state.take_browser_visual_request() else {
        return;
    };
    if let Some(reason) = visual.request_live(request.live) {
        visual.sync_state(state);
        state.browser_workspace.state_mut().presentation_reason = Some(reason.clone());
        state.status = format!("Live view unavailable · {reason}");
        return;
    }
    visual.sync_state(state);
    if let Some(failure) = request.failure {
        state.browser_workspace.state_mut().presentation_reason = Some(failure.clone());
        state.status = format!("Live view unavailable · {failure}");
    } else if let Some(status) = request.status {
        state.status = status;
    }
}

/// Run the interactive Glass Dev TUI with the complete browser policy.
pub(crate) fn run_with_browser_policy(
    root: impl AsRef<Path>,
    layout: TuiLayout,
    visual_options: TuiVisualOptions,
    yolo_mode: bool,
    browser_policy: BrowserPolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Glass Dev TUI requires an interactive terminal; use a CLI subcommand or --mcp for non-interactive use".into());
    }
    let mut state =
        DevTuiState::open_for_tui_with_browser_policy(root, layout, yolo_mode, browser_policy)?;
    let mut visual = VisualRuntime::new(visual_options)?;
    visual.sync_state(&mut state);
    let mut worker = snapshot::SnapshotWorker::spawn(&state);
    worker.request_refresh();
    let mut guard = TerminalGuard::enter()?;
    let mut last_refresh = Instant::now();
    let mut last_visual = Instant::now();
    let mut last_render = Instant::now() - Duration::from_millis(33);
    let mut render_requested = true;
    let mut previous_overlay_mask = 0_u32;
    let mut previous_active_overlay = overlay::active_overlay(&state);
    let mut pointer = pointer::PointerState::default();
    let mut previous_visual_pane_visible = None;
    let mut pending_visual_job = None;
    let mut pending_visual_job_hidden = false;
    loop {
        let size = guard.terminal.size()?;
        render_requested |= state.poll_native_browser_dialog();
        let active_overlay = overlay::active_overlay(&state);
        reset_pointer_on_overlay_transition(&state, &mut pointer, &mut previous_active_overlay);
        let overlay_mask = terminal_overlay_mask(&state);
        if overlay_mask != previous_overlay_mask {
            guard
                .terminal
                .resize(Rect::new(0, 0, size.width, size.height))?;
            previous_overlay_mask = overlay_mask;
        }
        let resized = state.terminal_width != size.width || state.terminal_height != size.height;
        state.set_terminal_size(size.width, size.height);
        render_requested |= resized;
        let visual_area = reconcile_visual_pane_visibility(
            &mut state,
            &visual,
            &mut previous_visual_pane_visible,
            pending_visual_job.is_some(),
            &mut pending_visual_job_hidden,
        );
        let kitty_area =
            visual_area.map(|area| PaneArea::new(area.x, area.y, area.width, area.height));
        visual.sync_kitty_area(kitty_area, &mut guard)?;
        let render_interval = if state.browser_visual_live && visual.live {
            Duration::from_millis(33)
        } else if active_overlay.is_some() || state.running_tool_job.is_some() {
            Duration::from_millis(100)
        } else if worker.is_busy() {
            // Background snapshots remain responsive without forcing an idle
            // terminal into the interactive overlay cadence.
            Duration::from_millis(250)
        } else {
            Duration::from_secs(1)
        };
        if render_requested || last_render.elapsed() >= render_interval {
            guard.terminal.draw(|frame| render::render(frame, &state))?;
            last_render = Instant::now();
            render_requested = false;
        }
        if state.quit {
            break;
        }
        if event::poll(Duration::from_millis(50))? {
            render_requested = true;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let active_overlay = overlay::active_overlay(&state);
                    // The quit modal is the strongest user-facing guard.
                    if active_overlay == Some(overlay::ActiveOverlay::QuitConfirmation) {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y' | 'Y') => state.confirm_quit(),
                            KeyCode::Esc | KeyCode::Char('n' | 'N') => state.cancel_quit(),
                            _ => {}
                        }
                    } else if active_overlay == Some(overlay::ActiveOverlay::EditorExitPrompt) {
                        state.handle_editor_exit_key(key.code);
                    } else if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        state.request_quit();
                    } else if active_overlay == Some(overlay::ActiveOverlay::BrowserDialog) {
                        state.handle_native_browser_dialog_key(key.code, key.modifiers);
                    } else if active_overlay == Some(overlay::ActiveOverlay::Help) {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('?') => state.toggle_help(),
                            KeyCode::Up | KeyCode::Char('k') => state.scroll_help(-1),
                            KeyCode::Down | KeyCode::Char('j') => state.scroll_help(1),
                            KeyCode::PageUp => state.scroll_help(-8),
                            KeyCode::PageDown => state.scroll_help(8),
                            KeyCode::Home => state.help_scroll = 0,
                            _ => {}
                        }
                    } else if active_overlay.is_none() && key.code == KeyCode::Char('?') {
                        state.toggle_help();
                    } else if active_overlay == Some(overlay::ActiveOverlay::CommandCenterMenu) {
                        match key.code {
                            KeyCode::Esc => state.close_menu(),
                            KeyCode::Enter => state.run_menu_action(),
                            KeyCode::Up | KeyCode::Char('k') => state.move_menu_selection(-1),
                            KeyCode::Down | KeyCode::Char('j') => state.move_menu_selection(1),
                            _ => {}
                        }
                    } else if active_overlay == Some(overlay::ActiveOverlay::BrowserTargetPicker) {
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
                    } else if active_overlay == Some(overlay::ActiveOverlay::BrowserRecovery) {
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
                    } else if active_overlay == Some(overlay::ActiveOverlay::AgentApproval) {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                                state.resolve_agent_approval(true, &mut worker);
                            }
                            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                                state.resolve_agent_approval(false, &mut worker);
                            }
                            _ => {}
                        }
                    } else if active_overlay == Some(overlay::ActiveOverlay::MutationConfirmation) {
                        match key.code {
                            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                                state.approve_confirmation_async(&mut worker);
                            }
                            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                                state.deny_confirmation();
                            }
                            _ => {}
                        }
                    } else if active_overlay == Some(overlay::ActiveOverlay::FullscreenEditor) {
                        match (key.code, key.modifiers) {
                            (KeyCode::Char('l'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.focus_composer_dock();
                            }
                            (KeyCode::Char('g'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.jump_to_app_keep_dock();
                            }
                            (KeyCode::Esc, _) => state.handle_editor_escape(),
                            _ => state.edit_code_key(key.code, key.modifiers),
                        }
                    } else if active_overlay == Some(overlay::ActiveOverlay::PiSlashCommand) {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_pi_command_palette(),
                            (KeyCode::Enter, _) => state.submit_pi_command(),
                            (KeyCode::Backspace, _) => state.pi_command_backspace(),
                            (KeyCode::Char('u'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.pi_command_input.clear();
                                state.pi_command_cursor = 0;
                                state.pi_command_selection = 0;
                                state.pi_command_scroll = 0;
                            }
                            (KeyCode::Left, _) => state.move_pi_command_cursor(false),
                            (KeyCode::Right, _) => state.move_pi_command_cursor(true),
                            (KeyCode::Home, _) => state.pi_command_cursor = 0,
                            (KeyCode::End, _) => {
                                state.pi_command_cursor = state.pi_command_input.len();
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                                state.move_pi_command_selection(-1)
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                                state.move_pi_command_selection(1)
                            }
                            (KeyCode::PageUp, _) => state.move_pi_command_selection(-8),
                            (KeyCode::PageDown, _) => state.move_pi_command_selection(8),
                            (KeyCode::Tab, _) => state.complete_pi_command(),
                            (KeyCode::Char(character), _) => {
                                state.insert_pi_command_char(character)
                            }
                            _ => {}
                        }
                    } else if dispatch_file_picker_key(&mut state, key.code, key.modifiers) {
                    } else if active_overlay == Some(overlay::ActiveOverlay::SessionPicker) {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_session_picker(),
                            (KeyCode::Enter, _) => state.submit_session_picker(&mut worker),
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                                state.move_session_picker_selection(-1)
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                                state.move_session_picker_selection(1)
                            }
                            _ => {}
                        }
                    } else if active_overlay == Some(overlay::ActiveOverlay::Composer) {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_composer(),
                            (KeyCode::Enter, value) if value.contains(KeyModifiers::SHIFT) => {
                                state.insert_composer_newline();
                            }
                            (KeyCode::Enter, _) => state.submit_composer(&mut worker),
                            (KeyCode::Tab, _) => state.complete_composer_mention(),
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
                            (KeyCode::Char('a'), value)
                                if value.contains(KeyModifiers::CONTROL)
                                    && value.contains(KeyModifiers::SHIFT) =>
                            {
                                state.cycle_composer_run_mode();
                            }
                            (KeyCode::Char('g'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.jump_to_app_keep_dock();
                            }
                            (KeyCode::Char('/'), _) if state.composer_input.trim().is_empty() => {
                                state.open_pi_command_palette();
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
                    } else if active_overlay == Some(overlay::ActiveOverlay::CommandPalette) {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => state.close_palette(),
                            (KeyCode::Enter, _) => state.submit_palette(&mut worker),
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
                            _ if handle_untrusted_trust_shortcut(
                                &mut state,
                                key.code,
                                key.modifiers,
                            ) => {}
                            (KeyCode::Char('l'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.focus_composer_dock();
                            }
                            (KeyCode::Char('a'), value)
                                if value.contains(KeyModifiers::CONTROL)
                                    && value.contains(KeyModifiers::SHIFT) =>
                            {
                                state.cycle_composer_run_mode();
                            }
                            (KeyCode::Char('g'), value)
                                if value.contains(KeyModifiers::CONTROL) =>
                            {
                                state.jump_to_app_keep_dock();
                            }
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
                            (KeyCode::Esc, _)
                                if state.git_diff_open && state.surface == DevSurface::Git =>
                            {
                                state.close_git_diff();
                            }
                            (KeyCode::Esc, _) if state.running_tool_job.is_some() => {
                                state.status =
                                    "Background operation is bounded · Ctrl-C opens quit confirmation"
                                        .into();
                            }
                            (KeyCode::Char('s'), _) if state.surface == DevSurface::Terminal => {
                                state.request_detected_dev();
                            }
                            (KeyCode::Char('u'), _) if state.surface == DevSurface::Terminal => {
                                match state.attach_selected_process_url() {
                                    Ok(message) => state.status = message,
                                    Err(error) => state.status = error,
                                }
                            }
                            (KeyCode::Char('x'), _)
                                if state.surface == DevSurface::Terminal
                                    && !state.composer_mode =>
                            {
                                state.stop_selected_process();
                            }
                            (KeyCode::Char('g'), _)
                                if state.surface == DevSurface::App && !state.code_edit_mode =>
                            {
                                state.jump_source_from_page();
                            }
                            (KeyCode::Char('d'), _) if state.surface == DevSurface::Git => {
                                state.queue_git_diff(&mut worker);
                            }
                            (KeyCode::Char('c'), _)
                                if state.surface == DevSurface::Git && !state.composer_mode =>
                            {
                                state.compose_git_commit();
                            }
                            (KeyCode::Char('o'), _)
                                if state.surface == DevSurface::Git && !state.composer_mode =>
                            {
                                state.open_selected_git_file();
                            }
                            (KeyCode::Char('x'), _)
                                if state.surface == DevSurface::Git && !state.composer_mode =>
                            {
                                state.discard_selected_git_file();
                            }
                            (KeyCode::Char('r'), _)
                                if state.surface == DevSurface::Git && !state.composer_mode =>
                            {
                                state.queue_github_review(&mut worker);
                            }
                            (KeyCode::Char(' '), _)
                                if state.surface == DevSurface::Git && !state.composer_mode =>
                            {
                                state.toggle_selected_git_stage();
                            }
                            (KeyCode::Char(' '), _)
                                if state.surface == DevSurface::Terminal
                                    && !state.composer_mode =>
                            {
                                state.restart_selected_process();
                            }
                            (KeyCode::Char(' '), _)
                                if state.surface == DevSurface::Debug && !state.composer_mode =>
                            {
                                state.continue_selected_debug();
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
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _)
                                if state.surface == DevSurface::Terminal =>
                            {
                                state.move_process_selection(-1);
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::Terminal =>
                            {
                                state.move_process_selection(1);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::Terminal => {
                                state.queue_selected_process_logs(&mut worker);
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _)
                                if state.surface == DevSurface::Debug =>
                            {
                                state.move_debug_selection(-1);
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::Debug =>
                            {
                                state.move_debug_selection(1);
                            }
                            (KeyCode::Char(']'), _) if state.surface == DevSurface::Debug => {
                                state.cycle_debug_pane(1);
                            }
                            (KeyCode::Char('['), _) if state.surface == DevSurface::Debug => {
                                state.cycle_debug_pane(-1);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::Debug => {
                                state.activate_debug_selection(&mut worker);
                            }
                            (KeyCode::Char('n'), _)
                                if state.surface == DevSurface::Debug && !state.composer_mode =>
                            {
                                state.step_selected_debug("over");
                            }
                            (KeyCode::Char('i'), _)
                                if state.surface == DevSurface::Debug && !state.composer_mode =>
                            {
                                state.step_selected_debug("in");
                            }
                            (KeyCode::Char('o'), _)
                                if state.surface == DevSurface::Debug && !state.composer_mode =>
                            {
                                state.step_selected_debug("out");
                            }
                            (KeyCode::Char('p'), _)
                                if state.surface == DevSurface::Debug && !state.composer_mode =>
                            {
                                state.pause_selected_debug();
                            }
                            (KeyCode::Char('b'), _)
                                if state.surface == DevSurface::Debug && !state.composer_mode =>
                            {
                                state.breakpoint_focused_line();
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _)
                                if state.surface == DevSurface::Tasks =>
                            {
                                state.move_todo_selection(-1);
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::Tasks =>
                            {
                                state.move_todo_selection(1);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::Tasks => {
                                state.complete_selected_todo();
                            }
                            (KeyCode::Char(' '), _)
                                if state.surface == DevSurface::Tasks && !state.composer_mode =>
                            {
                                state.activate_selected_todo();
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _)
                                if state.surface == DevSurface::More =>
                            {
                                state.move_more_selection(-1);
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::More =>
                            {
                                state.move_more_selection(1);
                            }
                            (KeyCode::Enter, _) if state.surface == DevSurface::More => {
                                state.activate_more_selection();
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
                            (KeyCode::Char('C'), _) if state.surface == DevSurface::App => {
                                state.comment_selected_app_entity();
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
                            (KeyCode::Char('f'), _) if state.surface == DevSurface::Agent => {
                                state.fork_selected_transcript(&mut worker);
                            }
                            (KeyCode::Char('r'), _) if state.surface == DevSurface::Agent => {
                                state.rewind_selected_transcript(&mut worker);
                            }
                            (KeyCode::Char('e'), _) if state.surface == DevSurface::Agent => {
                                state.edit_last_user_message();
                            }
                            (KeyCode::Char('o'), _) if state.surface == DevSurface::Agent => {
                                state.toggle_transcript_expand();
                            }
                            (KeyCode::Up, _) | (KeyCode::Char('k'), _)
                                if state.surface == DevSurface::Agent =>
                            {
                                state.move_transcript_selection(-1);
                            }
                            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                                if state.surface == DevSurface::Agent =>
                            {
                                state.move_transcript_selection(1);
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
                Event::Paste(text) => route_paste(&mut state, &text),
                Event::Mouse(mouse) => {
                    pointer.handle(&mut state, mouse, Instant::now());
                }
                Event::FocusLost => {
                    let _ = state
                        .browser_workspace
                        .reduce(BrowserWorkspaceIntent::CloseOverlay);
                }
                Event::Resize(width, height) => state.set_terminal_size(width, height),
                Event::Key(_) | Event::FocusGained => {}
            }
        }
        reset_pointer_on_overlay_transition(&state, &mut pointer, &mut previous_active_overlay);
        let menu_was_open = state.menu_open;
        pointer.poll(&mut state, Instant::now());
        reset_pointer_on_overlay_transition(&state, &mut pointer, &mut previous_active_overlay);
        reconcile_browser_visual_request(&mut state, &mut visual);
        let visual_area = reconcile_visual_pane_visibility(
            &mut state,
            &visual,
            &mut previous_visual_pane_visible,
            pending_visual_job.is_some(),
            &mut pending_visual_job_hidden,
        );
        let kitty_area =
            visual_area.map(|area| PaneArea::new(area.x, area.y, area.width, area.height));
        visual.sync_kitty_area(kitty_area, &mut guard)?;
        render_requested |= menu_was_open != state.menu_open;
        if state.agent_login_requested {
            state.agent_login_requested = false;
            run_agent_login(&mut state, &mut guard);
            worker.request_refresh();
        }
        if let Some(name) = state.harness_launch_requested.take() {
            run_external_harness(&mut state, &mut guard, &name);
            worker.request_refresh();
        }

        let visual_events = visual.poll_events();
        render_requested |= !visual_events.is_empty();
        for event in visual_events {
            handle_herdr_event(&mut state, &mut visual, event);
        }
        state.flush_pending_trust();
        state.flush_pending_open_file();
        if state.git_diff_requested {
            state.git_diff_requested = false;
            state.queue_git_diff(&mut worker);
        }
        if state.process_logs_requested {
            state.process_logs_requested = false;
            state.queue_selected_process_logs(&mut worker);
        }
        if state.debug_threads_requested {
            state.debug_threads_requested = false;
            state.queue_debug_threads(&mut worker);
        }
        if state.debug_stack_requested {
            state.debug_stack_requested = false;
            state.queue_debug_stack(&mut worker);
        }
        if state.debug_scopes_requested {
            state.debug_scopes_requested = false;
            state.queue_debug_scopes(&mut worker);
        }
        if state.debug_variables_requested {
            state.debug_variables_requested = false;
            state.queue_debug_variables(&mut worker);
        }
        if state.github_review_requested {
            state.github_review_requested = false;
            state.queue_github_review(&mut worker);
        }
        if state.queued_tool_request.is_some()
            && state.pending_confirmation.is_none()
            && !state.browser_target_picker_requested
        {
            state.submit_queued_tool(&mut worker);
        }
        state.flush_pending_selection_preview(&mut worker);
        if state.tick_pair_apply() {
            render_requested = true;
            last_render = Instant::now()
                .checked_sub(Duration::from_millis(33))
                .unwrap_or_else(Instant::now);
        }
        if state.tick_fim() {
            render_requested = true;
            last_render = Instant::now()
                .checked_sub(Duration::from_millis(33))
                .unwrap_or_else(Instant::now);
        }
        if last_refresh.elapsed() >= Duration::from_millis(250) {
            worker.request_refresh();
            last_refresh = Instant::now();
        } else if worker.is_busy() && last_render.elapsed() >= Duration::from_millis(100) {
            // Conversation tail keeps streaming while a full pass is in flight.
            worker.request_conversation();
            state.conversation_cursor = worker.conversation_cursor();
        }
        if pending_visual_job.is_none()
            && visual_capture_area(&state, &visual).is_some()
            && last_visual.elapsed() >= Duration::from_millis(frame_interval_ms(visual.quality))
        {
            let area = visual_capture_area(&state, &visual).expect("checked visual capture area");
            let available = (area.width, area.height);
            let (columns, rows) = pane_size(visual.quality, available);
            pending_visual_job = worker.submit_screenshot(columns, rows);
            if pending_visual_job.is_some() {
                last_visual = Instant::now();
            }
        }
        if let Ok(Some(result)) = worker.try_job_result() {
            render_requested = true;
            let browser_start = result.tool == "glass.browser.start" && result.result.is_ok();
            let browser_observe = result.tool == "glass.browser.observe" && result.result.is_ok();
            state.apply_tool_job_result(result);
            reconcile_browser_visual_request(&mut state, &mut visual);
            if state.pending_verify.is_some() {
                state.submit_pending_verify(&mut worker);
            } else if browser_start {
                state.continue_pending_browser_navigation(&mut worker);
            } else if browser_observe && state.pending_browser_navigation.is_some() {
                state.submit_pending_browser_navigation(&mut worker);
            } else {
                state.queue_browser_observe(&mut worker);
            }
        }
        state.flush_pending_selection_preview(&mut worker);
        if let Ok(Some(result)) = worker.try_visual_result() {
            render_requested = true;
            if take_visual_result_if_current(
                result.id,
                &mut pending_visual_job,
                &mut pending_visual_job_hidden,
                visual_capture_area(&state, &visual).is_some(),
            ) {
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
                            state.request_browser_visual_failure(error);
                            reconcile_browser_visual_request(&mut state, &mut visual);
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
                                        state.request_browser_visual_failure(error.clone());
                                        reconcile_browser_visual_request(&mut state, &mut visual);
                                        visual.sync_kitty_area(None, &mut guard)?;
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            state.request_browser_visual_failure(error.clone());
                            reconcile_browser_visual_request(&mut state, &mut visual);
                            visual.sync_kitty_area(None, &mut guard)?;
                        }
                    },
                    VisualPath::Kitty => {}
                    VisualPath::Ansi => apply_ansi_visual_result(&mut state, &visual, result),
                    VisualPath::SemanticOnly { .. } => {}
                    VisualPath::Herdr => {}
                }
                reconcile_browser_visual_request(&mut state, &mut visual);
            }
        }
        if let Some(snapshot) = worker.take_pending() {
            state.apply_snapshot(&snapshot);
            render_requested = true;
        }
        state.flush_pending_trust();
        state.flush_pending_open_file();
    }

    let kitty_cleanup = visual.shutdown();
    guard.write_bytes(&kitty_cleanup)?;
    drop(worker);
    Ok(())
}
fn terminal_overlay_mask(state: &DevTuiState) -> u32 {
    let overlay = overlay::active_overlay(state)
        .map(|active| 1_u32 << (16 + active as u32))
        .unwrap_or_default();
    // Preserve the pre-existing Git diff presentation bit while reserving the
    // upper bits for the single resolver-selected overlay.
    let git_diff = if state.git_diff_open { 1_u32 << 7 } else { 0 };
    overlay | git_diff
}

fn reset_pointer_on_overlay_transition(
    state: &DevTuiState,
    pointer: &mut pointer::PointerState,
    previous: &mut Option<overlay::ActiveOverlay>,
) {
    let current = overlay::active_overlay(state);
    if current != *previous {
        pointer.reset();
        *previous = current;
    }
}

fn route_paste(state: &mut DevTuiState, text: &str) {
    match overlay::active_overlay(state) {
        Some(overlay::ActiveOverlay::BrowserDialog) => {
            for character in text.chars() {
                state.insert_native_dialog_char(character);
            }
        }
        Some(overlay::ActiveOverlay::BrowserTargetPicker) => {
            for character in text.chars() {
                state.insert_browser_target_query(character);
            }
        }
        Some(overlay::ActiveOverlay::FilePicker) => {
            for character in text.chars() {
                state.insert_file_picker_char(character);
            }
        }
        Some(overlay::ActiveOverlay::PiSlashCommand) => {
            for character in text.chars() {
                state.insert_pi_command_char(character);
            }
        }
        Some(overlay::ActiveOverlay::Composer) => state.insert_composer_text(text),
        Some(overlay::ActiveOverlay::CommandPalette) => state.insert_palette_text(text),
        _ => {}
    }
}

fn handle_file_picker_key(state: &mut DevTuiState, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Esc, _) => state.close_file_picker(),
        (KeyCode::Char('p'), value) if value.contains(KeyModifiers::CONTROL) => {
            state.close_file_picker();
        }
        (KeyCode::Enter, _) => state.submit_file_picker(),
        (KeyCode::Backspace, _) => state.file_picker_backspace(),
        (KeyCode::Char('u'), value) if value.contains(KeyModifiers::CONTROL) => {
            state.file_picker_query.clear();
            state.file_picker_cursor = 0;
            state.file_picker_selection = 0;
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => state.move_file_picker_selection(-1),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => state.move_file_picker_selection(1),
        (KeyCode::Char(character), _) => state.insert_file_picker_char(character),
        _ => {}
    }
}

fn dispatch_file_picker_key(
    state: &mut DevTuiState,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> bool {
    if overlay::active_overlay(state) != Some(overlay::ActiveOverlay::FilePicker) {
        return false;
    }
    handle_file_picker_key(state, code, modifiers);
    true
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

fn apply_ansi_visual_result(
    state: &mut DevTuiState,
    visual: &VisualRuntime,
    result: snapshot::VisualJobResult,
) {
    if visual_capture_area(state, visual).is_some() {
        state.apply_visual_job_result_with_fit(result, frame_fit(visual.fit));
    }
}

fn handle_herdr_event(state: &mut DevTuiState, visual: &mut VisualRuntime, event: HerdrEvent) {
    match event {
        HerdrEvent::Connected if visual.live => {
            let browser = state.browser_workspace.state();
            let has_current_frame = browser
                .frame_revision
                .is_some_and(|revision| Some(revision) == browser.browser_revision);
            if rendered_browser_visual_area(state).is_some() && has_current_frame {
                state.browser_workspace.state_mut().presentation =
                    glass_browser::browser_workspace::BrowserPresentationPath::Herdr;
                state.browser_workspace.state_mut().presentation_reason =
                    Some("Herdr pane graphics stream connected".into());
                state.status = "Live view ready · Herdr pane graphics".into();
            }
        }
        HerdrEvent::Failed(reason) => {
            let failure = format!("Herdr graphics unavailable: {reason}");
            if visual.disable_herdr(failure.clone()) {
                state.request_browser_visual_failure(failure);
                reconcile_browser_visual_request(state, visual);
            }
        }
        HerdrEvent::Stopped => {
            let reason = "Herdr pane graphics stream stopped";
            if visual.disable_herdr(reason.into()) {
                state.request_browser_visual_live(
                    false,
                    Some("Live view stopped · semantic inspection remains available".into()),
                );
                reconcile_browser_visual_request(state, visual);
                state.browser_workspace.state_mut().presentation_reason = Some(reason.into());
            }
        }
        HerdrEvent::Connected => {}
    }
}

fn run_agent_login(state: &mut DevTuiState, guard: &mut TerminalGuard) {
    if let Err(error) = guard.suspend() {
        state.status = format!("Could not hand the terminal to Pi: {error}");
        return;
    }
    let provider = state.agent_login_provider.take();
    let result = crate::pi_runtime::setup_pi_runtime_with_provider(
        None,
        None,
        false,
        true,
        provider.as_deref(),
    );
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn overlay_test_state() -> (DevTuiState, std::path::PathBuf) {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-overlay-routing-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create test workspace");
        let state = DevTuiState::open(&root, TuiLayout::Desktop).expect("open TUI state");
        (state, root)
    }

    fn test_visual_runtime(path: VisualPath) -> VisualRuntime {
        VisualRuntime {
            path,
            live: false,
            quality: TuiLiveQuality::Balanced,
            fit: TuiLiveFit::Contain,
            herdr: None,
            kitty: None,
            kitty_generation: 0,
            kitty_pane: None,
            kitty_drawn: false,
        }
    }

    fn active_visual_state() -> (DevTuiState, std::path::PathBuf, VisualRuntime) {
        let (mut state, root) = overlay_test_state();
        state.set_terminal_size(120, 32);
        state.surface = DevSurface::App;
        state.browser_visual_live = true;
        let browser = state.browser_workspace.state_mut();
        browser.connection = BrowserConnectionPhase::Connected;
        browser.browser_revision = Some(42);
        browser.frame_revision = Some(41);
        browser.presentation = glass_browser::browser_workspace::BrowserPresentationPath::Ansi;
        browser.presentation_reason = Some("previous visible ANSI frame".into());
        state.status = "Live view updated · ANSI half-block".into();
        let mut visual = test_visual_runtime(VisualPath::Ansi);
        visual.live = true;
        (state, root, visual)
    }

    fn fixture_png() -> Vec<u8> {
        use base64::Engine as _;

        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==")
            .expect("decode PNG fixture")
    }

    fn fixture_ansi_pane() -> glass_browser::tui::live_view::AnsiPane {
        let png = fixture_png();
        let mut canvas = glass_browser::terminal_graphics::AnsiCanvas::default();
        glass_browser::tui::live_view::AnsiPane::from_png(
            &mut canvas,
            &png,
            8,
            4,
            glass_browser::terminal_graphics::FrameFit::Contain,
        )
        .expect("fixture is a successful ANSI screenshot")
    }

    #[test]
    fn visual_capture_uses_rendered_app_pane_and_keeps_composer_visible() {
        let (mut state, root, visual) = active_visual_state();
        assert!(visual_capture_area(&state, &visual).is_some());

        state.composer_mode = true;
        assert!(visual_capture_area(&state, &visual).is_some());

        state.file_picker_open = true;
        assert!(visual_capture_area(&state, &visual).is_none());

        state.file_picker_open = false;
        state.surface = DevSurface::Code;
        assert!(visual_capture_area(&state, &visual).is_none());

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn hidden_visual_pane_pauses_resumes_and_discards_hidden_in_flight_frame() {
        use base64::Engine as _;

        let (mut state, root, visual) = active_visual_state();
        state.browser_pane = Some(fixture_ansi_pane());
        let mut previous_visible = None;
        let mut pending_hidden = false;
        let mut pending_job = Some(7);
        assert!(
            reconcile_visual_pane_visibility(
                &mut state,
                &visual,
                &mut previous_visible,
                true,
                &mut pending_hidden,
            )
            .is_some()
        );

        state.file_picker_open = true;
        assert!(
            reconcile_visual_pane_visibility(
                &mut state,
                &visual,
                &mut previous_visible,
                true,
                &mut pending_hidden,
            )
            .is_none()
        );
        assert!(pending_hidden);
        assert!(state.browser_visual_live);
        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::Ansi,
            "pause must preserve the selected backend for resume"
        );
        assert!(state.browser_pane.is_none());
        assert_eq!(
            state.browser_workspace.state().frame_revision,
            None,
            "hidden frames must not remain advertised as current"
        );
        assert_eq!(state.status, VISUAL_PAUSED_STATUS);
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some(VISUAL_PAUSED_REASON)
        );

        state.file_picker_open = false;
        assert!(
            reconcile_visual_pane_visibility(
                &mut state,
                &visual,
                &mut previous_visible,
                true,
                &mut pending_hidden,
            )
            .is_some()
        );
        assert!(state.browser_visual_live);
        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::Ansi
        );
        assert_eq!(state.status, VISUAL_RESUMING_STATUS);
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some(VISUAL_RESUMING_REASON)
        );
        assert!(!take_visual_result_if_current(
            7,
            &mut pending_job,
            &mut pending_hidden,
            visual_capture_area(&state, &visual).is_some(),
        ));
        assert_eq!(state.status, VISUAL_RESUMING_STATUS);
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some(VISUAL_RESUMING_REASON)
        );

        let mut fresh_job = Some(8);
        let mut fresh_job_hidden = false;
        assert!(take_visual_result_if_current(
            8,
            &mut fresh_job,
            &mut fresh_job_hidden,
            visual_capture_area(&state, &visual).is_some(),
        ));
        let png = base64::engine::general_purpose::STANDARD.encode(fixture_png());
        apply_ansi_visual_result(
            &mut state,
            &visual,
            snapshot::VisualJobResult {
                id: 8,
                columns: 8,
                rows: 4,
                result: Ok(serde_json::json!({ "base64": png })),
            },
        );
        assert_eq!(state.status, "Live view updated · ANSI half-block");
        assert_eq!(state.browser_workspace.state().frame_revision, Some(42));

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn resuming_visual_state_survives_delayed_herdr_connection_until_fresh_frame() {
        let (mut state, root, mut visual) = active_visual_state();
        visual.path = VisualPath::Herdr;
        state.browser_workspace.state_mut().presentation =
            glass_browser::browser_workspace::BrowserPresentationPath::Herdr;
        state.browser_workspace.state_mut().frame_revision = None;
        state.browser_workspace.state_mut().presentation_reason =
            Some(VISUAL_RESUMING_REASON.into());
        state.status = VISUAL_RESUMING_STATUS.into();

        handle_herdr_event(&mut state, &mut visual, HerdrEvent::Connected);

        assert_eq!(state.status, VISUAL_RESUMING_STATUS);
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some(VISUAL_RESUMING_REASON)
        );
        assert_eq!(state.browser_workspace.state().frame_revision, None);

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn visual_manual_toggle_off_clears_cached_frame_and_rejects_in_flight_result() {
        let (mut state, root, mut visual) = active_visual_state();
        state.browser_pane = Some(fixture_ansi_pane());
        let mut previous_visible = Some(true);
        let mut pending_job_hidden = false;
        let mut pending_job = Some(17);

        visual.live = false;
        state.browser_visual_live = false;
        state.status = "Live view stopped · semantic inspection remains available".into();
        assert!(
            reconcile_visual_pane_visibility(
                &mut state,
                &visual,
                &mut previous_visible,
                true,
                &mut pending_job_hidden,
            )
            .is_none()
        );
        assert!(state.browser_pane.is_none());
        assert_eq!(state.browser_workspace.state().frame_revision, None);
        assert_eq!(
            state.status,
            "Live view stopped · semantic inspection remains available"
        );

        visual.live = true;
        state.browser_visual_live = true;
        assert!(
            reconcile_visual_pane_visibility(
                &mut state,
                &visual,
                &mut previous_visible,
                true,
                &mut pending_job_hidden,
            )
            .is_some()
        );
        assert!(!take_visual_result_if_current(
            17,
            &mut pending_job,
            &mut pending_job_hidden,
            visual_capture_area(&state, &visual).is_some(),
        ));
        assert!(state.browser_pane.is_none());

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn stale_ansi_screenshot_is_ignored_after_live_view_stops() {
        use base64::Engine as _;

        let (mut state, root) = overlay_test_state();
        let stopped_status = "Live view stopped · semantic inspection remains available";
        let stopped_reason = "visual presentation is off; semantic inspection remains available";
        state.status = stopped_status.into();
        state.browser_visual_live = false;
        let browser = state.browser_workspace.state_mut();
        browser.presentation =
            glass_browser::browser_workspace::BrowserPresentationPath::SemanticOnly;
        browser.presentation_reason = Some(stopped_reason.into());

        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==")
            .expect("decode PNG fixture");
        let mut canvas = glass_browser::terminal_graphics::AnsiCanvas::default();
        glass_browser::tui::live_view::AnsiPane::from_png(
            &mut canvas,
            &png,
            8,
            4,
            glass_browser::terminal_graphics::FrameFit::Contain,
        )
        .expect("fixture is a successful ANSI screenshot");

        let mut visual = test_visual_runtime(VisualPath::Ansi);
        assert!(!visual.live);
        apply_ansi_visual_result(
            &mut state,
            &visual,
            snapshot::VisualJobResult {
                id: 9,
                columns: 8,
                rows: 4,
                result: Ok(serde_json::json!({
                    "base64": base64::engine::general_purpose::STANDARD.encode(png)
                })),
            },
        );
        reconcile_browser_visual_request(&mut state, &mut visual);

        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::SemanticOnly
        );
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some(stopped_reason)
        );
        assert_eq!(state.status, stopped_status);

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn failed_herdr_worker_stays_disabled_after_stopped_and_rejects_restart() {
        let (mut state, root) = overlay_test_state();
        let mut visual = test_visual_runtime(VisualPath::Herdr);
        visual.live = true;

        handle_herdr_event(
            &mut state,
            &mut visual,
            HerdrEvent::Failed("socket closed".into()),
        );
        let failure_reason = "Herdr graphics unavailable: socket closed";
        assert!(!visual.live);
        assert!(matches!(
            &visual.path,
            VisualPath::SemanticOnly { reason } if reason == failure_reason
        ));

        handle_herdr_event(&mut state, &mut visual, HerdrEvent::Stopped);
        state.request_browser_visual_live(true, Some("starting again".into()));
        reconcile_browser_visual_request(&mut state, &mut visual);

        assert!(!visual.live);
        assert!(!state.browser_visual_live);
        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::SemanticOnly
        );
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some(failure_reason)
        );
        assert!(state.status.contains(failure_reason));

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn stopped_herdr_worker_rejects_restart_with_a_persistent_reason() {
        let (mut state, root) = overlay_test_state();
        let mut visual = test_visual_runtime(VisualPath::Herdr);
        visual.live = true;

        handle_herdr_event(&mut state, &mut visual, HerdrEvent::Stopped);
        state.request_browser_visual_live(true, Some("starting again".into()));
        reconcile_browser_visual_request(&mut state, &mut visual);

        assert!(!visual.live);
        assert!(!state.browser_visual_live);
        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::SemanticOnly
        );
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some("Herdr pane graphics stream stopped")
        );
        assert!(state.status.contains("Herdr pane graphics stream stopped"));

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn visual_request_reconciliation_keeps_runtime_state_and_presentation_aligned() {
        let (mut state, root) = overlay_test_state();
        let mut visual = test_visual_runtime(VisualPath::SemanticOnly {
            reason: "test renderer unavailable".into(),
        });
        state.request_browser_visual_live(true, Some("starting".into()));
        reconcile_browser_visual_request(&mut state, &mut visual);

        assert!(!visual.live);
        assert!(!state.browser_visual_live);
        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::SemanticOnly
        );
        assert_eq!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref(),
            Some("test renderer unavailable")
        );
        assert!(state.status.contains("Live view unavailable"));

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn agent_watch_requests_and_reconciles_live_presentation() {
        let (mut state, root) = overlay_test_state();
        let mut visual = test_visual_runtime(VisualPath::Ansi);
        state.watch_agent_on_app(
            "glass.browser.click",
            &serde_json::json!({"target":"Continue"}),
        );
        assert!(state.browser_visual_live);
        reconcile_browser_visual_request(&mut state, &mut visual);

        assert!(visual.live);
        assert!(state.browser_visual_live);
        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::Ansi
        );
        assert!(state.status.contains("Agent click Continue · watching"));

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn ansi_screenshot_failure_clears_runtime_and_workspace_live_state() {
        let (mut state, root) = overlay_test_state();
        let mut visual = test_visual_runtime(VisualPath::Ansi);
        state.request_browser_visual_live(true, Some("starting".into()));
        reconcile_browser_visual_request(&mut state, &mut visual);
        assert!(visual.live);
        assert!(state.browser_visual_live);

        state.apply_visual_job_result_with_fit(
            snapshot::VisualJobResult {
                id: 1,
                columns: 40,
                rows: 20,
                result: Err("browser disconnected".into()),
            },
            glass_browser::terminal_graphics::FrameFit::Contain,
        );
        assert!(!state.browser_visual_live);
        reconcile_browser_visual_request(&mut state, &mut visual);

        assert!(!visual.live);
        assert!(!state.browser_visual_live);
        assert_eq!(
            state.browser_workspace.state().presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::SemanticOnly
        );
        assert!(
            state
                .browser_workspace
                .state()
                .presentation_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("browser screenshot failed"))
        );
        assert!(state.status.contains("Live view unavailable"));

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn file_picker_receives_keys_and_paste_above_editor_pi_and_palette() {
        let (mut state, root) = overlay_test_state();
        state.file_picker_open = true;
        state.session_picker_open = true;
        state.code_edit_mode = true;
        state.pi_command_mode = true;
        state.command_mode = true;
        state.pi_command_input = "pi".into();
        state.command_input = "palette".into();

        assert!(dispatch_file_picker_key(
            &mut state,
            KeyCode::Char('r'),
            KeyModifiers::empty()
        ));
        route_paste(&mut state, "eadme");

        assert_eq!(state.file_picker_query, "readme");
        assert_eq!(state.pi_command_input, "pi");
        assert_eq!(state.command_input, "palette");
        assert!(state.code_edit_mode);
        assert_eq!(
            overlay::active_overlay(&state),
            Some(overlay::ActiveOverlay::FilePicker)
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn redraw_mask_tracks_the_resolved_picker_and_keeps_git_diff_bit() {
        let (mut state, root) = overlay_test_state();
        state.code_edit_mode = true;
        state.pi_command_mode = true;
        state.file_picker_open = true;
        state.git_diff_open = true;
        let file_mask = terminal_overlay_mask(&state);
        assert_ne!(
            file_mask & (1_u32 << (16 + overlay::ActiveOverlay::FilePicker as u32)),
            0
        );
        assert_ne!(file_mask & (1_u32 << 7), 0);
        assert_eq!(
            file_mask & (1_u32 << (16 + overlay::ActiveOverlay::PiSlashCommand as u32)),
            0
        );

        state.file_picker_open = false;
        state.session_picker_open = true;
        let session_mask = terminal_overlay_mask(&state);
        assert_ne!(
            session_mask & (1_u32 << (16 + overlay::ActiveOverlay::SessionPicker as u32)),
            0
        );
        assert_ne!(file_mask, session_mask);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn overlay_opening_discards_a_press_started_on_the_covered_surface() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

        let (mut state, root) = overlay_test_state();
        state.terminal_width = 100;
        state.terminal_height = 40;
        let mut pointer = pointer::PointerState::default();
        let mut previous = overlay::active_overlay(&state);
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 80,
                row: 39,
                modifiers: KeyModifiers::empty(),
            },
            Instant::now(),
        );
        state.file_picker_open = true;
        reset_pointer_on_overlay_transition(&state, &mut pointer, &mut previous);
        pointer.poll(&mut state, Instant::now() + Duration::from_millis(20));
        state.file_picker_open = false;
        reset_pointer_on_overlay_transition(&state, &mut pointer, &mut previous);
        assert!(!pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: 80,
                row: 39,
                modifiers: KeyModifiers::empty(),
            },
            Instant::now() + Duration::from_millis(500),
        ));
        assert!(!state.composer_mode);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn mouse_navigation_hits_use_the_rendered_panel_on_responsive_layouts() {
        let (mut state, root) = overlay_test_state();
        for (layout, width, height) in [
            (TuiLayout::Desktop, 140, 40),
            (TuiLayout::Compact, 96, 32),
            (TuiLayout::Mobile, 48, 20),
        ] {
            state.layout = layout;
            state.terminal_width = width;
            state.terminal_height = height;
            state.surface = DevSurface::Agent;
            let screen = Rect::new(0, 0, width, height);
            let geometry = render::screen_geometry(&state, screen);
            if let Some(navigation) = geometry.navigation {
                let content = render::panel_content_area(navigation);
                assert_eq!(
                    pointer::hit_test(&state, content.x, content.y),
                    pointer::HitRegion::Surface(DevSurface::Agent),
                    "first visible row should select Agent for {layout:?}"
                );
                assert_eq!(
                    pointer::hit_test(&state, navigation.x, navigation.y),
                    pointer::HitRegion::Other,
                    "navigation border should not select a surface for {layout:?}"
                );
                if navigation.right() < width {
                    assert_eq!(
                        pointer::hit_test(&state, navigation.right(), content.y),
                        pointer::HitRegion::Other,
                        "coordinates outside the navigation panel should not select a surface"
                    );
                }
            } else {
                assert_eq!(
                    pointer::hit_test(&state, 2, 3),
                    pointer::HitRegion::Other,
                    "phone layout has no navigation panel"
                );
            }
        }

        state.layout = TuiLayout::Desktop;
        state.terminal_width = 140;
        state.terminal_height = 40;
        state.surface = DevSurface::Trust;
        let navigation = render::screen_geometry(
            &state,
            Rect::new(0, 0, state.terminal_width, state.terminal_height),
        )
        .navigation
        .expect("desktop navigation panel");
        let content = render::panel_content_area(navigation);
        assert_eq!(
            pointer::hit_test(&state, content.x, content.y),
            pointer::HitRegion::Surface(DevSurface::Trust)
        );
        assert_eq!(
            pointer::hit_test(&state, content.x, content.y + 1),
            pointer::HitRegion::Surface(DevSurface::Agent)
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn initial_trust_shortcuts_block_app_jump_and_file_picker_submission() {
        let root =
            std::env::temp_dir().join(format!("glass-trust-key-routing-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        std::fs::write(
            root.join("glass.toml"),
            "[tools.probe]\ndescription='probe'\ncommand='echo unsafe'\n",
        )
        .expect("write executable project configuration");
        std::fs::write(root.join("README.md"), "review only\n").expect("write test file");

        let mut state =
            DevTuiState::open(&root, TuiLayout::Desktop).expect("open untrusted TUI workspace");
        assert_eq!(state.surface, DevSurface::Trust);
        state.files.push("README.md".into());

        assert!(handle_untrusted_trust_shortcut(
            &mut state,
            KeyCode::Char('p'),
            KeyModifiers::CONTROL,
        ));
        assert!(!state.file_picker_open);
        assert_eq!(state.surface, DevSurface::Trust);

        // Even if a picker state survives a trust transition, submit cannot
        // open a project file or replace the Trust surface.
        state.file_picker_open = true;
        state.submit_file_picker();
        assert!(!state.file_picker_open);
        assert!(state.focused_editor_path.is_empty());
        assert_eq!(state.surface, DevSurface::Trust);

        assert!(handle_untrusted_trust_shortcut(
            &mut state,
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(state.surface, DevSurface::Trust);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }
}

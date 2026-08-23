//! Focused browser-only terminal workspace.
//!
//! The complete development cockpit is owned by `glass-dev`. This module keeps
//! the independently installable browser product responsive and structured
//! first without importing project, process, agent, or debugger contracts.

use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::collections::BTreeMap;
use std::io::{self, IsTerminal};
use std::time::Duration;
use std::time::Instant;

use super::herdr_graphics::{HerdrEnvironment, HerdrEvent, HerdrFrame, HerdrGraphicsWorker};
use super::live_view::{
    AnsiPane, VisualPath, decide_path, frame_fit, frame_interval_ms, pane_size,
};
use crate::browser::session::{
    BrowserResult, BrowserSession, SessionOptions, WorkflowCheckpoint, WorkflowDefinition,
    WorkflowRunResult,
};
use crate::browser_workspace::{
    BrowserConnectionPhase, BrowserWorkspaceAdapterKind, BrowserWorkspaceController,
    BrowserWorkspaceEntity, BrowserWorkspaceIntent, BrowserWorkspaceLayout,
};
use crate::cli::args::{Cli, TuiLiveFit, TuiLiveMode, TuiLiveQuality};
use crate::terminal_graphics::AnsiCanvas;

const PHONE_MAX_COLUMNS: u16 = 72;
const COMPACT_MAX_COLUMNS: u16 = 109;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceMode {
    #[default]
    Browser,
    Semantic,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponsiveClass {
    Phone,
    Compact,
    Desktop,
}

impl ResponsiveClass {
    const fn for_width(width: u16) -> Self {
        if width <= PHONE_MAX_COLUMNS {
            Self::Phone
        } else if width <= COMPACT_MAX_COLUMNS {
            Self::Compact
        } else {
            Self::Desktop
        }
    }
}

struct BrowserTui {
    mode: WorkspaceMode,
    command: String,
    status: String,
    page: String,
    session: Option<BrowserSession>,
    workspace: BrowserWorkspaceController,
    graphics: Option<HerdrGraphicsWorker>,
    visual: VisualState,
    last_workflow: Option<(WorkflowDefinition, WorkflowRunResult)>,
    workflow_checkpoint: Option<WorkflowCheckpoint>,
}

/// Live-visual state decided once from CLI flags plus the environment.
#[derive(Default)]
struct VisualState {
    path: Option<VisualPath>,
    live: bool,
    ansi_canvas: AnsiCanvas,
    ansi_pane: Option<AnsiPane>,
    quality: TuiLiveQuality,
    fit: TuiLiveFit,
}

impl VisualState {
    fn configure(&mut self, cli: &Cli) {
        let herdr_available = HerdrEnvironment::from_process().is_some();
        self.path = Some(decide_path(
            cli.tui_live,
            cli.tui_live_backend,
            herdr_available,
        ));
        self.quality = cli.tui_live_quality;
        self.fit = cli.tui_live_fit;
    }

    fn request_live(&mut self, on: bool) -> Option<String> {
        let path = self.path.clone()?;
        self.live = on;
        if !on {
            self.ansi_pane = None;
            return None;
        }
        match path {
            VisualPath::Herdr | VisualPath::Ansi => None,
            VisualPath::SemanticOnly { reason } => {
                self.live = false;
                Some(reason)
            }
        }
    }

    fn label(&self) -> String {
        match &self.path {
            Some(VisualPath::Herdr) if self.live => "Herdr live".into(),
            Some(VisualPath::Ansi) if self.live => "ANSI live".into(),
            Some(VisualPath::SemanticOnly { reason }) => format!("Semantic · {reason}"),
            _ => "Semantic · live off".into(),
        }
    }
}

impl BrowserTui {
    fn new(cli: &Cli) -> Self {
        let graphics = HerdrEnvironment::from_process().map(HerdrGraphicsWorker::spawn);
        let mut visual = VisualState::default();
        visual.configure(cli);
        let auto_live = matches!(
            visual.path,
            Some(VisualPath::Herdr) | Some(VisualPath::Ansi)
        ) && matches!(cli.tui_live, TuiLiveMode::On | TuiLiveMode::Auto);
        let _ = visual.request_live(auto_live);
        Self {
            mode: WorkspaceMode::Browser,
            command: String::new(),
            status: "Ready · structured observation is the default".into(),
            page: "No browser session.\n\nSTART HERE\n  [l] launch a local browser on a free port\n  [a] attach an existing Chrome on a DevTools port\n  [n] enter an address to navigate\n  [?] show the command reference".into(),
            session: None,
            workspace: BrowserWorkspaceController::for_adapter(
                BrowserWorkspaceLayout::Desktop,
                BrowserWorkspaceAdapterKind::Standalone,
            ),
            graphics,
            visual,
            last_workflow: None,
            workflow_checkpoint: None,
        }
    }

    async fn submit(&mut self, cli: &Cli) -> BrowserResult<bool> {
        let command = std::mem::take(&mut self.command);
        let command = command.trim();
        if command.is_empty() {
            return Ok(false);
        }
        if matches!(command, "quit" | "exit" | "q") {
            return Ok(true);
        }
        if command == "help" {
            self.mode = WorkspaceMode::Help;
            self.status = "Browser TUI command reference".into();
            return Ok(false);
        }
        if command == "semantic" {
            self.mode = WorkspaceMode::Semantic;
            return self.observe().await.map(|_| false);
        }
        if command == "observe" {
            self.mode = WorkspaceMode::Browser;
            return self.observe().await.map(|_| false);
        }
        if let Some(text) = command.strip_prefix("type ") {
            let action = self
                .workspace
                .reduce(BrowserWorkspaceIntent::TypeSelected {
                    text: text.to_string(),
                })
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            let Some(crate::browser_workspace::BrowserWorkspaceAction::Type {
                target,
                text,
                expected_revision,
            }) = action
            else {
                return Ok(false);
            };
            self.session
                .as_ref()
                .ok_or("browser is detached")?
                .type_text_with_expected_revision(&text, target.as_deref(), Some(expected_revision))
                .await?;
            self.observe().await?;
            self.status = "Text sent to selected semantic target".into();
            return Ok(false);
        }
        if let Some(dy) = command.strip_prefix("scroll ") {
            let dy = dy
                .trim()
                .parse::<f64>()
                .map_err(|_| "scroll requires a pixel delta such as `scroll 600`")?;
            let action = self
                .workspace
                .reduce(BrowserWorkspaceIntent::ScrollBrowser { dx: 0.0, dy })
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            let Some(crate::browser_workspace::BrowserWorkspaceAction::Scroll {
                dx,
                dy,
                expected_revision,
            }) = action
            else {
                return Ok(false);
            };
            self.session
                .as_ref()
                .ok_or("browser is detached")?
                .scroll_with_revision(dx, dy, Some(expected_revision))
                .await?;
            self.observe().await?;
            self.status = format!("Scrolled page by {dy:.0}px");
            return Ok(false);
        }
        if command == "state" {
            self.status = format!(
                "{} · generation {} · revision {}",
                self.workspace.state().connection_label(),
                self.workspace.state().generation,
                self.workspace
                    .state()
                    .browser_revision
                    .map_or_else(|| "—".into(), |revision| revision.to_string())
            );
            return Ok(false);
        }
        if command == "targets" {
            let session = self.session.as_ref().ok_or("browser is detached")?;
            let targets = session.list_targets().await?;
            self.workspace.replace_targets(
                targets
                    .into_iter()
                    .map(|target| crate::browser_workspace::BrowserWorkspaceTarget {
                        id: target.id,
                        title: target.title,
                        url: target.url,
                        selected: target.active,
                    })
                    .collect(),
            );
            self.page = self
                .workspace
                .state()
                .targets
                .iter()
                .enumerate()
                .map(|(index, target)| {
                    format!(
                        "[{}] {} {} · {}\n  {}",
                        index + 1,
                        if target.selected { "◆" } else { "○" },
                        target.title,
                        target.url,
                        target.id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(false);
        }
        if let Some(target_id) = command.strip_prefix("select ") {
            let session = self.session.as_ref().ok_or("browser is detached")?;
            session.select_target(target_id.trim()).await?;
            self.observe().await?;
            self.status = "Target selected · prior semantic references invalidated".into();
            return Ok(false);
        }
        if command == "stop" {
            if let Some(session) = self.session.take() {
                session.close().await?;
            }
            self.workspace.state_mut().connection = BrowserConnectionPhase::Detached;
            self.workspace.state_mut().browser_revision = None;
            self.status = "Browser stopped; workspace remains open".into();
            return Ok(false);
        }
        if command == "reconnect" {
            if let Some(session) = self.session.take() {
                session.close().await?;
            }
            self.ensure_session(cli).await?;
            self.observe().await?;
            self.status = "Browser reconnected in place".into();
            return Ok(false);
        }
        if command == "launch auto" {
            if let Some(session) = self.session.take() {
                session.close().await?;
            }
            let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
            let port = listener.local_addr()?.port();
            drop(listener);
            self.start_at(cli, port, false).await?;
            self.status = format!("Browser launched on automatic free port {port}");
            return Ok(false);
        }
        if let Some(port) = command.strip_prefix("launch ") {
            let port = port
                .trim()
                .parse::<u16>()
                .map_err(|_| "launch requires a valid port or `auto`")?;
            if let Some(session) = self.session.take() {
                session.close().await?;
            }
            self.start_at(cli, port, false).await?;
            self.status = format!("Browser launched on explicit port {port}");
            return Ok(false);
        }
        if let Some(port) = command.strip_prefix("attach ") {
            let port = port
                .trim()
                .parse::<u16>()
                .map_err(|_| "attach requires a valid DevTools port")?;
            if let Some(session) = self.session.take() {
                session.close().await?;
            }
            self.start_at(cli, port, true).await?;
            self.status = format!("Attached to verified DevTools on port {port}");
            return Ok(false);
        }
        if command == "screenshot" {
            self.refresh_visual(80, 24).await?;
            self.status = "Explicit screenshot captured for the selected presentation path".into();
            return Ok(false);
        }
        if command == "live on" {
            if let Some(reason) = self.visual.request_live(true) {
                self.status = format!("Live view unavailable · {reason}");
            } else {
                self.status = format!("Live view enabled · {}", self.visual.label());
            }
            self.sync_presentation_state();
            return Ok(false);
        }
        if command == "live off" {
            self.visual.request_live(false);
            self.sync_presentation_state();
            self.status = "Continuous visual presentation disabled".into();
            return Ok(false);
        }
        if command == "workflow list" {
            self.page = self.last_workflow.as_ref().map_or_else(
                || "No workflow run in this workspace".into(),
                |(definition, result)| {
                    format!(
                        "{} · {} · final revision {} · {} steps",
                        definition.name,
                        result.status.label(),
                        result.final_revision,
                        result.steps.len()
                    )
                },
            );
            self.status = "Workflow history".into();
            return Ok(false);
        }
        if let Some(path) = command.strip_prefix("workflow run ") {
            let document: serde_json::Value = serde_json::from_slice(&std::fs::read(path.trim())?)?;
            let definition = WorkflowDefinition::from_value(document)?;
            let session = self.session.as_ref().ok_or("browser is detached")?;
            let result = session.run_workflow(&definition, &BTreeMap::new()).await?;
            self.workspace.state_mut().browser_revision = Some(result.final_revision);
            self.workspace.state_mut().workflow = result.status.label().to_string();
            self.page = format!(
                "{} · {} · final revision {} · {} steps",
                definition.name,
                result.status.label(),
                result.final_revision,
                result.steps.len()
            );
            self.last_workflow = Some((definition, result));
            self.status = "Workflow completed; run `workflow verify`".into();
            return Ok(false);
        }
        if command == "workflow pause" {
            let (definition, result) = self
                .last_workflow
                .as_ref()
                .ok_or("no workflow result to checkpoint")?;
            let session = self.session.as_ref().ok_or("browser is detached")?;
            self.workflow_checkpoint = Some(
                session
                    .export_workflow_checkpoint(definition, result)
                    .await?,
            );
            self.workspace.state_mut().workflow = "paused".into();
            self.status = "Workflow checkpoint retained in this workspace".into();
            return Ok(false);
        }
        if let Some(path) = command.strip_prefix("workflow resume ") {
            let document: serde_json::Value = serde_json::from_slice(&std::fs::read(path.trim())?)?;
            let definition = WorkflowDefinition::from_value(document)?;
            let checkpoint = self
                .workflow_checkpoint
                .as_ref()
                .ok_or("no workflow checkpoint to resume")?;
            let session = self.session.as_ref().ok_or("browser is detached")?;
            let result = session
                .resume_workflow(&definition, &BTreeMap::new(), checkpoint)
                .await?;
            self.workspace.state_mut().browser_revision = Some(result.final_revision);
            self.workspace.state_mut().workflow = result.status.label().to_string();
            self.last_workflow = Some((definition, result));
            self.status = "Workflow resumed to a terminal result".into();
            return Ok(false);
        }
        if command == "workflow cancel" {
            self.workflow_checkpoint = None;
            self.workspace.state_mut().workflow = "cancelled".into();
            self.status = "Retained workflow continuation cancelled".into();
            return Ok(false);
        }
        if command == "workflow verify" {
            let (_, result) = self
                .last_workflow
                .as_ref()
                .ok_or("no workflow result to verify")?;
            self.page = format!(
                "{} · final revision {} · {} recorded steps",
                if result.status.label() == "completed" {
                    "✓ verified"
                } else {
                    "× not completed"
                },
                result.final_revision,
                result.steps.len()
            );
            self.status = "Workflow verification projected".into();
            return Ok(false);
        }
        if let Some(url) = command.strip_prefix("navigate ") {
            self.ensure_session(cli).await?;
            let page = self
                .session
                .as_ref()
                .expect("session initialized")
                .navigate(url.trim())
                .await?;
            self.workspace.update_page(
                page.title.clone(),
                page.url.clone(),
                false,
                self.workspace.state().browser_revision,
            );
            self.page = format!("{}\n{}", page.title, page.url);
            self.status = "Navigation complete · run `observe` for structured evidence".into();
            return Ok(false);
        }
        self.status = format!("Unknown command `{command}` · enter `help`");
        Ok(false)
    }

    async fn ensure_session(&mut self, cli: &Cli) -> BrowserResult<()> {
        if self.session.is_none() {
            self.start_at(cli, cli.port, cli.attach).await?;
        }
        Ok(())
    }

    async fn start_at(&mut self, cli: &Cli, port: u16, attach: bool) -> BrowserResult<()> {
        if self.session.is_none() {
            let options = SessionOptions {
                port,
                chrome_path: cli.chrome_path.clone(),
                profile: cli.profile.clone(),
                incognito: cli.incognito,
                attach,
                target_id: cli.target_id.clone(),
                frame_id: cli.frame_id.clone(),
                headed: cli.headed,
                interaction_mode: cli.interaction,
                audit: cli.audit,
                policy: Some(crate::cli::runner::policy_from_cli(cli)?),
            };
            self.session = Some(BrowserSession::start(&options).await?);
            self.workspace
                .connected(!attach, Some(format!("127.0.0.1:{port}")), None);
        }
        Ok(())
    }

    async fn observe(&mut self) -> BrowserResult<()> {
        let Some(session) = self.session.as_ref() else {
            self.status = "Navigate first; observation never starts Chrome implicitly".into();
            return Ok(());
        };
        let observation = session.observe().await?;
        let revision = observation.accessibility.revision;
        self.workspace.update_page(
            observation.page.title.clone(),
            observation.page.url.clone(),
            observation.page.ready_state != "complete",
            Some(revision),
        );
        self.workspace.replace_entities(
            revision,
            observation
                .accessibility
                .interactive
                .iter()
                .map(|entity| BrowserWorkspaceEntity {
                    reference: entity.reference.clone(),
                    role: entity.role.clone(),
                    name: entity.name.clone(),
                    actionable: true,
                    revision,
                })
                .collect(),
        );
        self.page = semantic_text(&self.workspace);
        self.status = format!(
            "Structured observation · revision {}",
            observation.accessibility.revision
        );
        Ok(())
    }

    async fn close(&mut self) -> BrowserResult<()> {
        if let Some(session) = self.session.take() {
            session.close().await?;
        }
        self.workspace.state_mut().connection = BrowserConnectionPhase::Detached;
        Ok(())
    }

    async fn activate_selected(&mut self) -> BrowserResult<()> {
        let action = self
            .workspace
            .reduce(BrowserWorkspaceIntent::ActivateSelected)
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
        let Some(crate::browser_workspace::BrowserWorkspaceAction::Click {
            target,
            expected_revision,
        }) = action
        else {
            return Ok(());
        };
        let Some(session) = self.session.as_ref() else {
            return Err("browser is detached".into());
        };
        match session
            .click_with_revision(&target, expected_revision)
            .await
        {
            Ok(_) => self.observe().await,
            Err(error) => {
                let stale = error.to_string().to_lowercase().contains("stale");
                self.workspace.fail_action(error.to_string(), stale);
                self.status = format!("Action failed: {error}");
                Ok(())
            }
        }
    }

    async fn execute_control(&mut self, intent: BrowserWorkspaceIntent) -> BrowserResult<()> {
        let action = self
            .workspace
            .reduce(intent)
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
        let session = self.session.as_ref().ok_or("browser is detached")?;
        let outcome = match action {
            Some(crate::browser_workspace::BrowserWorkspaceAction::Back { expected_revision }) => {
                session.go_back_with_revision(expected_revision).await?
            }
            Some(crate::browser_workspace::BrowserWorkspaceAction::Forward {
                expected_revision,
            }) => session.go_forward_with_revision(expected_revision).await?,
            Some(crate::browser_workspace::BrowserWorkspaceAction::Reload {
                expected_revision,
            }) => session.reload_with_revision(expected_revision).await?,
            Some(crate::browser_workspace::BrowserWorkspaceAction::StopLoading {
                expected_revision,
            }) => {
                session
                    .stop_loading_with_revision(expected_revision)
                    .await?
            }
            _ => return Ok(()),
        };
        self.workspace.state_mut().browser_revision = Some(outcome.current_revision);
        self.status = format!(
            "{} complete · observe revision {}",
            outcome.action, outcome.current_revision
        );
        Ok(())
    }

    async fn refresh_visual(&mut self, columns: u16, rows: u16) -> BrowserResult<()> {
        let Some(session) = self.session.as_ref() else {
            return Err("browser is detached".into());
        };
        match self
            .visual
            .path
            .clone()
            .unwrap_or(VisualPath::SemanticOnly {
                reason: "visual policy not configured".into(),
            }) {
            VisualPath::Herdr => {
                let Some(graphics) = self.graphics.as_ref() else {
                    self.workspace.state_mut().presentation_reason =
                        Some("Herdr environment was not detected".into());
                    return Ok(());
                };
                let png = session.screenshot_png().await?;
                let (image_width, image_height) = png_dimensions(&png).unwrap_or((1, 1));
                if graphics.try_send(HerdrFrame {
                    png,
                    image_width,
                    image_height,
                    viewport_col: 0,
                    viewport_row: 3,
                    grid_cols: u32::from(columns),
                    grid_rows: u32::from(rows.saturating_sub(6).max(1)),
                }) {
                    self.workspace.state_mut().frame_revision =
                        self.workspace.state().browser_revision;
                }
                Ok(())
            }
            VisualPath::Ansi => {
                let png = session.screenshot_png().await?;
                let (columns, rows) = pane_size(self.visual.quality, (columns, rows));
                match AnsiPane::from_png(
                    &mut self.visual.ansi_canvas,
                    &png,
                    columns,
                    rows,
                    frame_fit(self.visual.fit),
                ) {
                    Ok(pane) => {
                        self.visual.ansi_pane = Some(pane);
                        self.workspace.state_mut().frame_revision =
                            self.workspace.state().browser_revision;
                        Ok(())
                    }
                    Err(error) => {
                        self.visual.request_live(false);
                        self.workspace.state_mut().presentation_reason =
                            Some(format!("ANSI renderer failed: {error}"));
                        Ok(())
                    }
                }
            }
            VisualPath::SemanticOnly { reason } => {
                self.workspace.state_mut().presentation_reason = Some(reason);
                Ok(())
            }
        }
    }

    /// Mirror the decided visual path into workspace presentation state.
    fn sync_presentation_state(&mut self) {
        let (presentation, reason) = match &self.visual.path {
            Some(VisualPath::Herdr) if self.visual.live => (
                crate::browser_workspace::BrowserPresentationPath::Herdr,
                None,
            ),
            Some(VisualPath::Ansi) if self.visual.live => (
                crate::browser_workspace::BrowserPresentationPath::Ansi,
                None,
            ),
            Some(VisualPath::SemanticOnly { reason }) => (
                crate::browser_workspace::BrowserPresentationPath::SemanticOnly,
                Some(reason.clone()),
            ),
            _ => (
                crate::browser_workspace::BrowserPresentationPath::SemanticOnly,
                Some("continuous visual presentation disabled".into()),
            ),
        };
        self.workspace.state_mut().presentation = presentation;
        self.workspace.state_mut().presentation_reason = reason;
    }

    fn poll_graphics(&mut self) {
        let Some(graphics) = self.graphics.as_ref() else {
            return;
        };
        while let Some(event) = graphics.try_event() {
            match event {
                HerdrEvent::Connected => {
                    self.workspace.state_mut().presentation =
                        crate::browser_workspace::BrowserPresentationPath::Herdr;
                    self.workspace.state_mut().presentation_reason = None;
                }
                HerdrEvent::Failed(reason) => {
                    self.visual.request_live(false);
                    self.workspace.state_mut().presentation =
                        crate::browser_workspace::BrowserPresentationPath::SemanticOnly;
                    self.workspace.state_mut().presentation_reason = Some(reason);
                }
                HerdrEvent::Stopped => {
                    self.visual.request_live(false);
                }
            }
        }
    }
}

pub async fn run_tui(cli: &Cli) -> BrowserResult<()> {
    run_tui_for_product(cli, false).await
}

/// Run the focused browser TUI.
///
/// The argument remains for source compatibility with callers from before the
/// product split; requesting development mode now fails explicitly.
pub async fn run_tui_for_product(cli: &Cli, development_enabled: bool) -> BrowserResult<()> {
    if development_enabled {
        return Err(
            "development TUI surfaces belong to the `glass` binary from `glass-dev`".into(),
        );
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("browser TUI requires an interactive terminal".into());
    }
    let mut terminal = TerminalGuard::enter()?;
    let mut app = BrowserTui::new(cli);
    app.status = if app.visual.live {
        format!(
            "Ready · {} · n address · Enter activates the selection",
            app.visual.label()
        )
    } else {
        "Ready · n enters an address · `navigate URL` starts a browser · `attach PORT` reuses one · help lists all"
            .into()
    };
    let mut last_visual = Instant::now();
    let result: BrowserResult<()> = loop {
        app.poll_graphics();
        terminal.terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('c')
                        if key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        break Ok(());
                    }
                    KeyCode::Char('q') => break Ok(()),
                    KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                        if let Err(error) = app.execute_control(BrowserWorkspaceIntent::Back).await
                        {
                            app.status = format!("Back failed: {error}");
                        }
                    }
                    KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                        if let Err(error) =
                            app.execute_control(BrowserWorkspaceIntent::Forward).await
                        {
                            app.status = format!("Forward failed: {error}");
                        }
                    }
                    KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Err(error) =
                            app.execute_control(BrowserWorkspaceIntent::Reload).await
                        {
                            app.status = format!("Reload failed: {error}");
                        }
                    }
                    KeyCode::Esc => {
                        app.command.clear();
                        if app.mode != WorkspaceMode::Browser {
                            app.mode = WorkspaceMode::Browser;
                            app.status = "Browser view restored".into();
                        } else {
                            let _ = app.workspace.reduce(BrowserWorkspaceIntent::CloseOverlay);
                        }
                    }
                    KeyCode::Char('?') if app.command.is_empty() => {
                        app.mode = WorkspaceMode::Help;
                        app.status = "Browser TUI command reference · Esc returns".into();
                    }
                    KeyCode::Char('l') if app.command.is_empty() => {
                        app.command = "launch auto".into();
                        match app.submit(cli).await {
                            Ok(true) | Ok(false) => {}
                            Err(error) => {
                                app.workspace.disconnected(error.to_string(), true);
                                app.status = format!(
                                    "Launch failed: {error} · press l to retry or a to attach"
                                );
                            }
                        }
                    }
                    KeyCode::Char('a') if app.command.is_empty() => {
                        app.command = "attach ".into();
                        app.status = "Attach entry · type a DevTools port · Enter connects".into();
                    }
                    KeyCode::Char('n') if app.command.is_empty() => {
                        app.command = "navigate ".into();
                        app.status = "Address entry · type URL · Enter navigates".into();
                    }
                    KeyCode::Char('t') if app.command.is_empty() => {
                        app.command = "type ".into();
                        app.status = "Type into selected semantic target · Enter sends".into();
                    }
                    KeyCode::Enter if app.command.is_empty() => {
                        if let Err(error) = app.activate_selected().await {
                            app.status = format!("Action failed: {error}");
                        }
                    }
                    KeyCode::Enter => match app.submit(cli).await {
                        Ok(true) => break Ok(()),
                        Ok(false) => {}
                        Err(error) => {
                            app.workspace.disconnected(error.to_string(), true);
                            app.status = format!(
                                "Command failed: {error} · reconnect, launch auto, or launch PORT"
                            );
                        }
                    },
                    KeyCode::Backspace => {
                        app.command.pop();
                    }
                    KeyCode::Tab => {
                        let _ = app.workspace.reduce(BrowserWorkspaceIntent::MoveFocus {
                            backwards: key.modifiers.contains(KeyModifiers::SHIFT),
                        });
                    }
                    KeyCode::Up | KeyCode::Char('k') if app.command.is_empty() => {
                        let _ = app
                            .workspace
                            .reduce(BrowserWorkspaceIntent::MoveSelection { delta: -1 });
                        app.page = semantic_text(&app.workspace);
                        app.status = "Selection moved · Enter activates the target".into();
                    }
                    KeyCode::Down | KeyCode::Char('j') if app.command.is_empty() => {
                        let _ = app
                            .workspace
                            .reduce(BrowserWorkspaceIntent::MoveSelection { delta: 1 });
                        app.page = semantic_text(&app.workspace);
                        app.status = "Selection moved · Enter activates the target".into();
                    }
                    KeyCode::Char(':') if app.command.is_empty() => {
                        app.status = "Command line · type help for the command reference".into();
                    }
                    KeyCode::Char(character) => app.command.push(character),
                    _ => {}
                },
                Event::Paste(text) => {
                    let normalized = text.replace(['\r', '\n'], " ");
                    app.command.extend(normalized.chars().take(8_192));
                }
                Event::Mouse(mouse) => match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollUp => {
                        let _ = app
                            .workspace
                            .reduce(BrowserWorkspaceIntent::MoveSelection { delta: -1 });
                        app.page = semantic_text(&app.workspace);
                        app.status = "Selection moved · Enter activates the target".into();
                    }
                    crossterm::event::MouseEventKind::ScrollDown => {
                        let _ = app
                            .workspace
                            .reduce(BrowserWorkspaceIntent::MoveSelection { delta: 1 });
                        app.page = semantic_text(&app.workspace);
                        app.status = "Selection moved · Enter activates the target".into();
                    }
                    // Left click on a semantic line selects it; clicking the
                    // command row focuses nothing (keyboard-first footer).
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        let semantic_top = semantic_first_row(&app.workspace);
                        let scrolled = app.workspace.state().semantic_scroll;
                        if mouse.row >= semantic_top {
                            let index = usize::from(mouse.row - semantic_top) + scrolled;
                            if app.workspace.state().entities.get(index).is_some() {
                                app.workspace.state_mut().selected_entity = Some(index);
                                app.page = semantic_text(&app.workspace);
                                app.status = "Selection moved · Enter activates the target".into();
                            }
                        }
                    }
                    _ => {}
                },
                Event::FocusLost => {
                    let _ = app.workspace.reduce(BrowserWorkspaceIntent::CloseOverlay);
                }
                Event::Resize(_, _) | Event::FocusGained | Event::Key(_) => {}
            }
        }
        if app.visual.live
            && last_visual.elapsed() >= Duration::from_millis(frame_interval_ms(app.visual.quality))
        {
            let size = terminal.terminal.size()?;
            if let Err(error) = app.refresh_visual(size.width, size.height).await {
                app.status = format!("Visual refresh failed: {error}");
                app.visual.request_live(false);
            }
            last_visual = Instant::now();
        }
    };
    let close = app.close().await;
    result?;
    close
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &BrowserTui) {
    let area = frame.area();
    let class = ResponsiveClass::for_width(area.width);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            // Three header lines plus a border need five rows; the previous
            // three-row header silently clipped both status lines.
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
    let title = match class {
        ResponsiveClass::Phone => "GLASS BROWSER · PHONE",
        ResponsiveClass::Compact => "GLASS BROWSER · COMPACT",
        ResponsiveClass::Desktop => "GLASS BROWSER · DESKTOP",
    };
    let workspace = app.workspace.state();
    let status = format!(
        "{} · rev {} · {} · owner {} · focus {}",
        workspace.connection_label(),
        workspace
            .browser_revision
            .map_or_else(|| "—".into(), |revision| revision.to_string()),
        workspace.presentation_label(),
        workspace.input_owner_label(),
        workspace.focus_label(),
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(status),
            Line::from(app.status.as_str()),
        ])
        .block(Block::default().borders(Borders::ALL)),
        rows[0],
    );
    let content = if app.mode == WorkspaceMode::Help {
        "START HERE\n[l] launch auto · [a] attach PORT · [n] navigate URL · [t] type text\n[j/k] select semantic target · Enter activate · Esc return\n\nCOMMANDS\nnavigate URL  start/navigate browser\nobserve       structured accessibility evidence\nsemantic      structured semantic view\ntargets       bounded page targets\nselect ID     change target and invalidate evidence\nstate         connection/revision status\nreconnect     recover in place\nattach PORT   attach verified DevTools\nlaunch auto   recover on a free port\nlaunch PORT   recover on an explicit port\nstop          stop browser, keep workspace\nscreenshot    explicit frame capture\nlive on|off   continuous pixels (Herdr or ANSI per policy)\nworkflow list|run FILE|pause|resume FILE|cancel|verify\nquit          close owned browser and exit"
    } else {
        app.page.as_str()
    };
    draw_content(frame, rows[1], content, class);
    if let Some(pane) = app.visual.ansi_pane.as_ref() {
        draw_ansi_pane(frame, rows[1], pane);
    }
    frame.render_widget(
        Paragraph::new(format!(
            "{} · {} · > {}",
            app.visual.label(),
            app.status,
            app.command
        ))
        .block(Block::default().title("COMMAND").borders(Borders::ALL)),
        rows[2],
    );
}

/// Paint an ANSI half-block pane inside the content area, below its border.
fn draw_ansi_pane(frame: &mut ratatui::Frame<'_>, area: Rect, pane: &AnsiPane) {
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    for (row_index, row_cells) in pane.cells.chunks(pane.columns as usize).enumerate() {
        let y = inner.y + row_index as u16;
        if y >= inner.bottom() {
            break;
        }
        for (column_index, cell) in row_cells.iter().enumerate() {
            let x = inner.x + column_index as u16;
            if x >= inner.right() {
                break;
            }
            if let Some(target) = frame.buffer_mut().cell_mut((x, y)) {
                target.set_char('▀');
                target.set_fg(ratatui::style::Color::Rgb(
                    cell.top.red,
                    cell.top.green,
                    cell.top.blue,
                ));
                target.set_bg(ratatui::style::Color::Rgb(
                    cell.bottom.red,
                    cell.bottom.green,
                    cell.bottom.blue,
                ));
            }
        }
    }
}

fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((
        u32::from_be_bytes(png[16..20].try_into().ok()?),
        u32::from_be_bytes(png[20..24].try_into().ok()?),
    ))
}

fn semantic_text(workspace: &BrowserWorkspaceController) -> String {
    let state = workspace.state();
    if state.entities.is_empty() {
        return if state.semantic_invalidated {
            "Semantic evidence is stale or unavailable. Run `observe`.".into()
        } else {
            "No interactive entities in the current observation.".into()
        };
    }
    state
        .entities
        .iter()
        .enumerate()
        .skip(state.semantic_scroll)
        .take(32)
        .map(|(index, entity)| {
            format!(
                "[{}] {} {} · {}",
                index + 1,
                if Some(index) == state.selected_entity {
                    "◆"
                } else {
                    "○"
                },
                entity.name,
                entity.role
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn draw_content(frame: &mut ratatui::Frame<'_>, area: Rect, content: &str, class: ResponsiveClass) {
    let title = match class {
        ResponsiveClass::Phone => "Overview · pixels opt-in",
        ResponsiveClass::Compact => "Browser evidence · help for keys",
        ResponsiveClass::Desktop => {
            "Browser / Structured Observation · n address · Enter activate · j/k select · help for all"
        }
    };
    frame.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .block(Block::default().title(title).borders(Borders::ALL)),
        area,
    );
}

/// First terminal row (inside the content border) that shows semantic entities.
fn semantic_first_row(workspace: &BrowserWorkspaceController) -> u16 {
    let state = workspace.state();
    let header_rows = 1 + usize::from(!state.title.is_empty()) + usize::from(!state.url.is_empty());
    3 + 1 + header_rows as u16
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
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(stdout))?,
        })
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
    fn responsive_classes_preserve_phone_compact_and_desktop_boundaries() {
        assert_eq!(ResponsiveClass::for_width(40), ResponsiveClass::Phone);
        assert_eq!(ResponsiveClass::for_width(72), ResponsiveClass::Phone);
        assert_eq!(ResponsiveClass::for_width(73), ResponsiveClass::Compact);
        assert_eq!(ResponsiveClass::for_width(109), ResponsiveClass::Compact);
        assert_eq!(ResponsiveClass::for_width(110), ResponsiveClass::Desktop);
    }

    #[test]
    fn browser_tui_starts_structured_first_and_without_a_session() {
        let cli = test_cli(&[], TuiLiveMode::Off);
        let app = BrowserTui::new(&cli);
        assert!(app.status.contains("structured observation"));
        assert!(app.session.is_none());
        assert_eq!(app.mode, WorkspaceMode::Browser);
        assert!(!app.visual.live);
    }

    #[test]
    fn browser_tui_welcome_shows_launch_attach_and_navigation_actions() {
        let app = BrowserTui::new(&test_cli(&[], TuiLiveMode::Off));
        assert!(app.page.contains("[l] launch a local browser"));
        assert!(app.page.contains("[a] attach an existing Chrome"));
        assert!(app.page.contains("[n] enter an address"));
    }

    #[test]
    fn semantic_entities_use_compact_local_refs_for_humans() {
        let mut app = BrowserTui::new(&test_cli(&[], TuiLiveMode::Off));
        app.workspace.replace_entities(
            7,
            vec![BrowserWorkspaceEntity {
                reference: "r7:b42".into(),
                role: "button".into(),
                name: "Save".into(),
                actionable: true,
                revision: 7,
            }],
        );
        assert_eq!(semantic_text(&app.workspace), "[1] ◆ Save · button");
    }

    #[test]
    fn live_on_uses_ansi_fallback_when_herdr_is_absent() {
        let cli = test_cli(&["--tui-live", "on"], TuiLiveMode::On);
        let mut app = BrowserTui::new(&cli);
        assert_eq!(app.visual.path, Some(VisualPath::Ansi));
        app.visual.request_live(true);
        assert!(app.visual.live);
        app.sync_presentation_state();
        assert_eq!(
            app.workspace.state().presentation,
            crate::browser_workspace::BrowserPresentationPath::Ansi
        );
    }

    fn test_cli(extra: &[&str], _mode: TuiLiveMode) -> Cli {
        let mut base: Cli =
            clap::Parser::parse_from(std::iter::once("glass-browser").chain(extra.iter().copied()));
        base.tui_layout = crate::cli::args::TuiLayout::Desktop;
        base
    }
}

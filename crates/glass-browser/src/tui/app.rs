//! Interactive TUI application state and rendering.
//!
//! Implements the Ratatui-based terminal interface with split-pane layout,
//! command input, observation display, and keyboard-driven interaction.

use base64::Engine as _;
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    io::{self, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc as std_mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, watch},
    task::{JoinHandle, LocalSet},
    time::{self, MissedTickBehavior},
};

use crate::capabilities::GlassCapabilityManifest;
use crate::development::{
    Actor, AttentionState, HarnessRequest, LocalHarness, PiHarness, ProcessState, ProjectWorkspace,
    ReconnectCapsule, ReconnectCapsuleStore, VerificationCard, attention_inbox,
};

use crate::browser::connection::{
    EndpointClassification, EndpointProbe, probe_local_endpoint, reserve_loopback_port,
};
use crate::browser::policy::BrowserPolicy;
use crate::browser::profile::ProfileManager;
use crate::browser::session::{
    ActionOutcome, BrowserResult, BrowserSession, KnowledgeStore, PageContext, PageInfo,
    ScreencastScope, SemanticIntentExecutionRequest, SemanticIntentExecutionResult,
    SemanticIntentRequest, SemanticIntentResult, SemanticObservation, SemanticObservationLevel,
    SessionOptions, VisualFormat, WorkflowDefinition, default_knowledge_store_path,
};
use crate::cli::args::{
    Cli, TuiGraphics, TuiLayout, TuiLiveBackend, TuiLiveFit, TuiLiveMode, TuiLiveQuality,
    TuiTransport,
};
use crate::connection::{
    ActivityClass, ConnectionEnvironment, ConnectionMeasurements, ConnectionOverrides,
    ConnectionSignals, GraphicsClass, LayoutClass, PixelIntent, PresentationPolicy, QualityIntent,
    TransportClass,
};
use crate::presentation::{BrowserFrame, CaptureScale, PixelSize, TargetResourceIdentity};
use crate::terminal_graphics::{
    AnsiCanvas, FrameFit, GraphicsMode, MAX_FRAME_BYTES, PaneArea, SubmitResult, TerminalGraphics,
};
use crate::tui::herdr_graphics::{HerdrEnvironment, HerdrEvent, HerdrFrame, HerdrGraphicsWorker};
const INPUT_CHANNEL_CAPACITY: usize = 64;
const BROWSER_COMMAND_CHANNEL_CAPACITY: usize = 8;
const BROWSER_EVENT_CHANNEL_CAPACITY: usize = 2;
const MAX_VISUAL_ENCODED_BYTES: usize = 6 * 1024 * 1024;
const ACTIVITY_LIMIT: usize = 100;
const TUI_PAGE_MAX_BYTES: usize = 24 * 1024;
const TUI_HEADER_MAX_BYTES: usize = 512;
const TUI_ACTIVITY_MAX_BYTES: usize = 512;
const PHONE_MAX_COLUMNS: u16 = 72;
const COMPACT_MAX_COLUMNS: u16 = 109;
const LIVE_CAPTURE_MAX_WIDTH: u32 = 1280;
const LIVE_CAPTURE_MAX_HEIGHT: u32 = 1024;
/// Visible Browser Workspace presentation modes. Each mode changes the
/// inspection surface, but never changes browser authority or policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceMode {
    #[default]
    Browser,
    Split,
    Workflow,
    Semantic,
    Inspect,
    Takeover,
    Development,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayClass {
    Phone,
    Compact,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MobileView {
    #[default]
    Home,
    Agent,
    App,
    Diff,
    Project,
    Process,
}

impl MobileView {
    const fn number(self) -> u8 {
        match self {
            Self::Home => 1,
            Self::Agent => 2,
            Self::App => 3,
            Self::Diff => 4,
            Self::Project => 5,
            Self::Process => 6,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Home => "Overview",
            Self::Agent => "Agent",
            Self::App => "Browser",
            Self::Diff => "Diff",
            Self::Project => "Project",
            Self::Process => "Process",
        }
    }

    const fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(Self::Home),
            2 => Some(Self::Agent),
            3 => Some(Self::App),
            4 => Some(Self::Diff),
            5 => Some(Self::Project),
            6 => Some(Self::Process),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RemoteContext {
    ssh: bool,
    mosh: bool,
    herdr: bool,
    tmux: bool,
    screen: bool,
}

impl RemoteContext {
    fn from_process() -> Self {
        Self {
            ssh: std::env::var_os("SSH_CONNECTION").is_some()
                || std::env::var_os("SSH_TTY").is_some(),
            mosh: std::env::var_os("MOSH_CONNECTION").is_some(),
            herdr: std::env::var("HERDR_ENV").is_ok_and(|value| value == "1"),
            tmux: std::env::var_os("TMUX").is_some(),
            screen: std::env::var_os("STY").is_some(),
        }
    }

    fn label(self) -> &'static str {
        if self.herdr {
            "Herdr"
        } else if self.mosh {
            "Mosh"
        } else if self.ssh {
            "SSH"
        } else {
            "local"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveLiveBackend {
    Herdr,
    Kitty,
    Ansi,
}

impl ActiveLiveBackend {
    const fn label(self) -> &'static str {
        match self {
            Self::Herdr => "Herdr graphics",
            Self::Kitty => "Kitty graphics",
            Self::Ansi => "ANSI half blocks",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreencastConfig {
    enabled: bool,
    max_width: u32,
    max_height: u32,
    minimum_interval: Duration,
    requested_fps: u16,
    capture_scale: f32,
}

impl Default for ScreencastConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_width: 320,
            max_height: 240,
            minimum_interval: Duration::from_millis(333),
            requested_fps: 0,
            capture_scale: 0.5,
        }
    }
}

#[derive(Debug)]
struct LiveMetrics {
    window_started: Instant,
    bytes: u64,
    acquired_frames: u64,
    presented_frames: u64,
    dropped: u64,
    window_dropped: u64,
    drop_ratio: f64,
    generation: u64,
    acquisition_fps: f64,
    presentation_fps: f64,
    bytes_per_second: f64,
}

impl Default for LiveMetrics {
    fn default() -> Self {
        Self {
            window_started: Instant::now(),
            bytes: 0,
            acquired_frames: 0,
            presented_frames: 0,
            dropped: 0,
            window_dropped: 0,
            drop_ratio: 0.0,
            generation: 0,
            acquisition_fps: 0.0,
            presentation_fps: 0.0,
            bytes_per_second: 0.0,
        }
    }
}

impl LiveMetrics {
    fn received(&mut self, bytes: usize) {
        self.bytes = self.bytes.saturating_add(bytes as u64);
        self.acquired_frames = self.acquired_frames.saturating_add(1);
        self.refresh();
    }

    fn presented(&mut self) {
        self.presented_frames = self.presented_frames.saturating_add(1);
        self.refresh();
    }

    fn dropped(&mut self) {
        self.dropped = self.dropped.saturating_add(1);
        self.window_dropped = self.window_dropped.saturating_add(1);
        self.refresh();
    }

    fn refresh(&mut self) {
        let elapsed = self.window_started.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let seconds = elapsed.as_secs_f64();
            self.acquisition_fps = self.acquired_frames as f64 / seconds;
            self.presentation_fps = self.presented_frames as f64 / seconds;
            self.bytes_per_second = self.bytes as f64 / seconds;
            let attempts = self.acquired_frames.saturating_add(self.window_dropped);
            self.drop_ratio = if attempts == 0 {
                0.0
            } else {
                self.window_dropped as f64 / attempts as f64
            };
            self.generation = self.generation.saturating_add(1);
            self.window_started = Instant::now();
            self.bytes = 0;
            self.acquired_frames = 0;
            self.presented_frames = 0;
            self.window_dropped = 0;
        }
    }
}

struct LiveViewState {
    mode: TuiLiveMode,
    preference: TuiLiveBackend,
    quality: TuiLiveQuality,
    effective_quality: TuiLiveQuality,
    adaptive_quality: bool,
    adaptive_scale_step: u8,
    stable_windows: u8,
    adapted_generation: u64,
    fit: TuiLiveFit,
    backend: Option<ActiveLiveBackend>,
    kitty_detected: bool,
    herdr_environment: Option<HerdrEnvironment>,
    herdr_worker: Option<HerdrGraphicsWorker>,
    ansi: AnsiCanvas,
    metrics: LiveMetrics,
}

#[derive(Debug, Clone, Copy)]
struct LiveViewOptions {
    mode: TuiLiveMode,
    backend: TuiLiveBackend,
    quality: TuiLiveQuality,
    fit: TuiLiveFit,
    kitty_detected: bool,
}

impl Default for LiveViewOptions {
    fn default() -> Self {
        Self {
            mode: TuiLiveMode::Off,
            backend: TuiLiveBackend::Auto,
            quality: TuiLiveQuality::Balanced,
            fit: TuiLiveFit::Contain,
            kitty_detected: false,
        }
    }
}

impl LiveViewState {
    fn new(
        mode: TuiLiveMode,
        preference: TuiLiveBackend,
        quality: TuiLiveQuality,
        fit: TuiLiveFit,
        kitty_detected: bool,
    ) -> Self {
        let mut state = Self {
            mode,
            preference,
            quality,
            effective_quality: quality,
            adaptive_quality: false,
            adaptive_scale_step: 0,
            stable_windows: 0,
            adapted_generation: 0,
            fit,
            backend: None,
            kitty_detected,
            herdr_environment: HerdrEnvironment::from_process(),
            herdr_worker: None,
            ansi: AnsiCanvas::default(),
            metrics: LiveMetrics::default(),
        };
        state.select_backend();
        state
    }

    fn enabled(&self) -> bool {
        self.backend.is_some()
    }

    fn select_backend(&mut self) {
        self.stop_herdr();
        self.ansi.clear();
        self.backend = if self.mode == TuiLiveMode::Off {
            None
        } else {
            match self.preference {
                TuiLiveBackend::Herdr if self.herdr_environment.is_some() => {
                    Some(ActiveLiveBackend::Herdr)
                }
                TuiLiveBackend::Herdr if self.mode == TuiLiveMode::On => {
                    Some(ActiveLiveBackend::Ansi)
                }
                TuiLiveBackend::Kitty => Some(ActiveLiveBackend::Kitty),
                TuiLiveBackend::Ansi => Some(ActiveLiveBackend::Ansi),
                TuiLiveBackend::Auto if self.herdr_environment.is_some() => {
                    Some(ActiveLiveBackend::Herdr)
                }
                TuiLiveBackend::Auto if self.kitty_detected => Some(ActiveLiveBackend::Kitty),
                TuiLiveBackend::Auto if self.mode == TuiLiveMode::On => {
                    Some(ActiveLiveBackend::Ansi)
                }
                _ => None,
            }
        };
        if self.backend == Some(ActiveLiveBackend::Herdr) {
            self.herdr_worker = self
                .herdr_environment
                .clone()
                .map(HerdrGraphicsWorker::spawn);
        }
    }

    fn fall_back_from(&mut self, failed: ActiveLiveBackend) {
        self.stop_herdr();
        self.ansi.clear();
        self.backend = match failed {
            ActiveLiveBackend::Herdr if self.kitty_detected => Some(ActiveLiveBackend::Kitty),
            ActiveLiveBackend::Herdr | ActiveLiveBackend::Kitty if self.mode == TuiLiveMode::On => {
                Some(ActiveLiveBackend::Ansi)
            }
            ActiveLiveBackend::Herdr | ActiveLiveBackend::Kitty | ActiveLiveBackend::Ansi => None,
        };
    }

    fn stop_herdr(&mut self) {
        if let Some(mut worker) = self.herdr_worker.take() {
            let _ = worker.stop();
        }
    }

    fn ensure_herdr(&mut self) {
        if self.backend == Some(ActiveLiveBackend::Herdr) && self.herdr_worker.is_none() {
            self.herdr_worker = self
                .herdr_environment
                .clone()
                .map(HerdrGraphicsWorker::spawn);
        }
    }

    fn fit(&self) -> FrameFit {
        match self.fit {
            TuiLiveFit::Contain => FrameFit::Contain,
            TuiLiveFit::Cover => FrameFit::Cover,
            TuiLiveFit::Actual => FrameFit::Actual,
        }
    }

    fn diagnostics(&self, policy: &PresentationPolicy) -> String {
        match self.backend {
            Some(backend) => format!(
                "LIVE · {} · {:?} · req {} / cap {:.1} / out {:.1} FPS · SCALE {:.2}x · {:.0} KiB/s · {} dropped · {:?}",
                backend.label(),
                policy.profile,
                policy.requested_fps,
                self.metrics.acquisition_fps,
                self.metrics.presentation_fps,
                policy.capture_scale,
                self.metrics.bytes_per_second / 1024.0,
                self.metrics.dropped,
                policy.reasons,
            ),
            None => "LIVE off · semantic browser view · screenshot remains explicit".into(),
        }
    }

    fn quality_label(&self) -> &'static str {
        match self.effective_quality {
            TuiLiveQuality::Data => "data",
            TuiLiveQuality::Balanced => "balanced",
            TuiLiveQuality::Smooth => "smooth",
        }
    }

    fn enable_adaptive_quality(&mut self) {
        self.adaptive_quality = true;
        self.quality = TuiLiveQuality::Balanced;
        self.effective_quality = TuiLiveQuality::Balanced;
        self.adaptive_scale_step = 0;
        self.stable_windows = 0;
        self.adapted_generation = self.metrics.generation;
    }

    fn set_manual_quality(&mut self, quality: TuiLiveQuality) {
        self.quality = quality;
        self.effective_quality = quality;
        self.adaptive_quality = false;
        self.adaptive_scale_step = 0;
        self.stable_windows = 0;
    }

    fn adapt_quality(&mut self) {
        if !self.adaptive_quality || self.adapted_generation == self.metrics.generation {
            return;
        }
        self.adapted_generation = self.metrics.generation;
        if self.metrics.drop_ratio >= 0.20 {
            if self.adaptive_scale_step < 3 {
                self.adaptive_scale_step += 1;
            } else {
                self.effective_quality = match self.effective_quality {
                    TuiLiveQuality::Smooth => TuiLiveQuality::Balanced,
                    TuiLiveQuality::Balanced | TuiLiveQuality::Data => TuiLiveQuality::Data,
                };
            }
            self.stable_windows = 0;
        } else if self.metrics.drop_ratio <= 0.02 {
            self.stable_windows = self.stable_windows.saturating_add(1);
            if self.stable_windows >= 3 {
                if self.adaptive_scale_step > 0 {
                    self.adaptive_scale_step -= 1;
                } else {
                    self.effective_quality = match self.effective_quality {
                        TuiLiveQuality::Data => TuiLiveQuality::Balanced,
                        TuiLiveQuality::Balanced | TuiLiveQuality::Smooth => TuiLiveQuality::Smooth,
                    };
                }
                self.stable_windows = 0;
            }
        } else {
            self.stable_windows = 0;
        }
    }
}

impl Drop for LiveViewState {
    fn drop(&mut self) {
        self.stop_herdr();
    }
}

fn display_class(preference: TuiLayout, width: u16, _remote: RemoteContext) -> DisplayClass {
    match preference {
        TuiLayout::Mobile => DisplayClass::Phone,
        TuiLayout::Desktop => DisplayClass::Wide,
        TuiLayout::Compact => DisplayClass::Compact,
        TuiLayout::Auto if width <= PHONE_MAX_COLUMNS => DisplayClass::Phone,
        TuiLayout::Auto if width <= COMPACT_MAX_COLUMNS => DisplayClass::Compact,
        TuiLayout::Auto => DisplayClass::Wide,
    }
}

fn connection_environment_for_context(
    preference: TuiLayout,
    columns: u16,
    rows: u16,
    remote: RemoteContext,
    kitty_probed: bool,
) -> ConnectionEnvironment {
    let layout = match preference {
        TuiLayout::Auto => None,
        TuiLayout::Mobile => Some(LayoutClass::Phone),
        TuiLayout::Compact => Some(LayoutClass::Compact),
        TuiLayout::Desktop => Some(LayoutClass::Wide),
    };
    let graphics = if remote.herdr {
        Some(GraphicsClass::Herdr)
    } else if kitty_probed {
        Some(GraphicsClass::Kitty)
    } else {
        None
    };
    ConnectionEnvironment::detect(
        columns.max(1),
        rows.max(1),
        &ConnectionSignals {
            ssh: remote.ssh,
            mosh: remote.mosh,
            tmux: remote.tmux,
            screen: remote.screen,
            herdr: remote.herdr,
        },
        graphics,
        ConnectionMeasurements::default(),
        ConnectionOverrides {
            layout,
            ..ConnectionOverrides::default()
        },
    )
    .expect("bounded terminal connection environment is valid")
}

impl WorkspaceMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Browser => "Browser",
            Self::Split => "Split",
            Self::Workflow => "Workflow",
            Self::Semantic => "Semantic",
            Self::Inspect => "Inspect",
            Self::Takeover => "Takeover",
            Self::Development => "Development",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationActor {
    Human,
    Agent,
    Observer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationLeaseState {
    Available,
    Held(MutationActor),
    Reconciling(MutationActor),
}

/// Small, revision-guarded TUI lease state machine. Browser operations still
/// enforce their own revision checks; this gate prevents concurrent mutation
/// intent before an operation reaches the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationLease {
    state: MutationLeaseState,
    revision: u64,
}

impl Default for MutationLease {
    fn default() -> Self {
        Self {
            state: MutationLeaseState::Available,
            revision: 0,
        }
    }
}

impl MutationLease {
    pub const fn state(self) -> MutationLeaseState {
        self.state
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }

    pub fn acquire(
        &mut self,
        actor: MutationActor,
        expected_revision: u64,
    ) -> Result<u64, &'static str> {
        if actor == MutationActor::Observer {
            return Err("observer cannot acquire mutation lease");
        }
        if expected_revision != self.revision {
            return Err("lease revision is stale");
        }
        if !matches!(self.state, MutationLeaseState::Available) {
            return Err("mutation lease is already held");
        }
        self.revision = self.revision.saturating_add(1);
        self.state = MutationLeaseState::Held(actor);
        Ok(self.revision)
    }

    pub fn takeover(
        &mut self,
        actor: MutationActor,
        expected_revision: u64,
    ) -> Result<u64, &'static str> {
        if actor == MutationActor::Observer {
            return Err("observer cannot take over mutation lease");
        }
        if expected_revision != self.revision {
            return Err("lease revision is stale");
        }
        if matches!(self.state, MutationLeaseState::Available) {
            return Err("no active lease to take over");
        }
        self.revision = self.revision.saturating_add(1);
        self.state = MutationLeaseState::Reconciling(actor);
        Ok(self.revision)
    }

    pub fn reconcile(&mut self, expected_revision: u64) -> Result<(), &'static str> {
        if expected_revision != self.revision {
            return Err("lease revision is stale");
        }
        let MutationLeaseState::Reconciling(actor) = self.state else {
            return Err("lease is not awaiting reconciliation");
        };
        self.state = MutationLeaseState::Held(actor);
        Ok(())
    }

    pub fn release(
        &mut self,
        actor: MutationActor,
        expected_revision: u64,
    ) -> Result<u64, &'static str> {
        if expected_revision != self.revision {
            return Err("lease revision is stale");
        }
        if self.state != MutationLeaseState::Held(actor) {
            return Err("actor does not hold mutation lease");
        }
        self.revision = self.revision.saturating_add(1);
        self.state = MutationLeaseState::Available;
        Ok(self.revision)
    }
}

const TUI_INPUT_MAX_BYTES: usize = 4 * 1024;
const BUSY_TICK: Duration = Duration::from_millis(120);
const INPUT_POLL: Duration = Duration::from_millis(50);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
pub struct App {
    url: String,
    title: String,
    browser_target_id: Option<String>,
    activity: VecDeque<String>,
    page_content: String,
    tap_targets: Vec<SemanticTapTarget>,
    tap_mode: bool,
    page_scroll: u16,
    input: String,
    cursor_pos: usize,
    should_quit: bool,
    error_msg: Option<String>,
    status: String,
    capability_summary: String,
    intent_request: Option<SemanticIntentRequest>,
    intent_result: Option<SemanticIntentResult>,
    intent_selection: usize,
    knowledge_path: PathBuf,
    browser_state: BrowserState,
    browser_recovery: Option<EndpointProbe>,
    busy: Option<BusyState>,
    next_operation_id: u64,
    graphics: TerminalGraphics,
    live: LiveViewState,
    mode: WorkspaceMode,
    layout_preference: TuiLayout,
    display_class: DisplayClass,
    mobile_view: MobileView,
    mobile_help: bool,
    mobile_nav_area: Option<Rect>,
    remote_context: RemoteContext,
    connection_environment: ConnectionEnvironment,
    mutation_lease: MutationLease,
    visual_revision: u64,
    visual_status: String,
    development: Option<ProjectWorkspace>,
    development_enabled: bool,
    harness: LocalHarness,
    pi_command_tx: Option<std_mpsc::Sender<HarnessRequest>>,
    lsp_command_tx: Option<std_mpsc::Sender<String>>,
    development_files: String,
    development_editor: String,
    development_runtime: String,
    development_diff: String,
    attention_summary: String,
    attention_notifications: bool,
    notified_attention: VecDeque<String>,
    verification_summary: String,
    active_buffer: Option<String>,
    editor_focus: bool,
    editor_cursor: usize,
    editor_area: Option<Rect>,
    live_area: Option<Rect>,
    development_event_tx: std_mpsc::Sender<DevelopmentAsyncEvent>,
    development_event_rx: std_mpsc::Receiver<DevelopmentAsyncEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticTapTarget {
    reference: String,
    role: String,
    name: String,
}

#[derive(Debug)]
enum DevelopmentAsyncEvent {
    Completed(String),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserState {
    Connecting,
    Ready,
    Recovery,
    SemanticOnly,
    Unavailable,
    Stopped,
}

#[derive(Debug, Clone)]
struct BusyState {
    id: u64,
    label: String,
    cancelling: bool,
    spinner: usize,
    /// Revision returned when this operation acquired the human mutation lease.
    /// Read-only operations intentionally leave this unset.
    lease_revision: Option<u64>,
}

#[derive(Debug, PartialEq)]
enum UiIntent {
    None,
    Submit(String),
    Pointer(BrowserOperation),
    Cancel(u64),
    Quit,
}

impl App {
    #[cfg(test)]
    fn new() -> Self {
        Self::new_for_product(true)
    }

    #[cfg(test)]
    fn new_for_product(development_enabled: bool) -> Self {
        Self::new_for_product_with_context(
            development_enabled,
            TuiLayout::Desktop,
            RemoteContext::default(),
            120,
            LiveViewOptions::default(),
        )
    }

    fn new_for_product_with_context(
        development_enabled: bool,
        layout_preference: TuiLayout,
        remote_context: RemoteContext,
        terminal_width: u16,
        live_options: LiveViewOptions,
    ) -> Self {
        let (development_event_tx, development_event_rx) = std_mpsc::channel();
        let mut activity = VecDeque::new();
        activity.push_back("Glass started.".to_string());
        activity.push_back("Connecting to Chrome…".to_string());
        let display_class = display_class(layout_preference, terminal_width, remote_context);
        let connection_environment = connection_environment_for_context(
            layout_preference,
            terminal_width,
            32,
            remote_context,
            live_options.kitty_detected,
        );
        let mut live = LiveViewState::new(
            live_options.mode,
            live_options.backend,
            live_options.quality,
            live_options.fit,
            live_options.kitty_detected,
        );
        if remote_context.mosh {
            live.herdr_environment = None;
            live.kitty_detected = false;
            live.backend = None;
        }
        let graphics_mode = if live.backend == Some(ActiveLiveBackend::Kitty) {
            GraphicsMode::Kitty
        } else {
            GraphicsMode::Semantic
        };
        let graphics = TerminalGraphics::new(
            graphics_mode,
            TargetResourceIdentity::new("tui", Some("terminal".to_string()))
                .expect("static TUI graphics identity is valid"),
        )
        .expect("static TUI graphics state is valid");
        let development = development_enabled
            .then(|| ProjectWorkspace::open(std::env::current_dir().unwrap_or_default()).ok())
            .flatten();
        let mut app = Self {
            url: String::new(),
            title: "Glass — Browser Agent".to_string(),
            browser_target_id: None,
            activity,
            page_content: "No page loaded.".to_string(),
            tap_targets: Vec::new(),
            tap_mode: false,
            page_scroll: 0,
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            error_msg: None,
            status: "Connecting to Chrome…".to_string(),
            capability_summary: "Capabilities: loading".to_string(),
            intent_request: None,
            intent_result: None,
            intent_selection: 0,
            knowledge_path: default_knowledge_store_path("default"),
            browser_state: BrowserState::Connecting,
            browser_recovery: None,
            busy: None,
            next_operation_id: 1,
            graphics,
            live,
            mode: if development_enabled && display_class == DisplayClass::Phone {
                WorkspaceMode::Development
            } else {
                WorkspaceMode::Browser
            },
            layout_preference,
            display_class,
            mobile_view: MobileView::Home,
            mobile_help: false,
            mobile_nav_area: None,
            remote_context,
            connection_environment,
            mutation_lease: MutationLease::default(),
            visual_revision: 0,
            visual_status: "semantic browser view; live capture is explicit".to_string(),
            development,
            development_enabled,
            harness: LocalHarness::default(),
            pi_command_tx: None,
            lsp_command_tx: None,
            development_files: "Project detection pending.".into(),
            development_editor: "Open a file with `project open PATH`.".into(),
            development_runtime: "No managed processes.".into(),
            development_diff: "No project diff available.".into(),
            attention_summary: "No items need attention.".into(),
            attention_notifications: false,
            notified_attention: VecDeque::new(),
            verification_summary: "No verification card available.".into(),
            active_buffer: None,
            editor_focus: false,
            editor_cursor: 0,
            editor_area: None,
            live_area: None,
            development_event_tx,
            development_event_rx,
        };
        app.refresh_development_view();
        app.refresh_development_diff();
        app
    }

    fn add_activity(&mut self, message: impl Into<String>) {
        let message = bounded_text(&message.into(), TUI_ACTIVITY_MAX_BYTES);
        if self.activity.len() == ACTIVITY_LIMIT {
            self.activity.pop_front();
        }
        self.activity.push_back(message);
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.error_msg = Some(bounded_text(&message.into(), TUI_ACTIVITY_MAX_BYTES));
    }

    fn report_error(&mut self, message: impl Into<String>) {
        let message = bounded_text(&message.into(), TUI_ACTIVITY_MAX_BYTES);
        self.set_error(message.clone());
        self.add_activity(format!("Error: {message}"));
    }

    fn clear_error(&mut self) {
        self.error_msg = None;
    }

    fn set_status(&mut self, status: impl Into<String>) {
        self.status = bounded_text(&status.into(), TUI_ACTIVITY_MAX_BYTES);
    }
    pub fn mode(&self) -> WorkspaceMode {
        self.mode
    }

    pub fn mutation_lease(&self) -> MutationLease {
        self.mutation_lease
    }

    fn set_mode(&mut self, mode: WorkspaceMode) {
        if self.mode != mode {
            self.mode = mode;
            self.graphics.clear_pane().ok();
            self.add_activity(format!("Workspace mode: {}.", mode.label()));
        }
    }

    fn set_mobile_view(&mut self, view: MobileView) {
        self.mobile_view = view;
        self.mobile_help = false;
        self.page_scroll = 0;
        self.set_status(format!("Mobile view: {}", view.label()));
    }

    fn configure_live(
        &mut self,
        mode: Option<TuiLiveMode>,
        backend: Option<TuiLiveBackend>,
        quality: Option<TuiLiveQuality>,
        fit: Option<TuiLiveFit>,
    ) {
        if let Some(mode) = mode {
            self.live.mode = mode;
        }
        if let Some(backend) = backend {
            self.live.preference = backend;
        }
        if let Some(quality) = quality {
            self.live.set_manual_quality(quality);
        }
        if let Some(fit) = fit {
            self.live.fit = fit;
        }
        self.live.metrics = LiveMetrics::default();
        self.live.select_backend();
        self.sync_live_graphics_mode();
        if self.live.enabled() {
            self.set_mobile_view(MobileView::App);
            self.set_status(self.live_diagnostics());
        } else {
            self.graphics.clear_pane().ok();
            self.set_status("Live view disabled; semantic browser view active.");
        }
    }

    fn sync_live_graphics_mode(&mut self) {
        let mode = if self.live.backend == Some(ActiveLiveBackend::Kitty) {
            GraphicsMode::Kitty
        } else {
            GraphicsMode::Semantic
        };
        if self.graphics.mode() == mode {
            return;
        }
        let cleanup = self.graphics.cleanup();
        if !cleanup.is_empty() {
            let mut stdout = io::stdout();
            let _ = stdout.write_all(&cleanup).and_then(|()| stdout.flush());
        }
        self.graphics = TerminalGraphics::new(
            mode,
            TargetResourceIdentity::new("tui", Some("terminal".to_string()))
                .expect("static TUI graphics identity is valid"),
        )
        .expect("static TUI graphics state is valid");
    }

    fn poll_live_worker(&mut self) -> bool {
        let event = self
            .live
            .herdr_worker
            .as_ref()
            .and_then(HerdrGraphicsWorker::try_event);
        match event {
            Some(HerdrEvent::Connected) => {
                self.visual_status = "Herdr pane graphics stream connected".into();
                self.add_activity("Live browser connected to Herdr pane graphics.");
                true
            }
            Some(HerdrEvent::Failed(message)) => {
                self.add_activity(format!(
                    "Herdr graphics unavailable ({message}); selecting fallback."
                ));
                self.live.fall_back_from(ActiveLiveBackend::Herdr);
                self.sync_live_graphics_mode();
                self.visual_status = self.live_diagnostics();
                true
            }
            Some(HerdrEvent::Stopped) => false,
            None => false,
        }
    }

    fn live_policy(&self) -> PresentationPolicy {
        let pixel_intent = match self.live.mode {
            TuiLiveMode::Off => PixelIntent::Off,
            TuiLiveMode::Auto => PixelIntent::Auto,
            TuiLiveMode::On => PixelIntent::On,
        };
        let quality = match self.live.effective_quality {
            TuiLiveQuality::Data => QualityIntent::Data,
            TuiLiveQuality::Balanced => QualityIntent::Balanced,
            TuiLiveQuality::Smooth => QualityIntent::Smooth,
        };
        let mut policy = PresentationPolicy::select(
            &self.connection_environment,
            ActivityClass::Interactive,
            pixel_intent,
            quality,
        );
        let adaptive_scale =
            [1.0_f32, 0.75, 0.65, 0.5][usize::from(self.live.adaptive_scale_step.min(3))];
        let next_scale = (policy.capture_scale * adaptive_scale).clamp(0.5, 1.0);
        if next_scale < policy.capture_scale {
            policy.capture_scale = next_scale;
            if !policy
                .reasons
                .contains(&crate::connection::PolicyReason::CaptureScaleReduced)
            {
                policy
                    .reasons
                    .push(crate::connection::PolicyReason::CaptureScaleReduced);
            }
        }
        policy
    }

    fn live_diagnostics(&self) -> String {
        self.live.diagnostics(&self.live_policy())
    }

    fn live_capture_config(&self) -> ScreencastConfig {
        let Some(area) = self.live_area else {
            return ScreencastConfig::default();
        };
        if !self.live.enabled() || area.width == 0 || area.height == 0 {
            return ScreencastConfig::default();
        }
        let policy = self.live_policy();
        if policy.requested_fps == 0 {
            return ScreencastConfig::default();
        }
        let scaled_width = (f32::from(area.width) * 8.0 * policy.capture_scale).round() as u32;
        let scaled_height = (f32::from(area.height) * 16.0 * policy.capture_scale).round() as u32;
        ScreencastConfig {
            enabled: true,
            max_width: scaled_width.clamp(64, LIVE_CAPTURE_MAX_WIDTH),
            max_height: scaled_height.clamp(64, LIVE_CAPTURE_MAX_HEIGHT),
            minimum_interval: Duration::from_micros(1_000_000 / u64::from(policy.requested_fps)),
            requested_fps: policy.requested_fps,
            capture_scale: policy.capture_scale,
        }
    }

    fn cycle_mobile_view(&mut self, delta: i8) {
        let current = i16::from(self.mobile_view.number()) - 1;
        let next = (current + i16::from(delta)).rem_euclid(6) + 1;
        if let Some(view) = MobileView::from_number(next as u8) {
            self.set_mobile_view(view);
        }
    }

    fn save_reconnect_capsule(&self) -> crate::development::DevelopmentResult<PathBuf> {
        let project = self.development.as_ref().ok_or_else(|| {
            crate::development::DevelopmentError::NotFound("project workspace".into())
        })?;
        let mut capsule = ReconnectCapsule::new(project.root())?;
        capsule.event_cursor = project
            .timeline()
            .events()
            .next_back()
            .map(|event| event.id.clone());
        capsule.mobile_view = Some(self.mobile_view.label().to_ascii_lowercase());
        capsule.browser_target_id = self.browser_target_id.clone();
        capsule.browser_revision = Some(self.visual_revision);
        capsule.pending_attention = attention_inbox(project.timeline().events().cloned())
            .into_iter()
            .find(|item| item.state == AttentionState::NeedsAttention)
            .map(|item| item.title);
        capsule.live_mode = Some(format!("{:?}", self.live.mode).to_ascii_lowercase());
        capsule.live_quality = Some(if self.live.adaptive_quality {
            "auto".into()
        } else {
            self.live.quality_label().into()
        });
        ReconnectCapsuleStore::save(&capsule)
    }

    fn restore_reconnect_capsule(&mut self) {
        let Some(root) = self
            .development
            .as_ref()
            .map(|project| project.root().to_path_buf())
        else {
            return;
        };
        let Ok(Some(capsule)) = ReconnectCapsuleStore::load(root) else {
            return;
        };
        self.mobile_view = match capsule.mobile_view.as_deref() {
            Some("agent") => MobileView::Agent,
            Some("app") | Some("browser") => MobileView::App,
            Some("diff") => MobileView::Diff,
            Some("project") | Some("more") => MobileView::Project,
            Some("process") | Some("logs") => MobileView::Process,
            _ => MobileView::Home,
        };
        self.browser_target_id = capsule.browser_target_id;
        self.visual_revision = capsule.browser_revision.unwrap_or(0);
        match capsule.live_mode.as_deref() {
            Some("on") => self.live.mode = TuiLiveMode::On,
            Some("auto") => self.live.mode = TuiLiveMode::Auto,
            Some("off") => self.live.mode = TuiLiveMode::Off,
            _ => {}
        }
        match capsule.live_quality.as_deref() {
            Some("auto") => self.live.enable_adaptive_quality(),
            Some("data") => self.live.set_manual_quality(TuiLiveQuality::Data),
            Some("balanced") => self.live.set_manual_quality(TuiLiveQuality::Balanced),
            Some("smooth") => self.live.set_manual_quality(TuiLiveQuality::Smooth),
            _ => {}
        }
        self.live.select_backend();
        self.add_activity("Restored non-sensitive reconnect capsule.");
    }

    fn refresh_development_view(&mut self) {
        let Some(project) = self.development.as_mut() else {
            self.development_files = "No project workspace could be opened.".into();
            self.development_editor = "Project detection unavailable.".into();
            self.development_runtime = "Project runtime unavailable.".into();
            return;
        };
        self.development_files = match project.list_files_result() {
            Ok(tree) => {
                let mut lines = vec![format!(
                    "{} entries · limit {} · truncated:{} · symlinks skipped:{}",
                    tree.entries.len(),
                    tree.limit,
                    tree.truncated,
                    tree.skipped_symlinks
                )];
                lines.extend(tree.entries.into_iter().take(79).map(|file| {
                    let git = file.git_status.as_deref().unwrap_or(" ");
                    let actor = file
                        .actor
                        .as_ref()
                        .map(|actor| format!(" ● {}", actor.name))
                        .unwrap_or_default();
                    format!(
                        "{git:>2} {} {}{}",
                        if matches!(file.kind, crate::development::FileKind::Directory) {
                            "▾"
                        } else {
                            " "
                        },
                        file.path,
                        actor
                    )
                }));
                lines.join("\n")
            }
            Err(error) => format!("File tree error: {error}"),
        };
        self.development_editor = self
            .active_buffer
            .as_deref()
            .and_then(|path| project.buffer(path))
            .or_else(|| project.buffers().next())
            .map(|buffer| {
                numbered_buffer_with_cursor(&buffer.path, &buffer.content, self.editor_cursor)
            })
            .unwrap_or_else(|| "No open buffer.\n\nproject open PATH".into());
        self.development_runtime = format_runtime_panel(project);
        let inbox = attention_inbox(project.timeline().events().cloned());
        if self.attention_notifications {
            let unseen = inbox
                .iter()
                .filter(|item| item.state == AttentionState::NeedsAttention)
                .filter(|item| !self.notified_attention.contains(&item.id))
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            if !unseen.is_empty() {
                let _ = io::stdout()
                    .write_all(b"\x07")
                    .and_then(|()| io::stdout().flush());
                for id in unseen {
                    if self.notified_attention.len() == 64 {
                        self.notified_attention.pop_front();
                    }
                    self.notified_attention.push_back(id);
                }
            }
        }
        self.attention_summary = format_attention_inbox(inbox);
    }

    fn refresh_development_diff(&mut self) {
        let Some(project) = self.development.as_mut() else {
            self.development_diff = "Project diff unavailable.".into();
            return;
        };
        match project.diff() {
            Ok(diff) => {
                self.verification_summary = VerificationCard::from_diff(
                    "Current project verification",
                    &diff,
                    Some(self.visual_revision),
                )
                .and_then(|card| serde_json::to_string_pretty(&card).map_err(Into::into))
                .unwrap_or_else(|error| format!("Verification card unavailable: {error}"));
                self.development_diff = serde_json::to_string_pretty(&diff)
                    .unwrap_or_else(|error| format!("Project diff unavailable: {error}"));
            }
            Err(error) => {
                self.development_diff = format!("Project diff unavailable: {error}");
                self.verification_summary = "Verification card unavailable.".into();
            }
        }
    }

    fn handle_project_command(&mut self, command: &str) {
        self.set_mode(WorkspaceMode::Development);
        let browser_agent_context = self.agent_browser_context();
        let command = command.trim();
        let operation_name = command
            .split_whitespace()
            .next()
            .unwrap_or("inspect")
            .to_ascii_lowercase();
        let result = (|| -> Result<String, String> {
            let mut parts = command.splitn(3, char::is_whitespace);
            let operation = parts.next().unwrap_or("inspect").to_ascii_lowercase();
            let project = self
                .development
                .as_mut()
                .ok_or_else(|| "project workspace is unavailable".to_string())?;
            match operation.as_str() {
                "inspect" => Ok(serde_json::to_string_pretty(project.detection())
                    .map_err(|error| error.to_string())?),
                "files" => Ok(serde_json::to_string_pretty(&project.list_files_result().map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?),
                "search" => {
                    let query = parts.next().ok_or_else(|| "project search requires QUERY".to_string())?;
                    Ok(serde_json::to_string_pretty(&project.search(query, 64).map_err(|error| error.to_string())?)
                        .map_err(|error| error.to_string())?)
                }
                "open" => {
                    let path = parts
                        .next()
                        .ok_or_else(|| "project open requires PATH".to_string())?;
                    let buffer = project
                        .open_buffer(path, Actor::local())
                        .map_err(|error| error.to_string())?;
                    self.active_buffer = Some(buffer.path.clone());
                    self.editor_cursor = buffer.content.chars().count();
                    self.editor_focus = true;
                    Ok(numbered_buffer(&buffer.path, &buffer.content))
                }
                "edit" => {
                    let path = parts
                        .next()
                        .ok_or_else(|| "project edit requires PATH and CONTENT".to_string())?;
                    let content = parts
                        .next()
                        .ok_or_else(|| "project edit requires PATH and CONTENT".to_string())?;
                    project
                        .open_buffer(path, Actor::local())
                        .and_then(|_| project.edit_buffer(path, content.into(), Actor::local()))
                        .and_then(|_| project.save_buffer(path))
                        .map(|buffer| numbered_buffer(&buffer.path, &buffer.content))
                        .map_err(|error| error.to_string())
                }
                "run" => {
                    let name = parts
                        .next()
                        .ok_or_else(|| "project run requires NAME and COMMAND".to_string())?;
                    let command = parts
                        .next()
                        .ok_or_else(|| "project run requires NAME and COMMAND".to_string())?;
                    let snapshot = project
                        .start_process(name, command)
                        .map_err(|error| error.to_string())?;
                    Ok(serde_json::to_string_pretty(&snapshot)
                        .map_err(|error| error.to_string())?)
                }
                "processes" => Ok(serde_json::to_string_pretty(&project.processes().list_checked().map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?),
                "restart" => {
                    let name = parts.next().ok_or_else(|| "project restart requires NAME".to_string())?;
                    Ok(serde_json::to_string_pretty(&project.processes().restart(name).map_err(|error| error.to_string())?)
                        .map_err(|error| error.to_string())?)
                }
                "stop" => {
                    let name = parts
                        .next()
                        .ok_or_else(|| "project stop requires NAME".to_string())?;
                    Ok(serde_json::to_string_pretty(
                        &project
                            .stop_process(name)
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?)
                }
                "output" => {
                    let name = parts
                        .next()
                        .ok_or_else(|| "project output requires NAME".to_string())?;
                    Ok(project
                        .processes()
                        .output(name)
                        .map_err(|error| error.to_string())?)
                }
                "diff" => Ok(serde_json::to_string_pretty(
                    &project.diff().map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?),
                "timeline" => Ok(serde_json::to_string_pretty(
                    &project.timeline().events().collect::<Vec<_>>(),
                )
                .map_err(|error| error.to_string())?),
                "replay" => Ok(serde_json::to_string_pretty(&project.replay(0, 64).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?),
                "diagnostics" => {
                    let path = parts.next().ok_or_else(|| "project diagnostics requires PATH".to_string())?;
                    let path = path.to_string();
                    let display_path = path.clone();
                    if self.lsp_command_tx.is_none() {
                        let root = project.root().to_path_buf();
                        let events = self.development_event_tx.clone();
                        let (sender, receiver) = std_mpsc::channel::<String>();
                        std::thread::Builder::new()
                            .name("glass-lsp-client".into())
                            .spawn(move || {
                                let mut client = match crate::development::LspClient::rust_analyzer(&root) {
                                    Ok(client) => client,
                                    Err(error) => {
                                        let _ = events.send(DevelopmentAsyncEvent::Failed(error.to_string()));
                                        return;
                                    }
                                };
                                while let Ok(path) = receiver.recv() {
                                    let result = client.diagnostics(&path)
                                        .and_then(|diagnostics| serde_json::to_string_pretty(&diagnostics).map_err(Into::into));
                                    let event = match result {
                                        Ok(output) => DevelopmentAsyncEvent::Completed(output),
                                        Err(error) => DevelopmentAsyncEvent::Failed(error.to_string()),
                                    };
                                    if events.send(event).is_err() { break; }
                                }
                            })
                            .map_err(|error| error.to_string())?;
                        self.lsp_command_tx = Some(sender);
                    }
                    self.lsp_command_tx.as_ref().expect("LSP worker initialized")
                        .send(path).map_err(|error| error.to_string())?;
                    Ok(format!("Diagnostics started for {display_path}; input remains active."))
                }
                "graph" => Ok(serde_json::to_string_pretty(&project.discover_runtime_links().map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?),
                "neovim" => Ok(serde_json::to_string_pretty(&crate::development::probe_neovim().map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?),
                "attach" => {
                    let actor = parts.next().ok_or_else(|| "project attach requires ACTOR".to_string())?;
                    let actor = Actor::external(actor);
                    project.attach_actor(actor.clone()).map_err(|error| error.to_string())?;
                    Ok(serde_json::to_string_pretty(&actor).map_err(|error| error.to_string())?)
                }
                "pi" => {
                    let action = parts.next().unwrap_or("state");
                    let payload = parts.next().unwrap_or("");
                    let request = match action {
                        "state" => HarnessRequest::State,
                        "models" => HarnessRequest::Models,
                        "prompt" if !payload.is_empty() => {
                            let context = crate::development::resolve_context_with_browser(
                                project,
                                payload,
                                browser_agent_context.as_ref(),
                            )
                                .map_err(|error| error.to_string())?;
                            HarnessRequest::Prompt {
                                text: format!(
                                    "{payload}\n\nGlass semantic context packet:\n{}",
                                    serde_json::to_string(&context).map_err(|error| error.to_string())?
                                ),
                            }
                        },
                        "steer" if !payload.is_empty() => HarnessRequest::Steer { text: payload.into() },
                        "follow-up" if !payload.is_empty() => HarnessRequest::FollowUp { text: payload.into() },
                        "abort" => HarnessRequest::Abort,
                        "new" => HarnessRequest::NewSession,
                        _ => return Err("project pi [state|models|prompt TEXT|steer TEXT|follow-up TEXT|abort|new]".into()),
                    };
                    if self.pi_command_tx.is_none() {
                        let root = project.root().to_path_buf();
                        let events = self.development_event_tx.clone();
                        let (sender, receiver) = std_mpsc::channel();
                        std::thread::Builder::new()
                            .name("glass-pi-harness".into())
                            .spawn(move || {
                                let mut harness = match PiHarness::spawn(&root) {
                                    Ok(harness) => harness,
                                    Err(error) => {
                                        let _ = events.send(DevelopmentAsyncEvent::Failed(error.to_string()));
                                        return;
                                    }
                                };
                                while let Ok(request) = receiver.recv() {
                                    let event = match harness.request(request)
                                        .and_then(|values| serde_json::to_string_pretty(&values).map_err(Into::into))
                                    {
                                        Ok(output) => DevelopmentAsyncEvent::Completed(output),
                                        Err(error) => DevelopmentAsyncEvent::Failed(error.to_string()),
                                    };
                                    if events.send(event).is_err() {
                                        break;
                                    }
                                }
                            })
                            .map_err(|error| error.to_string())?;
                        self.pi_command_tx = Some(sender);
                    }
                    self.pi_command_tx.as_ref().expect("Pi harness initialized")
                        .send(request).map_err(|error| error.to_string())?;
                    Ok("Pi request queued; editor and browser input remain active.".into())
                }
                "agent" => {
                    let prompt = parts
                        .next()
                        .ok_or_else(|| "project agent requires PROMPT".to_string())?;
                    self.harness
                        .set_browser_context(browser_agent_context.clone());
                    let events = self.harness
                        .handle(
                            project,
                            HarnessRequest::Prompt {
                                text: prompt.into(),
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    Ok(serde_json::to_string_pretty(&events)
                        .map_err(|error| error.to_string())?)
                }
                _ => Err("project commands: inspect | files | search QUERY | open PATH | edit PATH CONTENT | run NAME COMMAND | processes | restart NAME | stop NAME | output NAME | diagnostics PATH | graph | diff | timeline | replay | neovim | attach ACTOR | agent PROMPT | pi ACTION".into()),
            }
        })();
        match result {
            Ok(content) => {
                self.set_page_content(content);
                self.set_status("Development workspace");
                self.add_activity("Development command completed.");
                if self.display_class == DisplayClass::Phone {
                    let view = match operation_name.as_str() {
                        "agent" | "pi" | "timeline" | "replay" => MobileView::Agent,
                        "diff" => MobileView::Diff,
                        "run" | "processes" | "restart" | "stop" | "output" => MobileView::Process,
                        "inspect" | "files" | "search" | "open" | "edit" | "diagnostics"
                        | "graph" | "neovim" | "attach" => MobileView::Project,
                        _ => MobileView::Home,
                    };
                    self.set_mobile_view(view);
                }
            }
            Err(error) => self.report_error(error),
        }
        self.refresh_development_view();
        self.refresh_development_diff();
    }

    fn agent_browser_context(&self) -> Option<crate::development::BrowserAgentContext> {
        let connected = self.browser_ready();
        (connected || self.visual_revision > 0).then(|| {
            let origin = url::Url::parse(&self.url).ok().and_then(|url| {
                url.host_str().map(|host| match url.port() {
                    Some(port) => format!("{}://{host}:{port}", url.scheme()),
                    None => format!("{}://{host}", url.scheme()),
                })
            });
            crate::development::BrowserAgentContext {
                connected,
                target_id: self.browser_target_id.clone(),
                origin,
                title: bounded_text(&self.title, 512),
                browser_revision: self.visual_revision,
                semantic_summary: bounded_text(&self.page_content, 16 * 1024),
                workflow_state: if self.busy.is_some() {
                    "active"
                } else {
                    "idle"
                }
                .into(),
                memory_scope: "active-profile".into(),
            }
        })
    }

    fn poll_development_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.development_event_rx.try_recv() {
            changed = true;
            match event {
                DevelopmentAsyncEvent::Completed(output) => {
                    self.set_page_content(output);
                    self.set_status("Development operation completed");
                    self.add_activity("Asynchronous development operation completed.");
                }
                DevelopmentAsyncEvent::Failed(error) => self.report_error(error),
            }
        }
        if changed {
            self.refresh_development_view();
        }
        changed
    }

    fn apply_visual_status(&mut self, status: impl Into<String>) {
        self.visual_status = bounded_text(&status.into(), TUI_ACTIVITY_MAX_BYTES);
    }

    fn cursor_byte_index(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(index, _)| index)
            .unwrap_or(self.input.len())
    }

    fn insert_char(&mut self, character: char) -> bool {
        if self.input.len().saturating_add(character.len_utf8()) > TUI_INPUT_MAX_BYTES {
            return false;
        }
        let index = self.cursor_byte_index();
        self.input.insert(index, character);
        self.cursor_pos += 1;
        true
    }

    fn remove_before_cursor(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let end = self.cursor_byte_index();
        let start = self
            .input
            .char_indices()
            .nth(self.cursor_pos - 1)
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.input.drain(start..end);
        self.cursor_pos -= 1;
    }

    fn remove_at_cursor(&mut self) {
        let start = self.cursor_byte_index();
        let end = self
            .input
            .char_indices()
            .nth(self.cursor_pos + 1)
            .map(|(index, _)| index)
            .unwrap_or(self.input.len());
        if start < end {
            self.input.drain(start..end);
        }
    }

    fn insert_editor_text(&mut self, text: &str) {
        let Some(path) = self.active_buffer.clone() else {
            self.report_error("No active editor buffer.");
            return;
        };
        let result = (|| {
            let project = self
                .development
                .as_mut()
                .ok_or_else(|| "project workspace is unavailable".to_string())?;
            let current = project
                .buffer(&path)
                .ok_or_else(|| format!("buffer is unavailable: {path}"))?
                .content
                .clone();
            let byte = char_byte_index(&current, self.editor_cursor);
            let mut edited = current;
            edited.insert_str(byte, text);
            project
                .edit_buffer(&path, edited, Actor::local())
                .map_err(|error| error.to_string())?;
            self.editor_cursor = self.editor_cursor.saturating_add(text.chars().count());
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            self.report_error(error);
        }
        self.refresh_development_view();
    }

    fn reduce_editor_key(&mut self, key: KeyEvent) -> UiIntent {
        let Some(path) = self.active_buffer.clone() else {
            self.editor_focus = false;
            return UiIntent::None;
        };
        match key.code {
            KeyCode::Esc => {
                self.editor_focus = false;
                self.set_status("Command focus");
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let result = self
                    .development
                    .as_mut()
                    .ok_or_else(|| "project workspace is unavailable".to_string())
                    .and_then(|project| {
                        project
                            .save_buffer(&path)
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(_) => self.set_status(format!("Saved {path}")),
                    Err(error) => self.report_error(error),
                }
                self.refresh_development_view();
                self.refresh_development_diff();
            }
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(project) = self.development.as_mut() {
                    match project.undo_buffer(&path) {
                        Ok(buffer) => {
                            self.editor_cursor =
                                self.editor_cursor.min(buffer.content.chars().count())
                        }
                        Err(error) => self.report_error(error.to_string()),
                    }
                }
                self.refresh_development_view();
            }
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(project) = self.development.as_mut() {
                    match project.redo_buffer(&path) {
                        Ok(buffer) => {
                            self.editor_cursor =
                                self.editor_cursor.min(buffer.content.chars().count())
                        }
                        Err(error) => self.report_error(error.to_string()),
                    }
                }
                self.refresh_development_view();
            }
            KeyCode::Left => self.editor_cursor = self.editor_cursor.saturating_sub(1),
            KeyCode::Right => {
                let length = self
                    .development
                    .as_ref()
                    .and_then(|project| project.buffer(&path))
                    .map_or(0, |buffer| buffer.content.chars().count());
                self.editor_cursor = self.editor_cursor.saturating_add(1).min(length);
            }
            KeyCode::Backspace => {
                if self.editor_cursor > 0 {
                    self.editor_cursor -= 1;
                    self.remove_editor_character(&path, self.editor_cursor);
                }
            }
            KeyCode::Delete => self.remove_editor_character(&path, self.editor_cursor),
            KeyCode::Enter => self.insert_editor_text("\n"),
            KeyCode::Tab => self.insert_editor_text("    "),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert_editor_text(&character.to_string());
            }
            _ => {}
        }
        UiIntent::None
    }

    fn remove_editor_character(&mut self, path: &str, index: usize) {
        let result = (|| {
            let project = self
                .development
                .as_mut()
                .ok_or_else(|| "project workspace is unavailable".to_string())?;
            let mut content = project
                .buffer(path)
                .ok_or_else(|| format!("buffer is unavailable: {path}"))?
                .content
                .clone();
            let start = char_byte_index(&content, index);
            let end = char_byte_index(&content, index.saturating_add(1));
            if start < end {
                content.drain(start..end);
                project
                    .edit_buffer(path, content, Actor::local())
                    .map_err(|error| error.to_string())?;
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            self.report_error(error);
        }
        self.refresh_development_view();
    }
    fn mouse_intent(&mut self, mouse: MouseEvent) -> UiIntent {
        if self.display_class == DisplayClass::Phone
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && self.mobile_nav_area.is_some_and(|area| {
                mouse.column >= area.x
                    && mouse.column < area.x.saturating_add(area.width)
                    && mouse.row >= area.y
                    && mouse.row < area.y.saturating_add(area.height)
            })
        {
            let area = self
                .mobile_nav_area
                .expect("checked mobile navigation area");
            let relative = mouse.column.saturating_sub(area.x);
            let row = mouse.row.saturating_sub(area.y).min(1);
            let segment = (u32::from(relative) * 3 / u32::from(area.width.max(1))) as u8 + 1;
            let number = segment.min(3).saturating_add((row as u8).saturating_mul(3));
            if let Some(view) = MobileView::from_number(number.min(6)) {
                self.set_mobile_view(view);
            }
            return UiIntent::None;
        }
        if self.mode == WorkspaceMode::Development
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && self.editor_area.is_some_and(|area| {
                mouse.column >= area.x
                    && mouse.column < area.x.saturating_add(area.width)
                    && mouse.row >= area.y
                    && mouse.row < area.y.saturating_add(area.height)
            })
        {
            self.focus_editor_cell(mouse.column, mouse.row);
            return UiIntent::None;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => UiIntent::Pointer(BrowserOperation::ScrollAt {
                dx: 0.0,
                dy: -480.0,
                expected_revision: self.graphics.browser_revision(),
            }),
            MouseEventKind::ScrollDown => UiIntent::Pointer(BrowserOperation::ScrollAt {
                dx: 0.0,
                dy: 480.0,
                expected_revision: self.graphics.browser_revision(),
            }),
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(geometry) = self.graphics.geometry() else {
                    self.set_status("Pointer ignored: browser geometry is not ready.");
                    return UiIntent::None;
                };
                let point = crate::presentation::PixelPoint {
                    x: u32::from(mouse.column),
                    y: u32::from(mouse.row),
                };
                match geometry.pane_to_viewport(
                    point,
                    self.graphics.browser_revision(),
                    geometry.geometry_revision,
                ) {
                    Ok(viewport) => UiIntent::Pointer(BrowserOperation::ClickAt {
                        x: f64::from(viewport.x),
                        y: f64::from(viewport.y),
                        expected_revision: self.graphics.browser_revision(),
                    }),
                    Err(error) => {
                        self.set_status(format!("Pointer rejected: {error}"));
                        UiIntent::None
                    }
                }
            }
            MouseEventKind::Moved => {
                self.set_status(format!("Hover cell {},{}.", mouse.column, mouse.row));
                UiIntent::None
            }
            _ => UiIntent::None,
        }
    }

    fn focus_editor_cell(&mut self, column: u16, row: u16) {
        let (Some(area), Some(path)) = (self.editor_area, self.active_buffer.clone()) else {
            return;
        };
        let Some(project) = self.development.as_mut() else {
            return;
        };
        let Some(buffer) = project.buffer(&path) else {
            return;
        };
        let target_line = usize::from(row.saturating_sub(area.y).saturating_sub(1));
        let target_column = usize::from(column.saturating_sub(area.x).saturating_sub(7));
        let mut cursor = 0_usize;
        for (index, line) in buffer.content.split_inclusive('\n').enumerate() {
            if index == target_line {
                cursor = cursor
                    .saturating_add(target_column.min(line.trim_end_matches('\n').chars().count()));
                break;
            }
            cursor = cursor.saturating_add(line.chars().count());
        }
        self.editor_cursor = cursor.min(buffer.content.chars().count());
        self.editor_focus = true;
        self.set_status(format!("Editing {path} — Ctrl-S save, Esc command focus"));
    }

    fn reduce_key(&mut self, key: KeyEvent) -> UiIntent {
        if key.kind != KeyEventKind::Press {
            return UiIntent::None;
        }

        if self.mode == WorkspaceMode::Development && self.editor_focus {
            return self.reduce_editor_key(key);
        }

        if self.tap_mode && self.input.is_empty() {
            match key.code {
                KeyCode::Char(character @ '1'..='9') if key.modifiers == KeyModifiers::NONE => {
                    return match self.tap_operation(character as usize - '0' as usize) {
                        Ok(operation) => UiIntent::Pointer(operation),
                        Err(error) => {
                            self.report_error(error);
                            UiIntent::None
                        }
                    };
                }
                KeyCode::Esc => {
                    self.tap_mode = false;
                    self.set_status("Semantic tap mode closed");
                    return UiIntent::None;
                }
                _ => {}
            }
        }

        if self.display_class == DisplayClass::Phone && self.input.is_empty() {
            match key.code {
                KeyCode::Char(character @ '1'..='6') if key.modifiers == KeyModifiers::NONE => {
                    if let Some(view) = MobileView::from_number(character as u8 - b'0') {
                        self.set_mobile_view(view);
                    }
                    return UiIntent::None;
                }
                KeyCode::Tab => {
                    self.cycle_mobile_view(1);
                    return UiIntent::None;
                }
                KeyCode::BackTab => {
                    self.cycle_mobile_view(-1);
                    return UiIntent::None;
                }
                KeyCode::Char('?') if key.modifiers == KeyModifiers::NONE => {
                    self.mobile_help = !self.mobile_help;
                    return UiIntent::None;
                }
                _ => {}
            }
        }

        if let KeyCode::F(number) = key.code {
            let mode = match number {
                1 => Some(WorkspaceMode::Browser),
                2 => Some(WorkspaceMode::Split),
                3 => Some(WorkspaceMode::Workflow),
                4 => Some(WorkspaceMode::Semantic),
                5 => Some(WorkspaceMode::Inspect),
                6 => Some(WorkspaceMode::Takeover),
                7 if self.development_enabled => Some(WorkspaceMode::Development),
                _ => None,
            };
            if let Some(mode) = mode {
                self.set_mode(mode);
                return UiIntent::None;
            }
        }

        match key.code {
            KeyCode::Char('q' | 'c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                UiIntent::Quit
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.set_status("Screen redrawn");
                UiIntent::None
            }
            KeyCode::Char(':' | '/')
                if self.input.is_empty() && key.modifiers == KeyModifiers::NONE =>
            {
                self.set_status("Command palette · type help for the complete command index");
                UiIntent::None
            }
            KeyCode::Char('q') if self.input.is_empty() => UiIntent::Quit,
            KeyCode::Esc => {
                let cancellation = self.busy.as_mut().and_then(|busy| {
                    if busy.cancelling {
                        None
                    } else {
                        busy.cancelling = true;
                        Some((busy.id, busy.label.clone()))
                    }
                });
                if let Some((id, label)) = cancellation {
                    self.set_status(format!("Cancelling: {label}"));
                    self.add_activity(format!("Cancellation requested: {label}"));
                    UiIntent::Cancel(id)
                } else if self.busy.is_some() {
                    UiIntent::None
                } else if self.error_msg.is_some() {
                    self.clear_error();
                    UiIntent::None
                } else if self.mobile_help {
                    self.mobile_help = false;
                    UiIntent::None
                } else if self.display_class == DisplayClass::Phone
                    && self.mobile_view != MobileView::Home
                {
                    self.set_mobile_view(MobileView::Home);
                    UiIntent::None
                } else {
                    UiIntent::Quit
                }
            }
            KeyCode::Enter if !self.input.trim().is_empty() => {
                let command = std::mem::take(&mut self.input);
                self.cursor_pos = 0;
                UiIntent::Submit(command)
            }
            KeyCode::Backspace => {
                self.remove_before_cursor();
                UiIntent::None
            }
            KeyCode::Delete => {
                self.remove_at_cursor();
                UiIntent::None
            }
            KeyCode::Left => {
                self.cursor_pos = self.cursor_pos.saturating_sub(1);
                UiIntent::None
            }
            KeyCode::Right => {
                self.cursor_pos = (self.cursor_pos + 1).min(self.input.chars().count());
                UiIntent::None
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                UiIntent::None
            }
            KeyCode::End => {
                self.cursor_pos = self.input.chars().count();
                UiIntent::None
            }
            KeyCode::PageUp => {
                self.page_scroll = self.page_scroll.saturating_sub(10);
                UiIntent::None
            }
            KeyCode::PageDown => {
                self.page_scroll = self.page_scroll.saturating_add(10);
                UiIntent::None
            }
            KeyCode::Up if self.input.is_empty() => {
                self.move_intent_selection(-1);
                UiIntent::None
            }
            KeyCode::Down if self.input.is_empty() => {
                self.move_intent_selection(1);
                UiIntent::None
            }
            KeyCode::Char(character) => {
                if !self.insert_char(character) {
                    self.report_error(format!(
                        "Command input is limited to {TUI_INPUT_MAX_BYTES} bytes."
                    ));
                }
                UiIntent::None
            }
            _ => UiIntent::None,
        }
    }

    fn browser_ready(&self) -> bool {
        self.browser_state == BrowserState::Ready
    }

    fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    fn allocate_operation_id(&mut self) -> u64 {
        let id = self.next_operation_id;
        self.next_operation_id = self.next_operation_id.checked_add(1).unwrap_or(1);
        id
    }

    fn begin_operation(&mut self, id: u64, label: impl Into<String>) {
        let label = bounded_text(&label.into(), TUI_ACTIVITY_MAX_BYTES);
        self.busy = Some(BusyState {
            id,
            label: label.clone(),
            cancelling: false,
            spinner: 0,
            lease_revision: None,
        });
        self.set_status(format!("Queued: {label}"));
        self.add_activity(format!("Queued: {label}"));
    }

    fn attach_mutation_lease(&mut self, id: u64, revision: u64) {
        if let Some(busy) = self.busy.as_mut().filter(|busy| busy.id == id) {
            busy.lease_revision = Some(revision);
        }
    }

    fn release_operation_lease(&mut self, id: u64) {
        let Some(lease_revision) = self
            .busy
            .as_ref()
            .filter(|busy| busy.id == id)
            .and_then(|busy| busy.lease_revision)
        else {
            return;
        };
        let _ = self
            .mutation_lease
            .release(MutationActor::Human, lease_revision);
        if let Some(busy) = self.busy.as_mut().filter(|busy| busy.id == id) {
            busy.lease_revision = None;
        }
    }

    fn release_all_mutation_leases(&mut self) {
        if matches!(
            self.mutation_lease.state(),
            MutationLeaseState::Held(MutationActor::Human)
        ) {
            let revision = self.mutation_lease.revision();
            let _ = self.mutation_lease.release(MutationActor::Human, revision);
        }
        if let Some(busy) = self.busy.as_mut() {
            busy.lease_revision = None;
        }
    }

    fn finish_operation(&mut self, id: u64) {
        if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
            self.release_operation_lease(id);
            self.busy = None;
            if self.browser_ready() {
                self.set_status("Ready");
            }
        }
    }

    fn cancellation_enqueue_failed(&mut self, id: u64) {
        let label = self.busy.as_mut().filter(|busy| busy.id == id).map(|busy| {
            busy.cancelling = false;
            busy.label.clone()
        });
        if let Some(label) = label {
            self.set_status(format!("Working: {label}"));
        }
    }

    fn tick_busy(&mut self) {
        let status = self.busy.as_mut().map(|busy| {
            busy.spinner = busy.spinner.wrapping_add(1);
            if busy.cancelling {
                format!("Cancelling: {}", busy.label)
            } else {
                let frame = ['|', '/', '-', '\\'][busy.spinner % 4];
                format!("{frame} Working: {}", busy.label)
            }
        });
        if let Some(status) = status {
            self.set_status(status);
        }
    }
    fn apply_browser_event(
        &mut self,
        event: BrowserEvent,
    ) -> BrowserResult<Option<BrowserOperation>> {
        match event {
            BrowserEvent::Connecting => {
                self.browser_state = BrowserState::Connecting;
                self.visual_revision = 0;
                self.browser_target_id = None;
                self.page_content = "Browser connection changed; semantic state is invalid until a fresh observation completes.".into();
                let _ = self.graphics.clear_pane();
                self.set_status("Connecting to Chrome…");
                self.add_activity("Browser worker is connecting.");
            }
            BrowserEvent::Ready { port } => {
                self.browser_state = BrowserState::Ready;
                self.browser_recovery = None;
                self.set_status(format!("Connected on port {port}"));
                self.add_activity("Connected to Chrome.");
                return Ok(Some(BrowserOperation::Observe { fresh: true }));
            }
            BrowserEvent::StartupFailed { message } => {
                self.browser_state = BrowserState::Recovery;
                self.release_all_mutation_leases();
                self.busy = None;
                self.set_status("Browser unavailable");
                self.report_error(message);
            }
            BrowserEvent::RecoveryRequired { probe } => {
                self.browser_state = BrowserState::Recovery;
                let action = match probe.classification {
                    EndpointClassification::Free => "reconnect or launch with a chosen port",
                    EndpointClassification::CompatibleBrowser => {
                        "choose browser attach [TARGET], browser launch --port auto, or semantic-only"
                    }
                    EndpointClassification::UnrelatedService | EndpointClassification::Unknown => {
                        "choose browser launch --port auto, reconnect, or semantic-only"
                    }
                };
                self.set_page_content(format_recovery_probe(&probe));
                self.set_status(format!("Browser recovery · {action}"));
                self.add_activity(format!("Endpoint {}: {}", probe.port, probe.detail));
                self.browser_recovery = Some(probe);
                self.set_mobile_view(MobileView::App);
            }
            BrowserEvent::TargetsDiscovered { probe } => {
                self.set_page_content(format_recovery_probe(&probe));
                self.set_status(format!(
                    "Browser targets · {} eligible on port {}",
                    probe.targets.len(),
                    probe.port
                ));
                self.add_activity(format!(
                    "Refreshed bounded browser targets on port {}.",
                    probe.port
                ));
                self.browser_recovery = Some(probe);
                self.set_mobile_view(MobileView::App);
            }
            BrowserEvent::SemanticOnly { message } => {
                self.browser_state = BrowserState::SemanticOnly;
                self.release_all_mutation_leases();
                self.busy = None;
                self.set_status("Browser disconnected · project workspace remains active");
                self.add_activity(message);
            }
            BrowserEvent::RemoteViewStatus { message } => {
                self.set_page_content(message.clone());
                self.set_status("Remote View");
                self.add_activity(message);
                self.set_mobile_view(MobileView::App);
            }
            BrowserEvent::OperationStarted { id, label } => {
                if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
                    self.set_status(format!("Working: {label}"));
                    self.add_activity(format!("Started: {label}"));
                }
            }
            BrowserEvent::OperationFinished { id, result } => {
                self.finish_operation(id);
                if let Some(update) = result.update {
                    self.apply_page_update(update)?;
                }
                self.add_activity(result.activity);
            }
            BrowserEvent::OperationFailed { id, message } => {
                if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
                    self.finish_operation(id);
                    self.report_error(message);
                } else {
                    self.add_activity(format!("Rejected operation {id}: {message}"));
                }
            }
            BrowserEvent::VisualFrame {
                data,
                metadata,
                browser_revision,
            } => {
                self.apply_visual_frame(data, metadata, browser_revision)?;
            }
            BrowserEvent::VisualStatus { message } => {
                self.apply_visual_status(message);
            }
            BrowserEvent::OperationCancelled { id } => {
                if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
                    self.finish_operation(id);
                    self.add_activity(format!("Cancelled operation {id}."));
                }
            }
            BrowserEvent::WorkerFailed { message } => {
                self.browser_state = BrowserState::Unavailable;
                self.release_all_mutation_leases();
                self.busy = None;
                self.set_status("Browser worker failed");
                self.report_error(message);
            }
            BrowserEvent::WorkerStopped => {
                self.browser_state = BrowserState::Stopped;
                self.release_all_mutation_leases();
                self.busy = None;
                self.set_status("Browser worker stopped");
                self.add_activity("Browser worker stopped.");
            }
        }
        Ok(None)
    }

    fn apply_page_update(&mut self, update: PageUpdate) -> BrowserResult<()> {
        let result = match update {
            PageUpdate::Context(context) => self.apply_context(&context),
            PageUpdate::Semantic(observation) => self.apply_semantic(&observation),
            PageUpdate::IntentResolution { request, result } => {
                self.apply_intent_resolution(*request, *result)
            }

            PageUpdate::Text { page, text } => {
                self.apply_page_header(&page);
                self.set_page_content(text);
                Ok(())
            }
        };
        if result.is_ok() && self.display_class == DisplayClass::Phone {
            self.set_mobile_view(MobileView::App);
        }
        result
    }
    fn apply_visual_frame(
        &mut self,
        data: String,
        metadata: Value,
        browser_revision: u64,
    ) -> BrowserResult<()> {
        if data.len() > MAX_VISUAL_ENCODED_BYTES {
            self.apply_visual_status("visual frame rejected: encoded payload exceeds limit");
            return Ok(());
        }
        let metadata_bytes = serde_json::to_vec(&metadata)?;
        if metadata_bytes.len() > TUI_HEADER_MAX_BYTES {
            self.apply_visual_status("visual frame rejected: metadata exceeds limit");
            return Ok(());
        }
        if !self.live.enabled() {
            return Ok(());
        }
        let pane = self.graphics.pane();
        if pane.is_empty() {
            return Ok(());
        }
        let payload = match base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) {
            Ok(payload) if payload.len() <= MAX_FRAME_BYTES => payload,
            Ok(_) => {
                self.apply_visual_status("visual frame rejected: decoded payload exceeds limit");
                return Ok(());
            }
            Err(_) => {
                self.apply_visual_status("visual frame rejected: invalid base64 payload");
                return Ok(());
            }
        };
        let image_size = match imagesize::blob_size(&payload) {
            Ok(size) => size,
            Err(error) => {
                self.apply_visual_status(format!("live frame rejected: {error}"));
                self.live.metrics.dropped();
                return Ok(());
            }
        };
        let image_width = u32::try_from(image_size.width).map_err(|_| "live PNG width overflow")?;
        let image_height =
            u32::try_from(image_size.height).map_err(|_| "live PNG height overflow")?;
        let viewport_width = metadata
            .get("deviceWidth")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(image_width);
        let viewport_height = metadata
            .get("deviceHeight")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(image_height);
        self.graphics.bind_browser_revision(browser_revision)?;
        let capture_scale = CaptureScale::new(self.live_capture_config().capture_scale)?;
        self.graphics.resize(
            pane,
            PixelSize::new(viewport_width, viewport_height),
            PixelSize::new(viewport_width, viewport_height),
            capture_scale,
            browser_revision,
        )?;
        self.visual_revision = browser_revision;
        self.live.metrics.received(payload.len());
        let geometry = self
            .graphics
            .geometry()
            .copied()
            .ok_or("live graphics geometry was not initialized")?;
        let frame = BrowserFrame {
            schema_version: crate::presentation::PRESENTATION_CONTRACT_SCHEMA_VERSION,
            generation: self
                .graphics
                .diagnostics()
                .accepted_frames
                .saturating_add(1),
            identity: TargetResourceIdentity::new("tui", Some("terminal".to_string()))?,
            acquired_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_millis() as u64),
            viewport: geometry.viewport,
            content: geometry.content,
            capture_scale: geometry.capture_scale,
            encoding: crate::presentation::FrameEncoding::Png,
            keyframe: true,
            damage: crate::presentation::FrameDamage::Full,
            browser_revision,
            geometry_revision: geometry.geometry_revision,
            dropped: Default::default(),
        };
        match self.live.backend {
            Some(ActiveLiveBackend::Herdr) => {
                let queued = self.live.herdr_worker.as_ref().is_some_and(|worker| {
                    worker.try_send(HerdrFrame {
                        png: payload,
                        image_width,
                        image_height,
                        viewport_col: i32::from(pane.x),
                        viewport_row: i32::from(pane.y),
                        grid_cols: u32::from(pane.width),
                        grid_rows: u32::from(pane.height),
                    })
                });
                if !queued {
                    self.live.metrics.dropped();
                } else {
                    self.live.metrics.presented();
                }
            }
            Some(ActiveLiveBackend::Kitty) => match self.submit_graphics_frame(frame, &payload) {
                Ok(SubmitResult::Stale | SubmitResult::Replaced) => self.live.metrics.dropped(),
                Ok(SubmitResult::Presented) => self.live.metrics.presented(),
                Ok(SubmitResult::Queued) => {}
                Err(error) => {
                    self.add_activity(format!(
                        "Kitty graphics failed ({error}); selecting fallback."
                    ));
                    self.live.fall_back_from(ActiveLiveBackend::Kitty);
                    self.sync_live_graphics_mode();
                }
            },
            Some(ActiveLiveBackend::Ansi) => {
                match self
                    .live
                    .ansi
                    .update_png(&payload, pane.width, pane.height, self.live.fit())
                {
                    Ok(update) if update.changed_cells == 0 => self.live.metrics.dropped(),
                    Ok(_) => self.live.metrics.presented(),
                    Err(error) => {
                        self.add_activity(format!(
                            "ANSI live renderer rejected a frame ({error}); semantic fallback active."
                        ));
                        self.live.fall_back_from(ActiveLiveBackend::Ansi);
                    }
                }
            }
            None => {}
        }
        self.live.adapt_quality();
        self.apply_visual_status(self.live_diagnostics());
        Ok(())
    }

    fn apply_context(&mut self, context: &PageContext) -> BrowserResult<()> {
        if context.screenshot.is_some() {
            return Err("TUI worker must not retain screenshot data".into());
        }
        self.apply_page_header(&context.page);
        self.tap_targets = context
            .accessibility
            .interactive
            .iter()
            .take(9)
            .map(|target| SemanticTapTarget {
                reference: target.reference.clone(),
                role: bounded_text(&target.role, 64),
                name: bounded_text(&target.name, 128),
            })
            .collect();
        self.set_page_content(serde_json::to_string_pretty(context)?);
        Ok(())
    }

    fn apply_semantic(&mut self, observation: &SemanticObservation) -> BrowserResult<()> {
        self.url = bounded_text(&observation.page.url, TUI_HEADER_MAX_BYTES);
        self.title = bounded_text(
            &format!("Glass — {}", observation.page.title),
            TUI_HEADER_MAX_BYTES,
        );
        self.tap_targets = observation
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .take(9)
            .map(|target| SemanticTapTarget {
                reference: target.reference.clone(),
                role: bounded_text(&target.role, 64),
                name: bounded_text(&target.name, 128),
            })
            .collect();
        self.set_page_content(serde_json::to_string_pretty(observation)?);
        Ok(())
    }

    fn tap_overlay(&self) -> String {
        if self.tap_targets.is_empty() {
            return "SEMANTIC ACTIONS\nNo actionable targets. Run `observe` and try again.".into();
        }
        let mut lines = vec!["SEMANTIC ACTIONS · tap N · Esc closes".to_string()];
        lines.extend(self.tap_targets.iter().enumerate().map(|(index, target)| {
            let label = if target.name.is_empty() {
                target.role.as_str()
            } else {
                target.name.as_str()
            };
            format!("[{}] {} · {}", index + 1, label, target.role)
        }));
        lines.join("\n")
    }

    fn tap_operation(&mut self, number: usize) -> Result<BrowserOperation, String> {
        let target = self
            .tap_targets
            .get(number.saturating_sub(1))
            .ok_or_else(|| {
                format!(
                    "tap target must be between 1 and {}",
                    self.tap_targets.len()
                )
            })?;
        self.tap_mode = false;
        Ok(BrowserOperation::Click(target.reference.clone()))
    }

    fn apply_intent_resolution(
        &mut self,
        request: SemanticIntentRequest,
        result: SemanticIntentResult,
    ) -> BrowserResult<()> {
        self.intent_request = Some(request);
        self.intent_selection = 0;
        self.intent_result = Some(result.clone());
        self.url = result
            .route
            .as_ref()
            .map(|route| bounded_text(&route.url, TUI_HEADER_MAX_BYTES))
            .unwrap_or_default();
        self.title = "Glass — Intent resolution".into();
        self.set_page_content(format_intent_debug(&result, self.intent_selection));
        Ok(())
    }

    fn move_intent_selection(&mut self, delta: isize) {
        let Some(result) = self.intent_result.as_ref() else {
            return;
        };
        if result.candidates.is_empty() {
            return;
        }
        let maximum = result.candidates.len() - 1;
        self.intent_selection = if delta.is_negative() {
            self.intent_selection.saturating_sub(delta.unsigned_abs())
        } else {
            self.intent_selection
                .saturating_add(delta as usize)
                .min(maximum)
        };
        let content = format_intent_debug(result, self.intent_selection);
        let candidate_id = result.candidates[self.intent_selection].id.clone();
        self.set_page_content(content);
        self.set_status(format!(
            "Selected {} — submit: intent execute.",
            candidate_id
        ));
    }

    fn apply_page_header(&mut self, page: &PageInfo) {
        self.url = bounded_text(&page.url, TUI_HEADER_MAX_BYTES);
        self.browser_target_id = (!page.target_id.is_empty()).then(|| page.target_id.clone());
        self.title = bounded_text(&format!("Glass — {}", page.title), TUI_HEADER_MAX_BYTES);
    }

    fn set_page_content(&mut self, content: impl Into<String>) {
        self.page_content = bounded_text(&content.into(), TUI_PAGE_MAX_BYTES);
        self.page_scroll = 0;
    }
    fn sync_graphics_geometry(&mut self, area: Rect) -> BrowserResult<bool> {
        let next_class = display_class(self.layout_preference, area.width, self.remote_context);
        self.connection_environment.layout = match next_class {
            DisplayClass::Phone => LayoutClass::Phone,
            DisplayClass::Compact => LayoutClass::Compact,
            DisplayClass::Wide => LayoutClass::Wide,
        };
        self.connection_environment.terminal_columns = area.width.max(1);
        self.connection_environment.terminal_rows = area.height.max(1);
        if next_class != self.display_class {
            self.display_class = next_class;
            self.graphics.clear_pane()?;
            if next_class == DisplayClass::Phone && self.development_enabled {
                self.set_mode(WorkspaceMode::Development);
            }
            self.add_activity(format!("Responsive layout: {:?}.", next_class));
        }
        let root = root_regions(area, self.display_class);
        self.mobile_nav_area = root.nav;
        self.editor_area = if self.display_class == DisplayClass::Wide
            && self.mode == WorkspaceMode::Development
        {
            Some(wide_development_regions(root.content).editor)
        } else {
            None
        };
        let Some(pane) = live_panel_region(
            area,
            self.display_class,
            self.mode,
            self.mobile_view,
            self.live.enabled(),
        ) else {
            self.live_area = None;
            self.live.ansi.clear();
            self.live.stop_herdr();
            return Ok(self.graphics.clear_pane()?);
        };
        self.live.ensure_herdr();
        self.live_area = Some(Rect {
            x: pane.x.saturating_add(1),
            y: pane.y.saturating_add(1),
            width: pane.width.saturating_sub(2),
            height: pane.height.saturating_sub(2),
        });
        let image_width = pane.width.saturating_sub(2);
        let image_height = pane.height.saturating_sub(2);
        if image_width == 0 || image_height == 0 {
            let changed = self.graphics.clear_pane()?;
            if changed {
                self.add_activity("Graphics pane is empty; released terminal frame.");
            }
            return Ok(changed);
        }
        let changed = self.graphics.resize(
            PaneArea::new(
                pane.x.saturating_add(1),
                pane.y.saturating_add(1),
                image_width,
                image_height,
            ),
            PixelSize::new(image_width as u32, image_height as u32),
            PixelSize::new(image_width as u32, image_height as u32),
            CaptureScale::new(self.live_policy().capture_scale)?,
            self.graphics.browser_revision(),
        )?;
        if changed {
            self.add_activity(format!(
                "Graphics image area resized to {}x{} ({}).",
                image_width,
                image_height,
                self.graphics.mode().label()
            ));
        }
        Ok(changed)
    }

    fn graphics_shutdown(&mut self) -> io::Result<()> {
        let cleanup = self.graphics.shutdown();
        if cleanup.is_empty() {
            return Ok(());
        }
        let mut stdout = io::stdout();
        stdout.write_all(&cleanup)?;
        stdout.flush()
    }

    pub fn submit_graphics_frame(
        &mut self,
        frame: BrowserFrame,
        payload: &[u8],
    ) -> BrowserResult<SubmitResult> {
        Ok(self.graphics.submit(frame, payload)?)
    }

    fn render_graphics(&mut self) -> BrowserResult<()> {
        let presented_before = self.graphics.diagnostics().presented_frames;
        let rendered = match self.graphics.render_current(&self.page_content) {
            Ok(rendered) => rendered,
            Err(error) if self.live.backend == Some(ActiveLiveBackend::Kitty) => {
                self.add_activity(format!(
                    "Kitty graphics render failed ({error}); selecting fallback."
                ));
                self.live.fall_back_from(ActiveLiveBackend::Kitty);
                self.sync_live_graphics_mode();
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        if rendered.mode == crate::terminal_graphics::GraphicsMode::Kitty {
            let mut stdout = io::stdout();
            stdout.write_all(&rendered.bytes)?;
            stdout.flush()?;
        }
        if let Err(error) = self.graphics.present_pending() {
            if self.live.backend == Some(ActiveLiveBackend::Kitty) {
                self.add_activity(format!(
                    "Kitty graphics presentation failed ({error}); selecting fallback."
                ));
                self.live.fall_back_from(ActiveLiveBackend::Kitty);
                self.sync_live_graphics_mode();
                return Ok(());
            }
            return Err(error.into());
        }
        if self.graphics.diagnostics().presented_frames > presented_before {
            self.live.metrics.presented();
        }
        Ok(())
    }
}

#[derive(Debug)]
enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Redraw,
    Error(String),
}

struct InputWorker {
    shutdown: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl InputWorker {
    fn spawn(events: mpsc::Sender<InputEvent>) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let join = thread::spawn(move || {
            while !worker_shutdown.load(Ordering::Relaxed) {
                match event::poll(INPUT_POLL) {
                    Ok(false) => {}
                    Ok(true) => match event::read() {
                        Ok(Event::Key(key)) => {
                            if events.blocking_send(InputEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(Event::Mouse(mouse)) => {
                            if events.blocking_send(InputEvent::Mouse(mouse)).is_err() {
                                break;
                            }
                        }
                        Ok(Event::Paste(text)) => {
                            if events.blocking_send(InputEvent::Paste(text)).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {
                            if events.blocking_send(InputEvent::Redraw).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = events.blocking_send(InputEvent::Error(error.to_string()));
                            break;
                        }
                    },

                    Err(error) => {
                        let _ = events.blocking_send(InputEvent::Error(error.to_string()));
                        break;
                    }
                }
            }
        });
        Self {
            shutdown,
            join: Some(join),
        }
    }
    fn stop(&mut self) -> io::Result<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| io::Error::other("input worker thread panicked"))?;
        }
        Ok(())
    }
}

impl Drop for InputWorker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalCommand {
    Help,
    Safari,
    Inbox,
    Notifications(Option<bool>),
    Tap(Option<usize>),
    VerificationCard,
    Capsule(CapsuleCommand),
    Live(LiveCommand),
    Profiles,
    Knowledge(Option<String>),
    Daemon(DaemonView),
    Project(String),
    BrowserControl(BrowserControlCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BrowserControlCommand {
    Reconnect,
    Launch(BrowserLaunchRequest),
    Attach(BrowserAttachRequest),
    Targets(Option<u16>),
    Disconnect,
    Status,
    RemoteView(RemoteViewCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserAttachRequest {
    port: Option<u16>,
    target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserLaunchRequest {
    port: BrowserPortChoice,
    headed: Option<bool>,
    profile: Option<String>,
    incognito: Option<bool>,
    /// Outer `None` keeps the current preference. `Some(None)` restores
    /// executable auto-detection; `Some(Some(path))` selects a binary.
    chrome_path: Option<Option<PathBuf>>,
}

impl Default for BrowserLaunchRequest {
    fn default() -> Self {
        Self {
            port: BrowserPortChoice::Current,
            headed: None,
            profile: None,
            incognito: None,
            chrome_path: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserPortChoice {
    Current,
    Automatic,
    Exact(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveCommand {
    Show,
    Mode(TuiLiveMode),
    Backend(TuiLiveBackend),
    Quality(TuiLiveQuality),
    AdaptiveQuality,
    Fit(TuiLiveFit),
    Doctor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapsuleCommand {
    Save,
    Show,
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonView {
    Status,
    Doctor,
    Logs,
    Recovery,
}

#[derive(Debug, Clone, PartialEq)]
enum BrowserOperation {
    Navigate(String),
    Screenshot(String),
    Text,
    Dom,
    Observe {
        fresh: bool,
    },
    Semantic {
        level: SemanticObservationLevel,
        region: Option<String>,
    },
    Click(String),
    ClickAt {
        x: f64,
        y: f64,
        expected_revision: u64,
    },
    DoubleClick(String),
    Hover(String),
    Clear(String),
    Check(String),
    Uncheck(String),
    Select {
        target: String,
        value: String,
    },
    Type(String),
    KeyPress(String),
    Shortcut(String),
    Scroll {
        dx: f64,
        dy: f64,
    },
    /// Scroll captured from a live pane snapshot; stale revisions are rejected
    /// at execution rather than mutating a newer page.
    ScrollAt {
        dx: f64,
        dy: f64,
        expected_revision: u64,
    },
    AcceptDialog,
    DismissDialog,
    DismissConsent,
    Evaluate(String),
    Workflow(String),
    ResolveIntent(String),
    ExecuteIntent(Box<SemanticIntentExecutionRequest>),
}

impl BrowserOperation {
    fn label(&self) -> &'static str {
        match self {
            Self::Navigate(_) => "Navigate",
            Self::Screenshot(_) => "Screenshot",
            Self::Text => "Text",
            Self::Dom => "Compact DOM",
            Self::Observe { .. } => "Observe",
            Self::ClickAt { .. } => "Coordinate click",
            Self::Semantic { .. } => "Semantic observe",
            Self::Click(_) => "Click",
            Self::DoubleClick(_) => "Double-click",
            Self::Hover(_) => "Hover",
            Self::Clear(_) => "Clear",
            Self::Scroll { .. } | Self::ScrollAt { .. } => "Scroll",
            Self::Check(_) => "Check",
            Self::Uncheck(_) => "Uncheck",
            Self::Select { .. } => "Select",
            Self::Type(_) => "Type",
            Self::KeyPress(_) => "Key press",
            Self::Shortcut(_) => "Shortcut",
            Self::AcceptDialog => "Accept dialog",
            Self::DismissDialog => "Dismiss dialog",
            Self::DismissConsent => "Dismiss consent",
            Self::Evaluate(_) => "Evaluate",
            Self::Workflow(_) => "Workflow",
            Self::ResolveIntent(_) => "Resolve intent",
            Self::ExecuteIntent(_) => "Execute intent",
        }
    }
    fn requires_human_lease(&self) -> bool {
        !matches!(
            self,
            Self::Screenshot(_)
                | Self::Text
                | Self::Dom
                | Self::Observe { .. }
                | Self::Semantic { .. }
                | Self::ResolveIntent(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ParsedCommand {
    Local(LocalCommand),
    Browser(BrowserOperation),
}

fn parse_browser_port(value: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| "browser port must be an integer from 1 through 65535".into())
}

fn parse_browser_attach(arguments: &str) -> Result<BrowserAttachRequest, String> {
    let mut tokens = arguments.split_whitespace();
    let mut request = BrowserAttachRequest {
        port: None,
        target: None,
    };
    while let Some(token) = tokens.next() {
        if token.eq_ignore_ascii_case("--port") {
            let value = tokens
                .next()
                .ok_or_else(|| "browser attach --port expects a port".to_string())?;
            request.port = Some(parse_browser_port(value)?);
        } else if request.target.is_none() {
            request.target = Some(token.to_string());
        } else {
            return Err("browser attach accepts one target ID or target number".into());
        }
    }
    Ok(request)
}

fn parse_browser_launch(arguments: &str) -> Result<BrowserLaunchRequest, String> {
    let mut request = BrowserLaunchRequest::default();
    let mut tokens = arguments.split_whitespace();
    while let Some(token) = tokens.next() {
        match token.to_ascii_lowercase().as_str() {
            "--port" => {
                let value = tokens
                    .next()
                    .ok_or_else(|| "browser launch --port expects auto or a port".to_string())?;
                request.port = if value.eq_ignore_ascii_case("auto") {
                    BrowserPortChoice::Automatic
                } else {
                    BrowserPortChoice::Exact(parse_browser_port(value)?)
                };
            }
            "--headed" => request.headed = Some(true),
            "--headless" => request.headed = Some(false),
            "--incognito" | "--ephemeral" => request.incognito = Some(true),
            "--persistent" => request.incognito = Some(false),
            "--profile" => {
                request.profile = Some(
                    tokens
                        .next()
                        .ok_or_else(|| "browser launch --profile expects a name".to_string())?
                        .to_string(),
                );
            }
            "--chrome-path" => {
                let value = tokens.next().ok_or_else(|| {
                    "browser launch --chrome-path expects auto or an executable path".to_string()
                })?;
                request.chrome_path = Some(if value.eq_ignore_ascii_case("auto") {
                    None
                } else {
                    Some(PathBuf::from(value))
                });
            }
            _ => return Err(format!("unknown browser launch option '{token}'")),
        }
    }
    if request.incognito == Some(true) && request.profile.is_some() {
        return Err("browser launch cannot combine --incognito with --profile".into());
    }
    Ok(request)
}

fn parse_command(input: &str) -> Result<ParsedCommand, String> {
    let command = input.trim();
    if command.is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if command.eq_ignore_ascii_case("help") {
        return Ok(ParsedCommand::Local(LocalCommand::Help));
    }
    if command.eq_ignore_ascii_case("browser") || command.eq_ignore_ascii_case("browser status") {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Status,
        )));
    }
    for (name, action) in [
        ("browser remote-view open", RemoteViewCommand::Open),
        ("browser remote-view close", RemoteViewCommand::Close),
        ("browser remote-view status", RemoteViewCommand::Status),
    ] {
        if command.eq_ignore_ascii_case(name) {
            return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
                BrowserControlCommand::RemoteView(action),
            )));
        }
    }
    if command.eq_ignore_ascii_case("browser retry")
        || command.eq_ignore_ascii_case("browser connect")
        || command.eq_ignore_ascii_case("browser reconnect")
    {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Reconnect,
        )));
    }
    if command.eq_ignore_ascii_case("browser auto-port") {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Launch(BrowserLaunchRequest {
                port: BrowserPortChoice::Automatic,
                ..BrowserLaunchRequest::default()
            }),
        )));
    }
    if command.eq_ignore_ascii_case("browser disconnect")
        || command.eq_ignore_ascii_case("browser semantic-only")
    {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Disconnect,
        )));
    }
    if command.eq_ignore_ascii_case("browser launch") {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Launch(BrowserLaunchRequest::default()),
        )));
    }
    if let Some(arguments) = strip_ascii_prefix(command, "browser launch ") {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Launch(parse_browser_launch(arguments)?),
        )));
    }
    if command.eq_ignore_ascii_case("browser targets")
        || command.eq_ignore_ascii_case("browser targets refresh")
    {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Targets(None),
        )));
    }
    if let Some(port) = strip_ascii_prefix(command, "browser targets ") {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Targets(Some(parse_browser_port(port.trim())?)),
        )));
    }
    if command.eq_ignore_ascii_case("browser attach") {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Attach(BrowserAttachRequest {
                port: None,
                target: None,
            }),
        )));
    }
    if let Some(arguments) = strip_ascii_prefix(command, "browser attach ") {
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Attach(parse_browser_attach(arguments)?),
        )));
    }
    if let Some(target) = strip_ascii_prefix(command, "browser target ") {
        let target = target.trim();
        if target.is_empty() {
            return Err("browser target expects a target ID or target number".into());
        }
        return Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
            BrowserControlCommand::Attach(BrowserAttachRequest {
                port: None,
                target: Some(target.to_string()),
            }),
        )));
    }
    if command.eq_ignore_ascii_case("inbox") {
        return Ok(ParsedCommand::Local(LocalCommand::Inbox));
    }
    if command.eq_ignore_ascii_case("notify") || command.eq_ignore_ascii_case("notify status") {
        return Ok(ParsedCommand::Local(LocalCommand::Notifications(None)));
    }
    if let Some(value) = strip_ascii_prefix(command, "notify ") {
        return match value.trim().to_ascii_lowercase().as_str() {
            "on" => Ok(ParsedCommand::Local(LocalCommand::Notifications(Some(
                true,
            )))),
            "off" => Ok(ParsedCommand::Local(LocalCommand::Notifications(Some(
                false,
            )))),
            _ => Err("notify expects on, off, or status".into()),
        };
    }
    if command.eq_ignore_ascii_case("tap") {
        return Ok(ParsedCommand::Local(LocalCommand::Tap(None)));
    }
    if let Some(number) = strip_ascii_prefix(command, "tap ") {
        let number = number
            .trim()
            .parse::<usize>()
            .map_err(|_| "tap requires a target number".to_string())?;
        return Ok(ParsedCommand::Local(LocalCommand::Tap(Some(number))));
    }
    if command.eq_ignore_ascii_case("verify card") {
        return Ok(ParsedCommand::Local(LocalCommand::VerificationCard));
    }
    for (name, operation) in [
        ("capsule save", CapsuleCommand::Save),
        ("capsule show", CapsuleCommand::Show),
        ("capsule clear", CapsuleCommand::Clear),
    ] {
        if command.eq_ignore_ascii_case(name) {
            return Ok(ParsedCommand::Local(LocalCommand::Capsule(operation)));
        }
    }
    if command.eq_ignore_ascii_case("live") || command.eq_ignore_ascii_case("live status") {
        return Ok(ParsedCommand::Local(LocalCommand::Live(LiveCommand::Show)));
    }
    if command.eq_ignore_ascii_case("live doctor") {
        return Ok(ParsedCommand::Local(LocalCommand::Live(
            LiveCommand::Doctor,
        )));
    }
    if let Some(value) = strip_ascii_prefix(command, "live backend ") {
        let backend = match value.trim().to_ascii_lowercase().as_str() {
            "auto" => TuiLiveBackend::Auto,
            "herdr" => TuiLiveBackend::Herdr,
            "kitty" => TuiLiveBackend::Kitty,
            "ansi" => TuiLiveBackend::Ansi,
            _ => return Err("live backend must be auto, herdr, kitty, or ansi".into()),
        };
        return Ok(ParsedCommand::Local(LocalCommand::Live(
            LiveCommand::Backend(backend),
        )));
    }
    if let Some(value) = strip_ascii_prefix(command, "live quality ") {
        if value.trim().eq_ignore_ascii_case("auto") {
            return Ok(ParsedCommand::Local(LocalCommand::Live(
                LiveCommand::AdaptiveQuality,
            )));
        }
        let quality = match value.trim().to_ascii_lowercase().as_str() {
            "data" => TuiLiveQuality::Data,
            "balanced" => TuiLiveQuality::Balanced,
            "smooth" => TuiLiveQuality::Smooth,
            _ => return Err("live quality must be auto, data, balanced, or smooth".into()),
        };
        return Ok(ParsedCommand::Local(LocalCommand::Live(
            LiveCommand::Quality(quality),
        )));
    }
    if let Some(value) = strip_ascii_prefix(command, "live fit ") {
        let fit = match value.trim().to_ascii_lowercase().as_str() {
            "contain" => TuiLiveFit::Contain,
            "cover" => TuiLiveFit::Cover,
            "actual" => TuiLiveFit::Actual,
            _ => return Err("live fit must be contain, cover, or actual".into()),
        };
        return Ok(ParsedCommand::Local(LocalCommand::Live(LiveCommand::Fit(
            fit,
        ))));
    }
    if let Some(value) = strip_ascii_prefix(command, "live ") {
        let mode = match value.trim().to_ascii_lowercase().as_str() {
            "on" => TuiLiveMode::On,
            "auto" => TuiLiveMode::Auto,
            "off" => TuiLiveMode::Off,
            _ => {
                return Err(
                    "live expects on, auto, off, status, doctor, backend, quality, or fit".into(),
                );
            }
        };
        return Ok(ParsedCommand::Local(LocalCommand::Live(LiveCommand::Mode(
            mode,
        ))));
    }
    if matches!(
        command.to_ascii_lowercase().as_str(),
        "safari" | "phone" | "open phone"
    ) {
        return Ok(ParsedCommand::Local(LocalCommand::Safari));
    }
    if command.eq_ignore_ascii_case("project") {
        return Ok(ParsedCommand::Local(LocalCommand::Project(
            "inspect".into(),
        )));
    }
    if let Some(project_command) = strip_ascii_prefix(command, "project ") {
        return required_command_argument(project_command, "project command")
            .map(|command| ParsedCommand::Local(LocalCommand::Project(command)));
    }
    if let Some(prompt) = strip_ascii_prefix(command, "agent ") {
        return required_command_argument(prompt, "agent prompt")
            .map(|prompt| ParsedCommand::Local(LocalCommand::Project(format!("agent {prompt}"))));
    }
    if command.eq_ignore_ascii_case("profiles") {
        return Ok(ParsedCommand::Local(LocalCommand::Profiles));
    }
    if command.eq_ignore_ascii_case("knowledge") {
        return Ok(ParsedCommand::Local(LocalCommand::Knowledge(None)));
    }
    if let Some(record_id) = strip_ascii_prefix(command, "knowledge show ") {
        return required_command_argument(record_id, "knowledge record ID")
            .map(|record_id| ParsedCommand::Local(LocalCommand::Knowledge(Some(record_id))));
    }
    if command.eq_ignore_ascii_case("daemon") || command.eq_ignore_ascii_case("daemon doctor") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(
            DaemonView::Doctor,
        )));
    }
    if command.eq_ignore_ascii_case("daemon status") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(
            DaemonView::Status,
        )));
    }
    if command.eq_ignore_ascii_case("daemon logs") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(DaemonView::Logs)));
    }
    if command.eq_ignore_ascii_case("daemon recovery") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(
            DaemonView::Recovery,
        )));
    }
    for prefix in ["navigate ", "go to ", "go "] {
        if let Some(url) = strip_ascii_prefix(command, prefix) {
            return required_command_argument(url, "URL")
                .map(BrowserOperation::Navigate)
                .map(ParsedCommand::Browser);
        }
    }
    if let Some(target) = strip_ascii_prefix(command, "double click ") {
        return required_command_argument(target, "double-click target")
            .map(BrowserOperation::DoubleClick)
            .map(ParsedCommand::Browser);
    }
    if let Some(target) = strip_ascii_prefix(command, "click ") {
        return required_command_argument(target, "click target")
            .map(BrowserOperation::Click)
            .map(ParsedCommand::Browser);
    }
    if let Some(target) = strip_ascii_prefix(command, "hover ") {
        return required_command_argument(target, "hover target")
            .map(BrowserOperation::Hover)
            .map(ParsedCommand::Browser);
    }
    for (prefix, operation, name) in [
        (
            "clear ",
            BrowserOperation::Clear as fn(String) -> BrowserOperation,
            "clear target",
        ),
        (
            "check ",
            BrowserOperation::Check as fn(String) -> BrowserOperation,
            "check target",
        ),
        (
            "uncheck ",
            BrowserOperation::Uncheck as fn(String) -> BrowserOperation,
            "uncheck target",
        ),
    ] {
        if let Some(target) = strip_ascii_prefix(command, prefix) {
            return required_command_argument(target, name)
                .map(operation)
                .map(ParsedCommand::Browser);
        }
    }
    if let Some(values) = strip_ascii_prefix(command, "select ") {
        return parse_target_value(values, "select target and value").map(|(target, value)| {
            ParsedCommand::Browser(BrowserOperation::Select { target, value })
        });
    }
    if let Some(text) = strip_ascii_prefix(command, "type ") {
        return required_command_argument(text, "text")
            .map(BrowserOperation::Type)
            .map(ParsedCommand::Browser);
    }
    if let Some(key) = strip_ascii_prefix(command, "press ") {
        return required_command_argument(key, "key")
            .map(BrowserOperation::KeyPress)
            .map(ParsedCommand::Browser);
    }
    if let Some(shortcut) = strip_ascii_prefix(command, "shortcut ") {
        return required_command_argument(shortcut, "shortcut")
            .map(BrowserOperation::Shortcut)
            .map(ParsedCommand::Browser);
    }
    if let Some(path) = strip_ascii_prefix(command, "workflow ") {
        return required_command_argument(path, "workflow JSON path")
            .map(BrowserOperation::Workflow)
            .map(ParsedCommand::Browser);
    }
    if let Some(path) = strip_ascii_prefix(command, "resolve-intent ") {
        return required_command_argument(path, "intent JSON path")
            .map(BrowserOperation::ResolveIntent)
            .map(ParsedCommand::Browser);
    }
    if command.eq_ignore_ascii_case("screenshot") {
        return Ok(ParsedCommand::Browser(BrowserOperation::Screenshot(
            "screenshot.png".to_string(),
        )));
    }
    if let Some(output) = strip_ascii_prefix(command, "screenshot ") {
        let output = output.trim();
        return Ok(ParsedCommand::Browser(BrowserOperation::Screenshot(
            if output.is_empty() {
                "screenshot.png".to_string()
            } else {
                output.to_string()
            },
        )));
    }
    if ["text", "content", "get text", "page text"]
        .iter()
        .any(|candidate| command.eq_ignore_ascii_case(candidate))
    {
        return Ok(ParsedCommand::Browser(BrowserOperation::Text));
    }
    if ["dom", "snapshot", "get dom"]
        .iter()
        .any(|candidate| command.eq_ignore_ascii_case(candidate))
    {
        return Ok(ParsedCommand::Browser(BrowserOperation::Dom));
    }
    if ["observe", "context"]
        .iter()
        .any(|candidate| command.eq_ignore_ascii_case(candidate))
    {
        return Ok(ParsedCommand::Browser(BrowserOperation::Observe {
            fresh: false,
        }));
    }
    if command.eq_ignore_ascii_case("semantic") {
        return Ok(ParsedCommand::Browser(BrowserOperation::Semantic {
            level: SemanticObservationLevel::Summary,
            region: None,
        }));
    }
    if let Some(values) = strip_ascii_prefix(command, "semantic ") {
        return parse_semantic_observation(values).map(ParsedCommand::Browser);
    }
    if command.eq_ignore_ascii_case("scroll") {
        return Ok(ParsedCommand::Browser(BrowserOperation::Scroll {
            dx: 0.0,
            dy: 600.0,
        }));
    }
    if let Some(values) = strip_ascii_prefix(command, "scroll ") {
        return parse_scroll(values).map(ParsedCommand::Browser);
    }
    if command.eq_ignore_ascii_case("accept-dialog") {
        return Ok(ParsedCommand::Browser(BrowserOperation::AcceptDialog));
    }
    if command.eq_ignore_ascii_case("dismiss-dialog") {
        return Ok(ParsedCommand::Browser(BrowserOperation::DismissDialog));
    }
    if command.eq_ignore_ascii_case("dismiss-consent") {
        return Ok(ParsedCommand::Browser(BrowserOperation::DismissConsent));
    }
    Ok(ParsedCommand::Browser(BrowserOperation::Evaluate(
        command.to_string(),
    )))
}

fn strip_ascii_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))?;
    Some(&value[prefix.len()..])
}

fn required_command_argument(value: &str, name: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{name} cannot be empty"))
    } else {
        Ok(value.to_string())
    }
}

fn safari_handoff(value: &str) -> Result<String, String> {
    let mut remote = url::Url::parse(value.trim()).map_err(|_| "app URL must be absolute")?;
    if !matches!(remote.scheme(), "http" | "https") {
        return Err("Safari handoff supports HTTP and HTTPS app URLs".into());
    }
    let _ = remote.set_username("");
    let _ = remote.set_password(None);
    let query = remote
        .query_pairs()
        .map(|(name, value)| {
            let lower = name.to_ascii_lowercase();
            let sensitive = [
                "password",
                "passwd",
                "token",
                "secret",
                "cookie",
                "authorization",
                "api_key",
                "apikey",
            ]
            .iter()
            .any(|marker| lower.contains(marker));
            (
                name.into_owned(),
                if sensitive { "[redacted]" } else { &value }.to_string(),
            )
        })
        .collect::<Vec<_>>();
    remote.set_query(None);
    if !query.is_empty() {
        remote
            .query_pairs_mut()
            .extend_pairs(query.iter().map(|(name, value)| (name, value)));
    }
    if remote.fragment().is_some_and(|fragment| {
        let lower = fragment.to_ascii_lowercase();
        ["token", "secret", "password", "authorization"]
            .iter()
            .any(|marker| lower.contains(marker))
    }) {
        remote.set_fragment(None);
    }
    let port = remote
        .port_or_known_default()
        .ok_or_else(|| "app URL must include a usable port".to_string())?;
    let mut local = remote.clone();
    local
        .set_host(Some("127.0.0.1"))
        .map_err(|_| "app URL host is invalid".to_string())?;
    local
        .set_port(Some(port))
        .map_err(|_| "app URL port is invalid".to_string())?;

    Ok(format!(
        "SAFARI HANDOFF (private by default)\n\n\
         Glass cannot open Safari from the remote shell. In the iPhone SSH app, add a local port forward:\n\n\
           local port:  {port}\n\
           remote host: 127.0.0.1\n\
           remote port: {port}\n\n\
         Keep that SSH tunnel connected, then open:\n\n\
           {local}\n\n\
         Remote app: {remote}\n\n\
         Do not bind Chrome CDP or the dev server publicly just to reach it. Herdr can preserve the Glass pane; the SSH client owns this Safari tunnel."
    ))
}

fn parse_scroll(values: &str) -> Result<BrowserOperation, String> {
    let mut values = values.split_whitespace();
    let dx = values
        .next()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| "scroll dx must be a number")
        })
        .transpose()?
        .unwrap_or(0.0);
    let dy = values
        .next()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| "scroll dy must be a number")
        })
        .transpose()?
        .unwrap_or(600.0);
    if values.next().is_some() {
        return Err("scroll accepts at most dx and dy".to_string());
    }
    Ok(BrowserOperation::Scroll { dx, dy })
}

fn parse_target_value(values: &str, name: &str) -> Result<(String, String), String> {
    let mut values = values.trim().splitn(2, char::is_whitespace);
    let target = values.next().unwrap_or_default().trim();
    let value = values.next().unwrap_or_default().trim();
    if target.is_empty() || value.is_empty() {
        return Err(format!("{name} requires two non-empty arguments"));
    }
    Ok((target.to_string(), value.to_string()))
}

fn parse_semantic_observation(values: &str) -> Result<BrowserOperation, String> {
    let mut values = values.split_whitespace();
    let level = match values
        .next()
        .unwrap_or("summary")
        .to_ascii_lowercase()
        .as_str()
    {
        "summary" => SemanticObservationLevel::Summary,
        "interactive" => SemanticObservationLevel::Interactive,
        "structured" => SemanticObservationLevel::Structured,
        "detailed" => SemanticObservationLevel::Detailed,
        "raw" => SemanticObservationLevel::Raw,
        _ => {
            return Err(
                "semantic level must be summary, interactive, structured, detailed, or raw".into(),
            );
        }
    };
    let region = values.next().map(str::to_string);
    if values.next().is_some() {
        return Err("semantic accepts a level and optional region ID".into());
    }
    Ok(BrowserOperation::Semantic { level, region })
}

fn format_intent_activity(result: &SemanticIntentResult) -> String {
    format!(
        "Intent {:?}: {} (policy={:?}, candidates={}, revision={}).",
        result.resolution,
        result.normalized_intent,
        result.policy_decision,
        result.candidates.len(),
        result
            .revision
            .map(|revision| revision.to_string())
            .unwrap_or_else(|| "unknown".into())
    )
}

fn format_intent_execution_activity(result: &SemanticIntentExecutionResult) -> String {
    match result.status {
        crate::browser::session::SemanticIntentExecutionStatus::Executed => format!(
            "Intent executed: candidate={} resolution={} execution={}.",
            result.candidate_id,
            result.resolution_id,
            result.execution_id.as_deref().unwrap_or("unknown")
        ),
        crate::browser::session::SemanticIntentExecutionStatus::NotExecuted => format!(
            "Intent not executed: candidate={} resolution={:?}; {}.",
            result.candidate_id,
            result.resolution.resolution,
            result
                .reason
                .as_deref()
                .unwrap_or("policy did not authorize dispatch")
        ),
    }
}

fn format_intent_debug(result: &SemanticIntentResult, selected: usize) -> String {
    let mut output = vec![
        format!("Normalized intent: {}", result.normalized_intent),
        format!("Resolution: {:?}", result.resolution),
        format!("Policy: {:?}", result.policy_decision),
        format!(
            "Revision: {}",
            result
                .revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "unknown".into())
        ),
        String::new(),
        "Candidates:".into(),
    ];
    if result.candidates.is_empty() {
        output.push("  (none)".into());
    } else {
        for (index, candidate) in result.candidates.iter().enumerate() {
            let evidence = candidate
                .evidence
                .iter()
                .map(|item| format!("{:?}: {}", item.category, item.detail))
                .collect::<Vec<_>>()
                .join("; ");
            output.push(format!(
                "  {}{} [{}] {} — {:?}",
                if index == selected { "> " } else { "  " },
                candidate.id,
                candidate.role,
                candidate.name,
                candidate.confidence
            ));
            if !evidence.is_empty() {
                output.push(format!("    evidence: {evidence}"));
            }
        }
    }
    if !result.excluded_candidates.is_empty() {
        output.push(format!(
            "Excluded candidates: {}",
            result.excluded_candidates.len()
        ));
        for candidate in &result.excluded_candidates {
            output.push(format!(
                "  {} — {:?}: {}",
                candidate.id, candidate.reason.category, candidate.reason.detail
            ));
        }
    }
    if let Some(reason) = &result.reason {
        output.push(format!("Reason: {reason}"));
    }
    output.join("\n")
}

#[derive(Debug)]
enum BrowserCommand {
    Execute {
        id: u64,
        operation: BrowserOperation,
    },
    Cancel {
        id: u64,
    },
    Recover(BrowserRecovery),
    InspectTargets(Option<u16>),
    RemoteView(RemoteViewCommand),
    Shutdown,
}

#[derive(Debug, Clone)]
enum BrowserRecovery {
    Reconnect,
    Launch(BrowserLaunchRequest),
    Attach {
        port: u16,
        target_id: Option<String>,
    },
    Disconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteViewCommand {
    Open,
    Close,
    Status,
}

#[derive(Debug)]
enum BrowserEvent {
    Connecting,
    Ready {
        port: u16,
    },
    StartupFailed {
        message: String,
    },
    RecoveryRequired {
        probe: EndpointProbe,
    },
    TargetsDiscovered {
        probe: EndpointProbe,
    },
    SemanticOnly {
        message: String,
    },
    RemoteViewStatus {
        message: String,
    },
    OperationStarted {
        id: u64,
        label: String,
    },
    OperationFinished {
        id: u64,
        result: Box<OperationResult>,
    },
    OperationFailed {
        id: u64,
        message: String,
    },
    VisualFrame {
        data: String,
        metadata: Value,
        browser_revision: u64,
    },
    VisualStatus {
        message: String,
    },
    OperationCancelled {
        id: u64,
    },
    WorkerFailed {
        message: String,
    },
    WorkerStopped,
}

#[derive(Debug)]
enum PageUpdate {
    Context(Box<PageContext>),
    Semantic(Box<SemanticObservation>),
    IntentResolution {
        request: Box<SemanticIntentRequest>,
        result: Box<SemanticIntentResult>,
    },
    Text {
        page: PageInfo,
        text: String,
    },
}

fn format_recovery_probe(probe: &EndpointProbe) -> String {
    let mut lines = vec![
        "Browser recovery".to_string(),
        format!("Port {} · {:?}", probe.port, probe.classification),
        probe.detail.clone(),
        String::new(),
    ];
    match probe.classification {
        EndpointClassification::CompatibleBrowser => {
            lines.push(
                "Verified CDP endpoint. Glass will never attach without your command.".into(),
            );
            if probe.targets.is_empty() {
                lines.push("No selectable page targets were disclosed.".into());
            } else {
                lines.push("Targets (title and origin only):".into());
                lines.extend(probe.targets.iter().enumerate().map(|(index, target)| {
                    format!(
                        "  {}. {} · {} · {}",
                        index + 1,
                        target.title,
                        target.origin,
                        target.id
                    )
                }));
            }
        }
        EndpointClassification::Free => {
            lines.push("The requested port is free; retry launch or allocate a fresh port.".into());
        }
        EndpointClassification::UnrelatedService => {
            lines.push("Another service owns this port. Attach is intentionally disabled.".into());
        }
        EndpointClassification::Unknown => {
            lines.push("Ownership is unknown. Glass will not attach automatically.".into());
        }
    }
    lines.push(String::new());
    lines.push(
        "browser reconnect | browser launch [OPTIONS] | browser targets [PORT] | browser attach [--port PORT] [TARGET] | browser semantic-only".into(),
    );
    lines.join("\n")
}

#[derive(Debug)]
struct OperationResult {
    activity: String,
    update: Option<PageUpdate>,
}

enum ActiveOperationState {
    Completed(BrowserResult<Box<OperationResult>>),
    Cancelled,
    Shutdown,
}

async fn browser_worker(
    viewport: Option<(i64, i64)>,
    options: SessionOptions,
    policy: BrowserPolicy,
    visual_mode: watch::Receiver<ScreencastConfig>,
    mut commands: mpsc::Receiver<BrowserCommand>,
    events: mpsc::Sender<BrowserEvent>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut options = options;
    let mut automatic_port_retries = 0_u8;
    loop {
        if !send_browser_event(&events, BrowserEvent::Connecting).await {
            return;
        }
        if let Some(session) = start_browser_session(
            &options,
            policy.clone(),
            viewport,
            &mut commands,
            &events,
            &mut shutdown,
        )
        .await
        {
            if !send_browser_event(&events, BrowserEvent::Ready { port: options.port }).await {
                let _ = session.close().await;
                return;
            }
            let exit = worker_loop(
                session,
                options.port,
                &mut commands,
                &events,
                &mut shutdown,
                visual_mode.clone(),
            )
            .await;
            let WorkerLoopExit::Recover(mut recovery) = exit else {
                return;
            };
            let mut announced_semantic_only = false;
            loop {
                if matches!(recovery, BrowserRecovery::Disconnect) {
                    if !announced_semantic_only {
                        let _ = send_browser_event(&events, BrowserEvent::SemanticOnly {
                            message: "Browser disconnected cleanly; project, editor, processes, and agent remain active.".into(),
                        }).await;
                        announced_semantic_only = true;
                    }
                } else {
                    let automatic = matches!(
                        &recovery,
                        BrowserRecovery::Launch(BrowserLaunchRequest {
                            port: BrowserPortChoice::Automatic,
                            ..
                        })
                    );
                    if configure_browser_recovery(&mut options, recovery, &events).await {
                        automatic_port_retries = if automatic { 2 } else { 0 };
                        break;
                    }
                    // Invalid settings or an ambiguous attach must not silently
                    // restart the previous target. Stay detached for another
                    // explicit recovery choice.
                    recovery = BrowserRecovery::Disconnect;
                    continue;
                }
                match commands.recv().await {
                    Some(BrowserCommand::Recover(BrowserRecovery::Disconnect)) => {}
                    Some(BrowserCommand::Recover(next)) => {
                        recovery = next;
                        announced_semantic_only = false;
                    }
                    Some(BrowserCommand::Execute { id, .. }) => {
                        let _ = send_browser_event(&events, BrowserEvent::OperationFailed { id, message: "browser is disconnected; use browser reconnect, launch, or attach".into() }).await;
                    }
                    Some(BrowserCommand::Cancel { id }) => {
                        let _ =
                            send_browser_event(&events, BrowserEvent::OperationCancelled { id })
                                .await;
                    }
                    Some(BrowserCommand::InspectTargets(port)) => {
                        let probe = probe_local_endpoint(port.unwrap_or(options.port)).await;
                        let _ =
                            send_browser_event(&events, BrowserEvent::TargetsDiscovered { probe })
                                .await;
                    }
                    Some(BrowserCommand::RemoteView(_)) => {
                        let _ = send_browser_event(
                            &events,
                            BrowserEvent::RemoteViewStatus {
                                message:
                                    "Remote View is closed because the browser is disconnected."
                                        .into(),
                            },
                        )
                        .await;
                    }
                    Some(BrowserCommand::Shutdown) | None => {
                        let _ = send_browser_event(&events, BrowserEvent::WorkerStopped).await;
                        return;
                    }
                }
            }
            continue;
        }
        if *shutdown.borrow() {
            return;
        }

        if automatic_port_retries > 0 {
            match reserve_loopback_port() {
                Ok((port, reservation)) => {
                    options.port = port;
                    automatic_port_retries -= 1;
                    drop(reservation);
                    continue;
                }
                Err(error) => {
                    let _ = send_browser_event(
                        &events,
                        BrowserEvent::StartupFailed {
                            message: format!(
                                "automatic browser port retry could not reserve loopback: {error}"
                            ),
                        },
                    )
                    .await;
                }
            }
        }

        let probe = probe_local_endpoint(options.port).await;
        if !send_browser_event(
            &events,
            BrowserEvent::RecoveryRequired {
                probe: probe.clone(),
            },
        )
        .await
        {
            return;
        }
        let mut disconnected = false;
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        let _ = send_browser_event(&events, BrowserEvent::WorkerStopped).await;
                        return;
                    }
                }
                command = commands.recv() => match command {
                    Some(BrowserCommand::Recover(recovery)) => {
                        if matches!(recovery, BrowserRecovery::Disconnect) {
                            disconnected = true;
                            let _ = send_browser_event(&events, BrowserEvent::SemanticOnly {
                                message: "Browser control disconnected; development state and commands remain available.".into(),
                            }).await;
                        } else {
                            let automatic = matches!(
                                &recovery,
                                BrowserRecovery::Launch(BrowserLaunchRequest {
                                    port: BrowserPortChoice::Automatic,
                                    ..
                                })
                            );
                            if configure_browser_recovery(&mut options, recovery, &events).await {
                                automatic_port_retries = if automatic { 2 } else { 0 };
                                break;
                            }
                        }
                    }
                    Some(BrowserCommand::Execute { id, .. }) => {
                        let _ = send_browser_event(&events, BrowserEvent::OperationFailed {
                            id,
                            message: "browser recovery is required before browser operations can run".into(),
                        }).await;
                    }
                    Some(BrowserCommand::Cancel { id }) => {
                        let _ = send_browser_event(&events, BrowserEvent::OperationCancelled { id }).await;
                    }
                    Some(BrowserCommand::InspectTargets(port)) => {
                        let refreshed = probe_local_endpoint(port.unwrap_or(options.port)).await;
                        let _ = send_browser_event(
                            &events,
                            BrowserEvent::TargetsDiscovered { probe: refreshed },
                        ).await;
                    }
                    Some(BrowserCommand::RemoteView(_)) => {
                        let _ = send_browser_event(&events, BrowserEvent::RemoteViewStatus {
                            message: "Remote View requires a connected browser; finish recovery first.".into(),
                        }).await;
                    }
                    Some(BrowserCommand::Shutdown) | None => {
                        let _ = send_browser_event(&events, BrowserEvent::WorkerStopped).await;
                        return;
                    }
                }
            }
        }
        if disconnected {
            let _ = send_browser_event(&events, BrowserEvent::Connecting).await;
        }
    }
}

async fn start_browser_session(
    options: &SessionOptions,
    policy: BrowserPolicy,
    viewport: Option<(i64, i64)>,
    commands: &mut mpsc::Receiver<BrowserCommand>,
    events: &mpsc::Sender<BrowserEvent>,
    shutdown: &mut watch::Receiver<bool>,
) -> Option<BrowserSession> {
    let start = BrowserSession::start_with_policy_and_viewport(options, policy, viewport);
    tokio::pin!(start);

    loop {
        if *shutdown.borrow() {
            let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
            return None;
        }
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
                    return None;
                }
            }
            command = commands.recv() => match command {
                Some(BrowserCommand::Shutdown) | None => {
                    let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
                    return None;
                }
                Some(BrowserCommand::Execute { id, .. }) => {
                    if !send_browser_event(events, BrowserEvent::OperationFailed {
                        id,
                        message: "browser is still starting".to_string(),
                    }).await {
                        return None;
                    }
                }
                Some(BrowserCommand::Cancel { id }) => {
                    if !send_browser_event(events, BrowserEvent::OperationCancelled { id }).await {
                        return None;
                    }
                }
                Some(BrowserCommand::InspectTargets(port)) => {
                    let probe = probe_local_endpoint(port.unwrap_or(options.port)).await;
                    let _ = send_browser_event(
                        events,
                        BrowserEvent::TargetsDiscovered { probe },
                    )
                    .await;
                }
                Some(BrowserCommand::Recover(_)) => {}
                Some(BrowserCommand::RemoteView(_)) => {
                    let _ = send_browser_event(events, BrowserEvent::RemoteViewStatus {
                        message: "Remote View requires browser startup to complete.".into(),
                    }).await;
                }
            },
            result = &mut start => match result {
                Ok(session) => return Some(session),
                Err(error) => {
                    let message = error.to_string();
                    drop(error);
                    let _ = send_browser_event(events, BrowserEvent::StartupFailed {
                        message,
                    }).await;
                    return None;
                }
            },
        }
    }
}

enum WorkerLoopExit {
    Shutdown,
    Recover(BrowserRecovery),
}

async fn configure_browser_recovery(
    options: &mut SessionOptions,
    recovery: BrowserRecovery,
    events: &mpsc::Sender<BrowserEvent>,
) -> bool {
    let mut candidate = options.clone();
    match recovery {
        BrowserRecovery::Reconnect => {}
        BrowserRecovery::Launch(request) => {
            candidate.attach = false;
            candidate.target_id = None;
            candidate.frame_id = None;
            match request.port {
                BrowserPortChoice::Current => {}
                BrowserPortChoice::Exact(port) => candidate.port = port,
                BrowserPortChoice::Automatic => match reserve_loopback_port() {
                    Ok((port, reservation)) => {
                        candidate.port = port;
                        drop(reservation);
                    }
                    Err(error) => {
                        let _ = send_browser_event(
                            events,
                            BrowserEvent::StartupFailed {
                                message: format!(
                                    "could not reserve an automatic loopback port: {error}"
                                ),
                            },
                        )
                        .await;
                        return false;
                    }
                },
            }
            if let Some(headed) = request.headed {
                candidate.headed = headed;
            }
            if let Some(profile) = request.profile {
                candidate.profile = profile;
                candidate.incognito = false;
            }
            if let Some(incognito) = request.incognito {
                candidate.incognito = incognito;
                if incognito {
                    candidate.profile = "default".into();
                }
            }
            if let Some(chrome_path) = request.chrome_path {
                candidate.chrome_path = chrome_path;
            }
        }
        BrowserRecovery::Attach {
            port,
            mut target_id,
        } => {
            let probe = probe_local_endpoint(port).await;
            if probe.classification != EndpointClassification::CompatibleBrowser {
                let _ = send_browser_event(
                    events,
                    BrowserEvent::StartupFailed {
                        message: format!(
                            "attach rejected: port {port} is not a verified Chrome/Chromium DevTools endpoint"
                        ),
                    },
                )
                .await;
                return false;
            }
            if let Some(selection) = target_id.as_deref()
                && let Ok(number) = selection.parse::<usize>()
            {
                target_id = number
                    .checked_sub(1)
                    .and_then(|index| probe.targets.get(index))
                    .map(|target| target.id.clone());
                if target_id.is_none() {
                    let _ = send_browser_event(
                        events,
                        BrowserEvent::StartupFailed {
                            message:
                                "attach rejected: target number is outside the current target list"
                                    .into(),
                        },
                    )
                    .await;
                    return false;
                }
            }
            if probe.targets.len() > 1 && target_id.is_none() {
                let _ = send_browser_event(events, BrowserEvent::TargetsDiscovered { probe }).await;
                return false;
            }
            if let Some(target_id) = target_id.as_ref()
                && !probe.targets.iter().any(|target| &target.id == target_id)
            {
                let _ = send_browser_event(
                    events,
                    BrowserEvent::StartupFailed {
                        message: format!(
                            "attach rejected: target '{target_id}' was not in the fresh bounded probe"
                        ),
                    },
                )
                .await;
                return false;
            }
            candidate.port = port;
            candidate.attach = true;
            candidate.target_id =
                target_id.or_else(|| probe.targets.first().map(|target| target.id.clone()));
            candidate.profile = "default".into();
            candidate.incognito = false;
            candidate.chrome_path = None;
            candidate.headed = false;
        }
        BrowserRecovery::Disconnect => return false,
    }
    if let Err(error) = candidate.validate() {
        let _ = send_browser_event(
            events,
            BrowserEvent::StartupFailed {
                message: format!("browser connection settings rejected: {error}"),
            },
        )
        .await;
        return false;
    }
    *options = candidate;
    true
}

async fn worker_loop(
    session: BrowserSession,
    current_port: u16,
    commands: &mut mpsc::Receiver<BrowserCommand>,
    events: &mpsc::Sender<BrowserEvent>,
    shutdown: &mut watch::Receiver<bool>,
    mut visual_mode: watch::Receiver<ScreencastConfig>,
) -> WorkerLoopExit {
    let mut remote_view = None::<crate::development::RemoteView>;
    let initial_revision = session
        .observe_fresh()
        .await
        .map(|observation| observation.accessibility.revision)
        .unwrap_or(1);
    let mut visual_config = *visual_mode.borrow();
    let mut screencast = if visual_config.enabled {
        start_tui_screencast(&session, events, visual_config).await
    } else {
        let _ = send_browser_event(
            events,
            BrowserEvent::VisualStatus {
                message: "semantic mobile view; use screenshot explicitly".into(),
            },
        )
        .await;
        None
    };
    let mut fallback_tick = time::interval(Duration::from_millis(750));
    fallback_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut remote_input_tick = time::interval(Duration::from_millis(40));
    remote_input_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut visual_revision = initial_revision;
    let mut last_frame_sent = None::<Instant>;
    let mut requested_recovery = None;

    loop {
        if *shutdown.borrow() {
            break;
        }
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            changed = visual_mode.changed() => {
                if changed.is_err() {
                    break;
                }
                let requested = *visual_mode.borrow();
                if requested == visual_config {
                    continue;
                }
                if let Some(scope) = screencast.take() {
                    let _ = scope.stop().await;
                }
                visual_config = requested;
                last_frame_sent = None;
                if visual_config.enabled {
                    screencast = start_tui_screencast(&session, events, visual_config).await;
                } else {
                    let _ = send_browser_event(events, BrowserEvent::VisualStatus {
                        message: "live view off; semantic observation and explicit screenshots remain available".into(),
                    }).await;
                }
            }
            frame = async {
                match screencast.as_mut() {
                    Some(scope) => scope.next_frame().await,
                    None => std::future::pending().await,
                }
            }, if visual_config.enabled && screencast.is_some() => {
                match frame {
                    Some(frame) => {
                        let now = Instant::now();
                        if last_frame_sent.is_none_or(|last| now.duration_since(last) >= visual_config.minimum_interval) {
                            last_frame_sent = Some(now);
                            if let Some(view) = remote_view.as_ref() {
                                view.publish(crate::development::RemoteFrame {
                                    browser_revision: visual_revision,
                                    mime_type: "image/png".into(),
                                    data: frame.data.clone(),
                                });
                            }
                            let _ = send_browser_event(events, BrowserEvent::VisualFrame {
                                data: frame.data,
                                metadata: frame.metadata,
                                browser_revision: visual_revision,
                            }).await;
                        }
                    }
                    None => {
                        screencast = None;
                        let _ = send_browser_event(events, BrowserEvent::VisualStatus {
                            message: "screencast ended; bounded screenshot fallback active".into(),
                        }).await;
                    }
                }
            }
            _ = fallback_tick.tick(), if (visual_config.enabled || remote_view.is_some()) && screencast.is_none() => {
                if let Ok(bytes) = session.screenshot_png().await {
                    let metadata = serde_json::json!({"source":"screenshot-fallback"});
                    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                    if let Some(view) = remote_view.as_ref() {
                        view.publish(crate::development::RemoteFrame {
                            browser_revision: visual_revision,
                            mime_type: "image/png".into(),
                            data: data.clone(),
                        });
                    }
                    if visual_config.enabled {
                        let _ = send_browser_event(events, BrowserEvent::VisualFrame {
                            data,
                            metadata,
                            browser_revision: visual_revision,
                        }).await;
                    }
                }
            }
            _ = remote_input_tick.tick(), if remote_view.is_some() => {
                let input = remote_view.as_mut().and_then(|view| view.try_recv_input().ok());
                if let Some(input) = input {
                    let result = apply_remote_input(&session, input).await;
                    let message = match result {
                        Ok(()) => "Remote View input applied to the active BrowserSession".into(),
                        Err(error) => format!("Remote View input rejected: {error}"),
                    };
                    let _ = send_browser_event(events, BrowserEvent::VisualStatus { message }).await;
                }
            }
            command = commands.recv() => match command {
                Some(BrowserCommand::Shutdown) | None => break,
                Some(BrowserCommand::Cancel { .. }) => {}
                Some(BrowserCommand::InspectTargets(port)) => {
                    let probe = probe_local_endpoint(port.unwrap_or(current_port)).await;
                    let _ = send_browser_event(
                        events,
                        BrowserEvent::TargetsDiscovered { probe },
                    )
                    .await;
                }
                Some(BrowserCommand::Recover(action)) => {
                    requested_recovery = Some(action);
                    break;
                }
                Some(BrowserCommand::RemoteView(action)) => {
                    match action {
                        RemoteViewCommand::Open if remote_view.is_none() => {
                            match crate::development::RemoteView::bind().await {
                                Ok(view) => {
                                    let message = format!(
                                        "Remote View ready (same BrowserSession)\nURL: {}\nForward: {}\nRevoke with: browser remote-view close",
                                        view.local_url(), view.ssh_forward_hint()
                                    );
                                    remote_view = Some(view);
                                    let _ = send_browser_event(events, BrowserEvent::RemoteViewStatus { message }).await;
                                }
                                Err(error) => {
                                    let _ = send_browser_event(events, BrowserEvent::RemoteViewStatus { message: format!("Remote View failed: {error}") }).await;
                                }
                            }
                        }
                        RemoteViewCommand::Open | RemoteViewCommand::Status => {
                            let message = remote_view.as_ref().map_or_else(
                                || "Remote View is closed. Use browser remote-view open.".into(),
                                |view| format!("Remote View active\nURL: {}\nForward: {}", view.local_url(), view.ssh_forward_hint()),
                            );
                            let _ = send_browser_event(events, BrowserEvent::RemoteViewStatus { message }).await;
                        }
                        RemoteViewCommand::Close => {
                            if let Some(view) = remote_view.take() { view.revoke().await; }
                            let _ = send_browser_event(events, BrowserEvent::RemoteViewStatus { message: "Remote View revoked; its token and connected clients are invalid.".into() }).await;
                        }
                    }
                }
                Some(BrowserCommand::Execute { id, operation }) => {
                    let label = operation.label().to_string();
                    if !send_browser_event(events, BrowserEvent::OperationStarted { id, label }).await {
                        break;
                    }
                    match await_active_operation(
                        execute_browser_operation(&session, operation),
                        id,
                        commands,
                        shutdown,
                        events,
                    ).await {
                        ActiveOperationState::Completed(Ok(result)) => {
                            if let Some(PageUpdate::Context(context)) = result.update.as_ref() {
                                visual_revision = context.accessibility.revision;
                            }
                            if !send_browser_event(events, BrowserEvent::OperationFinished { id, result }).await {
                                break;
                            }
                        }
                        ActiveOperationState::Completed(Err(error)) => {
                            let message = error.to_string();
                            drop(error);
                            if !send_browser_event(events, BrowserEvent::OperationFailed { id, message }).await {
                                break;
                            }
                        }
                        ActiveOperationState::Cancelled => {
                            if !send_browser_event(events, BrowserEvent::OperationCancelled { id }).await {
                                break;
                            }
                        }
                        ActiveOperationState::Shutdown => break,
                    }
                }
            },
        }
    }

    if let Some(scope) = screencast {
        let _ = scope.stop().await;
    }
    if let Some(view) = remote_view {
        view.revoke().await;
    }
    let close_error = session.close().await.err().map(|error| error.to_string());
    if let Some(message) = close_error {
        let _ = send_browser_event(events, BrowserEvent::WorkerFailed { message }).await;
    }
    if let Some(recovery) = requested_recovery {
        WorkerLoopExit::Recover(recovery)
    } else {
        let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
        WorkerLoopExit::Shutdown
    }
}

async fn start_tui_screencast(
    session: &BrowserSession,
    events: &mpsc::Sender<BrowserEvent>,
    config: ScreencastConfig,
) -> Option<ScreencastScope> {
    match session
        .start_screencast(VisualFormat::Png, 80, config.max_width, config.max_height)
        .await
    {
        Ok(scope) => {
            let _ = send_browser_event(
                events,
                BrowserEvent::VisualStatus {
                    message: format!(
                        "event-driven PNG screencast active (max {}x{}, requested {} FPS, scale {:.2}x)",
                        config.max_width,
                        config.max_height,
                        config.requested_fps,
                        config.capture_scale,
                    ),
                },
            )
            .await;
            Some(scope)
        }
        Err(error) => {
            let _ = send_browser_event(
                events,
                BrowserEvent::VisualStatus {
                    message: format!(
                        "screencast unavailable; bounded screenshot fallback: {error}"
                    ),
                },
            )
            .await;
            None
        }
    }
}

async fn apply_remote_input(
    session: &BrowserSession,
    input: crate::development::RemoteInput,
) -> BrowserResult<()> {
    match input {
        crate::development::RemoteInput::Click {
            x,
            y,
            expected_revision,
        } => {
            let viewport = session
                .evaluate("({width: innerWidth, height: innerHeight})")
                .await?;
            let width = viewport["width"]
                .as_f64()
                .ok_or("browser viewport width is unavailable")?;
            let height = viewport["height"]
                .as_f64()
                .ok_or("browser viewport height is unavailable")?;
            session
                .click_at_with_revision(x * width, y * height, Some(expected_revision))
                .await?;
        }
        crate::development::RemoteInput::Scroll {
            dx,
            dy,
            expected_revision,
        } => {
            session
                .scroll_with_revision(dx, dy, Some(expected_revision))
                .await?;
        }
        crate::development::RemoteInput::Key {
            key,
            expected_revision,
        } => {
            session
                .key_press_with_revision(&key, Some(expected_revision))
                .await?;
        }
        crate::development::RemoteInput::Text {
            text,
            expected_revision,
        } => {
            session
                .type_text_with_expected_revision(&text, None, Some(expected_revision))
                .await?;
        }
    }
    Ok(())
}

async fn await_active_operation<F>(
    operation: F,
    id: u64,
    commands: &mut mpsc::Receiver<BrowserCommand>,
    shutdown: &mut watch::Receiver<bool>,
    events: &mpsc::Sender<BrowserEvent>,
) -> ActiveOperationState
where
    F: Future<Output = BrowserResult<Box<OperationResult>>>,
{
    tokio::pin!(operation);
    loop {
        if *shutdown.borrow() {
            return ActiveOperationState::Shutdown;
        }
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return ActiveOperationState::Shutdown;
                }
            }
            command = commands.recv() => match command {
                Some(BrowserCommand::Shutdown) | None => return ActiveOperationState::Shutdown,
                Some(BrowserCommand::Cancel { id: cancel_id }) if cancel_id == id => {
                    return ActiveOperationState::Cancelled;
                }
                Some(BrowserCommand::Execute { id: queued_id, .. }) => {
                    if !send_browser_event(events, BrowserEvent::OperationFailed {
                        id: queued_id,
                        message: "browser worker is already executing an operation".to_string(),
                    }).await {
                        return ActiveOperationState::Shutdown;
                    }
                }
                Some(BrowserCommand::Cancel { .. }) => {}
                Some(BrowserCommand::InspectTargets(_)) => {
                    let _ = send_browser_event(events, BrowserEvent::VisualStatus {
                        message: "target refresh waits until the active browser operation completes".into(),
                    }).await;
                }
                Some(BrowserCommand::Recover(_)) => {
                    let _ = send_browser_event(events, BrowserEvent::VisualStatus {
                        message: "browser lifecycle changes wait until the active operation completes".into(),
                    }).await;
                }
                Some(BrowserCommand::RemoteView(_)) => {
                    let _ = send_browser_event(events, BrowserEvent::VisualStatus {
                        message: "Remote View changes wait until the active operation completes".into(),
                    }).await;
                }
            },
            result = &mut operation => return ActiveOperationState::Completed(result),
        }
    }
}

async fn execute_browser_operation(
    session: &BrowserSession,
    operation: BrowserOperation,
) -> BrowserResult<Box<OperationResult>> {
    match operation {
        BrowserOperation::Navigate(url) => {
            let page = session.navigate(&url).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Page loaded: {}", page.title),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Screenshot(output) => {
            let output = session
                .policy()
                .require_output_path(std::path::Path::new(&output))?;
            tokio::fs::write(&output, session.screenshot_png().await?).await?;
            Ok(Box::new(OperationResult {
                activity: format!("Screenshot saved to {}", output.display()),
                update: None,
            }))
        }
        BrowserOperation::Text => {
            let context = session.observe().await?;
            Ok(Box::new(OperationResult {
                activity: "Page text refreshed.".to_string(),
                update: Some(PageUpdate::Text {
                    page: context.page,
                    text: context.text,
                }),
            }))
        }
        BrowserOperation::Dom => {
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: "Compact DOM and accessibility context refreshed.".to_string(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Observe { fresh } => {
            let context = if fresh {
                session.observe_fresh().await?
            } else {
                session.observe().await?
            };
            Ok(Box::new(OperationResult {
                activity: "Compact observation refreshed.".to_string(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Semantic { level, region } => {
            let observation = session.semantic_observe(level).await?;
            let observation = if let Some(region_id) = region {
                session
                    .semantic_expand_region(&region_id, observation.revision, level)
                    .await?
            } else {
                observation
            };
            Ok(Box::new(OperationResult {
                activity: format!(
                    "Semantic {} observation refreshed (revision {}).",
                    serde_json::to_value(level)?.as_str().unwrap_or("unknown"),
                    observation.revision
                ),
                update: Some(PageUpdate::Semantic(Box::new(observation))),
            }))
        }
        BrowserOperation::Click(target) => {
            let outcome = session.click(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Clicked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::ClickAt {
            x,
            y,
            expected_revision,
        } => {
            let observation = session.observe_fresh().await?;
            if observation.accessibility.revision != expected_revision {
                return Err(format!(
                    "stale page revision: expected {expected_revision}, current {}",
                    observation.accessibility.revision
                )
                .into());
            }

            let _outcome = session.click_at(x, y).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Coordinate click at ({x:.0},{y:.0}) succeeded."),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::DoubleClick(target) => {
            let outcome = session.double_click(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Double-clicked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Hover(target) => {
            let outcome = session.hover(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Hovered", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Clear(target) => {
            let outcome = session.clear(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Cleared", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Check(target) => {
            let outcome = session.check(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Checked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Uncheck(target) => {
            let outcome = session.uncheck(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Unchecked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Select { target, value } => {
            let outcome = session.select_option(&target, &value).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Selected", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Type(text) => {
            let character_count = text.chars().count();
            let outcome = session.type_text(&text, None).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!(
                    "Typed {character_count} characters (revision {}).",
                    outcome.revision
                ),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::KeyPress(key) => {
            let outcome = session.key_press(&key).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Pressed", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Shortcut(shortcut) => {
            let outcome = session.shortcut(&shortcut).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Ran shortcut", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Scroll { dx, dy } => {
            let outcome = session.scroll(dx, dy).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Scrolled (revision {}).", outcome.revision),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::ScrollAt {
            dx,
            dy,
            expected_revision,
        } => {
            let current = session.observe_fresh().await?;
            if current.accessibility.revision != expected_revision {
                return Err(format!(
                    "stale page revision: expected {expected_revision}, current {}",
                    current.accessibility.revision
                )
                .into());
            }
            let outcome = session.scroll(dx, dy).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Scrolled (revision {}).", outcome.revision),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::AcceptDialog => {
            session.accept_dialog().await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: "Accepted the JavaScript dialog.".into(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::DismissDialog => {
            session.dismiss_dialog().await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: "Dismissed the JavaScript dialog.".into(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::DismissConsent => {
            let outcome = session.dismiss_consent().await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Consent dismissal: {outcome:?}."),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Evaluate(expression) => {
            let result = session.evaluate(&expression).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!(
                    "Result: {}",
                    bounded_text(&result.to_string(), TUI_ACTIVITY_MAX_BYTES)
                ),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Workflow(path) => {
            let payload = tokio::fs::read_to_string(&path).await?;
            let payload: serde_json::Value = serde_json::from_str(&payload)?;
            let workflow_value = payload
                .get("workflow")
                .cloned()
                .unwrap_or_else(|| payload.clone());
            let inputs_value = payload
                .get("inputs")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let workflow = WorkflowDefinition::from_value(workflow_value)?;
            let inputs: BTreeMap<String, serde_json::Value> = serde_json::from_value(inputs_value)?;
            let result = session.run_workflow(&workflow, &inputs).await?;
            let step_summary = result
                .steps
                .iter()
                .map(|step| format!("{}={:?}", step.id, step.state))
                .collect::<Vec<_>>()
                .join(", ");
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: bounded_text(
                    &format!(
                        "Workflow {} {:?}; trace={} [{}].",
                        result.name,
                        result.status,
                        result.trace.events.len(),
                        step_summary
                    ),
                    TUI_ACTIVITY_MAX_BYTES,
                ),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::ResolveIntent(path) => {
            let payload = tokio::fs::read_to_string(&path).await?;
            let request = SemanticIntentRequest::from_json(&payload)?;
            let result = session.resolve_intent(&request).await?;
            Ok(Box::new(OperationResult {
                activity: format_intent_activity(&result),
                update: Some(PageUpdate::IntentResolution {
                    request: Box::new(request),
                    result: Box::new(result),
                }),
            }))
        }
        BrowserOperation::ExecuteIntent(execution) => {
            let result = session.execute_intent(&execution).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format_intent_execution_activity(&result),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
    }
}

fn action_activity(verb: &str, outcome: &ActionOutcome) -> String {
    let target = outcome
        .target
        .as_ref()
        .map(|target| target.label.as_str())
        .unwrap_or("page");
    let mut effects = Vec::new();
    if outcome.verification.url_changed {
        effects.push("url");
    }
    if outcome.verification.title_changed {
        effects.push("title");
    }
    if outcome.verification.popup_opened {
        effects.push("popup");
    }
    if outcome.verification.dialog_open {
        effects.push("dialog");
    }
    if outcome.verification.download_started {
        effects.push("download");
    }
    let effect_text = if effects.is_empty() {
        String::new()
    } else {
        format!(" effects={}", effects.join(","))
    };
    format!(
        "{verb} {target} ({}; revision {}{}).",
        outcome.execution_id, outcome.revision, effect_text
    )
}

async fn send_browser_event(events: &mpsc::Sender<BrowserEvent>, event: BrowserEvent) -> bool {
    events.send(event).await.is_ok()
}

fn dispatch_ui_intent(
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    policy: &BrowserPolicy,
    intent: UiIntent,
) {
    match intent {
        UiIntent::Pointer(operation) => queue_browser_operation(app, commands, operation),
        UiIntent::Submit(command) => handle_submission(app, commands, policy, command),
        UiIntent::Cancel(id) => {
            if commands.try_send(BrowserCommand::Cancel { id }).is_err() {
                app.cancellation_enqueue_failed(id);
                app.report_error(
                    "Unable to queue cancellation because the browser command queue is full.",
                );
            }
        }
        UiIntent::Quit => app.should_quit = true,
        UiIntent::None => {}
    }
}

fn handle_browser_control(
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    command: BrowserControlCommand,
) {
    if let BrowserControlCommand::RemoteView(action) = &command {
        match commands.try_send(BrowserCommand::RemoteView(*action)) {
            Ok(()) => app.set_status("Remote View command queued"),
            Err(error) => {
                app.report_error(format!("Browser command queue is unavailable: {error}"))
            }
        }
        return;
    }
    if command == BrowserControlCommand::Status {
        if let Some(probe) = app.browser_recovery.as_ref() {
            app.set_page_content(format_recovery_probe(probe));
            app.set_mobile_view(MobileView::App);
        } else {
            app.set_page_content(format!(
                "Browser state: {:?}\n{}",
                app.browser_state,
                app.live_diagnostics()
            ));
        }
        return;
    }
    if let BrowserControlCommand::Targets(port) = &command {
        match commands.try_send(BrowserCommand::InspectTargets(*port)) {
            Ok(()) => app.set_status("Refreshing bounded browser target list"),
            Err(error) => {
                app.report_error(format!("Browser command queue is unavailable: {error}"))
            }
        }
        return;
    }
    let recovery = match command {
        BrowserControlCommand::Reconnect => BrowserRecovery::Reconnect,
        BrowserControlCommand::Launch(request) => BrowserRecovery::Launch(request),
        BrowserControlCommand::Disconnect => BrowserRecovery::Disconnect,
        BrowserControlCommand::Attach(request) => {
            let port = match (request.port, app.browser_recovery.as_ref()) {
                (Some(port), _) => port,
                (None, Some(probe)) => probe.port,
                (None, None) => {
                    app.report_error(
                        "Attach needs a discovered endpoint; use browser targets PORT or browser attach --port PORT.",
                    );
                    return;
                }
            };
            BrowserRecovery::Attach {
                port,
                target_id: request.target,
            }
        }
        BrowserControlCommand::Status => unreachable!(),
        BrowserControlCommand::Targets(_) => unreachable!(),
        BrowserControlCommand::RemoteView(_) => unreachable!(),
    };
    match commands.try_send(BrowserCommand::Recover(recovery)) {
        Ok(()) => app.set_status("Browser recovery command queued"),
        Err(error) => app.report_error(format!("Browser recovery queue is unavailable: {error}")),
    }
}

fn handle_submission(
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    policy: &BrowserPolicy,
    command: String,
) {
    app.add_activity(format!("> {command}"));
    if app.tap_mode
        && let Ok(number) = command.trim().parse::<usize>()
    {
        match app.tap_operation(number) {
            Ok(operation) => queue_browser_operation(app, commands, operation),
            Err(error) => app.report_error(error),
        }
        return;
    }
    if command.eq_ignore_ascii_case("intent execute")
        || strip_ascii_prefix(&command, "intent execute ").is_some()
    {
        let value = strip_ascii_prefix(&command, "intent execute ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let Some(request) = app.intent_request.clone() else {
            app.report_error("Resolve an intent before executing a selected candidate.");
            return;
        };
        let Some(result) = app.intent_result.as_ref() else {
            app.report_error("No intent resolution is available for execution.");
            return;
        };
        let Some(candidate) = result.candidates.get(app.intent_selection) else {
            app.report_error("No candidate is selected.");
            return;
        };
        let execution = SemanticIntentExecutionRequest {
            request: SemanticIntentRequest {
                expected_revision: result.revision,
                ..request
            },
            candidate_id: candidate.id.clone(),
            value,
        };
        if let Err(error) = execution.validate() {
            app.report_error(error.to_string());
            return;
        }
        queue_browser_operation(
            app,
            commands,
            BrowserOperation::ExecuteIntent(Box::new(execution)),
        );
        return;
    }
    match parse_command(&command) {
        Ok(ParsedCommand::Local(LocalCommand::Project(project_command))) => {
            app.handle_project_command(&project_command);
        }
        Ok(ParsedCommand::Local(LocalCommand::BrowserControl(browser_command))) => {
            handle_browser_control(app, commands, browser_command);
        }
        Ok(ParsedCommand::Local(LocalCommand::Help)) => {
            app.add_activity(
                "navigate URL | click TARGET | double click TARGET | hover TARGET | type TEXT | clear TARGET | check TARGET | uncheck TARGET | select TARGET VALUE",
            );
            app.add_activity(
                "inbox | tap [N] | verify card | capsule [save|show|clear] | live [on|auto|off|status|doctor|backend NAME|quality auto|data|balanced|smooth|fit NAME] | browser [status|reconnect|launch OPTIONS|targets [PORT]|attach [--port PORT] [TARGET]|semantic-only|remote-view ACTION] | project [inspect|files|open PATH|edit PATH CONTENT|run NAME COMMAND|processes|stop NAME|output NAME|diff|timeline|agent PROMPT] | safari | profiles | knowledge [show RECORD_ID] | daemon [status|doctor|logs|recovery] | JavaScript",
            );
        }
        Ok(ParsedCommand::Local(LocalCommand::Inbox)) => {
            app.set_page_content(app.attention_summary.clone());
            app.set_mobile_view(MobileView::Home);
        }
        Ok(ParsedCommand::Local(LocalCommand::Notifications(enabled))) => {
            if let Some(enabled) = enabled {
                app.attention_notifications = enabled;
                if !enabled {
                    app.notified_attention.clear();
                }
            }
            app.set_status(format!(
                "Attention terminal notifications: {}",
                if app.attention_notifications {
                    "on"
                } else {
                    "off"
                }
            ));
        }
        Ok(ParsedCommand::Local(LocalCommand::Tap(number))) => {
            if let Some(number) = number {
                match app.tap_operation(number) {
                    Ok(operation) => queue_browser_operation(app, commands, operation),
                    Err(error) => app.report_error(error),
                }
            } else {
                app.tap_mode = true;
                if app.display_class != DisplayClass::Phone {
                    app.set_page_content(app.tap_overlay());
                }
                app.set_mobile_view(MobileView::App);
                app.set_status("Semantic tap mode · choose 1-9 · Esc closes");
            }
        }
        Ok(ParsedCommand::Local(LocalCommand::VerificationCard)) => {
            app.set_mobile_view(MobileView::Diff);
        }
        Ok(ParsedCommand::Local(LocalCommand::Capsule(operation))) => {
            let Some(project) = app.development.as_ref() else {
                app.report_error("Project workspace is unavailable.");
                return;
            };
            let root = project.root().to_path_buf();
            let result = match operation {
                CapsuleCommand::Save => {
                    let mut capsule = match ReconnectCapsule::new(&root) {
                        Ok(capsule) => capsule,
                        Err(error) => {
                            app.report_error(error.to_string());
                            return;
                        }
                    };
                    capsule.event_cursor = project
                        .timeline()
                        .events()
                        .next_back()
                        .map(|event| event.id.clone());
                    capsule.mobile_view = Some(app.mobile_view.label().to_ascii_lowercase());
                    capsule.browser_target_id = app.browser_target_id.clone();
                    capsule.browser_revision = Some(app.visual_revision);
                    capsule.pending_attention =
                        attention_inbox(project.timeline().events().cloned())
                            .into_iter()
                            .find(|item| item.state == AttentionState::NeedsAttention)
                            .map(|item| item.title);
                    capsule.live_mode = Some(format!("{:?}", app.live.mode).to_ascii_lowercase());
                    capsule.live_quality = Some(if app.live.adaptive_quality {
                        "auto".into()
                    } else {
                        app.live.quality_label().into()
                    });
                    ReconnectCapsuleStore::save(&capsule).and_then(|path| {
                        serde_json::to_string_pretty(&json!({"capsule": capsule, "path": path}))
                            .map_err(Into::into)
                    })
                }
                CapsuleCommand::Show => ReconnectCapsuleStore::load(&root).and_then(|capsule| {
                    serde_json::to_string_pretty(&json!({"capsule": capsule})).map_err(Into::into)
                }),
                CapsuleCommand::Clear => ReconnectCapsuleStore::clear(&root).and_then(|cleared| {
                    serde_json::to_string_pretty(&json!({"cleared": cleared})).map_err(Into::into)
                }),
            };
            match result {
                Ok(content) => {
                    app.set_page_content(content);
                    if app.display_class == DisplayClass::Phone {
                        app.set_mobile_view(MobileView::Agent);
                    }
                    app.set_status("Reconnect capsule updated");
                }
                Err(error) => app.report_error(error.to_string()),
            }
        }
        Ok(ParsedCommand::Local(LocalCommand::Live(command))) => match command {
            LiveCommand::Show => {
                app.set_page_content(format!(
                    "TERMINAL LIVE BROWSER\n\n{}\n\nmode: {:?}\npreference: {:?}\nquality: {:?}\nfit: {:?}\nkitty detected: {}\nherdr stream: {}\ntransport: {}\n\nSafari forwarding remains the stable native-browser channel.",
                    app.live_diagnostics(),
                    app.live.mode,
                    app.live.preference,
                    app.live.quality,
                    app.live.fit,
                    app.live.kitty_detected,
                    if app.live.herdr_environment.is_some() { "available" } else { "unavailable" },
                    app.remote_context.label(),
                ));
                app.set_mobile_view(MobileView::App);
            }
            LiveCommand::Doctor => {
                let recommendation = if app.remote_context.mosh {
                    "Mosh synchronizes terminal cells, so use the ANSI backend for live frames. Use SSH for Kitty/Herdr pixels."
                } else if app.live.herdr_environment.is_some() {
                    "Herdr pane graphics is available and is the preferred owned image layer."
                } else if app.live.kitty_detected {
                    "Kitty graphics is available directly over this terminal transport."
                } else {
                    "No native pixel backend was detected; `live on` will use portable ANSI half blocks."
                };
                app.set_page_content(format!(
                    "LIVE VIEW DOCTOR\n\ntransport: {}\nSSH: {}\nMosh: {}\nHerdr environment: {}\nKitty capability: {}\nactive: {}\n\n{}\n\nCommands:\n  live on\n  live backend ansi\n  live quality data|balanced|smooth\n  live fit contain|cover|actual",
                    app.remote_context.label(),
                    app.remote_context.ssh,
                    app.remote_context.mosh,
                    app.live.herdr_environment.is_some(),
                    app.live.kitty_detected,
                    app.live_diagnostics(),
                    recommendation,
                ));
                app.set_mobile_view(MobileView::App);
            }
            LiveCommand::Mode(mode) => app.configure_live(Some(mode), None, None, None),
            LiveCommand::Backend(backend) => app.configure_live(None, Some(backend), None, None),
            LiveCommand::Quality(quality) => app.configure_live(None, None, Some(quality), None),
            LiveCommand::AdaptiveQuality => {
                app.live.enable_adaptive_quality();
                app.set_status(app.live_diagnostics());
            }
            LiveCommand::Fit(fit) => app.configure_live(None, None, None, Some(fit)),
        },
        Ok(ParsedCommand::Local(LocalCommand::Safari)) => {
            let configured = app
                .development
                .as_ref()
                .and_then(|project| project.detection().browser_url.as_deref());
            let candidate = (!app.url.trim().is_empty())
                .then_some(app.url.as_str())
                .or(configured);
            match candidate.and_then(|url| safari_handoff(url).ok()) {
                Some(guidance) => {
                    app.set_page_content(guidance);
                    app.set_mobile_view(MobileView::App);
                    app.add_activity("Prepared secure Safari tunnel guidance.");
                }
                None => app.report_error(
                    "No HTTP app URL is available. Configure browserUrl in glass.toml or navigate first.",
                ),
            }
        }
        Ok(ParsedCommand::Local(LocalCommand::Profiles)) => {
            if let Err(error) =
                policy.require(crate::browser::policy::PolicyCapability::PersistentProfile)
            {
                app.report_error(error.to_string());
                return;
            }
            match ProfileManager::new().list_profiles() {
                Ok(profiles) if profiles.is_empty() => app.add_activity("No saved profiles."),
                Ok(profiles) => {
                    for profile in profiles {
                        app.add_activity(format!("  - {profile}"));
                    }
                }
                Err(error) => app.report_error(error.to_string()),
            }
        }
        Ok(ParsedCommand::Local(LocalCommand::Knowledge(record_id))) => {
            if let Err(error) =
                policy.require(crate::browser::policy::PolicyCapability::PersistentProfile)
            {
                app.report_error(error.to_string());
                return;
            }
            match KnowledgeStore::open(&app.knowledge_path) {
                Ok(store) => {
                    let content = match record_id {
                        Some(record_id) => store
                            .get(&record_id)
                            .map(|record| {
                                serde_json::to_string_pretty(record)
                                    .map_err(|error| error.to_string())
                            })
                            .unwrap_or_else(|| {
                                Err(format!("knowledge record not found: {record_id}"))
                            }),
                        None => match store.stats() {
                            Ok(stats) => serde_json::to_string_pretty(&serde_json::json!({
                                "path": store.path().display().to_string(),
                                "stats": stats,
                                "records": store.records().iter().map(|record| serde_json::json!({
                                    "recordId": &record.record_id,
                                    "kind": record.kind,
                                    "confidence": record.confidence,
                                    "origin": &record.scope.origin,
                                    "pathPattern": &record.scope.path_pattern,
                                })).collect::<Vec<_>>(),
                            }))
                            .map_err(|error| error.to_string()),
                            Err(error) => Err(error.to_string()),
                        },
                    };
                    match content {
                        Ok(content) => {
                            app.title = "Glass — Knowledge inspector".into();
                            app.set_page_content(content);
                            app.set_status("Knowledge inspector");
                            app.add_activity("Knowledge store inspected without browser startup.");
                        }
                        Err(error) => app.report_error(error.to_string()),
                    }
                }
                Err(error) => app.report_error(error.to_string()),
            }
        }
        Ok(ParsedCommand::Local(LocalCommand::Daemon(view))) => {
            let (socket, status) = crate::daemon::default_paths();
            let result: BrowserResult<serde_json::Value> = match view {
                DaemonView::Status => crate::daemon::status(Some(&socket), Some(&status))
                    .and_then(|value| serde_json::to_value(value).map_err(Into::into)),
                DaemonView::Doctor => crate::daemon::doctor(Some(&socket), Some(&status)),
                DaemonView::Logs => crate::daemon::logs(Some(&status)),
                DaemonView::Recovery => crate::daemon::recovery(Some(&status)),
            };
            match result {
                Ok(value) => {
                    app.title = "Glass — Daemon inspector".into();
                    match serde_json::to_string_pretty(&value) {
                        Ok(content) => app.set_page_content(content),
                        Err(error) => app.report_error(error.to_string()),
                    }
                    app.set_status("Daemon inspector");
                    app.add_activity(
                        "Daemon state inspected without starting a browser operation.",
                    );
                }
                Err(error) => app.report_error(error.to_string()),
            }
        }
        Ok(ParsedCommand::Browser(operation)) => queue_browser_operation(app, commands, operation),
        Err(error) => app.report_error(error),
    }
}

fn queue_browser_operation(
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    operation: BrowserOperation,
) {
    if !app.browser_ready() {
        app.report_error("Browser is not ready yet.");
        return;
    }
    if app.is_busy() {
        app.report_error("A browser operation is already running; press Esc to cancel it.");
        return;
    }
    let operation = match operation {
        BrowserOperation::Scroll { dx, dy } => BrowserOperation::ScrollAt {
            dx,
            dy,
            expected_revision: app.graphics.browser_revision(),
        },
        operation => operation,
    };

    let lease_revision = if operation.requires_human_lease() {
        let expected_revision = app.mutation_lease.revision();
        match app
            .mutation_lease
            .acquire(MutationActor::Human, expected_revision)
        {
            Ok(revision) => Some(revision),
            Err(error) => {
                app.report_error(format!("Mutation lease denied: {error}."));
                return;
            }
        }
    } else {
        None
    };

    let id = app.allocate_operation_id();
    let label = operation.label().to_string();
    match commands.try_send(BrowserCommand::Execute { id, operation }) {
        Ok(()) => {
            app.begin_operation(id, label);
            if let Some(revision) = lease_revision {
                app.attach_mutation_lease(id, revision);
            }
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            if let Some(revision) = lease_revision {
                let _ = app.mutation_lease.release(MutationActor::Human, revision);
            }
            app.report_error("Browser command queue is full.");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            if let Some(revision) = lease_revision {
                let _ = app.mutation_lease.release(MutationActor::Human, revision);
            }
            app.browser_state = BrowserState::Unavailable;
            app.report_error("Browser worker is unavailable.");
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RootRegions {
    header: Rect,
    content: Rect,
    command: Rect,
    nav: Option<Rect>,
    footer: Rect,
}

fn root_regions(area: Rect, class: DisplayClass) -> RootRegions {
    if class == DisplayClass::Phone {
        let regions = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(area);
        RootRegions {
            header: regions[0],
            content: regions[1],
            command: regions[2],
            nav: Some(regions[3]),
            footer: regions[4],
        }
    } else {
        let regions = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);
        RootRegions {
            header: regions[0],
            content: regions[1],
            command: regions[2],
            nav: None,
            footer: regions[3],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WideDevelopmentRegions {
    files: Rect,
    editor: Rect,
    app: Rect,
    runtime: Rect,
    timeline: Rect,
}

fn wide_development_regions(area: Rect) -> WideDevelopmentRegions {
    let development = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(76), Constraint::Percentage(24)])
        .split(area);
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(21),
            Constraint::Percentage(54),
            Constraint::Percentage(25),
        ])
        .split(development[0]);
    let work = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(main[1]);
    WideDevelopmentRegions {
        files: main[0],
        editor: work[0],
        app: work[1],
        runtime: main[2],
        timeline: development[1],
    }
}

fn live_panel_region(
    area: Rect,
    display: DisplayClass,
    mode: WorkspaceMode,
    mobile_view: MobileView,
    live_enabled: bool,
) -> Option<Rect> {
    if !live_enabled {
        return None;
    }
    let root = root_regions(area, display);
    match display {
        DisplayClass::Phone => (mobile_view == MobileView::App).then_some(root.content),
        DisplayClass::Wide if mode == WorkspaceMode::Development => {
            Some(wide_development_regions(root.content).app)
        }
        DisplayClass::Wide => {
            let content = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                .split(root.content);
            Some(content[1])
        }
        DisplayClass::Compact if mode == WorkspaceMode::Development => {
            let vertical = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(root.content);
            let main = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(vertical[0]);
            let work = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
                .split(main[1]);
            Some(work[1])
        }
        DisplayClass::Compact => {
            let content = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                .split(root.content);
            Some(content[1])
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let root = root_regions(frame.area(), app.display_class);
    match app.display_class {
        DisplayClass::Phone => draw_phone(frame, app, root),
        DisplayClass::Compact => draw_desktop(frame, app, root, true),
        DisplayClass::Wide => draw_desktop(frame, app, root, false),
    }
    draw_overlays(frame, app);
}

fn draw_phone(frame: &mut Frame, app: &App, root: RootRegions) {
    let header = format!(
        "{} · {:?} · {:?} · {}",
        app.mode.label(),
        app.connection_environment.transport,
        app.connection_environment.graphics,
        if app.browser_ready() {
            "browser attached"
        } else {
            "browser needs attention"
        }
    );
    frame.render_widget(
        Paragraph::new(header)
            .block(Block::default().borders(Borders::ALL).title("Glass"))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        root.header,
    );

    let (title, content) = match app.mobile_view {
        MobileView::Home => ("Overview", mobile_home(app)),
        MobileView::Agent => (
            "Agent / Timeline",
            format!(
                "{}\n\nLATEST RESULT\n{}",
                app.activity.iter().cloned().collect::<Vec<_>>().join("\n"),
                app.page_content
            ),
        ),
        MobileView::App => (
            "Browser · tap · visual assist · Remote View",
            if app.tap_mode {
                format!("{}\n\n{}", app.tap_overlay(), app.page_content)
            } else {
                app.page_content.clone()
            },
        ),
        MobileView::Diff => (
            "Diff / Verification",
            format!(
                "VERIFICATION CARD\n{}\n\nPROJECT DIFF\n{}",
                app.verification_summary, app.development_diff
            ),
        ),
        MobileView::Project => ("Project / Editor / Diagnostics", mobile_project(app)),
        MobileView::Process => (
            "Process / Tests / Logs",
            format!(
                "{}

RECENT ACTIVITY
{}",
                app.development_runtime,
                app.activity
                    .iter()
                    .rev()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        ),
    };
    if app.mobile_view == MobileView::App && app.live.enabled() {
        render_live_or_semantic(frame, app, title, &content, root.content);
    } else {
        frame.render_widget(
            Paragraph::new(content)
                .block(Block::default().borders(Borders::ALL).title(title))
                .scroll((app.page_scroll, 0))
                .wrap(Wrap { trim: true }),
            root.content,
        );
    }
    frame.render_widget(
        Paragraph::new(app.input.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Command · agent PROMPT"),
        ),
        root.command,
    );

    if let Some(nav) = root.nav {
        frame.render_widget(
            Paragraph::new(vec![
                mobile_nav_line(app, [MobileView::Home, MobileView::Agent, MobileView::App]),
                mobile_nav_line(
                    app,
                    [MobileView::Diff, MobileView::Project, MobileView::Process],
                ),
            ]),
            nav,
        );
    }
    frame.render_widget(
        Paragraph::new(format!(" {} | Tab: views | ?: help | q: quit", app.status))
            .style(Style::default().fg(Color::DarkGray)),
        root.footer,
    );
}

fn mobile_nav_line(app: &App, views: [MobileView; 3]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, view) in views.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let style = if view == app.mobile_view {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(
            format!("{} {}", view.number(), view.label()),
            style,
        ));
    }
    Line::from(spans)
}

fn mobile_home(app: &App) -> String {
    format!(
        "{}\n\nAGENT\n{}\n\nLIVE APP\n{}\nrevision {} · {}\n{}\n\nUNDERSTANDING\n{}\n\nTESTS / PROCESS\n{}\n\nCONNECTION\n{:?} · {:?} · {:?}\nRTT {} · throughput {}",
        app.attention_summary,
        app.status,
        if app.url.is_empty() {
            "No page loaded"
        } else {
            app.url.as_str()
        },
        app.visual_revision,
        if app.browser_ready() {
            "fresh"
        } else {
            "unavailable"
        },
        app.visual_status,
        app.page_content
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join("\n"),
        app.development_runtime,
        app.connection_environment.transport,
        app.connection_environment.graphics,
        app.connection_environment.multiplexer,
        app.connection_environment
            .measurements
            .rtt_ms
            .map(|value| format!("{value:.0}ms"))
            .unwrap_or_else(|| "unknown".into()),
        app.connection_environment
            .measurements
            .estimated_throughput_mbps
            .map(|value| format!("{value:.1}Mbps"))
            .unwrap_or_else(|| "unknown".into()),
    )
}

fn format_attention_inbox(items: Vec<crate::development::AttentionItem>) -> String {
    if items.is_empty() {
        return "NEEDS YOU (0)\nNo items need attention.".into();
    }
    let mut output = Vec::new();
    for (state, heading, marker) in [
        (AttentionState::NeedsAttention, "NEEDS YOU", "!"),
        (AttentionState::Running, "RUNNING", "●"),
        (AttentionState::Recent, "RECENT", "✓"),
    ] {
        let matching = items
            .iter()
            .filter(|item| item.state == state)
            .take(8)
            .collect::<Vec<_>>();
        output.push(format!("{heading} ({})", matching.len()));
        if matching.is_empty() {
            output.push("  none".into());
        } else {
            output.extend(
                matching
                    .into_iter()
                    .map(|item| format!("{marker} {} · {}", item.title, item.detail)),
            );
        }
    }
    output.join("\n")
}

fn mobile_project(app: &App) -> String {
    format!(
        "FILES / GIT\n{}\n\nEDITOR\n{}\n\nRUNTIME / TESTS / ACTORS\n{}",
        app.development_files, app.development_editor, app.development_runtime
    )
}

fn draw_desktop(frame: &mut Frame, app: &App, root: RootRegions, compact: bool) {
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(root.header);
    frame.render_widget(
        Paragraph::new(format!("{} — {}", app.title, app.mode.label()))
            .block(Block::default().borders(Borders::ALL).title("Glass"))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        header[0],
    );
    frame.render_widget(
        Paragraph::new(app.url.as_str())
            .block(Block::default().borders(Borders::ALL).title("URL"))
            .style(Style::default().fg(Color::Yellow)),
        header[1],
    );

    if app.mode == WorkspaceMode::Development {
        if compact {
            draw_compact_development(frame, app, root.content);
        } else {
            draw_wide_development(frame, app, root.content);
        }
    } else {
        let content = Layout::default()
            .direction(if compact {
                Direction::Vertical
            } else {
                Direction::Horizontal
            })
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(root.content);
        frame.render_widget(
            List::new(activity_items(app))
                .block(Block::default().borders(Borders::ALL).title("Activity"))
                .style(Style::default().fg(Color::Green)),
            content[0],
        );
        render_live_or_semantic(
            frame,
            app,
            workspace_content_title(app.mode),
            &app.page_content,
            content[1],
        );
    }

    frame.render_widget(
        Paragraph::new(app.input.as_str())
            .block(Block::default().borders(Borders::ALL).title("Command")),
        root.command,
    );
    let hint = if compact {
        format!(
            " {} | F1-F7 modes | Enter execute | Esc cancel/back",
            app.status
        )
    } else {
        format!(
            " {} | mode:{} lease:{:?}@{} | {} | {} | {}   PgUp/PgDn: observation   F1-F7: workspace modes   q/Ctrl-C: quit   Enter: execute   Esc: cancel/back   {}",
            app.status,
            app.mode.label(),
            app.mutation_lease.state(),
            app.mutation_lease.revision(),
            app.visual_status,
            app.capability_summary,
            app.graphics.diagnostics_label(),
            app.input.chars().count()
        )
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        root.footer,
    );
}

fn draw_wide_development(frame: &mut Frame, app: &App, area: Rect) {
    let regions = wide_development_regions(area);
    render_development_panel(
        frame,
        "Project / Git",
        &app.development_files,
        regions.files,
    );
    render_development_panel(
        frame,
        "Glass Editor",
        &app.development_editor,
        regions.editor,
    );
    render_live_or_semantic(
        frame,
        app,
        "Live App / Semantics",
        &app.page_content,
        regions.app,
    );
    render_development_panel(
        frame,
        "Runtime / Tests / Actors",
        &app.development_runtime,
        regions.runtime,
    );
    frame.render_widget(
        List::new(activity_items(app))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Glass Agent / Timeline"),
            )
            .style(Style::default().fg(Color::Green)),
        regions.timeline,
    );
}

fn draw_compact_development(frame: &mut Frame, app: &App, area: Rect) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(vertical[0]);
    let work = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(main[1]);
    let lower = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(vertical[1]);
    render_development_panel(frame, "Project / Git", &app.development_files, main[0]);
    render_development_panel(frame, "Editor", &app.development_editor, work[0]);
    render_live_or_semantic(frame, app, "App / Semantics", &app.page_content, work[1]);
    render_development_panel(frame, "Runtime", &app.development_runtime, lower[0]);
    frame.render_widget(
        List::new(activity_items(app))
            .block(Block::default().borders(Borders::ALL).title("Agent"))
            .style(Style::default().fg(Color::Green)),
        lower[1],
    );
}

fn render_development_panel(frame: &mut Frame, title: &'static str, content: &str, area: Rect) {
    frame.render_widget(
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_live_or_semantic(
    frame: &mut Frame,
    app: &App,
    title: &'static str,
    semantic: &str,
    area: Rect,
) {
    if !app.live.enabled() || app.live.ansi.cells().is_empty() {
        let content = if app.live.enabled() {
            format!(
                "{}\n\nWaiting for live browser frame…",
                app.live_diagnostics()
            )
        } else {
            semantic.to_string()
        };
        frame.render_widget(
            Paragraph::new(content)
                .block(Block::default().borders(Borders::ALL).title(title))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    if app.live.backend != Some(ActiveLiveBackend::Ansi) {
        frame.render_widget(
            Paragraph::new("").block(Block::default().borders(Borders::ALL).title(title)),
            area,
        );
        return;
    }
    let mut lines = Vec::with_capacity(usize::from(app.live.ansi.height()));
    for row in app
        .live
        .ansi
        .cells()
        .chunks(usize::from(app.live.ansi.width().max(1)))
    {
        let mut spans = Vec::new();
        let mut start = 0;
        while start < row.len() {
            let cell = row[start];
            let mut end = start + 1;
            while end < row.len() && row[end] == cell {
                end += 1;
            }
            spans.push(Span::styled(
                "▀".repeat(end - start),
                Style::default()
                    .fg(Color::Rgb(cell.top.red, cell.top.green, cell.top.blue))
                    .bg(Color::Rgb(
                        cell.bottom.red,
                        cell.bottom.green,
                        cell.bottom.blue,
                    )),
            ));
            start = end;
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn activity_items(app: &App) -> Vec<ListItem<'_>> {
    app.activity
        .iter()
        .map(|entry| ListItem::new(Line::from(entry.as_str())))
        .collect()
}

const fn workspace_content_title(mode: WorkspaceMode) -> &'static str {
    match mode {
        WorkspaceMode::Browser => "Browser / Structured Observation",
        WorkspaceMode::Split => "Split / Browser + Status",
        WorkspaceMode::Workflow => "Workflow Verification",
        WorkspaceMode::Semantic => "Semantic Inspector",
        WorkspaceMode::Inspect => "Inspect / Memory + Backend",
        WorkspaceMode::Takeover => "Takeover / Reconciliation",
        WorkspaceMode::Development => "Development / Live Runtime",
    }
}

fn draw_overlays(frame: &mut Frame, app: &App) {
    if app.mobile_help {
        let popup = centered_popup_sized(frame.area(), 38, 15);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(
                "1-5  switch views\nTab  next view\nEsc  home / cancel\n?    close help\n\nlive on / live off\nlive doctor\nagent PROMPT\nproject open PATH\nproject diff\nsafari  native tunnel\nscreenshot PATH  explicit evidence",
            )
            .block(Block::default().borders(Borders::ALL).title("Phone controls"))
            .wrap(Wrap { trim: true }),
            popup,
        );
    }
    if let Some(error) = &app.error_msg {
        let popup = centered_popup(frame.area());
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(error.as_str())
                .block(Block::default().borders(Borders::ALL).title("Error"))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: true }),
            popup,
        );
    }
}

fn centered_popup(area: Rect) -> Rect {
    let width = (area.width.saturating_mul(2) / 3).max(1).min(area.width);
    let height = 5.min(area.height);
    Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn centered_popup_sized(area: Rect, preferred_width: u16, preferred_height: u16) -> Rect {
    let width = preferred_width.min(area.width).max(1);
    let height = preferred_height.min(area.height).max(1);
    Rect {
        x: area.x.saturating_add(area.width.saturating_sub(width) / 2),
        y: area
            .y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    }
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { active: true })
    }

    fn restore(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let raw_result = disable_raw_mode();
        let mut stdout = io::stdout();
        let screen_result = execute!(stdout, LeaveAlternateScreen, Show);
        match (raw_result, screen_result) {
            (Err(error), _) | (_, Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Probe Kitty graphics before the input worker starts, so terminal replies
/// cannot be mistaken for user key events. Environment hints remain the fast
/// path; this bounded query covers generic `xterm-256color` SSH clients.
#[cfg(unix)]
fn probe_kitty_graphics() -> bool {
    let mut stdout = io::stdout();
    if stdout
        .write_all(b"\x1b_Gi=31,a=q,s=1,v=1,f=24;AAAA\x1b\\\x1b[c")
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return false;
    }
    let deadline = Instant::now() + Duration::from_millis(180);
    let mut response = Vec::with_capacity(256);
    while Instant::now() < deadline && response.len() < 4096 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX);
        let mut descriptor = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `descriptor` points to one initialized pollfd for the call.
        let ready = unsafe { libc::poll(&mut descriptor, 1, timeout) };
        if ready <= 0 {
            break;
        }
        let mut buffer = [0_u8; 256];
        // SAFETY: `buffer` is valid for its full length and stdin is open while
        // the TUI terminal guard is active.
        let read =
            unsafe { libc::read(libc::STDIN_FILENO, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read <= 0 {
            break;
        }
        response.extend_from_slice(&buffer[..read as usize]);
        if response.windows(8).any(|window| window == b"i=31;OK") {
            return true;
        }
        if response.ends_with(b"c") && response.contains(&0x1b) {
            break;
        }
    }
    false
}

#[cfg(not(unix))]
fn probe_kitty_graphics() -> bool {
    false
}

pub async fn run_tui(cli: &Cli) -> BrowserResult<()> {
    run_tui_for_product(cli, true).await
}

pub async fn run_tui_for_product(cli: &Cli, development_enabled: bool) -> BrowserResult<()> {
    let mut terminal_guard = TerminalGuard::enter()?;
    let remote_context = RemoteContext::from_process();
    let should_probe_kitty = cli.tui_live != TuiLiveMode::Off
        && cli.tui_graphics == TuiGraphics::Auto
        && matches!(
            cli.tui_live_backend,
            TuiLiveBackend::Auto | TuiLiveBackend::Kitty
        )
        && !remote_context.herdr
        && !remote_context.mosh;
    let kitty_detected =
        cli.tui_graphics == TuiGraphics::Kitty || (should_probe_kitty && probe_kitty_graphics());
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let initial_size = terminal.size()?;
    let connection_environment = ConnectionEnvironment::detect(
        initial_size.width.max(1),
        initial_size.height.max(1),
        &ConnectionSignals {
            ssh: remote_context.ssh,
            mosh: remote_context.mosh,
            tmux: remote_context.tmux,
            screen: remote_context.screen,
            herdr: remote_context.herdr,
        },
        if remote_context.herdr {
            Some(GraphicsClass::Herdr)
        } else if kitty_detected {
            Some(GraphicsClass::Kitty)
        } else {
            None
        },
        ConnectionMeasurements {
            rtt_ms: cli.tui_rtt_ms,
            estimated_throughput_mbps: cli.tui_throughput_mbps,
            ..ConnectionMeasurements::default()
        },
        ConnectionOverrides {
            layout: match cli.tui_layout {
                TuiLayout::Auto => None,
                TuiLayout::Mobile => Some(LayoutClass::Phone),
                TuiLayout::Compact => Some(LayoutClass::Compact),
                TuiLayout::Desktop => Some(LayoutClass::Wide),
            },
            transport: match cli.tui_transport {
                TuiTransport::Auto => None,
                TuiTransport::Local => Some(TransportClass::Local),
                TuiTransport::RemoteFast => Some(TransportClass::RemoteFast),
                TuiTransport::RemoteConstrained => Some(TransportClass::RemoteConstrained),
                TuiTransport::Mosh => Some(TransportClass::Mosh),
                TuiTransport::UnknownRemote => Some(TransportClass::UnknownRemote),
            },
            graphics: match cli.tui_graphics {
                TuiGraphics::Auto => None,
                TuiGraphics::Kitty => Some(GraphicsClass::Kitty),
                TuiGraphics::Sixel => Some(GraphicsClass::Sixel),
                TuiGraphics::ITermInline => Some(GraphicsClass::ITermInline),
                TuiGraphics::Ansi => Some(GraphicsClass::Ansi),
                TuiGraphics::SemanticOnly => Some(GraphicsClass::SemanticOnly),
            },
        },
    )?;
    let mut app = App::new_for_product_with_context(
        development_enabled,
        cli.tui_layout,
        remote_context,
        initial_size.width,
        LiveViewOptions {
            mode: cli.tui_live,
            backend: cli.tui_live_backend,
            quality: cli.tui_live_quality,
            fit: cli.tui_live_fit,
            kitty_detected,
        },
    );
    app.connection_environment = connection_environment;
    app.display_class = match app.connection_environment.layout {
        LayoutClass::Phone => DisplayClass::Phone,
        LayoutClass::Compact => DisplayClass::Compact,
        LayoutClass::Wide => DisplayClass::Wide,
    };
    app.restore_reconnect_capsule();

    let (input_tx, mut input_events) = mpsc::channel(INPUT_CHANNEL_CAPACITY);
    let (browser_commands, browser_command_rx) = mpsc::channel(BROWSER_COMMAND_CHANNEL_CAPACITY);
    let (browser_event_tx, mut browser_events) = mpsc::channel(BROWSER_EVENT_CHANNEL_CAPACITY);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (visual_mode_tx, visual_mode_rx) = watch::channel(ScreencastConfig::default());

    let viewport = cli
        .viewport
        .as_deref()
        .map(|value| -> Result<(i64, i64), Box<dyn std::error::Error>> {
            let (width, height) = value
                .split_once('x')
                .ok_or("viewport must use WIDTHxHEIGHT")?;
            Ok((width.parse::<i64>()?, height.parse::<i64>()?))
        })
        .transpose()?;
    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        attach: cli.attach,
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: cli.interaction,
        audit: cli.audit,
        policy: None,
    };
    let policy = crate::cli::runner::policy_from_cli(cli)?;
    let local = LocalSet::new();
    let browser_worker = local.spawn_local(browser_worker(
        viewport,
        options,
        policy.clone(),
        visual_mode_rx,
        browser_command_rx,
        browser_event_tx,
        shutdown_rx,
    ));
    let mut input_worker = InputWorker::spawn(input_tx);
    let manifest = GlassCapabilityManifest::for_policy_with_experimental_extensions(
        &policy,
        cli.experimental_extensions,
    );
    app.capability_summary = format!(
        "Capabilities: {} schemas, daemon {}",
        manifest.schemas.len(),
        if manifest.capabilities.get("localDaemon") == Some(&true) {
            "on"
        } else {
            "off"
        }
    );
    app.knowledge_path = cli
        .knowledge_store
        .clone()
        .unwrap_or_else(|| default_knowledge_store_path(&cli.profile));

    let loop_result = local
        .run_until(run_tui_loop(
            &mut terminal,
            &mut app,
            &browser_commands,
            &mut input_events,
            &mut browser_events,
            &policy,
            &visual_mode_tx,
        ))
        .await;

    drop(input_events);
    drop(browser_events);
    if app.development.is_some()
        && let Err(error) = app.save_reconnect_capsule()
    {
        tracing::warn!(%error, "failed to save TUI reconnect capsule");
    }
    app.release_all_mutation_leases();
    let _ = shutdown_tx.send(true);
    let _ = browser_commands.try_send(BrowserCommand::Shutdown);
    drop(browser_commands);
    app.live.stop_herdr();
    let graphics_result = app.graphics_shutdown();
    let input_result = input_worker.stop();
    let cursor_result = terminal.show_cursor();
    let terminal_result = terminal_guard.restore();
    let worker_result = local.run_until(finish_browser_worker(browser_worker)).await;

    loop_result?;
    graphics_result?;
    input_result?;
    cursor_result?;
    terminal_result?;
    worker_result
}

async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    input_events: &mut mpsc::Receiver<InputEvent>,
    browser_events: &mut mpsc::Receiver<BrowserEvent>,
    policy: &BrowserPolicy,
    visual_mode: &watch::Sender<ScreencastConfig>,
) -> BrowserResult<()> {
    let mut redraw = true;
    let mut browser_events_open = true;
    let mut busy_tick = time::interval(BUSY_TICK);
    busy_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    while !app.should_quit {
        if redraw {
            let area = terminal.size()?;
            app.sync_graphics_geometry(area.into())?;
            let next = app.live_capture_config();
            let _ = visual_mode.send_if_modified(|config| {
                if *config == next {
                    false
                } else {
                    *config = next;
                    true
                }
            });
            app.poll_live_worker();
            terminal.draw(|frame| draw(frame, app))?;
            app.render_graphics()?;
        }

        redraw = tokio::select! {
            biased;
            input = input_events.recv() => match input {
                Some(InputEvent::Key(key)) => {
                    let intent = app.reduce_key(key);
                    dispatch_ui_intent(app, commands, policy, intent);
                    true
                }
                Some(InputEvent::Mouse(mouse)) => {
                    let intent = app.mouse_intent(mouse);
                    dispatch_ui_intent(app, commands, policy, intent);
                    true
                }
                Some(InputEvent::Paste(text)) => {
                    if app.mode == WorkspaceMode::Development && app.editor_focus {
                        app.insert_editor_text(&text);
                    } else {
                        for character in text.chars() {
                            app.insert_char(character);
                        }
                    }
                    true
                }
                Some(InputEvent::Redraw) => true,
                Some(InputEvent::Error(error)) => return Err(error.into()),
                None => return Err("TUI input worker stopped".into()),
            },
            event = browser_events.recv(), if browser_events_open => match event {
                Some(event) => {
                    if let Some(operation) = app.apply_browser_event(event)? {
                        queue_browser_operation(app, commands, operation);
                    }
                    true
                }
                None => {
                    browser_events_open = false;
                    app.release_all_mutation_leases();
                    app.busy = None;
                    if !matches!(app.browser_state, BrowserState::Unavailable | BrowserState::Stopped) {
                        app.browser_state = BrowserState::Unavailable;
                        app.set_status("Browser worker unavailable");
                        app.report_error("Browser worker stopped unexpectedly.");
                    }
                    true
                }
            },
            _ = busy_tick.tick() => {
                if app.is_busy() {
                    app.tick_busy();
                }
                let live_changed = app.poll_live_worker();
                app.poll_development_events() || app.is_busy() || live_changed
            },
        };
    }
    Ok(())
}

async fn finish_browser_worker(mut worker: JoinHandle<()>) -> BrowserResult<()> {
    match time::timeout(WORKER_SHUTDOWN_TIMEOUT, &mut worker).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(format!("browser worker failed: {error}").into()),
        Err(_) => {
            worker.abort();
            let _ = worker.await;
            Err("timed out waiting for browser worker shutdown".into())
        }
    }
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }

    const MARKER: &str = "\n[truncated]";
    if max_bytes <= MARKER.len() {
        let mut end = max_bytes;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        return value[..end].to_string();
    }

    let mut end = max_bytes - MARKER.len();
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = value[..end].to_string();
    truncated.push_str(MARKER);
    truncated
}

fn char_byte_index(value: &str, character_index: usize) -> usize {
    value
        .char_indices()
        .nth(character_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn numbered_buffer(path: &str, content: &str) -> String {
    let mut output = format!("{path}\n\n");
    for (index, line) in content.lines().enumerate().take(512) {
        output.push_str(&format!("{:>4} │ {line}\n", index + 1));
    }
    if content.lines().count() > 512 {
        output.push_str("… [buffer view truncated]\n");
    }
    output
}

fn numbered_buffer_with_cursor(path: &str, content: &str, cursor: usize) -> String {
    let mut output = format!("{path}  [Ctrl-S save • Ctrl-Z undo • Esc commands]\n\n");
    let mut offset = 0_usize;
    for (index, line) in content.lines().enumerate().take(512) {
        let length = line.chars().count();
        let marker = if (offset..=offset.saturating_add(length)).contains(&cursor) {
            "▶"
        } else {
            " "
        };
        output.push_str(&format!("{marker}{:>4} │ {line}\n", index + 1));
        offset = offset.saturating_add(length).saturating_add(1);
    }
    if content.is_empty() {
        output.push_str("▶   1 │ \n");
    }
    if content.lines().count() > 512 {
        output.push_str("… [buffer view truncated]\n");
    }
    output
}

fn format_runtime_panel(project: &mut ProjectWorkspace) -> String {
    let detection = project.detection();
    let mut lines = vec![
        format!(
            "branch: {}",
            detection.git_branch.as_deref().unwrap_or("detached")
        ),
        format!("languages: {}", detection.languages.join(", ")),
        format!(
            "browser: {}",
            detection.browser_url.as_deref().unwrap_or("not configured")
        ),
        String::new(),
        "ACTORS".into(),
    ];
    lines.extend(project.actors().map(|actor| {
        let marker = match actor.kind {
            crate::development::ActorKind::Human | crate::development::ActorKind::EmbeddedAgent => {
                "◆"
            }
            _ => "◇",
        };
        format!("{marker} {} [{:?}]", actor.name, actor.authority)
    }));
    lines.extend([String::new(), "PROCESSES".into()]);
    match project.processes().list() {
        processes if processes.is_empty() => lines.push("○ none".into()),
        processes => lines.extend(processes.into_iter().map(|process| {
            let state = match process.state {
                ProcessState::Running => "→ running",
                ProcessState::Exited { .. } => "✓ exited",
                ProcessState::Stopped => "× stopped",
                ProcessState::Failed => "! failed",
            };
            format!("{state} {} [{:?}]", process.name, process.health)
        })),
    }
    lines.extend([String::new(), "DIAGNOSTICS".into()]);
    if project.diagnostics().is_empty() {
        lines.push("○ none published".into());
    } else {
        lines.extend(
            project
                .diagnostics()
                .iter()
                .map(|(path, diagnostics)| format!("{}  {} issue(s)", path, diagnostics.len())),
        );
    }
    lines.push(String::new());
    lines.push(format!("revision: {}", project.revision()));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn command_parser_preserves_browser_actions_and_rejects_bad_scroll() {
        assert!(matches!(
            parse_command("safari"),
            Ok(ParsedCommand::Local(LocalCommand::Safari))
        ));
        assert!(matches!(
            parse_command("live on"),
            Ok(ParsedCommand::Local(LocalCommand::Live(LiveCommand::Mode(
                TuiLiveMode::On
            ))))
        ));
        assert!(matches!(
            parse_command("live backend ansi"),
            Ok(ParsedCommand::Local(LocalCommand::Live(
                LiveCommand::Backend(TuiLiveBackend::Ansi)
            )))
        ));
        assert!(parse_command("live quality enormous").is_err());
        assert!(matches!(
            parse_command("live quality auto"),
            Ok(ParsedCommand::Local(LocalCommand::Live(
                LiveCommand::AdaptiveQuality
            )))
        ));
        assert!(matches!(
            parse_command("tap 3"),
            Ok(ParsedCommand::Local(LocalCommand::Tap(Some(3))))
        ));
        assert!(matches!(
            parse_command("inbox"),
            Ok(ParsedCommand::Local(LocalCommand::Inbox))
        ));
        assert!(matches!(
            parse_command("capsule save"),
            Ok(ParsedCommand::Local(LocalCommand::Capsule(
                CapsuleCommand::Save
            )))
        ));
        assert!(matches!(
            parse_command("double click r7:b42"),
            Ok(ParsedCommand::Browser(BrowserOperation::DoubleClick(target))) if target == "r7:b42"
        ));
        assert!(matches!(
            parse_command("select ref=r7:b42 premium"),
            Ok(ParsedCommand::Browser(BrowserOperation::Select { target, value }))
                if target == "ref=r7:b42" && value == "premium"
        ));
        assert!(matches!(
            parse_command("shortcut Ctrl+K"),
            Ok(ParsedCommand::Browser(BrowserOperation::Shortcut(shortcut)))
                if shortcut == "Ctrl+K"
        ));
        assert!(matches!(
            parse_command("dismiss-consent"),
            Ok(ParsedCommand::Browser(BrowserOperation::DismissConsent))
        ));
        assert!(parse_command("select target").is_err());
        assert!(matches!(
            parse_command("scroll -4 120"),
            Ok(ParsedCommand::Browser(BrowserOperation::Scroll { dx, dy })) if dx == -4.0 && dy == 120.0
        ));
        assert!(parse_command("scroll nope").is_err());
        assert!(matches!(
            parse_command("workflow workflow.json"),
            Ok(ParsedCommand::Browser(BrowserOperation::Workflow(path))) if path == "workflow.json"
        ));
        assert!(matches!(
            parse_command("resolve-intent intent.json"),
            Ok(ParsedCommand::Browser(BrowserOperation::ResolveIntent(path))) if path == "intent.json"
        ));
        assert!(matches!(
            parse_command("semantic interactive region_search_1"),
            Ok(ParsedCommand::Browser(BrowserOperation::Semantic {
                level: SemanticObservationLevel::Interactive,
                region: Some(region),
            })) if region == "region_search_1"
        ));
        assert!(parse_command("semantic verbose").is_err());
        assert!(matches!(
            parse_command("profiles"),
            Ok(ParsedCommand::Local(LocalCommand::Profiles))
        ));
        assert!(matches!(
            parse_command("knowledge"),
            Ok(ParsedCommand::Local(LocalCommand::Knowledge(None)))
        ));
        assert!(matches!(
            parse_command("knowledge show record-1"),
            Ok(ParsedCommand::Local(LocalCommand::Knowledge(Some(record))))
                if record == "record-1"
        ));
        assert!(matches!(
            parse_command("daemon status"),
            Ok(ParsedCommand::Local(LocalCommand::Daemon(
                DaemonView::Status
            )))
        ));
        assert!(matches!(
            parse_command("daemon recovery"),
            Ok(ParsedCommand::Local(LocalCommand::Daemon(
                DaemonView::Recovery
            )))
        ));
        assert!(matches!(
            parse_command("browser auto-port"),
            Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
                BrowserControlCommand::Launch(BrowserLaunchRequest {
                    port: BrowserPortChoice::Automatic,
                    ..
                })
            )))
        ));
        assert!(matches!(
            parse_command("browser attach 2"),
            Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
                BrowserControlCommand::Attach(BrowserAttachRequest {
                    port: None,
                    target: Some(target),
                })
            ))) if target == "2"
        ));
        assert!(matches!(
            parse_command("browser launch --port 9333 --headed --profile work --chrome-path /opt/chrome"),
            Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
                BrowserControlCommand::Launch(BrowserLaunchRequest {
                    port: BrowserPortChoice::Exact(9333),
                    headed: Some(true),
                    profile: Some(profile),
                    incognito: None,
                    chrome_path: Some(Some(path)),
                })
            ))) if profile == "work" && path == PathBuf::from("/opt/chrome")
        ));
        assert!(matches!(
            parse_command("browser attach --port 9333 target-7"),
            Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
                BrowserControlCommand::Attach(BrowserAttachRequest {
                    port: Some(9333),
                    target: Some(target),
                })
            ))) if target == "target-7"
        ));
        assert!(matches!(
            parse_command("browser targets 9333"),
            Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
                BrowserControlCommand::Targets(Some(9333))
            )))
        ));
        assert!(parse_command("browser launch --profile work --incognito").is_err());
        assert!(matches!(
            parse_command("browser remote-view open"),
            Ok(ParsedCommand::Local(LocalCommand::BrowserControl(
                BrowserControlCommand::RemoteView(RemoteViewCommand::Open)
            )))
        ));
    }

    #[test]
    fn responsive_layout_is_geometry_only_even_for_remote_sessions() {
        let local = RemoteContext::default();
        let remote = RemoteContext {
            ssh: true,
            ..RemoteContext::default()
        };

        assert_eq!(
            display_class(TuiLayout::Auto, 40, local),
            DisplayClass::Phone
        );
        assert_eq!(
            display_class(TuiLayout::Auto, 80, local),
            DisplayClass::Compact
        );
        assert_eq!(
            display_class(TuiLayout::Auto, 80, remote),
            DisplayClass::Compact
        );
        assert_eq!(
            display_class(TuiLayout::Auto, 120, remote),
            DisplayClass::Wide
        );
        assert_eq!(
            display_class(TuiLayout::Mobile, 200, local),
            DisplayClass::Phone
        );
        assert_eq!(
            display_class(TuiLayout::Desktop, 40, remote),
            DisplayClass::Wide
        );
    }

    #[tokio::test]
    async fn live_browser_launch_reconfigures_session_without_restarting_the_tui() {
        let mut options = SessionOptions {
            port: 9222,
            attach: true,
            target_id: Some("old-target".into()),
            ..SessionOptions::default()
        };
        let (events, _event_rx) = mpsc::channel(2);
        let applied = configure_browser_recovery(
            &mut options,
            BrowserRecovery::Launch(BrowserLaunchRequest {
                port: BrowserPortChoice::Exact(9333),
                headed: Some(true),
                profile: Some("mobile-work".into()),
                incognito: None,
                chrome_path: Some(Some(PathBuf::from("/opt/chrome"))),
            }),
            &events,
        )
        .await;

        assert!(applied);
        assert_eq!(options.port, 9333);
        assert!(!options.attach);
        assert_eq!(options.target_id, None);
        assert!(options.headed);
        assert_eq!(options.profile, "mobile-work");
        assert!(!options.incognito);
        assert_eq!(options.chrome_path, Some(PathBuf::from("/opt/chrome")));

        assert!(
            configure_browser_recovery(
                &mut options,
                BrowserRecovery::Launch(BrowserLaunchRequest {
                    incognito: Some(true),
                    chrome_path: Some(None),
                    ..BrowserLaunchRequest::default()
                }),
                &events,
            )
            .await
        );
        assert!(options.incognito);
        assert_eq!(options.profile, "default");
        assert_eq!(options.chrome_path, None);

        let stable_port = options.port;
        assert!(
            !configure_browser_recovery(
                &mut options,
                BrowserRecovery::Launch(BrowserLaunchRequest {
                    port: BrowserPortChoice::Exact(9444),
                    profile: Some("../outside".into()),
                    ..BrowserLaunchRequest::default()
                }),
                &events,
            )
            .await
        );
        assert_eq!(options.port, stable_port);
        assert_eq!(options.profile, "default");
    }

    #[test]
    fn phone_semantic_tap_uses_revisioned_reference_before_navigation_keys() {
        let mut app = App::new_for_product_with_context(
            true,
            TuiLayout::Mobile,
            RemoteContext::default(),
            60,
            LiveViewOptions::default(),
        );
        app.tap_targets = vec![SemanticTapTarget {
            reference: "r9:b42".into(),
            role: "button".into(),
            name: "Continue".into(),
        }];
        app.tap_mode = true;
        assert!(matches!(
            app.reduce_key(key(KeyCode::Char('1'))),
            UiIntent::Pointer(BrowserOperation::Click(reference)) if reference == "r9:b42"
        ));
        assert!(!app.tap_mode);
        assert_eq!(app.mobile_view, MobileView::Home);
    }

    #[test]
    fn adaptive_live_quality_degrades_on_pressure_and_recovers_after_stability() {
        let mut live = LiveViewState::new(
            TuiLiveMode::On,
            TuiLiveBackend::Ansi,
            TuiLiveQuality::Smooth,
            TuiLiveFit::Contain,
            false,
        );
        live.enable_adaptive_quality();
        live.effective_quality = TuiLiveQuality::Smooth;
        live.metrics.drop_ratio = 0.5;
        live.metrics.generation = 1;
        live.adapt_quality();
        assert_eq!(live.effective_quality, TuiLiveQuality::Smooth);
        assert_eq!(live.adaptive_scale_step, 1);
        for generation in 2..=4 {
            live.metrics.drop_ratio = 0.5;
            live.metrics.generation = generation;
            live.adapt_quality();
        }
        assert_eq!(live.effective_quality, TuiLiveQuality::Balanced);
        assert_eq!(live.adaptive_scale_step, 3);
        for generation in 5..=13 {
            live.metrics.drop_ratio = 0.0;
            live.metrics.generation = generation;
            live.adapt_quality();
        }
        assert_eq!(live.effective_quality, TuiLiveQuality::Balanced);
        assert_eq!(live.adaptive_scale_step, 0);
        for generation in 14..=16 {
            live.metrics.generation = generation;
            live.adapt_quality();
        }
        assert_eq!(live.effective_quality, TuiLiveQuality::Smooth);
    }

    #[test]
    fn phone_navigation_is_reachable_without_function_or_control_keys() {
        let mut app = App::new_for_product_with_context(
            true,
            TuiLayout::Mobile,
            RemoteContext {
                ssh: true,
                herdr: true,
                ..RemoteContext::default()
            },
            40,
            LiveViewOptions::default(),
        );
        assert_eq!(app.mode, WorkspaceMode::Development);
        assert_eq!(app.graphics.mode(), GraphicsMode::Semantic);

        assert_eq!(app.reduce_key(key(KeyCode::Char('3'))), UiIntent::None);
        assert_eq!(app.mobile_view, MobileView::App);
        assert!(app.input.is_empty());
        assert_eq!(app.reduce_key(key(KeyCode::Tab)), UiIntent::None);
        assert_eq!(app.mobile_view, MobileView::Diff);
        assert_eq!(app.reduce_key(key(KeyCode::Esc)), UiIntent::None);
        assert_eq!(app.mobile_view, MobileView::Home);

        app.reduce_key(key(KeyCode::Char('a')));
        app.reduce_key(key(KeyCode::Char('3')));
        assert_eq!(app.input, "a3");

        app.input.clear();
        app.handle_project_command("diff");
        assert_eq!(app.mobile_view, MobileView::Diff);
        app.handle_project_command("files");
        assert_eq!(app.mobile_view, MobileView::Project);
    }

    #[test]
    fn phone_layout_renders_at_portrait_size_and_has_no_graphics_pane() {
        use ratatui::backend::TestBackend;

        let mut app = App::new_for_product_with_context(
            true,
            TuiLayout::Mobile,
            RemoteContext {
                ssh: true,
                ..RemoteContext::default()
            },
            40,
            LiveViewOptions::default(),
        );
        app.sync_graphics_geometry(Rect::new(0, 0, 40, 20)).unwrap();
        assert!(app.graphics.geometry().is_none());
        assert!(app.mobile_nav_area.is_some());

        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("1 Overview"));
        assert!(rendered.contains("6 Process"));
        assert!(rendered.contains("Command"));
    }

    #[test]
    fn phone_live_view_uses_bounded_ansi_frames_and_adaptive_capture() {
        use ratatui::backend::TestBackend;

        let mut app = App::new_for_product_with_context(
            true,
            TuiLayout::Mobile,
            RemoteContext {
                ssh: true,
                ..RemoteContext::default()
            },
            40,
            LiveViewOptions {
                mode: TuiLiveMode::On,
                backend: TuiLiveBackend::Ansi,
                quality: TuiLiveQuality::Data,
                ..LiveViewOptions::default()
            },
        );
        app.set_mobile_view(MobileView::App);
        app.sync_graphics_geometry(Rect::new(0, 0, 40, 20)).unwrap();
        let config = app.live_capture_config();
        assert!(config.enabled);
        assert_eq!(config.requested_fps, 3);
        assert!(config.minimum_interval <= Duration::from_millis(334));
        assert!(config.max_width <= 320);
        app.apply_visual_frame(
            base64::engine::general_purpose::STANDARD.encode(test_live_png()),
            serde_json::json!({"deviceWidth": 2, "deviceHeight": 2}),
            7,
        )
        .unwrap();
        assert!(!app.live.ansi.cells().is_empty());
        assert_eq!(app.graphics.browser_revision(), 7);

        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| cell.symbol() == "▀")
        );
    }

    #[test]
    fn live_backend_policy_is_conservative_and_mosh_safe() {
        let mut live = LiveViewState::new(
            TuiLiveMode::Auto,
            TuiLiveBackend::Auto,
            TuiLiveQuality::Balanced,
            TuiLiveFit::Contain,
            false,
        );
        live.herdr_environment = None;
        live.select_backend();
        assert_eq!(live.backend, None);

        live.mode = TuiLiveMode::On;
        live.select_backend();
        assert_eq!(live.backend, Some(ActiveLiveBackend::Ansi));

        live.kitty_detected = true;
        live.select_backend();
        assert_eq!(live.backend, Some(ActiveLiveBackend::Kitty));

        let app = App::new_for_product_with_context(
            true,
            TuiLayout::Mobile,
            RemoteContext {
                ssh: true,
                mosh: true,
                ..RemoteContext::default()
            },
            40,
            LiveViewOptions {
                mode: TuiLiveMode::On,
                ..LiveViewOptions::default()
            },
        );
        assert_eq!(app.live.backend, None);
        assert_eq!(app.live_capture_config().requested_fps, 0);
    }

    #[test]
    fn malformed_live_frame_degrades_without_terminating_the_tui() {
        let mut app = App::new();
        app.configure_live(
            Some(TuiLiveMode::On),
            Some(TuiLiveBackend::Ansi),
            None,
            None,
        );
        app.sync_graphics_geometry(Rect::new(0, 0, 100, 30))
            .unwrap();
        app.apply_visual_frame(
            base64::engine::general_purpose::STANDARD.encode(b"not a png"),
            serde_json::json!({}),
            1,
        )
        .unwrap();
        assert!(app.visual_status.contains("rejected"));
        assert_eq!(app.live.metrics.dropped, 1);
    }

    fn test_live_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255])
                .unwrap();
        }
        bytes
    }

    #[test]
    fn safari_handoff_keeps_the_app_private_and_preserves_the_route() {
        let guidance = safari_handoff("http://0.0.0.0:3000/orders?state=open").unwrap();
        assert!(guidance.contains("remote host: 127.0.0.1"));
        assert!(guidance.contains("http://127.0.0.1:3000/orders?state=open"));
        assert!(guidance.contains("Do not bind Chrome CDP"));
        let secret =
            safari_handoff("https://user:password@localhost:3443/orders?token=secret&state=open")
                .unwrap();
        assert!(!secret.contains("password"));
        assert!(!secret.contains("token=secret"));
        assert!(secret.contains("token=%5Bredacted%5D"));
        assert!(secret.contains("state=open"));
        assert!(safari_handoff("file:///tmp/index.html").is_err());
    }

    #[test]
    fn reducer_edits_unicode_and_requests_matching_cancellation() {
        let mut app = App::new();
        assert_eq!(app.reduce_key(key(KeyCode::Char('日'))), UiIntent::None);
        assert_eq!(app.reduce_key(key(KeyCode::Char('本'))), UiIntent::None);
        app.reduce_key(key(KeyCode::Backspace));
        assert_eq!(app.input, "日");

        app.browser_state = BrowserState::Ready;
        app.begin_operation(4, "Observe");
        assert_eq!(app.reduce_key(key(KeyCode::Esc)), UiIntent::Cancel(4));
        assert!(app.busy.as_ref().unwrap().cancelling);
        assert_eq!(app.reduce_key(key(KeyCode::Esc)), UiIntent::None);
    }
    #[test]
    fn mutating_queue_requires_and_releases_human_lease() {
        let mut app = App::new();
        app.browser_state = BrowserState::Ready;
        let (commands, _receiver) = mpsc::channel(1);

        queue_browser_operation(
            &mut app,
            &commands,
            BrowserOperation::Click("r1:b1".to_string()),
        );
        assert!(matches!(
            app.mutation_lease.state(),
            MutationLeaseState::Held(MutationActor::Human)
        ));
        let id = app.busy.as_ref().expect("queued operation").id;
        app.apply_browser_event(BrowserEvent::OperationCancelled { id })
            .unwrap();
        assert_eq!(app.mutation_lease.state(), MutationLeaseState::Available);
    }

    #[test]
    fn app_bounds_retained_page_state() {
        let mut app = App::new();
        app.set_page_content("界".repeat(TUI_PAGE_MAX_BYTES));

        assert!(app.page_content.len() <= TUI_PAGE_MAX_BYTES);
        assert!(app.page_content.contains("[truncated]"));
    }

    #[test]
    fn graphics_geometry_tracks_resize_and_releases_on_shutdown() {
        let mut app = App::new();
        app.configure_live(
            Some(TuiLiveMode::On),
            Some(TuiLiveBackend::Ansi),
            None,
            None,
        );
        assert!(
            app.sync_graphics_geometry(Rect::new(0, 0, 100, 30))
                .unwrap()
        );
        let first_revision = app.graphics.geometry_revision();
        assert!(first_revision > 0);
        assert!(
            !app.sync_graphics_geometry(Rect::new(0, 0, 100, 30))
                .unwrap()
        );
        assert!(
            app.sync_graphics_geometry(Rect::new(0, 0, 120, 30))
                .unwrap()
        );
        assert!(app.graphics.geometry_revision() > first_revision);
        app.graphics_shutdown().unwrap();
        assert_eq!(app.graphics.diagnostics().current_bytes, 0);
        assert_eq!(app.graphics.diagnostics().pending_bytes, 0);
    }

    #[tokio::test]
    async fn matching_cancel_interrupts_an_active_operation() {
        let (command_tx, mut command_rx) = mpsc::channel(2);
        let (_shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let (event_tx, _event_rx) = mpsc::channel(2);
        command_tx
            .send(BrowserCommand::Cancel { id: 9 })
            .await
            .unwrap();

        let result = await_active_operation(
            std::future::pending::<BrowserResult<Box<OperationResult>>>(),
            9,
            &mut command_rx,
            &mut shutdown_rx,
            &event_tx,
        )
        .await;

        assert!(matches!(result, ActiveOperationState::Cancelled));
    }

    #[tokio::test]
    async fn delayed_worker_event_does_not_block_input_reducer() {
        let (command_tx, mut command_rx) = mpsc::channel(2);
        let (event_tx, mut event_rx) = mpsc::channel(4);
        let worker = tokio::spawn(async move {
            let Some(BrowserCommand::Execute { id, .. }) = command_rx.recv().await else {
                return;
            };
            event_tx
                .send(BrowserEvent::OperationStarted {
                    id,
                    label: "Observe".to_string(),
                })
                .await
                .unwrap();
            time::sleep(Duration::from_millis(100)).await;
            event_tx
                .send(BrowserEvent::OperationFinished {
                    id,
                    result: Box::new(OperationResult {
                        activity: "Observation refreshed.".to_string(),
                        update: None,
                    }),
                })
                .await
                .unwrap();
        });

        let mut app = App::new();
        app.browser_state = BrowserState::Ready;
        app.begin_operation(1, "Observe");
        command_tx
            .send(BrowserCommand::Execute {
                id: 1,
                operation: BrowserOperation::Observe { fresh: false },
            })
            .await
            .unwrap();
        app.apply_browser_event(event_rx.recv().await.unwrap())
            .unwrap();

        assert_eq!(app.reduce_key(key(KeyCode::Char('x'))), UiIntent::None);
        assert_eq!(app.input, "x");
        assert!(
            time::timeout(Duration::from_millis(20), event_rx.recv())
                .await
                .is_err()
        );

        app.apply_browser_event(event_rx.recv().await.unwrap())
            .unwrap();
        assert!(!app.is_busy());
        worker.await.unwrap();
    }

    #[test]
    fn workspace_modes_and_lease_are_revision_guarded() {
        let mut app = App::new();
        assert_eq!(app.mode(), WorkspaceMode::Browser);
        assert_eq!(app.reduce_key(key(KeyCode::F(4))), UiIntent::None);
        assert_eq!(app.mode(), WorkspaceMode::Semantic);

        let mut lease = MutationLease::default();
        let revision = lease.acquire(MutationActor::Human, 0).unwrap();
        assert!(lease.acquire(MutationActor::Agent, revision).is_err());
        let takeover = lease.takeover(MutationActor::Agent, revision).unwrap();
        assert!(lease.reconcile(takeover).is_ok());
        assert_eq!(
            lease.state(),
            MutationLeaseState::Held(MutationActor::Agent)
        );
        assert!(lease.release(MutationActor::Human, takeover).is_err());
        assert!(lease.release(MutationActor::Agent, takeover).is_ok());
    }

    #[test]
    fn pointer_mapping_rejects_stale_geometry_and_emits_coordinate_action() {
        let mut app = App::new();
        app.configure_live(
            Some(TuiLiveMode::On),
            Some(TuiLiveBackend::Ansi),
            None,
            None,
        );
        app.sync_graphics_geometry(Rect::new(0, 0, 100, 30))
            .unwrap();
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 60,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        assert!(matches!(
            app.mouse_intent(mouse),
            UiIntent::Pointer(BrowserOperation::ClickAt { .. })
        ));
        app.graphics.clear_pane().unwrap();
        assert!(matches!(app.mouse_intent(mouse), UiIntent::None));
    }
}

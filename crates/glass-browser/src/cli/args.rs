//! CLI argument definitions (clap).
//!
//! Defines the top-level `Cli` struct and all subcommands for one-shot
//! browser operations, profile management, and server modes.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::browser::policy::{PolicyCapability, PolicyPreset};
use crate::browser::session::{
    BatchMode, InteractionMode, PreflightAction, VisualClip, VisualFormat,
};
use crate::results::ResponseMode;
/// Glass — semantic browser execution for humans and agents.
///
/// Run `glass` for the interactive terminal workspace, or choose a command for
/// a bounded one-shot operation. Browser actions share revision guards,
/// verification, workspace ownership, and typed results across CLI, TUI, MCP,
/// daemon, and Rust interfaces.
#[derive(Debug, Parser)]
#[command(
    name = "glass",
    version,
    about = "Semantic browser execution for humans and agents",
    disable_help_subcommand = true,
    after_help = "Start here:\n  glass doctor                         Check browser and local runtime support\n  glass                                Open the interactive terminal workspace\n  glass navigate https://example.com   Open a page in one bounded operation\n  glass observe --level interactive    Inspect current semantic understanding\n  glass task compile task.json ir.json Compile a deterministic browser-free task\n\nUse `glass <command> --help` for command-specific options and examples."
)]
pub struct Cli {
    /// Run trusted local automation without interactive capability or tool approvals.
    /// Glass Dev also enables unrestricted Pi resources and tools in this mode.
    #[arg(long, global = true)]
    pub yolo: bool,

    /// Browser safety preset. Hardened mode fails closed for privileged operations.
    #[arg(long, global = true, value_enum, default_value_t = PolicyPreset::Development)]
    pub policy: PolicyPreset,

    /// Explicitly allow a privileged capability under the selected policy.
    #[arg(long = "policy-allow", global = true, value_enum)]
    pub policy_allow: Vec<PolicyCapability>,

    /// Return a typed confirmation-required result for this capability.
    #[arg(long = "policy-confirm", global = true, value_enum)]
    pub policy_confirm: Vec<PolicyCapability>,

    /// Supply one consumable approval token for a confirmation-required capability.
    #[arg(long = "policy-confirm-once", global = true, value_enum)]
    pub policy_confirm_once: Vec<PolicyCapability>,

    /// Opt into the experimental sandboxed extension capability.
    #[arg(long, global = true)]
    pub experimental_extensions: bool,

    /// Permit only these exact hosts in hardened mode (repeatable).
    #[arg(long = "policy-allow-host", global = true)]
    pub policy_allow_host: Vec<String>,

    /// Deny these exact hosts in hardened mode (repeatable).
    #[arg(long = "policy-deny-host", global = true)]
    pub policy_deny_host: Vec<String>,

    /// Named browser profile used for persistent cookies and storage.
    #[arg(long, global = true, default_value = "default")]
    pub profile: String,

    /// Use a temporary browser profile without persistence.
    #[arg(long, global = true)]
    pub incognito: bool,

    /// Attach to an existing Chrome CDP endpoint instead of launching Chrome.
    /// The default profile value is ignored in this mode.
    #[arg(long, global = true)]
    pub attach: bool,

    /// Attach browser operations to a named persistent local session.
    /// The session's verified loopback port is resolved before the operation starts.
    #[arg(long, global = true, value_name = "NAME")]
    pub session: Option<String>,

    /// Chrome page target ID. Required when the selected endpoint has multiple
    /// page targets.
    #[arg(long = "target-id", global = true)]
    pub target_id: Option<String>,

    /// Chrome frame ID used by commands in this one-shot session.
    #[arg(long = "frame-id", global = true)]
    pub frame_id: Option<String>,

    /// Chrome remote debugging port.
    #[arg(long, global = true, default_value_t = 9222)]
    pub port: u16,

    /// Show the browser window instead of using headless mode.
    #[arg(long, global = true)]
    pub headed: bool,
    /// CSS viewport dimensions applied before navigation, for example `1280x800`.
    #[arg(long, global = true, value_name = "WIDTHxHEIGHT")]
    pub viewport: Option<String>,

    /// Pointer behavior for click actions.
    #[arg(long, global = true, value_enum, default_value_t = InteractionMode::Human)]
    pub interaction: InteractionMode,

    /// Enable bounded session audit log of high-risk operations.
    #[arg(long, global = true)]
    pub audit: bool,

    /// Emit a bounded JSON failure-trace pack when a browser operation fails.
    #[arg(long, global = true)]
    pub trace_on_error: bool,

    /// Path to a Chrome/Chromium binary.
    #[arg(long = "chrome-path", alias = "chrome", global = true)]
    pub chrome_path: Option<PathBuf>,

    /// Run the MCP server over stdio.
    #[arg(long)]
    pub mcp: bool,

    /// Terminal workspace layout. Auto uses terminal geometry only.
    #[arg(long = "tui-layout", global = true, value_enum, default_value_t = TuiLayout::Auto)]
    pub tui_layout: TuiLayout,

    /// Connection transport policy. Auto keeps unmeasured SSH links unknown.
    #[arg(long = "tui-transport", global = true, value_enum, default_value_t = TuiTransport::Auto)]
    pub tui_transport: TuiTransport,

    /// Graphics capability override. Auto requires an active protocol probe.
    #[arg(long = "tui-graphics", global = true, value_enum, default_value_t = TuiGraphics::Auto)]
    pub tui_graphics: TuiGraphics,

    /// Measured/known round-trip latency for presentation policy diagnostics.
    #[arg(long = "tui-rtt-ms", global = true)]
    pub tui_rtt_ms: Option<f64>,

    /// Measured/known effective terminal throughput in megabits per second.
    #[arg(long = "tui-throughput-mbps", global = true)]
    pub tui_throughput_mbps: Option<f64>,

    /// Terminal-native live browser policy. Off preserves semantic-only startup.
    #[arg(long = "tui-live", global = true, value_enum, default_value_t = TuiLiveMode::Off)]
    pub tui_live: TuiLiveMode,

    /// Preferred terminal-native live browser renderer.
    #[arg(long = "tui-live-backend", global = true, value_enum, default_value_t = TuiLiveBackend::Auto)]
    pub tui_live_backend: TuiLiveBackend,

    /// Terminal-native live browser bandwidth and frame-rate profile.
    #[arg(long = "tui-live-quality", global = true, value_enum, default_value_t = TuiLiveQuality::Balanced)]
    pub tui_live_quality: TuiLiveQuality,

    /// How ANSI-sampled browser pixels fit inside the terminal App pane.
    #[arg(long = "tui-live-fit", global = true, value_enum, default_value_t = TuiLiveFit::Contain)]
    pub tui_live_fit: TuiLiveFit,

    /// Override the per-profile persistent knowledge snapshot path.
    #[arg(long, global = true)]
    pub knowledge_store: Option<PathBuf>,

    /// Select the bounded agent-facing response projection.
    #[arg(long, global = true, value_enum, default_value_t = ResponseMode::Minimal)]
    pub response_mode: ResponseMode,

    /// One-shot prompt, for example: `navigate to https://example.com`.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum McpClient {
    Generic,
    ClaudeCode,
    Codex,
}

/// Responsive layout policy for the interactive terminal workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TuiLayout {
    /// Select from terminal geometry only.
    #[default]
    Auto,
    /// Force the multi-pane desktop workspace, including on narrow terminals.
    Desktop,
    /// Use a single-pane, phone-friendly workspace and semantic browser output.
    Mobile,
    /// Force a condensed workspace independent of transport.
    Compact,
}

/// Presentation transport classification. This is independent from layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TuiTransport {
    #[default]
    Auto,
    Local,
    RemoteFast,
    RemoteConstrained,
    Mosh,
    UnknownRemote,
}

/// Terminal graphics evidence/override, independent from transport and layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TuiGraphics {
    #[default]
    Auto,
    Kitty,
    Sixel,
    ITermInline,
    Ansi,
    SemanticOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TuiLiveMode {
    /// Do not capture continuous browser pixels; explicit screenshots still work.
    #[default]
    Off,
    /// Enable live pixels only when a native Herdr or Kitty backend is detected.
    Auto,
    /// Enable live pixels and use the ANSI renderer when native graphics are absent.
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TuiLiveBackend {
    /// Prefer Herdr, then Kitty, then ANSI according to the live policy.
    #[default]
    Auto,
    /// Use Herdr's owned pane graphics stream.
    Herdr,
    /// Emit the Kitty terminal graphics protocol directly.
    Kitty,
    /// Render browser pixels as true-color Unicode half blocks.
    Ansi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TuiLiveQuality {
    /// Approximately 3 FPS at a compact capture size.
    Data,
    /// Approximately 6 FPS with balanced resolution and transfer cost.
    #[default]
    Balanced,
    /// Approximately 12 FPS at the largest bounded pane resolution.
    Smooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TuiLiveFit {
    /// Show the complete frame with letterboxing; native image backends always use this.
    #[default]
    Contain,
    /// Fill the ANSI pane and crop browser edges when aspect ratios differ.
    Cover,
    /// Use one source pixel per ANSI half-block sample and crop overflow.
    Actual,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Update the Cargo package that owns this executable.
    Update {
        /// Print the resolved package, provenance, root, and Cargo command without running it.
        #[arg(long)]
        dry_run: bool,
        /// Install a specific Cargo-compatible version requirement instead of the latest release.
        #[arg(long)]
        version: Option<String>,
        /// Reinstall even when Cargo considers the selected version current.
        #[arg(long)]
        force: bool,
        /// Use this configured Cargo registry. Required to change from an unknown or non-crates.io source.
        #[arg(long)]
        registry: Option<String>,
    },

    /// Download and install a managed Chrome for Testing build.
    InstallChromium {
        /// Reinstall the version pinned by this Glass release.
        #[arg(long)]
        update: bool,
    },

    /// Evaluate release evidence and forbidden outcomes without starting a browser.
    Certify {
        #[command(subcommand)]
        action: CertifyCommand,
    },

    /// Inspect local workspace identity, ownership, and lifecycle state.
    Workspace {
        #[command(subcommand)]
        action: WorkspaceCommand,
    },
    /// Inspect and operate the current project development runtime.
    Project {
        #[command(subcommand)]
        action: ProjectCommand,
    },
    /// Send a prompt or control request to the local Glass harness.
    Agent {
        #[command(subcommand)]
        action: AgentCommand,
    },
    /// Inspect and manage advisory semantic memory.
    Memory {
        #[command(subcommand)]
        action: MemoryCommand,
    },
    /// Validate surface evidence and report capability coverage.
    Surfaces {
        #[command(subcommand)]
        action: SurfaceCommand,
    },
    /// Inspect a declared transport-neutral backend profile.
    Backend {
        #[command(subcommand)]
        action: BackendCommand,
    },

    /// Start and inspect the local daemon lifecycle.
    Daemon {
        #[command(subcommand)]
        action: DaemonCommand,
    },
    /// Inspect, compare, or attach a validated replay bundle.
    Replay {
        #[command(subcommand)]
        action: ReplayCommand,
    },
    /// Print the versioned Glass capability manifest without starting Chrome.
    Capabilities,

    /// Inspect local browser, daemon, profile, policy, and store health.
    Doctor {
        /// Emit the stable machine-readable diagnostic contract.
        #[arg(long)]
        json: bool,
    },

    /// Print deterministic MCP configuration for a supported client.
    McpConfig {
        #[arg(long, value_enum, default_value_t = McpClient::Generic)]
        client: McpClient,
        /// Explicitly print the generated JSON configuration.
        #[arg(long)]
        print: bool,
    },

    /// List or manage saved profiles.
    Profiles {
        #[command(subcommand)]
        action: Option<ProfileCommand>,
    },

    /// Inspect and manage the bounded local knowledge store.
    Knowledge {
        #[command(subcommand)]
        action: KnowledgeCommand,
    },

    /// Inspect and purge bounded local diagnostic result artifacts.
    Result {
        #[command(subcommand)]
        action: ResultCommand,
    },

    /// Delete a saved profile.
    DeleteProfile { name: String },

    /// Navigate to a URL.
    Navigate {
        url: String,
        #[arg(long, default_value_t = 20_000)]
        timeout_ms: u64,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Click an element by an explicit ref/name/role/text/CSS/ordinal locator.
    Click {
        target: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Resolve a target and report clickability without performing an action.
    Preflight {
        target: String,
        #[arg(long, value_enum, default_value_t = PreflightAction::Click)]
        action: PreflightAction,
    },

    /// Click exact viewport coordinates for canvas/map surfaces.
    ClickAt { x: f64, y: f64 },

    /// Click an element expected to open exactly one causally verified popup.
    ClickExpectPopup {
        target: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Double-click an element by an explicit ref/name/role/text/CSS/ordinal locator.
    DoubleClick {
        target: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Move the pointer over an element without clicking.
    Hover { target: String },

    /// Drag one element to another uniquely resolved element.
    Drag {
        source: String,
        destination: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Type text into the focused element, optionally clicking a target first.
    Type {
        text: String,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Dispatch one complete key press.
    Key {
        key: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Dispatch only a key-down event.
    KeyDown {
        key: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Dispatch only a key-up event.
    KeyUp {
        key: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Dispatch a modifier shortcut such as Control+A.
    Shortcut {
        shortcut: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Clear an editable element.
    Clear {
        target: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Ensure a checkbox or radio is checked.
    Check {
        target: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Ensure a checkbox is unchecked.
    Uncheck {
        target: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Select one exact option value.
    Select {
        target: String,
        value: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Set a bounded list of regular files on one file input.
    Upload {
        target: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Capture a PNG screenshot.
    Screenshot {
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,
        #[arg(long, value_enum, default_value_t = VisualFormat::Png)]
        format: VisualFormat,
        #[arg(long)]
        quality: Option<u8>,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        #[arg(long, conflicts_with_all = ["clip", "target"])]
        full_page: bool,
        #[arg(long, conflicts_with_all = ["full_page", "target"])]
        clip: Option<VisualClip>,
        #[arg(long, conflicts_with_all = ["full_page", "clip"])]
        target: Option<String>,
    },

    /// Print the visible page text.
    Text,

    /// Print the full DOM tree. This is an explicit deep-inspection request.
    Dom,

    /// Print compact accessibility and text context.
    Observe {
        /// Include the full DOM tree. This is an explicit deep-inspection request.
        #[arg(long)]
        deep_dom: bool,
        /// Include a PNG screenshot in the structured context.
        #[arg(long)]
        screenshot: bool,
        /// Include bounded, policy-gated form field values.
        #[arg(long)]
        form_values: bool,
        /// Return the versioned semantic observation at the requested level.
        #[arg(long = "level", alias = "semantic-level", value_parser = parse_semantic_level)]
        semantic_level: Option<String>,
        /// Expand one semantic region from the current observation.
        #[arg(long, requires = "semantic_level")]
        region: Option<String>,
    },

    /// Capture the bounded task-oriented page inspection contract.
    InspectPage,
    /// Run bounded navigation, observation, inspection, and safe target probes for a site manifest.
    SmokeSites {
        /// JSON file containing `{ "sites": [...] }`.
        input: PathBuf,
        /// Stop after the first site-level failure.
        #[arg(long)]
        stop_on_error: bool,
    },

    /// Resolve target candidates without acting.
    FindTarget { input: PathBuf },

    /// Run one guarded semantic action and verify an optional postcondition.
    ActAndVerify {
        input: PathBuf,
        #[arg(long)]
        predicate: Option<String>,
        #[arg(long, default_value_t = 10_000)]
        timeout_ms: u64,
    },

    /// Extract typed records from a fresh semantic region.
    ExtractStructured { input: PathBuf },

    /// Recover a potentially indeterminate execution conservatively.
    RecoverRun { execution_id: String },

    /// Scroll the page by CSS pixels.
    Scroll {
        #[arg(long, default_value_t = 0.0)]
        dx: f64,
        #[arg(long, default_value_t = 600.0)]
        dy: f64,
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Wait for one explicit browser condition until a bounded deadline.
    Wait {
        condition: String,
        #[arg(long, default_value_t = 10_000)]
        timeout_ms: u64,
    },

    /// Collect bounded, redacted console and network evidence.
    Diagnostics {
        #[arg(long, default_value_t = 1_000)]
        duration_ms: u64,
    },

    /// Accept the currently open JavaScript dialog.
    AcceptDialog,

    /// Dismiss the currently open JavaScript dialog.
    DismissDialog,

    /// Dismiss a recognized OneTrust/Cookiebot consent wall.
    DismissConsent,

    /// Wait for one download into an authorized existing directory.
    Download {
        destination: PathBuf,
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
    },

    /// List discoverable page targets without changing the active target.
    Targets,

    /// Write a bounded, redacted page-target archive without selecting or closing any target.
    ArchiveTargets {
        /// Optional JSON output path. Without it, print the archive to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Create a page target without selecting it.
    NewTarget { url: String },

    /// Explicitly select the page target used by subsequent commands.
    SelectTarget { id: String },

    /// Close one page target.
    CloseTarget { id: String },

    /// List frames in the active page target.
    Frames,

    /// Explicitly select the frame used by subsequent commands.
    SelectFrame { id: String },

    /// Evaluate JavaScript in the current page.
    Evaluate { expression: String },

    /// List all browser cookies for the current page.
    Cookies,

    /// Export current browser cookies as bounded JSON.
    ExportCookies { output: PathBuf },

    /// Import browser cookies from bounded JSON.
    ImportCookies { input: PathBuf },

    /// Save the current page as a PDF.
    Pdf {
        #[arg(short, long, default_value = "page.pdf")]
        output: String,
        #[arg(long)]
        background: bool,
    },

    /// Fill multiple form fields from a JSON value.
    FillForm {
        /// JSON array of {target, value} objects.
        #[arg(long)]
        fields: String,
        /// Initial observation revision required before filling.
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    /// Execute a bounded typed batch from a JSON array or stdin.
    Batch {
        /// JSON file containing the batch steps; omit to read stdin.
        input: Option<PathBuf>,
        #[arg(long)]
        atomic: bool,
        /// Revision policy: fixed, chain, or unguarded.
        #[arg(long, value_enum, default_value_t = BatchMode::Unguarded)]
        mode: BatchMode,
        /// Initial observation revision required by fixed and chain modes.
        #[arg(long)]
        expected_revision: Option<u64>,
    },

    #[command(subcommand_precedence_over_arg = true)]
    Workflow {
        /// Offline authoring operation. Omit to execute the workflow.
        #[command(subcommand)]
        action: Option<WorkflowAuthoringCommand>,
        /// JSON file containing `{ "workflow": ..., "inputs": ... }`.
        input: Option<PathBuf>,
    },

    /// Validate and compile a browser-free Task Protocol task.
    Task {
        #[command(subcommand)]
        action: TaskCommand,
    },

    /// Inspect or diff browser-free Glass Web IR v1 JSON.
    Ir {
        #[command(subcommand)]
        action: IrCommand,
    },

    /// Reconcile a workflow checkpoint and execute only its safe pending suffix.
    WorkflowResume {
        /// JSON file containing the workflow definition.
        workflow: PathBuf,
        /// JSON file containing a workflow checkpoint.
        checkpoint: PathBuf,
        /// Optional JSON file containing the workflow input map.
        #[arg(long)]
        inputs: Option<PathBuf>,
    },

    /// Resolve a declared intent from JSON or stdin without dispatching it.
    ResolveIntent {
        /// JSON file containing the versioned intent request; omit to read stdin.
        input: Option<PathBuf>,
    },

    /// Resolve and execute one explicitly selected intent candidate.
    ExecuteIntent {
        /// JSON file containing the versioned execution request; omit to read stdin.
        input: Option<PathBuf>,
    },

    /// Evaluate a bounded JSON verification predicate.
    Verify {
        /// JSON object such as `{"urlEquals":"https://example.com"}`.
        predicate: String,
        #[arg(long, default_value_t = 10_000)]
        timeout_ms: u64,
    },

    /// Reconcile revisioned references against the current observation.
    ReconcileRefs {
        #[arg(long)]
        from_revision: u64,
        /// Stable locators tried positionally after backend identity is gone.
        #[arg(long = "hint")]
        hints: Vec<String>,
        /// Current revisioned landmark/container ref used to narrow relocation.
        #[arg(long)]
        scope: Option<String>,
        #[arg(required = true)]
        refs: Vec<String>,
    },

    /// Report a bounded delta from the last compact observation.
    ObserveDelta,

    /// Export or import a bounded workflow checkpoint.
    Checkpoint {
        #[command(subcommand)]
        action: CheckpointCommand,
    },

    /// Create, inspect, diff, or purge redacted local session snapshots.
    Snapshot {
        #[command(subcommand)]
        action: SnapshotCommand,
    },

    /// Read text from the system clipboard.
    ClipboardRead,

    /// Write text to the system clipboard.
    ClipboardWrite { text: String },

    /// Launch the browser-first terminal workspace.
    ///
    /// `glass browser` is the shortest path to a browser session; `glass browser tui`
    /// is an explicit equivalent.
    Browser {
        #[command(subcommand)]
        action: Option<BrowserCommand>,
    },

    /// Start, inspect, and stop a persistent local browser session.
    Session {
        #[command(subcommand)]
        action: SessionCommand,
    },

    /// Show grouped help and the embedded Glass workflow skills.
    Help { topic: Option<String> },

    /// Launch the development terminal workspace.
    Tui,
}

#[derive(Debug, Subcommand)]
pub enum BrowserCommand {
    /// Launch the focused browser terminal workspace.
    Tui,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// Start one named persistent browser session.
    Start {
        #[arg(default_value = "default")]
        name: String,
    },
    /// Inspect one persistent browser session without starting Chrome.
    Status {
        #[arg(default_value = "default")]
        name: String,
    },
    /// Stop one persistent browser session and its owned browser.
    Stop {
        #[arg(default_value = "default")]
        name: String,
    },
    /// Print the attach command for one persistent browser session.
    Open {
        #[arg(default_value = "default")]
        name: String,
    },
    /// Foreground owner used internally by `session start`.
    #[command(hide = true)]
    Serve {
        name: String,
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        status: PathBuf,
    },
}

fn parse_semantic_level(value: &str) -> Result<String, String> {
    match value {
        "summary" | "interactive" | "structured" | "detailed" | "raw" => Ok(value.into()),
        _ => Err("expected summary, interactive, structured, detailed, or raw".into()),
    }
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    List,
    Create { name: String },
    Delete { name: String },
}

#[derive(Debug, Subcommand)]
pub enum KnowledgeCommand {
    /// List all validated records.
    List,
    /// Show one record by ID.
    Show { record_id: String },
    /// Explain one record's provenance, lifecycle, and invalidation rules.
    Explain { record_id: String },
    /// Print lifecycle and serialized-size statistics.
    Stats,
    /// Export the validated snapshot to stdout or a file.
    Export { output: Option<PathBuf> },
    /// Import and replace the complete validated snapshot.
    Import { input: PathBuf },
    /// Move one record to a non-eligible state.
    Invalidate {
        record_id: String,
        #[arg(value_enum)]
        state: KnowledgeInvalidationState,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        observed_at: Option<String>,
    },
    /// Remove every record for one exact origin.
    Purge { origin: String },
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    List,
    Inspect { id: String },
    Suspend { id: String },
    Resume { id: String },
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Detect the project and print its runtime configuration.
    Inspect {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// List bounded project files.
    Files {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Search files, browser entities, processes, events, and commands.
    Search {
        query: String,
        #[arg(long, default_value_t = 64)]
        limit: usize,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Read one workspace-confined file.
    Read {
        path: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Edit and save one file through the native buffer contract.
    Edit {
        path: String,
        #[arg(long, conflicts_with = "input")]
        content: Option<String>,
        #[arg(long, conflicts_with = "content")]
        input: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Create a workspace-confined directory, including missing parents.
    Mkdir {
        path: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Rename or move one workspace-confined file or directory.
    Rename {
        from: String,
        to: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Delete one file or an empty directory after explicit confirmation.
    Delete {
        path: String,
        #[arg(long)]
        yes: bool,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Request real diagnostics from rust-analyzer through LSP.
    Diagnostics {
        path: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Start a named development process in a real PTY.
    Run {
        name: String,
        #[arg(long)]
        command: Option<String>,
        #[arg(long)]
        wait: bool,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Run the detected test command in a PTY and wait for completion.
    Test {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Run the detected lint command in a PTY and wait for completion.
    Lint {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Inspect, start, stop, or read a managed process.
    Process {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[command(subcommand)]
        action: ProjectProcessCommand,
    },
    /// Show code, runtime, semantic, and workflow impact.
    Diff {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Record an explicit source/runtime relationship with evidence.
    Link {
        entity: String,
        path: String,
        #[arg(long)]
        start_line: u32,
        #[arg(long)]
        end_line: u32,
        #[arg(long, default_value = "explicit-marker")]
        provenance: String,
        #[arg(long, default_value_t = 1.0)]
        confidence: f32,
        #[arg(long, default_value = "explicit project link")]
        detail: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Discover or inspect bidirectional source/runtime graph links.
    Graph {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[command(subcommand)]
        action: ProjectGraphCommand,
    },
    /// Evaluate one semantic regression breakpoint against two snapshots.
    Breakpoint {
        kind: String,
        entity: String,
        before: PathBuf,
        after: PathBuf,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Show the bounded development timeline.
    Timeline {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Replay a bounded window of attributed development revisions.
    Replay {
        #[arg(long, default_value_t = 0)]
        start: usize,
        #[arg(long, default_value_t = 64)]
        limit: usize,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Probe or launch Neovim integration.
    Neovim {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[command(subcommand)]
        action: NeovimCommand,
    },
    /// Create an isolated Git worktree development experiment.
    Experiment {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[command(subcommand)]
        action: ExperimentCommand,
    },
    /// Attach an external actor to the attributed development timeline.
    Attach {
        actor: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectGraphCommand {
    Discover,
    Entity {
        entity: String,
    },
    Source {
        path: String,
        #[arg(long)]
        line: Option<u32>,
    },
}

#[derive(Debug, Subcommand)]
pub enum NeovimCommand {
    Probe,
    Start {
        #[arg(long, default_value = "neovim")]
        name: String,
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ExperimentCommand {
    Create {
        name: String,
        #[arg(long)]
        port: u16,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectProcessCommand {
    List,
    Start {
        name: String,
        command: String,
        #[arg(long)]
        wait: bool,
    },
    Stop {
        name: String,
    },
    Restart {
        name: String,
    },
    Remove {
        name: String,
    },
    Input {
        name: String,
        input: String,
    },
    Resize {
        name: String,
        cols: u16,
        rows: u16,
    },
    Output {
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Inspect Node, Pi SDK, authentication, model, and session readiness.
    Doctor,
    /// Install or select the Glass-managed native Pi SDK runtime.
    Setup {
        /// Select an existing Pi SDK dist/index.js instead of installing one.
        #[arg(long)]
        sdk_entry: Option<PathBuf>,
        /// Use this existing Pi agent directory for credentials and models.
        #[arg(long, requires = "sdk_entry")]
        agent_dir: Option<PathBuf>,
        /// Reinstall the pinned managed SDK even when it is already ready.
        #[arg(long)]
        update: bool,
        /// Open the managed Pi CLI after setup so `/login` can configure auth.
        #[arg(long)]
        login: bool,
    },
    /// Print the concise current Glass Agent readiness state.
    Status,
    /// Execute one schema-validated call through the Glass agent-tool broker.
    #[command(hide = true)]
    Tool {
        call: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        allow_mutation: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Execute a one-use broker request from a private temporary file.
    #[command(hide = true)]
    ToolFile {
        path: PathBuf,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        allow_mutation: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Negotiate the Glass-owned harness protocol.
    Hello {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, value_enum, default_value_t = AgentHarness::Local)]
        harness: AgentHarness,
    },
    /// Run one bounded local prompt through the harness.
    Prompt {
        text: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, value_enum, default_value_t = AgentHarness::Local)]
        harness: AgentHarness,
    },
    /// Steer the active local harness request.
    Steer {
        text: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, value_enum, default_value_t = AgentHarness::Pi)]
        harness: AgentHarness,
    },
    /// Queue a follow-up through the native Pi SDK runtime.
    FollowUp {
        text: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// List models exposed by the native Pi SDK runtime.
    Models {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Select a Pi provider/model pair.
    SetModel {
        provider: String,
        model_id: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Set the Pi thinking level.
    Thinking {
        level: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Abort the active Pi request.
    Abort {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Start a fresh Pi session.
    NewSession {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AgentHarness {
    Local,
    Pi,
}

#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    Status,
    Inspect {
        record_id: String,
    },
    Explain {
        record_id: String,
    },
    Forget {
        record_id: String,
    },
    Export {
        output: Option<PathBuf>,
    },
    /// Remove stale, contradicted, and quarantined records.
    Prune,
    /// Re-read and validate the snapshot from disk.
    Reindex,
}

#[derive(Debug, Subcommand)]
pub enum SurfaceCommand {
    Inspect { input: PathBuf },
    Coverage { input: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum BackendCommand {
    Status { input: PathBuf },
    Capabilities { input: PathBuf },
    Test { input: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum ReplayCommand {
    Inspect {
        scenario: PathBuf,
        input: PathBuf,
    },
    Diff {
        scenario: PathBuf,
        before: PathBuf,
        after: PathBuf,
    },
    Attach {
        scenario: PathBuf,
        input: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum SnapshotCommand {
    Create,
    List,
    Inspect { snapshot_id: String },
    Diff { from: String, to: String },
    Purge,
}

#[derive(Debug, Subcommand)]
pub enum ResultCommand {
    /// Show a stored diagnostic artifact, optionally selecting one section.
    Show {
        result_id: String,
        #[arg(long)]
        section: Option<String>,
    },
    /// Purge artifacts older than a bounded duration such as 7d or 24h.
    Purge {
        #[arg(long = "older-than")]
        older_than: String,
    },
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum KnowledgeInvalidationState {
    Stale,
    Contradicted,
    Quarantined,
}

#[derive(Debug, Subcommand)]
pub enum WorkflowAuthoringCommand {
    /// Compile YAML or JSON source into canonical workflow JSON.
    Compile {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Format a YAML or JSON workflow as deterministic YAML.
    Format {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Show a redacted browser-free execution preview.
    Preview { input: PathBuf },
    /// Compare two workflow sources and print migration guidance.
    Diff { before: PathBuf, after: PathBuf },
    /// Import explicit semantic evidence into a reviewable draft.
    Record {
        /// JSON event envelope; omit to read stdin.
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Validate authoring source against the canonical workflow contract.
    Validate { input: PathBuf },
    /// Run static workflow diagnostics without starting a browser.
    Lint {
        input: PathBuf,
        #[arg(long)]
        warnings_as_errors: bool,
    },
    /// List or initialize one of the reviewable workflow starter templates.
    Templates {
        /// Optional template name; omit to list available templates.
        name: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Initialize one of the five reviewable issue 29 starter templates.
    Init {
        /// Template name: search, form-submit, paginated-extraction,
        /// authenticated-session, or dialog-and-download.
        name: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Validate strict Task Protocol JSON without starting Chrome.
    Validate {
        /// JSON file containing the authored task.
        input: PathBuf,
    },
    /// Compile strict Task Protocol JSON into a deterministic execution plan.
    Compile {
        /// JSON file containing the authored task.
        input: PathBuf,
        /// Stable Glass Web IR v1 JSON used as compilation evidence.
        ir: PathBuf,
        /// Optional output file for the canonical execution plan.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print a deterministic compilation explanation to stderr.
        #[arg(long)]
        explain: bool,
    },
    /// Execute a validated browser-backed Task Protocol task against the current browser.
    Execute {
        /// JSON file containing the authored task from the form, navigation, dialog,
        /// pagination, extraction, or field-read Task Protocol families.
        input: PathBuf,
        /// Revision from the caller's preceding semantic observation.
        #[arg(long)]
        expected_revision: u64,
        /// Confirm a task whose risk or ambiguity policy requires confirmation.
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum IrCommand {
    /// Validate one Glass Web IR v1 document without starting Chrome.
    Validate { input: PathBuf },
    /// Print a bounded summary of one validated Glass Web IR v1 document.
    Inspect { input: PathBuf },
    /// Compute a deterministic diff between two validated Web IR revisions.
    Diff {
        before: PathBuf,
        after: PathBuf,
        /// Print the bounded canonical summary instead of detailed changes.
        #[arg(long)]
        summary: bool,
    },
    /// Classify one entity's continuity across two validated Web IR revisions.
    Continuity {
        before: PathBuf,
        after: PathBuf,
        entity_id: String,
    },
    /// Print one validated Glass Web IR v1 document in canonical JSON form.
    Canonical { input: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon in the background.
    Start {
        #[arg(long)]
        socket: Option<PathBuf>,
        #[arg(long)]
        status: Option<PathBuf>,
    },
    /// Read the daemon status contract.
    Status {
        #[arg(long)]
        socket: Option<PathBuf>,
        #[arg(long)]
        status: Option<PathBuf>,
    },
    /// Stop the daemon recorded by the status contract.
    Stop {
        #[arg(long)]
        socket: Option<PathBuf>,
        #[arg(long)]
        status: Option<PathBuf>,
    },
    /// Check the daemon process, status, and local socket.
    Doctor {
        #[arg(long)]
        socket: Option<PathBuf>,
        #[arg(long)]
        status: Option<PathBuf>,
    },
    /// Read the bounded local daemon log tail.
    Logs {
        #[arg(long)]
        status: Option<PathBuf>,
    },
    /// Acknowledge that interrupted workflows were reconciled from checkpoints.
    AcknowledgeRecovery {
        #[arg(long)]
        status: Option<PathBuf>,
        /// Request ID for every recovery record reconciled from a checkpoint.
        #[arg(long = "request-id", required = true)]
        request_ids: Vec<String>,
    },
    /// Internal foreground server used by `daemon start`.
    #[command(hide = true)]
    Serve {
        #[arg(long)]
        socket: PathBuf,
        #[arg(long)]
        status: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum CertifyCommand {
    /// Run one scenario in a navigated browser fixture and emit evidence.
    Run {
        /// JSON scenario to execute.
        #[arg(long)]
        scenario: PathBuf,
        /// JSON fixture manifest used to bind controls and faults.
        #[arg(long)]
        fixture: PathBuf,
        /// Fixture URL to navigate before execution.
        #[arg(long)]
        url: String,
        /// Directory containing workflow sources referenced by the scenario.
        #[arg(long, default_value = ".")]
        workflow_root: PathBuf,
        /// Optional JSON object containing declared workflow inputs.
        #[arg(long)]
        inputs: Option<PathBuf>,
        /// Optional path for the redacted evidence bundle.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Expand a scenario into its manifest-bound execution plan.
    Plan {
        /// JSON scenario to plan.
        #[arg(long)]
        scenario: PathBuf,
        /// JSON fixture manifest used to bind controls and faults.
        #[arg(long)]
        fixture: PathBuf,
    },
    /// Evaluate a release-blocking reliability gate.
    Release {
        #[arg(long)]
        version: String,
        /// JSON array of validated reliability scenarios.
        #[arg(long)]
        scenarios: PathBuf,
        /// JSON array of scenario observations and oracle evidence.
        #[arg(long)]
        observations: PathBuf,
        /// Optional JSON array of redacted replay bundles to cross-check.
        #[arg(long)]
        replays: Option<PathBuf>,
    },
    /// Validate one redacted replay bundle against its versioned scenario.
    Replay {
        /// JSON scenario used to validate the replay binding.
        #[arg(long)]
        scenario: PathBuf,
        /// JSON replay bundle to validate.
        #[arg(long)]
        input: PathBuf,
    },
    /// Compare two redacted replay bundles for one scenario.
    ReplayDiff {
        /// JSON scenario used to validate both replay bindings.
        #[arg(long)]
        scenario: PathBuf,
        /// Baseline replay bundle.
        #[arg(long)]
        before: PathBuf,
        /// Candidate replay bundle.
        #[arg(long)]
        after: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum CheckpointCommand {
    Export,
    Import { input: Option<PathBuf> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_invocation_is_promptless_and_commandless() {
        let cli = Cli::try_parse_from(["glass"]).unwrap();

        assert!(cli.prompt.is_none());
        assert!(cli.command.is_none());
    }

    #[test]
    fn tui_is_discoverable_as_an_explicit_command() {
        assert!(matches!(
            Cli::try_parse_from(["glass", "tui"]).unwrap().command,
            Some(Commands::Tui)
        ));
    }

    #[test]
    fn browser_and_session_onboarding_commands_parse() {
        let browser = Cli::try_parse_from(["glass", "browser"]).unwrap();
        assert!(matches!(
            browser.command,
            Some(Commands::Browser { action: None })
        ));

        let session = Cli::try_parse_from(["glass", "--session", "work", "observe"]).unwrap();
        assert_eq!(session.session.as_deref(), Some("work"));
        assert!(matches!(session.command, Some(Commands::Observe { .. })));

        let start = Cli::try_parse_from(["glass", "session", "start", "work"]).unwrap();
        assert!(matches!(
            start.command,
            Some(Commands::Session {
                action: SessionCommand::Start { ref name }
            }) if name == "work"
        ));
    }
    #[test]
    fn update_options_are_explicit_and_browser_free() {
        let cli = Cli::try_parse_from([
            "glass",
            "update",
            "--dry-run",
            "--version",
            "0.3.4",
            "--force",
            "--registry",
            "internal",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Update {
                dry_run: true,
                version: Some(ref version),
                force: true,
                registry: Some(ref registry),
            }) if version == "0.3.4" && registry == "internal"
        ));
    }

    #[test]
    fn tui_layout_defaults_to_auto_and_accepts_mobile_override() {
        let default = Cli::try_parse_from(["glass", "tui"]).unwrap();
        assert!(!default.yolo);
        assert_eq!(default.tui_layout, TuiLayout::Auto);
        assert_eq!(default.tui_live, TuiLiveMode::Off);
        assert_eq!(default.tui_live_backend, TuiLiveBackend::Auto);
        assert_eq!(default.tui_live_quality, TuiLiveQuality::Balanced);
        assert_eq!(default.tui_live_fit, TuiLiveFit::Contain);

        let mobile = Cli::try_parse_from([
            "glass",
            "--tui-layout",
            "mobile",
            "--tui-live",
            "on",
            "--tui-live-backend",
            "ansi",
            "--tui-live-quality",
            "data",
            "--tui-live-fit",
            "cover",
            "tui",
        ])
        .unwrap();
        assert_eq!(mobile.tui_layout, TuiLayout::Mobile);
        assert_eq!(mobile.tui_live, TuiLiveMode::On);
        assert_eq!(mobile.tui_live_backend, TuiLiveBackend::Ansi);
        assert_eq!(mobile.tui_live_quality, TuiLiveQuality::Data);
        assert_eq!(mobile.tui_live_fit, TuiLiveFit::Cover);
    }

    #[test]
    fn yolo_is_an_explicit_global_opt_in() {
        assert!(
            Cli::try_parse_from(["glass", "--yolo", "tui"])
                .unwrap()
                .yolo
        );
        assert!(
            Cli::try_parse_from(["glass", "agent", "hello", "--yolo"])
                .unwrap()
                .yolo
        );
    }

    #[test]
    fn agent_readiness_commands_are_explicit() {
        assert!(matches!(
            Cli::try_parse_from(["glass", "agent", "doctor"])
                .unwrap()
                .command,
            Some(Commands::Agent {
                action: AgentCommand::Doctor
            })
        ));
        assert!(matches!(
            Cli::try_parse_from([
                "glass",
                "agent",
                "setup",
                "--sdk-entry",
                "/opt/pi/dist/index.js",
                "--agent-dir",
                "/opt/pi/config",
                "--update"
            ])
            .unwrap()
            .command,
            Some(Commands::Agent {
                action: AgentCommand::Setup {
                    sdk_entry: Some(_),
                    agent_dir: Some(_),
                    update: true,
                    login: false,
                }
            })
        ));
    }

    #[test]
    fn observation_and_human_interaction_are_defaults() {
        let cli = Cli::try_parse_from(["glass", "observe"]).unwrap();

        assert_eq!(cli.interaction, InteractionMode::Human);
        assert!(matches!(
            cli.command,
            Some(Commands::Observe {
                deep_dom: false,
                screenshot: false,
                form_values: false,
                semantic_level: None,
                region: None,
            })
        ));
    }

    #[test]
    fn screenshot_and_fast_interaction_require_explicit_flags() {
        let cli =
            Cli::try_parse_from(["glass", "--interaction", "fast", "observe", "--screenshot"])
                .unwrap();

        assert_eq!(cli.interaction, InteractionMode::Fast);
        assert!(matches!(
            cli.command,
            Some(Commands::Observe {
                deep_dom: false,
                screenshot: true,
                form_values: false,
                semantic_level: None,
                region: None,
            })
        ));
    }

    #[test]
    fn deep_dom_requires_an_explicit_observation_flag() {
        let cli = Cli::try_parse_from(["glass", "observe", "--deep-dom"]).unwrap();

        assert!(matches!(
            cli.command,
            Some(Commands::Observe {
                deep_dom: true,
                screenshot: false,
                form_values: false,
                semantic_level: None,
                region: None,
            })
        ));
    }

    #[test]
    fn semantic_observation_level_and_region_are_explicit() {
        let cli = Cli::try_parse_from([
            "glass",
            "observe",
            "--level",
            "interactive",
            "--region",
            "region_main",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Observe {
                semantic_level: Some(level),
                region: Some(region),
                ..
            }) if level == "interactive" && region == "region_main"
        ));

        assert!(Cli::try_parse_from(["glass", "observe", "--level", "verbose"]).is_err());
    }

    #[test]
    fn screenshot_remains_a_separate_explicit_command() {
        let cli = Cli::try_parse_from(["glass", "screenshot", "--output", "page.png"]).unwrap();

        assert!(matches!(
            cli.command,
            Some(Commands::Screenshot { output, .. }) if output == "page.png"
        ));
    }

    #[test]
    fn double_click_is_an_explicit_action_command() {
        let cli = Cli::try_parse_from(["glass", "double-click", "r7:b42"]).unwrap();

        assert!(matches!(
            cli.command,
            Some(Commands::DoubleClick { target, .. }) if target == "r7:b42"
        ));
    }

    #[test]
    fn click_expect_popup_is_an_explicit_action_command() {
        let cli = Cli::try_parse_from(["glass", "click-expect-popup", "css=#popup"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::ClickExpectPopup { target, .. }) if target == "css=#popup"
        ));
    }

    #[test]
    fn wait_has_an_explicit_condition_and_bounded_default() {
        let cli = Cli::try_parse_from(["glass", "wait", "text=Ready"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Wait { condition, timeout_ms: 10_000 }) if condition == "text=Ready"
        ));
    }

    #[test]
    fn topology_commands_are_explicit() {
        assert!(matches!(
            Cli::try_parse_from(["glass", "targets"]).unwrap().command,
            Some(Commands::Targets)
        ));
        let cli = Cli::try_parse_from([
            "glass",
            "--target-id",
            "page-1",
            "--frame-id",
            "frame-1",
            "evaluate",
            "document.title",
        ])
        .unwrap();
        assert_eq!(cli.target_id.as_deref(), Some("page-1"));
        assert_eq!(cli.frame_id.as_deref(), Some("frame-1"));
        assert!(matches!(
            Cli::try_parse_from(["glass", "select-frame", "frame-1"])
                .unwrap()
                .command,
            Some(Commands::SelectFrame { id }) if id == "frame-1"
        ));
    }

    #[test]
    fn archive_targets_accepts_optional_workspace_output() {
        let stdout = Cli::try_parse_from(["glass", "archive-targets"])
            .unwrap()
            .command;
        assert!(matches!(
            stdout,
            Some(Commands::ArchiveTargets { output: None })
        ));
        let file = Cli::try_parse_from(["glass", "archive-targets", "--output", "targets.json"])
            .unwrap()
            .command;
        assert!(matches!(
            file,
            Some(Commands::ArchiveTargets {
                output: Some(path)
            }) if path == std::path::Path::new("targets.json")
        ));
    }

    #[test]
    fn complete_input_commands_are_explicit() {
        assert!(matches!(
            Cli::try_parse_from(["glass", "drag", "css=#from", "css=#to"])
                .unwrap()
                .command,
            Some(Commands::Drag { source, destination, .. }) if source == "css=#from" && destination == "css=#to"
        ));
        assert!(matches!(
            Cli::try_parse_from(["glass", "shortcut", "Control+A"])
                .unwrap()
                .command,
            Some(Commands::Shortcut { shortcut, .. }) if shortcut == "Control+A"
        ));
        assert!(matches!(
            Cli::try_parse_from([
                "glass",
                "fill-form",
                "--fields",
                "[]",
                "--expected-revision",
                "7"
            ])
            .unwrap()
            .command,
            Some(Commands::FillForm {
                fields,
                expected_revision: Some(7)
            }) if fields == "[]"
        ));
    }

    #[test]
    fn rejects_unknown_interaction_modes() {
        assert!(Cli::try_parse_from(["glass", "--interaction", "instant", "observe"]).is_err());
    }

    #[test]
    fn attach_and_target_id_are_explicit_global_options() {
        let cli = Cli::try_parse_from([
            "glass",
            "--attach",
            "--port",
            "9333",
            "--target-id",
            "page-2",
            "observe",
        ])
        .unwrap();

        assert!(cli.attach);
        assert_eq!(cli.port, 9333);
        assert_eq!(cli.target_id.as_deref(), Some("page-2"));
    }

    #[test]
    fn workflow_command_accepts_optional_json_input() {
        let cli = Cli::try_parse_from(["glass", "workflow", "workflow.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Workflow {
                action: None,
                input: Some(path)
            })
                if path.as_os_str() == "workflow.json"
        ));
        let cli = Cli::try_parse_from(["glass", "workflow", "validate", "workflow.yaml"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Workflow {
                action: Some(WorkflowAuthoringCommand::Validate { input }),
                input: None,
            }) if input.as_os_str() == "workflow.yaml"
        ));
        let cli = Cli::try_parse_from(["glass", "workflow", "preview", "workflow.yaml"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Workflow {
                action: Some(WorkflowAuthoringCommand::Preview { input }),
                input: None,
            }) if input.as_os_str() == "workflow.yaml"
        ));
        let cli = Cli::try_parse_from([
            "glass",
            "workflow",
            "record",
            "--input",
            "events.json",
            "--output",
            "draft.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Workflow {
                action: Some(WorkflowAuthoringCommand::Record { input: Some(input), output: Some(output) }),
                input: None,
            }) if input.as_os_str() == "events.json" && output.as_os_str() == "draft.json"
        ));
    }

    #[test]
    fn certify_release_command_accepts_versioned_evidence_paths() {
        let cli = Cli::try_parse_from([
            "glass",
            "certify",
            "release",
            "--version",
            "0.2.0",
            "--scenarios",
            "scenarios.json",
            "--observations",
            "observations.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Certify {
                action: CertifyCommand::Release {
                    version,
                    scenarios,
                    observations,
                    replays: None,
                },
            }) if version == "0.2.0"
                && scenarios.as_os_str() == "scenarios.json"
                && observations.as_os_str() == "observations.json"
        ));
    }

    #[test]
    fn certify_plan_command_accepts_scenario_and_fixture_paths() {
        let cli = Cli::try_parse_from([
            "glass",
            "certify",
            "plan",
            "--scenario",
            "scenario.json",
            "--fixture",
            "fixture.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Certify {
                action: CertifyCommand::Plan { scenario, fixture },
            }) if scenario.as_os_str() == "scenario.json" && fixture.as_os_str() == "fixture.json"
        ));
    }

    #[test]
    fn certify_replay_command_accepts_scenario_and_bundle_paths() {
        let cli = Cli::try_parse_from([
            "glass",
            "certify",
            "replay",
            "--scenario",
            "scenario.json",
            "--input",
            "replay.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Certify {
                action: CertifyCommand::Replay { scenario, input },
            }) if scenario.as_os_str() == "scenario.json" && input.as_os_str() == "replay.json"
        ));
    }

    #[test]
    fn certify_replay_diff_command_accepts_two_bundle_paths() {
        let cli = Cli::try_parse_from([
            "glass",
            "certify",
            "replay-diff",
            "--scenario",
            "scenario.json",
            "--before",
            "before.json",
            "--after",
            "after.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Certify {
                action: CertifyCommand::ReplayDiff { scenario, before, after },
            }) if scenario.as_os_str() == "scenario.json"
                && before.as_os_str() == "before.json"
                && after.as_os_str() == "after.json"
        ));
    }

    #[test]
    fn workflow_resume_command_accepts_checkpoint_and_inputs() {
        let cli = Cli::try_parse_from([
            "glass",
            "workflow-resume",
            "workflow.json",
            "checkpoint.json",
            "--inputs",
            "inputs.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::WorkflowResume {
                workflow,
                checkpoint,
                inputs: Some(inputs)
            }) if workflow.as_os_str() == "workflow.json"
                && checkpoint.as_os_str() == "checkpoint.json"
                && inputs.as_os_str() == "inputs.json"
        ));
    }

    #[test]
    fn resolve_intent_command_accepts_optional_json_input() {
        let cli = Cli::try_parse_from(["glass", "resolve-intent", "intent.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::ResolveIntent { input: Some(path) })
                if path.as_os_str() == "intent.json"
        ));
        assert!(Cli::try_parse_from(["glass", "resolve-intent"]).is_ok());
    }

    #[test]
    fn execute_intent_command_accepts_optional_json_input() {
        let cli = Cli::try_parse_from(["glass", "execute-intent", "intent.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::ExecuteIntent { input: Some(path) })
                if path.as_os_str() == "intent.json"
        ));
        assert!(Cli::try_parse_from(["glass", "execute-intent"]).is_ok());
    }

    #[test]
    fn reliability_run_command_requires_fixture_url_and_sources() {
        let cli = Cli::try_parse_from([
            "glass",
            "certify",
            "run",
            "--scenario",
            "scenario.json",
            "--fixture",
            "fixture.json",
            "--url",
            "http://127.0.0.1:8000/fixture.html",
            "--workflow-root",
            "fixtures",
            "--inputs",
            "inputs.json",
            "--output",
            "evidence.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Certify {
                action: CertifyCommand::Run {
                    scenario,
                    fixture,
                    url,
                    workflow_root,
                    inputs: Some(inputs),
                    output: Some(output),
                }
            }) if scenario.as_os_str() == "scenario.json"
                && fixture.as_os_str() == "fixture.json"
                && url == "http://127.0.0.1:8000/fixture.html"
                && workflow_root.as_os_str() == "fixtures"
                && inputs.as_os_str() == "inputs.json"
                && output.as_os_str() == "evidence.json"
        ));
    }

    #[test]
    fn capabilities_command_is_explicitly_offline() {
        let cli = Cli::try_parse_from(["glass", "capabilities"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Capabilities)));
    }

    #[test]
    fn experimental_extensions_require_an_explicit_global_opt_in() {
        let cli =
            Cli::try_parse_from(["glass", "--experimental-extensions", "capabilities"]).unwrap();
        assert!(cli.experimental_extensions);
    }

    #[test]
    fn daemon_lifecycle_commands_accept_explicit_local_paths() {
        let cli = Cli::try_parse_from([
            "glass",
            "daemon",
            "start",
            "--socket",
            "/tmp/glass.sock",
            "--status",
            "/tmp/glass.json",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Daemon {
                action: DaemonCommand::Start { socket: Some(socket), status: Some(status) }
            }) if socket.as_os_str() == "/tmp/glass.sock"
                && status.as_os_str() == "/tmp/glass.json"
        ));
    }

    #[test]
    fn doctor_command_is_available_without_starting_a_browser() {
        let cli = Cli::try_parse_from(["glass", "doctor"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Doctor { json: false })
        ));
    }

    #[test]
    fn knowledge_management_commands_parse_without_browser_startup() {
        let cli = Cli::try_parse_from(["glass", "knowledge", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Knowledge {
                action: KnowledgeCommand::List
            })
        ));
        let cli = Cli::try_parse_from(["glass", "knowledge", "explain", "record-1"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Knowledge {
                action: KnowledgeCommand::Explain { .. }
            })
        ));
        let cli = Cli::try_parse_from([
            "glass",
            "--knowledge-store",
            "knowledge.json",
            "knowledge",
            "invalidate",
            "record-1",
            "stale",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Knowledge {
                action: KnowledgeCommand::Invalidate { .. }
            })
        ));
    }
    #[test]
    fn task_compile_command_is_explicitly_offline() {
        use clap::CommandFactory;

        let cli = Cli::try_parse_from([
            "glass",
            "task",
            "compile",
            "task.json",
            "web-ir.json",
            "--output",
            "plan.json",
            "--explain",
        ])
        .unwrap();
        assert!(Cli::command().find_subcommand("task").is_some());
        assert!(matches!(
            cli.command,
            Some(Commands::Task {
                action: TaskCommand::Compile {
                    input,
                    ir,
                    output: Some(output),
                    explain
                }
            }) if input.as_os_str() == "task.json"
                && ir.as_os_str() == "web-ir.json"
                && output.as_os_str() == "plan.json"
                && explain
        ));
    }

    #[test]
    fn task_execute_help_advertises_supported_families() {
        use clap::CommandFactory;

        let mut command = Cli::command();
        let execute = command
            .find_subcommand_mut("task")
            .and_then(|task| task.find_subcommand_mut("execute"))
            .expect("task execute command should be present");
        let help = execute.render_help().to_string();
        let normalized = help.split_whitespace().collect::<Vec<_>>().join(" ");

        assert!(normalized.contains(
            "form, navigation, dialog, pagination, extraction, or field-read Task Protocol families"
        ));
    }
    #[test]
    fn task_validate_command_is_explicitly_offline() {
        let cli = Cli::try_parse_from(["glass", "task", "validate", "task.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Task {
                action: TaskCommand::Validate { input }
            }) if input.as_os_str() == "task.json"
        ));
    }
    #[test]
    fn ir_commands_are_explicitly_offline() {
        let cli = Cli::try_parse_from(["glass", "ir", "inspect", "draft.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Ir {
                action: IrCommand::Inspect { input }
            }) if input.as_os_str() == "draft.json"
        ));

        let cli =
            Cli::try_parse_from(["glass", "ir", "diff", "before.json", "after.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Ir {
                action: IrCommand::Diff {
                    before,
                    after,
                    summary
                }
            }) if before.as_os_str() == "before.json"
                && after.as_os_str() == "after.json"
                && !summary
        ));

        let cli = Cli::try_parse_from([
            "glass",
            "ir",
            "diff",
            "before.json",
            "after.json",
            "--summary",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Ir {
                action: IrCommand::Diff {
                    before,
                    after,
                    summary
                }
            }) if before.as_os_str() == "before.json"
                && after.as_os_str() == "after.json"
                && summary
        ));

        let cli = Cli::try_parse_from([
            "glass",
            "ir",
            "continuity",
            "before.json",
            "after.json",
            "field-1",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Ir {
                action: IrCommand::Continuity {
                    before,
                    after,
                    entity_id
                }
            }) if before.as_os_str() == "before.json"
                && after.as_os_str() == "after.json"
                && entity_id == "field-1"
        ));

        let cli = Cli::try_parse_from(["glass", "ir", "validate", "draft.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Ir {
                action: IrCommand::Validate { input }
            }) if input.as_os_str() == "draft.json"
        ));

        let cli = Cli::try_parse_from(["glass", "ir", "canonical", "draft.json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Ir {
                action: IrCommand::Canonical { input }
            }) if input.as_os_str() == "draft.json"
        ));
    }
    #[test]
    fn browser_first_and_persistent_session_commands_parse() {
        let cli = Cli::try_parse_from(["glass", "browser", "tui"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Browser {
                action: Some(BrowserCommand::Tui)
            })
        ));

        let cli = Cli::try_parse_from(["glass", "--session", "work", "observe"]).unwrap();
        assert_eq!(cli.session.as_deref(), Some("work"));
        assert!(matches!(cli.command, Some(Commands::Observe { .. })));

        let cli = Cli::try_parse_from(["glass", "session", "status", "work"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Session {
                action: SessionCommand::Status { name }
            }) if name == "work"
        ));
    }
}

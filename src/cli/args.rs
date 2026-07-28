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

/// Top-level CLI configuration parsed from command-line arguments.
///
/// Wraps clap-derived flags for policy, browser selection, session options,
/// and the subcommand to execute.
#[derive(Debug, Parser)]
#[command(
    name = "glass",
    version,
    about = "Lightweight local-first browser agent using raw Chrome DevTools Protocol"
)]
pub struct Cli {
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

    /// Override the per-profile persistent knowledge snapshot path.
    #[arg(long, global = true)]
    pub knowledge_store: Option<PathBuf>,

    /// One-shot prompt, for example: `navigate to https://example.com`.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
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

    /// Execute a validated workflow document from JSON or stdin.
    Workflow {
        /// Offline authoring operation. Omit to execute the workflow.
        #[command(subcommand)]
        action: Option<WorkflowAuthoringCommand>,
        /// JSON file containing `{ "workflow": ..., "inputs": ... }`.
        input: Option<PathBuf>,
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

    /// Read text from the system clipboard.
    ClipboardRead,

    /// Write text to the system clipboard.
    ClipboardWrite { text: String },

    /// Launch the interactive TUI.
    Tui,
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
}

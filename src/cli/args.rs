//! CLI argument definitions (clap).
//!
//! Defines the top-level `Cli` struct and all subcommands for one-shot
//! browser operations, profile management, and server modes.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::browser::policy::{PolicyCapability, PolicyPreset};
use crate::browser::session::{InteractionMode, PreflightAction, VisualClip, VisualFormat};

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

    /// Path to a Chrome/Chromium binary.
    #[arg(long = "chrome-path", alias = "chrome", global = true)]
    pub chrome_path: Option<PathBuf>,

    /// Run the MCP server over stdio.
    #[arg(long)]
    pub mcp: bool,

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

    /// List or manage saved profiles.
    Profiles {
        #[command(subcommand)]
        action: Option<ProfileCommand>,
    },

    /// Delete a saved profile.
    DeleteProfile { name: String },

    /// Navigate to a URL.
    Navigate {
        url: String,
        #[arg(long, default_value_t = 20_000)]
        timeout_ms: u64,
    },

    /// Click an element by an explicit ref/name/role/text/CSS/ordinal locator.
    Click { target: String },

    /// Resolve a target and report clickability without performing an action.
    Preflight {
        target: String,
        #[arg(long, value_enum, default_value_t = PreflightAction::Click)]
        action: PreflightAction,
    },

    /// Click exact viewport coordinates for canvas/map surfaces.
    ClickAt { x: f64, y: f64 },

    /// Click an element expected to open exactly one causally verified popup.
    ClickExpectPopup { target: String },

    /// Double-click an element by an explicit ref/name/role/text/CSS/ordinal locator.
    DoubleClick { target: String },

    /// Move the pointer over an element without clicking.
    Hover { target: String },

    /// Drag one element to another uniquely resolved element.
    Drag { source: String, destination: String },

    /// Type text into the focused element, optionally clicking a target first.
    Type {
        text: String,
        #[arg(long)]
        target: Option<String>,
    },

    /// Dispatch one complete key press.
    Key { key: String },

    /// Dispatch only a key-down event.
    KeyDown { key: String },

    /// Dispatch only a key-up event.
    KeyUp { key: String },

    /// Dispatch a modifier shortcut such as Control+A.
    Shortcut { shortcut: String },

    /// Clear an editable element.
    Clear { target: String },

    /// Ensure a checkbox or radio is checked.
    Check { target: String },

    /// Ensure a checkbox is unchecked.
    Uncheck { target: String },

    /// Select one exact option value.
    Select { target: String, value: String },

    /// Set a bounded list of regular files on one file input.
    Upload {
        target: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
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
    },

    /// Scroll the page by CSS pixels.
    Scroll {
        #[arg(long, default_value_t = 0.0)]
        dx: f64,
        #[arg(long, default_value_t = 600.0)]
        dy: f64,
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
    },

    /// Execute a bounded typed batch from a JSON array or stdin.
    Batch {
        /// JSON file containing the batch steps; omit to read stdin.
        input: Option<PathBuf>,
        #[arg(long)]
        atomic: bool,
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

#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    List,
    Create { name: String },
    Delete { name: String },
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
                form_values: false
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
                form_values: false
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
                form_values: false
            })
        ));
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
            Some(Commands::DoubleClick { target }) if target == "r7:b42"
        ));
    }

    #[test]
    fn click_expect_popup_is_an_explicit_action_command() {
        let cli = Cli::try_parse_from(["glass", "click-expect-popup", "css=#popup"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::ClickExpectPopup { target }) if target == "css=#popup"
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
            Some(Commands::Drag { source, destination }) if source == "css=#from" && destination == "css=#to"
        ));
        assert!(matches!(
            Cli::try_parse_from(["glass", "shortcut", "Control+A"])
                .unwrap()
                .command,
            Some(Commands::Shortcut { shortcut }) if shortcut == "Control+A"
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
}

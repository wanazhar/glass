use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::browser::session::InteractionMode;

#[derive(Debug, Parser)]
#[command(
    name = "glass",
    version,
    about = "Lightweight local-first browser agent using raw Chrome DevTools Protocol"
)]
pub struct Cli {
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

    /// Chrome remote debugging port.
    #[arg(long, global = true, default_value_t = 9222)]
    pub port: u16,

    /// Show the browser window instead of using headless mode.
    #[arg(long, global = true)]
    pub headed: bool,

    /// Pointer behavior for click actions.
    #[arg(long, global = true, value_enum, default_value_t = InteractionMode::Human)]
    pub interaction: InteractionMode,

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
    InstallChromium,

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

    /// Double-click an element by an explicit ref/name/role/text/CSS/ordinal locator.
    DoubleClick { target: String },

    /// Type text into the focused element, optionally clicking a target first.
    Type {
        text: String,
        #[arg(long)]
        target: Option<String>,
    },

    /// Capture a PNG screenshot.
    Screenshot {
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,
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

    /// Evaluate JavaScript in the current page.
    Evaluate { expression: String },

    /// Launch the interactive TUI.
    Tui,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    List,
    Create { name: String },
    Delete { name: String },
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
                screenshot: false
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
                screenshot: true
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
                screenshot: false
            })
        ));
    }

    #[test]
    fn screenshot_remains_a_separate_explicit_command() {
        let cli = Cli::try_parse_from(["glass", "screenshot", "--output", "page.png"]).unwrap();

        assert!(matches!(
            cli.command,
            Some(Commands::Screenshot { output }) if output == "page.png"
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
    fn wait_has_an_explicit_condition_and_bounded_default() {
        let cli = Cli::try_parse_from(["glass", "wait", "text=Ready"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Wait { condition, timeout_ms: 10_000 }) if condition == "text=Ready"
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

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

    /// One-shot prompt, for example: "navigate to https://example.com".
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
    Navigate { url: String },

    /// Click an element by accessibility reference, name, or CSS selector.
    Click { target: String },

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

    /// Print the accessibility snapshot.
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
    fn rejects_unknown_interaction_modes() {
        assert!(Cli::try_parse_from(["glass", "--interaction", "instant", "observe"]).is_err());
    }
}

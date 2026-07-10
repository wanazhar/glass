use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "glass",
    version,
    about = "Glass — lightweight local-first browser agent (raw CDP)",
    long_about = "Drive Chrome like a human via CDP. TUI by default, one-shot CLI prompts, or MCP server mode."
)]
pub struct Cli {
    /// Named session profile (cookies + chromium user-data-dir)
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// No persistence (ephemeral profile)
    #[arg(long, global = true, default_value_t = false)]
    pub incognito: bool,

    /// Chrome remote debugging port
    #[arg(long, global = true, default_value_t = 9222)]
    pub port: u16,

    /// Run headed (show browser window). Default is headless.
    #[arg(long, global = true, default_value_t = false)]
    pub headed: bool,

    /// Path to Chrome/Chromium binary
    #[arg(long, global = true)]
    pub chrome: Option<String>,

    /// Run as MCP server (stdio)
    #[arg(long, default_value_t = false)]
    pub mcp: bool,

    /// One-shot natural language / command prompt (CLI mode)
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Download a managed Chromium build
    InstallChromium,

    /// List saved profiles
    Profiles {
        #[command(subcommand)]
        action: Option<ProfileCmd>,
    },

    /// Navigate to a URL
    Navigate {
        url: String,
    },

    /// Click an element by accessibility ref/index/name
    Click {
        /// Element index, or substring of accessible name
        target: String,
    },

    /// Type text into the focused / targeted field
    Type {
        text: String,
        /// Optional element ref to click first
        #[arg(long)]
        target: Option<String>,
    },

    /// Capture a PNG screenshot
    Screenshot {
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,
    },

    /// Print page text content
    Text,

    /// Dump interactive accessibility snapshot
    Dom,

    /// Scroll the page
    Scroll {
        #[arg(long, default_value_t = 0.0)]
        dx: f64,
        #[arg(long, default_value_t = 600.0)]
        dy: f64,
    },

    /// Launch interactive TUI
    Tui,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCmd {
    List,
    Create { name: String },
    Delete { name: String },
}

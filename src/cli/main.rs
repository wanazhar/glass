use clap::{Parser, Subcommand};
use std::path::PathBuf;


#[derive(Parser)]
#[command(name = "glass", version, about = "Lightweight browser agent for AI")]
pub struct Cli {
    /// One-shot command: "glass navigate to https://example.com"
    pub prompt: Option<String>,

    /// Run as MCP server
    #[arg(long)]
    pub mcp: bool,

    /// Named profile for cookies/persistence
    #[arg(long, default_value = "default")]
    pub profile: String,

    /// Incognito mode (no persistence)
    #[arg(long)]
    pub incognito: bool,

    /// Chrome debugging port
    #[arg(long, default_value = "9222")]
    pub port: u16,

    /// Custom Chrome binary path
    #[arg(long)]
    pub chrome_path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Download and install Chromium
    InstallChromium,
    /// List saved profiles
    Profiles,
    /// Delete a profile
    DeleteProfile {
        /// Profile name to delete
        name: String,
    },
}

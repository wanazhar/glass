//! Terminal-native project development runtime.
//!
//! This module owns the local development contracts used by the CLI, MCP, TUI,
//! and embedded harness. Browser authority remains in the existing browser
//! runtime: project state can coordinate a live application without duplicating
//! browser control logic.

pub mod agent;
pub mod diff;
pub mod events;
pub mod graph;
pub mod process;
pub mod project;

use std::{fmt, io, path::PathBuf};

pub use agent::{HarnessEvent, HarnessRequest, LocalHarness, ToolCall};
pub use diff::{ProjectDiff, build_diff};
pub use events::{Actor, ActorKind, DevelopmentEvent, DevelopmentEventKind, Timeline};
pub use graph::{DevelopmentGraph, LinkEvidence, LinkProvenance, RuntimeLink, SourceLocation};
pub use process::{ProcessManager, ProcessSnapshot, ProcessState};
pub use project::{
    CommandConfig, EditorBuffer, FileEntry, FileKind, GlassProjectConfig, ProjectDetection,
    ProjectWorkspace, detect_project,
};

pub const DEVELOPMENT_SCHEMA_VERSION: &str = "glass.development.v1";
pub const MAX_FILE_BYTES: usize = 512 * 1024;
pub const MAX_BUFFER_BYTES: usize = 1024 * 1024;
pub const MAX_FILE_ENTRIES: usize = 2_048;
pub const MAX_TIMELINE_EVENTS: usize = 512;
pub const MAX_PROCESS_OUTPUT_BYTES: usize = 32 * 1024;

pub type DevelopmentResult<T> = Result<T, DevelopmentError>;

#[derive(Debug)]
pub enum DevelopmentError {
    Io(io::Error),
    InvalidInput(String),
    PathOutsideWorkspace(PathBuf),
    NotFound(String),
    Config(String),
    Process(String),
    Serialization(String),
}

impl fmt::Display for DevelopmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "development I/O error: {error}"),
            Self::InvalidInput(message) => {
                write!(formatter, "invalid development input: {message}")
            }
            Self::PathOutsideWorkspace(path) => write!(
                formatter,
                "path escapes the project workspace: {}",
                path.display()
            ),
            Self::NotFound(value) => write!(formatter, "development resource not found: {value}"),
            Self::Config(message) => write!(formatter, "invalid glass.toml: {message}"),
            Self::Process(message) => write!(formatter, "development process error: {message}"),
            Self::Serialization(message) => {
                write!(formatter, "development serialization error: {message}")
            }
        }
    }
}

impl std::error::Error for DevelopmentError {}

impl From<io::Error> for DevelopmentError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DevelopmentError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}

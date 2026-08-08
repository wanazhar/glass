//! Terminal-native project development runtime.
//!
//! This module owns the local development contracts used by the CLI, MCP, TUI,
//! and embedded harness. Browser authority remains in the existing browser
//! runtime: project state can coordinate a live application without duplicating
//! browser control logic.

pub mod agent;
pub mod cockpit;
pub mod collaboration;
pub mod debug;
pub mod diff;
pub mod editor;
pub mod events;
pub mod experiment;
pub mod graph;
pub mod language;
pub mod neovim;
#[cfg(feature = "development-runtime")]
pub mod process;
#[cfg(not(feature = "development-runtime"))]
#[path = "process_disabled.rs"]
pub mod process;
pub mod project;
pub mod replay;
pub mod search;

use std::{fmt, io, path::PathBuf};

pub use agent::{
    AgentContextPacket, HarnessEvent, HarnessRequest, LocalHarness, PiHarness, ToolCall,
    ToolDescriptor, ToolRegistry, resolve_context,
};
pub use cockpit::{
    AttentionItem, AttentionState, ReconnectCapsule, ReconnectCapsuleStore,
    ResidentDevelopmentSessions, VerificationCard, VerificationCheck, attention_inbox,
};
pub use collaboration::{CollaborationBus, CollaborationEvent, EditAccess, EditClaim};
pub use debug::{
    SemanticBreakpoint, SemanticBreakpointHit, SemanticEntityState, SemanticSnapshot,
    evaluate_breakpoints,
};
pub use diff::{ProjectDiff, build_diff};
pub use editor::{
    FoldRegion, SyntaxKind, SyntaxSpan, TextPosition, TextSelection, fold_regions,
    matching_bracket, syntax_spans,
};
pub use events::{
    Actor, ActorAuthority, ActorConnection, ActorKind, DevelopmentEvent, DevelopmentEventKind,
    DevelopmentEventPage, Timeline,
};
pub use experiment::{
    ExperimentComparison, ExperimentEvidence, ExperimentManager, ExperimentWorkspace,
};
pub use graph::{DevelopmentGraph, LinkEvidence, LinkProvenance, RuntimeLink, SourceLocation};
pub use language::{DiagnosticPosition, LanguageDiagnostic, LspClient};
pub use neovim::{NeovimCapability, probe_neovim, start_neovim};
pub use process::{ProcessHealth, ProcessManager, ProcessSnapshot, ProcessState};
pub use project::{
    CommandConfig, EditorBuffer, FileEntry, FileKind, GlassProjectConfig, ProjectDetection,
    ProjectWorkspace, detect_project,
};
pub use replay::{DevelopmentRevision, ReplayWindow, replay};
pub use search::{SearchHit, SearchKind, fuzzy_score, rank};

pub const DEVELOPMENT_SCHEMA_VERSION: &str = "glass.development.v1";
pub const DEVELOPMENT_EVENT_SCHEMA_VERSION: u32 = 1;
pub const DEVELOPMENT_COCKPIT_SCHEMA_VERSION: u32 = 1;
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
    Conflict(String),
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
            Self::Conflict(message) => write!(formatter, "development conflict: {message}"),
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

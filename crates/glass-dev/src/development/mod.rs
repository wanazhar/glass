//! Terminal-native project development runtime.
//!
//! This module owns the local development contracts used by the CLI, MCP, TUI,
//! and embedded harness. Browser authority remains in the existing browser
//! runtime: project state can coordinate a live application without duplicating
//! browser control logic.
//!
//! Glass Dev directly owns the concrete project, editor, PTY, language-server,
//! graph, replay, Neovim, experiment, and agent contracts. There is no browser
//! feature bridge or disabled compatibility implementation.
//!
//! Project paths are resolved beneath one canonical root; reads, output tails,
//! event logs, and retained buffers have hard bounds. Writes are atomic and
//! actor-attributed. Runtime claims such as live-update provenance remain
//! pending until explicit source/runtime evidence or browser revision evidence
//! supports them. A project session never implies browser mutation authority.

/// Resident agent and harness tool contracts.
pub mod agent;
/// Development cockpit and attention projections.
pub mod cockpit;
/// Collaboration bus and edit claims.
pub mod collaboration;
/// Semantic debugging and breakpoint evaluation.
pub mod debug;
/// Project diff construction and summaries.
pub mod diff;
/// Low-level bounded editor helpers; the TUI renderer is private and separate.
pub mod editor;
/// Durable actor-attributed development events.
pub mod events;
/// Experiment workspace and evidence support.
pub mod experiment;
/// Source/runtime development graph contracts.
pub mod graph;
/// Language documents, diagnostics, and LSP clients.
pub mod language;
/// Neovim capability probing and startup.
pub mod neovim;
/// Bounded process management and snapshots.
pub mod process;
/// Project files, commands, configuration, and detection.
pub mod project;
/// Remote-view state and input/frame types.
pub mod remote_view;
/// Revision identifiers and bounded replay.
pub mod replay;
/// Search hit types and ranking helpers.
pub mod search;

/// Agent authority, harness handoff, and tool registry contracts.
pub use agent::{
    AgentAuthorityContext, AgentContextPacket, AgentToolGateway, BrowserAgentContext, HarnessEvent,
    HarnessRequest, LocalHarness, PiHarness, PiHarnessOptions, ToolAuthorization, ToolCall,
    ToolDescriptor, ToolRegistry, resolve_context, resolve_context_with_browser,
};
/// Cockpit attention, verification, and reconnect projections.
pub use cockpit::{
    AttentionItem, AttentionState, LocalCockpit, ReconnectCapsule, ReconnectCapsuleStore,
    ResidentDevelopmentSessions, VerificationCard, VerificationCheck, attention_inbox,
};
/// Collaboration events and edit-access claims.
pub use collaboration::{CollaborationBus, CollaborationEvent, EditAccess, EditClaim};
/// Semantic breakpoint and snapshot types.
pub use debug::{
    SemanticBreakpoint, SemanticBreakpointHit, SemanticEntityState, SemanticSnapshot,
    evaluate_breakpoints,
};
/// Project diff model and builder.
pub use diff::{ProjectDiff, build_diff};
/// Low-level editor positions, syntax spans, folds, and matching helpers.
pub use editor::{
    FoldRegion, SyntaxKind, SyntaxSpan, TextPosition, TextSelection, fold_regions,
    matching_bracket, syntax_spans,
};
/// Actor, authority, timeline, and serialized development events.
pub use events::{
    Actor, ActorAuthority, ActorConnection, ActorKind, DevelopmentEvent, DevelopmentEventKind,
    DevelopmentEventPage, Timeline,
};
/// Experiment comparison, evidence, and workspace management.
pub use experiment::{
    ExperimentComparison, ExperimentEvidence, ExperimentManager, ExperimentWorkspace,
};
/// Development graph nodes, links, provenance, and source locations.
pub use graph::{DevelopmentGraph, LinkEvidence, LinkProvenance, RuntimeLink, SourceLocation};
/// Language documents, diagnostics, responses, and client.
pub use language::{
    DiagnosticPosition, LanguageDiagnostic, LanguageDocument, LanguageResponse, LspClient,
};
/// Neovim capability probe and startup helpers.
pub use neovim::{NeovimCapability, probe_neovim, start_neovim};
/// Process health, state, and bounded snapshots.
pub use process::{ProcessHealth, ProcessManager, ProcessSnapshot, ProcessState};
/// Project files, commands, configuration, and detection.
pub use project::{
    CommandConfig, EditorBuffer, FileEntry, FileKind, GlassProjectConfig, ProjectDetection,
    ProjectTreeResult, ProjectWorkspace, detect_project,
};
/// Remote-view state and input/frame types.
pub use remote_view::{RemoteFrame, RemoteInput, RemoteView};
/// Revision identifiers and bounded replay.
pub use replay::{DevelopmentRevision, ReplayWindow, replay};
/// Search hit types and ranking helpers.
pub use search::{SearchHit, SearchKind, fuzzy_score, rank};
use std::{
    fmt,
    fs::OpenOptions,
    io::{self, Read},
    path::{Path, PathBuf},
};

/// Version identifier for the top-level development payload schema.
pub const DEVELOPMENT_SCHEMA_VERSION: &str = "glass.development.v1";
/// Version of serialized development timeline events.
pub const DEVELOPMENT_EVENT_SCHEMA_VERSION: u32 = 1;
/// Version of serialized cockpit snapshots.
pub const DEVELOPMENT_COCKPIT_SCHEMA_VERSION: u32 = 1;
/// Maximum bytes read from one project file.
pub const MAX_FILE_BYTES: usize = 512 * 1024;
/// Maximum bytes retained in an editor or process buffer.
pub const MAX_BUFFER_BYTES: usize = 1024 * 1024;
/// Maximum entries returned by bounded project-tree scans.
pub const MAX_FILE_ENTRIES: usize = 2_048;
/// Maximum retained timeline events.
pub const MAX_TIMELINE_EVENTS: usize = 512;
/// Maximum retained process output bytes.
pub const MAX_PROCESS_OUTPUT_BYTES: usize = 32 * 1024;

pub(crate) fn read_bounded_utf8(
    path: &Path,
    limit: usize,
    description: &str,
) -> DevelopmentResult<String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} is not a regular file"
        )));
    }
    if metadata.len() > limit as u64 {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} exceeds the {limit} byte limit"
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(limit as u64) as usize);
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} exceeds the {limit} byte limit"
        )));
    }
    String::from_utf8(bytes)
        .map_err(|_| DevelopmentError::InvalidInput(format!("{description} is not valid UTF-8")))
}

/// Result type returned by development services and bounded helpers.
pub type DevelopmentResult<T> = Result<T, DevelopmentError>;

/// Errors produced while resolving, reading, or executing development work.
#[derive(Debug)]
pub enum DevelopmentError {
    /// Filesystem or operating-system I/O failure.
    Io(io::Error),
    /// Caller input violates a documented bound or format.
    InvalidInput(String),
    /// A path resolves outside the canonical project workspace.
    PathOutsideWorkspace(PathBuf),
    /// A requested project resource does not exist.
    NotFound(String),
    /// Project configuration is invalid or cannot be loaded.
    Config(String),
    /// The requested operation conflicts with current runtime state.
    Conflict(String),
    /// A managed process or subprocess failed.
    Process(String),
    /// Serialization or deserialization failed.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_utf8_reader_rejects_oversized_and_invalid_files() {
        let root =
            std::env::temp_dir().join(format!("glass-bounded-reader-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("input");

        std::fs::write(&path, b"bounded").unwrap();
        assert_eq!(read_bounded_utf8(&path, 7, "fixture").unwrap(), "bounded");
        std::fs::write(&path, b"oversized").unwrap();
        assert!(read_bounded_utf8(&path, 8, "fixture").is_err());
        std::fs::write(&path, [0xff]).unwrap();
        assert!(read_bounded_utf8(&path, 1, "fixture").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = root.join("target");
            let link = root.join("link");
            std::fs::write(&target, b"bounded").unwrap();
            symlink(&target, &link).unwrap();
            assert!(read_bounded_utf8(&link, 7, "fixture").is_err());
        }

        let _ = std::fs::remove_dir_all(root);
    }
}

//! Terminal-native project development runtime.
//!
//! This module owns the local development contracts used by the CLI, MCP, TUI,
//! and embedded harness. Browser authority remains in the existing browser
//! runtime: project state can coordinate a live application without duplicating
//! browser control logic.
//!
//! Enable Cargo feature `development-runtime` for the concrete project, editor,
//! PTY, language-server, graph, replay, Neovim, experiment, and agent harness
//! implementations used by `glass-dev`. Without the feature, the public
//! browser library retains only the stable shared contracts needed by its
//! normal surfaces.
//!
//! Project paths are resolved beneath one canonical root; reads, output tails,
//! event logs, and retained buffers have hard bounds. Writes are atomic and
//! actor-attributed. Runtime claims such as live-update provenance remain
//! pending until explicit source/runtime evidence or browser revision evidence
//! supports them. A project session never implies browser mutation authority.

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
pub mod remote_view;
pub mod replay;
pub mod search;

use std::{
    fmt,
    fs::OpenOptions,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub use agent::{
    AgentAuthorityContext, AgentContextPacket, AgentToolGateway, BrowserAgentContext, HarnessEvent,
    HarnessRequest, LocalHarness, PiHarness, PiHarnessOptions, ToolAuthorization, ToolCall,
    ToolDescriptor, ToolRegistry, resolve_context, resolve_context_with_browser,
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
pub use language::{
    DiagnosticPosition, LanguageDiagnostic, LanguageDocument, LanguageResponse, LspClient,
};
pub use neovim::{NeovimCapability, probe_neovim, start_neovim};
pub use process::{ProcessHealth, ProcessManager, ProcessSnapshot, ProcessState};
pub use project::{
    CommandConfig, EditorBuffer, FileEntry, FileKind, GlassProjectConfig, ProjectDetection,
    ProjectTreeResult, ProjectWorkspace, detect_project,
};
pub use remote_view::{RemoteFrame, RemoteInput, RemoteView};
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

//! Resident agent harness selection.
//!
//! Glass Dev runs resident agents through the standalone `glass-core` host runtime.

use crate::development::{DevelopmentError, DevelopmentResult, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Executes one host tool call against the resident Glass workspace.
pub type AgentToolExecutor =
    Arc<dyn Fn(&ToolCall, bool, bool) -> DevelopmentResult<Value> + Send + Sync>;

/// Operations available to a resident Glass conversation.
#[derive(Debug, Clone)]
pub enum ResidentSessionRequest {
    Hello,
    Prompt {
        text: String,
        context: Option<Value>,
    },
    Steer {
        text: String,
        context: Option<Value>,
    },
    FollowUp {
        text: String,
        context: Option<Value>,
    },
    Complete {
        prefix: String,
        suffix: String,
    },
    Abort,
    State,
    Models,
    SetModel {
        provider: String,
        model_id: String,
    },
    SetThinking {
        level: String,
    },
    NewSession,
    Compact {
        instructions: Option<String>,
    },
    CloneSession,
    Rewind {
        entry_id: String,
    },
    Fork {
        entry_id: String,
    },
    SwitchSession {
        path: String,
    },
    ListSessions,
    Entries {
        since: Option<String>,
    },
    Tree,
    Messages,
    SessionStats,
    SetSessionName {
        name: String,
    },
}

/// Resident agent engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResidentHarnessKind {
    /// The `glass-core` engine over its host runtime protocol.
    GlassCore,
}

impl ResidentHarnessKind {
    /// The engine used when nothing selects one explicitly.
    pub const fn default_kind() -> Self {
        Self::GlassCore
    }

    /// Parse a resident engine id.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "glass-core" | "glasscore" | "core" | "gc" => Some(Self::GlassCore),
            _ => None,
        }
    }

    /// Stable machine-readable id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::GlassCore => "glass-core",
        }
    }

    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GlassCore => "Glass Core",
        }
    }

    /// Short description used in readiness summaries.
    pub const fn description(self) -> &'static str {
        match self {
            Self::GlassCore => "resident engine with Glass-owned tools and sessions",
        }
    }

    /// True when this engine is the supported default.
    pub const fn is_default(self) -> bool {
        matches!(self, Self::GlassCore)
    }
}

/// Options for one resident agent session, independent of the engine.
#[derive(Clone, Default)]
pub struct ResidentHarnessOptions {
    /// Directory holding session records.
    pub session_dir: std::path::PathBuf,
    /// Display name stored with the session.
    pub name: Option<String>,
    /// Model id selected for the session.
    pub model: Option<String>,
    /// Thinking level requested by the caller, where the engine supports one.
    pub thinking: Option<String>,
    /// Authoritative resident daemon broker used when no local executor exists.
    pub broker: Option<crate::agents::ResidentAgentBroker>,
    /// Executes host tool calls inside the resident workspace.
    pub local_tool_executor: Option<AgentToolExecutor>,
    /// Extra instructions appended to the Glass collaboration brief.
    pub additional_system_prompt: Option<String>,
    /// Resume the newest stored session for this workspace.
    pub resume: bool,
    /// Resume this exact persisted Core session during Glass thread recovery.
    pub resume_session_id: Option<String>,
    /// Explicit Core executable override; primarily used by offline runtime fixtures.
    pub binary: Option<std::path::PathBuf>,
    /// Extra arguments passed to the Core executable before `runtime`.
    pub binary_args: Vec<String>,
    /// Session reference to branch from, when the caller wants a fork.
    pub fork_from: Option<std::path::PathBuf>,
}

impl std::fmt::Debug for ResidentHarnessOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResidentHarnessOptions")
            .field("session_dir", &self.session_dir)
            .field("name", &self.name)
            .field("model", &self.model)
            .field("thinking", &self.thinking)
            .field("broker", &self.broker)
            .field(
                "local_tool_executor",
                &self.local_tool_executor.as_ref().map(|_| "configured"),
            )
            .field("additional_system_prompt", &self.additional_system_prompt)
            .field("resume", &self.resume)
            .field("resume_session_id", &self.resume_session_id)
            .field("binary", &self.binary)
            .field("binary_args", &self.binary_args)
            .field("fork_from", &self.fork_from)
            .finish()
    }
}

impl ResidentHarnessOptions {
    fn into_core(self) -> crate::core_runtime::CoreRuntimeOptions {
        crate::core_runtime::CoreRuntimeOptions {
            session_dir: self.session_dir,
            name: self.name,
            model: self.model,
            thinking: self.thinking,
            broker: self.broker,
            local_tool_executor: self.local_tool_executor,
            additional_system_prompt: self.additional_system_prompt,
            resume: self.resume,
            resume_session_id: self.resume_session_id,
            fork_from: self.fork_from,
            binary: self.binary,
            binary_args: self.binary_args,
        }
    }
}

/// One running resident agent engine.
pub(crate) enum ResidentHarness {
    /// `glass-core` host runtime process.
    GlassCore(crate::core_runtime::GlassCoreRuntime),
}

impl std::fmt::Debug for ResidentHarness {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GlassCore(runtime) => formatter.debug_tuple("GlassCore").field(runtime).finish(),
        }
    }
}

impl ResidentHarness {
    /// Start the selected engine for one resident agent.
    pub(crate) fn spawn(
        kind: ResidentHarnessKind,
        root: &Path,
        options: ResidentHarnessOptions,
    ) -> DevelopmentResult<Self> {
        match kind {
            ResidentHarnessKind::GlassCore => {
                crate::core_runtime::GlassCoreRuntime::spawn(root, options.into_core())
                    .map(Self::GlassCore)
            }
        }
    }

    /// Queue one operation and return the engine request id.
    pub(crate) fn start_request(
        &mut self,
        request: ResidentSessionRequest,
    ) -> DevelopmentResult<String> {
        match self {
            Self::GlassCore(runtime) => runtime.start_request(request),
        }
    }

    /// Return the actual initialized Core session ID.
    pub(crate) fn core_session_id(&self) -> Option<&str> {
        match self {
            Self::GlassCore(runtime) => Some(runtime.session_id()),
        }
    }

    /// Take transcript events loaded before worker readiness.
    pub(crate) fn take_startup_events(&mut self) -> Vec<Value> {
        match self {
            Self::GlassCore(runtime) => runtime.take_startup_events(),
        }
    }

    /// Poll for the next Glass conversation event.
    pub(crate) fn recv_event_timeout(
        &mut self,
        timeout: Duration,
    ) -> DevelopmentResult<Option<Value>> {
        match self {
            Self::GlassCore(runtime) => runtime.recv_event_timeout(timeout),
        }
    }

    /// Answer one parked host tool approval.
    pub(crate) fn resolve_tool_approval(
        &mut self,
        frame_id: &str,
        approved: bool,
    ) -> DevelopmentResult<()> {
        match self {
            Self::GlassCore(runtime) => runtime.resolve_tool_approval(frame_id, approved),
        }
    }
}

/// Readiness for the resident engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidentReadiness {
    /// Engine that will run resident agents.
    pub selected: ResidentHarnessKind,
    /// True when the selected engine can start a session.
    pub ready: bool,
    /// One-line status for the terminal workspace.
    pub detail: String,
    /// Glass Core engine probe.
    pub glass_core: crate::core_runtime::CoreReadiness,
    /// Ordered repair steps for the selected engine.
    pub remediation: Vec<String>,
}

/// Probe the Glass Core resident engine.
pub fn resident_readiness() -> DevelopmentResult<ResidentReadiness> {
    let selected = ResidentHarnessKind::default_kind();
    let glass_core = crate::core_runtime::core_readiness()?;
    let ready = glass_core.ready;
    let detail = if ready {
        format!(
            "{} ready · {}",
            selected.label(),
            glass_core
                .binary
                .version
                .as_deref()
                .map(|version| format!("glass-core {version}"))
                .unwrap_or_else(|| "engine available".into())
        )
    } else {
        format!(
            "{} unavailable · {}",
            selected.label(),
            glass_core.binary.detail
        )
    };
    let remediation = glass_core.remediation.clone();
    Ok(ResidentReadiness {
        selected,
        ready,
        detail,
        glass_core,
        remediation,
    })
}

/// Reject a harness id that Glass does not know.
pub fn harness_kind_from_id(id: &str) -> DevelopmentResult<ResidentHarnessKind> {
    ResidentHarnessKind::parse(id).ok_or_else(|| {
        DevelopmentError::InvalidInput(format!(
            "unknown resident harness {id}; expected glass-core"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_core_is_the_default_harness() {
        assert_eq!(
            ResidentHarnessKind::default_kind(),
            ResidentHarnessKind::GlassCore
        );
        assert!(ResidentHarnessKind::GlassCore.is_default());
        assert!(ResidentHarnessKind::GlassCore.is_default());
    }

    #[test]
    fn harness_ids_accept_core_spellings_only() {
        assert_eq!(
            ResidentHarnessKind::parse("GLASS-CORE"),
            Some(ResidentHarnessKind::GlassCore)
        );
        assert_eq!(
            ResidentHarnessKind::parse("gc"),
            Some(ResidentHarnessKind::GlassCore)
        );
        assert_eq!(ResidentHarnessKind::parse("unknown"), None);
    }

    #[test]
    fn options_survive_conversion_to_core() {
        let options = ResidentHarnessOptions {
            name: Some("reviewer".into()),
            model: Some("model-a".into()),
            ..ResidentHarnessOptions::default()
        };
        let core = options.clone().into_core();
        assert_eq!(core.name.as_deref(), Some("reviewer"));
        assert_eq!(core.model.as_deref(), Some("model-a"));
    }
}

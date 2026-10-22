//! Resident agent harness selection.
//!
//! Glass Dev runs its resident agents on one of two engines:
//!
//! - [`ResidentHarnessKind::GlassCore`] drives the `glass-core` engine over its
//!   host runtime protocol. This is the default and the supported path.
//! - [`ResidentHarnessKind::Pi`] drives the Pi SDK runtime. It stays available
//!   as a legacy backend for existing installations and is not the default.
//!
//! Glass keeps the workspace, the governed tool router, and the approval
//! surface for both engines, so switching engines never changes who owns the
//! effects of a tool call.

use crate::development::{DevelopmentError, DevelopmentResult, ToolCall};
use crate::pi_runtime::PiSessionRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Executes one host tool call against the resident Glass workspace.
pub type AgentToolExecutor =
    Arc<dyn Fn(&ToolCall, bool, bool) -> DevelopmentResult<Value> + Send + Sync>;

/// Environment override for the resident harness.
pub const HARNESS_ENV: &str = "GLASS_AGENT_HARNESS";

/// Supported resident agent engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResidentHarnessKind {
    /// The `glass-core` engine over its host runtime protocol.
    GlassCore,
    /// The Pi SDK runtime, retained as a legacy backend.
    Pi,
}

impl ResidentHarnessKind {
    /// The engine used when nothing selects one explicitly.
    pub const fn default_kind() -> Self {
        Self::GlassCore
    }

    /// Parse a harness id, accepting the historical spellings.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "glass-core" | "glasscore" | "core" | "gc" => Some(Self::GlassCore),
            "pi" | "pi-sdk" | "legacy-pi" => Some(Self::Pi),
            _ => None,
        }
    }

    /// Select the harness from `GLASS_AGENT_HARNESS`, falling back to the
    /// default when the variable is unset or unrecognised.
    pub fn from_env() -> Self {
        std::env::var(HARNESS_ENV)
            .ok()
            .and_then(|value| Self::parse(&value))
            .unwrap_or_else(Self::default_kind)
    }

    /// Stable machine-readable id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::GlassCore => "glass-core",
            Self::Pi => "pi",
        }
    }

    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::GlassCore => "Glass Core",
            Self::Pi => "Pi SDK (legacy)",
        }
    }

    /// Short description used in readiness summaries.
    pub const fn description(self) -> &'static str {
        match self {
            Self::GlassCore => "resident engine with Glass-owned tools and sessions",
            Self::Pi => "legacy Pi AgentSession runtime, kept for existing installs",
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
    /// Execute mutations without pausing for an approval frame.
    pub unrestricted: bool,
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
    /// Session reference to branch from, when the caller wants a fork.
    pub fork_from: Option<std::path::PathBuf>,
}

impl std::fmt::Debug for ResidentHarnessOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResidentHarnessOptions")
            .field("unrestricted", &self.unrestricted)
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
            .field("fork_from", &self.fork_from)
            .finish()
    }
}

impl ResidentHarnessOptions {
    fn into_pi(self) -> crate::pi_runtime::PiRuntimeOptions {
        crate::pi_runtime::PiRuntimeOptions {
            unrestricted: self.unrestricted,
            session_dir: self.session_dir,
            name: self.name,
            model: self.model,
            thinking: self.thinking,
            broker: self.broker,
            local_tool_executor: self.local_tool_executor,
            additional_system_prompt: self.additional_system_prompt,
            resume: self.resume,
            fork_from: self.fork_from,
        }
    }

    fn into_core(self) -> crate::core_runtime::CoreRuntimeOptions {
        crate::core_runtime::CoreRuntimeOptions {
            unrestricted: self.unrestricted,
            session_dir: self.session_dir,
            name: self.name,
            model: self.model,
            thinking: self.thinking,
            broker: self.broker,
            local_tool_executor: self.local_tool_executor,
            additional_system_prompt: self.additional_system_prompt,
            resume: self.resume,
            fork_from: self.fork_from,
            binary: None,
            binary_args: Vec::new(),
        }
    }
}

/// One running resident agent engine.
pub(crate) enum ResidentHarness {
    /// `glass-core` host runtime process.
    GlassCore(crate::core_runtime::GlassCoreRuntime),
    /// Pi SDK runtime process.
    Pi(crate::pi_runtime::GlassPiRuntime),
}

impl std::fmt::Debug for ResidentHarness {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GlassCore(runtime) => formatter.debug_tuple("GlassCore").field(runtime).finish(),
            Self::Pi(runtime) => formatter.debug_tuple("Pi").field(runtime).finish(),
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
            ResidentHarnessKind::Pi => {
                crate::pi_runtime::GlassPiRuntime::spawn(root, options.into_pi()).map(Self::Pi)
            }
        }
    }

    /// Queue one operation and return the engine request id.
    pub(crate) fn start_request(&mut self, request: PiSessionRequest) -> DevelopmentResult<String> {
        match self {
            Self::GlassCore(runtime) => runtime.start_request(request),
            Self::Pi(runtime) => runtime.start_request(request),
        }
    }

    /// Return the actual initialized Core session ID; legacy Pi has no compatible reference.
    pub(crate) fn core_session_id(&self) -> Option<&str> {
        match self {
            Self::GlassCore(runtime) => Some(runtime.session_id()),
            Self::Pi(_) => None,
        }
    }

    /// Poll for the next Glass conversation event.
    pub(crate) fn recv_event_timeout(
        &mut self,
        timeout: Duration,
    ) -> DevelopmentResult<Option<Value>> {
        match self {
            Self::GlassCore(runtime) => runtime.recv_event_timeout(timeout),
            Self::Pi(runtime) => runtime.recv_event_timeout(timeout),
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
            Self::Pi(runtime) => runtime.resolve_tool_approval(frame_id, approved),
        }
    }
}

/// Combined readiness for the resident harness selection.
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
    /// Pi SDK probe, retained for legacy installs.
    pub pi: Option<crate::pi_runtime::PiReadiness>,
    /// Ordered repair steps for the selected engine.
    pub remediation: Vec<String>,
}

/// Probe the selected resident harness and report the alternatives.
pub fn resident_readiness() -> DevelopmentResult<ResidentReadiness> {
    let selected = ResidentHarnessKind::from_env();
    let glass_core = crate::core_runtime::core_readiness()?;
    let pi = crate::pi_runtime::pi_readiness().ok();
    let ready = match selected {
        ResidentHarnessKind::GlassCore => glass_core.ready,
        ResidentHarnessKind::Pi => pi.as_ref().is_some_and(|readiness| readiness.ready),
    };
    let detail = match selected {
        ResidentHarnessKind::GlassCore if glass_core.ready => format!(
            "{} ready · {}",
            selected.label(),
            glass_core
                .binary
                .version
                .as_deref()
                .map(|version| format!("glass-core {version}"))
                .unwrap_or_else(|| "engine available".into())
        ),
        ResidentHarnessKind::GlassCore => format!(
            "{} unavailable · {}",
            selected.label(),
            glass_core.binary.detail
        ),
        ResidentHarnessKind::Pi if ready => format!("{} ready", selected.label()),
        ResidentHarnessKind::Pi => format!("{} unavailable", selected.label()),
    };
    let remediation = match selected {
        ResidentHarnessKind::GlassCore => glass_core.remediation.clone(),
        ResidentHarnessKind::Pi => pi
            .as_ref()
            .map(|readiness| readiness.remediation.clone())
            .unwrap_or_default(),
    };
    Ok(ResidentReadiness {
        selected,
        ready,
        detail,
        glass_core,
        pi,
        remediation,
    })
}

/// Reject a harness id that Glass does not know.
pub fn harness_kind_from_id(id: &str) -> DevelopmentResult<ResidentHarnessKind> {
    ResidentHarnessKind::parse(id).ok_or_else(|| {
        DevelopmentError::InvalidInput(format!(
            "unknown resident harness {id}; expected glass-core or pi"
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
        assert!(!ResidentHarnessKind::Pi.is_default());
    }

    #[test]
    fn harness_ids_accept_historical_spellings() {
        assert_eq!(
            ResidentHarnessKind::parse("GLASS-CORE"),
            Some(ResidentHarnessKind::GlassCore)
        );
        assert_eq!(
            ResidentHarnessKind::parse("gc"),
            Some(ResidentHarnessKind::GlassCore)
        );
        assert_eq!(
            ResidentHarnessKind::parse("pi"),
            Some(ResidentHarnessKind::Pi)
        );
        assert_eq!(ResidentHarnessKind::parse("unknown"), None);
    }

    #[test]
    fn options_survive_conversion_to_each_engine() {
        let options = ResidentHarnessOptions {
            unrestricted: true,
            name: Some("reviewer".into()),
            model: Some("model-a".into()),
            ..ResidentHarnessOptions::default()
        };
        let core = options.clone().into_core();
        assert_eq!(core.name.as_deref(), Some("reviewer"));
        assert_eq!(core.model.as_deref(), Some("model-a"));
        let pi = options.into_pi();
        assert_eq!(pi.name.as_deref(), Some("reviewer"));
        assert!(pi.unrestricted);
    }
}

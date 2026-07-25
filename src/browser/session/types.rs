#![allow(dead_code)]
#![allow(unused_imports)]
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::Mutex;

use crate::browser::cdp::{CdpClient, CdpEventWithParams};
use crate::browser::chrome::{ChromeProcess, get_ws_url};
use crate::browser::dom::{
    AxNode, CompactAxNode, CompactInteractiveElement, DomNode, backend_node_reference,
    find_interactive_elements, format_tree,
};
use crate::browser::mouse::{MouseEngine, Point};
use crate::browser::policy::{BrowserPolicy, PolicyError};
use crate::browser::profile::ProfileManager;

pub type BrowserResult<T> = Result<T, Box<dyn Error>>;

/// Maximum number of steps in a single batch operation.
pub const MAX_BATCH_STEPS: usize = 32;

/// Maximum audit entries retained per session.
pub const MAX_AUDIT_ENTRIES: usize = 512;

/// A bounded, redacted audit entry for a high-risk session operation.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    /// Monotonic sequence number within the session.
    pub sequence: u64,
    /// Operation kind: navigate, evaluate, upload, download, attach.
    pub operation: String,
    /// Bounded, redacted detail (URLs truncated, expressions summarized).
    pub detail: String,
    /// Policy preset active at the time of the operation.
    pub policy_preset: String,
}

/// A single step in a typed batch operation.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum BatchStep {
    #[serde(rename = "navigate")]
    Navigate {
        url: String,
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
    },
    #[serde(rename = "click")]
    Click { target: String },
    #[serde(rename = "type")]
    Type {
        text: String,
        #[serde(default)]
        target: Option<String>,
    },
    #[serde(rename = "check")]
    Check { target: String },
    #[serde(rename = "uncheck")]
    Uncheck { target: String },
    #[serde(rename = "select")]
    Select { target: String, value: String },
    #[serde(rename = "clear")]
    Clear { target: String },
    #[serde(rename = "scroll")]
    Scroll {
        #[serde(default)]
        dx: f64,
        #[serde(default)]
        dy: f64,
    },
    #[serde(rename = "wait")]
    Wait {
        condition: String,
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
    },
    #[serde(rename = "observe")]
    Observe {
        #[serde(default)]
        include_dom: bool,
        #[serde(default)]
        include_screenshot: bool,
        #[serde(default)]
        include_form_values: bool,
    },
    #[serde(rename = "screenshot")]
    Screenshot,
    #[serde(rename = "evaluate")]
    Evaluate { expression: String },
    #[serde(rename = "acceptDialog")]
    AcceptDialog,
    #[serde(rename = "dismissDialog")]
    DismissDialog,
}

const fn default_timeout_ms() -> u64 {
    20_000
}

/// Outcome of a single step in a batch.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BatchStepOutcome {
    Success {
        index: usize,
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_bytes: Option<usize>,
    },
    Error {
        index: usize,
        action: String,
        message: String,
    },
}

/// Aggregate batch result.
#[derive(Debug, Clone, Serialize)]
pub struct BatchOutcome {
    pub steps: Vec<BatchStepOutcome>,
    pub completed: usize,
    pub failed: usize,
    pub total: usize,
    pub success: bool,
}

/// Error returned when batch policy pre-flight fails.
#[derive(Debug, Clone, Serialize)]
pub struct BatchPolicyDenial {
    pub step_index: usize,
    pub action: String,
    pub reason: String,
}

// ── Reference Reconciliation ──────────────────────────────────────────

/// Maximum number of refs that can be reconciled in a single call.
pub const MAX_RECONCILE_REFS: usize = 16;

/// Mapping of a prior reference to its current identity.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReferenceMapping {
    /// Same backend node ID still present and uniquely actionable.
    Preserved { old: String, new: String },
    /// Backend node changed but a unique stable identity match exists.
    Relocated {
        old: String,
        new: String,
        #[serde(rename = "matchedBy")]
        matched_by: String,
    },
    /// No safe mapping; agent must re-observe.
    Lost { old: String, reason: String },
}

/// Outcome of a reconcileReferences call.
#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationOutcome {
    #[serde(rename = "toRevision")]
    pub to_revision: u64,
    pub mappings: Vec<ReferenceMapping>,
    pub preserved: usize,
    pub relocated: usize,
    pub lost: usize,
}

/// Recovery hint included in StaleReference errors.
#[derive(Debug, Clone, Serialize)]
pub struct StaleReferenceRecovery {
    pub suggestion: &'static str,
    #[serde(rename = "fromRevision")]
    pub from_revision: u64,
    #[serde(rename = "staleRef")]
    pub stale_ref: String,
}

// ── Session Checkpoint ─────────────────────────────────────────────────

/// Versioned checkpoint for cross-process agent workflow resume.
/// Bounded to ≤ 4 KiB JSON; no cookies, passwords, or form values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointV1 {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u8,
    #[serde(rename = "glassVersion")]
    pub glass_version: String,
    #[serde(rename = "exportedAt")]
    pub exported_at: String,
    pub profile: String,
    #[serde(rename = "attachMode")]
    pub attach_mode: bool,
    pub topology: CheckpointTopology,
    pub observation: CheckpointObservation,
    pub policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointTopology {
    #[serde(rename = "targetId", skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(rename = "frameId", skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointObservation {
    pub revision: u64,
    /// Capped at 8 entries.
    #[serde(rename = "lastRefs")]
    pub last_refs: Vec<String>,
}

/// Error when a checkpoint cannot be imported.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckpointError {
    SchemaVersionMismatch { expected: u8, found: u8 },
    TargetClosed,
    Stale,
    InvalidJson(String),
}

/// Maximum UTF-8 byte length of visible text returned by a compact observation.
pub const COMPACT_TEXT_MAX_BYTES: usize = 16 * 1024;
pub(crate) const TEXT_TRUNCATION_MARKER: &str = "\n[truncated]";
pub(crate) const COMPACT_OBSERVATION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);
pub(crate) const COMPACT_OBSERVATION_MAX_ATTEMPTS: u8 = 2;
pub(crate) const COMPACT_PAGE_STATE_EXPRESSION: &str = r#"(() => {
    pub(crate) const key = '__glassObservationRevision';
    let state = globalThis[key];
    if (!state) {
        state = {revision: 0};
        pub(crate) const observer = new MutationObserver(() => { state.revision += 1; });
        observer.observe(document, {subtree:true, childList:true, attributes:true, characterData:true});
        globalThis[key] = state;
    }
    pub(crate) const summary = {scanned_elements:0, scan_limit:512, shadow_roots:0, child_frames:0, canvases:0, truncated:false};
    pub(crate) const walker = document.createTreeWalker(document, NodeFilter.SHOW_ELEMENT);
    while (walker.nextNode()) {
        if (summary.scanned_elements >= summary.scan_limit) { summary.truncated = true; break; }
        pub(crate) const element = walker.currentNode;
        summary.scanned_elements += 1;
        if (element.shadowRoot) summary.shadow_roots += 1;
        if (element.localName === 'iframe' || element.localName === 'frame') summary.child_frames += 1;
        if (element.localName === 'canvas') summary.canvases += 1;
    }
    return JSON.stringify({url:location.href, title:document.title, ready_state:document.readyState,
        text:(() => { const source=document.body ? document.body.innerText : ''; const bytes=new Uint8Array(16384);
            pub(crate) const encoded=new TextEncoder().encodeInto(source, bytes); summary.text_truncated=encoded.read < source.length;
            return new TextDecoder().decode(bytes.subarray(0, encoded.written)); })(),
        mutation_revision:state.revision, boundaries:summary});
})()"#;
pub(crate) const OWNED_BROWSER_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const AMBIGUOUS_CANDIDATE_LIMIT: usize = 8;
pub(crate) const CANDIDATE_LABEL_MAX_BYTES: usize = 160;
pub(crate) const TOPOLOGY_MAX_TARGETS: usize = 32;
pub(crate) const TOPOLOGY_MAX_FRAMES: usize = 128;
pub(crate) const TOPOLOGY_ID_MAX_BYTES: usize = 256;
pub(crate) const TOPOLOGY_TEXT_MAX_BYTES: usize = 1024;
pub(crate) const TOPOLOGY_MAX_EVENTS: usize = 64;
pub(crate) const POPUP_WITNESS_LIFETIME_MS: u64 = 5_000;
pub(crate) const POPUP_EVIDENCE_DEADLINE: Duration = Duration::from_secs(2);
pub(crate) const POPUP_TOPOLOGY_QUIET_INTERVAL: Duration = Duration::from_millis(50);
pub(crate) const POPUP_TOPOLOGY_POLL_INTERVAL: Duration = Duration::from_millis(5);
pub(crate) const POPUP_VERIFY_CALL_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const POPUP_RELEASE_ACK_TIMEOUT: Duration = Duration::from_millis(500);
pub(crate) const POPUP_ERROR_MESSAGE_MAX_BYTES: usize = 512;
pub(crate) const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub(crate) const WAIT_LAST_STATE_MAX_BYTES: usize = 512;
pub(crate) const NETWORK_IN_FLIGHT_LIMIT: usize = 1024;
pub(crate) const MAX_WAIT_DEADLINE: Duration = Duration::from_secs(300);
pub(crate) const MAX_WAIT_CONDITION_BYTES: usize = 4 * 1024;
pub(crate) const MAX_DIAGNOSTIC_DURATION: Duration = Duration::from_secs(30);
pub(crate) const MAX_DIAGNOSTIC_EVENTS: usize = 128;
pub(crate) const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 2 * 1024;
pub(crate) const MAX_DIAGNOSTIC_URL_BYTES: usize = 4 * 1024;
// Eight megapixels bounds worst-case 4-byte pixels plus base64 below 64 MiB
// before Chrome is asked to encode or the generic CDP actor receives JSON.
pub(crate) const MAX_VISUAL_PIXELS: f64 = 8.0 * 1024.0 * 1024.0;
pub(crate) const MAX_VISUAL_BASE64_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const VISUAL_HEADER_BASE64_BYTES: usize = 64 * 1024;
pub(crate) const HIT_TEST_FUNCTION: &str = r#"async function() {
    let element = this && this.nodeType === Node.ELEMENT_NODE ? this : this && this.parentElement;
    if (element) element = element.closest('button,a,input,select,textarea,[role],[tabindex]') || element;
    if (!element || !element.isConnected) return {ok:false, reason:'detached'};
    pub(crate) const sample = () => {
        pub(crate) const rect = element.getBoundingClientRect();
        return {left:rect.left, top:rect.top, width:rect.width, height:rect.height};
    };
    element.scrollIntoView({block:'nearest', inline:'nearest'});
    pub(crate) const first = sample();
    if (!element.isConnected) return {ok:false, reason:'detached'};
    if (element.getAnimations({subtree:true}).some(animation => animation.playState === 'running'))
        return {ok:false, reason:'unstable_geometry'};
    pub(crate) const second = sample();
    pub(crate) const style = getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || second.width <= 0 || second.height <= 0)
        return {ok:false, reason:'not_visible'};
    if (element.matches(':disabled') || element.getAttribute('aria-disabled') === 'true')
        return {ok:false, reason:'disabled'};
    if ([first.left, first.top, first.width, first.height].some((value, index) => Math.abs(value - [second.left, second.top, second.width, second.height][index]) > 1))
        return {ok:false, reason:'unstable_geometry'};
    pub(crate) const x = second.left + second.width / 2;
    pub(crate) const y = second.top + second.height / 2;
    if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight)
        return {ok:false, reason:'outside_viewport'};
    pub(crate) const hit = document.elementFromPoint(x, y);
    if (!hit || (hit !== element && !element.contains(hit)))
        return {ok:false, reason:'hit_test_blocked'};
    return {ok:true, x, y};
}"#;
pub(crate) const WAIT_TARGET_STATE_FUNCTION: &str = r#"function() {
    let element = this && this.nodeType === Node.ELEMENT_NODE ? this : this && this.parentElement;
    if (!element || !element.isConnected) return {attached:false, visible:false, enabled:false};
    pub(crate) const rect = element.getBoundingClientRect();
    pub(crate) const style = getComputedStyle(element);
    pub(crate) const visible = style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity) !== 0 && rect.width > 0 && rect.height > 0;
    pub(crate) const enabled = !element.matches(':disabled') && element.getAttribute('aria-disabled') !== 'true';
    return {attached:true, visible, enabled, geometry:[rect.left, rect.top, rect.width, rect.height].map(value => Math.round(value * 10) / 10).join(',')};
}"#;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub port: u16,
    pub chrome_path: Option<PathBuf>,
    pub profile: String,
    pub incognito: bool,
    /// Attach to an existing Chrome CDP endpoint instead of launching Chrome.
    pub attach: bool,
    /// Explicit Chrome page target ID, required whenever the endpoint has more
    /// than one page target.
    pub target_id: Option<String>,
    pub frame_id: Option<String>,
    pub headed: bool,
    pub interaction_mode: InteractionMode,
    /// Whether the session audit log is enabled.
    pub audit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InteractionMode {
    Human,
    Fast,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            port: 9222,
            chrome_path: None,
            profile: "default".to_string(),
            incognito: false,
            attach: false,
            target_id: None,
            frame_id: None,
            headed: false,
            interaction_mode: InteractionMode::Human,
            audit: false,
        }
    }
}

impl SessionOptions {
    /// Validate combinations that cannot be honored by an attached session.
    pub fn validate(&self) -> BrowserResult<()> {
        if self
            .target_id
            .as_deref()
            .is_some_and(|target_id| target_id.trim().is_empty())
        {
            return Err("target ID cannot be empty".into());
        }
        if self
            .frame_id
            .as_deref()
            .is_some_and(|frame_id| frame_id.trim().is_empty())
        {
            return Err("frame ID cannot be empty".into());
        }

        if self.attach {
            if self.incognito {
                return Err("--attach cannot be combined with --incognito".into());
            }
            if self.profile != "default" {
                return Err(
                    "--attach cannot be combined with a named --profile; attached Chrome owns its profile"
                        .into(),
                );
            }
            if self.chrome_path.is_some() {
                return Err("--attach cannot be combined with --chrome-path".into());
            }
            if self.headed {
                return Err("--attach cannot be combined with --headed".into());
            }
        } else {
            ProfileManager::validate_name(&self.profile)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub ready_state: String,
    #[serde(default)]
    pub target_id: String,
    #[serde(default)]
    pub frame_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageTargetInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opener_id: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub url: String,
    pub active: bool,
    pub out_of_process: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyEventSummary {
    pub sequence: u64,
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DestroyedPageTarget {
    pub(crate) target: PageTargetInfo,
    pub(crate) observed_sequence: u64,
}

#[derive(Default)]
pub(crate) struct TopologyRegistry {
    pub(crate) targets: Vec<PageTargetInfo>,
    pub(crate) frames: Vec<FrameInfo>,
    pub(crate) active_target_id: Option<String>,
    pub(crate) active_frame_id: Option<String>,
    pub(crate) active_target_session_id: Option<String>,
    pub(crate) active_session_id: Option<String>,
    pub(crate) frame_sessions: HashMap<String, String>,
    pub(crate) frame_parents: HashMap<String, String>,
    pub(crate) events: VecDeque<TopologyEventSummary>,
    pub(crate) sequence: u64,
    pub(crate) event_loss_count: u64,
    pub(crate) target_sequences: HashMap<String, u64>,
    pub(crate) destroyed_targets: VecDeque<DestroyedPageTarget>,
    /// Most recently opened JavaScript dialog, if any.
    pub(crate) pending_dialog: Option<PendingDialog>,
}

/// JavaScript dialog content surfaced to agents before resolution.
#[derive(Debug, Clone, Serialize)]
pub struct PendingDialog {
    /// `alert`, `confirm`, `prompt`, or `beforeunload`.
    #[serde(rename = "type")]
    pub dialog_type: String,
    /// Dialog message text (bounded to 256 bytes).
    pub message: String,
    /// Default prompt value (only for `prompt` dialogs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// Page URL that opened the dialog.
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveElement {
    pub reference: String,
    pub role: String,
    pub name: String,
    pub description: String,
    pub backend_dom_node_id: i64,
}

/// An explicit, deterministic element lookup strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Reference(String),
    AccessibleName(String),
    RoleAndName { role: String, name: String },
    Text(String),
    Css(String),
    Ordinal(usize),
}

/// A bounded description returned when a locator is ambiguous.
#[derive(Debug, Clone, Serialize)]
pub struct CandidateSummary {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// A bounded, structured targeting failure safe for agent-facing protocols.
#[derive(Debug, Clone, Serialize)]
pub struct TargetError {
    pub kind: TargetErrorKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<TargetActionabilityReason>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<CandidateSummary>,
}
/// Outcome of a side-effect-free preflight target resolution and actionability check.
#[derive(Debug, Clone, Serialize)]
pub struct PreflightOutcome {
    /// Whether the target resolved uniquely.
    pub unique: bool,
    /// The resolved element (only present when unique).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<ResolvedElement>,
    /// Whether the target is actionable (only meaningful when unique).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actionable: Option<bool>,
    /// Reason the target is not actionable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actionability_reason: Option<TargetActionabilityReason>,
    /// Ambiguous candidates (only when resolution is ambiguous).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<CandidateSummary>,
    /// Error kind when resolution fails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<TargetErrorKind>,
    /// Current page revision.
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetActionabilityReason {
    Detached,
    NotVisible,
    Disabled,
    UnstableGeometry,
    OutsideViewport,
    HitTestBlocked,
    GeometryChanged,
    NodeUnavailable,
    VerificationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetErrorKind {
    Ambiguous,
    NotFound,
    StaleReference,
    NotActionable,
}

impl std::fmt::Display for TargetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.kind {
            TargetErrorKind::Ambiguous => "element target is ambiguous",
            TargetErrorKind::NotFound => "element target was not found",
            TargetErrorKind::StaleReference => "element reference is stale",
            TargetErrorKind::NotActionable => "element target is not actionable",
        };
        formatter.write_str(message)
    }
}

impl Error for TargetError {}

/// Machine-readable topology failure with an agent-facing recovery hint.
///
/// When an agent receives this error, the [`recovery`](TopologyError::recovery) field
/// holds a stable enumeration value the agent can match on to choose its next
/// action without parsing free-text messages.
#[derive(Debug, Clone, Serialize)]
pub struct TopologyError {
    pub kind: TopologyErrorKind,
    pub message: String,
    /// Stable hint telling an agent what recovery action to take next.
    pub recovery: TopologyRecoveryHint,
}

/// Stable, machine-readable kind for [`TopologyError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyErrorKind {
    /// No active page target is selected; agent should call `listTargets` and
    /// (re-)select one.
    NoTargetSelected,
    /// The active target is no longer reachable (detached, crashed, or destroyed).
    StaleTarget,
    /// The active frame is no longer present in the current target's frame tree.
    StaleFrame,
    /// The requested frame was not found in the current target.
    NoSuchFrame,
    /// The active target has no open CDP session (internal routing loss).
    NoPageSession,
    /// Topology budget exceeded (too many targets, frames, or events).
    BudgetExceeded,
    /// CDP routing was lost and the session must be re-synchronised.
    RoutingLost,
}

/// Action an agent should take after receiving a [`TopologyError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyRecoveryHint {
    /// Call `listTargets` to refresh the page-target inventory and then select one.
    ListTargets,
    /// Call `observe` to obtain a fresh view of the current page.
    ReObserve,
    /// Call `listFrames` and then `selectFrame` with a valid frame ID.
    ListFrames,
    /// The session may need to be re-established; recreate the session.
    Reconnect,
}

impl TopologyError {
    pub(crate) fn new(kind: TopologyErrorKind, message: impl Into<String>) -> Self {
        let recovery = match kind {
            TopologyErrorKind::NoTargetSelected => TopologyRecoveryHint::ListTargets,
            TopologyErrorKind::StaleTarget => TopologyRecoveryHint::ListTargets,
            TopologyErrorKind::StaleFrame => TopologyRecoveryHint::ListFrames,
            TopologyErrorKind::NoSuchFrame => TopologyRecoveryHint::ListFrames,
            TopologyErrorKind::NoPageSession => TopologyRecoveryHint::Reconnect,
            TopologyErrorKind::BudgetExceeded => TopologyRecoveryHint::ReObserve,
            TopologyErrorKind::RoutingLost => TopologyRecoveryHint::Reconnect,
        };
        Self {
            kind,
            message: message.into(),
            recovery,
        }
    }
}

impl std::fmt::Display for TopologyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "topology {:?}: {} (recovery: {:?})",
            self.kind, self.message, self.recovery
        )
    }
}

impl Error for TopologyError {}

#[derive(Debug)]
pub(crate) enum TargetResolution {
    Unique(ResolvedElement),
    Ambiguous(Vec<CandidateSummary>),
    NotFound,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessibilitySnapshot {
    pub page: PageInfo,
    pub roots: Vec<AxNode>,
    pub interactive: Vec<InteractiveElement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitCondition {
    Lifecycle(String),
    UrlExact(String),
    UrlPrefix(String),
    TargetAttached(String),
    TargetVisible(String),
    TargetHidden(String),
    TargetEnabled(String),
    TargetStable(String),
    Text(String),
    JavaScript(String),
    NetworkQuiet(Duration),
}

#[derive(Debug, Clone, Serialize)]
pub struct WaitOutcome {
    pub condition: String,
    pub elapsed_ms: u64,
    pub last_state: String,
    pub target_id: String,
    pub frame_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WaitTimeout {
    pub condition: String,
    pub deadline_ms: u64,
    pub last_state: String,
    pub reason: &'static str,
}

impl std::fmt::Display for WaitTimeout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "wait timed out for {}", self.condition)
    }
}

impl Error for WaitTimeout {}

/// Bounded accessibility state included with compact page observations.
#[derive(Debug, Clone, Serialize)]
pub struct CompactAccessibilitySnapshot {
    pub page: PageInfo,
    /// Page generation used by every published interactive reference.
    pub revision: u64,
    pub roots: Vec<CompactAxNode>,
    pub interactive: Vec<CompactInteractiveElement>,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
    /// Number of interactive controls discovered but omitted due to the 32-control budget.
    #[serde(skip_serializing_if = "is_zero")]
    pub omitted_count: usize,
    /// Whether the interactive list was relevance-ranked before truncation.
    #[serde(skip_serializing_if = "is_false")]
    pub ranking_applied: bool,
    /// Completeness assessment for agent escalation decisions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completeness: Option<ObservationCompleteness>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// Deterministic completeness score for compact observations.
#[derive(Debug, Clone, Serialize)]
pub struct ObservationCompleteness {
    /// 0.0–1.0 score indicating how much of the interactive surface is represented.
    pub score: f64,
    /// Advisory escalation suggestion for agents.
    pub suggest_escalation: EscalationSuggestion,
    /// Input signals used to compute the score.
    pub signals: CompletenessSignals,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletenessSignals {
    pub interactive_discovered: usize,
    pub interactive_returned: usize,
    pub shadow_hosts: usize,
    pub shadow_hosts_pierced: usize,
    pub canvases: usize,
    pub child_frames: usize,
    pub mutation_race: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationSuggestion {
    None,
    #[serde(rename = "getDOM")]
    GetDom,
    Coordinate,
    SelectFrame,
    Reobserve,
}

impl ObservationCompleteness {
    pub fn compute(
        interactive_discovered: usize,
        interactive_returned: usize,
        shadow_hosts: usize,
        shadow_hosts_pierced: usize,
        canvases: usize,
        child_frames: usize,
        mutation_race: bool,
    ) -> Self {
        // interactive factor
        let interactive_factor = if interactive_discovered > 0 {
            (interactive_returned as f64 / interactive_discovered as f64).min(1.0)
        } else {
            1.0
        };

        // shadow factor
        let shadow_factor = if shadow_hosts == 0 || shadow_hosts_pierced >= shadow_hosts {
            1.0
        } else if shadow_hosts_pierced > 0 {
            0.7
        } else {
            0.5
        };

        // frame factor
        let frame_factor = if child_frames == 0 { 1.0 } else { 0.8 };

        // consistency factor
        let consistency_factor = if mutation_race { 0.0 } else { 1.0 };

        let score = (interactive_factor * shadow_factor * frame_factor * consistency_factor)
            .clamp(0.0, 1.0);
        let score = (score * 100.0).round() / 100.0; // round to 2 decimal places

        let has_shadow_boundary = shadow_hosts > 0 && shadow_hosts_pierced < shadow_hosts;

        let suggest_escalation = if mutation_race {
            EscalationSuggestion::Reobserve
        } else if score >= 0.85 && !has_shadow_boundary {
            EscalationSuggestion::None
        } else if has_shadow_boundary || score < 0.6 {
            EscalationSuggestion::GetDom
        } else if canvases > 0 && interactive_returned < 8 {
            EscalationSuggestion::Coordinate
        } else if child_frames > 0 {
            EscalationSuggestion::SelectFrame
        } else {
            EscalationSuggestion::None
        };

        Self {
            score,
            suggest_escalation,
            signals: CompletenessSignals {
                interactive_discovered,
                interactive_returned,
                shadow_hosts,
                shadow_hosts_pierced,
                canvases,
                child_frames,
                mutation_race,
            },
        }
    }
}

/// Structured page state. Default observations omit optional deep data.
#[derive(Debug, Clone, Serialize)]
pub struct PageContext {
    pub page: PageInfo,
    pub text: String,
    /// Full DOM data is included only by an explicit deep-DOM observation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomNode>,
    pub accessibility: CompactAccessibilitySnapshot,
    pub consistency: ObservationConsistency,
    pub boundaries: ObservationBoundarySummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub incomplete: Vec<ObservationIncompleteReason>,
    /// Base64 PNG data is populated only when visual context is explicitly requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservationConsistency {
    pub consistent: bool,
    pub attempts: u8,
    pub start_revision: u64,
    pub end_revision: u64,
    pub start_mutation_revision: u64,
    pub end_mutation_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    pub target_id: String,
    pub frame_id: String,
    pub duration_ms: u64,
    pub console: Vec<ConsoleEvidence>,
    pub network: Vec<NetworkEvidence>,
    pub dropped_events: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsoleEvidence {
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkEvidence {
    pub request_id: String,
    pub method: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub safe_header_names: Vec<String>,
    pub redirect_count: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadOutcome {
    pub guid: String,
    pub suggested_filename: String,
    pub state: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub target_id: String,
    pub frame_id: String,
    /// SHA-256 hash of the downloaded file content, if the download completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadErrorKind {
    AuthorizationFailed,
    RestorationFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadError {
    pub kind: DownloadErrorKind,
    pub message: String,
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "download {:?}: {}", self.kind, self.message)
    }
}

impl Error for DownloadError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum VisualFormat {
    Png,
    Jpeg,
    Webp,
}

impl VisualFormat {
    pub fn as_cdp(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}

impl std::str::FromStr for VisualClip {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let values = value
            .split(',')
            .map(|part| {
                part.trim()
                    .parse::<f64>()
                    .map_err(|_| "clip must be x,y,width,height".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != 4 {
            return Err("clip must be x,y,width,height".to_string());
        }
        Ok(Self {
            x: values[0],
            y: values[1],
            width: values[2],
            height: values[3],
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VisualClip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct VisualCaptureOptions {
    pub format: VisualFormat,
    pub quality: Option<u8>,
    pub scale: f64,
    pub clip: Option<VisualClip>,
    pub full_page: bool,
    pub target: Option<String>,
}

impl Default for VisualCaptureOptions {
    fn default() -> Self {
        Self {
            format: VisualFormat::Png,
            quality: None,
            scale: 1.0,
            clip: None,
            full_page: false,
            target: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VisualCaptureMetadata {
    pub format: VisualFormat,
    pub width: usize,
    pub height: usize,
    pub encoded_bytes: usize,
    pub device_scale_factor: f64,
    pub scale: f64,
    pub full_page: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<VisualClip>,
    pub target_id: String,
    pub frame_id: String,
}

#[derive(Debug)]
pub struct VisualCapture {
    pub data: String,
    pub metadata: VisualCaptureMetadata,
}

#[cfg(feature = "visual-compare")]
#[derive(Debug, Serialize)]
pub struct VisualComparison {
    pub width: u32,
    pub height: u32,
    pub changed_pixels: u64,
    pub changed_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference_box: Option<VisualClip>,
}

#[cfg(feature = "visual-compare")]
pub fn compare_png_visuals(first: &str, second: &str) -> BrowserResult<VisualComparison> {
    let first = decode_png_for_comparison(first)?;
    let second = decode_png_for_comparison(second)?;
    if first.0 != second.0 || first.1 != second.1 || first.2 != second.2 {
        return Err("visual comparison requires equal PNG dimensions and color layout".into());
    }
    let (width, height, samples, first_pixels) = first;
    let second_pixels = second.3;
    let mut changed = 0_u64;
    let mut left = width;
    let mut top = height;
    let mut right = 0_u32;
    let mut bottom = 0_u32;
    for (index, (a, b)) in first_pixels
        .chunks_exact(samples)
        .zip(second_pixels.chunks_exact(samples))
        .enumerate()
    {
        if a != b {
            changed += 1;
            let x = index as u32 % width;
            let y = index as u32 / width;
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    Ok(VisualComparison {
        width,
        height,
        changed_pixels: changed,
        changed_ratio: changed as f64 / (u64::from(width) * u64::from(height)) as f64,
        difference_box: (changed > 0).then_some(VisualClip {
            x: f64::from(left),
            y: f64::from(top),
            width: f64::from(right - left + 1),
            height: f64::from(bottom - top + 1),
        }),
    })
}

#[cfg(feature = "visual-compare")]
pub(crate) fn decode_png_for_comparison(value: &str) -> BrowserResult<(u32, u32, usize, Vec<u8>)> {
    if value.len() > MAX_VISUAL_BASE64_BYTES {
        return Err("comparison PNG exceeded 64 MiB base64 budget".into());
    }
    let encoded = STANDARD.decode(value.as_bytes())?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(encoded));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let output_size = reader.output_buffer_size();
    if output_size > 32 * 1024 * 1024 {
        return Err("comparison PNG decoded size exceeded 32 MiB".into());
    }
    let mut pixels = vec![0; output_size];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    let samples = info.color_type.samples();
    Ok((info.width, info.height, samples, pixels))
}

#[derive(Debug, Serialize)]
pub struct ScreencastFrame {
    pub data: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScreencastStats {
    pub received: u64,
    pub dropped: u64,
}

pub struct ScreencastScope {
    pub(crate) cdp: CdpClient,
    pub(crate) session_id: Option<String>,
    pub(crate) receiver: tokio::sync::mpsc::Receiver<crate::browser::cdp::CdpScreencastFrame>,
    pub(crate) armed: bool,
}

pub(crate) struct ScreencastStartupGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) session_id: Option<String>,
    pub(crate) armed: bool,
}

impl ScreencastStartupGuard {
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ScreencastStartupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.cdp.close_screencast_channel();
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = stop_screencast_for(&cdp, session_id.as_deref()).await;
        });
    }
}

impl ScreencastScope {
    pub async fn next_frame(&mut self) -> Option<ScreencastFrame> {
        while let Some(frame) = self.receiver.recv().await {
            if frame.session_id == self.session_id {
                return Some(ScreencastFrame {
                    data: frame.data,
                    metadata: frame.metadata,
                });
            }
        }
        None
    }

    pub fn stats(&self) -> ScreencastStats {
        let (received, dropped) = self.cdp.screencast_stats();
        ScreencastStats { received, dropped }
    }

    pub async fn stop(mut self) -> BrowserResult<ScreencastStats> {
        stop_screencast_for(&self.cdp, self.session_id.as_deref()).await?;
        let (received, dropped) = self.cdp.close_screencast_channel();
        self.armed = false;
        Ok(ScreencastStats { received, dropped })
    }
}

impl Drop for ScreencastScope {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.cdp.close_screencast_channel();
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = stop_screencast_for(&cdp, session_id.as_deref()).await;
        });
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ObservationBoundarySummary {
    pub scanned_elements: usize,
    pub scan_limit: usize,
    pub shadow_roots: usize,
    pub child_frames: usize,
    pub canvases: usize,
    pub truncated: bool,
    #[serde(default)]
    pub text_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationIncompleteReason {
    VisibleText,
    AccessibilityNode,
    AccessibilityLabel,
    Control,
    ShadowBoundary,
    FrameBoundary,
    Canvas,
    BoundaryScan,
    MutationRace,
}

/// The completed browser operation represented by an [`ActionOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Click,
    ClickExpectPopup,
    DoubleClick,
    Hover,
    Drag,
    Type,
    KeyDown,
    KeyUp,
    KeyPress,
    Shortcut,
    Clear,
    Check,
    Uncheck,
    Select,
    Upload,
    Scroll,
}

/// A resolved browser target recorded in an action result.
#[derive(Debug, Clone, Serialize)]
pub struct ActionTarget {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// A compact, serializable result from an input action.
///
/// `revision` is the generation after the action invalidated page context. A
/// caller should observe again before reusing a previous element reference.
#[derive(Debug, Clone, Serialize)]
pub struct ActionOutcome {
    pub action: ActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<ActionTarget>,
    pub revision: u64,
    pub target_id: String,
    pub frame_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
}

/// Evidence returned by [`BrowserSession::click_expect_popup`].
#[derive(Debug, Clone, Serialize)]
pub struct PopupClickOutcome {
    pub action: ActionKind,
    pub target: ActionTarget,
    pub revision: u64,
    pub target_id: String,
    pub frame_id: String,
    pub causally_verified_popup: bool,
    pub popup_id: String,
    pub opener_id: String,
    pub evidence: PopupVerificationEvidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopupVerificationEvidence {
    pub trusted_click_witness: bool,
    pub release_acknowledged: bool,
    pub release_ack_wait_ms: f64,
    pub topology_sequence_before_release: u64,
    pub popup_observed_sequence: u64,
    pub attached: bool,
    pub ready_state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupClickErrorKind {
    ReleaseFailed,
    WitnessMissing,
    TopologyLagged,
    PopupMissing,
    PopupAmbiguous,
    PopupDestroyed,
    PopupOpenerMismatch,
    PopupUnreadable,
}

/// A fail-closed popup-click failure with a stable machine-readable kind.
#[derive(Debug, Clone, Serialize)]
pub struct PopupClickError {
    pub kind: PopupClickErrorKind,
    pub message: String,
}

impl std::fmt::Display for PopupClickError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "popup click {:?}: {}", self.kind, self.message)
    }
}

impl Error for PopupClickError {}

#[derive(Debug, Clone)]
pub(crate) struct PopupTopologySnapshot {
    pub(crate) original_target_id: String,
    pub(crate) original_frame_id: String,
    pub(crate) preexisting_target_ids: HashSet<String>,
    pub(crate) sequence: u64,
    pub(crate) event_loss_count: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct PopupCandidate {
    pub(crate) target: PageTargetInfo,
    pub(crate) observed_sequence: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct CompactPageContext {
    pub(crate) page: PageInfo,
    pub(crate) text: String,
    pub(crate) accessibility: CompactAccessibilitySnapshot,
    pub(crate) consistency: ObservationConsistency,
    pub(crate) boundaries: ObservationBoundarySummary,
    pub(crate) incomplete: Vec<ObservationIncompleteReason>,
}

impl CompactPageContext {
    pub(crate) fn into_page_context(self) -> PageContext {
        PageContext {
            page: self.page,
            text: self.text,
            dom: None,
            accessibility: self.accessibility,
            consistency: self.consistency,
            boundaries: self.boundaries,
            incomplete: self.incomplete,
            screenshot: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct EvaluatedPageState {
    pub(crate) url: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) ready_state: String,
    #[serde(default)]
    pub(crate) mutation_revision: u64,
    #[serde(default)]
    pub(crate) boundaries: ObservationBoundarySummary,
    pub(crate) text: String,
}

impl AccessibilitySnapshot {
    pub fn format(&self) -> String {
        let mut output = format!(
            "url: {}\ntitle: {}\nreadyState: {}\n\n{}",
            self.page.url,
            self.page.title,
            self.page.ready_state,
            format_tree(&self.roots, 0)
        );
        if !self.interactive.is_empty() {
            output.push_str("\nInteractive elements:\n");
            for element in &self.interactive {
                output.push_str(&format!(
                    "{} [{}] {}\n",
                    element.reference, element.role, element.name
                ));
            }
        }
        output
    }
}

/// Bounded JSON bundle produced on action/wait failure.
///
/// Contains the last compact observe context, action outcome, and topology
/// summary — enough for an agent to self-correct without requesting a full
/// DOM or screenshot. Redaction removes DOM/expression evidence, secrets,
/// and raw screenshots. Total payload is capped at 8 KiB.
#[derive(Debug, Clone, Serialize)]
pub struct FailureTracePack {
    /// The action or wait outcome that triggered this trace.
    pub outcome: ActionOutcome,
    /// Bounded error message (≤ 512 bytes, redacted for secrets).
    pub error: String,
    /// Last compact observation context available at the time of failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_observation: Option<CompactObservationTrace>,
    /// Active topology snapshot at the time of failure.
    pub topology: TopologyTrace,
    /// Total byte size of this trace pack.
    pub trace_bytes: usize,
}

/// Bounded subset of a compact observation for failure-trace purposes.
#[derive(Debug, Clone, Serialize)]
pub struct CompactObservationTrace {
    pub page: PageInfo,
    pub revision: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interactive: Vec<CompactInteractiveElement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completeness: Option<ObservationCompleteness>,
}

/// Bounded topology snapshot for failure-trace purposes.
#[derive(Debug, Clone, Serialize)]
pub struct TopologyTrace {
    pub sequence: u64,
    pub active_target_id: Option<String>,
    pub active_frame_id: Option<String>,
    pub target_count: usize,
    pub frame_count: usize,
    pub event_loss_count: u64,
}

/// A browser process, one CDP page connection, and its profile state.
pub struct BrowserSession {
    pub(crate) cdp: CdpClient,
    pub(crate) chrome: Option<ChromeProcess>,
    pub(crate) disposable_profile: Option<DisposableProfileDir>,
    pub(crate) launched_incognito_context_id: Option<String>,
    pub(crate) profile: String,
    pub(crate) interaction_mode: InteractionMode,
    pub(crate) mouse: MouseEngine,
    pub(crate) pointer: Mutex<Option<Point>>,
    pub(crate) page_revision: Arc<AtomicU64>,
    pub(crate) observation_cache: Mutex<Option<CachedObservation>>,
    pub(crate) network_wait_leases: Arc<Mutex<NetworkLeaseState>>,
    pub(crate) diagnostic_leases: Arc<Mutex<DiagnosticLeaseState>>,
    pub(crate) download_scope: Arc<Mutex<()>>,
    pub(crate) topology: Arc<Mutex<TopologyRegistry>>,
    pub(crate) popup_click_scope: Mutex<()>,
    pub(crate) upload_root: PathBuf,
    pub(crate) policy: BrowserPolicy,
    pub(crate) policy_interception: Option<PolicyInterception>,
    pub(crate) audit_log: std::sync::Mutex<VecDeque<AuditEntry>>,
    pub(crate) audit_sequence: AtomicU64,
    pub(crate) audit_enabled: bool,
}

pub(crate) struct CachedObservation {
    pub(crate) revision: u64,
    pub(crate) context: CompactPageContext,
}

pub(crate) type PausedPolicyRequests = Arc<Mutex<HashSet<(Option<String>, String)>>>;

pub(crate) struct PolicyInterception {
    pub(crate) cdp: CdpClient,
    pub(crate) sessions: Arc<Mutex<HashSet<String>>>,
    pub(crate) paused: PausedPolicyRequests,
    pub(crate) last_denial: Arc<Mutex<Option<PolicyError>>>,
    pub(crate) worker: tokio::task::JoinHandle<()>,
}

impl PolicyInterception {
    pub(crate) async fn start(
        cdp: CdpClient,
        policy: BrowserPolicy,
        initial_session: String,
    ) -> BrowserResult<Self> {
        let mut events = cdp.subscribe_events_with_params();
        let sessions = Arc::new(Mutex::new(HashSet::from([initial_session.clone()])));
        let paused = Arc::new(Mutex::new(HashSet::new()));
        let last_denial = Arc::new(Mutex::new(None));
        let worker_cdp = cdp.clone();
        let worker_sessions = Arc::clone(&sessions);
        let worker_paused = Arc::clone(&paused);
        let worker_denial = Arc::clone(&last_denial);
        let worker = tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        *worker_denial.lock().await = Some(PolicyError::Denied {
                            operation: "navigation".to_string(),
                            reason: format!(
                                "policy event stream lagged by {count}; paused requests remain blocked"
                            ),
                        });
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                if event.method == "Target.attachedToTarget" {
                    if let Some(session_id) = event.params["sessionId"].as_str() {
                        let session_id = session_id.to_string();
                        if enable_fetch_for(&worker_cdp, &session_id).await.is_ok() {
                            worker_sessions.lock().await.insert(session_id.clone());
                            let _ = worker_cdp
                                .send_to_session(
                                    &session_id,
                                    "Runtime.runIfWaitingForDebugger",
                                    None,
                                )
                                .await;
                        }
                    }
                    continue;
                }
                if event.method != "Fetch.requestPaused" {
                    continue;
                }
                let Some(request_id) = event.params["requestId"].as_str() else {
                    continue;
                };
                let request_id = request_id.to_string();
                let key = (event.session_id.clone(), request_id.clone());
                worker_paused.lock().await.insert(key.clone());
                let url = event.params["request"]["url"].as_str().unwrap_or_default();
                let decision = policy.require_url(url).await;
                let (method, params) = match decision {
                    Ok(_) => (
                        "Fetch.continueRequest",
                        serde_json::json!({"requestId": &request_id}),
                    ),
                    Err(error) => {
                        *worker_denial.lock().await = Some(error);
                        (
                            "Fetch.failRequest",
                            serde_json::json!({
                                "requestId": &request_id,
                                "errorReason": "BlockedByClient"
                            }),
                        )
                    }
                };
                let _ = match event.session_id.as_deref() {
                    Some(session_id) => {
                        worker_cdp
                            .send_to_session(session_id, method, Some(params))
                            .await
                    }
                    None => worker_cdp.send(method, Some(params)).await,
                };
                worker_paused.lock().await.remove(&key);
            }
        });
        if let Err(error) = enable_fetch_for(&cdp, &initial_session).await {
            worker.abort();
            return Err(error);
        }
        Ok(Self {
            cdp,
            sessions,
            paused,
            last_denial,
            worker,
        })
    }

    async fn take_denial(&self) -> Option<PolicyError> {
        self.last_denial.lock().await.take()
    }

    async fn shutdown(self) {
        for (session_id, request_id) in self.paused.lock().await.clone() {
            let params = Some(serde_json::json!({
                "requestId": request_id,
                "errorReason": "Aborted"
            }));
            let _ = match session_id.as_deref() {
                Some(session_id) => {
                    self.cdp
                        .send_to_session(session_id, "Fetch.failRequest", params)
                        .await
                }
                None => self.cdp.send("Fetch.failRequest", params).await,
            };
        }
        for session_id in self.sessions.lock().await.clone() {
            let _ = disable_fetch_for(&self.cdp, Some(&session_id)).await;
        }
        self.worker.abort();
    }
}

/// A unique user-data directory owned by an incognito Glass session.
///
/// Chrome still receives `--incognito`; the fresh directory also prevents it
/// from inheriting a user's default browser profile or leaving state behind
/// after a normal Glass shutdown.
#[derive(Debug)]
pub(crate) struct DisposableProfileDir {
    pub(crate) path: PathBuf,
}

pub(crate) const DISPOSABLE_OWNER_FILE: &str = ".glass-owner.json";
pub(crate) const DISPOSABLE_CLEANUP_BATCH: usize = 1024;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DisposableProfileOwner {
    pub(crate) pid: u32,
    pub(crate) process_start: u64,
}

impl DisposableProfileDir {
    fn create() -> BrowserResult<Self> {
        static NEXT_DISPOSABLE_PROFILE: AtomicU64 = AtomicU64::new(0);

        let root = std::env::temp_dir().join("glass");
        std::fs::create_dir_all(&root)?;
        Self::cleanup_abandoned(&root)?;
        let pid = std::process::id();
        let process_start = process_start_identity(pid)
            .ok_or("could not determine Glass process start identity")?;
        for _ in 0..32 {
            let sequence = NEXT_DISPOSABLE_PROFILE.fetch_add(1, Ordering::Relaxed);
            let nonce = format!(
                "{}-{}-{sequence}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            );
            let path = root.join(format!("incognito-{nonce}"));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let owner = DisposableProfileOwner { pid, process_start };
                    let owner_json = serde_json::to_vec(&owner)?;
                    if let Err(error) = std::fs::write(path.join(DISPOSABLE_OWNER_FILE), owner_json)
                    {
                        let _ = std::fs::remove_dir_all(&path);
                        return Err(error.into());
                    }
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("could not allocate a unique incognito user-data directory".into())
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn cleanup_abandoned(root: &Path) -> BrowserResult<()> {
        let mut candidates = Vec::new();
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir()
                || !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("incognito-")
            {
                continue;
            }
            let bytes = match std::fs::read(entry.path().join(DISPOSABLE_OWNER_FILE)) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let owner = match serde_json::from_slice::<DisposableProfileOwner>(&bytes) {
                Ok(owner) if owner.pid != 0 && owner.process_start != 0 => owner,
                _ => continue,
            };
            candidates.push((entry.path(), owner));
            if candidates.len() == DISPOSABLE_CLEANUP_BATCH {
                reap_disposable_candidates(&mut candidates)?;
            }
        }
        reap_disposable_candidates(&mut candidates)
    }
}

pub(crate) fn reap_disposable_candidates(
    candidates: &mut Vec<(PathBuf, DisposableProfileOwner)>,
) -> BrowserResult<()> {
    if candidates.is_empty() {
        return Ok(());
    }
    let pids = candidates
        .iter()
        .map(|(_, owner)| Pid::from_u32(owner.pid))
        .collect::<Vec<_>>();
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&pids), true);
    for (path, owner) in candidates.drain(..) {
        let live_start = system
            .process(Pid::from_u32(owner.pid))
            .map(|process| process.start_time());
        if live_start != Some(owner.process_start)
            && let Err(error) = std::fs::remove_dir_all(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error.into());
        }
    }
    Ok(())
}

pub(crate) fn process_start_identity(pid: u32) -> Option<u64> {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.start_time())
}

impl Drop for DisposableProfileDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %self.path.display(), %error, "could not remove disposable incognito profile");
        }
    }
}

pub(crate) async fn wait_for_stable_popup_topology(
    topology: &Arc<Mutex<TopologyRegistry>>,
    snapshot: &PopupTopologySnapshot,
    candidate: &PopupCandidate,
    deadline: tokio::time::Instant,
    quiet_interval: Duration,
) -> Result<PopupCandidate, PopupClickError> {
    let mut quiet_since = tokio::time::Instant::now();
    let mut stable_state = None;
    loop {
        let (current, state) = {
            let topology = topology.lock().await;
            let current = assess_popup_topology(snapshot, &topology, true)?;
            (current, (topology.sequence, topology.event_loss_count))
        };
        if current.target.id != candidate.target.id {
            return Err(popup_typed_error(
                PopupClickErrorKind::PopupAmbiguous,
                "popup candidate changed during topology stabilization",
            ));
        }

        let now = tokio::time::Instant::now();
        if stable_state != Some(state) {
            stable_state = Some(state);
            quiet_since = now;
        } else if now.duration_since(quiet_since) >= quiet_interval {
            return Ok(current);
        }
        if now >= deadline {
            return Err(popup_typed_error(
                PopupClickErrorKind::TopologyLagged,
                "popup topology did not stabilize before the evidence deadline",
            ));
        }

        let quiet_remaining = quiet_interval.saturating_sub(now.duration_since(quiet_since));
        let deadline_remaining = deadline.saturating_duration_since(now);
        tokio::time::sleep(
            POPUP_TOPOLOGY_POLL_INTERVAL
                .min(quiet_remaining)
                .min(deadline_remaining),
        )
        .await;
    }
}

pub(crate) fn interactive_elements(roots: &[AxNode], revision: u64) -> Vec<InteractiveElement> {
    find_interactive_elements(roots)
        .into_iter()
        .filter_map(|node| {
            let backend_dom_node_id = node.backend_dom_node_id?;
            Some(InteractiveElement {
                reference: backend_node_reference(revision, backend_dom_node_id),
                role: node.role.clone(),
                name: node.name.clone(),
                description: node.description.clone(),
                backend_dom_node_id,
            })
        })
        .collect()
}

pub(crate) fn truncate_visible_text(text: &str, max_bytes: usize) -> String {
    truncate_visible_text_with_status(text, max_bytes).0
}

pub(crate) fn collect_diagnostic_event(
    event: &CdpEventWithParams,
    console: &mut Vec<ConsoleEvidence>,
    network: &mut Vec<NetworkEvidence>,
    request_indexes: &mut HashMap<String, usize>,
    dropped: &mut u64,
) {
    match event.method.as_str() {
        "Runtime.consoleAPICalled" => {
            push_bounded(
                console,
                ConsoleEvidence {
                    level: bounded_diagnostic_text(event.params["type"].as_str().unwrap_or("log")),
                    text: "[console arguments redacted]".to_string(),
                },
                dropped,
            );
        }
        "Log.entryAdded" => {
            let entry = &event.params["entry"];
            push_bounded(
                console,
                ConsoleEvidence {
                    level: bounded_diagnostic_text(entry["level"].as_str().unwrap_or("log")),
                    text: redact_diagnostic_text(entry["text"].as_str().unwrap_or("")),
                },
                dropped,
            );
        }
        "Network.requestWillBeSent" => {
            let Some(request_id) = event.params["requestId"].as_str() else {
                return;
            };
            if let Some(index) = request_indexes.get(request_id).copied() {
                network[index].redirect_count = network[index].redirect_count.saturating_add(1);
                return;
            }
            if network.len() >= MAX_DIAGNOSTIC_EVENTS {
                *dropped = dropped.saturating_add(1);
                return;
            }
            let request = &event.params["request"];
            let index = network.len();
            request_indexes.insert(request_id.to_string(), index);
            network.push(NetworkEvidence {
                request_id: bounded_diagnostic_text(request_id),
                method: bounded_diagnostic_text(request["method"].as_str().unwrap_or("")),
                url: redact_diagnostic_url(request["url"].as_str().unwrap_or("")),
                status: None,
                failure: None,
                safe_header_names: safe_header_names(&request["headers"]),
                redirect_count: u16::from(event.params.get("redirectResponse").is_some()),
            });
        }
        "Network.responseReceived" => {
            if let Some(index) = event.params["requestId"]
                .as_str()
                .and_then(|id| request_indexes.get(id))
                .copied()
            {
                network[index].status = event.params["response"]["status"]
                    .as_u64()
                    .and_then(|status| u16::try_from(status).ok());
            }
        }
        "Network.loadingFailed" => {
            if let Some(index) = event.params["requestId"]
                .as_str()
                .and_then(|id| request_indexes.get(id))
                .copied()
            {
                network[index].failure = Some(bounded_diagnostic_text(
                    event.params["errorText"]
                        .as_str()
                        .unwrap_or("request_failed"),
                ));
            }
        }
        _ => {}
    }
}

pub(crate) fn push_bounded<T>(values: &mut Vec<T>, value: T, dropped: &mut u64) {
    if values.len() < MAX_DIAGNOSTIC_EVENTS {
        values.push(value);
    } else {
        *dropped = dropped.saturating_add(1);
    }
}

pub(crate) fn bounded_diagnostic_text(value: &str) -> String {
    truncate_utf8_bytes(value, MAX_DIAGNOSTIC_TEXT_BYTES)
}

pub(crate) fn redact_diagnostic_text(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if ["authorization", "cookie", "password", "token", "secret"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return "[redacted sensitive console entry]".to_string();
    }
    let redacted = value
        .split_whitespace()
        .map(|part| {
            if part.starts_with("http://") || part.starts_with("https://") {
                redact_diagnostic_url(part)
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    bounded_diagnostic_text(&redacted)
}

pub(crate) fn truncate_utf8_bytes(value: &str, max_bytes: usize) -> String {
    let mut end = value.len().min(max_bytes);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

pub(crate) fn redact_diagnostic_url(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return truncate_utf8_bytes(
            value.split('?').next().unwrap_or(""),
            MAX_DIAGNOSTIC_URL_BYTES,
        );
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    if url.query().is_some() {
        let names = url
            .query_pairs()
            .map(|(name, _)| name.into_owned())
            .collect::<Vec<_>>();
        url.set_query(None);
        if !names.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(names.iter().map(|name| (name.as_str(), "[redacted]")));
        }
    }
    url.set_fragment(None);
    truncate_utf8_bytes(url.as_str(), MAX_DIAGNOSTIC_URL_BYTES)
}

pub(crate) fn safe_header_names(headers: &Value) -> Vec<String> {
    let mut names = headers
        .as_object()
        .into_iter()
        .flat_map(|headers| headers.keys())
        .filter(|name| {
            !matches!(
                name.to_ascii_lowercase().as_str(),
                "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
            )
        })
        .take(32)
        .map(|name| truncate_utf8_bytes(name, 128))
        .collect::<Vec<_>>();
    names.sort();
    names
}

pub(crate) fn finite_nonnegative_u64(value: &Value) -> u64 {
    value
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.min(u64::MAX as f64) as u64)
        .unwrap_or(0)
}

pub(crate) fn validate_visual_options(options: &VisualCaptureOptions) -> BrowserResult<()> {
    if !options.scale.is_finite() || !(0.1..=4.0).contains(&options.scale) {
        return Err("visual scale must be finite and between 0.1 and 4.0".into());
    }
    if options.full_page as u8 + options.clip.is_some() as u8 + options.target.is_some() as u8 > 1 {
        return Err("full-page, clip, and element capture are mutually exclusive".into());
    }
    if options.format == VisualFormat::Png && options.quality.is_some() {
        return Err("PNG capture does not accept quality".into());
    }
    if options.quality.is_some_and(|quality| quality > 100) {
        return Err("visual quality must be between 0 and 100".into());
    }
    if let Some(clip) = options.clip {
        for value in [clip.x, clip.y, clip.width, clip.height] {
            if !value.is_finite() {
                return Err("visual clip values must be finite".into());
            }
        }
        if clip.x < 0.0 || clip.y < 0.0 || clip.width <= 0.0 || clip.height <= 0.0 {
            return Err("visual clip must have non-negative origin and positive size".into());
        }
        validate_effective_visual_clip(Some(clip), options.scale)?;
    }
    Ok(())
}

pub(crate) fn validate_effective_visual_clip(
    clip: Option<VisualClip>,
    scale: f64,
) -> BrowserResult<()> {
    let Some(clip) = clip else { return Ok(()) };
    let width = clip.width * scale;
    let height = clip.height * scale;
    if width > 16_384.0 || height > 16_384.0 || width * height > MAX_VISUAL_PIXELS {
        return Err("visual output exceeds the 16384-axis or 8-megapixel budget".into());
    }
    Ok(())
}

pub(crate) fn visual_clips_match(first: VisualClip, second: VisualClip) -> bool {
    [
        (first.x, second.x),
        (first.y, second.y),
        (first.width, second.width),
        (first.height, second.height),
    ]
    .into_iter()
    .all(|(first, second)| (first - second).abs() <= 0.5)
}

pub(crate) fn visual_rect(value: &Value) -> BrowserResult<VisualClip> {
    let clip = VisualClip {
        x: value["x"].as_f64().unwrap_or(0.0),
        y: value["y"].as_f64().unwrap_or(0.0),
        width: value["width"].as_f64().ok_or("visual width was missing")?,
        height: value["height"]
            .as_f64()
            .ok_or("visual height was missing")?,
    };
    validate_visual_options(&VisualCaptureOptions {
        clip: Some(clip),
        ..VisualCaptureOptions::default()
    })?;
    Ok(clip)
}

pub(crate) fn visual_viewport_rect(value: &Value) -> BrowserResult<VisualClip> {
    visual_rect(&serde_json::json!({
        "x": value["pageX"].as_f64().unwrap_or(0.0),
        "y": value["pageY"].as_f64().unwrap_or(0.0),
        "width": value["clientWidth"].as_f64().ok_or("visual viewport width was missing")?,
        "height": value["clientHeight"].as_f64().ok_or("visual viewport height was missing")?
    }))
}

pub(crate) fn decoded_base64_len(value: &str) -> BrowserResult<usize> {
    if !value.len().is_multiple_of(4) {
        return Err("visual base64 payload had invalid length".into());
    }
    let padding = value
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    Ok(value.len() / 4 * 3 - padding)
}

pub(crate) async fn stop_screencast_for(
    cdp: &CdpClient,
    session_id: Option<&str>,
) -> BrowserResult<()> {
    match session_id {
        Some(session_id) => {
            cdp.send_to_session(session_id, "Page.stopScreencast", None)
                .await?;
        }
        None => {
            cdp.send("Page.stopScreencast", None).await?;
        }
    }
    Ok(())
}

pub(crate) async fn disable_fetch_for(
    cdp: &CdpClient,
    session_id: Option<&str>,
) -> BrowserResult<()> {
    match session_id {
        Some(session_id) => {
            cdp.send_to_session(session_id, "Fetch.disable", None)
                .await?
        }
        None => cdp.send("Fetch.disable", None).await?,
    };
    Ok(())
}

pub(crate) async fn enable_fetch_for(cdp: &CdpClient, session_id: &str) -> BrowserResult<()> {
    cdp.send_to_session(
        session_id,
        "Fetch.enable",
        Some(serde_json::json!({
            "patterns": [{
                "urlPattern": "*",
                "resourceType": "Document",
                "requestStage": "Request"
            }]
        })),
    )
    .await?;
    Ok(())
}

pub(crate) fn visual_quad_rect(value: &Value) -> BrowserResult<VisualClip> {
    let values = value
        .as_array()
        .ok_or("visual element border quad was missing")?;
    if values.len() != 8 {
        return Err("visual element border quad must contain eight coordinates".into());
    }
    let coordinates = values
        .iter()
        .map(|value| {
            value
                .as_f64()
                .ok_or("visual border coordinate was not numeric")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let xs = [
        coordinates[0],
        coordinates[2],
        coordinates[4],
        coordinates[6],
    ];
    let ys = [
        coordinates[1],
        coordinates[3],
        coordinates[5],
        coordinates[7],
    ];
    let left = xs.into_iter().fold(f64::INFINITY, f64::min);
    let right = xs.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let top = ys.into_iter().fold(f64::INFINITY, f64::min);
    let bottom = ys.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let width = right - left;
    let height = bottom - top;
    if width <= 0.0 || height <= 0.0 {
        return Err("visual element has empty geometry".into());
    }
    Ok(VisualClip {
        x: left,
        y: top,
        width,
        height,
    })
}

pub(crate) fn truncate_visible_text_with_status(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }

    let content_limit = max_bytes.saturating_sub(TEXT_TRUNCATION_MARKER.len());
    let mut end = content_limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    let mut truncated = text[..end].to_string();
    if max_bytes >= TEXT_TRUNCATION_MARKER.len() {
        truncated.push_str(TEXT_TRUNCATION_MARKER);
    }
    (truncated, true)
}

pub(crate) fn interaction_path(
    mode: InteractionMode,
    mouse: &MouseEngine,
    start: Point,
    end: Point,
) -> Vec<Point> {
    match mode {
        InteractionMode::Human => mouse.generate_path(start, end),
        InteractionMode::Fast => vec![start, end],
    }
}

pub(crate) fn validate_key(key: &str) -> BrowserResult<()> {
    if key.is_empty() || key.len() > 64 || key.chars().any(char::is_control) {
        return Err("key must be 1..=64 printable UTF-8 bytes".into());
    }
    Ok(())
}

pub(crate) fn key_code(key: &str) -> String {
    match key {
        " " => "Space".to_string(),
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Enter" | "Tab" | "Escape"
        | "Backspace" | "Delete" | "Home" | "End" | "PageUp" | "PageDown" => key.to_string(),
        _ if key.chars().count() == 1 => {
            let character = key.chars().next().unwrap();
            if character.is_ascii_alphabetic() {
                format!("Key{}", character.to_ascii_uppercase())
            } else if character.is_ascii_digit() {
                format!("Digit{character}")
            } else {
                key.to_string()
            }
        }
        _ => key.to_string(),
    }
}

pub(crate) fn parse_shortcut(value: &str) -> BrowserResult<(i64, String)> {
    if value.is_empty() || value.len() > 256 {
        return Err("shortcut must be 1..=256 bytes".into());
    }
    let mut modifiers = 0;
    let mut key = None;
    for part in value.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "alt" => modifiers |= 1,
            "control" | "ctrl" => modifiers |= 2,
            "meta" | "cmd" | "command" => modifiers |= 4,
            "shift" => modifiers |= 8,
            _ if key.is_none() => key = Some(part.to_string()),
            _ => return Err("shortcut must contain exactly one non-modifier key".into()),
        }
    }
    let key = key.ok_or("shortcut requires a non-modifier key")?;
    validate_key(&key)?;
    Ok((modifiers, key))
}

pub(crate) fn context_event_invalidates_observation(method: &str) -> bool {
    matches!(
        method,
        "Page.frameNavigated"
            | "Page.loadEventFired"
            | "Page.frameStartedLoading"
            | "Page.frameStoppedLoading"
            | "DOM.documentUpdated"
            | "DOM.childNodeInserted"
            | "DOM.childNodeRemoved"
            | "DOM.attributeModified"
            | "DOM.attributeRemoved"
            | "DOM.characterDataModified"
            | "DOM.setChildNodes"
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedElement {
    pub node_id: Option<i64>,
    pub backend_dom_node_id: Option<i64>,
    pub label: String,
    pub reference: Option<String>,
}

pub(crate) struct PressedButtonGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) point: Point,
    pub(crate) click_count: u32,
    pub(crate) armed: bool,
}

pub(crate) struct RemoteObjectGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) object_id: String,
}

pub(crate) struct PopupWitnessGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) session_id: String,
    pub(crate) state_object_id: String,
    pub(crate) element_object_id: String,
    pub(crate) armed: bool,
}

pub(crate) struct PopupAttachmentGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) session_id: String,
    pub(crate) armed: bool,
}

pub(crate) struct NetworkDomainGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) leases: Arc<Mutex<NetworkLeaseState>>,
    pub(crate) session_id: Option<String>,
    pub(crate) armed: bool,
}

pub(crate) struct DiagnosticDomainGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) network: NetworkDomainGuard,
    pub(crate) leases: Arc<Mutex<DiagnosticLeaseState>>,
    pub(crate) session_id: Option<String>,
    pub(crate) armed: bool,
}

pub(crate) struct DownloadBehaviorGuard {
    pub(crate) cdp: CdpClient,
    pub(crate) page_session_id: Option<String>,
    pub(crate) armed: bool,
}

#[derive(Default)]
pub(crate) struct DiagnosticLeaseState {
    pub(crate) counts: HashMap<Option<String>, usize>,
}

#[derive(Default)]
pub(crate) struct NetworkLeaseState {
    pub(crate) counts: HashMap<Option<String>, usize>,
}

impl NetworkDomainGuard {
    pub(crate) async fn acquire(
        cdp: CdpClient,
        leases: Arc<Mutex<NetworkLeaseState>>,
    ) -> BrowserResult<Self> {
        let session_id = cdp.current_session_id();
        let mut state = leases.lock().await;
        let count = state.counts.entry(session_id.clone()).or_default();
        *count += 1;
        let mut guard = Self {
            cdp,
            leases: Arc::clone(&leases),
            session_id: session_id.clone(),
            armed: true,
        };
        if *count == 1
            && let Err(error) = guard
                .cdp
                .set_domain_enabled_for(session_id.clone(), "Network", true)
                .await
        {
            state.counts.remove(&session_id);
            guard.armed = false;
            drop(state);
            let _ = guard
                .cdp
                .set_domain_enabled_for(session_id, "Network", false)
                .await;
            return Err(error.into());
        }
        drop(state);
        Ok(guard)
    }

    pub(crate) async fn disable(&mut self) -> BrowserResult<()> {
        let state = self.leases.lock().await;
        release_network_lease_locked(&self.cdp, &self.session_id, state).await?;
        self.armed = false;
        Ok(())
    }
}

impl DiagnosticDomainGuard {
    pub(crate) async fn acquire(
        cdp: CdpClient,
        network_leases: Arc<Mutex<NetworkLeaseState>>,
        leases: Arc<Mutex<DiagnosticLeaseState>>,
    ) -> BrowserResult<Self> {
        let network = NetworkDomainGuard::acquire(cdp.clone(), network_leases).await?;
        let session_id = cdp.current_session_id();
        let mut state = leases.lock().await;
        let count = state.counts.entry(session_id.clone()).or_default();
        *count += 1;
        if *count == 1 {
            if let Err(error) = cdp
                .set_domain_enabled_for(session_id.clone(), "Runtime", true)
                .await
            {
                state.counts.remove(&session_id);
                return Err(error.into());
            }
            if let Err(error) = cdp
                .set_domain_enabled_for(session_id.clone(), "Log", true)
                .await
            {
                state.counts.remove(&session_id);
                let _ = cdp
                    .set_domain_enabled_for(session_id.clone(), "Runtime", false)
                    .await;
                return Err(error.into());
            }
        }
        drop(state);
        Ok(Self {
            cdp,
            network,
            leases,
            session_id,
            armed: true,
        })
    }

    pub(crate) async fn disable(&mut self) -> BrowserResult<()> {
        let mut state = self.leases.lock().await;
        let count = state.counts.entry(self.session_id.clone()).or_default();
        *count = count.saturating_sub(1);
        if *count == 0 {
            state.counts.remove(&self.session_id);
            self.cdp
                .set_domain_enabled_for(self.session_id.clone(), "Log", false)
                .await?;
            self.cdp
                .set_domain_enabled_for(self.session_id.clone(), "Runtime", false)
                .await?;
        }
        drop(state);
        self.network.disable().await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for DiagnosticDomainGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let leases = Arc::clone(&self.leases);
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let mut state = leases.lock().await;
            let count = state.counts.entry(session_id.clone()).or_default();
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.counts.remove(&session_id);
                let _ = cdp
                    .set_domain_enabled_for(session_id.clone(), "Log", false)
                    .await;
                let _ = cdp
                    .set_domain_enabled_for(session_id, "Runtime", false)
                    .await;
            }
        });
    }
}

impl DownloadBehaviorGuard {
    pub(crate) async fn acquire_for_incognito(
        cdp: CdpClient,
        destination: PathBuf,
        target_id: String,
        page_session_id: String,
        launched_context_id: String,
    ) -> BrowserResult<Self> {
        let selected_context_id = target_browser_context_id(&cdp, &target_id, true).await?;
        if selected_context_id.as_deref() != Some(launched_context_id.as_str()) {
            return Err(download_error(
                DownloadErrorKind::AuthorizationFailed,
                "selected target does not belong to the launched incognito context",
            )
            .into());
        }
        Self::acquire(cdp, destination, Some(page_session_id)).await
    }

    pub(crate) async fn acquire(
        cdp: CdpClient,
        destination: PathBuf,
        page_session_id: Option<String>,
    ) -> BrowserResult<Self> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = Self::acquire_owned(cdp, &destination, page_session_id).await;
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                download_error(
                    DownloadErrorKind::AuthorizationFailed,
                    "download authorization worker ended without a result",
                )
            })?
            .map_err(Into::into)
    }

    pub(crate) async fn acquire_owned(
        cdp: CdpClient,
        destination: &Path,
        page_session_id: Option<String>,
    ) -> Result<Self, DownloadError> {
        cdp.set_download_behavior("allow", Some(destination), true)
            .await
            .map_err(|error| {
                download_error(
                    DownloadErrorKind::AuthorizationFailed,
                    format!("browser download authorization failed: {error}"),
                )
            })?;
        if let Some(session_id) = page_session_id.as_deref()
            && let Err(error) = cdp
                .send_to_session(
                    session_id,
                    "Page.setDownloadBehavior",
                    Some(serde_json::json!({
                        "behavior": "allow",
                        "downloadPath": destination.to_string_lossy()
                    })),
                )
                .await
        {
            let restoration = cdp.set_download_behavior("deny", None, false).await;
            return Err(download_error(
                DownloadErrorKind::AuthorizationFailed,
                format!(
                    "incognito page download authorization failed: {error}; browser deny restoration: {}",
                    restoration
                        .map(|_| "completed".to_string())
                        .unwrap_or_else(|restore| restore.to_string())
                ),
            ));
        }
        Ok(Self {
            cdp,
            page_session_id,
            armed: true,
        })
    }

    pub(crate) async fn disable(&mut self) -> BrowserResult<()> {
        let page_result = match self.page_session_id.as_deref() {
            Some(session_id) => self
                .cdp
                .send_to_session(
                    session_id,
                    "Page.setDownloadBehavior",
                    Some(serde_json::json!({"behavior": "deny"})),
                )
                .await
                .map(|_| ()),
            None => Ok(()),
        };
        let browser_result = self
            .cdp
            .set_download_behavior("deny", None, false)
            .await
            .map(|_| ());
        match (page_result, browser_result) {
            (Ok(()), Ok(())) => {
                self.armed = false;
                Ok(())
            }
            (page, browser) => Err(download_error(
                DownloadErrorKind::RestorationFailed,
                format!(
                    "download deny restoration failed (page: {}; browser: {})",
                    protocol_result_summary(page),
                    protocol_result_summary(browser)
                ),
            )
            .into()),
        }
    }
}

impl Drop for DownloadBehaviorGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let page_session_id = self.page_session_id.clone();
        tokio::spawn(async move {
            if let Some(session_id) = page_session_id.as_deref() {
                let _ = cdp
                    .send_to_session(
                        session_id,
                        "Page.setDownloadBehavior",
                        Some(serde_json::json!({"behavior": "deny"})),
                    )
                    .await;
            }
            let _ = cdp.set_download_behavior("deny", None, false).await;
        });
    }
}

impl Drop for NetworkDomainGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let leases = Arc::clone(&self.leases);
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = release_network_lease(&cdp, &leases, &session_id).await;
        });
    }
}

pub(crate) async fn release_network_lease(
    cdp: &CdpClient,
    leases: &Mutex<NetworkLeaseState>,
    session_id: &Option<String>,
) -> BrowserResult<()> {
    release_network_lease_locked(cdp, session_id, leases.lock().await).await
}

pub(crate) async fn release_network_lease_locked(
    cdp: &CdpClient,
    session_id: &Option<String>,
    mut state: tokio::sync::MutexGuard<'_, NetworkLeaseState>,
) -> BrowserResult<()> {
    let count = state.counts.entry(session_id.clone()).or_default();
    *count = count.saturating_sub(1);
    if *count == 0 {
        state.counts.remove(session_id);
        cdp.set_domain_enabled_for(session_id.clone(), "Network", false)
            .await?;
    }
    Ok(())
}

impl Drop for RemoteObjectGuard {
    fn drop(&mut self) {
        let cdp = self.cdp.clone();
        let object_id = self.object_id.clone();
        tokio::spawn(async move {
            let _ = cdp.release_object(&object_id).await;
        });
    }
}

impl PopupWitnessGuard {
    pub(crate) async fn fired(&self) -> BrowserResult<bool> {
        let value = popup_verification_call(
            self.cdp.send_to_session(
                &self.session_id,
                "Runtime.callFunctionOn",
                Some(serde_json::json!({
                    "objectId": self.state_object_id,
                    "functionDeclaration": "function(){return this.read();}",
                    "returnByValue": true,
                    "awaitPromise": false
                })),
            ),
            "popup witness read",
        )
        .await?;
        value["result"]["value"].as_bool().ok_or_else(|| {
            popup_error(
                PopupClickErrorKind::WitnessMissing,
                "popup witness returned no trusted-click state",
            )
        })
    }

    pub(crate) async fn cleanup(&mut self) -> BrowserResult<()> {
        cleanup_popup_witness(
            &self.cdp,
            &self.session_id,
            &self.state_object_id,
            &self.element_object_id,
        )
        .await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for PopupWitnessGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        let state_object_id = self.state_object_id.clone();
        let element_object_id = self.element_object_id.clone();
        tokio::spawn(async move {
            let _ = cleanup_popup_witness(&cdp, &session_id, &state_object_id, &element_object_id)
                .await;
        });
    }
}

impl PopupAttachmentGuard {
    pub(crate) async fn detach(&mut self) -> BrowserResult<()> {
        popup_verification_call(
            self.cdp.send_browser(
                "Target.detachFromTarget",
                Some(serde_json::json!({"sessionId": self.session_id})),
            ),
            "popup detach",
        )
        .await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for PopupAttachmentGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                POPUP_VERIFY_CALL_TIMEOUT,
                cdp.send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": session_id})),
                ),
            )
            .await;
        });
    }
}

impl Drop for PressedButtonGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let point = self.point;
        let click_count = self.click_count;
        tokio::spawn(async move {
            let _ = cdp
                .dispatch_mouse_event(
                    "mouseReleased",
                    point.x,
                    point.y,
                    Some("left"),
                    Some(click_count),
                )
                .await;
        });
    }
}

impl WaitCondition {
    pub fn parse(value: &str) -> BrowserResult<Self> {
        if value.len() > MAX_WAIT_CONDITION_BYTES {
            return Err("wait condition exceeds 4096 bytes".into());
        }
        let (kind, argument) = value
            .split_once('=')
            .ok_or("wait condition must use <kind>=<value>")?;
        if argument.is_empty() {
            return Err("wait condition value cannot be empty".into());
        }
        Ok(match kind {
            "lifecycle" if matches!(argument, "load" | "domcontentloaded" | "complete") => {
                Self::Lifecycle(
                    if argument == "load" {
                        "complete"
                    } else {
                        argument
                    }
                    .to_string(),
                )
            }
            "url" => Self::UrlExact(argument.to_string()),
            "url-prefix" => Self::UrlPrefix(argument.to_string()),
            "target-attached" => Self::TargetAttached(argument.to_string()),
            "target-visible" => Self::TargetVisible(argument.to_string()),
            "target-hidden" => Self::TargetHidden(argument.to_string()),
            "target-enabled" => Self::TargetEnabled(argument.to_string()),
            "target-stable" => Self::TargetStable(argument.to_string()),
            "text" => Self::Text(argument.to_string()),
            "js" => Self::JavaScript(argument.to_string()),
            "network-quiet" => {
                let duration = Duration::from_millis(argument.parse::<u64>()?);
                if duration.is_zero() || duration > MAX_WAIT_DEADLINE {
                    return Err("network quiet duration must be between 1 ms and 300000 ms".into());
                }
                Self::NetworkQuiet(duration)
            }
            "lifecycle" => return Err("unsupported lifecycle wait value".into()),
            _ => return Err("unknown wait condition kind".into()),
        })
    }

    pub(crate) fn description(&self) -> String {
        match self {
            Self::Lifecycle(_) => "lifecycle".to_string(),
            Self::UrlExact(_) => "url_exact".to_string(),
            Self::UrlPrefix(_) => "url_prefix".to_string(),
            Self::TargetAttached(_) => "target_attached".to_string(),
            Self::TargetVisible(_) => "target_visible".to_string(),
            Self::TargetHidden(_) => "target_hidden".to_string(),
            Self::TargetEnabled(_) => "target_enabled".to_string(),
            Self::TargetStable(_) => "target_stable".to_string(),
            Self::Text(_) => "text".to_string(),
            Self::JavaScript(_) => "javascript_predicate".to_string(),
            Self::NetworkQuiet(_) => "network_quiet".to_string(),
        }
    }

    pub(crate) fn validate(&self) -> BrowserResult<()> {
        let value = match self {
            Self::Lifecycle(value)
            | Self::UrlExact(value)
            | Self::UrlPrefix(value)
            | Self::TargetAttached(value)
            | Self::TargetVisible(value)
            | Self::TargetHidden(value)
            | Self::TargetEnabled(value)
            | Self::TargetStable(value)
            | Self::Text(value)
            | Self::JavaScript(value) => Some(value),
            Self::NetworkQuiet(duration) => {
                if duration.is_zero() || *duration > MAX_WAIT_DEADLINE {
                    return Err("network quiet duration must be between 1 ms and 300000 ms".into());
                }
                None
            }
        };
        if value.is_some_and(|value| value.is_empty() || value.len() > MAX_WAIT_CONDITION_BYTES) {
            return Err("wait condition value must contain 1-4096 bytes".into());
        }
        Ok(())
    }
}

pub(crate) fn bounded_wait_state(value: &str) -> String {
    truncate_visible_text(value, WAIT_LAST_STATE_MAX_BYTES)
}

pub(crate) fn validate_wait_deadline(deadline: Duration) -> BrowserResult<()> {
    if deadline.is_zero() || deadline > MAX_WAIT_DEADLINE {
        return Err("wait deadline must be between 1 ms and 300000 ms".into());
    }
    Ok(())
}

pub(crate) fn wait_timeout(condition: &str, deadline: Duration, last_state: &str) -> WaitTimeout {
    WaitTimeout {
        condition: condition.to_string(),
        deadline_ms: deadline.as_millis() as u64,
        last_state: bounded_wait_state(last_state),
        reason: "deadline_exceeded",
    }
}

impl Locator {
    pub fn parse(value: &str) -> BrowserResult<Self> {
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            return Err("element target cannot be empty".into());
        }
        if parse_revisioned_reference(value)?.is_some() {
            return Ok(Self::Reference(value.to_string()));
        }
        if let Some(reference) = value.strip_prefix("ref=") {
            if reference.is_empty() {
                return Err("reference locator cannot be empty".into());
            }
            return Ok(Self::Reference(reference.to_string()));
        }
        if let Some(name) = value.strip_prefix("name=") {
            return nonempty_locator(name, "accessible name").map(Self::AccessibleName);
        }
        if let Some(text) = value.strip_prefix("text=") {
            return nonempty_locator(text, "text").map(Self::Text);
        }
        if let Some(selector) = value.strip_prefix("css=") {
            return nonempty_locator(selector, "CSS selector").map(Self::Css);
        }
        if let Some(index) = value.strip_prefix("ordinal=") {
            let index = index
                .parse::<usize>()
                .ok()
                .filter(|index| *index > 0)
                .ok_or("ordinal locator must be a positive one-based integer")?;
            return Ok(Self::Ordinal(index));
        }
        if let Some(rest) = value.strip_prefix("role=") {
            let (role, name) = rest
                .split_once(";name=")
                .ok_or("role locator must use role=<role>;name=<accessible name>")?;
            return Ok(Self::RoleAndName {
                role: nonempty_locator(role, "role")?,
                name: nonempty_locator(name, "accessible name")?,
            });
        }
        Ok(Self::AccessibleName(value.to_string()))
    }
}

pub(crate) fn nonempty_locator(value: &str, kind: &str) -> BrowserResult<String> {
    if value.is_empty() {
        return Err(format!("{kind} locator cannot be empty").into());
    }
    Ok(value.to_string())
}

pub(crate) fn dom_nodes_resolution(
    count: usize,
    nodes: Vec<i64>,
    label: String,
    candidate_kind: &str,
) -> BrowserResult<TargetResolution> {
    match count {
        0 => Ok(TargetResolution::NotFound),
        1 if nodes.len() == 1 => Ok(TargetResolution::Unique(ResolvedElement {
            node_id: Some(nodes[0]),
            backend_dom_node_id: None,
            label,
            reference: None,
        })),
        1 => Err("unique element query returned no DOM node".into()),
        _ => Ok(TargetResolution::Ambiguous(
            nodes
                .into_iter()
                .take(AMBIGUOUS_CANDIDATE_LIMIT)
                .enumerate()
                .map(|(index, _)| CandidateSummary {
                    label: format!("{candidate_kind} {}", index + 1),
                    reference: None,
                })
                .collect(),
        )),
    }
}

pub(crate) fn css_query_expression(selector: &str) -> BrowserResult<String> {
    let selector = serde_json::to_string(selector)?;
    Ok(format!(
        "(() => {{ const nodes = document.querySelectorAll({selector}); const out = []; for (let i = 0; i < Math.min(nodes.length, {AMBIGUOUS_CANDIDATE_LIMIT}); i++) out.push(nodes[i]); out.glassCount = nodes.length; return out; }})()"
    ))
}

pub(crate) fn text_query_expression(text: &str) -> BrowserResult<String> {
    let text = serde_json::to_string(text)?;
    Ok(format!(
        r#"(() => {{
            pub(crate) const wanted = ({text}).replace(/\s+/g, ' ').trim();
            pub(crate) const matches = [];
            let count = 0;
            pub(crate) const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_ELEMENT);
            for (let element = walker.currentNode; element; element = walker.nextNode()) {{
                pub(crate) const style = getComputedStyle(element);
                pub(crate) const rect = element.getBoundingClientRect();
                if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || rect.width <= 0 || rect.height <= 0) continue;
                if (element.checkVisibility && !element.checkVisibility({{checkOpacity:true, checkVisibilityCSS:true}})) continue;
                let clipped = false;
                let visibleLeft = rect.left, visibleTop = rect.top, visibleRight = rect.right, visibleBottom = rect.bottom;
                for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) {{
                    pub(crate) const ancestorStyle = getComputedStyle(ancestor);
                    if (ancestorStyle.display === 'none' || ancestorStyle.visibility === 'hidden' || Number(ancestorStyle.opacity) === 0) {{ clipped = true; break; }}
                    if (/(hidden|clip)/.test(ancestorStyle.overflow + ancestorStyle.overflowX + ancestorStyle.overflowY)) {{
                        pub(crate) const bounds = ancestor.getBoundingClientRect();
                        visibleLeft = Math.max(visibleLeft, bounds.left); visibleTop = Math.max(visibleTop, bounds.top);
                        visibleRight = Math.min(visibleRight, bounds.right); visibleBottom = Math.min(visibleBottom, bounds.bottom);
                        if (visibleRight <= visibleLeft || visibleBottom <= visibleTop) {{ clipped = true; break; }}
                    }}
                }}
                if (clipped) continue;
                pub(crate) const actual = (element.innerText || '').replace(/\s+/g, ' ').trim();
                if (actual !== wanted) continue;
                pub(crate) const candidate = element.closest('button,a,input,select,textarea,[role],[tabindex]') || element;
                if (matches.includes(candidate)) continue;
                for (let index = matches.length - 1; index >= 0; index--) {{
                    if (matches[index].contains(candidate)) {{ matches.splice(index, 1); count--; }}
                }}
                count++;
                if (matches.length < {AMBIGUOUS_CANDIDATE_LIMIT}) matches.push(candidate);
            }}
            matches.glassCount = count;
            return matches;
        }})()"#
    ))
}

pub(crate) fn visible_text_contains_expression(text: &str) -> BrowserResult<String> {
    let text = serde_json::to_string(text)?;
    Ok(format!(
        r#"(() => {{
            pub(crate) const wanted = {text};
            pub(crate) const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
            for (let node = walker.nextNode(); node; node = walker.nextNode()) {{
                if (!(node.nodeValue || '').includes(wanted)) continue;
                pub(crate) const element = node.parentElement;
                if (!element) continue;
                if (element.checkVisibility && !element.checkVisibility({{checkOpacity:true, checkVisibilityCSS:true}})) continue;
                pub(crate) const rect = element.getBoundingClientRect();
                if (rect.width <= 0 || rect.height <= 0) continue;
                let left = rect.left, top = rect.top, right = rect.right, bottom = rect.bottom, hidden = false;
                for (let ancestor = element; ancestor; ancestor = ancestor.parentElement) {{
                    pub(crate) const style = getComputedStyle(ancestor);
                    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) {{ hidden = true; break; }}
                    if (/(hidden|clip)/.test(style.overflow + style.overflowX + style.overflowY)) {{
                        pub(crate) const bounds = ancestor.getBoundingClientRect();
                        left = Math.max(left, bounds.left); top = Math.max(top, bounds.top);
                        right = Math.min(right, bounds.right); bottom = Math.min(bottom, bounds.bottom);
                        if (right <= left || bottom <= top) {{ hidden = true; break; }}
                    }}
                }}
                if (!hidden) return true;
            }}
            return false;
        }})()"#
    ))
}

pub(crate) fn bounded_candidate_label(value: &str) -> String {
    if value.len() <= CANDIDATE_LABEL_MAX_BYTES {
        return value.to_string();
    }
    let mut end = CANDIDATE_LABEL_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

pub(crate) fn validate_topology_id(value: &str) -> BrowserResult<()> {
    if value.is_empty() {
        return Err("topology ID cannot be empty".into());
    }
    if value.len() > TOPOLOGY_ID_MAX_BYTES {
        return Err(format!("topology ID exceeds {TOPOLOGY_ID_MAX_BYTES} UTF-8 bytes").into());
    }
    Ok(())
}

pub(crate) fn bounded_topology_text(value: &str) -> String {
    if value.len() <= TOPOLOGY_TEXT_MAX_BYTES {
        return value.to_string();
    }
    let mut end = TOPOLOGY_TEXT_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

pub(crate) fn collect_frames(
    frame_tree: &Value,
    parent_id: Option<&str>,
    active_frame_id: Option<&str>,
    frames: &mut Vec<FrameInfo>,
) -> BrowserResult<()> {
    if frames.len() == TOPOLOGY_MAX_FRAMES {
        return Err("frame limit exceeded".into());
    }
    let frame = frame_tree
        .get("frame")
        .ok_or("Page.getFrameTree returned a node without frame data")?;
    let id = frame["id"]
        .as_str()
        .ok_or("Page.getFrameTree returned a frame without an ID")?;
    validate_topology_id(id)?;
    frames.push(FrameInfo {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        url: bounded_topology_text(frame["url"].as_str().unwrap_or_default()),
        active: active_frame_id == Some(id),
        out_of_process: false,
    });
    if let Some(children) = frame_tree["childFrames"].as_array() {
        for child in children {
            collect_frames(child, Some(id), active_frame_id, frames)?;
        }
    }
    Ok(())
}

pub(crate) fn push_topology_event(topology: &mut TopologyRegistry, kind: &str, id: &str) -> u64 {
    topology.sequence = topology
        .sequence
        .checked_add(1)
        .expect("topology sequence exhausted");
    let sequence = topology.sequence;
    topology.events.push_back(TopologyEventSummary {
        sequence,
        kind: kind.to_string(),
        id: bounded_topology_id(id),
    });
    while topology.events.len() > TOPOLOGY_MAX_EVENTS {
        topology.events.pop_front();
    }
    sequence
}

pub(crate) fn bounded_topology_id(value: &str) -> String {
    if value.len() <= TOPOLOGY_ID_MAX_BYTES {
        return value.to_string();
    }
    let mut end = TOPOLOGY_ID_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

pub(crate) fn retained_optional_topology_id(value: Option<&str>) -> BrowserResult<Option<String>> {
    value
        .map(|id| {
            validate_topology_id(id)?;
            Ok(id.to_string())
        })
        .transpose()
}

pub(crate) fn popup_typed_error(
    kind: PopupClickErrorKind,
    message: impl Into<String>,
) -> PopupClickError {
    let message = message.into();
    PopupClickError {
        kind,
        message: truncate_utf8_bytes(&message, POPUP_ERROR_MESSAGE_MAX_BYTES),
    }
}

pub(crate) fn popup_witness_install_function() -> String {
    format!(
        r#"function() {{
            pub(crate) const element = this;
            pub(crate) const nativeAdd = EventTarget.prototype.addEventListener;
            pub(crate) const nativeRemove = EventTarget.prototype.removeEventListener;
            pub(crate) const nativeApply = Reflect.apply;
            pub(crate) const nativeSetTimeout = setTimeout;
            pub(crate) const nativeClearTimeout = clearTimeout;
            pub(crate) const state = {{fired:false, cleaned:false}};
            pub(crate) const listener = function(event) {{
                if (event.isTrusted === true && event.currentTarget === element) state.fired = true;
            }};
            let timer;
            pub(crate) const cleanup = function() {{
                if (state.cleaned) return true;
                state.cleaned = true;
                nativeApply(nativeRemove, element, ['click', listener, true]);
                nativeClearTimeout(timer);
                return true;
            }};
            Object.defineProperties(state, {{
                read: {{value:function(){{return state.fired;}}, enumerable:false}},
                cleanup: {{value:cleanup, enumerable:false}}
            }});
            nativeApply(nativeAdd, element, ['click', listener, {{capture:true, once:true}}]);
            timer = nativeSetTimeout(cleanup, {POPUP_WITNESS_LIFETIME_MS});
            return state;
        }}"#
    )
}

pub(crate) async fn popup_witness_call<F>(future: F, step: &str) -> Result<Value, PopupClickError>
where
    F: std::future::Future<Output = Result<Value, crate::browser::cdp::CdpError>>,
{
    match tokio::time::timeout(POPUP_VERIFY_CALL_TIMEOUT, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            format!("{step} failed: {error}"),
        )),
        Err(_) => Err(popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            format!("{step} exceeded its bounded deadline"),
        )),
    }
}

pub(crate) async fn arm_popup_witness_owned(
    cdp: CdpClient,
    session_id: String,
    frame_id: String,
    backend_node_id: i64,
) -> Result<PopupWitnessGuard, PopupClickError> {
    let world = popup_witness_call(
        cdp.send_to_session(
            &session_id,
            "Page.createIsolatedWorld",
            Some(serde_json::json!({
                "frameId": frame_id,
                "worldName": "glass-popup-witness"
            })),
        ),
        "popup witness isolated world",
    )
    .await?;
    let context_id = world["executionContextId"].as_i64().ok_or_else(|| {
        popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            "popup witness isolated world returned no context ID",
        )
    })?;
    let resolved = popup_witness_call(
        cdp.send_to_session(
            &session_id,
            "DOM.resolveNode",
            Some(serde_json::json!({
                "backendNodeId": backend_node_id,
                "executionContextId": context_id
            })),
        ),
        "popup witness exact-node resolution",
    )
    .await?;
    let element_object_id = resolved["object"]["objectId"]
        .as_str()
        .ok_or_else(|| {
            popup_typed_error(
                PopupClickErrorKind::WitnessMissing,
                "popup witness exact-node resolution returned no object",
            )
        })?
        .to_string();
    let install = popup_witness_call(
        cdp.send_to_session(
            &session_id,
            "Runtime.callFunctionOn",
            Some(serde_json::json!({
                "objectId": element_object_id,
                "functionDeclaration": popup_witness_install_function(),
                "returnByValue": false,
                "awaitPromise": false
            })),
        ),
        "popup witness installation",
    )
    .await;
    let state_object_id = match install {
        Ok(value) => match value["result"]["objectId"].as_str() {
            Some(id) => id.to_string(),
            None => {
                let _ = tokio::time::timeout(
                    POPUP_VERIFY_CALL_TIMEOUT,
                    cdp.release_object_for_session(&session_id, &element_object_id),
                )
                .await;
                return Err(popup_typed_error(
                    PopupClickErrorKind::WitnessMissing,
                    "popup witness installation returned no private state",
                ));
            }
        },
        Err(error) => {
            let _ = tokio::time::timeout(
                POPUP_VERIFY_CALL_TIMEOUT,
                cdp.release_object_for_session(&session_id, &element_object_id),
            )
            .await;
            return Err(error);
        }
    };
    Ok(PopupWitnessGuard {
        cdp,
        session_id,
        state_object_id,
        element_object_id,
        armed: true,
    })
}

pub(crate) async fn cleanup_popup_witness(
    cdp: &CdpClient,
    session_id: &str,
    state_object_id: &str,
    element_object_id: &str,
) -> BrowserResult<()> {
    let cleanup = tokio::time::timeout(
        POPUP_VERIFY_CALL_TIMEOUT,
        cdp.send_to_session(
            session_id,
            "Runtime.callFunctionOn",
            Some(serde_json::json!({
                "objectId": state_object_id,
                "functionDeclaration": "function(){return this.cleanup();}",
                "returnByValue": true,
                "awaitPromise": false
            })),
        ),
    )
    .await;
    let release_state = tokio::time::timeout(
        POPUP_VERIFY_CALL_TIMEOUT,
        cdp.release_object_for_session(session_id, state_object_id),
    )
    .await;
    let release_element = tokio::time::timeout(
        POPUP_VERIFY_CALL_TIMEOUT,
        cdp.release_object_for_session(session_id, element_object_id),
    )
    .await;
    if !matches!(cleanup, Ok(Ok(_)))
        || !matches!(release_state, Ok(Ok(_)))
        || !matches!(release_element, Ok(Ok(_)))
    {
        return Err(popup_error(
            PopupClickErrorKind::PopupUnreadable,
            "popup witness remote-object cleanup failed",
        ));
    }
    Ok(())
}

pub(crate) fn popup_error(kind: PopupClickErrorKind, message: impl Into<String>) -> Box<dyn Error> {
    Box::new(popup_typed_error(kind, message))
}

pub(crate) fn download_error(kind: DownloadErrorKind, message: impl Into<String>) -> DownloadError {
    let message = message.into();
    DownloadError {
        kind,
        message: truncate_utf8_bytes(&message, POPUP_ERROR_MESSAGE_MAX_BYTES),
    }
}

pub(crate) async fn target_browser_context_id(
    cdp: &CdpClient,
    target_id: &str,
    required: bool,
) -> Result<Option<String>, DownloadError> {
    let response = cdp
        .send_browser(
            "Target.getTargetInfo",
            Some(serde_json::json!({"targetId": target_id})),
        )
        .await
        .map_err(|error| {
            download_error(
                DownloadErrorKind::AuthorizationFailed,
                format!("download target context lookup failed: {error}"),
            )
        })?;
    let target_info = response["targetInfo"].as_object().ok_or_else(|| {
        download_error(
            DownloadErrorKind::AuthorizationFailed,
            "download target context lookup returned no targetInfo",
        )
    })?;
    if target_info.get("targetId").and_then(Value::as_str) != Some(target_id) {
        return Err(download_error(
            DownloadErrorKind::AuthorizationFailed,
            "download target context lookup returned a mismatched target ID",
        ));
    }
    let context_id = target_info
        .get("browserContextId")
        .and_then(Value::as_str)
        .map(str::to_string);
    if required && context_id.is_none() {
        return Err(download_error(
            DownloadErrorKind::AuthorizationFailed,
            "download target context lookup returned no browser context ID",
        ));
    }
    if let Some(context_id) = context_id.as_deref() {
        validate_topology_id(context_id).map_err(|error| {
            download_error(
                DownloadErrorKind::AuthorizationFailed,
                format!("download target returned an invalid browser context ID: {error}"),
            )
        })?;
    }
    Ok(context_id)
}

pub(crate) fn use_page_download_compatibility(owned: bool, command_line_incognito: bool) -> bool {
    owned && command_line_incognito
}

pub(crate) fn protocol_result_summary(result: Result<(), crate::browser::cdp::CdpError>) -> String {
    result
        .map(|_| "completed".to_string())
        .unwrap_or_else(|error| error.to_string())
}

pub(crate) fn assess_popup_topology(
    snapshot: &PopupTopologySnapshot,
    topology: &TopologyRegistry,
    witnessed: bool,
) -> Result<PopupCandidate, PopupClickError> {
    if !witnessed {
        return Err(popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            "trusted-click witness was not observed",
        ));
    }
    if topology.event_loss_count != snapshot.event_loss_count {
        return Err(popup_typed_error(
            PopupClickErrorKind::TopologyLagged,
            "topology event stream lagged after mouseReleased",
        ));
    }

    let later_targets = topology.targets.iter().filter_map(|target| {
        let sequence = *topology.target_sequences.get(&target.id)?;
        (!snapshot.preexisting_target_ids.contains(&target.id) && sequence > snapshot.sequence)
            .then_some((target, sequence))
    });
    let mut matching = Vec::new();
    let mut nonmatching_count = 0usize;
    for (target, sequence) in later_targets {
        if target.opener_id.as_deref() == Some(snapshot.original_target_id.as_str()) {
            matching.push(PopupCandidate {
                target: target.clone(),
                observed_sequence: sequence,
            });
        } else {
            nonmatching_count += 1;
        }
    }
    if matching.len() > 1 {
        return Err(popup_typed_error(
            PopupClickErrorKind::PopupAmbiguous,
            format!(
                "trusted click produced {} opener-matching page targets",
                matching.len()
            ),
        ));
    }
    if let Some(candidate) = matching.pop() {
        return Ok(candidate);
    }
    if topology.destroyed_targets.iter().any(|destroyed| {
        destroyed.observed_sequence > snapshot.sequence
            && !snapshot
                .preexisting_target_ids
                .contains(&destroyed.target.id)
            && destroyed.target.opener_id.as_deref() == Some(snapshot.original_target_id.as_str())
    }) {
        return Err(popup_typed_error(
            PopupClickErrorKind::PopupDestroyed,
            "the opener-matching popup was destroyed before verification",
        ));
    }
    if nonmatching_count > 0 {
        return Err(popup_typed_error(
            PopupClickErrorKind::PopupOpenerMismatch,
            "later page targets did not name the original target as opener",
        ));
    }
    Err(popup_typed_error(
        PopupClickErrorKind::PopupMissing,
        "no later opener-matching popup was observed",
    ))
}

pub(crate) async fn popup_verification_call<F>(future: F, step: &str) -> BrowserResult<Value>
where
    F: std::future::Future<Output = Result<Value, crate::browser::cdp::CdpError>>,
{
    match tokio::time::timeout(POPUP_VERIFY_CALL_TIMEOUT, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(popup_error(
            PopupClickErrorKind::PopupUnreadable,
            format!("{step} failed: {error}"),
        )),
        Err(_) => Err(popup_error(
            PopupClickErrorKind::PopupUnreadable,
            format!("{step} exceeded its bounded deadline"),
        )),
    }
}

/// Apply one bounded lifecycle notification. Returns true when the selected
/// target was lost and CDP command routing must be cleared.
pub(crate) fn apply_topology_event(
    topology: &mut TopologyRegistry,
    event: &CdpEventWithParams,
) -> bool {
    match event.method.as_str() {
        "Target.targetCreated" | "Target.targetInfoChanged" => {
            let info = &event.params["targetInfo"];
            if info["type"].as_str() != Some("page") {
                return false;
            }
            let Some(id) = info["targetId"].as_str() else {
                return false;
            };
            if validate_topology_id(id).is_err() {
                push_topology_event(topology, "rejected-target", id);
                return false;
            }
            let Ok(opener_id) = retained_optional_topology_id(info["openerId"].as_str()) else {
                push_topology_event(topology, "rejected-target-opener", id);
                return false;
            };
            let target = PageTargetInfo {
                id: id.to_string(),
                url: bounded_topology_text(info["url"].as_str().unwrap_or_default()),
                title: bounded_topology_text(info["title"].as_str().unwrap_or_default()),
                opener_id,
                active: topology.active_target_id.as_deref() == Some(id),
            };
            if let Some(existing) = topology.targets.iter_mut().find(|target| target.id == id) {
                *existing = target;
            } else if topology.targets.len() < TOPOLOGY_MAX_TARGETS {
                topology.targets.push(target);
            } else {
                push_topology_event(topology, "rejected-target-budget", id);
                return false;
            }
            let sequence = push_topology_event(topology, "target-updated", id);
            topology.target_sequences.insert(id.to_string(), sequence);
        }
        "Target.targetDestroyed" | "Target.targetCrashed" => {
            let Some(id) = event.params["targetId"].as_str() else {
                return false;
            };
            let removed = topology
                .targets
                .iter()
                .find(|target| target.id == id)
                .cloned();
            topology.targets.retain(|target| target.id != id);
            topology.target_sequences.remove(id);
            let sequence = push_topology_event(topology, event.method.as_str(), id);
            if let Some(target) = removed {
                topology.destroyed_targets.push_back(DestroyedPageTarget {
                    target,
                    observed_sequence: sequence,
                });
                while topology.destroyed_targets.len() > TOPOLOGY_MAX_EVENTS {
                    topology.destroyed_targets.pop_front();
                }
            }
            if topology.active_target_id.as_deref() == Some(id) {
                topology.active_target_id = None;
                topology.active_target_session_id = None;
                topology.active_session_id = None;
                topology.active_frame_id = None;
                topology.frames.clear();
                topology.frame_sessions.clear();
                topology.frame_parents.clear();
                return true;
            }
        }
        "Target.detachedFromTarget" => {
            let Some(session_id) = event.params["sessionId"].as_str() else {
                return false;
            };
            push_topology_event(topology, "Target.detachedFromTarget", session_id);
            if topology.active_target_session_id.as_deref() == Some(session_id) {
                topology.active_target_id = None;
                topology.active_target_session_id = None;
                topology.active_session_id = None;
                topology.active_frame_id = None;
                topology.frames.clear();
                return true;
            }
            let active_session_detached = topology.active_session_id.as_deref() == Some(session_id);
            topology
                .frame_sessions
                .retain(|_, attached_session| attached_session != session_id);
            if active_session_detached {
                topology.active_frame_id = None;
                topology.active_session_id = topology.active_target_session_id.clone();
            }
        }
        "Target.attachedToTarget" => {
            let info = &event.params["targetInfo"];
            if info["type"].as_str() != Some("iframe") {
                return false;
            }
            let (Some(frame_id), Some(session_id)) = (
                info["targetId"].as_str(),
                event.params["sessionId"].as_str(),
            ) else {
                return false;
            };
            if validate_topology_id(frame_id).is_err() || validate_topology_id(session_id).is_err()
            {
                return false;
            }
            if topology.frame_sessions.len() < TOPOLOGY_MAX_FRAMES {
                topology
                    .frame_sessions
                    .insert(frame_id.to_string(), session_id.to_string());
                push_topology_event(topology, "Target.attachedToTarget", frame_id);
            }
        }
        "Page.frameAttached" | "Page.frameNavigated" | "Page.frameDetached" => {
            let event_session = event.session_id.as_deref();
            let belongs_to_topology = event_session == topology.active_target_session_id.as_deref()
                || event_session.is_some_and(|session_id| {
                    topology
                        .frame_sessions
                        .values()
                        .any(|attached| attached == session_id)
                });
            if !belongs_to_topology {
                return false;
            }
            let id = event.params["frameId"]
                .as_str()
                .or_else(|| event.params["frame"]["id"].as_str());
            if let Some(id) = id
                && validate_topology_id(id).is_ok()
            {
                push_topology_event(topology, event.method.as_str(), id);
                if event.method == "Page.frameAttached"
                    && let Some(parent_id) = event.params["parentFrameId"].as_str()
                    && validate_topology_id(parent_id).is_ok()
                {
                    topology
                        .frame_parents
                        .insert(id.to_string(), parent_id.to_string());
                }
                if event.method == "Page.frameDetached" {
                    topology.frame_parents.remove(id);
                }
            }
            let selected_was_affected = id.is_some_and(|changed_id| {
                topology.active_frame_id.as_deref() == Some(changed_id)
                    || frame_is_descendant_of(
                        &topology.frames,
                        topology.active_frame_id.as_deref(),
                        changed_id,
                    )
            });
            let selected_is_main = topology
                .active_frame_id
                .as_deref()
                .and_then(|selected| topology.frames.iter().find(|frame| frame.id == selected))
                .is_some_and(|frame| frame.parent_id.is_none());
            topology.frames.clear();
            if selected_was_affected
                && matches!(
                    event.method.as_str(),
                    "Page.frameNavigated" | "Page.frameDetached"
                )
                && !(event.method == "Page.frameNavigated" && selected_is_main)
            {
                topology.active_frame_id = None;
            }
        }
        "Page.javascriptDialogOpening" => {
            let dialog_type = event.params["type"].as_str().unwrap_or("alert").to_string();
            let message =
                bounded_topology_text(event.params["message"].as_str().unwrap_or_default());
            let default_value = event.params["defaultPrompt"].as_str().map(String::from);
            let url = bounded_topology_text(event.params["url"].as_str().unwrap_or_default());
            topology.pending_dialog = Some(PendingDialog {
                dialog_type,
                message,
                default_value,
                url,
            });
            push_topology_event(topology, "Page.javascriptDialogOpening", "dialog");
        }
        "Page.javascriptDialogClosed" => {
            topology.pending_dialog = None;
            push_topology_event(topology, "Page.javascriptDialogClosed", "dialog");
        }
        _ => {}
    }
    false
}

pub(crate) fn frame_is_descendant_of(
    frames: &[FrameInfo],
    selected: Option<&str>,
    ancestor: &str,
) -> bool {
    let Some(mut current) = selected else {
        return false;
    };
    while let Some(frame) = frames.iter().find(|frame| frame.id == current) {
        let Some(parent) = frame.parent_id.as_deref() else {
            return false;
        };
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

pub(crate) async fn resync_topology(
    cdp: &CdpClient,
    registry: &Arc<Mutex<TopologyRegistry>>,
) -> BrowserResult<()> {
    let raw = cdp.send_browser("Target.getTargets", None).await?;
    let (active_target, active_frame, target_session) = {
        let topology = registry.lock().await;
        (
            topology.active_target_id.clone(),
            topology.active_frame_id.clone(),
            topology.active_target_session_id.clone(),
        )
    };
    let mut targets = Vec::new();
    for info in raw["targetInfos"].as_array().into_iter().flatten() {
        if info["type"].as_str() != Some("page") {
            continue;
        }
        let id = info["targetId"]
            .as_str()
            .ok_or("Target.getTargets returned a page without an ID")?;
        validate_topology_id(id)?;
        if targets.len() == TOPOLOGY_MAX_TARGETS {
            return Err(TopologyError::new(
                TopologyErrorKind::BudgetExceeded,
                "page target limit exceeded during topology resync; consider closing unused targets",
            )
            .into());
        }
        targets.push(PageTargetInfo {
            id: id.to_string(),
            url: bounded_topology_text(info["url"].as_str().unwrap_or_default()),
            title: bounded_topology_text(info["title"].as_str().unwrap_or_default()),
            opener_id: retained_optional_topology_id(info["openerId"].as_str())?,
            active: active_target.as_deref() == Some(id),
        });
    }
    let mut frames = Vec::new();
    if let Some(target_session) = target_session {
        let raw = cdp
            .send_to_session(&target_session, "Page.getFrameTree", None)
            .await?;
        collect_frames(
            &raw["frameTree"],
            None,
            active_frame.as_deref(),
            &mut frames,
        )?;
    }
    let mut topology = registry.lock().await;
    topology.targets = targets;
    topology.frames = frames;
    let sequence = push_topology_event(&mut topology, "resynchronized", "topology");
    topology.target_sequences = topology
        .targets
        .iter()
        .map(|target| (target.id.clone(), sequence))
        .collect();
    Ok(())
}

pub(crate) fn actionability_reason(reason: &str) -> TargetActionabilityReason {
    match reason {
        "detached" => TargetActionabilityReason::Detached,
        "not_visible" => TargetActionabilityReason::NotVisible,
        "disabled" => TargetActionabilityReason::Disabled,
        "unstable_geometry" => TargetActionabilityReason::UnstableGeometry,
        "outside_viewport" => TargetActionabilityReason::OutsideViewport,
        "hit_test_blocked" => TargetActionabilityReason::HitTestBlocked,
        _ => TargetActionabilityReason::VerificationFailed,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RevisionedElementReference {
    pub(crate) revision: u64,
    pub(crate) backend_dom_node_id: i64,
}

/// Parse the public `r<revision>:b<backend-node-id>` reference shape.
///
/// Values that do not resemble this exact shape remain normal accessible-name
/// or CSS-selector targets. A malformed value with the marker is an explicit
/// error instead of a silent fallback.
pub(crate) fn parse_revisioned_reference(
    value: &str,
) -> BrowserResult<Option<RevisionedElementReference>> {
    let Some(rest) = value.strip_prefix('r') else {
        return Ok(None);
    };
    let Some((revision, backend_dom_node_id)) = rest.split_once(":b") else {
        return Ok(None);
    };
    if revision.is_empty() || backend_dom_node_id.is_empty() {
        return Err(format!("invalid revisioned element reference: {value}").into());
    }
    let revision = revision
        .parse::<u64>()
        .map_err(|_| format!("invalid element reference revision: {value}"))?;
    let backend_dom_node_id = backend_dom_node_id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| format!("invalid backend node ID in element reference: {value}"))?;
    Ok(Some(RevisionedElementReference {
        revision,
        backend_dom_node_id,
    }))
}

pub(crate) fn runtime_value(raw: &Value) -> BrowserResult<Value> {
    if let Some(exception) = raw.get("exceptionDetails") {
        return Err(format!("JavaScript evaluation failed: {exception}").into());
    }
    Ok(raw["result"]["value"].clone())
}

pub(crate) async fn wait_for_ws_url(port: u16, target_id: Option<&str>) -> BrowserResult<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match get_ws_url(port, target_id).await {
            Ok(url) => return Ok(url),
            Err(error)
                if error.to_string().starts_with("No page target")
                    && tokio::time::Instant::now() < deadline =>
            {
                tracing::debug!(%error, "waiting for Chrome page target");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("about:")
        || url.starts_with("file:")
        || url.starts_with("data:")
    {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

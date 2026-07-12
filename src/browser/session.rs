use base64::{Engine, engine::general_purpose::STANDARD};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::Mutex;

use super::cdp::{CdpClient, CdpEventWithParams};
use super::chrome::{
    ChromeProcess, PortLaunchLock, check_chrome_health, get_browser_ws_url, get_ws_url,
    is_port_occupied, launch_chrome_with_options, resolve_chrome_path,
};
use super::dom::{
    AxNode, CompactAxNode, CompactInteractiveElement, DomNode, backend_node_reference,
    find_interactive_elements, format_tree, parse_accessibility_tree, parse_dom_tree,
    project_compact_accessibility,
};
use super::mouse::{MouseEngine, Point};
use super::profile::ProfileManager;

pub type BrowserResult<T> = Result<T, Box<dyn Error>>;

/// Maximum UTF-8 byte length of visible text returned by a compact observation.
pub const COMPACT_TEXT_MAX_BYTES: usize = 16 * 1024;
const TEXT_TRUNCATION_MARKER: &str = "\n[truncated]";
const COMPACT_OBSERVATION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);
const COMPACT_OBSERVATION_MAX_ATTEMPTS: u8 = 2;
const COMPACT_PAGE_STATE_EXPRESSION: &str = r#"(() => {
    const key = '__glassObservationRevision';
    let state = globalThis[key];
    if (!state) {
        state = {revision: 0};
        const observer = new MutationObserver(() => { state.revision += 1; });
        observer.observe(document, {subtree:true, childList:true, attributes:true, characterData:true});
        globalThis[key] = state;
    }
    const summary = {scanned_elements:0, scan_limit:512, shadow_roots:0, child_frames:0, canvases:0, truncated:false};
    const walker = document.createTreeWalker(document, NodeFilter.SHOW_ELEMENT);
    while (walker.nextNode()) {
        if (summary.scanned_elements >= summary.scan_limit) { summary.truncated = true; break; }
        const element = walker.currentNode;
        summary.scanned_elements += 1;
        if (element.shadowRoot) summary.shadow_roots += 1;
        if (element.localName === 'iframe' || element.localName === 'frame') summary.child_frames += 1;
        if (element.localName === 'canvas') summary.canvases += 1;
    }
    return JSON.stringify({url:location.href, title:document.title, ready_state:document.readyState,
        text:document.body ? document.body.innerText : '', mutation_revision:state.revision, boundaries:summary});
})()"#;
const OWNED_BROWSER_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const AMBIGUOUS_CANDIDATE_LIMIT: usize = 8;
const CANDIDATE_LABEL_MAX_BYTES: usize = 160;
const TOPOLOGY_MAX_TARGETS: usize = 32;
const TOPOLOGY_MAX_FRAMES: usize = 128;
const TOPOLOGY_ID_MAX_BYTES: usize = 256;
const TOPOLOGY_TEXT_MAX_BYTES: usize = 1024;
const TOPOLOGY_MAX_EVENTS: usize = 64;
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const WAIT_LAST_STATE_MAX_BYTES: usize = 512;
const NETWORK_IN_FLIGHT_LIMIT: usize = 1024;
const MAX_WAIT_DEADLINE: Duration = Duration::from_secs(300);
const MAX_WAIT_CONDITION_BYTES: usize = 4 * 1024;
const HIT_TEST_FUNCTION: &str = r#"async function() {
    let element = this && this.nodeType === Node.ELEMENT_NODE ? this : this && this.parentElement;
    if (element) element = element.closest('button,a,input,select,textarea,[role],[tabindex]') || element;
    if (!element || !element.isConnected) return {ok:false, reason:'detached'};
    const sample = () => {
        const rect = element.getBoundingClientRect();
        return {left:rect.left, top:rect.top, width:rect.width, height:rect.height};
    };
    element.scrollIntoView({block:'nearest', inline:'nearest'});
    const first = sample();
    if (!element.isConnected) return {ok:false, reason:'detached'};
    if (element.getAnimations({subtree:true}).some(animation => animation.playState === 'running'))
        return {ok:false, reason:'unstable_geometry'};
    const second = sample();
    const style = getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || second.width <= 0 || second.height <= 0)
        return {ok:false, reason:'not_visible'};
    if (element.matches(':disabled') || element.getAttribute('aria-disabled') === 'true')
        return {ok:false, reason:'disabled'};
    if ([first.left, first.top, first.width, first.height].some((value, index) => Math.abs(value - [second.left, second.top, second.width, second.height][index]) > 1))
        return {ok:false, reason:'unstable_geometry'};
    const x = second.left + second.width / 2;
    const y = second.top + second.height / 2;
    if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight)
        return {ok:false, reason:'outside_viewport'};
    const hit = document.elementFromPoint(x, y);
    if (!hit || (hit !== element && !element.contains(hit)))
        return {ok:false, reason:'hit_test_blocked'};
    return {ok:true, x, y};
}"#;
const WAIT_TARGET_STATE_FUNCTION: &str = r#"function() {
    let element = this && this.nodeType === Node.ELEMENT_NODE ? this : this && this.parentElement;
    if (!element || !element.isConnected) return {attached:false, visible:false, enabled:false};
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    const visible = style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity) !== 0 && rect.width > 0 && rect.height > 0;
    const enabled = !element.matches(':disabled') && element.getAttribute('aria-disabled') !== 'true';
    return {attached:true, visible, enabled, geometry:[rect.left, rect.top, rect.width, rect.height].map(value => Math.round(value * 10) / 10).join(',')};
}"#;

fn is_false(value: &bool) -> bool {
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
    pub kind: String,
    pub id: String,
}

#[derive(Default)]
struct TopologyRegistry {
    targets: Vec<PageTargetInfo>,
    frames: Vec<FrameInfo>,
    active_target_id: Option<String>,
    active_frame_id: Option<String>,
    active_target_session_id: Option<String>,
    active_session_id: Option<String>,
    frame_sessions: HashMap<String, String>,
    frame_parents: HashMap<String, String>,
    events: VecDeque<TopologyEventSummary>,
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

#[derive(Debug)]
enum TargetResolution {
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ObservationBoundarySummary {
    pub scanned_elements: usize,
    pub scan_limit: usize,
    pub shadow_roots: usize,
    pub child_frames: usize,
    pub canvases: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationIncompleteReason {
    VisibleText,
    Accessibility,
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

#[derive(Debug, Clone)]
struct CompactPageContext {
    page: PageInfo,
    text: String,
    accessibility: CompactAccessibilitySnapshot,
    consistency: ObservationConsistency,
    boundaries: ObservationBoundarySummary,
    incomplete: Vec<ObservationIncompleteReason>,
}

impl CompactPageContext {
    fn into_page_context(self) -> PageContext {
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
struct EvaluatedPageState {
    url: String,
    title: String,
    #[serde(default)]
    ready_state: String,
    #[serde(default)]
    mutation_revision: u64,
    #[serde(default)]
    boundaries: ObservationBoundarySummary,
    text: String,
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

/// A browser process, one CDP page connection, and its profile state.
pub struct BrowserSession {
    cdp: CdpClient,
    chrome: Option<ChromeProcess>,
    disposable_profile: Option<DisposableProfileDir>,
    profile_manager: ProfileManager,
    profile: String,
    interaction_mode: InteractionMode,
    mouse: MouseEngine,
    pointer: Mutex<Option<Point>>,
    page_revision: Arc<AtomicU64>,
    observation_cache: Mutex<Option<CachedObservation>>,
    network_wait_leases: Arc<Mutex<NetworkLeaseState>>,
    topology: Arc<Mutex<TopologyRegistry>>,
    upload_root: PathBuf,
}

struct CachedObservation {
    revision: u64,
    context: CompactPageContext,
}

/// A unique user-data directory owned by an incognito Glass session.
///
/// Chrome still receives `--incognito`; the fresh directory also prevents it
/// from inheriting a user's default browser profile or leaving state behind
/// after a normal Glass shutdown.
#[derive(Debug)]
struct DisposableProfileDir {
    path: PathBuf,
}

impl DisposableProfileDir {
    fn create() -> BrowserResult<Self> {
        static NEXT_DISPOSABLE_PROFILE: AtomicU64 = AtomicU64::new(0);

        let root = std::env::temp_dir().join("glass");
        std::fs::create_dir_all(&root)?;
        for _ in 0..32 {
            let sequence = NEXT_DISPOSABLE_PROFILE.fetch_add(1, Ordering::Relaxed);
            let nonce = format!(
                "{}-{}-{sequence}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            );
            let path = root.join(format!("incognito-{nonce}"));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("could not allocate a unique incognito user-data directory".into())
    }

    fn path(&self) -> &Path {
        &self.path
    }
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

impl BrowserSession {
    /// PID of Chrome launched by this session, absent for attached sessions.
    pub fn owned_chrome_pid(&self) -> Option<u32> {
        self.chrome.as_ref().map(|chrome| chrome.pid)
    }

    /// Number of CDP commands issued by this session's page connection.
    pub fn cdp_request_count(&self) -> u64 {
        self.cdp.request_count()
    }

    pub async fn start(options: &SessionOptions) -> BrowserResult<Self> {
        options.validate()?;
        let profile_manager = ProfileManager::new();
        let mut disposable_profile = None;
        let mut chrome = None;

        // Hold an OS-backed lock until the launched child has been verified
        // and its CDP connection is established. A second Glass process that
        // starts at the same time will re-check the port after this session
        // owns it instead of accepting our endpoint as its own.
        let _launch_lock = if options.attach {
            None
        } else {
            Some(PortLaunchLock::acquire(options.port).await?)
        };

        if options.attach {
            if !check_chrome_health(options.port).await {
                return Err(format!(
                    "cannot attach: no healthy Chrome CDP endpoint is listening on port {}; start Chrome with remote debugging or choose another --port",
                    options.port
                )
                .into());
            }
        } else {
            if is_port_occupied(options.port).await {
                return Err(format!(
                    "CDP port {} is already occupied; use --attach to connect to that Chrome endpoint or choose another --port",
                    options.port
                )
                .into());
            }

            let chrome_path = resolve_chrome_path(options.chrome_path.clone())
                .ok_or("Chrome/Chromium not found; run install-chromium or pass --chrome-path")?;
            let profile_dir = if options.incognito {
                let directory = DisposableProfileDir::create()?;
                let path = directory.path().to_path_buf();
                disposable_profile = Some(directory);
                path
            } else {
                profile_manager.ensure_profile_dir(&options.profile)?
            };
            chrome = Some(
                launch_chrome_with_options(
                    &chrome_path,
                    options.port,
                    Some(&profile_dir),
                    options.headed,
                    options.incognito,
                )
                .await?,
            );
        }

        let ws_url = match if options.attach {
            get_ws_url(options.port, options.target_id.as_deref()).await
        } else {
            wait_for_ws_url(options.port, options.target_id.as_deref()).await
        } {
            Ok(url) => url,
            Err(error) => {
                if let Some(process) = chrome.as_mut() {
                    let _ = process.shutdown().await;
                }
                return Err(error);
            }
        };
        let target_id = ws_url
            .rsplit('/')
            .next()
            .filter(|id| !id.is_empty())
            .ok_or("page WebSocket URL contained no target ID")?
            .to_string();
        let browser_ws_url = get_browser_ws_url(options.port).await?;
        let cdp = match CdpClient::connect(&browser_ws_url).await {
            Ok(cdp) => cdp,
            Err(error) => {
                if let Some(process) = chrome.as_mut() {
                    let _ = process.shutdown().await;
                }
                return Err(error);
            }
        };

        cdp.send_browser(
            "Target.setDiscoverTargets",
            Some(serde_json::json!({"discover": true})),
        )
        .await?;
        let attached = cdp
            .send_browser(
                "Target.attachToTarget",
                Some(serde_json::json!({"targetId": target_id, "flatten": true})),
            )
            .await?;
        let session_id = attached["sessionId"]
            .as_str()
            .ok_or("Target.attachToTarget returned no sessionId")?
            .to_string();
        cdp.set_active_target_route(
            Some(target_id.clone()),
            Some(session_id.clone()),
            None,
            None,
        );

        let setup = cdp.enable_observation_events().await;
        if let Err(error) = setup {
            cdp.close().await;
            if let Some(process) = chrome.as_mut() {
                let _ = process.shutdown().await;
            }
            return Err(Box::new(error));
        }

        let page_revision = Arc::new(AtomicU64::new(1));
        let mut events = cdp.subscribe_events();
        let revision_for_events = Arc::clone(&page_revision);
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if context_event_invalidates_observation(&event.method) {
                    revision_for_events.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        let topology = Arc::new(Mutex::new(TopologyRegistry {
            active_target_id: Some(target_id.clone()),
            active_target_session_id: Some(session_id.clone()),
            active_session_id: Some(session_id.clone()),
            ..TopologyRegistry::default()
        }));
        let mut topology_events = cdp.subscribe_events_with_params();
        let topology_for_events = Arc::clone(&topology);
        let cdp_for_events = cdp.clone();
        tokio::spawn(async move {
            loop {
                match topology_events.recv().await {
                    Ok(event) => {
                        let mut topology = topology_for_events.lock().await;
                        let selected_frame = topology.active_frame_id.clone();
                        let selected_session = topology.active_session_id.clone();
                        let selected_context_invalidated = event.method == "Page.frameNavigated"
                            && event.params["frame"]["id"].as_str() == selected_frame.as_deref();
                        if apply_topology_event(&mut topology, &event) {
                            cdp_for_events.set_active_target_route(None, None, None, None);
                        } else if selected_frame.is_some() && topology.active_frame_id.is_none() {
                            cdp_for_events.set_active_route(
                                topology.active_session_id.clone(),
                                None,
                                None,
                            );
                        } else if selected_session != topology.active_session_id
                            || selected_context_invalidated
                        {
                            cdp_for_events.set_active_route(
                                topology.active_session_id.clone(),
                                topology.active_frame_id.clone(),
                                None,
                            );
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let _ = resync_topology(&cdp_for_events, &topology_for_events).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        cdp.send(
            "Target.setAutoAttach",
            Some(serde_json::json!({
                "autoAttach": true,
                "waitForDebuggerOnStart": false,
                "flatten": true
            })),
        )
        .await?;

        let session = Self {
            cdp,
            chrome,
            disposable_profile,
            profile_manager,
            profile: options.profile.clone(),
            interaction_mode: options.interaction_mode,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision,
            observation_cache: Mutex::new(None),
            network_wait_leases: Arc::new(Mutex::new(NetworkLeaseState::default())),
            topology,
            upload_root: std::fs::canonicalize(std::env::current_dir()?)?,
        };
        let initialize_frame = async {
            let frame_id = match options.frame_id.as_deref() {
                Some(frame_id) => frame_id.to_string(),
                None => {
                    session
                        .list_frames()
                        .await?
                        .into_iter()
                        .next()
                        .ok_or("active target returned no main frame")?
                        .id
                }
            };
            session.select_frame(&frame_id).await?;
            Ok::<(), Box<dyn Error>>(())
        }
        .await;
        if let Err(error) = initialize_frame {
            let _ = session.close().await;
            return Err(error);
        }
        Ok(session)
    }

    pub fn cdp(&self) -> &CdpClient {
        &self.cdp
    }

    pub fn profile_manager(&self) -> &ProfileManager {
        &self.profile_manager
    }

    pub fn profile_name(&self) -> &str {
        &self.profile
    }

    pub async fn list_targets(&self) -> BrowserResult<Vec<PageTargetInfo>> {
        let raw = self.cdp.send_browser("Target.getTargets", None).await?;
        let active = self.topology.lock().await.active_target_id.clone();
        let mut targets = Vec::new();
        for info in raw["targetInfos"].as_array().into_iter().flatten() {
            if info["type"].as_str() != Some("page") {
                continue;
            }
            let Some(id) = info["targetId"].as_str() else {
                continue;
            };
            validate_topology_id(id)?;
            if targets.len() == TOPOLOGY_MAX_TARGETS {
                return Err("page target limit exceeded".into());
            }
            targets.push(PageTargetInfo {
                id: id.to_string(),
                url: bounded_topology_text(info["url"].as_str().unwrap_or_default()),
                title: bounded_topology_text(info["title"].as_str().unwrap_or_default()),
                opener_id: retained_optional_topology_id(info["openerId"].as_str())?,
                active: active.as_deref() == Some(id),
            });
        }
        self.topology.lock().await.targets = targets.clone();
        Ok(targets)
    }

    pub async fn topology_events(&self) -> Vec<TopologyEventSummary> {
        self.topology.lock().await.events.iter().cloned().collect()
    }

    async fn route_identity(&self) -> BrowserResult<(String, String)> {
        self.cdp
            .operation_identity()
            .ok_or_else(|| "operation has no target/frame identity".into())
    }

    async fn ensured_route_identity(&self) -> BrowserResult<(String, String)> {
        if let Ok(route) = self.route_identity().await {
            return Ok(route);
        }
        let main_frame = self
            .list_frames()
            .await?
            .into_iter()
            .find(|frame| frame.parent_id.is_none())
            .ok_or("active target returned no main frame")?;
        self.select_frame(&main_frame.id).await?;
        self.route_identity().await
    }

    pub async fn create_target(&self, url: &str) -> BrowserResult<PageTargetInfo> {
        let url = normalize_url(url);
        let result = self
            .cdp
            .send_browser("Target.createTarget", Some(serde_json::json!({"url": url})))
            .await?;
        let id = result["targetId"]
            .as_str()
            .ok_or("Target.createTarget returned no targetId")?;
        validate_topology_id(id)?;
        let targets = self.list_targets().await?;
        targets
            .into_iter()
            .find(|target| target.id == id)
            .ok_or_else(|| "created target was not discoverable".into())
    }

    pub async fn select_target(&self, target_id: &str) -> BrowserResult<PageTargetInfo> {
        validate_topology_id(target_id)?;
        let target = self
            .list_targets()
            .await?
            .into_iter()
            .find(|target| target.id == target_id)
            .ok_or("page target was not found")?;
        let attached = self
            .cdp
            .send_browser(
                "Target.attachToTarget",
                Some(serde_json::json!({"targetId": target_id, "flatten": true})),
            )
            .await?;
        let new_session = attached["sessionId"]
            .as_str()
            .ok_or("Target.attachToTarget returned no sessionId")?
            .to_string();
        let old_session = self.topology.lock().await.active_session_id.clone();
        if let Err(error) = self.cdp.enable_observation_events_for(&new_session).await {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": new_session})),
                )
                .await;
            return Err(error.into());
        }
        if let Err(error) = self
            .cdp
            .send_to_session(
                &new_session,
                "Target.setAutoAttach",
                Some(serde_json::json!({
                    "autoAttach": true,
                    "waitForDebuggerOnStart": false,
                    "flatten": true
                })),
            )
            .await
        {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": new_session})),
                )
                .await;
            return Err(error.into());
        }
        let prepared = async {
            let raw_frames = self
                .cdp
                .send_to_session(&new_session, "Page.getFrameTree", None)
                .await?;
            let mut frames = Vec::new();
            collect_frames(&raw_frames["frameTree"], None, None, &mut frames)?;
            let main_frame = frames
                .iter()
                .find(|frame| frame.parent_id.is_none())
                .ok_or("selected target returned no main frame")?
                .id
                .clone();
            Ok::<_, Box<dyn Error>>((frames, main_frame))
        }
        .await;
        let (mut frames, main_frame) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = self
                    .cdp
                    .send_browser(
                        "Target.detachFromTarget",
                        Some(serde_json::json!({"sessionId": new_session})),
                    )
                    .await;
                return Err(error);
            }
        };
        for frame in &mut frames {
            frame.active = frame.id == main_frame;
        }
        {
            let mut topology = self.topology.lock().await;
            topology.active_target_id = Some(target_id.to_string());
            topology.active_session_id = Some(new_session.clone());
            topology.active_target_session_id = Some(new_session.clone());
            topology.active_frame_id = Some(main_frame.clone());
            topology.frames = frames;
        }
        self.cdp.set_active_target_route(
            Some(target_id.to_string()),
            Some(new_session.clone()),
            Some(main_frame),
            None,
        );
        if let Some(old_session) = old_session {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": old_session})),
                )
                .await;
        }
        self.invalidate_observation();
        Ok(PageTargetInfo {
            active: true,
            ..target
        })
    }

    pub async fn close_target(&self, target_id: &str) -> BrowserResult<()> {
        validate_topology_id(target_id)?;
        let result = self
            .cdp
            .send_browser(
                "Target.closeTarget",
                Some(serde_json::json!({"targetId": target_id})),
            )
            .await?;
        if result["success"].as_bool() != Some(true) {
            return Err("Chrome refused to close target".into());
        }
        let mut topology = self.topology.lock().await;
        if topology.active_target_id.as_deref() == Some(target_id) {
            topology.active_target_id = None;
            topology.active_session_id = None;
            topology.active_target_session_id = None;
            topology.active_frame_id = None;
            topology.frames.clear();
            topology.frame_sessions.clear();
            topology.frame_parents.clear();
            self.cdp.set_active_target_route(None, None, None, None);
        }
        topology.targets.retain(|target| target.id != target_id);
        Ok(())
    }

    pub async fn list_frames(&self) -> BrowserResult<Vec<FrameInfo>> {
        let (target_id, target_session, active) = {
            let topology = self.topology.lock().await;
            (
                topology.active_target_id.clone(),
                topology.active_target_session_id.clone(),
                topology.active_frame_id.clone(),
            )
        };
        if target_id.is_none() {
            return Err("no active target is selected".into());
        }
        let target_session = target_session.ok_or("active target has no CDP session")?;
        let raw = self
            .cdp
            .send_to_session(&target_session, "Page.getFrameTree", None)
            .await?;
        let mut frames = Vec::new();
        collect_frames(&raw["frameTree"], None, active.as_deref(), &mut frames)?;
        let (attached_sessions, frame_parents) = {
            let topology = self.topology.lock().await;
            (
                topology.frame_sessions.clone(),
                topology.frame_parents.clone(),
            )
        };
        let mut discovered_frame_sessions = Vec::new();
        let mut stale_frame_sessions = HashSet::new();
        let mut queried_sessions = HashSet::new();
        for (attached_frame_id, session_id) in &attached_sessions {
            if !queried_sessions.insert(session_id.clone()) {
                continue;
            }
            let oopif_tree = match self
                .cdp
                .send_to_session(session_id, "Page.getFrameTree", None)
                .await
            {
                Ok(tree) => tree,
                Err(error) if error.code == -32_001 => {
                    stale_frame_sessions.insert(session_id.clone());
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let start = frames.len();
            collect_frames(
                &oopif_tree["frameTree"],
                frame_parents.get(attached_frame_id).map(String::as_str),
                active.as_deref(),
                &mut frames,
            )?;
            for frame in &frames[start..] {
                discovered_frame_sessions.push((frame.id.clone(), session_id.clone()));
            }
            for frame in &mut frames[start..] {
                frame.out_of_process = true;
            }
        }
        let mut topology = self.topology.lock().await;
        topology
            .frame_sessions
            .retain(|_, session| !stale_frame_sessions.contains(session));
        for (frame_id, session_id) in discovered_frame_sessions {
            topology.frame_sessions.insert(frame_id, session_id);
        }
        topology.frames = frames.clone();
        Ok(frames)
    }

    pub async fn select_frame(&self, frame_id: &str) -> BrowserResult<FrameInfo> {
        validate_topology_id(frame_id)?;
        let frame = self
            .list_frames()
            .await?
            .into_iter()
            .find(|frame| frame.id == frame_id)
            .ok_or("frame was not found")?;
        let session_id = {
            let topology = self.topology.lock().await;
            topology
                .frame_sessions
                .get(frame_id)
                .cloned()
                .or_else(|| topology.active_target_session_id.clone())
                .ok_or("active target has no CDP session")?
        };
        let context_id = if frame.parent_id.is_none() {
            None
        } else {
            let world = self
                .cdp
                .send_to_session(
                    &session_id,
                    "Page.createIsolatedWorld",
                    Some(serde_json::json!({"frameId": frame_id, "worldName":"glass"})),
                )
                .await?;
            Some(
                world["executionContextId"]
                    .as_i64()
                    .ok_or("Page.createIsolatedWorld returned no executionContextId")?,
            )
        };
        self.cdp.set_active_route(
            Some(session_id.clone()),
            Some(frame_id.to_string()),
            context_id,
        );
        {
            let mut topology = self.topology.lock().await;
            topology.active_frame_id = Some(frame_id.to_string());
            topology.active_session_id = Some(session_id);
        }
        self.invalidate_observation();
        Ok(FrameInfo {
            active: true,
            ..frame
        })
    }

    /// Whether the Chrome process was explicitly attached rather than launched
    /// by this session.
    pub fn is_attached(&self) -> bool {
        self.chrome.is_none()
    }

    /// Whether this session owns the Chrome process and will stop it on close.
    pub fn owns_chrome(&self) -> bool {
        self.chrome.is_some()
    }

    pub async fn close(mut self) -> BrowserResult<()> {
        if self.chrome.is_some() {
            // `Browser.close` lets Chrome commit profile-backed storage before
            // the owned child process falls back to termination below. A page
            // target can close its websocket before replying, so this is best
            // effort and intentionally bounded.
            let _ =
                tokio::time::timeout(OWNED_BROWSER_CLOSE_TIMEOUT, self.cdp.close_browser()).await;
        }
        self.cdp.close().await;
        let shutdown_result = if let Some(process) = self.chrome.as_mut() {
            process.shutdown().await
        } else {
            Ok(())
        };
        self.chrome = None;
        // Drop after the owned child has stopped so Chrome no longer holds
        // files in the disposable user-data directory.
        drop(self.disposable_profile.take());
        shutdown_result
    }

    pub async fn page_info(&self) -> BrowserResult<PageInfo> {
        self.cdp.with_current_route(async {
        let raw = self
            .cdp
            .evaluate(
                "JSON.stringify({url: location.href, title: document.title, ready_state: document.readyState})",
            )
            .await?;
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("document state evaluation returned a non-string value")?;
                let mut page: PageInfo = serde_json::from_str(json)?;
                (page.target_id, page.frame_id) = self.route_identity().await?;
                Ok(page)
        }).await
    }

    pub async fn navigate(&self, url: &str) -> BrowserResult<PageInfo> {
        self.navigate_with_deadline(url, Duration::from_secs(20))
            .await
    }

    pub async fn navigate_with_deadline(
        &self,
        url: &str,
        deadline: Duration,
    ) -> BrowserResult<PageInfo> {
        self.cdp
            .with_current_target_route(async {
                validate_wait_deadline(deadline)?;
                let url = normalize_url(url);
                let mut events = self.cdp.subscribe_events();
                let started = tokio::time::Instant::now();
                let navigation = tokio::time::timeout(deadline, self.cdp.navigate(&url))
                    .await
                    .map_err(|_| {
                        wait_timeout("lifecycle", deadline, "navigate_command_pending")
                    })??;
                if let Some(frame_id) = navigation["frameId"].as_str() {
                    validate_topology_id(frame_id)?;
                    self.topology.lock().await.active_frame_id = Some(frame_id.to_string());
                    self.cdp
                        .set_active_frame_context(Some(frame_id.to_string()), None);
                }
                let remaining = deadline.saturating_sub(started.elapsed());
                self.wait_loop(
                    WaitCondition::Lifecycle("complete".to_string()),
                    remaining,
                    deadline,
                    &mut events,
                    true,
                )
                .await?;
                let remaining = deadline.saturating_sub(started.elapsed());
                let main_frame = self
                    .list_frames()
                    .await?
                    .into_iter()
                    .find(|frame| frame.parent_id.is_none())
                    .ok_or("navigated target returned no main frame")?;
                self.select_frame(&main_frame.id).await?;
                let page = tokio::time::timeout(remaining, self.page_info())
                    .await
                    .map_err(|_| wait_timeout("lifecycle", deadline, "page_info_pending"))??;
                self.invalidate_observation();
                Ok(page)
            })
            .await
    }

    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        self.cdp
            .with_current_route(async {
                let result = self.evaluate_value(expression).await;
                // Arbitrary JavaScript may mutate DOM, styles, form state, or history.
                // Invalidate synchronously so the next cached observation cannot race
                // the asynchronous CDP mutation event stream.
                self.invalidate_observation();
                result
            })
            .await
    }

    pub async fn text(&self) -> BrowserResult<String> {
        self.cdp
            .with_current_route(async {
                let value = self
                    .evaluate_value("document.body ? document.body.innerText : ''")
                    .await?;
                Ok(truncate_visible_text(
                    value.as_str().unwrap_or_default(),
                    COMPACT_TEXT_MAX_BYTES,
                ))
            })
            .await
    }

    /// Fetch the full DOM only for an explicit deep-inspection operation.
    pub async fn deep_dom(&self) -> BrowserResult<DomNode> {
        self.cdp
            .with_current_route(async {
                let raw = self.cdp.get_deep_document().await?;
                parse_dom_tree(&raw).ok_or_else(|| {
                    "CDP deep DOM response contained no parseable root node"
                        .to_string()
                        .into()
                })
            })
            .await
    }

    /// Collect compact page context without a deep DOM or screenshot.
    pub async fn observe(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, true).await
    }

    /// Collect compact context and explicitly include the full DOM tree.
    pub async fn observe_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, true).await
    }

    /// Collect structured context and explicitly include a current screenshot.
    pub async fn observe_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, true).await
    }

    /// Collect context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, true).await
    }

    /// Collect fresh compact context, bypassing the compact-context cache.
    pub async fn observe_fresh(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, false).await
    }

    /// Collect fresh context and explicitly include the full DOM tree.
    pub async fn observe_fresh_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, false).await
    }

    /// Collect fresh structured context and explicitly include a screenshot.
    pub async fn observe_fresh_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, false).await
    }

    /// Collect fresh context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_fresh_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, false).await
    }

    async fn observe_internal(
        &self,
        include_dom: bool,
        include_screenshot: bool,
        use_cache: bool,
    ) -> BrowserResult<PageContext> {
        self.cdp
            .with_current_route(async {
                let mut context = self
                    .compact_observation(use_cache)
                    .await?
                    .into_page_context();
                if include_dom {
                    context.dom = Some(self.deep_dom().await?);
                }
                if include_screenshot {
                    context.screenshot = Some(self.screenshot_base64().await?);
                }
                Ok(context)
            })
            .await
    }

    async fn compact_observation(&self, use_cache: bool) -> BrowserResult<CompactPageContext> {
        let revision = self.page_revision.load(Ordering::Relaxed);
        if use_cache {
            let cached_context = {
                let cache = self.observation_cache.lock().await;
                cache
                    .as_ref()
                    .filter(|cached| cached.revision == revision)
                    .map(|cached| cached.context.clone())
            };
            if let Some(context) = cached_context {
                return Ok(context);
            }
        }

        let mut collected = None;
        for attempt in 1..=COMPACT_OBSERVATION_MAX_ATTEMPTS {
            let start_revision = self.page_revision.load(Ordering::Relaxed);
            let attempt_result = tokio::time::timeout(COMPACT_OBSERVATION_ATTEMPT_TIMEOUT, async {
                let start = self.compact_page_state().await?;
                let accessibility = self.cdp.get_accessibility_tree().await?;
                let end = self.compact_page_state().await?;
                BrowserResult::Ok((start, accessibility, end))
            })
            .await
            .map_err(|_| "compact observation attempt exceeded its one-second deadline")??;
            let end_revision = self.page_revision.load(Ordering::Relaxed);
            let consistent = start_revision == end_revision
                && attempt_result.0.mutation_revision == attempt_result.2.mutation_revision;
            collected = Some((
                attempt,
                consistent,
                start_revision,
                end_revision,
                attempt_result,
            ));
            if consistent {
                break;
            }
        }
        let (
            attempts,
            consistent,
            start_revision,
            end_revision,
            (start_state, accessibility_raw, page_state),
        ) = collected.expect("observation always performs at least one attempt");
        let (target_id, frame_id) = self.route_identity().await?;
        let page = PageInfo {
            url: page_state.url,
            title: page_state.title,
            ready_state: page_state.ready_state,
            target_id,
            frame_id,
        };
        let full_roots = parse_accessibility_tree(&accessibility_raw);
        let compact_accessibility = project_compact_accessibility(&full_roots, end_revision);
        let (text, text_truncated) =
            truncate_visible_text_with_status(&page_state.text, COMPACT_TEXT_MAX_BYTES);
        let mut incomplete = Vec::new();
        if text_truncated {
            incomplete.push(ObservationIncompleteReason::VisibleText);
        }
        if compact_accessibility.truncated {
            incomplete.push(ObservationIncompleteReason::Accessibility);
        }
        if page_state.boundaries.shadow_roots > 0 {
            incomplete.push(ObservationIncompleteReason::ShadowBoundary);
        }
        if page_state.boundaries.child_frames > 0 {
            incomplete.push(ObservationIncompleteReason::FrameBoundary);
        }
        if page_state.boundaries.canvases > 0 {
            incomplete.push(ObservationIncompleteReason::Canvas);
        }
        if page_state.boundaries.truncated {
            incomplete.push(ObservationIncompleteReason::BoundaryScan);
        }
        if !consistent {
            incomplete.push(ObservationIncompleteReason::MutationRace);
        }
        let accessibility = CompactAccessibilitySnapshot {
            page: page.clone(),
            revision: end_revision,
            roots: compact_accessibility.roots,
            interactive: compact_accessibility.interactive,
            truncated: compact_accessibility.truncated,
        };
        let context = CompactPageContext {
            page,
            text,
            accessibility,
            consistency: ObservationConsistency {
                consistent,
                attempts,
                start_revision,
                end_revision,
                start_mutation_revision: start_state.mutation_revision,
                end_mutation_revision: page_state.mutation_revision,
            },
            boundaries: page_state.boundaries,
            incomplete,
        };
        if consistent && self.page_revision.load(Ordering::Relaxed) == end_revision {
            *self.observation_cache.lock().await = Some(CachedObservation {
                revision: end_revision,
                context: context.clone(),
            });
        }
        Ok(context)
    }

    async fn compact_page_state(&self) -> BrowserResult<EvaluatedPageState> {
        let raw = self.cdp.evaluate(COMPACT_PAGE_STATE_EXPRESSION).await?;
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("compact page-state evaluation returned a non-string value")?;
        Ok(serde_json::from_str(json)?)
    }

    pub async fn screenshot_png(&self) -> BrowserResult<Vec<u8>> {
        let data = self.screenshot_base64().await?;
        Ok(STANDARD.decode(data.as_bytes())?)
    }

    /// Capture a PNG while preserving CDP's base64 payload for image APIs.
    pub async fn screenshot_base64(&self) -> BrowserResult<String> {
        self.cdp
            .with_current_route(async { Ok(self.cdp.screenshot("png").await?) })
            .await
    }

    pub async fn scroll(&self, dx: f64, dy: f64) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.cdp.scroll_by(dx, dy).await?;
                let (target_id, frame_id) = self.ensured_route_identity().await?;
                Ok(ActionOutcome {
                    action: ActionKind::Scroll,
                    target: None,
                    revision: self.invalidate_observation(),
                    target_id,
                    frame_id,
                    evidence: None,
                })
            })
            .await
    }

    pub async fn snapshot(&self) -> BrowserResult<AccessibilitySnapshot> {
        self.cdp
            .with_current_route(async {
                let revision = self.page_revision.load(Ordering::Relaxed);
                let raw = self.cdp.get_accessibility_tree().await?;
                let roots = parse_accessibility_tree(&raw);
                let interactive = interactive_elements(&roots, revision);
                Ok(AccessibilitySnapshot {
                    page: self.page_info().await?,
                    roots,
                    interactive,
                })
            })
            .await
    }

    pub async fn wait(
        &self,
        condition: WaitCondition,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        self.cdp
            .with_current_route(async {
                validate_wait_deadline(deadline)?;
                condition.validate()?;
                if let WaitCondition::NetworkQuiet(quiet) = condition {
                    return tokio::time::timeout(
                        deadline,
                        self.wait_for_network_quiet(quiet, deadline),
                    )
                    .await
                    .map_err(|_| {
                        wait_timeout("network_quiet", deadline, "network_check_pending")
                    })?;
                }
                let mut events = self.cdp.subscribe_events();
                self.wait_loop(condition, deadline, deadline, &mut events, false)
                    .await
            })
            .await
    }

    async fn wait_loop(
        &self,
        condition: WaitCondition,
        deadline: Duration,
        reported_deadline: Duration,
        events: &mut tokio::sync::broadcast::Receiver<super::cdp::CdpEvent>,
        require_load_event: bool,
    ) -> BrowserResult<WaitOutcome> {
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut previous_geometry = None;
        let description = condition.description();
        let mut load_event_seen = !require_load_event;
        let mut last_state = "not_checked".to_string();
        loop {
            let now = tokio::time::Instant::now();
            if now >= expires {
                return Err(wait_timeout(&description, reported_deadline, &last_state).into());
            }
            let remaining = expires - now;
            let (matched, state, geometry) = tokio::time::timeout(
                remaining,
                self.check_wait_condition(&condition, previous_geometry.as_deref()),
            )
            .await
            .map_err(|_| wait_timeout(&description, reported_deadline, &last_state))??;
            last_state = bounded_wait_state(&state);
            previous_geometry = geometry;
            if matched && load_event_seen {
                let (target_id, frame_id) = self.ensured_route_identity().await?;
                return Ok(WaitOutcome {
                    condition: description,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state,
                    target_id,
                    frame_id,
                });
            }
            let now = tokio::time::Instant::now();
            let remaining = expires - now;
            tokio::select! {
                _ = tokio::time::sleep(WAIT_POLL_INTERVAL.min(remaining)) => {}
                event = events.recv() => match event {
                    Ok(event) => { load_event_seen |= event.method == "Page.loadEventFired"; }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return Err("CDP event stream closed during wait".into()),
                }
            }
        }
    }

    async fn check_wait_condition(
        &self,
        condition: &WaitCondition,
        previous_geometry: Option<&str>,
    ) -> BrowserResult<(bool, String, Option<String>)> {
        match condition {
            WaitCondition::Lifecycle(expected) => {
                let page = self.page_info().await?;
                Ok((page.ready_state == *expected, page.ready_state, None))
            }
            WaitCondition::UrlExact(expected) => {
                let page = self.page_info().await?;
                Ok((page.url == *expected, page.url, None))
            }
            WaitCondition::UrlPrefix(prefix) => {
                let page = self.page_info().await?;
                Ok((page.url.starts_with(prefix), page.url, None))
            }
            WaitCondition::Text(expected) => {
                let expression = visible_text_contains_expression(expected)?;
                let value = self.evaluate_value(&expression).await?;
                let matched = value.as_bool().unwrap_or(false);
                Ok((matched, format!("present={matched}"), None))
            }
            WaitCondition::JavaScript(expression) => {
                let value = self.evaluate_value(expression).await?;
                let matched = value
                    .as_bool()
                    .ok_or("wait JavaScript predicate must return a boolean")?;
                Ok((matched, matched.to_string(), None))
            }
            WaitCondition::TargetAttached(target)
            | WaitCondition::TargetVisible(target)
            | WaitCondition::TargetHidden(target)
            | WaitCondition::TargetEnabled(target)
            | WaitCondition::TargetStable(target) => {
                self.check_target_wait(condition, target, previous_geometry)
                    .await
            }
            WaitCondition::NetworkQuiet(_) => unreachable!("handled by wait"),
        }
    }

    async fn check_target_wait(
        &self,
        condition: &WaitCondition,
        target: &str,
        previous_geometry: Option<&str>,
    ) -> BrowserResult<(bool, String, Option<String>)> {
        let element = match self.resolve_element(target).await {
            Ok(element) => element,
            Err(error)
                if error
                    .downcast_ref::<TargetError>()
                    .is_some_and(|error| error.kind == TargetErrorKind::NotFound) =>
            {
                let matched = matches!(condition, WaitCondition::TargetHidden(_));
                return Ok((matched, "detached".to_string(), None));
            }
            Err(error) => return Err(error),
        };
        if matches!(condition, WaitCondition::TargetAttached(_)) {
            return Ok((true, "attached".to_string(), None));
        }
        let object_id = self
            .cdp
            .resolve_node_object(element.node_id, element.backend_dom_node_id)
            .await?;
        let raw = self
            .cdp
            .call_on_object(&object_id, WAIT_TARGET_STATE_FUNCTION)
            .await;
        let _ = self.cdp.release_object(&object_id).await;
        let value = runtime_value(&raw?)?;
        let visible = value["visible"].as_bool().unwrap_or(false);
        let enabled = value["enabled"].as_bool().unwrap_or(false);
        let geometry = value["geometry"].as_str().map(str::to_string);
        let matched = match condition {
            WaitCondition::TargetVisible(_) => visible,
            WaitCondition::TargetHidden(_) => !visible,
            WaitCondition::TargetEnabled(_) => visible && enabled,
            WaitCondition::TargetStable(_) => {
                visible
                    && geometry
                        .as_deref()
                        .is_some_and(|geometry| previous_geometry == Some(geometry))
            }
            _ => unreachable!(),
        };
        Ok((matched, value.to_string(), geometry))
    }

    async fn wait_for_network_quiet(
        &self,
        quiet: Duration,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        if quiet.is_zero() {
            return Err("network quiet duration must be positive".into());
        }
        let mut events = self.cdp.subscribe_events_with_params();
        let mut guard =
            NetworkDomainGuard::acquire(self.cdp.clone(), Arc::clone(&self.network_wait_leases))
                .await?;
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut empty_since = started;
        let mut in_flight = HashSet::new();
        let mut overflowed = false;
        loop {
            let now = tokio::time::Instant::now();
            if in_flight.is_empty() && !overflowed && now.duration_since(empty_since) >= quiet {
                guard.disable().await?;
                let (target_id, frame_id) = self.route_identity().await?;
                return Ok(WaitOutcome {
                    condition: "network_quiet".to_string(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state: "in_flight=0".to_string(),
                    target_id,
                    frame_id,
                });
            }
            if now >= expires {
                return Err(WaitTimeout {
                    condition: "network_quiet".to_string(),
                    deadline_ms: deadline.as_millis() as u64,
                    last_state: if overflowed {
                        "in_flight=overflow".to_string()
                    } else {
                        format!("in_flight={}", in_flight.len())
                    },
                    reason: "deadline_exceeded",
                }
                .into());
            }
            tokio::select! {
                _ = tokio::time::sleep((expires - now).min(WAIT_POLL_INTERVAL)) => {}
                event = events.recv() => match event {
                    Ok(event) => {
                      let request_id = event.params["requestId"].as_str();
                      match event.method.as_str() {
                        "Network.requestWillBeSent" => {
                            if let Some(id) = request_id {
                                if in_flight.len() < NETWORK_IN_FLIGHT_LIMIT {
                                    in_flight.insert(id.to_string());
                                } else {
                                    overflowed = true;
                                }
                            }
                        }
                        "Network.loadingFinished" | "Network.loadingFailed" => {
                            if let Some(id) = request_id { in_flight.remove(id); }
                            if in_flight.is_empty() && !overflowed { empty_since = tokio::time::Instant::now(); }
                        }
                        _ => {}
                      }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => return Err("network wait event stream lagged".into()),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err("network wait event stream closed".into()),
                }
            }
        }
    }

    /// Click an element and return its structured action outcome.
    pub async fn click(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.pointer_click(target, false).await
    }

    /// Double-click an element with the same target, scroll, and pointer
    /// contract as a single click.
    pub async fn double_click(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.pointer_click(target, true).await
    }

    pub async fn hover(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await?;
                let remote = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id,
                };
                let local = self.verified_action_point(&remote.object_id).await?;
                let point = self.target_viewport_point(local).await?;
                self.move_pointer(point).await?;
                self.action_outcome(ActionKind::Hover, Some(element), None)
                    .await
            })
            .await
    }

    pub async fn drag(&self, source: &str, destination: &str) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let source = self.resolve_element(source).await?;
                let source_object = self
                    .cdp
                    .resolve_node_object(source.node_id, source.backend_dom_node_id)
                    .await?;
                let source_guard = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id: source_object,
                };
                let destination = self.resolve_element(destination).await?;
                let destination_object = self
                    .cdp
                    .resolve_node_object(destination.node_id, destination.backend_dom_node_id)
                    .await?;
                let destination_guard = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id: destination_object,
                };
                let source_local = self.verified_action_point(&source_guard.object_id).await?;
                let destination_local = self
                    .verified_action_point(&destination_guard.object_id)
                    .await?;
                let source_point = self.target_viewport_point(source_local).await?;
                let destination_point = self.target_viewport_point(destination_local).await?;
                self.move_pointer(source_point).await?;
                let verified_source = self.verified_action_point(&source_guard.object_id).await?;
                if (verified_source.x - source_local.x).abs() > 1.0
                    || (verified_source.y - source_local.y).abs() > 1.0
                {
                    return Err(TargetError {
                        kind: TargetErrorKind::NotActionable,
                        reason: Some(TargetActionabilityReason::GeometryChanged),
                        candidates: Vec::new(),
                    }
                    .into());
                }
                self.cdp
                    .dispatch_mouse_event(
                        "mousePressed",
                        source_point.x,
                        source_point.y,
                        Some("left"),
                        Some(1),
                    )
                    .await?;
                let mut pressed = PressedButtonGuard {
                    cdp: self.cdp.clone(),
                    point: source_point,
                    click_count: 1,
                    armed: true,
                };
                let drag_path = interaction_path(
                    self.interaction_mode,
                    &self.mouse,
                    source_point,
                    destination_point,
                );
                for window in drag_path.windows(2) {
                    let point = window[1];
                    if self.interaction_mode == InteractionMode::Human {
                        tokio::time::sleep(self.mouse.move_delay(window[0], point)).await;
                    }
                    self.cdp
                        .dispatch_mouse_event("mouseMoved", point.x, point.y, Some("left"), Some(1))
                        .await?;
                }
                let verified_destination = self
                    .verified_action_point(&destination_guard.object_id)
                    .await?;
                if (verified_destination.x - destination_local.x).abs() > 1.0
                    || (verified_destination.y - destination_local.y).abs() > 1.0
                {
                    return Err(TargetError {
                        kind: TargetErrorKind::NotActionable,
                        reason: Some(TargetActionabilityReason::GeometryChanged),
                        candidates: Vec::new(),
                    }
                    .into());
                }
                self.cdp
                    .dispatch_mouse_event(
                        "mouseReleased",
                        destination_point.x,
                        destination_point.y,
                        Some("left"),
                        Some(1),
                    )
                    .await?;
                pressed.armed = false;
                *self.pointer.lock().await = Some(destination_point);
                self.action_outcome(ActionKind::Drag, Some(source), None)
                    .await
            })
            .await
    }

    pub async fn key_down(&self, key: &str) -> BrowserResult<ActionOutcome> {
        self.keyboard_action(ActionKind::KeyDown, key, "rawKeyDown", 0)
            .await
    }

    pub async fn key_up(&self, key: &str) -> BrowserResult<ActionOutcome> {
        self.keyboard_action(ActionKind::KeyUp, key, "keyUp", 0)
            .await
    }

    pub async fn key_press(&self, key: &str) -> BrowserResult<ActionOutcome> {
        validate_key(key)?;
        self.cdp
            .with_current_route(async {
                let code = key_code(key);
                self.cdp
                    .dispatch_key_event_with_modifiers("rawKeyDown", key, &code, "", 0)
                    .await?;
                if key.chars().count() == 1 {
                    self.cdp
                        .dispatch_key_event_with_modifiers("char", key, &code, key, 0)
                        .await?;
                }
                self.cdp
                    .dispatch_key_event_with_modifiers("keyUp", key, &code, "", 0)
                    .await?;
                self.action_outcome(ActionKind::KeyPress, None, None).await
            })
            .await
    }

    pub async fn shortcut(&self, shortcut: &str) -> BrowserResult<ActionOutcome> {
        let (modifiers, key) = parse_shortcut(shortcut)?;
        self.cdp
            .with_current_route(async {
                let code = key_code(&key);
                self.cdp
                    .dispatch_key_event_with_modifiers("rawKeyDown", &key, &code, "", modifiers)
                    .await?;
                self.cdp
                    .dispatch_key_event_with_modifiers("keyUp", &key, &code, "", modifiers)
                    .await?;
                self.action_outcome(ActionKind::Shortcut, None, None).await
            })
            .await
    }

    pub async fn clear(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self.cdp.resolve_node_object(element.node_id, element.backend_dom_node_id).await?;
                let remote = RemoteObjectGuard { cdp: self.cdp.clone(), object_id };
                let editable = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this instanceof HTMLInputElement || this instanceof HTMLTextAreaElement || this.isContentEditable}").await?)?;
                if editable.as_bool() != Some(true) { return Err("clear target is not editable".into()); }
                let clicked = self.click(target).await?;
                self.shortcut(if cfg!(target_os = "macos") {
                    "Meta+A"
                } else {
                    "Control+A"
                })
                .await?;
                self.key_press("Backspace").await?;
                let empty = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this instanceof HTMLInputElement || this instanceof HTMLTextAreaElement ? this.value === '' : this.textContent === ''}").await?)?;
                if empty.as_bool() != Some(true) { return Err("clear target did not become empty".into()); }
                self.action_outcome_from_target(ActionKind::Clear, clicked.target)
                    .await
            })
            .await
    }

    pub async fn check(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.set_checked(target, true).await
    }

    pub async fn uncheck(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.set_checked(target, false).await
    }

    pub async fn select_option(&self, target: &str, value: &str) -> BrowserResult<ActionOutcome> {
        if value.is_empty() || value.len() > 4096 {
            return Err("select value must be 1..=4096 bytes".into());
        }
        let value_json = serde_json::to_string(value)?;
        self.form_object_action(target, ActionKind::Select, &format!(r#"function() {{ if (!(this instanceof HTMLSelectElement)) return {{ok:false,reason:'not_select'}}; const option = Array.from(this.options).find(option => option.value === {value_json}); if (!option) return {{ok:false,reason:'option_not_found'}}; this.value = option.value; this.dispatchEvent(new Event('input',{{bubbles:true}})); this.dispatchEvent(new Event('change',{{bubbles:true}})); return {{ok:this.value === option.value}}; }}"#)).await
    }

    pub async fn upload_files(
        &self,
        target: &str,
        paths: &[PathBuf],
    ) -> BrowserResult<ActionOutcome> {
        self.cdp.with_current_route(async {
            if paths.is_empty() || paths.len() > 16 { return Err("upload requires 1..=16 files".into()); }
            let mut files = Vec::with_capacity(paths.len());
            for path in paths {
                let canonical = std::fs::canonicalize(path)?;
                if !canonical.is_file() { return Err("upload path must be a regular file".into()); }
                if !canonical.starts_with(&self.upload_root) { return Err("upload path is outside the allowed workspace root".into()); }
                files.push(canonical.to_string_lossy().into_owned());
            }
            let element = self.resolve_element(target).await?;
            let object_id = self.cdp.resolve_node_object(element.node_id, element.backend_dom_node_id).await?;
            let remote = RemoteObjectGuard { cdp: self.cdp.clone(), object_id };
            self.verified_action_point(&remote.object_id).await?;
            let input = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return {ok:this instanceof HTMLInputElement && this.type === 'file'}}").await?)?;
            if input["ok"].as_bool() != Some(true) { return Err("upload target is not a file input".into()); }
            if element.node_id.is_none() && element.backend_dom_node_id.is_none() { return Err("file input target has no DOM node ID".into()); }
            self.cdp.set_file_input_files(element.node_id, element.backend_dom_node_id, &files).await?;
            let verified = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this.files.length}").await?)?;
            if verified.as_u64() != Some(files.len() as u64) { return Err("file input did not retain the requested file count".into()); }
            self.action_outcome(ActionKind::Upload, Some(element), Some(serde_json::json!({"file_count": files.len()}))).await
        }).await
    }

    async fn pointer_click(
        &self,
        target: &str,
        double_click: bool,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await
                    .map_err(|error| {
                        tracing::debug!(%error, "target node could not be resolved");
                        TargetError {
                            kind: TargetErrorKind::NotActionable,
                            reason: Some(TargetActionabilityReason::NodeUnavailable),
                            candidates: Vec::new(),
                        }
                    })?;
                let remote = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id,
                };
                let local_point = self.verified_action_point(&remote.object_id).await?;
                let point = self.target_viewport_point(local_point).await?;
                let events = if double_click {
                    self.mouse.generate_double_click_events(point)
                } else {
                    self.mouse.generate_click_events(point)
                };
                self.dispatch_pointer_events(&remote.object_id, local_point, point, events)
                    .await?;
                let (target_id, frame_id) = self.route_identity().await?;
                Ok(ActionOutcome {
                    action: if double_click {
                        ActionKind::DoubleClick
                    } else {
                        ActionKind::Click
                    },
                    target: Some(ActionTarget {
                        label: element.label,
                        reference: element.reference,
                    }),
                    revision: self.invalidate_observation(),
                    target_id,
                    frame_id,
                    evidence: None,
                })
            })
            .await
    }

    async fn dispatch_pointer_events(
        &self,
        object_id: &str,
        local_point: Point,
        point: Point,
        events: Vec<super::mouse::MouseEvent>,
    ) -> BrowserResult<()> {
        let mut pointer = self.pointer.lock().await;
        let start = match (self.interaction_mode, *pointer) {
            (_, Some(point)) => point,
            (InteractionMode::Human, None) => self
                .viewport_center()
                .await
                .unwrap_or(Point { x: 640.0, y: 360.0 }),
            (InteractionMode::Fast, None) => point,
        };
        let path = interaction_path(self.interaction_mode, &self.mouse, start, point);
        if self.interaction_mode == InteractionMode::Human && pointer.is_none() {
            self.cdp
                .dispatch_mouse_event("mouseMoved", start.x, start.y, None, None)
                .await?;
        }
        for window in path.windows(2) {
            let next = window[1];
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], next)).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", next.x, next.y, None, None)
                .await?;
        }
        let press_point = self.verified_action_point(object_id).await?;
        if (press_point.x - local_point.x).abs() > 1.0
            || (press_point.y - local_point.y).abs() > 1.0
        {
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(TargetActionabilityReason::GeometryChanged),
                candidates: Vec::new(),
            }
            .into());
        }
        let mut pressed = None;
        for event in events {
            if event.event_type == "mousePressed" {
                pressed = Some(PressedButtonGuard {
                    cdp: self.cdp.clone(),
                    point,
                    click_count: event.click_count,
                    armed: true,
                });
            }
            self.cdp
                .dispatch_mouse_event(
                    &event.event_type,
                    event.x,
                    event.y,
                    Some(&event.button),
                    Some(event.click_count),
                )
                .await?;
            if event.event_type == "mouseReleased"
                && let Some(mut guard) = pressed.take()
            {
                guard.armed = false;
            }
            if self.interaction_mode == InteractionMode::Human && event.event_type == "mousePressed"
            {
                tokio::time::sleep(self.mouse.click_delay()).await;
            }
        }
        *pointer = Some(point);
        Ok(())
    }

    pub async fn type_text(
        &self,
        text: &str,
        target: Option<&str>,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let target = match target {
                    Some(target) => self.click(target).await?.target,
                    None => None,
                };
                self.cdp.insert_text(text).await?;
                let (target_id, frame_id) = self.route_identity().await?;
                Ok(ActionOutcome {
                    action: ActionKind::Type,
                    target,
                    revision: self.invalidate_observation(),
                    target_id,
                    frame_id,
                    evidence: None,
                })
            })
            .await
    }

    async fn move_pointer(&self, destination: Point) -> BrowserResult<()> {
        let mut pointer = self.pointer.lock().await;
        let start = pointer.unwrap_or(destination);
        for window in
            interaction_path(self.interaction_mode, &self.mouse, start, destination).windows(2)
        {
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], window[1])).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", window[1].x, window[1].y, None, None)
                .await?;
        }
        if start == destination {
            self.cdp
                .dispatch_mouse_event("mouseMoved", destination.x, destination.y, None, None)
                .await?;
        }
        *pointer = Some(destination);
        Ok(())
    }

    async fn keyboard_action(
        &self,
        action: ActionKind,
        key: &str,
        event_type: &str,
        modifiers: i64,
    ) -> BrowserResult<ActionOutcome> {
        validate_key(key)?;
        self.cdp
            .with_current_route(async {
                self.cdp
                    .dispatch_key_event_with_modifiers(
                        event_type,
                        key,
                        &key_code(key),
                        "",
                        modifiers,
                    )
                    .await?;
                self.action_outcome(action, None, None).await
            })
            .await
    }

    async fn set_checked(&self, target: &str, checked: bool) -> BrowserResult<ActionOutcome> {
        let action = if checked {
            ActionKind::Check
        } else {
            ActionKind::Uncheck
        };
        let script = format!(
            r#"function() {{ if (!(this instanceof HTMLInputElement) || !['checkbox','radio'].includes(this.type)) return {{ok:false,reason:'not_checkable'}}; if (this.checked !== {checked}) this.click(); return {{ok:this.checked === {checked}}}; }}"#
        );
        self.form_object_action(target, action, &script).await
    }

    async fn form_object_action(
        &self,
        target: &str,
        action: ActionKind,
        function: &str,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await?;
                let remote = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id,
                };
                self.verified_action_point(&remote.object_id).await?;
                let result = self.cdp.call_on_object(&remote.object_id, function).await?;
                let value = runtime_value(&result)?;
                if value["ok"].as_bool() != Some(true) {
                    return Err(format!(
                        "form action failed: {}",
                        value["reason"].as_str().unwrap_or("verification_failed")
                    )
                    .into());
                }
                self.action_outcome(action, Some(element), None).await
            })
            .await
    }

    async fn action_outcome(
        &self,
        action: ActionKind,
        element: Option<ResolvedElement>,
        evidence: Option<Value>,
    ) -> BrowserResult<ActionOutcome> {
        let target = element.map(|element| ActionTarget {
            label: element.label,
            reference: element.reference,
        });
        let mut outcome = self.action_outcome_from_target(action, target).await?;
        outcome.evidence = evidence;
        Ok(outcome)
    }

    async fn action_outcome_from_target(
        &self,
        action: ActionKind,
        target: Option<ActionTarget>,
    ) -> BrowserResult<ActionOutcome> {
        let (target_id, frame_id) = self.route_identity().await?;
        Ok(ActionOutcome {
            action,
            target,
            revision: self.invalidate_observation(),
            target_id,
            frame_id,
            evidence: None,
        })
    }

    async fn viewport_center(&self) -> BrowserResult<Point> {
        let value = self
            .evaluate_value("[window.innerWidth / 2, window.innerHeight / 2]")
            .await?;
        let coordinates = value
            .as_array()
            .filter(|coordinates| coordinates.len() == 2)
            .ok_or("viewport evaluation returned invalid coordinates")?;
        let x = coordinates[0]
            .as_f64()
            .ok_or("viewport width was not numeric")?;
        let y = coordinates[1]
            .as_f64()
            .ok_or("viewport height was not numeric")?;
        Ok(Point { x, y })
    }

    async fn target_viewport_point(&self, point: Point) -> BrowserResult<Point> {
        let Some(frame_id) = self.cdp.active_frame() else {
            return Ok(point);
        };
        let frames = self.list_frames().await?;
        let Some(frame) = frames.iter().find(|frame| frame.id == frame_id) else {
            return Err("selected frame is no longer attached".into());
        };
        if frame.parent_id.is_none() {
            return Ok(point);
        }
        let (x, y) = self.cdp.frame_viewport_offset(&frame_id).await?;
        Ok(Point {
            x: point.x + x,
            y: point.y + y,
        })
    }

    async fn evaluate_value(&self, expression: &str) -> BrowserResult<Value> {
        let raw = self.cdp.evaluate(expression).await?;
        runtime_value(&raw)
    }

    fn invalidate_observation(&self) -> u64 {
        self.page_revision.fetch_add(1, Ordering::Relaxed) + 1
    }

    async fn resolve_element(&self, target: &str) -> BrowserResult<ResolvedElement> {
        let locator = Locator::parse(target)?;
        match self.resolve_locator(&locator).await? {
            TargetResolution::Unique(element) => Ok(element),
            TargetResolution::Ambiguous(candidates) => Err(TargetError {
                kind: TargetErrorKind::Ambiguous,
                reason: None,
                candidates,
            }
            .into()),
            TargetResolution::NotFound => Err(TargetError {
                kind: TargetErrorKind::NotFound,
                reason: None,
                candidates: Vec::new(),
            }
            .into()),
        }
    }

    async fn resolve_locator(&self, locator: &Locator) -> BrowserResult<TargetResolution> {
        if let Locator::Reference(target) = locator {
            let reference = parse_revisioned_reference(target)?
                .ok_or_else(|| format!("invalid revisioned element reference: {target}"))?;
            let current_revision = self.page_revision.load(Ordering::Relaxed);
            if reference.revision != current_revision {
                return Err(TargetError {
                    kind: TargetErrorKind::StaleReference,
                    reason: None,
                    candidates: Vec::new(),
                }
                .into());
            }
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(reference.backend_dom_node_id),
                label: target.to_string(),
                reference: Some(target.to_string()),
            }));
        }

        let snapshot = self.snapshot().await?;
        let matches: Vec<&InteractiveElement> = match locator {
            Locator::AccessibleName(name) => snapshot
                .interactive
                .iter()
                .filter(|element| element.name.eq_ignore_ascii_case(name))
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::RoleAndName { role, name } => snapshot
                .interactive
                .iter()
                .filter(|element| {
                    element.role.eq_ignore_ascii_case(role)
                        && element.name.eq_ignore_ascii_case(name)
                })
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::Ordinal(index) => snapshot.interactive.get(index - 1).into_iter().collect(),
            Locator::Reference(_) | Locator::Text(_) | Locator::Css(_) => Vec::new(),
        };
        if matches.len() == 1 {
            let element = matches[0];
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(element.backend_dom_node_id),
                label: format!("{} {}", element.role, element.name),
                reference: Some(element.reference.clone()),
            }));
        }
        if matches.len() > 1 {
            return Ok(TargetResolution::Ambiguous(
                matches
                    .into_iter()
                    .take(AMBIGUOUS_CANDIDATE_LIMIT)
                    .map(|element| CandidateSummary {
                        label: bounded_candidate_label(&format!(
                            "{} {}",
                            element.role, element.name
                        )),
                        reference: Some(element.reference.clone()),
                    })
                    .collect(),
            ));
        }

        match locator {
            Locator::Css(selector) => {
                let expression = css_query_expression(selector)?;
                let (count, nodes) = self
                    .cdp
                    .bounded_element_query(&expression, AMBIGUOUS_CANDIDATE_LIMIT)
                    .await?;
                dom_nodes_resolution(count, nodes, format!("css={selector}"), "css match")
            }
            Locator::Text(text) => {
                let expression = text_query_expression(text)?;
                let (count, nodes) = self
                    .cdp
                    .bounded_element_query(&expression, AMBIGUOUS_CANDIDATE_LIMIT)
                    .await?;
                if count > 1 {
                    return Ok(TargetResolution::Ambiguous(
                        (1..=count.min(AMBIGUOUS_CANDIDATE_LIMIT))
                            .map(|index| CandidateSummary {
                                label: format!("text match {index}"),
                                reference: None,
                            })
                            .collect(),
                    ));
                }
                dom_nodes_resolution(count, nodes, format!("text={text}"), "text match")
            }
            Locator::Reference(_)
            | Locator::AccessibleName(_)
            | Locator::RoleAndName { .. }
            | Locator::Ordinal(_) => Ok(TargetResolution::NotFound),
        }
    }

    async fn verified_action_point(&self, object_id: &str) -> BrowserResult<Point> {
        let raw = match self.cdp.call_on_object(object_id, HIT_TEST_FUNCTION).await {
            Ok(raw) => raw,
            Err(error) => {
                tracing::debug!(%error, "target node could not be verified");
                return Err(TargetError {
                    kind: TargetErrorKind::NotActionable,
                    reason: Some(TargetActionabilityReason::NodeUnavailable),
                    candidates: Vec::new(),
                }
                .into());
            }
        };
        let value = runtime_value(&raw)?;
        if value["ok"].as_bool() != Some(true) {
            let reason = value["reason"].as_str().unwrap_or("verification_failed");
            tracing::debug!(reason, "target actionability check failed");
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(actionability_reason(reason)),
                candidates: Vec::new(),
            }
            .into());
        }
        let x = value["x"]
            .as_f64()
            .ok_or("verified target x was not numeric")?;
        let y = value["y"]
            .as_f64()
            .ok_or("verified target y was not numeric")?;
        Ok(Point { x, y })
    }
}

fn interactive_elements(roots: &[AxNode], revision: u64) -> Vec<InteractiveElement> {
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

fn truncate_visible_text(text: &str, max_bytes: usize) -> String {
    truncate_visible_text_with_status(text, max_bytes).0
}

fn truncate_visible_text_with_status(text: &str, max_bytes: usize) -> (String, bool) {
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

fn interaction_path(
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

fn validate_key(key: &str) -> BrowserResult<()> {
    if key.is_empty() || key.len() > 64 || key.chars().any(char::is_control) {
        return Err("key must be 1..=64 printable UTF-8 bytes".into());
    }
    Ok(())
}

fn key_code(key: &str) -> String {
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

fn parse_shortcut(value: &str) -> BrowserResult<(i64, String)> {
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

fn context_event_invalidates_observation(method: &str) -> bool {
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

#[derive(Debug)]
struct ResolvedElement {
    node_id: Option<i64>,
    backend_dom_node_id: Option<i64>,
    label: String,
    reference: Option<String>,
}

struct PressedButtonGuard {
    cdp: CdpClient,
    point: Point,
    click_count: u32,
    armed: bool,
}

struct RemoteObjectGuard {
    cdp: CdpClient,
    object_id: String,
}

struct NetworkDomainGuard {
    cdp: CdpClient,
    leases: Arc<Mutex<NetworkLeaseState>>,
    armed: bool,
}

#[derive(Default)]
struct NetworkLeaseState {
    count: usize,
}

impl NetworkDomainGuard {
    async fn acquire(cdp: CdpClient, leases: Arc<Mutex<NetworkLeaseState>>) -> BrowserResult<Self> {
        let mut state = leases.lock().await;
        state.count += 1;
        let mut guard = Self {
            cdp,
            leases: Arc::clone(&leases),
            armed: true,
        };
        if state.count == 1
            && let Err(error) = guard.cdp.enable_network().await
        {
            state.count = 0;
            guard.armed = false;
            drop(state);
            let _ = guard.cdp.disable_network().await;
            return Err(error.into());
        }
        drop(state);
        Ok(guard)
    }

    async fn disable(&mut self) -> BrowserResult<()> {
        let state = self.leases.lock().await;
        self.armed = false;
        release_network_lease_locked(&self.cdp, state).await
    }
}

impl Drop for NetworkDomainGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let leases = Arc::clone(&self.leases);
        tokio::spawn(async move {
            let _ = release_network_lease(&cdp, &leases).await;
        });
    }
}

async fn release_network_lease(
    cdp: &CdpClient,
    leases: &Mutex<NetworkLeaseState>,
) -> BrowserResult<()> {
    release_network_lease_locked(cdp, leases.lock().await).await
}

async fn release_network_lease_locked(
    cdp: &CdpClient,
    mut state: tokio::sync::MutexGuard<'_, NetworkLeaseState>,
) -> BrowserResult<()> {
    state.count = state.count.saturating_sub(1);
    if state.count == 0 {
        cdp.disable_network().await?;
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

    fn description(&self) -> String {
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

    fn validate(&self) -> BrowserResult<()> {
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

fn bounded_wait_state(value: &str) -> String {
    truncate_visible_text(value, WAIT_LAST_STATE_MAX_BYTES)
}

fn validate_wait_deadline(deadline: Duration) -> BrowserResult<()> {
    if deadline.is_zero() || deadline > MAX_WAIT_DEADLINE {
        return Err("wait deadline must be between 1 ms and 300000 ms".into());
    }
    Ok(())
}

fn wait_timeout(condition: &str, deadline: Duration, last_state: &str) -> WaitTimeout {
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

fn nonempty_locator(value: &str, kind: &str) -> BrowserResult<String> {
    if value.is_empty() {
        return Err(format!("{kind} locator cannot be empty").into());
    }
    Ok(value.to_string())
}

fn dom_nodes_resolution(
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

fn css_query_expression(selector: &str) -> BrowserResult<String> {
    let selector = serde_json::to_string(selector)?;
    Ok(format!(
        "(() => {{ const nodes = document.querySelectorAll({selector}); const out = []; for (let i = 0; i < Math.min(nodes.length, {AMBIGUOUS_CANDIDATE_LIMIT}); i++) out.push(nodes[i]); out.glassCount = nodes.length; return out; }})()"
    ))
}

fn text_query_expression(text: &str) -> BrowserResult<String> {
    let text = serde_json::to_string(text)?;
    Ok(format!(
        r#"(() => {{
            const wanted = ({text}).replace(/\s+/g, ' ').trim();
            const matches = [];
            let count = 0;
            const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_ELEMENT);
            for (let element = walker.currentNode; element; element = walker.nextNode()) {{
                const style = getComputedStyle(element);
                const rect = element.getBoundingClientRect();
                if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || rect.width <= 0 || rect.height <= 0) continue;
                if (element.checkVisibility && !element.checkVisibility({{checkOpacity:true, checkVisibilityCSS:true}})) continue;
                let clipped = false;
                let visibleLeft = rect.left, visibleTop = rect.top, visibleRight = rect.right, visibleBottom = rect.bottom;
                for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) {{
                    const ancestorStyle = getComputedStyle(ancestor);
                    if (ancestorStyle.display === 'none' || ancestorStyle.visibility === 'hidden' || Number(ancestorStyle.opacity) === 0) {{ clipped = true; break; }}
                    if (/(hidden|clip)/.test(ancestorStyle.overflow + ancestorStyle.overflowX + ancestorStyle.overflowY)) {{
                        const bounds = ancestor.getBoundingClientRect();
                        visibleLeft = Math.max(visibleLeft, bounds.left); visibleTop = Math.max(visibleTop, bounds.top);
                        visibleRight = Math.min(visibleRight, bounds.right); visibleBottom = Math.min(visibleBottom, bounds.bottom);
                        if (visibleRight <= visibleLeft || visibleBottom <= visibleTop) {{ clipped = true; break; }}
                    }}
                }}
                if (clipped) continue;
                const actual = (element.innerText || '').replace(/\s+/g, ' ').trim();
                if (actual !== wanted) continue;
                const candidate = element.closest('button,a,input,select,textarea,[role],[tabindex]') || element;
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

fn visible_text_contains_expression(text: &str) -> BrowserResult<String> {
    let text = serde_json::to_string(text)?;
    Ok(format!(
        r#"(() => {{
            const wanted = {text};
            const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
            for (let node = walker.nextNode(); node; node = walker.nextNode()) {{
                if (!(node.nodeValue || '').includes(wanted)) continue;
                const element = node.parentElement;
                if (!element) continue;
                if (element.checkVisibility && !element.checkVisibility({{checkOpacity:true, checkVisibilityCSS:true}})) continue;
                const rect = element.getBoundingClientRect();
                if (rect.width <= 0 || rect.height <= 0) continue;
                let left = rect.left, top = rect.top, right = rect.right, bottom = rect.bottom, hidden = false;
                for (let ancestor = element; ancestor; ancestor = ancestor.parentElement) {{
                    const style = getComputedStyle(ancestor);
                    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) {{ hidden = true; break; }}
                    if (/(hidden|clip)/.test(style.overflow + style.overflowX + style.overflowY)) {{
                        const bounds = ancestor.getBoundingClientRect();
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

fn bounded_candidate_label(value: &str) -> String {
    if value.len() <= CANDIDATE_LABEL_MAX_BYTES {
        return value.to_string();
    }
    let mut end = CANDIDATE_LABEL_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn validate_topology_id(value: &str) -> BrowserResult<()> {
    if value.is_empty() {
        return Err("topology ID cannot be empty".into());
    }
    if value.len() > TOPOLOGY_ID_MAX_BYTES {
        return Err(format!("topology ID exceeds {TOPOLOGY_ID_MAX_BYTES} UTF-8 bytes").into());
    }
    Ok(())
}

fn bounded_topology_text(value: &str) -> String {
    if value.len() <= TOPOLOGY_TEXT_MAX_BYTES {
        return value.to_string();
    }
    let mut end = TOPOLOGY_TEXT_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn collect_frames(
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

fn push_topology_event(topology: &mut TopologyRegistry, kind: &str, id: &str) {
    topology.events.push_back(TopologyEventSummary {
        kind: kind.to_string(),
        id: bounded_topology_id(id),
    });
    while topology.events.len() > TOPOLOGY_MAX_EVENTS {
        topology.events.pop_front();
    }
}

fn bounded_topology_id(value: &str) -> String {
    if value.len() <= TOPOLOGY_ID_MAX_BYTES {
        return value.to_string();
    }
    let mut end = TOPOLOGY_ID_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn retained_optional_topology_id(value: Option<&str>) -> BrowserResult<Option<String>> {
    value
        .map(|id| {
            validate_topology_id(id)?;
            Ok(id.to_string())
        })
        .transpose()
}

/// Apply one bounded lifecycle notification. Returns true when the selected
/// target was lost and CDP command routing must be cleared.
fn apply_topology_event(topology: &mut TopologyRegistry, event: &CdpEventWithParams) -> bool {
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
            push_topology_event(topology, "target-updated", id);
        }
        "Target.targetDestroyed" | "Target.targetCrashed" => {
            let Some(id) = event.params["targetId"].as_str() else {
                return false;
            };
            topology.targets.retain(|target| target.id != id);
            push_topology_event(topology, event.method.as_str(), id);
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
        _ => {}
    }
    false
}

fn frame_is_descendant_of(frames: &[FrameInfo], selected: Option<&str>, ancestor: &str) -> bool {
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

async fn resync_topology(
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
            return Err("page target limit exceeded during topology resync".into());
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
    push_topology_event(&mut topology, "resynchronized", "topology");
    Ok(())
}

fn actionability_reason(reason: &str) -> TargetActionabilityReason {
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
struct RevisionedElementReference {
    revision: u64,
    backend_dom_node_id: i64,
}

/// Parse the public `r<revision>:b<backend-node-id>` reference shape.
///
/// Values that do not resemble this exact shape remain normal accessible-name
/// or CSS-selector targets. A malformed value with the marker is an explicit
/// error instead of a silent fallback.
fn parse_revisioned_reference(value: &str) -> BrowserResult<Option<RevisionedElementReference>> {
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

fn runtime_value(raw: &Value) -> BrowserResult<Value> {
    if let Some(exception) = raw.get("exceptionDetails") {
        return Err(format!("JavaScript evaluation failed: {exception}").into());
    }
    Ok(raw["result"]["value"].clone())
}

async fn wait_for_ws_url(port: u16, target_id: Option<&str>) -> BrowserResult<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    fn test_session(cdp: CdpClient) -> BrowserSession {
        cdp.set_active_target_route(
            Some("test-target".to_string()),
            None,
            Some("test-frame".to_string()),
            None,
        );
        BrowserSession {
            cdp,
            chrome: None,
            disposable_profile: None,
            profile_manager: ProfileManager::new(),
            profile: "test".to_string(),
            interaction_mode: InteractionMode::Fast,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision: Arc::new(AtomicU64::new(1)),
            observation_cache: Mutex::new(None),
            network_wait_leases: Arc::new(Mutex::new(NetworkLeaseState::default())),
            topology: Arc::new(Mutex::new(TopologyRegistry {
                active_target_id: Some("test-target".to_string()),
                active_frame_id: Some("test-frame".to_string()),
                ..TopologyRegistry::default()
            })),
            upload_root: std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
        }
    }

    #[test]
    fn attach_options_reject_launch_only_configuration() {
        let attached = SessionOptions {
            attach: true,
            ..SessionOptions::default()
        };
        assert!(attached.validate().is_ok());

        let mut incognito = attached.clone();
        incognito.incognito = true;
        assert!(
            incognito
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--incognito")
        );

        let mut profile = attached.clone();
        profile.profile = "work".to_string();
        assert!(
            profile
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--profile")
        );

        let mut chrome_path = attached.clone();
        chrome_path.chrome_path = Some(PathBuf::from("/tmp/chrome"));
        assert!(
            chrome_path
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--chrome-path")
        );

        let mut headed = attached;
        headed.headed = true;
        assert!(
            headed
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--headed")
        );
    }

    #[test]
    fn target_id_must_not_be_empty() {
        let options = SessionOptions {
            target_id: Some("   ".to_string()),
            ..SessionOptions::default()
        };

        assert!(
            options
                .validate()
                .unwrap_err()
                .to_string()
                .contains("target ID")
        );
    }

    #[test]
    fn disposable_incognito_directories_are_unique_and_removed() {
        let first = DisposableProfileDir::create().unwrap();
        let first_path = first.path().to_path_buf();
        let second = DisposableProfileDir::create().unwrap();
        let second_path = second.path().to_path_buf();

        assert_ne!(first_path, second_path);
        assert!(first_path.is_dir());
        assert!(second_path.is_dir());

        drop(first);
        drop(second);
        assert!(!first_path.exists());
        assert!(!second_path.exists());
    }

    async fn observation_server(include_dom: bool) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut saw_runtime = false;
            let mut saw_accessibility = false;
            let mut saw_deep_dom = false;

            for _ in 0..if include_dom { 4 } else { 3 } {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Runtime.evaluate") => {
                        saw_runtime = true;
                        let expression = request["params"]["expression"].as_str().unwrap();
                        assert!(expression.contains("document.body.innerText"));
                        assert!(!expression.contains(".slice(0,"));
                        let text = format!(
                            "{}😀{}",
                            "a".repeat(4_095),
                            "b".repeat(COMPACT_TEXT_MAX_BYTES)
                        );
                        let page_state = serde_json::json!({
                            "url": "https://example.test",
                            "title": "Example",
                            "ready_state": "complete",
                            "text": text,
                            "mutation_revision": 0,
                            "boundaries": {
                                "scanned_elements": 12,
                                "scan_limit": 512,
                                "shadow_roots": 1,
                                "child_frames": 1,
                                "canvases": 1,
                                "truncated": false
                            }
                        })
                        .to_string();
                        serde_json::json!({
                            "result": {"value": page_state}
                        })
                    }
                    Some("Accessibility.getFullAXTree") => {
                        saw_accessibility = true;
                        serde_json::json!({"nodes": []})
                    }
                    Some("DOM.getDocument") => {
                        saw_deep_dom = true;
                        assert_eq!(request["params"], serde_json::json!({"depth": -1}));
                        serde_json::json!({
                            "root": {
                                "nodeId": 1,
                                "nodeName": "#document",
                                "nodeValue": "",
                                "children": []
                            }
                        })
                    }
                    method => panic!("unexpected compact-observation command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }

            assert!(saw_runtime);
            assert!(saw_accessibility);
            assert_eq!(saw_deep_dom, include_dom);
        });
        (format!("ws://{address}"), server)
    }

    async fn mutation_race_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut runtime_revision = 0_u64;
            for _ in 0..6 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Runtime.evaluate") => {
                        runtime_revision += 1;
                        serde_json::json!({"result": {"value": serde_json::json!({
                            "url": "https://race.test",
                            "title": "Race",
                            "ready_state": "complete",
                            "text": "changing",
                            "mutation_revision": runtime_revision,
                            "boundaries": {"scanned_elements": 1, "scan_limit": 512,
                                "shadow_roots": 0, "child_frames": 0, "canvases": 0,
                                "truncated": false}
                        }).to_string()}})
                    }
                    Some("Accessibility.getFullAXTree") => serde_json::json!({"nodes": []}),
                    method => panic!("unexpected mutation-race command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({
                            "id": request["id"], "result": result
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        (format!("ws://{address}"), server)
    }

    async fn large_accessibility_server() -> (String, tokio::task::JoinHandle<()>, String) {
        let huge_text = "x".repeat(33 * 1024);
        let tree = serde_json::json!({
            "nodes": [
                {
                    "nodeId": "root",
                    "role": {"value": "RootWebArea"},
                    "name": {"value": huge_text.clone()},
                    "description": {"value": huge_text.clone()},
                    "value": {"value": huge_text.clone()},
                    "childIds": ["save"]
                },
                {
                    "nodeId": "save",
                    "parentId": "root",
                    "backendDOMNodeId": 42,
                    "role": {"value": "button"},
                    "name": {"value": "Save"},
                    "description": {"value": huge_text.clone()},
                    "value": {"value": huge_text.clone()},
                    "childIds": []
                }
            ]
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let text_for_server = huge_text.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            for _ in 0..5 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Runtime.evaluate") => serde_json::json!({
                        "result": {"value": serde_json::json!({
                            "url": "https://example.test",
                            "title": "Example",
                            "ready_state": "complete",
                            "text": text_for_server.clone(),
                        }).to_string()}
                    }),
                    Some("Accessibility.getFullAXTree") => tree.clone(),
                    method => panic!("unexpected compact-observation command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        (format!("ws://{address}"), server, huge_text)
    }

    #[test]
    fn normalizes_urls_without_touching_supported_schemes() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
        assert_eq!(normalize_url(" about:blank "), "about:blank");
        assert_eq!(
            normalize_url("file:///tmp/page.html"),
            "file:///tmp/page.html"
        );
    }

    #[test]
    fn interaction_modes_plan_smooth_or_direct_motion() {
        let mouse = MouseEngine::new();
        let start = Point { x: 10.0, y: 20.0 };
        let end = Point { x: 410.0, y: 220.0 };

        let human = interaction_path(InteractionMode::Human, &mouse, start, end);
        let fast = interaction_path(InteractionMode::Fast, &mouse, start, end);

        assert!(human.len() > 2);
        assert_eq!(human.first(), Some(&start));
        assert_eq!(human.last(), Some(&end));
        assert_eq!(fast, vec![start, end]);
    }

    #[test]
    fn topology_events_never_select_a_popup_and_clear_a_lost_active_target() {
        let mut topology = TopologyRegistry {
            active_target_id: Some("page-1".to_string()),
            active_session_id: Some("session-1".to_string()),
            ..TopologyRegistry::default()
        };
        let popup = CdpEventWithParams {
            method: "Target.targetCreated".to_string(),
            params: serde_json::json!({"targetInfo": {
                "type": "page", "targetId": "popup-1", "url": "about:blank",
                "title": "", "openerId": "page-1"
            }}),
            session_id: None,
        };
        assert!(!apply_topology_event(&mut topology, &popup));
        assert_eq!(topology.active_target_id.as_deref(), Some("page-1"));
        assert_eq!(topology.targets[0].opener_id.as_deref(), Some("page-1"));

        let crashed = CdpEventWithParams {
            method: "Target.targetCrashed".to_string(),
            params: serde_json::json!({"targetId": "page-1"}),
            session_id: None,
        };
        assert!(apply_topology_event(&mut topology, &crashed));
        assert!(topology.active_target_id.is_none());
        assert!(topology.events.len() <= TOPOLOGY_MAX_EVENTS);
    }

    #[test]
    fn frame_collection_is_bounded_and_preserves_parents() {
        let tree = serde_json::json!({
            "frame": {"id":"root", "url":"https://root.test"},
            "childFrames": [{"frame":{"id":"child", "url":"https://child.test"}}]
        });
        let mut frames = Vec::new();
        collect_frames(&tree, None, Some("child"), &mut frames).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].parent_id.as_deref(), Some("root"));
        assert!(frames[1].active);
    }

    #[test]
    fn keyboard_shortcuts_are_bounded_and_map_modifiers() {
        assert_eq!(
            parse_shortcut("Control+Shift+A").unwrap(),
            (10, "A".to_string())
        );
        assert_eq!(key_code("a"), "KeyA");
        assert!(parse_shortcut("Control+A+B").is_err());
        assert!(validate_key("").is_err());
    }

    #[test]
    fn invalidates_context_only_for_page_or_dom_mutations() {
        assert!(context_event_invalidates_observation(
            "DOM.childNodeInserted"
        ));
        assert!(context_event_invalidates_observation("Page.frameNavigated"));
        assert!(!context_event_invalidates_observation(
            "Network.loadingFinished"
        ));
    }

    #[test]
    fn structured_context_omits_screenshot_until_explicitly_populated() {
        let page = PageInfo {
            url: "https://example.test".to_string(),
            title: "Example".to_string(),
            ready_state: "complete".to_string(),
            target_id: "target-1".to_string(),
            frame_id: "frame-1".to_string(),
        };
        let mut context = PageContext {
            page: page.clone(),
            text: "Example".to_string(),
            dom: None,
            accessibility: CompactAccessibilitySnapshot {
                page,
                revision: 7,
                roots: Vec::new(),
                interactive: Vec::new(),
                truncated: false,
            },
            consistency: ObservationConsistency {
                consistent: true,
                attempts: 1,
                start_revision: 0,
                end_revision: 0,
                start_mutation_revision: 0,
                end_mutation_revision: 0,
            },
            boundaries: ObservationBoundarySummary::default(),
            incomplete: Vec::new(),
            screenshot: None,
        };

        let structured = serde_json::to_value(&context).unwrap();
        assert!(structured.get("dom").is_none());
        assert!(structured.get("screenshot").is_none());
        assert_eq!(structured["accessibility"]["revision"], 7);

        context.screenshot = Some("png-data".to_string());
        let visual = serde_json::to_value(&context).unwrap();
        assert_eq!(visual["screenshot"], "png-data");
    }

    #[test]
    fn revisioned_references_are_parsed_and_validate_their_shape() {
        assert_eq!(
            parse_revisioned_reference("r7:b42").unwrap(),
            Some(RevisionedElementReference {
                revision: 7,
                backend_dom_node_id: 42,
            })
        );
        assert_eq!(parse_revisioned_reference("Save").unwrap(), None);
        assert!(parse_revisioned_reference("r7:b0").is_err());
        assert!(parse_revisioned_reference("r:b42").is_err());
    }

    #[test]
    fn locators_parse_explicit_strategies_without_role_only_fallbacks() {
        assert_eq!(
            Locator::parse("r7:b42").unwrap(),
            Locator::Reference("r7:b42".to_string())
        );
        assert_eq!(
            Locator::parse("name=Save").unwrap(),
            Locator::AccessibleName("Save".to_string())
        );
        assert_eq!(
            Locator::parse("role=button;name=Save").unwrap(),
            Locator::RoleAndName {
                role: "button".to_string(),
                name: "Save".to_string(),
            }
        );
        assert_eq!(Locator::parse("ordinal=2").unwrap(), Locator::Ordinal(2));
        assert_eq!(
            Locator::parse("Save").unwrap(),
            Locator::AccessibleName("Save".to_string())
        );
        assert!(Locator::parse("role=button").is_err());
        assert!(Locator::parse("ordinal=0").is_err());
        assert!(Locator::parse("css=").is_err());
    }

    #[test]
    fn wait_conditions_parse_typed_forms_and_reject_unbounded_values() {
        assert_eq!(
            WaitCondition::parse("lifecycle=load").unwrap(),
            WaitCondition::Lifecycle("complete".to_string())
        );
        assert_eq!(
            WaitCondition::parse("target-visible=name=Save").unwrap(),
            WaitCondition::TargetVisible("name=Save".to_string())
        );
        assert_eq!(
            WaitCondition::parse("network-quiet=250").unwrap(),
            WaitCondition::NetworkQuiet(Duration::from_millis(250))
        );
        assert!(WaitCondition::parse("network-quiet=0").is_err());
        assert!(WaitCondition::parse(&format!("text={}", "x".repeat(4096))).is_err());
        assert!(WaitCondition::parse("lifecycle=forever").is_err());
        assert!(WaitCondition::parse("unknown=value").is_err());
        assert!(validate_wait_deadline(Duration::from_millis(1)).is_ok());
        assert!(validate_wait_deadline(Duration::from_secs(301)).is_err());
    }

    #[test]
    fn ambiguity_candidate_labels_are_utf8_safe_and_bounded() {
        let label = bounded_candidate_label(&"界".repeat(100));
        assert!(label.len() <= CANDIDATE_LABEL_MAX_BYTES);
        assert!(label.ends_with('…'));
        assert!(std::str::from_utf8(label.as_bytes()).is_ok());
    }

    #[test]
    fn action_outcomes_are_compact_and_serializable() {
        let outcome = ActionOutcome {
            action: ActionKind::Click,
            target: Some(ActionTarget {
                label: "button Save".to_string(),
                reference: Some("r9:b42".to_string()),
            }),
            revision: 10,
            target_id: "target-1".to_string(),
            frame_id: "frame-1".to_string(),
            evidence: None,
        };

        let value = serde_json::to_value(outcome).unwrap();
        assert_eq!(value["action"], "click");
        assert_eq!(value["target"]["reference"], "r9:b42");
        assert_eq!(value["revision"], 10);
    }

    #[test]
    fn full_snapshot_controls_use_revisioned_backend_references() {
        let roots = vec![AxNode {
            ax_node_id: "button".to_string(),
            backend_dom_node_id: Some(42),
            role: "button".to_string(),
            name: "Save".to_string(),
            description: String::new(),
            value: None,
            children: Vec::new(),
            bounds: None,
            interactive: true,
        }];
        let controls = interactive_elements(&roots, 12);
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].reference, "r12:b42");
        assert_eq!(controls[0].backend_dom_node_id, 42);
    }

    #[test]
    fn compact_text_cap_is_utf8_safe_and_marks_truncation() {
        let text = "🙂".repeat(COMPACT_TEXT_MAX_BYTES);
        let compact = truncate_visible_text(&text, COMPACT_TEXT_MAX_BYTES);

        assert!(compact.len() <= COMPACT_TEXT_MAX_BYTES);
        assert!(compact.ends_with(TEXT_TRUNCATION_MARKER));
        assert!(compact.is_char_boundary(compact.len()));
    }

    #[tokio::test]
    async fn default_observation_is_compact_and_never_requests_deep_dom() {
        let (url, server) = observation_server(false).await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        assert!(context.dom.is_none());
        assert!(context.screenshot.is_none());
        assert!(context.text.contains('😀'));
        assert!(context.text.ends_with(TEXT_TRUNCATION_MARKER));
        assert!(context.text.len() <= COMPACT_TEXT_MAX_BYTES);
        assert!(std::str::from_utf8(context.text.as_bytes()).is_ok());
        assert!(context.consistency.consistent);
        assert_eq!(context.consistency.attempts, 1);
        assert_eq!(context.boundaries.shadow_roots, 1);
        assert_eq!(
            context.incomplete,
            vec![
                ObservationIncompleteReason::VisibleText,
                ObservationIncompleteReason::ShadowBoundary,
                ObservationIncompleteReason::FrameBoundary,
                ObservationIncompleteReason::Canvas,
            ]
        );
        let serialized = serde_json::to_value(&context).unwrap();
        assert!(serialized.get("dom").is_none());
        assert!(serialized.get("screenshot").is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn mutation_race_retries_once_marks_incomplete_and_is_not_cached() {
        let (url, server) = mutation_race_server().await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        assert!(!context.consistency.consistent);
        assert_eq!(context.consistency.attempts, 2);
        assert!(
            context.consistency.end_mutation_revision > context.consistency.start_mutation_revision
        );
        assert!(
            context
                .incomplete
                .contains(&ObservationIncompleteReason::MutationRace)
        );
        assert!(session.observation_cache.lock().await.is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn deep_dom_observation_is_explicit_and_not_cached() {
        let (url, server) = observation_server(true).await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let deep = session.observe_with_dom().await.unwrap();
        assert_eq!(deep.dom.as_ref().unwrap().node_name, "#document");
        assert!(serde_json::to_value(&deep).unwrap().get("dom").is_some());

        let compact = session.observe().await.unwrap();
        assert!(compact.dom.is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn compact_observation_bounds_accessibility_while_snapshot_remains_full() {
        let (url, server, huge_text) = large_accessibility_server().await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        let serialized = serde_json::to_string(&context).unwrap();
        assert!(context.accessibility.truncated);
        assert_eq!(context.accessibility.revision, 1);
        assert_eq!(context.accessibility.roots[0].role, "RootWebArea");
        assert_eq!(context.accessibility.interactive[0].reference, "r1:b42");
        assert_eq!(context.accessibility.interactive[0].role, "button");
        assert_eq!(context.accessibility.interactive[0].name, "Save");
        assert!(!serialized.contains(&huge_text));
        assert!(
            serialized.len()
                <= COMPACT_TEXT_MAX_BYTES + crate::browser::dom::COMPACT_AX_TEXT_MAX_BYTES + 2_048
        );

        let cached = {
            let cache = session.observation_cache.lock().await;
            cache.as_ref().unwrap().context.clone()
        };
        let cached_json = serde_json::to_string(&cached.into_page_context()).unwrap();
        assert!(!cached_json.contains(&huge_text));

        let snapshot = session.snapshot().await.unwrap();
        assert_eq!(snapshot.roots[0].name, huge_text);
        assert_eq!(snapshot.interactive[0].reference, "r1:b42");
        assert_eq!(snapshot.interactive[0].description.len(), 33 * 1024);

        session.close().await.unwrap();
        server.await.unwrap();
    }
}

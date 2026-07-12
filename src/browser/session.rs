use base64::{Engine, engine::general_purpose::STANDARD};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::Mutex;

use super::cdp::CdpClient;
use super::chrome::{
    ChromeProcess, PortLaunchLock, check_chrome_health, get_ws_url, is_port_occupied,
    launch_chrome_with_options, resolve_chrome_path,
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
const COMPACT_PAGE_STATE_EXPRESSION: &str = "JSON.stringify({url: location.href, title: document.title, ready_state: document.readyState, text: document.body ? document.body.innerText : ''})";
const OWNED_BROWSER_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const AMBIGUOUS_CANDIDATE_LIMIT: usize = 8;
const CANDIDATE_LABEL_MAX_BYTES: usize = 160;
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const WAIT_LAST_STATE_MAX_BYTES: usize = 512;
const NETWORK_IN_FLIGHT_LIMIT: usize = 1024;
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
    /// Base64 PNG data is populated only when visual context is explicitly requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

/// The completed browser operation represented by an [`ActionOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Click,
    DoubleClick,
    Type,
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
}

#[derive(Debug, Clone)]
struct CompactPageContext {
    page: PageInfo,
    text: String,
    accessibility: CompactAccessibilitySnapshot,
}

impl CompactPageContext {
    fn into_page_context(self) -> PageContext {
        PageContext {
            page: self.page,
            text: self.text,
            dom: None,
            accessibility: self.accessibility,
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
        let cdp = match CdpClient::connect(&ws_url).await {
            Ok(cdp) => cdp,
            Err(error) => {
                if let Some(process) = chrome.as_mut() {
                    let _ = process.shutdown().await;
                }
                return Err(error);
            }
        };

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

        Ok(Self {
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
        })
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
        Ok(serde_json::from_str(json)?)
    }

    pub async fn navigate(&self, url: &str) -> BrowserResult<PageInfo> {
        let url = normalize_url(url);
        let mut events = self.cdp.subscribe_events();
        self.cdp.navigate(&url).await?;
        self.wait_for_page_event("Page.loadEventFired", &mut events, Duration::from_secs(20))
            .await?;
        let page = self.page_info().await?;
        self.invalidate_observation();
        Ok(page)
    }

    async fn wait_for_page_event(
        &self,
        method: &str,
        events: &mut tokio::sync::broadcast::Receiver<super::cdp::CdpEvent>,
        deadline: Duration,
    ) -> BrowserResult<()> {
        let wait = tokio::time::timeout(deadline, async {
            loop {
                match events.recv().await {
                    Ok(event) if event.method == method => return Ok(()),
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return Err("CDP event stream closed during wait"),
                }
            }
        })
        .await;
        match wait {
            Ok(result) => result.map_err(Into::into),
            Err(_) => Err(WaitTimeout {
                condition: "lifecycle".to_string(),
                deadline_ms: deadline.as_millis() as u64,
                last_state: "load_event_not_observed".to_string(),
                reason: "deadline_exceeded",
            }
            .into()),
        }
    }

    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        let result = self.evaluate_value(expression).await;
        // Arbitrary JavaScript may mutate DOM, styles, form state, or history.
        // Invalidate synchronously so the next cached observation cannot race
        // the asynchronous CDP mutation event stream.
        self.invalidate_observation();
        result
    }

    pub async fn text(&self) -> BrowserResult<String> {
        let value = self
            .evaluate_value("document.body ? document.body.innerText : ''")
            .await?;
        Ok(truncate_visible_text(
            value.as_str().unwrap_or_default(),
            COMPACT_TEXT_MAX_BYTES,
        ))
    }

    /// Fetch the full DOM only for an explicit deep-inspection operation.
    pub async fn deep_dom(&self) -> BrowserResult<DomNode> {
        let raw = self.cdp.get_deep_document().await?;
        parse_dom_tree(&raw).ok_or_else(|| {
            "CDP deep DOM response contained no parseable root node"
                .to_string()
                .into()
        })
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

        let (page_state, accessibility) =
            tokio::join!(self.compact_page_state(), self.cdp.get_accessibility_tree(),);
        let page_state = page_state?;
        let accessibility_raw = accessibility?;
        let page = PageInfo {
            url: page_state.url,
            title: page_state.title,
            ready_state: page_state.ready_state,
        };
        let full_roots = parse_accessibility_tree(&accessibility_raw);
        let compact_accessibility = project_compact_accessibility(&full_roots, revision);
        let accessibility = CompactAccessibilitySnapshot {
            page: page.clone(),
            revision,
            roots: compact_accessibility.roots,
            interactive: compact_accessibility.interactive,
            truncated: compact_accessibility.truncated,
        };
        let context = CompactPageContext {
            page,
            text: truncate_visible_text(&page_state.text, COMPACT_TEXT_MAX_BYTES),
            accessibility,
        };
        *self.observation_cache.lock().await = Some(CachedObservation {
            revision,
            context: context.clone(),
        });
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
        Ok(self.cdp.screenshot("png").await?)
    }

    pub async fn scroll(&self, dx: f64, dy: f64) -> BrowserResult<ActionOutcome> {
        self.cdp.scroll_by(dx, dy).await?;
        Ok(ActionOutcome {
            action: ActionKind::Scroll,
            target: None,
            revision: self.invalidate_observation(),
        })
    }

    pub async fn snapshot(&self) -> BrowserResult<AccessibilitySnapshot> {
        let revision = self.page_revision.load(Ordering::Relaxed);
        let raw = self.cdp.get_accessibility_tree().await?;
        let roots = parse_accessibility_tree(&raw);
        let interactive = interactive_elements(&roots, revision);
        Ok(AccessibilitySnapshot {
            page: self.page_info().await?,
            roots,
            interactive,
        })
    }

    pub async fn wait(
        &self,
        condition: WaitCondition,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        if deadline.is_zero() {
            return Err("wait deadline must be positive".into());
        }
        if let WaitCondition::NetworkQuiet(quiet) = condition {
            return self.wait_for_network_quiet(quiet, deadline).await;
        }
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut events = self.cdp.subscribe_events();
        let mut previous_geometry = None;
        let description = condition.description();
        loop {
            let (matched, state, geometry) = self
                .check_wait_condition(&condition, previous_geometry.as_deref())
                .await?;
            let last_state = bounded_wait_state(&state);
            previous_geometry = geometry;
            if matched {
                return Ok(WaitOutcome {
                    condition: description,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state,
                });
            }
            let now = tokio::time::Instant::now();
            if now >= expires {
                return Err(WaitTimeout {
                    condition: description,
                    deadline_ms: deadline.as_millis() as u64,
                    last_state,
                    reason: "deadline_exceeded",
                }
                .into());
            }
            let remaining = expires - now;
            tokio::select! {
                _ = tokio::time::sleep(WAIT_POLL_INTERVAL.min(remaining)) => {}
                event = events.recv() => match event {
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
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
                let text = self.text().await?;
                Ok((text.contains(expected), text, None))
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
        self.cdp.enable_network().await?;
        let mut guard = NetworkDomainGuard {
            cdp: self.cdp.clone(),
            armed: true,
        };
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut empty_since = started;
        let mut in_flight = HashSet::new();
        let mut overflow_in_flight = 0usize;
        loop {
            let now = tokio::time::Instant::now();
            if in_flight.is_empty() && overflow_in_flight == 0 && now.duration_since(empty_since) >= quiet {
                guard.disable().await?;
                return Ok(WaitOutcome {
                    condition: "network_quiet".to_string(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state: "in_flight=0".to_string(),
                });
            }
            if now >= expires {
                return Err(WaitTimeout {
                    condition: "network_quiet".to_string(),
                    deadline_ms: deadline.as_millis() as u64,
                    last_state: format!("in_flight={}", in_flight.len() + overflow_in_flight),
                    reason: "deadline_exceeded",
                }
                .into());
            }
            tokio::select! {
                _ = tokio::time::sleep((expires - now).min(WAIT_POLL_INTERVAL)) => {}
                event = events.recv() => if let Ok(event) = event {
                    let request_id = event.params["requestId"].as_str();
                    match event.method.as_str() {
                        "Network.requestWillBeSent" => {
                            if let Some(id) = request_id {
                                if in_flight.len() < NETWORK_IN_FLIGHT_LIMIT {
                                    in_flight.insert(id.to_string());
                                } else {
                                    overflow_in_flight = overflow_in_flight.saturating_add(1);
                                }
                            }
                        }
                        "Network.loadingFinished" | "Network.loadingFailed" => {
                            if let Some(id) = request_id
                                && !in_flight.remove(id)
                                && overflow_in_flight > 0
                            {
                                overflow_in_flight -= 1;
                            }
                            if in_flight.is_empty() && overflow_in_flight == 0 { empty_since = tokio::time::Instant::now(); }
                        }
                        _ => {}
                    }
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

    async fn pointer_click(
        &self,
        target: &str,
        double_click: bool,
    ) -> BrowserResult<ActionOutcome> {
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
        let point = self.verified_action_point(&remote.object_id).await?;
        let events = if double_click {
            self.mouse.generate_double_click_events(point)
        } else {
            self.mouse.generate_click_events(point)
        };
        self.dispatch_pointer_events(&remote.object_id, point, events)
            .await?;
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
        })
    }

    async fn dispatch_pointer_events(
        &self,
        object_id: &str,
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
        if (press_point.x - point.x).abs() > 1.0 || (press_point.y - point.y).abs() > 1.0 {
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
        let target = match target {
            Some(target) => self.click(target).await?.target,
            None => None,
        };
        self.cdp.insert_text(text).await?;
        Ok(ActionOutcome {
            action: ActionKind::Type,
            target,
            revision: self.invalidate_observation(),
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
    if text.len() <= max_bytes {
        return text.to_string();
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
    truncated
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
    armed: bool,
}

impl NetworkDomainGuard {
    async fn disable(&mut self) -> BrowserResult<()> {
        self.cdp.disable_network().await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for NetworkDomainGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        tokio::spawn(async move {
            let _ = cdp.disable_network().await;
        });
    }
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
            "network-quiet" => Self::NetworkQuiet(Duration::from_millis(argument.parse::<u64>()?)),
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
}

fn bounded_wait_state(value: &str) -> String {
    truncate_visible_text(value, WAIT_LAST_STATE_MAX_BYTES)
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

            for _ in 0..if include_dom { 3 } else { 2 } {
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
            for _ in 0..4 {
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
        assert!(WaitCondition::parse("network-quiet=0").is_ok());
        assert!(WaitCondition::parse("lifecycle=forever").is_err());
        assert!(WaitCondition::parse("unknown=value").is_err());
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
        let serialized = serde_json::to_value(&context).unwrap();
        assert!(serialized.get("dom").is_none());
        assert!(serialized.get("screenshot").is_none());

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

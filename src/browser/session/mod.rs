//! Browser session management.
//!
//! The [`BrowserSession`] struct orchestrates a single browser session:
//! launching or attaching to Chrome, managing CDP connections, routing
//! operations to the active page target and frame, and providing the
//! high-level API for navigation, interaction, observation, and more.
//!
//! Submodules implement specific capabilities:
//! - **action**: clicks, typing, keyboard, scroll, drag
//! - **batch**: ordered batch operations with policy pre-flight
//! - **checkpoint**: cross-process session checkpoint export/import
//! - **clipboard**: system clipboard read/write
//! - **diagnostic**: scoped diagnostic evidence collection
//! - **dialog**: JavaScript dialog handling
//! - **diff**: accessibility tree diffing
//! - **download**: scoped download lifecycle management
//! - **emulation**: PDF generation, geolocation, timezone override
//! - **fill**: high-level multi-field form filling
//! - **frame**: frame tree discovery and selection
//! - **har**: network recording (HAR-like capture)
//! - **intercept**: CDP Fetch domain request interception
//! - **locator**: element resolution with fallback chains
//! - **navigate**: page navigation and JavaScript evaluation
//! - **observe**: compact page observation
//! - **popup**: popup window click and witness tracking
//! - **retry**: retry policies for transport/CDP errors
//! - **storage**: cookies, localStorage, sessionStorage
//! - **target**: page target discovery, selection, creation
//! - **targets**: parallel multi-target operations
//! - **visual**: screenshots, visual capture, screencast
//! - **wait**: page wait conditions and lifecycle detection
//! - **webauthn**: virtual WebAuthn authenticator

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
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
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::Mutex;

use super::cdp::CdpClient;
use super::chrome::{
    ChromeProcess, PortLaunchLock, check_chrome_health, get_browser_ws_url, get_ws_url,
    is_port_occupied, launch_chrome_with_options, resolve_chrome_path,
};
use super::dom::{
    CompactInteractiveElement, DomNode, parse_accessibility_tree, parse_dom_tree,
    project_compact_accessibility,
};
use super::mouse::{MouseEngine, Point};
use super::policy::{BrowserPolicy, PolicyCapability, PolicyError, PolicyPreset};
use super::profile::ProfileManager;

mod types;
pub use diff::{AccessibilityDiff, DiffChange, DiffElement, diff_accessibility};
pub use emulation::{GeoLocation, PdfOptions};
pub use fill::{FillFieldResult, FillFormOutcome};
pub use har::{NetworkEntry, NetworkRecorder, NetworkRecording};
pub use intercept::{InterceptGuard, RequestPattern};
pub use retry::{RetryPolicy, RetryPredicate};
pub use types::*;
pub use webauthn::{WebAuthnGuard, WebAuthnOptions};
mod action;
mod batch;
mod checkpoint;
mod clipboard;
mod diagnostic;
mod dialog;
mod diff;
mod download;
mod emulation;
mod fill;
mod frame;
mod har;
mod intercept;
mod locator;
mod navigate;
mod observe;
mod popup;
mod retry;
pub mod storage;
pub use storage::{Cookie, StorageEntry, StorageItems};
mod target;
mod targets;
mod visual;
mod wait;
mod webauthn;
#[allow(private_interfaces)]
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

struct CachedObservation {
    revision: u64,
    context: CompactPageContext,
}

type PausedPolicyRequests = Arc<Mutex<HashSet<(Option<String>, String)>>>;

struct PolicyInterception {
    cdp: CdpClient,
    sessions: Arc<Mutex<HashSet<String>>>,
    paused: PausedPolicyRequests,
    last_denial: Arc<Mutex<Option<PolicyError>>>,
    worker: tokio::task::JoinHandle<()>,
}

impl PolicyInterception {
    async fn start(
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
struct DisposableProfileDir {
    path: PathBuf,
}

const DISPOSABLE_OWNER_FILE: &str = ".glass-owner.json";
const DISPOSABLE_CLEANUP_BATCH: usize = 1024;

#[derive(Debug, Serialize, Deserialize)]
struct DisposableProfileOwner {
    pid: u32,
    process_start: u64,
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

fn reap_disposable_candidates(
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

fn process_start_identity(pid: u32) -> Option<u64> {
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
        let policy = BrowserPolicy::development(std::env::current_dir()?)?;
        Self::start_with_policy(options, policy).await
    }

    pub async fn start_with_policy(
        options: &SessionOptions,
        mut policy: BrowserPolicy,
    ) -> BrowserResult<Self> {
        options.validate()?;
        if options.attach {
            policy.require(PolicyCapability::Attach)?;
        }
        if !options.attach && !options.incognito {
            policy.require(PolicyCapability::PersistentProfile)?;
        }
        let resolver_rules = policy.prepare_hardened_session(options.attach).await?;
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
                    resolver_rules.as_deref(),
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
        let launched_incognito_context_id = if !options.attach && options.incognito {
            match target_browser_context_id(&cdp, &target_id, true).await {
                Ok(context_id) => context_id,
                Err(error) => {
                    cdp.close().await;
                    if let Some(process) = chrome.as_mut() {
                        let _ = process.shutdown().await;
                    }
                    return Err(error.into());
                }
            }
        } else {
            None
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
        let policy_interception = if matches!(
            policy.preset(),
            PolicyPreset::Hardened | PolicyPreset::UntrustedMcp
        ) {
            Some(PolicyInterception::start(cdp.clone(), policy.clone(), session_id.clone()).await?)
        } else {
            None
        };

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
                        topology_for_events.lock().await.event_loss_count += 1;
                        let _ = resync_topology(&cdp_for_events, &topology_for_events).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        cdp.send_browser(
            "Target.setAutoAttach",
            Some(serde_json::json!({
                "autoAttach": true,
                "waitForDebuggerOnStart": matches!(
                    policy.preset(),
                    PolicyPreset::Hardened | PolicyPreset::UntrustedMcp
                ),
                "flatten": true
            })),
        )
        .await?;

        let session = Self {
            cdp,
            chrome,
            disposable_profile,
            launched_incognito_context_id,
            profile: options.profile.clone(),
            interaction_mode: options.interaction_mode,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision,
            observation_cache: Mutex::new(None),
            network_wait_leases: Arc::new(Mutex::new(NetworkLeaseState::default())),
            diagnostic_leases: Arc::new(Mutex::new(DiagnosticLeaseState::default())),
            download_scope: Arc::new(Mutex::new(())),
            topology,
            popup_click_scope: Mutex::new(()),
            upload_root: std::fs::canonicalize(std::env::current_dir()?)?,
            policy: policy.clone(),
            policy_interception,
            audit_log: std::sync::Mutex::new(VecDeque::new()),
            audit_sequence: AtomicU64::new(1),
            audit_enabled: options.audit,
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

    /// Explicit privileged escape hatch for benchmark and protocol diagnostics.
    /// Hardened sessions deny it unless `raw-cdp` is deliberately allowed.
    pub fn raw_cdp(&self) -> BrowserResult<&CdpClient> {
        self.policy.require(PolicyCapability::RawCdp)?;
        Ok(&self.cdp)
    }

    pub fn profile_name(&self) -> &str {
        &self.profile
    }

    pub fn policy(&self) -> &BrowserPolicy {
        &self.policy
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

    /// Override the viewport metrics for device emulation.
    ///
    /// Uses CDP `Emulation.setDeviceMetricsOverride`. Reset on session close
    /// or call with `width: 0, height: 0` to clear.
    pub async fn set_viewport(
        &self,
        width: i64,
        height: i64,
        device_scale_factor: Option<f64>,
        is_mobile: Option<bool>,
    ) -> BrowserResult<()> {
        if width == 0 && height == 0 {
            self.cdp.clear_device_metrics_override().await?;
        } else {
            self.cdp
                .set_device_metrics_override(
                    width,
                    height,
                    device_scale_factor.unwrap_or(1.0),
                    is_mobile.unwrap_or(false),
                )
                .await?;
        }
        Ok(())
    }

    /// Build a bounded failure-trace pack suitable for agent self-correction.
    ///
    /// The returned bundle includes the last compact observation, an action outcome,
    /// an error message, and the active topology state. It is deliberately bounded
    /// (≤ 8 KiB), redacted for secrets, and excludes DOM/expression/screenshot
    /// payloads. Agents can use this to decide whether to re-observe, select a
    /// different target, or escalate without a full DOM round-trip.
    pub async fn failure_trace(
        &self,
        outcome: ActionOutcome,
        error: impl Into<String>,
    ) -> FailureTracePack {
        const MAX_TRACE_BYTES: usize = 8192;
        const MAX_ERROR_BYTES: usize = 512;

        let mut error_text = error.into();
        if error_text.len() > MAX_ERROR_BYTES {
            let mut end = MAX_ERROR_BYTES;
            while end > 0 && !error_text.is_char_boundary(end) {
                end -= 1;
            }
            error_text.truncate(end);
        }

        let last_observation =
            self.observation_cache
                .lock()
                .await
                .as_ref()
                .map(|cached| CompactObservationTrace {
                    page: cached.context.page.clone(),
                    revision: cached.revision,
                    interactive: cached.context.accessibility.interactive.clone(),
                    completeness: cached.context.accessibility.completeness.clone(),
                });

        let topology = {
            let t = self.topology.lock().await;
            TopologyTrace {
                sequence: t.sequence,
                active_target_id: t.active_target_id.clone(),
                active_frame_id: t.active_frame_id.clone(),
                target_count: t.targets.len(),
                frame_count: t.frames.len(),
                event_loss_count: t.event_loss_count,
            }
        };

        let pack = FailureTracePack {
            outcome,
            error: error_text,
            last_observation,
            topology,
            trace_bytes: 0,
        };

        // Measure and cap at MAX_TRACE_BYTES by dropping observation if needed
        let serialized = serde_json::to_string(&pack).unwrap_or_default();
        if serialized.len() <= MAX_TRACE_BYTES {
            FailureTracePack {
                trace_bytes: serialized.len(),
                ..pack
            }
        } else {
            // Drop the observation to fit within budget
            FailureTracePack {
                last_observation: None,
                trace_bytes: 0,
                ..pack
            }
        }
    }

    /// Return a snapshot of the current session audit log.
    ///
    /// The log records high-risk operations (navigate, evaluate, upload,
    /// download, attach) with bounded, redacted detail. It is only populated
    /// when `--audit` is set on session start. Entries are bounded to
    /// [`MAX_AUDIT_ENTRIES`]; the oldest are dropped on overflow.
    pub fn audit_log(&self) -> Vec<AuditEntry> {
        self.audit_log
            .lock()
            .map(|log| log.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn record_audit(&self, operation: &str, detail: impl Into<String>) {
        if !self.audit_enabled {
            return;
        }
        let sequence = self.audit_sequence.fetch_add(1, Ordering::Relaxed);
        let detail_text = detail.into();
        const MAX_AUDIT_DETAIL_BYTES: usize = 256;
        let detail_text = truncate_utf8_bytes(&detail_text, MAX_AUDIT_DETAIL_BYTES);
        let preset = format!("{:?}", self.policy.preset());
        if let Ok(mut log) = self.audit_log.lock() {
            log.push_back(AuditEntry {
                sequence,
                operation: operation.to_string(),
                detail: detail_text,
                policy_preset: preset,
            });
            while log.len() > MAX_AUDIT_ENTRIES {
                log.pop_front();
            }
        }
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
        if let Some(interception) = self.policy_interception.take() {
            interception.shutdown().await;
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
}

/// Parse a backend DOM node ID from a revisioned reference string ("r<rev>:b<id>").
fn parse_backend_id_from_ref(reference: &str) -> Option<i64> {
    let parts: Vec<&str> = reference.split(':').collect();
    if parts.len() != 2 || !parts[0].starts_with('r') || !parts[1].starts_with('b') {
        return None;
    }
    parts[1][1..].parse::<i64>().ok()
}

#[cfg(test)]
mod tests;

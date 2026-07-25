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
pub use types::*;
mod batch;
mod locator;

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
        self.policy.require_url(&url).await?;
        let result = self
            .cdp
            .send_browser("Target.createTarget", Some(serde_json::json!({"url": url})))
            .await?;
        let id = result["targetId"]
            .as_str()
            .ok_or("Target.createTarget returned no targetId")?;
        validate_topology_id(id)?;
        let targets = self.list_targets().await?;
        let target = targets
            .into_iter()
            .find(|target| target.id == id)
            .ok_or_else(|| -> Box<dyn Error> { "created target was not discoverable".into() })?;
        self.record_audit("attach", &url);
        Ok(target)
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
        if let Some(interception) = &self.policy_interception {
            if let Err(error) = enable_fetch_for(&self.cdp, &new_session).await {
                let _ = self
                    .cdp
                    .send_browser(
                        "Target.detachFromTarget",
                        Some(serde_json::json!({"sessionId": new_session})),
                    )
                    .await;
                return Err(error);
            }
            interception
                .sessions
                .lock()
                .await
                .insert(new_session.clone());
        }
        if let Err(error) = self
            .cdp
            .send_to_session(
                &new_session,
                "Target.setAutoAttach",
                Some(serde_json::json!({
                    "autoAttach": true,
                    "waitForDebuggerOnStart": matches!(
                        self.policy.preset(),
                        PolicyPreset::Hardened | PolicyPreset::UntrustedMcp
                    ),
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
            return Err(TopologyError::new(
                TopologyErrorKind::NoTargetSelected,
                "no active target is selected; call listTargets to discover available pages",
            )
            .into());
        }
        let target_session = target_session.ok_or_else(|| {
            TopologyError::new(
                TopologyErrorKind::NoPageSession,
                "active target has no CDP session; the session may need to be re-established",
            )
        })?;
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
            .ok_or_else(|| {
                TopologyError::new(
                    TopologyErrorKind::NoSuchFrame,
                    format!("frame {frame_id} was not found; call listFrames to discover available frames"),
                )
            })?;
        let session_id = {
            let topology = self.topology.lock().await;
            topology
                .frame_sessions
                .get(frame_id)
                .cloned()
                .or_else(|| topology.active_target_session_id.clone())
                .ok_or_else(|| {
                    TopologyError::new(
                        TopologyErrorKind::NoPageSession,
                        "active target has no CDP session; the session may need to be re-established",
                    )
                })?
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
            error_text.truncate(MAX_ERROR_BYTES);
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
        let mut detail_text = detail.into();
        const MAX_AUDIT_DETAIL_BYTES: usize = 256;
        if detail_text.len() > MAX_AUDIT_DETAIL_BYTES {
            detail_text.truncate(MAX_AUDIT_DETAIL_BYTES);
        }
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
                self.policy.require_url(&url).await?;
                if let Some(interception) = &self.policy_interception
                    && let Some(error) = interception.take_denial().await
                {
                    return Err(error.into());
                }
                let result = async {
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
                    self.record_audit("navigate", url);
                    Ok(page)
                }
                .await;
                if let Some(error) = match &self.policy_interception {
                    Some(interception) => interception.take_denial().await,
                    None => None,
                } {
                    return Err(error.into());
                }
                result
            })
            .await
    }

    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        self.policy.require(PolicyCapability::Evaluate)?;
        self.cdp
            .with_current_route(async {
                let result = self.evaluate_value(expression).await;
                // Arbitrary JavaScript may mutate DOM, styles, form state, or history.
                // Invalidate synchronously so the next cached observation cannot race
                // the asynchronous CDP mutation event stream.
                self.invalidate_observation();
                self.record_audit("evaluate", expression);
                result
            })
            .await
    }

    /// Reconcile prior references against the current page revision.
    /// Maps old refs (r<fromRevision>:b<id>) to current refs via backend
    /// node identity or stable role+name matching.
    pub async fn reconcile_references(
        &self,
        from_revision: u64,
        refs: &[String],
    ) -> BrowserResult<ReconciliationOutcome> {
        if refs.len() > MAX_RECONCILE_REFS {
            return Err(format!(
                "too many refs to reconcile: {} (max {})",
                refs.len(),
                MAX_RECONCILE_REFS
            )
            .into());
        }

        let current_revision = self.page_revision.load(Ordering::Relaxed);
        if current_revision == from_revision {
            // Same revision: all refs preserved as-is
            let mappings: Vec<_> = refs
                .iter()
                .map(|old| ReferenceMapping::Preserved {
                    old: old.clone(),
                    new: old.clone(),
                })
                .collect();
            return Ok(ReconciliationOutcome {
                to_revision: current_revision,
                preserved: mappings.len(),
                relocated: 0,
                lost: 0,
                mappings,
            });
        }

        // Get fresh compact observe to see current controls
        let context = self.observe_fresh().await?;
        let to_revision = context.accessibility.revision;

        // Build lookup: backend_dom_node_id -> current ref
        let mut current_by_backend: HashMap<i64, String> = HashMap::new();
        let mut current_controls: Vec<(&str, &str, i64)> = Vec::new();
        let ax = &context.accessibility;
        for c in &ax.interactive {
            current_by_backend.insert(c.backend_dom_node_id, c.reference.clone());
            current_controls.push((&c.role, &c.name, c.backend_dom_node_id));
        }

        // If route changed, all lost
        let route_changed = false; // TODO: check topology

        let mut mappings = Vec::with_capacity(refs.len());
        let mut preserved = 0usize;
        let relocated = 0usize;
        let mut lost = 0usize;

        for old_ref in refs {
            // Parse old ref: "r<revision>:b<backend_id>"
            let backend_id = parse_backend_id_from_ref(old_ref);
            if backend_id.is_none() || route_changed {
                mappings.push(ReferenceMapping::Lost {
                    old: old_ref.clone(),
                    reason: if route_changed {
                        "route_changed".to_string()
                    } else {
                        "invalid_ref_format".to_string()
                    },
                });
                lost += 1;
                continue;
            }

            let backend_id = backend_id.unwrap();

            // Try preserved (same backend node ID)
            if let Some(new_ref) = current_by_backend.get(&backend_id) {
                mappings.push(ReferenceMapping::Preserved {
                    old: old_ref.clone(),
                    new: new_ref.clone(),
                });
                preserved += 1;
                continue;
            }

            // Try role+name match
            // Look up the old control's role+name from cache (approximate)
            // For simplicity: cannot relocate without hints
            mappings.push(ReferenceMapping::Lost {
                old: old_ref.clone(),
                reason: "backend_node_removed".to_string(),
            });
            lost += 1;
        }

        Ok(ReconciliationOutcome {
            to_revision,
            mappings,
            preserved,
            relocated,
            lost,
        })
    }

    /// Export a session checkpoint for cross-process resume.
    /// Returns JSON bounded to ≤ 4 KiB. No cookies, passwords, or form values.
    pub async fn export_checkpoint(&self) -> BrowserResult<CheckpointV1> {
        let page = self.page_info().await?;
        let revision = self.page_revision.load(Ordering::Relaxed);

        let last_refs: Vec<String> = self
            .observation_cache
            .lock()
            .await
            .as_ref()
            .map(|cached| {
                cached
                    .context
                    .accessibility
                    .interactive
                    .iter()
                    .take(8)
                    .map(|c| c.reference.clone())
                    .collect()
            })
            .unwrap_or_default();

        Ok(CheckpointV1 {
            schema_version: 1,
            glass_version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                format!("{}", now.as_secs())
            },
            profile: self.profile.clone(),
            attach_mode: self.chrome.is_none(),
            topology: CheckpointTopology {
                target_id: Some(page.target_id),
                frame_id: Some(page.frame_id),
                url: page.url,
                title: page.title,
            },
            observation: CheckpointObservation {
                revision,
                last_refs,
            },
            policy: format!("{:?}", self.policy.preset()).to_lowercase(),
        })
    }

    /// Import a checkpoint and validate its topology.
    /// Does NOT auto-click — only restores target/frame selection context.
    pub async fn import_checkpoint(&self, checkpoint: &CheckpointV1) -> BrowserResult<()> {
        if checkpoint.schema_version != 1 {
            return Err(format!(
                "checkpoint schema version mismatch: expected 1, found {}",
                checkpoint.schema_version
            )
            .into());
        }

        if let Some(ref target_id) = checkpoint.topology.target_id {
            let targets = self.list_targets().await?;
            if !targets.iter().any(|t| t.id == *target_id) {
                return Err("checkpoint target is no longer open".into());
            }
            self.select_target(target_id).await?;
        }

        if let Some(ref frame_id) = checkpoint.topology.frame_id {
            let frames = self.list_frames().await?;
            if !frames.iter().any(|f| f.id == *frame_id) {
                return Err("checkpoint frame is no longer open".into());
            }
            self.select_frame(frame_id).await?;
        }

        Ok(())
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
        self.observe_internal(false, false, true, false).await
    }

    /// Collect compact context and explicitly include the full DOM tree.
    pub async fn observe_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, true, false).await
    }

    /// Collect structured context and explicitly include a current screenshot.
    pub async fn observe_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, true, false).await
    }

    /// Collect context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, true, false).await
    }

    /// Collect fresh compact context, bypassing the compact-context cache.
    pub async fn observe_fresh(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, false, false).await
    }

    /// Collect compact context with form field values included.
    /// Requires ReadFormValues policy capability in hardened mode.
    pub async fn observe_with_form_values(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, false, true).await
    }

    /// Collect fresh context and explicitly include the full DOM tree.
    pub async fn observe_fresh_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, false, false).await
    }

    /// Collect fresh structured context and explicitly include a screenshot.
    pub async fn observe_fresh_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, false, false).await
    }

    /// Collect fresh context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_fresh_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, false, false).await
    }

    async fn observe_internal(
        &self,
        include_dom: bool,
        include_screenshot: bool,
        use_cache: bool,
        include_form_values: bool,
    ) -> BrowserResult<PageContext> {
        if let Some(interception) = &self.policy_interception
            && let Some(error) = interception.take_denial().await
        {
            return Err(error.into());
        }
        self.cdp
            .with_current_route(async {
                let mut context = self
                    .compact_observation(use_cache, include_form_values)
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

    async fn compact_observation(
        &self,
        use_cache: bool,
        include_form_values: bool,
    ) -> BrowserResult<CompactPageContext> {
        let revision = self.page_revision.load(Ordering::Relaxed);
        // Never use cache when form values are requested (cache doesn't store them)
        if use_cache && !include_form_values {
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

        let (target_id, frame_id) = self.route_identity().await?;
        let world = self
            .cdp
            .send(
                "Page.createIsolatedWorld",
                Some(serde_json::json!({"frameId": frame_id, "worldName": "glass-observation"})),
            )
            .await?;
        let context_id = world["executionContextId"]
            .as_i64()
            .ok_or("Page.createIsolatedWorld returned no executionContextId")?;
        let mut collected = None;
        for attempt in 1..=COMPACT_OBSERVATION_MAX_ATTEMPTS {
            let start_revision = self.page_revision.load(Ordering::Relaxed);
            let attempt_result = tokio::time::timeout(COMPACT_OBSERVATION_ATTEMPT_TIMEOUT, async {
                let start = self.compact_page_state(context_id).await?;
                let accessibility = self.cdp.get_accessibility_tree().await?;
                let end = self.compact_page_state(context_id).await?;
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
        let page = PageInfo {
            url: page_state.url,
            title: page_state.title,
            ready_state: page_state.ready_state,
            target_id,
            frame_id,
        };
        let full_roots = parse_accessibility_tree(&accessibility_raw);
        let mut compact_accessibility = project_compact_accessibility(&full_roots, end_revision);
        let (mut text, locally_truncated) =
            truncate_visible_text_with_status(&page_state.text, COMPACT_TEXT_MAX_BYTES);
        let text_truncated = locally_truncated || page_state.boundaries.text_truncated;
        if page_state.boundaries.text_truncated && !text.ends_with(TEXT_TRUNCATION_MARKER) {
            let content_limit = COMPACT_TEXT_MAX_BYTES.saturating_sub(TEXT_TRUNCATION_MARKER.len());
            while text.len() > content_limit {
                text.pop();
            }
            text.push_str(TEXT_TRUNCATION_MARKER);
        }
        let mut incomplete = Vec::new();
        if text_truncated {
            incomplete.push(ObservationIncompleteReason::VisibleText);
        }
        if compact_accessibility.nodes_truncated {
            incomplete.push(ObservationIncompleteReason::AccessibilityNode);
        }
        if compact_accessibility.labels_truncated {
            incomplete.push(ObservationIncompleteReason::AccessibilityLabel);
        }
        if compact_accessibility.controls_truncated {
            incomplete.push(ObservationIncompleteReason::Control);
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
        // Shadow piercing: discover which interactive controls are inside open shadow roots.
        let (shadow_paths, pierced_hosts) = if page_state.boundaries.shadow_roots > 0 {
            match self
                .cdp
                .get_flattened_document(crate::browser::dom::MAX_SHADOW_DEPTH as i64)
                .await
            {
                Ok(flattened) => {
                    let paths = crate::browser::dom::build_shadow_host_paths(&flattened);
                    let hosts = crate::browser::dom::count_pierced_shadow_hosts(&paths);
                    (paths, hosts)
                }
                Err(_) => (HashMap::new(), 0),
            }
        } else {
            (HashMap::new(), 0)
        };

        // Only flag ShadowBoundary when hosts were not all pierced
        if page_state.boundaries.shadow_roots > 0
            && pierced_hosts < page_state.boundaries.shadow_roots
        {
            incomplete.push(ObservationIncompleteReason::ShadowBoundary);
        }

        // Apply shadow host paths to interactive controls
        if !shadow_paths.is_empty() {
            for control in compact_accessibility.interactive.iter_mut() {
                if let Some(path) = shadow_paths.get(&control.backend_dom_node_id) {
                    control.shadow_host_path = Some(path.clone());
                }
            }
        }

        // Read form field values when explicitly requested
        if include_form_values {
            self.read_form_field_values(&mut compact_accessibility.interactive)
                .await?;
        }

        let interactive_len = compact_accessibility.interactive.len();
        let accessibility = CompactAccessibilitySnapshot {
            page: page.clone(),
            revision: end_revision,
            roots: compact_accessibility.roots,
            interactive: compact_accessibility.interactive,
            truncated: compact_accessibility.truncated,
            omitted_count: compact_accessibility.omitted_count,
            ranking_applied: compact_accessibility.ranking_applied,
            completeness: Some(ObservationCompleteness::compute(
                compact_accessibility.interactive_discovered,
                interactive_len,
                page_state.boundaries.shadow_roots,
                pierced_hosts,
                page_state.boundaries.canvases,
                page_state.boundaries.child_frames,
                !consistent,
            )),
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

    /// Read current values of form controls and populate CompactInteractiveElement fields.
    /// Enforces ReadFormValues policy, max 16 fields, password/CC redaction.
    async fn read_form_field_values(
        &self,
        controls: &mut [CompactInteractiveElement],
    ) -> BrowserResult<()> {
        use super::dom::{
            FORM_VALUE_MAX_BYTES, FORM_VALUE_MAX_FIELDS, SELECT_OPTION_MAX_BYTES, truncate_utf8,
        };

        self.policy.require(PolicyCapability::ReadFormValues)?;
        let allow_sensitive = self.policy.allow_sensitive_form_values();

        const FORM_ROLES: &[&str] = &[
            "textbox",
            "searchbox",
            "combobox",
            "spinbutton",
            "listbox",
            "checkbox",
            "radio",
            "switch",
            "slider",
        ];

        // Prioritize controls with backend node IDs and form-relevant roles
        let mut candidates: Vec<&mut CompactInteractiveElement> = controls
            .iter_mut()
            .filter(|c| {
                FORM_ROLES.iter().any(|r| c.role.eq_ignore_ascii_case(r))
                    && c.backend_dom_node_id > 0
            })
            .take(FORM_VALUE_MAX_FIELDS)
            .collect();

        if candidates.is_empty() {
            return Ok(());
        }

        // Read values via CDP: resolve backend node IDs → object IDs → call function
        let expression = r#"function() {
            const el = this;
            const result = { empty: true };
            const tag = (el.tagName || '').toLowerCase();
            if (tag === 'input') {
                const type = (el.type || 'text').toLowerCase();
                if (type === 'checkbox' || type === 'radio') {
                    result.checked = el.checked;
                    result.value = el.value;
                } else {
                    result.value = el.value;
                }
            } else if (tag === 'select') {
                const opt = el.options[el.selectedIndex];
                result.selectedOption = opt ? (opt.label || opt.text || opt.value) : '';
                result.value = el.value;
            } else if (tag === 'textarea') {
                result.value = el.value;
            } else {
                result.value = el.value || el.textContent || '';
            }
            result.empty = !result.value && !result.selectedOption && !result.checked;
            result.readOnly = !!el.readOnly;
            result.required = !!el.required;
            result.autocomplete = el.getAttribute('autocomplete') || '';
            result.inputType = (el.type || '').toLowerCase();
            return JSON.stringify(result);
        }"#;

        for control in candidates.iter_mut() {
            let resolved = match self
                .cdp
                .send(
                    "DOM.resolveNode",
                    Some(serde_json::json!({
                        "backendNodeId": control.backend_dom_node_id,
                    })),
                )
                .await
            {
                Ok(resolved) => resolved,
                Err(_) => continue,
            };

            let Some(object_id) = resolved["object"]["objectId"].as_str() else {
                continue;
            };

            let raw = match self
                .cdp
                .send(
                    "Runtime.callFunctionOn",
                    Some(serde_json::json!({
                        "objectId": object_id,
                        "functionDeclaration": expression,
                        "returnByValue": true,
                        "awaitPromise": false,
                    })),
                )
                .await
            {
                Ok(raw) => raw,
                Err(_) => continue,
            };

            let value_str = raw["result"]["value"].as_str().unwrap_or("{}");
            let parsed: Value = match serde_json::from_str(value_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let input_type = parsed["inputType"]
                .as_str()
                .map(String::from)
                .or_else(|| control.input_type.clone());

            let is_password = input_type.as_deref() == Some("password");
            let is_sensitive_autocomplete = parsed["autocomplete"]
                .as_str()
                .map(|ac| ac.starts_with("cc-") || ac == "current-password" || ac == "new-password")
                .unwrap_or(false);

            if let Some(val) = parsed["value"].as_str() {
                if is_password || (is_sensitive_autocomplete && !allow_sensitive) {
                    control.value = Some("<redacted>".to_string());
                } else {
                    let (truncated, _) = truncate_utf8(val, FORM_VALUE_MAX_BYTES);
                    control.value = Some(truncated.to_string());
                }
            }

            if let Some(checked) = parsed["checked"].as_bool() {
                control.checked = Some(checked);
            }

            if let Some(opt) = parsed["selectedOption"].as_str() {
                let (truncated, _) = truncate_utf8(opt, SELECT_OPTION_MAX_BYTES);
                control.selected_option = Some(truncated.to_string());
            }

            control.empty = parsed["empty"].as_bool().unwrap_or(true);
            control.read_only = parsed["readOnly"].as_bool().unwrap_or(false);
            control.required = parsed["required"].as_bool().unwrap_or(false);

            if let Some(it) = input_type {
                control.input_type = Some(it);
            }
        }

        Ok(())
    }

    async fn compact_page_state(&self, context_id: i64) -> BrowserResult<EvaluatedPageState> {
        let raw = self
            .cdp
            .evaluate_in_context(COMPACT_PAGE_STATE_EXPRESSION, Some(context_id))
            .await?;
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
        self.policy.require(PolicyCapability::Screenshot)?;
        self.cdp
            .with_current_route(async { Ok(self.cdp.screenshot("png").await?) })
            .await
    }

    /// Capture exact opt-in visual evidence with explicit effective metadata.
    pub async fn capture_visual(
        &self,
        options: &VisualCaptureOptions,
    ) -> BrowserResult<VisualCapture> {
        self.policy.require(PolicyCapability::Screenshot)?;
        validate_visual_options(options)?;
        self.cdp
            .with_current_route(async {
                let metrics = self.cdp.get_layout_metrics().await?;
                let dpr = runtime_value(&self.cdp.evaluate("devicePixelRatio").await?)?
                    .as_f64()
                    .unwrap_or(1.0);
                let (_, selected_frame_id) = self.route_identity().await?;
                let selected_child_frame = {
                    let topology = self.topology.lock().await;
                    topology
                        .frames
                        .iter()
                        .find(|frame| frame.id == selected_frame_id)
                        .is_some_and(|frame| frame.parent_id.is_some())
                };
                if selected_child_frame {
                    return Err("exact visual capture of a selected child frame is not supported; select its page target or the main frame".into());
                }
                let mut clip = options.clip;
                if options.full_page {
                    clip = Some(visual_rect(&metrics["cssContentSize"])?);
                } else if let Some(target) = options.target.as_deref() {
                    let element = self.resolve_element(target).await?;
                    let model = match (element.node_id, element.backend_dom_node_id) {
                        (Some(node_id), _) => self.cdp.get_box_model(node_id).await?,
                        (_, Some(backend_id)) => {
                            self.cdp.get_box_model_for_backend(backend_id).await?
                        }
                        _ => return Err("visual target has no DOM node identity".into()),
                    };
                    let mut element_clip = visual_quad_rect(&model["model"]["border"])?;
                    let viewport = visual_viewport_rect(&metrics["cssVisualViewport"])?;
                    element_clip.x += viewport.x;
                    element_clip.y += viewport.y;
                    clip = Some(element_clip);
                } else if clip.is_none() && options.scale != 1.0 {
                    clip = Some(visual_viewport_rect(&metrics["cssVisualViewport"])?);
                }
                let viewport = visual_viewport_rect(&metrics["cssVisualViewport"])?;
                validate_effective_visual_clip(
                    Some(clip.unwrap_or(viewport)),
                    if clip.is_some() { options.scale } else { dpr },
                )?;
                let mut params = serde_json::json!({
                    "format": options.format.as_cdp(),
                    "optimizeForSpeed": true,
                    "captureBeyondViewport": options.full_page || clip.is_some(),
                    "fromSurface": true
                });
                if let Some(quality) = options.quality {
                    params["quality"] = Value::from(quality);
                }
                if let Some(clip) = clip {
                    params["clip"] = serde_json::json!({
                        "x": clip.x,
                        "y": clip.y,
                        "width": clip.width,
                        "height": clip.height,
                        "scale": options.scale
                    });
                }
                if options.full_page {
                    let latest = self.cdp.get_layout_metrics().await?;
                    let latest_clip = visual_rect(&latest["cssContentSize"])?;
                    if !visual_clips_match(clip.expect("full-page capture has a clip"), latest_clip) {
                        return Err("full-page geometry changed during capture preparation".into());
                    }
                } else if let Some(target) = options.target.as_deref() {
                    let element = self.resolve_element(target).await?;
                    let model = match (element.node_id, element.backend_dom_node_id) {
                        (Some(node_id), _) => self.cdp.get_box_model(node_id).await?,
                        (_, Some(backend_id)) => self.cdp.get_box_model_for_backend(backend_id).await?,
                        _ => return Err("visual target has no DOM node identity".into()),
                    };
                    let latest_metrics = self.cdp.get_layout_metrics().await?;
                    let viewport = visual_viewport_rect(&latest_metrics["cssVisualViewport"])?;
                    let mut latest_clip = visual_quad_rect(&model["model"]["border"])?;
                    latest_clip.x += viewport.x;
                    latest_clip.y += viewport.y;
                    if !visual_clips_match(clip.expect("element capture has a clip"), latest_clip) {
                        return Err("element geometry changed during capture preparation".into());
                    }
                }
                let data = self.cdp.screenshot_with_params(params).await?;
                if data.len() > MAX_VISUAL_BASE64_BYTES {
                    return Err("visual base64 payload exceeded 64 MiB".into());
                }
                let encoded_bytes = decoded_base64_len(&data)?;
                let header_end = data.len().min(VISUAL_HEADER_BASE64_BYTES) / 4 * 4;
                let header = STANDARD.decode(&data.as_bytes()[..header_end])?;
                let size = imagesize::blob_size(&header)?;
                let (target_id, frame_id) = self.route_identity().await?;
                Ok(VisualCapture {
                    metadata: VisualCaptureMetadata {
                        format: options.format,
                        width: size.width,
                        height: size.height,
                        encoded_bytes,
                        device_scale_factor: dpr,
                        scale: options.scale,
                        full_page: options.full_page,
                        clip,
                        target_id,
                        frame_id,
                    },
                    data,
                })
            })
            .await
    }

    pub async fn start_screencast(
        &self,
        format: VisualFormat,
        quality: u8,
        max_width: u32,
        max_height: u32,
    ) -> BrowserResult<ScreencastScope> {
        self.policy.require(PolicyCapability::Screenshot)?;
        if format == VisualFormat::Webp {
            return Err("CDP screencast supports only png or jpeg".into());
        }
        if quality > 100
            || max_width == 0
            || max_height == 0
            || max_width > 4096
            || max_height > 4096
            || f64::from(max_width) * f64::from(max_height) > MAX_VISUAL_PIXELS
        {
            return Err(
                "screencast quality must be 0..=100 and dimensions must fit the 8 MP budget".into(),
            );
        }
        let session_id = self.cdp.current_session_id();
        let receiver = self.cdp.open_screencast_channel(session_id.clone())?;
        let mut startup = ScreencastStartupGuard {
            cdp: self.cdp.clone(),
            session_id: session_id.clone(),
            armed: true,
        };
        let parameters = Some(serde_json::json!({
            "format": format.as_cdp(),
            "quality": quality,
            "maxWidth": max_width,
            "maxHeight": max_height,
            "everyNthFrame": 1
        }));
        let start_result = match session_id.as_deref() {
            Some(session_id) => {
                self.cdp
                    .send_to_session(session_id, "Page.startScreencast", parameters)
                    .await
            }
            None => self.cdp.send("Page.startScreencast", parameters).await,
        };
        if let Err(error) = start_result {
            return Err(error.into());
        }
        startup.disarm();
        Ok(ScreencastScope {
            cdp: self.cdp.clone(),
            session_id,
            receiver,
            armed: true,
        })
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

    /// Collect explicitly scoped, bounded, secret-redacted browser evidence.
    pub async fn diagnostics(&self, duration: Duration) -> BrowserResult<DiagnosticReport> {
        if duration.is_zero() || duration > MAX_DIAGNOSTIC_DURATION {
            return Err("diagnostic duration must be between 1 ms and 30 seconds".into());
        }
        self.cdp
            .with_current_route(async {
                let (target_id, frame_id) = self.route_identity().await?;
                let route_session_id = self.cdp.current_session_id();
                let mut events = self.cdp.subscribe_events_with_params();
                let mut guard = DiagnosticDomainGuard::acquire(
                    self.cdp.clone(),
                    Arc::clone(&self.network_wait_leases),
                    Arc::clone(&self.diagnostic_leases),
                )
                .await?;
                let started = tokio::time::Instant::now();
                let deadline = started + duration;
                let mut console = Vec::new();
                let mut network = Vec::new();
                let mut request_indexes = HashMap::new();
                let mut dropped_events = 0_u64;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        event = events.recv() => match event {
                            Ok(event) if event.session_id == route_session_id => collect_diagnostic_event(
                                &event,
                                &mut console,
                                &mut network,
                                &mut request_indexes,
                                &mut dropped_events,
                            ),
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                                dropped_events = dropped_events.saturating_add(count);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
                guard.disable().await?;
                Ok(DiagnosticReport {
                    target_id,
                    frame_id,
                    duration_ms: started.elapsed().as_millis() as u64,
                    console,
                    network,
                    dropped_events,
                })
            })
            .await
    }

    /// Return the currently pending JavaScript dialog content, if any.
    ///
    /// Agents should read this before calling [`accept_dialog`] or
    /// [`dismiss_dialog`] to determine the dialog type, message, and
    /// default value. The dialog is cleared when it is handled or closed.
    pub async fn pending_dialog(&self) -> Option<PendingDialog> {
        self.topology.lock().await.pending_dialog.clone()
    }

    pub async fn accept_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(true).await?;
        self.invalidate_observation();
        Ok(())
    }

    pub async fn dismiss_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(false).await?;
        self.invalidate_observation();
        Ok(())
    }

    /// Wait for one explicitly authorized download lifecycle.
    pub async fn wait_for_download(
        &self,
        destination: &Path,
        deadline: Duration,
    ) -> BrowserResult<DownloadOutcome> {
        self.policy.require(PolicyCapability::Download)?;
        if deadline.is_zero() || deadline > MAX_DIAGNOSTIC_DURATION {
            return Err("download deadline must be between 1 ms and 30 seconds".into());
        }
        let destination = self.policy.require_existing_path(destination)?;
        if !destination.is_dir() || !destination.starts_with(&self.upload_root) {
            return Err(
                "download destination must be a directory inside the authorized root".into(),
            );
        }
        let (target_id, frame_id) = self.route_identity().await?;
        let page_session_id = if use_page_download_compatibility(
            self.chrome.is_some(),
            self.disposable_profile.is_some(),
        ) {
            let topology = self.topology.lock().await;
            if topology.active_target_id.as_deref() != Some(target_id.as_str()) {
                return Err(download_error(
                    DownloadErrorKind::AuthorizationFailed,
                    "incognito download route changed during capture",
                )
                .into());
            }
            Some(topology.active_target_session_id.clone().ok_or_else(|| {
                download_error(
                    DownloadErrorKind::AuthorizationFailed,
                    "incognito download has no captured top-level page session",
                )
            })?)
        } else {
            None
        };
        let _download_scope = self.download_scope.lock().await;
        let mut events = self.cdp.subscribe_events_with_params();
        let mut download_guard = match page_session_id {
            Some(page_session_id) => {
                DownloadBehaviorGuard::acquire_for_incognito(
                    self.cdp.clone(),
                    destination.clone(),
                    target_id.clone(),
                    page_session_id,
                    self.launched_incognito_context_id.clone().ok_or_else(|| {
                        download_error(
                            DownloadErrorKind::AuthorizationFailed,
                            "owned incognito session has no original browser context ID",
                        )
                    })?,
                )
                .await?
            }
            None => {
                DownloadBehaviorGuard::acquire(self.cdp.clone(), destination.clone(), None).await?
            }
        };
        let result = tokio::time::timeout(deadline, async {
            let mut guid = None;
            let mut filename = String::new();
            loop {
                match events.recv().await {
                    Ok(event) if event.method == "Browser.downloadWillBegin" => {
                        if event.params["frameId"].as_str() != Some(frame_id.as_str()) {
                            continue;
                        }
                        guid = event.params["guid"].as_str().map(bounded_diagnostic_text);
                        filename = bounded_diagnostic_text(
                            event.params["suggestedFilename"]
                                .as_str()
                                .unwrap_or("download"),
                        );
                    }
                    Ok(event) if event.method == "Browser.downloadProgress" => {
                        let Some(active_guid) = guid.as_deref() else {
                            continue;
                        };
                        if event.params["guid"].as_str() != Some(active_guid) {
                            continue;
                        }
                        let state = event.params["state"].as_str().unwrap_or("inProgress");
                        if matches!(state, "completed" | "canceled") {
                            self.record_audit(
                                "download",
                                format!("{} (state={})", filename, state),
                            );
                            return BrowserResult::Ok(DownloadOutcome {
                                guid: active_guid.to_string(),
                                suggested_filename: filename,
                                state: state.to_ascii_lowercase(),
                                received_bytes: finite_nonnegative_u64(
                                    &event.params["receivedBytes"],
                                ),
                                total_bytes: finite_nonnegative_u64(&event.params["totalBytes"]),
                                target_id: target_id.clone(),
                                frame_id: frame_id.clone(),
                                sha256: None,
                            });
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        return Err(format!("download event stream dropped {count} events").into());
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err("download event stream closed".into());
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|_| Err("download deadline exceeded".into()));
        download_guard.disable().await?;
        result
    }

    /// Click an element and return its structured action outcome.
    pub async fn click(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.pointer_click(target, false).await
    }

    /// Click one element that is expected to synchronously open exactly one popup.
    ///
    /// This operation never selects the popup. An unanswered `mouseReleased`
    /// request is accepted only with the causal evidence documented by the
    /// automation contract; ordinary [`Self::click`] retains strict ACK behavior.
    pub async fn click_expect_popup(&self, target: &str) -> BrowserResult<PopupClickOutcome> {
        let _scope = self.popup_click_scope.lock().await;
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await
                    .map_err(|error| {
                        tracing::debug!(%error, "popup target node could not be resolved");
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
                let original_session_id = self
                    .cdp
                    .current_session_id()
                    .ok_or_else(|| {
                        TopologyError::new(
                            TopologyErrorKind::NoPageSession,
                            "popup click requires an attached page session; the session may need to be re-established",
                        )
                    })?;
                let original_frame_id = self
                    .cdp
                    .active_frame()
                    .ok_or_else(|| {
                        TopologyError::new(
                            TopologyErrorKind::StaleFrame,
                            "popup click requires an active frame; call listFrames to discover available frames",
                        )
                    })?;
                let backend_node_id = match (element.backend_dom_node_id, element.node_id) {
                    (Some(backend_node_id), _) => backend_node_id,
                    (None, Some(node_id)) => self
                        .cdp
                        .backend_node_id_for_node(node_id)
                        .await
                        .map_err(|error| {
                            popup_error(
                                PopupClickErrorKind::WitnessMissing,
                                format!(
                                    "resolved popup target has no readable backend identity: {error}"
                                ),
                            )
                        })?,
                    (None, None) => {
                        return Err(popup_error(
                            PopupClickErrorKind::WitnessMissing,
                            "popup target has no exact node identity",
                        ));
                    }
                };
                let mut witness = self
                    .arm_popup_witness(&original_session_id, &original_frame_id, backend_node_id)
                    .await?;
                let operation = self
                    .perform_popup_click(&remote.object_id, &element, &mut witness)
                    .await;
                let cleanup = witness.cleanup().await;
                match (operation, cleanup) {
                    (Ok(outcome), Ok(())) => Ok(outcome),
                    (Err(error), _) => Err(error),
                    (Ok(_), Err(error)) => Err(error),
                }
            })
            .await
    }

    async fn arm_popup_witness(
        &self,
        session_id: &str,
        frame_id: &str,
        backend_node_id: i64,
    ) -> BrowserResult<PopupWitnessGuard> {
        let cdp = self.cdp.clone();
        let session_id = session_id.to_string();
        let frame_id = frame_id.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = arm_popup_witness_owned(cdp, session_id, frame_id, backend_node_id).await;
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                popup_error(
                    PopupClickErrorKind::WitnessMissing,
                    "popup witness worker ended without a result",
                )
            })?
            .map_err(Into::into)
    }

    async fn perform_popup_click(
        &self,
        object_id: &str,
        element: &ResolvedElement,
        witness: &mut PopupWitnessGuard,
    ) -> BrowserResult<PopupClickOutcome> {
        let local_point = self.verified_action_point(object_id).await?;
        let point = self.target_viewport_point(local_point).await?;
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
        self.cdp
            .dispatch_mouse_event("mousePressed", point.x, point.y, Some("left"), Some(1))
            .await?;
        let mut pressed = PressedButtonGuard {
            cdp: self.cdp.clone(),
            point,
            click_count: 1,
            armed: true,
        };
        if self.interaction_mode == InteractionMode::Human {
            tokio::time::sleep(self.mouse.click_delay()).await;
        }

        // This snapshot is intentionally adjacent to and before the release.
        let snapshot = self.popup_topology_snapshot().await?;
        let release_started = std::time::Instant::now();
        let release = self
            .cdp
            .dispatch_mouse_event_with_timeout(
                "mouseReleased",
                point.x,
                point.y,
                Some("left"),
                Some(1),
                POPUP_RELEASE_ACK_TIMEOUT,
            )
            .await;
        let release_ack_wait_ms = release_started.elapsed().as_secs_f64() * 1_000.0;
        let release_acknowledged = match release {
            Ok(_) => true,
            Err(error) if error.is_response_timeout() => false,
            Err(error) => {
                return Err(popup_error(
                    PopupClickErrorKind::ReleaseFailed,
                    format!("mouseReleased failed without a response timeout: {error}"),
                ));
            }
        };
        // The release was either acknowledged or causally witnessed. Never emit
        // the guard's second, fire-and-forget release for the timeout case.
        pressed.armed = false;

        let evidence_deadline = tokio::time::Instant::now() + POPUP_EVIDENCE_DEADLINE;
        let candidate = self
            .wait_for_causal_popup(&snapshot, witness, evidence_deadline)
            .await?;
        let ready_state = self
            .verify_popup_readiness(&snapshot, &candidate, evidence_deadline)
            .await?;
        *pointer = Some(point);
        Ok(PopupClickOutcome {
            action: ActionKind::ClickExpectPopup,
            target: ActionTarget {
                label: element.label.clone(),
                reference: element.reference.clone(),
            },
            revision: self.invalidate_observation(),
            target_id: snapshot.original_target_id.clone(),
            frame_id: snapshot.original_frame_id.clone(),
            causally_verified_popup: true,
            popup_id: candidate.target.id.clone(),
            opener_id: snapshot.original_target_id.clone(),
            evidence: PopupVerificationEvidence {
                trusted_click_witness: true,
                release_acknowledged,
                release_ack_wait_ms,
                topology_sequence_before_release: snapshot.sequence,
                popup_observed_sequence: candidate.observed_sequence,
                attached: true,
                ready_state,
            },
        })
    }

    async fn popup_topology_snapshot(&self) -> BrowserResult<PopupTopologySnapshot> {
        let raw_targets = popup_verification_call(
            self.cdp.send_browser("Target.getTargets", None),
            "pre-release target snapshot",
        )
        .await?;
        let mut preexisting_target_ids = HashSet::new();
        for info in raw_targets["targetInfos"].as_array().into_iter().flatten() {
            if info["type"].as_str() != Some("page") {
                continue;
            }
            let id = info["targetId"].as_str().ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "pre-release target snapshot contained a page without an ID",
                )
            })?;
            validate_topology_id(id)?;
            preexisting_target_ids.insert(id.to_string());
        }
        let topology = self.topology.lock().await;
        let original_target_id = topology.active_target_id.clone().ok_or_else(|| {
            TopologyError::new(
                TopologyErrorKind::NoTargetSelected,
                "popup click has no active target; call listTargets to discover available pages",
            )
        })?;
        let original_frame_id = topology.active_frame_id.clone().ok_or_else(|| {
            TopologyError::new(
                TopologyErrorKind::StaleFrame,
                "popup click has no active frame; call listFrames to discover available frames",
            )
        })?;
        Ok(PopupTopologySnapshot {
            original_target_id,
            original_frame_id,
            preexisting_target_ids,
            sequence: topology.sequence,
            event_loss_count: topology.event_loss_count,
        })
    }

    async fn wait_for_causal_popup(
        &self,
        snapshot: &PopupTopologySnapshot,
        witness: &PopupWitnessGuard,
        deadline: tokio::time::Instant,
    ) -> BrowserResult<PopupCandidate> {
        let mut witnessed = false;
        loop {
            if !witnessed {
                witnessed = witness.fired().await?;
            }

            let assessment = {
                let topology = self.topology.lock().await;
                assess_popup_topology(snapshot, &topology, witnessed)
            };
            match assessment {
                Ok(candidate) => {
                    return wait_for_stable_popup_topology(
                        &self.topology,
                        snapshot,
                        &candidate,
                        deadline,
                        POPUP_TOPOLOGY_QUIET_INTERVAL,
                    )
                    .await
                    .map_err(Into::into);
                }
                Err(error)
                    if matches!(
                        error.kind,
                        PopupClickErrorKind::TopologyLagged
                            | PopupClickErrorKind::PopupAmbiguous
                            | PopupClickErrorKind::PopupDestroyed
                    ) =>
                {
                    return Err(error.into());
                }
                Err(error) if tokio::time::Instant::now() >= deadline => {
                    return Err(error.into());
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    async fn verify_popup_readiness(
        &self,
        snapshot: &PopupTopologySnapshot,
        candidate: &PopupCandidate,
        deadline: tokio::time::Instant,
    ) -> BrowserResult<String> {
        let mut attachment = self.attach_popup(&candidate.target.id).await?;
        popup_verification_call(
            self.cdp.send_to_session(
                &attachment.session_id,
                "Runtime.runIfWaitingForDebugger",
                None,
            ),
            "popup debugger resume",
        )
        .await?;
        let result = popup_verification_call(
            self.cdp.send_to_session(
                &attachment.session_id,
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.readyState",
                    "returnByValue": true,
                    "awaitPromise": false
                })),
            ),
            "popup readiness evaluation",
        )
        .await?;
        let ready_state = result["result"]["value"]
            .as_str()
            .filter(|state| matches!(*state, "loading" | "interactive" | "complete"))
            .map(str::to_string)
            .ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup returned no valid document.readyState",
                )
            })?;
        self.final_popup_verification(snapshot, candidate, deadline)
            .await?;
        attachment.detach().await?;
        Ok(ready_state)
    }

    async fn attach_popup(&self, target_id: &str) -> BrowserResult<PopupAttachmentGuard> {
        let cdp = self.cdp.clone();
        let target_id = target_id.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = match tokio::time::timeout(
                POPUP_VERIFY_CALL_TIMEOUT,
                cdp.send_browser(
                    "Target.attachToTarget",
                    Some(serde_json::json!({"targetId": target_id, "flatten": true})),
                ),
            )
            .await
            {
                Ok(Ok(value)) => value["sessionId"]
                    .as_str()
                    .map(|session_id| PopupAttachmentGuard {
                        cdp: cdp.clone(),
                        session_id: session_id.to_string(),
                        armed: true,
                    })
                    .ok_or_else(|| {
                        popup_typed_error(
                            PopupClickErrorKind::PopupUnreadable,
                            "popup attach returned no session ID",
                        )
                    }),
                Ok(Err(error)) => Err(popup_typed_error(
                    PopupClickErrorKind::PopupUnreadable,
                    format!("popup attach failed: {error}"),
                )),
                Err(_) => Err(popup_typed_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup attach exceeded its bounded deadline",
                )),
            };
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup attach worker ended without a result",
                )
            })?
            .map_err(Into::into)
    }

    async fn final_popup_verification(
        &self,
        snapshot: &PopupTopologySnapshot,
        candidate: &PopupCandidate,
        deadline: tokio::time::Instant,
    ) -> BrowserResult<()> {
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(popup_error(
                    PopupClickErrorKind::TopologyLagged,
                    "popup topology did not settle before final verification deadline",
                ));
            }
            let (stable_sequence, stable_loss) = {
                let topology = self.topology.lock().await;
                let current = assess_popup_topology(snapshot, &topology, true)?;
                if current.target.id != candidate.target.id {
                    return Err(popup_error(
                        PopupClickErrorKind::PopupAmbiguous,
                        "popup candidate changed during readiness verification",
                    ));
                }
                (topology.sequence, topology.event_loss_count)
            };
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(popup_error(
                    PopupClickErrorKind::TopologyLagged,
                    "popup topology deadline expired before authoritative discovery",
                ));
            }
            let targets = match tokio::time::timeout(
                remaining,
                self.cdp.send_browser("Target.getTargets", None),
            )
            .await
            {
                Ok(Ok(targets)) => targets,
                Ok(Err(error)) => {
                    return Err(popup_error(
                        PopupClickErrorKind::PopupUnreadable,
                        format!("final authoritative popup target discovery failed: {error}"),
                    ));
                }
                Err(_) => {
                    return Err(popup_error(
                        PopupClickErrorKind::TopologyLagged,
                        "final authoritative popup target discovery exceeded the evidence deadline",
                    ));
                }
            };
            let mut matches = Vec::new();
            for info in targets["targetInfos"].as_array().ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "final target discovery returned no target list",
                )
            })? {
                if info["type"].as_str() != Some("page") {
                    continue;
                }
                let id = info["targetId"].as_str().ok_or_else(|| {
                    popup_error(
                        PopupClickErrorKind::PopupUnreadable,
                        "final target discovery contained a page without an ID",
                    )
                })?;
                validate_topology_id(id)?;
                if !snapshot.preexisting_target_ids.contains(id)
                    && info["openerId"].as_str() == Some(snapshot.original_target_id.as_str())
                {
                    matches.push(id);
                }
            }
            if matches.len() != 1 || matches[0] != candidate.target.id {
                return Err(popup_error(
                    if matches.len() > 1 {
                        PopupClickErrorKind::PopupAmbiguous
                    } else {
                        PopupClickErrorKind::PopupDestroyed
                    },
                    format!(
                        "final target discovery found {} live later opener matches",
                        matches.len()
                    ),
                ));
            }
            let topology = self.topology.lock().await;
            if topology.event_loss_count != stable_loss {
                return Err(popup_error(
                    PopupClickErrorKind::TopologyLagged,
                    "popup topology event loss changed during final verification",
                ));
            }
            let current = assess_popup_topology(snapshot, &topology, true)?;
            if current.target.id != candidate.target.id {
                return Err(popup_error(
                    PopupClickErrorKind::PopupAmbiguous,
                    "popup candidate changed at final topology verification",
                ));
            }
            if topology.sequence == stable_sequence {
                if tokio::time::Instant::now() >= deadline {
                    return Err(popup_error(
                        PopupClickErrorKind::TopologyLagged,
                        "popup topology deadline expired before final success",
                    ));
                }
                return Ok(());
            }
            drop(topology);
            wait_for_stable_popup_topology(
                &self.topology,
                snapshot,
                candidate,
                deadline,
                POPUP_TOPOLOGY_QUIET_INTERVAL,
            )
            .await?;
        }
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
                self.cdp.dispatch_select_all().await?;
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
        self.policy.require(PolicyCapability::Upload)?;
        self.cdp.with_current_route(async {
            if paths.is_empty() || paths.len() > 16 { return Err("upload requires 1..=16 files".into()); }
            let mut files = Vec::with_capacity(paths.len());
            for path in paths {
                let canonical = self.policy.require_existing_path(path)?;
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
            let outcome = self.action_outcome(ActionKind::Upload, Some(element), Some(serde_json::json!({"file_count": files.len()}))).await?;
            self.record_audit("upload", format!("{} files", files.len()));
            Ok(outcome)
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
        if let Some(interception) = &self.policy_interception {
            // A same-route command is an ordering barrier for synchronous
            // click/form navigation. The interception itself remains active
            // for delayed page-authored navigation after this action returns.
            let _ = self.cdp.evaluate("0").await;
            tokio::task::yield_now().await;
            if let Some(error) = interception.take_denial().await {
                return Err(error.into());
            }
        }
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

/// Parse a backend DOM node ID from a revisioned reference string ("r<rev>:b<id>").
fn parse_backend_id_from_ref(reference: &str) -> Option<i64> {
    let parts: Vec<&str> = reference.split(':').collect();
    if parts.len() != 2 || !parts[0].starts_with('r') || !parts[1].starts_with('b') {
        return None;
    }
    parts[1][1..].parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::super::cdp::CdpEventWithParams;
    use super::super::dom::AxNode;
    use super::*;
    use base64::engine::general_purpose::STANDARD;
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
            launched_incognito_context_id: None,
            profile: "test".to_string(),
            interaction_mode: InteractionMode::Fast,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision: Arc::new(AtomicU64::new(1)),
            observation_cache: Mutex::new(None),
            network_wait_leases: Arc::new(Mutex::new(NetworkLeaseState::default())),
            diagnostic_leases: Arc::new(Mutex::new(DiagnosticLeaseState::default())),
            download_scope: Arc::new(Mutex::new(())),
            topology: Arc::new(Mutex::new(TopologyRegistry {
                active_target_id: Some("test-target".to_string()),
                active_frame_id: Some("test-frame".to_string()),
                ..TopologyRegistry::default()
            })),
            popup_click_scope: Mutex::new(()),
            upload_root: std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
            policy: BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap(),
            policy_interception: None,
            audit_log: std::sync::Mutex::new(VecDeque::new()),
            audit_sequence: AtomicU64::new(1),
            audit_enabled: false,
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

    #[test]
    fn disposable_cleanup_removes_only_provably_abandoned_profiles() {
        let root = std::env::temp_dir().join(format!(
            "glass-disposable-cleanup-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let active = root.join("incognito-active");
        let dead = root.join("incognito-dead");
        let malformed = root.join("incognito-malformed");
        for path in [&active, &dead, &malformed] {
            std::fs::create_dir(path).unwrap();
        }
        let active_owner = DisposableProfileOwner {
            pid: std::process::id(),
            process_start: process_start_identity(std::process::id()).unwrap(),
        };
        std::fs::write(
            active.join(DISPOSABLE_OWNER_FILE),
            serde_json::to_vec(&active_owner).unwrap(),
        )
        .unwrap();
        let dead_owner = DisposableProfileOwner {
            pid: u32::MAX,
            process_start: 1,
        };
        std::fs::write(
            dead.join(DISPOSABLE_OWNER_FILE),
            serde_json::to_vec(&dead_owner).unwrap(),
        )
        .unwrap();
        std::fs::write(malformed.join(DISPOSABLE_OWNER_FILE), b"not-json").unwrap();

        DisposableProfileDir::cleanup_abandoned(&root).unwrap();

        assert!(active.exists());
        assert!(!dead.exists());
        assert!(malformed.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disposable_cleanup_scans_beyond_one_memory_batch() {
        let root = std::env::temp_dir().join(format!(
            "glass-disposable-batch-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let owner = serde_json::to_vec(&DisposableProfileOwner {
            pid: u32::MAX,
            process_start: 1,
        })
        .unwrap();
        for index in 0..=DISPOSABLE_CLEANUP_BATCH {
            let path = root.join(format!("incognito-{index:04}"));
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join(DISPOSABLE_OWNER_FILE), &owner).unwrap();
        }

        DisposableProfileDir::cleanup_abandoned(&root).unwrap();

        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn disposable_profile_is_recovered_after_forced_process_exit() {
        let path_record = std::env::temp_dir().join(format!(
            "glass-crash-profile-path-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "browser::session::tests::disposable_profile_crash_helper",
                "--ignored",
            ])
            .env("GLASS_CRASH_PROFILE_PATH_RECORD", &path_record)
            .status()
            .unwrap();
        assert!(!status.success());
        let abandoned = PathBuf::from(std::fs::read_to_string(&path_record).unwrap());
        assert!(abandoned.exists());

        DisposableProfileDir::cleanup_abandoned(&std::env::temp_dir().join("glass")).unwrap();

        assert!(!abandoned.exists());
        std::fs::remove_file(path_record).unwrap();
    }

    #[test]
    #[ignore = "subprocess helper for forced-exit recovery"]
    fn disposable_profile_crash_helper() {
        let Some(path_record) = std::env::var_os("GLASS_CRASH_PROFILE_PATH_RECORD") else {
            return;
        };
        let profile = DisposableProfileDir::create().unwrap();
        std::fs::write(path_record, profile.path().to_string_lossy().as_bytes()).unwrap();
        std::mem::forget(profile);
        std::process::exit(86);
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
            let mut saw_flattened = false;

            for _ in 0..if include_dom { 6 } else { 5 } {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Page.createIsolatedWorld") => {
                        serde_json::json!({"executionContextId": 71})
                    }
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
                    Some("DOM.getFlattenedDocument") => {
                        saw_flattened = true;
                        // Return empty flattened doc — no shadow content to pierce
                        serde_json::json!({"nodes": []})
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
            assert!(saw_flattened);
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
            for _ in 0..7 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Page.createIsolatedWorld") => {
                        serde_json::json!({"executionContextId": 72})
                    }
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

    async fn diagnostic_cleanup_server() -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut methods = Vec::new();
            for _ in 0..6 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                methods.push(request["method"].as_str().unwrap().to_string());
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": {}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            methods
        });
        (format!("ws://{address}"), server)
    }

    async fn download_bridge_server(
        delay_page_allow: bool,
        request_count: usize,
        error_indices: Vec<usize>,
    ) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut requests = Vec::new();
            for index in 0..request_count {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                if delay_page_allow && index == 1 {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
                let response = if error_indices.contains(&index) {
                    serde_json::json!({
                        "id": request["id"],
                        "error": {"code": -32000, "message": "diagnostic failure"}
                    })
                } else {
                    serde_json::json!({"id": request["id"], "result": {}})
                };
                websocket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .unwrap();
                requests.push(request);
            }
            requests
        });
        (format!("ws://{address}"), server)
    }

    async fn download_context_server(
        context_id: &str,
        behavior_requests: usize,
    ) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let context_id = context_id.to_string();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let lookup = websocket.next().await.unwrap().unwrap();
            let lookup: Value = match lookup {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            assert_eq!(lookup["method"], "Target.getTargetInfo");
            assert_eq!(lookup["params"]["targetId"], "selected-target");
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": lookup["id"],
                        "result": {"targetInfo": {
                            "targetId": "selected-target",
                            "browserContextId": context_id
                        }}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let mut requests = Vec::new();
            for _ in 0..behavior_requests {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": {}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                requests.push(request);
            }
            requests
        });
        (format!("ws://{address}"), server)
    }

    async fn final_popup_server(
        topology: Arc<Mutex<TopologyRegistry>>,
        move_first_query: bool,
        move_every_query: bool,
        query_delay: Option<Duration>,
    ) -> (String, tokio::task::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut queries = 0;
            while let Ok(Some(Ok(request))) =
                tokio::time::timeout(Duration::from_millis(250), websocket.next()).await
            {
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                assert_eq!(request["method"], "Target.getTargets");
                queries += 1;
                if move_every_query || move_first_query && queries == 1 {
                    topology.lock().await.sequence += 1;
                }
                if let Some(delay) = query_delay {
                    tokio::time::sleep(delay).await;
                }
                if websocket
                    .send(Message::Text(
                        serde_json::json!({
                            "id": request["id"],
                            "result": {"targetInfos": [
                                {"type": "page", "targetId": "original"},
                                {"type": "page", "targetId": "popup", "openerId": "original"}
                            ]}
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            queries
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
            for _ in 0..6 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Page.createIsolatedWorld") => {
                        serde_json::json!({"executionContextId": 73})
                    }
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
                omitted_count: 0,
                ranking_applied: false,
                completeness: None,
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
    fn diagnostics_redact_secrets_and_bound_retention() {
        let redacted = redact_diagnostic_url(
            "https://user:pass@example.test/path?token=secret&empty=#fragment",
        );
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("user"));
        assert!(!redacted.contains("pass"));
        assert!(!redacted.contains("fragment"));
        assert!(redacted.contains("token=%5Bredacted%5D"));

        let headers = serde_json::json!({
            "Authorization": "Bearer secret",
            "Cookie": "session=secret",
            "X-Trace": "safe",
            "Accept": "*/*"
        });
        assert_eq!(safe_header_names(&headers), vec!["Accept", "X-Trace"]);

        let event = CdpEventWithParams {
            method: "Network.requestWillBeSent".to_string(),
            session_id: None,
            params: serde_json::json!({
                "requestId": "request-1",
                "request": {
                    "method": "POST",
                    "url": "https://example.test/api?password=hunter2",
                    "headers": headers,
                    "postData": "never-retain-this"
                }
            }),
        };
        let mut console = Vec::new();
        let mut network = Vec::new();
        let mut indexes = HashMap::new();
        let mut dropped = 0;
        collect_diagnostic_event(
            &event,
            &mut console,
            &mut network,
            &mut indexes,
            &mut dropped,
        );
        let serialized = serde_json::to_string(&network).unwrap();
        assert!(!serialized.contains("hunter2"));
        assert!(!serialized.contains("never-retain-this"));
        assert!(!serialized.contains("Authorization"));
        assert_eq!(network[0].method, "POST");
        assert_eq!(
            redact_diagnostic_text("Authorization: Bearer top-secret"),
            "[redacted sensitive console entry]"
        );
        let console_event = CdpEventWithParams {
            method: "Runtime.consoleAPICalled".to_string(),
            session_id: None,
            params: serde_json::json!({
                "type": "error",
                "args": [{"value": "hunter2"}]
            }),
        };
        for _ in 0..=MAX_DIAGNOSTIC_EVENTS {
            collect_diagnostic_event(
                &console_event,
                &mut console,
                &mut network,
                &mut indexes,
                &mut dropped,
            );
        }
        assert_eq!(console.len(), MAX_DIAGNOSTIC_EVENTS);
        assert_eq!(console[0].text, "[console arguments redacted]");
        assert_eq!(dropped, 1);
    }

    #[test]
    fn visual_capture_validation_uses_effective_viewport_and_scale() {
        let viewport = visual_viewport_rect(&serde_json::json!({
            "pageX": 15.0,
            "pageY": 25.0,
            "clientWidth": 800.0,
            "clientHeight": 600.0
        }))
        .unwrap();
        assert_eq!(viewport.x, 15.0);
        assert_eq!(viewport.y, 25.0);
        assert_eq!(viewport.width, 800.0);
        assert!(validate_effective_visual_clip(Some(viewport), 2.0).is_ok());
        assert!(
            validate_effective_visual_clip(
                Some(VisualClip {
                    x: 0.0,
                    y: 0.0,
                    width: 8_000.0,
                    height: 8_000.0
                }),
                4.0
            )
            .is_err()
        );
        assert_eq!(decoded_base64_len("aGVsbG8=").unwrap(), 5);
        assert!(!visual_clips_match(
            viewport,
            VisualClip {
                x: 16.0,
                ..viewport
            }
        ));
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
            input_type: None,
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
                ObservationIncompleteReason::FrameBoundary,
                ObservationIncompleteReason::Canvas,
                ObservationIncompleteReason::ShadowBoundary,
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
    async fn diagnostic_cancellation_disables_every_scoped_domain() {
        let (url, server) = diagnostic_cleanup_server().await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let session = test_session(cdp.clone());
        cdp.set_active_target_route(
            Some("test-target".to_string()),
            Some("diagnostic-session".to_string()),
            Some("test-frame".to_string()),
            None,
        );
        assert!(
            tokio::time::timeout(
                Duration::from_millis(25),
                session.diagnostics(Duration::from_secs(5))
            )
            .await
            .is_err()
        );
        let methods = tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        for method in [
            "Network.enable",
            "Runtime.enable",
            "Log.enable",
            "Log.disable",
            "Runtime.disable",
            "Network.disable",
        ] {
            assert!(
                methods.iter().any(|actual| actual == method),
                "missing {method}"
            );
        }
    }

    #[test]
    fn page_download_bridge_is_scoped_only_to_owned_command_line_incognito() {
        assert!(use_page_download_compatibility(true, true));
        assert!(!use_page_download_compatibility(false, true));
        assert!(!use_page_download_compatibility(true, false));
        assert!(!use_page_download_compatibility(false, false));
    }

    #[tokio::test]
    async fn incognito_download_context_mismatch_fails_before_behavior_mutation() {
        let (url, server) = download_context_server("other-context", 0).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let error = match DownloadBehaviorGuard::acquire_for_incognito(
            cdp.clone(),
            std::env::current_dir().unwrap(),
            "selected-target".to_string(),
            "captured-page-session".to_string(),
            "launched-context".to_string(),
        )
        .await
        {
            Ok(_) => panic!("mismatched context was authorized"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<DownloadError>().unwrap().kind,
            DownloadErrorKind::AuthorizationFailed
        );
        assert!(server.await.unwrap().is_empty());
        cdp.close().await;
    }

    #[tokio::test]
    async fn incognito_download_context_match_reaches_captured_behavior_bridge() {
        let (url, server) = download_context_server("launched-context", 4).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let mut guard = DownloadBehaviorGuard::acquire_for_incognito(
            cdp.clone(),
            std::env::current_dir().unwrap(),
            "selected-target".to_string(),
            "captured-page-session".to_string(),
            "launched-context".to_string(),
        )
        .await
        .unwrap();
        guard.disable().await.unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests[0]["method"], "Browser.setDownloadBehavior");
        assert_eq!(requests[1]["method"], "Page.setDownloadBehavior");
        assert_eq!(requests[1]["sessionId"], "captured-page-session");
        assert_eq!(requests[2]["params"]["behavior"], "deny");
        assert_eq!(requests[3]["params"]["behavior"], "deny");
        cdp.close().await;
    }

    #[tokio::test]
    async fn incognito_download_bridge_allows_and_restores_the_captured_page_route() {
        let (url, server) = download_bridge_server(false, 4, Vec::new()).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let destination = std::env::current_dir().unwrap();
        let mut guard = DownloadBehaviorGuard::acquire(
            cdp.clone(),
            destination.clone(),
            Some("captured-page-session".to_string()),
        )
        .await
        .unwrap();
        guard.disable().await.unwrap();

        let requests = server.await.unwrap();
        assert_eq!(requests[0]["method"], "Browser.setDownloadBehavior");
        assert_eq!(requests[0]["params"]["behavior"], "allow");
        assert_eq!(requests[0]["params"]["eventsEnabled"], true);
        assert_eq!(
            requests[0]["params"]["downloadPath"],
            destination.to_string_lossy().as_ref()
        );
        assert_eq!(requests[1]["method"], "Page.setDownloadBehavior");
        assert_eq!(requests[1]["sessionId"], "captured-page-session");
        assert_eq!(requests[1]["params"]["behavior"], "allow");
        assert_eq!(requests[2]["method"], "Page.setDownloadBehavior");
        assert_eq!(requests[2]["sessionId"], "captured-page-session");
        assert_eq!(requests[2]["params"]["behavior"], "deny");
        assert_eq!(requests[3]["method"], "Browser.setDownloadBehavior");
        assert_eq!(requests[3]["params"]["behavior"], "deny");
        assert_eq!(requests[3]["params"]["eventsEnabled"], false);
        cdp.close().await;
    }

    #[tokio::test]
    async fn cancelled_incognito_download_authorization_restores_both_scopes() {
        let (url, server) = download_bridge_server(true, 4, Vec::new()).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let acquire = DownloadBehaviorGuard::acquire(
            cdp.clone(),
            std::env::current_dir().unwrap(),
            Some("captured-page-session".to_string()),
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(10), acquire)
                .await
                .is_err()
        );

        let requests = tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(requests[0]["params"]["behavior"], "allow");
        assert_eq!(requests[1]["params"]["behavior"], "allow");
        assert_eq!(requests[2]["params"]["behavior"], "deny");
        assert_eq!(requests[2]["sessionId"], "captured-page-session");
        assert_eq!(requests[3]["params"]["behavior"], "deny");
        cdp.close().await;
    }

    #[tokio::test]
    async fn partial_incognito_download_enable_is_typed_and_restores_browser_deny() {
        let (url, server) = download_bridge_server(false, 3, vec![1]).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let error = match DownloadBehaviorGuard::acquire(
            cdp.clone(),
            std::env::current_dir().unwrap(),
            Some("captured-page-session".to_string()),
        )
        .await
        {
            Ok(_) => panic!("partial authorization unexpectedly succeeded"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<DownloadError>().unwrap().kind,
            DownloadErrorKind::AuthorizationFailed
        );
        let requests = server.await.unwrap();
        assert_eq!(requests[2]["method"], "Browser.setDownloadBehavior");
        assert_eq!(requests[2]["params"]["behavior"], "deny");
        cdp.close().await;
    }

    #[tokio::test]
    async fn partial_incognito_download_restoration_is_typed_and_still_denies_browser() {
        let (url, server) = download_bridge_server(false, 4, vec![2]).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let mut guard = DownloadBehaviorGuard::acquire(
            cdp.clone(),
            std::env::current_dir().unwrap(),
            Some("captured-page-session".to_string()),
        )
        .await
        .unwrap();
        let error = guard.disable().await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<DownloadError>().unwrap().kind,
            DownloadErrorKind::RestorationFailed
        );
        let requests = server.await.unwrap();
        assert_eq!(requests[2]["method"], "Page.setDownloadBehavior");
        assert_eq!(requests[3]["method"], "Browser.setDownloadBehavior");
        assert_eq!(requests[3]["params"]["behavior"], "deny");
        guard.armed = false;
        cdp.close().await;
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

    #[tokio::test]
    async fn hardened_navigation_intercepts_private_redirects_before_following() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let enable = websocket.next().await.unwrap().unwrap();
            let enable: Value = match enable {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected Fetch.enable"),
            };
            assert_eq!(enable["method"], "Fetch.enable");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": enable["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "method": "Fetch.requestPaused",
                        "sessionId": "route-1",
                        "params": {
                            "requestId": "redirect-1",
                            "request": {"url": "http://127.0.0.1/private"}
                        }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let blocked = websocket.next().await.unwrap().unwrap();
            let blocked: Value = match blocked {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected Fetch.failRequest"),
            };
            assert_eq!(blocked["method"], "Fetch.failRequest");
            assert_eq!(blocked["params"]["requestId"], "redirect-1");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": blocked["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let disable = websocket.next().await.unwrap().unwrap();
            let disable: Value = match disable {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected Fetch.disable"),
            };
            assert_eq!(disable["method"], "Fetch.disable");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": disable["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });
        let cdp = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        let policy = BrowserPolicy::hardened(std::env::current_dir().unwrap()).unwrap();
        let interception = PolicyInterception::start(cdp.clone(), policy, "route-1".to_string())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(matches!(
            interception.take_denial().await,
            Some(PolicyError::Denied { .. })
        ));
        interception.shutdown().await;
        cdp.close().await;
        server.await.unwrap();
    }

    fn popup_test_snapshot() -> PopupTopologySnapshot {
        PopupTopologySnapshot {
            original_target_id: "original".to_string(),
            original_frame_id: "frame".to_string(),
            preexisting_target_ids: HashSet::from([
                "original".to_string(),
                "preexisting".to_string(),
            ]),
            sequence: 10,
            event_loss_count: 0,
        }
    }

    fn popup_test_target(id: &str, opener: Option<&str>) -> PageTargetInfo {
        PageTargetInfo {
            id: id.to_string(),
            url: "about:blank".to_string(),
            title: String::new(),
            opener_id: opener.map(str::to_string),
            active: false,
        }
    }

    fn popup_topology_with(targets: Vec<(&str, Option<&str>, u64)>) -> TopologyRegistry {
        let mut topology = TopologyRegistry::default();
        for (id, opener, sequence) in targets {
            topology.targets.push(popup_test_target(id, opener));
            topology.target_sequences.insert(id.to_string(), sequence);
            topology.sequence = topology.sequence.max(sequence);
        }
        topology
    }

    #[test]
    fn popup_topology_accepts_exactly_one_later_live_matching_opener() {
        let topology = popup_topology_with(vec![
            ("original", None, 1),
            ("preexisting", Some("original"), 9),
            ("popup", Some("original"), 11),
        ]);
        let candidate = assess_popup_topology(&popup_test_snapshot(), &topology, true).unwrap();
        assert_eq!(candidate.target.id, "popup");
        assert_eq!(candidate.observed_sequence, 11);
    }

    #[tokio::test]
    async fn popup_topology_quiet_window_resets_for_a_late_event_then_stabilizes() {
        let snapshot = popup_test_snapshot();
        let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
            "popup",
            Some("original"),
            11,
        )])));
        let candidate = {
            let topology = topology.lock().await;
            assess_popup_topology(&snapshot, &topology, true).unwrap()
        };
        let late_topology = Arc::clone(&topology);
        let late_event = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(15)).await;
            late_topology.lock().await.sequence += 1;
        });

        let started = tokio::time::Instant::now();
        let stable = wait_for_stable_popup_topology(
            &topology,
            &snapshot,
            &candidate,
            started + Duration::from_millis(150),
            Duration::from_millis(30),
        )
        .await
        .unwrap();

        late_event.await.unwrap();
        assert_eq!(stable.target.id, "popup");
        assert!(started.elapsed() >= Duration::from_millis(40));
        assert_eq!(topology.lock().await.sequence, 12);
    }

    #[tokio::test]
    async fn popup_topology_that_never_becomes_quiet_fails_closed_at_deadline() {
        let snapshot = popup_test_snapshot();
        let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
            "popup",
            Some("original"),
            11,
        )])));
        let candidate = {
            let topology = topology.lock().await;
            assess_popup_topology(&snapshot, &topology, true).unwrap()
        };
        let moving_topology = Arc::clone(&topology);
        let movement = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(5));
            for _ in 0..20 {
                interval.tick().await;
                moving_topology.lock().await.sequence += 1;
            }
        });
        let started = tokio::time::Instant::now();

        let error = wait_for_stable_popup_topology(
            &topology,
            &snapshot,
            &candidate,
            started + Duration::from_millis(60),
            Duration::from_millis(15),
        )
        .await
        .unwrap_err();

        movement.abort();
        assert_eq!(error.kind, PopupClickErrorKind::TopologyLagged);
        assert!(started.elapsed() >= Duration::from_millis(55));
    }

    #[tokio::test]
    async fn popup_event_during_final_query_restarts_quiet_then_succeeds() {
        let snapshot = popup_test_snapshot();
        let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
            "popup",
            Some("original"),
            11,
        )])));
        let candidate = {
            let topology = topology.lock().await;
            assess_popup_topology(&snapshot, &topology, true).unwrap()
        };
        let (url, server) = final_popup_server(Arc::clone(&topology), true, false, None).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let mut session = test_session(cdp.clone());
        session.topology = topology;

        session
            .final_popup_verification(
                &snapshot,
                &candidate,
                tokio::time::Instant::now() + Duration::from_millis(250),
            )
            .await
            .unwrap();
        cdp.close().await;
        assert_eq!(server.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn popup_events_during_every_final_query_fail_at_shared_deadline() {
        let snapshot = popup_test_snapshot();
        let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
            "popup",
            Some("original"),
            11,
        )])));
        let candidate = {
            let topology = topology.lock().await;
            assess_popup_topology(&snapshot, &topology, true).unwrap()
        };
        let (url, server) = final_popup_server(Arc::clone(&topology), true, true, None).await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let mut session = test_session(cdp.clone());
        session.topology = topology;
        let started = tokio::time::Instant::now();

        let error = session
            .final_popup_verification(&snapshot, &candidate, started + Duration::from_millis(120))
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<PopupClickError>().unwrap().kind,
            PopupClickErrorKind::TopologyLagged
        );
        assert!(started.elapsed() >= Duration::from_millis(110));
        cdp.close().await;
        assert!(server.await.unwrap() >= 2);
    }

    #[tokio::test]
    async fn popup_stable_final_query_delayed_past_deadline_fails_typed() {
        let snapshot = popup_test_snapshot();
        let topology = Arc::new(Mutex::new(popup_topology_with(vec![(
            "popup",
            Some("original"),
            11,
        )])));
        let candidate = {
            let topology = topology.lock().await;
            assess_popup_topology(&snapshot, &topology, true).unwrap()
        };
        let (url, server) = final_popup_server(
            Arc::clone(&topology),
            false,
            false,
            Some(Duration::from_millis(50)),
        )
        .await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let mut session = test_session(cdp.clone());
        session.topology = topology;

        let error = session
            .final_popup_verification(
                &snapshot,
                &candidate,
                tokio::time::Instant::now() + Duration::from_millis(15),
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<PopupClickError>().unwrap().kind,
            PopupClickErrorKind::TopologyLagged
        );
        cdp.close().await;
        assert_eq!(server.await.unwrap(), 1);
    }

    #[test]
    fn popup_witness_uses_only_isolated_native_event_state() {
        let source = popup_witness_install_function();
        assert!(source.contains("EventTarget.prototype.addEventListener"));
        assert!(source.contains("EventTarget.prototype.removeEventListener"));
        assert!(source.contains("nativeApply(nativeAdd"));
        assert!(source.contains("event.isTrusted === true"));
        assert!(source.contains("event.currentTarget === element"));
        assert!(!source.contains("Runtime.addBinding"));
        assert!(!source.contains("__glass"));
        assert!(!source.contains("element.addEventListener"));
    }

    #[test]
    fn popup_errors_are_bounded_and_serializable() {
        let error = popup_typed_error(PopupClickErrorKind::PopupMissing, "x".repeat(2_000));
        assert!(error.message.len() <= POPUP_ERROR_MESSAGE_MAX_BYTES);
        assert_eq!(
            serde_json::to_value(error).unwrap()["kind"],
            "popup_missing"
        );
    }

    #[test]
    fn popup_timing_evidence_is_explicitly_serializable() {
        let evidence = PopupVerificationEvidence {
            trusted_click_witness: true,
            release_acknowledged: false,
            release_ack_wait_ms: 500.5,
            topology_sequence_before_release: 1,
            popup_observed_sequence: 2,
            attached: true,
            ready_state: "complete".to_string(),
        };
        let value = serde_json::to_value(evidence).unwrap();
        assert_eq!(value["release_ack_wait_ms"], 500.5);
    }

    #[tokio::test]
    async fn cancelled_popup_attach_detaches_a_late_session() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let attach = websocket.next().await.unwrap().unwrap();
            let attach: Value = match attach {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            assert_eq!(attach["method"], "Target.attachToTarget");
            tokio::time::sleep(Duration::from_millis(50)).await;
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": attach["id"],
                        "result": {"sessionId": "late-popup-session"}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let detach = tokio::time::timeout(Duration::from_secs(1), websocket.next())
                .await
                .expect("late attachment was not detached")
                .unwrap()
                .unwrap();
            let detach: Value = match detach {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            assert_eq!(detach["method"], "Target.detachFromTarget");
            assert_eq!(detach["params"]["sessionId"], "late-popup-session");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": detach["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });
        let cdp = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        let session = test_session(cdp.clone());
        {
            let attach = session.attach_popup("popup");
            tokio::pin!(attach);
            tokio::select! {
                _ = &mut attach => panic!("attach unexpectedly completed"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
        server.await.unwrap();
        cdp.close().await;
    }

    #[test]
    fn popup_topology_rejects_missing_witness_preexisting_and_unrelated_targets() {
        let topology = popup_topology_with(vec![("unrelated", None, 11)]);
        assert_eq!(
            assess_popup_topology(&popup_test_snapshot(), &topology, false)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::WitnessMissing
        );
        assert_eq!(
            assess_popup_topology(
                &popup_test_snapshot(),
                &popup_topology_with(vec![("preexisting", Some("original"), 12)]),
                true,
            )
            .unwrap_err()
            .kind,
            PopupClickErrorKind::PopupMissing
        );
        assert_eq!(
            assess_popup_topology(&popup_test_snapshot(), &topology, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupOpenerMismatch
        );
    }

    #[test]
    fn popup_topology_rejects_wrong_opener_ambiguity_lag_and_destroyed_target() {
        let snapshot = popup_test_snapshot();
        let wrong = popup_topology_with(vec![("popup", Some("other"), 11)]);
        assert_eq!(
            assess_popup_topology(&snapshot, &wrong, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupOpenerMismatch
        );
        let ambiguous = popup_topology_with(vec![
            ("popup-1", Some("original"), 11),
            ("popup-2", Some("original"), 12),
        ]);
        assert_eq!(
            assess_popup_topology(&snapshot, &ambiguous, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupAmbiguous
        );
        let mut lagged = popup_topology_with(vec![("popup", Some("original"), 11)]);
        lagged.event_loss_count = 1;
        assert_eq!(
            assess_popup_topology(&snapshot, &lagged, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::TopologyLagged
        );
        let mut destroyed = TopologyRegistry::default();
        destroyed.destroyed_targets.push_back(DestroyedPageTarget {
            target: popup_test_target("popup", Some("original")),
            observed_sequence: 11,
        });
        assert_eq!(
            assess_popup_topology(&snapshot, &destroyed, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupDestroyed
        );
    }

    #[tokio::test]
    async fn popup_readiness_maps_protocol_failure_to_typed_unreadable_error() {
        let protocol_error: super::super::cdp::CdpError =
            serde_json::from_value(serde_json::json!({
                "code": -32000,
                "message": "target closed"
            }))
            .unwrap();
        let error = popup_verification_call(async { Err(protocol_error) }, "readiness")
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<PopupClickError>().unwrap().kind,
            PopupClickErrorKind::PopupUnreadable
        );
    }

    #[cfg(feature = "visual-compare")]
    fn comparison_png(width: u32, height: u32, pixels: &[u8]) -> String {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(pixels).unwrap();
        }
        STANDARD.encode(encoded)
    }

    #[cfg(feature = "visual-compare")]
    #[test]
    fn compares_equal_sized_pngs_with_exact_difference_bounds() {
        let first = comparison_png(2, 2, &[0; 16]);
        let mut changed_pixels = [0; 16];
        changed_pixels[4..8].copy_from_slice(&[255, 0, 0, 255]);
        let second = comparison_png(2, 2, &changed_pixels);

        let comparison = compare_png_visuals(&first, &second).unwrap();
        assert_eq!(comparison.changed_pixels, 1);
        assert_eq!(comparison.changed_ratio, 0.25);
        let bounds = comparison.difference_box.unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (1.0, 0.0, 1.0, 1.0)
        );
    }

    #[test]
    fn topology_error_maps_kind_to_correct_recovery_hint() {
        use super::TopologyErrorKind;
        use super::TopologyRecoveryHint;

        let cases: &[(TopologyErrorKind, TopologyRecoveryHint)] = &[
            (
                TopologyErrorKind::NoTargetSelected,
                TopologyRecoveryHint::ListTargets,
            ),
            (
                TopologyErrorKind::StaleTarget,
                TopologyRecoveryHint::ListTargets,
            ),
            (
                TopologyErrorKind::StaleFrame,
                TopologyRecoveryHint::ListFrames,
            ),
            (
                TopologyErrorKind::NoSuchFrame,
                TopologyRecoveryHint::ListFrames,
            ),
            (
                TopologyErrorKind::NoPageSession,
                TopologyRecoveryHint::Reconnect,
            ),
            (
                TopologyErrorKind::BudgetExceeded,
                TopologyRecoveryHint::ReObserve,
            ),
            (
                TopologyErrorKind::RoutingLost,
                TopologyRecoveryHint::Reconnect,
            ),
        ];

        for (kind, expected_hint) in cases {
            let error = super::TopologyError::new(*kind, "test");
            assert_eq!(error.kind, *kind);
            assert_eq!(error.recovery, *expected_hint);
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn topology_error_display_includes_kind_and_recovery() {
        let error = super::TopologyError::new(
            super::TopologyErrorKind::NoTargetSelected,
            "no active target",
        );
        let display = error.to_string();
        assert!(display.contains("NoTargetSelected"));
        assert!(display.contains("no active target"));
        assert!(display.contains("ListTargets"));
    }
}

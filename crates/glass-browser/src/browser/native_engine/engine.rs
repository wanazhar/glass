use super::browsing_context::NativeBrowsingContext;
use super::config::{
    NativeEngineConfig, Viewport, decode_percent_encoded_fragment, decode_text_fragment_terms,
    is_network_url, resolve_fixture_relative_url, validate_context_id, validate_url_text,
    without_fragment,
};
use super::content_process::{
    NativeContentLoad, NativeContentLoadResult, NativeContentMutation, NativeContentNavigation,
    NativeContentProcess, NativeContentScriptResult,
};
use super::diagnostics::NativeDiagnostic;
use super::dom::{NativeDocument, NativeNodeId, NativeScriptDocumentSnapshot};
use super::environment::{NativeEnvironmentOverrides, NativeGeolocation, NativeNetworkConditions};
use super::error::NativeEngineError;
use super::error::NativeWorkerFailureKind;
use super::history::{NativeHistory, NativeHistoryDirection};
use super::interaction::{
    MAX_NATIVE_EFFECTS, NativeAction, NativeEffect, NativeEventKind, parse_native_shortcut,
    validate_native_edit_key, validate_native_key,
};
use super::javascript::{
    MAX_NATIVE_DIALOG_TEXT_BYTES, MAX_NATIVE_DIALOGS, MAX_NATIVE_HISTORY_STATE_BYTES,
    MAX_NATIVE_SCRIPT_BYTES, NativeCookieProfileEntry, NativeDialog, NativeFrameScriptBinding,
    NativeFrameScriptContext, NativeFrameScriptRequest, NativeHashChangeEvent,
    NativeIndexedDbChange, NativeIndexedDbState, NativeJavaScriptRuntime,
    NativeMessagePortPageMessage, NativePageEventBatch, NativePageMessageEvent,
    NativePageNavigation, NativePopupRequest, NativePostMessageRequest, NativeScriptCommand,
    NativeScriptEvaluation, NativeServiceWorkerClientLease, NativeServiceWorkerClientMessage,
    NativeServiceWorkerClientState, NativeServiceWorkerOpenWindowRequest, NativeStorageEvent,
    NativeWebStorageState, NativeWindowCloseRequest, NativeWindowNavigationRequest,
    NativeWindowProxyUpdate, NativeWorkerRegistry, append_storage_changes,
    apply_indexed_db_changes, diff_indexed_db_changes, execute_dynamic_page_scripts,
    execute_inline_scripts, frame_event_batch, host_event_batch,
    host_key_event_batch_with_modifiers, host_submit_event_batch, load_indexed_db_profile,
    load_service_worker_client_leases, load_web_storage_profile, new_storage_writer_id,
    page_script_sources_to_scripts, read_storage_event_journal, register_storage_reader,
    save_web_storage_profile, storage_event_cursor, storage_key,
    unregister_service_worker_client_lease, unregister_storage_reader,
    validate_frame_script_command, validate_service_worker_client_states,
};
use super::layout::{NativeLayoutSnapshot, NativePoint, NativeRect};
use super::lifecycle::NativeLifecycleState;
use super::origin::NativeOrigin;
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
use super::resource_loader::{
    NativeFetchResponse, NativeNavigationMethod, NativeNavigationRequest, NativeResource,
    NativeResourceLoader, csp_sources_allow, referrer_for_navigation,
};
use super::runtime::{NativeRuntimeState, NativeRuntimeTraceEvent};
use super::scheduler::{DeterministicScheduler, NativeTask};
use super::service_worker::native_service_worker_client_id;
use super::worker::{NativeRuntimeShared, NativeRuntimeWorker};
use crate::browser::session::{
    Cookie, DownloadOutcome, GeoLocation, NetworkConditions, PendingDialog,
};
use crate::browser_backend::{PromptDecision, PromptResult, StorageOperation, StorageScope};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

const MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS: usize = 8;
const MAX_NATIVE_IN_FLIGHT_REQUESTS: usize = 64;
const MAX_NATIVE_PENDING_DOWNLOADS: usize = 8;
const MAX_NATIVE_PENDING_POPUPS: usize = 8;
const MAX_NATIVE_COMPLETED_DOWNLOAD_IDS: usize = 8;
const MAX_NATIVE_DOWNLOAD_FILENAME_BYTES: usize = 128;
const MAX_NATIVE_DOWNLOAD_DEADLINE: Duration = Duration::from_secs(30);
const MAX_NATIVE_HISTORY_DELTA: i32 = 1024;

#[derive(Debug)]
struct NativeRequestLedger {
    in_flight: usize,
    completed: u64,
    last_activity: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativePendingDownload {
    guid: String,
    url: String,
    suggested_filename: String,
    target_id: String,
    frame_id: String,
}

#[derive(Debug, Clone)]
struct NativePendingServiceWorkerNavigation {
    history_commit: HistoryCommit,
    page_navigation_handoffs: usize,
}

impl NativeRequestLedger {
    fn new() -> Self {
        Self {
            in_flight: 0,
            completed: 0,
            last_activity: Instant::now(),
        }
    }

    fn begin(&mut self) -> Result<(), NativeEngineError> {
        if self.in_flight >= MAX_NATIVE_IN_FLIGHT_REQUESTS {
            return Err(NativeEngineError::limit(
                "native in-flight requests",
                MAX_NATIVE_IN_FLIGHT_REQUESTS,
                self.in_flight.saturating_add(1),
            ));
        }
        self.in_flight += 1;
        self.last_activity = Instant::now();
        Ok(())
    }

    fn finish(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.completed = self.completed.saturating_add(1);
        self.last_activity = Instant::now();
    }

    fn quiet(&self, duration: Duration) -> (bool, String) {
        let quiet_for = self.last_activity.elapsed();
        let matched = self.in_flight == 0 && quiet_for >= duration;
        (
            matched,
            format!(
                "inFlight={};quietForMs={};requiredMs={};completed={}",
                self.in_flight,
                quiet_for.as_millis(),
                duration.as_millis(),
                self.completed,
            ),
        )
    }
}

fn should_apply_native_key_default(key: &str, modifiers: i64) -> bool {
    let primary_modifier = modifiers & (2 | 4) != 0;
    if primary_modifier {
        return key.eq_ignore_ascii_case("a") && modifiers & 1 == 0;
    }
    modifiers & 1 == 0
        && (key.chars().count() == 1
            || matches!(
                key,
                "Backspace" | "Delete" | "ArrowLeft" | "ArrowRight" | "Home" | "End" | "Tab"
            ))
}

fn public_cookie_from_profile(
    profile: NativeCookieProfileEntry,
) -> Result<Cookie, NativeEngineError> {
    let size = profile
        .name
        .len()
        .saturating_add(profile.value.len())
        .try_into()
        .ok();
    Ok(Cookie {
        name: profile.name,
        value: profile.value,
        domain: profile.domain,
        path: profile.path,
        expires: profile
            .expires_at_unix_seconds
            .map_or(0.0, |expires| expires as f64),
        http_only: profile.http_only,
        secure: profile.secure,
        same_site: profile.same_site,
        is_session: profile.expires_at_unix_seconds.is_none(),
        size,
        priority: profile.priority,
    })
}

fn profile_from_public_cookie(
    cookie: &Cookie,
) -> Result<NativeCookieProfileEntry, NativeEngineError> {
    if cookie.name.is_empty() || cookie.domain.is_empty() {
        return Err(NativeEngineError::invalid(
            "cookie",
            "name and domain must not be empty",
        ));
    }
    if !cookie.expires.is_finite() || cookie.expires < 0.0 {
        return Err(NativeEngineError::invalid(
            "cookie expiration",
            "must be a finite non-negative Unix timestamp",
        ));
    }
    let expires_at_unix_seconds = if cookie.is_session || cookie.expires == 0.0 {
        None
    } else if cookie.expires > u64::MAX as f64 {
        return Err(NativeEngineError::invalid(
            "cookie expiration",
            "exceeds the supported Unix timestamp range",
        ));
    } else {
        Some(cookie.expires.floor() as u64)
    };
    let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
    let path = if cookie.path.is_empty() {
        "/".to_owned()
    } else {
        cookie.path.clone()
    };
    Ok(NativeCookieProfileEntry {
        name: cookie.name.clone(),
        value: cookie.value.clone(),
        domain,
        path,
        host_only: !cookie.domain.starts_with('.'),
        secure: cookie.secure,
        http_only: cookie.http_only,
        same_site: cookie.same_site.clone(),
        priority: cookie.priority.clone(),
        expires_at_unix_seconds,
    })
}

fn native_action_supported(
    action: NativePreflightAction,
    node: &super::dom::NativeSemanticNode,
) -> bool {
    match action {
        NativePreflightAction::Click => matches!(
            node.role.as_str(),
            "button" | "link" | "checkbox" | "radio" | "textbox" | "combobox" | "option"
        ),
        NativePreflightAction::Hover => !node.hidden,
        NativePreflightAction::Upload => node.role == "file" && node.tag_name == "input",
        NativePreflightAction::Type => {
            node.role == "textbox" && matches!(node.tag_name.as_str(), "input" | "textarea")
        }
        NativePreflightAction::Check => node.role == "checkbox" || node.role == "radio",
        NativePreflightAction::Select => node.role == "combobox" && node.tag_name == "select",
    }
}

/// Bounded observation of the current native document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEngineSnapshot {
    pub lifecycle: NativeLifecycleState,
    pub url: String,
    pub origin: NativeOrigin,
    pub title: String,
    pub title_truncated: bool,
    pub visible_text: String,
    pub text_truncated: bool,
    pub revision: u64,
    pub viewport: super::config::Viewport,
}

/// Result of an accepted native semantic action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeActionResult {
    pub revision: u64,
    pub accepted: bool,
}

/// Bounded native effects observed since a caller's revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEffectsSnapshot {
    pub revision: u64,
    pub changed: bool,
    pub effects: Vec<NativeEffect>,
}

/// Bounded diagnostics associated with the current native document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDiagnosticsSnapshot {
    pub revision: u64,
    pub diagnostics: Vec<NativeDiagnostic>,
    pub truncated: bool,
}

/// Action families accepted by the shared browser preflight surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePreflightAction {
    Click,
    Hover,
    Upload,
    Type,
    Check,
    Select,
}

/// Stable resolution taxonomy used by the native preflight projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeTargetErrorKind {
    Ambiguous,
    NotFound,
    StaleReference,
    NotActionable,
}

/// Stable actionability taxonomy used by native callers before mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeActionabilityReason {
    NotVisible,
    OutsideViewport,
    Disabled,
    ReadOnly,
    UnsupportedAction,
}

/// Side-effect-free native target resolution and actionability result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTargetPreflight {
    pub action: NativePreflightAction,
    pub unique: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<super::dom::NativeSemanticNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actionable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actionability_reason: Option<NativeActionabilityReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<NativeTargetErrorKind>,
    pub revision: u64,
    /// Owning frame for a backend-level preflight. The engine-level API does
    /// not select among frames and therefore leaves this unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<NativeRect>,
    pub likely_navigation: bool,
    pub likely_popup: bool,
    pub likely_form_submit: bool,
}

/// One atomic native inspection snapshot used by higher-level semantic
/// discovery surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeInspectionSnapshot {
    pub context_id: String,
    pub snapshot: NativeEngineSnapshot,
    pub nodes: Vec<super::dom::NativeSemanticNode>,
    pub layout: NativeLayoutSnapshot,
}

/// Single-owner native browser kernel.
pub struct NativeEngine {
    config: NativeEngineConfig,
    frame_id: String,
    loader: NativeResourceLoader,
    environment: NativeEnvironmentOverrides,
    runtime: NativeRuntimeShared,
    runtime_worker: Option<NativeRuntimeWorker>,
    content_process: Option<NativeContentProcess>,
    javascript: Option<NativeJavaScriptRuntime>,
    service_worker_clients: Vec<NativeServiceWorkerClientState>,
    workers: NativeWorkerRegistry,
    web_storage: NativeWebStorageState,
    indexed_db: NativeIndexedDbState,
    storage_writer_id: String,
    storage_event_offset: u64,
    storage_state_recovery_pending: bool,
    indexed_db_state_delivery_pending: bool,
    pending_external_storage_events: Vec<NativeStorageEvent>,
    pending_dialogs: VecDeque<PendingDialog>,
    request_ledger: NativeRequestLedger,
    pending_downloads: VecDeque<NativePendingDownload>,
    pending_popups: VecDeque<NativePopupRequest>,
    pending_post_messages: VecDeque<NativePostMessageRequest>,
    pending_message_port_messages: VecDeque<NativeMessagePortPageMessage>,
    pending_window_closes: VecDeque<NativeWindowCloseRequest>,
    pending_window_navigations: VecDeque<NativeWindowNavigationRequest>,
    pending_service_worker_client_messages: VecDeque<NativeServiceWorkerClientMessage>,
    pending_service_worker_open_windows: VecDeque<NativeServiceWorkerOpenWindowRequest>,
    pending_service_worker_navigation: Option<NativePendingServiceWorkerNavigation>,
    pending_frame_scripts: VecDeque<NativeFrameScriptRequest>,
    completed_download_ids: VecDeque<String>,
    completed_downloads: u64,
    next_download_id: u64,
    history: NativeHistory,
    lifecycle: NativeLifecycleState,
    document: NativeDocument,
    url: String,
    origin: NativeOrigin,
    document_frame_sources: Option<Vec<String>>,
    frame_script_bindings: Vec<NativeFrameScriptBinding>,
    frame_script_context: Option<NativeFrameScriptContext>,
    embedding_document_url: Option<String>,
    embedding_frame_sources: Option<Vec<String>>,
    revision: u64,
    scroll_offset: NativePoint,
    nested_scroll_offsets: BTreeMap<u32, NativePoint>,
    effects: VecDeque<NativeEffect>,
    pending_lifecycle_effects: Vec<(NativeNodeId, NativeEventKind)>,
    skip_next_navigation_lifecycle: bool,
}

impl Drop for NativeEngine {
    fn drop(&mut self) {
        if self.lifecycle == NativeLifecycleState::Running {
            let _ = self.persist_local_web_storage();
        }
        if self.lifecycle != NativeLifecycleState::Closed {
            let _ = unregister_storage_reader(
                self.config.storage_path.as_deref(),
                &self.storage_writer_id,
            );
            let _ = unregister_service_worker_client_lease(
                self.config.storage_path.as_deref(),
                &self.service_worker_client_id(),
                &self.storage_writer_id,
            );
        }
    }
}

impl NativeEngine {
    pub fn new(config: NativeEngineConfig) -> Result<Self, NativeEngineError> {
        config.validate()?;
        let web_storage = load_web_storage_profile(config.storage_path.as_deref())?;
        let indexed_db = load_indexed_db_profile(config.storage_path.as_deref())?;
        let storage_event_offset = storage_event_cursor(config.storage_path.as_deref())?;
        let storage_writer_id = new_storage_writer_id()?;
        let loader = NativeResourceLoader::new(&config)?;
        if !is_network_url(&config.initial_url) {
            loader.load(&config.initial_url)?;
        }
        let frame_id = format!("{}:main", config.context_id);
        let own_client = NativeServiceWorkerClientState {
            id: native_service_worker_client_id(&config.context_id, &frame_id),
            url: without_fragment(&config.initial_url).to_owned(),
            client_type: "window".into(),
            frame_type: "top-level".into(),
            visibility_state: "visible".into(),
            focused: true,
        };
        let mut service_worker_clients =
            load_service_worker_client_leases(config.storage_path.as_deref())?;
        if let Some(client) = service_worker_clients
            .iter_mut()
            .find(|client| client.id == own_client.id)
        {
            *client = own_client;
        } else {
            service_worker_clients.push(own_client);
        }
        validate_service_worker_client_states(&service_worker_clients)?;
        let runtime = NativeRuntimeShared::new(config.limits.max_scheduler_tasks)?;
        let max_history_entries = config.limits.max_history_entries;
        register_storage_reader(
            config.storage_path.as_deref(),
            &storage_writer_id,
            storage_event_offset,
        )?;
        Ok(Self {
            url: config.initial_url.clone(),
            frame_id,
            config,
            loader,
            environment: NativeEnvironmentOverrides::default(),
            runtime,
            runtime_worker: None,
            content_process: None,
            javascript: None,
            service_worker_clients,
            workers: NativeWorkerRegistry::new(),
            web_storage,
            indexed_db,
            storage_writer_id,
            storage_event_offset,
            storage_state_recovery_pending: false,
            indexed_db_state_delivery_pending: false,
            pending_external_storage_events: Vec::new(),
            pending_dialogs: VecDeque::new(),
            request_ledger: NativeRequestLedger::new(),
            pending_downloads: VecDeque::new(),
            pending_popups: VecDeque::new(),
            pending_post_messages: VecDeque::new(),
            pending_message_port_messages: VecDeque::new(),
            pending_window_closes: VecDeque::new(),
            pending_window_navigations: VecDeque::new(),
            pending_service_worker_client_messages: VecDeque::new(),
            pending_service_worker_open_windows: VecDeque::new(),
            pending_service_worker_navigation: None,
            pending_frame_scripts: VecDeque::new(),
            completed_download_ids: VecDeque::new(),
            completed_downloads: 0,
            next_download_id: 1,
            history: NativeHistory::new(max_history_entries),
            lifecycle: NativeLifecycleState::New,
            document: NativeDocument::empty(),
            origin: NativeOrigin::Opaque,
            document_frame_sources: None,
            frame_script_bindings: Vec::new(),
            frame_script_context: None,
            embedding_document_url: None,
            embedding_frame_sources: None,
            revision: 0,
            scroll_offset: NativePoint { x: 0, y: 0 },
            nested_scroll_offsets: BTreeMap::new(),
            effects: VecDeque::new(),
            pending_lifecycle_effects: Vec::new(),
            skip_next_navigation_lifecycle: false,
        })
    }

    pub fn config(&self) -> &NativeEngineConfig {
        &self.config
    }

    /// Apply session-scoped network shaping to top-level navigation and every
    /// resource-loader owner used by the current document.
    pub async fn set_network_conditions_async(
        &mut self,
        conditions: Option<&NetworkConditions>,
    ) -> Result<(), NativeEngineError> {
        self.require_running("set network conditions")?;
        let mut next = self.environment.clone();
        next.network = NativeNetworkConditions::from_public(conditions)?;
        next.validate()?;
        self.loader.set_environment(&next)?;
        if let Some(process) = self.content_process.as_mut() {
            process.set_environment(&next).await?;
        }
        self.environment = next;
        Ok(())
    }

    /// Apply the same bounded CPU throttling multiplier used by the Chromium
    /// session. A value of `1` is normal execution; larger values lengthen
    /// browser-visible timer intervals and preserve the setting across the
    /// live content worker.
    pub async fn set_cpu_throttling_async(
        &mut self,
        rate: Option<f64>,
    ) -> Result<(), NativeEngineError> {
        self.require_running("set CPU throttling")?;
        let mut next = self.environment.clone();
        next.cpu_throttling_rate = rate.unwrap_or(1.0);
        next.validate()?;
        if let Some(process) = self.content_process.as_mut() {
            process.set_environment(&next).await?;
        }
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_environment(next.clone());
        }
        self.environment = next;
        Ok(())
    }

    /// Override the request and `navigator` user-agent identity for the
    /// current native browsing context. Passing `None` restores the native
    /// default identity.
    pub async fn set_user_agent_async(
        &mut self,
        user_agent: Option<&str>,
        accept_language: Option<&str>,
        platform: Option<&str>,
    ) -> Result<(), NativeEngineError> {
        self.require_running("set user agent")?;
        let mut next = self.environment.clone();
        next.user_agent = user_agent.map(str::to_owned);
        next.accept_language = accept_language.map(str::to_owned);
        next.platform = platform.map(str::to_owned);
        next.validate()?;
        self.loader.set_environment(&next)?;
        if let Some(process) = self.content_process.as_mut() {
            process.set_environment(&next).await?;
        }
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_environment(next.clone());
        }
        self.environment = next;
        Ok(())
    }

    /// Override the page-visible geolocation. The native host does not read
    /// the machine's position; clearing the override restores the denied
    /// permission state.
    pub async fn set_geolocation_async(
        &mut self,
        location: Option<&GeoLocation>,
    ) -> Result<(), NativeEngineError> {
        self.require_running("set geolocation")?;
        let mut next = self.environment.clone();
        next.geolocation = location.map(NativeGeolocation::from_public).transpose()?;
        next.validate()?;
        if let Some(process) = self.content_process.as_mut() {
            process.set_environment(&next).await?;
        }
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_environment(next.clone());
        }
        self.environment = next;
        Ok(())
    }

    /// Override the page-visible IANA timezone identifier. Clearing it uses
    /// the native default (`UTC`) rather than consulting ambient host state.
    pub async fn set_timezone_async(
        &mut self,
        timezone_id: Option<&str>,
    ) -> Result<(), NativeEngineError> {
        self.require_running("set timezone")?;
        let mut next = self.environment.clone();
        next.timezone_id = timezone_id.map(str::to_owned);
        next.validate()?;
        if let Some(process) = self.content_process.as_mut() {
            process.set_environment(&next).await?;
        }
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_environment(next.clone());
        }
        self.environment = next;
        Ok(())
    }

    pub(crate) fn set_frame_id(&mut self, frame_id: String) {
        let previous_client_id =
            native_service_worker_client_id(&self.config.context_id, &self.frame_id);
        self.frame_id = frame_id;
        let next_client_id = self.service_worker_client_id();
        if let Some(client) = self
            .service_worker_clients
            .iter_mut()
            .find(|client| client.id == previous_client_id)
        {
            client.id = next_client_id;
            client.frame_type = "nested".into();
            client.focused = false;
        }
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_frame_id(self.frame_id.clone());
        }
    }

    pub(crate) fn service_worker_client_id(&self) -> String {
        native_service_worker_client_id(&self.config.context_id, &self.frame_id)
    }

    pub(crate) fn service_worker_client_state(
        &self,
        frame_type: &str,
        focused: bool,
    ) -> NativeServiceWorkerClientState {
        NativeServiceWorkerClientState {
            id: self.service_worker_client_id(),
            url: without_fragment(&self.url).to_owned(),
            client_type: "window".into(),
            frame_type: frame_type.to_owned(),
            visibility_state: "visible".into(),
            focused,
        }
    }

    pub(crate) fn service_worker_client_lease(
        &self,
        frame_type: &str,
        focused: bool,
    ) -> NativeServiceWorkerClientLease {
        NativeServiceWorkerClientLease {
            state: self.service_worker_client_state(frame_type, focused),
            owner_id: self.storage_writer_id.clone(),
            heartbeat_unix_seconds: 0,
        }
    }

    pub(crate) async fn replace_service_worker_clients(
        &mut self,
        clients: Vec<NativeServiceWorkerClientState>,
    ) -> Result<(), NativeEngineError> {
        if self.service_worker_clients == clients {
            return Ok(());
        }
        if let Some(process) = self.content_process.as_mut() {
            process.sync_service_worker_clients(&clients).await?;
        }
        self.service_worker_clients = clients;
        Ok(())
    }

    pub(crate) async fn synchronize_service_worker_registrations(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        if self.config.storage_path.is_none() {
            return Ok(());
        }
        if let Some(process) = self.content_process.as_mut() {
            process.sync_service_worker_registrations().await?;
        }
        Ok(())
    }

    pub(crate) async fn resolve_service_worker_open_window_async(
        &mut self,
        worker_id: u32,
        request_id: u32,
        window: serde_json::Value,
    ) -> Result<(), NativeEngineError> {
        self.require_running("resolve service worker openWindow")?;
        self.sync_external_storage_events()?;
        let NativeContentScriptResult {
            value: _,
            mutation,
            history,
            frame_scripts,
            storage_events,
            indexed_db_changes,
            dialogs,
            popups,
            post_messages,
            window_closes,
            window_navigations,
            service_worker_client_messages,
            service_worker_open_windows,
            service_worker_fetch_resumed,
            window_name,
        } = {
            let process =
                self.content_process
                    .as_mut()
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "service worker openWindow resolution".into(),
                        reason: "native content process is not running".into(),
                    })?;
            if !process.refresh_health() {
                return Err(NativeEngineError::worker_failure(
                    "service worker openWindow resolution",
                    process
                        .failure_kind()
                        .unwrap_or(NativeWorkerFailureKind::Exited),
                    "content process is unavailable after a failed operation; navigate to recover it",
                ));
            }
            self.request_ledger.begin()?;
            let result = process
                .resolve_service_worker_open_window(worker_id, request_id, &window)
                .await;
            self.request_ledger.finish();
            result?
        };
        self.config.window_name = window_name;
        self.queue_frame_script_requests(frame_scripts)?;
        self.queue_service_worker_client_messages(service_worker_client_messages)?;
        self.queue_service_worker_open_window_requests(service_worker_open_windows)?;
        if let Some(mutation) = mutation {
            let navigation = mutation.navigation.clone();
            self.apply_content_process_mutation(mutation)?;
            self.apply_content_history_commands(&history)?;
            if let Some(navigation) = navigation {
                self.navigate_script_navigation_async(navigation, 0).await?;
            }
        } else {
            self.publish_content_state(&storage_events, &indexed_db_changes)?;
            self.queue_popup_requests(popups)?;
            self.queue_post_message_requests(post_messages)?;
            self.queue_window_close_requests(window_closes)?;
            self.queue_window_navigation_requests(window_navigations)?;
            let dialog_url = self.url.clone();
            self.install_dialogs(dialogs, &dialog_url)?;
        }
        if !history.is_empty() {
            self.sync_content_history_async().await?;
        }
        if service_worker_fetch_resumed {
            self.resume_pending_service_worker_navigation().await?;
        }
        Ok(())
    }

    pub(crate) async fn dispatch_service_worker_client_message_async(
        &mut self,
        message: NativeServiceWorkerClientMessage,
    ) -> Result<(), NativeEngineError> {
        self.require_running("deliver service worker client message")?;
        if message.client_id != self.service_worker_client_id() {
            return Err(NativeEngineError::invalid(
                "service worker client message target",
                "message client does not belong to this native frame",
            ));
        }
        let mut page_events = NativePageEventBatch::default();
        page_events.service_worker_client_messages.push(message);
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub(crate) fn service_worker_clients(&self) -> Vec<NativeServiceWorkerClientState> {
        self.service_worker_clients.clone()
    }

    pub(crate) fn inherit_service_worker_clients(
        &mut self,
        clients: &[NativeServiceWorkerClientState],
    ) {
        self.service_worker_clients = clients.to_vec();
    }

    pub(crate) fn set_frame_script_bindings(&mut self, bindings: Vec<NativeFrameScriptBinding>) {
        self.frame_script_bindings = bindings.clone();
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_frame_script_bindings(bindings);
        }
    }

    pub(crate) fn set_frame_script_context(&mut self, context: Option<NativeFrameScriptContext>) {
        self.frame_script_context = context.clone();
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_frame_script_context(context);
        }
    }

    pub(crate) fn script_document_snapshot(
        &self,
    ) -> Result<NativeScriptDocumentSnapshot, NativeEngineError> {
        self.require_running("frame script projection")?;
        self.document.script_snapshot_with_layout(
            crate::browser_backend::MAX_TEXT_BYTES,
            self.config.viewport,
            self.scroll_offset,
            &self.nested_scroll_offsets,
        )
    }

    pub(crate) fn document_generation(&self) -> Result<u32, NativeEngineError> {
        self.require_running("frame discovery")?;
        Ok(self.document.generation())
    }

    pub(crate) fn embedded_frame_sources(&self) -> Result<Vec<(u32, String)>, NativeEngineError> {
        self.require_running("frame discovery")?;
        Ok(self
            .document
            .embedded_frame_sources()
            .into_iter()
            .map(|(node_id, source)| (node_id.index(), source))
            .collect())
    }

    pub(crate) fn resolve_embedded_frame_url(
        &self,
        source: &str,
    ) -> Result<String, NativeEngineError> {
        self.require_running("frame discovery")?;
        self.resolve_link_href(source)
    }

    pub(crate) fn allows_embedded_frame_url(
        &self,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        self.require_running("frame discovery")?;
        let Some(frame_sources) = self.document_frame_sources.as_deref() else {
            return Ok(true);
        };
        validate_url_text("embedded frame URL", target_url)?;
        let document_url = url::Url::parse(without_fragment(&self.url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "the frame policy owner URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = url::Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "embedded frame URL is not valid URL syntax".into(),
            }
        })?;
        Ok(csp_sources_allow(
            Some(frame_sources),
            &document_url,
            &target_url,
        ))
    }

    pub(crate) fn frame_navigation_policy(&self) -> (String, Option<Vec<String>>) {
        (self.url.clone(), self.document_frame_sources.clone())
    }

    pub(crate) fn set_embedding_frame_policy(
        &mut self,
        document_url: String,
        frame_sources: Option<Vec<String>>,
    ) {
        self.embedding_document_url = Some(document_url);
        self.embedding_frame_sources = frame_sources;
    }

    fn allows_frame_navigation(&self, target_url: &str) -> Result<bool, NativeEngineError> {
        let Some(document_url) = self.embedding_document_url.as_deref() else {
            return Ok(true);
        };
        validate_url_text("frame navigation URL", target_url)?;
        let document_url = url::Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "the frame policy owner URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = url::Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "frame navigation URL is not valid URL syntax".into(),
            }
        })?;
        Ok(csp_sources_allow(
            self.embedding_frame_sources.as_deref(),
            &document_url,
            &target_url,
        ))
    }

    pub const fn lifecycle(&self) -> NativeLifecycleState {
        self.lifecycle
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the current root viewport scroll offset.
    pub const fn scroll_offset(&self) -> NativePoint {
        self.scroll_offset
    }

    pub fn initialize(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => return Ok(()),
            NativeLifecycleState::Closed => {
                return Err(
                    self.lifecycle_error("initialize", "a closed native engine cannot be reopened")
                );
            }
            NativeLifecycleState::New => {}
        }
        let prepared = self.prepare_navigation(&self.config.initial_url.clone())?;
        self.runtime.start()?;
        let page_navigation = match self.commit_navigation(prepared, HistoryCommit::Push) {
            Ok(page_navigation) => page_navigation,
            Err(error) => {
                self.runtime.rollback_start()?;
                return Err(error);
            }
        };
        self.lifecycle = NativeLifecycleState::Running;
        if let Some(page_navigation) = page_navigation {
            self.navigate_page_script_sync(page_navigation, 1)?;
        }
        Ok(())
    }

    pub async fn initialize_async(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => return Ok(()),
            NativeLifecycleState::Closed => {
                return Err(
                    self.lifecycle_error("initialize", "a closed native engine cannot be reopened")
                );
            }
            NativeLifecycleState::New => {}
        }
        let initial_url = self.config.initial_url.clone();
        let mut content_process = if is_network_url(&initial_url) {
            Some(NativeContentProcess::spawn(self.config.storage_path.as_deref()).await?)
        } else {
            None
        };
        if let Some(process) = content_process.as_mut() {
            process
                .start(
                    self.config.storage_path.as_deref(),
                    &self.config.context_id,
                    &self.frame_id,
                    &self.config.window_name,
                    self.config.opener_context_id.as_deref(),
                    &self.config.opener_window_name,
                    &self.config.opener_url,
                    self.frame_script_context.as_ref(),
                    &self.environment,
                    &self.service_worker_clients,
                )
                .await?;
        }
        let has_content_process = content_process.is_some();
        self.content_process = content_process;
        let mut use_content_process = has_content_process;
        let (prepared, history_commit, page_navigation_handoffs) = if has_content_process {
            let Some((content, history_commit, page_navigation_handoffs)) = self
                .load_content_with_page_navigation(
                    NativeNavigationRequest::get(initial_url.clone()),
                    None,
                    HistoryCommit::Push,
                    0,
                )
                .await?
            else {
                let worker = NativeRuntimeWorker::spawn_shared(self.runtime.clone())?;
                worker.start().await?;
                self.runtime_worker = Some(worker);
                self.lifecycle = NativeLifecycleState::Running;
                return Ok(());
            };
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation(&content.url)?
            {
                self.content_process.take();
                use_content_process = false;
                (
                    self.prepare_navigation_resource(self.loader.load("about:blank")?)?,
                    HistoryCommit::Push,
                    0,
                )
            } else {
                (
                    self.prepare_navigation_content(content)?,
                    history_commit,
                    page_navigation_handoffs,
                )
            }
        } else {
            (
                self.prepare_navigation_async(&initial_url).await?,
                HistoryCommit::Push,
                0,
            )
        };
        let worker = NativeRuntimeWorker::spawn_shared(self.runtime.clone())?;
        worker.start().await?;
        if use_content_process {
            self.commit_content_process().await?;
        }
        let page_navigation = match self
            .commit_navigation_async(prepared, &worker, history_commit)
            .await
        {
            Ok(page_navigation) => page_navigation,
            Err(error) => {
                self.content_process.take();
                worker.rollback_start().await?;
                return Err(error);
            }
        };
        self.runtime_worker = Some(worker);
        self.lifecycle = NativeLifecycleState::Running;
        if let Some(page_navigation) = page_navigation {
            self.navigate_request_async(page_navigation, 1).await?;
        } else if use_content_process
            && let Some(page_navigation) = self.dispatch_content_page_show_async().await?
        {
            self.navigate_script_navigation_async(page_navigation, page_navigation_handoffs)
                .await?;
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => {
                self.persist_local_web_storage()?;
                unregister_storage_reader(
                    self.config.storage_path.as_deref(),
                    &self.storage_writer_id,
                )?;
                unregister_service_worker_client_lease(
                    self.config.storage_path.as_deref(),
                    &self.service_worker_client_id(),
                    &self.storage_writer_id,
                )?;
                self.runtime.close()?;
                self.runtime_worker.take();
                self.content_process.take();
                self.lifecycle = NativeLifecycleState::Closed;
                Ok(())
            }
            NativeLifecycleState::New => Err(self.lifecycle_error(
                "close",
                "the native engine must be initialized before close",
            )),
            NativeLifecycleState::Closed => {
                Err(self.lifecycle_error("close", "the native engine is already closed"))
            }
        }
    }

    pub async fn close_async(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => {
                self.persist_local_web_storage()?;
                unregister_storage_reader(
                    self.config.storage_path.as_deref(),
                    &self.storage_writer_id,
                )?;
                unregister_service_worker_client_lease(
                    self.config.storage_path.as_deref(),
                    &self.service_worker_client_id(),
                    &self.storage_writer_id,
                )?;
                self.runtime.close()?;
                self.runtime_worker.take();
                self.lifecycle = NativeLifecycleState::Closed;
                if let Some(mut process) = self.content_process.take()
                    && process.refresh_health()
                {
                    process.close().await?;
                }
                Ok(())
            }
            NativeLifecycleState::New => Err(self.lifecycle_error(
                "close",
                "the native engine must be initialized before close",
            )),
            NativeLifecycleState::Closed => {
                Err(self.lifecycle_error("close", "the native engine is already closed"))
            }
        }
    }

    pub fn navigate(
        &mut self,
        url: impl Into<String>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("navigate")?;
        self.pending_lifecycle_effects.clear();
        self.sync_external_storage_events()?;
        let url = url.into();
        let resource = self.loader.load(&url)?;
        if self.is_same_document_navigation(&resource.url) {
            if let Some(navigation) =
                self.commit_same_document_navigation(resource.url, HistoryCommit::Push)?
            {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else if !self.allows_frame_navigation(&resource.url)? {
            return Ok(self.snapshot_unchecked());
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            if let Some(page_navigation) = self.commit_navigation(prepared, HistoryCommit::Push)? {
                self.navigate_page_script_sync(page_navigation, 1)?;
            }
        }
        Ok(self.snapshot_unchecked())
    }

    pub async fn navigate_async(
        &mut self,
        url: impl Into<String>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async(NativeNavigationRequest::get(url), 0)
            .await
    }

    pub(crate) async fn navigate_async_with_history(
        &mut self,
        url: impl Into<String>,
        replace_history: bool,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        let mut request = NativeNavigationRequest::get(url);
        request.replace_history = replace_history;
        self.navigate_request_async(request, 0).await
    }

    /// Rebuild the current document owner and reload the active URL without
    /// replaying the operation that may have killed the content worker.
    ///
    /// Recovery deliberately skips page unload handlers: a failed worker has
    /// no trustworthy lifecycle state to deliver, and replaying an outgoing
    /// mutation could duplicate an external side effect. The current history
    /// entry is replaced, so recovery does not manufacture a new user-visible
    /// history step.
    pub async fn recover_async(&mut self) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("recover")?;
        if self
            .content_process
            .as_mut()
            .is_some_and(|process| !process.refresh_health())
        {
            self.content_process.take();
        }
        let mut navigation = NativeNavigationRequest::get(self.url.clone());
        navigation.replace_history = true;
        self.navigate_request_async_with_lifecycle(navigation, 0, false)
            .await
    }

    async fn navigate_request_async(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async_with_lifecycle(navigation, page_navigation_handoffs, true)
            .await
    }

    async fn navigate_request_async_with_lifecycle(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        dispatch_lifecycle: bool,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("navigate")?;
        if dispatch_lifecycle {
            self.pending_lifecycle_effects.clear();
        }
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let url = navigation.url.as_str();
        let history_commit = if navigation.replace_history {
            HistoryCommit::Replace
        } else {
            HistoryCommit::Push
        };
        let same_document = self.is_same_document_navigation(url);
        if !same_document && !self.allows_frame_navigation(url)? {
            return Ok(self.snapshot_unchecked());
        }
        if !same_document && dispatch_lifecycle {
            let (allowed, lifecycle_navigation) =
                self.dispatch_navigation_lifecycle_async().await?;
            if !allowed {
                return Ok(self.snapshot_unchecked());
            }
            if let Some(lifecycle_navigation) = lifecycle_navigation {
                if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                    return Err(NativeEngineError::limit(
                        "page navigation handoffs",
                        MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                        page_navigation_handoffs.saturating_add(1),
                    ));
                }
                return Box::pin(self.navigate_request_async_with_lifecycle(
                    lifecycle_navigation,
                    page_navigation_handoffs + 1,
                    false,
                ))
                .await;
            }
        }
        if same_document && navigation.method == NativeNavigationMethod::Get {
            validate_url_text("navigation URL", url)?;
            if let Some(worker) = self.runtime_worker.clone() {
                self.commit_same_document_navigation_async(
                    navigation.url,
                    history_commit,
                    &worker,
                    page_navigation_handoffs,
                )
                .await?;
            } else {
                if let Some(navigation) =
                    self.commit_same_document_navigation(navigation.url, history_commit)?
                {
                    self.navigate_page_script_sync(navigation, page_navigation_handoffs + 1)?;
                }
            }
            return Ok(self.snapshot_unchecked());
        }
        if is_network_url(url) {
            let referrer = referrer_for_navigation(&self.url, url)?;
            self.ensure_content_process().await?;
            let Some((content, history_commit, page_navigation_handoffs)) = self
                .load_content_with_page_navigation(
                    navigation,
                    referrer,
                    history_commit,
                    page_navigation_handoffs,
                )
                .await?
            else {
                return Ok(self.snapshot_unchecked());
            };
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation(&content.url)?
            {
                return Ok(self.snapshot_unchecked());
            }
            self.commit_content_process().await?;
            if let Some(worker) = self.runtime_worker.clone() {
                return self
                    .navigate_content_async(
                        content,
                        &worker,
                        history_commit,
                        page_navigation_handoffs,
                    )
                    .await;
            }
            return self.navigate_content(content, history_commit);
        }
        self.content_process.take();
        let resource = self
            .loader
            .load_async_request_with_referrer(&navigation, None)
            .await?;
        if let Some(worker) = self.runtime_worker.clone() {
            self.navigate_resource_async(
                resource,
                &worker,
                history_commit,
                page_navigation_handoffs,
            )
            .await
        } else {
            if is_network_url(&resource.url) {
                self.commit_content_process().await?;
            }
            self.navigate_resource(resource, history_commit)
        }
    }

    fn navigate_resource(
        &mut self,
        resource: NativeResource,
        history_commit: HistoryCommit,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&resource.url) {
            if let Some(navigation) =
                self.commit_same_document_navigation(resource.url, history_commit)?
            {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            if let Some(page_navigation) = self.commit_navigation(prepared, history_commit)? {
                self.navigate_page_script_sync(page_navigation, 1)?;
            }
        }
        Ok(self.snapshot_unchecked())
    }

    async fn ensure_content_process(&mut self) -> Result<(), NativeEngineError> {
        if self
            .content_process
            .as_mut()
            .is_some_and(|process| !process.refresh_health())
        {
            self.content_process.take();
        }
        if self.content_process.is_none() {
            let mut process =
                NativeContentProcess::spawn(self.config.storage_path.as_deref()).await?;
            process
                .start(
                    self.config.storage_path.as_deref(),
                    &self.config.context_id,
                    &self.frame_id,
                    &self.config.window_name,
                    self.config.opener_context_id.as_deref(),
                    &self.config.opener_window_name,
                    &self.config.opener_url,
                    self.frame_script_context.as_ref(),
                    &self.environment,
                    &self.service_worker_clients,
                )
                .await?;
            self.content_process = Some(process);
        }
        Ok(())
    }

    async fn load_content_with_page_navigation(
        &mut self,
        mut navigation: NativeNavigationRequest,
        mut referrer: Option<String>,
        mut history_commit: HistoryCommit,
        mut page_navigation_handoffs: usize,
    ) -> Result<Option<(NativeContentLoad, HistoryCommit, usize)>, NativeEngineError> {
        if page_navigation_handoffs > MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
            return Err(NativeEngineError::limit(
                "page navigation handoffs",
                MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                page_navigation_handoffs,
            ));
        }
        loop {
            self.request_ledger.begin()?;
            let client_id = self.service_worker_client_id();
            let service_worker_clients = self.service_worker_clients.clone();
            let content_result = match self.content_process.as_mut() {
                Some(process) => {
                    process
                        .load(
                            &navigation,
                            &self.config.limits,
                            self.config.viewport,
                            referrer.as_deref(),
                            &client_id,
                            &service_worker_clients,
                        )
                        .await
                }
                None => Err(NativeEngineError::Worker {
                    operation: "content process load".into(),
                    reason: "native content process is not running".into(),
                }),
            };
            self.request_ledger.finish();
            let mut content = match content_result? {
                NativeContentLoadResult::Loaded(content) => content,
                NativeContentLoadResult::Suspended(open_windows) => {
                    if self.pending_service_worker_navigation.is_some() {
                        return Err(NativeEngineError::Worker {
                            operation: "native service worker navigation".into(),
                            reason: "another service worker navigation is already suspended".into(),
                        });
                    }
                    self.queue_service_worker_open_window_requests(open_windows)?;
                    self.pending_service_worker_navigation =
                        Some(NativePendingServiceWorkerNavigation {
                            history_commit,
                            page_navigation_handoffs,
                        });
                    return Ok(None);
                }
            };
            self.apply_content_load_effects(&mut content)?;
            let Some(page_navigation) = content.navigation.clone() else {
                return Ok(Some((content, history_commit, page_navigation_handoffs)));
            };
            if page_navigation_handoffs == MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                return Err(NativeEngineError::limit(
                    "page navigation handoffs",
                    MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                    page_navigation_handoffs.saturating_add(1),
                ));
            }
            page_navigation_handoffs = page_navigation_handoffs.saturating_add(1);
            let target_url =
                self.resolve_page_navigation_href(&content.url, &page_navigation.href)?;
            navigation = NativeNavigationRequest::get(target_url.clone());
            navigation.replace_history = page_navigation.replace_history;
            history_commit = if page_navigation.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            referrer = referrer_for_navigation(&content.url, &target_url)?;
        }
    }

    fn apply_content_load_effects(
        &mut self,
        content: &mut NativeContentLoad,
    ) -> Result<(), NativeEngineError> {
        self.config.window_name = content.window_name.clone();
        self.publish_content_state(&content.storage_events, &content.indexed_db_changes)?;
        self.queue_popup_requests(std::mem::take(&mut content.popups))?;
        let content_origin = content.origin.clone();
        self.queue_post_message_requests_from_origin(
            std::mem::take(&mut content.post_messages),
            &content_origin,
        )?;
        self.queue_window_close_requests(std::mem::take(&mut content.window_closes))?;
        self.queue_window_navigation_requests(std::mem::take(&mut content.window_navigations))?;
        self.queue_service_worker_client_messages(std::mem::take(
            &mut content.service_worker_client_messages,
        ))?;
        self.queue_service_worker_open_window_requests(std::mem::take(
            &mut content.service_worker_open_windows,
        ))?;
        Ok(())
    }

    async fn resume_pending_service_worker_navigation(&mut self) -> Result<(), NativeEngineError> {
        let content_result = {
            let process =
                self.content_process
                    .as_mut()
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "service worker navigation resume".into(),
                        reason: "native content process is not running".into(),
                    })?;
            if !process.refresh_health() {
                return Err(NativeEngineError::worker_failure(
                    "service worker navigation resume",
                    process
                        .failure_kind()
                        .unwrap_or(NativeWorkerFailureKind::Exited),
                    "content process is unavailable after a failed service worker navigation",
                ));
            }
            self.request_ledger.begin()?;
            let result = process.resume_service_worker_navigation().await;
            self.request_ledger.finish();
            result?
        };
        let mut content = match content_result {
            NativeContentLoadResult::Loaded(content) => content,
            NativeContentLoadResult::Suspended(open_windows) => {
                self.queue_service_worker_open_window_requests(open_windows)?;
                return Ok(());
            }
        };
        let pending = self
            .pending_service_worker_navigation
            .take()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "service worker navigation resume".into(),
                reason: "content process resumed a navigation that the engine did not retain"
                    .into(),
            })?;
        self.apply_content_load_effects(&mut content)?;
        if !self.is_same_document_navigation(&content.url)
            && !self.allows_frame_navigation(&content.url)?
        {
            return Ok(());
        }
        self.commit_content_process().await?;
        let worker = self
            .runtime_worker
            .clone()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "service worker navigation resume".into(),
                reason: "native runtime worker is not running".into(),
            })?;
        self.navigate_content_async(
            content,
            &worker,
            pending.history_commit,
            pending.page_navigation_handoffs,
        )
        .await
        .map(|_| ())
    }

    async fn commit_content_process(&mut self) -> Result<(), NativeEngineError> {
        let Some(process) = self.content_process.as_mut() else {
            return Err(NativeEngineError::Worker {
                operation: "commit content process".into(),
                reason: "native content process is not running".into(),
            });
        };
        process.commit().await
    }

    async fn navigate_resource_async(
        &mut self,
        resource: NativeResource,
        worker: &NativeRuntimeWorker,
        history_commit: HistoryCommit,
        page_navigation_handoffs: usize,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if is_network_url(&resource.url) {
            self.commit_content_process().await?;
        } else {
            self.content_process.take();
        }
        if self.is_same_document_navigation(&resource.url) {
            self.commit_same_document_navigation_async(
                resource.url,
                history_commit,
                worker,
                page_navigation_handoffs,
            )
            .await?;
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            if let Some(page_navigation) = self
                .commit_navigation_async(prepared, worker, history_commit)
                .await?
            {
                if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                    return Err(NativeEngineError::limit(
                        "page navigation handoffs",
                        MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                        page_navigation_handoffs.saturating_add(1),
                    ));
                }
                return Box::pin(
                    self.navigate_request_async(page_navigation, page_navigation_handoffs + 1),
                )
                .await;
            }
        }
        Ok(self.snapshot_unchecked())
    }

    fn navigate_content(
        &mut self,
        content: NativeContentLoad,
        history_commit: HistoryCommit,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&content.url) {
            if let Some(navigation) =
                self.commit_same_document_navigation(content.url, history_commit)?
            {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else if !self.allows_frame_navigation(&content.url)? {
            return Ok(self.snapshot_unchecked());
        } else {
            let prepared = self.prepare_navigation_content(content)?;
            let _ = self.commit_navigation(prepared, history_commit)?;
        }
        Ok(self.snapshot_unchecked())
    }

    async fn navigate_content_async(
        &mut self,
        content: NativeContentLoad,
        worker: &NativeRuntimeWorker,
        history_commit: HistoryCommit,
        page_navigation_handoffs: usize,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        let same_document = self.is_same_document_navigation(&content.url);
        if !same_document && !self.allows_frame_navigation(&content.url)? {
            return Ok(self.snapshot_unchecked());
        }
        if same_document {
            self.commit_same_document_navigation_async(
                content.url,
                history_commit,
                worker,
                page_navigation_handoffs,
            )
            .await?;
        } else {
            let prepared = self.prepare_navigation_content(content)?;
            let _ = self
                .commit_navigation_async(prepared, worker, history_commit)
                .await?;
            if let Some(page_navigation) = self.dispatch_content_page_show_async().await? {
                if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                    return Err(NativeEngineError::limit(
                        "page navigation handoffs",
                        MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                        page_navigation_handoffs.saturating_add(1),
                    ));
                }
                return Box::pin(self.navigate_script_navigation_async(
                    page_navigation,
                    page_navigation_handoffs + 1,
                ))
                .await
                .map(|_| self.snapshot_unchecked());
            }
        }
        Ok(self.snapshot_unchecked())
    }

    async fn dispatch_navigation_lifecycle_async(
        &mut self,
    ) -> Result<(bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let mut navigation = None;
        let allowed = if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            let (allowed, next_navigation) = self.dispatch_content_before_unload_async().await?;
            navigation = next_navigation;
            allowed
        } else if self.javascript.is_some() {
            let (allowed, next_navigation) = self.dispatch_local_before_unload()?;
            navigation = next_navigation;
            allowed
        } else {
            true
        };
        if !allowed {
            return Ok((false, None));
        }
        let events = [NativeEventKind::PageHide, NativeEventKind::Unload];
        if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            if let Some(next_navigation) = self.dispatch_content_events_async(&events).await? {
                if navigation.is_some() {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "multiple outgoing lifecycle navigations are not supported".into(),
                    });
                }
                navigation = Some(next_navigation);
            }
        } else if self.javascript.is_some()
            && let Some(next_navigation) = self.dispatch_local_navigation_lifecycle()?
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple outgoing lifecycle navigations are not supported".into(),
                });
            }
            navigation = Some(next_navigation);
        }
        self.persist_local_web_storage()?;
        Ok((true, navigation))
    }

    async fn dispatch_content_before_unload_async(
        &mut self,
    ) -> Result<(bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let mutation = {
            let Some(process) = self.content_process.as_mut() else {
                return Ok((true, None));
            };
            if !process.refresh_health() {
                return Ok((true, None));
            }
            process.dispatch_before_unload().await?
        };
        let navigation = mutation
            .navigation
            .clone()
            .map(|navigation| self.content_navigation_request(navigation))
            .transpose()?;
        let allowed = mutation.allowed;
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        Ok((allowed, navigation.filter(|_| allowed)))
    }

    async fn dispatch_content_events_async(
        &mut self,
        events: &[NativeEventKind],
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let mutation = {
            let Some(process) = self.content_process.as_mut() else {
                return Ok(None);
            };
            if !process.refresh_health() {
                return Ok(None);
            }
            process.dispatch_lifecycle_events(events).await?
        };
        let navigation = mutation
            .navigation
            .clone()
            .map(|navigation| self.content_navigation_request(navigation))
            .transpose()?;
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        Ok(navigation)
    }

    async fn dispatch_content_page_show_async(
        &mut self,
    ) -> Result<Option<NativeContentNavigation>, NativeEngineError> {
        let Some(process) = self.content_process.as_mut() else {
            return Ok(None);
        };
        if !process.refresh_health() {
            return Ok(None);
        }
        let mutation = process
            .dispatch_lifecycle_events(&[NativeEventKind::PageShow])
            .await?;
        let navigation = mutation.navigation.clone();
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        Ok(navigation)
    }

    async fn dispatch_content_hash_change_async(
        &mut self,
        old_url: &str,
        new_url: &str,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let mutation = {
            let Some(process) = self.content_process.as_mut() else {
                return Ok(None);
            };
            if !process.refresh_health() {
                return Ok(None);
            }
            process.dispatch_hash_change(old_url, new_url).await?
        };
        let navigation = mutation
            .navigation
            .clone()
            .map(|navigation| self.content_navigation_request(navigation))
            .transpose()?;
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        Ok(navigation)
    }

    /// Move to the previous bounded local history entry.
    ///
    /// `None` is an explicit boundary no-op and leaves the engine unchanged.
    pub fn go_back(&mut self) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history(NativeHistoryDirection::Back, "go back")
    }

    /// Move to the next bounded local history entry.
    ///
    /// `None` is an explicit boundary no-op and leaves the engine unchanged.
    pub fn go_forward(&mut self) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history(NativeHistoryDirection::Forward, "go forward")
    }

    /// Move to the previous history entry through the asynchronous native
    /// navigation owner. Network entries are loaded by the sandboxed content
    /// process; local entries use the deterministic loader.
    pub async fn go_back_async(
        &mut self,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history_async(NativeHistoryDirection::Back, "go back")
            .await
    }

    /// Move to the next history entry through the asynchronous native
    /// navigation owner. A missing forward entry is an explicit boundary
    /// no-op.
    pub async fn go_forward_async(
        &mut self,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history_async(NativeHistoryDirection::Forward, "go forward")
            .await
    }

    pub fn snapshot(&self) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("evidence")?;
        Ok(self.snapshot_unchecked())
    }

    /// Execute one bounded native GET/fetch request from the current external
    /// document. This is the first executable consumer of the shared
    /// connect/CSP/CORS policy; it is deliberately narrower than the eventual
    /// JavaScript Fetch/Web IDL surface and accepts no custom headers or body.
    pub async fn fetch_async(
        &mut self,
        href: impl Into<String>,
        credentials: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        self.require_running("fetch")?;
        if !is_network_url(&self.url) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native fetch requires an HTTP(S) document".into(),
            });
        }
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let href = href.into();
        self.ensure_content_process().await?;
        self.request_ledger.begin()?;
        let result = match self.content_process.as_mut() {
            Some(process) => process.fetch(&self.url, &href, credentials).await,
            None => Err(NativeEngineError::Worker {
                operation: "content process fetch".into(),
                reason: "native content process is not running".into(),
            }),
        };
        self.request_ledger.finish();
        result
    }

    /// Evaluate bounded ECMAScript in the current page realm. Network
    /// documents execute in the child-owned content process; local documents
    /// use the same runtime implementation in the engine owner.
    pub async fn evaluate_async(
        &mut self,
        source: impl Into<String>,
    ) -> Result<serde_json::Value, NativeEngineError> {
        self.evaluate_page_with_events_async(source.into(), NativePageEventBatch::default())
            .await
    }

    async fn evaluate_page_with_events_async(
        &mut self,
        source: String,
        mut page_events: NativePageEventBatch,
    ) -> Result<serde_json::Value, NativeEngineError> {
        self.require_running("script")?;
        self.sync_external_storage_events()?;
        if self.content_process.is_some() {
            self.deliver_pending_external_storage_events().await?;
            if let Some(process) = self.content_process.as_mut() {
                process
                    .sync_frame_script_context(self.frame_script_context.as_ref())
                    .await?;
                process
                    .sync_frame_script_bindings(&self.frame_script_bindings)
                    .await?;
                process.sync_scroll_offset(self.scroll_offset).await?;
                process
                    .sync_nested_scroll_offsets(&self.nested_scroll_offsets)
                    .await?;
            }
            let NativeContentScriptResult {
                value,
                mutation,
                storage_events,
                indexed_db_changes,
                dialogs,
                popups,
                post_messages,
                window_closes,
                window_navigations,
                service_worker_client_messages,
                service_worker_open_windows,
                service_worker_fetch_resumed: _,
                frame_scripts,
                window_name,
                mut history,
            } = {
                let process = self
                    .content_process
                    .as_mut()
                    .expect("content process presence was checked");
                if !process.refresh_health() {
                    return Err(NativeEngineError::worker_failure(
                        "content process script",
                        process
                            .failure_kind()
                            .unwrap_or(NativeWorkerFailureKind::Exited),
                        "content process is unavailable after a failed operation; navigate to recover it",
                    ));
                }
                self.request_ledger.begin()?;
                let result = process
                    .evaluate_with_page_events(&source, &page_events)
                    .await;
                self.request_ledger.finish();
                result?
            };
            self.config.window_name = window_name;
            self.queue_frame_script_requests(frame_scripts)?;
            self.queue_service_worker_client_messages(service_worker_client_messages)?;
            self.queue_service_worker_open_window_requests(service_worker_open_windows)?;
            let mut history_traversal = None;
            let mutation_history = mutation
                .as_ref()
                .map(|mutation| mutation.history.clone())
                .unwrap_or_default();
            history.extend(mutation_history);
            if let Some(mutation) = mutation {
                let navigation = mutation.navigation.clone();
                self.apply_content_process_mutation(mutation)?;
                history_traversal = self.apply_content_history_commands(&history)?;
                if let Some(navigation) = navigation {
                    self.navigate_script_navigation_async(navigation, 0).await?;
                }
            } else {
                self.publish_content_state(&storage_events, &indexed_db_changes)?;
                self.queue_popup_requests(popups)?;
                self.queue_post_message_requests(post_messages)?;
                self.queue_window_close_requests(window_closes)?;
                self.queue_window_navigation_requests(window_navigations)?;
                let dialog_url = self.url.clone();
                self.install_dialogs(dialogs, &dialog_url)?;
            }
            if !history.is_empty() {
                self.sync_content_history_async().await?;
            }
            if let Some(delta) = history_traversal
                && delta != 0
            {
                self.traverse_history_delta_async(delta).await?;
            }
            return Ok(value);
        }
        if self.javascript.is_none() {
            let javascript = NativeJavaScriptRuntime::new_with_context_metadata(
                &self.config.context_id,
                &self.config.window_name,
                self.config.opener_context_id.as_deref(),
                &self.config.opener_window_name,
                &self.config.opener_url,
            )?;
            javascript.set_storage_state(self.web_storage.clone());
            javascript.set_indexed_db_state(
                self.indexed_db
                    .origin(&storage_key(&self.url, &self.origin)),
            );
            javascript.set_frame_script_bindings(self.frame_script_bindings.clone());
            javascript.set_frame_script_context(self.frame_script_context.clone());
            javascript.set_frame_id(self.frame_id.clone());
            javascript.set_environment(self.environment.clone());
            javascript.set_scroll_offset(self.scroll_offset);
            javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
            self.javascript = Some(javascript);
        }
        self.deliver_pending_external_storage_events().await?;
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_environment(self.environment.clone());
        }
        let cookie = self.loader.document_cookie(&self.url)?;
        self.javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .set_cookie_state(cookie);
        self.sync_javascript_scroll_offset();
        self.sync_javascript_history();
        let initial_worker_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_worker_commands();
        let owner_url = self.url.clone();
        self.workers
            .apply_commands(initial_worker_commands, &mut self.loader, &owner_url)
            .await?;
        let initial_message_port_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_message_port_commands();
        self.workers
            .apply_page_message_port_commands(initial_message_port_commands, &mut self.loader)
            .await?;
        self.workers.run_due_timers(&mut self.loader).await?;
        page_events
            .worker_messages
            .extend(self.workers.take_messages());
        page_events
            .message_port_messages
            .extend(std::mem::take(&mut self.pending_message_port_messages));
        page_events
            .message_port_messages
            .extend(self.workers.take_message_port_messages());
        let evaluation = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .evaluate_with_page_events(
                &source,
                &self.document,
                &self.url,
                &self.origin,
                self.config.viewport,
                &page_events,
            )?;
        let worker_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_worker_commands();
        let owner_url = self.url.clone();
        self.workers
            .apply_commands(worker_commands, &mut self.loader, &owner_url)
            .await?;
        let message_port_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_message_port_commands();
        self.workers
            .apply_page_message_port_commands(message_port_commands, &mut self.loader)
            .await?;
        self.pending_message_port_messages
            .extend(self.workers.take_message_port_messages());
        if !self.workers.take_websocket_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker WebSocket requires a process-backed HTTP(S) document".into(),
            });
        }
        if !self.workers.take_event_source_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker EventSource requires a process-backed HTTP(S) document".into(),
            });
        }
        if evaluation.top_level_await_pending {
            return Err(NativeEngineError::Worker {
                operation: "evaluate JavaScript".into(),
                reason: "top-level await remained pending without a native host operation".into(),
            });
        }
        self.drain_local_popups()?;
        let navigation = self.apply_local_script_commands(&evaluation.commands, true)?;
        let dynamic_worker_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_worker_commands();
        let owner_url = self.url.clone();
        self.workers
            .apply_commands(dynamic_worker_commands, &mut self.loader, &owner_url)
            .await?;
        let dynamic_message_port_commands = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_message_port_commands();
        self.workers
            .apply_page_message_port_commands(dynamic_message_port_commands, &mut self.loader)
            .await?;
        self.pending_message_port_messages
            .extend(self.workers.take_message_port_messages());
        if !self.workers.take_websocket_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker WebSocket requires a process-backed HTTP(S) document".into(),
            });
        }
        if !self.workers.take_event_source_commands().is_empty() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "worker EventSource requires a process-backed HTTP(S) document".into(),
            });
        }
        let frame_scripts = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime initialized")
            .take_frame_script_events();
        self.queue_frame_script_requests(frame_scripts)?;
        self.drain_local_dialogs()?;
        self.persist_local_web_storage()?;
        if let Some(navigation) = navigation {
            self.navigate_request_async(navigation, 0).await?;
        }
        Ok(evaluation.value)
    }

    pub(crate) async fn sync_window_proxies(
        &mut self,
        updates: &[NativeWindowProxyUpdate],
    ) -> Result<(), NativeEngineError> {
        self.require_running("WindowProxy synchronization")?;
        if updates.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native WindowProxy updates",
                MAX_NATIVE_EFFECTS,
                updates.len(),
            ));
        }
        if let Some(process) = self.content_process.as_mut() {
            process.sync_window_proxies(updates).await
        } else if let Some(javascript) = self.javascript.as_ref() {
            javascript.sync_window_proxies(updates)
        } else {
            Ok(())
        }
    }

    /// Read or mutate the page-visible Web Storage owned by this engine.
    ///
    /// Semantic backend storage must use the same origin-keyed state as the
    /// JavaScript realm. Keeping a second adapter-owned map would make a
    /// successful `storage` operation invisible to page script (and vice
    /// versa), which is a correctness failure for a browser backend.
    pub async fn storage_async(
        &mut self,
        scope: StorageScope,
        operation: StorageOperation,
    ) -> Result<std::collections::BTreeMap<String, String>, NativeEngineError> {
        self.require_running("storage")?;
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;

        if matches!(&scope, StorageScope::Cookies) {
            return match operation {
                StorageOperation::Read => {
                    let cookies = self.cookies_async().await?;
                    let mut entries = std::collections::BTreeMap::new();
                    for cookie in cookies {
                        entries.entry(cookie.name).or_insert(cookie.value);
                    }
                    Ok(entries)
                }
                StorageOperation::Clear => {
                    self.clear_cookies_async().await?;
                    Ok(std::collections::BTreeMap::new())
                }
                StorageOperation::Write { key, value } => {
                    let cookie_line = format!("{key}={value}");
                    if !is_network_url(&self.url) {
                        return Err(NativeEngineError::UnsupportedUrl {
                            reason: "semantic cookie writes require an HTTP(S) document".into(),
                        });
                    }
                    if self.content_process.is_some() {
                        let cookie_line = serde_json::to_string(&cookie_line).map_err(|_| {
                            NativeEngineError::Worker {
                                operation: "native cookie write".into(),
                                reason: "cookie assignment could not be encoded".into(),
                            }
                        })?;
                        self.evaluate_async(format!("document.cookie = {cookie_line}; true"))
                            .await?;
                    } else {
                        self.loader.set_document_cookie(&self.url, &cookie_line)?;
                        self.persist_local_web_storage()?;
                    }
                    let cookies = self.cookies_async().await?;
                    let mut entries = std::collections::BTreeMap::new();
                    for cookie in cookies {
                        entries.entry(cookie.name).or_insert(cookie.value);
                    }
                    Ok(entries)
                }
            };
        }

        let storage_name = match scope {
            StorageScope::Local => "localStorage",
            StorageScope::Session => "sessionStorage",
            StorageScope::Cookies => unreachable!("cookies are handled above"),
        };

        match operation {
            StorageOperation::Read => {}
            StorageOperation::Write { key, value } => {
                let key = serde_json::to_string(&key).map_err(|_| NativeEngineError::Worker {
                    operation: "native storage write".into(),
                    reason: "storage key could not be encoded".into(),
                })?;
                let value =
                    serde_json::to_string(&value).map_err(|_| NativeEngineError::Worker {
                        operation: "native storage write".into(),
                        reason: "storage value could not be encoded".into(),
                    })?;
                self.evaluate_async(format!(
                    "window.{storage_name}.setItem({key}, {value}); true"
                ))
                .await?;
            }
            StorageOperation::Clear => {
                self.evaluate_async(format!("window.{storage_name}.clear(); true"))
                    .await?;
            }
        }

        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let storage_key = storage_key(&self.url, &self.origin);
        Ok(self.web_storage.entries_for(
            match scope {
                StorageScope::Local => "local",
                StorageScope::Session => "session",
                StorageScope::Cookies => unreachable!("cookies are handled above"),
            },
            &storage_key,
        ))
    }

    /// Return cookies matching the current document URL, including HTTP-only
    /// entries that are not visible to `document.cookie`.
    pub async fn cookies_async(&mut self) -> Result<Vec<Cookie>, NativeEngineError> {
        self.require_running("cookies")?;
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let profiles = if let Some(process) = self.content_process.as_mut() {
            process.cookies(&self.url).await?
        } else {
            self.loader.cookies_for_document(&self.url)?
        };
        profiles
            .into_iter()
            .map(public_cookie_from_profile)
            .collect()
    }

    /// Import cookies into the current native profile and refresh the active
    /// content realm. Cookie attributes are retained rather than flattened
    /// into a name/value map.
    pub async fn set_cookies_async(&mut self, cookies: &[Cookie]) -> Result<(), NativeEngineError> {
        self.require_running("set cookies")?;
        if cookies.len() > super::javascript::MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native cookie profile entries",
                super::javascript::MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                cookies.len(),
            ));
        }
        let profiles = cookies
            .iter()
            .map(profile_from_public_cookie)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(process) = self.content_process.as_mut() {
            process.set_cookies(&profiles).await?;
        }
        self.loader.set_cookie_profiles(&profiles)?;
        if self.content_process.is_none() {
            self.persist_local_web_storage()?;
        }
        Ok(())
    }

    /// Clear all cookies in the native profile and active content realm.
    pub async fn clear_cookies_async(&mut self) -> Result<(), NativeEngineError> {
        self.require_running("clear cookies")?;
        if let Some(process) = self.content_process.as_mut() {
            process.clear_cookies().await?;
        }
        self.loader.clear_cookies();
        if self.content_process.is_none() {
            self.persist_local_web_storage()?;
        }
        Ok(())
    }

    /// Return the oldest unresolved JavaScript dialog for the active page.
    pub fn pending_dialog(&self) -> Result<Option<PendingDialog>, NativeEngineError> {
        self.require_running("inspect dialog")?;
        Ok(self.pending_dialogs.front().cloned())
    }

    /// Report whether the native request owner has remained idle for the
    /// caller's bounded quiet interval.
    pub fn network_quiet(&self, duration: Duration) -> Result<(bool, String), NativeEngineError> {
        self.require_running("inspect network activity")?;
        Ok(self.request_ledger.quiet(duration))
    }

    /// Return the bounded native download IDs visible to the browser backend.
    pub fn download_ids(&self) -> Result<Vec<String>, NativeEngineError> {
        self.require_running("list downloads")?;
        Ok(self
            .pending_downloads
            .iter()
            .map(|download| download.guid.clone())
            .chain(self.completed_download_ids.iter().cloned())
            .collect())
    }

    /// Cancel one queued native download before its bytes are fetched.
    pub fn cancel_download(&mut self, download_id: &str) -> Result<bool, NativeEngineError> {
        self.require_running("cancel download")?;
        if download_id.is_empty()
            || download_id.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES
        {
            return Err(NativeEngineError::invalid(
                "download id",
                "must be a non-empty bounded value",
            ));
        }
        let before = self.pending_downloads.len();
        self.pending_downloads
            .retain(|download| download.guid != download_id);
        Ok(self.pending_downloads.len() != before)
    }

    /// Complete the oldest queued native download into an already-authorized
    /// directory. The click action only queues the request, which keeps the
    /// action boundary responsive and lets the caller choose its destination.
    pub async fn wait_for_download_async(
        &mut self,
        destination: &Path,
        deadline: Duration,
    ) -> Result<DownloadOutcome, NativeEngineError> {
        self.require_running("download")?;
        if deadline.is_zero() || deadline > MAX_NATIVE_DOWNLOAD_DEADLINE {
            return Err(NativeEngineError::invalid(
                "download deadline",
                "must be between 1 ms and 30 seconds",
            ));
        }
        if !std::fs::metadata(destination).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(NativeEngineError::invalid(
                "download destination",
                "must be an existing directory",
            ));
        }
        let Some(pending) = self.pending_downloads.pop_front() else {
            return Err(NativeEngineError::Network {
                operation: "download".into(),
                reason: "no native download is pending; activate a download link first".into(),
            });
        };
        self.request_ledger.begin()?;
        let response = match tokio::time::timeout(
            deadline,
            self.loader.download_async(&self.url, &pending.url),
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                self.pending_downloads.push_front(pending);
                self.request_ledger.finish();
                return Err(error);
            }
            Err(_) => {
                self.pending_downloads.push_front(pending);
                self.request_ledger.finish();
                return Err(NativeEngineError::Network {
                    operation: "download".into(),
                    reason: "download exceeded its deadline".into(),
                });
            }
        };
        self.request_ledger.finish();
        if !(200..=299).contains(&response.status) {
            self.pending_downloads.push_front(pending);
            return Err(NativeEngineError::Network {
                operation: "download".into(),
                reason: format!("server returned HTTP {}", response.status),
            });
        }
        match write_native_download(destination, &pending.suggested_filename, &response.body).await
        {
            Ok(_) => {}
            Err(error) => {
                self.pending_downloads.push_front(pending);
                return Err(error);
            }
        };
        let mut digest = Sha256::new();
        digest.update(&response.body);
        let sha256 = format!("{:x}", digest.finalize());
        self.completed_downloads = self.completed_downloads.saturating_add(1);
        self.completed_download_ids.push_back(pending.guid.clone());
        while self.completed_download_ids.len() > MAX_NATIVE_COMPLETED_DOWNLOAD_IDS {
            self.completed_download_ids.pop_front();
        }
        Ok(DownloadOutcome {
            guid: pending.guid,
            suggested_filename: pending.suggested_filename,
            state: "completed".into(),
            received_bytes: response.body.len() as u64,
            total_bytes: response.body.len() as u64,
            target_id: pending.target_id,
            frame_id: pending.frame_id,
            sha256: Some(sha256),
        })
    }

    /// Number of native downloads that completed in this engine instance.
    pub fn completed_download_count(&self) -> Result<u64, NativeEngineError> {
        self.require_running("inspect downloads")?;
        Ok(self.completed_downloads)
    }

    fn queue_download(
        &mut self,
        url: String,
        download_attribute: &str,
    ) -> Result<(), NativeEngineError> {
        if self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS {
            return Err(NativeEngineError::limit(
                "native pending downloads",
                MAX_NATIVE_PENDING_DOWNLOADS,
                self.pending_downloads.len().saturating_add(1),
            ));
        }
        let guid = format!("native-download-{}", self.next_download_id);
        self.next_download_id = self.next_download_id.saturating_add(1);
        let suggested_filename = native_download_filename(download_attribute, &url);
        self.pending_downloads.push_back(NativePendingDownload {
            guid,
            url,
            suggested_filename,
            target_id: self.config.context_id.clone(),
            frame_id: self.frame_id.clone(),
        });
        Ok(())
    }

    fn queue_popup(&mut self, url: String) -> Result<(), NativeEngineError> {
        self.queue_popup_request(NativePopupRequest {
            url,
            target: "_blank".into(),
            handle: None,
            source_context_id: String::new(),
        })
    }

    fn queue_popup_request(
        &mut self,
        mut popup: NativePopupRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_popups.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending popups",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_popups.len().saturating_add(1),
            ));
        }
        validate_url_text("popup target URL", &popup.url)?;
        validate_url_text("popup target name", &popup.target)?;
        if let Some(handle) = popup.handle.as_deref() {
            validate_url_text("popup window handle", handle)?;
        }
        if popup.source_context_id.is_empty() {
            popup.source_context_id = self.config.context_id.clone();
        } else {
            super::config::validate_context_id(&popup.source_context_id)?;
        }
        self.pending_popups.push_back(popup);
        Ok(())
    }

    fn queue_popup_requests(
        &mut self,
        popups: Vec<NativePopupRequest>,
    ) -> Result<(), NativeEngineError> {
        for popup in popups {
            self.queue_popup_request(popup)?;
        }
        Ok(())
    }

    fn queue_post_message_request(
        &mut self,
        mut message: NativePostMessageRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_post_messages.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending postMessage requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_post_messages.len().saturating_add(1),
            ));
        }
        if message.target.is_empty() && message.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "postMessage target",
                "must not be empty without a direct target context",
            ));
        }
        if !message.target.is_empty() {
            validate_url_text("postMessage target", &message.target)?;
        }
        validate_url_text("postMessage target origin", &message.target_origin)?;
        if let Some(target_context_id) = message.target_context_id.as_deref() {
            super::config::validate_context_id(target_context_id)?;
        }
        let encoded = serde_json::to_vec(&message.data).map_err(|_| NativeEngineError::Worker {
            operation: "queue native postMessage".into(),
            reason: "postMessage data could not be serialized".into(),
        })?;
        if encoded.len() > super::javascript::MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native postMessage data",
                super::javascript::MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
        if message.source_context_id.is_empty() {
            message.source_context_id = self.config.context_id.clone();
        } else {
            super::config::validate_context_id(&message.source_context_id)?;
        }
        if message.source_origin.is_empty() {
            message.source_origin = self.origin.serialized();
        } else {
            validate_url_text("postMessage source origin", &message.source_origin)?;
        }
        self.pending_post_messages.push_back(message);
        Ok(())
    }

    fn queue_post_message_requests(
        &mut self,
        messages: Vec<NativePostMessageRequest>,
    ) -> Result<(), NativeEngineError> {
        for message in messages {
            self.queue_post_message_request(message)?;
        }
        Ok(())
    }

    fn queue_post_message_requests_from_origin(
        &mut self,
        messages: Vec<NativePostMessageRequest>,
        origin: &NativeOrigin,
    ) -> Result<(), NativeEngineError> {
        for mut message in messages {
            if message.source_origin.is_empty() {
                message.source_origin = origin.serialized();
            }
            self.queue_post_message_request(message)?;
        }
        Ok(())
    }

    fn queue_window_close_request(
        &mut self,
        mut request: NativeWindowCloseRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_window_closes.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending window close requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_window_closes.len().saturating_add(1),
            ));
        }
        validate_url_text("window close target", &request.target)?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            super::config::validate_context_id(target_context_id)?;
        }
        if request.source_context_id.is_empty() {
            request.source_context_id = self.config.context_id.clone();
        } else {
            super::config::validate_context_id(&request.source_context_id)?;
        }
        self.pending_window_closes.push_back(request);
        Ok(())
    }

    fn queue_window_close_requests(
        &mut self,
        requests: Vec<NativeWindowCloseRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            self.queue_window_close_request(request)?;
        }
        Ok(())
    }

    fn queue_window_navigation_request(
        &mut self,
        mut request: NativeWindowNavigationRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_window_navigations.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending window navigation requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_window_navigations.len().saturating_add(1),
            ));
        }
        if request.target.is_empty() && request.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "window navigation target",
                "must not be empty without a direct target context",
            ));
        }
        if !request.target.is_empty() {
            validate_url_text("window navigation target", &request.target)?;
        }
        validate_url_text("window navigation href", &request.href)?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            super::config::validate_context_id(target_context_id)?;
        }
        if request.source_context_id.is_empty() {
            request.source_context_id = self.config.context_id.clone();
        } else {
            super::config::validate_context_id(&request.source_context_id)?;
        }
        self.pending_window_navigations.push_back(request);
        Ok(())
    }

    fn queue_window_navigation_requests(
        &mut self,
        requests: Vec<NativeWindowNavigationRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            self.queue_window_navigation_request(request)?;
        }
        Ok(())
    }

    fn queue_service_worker_open_window_request(
        &mut self,
        mut request: NativeServiceWorkerOpenWindowRequest,
    ) -> Result<(), NativeEngineError> {
        if self.pending_service_worker_open_windows.len() >= MAX_NATIVE_PENDING_POPUPS {
            return Err(NativeEngineError::limit(
                "native pending service worker openWindow requests",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_service_worker_open_windows
                    .len()
                    .saturating_add(1),
            ));
        }
        if request.request_id == 0 || request.worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "service worker openWindow request",
                "request and worker ids must be positive",
            ));
        }
        validate_url_text("service worker openWindow URL", &request.url)?;
        if request.source_context_id.is_empty() {
            request.source_context_id = self.config.context_id.clone();
        } else {
            super::config::validate_context_id(&request.source_context_id)?;
        }
        if request.source_frame_id.is_empty() {
            request.source_frame_id = self.frame_id.clone();
        } else {
            super::config::validate_context_id(&request.source_frame_id)?;
        }
        self.pending_service_worker_open_windows.push_back(request);
        Ok(())
    }

    fn queue_service_worker_open_window_requests(
        &mut self,
        requests: Vec<NativeServiceWorkerOpenWindowRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            self.queue_service_worker_open_window_request(request)?;
        }
        Ok(())
    }

    fn queue_service_worker_client_messages(
        &mut self,
        messages: Vec<NativeServiceWorkerClientMessage>,
    ) -> Result<(), NativeEngineError> {
        if self
            .pending_service_worker_client_messages
            .len()
            .saturating_add(messages.len())
            > MAX_NATIVE_PENDING_POPUPS
        {
            return Err(NativeEngineError::limit(
                "native pending service worker client messages",
                MAX_NATIVE_PENDING_POPUPS,
                self.pending_service_worker_client_messages
                    .len()
                    .saturating_add(messages.len()),
            ));
        }
        for message in messages {
            if message.worker_id == 0 {
                return Err(NativeEngineError::invalid(
                    "service worker client message worker id",
                    "must be positive",
                ));
            }
            if message.client_id.is_empty() {
                return Err(NativeEngineError::invalid(
                    "service worker client message id",
                    "must not be empty",
                ));
            }
            self.pending_service_worker_client_messages
                .push_back(message);
        }
        Ok(())
    }

    fn queue_frame_script_requests(
        &mut self,
        requests: Vec<NativeFrameScriptRequest>,
    ) -> Result<(), NativeEngineError> {
        for request in requests {
            if self.pending_frame_scripts.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native pending frame script requests",
                    MAX_NATIVE_EFFECTS,
                    self.pending_frame_scripts.len().saturating_add(1),
                ));
            }
            validate_context_id(&request.frame_id)?;
            validate_context_id(&request.source_frame_id)?;
            if matches!(
                request.command.as_ref(),
                NativeScriptCommand::FrameScript { .. }
            ) {
                return Err(NativeEngineError::invalid(
                    "native frame script command",
                    "nested frame script commands are not allowed",
                ));
            }
            self.pending_frame_scripts.push_back(request);
        }
        Ok(())
    }

    pub(crate) fn take_pending_popups(&mut self) -> Vec<NativePopupRequest> {
        self.pending_popups.drain(..).collect()
    }

    pub(crate) fn take_pending_post_messages(&mut self) -> Vec<NativePostMessageRequest> {
        self.pending_post_messages.drain(..).collect()
    }

    pub(crate) fn take_pending_window_closes(&mut self) -> Vec<NativeWindowCloseRequest> {
        self.pending_window_closes.drain(..).collect()
    }

    pub(crate) fn take_pending_window_navigations(&mut self) -> Vec<NativeWindowNavigationRequest> {
        self.pending_window_navigations.drain(..).collect()
    }

    pub(crate) fn take_pending_service_worker_open_windows(
        &mut self,
    ) -> Vec<NativeServiceWorkerOpenWindowRequest> {
        self.pending_service_worker_open_windows.drain(..).collect()
    }

    pub(crate) fn take_pending_service_worker_client_messages(
        &mut self,
    ) -> Vec<NativeServiceWorkerClientMessage> {
        self.pending_service_worker_client_messages
            .drain(..)
            .collect()
    }

    pub(crate) fn take_pending_frame_scripts(&mut self) -> Vec<NativeFrameScriptRequest> {
        self.pending_frame_scripts.drain(..).collect()
    }

    /// Apply one already-authorized command emitted by a same-origin parent
    /// realm to this frame's own document owner. The command enters through
    /// the normal JavaScript host path so event listeners, navigation, and
    /// child effects retain their ordinary ordering.
    pub(crate) async fn apply_frame_script_command_async(
        &mut self,
        command: NativeScriptCommand,
    ) -> Result<(), NativeEngineError> {
        validate_frame_script_command(&command)?;
        let mut page_events = NativePageEventBatch::default();
        page_events.frame_script_commands.push(command);
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub(crate) async fn apply_frame_script_command_with_effects_async(
        &mut self,
        command: NativeScriptCommand,
    ) -> Result<Vec<NativeEffect>, NativeEngineError> {
        let previous_revision = self.revision();
        self.apply_frame_script_command_async(command).await?;
        Ok(self.effects_since(previous_revision)?.effects)
    }

    /// Deliver events observed by a child engine to the parent realm's
    /// same-origin projection. The backend supplies only typed effect records;
    /// the parent runtime resolves node identity from its current binding.
    pub(crate) async fn dispatch_frame_events_async(
        &mut self,
        frame_id: &str,
        effects: &[NativeEffect],
    ) -> Result<(), NativeEngineError> {
        let metadata = effects
            .iter()
            .map(|effect| {
                (
                    effect.node_id.index(),
                    effect.node_id.generation(),
                    effect.kind,
                )
            })
            .collect::<Vec<_>>();
        let Some(batch) = frame_event_batch(frame_id, &metadata)? else {
            return Ok(());
        };
        let mut page_events = NativePageEventBatch::default();
        page_events.frame_event_batches.push(batch);
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub(crate) async fn dispatch_post_message(
        &mut self,
        source_context_id: &str,
        source_origin: &str,
        data: &serde_json::Value,
    ) -> Result<(), NativeEngineError> {
        self.require_running("message event")?;
        let mut page_events = NativePageEventBatch::default();
        page_events
            .post_message_events
            .push(NativePageMessageEvent {
                source_context_id: source_context_id.to_owned(),
                source_origin: source_origin.to_owned(),
                data: data.clone(),
            });
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    /// Resolve one queued JavaScript dialog without creating a browser or
    /// routing through CDP. The page realm's bounded alert/confirm/prompt
    /// result is deterministic for the current script batch; resolution
    /// removes the user-facing prompt and lets the next queued prompt surface.
    pub fn resolve_dialog(
        &mut self,
        _decision: PromptDecision,
    ) -> Result<PromptResult, NativeEngineError> {
        self.require_running("resolve dialog")?;
        Ok(PromptResult {
            handled: self.pending_dialogs.pop_front().is_some(),
        })
    }

    fn install_dialogs(
        &mut self,
        dialogs: impl IntoIterator<Item = NativeDialog>,
        document_url: &str,
    ) -> Result<(), NativeEngineError> {
        for dialog in dialogs {
            if self.pending_dialogs.len() >= MAX_NATIVE_DIALOGS {
                return Err(NativeEngineError::limit(
                    "native dialogs",
                    MAX_NATIVE_DIALOGS,
                    self.pending_dialogs.len().saturating_add(1),
                ));
            }
            if !matches!(dialog.dialog_type.as_str(), "alert" | "confirm" | "prompt") {
                return Err(NativeEngineError::invalid(
                    "native dialog type",
                    "must be alert, confirm, or prompt",
                ));
            }
            if dialog.message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES
                || dialog
                    .default_value
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
            {
                return Err(NativeEngineError::limit(
                    "native dialog text",
                    MAX_NATIVE_DIALOG_TEXT_BYTES,
                    dialog
                        .default_value
                        .as_ref()
                        .map_or(dialog.message.len(), String::len),
                ));
            }
            self.pending_dialogs.push_back(PendingDialog {
                dialog_type: dialog.dialog_type,
                message: dialog.message,
                default_value: dialog.default_value,
                url: document_url.to_owned(),
            });
        }
        Ok(())
    }

    /// Return diagnostics for CSS that the bounded native presentation model
    /// intentionally ignored or could not parse.
    pub fn diagnostics(&self) -> Result<NativeDiagnosticsSnapshot, NativeEngineError> {
        self.require_running("diagnostics")?;
        Ok(NativeDiagnosticsSnapshot {
            revision: self.revision,
            diagnostics: self.document.diagnostics().to_vec(),
            truncated: self.document.diagnostics_truncated(),
        })
    }

    pub fn semantic_nodes(&self) -> Result<Vec<super::dom::NativeSemanticNode>, NativeEngineError> {
        self.require_running("semantic DOM")?;
        Ok(self.document.semantic_nodes())
    }

    /// Capture the current native page, semantic nodes, and layout under one
    /// revision so agent-facing discovery cannot combine different states.
    pub fn inspection_snapshot(&self) -> Result<NativeInspectionSnapshot, NativeEngineError> {
        self.require_running("semantic inspection")?;
        Ok(NativeInspectionSnapshot {
            context_id: self.config.context_id.clone(),
            snapshot: self.snapshot()?,
            nodes: self.document.semantic_nodes(),
            layout: self.layout()?,
        })
    }

    /// Resolve one target and report actionability without scrolling, focus,
    /// event dispatch, navigation, or any other document mutation.
    pub fn preflight_target(
        &self,
        target: &str,
        action: NativePreflightAction,
    ) -> Result<NativeTargetPreflight, NativeEngineError> {
        self.require_running("target preflight")?;
        let unresolved = |error_kind| NativeTargetPreflight {
            action,
            unique: false,
            node: None,
            actionable: None,
            actionability_reason: None,
            error_kind: Some(error_kind),
            revision: self.revision,
            frame_id: None,
            geometry: None,
            likely_navigation: false,
            likely_popup: false,
            likely_form_submit: false,
        };
        let id = match self.document.resolve_target(target) {
            Ok(id) => id,
            Err(NativeEngineError::AmbiguousTarget { .. }) => {
                return Ok(unresolved(NativeTargetErrorKind::Ambiguous));
            }
            Err(NativeEngineError::TargetNotFound) => {
                return Ok(unresolved(NativeTargetErrorKind::NotFound));
            }
            Err(NativeEngineError::DetachedTarget) => {
                return Ok(unresolved(NativeTargetErrorKind::StaleReference));
            }
            Err(error) => return Err(error),
        };
        let Some(node) = self
            .document
            .semantic_nodes()
            .into_iter()
            .find(|node| node.node_id == id)
        else {
            return Ok(unresolved(NativeTargetErrorKind::StaleReference));
        };
        let geometry = self.layout()?.viewport_rect_for(id);
        let requires_geometry = !matches!(action, NativePreflightAction::Upload);
        let actionability_reason = if node.disabled {
            Some(NativeActionabilityReason::Disabled)
        } else if node.hidden && requires_geometry {
            Some(NativeActionabilityReason::NotVisible)
        } else if matches!(action, NativePreflightAction::Type) && node.read_only {
            Some(NativeActionabilityReason::ReadOnly)
        } else if requires_geometry && geometry.is_none() {
            Some(NativeActionabilityReason::OutsideViewport)
        } else if !native_action_supported(action, &node) {
            Some(NativeActionabilityReason::UnsupportedAction)
        } else {
            None
        };
        Ok(NativeTargetPreflight {
            action,
            unique: true,
            likely_navigation: self.document.link_href(id).is_some(),
            likely_popup: false,
            likely_form_submit: self.document.submit_control_form(id).is_some(),
            node: Some(node),
            actionable: Some(actionability_reason.is_none()),
            actionability_reason,
            error_kind: None,
            revision: self.revision,
            frame_id: None,
            geometry,
        })
    }

    pub(crate) fn resolve_target(&self, locator: &str) -> Result<NativeNodeId, NativeEngineError> {
        self.document.resolve_target(locator)
    }

    /// Return the current document's derived integer-pixel layout.
    pub fn layout(&self) -> Result<NativeLayoutSnapshot, NativeEngineError> {
        self.require_running("layout")?;
        self.layout_at_viewport(self.config.viewport, self.scroll_offset)
    }

    /// Derive layout for a capture viewport without changing the live engine
    /// viewport or scroll state. Full-page and element captures use this
    /// snapshot to render off-screen document content while keeping ordinary
    /// input and script coordinates tied to the configured viewport.
    pub(crate) fn layout_at_viewport(
        &self,
        viewport: Viewport,
        scroll_offset: NativePoint,
    ) -> Result<NativeLayoutSnapshot, NativeEngineError> {
        self.require_running("layout")?;
        self.document.layout(viewport)?.with_scroll_offset(
            &self.document,
            scroll_offset,
            &self.nested_scroll_offsets,
        )
    }

    /// Hit test one point in the configured viewport without scrolling or
    /// adjusting the requested coordinates.
    pub fn hit_test(
        &self,
        x: i64,
        y: i64,
    ) -> Result<Option<super::dom::NativeNodeId>, NativeEngineError> {
        self.require_running("hit testing")?;
        self.layout()?.hit_test(x, y)
    }

    pub(crate) fn is_descendant_or_self(
        &self,
        id: super::dom::NativeNodeId,
        ancestor: super::dom::NativeNodeId,
    ) -> Result<bool, NativeEngineError> {
        self.require_running("frame hit testing")?;
        Ok(self.document.is_descendant_or_self(id, ancestor))
    }

    /// Return the current document's immutable Rust display-list projection.
    pub fn display_list(&self) -> Result<NativeDisplayList, NativeEngineError> {
        self.require_running("display list")?;
        NativeDisplayList::build(&self.document, &self.layout()?)
    }

    /// Replay the current document's display list into a bounded Rust surface.
    pub fn rasterize(&self) -> Result<NativeSurface, NativeEngineError> {
        self.require_running("raster surface")?;
        self.display_list()?.rasterize()
    }

    /// Replay the current document into a temporary viewport. This is used by
    /// full-page and off-screen element capture and never mutates live engine
    /// geometry, scroll state, or script-visible viewport values.
    pub(crate) fn rasterize_at_viewport(
        &self,
        viewport: Viewport,
    ) -> Result<NativeSurface, NativeEngineError> {
        self.require_running("raster surface")?;
        let layout = self.layout_at_viewport(viewport, NativePoint { x: 0, y: 0 })?;
        NativeDisplayList::build(&self.document, &layout)?.rasterize()
    }

    /// Encode the current logical renderer surface as bounded PNG bytes.
    pub fn capture_png(&self) -> Result<Vec<u8>, NativeEngineError> {
        self.require_running("capture")?;
        self.rasterize()?.to_png()
    }

    /// Apply one semantic action and advance the document revision exactly
    /// once. Target resolution and actionability checks happen before state
    /// mutation, so rejected actions leave the document unchanged.
    pub fn action(
        &mut self,
        action: NativeAction,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.require_running("action")?;
        self.scroll_action_targets_into_view(&action)?;
        if let NativeAction::DoubleClick { target } = action {
            self.action(NativeAction::Click {
                target: target.clone(),
            })?;
            return self.action(NativeAction::Click { target });
        }
        let (events, accepted) = match action {
            NativeAction::DoubleClick { .. } => {
                unreachable!("double-click is handled before the native action match")
            }
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                self.require_layout_actionable(id)?;
                if let Some(href) = self.document.link_href(id).map(str::to_owned)
                    && !href.is_empty()
                {
                    if self.javascript.is_some() {
                        self.preflight_local_link_navigation(id, &href)?;
                        return self.action_local_click_with_event_preflight(id);
                    }
                    return self.activate_link(id, &href, false);
                }
                if let Some(form_id) = self.document.submit_control_form(id)
                    && self.javascript.is_none()
                {
                    return self.action_local_submit_click_without_script(id, form_id);
                }
                if self.javascript.is_some() {
                    return self.action_local_click_with_event_preflight(id);
                }
                (self.document.apply_click(id)?, true)
            }
            NativeAction::Hover { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_hover(id),
                        "native hover event effects",
                    );
                }
                (self.document.apply_hover(id)?, true)
            }
            NativeAction::Drag {
                source,
                destination,
            } => {
                let source_id = self.document.resolve_target(&source)?;
                let destination_id = self.document.resolve_target(&destination)?;
                self.require_layout_actionable(source_id)?;
                self.require_layout_actionable(destination_id)?;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_drag(source_id, destination_id),
                        "native drag event effects",
                    );
                }
                (self.document.apply_drag(source_id, destination_id)?, true)
            }
            NativeAction::Upload { target, files } => {
                let id = self.document.resolve_target(&target)?;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_upload(id, &files),
                        "native upload event effects",
                    );
                }
                (self.document.apply_upload(id, &files)?, true)
            }
            NativeAction::Type { target, text } => {
                let id = if target == "focused" {
                    self.document.focused_text_control()?
                } else {
                    self.document.resolve_target(&target)?
                };
                self.require_layout_actionable(id)?;
                if self.javascript.is_some() {
                    return self.action_local_type_with_event_transaction(id, &text);
                }
                (self.document.apply_type(id, &text)?, true)
            }
            NativeAction::Clear { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_clear(id),
                        "native clear event effects",
                    );
                }
                (self.document.apply_clear(id)?, true)
            }
            NativeAction::Check { target } => {
                return self.action_checked_with_click(target, true);
            }
            NativeAction::Uncheck { target } => {
                return self.action_checked_with_click(target, false);
            }
            NativeAction::Select { target, value } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_select(id, &value),
                        "native select event effects",
                    );
                }
                (self.document.apply_select(id, &value)?, true)
            }
            NativeAction::KeyDown { key } => {
                let id = self.document.focused_node();
                return self.action_local_key_event(id, &key, NativeEventKind::KeyDown, 0);
            }
            NativeAction::KeyUp { key } => {
                let id = self.document.focused_node();
                return self.action_local_key_event(id, &key, NativeEventKind::KeyUp, 0);
            }
            NativeAction::Shortcut { shortcut } => {
                let (modifiers, key) = parse_native_shortcut(&shortcut)?;
                let id = self.document.focused_node();
                return self.action_local_key_sequence(id, &key, modifiers, true);
            }
            NativeAction::KeyPress { key } => {
                validate_native_edit_key(&key)?;
                let id = self.document.focused_text_control()?;
                return self.action_local_key_sequence(id, &key, 0, true);
            }
            NativeAction::Scroll { delta_x, delta_y } => {
                let moved = self.apply_scroll(delta_x, delta_y)?;
                let events = moved
                    .then(|| {
                        (
                            NativeNodeId::from_parts(self.document.generation(), u32::MAX),
                            NativeEventKind::Scroll,
                        )
                    })
                    .into_iter()
                    .collect();
                (events, moved)
            }
        };
        if !accepted {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: false,
            });
        }
        let next_revision = self.next_revision()?;
        self.document.set_revision(next_revision);
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.record_effects(events.clone());
        self.dispatch_local_events(&events)?;
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    /// Apply an action through the child-owned document when the current
    /// navigation is process-backed. Navigation actions remain parent-owned so
    /// a link cannot mutate the child document without also committing a new
    /// resource and history entry.
    pub async fn action_async(
        &mut self,
        action: NativeAction,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.require_running("action")?;
        self.scroll_action_targets_into_view(&action)?;
        if let NativeAction::DoubleClick { target } = action {
            Box::pin(self.action_async(NativeAction::Click {
                target: target.clone(),
            }))
            .await?;
            return Box::pin(self.action_async(NativeAction::Click { target })).await;
        }
        self.sync_external_storage_events()?;
        if self.content_process.is_some() {
            self.deliver_pending_external_storage_events().await?;
        }
        let Some(process) = self.content_process.as_mut() else {
            return self.action(action);
        };
        if !process.refresh_health() {
            return Err(NativeEngineError::worker_failure(
                "content process mutation",
                process
                    .failure_kind()
                    .unwrap_or(NativeWorkerFailureKind::Exited),
                "content process is unavailable after a failed mutation; navigate to recover it",
            ));
        }
        self.content_process
            .as_mut()
            .expect("content process presence was checked")
            .sync_scroll_offset(self.scroll_offset)
            .await?;
        self.content_process
            .as_mut()
            .expect("content process presence was checked")
            .sync_nested_scroll_offsets(&self.nested_scroll_offsets)
            .await?;
        match action {
            NativeAction::DoubleClick { .. } => {
                unreachable!("double-click is handled before the native async action match")
            }
            NativeAction::Check { target } => {
                self.action_checked_async_with_click(target, true).await
            }
            NativeAction::Uncheck { target } => {
                self.action_checked_async_with_click(target, false).await
            }
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                self.require_layout_actionable(id)?;
                let link_href = self
                    .document
                    .link_href(id)
                    .filter(|href| !href.is_empty())
                    .map(str::to_owned);
                let download_attribute =
                    self.document.link_download_attribute(id).map(str::to_owned);
                if download_attribute.is_some()
                    && self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS
                {
                    return Err(NativeEngineError::limit(
                        "native pending downloads",
                        MAX_NATIVE_PENDING_DOWNLOADS,
                        self.pending_downloads.len().saturating_add(1),
                    ));
                }
                let mut preview = self.document.clone();
                preview.apply_click(id)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process click preflight".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_click_with_event_preflight(id.index())
                        .await?
                };
                let navigation = mutation.navigation.clone();
                let click_allowed = mutation.allowed;
                if click_allowed
                    && link_href.is_some()
                    && mutation.history.iter().any(|command| {
                        matches!(
                            command,
                            NativeScriptCommand::HistoryGo { delta } if *delta != 0
                        )
                    })
                {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "history traversal cannot share a content click with navigation"
                            .into(),
                    });
                }
                let next_revision = self.next_revision()?;
                let outcome = self
                    .apply_content_process_mutation_async_at(next_revision, mutation)
                    .await?;
                if let Some(navigation) = navigation {
                    self.navigate_script_navigation_async(navigation, 0).await?;
                    return Ok(NativeActionResult {
                        revision: self.revision,
                        accepted: outcome.accepted,
                    });
                }
                if click_allowed && let Some(href) = link_href {
                    let target_url = self.resolve_link_href(&href)?;
                    if let Some(download_attribute) = download_attribute {
                        self.queue_download(target_url, &download_attribute)?;
                        return Ok(NativeActionResult {
                            revision: self.revision,
                            accepted: outcome.accepted,
                        });
                    }
                    if self.document.link_opens_new_target(id) {
                        self.queue_popup(target_url)?;
                        return Ok(NativeActionResult {
                            revision: self.revision,
                            accepted: outcome.accepted,
                        });
                    }
                    self.navigate_request_async(NativeNavigationRequest::get(target_url), 0)
                        .await?;
                    return Ok(NativeActionResult {
                        revision: self.revision,
                        accepted: outcome.accepted,
                    });
                }
                Ok(outcome)
            }
            NativeAction::Hover { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                let preview = self.document.clone();
                preview.apply_hover(id)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process hover event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(serde_json::json!({
                            "kind": "hover",
                            "node_index": id.index(),
                        }))
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Drag {
                source,
                destination,
            } => {
                let source_id = self.document.resolve_target(&source)?;
                let destination_id = self.document.resolve_target(&destination)?;
                self.require_layout_actionable(source_id)?;
                self.require_layout_actionable(destination_id)?;
                let preview = self.document.clone();
                preview.apply_drag(source_id, destination_id)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process drag event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(serde_json::json!({
                            "kind": "drag",
                            "node_index": source_id.index(),
                            "destination_node_index": destination_id.index(),
                        }))
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Upload { target, files } => {
                let id = self.document.resolve_target(&target)?;
                let mut preview = self.document.clone();
                preview.apply_upload(id, &files)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process upload event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(serde_json::json!({
                            "kind": "upload",
                            "node_index": id.index(),
                            "files": files,
                        }))
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Type { target, text } => {
                let id = if target == "focused" {
                    self.document.focused_text_control()?
                } else {
                    self.document.resolve_target(&target)?
                };
                self.require_layout_actionable(id)?;
                let mut preview = self.document.clone();
                preview.apply_type(id, &text)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process type event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_type_with_event_bridge(id.index(), text)
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Clear { target } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                let mut preview = self.document.clone();
                preview.apply_clear(id)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process clear event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(serde_json::json!({
                            "kind": "clear",
                            "node_index": id.index(),
                        }))
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Select { target, value } => {
                let id = self.document.resolve_target(&target)?;
                self.require_layout_actionable(id)?;
                let mut preview = self.document.clone();
                preview.apply_select(id, &value)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process select event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_form_action_with_event_bridge(serde_json::json!({
                            "kind": "select",
                            "node_index": id.index(),
                            "value": value,
                        }))
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::KeyDown { key } => {
                let node_index = self.document.focused_node().index();
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process keydown event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_event_with_event_bridge(
                            node_index,
                            key,
                            NativeEventKind::KeyDown,
                            0,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::KeyUp { key } => {
                let node_index = self.document.focused_node().index();
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process keyup event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_event_with_event_bridge(
                            node_index,
                            key,
                            NativeEventKind::KeyUp,
                            0,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Shortcut { shortcut } => {
                let (modifiers, key) = parse_native_shortcut(&shortcut)?;
                let node_index = self.document.focused_node().index();
                let default_allowed = should_apply_native_key_default(&key, modifiers);
                let apply_default = default_allowed
                    && (key == "Tab"
                        || self
                            .document
                            .focused_text_control()
                            .is_ok_and(|id| id.index() == node_index));
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process shortcut event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_shortcut_with_event_bridge(
                            node_index,
                            key,
                            modifiers,
                            apply_default,
                        )
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::KeyPress { key } => {
                validate_native_edit_key(&key)?;
                let id = self.document.focused_text_control()?;
                let mut preview = self.document.clone();
                preview.apply_key_press(id, &key)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process key event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_key_with_event_bridge(id.index(), key)
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_async_at(next_revision, mutation)
                    .await
            }
            NativeAction::Scroll { .. } => self.action(action),
        }
    }

    /// Apply the browser action contract's bounded scroll-into-view step for
    /// semantic element targets. Coordinate clicks are deliberately excluded:
    /// their viewport coordinates are explicit and must never be adjusted.
    /// The scroll is folded into the following action transaction, so the
    /// action still commits one revision; read-only preflight never calls this
    /// helper.
    fn scroll_action_targets_into_view(
        &mut self,
        action: &NativeAction,
    ) -> Result<(), NativeEngineError> {
        let mut targets = Vec::new();
        match action {
            NativeAction::Click { target }
            | NativeAction::DoubleClick { target }
            | NativeAction::Hover { target }
            | NativeAction::Type { target, .. }
            | NativeAction::Clear { target }
            | NativeAction::Check { target }
            | NativeAction::Uncheck { target }
            | NativeAction::Select { target, .. } => targets.push(target.as_str()),
            NativeAction::Drag {
                source,
                destination,
            } => {
                targets.push(source.as_str());
                targets.push(destination.as_str());
            }
            NativeAction::Upload { .. }
            | NativeAction::KeyDown { .. }
            | NativeAction::KeyUp { .. }
            | NativeAction::Shortcut { .. }
            | NativeAction::KeyPress { .. }
            | NativeAction::Scroll { .. } => {}
        }
        for target in targets {
            if parse_point_target(target)?.is_some() {
                continue;
            }
            let id = self.document.resolve_target(target)?;
            self.scroll_node_into_view(id)?;
        }
        Ok(())
    }

    fn scroll_node_into_view(&mut self, id: NativeNodeId) -> Result<(), NativeEngineError> {
        if self.document.is_hidden_for_layout(id) {
            return Ok(());
        }
        let layout = self.layout()?;
        if layout.viewport_rect_for(id).is_some() {
            return Ok(());
        }
        let Some(target) = layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == id)
        else {
            return Ok(());
        };
        let max_scroll = layout.max_scroll_offset();
        let viewport_right = self.scroll_offset.x.saturating_add(layout.viewport.width);
        let viewport_bottom = self.scroll_offset.y.saturating_add(layout.viewport.height);
        let next = NativePoint {
            x: scroll_axis_into_view(
                target.rect.x,
                target.rect.right(),
                self.scroll_offset.x,
                viewport_right,
                max_scroll.x,
            ),
            y: scroll_axis_into_view(
                target.rect.y,
                target.rect.bottom(),
                self.scroll_offset.y,
                viewport_bottom,
                max_scroll.y,
            ),
        };
        if next == self.scroll_offset {
            return Ok(());
        }
        self.scroll_offset = next;
        self.sync_javascript_scroll_offset();
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        Ok(())
    }

    fn action_checked_with_click(
        &mut self,
        target: String,
        desired: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let id = self.document.resolve_target(&target)?;
        self.require_layout_actionable(id)?;
        if self.document.checked_control_state(id)? == desired {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        let outcome = self.action(NativeAction::Click { target })?;
        if self.document.checked_control_state(id)? != desired {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "checkable control did not reach the requested state".into(),
            });
        }
        Ok(outcome)
    }

    async fn action_checked_async_with_click(
        &mut self,
        target: String,
        desired: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let id = self.document.resolve_target(&target)?;
        self.require_layout_actionable(id)?;
        if self.document.checked_control_state(id)? == desired {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        let outcome = Box::pin(self.action_async(NativeAction::Click { target })).await?;
        if self.document.checked_control_state(id)? != desired {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "checkable control did not reach the requested state".into(),
            });
        }
        Ok(outcome)
    }

    fn apply_local_script_commands(
        &mut self,
        commands: &[super::javascript::NativeScriptCommand],
        allow_script_navigation: bool,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if commands.is_empty() {
            return Ok(None);
        }
        if commands.iter().any(|command| {
            matches!(
                command,
                super::javascript::NativeScriptCommand::Fetch { .. }
                    | super::javascript::NativeScriptCommand::WebSocketOpen { .. }
                    | super::javascript::NativeScriptCommand::WebSocketSend { .. }
                    | super::javascript::NativeScriptCommand::WebSocketClose { .. }
                    | super::javascript::NativeScriptCommand::EventSourceOpen { .. }
                    | super::javascript::NativeScriptCommand::EventSourceClose { .. }
                    | super::javascript::NativeScriptCommand::FetchStreamRead { .. }
                    | super::javascript::NativeScriptCommand::FetchStreamCancel { .. }
            )
        }) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "script network transport requires a process-backed HTTP(S) document"
                    .into(),
            });
        }
        let history_commands = self.prepare_local_history_commands(commands)?;
        let mut scroll_commands = extract_local_scroll_commands(commands);
        let mut document = self.document.clone();
        let mut events = if allow_script_navigation {
            document.apply_script_commands_allowing_links(commands)?
        } else {
            document.apply_script_commands(commands)?
        };
        let mut dynamic_navigation = None;
        let mut dynamic_dialogs = Vec::new();
        if let Some(javascript) = self.javascript.as_ref() {
            let dynamic_sources = document.take_newly_attached_page_script_sources(
                commands,
                super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                MAX_NATIVE_SCRIPT_BYTES,
            );
            if !dynamic_sources.is_empty() {
                if dynamic_sources.iter().any(|source| {
                    matches!(
                        source,
                        super::dom::NativePageScriptSource::External { .. }
                            | super::dom::NativePageScriptSource::ModuleExternal { .. }
                    )
                }) {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "dynamic external/module scripts require a process-backed HTTP(S) document"
                            .into(),
                    });
                }
                let dynamic_scripts = page_script_sources_to_scripts(
                    dynamic_sources,
                    &self.url,
                    "glass-dynamic-module",
                );
                let dynamic_result = execute_dynamic_page_scripts(
                    &mut document,
                    javascript,
                    dynamic_scripts,
                    &self.url,
                    &self.origin,
                    self.config.viewport,
                    &[],
                )?;
                if !dynamic_result.pending_script_sources.is_empty() {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "dynamic external/module scripts require a process-backed HTTP(S) document"
                            .into(),
                    });
                }
                if !dynamic_result.pending_fetches.is_empty()
                    || !dynamic_result.websocket_commands.is_empty()
                    || !dynamic_result.event_source_commands.is_empty()
                {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "dynamic script network transport requires a process-backed HTTP(S) document"
                            .into(),
                    });
                }
                events.extend(dynamic_result.events.into_iter().map(|(node_index, kind)| {
                    (
                        NativeNodeId::from_parts(document.generation(), node_index),
                        kind,
                    )
                }));
                scroll_commands.extend(dynamic_result.scroll_commands);
                if let Some(navigation) = dynamic_result.navigation {
                    dynamic_navigation = Some(ScriptNavigationTarget::Location {
                        href: navigation.href,
                        replace_history: navigation.replace_history,
                    });
                }
                dynamic_dialogs = dynamic_result.dialogs;
            }
        }
        if !dynamic_dialogs.is_empty() {
            let dialog_url = self.url.clone();
            self.install_dialogs(dynamic_dialogs, &dialog_url)?;
        }
        let validation_events = events
            .iter()
            .filter(|(_, kind)| *kind == NativeEventKind::Invalid)
            .copied()
            .collect::<Vec<_>>();
        if !validation_events.is_empty()
            && let Some(evaluation) = self.evaluate_local_events(&document, &validation_events)?
        {
            scroll_commands.extend(extract_local_scroll_commands(&evaluation.commands));
            events.extend(document.apply_script_commands(&evaluation.commands)?);
        }
        let mut navigation = if allow_script_navigation {
            self.script_navigation_target(&document, commands)?
        } else {
            None
        };
        if let Some(dynamic_navigation) = dynamic_navigation {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one script batch cannot activate multiple navigations".into(),
                });
            }
            navigation = Some(dynamic_navigation);
        }
        if let Some(ScriptNavigationTarget::Form {
            form_id,
            dispatch_submit: true,
            submitter,
        }) = navigation.as_ref()
        {
            let invalid = document.invalid_form_controls(*form_id, *submitter)?;
            if invalid.is_empty() {
                let evaluation = self
                    .evaluate_local_submit_event(&document, *form_id, *submitter)?
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "native submit event".into(),
                        reason: "native JavaScript realm disappeared during submit dispatch".into(),
                    })?;
                let allowed = evaluation
                    .value
                    .as_array()
                    .and_then(|values| values.first())
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "native submit event".into(),
                        reason: "native submit event result was invalid".into(),
                    })?;
                events.push((*form_id, NativeEventKind::Submit));
                scroll_commands.extend(extract_local_scroll_commands(&evaluation.commands));
                events.extend(document.apply_script_commands(&evaluation.commands)?);
                if !allowed {
                    navigation = None;
                }
            } else {
                let invalid_events = invalid
                    .iter()
                    .copied()
                    .map(|id| (id, NativeEventKind::Invalid))
                    .collect::<Vec<_>>();
                if let Some(evaluation) = self.evaluate_local_events(&document, &invalid_events)? {
                    events.extend(invalid_events);
                    scroll_commands.extend(extract_local_scroll_commands(&evaluation.commands));
                    events.extend(document.apply_script_commands(&evaluation.commands)?);
                }
                navigation = None;
            }
        }
        if history_commands
            .iter()
            .any(|command| matches!(command, LocalHistoryCommand::Go(_)))
            && navigation.is_some()
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share a script batch with navigation".into(),
            });
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        let scroll_events = self.apply_scroll_commands(&self.document.clone(), &scroll_commands)?;
        events.extend(scroll_events.iter().copied());
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_prepared_history_commands(history_commands, next_revision)?;
        self.record_effects(events);
        if !scroll_events.is_empty() {
            self.dispatch_local_events(&scroll_events)?;
        }
        let navigation = match navigation {
            Some(ScriptNavigationTarget::Link { href, popup }) => {
                let target_url = self.resolve_link_href(&href)?;
                if popup {
                    self.queue_popup(target_url)?;
                    None
                } else {
                    Some(NativeNavigationRequest::get(target_url))
                }
            }
            Some(ScriptNavigationTarget::Form {
                form_id, submitter, ..
            }) => Some(
                self.document
                    .form_submission_request_with_submitter(form_id, &self.url, submitter)?,
            ),
            Some(ScriptNavigationTarget::Location {
                href,
                replace_history,
            }) => {
                let mut request = NativeNavigationRequest::get(self.resolve_link_href(&href)?);
                request.replace_history = replace_history;
                Some(request)
            }
            None => None,
        };
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    fn prepare_local_history_commands(
        &self,
        commands: &[super::javascript::NativeScriptCommand],
    ) -> Result<Vec<LocalHistoryCommand>, NativeEngineError> {
        let mut prepared = Vec::new();
        let mut base_url = self.url.clone();
        let mut traversal_seen = false;
        for command in commands {
            let prepared_command = match command {
                super::javascript::NativeScriptCommand::HistoryPushState { href, state } => {
                    if traversal_seen {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "history state mutation cannot follow traversal in one script batch"
                                    .into(),
                        });
                    }
                    let url = self.resolve_history_href_from(&base_url, href)?;
                    validate_history_state(state)?;
                    base_url = url.clone();
                    LocalHistoryCommand::PushState {
                        url,
                        state: state.clone(),
                    }
                }
                super::javascript::NativeScriptCommand::HistoryReplaceState { href, state } => {
                    if traversal_seen {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "history state mutation cannot follow traversal in one script batch"
                                    .into(),
                        });
                    }
                    let url = self.resolve_history_href_from(&base_url, href)?;
                    validate_history_state(state)?;
                    base_url = url.clone();
                    LocalHistoryCommand::ReplaceState {
                        url,
                        state: state.clone(),
                    }
                }
                super::javascript::NativeScriptCommand::HistoryGo { delta } => {
                    if traversal_seen {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "a script batch cannot request multiple history traversals"
                                .into(),
                        });
                    }
                    traversal_seen = true;
                    LocalHistoryCommand::Go(*delta)
                }
                _ => continue,
            };
            prepared.push(prepared_command);
        }
        Ok(prepared)
    }

    fn script_navigation_target(
        &self,
        document: &NativeDocument,
        commands: &[super::javascript::NativeScriptCommand],
    ) -> Result<Option<ScriptNavigationTarget>, NativeEngineError> {
        let mut navigation = None;
        for command in commands {
            let target = match command {
                super::javascript::NativeScriptCommand::Click { node_index } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    if let Some(href) = document.link_href(id).filter(|href| !href.is_empty()) {
                        Some(ScriptNavigationTarget::Link {
                            href: href.to_owned(),
                            popup: document.link_opens_new_target(id),
                        })
                    } else if let Some(form_id) = document.submit_control_form(id) {
                        document.form_submission_request_with_submitter(
                            form_id,
                            &self.url,
                            Some(id),
                        )?;
                        Some(ScriptNavigationTarget::Form {
                            form_id,
                            dispatch_submit: true,
                            submitter: Some(id),
                        })
                    } else {
                        None
                    }
                }
                super::javascript::NativeScriptCommand::SubmitForm { node_index } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    document.form_submission_request(id, &self.url)?;
                    Some(ScriptNavigationTarget::Form {
                        form_id: id,
                        dispatch_submit: false,
                        submitter: None,
                    })
                }
                super::javascript::NativeScriptCommand::RequestSubmitForm {
                    node_index,
                    submitter_index,
                } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    let submitter = submitter_index
                        .map(|index| NativeNodeId::from_parts(document.generation(), index));
                    if let Some(submitter) = submitter
                        && document.submit_control_form(submitter) != Some(id)
                    {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "requestSubmit submitter must be a submit control for the form"
                                .into(),
                        });
                    }
                    document.form_submission_request_with_submitter(id, &self.url, submitter)?;
                    Some(ScriptNavigationTarget::Form {
                        form_id: id,
                        dispatch_submit: true,
                        submitter,
                    })
                }
                super::javascript::NativeScriptCommand::Navigate { href, replace } => {
                    validate_url_text("script location href", href)?;
                    Some(ScriptNavigationTarget::Location {
                        href: href.to_owned(),
                        replace_history: *replace,
                    })
                }
                _ => None,
            };
            let Some(target) = target else {
                continue;
            };
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one script batch cannot activate multiple navigations".into(),
                });
            }
            navigation = Some(target);
        }
        Ok(navigation)
    }

    fn script_location_navigation_request(
        &self,
        commands: &[super::javascript::NativeScriptCommand],
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let mut navigation = None;
        for command in commands {
            let super::javascript::NativeScriptCommand::Navigate { href, replace } = command else {
                continue;
            };
            validate_url_text("script location href", href)?;
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one lifecycle event cannot activate multiple navigations".into(),
                });
            }
            let mut request = NativeNavigationRequest::get(self.resolve_link_href(href)?);
            request.replace_history = *replace;
            navigation = Some(request);
        }
        Ok(navigation)
    }

    async fn navigate_script_navigation_async(
        &mut self,
        navigation: NativeContentNavigation,
        page_navigation_handoffs: usize,
    ) -> Result<(), NativeEngineError> {
        if page_navigation_handoffs > MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
            return Err(NativeEngineError::limit(
                "page navigation handoffs",
                MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                page_navigation_handoffs,
            ));
        }
        if navigation.location {
            let target_url = self.resolve_link_href(&navigation.href)?;
            let mut request = NativeNavigationRequest::get(target_url);
            request.replace_history = navigation.replace_history;
            Box::pin(self.navigate_request_async(request, page_navigation_handoffs))
                .await
                .map(|_| ())?;
            return Ok(());
        }
        let id = NativeNodeId::from_parts(self.document.generation(), navigation.node_index);
        let submitter = navigation
            .submitter_node_index
            .map(|index| NativeNodeId::from_parts(self.document.generation(), index));
        let download_attribute = self.document.link_download_attribute(id).map(str::to_owned);
        let mut request =
            if let Some(href) = self.document.link_href(id).filter(|href| !href.is_empty()) {
                NativeNavigationRequest::get(href)
            } else {
                self.document
                    .form_submission_request_with_submitter(id, &self.url, submitter)?
            };
        if request.url != navigation.href {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script navigation target changed during transfer".into(),
            });
        }
        let target_url = self.resolve_link_href(&request.url)?;
        if let Some(download_attribute) = download_attribute {
            self.queue_download(target_url, &download_attribute)?;
            return Ok(());
        }
        if self.document.link_opens_new_target(id) {
            self.queue_popup(target_url)?;
            return Ok(());
        }
        request.url = target_url.clone();
        request.replace_history = navigation.replace_history;
        if request.method == NativeNavigationMethod::Get
            && self.is_same_document_navigation(&target_url)
        {
            let history_commit = if request.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            if let Some(worker) = self.runtime_worker.clone() {
                self.commit_same_document_navigation_async(
                    target_url,
                    history_commit,
                    &worker,
                    page_navigation_handoffs,
                )
                .await?;
            } else {
                if let Some(navigation) =
                    self.commit_same_document_navigation(target_url, history_commit)?
                {
                    self.navigate_page_script_sync(navigation, page_navigation_handoffs + 1)?;
                }
            }
            return Ok(());
        }
        Box::pin(self.navigate_request_async(request, page_navigation_handoffs))
            .await
            .map(|_| ())
    }

    fn sync_external_storage_events(&mut self) -> Result<(), NativeEngineError> {
        let journal = read_storage_event_journal(
            self.config.storage_path.as_deref(),
            &self.storage_writer_id,
            &mut self.storage_event_offset,
        )?;
        if journal.recovered {
            let profile_state = load_web_storage_profile(self.config.storage_path.as_deref())?;
            let indexed_db_state = load_indexed_db_profile(self.config.storage_path.as_deref())?;
            self.web_storage.replace_profile_state(profile_state);
            self.indexed_db = indexed_db_state;
            self.storage_state_recovery_pending = true;
            self.indexed_db_state_delivery_pending = true;
            self.pending_external_storage_events.clear();
            if let Some(javascript) = self.javascript.as_ref() {
                javascript.replace_storage_state(self.web_storage.clone());
                javascript.set_indexed_db_state(
                    self.indexed_db
                        .origin(&storage_key(&self.url, &self.origin)),
                );
            }
        }
        let current_storage_key = storage_key(&self.url, &self.origin);
        let mut events = Vec::new();
        let mut indexed_db_changes = Vec::new();
        for record in journal.records {
            if record.writer_id == self.storage_writer_id {
                continue;
            }
            if let Some(event) = record.event
                && event.storage_key == current_storage_key
                && (event.scope == "local"
                    || (event.scope == "session"
                        && event.source_context_id == self.config.context_id))
            {
                events.push(event);
            }
            if !record.indexed_db_changes.is_empty() {
                apply_indexed_db_changes(&mut self.indexed_db, &record.indexed_db_changes)?;
                indexed_db_changes.extend(record.indexed_db_changes);
            }
        }
        if !indexed_db_changes.is_empty() {
            self.indexed_db_state_delivery_pending = true;
        }
        if events.is_empty() {
            return Ok(());
        }
        for event in &events {
            self.web_storage.apply_storage_event(event)?;
        }
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_storage_state(self.web_storage.clone());
            javascript.set_storage_events(events)?;
        } else {
            let next_len = self
                .pending_external_storage_events
                .len()
                .saturating_add(events.len());
            if next_len > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native pending storage events",
                    MAX_NATIVE_EFFECTS,
                    next_len,
                ));
            }
            self.pending_external_storage_events.extend(events);
        }
        Ok(())
    }

    async fn deliver_pending_external_storage_events(&mut self) -> Result<(), NativeEngineError> {
        if self.pending_external_storage_events.is_empty()
            && !self.storage_state_recovery_pending
            && !self.indexed_db_state_delivery_pending
        {
            return Ok(());
        }
        let events = std::mem::take(&mut self.pending_external_storage_events);
        let storage_state = self.web_storage.clone();
        let indexed_db_state = self.indexed_db.clone();
        let recovery_pending = self.storage_state_recovery_pending;
        let indexed_db_pending = self.indexed_db_state_delivery_pending;
        if let Some(process) = self.content_process.as_mut() {
            if !process.refresh_health() {
                self.pending_external_storage_events = events;
                return Err(NativeEngineError::worker_failure(
                    "content process storage events",
                    process
                        .failure_kind()
                        .unwrap_or(NativeWorkerFailureKind::Exited),
                    "content process is unavailable after a failed operation; navigate to recover it",
                ));
            }
            let result = async {
                if recovery_pending || indexed_db_pending {
                    process
                        .sync_storage_state(&storage_state, &indexed_db_state)
                        .await?;
                }
                process.sync_storage_events(&events).await
            }
            .await;
            match result {
                Ok(()) => {
                    self.storage_state_recovery_pending = false;
                    self.indexed_db_state_delivery_pending = false;
                    Ok(())
                }
                Err(error) => {
                    self.pending_external_storage_events = events;
                    Err(error)
                }
            }
        } else if let Some(javascript) = self.javascript.as_ref() {
            if recovery_pending {
                javascript.replace_storage_state(storage_state);
            } else {
                javascript.set_storage_state(storage_state);
            }
            if indexed_db_pending || recovery_pending {
                javascript.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&self.url, &self.origin)),
                );
            }
            let result = if events.is_empty() {
                Ok(())
            } else {
                javascript.set_storage_events(events.clone())
            };
            if result.is_ok() {
                self.storage_state_recovery_pending = false;
                self.indexed_db_state_delivery_pending = false;
            } else {
                self.pending_external_storage_events = events;
            }
            result
        } else {
            self.pending_external_storage_events = events;
            Ok(())
        }
    }

    fn publish_content_state(
        &mut self,
        events: &[NativeStorageEvent],
        indexed_db_changes: &[NativeIndexedDbChange],
    ) -> Result<(), NativeEngineError> {
        for event in events {
            self.web_storage.apply_storage_event(event)?;
        }
        apply_indexed_db_changes(&mut self.indexed_db, indexed_db_changes)?;
        append_storage_changes(
            self.config.storage_path.as_deref(),
            &self.storage_writer_id,
            events,
            indexed_db_changes,
        )?;
        Ok(())
    }

    fn persist_local_web_storage(&mut self) -> Result<(), NativeEngineError> {
        if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            return Ok(());
        }
        let (storage_changes, indexed_db_changes) = if let Some(javascript) =
            self.javascript.as_ref()
        {
            let cookie_updates = javascript.take_cookie_updates();
            self.web_storage = javascript.storage_state();
            let indexed_db_key = storage_key(&self.url, &self.origin);
            let before_indexed_db = self.indexed_db.origin(&indexed_db_key);
            let after_indexed_db = javascript.indexed_db_state();
            let indexed_db_changes =
                diff_indexed_db_changes(&indexed_db_key, &before_indexed_db, &after_indexed_db)?;
            self.indexed_db
                .replace_origin(indexed_db_key, after_indexed_db)?;
            for value in cookie_updates {
                self.loader.set_document_cookie(&self.url, &value)?;
            }
            (javascript.take_storage_changes(), indexed_db_changes)
        } else {
            (Vec::new(), Vec::new())
        };
        let cookie_state = self.loader.cookie_profile();
        let cookie_changes = self.loader.take_cookie_changes();
        save_web_storage_profile(
            self.config.storage_path.as_deref(),
            &self.web_storage,
            &storage_changes,
            &cookie_state,
            &cookie_changes,
            &self.indexed_db,
            &indexed_db_changes,
        )?;
        append_storage_changes(
            self.config.storage_path.as_deref(),
            &self.storage_writer_id,
            &storage_changes,
            &indexed_db_changes,
        )?;
        Ok(())
    }

    fn persist_local_script_state(&mut self) -> Result<(), NativeEngineError> {
        self.persist_local_web_storage()
    }

    fn drain_local_dialogs(&mut self) -> Result<(), NativeEngineError> {
        let dialogs = self
            .javascript
            .as_ref()
            .map(NativeJavaScriptRuntime::take_dialog_events)
            .unwrap_or_default();
        if dialogs.is_empty() {
            return Ok(());
        }
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)
    }

    fn drain_local_popups(&mut self) -> Result<(), NativeEngineError> {
        if let Some(javascript) = self.javascript.as_ref() {
            self.config.window_name = javascript.window_name();
        }
        let popups = self
            .javascript
            .as_ref()
            .map(NativeJavaScriptRuntime::take_popup_events)
            .unwrap_or_default();
        self.queue_popup_requests(popups)?;
        let messages = self
            .javascript
            .as_ref()
            .map(NativeJavaScriptRuntime::take_post_message_events)
            .unwrap_or_default();
        self.queue_post_message_requests(messages)?;
        let window_closes = self
            .javascript
            .as_ref()
            .map(NativeJavaScriptRuntime::take_window_close_events)
            .unwrap_or_default();
        self.queue_window_close_requests(window_closes)?;
        let window_navigations = self
            .javascript
            .as_ref()
            .map(NativeJavaScriptRuntime::take_window_navigation_events)
            .unwrap_or_default();
        self.queue_window_navigation_requests(window_navigations)
    }

    fn dispatch_local_events(
        &mut self,
        events: &[(NativeNodeId, NativeEventKind)],
    ) -> Result<(), NativeEngineError> {
        let document = self.document.clone();
        let Some(evaluation) = self.evaluate_local_events(&document, events)? else {
            return Ok(());
        };
        self.apply_local_script_commands(&evaluation.commands, false)
            .map(|_| ())
    }

    fn dispatch_local_navigation_lifecycle(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let lifecycle_events = [
            (window, NativeEventKind::PageHide),
            (window, NativeEventKind::Unload),
        ];
        let document = self.document.clone();
        let Some(evaluation) = self.evaluate_local_events(&document, &lifecycle_events)? else {
            return Ok(None);
        };
        let history_commands = extract_local_history_commands(&evaluation.commands);
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        effects.extend(lifecycle_events);
        if evaluation.commands.is_empty() {
            self.pending_lifecycle_effects.extend(effects);
            return Ok(navigation);
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share a lifecycle event with navigation"
                        .into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    fn dispatch_local_before_unload(
        &mut self,
    ) -> Result<(bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let document = self.document.clone();
        let Some(evaluation) =
            self.evaluate_local_events(&document, &[(window, NativeEventKind::BeforeUnload)])?
        else {
            return Ok((true, None));
        };
        let allowed = evaluation
            .value
            .as_array()
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native beforeunload".into(),
                reason: "beforeunload event result was invalid".into(),
            })?;
        let history_commands = extract_local_history_commands(&evaluation.commands);
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        effects.push((window, NativeEventKind::BeforeUnload));
        if evaluation.commands.is_empty() {
            self.record_effects(effects);
            return Ok((allowed, navigation.filter(|_| allowed)));
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share beforeunload navigation".into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok((allowed, navigation.filter(|_| allowed)))
    }

    fn dispatch_local_page_show(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if self.javascript.is_none() {
            return Ok(None);
        }
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let event = (window, NativeEventKind::PageShow);
        let document = self.document.clone();
        let navigation = self
            .evaluate_local_events(&document, &[event])?
            .map(|evaluation| self.apply_local_script_commands(&evaluation.commands, true))
            .transpose()?
            .flatten();
        self.record_effects(vec![event]);
        Ok(navigation)
    }

    fn dispatch_local_pop_state(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if self.javascript.is_none() {
            return Ok(None);
        }
        let window = NativeNodeId::from_parts(self.document.generation(), u32::MAX);
        let event = (window, NativeEventKind::PopState);
        let document = self.document.clone();
        let Some(evaluation) = self.evaluate_local_events(&document, &[event])? else {
            return Ok(None);
        };
        let history_commands = extract_local_history_commands(&evaluation.commands);
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        effects.push(event);
        if evaluation.commands.is_empty() {
            self.record_effects(effects);
            return Ok(navigation);
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share popstate navigation".into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    fn dispatch_local_hash_change(
        &mut self,
        old_url: &str,
        new_url: &str,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if self.javascript.is_none() {
            return Ok(None);
        }
        let javascript = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime is present");
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        let page_events = NativePageEventBatch {
            hash_change_events: vec![NativeHashChangeEvent {
                old_url: old_url.to_owned(),
                new_url: new_url.to_owned(),
            }],
            ..NativePageEventBatch::default()
        };
        let evaluation = self
            .javascript
            .as_ref()
            .expect("local JavaScript runtime is present")
            .evaluate_with_page_events(
                "undefined;",
                &self.document,
                new_url,
                &self.origin,
                self.config.viewport,
                &page_events,
            )?;
        let history_commands = extract_local_history_commands(&evaluation.commands);
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        let event = (
            NativeNodeId::from_parts(self.document.generation(), u32::MAX),
            NativeEventKind::HashChange,
        );
        let navigation = self.script_location_navigation_request(&evaluation.commands)?;
        if evaluation.commands.is_empty() {
            self.record_effects(vec![event]);
            return Ok(navigation);
        }
        let mut document = self.document.clone();
        let mut effects = document.apply_script_commands_allowing_links(&evaluation.commands)?;
        effects.push(event);
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(effects);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "history traversal cannot share hashchange navigation".into(),
                });
            }
            self.traverse_history_delta(delta)?;
        }
        Ok(navigation)
    }

    fn evaluate_local_events(
        &mut self,
        document: &NativeDocument,
        events: &[(NativeNodeId, NativeEventKind)],
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let event_metadata = events
            .iter()
            .map(|(node_id, kind)| (node_id.index(), *kind))
            .collect::<Vec<_>>();
        let Some(event_batch) = host_event_batch(&event_metadata)? else {
            return Ok(None);
        };
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        let evaluation = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        )?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    fn evaluate_local_submit_event(
        &mut self,
        document: &NativeDocument,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let Some(event_batch) =
            host_submit_event_batch(form_id.index(), submitter.map(NativeNodeId::index))?
        else {
            return Ok(None);
        };
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        let evaluation = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        )?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    fn evaluate_local_key_event_with_modifiers(
        &mut self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        kind: NativeEventKind,
        key: &str,
        modifiers: i64,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let Some(event_batch) =
            host_key_event_batch_with_modifiers(node_id.index(), kind, key, modifiers)?
        else {
            return Ok(None);
        };
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        let evaluation = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        )?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    fn action_local_click_with_event_preflight(
        &mut self,
        id: NativeNodeId,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = document.apply_script_focus(id)?;
        if let Some(evaluation) = self.evaluate_local_events(&document, &events)? {
            history_commands.extend(extract_local_history_commands(&evaluation.commands));
            events.extend(document.apply_script_commands(&evaluation.commands)?);
        }

        let click_evaluation = self
            .evaluate_local_events(&document, &[(id, NativeEventKind::Click)])?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native click event preflight".into(),
                reason: "native JavaScript realm disappeared during click preflight".into(),
            })?;
        let click_allowed = click_evaluation
            .value
            .as_array()
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native click event preflight".into(),
                reason: "native click event result was invalid".into(),
            })?;
        history_commands.extend(extract_local_history_commands(&click_evaluation.commands));
        events.extend(document.apply_script_commands(&click_evaluation.commands)?);
        let mut navigation: Option<(NativeNodeId, NativeNodeId)> = None;
        let mut link_navigation = None;
        if click_allowed {
            events.extend(document.apply_click(id)?);
            if let Some(href) = document.link_href(id).filter(|href| !href.is_empty()) {
                link_navigation = Some(href.to_owned());
            } else if let Some(form_id) = document.submit_control_form(id) {
                let invalid = document.invalid_form_controls(form_id, Some(id))?;
                if invalid.is_empty() {
                    let submit_evaluation = self
                        .evaluate_local_submit_event(&document, form_id, Some(id))?
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "native submit event".into(),
                            reason: "native JavaScript realm disappeared during submit dispatch"
                                .into(),
                        })?;
                    history_commands
                        .extend(extract_local_history_commands(&submit_evaluation.commands));
                    let submit_allowed = submit_evaluation
                        .value
                        .as_array()
                        .and_then(|values| values.first())
                        .and_then(serde_json::Value::as_bool)
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "native submit event".into(),
                            reason: "native submit event result was invalid".into(),
                        })?;
                    events.push((form_id, NativeEventKind::Submit));
                    events.extend(document.apply_script_commands(&submit_evaluation.commands)?);
                    if submit_allowed {
                        navigation = Some((form_id, id));
                    }
                } else {
                    let invalid_events = invalid
                        .iter()
                        .copied()
                        .map(|invalid_id| (invalid_id, NativeEventKind::Invalid))
                        .collect::<Vec<_>>();
                    if let Some(evaluation) =
                        self.evaluate_local_events(&document, &invalid_events)?
                    {
                        history_commands
                            .extend(extract_local_history_commands(&evaluation.commands));
                        events.extend(invalid_events);
                        events.extend(document.apply_script_commands(&evaluation.commands)?);
                    }
                }
            }
        } else {
            events.push((id, NativeEventKind::Click));
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native click event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        if history_commands.iter().any(|command| {
            matches!(
                command,
                NativeScriptCommand::HistoryGo { delta } if *delta != 0
            )
        }) && (link_navigation.is_some() || navigation.is_some())
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share a click with navigation".into(),
            });
        }

        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(href) = link_navigation {
            return self.activate_link(id, &href, true);
        }
        if let Some((form_id, submitter)) = navigation {
            let request = self.document.form_submission_request_with_submitter(
                form_id,
                &self.url,
                Some(submitter),
            )?;
            let snapshot = self.navigate(request.url)?;
            return Ok(NativeActionResult {
                revision: snapshot.revision,
                accepted: true,
            });
        }
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    /// Validate the synchronous local default-navigation path before running
    /// click handlers. Event dispatch is transactional from the caller's
    /// perspective: an unsupported destination must not leave focus, event
    /// effects, or a revision behind merely because the handler ran first.
    fn preflight_local_link_navigation(
        &self,
        id: NativeNodeId,
        href: &str,
    ) -> Result<(), NativeEngineError> {
        if self.document.link_download_attribute(id).is_some()
            || self.document.link_opens_new_target(id)
        {
            return Ok(());
        }
        let target_url = self.resolve_link_href(href)?;
        if !self.allows_frame_navigation(&target_url)? {
            return Ok(());
        }
        self.loader.load(&target_url).map(|_| ())
    }

    fn action_local_submit_click_without_script(
        &mut self,
        id: NativeNodeId,
        form_id: NativeNodeId,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let events = self.document.apply_click(id)?;
        if !self
            .document
            .invalid_form_controls(form_id, Some(id))?
            .is_empty()
        {
            let next_revision = self.next_revision()?;
            self.document.set_revision(next_revision);
            self.revision = next_revision;
            self.history
                .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision: next_revision,
                accepted: true,
            });
        }
        let next_revision = self.next_revision()?;
        self.document.set_revision(next_revision);
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.record_effects(events);
        let request =
            self.document
                .form_submission_request_with_submitter(form_id, &self.url, Some(id))?;
        let snapshot = self.navigate(request.url)?;
        Ok(NativeActionResult {
            revision: snapshot.revision,
            accepted: true,
        })
    }

    fn action_local_type_with_event_transaction(
        &mut self,
        id: NativeNodeId,
        text: &str,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = document.apply_type(id, text)?;
        let default_events = events.clone();
        for event in default_events {
            if let Some(evaluation) = self.evaluate_local_events(&document, &[event])? {
                history_commands.extend(extract_local_history_commands(&evaluation.commands));
                events.extend(document.apply_script_commands(&evaluation.commands)?);
            }
            if events.len() > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native type event effects",
                    MAX_NATIVE_EFFECTS,
                    events.len(),
                ));
            }
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn action_local_form_with_event_transaction(
        &mut self,
        apply: impl FnOnce(
            &mut NativeDocument,
        ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError>,
        effect_limit_name: &str,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = apply(&mut document)?;
        let default_events = events.clone();
        for (event_node, event_kind) in default_events {
            if let Some(evaluation) =
                self.evaluate_local_events(&document, &[(event_node, event_kind)])?
            {
                history_commands.extend(extract_local_history_commands(&evaluation.commands));
                events.extend(document.apply_script_commands(&evaluation.commands)?);
            }
            if events.len() > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    effect_limit_name,
                    MAX_NATIVE_EFFECTS,
                    events.len(),
                ));
            }
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn action_local_key_event(
        &mut self,
        id: NativeNodeId,
        key: &str,
        kind: NativeEventKind,
        modifiers: i64,
    ) -> Result<NativeActionResult, NativeEngineError> {
        validate_native_key(key)?;
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = vec![(id, kind)];
        if let Some(evaluation) =
            self.evaluate_local_key_event_with_modifiers(&document, id, kind, key, modifiers)?
        {
            history_commands.extend(extract_local_history_commands(&evaluation.commands));
            events.extend(document.apply_script_commands(&evaluation.commands)?);
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native key event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn action_local_key_sequence(
        &mut self,
        id: NativeNodeId,
        key: &str,
        modifiers: i64,
        apply_default: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        validate_native_key(key)?;
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = vec![(id, NativeEventKind::KeyDown)];
        let keydown = self.evaluate_local_key_event_with_modifiers(
            &document,
            id,
            NativeEventKind::KeyDown,
            key,
            modifiers,
        )?;
        let keydown_allowed = keydown
            .as_ref()
            .and_then(|evaluation| evaluation.value.as_array())
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if let Some(keydown) = keydown {
            history_commands.extend(extract_local_history_commands(&keydown.commands));
            events.extend(document.apply_script_commands(&keydown.commands)?);
        }

        if keydown_allowed && apply_default && should_apply_native_key_default(key, modifiers) {
            let default_events = if key == "Tab" {
                if document.focused_node() == id {
                    document.apply_tab_focus(modifiers & 8 != 0)?
                } else {
                    Vec::new()
                }
            } else if document
                .focused_text_control()
                .is_ok_and(|focused| focused == id)
            {
                document.apply_key_default(id, key, modifiers)?
            } else {
                Vec::new()
            };
            events.extend(default_events.clone());
            for (event_node, event_kind) in default_events {
                if let Some(evaluation) =
                    self.evaluate_local_events(&document, &[(event_node, event_kind)])?
                {
                    history_commands.extend(extract_local_history_commands(&evaluation.commands));
                    events.extend(document.apply_script_commands(&evaluation.commands)?);
                }
            }
        }

        events.push((id, NativeEventKind::KeyUp));
        if let Some(evaluation) = self.evaluate_local_key_event_with_modifiers(
            &document,
            id,
            NativeEventKind::KeyUp,
            key,
            modifiers,
        )? {
            history_commands.extend(extract_local_history_commands(&evaluation.commands));
            events.extend(document.apply_script_commands(&evaluation.commands)?);
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native key press effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }

        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(&history_commands, next_revision)?;
        self.record_effects(events);
        if let Some(delta) = history_traversal
            && delta != 0
        {
            self.traverse_history_delta(delta)?;
        }
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn apply_content_process_mutation(
        &mut self,
        mutation: NativeContentMutation,
    ) -> Result<(), NativeEngineError> {
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_at(next_revision, mutation)?;
        Ok(())
    }

    async fn apply_content_process_mutation_async_at(
        &mut self,
        next_revision: u64,
        mut mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let history = std::mem::take(&mut mutation.history);
        if history.iter().any(|command| {
            matches!(
                command,
                NativeScriptCommand::HistoryGo { delta } if *delta != 0
            )
        }) && mutation.navigation.is_some()
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share a content event with navigation".into(),
            });
        }
        let outcome = self.apply_content_process_mutation_at(next_revision, mutation)?;
        let history_traversal = self.apply_content_history_commands(&history)?;
        if !history.is_empty() {
            self.sync_content_history_async().await?;
        }
        if let Some(delta) = history_traversal
            && delta != 0
        {
            Box::pin(self.traverse_history_delta_async(delta)).await?;
        }
        Ok(outcome)
    }

    fn apply_content_history_commands(
        &mut self,
        commands: &[NativeScriptCommand],
    ) -> Result<Option<i32>, NativeEngineError> {
        let prepared = self.prepare_local_history_commands(commands)?;
        let revision = self.revision;
        self.apply_prepared_history_commands(prepared, revision)
    }

    fn apply_prepared_history_commands(
        &mut self,
        prepared: Vec<LocalHistoryCommand>,
        revision: u64,
    ) -> Result<Option<i32>, NativeEngineError> {
        let mut traversal = None;
        for command in prepared {
            match command {
                LocalHistoryCommand::PushState { url, state } => {
                    self.url = url.clone();
                    self.history.push_with_state(
                        url,
                        revision,
                        self.scroll_offset,
                        &self.nested_scroll_offsets,
                        state,
                        true,
                    );
                }
                LocalHistoryCommand::ReplaceState { url, state } => {
                    self.url = url.clone();
                    self.history.replace_current_with_state(
                        url,
                        revision,
                        self.scroll_offset,
                        &self.nested_scroll_offsets,
                        state,
                        true,
                    );
                }
                LocalHistoryCommand::Go(delta) => traversal = Some(delta),
            }
        }
        self.sync_javascript_history();
        Ok(traversal)
    }

    fn apply_local_history_commands_at(
        &mut self,
        commands: &[NativeScriptCommand],
        revision: u64,
    ) -> Result<Option<i32>, NativeEngineError> {
        let prepared = self.prepare_local_history_commands(commands)?;
        self.apply_prepared_history_commands(prepared, revision)
    }

    async fn sync_content_scroll_offsets_async(&mut self) -> Result<(), NativeEngineError> {
        if let Some(process) = self.content_process.as_mut() {
            process.sync_scroll_offset(self.scroll_offset).await?;
            process
                .sync_nested_scroll_offsets(&self.nested_scroll_offsets)
                .await?;
        }
        Ok(())
    }

    async fn sync_content_history_async(&mut self) -> Result<(), NativeEngineError> {
        let state = self
            .history
            .current()
            .map(|entry| entry.state.clone())
            .unwrap_or(serde_json::Value::Null);
        if let Some(process) = self.content_process.as_mut() {
            process
                .sync_history(&self.url, &state, self.history.len())
                .await?;
        }
        Ok(())
    }

    fn apply_content_process_mutation_at(
        &mut self,
        next_revision: u64,
        mut mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.config.window_name = mutation.window_name.clone();
        self.queue_popup_requests(std::mem::take(&mut mutation.popups))?;
        self.queue_post_message_requests(std::mem::take(&mut mutation.post_messages))?;
        self.queue_window_close_requests(std::mem::take(&mut mutation.window_closes))?;
        self.queue_window_navigation_requests(std::mem::take(&mut mutation.window_navigations))?;
        self.publish_content_state(&mutation.storage_events, &mutation.indexed_db_changes)?;
        let dialogs = mutation.dialogs.clone();
        let scroll_commands = std::mem::take(&mut mutation.scroll_commands);
        let generation = self.document.generation();
        let mut document = match NativeDocument::from_content_wire(
            mutation.document,
            &self.config.limits,
            generation,
        ) {
            Ok(document) => document,
            Err(error) => {
                self.content_process.take();
                return Err(error);
            }
        };
        let mut events = match mutation
            .events
            .into_iter()
            .map(|event| {
                let node_id = NativeNodeId::from_parts(generation, event.node_index);
                if event.node_index == u32::MAX || document.node(node_id).is_some() {
                    Ok((node_id, event.kind))
                } else {
                    Err(NativeEngineError::DetachedTarget)
                }
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(events) => events,
            Err(error) => {
                self.content_process.take();
                return Err(error);
            }
        };
        let document_changed = {
            let mut normalized = document.clone();
            normalized.set_revision(self.document.revision());
            normalized != self.document
        };
        let lifecycle_only = events.iter().all(|(_, kind)| {
            matches!(
                kind,
                NativeEventKind::ReadyStateChange
                    | NativeEventKind::DomContentLoaded
                    | NativeEventKind::Load
                    | NativeEventKind::PageHide
                    | NativeEventKind::Unload
                    | NativeEventKind::PageShow
                    | NativeEventKind::BeforeUnload
                    | NativeEventKind::HashChange
                    | NativeEventKind::PopState
            )
        });
        if !document_changed && lifecycle_only && scroll_commands.is_empty() {
            let defer = events.iter().all(|(_, kind)| {
                matches!(kind, NativeEventKind::PageHide | NativeEventKind::Unload)
            });
            if defer {
                self.pending_lifecycle_effects.extend(events);
            } else {
                self.record_effects(events);
            }
            let dialog_url = self.url.clone();
            self.install_dialogs(dialogs, &dialog_url)?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        let scroll_events = self.apply_scroll_commands(&self.document.clone(), &scroll_commands)?;
        events.extend(scroll_events);
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.record_effects(events);
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn activate_link(
        &mut self,
        id: super::dom::NativeNodeId,
        href: &str,
        click_already_applied: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let target_url = self.resolve_link_href(href)?;
        let revision = if click_already_applied {
            self.revision
        } else {
            self.next_revision()?
        };
        if let Some(download_attribute) =
            self.document.link_download_attribute(id).map(str::to_owned)
        {
            if self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS {
                return Err(NativeEngineError::limit(
                    "native pending downloads",
                    MAX_NATIVE_PENDING_DOWNLOADS,
                    self.pending_downloads.len().saturating_add(1),
                ));
            }
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            self.document.set_revision(revision);
            self.revision = revision;
            self.history
                .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            self.record_effects(events);
            self.queue_download(target_url, &download_attribute)?;
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
        if self.document.link_opens_new_target(id) {
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            self.document.set_revision(revision);
            self.revision = revision;
            self.history
                .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            self.record_effects(events);
            self.queue_popup(target_url)?;
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
        if !self.allows_frame_navigation(&target_url)? {
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            let revision = if click_already_applied {
                self.revision
            } else {
                self.document.set_revision(revision);
                self.revision = revision;
                self.history
                    .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
                self.record_effects(events);
                return Ok(NativeActionResult {
                    revision,
                    accepted: true,
                });
            };
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
        let resource = self.loader.load(&target_url)?;
        if self.is_same_document_navigation(&resource.url) {
            let scroll_offset = self.fragment_scroll_offset(&resource.url)?;
            self.run_commit_task(
                NativeTask::CommitSameDocumentNavigation,
                "link same-document navigation",
            )?;
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
            self.document.set_revision(revision);
            self.url = resource.url.clone();
            self.scroll_offset = scroll_offset;
            self.sync_javascript_scroll_offset();
            self.revision = revision;
            self.history.push_same_document(
                resource.url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
            );
            self.sync_javascript_history();
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }

        let prepared = self.prepare_navigation_resource_at_revision(resource, revision)?;
        let scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task(NativeTask::CommitNavigation, "link navigation")?;
        if !click_already_applied {
            let _events = self.document.apply_click(id)?;
        }
        self.document = prepared.document;
        self.javascript = None;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.scroll_offset = scroll_offset;
        self.sync_javascript_scroll_offset();
        self.revision = revision;
        self.history.push(self.url.clone(), revision, scroll_offset);
        Ok(NativeActionResult {
            revision,
            accepted: true,
        })
    }

    fn resolve_link_href(&self, href: &str) -> Result<String, NativeEngineError> {
        validate_url_text("link href", href)?;
        if let Some(fragment) = href.strip_prefix('#') {
            let target = format!("{}#{fragment}", without_fragment(&self.url));
            validate_url_text("link target URL", &target)?;
            return Ok(target);
        }
        if url::Url::parse(href).is_ok() {
            return Ok(href.to_owned());
        }
        if let Ok(base) = url::Url::parse(without_fragment(&self.url))
            && is_network_url(base.as_str())
        {
            let resolved = base
                .join(href)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "relative network link reference is malformed".into(),
                })?;
            let resolved = resolved.to_string();
            validate_url_text("link target URL", &resolved)?;
            return Ok(resolved);
        }
        resolve_fixture_relative_url(&self.url, href)
    }

    fn resolve_history_href_from(
        &self,
        base_url: &str,
        href: &str,
    ) -> Result<String, NativeEngineError> {
        validate_url_text("history URL", href)?;
        let target = self.resolve_page_navigation_href(base_url, href)?;
        let base = url::Url::parse(base_url).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "history base URL is malformed".into(),
        })?;
        let target_url =
            url::Url::parse(&target).map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "history target URL is malformed".into(),
            })?;
        let target_origin = NativeOrigin::from_url(&target_url)?;
        let same_origin = if self.origin == NativeOrigin::Opaque {
            target_origin == NativeOrigin::Opaque
                && base.scheme() == target_url.scheme()
                && base.host_str() == target_url.host_str()
                && base.port_or_known_default() == target_url.port_or_known_default()
        } else {
            target_origin == self.origin
        };
        if !same_origin {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "history URL must be same-origin".into(),
            });
        }
        Ok(target)
    }

    fn resolve_page_navigation_href(
        &self,
        base_url: &str,
        href: &str,
    ) -> Result<String, NativeEngineError> {
        validate_url_text("page navigation href", href)?;
        if let Some(fragment) = href.strip_prefix('#') {
            let target = format!("{}#{fragment}", without_fragment(base_url));
            validate_url_text("page navigation target URL", &target)?;
            return Ok(target);
        }
        if url::Url::parse(href).is_ok() {
            return Ok(href.to_owned());
        }
        if let Ok(base) = url::Url::parse(without_fragment(base_url))
            && is_network_url(base.as_str())
        {
            let resolved = base
                .join(href)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "relative page navigation reference is malformed".into(),
                })?;
            let resolved = resolved.to_string();
            validate_url_text("page navigation target URL", &resolved)?;
            return Ok(resolved);
        }
        resolve_fixture_relative_url(base_url, href)
    }

    fn content_navigation_request(
        &self,
        navigation: NativeContentNavigation,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        if !navigation.location {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "content lifecycle navigation must target the current browsing context"
                    .into(),
            });
        }
        let mut request = NativeNavigationRequest::get(
            self.resolve_page_navigation_href(&self.url, &navigation.href)?,
        );
        request.replace_history = navigation.replace_history;
        Ok(request)
    }

    fn page_navigation_request(
        &self,
        navigation: NativePageNavigation,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        let mut request = NativeNavigationRequest::get(
            self.resolve_page_navigation_href(&self.url, &navigation.href)?,
        );
        request.replace_history = navigation.replace_history;
        Ok(request)
    }

    fn navigate_page_script_sync(
        &mut self,
        mut navigation: NativeNavigationRequest,
        mut page_navigation_handoffs: usize,
    ) -> Result<(), NativeEngineError> {
        loop {
            if page_navigation_handoffs > MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                return Err(NativeEngineError::limit(
                    "page navigation handoffs",
                    MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                    page_navigation_handoffs,
                ));
            }
            if is_network_url(&navigation.url) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason:
                        "synchronous page-script navigation requires an asynchronous native owner"
                            .into(),
                });
            }
            let history_commit = if navigation.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            let resource = self.loader.load(&navigation.url)?;
            if self.is_same_document_navigation(&resource.url) {
                let _ = std::mem::take(&mut self.skip_next_navigation_lifecycle);
                if let Some(next_navigation) =
                    self.commit_same_document_navigation(resource.url, history_commit)?
                {
                    navigation = next_navigation;
                    page_navigation_handoffs = page_navigation_handoffs.saturating_add(1);
                    continue;
                }
                return Ok(());
            }
            let prepared = self.prepare_navigation_resource(resource)?;
            let Some(next_navigation) = self.commit_navigation(prepared, history_commit)? else {
                return Ok(());
            };
            navigation = next_navigation;
            page_navigation_handoffs = page_navigation_handoffs.saturating_add(1);
        }
    }

    pub fn effects_since(
        &self,
        since_revision: u64,
    ) -> Result<NativeEffectsSnapshot, NativeEngineError> {
        self.require_running("effects")?;
        if since_revision > self.revision {
            return Err(NativeEngineError::invalid(
                "since revision",
                "since revision cannot exceed current native revision",
            ));
        }
        Ok(NativeEffectsSnapshot {
            revision: self.revision,
            changed: since_revision < self.revision,
            effects: self
                .effects
                .iter()
                .filter(|effect| effect.revision > since_revision)
                .copied()
                .collect(),
        })
    }

    fn snapshot_unchecked(&self) -> NativeEngineSnapshot {
        let (title, title_truncated) = self.document.title(self.config.limits.max_text_bytes);
        let (visible_text, text_truncated) = self
            .document
            .visible_text(self.config.limits.max_text_bytes);
        NativeEngineSnapshot {
            lifecycle: self.lifecycle,
            url: self.url.clone(),
            origin: self.origin.clone(),
            title,
            title_truncated,
            visible_text,
            text_truncated,
            revision: self.revision,
            viewport: self.config.viewport,
        }
    }

    pub fn context(&self) -> Result<NativeBrowsingContext, NativeEngineError> {
        self.require_running("contexts")?;
        Ok(NativeBrowsingContext {
            context_id: self.config.context_id.clone(),
            url: self.url.clone(),
            origin: self.origin.clone(),
            active: true,
        })
    }

    pub fn history(&self) -> &NativeHistory {
        &self.history
    }

    pub fn scheduler(&self) -> Result<DeterministicScheduler, NativeEngineError> {
        self.runtime.scheduler()
    }

    pub fn runtime_state(&self) -> NativeRuntimeState {
        self.runtime.state()
    }

    pub fn runtime_trace(&self) -> Vec<NativeRuntimeTraceEvent> {
        self.runtime.trace()
    }

    fn prepare_navigation(&self, url: &str) -> Result<PreparedNavigation, NativeEngineError> {
        let resource = self.loader.load(url)?;
        self.prepare_navigation_resource(resource)
    }

    async fn prepare_navigation_async(
        &mut self,
        url: &str,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let resource = self.loader.load_async(url).await?;
        self.prepare_navigation_resource(resource)
    }

    fn prepare_navigation_content(
        &self,
        content: NativeContentLoad,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let next_revision = self.next_revision()?;
        let generation = u32::try_from(next_revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let initial_events = content
            .events
            .iter()
            .map(|event| (event.node_index, event.kind))
            .collect();
        let document =
            NativeDocument::from_content_wire(content.document, &self.config.limits, generation)?;
        Ok(PreparedNavigation {
            resource: NativeResource {
                url: content.url,
                origin: content.origin,
                body: String::new(),
            },
            document,
            frame_sources: content.frame_sources,
            dialogs: content.dialogs,
            initial_events,
            initial_scroll_commands: content.scroll_commands,
            execute_inline_scripts: false,
        })
    }

    fn prepare_navigation_resource(
        &self,
        resource: NativeResource,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let next_revision = self.next_revision()?;
        self.prepare_navigation_resource_at_revision(resource, next_revision)
    }

    fn prepare_navigation_resource_at_revision(
        &self,
        resource: NativeResource,
        revision: u64,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let generation = u32::try_from(revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let document =
            NativeDocument::parse_with_generation(&resource.body, &self.config.limits, generation)?;
        let frame_sources = self.loader.frame_sources_for_document(&resource.url)?;
        Ok(PreparedNavigation {
            resource,
            document,
            frame_sources,
            dialogs: Vec::new(),
            initial_events: Vec::new(),
            initial_scroll_commands: Vec::new(),
            execute_inline_scripts: true,
        })
    }

    fn commit_navigation(
        &mut self,
        mut prepared: PreparedNavigation,
        history_commit: HistoryCommit,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let skip_lifecycle = std::mem::take(&mut self.skip_next_navigation_lifecycle);
        let mut initial_events = std::mem::take(&mut prepared.initial_events);
        let mut initial_scroll_commands = std::mem::take(&mut prepared.initial_scroll_commands);
        if !skip_lifecycle && self.javascript.is_some() {
            let (allowed, before_navigation) = self.dispatch_local_before_unload()?;
            if !allowed {
                return Ok(None);
            }
            let lifecycle_navigation = self.dispatch_local_navigation_lifecycle()?;
            if before_navigation.is_some() && lifecycle_navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple outgoing lifecycle navigations are not supported".into(),
                });
            }
            if let Some(navigation) = before_navigation.or(lifecycle_navigation) {
                self.skip_next_navigation_lifecycle = true;
                return Ok(Some(navigation));
            }
        }
        self.persist_local_web_storage()?;
        let storage_state = self.web_storage.clone();
        let cookie = self.loader.document_cookie(&prepared.resource.url)?;
        let mut javascript = if prepared.execute_inline_scripts {
            Some(NativeJavaScriptRuntime::new_with_context_metadata(
                &self.config.context_id,
                &self.config.window_name,
                self.config.opener_context_id.as_deref(),
                &self.config.opener_window_name,
                &self.config.opener_url,
            )?)
        } else {
            None
        };
        if let Some(javascript) = javascript.as_ref() {
            javascript.set_frame_script_bindings(self.frame_script_bindings.clone());
            javascript.set_frame_script_context(self.frame_script_context.clone());
            javascript.set_frame_id(self.frame_id.clone());
            javascript.set_environment(self.environment.clone());
        }
        let mut dialogs = std::mem::take(&mut prepared.dialogs);
        let page_navigation = if prepared.execute_inline_scripts {
            let result = execute_inline_scripts(
                &mut prepared.document,
                &mut javascript,
                &self.config.context_id,
                &prepared.resource.url,
                &prepared.resource.origin,
                self.config.viewport,
                &storage_state,
                &self.indexed_db,
                &cookie,
            )?;
            dialogs.extend(result.dialogs);
            initial_events.extend(result.events);
            initial_scroll_commands.extend(result.scroll_commands);
            let popups = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_popup_events)
                .unwrap_or_default();
            self.queue_popup_requests(popups)?;
            let messages = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_post_message_events)
                .unwrap_or_default();
            self.queue_post_message_requests_from_origin(messages, &prepared.resource.origin)?;
            let window_closes = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_close_events)
                .unwrap_or_default();
            self.queue_window_close_requests(window_closes)?;
            let window_navigations = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_navigation_events)
                .unwrap_or_default();
            self.queue_window_navigation_requests(window_navigations)?;
            if let Some(javascript) = javascript.as_ref() {
                self.config.window_name = javascript.window_name();
            }
            result.navigation
        } else {
            None
        };
        let fragment_scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task(NativeTask::CommitNavigation, "navigation")?;
        let revision = prepared.document.revision();
        self.document = prepared.document;
        self.workers.clear();
        self.pending_message_port_messages.clear();
        self.javascript = javascript;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.scroll_offset = fragment_scroll_offset;
        self.sync_javascript_scroll_offset();
        let _ = self.apply_scroll_commands(&self.document.clone(), &initial_scroll_commands)?;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        match history_commit {
            HistoryCommit::Push => {
                self.history
                    .push(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Replace => {
                self.history
                    .replace_current(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Activate(_) => {
                return Err(NativeEngineError::Scheduler {
                    reason: "full navigation cannot activate a history entry".into(),
                });
            }
        }
        self.sync_javascript_history();
        self.flush_pending_lifecycle_effects();
        self.record_initial_events(initial_events)?;
        if let Some(javascript) = self.javascript.as_mut() {
            javascript.reset_timer_clock();
        }
        let page_navigation = match page_navigation {
            Some(page_navigation) => Some(self.page_navigation_request(page_navigation)?),
            None => self.dispatch_local_page_show()?,
        };
        if let Some(javascript) = self.javascript.as_mut() {
            javascript.reset_timer_clock();
        }
        self.persist_local_web_storage()?;
        Ok(page_navigation)
    }

    async fn commit_navigation_async(
        &mut self,
        mut prepared: PreparedNavigation,
        worker: &NativeRuntimeWorker,
        history_commit: HistoryCommit,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if let HistoryCommit::Activate(history_index) = &history_commit {
            self.commit_history_navigation_async(prepared, *history_index, worker)
                .await?;
            return Ok(None);
        }
        let execute_page_scripts = prepared.execute_inline_scripts;
        let mut initial_events = std::mem::take(&mut prepared.initial_events);
        let mut initial_scroll_commands = std::mem::take(&mut prepared.initial_scroll_commands);
        self.persist_local_web_storage()?;
        let storage_state = self.web_storage.clone();
        let cookie = self.loader.document_cookie(&prepared.resource.url)?;
        let mut javascript = if execute_page_scripts {
            Some(NativeJavaScriptRuntime::new_with_context_metadata(
                &self.config.context_id,
                &self.config.window_name,
                self.config.opener_context_id.as_deref(),
                &self.config.opener_window_name,
                &self.config.opener_url,
            )?)
        } else {
            None
        };
        if let Some(javascript) = javascript.as_ref() {
            javascript.set_frame_script_bindings(self.frame_script_bindings.clone());
            javascript.set_frame_script_context(self.frame_script_context.clone());
            javascript.set_frame_id(self.frame_id.clone());
            javascript.set_environment(self.environment.clone());
        }
        let mut dialogs = std::mem::take(&mut prepared.dialogs);
        let page_navigation = if execute_page_scripts {
            let result = execute_inline_scripts(
                &mut prepared.document,
                &mut javascript,
                &self.config.context_id,
                &prepared.resource.url,
                &prepared.resource.origin,
                self.config.viewport,
                &storage_state,
                &self.indexed_db,
                &cookie,
            )?;
            dialogs.extend(result.dialogs);
            initial_events.extend(result.events);
            initial_scroll_commands.extend(result.scroll_commands);
            let popups = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_popup_events)
                .unwrap_or_default();
            self.queue_popup_requests(popups)?;
            let messages = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_post_message_events)
                .unwrap_or_default();
            self.queue_post_message_requests_from_origin(messages, &prepared.resource.origin)?;
            let window_closes = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_close_events)
                .unwrap_or_default();
            self.queue_window_close_requests(window_closes)?;
            let window_navigations = javascript
                .as_ref()
                .map(NativeJavaScriptRuntime::take_window_navigation_events)
                .unwrap_or_default();
            self.queue_window_navigation_requests(window_navigations)?;
            if let Some(javascript) = javascript.as_ref() {
                self.config.window_name = javascript.window_name();
            }
            result.navigation
        } else {
            None
        };
        let fragment_scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task_async(NativeTask::CommitNavigation, "navigation", worker)
            .await?;
        let revision = prepared.document.revision();
        self.document = prepared.document;
        self.workers.clear();
        self.pending_message_port_messages.clear();
        self.javascript = javascript;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.scroll_offset = fragment_scroll_offset;
        self.sync_javascript_scroll_offset();
        let _ = self.apply_scroll_commands(&self.document.clone(), &initial_scroll_commands)?;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        match history_commit {
            HistoryCommit::Push => {
                self.history
                    .push(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Replace => {
                self.history
                    .replace_current(self.url.clone(), revision, self.scroll_offset)
            }
            HistoryCommit::Activate(_) => {
                return Err(NativeEngineError::Scheduler {
                    reason: "full navigation cannot activate a history entry".into(),
                });
            }
        }
        self.sync_javascript_history();
        self.sync_content_history_async().await?;
        self.flush_pending_lifecycle_effects();
        self.record_initial_events(initial_events)?;
        if execute_page_scripts {
            if let Some(javascript) = self.javascript.as_mut() {
                javascript.reset_timer_clock();
            }
            let page_navigation = match page_navigation {
                Some(page_navigation) => Some(self.page_navigation_request(page_navigation)?),
                None => self.dispatch_local_page_show()?,
            };
            if let Some(javascript) = self.javascript.as_mut() {
                javascript.reset_timer_clock();
            }
            self.persist_local_web_storage()?;
            return Ok(page_navigation);
        }
        Ok(None)
    }

    /// Clamp a history entry's saved scroll state against the document being
    /// activated. Same-document mutations can remove a scroller, and a full
    /// resource traversal can rebuild the DOM with different bounds; both
    /// cases should restore the usable portion of the saved state instead of
    /// making the next layout fail.
    fn restore_history_scroll_state(
        &self,
        document: &NativeDocument,
        saved_scroll: NativePoint,
        saved_nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) -> Result<(NativePoint, BTreeMap<u32, NativePoint>), NativeEngineError> {
        let layout = document.layout(self.config.viewport)?;
        let max_scroll = layout.max_scroll_offset();
        let scroll_offset = NativePoint {
            x: saved_scroll.x.min(max_scroll.x),
            y: saved_scroll.y.min(max_scroll.y),
        };
        let mut nested_scroll_offsets = BTreeMap::new();
        for (&node_index, &saved_offset) in saved_nested_scroll_offsets {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            let Some(container) = layout.scroll_container_for(node_id) else {
                continue;
            };
            let max_scroll = container.max_scroll_offset();
            let offset = NativePoint {
                x: saved_offset.x.min(max_scroll.x),
                y: saved_offset.y.min(max_scroll.y),
            };
            if offset.x != 0 || offset.y != 0 {
                nested_scroll_offsets.insert(node_index, offset);
            }
        }
        Ok((scroll_offset, nested_scroll_offsets))
    }

    fn commit_same_document_navigation(
        &mut self,
        url: String,
        history_commit: HistoryCommit,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let old_url = self.url.clone();
        let (saved_scroll, saved_nested_scroll_offsets) = match &history_commit {
            HistoryCommit::Push | HistoryCommit::Replace => (
                self.fragment_scroll_offset(&url)?,
                self.nested_scroll_offsets.clone(),
            ),
            HistoryCommit::Activate(index) => {
                let entry =
                    self.history
                        .entry(*index)
                        .ok_or_else(|| NativeEngineError::Scheduler {
                            reason: "history target is no longer available".into(),
                        })?;
                (entry.scroll_offset, entry.nested_scroll_offsets.clone())
            }
        };
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &self.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        self.run_commit_task(
            NativeTask::CommitSameDocumentNavigation,
            "same-document navigation",
        )?;
        let revision = self.next_revision()?;
        self.document.set_revision(revision);
        self.url = url.clone();
        self.scroll_offset = scroll_offset;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.sync_javascript_scroll_offset();
        self.revision = revision;
        let traversing_history = matches!(&history_commit, HistoryCommit::Activate(_));
        match history_commit {
            HistoryCommit::Push => self.history.push_same_document(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
            ),
            HistoryCommit::Replace => self.history.replace_current_with_state(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
                serde_json::Value::Null,
                true,
            ),
            HistoryCommit::Activate(index) => {
                self.history.activate(index, revision).ok_or_else(|| {
                    NativeEngineError::Scheduler {
                        reason: "history entry disappeared during same-document traversal".into(),
                    }
                })?;
                self.history
                    .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            }
        }
        self.sync_javascript_history();
        let mut navigation = if traversing_history {
            self.dispatch_local_pop_state()?
        } else {
            None
        };
        let hash_navigation =
            if old_url != self.url && without_fragment(&old_url) == without_fragment(&self.url) {
                self.dispatch_local_hash_change(&old_url, &self.url.clone())?
            } else {
                None
            };
        if let Some(next_navigation) = hash_navigation {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple same-document lifecycle navigations are not supported".into(),
                });
            }
            navigation = Some(next_navigation);
        }
        self.persist_local_web_storage()?;
        Ok(navigation)
    }

    async fn commit_same_document_navigation_async(
        &mut self,
        url: String,
        history_commit: HistoryCommit,
        worker: &NativeRuntimeWorker,
        page_navigation_handoffs: usize,
    ) -> Result<(), NativeEngineError> {
        let old_url = self.url.clone();
        let (saved_scroll, saved_nested_scroll_offsets) = match &history_commit {
            HistoryCommit::Push | HistoryCommit::Replace => (
                self.fragment_scroll_offset(&url)?,
                self.nested_scroll_offsets.clone(),
            ),
            HistoryCommit::Activate(index) => {
                let entry =
                    self.history
                        .entry(*index)
                        .ok_or_else(|| NativeEngineError::Scheduler {
                            reason: "history target is no longer available".into(),
                        })?;
                (entry.scroll_offset, entry.nested_scroll_offsets.clone())
            }
        };
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &self.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        self.run_commit_task_async(
            NativeTask::CommitSameDocumentNavigation,
            "same-document navigation",
            worker,
        )
        .await?;
        let revision = self.next_revision()?;
        self.document.set_revision(revision);
        self.url = url.clone();
        self.scroll_offset = scroll_offset;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.sync_javascript_scroll_offset();
        self.revision = revision;
        let traversing_history = matches!(&history_commit, HistoryCommit::Activate(_));
        match history_commit {
            HistoryCommit::Push => self.history.push_same_document(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
            ),
            HistoryCommit::Replace => self.history.replace_current_with_state(
                url,
                revision,
                scroll_offset,
                &self.nested_scroll_offsets,
                serde_json::Value::Null,
                true,
            ),
            HistoryCommit::Activate(index) => {
                self.history.activate(index, revision).ok_or_else(|| {
                    NativeEngineError::Scheduler {
                        reason: "history entry disappeared during same-document traversal".into(),
                    }
                })?;
                self.history
                    .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
            }
        }
        self.sync_javascript_history();
        self.sync_content_scroll_offsets_async().await?;
        self.sync_content_history_async().await?;
        let mut navigation = if traversing_history {
            self.dispatch_content_events_async(&[NativeEventKind::PopState])
                .await?
        } else {
            None
        };
        let new_url = self.url.clone();
        let hash_navigation =
            if old_url != new_url && without_fragment(&old_url) == without_fragment(&new_url) {
                if self
                    .content_process
                    .as_mut()
                    .is_some_and(NativeContentProcess::refresh_health)
                {
                    self.dispatch_content_hash_change_async(&old_url, &new_url)
                        .await?
                } else {
                    self.dispatch_local_hash_change(&old_url, &new_url)?
                }
            } else {
                None
            };
        if let Some(next_navigation) = hash_navigation {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple same-document lifecycle navigations are not supported".into(),
                });
            }
            navigation = Some(next_navigation);
        }
        self.persist_local_web_storage()?;
        if let Some(navigation) = navigation {
            if page_navigation_handoffs >= MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
                return Err(NativeEngineError::limit(
                    "page navigation handoffs",
                    MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                    page_navigation_handoffs.saturating_add(1),
                ));
            }
            Box::pin(self.navigate_script_navigation_async(
                NativeContentNavigation {
                    node_index: 0,
                    href: navigation.url,
                    submitter_node_index: None,
                    location: true,
                    replace_history: navigation.replace_history,
                },
                page_navigation_handoffs + 1,
            ))
            .await?;
        }
        Ok(())
    }

    fn commit_history_navigation(
        &mut self,
        prepared: PreparedNavigation,
        history_index: usize,
    ) -> Result<(), NativeEngineError> {
        self.persist_local_web_storage()?;
        if self.history.entry(history_index).is_none() {
            return Err(NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            });
        }
        let (saved_scroll, saved_nested_scroll_offsets) = self
            .history
            .entry(history_index)
            .map(|entry| (entry.scroll_offset, entry.nested_scroll_offsets.clone()))
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &prepared.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        let dialogs = prepared.dialogs;
        self.run_commit_task(NativeTask::TraverseHistory, "history traversal")?;
        let revision = prepared.document.revision();
        self.document = prepared.document;
        self.javascript = None;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        self.history
            .activate(history_index, revision)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target disappeared during traversal".into(),
            })?;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        Ok(())
    }

    async fn commit_history_navigation_async(
        &mut self,
        prepared: PreparedNavigation,
        history_index: usize,
        worker: &NativeRuntimeWorker,
    ) -> Result<(), NativeEngineError> {
        self.persist_local_web_storage()?;
        if self.history.entry(history_index).is_none() {
            return Err(NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            });
        }
        let (saved_scroll, saved_nested_scroll_offsets) = self
            .history
            .entry(history_index)
            .map(|entry| (entry.scroll_offset, entry.nested_scroll_offsets.clone()))
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let (scroll_offset, nested_scroll_offsets) = self.restore_history_scroll_state(
            &prepared.document,
            saved_scroll,
            &saved_nested_scroll_offsets,
        )?;
        let dialogs = prepared.dialogs;
        self.run_commit_task_async(NativeTask::TraverseHistory, "history traversal", worker)
            .await?;
        let revision = prepared.document.revision();
        self.document = prepared.document;
        self.javascript = None;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.pending_dialogs.clear();
        let dialog_url = self.url.clone();
        self.install_dialogs(dialogs, &dialog_url)?;
        self.history
            .activate(history_index, revision)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target disappeared during traversal".into(),
            })?;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        self.sync_content_scroll_offsets_async().await?;
        self.sync_content_history_async().await?;
        Ok(())
    }

    fn traverse_history(
        &mut self,
        direction: NativeHistoryDirection,
        operation: &str,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.require_running(operation)?;
        let Some(history_index) = self.history.target_index(direction) else {
            return Ok(None);
        };
        let target_url = self
            .history
            .entry(history_index)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?
            .url
            .clone();
        if self.history.is_same_document(history_index) {
            if let Some(navigation) = self.commit_same_document_navigation(
                target_url,
                HistoryCommit::Activate(history_index),
            )? {
                self.navigate_page_script_sync(navigation, 1)?;
            }
            return Ok(Some(self.snapshot_unchecked()));
        }
        let resource = self.loader.load(&target_url)?;
        if self.is_same_document_navigation(&resource.url) {
            if let Some(navigation) = self.commit_same_document_navigation(
                resource.url,
                HistoryCommit::Activate(history_index),
            )? {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else if !self.allows_frame_navigation(&resource.url)? {
            return Ok(Some(self.snapshot_unchecked()));
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            self.commit_history_navigation(prepared, history_index)?;
        }
        Ok(Some(self.snapshot_unchecked()))
    }

    fn traverse_history_delta(&mut self, delta: i32) -> Result<(), NativeEngineError> {
        let steps = delta.unsigned_abs() as usize;
        if steps > MAX_NATIVE_HISTORY_DELTA as usize {
            return Err(NativeEngineError::limit(
                "native history traversal delta",
                MAX_NATIVE_HISTORY_DELTA as usize,
                steps,
            ));
        }
        let direction = if delta < 0 {
            NativeHistoryDirection::Back
        } else {
            NativeHistoryDirection::Forward
        };
        for _ in 0..steps {
            if self
                .traverse_history(direction, "history traversal")?
                .is_none()
            {
                break;
            }
        }
        Ok(())
    }

    async fn traverse_history_async(
        &mut self,
        direction: NativeHistoryDirection,
        operation: &str,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.require_running(operation)?;
        let Some(history_index) = self.history.target_index(direction) else {
            return Ok(None);
        };
        let target_url = self
            .history
            .entry(history_index)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?
            .url
            .clone();
        if self.history.is_same_document(history_index)
            || self.is_same_document_navigation(&target_url)
        {
            if let Some(worker) = self.runtime_worker.clone() {
                self.commit_same_document_navigation_async(
                    target_url,
                    HistoryCommit::Activate(history_index),
                    &worker,
                    0,
                )
                .await?;
            } else {
                if let Some(navigation) = self.commit_same_document_navigation(
                    target_url,
                    HistoryCommit::Activate(history_index),
                )? {
                    self.navigate_page_script_sync(navigation, 1)?;
                }
            }
            return Ok(Some(self.snapshot_unchecked()));
        }

        let history_commit = HistoryCommit::Activate(history_index);
        if !self.allows_frame_navigation(&target_url)? {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if is_network_url(&target_url) {
            let referrer = referrer_for_navigation(&self.url, &target_url)?;
            self.ensure_content_process().await?;
            let Some((content, history_commit, page_navigation_handoffs)) = self
                .load_content_with_page_navigation(
                    NativeNavigationRequest::get(target_url),
                    referrer,
                    history_commit,
                    0,
                )
                .await?
            else {
                return Ok(Some(self.snapshot_unchecked()));
            };
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation(&content.url)?
            {
                return Ok(Some(self.snapshot_unchecked()));
            }
            self.commit_content_process().await?;
            if let Some(worker) = self.runtime_worker.clone() {
                return Ok(Some(
                    self.navigate_content_async(
                        content,
                        &worker,
                        history_commit,
                        page_navigation_handoffs,
                    )
                    .await?,
                ));
            }
            return Ok(Some(self.navigate_content(content, history_commit)?));
        }

        self.content_process.take();
        let resource = self.loader.load_async(&target_url).await?;
        if let Some(worker) = self.runtime_worker.clone() {
            return Ok(Some(
                self.navigate_resource_async(resource, &worker, history_commit, 0)
                    .await?,
            ));
        }
        Ok(Some(self.navigate_resource(resource, history_commit)?))
    }

    async fn traverse_history_delta_async(&mut self, delta: i32) -> Result<(), NativeEngineError> {
        let steps = delta.unsigned_abs() as usize;
        if steps > MAX_NATIVE_HISTORY_DELTA as usize {
            return Err(NativeEngineError::limit(
                "native history traversal delta",
                MAX_NATIVE_HISTORY_DELTA as usize,
                steps,
            ));
        }
        let direction = if delta < 0 {
            NativeHistoryDirection::Back
        } else {
            NativeHistoryDirection::Forward
        };
        for _ in 0..steps {
            if self
                .traverse_history_async(direction, "history traversal")
                .await?
                .is_none()
            {
                break;
            }
        }
        Ok(())
    }

    fn is_same_document_navigation(&self, target_url: &str) -> bool {
        self.url != target_url && without_fragment(&self.url) == without_fragment(target_url)
    }

    fn fragment_scroll_offset(&self, target_url: &str) -> Result<NativePoint, NativeEngineError> {
        self.fragment_scroll_offset_for_document(&self.document, target_url, self.scroll_offset)
    }

    fn fragment_scroll_offset_for_document(
        &self,
        document: &NativeDocument,
        target_url: &str,
        fallback: NativePoint,
    ) -> Result<NativePoint, NativeEngineError> {
        let Some((_, fragment)) = target_url.split_once('#') else {
            return Ok(fallback);
        };
        if fragment.is_empty() {
            return Ok(fallback);
        }
        let layout = document.layout(self.config.viewport)?;
        let target_id = if fragment.starts_with(":~:text=") {
            let Some(terms) = decode_text_fragment_terms(fragment) else {
                return Ok(fallback);
            };
            document.text_fragment_target(&layout, &terms)
        } else {
            let Some(decoded_fragment) = decode_percent_encoded_fragment(fragment) else {
                return Ok(fallback);
            };
            document.fragment_target(&decoded_fragment)
        };
        let Some(target_id) = target_id else {
            return Ok(fallback);
        };
        let Some(target_box) = layout.box_for(target_id) else {
            return Ok(fallback);
        };
        if target_box.width == 0 || target_box.height == 0 {
            return Ok(fallback);
        }
        Ok(NativePoint {
            x: 0,
            y: target_box.y.min(layout.max_scroll_offset().y),
        })
    }

    fn run_commit_task(
        &mut self,
        expected: NativeTask,
        operation: &str,
    ) -> Result<(), NativeEngineError> {
        self.runtime.schedule(expected, 0)?;
        let Some(task) = self.runtime.pop_ready()? else {
            return Err(NativeEngineError::Scheduler {
                reason: format!("{operation} commit was not ready at the current logical time"),
            });
        };
        if task.task != expected {
            return Err(NativeEngineError::Scheduler {
                reason: format!("{operation} produced an unexpected task kind"),
            });
        }
        Ok(())
    }

    async fn run_commit_task_async(
        &mut self,
        expected: NativeTask,
        operation: &str,
        worker: &NativeRuntimeWorker,
    ) -> Result<(), NativeEngineError> {
        worker.schedule(expected, 0).await?;
        let Some(task) = worker.pop_ready().await? else {
            return Err(NativeEngineError::Scheduler {
                reason: format!("{operation} commit was not ready at the current logical time"),
            });
        };
        if task.task != expected {
            return Err(NativeEngineError::Scheduler {
                reason: format!("{operation} produced an unexpected task kind"),
            });
        }
        Ok(())
    }

    fn next_revision(&self) -> Result<u64, NativeEngineError> {
        self.revision.checked_add(1).ok_or_else(|| {
            NativeEngineError::limit("document revisions", u64::MAX as usize, usize::MAX)
        })
    }

    fn resolve_click_target(
        &self,
        target: &str,
    ) -> Result<super::dom::NativeNodeId, NativeEngineError> {
        let Some((x, y)) = parse_point_target(target)? else {
            return self.document.resolve_target(target);
        };
        let hit = self.layout()?.hit_test(x, y)?;
        let hit = hit.ok_or_else(|| NativeEngineError::TargetNotActionable {
            reason: "point hit no visible element".into(),
        })?;
        self.document
            .nearest_clickable_ancestor(hit)
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "point hit no actionable semantic control".into(),
            })
    }

    fn require_layout_actionable(
        &self,
        id: super::dom::NativeNodeId,
    ) -> Result<(), NativeEngineError> {
        if self.document.is_hidden_for_layout(id) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "hidden targets are not actionable".into(),
            });
        }
        let layout = self.layout()?;
        let visible = layout
            .viewport_rect_for(id)
            .is_some_and(|rect| rect.width > 0 && rect.height > 0);
        if visible {
            return Ok(());
        }
        Err(NativeEngineError::TargetNotActionable {
            reason: "target has no visible layout box in the native viewport".into(),
        })
    }

    fn record_effects(&mut self, events: Vec<(super::dom::NativeNodeId, NativeEventKind)>) {
        for (node_id, kind) in events {
            while self.effects.len() >= MAX_NATIVE_EFFECTS {
                self.effects.pop_front();
            }
            self.effects.push_back(NativeEffect {
                revision: self.revision,
                node_id,
                kind,
            });
        }
    }

    fn record_initial_events(
        &mut self,
        events: Vec<(u32, NativeEventKind)>,
    ) -> Result<(), NativeEngineError> {
        let generation = self.document.generation();
        let events = events
            .into_iter()
            .map(|(node_index, kind)| {
                let node_id = NativeNodeId::from_parts(generation, node_index);
                if node_index != u32::MAX && self.document.node(node_id).is_none() {
                    return Err(NativeEngineError::DetachedTarget);
                }
                Ok((node_id, kind))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.record_effects(events);
        Ok(())
    }

    fn flush_pending_lifecycle_effects(&mut self) {
        let effects = std::mem::take(&mut self.pending_lifecycle_effects);
        self.record_effects(effects);
    }

    fn apply_scroll_commands(
        &mut self,
        document: &NativeDocument,
        commands: &[NativeScriptCommand],
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        let layout = document.layout(self.config.viewport)?;
        let mut root_offset = self.scroll_offset;
        let mut nested_offsets = self.nested_scroll_offsets.clone();
        let mut events = Vec::new();
        for command in commands {
            let NativeScriptCommand::ScrollTo {
                node_index,
                left,
                top,
            } = command
            else {
                continue;
            };
            if *node_index == 0 {
                let max_scroll = layout.max_scroll_offset();
                let next = NativePoint {
                    x: clamp_script_scroll(*left, max_scroll.x),
                    y: clamp_script_scroll(*top, max_scroll.y),
                };
                if next != root_offset {
                    root_offset = next;
                    events.push((
                        NativeNodeId::from_parts(document.generation(), u32::MAX),
                        NativeEventKind::Scroll,
                    ));
                }
                continue;
            }
            let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
            let Some(container) = layout.scroll_container_for(node_id) else {
                continue;
            };
            let max_scroll = container.max_scroll_offset();
            let next = NativePoint {
                x: clamp_script_scroll(*left, max_scroll.x),
                y: clamp_script_scroll(*top, max_scroll.y),
            };
            let current = nested_offsets
                .get(node_index)
                .copied()
                .unwrap_or(NativePoint { x: 0, y: 0 });
            if next == current {
                continue;
            }
            if next.x == 0 && next.y == 0 {
                nested_offsets.remove(node_index);
            } else {
                nested_offsets.insert(*node_index, next);
            }
            events.push((node_id, NativeEventKind::Scroll));
        }
        self.scroll_offset = root_offset;
        self.nested_scroll_offsets = nested_offsets;
        self.sync_javascript_scroll_offset();
        Ok(events)
    }

    fn apply_scroll(&mut self, delta_x: i32, delta_y: i32) -> Result<bool, NativeEngineError> {
        if delta_x == 0 && delta_y == 0 {
            return Ok(false);
        }
        let max_scroll = self
            .document
            .layout(self.config.viewport)?
            .max_scroll_offset();
        let requested_x = i64::from(self.scroll_offset.x).saturating_add(i64::from(delta_x));
        let next_x = requested_x.clamp(0, i64::from(max_scroll.x));
        let next_x = u32::try_from(next_x).map_err(|_| {
            NativeEngineError::invalid("native scroll action", "scroll offset exceeds bounds")
        })?;
        let requested_y = i64::from(self.scroll_offset.y).saturating_add(i64::from(delta_y));
        let next_y = requested_y.clamp(0, i64::from(max_scroll.y));
        let next_y = u32::try_from(next_y).map_err(|_| {
            NativeEngineError::invalid("native scroll action", "scroll offset exceeds bounds")
        })?;
        if next_x == self.scroll_offset.x && next_y == self.scroll_offset.y {
            return Ok(false);
        }
        self.scroll_offset = NativePoint {
            x: next_x,
            y: next_y,
        };
        self.sync_javascript_scroll_offset();
        Ok(true)
    }

    fn sync_javascript_scroll_offset(&self) {
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_scroll_offset(self.scroll_offset);
            javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        }
    }

    fn sync_javascript_history(&self) {
        if let Some(javascript) = self.javascript.as_ref() {
            let state = self
                .history
                .current()
                .map(|entry| entry.state.clone())
                .unwrap_or(serde_json::Value::Null);
            javascript.set_history_state(state);
            javascript.set_history_length(self.history.len());
        }
    }

    fn require_running(&self, operation: &str) -> Result<(), NativeEngineError> {
        if self.lifecycle == NativeLifecycleState::Running {
            return Ok(());
        }
        Err(self.lifecycle_error(operation, "initialize the native engine first"))
    }

    fn lifecycle_error(&self, operation: &str, reason: &str) -> NativeEngineError {
        NativeEngineError::Lifecycle {
            operation: operation.into(),
            state: self.lifecycle,
            reason: reason.into(),
        }
    }
}

fn clamp_script_scroll(value: i64, maximum: u32) -> u32 {
    if value <= 0 {
        0
    } else {
        u32::try_from(value).unwrap_or(u32::MAX).min(maximum)
    }
}

fn scroll_axis_into_view(
    start: u32,
    end: u32,
    viewport_start: u32,
    viewport_end: u32,
    maximum: u32,
) -> u32 {
    let viewport_extent = viewport_end.saturating_sub(viewport_start);
    let target_extent = end.saturating_sub(start);
    let requested = if target_extent >= viewport_extent || start < viewport_start {
        start
    } else if end > viewport_end {
        end.saturating_sub(viewport_end.saturating_sub(viewport_start))
    } else {
        viewport_start
    };
    requested.min(maximum)
}

struct PreparedNavigation {
    resource: NativeResource,
    document: NativeDocument,
    frame_sources: Option<Vec<String>>,
    dialogs: Vec<NativeDialog>,
    initial_events: Vec<(u32, NativeEventKind)>,
    initial_scroll_commands: Vec<NativeScriptCommand>,
    execute_inline_scripts: bool,
}

fn native_download_filename(download_attribute: &str, url: &str) -> String {
    let candidate = if download_attribute.trim().is_empty() {
        url::Url::parse(without_fragment(url))
            .ok()
            .and_then(|url| {
                url.path_segments().and_then(|mut segments| {
                    segments
                        .rfind(|segment| !segment.is_empty())
                        .map(str::to_owned)
                })
            })
            .unwrap_or_else(|| "download".into())
    } else {
        download_attribute.trim().to_owned()
    };
    let candidate = candidate.split(['/', '\\']).next_back().unwrap_or_default();
    let mut sanitized = String::new();
    for character in candidate.chars() {
        if character.is_control() || matches!(character, ':' | '/' | '\\') {
            continue;
        }
        if sanitized.len().saturating_add(character.len_utf8()) > MAX_NATIVE_DOWNLOAD_FILENAME_BYTES
        {
            break;
        }
        sanitized.push(character);
    }
    let sanitized = sanitized.trim().trim_matches('.');
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "download".into()
    } else {
        sanitized.into()
    }
}

async fn write_native_download(
    destination: &Path,
    filename: &str,
    bytes: &[u8],
) -> Result<PathBuf, NativeEngineError> {
    for suffix in 0..100_u16 {
        let candidate_name = if suffix == 0 {
            filename.to_owned()
        } else {
            let path = Path::new(filename);
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("download");
            let extension = path.extension().and_then(|value| value.to_str());
            match extension {
                Some(extension) if !extension.is_empty() => {
                    format!("{stem} ({suffix}).{extension}")
                }
                _ => format!("{stem} ({suffix})"),
            }
        };
        let path = destination.join(candidate_name);
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await;
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(NativeEngineError::Network {
                    operation: "download write".into(),
                    reason: error.to_string(),
                });
            }
        };
        if let Err(error) = file.write_all(bytes).await {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(NativeEngineError::Network {
                operation: "download write".into(),
                reason: error.to_string(),
            });
        }
        if let Err(error) = file.sync_all().await {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(NativeEngineError::Network {
                operation: "download write".into(),
                reason: error.to_string(),
            });
        }
        return Ok(path);
    }
    Err(NativeEngineError::limit(
        "download filename collisions",
        100,
        100,
    ))
}

enum ScriptNavigationTarget {
    Link {
        href: String,
        popup: bool,
    },
    Form {
        form_id: NativeNodeId,
        dispatch_submit: bool,
        submitter: Option<NativeNodeId>,
    },
    Location {
        href: String,
        replace_history: bool,
    },
}

enum LocalHistoryCommand {
    PushState {
        url: String,
        state: serde_json::Value,
    },
    ReplaceState {
        url: String,
        state: serde_json::Value,
    },
    Go(i32),
}

fn extract_local_history_commands(commands: &[NativeScriptCommand]) -> Vec<NativeScriptCommand> {
    commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                NativeScriptCommand::HistoryPushState { .. }
                    | NativeScriptCommand::HistoryReplaceState { .. }
                    | NativeScriptCommand::HistoryGo { .. }
            )
        })
        .cloned()
        .collect()
}

fn extract_local_scroll_commands(commands: &[NativeScriptCommand]) -> Vec<NativeScriptCommand> {
    commands
        .iter()
        .filter(|command| matches!(command, NativeScriptCommand::ScrollTo { .. }))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum HistoryCommit {
    Push,
    Replace,
    Activate(usize),
}

fn validate_history_state(state: &serde_json::Value) -> Result<(), NativeEngineError> {
    let encoded = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native history state".into(),
        reason: "history state could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_HISTORY_STATE_BYTES {
        return Err(NativeEngineError::limit(
            "native history state",
            MAX_NATIVE_HISTORY_STATE_BYTES,
            encoded.len(),
        ));
    }
    Ok(())
}

pub(crate) fn parse_point_target(target: &str) -> Result<Option<(i64, i64)>, NativeEngineError> {
    let Some(value) = target.strip_prefix("point=") else {
        return Ok(None);
    };
    let Some((x, y)) = value.split_once(',') else {
        return Err(NativeEngineError::invalid(
            "action locator",
            "point target must use point=<unsigned-x>,<unsigned-y>",
        ));
    };
    if x.is_empty() || y.is_empty() || y.contains(',') {
        return Err(NativeEngineError::invalid(
            "action locator",
            "point target must use point=<unsigned-x>,<unsigned-y>",
        ));
    }
    let x = x.parse::<u32>().map_err(|_| {
        NativeEngineError::invalid(
            "action locator",
            "point coordinates must be unsigned integers",
        )
    })?;
    let y = y.parse::<u32>().map_err(|_| {
        NativeEngineError::invalid(
            "action locator",
            "point coordinates must be unsigned integers",
        )
    })?;
    Ok(Some((i64::from(x), i64::from(y))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized_engine() -> NativeEngine {
        let config = NativeEngineConfig::default()
            .with_initial_url("fixture://preflight.test/index")
            .with_fixture(
                "fixture://preflight.test/index",
                r#"
                    <button id="save">Save</button>
                    <button id="hidden" hidden>Hidden</button>
                    <button id="disabled" disabled>Disabled</button>
                    <input id="readonly" readonly>
                    <button>Duplicate</button>
                    <button>Duplicate</button>
                "#,
            )
            .expect("preflight fixture must validate");
        let mut engine = NativeEngine::new(config).expect("native engine must construct");
        engine.initialize().expect("native engine must initialize");
        engine
    }

    #[test]
    fn preflight_resolves_current_semantics_and_geometry_without_mutation() {
        let engine = initialized_engine();
        let revision = engine.revision();
        let result = engine
            .preflight_target("id=save", NativePreflightAction::Click)
            .expect("button preflight must succeed");

        assert_eq!(result.action, NativePreflightAction::Click);
        assert!(result.unique);
        assert_eq!(result.actionable, Some(true));
        assert!(result.actionability_reason.is_none());
        assert_eq!(result.error_kind, None);
        assert_eq!(result.revision, revision);
        assert!(
            result
                .geometry
                .is_some_and(|rect| rect.width > 0 && rect.height > 0)
        );
        assert!(!result.likely_navigation);
        assert!(!result.likely_form_submit);
        assert_eq!(engine.revision(), revision);
    }

    #[test]
    fn preflight_reports_actionability_and_resolution_failures() {
        let engine = initialized_engine();

        let hidden = engine
            .preflight_target("id=hidden", NativePreflightAction::Click)
            .expect("hidden target should produce a preflight result");
        assert_eq!(hidden.actionable, Some(false));
        assert_eq!(
            hidden.actionability_reason,
            Some(NativeActionabilityReason::NotVisible)
        );

        let disabled = engine
            .preflight_target("id=disabled", NativePreflightAction::Click)
            .expect("disabled target should produce a preflight result");
        assert_eq!(disabled.actionable, Some(false));
        assert_eq!(
            disabled.actionability_reason,
            Some(NativeActionabilityReason::Disabled)
        );

        let read_only = engine
            .preflight_target("id=readonly", NativePreflightAction::Type)
            .expect("read-only target should produce a preflight result");
        assert_eq!(read_only.actionable, Some(false));
        assert_eq!(
            read_only.actionability_reason,
            Some(NativeActionabilityReason::ReadOnly)
        );

        let ambiguous = engine
            .preflight_target("name=Duplicate", NativePreflightAction::Click)
            .expect("ambiguous target should produce a preflight result");
        assert!(!ambiguous.unique);
        assert_eq!(ambiguous.error_kind, Some(NativeTargetErrorKind::Ambiguous));

        let missing = engine
            .preflight_target("id=missing", NativePreflightAction::Click)
            .expect("missing target should produce a preflight result");
        assert!(!missing.unique);
        assert_eq!(missing.error_kind, Some(NativeTargetErrorKind::NotFound));
    }

    #[test]
    fn preflight_rejects_stale_revision_references_without_mutation() {
        let config = NativeEngineConfig::default()
            .with_initial_url("fixture://preflight.test/one")
            .with_fixture(
                "fixture://preflight.test/one",
                "<button id='one'>One</button>",
            )
            .and_then(|config| {
                config.with_fixture(
                    "fixture://preflight.test/two",
                    "<button id='two'>Two</button>",
                )
            })
            .expect("stale-reference fixtures must validate");
        let mut engine = NativeEngine::new(config).expect("native engine must construct");
        engine.initialize().expect("native engine must initialize");
        let reference = engine
            .semantic_nodes()
            .expect("semantic nodes must be available")
            .into_iter()
            .find(|node| node.tag_name == "button")
            .expect("fixture button must be semantic")
            .reference;
        engine
            .navigate("fixture://preflight.test/two")
            .expect("second fixture navigation must succeed");
        let revision = engine.revision();

        let result = engine
            .preflight_target(&reference, NativePreflightAction::Click)
            .expect("stale reference should produce a preflight result");
        assert!(!result.unique);
        assert_eq!(
            result.error_kind,
            Some(NativeTargetErrorKind::StaleReference)
        );
        assert_eq!(result.revision, revision);
    }
}

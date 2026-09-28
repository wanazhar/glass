use super::browsing_context::NativeBrowsingContext;
use super::cancellation::NativeNavigationCancellation;
use super::config::{
    NativeEngineConfig, Viewport, decode_percent_encoded_fragment, decode_text_fragment_terms,
    is_file_url, is_network_url, resolve_fixture_relative_url, validate_context_id,
    validate_url_text, validate_window_name, without_fragment,
};
use super::content_process::{
    NativeContentLoad, NativeContentLoadResult, NativeContentMutation, NativeContentNavigation,
    NativeContentProcess, NativeContentScriptResult, merge_dynamic_page_script_result,
};
use super::css::{
    FontWeightValue, NativeFontFaceSource, absolutize_stylesheet_urls, css_import_matches,
    decode_css_url_value, static_css_imports,
};
use super::diagnostics::NativeDiagnostic;
use super::dialog::NativeDialogControlPlane;
use super::dom::{
    NativeCheckableKind, NativeDocument, NativeNodeId, NativePageScriptSource,
    NativeScriptDocumentSnapshot,
};
use super::environment::{NativeEnvironmentOverrides, NativeGeolocation, NativeNetworkConditions};
use super::error::NativeEngineError;
use super::error::NativeWorkerFailureKind;
use super::font::{MAX_NATIVE_FONT_FACES, NativeFontBook, NativeFontFaceResource};
use super::history::{NativeHistory, NativeHistoryDirection};
use super::interaction::{
    MAX_NATIVE_EFFECTS, NativeAction, NativeEffect, NativeEventKind, normalize_native_keyboard_key,
    parse_native_shortcut, validate_native_edit_key, validate_native_key,
};
use super::javascript::{
    MAX_NATIVE_DIALOG_TEXT_BYTES, MAX_NATIVE_DIALOGS, MAX_NATIVE_HISTORY_STATE_BYTES,
    MAX_NATIVE_MODULE_IMPORTS, MAX_NATIVE_SCRIPT_BYTES, MAX_NATIVE_WORKER_MESSAGES,
    NativeCookieProfileEntry, NativeDialog, NativeFrameScriptBinding, NativeFrameScriptContext,
    NativeFrameScriptRequest, NativeHashChangeEvent, NativeHostEvent, NativeIndexedDbChange,
    NativeIndexedDbState, NativeJavaScriptRuntime, NativeMessagePortPageMessage,
    NativeMessagePortTransfer, NativePageEventBatch, NativePageMessageEvent,
    NativePageMessagePortCommand, NativePageNavigation, NativePageScript, NativePageScriptResult,
    NativePopupRequest, NativePostMessageRequest, NativeScriptCommand, NativeScriptEvaluation,
    NativeServiceWorkerClientLease, NativeServiceWorkerClientMessage,
    NativeServiceWorkerClientState, NativeServiceWorkerOpenWindowRequest,
    NativeSharedWorkerCreateRequest, NativeStorageEvent, NativeWebStorageState,
    NativeWindowCloseRequest, NativeWindowNavigationRequest, NativeWindowProxyUpdate,
    NativeWorkerRegistry, append_storage_changes, apply_document_commands_with_font_face_ack,
    apply_indexed_db_changes, diff_indexed_db_changes, execute_dynamic_page_scripts,
    execute_inline_scripts, frame_event_batch, host_click_event_batch_with_modifiers,
    host_event_batch, host_event_batch_at, host_key_event_batch_with_modifiers,
    host_submit_event_batch, load_indexed_db_profile,
    load_local_file_dynamic_module_graph_with_import_map,
    load_local_file_module_graph_with_import_map, load_service_worker_client_leases,
    load_web_storage_profile, new_storage_writer_id, read_storage_event_journal,
    register_storage_reader, resolve_module_request_url, save_web_storage_profile,
    storage_event_cursor, storage_key, unregister_service_worker_client_lease,
    unregister_storage_reader, validate_frame_script_command, validate_message_port_transfers,
    validate_native_message_payload, validate_native_object_url_transfers,
    validate_page_message_port_command, validate_service_worker_client_states,
};
use super::layout::{NativeLayoutSnapshot, NativePoint, NativeRect};
use super::lifecycle::NativeLifecycleState;
use super::module_import_map::NativeModuleImportMap;
use super::origin::NativeOrigin;
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
use super::resource_loader::{
    NativeCspViolation, NativeFetchResponse, NativeModuleResourceType, NativeNavigationMethod,
    NativeNavigationPolicyKind, NativeNavigationRequest, NativeObjectUrlTransfer, NativeResource,
    NativeResourceLoader, csp_sources_allow, csp_sources_allow_for_redirect,
    referrer_for_navigation, validate_target_navigation_payload,
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
use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
const MAX_NATIVE_LOCAL_STYLESHEETS: usize = 16;
const MAX_NATIVE_LOCAL_STYLESHEET_BYTES: usize = 512 * 1024;
const MAX_NATIVE_LOCAL_IMAGES: usize = 64;
const MAX_NATIVE_LOCAL_MEDIA: usize = 64;

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
    initial_url: String,
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
                "Backspace"
                    | "Delete"
                    | "ArrowLeft"
                    | "ArrowRight"
                    | "ArrowUp"
                    | "ArrowDown"
                    | "Home"
                    | "End"
                    | "Tab"
            ))
}

fn modifier_click_opens_new_target(modifiers: u8) -> bool {
    modifiers & (2 | 4 | 8) != 0
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

enum NativeLocalKeyboardNavigation {
    Link {
        target: NativeNodeId,
        href: String,
    },
    FormSubmit {
        form_id: NativeNodeId,
        submitter: NativeNodeId,
    },
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
    external_shared_worker_routing: bool,
    javascript: Option<NativeJavaScriptRuntime>,
    dialog_control: NativeDialogControlPlane,
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
    pending_page_message_port_commands: VecDeque<NativePageMessagePortCommand>,
    pending_shared_worker_creates: VecDeque<NativeSharedWorkerCreateRequest>,
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
    document_frame_sources: Option<Vec<Vec<String>>>,
    document_navigate_to_sources: Option<Vec<Vec<String>>>,
    frame_script_bindings: Vec<NativeFrameScriptBinding>,
    frame_script_context: Option<NativeFrameScriptContext>,
    embedding_document_url: Option<String>,
    embedding_frame_sources: Option<Vec<Vec<String>>>,
    revision: u64,
    scroll_offset: NativePoint,
    nested_scroll_offsets: BTreeMap<u32, NativePoint>,
    effects: VecDeque<NativeEffect>,
    pending_lifecycle_effects: Vec<(NativeNodeId, NativeEventKind)>,
    skip_next_navigation_lifecycle: bool,
    document_has_sticky_activation: bool,
    pending_space_activation: Option<(NativeNodeId, Option<NativeCheckableKind>)>,
    outgoing_lifecycle_dispatch_depth: usize,
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
        Self::new_with_dialog_control(config, NativeDialogControlPlane::default())
    }

    pub(crate) fn new_with_dialog_control(
        config: NativeEngineConfig,
        dialog_control: NativeDialogControlPlane,
    ) -> Result<Self, NativeEngineError> {
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
            external_shared_worker_routing: false,
            javascript: None,
            dialog_control,
            service_worker_clients,
            workers: NativeWorkerRegistry::new_with_fetch_streams(),
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
            pending_page_message_port_commands: VecDeque::new(),
            pending_shared_worker_creates: VecDeque::new(),
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
            document_navigate_to_sources: None,
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
            document_has_sticky_activation: false,
            pending_space_activation: None,
            outgoing_lifecycle_dispatch_depth: 0,
        })
    }

    pub(crate) fn new_with_browser_shared_workers(
        config: NativeEngineConfig,
        dialog_control: NativeDialogControlPlane,
    ) -> Result<Self, NativeEngineError> {
        let mut engine = Self::new_with_dialog_control(config, dialog_control)?;
        engine.external_shared_worker_routing = true;
        Ok(engine)
    }

    pub(crate) fn dialog_control_plane(&self) -> NativeDialogControlPlane {
        self.dialog_control.clone()
    }

    pub fn config(&self) -> &NativeEngineConfig {
        &self.config
    }

    pub(crate) fn clone_resource_loader(&self) -> NativeResourceLoader {
        self.loader.clone()
    }

    /// Update the live CSS viewport and invalidate viewport-dependent
    /// computed style and geometry state without navigating the document.
    pub async fn set_viewport_async(
        &mut self,
        viewport: Viewport,
    ) -> Result<(), NativeEngineError> {
        self.require_running("set viewport")?;
        viewport.validate()?;
        if self.config.viewport == viewport {
            return Ok(());
        }
        if let Some(process) = self.content_process.as_mut() {
            process.set_viewport(viewport).await?;
        }
        self.document.set_viewport(viewport)?;
        let next_revision = self.next_revision()?;
        self.document.set_revision(next_revision);
        self.revision = next_revision;
        self.config.viewport = viewport;
        self.scroll_offset = NativePoint { x: 0, y: 0 };
        self.nested_scroll_offsets.clear();
        Ok(())
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
            page_message_port_commands,
            shared_worker_commands,
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
        self.queue_page_message_port_commands(page_message_port_commands)?;
        self.queue_shared_worker_create_commands(shared_worker_commands)?;
        self.queue_service_worker_client_messages(service_worker_client_messages)?;
        self.queue_service_worker_open_window_requests(service_worker_open_windows)?;
        if let Some(mut mutation) = mutation {
            let csp_violations = std::mem::take(&mut mutation.csp_violations);
            let navigation = mutation.navigation.clone();
            self.apply_content_process_mutation(mutation)?;
            self.dispatch_content_csp_violations_async(csp_violations)
                .await?;
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

    pub(crate) fn frame_owner_id(&self) -> &str {
        &self.frame_id
    }

    pub(crate) fn document_transfer_draft(&self) -> NativeDocument {
        self.document.clone()
    }

    pub(crate) fn validate_document_transfer_candidate(
        &self,
        candidate: &NativeDocument,
    ) -> Result<(), NativeEngineError> {
        self.require_running("cross-context node transfer")?;
        if candidate.generation() != self.document.generation()
            || self.document.revision() != self.revision
            || candidate.revision() != self.next_revision()?
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "native node transfer document generation or revision is stale".into(),
            });
        }
        Ok(())
    }

    pub(crate) async fn synchronize_document_transfer_snapshot(
        &mut self,
        snapshot: &NativeDocument,
    ) -> Result<(), NativeEngineError> {
        if snapshot.generation() != self.document.generation() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "native node transfer cannot synchronize a stale document generation"
                    .into(),
            });
        }
        if let Some(process) = self.content_process.as_mut() {
            let result = process
                .sync_document(
                    &snapshot.to_content_wire(),
                    &self.config.limits,
                    snapshot.generation(),
                )
                .await;
            let worker_failed = !process.is_healthy();
            if worker_failed {
                self.content_process.take();
            }
            result?;
        }
        Ok(())
    }

    pub(crate) fn publish_document_transfer_snapshot(&mut self, candidate: NativeDocument) {
        self.revision = candidate.revision();
        self.document = candidate;
    }

    pub(crate) fn navigation_requires_document_lifecycle(&self, target_url: &str) -> bool {
        !self.is_same_document_navigation(target_url)
    }

    pub(crate) fn embedded_frame_sources(
        &self,
    ) -> Result<Vec<(u32, String, Option<String>)>, NativeEngineError> {
        self.require_running("frame discovery")?;
        Ok(self
            .document
            .embedded_frame_sources()
            .into_iter()
            .map(|(node_id, source, sandbox)| (node_id.index(), source, sandbox))
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
        if is_file_url(&self.url) {
            return self
                .loader
                .allows_rooted_file_frame_navigation(&self.url, target_url);
        }
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
        Ok(frame_sources
            .iter()
            .all(|sources| csp_sources_allow(Some(sources), &document_url, &target_url)))
    }

    pub(crate) fn frame_navigation_policy(&self) -> (String, Option<Vec<Vec<String>>>) {
        (self.url.clone(), self.document_frame_sources.clone())
    }

    pub(crate) fn set_embedding_frame_policy(
        &mut self,
        document_url: String,
        frame_sources: Option<Vec<Vec<String>>>,
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
        Ok(self
            .embedding_frame_sources
            .as_deref()
            .is_none_or(|groups| {
                groups
                    .iter()
                    .all(|sources| csp_sources_allow(Some(sources), &document_url, &target_url))
            }))
    }

    fn allows_top_level_navigation(&mut self, target_url: &str) -> Result<bool, NativeEngineError> {
        let document_url = self.url.clone();
        let sources = self.document_navigate_to_sources.clone();
        self.allows_top_level_navigation_for_owner(
            &document_url,
            target_url,
            sources.as_deref(),
            true,
        )
    }

    fn allows_top_level_navigation_silent(
        &self,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        let document_url = self.url.clone();
        let sources = self.document_navigate_to_sources.clone();
        if !self.loader.allows_navigation_silent(
            &document_url,
            target_url,
            NativeNavigationPolicyKind::NavigateTo,
        )? {
            return Ok(false);
        }
        Self::navigation_sources_allow(&document_url, target_url, sources.as_deref())
    }

    async fn allows_top_level_navigation_async(
        &mut self,
        target_url: &str,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        let document_url = self.url.clone();
        let sources = self.document_navigate_to_sources.clone();
        self.allows_top_level_navigation_for_owner_async(
            &document_url,
            target_url,
            sources.as_deref(),
            report,
        )
        .await
    }

    async fn allows_top_level_navigation_for_owner_async(
        &mut self,
        document_url: &str,
        target_url: &str,
        sources: Option<&[Vec<String>]>,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        // The process-backed document is the authority for response CSP. The
        // parent keeps the bounded source snapshot for enforced decisions, but
        // only the child can observe report-only violations for this document.
        // Copy it before the event bridge can mutate the engine state.
        let sources = sources.map(|groups| groups.to_vec());
        let child_ready = report
            && is_network_url(without_fragment(document_url))
            && self
                .content_process
                .as_mut()
                .is_some_and(|process| process.refresh_health() && process.has_current_document());
        let loader_allowed = if child_ready {
            let policy = {
                let process =
                    self.content_process
                        .as_mut()
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "content process navigation policy".into(),
                            reason: "native content process is not running".into(),
                        })?;
                process
                    .check_navigation_policy(document_url, target_url, true)
                    .await?
            };
            if !policy.csp_violations.is_empty() {
                let page_events = NativePageEventBatch {
                    csp_violations: policy.csp_violations,
                    ..NativePageEventBatch::default()
                };
                Box::pin(self.evaluate_page_with_events_async("undefined;".into(), page_events))
                    .await?;
            }
            policy.allowed
        } else if report {
            self.loader.allows_navigation(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        } else {
            self.loader.allows_navigation_silent(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        };
        if !loader_allowed {
            return Ok(false);
        }
        Self::navigation_sources_allow(document_url, target_url, sources.as_deref())
    }

    fn allows_top_level_navigation_for_owner(
        &mut self,
        document_url: &str,
        target_url: &str,
        sources: Option<&[Vec<String>]>,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        let loader_allowed = if report {
            self.loader.allows_navigation(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        } else {
            self.loader.allows_navigation_silent(
                document_url,
                target_url,
                NativeNavigationPolicyKind::NavigateTo,
            )?
        };
        if !loader_allowed {
            return Ok(false);
        }
        Self::navigation_sources_allow(document_url, target_url, sources)
    }

    fn navigation_sources_allow(
        document_url: &str,
        target_url: &str,
        sources: Option<&[Vec<String>]>,
    ) -> Result<bool, NativeEngineError> {
        let Some(sources) = sources else {
            return Ok(true);
        };
        validate_url_text("CSP navigation owner URL", document_url)?;
        validate_url_text("CSP navigation target URL", target_url)?;
        let document_url = url::Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP navigation owner URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = url::Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP navigation target URL is not valid URL syntax".into(),
            }
        })?;
        Ok(sources
            .iter()
            .all(|group| csp_sources_allow(Some(group), &document_url, &target_url)))
    }

    fn allows_frame_navigation_after_redirect(
        &self,
        initial_url: &str,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        let Some(document_url) = self.embedding_document_url.as_deref() else {
            return Ok(true);
        };
        validate_url_text("initial frame navigation URL", initial_url)?;
        validate_url_text("frame navigation URL", target_url)?;
        let document_url = url::Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "the frame policy owner URL is not valid URL syntax".into(),
            }
        })?;
        let initial_url = url::Url::parse(without_fragment(initial_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "initial frame navigation URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = url::Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "frame navigation URL is not valid URL syntax".into(),
            }
        })?;
        let redirected = initial_url != target_url;
        Ok(self
            .embedding_frame_sources
            .as_deref()
            .is_none_or(|groups| {
                groups.iter().all(|sources| {
                    csp_sources_allow(Some(sources), &document_url, &initial_url)
                        && if redirected {
                            csp_sources_allow_for_redirect(
                                Some(sources),
                                &document_url,
                                &target_url,
                            )
                        } else {
                            csp_sources_allow(Some(sources), &document_url, &target_url)
                        }
                })
            }))
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
            Some(
                NativeContentProcess::spawn(
                    self.config.storage_path.as_deref(),
                    self.loader.allowed_file_roots(),
                    self.dialog_control.clone(),
                )
                .await?,
            )
        } else {
            None
        };
        if let Some(process) = content_process.as_mut() {
            process
                .start(
                    self.config.storage_path.as_deref(),
                    self.loader.allowed_file_roots(),
                    &self.config.context_id,
                    &self.frame_id,
                    &self.config.window_name,
                    self.config.opener_context_id.as_deref(),
                    &self.config.opener_window_name,
                    &self.config.opener_url,
                    self.frame_script_context.as_ref(),
                    &self.environment,
                    &self.service_worker_clients,
                    self.external_shared_worker_routing,
                )
                .await?;
        }
        let has_content_process = content_process.is_some();
        self.content_process = content_process;
        let mut use_content_process = has_content_process;
        let (prepared, history_commit, page_navigation_handoffs) = if has_content_process {
            let Some((content, history_commit, page_navigation_handoffs, initial_url)) = self
                .load_content_with_page_navigation(
                    NativeNavigationRequest::get(initial_url.clone()),
                    None,
                    HistoryCommit::Push,
                    0,
                    None,
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
                && !self.allows_frame_navigation_after_redirect(&initial_url, &content.url)?
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
        if !self.allows_top_level_navigation(&url)? {
            return Ok(self.snapshot_unchecked());
        }
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
        self.navigate_request_async_with_lifecycle(navigation, 0, false, None)
            .await
    }

    pub(crate) async fn navigate_request_async(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async_with_cancellation(navigation, page_navigation_handoffs, None)
            .await
    }

    async fn navigate_request_async_with_cancellation(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async_with_policy(
            navigation,
            page_navigation_handoffs,
            true,
            cancellation,
        )
        .await
    }

    pub(crate) async fn navigate_request_async_after_lifecycle(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.navigate_request_async_with_policy(
            navigation,
            page_navigation_handoffs,
            false,
            cancellation,
        )
        .await
    }

    async fn navigate_request_async_with_policy(
        &mut self,
        mut navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        dispatch_lifecycle: bool,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if let Some(target) = navigation.target.take() {
            let target_kind = target.to_ascii_lowercase();
            if !matches!(
                target_kind.as_str(),
                "_self" | "_parent" | "_top" | "_unfencedtop"
            ) || (target_kind != "_self" && self.frame_script_context.is_some())
            {
                if !self
                    .allows_top_level_navigation_async(&navigation.url, true)
                    .await?
                {
                    return Ok(self.snapshot_unchecked());
                }
                if self.queue_form_target_navigation(&target, &navigation)? {
                    return Ok(self.snapshot_unchecked());
                }
            }
        }
        self.navigate_request_async_with_lifecycle(
            navigation,
            page_navigation_handoffs,
            dispatch_lifecycle,
            cancellation,
        )
        .await
    }

    async fn navigate_request_async_with_lifecycle(
        &mut self,
        navigation: NativeNavigationRequest,
        page_navigation_handoffs: usize,
        dispatch_lifecycle: bool,
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("navigate")?;
        let same_document = self.is_same_document_navigation(&navigation.url);
        let allowed = self
            .allows_top_level_navigation_async(&navigation.url, true)
            .await?;
        if !allowed {
            return Ok(self.snapshot_unchecked());
        }
        if cancellation
            .as_ref()
            .is_some_and(NativeNavigationCancellation::is_cancelled)
        {
            return Err(NativeEngineError::NavigationCancelled);
        }
        if dispatch_lifecycle {
            self.pending_lifecycle_effects.clear();
        }
        self.sync_external_storage_events()?;
        self.deliver_pending_external_storage_events().await?;
        let navigation_url = navigation.url.clone();
        let url = navigation_url.as_str();
        let history_commit = if navigation.replace_history {
            HistoryCommit::Replace
        } else {
            HistoryCommit::Push
        };
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
                    cancellation,
                ))
                .await;
            }
        }
        if same_document && navigation.method == NativeNavigationMethod::Get {
            validate_url_text("navigation URL", url)?;
            if cancellation
                .as_ref()
                .is_some_and(|cancellation| !cancellation.begin_commit())
            {
                return Err(NativeEngineError::NavigationCancelled);
            }
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
            let Some((content, history_commit, page_navigation_handoffs, initial_url)) = self
                .load_content_with_page_navigation(
                    navigation,
                    referrer,
                    history_commit,
                    page_navigation_handoffs,
                    cancellation.clone(),
                )
                .await?
            else {
                if cancellation
                    .as_ref()
                    .is_some_and(NativeNavigationCancellation::is_cancelled)
                {
                    self.terminate_content_process_after_navigation_cancel()
                        .await?;
                    return Err(NativeEngineError::NavigationCancelled);
                }
                return Ok(self.snapshot_unchecked());
            };
            if cancellation
                .as_ref()
                .is_some_and(NativeNavigationCancellation::is_cancelled)
            {
                self.terminate_content_process_after_navigation_cancel()
                    .await?;
                return Err(NativeEngineError::NavigationCancelled);
            }
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation_after_redirect(&initial_url, &content.url)?
            {
                return Ok(self.snapshot_unchecked());
            }
            if cancellation
                .as_ref()
                .is_some_and(|cancellation| !cancellation.begin_commit())
            {
                self.terminate_content_process_after_navigation_cancel()
                    .await?;
                return Err(NativeEngineError::NavigationCancelled);
            }
            self.commit_content_process().await?;
            if let Some(worker) = self.runtime_worker.clone() {
                return self
                    .navigate_content_async(
                        content,
                        &worker,
                        history_commit,
                        page_navigation_handoffs,
                        &initial_url,
                    )
                    .await;
            }
            return self.navigate_content(content, &initial_url, history_commit);
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
            let mut process = NativeContentProcess::spawn(
                self.config.storage_path.as_deref(),
                self.loader.allowed_file_roots(),
                self.dialog_control.clone(),
            )
            .await?;
            process
                .start(
                    self.config.storage_path.as_deref(),
                    self.loader.allowed_file_roots(),
                    &self.config.context_id,
                    &self.frame_id,
                    &self.config.window_name,
                    self.config.opener_context_id.as_deref(),
                    &self.config.opener_window_name,
                    &self.config.opener_url,
                    self.frame_script_context.as_ref(),
                    &self.environment,
                    &self.service_worker_clients,
                    self.external_shared_worker_routing,
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
        cancellation: Option<NativeNavigationCancellation>,
    ) -> Result<Option<(NativeContentLoad, HistoryCommit, usize, String)>, NativeEngineError> {
        if page_navigation_handoffs > MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS {
            return Err(NativeEngineError::limit(
                "page navigation handoffs",
                MAX_NATIVE_PAGE_NAVIGATION_HANDOFFS,
                page_navigation_handoffs,
            ));
        }
        let mut policy_owner_url = self.url.clone();
        let mut policy_sources = self.document_navigate_to_sources.clone();
        let mut first_content_load = true;
        loop {
            if cancellation
                .as_ref()
                .is_some_and(NativeNavigationCancellation::is_cancelled)
            {
                return Err(NativeEngineError::NavigationCancelled);
            }
            let initial_url = navigation.url.clone();
            let report = !first_content_load;
            if !self
                .allows_top_level_navigation_for_owner_async(
                    &policy_owner_url,
                    &initial_url,
                    policy_sources.as_deref(),
                    report,
                )
                .await?
            {
                return Ok(None);
            }
            first_content_load = false;
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
                            cancellation.as_ref(),
                        )
                        .await
                }
                None => Err(NativeEngineError::Worker {
                    operation: "content process load".into(),
                    reason: "native content process is not running".into(),
                }),
            };
            self.request_ledger.finish();
            let mut content = match content_result {
                Ok(NativeContentLoadResult::Loaded(content)) => content,
                Ok(NativeContentLoadResult::Suspended(open_windows)) => {
                    if self.pending_service_worker_navigation.is_some() {
                        return Err(NativeEngineError::Worker {
                            operation: "native service worker navigation".into(),
                            reason: "another service worker navigation is already suspended".into(),
                        });
                    }
                    self.queue_service_worker_open_window_requests(open_windows)?;
                    self.pending_service_worker_navigation =
                        Some(NativePendingServiceWorkerNavigation {
                            initial_url,
                            history_commit,
                            page_navigation_handoffs,
                        });
                    return Ok(None);
                }
                Err(NativeEngineError::NavigationCancelled) => {
                    // `NativeContentProcess::load` has already killed and
                    // reaped the worker while settling its IPC exchange.
                    // Remove that dead handle so the next navigation starts a
                    // fresh content process instead of writing to a closed pipe.
                    self.content_process.take();
                    return Err(NativeEngineError::NavigationCancelled);
                }
                Err(error) => return Err(error),
            };
            if cancellation
                .as_ref()
                .is_some_and(NativeNavigationCancellation::is_cancelled)
            {
                self.terminate_content_process_after_navigation_cancel()
                    .await?;
                return Err(NativeEngineError::NavigationCancelled);
            }
            self.apply_content_load_effects(&mut content)?;
            if !self.allows_top_level_navigation_for_owner(
                &policy_owner_url,
                &content.url,
                policy_sources.as_deref(),
                false,
            )? {
                return Ok(None);
            }
            let Some(page_navigation) = content.navigation.clone() else {
                return Ok(Some((
                    content,
                    history_commit,
                    page_navigation_handoffs,
                    initial_url,
                )));
            };
            policy_sources = content.navigate_to_sources.clone();
            policy_owner_url = content.url.clone();
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
            navigation.object_url = page_navigation.object_url;
            history_commit = if page_navigation.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            referrer = referrer_for_navigation(&content.url, &target_url)?;
        }
    }

    async fn terminate_content_process_after_navigation_cancel(
        &mut self,
    ) -> Result<(), NativeEngineError> {
        if let Some(mut process) = self.content_process.take() {
            process
                .terminate_and_reap("cancel native navigation")
                .await?;
        }
        Ok(())
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
        self.queue_page_message_port_commands(std::mem::take(
            &mut content.page_message_port_commands,
        ))?;
        self.queue_shared_worker_create_commands(std::mem::take(
            &mut content.shared_worker_commands,
        ))?;
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
        let policy_owner_url = self.url.clone();
        let policy_sources = self.document_navigate_to_sources.clone();
        if !self.allows_top_level_navigation_for_owner(
            &policy_owner_url,
            &content.url,
            policy_sources.as_deref(),
            false,
        )? {
            return Ok(());
        }
        if !self.is_same_document_navigation(&content.url)
            && !self.allows_frame_navigation_after_redirect(&pending.initial_url, &content.url)?
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
            &pending.initial_url,
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
        initial_url: &str,
        history_commit: HistoryCommit,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&content.url) {
            if let Some(navigation) =
                self.commit_same_document_navigation(content.url, history_commit)?
            {
                self.navigate_page_script_sync(navigation, 1)?;
            }
        } else if !self.allows_frame_navigation_after_redirect(initial_url, &content.url)? {
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
        initial_url: &str,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        let same_document = self.is_same_document_navigation(&content.url);
        if !same_document
            && !self.allows_frame_navigation_after_redirect(initial_url, &content.url)?
        {
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
        let (canceled, sticky_activation, mut navigation) =
            self.dispatch_before_unload_for_navigation().await?;
        let allowed = if !canceled || !sticky_activation {
            true
        } else {
            self.confirm_beforeunload_for_navigation().await?
        };
        if !allowed {
            return Ok((false, None));
        }
        if let Some(next_navigation) = self.dispatch_unload_for_navigation().await? {
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "multiple outgoing lifecycle navigations are not supported".into(),
                });
            }
            navigation = Some(next_navigation);
        }
        Ok((true, navigation))
    }

    pub(crate) async fn dispatch_before_unload_for_navigation(
        &mut self,
    ) -> Result<(bool, bool, Option<NativeNavigationRequest>), NativeEngineError> {
        let (event_allowed, navigation) = if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            self.dispatch_content_before_unload_async().await?
        } else if self.javascript.is_some() {
            self.dispatch_local_before_unload()?
        } else {
            (true, None)
        };
        Ok((
            !event_allowed,
            self.document_has_sticky_activation,
            navigation,
        ))
    }

    pub(crate) async fn confirm_beforeunload_for_navigation(
        &self,
    ) -> Result<bool, NativeEngineError> {
        self.wait_for_beforeunload_confirmation_async().await
    }

    pub(crate) async fn dispatch_unload_for_navigation(
        &mut self,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        let events = [NativeEventKind::PageHide, NativeEventKind::Unload];
        let navigation = if self
            .content_process
            .as_mut()
            .is_some_and(NativeContentProcess::refresh_health)
        {
            self.dispatch_content_events_async(&events, true).await?
        } else if self.javascript.is_some() {
            self.dispatch_local_navigation_lifecycle()?
        } else {
            None
        };
        self.persist_local_web_storage()?;
        Ok(navigation)
    }

    async fn wait_for_beforeunload_confirmation_async(&self) -> Result<bool, NativeEngineError> {
        let pending = self
            .dialog_control
            .wait_for_beforeunload(&self.config.context_id, &self.frame_id, &self.url)
            .await?;
        let accepted = pending.resolution.accepted;
        pending.finish();
        Ok(accepted)
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
        self.apply_content_process_lifecycle_mutation_async_at(next_revision, mutation)
            .await?;
        Ok((allowed, navigation))
    }

    async fn dispatch_content_events_async(
        &mut self,
        events: &[NativeEventKind],
        outgoing_lifecycle: bool,
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
        if outgoing_lifecycle {
            self.apply_content_process_lifecycle_mutation_async_at(next_revision, mutation)
                .await?;
        } else {
            self.apply_content_process_mutation_async_at(next_revision, mutation)
                .await?;
        }
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
        self.traverse_history_async(NativeHistoryDirection::Back, "go back", true)
            .await
    }

    /// Move to the next history entry through the asynchronous native
    /// navigation owner. A missing forward entry is an explicit boundary
    /// no-op.
    pub async fn go_forward_async(
        &mut self,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history_async(NativeHistoryDirection::Forward, "go forward", true)
            .await
    }

    pub(crate) fn history_target_is_cross_document(
        &self,
        direction: NativeHistoryDirection,
    ) -> Option<bool> {
        let history_index = self.history.target_index(direction)?;
        let target_url = &self.history.entry(history_index)?.url;
        Some(
            !self.history.is_same_document(history_index)
                && !self.is_same_document_navigation(target_url),
        )
    }

    pub(crate) async fn traverse_history_async_after_lifecycle(
        &mut self,
        direction: NativeHistoryDirection,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        let operation = match direction {
            NativeHistoryDirection::Back => "go back",
            NativeHistoryDirection::Forward => "go forward",
        };
        self.traverse_history_async(direction, operation, false)
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

    /// Give the inline owner the same bounded worker Fetch stream turn that
    /// the content process already runs around page evaluations. The yield is
    /// intentional: local fixture streams are spawned tasks, so their next
    /// demand-driven event must get a scheduler opportunity before the owner
    /// polls the event channel.
    async fn pump_local_worker_fetch_streams(&mut self) -> Result<(), NativeEngineError> {
        for _ in 0..MAX_NATIVE_WORKER_MESSAGES {
            tokio::task::yield_now().await;
            if !self
                .workers
                .pump_fetch_stream_event(&mut self.loader)
                .await?
            {
                break;
            }
        }
        Ok(())
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
                page_message_port_commands,
                shared_worker_commands,
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
            self.queue_page_message_port_commands(page_message_port_commands)?;
            self.queue_shared_worker_create_commands(shared_worker_commands)?;
            self.queue_service_worker_client_messages(service_worker_client_messages)?;
            self.queue_service_worker_open_window_requests(service_worker_open_windows)?;
            let mut history_traversal = None;
            let mutation_history = mutation
                .as_ref()
                .map(|mutation| mutation.history.clone())
                .unwrap_or_default();
            history.extend(mutation_history);
            if let Some(mut mutation) = mutation {
                let csp_violations = std::mem::take(&mut mutation.csp_violations);
                let navigation = mutation.navigation.clone();
                self.apply_content_process_mutation(mutation)?;
                self.dispatch_content_csp_violations_async(csp_violations)
                    .await?;
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
        self.collect_page_message_port_commands()?;
        self.pump_local_worker_fetch_streams().await?;
        self.workers.run_due_timers(&mut self.loader).await?;
        self.pump_local_worker_fetch_streams().await?;
        page_events
            .worker_messages
            .extend(self.workers.take_messages());
        page_events
            .message_port_messages
            .extend(std::mem::take(&mut self.pending_message_port_messages));
        page_events
            .message_port_messages
            .extend(self.workers.take_message_port_messages());
        let evaluation = {
            let javascript = self
                .javascript
                .as_ref()
                .expect("local JavaScript runtime initialized");
            self.synchronize_local_inline_csp_policy(&self.document, javascript, &self.url)?;
            javascript.set_sync_xhr_loader(&self.loader);
            let result = javascript.evaluate_with_page_events(
                &source,
                &self.document,
                &self.url,
                &self.origin,
                self.config.viewport,
                &page_events,
            );
            if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            result?
        };
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
        self.collect_page_message_port_commands()?;
        self.pump_local_worker_fetch_streams().await?;
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
        self.collect_page_message_port_commands()?;
        self.pump_local_worker_fetch_streams().await?;
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

    async fn dispatch_content_csp_violations_async(
        &mut self,
        csp_violations: Vec<NativeCspViolation>,
    ) -> Result<(), NativeEngineError> {
        if csp_violations.is_empty() {
            return Ok(());
        }
        let page_events = NativePageEventBatch {
            csp_violations,
            ..NativePageEventBatch::default()
        };
        Box::pin(self.evaluate_page_with_events_async("undefined;".into(), page_events)).await?;
        Ok(())
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
            method: NativeNavigationMethod::Get,
            body: None,
            body_content_type: None,
            handle: None,
            object_url: None,
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
        validate_target_navigation_payload(
            popup.method,
            popup.body.as_ref(),
            popup.body_content_type.as_deref(),
            "popup navigation payload",
        )?;
        if let Some(handle) = popup.handle.as_deref() {
            validate_url_text("popup window handle", handle)?;
        }
        if popup.object_url.is_none()
            && popup
                .url
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
        {
            popup.object_url = self
                .javascript
                .as_ref()
                .map(|javascript| javascript.object_url_resource(&popup.url))
                .transpose()?
                .flatten();
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
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_object_url_transfers(&message.object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native postMessage data",
        )?;
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
        if message.source_frame_id.is_empty() {
            message.source_frame_id = self.frame_id.clone();
        } else {
            validate_context_id(&message.source_frame_id)?;
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

    pub(crate) fn queue_page_message_port_commands(
        &mut self,
        commands: Vec<NativePageMessagePortCommand>,
    ) -> Result<(), NativeEngineError> {
        for mut command in commands {
            validate_page_message_port_command(&command)?;
            if self.pending_page_message_port_commands.len() >= MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native pending page MessagePort commands",
                    MAX_NATIVE_EFFECTS,
                    self.pending_page_message_port_commands
                        .len()
                        .saturating_add(1),
                ));
            }
            if command.source_context_id.is_empty() {
                command.source_context_id = self.config.context_id.clone();
            } else {
                validate_context_id(&command.source_context_id)?;
            }
            if command.source_frame_id.is_empty() {
                command.source_frame_id = self.frame_id.clone();
            } else {
                validate_context_id(&command.source_frame_id)?;
            }
            self.pending_page_message_port_commands.push_back(command);
        }
        Ok(())
    }

    fn queue_shared_worker_create_commands(
        &mut self,
        commands: Vec<NativeScriptCommand>,
    ) -> Result<(), NativeEngineError> {
        if commands.len()
            > MAX_NATIVE_EFFECTS.saturating_sub(self.pending_shared_worker_creates.len())
        {
            return Err(NativeEngineError::limit(
                "native pending SharedWorker creates",
                MAX_NATIVE_EFFECTS,
                self.pending_shared_worker_creates
                    .len()
                    .saturating_add(commands.len()),
            ));
        }
        for command in commands {
            let NativeScriptCommand::SharedWorkerCreate {
                connection_id,
                href,
                name,
                worker_type,
                transfer_port,
            } = command
            else {
                return Err(NativeEngineError::Worker {
                    operation: "route SharedWorker create".into(),
                    reason: "content process returned an unexpected worker command".into(),
                });
            };
            if connection_id == 0 {
                return Err(NativeEngineError::invalid(
                    "native SharedWorker connection id",
                    "must be positive",
                ));
            }
            validate_context_id(&self.config.context_id)?;
            validate_context_id(&self.frame_id)?;
            validate_message_port_transfers(std::slice::from_ref(&transfer_port))?;
            self.pending_shared_worker_creates
                .push_back(NativeSharedWorkerCreateRequest {
                    source_context_id: self.config.context_id.clone(),
                    source_frame_id: self.frame_id.clone(),
                    owner_url: self.url.clone(),
                    href,
                    name,
                    worker_type,
                    transfer_port,
                });
        }
        Ok(())
    }

    fn collect_page_message_port_commands(&mut self) -> Result<(), NativeEngineError> {
        let commands = self.workers.take_page_message_port_commands();
        self.queue_page_message_port_commands(commands)
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
        validate_target_navigation_payload(
            request.method,
            request.body.as_ref(),
            request.body_content_type.as_deref(),
            "window navigation payload",
        )?;
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

    fn queue_form_target_navigation(
        &mut self,
        target: &str,
        navigation: &NativeNavigationRequest,
    ) -> Result<bool, NativeEngineError> {
        validate_window_name(target)?;
        let target_kind = target.to_ascii_lowercase();
        let target_context = match target_kind.as_str() {
            "_parent" => self
                .frame_script_context
                .as_ref()
                .and_then(|context| context.parent.as_ref()),
            "_top" | "_unfencedtop" => self
                .frame_script_context
                .as_ref()
                .and_then(|context| context.top.as_ref()),
            "_self" => None,
            _ => {
                self.queue_popup_request(NativePopupRequest {
                    url: navigation.url.clone(),
                    target: target.to_owned(),
                    method: navigation.method,
                    body: navigation.body.clone(),
                    body_content_type: navigation.body_content_type.clone(),
                    handle: None,
                    object_url: navigation.object_url.clone(),
                    source_context_id: String::new(),
                })?;
                return Ok(true);
            }
        };
        let Some(target_context) = target_context else {
            return Ok(false);
        };
        if target_context.context_id == self.config.context_id {
            return Ok(false);
        }
        self.queue_window_navigation_request(NativeWindowNavigationRequest {
            target: String::new(),
            target_context_id: Some(target_context.context_id.clone()),
            href: navigation.url.clone(),
            method: navigation.method,
            body: navigation.body.clone(),
            body_content_type: navigation.body_content_type.clone(),
            replace: navigation.replace_history,
            object_url: navigation.object_url.clone(),
            source_context_id: String::new(),
        })?;
        Ok(true)
    }

    fn navigate_local_form_request(
        &mut self,
        mut request: NativeNavigationRequest,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let target = request.target.take().unwrap_or_else(|| "_self".into());
        let target_kind = target.to_ascii_lowercase();
        let targets_current_document = match target_kind.as_str() {
            "_self" => true,
            "_parent" | "_top" | "_unfencedtop" => self.frame_script_context.is_none(),
            _ => false,
        };
        if !targets_current_document {
            if !self.allows_top_level_navigation(&request.url)? {
                return Ok(NativeActionResult {
                    revision: self.revision,
                    accepted: true,
                });
            }
            if self.queue_form_target_navigation(&target, &request)? {
                return Ok(NativeActionResult {
                    revision: self.revision,
                    accepted: true,
                });
            }
        }
        let snapshot = self.navigate(request.url)?;
        Ok(NativeActionResult {
            revision: snapshot.revision,
            accepted: true,
        })
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

    pub(crate) fn take_pending_page_message_port_commands(
        &mut self,
    ) -> Vec<NativePageMessagePortCommand> {
        self.pending_page_message_port_commands.drain(..).collect()
    }

    pub(crate) fn take_pending_shared_worker_creates(
        &mut self,
    ) -> Vec<NativeSharedWorkerCreateRequest> {
        self.pending_shared_worker_creates.drain(..).collect()
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
        transfer_ports: &[NativeMessagePortTransfer],
        object_urls: &[super::resource_loader::NativeObjectUrlTransfer],
    ) -> Result<(), NativeEngineError> {
        self.require_running("message event")?;
        validate_message_port_transfers(transfer_ports)?;
        validate_native_object_url_transfers(object_urls)?;
        let mut page_events = NativePageEventBatch::default();
        page_events
            .post_message_events
            .push(NativePageMessageEvent {
                source_context_id: source_context_id.to_owned(),
                source_origin: source_origin.to_owned(),
                data: data.clone(),
                transfer_ports: transfer_ports.to_vec(),
                object_urls: object_urls.to_vec(),
            });
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub(crate) async fn dispatch_page_message_port(
        &mut self,
        bridge_key: &str,
        data: &serde_json::Value,
        transfer_ports: &[NativeMessagePortTransfer],
        object_urls: &[NativeObjectUrlTransfer],
    ) -> Result<(), NativeEngineError> {
        self.require_running("MessagePort event")?;
        let command = NativePageMessagePortCommand {
            bridge_key: bridge_key.to_owned(),
            data: data.clone(),
            close: false,
            transfer_ports: transfer_ports.to_vec(),
            object_urls: object_urls.to_vec(),
            source_context_id: String::new(),
            source_frame_id: String::new(),
        };
        validate_page_message_port_command(&command)?;
        let mut page_events = NativePageEventBatch::default();
        page_events
            .message_port_messages
            .push(NativeMessagePortPageMessage {
                bridge_key: command.bridge_key,
                data: command.data,
                close: false,
                transfer_ports: command.transfer_ports,
                object_urls: command.object_urls,
            });
        self.evaluate_page_with_events_async("undefined;".into(), page_events)
            .await
            .map(|_| ())
    }

    pub(crate) async fn dispatch_page_message_port_close(
        &mut self,
        bridge_key: &str,
    ) -> Result<(), NativeEngineError> {
        self.require_running("MessagePort close event")?;
        validate_url_text("native MessagePort bridge key", bridge_key)?;
        if bridge_key.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native MessagePort bridge key",
                crate::browser_backend::MAX_BACKEND_ID_BYTES,
                bridge_key.len(),
            ));
        }
        let mut page_events = NativePageEventBatch::default();
        page_events
            .message_port_messages
            .push(NativeMessagePortPageMessage {
                bridge_key: bridge_key.to_owned(),
                data: serde_json::Value::Null,
                close: true,
                transfer_ports: Vec::new(),
                object_urls: Vec::new(),
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
        let layout = self.layout()?;
        let geometry = layout
            .viewport_rect_for(id)
            .or_else(|| self.document.image_map_area_viewport_rect(id, &layout));
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
        let layout = self.layout()?;
        self.document.hit_test_with_layout(&layout, x, y)
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
        let (action, click_modifiers) = match action {
            NativeAction::ClickWithModifiers { target, modifiers } => {
                if modifiers > 15 {
                    return Err(NativeEngineError::invalid(
                        "native click modifiers",
                        "must be a bit mask from 0 through 15",
                    ));
                }
                (NativeAction::Click { target }, modifiers)
            }
            action => (action, 0),
        };
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
            NativeAction::ClickWithModifiers { .. } => {
                unreachable!("modifier clicks are normalized before the native action match")
            }
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                self.require_layout_actionable(id)?;
                self.document_has_sticky_activation = true;
                if let Some(href) = self.document.link_href(id).map(str::to_owned)
                    && !href.is_empty()
                {
                    if self.javascript.is_some() {
                        return self.action_local_click_with_event_preflight(id, click_modifiers);
                    }
                    return self.activate_link(
                        id,
                        &href,
                        false,
                        modifier_click_opens_new_target(click_modifiers),
                    );
                }
                if let Some(form_id) = self.document.submit_control_form(id)
                    && self.javascript.is_none()
                {
                    return self.action_local_submit_click_without_script(id, form_id);
                }
                if self.javascript.is_some() {
                    return self.action_local_click_with_event_preflight(id, click_modifiers);
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
                self.document_has_sticky_activation = true;
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
                self.document_has_sticky_activation = true;
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
                self.document_has_sticky_activation = true;
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
                self.document_has_sticky_activation = true;
                if self.javascript.is_some() {
                    return self.action_local_form_with_event_transaction(
                        |document| document.apply_select(id, &value),
                        "native select event effects",
                    );
                }
                (self.document.apply_select(id, &value)?, true)
            }
            NativeAction::KeyDown { key } => {
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                self.document_has_sticky_activation = true;
                let id = self.document.focused_node();
                return self.action_local_key_event(id, &key, NativeEventKind::KeyDown, 0);
            }
            NativeAction::KeyUp { key } => {
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                let id = self.document.focused_node();
                return self.action_local_key_event(id, &key, NativeEventKind::KeyUp, 0);
            }
            NativeAction::Shortcut { shortcut } => {
                let (modifiers, key) = parse_native_shortcut(&shortcut)?;
                self.document_has_sticky_activation = true;
                let id = self.document.focused_node();
                return self.action_local_key_sequence(id, &key, modifiers, true);
            }
            NativeAction::KeyPress { key } => {
                validate_native_edit_key(&key)?;
                let id = self.document.focused_text_control()?;
                self.document_has_sticky_activation = true;
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
        let (action, click_modifiers) = match action {
            NativeAction::ClickWithModifiers { target, modifiers } => {
                if modifiers > 15 {
                    return Err(NativeEngineError::invalid(
                        "native click modifiers",
                        "must be a bit mask from 0 through 15",
                    ));
                }
                (NativeAction::Click { target }, modifiers)
            }
            action => (action, 0),
        };
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
            return if click_modifiers == 0 {
                self.action(action)
            } else if let NativeAction::Click { target } = action {
                self.action(NativeAction::ClickWithModifiers {
                    target,
                    modifiers: click_modifiers,
                })
            } else {
                unreachable!("only click actions carry pointer modifiers")
            };
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
            NativeAction::ClickWithModifiers { .. } => {
                unreachable!("modifier clicks are normalized before the native async action match")
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
                let initial_link_href = self
                    .document
                    .link_href(id)
                    .filter(|href| !href.is_empty())
                    .map(str::to_owned);
                let initial_download_attribute =
                    self.document.link_download_attribute(id).map(str::to_owned);
                if initial_download_attribute.is_some()
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
                self.document_has_sticky_activation = true;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process click preflight".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_click_with_event_preflight(id.index(), click_modifiers)
                        .await?
                };
                let navigation = mutation.navigation.clone();
                let click_allowed = mutation.allowed;
                if click_allowed
                    && initial_link_href.is_some()
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
                if click_allowed
                    && let Some(href) = self
                        .document
                        .link_href(id)
                        .filter(|href| !href.is_empty())
                        .map(str::to_owned)
                {
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
                    let target_url = self.resolve_link_href(&href)?;
                    let opens_new_target = self.document.link_opens_new_target(id)
                        || modifier_click_opens_new_target(click_modifiers);
                    let special_navigation = download_attribute.is_some() || opens_new_target;
                    if special_navigation
                        && !self
                            .allows_top_level_navigation_async(&target_url, true)
                            .await?
                    {
                        return Ok(NativeActionResult {
                            revision: self.revision,
                            accepted: outcome.accepted,
                        });
                    }
                    if let Some(download_attribute) = download_attribute {
                        self.queue_download(target_url, &download_attribute)?;
                        return Ok(NativeActionResult {
                            revision: self.revision,
                            accepted: outcome.accepted,
                        });
                    }
                    if opens_new_target {
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
                let mut preview = self.document.clone();
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
                self.document_has_sticky_activation = true;
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
                self.document_has_sticky_activation = true;
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
                self.document_has_sticky_activation = true;
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
                self.document_has_sticky_activation = true;
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
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                self.document_has_sticky_activation = true;
                let focused_node = self.document.focused_node();
                let keyboard_link_target = (key == "Enter"
                    && self
                        .document
                        .has_native_keyboard_link_activation(focused_node))
                .then_some(focused_node);
                let node_index = self.document.keyboard_event_target(focused_node).index();
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
                self.apply_keyboard_mutation_async(mutation, keyboard_link_target)
                    .await
            }
            NativeAction::KeyUp { key } => {
                validate_native_key(&key)?;
                let key = normalize_native_keyboard_key(key);
                let focused_node = self.document.focused_node();
                let node_index = self.document.keyboard_event_target(focused_node).index();
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
                self.apply_keyboard_mutation_async(mutation, None).await
            }
            NativeAction::Shortcut { shortcut } => {
                let (modifiers, key) = parse_native_shortcut(&shortcut)?;
                self.document_has_sticky_activation = true;
                let focused_node = self.document.focused_node();
                let keyboard_link_target = (key == "Enter"
                    && self
                        .document
                        .has_native_keyboard_link_activation(focused_node))
                .then_some(focused_node);
                let node_index = self.document.keyboard_event_target(focused_node).index();
                let default_allowed = should_apply_native_key_default(&key, modifiers);
                let apply_default = default_allowed
                    && (key == "Tab"
                        || self.document.can_apply_radio_group_arrow_navigation(
                            focused_node,
                            &key,
                            modifiers,
                        )
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
                self.apply_keyboard_mutation_async(mutation, keyboard_link_target)
                    .await
            }
            NativeAction::KeyPress { key } => {
                validate_native_edit_key(&key)?;
                let id = self.document.focused_text_control()?;
                let mut preview = self.document.clone();
                preview.apply_key_press(id, &key)?;
                self.document_has_sticky_activation = true;
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
            | NativeAction::ClickWithModifiers { target, .. }
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

    fn synchronize_local_inline_csp_policy(
        &self,
        document: &NativeDocument,
        runtime: &NativeJavaScriptRuntime,
        document_url: &str,
    ) -> Result<(), NativeEngineError> {
        runtime.set_inline_script_policy(self.loader.inline_script_policy(document_url)?);
        runtime.mark_processed_inline_csp_meta_nodes(
            document.processed_content_security_policy_meta_nodes(),
        );
        Ok(())
    }

    fn apply_local_document_commands_with_font_face_ack(
        &mut self,
        document: &mut NativeDocument,
        commands: &[NativeScriptCommand],
        allow_script_navigation: bool,
    ) -> Result<
        (
            Vec<(NativeNodeId, NativeEventKind)>,
            Vec<NativeScriptCommand>,
        ),
        NativeEngineError,
    > {
        let inline_style_state_before = (is_file_url(&self.url)
            && script_commands_may_change_inline_styles(commands))
        .then(|| inline_style_source_state(document));
        let result = if let Some(javascript) = self.javascript.as_ref() {
            apply_document_commands_with_font_face_ack(
                document,
                javascript,
                &self.url,
                &self.origin,
                self.config.viewport,
                commands,
                allow_script_navigation,
            )
        } else {
            let events = if allow_script_navigation {
                document.apply_script_commands_allowing_links(commands)?
            } else {
                document.apply_script_commands(commands)?
            };
            Ok((events, Vec::new()))
        };
        let (events, follow_up_commands) = result?;
        if is_file_url(&self.url) {
            super::content_process::apply_pending_meta_content_security_policies(
                document,
                &mut self.loader,
                &self.url,
            )?;
            if let Some(runtime) = self.javascript.as_ref() {
                self.synchronize_local_inline_csp_policy(document, runtime, &self.url)?;
            }
        }
        if let Some(inline_style_state_before) = inline_style_state_before {
            refresh_rooted_file_inline_styles(
                document,
                &mut self.loader,
                &self.url,
                self.javascript.as_ref(),
                &inline_style_state_before,
            )?;
        }
        Ok((events, follow_up_commands))
    }

    fn retain_local_font_face_follow_up_commands(
        &self,
        effective_commands: &mut Vec<NativeScriptCommand>,
        scroll_commands: &mut Vec<NativeScriptCommand>,
        history_commands: &mut Vec<LocalHistoryCommand>,
        commands: Vec<NativeScriptCommand>,
    ) -> Result<(), NativeEngineError> {
        if commands.iter().any(|command| {
            matches!(
                command,
                NativeScriptCommand::Fetch { .. }
                    | NativeScriptCommand::WebSocketOpen { .. }
                    | NativeScriptCommand::WebSocketSend { .. }
                    | NativeScriptCommand::WebSocketClose { .. }
                    | NativeScriptCommand::EventSourceOpen { .. }
                    | NativeScriptCommand::EventSourceClose { .. }
                    | NativeScriptCommand::FetchStreamRead { .. }
                    | NativeScriptCommand::FetchStreamCancel { .. }
            )
        }) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "script network transport requires a process-backed HTTP(S) document"
                    .into(),
            });
        }
        history_commands.extend(self.prepare_local_history_commands(&commands)?);
        scroll_commands.extend(extract_local_scroll_commands(&commands));
        effective_commands.extend(commands);
        Ok(())
    }

    fn apply_local_evaluation_commands(
        &mut self,
        document: &mut NativeDocument,
        commands: &[NativeScriptCommand],
        history_commands: &mut Vec<NativeScriptCommand>,
    ) -> Result<Vec<(NativeNodeId, NativeEventKind)>, NativeEngineError> {
        history_commands.extend(extract_local_history_commands(commands));
        let (effects, follow_up_commands) =
            self.apply_local_document_commands_with_font_face_ack(document, commands, false)?;
        history_commands.extend(extract_local_history_commands(&follow_up_commands));
        Ok(effects)
    }

    fn apply_local_script_commands(
        &mut self,
        commands: &[NativeScriptCommand],
        allow_script_navigation: bool,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if commands.is_empty() {
            return Ok(None);
        }

        let mut command_queue = VecDeque::from(commands.to_vec());
        let mut module_fetches = VecDeque::new();
        let mut navigation = None;
        let mut resolved_module_requests = 0usize;
        while !command_queue.is_empty() || !module_fetches.is_empty() {
            let mut batch = Vec::new();
            while let Some(command) = command_queue.pop_front() {
                match &command {
                    NativeScriptCommand::Fetch {
                        destination: Some(destination),
                        module_referrer: Some(_),
                        ..
                    } if destination == "module" => module_fetches.push_back(command),
                    NativeScriptCommand::Fetch { .. }
                    | NativeScriptCommand::WebSocketOpen { .. }
                    | NativeScriptCommand::WebSocketSend { .. }
                    | NativeScriptCommand::WebSocketClose { .. }
                    | NativeScriptCommand::EventSourceOpen { .. }
                    | NativeScriptCommand::EventSourceClose { .. }
                    | NativeScriptCommand::FetchStreamRead { .. }
                    | NativeScriptCommand::FetchStreamCancel { .. } => {
                        return Err(NativeEngineError::UnsupportedUrl {
                            reason: "script network transport requires a process-backed HTTP(S) document"
                                .into(),
                        });
                    }
                    _ => batch.push(command),
                }
            }

            if !batch.is_empty() {
                let mut follow_up_fetches = Vec::new();
                let batch_navigation = self.apply_local_script_command_batch(
                    &batch,
                    allow_script_navigation,
                    &mut follow_up_fetches,
                )?;
                module_fetches.extend(follow_up_fetches);
                if let Some(batch_navigation) = batch_navigation {
                    if navigation.is_some() {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason:
                                "one script event-loop batch cannot activate multiple navigations"
                                    .into(),
                        });
                    }
                    navigation = Some(batch_navigation);
                }
            }

            let Some(module_fetch) = module_fetches.pop_front() else {
                continue;
            };
            resolved_module_requests = resolved_module_requests.saturating_add(1);
            if resolved_module_requests > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "rooted-file dynamic module requests",
                    MAX_NATIVE_MODULE_IMPORTS,
                    resolved_module_requests,
                ));
            }
            command_queue.extend(self.resolve_local_dynamic_module_fetch(module_fetch)?);
        }

        Ok(navigation)
    }

    fn resolve_local_dynamic_module_fetch(
        &self,
        command: NativeScriptCommand,
    ) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
        let NativeScriptCommand::Fetch {
            request_id,
            worker_id,
            href,
            credentials,
            method,
            headers,
            body,
            body_base64,
            content_type,
            mode,
            redirect,
            cache,
            timeout_ms,
            upload_stream_id,
            destination,
            module_referrer,
            module_type,
        } = command
        else {
            return Err(NativeEngineError::invalid(
                "rooted-file dynamic module request",
                "command was not a fetch request",
            ));
        };
        if request_id == 0 {
            return Err(NativeEngineError::invalid(
                "rooted-file dynamic module request id",
                "must be positive",
            ));
        }
        let module_referrer = module_referrer.unwrap_or_default();
        let request_is_valid = worker_id.is_none()
            && credentials
            && method == "GET"
            && headers.is_empty()
            && body.is_none()
            && body_base64.is_none()
            && content_type.is_none()
            && mode.as_deref() == Some("cors")
            && redirect.as_deref() == Some("follow")
            && cache.as_deref() == Some("default")
            && timeout_ms.is_none()
            && upload_stream_id.is_none()
            && destination.as_deref() == Some("module")
            && module_type != Some(NativeModuleResourceType::Unsupported);

        let runtime = self
            .javascript
            .as_ref()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "rooted-file dynamic module import".into(),
                reason: "page JavaScript runtime is unavailable".into(),
            })?;
        let loaded_module = if request_is_valid {
            self.load_local_dynamic_page_module(
                request_id,
                &module_referrer,
                &href,
                module_type,
                runtime,
            )
        } else {
            Err(NativeEngineError::invalid(
                "rooted-file dynamic module request",
                "must be a credentialed bodyless CORS GET with default cache and follow redirects",
            ))
        };
        let module_alias = loaded_module.as_ref().ok().cloned();
        let payload = match loaded_module {
            Ok(module_key) => serde_json::json!({
                "dynamicModuleImport": {"moduleKey": module_key},
            }),
            Err(_) => serde_json::json!({
                "dynamicModuleImport": {"error": "Failed to load dynamically imported module"},
            }),
        };
        let evaluation = runtime.resolve_fetch(
            request_id,
            &payload,
            &self.document,
            &self.url,
            &self.origin,
            self.config.viewport,
            &NativePageEventBatch::default(),
        );
        if let Some(module_alias) = module_alias {
            runtime.discard_dynamic_module_alias(&module_alias);
        }
        let evaluation = evaluation?;
        if evaluation.top_level_await_pending {
            return Err(NativeEngineError::Worker {
                operation: "rooted-file dynamic module import".into(),
                reason: "module evaluation remained pending without a native host operation".into(),
            });
        }
        Ok(evaluation.commands)
    }

    fn load_local_dynamic_page_module(
        &self,
        request_id: u32,
        module_referrer: &str,
        specifier: &str,
        module_type: Option<NativeModuleResourceType>,
        runtime: &NativeJavaScriptRuntime,
    ) -> Result<String, NativeEngineError> {
        if !is_file_url(&self.url) || !is_file_url(module_referrer) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason:
                    "rooted-file dynamic modules require a file document and active file referrer"
                        .into(),
            });
        }
        if !runtime.is_dynamic_import_referrer_allowed(module_referrer)? {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "dynamic module referrer is not an active page script".into(),
            });
        }

        let mut import_map = runtime.module_import_map()?;
        let target_result = import_map.resolve_and_record(module_referrer, specifier);
        runtime.set_module_import_map(import_map.clone());
        let target = target_result.map_err(|reason| NativeEngineError::Network {
            operation: "rooted-file dynamic module import".into(),
            reason: reason.into(),
        })?;
        if !is_file_url(&target) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "rooted-file dynamic module target must use the file scheme".into(),
            });
        }
        let module_type = module_type.unwrap_or(NativeModuleResourceType::JavaScript);
        if module_type == NativeModuleResourceType::Unsupported {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "rooted-file dynamic import attributes request an unsupported type".into(),
            });
        }
        let module_name = super::javascript::native_module_loader_name(&target, module_type);

        let (existing_sources, _) = runtime.module_sources_snapshot()?;
        if existing_sources.contains_key(&module_name) {
            return runtime.register_dynamic_module_alias(request_id, &target);
        }
        if existing_sources.len() >= MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "native page module graph entries",
                MAX_NATIVE_MODULE_IMPORTS,
                existing_sources.len().saturating_add(1),
            ));
        }
        let existing_names = existing_sources.keys().cloned().collect::<BTreeSet<_>>();
        let existing_bytes = existing_sources
            .values()
            .map(String::len)
            .fold(0usize, usize::saturating_add);
        let integrity = import_map.integrity_for_url(&target).map(str::to_owned);
        let resource = self
            .loader
            .load_local_file_script_with_policy(
                &self.url,
                &target,
                MAX_NATIVE_SCRIPT_BYTES,
                false,
                None,
                integrity.as_deref(),
            )?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "rooted-file dynamic module import".into(),
                reason: "module resource is missing, blocked, or outside the configured file root"
                    .into(),
            })?;
        let graph_result = load_local_file_dynamic_module_graph_with_import_map(
            &self.loader,
            &self.url,
            module_name.clone(),
            resource.url,
            resource.body,
            &mut import_map,
            &existing_names,
            existing_bytes,
        );
        runtime.set_module_import_map(import_map);
        let graph = graph_result?;

        let mut new_sources = BTreeMap::new();
        let mut new_base_urls = BTreeMap::new();
        for (_, script) in graph {
            let (name, source, base_url) = match script {
                NativePageScript::Module {
                    name,
                    source,
                    base_url,
                    ..
                }
                | NativePageScript::ModuleDependency {
                    name,
                    source,
                    base_url,
                } => (name, source, base_url),
                NativePageScript::Classic { .. } | NativePageScript::ImportMap(_) => continue,
            };
            new_sources.entry(name.clone()).or_insert(source);
            new_base_urls.entry(name).or_insert(base_url);
        }
        let combined_entries = existing_sources.len().saturating_add(new_sources.len());
        if combined_entries > MAX_NATIVE_MODULE_IMPORTS {
            return Err(NativeEngineError::limit(
                "native page module graph entries",
                MAX_NATIVE_MODULE_IMPORTS,
                combined_entries,
            ));
        }
        let combined_bytes = existing_bytes.saturating_add(
            new_sources
                .values()
                .map(String::len)
                .fold(0usize, usize::saturating_add),
        );
        let maximum_module_bytes =
            MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS);
        if combined_bytes > maximum_module_bytes {
            return Err(NativeEngineError::limit(
                "native page module graph bytes",
                maximum_module_bytes,
                combined_bytes,
            ));
        }
        runtime.extend_module_sources(new_sources, new_base_urls)?;
        runtime.register_dynamic_module_alias(request_id, &target)
    }

    fn apply_local_script_command_batch(
        &mut self,
        commands: &[NativeScriptCommand],
        allow_script_navigation: bool,
        pending_fetches: &mut Vec<NativeScriptCommand>,
    ) -> Result<Option<NativeNavigationRequest>, NativeEngineError> {
        if commands.is_empty() {
            return Ok(None);
        }
        if commands.iter().any(|command| {
            matches!(
                command,
                NativeScriptCommand::Fetch { .. }
                    | NativeScriptCommand::WebSocketOpen { .. }
                    | NativeScriptCommand::WebSocketSend { .. }
                    | NativeScriptCommand::WebSocketClose { .. }
                    | NativeScriptCommand::EventSourceOpen { .. }
                    | NativeScriptCommand::EventSourceClose { .. }
                    | NativeScriptCommand::FetchStreamRead { .. }
                    | NativeScriptCommand::FetchStreamCancel { .. }
            )
        }) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "script network transport requires a process-backed HTTP(S) document"
                    .into(),
            });
        }

        let mut effective_commands = commands.to_vec();
        let mut document = self.document.clone();
        let (mut events, font_face_follow_up_commands) = self
            .apply_local_document_commands_with_font_face_ack(
                &mut document,
                commands,
                allow_script_navigation,
            )?;
        effective_commands.extend(font_face_follow_up_commands.iter().cloned());
        let mut history_commands = self.prepare_local_history_commands(&effective_commands)?;
        let mut scroll_commands = extract_local_scroll_commands(&effective_commands);
        if font_face_follow_up_commands.iter().any(|command| {
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
        let mut dynamic_navigation = None;
        let mut dynamic_dialogs = Vec::new();
        if let Some(javascript) = self.javascript.as_ref() {
            let dynamic_sources = document.take_newly_attached_page_script_sources(
                &effective_commands,
                super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                MAX_NATIVE_SCRIPT_BYTES,
            );
            if !dynamic_sources.is_empty() {
                let dynamic_result = execute_local_dynamic_page_script_batches(
                    &mut document,
                    javascript,
                    dynamic_sources,
                    &mut self.loader,
                    &self.url,
                    &self.origin,
                    self.config.viewport,
                )?;
                if !dynamic_result.pending_script_sources.is_empty() {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason: "dynamic external/module scripts require a process-backed HTTP(S) document"
                            .into(),
                    });
                }
                pending_fetches.extend(dynamic_result.pending_fetches);
                if !dynamic_result.websocket_commands.is_empty()
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
        let (stylesheet_events, image_events, media_events) =
            if let Some(javascript) = self.javascript.as_ref() {
                let stylesheet_events = load_local_dynamic_stylesheets(
                    &mut document,
                    javascript,
                    &mut self.loader,
                    &self.url,
                    self.config.viewport,
                )?;
                let image_events = load_local_dynamic_images(
                    &mut document,
                    javascript,
                    &mut self.loader,
                    &self.url,
                    self.config.viewport,
                )?;
                let media_events = load_local_dynamic_blob_media(
                    &mut document,
                    javascript,
                    &mut self.loader,
                    &self.url,
                )?;
                (stylesheet_events, image_events, media_events)
            } else {
                (Vec::new(), Vec::new(), Vec::new())
            };
        for (node_index, event_kind) in stylesheet_events {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            if let Some(evaluation) =
                self.evaluate_local_events(&document, &[(node_id, event_kind)])?
            {
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
            }
            events.push((node_id, event_kind));
        }
        for (node_index, event_kind) in image_events {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            if let Some(evaluation) =
                self.evaluate_local_events(&document, &[(node_id, event_kind)])?
            {
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
            }
            events.push((node_id, event_kind));
        }
        for (node_index, event_kind) in media_events {
            let node_id = NativeNodeId::from_parts(document.generation(), node_index);
            if let Some(evaluation) =
                self.evaluate_local_events(&document, &[(node_id, event_kind)])?
            {
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
            }
            events.push((node_id, event_kind));
        }
        let validation_events = events
            .iter()
            .filter(|(_, kind)| *kind == NativeEventKind::Invalid)
            .copied()
            .collect::<Vec<_>>();
        if !validation_events.is_empty()
            && let Some(evaluation) = self.evaluate_local_events(&document, &validation_events)?
        {
            retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
            let (effects, follow_up_commands) = self
                .apply_local_document_commands_with_font_face_ack(
                    &mut document,
                    &evaluation.commands,
                    false,
                )?;
            events.extend(effects);
            self.retain_local_font_face_follow_up_commands(
                &mut effective_commands,
                &mut scroll_commands,
                &mut history_commands,
                follow_up_commands,
            )?;
        }
        let mut navigation = if allow_script_navigation {
            self.script_navigation_target(&document, &effective_commands)?
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
            ..
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
                retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
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
                let (effects, follow_up_commands) = self
                    .apply_local_document_commands_with_font_face_ack(
                        &mut document,
                        &evaluation.commands,
                        false,
                    )?;
                events.extend(effects);
                self.retain_local_font_face_follow_up_commands(
                    &mut effective_commands,
                    &mut scroll_commands,
                    &mut history_commands,
                    follow_up_commands,
                )?;
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
                    retain_page_script_fetch_commands(&evaluation.commands, pending_fetches);
                    events.extend(invalid_events);
                    let (effects, follow_up_commands) = self
                        .apply_local_document_commands_with_font_face_ack(
                            &mut document,
                            &evaluation.commands,
                            false,
                        )?;
                    events.extend(effects);
                    self.retain_local_font_face_follow_up_commands(
                        &mut effective_commands,
                        &mut scroll_commands,
                        &mut history_commands,
                        follow_up_commands,
                    )?;
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
                    let mut request = NativeNavigationRequest::get(target_url.clone());
                    request.object_url = self
                        .javascript
                        .as_ref()
                        .map(|javascript| javascript.object_url_resource(&target_url))
                        .transpose()?
                        .flatten();
                    Some(request)
                }
            }
            Some(ScriptNavigationTarget::Form {
                form_id,
                submitter,
                target,
                ..
            }) => {
                let mut request = self
                    .document
                    .form_submission_request_with_submitter(form_id, &self.url, submitter)?;
                request.target = Some(target);
                request.object_url = self
                    .javascript
                    .as_ref()
                    .map(|javascript| javascript.object_url_resource(&request.url))
                    .transpose()?
                    .flatten();
                self.loader
                    .allows_navigation(
                        &self.url,
                        &request.url,
                        NativeNavigationPolicyKind::FormAction,
                    )?
                    .then_some(request)
            }
            Some(ScriptNavigationTarget::Location {
                href,
                replace_history,
            }) => {
                let mut request = NativeNavigationRequest::get(self.resolve_link_href(&href)?);
                request.replace_history = replace_history;
                request.object_url = self
                    .javascript
                    .as_ref()
                    .map(|javascript| javascript.object_url_resource(&request.url))
                    .transpose()?
                    .flatten();
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
                        let target = document.form_submission_target(form_id, Some(id))?;
                        document.form_submission_request_with_submitter(
                            form_id,
                            &self.url,
                            Some(id),
                        )?;
                        Some(ScriptNavigationTarget::Form {
                            form_id,
                            dispatch_submit: true,
                            submitter: Some(id),
                            target,
                        })
                    } else {
                        None
                    }
                }
                super::javascript::NativeScriptCommand::SubmitForm { node_index } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    let target = document.form_submission_target(id, None)?;
                    document.form_submission_request(id, &self.url)?;
                    Some(ScriptNavigationTarget::Form {
                        form_id: id,
                        dispatch_submit: false,
                        submitter: None,
                        target,
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
                    let target = document.form_submission_target(id, submitter)?;
                    document.form_submission_request_with_submitter(id, &self.url, submitter)?;
                    Some(ScriptNavigationTarget::Form {
                        form_id: id,
                        dispatch_submit: true,
                        submitter,
                        target,
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
        if request.method != navigation.method
            || request.body != navigation.body
            || request.body_content_type != navigation.body_content_type
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script form request payload changed during transfer".into(),
            });
        }
        let request_target = request.target.as_deref().unwrap_or("_self");
        if !request_target.eq_ignore_ascii_case(&navigation.target) {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script form target changed during transfer".into(),
            });
        }
        let target_url = self.resolve_link_href(&request.url)?;
        let same_document = self.is_same_document_navigation(&target_url);
        let target_kind = request_target.to_ascii_lowercase();
        let targets_current_document = match target_kind.as_str() {
            "_self" => true,
            "_parent" | "_top" | "_unfencedtop" => self.frame_script_context.is_none(),
            _ => false,
        };
        let opens_new_target = self.document.link_opens_new_target(id);
        if (same_document || download_attribute.is_some() || opens_new_target)
            && !self
                .allows_top_level_navigation_async(&target_url, true)
                .await?
        {
            return Ok(());
        }
        if let Some(download_attribute) = download_attribute {
            self.queue_download(target_url, &download_attribute)?;
            return Ok(());
        }
        if opens_new_target {
            self.queue_popup(target_url)?;
            return Ok(());
        }
        request.url = target_url.clone();
        request.replace_history = navigation.replace_history;
        if request.method == NativeNavigationMethod::Get
            && same_document
            && targets_current_document
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
        self.outgoing_lifecycle_dispatch_depth += 1;
        let result = self.dispatch_local_navigation_lifecycle_inner();
        self.outgoing_lifecycle_dispatch_depth -= 1;
        result
    }

    fn dispatch_local_navigation_lifecycle_inner(
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
        self.outgoing_lifecycle_dispatch_depth += 1;
        let result = self.dispatch_local_before_unload_inner();
        self.outgoing_lifecycle_dispatch_depth -= 1;
        result
    }

    fn dispatch_local_before_unload_inner(
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
            return Ok((allowed, navigation));
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
        Ok((allowed, navigation))
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
        let evaluation = {
            let javascript = self
                .javascript
                .as_ref()
                .expect("local JavaScript runtime is present");
            javascript.set_sync_xhr_loader(&self.loader);
            let result = javascript.evaluate_with_page_events(
                "undefined;",
                &self.document,
                new_url,
                &self.origin,
                self.config.viewport,
                &page_events,
            );
            if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            result?
        };
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
        self.evaluate_local_host_events(document, &event_batch)
    }

    fn evaluate_local_transition_event(
        &mut self,
        document: &NativeDocument,
        events: &[(NativeNodeId, NativeEventKind)],
        index: usize,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let event_metadata = events
            .iter()
            .map(|(node_id, kind)| (node_id.index(), *kind))
            .collect::<Vec<_>>();
        let Some(event_batch) = host_event_batch_at(&event_metadata, index)? else {
            return Ok(None);
        };
        self.evaluate_local_host_events(document, &event_batch)
    }

    fn evaluate_local_host_events(
        &mut self,
        document: &NativeDocument,
        event_batch: &[NativeHostEvent],
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    fn evaluate_local_click_event(
        &mut self,
        document: &NativeDocument,
        node_id: NativeNodeId,
        modifiers: u8,
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let event_batch = host_click_event_batch_with_modifiers(node_id.index(), modifiers)?;
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        javascript.set_scroll_offset(self.scroll_offset);
        javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
        self.sync_javascript_history();
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
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
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
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
        javascript.set_sync_xhr_loader(&self.loader);
        let evaluation_result = javascript.evaluate_with_host_events(
            &event_batch,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        );
        if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
            self.loader.merge_fetch_task_state(updated_loader)?;
        }
        let evaluation = evaluation_result?;
        self.drain_local_popups()?;
        self.drain_local_dialogs()?;
        self.persist_local_script_state()?;
        Ok(Some(evaluation))
    }

    fn action_local_click_with_event_preflight(
        &mut self,
        id: NativeNodeId,
        modifiers: u8,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut history_commands = Vec::new();
        let mut events = document.apply_script_focus(id)?;
        if let Some(evaluation) = self.evaluate_local_events(&document, &events)? {
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &evaluation.commands,
                &mut history_commands,
            )?);
        }

        let checkable_pre_activation = document.pre_activate_checkable(id)?;

        let click_evaluation = self
            .evaluate_local_click_event(&document, id, modifiers)?
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
        events.extend(self.apply_local_evaluation_commands(
            &mut document,
            &click_evaluation.commands,
            &mut history_commands,
        )?);
        let mut navigation: Option<(NativeNodeId, NativeNodeId)> = None;
        let mut link_navigation = None;
        if click_allowed {
            let click_events = if checkable_pre_activation.is_some() {
                document.apply_click_after_checkable_pre_activation(id)?
            } else {
                document.apply_click(id)?
            };
            let input_change_events = click_events
                .iter()
                .copied()
                .filter(|(_, kind)| {
                    matches!(kind, NativeEventKind::Input | NativeEventKind::Change)
                })
                .collect::<Vec<_>>();
            events.extend(click_events);
            if !input_change_events.is_empty()
                && let Some(evaluation) =
                    self.evaluate_local_events(&document, &input_change_events)?
            {
                events.extend(self.apply_local_evaluation_commands(
                    &mut document,
                    &evaluation.commands,
                    &mut history_commands,
                )?);
            }
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
                    events.extend(self.apply_local_evaluation_commands(
                        &mut document,
                        &submit_evaluation.commands,
                        &mut history_commands,
                    )?);
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
                        events.extend(invalid_events);
                        events.extend(self.apply_local_evaluation_commands(
                            &mut document,
                            &evaluation.commands,
                            &mut history_commands,
                        )?);
                    }
                }
            }
        } else {
            if let Some(checkable_pre_activation) = checkable_pre_activation {
                document.restore_checkable_pre_activation(checkable_pre_activation)?;
            }
            events.push((id, NativeEventKind::Click));
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native click event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        if let Some(href) = link_navigation.as_deref() {
            self.preflight_local_link_navigation(&document, id, href)?;
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
            return self.activate_link(id, &href, true, modifier_click_opens_new_target(modifiers));
        }
        if let Some((form_id, submitter)) = navigation {
            let request = self.document.form_submission_request_with_submitter(
                form_id,
                &self.url,
                Some(submitter),
            )?;
            if !self.loader.allows_navigation(
                &self.url,
                &request.url,
                NativeNavigationPolicyKind::FormAction,
            )? {
                return Ok(NativeActionResult {
                    revision: self.revision,
                    accepted: true,
                });
            }
            return self.navigate_local_form_request(request);
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
        document: &NativeDocument,
        id: NativeNodeId,
        href: &str,
    ) -> Result<(), NativeEngineError> {
        if document.link_download_attribute(id).is_some() || document.link_opens_new_target(id) {
            return Ok(());
        }
        let target_url = self.resolve_link_href(href)?;
        if !self.allows_frame_navigation(&target_url)? {
            return Ok(());
        }
        if !self.allows_top_level_navigation_silent(&target_url)? {
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
        if !self.loader.allows_navigation(
            &self.url,
            &request.url,
            NativeNavigationPolicyKind::FormAction,
        )? {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        self.navigate_local_form_request(request)
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
                events.extend(self.apply_local_evaluation_commands(
                    &mut document,
                    &evaluation.commands,
                    &mut history_commands,
                )?);
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
        for index in 0..default_events.len() {
            if let Some(evaluation) =
                self.evaluate_local_transition_event(&document, &default_events, index)?
            {
                events.extend(self.apply_local_evaluation_commands(
                    &mut document,
                    &evaluation.commands,
                    &mut history_commands,
                )?);
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

    fn apply_local_keyboard_activation(
        &mut self,
        document: &mut NativeDocument,
        id: NativeNodeId,
        events: &mut Vec<(NativeNodeId, NativeEventKind)>,
        history_commands: &mut Vec<NativeScriptCommand>,
    ) -> Result<Option<NativeLocalKeyboardNavigation>, NativeEngineError> {
        if document.focused_node() != id
            || (!document.has_native_keyboard_button_activation(id)
                && !document.has_native_keyboard_link_activation(id)
                && !document.has_native_keyboard_checkable_activation(id))
        {
            return Ok(None);
        }

        let checkable_pre_activation = document.pre_activate_checkable(id)?;

        let click_evaluation =
            self.evaluate_local_events(document, &[(id, NativeEventKind::Click)])?;
        let click_allowed = if let Some(evaluation) = click_evaluation.as_ref() {
            evaluation
                .value
                .as_array()
                .and_then(|values| values.first())
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "native keyboard click preflight".into(),
                    reason: "native click event result was invalid".into(),
                })?
        } else {
            true
        };
        if let Some(evaluation) = click_evaluation {
            events.extend(self.apply_local_evaluation_commands(
                document,
                &evaluation.commands,
                history_commands,
            )?);
        }

        let mut navigation = None;
        if click_allowed {
            let click_events = if checkable_pre_activation.is_some() {
                document.apply_click_after_checkable_pre_activation(id)?
            } else {
                document.apply_click(id)?
            };
            let input_change_events = click_events
                .iter()
                .copied()
                .filter(|(_, kind)| {
                    matches!(kind, NativeEventKind::Input | NativeEventKind::Change)
                })
                .collect::<Vec<_>>();
            events.extend(click_events);
            if !input_change_events.is_empty()
                && let Some(evaluation) =
                    self.evaluate_local_events(document, &input_change_events)?
            {
                events.extend(self.apply_local_evaluation_commands(
                    document,
                    &evaluation.commands,
                    history_commands,
                )?);
            }
            if let Some(href) = document
                .link_href(id)
                .filter(|href| !href.is_empty())
                .map(str::to_owned)
            {
                self.preflight_local_link_navigation(document, id, &href)?;
                navigation = Some(NativeLocalKeyboardNavigation::Link { target: id, href });
            } else if let Some(form_id) = document.reset_control_form(id) {
                if let Some(javascript) = self.javascript.as_ref() {
                    javascript.set_scroll_offset(self.scroll_offset);
                    javascript.set_nested_scroll_offsets(self.nested_scroll_offsets.clone());
                    self.sync_javascript_history();
                    javascript.set_sync_xhr_loader(&self.loader);
                    let command = serde_json::json!({
                        "kind": "activateFormReset",
                        "node_index": form_id.index(),
                    });
                    let command =
                        serde_json::to_string(&command).map_err(|_| NativeEngineError::Worker {
                            operation: "activate native keyboard reset button".into(),
                            reason: "reset form target could not be encoded".into(),
                        })?;
                    let evaluation_result = javascript.evaluate(
                        &format!("globalThis.__glassApplyNativeCommand({command})"),
                        document,
                        &self.url,
                        &self.origin,
                        self.config.viewport,
                    );
                    if let Some(updated_loader) = javascript.take_sync_xhr_loader() {
                        self.loader.merge_fetch_task_state(updated_loader)?;
                    }
                    let evaluation = evaluation_result?;
                    events.extend(self.apply_local_evaluation_commands(
                        document,
                        &evaluation.commands,
                        history_commands,
                    )?);
                    self.drain_local_popups()?;
                    self.drain_local_dialogs()?;
                    self.persist_local_script_state()?;
                } else {
                    events.push((form_id, NativeEventKind::Reset));
                    document.reset_form_controls(form_id)?;
                }
            } else if let Some(form_id) = document.submit_control_form(id) {
                let invalid = document.invalid_form_controls(form_id, Some(id))?;
                if invalid.is_empty() {
                    let submit_evaluation =
                        self.evaluate_local_submit_event(document, form_id, Some(id))?;
                    let submit_allowed = if let Some(evaluation) = submit_evaluation.as_ref() {
                        evaluation
                            .value
                            .as_array()
                            .and_then(|values| values.first())
                            .and_then(serde_json::Value::as_bool)
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "native keyboard submit event".into(),
                                reason: "native submit event result was invalid".into(),
                            })?
                    } else {
                        true
                    };
                    events.push((form_id, NativeEventKind::Submit));
                    if let Some(evaluation) = submit_evaluation {
                        events.extend(self.apply_local_evaluation_commands(
                            document,
                            &evaluation.commands,
                            history_commands,
                        )?);
                    }
                    if submit_allowed {
                        navigation = Some(NativeLocalKeyboardNavigation::FormSubmit {
                            form_id,
                            submitter: id,
                        });
                    }
                } else {
                    let invalid_events = invalid
                        .iter()
                        .copied()
                        .map(|invalid_id| (invalid_id, NativeEventKind::Invalid))
                        .collect::<Vec<_>>();
                    events.extend(invalid_events.iter().copied());
                    if let Some(evaluation) =
                        self.evaluate_local_events(document, &invalid_events)?
                    {
                        events.extend(self.apply_local_evaluation_commands(
                            document,
                            &evaluation.commands,
                            history_commands,
                        )?);
                    }
                }
            }
        } else {
            if let Some(checkable_pre_activation) = checkable_pre_activation {
                document.restore_checkable_pre_activation(checkable_pre_activation)?;
            }
            events.push((id, NativeEventKind::Click));
        }

        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native keyboard activation effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        if navigation.is_some()
            && history_commands.iter().any(|command| {
                matches!(command, NativeScriptCommand::HistoryGo { delta } if *delta != 0)
            })
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "history traversal cannot share keyboard activation with navigation"
                    .into(),
            });
        }
        Ok(navigation)
    }

    fn finish_local_keyboard_action(
        &mut self,
        document: NativeDocument,
        events: Vec<(NativeNodeId, NativeEventKind)>,
        history_commands: &[NativeScriptCommand],
        navigation: Option<NativeLocalKeyboardNavigation>,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let next_revision = self.next_revision()?;
        let mut document = document;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history
            .update_current_scroll(self.scroll_offset, &self.nested_scroll_offsets);
        let history_traversal =
            self.apply_local_history_commands_at(history_commands, next_revision)?;
        self.record_effects(events);

        match navigation {
            Some(NativeLocalKeyboardNavigation::Link { target, href }) => {
                return self.activate_link(target, &href, true, false);
            }
            Some(NativeLocalKeyboardNavigation::FormSubmit { form_id, submitter }) => {
                let request = self.document.form_submission_request_with_submitter(
                    form_id,
                    &self.url,
                    Some(submitter),
                )?;
                if self.loader.allows_navigation(
                    &self.url,
                    &request.url,
                    NativeNavigationPolicyKind::FormAction,
                )? {
                    return self.navigate_local_form_request(request);
                }
            }
            None => {}
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
        let event_target = document.keyboard_event_target(id);
        let mut events = vec![(event_target, kind)];
        let was_button = document.has_native_keyboard_button_activation(id);
        let was_link = document.has_native_keyboard_link_activation(id);
        let was_checkable_kind = document.keyboard_checkable_kind(id);
        let was_radio_arrow_target =
            document.can_apply_radio_group_arrow_navigation(id, key, modifiers);
        let evaluation = self.evaluate_local_key_event_with_modifiers(
            &document,
            event_target,
            kind,
            key,
            modifiers,
        )?;
        let event_allowed = evaluation
            .as_ref()
            .and_then(|evaluation| evaluation.value.as_array())
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if let Some(evaluation) = evaluation {
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &evaluation.commands,
                &mut history_commands,
            )?);
        }

        if kind == NativeEventKind::KeyDown
            && event_allowed
            && was_radio_arrow_target
            && document.focused_node() == id
            && document.can_apply_radio_group_arrow_navigation(id, key, modifiers)
        {
            let default_events = document.apply_key_default(id, key, modifiers)?;
            events.extend(default_events.clone());
            for index in 0..default_events.len() {
                if let Some(evaluation) =
                    self.evaluate_local_transition_event(&document, &default_events, index)?
                {
                    events.extend(self.apply_local_evaluation_commands(
                        &mut document,
                        &evaluation.commands,
                        &mut history_commands,
                    )?);
                }
            }
        }

        let mut submit_navigation = None;
        if kind == NativeEventKind::KeyDown && key == " " {
            self.pending_space_activation = None;
            if event_allowed && document.focused_node() == id {
                if was_button && document.has_native_keyboard_button_activation(id) {
                    self.pending_space_activation = Some((id, None));
                } else if let Some(expected) = was_checkable_kind
                    && document.keyboard_checkable_kind(id) == Some(expected)
                {
                    self.pending_space_activation = Some((id, Some(expected)));
                }
            }
        } else if kind == NativeEventKind::KeyUp && key == " " {
            let pending = self.pending_space_activation.take();
            let pending_checkable = pending.is_some_and(|(target, expected)| {
                target == id
                    && expected.is_some_and(|expected| {
                        document.keyboard_checkable_kind(id) == Some(expected)
                    })
            });
            let pending_button =
                pending == Some((id, None)) && document.has_native_keyboard_button_activation(id);
            if (pending_button && event_allowed || pending_checkable)
                && document.focused_node() == id
            {
                submit_navigation = self.apply_local_keyboard_activation(
                    &mut document,
                    id,
                    &mut events,
                    &mut history_commands,
                )?;
            }
        } else if kind == NativeEventKind::KeyDown
            && key == "Enter"
            && (was_button || was_link)
            && event_allowed
            && document.focused_node() == id
            && (document.has_native_keyboard_button_activation(id)
                || (was_link && document.has_native_keyboard_link_activation(id)))
        {
            submit_navigation = self.apply_local_keyboard_activation(
                &mut document,
                id,
                &mut events,
                &mut history_commands,
            )?;
        }

        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native key event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        self.finish_local_keyboard_action(document, events, &history_commands, submit_navigation)
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
        let keydown_target = document.keyboard_event_target(id);
        let mut events = vec![(keydown_target, NativeEventKind::KeyDown)];
        let keyboard_button_target = document.has_native_keyboard_button_activation(id);
        let keyboard_checkable_kind = document.keyboard_checkable_kind(id);
        let keyboard_radio_arrow_target =
            document.can_apply_radio_group_arrow_navigation(id, key, modifiers);
        let keyboard_link_target =
            key == "Enter" && document.has_native_keyboard_link_activation(id);
        let keydown = self.evaluate_local_key_event_with_modifiers(
            &document,
            keydown_target,
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
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &keydown.commands,
                &mut history_commands,
            )?);
        }

        if keydown_allowed && apply_default && should_apply_native_key_default(key, modifiers) {
            let default_events = if key == "Tab" {
                if document.focused_node() == id {
                    document.apply_tab_focus(modifiers & 8 != 0)?
                } else {
                    Vec::new()
                }
            } else if keyboard_radio_arrow_target {
                if document.can_apply_radio_group_arrow_navigation(id, key, modifiers) {
                    document.apply_key_default(id, key, modifiers)?
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
            for index in 0..default_events.len() {
                if let Some(evaluation) =
                    self.evaluate_local_transition_event(&document, &default_events, index)?
                {
                    events.extend(self.apply_local_evaluation_commands(
                        &mut document,
                        &evaluation.commands,
                        &mut history_commands,
                    )?);
                }
            }
        }

        let mut submit_navigation = None;
        if keydown_allowed
            && key == "Enter"
            && (keyboard_button_target || keyboard_link_target)
            && document.focused_node() == id
            && ((keyboard_button_target && document.has_native_keyboard_button_activation(id))
                || (keyboard_link_target && document.has_native_keyboard_link_activation(id)))
        {
            submit_navigation = self.apply_local_keyboard_activation(
                &mut document,
                id,
                &mut events,
                &mut history_commands,
            )?;
        }

        let keyup_target = document.keyboard_event_target(document.focused_node());
        events.push((keyup_target, NativeEventKind::KeyUp));
        if let Some(evaluation) = self.evaluate_local_key_event_with_modifiers(
            &document,
            keyup_target,
            NativeEventKind::KeyUp,
            key,
            modifiers,
        )? {
            events.extend(self.apply_local_evaluation_commands(
                &mut document,
                &evaluation.commands,
                &mut history_commands,
            )?);
        }
        let keyboard_checkable_target_remains = modifiers == 0
            && keyboard_checkable_kind
                .is_some_and(|expected| document.keyboard_checkable_kind(id) == Some(expected));
        if keydown_allowed
            && key == " "
            && document.focused_node() == id
            && ((keyboard_button_target && document.has_native_keyboard_button_activation(id))
                || keyboard_checkable_target_remains)
        {
            submit_navigation = self.apply_local_keyboard_activation(
                &mut document,
                id,
                &mut events,
                &mut history_commands,
            )?;
        }
        if key == " " {
            self.pending_space_activation = None;
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native key press effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }

        self.finish_local_keyboard_action(document, events, &history_commands, submit_navigation)
    }

    async fn apply_keyboard_mutation_async(
        &mut self,
        mutation: NativeContentMutation,
        keyboard_link_target: Option<NativeNodeId>,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let navigation = mutation.navigation.clone();
        let activated_link = keyboard_link_target.filter(|target| {
            mutation.allowed
                && mutation.events.iter().any(|event| {
                    event.node_index == target.index() && event.kind == NativeEventKind::Click
                })
        });
        let next_revision = self.next_revision()?;
        let outcome = self
            .apply_content_process_mutation_async_at(next_revision, mutation)
            .await?;
        if let Some(navigation) = navigation {
            self.navigate_script_navigation_async(navigation, 0).await?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: true,
            });
        }
        if let Some(target) = activated_link {
            return self
                .finish_content_keyboard_link_activation_async(target, outcome)
                .await;
        }
        Ok(outcome)
    }

    async fn finish_content_keyboard_link_activation_async(
        &mut self,
        target: NativeNodeId,
        outcome: NativeActionResult,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let Some(href) = self
            .document
            .link_href(target)
            .filter(|href| !href.is_empty())
            .map(str::to_owned)
        else {
            return Ok(outcome);
        };
        let download_attribute = self
            .document
            .link_download_attribute(target)
            .map(str::to_owned);
        if download_attribute.is_some()
            && self.pending_downloads.len() >= MAX_NATIVE_PENDING_DOWNLOADS
        {
            return Err(NativeEngineError::limit(
                "native pending downloads",
                MAX_NATIVE_PENDING_DOWNLOADS,
                self.pending_downloads.len().saturating_add(1),
            ));
        }
        let target_url = self.resolve_link_href(&href)?;
        let opens_new_target = self.document.link_opens_new_target(target);
        let special_navigation = download_attribute.is_some() || opens_new_target;
        if special_navigation
            && !self
                .allows_top_level_navigation_async(&target_url, true)
                .await?
        {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: outcome.accepted,
            });
        }
        if let Some(download_attribute) = download_attribute {
            self.queue_download(target_url, &download_attribute)?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: outcome.accepted,
            });
        }
        if opens_new_target {
            self.queue_popup(target_url)?;
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: outcome.accepted,
            });
        }
        self.navigate_request_async(NativeNavigationRequest::get(target_url), 0)
            .await?;
        Ok(NativeActionResult {
            revision: self.revision,
            accepted: outcome.accepted,
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
        mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.apply_content_process_mutation_async_at_with_lifecycle(next_revision, mutation, false)
            .await
    }

    async fn apply_content_process_lifecycle_mutation_async_at(
        &mut self,
        next_revision: u64,
        mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.apply_content_process_mutation_async_at_with_lifecycle(next_revision, mutation, true)
            .await
    }

    async fn apply_content_process_mutation_async_at_with_lifecycle(
        &mut self,
        next_revision: u64,
        mut mutation: NativeContentMutation,
        outgoing_lifecycle: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let csp_violations = std::mem::take(&mut mutation.csp_violations);
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
        self.dispatch_content_csp_violations_async(csp_violations)
            .await?;
        let history_traversal = self.apply_content_history_commands(&history)?;
        if outgoing_lifecycle
            && history_traversal.is_some_and(|delta| self.history_delta_crosses_document(delta))
        {
            return Err(NativeEngineError::invalid(
                "native history traversal",
                "cross-document traversal cannot re-enter an outgoing lifecycle callback",
            ));
        }
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

    fn history_delta_crosses_document(&self, delta: i32) -> bool {
        if delta == 0 || delta.unsigned_abs() as usize > MAX_NATIVE_HISTORY_DELTA as usize {
            return false;
        }
        let Some(current_index) = self.history.current_index() else {
            return false;
        };
        let Some(current_document_id) = self
            .history
            .entry(current_index)
            .map(|entry| entry.document_id)
        else {
            return false;
        };
        let mut index = current_index;
        for _ in 0..(delta.unsigned_abs() as usize).min(self.history.len()) {
            index = if delta < 0 {
                let Some(previous) = index.checked_sub(1) else {
                    break;
                };
                previous
            } else {
                let Some(next) = index.checked_add(1) else {
                    break;
                };
                next
            };
            let Some(entry) = self.history.entry(index) else {
                break;
            };
            if entry.document_id != current_document_id {
                return true;
            }
        }
        false
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
        let navigate_to_sources = mutation.navigate_to_sources.take();
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
        document.set_css_target_from_url(&self.url)?;
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
        self.document_navigate_to_sources = navigate_to_sources;
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
        force_new_target: bool,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let target_url = self.resolve_link_href(href)?;
        let revision = if click_already_applied {
            self.revision
        } else {
            self.next_revision()?
        };
        if !self.allows_top_level_navigation(&target_url)? {
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
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }
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
        if force_new_target || self.document.link_opens_new_target(id) {
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
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.javascript = None;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
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
            && (is_network_url(base.as_str()) || is_file_url(base.as_str()))
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
            && (is_network_url(base.as_str()) || is_file_url(base.as_str()))
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
        request.object_url = navigation.object_url;
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
        request.object_url = navigation.object_url;
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
            if !self.allows_top_level_navigation(&navigation.url)? {
                return Ok(());
            }
            let history_commit = if navigation.replace_history {
                HistoryCommit::Replace
            } else {
                HistoryCommit::Push
            };
            let resource = self.loader.load_navigation(&navigation)?;
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

    fn navigate_lifecycle_handoff_sync(
        &mut self,
        navigation: NativeNavigationRequest,
    ) -> Result<(), NativeEngineError> {
        let previous_skip = std::mem::replace(&mut self.skip_next_navigation_lifecycle, true);
        let result = self.navigate_page_script_sync(navigation, 1);
        if self.skip_next_navigation_lifecycle {
            self.skip_next_navigation_lifecycle = previous_skip;
        }
        result
    }

    fn synchronous_beforeunload_error() -> NativeEngineError {
        NativeEngineError::invalid(
            "native beforeunload prompt",
            "synchronous navigation cannot wait for a user decision; use an async native session with a responsive dialog controller",
        )
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

    fn prepare_navigation(&mut self, url: &str) -> Result<PreparedNavigation, NativeEngineError> {
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
        let mut document =
            NativeDocument::from_content_wire(content.document, &self.config.limits, generation)?;
        document.set_css_target_from_url(&content.url)?;
        document.set_viewport(self.config.viewport)?;
        Ok(PreparedNavigation {
            resource: NativeResource {
                url: content.url,
                origin: content.origin,
                body: String::new(),
            },
            document,
            frame_sources: content.frame_sources,
            navigate_to_sources: content.navigate_to_sources,
            dialogs: content.dialogs,
            initial_events,
            initial_scroll_commands: content.scroll_commands,
            execute_inline_scripts: false,
        })
    }

    fn prepare_navigation_resource(
        &mut self,
        resource: NativeResource,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let next_revision = self.next_revision()?;
        self.prepare_navigation_resource_at_revision(resource, next_revision)
    }

    fn prepare_navigation_resource_at_revision(
        &mut self,
        resource: NativeResource,
        revision: u64,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let generation = u32::try_from(revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let mut document =
            NativeDocument::parse_with_generation(&resource.body, &self.config.limits, generation)?;
        document.set_viewport(self.config.viewport)?;
        let initial_events = if is_file_url(&resource.url) {
            self.loader.apply_meta_content_security_policies(
                &resource.url,
                &document.content_security_policy_meta(),
            )?;
            document.mark_content_security_policy_meta_processed();
            let allowed_inline_style_nodes = super::content_process::inline_style_policy_nodes(
                &mut document,
                &mut self.loader,
                &resource.url,
            )?;
            let (stylesheet_states, mut events) = load_local_initial_file_stylesheets(
                &document,
                &self.loader,
                &resource.url,
                self.config.viewport,
            )?;
            let external_stylesheets = stylesheet_states
                .iter()
                .filter_map(|(_, href, stylesheet_url, body)| {
                    body.as_ref().map(|body| {
                        let stylesheet_url = stylesheet_url.as_deref().unwrap_or(href);
                        absolutize_stylesheet_urls(body, &resource.url, stylesheet_url)
                    })
                })
                .collect::<Vec<_>>();
            document = NativeDocument::parse_with_stylesheets_and_inline_style_policy(
                &resource.body,
                &self.config.limits,
                &external_stylesheets,
                generation,
                Some(&allowed_inline_style_nodes),
            )?;
            document.mark_inline_style_reports_seen();
            document.mark_content_security_policy_meta_processed();
            document.set_external_stylesheet_states(stylesheet_states);
            events.extend(load_local_initial_file_images(
                &mut document,
                &self.loader,
                &resource.url,
                self.config.viewport,
            )?);
            events.extend(load_local_initial_media(
                &mut document,
                &self.loader,
                &resource.url,
            )?);
            events
        } else {
            load_local_initial_media(&mut document, &self.loader, &resource.url)?
        };
        document.set_css_target_from_url(&resource.url)?;
        load_font_faces(&mut document, None, &mut self.loader, &resource.url)?;
        let frame_sources = self.loader.frame_sources_for_document(&resource.url)?;
        let navigate_to_sources = self.loader.navigation_sources_for_document(
            &resource.url,
            NativeNavigationPolicyKind::NavigateTo,
        )?;
        Ok(PreparedNavigation {
            resource,
            document,
            frame_sources,
            navigate_to_sources,
            dialogs: Vec::new(),
            initial_events,
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
            if !allowed && self.document_has_sticky_activation {
                return Err(Self::synchronous_beforeunload_error());
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
            if let Some(javascript) = javascript.as_ref() {
                javascript.set_sync_xhr_loader(&self.loader);
            }
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
                &mut self.loader,
            );
            if let Some(javascript) = javascript.as_ref()
                && let Some(updated_loader) = javascript.take_sync_xhr_loader()
            {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            let result = result?;
            if is_file_url(&prepared.resource.url) {
                super::content_process::apply_pending_meta_content_security_policies(
                    &mut prepared.document,
                    &mut self.loader,
                    &prepared.resource.url,
                )?;
                if let Some(runtime) = javascript.as_ref() {
                    self.synchronize_local_inline_csp_policy(
                        &prepared.document,
                        runtime,
                        &prepared.resource.url,
                    )?;
                }
            }
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
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.workers.clear();
        self.pending_message_port_messages.clear();
        self.javascript = javascript;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
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
        let mut initial_module_fetches = Vec::new();
        let page_navigation = if execute_page_scripts {
            if let Some(javascript) = javascript.as_ref() {
                javascript.set_sync_xhr_loader(&self.loader);
            }
            let inline_style_state_before = is_file_url(&prepared.resource.url)
                .then(|| inline_style_source_state(&prepared.document));
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
                &mut self.loader,
            );
            if let Some(javascript) = javascript.as_ref()
                && let Some(updated_loader) = javascript.take_sync_xhr_loader()
            {
                self.loader.merge_fetch_task_state(updated_loader)?;
            }
            let result = result?;
            if is_file_url(&prepared.resource.url) {
                super::content_process::apply_pending_meta_content_security_policies(
                    &mut prepared.document,
                    &mut self.loader,
                    &prepared.resource.url,
                )?;
                if let Some(runtime) = javascript.as_ref() {
                    self.synchronize_local_inline_csp_policy(
                        &prepared.document,
                        runtime,
                        &prepared.resource.url,
                    )?;
                }
            }
            if let Some(inline_style_state_before) = inline_style_state_before
                && inline_style_state_before != inline_style_source_state(&prepared.document)
            {
                refresh_rooted_file_inline_styles(
                    &mut prepared.document,
                    &mut self.loader,
                    &prepared.resource.url,
                    javascript.as_ref(),
                    &inline_style_state_before,
                )?;
            }
            initial_module_fetches.extend(result.pending_fetches.into_iter().filter(|command| {
                matches!(
                    command,
                    NativeScriptCommand::Fetch {
                        destination: Some(destination),
                        module_referrer: Some(_),
                        ..
                    } if destination == "module"
                )
            }));
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
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.workers.clear();
        self.pending_message_port_messages.clear();
        self.javascript = javascript;
        self.nested_scroll_offsets.clear();
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
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
            let mut page_navigation = match page_navigation {
                Some(page_navigation) => Some(self.page_navigation_request(page_navigation)?),
                None => self.dispatch_local_page_show()?,
            };
            if let Some(import_navigation) =
                self.apply_local_script_commands(&initial_module_fetches, true)?
            {
                if page_navigation.is_some() {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "one navigation commit cannot activate multiple page navigations"
                            .into(),
                    });
                }
                page_navigation = Some(import_navigation);
            }
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
        self.document.set_css_target_from_url(&url)?;
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
        self.document.set_css_target_from_url(&url)?;
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
            self.dispatch_content_events_async(&[NativeEventKind::PopState], false)
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
                    target: "_self".into(),
                    method: NativeNavigationMethod::Get,
                    body: None,
                    body_content_type: None,
                    location: true,
                    replace_history: navigation.replace_history,
                    object_url: None,
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
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.javascript = None;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
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
        self.flush_pending_lifecycle_effects();
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
        self.pending_space_activation = None;
        self.document = prepared.document;
        self.document_has_sticky_activation = false;
        self.javascript = None;
        self.nested_scroll_offsets = nested_scroll_offsets;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.document_frame_sources = prepared.frame_sources;
        self.document_navigate_to_sources = prepared.navigate_to_sources;
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
        self.flush_pending_lifecycle_effects();
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
        let same_document_target = self.history.is_same_document(history_index)
            || self.is_same_document_navigation(&target_url);
        if !same_document_target {
            if self.outgoing_lifecycle_dispatch_depth > 0 {
                return Err(NativeEngineError::invalid(
                    "native history traversal",
                    "cross-document traversal cannot re-enter an outgoing lifecycle callback",
                ));
            }
            if self.content_process.is_some() {
                return Err(NativeEngineError::invalid(
                    "synchronous native history traversal",
                    "process-backed cross-document history requires the asynchronous native history API",
                ));
            }
        }
        if !self.allows_top_level_navigation(&target_url)? {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if same_document_target {
            if let Some(navigation) = self.commit_same_document_navigation(
                target_url,
                HistoryCommit::Activate(history_index),
            )? {
                self.navigate_page_script_sync(navigation, 1)?;
            }
            return Ok(Some(self.snapshot_unchecked()));
        }
        if !self.allows_frame_navigation(&target_url)? {
            return Ok(Some(self.snapshot_unchecked()));
        }
        let current_history_index = self.history.current_index();
        let target_document_id = self
            .history
            .entry(history_index)
            .map(|entry| entry.document_id)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let (beforeunload_allowed, beforeunload_navigation) = if self.javascript.is_some() {
            self.dispatch_local_before_unload()?
        } else {
            (true, None)
        };
        if !beforeunload_allowed && self.document_has_sticky_activation {
            return Err(Self::synchronous_beforeunload_error());
        }
        let lifecycle_navigation = if self.javascript.is_some() {
            self.dispatch_local_navigation_lifecycle()?
        } else {
            None
        };
        if beforeunload_navigation.is_some() && lifecycle_navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "multiple outgoing lifecycle navigations are not supported".into(),
            });
        }
        if let Some(navigation) = beforeunload_navigation.or(lifecycle_navigation) {
            self.navigate_lifecycle_handoff_sync(navigation)?;
            return Ok(Some(self.snapshot_unchecked()));
        }
        if self.history.current_index() != current_history_index
            || self.history.entry(history_index).is_none_or(|entry| {
                entry.document_id != target_document_id || entry.url != target_url
            })
        {
            return Ok(Some(self.snapshot_unchecked()));
        }
        self.persist_local_web_storage()?;
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
        dispatch_lifecycle: bool,
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
        let target_document_id = self
            .history
            .entry(history_index)
            .map(|entry| entry.document_id)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let same_document_target = self.history.is_same_document(history_index)
            || self.is_same_document_navigation(&target_url);
        if !same_document_target && self.outgoing_lifecycle_dispatch_depth > 0 {
            return Err(NativeEngineError::invalid(
                "native history traversal",
                "cross-document traversal cannot re-enter an outgoing lifecycle callback",
            ));
        }
        let allowed = self
            .allows_top_level_navigation_async(&target_url, true)
            .await?;
        if !allowed {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if same_document_target {
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
        let current_history_index = self.history.current_index();
        let (lifecycle_allowed, lifecycle_navigation) = if dispatch_lifecycle {
            self.dispatch_navigation_lifecycle_async().await?
        } else {
            (true, None)
        };
        if !lifecycle_allowed {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if let Some(navigation) = lifecycle_navigation {
            return self
                .navigate_request_async_with_lifecycle(navigation, 0, false, None)
                .await
                .map(Some);
        }
        if self.history.current_index() != current_history_index
            || self.history.entry(history_index).is_none_or(|entry| {
                entry.document_id != target_document_id || entry.url != target_url
            })
        {
            return Ok(Some(self.snapshot_unchecked()));
        }
        if is_network_url(&target_url) {
            let referrer = referrer_for_navigation(&self.url, &target_url)?;
            self.ensure_content_process().await?;
            let Some((content, history_commit, page_navigation_handoffs, initial_url)) = self
                .load_content_with_page_navigation(
                    NativeNavigationRequest::get(target_url),
                    referrer,
                    history_commit,
                    0,
                    None,
                )
                .await?
            else {
                return Ok(Some(self.snapshot_unchecked()));
            };
            if !self.is_same_document_navigation(&content.url)
                && !self.allows_frame_navigation_after_redirect(&initial_url, &content.url)?
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
                        &initial_url,
                    )
                    .await?,
                ));
            }
            return Ok(Some(self.navigate_content(
                content,
                &initial_url,
                history_commit,
            )?));
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
                .traverse_history_async(direction, "history traversal", true)
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
        let hit = self.hit_test(x, y)?;
        let hit = hit.ok_or_else(|| NativeEngineError::TargetNotActionable {
            reason: "point hit no visible element".into(),
        })?;
        if self.document.node(hit).and_then(|node| node.element_name()) == Some("area") {
            return Ok(hit);
        }
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
        if self.document.node(id).and_then(|node| node.element_name()) == Some("area")
            && self.document.image_map_area_has_visible_image(id, &layout)
        {
            return Ok(());
        }
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
    frame_sources: Option<Vec<Vec<String>>>,
    navigate_to_sources: Option<Vec<Vec<String>>>,
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
        target: String,
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

fn load_local_dynamic_page_script_sources(
    sources: Vec<NativePageScriptSource>,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<(Vec<NativePageScript>, Vec<(u32, NativeEventKind)>), NativeEngineError> {
    let mut import_map = runtime.module_import_map()?;
    let mut scripts = vec![NativePageScript::ImportMap(import_map.clone())];
    let mut resource_events = Vec::new();
    for (index, source) in sources.into_iter().enumerate() {
        match source {
            NativePageScriptSource::ImportMap {
                source,
                node_index,
                nonce,
            } => {
                let Some(source) = source else {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                };
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                let Ok(parsed) = NativeModuleImportMap::parse(&source, document_url) else {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                };
                let mut merged = import_map.clone();
                if merged.merge(parsed).is_err() {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                import_map = merged;
                scripts.push(NativePageScript::ImportMap(import_map.clone()));
            }
            NativePageScriptSource::Inline {
                source,
                node_index,
                nonce,
                ..
            } => {
                let allowed =
                    loader.allows_inline_script(document_url, &source, nonce.as_deref())?;
                if !allowed {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                scripts.push(NativePageScript::Classic {
                    source,
                    base_url: document_url.to_owned(),
                    node_index: Some(node_index),
                });
            }
            NativePageScriptSource::ModuleInline {
                source,
                timing,
                node_index,
                nonce,
                ..
            } => {
                if !loader.allows_inline_script(document_url, &source, nonce.as_deref())? {
                    resource_events.push((node_index, NativeEventKind::Error));
                    continue;
                }
                let name =
                    format!("{document_url}#glass-local-dynamic-module-{node_index}-{index}");
                match load_local_file_module_graph_with_import_map(
                    loader,
                    document_url,
                    name,
                    document_url.to_owned(),
                    source,
                    timing,
                    node_index,
                    &mut import_map,
                ) {
                    Ok(graph) => scripts.extend(graph.into_iter().map(|(_, script)| script)),
                    Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
                scripts.push(NativePageScript::ImportMap(import_map.clone()));
            }
            NativePageScriptSource::External {
                href,
                node_index,
                nonce,
                integrity,
                parser_inserted,
                ..
            } => {
                let is_blob = href
                    .get(..5)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
                let is_file = is_file_subresource_source(document_url, &href);
                if !is_blob && !is_file {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason:
                            "dynamic external scripts require a rooted file or process-backed HTTP(S) document"
                                .into(),
                    });
                }
                let resource = if is_blob {
                    let object_url = runtime.object_url_resource(&href)?;
                    loader.load_local_blob_script(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        integrity.as_deref(),
                        object_url.as_ref(),
                    )
                } else {
                    loader.load_local_file_script_with_policy(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        parser_inserted,
                        nonce.as_deref(),
                        integrity.as_deref(),
                    )
                };
                match resource {
                    Ok(Some(resource)) => {
                        scripts.push(NativePageScript::Classic {
                            source: resource.body,
                            base_url: resource.url,
                            node_index: Some(node_index),
                        });
                        resource_events.push((node_index, NativeEventKind::Load));
                    }
                    Ok(None) | Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
            NativePageScriptSource::ModuleExternal {
                href,
                timing,
                node_index,
                nonce,
                integrity,
                parser_inserted,
                ..
            } => {
                let request_url = match resolve_module_request_url(document_url, &href) {
                    Ok(request_url) => request_url,
                    Err(_) => {
                        resource_events.push((node_index, NativeEventKind::Error));
                        continue;
                    }
                };
                let is_blob = href
                    .get(..5)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
                let is_file = is_file_subresource_source(document_url, &href);
                if !is_blob && !is_file {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason:
                            "dynamic external module scripts require a rooted file or process-backed HTTP(S) document"
                                .into(),
                    });
                }
                let resource = if is_blob {
                    let object_url = runtime.object_url_resource(&href)?;
                    loader.load_local_blob_script(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        integrity.as_deref(),
                        object_url.as_ref(),
                    )
                } else {
                    loader.load_local_file_script_with_policy(
                        document_url,
                        &href,
                        MAX_NATIVE_SCRIPT_BYTES,
                        parser_inserted,
                        nonce.as_deref(),
                        integrity.as_deref(),
                    )
                };
                match resource {
                    Ok(Some(resource)) => {
                        let response_url = resource.url;
                        let source = resource.body;
                        let graph = if is_file {
                            load_local_file_module_graph_with_import_map(
                                loader,
                                document_url,
                                request_url,
                                response_url,
                                source,
                                timing,
                                node_index,
                                &mut import_map,
                            )
                        } else {
                            Ok(vec![(
                                timing,
                                NativePageScript::Module {
                                    name: request_url,
                                    source,
                                    base_url: response_url,
                                    node_index: Some(node_index),
                                },
                            )])
                        };
                        match graph {
                            Ok(graph) => {
                                scripts.extend(graph.into_iter().map(|(_, script)| script));
                                resource_events.push((node_index, NativeEventKind::Load));
                            }
                            Err(_) => {
                                resource_events.push((node_index, NativeEventKind::Error));
                            }
                        }
                        scripts.push(NativePageScript::ImportMap(import_map.clone()));
                    }
                    Ok(None) | Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
        }
    }
    Ok((scripts, resource_events))
}

fn execute_local_dynamic_page_script_batches(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    mut sources: Vec<NativePageScriptSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let mut aggregate = NativePageScriptResult::default();
    let mut batches = 0usize;
    loop {
        batches = batches.saturating_add(1);
        if batches > super::javascript::MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native local dynamic script turns",
                super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                batches,
            ));
        }
        let (scripts, resource_events) =
            load_local_dynamic_page_script_sources(sources, runtime, loader, document_url)?;
        let inline_style_state_before =
            is_file_url(document_url).then(|| inline_style_source_state(document));
        runtime.set_sync_xhr_loader(loader);
        let mut result = execute_dynamic_page_scripts(
            document,
            runtime,
            scripts,
            document_url,
            document_origin,
            viewport,
            &resource_events,
            &[],
        )?;
        if is_file_url(document_url) {
            super::content_process::apply_pending_meta_content_security_policies(
                document,
                loader,
                document_url,
            )?;
            runtime.set_inline_script_policy(loader.inline_script_policy(document_url)?);
            runtime.mark_processed_inline_csp_meta_nodes(
                document.processed_content_security_policy_meta_nodes(),
            );
        }
        if let Some(inline_style_state_before) = inline_style_state_before {
            refresh_rooted_file_inline_styles(
                document,
                loader,
                document_url,
                Some(runtime),
                &inline_style_state_before,
            )?;
        }
        if let Some(updated_loader) = runtime.take_sync_xhr_loader() {
            loader.merge_fetch_task_state(updated_loader)?;
        }
        sources = std::mem::take(&mut result.pending_script_sources);
        merge_dynamic_page_script_result(&mut aggregate, result)?;
        if sources.is_empty() {
            return Ok(aggregate);
        }
    }
}

fn inline_style_source_state(
    document: &NativeDocument,
) -> (Vec<(u32, String, Option<String>)>, Vec<(u32, String)>) {
    (
        document.inline_style_elements(),
        document.inline_style_attributes(),
    )
}

fn script_commands_may_change_inline_styles(commands: &[NativeScriptCommand]) -> bool {
    commands.iter().any(|command| match command {
        NativeScriptCommand::SetAttribute { name, .. }
        | NativeScriptCommand::RemoveAttribute { name, .. } => {
            name.eq_ignore_ascii_case("style") || name.eq_ignore_ascii_case("nonce")
        }
        NativeScriptCommand::SetTextContent { .. }
        | NativeScriptCommand::SetInnerHtml { .. }
        | NativeScriptCommand::AppendChild { .. }
        | NativeScriptCommand::InsertBefore { .. }
        | NativeScriptCommand::RemoveNode { .. } => true,
        _ => false,
    })
}

fn refresh_rooted_file_inline_styles(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    runtime: Option<&NativeJavaScriptRuntime>,
    previous_state: &(Vec<(u32, String, Option<String>)>, Vec<(u32, String)>),
) -> Result<(), NativeEngineError> {
    if !is_file_url(document_url) {
        return Ok(());
    }
    let current_state = inline_style_source_state(document);
    if *previous_state == current_state {
        return Ok(());
    }
    super::content_process::refresh_inline_style_policy(document, loader, document_url)?;
    if previous_state.0 != current_state.0 {
        document.rebuild_external_stylesheet(document_url)?;
        load_font_faces(document, runtime, loader, document_url)?;
    }
    Ok(())
}

fn load_local_dynamic_stylesheets(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let links = document
        .external_stylesheet_links_with_nonce()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_STYLESHEETS)
        .collect::<Vec<_>>();
    let previous_states = document.external_stylesheet_states();
    let live_local_nodes = links
        .iter()
        .filter(|(_, href, _, _, _)| {
            href.get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"))
                || is_file_subresource_source(document_url, href)
        })
        .map(|(node_index, _, _, _, _)| *node_index)
        .collect::<BTreeSet<_>>();
    let mut states = document
        .external_stylesheet_states()
        .into_iter()
        .map(|(node_index, href, stylesheet_url, body)| (node_index, (href, stylesheet_url, body)))
        .collect::<BTreeMap<_, _>>();
    states.retain(|node_index, _| live_local_nodes.contains(node_index));
    let mut loaded_bytes = states
        .values()
        .filter_map(|(_, _, body)| body.as_ref())
        .map(String::len)
        .sum::<usize>();
    let mut events = Vec::new();

    for (node_index, href, integrity, _, nonce) in links {
        let is_blob = href
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_file = is_file_subresource_source(document_url, &href);
        if !is_blob && !is_file {
            continue;
        }
        if states
            .get(&node_index)
            .is_some_and(|(loaded_href, _, _)| loaded_href == &href)
        {
            continue;
        }
        if let Some((_, _, previous_body)) = states.get(&node_index)
            && let Some(previous_body) = previous_body
        {
            loaded_bytes = loaded_bytes.saturating_sub(previous_body.len());
        }
        let body = if is_blob {
            let object_url = runtime.object_url_resource(&href)?;
            match loader.load_local_blob_stylesheet(
                document_url,
                &href,
                integrity.as_deref(),
                object_url.as_ref(),
            ) {
                Ok(Some(stylesheet)) => {
                    let next_len = loaded_bytes.saturating_add(stylesheet.len());
                    if next_len <= MAX_NATIVE_LOCAL_STYLESHEET_BYTES {
                        loaded_bytes = next_len;
                        Some(stylesheet)
                    } else {
                        None
                    }
                }
                Ok(None) | Err(_) => None,
            }
        } else {
            match loader.load_local_file_stylesheet_with_nonce(
                document_url,
                &href,
                integrity.as_deref(),
                nonce.as_deref(),
            ) {
                Ok(Some(stylesheet)) => expand_local_file_stylesheet_imports(
                    loader,
                    document_url,
                    &href,
                    stylesheet,
                    &mut loaded_bytes,
                    viewport,
                )
                .ok(),
                Ok(None) | Err(_) => None,
            }
        };
        let event_kind = body
            .as_ref()
            .map_or(NativeEventKind::Error, |_| NativeEventKind::Load);
        states.insert(node_index, (href, None, body));
        events.push((node_index, event_kind));
    }

    let next_states = states
        .into_iter()
        .map(|(node_index, (href, stylesheet_url, body))| (node_index, href, stylesheet_url, body))
        .collect::<Vec<_>>();
    if previous_states != next_states {
        document.set_external_stylesheet_states(next_states);
        document.rebuild_external_stylesheet(document_url)?;
        load_font_faces(document, Some(runtime), loader, document_url)?;
        document.refresh_background_image_sources();
    }
    Ok(events)
}

fn load_local_dynamic_images(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut events = Vec::new();
    for (node_index, source) in document
        .external_image_links(viewport)
        .into_iter()
        .take(MAX_NATIVE_LOCAL_IMAGES)
    {
        let is_blob = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_file = is_file_subresource_source(document_url, &source);
        if !is_blob && !is_file {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.image_resource_for_node(node_id).is_some() {
            continue;
        }
        document.mark_image_load(node_index, source.clone(), viewport)?;
        let image = if is_blob {
            let object_url = runtime.object_url_resource(&source)?;
            loader.load_local_blob_image(document_url, &source, object_url.as_ref())?
        } else {
            match loader.load_local_file_image(document_url, &source) {
                Ok(image) => image,
                Err(_) => None,
            }
        };
        let event_kind = match image {
            Some(image) => {
                document.set_image_resource(node_index, source, image)?;
                NativeEventKind::Load
            }
            None => NativeEventKind::Error,
        };
        events.push((node_index, event_kind));
    }
    document.refresh_background_image_sources();
    for (node_index, source) in document
        .external_background_image_links()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_IMAGES)
    {
        let is_blob = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_file = is_file_subresource_source(document_url, &source);
        if !is_blob && !is_file {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document
            .background_image_resource_for_node(node_id)
            .is_some()
        {
            continue;
        }
        let image = if is_blob {
            let object_url = runtime.object_url_resource(&source)?;
            loader.load_local_blob_image(document_url, &source, object_url.as_ref())?
        } else {
            match loader.load_local_file_image(document_url, &source) {
                Ok(image) => image,
                Err(_) => None,
            }
        };
        if let Some(image) = image {
            document.set_background_image_resource(node_index, source, image)?;
        }
    }
    Ok(events)
}

fn load_local_dynamic_blob_media(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut events = Vec::new();
    for (node_index, source) in document
        .external_media_links()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_MEDIA)
    {
        let is_blob = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("blob:"));
        let is_data = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"));
        let is_file = is_file_subresource_source(document_url, &source);
        if !is_blob && !is_data && !is_file {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.has_media_resource_for_node(node_id) {
            continue;
        }
        document.mark_media_load(node_index, source.clone())?;
        let metadata = if is_blob {
            let object_url = runtime.object_url_resource(&source)?;
            loader.load_local_blob_media(document_url, &source, object_url.as_ref())?
        } else if is_file {
            loader.load_local_file_media(document_url, &source)?
        } else {
            loader.load_data_media(document_url, &source)?
        };
        let event_kind = match metadata {
            Some(metadata) => {
                document.set_media_resource(node_index, source, metadata)?;
                NativeEventKind::Load
            }
            None => {
                document.set_media_error(node_index, source)?;
                NativeEventKind::Error
            }
        };
        events.push((node_index, event_kind));
    }
    document.refresh_media_loads();
    Ok(events)
}

fn load_local_initial_file_stylesheets(
    document: &NativeDocument,
    loader: &NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
) -> Result<
    (
        Vec<(u32, String, Option<String>, Option<String>)>,
        Vec<(u32, NativeEventKind)>,
    ),
    NativeEngineError,
> {
    let mut states = Vec::new();
    let mut events = Vec::new();
    let mut loaded_bytes = 0usize;
    for (node_index, href, integrity, _, nonce) in document
        .external_stylesheet_links_with_nonce()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_STYLESHEETS)
    {
        let body = match loader.load_local_file_stylesheet_with_nonce(
            document_url,
            &href,
            integrity.as_deref(),
            nonce.as_deref(),
        ) {
            Ok(Some(stylesheet)) => expand_local_file_stylesheet_imports(
                loader,
                document_url,
                &href,
                stylesheet,
                &mut loaded_bytes,
                viewport,
            )
            .ok(),
            Ok(None) | Err(_) => None,
        };
        let event_kind = body
            .as_ref()
            .map_or(NativeEventKind::Error, |_| NativeEventKind::Load);
        states.push((node_index, href, None, body));
        events.push((node_index, event_kind));
    }
    Ok((states, events))
}

fn expand_local_file_stylesheet_imports(
    loader: &NativeResourceLoader,
    document_url: &str,
    stylesheet_href: &str,
    stylesheet: String,
    loaded_bytes: &mut usize,
    viewport: Viewport,
) -> Result<String, NativeEngineError> {
    let stylesheet_url = resolve_local_file_stylesheet_url(document_url, stylesheet_href)?;
    let mut graph_entries = 0usize;
    let mut graph_bytes = 0usize;
    let mut active = BTreeSet::new();
    let mut loaded = BTreeSet::from([stylesheet_url.clone()]);
    let expanded = expand_local_file_stylesheet_body(
        loader,
        document_url,
        &stylesheet_url,
        stylesheet,
        &mut graph_entries,
        &mut graph_bytes,
        &mut active,
        &mut loaded,
        viewport,
    )?;
    let next_bytes = loaded_bytes.saturating_add(graph_bytes);
    if next_bytes > MAX_NATIVE_LOCAL_STYLESHEET_BYTES {
        return Err(NativeEngineError::limit(
            "native file stylesheet bytes",
            MAX_NATIVE_LOCAL_STYLESHEET_BYTES,
            next_bytes,
        ));
    }
    *loaded_bytes = next_bytes;
    Ok(expanded)
}

fn expand_local_file_stylesheet_body(
    loader: &NativeResourceLoader,
    document_url: &str,
    stylesheet_url: &str,
    stylesheet: String,
    graph_entries: &mut usize,
    graph_bytes: &mut usize,
    active: &mut BTreeSet<String>,
    loaded: &mut BTreeSet<String>,
    viewport: Viewport,
) -> Result<String, NativeEngineError> {
    *graph_entries = graph_entries.saturating_add(1);
    if *graph_entries > MAX_NATIVE_LOCAL_STYLESHEETS {
        return Err(NativeEngineError::limit(
            "native file stylesheet graph entries",
            MAX_NATIVE_LOCAL_STYLESHEETS,
            *graph_entries,
        ));
    }
    *graph_bytes = graph_bytes.saturating_add(stylesheet.len());
    if *graph_bytes > MAX_NATIVE_LOCAL_STYLESHEET_BYTES {
        return Err(NativeEngineError::limit(
            "native file stylesheet graph bytes",
            MAX_NATIVE_LOCAL_STYLESHEET_BYTES,
            *graph_bytes,
        ));
    }
    active.insert(stylesheet_url.to_owned());
    let imports =
        static_css_imports(&stylesheet).map_err(|reason| NativeEngineError::UnsupportedUrl {
            reason: format!("file stylesheet import: {reason}"),
        })?;
    let mut expanded = String::with_capacity(stylesheet.len());
    let mut cursor = 0usize;
    for import in imports {
        expanded.push_str(&stylesheet[cursor..import.start]);
        if !css_import_matches(&import, viewport) {
            cursor = import.end;
            continue;
        }
        let target = resolve_local_file_stylesheet_url(stylesheet_url, &import.specifier)?;
        if active.contains(&target) || !loaded.insert(target.clone()) {
            cursor = import.end;
            continue;
        }
        let dependency = loader
            .load_local_file_stylesheet(document_url, &target, None)?
            .ok_or_else(|| NativeEngineError::Network {
                operation: "file stylesheet dependency".into(),
                reason: format!(
                    "file stylesheet dependency {:?} was blocked or unavailable",
                    import.specifier
                ),
            })?;
        let dependency = expand_local_file_stylesheet_body(
            loader,
            document_url,
            &target,
            dependency,
            graph_entries,
            graph_bytes,
            active,
            loaded,
            viewport,
        )?;
        if let Some(layer) = import.layer {
            expanded.push_str("@layer ");
            expanded.push_str(&layer);
            expanded.push_str(" { ");
            expanded.push_str(&dependency);
            expanded.push_str(" }");
        } else {
            expanded.push_str(&dependency);
        }
        cursor = import.end;
    }
    expanded.push_str(&stylesheet[cursor..]);
    active.remove(stylesheet_url);
    Ok(absolutize_stylesheet_urls(
        &expanded,
        document_url,
        stylesheet_url,
    ))
}

fn resolve_local_file_stylesheet_url(
    owner_url: &str,
    href: &str,
) -> Result<String, NativeEngineError> {
    validate_url_text("file stylesheet import URL", href)?;
    let href = decode_css_url_value(href).ok_or_else(|| NativeEngineError::UnsupportedUrl {
        reason: "file stylesheet import URL contains an invalid CSS escape".into(),
    })?;
    let owner_url = url::Url::parse(without_fragment(owner_url)).map_err(|_| {
        NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet owner URL is not valid URL syntax".into(),
        }
    })?;
    if !owner_url.scheme().eq_ignore_ascii_case("file") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet owner URL must use the file scheme".into(),
        });
    }
    let mut target = owner_url
        .join(&href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet import URL could not be resolved against its owner".into(),
        })?;
    if !target.scheme().eq_ignore_ascii_case("file") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet imports must use the file scheme".into(),
        });
    }
    if !target.username().is_empty() || target.password().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet imports must not contain credentials".into(),
        });
    }
    if target
        .host_str()
        .is_some_and(|host| !host.eq_ignore_ascii_case("localhost"))
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "file stylesheet imports must use an empty or localhost host".into(),
        });
    }
    target.set_fragment(None);
    Ok(target.to_string())
}

fn load_local_initial_file_images(
    document: &mut NativeDocument,
    loader: &NativeResourceLoader,
    document_url: &str,
    viewport: super::config::Viewport,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut events = Vec::new();
    for (node_index, source) in document
        .external_image_links(viewport)
        .into_iter()
        .take(MAX_NATIVE_LOCAL_IMAGES)
    {
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.image_resource_for_node(node_id).is_some() {
            continue;
        }
        document.mark_image_load(node_index, source.clone(), viewport)?;
        let image = if is_file_subresource_source(document_url, &source) {
            match loader.load_local_file_image(document_url, &source) {
                Ok(image) => image,
                Err(_) => None,
            }
        } else {
            None
        };
        let event_kind = match image {
            Some(image) => {
                document.set_image_resource(node_index, source, image)?;
                NativeEventKind::Load
            }
            None => NativeEventKind::Error,
        };
        events.push((node_index, event_kind));
    }
    document.refresh_background_image_sources();
    for (node_index, source) in document
        .external_background_image_links()
        .into_iter()
        .take(MAX_NATIVE_LOCAL_IMAGES)
    {
        if !is_file_subresource_source(document_url, &source) {
            continue;
        }
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document
            .background_image_resource_for_node(node_id)
            .is_some()
        {
            continue;
        }
        if let Some(image) = loader.load_local_file_image(document_url, &source)? {
            document.set_background_image_resource(node_index, source, image)?;
        }
    }
    Ok(events)
}

fn load_local_initial_media(
    document: &mut NativeDocument,
    loader: &NativeResourceLoader,
    document_url: &str,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut events = Vec::new();
    for (node_index, source) in document.external_media_links() {
        let is_data = source
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"));
        let is_file = is_file_subresource_source(document_url, &source);
        if !is_data && !is_file {
            continue;
        }
        document.mark_media_load(node_index, source.clone())?;
        let metadata = if is_file {
            loader.load_local_file_media(document_url, &source)?
        } else {
            loader.load_data_media(document_url, &source)?
        };
        let event_kind = match metadata {
            Some(metadata) => {
                document.set_media_resource(node_index, source, metadata)?;
                NativeEventKind::Load
            }
            None => {
                document.set_media_error(node_index, source)?;
                NativeEventKind::Error
            }
        };
        events.push((node_index, event_kind));
    }
    document.refresh_media_loads();
    Ok(events)
}

fn is_file_subresource_source(document_url: &str, source: &str) -> bool {
    if source
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    {
        return true;
    }
    let Ok(base) = url::Url::parse(without_fragment(document_url)) else {
        return false;
    };
    is_file_url(base.as_str())
        && base
            .join(source)
            .is_ok_and(|target| target.scheme().eq_ignore_ascii_case("file"))
}

fn retain_page_script_fetch_commands(
    commands: &[NativeScriptCommand],
    pending_fetches: &mut Vec<NativeScriptCommand>,
) {
    pending_fetches.extend(
        commands
            .iter()
            .filter(|command| matches!(command, NativeScriptCommand::Fetch { .. }))
            .cloned(),
    );
}

fn load_font_faces(
    document: &mut NativeDocument,
    runtime: Option<&NativeJavaScriptRuntime>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<(), NativeEngineError> {
    let mut resources = Vec::new();
    let system_fonts = NativeFontBook::system();
    for rule in document
        .font_face_rules()
        .iter()
        .take(MAX_NATIVE_FONT_FACES)
    {
        for source in &rule.sources {
            let bytes = match source {
                NativeFontFaceSource::Local(family) => system_fonts.local_font_bytes(
                    family,
                    FontWeightValue::from_numeric(rule.weight.nominal())
                        .unwrap_or(FontWeightValue::Normal),
                    rule.style,
                ),
                NativeFontFaceSource::Url(source) => {
                    let object_url = runtime
                        .map(|runtime| runtime.object_url_resource(source))
                        .transpose()?
                        .flatten();
                    loader
                        .load_font(document_url, source, object_url.as_ref())
                        .ok()
                        .flatten()
                }
            };
            if let Some(bytes) = bytes {
                resources.push(NativeFontFaceResource::from_rule(rule, bytes));
                break;
            }
        }
    }
    document.set_font_resources(resources)
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
    use super::super::dialog::{NativeDialogController, NativeDialogResolution};
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

    #[tokio::test]
    async fn beforeunload_prompt_holds_navigation_until_exact_user_decision() {
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://beforeunload-modal-start",
                "<script>globalThis.lifecycle = []; addEventListener('beforeunload', event => { lifecycle.push(event.type); event.preventDefault(); event.returnValue = 'must not be shown'; globalThis.beforeunloadMutation = 'seen'; }); addEventListener('pagehide', () => lifecycle.push('pagehide')); addEventListener('unload', () => lifecycle.push('unload'));</script><button id='activate'>Activate</button><title>Start</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://beforeunload-modal-next",
                "<title>Next</title>",
            )
            .unwrap()
            .with_initial_url("fixture://beforeunload-modal-start");
        let control = NativeDialogControlPlane::for_modal_owner();
        let controller = NativeDialogController::new(control.clone()).unwrap();
        let mut engine = NativeEngine::new_with_dialog_control(config, control).unwrap();
        engine.initialize_async().await.unwrap();
        engine
            .action_async(NativeAction::Click {
                target: "id=activate".into(),
            })
            .await
            .unwrap();

        let mut navigation = Box::pin(engine.navigate_async("fixture://beforeunload-modal-next"));
        let first_pending = loop {
            if let Some(pending) = controller.pending_dialog().unwrap() {
                break pending;
            }
            tokio::select! {
                result = &mut navigation => panic!("navigation completed before its prompt: {}", result.is_ok()),
                () = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        };
        assert_eq!(first_pending.dialog.dialog_type, "beforeunload");
        assert!(first_pending.dialog.message.is_empty());
        assert!(first_pending.dialog.default_value.is_none());
        assert!(
            controller
                .resolve_dialog(
                    "stale-beforeunload-id",
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: None,
                    },
                )
                .is_err()
        );
        controller
            .resolve_dialog(
                &first_pending.id,
                NativeDialogResolution {
                    accepted: false,
                    prompt_value: None,
                },
            )
            .unwrap();
        let snapshot = navigation.await.unwrap();
        assert_eq!(snapshot.url, "fixture://beforeunload-modal-start");
        assert_eq!(snapshot.title, "Start");
        assert_eq!(
            engine.evaluate_async("globalThis.lifecycle").await.unwrap(),
            serde_json::json!(["beforeunload"])
        );
        assert_eq!(
            engine
                .evaluate_async("globalThis.beforeunloadMutation")
                .await
                .unwrap(),
            serde_json::json!("seen")
        );

        let mut navigation = Box::pin(engine.navigate_async("fixture://beforeunload-modal-next"));
        let second_pending = loop {
            if let Some(pending) = controller.pending_dialog().unwrap() {
                break pending;
            }
            tokio::select! {
                result = &mut navigation => panic!("navigation completed before its prompt: {}", result.is_ok()),
                () = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        };
        assert_ne!(first_pending.id, second_pending.id);
        controller
            .resolve_dialog(
                &second_pending.id,
                NativeDialogResolution {
                    accepted: true,
                    prompt_value: None,
                },
            )
            .unwrap();
        let snapshot = navigation.await.unwrap();
        assert_eq!(snapshot.url, "fixture://beforeunload-modal-next");
        assert_eq!(snapshot.title, "Next");
        assert!(controller.pending_dialog().unwrap().is_none());
        engine.close_async().await.unwrap();
    }

    #[tokio::test]
    async fn local_history_beforeunload_waits_for_exact_confirmation() {
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://beforeunload-history-first",
                "<title>First</title><p>First history document</p>",
            )
            .unwrap()
            .with_fixture(
                "fixture://beforeunload-history-second",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'not shown'; globalThis.beforeunloadRan = 'yes'; }); addEventListener('pagehide', () => globalThis.pagehideRan = 'yes'); addEventListener('unload', () => globalThis.unloadRan = 'yes');</script><title>Second</title><button id='activate'>Activate</button>",
            )
            .unwrap()
            .with_initial_url("fixture://beforeunload-history-first");
        let control = NativeDialogControlPlane::for_modal_owner();
        let controller = NativeDialogController::new(control.clone()).unwrap();
        let mut engine = NativeEngine::new_with_dialog_control(config, control).unwrap();
        engine.initialize_async().await.unwrap();
        engine
            .navigate_async("fixture://beforeunload-history-second")
            .await
            .unwrap();
        engine
            .action_async(NativeAction::Click {
                target: "id=activate".into(),
            })
            .await
            .unwrap();

        let mut back = Box::pin(engine.go_back_async());
        let first_pending = loop {
            if let Some(pending) = controller.pending_dialog().unwrap() {
                break pending;
            }
            tokio::select! {
                result = &mut back => panic!("history traversal completed before confirmation: {}", result.is_ok()),
                () = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        };
        assert_eq!(first_pending.dialog.dialog_type, "beforeunload");
        assert_eq!(
            first_pending.dialog.url,
            "fixture://beforeunload-history-second"
        );
        controller
            .resolve_dialog(
                &first_pending.id,
                NativeDialogResolution {
                    accepted: false,
                    prompt_value: None,
                },
            )
            .unwrap();
        let dismissed = back.await.unwrap().unwrap();
        assert_eq!(dismissed.url, "fixture://beforeunload-history-second");
        assert_eq!(
            engine
                .evaluate_async("[globalThis.beforeunloadRan || null, globalThis.pagehideRan || null, globalThis.unloadRan || null]")
                .await
                .unwrap(),
            serde_json::json!(["yes", null, null])
        );

        let mut back = Box::pin(engine.go_back_async());
        let second_pending = loop {
            if let Some(pending) = controller.pending_dialog().unwrap() {
                break pending;
            }
            tokio::select! {
                result = &mut back => panic!("retry history traversal completed before confirmation: {}", result.is_ok()),
                () = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        };
        assert_ne!(first_pending.id, second_pending.id);
        controller
            .resolve_dialog(
                &second_pending.id,
                NativeDialogResolution {
                    accepted: true,
                    prompt_value: None,
                },
            )
            .unwrap();
        let accepted = back.await.unwrap().unwrap();
        assert_eq!(accepted.url, "fixture://beforeunload-history-first");
        engine.close_async().await.unwrap();
    }

    #[tokio::test]
    async fn synchronous_navigation_fails_closed_when_beforeunload_needs_confirmation() {
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://beforeunload-sync-start",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); globalThis.beforeunloadMutation = 'seen'; }); addEventListener('pagehide', () => globalThis.pagehideMutation = 'yes'); addEventListener('unload', () => globalThis.unloadMutation = 'yes');</script><button id='activate'>Activate</button><title>Start</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://beforeunload-sync-next",
                "<title>Next</title>",
            )
            .unwrap()
            .with_initial_url("fixture://beforeunload-sync-start");
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        engine
            .action(NativeAction::Click {
                target: "id=activate".into(),
            })
            .unwrap();

        let error = engine
            .navigate("fixture://beforeunload-sync-next")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("synchronous navigation cannot wait")
        );
        assert_eq!(
            engine.snapshot().unwrap().url,
            "fixture://beforeunload-sync-start"
        );
        assert_eq!(
            engine
                .evaluate_async("[globalThis.beforeunloadMutation || null, globalThis.pagehideMutation || null, globalThis.unloadMutation || null]")
                .await
                .unwrap(),
            serde_json::json!(["seen", null, null])
        );
        engine.close_async().await.unwrap();
    }

    #[tokio::test]
    async fn synchronous_local_history_beforeunload_fails_closed_after_activation() {
        let config = NativeEngineConfig::default()
            .with_fixture("fixture://sync-history-first", "<title>First</title>")
            .unwrap()
            .with_fixture(
                "fixture://sync-history-second",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'not shown'; globalThis.beforeunloadMutation = 'seen'; }); addEventListener('pagehide', () => globalThis.pagehideMutation = 'yes'); addEventListener('unload', () => globalThis.unloadMutation = 'yes');</script><title>Second</title><button id='activate'>Activate</button>",
            )
            .unwrap()
            .with_initial_url("fixture://sync-history-first");
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        engine.navigate("fixture://sync-history-second").unwrap();
        engine
            .action(NativeAction::Click {
                target: "id=activate".into(),
            })
            .unwrap();

        let current_history_index = engine.history().current_index();
        let error = engine.go_back().unwrap_err();

        assert!(
            error
                .to_string()
                .contains("synchronous navigation cannot wait")
        );
        assert_eq!(
            engine.snapshot().unwrap().url,
            "fixture://sync-history-second"
        );
        assert_eq!(engine.snapshot().unwrap().title, "Second");
        assert_eq!(engine.history().current_index(), current_history_index);
        assert_eq!(engine.history().len(), 2);
        assert_eq!(
            engine
                .evaluate_async(
                    "[globalThis.beforeunloadMutation || null, globalThis.pagehideMutation || null, globalThis.unloadMutation || null]",
                )
                .await
                .unwrap(),
            serde_json::json!(["seen", null, null])
        );
        engine.close_async().await.unwrap();
    }

    #[tokio::test]
    async fn synchronous_local_history_beforeunload_without_activation_commits_target() {
        let config = NativeEngineConfig::default()
            .with_fixture("fixture://sync-history-no-activation-first", "<title>First</title>")
            .unwrap()
            .with_fixture(
                "fixture://sync-history-no-activation-second",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); globalThis.beforeunloadRan = true; }); addEventListener('pagehide', () => globalThis.pagehideRan = true); addEventListener('unload', () => globalThis.unloadRan = true);</script><title>Second</title>",
            )
            .unwrap()
            .with_initial_url("fixture://sync-history-no-activation-first");
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        engine
            .navigate("fixture://sync-history-no-activation-second")
            .unwrap();
        let outgoing_window = NativeNodeId::from_parts(engine.document.generation(), u32::MAX);

        let snapshot = engine.go_back().unwrap().unwrap();

        assert_eq!(snapshot.url, "fixture://sync-history-no-activation-first");
        assert_eq!(engine.history().current_index(), Some(0));
        let lifecycle = engine
            .effects
            .iter()
            .filter(|effect| effect.node_id == outgoing_window)
            .map(|effect| effect.kind)
            .filter(|kind| {
                matches!(
                    kind,
                    NativeEventKind::BeforeUnload
                        | NativeEventKind::PageHide
                        | NativeEventKind::Unload
                )
            })
            .collect::<Vec<_>>();
        let beforeunload = lifecycle
            .iter()
            .position(|kind| *kind == NativeEventKind::BeforeUnload)
            .unwrap();
        let pagehide = lifecycle
            .iter()
            .position(|kind| *kind == NativeEventKind::PageHide)
            .unwrap();
        let unload = lifecycle
            .iter()
            .position(|kind| *kind == NativeEventKind::Unload)
            .unwrap();
        assert!(beforeunload < pagehide && pagehide < unload);
        engine.close_async().await.unwrap();
    }

    #[tokio::test]
    async fn synchronous_and_async_history_reentry_fails_closed_during_outgoing_dispatch() {
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://sync-history-reentry-first",
                "<title>First</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://sync-history-reentry-second",
                "<title>Second</title>",
            )
            .unwrap()
            .with_initial_url("fixture://sync-history-reentry-first");
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        engine
            .navigate("fixture://sync-history-reentry-second")
            .unwrap();
        let current_history_index = engine.history().current_index();
        engine.outgoing_lifecycle_dispatch_depth = 1;

        let synchronous_error = engine.go_back().unwrap_err();
        let asynchronous_error = engine.go_back_async().await.unwrap_err();
        engine.outgoing_lifecycle_dispatch_depth = 0;

        assert!(
            synchronous_error
                .to_string()
                .contains("cannot re-enter an outgoing lifecycle callback")
        );
        assert!(
            asynchronous_error
                .to_string()
                .contains("cannot re-enter an outgoing lifecycle callback")
        );
        assert_eq!(
            engine.snapshot().unwrap().url,
            "fixture://sync-history-reentry-second"
        );
        assert_eq!(engine.history().current_index(), current_history_index);
        assert_eq!(engine.snapshot().unwrap().title, "Second");
        engine.close_async().await.unwrap();
    }

    #[tokio::test]
    async fn synchronous_same_document_history_remains_in_place() {
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://sync-same-document-history",
                "<script>globalThis.beforeunloadCount = 0; addEventListener('beforeunload', () => { globalThis.beforeunloadCount += 1; });</script><title>History</title>",
            )
            .unwrap()
            .with_initial_url("fixture://sync-same-document-history#first");
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        engine
            .navigate("fixture://sync-same-document-history#second")
            .unwrap();

        assert_eq!(
            engine.go_back().unwrap().unwrap().url,
            "fixture://sync-same-document-history#first"
        );
        assert_eq!(
            engine.go_forward().unwrap().unwrap().url,
            "fixture://sync-same-document-history#second"
        );
        assert_eq!(engine.history().len(), 2);
        assert_eq!(
            engine
                .evaluate_async("globalThis.beforeunloadCount")
                .await
                .unwrap(),
            serde_json::json!(0)
        );
        engine.close_async().await.unwrap();
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

    #[tokio::test]
    async fn cssom_custom_property_mutations_recompute_inherited_font_size() {
        let config = NativeEngineConfig::default()
            .with_initial_url("fixture://cssom.test/index")
            .with_fixture(
                "fixture://cssom.test/index",
                "<style>#parent { --size: 1em; } #child { font-size: var(--size); }</style><div id='parent'><span id='child'>Child</span></div>",
            )
            .expect("CSSOM fixture must validate");
        let mut engine = NativeEngine::new(config).expect("native engine must construct");
        engine.initialize().expect("native engine must initialize");
        let child = engine
            .resolve_target("id=child")
            .expect("CSSOM child must resolve");

        assert_eq!(
            engine.document.computed_style_for_layout(child).font_size(),
            16
        );
        assert_eq!(
            engine
                .evaluate_async(
                    "document.getElementById('parent').style.setProperty('--size', '2em'); true"
                )
                .await
                .expect("CSSOM custom-property update must succeed"),
            serde_json::json!(true)
        );
        assert_eq!(
            engine.document.computed_style_for_layout(child).font_size(),
            32
        );
        assert_eq!(
            engine
                .evaluate_async(
                    "document.getElementById('parent').style.removeProperty('--size'); true"
                )
                .await
                .expect("CSSOM custom-property removal must succeed"),
            serde_json::json!(true)
        );
        assert_eq!(
            engine.document.computed_style_for_layout(child).font_size(),
            16
        );
    }

    #[tokio::test]
    async fn viewport_updates_recompute_css_and_script_dimensions() {
        let config = NativeEngineConfig::default()
            .with_initial_url("fixture://viewport.test/index")
            .with_viewport(Viewport {
                width: 1_000,
                height: 800,
                device_scale_factor_milli: 1_000,
            })
            .with_fixture(
                "fixture://viewport.test/index",
                "<style>#target { font-size: 2vw; }</style><span id='target'>Viewport</span>",
            )
            .expect("viewport fixture must validate");
        let mut engine = NativeEngine::new(config).expect("native engine must construct");
        engine.initialize().expect("native engine must initialize");
        let target = engine
            .resolve_target("id=target")
            .expect("viewport target must resolve");

        assert_eq!(
            engine
                .document
                .computed_style_for_layout(target)
                .font_size(),
            20
        );
        assert_eq!(
            engine
                .evaluate_async("({ width: innerWidth, height: innerHeight })")
                .await
                .expect("initial viewport evaluation must succeed"),
            serde_json::json!({"width": 1000, "height": 800})
        );

        engine
            .set_viewport_async(Viewport {
                width: 500,
                height: 400,
                device_scale_factor_milli: 1_000,
            })
            .await
            .expect("viewport update must succeed");

        assert_eq!(
            engine
                .document
                .computed_style_for_layout(target)
                .font_size(),
            10
        );
        assert_eq!(
            engine
                .evaluate_async("({ width: innerWidth, height: innerHeight })")
                .await
                .expect("updated viewport evaluation must succeed"),
            serde_json::json!({"width": 500, "height": 400})
        );
        assert_eq!(
            engine
                .snapshot()
                .expect("snapshot must succeed")
                .viewport
                .width,
            500
        );
    }
}

use super::browsing_context::NativeBrowsingContext;
use super::cancellation::NativeNavigationCancellation;
use super::config::{
    NativeEngineConfig, Viewport, decode_percent_encoded_fragment, decode_text_fragment_terms,
    is_file_url, is_network_url, resolve_fixture_relative_url, validate_context_id,
    validate_url_text, validate_window_name, without_fragment,
};
use super::content_process::{
    CONTENT_PROCESS_ASYNC_EFFECT_TIMEOUT, CONTENT_PROCESS_LIFECYCLE_TIMEOUT,
    CONTENT_PROCESS_SCRIPT_TIMEOUT, NativeContentAsyncEffectNotification, NativeContentLoad,
    NativeContentLoadResult, NativeContentMutation, NativeContentNavigation, NativeContentProcess,
    NativeContentScriptResult, merge_dynamic_page_script_result,
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
    DocumentCommandEffects, MAX_NATIVE_COOKIE_CHANGE_BATCH, MAX_NATIVE_DIALOG_TEXT_BYTES,
    MAX_NATIVE_DIALOGS, MAX_NATIVE_HISTORY_STATE_BYTES, MAX_NATIVE_MODULE_IMPORTS,
    MAX_NATIVE_SCRIPT_BYTES, MAX_NATIVE_WORKER_MESSAGES, NativeCookieChange,
    NativeCookieProfileEntry, NativeDialog, NativeDynamicPageScriptRequest,
    NativeFrameScriptBinding, NativeFrameScriptContext, NativeFrameScriptRequest,
    NativeHashChangeEvent, NativeHostEvent, NativeIndexedDbChange, NativeIndexedDbState,
    NativeInlinePageScriptRequest, NativeJavaScriptRuntime, NativeLocalModuleGraphRequest,
    NativeMessagePortPageMessage, NativeMessagePortTransfer, NativePageEventBatch,
    NativePageEventContext, NativePageMessageEvent, NativePageMessagePortCommand,
    NativePageNavigation, NativePageScript, NativePageScriptResult, NativePopupRequest,
    NativePostMessageRequest, NativeScriptCommand, NativeScriptEvaluation,
    NativeServiceWorkerClientLease, NativeServiceWorkerClientMessage,
    NativeServiceWorkerClientState, NativeServiceWorkerOpenWindowRequest,
    NativeSharedWorkerCreateRequest, NativeStorageEvent, NativeWebStorageState,
    NativeWindowCloseRequest, NativeWindowNavigationRequest, NativeWindowProxyUpdate,
    NativeWorkerMessage, NativeWorkerRegistry, append_storage_changes,
    apply_document_commands_with_font_face_ack, apply_indexed_db_changes, diff_indexed_db_changes,
    execute_dynamic_page_scripts, execute_inline_scripts, frame_event_batch,
    host_click_event_batch_with_modifiers, host_event_batch, host_event_batch_at,
    host_key_event_batch_with_modifiers, host_submit_event_batch, load_cookie_profile,
    load_indexed_db_profile, load_local_file_dynamic_module_graph_with_import_map,
    load_local_file_module_graph_with_import_map, load_service_worker_client_leases,
    load_web_storage_profile, new_storage_writer_id, read_storage_event_journal,
    register_storage_reader, resolve_module_request_url, save_web_storage_profile,
    storage_event_cursor, storage_key, unregister_service_worker_client_lease,
    unregister_storage_reader, validate_frame_script_command, validate_message_port_transfers,
    validate_native_message_payload, validate_native_object_url_transfers,
    validate_native_shared_worker_options, validate_page_message_port_command,
    validate_service_worker_client_states,
};
use super::layout::{NativeLayoutSnapshot, NativePoint, NativeRect, NativeScrollMetrics};
use super::lifecycle::NativeLifecycleState;
use super::module_import_map::NativeModuleImportMap;
use super::origin::NativeOrigin;
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
use super::resource_loader::{
    NativeCookieJar, NativeCspViolation, NativeFetchReferrerPolicy, NativeFetchResponse,
    NativeModuleResourceType, NativeNavigationMethod, NativeNavigationPolicyKind,
    NativeNavigationRequest, NativeObjectUrlTransfer, NativeResource, NativeResourceLoader,
    csp_sources_allow, csp_sources_allow_for_redirect, validate_target_navigation_payload,
};
use super::runtime::{NativeRuntimeState, NativeRuntimeTraceEvent};
use super::scheduler::{DeterministicScheduler, NativeTask};
use super::service_worker::native_service_worker_client_id;
use super::worker::{NativeRuntimeShared, NativeRuntimeWorker};
use crate::contracts::{
    Cookie, DownloadOutcome, GeoLocation, NetworkConditions, PendingDialog, PromptDecision,
    PromptResult, StorageOperation, StorageScope,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

mod actions;
mod document_navigation;
mod effects;
mod history_navigation;
mod local_events;
mod local_resources;
mod navigation;
mod navigation_commit;
mod script_commands;
mod script_execution;
mod storage;
use local_resources::*;

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

/// Lightweight current-page identity for waits and status checks that do not
/// need a semantic tree or layout snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePageMetadata {
    pub lifecycle: NativeLifecycleState,
    pub url: String,
    pub title: String,
    pub title_truncated: bool,
    pub revision: u64,
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

/// Results produced when an accepted child notification advances its exact
/// owning page through one script task turn.
#[derive(Debug)]
pub struct NativeAsyncEffectTurn {
    pub sequence: u64,
    pub context_id: String,
    pub frame_id: String,
    pub event_effects: Vec<NativeEffect>,
    pub frame_scripts: Vec<NativeFrameScriptRequest>,
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

/// Revision-bound visible overflow metadata from one native inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeScrollTargetSnapshot {
    pub reference: String,
    pub tag_name: String,
    pub id: Option<String>,
    pub label: Option<String>,
    pub scroll_offset: NativePoint,
    pub max_scroll_offset: NativePoint,
}

fn bounded_inspection_label(value: &str) -> String {
    let mut label = value.trim().chars().take(256).collect::<String>();
    if label.chars().any(char::is_control) {
        label.retain(|character| !character.is_control());
    }
    label
}

/// One atomic native inspection snapshot used by higher-level semantic
/// discovery surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeInspectionSnapshot {
    pub context_id: String,
    pub snapshot: NativeEngineSnapshot,
    pub nodes: Vec<super::dom::NativeSemanticNode>,
    pub layout: NativeLayoutSnapshot,
    pub scroll_targets: Vec<NativeScrollTargetSnapshot>,
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
    content_process_event_notify: Option<Arc<tokio::sync::Notify>>,
    pending_content_cookie_changes: Arc<Mutex<Vec<NativeCookieChange>>>,
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
    pending_external_cookie_changes: Vec<NativeCookieChange>,
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

    #[doc(hidden)]
    pub fn new_with_dialog_control(
        config: NativeEngineConfig,
        dialog_control: NativeDialogControlPlane,
    ) -> Result<Self, NativeEngineError> {
        Self::new_with_dialog_control_and_cookie_jar(config, dialog_control, None)
    }

    fn new_with_dialog_control_and_cookie_jar(
        config: NativeEngineConfig,
        dialog_control: NativeDialogControlPlane,
        shared_cookie_jar: Option<NativeCookieJar>,
    ) -> Result<Self, NativeEngineError> {
        config.validate()?;
        let web_storage = load_web_storage_profile(config.storage_path.as_deref())?;
        let indexed_db = load_indexed_db_profile(config.storage_path.as_deref())?;
        let storage_event_offset = storage_event_cursor(config.storage_path.as_deref())?;
        let storage_writer_id = new_storage_writer_id()?;
        let loader = if let Some(cookie_jar) = shared_cookie_jar {
            NativeResourceLoader::new_with_shared_cookie_jar(&config, cookie_jar)?
        } else {
            NativeResourceLoader::new(&config)?
        };
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
            content_process_event_notify: None,
            pending_content_cookie_changes: Arc::new(Mutex::new(Vec::new())),
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
            pending_external_cookie_changes: Vec::new(),
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

    pub fn new_with_browser_shared_workers(
        config: NativeEngineConfig,
        dialog_control: NativeDialogControlPlane,
    ) -> Result<Self, NativeEngineError> {
        let mut engine = Self::new_with_dialog_control(config, dialog_control)?;
        engine.external_shared_worker_routing = true;
        Ok(engine)
    }

    pub fn new_with_browser_shared_workers_and_cookie_jar(
        config: NativeEngineConfig,
        dialog_control: NativeDialogControlPlane,
        cookie_jar: NativeCookieJar,
    ) -> Result<Self, NativeEngineError> {
        let mut engine =
            Self::new_with_dialog_control_and_cookie_jar(config, dialog_control, Some(cookie_jar))?;
        engine.external_shared_worker_routing = true;
        Ok(engine)
    }

    pub fn dialog_control_plane(&self) -> NativeDialogControlPlane {
        self.dialog_control.clone()
    }

    pub fn config(&self) -> &NativeEngineConfig {
        &self.config
    }

    pub fn clone_resource_loader(&self) -> NativeResourceLoader {
        self.loader.clone()
    }

    pub fn shared_cookie_jar(&self) -> NativeCookieJar {
        self.loader.shared_cookie_jar()
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

    pub fn set_frame_id(&mut self, frame_id: String) {
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

    pub fn set_content_process_event_notify(&mut self, notify: Arc<tokio::sync::Notify>) {
        self.content_process_event_notify = Some(notify);
    }

    pub fn content_process_event_notify(&self) -> Option<Arc<tokio::sync::Notify>> {
        self.content_process_event_notify.clone()
    }

    /// Refresh the worker liveness before parent-owned integrations publish
    /// state to it. A caller-aborted IPC exchange retires its worker, while
    /// the committed document snapshot remains available for read-only
    /// inspection until a later operation starts a replacement worker.
    pub fn content_process_healthy(&mut self) -> bool {
        self.content_process
            .as_mut()
            .is_none_or(NativeContentProcess::refresh_health)
    }

    pub fn take_async_effect_notifications(
        &mut self,
    ) -> Result<Vec<NativeContentAsyncEffectNotification>, NativeEngineError> {
        // A content worker can be deliberately retired after an initial
        // navigation fails before it commits any document. The asynchronous
        // effect pump may still receive the child's terminal pipe error after
        // that navigation has already returned its typed failure. There is
        // no live document owner whose effects can be dispatched in this
        // state; drop that retired handle so a later navigation can start a
        // fresh worker instead of poisoning the whole browser backend.
        if self
            .content_process
            .as_mut()
            .is_some_and(|process| !process.is_healthy() && !process.has_current_document())
        {
            self.content_process.take();
            return Ok(Vec::new());
        }
        match self.content_process.as_mut() {
            Some(process) => process.take_async_effect_notifications(),
            None => Ok(Vec::new()),
        }
    }

    /// Deliver one validated child notification by advancing only this
    /// engine's owning realm. Backend routing must select the engine by the
    /// notification's context and frame identity before calling this method.
    pub async fn dispatch_async_effect_notification(
        &mut self,
        notification: &NativeContentAsyncEffectNotification,
    ) -> Result<NativeAsyncEffectTurn, NativeEngineError> {
        if notification.sequence == 0
            || notification.context_id != self.config.context_id
            || notification.frame_id != self.frame_id
        {
            return Err(NativeEngineError::worker_failure(
                "dispatch asynchronous content effects",
                NativeWorkerFailureKind::Protocol,
                "content effect notification does not identify this native context and frame",
            ));
        }

        let previous_revision = self.revision;
        self.evaluate_page_with_events_async_timeout(
            "globalThis.__glassRunTimers(performance.now());".into(),
            NativePageEventBatch::default(),
            CONTENT_PROCESS_ASYNC_EFFECT_TIMEOUT,
        )
        .await?;
        let event_effects = self.effects_since(previous_revision)?.effects;
        let frame_scripts = self.take_pending_frame_scripts();

        Ok(NativeAsyncEffectTurn {
            sequence: notification.sequence,
            context_id: notification.context_id.clone(),
            frame_id: notification.frame_id.clone(),
            event_effects,
            frame_scripts,
        })
    }

    pub fn service_worker_client_id(&self) -> String {
        native_service_worker_client_id(&self.config.context_id, &self.frame_id)
    }

    pub fn service_worker_client_state(
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

    pub fn service_worker_client_lease(
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

    pub async fn replace_service_worker_clients(
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

    pub async fn synchronize_service_worker_registrations(
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

    pub async fn resolve_service_worker_open_window_async(
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
            let loader = &mut self.loader;
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
            let result = Box::pin(
                process.resolve_service_worker_open_window(worker_id, request_id, &window, loader),
            )
            .await;
            self.request_ledger.finish();
            result?
        };
        self.config.window_name = window_name;
        self.queue_frame_script_requests(frame_scripts)?;
        self.queue_page_message_port_commands(page_message_port_commands)?;
        self.queue_shared_worker_create_commands(
            shared_worker_commands,
            self.origin.clone(),
            self.url.clone(),
        )?;
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
            Box::pin(self.resume_pending_service_worker_navigation()).await?;
        }
        Ok(())
    }

    pub async fn dispatch_service_worker_client_message_async(
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

    pub fn service_worker_clients(&self) -> Vec<NativeServiceWorkerClientState> {
        self.service_worker_clients.clone()
    }

    pub fn inherit_service_worker_clients(&mut self, clients: &[NativeServiceWorkerClientState]) {
        self.service_worker_clients = clients.to_vec();
    }

    pub fn set_frame_script_bindings(&mut self, bindings: Vec<NativeFrameScriptBinding>) {
        self.frame_script_bindings = bindings.clone();
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_frame_script_bindings(bindings);
        }
    }

    pub fn set_frame_script_context(&mut self, context: Option<NativeFrameScriptContext>) {
        self.frame_script_context = context.clone();
        if let Some(javascript) = self.javascript.as_ref() {
            javascript.set_frame_script_context(context);
        }
    }

    pub fn script_document_snapshot(
        &self,
    ) -> Result<NativeScriptDocumentSnapshot, NativeEngineError> {
        self.require_running("frame script projection")?;
        self.document.script_snapshot_with_layout(
            crate::limits::MAX_TEXT_BYTES,
            self.config.viewport,
            self.scroll_offset,
            &self.nested_scroll_offsets,
        )
    }

    pub fn document_generation(&self) -> Result<u32, NativeEngineError> {
        self.require_running("frame discovery")?;
        Ok(self.document.generation())
    }

    pub fn document_revision(&self) -> Result<u64, NativeEngineError> {
        self.require_running("frame discovery")?;
        Ok(self.document.revision())
    }

    pub fn embedded_frame_owner_removal_sequences(
        &self,
    ) -> Result<Vec<(u32, u64)>, NativeEngineError> {
        self.require_running("frame discovery")?;
        Ok(self.document.embedded_frame_owner_removal_sequences())
    }

    pub fn frame_owner_id(&self) -> &str {
        &self.frame_id
    }

    pub fn document_transfer_draft(&self) -> NativeDocument {
        self.document.clone()
    }

    pub fn validate_document_transfer_candidate(
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

    pub async fn synchronize_document_transfer_snapshot(
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

    pub fn publish_document_transfer_snapshot(&mut self, candidate: NativeDocument) {
        self.revision = candidate.revision();
        self.document = candidate;
    }

    pub fn navigation_requires_document_lifecycle(&self, target_url: &str) -> bool {
        !self.is_same_document_navigation(target_url)
    }

    pub fn embedded_frame_sources(
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

    pub fn resolve_embedded_frame_url(&self, source: &str) -> Result<String, NativeEngineError> {
        self.require_running("frame discovery")?;
        self.resolve_link_href(source)
    }

    pub fn allows_embedded_frame_url(&self, target_url: &str) -> Result<bool, NativeEngineError> {
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

    pub fn frame_navigation_policy(&self) -> (String, Option<Vec<Vec<String>>>) {
        (self.url.clone(), self.document_frame_sources.clone())
    }

    pub fn set_embedding_frame_policy(
        &mut self,
        document_url: String,
        frame_sources: Option<Vec<Vec<String>>>,
    ) {
        self.embedding_document_url = Some(document_url);
        self.embedding_frame_sources = frame_sources;
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

    /// Read current URL/title state without computing visible text, styles, or
    /// layout. This keeps simple page-identity waits proportional to the title
    /// node rather than the document's full presentation tree.
    pub fn page_metadata(&self) -> Result<NativePageMetadata, NativeEngineError> {
        self.require_running("page metadata")?;
        let (title, title_truncated) = self.document.title(self.config.limits.max_text_bytes);
        Ok(NativePageMetadata {
            lifecycle: self.lifecycle,
            url: self.url.clone(),
            title,
            title_truncated,
            revision: self.revision,
        })
    }

    /// Capture the current native page, semantic nodes, and layout under one
    /// revision so agent-facing discovery cannot combine different states.
    pub fn inspection_snapshot(&self) -> Result<NativeInspectionSnapshot, NativeEngineError> {
        self.require_running("semantic inspection")?;
        let layout = self.layout()?;
        let scroll_targets = layout
            .observed_scroll_targets()
            .into_iter()
            .filter_map(|metrics: NativeScrollMetrics| {
                let node = self.document.node(metrics.node_id)?;
                let tag_name = node.element_name()?.to_owned();
                let reference = self.document.node_reference(metrics.node_id)?;
                let id = node.attribute("id").map(bounded_inspection_label);
                let label = node
                    .attribute("aria-label")
                    .or_else(|| node.attribute("title"))
                    .map(bounded_inspection_label);
                Some(NativeScrollTargetSnapshot {
                    reference,
                    tag_name,
                    id,
                    label,
                    scroll_offset: metrics.scroll_offset,
                    max_scroll_offset: metrics.max_scroll_offset,
                })
            })
            .collect();
        Ok(NativeInspectionSnapshot {
            context_id: self.config.context_id.clone(),
            snapshot: self.snapshot()?,
            nodes: self.document.semantic_nodes(),
            scroll_targets,
            layout,
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

    pub fn resolve_target(&self, locator: &str) -> Result<NativeNodeId, NativeEngineError> {
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
    pub fn layout_at_viewport(
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

    pub fn is_descendant_or_self(
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
    pub fn rasterize_at_viewport(
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
    ) -> Result<DocumentCommandEffects, NativeEngineError> {
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

    fn apply_scroll_target(
        &mut self,
        target: NativeNodeId,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<bool, NativeEngineError> {
        if delta_x == 0 && delta_y == 0 {
            return Ok(false);
        }
        let node_name = self
            .document
            .node(target)
            .and_then(|node| node.element_name())
            .ok_or(NativeEngineError::TargetNotFound)?;
        if matches!(node_name, "html" | "body") {
            return self.apply_scroll(delta_x, delta_y);
        }
        let layout = self.document.layout(self.config.viewport)?;
        let container = layout.scroll_container_for(target).ok_or_else(|| {
            NativeEngineError::TargetNotActionable {
                reason: "selected element is not a native scroll container".into(),
            }
        })?;
        let maximum = container.max_scroll_offset();
        let current = self
            .nested_scroll_offsets
            .get(&target.index())
            .copied()
            .unwrap_or(NativePoint { x: 0, y: 0 });
        let next = NativePoint {
            x: u32::try_from(
                (i64::from(current.x) + i64::from(delta_x)).clamp(0, i64::from(maximum.x)),
            )
            .map_err(|_| {
                NativeEngineError::invalid(
                    "native targeted scroll action",
                    "horizontal scroll offset exceeds bounds",
                )
            })?,
            y: u32::try_from(
                (i64::from(current.y) + i64::from(delta_y)).clamp(0, i64::from(maximum.y)),
            )
            .map_err(|_| {
                NativeEngineError::invalid(
                    "native targeted scroll action",
                    "vertical scroll offset exceeds bounds",
                )
            })?,
        };
        if next == current {
            return Ok(false);
        }
        if next.x == 0 && next.y == 0 {
            self.nested_scroll_offsets.remove(&target.index());
        } else {
            self.nested_scroll_offsets.insert(target.index(), next);
        }
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

pub fn parse_point_target(target: &str) -> Result<Option<(i64, i64)>, NativeEngineError> {
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
#[path = "engine/tests.rs"]
mod tests;

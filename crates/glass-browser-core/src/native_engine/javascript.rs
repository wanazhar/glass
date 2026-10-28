//! Bounded JavaScript execution for the native browser realm.
//!
//! QuickJS supplies the ECMAScript implementation. Glass owns the host
//! objects and Web APIs, which are added in separate slices so every exposed
//! capability has an explicit resource and security contract.

#[path = "javascript/host_bindings.rs"]
mod host_bindings;
mod protocol;
use host_bindings::*;
pub(crate) use host_bindings::{NativeSyncXhrParentBroker, install_native_sync_xhr_parent_broker};
pub use protocol::NativeScriptCommand;
#[path = "javascript/document_bootstrap.rs"]
mod document_bootstrap;
use document_bootstrap::*;
#[path = "javascript/worker_bootstrap.rs"]
mod worker_bootstrap;
use worker_bootstrap::*;
#[path = "javascript/profile_storage.rs"]
mod profile_storage;
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NativeServiceWorkerClientLeaseFile {
    version: u64,
    clients: Vec<NativeServiceWorkerClientLease>,
}

#[path = "javascript/bootstrap_scripts.rs"]
mod bootstrap_scripts;
use bootstrap_scripts::*;
#[path = "javascript/service_worker_profiles.rs"]
mod service_worker_profiles;
pub(crate) use service_worker_profiles::{
    NativeServiceWorkerRegistrationProfile, NativeServiceWorkerWorkerProfile,
    validate_service_worker_client_states,
};
#[path = "javascript/scripts.rs"]
mod scripts;
pub(super) use profile_storage::*;
pub use profile_storage::{NativeCookieChange, synchronize_service_worker_client_leases};
pub use profile_storage::{NativeCookieProfileEntry, NativeServiceWorkerCacheBatchEntry};
use scripts::*;

pub(crate) type DocumentCommandEffects = (
    Vec<(NativeNodeId, NativeEventKind)>,
    Vec<NativeScriptCommand>,
);
pub(crate) type ModuleSourcesSnapshot = (BTreeMap<String, String>, BTreeMap<String, String>);

use super::config::{
    MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_NODES, MAX_NATIVE_WINDOW_NAME_BYTES, Viewport, is_file_url,
    is_network_url, validate_context_id, validate_url_text, validate_window_name, without_fragment,
};
use super::content_process::{
    NativeContentFetchBroker, is_parent_owned_network_target, missing_parent_network_authority,
};
use super::css::{
    FontStyleValue, FontWeightValue, parse_font_stretch_range, parse_font_weight_range,
};
use super::dialog::NativeDialogResolution;
use super::dom::{
    NativeDocument, NativeNodeId, NativeNodeSubtreeTransfer, NativePageImportMapSource,
    NativePageScriptSource, NativePageScriptTiming, NativeScriptDocumentSnapshot,
    NativeScriptElementSnapshot,
};
use super::environment::NativeEnvironmentOverrides;
use super::error::{NativeEngineError, NativeWorkerFailureKind};
use super::fetch_stream::{
    MAX_NATIVE_FETCH_UPLOAD_CHUNKS, NativeFetchStreamCommand, NativeFetchStreamConnection,
    NativeFetchStreamEvent, NativeFetchUploadCommand, NativeFetchUploadConnection,
    NativeFetchUploadEvent, spawn_native_fetch_bytes_stream, spawn_native_fetch_stream,
    spawn_native_fetch_upload_source,
};
use super::font::{MAX_NATIVE_FONT_BYTES, NativeFontBook};
use super::html_parser::{
    HtmlParsedAttribute, HtmlTreeSinkFailure,
    parse_document_with_limits_and_scripting as parse_html5_document,
    parse_fragment_with_limits as parse_html5_fragment,
};
use super::interaction::{
    MAX_NATIVE_EFFECTS, MAX_NATIVE_FILE_BYTES, MAX_NATIVE_FORM_BODY_BYTES,
    MAX_NATIVE_SCRIPT_COMMAND_BYTES, NativeEventKind, validate_native_key,
};
use super::layout::NativePoint;
use super::module_import_map::NativeModuleImportMap;
use super::origin::NativeOrigin;
use super::resource_loader::{
    JAVASCRIPT_MIME_TYPE_ESSENCES, MAX_NATIVE_CSP_VIOLATIONS, NativeCorsMode, NativeCspViolation,
    NativeFetchCacheMode, NativeFetchCredentialsMode, NativeFetchMethod, NativeFetchRedirectMode,
    NativeFetchReferrerPolicy, NativeFetchRequest, NativeFetchResponse, NativeFetchResponseStream,
    NativeInlineScriptPolicy, NativeModuleResourceType, NativeNavigationMethod,
    NativeObjectUrlResource, NativeObjectUrlTransfer, NativeRequestBody, NativeResourceLoader,
    NativeScriptLoadRequest, NativeScriptResource, NativeSubresourceKind,
};
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit as BlockKeyInit};
use aes::{Aes128, Aes192, Aes256};
use aes_gcm::aead::consts::U12;
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{AesGcm, Nonce};
use base64::Engine as _;
use ed25519_dalek::{
    Signature as Ed25519Signature, Signer as Ed25519Signer, SigningKey,
    Verifier as Ed25519Verifier, VerifyingKey,
};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use rquickjs::function::This;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{
    ArrayBuffer, CaughtError, Coerced, Context, Ctx, Error, FromJs, Function, Module, Persistent,
    Runtime, TypedArray, Value,
};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

fn default_true() -> bool {
    true
}

fn default_classic_worker_type() -> String {
    "classic".into()
}

fn default_same_origin_credentials() -> String {
    "same-origin".into()
}

fn skip_if_false(value: &bool) -> bool {
    !*value
}

pub(crate) fn default_navigation_preload_header_value() -> String {
    "true".into()
}

pub(crate) fn validate_navigation_preload_header_value(
    value: &str,
) -> Result<(), NativeEngineError> {
    let byte_len = value.chars().count();
    if byte_len > MAX_NATIVE_FETCH_HEADER_VALUE_BYTES {
        return Err(NativeEngineError::limit(
            "native ServiceWorker navigation preload header value",
            MAX_NATIVE_FETCH_HEADER_VALUE_BYTES,
            byte_len,
        ));
    }
    if value
        .chars()
        .any(|character| u32::from(character) > u32::from(u8::MAX))
    {
        return Err(NativeEngineError::invalid(
            "native ServiceWorker navigation preload header value",
            "must be a ByteString",
        ));
    }
    if matches!(value.chars().next(), Some('\t' | ' '))
        || matches!(value.chars().next_back(), Some('\t' | ' '))
    {
        return Err(NativeEngineError::invalid(
            "native ServiceWorker navigation preload header value",
            "must already be Fetch-normalized",
        ));
    }
    if value
        .chars()
        .any(|character| matches!(character, '\0' | '\r' | '\n'))
    {
        return Err(NativeEngineError::invalid(
            "native ServiceWorker navigation preload header value",
            "must not contain NUL or HTTP newline bytes",
        ));
    }
    Ok(())
}

/// Maximum source accepted by the native script evaluator.
pub(crate) const MAX_NATIVE_SCRIPT_BYTES: usize = crate::limits::MAX_TEXT_BYTES;
/// Maximum JSON representation returned to the semantic backend.
pub(crate) const MAX_NATIVE_SCRIPT_RESULT_BYTES: usize = crate::limits::MAX_JSON_BYTES;
/// Maximum structured-clone payload accepted by the native `postMessage`
/// bridge. The transport is JSON-framed, while the JavaScript payload uses a
/// tagged graph so cloneable values and cycles survive the IPC boundary.
pub(crate) const MAX_NATIVE_POST_MESSAGE_BYTES: usize = 256 * 1024;
/// Maximum number of realm-owned Blob URL snapshots carried by one message.
pub(crate) const MAX_NATIVE_OBJECT_URL_TRANSFERS: usize = 16;
/// Maximum JSON-backed state retained by one History API entry.
pub(crate) const MAX_NATIVE_HISTORY_STATE_BYTES: usize = 256 * 1024;
const MAX_NATIVE_HISTORY_DELTA: i32 = 1024;
/// Maximum frame-window index surface exposed by one script realm.
pub(crate) const MAX_NATIVE_FRAME_SCRIPT_BINDINGS: usize = 64;
/// Maximum inline page scripts executed while committing one document.
pub(crate) const MAX_NATIVE_INLINE_SCRIPTS: usize = 32;
pub(crate) const MAX_NATIVE_MODULE_IMPORTS: usize = 128;
pub(crate) const MAX_NATIVE_WORKERS: usize = 32;
pub const MAX_NATIVE_WORKER_MESSAGES: usize = 64;
const MAX_NATIVE_WORKER_SCRIPT_ERROR_BYTES: usize = 16 * 1024;
const MAX_NATIVE_WORKER_FETCH_STREAMS: usize = MAX_NATIVE_WORKER_MESSAGES;
const MAX_NATIVE_WORKER_FETCH_UPLOADS: usize = MAX_NATIVE_WORKER_MESSAGES;
pub(crate) const MAX_NATIVE_WORKER_TIMERS: usize = 64;
pub(crate) const MAX_NATIVE_SERVICE_WORKERS: usize = 16;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES: usize = MAX_NATIVE_SCRIPT_BYTES;
pub(crate) const MAX_NATIVE_SHARED_WORKER_NAME_BYTES: usize = crate::limits::MAX_BACKEND_ID_BYTES;
const MAX_NATIVE_WORKER_URLSEARCHPARAMS_ENTRIES: usize = 128;
const MAX_NATIVE_WORKER_URLSEARCHPARAMS_BYTES: usize = MAX_NATIVE_POST_MESSAGE_BYTES;
const MAX_NATIVE_WORKER_CRYPTO_POOL_BYTES: usize = 16 * 1024;
const MAX_NATIVE_WORKER_CRYPTO_VALUES_BYTES: usize = 65_536;
const MAX_NATIVE_PAGE_CRYPTO_POOL_BYTES: usize = 16 * 1024;
const MAX_NATIVE_PAGE_CRYPTO_VALUES_BYTES: usize = 65_536;
const MAX_NATIVE_CRYPTO_DERIVE_ITERATIONS: usize = 100_000;
const MAX_NATIVE_CRYPTO_DERIVE_WORK: usize = 10_000_000;
// Page script realms retain the current DOM projection while host views are
// refreshed between operations. Keep this bounded, with enough room for a
// multi-thousand-node host snapshot and a populated frame tree in one realm.
const NATIVE_SCRIPT_MEMORY_BYTES: usize = 256 * 1024 * 1024;
const NATIVE_SCRIPT_STACK_BYTES: usize = 1024 * 1024;
const NATIVE_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);
const NATIVE_HOST_VIEW_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_WEB_STORAGE_PROFILE_BYTES: usize = 4 * 1024 * 1024;
const WEB_STORAGE_PROFILE_VERSION: u64 = 1;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_CACHE_ORIGINS: usize = 64;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_CACHES: usize = 32;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES: usize = 256;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES: usize = 256;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES: usize = MAX_NATIVE_SCRIPT_BYTES;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES: usize = MAX_NATIVE_FORM_BODY_BYTES;
pub(crate) const MAX_NATIVE_SERVICE_WORKER_CACHE_HEADERS: usize = 64;
const MAX_NATIVE_SERVICE_WORKER_CACHE_TOTAL_BYTES: usize = 3 * 1024 * 1024;
const MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES: usize = 4 * 1024 * 1024;
const MAX_NATIVE_STORAGE_EVENTS: usize = 64;
const MAX_NATIVE_STORAGE_READER_LEASES: usize = 128;
const MAX_NATIVE_STORAGE_READER_LEASE_BYTES: usize = 64 * 1024;
const NATIVE_STORAGE_READER_LEASE_VERSION: u64 = 1;
const NATIVE_STORAGE_READER_LEASE_TTL: Duration = Duration::from_secs(15 * 60);
const NATIVE_STORAGE_READER_HEARTBEAT: Duration = Duration::from_secs(30);
const MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES: usize =
    crate::limits::TOPOLOGY_MAX_TARGETS * crate::limits::TOPOLOGY_MAX_FRAMES;
const MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASE_BYTES: usize = 128 * 1024;
const NATIVE_SERVICE_WORKER_CLIENT_LEASE_VERSION: u64 = 1;
const NATIVE_SERVICE_WORKER_CLIENT_LEASE_TTL: Duration = Duration::from_secs(15 * 60);
const NATIVE_SERVICE_WORKER_CLIENT_HEARTBEAT: Duration = Duration::from_secs(30);
pub const MAX_NATIVE_COOKIE_PROFILE_ENTRIES: usize = 128;
pub(crate) const MAX_NATIVE_COOKIE_PROFILE_BYTES: usize = 4096;
pub(crate) const MAX_NATIVE_COOKIE_CHANGE_BATCH: usize = MAX_NATIVE_COOKIE_PROFILE_ENTRIES * 2;
pub(crate) const MAX_NATIVE_DIALOGS: usize = 32;
pub const MAX_NATIVE_DIALOG_TEXT_BYTES: usize = 256;
const MAX_NATIVE_INDEXED_DB_DATABASES: usize = 16;
const MAX_NATIVE_INDEXED_DB_STORES: usize = 128;
const MAX_NATIVE_INDEXED_DB_INDEXES: usize = 128;
const MAX_NATIVE_INDEXED_DB_RECORDS: usize = 128;
pub(crate) const MAX_NATIVE_INDEXED_DB_CHANGES: usize = 128;
const MAX_NATIVE_INDEXED_DB_VALUE_BYTES: usize = 8 * 1024;
const MAX_NATIVE_INDEXED_DB_STATE_BYTES: usize = MAX_NATIVE_SCRIPT_RESULT_BYTES;
const NATIVE_STORAGE_PROFILE_LOCK_TIMEOUT: Duration = Duration::from_millis(500);
const NATIVE_STORAGE_PROFILE_LOCK_RETRY: Duration = Duration::from_millis(10);
const MAX_NATIVE_FETCH_HEADERS: usize = 16;
const MAX_NATIVE_FETCH_HEADER_NAME_BYTES: usize = 128;
const MAX_NATIVE_FETCH_HEADER_VALUE_BYTES: usize = 64 * 1024;
const MAX_NATIVE_FETCH_HEADER_BYTES: usize = 128 * 1024;
pub(crate) const MAX_NATIVE_XHR_TIMEOUT_MS: u32 = 4_000;
pub const MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES: usize = MAX_NATIVE_FORM_BODY_BYTES;
pub(crate) const MAX_NATIVE_WEBSOCKET_PROTOCOLS: usize = 16;
pub(crate) const MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES: usize = 128;
pub const MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES: usize = 123;
pub(crate) const MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES: usize = MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES;
pub(crate) const MAX_NATIVE_EVENTSOURCE_FIELD_BYTES: usize = 128;
pub(crate) const MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES: usize = 8 * 1024;
pub(crate) const MAX_NATIVE_FETCH_STREAM_BODY_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_NATIVE_FETCH_STREAM_QUEUED_CHUNKS: usize = 32;
const MAX_NATIVE_UNHANDLED_REJECTIONS: usize = 64;
const MAX_NATIVE_UNHANDLED_REJECTION_REASON_CHARS: usize = 4096;
const NATIVE_HOST_EVENT_RESULT_SOURCE: &str = "globalThis.__glassLastHostEventResults || []";

/// A bounded, owner-issued identity for one transferred MessagePort endpoint.
/// The source realm contributes `port_id`; the receiving endpoint remains
/// addressable through `peer_id` inside that source realm. The content process
/// owns the bridge key and never accepts a JavaScript object or raw pointer
/// across the realm boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeMessagePortTransfer {
    pub bridge_key: String,
    pub port_id: u32,
    pub peer_id: u32,
    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct NativeServiceWorkerWorkerState {
    pub(crate) script_url: String,
    pub(crate) state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct NativeServiceWorkerRegistrationState {
    pub(crate) script_url: String,
    pub(crate) scope: String,
    pub(crate) state: String,
    /// Whether the current top-level document is controlled by this
    /// registration. A matching scope alone does not control a page that has
    /// just registered a worker.
    #[serde(default)]
    pub(crate) controlled: bool,
    /// Lifecycle transitions that the page resolver replays for a newly
    /// installed or updated worker. Restored/navigation snapshots leave this
    /// empty because those transitions already happened in an earlier owner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) lifecycle: Vec<String>,
    /// Explicit active/waiting projections let a waiting update coexist with
    /// the prior active worker without overloading the top-level state fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) active: Option<NativeServiceWorkerWorkerState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) waiting: Option<NativeServiceWorkerWorkerState>,
}

/// A bounded browser-wide projection of one live window or frame client.
///
/// The content worker receives this projection from the semantic backend so a
/// Service Worker realm can answer `clients.matchAll()` without reaching into
/// another content-process address space. Control status is calculated by the
/// owning Service Worker registry because it depends on the registration and
/// on whether the current client has completed a controlling navigation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeServiceWorkerClientState {
    pub id: String,
    pub url: String,
    #[serde(rename = "type")]
    pub client_type: String,
    pub frame_type: String,
    pub visibility_state: String,
    pub focused: bool,
}

/// Durable ownership for one live Service Worker window or frame client.
/// Client identity remains stable across content-process replacement, while
/// the owner token prevents an old process from removing a newer lease during
/// shutdown after a recovery or context replacement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeServiceWorkerClientLease {
    #[serde(flatten)]
    pub(crate) state: NativeServiceWorkerClientState,
    pub(crate) owner_id: String,
    pub(crate) heartbeat_unix_seconds: u64,
}

pub(crate) struct NativeScriptEvaluation {
    pub(crate) value: serde_json::Value,
    pub(crate) commands: Vec<NativeScriptCommand>,
    pub(crate) top_level_await_pending: bool,
    pub(crate) worker_script_error: Option<String>,
}

#[derive(Deserialize)]
struct NativeWorkerScriptErrorReport {
    handled: bool,
    message: String,
}

enum NativeClassicWorkerScriptExecution<'js> {
    Completed,
    RuntimeError(Value<'js>),
}

fn retain_top_level_await_promise<'js>(
    ctx: &Ctx<'js>,
    promise: &rquickjs::Promise<'js>,
) -> Result<(), NativeEngineError> {
    ctx.eval::<(), _>("globalThis.__glassTopLevelAwaitState = { state: 'pending' };")
        .map_err(|error| NativeEngineError::Worker {
            operation: "prepare native Worker module promise".into(),
            reason: format!(
                "native Worker module promise could not be prepared: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    let on_fulfilled: Function = ctx
        .eval("value => { globalThis.__glassTopLevelAwaitState = { state: 'fulfilled', value: value === undefined ? null : value }; }")
        .map_err(|error| NativeEngineError::Worker {
            operation: "prepare native Worker module promise".into(),
            reason: format!(
                "native Worker module promise callback could not be created: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    let on_rejected: Function = ctx
        .eval("error => { globalThis.__glassTopLevelAwaitState = { state: 'rejected', error: String(error) }; }")
        .map_err(|error| NativeEngineError::Worker {
            operation: "prepare native Worker module promise".into(),
            reason: format!(
                "native Worker module rejection callback could not be created: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    promise
        .then()
        .and_then(|then| then.call::<_, ()>((This(promise.clone()), on_fulfilled, on_rejected)))
        .map_err(|error| NativeEngineError::Worker {
            operation: "prepare native Worker module promise".into(),
            reason: format!(
                "native Worker module promise callback could not be attached: {}",
                CaughtError::from_error(ctx, error)
            ),
        })
}

fn execute_classic_worker_script<'js>(
    ctx: Ctx<'js>,
    source: &str,
    worker_url: &str,
) -> Result<NativeClassicWorkerScriptExecution<'js>, NativeEngineError> {
    let filename = CString::new(worker_url).map_err(|_| NativeEngineError::Worker {
        operation: "compile native Worker script".into(),
        reason: "native Worker URL contained a null byte".into(),
    })?;
    let raw_context = ctx.as_raw().as_ptr();
    let source = CString::new(source).map_err(|_| NativeEngineError::Worker {
        operation: "compile native Worker script".into(),
        reason: "native Worker script source contained a null byte".into(),
    })?;
    let source_len = rquickjs::qjs::size_t::try_from(source.as_bytes().len()).map_err(|_| {
        NativeEngineError::limit(
            "native Worker script source",
            MAX_NATIVE_SCRIPT_BYTES,
            source.as_bytes().len(),
        )
    })?;
    // SAFETY: the context is live for the duration of the call, both input
    // buffers remain valid, and the source is NUL-terminated just as it is in
    // rquickjs-core's eval_raw wrapper. The explicit source length excludes
    // that terminator and bounds the read.
    // Compile-only returns an owned bytecode value for JS_EvalFunction.
    let bytecode = unsafe {
        rquickjs::qjs::JS_Eval(
            raw_context,
            source.as_ptr(),
            source_len,
            filename.as_ptr(),
            (rquickjs::qjs::JS_EVAL_TYPE_GLOBAL | rquickjs::qjs::JS_EVAL_FLAG_COMPILE_ONLY) as i32,
        )
    };
    if bytecode.tag == rquickjs::qjs::JS_TAG_EXCEPTION as i64 {
        let reason = CaughtError::from_error(&ctx, Error::Exception);
        return Err(NativeEngineError::Worker {
            operation: "compile native Worker script".into(),
            reason: format!("native Worker script compilation failed: {reason}"),
        });
    }
    // SAFETY: bytecode is the owned global-script result produced above, and
    // JS_EvalFunction consumes it exactly once. The context remains live.
    let result = unsafe { rquickjs::qjs::JS_EvalFunction(raw_context, bytecode) };
    if result.tag == rquickjs::qjs::JS_TAG_EXCEPTION as i64 {
        return Ok(NativeClassicWorkerScriptExecution::RuntimeError(
            ctx.catch(),
        ));
    }
    // SAFETY: successful JS_EvalFunction returns an owned JS value associated
    // with this live context; the worker script's completion value is ignored.
    unsafe { rquickjs::qjs::JS_FreeValue(raw_context, result) };
    Ok(NativeClassicWorkerScriptExecution::Completed)
}

fn report_worker_script_exception<'js>(
    ctx: &Ctx<'js>,
    exception: Value<'js>,
) -> Result<Option<String>, NativeEngineError> {
    let reporter: Function = ctx
        .globals()
        .get("__glassReportWorkerScriptError")
        .map_err(|error| NativeEngineError::Worker {
            operation: "report native Worker script error".into(),
            reason: format!(
                "native Worker error reporter was unavailable: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    let report_json: String =
        reporter
            .call((exception,))
            .map_err(|error| NativeEngineError::Worker {
                operation: "report native Worker script error".into(),
                reason: format!(
                    "native Worker error event could not be dispatched: {}",
                    CaughtError::from_error(ctx, error)
                ),
            })?;
    let report: NativeWorkerScriptErrorReport =
        serde_json::from_str(&report_json).map_err(|_| NativeEngineError::Worker {
            operation: "report native Worker script error".into(),
            reason: "native Worker error reporter returned invalid data".into(),
        })?;
    Ok((!report.handled).then_some(report.message))
}

/// A same-origin DOM operation emitted by one page realm for a different
/// embedded browsing context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFrameScriptRequest {
    pub frame_id: String,
    pub source_frame_id: String,
    pub command: Box<NativeScriptCommand>,
}

/// A bounded message emitted by a dedicated page worker for its owner page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeWorkerMessage {
    pub worker_id: u32,
    pub(crate) data: serde_json::Value,
    #[serde(default)]
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) transfer_ports: Vec<NativeMessagePortTransfer>,
    #[serde(default)]
    pub(crate) object_urls: Vec<NativeObjectUrlTransfer>,
}

/// A MessagePort event waiting for delivery to the page realm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeMessagePortPageMessage {
    pub bridge_key: String,
    pub data: serde_json::Value,
    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub close: bool,
    #[serde(default)]
    pub transfer_ports: Vec<NativeMessagePortTransfer>,
    #[serde(default)]
    pub object_urls: Vec<NativeObjectUrlTransfer>,
}

/// A page-owned MessagePort command whose bridge is outside the current
/// JavaScript owner. The browser backend fills the source metadata after the
/// record leaves the page or content-process owner; keeping it out of the
/// serialized payload makes the IPC format independent of host topology.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativePageMessagePortCommand {
    pub bridge_key: String,
    pub data: serde_json::Value,
    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub close: bool,
    #[serde(default)]
    pub transfer_ports: Vec<NativeMessagePortTransfer>,
    #[serde(default)]
    pub object_urls: Vec<NativeObjectUrlTransfer>,
    #[serde(default, skip_serializing)]
    pub source_context_id: String,
    #[serde(default, skip_serializing)]
    pub source_frame_id: String,
}

/// A SharedWorker connection created by a page whose content runtime is
/// coordinated at browser-session scope rather than inside its isolated
/// content process.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeSharedWorkerCreateRequest {
    pub page_worker_id: u32,
    pub source_context_id: String,
    pub source_frame_id: String,
    pub document_generation: u32,
    pub constructor_origin: NativeOrigin,
    pub owner_url: String,
    pub href: String,
    pub name: String,
    pub worker_type: String,
    pub credentials: String,
    pub extended_lifetime: bool,
    pub referrer_policy: Option<String>,
    pub transfer_port: NativeMessagePortTransfer,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NativeSharedWorkerStorageKey {
    TupleOrigin(String),
    OpaqueDocument {
        context_id: String,
        frame_id: String,
        document_generation: u32,
    },
}

impl NativeSharedWorkerStorageKey {
    pub fn for_document(
        origin: &NativeOrigin,
        context_id: &str,
        frame_id: &str,
        document_generation: u32,
    ) -> Self {
        match origin {
            NativeOrigin::Opaque => Self::OpaqueDocument {
                context_id: context_id.to_owned(),
                frame_id: frame_id.to_owned(),
                document_generation,
            },
            NativeOrigin::Tuple { .. } => Self::TupleOrigin(origin.serialized()),
        }
    }
}

#[cfg(test)]
#[path = "javascript/tests/native_shared_worker_command_protocol_tests.rs"]
mod native_shared_worker_command_protocol_tests;
#[cfg(test)]
#[path = "javascript/tests/native_shared_worker_storage_key_tests.rs"]
mod native_shared_worker_storage_key_tests;
/// A bounded message emitted by a Service Worker for one browser-wide page
/// client. The browser backend resolves the opaque client ID to its target and
/// frame owner before dispatching the page event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeServiceWorkerClientMessage {
    pub worker_id: u32,
    pub client_id: String,
    pub(crate) data: serde_json::Value,
    #[serde(default)]
    pub(crate) transfer_ports: Vec<NativeMessagePortTransfer>,
    #[serde(default)]
    pub(crate) object_urls: Vec<NativeObjectUrlTransfer>,
}

/// A bounded cross-window message waiting for delivery to a page realm.
///
/// Unlike `NativePostMessageRequest`, this is the browser-owned delivery
/// record rather than a page command. Keeping the source metadata here means
/// a large structured-clone payload can cross the host boundary without being
/// interpolated into generated JavaScript source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativePageMessageEvent {
    pub(crate) source_context_id: String,
    pub(crate) source_origin: String,
    pub(crate) data: serde_json::Value,
    #[serde(default)]
    pub(crate) transfer_ports: Vec<NativeMessagePortTransfer>,
    /// Blob URL snapshots captured in the source realm and installed before
    /// structured-clone decoding in the destination realm.
    #[serde(default)]
    pub(crate) object_urls: Vec<NativeObjectUrlTransfer>,
}

/// A browser-owned page event delivered through the installed host dispatcher.
/// Event metadata remains structured until it enters the page realm, so keys,
/// URLs, and other host values cannot become executable JavaScript source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeHostEvent {
    pub(crate) node_index: u32,
    #[serde(rename = "type")]
    pub(crate) event_type: String,
    pub(crate) bubbles: bool,
    pub(crate) cancelable: bool,
    #[serde(default)]
    pub(crate) persisted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) code: Option<String>,
    #[serde(default)]
    pub(crate) alt_key: bool,
    #[serde(default)]
    pub(crate) ctrl_key: bool,
    #[serde(default)]
    pub(crate) meta_key: bool,
    #[serde(default)]
    pub(crate) shift_key: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) submitter_node_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) related_target_node_index: Option<u32>,
}

/// A bounded event observed by a child browsing context and delivered to its
/// same-origin parent projection. Event metadata crosses the owner boundary
/// as data so frame IDs and node generations never become JavaScript source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeFrameEvent {
    pub(crate) node_index: u32,
    pub(crate) generation: u32,
    #[serde(rename = "type")]
    pub(crate) event_type: String,
    pub(crate) bubbles: bool,
    pub(crate) cancelable: bool,
    pub(crate) persisted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) related_target_node_index: Option<u32>,
}

/// A bounded batch of frame events for one same-origin parent projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeFrameEventBatch {
    pub(crate) frame_id: String,
    pub(crate) events: Vec<NativeFrameEvent>,
}

/// Hash-change metadata delivered to the page owner without embedding URLs in
/// generated JavaScript source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeHashChangeEvent {
    pub(crate) old_url: String,
    pub(crate) new_url: String,
}

/// Page-facing events admitted for one JavaScript host turn. The payloads stay
/// structured until they enter the already-installed QuickJS dispatch
/// functions; concatenating their JSON into the user script would make a
/// valid postMessage payload consume the script-source budget.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativePageEventBatch {
    #[serde(default)]
    pub(crate) worker_messages: Vec<NativeWorkerMessage>,
    #[serde(default)]
    pub(crate) message_port_messages: Vec<NativeMessagePortPageMessage>,
    #[serde(default)]
    pub(crate) service_worker_client_messages: Vec<NativeServiceWorkerClientMessage>,
    #[serde(default)]
    pub(crate) post_message_events: Vec<NativePageMessageEvent>,
    #[serde(default)]
    pub(crate) frame_event_batches: Vec<NativeFrameEventBatch>,
    #[serde(default)]
    pub(crate) frame_script_commands: Vec<NativeScriptCommand>,
    #[serde(default)]
    pub(crate) hash_change_events: Vec<NativeHashChangeEvent>,
    #[serde(default)]
    pub(crate) host_events: Vec<NativeHostEvent>,
    #[serde(default)]
    pub(crate) csp_violations: Vec<NativeCspViolation>,
}

enum NativeWorkerDispatch<'a> {
    SharedWorkerConnect {
        transfer_ports: &'a [NativeMessagePortTransfer],
    },
    Message {
        data: &'a serde_json::Value,
        transfer_ports: &'a [NativeMessagePortTransfer],
        object_urls: &'a [NativeObjectUrlTransfer],
    },
    MessagePort {
        bridge_key: &'a str,
        data: &'a serde_json::Value,
        transfer_ports: &'a [NativeMessagePortTransfer],
        object_urls: &'a [NativeObjectUrlTransfer],
    },
    MessagePortClose {
        bridge_key: &'a str,
    },
    Fetch {
        request_id: u32,
        payload: &'a serde_json::Value,
    },
    FetchStream {
        stream_id: u32,
        payload: &'a serde_json::Value,
    },
    FetchUpload {
        stream_id: u32,
        payload: &'a serde_json::Value,
    },
    ServiceWorkerCache {
        request_id: u32,
        payload: &'a serde_json::Value,
    },
    ServiceWorkerOpenWindow {
        request_id: u32,
        payload: &'a serde_json::Value,
    },
    ServiceWorkerNavigationPreload {
        request_id: u32,
        payload: &'a serde_json::Value,
    },
    ServiceWorkerFetch {
        payload: &'a serde_json::Value,
    },
    ServiceWorkerLifecycle {
        event_type: &'a str,
    },
    ServiceWorkerMessage {
        payload: &'a serde_json::Value,
    },
    WebSocket {
        socket_id: u32,
        payload: &'a serde_json::Value,
        csp_violations: &'a [NativeCspViolation],
    },
    EventSource {
        source_id: u32,
        payload: &'a serde_json::Value,
        csp_violations: &'a [NativeCspViolation],
    },
}

enum NativePageDispatch<'a> {
    Fetch {
        request_id: u32,
        payload: &'a serde_json::Value,
    },
    FontFaceInstall {
        request_id: u32,
        payload: &'a serde_json::Value,
    },
    WebSocket {
        socket_id: u32,
        payload: &'a serde_json::Value,
    },
    EventSource {
        source_id: u32,
        payload: &'a serde_json::Value,
    },
    FetchStream {
        stream_id: u32,
        payload: &'a serde_json::Value,
    },
    FetchUpload {
        stream_id: u32,
        payload: &'a serde_json::Value,
    },
    ScriptError {
        payload: &'a serde_json::Value,
    },
    ServiceWorkerRegistration {
        request_id: u32,
        payload: &'a serde_json::Value,
    },
}

/// A browser-owned request for a Service Worker to create a new top-level
/// WindowClient. The source context is filled by the owning NativeEngine after
/// the content-process response crosses back into the browser backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeServiceWorkerOpenWindowRequest {
    pub request_id: u32,
    pub worker_id: u32,
    pub url: String,
    #[serde(default, skip_serializing)]
    pub source_context_id: String,
    #[serde(default, skip_serializing)]
    pub source_frame_id: String,
}

pub struct NativeWorkerWebSocketCommand {
    pub worker_id: u32,
    pub worker_url: String,
    pub command: NativeScriptCommand,
}

pub struct NativeWorkerEventSourceCommand {
    pub worker_id: u32,
    pub worker_url: String,
    pub command: NativeScriptCommand,
}

struct NativeDedicatedWorker {
    url: String,
    module_name: String,
    module_base_url: String,
    runtime: NativeJavaScriptRuntime,
    top_level_await_pending: bool,
    import_script_counts: BTreeMap<String, usize>,
    dynamic_import_referrers: BTreeSet<String>,
    module_sources: BTreeMap<String, String>,
    module_base_urls: BTreeMap<String, String>,
    module_referrer_policies: BTreeMap<String, NativeFetchReferrerPolicy>,
    is_module: bool,
    is_shared: bool,
    credentials: String,
    extended_lifetime: bool,
    next_module_turn: AtomicU64,
}

pub(crate) struct NativeWorkerModuleGraph {
    pub(crate) root_name: String,
    pub(crate) root_base_url: String,
    pub(crate) sources: BTreeMap<String, String>,
    pub(crate) base_urls: BTreeMap<String, String>,
    pub(crate) referrer_policies: BTreeMap<String, NativeFetchReferrerPolicy>,
}

struct NativeWorkerClassicScriptGraph {
    source: String,
    import_script_counts: BTreeMap<String, usize>,
    dynamic_import_referrers: BTreeSet<String>,
}

fn effective_worker_module_referrer_policy(
    inherited: Option<NativeFetchReferrerPolicy>,
    response: Option<NativeFetchReferrerPolicy>,
) -> Option<NativeFetchReferrerPolicy> {
    inherited.map(|policy| response.unwrap_or(policy))
}

fn effective_worker_global_referrer_policy(
    worker_url: &str,
    creator_policy: Option<NativeFetchReferrerPolicy>,
    response_policy: Option<NativeFetchReferrerPolicy>,
) -> NativeFetchReferrerPolicy {
    if is_network_url(worker_url) {
        response_policy.unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin)
    } else if is_file_url(worker_url) {
        creator_policy.unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin)
    } else {
        response_policy
            .or(creator_policy)
            .unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin)
    }
}

#[derive(Clone, Copy)]
enum NativeWorkerClassicDynamicImportMode {
    RewritePerScript,
    RejectPerScript,
}

pub(super) struct NativeWorkerScriptResourceRequest<'a> {
    request_id: u32,
    document_url: &'a str,
    href: &'a str,
    referrer_url: Option<&'a str>,
    max_source_bytes: usize,
    module_type: Option<NativeModuleResourceType>,
    credentials_mode: Option<&'a str>,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
    shared_worker_module_entry: bool,
    parent_loader_owns_network: bool,
}

async fn load_worker_script_resource(
    loader: &mut NativeResourceLoader,
    parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    request: NativeWorkerScriptResourceRequest<'_>,
) -> Result<Option<NativeScriptResource>, NativeEngineError> {
    let NativeWorkerScriptResourceRequest {
        request_id,
        document_url,
        href,
        referrer_url,
        max_source_bytes,
        module_type,
        credentials_mode,
        referrer_policy,
        shared_worker_module_entry,
        parent_loader_owns_network,
    } = request;
    let request_base = referrer_url.unwrap_or(document_url);
    let network_target = Url::parse(href)
        .or_else(|_| Url::parse(without_fragment(request_base)).and_then(|base| base.join(href)))
        .is_ok_and(|target| is_network_url(target.as_str()));
    if network_target {
        if let Some(parent_fetch_broker) = parent_fetch_broker {
            return parent_fetch_broker
                .load_worker_script(
                    request_id,
                    document_url,
                    href,
                    referrer_url,
                    max_source_bytes,
                    module_type,
                    credentials_mode,
                    referrer_policy,
                    shared_worker_module_entry,
                    None,
                )
                .await
                .map(|(resource, _)| resource);
        }
        if !parent_loader_owns_network {
            return Err(missing_parent_network_authority("Worker script load"));
        }
    }
    if shared_worker_module_entry {
        return loader
            .load_shared_worker_module_async(
                document_url,
                href,
                max_source_bytes,
                credentials_mode.ok_or_else(|| {
                    NativeEngineError::invalid(
                        "SharedWorker module credentials",
                        "must be provided for a module entry",
                    )
                })?,
                referrer_policy,
            )
            .await;
    }
    if let Some(referrer_url) = referrer_url {
        return loader
            .load_worker_script_dependency_async_with_referrer_source(NativeScriptLoadRequest {
                document_url,
                referrer_url: Some(referrer_url),
                href,
                max_source_bytes,
                subresource_kind: NativeSubresourceKind::Script,
                parser_inserted: true,
                nonce: None,
                integrity: None,
                crossorigin: None,
                object_url: None,
                module_type,
                worker_credentials_mode: credentials_mode,
                referrer_policy,
            })
            .await;
    }
    loader
        .load_worker_async_with_referrer_policy(
            document_url,
            href,
            max_source_bytes,
            referrer_policy,
        )
        .await
}

#[derive(Debug, Clone)]
struct NativeMessagePortRoute {
    worker_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeSharedWorkerKey {
    constructor_storage_key: NativeSharedWorkerStorageKey,
    constructor_url: String,
    name: String,
}

pub(crate) fn validate_native_shared_worker_options(
    worker_type: &str,
    credentials: &str,
) -> Result<bool, NativeEngineError> {
    let is_module = match worker_type {
        "classic" => false,
        "module" => true,
        _ => {
            return Err(NativeEngineError::invalid(
                "native SharedWorker type",
                "must be classic or module",
            ));
        }
    };
    if !matches!(credentials, "omit" | "same-origin" | "include") {
        return Err(NativeEngineError::invalid(
            "native SharedWorker credentials",
            "must be omit, same-origin, or include",
        ));
    }
    Ok(is_module)
}

impl NativeDedicatedWorker {
    fn evaluate_initial(
        &self,
        worker_id: u32,
        source: &str,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        for referrer in &self.dynamic_import_referrers {
            self.runtime.register_dynamic_import_referrer(referrer)?;
        }
        if self.is_shared && self.is_module {
            self.runtime
                .evaluate_shared_worker_module(NativeWorkerModuleEvaluation {
                    worker_id,
                    worker_url: &self.url,
                    module_name: &self.module_name,
                    module_base_url: &self.module_base_url,
                    source,
                    module_sources: &self.module_sources,
                    module_base_urls: &self.module_base_urls,
                    module_referrer_policies: &self.module_referrer_policies,
                    referrer_policy: self
                        .module_referrer_policies
                        .get(&self.module_name)
                        .copied(),
                    capture_startup_runtime_errors: false,
                })
        } else if self.is_shared {
            self.runtime.evaluate_shared_worker_prepared(
                worker_id,
                &self.url,
                source,
                &self.import_script_counts,
            )
        } else if self.is_module {
            self.runtime
                .evaluate_worker_module(NativeWorkerModuleEvaluation {
                    worker_id,
                    worker_url: &self.url,
                    module_name: &self.module_name,
                    module_base_url: &self.module_base_url,
                    source,
                    module_sources: &self.module_sources,
                    module_base_urls: &self.module_base_urls,
                    module_referrer_policies: &self.module_referrer_policies,
                    referrer_policy: self
                        .module_referrer_policies
                        .get(&self.module_name)
                        .copied(),
                    capture_startup_runtime_errors: true,
                })
        } else {
            self.runtime.evaluate_worker_prepared(
                worker_id,
                &self.url,
                source,
                &self.import_script_counts,
            )
        }
    }

    fn evaluate_turn(
        &self,
        worker_id: u32,
        source: &str,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if self.is_shared && self.is_module {
            let turn = self.next_module_turn.fetch_add(1, Ordering::Relaxed);
            let module_name = format!("{}#glass-shared-worker-turn-{turn}", self.module_name);
            self.runtime
                .evaluate_shared_worker_module(NativeWorkerModuleEvaluation {
                    worker_id,
                    worker_url: &self.url,
                    module_name: &module_name,
                    module_base_url: &self.module_base_url,
                    source,
                    module_sources: &self.module_sources,
                    module_base_urls: &self.module_base_urls,
                    module_referrer_policies: &self.module_referrer_policies,
                    referrer_policy: self
                        .module_referrer_policies
                        .get(&self.module_name)
                        .copied(),
                    capture_startup_runtime_errors: false,
                })
        } else if self.is_shared {
            self.runtime.evaluate_shared_worker(
                worker_id,
                &self.url,
                source,
                &self.import_script_counts,
            )
        } else if self.is_module {
            let turn = self.next_module_turn.fetch_add(1, Ordering::Relaxed);
            let module_name = format!("{}#glass-worker-turn-{turn}", self.module_name);
            self.runtime
                .evaluate_worker_module(NativeWorkerModuleEvaluation {
                    worker_id,
                    worker_url: &self.url,
                    module_name: &module_name,
                    module_base_url: &self.module_base_url,
                    source,
                    module_sources: &self.module_sources,
                    module_base_urls: &self.module_base_urls,
                    module_referrer_policies: &self.module_referrer_policies,
                    referrer_policy: self
                        .module_referrer_policies
                        .get(&self.module_name)
                        .copied(),
                    capture_startup_runtime_errors: false,
                })
        } else {
            self.runtime
                .evaluate_worker(worker_id, &self.url, source, &self.import_script_counts)
        }
    }

    fn evaluate_turn_with_event(
        &self,
        worker_id: u32,
        dispatch: NativeWorkerDispatch<'_>,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let empty_import_script_counts = BTreeMap::new();
        let import_script_counts = if self.is_module {
            &empty_import_script_counts
        } else {
            &self.import_script_counts
        };
        let bootstrap = if self.is_shared {
            shared_worker_bootstrap(
                worker_id,
                &self.url,
                self.runtime.now_ms(),
                import_script_counts,
                self.is_module,
                self.runtime.worker_global_referrer_policy,
            )?
        } else {
            worker_bootstrap(
                worker_id,
                &self.url,
                self.runtime.now_ms(),
                import_script_counts,
                self.is_module,
                self.runtime.worker_global_referrer_policy,
                WorkerBootstrapCapabilities {
                    report_message_callback_errors: true,
                    forward_message_callback_errors: true,
                    capture_connect_callback_errors: false,
                },
            )?
        };
        let module_name = if self.is_module {
            let turn = self.next_module_turn.fetch_add(1, Ordering::Relaxed);
            let prefix = if self.is_shared {
                "glass-shared-worker-event"
            } else {
                "glass-worker-event"
            };
            Some(format!("{}#{prefix}-{turn}", self.url))
        } else {
            None
        };
        if self.is_module {
            self.runtime.set_module_sources(self.module_sources.clone());
        }
        self.runtime
            .evaluate_worker_source_with_bootstrap_and_event(
                runtime::NativeWorkerSourceIdentity {
                    worker_id,
                    worker_url: &self.url,
                    module_name: module_name.as_deref(),
                },
                "undefined;",
                bootstrap,
                runtime::NativeWorkerExecutionMode {
                    service_worker: false,
                    await_promise: false,
                },
                Some(dispatch),
            )
    }

    fn evaluate_shared_connect(
        &self,
        worker_id: u32,
        transfer_port: &NativeMessagePortTransfer,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.runtime.evaluate_shared_worker_connect(
            worker_id,
            &self.url,
            transfer_port,
            self.is_module,
        )
    }

    fn evaluate_websocket_event(
        &self,
        worker_id: u32,
        socket_id: u32,
        event: &serde_json::Value,
        csp_violations: &[NativeCspViolation],
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.evaluate_turn_with_event(
            worker_id,
            NativeWorkerDispatch::WebSocket {
                socket_id,
                payload: event,
                csp_violations,
            },
        )
    }

    fn evaluate_event_source_event(
        &self,
        worker_id: u32,
        source_id: u32,
        event: &serde_json::Value,
        csp_violations: &[NativeCspViolation],
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.evaluate_turn_with_event(
            worker_id,
            NativeWorkerDispatch::EventSource {
                source_id,
                payload: event,
                csp_violations,
            },
        )
    }

    fn evaluate_fetch_stream_event(
        &self,
        worker_id: u32,
        stream_id: u32,
        event: &serde_json::Value,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.evaluate_turn_with_event(
            worker_id,
            NativeWorkerDispatch::FetchStream {
                stream_id,
                payload: event,
            },
        )
    }

    fn evaluate_fetch_upload_event(
        &self,
        worker_id: u32,
        stream_id: u32,
        event: &serde_json::Value,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        self.evaluate_turn_with_event(
            worker_id,
            NativeWorkerDispatch::FetchUpload {
                stream_id,
                payload: event,
            },
        )
    }
}

/// Owns dedicated-worker realms and the bounded messages waiting for their
/// page owner. Worker execution is deliberately serialized at the page turn
/// boundary: this keeps the native backend deterministic while preserving the
/// observable message/error/termination contract.
pub struct NativeWorkerRegistry {
    workers: BTreeMap<u32, NativeDedicatedWorker>,
    shared_worker_keys: BTreeMap<NativeSharedWorkerKey, u32>,
    shared_worker_owners: BTreeMap<u32, BTreeSet<(String, String, u32)>>,
    next_worker_timer_id: u32,
    pending_messages: VecDeque<NativeWorkerMessage>,
    pending_message_port_messages: VecDeque<NativeMessagePortPageMessage>,
    pending_page_message_port_commands: VecDeque<NativePageMessagePortCommand>,
    pending_websocket_commands: VecDeque<NativeWorkerWebSocketCommand>,
    pending_event_source_commands: VecDeque<NativeWorkerEventSourceCommand>,
    worker_fetch_stream_connections: BTreeMap<(u32, u32), NativeFetchStreamConnection>,
    worker_fetch_upload_connections: BTreeMap<(u32, u32), NativeFetchUploadConnection>,
    stream_worker_fetches: bool,
    parent_loader_owns_network: bool,
    message_port_routes: BTreeMap<String, NativeMessagePortRoute>,
}

fn resolve_worker_module_specifier(
    module_base_url: &str,
    specifier: &str,
) -> Result<String, NativeEngineError> {
    if !specifier.starts_with("./")
        && !specifier.starts_with("../")
        && !specifier.starts_with('/')
        && !specifier.starts_with("http://")
        && !specifier.starts_with("https://")
        && !specifier.starts_with("fixture://")
        && !specifier
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
        && !specifier.starts_with("//")
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "bare Worker module specifiers require an import map".into(),
        });
    }
    let target = resolve_module_request_url(module_base_url, specifier)?;
    let target = Url::parse(&target).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "Worker module URL is invalid".into(),
    })?;
    if !matches!(target.scheme(), "http" | "https" | "fixture" | "file") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "Worker module URL must use HTTP(S), a registered fixture, or a rooted file"
                .into(),
        });
    }
    Ok(target.to_string())
}

fn worker_fetch_response_payload(
    result: Result<NativeFetchResponse, NativeEngineError>,
) -> serde_json::Value {
    match result {
        Ok(response) => serde_json::json!({
            "url": response.url,
            "status": response.status,
            "statusText": response.status_text,
            "contentType": response.content_type,
            "headers": response.headers,
            "body": String::from_utf8_lossy(&response.body),
            "bodyBase64": base64::engine::general_purpose::STANDARD.encode(&response.body),
            "redirected": response.redirected,
            "opaque": response.opaque,
            "opaqueRedirect": response.opaque_redirect,
        }),
        Err(error) => serde_json::json!({
            "error": error.to_string(),
            "timeout": matches!(
                &error,
                NativeEngineError::Network { reason, .. } if reason == "request timed out"
            ),
        }),
    }
}

fn worker_fetch_response_stream_payload(
    response: NativeFetchResponse,
    stream_id: u32,
) -> serde_json::Value {
    let mut payload = worker_fetch_response_payload(Ok(response));
    if let Some(object) = payload.as_object_mut() {
        object.insert("bodyStreamId".into(), serde_json::Value::from(stream_id));
    }
    payload
}

fn worker_fetch_stream_event_payload(event: &NativeFetchStreamEvent) -> serde_json::Value {
    match event {
        NativeFetchStreamEvent::Chunk { data } => serde_json::json!({
            "type": "chunk",
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(data),
        }),
        NativeFetchStreamEvent::End => serde_json::json!({"type": "end"}),
        NativeFetchStreamEvent::Error { message } => {
            serde_json::json!({"type": "error", "message": message})
        }
    }
}

/// A browser-context creation request emitted by `window.open`.
///
/// The JavaScript realm never receives a direct native target handle. The
/// request crosses the engine boundary and is materialized by the target
/// owner, which keeps popup creation and named-context reuse serialized with
/// the rest of the browser topology.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NativePopupRequest {
    pub url: String,
    pub target: String,
    #[serde(default)]
    pub method: NativeNavigationMethod,
    #[serde(default)]
    pub body: Option<NativeRequestBody>,
    #[serde(default)]
    pub body_content_type: Option<String>,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub object_url: Option<NativeObjectUrlResource>,
    #[serde(default, skip_serializing)]
    pub source_context_id: String,
}

/// A bounded cross-context message emitted by a native page realm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativePostMessageRequest {
    pub target: String,
    pub target_origin: String,
    pub data: serde_json::Value,
    #[serde(default)]
    pub target_context_id: Option<String>,
    #[serde(default)]
    pub transfer_ports: Vec<NativeMessagePortTransfer>,
    /// Blob URL snapshots captured while the source realm still owns the URL.
    #[serde(default)]
    pub object_urls: Vec<NativeObjectUrlTransfer>,
    #[serde(default, skip_serializing)]
    pub source_context_id: String,
    #[serde(default, skip_serializing)]
    pub source_origin: String,
    #[serde(default, skip_serializing)]
    pub source_frame_id: String,
}

/// A bounded request for the parent target owner to close a WindowProxy target.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NativeWindowCloseRequest {
    pub target: String,
    #[serde(default)]
    pub target_context_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub source_context_id: String,
}

/// A bounded request for the parent target owner to navigate a WindowProxy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NativeWindowNavigationRequest {
    pub target: String,
    #[serde(default)]
    pub target_context_id: Option<String>,
    pub href: String,
    #[serde(default)]
    pub method: NativeNavigationMethod,
    #[serde(default)]
    pub body: Option<NativeRequestBody>,
    #[serde(default)]
    pub body_content_type: Option<String>,
    #[serde(default)]
    pub replace: bool,
    #[serde(default)]
    pub object_url: Option<NativeObjectUrlResource>,
    #[serde(default, skip_serializing)]
    pub source_context_id: String,
}

/// A trusted parent-owned refresh for one cached WindowProxy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NativeWindowProxyUpdate {
    pub cache_key: String,
    pub target_context_id: String,
    pub href: String,
    pub name: String,
    pub closed: bool,
}

/// A parent-owned snapshot for one directly embedded browsing context.
///
/// The child engine remains the authority for navigation and mutation. The
/// snapshot only lets the embedding realm expose the browser's frame object
/// relationships and same-origin read projections without sharing an engine
/// pointer across JavaScript realms.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFrameScriptBinding {
    pub node_index: u32,
    pub frame_id: String,
    pub url: String,
    pub origin: String,
    pub generation: u32,
    pub revision: u64,
    pub same_origin: bool,
    pub document: NativeScriptDocumentSnapshot,
    #[serde(default)]
    pub children: Vec<NativeFrameScriptBinding>,
}

/// A bounded Window/document snapshot used to install the real parent/top
/// relationships while a child browsing context is selected.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFrameScriptWindow {
    pub context_id: String,
    pub url: String,
    pub origin: String,
    pub generation: u32,
    pub revision: u64,
    pub same_origin: bool,
    pub document: NativeScriptDocumentSnapshot,
    #[serde(default)]
    pub children: Vec<NativeFrameScriptBinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFrameScriptContext {
    pub current_frame_id: String,
    pub parent: Option<NativeFrameScriptWindow>,
    pub top: Option<NativeFrameScriptWindow>,
    pub frame_element: Option<NativeScriptElementSnapshot>,
}

#[derive(Debug, Clone)]
pub(crate) struct NativePageNavigation {
    pub(crate) href: String,
    pub(crate) replace_history: bool,
    pub(crate) object_url: Option<NativeObjectUrlResource>,
}

#[derive(Default)]
pub(crate) struct NativePageScriptResult {
    pub(crate) pending_fetches: Vec<NativeScriptCommand>,
    pub(crate) service_worker_commands: Vec<NativeScriptCommand>,
    pub(crate) websocket_commands: Vec<NativeScriptCommand>,
    pub(crate) event_source_commands: Vec<NativeScriptCommand>,
    /// External/module sources discovered by a dynamic script and awaiting
    /// resource-loader handoff in the owning content process.
    pub(crate) pending_script_sources: Vec<NativePageScriptSource>,
    pub(crate) scroll_commands: Vec<NativeScriptCommand>,
    pub(crate) navigation: Option<NativePageNavigation>,
    pub(crate) dialogs: Vec<NativeDialog>,
    pub(crate) events: Vec<(u32, NativeEventKind)>,
}

/// A bounded JavaScript dialog emitted by a native page realm.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeDialog {
    pub(crate) dialog_type: String,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) default_value: Option<String>,
}

pub(crate) type NativeDialogHandler = Arc<
    dyn Fn(NativeDialog, String) -> Result<Option<NativeDialogResolution>, String> + Send + Sync,
>;

fn install_native_dialog_host_call(
    ctx: rquickjs::Ctx<'_>,
    dialog_handler_slot: Arc<Mutex<Option<NativeDialogHandler>>>,
    dialog_url_slot: Arc<Mutex<String>>,
) -> Result<(), Error> {
    let native_dialog_call = Function::new(
        ctx.clone(),
        move |dialog_type: String,
              message: String,
              default_value: Option<String>|
              -> Option<String> {
            let error_json = |reason: &str| {
                serde_json::to_string(&serde_json::json!({"error": reason}))
                    .unwrap_or_else(|_| r#"{"error":"native dialog host failed"}"#.into())
            };
            if !matches!(dialog_type.as_str(), "alert" | "confirm" | "prompt") {
                return Some(error_json("native dialog type is invalid"));
            }
            if message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES
                || default_value
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
            {
                return Some(error_json("native dialog text exceeds its byte limit"));
            }
            let handler = match dialog_handler_slot.lock() {
                Ok(handler) => handler.clone(),
                Err(_) => return Some(error_json("native dialog handler state is unavailable")),
            };
            let handler = handler?;
            let url = match dialog_url_slot.lock() {
                Ok(url) => url.clone(),
                Err(_) => return Some(error_json("native dialog URL is unavailable")),
            };
            let dialog = NativeDialog {
                dialog_type,
                message,
                default_value,
            };
            let resolution = match handler(dialog, url) {
                Ok(Some(resolution)) => resolution,
                Ok(None) => return None,
                Err(reason) => return Some(error_json(&reason)),
            };
            let response = serde_json::json!({
                "accepted": resolution.accepted,
                "promptValue": resolution.prompt_value,
            });
            Some(serde_json::to_string(&response).unwrap_or_else(|_| {
                r#"{"error":"native dialog response could not be encoded"}"#.into()
            }))
        },
    )?;
    ctx.globals()
        .set("__glassNativeDialogCall", native_dialog_call)
}

/// Origin-keyed page storage retained by the native runtime owner.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct NativeWebStorageState {
    local: BTreeMap<String, BTreeMap<String, String>>,
    session: BTreeMap<String, BTreeMap<String, String>>,
}

/// Bounded JSON-backed IndexedDB state for one storage origin.
///
/// The native engine intentionally stores the result of the supported
/// structured-clone subset rather than pretending to implement every browser
/// value type. Database and store metadata are durable; transactions and
/// requests are represented by the JavaScript realm and validated at the
/// profile boundary.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct NativeIndexedDbState {
    origins: BTreeMap<String, NativeIndexedDbOrigin>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct NativeIndexedDbOrigin {
    databases: BTreeMap<String, NativeIndexedDbDatabase>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) struct NativeIndexedDbDatabase {
    version: u64,
    stores: BTreeMap<String, NativeIndexedDbObjectStore>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) struct NativeIndexedDbObjectStore {
    #[serde(default)]
    key_path: Option<String>,
    #[serde(default)]
    auto_increment: bool,
    #[serde(default = "default_indexed_db_next_key")]
    next_key: u64,
    #[serde(default)]
    indexes: BTreeMap<String, NativeIndexedDbIndex>,
    records: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct NativeIndexedDbIndex {
    key_path: String,
    #[serde(default)]
    unique: bool,
    #[serde(default)]
    multi_entry: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub(crate) enum NativeIndexedDbChange {
    ReplaceDatabase {
        storage_key: String,
        name: String,
        database: Option<NativeIndexedDbDatabase>,
    },
    ReplaceStore {
        storage_key: String,
        database: String,
        version: u64,
        name: String,
        store: Option<NativeIndexedDbObjectStore>,
    },
    UpdateStoreMetadata {
        storage_key: String,
        database: String,
        store: String,
        key_path: Option<String>,
        auto_increment: bool,
        next_key: u64,
    },
    UpdateStoreIndexes {
        storage_key: String,
        database: String,
        store: String,
        indexes: BTreeMap<String, NativeIndexedDbIndex>,
    },
    PutRecord {
        storage_key: String,
        database: String,
        store: String,
        key: String,
        value: serde_json::Value,
    },
    DeleteRecord {
        storage_key: String,
        database: String,
        store: String,
        key: String,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
struct NativeWebStorageView {
    local: BTreeMap<String, String>,
    session: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) enum NativePageScript {
    Classic {
        source: String,
        base_url: String,
        node_index: Option<u32>,
    },
    Module {
        name: String,
        source: String,
        base_url: String,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
        node_index: Option<u32>,
    },
    ModuleDependency {
        name: String,
        source: String,
        base_url: String,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    },
    ImportMap(NativeModuleImportMap),
}

fn module_source_maps(
    sources: &[NativePageScript],
) -> (
    BTreeMap<String, String>,
    BTreeMap<String, String>,
    BTreeMap<String, NativeFetchReferrerPolicy>,
) {
    let mut module_sources = BTreeMap::new();
    let mut module_base_urls = BTreeMap::new();
    let mut module_referrer_policies = BTreeMap::new();
    for source in sources {
        let (name, module_source, base_url, referrer_policy) = match source {
            NativePageScript::Module {
                name,
                source,
                base_url,
                referrer_policy,
                ..
            } => (name, source, base_url, referrer_policy),
            NativePageScript::ModuleDependency {
                name,
                source,
                base_url,
                referrer_policy,
            } => (name, source, base_url, referrer_policy),
            NativePageScript::Classic { .. } | NativePageScript::ImportMap(_) => continue,
        };
        module_sources.insert(name.clone(), module_source.clone());
        module_base_urls.insert(name.clone(), base_url.clone());
        if let Some(referrer_policy) = referrer_policy {
            module_referrer_policies.insert(name.clone(), *referrer_policy);
        }
    }
    (module_sources, module_base_urls, module_referrer_policies)
}

struct NativeModuleResolver {
    import_map: Arc<Mutex<NativeModuleImportMap>>,
    module_base_urls: Arc<Mutex<BTreeMap<String, String>>>,
    dynamic_module_aliases: Arc<Mutex<BTreeMap<String, String>>>,
}

pub(crate) const NATIVE_JSON_MODULE_NAME_PREFIX: &str = "glass-internal-json-module:";
// QuickJS deduplicates static request literals before the resolver sees their
// import attributes. This marker keeps JSON and JavaScript requests distinct.
const NATIVE_JSON_MODULE_REQUEST_PREFIX: &str = "glass-internal-json-request:";

pub(crate) fn native_module_loader_name(
    url: &str,
    module_type: NativeModuleResourceType,
) -> String {
    match module_type {
        NativeModuleResourceType::JavaScript => url.to_owned(),
        NativeModuleResourceType::Json => format!("{NATIVE_JSON_MODULE_NAME_PREFIX}{url}"),
        NativeModuleResourceType::Unsupported => {
            format!("glass-internal-unsupported-module:{url}")
        }
    }
}

fn native_module_type_from_attributes<'js>(
    attributes: Option<ImportAttributes<'js>>,
    base: &str,
    name: &str,
) -> rquickjs::Result<NativeModuleResourceType> {
    let Some(attributes) = attributes else {
        return Ok(NativeModuleResourceType::JavaScript);
    };
    for key in attributes.keys() {
        let key = key.map_err(|_| {
            Error::new_resolving_message(base, name, "module import attributes are unavailable")
        })?;
        if key != "type" {
            return Err(Error::new_resolving_message(
                base,
                name,
                "only the module import attribute `type` is supported",
            ));
        }
    }
    match attributes.get_type().map_err(|_| {
        Error::new_resolving_message(base, name, "module import type attribute is invalid")
    })? {
        None => Ok(NativeModuleResourceType::JavaScript),
        Some(value) if value == "json" => Ok(NativeModuleResourceType::Json),
        Some(_) => Err(Error::new_resolving_message(
            base,
            name,
            "the requested module type is unsupported",
        )),
    }
}

impl Resolver for NativeModuleResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &rquickjs::Ctx<'js>,
        base: &str,
        name: &str,
        attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        let module_type = native_module_type_from_attributes(attributes, base, name)?;
        let requested_name = if module_type == NativeModuleResourceType::Json {
            name.strip_prefix(NATIVE_JSON_MODULE_REQUEST_PREFIX)
                .unwrap_or(name)
        } else {
            name
        };
        let alias_target = self
            .dynamic_module_aliases
            .lock()
            .map_err(|_| {
                Error::new_resolving_message(base, name, "dynamic module aliases are unavailable")
            })?
            .remove(requested_name);
        if let Some(target) = alias_target {
            return Ok(native_module_loader_name(&target, module_type));
        }
        let base_url = self
            .module_base_urls
            .lock()
            .map_err(|_| {
                Error::new_resolving_message(base, name, "module base URL map is unavailable")
            })?
            .get(base)
            .cloned()
            .unwrap_or_else(|| base.to_owned());
        let base = Url::parse(&base_url).map_err(|_| {
            Error::new_resolving_message(&base_url, name, "module base is not a URL")
        })?;
        let import_map = self.import_map.lock().map_err(|_| {
            Error::new_resolving_message(base.as_str(), name, "module import map is unavailable")
        })?;
        let target = import_map
            .resolve(base.as_str(), requested_name)
            .map_err(|reason| {
                Error::new_resolving_message(base.as_str(), requested_name, reason)
            })?;
        Ok(native_module_loader_name(&target, module_type))
    }
}

#[derive(Clone, Copy)]
enum NativeModuleDynamicImportMode {
    Preserve,
    Resolve,
    Reject,
}

struct NativeModuleLoader {
    sources: Arc<Mutex<BTreeMap<String, String>>>,
    module_base_urls: Arc<Mutex<BTreeMap<String, String>>>,
    module_referrer_policies: Arc<Mutex<BTreeMap<String, NativeFetchReferrerPolicy>>>,
    dynamic_import_mode: Arc<Mutex<NativeModuleDynamicImportMode>>,
}

impl Loader for NativeModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &rquickjs::Ctx<'js>,
        name: &str,
        attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js>> {
        let module_type =
            native_module_type_from_attributes(attributes, name, name).map_err(|_| {
                Error::new_loading_message(name, "module import attributes are invalid")
            })?;
        let is_json_module = name.starts_with(NATIVE_JSON_MODULE_NAME_PREFIX);
        if is_json_module != (module_type == NativeModuleResourceType::Json) {
            return Err(Error::new_loading_message(
                name,
                "module name and import type do not match",
            ));
        }
        let source = self
            .sources
            .lock()
            .ok()
            .and_then(|sources| sources.get(name).cloned())
            .ok_or_else(|| Error::new_loading_message(name, "module was not prefetched"))?;
        let dynamic_import_mode = *self
            .dynamic_import_mode
            .lock()
            .map_err(|_| Error::new_loading_message(name, "module loader state is unavailable"))?;
        let source = if is_json_module {
            let json_text = serde_json::to_string(&source)
                .map_err(|_| Error::new_loading_message(name, "JSON module text is invalid"))?;
            let source = format!("export default globalThis.__glassParseJSONModule({json_text});");
            if source.len() > MAX_NATIVE_SCRIPT_BYTES {
                return Err(Error::new_loading_message(
                    name,
                    "transformed JSON module exceeds its size limit",
                ));
            }
            source
        } else {
            match dynamic_import_mode {
                NativeModuleDynamicImportMode::Preserve => source,
                NativeModuleDynamicImportMode::Resolve => {
                    let module_base_url = self
                        .module_base_urls
                        .lock()
                        .map_err(|_| {
                            Error::new_loading_message(name, "module base URLs are unavailable")
                        })?
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| name.to_owned());
                    let module_referrer =
                        serde_json::to_string(&module_base_url).map_err(|_| {
                            Error::new_loading_message(name, "module base URL could not be encoded")
                        })?;
                    let referrer_policy = self
                        .module_referrer_policies
                        .lock()
                        .map_err(|_| {
                            Error::new_loading_message(
                                name,
                                "module referrer policies are unavailable",
                            )
                        })?
                        .get(name)
                        .copied();
                    let referrer_policy = native_module_referrer_policy_expression(referrer_policy);
                    rewrite_runtime_dynamic_module_imports_with_policy(
                        &source,
                        &module_referrer,
                        &referrer_policy,
                    )
                    .map_err(|_| {
                        Error::new_loading_message(
                            name,
                            "transformed module exceeds its size limit",
                        )
                    })?
                }
                NativeModuleDynamicImportMode::Reject => {
                    rewrite_dynamic_imports_as_rejected(&source).map_err(|_| {
                        Error::new_loading_message(
                            name,
                            "transformed module exceeds its size limit",
                        )
                    })?
                }
            }
        };
        let source = if is_json_module {
            source
        } else {
            rewrite_static_json_module_specifiers(&source).map_err(|_| {
                Error::new_loading_message(
                    name,
                    "static JSON module requests could not be transformed",
                )
            })?
        };
        Module::declare(ctx.clone(), name, source)
    }
}

pub(crate) fn order_page_scripts(
    sources: Vec<(NativePageScriptTiming, NativePageScript)>,
) -> Vec<NativePageScript> {
    let mut parser_and_async = Vec::new();
    let mut deferred = Vec::new();
    for (timing, source) in sources {
        match timing {
            NativePageScriptTiming::ParserBlocking | NativePageScriptTiming::Async => {
                parser_and_async.push(source)
            }
            NativePageScriptTiming::Defer => deferred.push(source),
        }
    }
    parser_and_async.extend(deferred);
    parser_and_async
}

fn register_local_page_import_map_source(
    map_source: NativePageImportMapSource,
    import_map: &mut NativeModuleImportMap,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    sources: &mut Vec<(NativePageScriptTiming, NativePageScript)>,
    resource_events: &mut Vec<(u32, NativeEventKind)>,
) -> Result<(), NativeEngineError> {
    let node_index = map_source.node_index;
    let Some(source) = map_source.source else {
        resource_events.push((node_index, NativeEventKind::Error));
        return Ok(());
    };
    if !loader.allows_inline_script(document_url, &source, map_source.nonce.as_deref())? {
        resource_events.push((node_index, NativeEventKind::Error));
        return Ok(());
    }
    let parsed = match NativeModuleImportMap::parse(&source, document_url) {
        Ok(parsed) => parsed,
        Err(_) => {
            resource_events.push((node_index, NativeEventKind::Error));
            return Ok(());
        }
    };
    let mut merged = import_map.clone();
    if merged.merge(parsed).is_err() {
        resource_events.push((node_index, NativeEventKind::Error));
        return Ok(());
    }
    *import_map = merged;
    sources.push((
        NativePageScriptTiming::ParserBlocking,
        NativePageScript::ImportMap(import_map.clone()),
    ));
    Ok(())
}

fn enqueue_dynamic_page_scripts(
    document: &mut NativeDocument,
    commands: &[NativeScriptCommand],
    document_url: &str,
    pending: &mut VecDeque<NativePageScript>,
    pending_script_sources: &mut Vec<NativePageScriptSource>,
) -> Result<(), NativeEngineError> {
    let sources = document.take_newly_attached_page_script_sources(
        commands,
        MAX_NATIVE_INLINE_SCRIPTS,
        MAX_NATIVE_SCRIPT_BYTES,
    );
    let requires_loader = sources.iter().any(|source| {
        matches!(
            source,
            NativePageScriptSource::External { .. }
                | NativePageScriptSource::ModuleInline { .. }
                | NativePageScriptSource::ModuleExternal { .. }
                | NativePageScriptSource::ImportMap { .. }
        )
    });
    let deferred_sources = if requires_loader {
        sources.clone()
    } else {
        Vec::new()
    };
    if pending_script_sources
        .len()
        .saturating_add(deferred_sources.len())
        > MAX_NATIVE_INLINE_SCRIPTS
    {
        return Err(NativeEngineError::limit(
            "native dynamic external scripts",
            MAX_NATIVE_INLINE_SCRIPTS,
            pending_script_sources
                .len()
                .saturating_add(deferred_sources.len()),
        ));
    }
    pending_script_sources.extend(deferred_sources);
    if requires_loader {
        return Ok(());
    }
    for script in page_script_sources_to_scripts(sources, document_url, "glass-dynamic-module")
        .into_iter()
        .rev()
    {
        pending.push_front(script);
    }
    Ok(())
}

pub(crate) fn page_script_sources_to_scripts(
    sources: Vec<NativePageScriptSource>,
    document_url: &str,
    module_name_prefix: &str,
) -> Vec<NativePageScript> {
    sources
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| match source {
            NativePageScriptSource::Inline {
                source, node_index, ..
            } => Some(NativePageScript::Classic {
                source,
                base_url: document_url.to_owned(),
                node_index: Some(node_index),
            }),
            NativePageScriptSource::ModuleInline {
                source,
                node_index,
                referrer_policy,
                ..
            } => Some(NativePageScript::Module {
                name: format!("{document_url}#{module_name_prefix}-{node_index}-{index}"),
                source,
                base_url: document_url.to_owned(),
                referrer_policy: Some(referrer_policy),
                node_index: Some(node_index),
            }),
            NativePageScriptSource::External { .. }
            | NativePageScriptSource::ModuleExternal { .. }
            | NativePageScriptSource::ImportMap { .. } => None,
        })
        .collect()
}

fn page_script_error_message(error: &NativeEngineError) -> String {
    let message = match error {
        NativeEngineError::Worker { reason, .. } => reason
            .strip_prefix("JavaScript evaluation failed: ")
            .unwrap_or(reason)
            .to_owned(),
        _ => "JavaScript evaluation failed".to_owned(),
    };
    message
        .chars()
        .take(MAX_NATIVE_SCRIPT_BYTES.min(4096))
        .collect()
}

fn native_font_face_script() -> &'static str {
    NATIVE_FONT_FACE_SCRIPT
}

fn is_ignorable_page_script_error(error: &NativeEngineError) -> bool {
    matches!(
        error,
        NativeEngineError::Worker { operation, .. }
            if operation == "evaluate JavaScript"
                || operation == "evaluate JavaScript module"
    )
}

fn script_command_batch_error(
    commands: &[NativeScriptCommand],
    error: NativeEngineError,
) -> NativeEngineError {
    let summary = commands
        .iter()
        .take(8)
        .map(|command| format!("{command:?}"))
        .collect::<Vec<_>>()
        .join("; ");
    let summary = summary.chars().take(2048).collect::<String>();
    NativeEngineError::TargetNotActionable {
        reason: format!("native script command batch failed ({error}); commands: {summary}"),
    }
}

pub(crate) struct NativePageScriptCommandQueues<'a> {
    pub(crate) pending_fetches: &'a mut Vec<NativeScriptCommand>,
    pub(crate) websocket_commands: &'a mut Vec<NativeScriptCommand>,
    pub(crate) event_source_commands: &'a mut Vec<NativeScriptCommand>,
}

pub(crate) struct NativePageScriptContext<'a> {
    pub(crate) document_url: &'a str,
    pub(crate) origin: &'a NativeOrigin,
    pub(crate) viewport: Viewport,
}

pub(crate) fn apply_page_script_evaluation(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    context: NativePageScriptContext<'_>,
    evaluation: NativeScriptEvaluation,
    queues: NativePageScriptCommandQueues<'_>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
    navigation: &mut Option<NativePageNavigation>,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let NativePageScriptContext {
        document_url,
        origin: document_origin,
        viewport,
    } = context;
    let NativePageScriptCommandQueues {
        pending_fetches,
        websocket_commands,
        event_source_commands,
    } = queues;
    let mut pending_evaluations = VecDeque::from([evaluation]);
    let mut events = Vec::new();
    let mut acknowledgements = 0usize;
    while let Some(evaluation) = pending_evaluations.pop_front() {
        let mut commands = Vec::new();
        for command in evaluation.commands {
            match command {
                command @ NativeScriptCommand::Fetch { .. } => pending_fetches.push(command),
                command @ (NativeScriptCommand::WebSocketOpen { .. }
                | NativeScriptCommand::WebSocketSend { .. }
                | NativeScriptCommand::WebSocketClose { .. }) => websocket_commands.push(command),
                command @ (NativeScriptCommand::EventSourceOpen { .. }
                | NativeScriptCommand::EventSourceClose { .. }) => {
                    event_source_commands.push(command)
                }
                NativeScriptCommand::Navigate { href, replace } => {
                    validate_url_text("page script navigation href", &href)?;
                    if navigation.is_some() {
                        return Err(NativeEngineError::TargetNotActionable {
                            reason: "one page-script batch cannot activate multiple navigations"
                                .into(),
                        });
                    }
                    *navigation = Some(NativePageNavigation {
                        href,
                        replace_history: replace,
                        object_url: None,
                    });
                }
                command @ NativeScriptCommand::ScrollTo { .. } => {
                    scroll_commands.push(command.clone());
                    commands.push(command);
                }
                command => commands.push(command),
            }
        }
        if commands.is_empty() {
            continue;
        }
        let request_ids = commands
            .iter()
            .filter_map(|command| match command {
                NativeScriptCommand::FontFaceInstall { request_id, .. } => Some(*request_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        let (effects, acknowledgement_payload) = if request_ids.is_empty() {
            let applied = document
                .apply_script_commands(&commands)
                .map_err(|error| script_command_batch_error(&commands, error))?;
            (applied, None)
        } else {
            let mut next = document.clone();
            match next.apply_script_commands(&commands) {
                Ok(effects) => {
                    *document = next;
                    (effects, Some(font_face_install_success_payload()))
                }
                Err(_error) if font_face_install_admission_failed(document, &commands) => {
                    (Vec::new(), Some(font_face_install_error_payload()))
                }
                Err(error) => return Err(error),
            }
        };
        events.extend(effects.into_iter().map(|(node, kind)| (node.index(), kind)));
        for request_id in request_ids {
            acknowledgements = acknowledgements.saturating_add(1);
            if acknowledgements > super::interaction::MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native FontFace install acknowledgements",
                    super::interaction::MAX_NATIVE_EFFECTS,
                    acknowledgements,
                ));
            }
            let payload = acknowledgement_payload
                .as_ref()
                .expect("FontFace acknowledgement payload initialized");
            let acknowledgement = runtime.resolve_font_face_install(
                request_id,
                payload,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
            if acknowledgement.top_level_await_pending {
                return Err(NativeEngineError::Worker {
                    operation: "native FontFace install acknowledgement".into(),
                    reason: "acknowledgement unexpectedly left top-level await pending".into(),
                });
            }
            if !acknowledgement.commands.is_empty() {
                pending_evaluations.push_back(acknowledgement);
            }
        }
    }
    Ok(events)
}

/// Apply a document command batch and acknowledge every host-owned
/// `FontFaceInstall` only after the document clone has admitted its bytes.
/// The acknowledgement may release Promise continuations, so their commands
/// are applied in bounded follow-up batches and returned to the caller for
/// network/navigation handoff.
pub(crate) fn apply_document_commands_with_font_face_ack(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    commands: &[NativeScriptCommand],
    allow_script_navigation: bool,
) -> Result<DocumentCommandEffects, NativeEngineError> {
    let mut pending = VecDeque::from([commands.to_vec()]);
    let mut follow_up_commands = Vec::new();
    let mut events = Vec::new();
    let mut acknowledgements = 0usize;
    while let Some(batch) = pending.pop_front() {
        if batch.is_empty() {
            continue;
        }
        let request_ids = batch
            .iter()
            .filter_map(|command| match command {
                NativeScriptCommand::FontFaceInstall { request_id, .. } => Some(*request_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        let (effects, acknowledgement_payload) = if request_ids.is_empty() {
            let applied = if allow_script_navigation {
                document.apply_script_commands_allowing_links(&batch)
            } else {
                document.apply_script_commands(&batch)
            }
            .map_err(|error| script_command_batch_error(&batch, error))?;
            (applied, None)
        } else {
            let mut next = document.clone();
            let applied = if allow_script_navigation {
                next.apply_script_commands_allowing_links(&batch)
            } else {
                next.apply_script_commands(&batch)
            };
            match applied {
                Ok(effects) => {
                    *document = next;
                    (effects, Some(font_face_install_success_payload()))
                }
                Err(_error) if font_face_install_admission_failed(document, &batch) => {
                    (Vec::new(), Some(font_face_install_error_payload()))
                }
                Err(error) => return Err(script_command_batch_error(&batch, error)),
            }
        };
        events.extend(effects);
        for request_id in request_ids {
            acknowledgements = acknowledgements.saturating_add(1);
            if acknowledgements > super::interaction::MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native FontFace install acknowledgements",
                    super::interaction::MAX_NATIVE_EFFECTS,
                    acknowledgements,
                ));
            }
            let payload = acknowledgement_payload
                .as_ref()
                .expect("FontFace acknowledgement payload initialized");
            let acknowledgement = runtime.resolve_font_face_install(
                request_id,
                payload,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
            if acknowledgement.top_level_await_pending {
                return Err(NativeEngineError::Worker {
                    operation: "native FontFace install acknowledgement".into(),
                    reason: "acknowledgement unexpectedly left top-level await pending".into(),
                });
            }
            if !acknowledgement.commands.is_empty() {
                follow_up_commands.extend(acknowledgement.commands.iter().cloned());
                pending.push_back(acknowledgement.commands);
            }
        }
    }
    Ok((events, follow_up_commands))
}

fn font_face_install_success_payload() -> serde_json::Value {
    serde_json::json!({"ok": true})
}

fn font_face_install_error_payload() -> serde_json::Value {
    serde_json::json!({"error": "native FontFace install was rejected"})
}

fn font_face_install_admission_failed(
    document: &NativeDocument,
    commands: &[NativeScriptCommand],
) -> bool {
    let mut probe = document.clone();
    probe.apply_script_font_face_installs(commands).is_err()
}

fn dispatch_page_scroll_events(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    context: NativePageScriptContext<'_>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
    queues: NativePageScriptCommandQueues<'_>,
    navigation: &mut Option<NativePageNavigation>,
    events: &mut Vec<(u32, NativeEventKind)>,
) -> Result<(), NativeEngineError> {
    let NativePageScriptContext {
        document_url,
        origin: document_origin,
        viewport,
    } = context;
    let NativePageScriptCommandQueues {
        pending_fetches,
        websocket_commands,
        event_source_commands,
    } = queues;
    let mut pending = std::mem::take(scroll_commands);
    let mut cursor = 0;
    while cursor < pending.len() {
        let NativeScriptCommand::ScrollTo { node_index, .. } = pending[cursor] else {
            cursor += 1;
            continue;
        };
        let event_node_index = if node_index != 0 {
            node_index
        } else {
            u32::MAX
        };
        let event_batch = host_event_batch(&[(event_node_index, NativeEventKind::Scroll)])?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native page scroll event".into(),
                reason: "native page scroll event batch was empty".into(),
            })?;
        let evaluation = runtime.evaluate_with_host_events(
            &event_batch,
            document,
            document_url,
            document_origin,
            viewport,
        )?;
        let mut emitted_scroll_commands = Vec::new();
        apply_page_script_evaluation(
            document,
            runtime,
            NativePageScriptContext {
                document_url,
                origin: document_origin,
                viewport,
            },
            evaluation,
            NativePageScriptCommandQueues {
                pending_fetches,
                websocket_commands,
                event_source_commands,
            },
            &mut emitted_scroll_commands,
            navigation,
        )?;
        if pending.len().saturating_add(emitted_scroll_commands.len())
            > super::interaction::MAX_NATIVE_EFFECTS
        {
            return Err(NativeEngineError::limit(
                "native page scroll event commands",
                super::interaction::MAX_NATIVE_EFFECTS,
                pending.len().saturating_add(emitted_scroll_commands.len()),
            ));
        }
        events.push((event_node_index, NativeEventKind::Scroll));
        pending.extend(emitted_scroll_commands);
        if events.len() > super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native page scroll event effects",
                super::interaction::MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        cursor += 1;
    }
    *scroll_commands = pending;
    Ok(())
}

fn host_event_metadata(
    node_index: u32,
    kind: NativeEventKind,
    submitter_node_index: Option<u32>,
) -> NativeHostEvent {
    let (event_type, bubbles, cancelable) = match kind {
        NativeEventKind::Blur => ("blur", false, false),
        NativeEventKind::FocusOut => ("focusout", true, false),
        NativeEventKind::Focus => ("focus", false, false),
        NativeEventKind::FocusIn => ("focusin", true, false),
        NativeEventKind::ReadyStateChange => ("readystatechange", false, false),
        NativeEventKind::DomContentLoaded => ("DOMContentLoaded", false, false),
        NativeEventKind::Load => ("load", false, false),
        NativeEventKind::Error => ("error", false, false),
        NativeEventKind::PageHide => ("pagehide", false, false),
        NativeEventKind::Unload => ("unload", false, false),
        NativeEventKind::PageShow => ("pageshow", false, false),
        NativeEventKind::BeforeUnload => ("beforeunload", false, true),
        NativeEventKind::HashChange => ("hashchange", false, false),
        NativeEventKind::PopState => ("popstate", false, false),
        NativeEventKind::Invalid => ("invalid", false, true),
        NativeEventKind::KeyDown => ("keydown", true, true),
        NativeEventKind::KeyUp => ("keyup", true, false),
        NativeEventKind::Submit => ("submit", true, true),
        NativeEventKind::Reset => ("reset", true, true),
        NativeEventKind::Click => ("click", true, true),
        NativeEventKind::MouseOver => ("mouseover", true, true),
        NativeEventKind::MouseEnter => ("mouseenter", false, false),
        NativeEventKind::DragStart => ("dragstart", true, true),
        NativeEventKind::DragEnter => ("dragenter", true, true),
        NativeEventKind::DragOver => ("dragover", true, true),
        NativeEventKind::Drop => ("drop", true, true),
        NativeEventKind::DragEnd => ("dragend", true, false),
        NativeEventKind::Input => ("input", true, false),
        NativeEventKind::Change => ("change", true, false),
        NativeEventKind::Scroll => ("scroll", false, false),
    };
    NativeHostEvent {
        node_index,
        event_type: event_type.into(),
        bubbles,
        cancelable,
        persisted: false,
        key: None,
        code: None,
        alt_key: false,
        ctrl_key: false,
        meta_key: false,
        shift_key: false,
        submitter_node_index,
        related_target_node_index: None,
    }
}

/// Build structured metadata used to deliver Rust-owned semantic events into
/// the persistent page realm.
pub(crate) fn host_event_batch(
    events: &[(u32, NativeEventKind)],
) -> Result<Option<Vec<NativeHostEvent>>, NativeEngineError> {
    if events.is_empty() {
        return Ok(None);
    }
    let mut metadata = events
        .iter()
        .map(|(node_index, kind)| host_event_metadata(*node_index, *kind, None))
        .collect::<Vec<_>>();
    for (index, (_, kind)) in events.iter().enumerate() {
        if let Some(related_target_node_index) = focus_related_target_index(events, index, *kind) {
            metadata[index].related_target_node_index = Some(related_target_node_index);
        }
    }
    Ok(Some(metadata))
}

fn focus_related_target_index(
    events: &[(u32, NativeEventKind)],
    index: usize,
    kind: NativeEventKind,
) -> Option<u32> {
    match kind {
        NativeEventKind::Blur | NativeEventKind::FocusOut => events
            .iter()
            .skip(index + 1)
            .find(|(_, candidate)| {
                matches!(candidate, NativeEventKind::Focus | NativeEventKind::FocusIn)
            })
            .map(|(node_index, _)| *node_index),
        NativeEventKind::Focus | NativeEventKind::FocusIn => events[..index]
            .iter()
            .rev()
            .find(|(_, candidate)| {
                matches!(candidate, NativeEventKind::Blur | NativeEventKind::FocusOut)
            })
            .map(|(node_index, _)| *node_index),
        _ => None,
    }
}

pub(crate) fn host_event_batch_at(
    events: &[(u32, NativeEventKind)],
    index: usize,
) -> Result<Option<Vec<NativeHostEvent>>, NativeEngineError> {
    let (node_index, kind) = events.get(index).copied().ok_or_else(|| {
        NativeEngineError::invalid(
            "native host event index",
            "must identify an event in the transition batch",
        )
    })?;
    let mut event = host_event_metadata(node_index, kind, None);
    event.related_target_node_index = focus_related_target_index(events, index, kind);
    Ok(Some(vec![event]))
}

pub(crate) fn host_click_event_batch_with_modifiers(
    node_index: u32,
    modifiers: u8,
) -> Result<Vec<NativeHostEvent>, NativeEngineError> {
    if modifiers > 15 {
        return Err(NativeEngineError::invalid(
            "native click modifiers",
            "must be a bit mask from 0 through 15",
        ));
    }
    let mut events = host_event_batch(&[(node_index, NativeEventKind::Click)])?
        .expect("a single click always produces an event batch");
    let event = events
        .first_mut()
        .expect("a single click always produces one host event");
    event.alt_key = modifiers & 1 != 0;
    event.ctrl_key = modifiers & 2 != 0;
    event.meta_key = modifiers & 4 != 0;
    event.shift_key = modifiers & 8 != 0;
    Ok(events)
}

pub(crate) fn host_submit_event_batch(
    form_index: u32,
    submitter_index: Option<u32>,
) -> Result<Option<Vec<NativeHostEvent>>, NativeEngineError> {
    Ok(Some(vec![host_event_metadata(
        form_index,
        NativeEventKind::Submit,
        submitter_index,
    )]))
}

pub(crate) fn host_key_event_batch_with_modifiers(
    node_index: u32,
    kind: NativeEventKind,
    key: &str,
    modifiers: i64,
) -> Result<Option<Vec<NativeHostEvent>>, NativeEngineError> {
    let (event_type, bubbles, cancelable) = match kind {
        NativeEventKind::KeyDown => ("keydown", true, true),
        NativeEventKind::KeyUp => ("keyup", true, false),
        _ => {
            return Err(NativeEngineError::invalid(
                "native key event",
                "key event dispatch requires keydown or keyup",
            ));
        }
    };
    validate_native_key(key)?;
    if !(0..=15).contains(&modifiers) {
        return Err(NativeEngineError::invalid(
            "native key event modifiers",
            "must be an integer mask from 0 through 15",
        ));
    }
    let code = match key {
        " " => "Space".to_owned(),
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Enter" | "Tab" | "Escape"
        | "Backspace" | "Delete" | "Home" | "End" | "PageUp" | "PageDown" => key.to_owned(),
        _ if key.chars().count() == 1 => {
            let character = key.chars().next().expect("single-character key");
            if character.is_ascii_alphabetic() {
                format!("Key{}", character.to_ascii_uppercase())
            } else if character.is_ascii_digit() {
                format!("Digit{character}")
            } else {
                key.to_owned()
            }
        }
        _ => key.to_owned(),
    };
    Ok(Some(vec![NativeHostEvent {
        node_index,
        event_type: event_type.into(),
        bubbles,
        cancelable,
        persisted: false,
        key: Some(key.to_owned()),
        code: Some(code),
        alt_key: modifiers & 1 != 0,
        ctrl_key: modifiers & 2 != 0,
        meta_key: modifiers & 4 != 0,
        shift_key: modifiers & 8 != 0,
        submitter_node_index: None,
        related_target_node_index: None,
    }]))
}

pub(crate) fn host_key_event_batch(
    node_index: u32,
    kind: NativeEventKind,
    key: &str,
) -> Result<Option<Vec<NativeHostEvent>>, NativeEngineError> {
    host_key_event_batch_with_modifiers(node_index, kind, key, 0)
}

fn bounded_unhandled_promise_rejection_reason(reason: String) -> String {
    reason
        .chars()
        .take(MAX_NATIVE_UNHANDLED_REJECTION_REASON_CHARS)
        .collect()
}

/// Build the structured payload used to project events from a child browsing
/// context into its same-origin parent realm. The child node identity remains
/// data; the receiving realm resolves it against its own immutable binding
/// snapshot before dispatching the event.
pub(crate) fn frame_event_batch(
    frame_id: &str,
    events: &[(u32, u32, NativeEventKind)],
) -> Result<Option<NativeFrameEventBatch>, NativeEngineError> {
    if events.is_empty() {
        return Ok(None);
    }
    validate_context_id(frame_id)?;
    if events.len() > super::interaction::MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native frame event dispatch",
            super::interaction::MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mut metadata = events
        .iter()
        .map(|(node_index, generation, kind)| {
            let (event_type, bubbles, cancelable) = match kind {
                NativeEventKind::Blur => ("blur", false, false),
                NativeEventKind::FocusOut => ("focusout", true, false),
                NativeEventKind::Focus => ("focus", false, false),
                NativeEventKind::FocusIn => ("focusin", true, false),
                NativeEventKind::ReadyStateChange => ("readystatechange", false, false),
                NativeEventKind::DomContentLoaded => ("DOMContentLoaded", false, false),
                NativeEventKind::Load => ("load", false, false),
                NativeEventKind::Error => ("error", false, false),
                NativeEventKind::PageHide => ("pagehide", false, false),
                NativeEventKind::Unload => ("unload", false, false),
                NativeEventKind::PageShow => ("pageshow", false, false),
                NativeEventKind::BeforeUnload => ("beforeunload", false, true),
                NativeEventKind::HashChange => ("hashchange", false, false),
                NativeEventKind::PopState => ("popstate", false, false),
                NativeEventKind::Invalid => ("invalid", false, true),
                NativeEventKind::KeyDown => ("keydown", true, true),
                NativeEventKind::KeyUp => ("keyup", true, false),
                NativeEventKind::Submit => ("submit", true, true),
                NativeEventKind::Reset => ("reset", true, true),
                NativeEventKind::Click => ("click", true, true),
                NativeEventKind::MouseOver => ("mouseover", true, true),
                NativeEventKind::MouseEnter => ("mouseenter", false, false),
                NativeEventKind::DragStart => ("dragstart", true, true),
                NativeEventKind::DragEnter => ("dragenter", true, true),
                NativeEventKind::DragOver => ("dragover", true, true),
                NativeEventKind::Drop => ("drop", true, true),
                NativeEventKind::DragEnd => ("dragend", true, false),
                NativeEventKind::Input => ("input", true, false),
                NativeEventKind::Change => ("change", true, false),
                NativeEventKind::Scroll => ("scroll", false, false),
            };
            NativeFrameEvent {
                node_index: *node_index,
                generation: *generation,
                event_type: event_type.into(),
                bubbles,
                cancelable,
                persisted: false,
                related_target_node_index: None,
            }
        })
        .collect::<Vec<_>>();
    for (index, (_, _, kind)) in events.iter().enumerate() {
        let related_target_node_index = match kind {
            NativeEventKind::Blur | NativeEventKind::FocusOut => events
                .iter()
                .skip(index + 1)
                .find(|(_, _, candidate)| {
                    matches!(candidate, NativeEventKind::Focus | NativeEventKind::FocusIn)
                })
                .map(|(node_index, _, _)| *node_index),
            NativeEventKind::Focus | NativeEventKind::FocusIn => events[..index]
                .iter()
                .rev()
                .find(|(_, _, candidate)| {
                    matches!(candidate, NativeEventKind::Blur | NativeEventKind::FocusOut)
                })
                .map(|(node_index, _, _)| *node_index),
            _ => None,
        };
        metadata[index].related_target_node_index = related_target_node_index;
    }
    Ok(Some(NativeFrameEventBatch {
        frame_id: frame_id.to_owned(),
        events: metadata,
    }))
}

pub(crate) fn validate_frame_script_command(
    command: &NativeScriptCommand,
) -> Result<(), NativeEngineError> {
    let supported = match command {
        NativeScriptCommand::FrameScriptBatch { commands } => {
            if commands.is_empty() || commands.len() > super::interaction::MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "same-origin frame script batch",
                    super::interaction::MAX_NATIVE_EFFECTS,
                    commands.len(),
                ));
            }
            commands.iter().all(is_frame_script_batch_command)
        }
        _ => matches!(
            command,
            NativeScriptCommand::Focus { .. }
                | NativeScriptCommand::Blur { .. }
                | NativeScriptCommand::Click { .. }
                | NativeScriptCommand::ClearFileInput { .. }
                | NativeScriptCommand::SetValue { .. }
                | NativeScriptCommand::SetSelection { .. }
                | NativeScriptCommand::CanvasCommit { .. }
                | NativeScriptCommand::MediaLoad { .. }
                | NativeScriptCommand::MediaFrameTime { .. }
                | NativeScriptCommand::SetChecked { .. }
                | NativeScriptCommand::SetIndeterminate { .. }
                | NativeScriptCommand::SetSelected { .. }
                | NativeScriptCommand::SetAttribute { .. }
                | NativeScriptCommand::RemoveAttribute { .. }
                | NativeScriptCommand::SetTextContent { .. }
                | NativeScriptCommand::SetDocumentTitle { .. }
                | NativeScriptCommand::SetInnerHtml { .. }
                | NativeScriptCommand::BindInnerHtmlNodeIndexes { .. }
                | NativeScriptCommand::RemoveNode { .. }
                | NativeScriptCommand::CreateElement { .. }
                | NativeScriptCommand::CreateTextNode { .. }
                | NativeScriptCommand::CreateComment { .. }
                | NativeScriptCommand::CreateDocumentType { .. }
                | NativeScriptCommand::AppendChild { .. }
                | NativeScriptCommand::InsertBefore { .. }
                | NativeScriptCommand::SetCustomValidity { .. }
                | NativeScriptCommand::SetFormAssociatedDisabledState { .. }
                | NativeScriptCommand::CheckValidity { .. }
                | NativeScriptCommand::ReportValidity { .. }
                | NativeScriptCommand::ResetForm { .. }
        ),
    };
    if !supported {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "same-origin frame script command is not a DOM operation".into(),
        });
    }
    let encoded = serde_json::to_vec(command).map_err(|_| NativeEngineError::Worker {
        operation: "serialize same-origin frame command".into(),
        reason: "same-origin frame command could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_SCRIPT_COMMAND_BYTES {
        return Err(NativeEngineError::limit(
            "same-origin frame command",
            MAX_NATIVE_SCRIPT_COMMAND_BYTES,
            encoded.len(),
        ));
    }
    Ok(())
}

fn is_frame_script_batch_command(command: &NativeScriptCommand) -> bool {
    matches!(
        command,
        NativeScriptCommand::SetValue { .. }
            | NativeScriptCommand::ClearFileInput { .. }
            | NativeScriptCommand::SetSelection { .. }
            | NativeScriptCommand::CanvasCommit { .. }
            | NativeScriptCommand::MediaLoad { .. }
            | NativeScriptCommand::MediaFrameTime { .. }
            | NativeScriptCommand::SetChecked { .. }
            | NativeScriptCommand::SetIndeterminate { .. }
            | NativeScriptCommand::SetSelected { .. }
            | NativeScriptCommand::SetAttribute { .. }
            | NativeScriptCommand::RemoveAttribute { .. }
            | NativeScriptCommand::SetTextContent { .. }
            | NativeScriptCommand::SetDocumentTitle { .. }
            | NativeScriptCommand::SetInnerHtml { .. }
            | NativeScriptCommand::BindInnerHtmlNodeIndexes { .. }
            | NativeScriptCommand::RemoveNode { .. }
            | NativeScriptCommand::CreateElement { .. }
            | NativeScriptCommand::CreateTextNode { .. }
            | NativeScriptCommand::CreateComment { .. }
            | NativeScriptCommand::CreateDocumentType { .. }
            | NativeScriptCommand::AppendChild { .. }
            | NativeScriptCommand::InsertBefore { .. }
            | NativeScriptCommand::SetCustomValidity { .. }
            | NativeScriptCommand::SetFormAssociatedDisabledState { .. }
    )
}
/// One persistent ECMAScript realm. A full navigation creates a new value;
/// same-document navigation retains it, matching a page global object's
/// lifetime.
#[derive(Default)]
struct NativeUnhandledPromiseRejections {
    pending: BTreeMap<String, (u64, String)>,
    reported: BTreeMap<String, (u64, String)>,
    handled: BTreeMap<String, (u64, String)>,
    next_order: u64,
}

/// A QuickJS handle owned by one `NativeJavaScriptRuntime`. The upstream
/// handle is intentionally not `Send`, although its runtime is movable.
/// This wrapper permits that same exclusive owner transfer.
struct PersistentHostViewInstaller(Persistent<Function<'static>>);

// SAFETY: The handle is private to NativeJavaScriptRuntime, whose QuickJS
// context is entered under exclusive ownership. Runtime moves with the
// handle, the mutex serializes access, and Drop releases it before Runtime.
unsafe impl Send for PersistentHostViewInstaller {}

pub(crate) struct NativeJavaScriptRuntime {
    runtime: Runtime,
    context: Context,
    host_view_installer: Mutex<Option<PersistentHostViewInstaller>>,
    host_view_cache_key: Mutex<Option<NativeHostViewCacheKey>>,
    host_view_snapshot: Mutex<Option<serde_json::Value>>,
    deadline: Arc<Mutex<Option<Instant>>>,
    environment: Arc<Mutex<NativeEnvironmentOverrides>>,
    module_sources: Arc<Mutex<BTreeMap<String, String>>>,
    module_base_urls: Arc<Mutex<BTreeMap<String, String>>>,
    module_referrer_policies: Arc<Mutex<BTreeMap<String, NativeFetchReferrerPolicy>>>,
    worker_global_referrer_policy: Option<NativeFetchReferrerPolicy>,
    module_import_map: Arc<Mutex<NativeModuleImportMap>>,
    dynamic_module_aliases: Arc<Mutex<BTreeMap<String, String>>>,
    dynamic_import_mode: Arc<Mutex<NativeModuleDynamicImportMode>>,
    dynamic_import_referrers: Arc<Mutex<BTreeSet<String>>>,
    sync_xhr_loader: Arc<Mutex<Option<NativeResourceLoader>>>,
    sync_xhr_loader_used: Arc<AtomicBool>,
    timer_pump_enabled: Arc<Mutex<bool>>,
    storage: Arc<Mutex<NativeWebStorageState>>,
    indexed_db: Arc<Mutex<NativeIndexedDbOrigin>>,
    storage_changes: Arc<Mutex<Vec<NativeStorageEvent>>>,
    pending_storage_events: Arc<Mutex<Vec<NativeStorageEvent>>>,
    cookie: Arc<Mutex<String>>,
    cookie_updates: Arc<Mutex<Vec<String>>>,
    parent_brokered_cookie_updates: Arc<Mutex<VecDeque<String>>>,
    unhandled_promise_rejections: Arc<Mutex<NativeUnhandledPromiseRejections>>,
    dialog_events: Arc<Mutex<Vec<NativeDialog>>>,
    dialog_handler: Arc<Mutex<Option<NativeDialogHandler>>>,
    dialog_url: Arc<Mutex<String>>,
    popup_events: Arc<Mutex<Vec<NativePopupRequest>>>,
    post_message_events: Arc<Mutex<Vec<NativePostMessageRequest>>>,
    window_close_events: Arc<Mutex<Vec<NativeWindowCloseRequest>>>,
    window_navigation_events: Arc<Mutex<Vec<NativeWindowNavigationRequest>>>,
    frame_script_events: Arc<Mutex<Vec<NativeFrameScriptRequest>>>,
    worker_commands: Arc<Mutex<Vec<NativeScriptCommand>>>,
    message_port_commands: Arc<Mutex<Vec<NativeScriptCommand>>>,
    service_worker_commands: Arc<Mutex<Vec<NativeScriptCommand>>>,
    document_creation_url_state: Arc<Mutex<Option<(u32, String)>>>,
    service_worker_registrations: Arc<Mutex<Vec<NativeServiceWorkerRegistrationState>>>,
    service_worker_clients: Arc<Mutex<Vec<serde_json::Value>>>,
    inline_script_policy: Arc<Mutex<NativeInlineScriptPolicy>>,
    processed_inline_csp_meta_nodes: Arc<Mutex<BTreeSet<u32>>>,
    pending_window_proxy_updates: Arc<Mutex<Vec<NativeWindowProxyUpdate>>>,
    frame_script_bindings: Arc<Mutex<Vec<NativeFrameScriptBinding>>>,
    frame_script_context: Arc<Mutex<Option<NativeFrameScriptContext>>>,
    frame_id: Arc<Mutex<String>>,
    scroll_offset: Arc<Mutex<NativePoint>>,
    nested_scroll_offsets: Arc<Mutex<BTreeMap<u32, NativePoint>>>,
    history_state: Arc<Mutex<serde_json::Value>>,
    history_length: Arc<Mutex<usize>>,
    window_name: Arc<Mutex<String>>,
    opener_context_id: Option<String>,
    opener_window_name: String,
    opener_url: String,
    storage_context_id: String,
    ready_state: String,
    clock_origin: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeHostViewCacheKey {
    document_address: usize,
    generation: u32,
    revision: u64,
    snapshot_generation: u64,
    viewport: Viewport,
    include_layout: bool,
    scroll_offset: NativePoint,
    nested_scroll_offsets: BTreeMap<u32, NativePoint>,
}

impl NativeHostViewCacheKey {
    fn new(
        document: &NativeDocument,
        viewport: Viewport,
        include_layout: bool,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) -> Self {
        Self {
            document_address: std::ptr::from_ref(document) as usize,
            generation: document.generation(),
            revision: document.revision(),
            snapshot_generation: document.script_snapshot_generation(),
            viewport,
            include_layout,
            scroll_offset,
            nested_scroll_offsets: nested_scroll_offsets.clone(),
        }
    }
}

impl Drop for NativeJavaScriptRuntime {
    fn drop(&mut self) {
        // A Persistent QuickJS handle owns a JS value reference and must be
        // released while its runtime is still alive. Rust drops fields in
        // declaration order, so explicitly empty this cache before runtime.
        if let Ok(cached) = self.host_view_installer.get_mut() {
            cached.take();
        }
    }
}

pub(crate) struct NativeTimerPumpGuard<'a> {
    runtime: &'a NativeJavaScriptRuntime,
    previous: bool,
}

impl Drop for NativeTimerPumpGuard<'_> {
    fn drop(&mut self) {
        self.runtime.set_timer_pump_enabled(self.previous);
    }
}

mod module_analysis;
#[cfg(test)]
#[path = "javascript/tests/native_static_dynamic_import_tests.rs"]
mod native_static_dynamic_import_tests;
mod page_events;
mod page_scripts;
use page_events::*;
pub use page_events::{validate_message_port_transfers, validate_page_message_port_command};
pub(crate) use page_scripts::{
    NativeDynamicPageScriptRequest, NativeInlinePageScriptRequest, NativeLocalModuleGraphRequest,
    execute_dynamic_page_scripts, execute_inline_scripts, execute_page_scripts,
    load_local_file_dynamic_module_graph_with_import_map,
    load_local_file_module_graph_with_import_map, resolve_module_request_url,
};
mod runtime;
use module_analysis::*;
pub(crate) use module_analysis::{static_module_requests, static_worker_import_specifiers};
pub(crate) use runtime::NativePageEventContext;
pub(crate) use runtime::NativeServiceWorkerMessageEvent;
pub(crate) use runtime::NativeWorkerModuleEvaluation;
mod worker_registry;
pub(crate) use worker_registry::{load_service_worker_entry, load_service_worker_source};

#[path = "javascript/service_worker_commands.rs"]
mod service_worker_commands;
use service_worker_commands::{
    is_service_worker_cache_command, is_service_worker_lifecycle_command,
    is_service_worker_open_window_command, validate_service_worker_cache_command,
    validate_service_worker_lifecycle_command, validate_service_worker_open_window_command,
};

#[path = "javascript/message_protocol.rs"]
mod message_protocol;
pub(crate) use message_protocol::{
    validate_native_message_payload, validate_native_object_url_transfers,
};
#[cfg(test)]
#[path = "javascript/tests/native_aes_tests.rs"]
mod native_aes_tests;
#[cfg(test)]
#[path = "javascript/tests/native_ed25519_tests.rs"]
mod native_ed25519_tests;
use message_protocol::*;
fn validate_page_event_batch(events: &NativePageEventBatch) -> Result<(), NativeEngineError> {
    if events.csp_violations.len() > MAX_NATIVE_CSP_VIOLATIONS {
        return Err(NativeEngineError::limit(
            "native CSP violation event batch",
            MAX_NATIVE_CSP_VIOLATIONS,
            events.csp_violations.len(),
        ));
    }
    for (operation, count) in [
        ("native Worker event batch", events.worker_messages.len()),
        (
            "native page MessagePort event batch",
            events.message_port_messages.len(),
        ),
        (
            "native service-worker client event batch",
            events.service_worker_client_messages.len(),
        ),
        (
            "native page message event batch",
            events.post_message_events.len(),
        ),
        ("native frame event batch", events.frame_event_batches.len()),
        (
            "native frame script command batch",
            events.frame_script_commands.len(),
        ),
        (
            "native hashchange event batch",
            events.hash_change_events.len(),
        ),
        ("native host event batch", events.host_events.len()),
        (
            "native CSP violation event batch",
            events.csp_violations.len(),
        ),
    ] {
        if count > MAX_NATIVE_WORKER_MESSAGES {
            return Err(NativeEngineError::limit(
                operation,
                MAX_NATIVE_WORKER_MESSAGES,
                count,
            ));
        }
    }
    for message in &events.worker_messages {
        if message.worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "native Worker message id",
                "must be positive",
            ));
        }
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &message.data,
                "error": &message.error,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native Worker event",
        )?;
        validate_native_object_url_transfers(&message.object_urls)?;
    }
    for message in &events.message_port_messages {
        validate_url_text("native MessagePort bridge key", &message.bridge_key)?;
        if message.bridge_key.is_empty() {
            return Err(NativeEngineError::invalid(
                "native MessagePort bridge key",
                "must not be empty",
            ));
        }
        if message.bridge_key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native MessagePort bridge key",
                crate::limits::MAX_BACKEND_ID_BYTES,
                message.bridge_key.len(),
            ));
        }
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_message_payload(
            &serde_json::json!({
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native page MessagePort event",
        )?;
        validate_native_object_url_transfers(&message.object_urls)?;
    }
    for message in &events.service_worker_client_messages {
        if message.worker_id == 0 {
            return Err(NativeEngineError::invalid(
                "native service-worker client message worker id",
                "must be positive",
            ));
        }
        if message.client_id.is_empty() {
            return Err(NativeEngineError::invalid(
                "native service-worker client message id",
                "must not be empty",
            ));
        }
        if message.client_id.len() > crate::limits::MAX_BACKEND_ID_BYTES {
            return Err(NativeEngineError::limit(
                "native service-worker client message id",
                crate::limits::MAX_BACKEND_ID_BYTES,
                message.client_id.len(),
            ));
        }
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_object_url_transfers(&message.object_urls)?;
        validate_native_message_payload(
            &serde_json::json!({
                "worker_id": message.worker_id,
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native service-worker client event",
        )?;
    }
    for message in &events.post_message_events {
        validate_context_id(&message.source_context_id)?;
        validate_url_text("native message source origin", &message.source_origin)?;
        validate_message_port_transfers(&message.transfer_ports)?;
        validate_native_message_payload(
            &serde_json::json!({
                "source_context_id": &message.source_context_id,
                "source_origin": &message.source_origin,
                "data": &message.data,
                "transfer_ports": &message.transfer_ports,
                "object_urls": &message.object_urls,
            }),
            "native page message event",
        )?;
        validate_native_object_url_transfers(&message.object_urls)?;
    }
    for batch in &events.frame_event_batches {
        validate_context_id(&batch.frame_id)?;
        if batch.events.is_empty() {
            return Err(NativeEngineError::invalid(
                "native frame event batch",
                "must contain at least one event",
            ));
        }
        if batch.events.len() > super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native frame events",
                super::interaction::MAX_NATIVE_EFFECTS,
                batch.events.len(),
            ));
        }
        for event in &batch.events {
            if !matches!(
                event.event_type.as_str(),
                "blur"
                    | "focusout"
                    | "focus"
                    | "focusin"
                    | "readystatechange"
                    | "DOMContentLoaded"
                    | "load"
                    | "error"
                    | "pagehide"
                    | "unload"
                    | "pageshow"
                    | "beforeunload"
                    | "hashchange"
                    | "popstate"
                    | "invalid"
                    | "keydown"
                    | "keyup"
                    | "submit"
                    | "reset"
                    | "click"
                    | "mouseover"
                    | "mouseenter"
                    | "dragstart"
                    | "dragenter"
                    | "dragover"
                    | "drop"
                    | "dragend"
                    | "input"
                    | "change"
                    | "scroll"
            ) {
                return Err(NativeEngineError::invalid(
                    "native frame event type",
                    "must be a supported native event",
                ));
            }
            if event.related_target_node_index.is_some()
                && !matches!(
                    event.event_type.as_str(),
                    "blur" | "focusout" | "focus" | "focusin"
                )
            {
                return Err(NativeEngineError::invalid(
                    "native frame event related target",
                    "is supported only for focus events",
                ));
            }
        }
    }
    for command in &events.frame_script_commands {
        validate_frame_script_command(command)?;
    }
    for event in &events.hash_change_events {
        validate_url_text("native hashchange old URL", &event.old_url)?;
        validate_url_text("native hashchange new URL", &event.new_url)?;
        if without_fragment(&event.old_url) != without_fragment(&event.new_url)
            || event.old_url == event.new_url
        {
            return Err(NativeEngineError::invalid(
                "native hashchange URLs",
                "must differ only by a non-empty fragment",
            ));
        }
    }
    for event in &events.host_events {
        if !matches!(
            event.event_type.as_str(),
            "blur"
                | "focusout"
                | "focus"
                | "focusin"
                | "readystatechange"
                | "DOMContentLoaded"
                | "load"
                | "error"
                | "pagehide"
                | "unload"
                | "pageshow"
                | "beforeunload"
                | "hashchange"
                | "popstate"
                | "invalid"
                | "keydown"
                | "keyup"
                | "submit"
                | "reset"
                | "click"
                | "mouseover"
                | "mouseenter"
                | "dragstart"
                | "dragenter"
                | "dragover"
                | "drop"
                | "dragend"
                | "input"
                | "change"
                | "scroll"
        ) {
            return Err(NativeEngineError::invalid(
                "native host event type",
                "must be a supported native event",
            ));
        }
        let is_key_event = matches!(event.event_type.as_str(), "keydown" | "keyup");
        if is_key_event {
            let key = event.key.as_deref().ok_or_else(|| {
                NativeEngineError::invalid(
                    "native host key event",
                    "must include a printable key value",
                )
            })?;
            validate_native_key(key)?;
            if let Some(code) = event.code.as_deref() {
                validate_native_key(code)?;
            }
        } else if event.key.is_some()
            || event.code.is_some()
            || (event.event_type != "click"
                && (event.alt_key || event.ctrl_key || event.meta_key || event.shift_key))
        {
            return Err(NativeEngineError::invalid(
                "native host event modifier metadata",
                "modifiers require a keydown, keyup, or click event",
            ));
        }
        if event.submitter_node_index.is_some() && event.event_type != "submit" {
            return Err(NativeEngineError::invalid(
                "native host event submitter",
                "submitter metadata requires a submit event",
            ));
        }
        if event.related_target_node_index.is_some()
            && !matches!(
                event.event_type.as_str(),
                "blur" | "focusout" | "focus" | "focusin"
            )
        {
            return Err(NativeEngineError::invalid(
                "native host event related target",
                "is supported only for focus events",
            ));
        }
    }
    for violation in &events.csp_violations {
        validate_url_text("native CSP violation document URL", &violation.document_uri)?;
        if !violation.blocked_uri.is_empty() && violation.blocked_uri != "inline" {
            validate_url_text("native CSP violation blocked URL", &violation.blocked_uri)?;
        }
        if violation.document_uri.len() > crate::limits::MAX_BACKEND_ID_BYTES
            || violation.blocked_uri.len() > crate::limits::MAX_BACKEND_ID_BYTES
            || violation.original_policy.len()
                > super::resource_loader::MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES
            || violation.source_file.len() > crate::limits::MAX_BACKEND_ID_BYTES
            || violation.sample.len() > 160
        {
            return Err(NativeEngineError::limit(
                "native CSP violation metadata",
                crate::limits::MAX_BACKEND_ID_BYTES,
                violation
                    .document_uri
                    .len()
                    .max(violation.blocked_uri.len())
                    .max(violation.original_policy.len())
                    .max(violation.source_file.len())
                    .max(violation.sample.len()),
            ));
        }
        if !matches!(violation.disposition.as_str(), "enforce" | "report") {
            return Err(NativeEngineError::invalid(
                "native CSP violation disposition",
                "must be enforce or report",
            ));
        }
        if violation.effective_directive.is_empty()
            || violation.effective_directive != violation.violated_directive
        {
            return Err(NativeEngineError::invalid(
                "native CSP violation directive",
                "effective and violated directives must be equal and non-empty",
            ));
        }
    }
    Ok(())
}

fn read_indexed_db_state<'js>(
    ctx: rquickjs::Ctx<'js>,
) -> Result<NativeIndexedDbOrigin, NativeEngineError> {
    let json: String = ctx
        .eval("JSON.stringify(globalThis.__glassIndexedDbState || {databases:{}})")
        .map_err(|_| NativeEngineError::Worker {
            operation: "collect JavaScript IndexedDB state".into(),
            reason: "native IndexedDB state could not be collected".into(),
        })?;
    if json.len() > MAX_NATIVE_INDEXED_DB_STATE_BYTES {
        return Err(NativeEngineError::limit(
            "native IndexedDB state",
            MAX_NATIVE_INDEXED_DB_STATE_BYTES,
            json.len(),
        ));
    }
    let state: NativeIndexedDbOrigin =
        serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
            operation: "decode JavaScript IndexedDB state".into(),
            reason: "native IndexedDB state was malformed".into(),
        })?;
    state.validate()?;
    Ok(state)
}

fn contains_await_token(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index = index.saturating_add(2);
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || bytes[index] == b'_'
                        || bytes[index] == b'$')
                {
                    index += 1;
                }
                if &bytes[start..index] == b"await" {
                    return true;
                }
            }
            _ => index += 1,
        }
    }
    false
}

pub(crate) fn storage_key(document_url: &str, origin: &NativeOrigin) -> String {
    if matches!(origin, NativeOrigin::Opaque) {
        return format!(
            "opaque:{}",
            document_url
                .split_once('#')
                .map_or(document_url, |(url, _)| url)
        );
    }
    origin.serialized()
}

fn dispatch_window_proxy_updates(
    ctx: &rquickjs::Ctx<'_>,
    updates: &[NativeWindowProxyUpdate],
) -> Result<(), NativeEngineError> {
    if updates.is_empty() {
        return Ok(());
    }
    let payload = serde_json::to_value(updates).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native WindowProxy updates".into(),
        reason: "native WindowProxy updates could not be serialized".into(),
    })?;
    let payload = native_structured_payload(ctx, &payload, "native WindowProxy updates")?;
    let dispatch: Function = ctx
        .globals()
        .get("__glassSyncWindowProxies")
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native WindowProxy updates".into(),
            reason: format!(
                "native WindowProxy dispatcher was unavailable: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    dispatch
        .call::<_, Value>((payload,))
        .map_err(|error| NativeEngineError::Worker {
            operation: "dispatch native WindowProxy updates".into(),
            reason: format!(
                "native WindowProxy update dispatch failed: {}",
                CaughtError::from_error(ctx, error)
            ),
        })?;
    Ok(())
}

#[cfg(test)]
#[path = "javascript/tests/native_adopted_frame_command_tests.rs"]
mod native_adopted_frame_command_tests;
#[cfg(test)]
#[path = "javascript/tests/native_dialog_tests.rs"]
mod native_dialog_tests;
#[cfg(test)]
#[path = "javascript/tests/native_font_face_tests.rs"]
mod native_font_face_tests;
#[cfg(test)]
#[path = "javascript/tests/native_host_event_tests.rs"]
mod native_host_event_tests;
#[cfg(test)]
#[path = "javascript/tests/native_large_host_view_tests.rs"]
mod native_large_host_view_tests;
#[cfg(test)]
#[path = "javascript/tests/native_selector_tests.rs"]
mod native_selector_tests;
#[cfg(test)]
#[path = "javascript/tests/native_service_worker_client_tests.rs"]
mod native_service_worker_client_tests;
#[cfg(test)]
#[path = "javascript/tests/native_shared_element_methods_tests.rs"]
mod native_shared_element_methods_tests;
#[cfg(test)]
#[path = "javascript/tests/native_timer_probe_tests.rs"]
mod native_timer_probe_tests;
#[cfg(test)]
#[path = "javascript/tests/native_window_proxy_tests.rs"]
mod native_window_proxy_tests;
#[cfg(test)]
#[path = "javascript/tests/service_worker_message_event_tests.rs"]
mod service_worker_message_event_tests;

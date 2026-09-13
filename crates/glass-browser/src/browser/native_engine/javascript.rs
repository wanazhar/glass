//! Bounded JavaScript execution for the native browser realm.
//!
//! QuickJS supplies the ECMAScript implementation. Glass owns the host
//! objects and Web APIs, which are added in separate slices so every exposed
//! capability has an explicit resource and security contract.

use super::config::{
    MAX_NATIVE_WINDOW_NAME_BYTES, Viewport, validate_context_id, validate_url_text,
    validate_window_name,
};
use super::dom::{
    NativeDocument, NativePageScriptSource, NativePageScriptTiming, NativeScriptDocumentSnapshot,
    NativeScriptElementSnapshot,
};
use super::error::NativeEngineError;
use super::interaction::{
    MAX_NATIVE_FILE_BYTES, MAX_NATIVE_FORM_BODY_BYTES, MAX_NATIVE_SCRIPT_COMMAND_BYTES,
    NativeEventKind,
};
use super::layout::NativePoint;
use super::origin::NativeOrigin;
use fs2::FileExt;
use rquickjs::function::This;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{CaughtError, Coerced, Context, Error, FromJs, Function, Module, Runtime, Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use url::Url;

/// Maximum source accepted by the native script evaluator.
pub(crate) const MAX_NATIVE_SCRIPT_BYTES: usize = crate::browser_backend::MAX_TEXT_BYTES;
/// Maximum JSON representation returned to the semantic backend.
pub(crate) const MAX_NATIVE_SCRIPT_RESULT_BYTES: usize = crate::browser_backend::MAX_JSON_BYTES;
/// Maximum structured-clone payload accepted by the native `postMessage`
/// bridge. The payload is JSON-backed today, but the limit is kept separate
/// so future transferable values cannot silently enlarge IPC frames.
pub(crate) const MAX_NATIVE_POST_MESSAGE_BYTES: usize = 256 * 1024;
/// Maximum JSON-backed state retained by one History API entry.
pub(crate) const MAX_NATIVE_HISTORY_STATE_BYTES: usize = 256 * 1024;
const MAX_NATIVE_HISTORY_DELTA: i32 = 1024;
/// Maximum frame-window index surface exposed by one script realm.
pub(crate) const MAX_NATIVE_FRAME_SCRIPT_BINDINGS: usize = 64;
/// Maximum inline page scripts executed while committing one document.
pub(crate) const MAX_NATIVE_INLINE_SCRIPTS: usize = 32;
pub(crate) const MAX_NATIVE_MODULE_IMPORTS: usize = 128;
const NATIVE_SCRIPT_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const NATIVE_SCRIPT_STACK_BYTES: usize = 1024 * 1024;
const NATIVE_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_WEB_STORAGE_PROFILE_BYTES: usize = 4 * 1024 * 1024;
const WEB_STORAGE_PROFILE_VERSION: u64 = 1;
const MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES: usize = 4 * 1024 * 1024;
const MAX_NATIVE_STORAGE_EVENTS: usize = 64;
const MAX_NATIVE_STORAGE_READER_LEASES: usize = 128;
const MAX_NATIVE_STORAGE_READER_LEASE_BYTES: usize = 64 * 1024;
const NATIVE_STORAGE_READER_LEASE_VERSION: u64 = 1;
const NATIVE_STORAGE_READER_LEASE_TTL: Duration = Duration::from_secs(15 * 60);
const NATIVE_STORAGE_READER_HEARTBEAT: Duration = Duration::from_secs(30);
pub(crate) const MAX_NATIVE_COOKIE_PROFILE_ENTRIES: usize = 128;
pub(crate) const MAX_NATIVE_COOKIE_PROFILE_BYTES: usize = 4096;
pub(crate) const MAX_NATIVE_DIALOGS: usize = 32;
pub(crate) const MAX_NATIVE_DIALOG_TEXT_BYTES: usize = 256;
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
pub(crate) const MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES: usize = MAX_NATIVE_FORM_BODY_BYTES;
pub(crate) const MAX_NATIVE_WEBSOCKET_PROTOCOLS: usize = 16;
pub(crate) const MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES: usize = 128;
pub(crate) const MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES: usize = 123;
pub(crate) const MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES: usize = MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES;
pub(crate) const MAX_NATIVE_EVENTSOURCE_FIELD_BYTES: usize = 128;
pub(crate) const MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES: usize = 8 * 1024;
pub(crate) const MAX_NATIVE_FETCH_STREAM_BODY_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_NATIVE_FETCH_STREAM_QUEUED_CHUNKS: usize = 32;
const MAX_NATIVE_UNHANDLED_REJECTIONS: usize = 64;
const MAX_NATIVE_UNHANDLED_REJECTION_REASON_CHARS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum NativeScriptCommand {
    Focus {
        node_index: u32,
    },
    Blur {
        node_index: u32,
    },
    Click {
        node_index: u32,
    },
    SubmitForm {
        node_index: u32,
    },
    RequestSubmitForm {
        node_index: u32,
        #[serde(default)]
        submitter_index: Option<u32>,
    },
    Navigate {
        href: String,
        #[serde(default)]
        replace: bool,
    },
    HistoryPushState {
        href: String,
        state: serde_json::Value,
    },
    HistoryReplaceState {
        href: String,
        state: serde_json::Value,
    },
    HistoryGo {
        delta: i32,
    },
    ScrollTo {
        node_index: u32,
        left: i64,
        top: i64,
    },
    OpenWindow {
        href: String,
        target: String,
        #[serde(default)]
        handle: Option<String>,
    },
    SetWindowName {
        value: String,
    },
    CloseWindow {
        target: String,
        #[serde(default)]
        target_context_id: Option<String>,
    },
    NavigateWindow {
        target: String,
        #[serde(default)]
        target_context_id: Option<String>,
        href: String,
        #[serde(default)]
        replace: bool,
    },
    PostMessage {
        target: String,
        target_origin: String,
        data: serde_json::Value,
        #[serde(default)]
        target_context_id: Option<String>,
    },
    Fetch {
        request_id: u32,
        href: String,
        credentials: bool,
        method: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        body_base64: Option<String>,
        #[serde(default)]
        content_type: Option<String>,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        redirect: Option<String>,
        #[serde(default)]
        timeout_ms: Option<u32>,
    },
    WebSocketOpen {
        socket_id: u32,
        href: String,
        #[serde(default)]
        protocols: Vec<String>,
    },
    WebSocketSend {
        socket_id: u32,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        data_base64: Option<String>,
    },
    WebSocketClose {
        socket_id: u32,
        code: u16,
        reason: String,
    },
    EventSourceOpen {
        source_id: u32,
        href: String,
        with_credentials: bool,
    },
    EventSourceClose {
        source_id: u32,
    },
    FetchStreamRead {
        stream_id: u32,
    },
    FetchStreamCancel {
        stream_id: u32,
    },
    Dialog {
        dialog_type: String,
        message: String,
        #[serde(default)]
        default_value: Option<String>,
    },
    StorageSet {
        scope: String,
        key: String,
        value: String,
    },
    StorageRemove {
        scope: String,
        key: String,
    },
    StorageClear {
        scope: String,
    },
    CookieSet {
        value: String,
    },
    SetValue {
        node_index: u32,
        value: String,
    },
    ClearFileInput {
        node_index: u32,
    },
    SetSelection {
        node_index: u32,
        start: usize,
        end: usize,
        direction: String,
    },
    SetChecked {
        node_index: u32,
        checked: bool,
    },
    SetSelected {
        node_index: u32,
        selected: bool,
    },
    SetAttribute {
        node_index: u32,
        name: String,
        value: String,
        #[serde(default)]
        namespace_uri: Option<String>,
    },
    RemoveAttribute {
        node_index: u32,
        name: String,
        #[serde(default)]
        namespace_uri: Option<String>,
    },
    SetTextContent {
        node_index: u32,
        value: String,
    },
    SetDocumentTitle {
        value: String,
    },
    SetInnerHtml {
        node_index: u32,
        value: String,
    },
    RemoveNode {
        node_index: u32,
    },
    CreateElement {
        node_index: u32,
        tag_name: String,
        #[serde(default)]
        namespace_uri: Option<String>,
    },
    CreateTextNode {
        node_index: u32,
        value: String,
    },
    CreateComment {
        node_index: u32,
        value: String,
    },
    CreateDocumentType {
        node_index: u32,
        name: String,
        public_id: String,
        system_id: String,
    },
    AppendChild {
        parent_index: u32,
        child_index: u32,
    },
    InsertBefore {
        parent_index: u32,
        child_index: u32,
        #[serde(default)]
        before_index: Option<u32>,
    },
    /// The JavaScript realm already executed a newly inserted classic inline
    /// script synchronously; the host records the single-shot state without
    /// evaluating it a second time.
    StartScript {
        node_index: u32,
    },
    SetCustomValidity {
        node_index: u32,
        message: String,
    },
    CheckValidity {
        node_index: u32,
    },
    ReportValidity {
        node_index: u32,
    },
    /// A same-origin frame construction batch. Parent projections collect
    /// DOM mutations so the child owner can commit temporary node identities
    /// in one transaction.
    FrameScriptBatch {
        commands: Vec<NativeScriptCommand>,
    },
    /// Apply one ordinary DOM command in a same-origin embedded browsing
    /// context. The command is routed by the owning browser topology after
    /// the caller's script returns; it never transfers a native engine or
    /// JavaScript object across realms.
    FrameScript {
        frame_id: String,
        source_frame_id: String,
        command: Box<NativeScriptCommand>,
    },
}

pub(crate) struct NativeScriptEvaluation {
    pub(crate) value: serde_json::Value,
    pub(crate) commands: Vec<NativeScriptCommand>,
    pub(crate) top_level_await_pending: bool,
}

/// A same-origin DOM operation emitted by one page realm for a different
/// embedded browsing context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeFrameScriptRequest {
    pub(crate) frame_id: String,
    pub(crate) source_frame_id: String,
    pub(crate) command: Box<NativeScriptCommand>,
}

/// A browser-context creation request emitted by `window.open`.
///
/// The JavaScript realm never receives a direct native target handle. The
/// request crosses the engine boundary and is materialized by the target
/// owner, which keeps popup creation and named-context reuse serialized with
/// the rest of the browser topology.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativePopupRequest {
    pub(crate) url: String,
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) handle: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) source_context_id: String,
}

/// A bounded cross-context message emitted by a native page realm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativePostMessageRequest {
    pub(crate) target: String,
    pub(crate) target_origin: String,
    pub(crate) data: serde_json::Value,
    #[serde(default)]
    pub(crate) target_context_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) source_context_id: String,
    #[serde(default, skip_serializing)]
    pub(crate) source_origin: String,
}

/// A bounded request for the parent target owner to close a WindowProxy target.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeWindowCloseRequest {
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) target_context_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) source_context_id: String,
}

/// A bounded request for the parent target owner to navigate a WindowProxy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeWindowNavigationRequest {
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) target_context_id: Option<String>,
    pub(crate) href: String,
    #[serde(default)]
    pub(crate) replace: bool,
    #[serde(default, skip_serializing)]
    pub(crate) source_context_id: String,
}

/// A trusted parent-owned refresh for one cached WindowProxy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeWindowProxyUpdate {
    pub(crate) cache_key: String,
    pub(crate) target_context_id: String,
    pub(crate) href: String,
    pub(crate) name: String,
    pub(crate) closed: bool,
}

/// A parent-owned snapshot for one directly embedded browsing context.
///
/// The child engine remains the authority for navigation and mutation. The
/// snapshot only lets the embedding realm expose the browser's frame object
/// relationships and same-origin read projections without sharing an engine
/// pointer across JavaScript realms.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeFrameScriptBinding {
    pub(crate) node_index: u32,
    pub(crate) frame_id: String,
    pub(crate) url: String,
    pub(crate) origin: String,
    pub(crate) generation: u32,
    pub(crate) revision: u64,
    pub(crate) same_origin: bool,
    pub(crate) document: NativeScriptDocumentSnapshot,
    #[serde(default)]
    pub(crate) children: Vec<NativeFrameScriptBinding>,
}

/// A bounded Window/document snapshot used to install the real parent/top
/// relationships while a child browsing context is selected.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeFrameScriptWindow {
    pub(crate) context_id: String,
    pub(crate) url: String,
    pub(crate) origin: String,
    pub(crate) generation: u32,
    pub(crate) revision: u64,
    pub(crate) same_origin: bool,
    pub(crate) document: NativeScriptDocumentSnapshot,
    #[serde(default)]
    pub(crate) children: Vec<NativeFrameScriptBinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeFrameScriptContext {
    pub(crate) current_frame_id: String,
    pub(crate) parent: Option<NativeFrameScriptWindow>,
    pub(crate) top: Option<NativeFrameScriptWindow>,
    pub(crate) frame_element: Option<NativeScriptElementSnapshot>,
}

#[derive(Debug, Clone)]
pub(crate) struct NativePageNavigation {
    pub(crate) href: String,
    pub(crate) replace_history: bool,
}

#[derive(Default)]
pub(crate) struct NativePageScriptResult {
    pub(crate) pending_fetches: Vec<NativeScriptCommand>,
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

fn default_indexed_db_next_key() -> u64 {
    1
}

impl NativeIndexedDbState {
    pub(crate) fn validate(&self) -> Result<(), NativeEngineError> {
        if self.origins.len() > crate::browser_backend::MAX_STORAGE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native IndexedDB origins",
                crate::browser_backend::MAX_STORAGE_ENTRIES,
                self.origins.len(),
            ));
        }
        for (storage_key, origin) in &self.origins {
            if storage_key.is_empty() || storage_key.len() > crate::browser_backend::MAX_TEXT_BYTES
            {
                return Err(NativeEngineError::limit(
                    "native IndexedDB storage key",
                    crate::browser_backend::MAX_TEXT_BYTES,
                    storage_key.len(),
                ));
            }
            origin.validate()?;
        }
        validate_indexed_db_serialized_size(self)
    }

    pub(crate) fn origin(&self, storage_key: &str) -> NativeIndexedDbOrigin {
        self.origins.get(storage_key).cloned().unwrap_or_default()
    }

    pub(crate) fn replace_origin(
        &mut self,
        storage_key: impl Into<String>,
        origin: NativeIndexedDbOrigin,
    ) -> Result<(), NativeEngineError> {
        origin.validate()?;
        let storage_key = storage_key.into();
        if origin.databases.is_empty() {
            self.origins.remove(&storage_key);
        } else {
            self.origins.insert(storage_key, origin);
        }
        self.validate()
    }
}

impl NativeIndexedDbOrigin {
    fn validate(&self) -> Result<(), NativeEngineError> {
        if self.databases.len() > MAX_NATIVE_INDEXED_DB_DATABASES {
            return Err(NativeEngineError::limit(
                "native IndexedDB databases",
                MAX_NATIVE_INDEXED_DB_DATABASES,
                self.databases.len(),
            ));
        }
        for (name, database) in &self.databases {
            validate_indexed_db_name("native IndexedDB database name", name)?;
            if database.version == 0 {
                return Err(NativeEngineError::invalid(
                    "native IndexedDB database version",
                    "must be greater than zero",
                ));
            }
            if database.stores.len() > MAX_NATIVE_INDEXED_DB_STORES {
                return Err(NativeEngineError::limit(
                    "native IndexedDB object stores",
                    MAX_NATIVE_INDEXED_DB_STORES,
                    database.stores.len(),
                ));
            }
            for (store_name, store) in &database.stores {
                validate_indexed_db_name("native IndexedDB object store name", store_name)?;
                validate_indexed_db_store(store)?;
            }
        }
        Ok(())
    }
}

fn validate_indexed_db_store(store: &NativeIndexedDbObjectStore) -> Result<(), NativeEngineError> {
    if let Some(key_path) = store.key_path.as_deref()
        && key_path.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES
    {
        return Err(NativeEngineError::limit(
            "native IndexedDB key path",
            crate::browser_backend::MAX_BACKEND_ID_BYTES,
            key_path.len(),
        ));
    }
    if store.next_key == 0 {
        return Err(NativeEngineError::invalid(
            "native IndexedDB auto-increment key",
            "must be greater than zero",
        ));
    }
    if store.indexes.len() > MAX_NATIVE_INDEXED_DB_INDEXES {
        return Err(NativeEngineError::limit(
            "native IndexedDB indexes",
            MAX_NATIVE_INDEXED_DB_INDEXES,
            store.indexes.len(),
        ));
    }
    for (name, index) in &store.indexes {
        validate_indexed_db_name("native IndexedDB index name", name)?;
        validate_indexed_db_index(index)?;
    }
    if store.records.len() > MAX_NATIVE_INDEXED_DB_RECORDS {
        return Err(NativeEngineError::limit(
            "native IndexedDB records",
            MAX_NATIVE_INDEXED_DB_RECORDS,
            store.records.len(),
        ));
    }
    for (key, value) in &store.records {
        validate_indexed_db_record_key(key)?;
        validate_indexed_db_record_value(value)?;
    }
    Ok(())
}

fn validate_indexed_db_index(index: &NativeIndexedDbIndex) -> Result<(), NativeEngineError> {
    if index.key_path.is_empty() {
        return Err(NativeEngineError::invalid(
            "native IndexedDB index key path",
            "must not be empty",
        ));
    }
    validate_indexed_db_key_path(Some(&index.key_path))
}

fn validate_indexed_db_record_key(key: &str) -> Result<(), NativeEngineError> {
    if key.is_empty() || key.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "native IndexedDB record key",
            crate::browser_backend::MAX_BACKEND_ID_BYTES,
            key.len(),
        ));
    }
    Ok(())
}

fn validate_indexed_db_record_value(value: &serde_json::Value) -> Result<(), NativeEngineError> {
    let value_bytes = serde_json::to_vec(value).map_err(|_| {
        NativeEngineError::invalid(
            "native IndexedDB record",
            "must contain a JSON structured-clone value",
        )
    })?;
    if value_bytes.len() > MAX_NATIVE_INDEXED_DB_VALUE_BYTES {
        return Err(NativeEngineError::limit(
            "native IndexedDB record value",
            MAX_NATIVE_INDEXED_DB_VALUE_BYTES,
            value_bytes.len(),
        ));
    }
    Ok(())
}

fn validate_indexed_db_name(field: &str, value: &str) -> Result<(), NativeEngineError> {
    if value.is_empty() {
        return Err(NativeEngineError::invalid(field, "must not be empty"));
    }
    if value.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            field,
            crate::browser_backend::MAX_BACKEND_ID_BYTES,
            value.len(),
        ));
    }
    Ok(())
}

fn validate_indexed_db_serialized_size(
    state: &NativeIndexedDbState,
) -> Result<(), NativeEngineError> {
    let bytes = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
        operation: "validate native IndexedDB state".into(),
        reason: "native IndexedDB state cannot be encoded".into(),
    })?;
    if bytes.len() > MAX_NATIVE_INDEXED_DB_STATE_BYTES {
        return Err(NativeEngineError::limit(
            "native IndexedDB state",
            MAX_NATIVE_INDEXED_DB_STATE_BYTES,
            bytes.len(),
        ));
    }
    Ok(())
}

pub(crate) fn diff_indexed_db_changes(
    storage_key: &str,
    before: &NativeIndexedDbOrigin,
    after: &NativeIndexedDbOrigin,
) -> Result<Vec<NativeIndexedDbChange>, NativeEngineError> {
    validate_indexed_db_storage_key(storage_key)?;
    before.validate()?;
    after.validate()?;
    if before == after {
        return Ok(Vec::new());
    }
    let mut changes = Vec::new();
    let database_names = before
        .databases
        .keys()
        .chain(after.databases.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for name in database_names {
        match (before.databases.get(&name), after.databases.get(&name)) {
            (None, Some(database)) => push_indexed_db_change(
                &mut changes,
                NativeIndexedDbChange::ReplaceDatabase {
                    storage_key: storage_key.to_owned(),
                    name,
                    database: Some(database.clone()),
                },
            )?,
            (Some(_), None) => push_indexed_db_change(
                &mut changes,
                NativeIndexedDbChange::ReplaceDatabase {
                    storage_key: storage_key.to_owned(),
                    name,
                    database: None,
                },
            )?,
            (Some(before_database), Some(after_database)) => {
                if before_database.version != after_database.version {
                    push_indexed_db_change(
                        &mut changes,
                        NativeIndexedDbChange::ReplaceDatabase {
                            storage_key: storage_key.to_owned(),
                            name,
                            database: Some(after_database.clone()),
                        },
                    )?;
                    continue;
                }
                let store_names = before_database
                    .stores
                    .keys()
                    .chain(after_database.stores.keys())
                    .cloned()
                    .collect::<BTreeSet<_>>();
                for store_name in store_names {
                    match (
                        before_database.stores.get(&store_name),
                        after_database.stores.get(&store_name),
                    ) {
                        (None, Some(store)) => push_indexed_db_change(
                            &mut changes,
                            NativeIndexedDbChange::ReplaceStore {
                                storage_key: storage_key.to_owned(),
                                database: name.clone(),
                                version: after_database.version,
                                name: store_name,
                                store: Some(store.clone()),
                            },
                        )?,
                        (Some(_), None) => push_indexed_db_change(
                            &mut changes,
                            NativeIndexedDbChange::ReplaceStore {
                                storage_key: storage_key.to_owned(),
                                database: name.clone(),
                                version: after_database.version,
                                name: store_name,
                                store: None,
                            },
                        )?,
                        (Some(before_store), Some(after_store)) => {
                            if before_store.indexes != after_store.indexes {
                                push_indexed_db_change(
                                    &mut changes,
                                    NativeIndexedDbChange::UpdateStoreIndexes {
                                        storage_key: storage_key.to_owned(),
                                        database: name.clone(),
                                        store: store_name.clone(),
                                        indexes: after_store.indexes.clone(),
                                    },
                                )?;
                            }
                            if (
                                before_store.key_path.clone(),
                                before_store.auto_increment,
                                before_store.next_key,
                            ) != (
                                after_store.key_path.clone(),
                                after_store.auto_increment,
                                after_store.next_key,
                            ) {
                                push_indexed_db_change(
                                    &mut changes,
                                    NativeIndexedDbChange::UpdateStoreMetadata {
                                        storage_key: storage_key.to_owned(),
                                        database: name.clone(),
                                        store: store_name.clone(),
                                        key_path: after_store.key_path.clone(),
                                        auto_increment: after_store.auto_increment,
                                        next_key: after_store.next_key,
                                    },
                                )?;
                            }
                            let record_keys = before_store
                                .records
                                .keys()
                                .chain(after_store.records.keys())
                                .cloned()
                                .collect::<BTreeSet<_>>();
                            for key in record_keys {
                                match (
                                    before_store.records.get(&key),
                                    after_store.records.get(&key),
                                ) {
                                    (None, Some(value)) => push_indexed_db_change(
                                        &mut changes,
                                        NativeIndexedDbChange::PutRecord {
                                            storage_key: storage_key.to_owned(),
                                            database: name.clone(),
                                            store: store_name.clone(),
                                            key,
                                            value: value.clone(),
                                        },
                                    )?,
                                    (Some(_), None) => push_indexed_db_change(
                                        &mut changes,
                                        NativeIndexedDbChange::DeleteRecord {
                                            storage_key: storage_key.to_owned(),
                                            database: name.clone(),
                                            store: store_name.clone(),
                                            key,
                                        },
                                    )?,
                                    (Some(before_value), Some(after_value))
                                        if before_value != after_value =>
                                    {
                                        push_indexed_db_change(
                                            &mut changes,
                                            NativeIndexedDbChange::PutRecord {
                                                storage_key: storage_key.to_owned(),
                                                database: name.clone(),
                                                store: store_name.clone(),
                                                key,
                                                value: after_value.clone(),
                                            },
                                        )?;
                                    }
                                    (Some(_), Some(_)) => {}
                                    (None, None) => {}
                                }
                            }
                        }
                        (None, None) => {}
                    }
                }
            }
            (None, None) => {}
        }
    }
    Ok(changes)
}

fn push_indexed_db_change(
    changes: &mut Vec<NativeIndexedDbChange>,
    change: NativeIndexedDbChange,
) -> Result<(), NativeEngineError> {
    validate_indexed_db_change(&change)?;
    if changes.len() >= MAX_NATIVE_INDEXED_DB_CHANGES {
        return Err(NativeEngineError::limit(
            "native IndexedDB changes",
            MAX_NATIVE_INDEXED_DB_CHANGES,
            changes.len().saturating_add(1),
        ));
    }
    changes.push(change);
    Ok(())
}

fn validate_indexed_db_storage_key(storage_key: &str) -> Result<(), NativeEngineError> {
    if storage_key.is_empty() || storage_key.len() > crate::browser_backend::MAX_TEXT_BYTES {
        return Err(NativeEngineError::limit(
            "native IndexedDB storage key",
            crate::browser_backend::MAX_TEXT_BYTES,
            storage_key.len(),
        ));
    }
    Ok(())
}

fn validate_indexed_db_change(change: &NativeIndexedDbChange) -> Result<(), NativeEngineError> {
    match change {
        NativeIndexedDbChange::ReplaceDatabase {
            storage_key,
            name,
            database,
        } => {
            validate_indexed_db_storage_key(storage_key)?;
            validate_indexed_db_name("native IndexedDB database name", name)?;
            if let Some(database) = database {
                let mut databases = BTreeMap::new();
                databases.insert(name.clone(), database.clone());
                NativeIndexedDbOrigin { databases }.validate()?;
            }
        }
        NativeIndexedDbChange::ReplaceStore {
            storage_key,
            database,
            version,
            name,
            store,
        } => {
            validate_indexed_db_storage_key(storage_key)?;
            validate_indexed_db_name("native IndexedDB database name", database)?;
            if *version == 0 {
                return Err(NativeEngineError::invalid(
                    "native IndexedDB database version",
                    "must be greater than zero",
                ));
            }
            validate_indexed_db_name("native IndexedDB object store name", name)?;
            if let Some(store) = store {
                validate_indexed_db_store(store)?;
            }
        }
        NativeIndexedDbChange::UpdateStoreMetadata {
            storage_key,
            database,
            store,
            key_path,
            next_key,
            ..
        } => {
            validate_indexed_db_storage_key(storage_key)?;
            validate_indexed_db_name("native IndexedDB database name", database)?;
            validate_indexed_db_name("native IndexedDB object store name", store)?;
            validate_indexed_db_key_path(key_path.as_deref())?;
            if *next_key == 0 {
                return Err(NativeEngineError::invalid(
                    "native IndexedDB auto-increment key",
                    "must be greater than zero",
                ));
            }
        }
        NativeIndexedDbChange::UpdateStoreIndexes {
            storage_key,
            database,
            store,
            indexes,
        } => {
            validate_indexed_db_storage_key(storage_key)?;
            validate_indexed_db_name("native IndexedDB database name", database)?;
            validate_indexed_db_name("native IndexedDB object store name", store)?;
            if indexes.len() > MAX_NATIVE_INDEXED_DB_INDEXES {
                return Err(NativeEngineError::limit(
                    "native IndexedDB indexes",
                    MAX_NATIVE_INDEXED_DB_INDEXES,
                    indexes.len(),
                ));
            }
            for (name, index) in indexes {
                validate_indexed_db_name("native IndexedDB index name", name)?;
                validate_indexed_db_index(index)?;
            }
        }
        NativeIndexedDbChange::PutRecord {
            storage_key,
            database,
            store,
            key,
            value,
        } => {
            validate_indexed_db_storage_key(storage_key)?;
            validate_indexed_db_name("native IndexedDB database name", database)?;
            validate_indexed_db_name("native IndexedDB object store name", store)?;
            validate_indexed_db_record_key(key)?;
            validate_indexed_db_record_value(value)?;
        }
        NativeIndexedDbChange::DeleteRecord {
            storage_key,
            database,
            store,
            key,
        } => {
            validate_indexed_db_storage_key(storage_key)?;
            validate_indexed_db_name("native IndexedDB database name", database)?;
            validate_indexed_db_name("native IndexedDB object store name", store)?;
            validate_indexed_db_record_key(key)?;
        }
    }
    Ok(())
}

fn validate_indexed_db_key_path(key_path: Option<&str>) -> Result<(), NativeEngineError> {
    if let Some(key_path) = key_path
        && key_path.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES
    {
        return Err(NativeEngineError::limit(
            "native IndexedDB key path",
            crate::browser_backend::MAX_BACKEND_ID_BYTES,
            key_path.len(),
        ));
    }
    Ok(())
}

pub(crate) fn apply_indexed_db_changes(
    state: &mut NativeIndexedDbState,
    changes: &[NativeIndexedDbChange],
) -> Result<(), NativeEngineError> {
    if changes.len() > MAX_NATIVE_INDEXED_DB_CHANGES {
        return Err(NativeEngineError::limit(
            "native IndexedDB changes",
            MAX_NATIVE_INDEXED_DB_CHANGES,
            changes.len(),
        ));
    }
    for change in changes {
        validate_indexed_db_change(change)?;
        match change {
            NativeIndexedDbChange::ReplaceDatabase {
                storage_key,
                name,
                database,
            } => {
                if let Some(database) = database {
                    state
                        .origins
                        .entry(storage_key.clone())
                        .or_default()
                        .databases
                        .insert(name.clone(), database.clone());
                } else if let Some(origin) = state.origins.get_mut(storage_key) {
                    origin.databases.remove(name);
                    if origin.databases.is_empty() {
                        state.origins.remove(storage_key);
                    }
                }
            }
            NativeIndexedDbChange::ReplaceStore {
                storage_key,
                database,
                name,
                store,
                ..
            } => {
                let mut remove_origin = false;
                if let Some(origin) = state.origins.get_mut(storage_key)
                    && let Some(database) = origin.databases.get_mut(database)
                {
                    if let Some(store) = store {
                        database.stores.insert(name.clone(), store.clone());
                    } else {
                        database.stores.remove(name);
                    }
                    remove_origin = origin.databases.is_empty();
                }
                if remove_origin {
                    state.origins.remove(storage_key);
                }
            }
            NativeIndexedDbChange::UpdateStoreMetadata {
                storage_key,
                database,
                store,
                key_path,
                auto_increment,
                next_key,
            } => {
                if let Some(object_store) = state
                    .origins
                    .get_mut(storage_key)
                    .and_then(|origin| origin.databases.get_mut(database))
                    .and_then(|database| database.stores.get_mut(store))
                {
                    object_store.key_path = key_path.clone();
                    object_store.auto_increment = *auto_increment;
                    object_store.next_key = *next_key;
                }
            }
            NativeIndexedDbChange::UpdateStoreIndexes {
                storage_key,
                database,
                store,
                indexes,
            } => {
                if let Some(object_store) = state
                    .origins
                    .get_mut(storage_key)
                    .and_then(|origin| origin.databases.get_mut(database))
                    .and_then(|database| database.stores.get_mut(store))
                {
                    object_store.indexes = indexes.clone();
                }
            }
            NativeIndexedDbChange::PutRecord {
                storage_key,
                database,
                store,
                key,
                value,
            } => {
                if let Some(object_store) = state
                    .origins
                    .get_mut(storage_key)
                    .and_then(|origin| origin.databases.get_mut(database))
                    .and_then(|database| database.stores.get_mut(store))
                {
                    object_store.records.insert(key.clone(), value.clone());
                }
            }
            NativeIndexedDbChange::DeleteRecord {
                storage_key,
                database,
                store,
                key,
            } => {
                if let Some(object_store) = state
                    .origins
                    .get_mut(storage_key)
                    .and_then(|origin| origin.databases.get_mut(database))
                    .and_then(|database| database.stores.get_mut(store))
                {
                    object_store.records.remove(key);
                }
            }
        }
    }
    state.validate()
}

impl NativeWebStorageState {
    pub(crate) fn entries_for(&self, scope: &str, storage_key: &str) -> BTreeMap<String, String> {
        match scope {
            "local" => self.local.get(storage_key),
            "session" => self.session.get(storage_key),
            _ => None,
        }
        .cloned()
        .unwrap_or_default()
    }

    pub(crate) fn validate(&self) -> Result<(), NativeEngineError> {
        validate_web_storage_state(self)
    }

    pub(crate) fn replace_profile_state(&mut self, mut profile_state: Self) {
        profile_state.session = std::mem::take(&mut self.session);
        *self = profile_state;
    }

    pub(crate) fn apply_storage_event(
        &mut self,
        event: &NativeStorageEvent,
    ) -> Result<(), NativeEngineError> {
        let storage = match event.scope.as_str() {
            "local" => &mut self.local,
            "session" => &mut self.session,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Web Storage scope",
                    "must be local or session",
                ));
            }
        };
        let entries = storage.entry(event.storage_key.clone()).or_default();
        match (&event.key, &event.new_value) {
            (Some(key), Some(value)) => {
                if entries.len() >= crate::browser_backend::MAX_STORAGE_ENTRIES
                    && !entries.contains_key(key)
                {
                    return Err(NativeEngineError::limit(
                        "native Web Storage entries",
                        crate::browser_backend::MAX_STORAGE_ENTRIES,
                        entries.len().saturating_add(1),
                    ));
                }
                entries.insert(key.clone(), value.clone());
            }
            (Some(key), None) => {
                entries.remove(key);
            }
            (None, None) => entries.clear(),
            (None, Some(_)) => {
                return Err(NativeEngineError::invalid(
                    "native storage event",
                    "a clear event must not contain a new value",
                ));
            }
        }
        validate_web_storage_state(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeStorageEvent {
    pub(crate) source_context_id: String,
    pub(crate) scope: String,
    pub(crate) storage_key: String,
    pub(crate) key: Option<String>,
    pub(crate) old_value: Option<String>,
    pub(crate) new_value: Option<String>,
    pub(crate) url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct NativeWebStorageProfile {
    version: u64,
    #[serde(default)]
    revision: u64,
    local: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    cookies: Vec<NativeCookieProfileEntry>,
    #[serde(default)]
    indexed_db: NativeIndexedDbState,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeCookieProfileEntry {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) host_only: bool,
    pub(crate) secure: bool,
    pub(crate) http_only: bool,
    #[serde(default)]
    pub(crate) expires_at_unix_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeCookieChange {
    pub(crate) name: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) cookie: Option<NativeCookieProfileEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NativeStorageJournalRecord {
    pub(crate) writer_id: String,
    #[serde(default)]
    pub(crate) event: Option<NativeStorageEvent>,
    #[serde(default)]
    pub(crate) indexed_db_changes: Vec<NativeIndexedDbChange>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NativeStorageJournalRead {
    pub(crate) records: Vec<NativeStorageJournalRecord>,
    pub(crate) recovered: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NativeStorageReaderLease {
    reader_id: String,
    cursor: u64,
    heartbeat_unix_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NativeStorageReaderLeaseFile {
    version: u64,
    readers: Vec<NativeStorageReaderLease>,
}

static NATIVE_STORAGE_WRITER_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn new_storage_writer_id() -> Result<String, NativeEngineError> {
    let writer_id = format!(
        "{}-{}",
        std::process::id(),
        NATIVE_STORAGE_WRITER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    validate_context_id(&writer_id)?;
    Ok(writer_id)
}

/// Cross-process ownership of one bounded Web Storage profile I/O operation.
///
/// The lock file is retained after release. The operating system owns the
/// advisory lock on the open handle, so a crashed worker cannot leave a stale
/// path that blocks a later profile read or write.
struct NativeStorageProfileLock {
    file: File,
}

impl Drop for NativeStorageProfileLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn lock_web_storage_profile(
    path: &Path,
    exclusive: bool,
) -> Result<NativeStorageProfileLock, NativeEngineError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|_| NativeEngineError::Worker {
            operation: "open native Web Storage profile lock".into(),
            reason: "native Web Storage profile directory cannot be created".into(),
        })?;
    }
    let lock_path = path.with_extension("lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|_| NativeEngineError::Worker {
            operation: "open native Web Storage profile lock".into(),
            reason: "native Web Storage profile lock cannot be opened".into(),
        })?;
    let deadline = Instant::now() + NATIVE_STORAGE_PROFILE_LOCK_TIMEOUT;
    loop {
        let result = if exclusive {
            FileExt::try_lock_exclusive(&file)
        } else {
            FileExt::try_lock_shared(&file)
        };
        match result {
            Ok(()) => return Ok(NativeStorageProfileLock { file }),
            Err(error) if profile_lock_is_contended(&error) => {
                if Instant::now() >= deadline {
                    return Err(NativeEngineError::StorageProfileLocked {
                        path: path.to_string_lossy().into_owned(),
                    });
                }
                std::thread::sleep(NATIVE_STORAGE_PROFILE_LOCK_RETRY);
            }
            Err(_) => {
                return Err(NativeEngineError::Worker {
                    operation: if exclusive {
                        "write native Web Storage profile"
                    } else {
                        "read native Web Storage profile"
                    }
                    .into(),
                    reason: "native Web Storage profile lock cannot be acquired".into(),
                });
            }
        }
    }
}

fn profile_lock_is_contended(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::WouldBlock || (cfg!(windows) && error.raw_os_error() == Some(33))
}

fn storage_event_journal_path(path: &Path) -> PathBuf {
    path.with_extension("events")
}

fn storage_reader_lease_path(path: &Path) -> PathBuf {
    path.with_extension("readers")
}

fn complete_storage_journal_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index.saturating_add(1))
}

fn complete_storage_journal_prefix(bytes: &[u8], maximum: usize) -> usize {
    let maximum = maximum.min(bytes.len());
    if maximum == bytes.len() {
        return maximum;
    }
    bytes[..maximum]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index.saturating_add(1))
}

fn read_storage_reader_leases(
    path: &Path,
) -> Result<Vec<NativeStorageReaderLease>, NativeEngineError> {
    let lease_path = storage_reader_lease_path(path);
    let metadata = match fs::metadata(&lease_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "read native Web Storage reader leases".into(),
                reason: "native Web Storage reader lease metadata is unavailable".into(),
            });
        }
    };
    let lease_bytes = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if lease_bytes > MAX_NATIVE_STORAGE_READER_LEASE_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage reader leases",
            MAX_NATIVE_STORAGE_READER_LEASE_BYTES,
            lease_bytes,
        ));
    }
    let bytes = fs::read(&lease_path).map_err(|_| NativeEngineError::Worker {
        operation: "read native Web Storage reader leases".into(),
        reason: "native Web Storage reader leases cannot be read".into(),
    })?;
    let file: NativeStorageReaderLeaseFile = serde_json::from_slice(&bytes).map_err(|_| {
        NativeEngineError::invalid(
            "native Web Storage reader leases",
            "must contain a valid reader lease file",
        )
    })?;
    if file.version != NATIVE_STORAGE_READER_LEASE_VERSION {
        return Err(NativeEngineError::invalid(
            "native Web Storage reader lease version",
            "is unsupported",
        ));
    }
    if file.readers.len() > MAX_NATIVE_STORAGE_READER_LEASES {
        return Err(NativeEngineError::limit(
            "native Web Storage reader leases",
            MAX_NATIVE_STORAGE_READER_LEASES,
            file.readers.len(),
        ));
    }
    for (index, reader) in file.readers.iter().enumerate() {
        validate_context_id(&reader.reader_id)?;
        if file.readers[..index]
            .iter()
            .any(|previous| previous.reader_id == reader.reader_id)
        {
            return Err(NativeEngineError::invalid(
                "native Web Storage reader leases",
                "must not contain duplicate reader IDs",
            ));
        }
    }
    Ok(file.readers)
}

fn write_storage_reader_leases(
    path: &Path,
    readers: &[NativeStorageReaderLease],
) -> Result<(), NativeEngineError> {
    let lease_path = storage_reader_lease_path(path);
    if readers.is_empty() {
        match fs::remove_file(&lease_path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => {
                return Err(NativeEngineError::Worker {
                    operation: "remove native Web Storage reader leases".into(),
                    reason: "native Web Storage reader leases cannot be removed".into(),
                });
            }
        }
        return Ok(());
    }
    let file = NativeStorageReaderLeaseFile {
        version: NATIVE_STORAGE_READER_LEASE_VERSION,
        readers: readers.to_vec(),
    };
    let bytes = serde_json::to_vec(&file).map_err(|_| NativeEngineError::Worker {
        operation: "encode native Web Storage reader leases".into(),
        reason: "native Web Storage reader leases cannot be encoded".into(),
    })?;
    if bytes.len() > MAX_NATIVE_STORAGE_READER_LEASE_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage reader leases",
            MAX_NATIVE_STORAGE_READER_LEASE_BYTES,
            bytes.len(),
        ));
    }
    let temporary_path = lease_path.with_extension(format!("readers-tmp-{}", std::process::id()));
    let mut temporary = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)
        .map_err(|_| NativeEngineError::Worker {
            operation: "write native Web Storage reader leases".into(),
            reason: "native Web Storage reader leases cannot be opened".into(),
        })?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.sync_data())
        .map_err(|_| NativeEngineError::Worker {
            operation: "write native Web Storage reader leases".into(),
            reason: "native Web Storage reader leases cannot be committed".into(),
        })?;
    drop(temporary);
    if let Err(rename_error) = fs::rename(&temporary_path, &lease_path) {
        let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let fallback = fs::copy(&temporary_path, &lease_path).and_then(|copied_bytes| {
            (copied_bytes == expected_bytes)
                .then_some(())
                .ok_or_else(|| std::io::Error::from(ErrorKind::WriteZero))
        });
        if fallback.is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(NativeEngineError::Worker {
                operation: "write native Web Storage reader leases".into(),
                reason: format!(
                    "native Web Storage reader leases cannot be committed: {rename_error}"
                ),
            });
        }
        let _ = fs::remove_file(&temporary_path);
    }
    Ok(())
}

fn prune_storage_reader_leases(readers: &mut Vec<NativeStorageReaderLease>, now: u64) -> bool {
    let original_len = readers.len();
    readers.retain(|reader| {
        now.saturating_sub(reader.heartbeat_unix_seconds)
            <= NATIVE_STORAGE_READER_LEASE_TTL.as_secs()
    });
    readers.len() != original_len
}

fn upsert_storage_reader_lease(
    readers: &mut Vec<NativeStorageReaderLease>,
    reader_id: &str,
    cursor: u64,
    heartbeat_unix_seconds: u64,
) -> Result<bool, NativeEngineError> {
    validate_context_id(reader_id)?;
    if let Some(reader) = readers
        .iter_mut()
        .find(|reader| reader.reader_id == reader_id)
    {
        let changed =
            reader.cursor != cursor || reader.heartbeat_unix_seconds != heartbeat_unix_seconds;
        reader.cursor = cursor;
        reader.heartbeat_unix_seconds = heartbeat_unix_seconds;
        return Ok(changed);
    }
    if readers.len() >= MAX_NATIVE_STORAGE_READER_LEASES {
        return Err(NativeEngineError::limit(
            "native Web Storage reader leases",
            MAX_NATIVE_STORAGE_READER_LEASES,
            readers.len().saturating_add(1),
        ));
    }
    readers.push(NativeStorageReaderLease {
        reader_id: reader_id.to_owned(),
        cursor,
        heartbeat_unix_seconds,
    });
    Ok(true)
}

pub(crate) fn register_storage_reader(
    path: Option<&Path>,
    reader_id: &str,
    cursor: u64,
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    let _lock = lock_web_storage_profile(path, true)?;
    let now = unix_time_seconds();
    let mut readers = read_storage_reader_leases(path)?;
    let pruned = prune_storage_reader_leases(&mut readers, now);
    let changed = upsert_storage_reader_lease(&mut readers, reader_id, cursor, now)?;
    if pruned || changed {
        write_storage_reader_leases(path, &readers)?;
    }
    Ok(())
}

pub(crate) fn unregister_storage_reader(
    path: Option<&Path>,
    reader_id: &str,
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    validate_context_id(reader_id)?;
    let _lock = lock_web_storage_profile(path, true)?;
    let mut readers = read_storage_reader_leases(path)?;
    let original_len = readers.len();
    readers.retain(|reader| reader.reader_id != reader_id);
    if readers.len() != original_len {
        write_storage_reader_leases(path, &readers)?;
    }
    Ok(())
}

pub(crate) fn storage_event_cursor(path: Option<&Path>) -> Result<u64, NativeEngineError> {
    let Some(path) = path else {
        return Ok(0);
    };
    let _lock = lock_web_storage_profile(path, false)?;
    let journal_path = storage_event_journal_path(path);
    let bytes = match fs::read(&journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "open native Web Storage event journal".into(),
                reason: "native Web Storage event journal cannot be read".into(),
            });
        }
    };
    let byte_len = bytes.len();
    if byte_len > MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage event journal",
            MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES,
            byte_len,
        ));
    }
    Ok(u64::try_from(complete_storage_journal_len(&bytes)).unwrap_or(u64::MAX))
}

fn validate_storage_journal_event(event: &NativeStorageEvent) -> Result<(), NativeEngineError> {
    validate_context_id(&event.source_context_id)?;
    if event.url.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage event URL",
            MAX_NATIVE_SCRIPT_BYTES,
            event.url.len(),
        ));
    }
    if event
        .old_value
        .as_ref()
        .is_some_and(|value| value.len() > crate::browser_backend::MAX_TEXT_BYTES)
    {
        let actual = event.old_value.as_ref().map_or(0, String::len);
        return Err(NativeEngineError::limit(
            "native Web Storage old value",
            crate::browser_backend::MAX_TEXT_BYTES,
            actual,
        ));
    }
    let mut state = NativeWebStorageState::default();
    state.apply_storage_event(event)
}

fn decode_storage_journal_record(
    bytes: &[u8],
) -> Result<NativeStorageJournalRecord, NativeEngineError> {
    let record: NativeStorageJournalRecord = serde_json::from_slice(bytes).map_err(|_| {
        NativeEngineError::invalid(
            "native Web Storage event journal",
            "must contain newline-delimited storage event records",
        )
    })?;
    validate_context_id(&record.writer_id)?;
    if let Some(event) = record.event.as_ref() {
        validate_storage_journal_event(event)?;
    }
    if record.indexed_db_changes.is_empty() && record.event.is_none() {
        return Err(NativeEngineError::invalid(
            "native Web Storage event journal",
            "must contain a storage event or IndexedDB changes",
        ));
    }
    if record.indexed_db_changes.len() > MAX_NATIVE_INDEXED_DB_CHANGES {
        return Err(NativeEngineError::limit(
            "native IndexedDB changes",
            MAX_NATIVE_INDEXED_DB_CHANGES,
            record.indexed_db_changes.len(),
        ));
    }
    for change in &record.indexed_db_changes {
        validate_indexed_db_change(change)?;
    }
    Ok(record)
}

pub(crate) fn read_storage_event_journal(
    path: Option<&Path>,
    reader_id: &str,
    cursor: &mut u64,
) -> Result<NativeStorageJournalRead, NativeEngineError> {
    let Some(path) = path else {
        return Ok(NativeStorageJournalRead {
            records: Vec::new(),
            recovered: false,
        });
    };
    validate_context_id(reader_id)?;
    let _lock = lock_web_storage_profile(path, true)?;
    let now = unix_time_seconds();
    let mut readers = read_storage_reader_leases(path)?;
    let pruned = prune_storage_reader_leases(&mut readers, now);
    let lease = readers
        .iter()
        .find(|reader| reader.reader_id == reader_id)
        .cloned();
    let journal_path = storage_event_journal_path(path);
    let bytes = match fs::read(&journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "read native Web Storage event journal".into(),
                reason: "native Web Storage event journal cannot be read".into(),
            });
        }
    };
    let journal_bytes = bytes.len();
    if journal_bytes > MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage event journal",
            MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES,
            journal_bytes,
        ));
    }
    let complete_len = complete_storage_journal_len(&bytes);
    let mut recovered = false;
    if let Some(lease) = lease.as_ref() {
        *cursor = lease.cursor;
    } else {
        recovered = true;
        *cursor = u64::try_from(complete_len).unwrap_or(u64::MAX);
    }
    if *cursor > u64::try_from(complete_len).unwrap_or(u64::MAX) {
        recovered = true;
        *cursor = u64::try_from(complete_len).unwrap_or(u64::MAX);
    }
    let mut next_cursor = *cursor;
    let mut records = Vec::new();
    if !recovered {
        let start = usize::try_from(*cursor).unwrap_or(usize::MAX);
        for line in bytes[start..complete_len].split_inclusive(|byte| *byte == b'\n') {
            let record_bytes = &line[..line.len().saturating_sub(1)];
            if record_bytes.is_empty() {
                return Err(NativeEngineError::invalid(
                    "native Web Storage event journal",
                    "must not contain empty records",
                ));
            }
            records.push(decode_storage_journal_record(record_bytes)?);
            next_cursor = next_cursor.saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
        }
    }
    *cursor = next_cursor;
    let cursor_changed = lease.as_ref().is_none_or(|lease| lease.cursor != *cursor);
    let heartbeat_due = lease.as_ref().is_none_or(|lease| {
        now.saturating_sub(lease.heartbeat_unix_seconds)
            >= NATIVE_STORAGE_READER_HEARTBEAT.as_secs()
    });
    if pruned || lease.is_none() || cursor_changed || heartbeat_due {
        upsert_storage_reader_lease(&mut readers, reader_id, *cursor, now)?;
        write_storage_reader_leases(path, &readers)?;
    }
    Ok(NativeStorageJournalRead { records, recovered })
}

pub(crate) fn append_storage_changes(
    path: Option<&Path>,
    writer_id: &str,
    events: &[NativeStorageEvent],
    indexed_db_changes: &[NativeIndexedDbChange],
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    if events.is_empty() && indexed_db_changes.is_empty() {
        return Ok(());
    }
    validate_context_id(writer_id)?;
    let _lock = lock_web_storage_profile(path, true)?;
    let journal_path = storage_event_journal_path(path);
    let mut existing = match fs::read(&journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "append native Web Storage event journal".into(),
                reason: "native Web Storage event journal cannot be read".into(),
            });
        }
    };
    if existing.len() > MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage event journal",
            MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES,
            existing.len(),
        ));
    }
    let complete_len = complete_storage_journal_len(&existing);
    if complete_len < existing.len() {
        let file = OpenOptions::new()
            .write(true)
            .open(&journal_path)
            .map_err(|_| NativeEngineError::Worker {
                operation: "repair native Web Storage event journal".into(),
                reason: "native Web Storage event journal cannot be opened".into(),
            })?;
        file.set_len(u64::try_from(complete_len).unwrap_or(u64::MAX))
            .map_err(|_| NativeEngineError::Worker {
                operation: "repair native Web Storage event journal".into(),
                reason: "native Web Storage event journal cannot be truncated".into(),
            })?;
        existing.truncate(complete_len);
    }
    let mut payload = Vec::new();
    for event in events {
        validate_storage_journal_event(event)?;
        let record = NativeStorageJournalRecord {
            writer_id: writer_id.to_owned(),
            event: Some(event.clone()),
            indexed_db_changes: Vec::new(),
        };
        let mut encoded = serde_json::to_vec(&record).map_err(|_| NativeEngineError::Worker {
            operation: "encode native Web Storage event journal".into(),
            reason: "native Web Storage event journal record cannot be encoded".into(),
        })?;
        encoded.push(b'\n');
        payload.extend(encoded);
    }
    if !indexed_db_changes.is_empty() {
        if indexed_db_changes.len() > MAX_NATIVE_INDEXED_DB_CHANGES {
            return Err(NativeEngineError::limit(
                "native IndexedDB changes",
                MAX_NATIVE_INDEXED_DB_CHANGES,
                indexed_db_changes.len(),
            ));
        }
        for change in indexed_db_changes {
            validate_indexed_db_change(change)?;
        }
        let record = NativeStorageJournalRecord {
            writer_id: writer_id.to_owned(),
            event: None,
            indexed_db_changes: indexed_db_changes.to_vec(),
        };
        let mut encoded = serde_json::to_vec(&record).map_err(|_| NativeEngineError::Worker {
            operation: "encode native Web Storage event journal".into(),
            reason: "native Web Storage event journal record cannot be encoded".into(),
        })?;
        encoded.push(b'\n');
        payload.extend(encoded);
    }
    if payload.len() > MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage event journal",
            MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES,
            payload.len(),
        ));
    }
    let now = unix_time_seconds();
    let mut readers = read_storage_reader_leases(path)?;
    let mut leases_changed = prune_storage_reader_leases(&mut readers, now);
    let writer_can_acknowledge_existing = readers.iter().any(|reader| {
        reader.reader_id == writer_id
            && reader.cursor >= u64::try_from(existing.len()).unwrap_or(u64::MAX)
    });
    let total = existing.len().saturating_add(payload.len());
    let mut compacted = false;
    if total > MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES {
        let maximum_drop = readers
            .iter()
            .map(|reader| {
                usize::try_from(reader.cursor)
                    .unwrap_or(existing.len())
                    .min(existing.len())
            })
            .min()
            .unwrap_or(existing.len());
        let drop_len = complete_storage_journal_prefix(&existing, maximum_drop);
        if drop_len > 0 {
            existing = existing[drop_len..].to_vec();
            let dropped = u64::try_from(drop_len).unwrap_or(u64::MAX);
            for reader in &mut readers {
                reader.cursor = reader.cursor.saturating_sub(dropped);
            }
            compacted = true;
        }
    }
    let total_after_compaction = existing.len().saturating_add(payload.len());
    if total_after_compaction > MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage event journal",
            MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES,
            total_after_compaction,
        ));
    }
    if compacted {
        existing.extend(payload);
        write_storage_event_journal(&journal_path, &existing)?;
    } else {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)
            .map_err(|_| NativeEngineError::Worker {
                operation: "append native Web Storage event journal".into(),
                reason: "native Web Storage event journal cannot be opened".into(),
            })?;
        file.write_all(&payload)
            .and_then(|_| file.sync_data())
            .map_err(|_| NativeEngineError::Worker {
                operation: "append native Web Storage event journal".into(),
                reason: "native Web Storage event journal cannot be committed".into(),
            })?;
    }
    if writer_can_acknowledge_existing {
        leases_changed |= upsert_storage_reader_lease(
            &mut readers,
            writer_id,
            u64::try_from(total_after_compaction).unwrap_or(u64::MAX),
            now,
        )?;
    }
    if compacted || leases_changed {
        write_storage_reader_leases(path, &readers)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn append_storage_events(
    path: Option<&Path>,
    writer_id: &str,
    events: &[NativeStorageEvent],
) -> Result<(), NativeEngineError> {
    append_storage_changes(path, writer_id, events, &[])
}

fn write_storage_event_journal(path: &Path, bytes: &[u8]) -> Result<(), NativeEngineError> {
    let temporary_path = path.with_extension(format!("events-tmp-{}", std::process::id()));
    let mut temporary = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)
        .map_err(|_| NativeEngineError::Worker {
            operation: "rewrite native Web Storage event journal".into(),
            reason: "native Web Storage event journal cannot be opened".into(),
        })?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.sync_data())
        .map_err(|_| NativeEngineError::Worker {
            operation: "rewrite native Web Storage event journal".into(),
            reason: "native Web Storage event journal cannot be committed".into(),
        })?;
    drop(temporary);
    if let Err(rename_error) = fs::rename(&temporary_path, path) {
        let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let fallback = fs::copy(&temporary_path, path).and_then(|copied_bytes| {
            (copied_bytes == expected_bytes)
                .then_some(())
                .ok_or_else(|| std::io::Error::from(ErrorKind::WriteZero))
        });
        if fallback.is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(NativeEngineError::Worker {
                operation: "rewrite native Web Storage event journal".into(),
                reason: format!(
                    "native Web Storage event journal cannot be committed: {rename_error}"
                ),
            });
        }
        let _ = fs::remove_file(&temporary_path);
    }
    Ok(())
}

#[cfg(test)]
mod storage_journal_tests {
    use super::*;

    fn test_profile_path(label: &str) -> PathBuf {
        let sequence = NATIVE_STORAGE_WRITER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "glass-native-storage-journal-{}-{label}-{sequence}.json",
            std::process::id()
        ))
    }

    fn remove_test_profile(path: &Path) {
        for suffix in ["", "lock", "events", "readers"] {
            let candidate = if suffix.is_empty() {
                path.to_owned()
            } else {
                path.with_extension(suffix)
            };
            let _ = fs::remove_file(candidate);
        }
        let _ = fs::remove_file(path.with_extension(format!("events-tmp-{}", std::process::id())));
        let _ = fs::remove_file(path.with_extension(format!("readers-tmp-{}", std::process::id())));
    }

    fn journal_event(index: usize) -> NativeStorageEvent {
        NativeStorageEvent {
            source_context_id: "source".into(),
            scope: "local".into(),
            storage_key: "https://journal.test".into(),
            key: Some(format!("key-{index}")),
            old_value: None,
            new_value: Some("x".repeat(crate::browser_backend::MAX_TEXT_BYTES)),
            url: "https://journal.test/".into(),
        }
    }

    #[test]
    fn storage_event_journal_compacts_acknowledged_records() {
        let profile_path = test_profile_path("compaction");
        remove_test_profile(&profile_path);
        let first_batch = (0..128).map(journal_event).collect::<Vec<_>>();
        let second_batch = (128..256).map(journal_event).collect::<Vec<_>>();

        register_storage_reader(Some(&profile_path), "reader", 0).unwrap();
        append_storage_events(Some(&profile_path), "writer", &first_batch).unwrap();
        let mut cursor = 0;
        let first_read =
            read_storage_event_journal(Some(&profile_path), "reader", &mut cursor).unwrap();
        assert!(!first_read.recovered);
        assert_eq!(first_read.records.len(), first_batch.len());
        let first_end = cursor;
        assert!(first_end > 0);

        append_storage_events(Some(&profile_path), "writer", &second_batch).unwrap();
        let journal_size = fs::metadata(storage_event_journal_path(&profile_path))
            .unwrap()
            .len();
        assert!(journal_size <= MAX_WEB_STORAGE_EVENT_JOURNAL_BYTES as u64);

        let second_read =
            read_storage_event_journal(Some(&profile_path), "reader", &mut cursor).unwrap();
        assert!(!second_read.recovered);
        assert_eq!(second_read.records.len(), second_batch.len());
        assert_eq!(
            second_read.records[0]
                .event
                .as_ref()
                .and_then(|event| event.key.as_deref()),
            Some("key-128")
        );
        assert!(cursor > 0);

        unregister_storage_reader(Some(&profile_path), "reader").unwrap();
        remove_test_profile(&profile_path);
    }

    #[test]
    fn storage_event_journal_recovers_without_reader_lease() {
        let profile_path = test_profile_path("recovery");
        remove_test_profile(&profile_path);
        append_storage_events(
            Some(&profile_path),
            "writer",
            &[NativeStorageEvent {
                source_context_id: "source".into(),
                scope: "local".into(),
                storage_key: "https://journal.test".into(),
                key: Some("key".into()),
                old_value: None,
                new_value: Some("value".into()),
                url: "https://journal.test/".into(),
            }],
        )
        .unwrap();

        let mut cursor = 0;
        let recovered =
            read_storage_event_journal(Some(&profile_path), "crashed-reader", &mut cursor).unwrap();
        assert!(recovered.recovered);
        assert!(recovered.records.is_empty());
        let recovered_cursor = cursor;
        let next =
            read_storage_event_journal(Some(&profile_path), "crashed-reader", &mut cursor).unwrap();
        assert!(!next.recovered);
        assert!(next.records.is_empty());
        assert_eq!(cursor, recovered_cursor);

        unregister_storage_reader(Some(&profile_path), "crashed-reader").unwrap();
        remove_test_profile(&profile_path);
    }
}

pub(crate) fn load_web_storage_profile(
    path: Option<&Path>,
) -> Result<NativeWebStorageState, NativeEngineError> {
    let Some(path) = path else {
        return Ok(NativeWebStorageState::default());
    };
    let _lock = lock_web_storage_profile(path, false)?;
    let Some(profile) = read_web_storage_profile(path)? else {
        return Ok(NativeWebStorageState::default());
    };
    Ok(NativeWebStorageState {
        local: profile.local,
        session: BTreeMap::new(),
    })
}

pub(crate) fn load_cookie_profile(
    path: Option<&Path>,
) -> Result<Vec<NativeCookieProfileEntry>, NativeEngineError> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let _lock = lock_web_storage_profile(path, false)?;
    let Some(profile) = read_web_storage_profile(path)? else {
        return Ok(Vec::new());
    };
    Ok(profile.cookies)
}

pub(crate) fn load_indexed_db_profile(
    path: Option<&Path>,
) -> Result<NativeIndexedDbState, NativeEngineError> {
    let Some(path) = path else {
        return Ok(NativeIndexedDbState::default());
    };
    let _lock = lock_web_storage_profile(path, false)?;
    let Some(profile) = read_web_storage_profile(path)? else {
        return Ok(NativeIndexedDbState::default());
    };
    Ok(profile.indexed_db)
}

fn read_web_storage_profile(
    path: &Path,
) -> Result<Option<NativeWebStorageProfile>, NativeEngineError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "load native Web Storage profile".into(),
                reason: "native Web Storage profile metadata is unavailable".into(),
            });
        }
    };
    let profile_bytes = usize::try_from(metadata.len()).map_err(|_| {
        NativeEngineError::limit(
            "native Web Storage profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            usize::MAX,
        )
    })?;
    if profile_bytes > MAX_WEB_STORAGE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            profile_bytes,
        ));
    }
    let bytes = fs::read(path).map_err(|_| NativeEngineError::Worker {
        operation: "load native Web Storage profile".into(),
        reason: "native Web Storage profile cannot be read".into(),
    })?;
    let profile: NativeWebStorageProfile = serde_json::from_slice(&bytes).map_err(|_| {
        NativeEngineError::invalid(
            "storage profile",
            "must contain a valid native Web Storage profile",
        )
    })?;
    if profile.version != WEB_STORAGE_PROFILE_VERSION {
        return Err(NativeEngineError::invalid(
            "storage profile version",
            "is unsupported",
        ));
    }
    let NativeWebStorageProfile {
        revision,
        local,
        cookies,
        indexed_db,
        ..
    } = profile;
    let state = NativeWebStorageState {
        local,
        session: BTreeMap::new(),
    };
    validate_web_storage_state(&state)?;
    indexed_db.validate()?;
    validate_cookie_profile(&cookies)?;
    let cookies = cookies
        .into_iter()
        .filter(|cookie| {
            cookie
                .expires_at_unix_seconds
                .is_none_or(|expires_at| expires_at > unix_time_seconds())
        })
        .collect();
    Ok(Some(NativeWebStorageProfile {
        version: WEB_STORAGE_PROFILE_VERSION,
        revision,
        local: state.local,
        cookies,
        indexed_db,
    }))
}

pub(crate) fn save_web_storage_profile(
    path: Option<&Path>,
    state: &NativeWebStorageState,
    storage_changes: &[NativeStorageEvent],
    cookie_state: &[NativeCookieProfileEntry],
    cookie_changes: &[NativeCookieChange],
    indexed_db: &NativeIndexedDbState,
    indexed_db_changes: &[NativeIndexedDbChange],
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    validate_web_storage_state(state)?;
    indexed_db.validate()?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|_| NativeEngineError::Worker {
            operation: "save native Web Storage profile".into(),
            reason: "native Web Storage profile directory cannot be created".into(),
        })?;
    }
    let _lock = lock_web_storage_profile(path, true)?;
    let current = read_web_storage_profile(path)?;
    let mut local = current
        .as_ref()
        .map(|profile| profile.local.clone())
        .unwrap_or_else(|| state.local.clone());
    validate_cookie_profile(cookie_state)?;
    if current.is_some() {
        merge_local_storage_changes(&mut local, storage_changes)?;
    }
    let mut cookies = current
        .as_ref()
        .map(|profile| profile.cookies.clone())
        .unwrap_or_else(|| cookie_state.to_vec());
    if current.is_some() {
        merge_cookie_changes(&mut cookies, cookie_changes)?;
    }
    let mut merged_indexed_db = current
        .as_ref()
        .map(|profile| profile.indexed_db.clone())
        .unwrap_or_else(|| indexed_db.clone());
    if current.is_some() {
        apply_indexed_db_changes(&mut merged_indexed_db, indexed_db_changes)?;
    }
    merged_indexed_db.validate()?;
    let revision = match current.as_ref().map(|profile| profile.revision) {
        Some(revision) => revision
            .checked_add(1)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "save native Web Storage profile".into(),
                reason: "native Web Storage profile revision overflowed".into(),
            })?,
        None => 1,
    };
    let merged_state = NativeWebStorageState {
        local,
        session: state.session.clone(),
    };
    validate_web_storage_state(&merged_state)?;
    let profile = NativeWebStorageProfile {
        version: WEB_STORAGE_PROFILE_VERSION,
        revision,
        local: merged_state.local,
        cookies,
        indexed_db: merged_indexed_db,
    };
    let bytes = serde_json::to_vec(&profile).map_err(|_| NativeEngineError::Worker {
        operation: "save native Web Storage profile".into(),
        reason: "native Web Storage profile cannot be encoded".into(),
    })?;
    if bytes.len() > MAX_WEB_STORAGE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            bytes.len(),
        ));
    }
    let temporary_path = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary_path, &bytes).map_err(|_| NativeEngineError::Worker {
        operation: "save native Web Storage profile".into(),
        reason: "native Web Storage profile cannot be written".into(),
    })?;
    if let Err(rename_error) = fs::rename(&temporary_path, path) {
        // Unix replaces an existing destination atomically. Windows refuses
        // that rename, so copy the already-complete bounded snapshot as a
        // portable fallback and keep the original until the copy starts.
        let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let fallback = fs::copy(&temporary_path, path).and_then(|copied_bytes| {
            (copied_bytes == expected_bytes)
                .then_some(())
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::WriteZero))
        });
        if fallback.is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(NativeEngineError::Worker {
                operation: "save native Web Storage profile".into(),
                reason: format!("native Web Storage profile cannot be committed: {rename_error}"),
            });
        }
        let _ = fs::remove_file(&temporary_path);
    }
    Ok(())
}

fn merge_local_storage_changes(
    local: &mut BTreeMap<String, BTreeMap<String, String>>,
    storage_changes: &[NativeStorageEvent],
) -> Result<(), NativeEngineError> {
    let mut merged = NativeWebStorageState {
        local: std::mem::take(local),
        session: BTreeMap::new(),
    };
    for event in storage_changes {
        match event.scope.as_str() {
            "local" => merged.apply_storage_event(event)?,
            "session" => {}
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Web Storage scope",
                    "must be local or session",
                ));
            }
        }
    }
    *local = merged.local;
    Ok(())
}

fn merge_cookie_changes(
    cookies: &mut Vec<NativeCookieProfileEntry>,
    changes: &[NativeCookieChange],
) -> Result<(), NativeEngineError> {
    for change in changes {
        if let Some(cookie) = change.cookie.as_ref() {
            if cookie.name != change.name
                || cookie.domain != change.domain
                || cookie.path != change.path
            {
                return Err(NativeEngineError::invalid(
                    "native cookie change",
                    "cookie key does not match its change key",
                ));
            }
            validate_cookie_profile_entry(cookie)?;
        }
        cookies.retain(|cookie| {
            cookie.name != change.name
                || cookie.domain != change.domain
                || cookie.path != change.path
        });
        if let Some(cookie) = change.cookie.clone() {
            if cookies.len() >= MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
                cookies.remove(0);
            }
            cookies.push(cookie);
        }
    }
    validate_cookie_profile(cookies)
}

fn validate_cookie_profile(cookies: &[NativeCookieProfileEntry]) -> Result<(), NativeEngineError> {
    if cookies.len() > MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
        return Err(NativeEngineError::limit(
            "native cookie profile entries",
            MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
            cookies.len(),
        ));
    }
    for cookie in cookies {
        validate_cookie_profile_entry(cookie)?;
    }
    for (index, cookie) in cookies.iter().enumerate() {
        if cookies[..index].iter().any(|previous| {
            previous.name == cookie.name
                && previous.domain == cookie.domain
                && previous.path == cookie.path
        }) {
            return Err(NativeEngineError::invalid(
                "native cookie profile",
                "must not contain duplicate cookie keys",
            ));
        }
    }
    Ok(())
}

fn validate_cookie_profile_entry(
    cookie: &NativeCookieProfileEntry,
) -> Result<(), NativeEngineError> {
    if cookie.name.is_empty() {
        return Err(NativeEngineError::invalid(
            "native cookie profile name",
            "must not be empty",
        ));
    }
    if cookie.name.len() > MAX_NATIVE_COOKIE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native cookie profile name",
            MAX_NATIVE_COOKIE_PROFILE_BYTES,
            cookie.name.len(),
        ));
    }
    if cookie.value.len() > MAX_NATIVE_COOKIE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native cookie profile value",
            MAX_NATIVE_COOKIE_PROFILE_BYTES,
            cookie.value.len(),
        ));
    }
    if cookie.name.bytes().any(|byte| {
        byte.is_ascii_control()
            || matches!(
                byte,
                b'(' | b')'
                    | b'<'
                    | b'>'
                    | b'@'
                    | b','
                    | b';'
                    | b':'
                    | b'\\'
                    | b'"'
                    | b'/'
                    | b'['
                    | b']'
                    | b'?'
                    | b'='
                    | b'{'
                    | b'}'
                    | b' '
                    | b'\t'
            )
    }) || cookie
        .value
        .bytes()
        .any(|byte| byte.is_ascii_control() || byte == b';')
    {
        return Err(NativeEngineError::invalid(
            "native cookie profile",
            "name and value contain unsupported cookie bytes",
        ));
    }
    if cookie.domain.is_empty() || cookie.domain.len() > MAX_NATIVE_COOKIE_PROFILE_BYTES {
        return Err(NativeEngineError::invalid(
            "native cookie profile domain",
            "must be a non-empty bounded value",
        ));
    }
    if cookie.path.is_empty()
        || cookie.path.len() > MAX_NATIVE_COOKIE_PROFILE_BYTES
        || !cookie.path.starts_with('/')
    {
        return Err(NativeEngineError::invalid(
            "native cookie profile path",
            "must be a bounded absolute path",
        ));
    }
    Ok(())
}

fn unix_time_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn validate_web_storage_state(state: &NativeWebStorageState) -> Result<(), NativeEngineError> {
    for origins in [&state.local, &state.session] {
        if origins.len() > crate::browser_backend::MAX_STORAGE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native Web Storage origins",
                crate::browser_backend::MAX_STORAGE_ENTRIES,
                origins.len(),
            ));
        }
        for (origin, entries) in origins {
            if origin.len() > crate::browser_backend::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "native Web Storage origin",
                    crate::browser_backend::MAX_TEXT_BYTES,
                    origin.len(),
                ));
            }
            if entries.len() > crate::browser_backend::MAX_STORAGE_ENTRIES {
                return Err(NativeEngineError::limit(
                    "native Web Storage entries",
                    crate::browser_backend::MAX_STORAGE_ENTRIES,
                    entries.len(),
                ));
            }
            for (key, value) in entries {
                if key.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Web Storage key",
                        crate::browser_backend::MAX_BACKEND_ID_BYTES,
                        key.len(),
                    ));
                }
                if value.len() > crate::browser_backend::MAX_TEXT_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Web Storage value",
                        crate::browser_backend::MAX_TEXT_BYTES,
                        value.len(),
                    ));
                }
            }
        }
    }
    Ok(())
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
        node_index: Option<u32>,
    },
    Module {
        name: String,
        source: String,
        node_index: Option<u32>,
    },
    ModuleDependency {
        name: String,
        source: String,
    },
}

struct NativeModuleResolver;

impl Resolver for NativeModuleResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &rquickjs::Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        let base = Url::parse(base)
            .map_err(|_| Error::new_resolving_message(base, name, "module base is not a URL"))?;
        let target = Url::parse(name).or_else(|_| base.join(name)).map_err(|_| {
            Error::new_resolving_message(base.as_str(), name, "module URL is invalid")
        })?;
        if !target.username().is_empty() || target.password().is_some() {
            return Err(Error::new_resolving_message(
                base.as_str(),
                name,
                "module URL must not contain credentials",
            ));
        }
        let mut target = target;
        target.set_fragment(None);
        Ok(target.to_string())
    }
}

struct NativeModuleLoader {
    sources: Arc<Mutex<BTreeMap<String, String>>>,
}

impl Loader for NativeModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &rquickjs::Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js>> {
        let source = self
            .sources
            .lock()
            .ok()
            .and_then(|sources| sources.get(name).cloned())
            .ok_or_else(|| Error::new_loading_message(name, "module was not prefetched"))?;
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

/// Execute the bounded inline scripts discovered in one parsed document.
///
/// The caller owns the realm so local documents and the child content process
/// can both retain globals and listeners after the document commit. Script
/// navigation is returned as a typed handoff for the current navigation owner;
/// it is never executed recursively inside the JavaScript evaluator.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_inline_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    context_id: &str,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    storage_state: &NativeWebStorageState,
    indexed_db_state: &NativeIndexedDbState,
    cookie: &str,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let sources = document
        .page_script_sources(MAX_NATIVE_INLINE_SCRIPTS, MAX_NATIVE_SCRIPT_BYTES)
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| match source {
            NativePageScriptSource::Inline {
                source,
                timing,
                node_index,
            } => Some((
                timing,
                NativePageScript::Classic {
                    source,
                    node_index: Some(node_index),
                },
            )),
            NativePageScriptSource::ModuleInline {
                source,
                timing,
                node_index,
            } => Some((
                timing,
                NativePageScript::Module {
                    name: format!("{document_url}#glass-inline-module-{index}"),
                    source,
                    node_index: Some(node_index),
                },
            )),
            NativePageScriptSource::External { .. }
            | NativePageScriptSource::ModuleExternal { .. } => None,
        })
        .collect::<Vec<_>>();
    let sources = order_page_scripts(sources);
    execute_page_scripts(
        document,
        runtime,
        context_id,
        &sources,
        document_url,
        document_origin,
        viewport,
        storage_state,
        indexed_db_state,
        cookie,
        &[],
    )
}

pub(crate) fn execute_page_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    context_id: &str,
    sources: &[NativePageScript],
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    storage_state: &NativeWebStorageState,
    indexed_db_state: &NativeIndexedDbState,
    cookie: &str,
    resource_events: &[(u32, NativeEventKind)],
) -> Result<NativePageScriptResult, NativeEngineError> {
    document.mark_attached_scripts_started();
    if runtime.is_none() {
        *runtime = Some(NativeJavaScriptRuntime::new_with_context_id(context_id)?);
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_storage_state(storage_state.clone());
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_indexed_db_state(indexed_db_state.origin(&storage_key(document_url, document_origin)));
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_cookie_state(cookie.to_owned());
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_timer_pump_enabled(false);
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("loading");
    let module_sources = sources
        .iter()
        .filter_map(|source| match source {
            NativePageScript::Module { name, source, .. }
            | NativePageScript::ModuleDependency { name, source } => {
                Some((name.clone(), source.clone()))
            }
            NativePageScript::Classic { .. } => None,
        })
        .collect::<BTreeMap<_, _>>();
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_module_sources(module_sources);
    let mut pending_fetches = Vec::new();
    let mut websocket_commands = Vec::new();
    let mut event_source_commands = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut navigation = None;
    let mut events = Vec::new();
    let mut failed_script_nodes = BTreeSet::new();
    for source in sources {
        let script_node_index = match source {
            NativePageScript::Classic { node_index, .. }
            | NativePageScript::Module { node_index, .. } => *node_index,
            NativePageScript::ModuleDependency { .. } => None,
        };
        let evaluation = {
            let script_runtime = runtime.as_ref().expect("page script runtime initialized");
            match source {
                NativePageScript::Classic { source, .. } => script_runtime.evaluate(
                    source,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                ),
                NativePageScript::Module { name, source, .. } => script_runtime.evaluate_module(
                    name,
                    source,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                ),
                NativePageScript::ModuleDependency { .. } => continue,
            }
        };
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) if is_ignorable_page_script_error(&error) => {
                if let Some(node_index) = script_node_index {
                    failed_script_nodes.insert(node_index);
                    let message = page_script_error_message(&error);
                    if let Some(event_source) =
                        host_script_error_event_script(Some(node_index), &message, document_url)?
                    {
                        let error_evaluation = runtime
                            .as_ref()
                            .expect("page script runtime initialized")
                            .evaluate(
                                &event_source,
                                document,
                                document_url,
                                document_origin,
                                viewport,
                            )?;
                        apply_page_script_evaluation(
                            document,
                            error_evaluation,
                            &mut pending_fetches,
                            &mut websocket_commands,
                            &mut event_source_commands,
                            &mut scroll_commands,
                            &mut navigation,
                        )?;
                        events.push((node_index, NativeEventKind::Error));
                    }
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        apply_page_script_evaluation(
            document,
            evaluation,
            &mut pending_fetches,
            &mut websocket_commands,
            &mut event_source_commands,
            &mut scroll_commands,
            &mut navigation,
        )?;
    }
    for (node_index, event_kind) in resource_events {
        if *event_kind == NativeEventKind::Load && failed_script_nodes.contains(node_index) {
            continue;
        }
        let Some(event_source) = host_event_script(&[(*node_index, *event_kind)])? else {
            continue;
        };
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate(
                &event_source,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        apply_page_script_evaluation(
            document,
            evaluation,
            &mut pending_fetches,
            &mut websocket_commands,
            &mut event_source_commands,
            &mut scroll_commands,
            &mut navigation,
        )?;
        events.push((*node_index, *event_kind));
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("interactive");
    for (target, kind) in [
        (0, NativeEventKind::ReadyStateChange),
        (0, NativeEventKind::DomContentLoaded),
    ] {
        let Some(event_source) = host_event_script(&[(target, kind)])? else {
            continue;
        };
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate(
                &event_source,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        apply_page_script_evaluation(
            document,
            evaluation,
            &mut pending_fetches,
            &mut websocket_commands,
            &mut event_source_commands,
            &mut scroll_commands,
            &mut navigation,
        )?;
        events.push((target, kind));
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("complete");
    for (target, kind) in [
        (0, NativeEventKind::ReadyStateChange),
        (u32::MAX, NativeEventKind::Load),
    ] {
        let Some(event_source) = host_event_script(&[(target, kind)])? else {
            continue;
        };
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate(
                &event_source,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        apply_page_script_evaluation(
            document,
            evaluation,
            &mut pending_fetches,
            &mut websocket_commands,
            &mut event_source_commands,
            &mut scroll_commands,
            &mut navigation,
        )?;
        events.push((target, kind));
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_timer_pump_enabled(true);
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .reset_timer_clock();
    dispatch_page_scroll_events(
        document,
        runtime.as_ref().expect("page script runtime initialized"),
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut pending_fetches,
        &mut websocket_commands,
        &mut event_source_commands,
        &mut navigation,
        &mut events,
    )?;
    Ok(NativePageScriptResult {
        pending_fetches,
        websocket_commands,
        event_source_commands,
        pending_script_sources: Vec::new(),
        scroll_commands,
        navigation,
        dialogs: runtime
            .as_ref()
            .expect("page script runtime initialized")
            .take_dialog_events(),
        events,
    })
}

/// Execute scripts attached by a completed DOM mutation. Unlike the initial
/// page loader this path does not replay document lifecycle events; it only
/// runs each newly started script and the resource event associated with it.
/// The document owns the single-shot ledger, so a script created in one host
/// batch and attached in a later batch still executes exactly once.
pub(crate) fn execute_dynamic_page_scripts(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    sources: Vec<NativePageScript>,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    resource_events: &[(u32, NativeEventKind)],
) -> Result<NativePageScriptResult, NativeEngineError> {
    let module_sources = sources
        .iter()
        .filter_map(|source| match source {
            NativePageScript::Module { name, source, .. }
            | NativePageScript::ModuleDependency { name, source } => {
                Some((name.clone(), source.clone()))
            }
            NativePageScript::Classic { .. } => None,
        })
        .collect::<BTreeMap<_, _>>();
    runtime.set_module_sources(module_sources);

    let mut pending = VecDeque::from(sources);
    let mut pending_fetches = Vec::new();
    let mut websocket_commands = Vec::new();
    let mut event_source_commands = Vec::new();
    let mut pending_script_sources = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut navigation = None;
    let mut events = Vec::new();
    let mut failed_script_nodes = BTreeSet::new();
    let mut executed = 0usize;

    while let Some(source) = pending.pop_front() {
        if matches!(&source, NativePageScript::ModuleDependency { .. }) {
            continue;
        }
        executed = executed.saturating_add(1);
        if executed > MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native dynamic page scripts",
                MAX_NATIVE_INLINE_SCRIPTS,
                executed,
            ));
        }
        let node_index = match &source {
            NativePageScript::Classic { node_index, .. }
            | NativePageScript::Module { node_index, .. } => *node_index,
            NativePageScript::ModuleDependency { .. } => None,
        };
        let evaluation = match &source {
            NativePageScript::Classic { source, .. } => {
                runtime.evaluate(source, document, document_url, document_origin, viewport)
            }
            NativePageScript::Module { name, source, .. } => runtime.evaluate_module(
                name,
                source,
                document,
                document_url,
                document_origin,
                viewport,
            ),
            NativePageScript::ModuleDependency { .. } => continue,
        };
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) if is_ignorable_page_script_error(&error) => {
                if let Some(node_index) = node_index {
                    failed_script_nodes.insert(node_index);
                    let message = page_script_error_message(&error);
                    if let Some(event_source) =
                        host_script_error_event_script(Some(node_index), &message, document_url)?
                    {
                        let error_evaluation = runtime.evaluate(
                            &event_source,
                            document,
                            document_url,
                            document_origin,
                            viewport,
                        )?;
                        let commands = error_evaluation.commands.clone();
                        apply_page_script_evaluation(
                            document,
                            error_evaluation,
                            &mut pending_fetches,
                            &mut websocket_commands,
                            &mut event_source_commands,
                            &mut scroll_commands,
                            &mut navigation,
                        )?;
                        enqueue_dynamic_page_scripts(
                            document,
                            &commands,
                            document_url,
                            &mut pending,
                            &mut pending_script_sources,
                        )?;
                        events.push((node_index, NativeEventKind::Error));
                    }
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let commands = evaluation.commands.clone();
        apply_page_script_evaluation(
            document,
            evaluation,
            &mut pending_fetches,
            &mut websocket_commands,
            &mut event_source_commands,
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending,
            &mut pending_script_sources,
        )?;
    }

    for (node_index, event_kind) in resource_events {
        if *event_kind == NativeEventKind::Load && failed_script_nodes.contains(node_index) {
            continue;
        }
        let Some(event_source) = host_event_script(&[(*node_index, *event_kind)])? else {
            continue;
        };
        let evaluation = runtime.evaluate(
            &event_source,
            document,
            document_url,
            document_origin,
            viewport,
        )?;
        let commands = evaluation.commands.clone();
        apply_page_script_evaluation(
            document,
            evaluation,
            &mut pending_fetches,
            &mut websocket_commands,
            &mut event_source_commands,
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending,
            &mut pending_script_sources,
        )?;
        events.push((*node_index, *event_kind));
    }

    while let Some(source) = pending.pop_front() {
        if matches!(&source, NativePageScript::ModuleDependency { .. }) {
            continue;
        }
        executed = executed.saturating_add(1);
        if executed > MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native dynamic page scripts",
                MAX_NATIVE_INLINE_SCRIPTS,
                executed,
            ));
        }
        let node_index = match &source {
            NativePageScript::Classic { node_index, .. }
            | NativePageScript::Module { node_index, .. } => *node_index,
            NativePageScript::ModuleDependency { .. } => None,
        };
        let evaluation = match &source {
            NativePageScript::Classic { source, .. } => {
                runtime.evaluate(source, document, document_url, document_origin, viewport)
            }
            NativePageScript::Module { name, source, .. } => runtime.evaluate_module(
                name,
                source,
                document,
                document_url,
                document_origin,
                viewport,
            ),
            NativePageScript::ModuleDependency { .. } => continue,
        };
        let evaluation = match evaluation {
            Ok(evaluation) => evaluation,
            Err(error) if is_ignorable_page_script_error(&error) => {
                if let Some(node_index) = node_index {
                    let message = page_script_error_message(&error);
                    if let Some(event_source) =
                        host_script_error_event_script(Some(node_index), &message, document_url)?
                    {
                        let error_evaluation = runtime.evaluate(
                            &event_source,
                            document,
                            document_url,
                            document_origin,
                            viewport,
                        )?;
                        let commands = error_evaluation.commands.clone();
                        apply_page_script_evaluation(
                            document,
                            error_evaluation,
                            &mut pending_fetches,
                            &mut websocket_commands,
                            &mut event_source_commands,
                            &mut scroll_commands,
                            &mut navigation,
                        )?;
                        enqueue_dynamic_page_scripts(
                            document,
                            &commands,
                            document_url,
                            &mut pending,
                            &mut pending_script_sources,
                        )?;
                        events.push((node_index, NativeEventKind::Error));
                    }
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let commands = evaluation.commands.clone();
        apply_page_script_evaluation(
            document,
            evaluation,
            &mut pending_fetches,
            &mut websocket_commands,
            &mut event_source_commands,
            &mut scroll_commands,
            &mut navigation,
        )?;
        enqueue_dynamic_page_scripts(
            document,
            &commands,
            document_url,
            &mut pending,
            &mut pending_script_sources,
        )?;
    }

    Ok(NativePageScriptResult {
        pending_fetches,
        websocket_commands,
        event_source_commands,
        pending_script_sources,
        scroll_commands,
        navigation,
        dialogs: runtime.take_dialog_events(),
        events,
    })
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
    let deferred_sources = sources
        .iter()
        .filter(|source| {
            matches!(
                source,
                NativePageScriptSource::External { .. }
                    | NativePageScriptSource::ModuleExternal { .. }
            )
        })
        .cloned()
        .collect::<Vec<_>>();
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
                node_index: Some(node_index),
            }),
            NativePageScriptSource::ModuleInline {
                source, node_index, ..
            } => Some(NativePageScript::Module {
                name: format!("{document_url}#{module_name_prefix}-{node_index}-{index}"),
                source,
                node_index: Some(node_index),
            }),
            NativePageScriptSource::External { .. }
            | NativePageScriptSource::ModuleExternal { .. } => None,
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

fn is_ignorable_page_script_error(error: &NativeEngineError) -> bool {
    matches!(
        error,
        NativeEngineError::Worker { operation, .. }
            if operation == "evaluate JavaScript"
                || operation == "evaluate JavaScript module"
    )
}

fn apply_page_script_evaluation(
    document: &mut NativeDocument,
    evaluation: NativeScriptEvaluation,
    pending_fetches: &mut Vec<NativeScriptCommand>,
    websocket_commands: &mut Vec<NativeScriptCommand>,
    event_source_commands: &mut Vec<NativeScriptCommand>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
    navigation: &mut Option<NativePageNavigation>,
) -> Result<(), NativeEngineError> {
    let mut commands = Vec::new();
    for command in evaluation.commands {
        match command {
            command @ NativeScriptCommand::Fetch { .. } => pending_fetches.push(command),
            command @ (NativeScriptCommand::WebSocketOpen { .. }
            | NativeScriptCommand::WebSocketSend { .. }
            | NativeScriptCommand::WebSocketClose { .. }) => websocket_commands.push(command),
            command @ (NativeScriptCommand::EventSourceOpen { .. }
            | NativeScriptCommand::EventSourceClose { .. }) => event_source_commands.push(command),
            NativeScriptCommand::Navigate { href, replace } => {
                validate_url_text("page script navigation href", &href)?;
                if navigation.is_some() {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "one page-script batch cannot activate multiple navigations".into(),
                    });
                }
                *navigation = Some(NativePageNavigation {
                    href,
                    replace_history: replace,
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
        return Ok(());
    }
    let mut next = document.clone();
    next.apply_script_commands(&commands)?;
    *document = next;
    Ok(())
}

fn dispatch_page_scroll_events(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    scroll_commands: &mut Vec<NativeScriptCommand>,
    pending_fetches: &mut Vec<NativeScriptCommand>,
    websocket_commands: &mut Vec<NativeScriptCommand>,
    event_source_commands: &mut Vec<NativeScriptCommand>,
    navigation: &mut Option<NativePageNavigation>,
    events: &mut Vec<(u32, NativeEventKind)>,
) -> Result<(), NativeEngineError> {
    let mut pending = std::mem::take(scroll_commands);
    let mut cursor = 0;
    while cursor < pending.len() {
        let NativeScriptCommand::ScrollTo { node_index, .. } = pending[cursor] else {
            cursor += 1;
            continue;
        };
        let event_node_index = (node_index != 0).then_some(node_index).unwrap_or(u32::MAX);
        let source = host_event_script(&[(event_node_index, NativeEventKind::Scroll)])?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native page scroll event".into(),
                reason: "native page scroll event source was empty".into(),
            })?;
        let evaluation =
            runtime.evaluate(&source, document, document_url, document_origin, viewport)?;
        let mut emitted_scroll_commands = Vec::new();
        apply_page_script_evaluation(
            document,
            evaluation,
            pending_fetches,
            websocket_commands,
            event_source_commands,
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

/// Build the internal source used to deliver Rust-owned semantic events into
/// the persistent page realm. The source is generated from typed, bounded
/// event metadata and never contains page-provided strings.
pub(crate) fn host_event_script(
    events: &[(u32, NativeEventKind)],
) -> Result<Option<String>, NativeEngineError> {
    let events = events
        .iter()
        .map(|(node_index, kind)| (*node_index, *kind, None))
        .collect::<Vec<_>>();
    host_event_script_with_submitters(&events)
}

/// Build the internal source used to report an uncaught page-script failure.
/// The event metadata is serialized as data, and the host creates the
/// `ErrorEvent` plus its bounded `Error` value inside the page realm.
pub(crate) fn host_script_error_event_script(
    node_index: Option<u32>,
    message: &str,
    filename: &str,
) -> Result<Option<String>, NativeEngineError> {
    let descriptor = serde_json::json!({
        "node_index": node_index,
        "message": message,
        "filename": filename,
        "lineno": 0,
        "colno": 0,
    });
    let encoded = serde_json::to_string(&descriptor).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native script error event".into(),
        reason: "native script error event metadata could not be serialized".into(),
    })?;
    let source = format!("globalThis.__glassDispatchScriptError({encoded})");
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native script error event",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

/// Build the internal source used to report bounded unhandled Promise
/// rejections into the persistent page realm after a microtask checkpoint.
/// The rejection reason is intentionally transported as text until the native
/// realm has a complete structured-reason bridge.
fn promise_rejection_event_script(
    event_type: &str,
    reasons: &[String],
    cancelable: bool,
) -> Result<Option<String>, NativeEngineError> {
    if reasons.is_empty() {
        return Ok(None);
    }
    let descriptors = reasons
        .iter()
        .map(|reason| serde_json::json!({ "reason": reason }))
        .collect::<Vec<_>>();
    let encoded = serde_json::to_string(&descriptors).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native Promise rejection events".into(),
        reason: "native Promise rejection metadata could not be serialized".into(),
    })?;
    let event_type = serde_json::to_string(event_type).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native Promise rejection event type".into(),
        reason: "native Promise rejection event type could not be serialized".into(),
    })?;
    let source = format!(
        "globalThis.__glassDispatchPromiseRejections({event_type}, {encoded}, {cancelable})"
    );
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native Promise rejection events",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

fn bounded_unhandled_promise_rejection_reason(reason: String) -> String {
    reason
        .chars()
        .take(MAX_NATIVE_UNHANDLED_REJECTION_REASON_CHARS)
        .collect()
}

/// Build the internal source used to project events from a child browsing
/// context into its same-origin parent realm. The child node identity remains
/// data; the receiving realm resolves it against its own immutable binding
/// snapshot before dispatching the event.
pub(crate) fn frame_event_script(
    frame_id: &str,
    events: &[(u32, u32, NativeEventKind)],
) -> Result<Option<String>, NativeEngineError> {
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
    let descriptors = events
        .iter()
        .map(|(node_index, generation, kind)| {
            let (event_type, bubbles, cancelable) = match kind {
                NativeEventKind::Blur => ("blur", false, false),
                NativeEventKind::Focus => ("focus", false, false),
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
            serde_json::json!({
                "node_index": node_index,
                "generation": generation,
                "type": event_type,
                "bubbles": bubbles,
                "cancelable": cancelable,
                "persisted": false,
            })
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_string(&descriptors).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native frame event dispatch".into(),
        reason: "native frame event metadata could not be serialized".into(),
    })?;
    let frame_id = serde_json::to_string(frame_id).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native frame event dispatch".into(),
        reason: "native frame event context could not be serialized".into(),
    })?;
    let source = format!("globalThis.__glassDispatchFrameEvents({frame_id}, {encoded})");
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native frame event dispatch",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

pub(crate) fn host_submit_event_script(
    form_index: u32,
    submitter_index: Option<u32>,
) -> Result<Option<String>, NativeEngineError> {
    host_event_script_with_submitters(&[(form_index, NativeEventKind::Submit, submitter_index)])
}

pub(crate) fn host_hash_change_event_script(
    old_url: &str,
    new_url: &str,
) -> Result<Option<String>, NativeEngineError> {
    let old_url = serde_json::to_string(old_url).map_err(|_| NativeEngineError::Worker {
        operation: "serialize hashchange event".into(),
        reason: "hashchange old URL could not be serialized".into(),
    })?;
    let new_url = serde_json::to_string(new_url).map_err(|_| NativeEngineError::Worker {
        operation: "serialize hashchange event".into(),
        reason: "hashchange new URL could not be serialized".into(),
    })?;
    let source = format!(
        "globalThis.__glassDispatchHostEvents([{{\"node_index\":4294967295,\"type\":\"hashchange\",\"bubbles\":false,\"cancelable\":false,\"old_url\":{old_url},\"new_url\":{new_url}}}])"
    );
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "hashchange event",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

/// Build the internal source used to deliver a cross-context `message` event.
/// All page-controlled values are serialized as JSON data before entering the
/// generated source; the receiving realm never evaluates them as JavaScript.
pub(crate) fn host_message_event_script(
    source_context_id: &str,
    source_origin: &str,
    data: &serde_json::Value,
) -> Result<Option<String>, NativeEngineError> {
    validate_context_id(source_context_id)?;
    validate_url_text("message source origin", source_origin)?;
    let data = serde_json::to_string(data).map_err(|_| NativeEngineError::Worker {
        operation: "serialize message event".into(),
        reason: "message data could not be serialized".into(),
    })?;
    if data.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
        return Err(NativeEngineError::limit(
            "message event data",
            MAX_NATIVE_POST_MESSAGE_BYTES,
            data.len(),
        ));
    }
    let source_context_id =
        serde_json::to_string(source_context_id).map_err(|_| NativeEngineError::Worker {
            operation: "serialize message event".into(),
            reason: "message source context could not be serialized".into(),
        })?;
    let source_origin =
        serde_json::to_string(source_origin).map_err(|_| NativeEngineError::Worker {
            operation: "serialize message event".into(),
            reason: "message source origin could not be serialized".into(),
        })?;
    let source = format!(
        "globalThis.__glassDispatchMessage({{\"source_context_id\":{source_context_id},\"source_origin\":{source_origin},\"data\":{data}}})"
    );
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "message event",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

fn host_event_script_with_submitters(
    events: &[(u32, NativeEventKind, Option<u32>)],
) -> Result<Option<String>, NativeEngineError> {
    if events.is_empty() {
        return Ok(None);
    }
    let descriptors = events
        .iter()
        .map(|(node_index, kind, submitter_index)| {
            let (event_type, bubbles, cancelable) = match kind {
                NativeEventKind::Blur => ("blur", false, false),
                NativeEventKind::Focus => ("focus", false, false),
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
            serde_json::json!({
                "node_index": node_index,
                "type": event_type,
                "bubbles": bubbles,
                "cancelable": cancelable,
                "persisted": false,
                "submitter_node_index": submitter_index,
            })
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_string(&descriptors).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native event dispatch".into(),
        reason: "native event dispatch metadata could not be serialized".into(),
    })?;
    let source = format!("globalThis.__glassDispatchHostEvents({encoded})");
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native event dispatch",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

pub(crate) fn host_key_event_script(
    node_index: u32,
    kind: NativeEventKind,
    key: &str,
) -> Result<Option<String>, NativeEngineError> {
    host_key_event_script_with_modifiers(node_index, kind, key, 0)
}

pub(crate) fn host_key_event_script_with_modifiers(
    node_index: u32,
    kind: NativeEventKind,
    key: &str,
    modifiers: i64,
) -> Result<Option<String>, NativeEngineError> {
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
    let descriptors = serde_json::json!([{
        "node_index": node_index,
        "type": event_type,
        "bubbles": bubbles,
        "cancelable": cancelable,
        "key": key,
        "code": code,
        "alt_key": modifiers & 1 != 0,
        "ctrl_key": modifiers & 2 != 0,
        "meta_key": modifiers & 4 != 0,
        "shift_key": modifiers & 8 != 0,
    }]);
    let encoded = serde_json::to_string(&descriptors).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native key event dispatch".into(),
        reason: "native key event metadata could not be serialized".into(),
    })?;
    let source = format!("globalThis.__glassDispatchHostEvents({encoded})");
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native key event dispatch",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
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

pub(crate) struct NativeJavaScriptRuntime {
    runtime: Runtime,
    context: Context,
    deadline: Arc<Mutex<Option<Instant>>>,
    module_sources: Arc<Mutex<BTreeMap<String, String>>>,
    timer_pump_enabled: Arc<Mutex<bool>>,
    storage: Arc<Mutex<NativeWebStorageState>>,
    indexed_db: Arc<Mutex<NativeIndexedDbOrigin>>,
    storage_changes: Arc<Mutex<Vec<NativeStorageEvent>>>,
    pending_storage_events: Arc<Mutex<Vec<NativeStorageEvent>>>,
    cookie: Arc<Mutex<String>>,
    cookie_updates: Arc<Mutex<Vec<String>>>,
    unhandled_promise_rejections: Arc<Mutex<NativeUnhandledPromiseRejections>>,
    dialog_events: Arc<Mutex<Vec<NativeDialog>>>,
    popup_events: Arc<Mutex<Vec<NativePopupRequest>>>,
    post_message_events: Arc<Mutex<Vec<NativePostMessageRequest>>>,
    window_close_events: Arc<Mutex<Vec<NativeWindowCloseRequest>>>,
    window_navigation_events: Arc<Mutex<Vec<NativeWindowNavigationRequest>>>,
    frame_script_events: Arc<Mutex<Vec<NativeFrameScriptRequest>>>,
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

impl NativeJavaScriptRuntime {
    pub(crate) fn new_with_context_id(
        context_id: impl Into<String>,
    ) -> Result<Self, NativeEngineError> {
        Self::new_with_context_metadata(context_id, "", None, "", "")
    }

    pub(crate) fn new_with_context_metadata(
        context_id: impl Into<String>,
        window_name: impl Into<String>,
        opener_context_id: Option<&str>,
        opener_window_name: impl Into<String>,
        opener_url: impl Into<String>,
    ) -> Result<Self, NativeEngineError> {
        let context_id = context_id.into();
        let window_name = window_name.into();
        let opener_window_name = opener_window_name.into();
        let opener_url = opener_url.into();
        validate_context_id(&context_id)?;
        validate_window_name(&window_name)?;
        validate_window_name(&opener_window_name)?;
        if !opener_url.is_empty() {
            validate_url_text("opener URL", &opener_url)?;
        }
        if let Some(opener_context_id) = opener_context_id {
            validate_context_id(opener_context_id)?;
        }
        let runtime = Runtime::new().map_err(|_| NativeEngineError::Worker {
            operation: "create JavaScript runtime".into(),
            reason: "native JavaScript runtime could not be created".into(),
        })?;
        let module_sources = Arc::new(Mutex::new(BTreeMap::new()));
        runtime.set_loader(
            NativeModuleResolver,
            NativeModuleLoader {
                sources: Arc::clone(&module_sources),
            },
        );
        runtime.set_memory_limit(NATIVE_SCRIPT_MEMORY_BYTES);
        runtime.set_max_stack_size(NATIVE_SCRIPT_STACK_BYTES);
        let deadline = Arc::new(Mutex::new(None));
        let interrupt_deadline = Arc::clone(&deadline);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupt_deadline
                .lock()
                .ok()
                .and_then(|deadline| *deadline)
                .is_some_and(|deadline| Instant::now() >= deadline)
        })));
        let unhandled_promise_rejections =
            Arc::new(Mutex::new(NativeUnhandledPromiseRejections::default()));
        let rejection_queue = Arc::clone(&unhandled_promise_rejections);
        runtime.set_host_promise_rejection_tracker(Some(Box::new(
            move |ctx, promise, reason, is_handled| {
                let key = format!("{promise:?}");
                let fallback = format!("{reason:?}");
                let reason = Coerced::<std::string::String>::from_js(&ctx, reason)
                    .map(|value| value.0)
                    .unwrap_or(fallback);
                let reason = bounded_unhandled_promise_rejection_reason(reason);
                if let Ok(mut queue) = rejection_queue.lock() {
                    if is_handled {
                        if queue.pending.remove(&key).is_none()
                            && let Some((_, reason)) = queue.reported.remove(&key)
                            && queue.handled.len() < MAX_NATIVE_UNHANDLED_REJECTIONS
                        {
                            let order = queue.next_order;
                            queue.next_order = queue.next_order.saturating_add(1);
                            queue.handled.insert(key, (order, reason));
                        }
                    } else if queue.pending.len() < MAX_NATIVE_UNHANDLED_REJECTIONS
                        || queue.pending.contains_key(&key)
                    {
                        let order = queue
                            .pending
                            .get(&key)
                            .map(|(order, _)| *order)
                            .unwrap_or_else(|| {
                                let order = queue.next_order;
                                queue.next_order = queue.next_order.saturating_add(1);
                                order
                            });
                        queue.pending.insert(key, (order, reason));
                    }
                }
            },
        )));
        let context = Context::full(&runtime).map_err(|_| NativeEngineError::Worker {
            operation: "create JavaScript context".into(),
            reason: "native JavaScript context could not be created".into(),
        })?;
        Ok(Self {
            runtime,
            context,
            deadline,
            module_sources,
            timer_pump_enabled: Arc::new(Mutex::new(true)),
            storage: Arc::new(Mutex::new(NativeWebStorageState::default())),
            indexed_db: Arc::new(Mutex::new(NativeIndexedDbOrigin::default())),
            storage_changes: Arc::new(Mutex::new(Vec::new())),
            pending_storage_events: Arc::new(Mutex::new(Vec::new())),
            cookie: Arc::new(Mutex::new(String::new())),
            cookie_updates: Arc::new(Mutex::new(Vec::new())),
            unhandled_promise_rejections,
            dialog_events: Arc::new(Mutex::new(Vec::new())),
            popup_events: Arc::new(Mutex::new(Vec::new())),
            post_message_events: Arc::new(Mutex::new(Vec::new())),
            window_close_events: Arc::new(Mutex::new(Vec::new())),
            window_navigation_events: Arc::new(Mutex::new(Vec::new())),
            frame_script_events: Arc::new(Mutex::new(Vec::new())),
            pending_window_proxy_updates: Arc::new(Mutex::new(Vec::new())),
            frame_script_bindings: Arc::new(Mutex::new(Vec::new())),
            frame_script_context: Arc::new(Mutex::new(None)),
            frame_id: Arc::new(Mutex::new(context_id.clone())),
            scroll_offset: Arc::new(Mutex::new(NativePoint { x: 0, y: 0 })),
            nested_scroll_offsets: Arc::new(Mutex::new(BTreeMap::new())),
            history_state: Arc::new(Mutex::new(serde_json::Value::Null)),
            history_length: Arc::new(Mutex::new(1)),
            window_name: Arc::new(Mutex::new(window_name)),
            opener_context_id: opener_context_id.map(str::to_owned),
            opener_window_name,
            opener_url,
            storage_context_id: context_id,
            ready_state: "complete".into(),
            clock_origin: Instant::now(),
        })
    }

    pub(crate) fn window_name(&self) -> String {
        self.window_name
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    pub(crate) fn opener_context_id(&self) -> Option<&str> {
        self.opener_context_id.as_deref()
    }

    pub(crate) fn opener_window_name(&self) -> &str {
        &self.opener_window_name
    }

    pub(crate) fn set_storage_state(&self, state: NativeWebStorageState) {
        if let Ok(mut current) = self.storage.lock() {
            *current = state;
        }
    }

    pub(crate) fn replace_storage_state(&self, state: NativeWebStorageState) {
        self.set_storage_state(state);
        if let Ok(mut pending) = self.pending_storage_events.lock() {
            pending.clear();
        }
    }

    pub(crate) fn storage_state(&self) -> NativeWebStorageState {
        self.storage
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_indexed_db_state(&self, state: NativeIndexedDbOrigin) {
        if state.validate().is_err() {
            return;
        }
        if let Ok(mut current) = self.indexed_db.lock() {
            *current = state;
        }
    }

    pub(crate) fn set_timer_pump_enabled(&self, enabled: bool) {
        if let Ok(mut current) = self.timer_pump_enabled.lock() {
            *current = enabled;
        }
    }

    fn timer_pump_enabled(&self) -> bool {
        self.timer_pump_enabled
            .lock()
            .map(|enabled| *enabled)
            .unwrap_or(true)
    }

    pub(crate) fn indexed_db_state(&self) -> NativeIndexedDbOrigin {
        self.indexed_db
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    fn take_unhandled_promise_rejections(&self) -> Vec<String> {
        let Ok(mut queue) = self.unhandled_promise_rejections.lock() else {
            return Vec::new();
        };
        let mut values = std::mem::take(&mut queue.pending)
            .into_iter()
            .collect::<Vec<_>>();
        values.sort_unstable_by_key(|(_, (order, _))| *order);
        let mut reasons = Vec::with_capacity(values.len());
        for (key, (order, reason)) in values {
            if queue.reported.len() >= MAX_NATIVE_UNHANDLED_REJECTIONS
                && let Some(key) = queue
                    .reported
                    .iter()
                    .min_by_key(|(_, (reported_order, _))| *reported_order)
                    .map(|(key, _)| key.clone())
            {
                queue.reported.remove(&key);
            }
            queue.reported.insert(key, (order, reason.clone()));
            reasons.push(reason);
        }
        reasons
    }

    fn take_handled_promise_rejections(&self) -> Vec<String> {
        let Ok(mut queue) = self.unhandled_promise_rejections.lock() else {
            return Vec::new();
        };
        let mut values = std::mem::take(&mut queue.handled)
            .into_values()
            .collect::<Vec<_>>();
        values.sort_unstable_by_key(|(order, _)| *order);
        values.into_iter().map(|(_, reason)| reason).collect()
    }

    pub(crate) fn take_storage_changes(&self) -> Vec<NativeStorageEvent> {
        self.storage_changes
            .lock()
            .map(|mut changes| std::mem::take(&mut *changes))
            .unwrap_or_default()
    }

    pub(crate) fn set_storage_events(
        &self,
        events: Vec<NativeStorageEvent>,
    ) -> Result<(), NativeEngineError> {
        if events.is_empty() {
            return Ok(());
        }
        let mut pending =
            self.pending_storage_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue native storage events".into(),
                    reason: "native storage event queue is unavailable".into(),
                })?;
        let next_len = pending.len().saturating_add(events.len());
        if next_len > MAX_NATIVE_STORAGE_EVENTS {
            return Err(NativeEngineError::limit(
                "native storage events",
                MAX_NATIVE_STORAGE_EVENTS,
                next_len,
            ));
        }
        pending.extend(events);
        Ok(())
    }

    fn take_storage_events(&self) -> Vec<NativeStorageEvent> {
        self.pending_storage_events
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default()
    }

    pub(crate) fn set_cookie_state(&self, value: impl Into<String>) {
        let value = value.into();
        if value.len() > crate::browser_backend::MAX_TEXT_BYTES {
            return;
        }
        if let Ok(mut current) = self.cookie.lock() {
            *current = value;
        }
    }

    fn cookie_state(&self) -> String {
        self.cookie
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    pub(crate) fn take_cookie_updates(&self) -> Vec<String> {
        self.cookie_updates
            .lock()
            .map(|mut updates| std::mem::take(&mut *updates))
            .unwrap_or_default()
    }

    pub(crate) fn take_dialog_events(&self) -> Vec<NativeDialog> {
        self.dialog_events
            .lock()
            .map(|mut dialogs| std::mem::take(&mut *dialogs))
            .unwrap_or_default()
    }

    pub(crate) fn take_popup_events(&self) -> Vec<NativePopupRequest> {
        self.popup_events
            .lock()
            .map(|mut popups| std::mem::take(&mut *popups))
            .unwrap_or_default()
    }

    pub(crate) fn take_post_message_events(&self) -> Vec<NativePostMessageRequest> {
        self.post_message_events
            .lock()
            .map(|mut messages| std::mem::take(&mut *messages))
            .unwrap_or_default()
    }

    pub(crate) fn take_window_close_events(&self) -> Vec<NativeWindowCloseRequest> {
        self.window_close_events
            .lock()
            .map(|mut requests| std::mem::take(&mut *requests))
            .unwrap_or_default()
    }

    pub(crate) fn take_window_navigation_events(&self) -> Vec<NativeWindowNavigationRequest> {
        self.window_navigation_events
            .lock()
            .map(|mut requests| std::mem::take(&mut *requests))
            .unwrap_or_default()
    }

    pub(crate) fn take_frame_script_events(&self) -> Vec<NativeFrameScriptRequest> {
        self.frame_script_events
            .lock()
            .map(|mut requests| std::mem::take(&mut *requests))
            .unwrap_or_default()
    }

    pub(crate) fn set_frame_script_bindings(&self, bindings: Vec<NativeFrameScriptBinding>) {
        if let Ok(mut current) = self.frame_script_bindings.lock() {
            *current = bindings;
        }
    }

    pub(crate) fn set_frame_script_context(&self, context: Option<NativeFrameScriptContext>) {
        if let Ok(mut current) = self.frame_script_context.lock() {
            *current = context;
        }
    }

    pub(crate) fn set_frame_id(&self, frame_id: String) {
        if validate_context_id(&frame_id).is_err() {
            return;
        }
        if let Ok(mut current) = self.frame_id.lock() {
            *current = frame_id;
        }
    }

    pub(crate) fn set_scroll_offset(&self, scroll_offset: NativePoint) {
        if let Ok(mut current) = self.scroll_offset.lock() {
            *current = scroll_offset;
        }
    }

    fn scroll_offset(&self) -> NativePoint {
        self.scroll_offset
            .lock()
            .map(|offset| *offset)
            .unwrap_or(NativePoint { x: 0, y: 0 })
    }

    pub(crate) fn set_nested_scroll_offsets(&self, offsets: BTreeMap<u32, NativePoint>) {
        if let Ok(mut current) = self.nested_scroll_offsets.lock() {
            *current = offsets;
        }
    }

    fn nested_scroll_offsets(&self) -> BTreeMap<u32, NativePoint> {
        self.nested_scroll_offsets
            .lock()
            .map(|offsets| offsets.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_history_state(&self, state: serde_json::Value) {
        if let Ok(mut current) = self.history_state.lock() {
            *current = state;
        }
    }

    pub(crate) fn set_history_length(&self, length: usize) {
        if let Ok(mut current) = self.history_length.lock() {
            *current = length.max(1);
        }
    }

    fn history_state(&self) -> serde_json::Value {
        self.history_state
            .lock()
            .map(|state| state.clone())
            .unwrap_or(serde_json::Value::Null)
    }

    pub(crate) fn history_length(&self) -> usize {
        self.history_length
            .lock()
            .map(|length| (*length).max(1))
            .unwrap_or(1)
    }

    fn frame_id(&self) -> String {
        self.frame_id
            .lock()
            .map(|frame_id| frame_id.clone())
            .unwrap_or_else(|_| self.storage_context_id.clone())
    }

    fn frame_script_context(&self) -> Option<NativeFrameScriptContext> {
        self.frame_script_context
            .lock()
            .ok()
            .and_then(|context| context.clone())
    }

    fn frame_script_bindings(&self) -> Vec<NativeFrameScriptBinding> {
        self.frame_script_bindings
            .lock()
            .map(|bindings| bindings.clone())
            .unwrap_or_default()
    }

    pub(crate) fn sync_window_proxies(
        &self,
        updates: &[NativeWindowProxyUpdate],
    ) -> Result<(), NativeEngineError> {
        if updates.is_empty() {
            return Ok(());
        }
        if updates.len() > super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native WindowProxy updates",
                super::interaction::MAX_NATIVE_EFFECTS,
                updates.len(),
            ));
        }
        for update in updates {
            if update.cache_key.len() > crate::browser_backend::MAX_JSON_BYTES
                || update.href.len() > crate::browser_backend::MAX_TEXT_BYTES
                || update.name.len() > MAX_NATIVE_WINDOW_NAME_BYTES
            {
                return Err(NativeEngineError::limit(
                    "native WindowProxy update",
                    crate::browser_backend::MAX_JSON_BYTES,
                    update
                        .cache_key
                        .len()
                        .max(update.href.len())
                        .max(update.name.len()),
                ));
            }
            validate_context_id(&update.target_context_id)?;
            validate_url_text("native WindowProxy update URL", &update.href)?;
            validate_window_name(&update.name)?;
        }
        window_proxy_update_script(updates)?;
        let mut pending =
            self.pending_window_proxy_updates
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue native WindowProxy updates".into(),
                    reason: "native WindowProxy update queue is unavailable".into(),
                })?;
        *pending = updates.to_vec();
        Ok(())
    }

    fn take_window_proxy_updates(&self) -> Vec<NativeWindowProxyUpdate> {
        self.pending_window_proxy_updates
            .lock()
            .map(|mut updates| std::mem::take(&mut *updates))
            .unwrap_or_default()
    }

    fn apply_window_name_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::SetWindowName { value } = command else {
            return Ok(false);
        };
        validate_window_name(value)?;
        let mut current = self
            .window_name
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "set window name".into(),
                reason: "native window name state lock is unavailable".into(),
            })?;
        *current = value.clone();
        Ok(true)
    }

    fn apply_window_close_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::CloseWindow {
            target,
            target_context_id,
        } = command
        else {
            return Ok(false);
        };
        validate_url_text("window close target", target)?;
        if let Some(target_context_id) = target_context_id {
            validate_context_id(target_context_id)?;
        }
        let mut requests =
            self.window_close_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "record native window close".into(),
                    reason: "native window close queue is unavailable".into(),
                })?;
        if requests.len() >= super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native window close requests",
                super::interaction::MAX_NATIVE_EFFECTS,
                requests.len().saturating_add(1),
            ));
        }
        requests.push(NativeWindowCloseRequest {
            target: target.clone(),
            target_context_id: target_context_id.clone(),
            source_context_id: String::new(),
        });
        Ok(true)
    }

    fn apply_window_navigation_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::NavigateWindow {
            target,
            target_context_id,
            href,
            replace,
        } = command
        else {
            return Ok(false);
        };
        if target.is_empty() && target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "window navigation target",
                "must not be empty without a direct target context",
            ));
        }
        if !target.is_empty() {
            validate_url_text("window navigation target", target)?;
        }
        validate_url_text("window navigation href", href)?;
        if let Some(target_context_id) = target_context_id {
            validate_context_id(target_context_id)?;
        }
        let mut requests =
            self.window_navigation_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "record native window navigation".into(),
                    reason: "native window navigation queue is unavailable".into(),
                })?;
        if requests.len() >= super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native window navigation requests",
                super::interaction::MAX_NATIVE_EFFECTS,
                requests.len().saturating_add(1),
            ));
        }
        requests.push(NativeWindowNavigationRequest {
            target: target.clone(),
            target_context_id: target_context_id.clone(),
            href: href.clone(),
            replace: *replace,
            source_context_id: String::new(),
        });
        Ok(true)
    }

    fn apply_frame_script_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::FrameScript {
            frame_id,
            source_frame_id,
            command,
        } = command
        else {
            return Ok(false);
        };
        validate_context_id(frame_id)?;
        validate_context_id(source_frame_id)?;
        let current_frame_id = self.frame_id();
        if source_frame_id != &current_frame_id {
            return Err(NativeEngineError::invalid(
                "same-origin frame script source",
                "must identify the current JavaScript frame",
            ));
        }
        if frame_id == source_frame_id {
            return Err(NativeEngineError::invalid(
                "same-origin frame script target",
                "must identify an embedded frame distinct from its caller",
            ));
        }
        if matches!(command.as_ref(), NativeScriptCommand::FrameScript { .. }) {
            return Err(NativeEngineError::invalid(
                "same-origin frame script command",
                "nested frame script commands are not allowed",
            ));
        }
        let encoded =
            serde_json::to_vec(command.as_ref()).map_err(|_| NativeEngineError::Worker {
                operation: "serialize same-origin frame script".into(),
                reason: "same-origin frame script command could not be serialized".into(),
            })?;
        if encoded.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
            return Err(NativeEngineError::limit(
                "same-origin frame script command",
                MAX_NATIVE_SCRIPT_RESULT_BYTES,
                encoded.len(),
            ));
        }
        let mut requests =
            self.frame_script_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "queue same-origin frame script".into(),
                    reason: "same-origin frame script queue is unavailable".into(),
                })?;
        if requests.len() >= super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "same-origin frame script requests",
                super::interaction::MAX_NATIVE_EFFECTS,
                requests.len().saturating_add(1),
            ));
        }
        requests.push(NativeFrameScriptRequest {
            frame_id: frame_id.clone(),
            source_frame_id: source_frame_id.clone(),
            command: command.clone(),
        });
        Ok(true)
    }

    fn now_ms(&self) -> u64 {
        self.clock_origin
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    /// Return the delay until the next page-owned timer or animation frame.
    ///
    /// Timer callbacks live inside the persistent QuickJS realm, so the host
    /// must inspect their bounded schedule before deciding whether an async
    /// evaluation can make progress. The result is relative to this realm's
    /// monotonic clock and never exposes callback objects or page data.
    pub(crate) fn next_timer_delay_ms(&self) -> Result<Option<u64>, NativeEngineError> {
        let now_ms = self.now_ms();
        let source = format!(
            "JSON.stringify((() => {{ let next = null; const consider = values => {{ for (const timer of values) {{ const dueAt = Number(timer && (timer.dueAt === undefined ? timer.timeoutAt : timer.dueAt)); if (!Number.isFinite(dueAt)) continue; const delay = Math.max(0, Math.ceil(dueAt - {now_ms})); if (next === null || delay < next) next = delay; }} }}; consider(globalThis.__glassTimers instanceof Map ? globalThis.__glassTimers.values() : []); consider(globalThis.__glassAnimationFrames instanceof Map ? globalThis.__glassAnimationFrames.values() : []); consider(globalThis.__glassIdleCallbacks instanceof Map ? globalThis.__glassIdleCallbacks.values() : []); return next; }})())"
        );
        self.context.with(|ctx| {
            let json: String =
                ctx.eval(source.as_str())
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "inspect native timer queue".into(),
                        reason: format!(
                            "native timer queue could not be inspected: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "native timer queue",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                operation: "decode native timer queue".into(),
                reason: "native timer queue returned an invalid delay".into(),
            })
        })
    }

    /// Execute one host event-loop turn for page timers and animation frames.
    ///
    /// The bootstrap timer pump is disabled for this call so each turn runs
    /// the queue exactly once. QuickJS then drains its pending promise jobs,
    /// allowing timer callbacks to settle top-level await or emit the same
    /// bounded DOM/network commands as ordinary script evaluation.
    pub(crate) fn run_timer_turn(
        &self,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let previous = self.timer_pump_enabled();
        self.set_timer_pump_enabled(false);
        let result = self.evaluate(
            "globalThis.__glassRunTimers(performance.now());",
            document,
            document_url,
            origin,
            viewport,
        );
        self.set_timer_pump_enabled(previous);
        result
    }

    /// Deliver one host-owned WebSocket event into the persistent page realm.
    /// The event callback runs on the same serialized QuickJS owner as every
    /// other page task, so DOM mutations and follow-up fetches cannot race the
    /// content snapshot commit.
    pub(crate) fn dispatch_websocket_event(
        &self,
        socket_id: u32,
        event: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let serialized = serde_json::to_string(event).map_err(|_| NativeEngineError::Worker {
            operation: "serialize native WebSocket event".into(),
            reason: "native WebSocket event could not be serialized".into(),
        })?;
        let source =
            format!("globalThis.__glassDispatchWebSocketEvent({socket_id}, {serialized});");
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "native WebSocket event",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        self.evaluate(&source, document, document_url, origin, viewport)
    }

    /// Deliver one host-owned EventSource event into the persistent page
    /// realm. The event callback runs on the same serialized QuickJS owner as
    /// every other page task, so streamed messages cannot race a document
    /// snapshot commit.
    pub(crate) fn dispatch_event_source_event(
        &self,
        source_id: u32,
        event: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let serialized = serde_json::to_string(event).map_err(|_| NativeEngineError::Worker {
            operation: "serialize native EventSource event".into(),
            reason: "native EventSource event could not be serialized".into(),
        })?;
        let source =
            format!("globalThis.__glassDispatchEventSourceEvent({source_id}, {serialized});");
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "native EventSource event",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        self.evaluate(&source, document, document_url, origin, viewport)
    }

    /// Deliver one host-owned Fetch response-stream event into the persistent
    /// page realm. Body chunks and terminal state use the same serialized
    /// owner as response continuations and other host events.
    pub(crate) fn dispatch_fetch_stream_event(
        &self,
        stream_id: u32,
        event: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let serialized = serde_json::to_string(event).map_err(|_| NativeEngineError::Worker {
            operation: "serialize native fetch response stream event".into(),
            reason: "native fetch response stream event could not be serialized".into(),
        })?;
        let source =
            format!("globalThis.__glassDispatchFetchStreamEvent({stream_id}, {serialized});");
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "native fetch response stream event",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        self.evaluate(&source, document, document_url, origin, viewport)
    }

    pub(crate) fn reset_timer_clock(&mut self) {
        self.clock_origin = Instant::now();
    }

    pub(crate) fn set_ready_state(&mut self, ready_state: &str) {
        self.ready_state.clear();
        self.ready_state.push_str(ready_state);
    }

    fn storage_view(&self, document_url: &str, origin: &NativeOrigin) -> NativeWebStorageView {
        let key = storage_key(document_url, origin);
        let Ok(state) = self.storage.lock() else {
            return NativeWebStorageView::default();
        };
        NativeWebStorageView {
            local: state.local.get(&key).cloned().unwrap_or_default(),
            session: state.session.get(&key).cloned().unwrap_or_default(),
        }
    }

    fn apply_storage_command(
        &self,
        command: &NativeScriptCommand,
        document_url: &str,
        origin: &NativeOrigin,
    ) -> Result<Option<NativeStorageEvent>, NativeEngineError> {
        let (scope, entry_key, value, operation) = match command {
            NativeScriptCommand::StorageSet { scope, key, value } => (
                scope.as_str(),
                Some(key.as_str()),
                Some(value.as_str()),
                "set",
            ),
            NativeScriptCommand::StorageRemove { scope, key } => {
                (scope.as_str(), Some(key.as_str()), None, "remove")
            }
            NativeScriptCommand::StorageClear { scope } => (scope.as_str(), None, None, "clear"),
            _ => return Ok(None),
        };
        let mut state = self.storage.lock().map_err(|_| NativeEngineError::Worker {
            operation: "native Web Storage".into(),
            reason: "native Web Storage state lock is unavailable".into(),
        })?;
        let source_context_id = self.storage_context_id.clone();
        let storage = match scope {
            "local" => &mut state.local,
            "session" => &mut state.session,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Web Storage scope",
                    "must be local or session",
                ));
            }
        };
        let origin_key = storage_key(document_url, origin);
        let entries = storage.entry(origin_key.clone()).or_default();
        if let Some(entry_key) = entry_key {
            if entry_key.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
                return Err(NativeEngineError::limit(
                    "native Web Storage key",
                    crate::browser_backend::MAX_BACKEND_ID_BYTES,
                    entry_key.len(),
                ));
            }
        }
        if let Some(value) = value {
            if value.len() > crate::browser_backend::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "native Web Storage value",
                    crate::browser_backend::MAX_TEXT_BYTES,
                    value.len(),
                ));
            }
        }
        let change = match operation {
            "set" => {
                if entries.len() >= crate::browser_backend::MAX_STORAGE_ENTRIES
                    && entry_key.is_some_and(|key| !entries.contains_key(key))
                {
                    return Err(NativeEngineError::limit(
                        "native Web Storage entries",
                        crate::browser_backend::MAX_STORAGE_ENTRIES,
                        entries.len().saturating_add(1),
                    ));
                }
                let key = entry_key.expect("storage set key");
                let value = value.expect("storage set value");
                let old_value = entries.get(key).cloned();
                if old_value.as_deref() == Some(value) {
                    None
                } else {
                    entries.insert(key.to_owned(), value.to_owned());
                    Some(NativeStorageEvent {
                        source_context_id: source_context_id.clone(),
                        scope: scope.to_owned(),
                        storage_key: origin_key,
                        key: Some(key.to_owned()),
                        old_value,
                        new_value: Some(value.to_owned()),
                        url: document_url.to_owned(),
                    })
                }
            }
            "remove" => {
                let key = entry_key.expect("storage remove key");
                entries.remove(key).map(|old_value| NativeStorageEvent {
                    source_context_id: source_context_id.clone(),
                    scope: scope.to_owned(),
                    storage_key: origin_key,
                    key: Some(key.to_owned()),
                    old_value: Some(old_value),
                    new_value: None,
                    url: document_url.to_owned(),
                })
            }
            "clear" if entries.is_empty() => None,
            "clear" => {
                entries.clear();
                Some(NativeStorageEvent {
                    source_context_id,
                    scope: scope.to_owned(),
                    storage_key: origin_key,
                    key: None,
                    old_value: None,
                    new_value: None,
                    url: document_url.to_owned(),
                })
            }
            _ => unreachable!("storage operation matched above"),
        };
        if let Some(change) = change.as_ref() {
            let mut changes =
                self.storage_changes
                    .lock()
                    .map_err(|_| NativeEngineError::Worker {
                        operation: "record native storage event".into(),
                        reason: "native storage change queue is unavailable".into(),
                    })?;
            if changes.len() >= MAX_NATIVE_STORAGE_EVENTS {
                return Err(NativeEngineError::limit(
                    "native storage changes",
                    MAX_NATIVE_STORAGE_EVENTS,
                    changes.len().saturating_add(1),
                ));
            }
            changes.push(change.clone());
        }
        Ok(change)
    }

    fn apply_cookie_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::CookieSet { value } = command else {
            return Ok(false);
        };
        if value.len() > crate::browser_backend::MAX_TEXT_BYTES {
            return Err(NativeEngineError::limit(
                "native document.cookie value",
                crate::browser_backend::MAX_TEXT_BYTES,
                value.len(),
            ));
        }
        let mut updates = self
            .cookie_updates
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "native document.cookie".into(),
                reason: "native document.cookie update queue is unavailable".into(),
            })?;
        if updates.len() >= super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native document.cookie updates",
                super::interaction::MAX_NATIVE_EFFECTS,
                updates.len().saturating_add(1),
            ));
        }
        updates.push(value.clone());
        Ok(true)
    }

    fn apply_dialog_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::Dialog {
            dialog_type,
            message,
            default_value,
        } = command
        else {
            return Ok(false);
        };
        if !matches!(dialog_type.as_str(), "alert" | "confirm" | "prompt") {
            return Err(NativeEngineError::invalid(
                "native dialog type",
                "must be alert, confirm, or prompt",
            ));
        }
        if message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES {
            return Err(NativeEngineError::limit(
                "native dialog message",
                MAX_NATIVE_DIALOG_TEXT_BYTES,
                message.len(),
            ));
        }
        if default_value
            .as_ref()
            .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "native dialog default value",
                MAX_NATIVE_DIALOG_TEXT_BYTES,
                default_value.as_ref().map_or(0, String::len),
            ));
        }
        let mut dialogs = self
            .dialog_events
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "record native dialog".into(),
                reason: "native dialog queue is unavailable".into(),
            })?;
        if dialogs.len() >= MAX_NATIVE_DIALOGS {
            return Err(NativeEngineError::limit(
                "native dialogs",
                MAX_NATIVE_DIALOGS,
                dialogs.len().saturating_add(1),
            ));
        }
        dialogs.push(NativeDialog {
            dialog_type: dialog_type.clone(),
            message: message.clone(),
            default_value: default_value.clone(),
        });
        Ok(true)
    }

    fn apply_popup_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::OpenWindow {
            href,
            target,
            handle,
        } = command
        else {
            return Ok(false);
        };
        validate_url_text("window.open URL", href)?;
        validate_url_text("window.open target", target)?;
        let mut popups = self
            .popup_events
            .lock()
            .map_err(|_| NativeEngineError::Worker {
                operation: "record native popup".into(),
                reason: "native popup queue is unavailable".into(),
            })?;
        if popups.len() >= super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native popup requests",
                super::interaction::MAX_NATIVE_EFFECTS,
                popups.len().saturating_add(1),
            ));
        }
        popups.push(NativePopupRequest {
            url: href.clone(),
            target: target.clone(),
            handle: handle.clone(),
            source_context_id: String::new(),
        });
        Ok(true)
    }

    fn apply_post_message_command(
        &self,
        command: &NativeScriptCommand,
    ) -> Result<bool, NativeEngineError> {
        let NativeScriptCommand::PostMessage {
            target,
            target_origin,
            data,
            target_context_id,
        } = command
        else {
            return Ok(false);
        };
        if target.is_empty() && target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "postMessage target",
                "must not be empty without a direct target context",
            ));
        }
        if !target.is_empty() {
            validate_url_text("postMessage target", target)?;
        }
        validate_url_text("postMessage target origin", target_origin)?;
        if let Some(target_context_id) = target_context_id {
            validate_context_id(target_context_id)?;
        }
        let encoded = serde_json::to_vec(data).map_err(|_| NativeEngineError::Worker {
            operation: "record native postMessage".into(),
            reason: "postMessage data could not be serialized".into(),
        })?;
        if encoded.len() > MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native postMessage data",
                MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
        let mut messages =
            self.post_message_events
                .lock()
                .map_err(|_| NativeEngineError::Worker {
                    operation: "record native postMessage".into(),
                    reason: "native postMessage queue is unavailable".into(),
                })?;
        if messages.len() >= super::interaction::MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native postMessage requests",
                super::interaction::MAX_NATIVE_EFFECTS,
                messages.len().saturating_add(1),
            ));
        }
        messages.push(NativePostMessageRequest {
            target: target.clone(),
            target_origin: target_origin.clone(),
            data: data.clone(),
            target_context_id: target_context_id.clone(),
            source_context_id: String::new(),
            source_origin: String::new(),
        });
        Ok(true)
    }

    fn set_module_sources(&self, sources: BTreeMap<String, String>) {
        if let Ok(mut current) = self.module_sources.lock() {
            *current = sources;
        }
    }

    pub(crate) fn evaluate(
        &self,
        source: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if source.is_empty() {
            return Err(NativeEngineError::invalid(
                "script source",
                "must not be empty",
            ));
        }
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "script source",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        let storage_events = self.take_storage_events();
        let proxy_update_script = window_proxy_update_script(&self.take_window_proxy_updates())?;
        let window_name = self.window_name();
        let opener_window_name = self.opener_window_name();
        let frame_id = self.frame_id();
        let scroll_offset = self.scroll_offset();
        let nested_scroll_offsets = self.nested_scroll_offsets();
        let history_state = self.history_state();
        let history_length = self.history_length();
        let bootstrap = document_bootstrap(
            document,
            document_url,
            &frame_id,
            &window_name,
            self.opener_context_id(),
            opener_window_name,
            &self.opener_url,
            origin,
            viewport,
            scroll_offset,
            &nested_scroll_offsets,
            &history_state,
            history_length,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
            &self.indexed_db_state(),
            &storage_events,
            &self.cookie_state(),
            &self.frame_script_bindings(),
            self.frame_script_context().as_ref(),
            self.timer_pump_enabled(),
        )?;
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let result = self.context.with(|ctx| {
            ctx.eval::<(), _>(bootstrap.as_str())
                .map_err(|_| NativeEngineError::Worker {
                    operation: "install JavaScript host view".into(),
                    reason: "native JavaScript host view could not be installed".into(),
                })?;
            if let Some(source) = proxy_update_script.as_deref() {
                ctx.eval::<(), _>(source)
                    .map_err(|_| NativeEngineError::Worker {
                        operation: "synchronize native WindowProxy state".into(),
                        reason: "native WindowProxy state could not be synchronized".into(),
                    })?;
            }
            let mut top_level_await_pending = false;
            let (value, async_evaluation): (Value, bool) = match ctx.eval::<Value, _>(source) {
                Ok(value) => (value, false),
                Err(_) if contains_await_token(source) => {
                    let promise = ctx.eval_promise(source).map_err(|error| {
                        NativeEngineError::Worker {
                            operation: "evaluate JavaScript".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        }
                    })?;
                    ctx.eval::<(), _>(
                        "globalThis.__glassTopLevelAwaitState = { state: 'pending' };",
                    )
                    .map_err(|error| NativeEngineError::Worker {
                        operation: "prepare JavaScript evaluation promise".into(),
                        reason: format!(
                            "JavaScript evaluation failed: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    })?;
                    let on_fulfilled: Function = ctx
                        .eval("value => { globalThis.__glassTopLevelAwaitState = { state: 'fulfilled', value: value === undefined ? null : value }; }")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "prepare JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    let on_rejected: Function = ctx
                        .eval("error => { globalThis.__glassTopLevelAwaitState = { state: 'rejected', error: String(error) }; }")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "prepare JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    promise
                        .then()
                        .and_then(|then| {
                            then.call::<_, ()>((
                                This(promise.clone()),
                                on_fulfilled,
                                on_rejected,
                            ))
                        })
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "prepare JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    match promise.finish::<Value>() {
                        Ok(value) => (value, true),
                        Err(Error::WouldBlock) => {
                            top_level_await_pending = true;
                            let value =
                                ctx.eval::<Value, _>("undefined").map_err(|error| {
                                    NativeEngineError::Worker {
                                        operation: "evaluate JavaScript".into(),
                                        reason: format!(
                                            "JavaScript evaluation failed: {}",
                                            CaughtError::from_error(&ctx, error)
                                        ),
                                    }
                                })?;
                            (value, true)
                        }
                        Err(error) => {
                            return Err(NativeEngineError::Worker {
                                operation: "evaluate JavaScript".into(),
                                reason: format!(
                                    "JavaScript evaluation failed: {}",
                                    CaughtError::from_error(&ctx, error)
                                ),
                            });
                        }
                    }
                }
                Err(error) => {
                    return Err(NativeEngineError::Worker {
                        operation: "evaluate JavaScript".into(),
                        reason: format!(
                            "JavaScript evaluation failed: {}",
                            CaughtError::from_error(&ctx, error)
                        ),
                    });
                }
            };
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueResizeObserverChanges === 'function') globalThis.__glassQueueResizeObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native resize observers".into(),
                reason: "native resize observers could not be scheduled".into(),
            })?;
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueIntersectionObserverChanges === 'function') globalThis.__glassQueueIntersectionObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native intersection observers".into(),
                reason: "native intersection observers could not be scheduled".into(),
            })?;
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
                }
            }
            for (event_type, cancelable, reasons) in [
                (
                    "unhandledrejection",
                    true,
                    self.take_unhandled_promise_rejections(),
                ),
                (
                    "rejectionhandled",
                    false,
                    self.take_handled_promise_rejections(),
                ),
            ] {
                if let Some(event_source) =
                    promise_rejection_event_script(event_type, &reasons, cancelable)?
                {
                    ctx.eval::<(), _>(event_source.as_str()).map_err(|error| {
                        NativeEngineError::Worker {
                            operation: "dispatch native Promise rejection events".into(),
                            reason: format!(
                                "native Promise rejection event dispatch failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        }
                    })?;
                }
            }
            let commands = read_script_commands(ctx.clone())?;
            let mut document_commands = Vec::with_capacity(commands.len());
            for command in commands {
                if self
                    .apply_storage_command(&command, document_url, origin)?
                    .is_some()
                {
                    continue;
                }
                if self.apply_cookie_command(&command)? {
                    continue;
                }
                if self.apply_dialog_command(&command)? {
                    continue;
                }
                if self.apply_popup_command(&command)? {
                    continue;
                }
                if self.apply_window_name_command(&command)? {
                    continue;
                }
                if self.apply_window_close_command(&command)? {
                    continue;
                }
                if self.apply_window_navigation_command(&command)? {
                    continue;
                }
                if self.apply_frame_script_command(&command)? {
                    continue;
                }
                if self.apply_post_message_command(&command)? {
                    continue;
                }
                document_commands.push(command);
            }
            let commands = document_commands;
            let indexed_db_state = read_indexed_db_state(ctx.clone())?;
            self.set_indexed_db_state(indexed_db_state);
            let json = ctx
                .json_stringify(value)
                .map_err(|error| NativeEngineError::Worker {
                    operation: "serialize JavaScript result".into(),
                    reason: format!(
                        "JavaScript result could not be serialized: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            let Some(json) = json else {
                return Ok(NativeScriptEvaluation {
                    value: serde_json::Value::Null,
                    commands,
                    top_level_await_pending,
                });
            };
            let json = json.to_string().map_err(|_| NativeEngineError::Worker {
                operation: "serialize JavaScript result".into(),
                reason: "JavaScript result could not be converted to UTF-8".into(),
            })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "script result",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            let mut result: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                    operation: "decode JavaScript result".into(),
                    reason: "JavaScript result was not valid JSON".into(),
                })?;
            if async_evaluation
                && let Some(object) = result.as_object_mut()
                && object.len() == 1
                && let Some(value) = object.remove("value")
            {
                result = value;
            }
            Ok(NativeScriptEvaluation {
                value: result,
                commands,
                top_level_await_pending,
            })
        });
        if let Ok(mut current) = self.deadline.lock() {
            *current = None;
        }
        result
    }

    pub(crate) fn resolve_fetch(
        &self,
        request_id: u32,
        payload: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let serialized = serde_json::to_string(payload).map_err(|_| NativeEngineError::Worker {
            operation: "serialize JavaScript fetch response".into(),
            reason: "native fetch response could not be serialized".into(),
        })?;
        if serialized.len() > MAX_NATIVE_SCRIPT_BYTES {
            return self.evaluate(
                &format!(
                    "globalThis.__glassResolveFetch({request_id}, {{ error: \"fetch response exceeded the script transfer limit\" }});"
                ),
                document,
                document_url,
                origin,
                viewport,
            );
        }
        self.evaluate(
            &format!("globalThis.__glassResolveFetch({request_id}, {serialized});"),
            document,
            document_url,
            origin,
            viewport,
        )
    }

    pub(crate) fn take_top_level_await_result(
        &self,
    ) -> Result<Option<serde_json::Value>, NativeEngineError> {
        self.context.with(|ctx| {
            let json: String = ctx
                .eval("JSON.stringify(globalThis.__glassTopLevelAwaitState || { state: 'none' })")
                .map_err(|error| NativeEngineError::Worker {
                    operation: "read JavaScript evaluation promise".into(),
                    reason: format!(
                        "JavaScript evaluation state could not be read: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "script result",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            let state: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                    operation: "decode JavaScript evaluation promise".into(),
                    reason: "JavaScript evaluation state was not valid JSON".into(),
                })?;
            match state.get("state").and_then(serde_json::Value::as_str) {
                Some("none") | Some("pending") => Ok(None),
                Some("fulfilled") => {
                    ctx.eval::<(), _>("globalThis.__glassTopLevelAwaitState = null;")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "clear JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation state could not be cleared: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    let mut value = state
                        .get("value")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    if let Some(object) = value.as_object_mut()
                        && object.len() == 1
                        && let Some(enveloped) = object.remove("value")
                    {
                        value = enveloped;
                    }
                    Ok(Some(value))
                }
                Some("rejected") => {
                    let reason = state
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown JavaScript evaluation error")
                        .to_owned();
                    ctx.eval::<(), _>("globalThis.__glassTopLevelAwaitState = null;")
                        .map_err(|error| NativeEngineError::Worker {
                            operation: "clear JavaScript evaluation promise".into(),
                            reason: format!(
                                "JavaScript evaluation state could not be cleared: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        })?;
                    Err(NativeEngineError::Worker {
                        operation: "resolve JavaScript evaluation promise".into(),
                        reason,
                    })
                }
                _ => Err(NativeEngineError::Worker {
                    operation: "decode JavaScript evaluation promise".into(),
                    reason: "JavaScript evaluation state had an unknown status".into(),
                }),
            }
        })
    }

    pub(crate) fn evaluate_module(
        &self,
        name: &str,
        source: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if name.is_empty() {
            return Err(NativeEngineError::invalid(
                "module name",
                "must not be empty",
            ));
        }
        if source.is_empty() {
            return Err(NativeEngineError::invalid(
                "module source",
                "must not be empty",
            ));
        }
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "module source",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        let storage_events = self.take_storage_events();
        let proxy_update_script = window_proxy_update_script(&self.take_window_proxy_updates())?;
        let window_name = self.window_name();
        let opener_window_name = self.opener_window_name();
        let frame_id = self.frame_id();
        let scroll_offset = self.scroll_offset();
        let nested_scroll_offsets = self.nested_scroll_offsets();
        let history_state = self.history_state();
        let history_length = self.history_length();
        let bootstrap = document_bootstrap(
            document,
            document_url,
            &frame_id,
            &window_name,
            self.opener_context_id(),
            opener_window_name,
            &self.opener_url,
            origin,
            viewport,
            scroll_offset,
            &nested_scroll_offsets,
            &history_state,
            history_length,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
            &self.indexed_db_state(),
            &storage_events,
            &self.cookie_state(),
            &self.frame_script_bindings(),
            self.frame_script_context().as_ref(),
            self.timer_pump_enabled(),
        )?;
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let result = self.context.with(|ctx| {
            ctx.eval::<(), _>(bootstrap.as_str())
                .map_err(|_| NativeEngineError::Worker {
                    operation: "install JavaScript host view".into(),
                    reason: "native JavaScript host view could not be installed".into(),
                })?;
            if let Some(source) = proxy_update_script.as_deref() {
                ctx.eval::<(), _>(source)
                    .map_err(|_| NativeEngineError::Worker {
                        operation: "synchronize native WindowProxy state".into(),
                        reason: "native WindowProxy state could not be synchronized".into(),
                    })?;
            }
            Module::evaluate(ctx.clone(), name, source)
                .and_then(|promise| promise.finish::<()>())
                .map_err(|error| NativeEngineError::Worker {
                    operation: "evaluate JavaScript module".into(),
                    reason: format!(
                        "JavaScript module evaluation failed: {}",
                        CaughtError::from_error(&ctx, error)
                    ),
                })?;
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueResizeObserverChanges === 'function') globalThis.__glassQueueResizeObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native resize observers".into(),
                reason: "native resize observers could not be scheduled".into(),
            })?;
            ctx.eval::<(), _>(
                "if (typeof globalThis.__glassQueueIntersectionObserverChanges === 'function') globalThis.__glassQueueIntersectionObserverChanges();",
            )
            .map_err(|_| NativeEngineError::Worker {
                operation: "queue native intersection observers".into(),
                reason: "native intersection observers could not be scheduled".into(),
            })?;
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
                }
            }
            for (event_type, cancelable, reasons) in [
                (
                    "unhandledrejection",
                    true,
                    self.take_unhandled_promise_rejections(),
                ),
                (
                    "rejectionhandled",
                    false,
                    self.take_handled_promise_rejections(),
                ),
            ] {
                if let Some(event_source) =
                    promise_rejection_event_script(event_type, &reasons, cancelable)?
                {
                    ctx.eval::<(), _>(event_source.as_str()).map_err(|error| {
                        NativeEngineError::Worker {
                            operation: "dispatch native Promise rejection events".into(),
                            reason: format!(
                                "native Promise rejection event dispatch failed: {}",
                                CaughtError::from_error(&ctx, error)
                            ),
                        }
                    })?;
                }
            }
            let commands = read_script_commands(ctx.clone())?;
            let mut document_commands = Vec::with_capacity(commands.len());
            for command in commands {
                if self
                    .apply_storage_command(&command, document_url, origin)?
                    .is_some()
                {
                    continue;
                }
                if self.apply_cookie_command(&command)? {
                    continue;
                }
                if self.apply_dialog_command(&command)? {
                    continue;
                }
                if self.apply_popup_command(&command)? {
                    continue;
                }
                if self.apply_window_name_command(&command)? {
                    continue;
                }
                if self.apply_window_close_command(&command)? {
                    continue;
                }
                if self.apply_window_navigation_command(&command)? {
                    continue;
                }
                if self.apply_frame_script_command(&command)? {
                    continue;
                }
                if self.apply_post_message_command(&command)? {
                    continue;
                }
                document_commands.push(command);
            }
            let indexed_db_state = read_indexed_db_state(ctx.clone())?;
            self.set_indexed_db_state(indexed_db_state);
            Ok(NativeScriptEvaluation {
                value: serde_json::Value::Null,
                commands: document_commands,
                top_level_await_pending: false,
            })
        });
        if let Ok(mut current) = self.deadline.lock() {
            *current = None;
        }
        result
    }

    #[allow(dead_code)]
    pub(crate) fn has_pending_jobs(&self) -> bool {
        self.runtime.is_job_pending()
    }
}

fn read_script_commands<'js>(
    ctx: rquickjs::Ctx<'js>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let json: String = ctx
        .eval("JSON.stringify(globalThis.__glassHostCommands || [])")
        .map_err(|_| NativeEngineError::Worker {
            operation: "collect JavaScript host commands".into(),
            reason: "native JavaScript host commands could not be collected".into(),
        })?;
    if json.len() > MAX_NATIVE_SCRIPT_COMMAND_BYTES {
        return Err(NativeEngineError::limit(
            "script host commands",
            MAX_NATIVE_SCRIPT_COMMAND_BYTES,
            json.len(),
        ));
    }
    let commands = serde_json::from_str::<Vec<NativeScriptCommand>>(&json).map_err(|_| {
        NativeEngineError::Worker {
            operation: "decode JavaScript host commands".into(),
            reason: "native JavaScript host commands were invalid".into(),
        }
    })?;
    if commands.len() > super::interaction::MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "script host commands",
            super::interaction::MAX_NATIVE_EFFECTS,
            commands.len(),
        ));
    }
    Ok(commands)
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

/// Extract the bounded static import/export specifiers from a module source.
///
/// This is intentionally a lexical prefetch pass, not a replacement for the
/// JavaScript parser. QuickJS remains authoritative for module grammar and
/// evaluation; this pass only discovers URLs that the content process must
/// fetch before installing the in-memory module loader.
pub(crate) fn static_module_specifiers(source: &str) -> Result<Vec<String>, NativeEngineError> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut specifiers = Vec::new();
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() {
            break;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if !is_javascript_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
            index += 1;
        }
        let keyword = &bytes[start..index];
        if keyword != b"import" && keyword != b"export" {
            continue;
        }
        if let Some(specifier) = module_specifier_after_keyword(bytes, index, keyword == b"import")
        {
            if specifier.is_empty() {
                return Err(NativeEngineError::invalid(
                    "module import",
                    "module specifier must not be empty",
                ));
            }
            specifiers.push(specifier);
            if specifiers.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "module imports",
                    MAX_NATIVE_MODULE_IMPORTS,
                    specifiers.len(),
                ));
            }
        }
    }
    Ok(specifiers)
}

/// Extract literal dynamic-import specifiers. Computed expressions remain
/// unresolved and therefore fail through the bounded module loader instead of
/// receiving an implicit network capability.
pub(crate) fn literal_dynamic_module_specifiers(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut specifiers = Vec::new();
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() {
            break;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if !is_javascript_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
            index += 1;
        }
        if &bytes[start..index] != b"import" {
            continue;
        }
        let argument = skip_javascript_space_and_comments(bytes, index);
        if bytes.get(argument) != Some(&b'(') {
            continue;
        }
        let specifier_start = skip_javascript_space_and_comments(bytes, argument + 1);
        if let Some((specifier, _)) = read_javascript_string(bytes, specifier_start) {
            specifiers.push(specifier);
            if specifiers.len() >= MAX_NATIVE_MODULE_IMPORTS {
                break;
            }
        }
    }
    specifiers
}

fn module_specifier_after_keyword(
    bytes: &[u8],
    keyword_end: usize,
    import_keyword: bool,
) -> Option<String> {
    let start = skip_javascript_space_and_comments(bytes, keyword_end);
    if import_keyword && bytes.get(start) == Some(&b'(') {
        return None;
    }
    if bytes
        .get(start)
        .is_some_and(|byte| matches!(byte, b'\'' | b'"'))
    {
        return read_javascript_string(bytes, start).map(|(value, _)| value);
    }
    let mut index = start;
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() || bytes[index] == b';' {
            return None;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if is_javascript_identifier_start(bytes[index]) {
            let start = index;
            index += 1;
            while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
                index += 1;
            }
            if &bytes[start..index] == b"from" {
                let specifier_start = skip_javascript_space_and_comments(bytes, index);
                return read_javascript_string(bytes, specifier_start).map(|(value, _)| value);
            }
            continue;
        }
        index += 1;
    }
    None
}

fn skip_javascript_space_and_comments(bytes: &[u8], mut index: usize) -> usize {
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        return index;
    }
}

fn skip_javascript_string(bytes: &[u8], mut index: usize) -> usize {
    let Some(&quote) = bytes.get(index) else {
        return index;
    };
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = index.saturating_add(2);
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += 1;
        }
    }
    index
}

fn read_javascript_string(bytes: &[u8], mut index: usize) -> Option<(String, usize)> {
    let quote = *bytes.get(index)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    index += 1;
    let mut value = String::new();
    while index < bytes.len() {
        match bytes[index] {
            byte if byte == quote => return Some((value, index + 1)),
            b'\\' if index + 1 < bytes.len() => {
                value.push(bytes[index + 1] as char);
                index += 2;
            }
            byte => {
                value.push(byte as char);
                index += 1;
            }
        }
    }
    None
}

fn is_javascript_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_javascript_identifier_continue(byte: u8) -> bool {
    is_javascript_identifier_start(byte) || byte.is_ascii_digit()
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

fn window_proxy_update_script(
    updates: &[NativeWindowProxyUpdate],
) -> Result<Option<String>, NativeEngineError> {
    if updates.is_empty() {
        return Ok(None);
    }
    let serialized = serde_json::to_string(updates).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native WindowProxy updates".into(),
        reason: "native WindowProxy updates could not be serialized".into(),
    })?;
    let source = format!(
        "globalThis.__glassSyncWindowProxies && globalThis.__glassSyncWindowProxies({serialized});"
    );
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native WindowProxy update script",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

fn document_bootstrap(
    document: &NativeDocument,
    document_url: &str,
    context_id: &str,
    window_name: &str,
    opener_context_id: Option<&str>,
    opener_window_name: &str,
    opener_url: &str,
    origin: &NativeOrigin,
    viewport: Viewport,
    scroll_offset: NativePoint,
    nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    history_state: &serde_json::Value,
    history_length: usize,
    ready_state: &str,
    now_ms: u64,
    storage: &NativeWebStorageView,
    indexed_db: &NativeIndexedDbOrigin,
    storage_events: &[NativeStorageEvent],
    cookie: &str,
    frame_bindings: &[NativeFrameScriptBinding],
    frame_context: Option<&NativeFrameScriptContext>,
    run_timers: bool,
) -> Result<String, NativeEngineError> {
    let native_file_bytes = MAX_NATIVE_FILE_BYTES;
    let native_form_body_bytes = MAX_NATIVE_FORM_BODY_BYTES;
    let state = document.script_snapshot_with_layout(
        crate::browser_backend::MAX_TEXT_BYTES,
        viewport,
        scroll_offset,
        nested_scroll_offsets,
    )?;
    let serialized = serde_json::to_string(&serde_json::json!({
        "url": document_url,
        "context_id": context_id,
        "frame_id": context_id,
        "window_name": window_name,
        "opener_context_id": opener_context_id,
        "opener_window_name": opener_window_name,
        "opener_url": opener_url,
        "origin": origin.serialized(),
        "state": state,
        "history_state": history_state,
        "history_length": history_length,
        "now_ms": now_ms,
        "storage": storage,
        "indexed_db": indexed_db,
        "storage_events": storage_events,
        "cookie": cookie,
        "frames": frame_bindings,
        "frame_context": frame_context,
        "device_scale_factor_milli": viewport.device_scale_factor_milli,
    }))
    .map_err(|_| NativeEngineError::Worker {
        operation: "serialize JavaScript host view".into(),
        reason: "native JavaScript host view could not be serialized".into(),
    })?;
    let ready_state =
        serde_json::to_string(ready_state).map_err(|_| NativeEngineError::Worker {
            operation: "serialize document ready state".into(),
            reason: "native document ready state could not be serialized".into(),
        })?;
    Ok(format!(
        r###"(() => {{
  const host = {serialized};
  const state = host.state;
  const geometryByIndex = globalThis.__glassHostGeometry instanceof Map
    ? globalThis.__glassHostGeometry
    : new Map();
  geometryByIndex.clear();
  for (const entry of Array.isArray(state.geometry) ? state.geometry : []) {{
    if (!entry || entry.nodeIndex === undefined) continue;
    geometryByIndex.set(Number(entry.nodeIndex), entry);
  }}
  globalThis.__glassHostGeometry = geometryByIndex;
  const zeroGeometry = () => ({{
    x: 0,
    y: 0,
    width: 0,
    height: 0,
    contentX: 0,
    contentY: 0,
    contentWidth: 0,
    contentHeight: 0,
    scrollX: 0,
    scrollY: 0,
    scrollWidth: 0,
    scrollHeight: 0,
    clientWidth: 0,
    clientHeight: 0,
  }});
  const geometryForNode = (node) => {{
    if (node && typeof node.__glassGeometrySource === "function") {{
      const projected = node.__glassGeometrySource();
      if (projected) return projected;
    }}
    if (node && node.__glassGeometry) return node.__glassGeometry;
    return node && typeof node.nodeIndex === "number"
      ? geometryByIndex.get(Number(node.nodeIndex)) || zeroGeometry()
      : zeroGeometry();
  }};
  const makeDomRect = (geometry) => {{
    const value = geometry || zeroGeometry();
    const x = Number(value.x) || 0;
    const y = Number(value.y) || 0;
    const width = Math.max(0, Number(value.width) || 0);
    const height = Math.max(0, Number(value.height) || 0);
    const Constructor = typeof globalThis.DOMRect === "function" ? globalThis.DOMRect : null;
    return Constructor
      ? new Constructor(x, y, width, height)
      : {{
          x, y, width, height,
          top: y,
          right: x + width,
          bottom: y + height,
          left: x,
          toJSON() {{ return {{ x, y, width, height, top: y, right: x + width, bottom: y + height, left: x }}; }},
        }};
  }};
  const resizeObservers = globalThis.__glassResizeObservers instanceof Set
    ? globalThis.__glassResizeObservers
    : new Set();
  globalThis.__glassResizeObservers = resizeObservers;
  let resizeDeliveryQueued = false;
  const resizeGeometryChanged = (previous, next) => !previous
    || previous.width !== next.width
    || previous.height !== next.height
    || previous.contentWidth !== next.contentWidth
    || previous.contentHeight !== next.contentHeight;
  const resizeEntry = (target, geometry) => {{
    const contentRect = makeDomRect({{
      x: geometry.contentX,
      y: geometry.contentY,
      width: geometry.contentWidth,
      height: geometry.contentHeight,
    }});
    const borderBoxSize = [{{ inlineSize: geometry.width, blockSize: geometry.height }}];
    const contentBoxSize = [{{ inlineSize: geometry.contentWidth, blockSize: geometry.contentHeight }}];
    const devicePixelContentBoxSize = [{{
      inlineSize: geometry.contentWidth * (Number(host.device_scale_factor_milli || 1000) / 1000),
      blockSize: geometry.contentHeight * (Number(host.device_scale_factor_milli || 1000) / 1000),
    }}];
    return {{
      target,
      contentRect,
      borderBoxSize,
      contentBoxSize,
      devicePixelContentBoxSize,
    }};
  }};
  const deliverResizeObservers = () => {{
    resizeDeliveryQueued = false;
    for (const observer of Array.from(resizeObservers)) {{
      if (!observer.__glassRecords || observer.__glassRecords.length === 0) continue;
      const records = observer.__glassRecords.splice(0, observer.__glassRecords.length);
      observer.__glassCallback(records, observer);
    }}
  }};
  const scheduleResizeObserverDelivery = () => {{
    if (resizeDeliveryQueued) return;
    resizeDeliveryQueued = true;
    Promise.resolve().then(deliverResizeObservers);
  }};
  const queueResizeObserverChanges = () => {{
    for (const observer of Array.from(resizeObservers)) {{
      for (const registration of observer.__glassRegistrations || []) {{
        const geometry = geometryForNode(registration.target);
        if (!resizeGeometryChanged(registration.last, geometry)) continue;
        registration.last = {{ ...geometry }};
        observer.__glassRecords.push(resizeEntry(registration.target, geometry));
        if (observer.__glassRecords.length > {max_commands}) {{
          observer.__glassRecords.splice(0, observer.__glassRecords.length - {max_commands});
        }}
      }}
    }}
    if (Array.from(resizeObservers).some((observer) => observer.__glassRecords.length > 0)) {{
      scheduleResizeObserverDelivery();
    }}
  }};
  const intersectionObservers = globalThis.__glassIntersectionObservers instanceof Set
    ? globalThis.__glassIntersectionObservers
    : new Set();
  globalThis.__glassIntersectionObservers = intersectionObservers;
  let intersectionDeliveryQueued = false;
  const intersectionViewport = () => ({{
    x: 0,
    y: 0,
    width: Math.max(0, Number({width}) || 0),
    height: Math.max(0, Number({height}) || 0),
  }});
  const intersectionRect = (geometry) => {{
    const value = geometry || zeroGeometry();
    return {{
      x: Number(value.x) || 0,
      y: Number(value.y) || 0,
      width: Math.max(0, Number(value.width) || 0),
      height: Math.max(0, Number(value.height) || 0),
    }};
  }};
  const intersectionWith = (left, right) => {{
    const x = Math.max(left.x, right.x);
    const y = Math.max(left.y, right.y);
    const rightEdge = Math.min(left.x + left.width, right.x + right.width);
    const bottomEdge = Math.min(left.y + left.height, right.y + right.height);
    return {{
      x,
      y,
      width: Math.max(0, rightEdge - x),
      height: Math.max(0, bottomEdge - y),
    }};
  }};
  const intersectionWithMargin = (rect, margin) => ({{
    x: rect.x - margin[3],
    y: rect.y - margin[0],
    width: Math.max(0, rect.width + margin[1] + margin[3]),
    height: Math.max(0, rect.height + margin[0] + margin[2]),
  }});
  const intersectionArea = (rect) => rect.width * rect.height;
  const intersectionState = (registration) => {{
    const viewport = intersectionViewport();
    const targetRect = intersectionRect(geometryForNode(registration.target));
    let rootRect = registration.root && Number(registration.root.nodeType) === 1
      ? intersectionRect(geometryForNode(registration.root))
      : viewport;
    rootRect = intersectionWithMargin(rootRect, registration.rootMargin);
    const rootIntersection = registration.root && Number(registration.root.nodeType) === 1
      ? intersectionWith(rootRect, viewport)
      : rootRect;
    const visible = intersectionWith(targetRect, rootIntersection);
    const targetArea = intersectionArea(targetRect);
    const visibleArea = intersectionArea(visible);
    const isIntersecting = visible.width > 0 && visible.height > 0;
    return {{
      rootBounds: rootIntersection,
      boundingClientRect: targetRect,
      intersectionRect: visible,
      isIntersecting,
      intersectionRatio: targetArea > 0 ? visibleArea / targetArea : (isIntersecting ? 1 : 0),
    }};
  }};
  const intersectionThresholdCrossed = (previous, next, thresholds) => {{
    if (!previous) return true;
    if (previous.isIntersecting !== next.isIntersecting) return true;
    return thresholds.some((threshold) =>
      (previous.intersectionRatio < threshold) !== (next.intersectionRatio < threshold)
    );
  }};
  const intersectionEntry = (target, observation) => new IntersectionObserverEntryNative(
    Number(host.now_ms) || 0,
    {{
      target,
      rootBounds: makeDomRect(observation.rootBounds),
      boundingClientRect: makeDomRect(observation.boundingClientRect),
      intersectionRect: makeDomRect(observation.intersectionRect),
      isIntersecting: observation.isIntersecting,
      intersectionRatio: observation.intersectionRatio,
      isVisible: observation.isIntersecting,
    }},
  );
  const deliverIntersectionObservers = () => {{
    intersectionDeliveryQueued = false;
    for (const observer of Array.from(intersectionObservers)) {{
      if (!observer.__glassRecords || observer.__glassRecords.length === 0) continue;
      const records = observer.__glassRecords.splice(0, observer.__glassRecords.length);
      observer.__glassCallback(records, observer);
    }}
  }};
  const scheduleIntersectionObserverDelivery = () => {{
    if (intersectionDeliveryQueued) return;
    intersectionDeliveryQueued = true;
    Promise.resolve().then(deliverIntersectionObservers);
  }};
  const queueIntersectionObserverChanges = () => {{
    for (const observer of Array.from(intersectionObservers)) {{
      for (const registration of observer.__glassRegistrations || []) {{
        const observation = intersectionState(registration);
        if (!intersectionThresholdCrossed(
          registration.last,
          observation,
          registration.thresholds,
        )) continue;
        registration.last = {{
          isIntersecting: observation.isIntersecting,
          intersectionRatio: observation.intersectionRatio,
        }};
        observer.__glassRecords.push(intersectionEntry(registration.target, observation));
        if (observer.__glassRecords.length > {max_commands}) {{
          observer.__glassRecords.splice(0, observer.__glassRecords.length - {max_commands});
        }}
      }}
    }}
    if (Array.from(intersectionObservers).some((observer) => observer.__glassRecords.length > 0)) {{
      scheduleIntersectionObserverDelivery();
    }}
  }};
  const commands = [];
  let suppressHostCommands = 0;
  const activeCommands = () => Array.isArray(globalThis.__glassHostCommandBuffer)
    ? globalThis.__glassHostCommandBuffer
    : commands;
  const pushCommand = (command) => {{
    if (suppressHostCommands > 0) return;
    const target = activeCommands();
    if (target.length >= {max_commands}) throw new RangeError("native host command limit exceeded");
    target.push(command);
    if (typeof globalThis.__glassRecordMutationCommand === "function") {{
      globalThis.__glassRecordMutationCommand(command);
    }}
  }};
  const timers = globalThis.__glassTimers instanceof Map
    ? globalThis.__glassTimers
    : new Map();
  const runningTimers = globalThis.__glassRunningTimers instanceof Map
    ? globalThis.__glassRunningTimers
    : new Map();
  const animationFrames = globalThis.__glassAnimationFrames instanceof Map
    ? globalThis.__glassAnimationFrames
    : new Map();
  const idleCallbacks = globalThis.__glassIdleCallbacks instanceof Map
    ? globalThis.__glassIdleCallbacks
    : new Map();
  let nextAnimationFrameId = Number.isSafeInteger(globalThis.__glassNextAnimationFrameId)
    ? globalThis.__glassNextAnimationFrameId
    : 1;
  let nextIdleCallbackId = Number.isSafeInteger(globalThis.__glassNextIdleCallbackId)
    ? globalThis.__glassNextIdleCallbackId
    : 1;
  let nextTimerId = Number.isSafeInteger(globalThis.__glassNextTimerId)
    ? globalThis.__glassNextTimerId
    : 1;
  const scheduleTimer = (callback, delay, args, repeating) => {{
    if (typeof callback !== "function") throw new TypeError("timer callback must be callable");
    if (timers.size >= {max_timers}) throw new RangeError("native timer limit exceeded");
    const id = nextTimerId;
    nextTimerId += 1;
    globalThis.__glassNextTimerId = nextTimerId;
    const numericDelay = Number(delay);
    const normalizedDelay = Number.isFinite(numericDelay)
      ? Math.max(0, Math.min(2147483647, numericDelay))
      : 0;
    timers.set(id, {{
      callback,
      args,
      dueAt: host.now_ms + normalizedDelay,
      intervalMs: repeating ? Math.max(1, normalizedDelay) : 0,
      cancelled: false,
    }});
    return id;
  }};
  const setTimeoutNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, false);
  const setIntervalNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, true);
  const clearTimer = (id) => {{
    const timerId = Number(id);
    const timer = timers.get(timerId) || runningTimers.get(timerId);
    if (timer) timer.cancelled = true;
    timers.delete(timerId);
  }};
  const requestAnimationFrameNative = (callback) => {{
    if (typeof callback !== "function") throw new TypeError("animation frame callback must be callable");
    if (animationFrames.size >= {max_timers}) throw new RangeError("native animation frame limit exceeded");
    const id = nextAnimationFrameId;
    nextAnimationFrameId += 1;
    globalThis.__glassNextAnimationFrameId = nextAnimationFrameId;
    animationFrames.set(id, {{
      callback,
      dueAt: host.now_ms + 16,
    }});
    return id;
  }};
  const cancelAnimationFrameNative = (id) => {{
    animationFrames.delete(Number(id));
  }};
  const requestIdleCallbackNative = (callback, options) => {{
    if (typeof callback !== "function") throw new TypeError("idle callback must be callable");
    if (options !== undefined && (options === null || typeof options !== "object"))
      throw new TypeError("idle callback options must be an object");
    if (idleCallbacks.size >= {max_timers}) throw new RangeError("native idle callback limit exceeded");
    const timeoutValue = options && options.timeout !== undefined ? Number(options.timeout) : null;
    const timeout = timeoutValue === null
      ? null
      : Number.isFinite(timeoutValue) ? Math.max(0, Math.min(2147483647, timeoutValue)) : null;
    const id = nextIdleCallbackId;
    nextIdleCallbackId += 1;
    globalThis.__glassNextIdleCallbackId = nextIdleCallbackId;
    idleCallbacks.set(id, {{
      callback,
      scheduledAt: host.now_ms,
      timeoutAt: timeout === null ? null : host.now_ms + timeout,
    }});
    return id;
  }};
  const cancelIdleCallbackNative = (id) => {{
    idleCallbacks.delete(Number(id));
  }};
  const storageEntryLimit = {storage_entry_limit};
  const storageKeyLimit = {storage_key_limit};
  const storageValueLimit = {storage_value_limit};
  const HTML_NAMESPACE = "http://www.w3.org/1999/xhtml";
  const SVG_NAMESPACE = "http://www.w3.org/2000/svg";
  const MATHML_NAMESPACE = "http://www.w3.org/1998/Math/MathML";
  const XML_NAMESPACE = "http://www.w3.org/XML/1998/namespace";
  const XMLNS_NAMESPACE = "http://www.w3.org/2000/xmlns/";
  const XLINK_NAMESPACE = "http://www.w3.org/1999/xlink";
  const boundedStorageText = (value, limit, field) => {{
    const text = String(value);
    if (text.length > limit) throw new RangeError("native storage " + field + " exceeds its limit");
    return text;
  }};
  const createStorage = (mapSlot, objectSlot, initialValues) => {{
    const hasExistingValues = globalThis[mapSlot] instanceof Map;
    const values = hasExistingValues ? globalThis[mapSlot] : new Map();
    if (!hasExistingValues && initialValues && typeof initialValues === "object") {{
      for (const key of Object.keys(initialValues)) {{
        values.set(key, String(initialValues[key]));
      }}
    }}
    globalThis[mapSlot] = values;
    const existing = globalThis[objectSlot];
    if (existing && existing.__glassNativeStorage === true) return existing;
    const store = {{
      get length() {{ return values.size; }},
      key(index) {{
        const position = Number(index);
        if (!Number.isInteger(position) || position < 0) return null;
        return Array.from(values.keys())[position] ?? null;
      }},
      getItem(key) {{
        const value = values.get(String(key));
        return value === undefined ? null : value;
      }},
      setItem(key, value) {{
        const normalizedKey = boundedStorageText(key, storageKeyLimit, "key");
        const normalizedValue = boundedStorageText(value, storageValueLimit, "value");
        if (!values.has(normalizedKey) && values.size >= storageEntryLimit) {{
          throw new RangeError("native storage entry limit exceeded");
        }}
        values.set(normalizedKey, normalizedValue);
        pushCommand({{ kind: "storageSet", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session", key: normalizedKey, value: normalizedValue }});
      }},
      removeItem(key) {{
        const normalizedKey = String(key);
        if (!values.delete(normalizedKey)) return;
        pushCommand({{ kind: "storageRemove", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session", key: normalizedKey }});
      }},
      clear() {{
        if (values.size === 0) return;
        values.clear();
        pushCommand({{ kind: "storageClear", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session" }});
      }},
    }};
    Object.defineProperty(store, "__glassNativeStorage", {{
      value: true,
      enumerable: false,
      configurable: false,
    }});
    globalThis[objectSlot] = store;
    return store;
  }};
  globalThis.localStorage = createStorage("__glassLocalStorageValues", "__glassLocalStorageObject", host.storage.local);
  globalThis.sessionStorage = createStorage("__glassSessionStorageValues", "__glassSessionStorageObject", host.storage.session);
  const indexedDbHostState = host.indexed_db && typeof host.indexed_db === "object"
    ? host.indexed_db
    : {{ databases: {{}} }};
  if (!indexedDbHostState.databases || typeof indexedDbHostState.databases !== "object") indexedDbHostState.databases = {{}};
  globalThis.__glassIndexedDbState = indexedDbHostState;
  const indexedDbState = () => globalThis.__glassIndexedDbState;
  const indexedDbConnections = globalThis.__glassIndexedDbConnections instanceof Map
    ? globalThis.__glassIndexedDbConnections
    : new Map();
  const indexedDbPendingOpens = globalThis.__glassIndexedDbPendingOpens instanceof Map
    ? globalThis.__glassIndexedDbPendingOpens
    : new Map();
  const indexedDbPendingDeletes = globalThis.__glassIndexedDbPendingDeletes instanceof Map
    ? globalThis.__glassIndexedDbPendingDeletes
    : new Map();
  const indexedDbTransactionQueues = globalThis.__glassIndexedDbTransactionQueues instanceof Map
    ? globalThis.__glassIndexedDbTransactionQueues
    : new Map();
  globalThis.__glassIndexedDbConnections = indexedDbConnections;
  globalThis.__glassIndexedDbPendingOpens = indexedDbPendingOpens;
  globalThis.__glassIndexedDbPendingDeletes = indexedDbPendingDeletes;
  globalThis.__glassIndexedDbTransactionQueues = indexedDbTransactionQueues;
  if (!globalThis.__glassNativeIndexedDBInstalled) {{
  const indexedDbDatabaseLimit = {indexed_db_database_limit};
  const indexedDbStoreLimit = {indexed_db_store_limit};
  const indexedDbIndexLimit = {indexed_db_index_limit};
  const indexedDbRecordLimit = {indexed_db_record_limit};
  const indexedDbValueLimit = {indexed_db_value_limit};
  const indexedDbSchedule = (callback) => Promise.resolve().then(callback);
  const indexedDbError = (name, message) => {{
    const error = new Error(message);
    error.name = name;
    return error;
  }};
  const indexedDbName = (value, field) => {{
    const name = String(value);
    if (!name || name.length > storageKeyLimit) throw indexedDbError("TypeError", field + " is outside the native limit");
    return name;
  }};
  const indexedDbTypeKey = "__glassNativeIndexedDbType";
  const indexedDbTypedArrayNames = new Set([
    "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array",
    "Uint16Array", "Int32Array", "Uint32Array", "Float32Array", "Float64Array",
    "BigInt64Array", "BigUint64Array",
  ]);
  const indexedDbEncode = (value, seen = new Set(), depth = 0) => {{
    if (depth > 64) throw indexedDbError("DataCloneError", "native IndexedDB value is too deeply nested");
    if (value === undefined) return {{ [indexedDbTypeKey]: "undefined" }};
    if (value === null || typeof value === "boolean" || typeof value === "string") return value;
    if (typeof value === "number") {{
      if (Number.isFinite(value) && !Object.is(value, -0)) return value;
      return {{
        [indexedDbTypeKey]: "number",
        value: Number.isNaN(value) ? "NaN" : (value === Infinity ? "Infinity" : (value === -Infinity ? "-Infinity" : "-0")),
      }};
    }}
    if (typeof value === "bigint" || typeof value === "function" || typeof value === "symbol") throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    if (seen.has(value)) throw indexedDbError("DataCloneError", "cyclic value cannot be cloned by native IndexedDB");
    seen.add(value);
    let encoded;
    if (typeof SharedArrayBuffer === "function" && value instanceof SharedArrayBuffer) throw indexedDbError("DataCloneError", "shared buffers cannot be cloned by native IndexedDB");
    if (value instanceof ArrayBuffer) {{
      encoded = {{
        [indexedDbTypeKey]: "arrayBuffer",
        bytes: Array.from(new Uint8Array(value)),
      }};
    }} else if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value)) {{
      const constructorName = value.constructor && value.constructor.name;
      if (constructorName !== "DataView" && !indexedDbTypedArrayNames.has(constructorName)) throw indexedDbError("DataCloneError", "typed array cannot be cloned by native IndexedDB");
      encoded = {{
        [indexedDbTypeKey]: "typedArray",
        constructor: constructorName,
        bytes: Array.from(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)),
      }};
    }} else if (value && value.__glassNativeBlob === true) {{
      encoded = {{
        [indexedDbTypeKey]: "blob",
        text: String(value._text || ""),
        type: String(value.type || ""),
        file: value.__glassNativeFile === true,
        name: value.__glassNativeFile === true ? String(value.name || "") : "",
        lastModified: value.__glassNativeFile === true ? Number(value.lastModified || 0) : 0,
      }};
    }} else if (value instanceof Date) {{
      const timestamp = value.getTime();
      if (!Number.isFinite(timestamp)) throw indexedDbError("DataCloneError", "invalid date cannot be cloned by native IndexedDB");
      encoded = {{ [indexedDbTypeKey]: "date", value: timestamp }};
    }} else if (value instanceof RegExp) {{
      encoded = {{ [indexedDbTypeKey]: "regexp", source: value.source, flags: value.flags }};
    }} else if (value instanceof Map) {{
      encoded = {{
        [indexedDbTypeKey]: "map",
        entries: Array.from(value.entries(), entry => [
          indexedDbEncode(entry[0], seen, depth + 1),
          indexedDbEncode(entry[1], seen, depth + 1),
        ]),
      }};
    }} else if (value instanceof Set) {{
      encoded = {{
        [indexedDbTypeKey]: "set",
        values: Array.from(value.values(), entry => indexedDbEncode(entry, seen, depth + 1)),
      }};
    }} else if (Array.isArray(value)) {{
      encoded = Array.from(value, entry => indexedDbEncode(entry, seen, depth + 1));
    }} else {{
      if (Object.prototype.hasOwnProperty.call(value, indexedDbTypeKey)) throw indexedDbError("DataCloneError", "reserved native IndexedDB value tag is not writable");
      encoded = Object.create(null);
      for (const key of Object.keys(value)) encoded[key] = indexedDbEncode(value[key], seen, depth + 1);
    }}
    seen.delete(value);
    return encoded;
  }};
  const indexedDbDecode = (value) => {{
    if (Array.isArray(value)) return value.map(entry => indexedDbDecode(entry));
    if (!value || typeof value !== "object") return value;
    const type = value[indexedDbTypeKey];
    if (type === "undefined") return undefined;
    if (type === "number") return value.value === "NaN" ? NaN : (value.value === "Infinity" ? Infinity : (value.value === "-Infinity" ? -Infinity : -0));
    if (type === "date") return new Date(value.value);
    if (type === "regexp") return new RegExp(value.source, value.flags);
    if (type === "map") return new Map((value.entries || []).map(entry => [indexedDbDecode(entry[0]), indexedDbDecode(entry[1])]));
    if (type === "set") return new Set((value.values || []).map(entry => indexedDbDecode(entry)));
    if (type === "arrayBuffer") return new Uint8Array(value.bytes || []).buffer;
    if (type === "typedArray") {{
      const buffer = new Uint8Array(value.bytes || []).buffer;
      if (value.constructor === "DataView") return new DataView(buffer);
      if (!indexedDbTypedArrayNames.has(value.constructor) || typeof globalThis[value.constructor] !== "function") throw indexedDbError("DataCloneError", "typed array cannot be reconstructed by native IndexedDB");
      return new globalThis[value.constructor](buffer);
    }}
    if (type === "blob") return value.file
      ? new File([value.text], value.name, {{ type: value.type, lastModified: value.lastModified }})
      : new Blob([value.text], {{ type: value.type }});
    const decoded = {{}};
    for (const key of Object.keys(value)) Object.defineProperty(decoded, key, {{
      value: indexedDbDecode(value[key]),
      enumerable: true,
      configurable: true,
      writable: true,
    }});
    return decoded;
  }};
  const indexedDbStoredClone = (value) => indexedDbDecode(value);
  const indexedDbClone = (value) => {{
    let serialized;
    try {{ serialized = JSON.stringify(indexedDbEncode(value)); }} catch (error) {{
      if (error && (error.name === "DataCloneError" || error.name === "QuotaExceededError")) throw error;
      throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    }}
    if (serialized === undefined) throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    if (serialized.length > indexedDbValueLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB value limit exceeded");
    return indexedDbDecode(JSON.parse(serialized));
  }};
  const indexedDbKeyToken = (key, allowUndefined) => {{
    if (key === undefined && allowUndefined) return undefined;
    if (typeof key === "string") {{
      if (key.length > storageKeyLimit) throw indexedDbError("DataError", "native IndexedDB key limit exceeded");
      return "s:" + key;
    }}
    if (typeof key === "number" && Number.isFinite(key)) {{
      const normalized = Object.is(key, -0) ? 0 : key;
      return "n:" + String(normalized);
    }}
    throw indexedDbError("DataError", "native IndexedDB supports only string and finite number keys");
  }};
  const indexedDbKeyValue = (token) => token.startsWith("s:")
    ? token.slice(2)
    : Number(token.slice(2));
  const indexedDbCompareTokens = (left, right) => {{
    const leftNumber = left.startsWith("n:");
    const rightNumber = right.startsWith("n:");
    if (leftNumber !== rightNumber) return leftNumber ? -1 : 1;
    const leftValue = indexedDbKeyValue(left);
    const rightValue = indexedDbKeyValue(right);
    return leftValue < rightValue ? -1 : (leftValue > rightValue ? 1 : 0);
  }};
  const indexedDbRangeIncludes = (range, token) => {{
    if (!range) return true;
    if (range.__lowerToken !== undefined) {{
      const comparison = indexedDbCompareTokens(token, range.__lowerToken);
      if (comparison < 0 || (comparison === 0 && range.lowerOpen)) return false;
    }}
    if (range.__upperToken !== undefined) {{
      const comparison = indexedDbCompareTokens(token, range.__upperToken);
      if (comparison > 0 || (comparison === 0 && range.upperOpen)) return false;
    }}
    return true;
  }};
  const makeIndexedDbKeyRange = (lower, upper, lowerOpen, upperOpen) => {{
    if (lower !== undefined && upper !== undefined) {{
      const comparison = indexedDbCompareTokens(lower, upper);
      if (comparison > 0 || (comparison === 0 && (lowerOpen || upperOpen))) throw indexedDbError("DataError", "native IndexedDB key range bounds are invalid");
    }}
    const range = {{
      lower: lower === undefined ? undefined : indexedDbKeyValue(lower),
      upper: upper === undefined ? undefined : indexedDbKeyValue(upper),
      lowerOpen: Boolean(lowerOpen),
      upperOpen: Boolean(upperOpen),
    }};
    Object.defineProperties(range, {{
      __glassNativeKeyRange: {{ value: true }},
      __lowerToken: {{ value: lower }},
      __upperToken: {{ value: upper }},
    }});
    range.includes = (value) => indexedDbRangeIncludes(range, indexedDbKeyToken(value, false));
    return range;
  }};
  const indexedDbQueryRange = (query) => {{
    if (query === undefined) return null;
    if (query && query.__glassNativeKeyRange === true) return query;
    const token = indexedDbKeyToken(query, false);
    return makeIndexedDbKeyRange(token, token, false, false);
  }};
  const indexedDbQueryLimit = (count) => {{
    if (count === undefined) return indexedDbRecordLimit;
    const number = Number(count);
    if (!Number.isInteger(number) || number < 0) throw indexedDbError("TypeError", "native IndexedDB count must be a non-negative integer");
    return Math.min(indexedDbRecordLimit, number);
  }};
  const indexedDbDirection = (direction) => {{
    const value = direction === undefined ? "next" : String(direction);
    if (!["next", "nextunique", "prev", "prevunique"].includes(value)) throw indexedDbError("TypeError", "native IndexedDB cursor direction is unsupported");
    return value;
  }};
  const indexedDbReadKeyPath = (value, keyPath) => {{
    if (!keyPath) return undefined;
    let current = value;
    for (const part of keyPath.split(".")) {{
      if (current === null || current === undefined || typeof current !== "object") return undefined;
      current = current[part];
    }}
    return current;
  }};
  const indexedDbWriteKeyPath = (value, keyPath, key) => {{
    if (!keyPath || value === null || typeof value !== "object") throw indexedDbError("DataError", "auto-increment key path requires an object");
    const parts = keyPath.split(".");
    let current = value;
    for (let index = 0; index < parts.length - 1; index += 1) {{
      const part = parts[index];
      if (!current[part] || typeof current[part] !== "object") current[part] = {{}};
      current = current[part];
    }}
    current[parts[parts.length - 1]] = key;
  }};
  const indexedDbIndexKeys = (value, index) => {{
    const extracted = indexedDbReadKeyPath(indexedDbDecode(value), index.key_path);
    if (extracted === undefined) return [];
    const candidates = index.multi_entry && Array.isArray(extracted) ? extracted : [extracted];
    const keys = [];
    for (const candidate of candidates) {{
      const token = indexedDbKeyToken(candidate, false);
      if (!keys.includes(token)) keys.push(token);
    }}
    return keys;
  }};
  const indexedDbObjectEntries = (store) => Object.keys(store.records)
    .map(primaryToken => ({{ indexToken: primaryToken, primaryToken, value: store.records[primaryToken] }}))
    .sort((left, right) => indexedDbCompareTokens(left.primaryToken, right.primaryToken));
  const indexedDbIndexEntries = (store, index) => {{
    const entries = [];
    for (const primaryToken of Object.keys(store.records)) {{
      for (const indexToken of indexedDbIndexKeys(store.records[primaryToken], index)) {{
        entries.push({{ indexToken, primaryToken, value: store.records[primaryToken] }});
      }}
    }}
    return entries.sort((left, right) =>
      indexedDbCompareTokens(left.indexToken, right.indexToken)
      || indexedDbCompareTokens(left.primaryToken, right.primaryToken));
  }};
  const indexedDbOrderedEntries = (entries, range, direction) => {{
    const ordered = entries.filter(entry => indexedDbRangeIncludes(range, entry.indexToken));
    if (direction.startsWith("prev")) ordered.reverse();
    if (direction.endsWith("unique")) {{
      const seen = new Set();
      return ordered.filter(entry => {{
        if (seen.has(entry.indexToken)) return false;
        seen.add(entry.indexToken);
        return true;
      }});
    }}
    return ordered;
  }};
  const makeIndexedDbRequest = () => ({{
    result: undefined,
    error: null,
    readyState: "pending",
    onupgradeneeded: null,
    onblocked: null,
    onsuccess: null,
    onerror: null,
  }});
  const finishIndexedDbRequest = (request, value, error, after) => {{
    indexedDbSchedule(() => {{
      request.readyState = "done";
      if (error) {{
        request.error = error;
        if (typeof request.onerror === "function") request.onerror.call(request, {{ target: request }});
      }} else {{
        request.result = value;
        if (typeof request.onsuccess === "function") request.onsuccess.call(request, {{ target: request }});
      }}
      if (typeof after === "function") after();
    }});
  }};
  const finishIndexedDbTransaction = (transaction) => {{
    if (transaction.__finished || transaction.__pending !== 0) return;
    transaction.__finished = true;
    releaseIndexedDbTransaction(transaction);
    if (transaction.__aborted) {{
      if (!transaction.__abortDispatched) {{
        transaction.__abortDispatched = true;
        if (typeof transaction.onabort === "function") transaction.onabort.call(transaction, {{ target: transaction }});
      }}
      return;
    }}
    if (typeof transaction.oncomplete === "function") transaction.oncomplete.call(transaction, {{ target: transaction }});
  }};
  const maybeFinishIndexedDbTransaction = (transaction) => {{
    if (transaction.__pending !== 0 || transaction.__finished || transaction.__finishScheduled) return;
    transaction.__finishScheduled = true;
    indexedDbSchedule(() => {{
      transaction.__finishScheduled = false;
      finishIndexedDbTransaction(transaction);
    }});
  }};
  const drainIndexedDbTransactions = (databaseName) => {{
    const queue = indexedDbTransactionQueues.get(databaseName);
    if (!queue || queue.length === 0) return;
    const transaction = queue[0];
    if (transaction.__running || transaction.__operationQueue.length === 0) return;
    const next = transaction.__operationQueue.shift();
    transaction.__running = true;
    indexedDbSchedule(() => {{
      if (transaction.__initialState === null && !transaction.__aborted) transaction.__initialState = cloneIndexedDbDatabaseState(transaction.__databaseState);
      let value;
      let error = null;
      if (transaction.__aborted) {{
        error = indexedDbError("AbortError", "native IndexedDB transaction was aborted");
      }} else {{
        try {{ value = next.operation(); }} catch (caught) {{
          error = caught instanceof Error ? caught : indexedDbError("UnknownError", String(caught));
          transaction.__aborted = true;
          transaction.error = error;
          transaction.__rollback();
          if (typeof transaction.onerror === "function") transaction.onerror.call(transaction, {{ target: transaction }});
        }}
      }}
      finishIndexedDbRequest(next.request, value, error, () => {{
        transaction.__pending -= 1;
        transaction.__running = false;
        drainIndexedDbTransactions(databaseName);
        maybeFinishIndexedDbTransaction(transaction);
      }});
    }});
  }};
  const releaseIndexedDbTransaction = (transaction) => {{
    const queue = indexedDbTransactionQueues.get(transaction.db.name);
    if (!queue) return;
    const position = queue.indexOf(transaction);
    if (position >= 0) queue.splice(position, 1);
    if (queue.length === 0) indexedDbTransactionQueues.delete(transaction.db.name);
    else drainIndexedDbTransactions(transaction.db.name);
  }};
  const queueIndexedDbRequest = (transaction, operation) => {{
    if (transaction.__finished || transaction.__aborted) throw indexedDbError("TransactionInactiveError", "native IndexedDB transaction is inactive");
    const request = makeIndexedDbRequest();
    transaction.__pending += 1;
    transaction.__operationQueue.push({{ request, operation }});
    let queue = indexedDbTransactionQueues.get(transaction.db.name);
    if (!queue) {{
      queue = [];
      indexedDbTransactionQueues.set(transaction.db.name, queue);
    }}
    if (!queue.includes(transaction)) queue.push(transaction);
    drainIndexedDbTransactions(transaction.db.name);
    return request;
  }};
  const cloneIndexedDbDatabaseState = (databaseState) => JSON.parse(JSON.stringify(databaseState));
  const restoreIndexedDbDatabaseState = (databaseState, snapshot) => {{
    const stores = databaseState.stores || {{}};
    const snapshotStores = snapshot.stores || {{}};
    for (const name of Object.keys(stores)) {{
      if (!Object.prototype.hasOwnProperty.call(snapshotStores, name)) delete stores[name];
    }}
    for (const [name, snapshotStore] of Object.entries(snapshotStores)) {{
      const currentStore = stores[name];
      if (!currentStore) {{
        stores[name] = snapshotStore;
        continue;
      }}
      for (const key of Object.keys(currentStore)) delete currentStore[key];
      for (const [key, value] of Object.entries(snapshotStore)) currentStore[key] = value;
    }}
    databaseState.version = snapshot.version;
    databaseState.stores = stores;
  }};
  const makeIndexedDbTransaction = (database, databaseState, storeNames, mode, upgrade) => {{
    const transaction = {{
      db: database,
      mode,
      error: null,
      __storeNames: storeNames,
      __databaseState: databaseState,
      __initialState: null,
      __rolledBack: false,
      get objectStoreNames() {{ return storeNames.slice().sort(); }},
      oncomplete: null,
      onerror: null,
      onabort: null,
      __upgrade: upgrade,
      __pending: 0,
      __aborted: false,
      __finished: false,
      __finishScheduled: false,
      __abortDispatched: false,
      __operationQueue: [],
      __running: false,
      __rollback() {{
        if (this.__rolledBack) return;
        if (this.__initialState !== null) restoreIndexedDbDatabaseState(databaseState, this.__initialState);
        this.__rolledBack = true;
      }},
      abort() {{
        if (this.__finished) throw indexedDbError("InvalidStateError", "native IndexedDB transaction is inactive");
        if (this.__aborted) return;
        this.__aborted = true;
        this.error = indexedDbError("AbortError", "native IndexedDB transaction was aborted");
        this.__rollback();
        maybeFinishIndexedDbTransaction(this);
      }},
      objectStore(name) {{
        const normalizedName = indexedDbName(name, "object store name");
        if (!storeNames.includes(normalizedName) || !databaseState.stores[normalizedName]) throw indexedDbError("NotFoundError", "object store is not in this transaction");
        const store = databaseState.stores[normalizedName];
        return makeIndexedDbObjectStore(this, store, normalizedName);
      }},
    }};
    indexedDbSchedule(() => indexedDbSchedule(() => maybeFinishIndexedDbTransaction(transaction)));
    return transaction;
  }};
  const makeIndexedDbCursor = (transaction, request, entries, indexCursor, keyOnly, direction, source, store, resolveCursorRecord, ensureIndexConstraints) => {{
    let position = 0;
    let pending = false;
    let done = false;
    const currentEntry = () => entries[position];
    const currentKeyToken = () => {{
      const entry = currentEntry();
      return indexCursor ? entry.indexToken : entry.primaryToken;
    }};
    const queuePosition = (nextPosition) => {{
      if (done || pending || transaction.__finished || transaction.__aborted) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is inactive");
      pending = true;
      transaction.__pending += 1;
      indexedDbSchedule(() => {{
        pending = false;
        position = nextPosition;
        const entry = currentEntry();
        if (!entry) {{
          done = true;
          request.result = null;
        }} else {{
          request.result = cursor;
        }}
        request.readyState = "done";
        if (typeof request.onsuccess === "function") request.onsuccess.call(request, {{ target: request }});
        transaction.__pending -= 1;
        maybeFinishIndexedDbTransaction(transaction);
      }});
    }};
    const seekToKey = (token, start) => {{
      const forward = direction.startsWith("next");
      let candidate = start;
      while (candidate < entries.length) {{
        const comparison = indexedDbCompareTokens(currentKeyTokenFor(entries[candidate]), token);
        if ((forward && comparison >= 0) || (!forward && comparison <= 0)) break;
        candidate += 1;
      }}
      return candidate;
    }};
    const seekToPair = (indexToken, primaryToken, start) => {{
      const forward = direction.startsWith("next");
      let candidate = start;
      while (candidate < entries.length) {{
        const entry = entries[candidate];
        const indexComparison = indexedDbCompareTokens(entry.indexToken, indexToken);
        const comparison = indexComparison || indexedDbCompareTokens(entry.primaryToken, primaryToken);
        if ((forward && comparison >= 0) || (!forward && comparison <= 0)) break;
        candidate += 1;
      }}
      return candidate;
    }};
    const cursor = {{
      source,
      get direction() {{ return direction; }},
      get key() {{ const entry = currentEntry(); return entry ? indexedDbKeyValue(entry.indexToken) : undefined; }},
      get primaryKey() {{ const entry = currentEntry(); return entry ? indexedDbKeyValue(entry.primaryToken) : undefined; }},
      get request() {{ return request; }},
      get value() {{
        if (keyOnly) return undefined;
        const entry = currentEntry();
        return entry ? indexedDbStoredClone(entry.value) : undefined;
      }},
      continue(key) {{
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        let nextPosition = position + 1;
        if (key !== undefined) {{
          const token = indexedDbKeyToken(key, false);
          const comparison = indexedDbCompareTokens(token, currentKeyToken());
          if ((direction.startsWith("next") && comparison <= 0) || (direction.startsWith("prev") && comparison >= 0)) throw indexedDbError("DataError", "native IndexedDB cursor key must advance");
          nextPosition = seekToKey(token, nextPosition);
        }}
        queuePosition(nextPosition);
      }},
      advance(count) {{
        if (!Number.isInteger(Number(count)) || Number(count) < 1) throw indexedDbError("TypeError", "native IndexedDB cursor advance count must be positive");
        queuePosition(position + Number(count));
      }},
      continuePrimaryKey(key, primaryKey) {{
        if (!indexCursor) throw indexedDbError("InvalidAccessError", "native IndexedDB primary-key continuation requires an index cursor");
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        const indexToken = indexedDbKeyToken(key, false);
        const primaryToken = indexedDbKeyToken(primaryKey, false);
        const comparison = indexedDbCompareTokens(indexToken, currentKeyToken()) || indexedDbCompareTokens(primaryToken, currentEntry().primaryToken);
        if ((direction.startsWith("next") && comparison <= 0) || (direction.startsWith("prev") && comparison >= 0)) throw indexedDbError("DataError", "native IndexedDB cursor key must advance");
        queuePosition(seekToPair(indexToken, primaryToken, position + 1));
      }},
      update(value) {{
        if (keyOnly) throw indexedDbError("InvalidStateError", "native IndexedDB key cursor cannot update records");
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        if (transaction.mode === "readonly") throw indexedDbError("ReadOnlyError", "native IndexedDB transaction is read-only");
        const primaryToken = currentEntry().primaryToken;
        return queueIndexedDbRequest(transaction, () => {{
          const record = resolveCursorRecord(value, primaryToken);
          ensureIndexConstraints(record.token, record.value);
          store.records[record.token] = record.value;
          currentEntry().value = record.value;
          return record.key;
        }});
      }},
      delete() {{
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        if (transaction.mode === "readonly") throw indexedDbError("ReadOnlyError", "native IndexedDB transaction is read-only");
        const primaryToken = currentEntry().primaryToken;
        return queueIndexedDbRequest(transaction, () => {{ delete store.records[primaryToken]; return undefined; }});
      }},
    }};
    const currentKeyTokenFor = (entry) => indexCursor ? entry.indexToken : entry.primaryToken;
    return cursor;
  }};
  const makeIndexedDbIndex = (transaction, store, name, metadata, resolveCursorRecord, ensureIndexConstraints) => {{
    const queryEntries = (query, direction) => indexedDbOrderedEntries(indexedDbIndexEntries(store, metadata), query, direction);
    let index;
    index = {{
      name,
      keyPath: metadata.key_path,
      multiEntry: metadata.multi_entry,
      unique: metadata.unique,
      get(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = queryEntries(range, "next")[0];
          return entry ? indexedDbStoredClone(entry.value) : undefined;
        }});
      }},
      getKey(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = queryEntries(range, "next")[0];
          return entry ? indexedDbKeyValue(entry.primaryToken) : undefined;
        }});
      }},
      getAll(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => queryEntries(range, "next").slice(0, limit).map(entry => indexedDbStoredClone(entry.value)));
      }},
      getAllKeys(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => queryEntries(range, "next").slice(0, limit).map(entry => indexedDbKeyValue(entry.primaryToken)));
      }},
      count(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => queryEntries(range, "next").length);
      }},
      openCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = queryEntries(range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, true, false, selectedDirection, index, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
      openKeyCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = queryEntries(range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, true, true, selectedDirection, index, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
    }};
    return index;
  }};
  const makeIndexedDbObjectStore = (transaction, store, name) => {{
    const requireWritable = () => {{
      if (transaction.mode === "readonly") throw indexedDbError("ReadOnlyError", "native IndexedDB transaction is read-only");
    }};
    const resolveRecord = (value, key) => {{
      const cloned = indexedDbClone(value);
      if (store.key_path && key !== undefined) throw indexedDbError("InvalidAccessError", "key path stores do not accept an explicit key");
      let resolved = store.key_path ? indexedDbReadKeyPath(cloned, store.key_path) : key;
      if (resolved === undefined && store.auto_increment) {{
        if (!Number.isSafeInteger(store.next_key) || store.next_key <= 0) throw indexedDbError("QuotaExceededError", "native IndexedDB key space exhausted");
        resolved = store.next_key;
        store.next_key += 1;
        indexedDbWriteKeyPath(cloned, store.key_path, resolved);
      }}
      const token = indexedDbKeyToken(resolved, false);
      return {{ token, value: indexedDbEncode(cloned), key: indexedDbKeyValue(token) }};
    }};
    const resolveCursorRecord = (value, primaryToken) => {{
      const cloned = indexedDbClone(value);
      const token = store.key_path
        ? indexedDbKeyToken(indexedDbReadKeyPath(cloned, store.key_path), false)
        : primaryToken;
      if (token !== primaryToken) throw indexedDbError("DataError", "native IndexedDB cursor update cannot change the primary key");
      return {{ token, value: indexedDbEncode(cloned), key: indexedDbKeyValue(token) }};
    }};
    const ensureIndexConstraints = (primaryToken, value) => {{
      for (const index of Object.values(store.indexes)) {{
        if (!index.unique) continue;
        const candidateKeys = indexedDbIndexKeys(value, index);
        for (const existingToken of Object.keys(store.records)) {{
          if (existingToken === primaryToken) continue;
          const existingKeys = indexedDbIndexKeys(store.records[existingToken], index);
          if (candidateKeys.some(candidate => existingKeys.includes(candidate))) throw indexedDbError("ConstraintError", "native IndexedDB unique index constraint failed");
        }}
      }}
    }};
    let objectStore;
    objectStore = {{
      name,
      keyPath: store.key_path,
      autoIncrement: store.auto_increment,
      get indexNames() {{ return Object.keys(store.indexes).sort(); }},
      createIndex(indexName, keyPath, options) {{
        if (!transaction.__upgrade || transaction.__finished) throw indexedDbError("InvalidStateError", "indexes can only be created during upgrade");
        const normalizedName = indexedDbName(indexName, "index name");
        if (store.indexes[normalizedName]) throw indexedDbError("ConstraintError", "index already exists");
        if (Object.keys(store.indexes).length >= indexedDbIndexLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB index limit exceeded");
        if (Array.isArray(keyPath)) throw indexedDbError("TypeError", "native IndexedDB compound index key paths are unsupported");
        const normalizedKeyPath = indexedDbName(keyPath, "index key path");
        const metadata = {{
          key_path: normalizedKeyPath,
          unique: Boolean(options && options.unique),
          multi_entry: Boolean(options && options.multiEntry),
        }};
        const seen = new Set();
        for (const value of Object.values(store.records)) {{
          for (const token of indexedDbIndexKeys(value, metadata)) {{
            if (metadata.unique && seen.has(token)) throw indexedDbError("ConstraintError", "native IndexedDB unique index constraint failed");
            seen.add(token);
          }}
        }}
        store.indexes[normalizedName] = metadata;
        return makeIndexedDbIndex(transaction, store, normalizedName, metadata, resolveCursorRecord, ensureIndexConstraints);
      }},
      deleteIndex(indexName) {{
        if (!transaction.__upgrade || transaction.__finished) throw indexedDbError("InvalidStateError", "indexes can only be deleted during upgrade");
        const normalizedName = indexedDbName(indexName, "index name");
        if (!store.indexes[normalizedName]) throw indexedDbError("NotFoundError", "index does not exist");
        delete store.indexes[normalizedName];
      }},
      get(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = indexedDbObjectEntries(store).find(candidate => indexedDbRangeIncludes(range, candidate.indexToken));
          return entry ? indexedDbStoredClone(entry.value) : undefined;
        }});
      }},
      getKey(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = indexedDbObjectEntries(store).find(candidate => indexedDbRangeIncludes(range, candidate.indexToken));
          return entry ? indexedDbKeyValue(entry.primaryToken) : undefined;
        }});
      }},
      getAll(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => indexedDbObjectEntries(store)
          .filter(entry => indexedDbRangeIncludes(range, entry.indexToken))
          .slice(0, limit)
          .map(entry => indexedDbStoredClone(entry.value)));
      }},
      getAllKeys(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => indexedDbObjectEntries(store)
          .filter(entry => indexedDbRangeIncludes(range, entry.indexToken))
          .slice(0, limit)
          .map(entry => indexedDbKeyValue(entry.primaryToken)));
      }},
      count(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => indexedDbObjectEntries(store)
          .filter(entry => indexedDbRangeIncludes(range, entry.indexToken)).length);
      }},
      openCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = indexedDbOrderedEntries(indexedDbObjectEntries(store), range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, false, false, selectedDirection, () => objectStore, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
      openKeyCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = indexedDbOrderedEntries(indexedDbObjectEntries(store), range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, false, true, selectedDirection, () => objectStore, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
      index(indexName) {{
        const normalizedName = indexedDbName(indexName, "index name");
        const metadata = store.indexes[normalizedName];
        if (!metadata) throw indexedDbError("NotFoundError", "index does not exist");
        return makeIndexedDbIndex(transaction, store, normalizedName, metadata, resolveCursorRecord, ensureIndexConstraints);
      }},
      put(value, key) {{
        requireWritable();
        return queueIndexedDbRequest(transaction, () => {{
          const record = resolveRecord(value, key);
          if (store.records[record.token] === undefined && Object.keys(store.records).length >= indexedDbRecordLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB record limit exceeded");
          ensureIndexConstraints(record.token, record.value);
          store.records[record.token] = record.value;
          return record.key;
        }});
      }},
      add(value, key) {{
        requireWritable();
        return queueIndexedDbRequest(transaction, () => {{
          const record = resolveRecord(value, key);
          if (store.records[record.token] !== undefined) throw indexedDbError("ConstraintError", "native IndexedDB key already exists");
          if (Object.keys(store.records).length >= indexedDbRecordLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB record limit exceeded");
          ensureIndexConstraints(record.token, record.value);
          store.records[record.token] = record.value;
          return record.key;
        }});
      }},
      delete(query) {{
        requireWritable();
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          for (const entry of indexedDbObjectEntries(store)) if (indexedDbRangeIncludes(range, entry.indexToken)) delete store.records[entry.primaryToken];
          return undefined;
        }});
      }},
      clear() {{
        requireWritable();
        return queueIndexedDbRequest(transaction, () => {{ store.records = {{}}; return undefined; }});
      }},
    }};
    return objectStore;
  }};
  const removeIndexedDbConnection = (database) => {{
    const connections = indexedDbConnections.get(database.name) || [];
    const remaining = connections.filter(candidate => candidate !== database && !candidate.__closed);
    if (remaining.length === 0) indexedDbConnections.delete(database.name);
    else indexedDbConnections.set(database.name, remaining);
    const pendingDelete = indexedDbPendingDeletes.get(database.name);
    if (pendingDelete && remaining.length === 0) {{
      indexedDbPendingDeletes.delete(database.name);
      indexedDbSchedule(() => processIndexedDbDelete(pendingDelete, database.name));
      return;
    }}
    const pending = indexedDbPendingOpens.get(database.name);
    if (pending && remaining.length === 0) {{
      indexedDbPendingOpens.delete(database.name);
      indexedDbSchedule(() => processIndexedDbOpen(pending.request, database.name, pending.requestedVersion));
    }}
  }};
  const registerIndexedDbConnection = (database) => {{
    const connections = indexedDbConnections.get(database.name) || [];
    connections.push(database);
    indexedDbConnections.set(database.name, connections);
  }};
  const makeIndexedDbDatabase = (name, databaseState, upgradeTransaction) => {{
    let closed = false;
    const database = {{
      name,
      __upgradeTransaction: upgradeTransaction,
      get __closed() {{ return closed; }},
      onversionchange: null,
      onclose: null,
      get version() {{ return databaseState.version; }},
      get objectStoreNames() {{ return Object.keys(databaseState.stores).sort(); }},
      createObjectStore(storeName, options) {{
        const activeUpgradeTransaction = database.__upgradeTransaction;
        if (!activeUpgradeTransaction || activeUpgradeTransaction.__finished) throw indexedDbError("InvalidStateError", "object stores can only be created during upgrade");
        const normalizedName = indexedDbName(storeName, "object store name");
        if (Object.keys(databaseState.stores).length >= indexedDbStoreLimit && !databaseState.stores[normalizedName]) throw indexedDbError("QuotaExceededError", "native IndexedDB object store limit exceeded");
        if (databaseState.stores[normalizedName]) throw indexedDbError("ConstraintError", "native IndexedDB object store already exists");
        const keyPath = options && options.keyPath !== undefined && options.keyPath !== null
          ? indexedDbName(options.keyPath, "key path")
          : null;
        const autoIncrement = Boolean(options && options.autoIncrement);
        if (autoIncrement && !keyPath) throw indexedDbError("InvalidAccessError", "auto-increment stores require a key path");
        databaseState.stores[normalizedName] = {{
          key_path: keyPath,
          auto_increment: autoIncrement,
          next_key: 1,
          indexes: {{}},
          records: {{}},
        }};
        if (!activeUpgradeTransaction.__storeNames.includes(normalizedName)) activeUpgradeTransaction.__storeNames.push(normalizedName);
        return makeIndexedDbObjectStore(activeUpgradeTransaction, databaseState.stores[normalizedName], normalizedName);
      }},
      deleteObjectStore(storeName) {{
        const activeUpgradeTransaction = database.__upgradeTransaction;
        if (!activeUpgradeTransaction || activeUpgradeTransaction.__finished) throw indexedDbError("InvalidStateError", "object stores can only be deleted during upgrade");
        const normalizedName = indexedDbName(storeName, "object store name");
        if (!databaseState.stores[normalizedName]) throw indexedDbError("NotFoundError", "object store does not exist");
        delete databaseState.stores[normalizedName];
        const position = activeUpgradeTransaction.__storeNames.indexOf(normalizedName);
        if (position >= 0) activeUpgradeTransaction.__storeNames.splice(position, 1);
      }},
      transaction(storeNames, mode) {{
        if (closed) throw indexedDbError("InvalidStateError", "native IndexedDB database is closed");
        const names = Array.isArray(storeNames) ? storeNames.map(value => indexedDbName(value, "object store name")) : [indexedDbName(storeNames, "object store name")];
        const selectedMode = mode === undefined ? "readonly" : String(mode);
        if (!["readonly", "readwrite"].includes(selectedMode)) throw indexedDbError("TypeError", "native IndexedDB transaction mode is unsupported");
        for (const storeName of names) if (!databaseState.stores[storeName]) throw indexedDbError("NotFoundError", "object store does not exist");
        return makeIndexedDbTransaction(database, databaseState, names, selectedMode, false);
      }},
      close() {{
        if (closed) return;
        closed = true;
        removeIndexedDbConnection(database);
        if (typeof database.onclose === "function") database.onclose.call(database, {{ target: database }});
      }},
    }};
    return database;
  }};
  const processIndexedDbOpen = (request, normalizedName, requestedVersion) => {{
    const databaseState = indexedDbState().databases[normalizedName];
    const oldVersion = databaseState ? Number(databaseState.version) : 0;
    const nextVersion = requestedVersion === undefined ? (databaseState ? oldVersion : 1) : requestedVersion;
    if (databaseState && nextVersion < oldVersion) {{
      finishIndexedDbRequest(request, undefined, indexedDbError("VersionError", "native IndexedDB version is older than the database"), null);
      return;
    }}
    const upgraded = nextVersion > oldVersion;
    if (upgraded) {{
      const connections = indexedDbConnections.get(normalizedName) || [];
      for (const connection of connections) {{
        if (connection.__closed || typeof connection.onversionchange !== "function") continue;
        try {{ connection.onversionchange.call(connection, {{ target: connection, oldVersion, newVersion: nextVersion }}); }} catch (_error) {{}}
      }}
      const remaining = (indexedDbConnections.get(normalizedName) || []).filter(connection => !connection.__closed);
      if (remaining.length > 0) {{
        indexedDbConnections.set(normalizedName, remaining);
        if (!request.__blocked) {{
          request.__blocked = true;
          indexedDbPendingOpens.set(normalizedName, {{ request, requestedVersion }});
          indexedDbSchedule(() => {{
            if (typeof request.onblocked === "function") request.onblocked.call(request, {{ target: request, oldVersion, newVersion: nextVersion }});
          }});
        }}
        return;
      }}
      indexedDbConnections.delete(normalizedName);
    }}
    let nextDatabaseState = databaseState;
    if (!nextDatabaseState) {{
      if (Object.keys(indexedDbState().databases).length >= indexedDbDatabaseLimit) {{
        finishIndexedDbRequest(request, undefined, indexedDbError("QuotaExceededError", "native IndexedDB database limit exceeded"), null);
        return;
      }}
      nextDatabaseState = {{ version: nextVersion, stores: {{}} }};
      indexedDbState().databases[normalizedName] = nextDatabaseState;
    }} else if (upgraded) {{
      nextDatabaseState.version = nextVersion;
    }}
    const database = makeIndexedDbDatabase(normalizedName, nextDatabaseState, null);
    registerIndexedDbConnection(database);
    if (upgraded) {{
      const upgradeTransaction = makeIndexedDbTransaction(database, nextDatabaseState, Object.keys(nextDatabaseState.stores), "versionchange", true);
      database.__upgradeTransaction = upgradeTransaction;
      request.result = database;
      request.transaction = upgradeTransaction;
      try {{
        if (typeof request.onupgradeneeded === "function") request.onupgradeneeded.call(request, {{ target: request, oldVersion, newVersion: nextVersion }});
        finishIndexedDbRequest(request, database, null, null);
      }} catch (error) {{
        finishIndexedDbRequest(request, undefined, error instanceof Error ? error : indexedDbError("AbortError", String(error)), null);
      }}
    }} else {{
      finishIndexedDbRequest(request, database, null, null);
    }}
  }};
  const processIndexedDbDelete = (request, normalizedName) => {{
    const databaseState = indexedDbState().databases[normalizedName];
    if (!databaseState) {{
      finishIndexedDbRequest(request, undefined, null, null);
      return;
    }}
    const oldVersion = Number(databaseState.version);
    const connections = indexedDbConnections.get(normalizedName) || [];
    if (!request.__versionChangeNotified) {{
      request.__versionChangeNotified = true;
      for (const connection of connections) {{
        if (connection.__closed || typeof connection.onversionchange !== "function") continue;
        try {{ connection.onversionchange.call(connection, {{ target: connection, oldVersion, newVersion: null }}); }} catch (_error) {{}}
      }}
    }}
    const remaining = (indexedDbConnections.get(normalizedName) || []).filter(connection => !connection.__closed);
    if (remaining.length > 0) {{
      indexedDbConnections.set(normalizedName, remaining);
      if (!request.__blocked) {{
        request.__blocked = true;
        indexedDbPendingDeletes.set(normalizedName, request);
        indexedDbSchedule(() => {{
          if (typeof request.onblocked === "function") request.onblocked.call(request, {{ target: request, oldVersion, newVersion: null }});
        }});
      }}
      return;
    }}
    indexedDbConnections.delete(normalizedName);
    delete indexedDbState().databases[normalizedName];
    finishIndexedDbRequest(request, undefined, null, null);
  }};
  const nativeIndexedDB = {{
    __glassNativeIndexedDB: true,
    open(name, version) {{
      const normalizedName = indexedDbName(name, "database name");
      if (version !== undefined && (!Number.isSafeInteger(Number(version)) || Number(version) < 1)) throw indexedDbError("TypeError", "native IndexedDB version must be a positive integer");
      const requestedVersion = version === undefined ? undefined : Number(version);
      const request = makeIndexedDbRequest();
      indexedDbSchedule(() => processIndexedDbOpen(request, normalizedName, requestedVersion));
      return request;
    }},
    deleteDatabase(name) {{
      const normalizedName = indexedDbName(name, "database name");
      const request = makeIndexedDbRequest();
      indexedDbSchedule(() => processIndexedDbDelete(request, normalizedName));
      return request;
    }},
    databases() {{
      return Promise.resolve(Object.keys(indexedDbState().databases).map(name => ({{ name, version: indexedDbState().databases[name].version }})));
    }},
  }};
  const nativeIDBKeyRange = {{
    only(value) {{ const token = indexedDbKeyToken(value, false); return makeIndexedDbKeyRange(token, token, false, false); }},
    lowerBound(value, open) {{ return makeIndexedDbKeyRange(indexedDbKeyToken(value, false), undefined, Boolean(open), false); }},
    upperBound(value, open) {{ return makeIndexedDbKeyRange(undefined, indexedDbKeyToken(value, false), false, Boolean(open)); }},
    bound(lower, upper, lowerOpen, upperOpen) {{ return makeIndexedDbKeyRange(indexedDbKeyToken(lower, false), indexedDbKeyToken(upper, false), Boolean(lowerOpen), Boolean(upperOpen)); }},
  }};
  globalThis.indexedDB = nativeIndexedDB;
  globalThis.__glassNativeIndexedDB = nativeIndexedDB;
  globalThis.IDBKeyRange = nativeIDBKeyRange;
  globalThis.__glassNativeIndexedDBInstalled = true;
  }} else if (globalThis.indexedDB !== globalThis.__glassNativeIndexedDB) {{
    globalThis.indexedDB = globalThis.__glassNativeIndexedDB;
  }}
  globalThis.__glassTimers = timers;
  globalThis.__glassRunningTimers = runningTimers;
  globalThis.__glassNextTimerId = nextTimerId;
  globalThis.__glassAnimationFrames = animationFrames;
  globalThis.__glassNextAnimationFrameId = nextAnimationFrameId;
  globalThis.__glassIdleCallbacks = idleCallbacks;
  globalThis.__glassNextIdleCallbackId = nextIdleCallbackId;
  globalThis.setTimeout = setTimeoutNative;
  globalThis.setInterval = setIntervalNative;
  globalThis.clearTimeout = clearTimer;
  globalThis.clearInterval = clearTimer;
  globalThis.requestAnimationFrame = requestAnimationFrameNative;
  globalThis.cancelAnimationFrame = cancelAnimationFrameNative;
  globalThis.requestIdleCallback = requestIdleCallbackNative;
  globalThis.cancelIdleCallback = cancelIdleCallbackNative;
  const fetchRequests = globalThis.__glassFetchRequests instanceof Map
    ? globalThis.__glassFetchRequests
    : new Map();
  let nextFetchRequestId = Number.isSafeInteger(globalThis.__glassNextFetchRequestId)
    ? globalThis.__glassNextFetchRequestId
    : 1;
  const formDataEntries = (form) => {{
    if (!form || form.tagName !== "FORM") {{
      throw new TypeError("FormData constructor requires a form element");
    }}
    const entries = [];
    for (const control of elements) {{
      if (control.formOwnerIndex !== form.nodeIndex || control.disabled) continue;
      const name = String(control.getAttribute("name") || "");
      if (!name) continue;
      if (control.tagName === "SELECT") {{
        const options = elements.filter(option =>
          option.tagName === "OPTION"
            && option.parentIndex === control.nodeIndex
            && option.selected
            && !option.disabled
        );
        const selectedOptions = control.getAttribute("multiple") === null
          ? options.slice(0, 1)
          : options;
        for (const option of selectedOptions) {{
          const optionValue = option.getAttribute("value");
          entries.push([name, optionValue === null ? String(option.textContent || "") : String(optionValue)]);
        }}
        continue;
      }}
      const type = String(control.getAttribute("type") || "text").toLowerCase();
      if (["button", "reset", "submit", "image"].includes(type)) continue;
      if (type === "file") {{
        const selectedFiles = control.files && typeof control.files.length === "number"
          ? Array.from(control.files)
          : [];
        if (selectedFiles.length === 0) {{
          entries.push([name, formDataValue(new FileNative([], "", {{ type: "", lastModified: 0 }}), "")]);
        }} else {{
          for (const file of selectedFiles) entries.push([name, formDataValue(file)]);
        }}
        continue;
      }}
      if (["checkbox", "radio"].includes(type) && !control.checked) continue;
      if (control.tagName === "OPTION") continue;
      entries.push([name, String(control.value)]);
    }}
    return entries;
  }};
  const blobPartText = (part) => {{
    if (part && part.__glassNativeBlob === true) return part._text;
    if (typeof part === "string") return part;
    throw new TypeError("native Blob supports only text or Blob parts");
  }};
  const boundedBlobText = (parts) => {{
    if (parts === undefined || parts === null) return "";
    if (!Array.isArray(parts)) throw new TypeError("native Blob parts must be an array");
    const text = parts.map(blobPartText).join("");
    if (text.length > storageValueLimit) throw new RangeError("native Blob size limit exceeded");
    return text;
  }};
  const normalizeBlobType = (options) => {{
    const type = options && typeof options.type === "string"
      ? options.type.toLowerCase()
      : "";
    return /^[\x20-\x7e]*$/.test(type) ? type : "";
  }};
  const BlobNative = typeof globalThis.__glassBlobConstructor === "function"
    ? globalThis.__glassBlobConstructor
    : function(parts, options) {{
    this.__glassNativeBlob = true;
    const payload = boundedBlobParts(parts);
    this._text = payload.text;
    this._bytes = payload.bytes;
    this.size = Array.isArray(this._bytes) ? this._bytes.length : this._text.length;
    this.type = normalizeBlobType(options);
  }};
  const blobUtf8Bytes = (text) => {{
    const bytes = [];
    for (let index = 0; index < text.length; index += 1) {{
      let code = text.charCodeAt(index);
      if (code >= 0xd800 && code <= 0xdbff && index + 1 < text.length) {{
        const low = text.charCodeAt(index + 1);
        if (low >= 0xdc00 && low <= 0xdfff) {{
          code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
          index += 1;
        }}
      }} else if (code >= 0xd800 && code <= 0xdfff) {{
        code = 0xfffd;
      }}
      if (code <= 0x7f) bytes.push(code);
      else if (code <= 0x7ff) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
      else if (code <= 0xffff) bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
      else bytes.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 0x3f), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    }}
    if (bytes.length > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
    return bytes;
  }};
  const isBinaryBlobPart = (part) => (part && part.__glassNativeBlob === true && Array.isArray(part._bytes))
    || part instanceof ArrayBuffer
    || (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(part));
  const blobPartBytes = (part) => {{
    if (part && part.__glassNativeBlob === true) return Array.isArray(part._bytes)
      ? part._bytes.slice()
      : blobUtf8Bytes(part._text);
    if (part instanceof ArrayBuffer) {{
      if (part.byteLength > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
      return Array.from(new Uint8Array(part));
    }}
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(part)) {{
      if (part.byteLength > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
      return Array.from(new Uint8Array(part.buffer, part.byteOffset, part.byteLength));
    }}
    throw new TypeError("native Blob supports only text, buffers, or Blob parts");
  }};
  const boundedBlobParts = (parts) => {{
    if (parts === undefined || parts === null) return {{ text: "", bytes: null }};
    if (!Array.isArray(parts)) throw new TypeError("native Blob parts must be an array");
    let text = "";
    let bytes = null;
    for (const part of parts) {{
      if (bytes === null && !isBinaryBlobPart(part)) {{
        text += blobPartText(part);
        if (text.length > storageValueLimit) throw new RangeError("native Blob size limit exceeded");
        continue;
      }}
      if (bytes === null) bytes = blobUtf8Bytes(text);
      const partBytes = blobPartBytes(part);
      if (bytes.length + partBytes.length > storageValueLimit) throw new RangeError("native Blob binary size limit exceeded");
      for (const value of partBytes) bytes.push(value);
    }}
    return {{ text: bytes === null ? text : utf8TextFromBytes(bytes), bytes }};
  }};
  const base64Digit = (character) => {{
    const code = character.charCodeAt(0);
    if (code >= 65 && code <= 90) return code - 65;
    if (code >= 97 && code <= 122) return code - 97 + 26;
    if (code >= 48 && code <= 57) return code - 48 + 52;
    if (character === "+") return 62;
    if (character === "/") return 63;
    if (character === "=") return -2;
    return -1;
  }};
  const decodeBase64 = (encoded, maxBytes = storageValueLimit) => {{
    if (typeof encoded !== "string" || encoded.length % 4 !== 0) throw new TypeError("native response bytes are not valid base64");
    const bytes = [];
    for (let index = 0; index < encoded.length; index += 4) {{
      const first = base64Digit(encoded[index]);
      const second = base64Digit(encoded[index + 1]);
      const third = base64Digit(encoded[index + 2]);
      const fourth = base64Digit(encoded[index + 3]);
      if (first < 0 || second < 0 || third === -1 || fourth === -1
          || (third === -2 && fourth !== -2)
          || (third === -2 && index + 4 !== encoded.length)
          || (fourth === -2 && index + 4 !== encoded.length)) throw new TypeError("native response bytes are not valid base64");
      bytes.push((first << 2) | (second >> 4));
      if (third !== -2) bytes.push(((second & 15) << 4) | (third >> 2));
      if (fourth !== -2) bytes.push(((third & 3) << 6) | fourth);
    }}
    if (bytes.length > maxBytes) throw new RangeError("native binary payload exceeds its limit");
    return bytes;
  }};
  const encodeBase64 = (bytes, maxBytes = storageValueLimit) => {{
    if (!Array.isArray(bytes)) throw new TypeError("native Blob bytes are invalid");
    if (bytes.length > maxBytes) throw new RangeError("native binary payload exceeds its limit");
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let encoded = "";
    for (let index = 0; index < bytes.length; index += 3) {{
      const first = bytes[index];
      const second = index + 1 < bytes.length ? bytes[index + 1] : 0;
      const third = index + 2 < bytes.length ? bytes[index + 2] : 0;
      for (const value of [first, second, third]) {{
        if (!Number.isInteger(value) || value < 0 || value > 255) throw new TypeError("native Blob bytes are invalid");
      }}
      encoded += alphabet[first >> 2];
      encoded += alphabet[((first & 3) << 4) | (second >> 4)];
      encoded += index + 1 < bytes.length ? alphabet[((second & 15) << 2) | (third >> 6)] : "=";
      encoded += index + 2 < bytes.length ? alphabet[third & 63] : "=";
    }}
    return encoded;
  }};
  const utf8TextFromBytes = (bytes) => {{
    const continuation = value => value >= 0x80 && value <= 0xbf;
    let text = "";
    for (let index = 0; index < bytes.length;) {{
      const first = bytes[index];
      let codePoint = -1;
      let width = 1;
      if (first <= 0x7f) {{
        codePoint = first;
      }} else if (first >= 0xc2 && first <= 0xdf && continuation(bytes[index + 1])) {{
        codePoint = ((first & 0x1f) << 6) | (bytes[index + 1] & 0x3f);
        width = 2;
      }} else if (first === 0xe0 && bytes[index + 1] >= 0xa0 && bytes[index + 1] <= 0xbf && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first >= 0xe1 && first <= 0xec && continuation(bytes[index + 1]) && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first === 0xed && bytes[index + 1] >= 0x80 && bytes[index + 1] <= 0x9f && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first >= 0xee && first <= 0xef && continuation(bytes[index + 1]) && continuation(bytes[index + 2])) {{
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      }} else if (first === 0xf0 && bytes[index + 1] >= 0x90 && bytes[index + 1] <= 0xbf && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {{
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      }} else if (first >= 0xf1 && first <= 0xf3 && continuation(bytes[index + 1]) && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {{
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      }} else if (first === 0xf4 && bytes[index + 1] >= 0x80 && bytes[index + 1] <= 0x8f && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {{
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      }}
      if (codePoint < 0) {{
        text += "\ufffd";
        index += 1;
      }} else {{
        text += String.fromCodePoint(codePoint);
        index += width;
      }}
    }}
    return text;
  }};
  const blobBytes = (blob) => Array.isArray(blob._bytes)
    ? blob._bytes.slice()
    : blobUtf8Bytes(blob._text);
  BlobNative.prototype.text = function() {{
    return Promise.resolve(Array.isArray(this._bytes) ? utf8TextFromBytes(this._bytes) : this._text);
  }};
  BlobNative.prototype.arrayBuffer = function() {{
    return Promise.resolve(new Uint8Array(blobBytes(this)).buffer);
  }};
  BlobNative.prototype.bytes = function() {{
    return Promise.resolve(new Uint8Array(blobBytes(this)));
  }};
  BlobNative.prototype.slice = function(start, end, contentType) {{
    const length = Array.isArray(this._bytes) ? this._bytes.length : this._text.length;
    const normalizePosition = (value, fallback) => {{
      if (value === undefined) return fallback;
      const number = Number(value);
      if (!Number.isFinite(number)) return fallback;
      return number < 0 ? Math.max(length + Math.trunc(number), 0) : Math.min(Math.trunc(number), length);
    }};
    const begin = normalizePosition(start, 0);
    const finish = normalizePosition(end, length);
    if (Array.isArray(this._bytes)) {{
      const blob = new BlobNative([], {{ type: contentType === undefined ? this.type : contentType }});
      blob._bytes = this._bytes.slice(begin, finish);
      blob._text = utf8TextFromBytes(blob._bytes);
      blob.size = blob._bytes.length;
      return blob;
    }}
    return new BlobNative([begin > finish ? "" : this._text.slice(begin, finish)], {{ type: contentType }});
  }};
  const FileNative = typeof globalThis.__glassFileConstructor === "function"
    ? globalThis.__glassFileConstructor
    : function(parts, name, options) {{
    if (name === undefined) throw new TypeError("native File requires a name");
    BlobNative.call(this, parts, options);
    this.__glassNativeFile = true;
    this.name = String(name);
    const modified = options && Number.isFinite(Number(options.lastModified))
      ? Number(options.lastModified)
      : 0;
    this.lastModified = Math.max(0, modified);
  }};
  if (typeof globalThis.__glassFileConstructor !== "function") {{
    FileNative.prototype = Object.create(BlobNative.prototype);
    FileNative.prototype.constructor = FileNative;
  }}
  globalThis.__glassBlobConstructor = BlobNative;
  globalThis.__glassFileConstructor = FileNative;
  globalThis.Blob = BlobNative;
  globalThis.File = FileNative;
  const FileListNative = function(entries) {{
    const files = Array.isArray(entries) ? entries : [];
    Object.defineProperty(this, "_files", {{ enumerable: false, configurable: false, value: files.slice() }});
    for (let index = 0; index < this._files.length; index += 1) {{
      Object.defineProperty(this, String(index), {{ enumerable: true, configurable: false, value: this._files[index] }});
    }}
  }};
  FileListNative.prototype.constructor = FileListNative;
  Object.defineProperty(FileListNative.prototype, "length", {{
    configurable: false,
    get() {{ return this._files.length; }},
  }});
  FileListNative.prototype.item = function(index) {{
    const numeric = Number(index);
    return Number.isInteger(numeric) && numeric >= 0 && numeric < this._files.length
      ? this._files[numeric]
      : null;
  }};
  FileListNative.prototype[Symbol.iterator] = function() {{ return this._files[Symbol.iterator](); }};
  globalThis.FileList = FileListNative;
  const nativeFileByteLimit = {native_file_bytes};
  const nativeFormBodyLimit = {native_form_body_bytes};
  const nativeFileFromEntry = (entry) => {{
    const bytes = decodeBase64(String(entry && entry.bytes || ""), nativeFileByteLimit);
    const file = new globalThis.File([], String(entry && entry.name || ""), {{
      type: String(entry && entry.type || ""),
      lastModified: Number(entry && entry.lastModified) || 0,
    }});
    file._bytes = bytes;
    file._text = utf8TextFromBytes(bytes);
    file.size = bytes.length;
    return file;
  }};
  const makeNativeFileList = (entries) => new globalThis.FileList(
    (Array.isArray(entries) ? entries : []).map(nativeFileFromEntry),
  );
  const formDataValue = (value, filename) => {{
    if (value && value.__glassNativeBlob === true) {{
      const defaultFilename = value.__glassNativeFile === true ? value.name : "blob";
      return {{
        kind: "file",
        value,
        filename: filename === undefined ? defaultFilename : String(filename),
      }};
    }}
    return String(value);
  }};
  const FormDataNative = function(form) {{
    this.__glassFormData = true;
    this._entries = [];
    if (form !== undefined && form !== null) this._entries = formDataEntries(form);
  }};
  const formDataEntryValue = entry => entry[1].kind === "file" ? entry[1].value : entry[1];
  const formDataIterator = (owner, kind) => {{
    let index = 0;
    const iterator = {{
      next() {{
        if (index >= owner._entries.length) return {{ value: undefined, done: true }};
        const entry = owner._entries[index++];
        if (kind === "keys") return {{ value: entry[0], done: false }};
        if (kind === "values") return {{ value: formDataEntryValue(entry), done: false }};
        return {{ value: [entry[0], formDataEntryValue(entry)], done: false }};
      }},
      [Symbol.iterator]() {{ return this; }},
    }};
    return iterator;
  }};
  FormDataNative.prototype.append = function(name, value, filename) {{
    this._entries.push([String(name), formDataValue(value, filename)]);
  }};
  FormDataNative.prototype.set = function(name, value, filename) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
    this._entries.push([key, formDataValue(value, filename)]);
  }};
  FormDataNative.prototype.delete = function(name) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
  }};
  FormDataNative.prototype.get = function(name) {{
    const key = String(name);
    const entry = this._entries.find(candidate => candidate[0] === key);
    return entry ? formDataEntryValue(entry) : null;
  }};
  FormDataNative.prototype.getAll = function(name) {{
    const key = String(name);
    return this._entries
      .filter(entry => entry[0] === key)
      .map(formDataEntryValue);
  }};
  FormDataNative.prototype.has = function(name) {{
    const key = String(name);
    return this._entries.some(entry => entry[0] === key);
  }};
  FormDataNative.prototype.entries = function() {{
    return formDataIterator(this, "entries");
  }};
  FormDataNative.prototype.keys = function() {{
    return formDataIterator(this, "keys");
  }};
  FormDataNative.prototype.values = function() {{
    return formDataIterator(this, "values");
  }};
  FormDataNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("FormData callback must be callable");
    this._entries.forEach(entry => callback.call(
      thisArg,
      formDataEntryValue(entry),
      entry[0],
      this,
    ));
  }};
  FormDataNative.prototype[Symbol.iterator] = FormDataNative.prototype.entries;
  const escapeFormDataName = value => String(value)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, "\\\"")
    .replace(/\r/g, "%0D")
    .replace(/\n/g, "%0A");
  const serializeFormData = (formData, requestId) => {{
    const boundary = "----GlassNativeForm" + requestId;
    const hasBinaryFile = formData._entries.some(entry =>
      entry[1].kind === "file" && Array.isArray(entry[1].value._bytes)
    );
    if (hasBinaryFile) {{
      const bytes = [];
      const appendBytes = source => {{
        if (bytes.length + source.length > nativeFormBodyLimit) throw new RangeError("native FormData body limit exceeded");
        for (const value of source) bytes.push(value);
      }};
      const appendText = value => appendBytes(blobUtf8Bytes(String(value)));
      for (const entry of formData._entries) {{
        appendText("--" + boundary + "\r\n");
        const file = entry[1].kind === "file" ? entry[1] : null;
        appendText("Content-Disposition: form-data; name=\"" + escapeFormDataName(entry[0]) + "\"");
        if (file) appendText("; filename=\"" + escapeFormDataName(file.filename) + "\"");
        appendText("\r\n");
        if (file) appendText("Content-Type: " + (file.value.type || "application/octet-stream") + "\r\n");
        appendText("\r\n");
        appendBytes(file ? blobBytes(file.value) : blobUtf8Bytes(entry[1]));
        appendText("\r\n");
      }}
      appendText("--" + boundary + "--\r\n");
      return {{
        body: utf8TextFromBytes(bytes),
        bodyBase64: encodeBase64(bytes, nativeFormBodyLimit),
        contentType: "multipart/form-data; boundary=" + boundary,
      }};
    }}
    let body = "";
    for (const entry of formData._entries) {{
      body += "--" + boundary + "\r\n";
      const file = entry[1].kind === "file" ? entry[1] : null;
      body += "Content-Disposition: form-data; name=\"" + escapeFormDataName(entry[0]) + "\"";
      if (file) body += "; filename=\"" + escapeFormDataName(file.filename) + "\"";
      body += "\r\n";
      if (file) body += "Content-Type: " + (file.value.type || "application/octet-stream") + "\r\n";
      body += "\r\n";
      body += file ? file.value._text : entry[1];
      body += "\r\n";
    }}
    body += "--" + boundary + "--\r\n";
    return {{ body, bodyBase64: null, contentType: "multipart/form-data; boundary=" + boundary }};
  }};
  globalThis.FormData = FormDataNative;
  const appendUrlSearchParam = (target, name, value) => {{
    const key = String(name);
    const normalizedValue = String(value);
    if (key.length > storageKeyLimit) throw new RangeError("native URLSearchParams name exceeds its limit");
    if (normalizedValue.length > storageValueLimit) throw new RangeError("native URLSearchParams value exceeds its limit");
    if (target._entries.length >= storageEntryLimit) throw new RangeError("native URLSearchParams entry limit exceeded");
    target._entries.push([key, normalizedValue]);
  }};
  const URLSearchParamsNative = function(init) {{
    this.__glassUrlSearchParams = true;
    this._entries = [];
    if (init === undefined || init === null) return;
    if (typeof init === "string") {{
      const source = init.startsWith("?") ? init.slice(1) : init;
      for (const part of source.split("&")) {{
        if (!part) continue;
        const separator = part.indexOf("=");
        const decode = value => decodeURIComponent(String(value).replace(/\+/g, " "));
        const name = separator < 0 ? part : part.slice(0, separator);
        const value = separator < 0 ? "" : part.slice(separator + 1);
        appendUrlSearchParam(this, decode(name), decode(value));
      }}
      return;
    }}
    if (init.__glassUrlSearchParams === true) {{
      init._entries.forEach(entry => appendUrlSearchParam(this, entry[0], entry[1]));
      return;
    }}
    if (Array.isArray(init)) {{
      for (const pair of init) {{
        if (!Array.isArray(pair) || pair.length !== 2) throw new TypeError("native URLSearchParams pairs must contain two values");
        appendUrlSearchParam(this, pair[0], pair[1]);
      }}
      return;
    }}
    if (typeof init[Symbol.iterator] === "function") {{
      for (const pair of init) {{
        if (!pair || typeof pair[Symbol.iterator] !== "function") throw new TypeError("native URLSearchParams iterable pairs must be iterable");
        const values = Array.from(pair);
        if (values.length !== 2) throw new TypeError("native URLSearchParams iterable pairs must contain two values");
        appendUrlSearchParam(this, values[0], values[1]);
      }}
      return;
    }}
    if (typeof init === "object") {{
      for (const name of Object.keys(init)) appendUrlSearchParam(this, name, init[name]);
      return;
    }}
    throw new TypeError("native URLSearchParams accepts text, pairs, records, or URLSearchParams");
  }};
  URLSearchParamsNative.prototype.append = function(name, value) {{
    appendUrlSearchParam(this, name, value);
  }};
  URLSearchParamsNative.prototype.set = function(name, value) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
    appendUrlSearchParam(this, key, value);
  }};
  URLSearchParamsNative.prototype.delete = function(name, value) {{
    const key = String(name);
    if (arguments.length > 1) {{
      const normalizedValue = String(value);
      this._entries = this._entries.filter(entry => entry[0] !== key || entry[1] !== normalizedValue);
    }} else {{
      this._entries = this._entries.filter(entry => entry[0] !== key);
    }}
  }};
  URLSearchParamsNative.prototype.get = function(name) {{
    const key = String(name);
    const entry = this._entries.find(candidate => candidate[0] === key);
    return entry ? entry[1] : null;
  }};
  URLSearchParamsNative.prototype.getAll = function(name) {{
    const key = String(name);
    return this._entries.filter(entry => entry[0] === key).map(entry => entry[1]);
  }};
  URLSearchParamsNative.prototype.has = function(name) {{
    const key = String(name);
    return this._entries.some(entry => entry[0] === key);
  }};
  Object.defineProperty(URLSearchParamsNative.prototype, "size", {{
    get() {{ return this._entries.length; }},
  }});
  const urlSearchParamsIterator = (owner, kind) => {{
    let index = 0;
    const iterator = {{
      next() {{
        if (index >= owner._entries.length) return {{ value: undefined, done: true }};
        const entry = owner._entries[index++];
        if (kind === "keys") return {{ value: entry[0], done: false }};
        if (kind === "values") return {{ value: entry[1], done: false }};
        return {{ value: [entry[0], entry[1]], done: false }};
      }},
      [Symbol.iterator]() {{ return this; }},
    }};
    return iterator;
  }};
  URLSearchParamsNative.prototype.entries = function() {{
    return urlSearchParamsIterator(this, "entries");
  }};
  URLSearchParamsNative.prototype.keys = function() {{
    return urlSearchParamsIterator(this, "keys");
  }};
  URLSearchParamsNative.prototype.values = function() {{
    return urlSearchParamsIterator(this, "values");
  }};
  URLSearchParamsNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("native URLSearchParams callback must be callable");
    this._entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], this));
  }};
  URLSearchParamsNative.prototype.sort = function() {{
    for (let index = 1; index < this._entries.length; index += 1) {{
      const current = this._entries[index];
      let position = index;
      while (position > 0 && this._entries[position - 1][0] > current[0]) {{
        this._entries[position] = this._entries[position - 1];
        position -= 1;
      }}
      this._entries[position] = current;
    }}
  }};
  URLSearchParamsNative.prototype[Symbol.iterator] = URLSearchParamsNative.prototype.entries;
  URLSearchParamsNative.prototype.toString = function() {{
    const encode = value => encodeURIComponent(String(value))
      .replace(/[!'()~]/g, character => "%" + character.charCodeAt(0).toString(16).toUpperCase())
      .replace(/%20/g, "+");
    return this._entries.map(entry => encode(entry[0]) + "=" + encode(entry[1])).join("&");
  }};
  globalThis.URLSearchParams = URLSearchParamsNative;
  const nativeUrlScheme = /^[A-Za-z][A-Za-z0-9+.-]*:/;
  const nativeUrlNormalizePath = (path) => {{
    const source = String(path || "/");
    const trailing = source.endsWith("/");
    const segments = [];
    for (const segment of source.split("/")) {{
      if (!segment || segment === ".") continue;
      if (segment === "..") {{
        if (segments.length > 0) segments.pop();
        continue;
      }}
      segments.push(segment);
    }}
    let normalized = "/" + segments.join("/");
    if (trailing && normalized !== "/") normalized += "/";
    return normalized;
  }};
  const nativeUrlParts = (input) => {{
    const source = String(input);
    const hashIndex = source.indexOf("#");
    const hash = hashIndex < 0 ? "" : source.slice(hashIndex);
    const withoutHash = hashIndex < 0 ? source : source.slice(0, hashIndex);
    const queryIndex = withoutHash.indexOf("?");
    const search = queryIndex < 0 ? "" : withoutHash.slice(queryIndex);
    const main = queryIndex < 0 ? withoutHash : withoutHash.slice(0, queryIndex);
    const match = main.match(/^([A-Za-z][A-Za-z0-9+.-]*:)(.*)$/);
    if (!match) throw new TypeError("native URL requires an absolute or resolvable URL");
    const protocol = match[1].toLowerCase();
    let rest = match[2];
    let authority = "";
    let pathname = "";
    if (rest.startsWith("//")) {{
      rest = rest.slice(2);
      const slash = rest.indexOf("/");
      if (slash < 0) {{
        authority = rest;
        pathname = protocol === "http:" || protocol === "https:" ? "/" : "";
      }} else {{
        authority = rest.slice(0, slash);
        pathname = rest.slice(slash) || "/";
      }}
    }} else pathname = rest || ((protocol === "http:" || protocol === "https:") ? "/" : "");
    const at = authority.lastIndexOf("@");
    const userInfo = at < 0 ? "" : authority.slice(0, at);
    const credentialSeparator = userInfo.indexOf(":");
    const username = credentialSeparator < 0 ? userInfo : userInfo.slice(0, credentialSeparator);
    const password = credentialSeparator < 0 ? "" : userInfo.slice(credentialSeparator + 1);
    const hostPort = at < 0 ? authority : authority.slice(at + 1);
    let hostname = hostPort;
    let port = "";
    if (hostname.startsWith("[")) {{
      const closing = hostname.indexOf("]");
      if (closing >= 0) {{
        const suffix = hostname.slice(closing + 1);
        hostname = hostname.slice(0, closing + 1);
        if (suffix.startsWith(":")) port = suffix.slice(1);
      }}
    }} else {{
      const colon = hostname.lastIndexOf(":");
      if (colon >= 0 && /^[0-9]*$/.test(hostname.slice(colon + 1))) {{
        port = hostname.slice(colon + 1);
        hostname = hostname.slice(0, colon);
      }}
    }}
    hostname = hostname.toLowerCase();
    const origin = protocol === "http:" || protocol === "https:"
      ? protocol + "//" + hostPort.toLowerCase()
      : "null";
    const prefix = authority ? protocol + "//" + authority : protocol;
    const href = prefix + pathname + search + hash;
    return {{
      href,
      protocol,
      origin,
      authority,
      username,
      password,
      host: hostPort,
      hostname,
      port,
      pathname,
      search,
      hash,
    }};
  }};
  const nativeUrlResolve = (input, base) => {{
    const value = String(input);
    if (nativeUrlScheme.test(value)) return value;
    if (base === undefined || base === null) throw new TypeError("native relative URL requires a base");
    const baseHref = base && base.__glassUrl === true ? base.href : String(base);
    const baseParts = nativeUrlParts(baseHref);
    if (value.startsWith("//")) return baseParts.protocol + value;
    const hashIndex = value.indexOf("#");
    const hash = hashIndex < 0 ? "" : value.slice(hashIndex);
    const withoutHash = hashIndex < 0 ? value : value.slice(0, hashIndex);
    const queryIndex = withoutHash.indexOf("?");
    const search = queryIndex < 0 ? "" : withoutHash.slice(queryIndex);
    const path = queryIndex < 0 ? withoutHash : withoutHash.slice(0, queryIndex);
    const baseWithoutHash = baseParts.href.slice(0, baseParts.href.indexOf("#") < 0 ? baseParts.href.length : baseParts.href.indexOf("#"));
    if (!path && !search && hash) return baseWithoutHash + hash;
    if (!path && search) {{
      const baseQuery = baseWithoutHash.indexOf("?");
      return (baseQuery < 0 ? baseWithoutHash : baseWithoutHash.slice(0, baseQuery)) + search + hash;
    }}
    if (!path && !search && !hash) return baseWithoutHash;
    const baseDirectory = baseParts.pathname.slice(0, baseParts.pathname.lastIndexOf("/") + 1);
    const resolvedPath = path.startsWith("/") ? path : baseDirectory + path;
    return baseParts.protocol + "//" + baseParts.authority + nativeUrlNormalizePath(resolvedPath) + search + hash;
  }};
  const URLNative = function(input, base) {{
    const parts = nativeUrlParts(nativeUrlResolve(input && input.__glassUrl === true ? input.href : input, base));
    const state = {{
      hasAuthority: Boolean(parts.authority),
      prefix: parts.authority ? parts.protocol + "//" + parts.authority : parts.protocol,
      origin: parts.origin,
      protocol: parts.protocol,
      username: parts.username,
      password: parts.password,
      host: parts.host,
      hostname: parts.hostname,
      port: parts.port,
      pathname: parts.pathname,
      search: parts.search,
      hash: parts.hash,
    }};
    const searchParams = new URLSearchParamsNative(parts.search);
    const updateParts = (next) => {{
      state.hasAuthority = Boolean(next.authority);
      state.prefix = next.authority ? next.protocol + "//" + next.authority : next.protocol;
      state.origin = next.origin;
      state.protocol = next.protocol;
      state.username = next.username;
      state.password = next.password;
      state.host = next.host;
      state.hostname = next.hostname;
      state.port = next.port;
      state.pathname = next.pathname;
      state.search = next.search;
      state.hash = next.hash;
      searchParams._entries = new URLSearchParamsNative(next.search)._entries;
    }};
    const credentialsFor = (username, password) => username || password
      ? username + (password ? ":" + password : "") + "@"
      : "";
    const currentCredentials = () => credentialsFor(state.username, state.password);
    const currentAuthority = () => currentCredentials() + state.host;
    const requireAuthority = () => {{
      if (!state.hasAuthority) throw new TypeError("native URL authority mutation requires a host");
    }};
    const replaceAuthority = (authority, protocol = state.protocol) => {{
      requireAuthority();
      const next = nativeUrlParts(protocol + "//" + authority + state.pathname + state.search + state.hash);
      updateParts(next);
    }};
    const validateHost = value => {{
      const source = String(value);
      if (!source || /[\/\?#@\s]/.test(source)) throw new TypeError("native URL host is invalid");
      if (!source.startsWith("[") && source.includes(":") && !/:[0-9]+$/.test(source))
        throw new TypeError("native URL host port is invalid");
      const parsed = nativeUrlParts(state.protocol + "//" + source + "/");
      if (!parsed.host || (parsed.port && Number(parsed.port) > 65535))
        throw new TypeError("native URL host is invalid");
      return parsed.host;
    }};
    const normalizeCredential = value => encodeURIComponent(String(value));
    const currentHref = () => state.prefix + state.pathname + state.search + state.hash;
    const syncSearch = () => {{
      const encoded = searchParams.toString();
      state.search = encoded ? "?" + encoded : "";
    }};
    for (const method of ["append", "set", "delete", "sort"]) {{
      const original = searchParams[method];
      searchParams[method] = function(...args) {{
        const result = original.apply(searchParams, args);
        syncSearch();
        return result;
      }};
    }}
    Object.defineProperty(this, "__glassUrl", {{ value: true }});
    const define = (name, getter) => Object.defineProperty(this, name, {{ enumerable: true, get: getter }});
    Object.defineProperty(this, "href", {{
      enumerable: true,
      get: currentHref,
      set: value => updateParts(nativeUrlParts(nativeUrlResolve(value, currentHref()))),
    }});
    define("origin", () => state.origin);
    Object.defineProperty(this, "protocol", {{
      enumerable: true,
      get: () => state.protocol,
      set: value => {{
        const source = String(value).toLowerCase();
        const protocol = source.endsWith(":") ? source : source + ":";
        if (!["http:", "https:"].includes(protocol)) throw new TypeError("native URL protocol is unsupported");
        if (protocol !== state.protocol) replaceAuthority(currentAuthority(), protocol);
      }},
    }});
    Object.defineProperty(this, "username", {{
      enumerable: true,
      get: () => state.username,
      set: value => replaceAuthority(credentialsFor(normalizeCredential(value), state.password) + state.host),
    }});
    Object.defineProperty(this, "password", {{
      enumerable: true,
      get: () => state.password,
      set: value => replaceAuthority(credentialsFor(state.username, normalizeCredential(value)) + state.host),
    }});
    Object.defineProperty(this, "host", {{
      enumerable: true,
      get: () => state.host,
      set: value => replaceAuthority(currentCredentials() + validateHost(value)),
    }});
    Object.defineProperty(this, "hostname", {{
      enumerable: true,
      get: () => state.hostname,
      set: value => {{
        const source = String(value);
        if (!source || /[\/\?#@\s]/.test(source) || (!source.startsWith("[") && source.includes(":")))
          throw new TypeError("native URL hostname is invalid");
        if (source.startsWith("[") && !/^\[[^\]]+\]$/.test(source))
          throw new TypeError("native URL hostname is invalid");
        const host = source + (state.port ? ":" + state.port : "");
        replaceAuthority(currentCredentials() + validateHost(host));
      }},
    }});
    Object.defineProperty(this, "port", {{
      enumerable: true,
      get: () => state.port,
      set: value => {{
        const source = String(value);
        if (source && (!/^[0-9]+$/.test(source) || Number(source) > 65535))
          throw new TypeError("native URL port is invalid");
        const host = state.hostname + (source ? ":" + source : "");
        replaceAuthority(currentCredentials() + validateHost(host));
      }},
    }});
    Object.defineProperty(this, "pathname", {{
      enumerable: true,
      get: () => state.pathname,
      set: value => {{ state.pathname = nativeUrlNormalizePath(String(value)); }},
    }});
    Object.defineProperty(this, "search", {{
      enumerable: true,
      get: () => state.search,
      set: value => {{
        const source = String(value);
        searchParams._entries = new URLSearchParamsNative(source)._entries;
        syncSearch();
      }},
    }});
    Object.defineProperty(this, "hash", {{
      enumerable: true,
      get: () => state.hash,
      set: value => {{
        const source = String(value);
        state.hash = !source ? "" : source.startsWith("#") ? source : "#" + source;
      }},
    }});
    define("searchParams", () => searchParams);
    Object.freeze(this);
  }};
  URLNative.prototype.toString = function() {{ return this.href; }};
  URLNative.prototype.toJSON = function() {{ return this.href; }};
  globalThis.URL = URLNative;
  const requestHeaderNameNative = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const forbiddenRequestHeaderNative = (name) => [
    "accept-charset", "accept-encoding", "access-control-request-headers",
    "access-control-request-method", "connection", "content-length",
    "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
    "origin", "referer", "te", "trailer", "transfer-encoding", "upgrade",
    "user-agent", "via",
  ].includes(name) || name.startsWith("proxy-") || name.startsWith("sec-");
  const validateNativeRequestHeader = (name, value) => {{
    const normalizedName = String(name).toLowerCase();
    if (!requestHeaderNameNative.test(String(name)) || normalizedName.length > {fetch_header_name_limit}) throw new TypeError("native Headers name is invalid");
    if (forbiddenRequestHeaderNative(normalizedName)) throw new TypeError("native Headers name is forbidden");
    const normalizedValue = String(value);
    if (normalizedValue.length > {fetch_header_value_limit} || /[\u0000-\u001f\u007f]/.test(normalizedValue)) throw new TypeError("native Headers value is invalid");
    return [normalizedName, normalizedValue];
  }};
  const validateNativeHeaderEntries = (entries) => {{
    if (entries.length > {fetch_header_count_limit}) throw new RangeError("native Headers limit exceeded");
    let totalBytes = 0;
    for (const entry of entries) {{
      totalBytes += entry[0].length + entry[1].length;
      if (totalBytes > {fetch_header_bytes_limit}) throw new RangeError("native Headers exceed their limit");
    }}
  }};
  const HeadersNative = function(init) {{
    this.__glassHeaders = true;
    this._entries = [];
    if (init === undefined || init === null) return;
    if (init.__glassHeaders === true) {{
      init._entries.forEach(entry => this.append(entry[0], entry[1]));
      return;
    }}
    if (Array.isArray(init)) {{
      for (const entry of init) {{
        if (!Array.isArray(entry) || entry.length !== 2) throw new TypeError("native Headers pairs must contain two values");
        this.append(entry[0], entry[1]);
      }}
      return;
    }}
    if (typeof init !== "object") throw new TypeError("native Headers accepts records, pairs, or Headers");
    for (const name of Object.keys(init)) this.append(name, init[name]);
  }};
  HeadersNative.prototype.append = function(name, value) {{
    const normalized = validateNativeRequestHeader(name, value);
    const existing = this._entries.find(entry => entry[0] === normalized[0]);
    const nextValue = existing ? existing[1] + ", " + normalized[1] : normalized[1];
    const nextEntries = existing
      ? this._entries.map(entry => entry[0] === normalized[0] ? [entry[0], nextValue] : entry)
      : this._entries.concat([[normalized[0], normalized[1]]]);
    validateNativeHeaderEntries(nextEntries);
    this._entries = nextEntries;
  }};
  HeadersNative.prototype.set = function(name, value) {{
    const normalized = validateNativeRequestHeader(name, value);
    const nextEntries = this._entries.filter(entry => entry[0] !== normalized[0]);
    nextEntries.push(normalized);
    validateNativeHeaderEntries(nextEntries);
    this._entries = nextEntries;
  }};
  HeadersNative.prototype.delete = function(name) {{
    const normalizedName = validateNativeRequestHeader(name, "")[0];
    this._entries = this._entries.filter(entry => entry[0] !== normalizedName);
  }};
  HeadersNative.prototype.get = function(name) {{
    const normalizedName = validateNativeRequestHeader(name, "")[0];
    const entry = this._entries.find(candidate => candidate[0] === normalizedName);
    return entry ? entry[1] : null;
  }};
  HeadersNative.prototype.has = function(name) {{
    return this.get(name) !== null;
  }};
  const nativeHeaderIterator = (owner, kind) => {{
    let index = 0;
    const iterator = {{
      next() {{
        if (index >= owner._entries.length) return {{ value: undefined, done: true }};
        const entry = owner._entries[index++];
        if (kind === "keys") return {{ value: entry[0], done: false }};
        if (kind === "values") return {{ value: entry[1], done: false }};
        return {{ value: [entry[0], entry[1]], done: false }};
      }},
      [Symbol.iterator]() {{ return this; }},
    }};
    return iterator;
  }};
  HeadersNative.prototype.entries = function() {{ return nativeHeaderIterator(this, "entries"); }};
  HeadersNative.prototype.keys = function() {{ return nativeHeaderIterator(this, "keys"); }};
  HeadersNative.prototype.values = function() {{ return nativeHeaderIterator(this, "values"); }};
  HeadersNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("native Headers callback must be callable");
    this._entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], this));
  }};
  Object.defineProperty(HeadersNative.prototype, "size", {{
    get() {{ return this._entries.length; }},
  }});
  HeadersNative.prototype[Symbol.iterator] = HeadersNative.prototype.entries;
  globalThis.Headers = HeadersNative;
  const nativeRequestMethods = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
  const nativeBodylessMethods = ["GET", "HEAD"];
  const RequestNative = function(input, init) {{
    const source = input && input.__glassRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    const overrides = init && typeof init === "object" ? init : {{}};
    const hasBodyOverride = Object.prototype.hasOwnProperty.call(overrides, "body");
    if (source && !hasBodyOverride && (source.bodyUsed || (source.body && source.body.locked)))
      throw new TypeError("native Request body is unusable");
    const href = source ? source.url : sourceUrl ? sourceUrl.href : input;
    if (typeof href !== "string") throw new TypeError("native Request URL must be a string");
    const settings = Object.assign({{}}, source ? source._settings : {{}}, overrides);
    const inheritedPayload = source && !Object.prototype.hasOwnProperty.call(overrides, "body")
      ? source.__glassRequestBodyPayload
      : null;
    let payload;
    if (inheritedPayload && inheritedPayload.stream) {{
      const [sourceBody, cloneBody] = source.body.tee();
      source.__glassRequestBodyPayload.stream = sourceBody;
      source.__glassRequestBodyState.stream = sourceBody;
      payload = {{ bodyNull: false, bytes: null, stream: cloneBody, contentType: inheritedPayload.contentType }};
    }} else if (inheritedPayload) {{
      payload = {{
        bodyNull: inheritedPayload.bodyNull,
        bytes: inheritedPayload.bytes.slice(),
        stream: null,
        contentType: inheritedPayload.contentType,
      }};
    }} else {{
      payload = nativeRequestBodyPayload(settings);
    }}
    const method = settings.method === undefined ? "GET" : String(settings.method).toUpperCase();
    if (!nativeRequestMethods.includes(method)) throw new TypeError("native Request method is unsupported");
    if (nativeBodylessMethods.includes(method) && settings.body !== undefined && settings.body !== null)
      throw new TypeError("native " + method + " Requests must not have a body");
    const mode = settings.mode === undefined ? "cors" : String(settings.mode).toLowerCase();
    if (!["cors", "no-cors", "same-origin"].includes(mode)) throw new TypeError("native Request mode is unsupported");
    const redirect = settings.redirect === undefined ? "follow" : String(settings.redirect).toLowerCase();
    if (!["follow", "error", "manual"].includes(redirect)) throw new TypeError("native Request redirect mode is unsupported");
    const headers = new HeadersNative(settings.headers);
    settings.method = method;
    settings.mode = mode;
    settings.redirect = redirect;
    settings.headers = headers;
    Object.defineProperty(this, "__glassRequest", {{ value: true }});
    Object.defineProperty(this, "_settings", {{ value: settings }});
    Object.defineProperty(this, "__glassRequestBodyPayload", {{ value: payload }});
    Object.defineProperty(this, "__glassRequestBodyState", {{ value: {{ used: false, stream: null }} }});
    this.method = method;
    this.url = href;
    this.headers = headers;
    this.mode = mode;
    this.redirect = redirect;
    this.credentials = settings.credentials === undefined ? "same-origin" : String(settings.credentials);
    this.signal = settings.signal === undefined ? null : settings.signal;
    const requestBodyState = this.__glassRequestBodyState;
    requestBodyState.stream = payload.bodyNull
      ? null
      : payload.stream || new ReadableStreamNative(payload.bytes, null, () => {{ requestBodyState.used = true; }});
    Object.defineProperty(this, "body", {{
      configurable: true,
      enumerable: true,
      get() {{ return requestBodyState.stream; }},
    }});
    Object.defineProperty(this, "bodyUsed", {{
      configurable: true,
      get() {{ return nativeRequestBodyIsUsed(this); }},
    }});
    Object.freeze(this);
  }};
  RequestNative.prototype.clone = function() {{
    if (this.bodyUsed || (this.body && this.body.locked)) throw new TypeError("native Request body is unusable");
    return new RequestNative(this);
  }};
  globalThis.Request = RequestNative;
  const nativeAbortError = () => {{
    const error = new Error("The operation was aborted");
    error.name = "AbortError";
    return error;
  }};
  const nativeTimeoutError = () => {{
    const error = new Error("The XMLHttpRequest operation timed out");
    error.name = "TimeoutError";
    return error;
  }};
  const nativeOpaqueResponseError = () => {{
    const error = new TypeError("opaque native fetch response body is unavailable");
    error.name = "TypeError";
    return error;
  }};
  const AbortSignalNative = function() {{
    this.aborted = false;
    this.reason = undefined;
    this.onabort = null;
    this._abortListeners = [];
  }};
  AbortSignalNative.prototype.addEventListener = function(type, listener) {{
    if (type !== "abort" || typeof listener !== "function" || this.aborted) return;
    if (!this._abortListeners.includes(listener)) this._abortListeners.push(listener);
  }};
  AbortSignalNative.prototype.removeEventListener = function(type, listener) {{
    if (type !== "abort") return;
    this._abortListeners = this._abortListeners.filter(candidate => candidate !== listener);
  }};
  AbortSignalNative.prototype.throwIfAborted = function() {{
    if (this.aborted) throw this.reason;
  }};
  const dispatchAbort = (signal) => {{
    const event = {{ type: "abort", target: signal }};
    const listeners = signal._abortListeners.slice();
    signal._abortListeners = [];
    for (const listener of listeners) listener.call(signal, event);
    if (typeof signal.onabort === "function") signal.onabort.call(signal, event);
  }};
  const AbortControllerNative = function() {{
    this.signal = new AbortSignalNative();
  }};
  AbortControllerNative.prototype.abort = function(reason) {{
    if (this.signal.aborted) return;
    this.signal.aborted = true;
    this.signal.reason = reason === undefined ? nativeAbortError() : reason;
    dispatchAbort(this.signal);
  }};
  AbortSignalNative.timeout = function(delay) {{
    const numeric = Number(delay);
    if (!Number.isFinite(numeric) || numeric < 0 || numeric > 2147483647)
      throw new RangeError("native AbortSignal timeout is outside the bounded range");
    const signal = new AbortSignalNative();
    scheduleTimer(() => {{
      if (signal.aborted) return;
      signal.aborted = true;
      signal.reason = nativeTimeoutError();
      dispatchAbort(signal);
    }}, Math.trunc(numeric), [], false);
    return signal;
  }};
  AbortSignalNative.abort = function(reason) {{
    const signal = new AbortSignalNative();
    signal.aborted = true;
    signal.reason = reason === undefined ? nativeAbortError() : reason;
    return signal;
  }};
  AbortSignalNative.any = function(signals) {{
    if (!signals || typeof signals[Symbol.iterator] !== "function")
      throw new TypeError("native AbortSignal.any requires an iterable");
    const iterator = signals[Symbol.iterator]();
    if (!iterator || typeof iterator.next !== "function")
      throw new TypeError("native AbortSignal.any iterable is invalid");
    const inputs = [];
    while (true) {{
      const step = iterator.next();
      if (!step || typeof step !== "object") throw new TypeError("native AbortSignal.any iterator is invalid");
      if (step.done) break;
      if (inputs.length >= {max_commands}) throw new RangeError("native AbortSignal.any signal limit exceeded");
      const signal = step.value;
      if (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function")
        throw new TypeError("native AbortSignal.any input is invalid");
      inputs.push(signal);
    }}
    const combined = new AbortSignalNative();
    const alreadyAborted = inputs.find(signal => signal.aborted);
    if (alreadyAborted) {{
      combined.aborted = true;
      combined.reason = alreadyAborted.reason === undefined ? nativeAbortError() : alreadyAborted.reason;
      return combined;
    }}
    const listeners = [];
    const abortFrom = (source) => {{
      if (combined.aborted) return;
      combined.aborted = true;
      combined.reason = source.reason === undefined ? nativeAbortError() : source.reason;
      dispatchAbort(combined);
      for (const [signal, listener] of listeners) signal.removeEventListener("abort", listener);
    }};
    for (const signal of inputs) {{
      const listener = () => abortFrom(signal);
      listeners.push([signal, listener]);
      signal.addEventListener("abort", listener);
    }}
    return combined;
  }};
  globalThis.AbortSignal = AbortSignalNative;
  globalThis.AbortController = AbortControllerNative;
  const clearFetchAbortListener = (pending) => {{
    if (pending.signal && pending.abortListener) pending.signal.removeEventListener("abort", pending.abortListener);
  }};
  const fetchNativeDispatch = (input, options, streamedBodyBytes) => {{
    const sourceRequest = input && input.__glassRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    if (typeof input !== "string" && !sourceRequest && !sourceUrl) throw new TypeError("native fetch requires a URL string, URL, or Request");
    const href = sourceRequest ? sourceRequest.url : sourceUrl ? sourceUrl.href : input;
    const hasBodyOverride = options && typeof options === "object" && Object.prototype.hasOwnProperty.call(options, "body");
    const sourceBodyPayload = (() => {{
      const payload = sourceRequest && !hasBodyOverride && sourceRequest.body !== null
        ? sourceRequest.__glassRequestBodyPayload
        : null;
      if (payload && payload.stream && streamedBodyBytes !== undefined)
        return Object.assign({{}}, payload, {{ bytes: streamedBodyBytes, stream: null }});
      return payload;
    }})();
    const settings = Object.assign(
      {{}},
      sourceRequest ? sourceRequest._settings : {{}},
      options && typeof options === "object" ? options : {{}},
    );
    const signal = settings.signal === undefined ? null : settings.signal;
    if (signal !== null && (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function")) throw new TypeError("native fetch signal is invalid");
    if (signal && signal.aborted) return Promise.reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
    const mode = settings.mode === undefined ? "cors" : String(settings.mode).toLowerCase();
    if (!["cors", "no-cors", "same-origin"].includes(mode))
      return Promise.reject(new TypeError("native fetch mode is unsupported"));
    const redirect = settings.redirect === undefined ? "follow" : String(settings.redirect).toLowerCase();
    if (!["follow", "error", "manual"].includes(redirect))
      return Promise.reject(new TypeError("native fetch redirect mode is unsupported"));
    const method = settings.method === undefined ? "GET" : String(settings.method).toUpperCase();
    const blobBody = sourceBodyPayload
      ? null
      : settings.body && settings.body.__glassNativeBlob === true
      ? settings.body
      : null;
    const binaryBody = sourceBodyPayload
      ? null
      : streamedBodyBytes !== undefined
      ? streamedBodyBytes
      : settings.body instanceof ArrayBuffer
      ? Array.from(new Uint8Array(settings.body))
      : typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(settings.body)
        ? Array.from(new Uint8Array(settings.body.buffer, settings.body.byteOffset, settings.body.byteLength))
        : null;
    const rawBody = sourceBodyPayload || streamedBodyBytes !== undefined || settings.body === undefined || settings.body === null || blobBody || binaryBody !== null
      ? null
      : String(settings.body);
    const formData = sourceBodyPayload
      ? null
      : settings.body && settings.body.__glassFormData === true
      ? settings.body
      : null;
    const urlSearchParams = sourceBodyPayload
      ? null
      : settings.body && settings.body.__glassUrlSearchParams === true
      ? settings.body
      : null;
    let body = sourceBodyPayload
      ? utf8TextFromBytes(sourceBodyPayload.bytes)
      : blobBody
      ? blobBody._text
      : binaryBody !== null
        ? utf8TextFromBytes(binaryBody)
        : rawBody;
    let bodyBase64 = sourceBodyPayload
      ? encodeBase64(sourceBodyPayload.bytes, nativeFormBodyLimit)
      : streamedBodyBytes !== undefined
      ? encodeBase64(streamedBodyBytes, nativeFormBodyLimit)
      : blobBody && Array.isArray(blobBody._bytes)
      ? encodeBase64(blobBody._bytes, nativeFormBodyLimit)
      : binaryBody !== null
        ? encodeBase64(binaryBody, nativeFormBodyLimit)
        : null;
    if (!nativeRequestMethods.includes(method)) {{
      return Promise.reject(new TypeError("native fetch method is unsupported"));
    }}
    const usesSourceBody = sourceBodyPayload !== null;
    if (usesSourceBody && streamedBodyBytes === undefined) {{
      if (!nativeRequestBodyUse(sourceRequest)) return Promise.reject(new TypeError("native Request body is unusable"));
    }}
    const requestHeaderName = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
    const forbiddenRequestHeader = (name) => [
      "accept-charset", "accept-encoding", "access-control-request-headers",
      "access-control-request-method", "connection", "content-length",
      "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
      "origin", "referer", "te", "trailer", "transfer-encoding", "upgrade",
      "user-agent", "via",
    ].includes(name) || name.startsWith("proxy-") || name.startsWith("sec-");
    const normalizeRequestHeaders = (input) => {{
      if (input === undefined || input === null) return {{}};
      if (typeof input !== "object") throw new TypeError("native fetch headers must be an object");
      if (input.__glassHeaders === true) {{
        const record = {{}};
        input._entries.forEach(entry => {{ record[entry[0]] = entry[1]; }});
        input = record;
      }}
      const normalized = {{}};
      let count = 0;
      let totalBytes = 0;
      for (const name of Object.keys(input)) {{
        const normalizedName = String(name).toLowerCase();
        if (!requestHeaderName.test(String(name)) || normalizedName.length > {fetch_header_name_limit}) throw new TypeError("native fetch header name is invalid");
        if (forbiddenRequestHeader(normalizedName)) throw new TypeError("native fetch header is forbidden");
        const value = String(input[name]);
        if (value.length > {fetch_header_value_limit} || /[\u0000-\u001f\u007f]/.test(value)) throw new TypeError("native fetch header value is invalid");
        if (Object.prototype.hasOwnProperty.call(normalized, normalizedName)) normalized[normalizedName] += ", " + value;
        else {{
          count += 1;
          if (count > {fetch_header_count_limit}) throw new RangeError("native fetch header limit exceeded");
          normalized[normalizedName] = value;
        }}
        totalBytes += normalizedName.length + value.length;
        if (totalBytes > {fetch_header_bytes_limit}) throw new RangeError("native fetch headers exceed their limit");
      }}
      return normalized;
    }};
    const requestHeaders = normalizeRequestHeaders(settings.headers);
    let contentType = null;
    if (Object.prototype.hasOwnProperty.call(requestHeaders, "content-type")) {{
      contentType = requestHeaders["content-type"];
      delete requestHeaders["content-type"];
    }}
    if (contentType === null && sourceBodyPayload && sourceBodyPayload.contentType !== null)
      contentType = sourceBodyPayload.contentType;
    const requestId = nextFetchRequestId;
    if (formData) {{
      if (contentType !== null) return Promise.reject(new TypeError("FormData chooses its own Content-Type boundary"));
      const serialized = serializeFormData(formData, requestId);
      body = serialized.body;
      bodyBase64 = serialized.bodyBase64;
      contentType = serialized.contentType;
    }}
    if (urlSearchParams) {{
      if (contentType !== null) return Promise.reject(new TypeError("URLSearchParams chooses its own Content-Type"));
      body = urlSearchParams.toString();
      bodyBase64 = null;
      contentType = "application/x-www-form-urlencoded;charset=UTF-8";
    }}
    if (blobBody && contentType === null && blobBody.type) contentType = blobBody.type;
    if (nativeBodylessMethods.includes(method) && body !== null) {{
      return Promise.reject(new TypeError(method + " fetch requests must not have a body"));
    }}
    nextFetchRequestId += 1;
    globalThis.__glassNextFetchRequestId = nextFetchRequestId;
    const credentials = settings.credentials !== "omit";
    const timeoutSetting = settings.__glassTimeoutMs === undefined
      ? null
      : Number(settings.__glassTimeoutMs);
    const timeoutMs = timeoutSetting === 0 ? null : timeoutSetting;
    if (timeoutMs !== null && (!Number.isSafeInteger(timeoutMs) || timeoutMs < 0 || timeoutMs > {max_native_xhr_timeout_ms}))
      throw new RangeError("native fetch timeout is outside the bounded XHR range");
    return new Promise((resolve, reject) => {{
      const pending = {{ resolve, reject, signal, abortListener: null }};
      const abort = () => {{
        if (fetchRequests.get(requestId) !== pending) return;
        fetchRequests.delete(requestId);
        clearFetchAbortListener(pending);
        reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
      }};
      pending.abortListener = abort;
      fetchRequests.set(requestId, pending);
      if (signal) signal.addEventListener("abort", abort);
      if (!fetchRequests.has(requestId)) return;
      pushCommand({{ kind: "fetch", request_id: requestId, href, credentials, method, headers: requestHeaders, body, body_base64: bodyBase64, content_type: contentType, mode, redirect, timeout_ms: timeoutMs }});
    }});
  }};
  const responseHeaders = (rawEntries, contentType) => {{
    const entries = [];
    const byName = new Map();
    if (Array.isArray(rawEntries)) for (const rawEntry of rawEntries) {{
      if (!Array.isArray(rawEntry) || rawEntry.length !== 2) continue;
      const name = String(rawEntry[0]).toLowerCase();
      const value = String(rawEntry[1]);
      if (!name) continue;
      const existing = byName.get(name);
      if (existing) existing[1] += ", " + value;
      else {{
        const entry = [name, value];
        byName.set(name, entry);
        entries.push(entry);
      }}
    }}
    if (!byName.has("content-type") && contentType !== null && contentType !== undefined) {{
      const entry = ["content-type", String(contentType)];
      byName.set("content-type", entry);
      entries.push(entry);
    }}
    const iterator = values => values[Symbol.iterator]();
    const headers = Object.create(HeadersNative.prototype);
    Object.defineProperty(headers, "__glassHeaders", {{ value: true }});
    Object.defineProperty(headers, "_entries", {{ value: entries }});
    const readOnly = () => {{ throw new TypeError("native response Headers are immutable"); }};
    headers.append = readOnly;
    headers.set = readOnly;
    headers.delete = readOnly;
    Object.assign(headers, {{
      get(name) {{
        const key = String(name).toLowerCase();
        const entry = entries.find(candidate => candidate[0] === key);
        return entry ? entry[1] : null;
      }},
      has(name) {{ return this.get(name) !== null; }},
      entries() {{ return iterator(entries.map(entry => [entry[0], entry[1]])); }},
      keys() {{ return iterator(entries.map(entry => entry[0])); }},
      values() {{ return iterator(entries.map(entry => entry[1])); }},
      forEach(callback, thisArg) {{
        if (typeof callback !== "function") throw new TypeError("native response header callback must be callable");
        entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], headers));
      }},
      [Symbol.iterator]() {{ return this.entries(); }},
    }});
    return Object.freeze(headers);
  }};
  const fetchStreamGroups = globalThis.__glassFetchStreamGroups instanceof Map
    ? globalThis.__glassFetchStreamGroups
    : new Map();
  const fetchStreamGroup = (streamId) => {{
    const id = Number(streamId);
    if (!Number.isSafeInteger(id) || id <= 0) throw new TypeError("native fetch stream identifier is invalid");
    let group = fetchStreamGroups.get(id);
    if (!group) {{
      group = {{ id, chunks: [], totalBytes: 0, done: false, error: null, streams: [], waiters: [], started: false, readRequested: false, cancelRequested: false }};
      fetchStreamGroups.set(id, group);
    }}
    return group;
  }};
  const fetchStreamCanAcceptDemand = (group) => group && group.streams
    .filter(state => !state.cancelled && !state.consumedByResponse)
    .every(state => state.queued.length < {fetch_stream_queue_limit});
  const requestFetchStreamRead = (group) => {{
    if (!group || group.done || group.error !== null || group.cancelRequested || group.readRequested || !fetchStreamCanAcceptDemand(group)) return;
    group.readRequested = true;
    pushCommand({{ kind: "fetchStreamRead", stream_id: Number(group.id) }});
  }};
  const cancelFetchStreamGroup = (group) => {{
    if (group.done || group.cancelRequested) return;
    group.cancelRequested = true;
    group.done = true;
    for (const state of group.streams) {{
      state.done = true;
      if (state.pendingRead) {{ state.pendingRead.resolve({{ value: undefined, done: true }}); state.pendingRead = null; }}
      settleReadableStreamClosed(state);
    }}
    settleFetchStreamWaiters(group);
    pushCommand({{ kind: "fetchStreamCancel", stream_id: Number(group.id) }});
  }};
  const maybeCancelFetchStreamGroup = (group) => {{
    if (!group) return;
    if (group.streams.some(state => !state.cancelled && !state.consumedByResponse) || group.waiters.length > 0) return;
    cancelFetchStreamGroup(group);
  }};
  const flattenFetchStream = (group) => {{
    const bytes = [];
    for (const chunk of group.chunks) bytes.push(...chunk);
    return bytes;
  }};
  const settleFetchStreamWaiters = (group) => {{
    const waiters = group.waiters.splice(0);
    const bytes = flattenFetchStream(group);
    for (const waiter of waiters) waiter.resolve(bytes.slice());
  }};
  const rejectFetchStreamWaiters = (group) => {{
    const waiters = group.waiters.splice(0);
    const error = new Error(group.error || "native fetch response stream failed");
    for (const waiter of waiters) waiter.reject(error);
  }};
  const readableStreamState = (stream) => {{
    if (!stream || stream.__glassReadableStream !== true || !stream._state)
      throw new TypeError("native ReadableStream receiver is invalid");
    return stream._state;
  }};
  const readableStreamStrategy = (strategy, byteMode) => {{
    const value = strategy === undefined || strategy === null ? {{}} : strategy;
    if (typeof value !== "object" && typeof value !== "function")
      throw new TypeError("native ReadableStream strategy is invalid");
    let highWaterMark = value.highWaterMark === undefined
      ? {fetch_stream_queue_limit}
      : Number(value.highWaterMark);
    if (!Number.isFinite(highWaterMark) || highWaterMark < 0)
      throw new RangeError("native ReadableStream highWaterMark is invalid");
    highWaterMark = Math.min(highWaterMark, {fetch_stream_queue_limit});
    const size = value.size === undefined
      ? (chunk => byteMode
        ? (chunk instanceof ArrayBuffer || ArrayBuffer.isView(chunk) ? chunk.byteLength : 1)
        : 1)
      : value.size;
    if (typeof size !== "function")
      throw new TypeError("native ReadableStream size algorithm is invalid");
    return {{ highWaterMark, size }};
  }};
  const readableByteView = (value) => {{
    if (value instanceof ArrayBuffer)
      return new Uint8Array(value.slice(0));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value))
      return new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();
    throw new TypeError("native byte stream chunks must be ArrayBuffer or views");
  }};
  const readableValueByteLength = (value) => value instanceof ArrayBuffer
    ? value.byteLength
    : typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value)
      ? value.byteLength
      : typeof value === "string"
        ? blobUtf8Bytes(value).length
        : 1;
  const readableChunkSize = (state, value) => {{
    const size = Number(state.strategy.size(value));
    if (!Number.isFinite(size) || size < 0)
      throw new RangeError("native ReadableStream chunk size is invalid");
    return size;
  }};
  const readableByobResultView = (view, byteLength) => {{
    const bytesPerElement = Number(view.BYTES_PER_ELEMENT) || 1;
    const elements = Math.floor(byteLength / bytesPerElement);
    if (typeof view.subarray === "function") return view.subarray(0, elements);
    return new Uint8Array(view.buffer, view.byteOffset, elements * bytesPerElement);
  }};
  const clearReadablePendingRead = (state) => {{
    const pending = state.pendingRead;
    state.pendingRead = null;
    state.byobRequest = null;
    return pending;
  }};
  const resolveReadablePending = (state, value, done) => {{
    const pending = state.pendingRead;
    if (!pending) return false;
    if (pending.byobView) {{
      if (done) {{
        const current = clearReadablePendingRead(state);
        current.resolve({{ value: readableByobResultView(current.byobView, 0), done: true }});
        return true;
      }}
      let bytes;
      try {{ bytes = readableByteView(value); }}
      catch (error) {{
        const current = clearReadablePendingRead(state);
        current.reject(error);
        return true;
      }}
      const bytesPerElement = Number(pending.byobView.BYTES_PER_ELEMENT) || 1;
      const capacity = Math.floor(pending.byobView.byteLength / bytesPerElement) * bytesPerElement;
      const count = Math.min(bytes.length, capacity);
      if (count === 0) return false;
      new Uint8Array(pending.byobView.buffer, pending.byobView.byteOffset, count)
        .set(bytes.subarray(0, count));
      if (count < bytes.length) {{
        const remainder = bytes.slice(count);
        state.queued.unshift(remainder);
        const remainderSize = readableChunkSize(state, remainder);
        state.queueSizes.unshift(remainderSize);
        state.queueSize += remainderSize;
      }}
      const current = clearReadablePendingRead(state);
      current.resolve({{ value: readableByobResultView(current.byobView, count), done: false }});
      return true;
    }}
    const current = clearReadablePendingRead(state);
    current.resolve({{
      value: value === undefined ? undefined : state.byteMode ? new Uint8Array(readableByteView(value)) : value,
      done: Boolean(done),
    }});
    return true;
  }};
  const readableByobRequest = (state) => {{
    const pending = state.pendingRead;
    if (!pending || !pending.byobView || state.done || state.cancelled || state.error !== null)
      return null;
    if (state.byobRequest) return state.byobRequest;
    const request = {{
      get view() {{ return pending.byobView; }},
      respond(bytesWritten) {{
        const count = Number(bytesWritten);
        if (!Number.isInteger(count) || count < 0 || count > pending.byobView.byteLength)
          throw new RangeError("native ReadableStream BYOB response is invalid");
        if (count === 0 && !state.done)
          throw new TypeError("native ReadableStream BYOB response is empty");
        state.sourceProduced = true;
        const current = clearReadablePendingRead(state);
        current.resolve({{ value: readableByobResultView(current.byobView, count), done: false }});
      }},
      respondWithNewView(view) {{
        if (!view || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(view))
          throw new TypeError("native ReadableStream BYOB view is invalid");
        const bytes = readableByteView(view);
        if (bytes.byteLength > pending.byobView.byteLength)
          throw new RangeError("native ReadableStream BYOB view exceeds its request");
        new Uint8Array(pending.byobView.buffer, pending.byobView.byteOffset, bytes.byteLength)
          .set(bytes);
        state.sourceProduced = true;
        const current = clearReadablePendingRead(state);
        current.resolve({{ value: readableByobResultView(current.byobView, bytes.byteLength), done: false }});
      }},
    }};
    state.byobRequest = Object.freeze(request);
    return state.byobRequest;
  }};
  const settleReadableStreamClosed = (state) => {{
    if (!state.closedWaiters || state.closedWaiters.length === 0) return;
    const waiters = state.closedWaiters.splice(0);
    if (state.error !== null) {{
      const error = new Error(state.error);
      for (const waiter of waiters) waiter.reject(error);
    }} else if (readableStreamDone(state)) {{
      for (const waiter of waiters) waiter.resolve();
    }} else {{
      state.closedWaiters.push(...waiters);
    }}
  }};
  const watchReadableStreamClosed = (state, resolve, reject) => {{
    if (state.error !== null) {{
      reject(new Error(state.error));
    }} else if (readableStreamDone(state)) {{
      resolve();
    }} else {{
      state.closedWaiters.push({{ resolve, reject }});
    }}
  }};
  const markReadableStreamDisturbed = (state) => {{
    if (state.disturbed) return;
    state.disturbed = true;
    if (typeof state.onDisturb === "function") state.onDisturb();
  }};
  const failReadableStreamSource = (state, error) => {{
    if (state.done || state.cancelled || state.error !== null) return;
    state.error = error instanceof Error ? error.message : String(error);
    state.done = true;
    if (state.pendingRead) {{
      const pending = clearReadablePendingRead(state);
      pending.reject(new Error(state.error));
    }}
    settleReadableStreamClosed(state);
  }};
  const closeReadableStreamSource = (state) => {{
    if (state.done || state.cancelled || state.error !== null) return;
    state.done = true;
    if (state.pendingRead) {{
      resolveReadablePending(state, undefined, true);
    }}
    settleReadableStreamClosed(state);
  }};
  const enqueueReadableStreamSource = (state, value) => {{
    if (state.done || state.cancelled || state.error !== null)
      throw new TypeError("native ReadableStream controller is closed");
    state.sourceProduced = true;
    const queuedValue = state.byteMode ? readableByteView(value) : value;
    const byteLength = readableValueByteLength(queuedValue);
    if (byteLength > storageValueLimit) throw new RangeError("native ReadableStream chunk exceeds its limit");
    const size = readableChunkSize(state, queuedValue);
    if (size > storageValueLimit) throw new RangeError("native ReadableStream chunk exceeds its limit");
    if (state.pendingRead) {{
      if (resolveReadablePending(state, queuedValue, false)) return;
    }} else {{
      if (state.queued.length >= {fetch_stream_queue_limit})
        throw new RangeError("native ReadableStream queue limit exceeded");
      state.queued.push(queuedValue);
      state.queueSizes.push(size);
      state.queueSize += size;
    }}
  }};
  const pullReadableStreamSource = (state) => {{
    const source = state.underlyingSource;
    if (!source || state.sourceStarting || state.sourcePulling || state.done || state.cancelled || state.error !== null)
      return;
    if (typeof source.pull !== "function") return;
    state.sourceProduced = false;
    state.sourcePulling = true;
    let result;
    try {{
      result = source.pull(state.sourceController);
    }} catch (error) {{
      state.sourcePulling = false;
      failReadableStreamSource(state, error);
      return;
    }}
    Promise.resolve(result).then(
      () => {{
        const produced = state.sourceProduced;
        state.sourceProduced = false;
        state.sourcePulling = false;
        if (produced && state.pendingRead && !state.done && !state.cancelled && state.error === null)
          pullReadableStreamSource(state);
      }},
      error => {{ state.sourcePulling = false; failReadableStreamSource(state, error); }},
    );
  }};
  const startReadableStreamSource = (state) => {{
    const source = state.underlyingSource;
    if (!source || typeof source.start !== "function") {{
      state.sourceStarting = false;
      return;
    }}
    let result;
    try {{
      result = source.start(state.sourceController);
    }} catch (error) {{
      state.sourceStarting = false;
      failReadableStreamSource(state, error);
      return;
    }}
    Promise.resolve(result).then(
      () => {{ state.sourceStarting = false; pullReadableStreamSource(state); }},
      error => {{ state.sourceStarting = false; failReadableStreamSource(state, error); }},
    );
  }};
  const cancelReadableStreamSource = (state, reason) => {{
    if (state.cancelled) return Promise.resolve(undefined);
    markReadableStreamDisturbed(state);
    state.cancelled = true;
    state.done = true;
    state.queued = [];
    state.queueSizes = [];
    state.queueSize = 0;
    if (state.pendingRead) {{
      resolveReadablePending(state, undefined, true);
    }}
    settleReadableStreamClosed(state);
    if (!state.underlyingSource || typeof state.underlyingSource.cancel !== "function")
      return Promise.resolve(undefined);
    try {{ return Promise.resolve(state.underlyingSource.cancel(reason)); }}
    catch (error) {{ return Promise.reject(error); }}
  }};
  const readableStreamDone = (state) => state.cancelled || state.done || state.error !== null;
  const responseBodyIsUsed = (response) => {{
    if (!response || !response.__glassBodyState) return false;
    const body = response.body;
    if (!body) return response.__glassBodyState.used === true;
    return response.__glassBodyState.used === true || readableStreamState(body).disturbed === true;
  }};
  const responseBodyUse = (response) => {{
    const body = response && response.body;
    if (!body) return true;
    const state = readableStreamState(body);
    if (responseBodyIsUsed(response) || state.locked) return false;
    response.__glassBodyState.used = true;
    markReadableStreamDisturbed(state);
    state.consumedByResponse = true;
    if (!response.__glassResponseStreamBody) state.queued = [];
    return true;
  }};
  const responseBodyUnusable = () => Promise.reject(new TypeError("native Response body is unusable"));
  const defineResponseBodyState = (response) => {{
    Object.defineProperty(response, "__glassBodyState", {{ value: {{ used: false }} }});
    Object.defineProperty(response, "bodyUsed", {{
      configurable: true,
      get() {{ return responseBodyIsUsed(response); }},
    }});
  }};
  const ReadableStreamNative = typeof globalThis.__glassReadableStreamConstructor === "function"
    ? globalThis.__glassReadableStreamConstructor
    : function(bytes, streamId, onDisturb) {{
    if (!(this instanceof ReadableStreamNative)) throw new TypeError("native ReadableStream requires new");
    const underlyingSource = bytes && typeof bytes === "object" && !Array.isArray(bytes)
      && (streamId === undefined || (streamId && typeof streamId === "object"))
      ? bytes
      : null;
    const streamOptions = underlyingSource && streamId && typeof streamId === "object" ? streamId : {{}};
    if (underlyingSource && underlyingSource.type !== undefined && underlyingSource.type !== "bytes")
      throw new TypeError("native ReadableStream type is unsupported");
    const byteMode = (underlyingSource && underlyingSource.type === "bytes")
      || (underlyingSource === null && Array.isArray(bytes))
      || (underlyingSource === null && streamId !== undefined && streamId !== null);
    const strategy = readableStreamStrategy(streamOptions, byteMode);
    const hostId = underlyingSource || streamId === undefined || streamId === null ? null : Number(streamId);
    const values = hostId === null && underlyingSource === null && Array.isArray(bytes) ? bytes.slice() : [];
    if (values.length > storageValueLimit) throw new RangeError("native ReadableStream body limit exceeded");
    for (const value of values) {{
      if (!Number.isInteger(value) || value < 0 || value > 255)
        throw new TypeError("native ReadableStream bytes are invalid");
    }}
    const group = hostId === null ? null : fetchStreamGroup(hostId);
    const queued = group ? group.chunks.map(chunk => chunk.slice()) : [];
    const queueSizes = queued.map(value => value.byteLength);
    const state = {{
      bytes: values,
      offset: 0,
      queued,
      queueSizes,
      queueSize: queueSizes.reduce((total, size) => total + size, 0),
      strategy,
      underlyingSource,
      sourceController: null,
      sourceStarting: Boolean(underlyingSource && typeof underlyingSource.start === "function"),
      sourcePulling: false,
      sourceProduced: false,
      locked: false,
      cancelled: false,
      disturbed: false,
      consumedByResponse: false,
      consumingResponseBody: false,
      consumedByRequest: false,
      consumingRequestBody: false,
      done: underlyingSource ? false : group ? group.done : true,
      error: group ? group.error : null,
      streamId: hostId,
      group,
      pendingRead: null,
      byobRequest: null,
      closedWaiters: [],
      onDisturb: typeof onDisturb === "function" ? onDisturb : null,
      byteMode,
    }};
    if (underlyingSource) {{
      state.sourceController = Object.freeze({{
        get desiredSize() {{ return state.strategy.highWaterMark - state.queueSize; }},
        get byobRequest() {{ return readableByobRequest(state); }},
        enqueue(value) {{ enqueueReadableStreamSource(state, value); }},
        close() {{ closeReadableStreamSource(state); }},
        error(error) {{ failReadableStreamSource(state, error); }},
      }});
    }}
    if (group) group.streams.push(state);
    Object.defineProperty(this, "__glassReadableStream", {{ value: true }});
    Object.defineProperty(this, "_state", {{ value: state }});
    if (underlyingSource) startReadableStreamSource(state);
    Object.freeze(this);
  }};
  Object.defineProperty(ReadableStreamNative.prototype, "locked", {{
    configurable: true,
    get() {{ return readableStreamState(this).locked; }},
  }});
  ReadableStreamNative.prototype.getReader = function(options) {{
    const state = readableStreamState(this);
    if (state.locked) throw new TypeError("native ReadableStream is already locked");
    const readerOptions = options === undefined || options === null ? {{}} : options;
    if (typeof readerOptions !== "object" && typeof readerOptions !== "function")
      throw new TypeError("native ReadableStream reader options are invalid");
    const mode = readerOptions.mode === undefined ? undefined : String(readerOptions.mode);
    if (mode !== undefined && mode !== "byob")
      throw new TypeError("native ReadableStream reader mode is unsupported");
    const byob = mode === "byob";
    if (byob && !state.byteMode)
      throw new TypeError("native BYOB reader requires a byte stream");
    state.locked = true;
    let released = false;
    let resolveClosed;
    let rejectClosed;
    const closedPromise = new Promise((resolve, reject) => {{
      resolveClosed = resolve;
      rejectClosed = reject;
    }});
    watchReadableStreamClosed(state, resolveClosed, rejectClosed);
    const release = () => {{
      if (released) return;
      released = true;
      state.locked = false;
    }};
    const readQueued = () => {{
      if (state.streamId === null && state.underlyingSource === null) {{
        if (state.offset >= state.bytes.length) return null;
        const value = new Uint8Array(state.bytes.slice(state.offset));
        state.offset = state.bytes.length;
        return {{ value, done: false }};
      }}
      if (state.queued.length === 0) return null;
      const value = state.queued.shift();
      state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
      return {{ value: state.byteMode ? new Uint8Array(value) : value, done: false }};
    }};
    const readByteQueued = (view) => {{
      const target = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
      if (state.streamId === null && state.underlyingSource === null) {{
        if (state.offset >= state.bytes.length) return null;
        const count = Math.min(target.byteLength, state.bytes.length - state.offset);
        target.set(state.bytes.slice(state.offset, state.offset + count));
        state.offset += count;
        return {{ value: readableByobResultView(view, count), done: false }};
      }}
      if (state.queued.length === 0) return null;
      const value = new Uint8Array(state.queued[0]);
      const count = Math.min(target.byteLength, value.byteLength);
      target.set(value.subarray(0, count));
      if (count === value.byteLength) {{
        state.queued.shift();
        state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
      }} else {{
        const remainder = value.slice(count);
        state.queued[0] = remainder;
        state.queueSize = Math.max(0, state.queueSize - state.queueSizes[0]);
        state.queueSizes[0] = readableChunkSize(state, remainder);
        state.queueSize += state.queueSizes[0];
      }}
      return {{ value: readableByobResultView(view, count), done: false }};
    }};
    const resolveQueuedRead = () => {{
      const pending = state.pendingRead;
      if (!pending) return false;
      const result = pending.byobView ? readByteQueued(pending.byobView) : readQueued();
      if (!result) return false;
      const current = clearReadablePendingRead(state);
      current.resolve(result);
      return true;
    }};
    const read = (view) => {{
      if (state.consumedByResponse && !state.consumingResponseBody) return responseBodyUnusable();
      if (state.consumedByRequest && !state.consumingRequestBody) return Promise.reject(new TypeError("native Request body is unusable"));
      if (byob) {{
        if (!view || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(view) || view.byteLength === 0)
          throw new TypeError("native BYOB read requires a non-empty view");
      }} else if (view !== undefined) {{
        throw new TypeError("native default reader read does not accept a view");
      }}
      markReadableStreamDisturbed(state);
      const queued = byob ? readByteQueued(view) : readQueued();
      if (queued) {{
        requestFetchStreamRead(state.group);
        return Promise.resolve(queued);
      }}
      if (state.error !== null) return Promise.reject(new Error(state.error));
      if (readableStreamDone(state)) {{
        return Promise.resolve(byob
          ? {{ value: readableByobResultView(view, 0), done: true }}
          : {{ value: undefined, done: true }});
      }}
      if (state.pendingRead) return Promise.reject(new TypeError("native ReadableStream read is already pending"));
      const promise = new Promise((resolve, reject) => {{ state.pendingRead = {{ resolve, reject, byobView: byob ? view : null }}; }});
      try {{
        if (state.underlyingSource) pullReadableStreamSource(state);
        else requestFetchStreamRead(state.group);
      }}
      catch (error) {{ state.pendingRead = null; return Promise.reject(error); }}
      return promise;
    }};
    const reader = {{
      read(view) {{
        if (released) return Promise.reject(new TypeError("native ReadableStream reader is released"));
        return read(view);
      }},
      cancel(reason) {{
        if (released) return Promise.reject(new TypeError("native ReadableStream reader is released"));
        if (state.underlyingSource) return cancelReadableStreamSource(state, reason);
        state.cancelled = true;
        markReadableStreamDisturbed(state);
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) resolveReadablePending(state, undefined, true);
        settleReadableStreamClosed(state);
        maybeCancelFetchStreamGroup(state.group);
        return Promise.resolve(undefined);
      }},
      releaseLock() {{ release(); }},
      return() {{
        if (released) return Promise.resolve({{ value: undefined, done: true }});
        if (state.underlyingSource) {{
          const cancellation = cancelReadableStreamSource(state);
          release();
          return cancellation.then(() => ({{ value: undefined, done: true }}));
        }}
        state.cancelled = true;
        markReadableStreamDisturbed(state);
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) resolveReadablePending(state, undefined, true);
        settleReadableStreamClosed(state);
        release();
        maybeCancelFetchStreamGroup(state.group);
        return Promise.resolve({{ value: undefined, done: true }});
      }},
      [Symbol.asyncIterator]() {{ return this; }},
    }};
    Object.defineProperty(reader, "closed", {{ value: closedPromise }});
    return Object.freeze(reader);
  }};
  ReadableStreamNative.prototype.cancel = function(reason) {{
    const state = readableStreamState(this);
    if (state.locked) return Promise.reject(new TypeError("native ReadableStream is locked"));
    if (state.underlyingSource) return cancelReadableStreamSource(state, reason);
    state.cancelled = true;
    markReadableStreamDisturbed(state);
    state.done = true;
    state.queued = [];
    state.queueSizes = [];
    state.queueSize = 0;
    settleReadableStreamClosed(state);
    maybeCancelFetchStreamGroup(state.group);
    return Promise.resolve(undefined);
  }};
  ReadableStreamNative.prototype[Symbol.asyncIterator] = function() {{
    const reader = this.getReader();
    return Object.freeze({{
      next() {{ return reader.read(); }},
      return() {{ return reader.return(); }},
      [Symbol.asyncIterator]() {{ return this; }},
    }});
  }};
  ReadableStreamNative.prototype.tee = function() {{
    const sourceState = readableStreamState(this);
    if (sourceState.locked) throw new TypeError("native ReadableStream is locked");
    markReadableStreamDisturbed(sourceState);
    const upstreamReader = this.getReader();
    const tee = {{
      upstreamReader,
      reading: false,
      done: false,
      error: null,
      upstreamReleased: false,
      branches: [],
    }};
    const releaseUpstream = () => {{
      if (tee.upstreamReleased) return;
      tee.upstreamReleased = true;
      tee.upstreamReader.releaseLock();
    }};
    const cancelUpstream = reason => {{
      if (tee.upstreamReleased) return Promise.resolve(undefined);
      tee.done = true;
      return Promise.resolve(tee.upstreamReader.cancel(reason)).then(
        () => {{ releaseUpstream(); }},
        error => {{ releaseUpstream(); throw error; }},
      );
    }};
    const failTee = error => {{
      if (tee.error !== null) return Promise.resolve(undefined);
      const normalized = error instanceof Error ? error : new Error(String(error));
      tee.error = normalized.message;
      tee.done = true;
      for (const branch of tee.branches) {{
        if (!branch.cancelled && branch.controller) branch.controller.error(normalized);
      }}
      return cancelUpstream(normalized).catch(() => undefined);
    }};
    const finishTee = () => {{
      if (tee.done) return;
      tee.done = true;
      for (const branch of tee.branches) {{
        if (!branch.cancelled && branch.controller) branch.controller.close();
      }}
      releaseUpstream();
    }};
    const maybePullTee = () => {{
      if (tee.reading || tee.done || tee.error !== null) return Promise.resolve(undefined);
      const activeBranches = tee.branches.filter(branch => !branch.cancelled);
      if (activeBranches.length === 0) return cancelUpstream();
      if (activeBranches.some(branch => branch.state && branch.state.queued.length >= {fetch_stream_queue_limit}))
        return Promise.resolve(undefined);
      tee.reading = true;
      return tee.upstreamReader.read().then(
        result => {{
          tee.reading = false;
          if (result.done) {{
            finishTee();
            return;
          }}
          for (const branch of tee.branches) {{
            if (branch.cancelled || !branch.controller) continue;
            try {{ branch.controller.enqueue(result.value); }}
            catch (error) {{ return failTee(error); }}
          }}
        }},
        error => {{
          tee.reading = false;
          return failTee(error);
        }},
      );
    }};
    const cancelBranch = (branch, reason) => {{
      if (branch.cancelled) return Promise.resolve(undefined);
      branch.cancelled = true;
      if (tee.branches.every(candidate => candidate.cancelled)) return cancelUpstream(reason);
      return Promise.resolve(undefined);
    }};
    const createBranch = () => {{
      const branch = {{ controller: null, stream: null, state: null, cancelled: false }};
      const branchSource = {{
        start(controller) {{ branch.controller = controller; }},
        pull() {{ return maybePullTee(); }},
        cancel(reason) {{ return cancelBranch(branch, reason); }},
      }};
      if (sourceState.byteMode) branchSource.type = "bytes";
      branch.stream = new ReadableStreamNative(branchSource);
      branch.state = readableStreamState(branch.stream);
      tee.branches.push(branch);
      return branch.stream;
    }};
    const branches = [createBranch(), createBranch()];
    return branches;
  }};
  const writableStreamState = (stream) => {{
    if (!stream || stream.__glassWritableStream !== true || !stream._state)
      throw new TypeError("native WritableStream receiver is invalid");
    return stream._state;
  }};
  const settleWritableStreamClosed = (state) => {{
    if (!state.closedWaiters || state.closedWaiters.length === 0) return;
    const waiters = state.closedWaiters.splice(0);
    if (state.error !== null) {{
      const error = new Error(state.error);
      for (const waiter of waiters) waiter.reject(error);
    }} else if (state.closed) {{
      for (const waiter of waiters) waiter.resolve();
    }} else {{
      state.closedWaiters.push(...waiters);
    }}
  }};
  const watchWritableStreamClosed = (state, resolve, reject) => {{
    if (state.error !== null) reject(new Error(state.error));
    else if (state.closed) resolve();
    else state.closedWaiters.push({{ resolve, reject }});
  }};
  const failWritableStream = (state, error) => {{
    if (state.closed) return;
    state.error = error instanceof Error ? error.message : String(error);
    state.closed = true;
    state.closing = true;
    settleWritableStreamClosed(state);
  }};
  const finishWritableStream = (state) => {{
    if (state.closed) return;
    state.closed = true;
    state.closing = true;
    settleWritableStreamClosed(state);
  }};
  const WritableStreamNative = function(underlyingSink) {{
    if (!(this instanceof WritableStreamNative)) throw new TypeError("native WritableStream requires new");
    const sink = underlyingSink === undefined || underlyingSink === null ? {{}} : underlyingSink;
    if (typeof sink !== "object" && typeof sink !== "function")
      throw new TypeError("native WritableStream sink is invalid");
    const state = {{
      sink,
      locked: false,
      closing: false,
      closed: false,
      error: null,
      writeQueue: 0,
      writeTail: Promise.resolve(undefined),
      closePromise: null,
      startPromise: null,
      aborted: false,
      closedWaiters: [],
    }};
    let startResult;
    try {{ startResult = typeof sink.start === "function" ? sink.start() : undefined; }}
    catch (error) {{
      state.error = error instanceof Error ? error.message : String(error);
      state.closed = true;
      state.closing = true;
      startResult = Promise.reject(error);
    }}
    state.startPromise = Promise.resolve(startResult).then(
      () => undefined,
      error => {{ failWritableStream(state, error); throw error; }},
    );
    state.startPromise.catch(() => undefined);
    Object.defineProperty(this, "__glassWritableStream", {{ value: true }});
    Object.defineProperty(this, "_state", {{ value: state }});
    Object.freeze(this);
  }};
  Object.defineProperty(WritableStreamNative.prototype, "locked", {{
    configurable: true,
    get() {{ return writableStreamState(this).locked; }},
  }});
  const writableStreamWrite = (state, chunk) => {{
    if (state.error !== null) return Promise.reject(new Error(state.error));
    if (state.closed || state.closing) return Promise.reject(new TypeError("native WritableStream is closed"));
    if (state.writeQueue >= {fetch_stream_queue_limit})
      return Promise.reject(new RangeError("native WritableStream queue limit exceeded"));
    state.writeQueue += 1;
    const operation = state.writeTail
      .then(() => state.startPromise)
      .then(() => {{
        if (state.error !== null) throw new Error(state.error);
        if (typeof state.sink.write !== "function") return undefined;
        return state.sink.write(chunk);
      }})
      .then(
        () => {{ state.writeQueue -= 1; }},
        error => {{ state.writeQueue -= 1; failWritableStream(state, error); throw error; }},
      );
    state.writeTail = operation.catch(() => undefined);
    return operation;
  }};
  const writableStreamClose = (state) => {{
    if (state.error !== null) return Promise.reject(new Error(state.error));
    if (state.closePromise) return state.closePromise;
    if (state.closed) return Promise.resolve(undefined);
    state.closing = true;
    state.closePromise = state.writeTail
      .then(() => state.startPromise)
      .then(() => {{
        if (state.error !== null) throw new Error(state.error);
        return typeof state.sink.close === "function" ? state.sink.close() : undefined;
      }})
      .then(
        () => {{ finishWritableStream(state); }},
        error => {{ failWritableStream(state, error); throw error; }},
      );
    return state.closePromise;
  }};
  const writableStreamAbort = (state, reason) => {{
    if (state.aborted) return state.error === null
      ? Promise.resolve(undefined)
      : Promise.reject(new Error(state.error));
    if (state.closed && state.error === null) return Promise.resolve(undefined);
    state.aborted = true;
    state.closing = true;
    const error = reason instanceof Error ? reason : new Error(String(reason || "native WritableStream aborted"));
    state.error = error.message;
    state.closed = true;
    settleWritableStreamClosed(state);
    let result;
    try {{ result = typeof state.sink.abort === "function" ? state.sink.abort(reason) : undefined; }}
    catch (abortError) {{ return Promise.reject(abortError); }}
    return Promise.resolve(result);
  }};
  WritableStreamNative.prototype.getWriter = function() {{
    const state = writableStreamState(this);
    if (state.locked) throw new TypeError("native WritableStream is already locked");
    state.locked = true;
    let released = false;
    let resolveClosed;
    let rejectClosed;
    const closed = new Promise((resolve, reject) => {{
      resolveClosed = resolve;
      rejectClosed = reject;
    }});
    watchWritableStreamClosed(state, resolveClosed, rejectClosed);
    const writer = {{
      get ready() {{ return Promise.resolve(undefined); }},
      get desiredSize() {{ return state.closed || state.error !== null ? null : {fetch_stream_queue_limit} - state.writeQueue; }},
      get closed() {{ return closed; }},
      write(chunk) {{
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return writableStreamWrite(state, chunk);
      }},
      close() {{
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return writableStreamClose(state);
      }},
      abort(reason) {{
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return writableStreamAbort(state, reason);
      }},
      releaseLock() {{
        if (released) return;
        released = true;
        state.locked = false;
      }},
    }};
    return Object.freeze(writer);
  }};
  WritableStreamNative.prototype.abort = function(reason) {{
    const state = writableStreamState(this);
    if (state.locked) return Promise.reject(new TypeError("native WritableStream is locked"));
    return writableStreamAbort(state, reason);
  }};
  ReadableStreamNative.prototype.pipeTo = function(destination, options) {{
    const sourceState = readableStreamState(this);
    const destinationState = writableStreamState(destination);
    if (sourceState.locked) throw new TypeError("native ReadableStream is locked");
    if (destinationState.locked) throw new TypeError("native WritableStream is locked");
    const settings = options && typeof options === "object" ? options : {{}};
    const preventClose = settings.preventClose === true;
    const preventAbort = settings.preventAbort === true;
    const preventCancel = settings.preventCancel === true;
    const signal = settings.signal === undefined ? null : settings.signal;
    if (signal !== null && (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
      throw new TypeError("native pipeTo signal is invalid");
    if (signal && signal.aborted) return Promise.reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
    const reader = this.getReader();
    const writer = destination.getWriter();
    let abortListener = null;
    const cleanup = () => {{
      if (signal && abortListener) signal.removeEventListener("abort", abortListener);
      reader.releaseLock();
      writer.releaseLock();
    }};
    const readNext = () => reader.read().then(result => {{
      if (result.done) {{
        reader.releaseLock();
        return preventClose ? undefined : writer.close();
      }}
      return writer.write(result.value).then(readNext);
    }});
    const abortPromise = signal
      ? new Promise((_, reject) => {{
          abortListener = () => reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
          signal.addEventListener("abort", abortListener);
        }})
      : new Promise(() => {{}});
    const operation = Promise.race([readNext(), abortPromise]);
    return operation.then(
      value => {{ cleanup(); return value; }},
      error => {{
        const actions = [];
        if (!preventAbort) actions.push(writer.abort(error).catch(() => undefined));
        if (!preventCancel) actions.push(reader.cancel(error).catch(() => undefined));
        return Promise.all(actions).then(() => {{ cleanup(); throw error; }});
      }},
    );
  }};
  ReadableStreamNative.prototype.pipeThrough = function(transform, options) {{
    if (!transform || typeof transform !== "object") throw new TypeError("native pipeThrough transform is invalid");
    const writable = transform.writable;
    const readable = transform.readable;
    writableStreamState(writable);
    readableStreamState(readable);
    this.pipeTo(writable, options);
    return readable;
  }};
  const TransformStreamNative = function(transformer) {{
    if (!(this instanceof TransformStreamNative)) throw new TypeError("native TransformStream requires new");
    const source = transformer === undefined || transformer === null ? {{}} : transformer;
    if (typeof source !== "object" && typeof source !== "function")
      throw new TypeError("native TransformStream transformer is invalid");
    const transformState = {{ readable: null, controller: null, errored: null, terminated: false }};
    const readable = new ReadableStreamNative({{
      start(controller) {{ transformState.controller = controller; }},
    }});
    transformState.readable = readable;
    const transformController = Object.freeze({{
      get desiredSize() {{
        if (transformState.errored !== null || transformState.terminated) return null;
        const outputState = readableStreamState(readable);
        return outputState.strategy.highWaterMark - outputState.queueSize;
      }},
      enqueue(value) {{
        if (transformState.errored !== null || transformState.terminated)
          throw new TypeError("native TransformStream controller is closed");
        transformState.controller.enqueue(value);
      }},
      error(reason) {{
        if (transformState.errored !== null) return;
        transformState.errored = reason instanceof Error ? reason.message : String(reason);
        transformState.controller.error(reason);
      }},
      terminate() {{
        if (transformState.errored !== null || transformState.terminated) return;
        transformState.terminated = true;
        transformState.controller.close();
      }},
    }});
    const invoke = (name, args) => {{
      if (transformState.errored !== null) return Promise.reject(new Error(transformState.errored));
      if (transformState.terminated && name === "transform")
        return Promise.reject(new TypeError("native TransformStream is terminated"));
      const callback = source[name];
      if (typeof callback !== "function") return Promise.resolve(undefined);
      try {{ return Promise.resolve(callback.call(source, ...args)); }}
      catch (error) {{
        transformState.errored = error instanceof Error ? error.message : String(error);
        if (!transformState.terminated) transformState.controller.error(error);
        return Promise.reject(error);
      }}
    }};
    const writable = new WritableStreamNative({{
      start() {{ return invoke("start", [transformController]); }},
      write(chunk) {{
        if (typeof source.transform === "function") return invoke("transform", [chunk, transformController]);
        transformController.enqueue(chunk);
        return undefined;
      }},
      close() {{
        return invoke("flush", [transformController]).then(() => {{
          if (transformState.errored !== null || transformState.terminated) return;
          transformState.controller.close();
        }});
      }},
      abort(reason) {{
        if (!transformState.terminated && transformState.errored === null) transformState.controller.error(reason);
        return undefined;
      }},
    }});
    this.readable = readable;
    this.writable = writable;
    Object.freeze(this);
  }};
  globalThis.TransformStream = TransformStreamNative;
  globalThis.WritableStream = WritableStreamNative;
  globalThis.__glassDispatchFetchStreamEvent = (streamId, payload) => {{
    const group = fetchStreamGroup(streamId);
    if (!payload || typeof payload !== "object" || group.done) return null;
    const type = String(payload.type || "");
    group.readRequested = false;
    if (type === "chunk") {{
      let bytes;
      try {{ bytes = decodeBase64(String(payload.dataBase64 || ""), {fetch_stream_chunk_limit}); }}
      catch (_) {{ group.error = "native fetch response stream chunk is invalid"; group.done = true; rejectFetchStreamWaiters(group); return null; }}
      if (group.totalBytes + bytes.length > {fetch_stream_body_limit}) {{
        group.error = "native fetch response stream exceeds its limit";
        group.done = true;
        for (const state of group.streams) {{
          state.error = group.error;
          state.done = true;
          state.queued = [];
          state.queueSizes = [];
          state.queueSize = 0;
          if (state.pendingRead) {{
            const pending = clearReadablePendingRead(state);
            pending.reject(new Error(group.error));
          }}
        }}
        rejectFetchStreamWaiters(group);
        return null;
      }}
      group.chunks.push(bytes.slice());
      group.totalBytes += bytes.length;
      for (const state of group.streams) {{
        if (state.cancelled || state.consumedByResponse) continue;
        state.done = false;
        if (state.pendingRead) {{
          resolveReadablePending(state, new Uint8Array(bytes), false);
        }} else {{
          const queued = new Uint8Array(bytes.slice());
          state.queued.push(queued);
          state.queueSizes.push(queued.byteLength);
          state.queueSize += queued.byteLength;
        }}
      }}
      if (group.waiters.length > 0) requestFetchStreamRead(group);
    }} else if (type === "end") {{
      group.done = true;
      for (const state of group.streams) {{
        state.done = true;
        if (state.pendingRead) resolveReadablePending(state, undefined, true);
        settleReadableStreamClosed(state);
      }}
      settleFetchStreamWaiters(group);
    }} else if (type === "error") {{
      group.error = String(payload.message || "native fetch response stream failed");
      group.done = true;
      for (const state of group.streams) {{
        state.error = group.error;
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) {{
          const pending = clearReadablePendingRead(state);
          pending.reject(new Error(group.error));
        }}
        settleReadableStreamClosed(state);
      }}
      rejectFetchStreamWaiters(group);
    }}
    return null;
  }};
  globalThis.__glassFetchStreamGroups = fetchStreamGroups;
  globalThis.__glassReadableStreamConstructor = ReadableStreamNative;
  globalThis.ReadableStream = ReadableStreamNative;
  const responseBodyBytes = (payload) => typeof payload.bodyBase64 === "string"
    ? decodeBase64(payload.bodyBase64)
    : blobUtf8Bytes(String(payload.body || ""));
  const responseBodyBlob = (payload) => {{
    const blob = new BlobNative([], {{ type: payload.contentType || "" }});
    blob._bytes = responseBodyBytes(payload);
    blob._text = payload.body === undefined
      ? utf8TextFromBytes(blob._bytes)
      : String(payload.body);
    blob.size = blob._bytes.length;
    return blob;
  }};
  const responseBodyBlobFromBytes = (bytes, contentType) => {{
    const blob = new BlobNative([], {{ type: contentType || "" }});
    blob._bytes = bytes.slice();
    blob._text = utf8TextFromBytes(blob._bytes);
    blob.size = blob._bytes.length;
    return blob;
  }};
  const nativeRequestBodyPayload = (settings) => {{
    const input = settings.body;
    if (input === undefined || input === null) return {{ bodyNull: true, bytes: [], stream: null, contentType: null }};
    if (input.__glassReadableStream === true) {{
      const state = readableStreamState(input);
      if (state.locked || state.disturbed) throw new TypeError("native Request body stream is unusable");
      return {{ bodyNull: false, bytes: null, stream: input, contentType: null }};
    }}
    if (input.__glassFormData === true) {{
      const serialized = serializeFormData(input, nextFetchRequestId);
      const bytes = serialized.bodyBase64 === null
        ? blobUtf8Bytes(serialized.body)
        : decodeBase64(serialized.bodyBase64, nativeFormBodyLimit);
      return {{ bodyNull: false, bytes, stream: null, contentType: serialized.contentType }};
    }}
    if (input.__glassUrlSearchParams === true) {{
      const text = input.toString();
      return {{ bodyNull: false, bytes: blobUtf8Bytes(text), stream: null, contentType: "application/x-www-form-urlencoded;charset=UTF-8" }};
    }}
    if (input.__glassNativeBlob === true)
      return {{ bodyNull: false, bytes: blobBytes(input), stream: null, contentType: input.type || null }};
    if (input instanceof ArrayBuffer)
      return {{ bodyNull: false, bytes: Array.from(new Uint8Array(input)), stream: null, contentType: null }};
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(input))
      return {{ bodyNull: false, bytes: Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength)), stream: null, contentType: null }};
    return {{ bodyNull: false, bytes: blobUtf8Bytes(String(input)), stream: null, contentType: null }};
  }};
  const nativeMultipartParameter = (value, parameter) => {{
    for (const piece of String(value).split(";")) {{
      const separator = piece.indexOf("=");
      if (separator < 0 || piece.slice(0, separator).trim().toLowerCase() !== parameter) continue;
      let result = piece.slice(separator + 1).trim();
      if (result.startsWith('"') && result.endsWith('"')) result = result.slice(1, -1);
      return result.replace(/\\"/g, '"').replace(/\\\\/g, "\\");
    }}
    return null;
  }};
  const nativeRequestFormData = (bytes, payload, request) => {{
    if (bytes.length > nativeFormBodyLimit) throw new RangeError("native Request FormData body limit exceeded");
    const declaredType = request.headers.get("content-type") || payload.contentType || "";
    const mediaType = String(declaredType).split(";", 1)[0].trim().toLowerCase();
    if (mediaType === "application/x-www-form-urlencoded") {{
      const form = new FormDataNative();
      const params = new URLSearchParamsNative(utf8TextFromBytes(bytes));
      for (const entry of params._entries) form.append(entry[0], entry[1]);
      return form;
    }}
    if (mediaType !== "multipart/form-data")
      throw new TypeError("native Request body is not FormData-compatible");
    const boundaryMatch = String(declaredType).match(/boundary=(?:"([^"]+)"|([^;\s]+))/i);
    const boundary = boundaryMatch ? (boundaryMatch[1] || boundaryMatch[2]) : null;
    if (!boundary || boundary.length > 256) throw new TypeError("native multipart boundary is invalid");
    const source = (() => {{
      let text = "";
      for (let offset = 0; offset < bytes.length; offset += 4096)
        text += String.fromCharCode(...bytes.slice(offset, Math.min(offset + 4096, bytes.length)));
      return text;
    }})();
    const marker = "--" + boundary;
    if (!source.startsWith(marker)) throw new TypeError("native multipart body is malformed");
    const form = new FormDataNative();
    let offset = marker.length;
    let entryCount = 0;
    if (source.startsWith("--", offset)) return form;
    if (source.slice(offset, offset + 2) !== "\r\n") throw new TypeError("native multipart body is malformed");
    offset += 2;
    while (offset < source.length) {{
      const headerEnd = source.indexOf("\r\n\r\n", offset);
      if (headerEnd < 0) throw new TypeError("native multipart headers are malformed");
      const headerValues = {{}};
      for (const line of source.slice(offset, headerEnd).split("\r\n")) {{
        const separator = line.indexOf(":");
        if (separator < 0) continue;
        headerValues[line.slice(0, separator).trim().toLowerCase()] = line.slice(separator + 1).trim();
      }}
      const disposition = headerValues["content-disposition"] || "";
      if (disposition.toLowerCase().split(";", 1)[0].trim() !== "form-data")
        throw new TypeError("native multipart disposition is unsupported");
      const name = nativeMultipartParameter(disposition, "name");
      if (name === null || name.length > storageKeyLimit) throw new TypeError("native multipart field name is invalid");
      const bodyStart = headerEnd + 4;
      const nextBoundary = source.indexOf("\r\n" + marker, bodyStart);
      if (nextBoundary < 0) throw new TypeError("native multipart boundary is missing");
      const bodyEnd = nextBoundary;
      const filename = nativeMultipartParameter(disposition, "filename");
      const fieldBytes = bytes.slice(bodyStart, bodyEnd);
      entryCount += 1;
      if (entryCount > storageEntryLimit) throw new RangeError("native Request FormData entry limit exceeded");
      if (filename === null) form.append(name, utf8TextFromBytes(fieldBytes));
      else {{
        const file = new FileNative([], filename, {{ type: headerValues["content-type"] || "" }});
        file._bytes = fieldBytes;
        file._text = utf8TextFromBytes(fieldBytes);
        file.size = fieldBytes.length;
        form.append(name, file);
      }}
      offset = nextBoundary + 2 + marker.length;
      if (source.startsWith("--", offset)) return form;
      if (source.slice(offset, offset + 2) !== "\r\n") throw new TypeError("native multipart body is malformed");
      offset += 2;
    }}
    throw new TypeError("native multipart body is incomplete");
  }};
  const nativeRequestBodyIsUsed = (request) => request.__glassRequestBodyState.used === true
    || Boolean(request.body && readableStreamState(request.body).disturbed);
  const nativeRequestBodyUse = (request) => {{
    const body = request.body;
    if (nativeRequestBodyIsUsed(request) || (body && readableStreamState(body).locked)) return false;
    request.__glassRequestBodyState.used = true;
    if (body) {{
      const state = readableStreamState(body);
      state.consumedByRequest = true;
      markReadableStreamDisturbed(state);
    }}
    return true;
  }};
  const nativeRequestBodyUnusable = () => Promise.reject(new TypeError("native Request body is unusable"));
  const nativeRequestBodyPromise = (request, transform) => {{
    if (!nativeRequestBodyUse(request)) return nativeRequestBodyUnusable();
    const payload = request.__glassRequestBodyPayload;
    if (payload.stream)
      return nativeDrainReadableStream(payload.stream, true).then(bytes => transform(bytes, payload));
    try {{
      return Promise.resolve(transform(payload.bytes.slice(), payload));
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  const nativeReadableStreamChunkBytes = (value) => {{
    if (typeof value === "string") return blobUtf8Bytes(value);
    if (value instanceof ArrayBuffer) return Array.from(new Uint8Array(value));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value))
      return Array.from(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
    throw new TypeError("native Request stream chunks must be byte data");
  }};
  const nativeDrainReadableStream = (stream, claimed, responseBody) => {{
    const state = readableStreamState(stream);
    if (!claimed && (state.locked || state.disturbed))
      return Promise.reject(new TypeError("native Request body stream is unusable"));
    if (responseBody) {{
      state.consumedByResponse = true;
      state.consumingResponseBody = true;
    }} else {{
      state.consumedByRequest = true;
      state.consumingRequestBody = true;
    }}
    if (!claimed) markReadableStreamDisturbed(state);
    let reader;
    try {{ reader = stream.getReader(); }}
    catch (error) {{
      state.consumingRequestBody = false;
      return Promise.reject(error);
    }}
    const chunks = [];
    let totalBytes = 0;
    const release = () => {{
      if (responseBody) state.consumingResponseBody = false;
      else state.consumingRequestBody = false;
      reader.releaseLock();
    }};
    const readNext = () => reader.read().then(
      result => {{
        if (result.done) {{
          release();
          const bytes = [];
          for (const chunk of chunks) bytes.push(...chunk);
          return bytes;
        }}
        let bytes;
        try {{ bytes = nativeReadableStreamChunkBytes(result.value); }}
        catch (error) {{ release(); throw error; }}
        totalBytes += bytes.length;
        if (totalBytes > nativeFormBodyLimit) {{
          release();
          throw new RangeError("native Request stream body exceeds its limit");
        }}
        chunks.push(bytes);
        return readNext();
      }},
      error => {{ release(); throw error; }},
    );
    return readNext();
  }};
  const fetchNative = (input, options) => {{
    const sourceRequest = input && input.__glassRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    if (typeof input !== "string" && !sourceRequest && !sourceUrl)
      throw new TypeError("native fetch requires a URL string, URL, or Request");
    const hasBodyOverride = options && typeof options === "object" && Object.prototype.hasOwnProperty.call(options, "body");
    const sourceBody = sourceRequest && !hasBodyOverride ? sourceRequest.body : null;
    if (sourceBody && sourceBody.__glassReadableStream === true) {{
      if (!nativeRequestBodyUse(sourceRequest)) return Promise.reject(new TypeError("native Request body is unusable"));
      return nativeDrainReadableStream(sourceBody, true)
        .then(bytes => fetchNativeDispatch(input, options, bytes));
    }}
    const settings = Object.assign(
      {{}},
      sourceRequest ? sourceRequest._settings : {{}},
      options && typeof options === "object" ? options : {{}},
    );
    const body = settings.body;
    if (body && body.__glassReadableStream === true)
      return nativeDrainReadableStream(body, false)
        .then(bytes => fetchNativeDispatch(input, options, bytes));
    return fetchNativeDispatch(input, options);
  }};
  RequestNative.prototype.text = function() {{
    return nativeRequestBodyPromise(this, bytes => utf8TextFromBytes(bytes));
  }};
  RequestNative.prototype.json = function() {{
    return nativeRequestBodyPromise(this, bytes => JSON.parse(utf8TextFromBytes(bytes)));
  }};
  RequestNative.prototype.blob = function() {{
    return nativeRequestBodyPromise(this, (bytes, payload) => responseBodyBlobFromBytes(
      bytes,
      payload.contentType || this.headers.get("content-type") || "",
    ));
  }};
  RequestNative.prototype.arrayBuffer = function() {{
    return nativeRequestBodyPromise(this, bytes => new Uint8Array(bytes).buffer);
  }};
  RequestNative.prototype.bytes = function() {{
    return nativeRequestBodyPromise(this, bytes => new Uint8Array(bytes));
  }};
  RequestNative.prototype.formData = function() {{
    return nativeRequestBodyPromise(this, (bytes, payload) => nativeRequestFormData(bytes, payload, this));
  }};
  const responseInitEntries = (input) => {{
    if (input === undefined || input === null) return [];
    if (input.__glassHeaders === true) return input._entries.map(entry => [entry[0], entry[1]]);
    if (Array.isArray(input)) return input.map(entry => {{
      if (!Array.isArray(entry) || entry.length !== 2) throw new TypeError("native Response header pairs must contain two values");
      return [entry[0], entry[1]];
    }});
    if (typeof input !== "object") throw new TypeError("native Response headers must be an object or pairs");
    return Object.keys(input).map(name => [name, input[name]]);
  }};
  const responseConstructorPayload = (body, options) => {{
    const settings = options && typeof options === "object" ? options : {{}};
    const bodyNull = body === undefined || body === null;
    const bodyPayload = bodyNull
      ? {{ body: "", bodyBase64: encodeBase64([]), contentType: null }}
      : body && body.__glassReadableStream === true
        ? {{ body: "", bodyBase64: null, contentType: null, bodyStream: body }}
      : body && body.__glassNativeBlob === true
        ? {{ body: body._text, bodyBase64: encodeBase64(blobBytes(body)), contentType: body.type || null }}
        : body && body.__glassUrlSearchParams === true
          ? {{ body: body.toString(), bodyBase64: encodeBase64(blobUtf8Bytes(body.toString())), contentType: "application/x-www-form-urlencoded;charset=UTF-8" }}
          : body instanceof ArrayBuffer
            ? {{ body: utf8TextFromBytes(Array.from(new Uint8Array(body))), bodyBase64: encodeBase64(Array.from(new Uint8Array(body))), contentType: null }}
            : typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(body)
              ? {{ body: utf8TextFromBytes(Array.from(new Uint8Array(body.buffer, body.byteOffset, body.byteLength))), bodyBase64: encodeBase64(Array.from(new Uint8Array(body.buffer, body.byteOffset, body.byteLength))), contentType: null }}
              : {{ body: String(body), bodyBase64: encodeBase64(blobUtf8Bytes(String(body))), contentType: null }};
    const status = settings.status === undefined ? 200 : Number(settings.status);
    if (!Number.isInteger(status) || status < 200 || status > 599) throw new RangeError("native Response status is outside the bounded range");
    const headers = responseInitEntries(settings.headers);
    return {{
      url: "",
      status,
      statusText: settings.statusText === undefined ? "" : String(settings.statusText),
      headers,
      contentType: bodyPayload.contentType,
      body: bodyPayload.body,
      bodyBase64: bodyPayload.bodyBase64,
      bodyStream: bodyPayload.bodyStream || null,
      bodyNull,
      redirected: false,
      opaque: false,
      opaqueRedirect: false,
    }};
  }};
  const ResponseNative = function(body, options) {{
    if (!(this instanceof ResponseNative)) throw new TypeError("native Response requires new");
    if (body && body.__glassReadableStream === true) {{
      const state = readableStreamState(body);
      if (state.locked || state.disturbed) throw new TypeError("native Response body stream is unusable");
    }}
    return responseFromFetch(responseConstructorPayload(body, options));
  }};
  ResponseNative.prototype.constructor = ResponseNative;
  ResponseNative.json = function(data, options) {{
    const settings = options && typeof options === "object" ? Object.assign({{}}, options) : {{}};
    const headers = new HeadersNative(settings.headers);
    if (!headers.has("content-type")) headers.set("content-type", "application/json");
    settings.headers = headers;
    return new ResponseNative(JSON.stringify(data), settings);
  }};
  ResponseNative.error = function() {{
    const response = Object.create(ResponseNative.prototype);
    Object.assign(response, {{
      type: "error",
      ok: false,
      status: 0,
      statusText: "",
      url: "",
      redirected: false,
      headers: responseHeaders([], null),
      body: null,
      clone() {{ return ResponseNative.error(); }},
      text() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      json() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      blob() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      arrayBuffer() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      bytes() {{ return Promise.reject(nativeOpaqueResponseError()); }},
    }});
    defineResponseBodyState(response);
    return Object.freeze(response);
  }};
  ResponseNative.redirect = function(url, status) {{
    const code = status === undefined ? 302 : Number(status);
    if (![301, 302, 303, 307, 308].includes(code)) throw new RangeError("native Response redirect status is unsupported");
    const location = url && url.__glassUrl === true ? url.href : String(url);
    return responseFromFetch({{
      url: "",
      status: code,
      statusText: "",
      headers: [["location", location]],
      contentType: null,
      body: "",
      bodyBase64: encodeBase64([]),
      bodyNull: true,
      redirected: false,
      opaque: false,
      opaqueRedirect: false,
    }});
  }};
  globalThis.Response = ResponseNative;
  const responseFromFetch = (payload) => {{
    const opaque = payload && payload.opaque === true;
    const opaqueRedirect = payload && payload.opaqueRedirect === true;
    const error = payload && payload.error === true;
    const filtered = opaque || opaqueRedirect || error;
    const bodyNull = payload && payload.bodyNull === true;
    const bodyStream = payload && payload.bodyStream && payload.bodyStream.__glassReadableStream === true
      ? payload.bodyStream
      : null;
    const streamId = payload && Number.isSafeInteger(Number(payload.bodyStreamId))
      ? Number(payload.bodyStreamId)
      : null;
    const opaqueBody = () => Promise.reject(nativeOpaqueResponseError());
    const responseBodyPromise = (response) => {{
      if (!responseBodyUse(response)) return responseBodyUnusable();
      if (response.__glassResponseStreamBody)
        return nativeDrainReadableStream(response.body, true, true);
      if (response.__glassFetchStreamId === null)
        return Promise.resolve(responseBodyBytes(response.__glassPayload));
      const group = fetchStreamGroup(response.__glassFetchStreamId);
      if (group.error !== null) return Promise.reject(new Error(group.error));
      if (group.done) return Promise.resolve(flattenFetchStream(group));
      return new Promise((resolve, reject) => {{
        group.waiters.push({{ resolve, reject }});
        try {{ requestFetchStreamRead(group); }}
        catch (error) {{ group.waiters.pop(); reject(error); }}
      }});
    }};
    const response = Object.create(ResponseNative.prototype);
    Object.assign(response, {{
      type: error ? "error" : opaqueRedirect ? "opaqueredirect" : (opaque ? "opaque" : "basic"),
      ok: !filtered && payload.status >= 200 && payload.status < 300,
      status: filtered ? 0 : payload.status,
      statusText: filtered ? "" : (payload.statusText === undefined ? String(payload.status) : String(payload.statusText)),
      url: filtered ? "" : payload.url,
      redirected: opaqueRedirect ? false : payload.redirected === true,
      headers: filtered ? responseHeaders([], null) : responseHeaders(payload.headers, payload.contentType),
      body: filtered || bodyNull
        ? null
        : bodyStream
          ? bodyStream
        : streamId === null
          ? new ReadableStreamNative(responseBodyBytes(payload))
          : new ReadableStreamNative([], streamId),
      __glassPayload: payload,
      __glassFetchStreamId: streamId,
      __glassResponseStreamBody: bodyStream !== null,
      clone() {{
        const body = this.body;
        if (body && (responseBodyIsUsed(this) || readableStreamState(body).locked))
          throw new TypeError("native Response body is unusable");
        if (this.__glassResponseStreamBody) {{
          const [sourceBody, cloneBody] = body.tee();
          this.__glassPayload.bodyStream = sourceBody;
          this.__glassResponseBodyState.stream = sourceBody;
          return responseFromFetch(Object.assign({{}}, this.__glassPayload, {{ bodyStream: cloneBody }}));
        }}
        return responseFromFetch(payload);
      }},
      text() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => utf8TextFromBytes(bytes)); }},
      json() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => JSON.parse(utf8TextFromBytes(bytes))); }},
      blob() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => responseBodyBlobFromBytes(bytes, payload.contentType)); }},
      arrayBuffer() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => responseBodyBlobFromBytes(bytes, payload.contentType).arrayBuffer()); }},
      bytes() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => new Uint8Array(bytes)); }},
    }});
    if (bodyStream) {{
      const responseBodyState = {{ stream: bodyStream }};
      Object.defineProperty(response, "__glassResponseBodyState", {{ value: responseBodyState }});
      Object.defineProperty(response, "body", {{
        configurable: true,
        enumerable: true,
        get() {{ return responseBodyState.stream; }},
      }});
    }}
    defineResponseBodyState(response);
    if (streamId !== null && !filtered && !bodyNull) {{
      const group = fetchStreamGroup(streamId);
      if (!group.started) {{
        group.started = true;
        requestFetchStreamRead(group);
      }}
    }}
    return Object.freeze(response);
  }};
  const XMLHttpRequestNative = function() {{
    this.readyState = 0;
    this.status = 0;
    this.statusText = "";
    this.responseText = "";
    this.responseURL = "";
    this.response = "";
    this.responseType = "";
    this.withCredentials = false;
    this.onreadystatechange = null;
    this.onload = null;
    this.onerror = null;
    this.onabort = null;
    this.ontimeout = null;
    this._method = "GET";
    this._url = "";
    this._headers = {{}};
    this._responseContentType = null;
    this._responseHeaders = responseHeaders([], null);
    this._controller = null;
    this._aborted = false;
    this._timeout = 0;
  }};
  XMLHttpRequestNative.prototype._notifyReadyState = function() {{
    if (typeof this.onreadystatechange === "function") this.onreadystatechange.call(this);
  }};
  Object.defineProperty(XMLHttpRequestNative.prototype, "timeout", {{
    get() {{ return this._timeout; }},
    set(value) {{
      const numeric = Number(value);
      if (!Number.isFinite(numeric) || numeric < 0 || numeric > {max_native_xhr_timeout_ms})
        throw new RangeError("native XMLHttpRequest timeout is outside the bounded range");
      this._timeout = Math.trunc(numeric);
    }},
  }});
  XMLHttpRequestNative.prototype.open = function(method, url, async) {{
    if (async === false) throw new TypeError("native XMLHttpRequest requires async mode");
    const normalizedMethod = String(method).toUpperCase();
    if (!nativeRequestMethods.includes(normalizedMethod))
      throw new TypeError("native XMLHttpRequest method is unsupported");
    if (typeof url !== "string") throw new TypeError("native XMLHttpRequest URL must be text");
    this._method = normalizedMethod;
    this._url = url;
    this._headers = {{}};
    this._controller = null;
    this._aborted = false;
    this.readyState = 1;
    this._notifyReadyState();
  }};
  XMLHttpRequestNative.prototype.setRequestHeader = function(name, value) {{
    if (String(name).toLowerCase() !== "content-type")
      throw new TypeError("native XMLHttpRequest only supports the Content-Type header");
    this._headers["Content-Type"] = String(value);
  }};
  XMLHttpRequestNative.prototype.abort = function() {{
    const active = this.readyState !== 0 && this.readyState !== 4;
    const controller = this._controller;
    this._aborted = true;
    this._controller = null;
    if (controller) controller.abort();
    if (!active) return;
    this.readyState = 0;
    this.status = 0;
    this.statusText = "";
    this.responseText = "";
    this.responseURL = "";
    this.response = "";
    this._responseContentType = null;
    this._responseHeaders = responseHeaders([], null);
    this._notifyReadyState();
    if (typeof this.onabort === "function") this.onabort.call(this, {{ type: "abort", target: this }});
  }};
  XMLHttpRequestNative.prototype.getResponseHeader = function(name) {{
    return this._responseHeaders.get(name);
  }};
  XMLHttpRequestNative.prototype.getAllResponseHeaders = function() {{
    return Array.from(this._responseHeaders.entries())
      .map(entry => entry[0] + ": " + entry[1] + "\r\n")
      .join("");
  }};
  XMLHttpRequestNative.prototype.send = function(body) {{
    if (this.readyState !== 1) throw new TypeError("native XMLHttpRequest is not open");
    const responseType = String(this.responseType || "").toLowerCase();
    if (!["", "text", "arraybuffer", "blob"].includes(responseType)) throw new TypeError("native XMLHttpRequest responseType is unsupported");
    const requestBody = body && (body.__glassFormData === true || body.__glassUrlSearchParams === true || body.__glassNativeBlob === true)
      ? body
      : body instanceof ArrayBuffer || (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(body))
        ? body
        : body === undefined || body === null ? null : String(body);
    const controller = new AbortControllerNative();
    this._controller = controller;
    this._aborted = false;
    const request = fetchNative(this._url, {{
      method: this._method,
      body: requestBody,
      headers: this._headers,
      credentials: this.withCredentials ? "include" : "omit",
      signal: controller.signal,
      __glassTimeoutMs: this._timeout,
    }});
    request.then(response => {{
      if (this._controller !== controller || this._aborted) return null;
      this.status = response.status;
      this.statusText = String(response.status);
      this.responseURL = response.url;
      this._responseContentType = response.headers.get("content-type");
      this._responseHeaders = response.headers;
      if (responseType === "arraybuffer") return response.arrayBuffer();
      if (responseType === "blob") return response.blob();
      return response.text();
    }}).then(value => {{
      if (value === null || this._controller !== controller || this._aborted) return;
      this._controller = null;
      this.responseText = typeof value === "string" ? value : "";
      this.response = value;
      this.readyState = 4;
      this._notifyReadyState();
      if (typeof this.onload === "function") this.onload.call(this, {{ type: "load", target: this }});
    }}).catch(error => {{
      if (this._controller !== controller || this._aborted) return;
      this._controller = null;
      if (error && error.name === "TimeoutError") {{
        this.status = 0;
        this.statusText = "";
        this.responseText = "";
        this.responseURL = "";
        this.response = "";
        this._responseContentType = null;
        this._responseHeaders = responseHeaders([], null);
        this.readyState = 4;
        this._notifyReadyState();
        if (typeof this.ontimeout === "function") this.ontimeout.call(this, {{ type: "timeout", target: this }});
        return;
      }}
      this.readyState = 4;
      this._notifyReadyState();
      if (typeof this.onerror === "function") this.onerror.call(this, {{ type: "error", target: this, error }});
    }});
  }};
  globalThis.XMLHttpRequest = XMLHttpRequestNative;
  globalThis.__glassFetchRequests = fetchRequests;
  globalThis.__glassNextFetchRequestId = nextFetchRequestId;
  globalThis.fetch = fetchNative;
  const websocketMessageLimit = {websocket_message_limit};
  const websocketProtocolLimit = {websocket_protocol_limit};
  const websocketProtocolCountLimit = {websocket_protocol_count_limit};
  const websocketCloseReasonLimit = {websocket_close_reason_limit};
  const websocketSockets = globalThis.__glassWebSocketSockets instanceof Map
    ? globalThis.__glassWebSocketSockets
    : new Map();
  let nextWebSocketId = Number.isSafeInteger(globalThis.__glassNextWebSocketId)
    ? globalThis.__glassNextWebSocketId
    : 1;
  const websocketProtocolToken = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const normalizeWebSocketProtocols = (value) => {{
    if (value === undefined) return [];
    const values = typeof value === "string"
      ? [value]
      : value && typeof value[Symbol.iterator] === "function"
        ? Array.from(value)
        : (() => {{ throw new TypeError("native WebSocket protocols must be a string or iterable"); }})();
    if (values.length > websocketProtocolCountLimit)
      throw new RangeError("native WebSocket protocol count limit exceeded");
    const seen = new Set();
    return values.map((protocol) => {{
      const text = String(protocol);
      if (!text || text.length > websocketProtocolLimit || !websocketProtocolToken.test(text) || seen.has(text))
        throw new SyntaxError("native WebSocket protocol is invalid or duplicated");
      seen.add(text);
      return text;
    }});
  }};
  const websocketDispatch = (socket, type, event) => {{
    const listeners = socket.__glassWebSocketListeners[type]
      ? socket.__glassWebSocketListeners[type].slice()
      : [];
    if (typeof socket["on" + type] === "function") {{
      try {{ socket["on" + type].call(socket, event); }} catch (_) {{}}
    }}
    for (const listener of listeners) {{
      try {{ listener.call(socket, event); }} catch (_) {{}}
    }}
  }};
  globalThis.__glassWebSocketSockets = websocketSockets;
  globalThis.__glassNextWebSocketId = nextWebSocketId;
  globalThis.__glassDispatchWebSocketEvent = (socketId, payload) => {{
    const socket = websocketSockets.get(Number(socketId));
    if (!socket || !payload || typeof payload !== "object") return null;
    const type = String(payload.type || "");
    if (!["open", "message", "error", "close"].includes(type)) return null;
    if (type === "open") {{
      socket.readyState = WebSocketNative.OPEN;
      socket.protocol = String(payload.protocol || "");
      websocketDispatch(socket, "open", {{ type: "open", target: socket, currentTarget: socket }});
    }} else if (type === "message") {{
      let data = String(payload.data || "");
      if (payload.binary === true) {{
        const bytes = decodeBase64(String(payload.dataBase64 || ""), websocketMessageLimit);
        if (socket.binaryType === "arraybuffer") data = new Uint8Array(bytes).buffer;
        else {{
          const blob = new BlobNative([], {{ type: "application/octet-stream" }});
          blob._bytes = bytes;
          blob._text = utf8TextFromBytes(bytes);
          blob.size = bytes.length;
          data = blob;
        }}
      }}
      websocketDispatch(socket, "message", {{ type: "message", data, origin: String(payload.origin || ""), target: socket, currentTarget: socket }});
    }} else if (type === "error") {{
      websocketDispatch(socket, "error", {{ type: "error", message: String(payload.message || ""), target: socket, currentTarget: socket }});
    }} else {{
      socket.readyState = WebSocketNative.CLOSED;
      websocketDispatch(socket, "close", {{
        type: "close",
        code: Number(payload.code) || 1006,
        reason: String(payload.reason || ""),
        wasClean: payload.wasClean === true,
        target: socket,
        currentTarget: socket,
      }});
      websocketSockets.delete(Number(socketId));
    }}
    return null;
  }};
  const WebSocketNative = function(input, protocols) {{
    if (!(this instanceof WebSocketNative)) throw new TypeError("native WebSocket requires new");
    const source = input && input.__glassUrl === true ? input.href : input;
    const resolved = new URLNative(String(source), host.url);
    if (!["ws:", "wss:"].includes(resolved.protocol) || resolved.username || resolved.password || !resolved.host)
      throw new SyntaxError("native WebSocket URL must use ws or wss without credentials");
    const normalizedProtocols = normalizeWebSocketProtocols(protocols);
    const socketId = nextWebSocketId;
    nextWebSocketId += 1;
    globalThis.__glassNextWebSocketId = nextWebSocketId;
    this.url = resolved.href;
    this.readyState = WebSocketNative.CONNECTING;
    this.bufferedAmount = 0;
    this.extensions = "";
    this.protocol = "";
    this.binaryType = "blob";
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.onclose = null;
    this.__glassSocketId = socketId;
    this.__glassWebSocketListeners = {{ open: [], message: [], error: [], close: [] }};
    websocketSockets.set(socketId, this);
    pushCommand({{ kind: "webSocketOpen", socket_id: socketId, href: resolved.href, protocols: normalizedProtocols }});
  }};
  WebSocketNative.CONNECTING = 0;
  WebSocketNative.OPEN = 1;
  WebSocketNative.CLOSING = 2;
  WebSocketNative.CLOSED = 3;
  WebSocketNative.prototype.addEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWebSocketListeners[name] || typeof listener !== "function") return;
    if (!this.__glassWebSocketListeners[name].includes(listener)) this.__glassWebSocketListeners[name].push(listener);
  }};
  WebSocketNative.prototype.removeEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWebSocketListeners[name]) return;
    this.__glassWebSocketListeners[name] = this.__glassWebSocketListeners[name].filter(candidate => candidate !== listener);
  }};
  WebSocketNative.prototype.dispatchEvent = function(event) {{
    if (!event || !event.type) throw new TypeError("native WebSocket event is invalid");
    websocketDispatch(this, String(event.type), event);
    return true;
  }};
  WebSocketNative.prototype.send = function(data) {{
    if (this.readyState !== WebSocketNative.OPEN) throw new Error("native WebSocket is not open");
    if (typeof data === "string") {{
      if (data.length > websocketMessageLimit) throw new RangeError("native WebSocket message exceeds its limit");
      pushCommand({{ kind: "webSocketSend", socket_id: this.__glassSocketId, data, data_base64: null }});
      return;
    }}
    let bytes = null;
    if (data && data.__glassNativeBlob === true) bytes = blobBytes(data);
    else if (data instanceof ArrayBuffer) bytes = Array.from(new Uint8Array(data));
    else if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(data))
      bytes = Array.from(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    if (!bytes) throw new TypeError("native WebSocket data must be text, Blob, or an ArrayBuffer view");
    pushCommand({{ kind: "webSocketSend", socket_id: this.__glassSocketId, data: null, data_base64: encodeBase64(bytes, websocketMessageLimit) }});
  }};
  WebSocketNative.prototype.close = function(code, reason) {{
    if (this.readyState === WebSocketNative.CLOSING || this.readyState === WebSocketNative.CLOSED) return;
    const normalizedCode = code === undefined ? 1000 : Number(code);
    if (!Number.isInteger(normalizedCode) || (normalizedCode !== 1000 && (normalizedCode < 3000 || normalizedCode > 4999)))
      throw new RangeError("native WebSocket close code is invalid");
    const normalizedReason = reason === undefined ? "" : String(reason);
    if (normalizedReason.length > websocketCloseReasonLimit)
      throw new SyntaxError("native WebSocket close reason exceeds its limit");
    this.readyState = WebSocketNative.CLOSING;
    pushCommand({{ kind: "webSocketClose", socket_id: this.__glassSocketId, code: normalizedCode, reason: normalizedReason }});
  }};
  Object.defineProperties(WebSocketNative.prototype, {{
    CONNECTING: {{ value: 0 }}, OPEN: {{ value: 1 }}, CLOSING: {{ value: 2 }}, CLOSED: {{ value: 3 }},
  }});
  globalThis.WebSocket = WebSocketNative;
  const eventSourceMessageLimit = {eventsource_message_limit};
  const eventSourceFieldLimit = {eventsource_field_limit};
  const eventSources = globalThis.__glassEventSources instanceof Map
    ? globalThis.__glassEventSources
    : new Map();
  let nextEventSourceId = Number.isSafeInteger(globalThis.__glassNextEventSourceId)
    ? globalThis.__glassNextEventSourceId
    : 1;
  const eventSourceDispatch = (source, type, event) => {{
    const listeners = source.__glassEventSourceListeners[type]
      ? source.__glassEventSourceListeners[type].slice()
      : [];
    const handler = source["on" + type];
    if (typeof handler === "function") {{
      try {{ handler.call(source, event); }} catch (_) {{}}
    }}
    for (const listener of listeners) {{
      try {{ listener.call(source, event); }} catch (_) {{}}
    }}
  }};
  globalThis.__glassEventSources = eventSources;
  globalThis.__glassNextEventSourceId = nextEventSourceId;
  globalThis.__glassDispatchEventSourceEvent = (sourceId, payload) => {{
    const source = eventSources.get(Number(sourceId));
    if (!source || !payload || typeof payload !== "object") return null;
    const type = String(payload.type || "");
    if (type === "open") {{
      source.readyState = EventSourceNative.OPEN;
      eventSourceDispatch(source, "open", {{ type: "open", target: source, currentTarget: source }});
    }} else if (type === "message") {{
      const data = String(payload.data || "");
      if (data.length > eventSourceMessageLimit) return null;
      const eventType = String(payload.event || "message");
      if (!eventType || eventType.length > eventSourceFieldLimit) return null;
      const event = {{
        type: eventType,
        data,
        origin: String(payload.origin || ""),
        lastEventId: String(payload.lastEventId || ""),
        target: source,
        currentTarget: source,
      }};
      eventSourceDispatch(source, eventType, event);
    }} else if (type === "error") {{
      source.readyState = EventSourceNative.CONNECTING;
      eventSourceDispatch(source, "error", {{ type: "error", message: String(payload.message || ""), target: source, currentTarget: source }});
    }} else if (type === "close") {{
      source.readyState = EventSourceNative.CLOSED;
      eventSourceDispatch(source, "close", {{ type: "close", target: source, currentTarget: source }});
      eventSources.delete(Number(sourceId));
    }}
    return null;
  }};
  const EventSourceNative = function(input, options) {{
    if (!(this instanceof EventSourceNative)) throw new TypeError("native EventSource requires new");
    const source = input && input.__glassUrl === true ? input.href : input;
    const resolved = new URLNative(String(source), host.url);
    if (!["http:", "https:"].includes(resolved.protocol) || resolved.username || resolved.password || !resolved.host)
      throw new SyntaxError("native EventSource URL must use http or https without credentials");
    if (options !== undefined && (options === null || typeof options !== "object"))
      throw new TypeError("native EventSource options must be an object");
    const withCredentials = options !== undefined && options.withCredentials === true;
    const sourceId = nextEventSourceId;
    nextEventSourceId += 1;
    globalThis.__glassNextEventSourceId = nextEventSourceId;
    this.url = resolved.href;
    this.readyState = EventSourceNative.CONNECTING;
    this.withCredentials = withCredentials;
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.__glassSourceId = sourceId;
    this.__glassEventSourceListeners = {{}};
    eventSources.set(sourceId, this);
    pushCommand({{ kind: "eventSourceOpen", source_id: sourceId, href: resolved.href, with_credentials: withCredentials }});
  }};
  EventSourceNative.CONNECTING = 0;
  EventSourceNative.OPEN = 1;
  EventSourceNative.CLOSED = 2;
  EventSourceNative.prototype.addEventListener = function(type, listener) {{
    const name = String(type);
    if (!name || name.length > eventSourceFieldLimit || typeof listener !== "function") return;
    if (!this.__glassEventSourceListeners[name]) this.__glassEventSourceListeners[name] = [];
    if (!this.__glassEventSourceListeners[name].includes(listener)) this.__glassEventSourceListeners[name].push(listener);
  }};
  EventSourceNative.prototype.removeEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassEventSourceListeners[name]) return;
    this.__glassEventSourceListeners[name] = this.__glassEventSourceListeners[name].filter(candidate => candidate !== listener);
  }};
  EventSourceNative.prototype.dispatchEvent = function(event) {{
    if (!event || !event.type) throw new TypeError("native EventSource event is invalid");
    eventSourceDispatch(this, String(event.type), event);
    return true;
  }};
  EventSourceNative.prototype.close = function() {{
    if (this.readyState === EventSourceNative.CLOSED) return;
    this.readyState = EventSourceNative.CLOSED;
    eventSources.delete(this.__glassSourceId);
    pushCommand({{ kind: "eventSourceClose", source_id: this.__glassSourceId }});
  }};
  Object.defineProperties(EventSourceNative.prototype, {{
    CONNECTING: {{ value: 0 }}, OPEN: {{ value: 1 }}, CLOSED: {{ value: 2 }},
  }});
  globalThis.EventSource = EventSourceNative;
  globalThis.__glassResolveFetch = (requestId, payload) => {{
    const pending = fetchRequests.get(Number(requestId));
    if (!pending) return;
    fetchRequests.delete(Number(requestId));
    clearFetchAbortListener(pending);
    if (payload && payload.error) {{
      pending.reject(payload.timeout === true ? nativeTimeoutError() : new Error(String(payload.error)));
    }}
    else pending.resolve(responseFromFetch(payload));
  }};
  globalThis.queueMicrotask = (callback) => {{
    if (typeof callback !== "function") throw new TypeError("microtask callback must be callable");
    Promise.resolve().then(callback);
  }};
  globalThis.__glassRunTimers = (currentNow) => {{
    const now = Number.isFinite(Number(currentNow)) ? Number(currentNow) : host.now_ms;
    const pending = Array.from(timers.entries())
      .filter(([, timer]) => Number(timer.dueAt === undefined ? 0 : timer.dueAt) <= now)
      .sort((left, right) => {{
        const due = Number(left[1].dueAt === undefined ? 0 : left[1].dueAt)
          - Number(right[1].dueAt === undefined ? 0 : right[1].dueAt);
        return due || left[0] - right[0];
    }});
    for (const [id, timer] of pending) {{
      if (!timers.has(id)) continue;
      timers.delete(id);
      runningTimers.set(id, timer);
      try {{
        timer.callback(...timer.args);
      }} finally {{
        runningTimers.delete(id);
        if (timer.intervalMs > 0 && !timer.cancelled) {{
          timer.dueAt = now + timer.intervalMs;
          timers.set(id, timer);
        }}
      }}
    }}
    const frames = Array.from(animationFrames.entries())
      .filter(([, frame]) => Number(frame.dueAt === undefined ? 0 : frame.dueAt) <= now)
      .sort((left, right) => {{
        const due = Number(left[1].dueAt === undefined ? 0 : left[1].dueAt)
          - Number(right[1].dueAt === undefined ? 0 : right[1].dueAt);
        return due || left[0] - right[0];
      }});
    for (const [id, frame] of frames) {{
      if (!animationFrames.has(id)) continue;
      animationFrames.delete(id);
      frame.callback.call(globalThis, now);
    }}
    const idles = Array.from(idleCallbacks.entries())
      .filter(([, idle]) => idle.timeoutAt === null || Number(idle.timeoutAt) <= now)
      .sort((left, right) => {{
        const leftAt = left[1].timeoutAt === null ? Number.MAX_SAFE_INTEGER : Number(left[1].timeoutAt);
        const rightAt = right[1].timeoutAt === null ? Number.MAX_SAFE_INTEGER : Number(right[1].timeoutAt);
        return leftAt - rightAt || left[0] - right[0];
      }});
    for (const [id, idle] of idles) {{
      if (!idleCallbacks.has(id)) continue;
      idleCallbacks.delete(id);
      const startedAt = performance.now();
      const didTimeout = idle.timeoutAt !== null && Number(idle.timeoutAt) <= now;
      const deadline = {{
        didTimeout,
        timeRemaining() {{
          return Math.max(0, 50 - (performance.now() - startedAt));
        }},
      }};
      idle.callback.call(globalThis, deadline);
    }}
  }};
  const listeners = globalThis.__glassHostListeners instanceof Map
    ? globalThis.__glassHostListeners
    : new Map();
  globalThis.__glassHostListeners = listeners;
  const normalizeEventType = (type) => {{
    const value = String(type).toLowerCase();
    if (!value || value.length > 128) throw new TypeError("invalid native event type");
    return value;
  }};
  const listenerKey = (owner, type) => String(owner) + ":" + normalizeEventType(type);
  const listenerCount = () => {{
    let count = 0;
    for (const callbacks of listeners.values()) count += callbacks.length;
    return count;
  }};
  const listenerOptions = (options) => {{
    if (options === true) return {{ capture: true, once: false }};
    if (!options || typeof options !== "object") return {{ capture: false, once: false }};
    return {{ capture: Boolean(options.capture), once: Boolean(options.once) }};
  }};
  const addListener = (owner, type, callback, options) => {{
    if (typeof callback !== "function") throw new TypeError("event listener must be callable");
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key) || [];
    const settings = listenerOptions(options);
    if (callbacks.some((record) => record.callback === callback && record.capture === settings.capture)) return;
    if (listenerCount() >= {max_listeners}) throw new RangeError("native event listener limit exceeded");
    callbacks.push({{ callback, capture: settings.capture, once: settings.once }});
    listeners.set(key, callbacks);
  }};
  const removeListener = (owner, type, callback, options) => {{
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key);
    if (!callbacks) return;
    const capture = listenerOptions(options).capture;
    const index = callbacks.findIndex((record) => record.callback === callback && record.capture === capture);
    if (index < 0) return;
    callbacks.splice(index, 1);
    if (callbacks.length === 0) listeners.delete(key);
  }};
  const installEventHandlerProperty = (element, type) => {{
    let handler = null;
    let registered = null;
    Object.defineProperty(element, "on" + type, {{
      enumerable: true,
      configurable: false,
      get() {{ return handler; }},
      set(next) {{
        if (registered) removeListener(ownerFor(element), type, registered, false);
        handler = typeof next === "function" ? next : null;
        registered = handler && element === globalThis && type === "error"
          ? event => handler.call(
              element,
              event.message || "",
              event.filename || "",
              Number(event.lineno) || 0,
              Number(event.colno) || 0,
              event.error || null,
            )
          : handler;
        if (registered) addListener(ownerFor(element), type, registered, false);
      }},
    }});
  }};
  const createEvent = (type, options) => {{
    const settings = options && typeof options === "object" ? options : {{}};
    const event = {{
      type: normalizeEventType(type),
      bubbles: Boolean(settings.bubbles),
      cancelable: Boolean(settings.cancelable),
      key: settings.key === undefined ? "" : String(settings.key),
      code: settings.code === undefined ? "" : String(settings.code),
      altKey: Boolean(settings.altKey),
      ctrlKey: Boolean(settings.ctrlKey),
      metaKey: Boolean(settings.metaKey),
      shiftKey: Boolean(settings.shiftKey),
      target: null,
      currentTarget: null,
      eventPhase: 0,
      defaultPrevented: false,
      returnValue: "",
      persisted: Boolean(settings.persisted),
      submitter: settings.submitter === undefined ? null : settings.submitter,
      state: settings.state === undefined ? null : settings.state,
      oldURL: settings.oldURL === undefined ? "" : String(settings.oldURL),
      newURL: settings.newURL === undefined ? "" : String(settings.newURL),
      preventDefault() {{
        if (this.cancelable) this.defaultPrevented = true;
      }},
      stopPropagation() {{ eventState.stopped = true; }},
      stopImmediatePropagation() {{
        eventState.stopped = true;
        eventState.immediate = true;
      }},
    }};
    const eventState = {{ stopped: false, immediate: false, dispatching: false }};
    Object.defineProperty(event, "__glassState", {{
      value: eventState,
      enumerable: false,
      configurable: false,
    }});
    if (typeof globalThis.Event === "function" && globalThis.Event.prototype) {{
      try {{ Object.setPrototypeOf(event, globalThis.Event.prototype); }} catch (_error) {{}}
    }}
    return event;
  }};
  const ownerFor = (target) => {{
    if (target && typeof target.__glassEventOwner === "string") return target.__glassEventOwner;
    if (target === globalThis) return "window";
    if (target === document) return "document";
    return "node:" + target.nodeIndex;
  }};
  const invokeListeners = (target, event, capture, phase) => {{
    const owner = ownerFor(target);
    const callbacks = (listeners.get(listenerKey(owner, event.type)) || []).slice();
    const eventState = event.__glassState;
    event.currentTarget = target;
    event.eventPhase = phase;
    for (const record of callbacks) {{
      if (record.capture !== capture) continue;
      record.callback.call(target, event);
      if (record.once) removeListener(owner, event.type, record.callback, capture);
      if (eventState.immediate) break;
    }}
  }};
  const dispatchTarget = (target, event) => {{
    if (!event || typeof event.type !== "string") throw new TypeError("invalid native event");
    normalizeEventType(event.type);
    const eventState = event.__glassState;
    if (!eventState || eventState.dispatching) throw new TypeError("event is already being dispatched");
    eventState.dispatching = true;
    event.target = target;
    const path = [target];
    const ownerDocument = target && target.nodeType === 9
      ? target
      : target && target.ownerDocument
        ? target.ownerDocument
        : null;
    const ownerWindow = ownerDocument && ownerDocument.defaultView
      ? ownerDocument.defaultView
      : globalThis;
    if (target !== ownerWindow && target !== ownerDocument && typeof target.nodeIndex === "number") {{
      let parent = target.parentNode || null;
      while (parent) {{
        path.push(parent);
        if (parent === ownerDocument) break;
        if (typeof parent.parentNode === "undefined") break;
        parent = parent.parentNode;
      }}
      if (path[path.length - 1] === ownerDocument && ownerWindow !== ownerDocument) {{
        path.push(ownerWindow);
      }}
    }} else if (target === ownerDocument && ownerWindow !== ownerDocument) {{
      path.push(ownerWindow);
    }}
    for (let index = path.length - 1; index > 0; index -= 1) {{
      invokeListeners(path[index], event, true, 1);
      if (eventState.stopped || eventState.immediate) break;
    }}
    if (!eventState.immediate) {{
      invokeListeners(target, event, true, 2);
      if (!eventState.immediate) invokeListeners(target, event, false, 2);
    }}
    if (event.bubbles && !eventState.stopped && !eventState.immediate) {{
      for (let index = 1; index < path.length; index += 1) {{
        invokeListeners(path[index], event, false, 3);
        if (eventState.stopped || eventState.immediate) break;
      }}
    }}
    if (event.type === "beforeunload" && event.returnValue !== "") event.defaultPrevented = true;
    event.currentTarget = null;
    event.eventPhase = 0;
    eventState.dispatching = false;
    return !event.defaultPrevented;
  }};
  const mutationObservers = globalThis.__glassMutationObservers instanceof Set
    ? globalThis.__glassMutationObservers
    : new Set();
  globalThis.__glassMutationObservers = mutationObservers;
  const mutationCreatedNodes = globalThis.__glassCreatedNodes instanceof Map
    ? globalThis.__glassCreatedNodes
    : new Map();
  mutationCreatedNodes.clear();
  globalThis.__glassCreatedNodes = mutationCreatedNodes;
  const scriptNodeObjects = globalThis.__glassScriptNodeObjects instanceof Map
    ? globalThis.__glassScriptNodeObjects
    : new Map();
  globalThis.__glassScriptNodeObjects = scriptNodeObjects;
  const scriptNodeAliasesByIndex = new Map();
  for (const identity of Array.isArray(state.scriptNodes) ? state.scriptNodes : []) {{
    const temporaryIndex = Number(identity && identity.temporaryIndex);
    const nodeIndex = Number(identity && identity.nodeIndex);
    const object = scriptNodeObjects.get(temporaryIndex);
    if (!object || !Number.isSafeInteger(nodeIndex) || nodeIndex < 0) continue;
    object.nodeIndex = nodeIndex;
    scriptNodeAliasesByIndex.set(nodeIndex, object);
  }}
  const mutationShadowAttributes = new Map();
  for (const entry of Array.isArray(state.elements) ? state.elements : []) {{
    mutationShadowAttributes.set(Number(entry.nodeIndex), {{ ...(entry.attributes || {{}}) }});
  }}
  const mutationShadowText = new Map();
  const mutationShadowParents = new Map();
  const mutationShadowChildren = new Map();
  for (const entry of Array.isArray(state.nodes) ? state.nodes : []) {{
    const index = Number(entry.nodeIndex);
    if ([3, 8].includes(Number(entry.nodeType))) mutationShadowText.set(index, String(entry.nodeValue || ""));
    if (entry.parentIndex !== null && entry.parentIndex !== undefined) {{
      mutationShadowParents.set(index, Number(entry.parentIndex));
    }}
    mutationShadowChildren.set(index, Array.isArray(entry.children)
      ? entry.children.map(Number)
      : []);
  }}
  const mutationNode = (nodeIndex) => {{
    const index = Number(nodeIndex);
    if (index === 0 && globalThis.__glassHostDocument) return globalThis.__glassHostDocument;
    const nodes = globalThis.__glassHostNodes;
    if (nodes instanceof Map && nodes.has(index)) return nodes.get(index);
    return mutationCreatedNodes.get(index) || null;
  }};
  const mutationParent = (node) => {{
    if (!node) return null;
    if (node.__glassParent) return node.__glassParent;
    if (node.__glassMutationDocument) return node.__glassMutationDocument;
    if (node.parentIndex !== null && node.parentIndex !== undefined) {{
      return mutationNode(node.parentIndex);
    }}
    const owner = globalThis.__glassHostDocument;
    return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(node)
      ? owner
      : null;
  }};
  const mutationMatchesTarget = (target, observed, subtree) => {{
    if (target === observed) return true;
    if (!subtree) return false;
    let current = mutationParent(target);
    for (let depth = 0; current && depth <= {max_commands}; depth += 1) {{
      if (current === observed) return true;
      current = mutationParent(current);
    }}
    return false;
  }};
  const mutationTextNode = (value) => {{
    const text = String(value);
    return {{
      nodeType: 3,
      nodeName: "#text",
      nodeValue: text,
      textContent: text,
      data: text,
      parentNode: null,
      parentElement: null,
    }};
  }};
  const mutationRecord = (record, options) => {{
    const type = String(record.type);
    return {{
      type,
      target: record.target,
      addedNodes: type === "childList" ? [...(record.addedNodes || [])] : [],
      removedNodes: type === "childList" ? [...(record.removedNodes || [])] : [],
      previousSibling: record.previousSibling || null,
      nextSibling: record.nextSibling || null,
      attributeName: type === "attributes" ? String(record.attributeName || "") : null,
      attributeNamespace: null,
      oldValue: (type === "attributes" || type === "characterData") && options.oldValue
        ? (record.oldValue === undefined ? null : record.oldValue)
        : null,
    }};
  }};
  let mutationDeliveryQueued = false;
  const deliverMutationObservers = () => {{
    mutationDeliveryQueued = false;
    for (const observer of Array.from(mutationObservers)) {{
      if (!observer.__glassRecords || observer.__glassRecords.length === 0) continue;
      const records = observer.__glassRecords.splice(0, observer.__glassRecords.length);
      observer.__glassCallback(records, observer);
    }}
  }};
  const scheduleMutationDelivery = () => {{
    if (mutationDeliveryQueued) return;
    mutationDeliveryQueued = true;
    Promise.resolve().then(deliverMutationObservers);
  }};
  const queueMutation = (record) => {{
    if (!record || !record.target) return;
    for (const observer of Array.from(mutationObservers)) {{
      const registrations = observer.__glassRegistrations || [];
      for (const registration of registrations) {{
        const options = registration.options;
        if (!mutationMatchesTarget(record.target, registration.target, options.subtree)) continue;
        if (record.type === "attributes" && !options.attributes) continue;
        if (record.type === "characterData" && !options.characterData) continue;
        if (record.type === "childList" && !options.childList) continue;
        if (record.type === "attributes" && options.attributeFilter
            && !options.attributeFilter.includes(String(record.attributeName).toLowerCase())) continue;
        observer.__glassRecords.push(mutationRecord(record, {{
          oldValue: record.type === "attributes"
            ? options.attributeOldValue
            : options.characterDataOldValue,
        }}));
        if (observer.__glassRecords.length > {max_commands}) {{
          observer.__glassRecords.splice(0, observer.__glassRecords.length - {max_commands});
        }}
        break;
      }}
    }}
    scheduleMutationDelivery();
  }};
  const queueFragmentChildRemoval = (parent, child) => {{
    if (!parent || parent.__glassFragment !== true || !child) return;
    const children = Array.isArray(parent.__glassChildren) ? parent.__glassChildren : [];
    const position = children.indexOf(child);
    if (position < 0) return;
    queueMutation({{
      type: "childList",
      target: parent,
      removedNodes: [child],
      previousSibling: position > 0 ? children[position - 1] : null,
      nextSibling: position + 1 < children.length ? children[position + 1] : null,
    }});
  }};
  const normalizeMutationOptions = (rawOptions) => {{
    if (!rawOptions || typeof rawOptions !== "object") throw new TypeError("MutationObserver options must be an object");
    const attributes = rawOptions.attributes === undefined
      ? rawOptions.attributeOldValue !== undefined || rawOptions.attributeFilter !== undefined
      : Boolean(rawOptions.attributes);
    const characterData = rawOptions.characterData === undefined
      ? rawOptions.characterDataOldValue !== undefined
      : Boolean(rawOptions.characterData);
    const childList = Boolean(rawOptions.childList);
    if (!attributes && !characterData && !childList) throw new TypeError("MutationObserver requires an observation type");
    if (rawOptions.attributeOldValue && !attributes) throw new TypeError("attributeOldValue requires attributes");
    if (rawOptions.attributeFilter !== undefined && !attributes) throw new TypeError("attributeFilter requires attributes");
    if (rawOptions.characterDataOldValue && !characterData) throw new TypeError("characterDataOldValue requires characterData");
    let attributeFilter = null;
    if (rawOptions.attributeFilter !== undefined) {{
      if (!Array.isArray(rawOptions.attributeFilter)) throw new TypeError("attributeFilter must be an array");
      attributeFilter = rawOptions.attributeFilter.map((name) => String(name).toLowerCase());
    }}
    return {{
      attributes,
      attributeOldValue: Boolean(rawOptions.attributeOldValue),
      attributeFilter,
      characterData,
      characterDataOldValue: Boolean(rawOptions.characterDataOldValue),
      childList,
      subtree: Boolean(rawOptions.subtree),
    }};
  }};
  const MutationObserverNative = globalThis.__glassMutationObserverConstructor
    || function MutationObserver(callback) {{
      if (!(this instanceof MutationObserverNative)) throw new TypeError("MutationObserver requires new");
      if (typeof callback !== "function") throw new TypeError("MutationObserver callback must be callable");
      Object.defineProperties(this, {{
        __glassCallback: {{ value: callback, writable: true }},
        __glassRegistrations: {{ value: [], writable: true }},
        __glassRecords: {{ value: [], writable: true }},
      }});
    }};
  MutationObserverNative.prototype.observe = function(target, rawOptions) {{
    if (!target || ![1, 3, 8, 9, 11].includes(Number(target.nodeType))) throw new TypeError("MutationObserver target must be a node");
    const options = normalizeMutationOptions(rawOptions);
    const registrations = this.__glassRegistrations || [];
    const existing = registrations.find((registration) => registration.target === target);
    if (existing) existing.options = options;
    else registrations.push({{ target, options }});
    this.__glassRegistrations = registrations;
    mutationObservers.add(this);
  }};
  MutationObserverNative.prototype.disconnect = function() {{
    this.__glassRegistrations = [];
    this.__glassRecords = [];
    mutationObservers.delete(this);
  }};
  MutationObserverNative.prototype.takeRecords = function() {{
    const records = this.__glassRecords || [];
    this.__glassRecords = [];
    return records;
  }};
  globalThis.__glassMutationObserverConstructor = MutationObserverNative;
  globalThis.MutationObserver = MutationObserverNative;
  const mutationIndexList = (indexes) => (Array.isArray(indexes) ? indexes : [])
    .map((index) => mutationNode(index))
    .filter(Boolean);
  const recordCommandMutation = (command) => {{
    if (!command || typeof command !== "object") return;
    const kind = String(command.kind || "");
    const nodeIndex = Number(command.node_index);
    const target = mutationNode(nodeIndex);
    if (kind === "setAttribute") {{
      if (!target) return;
      const attributes = mutationShadowAttributes.get(nodeIndex) || {{}};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      const value = String(command.value);
      attributes[name] = value;
      mutationShadowAttributes.set(nodeIndex, attributes);
      if (oldValue !== value) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "removeAttribute") {{
      if (!target) return;
      const attributes = mutationShadowAttributes.get(nodeIndex) || {{}};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      delete attributes[name];
      mutationShadowAttributes.set(nodeIndex, attributes);
      if (oldValue !== null) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "setTextContent") {{
      if (!target) return;
      const value = String(command.value);
      if ([3, 8].includes(Number(target.nodeType))) {{
        const oldValue = mutationShadowText.has(nodeIndex)
          ? mutationShadowText.get(nodeIndex)
          : String(target.nodeValue || "");
        mutationShadowText.set(nodeIndex, value);
        if (oldValue !== value) queueMutation({{ type: "characterData", target, oldValue }});
        return;
      }}
      const oldChildren = mutationShadowChildren.get(nodeIndex) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = mutationIndexList(oldChildren);
      for (const childIndex of oldChildren) mutationShadowParents.delete(Number(childIndex));
      mutationShadowChildren.set(nodeIndex, addedIndexes);
      for (const childIndex of addedIndexes) mutationShadowParents.set(childIndex, nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || value.length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "setInnerHtml") {{
      if (!target) return;
      const oldChildren = mutationShadowChildren.get(nodeIndex) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = mutationIndexList(oldChildren);
      for (const childIndex of oldChildren) mutationShadowParents.delete(Number(childIndex));
      mutationShadowChildren.set(nodeIndex, addedIndexes);
      for (const childIndex of addedIndexes) mutationShadowParents.set(childIndex, nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || String(command.value).length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "removeNode") {{
      if (!target) return;
      const parentIndex = mutationShadowParents.get(nodeIndex);
      const parent = mutationNode(parentIndex);
      if (parent) {{
        const children = mutationShadowChildren.get(Number(parentIndex)) || [];
        const position = children.indexOf(nodeIndex);
        queueMutation({{
          type: "childList",
          target: parent,
          removedNodes: [target],
          previousSibling: position > 0 ? mutationNode(children[position - 1]) : null,
          nextSibling: position >= 0 ? mutationNode(children[position + 1]) : null,
        }});
        mutationShadowChildren.set(Number(parentIndex), children.filter((index) => index !== nodeIndex));
      }}
      mutationShadowParents.delete(nodeIndex);
      return;
    }}
    if (kind === "appendChild" || kind === "insertBefore") {{
      const parentIndex = Number(command.parent_index);
      const parent = mutationNode(parentIndex);
      const childIndex = Number(command.child_index);
      const child = mutationNode(childIndex);
      if (!parent || !child) return;
      const oldParentIndex = mutationShadowParents.get(childIndex);
      if (oldParentIndex !== undefined) {{
        const oldChildren = mutationShadowChildren.get(oldParentIndex) || [];
        const oldPosition = oldChildren.indexOf(childIndex);
        const oldParent = mutationNode(oldParentIndex);
        if (oldParent) queueMutation({{
          type: "childList",
          target: oldParent,
          removedNodes: [child],
          previousSibling: oldPosition > 0 ? mutationNode(oldChildren[oldPosition - 1]) : null,
          nextSibling: oldPosition >= 0 ? mutationNode(oldChildren[oldPosition + 1]) : null,
        }});
        mutationShadowChildren.set(oldParentIndex, oldChildren.filter((index) => index !== childIndex));
      }}
      const children = (mutationShadowChildren.get(parentIndex) || []).filter((index) => index !== childIndex);
      const beforeIndex = kind === "insertBefore" && command.before_index != null
        ? Number(command.before_index)
        : null;
      const insertion = beforeIndex === null ? children.length : Math.max(0, children.indexOf(beforeIndex));
      const previousSibling = insertion > 0 ? mutationNode(children[insertion - 1]) : null;
      const nextSibling = mutationNode(children[insertion]);
      children.splice(insertion, 0, childIndex);
      mutationShadowChildren.set(parentIndex, children);
      mutationShadowParents.set(childIndex, parentIndex);
      queueMutation({{ type: "childList", target: parent, addedNodes: [child], previousSibling, nextSibling }});
    }}
  }};
  globalThis.__glassRecordMutationCommand = recordCommandMutation;
  const validityFlags = (entry) => {{
    const source = entry.validity || {{}};
    const customError = String(entry.customValidity || "").length > 0;
    const validity = {{
      badInput: Boolean(source.badInput),
      customError,
      patternMismatch: Boolean(source.patternMismatch),
      rangeOverflow: Boolean(source.rangeOverflow),
      rangeUnderflow: Boolean(source.rangeUnderflow),
      stepMismatch: Boolean(source.stepMismatch),
      tooLong: Boolean(source.tooLong),
      tooShort: Boolean(source.tooShort),
      typeMismatch: Boolean(source.typeMismatch),
      valueMissing: Boolean(source.valueMissing),
      valid: false,
    }};
    validity.valid = String(entry.tagName).toUpperCase() === "FORM"
      ? Boolean(source.valid)
      : !validity.badInput && !validity.customError
        && !validity.patternMismatch && !validity.rangeOverflow
        && !validity.rangeUnderflow && !validity.stepMismatch
        && !validity.tooLong && !validity.tooShort
        && !validity.typeMismatch && !validity.valueMissing;
    return validity;
  }};
  const validationMessageFor = (entry, validity) => {{
    const custom = String(entry.customValidity || "");
    if (custom.length > 0) return custom;
    if (validity.valueMissing) return "Please fill out this field.";
    if (validity.typeMismatch || validity.badInput) return "Please enter a valid value.";
    if (validity.tooShort) return "Value is too short.";
    if (validity.tooLong) return "Value is too long.";
    if (validity.rangeUnderflow) return "Value is below the minimum.";
    if (validity.rangeOverflow) return "Value is above the maximum.";
    if (validity.stepMismatch) return "Value does not match the required step.";
    if (validity.patternMismatch) return "Please match the requested format.";
    return "";
  }};
  const escapeHtmlText = (value) => String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
  const textFromHtml = (value) => String(value)
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/gi, "\u00a0")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&amp;/gi, "&");
  const decodeHtmlEntities = (value) => String(value)
    .replace(/&nbsp;/gi, "\u00a0")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&#x([0-9a-f]+);/gi, (match, digits) => {{
      const codePoint = Number.parseInt(digits, 16);
      return Number.isFinite(codePoint) && codePoint <= 0x10ffff
        && !(codePoint >= 0xd800 && codePoint <= 0xdfff)
        ? String.fromCodePoint(codePoint)
        : match;
    }})
    .replace(/&#([0-9]+);/g, (match, digits) => {{
      const codePoint = Number.parseInt(digits, 10);
      return Number.isFinite(codePoint) && codePoint <= 0x10ffff
        && !(codePoint >= 0xd800 && codePoint <= 0xdfff)
        ? String.fromCodePoint(codePoint)
        : match;
    }})
    .replace(/&amp;/gi, "&");
  const populateDetachedFragment = (fragment, markup, createElement, createText, createComment) => {{
    const stack = [fragment];
    const source = String(markup);
    const voidElements = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"]);
    const rawTextElements = new Set(["script", "style"]);
    const rcdataElements = new Set(["textarea", "title"]);
    const shouldAutoClose = (current, next) =>
      (current === "li" && next === "li")
      || (current === "p" && [
        "address", "article", "aside", "blockquote", "details", "div", "dl",
        "fieldset", "figcaption", "figure", "footer", "form", "h1", "h2",
        "h3", "h4", "h5", "h6", "header", "hgroup", "hr", "main", "menu",
        "nav", "ol", "p", "pre", "section", "table", "ul",
      ].includes(next))
      || ((current === "dt" || current === "dd") && (next === "dt" || next === "dd"))
      || (current === "rt" && next === "rt")
      || (current === "rp" && next === "rp")
      || (current === "option" && ["option", "optgroup"].includes(next))
      || (current === "optgroup" && next === "optgroup")
      || (current === "tr" && ["tr", "tbody", "thead", "tfoot"].includes(next))
      || (["td", "th"].includes(current) && ["td", "th", "tr", "tbody", "thead", "tfoot"].includes(next))
      || (current === "thead" && ["tbody", "tfoot"].includes(next))
      || (current === "tbody" && ["tbody", "tfoot"].includes(next))
      || (current === "tfoot" && next === "tbody")
      || (current === "colgroup" && ["colgroup", "tbody", "thead", "tfoot"].includes(next));
    const findTagEnd = (from) => {{
      let quote = null;
      for (let index = from; index < source.length; index += 1) {{
        const character = source[index];
        if (quote !== null) {{
          if (character === quote) quote = null;
        }} else if (character === "\"" || character === "'") {{
          quote = character;
        }} else if (character === ">") {{
          return index;
        }}
      }}
      return -1;
    }};
    const findSpecialEnd = (from, name) => {{
      const lowerSource = source.toLowerCase();
      const needle = "</" + name.toLowerCase();
      let candidate = lowerSource.indexOf(needle, from);
      while (candidate >= 0) {{
        const afterName = candidate + needle.length;
        const boundary = source[afterName];
        if (boundary === undefined || /[\s>]/.test(boundary)) {{
          const end = findTagEnd(afterName);
          if (end >= 0) return {{ start: candidate, end: end + 1 }};
        }}
        candidate = lowerSource.indexOf(needle, candidate + 1);
      }}
      return null;
    }};
    let cursor = 0;
    while (cursor < source.length) {{
      let parent = stack[stack.length - 1];
      if (source.startsWith("<!--", cursor)) {{
        const end = source.indexOf("-->", cursor + 4);
        const commentEnd = end < 0 ? source.length : end;
        parent.appendChild(createComment(source.slice(cursor + 4, commentEnd)));
        cursor = end < 0 ? source.length : end + 3;
        continue;
      }}
      if (source[cursor] !== "<") {{
        const end = source.indexOf("<", cursor);
        const textEnd = end < 0 ? source.length : end;
        if (textEnd > cursor) parent.appendChild(createText(decodeHtmlEntities(source.slice(cursor, textEnd))));
        cursor = textEnd;
        continue;
      }}
      if (source.startsWith("<!", cursor) || source.startsWith("<?", cursor)) {{
        const end = findTagEnd(cursor + 2);
        const declaration = source.slice(cursor + 2, end < 0 ? source.length : end);
        if (!/^doctype(?:\s|$)/i.test(declaration)) parent.appendChild(createComment(declaration));
        cursor = end < 0 ? source.length : end + 1;
        continue;
      }}
      if (source.startsWith("</", cursor)) {{
        const end = findTagEnd(cursor + 2);
        if (end < 0) {{
          parent.appendChild(createText(decodeHtmlEntities(source.slice(cursor))));
          break;
        }}
        const closing = /^\s*([A-Za-z][A-Za-z0-9:_-]*)/.exec(source.slice(cursor + 2, end));
        if (!closing) {{
          parent.appendChild(createText(decodeHtmlEntities(source.slice(cursor, end + 1))));
          cursor = end + 1;
          continue;
        }}
        const name = closing[1].toLowerCase();
        for (let index = stack.length - 1; index > 0; index -= 1) {{
          if (stack[index].localName === name) {{
            stack.length = index;
            break;
          }}
        }}
        cursor = end + 1;
        continue;
      }}
      const end = findTagEnd(cursor + 1);
      if (end < 0) {{
        parent.appendChild(createText(decodeHtmlEntities(source.slice(cursor))));
        break;
      }}
      const rawTag = source.slice(cursor + 1, end);
      const opening = /^\s*([A-Za-z][A-Za-z0-9:_-]*)/.exec(rawTag);
      if (!opening) {{
        cursor = end + 1;
        continue;
      }}
      const normalizedName = opening[1].toLowerCase();
      while (stack.length > 1
          && shouldAutoClose(stack[stack.length - 1].localName, normalizedName)) {{
        stack.pop();
      }}
      parent = stack[stack.length - 1];
      const element = createElement(
        opening[1],
        namespaceForChildElement(parent, normalizedName),
      );
      const selfClosing = /\/\s*$/.test(rawTag);
      const attributeSource = rawTag
        .slice(opening[0].length)
        .replace(/\/\s*$/, "");
      const attributes = /([A-Za-z_:][A-Za-z0-9:._-]*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>]+)))?/g;
      let attribute;
      while ((attribute = attributes.exec(attributeSource)) !== null) {{
        const value = attribute[2] !== undefined
          ? attribute[2]
          : attribute[3] !== undefined
            ? attribute[3]
            : attribute[4] !== undefined
              ? attribute[4]
              : "";
        if (!element.hasAttribute(attribute[1])) {{
          const attributeNamespace = parsedAttributeNamespace(attribute[1], element.namespaceURI);
          if (attributeNamespace === null) element.setAttribute(attribute[1], decodeHtmlEntities(value));
          else element.setAttributeNS(attributeNamespace, attribute[1], decodeHtmlEntities(value));
        }}
      }}
      parent.appendChild(element);
      cursor = end + 1;
      if (selfClosing || voidElements.has(element.localName)) continue;
      if (rawTextElements.has(element.localName) || rcdataElements.has(element.localName)) {{
        const special = findSpecialEnd(cursor, element.localName);
        const textEnd = special ? special.start : source.length;
        if (textEnd > cursor) element.appendChild(createText(
          rawTextElements.has(element.localName)
            ? source.slice(cursor, textEnd)
            : decodeHtmlEntities(source.slice(cursor, textEnd)),
        ));
        cursor = special ? special.end : source.length;
        continue;
      }}
      stack.push(element);
    }}
  }};
  const installReflectedAttributeProperties = (element) => {{
    Object.defineProperty(element, "id", {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute("id");
        return value === null ? "" : value;
      }},
      set(next) {{ element.setAttribute("id", String(next)); }},
    }});
    Object.defineProperty(element, "className", {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute("class");
        return value === null ? "" : value;
      }},
      set(next) {{ element.setAttribute("class", String(next)); }},
    }});
  }};
  const installBooleanAttributeProperty = (element, property, attribute, read) => {{
    Object.defineProperty(element, property, {{
      enumerable: true,
      configurable: false,
      get() {{ return read(); }},
      set(next) {{
        if (Boolean(next)) element.setAttribute(attribute, "");
        else element.removeAttribute(attribute);
      }},
    }});
  }};
  const installStringAttributeProperty = (element, property, attribute) => {{
    Object.defineProperty(element, property, {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute(attribute);
        return value === null ? "" : value;
      }},
      set(next) {{ element.setAttribute(attribute, String(next)); }},
    }});
  }};
  const installUrlAttributeProperty = (element, property, attribute, baseUrl, onSet = null) => {{
    Object.defineProperty(element, property, {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute(attribute);
        if (value === null || value === "") return "";
        try {{ return new URLNative(value, baseUrl).href; }} catch (_error) {{ return value; }}
      }},
      set(next) {{
        const value = String(next);
        element.setAttribute(attribute, value);
        if (typeof onSet === "function") onSet(value === "");
      }},
    }});
  }};
  const installCommonAttributeProperties = (element, state = {{}}, baseUrl = host.url) => {{
    installReflectedAttributeProperties(element);
    for (const [property, attribute] of [
      ["disabled", "disabled"], ["hidden", "hidden"], ["multiple", "multiple"],
      ["required", "required"], ["readOnly", "readonly"], ["autofocus", "autofocus"],
      ["open", "open"], ["controls", "controls"], ["loop", "loop"],
      ["muted", "muted"], ["autoplay", "autoplay"], ["reversed", "reversed"],
      ["formNoValidate", "formnovalidate"], ["noValidate", "novalidate"],
    ]) {{
      installBooleanAttributeProperty(element, property, attribute,
        state[property] || (() => element.hasAttribute(attribute)));
    }}
    for (const [property, attribute] of [
      ["name", "name"], ["title", "title"], ["lang", "lang"], ["dir", "dir"],
      ["slot", "slot"], ["htmlFor", "for"], ["accept", "accept"], ["alt", "alt"],
      ["placeholder", "placeholder"], ["pattern", "pattern"], ["min", "min"],
      ["max", "max"], ["step", "step"], ["method", "method"],
      ["target", "target"], ["rel", "rel"], ["download", "download"],
    ]) installStringAttributeProperty(element, property, attribute);
    if (["A", "AREA", "BASE", "LINK"].includes(element.tagName))
      installUrlAttributeProperty(element, "href", "href", baseUrl);
    if (["IMG", "SCRIPT", "IFRAME", "FRAME", "EMBED", "SOURCE", "TRACK", "AUDIO", "VIDEO"].includes(element.tagName))
      installUrlAttributeProperty(element, "src", "src", baseUrl,
        element.tagName === "IMG" ? state.imageReset : null);
    if (element.tagName === "FORM")
      installUrlAttributeProperty(element, "action", "action", baseUrl);
    if (["IMG", "SOURCE"].includes(element.tagName)) {{
      installStringAttributeProperty(element, "srcset", "srcset");
      installStringAttributeProperty(element, "sizes", "sizes");
    }}
    if (element.tagName === "SOURCE")
      installStringAttributeProperty(element, "media", "media");
    if (element.tagName === "IMG") {{
      const imageState = () => ({{
        complete: typeof state.imageComplete === "function"
          ? Boolean(state.imageComplete())
          : !element.hasAttribute("src"),
        naturalWidth: typeof state.imageNaturalWidth === "function"
          ? Number(state.imageNaturalWidth()) || 0
          : 0,
        naturalHeight: typeof state.imageNaturalHeight === "function"
          ? Number(state.imageNaturalHeight()) || 0
          : 0,
      }});
      Object.defineProperty(element, "complete", {{
        enumerable: true,
        configurable: false,
        get() {{ return imageState().complete; }},
      }});
      Object.defineProperty(element, "naturalWidth", {{
        enumerable: true,
        configurable: false,
        get() {{ return imageState().naturalWidth; }},
      }});
      Object.defineProperty(element, "naturalHeight", {{
        enumerable: true,
        configurable: false,
        get() {{ return imageState().naturalHeight; }},
      }});
      Object.defineProperty(element, "currentSrc", {{
        enumerable: true,
        configurable: false,
        get() {{
          const selected = typeof state.imageCurrentSrc === "function"
            ? String(state.imageCurrentSrc() || "")
            : "";
          const value = selected || element.getAttribute("src");
          if (value === null || value === "") return "";
          try {{ return new URLNative(value, baseUrl).href; }} catch (_error) {{ return value; }}
        }},
      }});
      installEventHandlerProperty(element, "load");
      installEventHandlerProperty(element, "error");
    }}
    Object.defineProperty(element, "type", {{
      enumerable: true,
      configurable: false,
      get() {{
        const value = element.getAttribute("type");
        if (value !== null) return value.toLowerCase();
        if (element.localName === "input") return "text";
        if (element.localName === "button") return "submit";
        return "";
      }},
      set(next) {{ element.setAttribute("type", String(next)); }},
    }});
  }};
  if (!Number.isSafeInteger(globalThis.__glassNextTemporaryNodeIndex)) {{
    globalThis.__glassNextTemporaryNodeIndex = 4294967294;
  }}
  const allocateTemporaryNodeIndex = () => {{
    const value = Number(globalThis.__glassNextTemporaryNodeIndex);
    if (value < 4294963200) throw new RangeError("native temporary node limit exceeded");
    globalThis.__glassNextTemporaryNodeIndex = value - 1;
    return value;
  }};
  const attributeNodeKey = (namespace, name) =>
    String(namespace || "") + "\u0000" + String(name).toLowerCase();
  const normalizeAttributeNodeName = (name) => {{
    const value = String(name);
    if (!/^[A-Za-z_:][A-Za-z0-9:._-]*$/.test(value)) {{
      const constructor = globalThis.DOMException;
      if (typeof constructor === "function") throw new constructor("The attribute name is invalid", "InvalidCharacterError");
      throw new TypeError("The attribute name is invalid");
    }}
    return value.toLowerCase();
  }};
  const makeAttributeNode = (name, initialValue = "", ownerDocumentResolver = () => null, namespace = null) => {{
    const normalized = normalizeAttributeNodeName(name);
    const namespaceURI = normalizeAttributeNamespace(namespace);
    const separator = normalized.indexOf(":");
    let attributeValue = String(initialValue);
    if (attributeValue.length > {storage_value_limit}) throw new RangeError("native attribute value exceeds its limit");
    let ownerElement = null;
    const attribute = {{
      nodeIndex: allocateTemporaryNodeIndex(),
      parentIndex: null,
      nodeType: 2,
      nodeName: normalized,
      name: normalized,
      localName: separator < 0 ? normalized : normalized.slice(separator + 1),
      prefix: separator < 0 ? null : normalized.slice(0, separator),
      namespaceURI,
      specified: true,
      __glassAttribute: true,
      __glassChildren: [],
    }};
    Object.defineProperty(attribute, "ownerElement", {{
      enumerable: true,
      configurable: false,
      get() {{ return ownerElement; }},
    }});
    Object.defineProperty(attribute, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get() {{ return ownerDocumentResolver() || null; }},
    }});
    Object.defineProperty(attribute, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{ return null; }},
    }});
    Object.defineProperty(attribute, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{ return null; }},
    }});
    Object.defineProperties(attribute, {{
      value: {{
        enumerable: true,
        configurable: false,
        get() {{ return attributeValue; }},
        set(next) {{
          const value = String(next);
          if (value.length > {storage_value_limit}) throw new RangeError("native attribute value exceeds its limit");
          if (ownerElement) {{
            if (namespaceURI === null) ownerElement.setAttribute(normalized, value);
            else ownerElement.setAttributeNS(namespaceURI, normalized, value);
          }}
          else attributeValue = value;
        }},
      }},
      nodeValue: {{
        enumerable: true,
        configurable: false,
        get() {{ return attributeValue; }},
        set(next) {{ attribute.value = next; }},
      }},
      textContent: {{
        enumerable: true,
        configurable: false,
        get() {{ return attributeValue; }},
        set(next) {{ attribute.value = next; }},
      }},
    }});
    Object.defineProperty(attribute, "__glassSetOwner", {{
      enumerable: false,
      configurable: false,
      value(nextOwner) {{ ownerElement = nextOwner || null; }},
    }});
    Object.defineProperty(attribute, "__glassRefreshValue", {{
      enumerable: false,
      configurable: false,
      value(nextValue) {{
        const value = String(nextValue);
        attributeValue = value.length > {storage_value_limit}
          ? value.slice(0, {storage_value_limit})
          : value;
      }},
    }});
    defineTreeAccessors(attribute);
    const constructor = globalThis.Attr;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(attribute, constructor.prototype); }} catch (_error) {{}}
    }}
    return attribute;
  }};
  const attributeNodeException = (message, name) => {{
    const constructor = globalThis.DOMException;
    if (typeof constructor === "function") return new constructor(message, name);
    const error = new Error(message);
    error.name = name;
    return error;
  }};
  const installAttributeNodeSurface = (element, ownerDocumentResolver) => {{
    if (Object.prototype.hasOwnProperty.call(element, "__glassAttributeNodes")) return;
    const attributeNodes = new Map();
    Object.defineProperty(element, "__glassAttributeNodes", {{
      enumerable: false,
      configurable: false,
      value: attributeNodes,
    }});
    const originalSetAttribute = typeof element.setAttribute === "function"
      ? element.setAttribute.bind(element)
      : null;
    const originalRemoveAttribute = typeof element.removeAttribute === "function"
      ? element.removeAttribute.bind(element)
      : null;
    const syncAttributeNodes = () => {{
      for (const [key, attribute] of attributeNodes) {{
        const namespaceURI = attribute.namespaceURI || null;
        const value = namespaceURI === null
          ? element.getAttribute(attribute.name)
          : element.getAttributeNS(namespaceURI, attribute.localName);
        if (value === null) {{
          attribute.__glassSetOwner(null);
          attributeNodes.delete(key);
        }} else {{
          attribute.__glassRefreshValue(value);
          attribute.__glassSetOwner(element);
        }}
      }}
    }};
    Object.defineProperty(element, "__glassSyncAttributeNodes", {{
      enumerable: false,
      configurable: false,
      value: syncAttributeNodes,
    }});
    if (originalSetAttribute) {{
      element.setAttribute = (name, value) => {{
        originalSetAttribute(name, value);
        const key = attributeNodeKey(null, name);
        const attribute = attributeNodes.get(key);
        if (attribute) {{
          attribute.__glassRefreshValue(element.getAttribute(attribute.name));
          attribute.__glassSetOwner(element);
        }}
        syncAttributeNodes();
      }};
    }}
    if (originalRemoveAttribute) {{
      element.removeAttribute = (name) => {{
        originalRemoveAttribute(name);
        const key = attributeNodeKey(null, name);
        const attribute = attributeNodes.get(key);
        if (attribute) {{
          attribute.__glassSetOwner(null);
          attributeNodes.delete(key);
        }}
        syncAttributeNodes();
      }};
    }}
    const getAttributeNode = (name) => {{
      const key = normalizeAttributeNodeName(name);
      if (element.getAttribute(key) === null) return null;
      const namespaceURI = typeof element.__glassAttributeNamespace === "function"
        ? element.__glassAttributeNamespace(key)
        : null;
      if (namespaceURI !== null) return getAttributeNodeNS(namespaceURI, attributeLocalName(key));
      const nodeKey = attributeNodeKey(null, key);
      const existing = attributeNodes.get(nodeKey);
      if (existing) {{
        existing.__glassRefreshValue(element.getAttribute(key));
        existing.__glassSetOwner(element);
        return existing;
      }}
      const attribute = makeAttributeNode(key, element.getAttribute(key), ownerDocumentResolver);
      attribute.__glassSetOwner(element);
      attributeNodes.set(nodeKey, attribute);
      return attribute;
    }};
    const setAttributeNode = (attribute) => {{
      if (!attribute || attribute.__glassAttribute !== true) throw new TypeError("setAttributeNode requires an Attr");
      const ownerDocument = ownerDocumentResolver() || null;
      if (attribute.ownerDocument !== ownerDocument) throw attributeNodeException("The attribute belongs to another document", "WrongDocumentError");
      if (attribute.ownerElement && attribute.ownerElement !== element) throw attributeNodeException("The attribute is already in use", "InUseAttributeError");
      const key = normalizeAttributeNodeName(attribute.name);
      const namespaceURI = attribute.namespaceURI || null;
      const old = namespaceURI === null
        ? getAttributeNode(key)
        : getAttributeNodeNS(namespaceURI, attribute.localName);
      if (old === attribute) return old;
      if (old) {{
        old.__glassSetOwner(null);
        attributeNodes.delete(attributeNodeKey(old.namespaceURI, old.name));
      }}
      if (namespaceURI === null) element.setAttribute(key, attribute.value);
      else element.setAttributeNS(namespaceURI, key, attribute.value);
      attributeNodes.set(attributeNodeKey(namespaceURI, key), attribute);
      attribute.__glassSetOwner(element);
      return old;
    }};
    const removeAttributeNode = (attribute) => {{
      if (!attribute || attribute.__glassAttribute !== true) throw new TypeError("removeAttributeNode requires an Attr");
      if (attribute.ownerElement !== element) throw attributeNodeException("The attribute was not found", "NotFoundError");
      const namespaceURI = attribute.namespaceURI || null;
      if (namespaceURI === null) element.removeAttribute(attribute.name);
      else element.removeAttributeNS(namespaceURI, attribute.localName);
      return attribute;
    }};
    const getAttributeNodeNS = (namespace, name) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const localName = attributeLocalName(name);
      const candidate = element.getAttributeNames().find((attributeName) =>
        attributeLocalName(attributeName) === localName
          && (typeof element.__glassAttributeNamespace === "function"
            ? element.__glassAttributeNamespace(attributeName)
            : null) === namespaceURI);
      if (candidate === undefined) return null;
      if (namespaceURI === null) return getAttributeNode(candidate);
      const nodeKey = attributeNodeKey(namespaceURI, candidate);
      const existing = attributeNodes.get(nodeKey);
      const value = element.getAttributeNS(namespaceURI, localName);
      if (existing) {{
        existing.__glassRefreshValue(value);
        existing.__glassSetOwner(element);
        return existing;
      }}
      const attribute = makeAttributeNode(candidate, value, ownerDocumentResolver, namespaceURI);
      attribute.__glassSetOwner(element);
      attributeNodes.set(nodeKey, attribute);
      return attribute;
    }};
    element.getAttributeNode = getAttributeNode;
    element.getAttributeNodeNS = getAttributeNodeNS;
    element.setAttributeNode = setAttributeNode;
    element.setAttributeNodeNS = (namespace, attribute) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      if ((attribute && (attribute.namespaceURI || null)) !== namespaceURI) {{
        throw attributeNodeException("The attribute namespace does not match", "NamespaceError");
      }}
      return setAttributeNode(attribute);
    }};
    element.removeAttributeNode = removeAttributeNode;
    const namedNodeMap = {{
      get length() {{ return element.getAttributeNames().length; }},
      item(index) {{
        const names = element.getAttributeNames();
        const name = names[Number(index)];
        return name === undefined ? null : getAttributeNode(name);
      }},
      getNamedItem(name) {{ return getAttributeNode(name); }},
      getNamedItemNS(namespace, name) {{ return getAttributeNodeNS(namespace, name); }},
      setNamedItem(attribute) {{ return setAttributeNode(attribute); }},
      setNamedItemNS(namespace, attribute) {{ return element.setAttributeNodeNS(namespace, attribute); }},
      removeNamedItem(name) {{
        const attribute = getAttributeNode(name);
        if (!attribute) throw attributeNodeException("The attribute was not found", "NotFoundError");
        return removeAttributeNode(attribute);
      }},
      removeNamedItemNS(namespace, name) {{
        const attribute = getAttributeNodeNS(namespace, name);
        if (!attribute) throw attributeNodeException("The attribute was not found", "NotFoundError");
        return removeAttributeNode(attribute);
      }},
    }};
    Object.defineProperty(namedNodeMap, Symbol.iterator, {{
      configurable: false,
      value() {{
        return Array.from({{ length: namedNodeMap.length }}, (_value, index) => namedNodeMap.item(index))[Symbol.iterator]();
      }},
    }});
    const namedNodeMapView = new Proxy(namedNodeMap, {{
      get(target, property, receiver) {{
        if (typeof property === "string" && /^\d+$/.test(property)) return target.item(Number(property));
        return Reflect.get(target, property, receiver);
      }},
      has(target, property) {{
        if (typeof property === "string" && /^\d+$/.test(property)) return Number(property) < target.length;
        return Reflect.has(target, property);
      }},
    }});
    const namedNodeMapConstructor = globalThis.NamedNodeMap;
    const syncNamedNodeMapPrototype = () => {{
      const constructor = globalThis.NamedNodeMap || namedNodeMapConstructor;
      if (typeof constructor === "function" && constructor.prototype
          && Object.getPrototypeOf(namedNodeMap) !== constructor.prototype) {{
        try {{ Object.setPrototypeOf(namedNodeMap, constructor.prototype); }} catch (_error) {{}}
      }}
    }};
    Object.defineProperty(element, "attributes", {{
      enumerable: true,
      configurable: false,
      get() {{
        syncNamedNodeMapPrototype();
        syncAttributeNodes();
        return namedNodeMapView;
      }},
    }});
  }};
  const namespaceUriForEntry = (entry) => {{
    if (!Object.prototype.hasOwnProperty.call(entry, "namespaceUri")) return HTML_NAMESPACE;
    if (entry.namespaceUri === null || entry.namespaceUri === "") return null;
    return String(entry.namespaceUri);
  }};
  const tagNameForEntry = (entry) => {{
    const localName = String(entry.tagName || "").toLowerCase();
    return namespaceUriForEntry(entry) === HTML_NAMESPACE ? localName.toUpperCase() : localName;
  }};
  const normalizeElementNamespace = (namespace) => {{
    if (namespace === null || namespace === "" || namespace === undefined) return null;
    const value = String(namespace);
    if (![HTML_NAMESPACE, SVG_NAMESPACE, MATHML_NAMESPACE].includes(value)) {{
      throw new DOMExceptionNative("The element namespace is unsupported", "NamespaceError");
    }}
    return value;
  }};
  const normalizeAttributeNamespace = (namespace) => {{
    if (namespace === null || namespace === "" || namespace === undefined) return null;
    const value = String(namespace);
    if (![HTML_NAMESPACE, SVG_NAMESPACE, MATHML_NAMESPACE, XML_NAMESPACE, XMLNS_NAMESPACE, XLINK_NAMESPACE].includes(value)) {{
      throw new DOMExceptionNative("The attribute namespace is unsupported", "NamespaceError");
    }}
    return value;
  }};
  const normalizedAttributeName = (name) => normalizeAttributeNodeName(name);
  const attributeLocalName = (name) => {{
    const normalized = normalizedAttributeName(name);
    const separator = normalized.indexOf(":");
    return separator < 0 ? normalized : normalized.slice(separator + 1);
  }};
  const qualifiedAttributeName = (name, namespace) => {{
    const normalized = normalizedAttributeName(name);
    const separator = normalized.indexOf(":");
    const prefix = separator < 0 ? null : normalized.slice(0, separator);
    if (separator >= 0 && (separator === 0 || separator === normalized.length - 1
        || normalized.indexOf(":", separator + 1) >= 0)) {{
      throw new DOMExceptionNative("The qualified attribute name is invalid", "NamespaceError");
    }}
    if (prefix === "xml" && namespace !== XML_NAMESPACE) {{
      throw new DOMExceptionNative("The xml prefix requires the XML namespace", "NamespaceError");
    }}
    if ((prefix === "xmlns" || normalized === "xmlns") && namespace !== XMLNS_NAMESPACE) {{
      throw new DOMExceptionNative("The xmlns prefix requires the XMLNS namespace", "NamespaceError");
    }}
    if (namespace === XML_NAMESPACE && prefix !== "xml") {{
      throw new DOMExceptionNative("The XML namespace requires the xml prefix", "NamespaceError");
    }}
    if (namespace === XMLNS_NAMESPACE && prefix !== "xmlns" && normalized !== "xmlns") {{
      throw new DOMExceptionNative("The XMLNS namespace requires the xmlns prefix", "NamespaceError");
    }}
    if (namespace === XLINK_NAMESPACE && prefix !== "xlink") {{
      throw new DOMExceptionNative("The XLink namespace requires the xlink prefix", "NamespaceError");
    }}
    return normalized;
  }};
  const attributeNamespaceForEntry = (entry, name) => {{
    const key = normalizedAttributeName(name);
    const namespaces = entry && entry.attributeNamespaces && typeof entry.attributeNamespaces === "object"
      ? entry.attributeNamespaces
      : {{}};
    const value = namespaces[key];
    return value === undefined || value === "" ? null : String(value);
  }};
  const parsedAttributeNamespace = (name, elementNamespace) => {{
    if (elementNamespace !== SVG_NAMESPACE && elementNamespace !== MATHML_NAMESPACE) return null;
    const lower = String(name).toLowerCase();
    if (lower === "xmlns" || lower.startsWith("xmlns:")) return XMLNS_NAMESPACE;
    if (lower.startsWith("xml:")) return XML_NAMESPACE;
    if (lower.startsWith("xlink:")) return XLINK_NAMESPACE;
    return null;
  }};
  const namespaceForChildElement = (parent, localName) => {{
    const parentNamespace = parent && parent.namespaceURI;
    if (localName === "svg") return SVG_NAMESPACE;
    if (localName === "math") return MATHML_NAMESPACE;
    if (parentNamespace === SVG_NAMESPACE && parent.localName === "foreignobject") return HTML_NAMESPACE;
    if (parentNamespace === SVG_NAMESPACE) return SVG_NAMESPACE;
    if (parentNamespace === MATHML_NAMESPACE) return MATHML_NAMESPACE;
    return HTML_NAMESPACE;
  }};
  const makeElement = (initialEntry) => {{
    let entry = initialEntry;
    let textContent = String(entry.text || "");
    let innerHtml = String(entry.innerHtml || "");
    let value = entry.value === null
      ? (entry.tagName.toLowerCase() === "option"
        ? (entry.attributes.value === undefined ? entry.text : entry.attributes.value)
        : "")
      : entry.value;
    let files = makeNativeFileList(entry.files);
    let disabled = Boolean(entry.disabled);
    let hidden = Boolean(entry.hidden);
    let multiple = Object.prototype.hasOwnProperty.call(entry.attributes, "multiple");
    let imageComplete = entry.imageComplete === undefined
      ? !Object.prototype.hasOwnProperty.call(entry.attributes, "src")
      : Boolean(entry.imageComplete);
    let imageNaturalWidth = Number(entry.imageNaturalWidth) || 0;
    let imageNaturalHeight = Number(entry.imageNaturalHeight) || 0;
    let imageCurrentSrc = String(entry.imageCurrentSrc || "");
    let computedStyle = entry.computedStyle && typeof entry.computedStyle === "object"
      ? entry.computedStyle
      : {{}};
    let scrollLeft = Number(entry.scrollX) || 0;
    let scrollTop = Number(entry.scrollY) || 0;
    const resetImageState = (complete = false) => {{
      imageComplete = complete;
      imageNaturalWidth = 0;
      imageNaturalHeight = 0;
      imageCurrentSrc = "";
    }};
    if (!entry.attributeNamespaces || typeof entry.attributeNamespaces !== "object") entry.attributeNamespaces = {{}};
    const setNamespacedAttribute = (namespace, name, nextValue) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const qualified = qualifiedAttributeName(name, namespaceURI);
      const value = String(nextValue);
      if (value.length > {storage_value_limit}) throw new RangeError("native attribute value exceeds its limit");
      entry.attributes[qualified] = value;
      if (namespaceURI === null) delete entry.attributeNamespaces[qualified];
      else entry.attributeNamespaces[qualified] = namespaceURI;
      element.__glassSyncContent();
      if (typeof element.__glassSyncAttributeNodes === "function") element.__glassSyncAttributeNodes();
      pushCommand({{
        kind: "setAttribute",
        node_index: entry.nodeIndex,
        name: qualified,
        value,
        namespace_uri: namespaceURI === null ? "" : namespaceURI,
      }});
    }};
    const namespacedAttributeValue = (namespace, name) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const localName = attributeLocalName(name);
      for (const key of Object.keys(entry.attributes)) {{
        if (attributeLocalName(key) === localName
            && attributeNamespaceForEntry(entry, key) === namespaceURI) return entry.attributes[key];
      }}
      return null;
    }};
    const removeNamespacedAttribute = (namespace, name) => {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      const localName = attributeLocalName(name);
      const key = Object.keys(entry.attributes).find((candidate) =>
        attributeLocalName(candidate) === localName
          && attributeNamespaceForEntry(entry, candidate) === namespaceURI);
      if (key === undefined) return;
      delete entry.attributes[key];
      delete entry.attributeNamespaces[key];
      element.__glassSyncContent();
      if (typeof element.__glassSyncAttributeNodes === "function") element.__glassSyncAttributeNodes();
      pushCommand({{
        kind: "removeAttribute",
        node_index: entry.nodeIndex,
        name: key,
        namespace_uri: namespaceURI === null ? "" : namespaceURI,
      }});
    }};
    let selectionStart = entry.selectionStart;
    let selectionEnd = entry.selectionEnd;
    let selectionDirection = entry.selectionDirection || "none";
    const selectionLength = () => Array.from(String(value)).length;
    const selectionIndex = (candidate) => {{
      const numeric = Number(candidate);
      if (!Number.isFinite(numeric) || Math.trunc(numeric) !== numeric) {{
        throw new TypeError("selection offset must be a finite integer");
      }}
      return Math.max(0, Math.min(selectionLength(), numeric));
    }};
    const applySelection = (start, end, direction) => {{
      if (selectionStart == null || selectionEnd == null) {{
        throw new TypeError("selection is unavailable on this element");
      }}
      const nextStart = selectionIndex(start);
      const nextEnd = selectionIndex(end);
      const normalizedDirection = String(direction).toLowerCase();
      if (!["none", "forward", "backward"].includes(normalizedDirection)) {{
        throw new TypeError("invalid selection direction");
      }}
      selectionStart = Math.min(nextStart, nextEnd);
      selectionEnd = Math.max(nextStart, nextEnd);
      selectionDirection = normalizedDirection;
      pushCommand({{ kind: "setSelection", node_index: entry.nodeIndex, start: selectionStart, end: selectionEnd, direction: selectionDirection }});
    }};
    const scrollCoordinate = (candidate) => {{
      const numeric = Number(candidate);
      return Number.isFinite(numeric) ? Math.trunc(numeric) : 0;
    }};
    const applyScroll = (leftCandidate, topCandidate) => {{
      const isRoot = entry.tagName.toLowerCase() === "html";
      const geometry = geometryForNode(element);
      const clientWidth = isRoot
        ? Number(globalThis.innerWidth) || Number(geometry.clientWidth) || Number(geometry.width) || 0
        : Number(geometry.clientWidth) || Number(geometry.width) || 0;
      const clientHeight = isRoot
        ? Number(globalThis.innerHeight) || Number(geometry.clientHeight) || Number(geometry.height) || 0
        : Number(geometry.clientHeight) || Number(geometry.height) || 0;
      const scrollWidth = isRoot
        ? Number(state.scrollWidth) || clientWidth
        : Number(geometry.scrollWidth) || clientWidth;
      const scrollHeight = isRoot
        ? Number(state.scrollHeight) || clientHeight
        : Number(geometry.scrollHeight) || clientHeight;
      const currentLeft = isRoot ? Number(state.scrollX) || 0 : scrollLeft;
      const currentTop = isRoot ? Number(state.scrollY) || 0 : scrollTop;
      const left = Math.max(0, Math.min(
        Math.max(0, scrollWidth - clientWidth),
        scrollCoordinate(leftCandidate === undefined ? currentLeft : leftCandidate),
      ));
      const top = Math.max(0, Math.min(
        Math.max(0, scrollHeight - clientHeight),
        scrollCoordinate(topCandidate === undefined ? currentTop : topCandidate),
      ));
      if (isRoot) {{
        state.scrollX = left;
        state.scrollY = top;
        if (typeof globalThis.scrollX === "number") globalThis.scrollX = left;
        if (typeof globalThis.scrollY === "number") globalThis.scrollY = top;
        if (typeof globalThis.pageXOffset === "number") globalThis.pageXOffset = left;
        if (typeof globalThis.pageYOffset === "number") globalThis.pageYOffset = top;
        if (typeof element.__glassSetScrollState === "function") element.__glassSetScrollState(left, top);
      }} else {{
        scrollLeft = left;
        scrollTop = top;
      }}
      if (left !== currentLeft || top !== currentTop) {{
        pushCommand({{ kind: "scrollTo", node_index: isRoot ? 0 : entry.nodeIndex, left, top }});
      }}
      return undefined;
    }};
    const element = {{
      nodeIndex: entry.nodeIndex,
      parentIndex: entry.parentIndex,
      formOwnerIndex: entry.formOwnerIndex,
      tagName: tagNameForEntry(entry),
      nodeType: 1,
      nodeName: tagNameForEntry(entry),
      localName: entry.tagName.toLowerCase(),
      namespaceURI: namespaceUriForEntry(entry),
      id: entry.attributes.id || "",
      className: entry.attributes.class || "",
      value: entry.value === null
        ? (entry.tagName.toLowerCase() === "option"
          ? (entry.attributes.value === undefined ? entry.text : entry.attributes.value)
          : "")
        : entry.value,
      get files() {{ return files; }},
      checked: entry.checked,
      selected: entry.selected,
      multiple,
      disabled,
      hidden,
      focused: entry.focused,
      selectionStart: entry.selectionStart,
      selectionEnd: entry.selectionEnd,
      selectionDirection: entry.selectionDirection,
      get validity() {{ return validityFlags(entry); }},
      get validationMessage() {{
        return validationMessageFor(entry, validityFlags(entry));
      }},
      get willValidate() {{ return Boolean(entry.willValidate); }},
      getBoundingClientRect() {{ return makeDomRect(geometryForNode(element)); }},
      getClientRects() {{
        const geometry = geometryForNode(element);
        return geometry.width > 0 && geometry.height > 0
          ? asNodeList([makeDomRect(geometry)])
          : asNodeList([]);
      }},
      get clientWidth() {{
        const geometry = geometryForNode(element);
        return Number(geometry.clientWidth) || Number(geometry.width) || 0;
      }},
      get clientHeight() {{
        const geometry = geometryForNode(element);
        return Number(geometry.clientHeight) || Number(geometry.height) || 0;
      }},
      get offsetWidth() {{ return Number(geometryForNode(element).width) || 0; }},
      get offsetHeight() {{ return Number(geometryForNode(element).height) || 0; }},
      get scrollLeft() {{ return scrollLeft; }},
      set scrollLeft(value) {{ applyScroll(value, scrollTop); }},
      get scrollTop() {{ return scrollTop; }},
      set scrollTop(value) {{ applyScroll(scrollLeft, value); }},
      get scrollWidth() {{
        const geometry = geometryForNode(element);
        return Number(geometry.scrollWidth) || Number(geometry.width) || 0;
      }},
      get scrollHeight() {{
        const geometry = geometryForNode(element);
        return Number(geometry.scrollHeight) || Number(geometry.height) || 0;
      }},
      getAttribute(name) {{
        const key = String(name).toLowerCase();
        for (const attr of Object.keys(entry.attributes)) {{
          if (attr.toLowerCase() === key) return entry.attributes[attr];
        }}
        return null;
      }},
      getAttributeNS(namespace, name) {{
        return namespacedAttributeValue(namespace, name);
      }},
      getAttributeNames() {{ return Object.keys(entry.attributes); }},
      hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
      matches(selector) {{ return matchesSelector(element, selector); }},
      closest(selector) {{
        let current = element;
        while (current) {{
          if (matchesSelector(current, selector)) return current;
          current = current.parentElement;
        }}
        return null;
      }},
      querySelector(selector) {{
        return descendantsInTree(element, (candidate) => matchesSelector(candidate, selector))[0] || null;
      }},
      querySelectorAll(selector) {{
        return asNodeList(descendantsInTree(element, (candidate) => matchesSelector(candidate, selector)));
      }},
      getElementsByTagName(name) {{
        const value = String(name).toLowerCase();
        return asHtmlCollection(descendantsInTree(element, (candidate) =>
          value === "*" || candidate.tagName.toLowerCase() === value));
      }},
      getElementsByClassName(name) {{
        const value = String(name).trim();
        if (!value) return asHtmlCollection([]);
        const tokens = value.split(/\s+/);
        return asHtmlCollection(descendantsInTree(element, (candidate) =>
          tokens.every((token) => String(candidate.className).split(/\s+/).includes(token))));
      }},
      addEventListener(type, callback, options) {{
        addListener("node:" + entry.nodeIndex, type, callback, options);
      }},
      removeEventListener(type, callback, options) {{
        removeListener("node:" + entry.nodeIndex, type, callback, options);
      }},
      dispatchEvent(event) {{
        return dispatchTarget(this, event);
      }},
      focus() {{
        if (this.disabled || this.hidden) return;
        setLocalFocus(this);
      }},
      blur() {{
        if (!this.focused) return;
        this.focused = false;
        pushCommand({{ kind: "blur", node_index: entry.nodeIndex }});
        dispatchTarget(this, createEvent("blur"));
      }},
      scrollTo(leftOrOptions = 0, top = 0) {{
        if (leftOrOptions && typeof leftOrOptions === "object") {{
          applyScroll(leftOrOptions.left, leftOrOptions.top);
        }} else {{
          applyScroll(leftOrOptions, top);
        }}
      }},
      scrollBy(leftOrOptions = 0, top = 0) {{
        if (leftOrOptions && typeof leftOrOptions === "object") {{
          applyScroll(scrollLeft + scrollCoordinate(leftOrOptions.left), scrollTop + scrollCoordinate(leftOrOptions.top));
        }} else {{
          applyScroll(scrollLeft + scrollCoordinate(leftOrOptions), scrollTop + scrollCoordinate(top));
        }}
      }},
      scroll(leftOrOptions = 0, top = 0) {{ return this.scrollTo(leftOrOptions, top); }},
      setSelectionRange(start, end, direction = "none") {{
        applySelection(start, end, direction);
      }},
      select() {{
        applySelection(0, selectionLength(), "forward");
      }},
      click() {{
        if (this.disabled || this.hidden) return;
        this.focus();
        const event = createEvent("click", {{ bubbles: true, cancelable: true }});
        if (!dispatchTarget(this, event)) return;
        if (this.tagName === "INPUT") {{
          const type = String(entry.attributes.type || "text").toLowerCase();
          if (type === "checkbox") checked = !checked;
          if (type === "radio") checked = true;
        }}
        pushCommand({{ kind: "click", node_index: entry.nodeIndex }});
      }},
      submit() {{
        if (this.tagName !== "FORM") throw new TypeError("submit requires a form");
        pushCommand({{ kind: "submitForm", node_index: entry.nodeIndex }});
      }},
      requestSubmit() {{
        if (this.tagName !== "FORM") throw new TypeError("requestSubmit requires a form");
        const submitter = arguments.length === 0 ? null : arguments[0];
        if (submitter !== null && (typeof submitter !== "object" || typeof submitter.nodeIndex !== "number")) {{
          throw new TypeError("requestSubmit submitter must be an element");
        }}
        pushCommand({{ kind: "requestSubmitForm", node_index: entry.nodeIndex, submitter_index: submitter === null ? null : submitter.nodeIndex }});
      }},
      checkValidity() {{
        const valid = validityFlags(entry).valid;
        if (!valid) pushCommand({{ kind: "checkValidity", node_index: entry.nodeIndex }});
        return valid;
      }},
      reportValidity() {{
        const valid = validityFlags(entry).valid;
        if (!valid) pushCommand({{ kind: "reportValidity", node_index: entry.nodeIndex }});
        return valid;
      }},
      setCustomValidity(message) {{
        const value = String(message);
        entry.customValidity = value;
        if (!entry.validity) entry.validity = {{}};
        entry.validity.customError = value.length > 0;
        entry.validity.valid = validityFlags(entry).valid;
        pushCommand({{ kind: "setCustomValidity", node_index: entry.nodeIndex, message: value }});
      }},
      setAttribute(name, value) {{
        const key = String(name).toLowerCase();
        const stringValue = String(value);
        entry.attributes[key] = stringValue;
        delete entry.attributeNamespaces[key];
        if (["src", "srcset", "sizes"].includes(key) && element.tagName === "IMG") resetImageState(stringValue === "" && key === "src");
        if (key === "disabled") disabled = true;
        if (key === "hidden") hidden = true;
        if (key === "multiple") multiple = true;
        element.__glassSyncContent();
        pushCommand({{ kind: "setAttribute", node_index: entry.nodeIndex, name: key, value: stringValue, namespace_uri: "" }});
      }},
      setAttributeNS(namespace, qualifiedName, value) {{
        setNamespacedAttribute(namespace, qualifiedName, value);
      }},
      removeAttribute(name) {{
        const key = String(name).toLowerCase();
        delete entry.attributes[key];
        delete entry.attributeNamespaces[key];
        if (["src", "srcset", "sizes"].includes(key) && element.tagName === "IMG") resetImageState(key === "src");
        if (key === "disabled") disabled = false;
        if (key === "hidden") hidden = false;
        if (key === "multiple") multiple = false;
        element.__glassSyncContent();
        pushCommand({{ kind: "removeAttribute", node_index: entry.nodeIndex, name: key, namespace_uri: "" }});
      }},
      removeAttributeNS(namespace, name) {{
        removeNamespacedAttribute(namespace, name);
      }},
      appendChild(child) {{
        if (child && child.__glassFragment === true) {{
          const children = child.__glassChildren.slice();
          for (const fragmentChild of children) element.appendChild(fragmentChild);
          child.__glassChildren = [];
          return child;
        }}
        if (!child || typeof child.nodeIndex !== "number") throw new TypeError("child must be a native element");
        if (child === element) throw new TypeError("a node cannot contain itself");
        let ancestor = element;
        while (ancestor) {{
          if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
          ancestor = ancestor.__glassParent || null;
        }}
        const oldParent = child.__glassParent || null;
        if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
          queueFragmentChildRemoval(oldParent, child);
          oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
          if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
        }}
        element.__glassChildren = element.__glassChildren.filter(candidate => candidate !== child);
        element.__glassChildren.push(child);
        child.__glassParent = element;
        child.parentIndex = element.nodeIndex;
        element.__glassSyncContent();
        pushCommand({{ kind: "appendChild", parent_index: entry.nodeIndex, child_index: child.nodeIndex }});
        if (nodeIsConnected(element)) executeInsertedScripts(child);
        return child;
      }},
      insertBefore(child, before) {{
        if (child && child.__glassFragment === true) {{
          const children = child.__glassChildren.slice();
          for (const fragmentChild of children) element.insertBefore(fragmentChild, before);
          child.__glassChildren = [];
          return child;
        }}
        if (before == null) return this.appendChild(child);
        if (!child || typeof child.nodeIndex !== "number"
            || typeof before.nodeIndex !== "number") {{
          throw new TypeError("insertBefore requires native elements");
        }}
        if (before.__glassParent !== element) throw new TypeError("reference node is not a child");
        if (child === before) return child;
        let ancestor = element;
        while (ancestor) {{
          if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
          ancestor = ancestor.__glassParent || null;
        }}
        const oldParent = child.__glassParent || null;
        if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
          queueFragmentChildRemoval(oldParent, child);
          oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
          if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
        }}
        element.__glassChildren = element.__glassChildren.filter(candidate => candidate !== child);
        const index = element.__glassChildren.indexOf(before);
        element.__glassChildren.splice(index < 0 ? element.__glassChildren.length : index, 0, child);
        child.__glassParent = element;
        child.parentIndex = element.nodeIndex;
        element.__glassSyncContent();
        pushCommand({{
          kind: "insertBefore",
          parent_index: entry.nodeIndex,
          child_index: child.nodeIndex,
          before_index: before.nodeIndex,
        }});
        if (nodeIsConnected(element)) executeInsertedScripts(child);
        return child;
      }},
      remove() {{
        const parent = element.__glassParent || null;
        if (!parent && element.parentIndex === null) return;
        const commitRemoval = !element.__glassCreated || element.parentIndex !== null;
        queueFragmentChildRemoval(parent, element);
        if (parent && Array.isArray(parent.__glassChildren)) {{
          parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== element);
        }}
        element.__glassParent = null;
        element.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: entry.nodeIndex }});
      }},
      removeChild(child) {{
        if (!child || child.__glassParent !== element) {{
          throw new TypeError("child is not contained by this element");
        }}
        child.remove();
        return child;
      }},
      append(...items) {{
        for (const item of items) element.appendChild(
          item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
            ? item
            : makeDetachedText(String(item)),
        );
      }},
      prepend(...items) {{
        const before = element.__glassChildren[0] || null;
        for (const item of items) element.insertBefore(
          item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
            ? item
            : makeDetachedText(String(item)),
          before,
        );
      }},
      before(...items) {{
        const parent = element.__glassParent;
        if (!parent || typeof parent.insertBefore !== "function") return;
        for (const item of items) parent.insertBefore(
          item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
            ? item
            : makeDetachedText(String(item)),
          element,
        );
      }},
      after(...items) {{
        const parent = element.__glassParent;
        if (!parent || typeof parent.insertBefore !== "function") return;
        const before = element.nextSibling;
        for (const item of items) parent.insertBefore(
          item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
            ? item
            : makeDetachedText(String(item)),
          before,
        );
      }},
      replaceWith(...items) {{
        const parent = element.__glassParent;
        if (!parent || typeof parent.insertBefore !== "function") return;
        for (const item of items) parent.insertBefore(
          item && (item.nodeType === 1 || item.nodeType === 3 || item.nodeType === 8 || item.__glassFragment === true)
            ? item
            : makeDetachedText(String(item)),
          element,
        );
        element.remove();
      }},
      replaceChildren(...items) {{
        for (const child of element.__glassChildren.slice()) child.remove();
        element.append(...items);
      }}
    }};
    Object.defineProperty(element, "__glassChildren", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: [],
    }});
    Object.defineProperty(element, "__glassParent", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: null,
    }});
    Object.defineProperty(element, "__glassCreated", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: false,
    }});
    Object.defineProperty(element, "__glassSyncContent", {{
      enumerable: false,
      configurable: false,
      value(clearEmpty = false) {{
        if (element.__glassChildren.length > 0) {{
          innerHtml = element.__glassChildren.map(child => child.__glassMarkup).join("");
          textContent = element.__glassChildren.map(child => child.__glassTextValue).join("");
        }} else if (clearEmpty) {{
          innerHtml = "";
          textContent = "";
        }}
        if (element.__glassParent && typeof element.__glassParent.__glassSyncContent === "function") element.__glassParent.__glassSyncContent(clearEmpty);
      }},
    }});
    Object.defineProperty(element, "__glassTextValue", {{
      enumerable: false,
      configurable: false,
      get() {{ return textContent; }},
    }});
    Object.defineProperty(element, "__glassMarkup", {{
      enumerable: false,
      configurable: false,
      get() {{
        const attributes = Object.keys(entry.attributes)
          .sort()
          .map(name => " " + name + "=\"" + escapeHtmlText(entry.attributes[name]).replace(/\"/g, "&quot;") + "\"")
          .join("");
        const opening = "<" + element.localName + attributes + ">";
        if (["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"].includes(element.localName)) return opening;
        const content = element.__glassChildren.length > 0
          ? element.__glassChildren.map(child => child.__glassMarkup).join("")
          : innerHtml;
        return opening + content + "</" + element.localName + ">";
      }},
    }});
    Object.defineProperty(element, "outerHTML", {{
      enumerable: true,
      configurable: false,
      get() {{ return element.__glassMarkup; }},
      set(next) {{
        const value = String(next);
        if (value.length > storageValueLimit) throw new RangeError("native element outerHTML exceeds its limit");
        const owner = element.__glassParent || null;
        if (!owner || typeof owner.insertBefore !== "function") throw new TypeError("outerHTML requires an attached element");
        const fragment = document.createDocumentFragment();
        populateDetachedFragment(fragment, value, makeDetachedElement, makeDetachedText, makeDetachedComment);
        for (const child of fragment.__glassChildren.slice()) owner.insertBefore(child, element);
        element.remove();
      }},
    }});
    Object.defineProperty(element, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (element.__glassParent) return element.__glassParent.nodeType === 1 ? element.__glassParent : null;
        if (element.__glassCreated) return null;
        if (element.parentIndex === null) return null;
        const current = globalThis.__glassHostElements;
        if (!(current instanceof Map) || current.get(element.nodeIndex) !== element) return null;
        return current.get(element.parentIndex) || null;
      }},
    }});
    Object.defineProperty(element, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (element.__glassParent) return element.__glassParent;
        const owner = globalThis.document;
        return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(element)
          ? owner
          : null;
      }},
    }});
    Object.defineProperty(element, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get() {{ return globalThis.document || null; }},
    }});
    installAttributeNodeSurface(element, () => globalThis.document || null);
    installCommonAttributeProperties(element, {{
      disabled: () => disabled,
      hidden: () => hidden,
      multiple: () => multiple,
      imageComplete: () => imageComplete,
      imageNaturalWidth: () => imageNaturalWidth,
      imageNaturalHeight: () => imageNaturalHeight,
      imageCurrentSrc: () => imageCurrentSrc,
      imageReset: resetImageState,
    }});
    Object.defineProperty(element, "__glassAttributeSource", {{
      enumerable: false,
      configurable: false,
      value: () => entry.attributes,
    }});
    Object.defineProperty(element, "__glassAttributeNamespace", {{
      enumerable: false,
      configurable: false,
      value: (name) => attributeNamespaceForEntry(entry, name),
    }});
    for (const property of ["textContent", "innerText"]) {{
      Object.defineProperty(element, property, {{
        enumerable: true,
        configurable: false,
        get() {{ return textContent; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native element textContent exceeds its limit");
          for (const child of element.__glassChildren) {{
            child.__glassParent = null;
            child.parentIndex = null;
          }}
          element.__glassChildren = [];
          innerHtml = "";
          textContent = "";
          suppressHostCommands += 1;
          try {{
            if (value) element.appendChild(makeDetachedText(value));
          }} finally {{
            suppressHostCommands -= 1;
          }}
          element.__glassSyncContent(true);
          pushCommand({{ kind: "setTextContent", node_index: entry.nodeIndex, value }});
        }},
      }});
    }}
    Object.defineProperty(element, "innerHTML", {{
      enumerable: true,
      configurable: false,
      get() {{ return innerHtml; }},
      set(next) {{
        const value = String(next);
        if (value.length > storageValueLimit) throw new RangeError("native element innerHTML exceeds its limit");
        for (const child of element.__glassChildren) {{
          child.__glassParent = null;
          child.parentIndex = null;
        }}
        element.__glassChildren = [];
        innerHtml = "";
        textContent = "";
        suppressHostCommands += 1;
        try {{
          populateDetachedFragment(element, value, makeDetachedElement, makeDetachedText, makeDetachedComment);
        }} finally {{
          suppressHostCommands -= 1;
        }}
        element.__glassSyncContent(true);
        pushCommand({{ kind: "setInnerHtml", node_index: entry.nodeIndex, value }});
      }},
    }});
    value = element.value;
    Object.defineProperty(element, "value", {{
      enumerable: true,
      configurable: false,
      get() {{ return value; }},
      set(next) {{
        const nextValue = String(next);
        const inputType = entry.tagName.toLowerCase() === "input"
          ? String(entry.attributes.type || "text").toLowerCase()
          : "";
        if (inputType === "file") {{
          if (nextValue !== "") throw new DOMExceptionNative("This input element accepts a filename only when selected by the user", "InvalidStateError");
          value = "";
          files = makeNativeFileList([]);
          pushCommand({{ kind: "clearFileInput", node_index: entry.nodeIndex }});
          return;
        }}
        value = nextValue;
        if (selectionStart !== null) {{
          selectionStart = selectionLength();
          selectionEnd = selectionStart;
          selectionDirection = "none";
        }}
        pushCommand({{ kind: "setValue", node_index: entry.nodeIndex, value }});
      }}
    }});
    Object.defineProperty(element, "selectionStart", {{
      enumerable: true,
      configurable: false,
      get() {{ return selectionStart ?? entry.selectionStart ?? null; }},
      set(next) {{
        if (selectionStart == null) throw new TypeError("selection is unavailable on this element");
        applySelection(next, selectionEnd, selectionDirection);
      }},
    }});
    Object.defineProperty(element, "selectionEnd", {{
      enumerable: true,
      configurable: false,
      get() {{ return selectionEnd ?? entry.selectionEnd ?? null; }},
      set(next) {{
        if (selectionEnd == null) throw new TypeError("selection is unavailable on this element");
        applySelection(selectionStart, next, selectionDirection);
      }},
    }});
    Object.defineProperty(element, "selectionDirection", {{
      enumerable: true,
      configurable: false,
      get() {{ return selectionDirection ?? entry.selectionDirection ?? "none"; }},
      set(next) {{
        applySelection(selectionStart, selectionEnd, next);
      }},
    }});
    let checked = element.checked;
    Object.defineProperty(element, "checked", {{
      enumerable: true,
      configurable: false,
      get() {{ return checked; }},
      set(next) {{
        checked = Boolean(next);
        pushCommand({{ kind: "setChecked", node_index: entry.nodeIndex, checked }});
      }}
    }});
    let selected = element.selected;
    Object.defineProperty(element, "selected", {{
      enumerable: true,
      configurable: false,
      get() {{ return selected; }},
      set(next) {{
        selected = Boolean(next);
        pushCommand({{ kind: "setSelected", node_index: entry.nodeIndex, selected }});
      }}
    }});
    Object.defineProperty(element, "__glassSetScrollState", {{
      enumerable: false,
      configurable: false,
      value(nextLeft, nextTop) {{
        scrollLeft = Number(nextLeft) || 0;
        scrollTop = Number(nextTop) || 0;
      }},
    }});
    Object.defineProperty(element, "__glassComputedStyle", {{
      enumerable: false,
      configurable: false,
      value() {{ return computedStyle; }},
    }});
    Object.defineProperty(element, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        entry = nextEntry;
        computedStyle = nextEntry.computedStyle && typeof nextEntry.computedStyle === "object"
          ? nextEntry.computedStyle
          : {{}};
        if (!entry.attributeNamespaces || typeof entry.attributeNamespaces !== "object") entry.attributeNamespaces = {{}};
        element.nodeIndex = nextEntry.nodeIndex;
        element.parentIndex = nextEntry.parentIndex;
        element.tagName = tagNameForEntry(nextEntry);
        element.nodeName = tagNameForEntry(nextEntry);
        element.localName = nextEntry.tagName.toLowerCase();
        textContent = String(nextEntry.text || "");
        innerHtml = String(nextEntry.innerHtml || "");
        disabled = Boolean(nextEntry.disabled);
        hidden = Boolean(nextEntry.hidden);
        element.focused = nextEntry.focused;
        multiple = Object.prototype.hasOwnProperty.call(nextEntry.attributes, "multiple");
        value = nextEntry.value === null
          ? (nextEntry.tagName.toLowerCase() === "option"
            ? (nextEntry.attributes.value === undefined ? nextEntry.text : nextEntry.attributes.value)
            : "")
          : nextEntry.value;
        selectionStart = nextEntry.selectionStart;
        selectionEnd = nextEntry.selectionEnd;
        selectionDirection = nextEntry.selectionDirection || "none";
        checked = nextEntry.checked;
        selected = nextEntry.selected;
        files = makeNativeFileList(nextEntry.files);
        imageComplete = nextEntry.imageComplete === undefined
          ? !Object.prototype.hasOwnProperty.call(nextEntry.attributes, "src")
          : Boolean(nextEntry.imageComplete);
        imageNaturalWidth = Number(nextEntry.imageNaturalWidth) || 0;
        imageNaturalHeight = Number(nextEntry.imageNaturalHeight) || 0;
        imageCurrentSrc = String(nextEntry.imageCurrentSrc || "");
        scrollLeft = Number(nextEntry.scrollX) || 0;
        scrollTop = Number(nextEntry.scrollY) || 0;
        if (typeof element.__glassSyncAttributeNodes === "function") element.__glassSyncAttributeNodes();
      }}
    }});
    return element;
  }};
  const makeDetachedElement = (tagName, namespace = HTML_NAMESPACE) => {{
    const normalized = String(tagName).toLowerCase();
    if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalized)) throw new TypeError("invalid element name");
    const namespaceURI = normalizeElementNamespace(namespace);
    const nodeIndex = allocateTemporaryNodeIndex();
    const entry = {{
      nodeIndex,
      parentIndex: null,
      formOwnerIndex: null,
      tagName: normalized,
      namespaceUri: namespaceURI,
      attributes: {{}},
      attributeNamespaces: {{}},
      text: "",
      innerHtml: "",
      value: null,
      files: [],
      checked: false,
      selected: false,
      disabled: false,
      hidden: false,
      focused: false,
      validity: {{ valid: true }},
      validationMessage: "",
      customValidity: "",
      willValidate: false,
      selectionStart: null,
      selectionEnd: null,
      selectionDirection: "none",
      imageComplete: true,
      imageNaturalWidth: 0,
      imageNaturalHeight: 0,
      imageCurrentSrc: "",
    }};
    const element = makeElement(entry);
    element.__glassCreated = true;
    Object.defineProperty(element, "__glassDynamicScriptStarted", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: false,
    }});
    defineTreeAccessors(element);
    installClassList(element);
    installElementStyleAndDataset(element);
    mutationCreatedNodes.set(nodeIndex, element);
    scriptNodeObjects.set(nodeIndex, element);
    pushCommand({{
      kind: "createElement",
      node_index: nodeIndex,
      tag_name: normalized,
      namespace_uri: namespaceURI === null ? "" : namespaceURI,
    }});
    return element;
  }};
  const nodeIsConnected = (node) => {{
    let current = node;
    for (let depth = 0; current && depth <= {max_commands}; depth += 1) {{
      if (current === globalThis.document || rootChildren.includes(current)) return true;
      current = current.__glassParent || null;
    }}
    return false;
  }};
  const executeInsertedScripts = (node) => {{
    if (suppressHostCommands > 0 || !node) return;
    if (Number(node.nodeType) === 1 && String(node.localName || "").toLowerCase() === "script"
        && node.__glassDynamicScriptStarted === false) {{
      node.__glassDynamicScriptStarted = true;
      const type = String(node.getAttribute("type") || "").trim().toLowerCase();
      const source = String(node.textContent || "");
      const classic = type === "" || type === "text/javascript" || type === "application/javascript";
      if (!node.getAttribute("src") && classic && source) {{
        pushCommand({{ kind: "startScript", node_index: node.nodeIndex }});
        try {{
          (0, eval)(source);
        }} catch (error) {{
          dispatchTarget(node, createEvent("error"));
        }}
      }}
    }}
    for (const child of node.__glassChildren || []) executeInsertedScripts(child);
  }};
  const makeDetachedText = (value) => {{
    let textContent = String(value);
    if (textContent.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
    let nodeIndex = allocateTemporaryNodeIndex();
      const text = {{
      nodeIndex,
      parentIndex: null,
      nodeType: 3,
      nodeName: "#text",
      nodeValue: textContent,
      remove() {{
        const parent = text.__glassParent || null;
        if (!parent && text.parentIndex === null) return;
        const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
        queueFragmentChildRemoval(parent, text);
        if (parent && Array.isArray(parent.__glassChildren)) {{
          parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
        }}
        text.__glassParent = null;
        text.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: nodeIndex }});
      }},
    }};
    Object.defineProperty(text, "__glassParent", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: null,
    }});
    Object.defineProperty(text, "__glassCreated", {{
      enumerable: false,
      configurable: false,
      writable: true,
      value: true,
    }});
    Object.defineProperty(text, "__glassTextValue", {{
      enumerable: false,
      configurable: false,
      get() {{ return textContent; }},
    }});
    Object.defineProperty(text, "__glassMarkup", {{
      enumerable: false,
      configurable: false,
      get() {{ return escapeHtmlText(textContent); }},
    }});
    Object.defineProperty(text, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        text.nodeIndex = nodeIndex;
        text.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }},
    }});
    Object.defineProperty(text, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{ return text.__glassParent || null; }},
    }});
    Object.defineProperty(text, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (text.__glassParent) return text.__glassParent;
        const owner = globalThis.document;
        return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
          ? owner
          : null;
      }},
    }});
    Object.defineProperty(text, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get() {{ return globalThis.document || null; }},
    }});
    Object.defineProperty(text, "nodeValue", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ text.textContent = next; }},
    }});
    Object.defineProperty(text, "textContent", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{
        const value = String(next);
        if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
        textContent = value;
        if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent();
        pushCommand({{ kind: "setTextContent", node_index: nodeIndex, value }});
      }},
    }});
    try {{ Object.setPrototypeOf(text, TextNative.prototype); }} catch (_error) {{}}
    defineTreeAccessors(text);
    mutationCreatedNodes.set(nodeIndex, text);
    scriptNodeObjects.set(nodeIndex, text);
    pushCommand({{ kind: "createTextNode", node_index: nodeIndex, value: textContent }});
    return text;
  }};
  const makeDetachedComment = (value) => {{
    let textContent = String(value);
    if (textContent.length > {storage_value_limit}) throw new RangeError("native comment node exceeds its limit");
    let nodeIndex = allocateTemporaryNodeIndex();
    const comment = {{
      nodeIndex,
      parentIndex: null,
      nodeType: 8,
      nodeName: "#comment",
      nodeValue: textContent,
      remove() {{
        const parent = comment.__glassParent || null;
        if (!parent && comment.parentIndex === null) return;
        const commitRemoval = !comment.__glassCreated || comment.parentIndex !== null;
        queueFragmentChildRemoval(parent, comment);
        if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== comment);
        comment.__glassParent = null;
        comment.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: nodeIndex }});
      }},
    }};
    Object.defineProperty(comment, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
    Object.defineProperty(comment, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
    Object.defineProperty(comment, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return ""; }} }});
    Object.defineProperty(comment, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return "<!--" + textContent + "-->"; }} }});
    Object.defineProperty(comment, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        comment.nodeIndex = nodeIndex;
        comment.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }},
    }});
    Object.defineProperty(comment, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{
        let current = comment.__glassParent || null;
        while (current && current.nodeType !== 1) current = current.__glassParent || null;
        return current || null;
      }},
    }});
    Object.defineProperty(comment, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (comment.__glassParent) return comment.__glassParent;
        const owner = globalThis.document;
        return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(comment)
          ? owner
          : null;
      }},
    }});
    Object.defineProperty(comment, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return globalThis.document || null; }} }});
    Object.defineProperty(comment, "nodeValue", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ comment.textContent = next; }},
    }});
    Object.defineProperty(comment, "textContent", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{
        const nextValue = String(next);
        if (nextValue.length > {storage_value_limit}) throw new RangeError("native comment node exceeds its limit");
        textContent = nextValue;
        if (comment.__glassParent && typeof comment.__glassParent.__glassSyncContent === "function") comment.__glassParent.__glassSyncContent();
        pushCommand({{ kind: "setTextContent", node_index: nodeIndex, value: nextValue }});
      }},
    }});
    Object.defineProperty(comment, "data", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ comment.textContent = next; }},
    }});
    try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
    defineTreeAccessors(comment);
    mutationCreatedNodes.set(nodeIndex, comment);
    scriptNodeObjects.set(nodeIndex, comment);
    pushCommand({{ kind: "createComment", node_index: nodeIndex, value: textContent }});
    return comment;
  }};
  const makeDetachedDocumentType = (name, publicId = "", systemId = "") => {{
    let normalizedName = String(name);
    if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalizedName)) throw new TypeError("invalid document type name");
    let publicIdentifier = String(publicId);
    let systemIdentifier = String(systemId);
    if (publicIdentifier.length > {storage_value_limit} || systemIdentifier.length > {storage_value_limit}) throw new RangeError("native document type identifier exceeds its limit");
    const nodeIndex = allocateTemporaryNodeIndex();
    const documentType = {{
      nodeIndex,
      parentIndex: null,
      nodeType: 10,
      nodeName: normalizedName,
      nodeValue: null,
      name: normalizedName,
      publicId: publicIdentifier,
      systemId: systemIdentifier,
      textContent: null,
      __glassChildren: [],
      __glassParent: null,
      remove() {{
        const parent = documentType.__glassParent || null;
        if (!parent && documentType.parentIndex === null) return;
        const commitRemoval = !documentType.__glassCreated || documentType.parentIndex !== null;
        if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== documentType);
        documentType.__glassParent = null;
        documentType.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: nodeIndex }});
      }},
    }};
    Object.defineProperty(documentType, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
    Object.defineProperty(documentType, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{
      const publicPart = publicIdentifier ? " PUBLIC \"" + publicIdentifier + "\"" : "";
      const systemPart = systemIdentifier ? (publicPart ? " \"" + systemIdentifier + "\"" : " SYSTEM \"" + systemIdentifier + "\"") : "";
      return "<!DOCTYPE " + normalizedName + publicPart + systemPart + ">";
    }} }});
    Object.defineProperty(documentType, "__glassRefresh", {{ enumerable: false, configurable: false, value(nextEntry) {{
      documentType.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
      normalizedName = String(nextEntry.nodeName || normalizedName);
      documentType.name = normalizedName;
      documentType.nodeName = documentType.name;
      publicIdentifier = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
      systemIdentifier = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
      documentType.publicId = publicIdentifier;
      documentType.systemId = systemIdentifier;
    }} }});
    Object.defineProperty(documentType, "parentNode", {{ enumerable: false, configurable: false, get() {{
      if (documentType.__glassParent) return documentType.__glassParent;
      const owner = globalThis.document;
      return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(documentType) ? owner : null;
    }} }});
    Object.defineProperty(documentType, "parentElement", {{ enumerable: false, configurable: false, get() {{ return null; }} }});
    Object.defineProperty(documentType, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return globalThis.document || null; }} }});
    try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
    defineTreeAccessors(documentType);
    mutationCreatedNodes.set(nodeIndex, documentType);
    scriptNodeObjects.set(nodeIndex, documentType);
    pushCommand({{ kind: "createDocumentType", node_index: nodeIndex, name: normalizedName, public_id: publicIdentifier, system_id: systemIdentifier }});
    return documentType;
  }};
  const makeDocumentFragment = () => {{
    const fragment = {{
      nodeType: 11,
      nodeName: "#document-fragment",
      __glassFragment: true,
      __glassChildren: [],
      __glassParent: null,
      appendChild(child) {{
        if (child === this) throw new TypeError("a node cannot contain itself");
        if (child && child.__glassFragment === true) {{
          const children = child.__glassChildren.slice();
          for (const fragmentChild of children) this.appendChild(fragmentChild);
          child.__glassChildren = [];
          return child;
        }}
        if (!child || ![1, 3, 8].includes(Number(child.nodeType)))
          throw new TypeError("DocumentFragment children must be elements or text nodes");
        const oldParent = child.__glassParent || null;
        if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
          queueFragmentChildRemoval(oldParent, child);
          oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
          if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
        }}
        const oldParentIndex = child.parentIndex;
        this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
        this.__glassChildren.push(child);
        child.__glassParent = this;
        child.parentIndex = null;
        if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
          pushCommand({{ kind: "removeNode", node_index: child.nodeIndex }});
        }}
        queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling: this.__glassChildren.length > 1 ? this.__glassChildren[this.__glassChildren.length - 2] : null, nextSibling: null }});
        return child;
      }},
      insertBefore(child, before) {{
        if (child === this) throw new TypeError("a node cannot contain itself");
        if (before == null) return this.appendChild(child);
        if (child && child.__glassFragment === true) {{
          const children = child.__glassChildren.slice();
          for (const fragmentChild of children) this.insertBefore(fragmentChild, before);
          child.__glassChildren = [];
          return child;
        }}
        if (!child || ![1, 3, 8].includes(Number(child.nodeType)))
          throw new TypeError("DocumentFragment children must be elements or text nodes");
        if (before.__glassParent !== this) throw new TypeError("reference node is not a child");
        const oldParent = child.__glassParent || null;
        if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
          queueFragmentChildRemoval(oldParent, child);
          oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
          if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
        }}
        const oldParentIndex = child.parentIndex;
        this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
        const index = this.__glassChildren.indexOf(before);
        const previousSibling = index > 0 ? this.__glassChildren[index - 1] : null;
        const nextSibling = before;
        this.__glassChildren.splice(index < 0 ? this.__glassChildren.length : index, 0, child);
        child.__glassParent = this;
        child.parentIndex = null;
        if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
          pushCommand({{ kind: "removeNode", node_index: child.nodeIndex }});
        }}
        queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling, nextSibling }});
        return child;
      }},
    }};
    Object.defineProperty(fragment, "__glassTextValue", {{
      enumerable: false,
      configurable: false,
      get() {{ return fragment.__glassChildren.map(child => child.__glassTextValue || "").join(""); }},
    }});
    Object.defineProperty(fragment, "__glassMarkup", {{
      enumerable: false,
      configurable: false,
      get() {{ return fragment.__glassChildren.map(child => child.__glassMarkup || "").join(""); }},
    }});
    Object.defineProperty(fragment, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get() {{ return globalThis.document || null; }},
    }});
    Object.defineProperty(fragment, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{ return null; }},
    }});
    Object.defineProperty(fragment, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{ return null; }},
    }});
    Object.defineProperty(fragment, "textContent", {{
      enumerable: true,
      configurable: false,
      get() {{ return fragment.__glassTextValue; }},
      set(next) {{
        for (const child of fragment.__glassChildren.slice()) child.remove();
        const value = String(next);
        if (value) fragment.appendChild(fragment.ownerDocument.createTextNode(value));
        }},
      }});
    Object.defineProperty(fragment, "innerHTML", {{
      enumerable: true,
      configurable: false,
      get() {{ return fragment.__glassMarkup; }},
      set(next) {{
        const value = String(next);
        if (value.length > storageValueLimit) throw new RangeError("native fragment innerHTML exceeds its limit");
        for (const child of fragment.__glassChildren.slice()) child.remove();
        populateDetachedFragment(fragment, value, makeDetachedElement, makeDetachedText, makeDetachedComment);
      }},
    }});
    defineTreeAccessors(fragment);
    const constructor = globalThis.DocumentFragment;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(fragment, constructor.prototype); }} catch (_error) {{}}
    }}
    return fragment;
  }};
  const makeSnapshotText = (initialEntry) => {{
    const isComment = Number(initialEntry.nodeType) === 8;
    let textContent = String(initialEntry.nodeValue || "");
    const text = {{
      nodeIndex: initialEntry.nodeIndex,
      parentIndex: initialEntry.parentIndex == null ? null : initialEntry.parentIndex,
      nodeType: isComment ? 8 : 3,
      nodeName: isComment ? "#comment" : "#text",
      nodeValue: textContent,
      remove() {{
        const parent = text.__glassParent || null;
        if (!parent && text.parentIndex === null) return;
        const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
        queueFragmentChildRemoval(parent, text);
        if (parent && Array.isArray(parent.__glassChildren)) {{
          parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
        }}
        text.__glassParent = null;
        text.parentIndex = null;
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) pushCommand({{ kind: "removeNode", node_index: text.nodeIndex }});
      }},
    }};
    Object.defineProperty(text, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
    Object.defineProperty(text, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: false }});
    Object.defineProperty(text, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return isComment ? "" : textContent; }} }});
    Object.defineProperty(text, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return isComment ? "<!--" + textContent + "-->" : escapeHtmlText(textContent); }} }});
    Object.defineProperty(text, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{
        let current = text.__glassParent || null;
        while (current && current.nodeType !== 1) current = current.__glassParent || null;
        return current || null;
      }},
    }});
    Object.defineProperty(text, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{
        if (text.__glassParent) return text.__glassParent;
        const owner = globalThis.document;
        return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
          ? owner
          : null;
      }},
    }});
    Object.defineProperty(text, "nodeValue", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ text.textContent = next; }},
    }});
    Object.defineProperty(text, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return globalThis.document || null; }} }});
    Object.defineProperty(text, "textContent", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{
        const value = String(next);
        if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
        textContent = value;
        if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent();
        pushCommand({{ kind: "setTextContent", node_index: text.nodeIndex, value }});
      }},
    }});
    Object.defineProperty(text, "data", {{
      enumerable: true,
      configurable: false,
      get() {{ return textContent; }},
      set(next) {{ text.textContent = next; }},
    }});
    Object.defineProperty(text, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        text.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }},
    }});
    return text;
  }};
  const previousElements = globalThis.__glassHostElements instanceof Map
    ? globalThis.__glassHostElements
    : new Map();
  const previousNodes = globalThis.__glassHostNodes instanceof Map
    ? globalThis.__glassHostNodes
    : new Map();
  const snapshotNodes = Array.isArray(state.nodes) ? state.nodes : [];
  const elements = state.elements.map((entry) => {{
    const existing = scriptNodeAliasesByIndex.get(entry.nodeIndex) || previousElements.get(entry.nodeIndex);
    if (existing && typeof existing.__glassRefresh === "function") {{
      existing.__glassRefresh(entry);
      return existing;
    }}
    return makeElement(entry);
  }});
  const elementsByIndex = new Map(elements.map((element) => [element.nodeIndex, element]));
  const textNodes = snapshotNodes
    .filter((entry) => entry && Number(entry.nodeType) === 3)
    .map((entry) => {{
      const existing = previousNodes.get(entry.nodeIndex);
      const scriptAlias = scriptNodeAliasesByIndex.get(entry.nodeIndex);
      const reusable = scriptAlias || existing;
      if (reusable && typeof reusable.__glassRefresh === "function") {{
        reusable.__glassRefresh(entry);
        return reusable;
      }}
      return makeSnapshotText(entry);
    }});
  const commentNodes = snapshotNodes
    .filter((entry) => entry && Number(entry.nodeType) === 8)
    .map((entry) => {{
      const existing = previousNodes.get(entry.nodeIndex);
      const scriptAlias = scriptNodeAliasesByIndex.get(entry.nodeIndex);
      const reusable = scriptAlias || existing;
      if (reusable && typeof reusable.__glassRefresh === "function") {{
        reusable.__glassRefresh(entry);
        return reusable;
      }}
      return makeSnapshotText(entry);
    }});
  const documentTypeNodes = snapshotNodes
    .filter((entry) => entry && Number(entry.nodeType) === 10)
    .map((entry) => {{
      const existing = scriptNodeAliasesByIndex.get(entry.nodeIndex) || previousNodes.get(entry.nodeIndex);
      const documentType = existing || {{
        nodeIndex: entry.nodeIndex,
        parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
        nodeType: 10,
        nodeName: String(entry.nodeName || ""),
        nodeValue: null,
        name: String(entry.nodeName || ""),
        publicId: entry.publicId == null ? "" : String(entry.publicId),
        systemId: entry.systemId == null ? "" : String(entry.systemId),
        __glassChildren: [],
        __glassParent: null,
        textContent: null,
        remove() {{
          const parent = documentType.__glassParent || null;
          if (!parent && documentType.parentIndex == null) return;
          const owner = parent || (documentType.parentIndex === 0 ? globalThis.document : null);
          if (owner && Array.isArray(owner.__glassChildren)) owner.__glassChildren = owner.__glassChildren.filter(candidate => candidate !== documentType);
          documentType.__glassParent = null;
          documentType.parentIndex = null;
          pushCommand({{ kind: "removeNode", node_index: documentType.nodeIndex }});
        }},
      }};
      if (!Object.prototype.hasOwnProperty.call(documentType, "__glassRefresh")) {{
        Object.defineProperty(documentType, "__glassRefresh", {{
          enumerable: false,
          configurable: false,
          value(nextEntry) {{
            documentType.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
            documentType.name = String(nextEntry.nodeName || documentType.name || "");
            documentType.nodeName = documentType.name;
            documentType.publicId = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
            documentType.systemId = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
          }},
        }});
      }}
      if (existing && typeof documentType.__glassRefresh === "function") documentType.__glassRefresh(entry);
      documentType.parentIndex = entry.parentIndex == null ? null : entry.parentIndex;
      documentType.name = String(entry.nodeName || "");
      documentType.nodeName = documentType.name;
      documentType.publicId = entry.publicId == null ? "" : String(entry.publicId);
      documentType.systemId = entry.systemId == null ? "" : String(entry.systemId);
      if (!Object.prototype.hasOwnProperty.call(documentType, "ownerDocument")) {{
        Object.defineProperty(documentType, "ownerDocument", {{
          enumerable: false,
          configurable: false,
          get() {{ return globalThis.document || null; }},
        }});
      }}
      if (!Object.prototype.hasOwnProperty.call(documentType, "parentNode")) {{
        Object.defineProperty(documentType, "parentNode", {{
          enumerable: false,
          configurable: false,
          get() {{
            if (documentType.__glassParent) return documentType.__glassParent;
            const owner = globalThis.document;
            return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(documentType)
              ? owner
              : null;
          }},
        }});
      }}
      if (!Object.prototype.hasOwnProperty.call(documentType, "parentElement")) {{
        Object.defineProperty(documentType, "parentElement", {{
          enumerable: false,
          configurable: false,
          get() {{
            const parent = documentType.__glassParent || null;
            return parent && parent.nodeType === 1 ? parent : null;
          }},
        }});
      }}
      return documentType;
    }});
  const nodesByIndex = new Map([
    ...elements.map((element) => [element.nodeIndex, element]),
    ...textNodes.map((text) => [text.nodeIndex, text]),
    ...commentNodes.map((comment) => [comment.nodeIndex, comment]),
    ...documentTypeNodes.map((documentType) => [documentType.nodeIndex, documentType]),
  ]);
  globalThis.__glassHostElements = elementsByIndex;
  globalThis.__glassHostNodes = nodesByIndex;
  for (const element of elements) {{
    element.__glassChildren = [];
    element.__glassParent = null;
  }}
  for (const text of textNodes) {{
    text.__glassChildren = [];
    text.__glassParent = null;
  }}
  for (const comment of commentNodes) {{
    comment.__glassChildren = [];
    comment.__glassParent = null;
  }}
  for (const documentType of documentTypeNodes) {{
    documentType.__glassChildren = [];
    documentType.__glassParent = null;
  }}
  if (snapshotNodes.length > 0) {{
    for (const snapshotNode of snapshotNodes) {{
      const parent = nodesByIndex.get(snapshotNode.nodeIndex);
      if (!parent || !Array.isArray(snapshotNode.children)) continue;
      for (const childIndex of snapshotNode.children) {{
        const child = nodesByIndex.get(childIndex);
        if (!child) continue;
        parent.__glassChildren.push(child);
        child.__glassParent = parent;
      }}
    }}
  }} else {{
    for (const element of elements) {{
      if (element.parentIndex === null) continue;
      const parent = elementsByIndex.get(element.parentIndex);
      if (!parent) continue;
      parent.__glassChildren.push(element);
      element.__glassParent = parent;
    }}
  }}
  globalThis.__glassApplyNativeCommand = (command) => {{
    if (!command || typeof command !== "object") throw new TypeError("native frame command must be an object");
    const current = globalThis.__glassHostNodes;
    const nodeIndex = Number(command.node_index);
    const element = current instanceof Map ? current.get(nodeIndex) : null;
    if (!element) throw new Error("native frame command target is detached");
    switch (String(command.kind)) {{
      case "focus": element.focus(); break;
      case "blur": element.blur(); break;
      case "click": element.click(); break;
      case "setValue": element.value = String(command.value); break;
      case "setSelection": element.setSelectionRange(command.start, command.end, command.direction); break;
      case "setChecked": element.checked = Boolean(command.checked); break;
      case "setSelected": element.selected = Boolean(command.selected); break;
      case "setAttribute":
        if (command.namespace_uri) {{ element.setAttributeNS(command.namespace_uri, command.name, command.value); }}
        else element.setAttribute(command.name, command.value);
        break;
      case "removeAttribute":
        if (command.namespace_uri) {{ element.removeAttributeNS(command.namespace_uri, command.name); }}
        else element.removeAttribute(command.name);
        break;
      case "setTextContent": element.textContent = String(command.value); break;
      case "setInnerHtml": element.innerHTML = String(command.value); break;
      case "removeNode": element.remove(); break;
      case "setCustomValidity": element.setCustomValidity(command.message); break;
      case "checkValidity": element.checkValidity(); break;
      case "reportValidity": element.reportValidity(); break;
      default: throw new TypeError("native frame command is unsupported");
    }}
    return true;
  }};
  globalThis.__glassQueueNativeCommands = (incoming) => {{
    if (!Array.isArray(incoming)) throw new TypeError("native frame command batch must be an array");
    for (const command of incoming) {{
      if (!command || typeof command !== "object") throw new TypeError("native frame command is invalid");
      if (String(command.kind) === "frameScript" || String(command.kind) === "frameScriptBatch") {{
        throw new TypeError("nested native frame commands are not allowed");
      }}
      pushCommand(command);
    }}
    return true;
  }};
  for (const element of elements) {{
    if (!Object.prototype.hasOwnProperty.call(element, "parentElement")) {{
      Object.defineProperty(element, "parentElement", {{
        enumerable: false,
        configurable: false,
        get() {{
          if (element.parentIndex === null) return null;
          const current = globalThis.__glassHostElements;
          return current instanceof Map ? current.get(element.parentIndex) || null : null;
        }},
      }});
      Object.defineProperty(element, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{ return element.parentElement; }},
      }});
    }}
    if (["IFRAME", "FRAME"].includes(element.tagName)
        && !Object.prototype.hasOwnProperty.call(element, "contentWindow")) {{
      Object.defineProperty(element, "contentWindow", {{
        enumerable: false,
        configurable: false,
        get() {{
          const binding = frameBindingForNode(element.nodeIndex);
          return binding
            ? makeFrameWindow(binding, element, globalThis, relationshipTop || globalThis)
            : null;
        }},
      }});
      Object.defineProperty(element, "contentDocument", {{
        enumerable: false,
        configurable: false,
        get() {{
          const binding = frameBindingForNode(element.nodeIndex);
          return binding && binding.sameOrigin
            ? makeFrameDocument(binding, element, globalThis, relationshipTop || globalThis)
            : null;
        }},
      }});
    }}
    if (element.tagName === "SELECT" && !Object.prototype.hasOwnProperty.call(element, "options")) {{
      Object.defineProperty(element, "options", {{
        enumerable: false,
        configurable: false,
        get() {{
          const current = globalThis.__glassHostElements;
          if (!(current instanceof Map)) return [];
          return asHtmlCollection(Array.from(current.values()).filter(option =>
            option.tagName === "OPTION" && option.parentIndex === element.nodeIndex
          ));
        }},
      }});
      Object.defineProperty(element, "selectedOptions", {{
        enumerable: false,
        configurable: false,
        get() {{ return asHtmlCollection(element.options.filter(option => option.selected)); }},
      }});
    }}
  }}
  const setLocalFocus = (target) => {{
    const current = elements.find((element) => element.focused && element !== target) || null;
    if (current) {{
      current.focused = false;
      dispatchTarget(current, createEvent("blur"));
    }}
    if (target.focused) return;
    target.focused = true;
    pushCommand({{ kind: "focus", node_index: target.nodeIndex }});
    dispatchTarget(target, createEvent("focus"));
  }};
  const asNativeCollection = (values, constructorName) => {{
    const constructor = globalThis[constructorName];
    if (typeof constructor === "function" && constructor.prototype) {{
      Object.setPrototypeOf(values, constructor.prototype);
    }}
    return values;
  }};
  const asNodeList = (values) => asNativeCollection(values, "NodeList");
  const asHtmlCollection = (values) => asNativeCollection(values, "HTMLCollection");
  const collectionIndex = (property) => {{
    if (typeof property !== "string" || !/^(0|[1-9][0-9]*)$/.test(property)) return null;
    const index = Number(property);
    return Number.isSafeInteger(index) ? index : null;
  }};
  const liveCollection = (owner, filter, constructorName, includeDescendants = false) => {{
    const target = [];
    const current = () => includeDescendants
      ? descendantsInTree(owner, filter)
      : Array.isArray(owner.__glassChildren)
        ? owner.__glassChildren.filter((child) => child && filter(child))
        : [];
    const proxy = new Proxy(target, {{
      get(_target, property, receiver) {{
        const values = current();
        const index = collectionIndex(property);
        if (index !== null) return values[index];
        if (property === "length") return values.length;
        if (property === "item") return (requested) => {{
          const numeric = Number(requested);
          return Number.isSafeInteger(numeric) && numeric >= 0 ? current()[numeric] || null : null;
        }};
        if (property === "namedItem") return (name) => {{
          const key = String(name);
          return current().find((item) => item.id === key || item.name === key) || null;
        }};
        if (property === Symbol.iterator) return () => current()[Symbol.iterator]();
        if (property === "values") return () => current()[Symbol.iterator]();
        if (property === "keys") return () => current().keys();
        if (property === "entries") return () => current().entries();
        if (property === "forEach") return (callback, thisArg) => current().forEach(callback, thisArg);
        const value = Reflect.get(target, property, receiver);
        return typeof value === "function" ? value.bind(receiver) : value;
      }},
      has(_target, property) {{
        const index = collectionIndex(property);
        if (index !== null) return index < current().length;
        return property === "length" || property === "item" || property === "namedItem"
          || Reflect.has(target, property);
      }},
      ownKeys(_target) {{
        return current().map((_value, index) => String(index)).concat("length");
      }},
      getOwnPropertyDescriptor(_target, property) {{
        const index = collectionIndex(property);
        if (index !== null && index < current().length) {{
          return {{ enumerable: true, configurable: true, value: current()[index], writable: false }};
        }}
        return Reflect.getOwnPropertyDescriptor(target, property);
      }},
    }});
    return asNativeCollection(proxy, constructorName);
  }};
  const defineMissing = (target, name, descriptor) => {{
    if (!Object.prototype.hasOwnProperty.call(target, name)) Object.defineProperty(target, name, descriptor);
  }};
  const descendantsInTree = (owner, predicate) => {{
    const result = [];
    const visit = (node) => {{
      for (const child of Array.isArray(node && node.__glassChildren) ? node.__glassChildren : []) {{
        if (child && child.nodeType === 1 && predicate(child)) result.push(child);
        visit(child);
      }}
    }};
    visit(owner);
    return result;
  }};
  const defineTreeAccessors = (node) => {{
    if (!node || typeof node !== "object") return node;
    const children = () => Array.isArray(node.__glassChildren) ? node.__glassChildren : [];
    const parent = () => node.__glassParent
      || (node.parentNode && node.parentNode !== node && Array.isArray(node.parentNode.__glassChildren)
        ? node.parentNode
        : null);
    defineMissing(node, "childNodes", {{
      enumerable: false,
      configurable: false,
      get() {{ return liveCollection(node, () => true, "NodeList"); }},
    }});
    defineMissing(node, "children", {{
      enumerable: false,
      configurable: false,
      get() {{ return liveCollection(node, (child) => child.nodeType === 1, "HTMLCollection"); }},
    }});
    defineMissing(node, "firstChild", {{
      enumerable: false,
      configurable: false,
      get() {{ return children()[0] || null; }},
    }});
    defineMissing(node, "lastChild", {{
      enumerable: false,
      configurable: false,
      get() {{ const values = children(); return values[values.length - 1] || null; }},
    }});
    defineMissing(node, "firstElementChild", {{
      enumerable: false,
      configurable: false,
      get() {{ return children().find((child) => child.nodeType === 1) || null; }},
    }});
    defineMissing(node, "lastElementChild", {{
      enumerable: false,
      configurable: false,
      get() {{ const values = children().filter((child) => child.nodeType === 1); return values[values.length - 1] || null; }},
    }});
    defineMissing(node, "nextSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        const owner = parent();
        if (!owner) return null;
        const values = Array.isArray(owner.__glassChildren) ? owner.__glassChildren : [];
        const index = values.indexOf(node);
        return index >= 0 ? values[index + 1] || null : null;
      }},
    }});
    defineMissing(node, "previousSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        const owner = parent();
        if (!owner) return null;
        const values = Array.isArray(owner.__glassChildren) ? owner.__glassChildren : [];
        const index = values.indexOf(node);
        return index > 0 ? values[index - 1] : null;
      }},
    }});
    defineMissing(node, "nextElementSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        let sibling = node.nextSibling;
        while (sibling && sibling.nodeType !== 1) sibling = sibling.nextSibling;
        return sibling || null;
      }},
    }});
    defineMissing(node, "previousElementSibling", {{
      enumerable: false,
      configurable: false,
      get() {{
        let sibling = node.previousSibling;
        while (sibling && sibling.nodeType !== 1) sibling = sibling.previousSibling;
        return sibling || null;
      }},
    }});
    defineMissing(node, "hasChildNodes", {{
      enumerable: false,
      configurable: false,
      value() {{ return children().length > 0; }},
    }});
    defineMissing(node, "contains", {{
      enumerable: false,
      configurable: false,
      value(candidate) {{
        if (candidate === node) return true;
        let current = candidate && candidate.__glassParent;
        while (current) {{
          if (current === node) return true;
          if (current.__glassParent) {{
            current = current.__glassParent;
            continue;
          }}
          return current.ownerDocument === node;
        }}
        return Boolean(candidate && candidate.ownerDocument === node
        && node.__glassChildren && node.__glassChildren.includes(candidate));
      }},
    }});
    if ([1, 2, 3, 8, 10, 11].includes(Number(node.nodeType))) {{
      if (node.nodeType === 1 || node.nodeType === 11) {{
        defineMissing(node, "querySelector", {{
          enumerable: false,
          configurable: false,
          value(selector) {{
            return descendantsInTree(node, (candidate) => matchesSelector(candidate, selector))[0] || null;
          }},
        }});
        defineMissing(node, "querySelectorAll", {{
          enumerable: false,
          configurable: false,
          value(selector) {{
            return asNodeList(descendantsInTree(node, (candidate) => matchesSelector(candidate, selector)));
          }},
        }});
        defineMissing(node, "getElementsByTagName", {{
          enumerable: false,
          configurable: false,
          value(name) {{
            const value = String(name).toLowerCase();
            return asHtmlCollection(descendantsInTree(node, (candidate) =>
              value === "*" || candidate.tagName.toLowerCase() === value));
          }},
        }});
        defineMissing(node, "getElementsByClassName", {{
          enumerable: false,
          configurable: false,
          value(name) {{
            const value = String(name).trim();
            if (!value) return asHtmlCollection([]);
            const tokens = value.split(/\s+/);
            return asHtmlCollection(descendantsInTree(node, (candidate) =>
              tokens.every((token) => String(candidate.className).split(/\s+/).includes(token))));
          }},
        }});
      }}
      const insertionText = (value) => node.ownerDocument
        && typeof node.ownerDocument.createTextNode === "function"
        ? node.ownerDocument.createTextNode(String(value))
        : makeDetachedText(String(value));
      const insertionNode = (item) => item && [1, 3, 8, 10, 11].includes(Number(item.nodeType))
        ? item
        : insertionText(item);
      if (node.nodeType === 1 || node.nodeType === 11) {{
        defineMissing(node, "append", {{
          enumerable: false,
          configurable: false,
          value(...items) {{
            for (const item of items) node.appendChild(insertionNode(item));
          }},
        }});
        defineMissing(node, "prepend", {{
          enumerable: false,
          configurable: false,
          value(...items) {{
            const before = node.__glassChildren[0] || null;
            for (const item of items) node.insertBefore(insertionNode(item), before);
          }},
        }});
      }}
      defineMissing(node, "before", {{
        enumerable: false,
        configurable: false,
        value(...items) {{
          const owner = node.__glassParent;
          if (!owner || typeof owner.insertBefore !== "function") return;
          for (const item of items) owner.insertBefore(insertionNode(item), node);
        }},
      }});
      defineMissing(node, "after", {{
        enumerable: false,
        configurable: false,
        value(...items) {{
          const owner = node.__glassParent;
          if (!owner || typeof owner.insertBefore !== "function") return;
          const before = node.nextSibling;
          for (const item of items) owner.insertBefore(insertionNode(item), before);
        }},
      }});
      defineMissing(node, "replaceWith", {{
        enumerable: false,
        configurable: false,
        value(...items) {{
          const owner = node.__glassParent;
          if (!owner || typeof owner.insertBefore !== "function") return;
          for (const item of items) owner.insertBefore(insertionNode(item), node);
          if (typeof node.remove === "function") node.remove();
        }},
      }});
      if (node.nodeType === 1 || node.nodeType === 11) {{
        defineMissing(node, "replaceChildren", {{
          enumerable: false,
          configurable: false,
          value(...items) {{
            for (const child of node.__glassChildren.slice()) child.remove();
            node.append(...items);
          }},
        }});
      }}
    }}
    if (node.nodeType === 1 || node.nodeType === 11) {{
      defineMissing(node, "replaceChild", {{
        enumerable: false,
        configurable: false,
        value(next, oldChild) {{
          if (!oldChild || oldChild.__glassParent !== node) throw new TypeError("old child is not contained by this node");
          if (!next || ![1, 3, 8, 11].includes(Number(next.nodeType))) throw new TypeError("replacement must be a native node");
          if (next === oldChild) return oldChild;
          node.insertBefore(next, oldChild);
          oldChild.remove();
          return oldChild;
        }},
      }});
    }}
    defineMissing(node, "isConnected", {{
      enumerable: false,
      configurable: false,
      get() {{
        const owner = node.ownerDocument;
        if (!owner) return false;
        if (owner.__glassChildren && owner.__glassChildren.includes(node)) return true;
        let current = node.__glassParent || null;
        while (current) {{
          if (owner.__glassChildren && owner.__glassChildren.includes(current)) return true;
          current = current.__glassParent || null;
        }}
        return false;
      }},
    }});
    defineMissing(node, "getRootNode", {{
      enumerable: false,
      configurable: false,
      value(_options) {{
        let current = node;
        for (let depth = 0; depth <= {max_commands}; depth += 1) {{
          if (!current.__glassParent) break;
          current = current.__glassParent;
        }}
        const owner = node.ownerDocument || null;
        return owner && Array.isArray(owner.__glassChildren)
          && owner.__glassChildren.includes(current)
          ? owner
          : current;
      }},
    }});
    defineMissing(node, "cloneNode", {{
      enumerable: false,
      configurable: false,
      value(deep = false) {{
        const owner = node.ownerDocument || null;
        if (!owner) throw new DOMExceptionNative("The node has no owner document", "InvalidStateError");
        const copyChildren = (clone) => {{
          if (!Boolean(deep)) return;
          for (const child of children()) {{
            if (child && typeof child.cloneNode === "function") clone.appendChild(child.cloneNode(true));
          }}
        }};
        if (node.nodeType === 3) return owner.createTextNode(String(node.nodeValue || ""));
        if (node.nodeType === 8) return owner.createComment(String(node.nodeValue || ""));
        if (node.nodeType === 2) {{
          const clone = owner.createAttributeNS(node.namespaceURI || null, node.name);
          clone.value = String(node.value || "");
          return clone;
        }}
        if (node.nodeType === 10) return owner.implementation.createDocumentType(node.name, node.publicId, node.systemId);
        if (node.nodeType === 1) {{
          const clone = owner.createElementNS(node.namespaceURI || null, node.localName);
          for (const name of node.getAttributeNames()) {{
            const value = node.getAttribute(name);
            if (value === null) continue;
            const namespaceURI = typeof node.__glassAttributeNamespace === "function"
              ? node.__glassAttributeNamespace(name)
              : null;
            if (namespaceURI === null) clone.setAttribute(name, value);
            else clone.setAttributeNS(namespaceURI, name, value);
          }}
          copyChildren(clone);
          return clone;
        }}
        if (node.nodeType === 11) {{
          const clone = owner.createDocumentFragment();
          copyChildren(clone);
          return clone;
        }}
        throw new DOMExceptionNative("This native node type cannot be cloned", "NotSupportedError");
      }},
    }});
    defineMissing(node, "isSameNode", {{
      enumerable: false,
      configurable: false,
      value(other) {{ return node === other; }},
    }});
    const equalNodes = (left, right, depth = 0) => {{
      if (left === right) return true;
      if (!right || Number(left.nodeType) !== Number(right.nodeType) || depth > {max_commands}) return false;
      if ([3, 8].includes(Number(left.nodeType))) return String(left.nodeValue || "") === String(right.nodeValue || "");
      if (Number(left.nodeType) === 2) return String(left.name || "") === String(right.name || "")
        && String(left.value || "") === String(right.value || "")
        && String(left.namespaceURI || "") === String(right.namespaceURI || "")
        && String(left.localName || "") === String(right.localName || "");
      if (Number(left.nodeType) === 1) {{
        if (String(left.localName) !== String(right.localName)
            || String(left.namespaceURI || "") !== String(right.namespaceURI || "")) return false;
        const leftNames = left.getAttributeNames();
        const rightNames = right.getAttributeNames();
        if (leftNames.length !== rightNames.length) return false;
        for (const name of leftNames) {{
          if (left.getAttribute(name) !== right.getAttribute(name)) return false;
          const leftNamespace = typeof left.__glassAttributeNamespace === "function"
            ? left.__glassAttributeNamespace(name)
            : null;
          const rightNamespace = typeof right.__glassAttributeNamespace === "function"
            ? right.__glassAttributeNamespace(name)
            : null;
          if (leftNamespace !== rightNamespace) return false;
        }}
      }} else if (Number(left.nodeType) === 10) {{
        return String(left.name || left.nodeName || "") === String(right.name || right.nodeName || "")
          && String(left.publicId || "") === String(right.publicId || "")
          && String(left.systemId || "") === String(right.systemId || "");
      }} else if (![9, 11].includes(Number(left.nodeType))) {{
        return false;
      }}
      const leftChildren = Array.isArray(left.__glassChildren) ? left.__glassChildren : [];
      const rightChildren = Array.isArray(right.__glassChildren) ? right.__glassChildren : [];
      return leftChildren.length === rightChildren.length
        && leftChildren.every((child, index) => equalNodes(child, rightChildren[index], depth + 1));
    }};
    defineMissing(node, "isEqualNode", {{
      enumerable: false,
      configurable: false,
      value(other) {{ return equalNodes(node, other); }},
    }});
    defineMissing(node, "compareDocumentPosition", {{
      enumerable: false,
      configurable: false,
      value(other) {{
        if (!other || typeof other !== "object") return 1 | 32;
        if (node === other) return 0;
        const rootOf = (candidate) => candidate && typeof candidate.getRootNode === "function"
          ? candidate.getRootNode()
          : candidate;
        if (rootOf(node) !== rootOf(other)) return 1 | 32;
        const containsNode = (ancestor, candidate) => {{
          let current = candidate;
          for (let depth = 0; current && depth <= {max_commands}; depth += 1) {{
            current = current.parentNode || null;
            if (current === ancestor) return true;
          }}
          return false;
        }};
        if (containsNode(node, other)) return 4 | 16;
        if (containsNode(other, node)) return 2 | 8;
        const ordered = [];
        const visit = (candidate) => {{
          if (!candidate || ordered.includes(candidate)) return;
          ordered.push(candidate);
          for (const child of Array.isArray(candidate.__glassChildren) ? candidate.__glassChildren : []) visit(child);
        }};
        visit(rootOf(node));
        const left = ordered.indexOf(node);
        const right = ordered.indexOf(other);
        if (left < 0 || right < 0) return 1 | 32;
        return right > left ? 4 : 2;
      }},
    }});
    defineMissing(node, "normalize", {{
      enumerable: false,
      configurable: false,
      value() {{
        if (![1, 9, 11].includes(Number(node.nodeType))) return;
        const normalizeChildren = (owner) => {{
          let previousText = null;
          for (const child of (Array.isArray(owner.__glassChildren) ? owner.__glassChildren : []).slice()) {{
            if (Number(child.nodeType) === 3) {{
              const value = String(child.nodeValue || "");
              if (!value) {{
                child.remove();
              }} else if (previousText) {{
                previousText.data = String(previousText.data || "") + value;
                child.remove();
              }} else {{
                previousText = child;
              }}
            }} else {{
              if (typeof child.normalize === "function") child.normalize();
              previousText = null;
            }}
          }}
        }};
        normalizeChildren(node);
      }},
    }});
    return node;
  }};
  for (const element of elements) defineTreeAccessors(element);
  for (const text of textNodes) defineTreeAccessors(text);
  for (const comment of commentNodes) {{
    defineTreeAccessors(comment);
    try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
  }}
  for (const documentType of documentTypeNodes) {{
    defineTreeAccessors(documentType);
    try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
  }}
  const splitSelectorList = (selector) => {{
    const value = String(selector).trim();
    if (!value) throw new SyntaxError("selector cannot be empty");
    const result = [];
    let part = "";
    let depth = 0;
    let quote = "";
    for (let index = 0; index < value.length; index += 1) {{
      const character = value[index];
      if (quote) {{
        part += character;
        if (character === quote && value[index - 1] !== "\\") quote = "";
        continue;
      }}
      if (character === "'" || character === '"') {{
        quote = character;
        part += character;
      }} else if (character === "[" || character === "(") {{
        depth += 1;
        part += character;
      }} else if (character === "]" || character === ")") {{
        depth -= 1;
        if (depth < 0) throw new SyntaxError("unbalanced selector");
        part += character;
      }} else if (character === "," && depth === 0) {{
        if (!part.trim()) throw new SyntaxError("empty selector list member");
        result.push(part.trim());
        part = "";
      }} else {{
        part += character;
      }}
    }}
    if (quote || depth !== 0 || !part.trim()) throw new SyntaxError("invalid selector");
    result.push(part.trim());
    return result;
  }};
  const parseSelectorChain = (selector) => {{
    const tokens = [];
    let part = "";
    let depth = 0;
    let quote = "";
    let pendingSpace = false;
    const pushSimple = () => {{
      if (!part.trim()) return;
      tokens.push({{ type: "simple", value: part.trim() }});
      part = "";
    }};
    for (let index = 0; index < selector.length; index += 1) {{
      const character = selector[index];
      if (quote) {{
        part += character;
        if (character === quote && selector[index - 1] !== "\\") quote = "";
        continue;
      }}
      if (character === "'" || character === '"') {{
        quote = character;
        part += character;
      }} else if (character === "[" || character === "(") {{
        depth += 1;
        pendingSpace = false;
        part += character;
      }} else if (character === "]" || character === ")") {{
        depth -= 1;
        if (depth < 0) throw new SyntaxError("unbalanced selector");
        part += character;
      }} else if (depth === 0 && character === ">") {{
        pushSimple();
        if (tokens[tokens.length - 1] && tokens[tokens.length - 1].type === "combinator") {{
          throw new SyntaxError("invalid selector combinator");
        }}
        tokens.push({{ type: "combinator", value: ">" }});
        pendingSpace = false;
      }} else if (depth === 0 && /\s/.test(character)) {{
        pushSimple();
        pendingSpace = true;
      }} else {{
        if (pendingSpace && tokens.length > 0
            && tokens[tokens.length - 1].type === "simple") {{
          tokens.push({{ type: "combinator", value: " " }});
        }}
        pendingSpace = false;
        part += character;
      }}
    }}
    pushSimple();
    if (quote || depth !== 0 || tokens.length === 0
        || tokens[0].type !== "simple"
        || tokens[tokens.length - 1].type !== "simple") {{
      throw new SyntaxError("invalid selector chain");
    }}
    return tokens;
  }};
  const selectorAttribute = (element, expression) => {{
    const match = String(expression).trim().match(
      /^([^\s~|^$*!=]+)\s*(?:(!=|[~|^$*]?=)\s*(.*?)\s*)?$/
    );
    if (!match) throw new SyntaxError("invalid attribute selector");
    const name = match[1];
    const operator = match[2] || null;
    let expected = match[3] === undefined ? null : match[3].trim();
    let insensitive = false;
    if (expected && /\s+i$/i.test(expected)) {{
      expected = expected.replace(/\s+i$/i, "").trim();
      insensitive = true;
    }}
    if (expected && ((expected.startsWith('"') && expected.endsWith('"'))
        || (expected.startsWith("'") && expected.endsWith("'")))) {{
      expected = expected.slice(1, -1);
    }}
    const actual = element.getAttribute(name);
    if (!operator) return actual !== null;
    if (actual === null) return operator === "!=";
    const left = insensitive ? actual.toLowerCase() : actual;
    const right = insensitive ? expected.toLowerCase() : expected;
    switch (operator) {{
      case "=": return left === right;
      case "!=": return left !== right;
      case "~=": return left.split(/\s+/).includes(right);
      case "|=": return left === right || left.startsWith(right + "-");
      case "^=": return left.startsWith(right);
      case "$=": return left.endsWith(right);
      case "*=": return left.includes(right);
      default: throw new SyntaxError("unsupported attribute operator");
    }}
  }};
  const elementChildren = (element) => element && Array.isArray(element.__glassChildren)
    ? element.__glassChildren.filter((child) => child && child.nodeType === 1)
    : [];
  const matchesSimpleSelector = (element, selector) => {{
    if (!element || element.nodeType !== 1) return false;
    let rest = String(selector).trim();
    const tag = rest.match(/^([A-Za-z][A-Za-z0-9:_-]*|\*)/);
    if (tag) {{
      if (tag[1] !== "*" && element.tagName.toLowerCase() !== tag[1].toLowerCase()) return false;
      rest = rest.slice(tag[0].length);
    }}
    while (rest.length > 0) {{
      if (rest[0] === "#") {{
        const match = rest.slice(1).match(/^[A-Za-z0-9_\-:]+/);
        if (!match || element.id !== match[0]) return false;
        rest = rest.slice(match[0].length + 1);
      }} else if (rest[0] === ".") {{
        const match = rest.slice(1).match(/^[A-Za-z0-9_\-]+/);
        if (!match || !String(element.className).split(/\s+/).includes(match[0])) return false;
        rest = rest.slice(match[0].length + 1);
      }} else if (rest[0] === "[") {{
        const end = rest.indexOf("]");
        if (end < 0) throw new SyntaxError("unclosed attribute selector");
        if (!selectorAttribute(element, rest.slice(1, end))) return false;
        rest = rest.slice(end + 1);
      }} else if (rest[0] === ":") {{
        const pseudo = rest.match(/^:([A-Za-z-]+)(?:\(([^()]*)\))?/);
        if (!pseudo) throw new SyntaxError("invalid pseudo-class");
        const name = pseudo[1].toLowerCase();
        const argument = pseudo[2];
        const siblings = elementChildren(element.parentElement);
        const position = siblings.indexOf(element) + 1;
        if (name === "not") {{
          if (argument === undefined || matchesSelector(element, argument)) return false;
        }} else if (name === "is" || name === "where") {{
          if (argument === undefined || !matchesSelector(element, argument)) return false;
        }} else if (name === "first-child" && position !== 1) return false;
        else if (name === "last-child" && position !== siblings.length) return false;
        else if (name === "only-child" && siblings.length !== 1) return false;
        else if (name === "nth-child") {{
          const value = String(argument || "").trim().toLowerCase();
          const expected = value === "odd" ? position % 2 === 1
            : value === "even" ? position % 2 === 0
            : Number.isInteger(Number(value)) && position === Number(value);
          if (!expected) return false;
        }} else if (name === "root") {{
          if (!element.ownerDocument || element.ownerDocument.documentElement !== element) return false;
        }} else if (name === "empty") {{
          if (Array.isArray(element.__glassChildren)
              && element.__glassChildren.some((child) => child.nodeType === 1
                || String(child.__glassTextValue || "").length > 0)) return false;
        }} else if (name === "checked" && !element.checked) return false;
        else if (name === "selected" && !element.selected) return false;
        else if (name === "disabled" && !element.disabled) return false;
        else if (name === "enabled" && element.disabled) return false;
        else if (name !== "not" && name !== "is" && name !== "where"
            && name !== "first-child" && name !== "last-child"
            && name !== "only-child" && name !== "nth-child" && name !== "root"
            && name !== "empty" && name !== "checked" && name !== "selected"
            && name !== "disabled" && name !== "enabled") {{
          throw new SyntaxError("unsupported pseudo-class");
        }}
        rest = rest.slice(pseudo[0].length);
      }} else {{
        throw new SyntaxError("invalid selector token");
      }}
    }}
    return true;
  }};
  const matchesSelector = (element, selector) => splitSelectorList(selector).some((member) => {{
    const chain = parseSelectorChain(member);
    const visit = (candidate, index) => {{
      if (!candidate || !matchesSimpleSelector(candidate, chain[index].value)) return false;
      if (index === 0) return true;
      const combinator = chain[index - 1].value;
      if (combinator === ">") return visit(candidate.parentElement, index - 2);
      let ancestor = candidate.parentElement;
      while (ancestor) {{
        if (visit(ancestor, index - 2)) return true;
        ancestor = ancestor.parentElement;
      }}
      return false;
    }};
    return visit(element, chain.length - 1);
  }});
  const isDescendantOf = (element, owner) => {{
    let parent = element && element.parentElement;
    while (parent) {{
      if (parent === owner) return true;
      parent = parent.parentElement;
    }}
    return false;
  }};
  const descendantsMatching = (owner, candidates, selector) =>
    candidates.filter((element) => isDescendantOf(element, owner) && matchesSelector(element, selector));
  const classToken = (value) => {{
    const token = String(value);
    if (!token || /\s/.test(token)) throw new TypeError("class token must be non-empty and whitespace-free");
    return token;
  }};
  const makeClassList = (element) => {{
    const tokens = () => String(element.className || "").split(/\s+/).filter(Boolean);
    const write = (next) => element.setAttribute("class", Array.from(new Set(next)).join(" "));
    const api = {{
      get length() {{ return tokens().length; }},
      item(index) {{
        const value = tokens()[Number(index)];
        return value === undefined ? null : value;
      }},
      contains(value) {{ return tokens().includes(classToken(value)); }},
      add(...values) {{
        const next = tokens();
        for (const value of values) {{
          const token = classToken(value);
          if (!next.includes(token)) next.push(token);
        }}
        write(next);
      }},
      remove(...values) {{
        const removed = new Set(values.map(classToken));
        write(tokens().filter((token) => !removed.has(token)));
      }},
      toggle(value, force) {{
        const token = classToken(value);
        const present = tokens().includes(token);
        const shouldAdd = force === undefined ? !present : Boolean(force);
        if (shouldAdd && !present) write([...tokens(), token]);
        if (!shouldAdd && present) write(tokens().filter((candidate) => candidate !== token));
        return shouldAdd;
      }},
      replace(oldValue, newValue) {{
        const oldToken = classToken(oldValue);
        const newToken = classToken(newValue);
        const next = tokens();
        const index = next.indexOf(oldToken);
        if (index < 0) return false;
        next[index] = newToken;
        write(next);
        return true;
      }},
      toString() {{ return tokens().join(" "); }},
      get value() {{ return tokens().join(" "); }},
      set value(next) {{ write(String(next).split(/\s+/).filter(Boolean)); }},
    }};
    api[Symbol.iterator] = function() {{ return tokens()[Symbol.iterator](); }};
    const constructor = globalThis.DOMTokenList;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(api, constructor.prototype); }} catch (_error) {{}}
    }}
    return api;
  }};
  const installClassList = (element) => {{
    if (Object.prototype.hasOwnProperty.call(element, "classList")) return;
    let current = null;
    Object.defineProperty(element, "classList", {{
      enumerable: false,
      configurable: false,
      get() {{ return current || (current = makeClassList(element)); }},
    }});
  }};
  const stylePropertyName = (name) => {{
    let value = String(name).trim();
    if (!value) return "";
    if (value === "cssFloat") value = "float";
    if (!value.startsWith("--")) value = value.replace(/[A-Z]/g, (character) => "-" + character.toLowerCase()).toLowerCase();
    return value;
  }};
  const stylePropertyKey = (name) => {{
    const value = String(name);
    if (value.startsWith("--")) return value;
    return value.replace(/-([a-z])/g, (_match, character) => character.toUpperCase());
  }};
  const parseStyleDeclarations = (cssText) => {{
    const declarations = [];
    let part = "";
    let quote = "";
    let parentheses = 0;
    const append = (source) => {{
      const text = String(source).trim();
      if (!text) return;
      const separator = text.indexOf(":");
      if (separator <= 0) return;
      const name = stylePropertyName(text.slice(0, separator));
      if (!name) return;
      let value = text.slice(separator + 1).trim();
      let priority = "";
      if (/\s*!important\s*$/i.test(value)) {{
        priority = "important";
        value = value.replace(/\s*!important\s*$/i, "").trim();
      }}
      if (!value) return;
      const existing = declarations.find((declaration) => declaration.name === name);
      if (existing) {{
        existing.value = value;
        existing.priority = priority;
      }} else {{
        declarations.push({{ name, value, priority }});
      }}
    }};
    const text = String(cssText || "");
    for (let index = 0; index < text.length; index += 1) {{
      const character = text[index];
      if (quote) {{
        part += character;
        if (character === quote && text[index - 1] !== "\\") quote = "";
      }} else if (character === "'" || character === '"') {{
        quote = character;
        part += character;
      }} else if (character === "(") {{
        parentheses += 1;
        part += character;
      }} else if (character === ")" && parentheses > 0) {{
        parentheses -= 1;
        part += character;
      }} else if (character === ";" && parentheses === 0) {{
        append(part);
        part = "";
      }} else {{
        part += character;
      }}
    }}
    append(part);
    return declarations;
  }};
  const serializeStyleDeclarations = (declarations) =>
    declarations.length === 0
      ? ""
      : declarations.map((declaration) =>
        declaration.name + ": " + declaration.value
          + (declaration.priority ? " !important" : "")).join("; ") + ";";
  const makeStyleDeclaration = (element) => {{
    const declarations = () => parseStyleDeclarations(element.getAttribute("style") || "");
    const write = (next) => {{
      const cssText = serializeStyleDeclarations(next);
      if (cssText) element.setAttribute("style", cssText);
      else element.removeAttribute("style");
    }};
    const propertyValue = (name) => {{
      const property = stylePropertyName(name);
      const declaration = declarations().find((candidate) => candidate.name === property);
      return declaration ? declaration.value : "";
    }};
    const api = {{
      get length() {{ return declarations().length; }},
      item(index) {{
        const numeric = Number(index);
        const declaration = Number.isSafeInteger(numeric) && numeric >= 0
          ? declarations()[numeric]
          : null;
        return declaration ? declaration.name : "";
      }},
      getPropertyValue(name) {{ return propertyValue(name); }},
      getPropertyPriority(name) {{
        const property = stylePropertyName(name);
        const declaration = declarations().find((candidate) => candidate.name === property);
        return declaration ? declaration.priority : "";
      }},
      setProperty(name, value, priority = "") {{
        const property = stylePropertyName(name);
        if (!property) return;
        const text = String(value);
        if (!text) {{
          this.removeProperty(property);
          return;
        }}
        const normalizedPriority = String(priority).trim().toLowerCase();
        if (normalizedPriority !== "" && normalizedPriority !== "important") return;
        const next = declarations();
        const existing = next.find((candidate) => candidate.name === property);
        if (existing) {{
          existing.value = text;
          existing.priority = normalizedPriority;
        }} else {{
          next.push({{ name: property, value: text, priority: normalizedPriority }});
        }}
        write(next);
      }},
      removeProperty(name) {{
        const property = stylePropertyName(name);
        const next = declarations();
        const index = next.findIndex((candidate) => candidate.name === property);
        if (index < 0) return "";
        const previous = next[index].value;
        next.splice(index, 1);
        write(next);
        return previous;
      }},
      get cssText() {{ return element.getAttribute("style") || ""; }},
      set cssText(next) {{ write(parseStyleDeclarations(next)); }},
      toString() {{ return this.cssText; }},
    }};
    const proxy = new Proxy(api, {{
      get(target, property, receiver) {{
        const index = collectionIndex(property);
        if (index !== null) return target.item(index) || undefined;
        if (typeof property === "string"
            && !Object.prototype.hasOwnProperty.call(target, property)
            && !property.startsWith("__")) {{
          return propertyValue(property);
        }}
        return Reflect.get(target, property, receiver);
      }},
      set(target, property, value, receiver) {{
        if (property === "cssText") {{
          target.cssText = value;
          return true;
        }}
        if (typeof property === "string"
            && !Object.prototype.hasOwnProperty.call(target, property)
            && !property.startsWith("__")) {{
          target.setProperty(property, value);
          return true;
        }}
        return Reflect.set(target, property, value, receiver);
      }},
      getOwnPropertyDescriptor(target, property) {{
        const index = collectionIndex(property);
        if (index !== null && index < target.length) {{
          return {{ enumerable: true, configurable: true, value: target.item(index), writable: false }};
        }}
        return Reflect.getOwnPropertyDescriptor(target, property);
      }},
    }});
    const constructor = globalThis.CSSStyleDeclaration;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(proxy, constructor.prototype); }} catch (_error) {{}}
    }}
    return proxy;
  }};
  const datasetAttributeName = (key) =>
    "data-" + String(key).replace(/[A-Z]/g, (character) => "-" + character.toLowerCase());
  const datasetKeyFromAttribute = (name) =>
    String(name).slice(5).replace(/-([a-z])/g, (_match, character) => character.toUpperCase());
  const makeDataset = (element) => {{
    const target = Object.create(null);
    const dataNames = () => (typeof element.getAttributeNames === "function"
      ? element.getAttributeNames()
      : []).filter((name) => String(name).toLowerCase().startsWith("data-")
        && String(name).length > 5);
    const proxy = new Proxy(target, {{
      get(_target, property) {{
        if (typeof property !== "string") return undefined;
        const value = element.getAttribute(datasetAttributeName(property));
        return value === null ? undefined : value;
      }},
      set(_target, property, value) {{
        if (typeof property !== "string") return false;
        element.setAttribute(datasetAttributeName(property), String(value));
        return true;
      }},
      has(_target, property) {{
        return typeof property === "string"
          && dataNames().some((name) => datasetKeyFromAttribute(name) === property);
      }},
      ownKeys() {{
        return dataNames().map(datasetKeyFromAttribute);
      }},
      getOwnPropertyDescriptor(_target, property) {{
        if (typeof property !== "string" || !dataNames().some((name) => datasetKeyFromAttribute(name) === property)) {{
          return undefined;
        }}
        return {{
          enumerable: true,
          configurable: true,
          value: element.getAttribute(datasetAttributeName(property)),
          writable: true,
        }};
      }},
      deleteProperty(_target, property) {{
        if (typeof property !== "string") return true;
        element.removeAttribute(datasetAttributeName(property));
        return true;
      }},
      defineProperty(_target, property, descriptor) {{
        if (typeof property !== "string") return false;
        element.setAttribute(datasetAttributeName(property), descriptor.value === undefined ? "" : String(descriptor.value));
        return true;
      }},
    }});
    return proxy;
  }};
  const installElementStyleAndDataset = (element) => {{
    if (!Object.prototype.hasOwnProperty.call(element, "style")) {{
      let current = null;
      Object.defineProperty(element, "style", {{
        enumerable: true,
        configurable: false,
        get() {{ return current || (current = makeStyleDeclaration(element)); }},
      }});
    }}
    if (!Object.prototype.hasOwnProperty.call(element, "dataset")) {{
      let current = null;
      Object.defineProperty(element, "dataset", {{
        enumerable: true,
        configurable: false,
        get() {{ return current || (current = makeDataset(element)); }},
      }});
    }}
  }};
  for (const element of elements) {{
    installClassList(element);
    installElementStyleAndDataset(element);
  }}
  const computedStyleEnumName = (value, fallback = "") => {{
    const kebab = (name) => String(name).replace(/([a-z])([A-Z])/g, "$1-$2").toLowerCase();
    if (typeof value === "string") return kebab(value);
    if (!value || typeof value !== "object") return fallback;
    const keys = Object.keys(value);
    if (keys.length === 0) return fallback;
    const key = keys[0];
    const payload = value[key];
    if (typeof payload === "string") return kebab(payload);
    return kebab(key);
  }};
  const computedStyleEnumPayload = (value) => {{
    if (!value || typeof value !== "object") return value;
    const keys = Object.keys(value);
    return keys.length === 0 ? undefined : value[keys[0]];
  }};
  const computedStyleNumber = (value, fallback = null) => {{
    const payload = computedStyleEnumPayload(value);
    return typeof payload === "number" && Number.isFinite(payload) ? payload : fallback;
  }};
  const computedStyleColor = (value, fallback = "rgba(0, 0, 0, 0)") => {{
    if (!value || typeof value !== "object") return fallback;
    const red = Number(value.red);
    const green = Number(value.green);
    const blue = Number(value.blue);
    const alpha = Number(value.alpha);
    if (![red, green, blue, alpha].every(Number.isFinite)) return fallback;
    if (alpha >= 255) return "rgb(" + red + ", " + green + ", " + blue + ")";
    return "rgba(" + red + ", " + green + ", " + blue + ", "
      + (alpha / 255).toFixed(3).replace(/0+$/, "").replace(/\.$/, "") + ")";
  }};
  const computedStylePixels = (value, fallback = "auto") => {{
    const number = computedStyleNumber(value);
    return number === null ? fallback : String(number) + "px";
  }};
  const computedStyleEdge = (value, side, fallback = "0px") => {{
    if (!value || typeof value !== "object") return fallback;
    const number = Number(value[side]);
    return Number.isFinite(number) ? String(number) + "px" : fallback;
  }};
  const computedStyleDefaultDisplay = (element) =>
    ["HTML", "BODY", "ADDRESS", "ARTICLE", "ASIDE", "BLOCKQUOTE", "DD", "DIV", "DL", "DT",
      "FIELDSET", "FIGCAPTION", "FIGURE", "FOOTER", "FORM", "H1", "H2", "H3", "H4", "H5",
      "H6", "HEADER", "HR", "LI", "MAIN", "NAV", "OL", "P", "PRE", "SECTION", "TABLE",
      "TR", "UL"].includes(element.tagName)
      ? "block"
      : "inline";
  const computedStyleProperties = [
    "display", "position", "visibility", "opacity", "pointer-events", "z-index",
    "top", "right", "bottom", "left", "width", "height", "min-width", "max-width",
    "min-height", "max-height", "box-sizing", "margin-top", "margin-right", "margin-bottom",
    "margin-left", "padding-top", "padding-right", "padding-bottom", "padding-left",
    "color", "background-color", "background-image", "background-repeat", "background-position",
    "background-size", "border-top-width", "border-right-width", "border-bottom-width",
    "border-left-width", "border-top-style", "border-right-style", "border-bottom-style",
    "border-left-style", "border-top-color", "border-right-color", "border-bottom-color",
    "border-left-color", "border-radius", "overflow", "overflow-x", "overflow-y", "white-space",
    "text-align", "text-align-last", "text-justify", "text-indent", "text-transform", "text-overflow",
    "text-decoration", "text-decoration-style", "text-decoration-thickness", "text-underline-offset",
    "font-weight", "font-style", "line-height", "word-break", "word-spacing", "letter-spacing",
    "vertical-align", "flex-direction", "flex-wrap", "flex-grow", "flex-shrink", "flex-basis",
    "justify-content", "align-items", "align-self", "align-content", "gap", "row-gap", "column-gap",
    "order", "grid-template-columns", "grid-template-rows"
  ];
  const computedStyleValue = (element, property, inline) => {{
    const name = stylePropertyName(property);
    const override = inline.find((declaration) => declaration.name === name);
    if (override) return override.value;
    const raw = typeof element.__glassComputedStyle === "function"
      ? element.__glassComputedStyle()
      : {{}};
    const geometry = geometryForNode(element);
    const rawMargins = raw.margin || {{}};
    const rawPadding = raw.padding || {{}};
    const rawMarginAuto = raw.margin_auto || {{}};
    const sideIndex = {{ top: 0, right: 1, bottom: 2, left: 3 }};
    const side = name.match(/(?:margin|padding|border)-(top|right|bottom|left)/);
    if (name === "display") {{
      const display = computedStyleEnumName(raw.display, "auto");
      return display === "auto" ? computedStyleDefaultDisplay(element) : display;
    }}
    if (name === "position") return computedStyleEnumName(raw.position, "static");
    if (name === "visibility") return raw.visibility_hidden ? "hidden" : "visible";
    if (name === "opacity") {{
      const opacity = raw.opacity === null || raw.opacity === undefined ? 1 : Number(raw.opacity) / 255;
      return Number.isFinite(opacity) ? String(Number(opacity.toFixed(3))) : "1";
    }}
    if (name === "pointer-events") return computedStyleEnumName(raw.pointer_events, "auto");
    if (name === "z-index") {{
      const value = computedStyleNumber(raw.z_index);
      return value === null ? "auto" : String(value);
    }}
    if (["top", "right", "bottom", "left"].includes(name)) return computedStylePixels(raw[name]);
    if (name === "width") return String(Number(geometry.contentWidth) || 0) + "px";
    if (name === "height") return String(Number(geometry.contentHeight) || 0) + "px";
    if (name === "min-width") return computedStylePixels(raw.min_width, "0px");
    if (name === "max-width") return computedStylePixels(raw.max_width, "none");
    if (name === "min-height") return computedStylePixels(raw.min_height, "0px");
    if (name === "max-height") return computedStylePixels(raw.max_height, "none");
    if (name === "box-sizing") return computedStyleEnumName(raw.box_sizing, "content-box");
    if (side && side[0].startsWith("margin-")) {{
      const direction = side[1];
      return rawMarginAuto[direction] ? "auto" : computedStyleEdge(rawMargins, direction);
    }}
    if (side && side[0].startsWith("padding-")) return computedStyleEdge(rawPadding, side[1]);
    if (side && name.startsWith("border-") && name.endsWith("-width")) {{
      const values = Array.isArray(raw.border_widths) ? raw.border_widths : [];
      return String(Number(values[sideIndex[side[1]]]) || 0) + "px";
    }}
    if (side && name.startsWith("border-") && name.endsWith("-style")) {{
      const values = Array.isArray(raw.border_styles) ? raw.border_styles : [];
      return computedStyleEnumName(values[sideIndex[side[1]]], "none");
    }}
    if (side && name.startsWith("border-") && name.endsWith("-color")) {{
      const values = Array.isArray(raw.border_colors) ? raw.border_colors : [];
      return computedStyleColor(values[sideIndex[side[1]]]);
    }}
    if (name === "color") return computedStyleColor(raw.color, "rgb(0, 0, 0)");
    if (name === "background-color") return computedStyleColor(raw.background_color);
    if (name === "background-image") return raw.background_image === null || raw.background_image === undefined ? "none" : "url(\"\")";
    if (name === "background-repeat") return computedStyleEnumName(raw.background_repeat, "repeat");
    if (["background-position", "background-size", "border-radius"].includes(name)) return "0px";
    if (name === "overflow") return computedStyleEnumName(raw.overflow_x, "visible") + " " + computedStyleEnumName(raw.overflow_y, "visible");
    if (name === "overflow-x") return computedStyleEnumName(raw.overflow_x, "visible");
    if (name === "overflow-y") return computedStyleEnumName(raw.overflow_y, "visible");
    if (name === "white-space") return computedStyleEnumName(raw.white_space, "normal");
    if (["text-align", "text-align-last", "text-justify", "text-transform", "word-break", "text-overflow", "vertical-align",
      "flex-direction", "flex-wrap", "justify-content", "align-items", "align-self", "align-content", "flex-basis"].includes(name)) {{
      const fields = {{
        "text-align": "text_align", "text-align-last": "text_align_last", "text-justify": "text_justify",
        "text-transform": "text_transform", "word-break": "word_break", "text-overflow": "text_overflow",
        "vertical-align": "vertical_align", "flex-direction": "flex_direction", "flex-wrap": "flex_wrap",
        "justify-content": "justify_content", "align-items": "align_items", "align-self": "align_self",
        "align-content": "align_content", "flex-basis": "flex_basis",
      }};
      return computedStyleEnumName(raw[fields[name]], name === "flex-basis" ? "auto" : "normal");
    }}
    if (name === "text-indent") return String(Number(raw.text_indent) || 0) + "px";
    if (name === "text-decoration") {{
      const flags = computedStyleNumber(raw.text_decoration, 0);
      const lines = [];
      if ((flags & 1) !== 0) lines.push("underline");
      if ((flags & 2) !== 0) lines.push("overline");
      if ((flags & 4) !== 0) lines.push("line-through");
      return lines.length === 0 ? "none" : lines.join(" ");
    }}
    if (name === "text-decoration-style") return computedStyleEnumName(raw.text_decoration_style, "solid");
    if (name === "text-decoration-thickness") return String(Number(raw.text_decoration_thickness) || 0) + "px";
    if (name === "text-underline-offset") return String(Number(raw.text_underline_offset) || 0) + "px";
    if (name === "font-weight") return computedStyleEnumName(raw.font_weight, "normal");
    if (name === "font-style") return computedStyleEnumName(raw.font_style, "normal");
    if (name === "line-height") return computedStylePixels(raw.line_height, "normal");
    if (name === "word-spacing") return String(Number(raw.word_spacing) || 0) + "px";
    if (name === "letter-spacing") return String(Number(raw.letter_spacing) || 0) + "px";
    if (name === "flex-grow") return String(Number(raw.flex_grow) || 0);
    if (name === "flex-shrink") return String(Number(raw.flex_shrink) || 0);
    if (name === "gap" || name === "row-gap" || name === "column-gap") {{
      const field = name === "gap" ? "gap" : name.replace(/-([a-z])/g, (_match, character) => character.toUpperCase());
      return String(Number(raw[field]) || 0) + "px";
    }}
    if (name === "order") {{
      const order = computedStyleNumber(raw.flex_item_order);
      return order === null ? "0" : String(order);
    }}
    return "";
  }};
  const makeComputedStyle = (element) => {{
    const inline = () => parseStyleDeclarations(element.getAttribute("style") || "");
    const api = {{
      get length() {{ return computedStyleProperties.length; }},
      item(index) {{
        const numeric = Number(index);
        return Number.isSafeInteger(numeric) && numeric >= 0
          ? computedStyleProperties[numeric] || ""
          : "";
      }},
      getPropertyValue(name) {{ return computedStyleValue(element, name, inline()); }},
      getPropertyPriority(_name) {{ return ""; }},
      get cssText() {{ return ""; }},
      toString() {{ return ""; }},
    }};
    return new Proxy(api, {{
      get(target, property, receiver) {{
        const index = collectionIndex(property);
        if (index !== null) return target.item(index);
        if (typeof property === "string"
            && !Object.prototype.hasOwnProperty.call(target, property)
            && !property.startsWith("__")) {{
          return computedStyleValue(element, property, inline());
        }}
        return Reflect.get(target, property, receiver);
      }},
      set(_target, _property, _value) {{ return false; }},
      ownKeys() {{ return computedStyleProperties.slice(); }},
      getOwnPropertyDescriptor(_target, property) {{
        if (typeof property === "string" && computedStyleProperties.includes(property)) {{
          return {{ enumerable: true, configurable: true, value: computedStyleValue(element, property, inline()), writable: false }};
        }}
        return undefined;
      }},
    }});
  }};
  globalThis.getComputedStyle = (element, _pseudoElement) => {{
    if (!element || Number(element.nodeType) !== 1) throw new TypeError("getComputedStyle requires an element");
    return makeComputedStyle(element);
  }};
  const matches = (element, selector) => matchesSelector(element, selector);
  const findAll = (selector) => asNodeList(liveDocumentElements().filter((element) => matches(element, selector)));
  const body = elements.find((element) => element.tagName === "BODY") || null;
  const documentElement = elements.find((element) => element.tagName === "HTML") || null;
  const rootSnapshot = snapshotNodes.find((entry) => entry && Number(entry.nodeIndex) === 0);
  const rootChildren = rootSnapshot && Array.isArray(rootSnapshot.children)
    ? rootSnapshot.children.map((index) => nodesByIndex.get(index)).filter(Boolean)
    : elements.filter((element) => element.parentIndex === null);
  const liveDocumentElements = () => {{
    const values = [];
    const seen = new Set();
    const visit = (node) => {{
      if (!node || seen.has(node)) return;
      seen.add(node);
      if (Number(node.nodeType) === 1) values.push(node);
      for (const child of node.__glassChildren || []) visit(child);
    }};
    for (const root of rootChildren) visit(root);
    return values;
  }};
  let documentCookie = typeof host.cookie === "string" ? host.cookie : "";
  const previewCookieSet = (current, value) => {{
    const pair = String(value).split(";", 1)[0].trim();
    const separator = pair.indexOf("=");
    if (separator <= 0) return current;
    const name = pair.slice(0, separator).trim();
    const pairs = current ? current.split("; ").filter(Boolean) : [];
    const existing = pairs.findIndex(candidate => candidate.slice(0, candidate.indexOf("=")).trim() === name);
    if (existing >= 0) pairs[existing] = pair;
    else pairs.push(pair);
    return pairs.join("; ");
  }};
  let documentTitle = String(state.title || "");
  const document = {{
    get title() {{
      const titleElement = liveDocumentElements().find((element) => element.tagName === "TITLE");
      return titleElement ? titleElement.textContent : documentTitle;
    }},
    set title(value) {{
      const text = String(value);
      if (text.length > storageValueLimit) throw new RangeError("native document.title exceeds its limit");
      documentTitle = text;
      const titleElement = liveDocumentElements().find((element) => element.tagName === "TITLE");
      if (titleElement) {{
        titleElement.textContent = text;
        return;
      }}
      let parent = document.head;
      if (!parent && documentElement) {{
        parent = makeDetachedElement("head");
        documentElement.insertBefore(parent, documentElement.firstChild);
      }}
      if (parent) {{
        const created = makeDetachedElement("title");
        created.textContent = text;
        parent.appendChild(created);
        return;
      }}
      pushCommand({{ kind: "setDocumentTitle", value: text }});
    }},
    body,
    documentElement,
    nodeType: 9,
    nodeName: "#document",
    URL: host.url,
    documentURI: host.url,
    compatMode: "CSS1Compat",
    hidden: false,
    visibilityState: "visible",
    get defaultView() {{ return globalThis; }},
    get activeElement() {{ return liveDocumentElements().find((element) => element.focused) || null; }},
    get head() {{ return liveDocumentElements().find((element) => element.tagName === "HEAD") || null; }},
    get doctype() {{ return document.__glassChildren.find((node) => Number(node.nodeType) === 10) || null; }},
    get forms() {{ return liveCollection(document, (element) => element.tagName === "FORM", "HTMLCollection", true); }},
    get links() {{ return liveCollection(document, (element) => ["A", "AREA"].includes(element.tagName) && element.getAttribute("href") !== null, "HTMLCollection", true); }},
    get scripts() {{ return liveCollection(document, (element) => element.tagName === "SCRIPT", "HTMLCollection", true); }},
    get images() {{ return liveCollection(document, (element) => element.tagName === "IMG", "HTMLCollection", true); }},
    get scrollingElement() {{ return documentElement; }},
    get cookie() {{ return documentCookie; }},
    set cookie(value) {{
      const text = String(value);
      if (text.length > storageValueLimit) throw new RangeError("native document.cookie value exceeds its limit");
      documentCookie = previewCookieSet(documentCookie, text);
      pushCommand({{ kind: "cookieSet", value: text }});
    }},
    readyState: {ready_state},
    addEventListener(type, callback, options) {{
      addListener("document", type, callback, options);
    }},
    removeEventListener(type, callback, options) {{
      removeListener("document", type, callback, options);
    }},
    dispatchEvent(event) {{
      return dispatchTarget(this, event);
    }},
    createElement(tagName) {{ return makeDetachedElement(tagName, HTML_NAMESPACE); }},
    createElementNS(namespace, qualifiedName) {{
      return makeDetachedElement(qualifiedName, normalizeElementNamespace(namespace));
    }},
    createAttribute(name) {{ return makeAttributeNode(name, "", () => document); }},
    createAttributeNS(namespace, qualifiedName) {{
      const namespaceURI = normalizeAttributeNamespace(namespace);
      return makeAttributeNode(qualifiedName, "", () => document, namespaceURI);
    }},
    createTextNode(value) {{ return makeDetachedText(value); }},
    createComment(value) {{ return makeDetachedComment(value); }},
    appendChild(child) {{
      if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
      if (document.__glassChildren.includes(child)) return child;
      if (child.__glassParent && child.__glassParent !== document) throw new DOMExceptionNative("The document type has another parent", "HierarchyRequestError");
      const existing = document.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
      if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
      document.__glassChildren = document.__glassChildren.filter((candidate) => candidate !== child);
      document.__glassChildren.push(child);
      child.__glassParent = document;
      child.parentIndex = 0;
      pushCommand({{ kind: "appendChild", parent_index: 0, child_index: child.nodeIndex }});
      return child;
    }},
    insertBefore(child, before) {{
      if (before == null) return document.appendChild(child);
      if (!before || !document.__glassChildren.includes(before)) throw new TypeError("reference node is not a document child");
      if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
      if (child === before) return child;
      const existing = document.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
      if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
      document.__glassChildren = document.__glassChildren.filter((candidate) => candidate !== child);
      const index = document.__glassChildren.indexOf(before);
      document.__glassChildren.splice(index < 0 ? document.__glassChildren.length : index, 0, child);
      child.__glassParent = document;
      child.parentIndex = 0;
      pushCommand({{ kind: "insertBefore", parent_index: 0, child_index: child.nodeIndex, before_index: before.nodeIndex }});
      return child;
    }},
    createDocumentFragment() {{ return makeDocumentFragment(); }},
    implementation: {{
      createDocumentType(name, publicId = "", systemId = "") {{
        return makeDetachedDocumentType(name, publicId, systemId);
      }},
    }},
    getElementById(id) {{ return liveDocumentElements().find((element) => element.id === String(id)) || null; }},
    querySelector(selector) {{ return findAll(selector)[0] || null; }},
    querySelectorAll(selector) {{ return findAll(selector); }},
    getElementsByTagName(name) {{
      const value = String(name).toLowerCase();
      return liveCollection(document, (element) => value === "*" || element.tagName.toLowerCase() === value, "HTMLCollection", true);
    }},
    getElementsByClassName(name) {{
      const value = String(name);
      return liveCollection(document, (element) => element.className.split(/\s+/).includes(value), "HTMLCollection", true);
    }},
    getElementsByName(name) {{
      const value = String(name);
      return asNodeList(liveDocumentElements().filter((element) => element.getAttribute("name") === value));
    }},
  }};
  Object.defineProperty(document, "__glassChildren", {{
    enumerable: false,
    configurable: false,
    writable: true,
    value: rootChildren,
  }});
  defineTreeAccessors(document);
  globalThis.__glassHostDocument = document;
  globalThis.__glassDispatchHostEvents = (events) => events.map((descriptor) => {{
    const target = descriptor.node_index === 0
      ? document
      : descriptor.node_index === 4294967295
        ? globalThis
        : elements.find((element) => element.nodeIndex === descriptor.node_index) || null;
    if (!target) throw new TypeError("native event target is detached");
    const event = createEvent(descriptor.type, {{
      bubbles: Boolean(descriptor.bubbles),
      cancelable: Boolean(descriptor.cancelable),
      persisted: Boolean(descriptor.persisted),
      key: descriptor.key,
      code: descriptor.code,
      altKey: Boolean(descriptor.alt_key),
      ctrlKey: Boolean(descriptor.ctrl_key),
      metaKey: Boolean(descriptor.meta_key),
      shiftKey: Boolean(descriptor.shift_key),
      submitter: descriptor.submitter_node_index == null
        ? null
        : elements.find((element) => element.nodeIndex === descriptor.submitter_node_index) || null,
      oldURL: descriptor.old_url,
      newURL: descriptor.new_url,
    }});
    if (event.type === "popstate" && globalThis.history) event.state = globalThis.history.state;
    return dispatchTarget(target, event);
  }});
  globalThis.__glassDispatchScriptError = (descriptor) => {{
    if (!descriptor || typeof descriptor !== "object") throw new TypeError("native script error is invalid");
    const message = String(descriptor.message || "");
    const filename = String(descriptor.filename || "");
    const line = Number(descriptor.lineno) || 0;
    const column = Number(descriptor.colno) || 0;
    const error = new Error(message);
    const createErrorEvent = () => {{
      const constructor = globalThis.__glassErrorEventConstructor || globalThis.ErrorEvent;
      if (typeof constructor === "function") return new constructor("error", {{
        message,
        filename,
        lineno: line,
        colno: column,
        error,
      }});
      const event = createEvent("error", {{ bubbles: false, cancelable: false }});
      event.message = message;
      event.filename = filename;
      event.lineno = line;
      event.colno = column;
      event.error = error;
      return event;
    }};
    const results = [];
    const nodeIndex = descriptor.node_index;
    if (nodeIndex !== null && nodeIndex !== undefined) {{
      const target = elements.find((element) => element.nodeIndex === Number(nodeIndex)) || null;
      if (!target) throw new TypeError("native script error target is detached");
      results.push(dispatchTarget(target, createErrorEvent()));
    }}
    results.push(dispatchTarget(globalThis, createErrorEvent()));
    return results;
  }};
  globalThis.__glassDispatchPromiseRejections = (type, rejections, cancelable) => {{
    const eventType = String(type || "");
    if (eventType !== "unhandledrejection" && eventType !== "rejectionhandled")
      throw new TypeError("native Promise rejection event type is invalid");
    if (!Array.isArray(rejections)) throw new TypeError("native Promise rejections are invalid");
    return rejections.map((descriptor) => {{
      const reason = descriptor && typeof descriptor === "object" && descriptor.reason !== undefined
        ? descriptor.reason
        : null;
      const constructor = globalThis.__glassPromiseRejectionEventConstructor
        || globalThis.PromiseRejectionEvent;
      const event = typeof constructor === "function"
        ? new constructor(eventType, {{ reason, promise: null, cancelable: Boolean(cancelable) }})
        : createEvent(eventType, {{ bubbles: false, cancelable: Boolean(cancelable) }});
      if (event.reason === undefined) event.reason = reason;
      if (event.promise === undefined) event.promise = null;
      return dispatchTarget(globalThis, event);
    }});
  }};
  globalThis.window = globalThis;
  const dialogText = (value, field) => {{
    const text = String(value === undefined || value === null ? "" : value);
    if (text.length > {dialog_text_limit}) throw new RangeError("native dialog " + field + " exceeds its limit");
    return text;
  }};
  let dialogQueued = false;
  const queueDialog = (dialogType, message, defaultValue) => {{
    if (dialogQueued) throw new Error("native JavaScript dialog is already pending");
    dialogQueued = true;
    pushCommand({{
      kind: "dialog",
      dialog_type: dialogType,
      message: dialogText(message, "message"),
      default_value: defaultValue === undefined ? null : dialogText(defaultValue, "default value"),
    }});
  }};
  globalThis.alert = (message) => {{
    queueDialog("alert", message);
    return undefined;
  }};
  globalThis.confirm = (message) => {{
    queueDialog("confirm", message);
    return false;
  }};
  globalThis.prompt = (message, defaultValue = "") => {{
    queueDialog("prompt", message, defaultValue);
    return null;
  }};
  globalThis.__glassHostCommands = commands;
  globalThis.__glassHostCommandBuffer = commands;
  globalThis.document = document;
  const locationUrl = new URLNative(host.url);
  const navigateLocation = (value, replaceHistory) => {{
    const next = new URLNative(value, locationUrl.href);
    const href = next.href;
    pushCommand({{ kind: "navigate", href, replace: Boolean(replaceHistory) }});
    locationUrl.href = href;
  }};
  const setLocationComponent = (name, value) => {{
    const next = new URLNative(locationUrl.href);
    next[name] = value;
    navigateLocation(next.href, false);
  }};
  const location = {{
    assign(value) {{ navigateLocation(value, false); }},
    replace(value) {{ navigateLocation(value, true); }},
    reload() {{ navigateLocation(locationUrl.href, false); }},
    toString() {{ return locationUrl.href; }},
  }};
  for (const name of ["href", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"]) {{
    Object.defineProperty(location, name, {{
      enumerable: true,
      get: () => locationUrl[name],
      set: value => name === "href"
        ? navigateLocation(value, false)
        : setLocationComponent(name, value),
    }});
  }}
  Object.defineProperty(location, "origin", {{
    enumerable: true,
    get: () => locationUrl.origin,
  }});
  if (typeof globalThis.Location !== "function") {{
    globalThis.Location = function Location() {{
      throw new TypeError("Illegal constructor");
    }};
  }}
  if (typeof globalThis.Window !== "function") {{
    globalThis.Window = function Window() {{
      throw new TypeError("Illegal constructor");
    }};
  }}
  try {{ Object.setPrototypeOf(location, globalThis.Location.prototype); }} catch (_error) {{}}
  Object.freeze(location);
  globalThis.location = location;
  let currentHistoryState = host.history_state === undefined ? null : host.history_state;
  let currentHistoryLength = Math.max(1, Number(host.history_length) || 1);
  let historyScrollRestoration = "auto";
  const cloneHistoryState = (value) => {{
    let encoded;
    try {{ encoded = JSON.stringify(value === undefined ? null : value); }} catch (_error) {{
      throw new DOMExceptionNative("history state could not be cloned", "DataCloneError");
    }}
    if (encoded === undefined) throw new DOMExceptionNative("history state could not be cloned", "DataCloneError");
    if (encoded.length > {history_state_bytes_limit}) throw new RangeError("native history state exceeds its limit");
    try {{ return JSON.parse(encoded); }} catch (_error) {{
      throw new DOMExceptionNative("history state could not be cloned", "DataCloneError");
    }}
  }};
  const historyTarget = (value) => {{
    if (value === undefined || value === null || String(value) === "") return locationUrl.href;
    const next = new URLNative(String(value), locationUrl.href);
    const documentOrigin = String(host.origin || "null");
    const sameOrigin = documentOrigin !== "null"
      ? next.origin === documentOrigin
      : next.protocol === locationUrl.protocol && next.host === locationUrl.host;
    if (!sameOrigin) throw new DOMExceptionNative("history URL must be same-origin", "SecurityError");
    return next.href;
  }};
  const history = {{
    back() {{ pushCommand({{ kind: "historyGo", delta: -1 }}); }},
    forward() {{ pushCommand({{ kind: "historyGo", delta: 1 }}); }},
    go(delta = 0) {{
      const numeric = Number(delta);
      if (!Number.isFinite(numeric)) return;
      const offset = Math.trunc(numeric);
      if (offset < -{history_length_limit} || offset > {history_length_limit}) throw new RangeError("native history delta exceeds its limit");
      pushCommand({{ kind: "historyGo", delta: offset }});
    }},
    pushState(state, _title, url) {{
      const cloned = cloneHistoryState(state);
      const href = historyTarget(url);
      currentHistoryState = cloned;
      currentHistoryLength = Math.min({history_length_limit}, currentHistoryLength + 1);
      locationUrl.href = href;
      pushCommand({{ kind: "historyPushState", href, state: cloned }});
    }},
    replaceState(state, _title, url) {{
      const cloned = cloneHistoryState(state);
      const href = historyTarget(url);
      currentHistoryState = cloned;
      locationUrl.href = href;
      pushCommand({{ kind: "historyReplaceState", href, state: cloned }});
    }},
  }};
  Object.defineProperties(history, {{
    length: {{ enumerable: true, get: () => currentHistoryLength }},
    state: {{ enumerable: true, get: () => cloneHistoryState(currentHistoryState) }},
    scrollRestoration: {{
      enumerable: true,
      get: () => historyScrollRestoration,
      set: value => {{
        const next = String(value);
        if (next !== "auto" && next !== "manual") throw new TypeError("invalid history scrollRestoration");
        historyScrollRestoration = next;
      }},
    }},
  }});
  if (typeof globalThis.History !== "function") {{
    globalThis.History = function History() {{ throw new TypeError("Illegal constructor"); }};
  }}
  try {{ Object.setPrototypeOf(history, globalThis.History.prototype); }} catch (_error) {{}}
  globalThis.history = history;
  const windowProxyCache = globalThis.__glassWindowProxyCache instanceof Map
    ? globalThis.__glassWindowProxyCache
    : new Map();
  globalThis.__glassWindowProxyCache = windowProxyCache;
  const windowProxyStates = globalThis.__glassWindowProxyStates instanceof Map
    ? globalThis.__glassWindowProxyStates
    : new Map();
  globalThis.__glassWindowProxyStates = windowProxyStates;
  const cloneMessageData = (value) => {{
    let encoded;
    try {{ encoded = JSON.stringify(value); }} catch (_error) {{
      throw new TypeError("message could not be cloned");
    }}
    if (encoded === undefined) throw new TypeError("message could not be cloned");
    if (encoded.length > {post_message_bytes_limit}) throw new RangeError("native postMessage data exceeds its limit");
    try {{ return JSON.parse(encoded); }} catch (_error) {{
      throw new TypeError("message could not be cloned");
    }}
  }};
  const queueWindowMessage = (handle, targetContextId, message, targetOrigin) => {{
    const origin = targetOrigin === undefined ? "/" : String(targetOrigin);
    if (origin.length === 0 || origin.length > {storage_key_limit}) throw new TypeError("invalid postMessage target origin");
    pushCommand({{
      kind: "postMessage",
      target: String(handle || ""),
      target_origin: origin,
      data: cloneMessageData(message),
      target_context_id: targetContextId || null,
    }});
  }};
  const currentDocumentOrigin = String(host.origin || "null");
  const originForUrl = (value) => {{
    try {{ return new URLNative(String(value)).origin; }} catch (_error) {{ return "null"; }}
  }};
  const sameOriginForUrl = (value) =>
    currentDocumentOrigin !== "null" && originForUrl(value) === currentDocumentOrigin;
  const crossOriginSecurityError = (property) => new DOMExceptionNative(
    "Permission denied to access property '" + String(property) + "'",
    "SecurityError",
  );
  const installCrossOriginGuards = (proxy, state) => {{
    for (const property of [
      "history", "localStorage", "sessionStorage", "indexedDB", "navigator",
      "performance", "screen", "crypto",
    ]) {{
      if (Object.prototype.hasOwnProperty.call(proxy, property)) continue;
      Object.defineProperty(proxy, property, {{
        configurable: false,
        enumerable: false,
        get() {{
          if (!state.sameOrigin) throw crossOriginSecurityError(property);
          return undefined;
        }},
      }});
    }}
  }};
  const makeWindowProxy = (
    handle,
    targetName,
    targetContextId,
    targetUrl,
    sameOriginOverride = undefined,
  ) => {{
    const cacheKey = String(targetContextId || "") + "\\u0000" + String(handle || "");
    const existing = windowProxyCache.get(cacheKey);
    if (existing) return existing;
    const state = {{
      closed: false,
      targetName: String(targetName || ""),
      targetContextId: String(targetContextId || ""),
      targetLocationHref: String(targetUrl || "about:blank"),
      sameOrigin: sameOriginOverride === undefined
        ? sameOriginForUrl(targetUrl)
        : Boolean(sameOriginOverride),
    }};
    windowProxyStates.set(cacheKey, state);
    const navigateTarget = (value, replaceHistory) => {{
      if (state.closed) return;
      const next = new URLNative(String(value), state.targetLocationHref || locationUrl.href).href;
      state.targetLocationHref = next;
      pushCommand({{
        kind: "navigateWindow",
        target: String(handle || ""),
        target_context_id: state.targetContextId || targetContextId || null,
        href: next,
        replace: Boolean(replaceHistory),
      }});
    }};
    const targetLocation = {{
      get href() {{ return state.targetLocationHref; }},
      set href(value) {{ navigateTarget(value, false); }},
      get protocol() {{ return new URLNative(state.targetLocationHref).protocol; }},
      get host() {{ return new URLNative(state.targetLocationHref).host; }},
      get hostname() {{ return new URLNative(state.targetLocationHref).hostname; }},
      get port() {{ return new URLNative(state.targetLocationHref).port; }},
      get pathname() {{ return new URLNative(state.targetLocationHref).pathname; }},
      get search() {{ return new URLNative(state.targetLocationHref).search; }},
      get hash() {{ return new URLNative(state.targetLocationHref).hash; }},
      get origin() {{ return new URLNative(state.targetLocationHref).origin; }},
      assign(value) {{ navigateTarget(value, false); }},
      replace(value) {{ navigateTarget(value, true); }},
      reload() {{ navigateTarget(state.targetLocationHref, false); }},
      toString() {{ return state.targetLocationHref; }},
    }};
    if (typeof globalThis.Location === "function" && globalThis.Location.prototype) {{
      try {{ Object.setPrototypeOf(targetLocation, globalThis.Location.prototype); }} catch (_error) {{}}
    }}
    Object.freeze(targetLocation);
    const eventOwner = "window:" + cacheKey;
    const proxy = {{
      get name() {{ return state.targetName; }},
      get closed() {{ return state.closed; }},
      get location() {{ return targetLocation; }},
      addEventListener(type, callback, options) {{
        addListener(eventOwner, type, callback, options);
      }},
      removeEventListener(type, callback, options) {{
        removeListener(eventOwner, type, callback, options);
      }},
      dispatchEvent(event) {{
        return dispatchTarget(this, event);
      }},
      close() {{
        if (state.closed) return;
        state.closed = true;
        pushCommand({{
          kind: "closeWindow",
          target: String(handle || ""),
          target_context_id: state.targetContextId || targetContextId || null,
        }});
      }},
      postMessage(message, targetOrigin = "/") {{
        queueWindowMessage(handle, targetContextId, message, targetOrigin);
      }},
      toJSON() {{
        return {{ name: this.name, closed: this.closed }};
      }},
    }};
    Object.defineProperty(proxy, "__glassEventOwner", {{
      enumerable: false,
      configurable: false,
      value: eventOwner,
    }});
    if (typeof globalThis.Window === "function" && globalThis.Window.prototype) {{
      try {{ Object.setPrototypeOf(proxy, globalThis.Window.prototype); }} catch (_error) {{}}
    }}
    installCrossOriginGuards(proxy, state);
    windowProxyCache.set(cacheKey, proxy);
    return proxy;
  }};
  globalThis.__glassSyncWindowProxies = (updates) => {{
    if (!Array.isArray(updates)) return;
    for (const update of updates) {{
      if (!update || typeof update !== "object") continue;
      const cacheKey = String(update.cache_key || "");
      const targetContextId = String(update.target_context_id || "");
      for (const [candidateKey, proxy] of windowProxyCache.entries()) {{
        const state = windowProxyStates.get(candidateKey);
        if (!state) continue;
        if (candidateKey !== cacheKey &&
            (!targetContextId || state.targetContextId !== targetContextId)) continue;
        state.targetContextId = targetContextId || state.targetContextId;
        if (typeof update.href === "string" && update.href.length > 0)
          state.targetLocationHref = update.href;
        if (typeof update.name === "string") state.targetName = update.name;
        state.sameOrigin = sameOriginForUrl(state.targetLocationHref);
        state.closed = Boolean(update.closed);
      }}
    }}
  }};
  let windowName = typeof globalThis.__glassWindowName === "string"
    ? globalThis.__glassWindowName
    : String(host.window_name || "");
  globalThis.__glassWindowName = windowName;
  const setWindowName = (value) => {{
    const next = String(value);
    if (next.length > {window_name_bytes_limit}) throw new RangeError("native window.name exceeds its limit");
    if ([...next].some((character) => {{
      const code = character.codePointAt(0);
      return code < 0x20 || code === 0x7f;
    }})) throw new TypeError("native window.name contains a control character");
    windowName = next;
    globalThis.__glassWindowName = next;
    pushCommand({{ kind: "setWindowName", value: next }});
  }};
  Object.defineProperty(globalThis, "name", {{
    configurable: true,
    enumerable: true,
    get: () => windowName,
    set: setWindowName,
  }});
  const openerProxy = host.opener_context_id
    ? makeWindowProxy("opener:" + String(host.opener_context_id), String(host.opener_window_name || ""), String(host.opener_context_id), String(host.opener_url || "about:blank"))
    : null;
  Object.defineProperty(globalThis, "opener", {{
    configurable: true,
    enumerable: true,
    get: () => openerProxy,
  }});
  globalThis.postMessage = (message, targetOrigin = "/") =>
    queueWindowMessage("", host.context_id, message, targetOrigin);
  globalThis.__glassDispatchMessage = (descriptor) => {{
    const event = createEvent("message", {{ bubbles: false, cancelable: false }});
    event.data = descriptor && Object.prototype.hasOwnProperty.call(descriptor, "data")
      ? descriptor.data
      : null;
    event.origin = String(descriptor && descriptor.source_origin || "null");
    event.source = descriptor && descriptor.source_context_id
      ? makeWindowProxy("source:" + String(descriptor.source_context_id), "", String(descriptor.source_context_id), "about:blank")
      : null;
    event.ports = [];
    return dispatchTarget(globalThis, event);
  }};
  globalThis.open = function open(value, target) {{
    const rawTarget = target === undefined || target === null ? "_blank" : String(target);
    const normalizedTarget = rawTarget || "_blank";
    const lowerTarget = normalizedTarget.toLowerCase();
    const href = value === undefined || value === null || String(value) === ""
      ? "about:blank"
      : new URLNative(String(value), locationUrl.href).href;
    if (["_self", "_parent", "_top", "_unfencedtop"].includes(lowerTarget)) {{
      navigateLocation(href, false);
      return makeWindowProxy("", "", host.context_id, href);
    }}
    const windowHandles = globalThis.__glassWindowHandles instanceof Map
      ? globalThis.__glassWindowHandles
      : new Map();
    globalThis.__glassWindowHandles = windowHandles;
    let handle;
    if (normalizedTarget === "_blank") {{
      const next = Number.isSafeInteger(globalThis.__glassNextWindowHandle)
        ? globalThis.__glassNextWindowHandle
        : 1;
      handle = "glass-window-" + next;
      globalThis.__glassNextWindowHandle = next + 1;
    }} else {{
      handle = windowHandles.get(normalizedTarget);
      if (!handle) {{
        const next = Number.isSafeInteger(globalThis.__glassNextWindowHandle)
          ? globalThis.__glassNextWindowHandle
          : 1;
        handle = "glass-window-" + next;
        globalThis.__glassNextWindowHandle = next + 1;
        windowHandles.set(normalizedTarget, handle);
      }}
    }}
    pushCommand({{ kind: "openWindow", href, target: normalizedTarget, handle }});
    return makeWindowProxy(
      handle,
      normalizedTarget === "_blank" ? "" : normalizedTarget,
      null,
      href,
    );
  }};
  globalThis.innerWidth = {width};
  globalThis.innerHeight = {height};
  const nativeMediaDimension = (value) => {{
    const match = /^(-?\d+(?:\.\d+)?)px$/i.exec(String(value).trim());
    return match ? Number(match[1]) : null;
  }};
  const nativeMediaQueryMatches = (query) => {{
    let source = String(query).trim().toLowerCase();
    if (!source) return false;
    let negate = false;
    if (source.startsWith("not ")) {{
      negate = true;
      source = source.slice(4).trim();
    }}
    if (source.startsWith("only ")) source = source.slice(5).trim();
    const clauses = source.split(/\s+and\s+/).map((value) => value.trim()).filter(Boolean);
    let matches = true;
    for (const clause of clauses) {{
      if (clause === "all" || clause === "screen") continue;
      const feature = /^\(([-a-z]+)\s*:\s*([^)]*)\)$/.exec(clause);
      if (!feature) {{ matches = false; break; }}
      const name = feature[1];
      const value = feature[2].trim();
      if (name === "min-width") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerWidth >= dimension; }}
      else if (name === "max-width") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerWidth <= dimension; }}
      else if (name === "width") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerWidth === dimension; }}
      else if (name === "min-height") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerHeight >= dimension; }}
      else if (name === "max-height") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerHeight <= dimension; }}
      else if (name === "height") {{ const dimension = nativeMediaDimension(value); matches = matches && dimension !== null && innerHeight === dimension; }}
      else if (name === "orientation") matches = matches && value === (innerWidth >= innerHeight ? "landscape" : "portrait");
      else if (name === "prefers-color-scheme") matches = false;
      else matches = false;
    }}
    return negate ? !matches : matches;
  }};
  globalThis.matchMedia = (query) => {{
    const media = String(query);
    const listeners = new Set();
    const result = {{
      media,
      get matches() {{ return nativeMediaQueryMatches(media); }},
      onchange: null,
      addListener(callback) {{ if (typeof callback === "function") listeners.add(callback); }},
      removeListener(callback) {{ listeners.delete(callback); }},
      addEventListener(type, callback) {{ if (type === "change" && typeof callback === "function") listeners.add(callback); }},
      removeEventListener(type, callback) {{ if (type === "change") listeners.delete(callback); }},
      dispatchEvent(event) {{
        for (const callback of listeners) callback.call(result, event);
        if (typeof result.onchange === "function") result.onchange.call(result, event);
        return true;
      }},
    }};
    return result;
  }};
  const performanceNative = globalThis.__glassPerformance instanceof Object
    ? globalThis.__glassPerformance
    : {{}};
  performanceNative.now = () => Number(host.now_ms) || 0;
  performanceNative.timeOrigin = Date.now() - (Number(host.now_ms) || 0);
  globalThis.__glassPerformance = performanceNative;
  globalThis.performance = performanceNative;
  globalThis.scrollX = Number(state.scrollX || 0);
  globalThis.scrollY = Number(state.scrollY || 0);
  globalThis.pageXOffset = globalThis.scrollX;
  globalThis.pageYOffset = globalThis.scrollY;
  const applyWindowScroll = (leftCandidate, topCandidate) => {{
    const leftValue = Number(leftCandidate);
    const topValue = Number(topCandidate);
    const left = leftCandidate === undefined
      ? (Number(globalThis.scrollX) || 0)
      : (Number.isFinite(leftValue) ? Math.trunc(leftValue) : 0);
    const top = topCandidate === undefined
      ? (Number(globalThis.scrollY) || 0)
      : (Number.isFinite(topValue) ? Math.trunc(topValue) : 0);
    const nextLeft = Math.max(0, Math.min(
      Math.max(0, (Number(state.scrollWidth) || globalThis.innerWidth) - globalThis.innerWidth),
      left,
    ));
    const nextTop = Math.max(0, Math.min(
      Math.max(0, (Number(state.scrollHeight) || globalThis.innerHeight) - globalThis.innerHeight),
      top,
    ));
    const changed = nextLeft !== globalThis.scrollX || nextTop !== globalThis.scrollY;
    globalThis.scrollX = nextLeft;
    globalThis.scrollY = nextTop;
    globalThis.pageXOffset = nextLeft;
    globalThis.pageYOffset = nextTop;
    state.scrollX = nextLeft;
    state.scrollY = nextTop;
    if (documentElement && typeof documentElement.__glassSetScrollState === "function") {{
      documentElement.__glassSetScrollState(nextLeft, nextTop);
    }}
    if (changed) pushCommand({{ kind: "scrollTo", node_index: 0, left: nextLeft, top: nextTop }});
  }};
  globalThis.scrollTo = (leftOrOptions = 0, top = 0) => {{
    if (leftOrOptions && typeof leftOrOptions === "object") {{
      applyWindowScroll(leftOrOptions.left, leftOrOptions.top);
    }} else {{
      applyWindowScroll(leftOrOptions, top);
    }}
  }};
  globalThis.scrollBy = (leftOrOptions = 0, top = 0) => {{
    if (leftOrOptions && typeof leftOrOptions === "object") {{
      applyWindowScroll(globalThis.scrollX + (Number(leftOrOptions.left) || 0), globalThis.scrollY + (Number(leftOrOptions.top) || 0));
    }} else {{
      applyWindowScroll(globalThis.scrollX + (Number(leftOrOptions) || 0), globalThis.scrollY + (Number(top) || 0));
    }}
  }};
  globalThis.scroll = globalThis.scrollTo;
  globalThis.navigator = globalThis.navigator || {{ userAgent: "GlassNative" }};
  const nativeStorageUsage = () => {{
    const encoded = (value) => {{
      try {{ return JSON.stringify(value); }} catch (_error) {{ return ""; }}
    }};
    const mapBytes = (value) => {{
      const text = value instanceof Map ? encoded(Array.from(value.entries())) : encoded(value);
      return text ? text.length : 0;
    }};
    const usage = mapBytes(globalThis.__glassLocalStorageValues)
      + mapBytes(globalThis.__glassSessionStorageValues)
      + encoded(globalThis.__glassIndexedDbState || {{ databases: {{}} }}).length;
    return Math.min({storage_quota}, usage);
  }};
  const nativeStorageManager = globalThis.__glassNativeStorageManager instanceof Object
    ? globalThis.__glassNativeStorageManager
    : {{}};
  nativeStorageManager.estimate = () => Promise.resolve({{ usage: nativeStorageUsage(), quota: {storage_quota} }});
  nativeStorageManager.persist = () => Promise.resolve(false);
  nativeStorageManager.persisted = () => Promise.resolve(false);
  globalThis.__glassNativeStorageManager = nativeStorageManager;
  globalThis.navigator.storage = nativeStorageManager;
  const ensureNativeConstructor = (name, parent) => {{
    let constructor = globalThis[name];
    if (typeof constructor !== "function") {{
      constructor = function NativeWebIdlConstructor() {{
        throw new TypeError("Illegal constructor");
      }};
      globalThis[name] = constructor;
    }}
    if (constructor.prototype && parent && parent.prototype
        && Object.getPrototypeOf(constructor.prototype) !== parent.prototype) {{
      try {{ Object.setPrototypeOf(constructor.prototype, parent.prototype); }} catch (_error) {{}}
    }}
    return constructor;
  }};
  const defineDomRectValues = (target, x, y, width, height, writable) => {{
    const values = {{
      x: Number(x) || 0,
      y: Number(y) || 0,
      width: Number(width) || 0,
      height: Number(height) || 0,
    }};
    Object.defineProperties(target, {{
      x: {{ configurable: true, enumerable: true, writable, value: values.x }},
      y: {{ configurable: true, enumerable: true, writable, value: values.y }},
      width: {{ configurable: true, enumerable: true, writable, value: values.width }},
      height: {{ configurable: true, enumerable: true, writable, value: values.height }},
      top: {{ configurable: true, enumerable: true, get() {{ return this.y; }} }},
      right: {{ configurable: true, enumerable: true, get() {{ return this.x + this.width; }} }},
      bottom: {{ configurable: true, enumerable: true, get() {{ return this.y + this.height; }} }},
      left: {{ configurable: true, enumerable: true, get() {{ return this.x; }} }},
    }});
  }};
  const DOMRectReadOnlyNative = globalThis.__glassDOMRectReadOnlyConstructor || function DOMRectReadOnly(
    x = 0,
    y = 0,
    width = 0,
    height = 0,
  ) {{
    if (!(this instanceof DOMRectReadOnlyNative)) throw new TypeError("DOMRectReadOnly requires new");
    defineDomRectValues(this, x, y, width, height, false);
  }};
  if (!DOMRectReadOnlyNative.prototype.toJSON) {{
    Object.defineProperty(DOMRectReadOnlyNative.prototype, "toJSON", {{
      configurable: true,
      value() {{
        return {{
          x: this.x,
          y: this.y,
          width: this.width,
          height: this.height,
          top: this.top,
          right: this.right,
          bottom: this.bottom,
          left: this.left,
        }};
      }},
    }});
  }}
  const DOMRectNative = globalThis.__glassDOMRectConstructor || function DOMRect(
    x = 0,
    y = 0,
    width = 0,
    height = 0,
  ) {{
    if (!(this instanceof DOMRectNative)) throw new TypeError("DOMRect requires new");
    defineDomRectValues(this, x, y, width, height, true);
  }};
  try {{ Object.setPrototypeOf(DOMRectNative.prototype, DOMRectReadOnlyNative.prototype); }} catch (_error) {{}}
  globalThis.__glassDOMRectReadOnlyConstructor = DOMRectReadOnlyNative;
  globalThis.__glassDOMRectConstructor = DOMRectNative;
  globalThis.DOMRectReadOnly = DOMRectReadOnlyNative;
  globalThis.DOMRect = DOMRectNative;
  const ResizeObserverNative = globalThis.__glassResizeObserverConstructor || function ResizeObserver(callback) {{
    if (!(this instanceof ResizeObserverNative)) throw new TypeError("ResizeObserver requires new");
    if (typeof callback !== "function") throw new TypeError("ResizeObserver callback must be callable");
    Object.defineProperties(this, {{
      __glassCallback: {{ configurable: false, enumerable: false, value: callback }},
      __glassRegistrations: {{ configurable: false, enumerable: false, writable: true, value: [] }},
      __glassRecords: {{ configurable: false, enumerable: false, writable: true, value: [] }},
    }});
    resizeObservers.add(this);
  }};
  if (!ResizeObserverNative.prototype.observe) {{
    Object.defineProperties(ResizeObserverNative.prototype, {{
      observe: {{
        configurable: true,
        value(target, options = {{}}) {{
          if (!target || typeof target.nodeIndex !== "number") throw new TypeError("ResizeObserver target must be an Element");
          if (!options || typeof options !== "object") throw new TypeError("ResizeObserver options must be an object");
          const box = options.box === undefined ? "content-box" : String(options.box);
          if (!["content-box", "border-box", "device-pixel-content-box"].includes(box)) {{
            throw new TypeError("ResizeObserver box is unsupported");
          }}
          const registrations = this.__glassRegistrations;
          const existing = registrations.find((registration) => registration.target === target);
          if (existing) {{
            existing.box = box;
            return;
          }}
          registrations.push({{ target, box, last: null }});
        }},
      }},
      unobserve: {{
        configurable: true,
        value(target) {{
          this.__glassRegistrations = this.__glassRegistrations.filter(
            (registration) => registration.target !== target,
          );
        }},
      }},
      disconnect: {{
        configurable: true,
        value() {{
          this.__glassRegistrations = [];
          this.__glassRecords = [];
        }},
      }},
      takeRecords: {{
        configurable: true,
        value() {{
          const records = this.__glassRecords.slice();
          this.__glassRecords = [];
          return records;
        }},
      }},
    }});
  }}
  globalThis.__glassResizeObserverConstructor = ResizeObserverNative;
  globalThis.ResizeObserver = ResizeObserverNative;
  const normalizeIntersectionThresholds = (value) => {{
    const values = Array.isArray(value) ? value.slice() : [value === undefined ? 0 : value];
    if (values.length === 0 || values.length > 16) throw new RangeError("IntersectionObserver threshold limit exceeded");
    const normalized = values.map((entry) => Number(entry));
    if (normalized.some((entry) => !Number.isFinite(entry) || entry < 0 || entry > 1)) {{
      throw new RangeError("IntersectionObserver threshold must be between 0 and 1");
    }}
    return Array.from(new Set(normalized)).sort((left, right) => left - right);
  }};
  const normalizeIntersectionRootMargin = (value) => {{
    const raw = String(value === undefined ? "0px" : value).trim();
    const tokens = raw ? raw.split(/\s+/) : ["0px"];
    if (tokens.length < 1 || tokens.length > 4) throw new TypeError("IntersectionObserver rootMargin is invalid");
    const values = tokens.map((token) => {{
      if (!/^-?(?:\d+\.?\d*|\.\d+)px$/.test(token)) throw new TypeError("IntersectionObserver rootMargin only supports px lengths");
      const number = Number(token.slice(0, -2));
      if (!Number.isFinite(number)) throw new TypeError("IntersectionObserver rootMargin is invalid");
      return number;
    }});
    const expanded = values.length === 1
      ? [values[0], values[0], values[0], values[0]]
      : values.length === 2
        ? [values[0], values[1], values[0], values[1]]
        : values.length === 3
          ? [values[0], values[1], values[2], values[1]]
          : values;
    return {{
      values: expanded,
      text: expanded.map((entry) => String(entry) + "px").join(" "),
    }};
  }};
  const IntersectionObserverEntryNative = globalThis.__glassIntersectionObserverEntryConstructor || function IntersectionObserverEntry(time, init) {{
    if (!(this instanceof IntersectionObserverEntryNative)) throw new TypeError("IntersectionObserverEntry requires new");
    Object.defineProperties(this, {{
      time: {{ configurable: true, enumerable: true, value: Number(time) || 0 }},
      target: {{ configurable: true, enumerable: true, value: init.target || null }},
      rootBounds: {{ configurable: true, enumerable: true, value: init.rootBounds || null }},
      boundingClientRect: {{ configurable: true, enumerable: true, value: init.boundingClientRect || makeDomRect(null) }},
      intersectionRect: {{ configurable: true, enumerable: true, value: init.intersectionRect || makeDomRect(null) }},
      isIntersecting: {{ configurable: true, enumerable: true, value: Boolean(init.isIntersecting) }},
      intersectionRatio: {{ configurable: true, enumerable: true, value: Number(init.intersectionRatio) || 0 }},
      isVisible: {{ configurable: true, enumerable: true, value: Boolean(init.isVisible) }},
    }});
  }};
  globalThis.__glassIntersectionObserverEntryConstructor = IntersectionObserverEntryNative;
  globalThis.IntersectionObserverEntry = IntersectionObserverEntryNative;
  const IntersectionObserverNative = globalThis.__glassIntersectionObserverConstructor || function IntersectionObserver(callback, options = {{}}) {{
    if (!(this instanceof IntersectionObserverNative)) throw new TypeError("IntersectionObserver requires new");
    if (typeof callback !== "function") throw new TypeError("IntersectionObserver callback must be callable");
    if (!options || typeof options !== "object") throw new TypeError("IntersectionObserver options must be an object");
    const root = options.root === undefined ? null : options.root;
    if (root !== null && ![1, 9].includes(Number(root && root.nodeType))) throw new TypeError("IntersectionObserver root must be an Element or Document");
    const margin = normalizeIntersectionRootMargin(options.rootMargin);
    const thresholds = normalizeIntersectionThresholds(options.threshold);
    Object.defineProperties(this, {{
      __glassCallback: {{ configurable: false, enumerable: false, value: callback }},
      __glassRegistrations: {{ configurable: false, enumerable: false, writable: true, value: [] }},
      __glassRecords: {{ configurable: false, enumerable: false, writable: true, value: [] }},
      root: {{ configurable: false, enumerable: true, value: root }},
      rootMargin: {{ configurable: false, enumerable: true, value: margin.text }},
      thresholds: {{ configurable: false, enumerable: true, value: Object.freeze(thresholds.slice()) }},
    }});
    intersectionObservers.add(this);
  }};
  if (!IntersectionObserverNative.prototype.observe) {{
    Object.defineProperties(IntersectionObserverNative.prototype, {{
      observe: {{
        configurable: true,
        value(target) {{
          if (!target || Number(target.nodeType) !== 1) throw new TypeError("IntersectionObserver target must be an Element");
          const registrations = this.__glassRegistrations;
          const existing = registrations.find((registration) => registration.target === target);
          if (existing) {{
            existing.last = null;
            return;
          }}
          registrations.push({{
            target,
            root: this.root,
            rootMargin: normalizeIntersectionRootMargin(this.rootMargin).values,
            thresholds: this.thresholds.slice(),
            last: null,
          }});
        }},
      }},
      unobserve: {{
        configurable: true,
        value(target) {{
          this.__glassRegistrations = this.__glassRegistrations.filter(
            (registration) => registration.target !== target,
          );
        }},
      }},
      disconnect: {{
        configurable: true,
        value() {{
          this.__glassRegistrations = [];
          this.__glassRecords = [];
        }},
      }},
      takeRecords: {{
        configurable: true,
        value() {{
          const records = this.__glassRecords.slice();
          this.__glassRecords = [];
          return records;
        }},
      }},
    }});
  }}
  globalThis.__glassIntersectionObserverConstructor = IntersectionObserverNative;
  globalThis.IntersectionObserver = IntersectionObserverNative;
  const DOMExceptionNative = globalThis.__glassDOMExceptionConstructor || (() => {{
    const constructor = function DOMException(message = "", name = "Error") {{
      if (!(this instanceof constructor)) throw new TypeError("DOMException requires new");
      const normalizedName = String(name || "Error");
      const normalizedMessage = String(message || "");
      const codes = {{
        IndexSizeError: 1,
        DOMStringSizeError: 2,
        HierarchyRequestError: 3,
        WrongDocumentError: 4,
        InvalidCharacterError: 5,
        NoModificationAllowedError: 7,
        NotFoundError: 8,
        NotSupportedError: 9,
        InvalidStateError: 11,
        SyntaxError: 12,
        InvalidModificationError: 13,
        NamespaceError: 14,
        InvalidAccessError: 15,
        TypeMismatchError: 17,
        SecurityError: 18,
        NetworkError: 19,
        AbortError: 20,
        URLMismatchError: 21,
        QuotaExceededError: 22,
        TimeoutError: 23,
        InvalidNodeTypeError: 24,
        DataCloneError: 25,
      }};
      Object.defineProperties(this, {{
        name: {{ configurable: false, enumerable: true, value: normalizedName }},
        message: {{ configurable: false, enumerable: true, value: normalizedMessage }},
        code: {{ configurable: false, enumerable: true, value: codes[normalizedName] || 0 }},
      }});
    }};
    constructor.prototype = Object.create(Error.prototype);
    Object.defineProperty(constructor.prototype, "constructor", {{
      configurable: true,
      value: constructor,
    }});
    Object.defineProperty(constructor.prototype, "toString", {{
      configurable: true,
      value() {{ return this.name + ": " + this.message; }},
    }});
    return constructor;
  }})();
  globalThis.__glassDOMExceptionConstructor = DOMExceptionNative;
  globalThis.DOMException = DOMExceptionNative;
  const NodeNative = ensureNativeConstructor("Node", null);
  const CharacterDataNative = ensureNativeConstructor("CharacterData", NodeNative);
  const TextNative = ensureNativeConstructor("Text", CharacterDataNative);
  const CommentNative = ensureNativeConstructor("Comment", CharacterDataNative);
  const AttrNative = ensureNativeConstructor("Attr", NodeNative);
  const DocumentTypeNative = ensureNativeConstructor("DocumentType", NodeNative);
  const DocumentNative = ensureNativeConstructor("Document", NodeNative);
  const DocumentFragmentNative = ensureNativeConstructor("DocumentFragment", NodeNative);
  const ElementNative = ensureNativeConstructor("Element", NodeNative);
  const HTMLElementNative = ensureNativeConstructor("HTMLElement", ElementNative);
  const WindowNative = ensureNativeConstructor("Window", null);
  const LocationNative = ensureNativeConstructor("Location", null);
  const NodeListNative = ensureNativeConstructor("NodeList", null);
  const HtmlCollectionNative = ensureNativeConstructor("HTMLCollection", null);
  const NamedNodeMapNative = ensureNativeConstructor("NamedNodeMap", null);
  globalThis.Attr = AttrNative;
  globalThis.NamedNodeMap = NamedNodeMapNative;
  const characterDataTarget = (target) => {{
    if (!target || ![3, 8].includes(Number(target.nodeType))) throw new TypeError("CharacterData method called on a non-character-data node");
    return target;
  }};
  const characterDataText = (target) => {{
    const text = characterDataTarget(target);
    return String(Number(text.nodeType) === 8 ? text.nodeValue ?? "" : text.__glassTextValue ?? text.nodeValue ?? "");
  }};
  const characterDataOffset = (target, value) => {{
    const numeric = Number(value);
    const offset = Number.isNaN(numeric) ? 0 : Math.trunc(numeric);
    if (!Number.isFinite(numeric) || numeric < 0 || offset > characterDataText(target).length) {{
      throw new DOMExceptionNative("The character data offset is out of range", "IndexSizeError");
    }}
    return offset;
  }};
  const characterDataCount = (value) => {{
    const numeric = Number(value);
    if (Number.isNaN(numeric)) return 0;
    if (numeric < 0 || Number.isNaN(Math.trunc(numeric))) {{
      throw new DOMExceptionNative("The character data count is out of range", "IndexSizeError");
    }}
    return Number.isFinite(numeric) ? Math.max(0, Math.trunc(numeric)) : Number.MAX_SAFE_INTEGER;
  }};
  const defineCharacterDataProperty = (name, descriptor) => {{
    if (!Object.prototype.hasOwnProperty.call(CharacterDataNative.prototype, name)) {{
      Object.defineProperty(CharacterDataNative.prototype, name, descriptor);
    }}
  }};
  defineCharacterDataProperty("data", {{
    configurable: true,
    enumerable: true,
    get() {{ return characterDataText(this); }},
    set(next) {{ characterDataTarget(this).textContent = String(next); }},
  }});
  defineCharacterDataProperty("length", {{
    configurable: true,
    enumerable: true,
    get() {{ return characterDataText(this).length; }},
  }});
  defineCharacterDataProperty("substringData", {{
    configurable: true,
    value(offset, count) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      return text.slice(start, start + characterDataCount(count));
    }},
  }});
  defineCharacterDataProperty("appendData", {{
    configurable: true,
    value(value) {{
      const target = characterDataTarget(this);
      target.textContent = characterDataText(target) + String(value);
    }},
  }});
  defineCharacterDataProperty("insertData", {{
    configurable: true,
    value(offset, value) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      target.textContent = text.slice(0, start) + String(value) + text.slice(start);
    }},
  }});
  defineCharacterDataProperty("deleteData", {{
    configurable: true,
    value(offset, count) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      target.textContent = text.slice(0, start) + text.slice(start + characterDataCount(count));
    }},
  }});
  defineCharacterDataProperty("replaceData", {{
    configurable: true,
    value(offset, count, value) {{
      const target = characterDataTarget(this);
      const text = characterDataText(target);
      const start = characterDataOffset(target, offset);
      target.textContent = text.slice(0, start) + String(value) + text.slice(start + characterDataCount(count));
    }},
  }});
  for (const constructor of [NodeListNative, HtmlCollectionNative]) {{
    if (constructor.prototype && Object.getPrototypeOf(constructor.prototype) !== Array.prototype) {{
      try {{ Object.setPrototypeOf(constructor.prototype, Array.prototype); }} catch (_error) {{}}
    }}
  }}
  const elementConstructors = {{
    HTMLUnknownElement: HTMLElementNative,
    HTMLHtmlElement: HTMLElementNative,
    HTMLBodyElement: HTMLElementNative,
    HTMLFormElement: HTMLElementNative,
    HTMLInputElement: HTMLElementNative,
    HTMLTextAreaElement: HTMLElementNative,
    HTMLSelectElement: HTMLElementNative,
    HTMLOptionElement: HTMLElementNative,
    HTMLButtonElement: HTMLElementNative,
    HTMLAnchorElement: HTMLElementNative,
    HTMLIFrameElement: HTMLElementNative,
    HTMLFrameElement: HTMLElementNative,
  }};
  for (const name of Object.keys(elementConstructors)) {{
    elementConstructors[name] = ensureNativeConstructor(name, elementConstructors[name]);
  }}
  const elementPrototypeFor = (tagName) => {{
    const name = {{
      HTML: "HTMLHtmlElement",
      BODY: "HTMLBodyElement",
      FORM: "HTMLFormElement",
      INPUT: "HTMLInputElement",
      TEXTAREA: "HTMLTextAreaElement",
      SELECT: "HTMLSelectElement",
      OPTION: "HTMLOptionElement",
      BUTTON: "HTMLButtonElement",
      A: "HTMLAnchorElement",
      IFRAME: "HTMLIFrameElement",
      FRAME: "HTMLFrameElement",
    }}[tagName] || "HTMLUnknownElement";
    return elementConstructors[name].prototype;
  }};
  const frameBindingForNode = (nodeIndex) => {{
    const bindings = Array.isArray(globalThis.__glassFrameBindings)
      ? globalThis.__glassFrameBindings
      : [];
    return bindings.find((binding) => binding && binding.nodeIndex === nodeIndex) || null;
  }};
  const frameChildBindingForNode = (binding, nodeIndex) => {{
    const children = binding && Array.isArray(binding.children) ? binding.children : [];
    return children.find((child) => child && child.nodeIndex === nodeIndex) || null;
  }};
  const frameIdentifier = (binding) => String(
    binding && (binding.frameId || binding.contextId) || ""
  );
  const frameMutationNodes = globalThis.__glassFrameMutationNodes instanceof Map
    ? globalThis.__glassFrameMutationNodes
    : new Map();
  const frameMutationAttributes = globalThis.__glassFrameMutationAttributes instanceof Map
    ? globalThis.__glassFrameMutationAttributes
    : new Map();
  const frameMutationText = globalThis.__glassFrameMutationText instanceof Map
    ? globalThis.__glassFrameMutationText
    : new Map();
  const frameMutationParents = globalThis.__glassFrameMutationParents instanceof Map
    ? globalThis.__glassFrameMutationParents
    : new Map();
  const frameMutationChildren = globalThis.__glassFrameMutationChildren instanceof Map
    ? globalThis.__glassFrameMutationChildren
    : new Map();
  globalThis.__glassFrameMutationNodes = frameMutationNodes;
  globalThis.__glassFrameMutationAttributes = frameMutationAttributes;
  globalThis.__glassFrameMutationText = frameMutationText;
  globalThis.__glassFrameMutationParents = frameMutationParents;
  globalThis.__glassFrameMutationChildren = frameMutationChildren;
  const frameMutationKey = (binding, nodeIndex) =>
    frameIdentifier(binding) + "\\u0000" + String(nodeIndex);
  const frameMutationNode = (binding, nodeIndex) =>
    frameMutationNodes.get(frameMutationKey(binding, nodeIndex)) || null;
  const recordFrameMutation = (binding, command) => {{
    if (!binding || !command || typeof command !== "object") return;
    const kind = String(command.kind || "");
    const nodeIndex = Number(command.node_index);
    const key = frameMutationKey(binding, nodeIndex);
    const target = frameMutationNode(binding, nodeIndex);
    if (kind === "setAttribute") {{
      if (!target) return;
      const attributes = frameMutationAttributes.get(key) || {{}};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      const value = String(command.value);
      attributes[name] = value;
      frameMutationAttributes.set(key, attributes);
      if (oldValue !== value) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "removeAttribute") {{
      if (!target) return;
      const attributes = frameMutationAttributes.get(key) || {{}};
      const name = String(command.name).toLowerCase();
      const oldValue = Object.prototype.hasOwnProperty.call(attributes, name) ? attributes[name] : null;
      delete attributes[name];
      frameMutationAttributes.set(key, attributes);
      if (oldValue !== null) queueMutation({{ type: "attributes", target, attributeName: name, oldValue }});
      return;
    }}
    if (kind === "setTextContent") {{
      if (!target) return;
      const value = String(command.value);
      if ([3, 8].includes(Number(target.nodeType))) {{
        const oldValue = frameMutationText.has(key)
          ? frameMutationText.get(key)
          : String(target.nodeValue || "");
        frameMutationText.set(key, value);
        if (oldValue !== value) queueMutation({{ type: "characterData", target, oldValue }});
        return;
      }}
      const oldChildren = frameMutationChildren.get(key) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = oldChildren.map((index) => frameMutationNode(binding, index)).filter(Boolean);
      frameMutationChildren.set(key, addedIndexes);
      for (const childIndex of oldChildren) frameMutationParents.delete(frameMutationKey(binding, childIndex));
      for (const childIndex of addedIndexes) frameMutationParents.set(frameMutationKey(binding, childIndex), nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || value.length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "setInnerHtml") {{
      if (!target) return;
      const oldChildren = frameMutationChildren.get(key) || [];
      const addedNodes = Array.isArray(target.__glassChildren) ? target.__glassChildren.slice() : [];
      const addedIndexes = addedNodes
        .map((node) => Number(node && node.nodeIndex))
        .filter((index) => Number.isFinite(index));
      const removedNodes = oldChildren.map((index) => frameMutationNode(binding, index)).filter(Boolean);
      for (const childIndex of oldChildren) frameMutationParents.delete(frameMutationKey(binding, childIndex));
      frameMutationChildren.set(key, addedIndexes);
      for (const childIndex of addedIndexes) frameMutationParents.set(frameMutationKey(binding, childIndex), nodeIndex);
      if (oldChildren.length > 0 || addedNodes.length > 0 || String(command.value).length > 0) {{
        queueMutation({{ type: "childList", target, addedNodes, removedNodes }});
      }}
      return;
    }}
    if (kind === "removeNode") {{
      if (!target) return;
      const parentKey = frameMutationParents.get(key);
      const parent = frameMutationNode(binding, parentKey);
      if (parent) {{
        const childrenKey = frameMutationKey(binding, parentKey);
        const children = frameMutationChildren.get(childrenKey) || [];
        const position = children.indexOf(nodeIndex);
        queueMutation({{
          type: "childList",
          target: parent,
          removedNodes: [target],
          previousSibling: position > 0 ? frameMutationNode(binding, children[position - 1]) : null,
          nextSibling: position >= 0 ? frameMutationNode(binding, children[position + 1]) : null,
        }});
        frameMutationChildren.set(childrenKey, children.filter((index) => index !== nodeIndex));
      }}
      frameMutationParents.delete(key);
      return;
    }}
    if (kind === "appendChild" || kind === "insertBefore") {{
      const parentIndex = Number(command.parent_index);
      const parent = frameMutationNode(binding, parentIndex);
      const childIndex = Number(command.child_index);
      const child = frameMutationNode(binding, childIndex);
      if (!parent || !child) return;
      const parentKey = frameMutationKey(binding, parentIndex);
      const childKey = frameMutationKey(binding, childIndex);
      const oldParentIndex = frameMutationParents.get(childKey);
      if (oldParentIndex !== undefined) {{
        const oldParentKey = frameMutationKey(binding, oldParentIndex);
        const oldChildren = frameMutationChildren.get(oldParentKey) || [];
        const oldPosition = oldChildren.indexOf(childIndex);
        const oldParent = frameMutationNode(binding, oldParentIndex);
        if (oldParent) queueMutation({{
          type: "childList",
          target: oldParent,
          removedNodes: [child],
          previousSibling: oldPosition > 0 ? frameMutationNode(binding, oldChildren[oldPosition - 1]) : null,
          nextSibling: oldPosition >= 0 ? frameMutationNode(binding, oldChildren[oldPosition + 1]) : null,
        }});
        frameMutationChildren.set(oldParentKey, oldChildren.filter((index) => index !== childIndex));
      }}
      const children = (frameMutationChildren.get(parentKey) || []).filter((index) => index !== childIndex);
      const beforeIndex = kind === "insertBefore" && command.before_index != null
        ? Number(command.before_index)
        : null;
      const insertion = beforeIndex === null ? children.length : Math.max(0, children.indexOf(beforeIndex));
      const previousSibling = insertion > 0 ? frameMutationNode(binding, children[insertion - 1]) : null;
      const nextSibling = frameMutationNode(binding, children[insertion]);
      children.splice(insertion, 0, childIndex);
      frameMutationChildren.set(parentKey, children);
      frameMutationParents.set(childKey, parentIndex);
      queueMutation({{ type: "childList", target: parent, addedNodes: [child], previousSibling, nextSibling }});
    }}
  }};
  globalThis.__glassRecordFrameMutation = recordFrameMutation;
  const frameBatchableCommand = (command) => command && [
    "setValue", "setSelection", "setChecked", "setSelected",
    "setAttribute", "removeAttribute", "setTextContent", "setInnerHtml",
    "removeNode", "createElement", "createTextNode", "createComment", "createDocumentType", "appendChild",
    "insertBefore", "setCustomValidity",
  ].includes(String(command.kind));
  const queueFrameCommand = (binding, command) => {{
    if (!binding || binding.sameOrigin !== true) throw crossOriginSecurityError("document");
    if (suppressHostCommands > 0) return;
    if (typeof globalThis.__glassRecordFrameMutation === "function") {{
      globalThis.__glassRecordFrameMutation(binding, command);
    }}
    const frameId = frameIdentifier(binding);
    const sourceFrameId = String(host.frame_id || host.context_id || "");
    if (!frameId || !sourceFrameId || frameId === sourceFrameId) {{
      throw new TypeError("native frame command target is invalid");
    }}
    const target = activeCommands();
    const previous = target[target.length - 1];
    if (frameBatchableCommand(command)
        && previous && previous.kind === "frameScript"
        && previous.frame_id === frameId
        && previous.source_frame_id === sourceFrameId
        && (frameBatchableCommand(previous.command)
          || previous.command && previous.command.kind === "frameScriptBatch")) {{
      if (previous.command.kind === "frameScriptBatch") previous.command.commands.push(command);
      else previous.command = {{ kind: "frameScriptBatch", commands: [previous.command, command] }};
      return;
    }}
    pushCommand({{
      kind: "frameScript",
      frame_id: frameId,
      source_frame_id: sourceFrameId,
      command,
    }});
  }};
  const frameBindingForId = (frameId) => {{
    const visit = (bindings) => {{
      for (const binding of bindings) {{
        if (binding && frameIdentifier(binding) === frameId) return binding;
        const nested = binding && Array.isArray(binding.children) ? visit(binding.children) : null;
        if (nested) return nested;
      }}
      return null;
    }};
    const bindings = Array.isArray(globalThis.__glassFrameBindings)
      ? globalThis.__glassFrameBindings
      : [];
    return visit(bindings);
  }};
  const frameTopologyKey = (binding) => JSON.stringify([
    frameIdentifier(binding),
    String(binding && binding.url || ""),
    Number(binding && binding.revision || 0),
    Boolean(binding && binding.sameOrigin),
    Array.isArray(binding && binding.children)
      ? binding.children.map((child) => frameTopologyKey(child))
      : [],
  ]);
  const frameChildBindings = (binding) =>
    binding && Array.isArray(binding.children) ? binding.children : [];
  const currentFrameWindow = (binding, ownerFrameElement, parentWindow, topWindow) =>
    makeFrameWindow(binding, ownerFrameElement, parentWindow, topWindow);
  const frameDocumentCache = globalThis.__glassFrameDocumentCache instanceof Map
    ? globalThis.__glassFrameDocumentCache
    : new Map();
  const frameWindowCache = globalThis.__glassFrameWindowCache instanceof Map
    ? globalThis.__glassFrameWindowCache
    : new Map();
  globalThis.__glassFrameDocumentCache = frameDocumentCache;
  globalThis.__glassFrameWindowCache = frameWindowCache;
  const projectedFrameMatches = (element, selector) => {{
    return matchesSelector(element, selector);
  }};
  const makeFrameDocument = (
    binding,
    ownerFrameElement = null,
    parentWindow = globalThis,
    topWindow = globalThis,
  ) => {{
    const currentBinding = frameBindingForId(frameIdentifier(binding)) || binding;
    const currentFrameId = frameIdentifier(currentBinding);
    const topologyKey = frameTopologyKey(currentBinding);
    const cached = frameDocumentCache.get(currentFrameId);
    if (cached && cached.url === currentBinding.url && cached.revision === currentBinding.revision
        && cached.topologyKey === topologyKey) return cached.document;
    const snapshot = currentBinding.document && typeof currentBinding.document === "object"
      ? currentBinding.document
      : {{ title: "", visibleText: "", elements: [] }};
    const frameScriptNodeObjectsByFrame = globalThis.__glassFrameScriptNodeObjectsByFrame instanceof Map
      ? globalThis.__glassFrameScriptNodeObjectsByFrame
      : new Map();
    const frameScriptNodeGenerations = globalThis.__glassFrameScriptNodeGenerations instanceof Map
      ? globalThis.__glassFrameScriptNodeGenerations
      : new Map();
    globalThis.__glassFrameScriptNodeObjectsByFrame = frameScriptNodeObjectsByFrame;
    globalThis.__glassFrameScriptNodeGenerations = frameScriptNodeGenerations;
    const frameGeneration = String(currentBinding.generation || "");
    let frameScriptNodeObjects = frameScriptNodeObjectsByFrame.get(currentFrameId);
    if (!(frameScriptNodeObjects instanceof Map)
        || frameScriptNodeGenerations.get(currentFrameId) !== frameGeneration) {{
      frameScriptNodeObjects = new Map();
      frameScriptNodeObjectsByFrame.set(currentFrameId, frameScriptNodeObjects);
      frameScriptNodeGenerations.set(currentFrameId, frameGeneration);
    }}
    const frameScriptNodeAliasesByIndex = new Map();
    for (const identity of Array.isArray(snapshot.scriptNodes) ? snapshot.scriptNodes : []) {{
      const temporaryIndex = Number(identity && identity.temporaryIndex);
      const nodeIndex = Number(identity && identity.nodeIndex);
      const object = frameScriptNodeObjects.get(temporaryIndex);
      if (!object || !Number.isSafeInteger(nodeIndex) || nodeIndex < 0) continue;
      object.nodeIndex = nodeIndex;
      frameScriptNodeAliasesByIndex.set(nodeIndex, object);
    }}
    let frameDocumentTitle = String(snapshot.title || "");
    let frameDocument;
    const frameElements = (Array.isArray(snapshot.elements) ? snapshot.elements : []).map((entry) => {{
      const existing = frameScriptNodeAliasesByIndex.get(Number(entry.nodeIndex));
      if (existing && typeof existing.__glassRefresh === "function") {{
        existing.__glassRefresh(entry);
        return existing;
      }}
      const attributes = entry.attributes && typeof entry.attributes === "object" ? entry.attributes : {{}};
      const attributeNamespaces = {{}};
      for (const [name, namespace] of Object.entries(
        entry.attributeNamespaces && typeof entry.attributeNamespaces === "object"
          ? entry.attributeNamespaces
          : {{}},
      )) attributeNamespaces[name] = String(namespace);
      let textContent = String(entry.text || "");
      let innerHtml = String(entry.innerHtml || "");
      const namespaceURI = namespaceUriForEntry(entry);
      let value = entry.value == null ? "" : entry.value;
      let checked = Boolean(entry.checked);
      let selected = Boolean(entry.selected);
      let disabled = Boolean(entry.disabled);
      let hidden = Boolean(entry.hidden);
      let multiple = Object.prototype.hasOwnProperty.call(attributes, "multiple");
      const frameAttributeNamespace = (name) => {{
        const value = attributeNamespaces[String(name)];
        return value === undefined || value === "" ? null : String(value);
      }};
      const setFrameNamespacedAttribute = (namespace, name, nextValue) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const qualified = qualifiedAttributeName(name, namespaceURI);
      const stringValue = String(nextValue);
      if (stringValue.length > storageValueLimit) throw new RangeError("native frame attribute value exceeds its limit");
      attributes[qualified] = stringValue;
      if (namespaceURI === null) delete attributeNamespaces[qualified];
      else attributeNamespaces[qualified] = namespaceURI;
      projected.__glassSyncContent();
      if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
      queueFrameCommand(currentBinding, {{
          kind: "setAttribute",
          node_index: entry.nodeIndex,
          name: qualified,
          value: stringValue,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const frameNamespacedAttributeValue = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = attributeLocalName(name);
        for (const key of Object.keys(attributes)) {{
          if (attributeLocalName(key) === localName && frameAttributeNamespace(key) === namespaceURI) return attributes[key];
        }}
        return null;
      }};
      const removeFrameNamespacedAttribute = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = attributeLocalName(name);
        const key = Object.keys(attributes).find((candidate) =>
          attributeLocalName(candidate) === localName && frameAttributeNamespace(candidate) === namespaceURI);
        if (key === undefined) return;
        delete attributes[key];
        delete attributeNamespaces[key];
        projected.__glassSyncContent();
        if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        queueFrameCommand(currentBinding, {{
          kind: "removeAttribute",
          node_index: entry.nodeIndex,
          name: key,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const projected = {{
        nodeIndex: entry.nodeIndex,
        parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
        tagName: tagNameForEntry(entry),
        nodeType: 1,
        nodeName: tagNameForEntry(entry),
        localName: String(entry.tagName || "").toLowerCase(),
        namespaceURI,
        id: attributes.id || "",
        className: attributes.class || "",
        value,
        checked,
        selected,
        disabled,
        hidden,
        multiple,
        getBoundingClientRect() {{ return makeDomRect(geometryForNode(projected)); }},
        getClientRects() {{
          const geometry = geometryForNode(projected);
          return geometry.width > 0 && geometry.height > 0
            ? asNodeList([makeDomRect(geometry)])
            : asNodeList([]);
        }},
        get clientWidth() {{ return Number(geometryForNode(projected).width) || 0; }},
        get clientHeight() {{ return Number(geometryForNode(projected).height) || 0; }},
        get offsetWidth() {{ return Number(geometryForNode(projected).width) || 0; }},
        get offsetHeight() {{ return Number(geometryForNode(projected).height) || 0; }},
        get scrollWidth() {{ return Number(geometryForNode(projected).width) || 0; }},
        get scrollHeight() {{ return Number(geometryForNode(projected).height) || 0; }},
        getAttribute(name) {{
          const key = String(name).toLowerCase();
          for (const attribute of Object.keys(attributes)) {{
            if (attribute.toLowerCase() === key) return attributes[attribute];
          }}
          return null;
        }},
        getAttributeNS(namespace, name) {{ return frameNamespacedAttributeValue(namespace, name); }},
        getAttributeNames() {{ return Object.keys(attributes); }},
        hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
        matches(selector) {{ return matchesSelector(projected, selector); }},
        closest(selector) {{
          let current = projected;
          while (current) {{
            if (matchesSelector(current, selector)) return current;
            current = current.parentElement;
          }}
          return null;
        }},
        querySelector(selector) {{
          return descendantsInTree(projected, (candidate) => matchesSelector(candidate, selector))[0] || null;
        }},
        querySelectorAll(selector) {{
          return asNodeList(descendantsInTree(projected, (candidate) => matchesSelector(candidate, selector)));
        }},
        getElementsByTagName(name) {{
          const value = String(name).toLowerCase();
          return asHtmlCollection(descendantsInTree(projected, (candidate) =>
            value === "*" || candidate.tagName.toLowerCase() === value));
        }},
        getElementsByClassName(name) {{
          const value = String(name).trim();
          if (!value) return asHtmlCollection([]);
          const tokens = value.split(/\s+/);
          return asHtmlCollection(descendantsInTree(projected, (candidate) =>
            tokens.every((token) => String(candidate.className).split(/\s+/).includes(token))));
        }},
        focus() {{
          if (this.disabled || this.hidden) return;
          dispatchTarget(this, createEvent("focus"));
          queueFrameCommand(currentBinding, {{ kind: "focus", node_index: entry.nodeIndex }});
        }},
        blur() {{
          dispatchTarget(this, createEvent("blur"));
          queueFrameCommand(currentBinding, {{ kind: "blur", node_index: entry.nodeIndex }});
        }},
        click() {{
          if (this.disabled || this.hidden) return;
          const event = createEvent("click", {{ bubbles: true, cancelable: true }});
          if (!dispatchTarget(this, event)) return;
          queueFrameCommand(currentBinding, {{ kind: "click", node_index: entry.nodeIndex }});
        }},
        addEventListener(type, callback, options) {{
          addListener(ownerFor(projected), type, callback, options);
        }},
        removeEventListener(type, callback, options) {{
          removeListener(ownerFor(projected), type, callback, options);
        }},
        dispatchEvent(event) {{
          return dispatchTarget(this, event);
        }},
        setAttribute(name, nextValue) {{
          const key = String(name).toLowerCase();
          const stringValue = String(nextValue);
          attributes[key] = stringValue;
          delete attributeNamespaces[key];
          if (key === "disabled") disabled = true;
          if (key === "hidden") hidden = true;
          if (key === "multiple") multiple = true;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "setAttribute", node_index: entry.nodeIndex, name: key, value: stringValue, namespace_uri: "" }});
        }},
        setAttributeNS(namespace, qualifiedName, nextValue) {{
          setFrameNamespacedAttribute(namespace, qualifiedName, nextValue);
        }},
        removeAttribute(name) {{
          const key = String(name).toLowerCase();
          delete attributes[key];
          delete attributeNamespaces[key];
          if (key === "disabled") disabled = false;
          if (key === "hidden") hidden = false;
          if (key === "multiple") multiple = false;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "removeAttribute", node_index: entry.nodeIndex, name: key, namespace_uri: "" }});
        }},
        removeAttributeNS(namespace, name) {{
          removeFrameNamespacedAttribute(namespace, name);
        }},
        appendChild(child) {{
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.appendChild(fragmentChild);
            child.__glassChildren = [];
            return child;
          }}
          if (!child || typeof child.nodeIndex !== "number") throw new TypeError("child must be a native node");
          if (child === projected) throw new TypeError("a node cannot contain itself");
          let ancestor = projected;
          while (ancestor) {{
            if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
            ancestor = ancestor.__glassParent || null;
          }}
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          projected.__glassChildren.push(child);
          child.__glassParent = projected;
          child.parentIndex = projected.nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "appendChild", parent_index: entry.nodeIndex, child_index: child.nodeIndex }});
          return child;
        }},
        insertBefore(child, before) {{
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.insertBefore(fragmentChild, before);
            child.__glassChildren = [];
            return child;
          }}
          if (before == null) return this.appendChild(child);
          if (!child || typeof child.nodeIndex !== "number"
              || typeof before.nodeIndex !== "number") throw new TypeError("insertBefore requires native nodes");
          if (before.__glassParent !== projected) throw new TypeError("reference node is not a child");
          if (child === before) return child;
          let ancestor = projected;
          while (ancestor) {{
            if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
            ancestor = ancestor.__glassParent || null;
          }}
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          const index = projected.__glassChildren.indexOf(before);
          projected.__glassChildren.splice(index < 0 ? projected.__glassChildren.length : index, 0, child);
          child.__glassParent = projected;
          child.parentIndex = projected.nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{
            kind: "insertBefore",
            parent_index: entry.nodeIndex,
            child_index: child.nodeIndex,
            before_index: before.nodeIndex,
          }});
          return child;
        }},
        remove() {{
          const parent = projected.__glassParent || null;
          if (!parent && projected.parentIndex === null) return;
          const commitRemoval = !projected.__glassCreated || projected.parentIndex !== null;
          queueFragmentChildRemoval(parent, projected);
          if (parent && Array.isArray(parent.__glassChildren)) {{
            parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== projected);
          }}
          projected.__glassParent = null;
          projected.parentIndex = null;
          detachFrameSubtree(projected);
          if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
          if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: entry.nodeIndex }});
        }},
        removeChild(child) {{
          if (!child || child.__glassParent !== projected) {{
            throw new TypeError("child is not contained by this element");
          }}
          child.remove();
          return child;
        }},
      }};
      installCommonAttributeProperties(projected, {{
        disabled: () => disabled,
        hidden: () => hidden,
        multiple: () => multiple,
      }}, String(currentBinding.url));
      Object.defineProperty(projected, "__glassChildren", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: [],
      }});
      Object.defineProperty(projected, "__glassEventOwner", {{
        enumerable: false,
        configurable: false,
        value: "frame:" + currentFrameId + ":node:" + entry.nodeIndex,
      }});
      Object.defineProperty(projected, "__glassParent", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: null,
      }});
      Object.defineProperty(projected, "__glassCreated", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: false,
      }});
      Object.defineProperty(projected, "__glassAttached", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: true,
      }});
      Object.defineProperty(projected, "__glassAttributeSource", {{
        enumerable: false,
        configurable: false,
        value: () => attributes,
      }});
      Object.defineProperty(projected, "__glassAttributeNamespace", {{
        enumerable: false,
        configurable: false,
        value: (name) => frameAttributeNamespace(name),
      }});
      Object.defineProperty(projected, "__glassGeometrySource", {{
        enumerable: false,
        configurable: false,
        value: () => {{
          const active = frameBindingForId(currentFrameId) || currentBinding;
          const geometry = active && active.document && Array.isArray(active.document.geometry)
            ? active.document.geometry
            : [];
          return geometry.find((candidate) => Number(candidate.nodeIndex) === Number(entry.nodeIndex)) || null;
        }},
      }});
      Object.defineProperty(projected, "__glassSyncContent", {{
        enumerable: false,
        configurable: false,
        value(clearEmpty = false) {{
          if (projected.__glassChildren.length > 0) {{
            innerHtml = projected.__glassChildren.map(child => child.__glassMarkup).join("");
            textContent = projected.__glassChildren.map(child => child.__glassTextValue).join("");
          }} else if (clearEmpty) {{
            innerHtml = "";
            textContent = "";
          }}
          if (projected.__glassParent && typeof projected.__glassParent.__glassSyncContent === "function") projected.__glassParent.__glassSyncContent(clearEmpty);
        }},
      }});
      Object.defineProperty(projected, "__glassTextValue", {{
        enumerable: false,
        configurable: false,
        get() {{ return textContent; }},
      }});
      Object.defineProperty(projected, "__glassMarkup", {{
        enumerable: false,
        configurable: false,
        get() {{
          const markup = Object.keys(attributes)
            .sort()
            .map(name => " " + name + "=\"" + escapeHtmlText(attributes[name]).replace(/\"/g, "&quot;") + "\"")
            .join("");
          const opening = "<" + projected.localName + markup + ">";
          if (["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"].includes(projected.localName)) return opening;
          const content = projected.__glassChildren.length > 0
            ? projected.__glassChildren.map(child => child.__glassMarkup).join("")
            : innerHtml;
          return opening + content + "</" + projected.localName + ">";
        }},
      }});
      Object.defineProperty(projected, "outerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return projected.__glassMarkup; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element outerHTML exceeds its limit");
          const owner = projected.__glassParent || null;
          if (!owner || typeof owner.insertBefore !== "function") throw new TypeError("outerHTML requires an attached element");
          const fragment = makeFrameDocumentFragment();
          populateDetachedFragment(fragment, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment);
          for (const child of fragment.__glassChildren.slice()) owner.insertBefore(child, projected);
          projected.remove();
        }},
      }});
      Object.defineProperty(projected, "parentElement", {{
        enumerable: false,
        configurable: false,
        get() {{ return projected.__glassParent && projected.__glassParent.nodeType === 1 ? projected.__glassParent : null; }},
      }});
      Object.defineProperty(projected, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{
          if (projected.__glassParent) return projected.__glassParent;
          const owner = projected.ownerDocument;
          return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(projected)
            ? owner
            : null;
        }},
      }});
      for (const property of ["textContent", "innerText"]) {{
        Object.defineProperty(projected, property, {{
            enumerable: true,
            configurable: false,
            get() {{ return textContent; }},
            set(next) {{
              const value = String(next);
              if (value.length > storageValueLimit) throw new RangeError("native frame element textContent exceeds its limit");
              for (const child of projected.__glassChildren) {{
                child.__glassParent = null;
                child.parentIndex = null;
                detachFrameSubtree(child);
              }}
              projected.__glassChildren = [];
              innerHtml = "";
              textContent = "";
              suppressHostCommands += 1;
              try {{
                if (value) projected.appendChild(makeFrameDetachedText(value));
              }} finally {{
                suppressHostCommands -= 1;
              }}
              projected.__glassSyncContent(true);
              queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: entry.nodeIndex, value }});
            }},
          }});
        }}
      Object.defineProperty(projected, "innerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return innerHtml; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element innerHTML exceeds its limit");
          for (const child of projected.__glassChildren) {{
            child.__glassParent = null;
            child.parentIndex = null;
            detachFrameSubtree(child);
          }}
          projected.__glassChildren = [];
          innerHtml = "";
          textContent = "";
          suppressHostCommands += 1;
          try {{
          populateDetachedFragment(projected, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment);
          }} finally {{
            suppressHostCommands -= 1;
          }}
          projected.__glassSyncContent(true);
          queueFrameCommand(currentBinding, {{ kind: "setInnerHtml", node_index: entry.nodeIndex, value }});
        }},
      }});
      Object.defineProperty(projected, "value", {{
        enumerable: true,
        configurable: false,
        get() {{ return value; }},
        set(nextValue) {{
          value = String(nextValue);
          queueFrameCommand(currentBinding, {{ kind: "setValue", node_index: entry.nodeIndex, value }});
        }},
      }});
      Object.defineProperty(projected, "checked", {{
        enumerable: true,
        configurable: false,
        get() {{ return checked; }},
        set(nextValue) {{
          checked = Boolean(nextValue);
          queueFrameCommand(currentBinding, {{ kind: "setChecked", node_index: entry.nodeIndex, checked }});
        }},
      }});
      Object.defineProperty(projected, "selected", {{
        enumerable: true,
        configurable: false,
        get() {{ return selected; }},
        set(nextValue) {{
          selected = Boolean(nextValue);
          queueFrameCommand(currentBinding, {{ kind: "setSelected", node_index: entry.nodeIndex, selected }});
        }},
      }});
      Object.defineProperty(projected, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameDocument; }},
      }});
      installAttributeNodeSurface(projected, () => frameDocument);
      const childBinding = frameChildBindingForNode(currentBinding, entry.nodeIndex);
      if (["IFRAME", "FRAME"].includes(projected.tagName)) {{
        Object.defineProperty(projected, "contentWindow", {{
          enumerable: false,
          configurable: false,
          get() {{
            return childBinding
              ? currentFrameWindow(childBinding, projected, currentFrameWindow(currentBinding, ownerFrameElement, parentWindow, topWindow), topWindow)
              : null;
          }},
        }});
        Object.defineProperty(projected, "contentDocument", {{
          enumerable: false,
          configurable: false,
          get() {{
            if (!childBinding || !childBinding.sameOrigin) return null;
            const currentWindow = currentFrameWindow(currentBinding, ownerFrameElement, parentWindow, topWindow);
            return makeFrameDocument(childBinding, projected, currentWindow, topWindow);
          }},
        }});
      }}
      return projected;
    }});
    const frameNodeSnapshots = Array.isArray(snapshot.nodes) ? snapshot.nodes : [];
    const frameTextNodes = frameNodeSnapshots
      .filter((entry) => entry && [3, 8].includes(Number(entry.nodeType)))
      .map((entry) => {{
        const existing = frameScriptNodeAliasesByIndex.get(Number(entry.nodeIndex));
        if (existing && typeof existing.__glassRefresh === "function") {{
          existing.__glassRefresh(entry);
          return existing;
        }}
        const isComment = Number(entry.nodeType) === 8;
        let textContent = String(entry.nodeValue || "");
        const text = {{
          nodeIndex: entry.nodeIndex,
          parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
          nodeType: isComment ? 8 : 3,
          nodeName: isComment ? "#comment" : "#text",
          nodeValue: textContent,
          remove() {{
            const parent = text.__glassParent || null;
            if (!parent && text.parentIndex === null) return;
            const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
            queueFragmentChildRemoval(parent, text);
            if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
            text.__glassParent = null;
            text.parentIndex = null;
            text.__glassAttached = false;
            if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
            if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: text.nodeIndex }});
          }},
        }};
        Object.defineProperty(text, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
        Object.defineProperty(text, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: false }});
        Object.defineProperty(text, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: true }});
    Object.defineProperty(text, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return isComment ? "" : textContent; }} }});
        Object.defineProperty(text, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return isComment ? "<!--" + textContent + "-->" : escapeHtmlText(textContent); }} }});
        Object.defineProperty(text, "parentElement", {{
          enumerable: false,
          configurable: false,
          get() {{
            let current = text.__glassParent || null;
            while (current && current.nodeType !== 1) current = current.__glassParent || null;
            return current || null;
          }},
        }});
        Object.defineProperty(text, "parentNode", {{
          enumerable: false,
          configurable: false,
          get() {{
            if (text.__glassParent) return text.__glassParent;
            const owner = text.ownerDocument;
            return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
              ? owner
              : null;
          }},
        }});
        Object.defineProperty(text, "nodeValue", {{
          enumerable: true,
          configurable: false,
          get() {{ return textContent; }},
          set(next) {{ text.textContent = next; }},
        }});
        Object.defineProperty(text, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return frameDocument; }} }});
        Object.defineProperty(text, "textContent", {{
          enumerable: true,
          configurable: false,
          get() {{ return textContent; }},
          set(next) {{
            const value = String(next);
            if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
            textContent = value;
            if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent();
            queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: text.nodeIndex, value }});
          }},
        }});
        Object.defineProperty(text, "data", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ text.textContent = next; }} }});
        try {{ Object.setPrototypeOf(text, isComment ? CommentNative.prototype : TextNative.prototype); }} catch (_error) {{}}
        return text;
      }});
    const frameDocumentTypeNodes = frameNodeSnapshots
      .filter((entry) => entry && Number(entry.nodeType) === 10)
      .map((entry) => {{
        const existing = frameScriptNodeAliasesByIndex.get(Number(entry.nodeIndex));
        const documentType = existing || {{
          nodeIndex: entry.nodeIndex,
          parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
          nodeType: 10,
          nodeName: String(entry.nodeName || ""),
          nodeValue: null,
          name: String(entry.nodeName || ""),
          publicId: entry.publicId == null ? "" : String(entry.publicId),
          systemId: entry.systemId == null ? "" : String(entry.systemId),
          __glassChildren: [],
          __glassParent: null,
          textContent: null,
          remove() {{
            const parent = documentType.__glassParent || null;
            if (!parent && documentType.parentIndex == null) return;
            const owner = parent || (documentType.parentIndex === 0 ? frameDocument : null);
            if (owner && Array.isArray(owner.__glassChildren)) owner.__glassChildren = owner.__glassChildren.filter(candidate => candidate !== documentType);
            documentType.__glassParent = null;
            documentType.parentIndex = null;
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: documentType.nodeIndex }});
          }},
        }};
        if (!Object.prototype.hasOwnProperty.call(documentType, "__glassRefresh")) {{
          Object.defineProperty(documentType, "__glassRefresh", {{ enumerable: false, configurable: false, value(nextEntry) {{
            documentType.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
            documentType.name = String(nextEntry.nodeName || documentType.name || "");
            documentType.nodeName = documentType.name;
            documentType.publicId = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
            documentType.systemId = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
          }} }});
        }}
        if (existing && typeof documentType.__glassRefresh === "function") documentType.__glassRefresh(entry);
        if (!Object.prototype.hasOwnProperty.call(documentType, "ownerDocument")) Object.defineProperty(documentType, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return frameDocument; }} }});
        if (!Object.prototype.hasOwnProperty.call(documentType, "parentNode")) Object.defineProperty(documentType, "parentNode", {{ enumerable: false, configurable: false, get() {{ return documentType.__glassParent || null; }} }});
        if (!Object.prototype.hasOwnProperty.call(documentType, "parentElement")) Object.defineProperty(documentType, "parentElement", {{ enumerable: false, configurable: false, get() {{
          const parent = documentType.__glassParent || null;
          return parent && parent.nodeType === 1 ? parent : null;
        }} }});
        try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
        return documentType;
      }});
    const frameNodesByIndex = new Map([
      ...frameElements.map((element) => [element.nodeIndex, element]),
      ...frameTextNodes.map((text) => [text.nodeIndex, text]),
      ...frameDocumentTypeNodes.map((documentType) => [documentType.nodeIndex, documentType]),
    ]);
    const registerFrameSubtree = (node) => {{
      if (node.nodeType === 1 && !frameElements.includes(node)) frameElements.push(node);
      node.__glassAttached = true;
      for (const child of node.__glassChildren || []) registerFrameSubtree(child);
    }};
    const detachFrameSubtree = (node) => {{
      node.__glassAttached = false;
      for (const child of node.__glassChildren || []) detachFrameSubtree(child);
    }};
    for (const node of [...frameElements, ...frameTextNodes, ...frameDocumentTypeNodes]) {{
      node.__glassChildren = [];
      node.__glassParent = null;
      node.__glassAttached = true;
    }}
    if (frameNodeSnapshots.length > 0) {{
      for (const snapshotNode of frameNodeSnapshots) {{
        const parent = frameNodesByIndex.get(snapshotNode.nodeIndex);
        if (!parent || !Array.isArray(snapshotNode.children)) continue;
        for (const childIndex of snapshotNode.children) {{
          const child = frameNodesByIndex.get(childIndex);
          if (!child) continue;
          parent.__glassChildren.push(child);
          child.__glassParent = parent;
        }}
      }}
    }} else {{
      for (const element of frameElements) {{
        if (element.parentIndex === null) continue;
        const parent = frameElements.find(candidate => candidate.nodeIndex === element.parentIndex);
        if (!parent) continue;
        parent.__glassChildren.push(element);
        element.__glassParent = parent;
      }}
    }}
    for (const snapshotNode of frameNodeSnapshots) {{
      const nodeIndex = Number(snapshotNode.nodeIndex);
      const key = frameMutationKey(currentBinding, nodeIndex);
      const node = frameNodesByIndex.get(nodeIndex);
      if (node) frameMutationNodes.set(key, node);
      if (snapshotNode.parentIndex !== null && snapshotNode.parentIndex !== undefined) {{
        frameMutationParents.set(key, Number(snapshotNode.parentIndex));
      }}
      frameMutationChildren.set(key, Array.isArray(snapshotNode.children)
        ? snapshotNode.children.map(Number)
        : []);
      if ([3, 8].includes(Number(snapshotNode.nodeType))) {{
        frameMutationText.set(key, String(snapshotNode.nodeValue || ""));
      }} else if (node && node.nodeType === 1) {{
        frameMutationAttributes.set(key, {{ ...(frameElements.find((element) => element.nodeIndex === nodeIndex)?.__glassAttributeSource?.() || {{}}) }});
      }}
    }}
    for (const element of frameElements) defineTreeAccessors(element);
    for (const text of frameTextNodes) defineTreeAccessors(text);
    for (const documentType of frameDocumentTypeNodes) defineTreeAccessors(documentType);
    for (const element of frameElements) {{
      installClassList(element);
      installElementStyleAndDataset(element);
    }}
    const makeFrameDetachedElement = (tagName, namespace = HTML_NAMESPACE) => {{
      const normalized = String(tagName).toLowerCase();
      if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalized)) throw new TypeError("invalid element name");
      const namespaceURI = normalizeElementNamespace(namespace);
      let nodeIndex = allocateTemporaryNodeIndex();
      const attributes = {{}};
      const attributeNamespaces = {{}};
      let textContent = "";
      let innerHtml = "";
      let value = "";
      let checked = false;
      let selected = false;
      let disabled = false;
      let hidden = false;
      let multiple = false;
      const frameAttributeNamespace = (name) => {{
        const value = attributeNamespaces[String(name)];
        return value === undefined || value === "" ? null : String(value);
      }};
      const setFrameNamespacedAttribute = (namespace, name, nextValue) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const qualified = qualifiedAttributeName(name, namespaceURI);
        const stringValue = String(nextValue);
        if (stringValue.length > storageValueLimit) throw new RangeError("native frame attribute value exceeds its limit");
        attributes[qualified] = stringValue;
        if (namespaceURI === null) delete attributeNamespaces[qualified];
        else attributeNamespaces[qualified] = namespaceURI;
        projected.__glassSyncContent();
        if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        queueFrameCommand(currentBinding, {{
          kind: "setAttribute",
          node_index: nodeIndex,
          name: qualified,
          value: stringValue,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const frameNamespacedAttributeValue = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = attributeLocalName(name);
        for (const key of Object.keys(attributes)) {{
          if (attributeLocalName(key) === localName && frameAttributeNamespace(key) === namespaceURI) return attributes[key];
        }}
        return null;
      }};
      const removeFrameNamespacedAttribute = (namespace, name) => {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        const localName = attributeLocalName(name);
        const key = Object.keys(attributes).find((candidate) =>
          attributeLocalName(candidate) === localName && frameAttributeNamespace(candidate) === namespaceURI);
        if (key === undefined) return;
        delete attributes[key];
        delete attributeNamespaces[key];
        projected.__glassSyncContent();
        if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        queueFrameCommand(currentBinding, {{
          kind: "removeAttribute",
          node_index: nodeIndex,
          name: key,
          namespace_uri: namespaceURI === null ? "" : namespaceURI,
        }});
      }};
      const projected = {{
        nodeIndex,
        parentIndex: null,
        tagName: namespaceURI === HTML_NAMESPACE ? normalized.toUpperCase() : normalized,
        nodeType: 1,
        nodeName: namespaceURI === HTML_NAMESPACE ? normalized.toUpperCase() : normalized,
        localName: normalized,
        namespaceURI,
        id: "",
        className: "",
        value: "",
        checked: false,
        selected: false,
        disabled,
        hidden,
        multiple,
        getAttribute(name) {{
          const key = String(name).toLowerCase();
          for (const attribute of Object.keys(attributes)) {{
            if (attribute.toLowerCase() === key) return attributes[attribute];
          }}
          return null;
        }},
        getAttributeNS(namespace, name) {{ return frameNamespacedAttributeValue(namespace, name); }},
        getAttributeNames() {{ return Object.keys(attributes); }},
        hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
        matches(selector) {{ return projectedFrameMatches(projected, selector); }},
        closest(selector) {{
          let current = projected;
          while (current) {{
            if (projectedFrameMatches(current, selector)) return current;
            current = current.parentElement;
          }}
          return null;
        }},
        addEventListener(type, callback, options) {{
          addListener(ownerFor(projected), type, callback, options);
        }},
        removeEventListener(type, callback, options) {{
          removeListener(ownerFor(projected), type, callback, options);
        }},
        dispatchEvent(event) {{
          return dispatchTarget(this, event);
        }},
        setAttribute(name, nextValue) {{
          const key = String(name).toLowerCase();
          const stringValue = String(nextValue);
          attributes[key] = stringValue;
          delete attributeNamespaces[key];
          if (key === "disabled") disabled = true;
          if (key === "hidden") hidden = true;
          if (key === "multiple") multiple = true;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "setAttribute", node_index: nodeIndex, name: key, value: stringValue, namespace_uri: "" }});
        }},
        setAttributeNS(namespace, qualifiedName, nextValue) {{
          setFrameNamespacedAttribute(namespace, qualifiedName, nextValue);
        }},
        removeAttribute(name) {{
          const key = String(name).toLowerCase();
          delete attributes[key];
          delete attributeNamespaces[key];
          if (key === "disabled") disabled = false;
          if (key === "hidden") hidden = false;
          if (key === "multiple") multiple = false;
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "removeAttribute", node_index: nodeIndex, name: key, namespace_uri: "" }});
        }},
        removeAttributeNS(namespace, name) {{
          removeFrameNamespacedAttribute(namespace, name);
        }},
        appendChild(child) {{
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.appendChild(fragmentChild);
            child.__glassChildren = [];
            return child;
          }}
          if (!child || typeof child.nodeIndex !== "number") throw new TypeError("child must be a native node");
          if (child === projected) throw new TypeError("a node cannot contain itself");
          let ancestor = projected;
          while (ancestor) {{
            if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
            ancestor = ancestor.__glassParent || null;
          }}
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          projected.__glassChildren.push(child);
          child.__glassParent = projected;
          child.parentIndex = nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "appendChild", parent_index: nodeIndex, child_index: child.nodeIndex }});
          return child;
        }},
        insertBefore(child, before) {{
          if (child === this) throw new TypeError("a node cannot contain itself");
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) projected.insertBefore(fragmentChild, before);
            child.__glassChildren = [];
            return child;
          }}
          if (before == null) return this.appendChild(child);
          if (!child || typeof child.nodeIndex !== "number"
              || typeof before.nodeIndex !== "number") throw new TypeError("insertBefore requires native nodes");
          if (before.__glassParent !== projected) throw new TypeError("reference node is not a child");
          if (child === before) return child;
          let ancestor = projected;
          while (ancestor) {{
            if (ancestor === child) throw new TypeError("a node cannot contain one of its ancestors");
            ancestor = ancestor.__glassParent || null;
          }}
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          projected.__glassChildren = projected.__glassChildren.filter(candidate => candidate !== child);
          const index = projected.__glassChildren.indexOf(before);
          projected.__glassChildren.splice(index < 0 ? projected.__glassChildren.length : index, 0, child);
          child.__glassParent = projected;
          child.parentIndex = nodeIndex;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          if (projected.__glassAttached) registerFrameSubtree(child);
          else detachFrameSubtree(child);
          projected.__glassSyncContent();
          queueFrameCommand(currentBinding, {{ kind: "insertBefore", parent_index: nodeIndex, child_index: child.nodeIndex, before_index: before.nodeIndex }});
          return child;
        }},
        remove() {{
          const parent = projected.__glassParent || null;
          if (!parent && projected.parentIndex === null) return;
          const commitRemoval = !projected.__glassCreated || projected.parentIndex !== null;
          queueFragmentChildRemoval(parent, projected);
          if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== projected);
          projected.__glassParent = null;
          projected.parentIndex = null;
          detachFrameSubtree(projected);
          if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
          if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
        }},
        removeChild(child) {{
          if (!child || child.__glassParent !== projected) throw new TypeError("child is not contained by this element");
          child.remove();
          return child;
        }},
      }};
      installCommonAttributeProperties(projected, {{
        disabled: () => disabled,
        hidden: () => hidden,
        multiple: () => multiple,
      }}, String(currentBinding.url));
      Object.defineProperty(projected, "__glassChildren", {{ enumerable: false, configurable: false, writable: true, value: [] }});
      Object.defineProperty(projected, "__glassEventOwner", {{
        enumerable: false,
        configurable: false,
        value: "frame:" + currentFrameId + ":node:" + nodeIndex,
      }});
      Object.defineProperty(projected, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
      Object.defineProperty(projected, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(projected, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(projected, "__glassSyncContent", {{
        enumerable: false,
        configurable: false,
        value(clearEmpty = false) {{
          if (projected.__glassChildren.length > 0) {{
            innerHtml = projected.__glassChildren.map(child => child.__glassMarkup).join("");
            textContent = projected.__glassChildren.map(child => child.__glassTextValue).join("");
          }} else if (clearEmpty) {{
            innerHtml = "";
            textContent = "";
          }}
          if (projected.__glassParent && typeof projected.__glassParent.__glassSyncContent === "function") projected.__glassParent.__glassSyncContent(clearEmpty);
        }},
      }});
      Object.defineProperty(projected, "__glassRefresh", {{
        enumerable: false,
        configurable: false,
        value(nextEntry) {{
          nodeIndex = Number(nextEntry.nodeIndex);
          projected.nodeIndex = nodeIndex;
          projected.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
          projected.tagName = tagNameForEntry(nextEntry);
          projected.nodeName = tagNameForEntry(nextEntry);
          projected.localName = String(nextEntry.tagName || "").toLowerCase();
          for (const name of Object.keys(attributes)) delete attributes[name];
          const nextAttributes = nextEntry.attributes && typeof nextEntry.attributes === "object"
            ? nextEntry.attributes
            : {{}};
          for (const [name, nextValue] of Object.entries(nextAttributes)) attributes[name] = String(nextValue);
          for (const name of Object.keys(attributeNamespaces)) delete attributeNamespaces[name];
          const nextAttributeNamespaces = nextEntry.attributeNamespaces && typeof nextEntry.attributeNamespaces === "object"
            ? nextEntry.attributeNamespaces
            : {{}};
          for (const [name, namespace] of Object.entries(nextAttributeNamespaces)) attributeNamespaces[name] = String(namespace);
          textContent = String(nextEntry.text || "");
          innerHtml = String(nextEntry.innerHtml || "");
          value = nextEntry.value == null ? "" : nextEntry.value;
          checked = Boolean(nextEntry.checked);
          selected = Boolean(nextEntry.selected);
          disabled = Boolean(nextEntry.disabled);
          hidden = Boolean(nextEntry.hidden);
          multiple = Object.prototype.hasOwnProperty.call(attributes, "multiple");
          projected.__glassAttached = true;
          if (typeof projected.__glassSyncAttributeNodes === "function") projected.__glassSyncAttributeNodes();
        }},
      }});
      Object.defineProperty(projected, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return textContent; }} }});
      Object.defineProperty(projected, "__glassMarkup", {{
        enumerable: false,
        configurable: false,
        get() {{
          const markup = Object.keys(attributes).sort().map(name => " " + name + "=\"" + escapeHtmlText(attributes[name]).replace(/\"/g, "&quot;") + "\"").join("");
          const opening = "<" + normalized + markup + ">";
          if (["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"].includes(normalized)) return opening;
          const content = projected.__glassChildren.length > 0 ? projected.__glassChildren.map(child => child.__glassMarkup).join("") : innerHtml;
          return opening + content + "</" + normalized + ">";
        }},
      }});
      Object.defineProperty(projected, "outerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return projected.__glassMarkup; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element outerHTML exceeds its limit");
          const owner = projected.__glassParent || null;
          if (!owner || typeof owner.insertBefore !== "function") throw new TypeError("outerHTML requires an attached element");
          const fragment = makeFrameDocumentFragment();
          populateDetachedFragment(fragment, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment);
          for (const child of fragment.__glassChildren.slice()) owner.insertBefore(child, projected);
          projected.remove();
        }},
      }});
      Object.defineProperty(projected, "parentElement", {{ enumerable: false, configurable: false, get() {{ return projected.__glassParent && projected.__glassParent.nodeType === 1 ? projected.__glassParent : null; }} }});
      Object.defineProperty(projected, "parentNode", {{ enumerable: false, configurable: false, get() {{ return projected.__glassParent || projected.parentElement; }} }});
      for (const property of ["textContent", "innerText"]) {{
        Object.defineProperty(projected, property, {{
          enumerable: true,
          configurable: false,
          get() {{ return textContent; }},
          set(next) {{
            const value = String(next);
            if (value.length > storageValueLimit) throw new RangeError("native frame element textContent exceeds its limit");
            for (const child of projected.__glassChildren) {{ child.__glassParent = null; child.parentIndex = null; detachFrameSubtree(child); }}
            projected.__glassChildren = [];
            innerHtml = "";
            textContent = "";
            suppressHostCommands += 1;
            try {{
              if (value) projected.appendChild(makeFrameDetachedText(value));
            }} finally {{
              suppressHostCommands -= 1;
            }}
            projected.__glassSyncContent(true);
            queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: nodeIndex, value }});
          }},
        }});
      }}
      Object.defineProperty(projected, "innerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return innerHtml; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame element innerHTML exceeds its limit");
          for (const child of projected.__glassChildren) {{ child.__glassParent = null; child.parentIndex = null; detachFrameSubtree(child); }}
          projected.__glassChildren = [];
          innerHtml = "";
          textContent = "";
          suppressHostCommands += 1;
          try {{
            populateDetachedFragment(projected, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment);
          }} finally {{
            suppressHostCommands -= 1;
          }}
          projected.__glassSyncContent(true);
          queueFrameCommand(currentBinding, {{ kind: "setInnerHtml", node_index: nodeIndex, value }});
        }},
      }});
      Object.defineProperty(projected, "value", {{ enumerable: true, configurable: false, get() {{ return value; }}, set(next) {{ value = String(next); queueFrameCommand(currentBinding, {{ kind: "setValue", node_index: nodeIndex, value }}); }} }});
      Object.defineProperty(projected, "checked", {{ enumerable: true, configurable: false, get() {{ return checked; }}, set(next) {{ checked = Boolean(next); queueFrameCommand(currentBinding, {{ kind: "setChecked", node_index: nodeIndex, checked }}); }} }});
      Object.defineProperty(projected, "selected", {{ enumerable: true, configurable: false, get() {{ return selected; }}, set(next) {{ selected = Boolean(next); queueFrameCommand(currentBinding, {{ kind: "setSelected", node_index: nodeIndex, selected }}); }} }});
      Object.defineProperty(projected, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameDocumentCache.get(currentFrameId)?.document || frameDocument; }},
      }});
      Object.defineProperty(projected, "__glassAttributeSource", {{
        enumerable: false,
        configurable: false,
        value: () => attributes,
      }});
      Object.defineProperty(projected, "__glassAttributeNamespace", {{
        enumerable: false,
        configurable: false,
        value: (name) => frameAttributeNamespace(name),
      }});
      installAttributeNodeSurface(projected, () => frameDocumentCache.get(currentFrameId)?.document || frameDocument);
      try {{ Object.setPrototypeOf(projected, elementPrototypeFor(projected.tagName)); }} catch (_error) {{}}
      defineTreeAccessors(projected);
      installClassList(projected);
      installElementStyleAndDataset(projected);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), projected);
      frameMutationAttributes.set(frameMutationKey(currentBinding, nodeIndex), {{}});
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, projected);
      queueFrameCommand(currentBinding, {{
        kind: "createElement",
        node_index: nodeIndex,
        tag_name: normalized,
        namespace_uri: namespaceURI === null ? "" : namespaceURI,
      }});
      return projected;
    }};
    const makeFrameDetachedText = (value) => {{
      let textContent = String(value);
      if (textContent.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit");
      let nodeIndex = allocateTemporaryNodeIndex();
      const text = {{ nodeIndex, parentIndex: null, nodeType: 3, nodeName: "#text", nodeValue: textContent }};
      Object.defineProperty(text, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
      Object.defineProperty(text, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(text, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(text, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return textContent; }} }});
      Object.defineProperty(text, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return escapeHtmlText(textContent); }} }});
      Object.defineProperty(text, "parentElement", {{ enumerable: false, configurable: false, get() {{
        let current = text.__glassParent || null;
        while (current && current.nodeType !== 1) current = current.__glassParent || null;
        return current || null;
      }} }});
      Object.defineProperty(text, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{
          if (text.__glassParent) return text.__glassParent;
          const owner = text.ownerDocument;
          return owner && Array.isArray(owner.__glassChildren) && owner.__glassChildren.includes(text)
            ? owner
            : null;
        }},
      }});
      Object.defineProperty(text, "nodeValue", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ text.textContent = next; }} }});
      Object.defineProperty(text, "textContent", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ const value = String(next); if (value.length > {storage_value_limit}) throw new RangeError("native text node exceeds its limit"); textContent = value; if (text.__glassParent && typeof text.__glassParent.__glassSyncContent === "function") text.__glassParent.__glassSyncContent(); queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: nodeIndex, value: textContent }}); }} }});
      Object.defineProperty(text, "data", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ text.textContent = next; }} }});
      Object.defineProperty(text, "__glassRefresh", {{
        enumerable: false,
        configurable: false,
        value(nextEntry) {{
          nodeIndex = Number(nextEntry.nodeIndex);
          text.nodeIndex = nodeIndex;
          text.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
          textContent = String(nextEntry.nodeValue || "");
          text.__glassAttached = true;
        }},
      }});
      Object.defineProperty(text, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameDocumentCache.get(currentFrameId)?.document || frameDocument; }},
      }});
      text.remove = () => {{
        const parent = text.__glassParent || null;
        if (!parent && text.parentIndex === null) return;
        const commitRemoval = !text.__glassCreated || text.parentIndex !== null;
        queueFragmentChildRemoval(parent, text);
        if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== text);
        text.__glassParent = null;
        text.parentIndex = null;
        detachFrameSubtree(text);
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
      }};
      defineTreeAccessors(text);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), text);
      frameMutationText.set(frameMutationKey(currentBinding, nodeIndex), textContent);
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, text);
      queueFrameCommand(currentBinding, {{ kind: "createTextNode", node_index: nodeIndex, value: textContent }});
      try {{ Object.setPrototypeOf(text, TextNative.prototype); }} catch (_error) {{}}
      return text;
    }};
    const makeFrameDetachedComment = (value) => {{
      let textContent = String(value);
      if (textContent.length > {storage_value_limit}) throw new RangeError("native frame comment node exceeds its limit");
      let nodeIndex = allocateTemporaryNodeIndex();
      const comment = {{ nodeIndex, parentIndex: null, nodeType: 8, nodeName: "#comment", nodeValue: textContent }};
      Object.defineProperty(comment, "__glassParent", {{ enumerable: false, configurable: false, writable: true, value: null }});
      Object.defineProperty(comment, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(comment, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(comment, "__glassTextValue", {{ enumerable: false, configurable: false, get() {{ return ""; }} }});
      Object.defineProperty(comment, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{ return "<!--" + textContent + "-->"; }} }});
      Object.defineProperty(comment, "parentElement", {{ enumerable: false, configurable: false, get() {{
        let current = comment.__glassParent || null;
        while (current && current.nodeType !== 1) current = current.__glassParent || null;
        return current || null;
      }} }});
      Object.defineProperty(comment, "parentNode", {{ enumerable: false, configurable: false, get() {{ return comment.__glassParent || null; }} }});
      Object.defineProperty(comment, "nodeValue", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ comment.textContent = next; }} }});
      Object.defineProperty(comment, "textContent", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{
        const nextValue = String(next);
        if (nextValue.length > {storage_value_limit}) throw new RangeError("native frame comment node exceeds its limit");
        textContent = nextValue;
        if (comment.__glassParent && typeof comment.__glassParent.__glassSyncContent === "function") comment.__glassParent.__glassSyncContent();
        queueFrameCommand(currentBinding, {{ kind: "setTextContent", node_index: nodeIndex, value: nextValue }});
      }} }});
      Object.defineProperty(comment, "data", {{ enumerable: true, configurable: false, get() {{ return textContent; }}, set(next) {{ comment.textContent = next; }} }});
      Object.defineProperty(comment, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return frameDocumentCache.get(currentFrameId)?.document || frameDocument; }} }});
      Object.defineProperty(comment, "__glassRefresh", {{ enumerable: false, configurable: false, value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        comment.nodeIndex = nodeIndex;
        comment.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        textContent = String(nextEntry.nodeValue || "");
      }} }});
      comment.remove = () => {{
        const parent = comment.__glassParent || null;
        if (!parent && comment.parentIndex === null) return;
        const commitRemoval = !comment.__glassCreated || comment.parentIndex !== null;
        queueFragmentChildRemoval(parent, comment);
        if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== comment);
        comment.__glassParent = null;
        comment.parentIndex = null;
        detachFrameSubtree(comment);
        if (parent && typeof parent.__glassSyncContent === "function") parent.__glassSyncContent(true);
        if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
      }};
      defineTreeAccessors(comment);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), comment);
      frameMutationText.set(frameMutationKey(currentBinding, nodeIndex), textContent);
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, comment);
      queueFrameCommand(currentBinding, {{ kind: "createComment", node_index: nodeIndex, value: textContent }});
      try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
      return comment;
    }};
    const makeFrameDetachedDocumentType = (name, publicId = "", systemId = "") => {{
      let normalizedName = String(name);
      if (!/^[A-Za-z][A-Za-z0-9:_-]*$/.test(normalizedName)) throw new TypeError("invalid document type name");
      let publicIdentifier = String(publicId);
      let systemIdentifier = String(systemId);
      if (publicIdentifier.length > {storage_value_limit} || systemIdentifier.length > {storage_value_limit}) throw new RangeError("native frame document type identifier exceeds its limit");
      let nodeIndex = allocateTemporaryNodeIndex();
      const documentType = {{
        nodeIndex,
        parentIndex: null,
        nodeType: 10,
        nodeName: normalizedName,
        nodeValue: null,
        name: normalizedName,
        publicId: publicIdentifier,
        systemId: systemIdentifier,
        textContent: null,
        __glassChildren: [],
        __glassParent: null,
        remove() {{
          const parent = documentType.__glassParent || null;
          if (!parent && documentType.parentIndex === null) return;
          const commitRemoval = !documentType.__glassCreated || documentType.parentIndex !== null;
          if (parent && Array.isArray(parent.__glassChildren)) parent.__glassChildren = parent.__glassChildren.filter(candidate => candidate !== documentType);
          documentType.__glassParent = null;
          documentType.parentIndex = null;
          if (commitRemoval) queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: nodeIndex }});
        }},
      }};
      Object.defineProperty(documentType, "__glassCreated", {{ enumerable: false, configurable: false, writable: true, value: true }});
      Object.defineProperty(documentType, "__glassAttached", {{ enumerable: false, configurable: false, writable: true, value: false }});
      Object.defineProperty(documentType, "__glassMarkup", {{ enumerable: false, configurable: false, get() {{
        const publicPart = publicIdentifier ? " PUBLIC \"" + publicIdentifier + "\"" : "";
        const systemPart = systemIdentifier ? (publicPart ? " \"" + systemIdentifier + "\"" : " SYSTEM \"" + systemIdentifier + "\"") : "";
        return "<!DOCTYPE " + normalizedName + publicPart + systemPart + ">";
      }} }});
      Object.defineProperty(documentType, "__glassRefresh", {{ enumerable: false, configurable: false, value(nextEntry) {{
        nodeIndex = Number(nextEntry.nodeIndex);
        documentType.nodeIndex = nodeIndex;
        documentType.parentIndex = nextEntry.parentIndex == null ? null : nextEntry.parentIndex;
        normalizedName = String(nextEntry.nodeName || normalizedName);
        publicIdentifier = nextEntry.publicId == null ? "" : String(nextEntry.publicId);
        systemIdentifier = nextEntry.systemId == null ? "" : String(nextEntry.systemId);
        documentType.name = normalizedName;
        documentType.nodeName = normalizedName;
        documentType.publicId = publicIdentifier;
        documentType.systemId = systemIdentifier;
      }} }});
      Object.defineProperty(documentType, "parentNode", {{ enumerable: false, configurable: false, get() {{ return documentType.__glassParent || null; }} }});
      Object.defineProperty(documentType, "parentElement", {{ enumerable: false, configurable: false, get() {{ return null; }} }});
      Object.defineProperty(documentType, "ownerDocument", {{ enumerable: false, configurable: false, get() {{ return frameDocumentCache.get(currentFrameId)?.document || frameDocument; }} }});
      defineTreeAccessors(documentType);
      frameMutationNodes.set(frameMutationKey(currentBinding, nodeIndex), documentType);
      frameMutationChildren.set(frameMutationKey(currentBinding, nodeIndex), []);
      frameScriptNodeObjects.set(nodeIndex, documentType);
      queueFrameCommand(currentBinding, {{ kind: "createDocumentType", node_index: nodeIndex, name: normalizedName, public_id: publicIdentifier, system_id: systemIdentifier }});
      try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
      return documentType;
    }};
    const makeFrameDocumentFragment = () => {{
      const fragment = {{
        nodeType: 11,
        nodeName: "#document-fragment",
        __glassFragment: true,
        __glassChildren: [],
        __glassParent: null,
        appendChild(child) {{
          if (child === this) throw new TypeError("a node cannot contain itself");
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) this.appendChild(fragmentChild);
            child.__glassChildren = [];
            return child;
          }}
          if (!child || ![1, 3, 8].includes(Number(child.nodeType)))
            throw new TypeError("DocumentFragment children must be elements or text nodes");
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
          this.__glassChildren.push(child);
          child.__glassParent = this;
          child.parentIndex = null;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling: this.__glassChildren.length > 1 ? this.__glassChildren[this.__glassChildren.length - 2] : null, nextSibling: null }});
          detachFrameSubtree(child);
          return child;
        }},
        insertBefore(child, before) {{
          if (child === this) throw new TypeError("a node cannot contain itself");
          if (child && child.__glassFragment === true) {{
            const children = child.__glassChildren.slice();
            for (const fragmentChild of children) this.insertBefore(fragmentChild, before);
            child.__glassChildren = [];
            return child;
          }}
          if (before == null) return this.appendChild(child);
          if (!child || ![1, 3, 8].includes(Number(child.nodeType)))
            throw new TypeError("DocumentFragment children must be elements or text nodes");
          if (before.__glassParent !== this) throw new TypeError("reference node is not a child");
          const oldParent = child.__glassParent || null;
          if (oldParent && Array.isArray(oldParent.__glassChildren)) {{
            queueFragmentChildRemoval(oldParent, child);
            oldParent.__glassChildren = oldParent.__glassChildren.filter(candidate => candidate !== child);
            if (typeof oldParent.__glassSyncContent === "function") oldParent.__glassSyncContent(true);
          }}
          const oldParentIndex = child.parentIndex;
          this.__glassChildren = this.__glassChildren.filter(candidate => candidate !== child);
          const index = this.__glassChildren.indexOf(before);
          const previousSibling = index > 0 ? this.__glassChildren[index - 1] : null;
          const nextSibling = before;
          this.__glassChildren.splice(index < 0 ? this.__glassChildren.length : index, 0, child);
          child.__glassParent = this;
          child.parentIndex = null;
          if (oldParent && oldParent.nodeType === 1 && oldParentIndex !== null && oldParentIndex !== undefined) {{
            queueFrameCommand(currentBinding, {{ kind: "removeNode", node_index: child.nodeIndex }});
          }}
          queueMutation({{ type: "childList", target: this, addedNodes: [child], removedNodes: [], previousSibling, nextSibling }});
          detachFrameSubtree(child);
          return child;
        }},
      }};
      Object.defineProperty(fragment, "__glassTextValue", {{
        enumerable: false,
        configurable: false,
        get() {{ return fragment.__glassChildren.map(child => child.__glassTextValue || "").join(""); }},
      }});
      Object.defineProperty(fragment, "__glassMarkup", {{
        enumerable: false,
        configurable: false,
        get() {{ return fragment.__glassChildren.map(child => child.__glassMarkup || "").join(""); }},
      }});
      Object.defineProperty(fragment, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return frameDocument; }},
      }});
      Object.defineProperty(fragment, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{ return null; }},
      }});
      Object.defineProperty(fragment, "parentElement", {{
        enumerable: false,
        configurable: false,
        get() {{ return null; }},
      }});
      Object.defineProperty(fragment, "textContent", {{
        enumerable: true,
        configurable: false,
        get() {{ return fragment.__glassTextValue; }},
        set(next) {{
          for (const child of fragment.__glassChildren.slice()) child.remove();
          const value = String(next);
          if (value) fragment.appendChild(fragment.ownerDocument.createTextNode(value));
          }},
        }});
      Object.defineProperty(fragment, "innerHTML", {{
        enumerable: true,
        configurable: false,
        get() {{ return fragment.__glassMarkup; }},
        set(next) {{
          const value = String(next);
          if (value.length > storageValueLimit) throw new RangeError("native frame fragment innerHTML exceeds its limit");
          for (const child of fragment.__glassChildren.slice()) child.remove();
            populateDetachedFragment(fragment, value, makeFrameDetachedElement, makeFrameDetachedText, makeFrameDetachedComment);
        }},
      }});
      defineTreeAccessors(fragment);
      const constructor = globalThis.DocumentFragment;
      if (typeof constructor === "function" && constructor.prototype) {{
        try {{ Object.setPrototypeOf(fragment, constructor.prototype); }} catch (_error) {{}}
      }}
      return fragment;
    }};
    const find = (selector) => frameElements.filter((element) => element.__glassAttached && projectedFrameMatches(element, selector));
    const findById = (id) => frameElements.find((element) => element.__glassAttached && element.id === String(id)) || null;
    const body = frameElements.find((element) => element.tagName === "BODY") || null;
    const documentElement = frameElements.find((element) => element.tagName === "HTML") || null;
    const head = frameElements.find((element) => element.tagName === "HEAD") || null;
    const frameRootSnapshot = frameNodeSnapshots.find((entry) => entry && Number(entry.nodeIndex) === 0);
    const frameRootChildren = frameRootSnapshot && Array.isArray(frameRootSnapshot.children)
      ? frameRootSnapshot.children.map((index) => frameNodesByIndex.get(index)).filter(Boolean)
      : frameElements.filter((element) => element.parentIndex === null);
    frameDocument = {{
      nodeType: 9,
      nodeName: "#document",
      URL: String(currentBinding.url),
      documentURI: String(currentBinding.url),
      get title() {{
        const titleElement = frameElements.find((element) => element.__glassAttached && element.tagName === "TITLE");
        return titleElement ? titleElement.textContent : frameDocumentTitle;
      }},
      set title(value) {{
        const text = String(value);
        if (text.length > storageValueLimit) throw new RangeError("native frame document.title exceeds its limit");
        frameDocumentTitle = text;
        const titleElement = frameElements.find((element) => element.__glassAttached && element.tagName === "TITLE");
        if (titleElement) {{
          titleElement.textContent = text;
          return;
        }}
        let parent = frameDocument.head;
        if (!parent && documentElement) {{
          parent = makeFrameDetachedElement("head");
          documentElement.insertBefore(parent, documentElement.firstChild);
        }}
        if (parent) {{
          const created = makeFrameDetachedElement("title");
          created.textContent = text;
          parent.appendChild(created);
          return;
        }}
        queueFrameCommand(currentBinding, {{ kind: "setDocumentTitle", value: text }});
      }},
      textContent: String(snapshot.visibleText || ""),
      body,
      documentElement,
      get head() {{ return frameElements.find((element) => element.__glassAttached && element.tagName === "HEAD") || head; }},
      get doctype() {{ return frameDocument.__glassChildren.find((node) => Number(node.nodeType) === 10) || null; }},
      get forms() {{ return liveCollection(frameDocument, (element) => element.tagName === "FORM", "HTMLCollection", true); }},
      get links() {{ return liveCollection(frameDocument, (element) => ["A", "AREA"].includes(element.tagName) && element.getAttribute("href") !== null, "HTMLCollection", true); }},
      get scripts() {{ return liveCollection(frameDocument, (element) => element.tagName === "SCRIPT", "HTMLCollection", true); }},
      get images() {{ return liveCollection(frameDocument, (element) => element.tagName === "IMG", "HTMLCollection", true); }},
      get scrollingElement() {{ return documentElement; }},
      get defaultView() {{
        const current = frameBindingForId(currentFrameId) || currentBinding;
        return currentFrameWindow(current, ownerFrameElement, parentWindow, topWindow);
      }},
      createElement(tagName) {{ return makeFrameDetachedElement(tagName, HTML_NAMESPACE); }},
      createElementNS(namespace, qualifiedName) {{
        return makeFrameDetachedElement(qualifiedName, normalizeElementNamespace(namespace));
      }},
      createAttribute(name) {{
        return makeAttributeNode(name, "", () => frameDocumentCache.get(currentFrameId)?.document || frameDocument);
      }},
      createAttributeNS(namespace, qualifiedName) {{
        const namespaceURI = normalizeAttributeNamespace(namespace);
        return makeAttributeNode(qualifiedName, "", () => frameDocumentCache.get(currentFrameId)?.document || frameDocument, namespaceURI);
      }},
      createTextNode(value) {{ return makeFrameDetachedText(value); }},
      createComment(value) {{ return makeFrameDetachedComment(value); }},
      appendChild(child) {{
        if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
        if (frameDocument.__glassChildren.includes(child)) return child;
        if (child.__glassParent && child.__glassParent !== frameDocument) throw new DOMExceptionNative("The document type has another parent", "HierarchyRequestError");
        const existing = frameDocument.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
        if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
        frameDocument.__glassChildren = frameDocument.__glassChildren.filter((candidate) => candidate !== child);
        frameDocument.__glassChildren.push(child);
        child.__glassParent = frameDocument;
        child.parentIndex = 0;
        child.__glassAttached = true;
        queueFrameCommand(currentBinding, {{ kind: "appendChild", parent_index: 0, child_index: child.nodeIndex }});
        return child;
      }},
      insertBefore(child, before) {{
        if (before == null) return frameDocument.appendChild(child);
        if (!before || !frameDocument.__glassChildren.includes(before)) throw new TypeError("reference node is not a document child");
        if (!child || Number(child.nodeType) !== 10) throw new TypeError("Document children must be a document type");
        if (child === before) return child;
        const existing = frameDocument.__glassChildren.find((candidate) => Number(candidate.nodeType) === 10);
        if (existing && existing !== child) throw new DOMExceptionNative("The document already has a document type", "HierarchyRequestError");
        frameDocument.__glassChildren = frameDocument.__glassChildren.filter((candidate) => candidate !== child);
        const index = frameDocument.__glassChildren.indexOf(before);
        frameDocument.__glassChildren.splice(index < 0 ? frameDocument.__glassChildren.length : index, 0, child);
        child.__glassParent = frameDocument;
        child.parentIndex = 0;
        child.__glassAttached = true;
        queueFrameCommand(currentBinding, {{ kind: "insertBefore", parent_index: 0, child_index: child.nodeIndex, before_index: before.nodeIndex }});
        return child;
      }},
      implementation: {{
        createDocumentType(name, publicId = "", systemId = "") {{
          return makeFrameDetachedDocumentType(name, publicId, systemId);
        }},
      }},
      createDocumentFragment() {{ return makeFrameDocumentFragment(); }},
      getElementById: findById,
      querySelector(selector) {{ return find(selector)[0] || null; }},
      querySelectorAll(selector) {{ return asNodeList(find(selector)); }},
      getElementsByTagName(name) {{
        const value = String(name).toLowerCase();
        return liveCollection(frameDocument, (element) => value === "*" || element.tagName.toLowerCase() === value, "HTMLCollection", true);
      }},
      getElementsByClassName(name) {{
        const value = String(name);
        return liveCollection(frameDocument, (element) => element.className.split(/\s+/).includes(value), "HTMLCollection", true);
      }},
      getElementsByName(name) {{
        const value = String(name);
        return asNodeList(frameElements.filter((element) => element.__glassAttached
          && element.getAttribute("name") === value));
      }},
      addEventListener(type, callback, options) {{
        addListener(ownerFor(frameDocument), type, callback, options);
      }},
      removeEventListener(type, callback, options) {{
        removeListener(ownerFor(frameDocument), type, callback, options);
      }},
      dispatchEvent(event) {{
        return dispatchTarget(this, event);
      }},
    }};
    Object.defineProperty(frameDocument, "__glassChildren", {{
        enumerable: false,
        configurable: false,
        writable: true,
        value: frameRootChildren,
    }});
    for (const child of frameRootChildren) {{
      Object.defineProperty(child, "__glassMutationDocument", {{
        enumerable: false,
        configurable: true,
        writable: true,
        value: frameDocument,
      }});
    }}
    const frameDocumentKey = frameMutationKey(currentBinding, 0);
    frameMutationNodes.set(frameDocumentKey, frameDocument);
    frameMutationChildren.set(
      frameDocumentKey,
      frameRootChildren.map((child) => Number(child.nodeIndex)),
    );
    for (const child of frameRootChildren) {{
      frameMutationParents.set(frameMutationKey(currentBinding, child.nodeIndex), 0);
    }}
    Object.defineProperty(frameDocument, "__glassEventOwner", {{
      enumerable: false,
      configurable: false,
      value: "frame:" + currentFrameId + ":document",
    }});
    Object.defineProperty(frameDocument, "__glassEventTargetForNode", {{
      enumerable: false,
      configurable: false,
      value(nodeIndex) {{
        const index = Number(nodeIndex);
        if (index === 0) return frameDocument;
        if (index === 4294967295) return frameDocument.defaultView;
        const node = frameNodesByIndex.get(index);
        return node && node.__glassAttached ? node : null;
      }},
    }});
    defineTreeAccessors(frameDocument);
    try {{ Object.setPrototypeOf(frameDocument, DocumentNative.prototype); }} catch (_error) {{}}
    frameDocumentCache.set(currentFrameId, {{
      url: currentBinding.url,
      revision: currentBinding.revision,
      topologyKey,
      document: frameDocument,
    }});
    return frameDocument;
  }};
  globalThis.__glassDispatchFrameEvents = (frameId, events) => {{
    const identifier = String(frameId || "");
    const binding = frameBindingForId(identifier);
    if (!binding || binding.sameOrigin !== true) throw crossOriginSecurityError("event");
    if (!Array.isArray(events)) throw new TypeError("native frame events must be an array");
    if (events.length > {max_commands}) throw new RangeError("native frame event limit exceeded");
    const projectedDocument = makeFrameDocument(binding);
    const generation = Number(binding.generation);
    const delivered = [];
    for (const descriptor of events) {{
      if (!descriptor || typeof descriptor !== "object") throw new TypeError("native frame event is invalid");
      const nodeIndex = Number(descriptor.node_index);
      if (nodeIndex !== 4294967295 && Number(descriptor.generation) !== generation) continue;
      const target = projectedDocument.__glassEventTargetForNode(nodeIndex);
      if (!target) throw new TypeError("native frame event target is detached");
      const event = createEvent(descriptor.type, {{
        bubbles: Boolean(descriptor.bubbles),
        cancelable: Boolean(descriptor.cancelable),
      }});
      delivered.push(dispatchTarget(target, event));
    }}
    return delivered;
  }};
  const makeFrameWindow = (
    binding,
    frameElement,
    parentWindow = globalThis,
    topWindow = globalThis,
  ) => {{
    const currentBinding = frameBindingForId(frameIdentifier(binding)) || binding;
    const currentFrameId = frameIdentifier(currentBinding);
    const proxy = makeWindowProxy(
      "",
      "",
      currentFrameId,
      String(currentBinding.url),
      Boolean(currentBinding.sameOrigin),
    );
    const cacheKey = currentFrameId + "\\u0000";
    const state = windowProxyStates.get(cacheKey);
    if (state) {{
      state.targetLocationHref = String(currentBinding.url);
      state.closed = false;
      state.sameOrigin = Boolean(currentBinding.sameOrigin);
      if (frameElement !== null && frameElement !== undefined) state.frameElement = frameElement;
      state.parentWindow = parentWindow;
      state.topWindow = topWindow;
    }}
    if (!frameWindowCache.has(currentFrameId)) {{
      Object.defineProperty(proxy, "window", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "self", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "parent", {{
        enumerable: true,
        configurable: false,
        get: () => state.parentWindow || globalThis,
      }});
      Object.defineProperty(proxy, "top", {{
        enumerable: true,
        configurable: false,
        get: () => state.topWindow || globalThis,
      }});
      Object.defineProperty(proxy, "frameElement", {{
        enumerable: true,
        configurable: false,
        get: () => !state.sameOrigin
          ? null
          : state.frameElement || (globalThis.__glassHostElements instanceof Map
          ? globalThis.__glassHostElements.get(currentBinding.nodeIndex) || null
          : null),
      }});
      Object.defineProperty(proxy, "document", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = frameBindingForId(currentFrameId) || currentBinding;
          if (!state.sameOrigin || !current.sameOrigin) throw crossOriginSecurityError("document");
          return makeFrameDocument(
            current,
            state.frameElement || null,
            state.parentWindow || globalThis,
            state.topWindow || globalThis,
          );
        }},
      }});
      Object.defineProperty(proxy, "frames", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "length", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = frameBindingForId(currentFrameId) || currentBinding;
          return state.sameOrigin && current.sameOrigin ? frameChildBindings(current).length : 0;
        }},
      }});
      for (let index = 0; index < {max_frame_window_indices}; index += 1) {{
        Object.defineProperty(proxy, String(index), {{
          enumerable: false,
          configurable: false,
          get() {{
            const current = frameBindingForId(currentFrameId) || currentBinding;
            if (!state.sameOrigin || !current.sameOrigin) return undefined;
            const child = frameChildBindings(current)[index];
            return child
              ? makeFrameWindow(child, null, proxy, state.topWindow || globalThis)
              : undefined;
          }},
        }});
      }}
      frameWindowCache.set(currentFrameId, proxy);
    }}
    return frameWindowCache.get(currentFrameId);
  }};
  const relationshipWindowCache = globalThis.__glassRelationshipWindowCache instanceof Map
    ? globalThis.__glassRelationshipWindowCache
    : new Map();
  globalThis.__glassRelationshipWindowCache = relationshipWindowCache;
  const snapshotElementFromEntry = (entry) => {{
    if (!entry || typeof entry !== "object") return null;
    const attributes = entry.attributes && typeof entry.attributes === "object" ? entry.attributes : {{}};
    const projected = {{
      nodeIndex: entry.nodeIndex,
      parentIndex: entry.parentIndex == null ? null : entry.parentIndex,
      tagName: tagNameForEntry(entry),
      nodeType: 1,
      nodeName: tagNameForEntry(entry),
      localName: String(entry.tagName || "").toLowerCase(),
      namespaceURI: namespaceUriForEntry(entry),
      id: attributes.id || "",
      className: attributes.class || "",
      textContent: String(entry.text || ""),
      innerText: String(entry.text || ""),
      value: entry.value == null ? "" : entry.value,
      checked: Boolean(entry.checked),
      selected: Boolean(entry.selected),
      disabled: Boolean(entry.disabled),
      hidden: Boolean(entry.hidden),
      getAttribute(name) {{
        const key = String(name).toLowerCase();
        for (const attribute of Object.keys(attributes)) {{
          if (attribute.toLowerCase() === key) return attributes[attribute];
        }}
        return null;
      }},
      hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
    }};
    Object.defineProperty(projected, "ownerDocument", {{
      enumerable: false,
      configurable: false,
      get: () => null,
    }});
    try {{ Object.setPrototypeOf(projected, elementPrototypeFor(projected.tagName)); }} catch (_error) {{}}
    return projected;
  }};
  const makeRelationshipWindow = (descriptor, parentWindow = null, topWindow = null) => {{
    if (!descriptor || typeof descriptor !== "object" || !descriptor.contextId) return null;
    const contextId = String(descriptor.contextId);
    const proxy = makeWindowProxy(
      "",
      "",
      contextId,
      String(descriptor.url || "about:blank"),
      Boolean(descriptor.sameOrigin),
    );
    const cacheKey = contextId + "\\u0000";
    const state = windowProxyStates.get(cacheKey);
    if (state) {{
      state.targetLocationHref = String(descriptor.url || "about:blank");
      state.closed = false;
      state.sameOrigin = Boolean(descriptor.sameOrigin);
      state.relationshipDescriptor = descriptor;
      state.relationshipParent = parentWindow || proxy;
      state.relationshipTop = topWindow || proxy;
    }}
    if (!relationshipWindowCache.has(contextId)) {{
      Object.defineProperty(proxy, "window", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "self", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "parent", {{
        enumerable: true,
        configurable: false,
        get: () => state.relationshipParent || proxy,
      }});
      Object.defineProperty(proxy, "top", {{
        enumerable: true,
        configurable: false,
        get: () => state.relationshipTop || proxy,
      }});
      Object.defineProperty(proxy, "frameElement", {{
        enumerable: true,
        configurable: false,
        value: null,
      }});
      Object.defineProperty(proxy, "document", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = state.relationshipDescriptor;
          if (!state.sameOrigin || !current || !current.sameOrigin) throw crossOriginSecurityError("document");
          return makeFrameDocument(
            current,
            null,
            state.relationshipParent || proxy,
            state.relationshipTop || proxy,
          );
        }},
      }});
      Object.defineProperty(proxy, "frames", {{ enumerable: true, configurable: false, get: () => proxy }});
      Object.defineProperty(proxy, "length", {{
        enumerable: true,
        configurable: false,
        get() {{
          const current = state.relationshipDescriptor;
          return state.sameOrigin && current && current.sameOrigin
            ? frameChildBindings(current).length
            : 0;
        }},
      }});
      for (let index = 0; index < {max_frame_window_indices}; index += 1) {{
        Object.defineProperty(proxy, String(index), {{
          enumerable: false,
          configurable: false,
          get() {{
            const current = state.relationshipDescriptor;
            if (!state.sameOrigin || !current || !current.sameOrigin) return undefined;
            const child = frameChildBindings(current)[index];
            return child
              ? makeFrameWindow(child, null, proxy, state.relationshipTop || proxy)
              : undefined;
          }},
        }});
      }}
      relationshipWindowCache.set(contextId, proxy);
      frameWindowCache.set(contextId, proxy);
    }}
    return relationshipWindowCache.get(contextId);
  }};
  const frameBindings = Array.isArray(host.frames) ? host.frames : [];
  globalThis.__glassFrameBindings = frameBindings;
  globalThis.length = frameBindings.length;
  for (let index = 0; index < frameBindings.length; index += 1) {{
    try {{
      Object.defineProperty(globalThis, String(index), {{
        enumerable: false,
        configurable: true,
        get() {{
          return makeFrameWindow(
            frameBindings[index], null, globalThis, relationshipTop || globalThis
          );
        }},
      }});
    }} catch (_error) {{}}
  }};
  const frameContext = host.frame_context && typeof host.frame_context === "object"
    ? host.frame_context
    : null;
  let relationshipParent = null;
  let relationshipTop = null;
  let selectedFrameElement = null;
  if (frameContext) {{
    frameWindowCache.set(String(frameContext.currentFrameId), globalThis);
    const topDescriptor = frameContext.top && typeof frameContext.top === "object"
      ? frameContext.top
      : null;
    relationshipTop = makeRelationshipWindow(topDescriptor);
    if (frameContext.parent && typeof frameContext.parent === "object") {{
      relationshipParent = topDescriptor
          && String(frameContext.parent.contextId || "") === String(topDescriptor.contextId || "")
        ? relationshipTop
        : makeRelationshipWindow(frameContext.parent, relationshipTop, relationshipTop);
    }}
    selectedFrameElement = frameContext.parent
      && frameContext.parent.sameOrigin === true
      ? snapshotElementFromEntry(frameContext.frameElement)
      : null;
  }}
  const EventNative = globalThis.__glassEventConstructor || function Event(type, options) {{
    return globalThis.__glassCreateEvent(type, options);
  }};
  const CustomEventNative = globalThis.__glassCustomEventConstructor || function CustomEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    event.detail = options && typeof options === "object" ? options.detail : undefined;
    try {{ Object.setPrototypeOf(event, CustomEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const StorageEventNative = globalThis.__glassStorageEventConstructor || function StorageEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    event.key = options && options.key !== undefined ? options.key : null;
    event.oldValue = options && options.oldValue !== undefined ? options.oldValue : null;
    event.newValue = options && options.newValue !== undefined ? options.newValue : null;
    event.url = options && options.url !== undefined ? String(options.url) : "";
    event.storageArea = options && options.storageArea !== undefined ? options.storageArea : null;
    try {{ Object.setPrototypeOf(event, StorageEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const ErrorEventNative = globalThis.__glassErrorEventConstructor || function ErrorEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    const settings = options && typeof options === "object" ? options : {{}};
    event.message = settings.message === undefined ? "" : String(settings.message);
    event.filename = settings.filename === undefined ? "" : String(settings.filename);
    event.lineno = Number(settings.lineno) || 0;
    event.colno = Number(settings.colno) || 0;
    event.error = settings.error === undefined ? null : settings.error;
    try {{ Object.setPrototypeOf(event, ErrorEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const PromiseRejectionEventNative = globalThis.__glassPromiseRejectionEventConstructor || function PromiseRejectionEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    const settings = options && typeof options === "object" ? options : {{}};
    event.promise = settings.promise === undefined ? null : settings.promise;
    event.reason = settings.reason === undefined ? null : settings.reason;
    try {{ Object.setPrototypeOf(event, PromiseRejectionEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  globalThis.__glassEventConstructor = EventNative;
  globalThis.__glassCustomEventConstructor = CustomEventNative;
  globalThis.__glassStorageEventConstructor = StorageEventNative;
  globalThis.__glassErrorEventConstructor = ErrorEventNative;
  globalThis.__glassPromiseRejectionEventConstructor = PromiseRejectionEventNative;
  globalThis.__glassCreateEvent = createEvent;
  globalThis.Event = EventNative;
  globalThis.CustomEvent = CustomEventNative;
  globalThis.StorageEvent = StorageEventNative;
  globalThis.ErrorEvent = ErrorEventNative;
  globalThis.PromiseRejectionEvent = PromiseRejectionEventNative;
  try {{ Object.setPrototypeOf(CustomEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(StorageEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(ErrorEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(PromiseRejectionEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(document, DocumentNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(location, LocationNative.prototype); }} catch (_error) {{}}
  for (const element of elements) {{
    try {{ Object.setPrototypeOf(element, elementPrototypeFor(element.tagName)); }} catch (_error) {{}}
    if (!Object.prototype.hasOwnProperty.call(element, "ownerDocument")) {{
      Object.defineProperty(element, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return globalThis.document || null; }},
      }});
    }}
  }}
  for (const text of textNodes) {{
    try {{ Object.setPrototypeOf(text, TextNative.prototype); }} catch (_error) {{}}
  }}
  for (const comment of commentNodes) {{
    try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
  }}
  for (const documentType of documentTypeNodes) {{
    try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
  }}
  try {{ Object.setPrototypeOf(globalThis, WindowNative.prototype); }} catch (_error) {{}}
  globalThis.self = globalThis;
  globalThis.top = relationshipTop || globalThis;
  globalThis.parent = relationshipParent || globalThis;
  globalThis.frames = globalThis;
  globalThis.length = frameBindings.length;
  Object.defineProperty(globalThis, "frameElement", {{
    configurable: true,
    enumerable: true,
    value: selectedFrameElement,
  }});
  for (const proxy of windowProxyCache.values()) {{
    try {{ Object.setPrototypeOf(proxy, WindowNative.prototype); }} catch (_error) {{}}
  }}
  globalThis.addEventListener = (type, callback, options) => addListener("window", type, callback, options);
  globalThis.removeEventListener = (type, callback, options) => removeListener("window", type, callback, options);
  globalThis.dispatchEvent = (event) => dispatchTarget(globalThis, event);
  if (!Object.prototype.hasOwnProperty.call(globalThis, "onerror")) installEventHandlerProperty(globalThis, "error");
  if (!Object.prototype.hasOwnProperty.call(globalThis, "onunhandledrejection")) installEventHandlerProperty(globalThis, "unhandledrejection");
  if (!Object.prototype.hasOwnProperty.call(globalThis, "onrejectionhandled")) installEventHandlerProperty(globalThis, "rejectionhandled");
  const makeStorageEvent = (descriptor) => {{
    const event = createEvent("storage");
    event.key = descriptor.key === null ? null : String(descriptor.key);
    event.oldValue = descriptor.old_value === null ? null : String(descriptor.old_value);
    event.newValue = descriptor.new_value === null ? null : String(descriptor.new_value);
    event.url = String(descriptor.url || "");
    event.storageArea = descriptor.scope === "session"
      ? globalThis.sessionStorage
      : globalThis.localStorage;
    return event;
  }};
  globalThis.__glassDispatchStorageEvents = (events) => events.map((descriptor) => {{
    const values = descriptor.scope === "session"
      ? globalThis.__glassSessionStorageValues
      : globalThis.__glassLocalStorageValues;
    if (descriptor.key === null) values.clear();
    else if (descriptor.new_value === null) values.delete(String(descriptor.key));
    else values.set(String(descriptor.key), String(descriptor.new_value));
    return dispatchTarget(globalThis, makeStorageEvent(descriptor));
  }});
  globalThis.console = globalThis.console || {{
    log() {{}}, info() {{}}, warn() {{}}, error() {{}}
  }};
  globalThis.__glassQueueResizeObserverChanges = queueResizeObserverChanges;
  globalThis.__glassQueueIntersectionObserverChanges = queueIntersectionObserverChanges;
  if (Array.isArray(host.storage_events) && host.storage_events.length > 0) {{
    globalThis.__glassDispatchStorageEvents(host.storage_events);
  }}
  if ({run_timers}) {{
    globalThis.__glassRunTimers(host.now_ms);
  }}
}})();"###,
        serialized = serialized,
        max_commands = super::interaction::MAX_NATIVE_EFFECTS,
        max_listeners = super::interaction::MAX_NATIVE_EFFECTS,
        max_timers = super::interaction::MAX_NATIVE_EFFECTS,
        storage_entry_limit = crate::browser_backend::MAX_STORAGE_ENTRIES,
        storage_key_limit = crate::browser_backend::MAX_BACKEND_ID_BYTES,
        storage_value_limit = crate::browser_backend::MAX_TEXT_BYTES,
        indexed_db_database_limit = MAX_NATIVE_INDEXED_DB_DATABASES,
        indexed_db_store_limit = MAX_NATIVE_INDEXED_DB_STORES,
        indexed_db_index_limit = MAX_NATIVE_INDEXED_DB_INDEXES,
        indexed_db_record_limit = MAX_NATIVE_INDEXED_DB_RECORDS,
        indexed_db_value_limit = MAX_NATIVE_INDEXED_DB_VALUE_BYTES,
        storage_quota = MAX_WEB_STORAGE_PROFILE_BYTES,
        fetch_header_count_limit = MAX_NATIVE_FETCH_HEADERS,
        fetch_header_name_limit = MAX_NATIVE_FETCH_HEADER_NAME_BYTES,
        fetch_header_value_limit = MAX_NATIVE_FETCH_HEADER_VALUE_BYTES,
        fetch_header_bytes_limit = MAX_NATIVE_FETCH_HEADER_BYTES,
        max_native_xhr_timeout_ms = MAX_NATIVE_XHR_TIMEOUT_MS,
        websocket_message_limit = MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
        websocket_protocol_limit = MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES,
        websocket_protocol_count_limit = MAX_NATIVE_WEBSOCKET_PROTOCOLS,
        websocket_close_reason_limit = MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
        eventsource_message_limit = MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES,
        eventsource_field_limit = MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
        fetch_stream_chunk_limit = MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES,
        fetch_stream_body_limit = MAX_NATIVE_FETCH_STREAM_BODY_BYTES,
        fetch_stream_queue_limit = MAX_NATIVE_FETCH_STREAM_QUEUED_CHUNKS,
        dialog_text_limit = MAX_NATIVE_DIALOG_TEXT_BYTES,
        post_message_bytes_limit = MAX_NATIVE_POST_MESSAGE_BYTES,
        max_frame_window_indices = MAX_NATIVE_FRAME_SCRIPT_BINDINGS,
        window_name_bytes_limit = MAX_NATIVE_WINDOW_NAME_BYTES,
        run_timers = run_timers,
        width = viewport.width,
        height = viewport.height,
        ready_state = ready_state,
        history_state_bytes_limit = MAX_NATIVE_HISTORY_STATE_BYTES,
        history_length_limit = MAX_NATIVE_HISTORY_DELTA,
    ))
}

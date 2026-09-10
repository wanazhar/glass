//! Bounded JavaScript execution for the native browser realm.
//!
//! QuickJS supplies the ECMAScript implementation. Glass owns the host
//! objects and Web APIs, which are added in separate slices so every exposed
//! capability has an explicit resource and security contract.

use super::browsing_context::NATIVE_CONTEXT_ID;
use super::config::{Viewport, validate_context_id};
use super::dom::{NativeDocument, NativePageScriptSource, NativePageScriptTiming};
use super::error::NativeEngineError;
use super::interaction::NativeEventKind;
use super::origin::NativeOrigin;
use fs2::FileExt;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Context, Error, Module, Runtime, Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
const MAX_NATIVE_INDEXED_DB_DATABASES: usize = 16;
const MAX_NATIVE_INDEXED_DB_STORES: usize = 128;
const MAX_NATIVE_INDEXED_DB_INDEXES: usize = 128;
const MAX_NATIVE_INDEXED_DB_RECORDS: usize = 128;
pub(crate) const MAX_NATIVE_INDEXED_DB_CHANGES: usize = 128;
const MAX_NATIVE_INDEXED_DB_VALUE_BYTES: usize = 8 * 1024;
const MAX_NATIVE_INDEXED_DB_STATE_BYTES: usize = MAX_NATIVE_SCRIPT_RESULT_BYTES;
const NATIVE_STORAGE_PROFILE_LOCK_TIMEOUT: Duration = Duration::from_millis(500);
const NATIVE_STORAGE_PROFILE_LOCK_RETRY: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Deserialize)]
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
    Fetch {
        request_id: u32,
        href: String,
        credentials: bool,
        method: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        content_type: Option<String>,
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
    },
    RemoveAttribute {
        node_index: u32,
        name: String,
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
}

pub(crate) struct NativeScriptEvaluation {
    pub(crate) value: serde_json::Value,
    pub(crate) commands: Vec<NativeScriptCommand>,
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
    Classic { source: String },
    Module { name: String, source: String },
    ModuleDependency { name: String, source: String },
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
    let mut parser_blocking = Vec::new();
    let mut asynchronous = Vec::new();
    let mut deferred = Vec::new();
    for (timing, source) in sources {
        match timing {
            NativePageScriptTiming::ParserBlocking => parser_blocking.push(source),
            NativePageScriptTiming::Async => asynchronous.push(source),
            NativePageScriptTiming::Defer => deferred.push(source),
        }
    }
    parser_blocking.extend(asynchronous);
    parser_blocking.extend(deferred);
    parser_blocking
}

/// Execute the bounded inline scripts discovered in one parsed document.
///
/// The caller owns the realm so local documents and the child content process
/// can both retain globals and listeners after the document commit. Script
/// navigation is intentionally rejected during parsing; navigation only has a
/// defined owner after the document has been committed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_inline_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    storage_state: &NativeWebStorageState,
    indexed_db_state: &NativeIndexedDbState,
    cookie: &str,
) -> Result<(), NativeEngineError> {
    let sources = document
        .page_script_sources(MAX_NATIVE_INLINE_SCRIPTS, MAX_NATIVE_SCRIPT_BYTES)
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| match source {
            NativePageScriptSource::Inline { source, timing } => {
                Some((timing, NativePageScript::Classic { source }))
            }
            NativePageScriptSource::ModuleInline { source, timing } => Some((
                timing,
                NativePageScript::Module {
                    name: format!("{document_url}#glass-inline-module-{index}"),
                    source,
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
        &sources,
        document_url,
        document_origin,
        viewport,
        storage_state,
        indexed_db_state,
        cookie,
        &[],
    )
    .map(|_| ())
}

pub(crate) fn execute_page_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    sources: &[NativePageScript],
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    storage_state: &NativeWebStorageState,
    indexed_db_state: &NativeIndexedDbState,
    cookie: &str,
    resource_load_nodes: &[u32],
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    if runtime.is_none() {
        *runtime = Some(NativeJavaScriptRuntime::new_with_context_id(
            NATIVE_CONTEXT_ID,
        )?);
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
            NativePageScript::Module { name, source }
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
    for source in sources {
        let evaluation = {
            let script_runtime = runtime.as_ref().expect("page script runtime initialized");
            match source {
                NativePageScript::Classic { source } => script_runtime.evaluate(
                    source,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                ),
                NativePageScript::Module { name, source } => script_runtime.evaluate_module(
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
            Err(error) if is_ignorable_page_script_error(&error) => continue,
            Err(error) => return Err(error),
        };
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
    }
    for node_index in resource_load_nodes {
        let Some(event_source) = host_event_script(&[(*node_index, NativeEventKind::Load)])? else {
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
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
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
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
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
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_timer_pump_enabled(true);
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .reset_timer_clock();
    Ok(pending_fetches)
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
) -> Result<(), NativeEngineError> {
    let mut commands = Vec::new();
    for command in evaluation.commands {
        if matches!(command, NativeScriptCommand::Fetch { .. }) {
            pending_fetches.push(command);
        } else {
            commands.push(command);
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
                NativeEventKind::Input => ("input", true, false),
                NativeEventKind::Change => ("change", true, false),
                NativeEventKind::Scroll => ("scroll", true, false),
            };
            serde_json::json!({
                "node_index": node_index,
                "type": event_type,
                "bubbles": bubbles,
                "cancelable": cancelable,
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
    storage_context_id: String,
    ready_state: String,
    clock_origin: Instant,
}

impl NativeJavaScriptRuntime {
    pub(crate) fn new_with_context_id(
        context_id: impl Into<String>,
    ) -> Result<Self, NativeEngineError> {
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
            storage_context_id: context_id.into(),
            ready_state: "complete".into(),
            clock_origin: Instant::now(),
        })
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

    fn now_ms(&self) -> u64 {
        self.clock_origin
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
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
        let bootstrap = document_bootstrap(
            document,
            document_url,
            origin,
            viewport,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
            &self.indexed_db_state(),
            &storage_events,
            &self.cookie_state(),
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
            let (value, async_evaluation): (Value, bool) = match ctx.eval::<Value, _>(source) {
                Ok(value) => (value, false),
                Err(_) if contains_await_token(source) => (
                    ctx.eval_promise(source)
                        .and_then(|promise| promise.finish::<Value>())
                        .map_err(|_| NativeEngineError::Worker {
                            operation: "evaluate JavaScript".into(),
                            reason: "JavaScript evaluation failed".into(),
                        })?,
                    true,
                ),
                Err(_) => {
                    return Err(NativeEngineError::Worker {
                        operation: "evaluate JavaScript".into(),
                        reason: "JavaScript evaluation failed".into(),
                    });
                }
            };
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
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
                document_commands.push(command);
            }
            let commands = document_commands;
            let indexed_db_state = read_indexed_db_state(ctx.clone())?;
            self.set_indexed_db_state(indexed_db_state);
            let json = ctx
                .json_stringify(value)
                .map_err(|_| NativeEngineError::Worker {
                    operation: "serialize JavaScript result".into(),
                    reason: "JavaScript result could not be serialized".into(),
                })?;
            let Some(json) = json else {
                return Ok(NativeScriptEvaluation {
                    value: serde_json::Value::Null,
                    commands,
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
        let bootstrap = document_bootstrap(
            document,
            document_url,
            origin,
            viewport,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
            &self.indexed_db_state(),
            &storage_events,
            &self.cookie_state(),
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
            Module::evaluate(ctx.clone(), name, source)
                .and_then(|promise| promise.finish::<()>())
                .map_err(|_| NativeEngineError::Worker {
                    operation: "evaluate JavaScript module".into(),
                    reason: "JavaScript module evaluation failed".into(),
                })?;
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
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
                document_commands.push(command);
            }
            let indexed_db_state = read_indexed_db_state(ctx.clone())?;
            self.set_indexed_db_state(indexed_db_state);
            Ok(NativeScriptEvaluation {
                value: serde_json::Value::Null,
                commands: document_commands,
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
    if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
        return Err(NativeEngineError::limit(
            "script host commands",
            MAX_NATIVE_SCRIPT_RESULT_BYTES,
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

fn document_bootstrap(
    document: &NativeDocument,
    document_url: &str,
    origin: &NativeOrigin,
    viewport: Viewport,
    ready_state: &str,
    now_ms: u64,
    storage: &NativeWebStorageView,
    indexed_db: &NativeIndexedDbOrigin,
    storage_events: &[NativeStorageEvent],
    cookie: &str,
    run_timers: bool,
) -> Result<String, NativeEngineError> {
    let state = document.script_snapshot(crate::browser_backend::MAX_TEXT_BYTES);
    let serialized = serde_json::to_string(&serde_json::json!({
        "url": document_url,
        "origin": origin.serialized(),
        "state": state,
        "now_ms": now_ms,
        "storage": storage,
        "indexed_db": indexed_db,
        "storage_events": storage_events,
        "cookie": cookie,
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
  const commands = [];
  const activeCommands = () => Array.isArray(globalThis.__glassHostCommandBuffer)
    ? globalThis.__glassHostCommandBuffer
    : commands;
  const pushCommand = (command) => {{
    const target = activeCommands();
    if (target.length >= {max_commands}) throw new RangeError("native host command limit exceeded");
    target.push(command);
  }};
  const timers = globalThis.__glassTimers instanceof Map
    ? globalThis.__glassTimers
    : new Map();
  const runningTimers = globalThis.__glassRunningTimers instanceof Map
    ? globalThis.__glassRunningTimers
    : new Map();
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
  const storageEntryLimit = {storage_entry_limit};
  const storageKeyLimit = {storage_key_limit};
  const storageValueLimit = {storage_value_limit};
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
  globalThis.__glassIndexedDbConnections = indexedDbConnections;
  globalThis.__glassIndexedDbPendingOpens = indexedDbPendingOpens;
  globalThis.__glassIndexedDbPendingDeletes = indexedDbPendingDeletes;
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
  const indexedDbClone = (value) => {{
    let encoded;
    try {{ encoded = JSON.stringify(value); }} catch (_error) {{
      throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    }}
    if (encoded === undefined) throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    if (encoded.length > indexedDbValueLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB value limit exceeded");
    return JSON.parse(encoded);
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
    const extracted = indexedDbReadKeyPath(value, index.key_path);
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
  const queueIndexedDbRequest = (transaction, operation) => {{
    if (transaction.__finished || transaction.__aborted) throw indexedDbError("TransactionInactiveError", "native IndexedDB transaction is inactive");
    const request = makeIndexedDbRequest();
    transaction.__pending += 1;
    indexedDbSchedule(() => {{
      let value;
      let error = null;
      if (transaction.__aborted) {{
        error = indexedDbError("AbortError", "native IndexedDB transaction was aborted");
      }} else {{
        try {{ value = operation(); }} catch (caught) {{
          error = caught instanceof Error ? caught : indexedDbError("UnknownError", String(caught));
          transaction.__aborted = true;
          transaction.error = error;
          transaction.__rollback();
          if (typeof transaction.onerror === "function") transaction.onerror.call(transaction, {{ target: transaction }});
        }}
      }}
      finishIndexedDbRequest(request, value, error, () => {{
        transaction.__pending -= 1;
        maybeFinishIndexedDbTransaction(transaction);
      }});
    }});
    return request;
  }};
  const cloneIndexedDbDatabaseState = (databaseState) => JSON.parse(JSON.stringify(databaseState));
  const restoreIndexedDbDatabaseState = (databaseState, snapshot) => {{
    for (const key of Object.keys(databaseState)) delete databaseState[key];
    for (const [key, value] of Object.entries(snapshot)) databaseState[key] = value;
  }};
  const makeIndexedDbTransaction = (database, databaseState, storeNames, mode, upgrade) => {{
    const initialState = cloneIndexedDbDatabaseState(databaseState);
    const transaction = {{
      db: database,
      mode,
      error: null,
      __storeNames: storeNames,
      __initialState: initialState,
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
      __rollback() {{
        if (this.__rolledBack) return;
        restoreIndexedDbDatabaseState(databaseState, this.__initialState);
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
        return entry ? indexedDbClone(entry.value) : undefined;
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
          return entry ? indexedDbClone(entry.value) : undefined;
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
        return queueIndexedDbRequest(transaction, () => queryEntries(range, "next").slice(0, limit).map(entry => indexedDbClone(entry.value)));
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
      return {{ token, value: cloned, key: indexedDbKeyValue(token) }};
    }};
    const resolveCursorRecord = (value, primaryToken) => {{
      const cloned = indexedDbClone(value);
      const token = store.key_path
        ? indexedDbKeyToken(indexedDbReadKeyPath(cloned, store.key_path), false)
        : primaryToken;
      if (token !== primaryToken) throw indexedDbError("DataError", "native IndexedDB cursor update cannot change the primary key");
      return {{ token, value: cloned, key: indexedDbKeyValue(token) }};
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
          return entry ? indexedDbClone(entry.value) : undefined;
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
          .map(entry => indexedDbClone(entry.value)));
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
  globalThis.setTimeout = setTimeoutNative;
  globalThis.setInterval = setIntervalNative;
  globalThis.clearTimeout = clearTimer;
  globalThis.clearInterval = clearTimer;
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
      if (type === "file") throw new TypeError("native FormData file controls are unsupported");
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
  const BlobNative = function(parts, options) {{
    this.__glassNativeBlob = true;
    this._text = boundedBlobText(parts);
    this.size = this._text.length;
    this.type = normalizeBlobType(options);
  }};
  BlobNative.prototype.text = function() {{
    return Promise.resolve(this._text);
  }};
  BlobNative.prototype.slice = function(start, end, contentType) {{
    const length = this._text.length;
    const normalizePosition = (value, fallback) => {{
      if (value === undefined) return fallback;
      const number = Number(value);
      if (!Number.isFinite(number)) return fallback;
      return number < 0 ? Math.max(length + Math.trunc(number), 0) : Math.min(Math.trunc(number), length);
    }};
    const begin = normalizePosition(start, 0);
    const finish = normalizePosition(end, length);
    return new BlobNative([begin > finish ? "" : this._text.slice(begin, finish)], {{ type: contentType }});
  }};
  const FileNative = function(parts, name, options) {{
    if (name === undefined) throw new TypeError("native File requires a name");
    BlobNative.call(this, parts, options);
    this.__glassNativeFile = true;
    this.name = String(name);
    const modified = options && Number.isFinite(Number(options.lastModified))
      ? Number(options.lastModified)
      : 0;
    this.lastModified = Math.max(0, modified);
  }};
  FileNative.prototype = Object.create(BlobNative.prototype);
  FileNative.prototype.constructor = FileNative;
  globalThis.Blob = BlobNative;
  globalThis.File = FileNative;
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
    return entry ? (entry[1].kind === "file" ? entry[1].value : entry[1]) : null;
  }};
  FormDataNative.prototype.getAll = function(name) {{
    const key = String(name);
    return this._entries
      .filter(entry => entry[0] === key)
      .map(entry => entry[1].kind === "file" ? entry[1].value : entry[1]);
  }};
  FormDataNative.prototype.has = function(name) {{
    const key = String(name);
    return this._entries.some(entry => entry[0] === key);
  }};
  FormDataNative.prototype.entries = function() {{
    return this._entries.map(entry => [
      entry[0],
      entry[1].kind === "file" ? entry[1].value : entry[1],
    ]);
  }};
  FormDataNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("FormData callback must be callable");
    this._entries.forEach(entry => callback.call(
      thisArg,
      entry[1].kind === "file" ? entry[1].value : entry[1],
      entry[0],
      this,
    ));
  }};
  const escapeFormDataName = value => String(value)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, "\\\"")
    .replace(/\r/g, "%0D")
    .replace(/\n/g, "%0A");
  const serializeFormData = (formData, requestId) => {{
    const boundary = "----GlassNativeForm" + requestId;
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
    return {{ body, contentType: "multipart/form-data; boundary=" + boundary }};
  }};
  globalThis.FormData = FormDataNative;
  const URLSearchParamsNative = function(init) {{
    this.__glassUrlSearchParams = true;
    this._entries = [];
    if (init === undefined || init === null) return;
    if (typeof init === "string") {{
      for (const part of init.split("&")) {{
        if (!part) continue;
        const pieces = part.split("=");
        const decode = value => decodeURIComponent(String(value).replace(/\+/g, " "));
        this._entries.push([decode(pieces.shift()), decode(pieces.join("="))]);
      }}
      return;
    }}
    if (init.__glassUrlSearchParams === true) {{
      this._entries = init._entries.map(entry => [entry[0], entry[1]]);
      return;
    }}
    throw new TypeError("native URLSearchParams accepts only text or URLSearchParams");
  }};
  URLSearchParamsNative.prototype.append = function(name, value) {{
    this._entries.push([String(name), String(value)]);
  }};
  URLSearchParamsNative.prototype.set = function(name, value) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
    this._entries.push([key, String(value)]);
  }};
  URLSearchParamsNative.prototype.delete = function(name) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
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
  URLSearchParamsNative.prototype.entries = function() {{ return this._entries.slice(); }};
  URLSearchParamsNative.prototype.toString = function() {{
    const encode = value => encodeURIComponent(String(value)).replace(/%20/g, "+");
    return this._entries.map(entry => encode(entry[0]) + "=" + encode(entry[1])).join("&");
  }};
  globalThis.URLSearchParams = URLSearchParamsNative;
  const fetchNative = (input, options) => {{
    if (typeof input !== "string") throw new TypeError("native fetch requires a URL string");
    const settings = options && typeof options === "object" ? options : {{}};
    const method = settings.method === undefined ? "GET" : String(settings.method).toUpperCase();
    const rawBody = settings.body === undefined || settings.body === null
      ? null
      : String(settings.body);
    const formData = settings.body && settings.body.__glassFormData === true
      ? settings.body
      : null;
    const urlSearchParams = settings.body && settings.body.__glassUrlSearchParams === true
      ? settings.body
      : null;
    let body = rawBody;
    if (method !== "GET" && method !== "POST") {{
      return Promise.reject(new TypeError("native fetch supports only GET and POST requests"));
    }}
    const headers = settings.headers && typeof settings.headers === "object"
      ? settings.headers
      : {{}};
    let contentType = null;
    for (const name of Object.keys(headers)) {{
      if (String(name).toLowerCase() !== "content-type") {{
        return Promise.reject(new TypeError("native fetch only supports the Content-Type header"));
      }}
      contentType = String(headers[name]);
    }}
    const requestId = nextFetchRequestId;
    if (formData) {{
      if (contentType !== null) return Promise.reject(new TypeError("FormData chooses its own Content-Type boundary"));
      const serialized = serializeFormData(formData, requestId);
      body = serialized.body;
      contentType = serialized.contentType;
    }}
    if (urlSearchParams) {{
      if (contentType !== null) return Promise.reject(new TypeError("URLSearchParams chooses its own Content-Type"));
      body = urlSearchParams.toString();
      contentType = "application/x-www-form-urlencoded;charset=UTF-8";
    }}
    if (method === "GET" && body !== null) {{
      return Promise.reject(new TypeError("GET fetch requests must not have a body"));
    }}
    nextFetchRequestId += 1;
    globalThis.__glassNextFetchRequestId = nextFetchRequestId;
    const credentials = settings.credentials !== "omit";
    return new Promise((resolve, reject) => {{
      fetchRequests.set(requestId, {{ resolve, reject }});
      pushCommand({{ kind: "fetch", request_id: requestId, href: input, credentials, method, body, content_type: contentType }});
    }});
  }};
  const responseFromFetch = (payload) => Object.freeze({{
    ok: payload.status >= 200 && payload.status < 300,
    status: payload.status,
    url: payload.url,
    headers: Object.freeze({{
      get(name) {{
        return String(name).toLowerCase() === "content-type" ? payload.contentType : null;
      }}
    }}),
    text() {{ return Promise.resolve(payload.body); }},
    json() {{ return Promise.resolve(JSON.parse(payload.body)); }},
  }});
  const XMLHttpRequestNative = function() {{
    this.readyState = 0;
    this.status = 0;
    this.statusText = "";
    this.responseText = "";
    this.responseURL = "";
    this.response = "";
    this.withCredentials = false;
    this.onreadystatechange = null;
    this.onload = null;
    this.onerror = null;
    this._method = "GET";
    this._url = "";
    this._headers = {{}};
    this._responseContentType = null;
  }};
  XMLHttpRequestNative.prototype._notifyReadyState = function() {{
    if (typeof this.onreadystatechange === "function") this.onreadystatechange.call(this);
  }};
  XMLHttpRequestNative.prototype.open = function(method, url, async) {{
    if (async === false) throw new TypeError("native XMLHttpRequest requires async mode");
    const normalizedMethod = String(method).toUpperCase();
    if (normalizedMethod !== "GET" && normalizedMethod !== "POST")
      throw new TypeError("native XMLHttpRequest supports only GET and POST");
    if (typeof url !== "string") throw new TypeError("native XMLHttpRequest URL must be text");
    this._method = normalizedMethod;
    this._url = url;
    this._headers = {{}};
    this.readyState = 1;
    this._notifyReadyState();
  }};
  XMLHttpRequestNative.prototype.setRequestHeader = function(name, value) {{
    if (String(name).toLowerCase() !== "content-type")
      throw new TypeError("native XMLHttpRequest only supports the Content-Type header");
    this._headers["Content-Type"] = String(value);
  }};
  XMLHttpRequestNative.prototype.getResponseHeader = function(name) {{
    return String(name).toLowerCase() === "content-type" ? this._responseContentType : null;
  }};
  XMLHttpRequestNative.prototype.getAllResponseHeaders = function() {{
    return this._responseContentType
      ? "content-type: " + this._responseContentType + "\r\n"
      : "";
  }};
  XMLHttpRequestNative.prototype.send = function(body) {{
    if (this.readyState !== 1) throw new TypeError("native XMLHttpRequest is not open");
    const requestBody = body && (body.__glassFormData === true || body.__glassUrlSearchParams === true)
      ? body
      : body === undefined || body === null ? null : String(body);
    const request = fetchNative(this._url, {{
      method: this._method,
      body: requestBody,
      headers: this._headers,
      credentials: this.withCredentials ? "include" : "omit",
    }});
    request.then(response => {{
      this.status = response.status;
      this.statusText = String(response.status);
      this.responseURL = response.url;
      this._responseContentType = response.headers.get("content-type");
      return response.text();
    }}).then(text => {{
      this.responseText = text;
      this.response = text;
      this.readyState = 4;
      this._notifyReadyState();
      if (typeof this.onload === "function") this.onload.call(this, {{ type: "load", target: this }});
    }}).catch(error => {{
      this.readyState = 4;
      this._notifyReadyState();
      if (typeof this.onerror === "function") this.onerror.call(this, {{ type: "error", target: this, error }});
    }});
  }};
  globalThis.XMLHttpRequest = XMLHttpRequestNative;
  globalThis.__glassFetchRequests = fetchRequests;
  globalThis.__glassNextFetchRequestId = nextFetchRequestId;
  globalThis.fetch = fetchNative;
  globalThis.__glassResolveFetch = (requestId, payload) => {{
    const pending = fetchRequests.get(Number(requestId));
    if (!pending) return;
    fetchRequests.delete(Number(requestId));
    if (payload && payload.error) pending.reject(new Error(String(payload.error)));
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
  const createEvent = (type, options) => {{
    const settings = options && typeof options === "object" ? options : {{}};
    const event = {{
      type: normalizeEventType(type),
      bubbles: Boolean(settings.bubbles),
      cancelable: Boolean(settings.cancelable),
      key: settings.key === undefined ? "" : String(settings.key),
      code: settings.code === undefined ? "" : String(settings.code),
      target: null,
      currentTarget: null,
      eventPhase: 0,
      defaultPrevented: false,
      returnValue: "",
      submitter: settings.submitter === undefined ? null : settings.submitter,
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
    return event;
  }};
  const ownerFor = (target) => {{
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
    if (target !== globalThis && target !== document && typeof target.nodeIndex === "number") {{
      let parent = target.parentElement;
      while (parent) {{
        path.push(parent);
        parent = parent.parentElement;
      }}
      path.push(document, globalThis);
    }} else if (target === document) {{
      path.push(globalThis);
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
  const makeElement = (initialEntry) => {{
    let entry = initialEntry;
    const element = {{
      nodeIndex: entry.nodeIndex,
      parentIndex: entry.parentIndex,
      formOwnerIndex: entry.formOwnerIndex,
      tagName: entry.tagName.toUpperCase(),
      id: entry.attributes.id || "",
      className: entry.attributes.class || "",
      textContent: entry.text,
      innerText: entry.text,
      value: entry.value === null
        ? (entry.tagName.toLowerCase() === "option"
          ? (entry.attributes.value === undefined ? entry.text : entry.attributes.value)
          : "")
        : entry.value,
      checked: entry.checked,
      selected: entry.selected,
      multiple: Object.prototype.hasOwnProperty.call(entry.attributes, "multiple"),
      disabled: entry.disabled,
      hidden: entry.hidden,
      focused: entry.focused,
      get validity() {{ return validityFlags(entry); }},
      get validationMessage() {{
        return validationMessageFor(entry, validityFlags(entry));
      }},
      get willValidate() {{ return Boolean(entry.willValidate); }},
      getAttribute(name) {{
        const key = String(name).toLowerCase();
        for (const attr of Object.keys(entry.attributes)) {{
          if (attr.toLowerCase() === key) return entry.attributes[attr];
        }}
        return null;
      }},
      hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
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
        if (key === "id") this.id = stringValue;
        if (key === "class") this.className = stringValue;
        if (key === "disabled") this.disabled = true;
        if (key === "hidden") this.hidden = true;
        pushCommand({{ kind: "setAttribute", node_index: entry.nodeIndex, name: key, value: stringValue }});
      }},
      removeAttribute(name) {{
        const key = String(name).toLowerCase();
        delete entry.attributes[key];
        if (key === "id") this.id = "";
        if (key === "class") this.className = "";
        if (key === "disabled") this.disabled = false;
        if (key === "hidden") this.hidden = false;
        pushCommand({{ kind: "removeAttribute", node_index: entry.nodeIndex, name: key }});
      }}
    }};
    let value = element.value;
    Object.defineProperty(element, "value", {{
      enumerable: true,
      configurable: false,
      get() {{ return value; }},
      set(next) {{
        value = String(next);
        pushCommand({{ kind: "setValue", node_index: entry.nodeIndex, value }});
      }}
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
    Object.defineProperty(element, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        entry = nextEntry;
        element.nodeIndex = nextEntry.nodeIndex;
        element.parentIndex = nextEntry.parentIndex;
        element.tagName = nextEntry.tagName.toUpperCase();
        element.id = nextEntry.attributes.id || "";
        element.className = nextEntry.attributes.class || "";
        element.textContent = nextEntry.text;
        element.innerText = nextEntry.text;
        element.disabled = nextEntry.disabled;
        element.hidden = nextEntry.hidden;
        element.focused = nextEntry.focused;
        element.multiple = Object.prototype.hasOwnProperty.call(nextEntry.attributes, "multiple");
        value = nextEntry.value === null
          ? (nextEntry.tagName.toLowerCase() === "option"
            ? (nextEntry.attributes.value === undefined ? nextEntry.text : nextEntry.attributes.value)
            : "")
          : nextEntry.value;
        checked = nextEntry.checked;
        selected = nextEntry.selected;
      }}
    }});
    return element;
  }};
  const previousElements = globalThis.__glassHostElements instanceof Map
    ? globalThis.__glassHostElements
    : new Map();
  const elements = state.elements.map((entry) => {{
    const existing = previousElements.get(entry.nodeIndex);
    if (existing && typeof existing.__glassRefresh === "function") {{
      existing.__glassRefresh(entry);
      return existing;
    }}
    return makeElement(entry);
  }});
  const elementsByIndex = new Map(elements.map((element) => [element.nodeIndex, element]));
  globalThis.__glassHostElements = elementsByIndex;
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
    if (element.tagName === "SELECT" && !Object.prototype.hasOwnProperty.call(element, "options")) {{
      Object.defineProperty(element, "options", {{
        enumerable: false,
        configurable: false,
        get() {{
          const current = globalThis.__glassHostElements;
          if (!(current instanceof Map)) return [];
          return Array.from(current.values()).filter(option =>
            option.tagName === "OPTION" && option.parentIndex === element.nodeIndex
          );
        }},
      }});
      Object.defineProperty(element, "selectedOptions", {{
        enumerable: false,
        configurable: false,
        get() {{ return element.options.filter(option => option.selected); }},
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
  const matches = (element, selector) => {{
    const value = String(selector).trim();
    if (value.startsWith("#")) return element.id === value.slice(1);
    if (value.startsWith(".")) return element.className.split(/\s+/).includes(value.slice(1));
    return element.tagName.toLowerCase() === value.toLowerCase();
  }};
  const findAll = (selector) => elements.filter((element) => matches(element, selector));
  const body = elements.find((element) => element.tagName === "BODY") || null;
  const documentElement = elements.find((element) => element.tagName === "HTML") || null;
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
  const document = {{
    title: state.title,
    body,
    documentElement,
    get activeElement() {{ return elements.find((element) => element.focused) || null; }},
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
    getElementById(id) {{ return elements.find((element) => element.id === String(id)) || null; }},
    querySelector(selector) {{ return findAll(selector)[0] || null; }},
    querySelectorAll(selector) {{ return findAll(selector); }},
    getElementsByTagName(name) {{
      const value = String(name).toLowerCase();
      return elements.filter((element) => value === "*" || element.tagName.toLowerCase() === value);
    }},
    getElementsByClassName(name) {{
      const value = String(name);
      return elements.filter((element) => element.className.split(/\s+/).includes(value));
    }}
  }};
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
      key: descriptor.key,
      code: descriptor.code,
      submitter: descriptor.submitter_node_index == null
        ? null
        : elements.find((element) => element.nodeIndex === descriptor.submitter_node_index) || null,
      oldURL: descriptor.old_url,
      newURL: descriptor.new_url,
    }});
    return dispatchTarget(target, event);
  }});
  globalThis.window = globalThis;
  globalThis.__glassHostCommands = commands;
  globalThis.__glassHostCommandBuffer = commands;
  globalThis.document = document;
  globalThis.location = Object.freeze({{ href: host.url, origin: host.origin }});
  globalThis.innerWidth = {width};
  globalThis.innerHeight = {height};
  globalThis.navigator = globalThis.navigator || Object.freeze({{ userAgent: "GlassNative" }});
  globalThis.Event = globalThis.Event || function Event(type, options) {{
    return createEvent(type, options);
  }};
  globalThis.CustomEvent = globalThis.CustomEvent || function CustomEvent(type, options) {{
    const event = createEvent(type, options);
    event.detail = options && typeof options === "object" ? options.detail : undefined;
    return event;
  }};
  globalThis.addEventListener = (type, callback, options) => addListener("window", type, callback, options);
  globalThis.removeEventListener = (type, callback, options) => removeListener("window", type, callback, options);
  globalThis.dispatchEvent = (event) => dispatchTarget(globalThis, event);
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
  globalThis.StorageEvent = globalThis.StorageEvent || function StorageEvent(type, options) {{
    const event = createEvent(type, options);
    event.key = options && options.key !== undefined ? options.key : null;
    event.oldValue = options && options.oldValue !== undefined ? options.oldValue : null;
    event.newValue = options && options.newValue !== undefined ? options.newValue : null;
    event.url = options && options.url !== undefined ? String(options.url) : "";
    event.storageArea = options && options.storageArea !== undefined ? options.storageArea : null;
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
        run_timers = run_timers,
        width = viewport.width,
        height = viewport.height,
        ready_state = ready_state,
    ))
}

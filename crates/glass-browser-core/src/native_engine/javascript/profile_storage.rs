use super::*;

pub(super) fn default_indexed_db_next_key() -> u64 {
    1
}

impl NativeIndexedDbState {
    pub(crate) fn validate(&self) -> Result<(), NativeEngineError> {
        if self.origins.len() > crate::limits::MAX_STORAGE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native IndexedDB origins",
                crate::limits::MAX_STORAGE_ENTRIES,
                self.origins.len(),
            ));
        }
        for (storage_key, origin) in &self.origins {
            if storage_key.is_empty() || storage_key.len() > crate::limits::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "native IndexedDB storage key",
                    crate::limits::MAX_TEXT_BYTES,
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
    pub(super) fn validate(&self) -> Result<(), NativeEngineError> {
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

pub(super) fn validate_indexed_db_store(
    store: &NativeIndexedDbObjectStore,
) -> Result<(), NativeEngineError> {
    if let Some(key_path) = store.key_path.as_deref()
        && key_path.len() > crate::limits::MAX_BACKEND_ID_BYTES
    {
        return Err(NativeEngineError::limit(
            "native IndexedDB key path",
            crate::limits::MAX_BACKEND_ID_BYTES,
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

pub(super) fn validate_indexed_db_index(
    index: &NativeIndexedDbIndex,
) -> Result<(), NativeEngineError> {
    if index.key_path.is_empty() {
        return Err(NativeEngineError::invalid(
            "native IndexedDB index key path",
            "must not be empty",
        ));
    }
    validate_indexed_db_key_path(Some(&index.key_path))
}

pub(super) fn validate_indexed_db_record_key(key: &str) -> Result<(), NativeEngineError> {
    if key.is_empty() || key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            "native IndexedDB record key",
            crate::limits::MAX_BACKEND_ID_BYTES,
            key.len(),
        ));
    }
    Ok(())
}

pub(super) fn validate_indexed_db_record_value(
    value: &serde_json::Value,
) -> Result<(), NativeEngineError> {
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

pub(super) fn validate_indexed_db_name(field: &str, value: &str) -> Result<(), NativeEngineError> {
    if value.is_empty() {
        return Err(NativeEngineError::invalid(field, "must not be empty"));
    }
    if value.len() > crate::limits::MAX_BACKEND_ID_BYTES {
        return Err(NativeEngineError::limit(
            field,
            crate::limits::MAX_BACKEND_ID_BYTES,
            value.len(),
        ));
    }
    Ok(())
}

pub(super) fn validate_indexed_db_serialized_size(
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

pub(super) fn push_indexed_db_change(
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

pub(super) fn validate_indexed_db_storage_key(storage_key: &str) -> Result<(), NativeEngineError> {
    if storage_key.is_empty() || storage_key.len() > crate::limits::MAX_TEXT_BYTES {
        return Err(NativeEngineError::limit(
            "native IndexedDB storage key",
            crate::limits::MAX_TEXT_BYTES,
            storage_key.len(),
        ));
    }
    Ok(())
}

pub(super) fn validate_indexed_db_change(
    change: &NativeIndexedDbChange,
) -> Result<(), NativeEngineError> {
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

pub(super) fn validate_indexed_db_key_path(
    key_path: Option<&str>,
) -> Result<(), NativeEngineError> {
    if let Some(key_path) = key_path
        && key_path.len() > crate::limits::MAX_BACKEND_ID_BYTES
    {
        return Err(NativeEngineError::limit(
            "native IndexedDB key path",
            crate::limits::MAX_BACKEND_ID_BYTES,
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
                if entries.len() >= crate::limits::MAX_STORAGE_ENTRIES && !entries.contains_key(key)
                {
                    return Err(NativeEngineError::limit(
                        "native Web Storage entries",
                        crate::limits::MAX_STORAGE_ENTRIES,
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
    #[serde(default)]
    service_worker_caches: NativeServiceWorkerCacheState,
    #[serde(default)]
    service_worker_registrations: Vec<NativeServiceWorkerRegistrationProfile>,
}

#[derive(Debug, Deserialize, Serialize)]
struct NativeCookieProfileFile {
    version: u64,
    cookies: Vec<NativeCookieProfileEntry>,
}

const NATIVE_COOKIE_PROFILE_FILE_VERSION: u64 = 1;

/// Profile-owned CacheStorage state. The map is partitioned by serialized
/// origin, then cache name, then a method/URL key. Keeping this state beside
/// the existing browser profile makes cache writes survive content-process
/// replacement without giving JavaScript direct access to profile files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeServiceWorkerCacheState {
    pub(crate) origins:
        BTreeMap<String, BTreeMap<String, BTreeMap<String, NativeServiceWorkerCacheEntry>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NativeServiceWorkerCacheBatchEntry {
    pub(crate) request_url: String,
    pub(crate) request_method: String,
    #[serde(default)]
    pub(crate) request_headers: Vec<(String, String)>,
    pub(crate) response: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeServiceWorkerCacheEntry {
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) request_url: String,
    #[serde(default)]
    pub(crate) request_headers: Vec<(String, String)>,
    pub(crate) url: String,
    pub(crate) status: u16,
    pub(crate) status_text: String,
    pub(crate) headers: Vec<(String, String)>,
    #[serde(default)]
    pub(crate) content_type: Option<String>,
    #[serde(default)]
    pub(crate) body_base64: String,
    #[serde(default)]
    pub(crate) body_null: bool,
    #[serde(default)]
    pub(crate) redirected: bool,
    #[serde(default)]
    pub(crate) opaque: bool,
    #[serde(default)]
    pub(crate) opaque_redirect: bool,
}

pub(super) fn native_http_header_name_is_valid(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            matches!(
                byte,
                b'0'..=b'9'
                    | b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'!'
                    | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            )
        })
}

pub(crate) fn validate_native_service_worker_cache_request_headers(
    headers: &[(String, String)],
) -> Result<(), NativeEngineError> {
    if headers.len() > MAX_NATIVE_FETCH_HEADERS {
        return Err(NativeEngineError::limit(
            "native service worker cache request headers",
            MAX_NATIVE_FETCH_HEADERS,
            headers.len(),
        ));
    }
    let mut total_bytes = 0usize;
    for (name, value) in headers {
        if name.len() > MAX_NATIVE_FETCH_HEADER_NAME_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker cache request header name",
                MAX_NATIVE_FETCH_HEADER_NAME_BYTES,
                name.len(),
            ));
        }
        if !native_http_header_name_is_valid(name) {
            return Err(NativeEngineError::invalid(
                "native service worker cache request header name",
                "must be a valid HTTP token",
            ));
        }
        if value.len() > MAX_NATIVE_FETCH_HEADER_VALUE_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker cache request header value",
                MAX_NATIVE_FETCH_HEADER_VALUE_BYTES,
                value.len(),
            ));
        }
        if value
            .bytes()
            .any(|byte| byte == 0 || byte < 0x20 && byte != b'\t' || byte == 0x7f)
        {
            return Err(NativeEngineError::invalid(
                "native service worker cache request header value",
                "must not contain control characters",
            ));
        }
        total_bytes = total_bytes
            .saturating_add(name.len())
            .saturating_add(value.len());
        if total_bytes > MAX_NATIVE_FETCH_HEADER_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker cache request headers",
                MAX_NATIVE_FETCH_HEADER_BYTES,
                total_bytes,
            ));
        }
    }
    Ok(())
}

impl NativeServiceWorkerCacheState {
    pub(crate) fn validate(&self) -> Result<(), NativeEngineError> {
        if self.origins.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_ORIGINS {
            return Err(NativeEngineError::limit(
                "native service worker cache origins",
                MAX_NATIVE_SERVICE_WORKER_CACHE_ORIGINS,
                self.origins.len(),
            ));
        }
        let mut total_bytes = 0usize;
        for (origin, caches) in &self.origins {
            if origin.is_empty() || origin.len() > MAX_NATIVE_SCRIPT_BYTES {
                return Err(NativeEngineError::limit(
                    "native service worker cache origin",
                    MAX_NATIVE_SCRIPT_BYTES,
                    origin.len(),
                ));
            }
            if caches.len() > MAX_NATIVE_SERVICE_WORKER_CACHES {
                return Err(NativeEngineError::limit(
                    "native service worker caches",
                    MAX_NATIVE_SERVICE_WORKER_CACHES,
                    caches.len(),
                ));
            }
            for (name, entries) in caches {
                if name.is_empty() || name.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES {
                    return Err(NativeEngineError::limit(
                        "native service worker cache name",
                        MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES,
                        name.len(),
                    ));
                }
                if entries.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES {
                    return Err(NativeEngineError::limit(
                        "native service worker cache entries",
                        MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES,
                        entries.len(),
                    ));
                }
                for (key, entry) in entries {
                    if key.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES {
                        return Err(NativeEngineError::limit(
                            "native service worker cache key",
                            MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES,
                            key.len(),
                        ));
                    }
                    if key != &format!("{}\n{}", entry.method, entry.request_url) {
                        return Err(NativeEngineError::invalid(
                            "native service worker cache key",
                            "does not match its request",
                        ));
                    }
                    if !matches!(entry.method.as_str(), "GET") {
                        return Err(NativeEngineError::invalid(
                            "native service worker cache request method",
                            "only GET cache entries are supported",
                        ));
                    }
                    if entry.request_url.is_empty()
                        || entry.request_url.len() > MAX_NATIVE_SCRIPT_BYTES
                    {
                        return Err(NativeEngineError::limit(
                            "native service worker cache request URL",
                            MAX_NATIVE_SCRIPT_BYTES,
                            entry.request_url.len(),
                        ));
                    }
                    if entry.url.is_empty() || entry.url.len() > MAX_NATIVE_SCRIPT_BYTES {
                        return Err(NativeEngineError::limit(
                            "native service worker cached response URL",
                            MAX_NATIVE_SCRIPT_BYTES,
                            entry.url.len(),
                        ));
                    }
                    if !(200..=599).contains(&entry.status) {
                        return Err(NativeEngineError::invalid(
                            "native service worker cached response status",
                            "must be between 200 and 599",
                        ));
                    }
                    validate_native_service_worker_cache_request_headers(&entry.request_headers)?;
                    if entry.headers.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_HEADERS {
                        return Err(NativeEngineError::limit(
                            "native service worker cached response headers",
                            MAX_NATIVE_SERVICE_WORKER_CACHE_HEADERS,
                            entry.headers.len(),
                        ));
                    }
                    for (header_name, header_value) in &entry.headers {
                        if header_name.is_empty()
                            || header_name.len() > MAX_NATIVE_FETCH_HEADER_NAME_BYTES
                            || header_value.len() > MAX_NATIVE_FETCH_HEADER_VALUE_BYTES
                        {
                            return Err(NativeEngineError::invalid(
                                "native service worker cached response headers",
                                "contain an invalid name or value",
                            ));
                        }
                    }
                    let body = base64::engine::general_purpose::STANDARD
                        .decode(&entry.body_base64)
                        .map_err(|_| {
                            NativeEngineError::invalid(
                                "native service worker cached response body",
                                "must be valid base64",
                            )
                        })?;
                    if body.len() > MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES {
                        return Err(NativeEngineError::limit(
                            "native service worker cached response body",
                            MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES,
                            body.len(),
                        ));
                    }
                    if entry.body_null && !body.is_empty() {
                        return Err(NativeEngineError::invalid(
                            "native service worker cached response body",
                            "a null body must not contain bytes",
                        ));
                    }
                    total_bytes = total_bytes.saturating_add(body.len());
                    if total_bytes > MAX_NATIVE_SERVICE_WORKER_CACHE_TOTAL_BYTES {
                        return Err(NativeEngineError::limit(
                            "native service worker cache data",
                            MAX_NATIVE_SERVICE_WORKER_CACHE_TOTAL_BYTES,
                            total_bytes,
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

pub(super) fn validate_service_worker_registration_profiles(
    registrations: &[NativeServiceWorkerRegistrationProfile],
) -> Result<(), NativeEngineError> {
    if registrations.len() > MAX_NATIVE_SERVICE_WORKERS {
        return Err(NativeEngineError::limit(
            "native service worker registrations",
            MAX_NATIVE_SERVICE_WORKERS,
            registrations.len(),
        ));
    }
    let mut scopes = BTreeSet::new();
    for registration in registrations {
        registration.validate()?;
        if !scopes.insert(&registration.scope) {
            return Err(NativeEngineError::invalid(
                "native service worker registration scope",
                "must be unique in the profile",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NativeCookieProfileEntry {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub host_only: bool,
    pub secure: bool,
    pub http_only: bool,
    #[serde(default)]
    pub same_site: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub expires_at_unix_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct NativeCookieChange {
    pub name: String,
    pub domain: String,
    pub path: String,
    pub cookie: Option<NativeCookieProfileEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NativeStorageJournalRecord {
    pub(crate) writer_id: String,
    #[serde(default)]
    pub(crate) event: Option<NativeStorageEvent>,
    #[serde(default)]
    pub(crate) indexed_db_changes: Vec<NativeIndexedDbChange>,
    #[serde(default)]
    pub(crate) cookie_changes: Vec<NativeCookieChange>,
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
static NATIVE_PROFILE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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

pub(super) fn profile_lock_is_contended(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::WouldBlock || (cfg!(windows) && error.raw_os_error() == Some(33))
}

pub(super) fn storage_event_journal_path(path: &Path) -> PathBuf {
    path.with_extension("events")
}

pub(super) fn storage_reader_lease_path(path: &Path) -> PathBuf {
    path.with_extension("readers")
}

pub(super) fn service_worker_client_lease_path(path: &Path) -> PathBuf {
    path.with_extension("clients")
}

pub(super) fn read_service_worker_client_leases(
    path: &Path,
) -> Result<Vec<NativeServiceWorkerClientLease>, NativeEngineError> {
    let lease_path = service_worker_client_lease_path(path);
    let metadata = match fs::metadata(&lease_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "read native Service Worker client leases".into(),
                reason: "native Service Worker client lease metadata is unavailable".into(),
            });
        }
    };
    let lease_bytes = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if lease_bytes > MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASE_BYTES {
        return Err(NativeEngineError::limit(
            "native Service Worker client leases",
            MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASE_BYTES,
            lease_bytes,
        ));
    }
    let bytes = fs::read(&lease_path).map_err(|_| NativeEngineError::Worker {
        operation: "read native Service Worker client leases".into(),
        reason: "native Service Worker client leases cannot be read".into(),
    })?;
    let file: NativeServiceWorkerClientLeaseFile =
        serde_json::from_slice(&bytes).map_err(|_| {
            NativeEngineError::invalid(
                "native Service Worker client leases",
                "must contain a valid client lease file",
            )
        })?;
    if file.version != NATIVE_SERVICE_WORKER_CLIENT_LEASE_VERSION {
        return Err(NativeEngineError::invalid(
            "native Service Worker client lease version",
            "is unsupported",
        ));
    }
    if file.clients.len() > MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES {
        return Err(NativeEngineError::limit(
            "native Service Worker client leases",
            MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES,
            file.clients.len(),
        ));
    }
    let mut ids = BTreeSet::new();
    for lease in &file.clients {
        validate_context_id(&lease.owner_id)?;
        validate_service_worker_client_states(std::slice::from_ref(&lease.state))?;
        if !ids.insert(lease.state.id.clone()) {
            return Err(NativeEngineError::invalid(
                "native Service Worker client leases",
                "must not contain duplicate client IDs",
            ));
        }
    }
    Ok(file.clients)
}

pub(super) fn write_service_worker_client_leases(
    path: &Path,
    clients: &[NativeServiceWorkerClientLease],
) -> Result<(), NativeEngineError> {
    let lease_path = service_worker_client_lease_path(path);
    if clients.is_empty() {
        match fs::remove_file(&lease_path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => {
                return Err(NativeEngineError::Worker {
                    operation: "remove native Service Worker client leases".into(),
                    reason: "native Service Worker client leases cannot be removed".into(),
                });
            }
        }
        return Ok(());
    }
    let file = NativeServiceWorkerClientLeaseFile {
        version: NATIVE_SERVICE_WORKER_CLIENT_LEASE_VERSION,
        clients: clients.to_vec(),
    };
    let bytes = serde_json::to_vec(&file).map_err(|_| NativeEngineError::Worker {
        operation: "encode native Service Worker client leases".into(),
        reason: "native Service Worker client leases cannot be encoded".into(),
    })?;
    if bytes.len() > MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASE_BYTES {
        return Err(NativeEngineError::limit(
            "native Service Worker client leases",
            MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASE_BYTES,
            bytes.len(),
        ));
    }
    let temporary_path = lease_path.with_extension(format!(
        "clients-tmp-{}-{}",
        std::process::id(),
        NATIVE_STORAGE_WRITER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut temporary = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)
        .map_err(|_| NativeEngineError::Worker {
            operation: "write native Service Worker client leases".into(),
            reason: "native Service Worker client leases cannot be opened".into(),
        })?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.sync_data())
        .map_err(|_| NativeEngineError::Worker {
            operation: "write native Service Worker client leases".into(),
            reason: "native Service Worker client leases cannot be committed".into(),
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
                operation: "write native Service Worker client leases".into(),
                reason: format!(
                    "native Service Worker client leases cannot be committed: {rename_error}"
                ),
            });
        }
        let _ = fs::remove_file(&temporary_path);
    }
    Ok(())
}

pub(super) fn prune_service_worker_client_leases(
    clients: &mut Vec<NativeServiceWorkerClientLease>,
    now: u64,
) -> bool {
    let original_len = clients.len();
    clients.retain(|client| {
        now.saturating_sub(client.heartbeat_unix_seconds)
            <= NATIVE_SERVICE_WORKER_CLIENT_LEASE_TTL.as_secs()
    });
    clients.len() != original_len
}

pub(super) fn upsert_service_worker_client_lease(
    clients: &mut Vec<NativeServiceWorkerClientLease>,
    state: &NativeServiceWorkerClientState,
    owner_id: &str,
    now: u64,
) -> Result<bool, NativeEngineError> {
    validate_context_id(owner_id)?;
    validate_service_worker_client_states(std::slice::from_ref(state))?;
    if let Some(client) = clients
        .iter_mut()
        .find(|client| client.state.id == state.id)
    {
        let heartbeat_due = now.saturating_sub(client.heartbeat_unix_seconds)
            >= NATIVE_SERVICE_WORKER_CLIENT_HEARTBEAT.as_secs();
        let changed = client.owner_id != owner_id || client.state != *state || heartbeat_due;
        if changed {
            client.state = state.clone();
            client.owner_id = owner_id.to_owned();
            client.heartbeat_unix_seconds = now;
        }
        return Ok(changed);
    }
    if clients.len() >= MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES {
        return Err(NativeEngineError::limit(
            "native Service Worker client leases",
            MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES,
            clients.len().saturating_add(1),
        ));
    }
    clients.push(NativeServiceWorkerClientLease {
        state: state.clone(),
        owner_id: owner_id.to_owned(),
        heartbeat_unix_seconds: now,
    });
    Ok(true)
}

pub(crate) fn load_service_worker_client_leases(
    path: Option<&Path>,
) -> Result<Vec<NativeServiceWorkerClientState>, NativeEngineError> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let _lock = lock_web_storage_profile(path, true)?;
    let now = unix_time_seconds();
    let mut leases = read_service_worker_client_leases(path)?;
    if prune_service_worker_client_leases(&mut leases, now) {
        write_service_worker_client_leases(path, &leases)?;
    }
    Ok(leases.into_iter().map(|lease| lease.state).collect())
}

pub fn synchronize_service_worker_client_leases(
    path: Option<&Path>,
    local_leases: &[NativeServiceWorkerClientLease],
) -> Result<Vec<NativeServiceWorkerClientState>, NativeEngineError> {
    if local_leases.len() > MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES {
        return Err(NativeEngineError::limit(
            "native Service Worker client leases",
            MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES,
            local_leases.len(),
        ));
    }
    let local_states = local_leases
        .iter()
        .map(|lease| lease.state.clone())
        .collect::<Vec<_>>();
    validate_service_worker_client_states(&local_states)?;
    let Some(path) = path else {
        return Ok(local_states);
    };
    let _lock = lock_web_storage_profile(path, true)?;
    let now = unix_time_seconds();
    let mut leases = read_service_worker_client_leases(path)?;
    let mut changed = prune_service_worker_client_leases(&mut leases, now);
    for lease in local_leases {
        changed |=
            upsert_service_worker_client_lease(&mut leases, &lease.state, &lease.owner_id, now)?;
    }
    if changed {
        write_service_worker_client_leases(path, &leases)?;
    }
    Ok(leases.into_iter().map(|lease| lease.state).collect())
}

pub(crate) fn unregister_service_worker_client_lease(
    path: Option<&Path>,
    client_id: &str,
    owner_id: &str,
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    validate_context_id(client_id)?;
    validate_context_id(owner_id)?;
    let _lock = lock_web_storage_profile(path, true)?;
    let mut leases = read_service_worker_client_leases(path)?;
    let original_len = leases.len();
    leases.retain(|lease| !(lease.state.id == client_id && lease.owner_id == owner_id));
    if leases.len() != original_len {
        write_service_worker_client_leases(path, &leases)?;
    }
    Ok(())
}

pub(super) fn complete_storage_journal_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index.saturating_add(1))
}

pub(super) fn complete_storage_journal_prefix(bytes: &[u8], maximum: usize) -> usize {
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

pub(super) fn validate_storage_journal_event(
    event: &NativeStorageEvent,
) -> Result<(), NativeEngineError> {
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
        .is_some_and(|value| value.len() > crate::limits::MAX_TEXT_BYTES)
    {
        let actual = event.old_value.as_ref().map_or(0, String::len);
        return Err(NativeEngineError::limit(
            "native Web Storage old value",
            crate::limits::MAX_TEXT_BYTES,
            actual,
        ));
    }
    let mut state = NativeWebStorageState::default();
    state.apply_storage_event(event)
}

pub(super) fn validate_storage_journal_cookie_changes(
    changes: &[NativeCookieChange],
) -> Result<(), NativeEngineError> {
    if changes.len() > MAX_NATIVE_COOKIE_CHANGE_BATCH {
        return Err(NativeEngineError::limit(
            "native cookie journal changes",
            MAX_NATIVE_COOKIE_CHANGE_BATCH,
            changes.len(),
        ));
    }
    for change in changes {
        let key = NativeCookieProfileEntry {
            name: change.name.clone(),
            value: String::new(),
            domain: change.domain.clone(),
            path: change.path.clone(),
            host_only: true,
            secure: false,
            http_only: false,
            same_site: None,
            priority: None,
            expires_at_unix_seconds: None,
        };
        validate_cookie_profile_entry(&key)?;
        if let Some(cookie) = change.cookie.as_ref() {
            if cookie.name != change.name
                || cookie.domain != change.domain
                || cookie.path != change.path
            {
                return Err(NativeEngineError::invalid(
                    "native cookie journal change",
                    "cookie key does not match its change key",
                ));
            }
            validate_cookie_profile_entry(cookie)?;
        }
    }
    Ok(())
}

pub(super) fn decode_storage_journal_record(
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
    if record.indexed_db_changes.is_empty()
        && record.event.is_none()
        && record.cookie_changes.is_empty()
    {
        return Err(NativeEngineError::invalid(
            "native Web Storage event journal",
            "must contain a storage event, IndexedDB changes, or cookie changes",
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
    validate_storage_journal_cookie_changes(&record.cookie_changes)?;
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
    cookie_changes: &[NativeCookieChange],
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    if events.is_empty() && indexed_db_changes.is_empty() && cookie_changes.is_empty() {
        return Ok(());
    }
    validate_context_id(writer_id)?;
    validate_storage_journal_cookie_changes(cookie_changes)?;
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
            cookie_changes: Vec::new(),
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
            cookie_changes: Vec::new(),
        };
        let mut encoded = serde_json::to_vec(&record).map_err(|_| NativeEngineError::Worker {
            operation: "encode native Web Storage event journal".into(),
            reason: "native Web Storage event journal record cannot be encoded".into(),
        })?;
        encoded.push(b'\n');
        payload.extend(encoded);
    }
    if !cookie_changes.is_empty() {
        let record = NativeStorageJournalRecord {
            writer_id: writer_id.to_owned(),
            event: None,
            indexed_db_changes: Vec::new(),
            cookie_changes: cookie_changes.to_vec(),
        };
        let mut encoded = serde_json::to_vec(&record).map_err(|_| NativeEngineError::Worker {
            operation: "encode native cookie event journal".into(),
            reason: "native cookie event journal record cannot be encoded".into(),
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
    append_storage_changes(path, writer_id, events, &[], &[])
}

pub(super) fn write_storage_event_journal(
    path: &Path,
    bytes: &[u8],
) -> Result<(), NativeEngineError> {
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
#[path = "tests/storage_journal_tests.rs"]
mod storage_journal_tests;
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
    let _profile_lock = lock_web_storage_profile(path, false)?;
    let _cookie_lock = lock_web_storage_profile(&cookie_profile_path(path), false)?;
    if let Some(cookies) = read_cookie_profile_file(path)? {
        return Ok(cookies);
    }
    Ok(read_web_storage_profile(path)?.map_or_else(Vec::new, |profile| profile.cookies))
}

/// Move legacy cookies out of the combined profile before its path is shared
/// with the content process. The profile and cookie locks are always acquired
/// in that order by all writers, so this migration cannot deadlock a profile
/// update racing process startup.
pub(crate) fn migrate_cookie_profile_from_web_storage(
    path: Option<&Path>,
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    let _profile_lock = lock_web_storage_profile(path, true)?;
    let Some(mut profile) = read_web_storage_profile(path)? else {
        return Ok(());
    };
    let cookie_path = cookie_profile_path(path);
    let _cookie_lock = lock_web_storage_profile(&cookie_path, true)?;
    let cookies = match read_cookie_profile_file(path)? {
        Some(cookies) => cookies,
        None => profile.cookies.clone(),
    };
    if read_cookie_profile_file(path)?.is_none() && !cookies.is_empty() {
        write_cookie_profile_file(path, &cookies)?;
    }
    if profile.cookies.is_empty() {
        return Ok(());
    }
    profile.cookies.clear();
    write_web_storage_profile_file(path, &profile, "migrate native cookie profile")
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

pub(crate) fn load_service_worker_cache_profile(
    path: Option<&Path>,
) -> Result<NativeServiceWorkerCacheState, NativeEngineError> {
    let Some(path) = path else {
        return Ok(NativeServiceWorkerCacheState::default());
    };
    let _lock = lock_web_storage_profile(path, false)?;
    let Some(profile) = read_web_storage_profile(path)? else {
        return Ok(NativeServiceWorkerCacheState::default());
    };
    profile.service_worker_caches.validate()?;
    Ok(profile.service_worker_caches)
}

pub(crate) fn load_service_worker_registration_profiles(
    path: Option<&Path>,
) -> Result<Vec<NativeServiceWorkerRegistrationProfile>, NativeEngineError> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let _lock = lock_web_storage_profile(path, false)?;
    let Some(profile) = read_web_storage_profile(path)? else {
        return Ok(Vec::new());
    };
    validate_service_worker_registration_profiles(&profile.service_worker_registrations)?;
    Ok(profile.service_worker_registrations)
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
        service_worker_caches,
        service_worker_registrations,
        ..
    } = profile;
    let state = NativeWebStorageState {
        local,
        session: BTreeMap::new(),
    };
    validate_web_storage_state(&state)?;
    indexed_db.validate()?;
    service_worker_caches.validate()?;
    validate_service_worker_registration_profiles(&service_worker_registrations)?;
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
        service_worker_caches,
        service_worker_registrations,
    }))
}

pub(super) fn cookie_profile_path(path: &Path) -> PathBuf {
    let mut cookie_path = path.as_os_str().to_os_string();
    cookie_path.push(".cookies");
    PathBuf::from(cookie_path)
}

/// Read the sidecar while the caller holds the profile lock and then the
/// cookie-sidecar lock. A missing sidecar means the profile may still be in
/// the version-1 combined format.
pub(super) fn read_cookie_profile_file(
    profile_path: &Path,
) -> Result<Option<Vec<NativeCookieProfileEntry>>, NativeEngineError> {
    let cookie_path = cookie_profile_path(profile_path);
    let metadata = match fs::metadata(&cookie_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "load native cookie profile".into(),
                reason: "native cookie profile metadata is unavailable".into(),
            });
        }
    };
    let profile_bytes = usize::try_from(metadata.len()).map_err(|_| {
        NativeEngineError::limit(
            "native cookie profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            usize::MAX,
        )
    })?;
    if profile_bytes > MAX_WEB_STORAGE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native cookie profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            profile_bytes,
        ));
    }
    let bytes = fs::read(&cookie_path).map_err(|_| NativeEngineError::Worker {
        operation: "load native cookie profile".into(),
        reason: "native cookie profile cannot be read".into(),
    })?;
    let profile: NativeCookieProfileFile = serde_json::from_slice(&bytes).map_err(|_| {
        NativeEngineError::invalid(
            "cookie profile",
            "must contain a valid native cookie profile",
        )
    })?;
    if profile.version != NATIVE_COOKIE_PROFILE_FILE_VERSION {
        return Err(NativeEngineError::invalid(
            "cookie profile version",
            "is unsupported",
        ));
    }
    validate_cookie_profile(&profile.cookies)?;
    let now = unix_time_seconds();
    Ok(Some(
        profile
            .cookies
            .into_iter()
            .filter(|cookie| {
                cookie
                    .expires_at_unix_seconds
                    .is_none_or(|expires_at| expires_at > now)
            })
            .collect(),
    ))
}

pub(super) fn write_cookie_profile_file(
    profile_path: &Path,
    cookies: &[NativeCookieProfileEntry],
) -> Result<(), NativeEngineError> {
    validate_cookie_profile(cookies)?;
    let profile = NativeCookieProfileFile {
        version: NATIVE_COOKIE_PROFILE_FILE_VERSION,
        cookies: cookies.to_vec(),
    };
    let bytes = serde_json::to_vec(&profile).map_err(|_| NativeEngineError::Worker {
        operation: "save native cookie profile".into(),
        reason: "native cookie profile cannot be encoded".into(),
    })?;
    if bytes.len() > MAX_WEB_STORAGE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native cookie profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            bytes.len(),
        ));
    }
    write_profile_file_atomic(
        &cookie_profile_path(profile_path),
        &bytes,
        "native cookie profile",
    )
}

fn write_web_storage_profile_file(
    path: &Path,
    profile: &NativeWebStorageProfile,
    operation: &'static str,
) -> Result<(), NativeEngineError> {
    let bytes = serde_json::to_vec(profile).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "native Web Storage profile cannot be encoded".into(),
    })?;
    if bytes.len() > MAX_WEB_STORAGE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            bytes.len(),
        ));
    }
    write_profile_file_atomic(path, &bytes, "native Web Storage profile")
}

pub(super) fn write_profile_file_atomic(
    path: &Path,
    bytes: &[u8],
    resource_name: &'static str,
) -> Result<(), NativeEngineError> {
    let sequence = NATIVE_PROFILE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("profile");
    let temporary_path = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    ));
    fs::write(&temporary_path, bytes).map_err(|_| NativeEngineError::Worker {
        operation: format!("save {resource_name}"),
        reason: format!("{resource_name} cannot be written"),
    })?;
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
                operation: format!("save {resource_name}"),
                reason: format!("{resource_name} cannot be committed: {rename_error}"),
            });
        }
        let _ = fs::remove_file(&temporary_path);
    }
    Ok(())
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
    save_web_storage_profile_inner(
        path,
        state,
        storage_changes,
        cookie_state,
        cookie_changes,
        indexed_db,
        indexed_db_changes,
        true,
    )
}

pub(crate) fn save_content_web_storage_profile(
    path: Option<&Path>,
    state: &NativeWebStorageState,
    storage_changes: &[NativeStorageEvent],
    indexed_db: &NativeIndexedDbState,
    indexed_db_changes: &[NativeIndexedDbChange],
) -> Result<(), NativeEngineError> {
    save_web_storage_profile_inner(
        path,
        state,
        storage_changes,
        &[],
        &[],
        indexed_db,
        indexed_db_changes,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn save_web_storage_profile_inner(
    path: Option<&Path>,
    state: &NativeWebStorageState,
    storage_changes: &[NativeStorageEvent],
    cookie_state: &[NativeCookieProfileEntry],
    cookie_changes: &[NativeCookieChange],
    indexed_db: &NativeIndexedDbState,
    indexed_db_changes: &[NativeIndexedDbChange],
    parent_owns_cookies: bool,
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
    let sidecar_path = cookie_profile_path(path);
    let _cookie_lock = if parent_owns_cookies {
        Some(lock_web_storage_profile(&sidecar_path, true)?)
    } else {
        None
    };
    let mut local = current
        .as_ref()
        .map(|profile| profile.local.clone())
        .unwrap_or_else(|| state.local.clone());
    validate_cookie_profile(cookie_state)?;
    if current.is_some() {
        merge_local_storage_changes(&mut local, storage_changes)?;
    }
    let mut cookies = Vec::new();
    if parent_owns_cookies {
        let sidecar = read_cookie_profile_file(path)?;
        cookies = sidecar.unwrap_or_else(|| {
            current
                .as_ref()
                .map(|profile| profile.cookies.clone())
                .unwrap_or_else(|| cookie_state.to_vec())
        });
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
        cookies: Vec::new(),
        indexed_db: merged_indexed_db,
        service_worker_caches: current
            .as_ref()
            .map(|profile| profile.service_worker_caches.clone())
            .unwrap_or_default(),
        service_worker_registrations: current
            .as_ref()
            .map(|profile| profile.service_worker_registrations.clone())
            .unwrap_or_default(),
    };
    profile.service_worker_caches.validate()?;
    validate_service_worker_registration_profiles(&profile.service_worker_registrations)?;
    if parent_owns_cookies {
        let needs_cookie_sidecar = read_cookie_profile_file(path)?.is_some()
            || !cookies.is_empty()
            || !cookie_changes.is_empty()
            || current
                .as_ref()
                .is_some_and(|profile| !profile.cookies.is_empty());
        if needs_cookie_sidecar {
            write_cookie_profile_file(path, &cookies)?;
        }
    }
    write_web_storage_profile_file(path, &profile, "save native Web Storage profile")
}

pub(crate) fn save_service_worker_cache_profile(
    path: Option<&Path>,
    cache_state: &NativeServiceWorkerCacheState,
    registration_profiles: &[NativeServiceWorkerRegistrationProfile],
    registration_changes: &BTreeMap<String, Option<NativeServiceWorkerRegistrationProfile>>,
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    cache_state.validate()?;
    validate_service_worker_registration_profiles(registration_profiles)?;
    for (scope, profile) in registration_changes {
        if scope.is_empty() || scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker registration scope",
                MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                scope.len(),
            ));
        }
        if let Some(profile) = profile {
            if profile.scope != *scope {
                return Err(NativeEngineError::invalid(
                    "native service worker registration change",
                    "scope key must match the registration profile",
                ));
            }
            profile.validate()?;
        }
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|_| NativeEngineError::Worker {
            operation: "save native service worker cache profile".into(),
            reason: "native service worker cache profile directory cannot be created".into(),
        })?;
    }
    let _lock = lock_web_storage_profile(path, true)?;
    let current = read_web_storage_profile(path)?;
    let mut merged_registrations = current
        .as_ref()
        .map(|profile| profile.service_worker_registrations.clone())
        .unwrap_or_else(|| registration_profiles.to_vec());
    for (scope, change) in registration_changes {
        merged_registrations.retain(|profile| profile.scope != *scope);
        if let Some(profile) = change {
            merged_registrations.push(profile.clone());
        }
    }
    merged_registrations.sort_unstable_by(|left, right| left.scope.cmp(&right.scope));
    validate_service_worker_registration_profiles(&merged_registrations)?;
    let revision = current
        .as_ref()
        .map(|profile| profile.revision)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "save native service worker cache profile".into(),
            reason: "native Web Storage profile revision overflowed".into(),
        })?;
    let profile = NativeWebStorageProfile {
        version: WEB_STORAGE_PROFILE_VERSION,
        revision,
        local: current
            .as_ref()
            .map(|profile| profile.local.clone())
            .unwrap_or_default(),
        cookies: Vec::new(),
        indexed_db: current
            .as_ref()
            .map(|profile| profile.indexed_db.clone())
            .unwrap_or_default(),
        service_worker_caches: cache_state.clone(),
        service_worker_registrations: merged_registrations,
    };
    let bytes = serde_json::to_vec(&profile).map_err(|_| NativeEngineError::Worker {
        operation: "save native service worker cache profile".into(),
        reason: "native service worker cache profile cannot be encoded".into(),
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
        operation: "save native service worker cache profile".into(),
        reason: "native service worker cache profile cannot be written".into(),
    })?;
    if let Err(rename_error) = fs::rename(&temporary_path, path) {
        let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let fallback = fs::copy(&temporary_path, path).and_then(|copied_bytes| {
            (copied_bytes == expected_bytes)
                .then_some(())
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::WriteZero))
        });
        if fallback.is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(NativeEngineError::Worker {
                operation: "save native service worker cache profile".into(),
                reason: format!(
                    "native service worker cache profile cannot be committed: {rename_error}"
                ),
            });
        }
        let _ = fs::remove_file(&temporary_path);
    }
    Ok(())
}

pub(super) fn merge_local_storage_changes(
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

pub(super) fn merge_cookie_changes(
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

pub(super) fn validate_cookie_profile(
    cookies: &[NativeCookieProfileEntry],
) -> Result<(), NativeEngineError> {
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

pub(super) fn validate_cookie_profile_entry(
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
    if cookie.same_site.as_deref().is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "strict" | "lax" | "none"
        )
    }) {
        return Err(NativeEngineError::invalid(
            "native cookie SameSite",
            "must be Strict, Lax, or None",
        ));
    }
    if cookie
        .same_site
        .as_deref()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("none"))
        && !cookie.secure
    {
        return Err(NativeEngineError::invalid(
            "native cookie SameSite",
            "SameSite=None cookies must be Secure",
        ));
    }
    if cookie.priority.as_deref().is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "low" | "medium" | "high"
        )
    }) {
        return Err(NativeEngineError::invalid(
            "native cookie priority",
            "must be Low, Medium, or High",
        ));
    }
    Ok(())
}

pub(super) fn unix_time_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(super) fn validate_web_storage_state(
    state: &NativeWebStorageState,
) -> Result<(), NativeEngineError> {
    for origins in [&state.local, &state.session] {
        if origins.len() > crate::limits::MAX_STORAGE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native Web Storage origins",
                crate::limits::MAX_STORAGE_ENTRIES,
                origins.len(),
            ));
        }
        for (origin, entries) in origins {
            if origin.len() > crate::limits::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "native Web Storage origin",
                    crate::limits::MAX_TEXT_BYTES,
                    origin.len(),
                ));
            }
            if entries.len() > crate::limits::MAX_STORAGE_ENTRIES {
                return Err(NativeEngineError::limit(
                    "native Web Storage entries",
                    crate::limits::MAX_STORAGE_ENTRIES,
                    entries.len(),
                ));
            }
            for (key, value) in entries {
                if key.len() > crate::limits::MAX_BACKEND_ID_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Web Storage key",
                        crate::limits::MAX_BACKEND_ID_BYTES,
                        key.len(),
                    ));
                }
                if value.len() > crate::limits::MAX_TEXT_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Web Storage value",
                        crate::limits::MAX_TEXT_BYTES,
                        value.len(),
                    ));
                }
            }
        }
    }
    Ok(())
}

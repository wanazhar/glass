use super::*;

#[tokio::test]
async fn native_local_web_storage_persists_through_profile_restart() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-local-restart.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://persistent-storage", "<p>Persistent storage</p>")
        .unwrap()
        .with_initial_url("fixture://persistent-storage");

    {
        let mut engine = NativeEngine::new(config.clone()).unwrap();
        engine.initialize().unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "localStorage.setItem('theme', 'dark'); sessionStorage.setItem('tab', 'one'); true",
                )
                .await
                .unwrap(),
            serde_json::json!(true)
        );
        engine
            .navigate("fixture://persistent-storage#next")
            .unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "({ local: localStorage.getItem('theme'), session: sessionStorage.getItem('tab') })",
                )
                .await
                .unwrap(),
            serde_json::json!({"local":"dark","session":"one"})
        );
        engine.close().unwrap();
    }

    let profile = fs::read_to_string(&profile_path).unwrap();
    assert!(profile.contains("theme"));
    assert!(!profile.contains("tab"));

    {
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "({ local: localStorage.getItem('theme'), session: sessionStorage.getItem('tab') })",
                )
                .await
                .unwrap(),
            serde_json::json!({"local":"dark","session":null})
        );
        engine.close().unwrap();
    }
    let _ = fs::remove_file(profile_path);
}
#[tokio::test]
async fn native_local_indexed_db_supports_upgrade_crud_and_profile_restart() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-restart.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://indexed-db", "<p>IndexedDB owner</p>")
        .unwrap()
        .with_initial_url("fixture://indexed-db");

    {
        let mut engine = NativeEngine::new(config.clone()).unwrap();
        engine.initialize().unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "await (async () => { const request = indexedDB.open('notes', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('notes', { keyPath: 'id' }); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const write = db.transaction('notes', 'readwrite'); const store = write.objectStore('notes'); const initial = store.put({ id: 'one', title: 'hello', count: 2 }); await new Promise((resolve, reject) => { initial.onsuccess = () => { store.put({ id: 'one', title: 'hello', count: 3 }); write.oncomplete = resolve; }; initial.onerror = () => reject(initial.error); write.onerror = () => reject(write.error); }); const read = db.transaction('notes', 'readonly'); const get = read.objectStore('notes').get('one'); const value = await new Promise((resolve, reject) => { get.onsuccess = () => resolve(get.result); get.onerror = () => reject(get.error); }); return { name: db.name, version: db.version, stores: db.objectStoreNames, value, count: await new Promise((resolve, reject) => { const count = db.transaction('notes', 'readonly').objectStore('notes').count(); count.onsuccess = () => resolve(count.result); count.onerror = () => reject(count.error); }) }; })()",
                )
                .await
                .unwrap(),
            serde_json::json!({
                "name": "notes",
                "version": 1,
                "stores": ["notes"],
                "value": {"id": "one", "title": "hello", "count": 3},
                "count": 1,
            })
        );
        engine.close().unwrap();
    }

    let profile = fs::read_to_string(&profile_path).unwrap();
    assert!(profile.contains("indexed_db"));
    assert!(profile.contains("hello"));

    {
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "await (async () => { const request = indexedDB.open('notes'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const read = db.transaction('notes', 'readonly').objectStore('notes').getAll(); return await new Promise((resolve, reject) => { read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error); }); })()",
                )
                .await
                .unwrap(),
            serde_json::json!([{"id": "one", "title": "hello", "count": 3}])
        );
        engine.close().unwrap();
    }
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_supports_indexes_key_ranges_and_cursors() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-indexes.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://indexed-db-indexes", "<p>IndexedDB indexes</p>")
        .unwrap()
        .with_initial_url("fixture://indexed-db-indexes");

    {
        let mut engine = NativeEngine::new(config.clone()).unwrap();
        engine.initialize().unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "await (async () => { const request = indexedDB.open('catalog', 1); request.onupgradeneeded = event => { const store = event.target.result.createObjectStore('records', { keyPath: 'id' }); store.createIndex('byTag', 'tags', { multiEntry: true }); store.createIndex('byType', 'type'); store.createIndex('bySku', 'sku', { unique: true }); }; const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const write = db.transaction('records', 'readwrite'); const store = write.objectStore('records'); for (const record of [{ id: 'c', type: 'b', sku: 'sku-c', tags: ['green', 'blue'] }, { id: 'a', type: 'a', sku: 'sku-a', tags: ['green'] }, { id: 'b', type: 'b', sku: 'sku-b', tags: ['blue'] }]) store.put(record); await new Promise((resolve, reject) => { write.oncomplete = resolve; write.onerror = () => reject(write.error); }); const read = db.transaction('records', 'readonly'); const records = read.objectStore('records'); const byType = records.index('byType'); const byTag = records.index('byTag'); const requestValue = request => new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const values = await requestValue(byType.getAll(IDBKeyRange.bound('a', 'b'))); const green = await requestValue(byTag.getAll('green')); const firstGreenKey = await requestValue(byTag.getKey('green')); const count = await requestValue(byType.count(IDBKeyRange.lowerBound('b'))); const cursorKeys = await new Promise((resolve, reject) => { const cursorRequest = byType.openCursor(); const seen = []; cursorRequest.onsuccess = () => { const cursor = cursorRequest.result; if (!cursor) return resolve(seen); seen.push(cursor.primaryKey); if (seen.length === 1) cursor.advance(2); else cursor.continue(); }; cursorRequest.onerror = () => reject(cursorRequest.error); }); const keyCursorKeys = await new Promise((resolve, reject) => { const cursorRequest = records.openKeyCursor(IDBKeyRange.lowerBound('b'), 'prev'); const seen = []; cursorRequest.onsuccess = () => { const cursor = cursorRequest.result; if (!cursor) return resolve(seen); seen.push(cursor.key); cursor.continue(); }; cursorRequest.onerror = () => reject(cursorRequest.error); }); const conflictTx = db.transaction('records', 'readwrite'); const conflict = conflictTx.objectStore('records').put({ id: 'd', type: 'd', sku: 'sku-a', tags: [] }); const uniqueError = await new Promise(resolve => { conflict.onsuccess = () => resolve('none'); conflict.onerror = () => resolve(conflict.error.name); }); await new Promise((resolve, reject) => { const mutation = db.transaction('records', 'readwrite'); const mutationStore = mutation.objectStore('records'); const mutationCursor = mutationStore.index('byType').openCursor(IDBKeyRange.only('b')); mutation.oncomplete = resolve; mutation.onerror = () => reject(mutation.error); mutationCursor.onerror = () => reject(mutationCursor.error); mutationCursor.onsuccess = () => { const cursor = mutationCursor.result; if (!cursor) return; if (cursor.primaryKey === 'b') { const update = cursor.update({ id: 'b', type: 'b', sku: 'sku-b', tags: ['updated'] }); update.onerror = () => reject(update.error); update.onsuccess = () => cursor.continue(); } else { const remove = cursor.delete(); remove.onerror = () => reject(remove.error); remove.onsuccess = () => cursor.continue(); } }; }); const updated = await requestValue(db.transaction('records', 'readonly').objectStore('records').index('byTag').get('updated')); const remaining = await requestValue(db.transaction('records', 'readonly').objectStore('records').index('byType').getAll('b')); return { indexNames: records.indexNames, values: values.map(value => value.id), green: green.map(value => value.id), firstGreenKey, count, cursorKeys, keyCursorKeys, uniqueError, updated: updated.id, remaining: remaining.map(value => value.id) }; })()",
                )
                .await
                .unwrap(),
            serde_json::json!({
                "indexNames": ["bySku", "byTag", "byType"],
                "values": ["a", "b", "c"],
                "green": ["a", "c"],
                "firstGreenKey": "a",
                "count": 2,
                "cursorKeys": ["a", "c"],
                "keyCursorKeys": ["c", "b"],
                "uniqueError": "ConstraintError",
                "updated": "b",
                "remaining": ["b"],
            })
        );
        engine.close().unwrap();
    }

    {
        let mut engine = NativeEngine::new(config).unwrap();
        engine.initialize().unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "await (async () => { const request = indexedDB.open('catalog'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const read = db.transaction('records', 'readonly').objectStore('records').index('bySku').get('sku-b'); return await new Promise((resolve, reject) => { read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error); }); })()",
                )
                .await
                .unwrap(),
            serde_json::json!({"id": "b", "type": "b", "sku": "sku-b", "tags": ["updated"]})
        );
        engine.close().unwrap();
    }
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_versionchange_unblocks_closed_connection() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-versionchange.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture(
            "fixture://indexed-db-versionchange",
            "<p>IndexedDB version change</p>",
        )
        .unwrap()
        .with_initial_url("fixture://indexed-db-versionchange");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('versions', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('v1'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); globalThis.versionConnection = db; return db.version; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(1)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await new Promise((resolve, reject) => { const events = []; versionConnection.onversionchange = event => events.push(`versionchange:${event.oldVersion}:${event.newVersion}`); const request = indexedDB.open('versions', 2); request.onblocked = () => { events.push('blocked'); versionConnection.close(); }; request.onupgradeneeded = event => { events.push('upgrade'); event.target.result.createObjectStore('v2'); }; request.onsuccess = () => resolve({ version: request.result.version, stores: request.result.objectStoreNames, events }); request.onerror = () => reject(request.error); })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "version": 2,
            "stores": ["v1", "v2"],
            "events": ["versionchange:1:2", "blocked", "upgrade"],
        })
    );
    engine.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_delete_blocks_open_connection() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-delete.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://indexed-db-delete", "<p>IndexedDB deletion</p>")
        .unwrap()
        .with_initial_url("fixture://indexed-db-delete");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('discarded', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); globalThis.discardedConnection = db; return { version: db.version, stores: db.objectStoreNames }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({"version": 1, "stores": ["records"]})
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await new Promise((resolve, reject) => { const events = []; discardedConnection.onversionchange = event => events.push(`versionchange:${event.oldVersion}:${event.newVersion}`); const request = indexedDB.deleteDatabase('discarded'); request.onblocked = () => { events.push('blocked'); discardedConnection.close(); }; request.onsuccess = async () => resolve({ events, databases: await indexedDB.databases() }); request.onerror = () => reject(request.error); })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["versionchange:1:null", "blocked"],
            "databases": [],
        })
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('discarded'); request.onupgradeneeded = event => event.target.result.createObjectStore('fresh'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); return { version: db.version, stores: db.objectStoreNames }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({"version": 1, "stores": ["fresh"]})
    );
    engine.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_transaction_rolls_back_failed_write() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-rollback.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://indexed-db-rollback", "<p>IndexedDB rollback</p>")
        .unwrap()
        .with_initial_url("fixture://indexed-db-rollback");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('rollback', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); return await new Promise((resolve, reject) => { request.onsuccess = () => resolve(true); request.onerror = () => reject(request.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('rollback'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); return await new Promise((resolve, reject) => { const events = []; const transaction = db.transaction('records', 'readwrite'); const store = transaction.objectStore('records'); const first = store.put({ id: 'one' }, 'one'); const second = store.add({ id: 'one' }, 'one'); second.onerror = () => events.push(`request:${second.error.name}`); transaction.onabort = () => { events.push('abort'); const read = db.transaction('records', 'readonly').objectStore('records').count(); read.onsuccess = () => resolve({ events, count: read.result, transactionError: transaction.error.name }); read.onerror = () => reject(read.error); }; transaction.oncomplete = () => reject(new Error('failed transaction unexpectedly completed')); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["request:ConstraintError", "abort"],
            "count": 0,
            "transactionError": "ConstraintError",
        })
    );
    engine.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_explicit_abort_rolls_back_write() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-explicit-abort.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture(
            "fixture://indexed-db-explicit-abort",
            "<p>IndexedDB abort</p>",
        )
        .unwrap()
        .with_initial_url("fixture://indexed-db-explicit-abort");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('abort', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); return await new Promise((resolve, reject) => { request.onsuccess = () => resolve(true); request.onerror = () => reject(request.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('abort'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); return await new Promise((resolve, reject) => { const events = []; const transaction = db.transaction('records', 'readwrite'); const write = transaction.objectStore('records').put({ id: 'one' }, 'one'); write.onsuccess = () => { events.push('write'); transaction.abort(); }; transaction.onabort = () => { events.push('abort'); const read = db.transaction('records', 'readonly').objectStore('records').count(); read.onsuccess = () => resolve({ events, count: read.result, transactionError: transaction.error.name }); read.onerror = () => reject(read.error); }; transaction.oncomplete = () => reject(new Error('aborted transaction unexpectedly completed')); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["write", "abort"],
            "count": 0,
            "transactionError": "AbortError",
        })
    );
    engine.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_storage_manager_reports_bounded_estimate() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-storage-{}-estimate.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://storage-estimate", "<p>Storage estimate</p>")
        .unwrap()
        .with_initial_url("fixture://storage-estimate");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const before = await navigator.storage.estimate(); localStorage.setItem('quota-probe', 'x'.repeat(32)); const after = await navigator.storage.estimate(); return { available: typeof navigator.storage.estimate === 'function' && typeof navigator.storage.persist === 'function' && typeof navigator.storage.persisted === 'function', quota: after.quota, grew: after.usage >= before.usage, persisted: await navigator.storage.persisted(), requested: await navigator.storage.persist() }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "available": true,
            "quota": 4 * 1024 * 1024,
            "grew": true,
            "persisted": false,
            "requested": false,
        })
    );
    engine.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_serializes_transactions_before_abort() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-serialize.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture(
            "fixture://indexed-db-serialize",
            "<p>IndexedDB serialize</p>",
        )
        .unwrap()
        .with_initial_url("fixture://indexed-db-serialize");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('serialize', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); return await new Promise((resolve, reject) => { request.onsuccess = () => resolve(true); request.onerror = () => reject(request.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('serialize'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); return await new Promise((resolve, reject) => { const events = []; const firstTransaction = db.transaction('records', 'readwrite'); const secondTransaction = db.transaction('records', 'readwrite'); const first = firstTransaction.objectStore('records').put({ owner: 'first' }, 'first'); const second = secondTransaction.objectStore('records').put({ owner: 'second' }, 'second'); first.onsuccess = () => { events.push('first-write'); firstTransaction.abort(); }; firstTransaction.onabort = () => events.push('first-abort'); secondTransaction.oncomplete = () => { events.push('second-complete'); const read = db.transaction('records', 'readonly').objectStore('records').count(); read.onsuccess = () => resolve({ events, count: read.result }); read.onerror = () => reject(read.error); }; secondTransaction.onerror = () => reject(secondTransaction.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["first-write", "first-abort", "second-complete"],
            "count": 1,
        })
    );
    engine.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_round_trips_bounded_structured_clone_extensions() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-structured-clone.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture(
            "fixture://indexed-db-structured-clone",
            "<p>IndexedDB structured clone</p>",
        )
        .unwrap()
        .with_initial_url("fixture://indexed-db-structured-clone");
    let mut engine = NativeEngine::new(config.clone()).unwrap();
    engine.initialize().unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('structured', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); return await new Promise((resolve, reject) => { request.onsuccess = () => resolve(true); request.onerror = () => reject(request.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('structured'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); return await new Promise((resolve, reject) => { const transaction = db.transaction('records', 'readwrite'); const value = { undefinedValue: undefined, date: new Date('2026-01-02T03:04:05.000Z'), pattern: /glass/gi, numbers: [NaN, Infinity, -Infinity, -0], map: new Map([['missing', undefined], ['date', new Date('2026-01-02T03:04:05.000Z')]]), set: new Set(['a', 'b']), blob: new Blob(['payload'], { type: 'text/plain' }), file: new File(['note'], 'note.txt', { type: 'text/plain', lastModified: 7 }), buffer: new Uint8Array([1, 2, 255]).buffer, bytes: new Uint16Array([258, 513]), view: new DataView(new Uint8Array([9, 8]).buffer) }; const write = transaction.objectStore('records').put(value, 'special'); write.onerror = () => reject(write.error); transaction.oncomplete = () => resolve(true); transaction.onabort = () => reject(transaction.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine.close().unwrap();

    let mut restarted = NativeEngine::new(config).unwrap();
    restarted.initialize().unwrap();
    assert_eq!(
        restarted
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('structured'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const read = db.transaction('records', 'readonly').objectStore('records').get('special'); const value = await new Promise((resolve, reject) => { read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error); }); return { undefinedValue: Object.prototype.hasOwnProperty.call(value, 'undefinedValue') && value.undefinedValue === undefined, date: value.date instanceof Date && value.date.toISOString(), regexp: value.pattern instanceof RegExp && value.pattern.source === 'glass' && value.pattern.flags === 'gi' && value.pattern.lastIndex === 0, nan: Number.isNaN(value.numbers[0]), positiveInfinity: value.numbers[1] === Infinity, negativeInfinity: value.numbers[2] === -Infinity, negativeZero: Object.is(value.numbers[3], -0), map: value.map instanceof Map && value.map.get('missing') === undefined && value.map.get('date') instanceof Date, set: value.set instanceof Set && value.set.has('a') && value.set.has('b'), blob: value.blob instanceof Blob && value.blob.type === 'text/plain' && await value.blob.text() === 'payload', file: value.file instanceof File && value.file.name === 'note.txt' && value.file.lastModified === 7 && await value.file.text() === 'note', buffer: value.buffer instanceof ArrayBuffer && Array.from(new Uint8Array(value.buffer)).join(',') === '1,2,255', typedArray: value.bytes instanceof Uint16Array && value.bytes[0] === 258 && value.bytes[1] === 513, dataView: value.view instanceof DataView && value.view.getUint8(0) === 9 && value.view.getUint8(1) === 8 }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "undefinedValue": true,
            "date": "2026-01-02T03:04:05.000Z",
            "regexp": true,
            "nan": true,
            "positiveInfinity": true,
            "negativeInfinity": true,
            "negativeZero": true,
            "map": true,
            "set": true,
            "blob": true,
            "file": true,
            "buffer": true,
            "typedArray": true,
            "dataView": true,
        })
    );
    assert_eq!(
        restarted
            .evaluate_async(
                "await (async () => { if (typeof SharedArrayBuffer !== 'function') return true; const request = indexedDB.open('structured'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const transaction = db.transaction('records', 'readwrite'); const write = transaction.objectStore('records').put(new SharedArrayBuffer(2), 'shared'); return await new Promise(resolve => { write.onsuccess = () => resolve(false); write.onerror = () => resolve(write.error && write.error.name === 'DataCloneError'); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    restarted.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_indexed_db_journal_merges_live_origin_writers() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-indexed-db-{}-journal.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://indexed-db-journal", "<p>IndexedDB journal</p>")
        .unwrap()
        .with_initial_url("fixture://indexed-db-journal");
    let mut first = NativeEngine::new(config.clone()).unwrap();
    let mut second = NativeEngine::new(config).unwrap();
    first.initialize().unwrap();
    second.initialize().unwrap();

    first
        .evaluate_async(
            "await (async () => { const request = indexedDB.open('alpha', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const write = db.transaction('records', 'readwrite').objectStore('records').put({ owner: 'first' }, 'one'); return await new Promise((resolve, reject) => { write.onsuccess = () => resolve(true); write.onerror = () => reject(write.error); }); })()",
        )
        .await
        .unwrap();
    second
        .evaluate_async(
            "await (async () => { const request = indexedDB.open('beta', 1); request.onupgradeneeded = event => event.target.result.createObjectStore('records'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const write = db.transaction('records', 'readwrite').objectStore('records').put({ owner: 'second' }, 'two'); return await new Promise((resolve, reject) => { write.onsuccess = () => resolve(true); write.onerror = () => reject(write.error); }); })()",
        )
        .await
        .unwrap();
    assert_eq!(
        first
            .evaluate_async(
                "await (async () => { const request = indexedDB.open('beta'); const db = await new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); }); const read = db.transaction('records', 'readonly').objectStore('records').get('two'); return await new Promise((resolve, reject) => { read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error); }); })()",
            )
            .await
            .unwrap(),
        serde_json::json!({"owner": "second"})
    );
    first.close().unwrap();
    second.close().unwrap();
    for suffix in ["", "lock", "events", "readers"] {
        let path = if suffix.is_empty() {
            profile_path.clone()
        } else {
            profile_path.with_extension(suffix)
        };
        let _ = fs::remove_file(path);
    }
}
#[tokio::test]
async fn native_local_web_storage_merges_stale_profile_deltas() {
    let file_name = format!(
        "glass-native-web-storage-{}-stale-merge.json",
        std::process::id()
    );
    let first_path = std::env::temp_dir().join(&file_name);
    let second_path = std::env::temp_dir().join(".").join(&file_name);
    let lock_path = first_path.with_extension("lock");
    let _ = fs::remove_file(&first_path);
    let _ = fs::remove_file(&lock_path);
    let first_config = NativeEngineConfig::default()
        .with_storage_path(first_path.clone())
        .with_fixture("fixture://stale-profile-merge", "<p>Profile merge</p>")
        .unwrap()
        .with_initial_url("fixture://stale-profile-merge");
    let second_config = first_config.clone().with_storage_path(second_path);
    let mut first = NativeEngine::new(first_config.clone()).unwrap();
    let mut second = NativeEngine::new(second_config).unwrap();
    first.initialize().unwrap();
    second.initialize().unwrap();

    first
        .evaluate_async("localStorage.setItem('first', 'one'); true")
        .await
        .unwrap();
    second
        .evaluate_async("localStorage.setItem('second', 'two'); true")
        .await
        .unwrap();
    first.close().unwrap();
    second.close().unwrap();

    let profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&first_path).unwrap()).unwrap();
    assert!(profile["revision"].as_u64().unwrap_or_default() >= 2);
    let local = profile["local"].as_object().unwrap();
    assert_eq!(local.len(), 1);
    let entries = local.values().next().unwrap();
    assert_eq!(entries["first"], "one");
    assert_eq!(entries["second"], "two");

    let mut reopened = NativeEngine::new(first_config).unwrap();
    reopened.initialize().unwrap();
    assert_eq!(
        reopened
            .evaluate_async(
                "({ first: localStorage.getItem('first'), second: localStorage.getItem('second') })",
            )
            .await
            .unwrap(),
        serde_json::json!({"first":"one", "second":"two"})
    );
    reopened.close().unwrap();
    let _ = fs::remove_file(first_path);
    let _ = fs::remove_file(lock_path);
}
#[tokio::test]
async fn native_local_web_storage_delivers_origin_filtered_events_to_other_documents() {
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-cross-document-events.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let owner_config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://storage-events", "<p>Storage owner</p>")
        .unwrap()
        .with_initial_url("fixture://storage-events");
    let observer_config = owner_config.clone();
    let other_origin_config = NativeEngineConfig::default()
        .with_storage_path(profile_path.clone())
        .with_fixture("fixture://other-storage-events", "<p>Other storage</p>")
        .unwrap()
        .with_initial_url("fixture://other-storage-events");

    let mut owner = NativeEngine::new(owner_config).unwrap();
    let mut observer = NativeEngine::new(observer_config).unwrap();
    let mut other_origin = NativeEngine::new(other_origin_config).unwrap();
    owner.initialize().unwrap();
    observer.initialize().unwrap();
    other_origin.initialize().unwrap();

    for engine in [&mut owner, &mut observer, &mut other_origin] {
        engine
            .evaluate_async(
                "globalThis.storageEvents = []; addEventListener('storage', event => storageEvents.push({ key: event.key, oldValue: event.oldValue, newValue: event.newValue, url: event.url, local: event.storageArea === localStorage })); true",
            )
            .await
            .unwrap();
    }

    owner
        .evaluate_async("localStorage.setItem('theme', 'dark'); true")
        .await
        .unwrap();
    assert_eq!(
        observer
            .evaluate_async("storageEvents.splice(0)")
            .await
            .unwrap(),
        serde_json::json!([{
            "key": "theme",
            "oldValue": null,
            "newValue": "dark",
            "url": "fixture://storage-events",
            "local": true,
        }])
    );
    assert_eq!(
        other_origin.evaluate_async("storageEvents").await.unwrap(),
        serde_json::json!([])
    );
    assert_eq!(
        owner.evaluate_async("storageEvents").await.unwrap(),
        serde_json::json!([])
    );

    owner
        .evaluate_async("localStorage.setItem('theme', 'dark'); true")
        .await
        .unwrap();
    assert_eq!(
        observer
            .evaluate_async("storageEvents.splice(0)")
            .await
            .unwrap(),
        serde_json::json!([])
    );

    owner
        .evaluate_async("localStorage.setItem('theme', 'light'); true")
        .await
        .unwrap();
    assert_eq!(
        observer
            .evaluate_async("storageEvents.splice(0)")
            .await
            .unwrap(),
        serde_json::json!([{
            "key": "theme",
            "oldValue": "dark",
            "newValue": "light",
            "url": "fixture://storage-events",
            "local": true,
        }])
    );

    owner
        .evaluate_async("localStorage.removeItem('theme'); true")
        .await
        .unwrap();
    assert_eq!(
        observer
            .evaluate_async("storageEvents.splice(0)")
            .await
            .unwrap(),
        serde_json::json!([{
            "key": "theme",
            "oldValue": "light",
            "newValue": null,
            "url": "fixture://storage-events",
            "local": true,
        }])
    );

    owner
        .evaluate_async("localStorage.setItem('one', '1'); localStorage.setItem('two', '2'); true")
        .await
        .unwrap();
    let _ = observer
        .evaluate_async("storageEvents.splice(0)")
        .await
        .unwrap();
    owner
        .evaluate_async("localStorage.clear(); true")
        .await
        .unwrap();
    assert_eq!(
        observer
            .evaluate_async("({ events: storageEvents.splice(0), length: localStorage.length })")
            .await
            .unwrap(),
        serde_json::json!({
            "events": [{
                "key": null,
                "oldValue": null,
                "newValue": null,
                "url": "fixture://storage-events",
                "local": true,
            }],
            "length": 0,
        })
    );

    other_origin.close().unwrap();
    observer.close().unwrap();
    owner.close().unwrap();
    let _ = fs::remove_file(profile_path);
}

use super::*;

fn test_profile_path(label: &str) -> PathBuf {
    let sequence = NATIVE_STORAGE_WRITER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "glass-native-storage-journal-{}-{label}-{sequence}.json",
        std::process::id()
    ))
}

fn remove_test_profile(path: &Path) {
    for suffix in ["", "lock", "events", "readers", "clients"] {
        let candidate = if suffix.is_empty() {
            path.to_owned()
        } else {
            path.with_extension(suffix)
        };
        let _ = fs::remove_file(candidate);
    }
    let _ = fs::remove_file(path.with_extension(format!("events-tmp-{}", std::process::id())));
    let _ = fs::remove_file(path.with_extension(format!("readers-tmp-{}", std::process::id())));
    let _ = fs::remove_file(path.with_extension(format!(
        "clients-tmp-{}-{}",
        std::process::id(),
        NATIVE_STORAGE_WRITER_SEQUENCE.load(Ordering::Relaxed)
    )));
}

fn journal_event(index: usize) -> NativeStorageEvent {
    NativeStorageEvent {
        source_context_id: "source".into(),
        scope: "local".into(),
        storage_key: "https://journal.test".into(),
        key: Some(format!("key-{index}")),
        old_value: None,
        new_value: Some("x".repeat(crate::limits::MAX_TEXT_BYTES)),
        url: "https://journal.test/".into(),
    }
}

fn journal_cookie_change(name: &str, value: Option<&str>) -> NativeCookieChange {
    NativeCookieChange {
        name: name.into(),
        domain: "journal.test".into(),
        path: "/".into(),
        cookie: value.map(|value| NativeCookieProfileEntry {
            name: name.into(),
            value: value.into(),
            domain: "journal.test".into(),
            path: "/".into(),
            host_only: true,
            secure: false,
            http_only: false,
            same_site: Some("Lax".into()),
            priority: None,
            expires_at_unix_seconds: None,
        }),
    }
}

#[test]
fn empty_web_storage_profile_is_treated_as_uninitialized_without_rewriting_it() {
    let profile_path = test_profile_path("empty-web-storage");
    remove_test_profile(&profile_path);
    fs::write(&profile_path, []).unwrap();

    let state = load_web_storage_profile(Some(&profile_path)).unwrap();
    assert!(state.local.is_empty());
    assert!(state.session.is_empty());
    assert_eq!(fs::metadata(&profile_path).unwrap().len(), 0);

    remove_test_profile(&profile_path);
}

#[test]
fn cookie_changes_round_trip_through_profile_journal_and_old_records_decode() {
    let profile_path = test_profile_path("cookie-roundtrip");
    remove_test_profile(&profile_path);
    register_storage_reader(Some(&profile_path), "reader", 0).unwrap();
    let changes = [
        journal_cookie_change("visible", Some("first")),
        journal_cookie_change("visible", Some("latest")),
        journal_cookie_change("removed", None),
    ];
    append_storage_changes(Some(&profile_path), "writer", &[], &[], &changes).unwrap();

    let mut cursor = 0;
    let journal = read_storage_event_journal(Some(&profile_path), "reader", &mut cursor).unwrap();
    assert!(!journal.recovered);
    assert_eq!(journal.records.len(), 1);
    assert_eq!(journal.records[0].cookie_changes, changes);

    let old_record = serde_json::json!({
        "writer_id": "writer",
        "event": journal_event(0),
        "indexed_db_changes": [],
    });
    let old_record = serde_json::to_vec(&old_record).unwrap();
    let decoded = decode_storage_journal_record(&old_record).unwrap();
    assert!(decoded.cookie_changes.is_empty());

    let mut invalid = journal_cookie_change("visible", Some("value"));
    invalid.cookie.as_mut().unwrap().name = "different".into();
    assert!(append_storage_changes(Some(&profile_path), "writer", &[], &[], &[invalid]).is_err());

    unregister_storage_reader(Some(&profile_path), "reader").unwrap();
    remove_test_profile(&profile_path);
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

#[test]
fn service_worker_client_leases_prune_stale_and_fence_release() {
    let profile_path = test_profile_path("client-leases");
    remove_test_profile(&profile_path);
    let state = NativeServiceWorkerClientState {
        id: "client-one".into(),
        url: "https://client.test/".into(),
        client_type: "window".into(),
        frame_type: "top-level".into(),
        visibility_state: "visible".into(),
        focused: true,
    };
    let owner_a = NativeServiceWorkerClientLease {
        state: state.clone(),
        owner_id: "owner-a".into(),
        heartbeat_unix_seconds: 0,
    };
    assert_eq!(
        synchronize_service_worker_client_leases(
            Some(&profile_path),
            std::slice::from_ref(&owner_a),
        )
        .unwrap(),
        vec![state.clone()]
    );
    let owner_b = NativeServiceWorkerClientLease {
        state: state.clone(),
        owner_id: "owner-b".into(),
        heartbeat_unix_seconds: 0,
    };
    synchronize_service_worker_client_leases(Some(&profile_path), std::slice::from_ref(&owner_b))
        .unwrap();
    unregister_service_worker_client_lease(Some(&profile_path), "client-one", "owner-a").unwrap();
    assert_eq!(
        load_service_worker_client_leases(Some(&profile_path)).unwrap(),
        vec![state.clone()]
    );
    unregister_service_worker_client_lease(Some(&profile_path), "client-one", "owner-b").unwrap();
    assert!(
        load_service_worker_client_leases(Some(&profile_path))
            .unwrap()
            .is_empty()
    );

    let stale = NativeServiceWorkerClientLease {
        state,
        owner_id: "owner-stale".into(),
        heartbeat_unix_seconds: unix_time_seconds()
            .saturating_sub(NATIVE_SERVICE_WORKER_CLIENT_LEASE_TTL.as_secs() + 1),
    };
    write_service_worker_client_leases(&profile_path, &[stale]).unwrap();
    assert!(
        load_service_worker_client_leases(Some(&profile_path))
            .unwrap()
            .is_empty()
    );
    remove_test_profile(&profile_path);
}

#[test]
fn service_worker_registration_profile_writes_merge_scoped_changes() {
    let profile_path = test_profile_path("registration-merge");
    remove_test_profile(&profile_path);
    let first = NativeServiceWorkerRegistrationProfile {
        script_url: "https://registration.test/one/sw.js".into(),
        scope: "https://registration.test/one/".into(),
        worker_type: "classic".into(),
        navigation_preload_enabled: true,
        navigation_preload_header_value: "release-candidate".into(),
        waiting: None,
    };
    let second = NativeServiceWorkerRegistrationProfile {
        script_url: "https://registration.test/two/sw.js".into(),
        scope: "https://registration.test/two/".into(),
        worker_type: "classic".into(),
        navigation_preload_enabled: false,
        navigation_preload_header_value: "true".into(),
        waiting: None,
    };
    let legacy_profile: NativeServiceWorkerRegistrationProfile =
        serde_json::from_value(serde_json::json!({
            "script_url": "https://registration.test/legacy/sw.js",
            "scope": "https://registration.test/legacy/",
            "worker_type": "classic",
        }))
        .expect("older ServiceWorker profiles decode with navigation preload defaults");
    assert!(!legacy_profile.navigation_preload_enabled);
    assert_eq!(legacy_profile.navigation_preload_header_value, "true");

    save_service_worker_cache_profile(
        Some(&profile_path),
        &NativeServiceWorkerCacheState::default(),
        std::slice::from_ref(&first),
        &BTreeMap::from([(first.scope.clone(), Some(first.clone()))]),
    )
    .unwrap();
    save_service_worker_cache_profile(
        Some(&profile_path),
        &NativeServiceWorkerCacheState::default(),
        std::slice::from_ref(&second),
        &BTreeMap::from([(second.scope.clone(), Some(second.clone()))]),
    )
    .unwrap();
    assert_eq!(
        load_service_worker_registration_profiles(Some(&profile_path)).unwrap(),
        vec![first.clone(), second.clone()]
    );

    save_service_worker_cache_profile(
        Some(&profile_path),
        &NativeServiceWorkerCacheState::default(),
        std::slice::from_ref(&second),
        &BTreeMap::from([(first.scope.clone(), None)]),
    )
    .unwrap();
    assert_eq!(
        load_service_worker_registration_profiles(Some(&profile_path)).unwrap(),
        vec![second]
    );
    remove_test_profile(&profile_path);
}

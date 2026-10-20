use super::*;

#[tokio::test]
async fn native_content_process_recovers_after_storage_reader_lease_loss() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-content-reader-recovery.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&lock_path);
    let _ = fs::remove_file(&events_path);
    let _ = fs::remove_file(&readers_path);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<p>Content storage recovery</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let url = format!("http://{address}/page");
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(url),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    fs::remove_file(&readers_path).unwrap();
    engine
        .evaluate_async("localStorage.setItem('recovered', 'yes'); true")
        .await
        .unwrap();
    fs::remove_file(&readers_path).unwrap();
    assert_eq!(
        engine
            .evaluate_async("localStorage.getItem('recovered')")
            .await
            .unwrap(),
        serde_json::json!("yes")
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(lock_path);
    let _ = fs::remove_file(events_path);
    let _ = fs::remove_file(readers_path);
}
#[tokio::test]
async fn native_content_process_routes_session_storage_events_by_browsing_context() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-content-session-context-events.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let _ = fs::remove_file(&profile_path);
    let _ = fs::remove_file(&lock_path);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/page"));
            let body = "<p>Content session storage</p>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let url = format!("http://{address}/page");
    let owner_config = NativeEngineConfig::default()
        .with_context_id("network-tab-one")
        .with_storage_path(profile_path.clone())
        .with_initial_url(url.clone());
    let same_context_config = owner_config.clone();
    let other_context_config = owner_config.clone().with_context_id("network-tab-two");
    let mut owner = NativeEngine::new(owner_config).unwrap();
    let mut same_context = NativeEngine::new(same_context_config).unwrap();
    let mut other_context = NativeEngine::new(other_context_config).unwrap();
    owner.initialize_async().await.unwrap();
    same_context.initialize_async().await.unwrap();
    other_context.initialize_async().await.unwrap();

    for engine in [&mut owner, &mut same_context, &mut other_context] {
        engine
            .evaluate_async(
                "globalThis.storageEvents = []; addEventListener('storage', event => storageEvents.push({ key: event.key, oldValue: event.oldValue, newValue: event.newValue, url: event.url, session: event.storageArea === sessionStorage })); true",
            )
            .await
            .unwrap();
    }

    owner
        .evaluate_async("sessionStorage.setItem('tab', 'one'); true")
        .await
        .unwrap();
    assert_eq!(
        same_context
            .evaluate_async(
                "({ events: storageEvents.splice(0), value: sessionStorage.getItem('tab') })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "events": [{
                "key": "tab",
                "oldValue": null,
                "newValue": "one",
                "url": url,
                "session": true,
            }],
            "value": "one",
        })
    );
    assert_eq!(
        other_context
            .evaluate_async(
                "({ events: storageEvents.splice(0), value: sessionStorage.getItem('tab') })",
            )
            .await
            .unwrap(),
        serde_json::json!({"events": [], "value": null})
    );
    assert_eq!(
        owner.evaluate_async("storageEvents").await.unwrap(),
        serde_json::json!([])
    );

    other_context.close_async().await.unwrap();
    same_context.close_async().await.unwrap();
    owner.close_async().await.unwrap();
    server.await.unwrap();
    let _ = fs::remove_file(profile_path);
    let _ = fs::remove_file(lock_path);
}
#[tokio::test]
async fn native_content_process_web_storage_persists_through_profile_restart() {
    let _guard = native_content_process_test_lock().lock().await;
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-web-storage-{}-content-restart.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&profile_path);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some("/page"));
            let body = "<p>Persistent content storage</p>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let url = format!("http://{address}/page");

    {
        let mut engine = NativeEngine::new(
            NativeEngineConfig::default()
                .with_storage_path(profile_path.clone())
                .with_initial_url(url.clone()),
        )
        .unwrap();
        engine.initialize_async().await.unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "localStorage.setItem('theme', 'dark'); sessionStorage.setItem('tab', 'one'); true",
                )
                .await
                .unwrap(),
            serde_json::json!(true)
        );
        engine.close_async().await.unwrap();
    }

    let profile = fs::read_to_string(&profile_path).unwrap();
    assert!(
        profile.contains("theme"),
        "profile after child close: {profile}"
    );

    {
        let mut engine = NativeEngine::new(
            NativeEngineConfig::default()
                .with_storage_path(profile_path.clone())
                .with_initial_url(url),
        )
        .unwrap();
        engine.initialize_async().await.unwrap();
        assert_eq!(
            engine
                .evaluate_async(
                    "({ local: localStorage.getItem('theme'), session: sessionStorage.getItem('tab') })",
                )
                .await
                .unwrap(),
            serde_json::json!({"local":"dark","session":null})
        );
        engine.close_async().await.unwrap();
    }

    server.await.unwrap();
    let _ = fs::remove_file(profile_path);
}

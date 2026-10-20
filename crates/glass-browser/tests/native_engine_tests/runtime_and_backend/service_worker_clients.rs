use super::*;

#[test]
fn native_runtime_service_worker_clients_match_all_across_targets() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_runtime_service_worker_clients_match_all_across_targets_inner());
    });
}

async fn native_runtime_service_worker_clients_match_all_across_targets_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-clients-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default();
                    let (content_type, body) = match path {
                        "/register" => (
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>register</main>",
                        ),
                        "/sw.js" => (
                            "application/javascript",
                            r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  const path = new URL(event.request.url).pathname;
  if (path === '/api') {
    event.respondWith(clients.matchAll({ type: 'window' }).then(items => new Response(JSON.stringify(items.map(client => ({
      id: client.id,
      url: client.url,
      type: client.type,
      frameType: client.frameType,
      focused: client.focused,
    }))), { status: 200, headers: { 'Content-Type': 'application/json' } })));
  } else {
    event.respondWith(new Response('<!doctype html><main>controlled target</main>', { headers: { 'Content-Type': 'text/html' } }));
  }
});"#,
                        ),
                        "/app/second" | "/app/first" => (
                            "text/html",
                            "<!doctype html><main>network target</main>",
                        ),
                        _ => ("text/plain", "unexpected network request"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/register")),
    )
    .await
    .unwrap();
    let second = session
        .native_create_target(&format!("http://{address}/app/second"))
        .await
        .unwrap();
    session.native_select_target(&second.id).await.unwrap();
    let clients = session
        .script("await fetch('/api').then(response => response.json())")
        .await
        .unwrap()
        .value;
    let mut clients = clients.as_array().unwrap().clone();
    clients.sort_by(|left, right| {
        left.get("url")
            .and_then(serde_json::Value::as_str)
            .cmp(&right.get("url").and_then(serde_json::Value::as_str))
    });
    assert_eq!(clients.len(), 2);
    assert_eq!(
        clients
            .iter()
            .map(|client| client.get("url").cloned().unwrap())
            .collect::<Vec<_>>(),
        vec![
            serde_json::json!(format!("http://{address}/app/second")),
            serde_json::json!(format!("http://{address}/register")),
        ]
    );
    assert_ne!(clients[0].get("id"), clients[1].get("id"));
    assert!(
        clients
            .iter()
            .all(|client| client.get("type") == Some(&serde_json::json!("window")))
    );
    assert!(
        clients
            .iter()
            .all(|client| client.get("frameType") == Some(&serde_json::json!("top-level")))
    );

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn native_runtime_shared_worker_matches_constructor_identity() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-shared-worker-targets-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => accepted.unwrap(),
            };
            let (mut stream, _) = accepted;
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (status, content_type, body) = match path {
                "/first" => (
                    "200 OK",
                    "text/html",
                    r#"<script>
globalThis.pageId = 'first';
globalThis.sharedEvents = [];
globalThis.sharedWorker = new SharedWorker('/shared-target.js', { name: 'glass-cross-target' });
sharedWorker.port.onmessage = event => sharedEvents.push(event.data);
sharedWorker.port.start();
sharedWorker.port.postMessage({ kind: 'connect', pageId });
</script><main>first target</main>"#,
                ),
                "/second" => (
                    "200 OK",
                    "text/html",
                    r#"<script>
globalThis.pageId = 'second';
globalThis.sharedEvents = [];
globalThis.sharedWorker = new SharedWorker('/shared-target.js', { name: 'glass-cross-target' });
sharedWorker.port.onmessage = event => sharedEvents.push(event.data);
sharedWorker.port.start();
sharedWorker.port.postMessage({ kind: 'connect', pageId });
</script><main>second target</main>"#,
                ),
                "/shared-target.js" | "/shared-target-alt.js" => (
                    "200 OK",
                    "text/javascript",
                    r#"
let connectionCount = 0;
const runtimeId = Date.now().toString() + '-' + Math.random().toString(36).slice(2);
const ports = new Map();
onconnect = event => {
  const port = event.ports[0];
  const connection = ++connectionCount;
  let pageId = null;
  port.addEventListener('message', message => {
    if (message.data.kind === 'connect') {
      pageId = message.data.pageId;
      ports.set(pageId, port);
      port.postMessage({ kind: 'ready', pageId, connection, runtimeId });
    } else if (message.data.kind === 'status') {
      port.postMessage({ kind: 'status', pageId, connections: connectionCount, runtimeId });
    } else if (message.data.kind === 'relay') {
      const destination = ports.get(message.data.target);
      if (destination) {
        destination.postMessage({
          kind: 'relayed', from: pageId, payload: message.data.payload,
        });
        port.postMessage({ kind: 'relay-ack', pageId, target: message.data.target });
      } else {
        port.postMessage({ kind: 'relay-missing', pageId, target: message.data.target });
      }
    }
  });
  port.start();
};"#,
                ),
                _ => ("404 Not Found", "text/plain", "unexpected request"),
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/first")),
    )
    .await
    .unwrap();
    let first_ready = session
        .script("sharedEvents.filter(message => message.kind === 'ready')")
        .await
        .unwrap()
        .value;
    assert_eq!(first_ready.as_array().unwrap().len(), 1);
    assert_eq!(first_ready[0]["pageId"], "first");
    assert_eq!(first_ready[0]["connection"], 1);

    let second = session
        .native_create_target(&format!("http://{address}/second"))
        .await
        .unwrap();
    session.native_select_target(&second.id).await.unwrap();
    let second_ready = session
        .script("sharedEvents.filter(message => message.kind === 'ready')")
        .await
        .unwrap()
        .value;
    assert_eq!(second_ready.as_array().unwrap().len(), 1);
    assert_eq!(second_ready[0]["pageId"], "second");
    assert_eq!(
        second_ready[0]["connection"], 2,
        "same-origin targets must connect to the same SharedWorker global"
    );
    assert_eq!(second_ready[0]["runtimeId"], first_ready[0]["runtimeId"]);

    session
        .script(
            r#"
globalThis.sharedWorkerErrors = [];
for (const [label, options] of [
  ['type', { name: 'glass-cross-target', type: 'module' }],
  ['credentials', { name: 'glass-cross-target', credentials: 'include' }],
  ['extendedLifetime', { name: 'glass-cross-target', extendedLifetime: true }],
]) {
  const candidate = new SharedWorker('/shared-target.js', options);
  candidate.addEventListener('error', () => sharedWorkerErrors.push(label));
}
globalThis.invalidSharedWorkerOptions = [];
for (const options of [
  { name: 'invalid-type', type: 'MODULE' },
  { name: 'invalid-credentials', credentials: 'sameOrigin' },
]) {
  try {
    new SharedWorker('/shared-target.js', options);
  } catch (error) {
    invalidSharedWorkerOptions.push(error.name);
  }
}
true"#,
        )
        .await
        .unwrap();
    let mut mismatch_errors = wait_for_shared_worker_messages(&session, "sharedWorkerErrors").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if mismatch_errors
                .as_array()
                .is_some_and(|events| events.len() == 3)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            mismatch_errors = session.script("sharedWorkerErrors").await.unwrap().value;
        }
    })
    .await
    .expect("all incompatible SharedWorker options must dispatch error");
    assert_eq!(
        mismatch_errors,
        serde_json::json!(["type", "credentials", "extendedLifetime"])
    );
    assert_eq!(
        session
            .script("invalidSharedWorkerOptions")
            .await
            .unwrap()
            .value,
        serde_json::json!(["TypeError", "TypeError"])
    );
    session
        .script("sharedWorker.port.postMessage({ kind: 'status' }); true")
        .await
        .unwrap();
    let status = wait_for_shared_worker_messages(
        &session,
        "sharedEvents.filter(message => message.kind === 'status')",
    )
    .await;
    assert_eq!(status[0]["connections"], 2);
    assert_eq!(status[0]["runtimeId"], first_ready[0]["runtimeId"]);

    session
        .script(
            r#"
globalThis.independentWorkerEvents = [];
globalThis.independentWorkerByName = new SharedWorker('/shared-target.js', {
  name: 'glass-cross-target-other-name',
});
independentWorkerByName.port.addEventListener('message', event => independentWorkerEvents.push(event.data));
independentWorkerByName.port.start();
independentWorkerByName.port.postMessage({ kind: 'connect', pageId: 'different-name' });
true"#,
        )
        .await
        .unwrap();
    let independent_by_name = wait_for_shared_worker_messages(
        &session,
        "independentWorkerEvents.filter(message => message.pageId === 'different-name')",
    )
    .await;
    assert_ne!(
        independent_by_name[0]["runtimeId"],
        first_ready[0]["runtimeId"]
    );

    session
        .script(
            r#"
globalThis.independentWorkerByUrl = new SharedWorker('/shared-target-alt.js', {
  name: 'glass-cross-target',
});
independentWorkerByUrl.port.addEventListener('message', event => independentWorkerEvents.push(event.data));
independentWorkerByUrl.port.start();
independentWorkerByUrl.port.postMessage({ kind: 'connect', pageId: 'different-url' });
true"#,
        )
        .await
        .unwrap();
    let independent_by_url = wait_for_shared_worker_messages(
        &session,
        "independentWorkerEvents.filter(message => message.pageId === 'different-url')",
    )
    .await;
    assert_ne!(
        independent_by_url[0]["runtimeId"],
        first_ready[0]["runtimeId"]
    );
    assert_ne!(
        independent_by_url[0]["runtimeId"],
        independent_by_name[0]["runtimeId"]
    );

    assert_eq!(
        session
            .script("sharedWorker.port.postMessage({ kind: 'relay', target: 'first', payload: 'from-second' }); true")
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
    );
    let second_ack = session
        .script("sharedEvents.filter(message => message.kind === 'relay-ack')")
        .await
        .unwrap()
        .value;
    assert_eq!(second_ack[0]["target"], "first");

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    let first_relay = session
        .script("sharedEvents.filter(message => message.kind === 'relayed')")
        .await
        .unwrap()
        .value;
    assert_eq!(
        first_relay,
        serde_json::json!([{
            "kind": "relayed",
            "from": "second",
            "payload": "from-second",
        }])
    );

    session
        .script("sharedWorker.port.postMessage({ kind: 'relay', target: 'second', payload: 'from-first' }); true")
        .await
        .unwrap();
    session.native_select_target(&second.id).await.unwrap();
    let second_relay = session
        .script("sharedEvents.filter(message => message.kind === 'relayed')")
        .await
        .unwrap()
        .value;
    assert_eq!(
        second_relay,
        serde_json::json!([{
            "kind": "relayed",
            "from": "first",
            "payload": "from-first",
        }])
    );

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
        });
    });
}

#[test]
fn native_runtime_shared_worker_target_teardown_preserves_survivor_and_reaps_last_client() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(run_shared_worker_target_teardown());
    });
}

#[test]
fn native_runtime_shared_worker_document_navigation_retires_only_replaced_owners() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(run_shared_worker_document_navigation_teardown());
    });
}

#[test]
fn native_runtime_live_iframe_removal_retires_only_continuously_detached_owners() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(run_live_iframe_removal_lifecycle());
    });
}

#[test]
fn native_runtime_service_worker_client_leases_cross_sessions_and_close() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_runtime_service_worker_client_leases_cross_sessions_and_close_inner());
    });
}

async fn native_runtime_service_worker_client_leases_cross_sessions_and_close_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-client-leases-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    let clients_path = profile_path.with_extension("clients");
    for path in [
        &profile_path,
        &lock_path,
        &events_path,
        &readers_path,
        &clients_path,
    ] {
        let _ = fs::remove_file(path);
    }
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default();
                    let (content_type, body) = match path {
                        "/register" => (
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>network client</main>",
                        ),
                        "/two" => ("text/html", "<!doctype html><main>network client</main>"),
                        "/sw.js" => (
                            "application/javascript",
                            r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/api') {
    event.respondWith(clients.matchAll({ type: 'window', includeUncontrolled: true }).then(items => new Response(JSON.stringify(items.map(client => ({
      id: client.id,
      url: client.url,
    }))), { headers: { 'Content-Type': 'application/json' } })));
  }
});"#,
                        ),
                        "/api" => ("text/plain", "unexpected network API request"),
                        _ => ("text/plain", "unexpected native client lease request"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
    });

    let first = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_context_id("lease-one")
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/register")),
    )
    .await
    .unwrap();
    assert_eq!(
        first
            .script("await registrationPromise.then(reg => reg.active.state)")
            .await
            .unwrap()
            .value,
        serde_json::json!("activated")
    );

    let second = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_context_id("lease-two")
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/two")),
    )
    .await
    .unwrap();
    let clients = second
        .script("await fetch('/api').then(response => response.json())")
        .await
        .unwrap()
        .value;
    let mut clients = clients.as_array().unwrap().clone();
    clients.sort_by(|left, right| {
        left.get("url")
            .and_then(serde_json::Value::as_str)
            .cmp(&right.get("url").and_then(serde_json::Value::as_str))
    });
    assert_eq!(clients.len(), 2);
    assert_eq!(
        clients
            .iter()
            .map(|client| client.get("url").cloned().unwrap())
            .collect::<Vec<_>>(),
        vec![
            serde_json::json!(format!("http://{address}/register")),
            serde_json::json!(format!("http://{address}/two")),
        ]
    );
    assert_ne!(clients[0].get("id"), clients[1].get("id"));

    first.close().await.unwrap();
    let clients = second
        .script("await fetch('/api').then(response => response.json())")
        .await
        .unwrap()
        .value;
    assert_eq!(clients.as_array().unwrap().len(), 1);
    assert_eq!(
        clients[0].get("url"),
        Some(&serde_json::json!(format!("http://{address}/two")))
    );

    second.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    for path in [
        &profile_path,
        &lock_path,
        &events_path,
        &readers_path,
        &clients_path,
    ] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn native_runtime_propagates_service_worker_registration_to_running_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_runtime_propagates_service_worker_registration_to_running_target_inner(),
        );
    });
}

async fn native_runtime_propagates_service_worker_registration_to_running_target_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-registration-sync-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default();
                    let (content_type, body) = match path {
                        "/idle" => ("text/html", "<!doctype html><main>idle</main>"),
                        "/register" => (
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>register</main>",
                        ),
                        "/sw.js" => (
                            "application/javascript",
                            r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/controlled') {
    event.respondWith(new Response('<!doctype html><html><body><main id="service-worker">native worker</main></body></html>', {
      headers: { 'Content-Type': 'text/html' },
    }));
  }
});"#,
                        ),
                        "/controlled" => (
                            "text/html",
                            "<!doctype html><html><body><main>network fallback</main></body></html>",
                        ),
                        "/after-unregister" => (
                            "text/html",
                            "<!doctype html><html><body><main>network fallback after unregister</main></body></html>",
                        ),
                        _ => ("text/plain", "unexpected native registration sync request"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/idle")),
    )
    .await
    .unwrap();
    let registration_target = session
        .native_create_target(&format!("http://{address}/register"))
        .await
        .unwrap();
    session
        .native_select_target(&registration_target.id)
        .await
        .unwrap();
    assert_eq!(
        session
            .script("await registrationPromise.then(reg => [reg.active.state, reg.scope])")
            .await
            .unwrap()
            .value,
        serde_json::json!(["activated", format!("http://{address}/")])
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session
            .script(
                "await navigator.serviceWorker.getRegistration('/controlled').then(reg => reg && [reg.scope, reg.active.state])",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([format!("http://{address}/"), "activated"])
    );
    assert_eq!(
        session
            .navigate(format!("http://{address}/controlled"))
            .await
            .unwrap()
            .url,
        format!("http://{address}/controlled")
    );
    assert_eq!(
        session
            .script("document.body.innerText")
            .await
            .unwrap()
            .value,
        serde_json::json!("native worker")
    );

    session
        .native_select_target(&registration_target.id)
        .await
        .unwrap();
    assert_eq!(
        session
            .script(
                "await navigator.serviceWorker.getRegistration('/controlled').then(reg => reg.unregister())",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    session
        .navigate(format!("http://{address}/after-unregister"))
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.body.innerText")
            .await
            .unwrap()
            .value,
        serde_json::json!("network fallback after unregister")
    );

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn native_runtime_service_worker_clients_open_window_materializes_window_client() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_runtime_service_worker_clients_open_window_materializes_window_client_inner(),
        );
    });
}

async fn native_runtime_service_worker_clients_open_window_materializes_window_client_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default();
                    let (content_type, body) = match path {
                        "/register" => (
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>openWindow</main>",
                        ),
                        "/sw.js" => (
                            "application/javascript",
                            r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('message', async event => {
  const client = await clients.openWindow('/opened');
  self.openedClient = client && {
    id: client.id,
    url: client.url,
    type: client.type,
    frameType: client.frameType,
    visibilityState: client.visibilityState,
    focused: client.focused,
  };
  client?.postMessage({ kind: 'opened-target', value: 42 });
});
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/inspect') {
    event.respondWith(new Response(JSON.stringify(self.openedClient || null), {
      headers: { 'Content-Type': 'application/json' },
    }));
  }
});"#,
                        ),
                        "/opened" => (
                            "text/html",
                            "<!doctype html><script>globalThis.received = []; navigator.serviceWorker.addEventListener('message', event => globalThis.received.push(event.data));</script><main>opened window</main>",
                        ),
                        _ => ("text/plain", "unexpected native openWindow request"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/register")),
    )
    .await
    .unwrap();
    assert_eq!(
        session
            .script("await registrationPromise.then(reg => [reg.active.state, navigator.serviceWorker.controller !== null])")
            .await
            .unwrap()
            .value,
        serde_json::json!(["activated", true])
    );
    assert_eq!(
        session
            .script("navigator.serviceWorker.controller.postMessage({ kind: 'open' }); true")
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
    );

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].id, "native-context");
    assert!(targets[0].active);
    assert_eq!(targets[1].url, format!("http://{address}/opened"));
    assert_eq!(targets[1].opener_id.as_deref(), Some("native-context"));
    assert!(!targets[1].active);
    let client = session
        .script("await fetch('/inspect').then(response => response.json())")
        .await
        .unwrap()
        .value;
    assert!(
        client
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
    assert_eq!(
        client.get("url"),
        Some(&serde_json::json!(format!("http://{address}/opened")))
    );
    assert_eq!(client.get("type"), Some(&serde_json::json!("window")));
    assert_eq!(
        client.get("frameType"),
        Some(&serde_json::json!("top-level"))
    );
    assert_eq!(
        client.get("visibilityState"),
        Some(&serde_json::json!("visible"))
    );
    assert_eq!(client.get("focused"), Some(&serde_json::json!(false)));

    let opened_target = targets
        .iter()
        .find(|target| target.url == format!("http://{address}/opened"))
        .unwrap();
    session
        .native_select_target(&opened_target.id)
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([{"kind": "opened-target", "value": 42}])
    );

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
}

#[test]
fn native_runtime_service_worker_fetch_open_window_resumes_navigation() {
    std::thread::Builder::new()
        .name("glass-sw-navigation-resume-test".into())
        // The process-backed browser navigation future crosses several engine
        // and IPC layers; avoid depending on the test harness's 2 MiB stack.
        .stack_size(4 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("service worker resume test runtime should build");
            runtime.block_on(
                native_runtime_service_worker_fetch_open_window_resumes_navigation_inner(),
            );
        })
        .expect("service worker resume test thread should start")
        .join()
        .expect("service worker resume test thread should complete");
}

#[test]
fn native_runtime_restores_initial_service_worker_fetch_suspension() {
    // This start path includes nested Service Worker fetch IPC. Match the
    // documented resident-browser worker stack used by other process tests.
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-service-worker-initial-suspension-{}.json",
        std::process::id()
    ));
    let lock_path = profile_path.with_extension("lock");
    let events_path = profile_path.with_extension("events");
    let readers_path = profile_path.with_extension("readers");
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
    let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
    let service_worker = r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/suspended') {
    event.respondWith((async () => {
      const client = await clients.openWindow('/opened');
      return new Response('<!doctype html><html><body><main id="initial-resumed">' + (client ? client.url : 'missing') + '</main></body></html>', {
        headers: { 'Content-Type': 'text/html' },
      });
    })());
  }
});"#;
    let server = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_receiver => break,
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    let request = read_http_request(&mut stream).await;
                    let path = request.split_whitespace().nth(1).unwrap_or_default();
                    let (content_type, body) = match path {
                        "/register" => (
                            "text/html",
                            "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>register</main>",
                        ),
                        "/sw.js" => ("application/javascript", service_worker),
                        "/opened" => (
                            "text/html",
                            "<!doctype html><html><body><main>opened window</main></body></html>",
                        ),
                        "/suspended" => ("text/plain", "network fallback"),
                        _ => ("text/plain", "unexpected request"),
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            }
        }
    });

    let first = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/register"))
            .with_storage_path(profile_path.clone()),
    )
    .await
    .unwrap();
    assert_eq!(
        first
            .script(
                "await registrationPromise.then(reg => [reg.active.state, reg.active.scriptURL])"
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["activated", format!("http://{address}/sw.js")])
    );
    first.close().await.unwrap();

    let reopened = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/suspended"))
            .with_storage_path(profile_path.clone()),
    )
    .await
    .unwrap();
    assert_eq!(
        reopened
            .script("({ href: location.href, body: document.body && document.body.innerText })")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "href": format!("http://{address}/suspended"),
            "body": format!("http://{address}/opened"),
        })
    );
    let targets = reopened.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .any(|target| { target.url == format!("http://{address}/opened") && !target.active })
    );
    reopened.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();

    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
        });
    });
}

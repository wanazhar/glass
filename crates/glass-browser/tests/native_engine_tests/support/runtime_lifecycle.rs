use super::*;

pub(super) async fn native_runtime_supports_form_pdf_clipboard_and_consent_surfaces_inner() {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://native-surfaces",
                "<form><label for='name'>Name</label><input id='name' type='text'><label><input id='remember' type='checkbox'>Remember</label><select id='country'><option value='uk'>United Kingdom</option><option value='us'>United States</option></select></form><div id='onetrust-banner-sdk'><button id='onetrust-accept-btn-handler'>Accept</button></div>",
            )
            .unwrap()
            .with_initial_url("fixture://native-surfaces"),
    )
    .await
    .unwrap();
    let revision = session.native_observe().await.unwrap().revision;
    let fields = [
        ("id=name", "Ada"),
        ("id=remember", "true"),
        ("id=country", "us"),
    ];
    let outcome = session
        .native_fill_form(&fields, Some(revision))
        .await
        .unwrap();
    assert_eq!(outcome.filled, 3);
    assert_eq!(outcome.total, 3);
    assert!(outcome.fields.iter().all(|field| field.success));
    let values = session
        .script(
            "({ name: document.getElementById('name').value, remember: document.getElementById('remember').checked, country: document.getElementById('country').value })",
        )
        .await
        .unwrap()
        .value;
    assert_eq!(values["name"], "Ada");
    assert_eq!(values["remember"], true);
    assert_eq!(values["country"], "us");

    let pdf = session
        .native_print_to_pdf(&glass_browser::browser::session::PdfOptions::letter())
        .await
        .unwrap();
    let pdf = base64::engine::general_purpose::STANDARD
        .decode(pdf)
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert!(
        pdf.windows(b"United States".len())
            .any(|window| window == b"United States")
    );

    let capture = session.capture(CaptureFormat::Pdf).await.unwrap();
    assert_eq!(capture.format, CaptureFormat::Pdf);
    assert!(capture.bytes.starts_with(b"%PDF-1.4"));
    let capture = session.capture(CaptureFormat::Png).await.unwrap();
    assert_eq!(capture.format, CaptureFormat::Png);
    assert!(capture.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let before_recovery = session.native_observe().await.unwrap();
    let recovery = session
        .native_recover(Some(before_recovery.revision))
        .await
        .unwrap();
    assert_eq!(recovery.action, "recover");
    assert_eq!(recovery.previous_revision, before_recovery.revision);
    assert!(recovery.current_revision > recovery.previous_revision);
    assert_eq!(
        session.native_observe().await.unwrap().page.url,
        before_recovery.page.url
    );

    session
        .native_clipboard_write("native clipboard")
        .await
        .unwrap();
    assert_eq!(
        session.native_clipboard_read().await.unwrap(),
        "native clipboard"
    );

    let consent = session.native_dismiss_consent().await.unwrap();
    assert!(matches!(
        consent,
        glass_browser::browser::session::ConsentDismissalOutcome::Dismissed
    ));
    session.close().await.unwrap();
}

pub(super) async fn run_shared_worker_target_teardown() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-shared-worker-teardown-{}.json",
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
                "/first" | "/second" | "/third" => {
                    let page_id = match path {
                        "/first" => "first",
                        "/second" => "second",
                        _ => "third",
                    };
                    let body = format!(
                        r#"<script>
globalThis.pageId = {page_id:?};
globalThis.sharedMessages = [];
globalThis.closeEvents = [];
globalThis.sharedWorker = new SharedWorker('/shared-lifecycle.js', {{ name: 'glass-target-lifecycle' }});
sharedWorker.port.onmessage = event => sharedMessages.push(event.data);
sharedWorker.port.start();
sharedWorker.port.postMessage({{ kind: 'connect', pageId }});
</script><main>{page_id} target</main>"#
                    );
                    ("200 OK", "text/html", body)
                }
                "/shared-lifecycle.js" => (
                    "200 OK",
                    "text/javascript",
                    r#"
let connectionCount = 0;
const runtimeId = Date.now().toString() + '-' + Math.random().toString(36).slice(2);
const ports = new Map();
const closedConnections = [];
onconnect = event => {
  const port = event.ports[0];
  const connection = ++connectionCount;
  let pageId = null;
  port.addEventListener('message', message => {
    if (message.data.kind === 'connect') {
      pageId = message.data.pageId;
      ports.set(pageId, port);
      port.postMessage({ kind: 'ready', pageId, connection, runtimeId });
    } else if (message.data.kind === 'ping') {
      port.postMessage({
        kind: 'pong', pageId, connection, runtimeId,
        closedConnections: [...closedConnections],
      });
    } else if (message.data.kind === 'close-self') {
      port.close();
    } else if (message.data.kind === 'arm-close') {
      port.onclose = () => {
        closedConnections.push(pageId);
        ports.delete(pageId);
        for (const otherPort of ports.values()) {
          otherPort.postMessage({ kind: 'peer-closed', closed: pageId, runtimeId });
        }
      };
    }
  });
  port.start();
};"#
                    .to_owned(),
                ),
                "/sentinel" => (
                    "200 OK",
                    "text/html",
                    "<main>sentinel target</main>".to_owned(),
                ),
                _ => (
                    "404 Not Found",
                    "text/plain",
                    "unexpected request".to_owned(),
                ),
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
    const ARM_SHARED_WORKER_CLOSE_SCRIPT: &str = r#"
sharedWorker.port.addEventListener('close', event => closeEvents.push({
  type: event.type, target: event.target === sharedWorker.port,
}));
sharedWorker.port.postMessage({ kind: 'arm-close' });
true"#;
    let first_ready = session
        .script("sharedMessages.filter(message => message.kind === 'ready')")
        .await
        .unwrap()
        .value;
    assert_eq!(first_ready[0]["connection"], 1);
    let original_runtime = first_ready[0]["runtimeId"].clone();
    session
        .script(ARM_SHARED_WORKER_CLOSE_SCRIPT)
        .await
        .unwrap();

    let second = session
        .native_create_target(&format!("http://{address}/second"))
        .await
        .unwrap();
    session.native_select_target(&second.id).await.unwrap();
    let second_ready = session
        .script("sharedMessages.filter(message => message.kind === 'ready')")
        .await
        .unwrap()
        .value;
    assert_eq!(second_ready[0]["connection"], 2);
    assert_eq!(second_ready[0]["runtimeId"], original_runtime);
    session
        .script(ARM_SHARED_WORKER_CLOSE_SCRIPT)
        .await
        .unwrap();

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    session
        .script("sharedWorker.port.close(); true")
        .await
        .unwrap();
    let first_close_events = session.script("closeEvents").await.unwrap().value;
    assert_eq!(first_close_events, serde_json::json!([]));

    session.native_select_target(&second.id).await.unwrap();
    let peer_close = session
        .script("sharedMessages.filter(message => message.kind === 'peer-closed')")
        .await
        .unwrap()
        .value;
    assert_eq!(peer_close[0]["closed"], "first");
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let pong = session
        .script("sharedMessages.filter(message => message.kind === 'pong')")
        .await
        .unwrap()
        .value;
    assert_eq!(pong[0]["connection"], 2);
    assert_eq!(pong[0]["runtimeId"], original_runtime);
    assert_eq!(pong[0]["closedConnections"], serde_json::json!(["first"]));

    session
        .script("sharedWorker.port.postMessage({ kind: 'close-self' }); true")
        .await
        .unwrap();
    let second_close_events = session.script("closeEvents").await.unwrap().value;
    assert_eq!(second_close_events.as_array().unwrap().len(), 1);
    assert_eq!(second_close_events[0]["type"], "close");
    assert_eq!(second_close_events[0]["target"], true);
    let second_messages = session.script("sharedMessages").await.unwrap().value;
    assert!(
        second_messages
            .as_array()
            .unwrap()
            .iter()
            .all(|message| !message.is_null())
    );

    session
        .script(
            "globalThis.secondExtraWorker = new SharedWorker('/shared-lifecycle.js', { name: 'glass-target-lifecycle' }); secondExtraWorker.port.onmessage = event => sharedMessages.push(event.data); secondExtraWorker.port.start(); secondExtraWorker.port.postMessage({ kind: 'connect', pageId: 'second-extra' }); true",
        )
        .await
        .unwrap();
    let second_extra_ready = session
        .script("sharedMessages.filter(message => message.pageId === 'second-extra')")
        .await
        .unwrap()
        .value;
    assert_eq!(second_extra_ready[0]["connection"], 3);
    assert_eq!(second_extra_ready[0]["runtimeId"], original_runtime);
    session
        .script("secondExtraWorker.port.postMessage({ kind: 'arm-close' }); true")
        .await
        .unwrap();

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    session
        .script(
            "globalThis.reconnectedWorker = new SharedWorker('/shared-lifecycle.js', { name: 'glass-target-lifecycle' }); reconnectedWorker.port.addEventListener('message', event => sharedMessages.push(event.data)); reconnectedWorker.port.start(); reconnectedWorker.port.postMessage({ kind: 'connect', pageId: 'first-reconnected' }); true",
        )
        .await
        .unwrap();
    let first_reconnect = session
        .script("sharedMessages.filter(message => message.pageId === 'first-reconnected')")
        .await
        .unwrap()
        .value;
    assert_eq!(first_reconnect[0]["connection"], 4);
    assert_eq!(first_reconnect[0]["runtimeId"], original_runtime);

    session.native_close_target(&second.id).await.unwrap();
    session
        .script("reconnectedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let survivor_pong = session
        .script("sharedMessages.filter(message => message.kind === 'pong')")
        .await
        .unwrap()
        .value;
    let survivor_pong = survivor_pong.as_array().unwrap().last().unwrap();
    assert_eq!(survivor_pong["connection"], 4);
    assert_eq!(survivor_pong["runtimeId"], original_runtime);
    assert_eq!(
        survivor_pong["closedConnections"],
        serde_json::json!(["first", "second-extra"])
    );

    let sentinel = session
        .native_create_target(&format!("http://{address}/sentinel"))
        .await
        .unwrap();
    session.native_select_target(&sentinel.id).await.unwrap();
    session.native_close_target("native-context").await.unwrap();
    let third = session
        .native_create_target(&format!("http://{address}/third"))
        .await
        .unwrap();
    session.native_select_target(&third.id).await.unwrap();
    let third_ready = session
        .script("sharedMessages.filter(message => message.kind === 'ready')")
        .await
        .unwrap()
        .value;
    assert_eq!(third_ready[0]["connection"], 1);
    assert_ne!(third_ready[0]["runtimeId"], original_runtime);

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}

pub(super) async fn run_shared_worker_document_navigation_teardown() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-shared-worker-document-lifecycle-{}.json",
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
            let page = |page_id: &str| {
                format!(
                    r#"<script>
globalThis.pageId = {page_id:?};
globalThis.sharedMessages = [];
globalThis.closeEvents = [];
globalThis.sharedWorker = new SharedWorker('/shared-document.js', {{ name: 'glass-document-owner' }});
sharedWorker.port.addEventListener('message', event => sharedMessages.push(event.data));
sharedWorker.port.addEventListener('close', event => closeEvents.push(event.type));
sharedWorker.port.start();
sharedWorker.port.postMessage({{ kind: 'connect', pageId }});
</script><main>{page_id}</main>"#
                )
            };
            let (status, content_type, body) = match path {
                "/parent" => (
                    "200 OK",
                    "text/html",
                    "<main>parent<iframe src='/child'></iframe></main>".to_owned(),
                ),
                "/child" => ("200 OK", "text/html", page("child")),
                "/child-next" => ("200 OK", "text/html", page("child-next")),
                "/sibling" => ("200 OK", "text/html", page("sibling")),
                "/root-next" => ("200 OK", "text/html", page("root-next")),
                "/shared-document.js" => (
                    "200 OK",
                    "text/javascript",
                    r#"
let connectionCount = 0;
const runtimeId = Date.now().toString() + '-' + Math.random().toString(36).slice(2);
const ports = new Map();
const closedPages = [];
onconnect = event => {
  const port = event.ports[0];
  const connection = ++connectionCount;
  let pageId = null;
  port.addEventListener('message', message => {
    if (message.data.kind === 'connect') {
      pageId = message.data.pageId;
      ports.set(pageId, port);
      port.postMessage({ kind: 'ready', pageId, connection, runtimeId });
    } else if (message.data.kind === 'ping') {
      port.postMessage({ kind: 'pong', pageId, connection, runtimeId, closedPages: [...closedPages] });
    }
  });
  port.addEventListener('close', () => {
    closedPages.push(pageId);
    ports.delete(pageId);
    for (const otherPort of ports.values()) {
      otherPort.postMessage({ kind: 'peer-closed', pageId, runtimeId });
    }
  });
  port.start();
};"#
                        .to_owned(),
                ),
                _ => ("404 Not Found", "text/plain", "unexpected request".to_owned()),
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserSession::start(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let frames = session.list_frames().await.unwrap();
    let root_frame_id = frames[0].id.clone();
    let child_frame_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some(root_frame_id.as_str()))
        .unwrap()
        .id
        .clone();
    session.select_frame(&child_frame_id).await.unwrap();
    let child_ready = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'ready')",
    )
    .await;
    assert_eq!(child_ready.as_array().map(Vec::len), Some(1));
    assert_eq!(child_ready[0]["connection"], 1);
    let original_runtime = child_ready[0]["runtimeId"].clone();

    let sibling = session
        .create_target(&format!("http://{address}/sibling"))
        .await
        .unwrap();
    session.select_target(&sibling.id).await.unwrap();
    let sibling_ready = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'ready')",
    )
    .await;
    assert_eq!(sibling_ready[0]["connection"], 2);
    assert_eq!(sibling_ready[0]["runtimeId"], original_runtime);

    session.select_target("native-context").await.unwrap();
    session.select_frame(&child_frame_id).await.unwrap();
    session
        .navigate(&format!("http://{address}/child#same-document"))
        .await
        .unwrap();
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let same_document_pong = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'pong')",
    )
    .await;
    assert_eq!(same_document_pong[0]["runtimeId"], original_runtime);
    assert_eq!(same_document_pong[0]["closedPages"], serde_json::json!([]));

    session
        .navigate(&format!("http://{address}/child-next"))
        .await
        .unwrap();
    let replacement_child_ready = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'ready')",
    )
    .await;
    let replacement_child_ready = replacement_child_ready.as_array().unwrap().last().unwrap();
    assert_eq!(replacement_child_ready["pageId"], "child-next");
    assert_eq!(replacement_child_ready["connection"], 3);
    assert_eq!(replacement_child_ready["runtimeId"], original_runtime);
    assert_eq!(
        session.script("closeEvents").await.unwrap().value,
        serde_json::json!([]),
        "teardown of the outgoing Document must not dispatch close into its replacement"
    );

    session.select_target(&sibling.id).await.unwrap();
    let child_closed = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'peer-closed')",
    )
    .await;
    assert_eq!(child_closed[0]["pageId"], "child");
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let sibling_pong = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'pong')",
    )
    .await;
    assert_eq!(
        sibling_pong.as_array().unwrap().last().unwrap()["runtimeId"],
        original_runtime
    );

    session.select_target("native-context").await.unwrap();
    let root_frame_id = session.list_frames().await.unwrap()[0].id.clone();
    session.select_frame(&root_frame_id).await.unwrap();
    session
        .navigate(&format!("http://{address}/root-next"))
        .await
        .unwrap();
    let replacement_root_ready = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'ready')",
    )
    .await;
    let replacement_root_ready = replacement_root_ready.as_array().unwrap().last().unwrap();
    assert_eq!(replacement_root_ready["pageId"], "root-next");
    assert_eq!(replacement_root_ready["connection"], 4);
    assert_eq!(replacement_root_ready["runtimeId"], original_runtime);
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let replacement_pong = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'pong')",
    )
    .await;
    assert_eq!(
        replacement_pong.as_array().unwrap().last().unwrap()["pageId"],
        "root-next"
    );
    assert_eq!(
        replacement_pong.as_array().unwrap().last().unwrap()["closedPages"],
        serde_json::json!(["child", "child-next"])
    );

    session.recover().await.unwrap();
    let recovered_root_ready = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'ready')",
    )
    .await;
    let recovered_root_ready = recovered_root_ready.as_array().unwrap().last().unwrap();
    assert_eq!(recovered_root_ready["pageId"], "root-next");
    assert_eq!(recovered_root_ready["connection"], 5);
    assert_eq!(recovered_root_ready["runtimeId"], original_runtime);

    session.select_target(&sibling.id).await.unwrap();
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let recovered_sibling_pong = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'pong' && message.closedPages.includes('root-next'))",
    )
    .await;
    let recovered_sibling_pong = recovered_sibling_pong.as_array().unwrap().last().unwrap();
    assert_eq!(recovered_sibling_pong["runtimeId"], original_runtime);
    assert_eq!(
        recovered_sibling_pong["closedPages"],
        serde_json::json!(["child", "child-next", "root-next"])
    );

    session.select_target("native-context").await.unwrap();
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let recovered_root_pong = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'pong')",
    )
    .await;
    let recovered_root_pong = recovered_root_pong.as_array().unwrap().last().unwrap();
    assert_eq!(recovered_root_pong["pageId"], "root-next");
    assert_eq!(recovered_root_pong["runtimeId"], original_runtime);
    assert_eq!(
        recovered_root_pong["closedPages"],
        serde_json::json!(["child", "child-next", "root-next"])
    );

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}

pub(super) async fn run_live_iframe_removal_lifecycle() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let profile_path = std::env::temp_dir().join(format!(
        "glass-native-live-iframe-removal-{}.json",
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
            let (content_type, body) = match path {
                "/parent" => (
                    "text/html",
                    r#"<main>
<iframe id="removed-frame" src="/removed"></iframe>
<iframe id="moved-frame" src="/moved"></iframe>
<iframe id="survivor-frame" src="/survivor"></iframe>
</main>"#
                        .to_owned(),
                ),
                "/removed" | "/moved" | "/survivor" => {
                    let page_id = path.trim_start_matches('/');
                    let nested_frame = if path == "/removed" {
                        "<iframe src=\"/nested\"></iframe>"
                    } else {
                        ""
                    };
                    (
                        "text/html",
                        format!(
                            r#"<script>
globalThis.pageId = {page_id:?};
globalThis.sharedMessages = [];
globalThis.sharedWorker = new SharedWorker('/live-iframe.js', {{ name: 'glass-live-iframe' }});
sharedWorker.port.addEventListener('message', event => sharedMessages.push(event.data));
sharedWorker.port.start();
sharedWorker.port.postMessage({{ kind: 'connect', pageId }});
addEventListener('unload', () => sharedWorker.port.postMessage({{ kind: 'unload', pageId }}));
</script>{nested_frame}"#
                        ),
                    )
                }
                "/nested" => ("text/html", "<main>nested frame</main>".to_owned()),
                "/live-iframe.js" => (
                    "text/javascript",
                    r#"
let connectionCount = 0;
const runtimeId = Date.now().toString() + '-' + Math.random().toString(36).slice(2);
const ports = new Map();
const closedPages = [];
const unloadPages = [];
onconnect = event => {
  const port = event.ports[0];
  const connection = ++connectionCount;
  let pageId = null;
  port.addEventListener('message', message => {
    if (message.data.kind === 'connect') {
      pageId = message.data.pageId;
      ports.set(pageId, port);
      port.postMessage({ kind: 'ready', pageId, connection, runtimeId });
    } else if (message.data.kind === 'unload') {
      unloadPages.push(pageId);
    } else if (message.data.kind === 'ping') {
      port.postMessage({ kind: 'pong', pageId, connection, runtimeId, closedPages: [...closedPages], unloadPages: [...unloadPages] });
    }
  });
  port.addEventListener('close', () => {
    closedPages.push(pageId);
    ports.delete(pageId);
    for (const otherPort of ports.values()) {
      otherPort.postMessage({ kind: 'peer-closed', pageId, runtimeId });
    }
  });
  port.start();
};"#
                    .to_owned(),
                ),
                _ => ("text/plain", "unexpected request".to_owned()),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserSession::start(
        NativeEngineConfig::default()
            .with_storage_path(profile_path.clone())
            .with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let initial_frames = session.list_frames().await.unwrap();
    let root_frame_id = initial_frames[0].id.clone();
    let direct_children = initial_frames
        .iter()
        .filter(|frame| frame.parent_id.as_deref() == Some(root_frame_id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(direct_children.len(), 3);
    let removed_frame_id = direct_children[0].id.clone();
    let moved_frame_id = direct_children[1].id.clone();
    let survivor_frame_id = direct_children[2].id.clone();
    let removed_descendant_id = initial_frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some(removed_frame_id.as_str()))
        .unwrap()
        .id
        .clone();

    let mut runtime_id = None;
    for (frame_id, page_id, expected_connection) in [
        (&removed_frame_id, "removed", 1),
        (&moved_frame_id, "moved", 2),
        (&survivor_frame_id, "survivor", 3),
    ] {
        session.select_frame(frame_id).await.unwrap();
        let ready = wait_for_shared_worker_messages(
            &session,
            "sharedMessages.filter(message => message.kind === 'ready')",
        )
        .await;
        let ready = ready.as_array().unwrap().last().unwrap();
        assert_eq!(ready["pageId"], page_id);
        assert_eq!(ready["connection"], expected_connection);
        if let Some(original_runtime) = &runtime_id {
            assert_eq!(&ready["runtimeId"], original_runtime);
        } else {
            runtime_id = Some(ready["runtimeId"].clone());
        }
    }

    session.select_frame(&root_frame_id).await.unwrap();
    session
        .script("document.getElementById('removed-frame').remove(); true")
        .await
        .unwrap();
    let after_removal = session.list_frames().await.unwrap();
    let after_removal_ids = after_removal
        .iter()
        .map(|frame| frame.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(!after_removal_ids.contains(removed_frame_id.as_str()));
    assert!(!after_removal_ids.contains(removed_descendant_id.as_str()));
    assert!(after_removal_ids.contains(moved_frame_id.as_str()));
    assert!(after_removal_ids.contains(survivor_frame_id.as_str()));

    session.select_frame(&survivor_frame_id).await.unwrap();
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let after_remove_pong = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'pong')",
    )
    .await;
    let after_remove_pong = after_remove_pong.as_array().unwrap().last().unwrap();
    assert_eq!(after_remove_pong["runtimeId"], runtime_id.unwrap());
    assert_eq!(
        after_remove_pong["closedPages"],
        serde_json::json!(["removed"])
    );
    assert_eq!(after_remove_pong["unloadPages"], serde_json::json!([]));

    session.select_frame(&root_frame_id).await.unwrap();
    session
        .script(
            "const owner = document.getElementById('moved-frame'); const parent = owner.parentNode; owner.remove(); parent.appendChild(owner); true",
        )
        .await
        .unwrap();
    let after_move = session.list_frames().await.unwrap();
    let new_moved_frame_id = after_move
        .iter()
        .find(|frame| {
            frame.parent_id.as_deref() == Some(root_frame_id.as_str())
                && frame.url.ends_with("/moved")
                && frame.id != moved_frame_id
        })
        .unwrap()
        .id
        .clone();
    assert!(!after_move.iter().any(|frame| frame.id == moved_frame_id));
    assert!(after_move.iter().any(|frame| frame.id == survivor_frame_id));
    assert_ne!(new_moved_frame_id, moved_frame_id);

    session.select_frame(&new_moved_frame_id).await.unwrap();
    let replacement_ready = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'ready')",
    )
    .await;
    let replacement_ready = replacement_ready.as_array().unwrap().last().unwrap();
    assert_eq!(replacement_ready["pageId"], "moved");
    assert_eq!(replacement_ready["connection"], 4);
    session.select_frame(&survivor_frame_id).await.unwrap();
    session
        .script("sharedWorker.port.postMessage({ kind: 'ping' }); true")
        .await
        .unwrap();
    let after_move_pong = wait_for_shared_worker_messages(
        &session,
        "sharedMessages.filter(message => message.kind === 'pong')",
    )
    .await;
    let after_move_pong = after_move_pong.as_array().unwrap().last().unwrap();
    assert_eq!(
        after_move_pong["closedPages"],
        serde_json::json!(["removed", "moved"])
    );
    assert_eq!(after_move_pong["unloadPages"], serde_json::json!([]));

    session.close().await.unwrap();
    let _ = shutdown_sender.send(());
    server.await.unwrap();
    for path in [&profile_path, &lock_path, &events_path, &readers_path] {
        let _ = fs::remove_file(path);
    }
}

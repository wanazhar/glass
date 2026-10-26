use super::super::dialog::{
    NativeDialogControlPlane, NativeDialogController, NativeDialogResolution,
};
use super::*;

type TestHttpRequest = (String, Option<String>, String, Option<String>, Vec<u8>);

fn initialized_engine() -> NativeEngine {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://preflight.test/index")
        .with_fixture(
            "fixture://preflight.test/index",
            r#"
                    <button id="save">Save</button>
                    <button id="hidden" hidden>Hidden</button>
                    <button id="disabled" disabled>Disabled</button>
                    <input id="readonly" readonly>
                    <button>Duplicate</button>
                    <button>Duplicate</button>
                "#,
        )
        .expect("preflight fixture must validate");
    let mut engine = NativeEngine::new(config).expect("native engine must construct");
    engine.initialize().expect("native engine must initialize");
    engine
}

#[tokio::test]
async fn page_metadata_reads_current_url_title_and_revision() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://metadata.test/index")
        .with_fixture(
            "fixture://metadata.test/index",
            "<title>Initial title</title><main>body</main>",
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let metadata = engine.page_metadata().unwrap();
    assert_eq!(metadata.url, "fixture://metadata.test/index");
    assert_eq!(metadata.title, "Initial title");
    assert_eq!(metadata.revision, engine.revision());
    assert_eq!(metadata.lifecycle, NativeLifecycleState::Running);

    engine
        .evaluate_async("document.title = 'Updated title'")
        .await
        .unwrap();
    assert_eq!(engine.page_metadata().unwrap().title, "Updated title");
}

#[test]
fn page_metadata_handles_empty_title() {
    let engine = initialized_engine();
    let metadata = engine.page_metadata().unwrap();
    assert_eq!(metadata.url, "fixture://preflight.test/index");
    assert!(metadata.title.is_empty());
    assert_eq!(metadata.revision, engine.revision());
    assert_eq!(metadata.lifecycle, NativeLifecycleState::Running);
}

#[tokio::test]
async fn explicit_scroll_target_moves_only_the_observed_nested_container() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://targeted-scroll.test/")
        .with_fixture(
            "fixture://targeted-scroll.test/",
            "<style>#feed{width:120px;height:32px;overflow-y:auto}#item{height:160px}</style><main><div id='feed'><div id='item'>content</div></div><div id='outside'>outside</div></main>",
        )
        .expect("targeted scroll fixture must validate");
    let mut engine = NativeEngine::new(config).expect("native engine must construct");
    engine
        .initialize_async()
        .await
        .expect("native engine must initialize");

    let feed = engine.resolve_target("id=feed").unwrap();
    let feed_reference = format!("ref=r{}:n{}", engine.revision(), feed.index());
    engine.resolve_target("id=outside").unwrap();
    let root_before = engine.scroll_offset();
    let scroll_top_before = engine
        .evaluate_async("document.getElementById('feed').scrollTop")
        .await
        .unwrap();
    assert_eq!(scroll_top_before, serde_json::json!(0));

    let outcome = engine
        .action_async(NativeAction::ScrollTarget {
            target: feed_reference.clone(),
            delta_x: 0,
            delta_y: 24,
        })
        .await
        .unwrap();

    assert!(outcome.accepted);
    assert_eq!(outcome.revision, 2);
    assert_eq!(engine.scroll_offset(), root_before);
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('feed').scrollTop")
            .await
            .unwrap(),
        serde_json::json!(24)
    );
    assert!(matches!(
        engine
            .action_async(NativeAction::ScrollTarget {
                target: feed_reference,
                delta_x: 0,
                delta_y: 24,
            })
            .await,
        Err(NativeEngineError::DetachedTarget)
    ));
    assert!(matches!(
        engine
            .action_async(NativeAction::ScrollTarget {
                target: "id=outside".into(),
                delta_x: 0,
                delta_y: 24,
            })
            .await,
        Err(NativeEngineError::TargetNotActionable { .. })
    ));
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn percentage_height_shell_keeps_long_article_in_document_extent() {
    let article = (0..48)
        .map(|index| format!("<p>Paragraph {index}: detailed documentation content.</p>"))
        .collect::<String>();
    let html = format!(
        "<style>html,body{{width:100%;height:100%}}body{{margin:0}}main{{height:100%;position:relative}}.container{{flex-direction:column;display:flex}}.page{{padding:32px}}</style><main><div class='container'><div class='page content'><h1>Overview</h1>{article}</div></div></main>"
    );
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://percentage-shell.test/")
        .with_fixture("fixture://percentage-shell.test/", html)
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let layout = engine.layout().unwrap();
    assert!(
        layout.content_height > layout.viewport.height,
        "long article extent {} should exceed viewport {}",
        layout.content_height,
        layout.viewport.height
    );
    assert!(
        engine
            .action_async(NativeAction::Scroll {
                delta_x: 0,
                delta_y: 120
            })
            .await
            .unwrap()
            .accepted
    );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn asynchronous_effect_turn_rejects_a_different_context_or_frame() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://async-effect-owner")
        .with_fixture("fixture://async-effect-owner", "<main>owner</main>")
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let revision = engine.revision();
    let expected_context_id = engine.config.context_id.clone();
    let expected_frame_id = engine.frame_id.clone();

    for (context_id, frame_id) in [
        ("other-context".to_owned(), expected_frame_id),
        (expected_context_id, "other-frame".to_owned()),
    ] {
        let error = engine
            .dispatch_async_effect_notification(&NativeContentAsyncEffectNotification {
                sequence: 1,
                context_id,
                frame_id,
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            NativeEngineError::WorkerFailure {
                kind: NativeWorkerFailureKind::Protocol,
                ..
            }
        ));
        assert_eq!(engine.revision(), revision);
    }

    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn asynchronous_effect_turn_runs_in_the_matching_page_owner() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://async-effect-turn")
        .with_fixture("fixture://async-effect-turn", "<main>owner</main>")
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let context_id = engine.config.context_id.clone();
    let frame_id = engine.frame_id.clone();

    let turn = engine
        .dispatch_async_effect_notification(&NativeContentAsyncEffectNotification {
            sequence: 7,
            context_id: context_id.clone(),
            frame_id: frame_id.clone(),
        })
        .await
        .unwrap();

    assert_eq!(turn.sequence, 7);
    assert_eq!(turn.context_id, context_id);
    assert_eq!(turn.frame_id, frame_id);
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn failed_first_content_load_restarts_owner_before_next_navigation() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (body, expected_path) in [
            (
                format!(
                    "<!doctype html><style>{}</style><main>513 rules loaded</main>",
                    (0..513)
                        .map(|index| format!(".r{index}{{color:red}}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                "/513-rules",
            ),
            (
                format!(
                    "<!doctype html><style>{}</style><main>bad</main>",
                    (0..=crate::native_engine::css::MAX_NATIVE_STYLE_RULES)
                        .map(|index| format!(".r{index}{{color:red}}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                "/too-many-rules",
            ),
            (
                "<!doctype html><title>Recovered</title><main>ok</main>".into(),
                "/good",
            ),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with(&format!("GET {expected_path} ")));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://failed-initial-load/start")
        .with_fixture("fixture://failed-initial-load/start", "<main>start</main>")
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let committed_revision = engine.revision();

    let supported = engine
        .navigate_async(format!("http://{address}/513-rules"))
        .await
        .expect("a 513-rule sheet must fit the bounded native stylesheet budget");
    assert!(supported.visible_text.contains("513 rules loaded"));
    assert!(engine.revision() > committed_revision);
    let committed_revision = engine.revision();

    let error = engine
        .navigate_async(format!("http://{address}/too-many-rules"))
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("CSS style rules exceeds limit 4096 (actual 4097)")
    );
    assert_eq!(
        engine.revision(),
        committed_revision,
        "failed load must not commit a revision"
    );
    assert!(
        engine.take_async_effect_notifications().unwrap().is_empty(),
        "the background effect pump must discard the retired worker's terminal pipe error"
    );

    let recovered = engine
        .navigate_async(format!("http://{address}/good"))
        .await
        .expect("a later navigation must create a fresh content owner");
    assert_eq!(recovered.url, format!("http://{address}/good"));
    assert!(recovered.visible_text.contains("ok"));
    assert!(engine.revision() > committed_revision);
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn stalled_outgoing_unload_discards_worker_and_allows_navigation_to_recover() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8_lossy(&request);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or_default();
            let body = match path {
                "/start" => {
                    "<title>Start</title><script>addEventListener('unload', () => { while (true) {} });</script><main>start page</main>"
                }
                "/next" => "<title>Next</title><main>navigation recovered</main>",
                other => panic!("unexpected lifecycle test request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let start_url = format!("http://{address}/start");
    let next_url = format!("http://{address}/next");
    let config = NativeEngineConfig::default().with_initial_url(start_url);
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let snapshot = engine
        .navigate_request_async_with_lifecycle_timeout(
            NativeNavigationRequest::get(next_url.clone()),
            0,
            true,
            None,
            std::time::Duration::from_millis(250),
        )
        .await
        .expect("a timed-out outgoing unload must not strand the next document");

    assert_eq!(snapshot.url, next_url);
    assert!(snapshot.visible_text.contains("navigation recovered"));
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn native_content_process_worker_timer_fetch_uses_parent_cookie_authority() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests: Vec<TestHttpRequest> = Vec::new();
        for _ in 0..6 {
            let accepted =
                tokio::time::timeout(std::time::Duration::from_secs(45), listener.accept())
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "worker timer requests timed out after paths {:?}",
                            requests
                                .iter()
                                .map(|(path, _, _, _, _)| path)
                                .collect::<Vec<_>>()
                        )
                    })
                    .unwrap();
            let (mut stream, _) = accepted;
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            let header_end = loop {
                let read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "worker timer request ended before its headers");
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let request_headers = String::from_utf8_lossy(&request[..header_end]).into_owned();
            let content_length = request_headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap_or_default();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "worker timer request body ended early");
                request.extend_from_slice(&buffer[..read]);
            }
            let method = request_headers
                .split_whitespace()
                .next()
                .expect("worker timer HTTP request includes a method")
                .to_owned();
            let path = request_headers
                .split_whitespace()
                .nth(1)
                .expect("worker timer HTTP request includes a path")
                .to_owned();
            let cookie = request_headers
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let content_type_header = request_headers
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("content-type:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let request_body = request[header_end..header_end + content_length].to_vec();
            if path == "/timer-upload" {
                assert_eq!(method, "POST");
                assert_eq!(content_type_header.as_deref(), Some("text/plain"));
                assert_eq!(request_body, b"worker-timer-pull-body");
            }
            let (response_headers, response_content_type, response_body) = match path.as_str() {
                "/page" => (
                    concat!(
                        "Set-Cookie: timer_session=visible; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: timer_secret=before; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<main>worker timer cookie broker</main>",
                ),
                "/timer-worker.js" => (
                    "Set-Cookie: timer_script=loaded; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    "importScripts('/timer-dependency.js');",
                ),
                "/timer-dependency.js" => (
                    "Set-Cookie: timer_dependency=loaded; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    "setTimeout(async () => { try { const first = await fetch('/timer-first'); const firstBody = await first.text(); const upload = new ReadableStream({ pull(controller) { controller.enqueue(new TextEncoder().encode('worker-timer-pull-body')); controller.close(); } }); const uploaded = await fetch('/timer-upload', { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: upload }); const uploadBody = await uploaded.text(); const second = await fetch('/timer-second'); postMessage({ kind: 'complete', firstBody, uploadBody, secondBody: await second.text() }); } catch (error) { postMessage({ kind: 'error', message: String(error) }); } }, 25);",
                ),
                "/timer-first" => (
                    "Set-Cookie: timer_secret=after; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "first",
                ),
                "/timer-upload" => (
                    "Set-Cookie: timer_upload=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "uploaded",
                ),
                "/timer-second" => ("", "text/plain", "second"),
                other => panic!("unexpected worker timer request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{response_headers}Content-Type: {response_content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie, method, content_type_header, request_body));
        }
        requests
    });

    let config = NativeEngineConfig::default().with_initial_url(format!("http://{address}/page"));
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
            .evaluate_async(
                "document.cookie = 'timer_turn=present; Path=/'; globalThis.timerMessages = []; globalThis.timerWorker = new Worker('/timer-worker.js'); timerWorker.onmessage = event => timerMessages.push(event.data)",
            )
            .await
            .unwrap();

    let notification = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Some(notification) = engine
                .take_async_effect_notifications()
                .unwrap()
                .into_iter()
                .next()
            {
                break notification;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the browser owner should be notified when the worker timer is due");
    engine
        .dispatch_async_effect_notification(&notification)
        .await
        .unwrap();
    let visible_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    let cookies = engine.cookies_async().await.unwrap();
    let messages = engine.evaluate_async("timerMessages").await.unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        messages,
        serde_json::json!([{
            "kind": "complete",
            "firstBody": "first",
            "uploadBody": "uploaded",
            "secondBody": "second"
        }])
    );
    assert_eq!(
        requests
            .iter()
            .map(|(path, _, _, _, _)| path.as_str())
            .collect::<Vec<_>>(),
        [
            "/page",
            "/timer-worker.js",
            "/timer-dependency.js",
            "/timer-first",
            "/timer-upload",
            "/timer-second"
        ]
    );
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _, _, _, _)| request_path == path)
            .and_then(|(_, cookie, _, _, _)| cookie.as_deref())
            .unwrap_or_default()
    };
    assert!(cookie_for("/timer-worker.js").contains("timer_session=visible"));
    assert!(cookie_for("/timer-worker.js").contains("timer_secret=before"));
    assert!(cookie_for("/timer-dependency.js").contains("timer_script=loaded"));
    assert!(
        cookie_for("/timer-first").contains("timer_script=loaded"),
        "timer Fetch did not receive the parent script cookie: {:?}; parent cookies: {:?}",
        cookie_for("/timer-first"),
        cookies
            .iter()
            .map(|cookie| (&cookie.name, &cookie.value))
            .collect::<Vec<_>>()
    );
    assert!(cookie_for("/timer-first").contains("timer_dependency=loaded"));
    assert!(cookie_for("/timer-first").contains("timer_secret=before"));
    assert!(cookie_for("/timer-first").contains("timer_turn=present"));
    assert!(cookie_for("/timer-upload").contains("timer_secret=after"));
    assert!(cookie_for("/timer-upload").contains("timer_turn=present"));
    assert!(cookie_for("/timer-second").contains("timer_secret=after"));
    assert!(cookie_for("/timer-second").contains("timer_upload=accepted"));
    assert!(
        visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_session=visible"))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_secret="))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_script="))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_upload="))
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "timer_secret" && cookie.value == "after")
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "timer_script" && cookie.value == "loaded")
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "timer_upload" && cookie.value == "accepted")
    );
}

#[tokio::test]
async fn native_content_process_service_worker_timer_fetch_uses_parent_cookie_authority() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests: Vec<TestHttpRequest> = Vec::new();
        for _ in 0..5 {
            let accepted =
                tokio::time::timeout(std::time::Duration::from_secs(45), listener.accept())
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "Service Worker timer requests timed out after paths {:?}",
                            requests
                                .iter()
                                .map(|(path, _, _, _, _)| path)
                                .collect::<Vec<_>>()
                        )
                    })
                    .unwrap();
            let (mut stream, _) = accepted;
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            let header_end = loop {
                let read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "Service Worker timer request ended before headers");
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let request_headers = String::from_utf8_lossy(&request[..header_end]).into_owned();
            let content_length = request_headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap_or_default();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "Service Worker timer request body ended early");
                request.extend_from_slice(&buffer[..read]);
            }
            let method = request_headers
                .split_whitespace()
                .next()
                .expect("Service Worker timer request includes a method")
                .to_owned();
            let path = request_headers
                .split_whitespace()
                .nth(1)
                .expect("Service Worker timer request includes a path")
                .to_owned();
            let cookie = request_headers
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let content_type_header = request_headers
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("content-type:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let request_body = request[header_end..header_end + content_length].to_vec();
            if path == "/timer-upload" {
                assert_eq!(method, "POST");
                assert_eq!(content_type_header.as_deref(), Some("text/plain"));
                assert_eq!(request_body, b"service-worker-timer-body");
            }
            let (response_headers, response_content_type, response_body) = match path.as_str() {
                "/page" => (
                    concat!(
                        "Set-Cookie: timer_session=visible; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: timer_secret=before; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<main>Service Worker timer cookie broker</main>",
                ),
                "/sw.js" => (
                    "Set-Cookie: timer_script=loaded; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
setTimeout(async () => {
  try {
    const first = await fetch('/timer-first');
    await first.text();
    const upload = new ReadableStream({
      pull(controller) {
        controller.enqueue(new TextEncoder().encode('service-worker-timer-body'));
        controller.close();
      },
    });
    const posted = await fetch('/timer-upload', {
      method: 'POST',
      headers: { 'Content-Type': 'text/plain' },
      body: upload,
    });
    await posted.text();
    const second = await fetch('/timer-second');
    await second.text();
  } catch (_) {}
}, 0);"#,
                ),
                "/timer-first" => (
                    "Set-Cookie: timer_secret=after; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "first",
                ),
                "/timer-upload" => (
                    "Set-Cookie: timer_upload=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "uploaded",
                ),
                "/timer-second" => ("", "text/plain", "second"),
                other => panic!("unexpected Service Worker timer request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{response_headers}Content-Type: {response_content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie, method, content_type_header, request_body));
        }
        requests
    });

    let config = NativeEngineConfig::default().with_initial_url(format!("http://{address}/page"));
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
            .evaluate_async("document.cookie = 'timer_turn=present; Path=/'; globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' }); true")
            .await
            .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !server.is_finished() {
            for notification in engine.take_async_effect_notifications().unwrap() {
                engine
                    .dispatch_async_effect_notification(&notification)
                    .await
                    .unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect(
        "the browser owner should dispatch lifecycle and timer turns until both Fetches finish",
    );
    let visible_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(path, _, _, _, _)| path.as_str())
            .collect::<Vec<_>>(),
        [
            "/page",
            "/sw.js",
            "/timer-first",
            "/timer-upload",
            "/timer-second"
        ]
    );
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _, _, _, _)| request_path == path)
            .and_then(|(_, cookie, _, _, _)| cookie.as_deref())
            .unwrap_or_default()
    };
    assert!(cookie_for("/sw.js").contains("timer_secret=before"));
    assert!(
        cookie_for("/timer-first").contains("timer_script=loaded"),
        "timer Fetch did not receive the parent script cookie: {:?}; parent cookies: {:?}",
        cookie_for("/timer-first"),
        cookies
            .iter()
            .map(|cookie| (&cookie.name, &cookie.value))
            .collect::<Vec<_>>()
    );
    assert!(cookie_for("/timer-first").contains("timer_secret=before"));
    assert!(cookie_for("/timer-first").contains("timer_turn=present"));
    assert!(cookie_for("/timer-upload").contains("timer_script=loaded"));
    assert!(cookie_for("/timer-upload").contains("timer_secret=after"));
    assert!(cookie_for("/timer-upload").contains("timer_turn=present"));
    assert!(cookie_for("/timer-second").contains("timer_secret=after"));
    assert!(cookie_for("/timer-second").contains("timer_upload=accepted"));
    assert!(
        visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_session=visible"))
    );
    assert!(
        visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_turn=present"))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_secret="))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_script="))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("timer_upload="))
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "timer_secret" && cookie.value == "after")
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| { cookie.name == "timer_upload" && cookie.value == "accepted" })
    );
}

#[tokio::test]
async fn native_content_process_service_worker_message_fetch_uses_parent_cookie_authority() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..4 {
            let accepted =
                tokio::time::timeout(std::time::Duration::from_secs(45), listener.accept())
                    .await
                    .expect("ServiceWorker message and follow-up requests arrive")
                    .unwrap();
            let (mut stream, _) = accepted;
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let read = stream.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request).into_owned();
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("ServiceWorker message request includes a path")
                .to_owned();
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (headers, content_type, body) = match path.as_str() {
                "/page" => (
                    concat!(
                        "Set-Cookie: message_visible=visible; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: message_secret=before; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>ServiceWorker message cookie broker</main>",
                ),
                "/sw.js" => (
                    "Set-Cookie: message_script=loaded; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "application/javascript",
                    "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim())); self.addEventListener('message', event => { if (event.data === 'cookie-check') event.waitUntil(fetch('/message-fetch').then(response => response.text())); });",
                ),
                "/message-fetch" => (
                    "Set-Cookie: message_secret=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "message fetch completed",
                ),
                "/after" => ("", "text/plain", "after"),
                other => panic!("unexpected ServiceWorker message request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie));
        }
        requests
    });

    let config = NativeEngineConfig::default().with_initial_url(format!("http://{address}/page"));
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
            .evaluate_async(
                "await registrationPromise.then(registration => registration.active.postMessage('cookie-check'))",
            )
            .await
            .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await fetch('/after').then(response => response.text())")
            .await
            .unwrap(),
        "after"
    );
    let visible_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/page", "/sw.js", "/message-fetch", "/after"]
    );
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .and_then(|(_, cookie)| cookie.as_deref())
            .unwrap_or_default()
    };
    assert!(cookie_for("/message-fetch").contains("message_script=loaded"));
    assert!(cookie_for("/message-fetch").contains("message_secret=before"));
    assert!(cookie_for("/after").contains("message_secret=accepted"));
    assert!(
        visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("message_visible=visible"))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("message_secret="))
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "message_secret" && cookie.value == "accepted")
    );
}

#[tokio::test]
async fn browser_owned_service_worker_lifetime_fetch_uses_parent_cookie_authority() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request_tx, mut request_rx) = tokio::sync::mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..4 {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(45), listener.accept())
                    .await
                    .expect("Service Worker lifetime Fetch request arrives")
                    .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let read = stream.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request).into_owned();
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("request includes a path")
                .to_owned();
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (headers, content_type, body) = match path.as_str() {
                "/page" => (
                    concat!(
                        "Set-Cookie: lifetime_visible=visible; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: lifetime_secret=before; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>Service Worker lifetime cookie broker</main>",
                ),
                "/sw.js" => (
                    "",
                    "application/javascript",
                    "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim())); self.addEventListener('fetch', event => { if (new URL(event.request.url).pathname === '/trigger') { event.respondWith(new Response('triggered')); event.waitUntil(fetch('/lifetime-first').then(response => response.text()).then(text => { self.__lifetimeResult = text; })); } });",
                ),
                "/lifetime-first" => (
                    "Set-Cookie: lifetime_secret=after; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "lifetime completed",
                ),
                "/after" => ("", "text/plain", "after"),
                other => panic!("unexpected Service Worker lifetime request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            let observed = (path, cookie);
            request_tx.send(observed.clone()).unwrap();
            requests.push(observed);
        }
        requests
    });

    let config = NativeEngineConfig::default().with_initial_url(format!("http://{address}/page"));
    let mut engine =
        NativeEngine::new_with_browser_shared_workers(config, NativeDialogControlPlane::default())
            .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
            engine
                .evaluate_async(
                    "await registrationPromise.then(async registration => [registration.active.state, navigator.serviceWorker.controller !== null])",
                )
                .await
                .unwrap(),
            serde_json::json!(["activated", true])
        );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/trigger').then(response => response.text())")
            .await
            .unwrap(),
        "triggered"
    );

    let mut observed = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !observed
            .iter()
            .any(|(path, _): &(String, Option<String>)| path == "/lifetime-first")
        {
            for notification in engine.take_async_effect_notifications().unwrap() {
                engine
                    .dispatch_async_effect_notification(&notification)
                    .await
                    .unwrap();
            }
            while let Ok(request) = request_rx.try_recv() {
                observed.push(request);
            }
            if !observed
                .iter()
                .any(|(path, _): &(String, Option<String>)| path == "/lifetime-first")
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    })
    .await
    .expect("the FetchEvent lifetime request is dispatched on a parent-owned turn");

    let cookies = engine.cookies_async().await.unwrap();
    let visible_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "lifetime_secret" && cookie.value == "after")
    );
    assert!(
        visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("lifetime_visible=visible"))
    );
    assert!(
        !visible_cookies
            .as_str()
            .is_some_and(|value| value.contains("lifetime_secret="))
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/after').then(response => response.text())")
            .await
            .unwrap(),
        "after"
    );
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/page", "/sw.js", "/lifetime-first", "/after"]
    );
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .and_then(|(_, cookie)| cookie.as_deref())
            .unwrap_or_default()
    };
    assert!(cookie_for("/lifetime-first").contains("lifetime_secret=before"));
    assert!(cookie_for("/after").contains("lifetime_secret=after"));
}

#[tokio::test]
async fn beforeunload_prompt_holds_navigation_until_exact_user_decision() {
    let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://beforeunload-modal-start",
                "<script>globalThis.lifecycle = []; addEventListener('beforeunload', event => { lifecycle.push(event.type); event.preventDefault(); event.returnValue = 'must not be shown'; globalThis.beforeunloadMutation = 'seen'; }); addEventListener('pagehide', () => lifecycle.push('pagehide')); addEventListener('unload', () => lifecycle.push('unload'));</script><button id='activate'>Activate</button><title>Start</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://beforeunload-modal-next",
                "<title>Next</title>",
            )
            .unwrap()
            .with_initial_url("fixture://beforeunload-modal-start");
    let control = NativeDialogControlPlane::for_modal_owner();
    let controller = NativeDialogController::new(control.clone()).unwrap();
    let mut engine = NativeEngine::new_with_dialog_control(config, control).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=activate".into(),
        })
        .await
        .unwrap();

    let mut navigation = Box::pin(engine.navigate_async("fixture://beforeunload-modal-next"));
    let first_pending = loop {
        if let Some(pending) = controller.pending_dialog().unwrap() {
            break pending;
        }
        tokio::select! {
            result = &mut navigation => panic!("navigation completed before its prompt: {}", result.is_ok()),
            () = tokio::time::sleep(Duration::from_millis(1)) => {}
        }
    };
    assert_eq!(first_pending.dialog.dialog_type, "beforeunload");
    assert!(first_pending.dialog.message.is_empty());
    assert!(first_pending.dialog.default_value.is_none());
    assert!(
        controller
            .resolve_dialog(
                "stale-beforeunload-id",
                NativeDialogResolution {
                    accepted: true,
                    prompt_value: None,
                },
            )
            .is_err()
    );
    controller
        .resolve_dialog(
            &first_pending.id,
            NativeDialogResolution {
                accepted: false,
                prompt_value: None,
            },
        )
        .unwrap();
    let snapshot = navigation.await.unwrap();
    assert_eq!(snapshot.url, "fixture://beforeunload-modal-start");
    assert_eq!(snapshot.title, "Start");
    assert_eq!(
        engine.evaluate_async("globalThis.lifecycle").await.unwrap(),
        serde_json::json!(["beforeunload"])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.beforeunloadMutation")
            .await
            .unwrap(),
        serde_json::json!("seen")
    );

    let mut navigation = Box::pin(engine.navigate_async("fixture://beforeunload-modal-next"));
    let second_pending = loop {
        if let Some(pending) = controller.pending_dialog().unwrap() {
            break pending;
        }
        tokio::select! {
            result = &mut navigation => panic!("navigation completed before its prompt: {}", result.is_ok()),
            () = tokio::time::sleep(Duration::from_millis(1)) => {}
        }
    };
    assert_ne!(first_pending.id, second_pending.id);
    controller
        .resolve_dialog(
            &second_pending.id,
            NativeDialogResolution {
                accepted: true,
                prompt_value: None,
            },
        )
        .unwrap();
    let snapshot = navigation.await.unwrap();
    assert_eq!(snapshot.url, "fixture://beforeunload-modal-next");
    assert_eq!(snapshot.title, "Next");
    assert!(controller.pending_dialog().unwrap().is_none());
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn local_history_beforeunload_waits_for_exact_confirmation() {
    let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://beforeunload-history-first",
                "<title>First</title><p>First history document</p>",
            )
            .unwrap()
            .with_fixture(
                "fixture://beforeunload-history-second",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'not shown'; globalThis.beforeunloadRan = 'yes'; }); addEventListener('pagehide', () => globalThis.pagehideRan = 'yes'); addEventListener('unload', () => globalThis.unloadRan = 'yes');</script><title>Second</title><button id='activate'>Activate</button>",
            )
            .unwrap()
            .with_initial_url("fixture://beforeunload-history-first");
    let control = NativeDialogControlPlane::for_modal_owner();
    let controller = NativeDialogController::new(control.clone()).unwrap();
    let mut engine = NativeEngine::new_with_dialog_control(config, control).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .navigate_async("fixture://beforeunload-history-second")
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=activate".into(),
        })
        .await
        .unwrap();

    let mut back = Box::pin(engine.go_back_async());
    let first_pending = loop {
        if let Some(pending) = controller.pending_dialog().unwrap() {
            break pending;
        }
        tokio::select! {
            result = &mut back => panic!("history traversal completed before confirmation: {}", result.is_ok()),
            () = tokio::time::sleep(Duration::from_millis(1)) => {}
        }
    };
    assert_eq!(first_pending.dialog.dialog_type, "beforeunload");
    assert_eq!(
        first_pending.dialog.url,
        "fixture://beforeunload-history-second"
    );
    controller
        .resolve_dialog(
            &first_pending.id,
            NativeDialogResolution {
                accepted: false,
                prompt_value: None,
            },
        )
        .unwrap();
    let dismissed = back.await.unwrap().unwrap();
    assert_eq!(dismissed.url, "fixture://beforeunload-history-second");
    assert_eq!(
            engine
                .evaluate_async("[globalThis.beforeunloadRan || null, globalThis.pagehideRan || null, globalThis.unloadRan || null]")
                .await
                .unwrap(),
            serde_json::json!(["yes", null, null])
        );

    let mut back = Box::pin(engine.go_back_async());
    let second_pending = loop {
        if let Some(pending) = controller.pending_dialog().unwrap() {
            break pending;
        }
        tokio::select! {
            result = &mut back => panic!("retry history traversal completed before confirmation: {}", result.is_ok()),
            () = tokio::time::sleep(Duration::from_millis(1)) => {}
        }
    };
    assert_ne!(first_pending.id, second_pending.id);
    controller
        .resolve_dialog(
            &second_pending.id,
            NativeDialogResolution {
                accepted: true,
                prompt_value: None,
            },
        )
        .unwrap();
    let accepted = back.await.unwrap().unwrap();
    assert_eq!(accepted.url, "fixture://beforeunload-history-first");
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn synchronous_navigation_fails_closed_when_beforeunload_needs_confirmation() {
    let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://beforeunload-sync-start",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); globalThis.beforeunloadMutation = 'seen'; }); addEventListener('pagehide', () => globalThis.pagehideMutation = 'yes'); addEventListener('unload', () => globalThis.unloadMutation = 'yes');</script><button id='activate'>Activate</button><title>Start</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://beforeunload-sync-next",
                "<title>Next</title>",
            )
            .unwrap()
            .with_initial_url("fixture://beforeunload-sync-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    engine
        .action(NativeAction::Click {
            target: "id=activate".into(),
        })
        .unwrap();

    let error = engine
        .navigate("fixture://beforeunload-sync-next")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("synchronous navigation cannot wait")
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://beforeunload-sync-start"
    );
    assert_eq!(
            engine
                .evaluate_async("[globalThis.beforeunloadMutation || null, globalThis.pagehideMutation || null, globalThis.unloadMutation || null]")
                .await
                .unwrap(),
            serde_json::json!(["seen", null, null])
        );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn synchronous_local_history_beforeunload_fails_closed_after_activation() {
    let config = NativeEngineConfig::default()
            .with_fixture("fixture://sync-history-first", "<title>First</title>")
            .unwrap()
            .with_fixture(
                "fixture://sync-history-second",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'not shown'; globalThis.beforeunloadMutation = 'seen'; }); addEventListener('pagehide', () => globalThis.pagehideMutation = 'yes'); addEventListener('unload', () => globalThis.unloadMutation = 'yes');</script><title>Second</title><button id='activate'>Activate</button>",
            )
            .unwrap()
            .with_initial_url("fixture://sync-history-first");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    engine.navigate("fixture://sync-history-second").unwrap();
    engine
        .action(NativeAction::Click {
            target: "id=activate".into(),
        })
        .unwrap();

    let current_history_index = engine.history().current_index();
    let error = engine.go_back().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("synchronous navigation cannot wait")
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://sync-history-second"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Second");
    assert_eq!(engine.history().current_index(), current_history_index);
    assert_eq!(engine.history().len(), 2);
    assert_eq!(
            engine
                .evaluate_async(
                    "[globalThis.beforeunloadMutation || null, globalThis.pagehideMutation || null, globalThis.unloadMutation || null]",
                )
                .await
                .unwrap(),
            serde_json::json!(["seen", null, null])
        );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn synchronous_local_history_beforeunload_without_activation_commits_target() {
    let config = NativeEngineConfig::default()
            .with_fixture("fixture://sync-history-no-activation-first", "<title>First</title>")
            .unwrap()
            .with_fixture(
                "fixture://sync-history-no-activation-second",
                "<script>addEventListener('beforeunload', event => { event.preventDefault(); globalThis.beforeunloadRan = true; }); addEventListener('pagehide', () => globalThis.pagehideRan = true); addEventListener('unload', () => globalThis.unloadRan = true);</script><title>Second</title>",
            )
            .unwrap()
            .with_initial_url("fixture://sync-history-no-activation-first");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    engine
        .navigate("fixture://sync-history-no-activation-second")
        .unwrap();
    let outgoing_window = NativeNodeId::from_parts(engine.document.generation(), u32::MAX);

    let snapshot = engine.go_back().unwrap().unwrap();

    assert_eq!(snapshot.url, "fixture://sync-history-no-activation-first");
    assert_eq!(engine.history().current_index(), Some(0));
    let lifecycle = engine
        .effects
        .iter()
        .filter(|effect| effect.node_id == outgoing_window)
        .map(|effect| effect.kind)
        .filter(|kind| {
            matches!(
                kind,
                NativeEventKind::BeforeUnload | NativeEventKind::PageHide | NativeEventKind::Unload
            )
        })
        .collect::<Vec<_>>();
    let beforeunload = lifecycle
        .iter()
        .position(|kind| *kind == NativeEventKind::BeforeUnload)
        .unwrap();
    let pagehide = lifecycle
        .iter()
        .position(|kind| *kind == NativeEventKind::PageHide)
        .unwrap();
    let unload = lifecycle
        .iter()
        .position(|kind| *kind == NativeEventKind::Unload)
        .unwrap();
    assert!(beforeunload < pagehide && pagehide < unload);
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn synchronous_and_async_history_reentry_fails_closed_during_outgoing_dispatch() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://sync-history-reentry-first",
            "<title>First</title>",
        )
        .unwrap()
        .with_fixture(
            "fixture://sync-history-reentry-second",
            "<title>Second</title>",
        )
        .unwrap()
        .with_initial_url("fixture://sync-history-reentry-first");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    engine
        .navigate("fixture://sync-history-reentry-second")
        .unwrap();
    let current_history_index = engine.history().current_index();
    engine.outgoing_lifecycle_dispatch_depth = 1;

    let synchronous_error = engine.go_back().unwrap_err();
    let asynchronous_error = engine.go_back_async().await.unwrap_err();
    engine.outgoing_lifecycle_dispatch_depth = 0;

    assert!(
        synchronous_error
            .to_string()
            .contains("cannot re-enter an outgoing lifecycle callback")
    );
    assert!(
        asynchronous_error
            .to_string()
            .contains("cannot re-enter an outgoing lifecycle callback")
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://sync-history-reentry-second"
    );
    assert_eq!(engine.history().current_index(), current_history_index);
    assert_eq!(engine.snapshot().unwrap().title, "Second");
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn synchronous_same_document_history_remains_in_place() {
    let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://sync-same-document-history",
                "<script>globalThis.beforeunloadCount = 0; addEventListener('beforeunload', () => { globalThis.beforeunloadCount += 1; });</script><title>History</title>",
            )
            .unwrap()
            .with_initial_url("fixture://sync-same-document-history#first");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    engine
        .navigate("fixture://sync-same-document-history#second")
        .unwrap();

    assert_eq!(
        engine.go_back().unwrap().unwrap().url,
        "fixture://sync-same-document-history#first"
    );
    assert_eq!(
        engine.go_forward().unwrap().unwrap().url,
        "fixture://sync-same-document-history#second"
    );
    assert_eq!(engine.history().len(), 2);
    assert_eq!(
        engine
            .evaluate_async("globalThis.beforeunloadCount")
            .await
            .unwrap(),
        serde_json::json!(0)
    );
    engine.close_async().await.unwrap();
}

#[test]
fn preflight_resolves_current_semantics_and_geometry_without_mutation() {
    let engine = initialized_engine();
    let revision = engine.revision();
    let result = engine
        .preflight_target("id=save", NativePreflightAction::Click)
        .expect("button preflight must succeed");

    assert_eq!(result.action, NativePreflightAction::Click);
    assert!(result.unique);
    assert_eq!(result.actionable, Some(true));
    assert!(result.actionability_reason.is_none());
    assert_eq!(result.error_kind, None);
    assert_eq!(result.revision, revision);
    assert!(
        result
            .geometry
            .is_some_and(|rect| rect.width > 0 && rect.height > 0)
    );
    assert!(!result.likely_navigation);
    assert!(!result.likely_form_submit);
    assert_eq!(engine.revision(), revision);
}

#[test]
fn preflight_reports_actionability_and_resolution_failures() {
    let engine = initialized_engine();

    let hidden = engine
        .preflight_target("id=hidden", NativePreflightAction::Click)
        .expect("hidden target should produce a preflight result");
    assert_eq!(hidden.actionable, Some(false));
    assert_eq!(
        hidden.actionability_reason,
        Some(NativeActionabilityReason::NotVisible)
    );

    let disabled = engine
        .preflight_target("id=disabled", NativePreflightAction::Click)
        .expect("disabled target should produce a preflight result");
    assert_eq!(disabled.actionable, Some(false));
    assert_eq!(
        disabled.actionability_reason,
        Some(NativeActionabilityReason::Disabled)
    );

    let read_only = engine
        .preflight_target("id=readonly", NativePreflightAction::Type)
        .expect("read-only target should produce a preflight result");
    assert_eq!(read_only.actionable, Some(false));
    assert_eq!(
        read_only.actionability_reason,
        Some(NativeActionabilityReason::ReadOnly)
    );

    let ambiguous = engine
        .preflight_target("name=Duplicate", NativePreflightAction::Click)
        .expect("ambiguous target should produce a preflight result");
    assert!(!ambiguous.unique);
    assert_eq!(ambiguous.error_kind, Some(NativeTargetErrorKind::Ambiguous));

    let missing = engine
        .preflight_target("id=missing", NativePreflightAction::Click)
        .expect("missing target should produce a preflight result");
    assert!(!missing.unique);
    assert_eq!(missing.error_kind, Some(NativeTargetErrorKind::NotFound));
}

#[test]
fn preflight_rejects_stale_revision_references_without_mutation() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://preflight.test/one")
        .with_fixture(
            "fixture://preflight.test/one",
            "<button id='one'>One</button>",
        )
        .and_then(|config| {
            config.with_fixture(
                "fixture://preflight.test/two",
                "<button id='two'>Two</button>",
            )
        })
        .expect("stale-reference fixtures must validate");
    let mut engine = NativeEngine::new(config).expect("native engine must construct");
    engine.initialize().expect("native engine must initialize");
    let reference = engine
        .semantic_nodes()
        .expect("semantic nodes must be available")
        .into_iter()
        .find(|node| node.tag_name == "button")
        .expect("fixture button must be semantic")
        .reference;
    engine
        .navigate("fixture://preflight.test/two")
        .expect("second fixture navigation must succeed");
    let revision = engine.revision();

    let result = engine
        .preflight_target(&reference, NativePreflightAction::Click)
        .expect("stale reference should produce a preflight result");
    assert!(!result.unique);
    assert_eq!(
        result.error_kind,
        Some(NativeTargetErrorKind::StaleReference)
    );
    assert_eq!(result.revision, revision);
}

#[tokio::test]
async fn cssom_custom_property_mutations_recompute_inherited_font_size() {
    let config = NativeEngineConfig::default()
            .with_initial_url("fixture://cssom.test/index")
            .with_fixture(
                "fixture://cssom.test/index",
                "<style>#parent { --size: 1em; } #child { font-size: var(--size); }</style><div id='parent'><span id='child'>Child</span></div>",
            )
            .expect("CSSOM fixture must validate");
    let mut engine = NativeEngine::new(config).expect("native engine must construct");
    engine.initialize().expect("native engine must initialize");
    let child = engine
        .resolve_target("id=child")
        .expect("CSSOM child must resolve");

    assert_eq!(
        engine.document.computed_style_for_layout(child).font_size(),
        16
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.getElementById('parent').style.setProperty('--size', '2em'); true"
            )
            .await
            .expect("CSSOM custom-property update must succeed"),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.document.computed_style_for_layout(child).font_size(),
        32
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.getElementById('parent').style.removeProperty('--size'); true"
            )
            .await
            .expect("CSSOM custom-property removal must succeed"),
        serde_json::json!(true)
    );
    assert_eq!(
        engine.document.computed_style_for_layout(child).font_size(),
        16
    );
}

#[tokio::test]
async fn viewport_updates_recompute_css_and_script_dimensions() {
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://viewport.test/index")
        .with_viewport(Viewport {
            width: 1_000,
            height: 800,
            device_scale_factor_milli: 1_000,
        })
        .with_fixture(
            "fixture://viewport.test/index",
            "<style>#target { font-size: 2vw; }</style><span id='target'>Viewport</span>",
        )
        .expect("viewport fixture must validate");
    let mut engine = NativeEngine::new(config).expect("native engine must construct");
    engine.initialize().expect("native engine must initialize");
    let target = engine
        .resolve_target("id=target")
        .expect("viewport target must resolve");

    assert_eq!(
        engine
            .document
            .computed_style_for_layout(target)
            .font_size(),
        20
    );
    assert_eq!(
        engine
            .evaluate_async("({ width: innerWidth, height: innerHeight })")
            .await
            .expect("initial viewport evaluation must succeed"),
        serde_json::json!({"width": 1000, "height": 800})
    );

    engine
        .set_viewport_async(Viewport {
            width: 500,
            height: 400,
            device_scale_factor_milli: 1_000,
        })
        .await
        .expect("viewport update must succeed");

    assert_eq!(
        engine
            .document
            .computed_style_for_layout(target)
            .font_size(),
        10
    );
    assert_eq!(
        engine
            .evaluate_async("({ width: innerWidth, height: innerHeight })")
            .await
            .expect("updated viewport evaluation must succeed"),
        serde_json::json!({"width": 500, "height": 400})
    );
    assert_eq!(
        engine
            .snapshot()
            .expect("snapshot must succeed")
            .viewport
            .width,
        500
    );
}

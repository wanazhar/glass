use glass::browser::chrome::detect_chrome;
use glass::browser::session::{BrowserSession, InteractionMode, SessionOptions};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

struct FixtureServer {
    url: String,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl FixtureServer {
    async fn start(html: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, mut shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            tokio::spawn(serve_fixture(stream, html));
                        }
                        Err(_) => break,
                    },
                }
            }
        });
        Self {
            url: format!("http://{address}/fixture.html"),
            shutdown: Some(shutdown),
            task,
        }
    }

    async fn close(mut self) {
        let _ = self.shutdown.take().unwrap().send(());
        let _ = self.task.await;
    }
}

async fn serve_fixture(mut stream: TcpStream, html: &'static str) {
    let mut request = [0; 4_096];
    let _ = stream.read(&mut request).await;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

async fn page_target_id(port: u16) -> String {
    let targets: Vec<Value> = reqwest::get(format!("http://127.0.0.1:{port}/json"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    targets
        .into_iter()
        .find(|target| target["type"] == "page" && target["webSocketDebuggerUrl"].is_string())
        .and_then(|target| target["id"].as_str().map(str::to_string))
        .expect("Chrome should expose a page target")
}

#[tokio::test]
async fn concurrent_owned_sessions_on_one_port_do_not_adopt_each_other() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let Some(chrome_path) = detect_chrome() else {
        eprintln!("skipping browser smoke test; Chrome/Chromium is unavailable");
        return;
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let first_options = SessionOptions {
        port,
        chrome_path: Some(chrome_path.clone()),
        profile: "e2e-concurrent-first".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    };
    let second_options = SessionOptions {
        profile: "e2e-concurrent-second".to_string(),
        ..first_options.clone()
    };

    let (first, second) = tokio::join!(
        BrowserSession::start(&first_options),
        BrowserSession::start(&second_options),
    );
    match (first, second) {
        (Ok(session), Err(error)) | (Err(error), Ok(session)) => {
            assert!(
                error.to_string().contains("--attach"),
                "second owned startup should fail explicitly: {error}"
            );
            session.close().await.unwrap();
        }
        (Ok(first), Ok(second)) => {
            first.close().await.unwrap();
            second.close().await.unwrap();
            panic!("only one concurrent owned session may start on one CDP port");
        }
        (Err(first), Err(second)) => {
            panic!("one owned session should start; first error: {first}; second error: {second}");
        }
    }
}

#[tokio::test]
async fn browser_session_drives_a_local_fixture() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let Some(chrome_path) = detect_chrome() else {
        eprintln!("skipping browser smoke test; Chrome/Chromium is unavailable");
        return;
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let fixture_server = FixtureServer::start(include_str!("fixtures/basic.html")).await;
    let url = fixture_server.url.clone();
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path.clone()),
        profile: "e2e".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Human,
    })
    .await
    .unwrap();
    assert!(session.owns_chrome());
    assert!(!session.is_attached());

    let owned_error = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: None,
        profile: "e2e-conflict".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .err()
    .expect("an owned session must not adopt an occupied CDP endpoint");
    assert!(owned_error.to_string().contains("--attach"));

    let page = session.navigate(&url).await.unwrap();
    assert_eq!(page.title, "Glass Fixture");
    assert!(session.text().await.unwrap().contains("Glass Fixture"));

    let context = session.observe().await.unwrap();
    assert!(context.dom.is_none());
    assert!(!context.accessibility.interactive.is_empty());
    assert!(context.screenshot.is_none());
    assert!(session.observe_with_dom().await.unwrap().dom.is_some());

    session
        .evaluate("document.querySelector('#result').textContent = 'Changed'")
        .await
        .unwrap();
    assert!(session.observe().await.unwrap().text.contains("Changed"));

    let snapshot = session.snapshot().await.unwrap();
    assert!(
        snapshot
            .interactive
            .iter()
            .any(|element| element.name == "Save")
    );
    assert!(
        snapshot
            .interactive
            .iter()
            .any(|element| element.name == "Name")
    );

    session.type_text("Ada", Some("Name")).await.unwrap();
    session.evaluate("window.pointerEvents = []").await.unwrap();
    session.click("Save").await.unwrap();
    let pointer_events = session.evaluate("window.pointerEvents").await.unwrap();
    let pointer_events = pointer_events.as_array().unwrap();
    assert!(
        pointer_events
            .iter()
            .filter(|event| event["type"] == "mousemove")
            .count()
            > 1
    );
    assert_eq!(
        pointer_events[pointer_events.len() - 2]["type"],
        "mousedown"
    );
    assert_eq!(pointer_events[pointer_events.len() - 1]["type"], "mouseup");
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        "Saved Ada"
    );
    session
        .evaluate("localStorage.setItem('glass-incognito', 'private')")
        .await
        .unwrap();
    let visual_context = session.observe_with_screenshot().await.unwrap();
    assert!(visual_context.screenshot.is_some());
    assert!(session.observe().await.unwrap().screenshot.is_none());
    let screenshot = session.screenshot_png().await.unwrap();
    assert!(screenshot.len() > 100);
    assert!(screenshot.starts_with(b"\x89PNG\r\n\x1a\n"));

    let attached_session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: None,
        profile: "default".to_string(),
        incognito: false,
        attach: true,
        target_id: Some(page_target_id(port).await),
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    assert!(attached_session.is_attached());
    assert!(!attached_session.owns_chrome());
    attached_session.close().await.unwrap();
    assert_eq!(
        session.evaluate("document.title").await.unwrap(),
        "Glass Fixture"
    );

    session.close().await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fast_session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "e2e-fast".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    fast_session.navigate(&url).await.unwrap();
    assert_eq!(
        fast_session
            .evaluate("localStorage.getItem('glass-incognito')")
            .await
            .unwrap(),
        Value::Null
    );
    fast_session
        .evaluate("window.pointerEvents = []")
        .await
        .unwrap();
    fast_session.click("Save").await.unwrap();
    let pointer_events = fast_session.evaluate("window.pointerEvents").await.unwrap();
    let pointer_events = pointer_events.as_array().unwrap();
    assert_eq!(
        pointer_events
            .iter()
            .filter(|event| event["type"] == "mousemove")
            .count(),
        1
    );
    assert_eq!(
        pointer_events[pointer_events.len() - 2]["type"],
        "mousedown"
    );
    assert_eq!(pointer_events[pointer_events.len() - 1]["type"], "mouseup");
    fast_session.close().await.unwrap();
    fixture_server.close().await;
}

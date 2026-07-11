use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{BrowserSession, InteractionMode, SessionOptions};
use tokio::net::TcpListener;

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

    let html = include_str!("fixtures/basic.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(html));
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path.clone()),
        profile: "e2e".to_string(),
        incognito: true,
        headed: false,
        interaction_mode: InteractionMode::Human,
    })
    .await
    .unwrap();

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
    let visual_context = session.observe_with_screenshot().await.unwrap();
    assert!(visual_context.screenshot.is_some());
    assert!(session.observe().await.unwrap().screenshot.is_none());
    let screenshot = session.screenshot_png().await.unwrap();
    assert!(screenshot.len() > 100);
    assert!(screenshot.starts_with(b"\x89PNG\r\n\x1a\n"));
    session.close().await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fast_session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "e2e-fast".to_string(),
        incognito: true,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    fast_session.navigate(&url).await.unwrap();
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
}

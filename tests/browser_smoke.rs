use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{BrowserSession, SessionOptions};
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
        chrome_path: Some(chrome_path),
        profile: "e2e".to_string(),
        incognito: true,
        headed: false,
    })
    .await
    .unwrap();

    let page = session.navigate(&url).await.unwrap();
    assert_eq!(page.title, "Glass Fixture");
    assert!(session.text().await.unwrap().contains("Glass Fixture"));

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
    session.click("Save").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        "Saved Ada"
    );
    assert!(session.screenshot_png().await.unwrap().len() > 100);
    session.close().await.unwrap();
}

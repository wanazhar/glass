use super::*;

#[test]
fn native_cross_document_adoption_preserves_closed_shadow_root_identity() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(tokio::task::LocalSet::new().run_until(
            native_cross_document_adoption_preserves_closed_shadow_root_identity_inner(),
        ));
    });
}

async fn native_cross_document_adoption_preserves_closed_shadow_root_identity_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native shadow adoption request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<!doctype html><x-shadow-host id='moving'>LIGHT_TEXT</x-shadow-host><iframe id='child' src='/child'></iframe>"
                }
                Some("/child") => "<!doctype html><main id='target'></main>",
                other => panic!("unexpected native shadow adoption path: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let attached = session
        .script(
            r##"(() => {
              const host = document.getElementById('moving');
              const root = host.attachShadow({ mode: 'closed' });
              const label = document.createElement('span');
              label.textContent = 'CLOSED_SHADOW_ADOPTED';
              root.appendChild(label);
              globalThis.__adoptedShadowHost = host;
              globalThis.__adoptedClosedRoot = root;
              globalThis.__adoptedShadowLabel = label;
              return [root.host === host, host.shadowRoot === null, root.mode === 'closed'];
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(attached.value, serde_json::json!([true, true, true]));

    let result = session
        .script(
            r##"(() => {
              const host = globalThis.__adoptedShadowHost;
              const root = globalThis.__adoptedClosedRoot;
              const label = globalThis.__adoptedShadowLabel;
              const targetDocument = document.getElementById('child').contentDocument;
              const adopted = targetDocument.adoptNode(host);
              targetDocument.getElementById('target').appendChild(adopted);
              return [adopted === host, root.host === host, host.shadowRoot === null,
                root.mode === 'closed', root.querySelector('span') === label,
                targetDocument.getElementById('target').textContent === 'LIGHT_TEXT'];
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!([true, true, true, true, true, true])
    );

    let refreshed = session
        .script(
            r##"(() => {
              const host = globalThis.__adoptedShadowHost;
              const root = globalThis.__adoptedClosedRoot;
              const target = document.getElementById('child').contentDocument.getElementById('target');
              return [target.firstElementChild === host, root.host === host,
                host.shadowRoot === null, root.mode === 'closed',
                root.querySelector('span')?.textContent === 'CLOSED_SHADOW_ADOPTED'];
            })()"##,
        )
        .await
        .unwrap();
    assert_eq!(
        refreshed.value,
        serde_json::json!([true, true, true, true, true])
    );

    server.abort();
}

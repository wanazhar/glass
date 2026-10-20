use super::*;

#[tokio::test]
async fn native_content_process_xhr_stream_abort_cancels_buffered_response_reader() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/abort-stream"] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("timed out waiting for the aborting XHR request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/page" {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 12\r\nConnection: close\r\n\r\n<p>abort</p>",
                    )
                    .await
                    .unwrap();
                continue;
            }
            // The parent broker has already buffered this body before the
            // content process exposes it through the XHR stream adapter.
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 12\r\nConnection: close\r\n\r\nfirst-",
                )
                .await
                .unwrap();
            stream.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = stream.write_all(b"second").await;
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                globalThis.streamingAbortPromise = new Promise(resolve => {
                    const xhr = new XMLHttpRequest();
                    const events = [];
                    xhr.onreadystatechange = () => events.push(`state:${xhr.readyState}`);
                    xhr.onprogress = event => {
                        events.push(`progress:${event.loaded}`);
                        xhr.abort();
                    };
                    xhr.onload = () => events.push('load');
                    xhr.onerror = () => events.push('error');
                    xhr.onabort = () => events.push(`abort:${xhr.readyState}`);
                    xhr.onloadend = () => resolve(events);
                    xhr.open('GET', '/abort-stream');
                    xhr.send();
                });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await streamingAbortPromise")
            .await
            .unwrap(),
        serde_json::json!([
            "state:1",
            "state:2",
            "state:3",
            "progress:12",
            "state:0",
            "abort:0"
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_reopen_cancels_buffered_response_reader() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/stream-reopen", "/replacement"] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("timed out waiting for the reopening XHR request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            match expected_path {
                "/page" => {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 13\r\nConnection: close\r\n\r\n<p>reopen</p>",
                        )
                        .await
                        .unwrap();
                }
                "/stream-reopen" => {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 12\r\nConnection: close\r\n\r\nfirst-",
                        )
                        .await
                        .unwrap();
                    stream.flush().await.unwrap();
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let _ = stream.write_all(b"second").await;
                }
                "/replacement" => {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\nConnection: close\r\n\r\nnew-body",
                        )
                        .await
                        .unwrap();
                }
                _ => unreachable!(),
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                globalThis.reopenXhrPromise = new Promise(resolve => {
                    const xhr = new XMLHttpRequest();
                    let reopened = false;
                    const events = [];
                    xhr.onprogress = event => {
                        events.push(`progress:${event.loaded}:${xhr.responseText}`);
                        if (!reopened) {
                            reopened = true;
                            xhr.open('GET', '/replacement');
                            xhr.send();
                        }
                    };
                    xhr.onload = () => resolve({ events, text: xhr.responseText });
                    xhr.onerror = () => resolve({ error: 'reopen-xhr' });
                    xhr.open('GET', '/stream-reopen');
                    xhr.send();
                });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await reopenXhrPromise")
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["progress:12:first-second", "progress:8:new-body"],
            "text": "new-body",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

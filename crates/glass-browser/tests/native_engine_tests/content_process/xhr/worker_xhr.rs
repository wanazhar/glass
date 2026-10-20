use super::*;

#[tokio::test]
async fn native_content_process_worker_xhr_upload_reports_buffered_progress() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/worker-upload-page", "/worker-upload.js", "/worker-upload"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_bytes(&mut stream).await;
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .unwrap();
            let headers = String::from_utf8_lossy(&request[..header_end]);
            assert_eq!(headers.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/worker-upload" {
                assert_eq!(headers.split_whitespace().next(), Some("POST"));
                assert_eq!(&request[header_end..], [0_u8, 255, 128, 65]);
            }
            let (content_type, body) = match expected_path {
                "/worker-upload-page" => (
                    "text/html",
                    "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-upload.js'); worker.onmessage = event => workerMessages.push(event.data);</script>",
                ),
                "/worker-upload.js" => (
                    "text/javascript",
                    r#"(() => {
  const xhr = new XMLHttpRequest();
  const events = [];
  const xhrEvents = [];
  const record = event => events.push([
    event.type,
    event instanceof ProgressEvent,
    event.target === xhr.upload,
    event.currentTarget === xhr.upload,
    event.lengthComputable,
    event.loaded,
    event.total,
  ]);
  xhr.upload.addEventListener('loadstart', record);
  xhr.upload.onprogress = record;
  xhr.upload.addEventListener('load', record);
  xhr.upload.addEventListener('loadend', record);
  xhr.addEventListener('loadstart', event => xhrEvents.push([
    event.type,
    event instanceof ProgressEvent,
    event.target === xhr,
    event.currentTarget === xhr,
    event.lengthComputable,
    event.loaded,
    event.total,
  ]));
  xhr.open('POST', '/worker-upload');
  xhr.onload = () => postMessage({
    uploadIdentity: xhr.upload instanceof XMLHttpRequestUpload,
    events,
    xhrEvents,
    response: xhr.responseText,
  });
  xhr.onerror = error => postMessage({ error: String(error) });
  xhr.send(new Uint8Array([0, 255, 128, 65]));
})();"#,
                ),
                "/worker-upload" => ("text/plain", "worker-upload-response"),
                _ => unreachable!(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-upload-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "uploadIdentity": true,
            "events": [
                ["loadstart", true, true, true, true, 0, 4],
                ["progress", true, true, true, true, 4, 4],
                ["load", true, true, true, true, 4, 4],
                ["loadend", true, true, true, true, 4, 4],
            ],
            "xhrEvents": [["loadstart", true, true, true, false, 0, 0]],
            "response": "worker-upload-response",
        }])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_xhr_reports_response_progress() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/worker-xhr-stream-page",
            "/worker-xhr-stream.js",
            "/worker-xhr-stream",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            match expected_path {
                "/worker-xhr-stream" => {
                    let chunks: &[&[u8]] = &[b"first-", &[0xf0, 0x9f], &[0x92, 0xa9], b"-last"];
                    let body_len = chunks.iter().map(|chunk| chunk.len()).sum::<usize>();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    for chunk in chunks {
                        stream.write_all(chunk).await.unwrap();
                        stream.flush().await.unwrap();
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
                "/worker-xhr-stream-page" => {
                    let body = "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-xhr-stream.js'); worker.onmessage = event => workerMessages.push(event.data);</script>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/worker-xhr-stream.js" => {
                    let body = r#"(() => { const xhr = new XMLHttpRequest(); const states = []; const progress = []; xhr.onreadystatechange = () => states.push([xhr.readyState, xhr.responseText]); xhr.onprogress = event => progress.push([event instanceof ProgressEvent, event.loaded, event.total]); xhr.onload = () => postMessage({ kind: 'stream', states, progress, responseText: xhr.responseText, response: xhr.response }); xhr.onerror = error => postMessage({ kind: 'error', message: String(error) }); xhr.open('GET', '/worker-xhr-stream'); xhr.send(); })();"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                _ => unreachable!(),
            }
        }
    });
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{address}/worker-xhr-stream-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut messages = serde_json::Value::Array(Vec::new());
    for _ in 0..16 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages.as_array().is_some_and(|values| !values.is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    assert!(
        messages.as_array().is_some_and(|values| !values.is_empty()),
        "worker messages: {messages}"
    );
    let message = &messages[0];
    assert_eq!(message["kind"], "stream");
    assert_eq!(message["responseText"], "first-💩-last");
    assert_eq!(message["response"], "first-💩-last");
    let states = message["states"].as_array().unwrap();
    assert_eq!(states.first().unwrap()[0], 1);
    assert!(states.iter().any(|state| state[0] == 2));
    assert!(states.iter().any(|state| state[0] == 3));
    assert_eq!(states.last().unwrap()[0], 4);
    let progress = message["progress"].as_array().unwrap();
    assert!(!progress.is_empty());
    let mut previous_loaded = 0_u64;
    for event in progress {
        assert_eq!(event[0], true);
        let loaded = event[1].as_u64().unwrap();
        assert!(loaded >= previous_loaded);
        assert_eq!(event[2], 15);
        previous_loaded = loaded;
    }
    assert_eq!(previous_loaded, 15);
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

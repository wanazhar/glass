use super::*;

#[tokio::test]
async fn native_content_process_xhr_upload_reports_buffered_progress() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/upload"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request_bytes(&mut stream).await;
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .unwrap();
            let path = request[..header_end]
                .split(|byte| *byte == b' ')
                .nth(1)
                .unwrap();
            assert_eq!(path, expected_path.as_bytes());
            if expected_path == "/upload" {
                assert_eq!(
                    request.split(|byte| *byte == b' ').next(),
                    Some(&b"POST"[..])
                );
                assert_eq!(&request[header_end..], [0_u8, 255, 128, 65]);
            }
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<p>XHR upload</p>")
            } else {
                ("text/plain", "upload-response")
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
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
                globalThis.uploadResultPromise = new Promise(resolve => {
                    const xhr = new XMLHttpRequest();
                    const uploadEvents = [];
                    const xhrEvents = [];
                    const record = event => uploadEvents.push([
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
                    xhr.addEventListener('load', () => xhrEvents.push('load'));
                    xhr.addEventListener('loadend', () => resolve({
                        uploadIdentity: xhr.upload instanceof XMLHttpRequestUpload,
                        uploadEvents,
                        xhrEvents,
                        response: xhr.responseText,
                    }));
                    xhr.open('POST', '/upload');
                    xhr.send(new Uint8Array([0, 255, 128, 65]));
                });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await uploadResultPromise")
            .await
            .unwrap(),
        serde_json::json!({
            "uploadIdentity": true,
            "uploadEvents": [
                ["loadstart", true, true, true, true, 0, 4],
                ["progress", true, true, true, true, 4, 4],
                ["load", true, true, true, true, 4, 4],
                ["loadend", true, true, true, true, 4, 4],
            ],
            "xhrEvents": [["loadstart", true, true, true, false, 0, 0], "load"],
            "response": "upload-response",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

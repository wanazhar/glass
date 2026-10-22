use super::*;

#[tokio::test]
async fn native_content_process_worker_exposes_xhr_fetch_bridge() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/worker-xhr-page",
            "/worker-xhr.js",
            "/worker-xhr",
            "/worker-xhr-binary",
            "/worker-xhr-timeout",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request_bytes = read_http_request_bytes(&mut stream).await;
            assert!(
                !request_bytes.is_empty(),
                "expected request for {expected_path}, received an empty connection"
            );
            let header_end = request_bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .unwrap();
            let request = String::from_utf8_lossy(&request_bytes);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/worker-xhr" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            && value.trim() == "application/json"
                    })
                }));
                assert_eq!(&request_bytes[header_end..], br#"{"name":"worker"}"#);
            }
            let (content_type, body) = match expected_path {
                "/worker-xhr-page" => (
                    "text/html",
                    "<script>globalThis.workerMessages = []; globalThis.worker = new Worker('/worker-xhr.js'); worker.onmessage = event => workerMessages.push(event.data);</script>",
                ),
                "/worker-xhr.js" => (
                    "text/javascript",
                    r#"(() => {
  const xhr = new XMLHttpRequest();
  const states = [];
  xhr.onreadystatechange = () => states.push(xhr.readyState);
  xhr.open('POST', '/worker-xhr');
  xhr.setRequestHeader('Content-Type', 'application/json');
  xhr.onload = () => {
    const binary = new XMLHttpRequest();
    binary.responseType = 'arraybuffer';
    binary.open('GET', '/worker-xhr-binary');
    binary.onload = () => {
      const timeout = new XMLHttpRequest();
      const timeoutStates = [];
      timeout.onreadystatechange = () => timeoutStates.push(timeout.readyState);
      timeout.ontimeout = () => postMessage({
        kind: 'xhr',
        identity: xhr instanceof XMLHttpRequest,
        states,
        status: xhr.status,
        statusText: xhr.statusText,
        responseText: xhr.responseText,
        responseURL: xhr.responseURL,
        responseHeader: xhr.getResponseHeader('x-worker-response'),
        binaryIdentity: binary.response instanceof ArrayBuffer,
        binaryBytes: Array.from(new Uint8Array(binary.response)),
        binaryStatus: binary.status,
        binaryURL: binary.responseURL,
        timeout: [timeout.timeout, timeoutStates, timeout.readyState, timeout.status, timeout.responseText]
      });
      timeout.onerror = event => postMessage({ kind: 'error', stage: 'timeout', eventType: event.type, errorName: event.error && event.error.name, message: String(event.error || event) });
      timeout.open('GET', '/worker-xhr-timeout');
      timeout.timeout = 20;
      timeout.send();
    };
    binary.onerror = error => postMessage({ kind: 'error', stage: 'binary', message: String(error) });
    binary.send();
  };
  xhr.onerror = () => postMessage({ kind: 'error', stage: 'text' });
  xhr.send('{"name":"worker"}');
})();"#,
                ),
                "/worker-xhr" => ("text/plain", "worker-xhr-response"),
                "/worker-xhr-binary" => ("application/octet-stream", "\u{0000}\u{00ff}\u{0080}A"),
                "/worker-xhr-timeout" => ("text/plain", "late-response"),
                _ => unreachable!(),
            };
            if expected_path == "/worker-xhr-timeout" {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if expected_path == "/worker-xhr-binary" {
                let binary_body = [0_u8, 255, 128, b'A'];
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nX-Worker-Response: binary\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    binary_body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.write_all(&binary_body).await.unwrap();
            } else {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nX-Worker-Response: text\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/worker-xhr-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "kind": "xhr",
            "identity": true,
            "states": [1, 2, 3, 4],
            "status": 200,
            "statusText": "OK",
            "responseText": "worker-xhr-response",
            "responseURL": format!("http://{address}/worker-xhr"),
            "responseHeader": "text",
            "binaryIdentity": true,
            "binaryBytes": [0, 255, 128, 65],
            "binaryStatus": 200,
            "binaryURL": format!("http://{address}/worker-xhr-binary"),
            "timeout": [20, [1, 4], 4, 0, ""],
        }])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_page_and_worker_support_sync_xhr() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/sync-xhr-page",
            "/sync-xhr",
            "/sync-xhr-worker.js",
            "/sync-xhr-worker",
            "/sync-xhr-later",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request_bytes = read_http_request_bytes(&mut stream).await;
            let header_end = request_bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .unwrap();
            let request = String::from_utf8_lossy(&request_bytes);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            match expected_path {
                "/sync-xhr" | "/sync-xhr-worker" => {
                    assert_eq!(request.split_whitespace().next(), Some("POST"));
                    assert!(request.lines().any(|line| {
                        line.split_once(':').is_some_and(|(name, value)| {
                            name.eq_ignore_ascii_case("content-type")
                                && value.trim() == "text/plain"
                        })
                    }));
                    let expected_client = if expected_path == "/sync-xhr" {
                        "page, second"
                    } else {
                        "worker, second"
                    };
                    assert!(request.lines().any(|line| {
                        line.split_once(':').is_some_and(|(name, value)| {
                            name.eq_ignore_ascii_case("x-sync-client")
                                && value.trim() == expected_client
                        })
                    }));
                    assert!(request.lines().any(|line| {
                        line.split_once(':').is_some_and(|(name, value)| {
                            name.eq_ignore_ascii_case("accept") && value.trim() == "text/plain"
                        })
                    }));
                    let body = &request_bytes[header_end..];
                    let expected_body = if expected_path == "/sync-xhr" {
                        b"page-body".as_slice()
                    } else {
                        b"worker-body".as_slice()
                    };
                    assert_eq!(body, expected_body);
                }
                _ => assert_eq!(request.split_whitespace().next(), Some("GET")),
            }
            let (content_type, body) = match expected_path {
                "/sync-xhr-page" => (
                    "text/html",
                    "<script>globalThis.syncPageStates = []; const xhr = new XMLHttpRequest(); xhr.onreadystatechange = () => syncPageStates.push(xhr.readyState); xhr.open('POST', '/sync-xhr', false); xhr.setRequestHeader('Content-Type', 'text/plain'); xhr.setRequestHeader('X-Sync-Client', 'page'); xhr.setRequestHeader('X-Sync-Client', 'second'); xhr.setRequestHeader('Accept', 'text/plain'); xhr.send('page-body'); globalThis.syncPageResult = { status: xhr.status, statusText: xhr.statusText, responseText: xhr.responseText, response: xhr.response, responseURL: xhr.responseURL, responseHeader: xhr.getResponseHeader('x-sync'), constants: [XMLHttpRequest.UNSENT, XMLHttpRequest.OPENED, XMLHttpRequest.HEADERS_RECEIVED, XMLHttpRequest.LOADING, XMLHttpRequest.DONE, xhr.UNSENT, xhr.OPENED, xhr.HEADERS_RECEIVED, xhr.LOADING, xhr.DONE], states: syncPageStates, readyState: xhr.readyState }; globalThis.workerMessages = []; globalThis.worker = new Worker('/sync-xhr-worker.js'); worker.onmessage = event => workerMessages.push(event.data);</script>",
                ),
                "/sync-xhr" => ("text/plain", "sync-page-response"),
                "/sync-xhr-worker.js" => (
                    "text/javascript",
                    "(() => { const xhr = new XMLHttpRequest(); const states = []; xhr.onreadystatechange = () => states.push(xhr.readyState); xhr.open('POST', '/sync-xhr-worker', false); xhr.setRequestHeader('Content-Type', 'text/plain'); xhr.setRequestHeader('X-Sync-Client', 'worker'); xhr.setRequestHeader('X-Sync-Client', 'second'); xhr.setRequestHeader('Accept', 'text/plain'); xhr.send('worker-body'); postMessage({ status: xhr.status, statusText: xhr.statusText, responseText: xhr.responseText, responseURL: xhr.responseURL, responseHeader: xhr.getResponseHeader('x-sync'), constants: [XMLHttpRequest.UNSENT, XMLHttpRequest.OPENED, XMLHttpRequest.HEADERS_RECEIVED, XMLHttpRequest.LOADING, XMLHttpRequest.DONE, xhr.UNSENT, xhr.OPENED, xhr.HEADERS_RECEIVED, xhr.LOADING, xhr.DONE], states, readyState: xhr.readyState }); })();",
                ),
                "/sync-xhr-worker" => ("text/plain", "sync-worker-response"),
                "/sync-xhr-later" => ("text/plain", "sync-later-response"),
                _ => unreachable!(),
            };
            let sync_header = if expected_path == "/sync-xhr" {
                "page"
            } else if expected_path == "/sync-xhr-worker" {
                "worker"
            } else {
                ""
            };
            let response = if sync_header.is_empty() {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nX-Sync: {sync_header}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/sync-xhr-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("syncPageResult").await.unwrap(),
        serde_json::json!({
            "status": 200,
            "statusText": "OK",
            "responseText": "sync-page-response",
            "response": "sync-page-response",
            "responseURL": format!("http://{address}/sync-xhr"),
            "responseHeader": "page",
            "constants": [0, 1, 2, 3, 4, 0, 1, 2, 3, 4],
            "states": [1, 4],
            "readyState": 4,
        })
    );
    assert_eq!(
        engine.evaluate_async("workerMessages").await.unwrap(),
        serde_json::json!([{
            "status": 200,
            "statusText": "OK",
            "responseText": "sync-worker-response",
            "responseURL": format!("http://{address}/sync-xhr-worker"),
            "responseHeader": "worker",
            "constants": [0, 1, 2, 3, 4, 0, 1, 2, 3, 4],
            "states": [1, 4],
            "readyState": 4,
        }])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const xhr = new XMLHttpRequest(); xhr.open('GET', '/sync-xhr-later', false); let bodyError = ''; try { xhr.send(new ReadableStream({ start(controller) { controller.close(); } })); } catch (error) { bodyError = error.name; } xhr.send(); let completeHeaderError = ''; try { xhr.setRequestHeader('X-After', 'value'); } catch (error) { completeHeaderError = error.name; } return [xhr.status, xhr.responseText, xhr.responseURL, xhr.readyState, bodyError, completeHeaderError, [XMLHttpRequest.UNSENT, XMLHttpRequest.OPENED, XMLHttpRequest.HEADERS_RECEIVED, XMLHttpRequest.LOADING, XMLHttpRequest.DONE]]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            200,
            "sync-later-response",
            format!("http://{address}/sync-xhr-later"),
            4,
            "TypeError",
            "TypeError",
            [0, 1, 2, 3, 4],
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_exposes_status_text_and_safe_response_headers() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (status, reason, content_type, body) = match path {
                "/headers-page" => (
                    200,
                    "OK",
                    "text/html",
                    r#"<script>
globalThis.pageHeaderPromise = new Promise(resolve => {
  const xhr = new XMLHttpRequest();
  const identity = [
    xhr instanceof XMLHttpRequest,
    xhr instanceof EventTarget,
    Object.getPrototypeOf(XMLHttpRequest.prototype) === EventTarget.prototype,
    xhr.addEventListener === EventTarget.prototype.addEventListener,
    xhr.upload instanceof XMLHttpRequestUpload,
    xhr.upload instanceof EventTarget,
    Object.getPrototypeOf(XMLHttpRequestUpload.prototype) === EventTarget.prototype,
    xhr.upload.addEventListener === EventTarget.prototype.addEventListener,
  ];
  const readOnly = (() => {
    const probe = new XMLHttpRequest();
    const upload = probe.upload;
    const attempt = (name, value) => {
      try { probe[name] = value; } catch (_) {}
    };
    attempt('readyState', 99);
    attempt('status', 599);
    attempt('statusText', 'changed');
    attempt('responseURL', 'changed');
    attempt('response', 'changed');
    attempt('upload', null);
    const descriptor = name => Object.getOwnPropertyDescriptor(XMLHttpRequest.prototype, name);
    return [
      probe.readyState === 0,
      probe.status === 0,
      probe.statusText === '',
      probe.responseURL === '',
      probe.response === '',
      probe.upload === upload,
      !Object.prototype.hasOwnProperty.call(probe, 'status'),
      ['readyState', 'status', 'statusText', 'responseURL', 'response', 'upload']
        .every(name => descriptor(name).set === undefined && descriptor(name).enumerable === false),
    ];
  })();
  const synthetic = new XMLHttpRequest();
  const dispatchEvents = [];
  synthetic.onload = () => dispatchEvents.push('handler');
  synthetic.addEventListener('load', () => dispatchEvents.push('listener'));
  const dispatchResult = synthetic.dispatchEvent(new Event('load'));
  xhr.onload = () => {
    let invalid = '';
    try { xhr.getResponseHeader('bad header'); } catch (error) { invalid = error.name; }
    resolve({
      status: xhr.status,
      statusText: xhr.statusText,
      identity,
      readOnly,
      dispatch: [dispatchResult, dispatchEvents],
      multi: xhr.getResponseHeader('X-Multi'),
      hidden: xhr.getResponseHeader('set-cookie'),
      invalid,
      safe: xhr.getAllResponseHeaders().includes('set-cookie') === false,
      combined: xhr.getAllResponseHeaders().includes('x-multi: first, second\r\n'),
    });
  };
  xhr.onerror = () => resolve({ error: 'page-xhr' });
  xhr.open('GET', '/headers');
  xhr.send();
});
globalThis.workerHeaderPromise = new Promise(resolve => {
  globalThis.headerWorker = new Worker('/headers-worker.js');
  headerWorker.onmessage = event => resolve(event.data);
});
</script>"#,
                ),
                "/headers" => (201, "Created", "text/plain", "page-headers"),
                "/headers-worker.js" => (
                    200,
                    "OK",
                    "text/javascript",
                    r#"const xhr = new XMLHttpRequest();
const identity = [
  xhr instanceof XMLHttpRequest,
  xhr instanceof EventTarget,
  Object.getPrototypeOf(XMLHttpRequest.prototype) === EventTarget.prototype,
  xhr.addEventListener === EventTarget.prototype.addEventListener,
  xhr.upload instanceof XMLHttpRequestUpload,
  xhr.upload instanceof EventTarget,
  Object.getPrototypeOf(XMLHttpRequestUpload.prototype) === EventTarget.prototype,
  xhr.upload.addEventListener === EventTarget.prototype.addEventListener,
];
const readOnly = (() => {
  const probe = new XMLHttpRequest();
  const upload = probe.upload;
  const attempt = (name, value) => {
    try { probe[name] = value; } catch (_) {}
  };
  attempt('readyState', 99);
  attempt('status', 599);
  attempt('statusText', 'changed');
  attempt('responseURL', 'changed');
  attempt('response', 'changed');
  attempt('upload', null);
  const descriptor = name => Object.getOwnPropertyDescriptor(XMLHttpRequest.prototype, name);
  return [
    probe.readyState === 0,
    probe.status === 0,
    probe.statusText === '',
    probe.responseURL === '',
    probe.response === '',
    probe.upload === upload,
    !Object.prototype.hasOwnProperty.call(probe, 'status'),
    ['readyState', 'status', 'statusText', 'responseURL', 'response', 'upload']
      .every(name => descriptor(name).set === undefined && descriptor(name).enumerable === false),
  ];
})();
const synthetic = new XMLHttpRequest();
const dispatchEvents = [];
synthetic.onload = () => dispatchEvents.push('handler');
synthetic.addEventListener('load', () => dispatchEvents.push('listener'));
const dispatchResult = synthetic.dispatchEvent(new Event('load'));
xhr.onload = () => {
  let invalid = '';
  try { xhr.getResponseHeader('bad header'); } catch (error) { invalid = error.name; }
  postMessage({
    status: xhr.status,
    statusText: xhr.statusText,
    readOnly,
    identity,
    dispatch: [dispatchResult, dispatchEvents],
    multi: xhr.getResponseHeader('x-multi'),
    hidden: xhr.getResponseHeader('set-cookie'),
    invalid,
    safe: xhr.getAllResponseHeaders().includes('set-cookie') === false,
    combined: xhr.getAllResponseHeaders().includes('x-multi: worker, second\r\n'),
  });
};
xhr.onerror = () => postMessage({ error: 'worker-xhr' });
xhr.open('GET', '/headers-worker');
xhr.send();"#,
                ),
                "/headers-worker" => (202, "Accepted", "text/plain", "worker-headers"),
                other => panic!("unexpected response-header request path: {other}"),
            };
            let extra_headers = match path {
                "/headers" => "X-Multi: first\r\nX-Multi: second\r\n",
                "/headers-worker" => "X-Multi: worker\r\nX-Multi: second\r\n",
                _ => "",
            };
            let set_cookie = if matches!(path, "/headers" | "/headers-worker") {
                "Set-Cookie: secret=not-page-readable\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n{extra_headers}{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/headers-page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("await Promise.all([pageHeaderPromise, workerHeaderPromise])")
            .await
            .unwrap(),
        serde_json::json!([
            {
                "status": 201,
                "statusText": "Created",
                "identity": [true, true, true, true, true, true, true, true],
                "readOnly": [true, true, true, true, true, true, true, true],
                "dispatch": [true, ["handler", "listener"]],
                "multi": "first, second",
                "hidden": null,
                "invalid": "SyntaxError",
                "safe": true,
                "combined": true,
            },
            {
                "status": 202,
                "statusText": "Accepted",
                "identity": [true, true, true, true, true, true, true, true],
                "readOnly": [true, true, true, true, true, true, true, true],
                "dispatch": [true, ["handler", "listener"]],
                "multi": "worker, second",
                "hidden": null,
                "invalid": "SyntaxError",
                "safe": true,
                "combined": true,
            }
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_revalidates_fetch_and_xhr_responses() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut data_requests = 0;
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            match path {
                "/page" => {
                    let body = "<title>Fetch cache</title><p>response cache</p>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/data" => {
                    data_requests += 1;
                    match data_requests {
                        1 => {
                            let body = "version-one";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: max-age=600\r\nETag: \"data-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            stream.write_all(response.as_bytes()).await.unwrap();
                        }
                        2 => {
                            assert!(request.lines().any(|line| {
                                line.eq_ignore_ascii_case("cache-control: no-cache")
                            }));
                            let body = "version-two";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: max-age=600\r\nETag: \"data-v2\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            stream.write_all(response.as_bytes()).await.unwrap();
                        }
                        3 => {
                            assert!(request.lines().any(|line| {
                                line.eq_ignore_ascii_case("cache-control: no-cache")
                            }));
                            assert!(request.lines().any(|line| {
                                line.eq_ignore_ascii_case("if-none-match: \"data-v2\"")
                            }));
                            stream
                                .write_all(
                                    b"HTTP/1.1 304 Not Modified\r\nCache-Control: max-age=600\r\nETag: \"data-v2\"\r\nConnection: close\r\n\r\n",
                                )
                                .await
                                .unwrap();
                        }
                        4 => {
                            assert!(request.lines().any(|line| {
                                line.eq_ignore_ascii_case("cache-control: no-store")
                            }));
                            let body = "network-only";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nETag: \"data-v3\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            stream.write_all(response.as_bytes()).await.unwrap();
                        }
                        other => panic!("unexpected data request {other}"),
                    }
                }
                other => panic!("unexpected request path: {other}"),
            }
        }
        assert_eq!(data_requests, 4);
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    // Exercise each cache mode in a separate bounded script turn; the cache and
    // validator state remain resident in this same document between calls.
    for (source, expected) in [
        (
            "await fetch('/data').then(response => response.text())",
            "version-one",
        ),
        (
            "await fetch('/data').then(response => response.text())",
            "version-one",
        ),
        (
            "await fetch('/data', { cache: 'reload' }).then(response => response.text())",
            "version-two",
        ),
        (
            "await fetch('/data', { cache: 'no-cache' }).then(response => response.text())",
            "version-two",
        ),
        (
            "await fetch('/data', { cache: 'no-store' }).then(response => response.text())",
            "network-only",
        ),
        (
            "await fetch('/data', { cache: 'force-cache' }).then(response => response.text())",
            "version-two",
        ),
        (
            "await fetch(new Request('/data', { mode: 'same-origin', cache: 'only-if-cached' })).then(response => response.text())",
            "version-two",
        ),
        (
            "await new Promise((resolve, reject) => { const request = new XMLHttpRequest(); request.onload = () => resolve(request.responseText); request.onerror = reject; request.open('GET', '/data'); request.withCredentials = true; request.send(); })",
            "version-two",
        ),
    ] {
        assert_eq!(
            engine.evaluate_async(source).await.unwrap(),
            serde_json::json!(expected),
            "evaluation should preserve cache behavior for {source}"
        );
    }
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_worker_fetch_and_xhr_use_worker_policy_container() {
    fn request_header(request: &str, name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let target_origin = format!("http://{target_address}");
    let policy_worker_url = format!("{page_origin}/worker-policy.js?token=worker");
    let shared_worker_url = format!("{page_origin}/worker-shared.js?token=shared");
    let page = r#"<script>
      globalThis.workerMessages = [];
      const policyWorker = new Worker('/worker-policy.js?token=worker');
      policyWorker.onmessage = event => workerMessages.push(event.data);
      const defaultWorker = new Worker('/worker-default.js');
      defaultWorker.onmessage = event => workerMessages.push(event.data);
      const invalidWorker = new Worker('/worker-invalid.js');
      invalidWorker.onmessage = event => workerMessages.push(event.data);
      const sharedWorker = new SharedWorker('/worker-shared.js?token=shared');
      sharedWorker.port.onmessage = event => workerMessages.push({ type: 'shared', value: event.data });
    </script>"#;
    let policy_worker = format!(
        r#"const target = '{target_origin}/record?case=';
          const sourceRequest = new Request(target + 'source-request', {{ referrerPolicy: 'no-referrer' }});
          const xhrAsync = new Promise(resolve => {{
            const xhr = new XMLHttpRequest();
            xhr.onload = () => resolve(xhr.status);
            xhr.onerror = () => resolve(0);
            xhr.open('GET', target + 'xhr-async');
            xhr.send();
          }});
          const xhrSync = new XMLHttpRequest();
          xhrSync.open('GET', target + 'xhr-sync', false);
          xhrSync.send();
          Promise.all([
            fetch(new Request(target + 'fetch-default')).then(response => response.text()),
            fetch(target + 'fetch-init', {{ referrerPolicy: 'origin' }}).then(response => response.text()),
            fetch(sourceRequest).then(response => response.text()),
            xhrAsync,
          ]).then(results => postMessage({{
            type: 'policy',
            requestPolicy: new Request(target + 'visible').referrerPolicy,
            sourceRequestPolicy: sourceRequest.referrerPolicy,
            results,
            xhrSyncStatus: xhrSync.status,
          }})).catch(error => postMessage({{ type: 'policy-error', message: String(error) }}));"#
    );
    let default_worker = format!(
        "fetch('{target_origin}/record?case=header-absent').then(response => postMessage({{ type: 'default', status: response.status }})).catch(error => postMessage({{ type: 'default-error', message: String(error) }}));"
    );
    let invalid_worker = format!(
        "fetch('{target_origin}/record?case=header-invalid').then(response => postMessage({{ type: 'invalid', status: response.status }})).catch(error => postMessage({{ type: 'invalid-error', message: String(error) }}));"
    );
    let shared_worker = format!(
        r#"onconnect = event => {{
          const port = event.ports[0];
          fetch('{target_origin}/record?case=shared').then(response => response.text())
            .then(value => port.postMessage(value))
            .catch(error => port.postMessage('error:' + String(error)));
        }};"#
    );
    let page_server = tokio::spawn(async move {
        for _ in 0..5 {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(30), page_listener.accept())
                    .await
                    .expect("Document and Worker scripts should be requested")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("Worker script request has a path")
                .split('?')
                .next()
                .unwrap_or_default();
            let (content_type, referrer_policy, body) = match path {
                "/page" => (
                    "text/html",
                    "Referrer-Policy: no-referrer\r\n",
                    page.to_owned(),
                ),
                "/worker-policy.js" => (
                    "text/javascript",
                    "Referrer-Policy: unsafe-url\r\n",
                    policy_worker.clone(),
                ),
                "/worker-default.js" => ("text/javascript", "", default_worker.clone()),
                "/worker-invalid.js" => (
                    "text/javascript",
                    "Referrer-Policy: unknown-policy\r\n",
                    invalid_worker.clone(),
                ),
                "/worker-shared.js" => (
                    "text/javascript",
                    "Referrer-Policy: unsafe-url\r\n",
                    shared_worker.clone(),
                ),
                other => panic!("unexpected Worker policy fixture request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\n{referrer_policy}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let target_server = tokio::spawn(async move {
        let mut seen = Vec::new();
        for _ in 0..8 {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(30), target_listener.accept())
                    .await
                    .expect("all Worker Fetch/XHR requests should reach the target")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("target request has a path")
                .to_owned();
            let case = path
                .split_once("case=")
                .map(|(_, value)| value.to_owned())
                .expect("target request includes its case marker");
            let referer = request_header(&request, "referer");
            let body = case.as_str();
            let response = format!(
                "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            seen.push((case, referer));
        }
        seen
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut messages = serde_json::Value::Null;
    for _ in 0..8 {
        messages = engine.evaluate_async("workerMessages").await.unwrap();
        if messages.as_array().is_some_and(|items| items.len() == 4) {
            break;
        }
    }
    assert_eq!(messages.as_array().map(Vec::len), Some(4), "{messages}");
    assert!(
        messages.as_array().unwrap().contains(&serde_json::json!({
            "type": "policy",
            "requestPolicy": "",
            "sourceRequestPolicy": "no-referrer",
            "results": ["fetch-default", "fetch-init", "source-request", 200],
            "xhrSyncStatus": 200,
        })),
        "{messages}"
    );
    assert!(
        messages.as_array().unwrap().contains(&serde_json::json!({
            "type": "default", "status": 200
        })),
        "{messages}"
    );
    assert!(
        messages.as_array().unwrap().contains(&serde_json::json!({
            "type": "invalid", "status": 200
        })),
        "{messages}"
    );
    assert!(
        messages.as_array().unwrap().contains(&serde_json::json!({
            "type": "shared", "value": "shared"
        })),
        "{messages}"
    );

    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    let seen = target_server.await.unwrap();
    let observed = seen.into_iter().collect::<BTreeMap<_, _>>();
    let worker_origin = format!("{page_origin}/");
    let expected_referers = [
        ("fetch-default", Some(policy_worker_url.as_str())),
        ("fetch-init", Some(worker_origin.as_str())),
        ("source-request", None),
        ("xhr-async", Some(policy_worker_url.as_str())),
        ("xhr-sync", Some(policy_worker_url.as_str())),
        ("header-absent", Some(worker_origin.as_str())),
        ("header-invalid", Some(worker_origin.as_str())),
        ("shared", Some(shared_worker_url.as_str())),
    ];
    for (case, expected) in expected_referers {
        assert_eq!(
            observed.get(case).and_then(Option::as_deref),
            expected,
            "case={case}"
        );
    }
}
#[tokio::test]
async fn native_content_process_synchronous_xhr_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut page_stream, _) = listener.accept().await.unwrap();
        let page_request = read_http_request(&mut page_stream).await;
        assert_eq!(page_request.split_whitespace().nth(1), Some("/page"));
        let body = "<script>try { document.cookie = 'before-xhr=visible; Path=/'; document.cookie = 'theme=before; Path=/'; const xhr = new XMLHttpRequest(); xhr.open('GET', '/sync', false); xhr.send(); globalThis.syncXhrSnapshot = [xhr.status, xhr.responseText, document.cookie]; } catch (error) { globalThis.syncXhrSnapshot = [0, String(error), document.cookie]; } globalThis.workerMessages = []; globalThis.worker = new Worker('/worker.js'); worker.onmessage = event => workerMessages.push(event.data);</script>".to_owned();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: theme=initial; Path=/\r\nSet-Cookie: secret=parent-only; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        page_stream.write_all(response.as_bytes()).await.unwrap();

        let (mut xhr_stream, _) = listener.accept().await.unwrap();
        let xhr_request = read_http_request(&mut xhr_stream).await;
        assert_eq!(xhr_request.split_whitespace().nth(1), Some("/sync"));
        let cookie = xhr_request
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            })
            .unwrap_or_default();
        assert!(cookie.contains("secret=parent-only"));
        assert!(cookie.contains("theme=before"));
        assert!(cookie.contains("before-xhr=visible"));
        let xhr_body = "parent-cookie-ok";
        let xhr_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nSet-Cookie: theme=after; Path=/\r\nSet-Cookie: xhr-visible=updated; Path=/\r\nSet-Cookie: xhr-secret=parent-only; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xhr_body}",
            xhr_body.len()
        );
        xhr_stream.write_all(xhr_response.as_bytes()).await.unwrap();

        let (mut worker_script_stream, _) = listener.accept().await.unwrap();
        let worker_script_request = read_http_request(&mut worker_script_stream).await;
        assert_eq!(
            worker_script_request.split_whitespace().nth(1),
            Some("/worker.js")
        );
        let worker_script = "try { const xhr = new XMLHttpRequest(); xhr.open('GET', '/worker-sync', false); xhr.withCredentials = true; xhr.send(); const asyncXhr = new XMLHttpRequest(); asyncXhr.onload = () => self.postMessage([xhr.status, xhr.responseText, asyncXhr.status, asyncXhr.responseText]); asyncXhr.onerror = () => self.postMessage(['error', 'worker async XHR failed']); asyncXhr.open('GET', '/worker-async'); asyncXhr.send(); } catch (error) { self.postMessage(['error', String(error)]); }";
        let worker_script_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{worker_script}",
            worker_script.len()
        );
        worker_script_stream
            .write_all(worker_script_response.as_bytes())
            .await
            .unwrap();

        let (mut worker_xhr_stream, _) = listener.accept().await.unwrap();
        let worker_xhr_request = read_http_request(&mut worker_xhr_stream).await;
        assert_eq!(
            worker_xhr_request.split_whitespace().nth(1),
            Some("/worker-sync")
        );
        let worker_cookie = worker_xhr_request
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            })
            .unwrap_or_default();
        assert!(worker_cookie.contains("secret=parent-only"));
        assert!(worker_cookie.contains("xhr-secret=parent-only"));
        let worker_xhr_body = "worker-parent-cookie-ok";
        let worker_xhr_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nSet-Cookie: worker-secret=parent-only; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{worker_xhr_body}",
            worker_xhr_body.len()
        );
        worker_xhr_stream
            .write_all(worker_xhr_response.as_bytes())
            .await
            .unwrap();

        let (mut worker_async_stream, _) = listener.accept().await.unwrap();
        let worker_async_request = read_http_request(&mut worker_async_stream).await;
        assert_eq!(
            worker_async_request.split_whitespace().nth(1),
            Some("/worker-async")
        );
        let worker_async_cookie = worker_async_request
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            })
            .unwrap_or_default();
        assert!(worker_async_cookie.contains("secret=parent-only"));
        assert!(worker_async_cookie.contains("xhr-secret=parent-only"));
        assert!(worker_async_cookie.contains("worker-secret=parent-only"));
        let worker_async_body = "worker-async-parent-cookie-ok";
        let worker_async_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nSet-Cookie: worker-async-visible=updated; Path=/\r\nSet-Cookie: worker-async-secret=parent-only; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{worker_async_body}",
            worker_async_body.len()
        );
        worker_async_stream
            .write_all(worker_async_response.as_bytes())
            .await
            .unwrap();

        let (mut page_async_stream, _) = listener.accept().await.unwrap();
        let page_async_request = read_http_request(&mut page_async_stream).await;
        assert_eq!(
            page_async_request.split_whitespace().nth(1),
            Some("/page-async")
        );
        let page_async_cookie = page_async_request
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            })
            .unwrap_or_default();
        assert!(page_async_cookie.contains("secret=parent-only"));
        assert!(page_async_cookie.contains("worker-async-secret=parent-only"));
        let page_async_body = "page-async-parent-cookie-ok";
        let page_async_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nSet-Cookie: page-async-visible=updated; Path=/\r\nSet-Cookie: page-async-secret=parent-only; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page_async_body}",
            page_async_body.len()
        );
        page_async_stream
            .write_all(page_async_response.as_bytes())
            .await
            .unwrap();

        let (mut followup_stream, _) = listener.accept().await.unwrap();
        let followup_request = read_http_request(&mut followup_stream).await;
        assert_eq!(followup_request.split_whitespace().nth(1), Some("/after"));
        let followup_cookie = followup_request
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            })
            .unwrap_or_default()
            .to_owned();
        let followup_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{followup_cookie}",
            followup_cookie.len()
        );
        followup_stream
            .write_all(followup_response.as_bytes())
            .await
            .unwrap();
        followup_cookie
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "[globalThis.syncXhrSnapshot[0], globalThis.syncXhrSnapshot[1], globalThis.syncXhrSnapshot[2].includes('theme=after'), globalThis.syncXhrSnapshot[2].includes('theme=before'), globalThis.syncXhrSnapshot[2].includes('before-xhr=visible'), globalThis.syncXhrSnapshot[2].includes('xhr-visible=updated'), globalThis.syncXhrSnapshot[2].includes('secret=')]",
            )
            .await
            .unwrap(),
        serde_json::json!([200, "parent-cookie-ok", true, false, true, true, false])
    );
    let mut worker_messages = serde_json::Value::Null;
    for _ in 0..20 {
        worker_messages = engine.evaluate_async("workerMessages").await.unwrap();
        if worker_messages == serde_json::json!([[200, "worker-parent-cookie-ok"]]) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        worker_messages,
        serde_json::json!([[
            200,
            "worker-parent-cookie-ok",
            200,
            "worker-async-parent-cookie-ok"
        ]])
    );
    let page_async = engine
        .evaluate_async(
            "await new Promise((resolve, reject) => { const xhr = new XMLHttpRequest(); xhr.onload = () => resolve({ status: xhr.status, responseText: xhr.responseText, cookie: document.cookie }); xhr.onerror = () => reject(new Error('page async XHR failed')); xhr.open('GET', '/page-async'); xhr.send(); })",
        )
        .await
        .unwrap();
    assert_eq!(page_async["status"], serde_json::json!(200));
    assert_eq!(
        page_async["responseText"],
        serde_json::json!("page-async-parent-cookie-ok")
    );
    assert!(
        page_async["cookie"]
            .as_str()
            .is_some_and(|cookie| cookie.contains("page-async-visible=updated"))
    );
    assert!(
        !page_async["cookie"]
            .as_str()
            .is_some_and(|cookie| cookie.contains("page-async-secret="))
    );
    let followup = engine
        .evaluate_async("await fetch('/after').then(response => response.text())")
        .await
        .unwrap();
    assert!(
        followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("secret=parent-only"))
    );
    assert!(
        followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("xhr-secret=parent-only"))
    );
    assert!(
        followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("xhr-visible=updated"))
    );
    assert!(
        followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("theme=after"))
    );
    assert!(
        !followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("theme=before"))
    );
    assert!(
        followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("worker-secret=parent-only"))
    );
    assert!(
        followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("worker-async-secret=parent-only"))
    );
    assert!(
        followup
            .as_str()
            .is_some_and(|cookie| cookie.contains("page-async-secret=parent-only"))
    );
    let cookies = engine.cookies_async().await.unwrap();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "xhr-secret" && cookie.http_only)
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "worker-secret" && cookie.http_only)
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "worker-async-secret" && cookie.http_only)
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.name == "page-async-secret" && cookie.http_only)
    );
    engine.close_async().await.unwrap();
    let _ = server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_cross_origin_credentials_remain_parent_owned() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let cross_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cross_address = cross_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let cross_origin = format!("http://{cross_address}");
    let worker_cross_origin = cross_origin.clone();
    let worker_script = format!(
        r#"
const crossOrigin = "{worker_cross_origin}";
self.onmessage = async event => {{
  if (event.data !== "run") return;
  const send = (path, withCredentials) => new Promise(resolve => {{
    const xhr = new XMLHttpRequest();
    xhr.withCredentials = withCredentials;
    xhr.onload = () => resolve([xhr.status, xhr.responseText]);
    xhr.onerror = () => resolve([0, "error"]);
    xhr.open("GET", crossOrigin + path);
    xhr.send();
  }});
  try {{
    const results = [];
    results.push(await send("/worker-default", false));
    results.push(await send("/worker-include", true));
    results.push(await send("/worker-include-after", true));
    results.push(await send("/worker-default-after", false));
    self.postMessage({{ kind: "complete", results }});
  }} catch (error) {{
    self.postMessage({{ kind: "error", message: String(error) }});
  }}
}};
"#
    );

    let page_server = tokio::spawn(async move {
        for expected_path in ["/page", "/xhr-cross-origin-worker.js"] {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
                    .await
                    .expect("page and worker script requests stay bounded")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            assert_eq!(path, expected_path);
            if expected_path == "/xhr-cross-origin-worker.js" {
                let cookie = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim())
                    .unwrap_or_default();
                assert!(cookie.contains("root=seed"));
            }
            let (content_type, headers, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "<main>cross-origin XHR cookie owner</main>".to_owned(),
                )
            } else {
                ("text/javascript", "", worker_script.clone())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let cross_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(45), async move {
            let expectations: [(&str, bool, &[&str], Option<&str>); 10] = [
                (
                    "/page-default",
                    false,
                    &[],
                    Some("page_default=blocked"),
                ),
                (
                    "/page-include",
                    true,
                    &["root=seed"],
                    Some("page_include=accepted"),
                ),
                (
                    "/page-include-after",
                    true,
                    &["root=seed", "page_include=accepted"],
                    None,
                ),
                (
                    "/page-default-after",
                    false,
                    &[],
                    Some("page_default_after=blocked"),
                ),
                (
                    "/page-sync-default",
                    false,
                    &[],
                    Some("page_sync_default=blocked"),
                ),
                (
                    "/page-sync-include",
                    true,
                    &["root=seed", "page_include=accepted"],
                    Some("page_sync_include=accepted"),
                ),
                (
                    "/worker-default",
                    false,
                    &[],
                    Some("worker_default=blocked"),
                ),
                (
                    "/worker-include",
                    true,
                    &[
                        "root=seed",
                        "page_include=accepted",
                        "page_sync_include=accepted",
                    ],
                    Some("worker_include=accepted"),
                ),
                (
                    "/worker-include-after",
                    true,
                    &[
                        "root=seed",
                        "page_include=accepted",
                        "page_sync_include=accepted",
                        "worker_include=accepted",
                    ],
                    None,
                ),
                (
                    "/worker-default-after",
                    false,
                    &[],
                    Some("worker_default_after=blocked"),
                ),
            ];
            let mut requests = Vec::new();
            for (path, include, expected_cookies, set_cookie) in expectations {
                let (mut stream, _) = cross_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().next(), Some("GET"));
                assert_eq!(request.split_whitespace().nth(1), Some(path));
                let origin = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("origin").then_some(value.trim())
                    })
                });
                assert_eq!(origin, Some(page_origin.as_str()));
                let cookie = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim().to_owned())
                    .unwrap_or_default();
                if include {
                    for expected_cookie in expected_cookies {
                        assert!(
                            cookie.contains(expected_cookie),
                            "{path} must carry {expected_cookie}; received {cookie:?}"
                        );
                    }
                } else {
                    assert!(
                        cookie.is_empty(),
                        "{path} must omit cross-origin cookies; received {cookie:?}"
                    );
                }
                let cors_headers = if include {
                    format!(
                        "Access-Control-Allow-Origin: {page_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                    )
                } else {
                    "Access-Control-Allow-Origin: *\r\n".to_owned()
                };
                let cookie_header = set_cookie
                    .map(|cookie| {
                        format!(
                            "Set-Cookie: {cookie}; HttpOnly; Path=/; SameSite=Lax\r\n"
                        )
                    })
                    .unwrap_or_default();
                let body = path;
                let response = format!(
                    "HTTP/1.1 200 OK\r\n{cors_headers}{cookie_header}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((path.to_owned(), cookie));
            }
            requests
        })
        .await
        .expect("cross-origin XHR requests stay bounded")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{page_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let page_async = tokio::time::timeout(
        Duration::from_secs(30),
        engine.evaluate_async(format!(
            r#"await (async () => {{
              const crossOrigin = "{cross_origin}";
              const send = (path, withCredentials) => new Promise(resolve => {{
                const xhr = new XMLHttpRequest();
                xhr.withCredentials = withCredentials;
                xhr.onload = () => resolve([xhr.status, xhr.responseText]);
                xhr.onerror = () => resolve([0, "error"]);
                xhr.open("GET", crossOrigin + path);
                xhr.send();
              }});
              const results = [];
              results.push(await send("/page-default", false));
              results.push(await send("/page-include", true));
              results.push(await send("/page-include-after", true));
              results.push(await send("/page-default-after", false));
              return results;
            }})()"#
        )),
    )
    .await
    .expect("page async credentialed XHRs stay bounded")
    .unwrap();
    assert_eq!(
        page_async,
        serde_json::json!([
            [200, "/page-default"],
            [200, "/page-include"],
            [200, "/page-include-after"],
            [200, "/page-default-after"],
        ])
    );

    let page_sync = engine
        .evaluate_async(format!(
            r#"(() => {{
              const crossOrigin = "{cross_origin}";
              const send = (path, withCredentials) => {{
                const xhr = new XMLHttpRequest();
                xhr.withCredentials = withCredentials;
                xhr.open("GET", crossOrigin + path, false);
                xhr.send();
                return [xhr.status, xhr.responseText];
              }};
              return [
                send("/page-sync-default", false),
                send("/page-sync-include", true),
              ];
            }})()"#
        ))
        .await
        .unwrap();
    assert_eq!(
        page_sync,
        serde_json::json!([[200, "/page-sync-default"], [200, "/page-sync-include"],])
    );

    engine
        .evaluate_async(
            "globalThis.crossOriginXhrWorkerMessages = []; globalThis.crossOriginXhrWorker = new Worker('/xhr-cross-origin-worker.js'); crossOriginXhrWorker.onmessage = event => crossOriginXhrWorkerMessages.push(event.data); crossOriginXhrWorker.postMessage('run'); true",
        )
        .await
        .unwrap();
    let worker_messages = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let messages = engine
                .evaluate_async("crossOriginXhrWorkerMessages")
                .await
                .unwrap();
            if messages.as_array().is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message["kind"] == "complete" || message["kind"] == "error")
            }) {
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cross-origin Worker XHR requests settle");
    assert_eq!(
        worker_messages,
        serde_json::json!([{
            "kind": "complete",
            "results": [
                [200, "/worker-default"],
                [200, "/worker-include"],
                [200, "/worker-include-after"],
                [200, "/worker-default-after"],
            ]
        }])
    );

    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    let requests = cross_server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        [
            "/page-default",
            "/page-include",
            "/page-include-after",
            "/page-default-after",
            "/page-sync-default",
            "/page-sync-include",
            "/worker-default",
            "/worker-include",
            "/worker-include-after",
            "/worker-default-after",
        ]
    );
    for (path, cookie) in &requests {
        if path.ends_with("default") || path.ends_with("default-after") {
            assert!(
                cookie.is_empty(),
                "{path} unexpectedly received credentials: {cookie:?}"
            );
        }
    }
    for (name, value) in [
        ("root", "seed"),
        ("page_include", "accepted"),
        ("page_sync_include", "accepted"),
        ("worker_include", "accepted"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.value == value && cookie.http_only)
        );
    }
    for name in [
        "page_default",
        "page_default_after",
        "page_sync_default",
        "worker_default",
        "worker_default_after",
    ] {
        assert!(
            !cookies.iter().any(|cookie| cookie.name == name),
            "cross-origin default credentials must reject {name}"
        );
    }
}
#[tokio::test]
async fn native_content_process_xhr_redirect_credentials_are_parent_owned_per_hop() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let cross_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cross_address = cross_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let cross_origin = format!("http://{cross_address}");
    let worker_cross_origin = cross_origin.clone();
    let page_server_cross_origin = cross_origin.clone();
    let cross_server_page_origin = page_origin.clone();
    let worker_script = format!(
        r#"
const crossOrigin = "{worker_cross_origin}";
self.onmessage = async event => {{
  if (event.data !== "run") return;
  const send = (path, withCredentials) => new Promise(resolve => {{
    const xhr = new XMLHttpRequest();
    xhr.withCredentials = withCredentials;
    xhr.onload = () => resolve([xhr.status, xhr.responseText, xhr.responseURL]);
    xhr.onerror = () => resolve([0, "error", xhr.responseURL]);
    xhr.open("GET", path);
    xhr.send();
  }});
  try {{
    const results = [];
    results.push(await send("/worker-default-redirect", false));
    results.push(await send("/worker-default-after", false));
    results.push(await send("/worker-include-redirect", true));
    results.push(await send(crossOrigin + "/worker-include-after", true));
    self.postMessage({{ kind: "complete", results }});
  }} catch (error) {{
    self.postMessage({{ kind: "error", message: String(error) }});
  }}
}};
self.postMessage({{ kind: "ready" }});
"#
    );

    let page_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(60), async move {
            let mut requests = Vec::new();
            for _ in 0..8 {
                let (mut stream, _) = page_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("page and Worker requests have a path")
                    .to_owned();
                let cookie = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim().to_owned())
                    .unwrap_or_default();
                match path.as_str() {
                    "/page" => assert!(cookie.is_empty()),
                    "/xhr-redirect-worker.js" | "/page-default-redirect" => {
                        assert!(cookie.contains("root=seed"), "{path}: {cookie:?}");
                    }
                    "/page-default-after" => {
                        assert!(cookie.contains("page_default_hop=accepted"));
                        assert!(!cookie.contains("page_default_cross="));
                    }
                    "/page-sync-include-redirect" => {
                        assert!(cookie.contains("page_default_hop=accepted"));
                    }
                    "/worker-default-redirect" | "/worker-include-redirect" => {
                        assert!(cookie.contains("page_sync_cross=accepted"));
                    }
                    "/worker-default-after" => {
                        assert!(cookie.contains("worker_default_hop=accepted"));
                        assert!(!cookie.contains("worker_default_cross="));
                    }
                    other => panic!("unexpected page-origin XHR request: {other}"),
                }

                let (status, extra_headers, content_type, body) = match path.as_str() {
                    "/page" => (
                        "200 OK",
                        "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n".to_owned(),
                        "text/html",
                        "<script>globalThis.xhrRedirectMessages = []; globalThis.xhrRedirectWorker = new Worker('/xhr-redirect-worker.js'); xhrRedirectWorker.onmessage = event => xhrRedirectMessages.push(event.data);</script>".to_owned(),
                    ),
                    "/xhr-redirect-worker.js" => {
                        ("200 OK", String::new(), "text/javascript", worker_script.clone())
                    }
                    "/page-default-redirect" => (
                        "302 Found",
                        format!(
                            "Location: {}/page-default-final\r\nSet-Cookie: page_default_hop=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                            page_server_cross_origin
                        ),
                        "text/plain",
                        String::new(),
                    ),
                    "/page-default-after" => (
                        "200 OK",
                        String::new(),
                        "text/plain",
                        "page-default-after".to_owned(),
                    ),
                    "/page-sync-include-redirect" => (
                        "302 Found",
                        format!(
                            "Location: {}/page-sync-include-final\r\nSet-Cookie: page_sync_hop=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                            page_server_cross_origin
                        ),
                        "text/plain",
                        String::new(),
                    ),
                    "/worker-default-redirect" => (
                        "302 Found",
                        format!(
                            "Location: {}/worker-default-final\r\nSet-Cookie: worker_default_hop=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                            page_server_cross_origin
                        ),
                        "text/plain",
                        String::new(),
                    ),
                    "/worker-default-after" => (
                        "200 OK",
                        String::new(),
                        "text/plain",
                        "worker-default-after".to_owned(),
                    ),
                    "/worker-include-redirect" => (
                        "302 Found",
                        format!(
                            "Location: {}/worker-include-final\r\nSet-Cookie: worker_include_hop=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                            page_server_cross_origin
                        ),
                        "text/plain",
                        String::new(),
                    ),
                    other => panic!("unexpected page-origin XHR route: {other}"),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((path, cookie));
            }
            requests
        })
        .await
        .expect("page and Worker redirect requests stay bounded")
    });

    let cross_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(60), async move {
            let expectations: [(&str, bool, &[&str], Option<&str>); 6] = [
                (
                    "/page-default-final",
                    false,
                    &[],
                    Some("page_default_cross=blocked"),
                ),
                (
                    "/page-sync-include-final",
                    true,
                    &["root=seed", "page_default_hop=accepted"],
                    Some("page_sync_cross=accepted"),
                ),
                (
                    "/page-sync-include-after",
                    true,
                    &["page_sync_hop=accepted", "page_sync_cross=accepted"],
                    None,
                ),
                (
                    "/worker-default-final",
                    false,
                    &[],
                    Some("worker_default_cross=blocked"),
                ),
                (
                    "/worker-include-final",
                    true,
                    &[
                        "root=seed",
                        "page_sync_cross=accepted",
                        "worker_default_hop=accepted",
                    ],
                    Some("worker_include_cross=accepted"),
                ),
                (
                    "/worker-include-after",
                    true,
                    &["worker_include_hop=accepted", "worker_include_cross=accepted"],
                    None,
                ),
            ];
            let mut requests = Vec::new();
            for (path, include, expected_cookies, set_cookie) in expectations {
                let (mut stream, _) = cross_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().next(), Some("GET"));
                assert_eq!(request.split_whitespace().nth(1), Some(path));
                let origin = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("origin").then_some(value.trim())
                    })
                });
                assert_eq!(origin, Some(cross_server_page_origin.as_str()));
                let cookie = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim().to_owned())
                    .unwrap_or_default();
                if include {
                    for expected_cookie in expected_cookies {
                        assert!(
                            cookie.contains(expected_cookie),
                            "{path} must carry {expected_cookie}; received {cookie:?}"
                        );
                    }
                } else {
                    assert!(
                        cookie.is_empty(),
                        "{path} must omit cross-origin cookies; received {cookie:?}"
                    );
                }
                let cors_headers = if include {
                    format!(
                        "Access-Control-Allow-Origin: {cross_server_page_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                    )
                } else {
                    "Access-Control-Allow-Origin: *\r\n".to_owned()
                };
                let cookie_header = set_cookie
                    .map(|cookie| {
                        format!(
                            "Set-Cookie: {cookie}; HttpOnly; Path=/; SameSite=Lax\r\n"
                        )
                    })
                    .unwrap_or_default();
                let body = path;
                let response = format!(
                    "HTTP/1.1 200 OK\r\n{cors_headers}{cookie_header}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((path.to_owned(), cookie));
            }
            requests
        })
        .await
        .expect("cross-origin XHR redirect hops stay bounded")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{page_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine.evaluate_async("xhrRedirectMessages").await.unwrap();
            if messages
                .as_array()
                .is_some_and(|messages| messages.iter().any(|message| message["kind"] == "ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker loads before XHR redirect coverage");

    let page_async = tokio::time::timeout(
        Duration::from_secs(30),
        engine.evaluate_async(
            r#"await (async () => {
              const send = (path, withCredentials) => new Promise(resolve => {
                const xhr = new XMLHttpRequest();
                xhr.withCredentials = withCredentials;
                xhr.onload = () => resolve([xhr.status, xhr.responseText, xhr.responseURL]);
                xhr.onerror = () => resolve([0, "error", xhr.responseURL]);
                xhr.open("GET", path);
                xhr.send();
              });
              return [
                await send("/page-default-redirect", false),
                await send("/page-default-after", false),
              ];
            })()"#,
        ),
    )
    .await
    .expect("page asynchronous redirect XHRs stay bounded")
    .unwrap();
    assert_eq!(
        page_async,
        serde_json::json!([
            [
                200,
                "/page-default-final",
                format!("{cross_origin}/page-default-final")
            ],
            [
                200,
                "page-default-after",
                format!("{page_origin}/page-default-after")
            ],
        ])
    );

    let page_sync = engine
        .evaluate_async(format!(
            r#"(() => {{
              const send = path => {{
                const xhr = new XMLHttpRequest();
                xhr.withCredentials = true;
                xhr.open("GET", path, false);
                xhr.send();
                return [xhr.status, xhr.responseText, xhr.responseURL];
              }};
              return [
                send("/page-sync-include-redirect"),
                send("{cross_origin}/page-sync-include-after"),
              ];
            }})()"#
        ))
        .await
        .unwrap();
    assert_eq!(
        page_sync,
        serde_json::json!([
            [
                200,
                "/page-sync-include-final",
                format!("{cross_origin}/page-sync-include-final")
            ],
            [
                200,
                "/page-sync-include-after",
                format!("{cross_origin}/page-sync-include-after")
            ],
        ])
    );

    engine
        .evaluate_async("xhrRedirectWorker.postMessage('run'); true")
        .await
        .unwrap();
    let worker_messages = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let messages = engine.evaluate_async("xhrRedirectMessages").await.unwrap();
            if messages.as_array().is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message["kind"] == "complete" || message["kind"] == "error")
            }) {
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Worker redirect XHR requests settle");
    assert_eq!(
        worker_messages,
        serde_json::json!([{"kind": "ready"}, {
            "kind": "complete",
            "results": [
                [200, "/worker-default-final", format!("{cross_origin}/worker-default-final")],
                [200, "worker-default-after", format!("{page_origin}/worker-default-after")],
                [200, "/worker-include-final", format!("{cross_origin}/worker-include-final")],
                [200, "/worker-include-after", format!("{cross_origin}/worker-include-after")],
            ]
        }])
    );

    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    let page_requests = page_server.await.unwrap();
    let cross_requests = cross_server.await.unwrap();

    assert_eq!(page_requests.len(), 8);
    assert_eq!(
        cross_requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        [
            "/page-default-final",
            "/page-sync-include-final",
            "/page-sync-include-after",
            "/worker-default-final",
            "/worker-include-final",
            "/worker-include-after",
        ]
    );
    for (name, value) in [
        ("root", "seed"),
        ("page_default_hop", "accepted"),
        ("page_sync_hop", "accepted"),
        ("page_sync_cross", "accepted"),
        ("worker_default_hop", "accepted"),
        ("worker_include_hop", "accepted"),
        ("worker_include_cross", "accepted"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == name && cookie.value == value && cookie.http_only })
        );
    }
    for name in ["page_default_cross", "worker_default_cross"] {
        assert!(
            !cookies.iter().any(|cookie| cookie.name == name),
            "default same-origin mode must reject cross-origin redirect cookie {name}"
        );
    }
}
#[tokio::test]
async fn native_content_process_xhr_cors_preflight_never_carries_cookies() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let api_origin = format!("http://{api_address}");
    let worker_api_origin = api_origin.clone();
    let expected_origin = page_origin.clone();
    let worker_script = format!(
        r#"
const apiOrigin = "{worker_api_origin}";
self.onmessage = async event => {{
  if (event.data !== "run") return;
  const send = (method, path, withCredentials, headerName, headerValue, body) =>
    new Promise(resolve => {{
      const xhr = new XMLHttpRequest();
      xhr.withCredentials = withCredentials;
      xhr.onload = () => resolve([xhr.status, xhr.responseText, xhr.responseURL]);
      xhr.onerror = () => resolve([0, "error", xhr.responseURL]);
      xhr.open(method, apiOrigin + path);
      if (headerName) {{
        xhr.setRequestHeader("Content-Type", "application/json");
        xhr.setRequestHeader(headerName, headerValue);
      }}
      xhr.send(body);
    }});
  try {{
    const post = await send("POST", "/worker-include", true,
      "X-Glass-Worker", "worker-token", JSON.stringify({{source: "worker"}}));
    const after = await send("GET", "/worker-include-after", true, null, null, null);
    self.postMessage({{ kind: "complete", results: [post, after] }});
  }} catch (error) {{
    self.postMessage({{ kind: "error", message: String(error) }});
  }}
}};
self.postMessage({{ kind: "ready" }});
"#
    );

    let page_server = tokio::spawn(async move {
        for expected_path in ["/page", "/xhr-preflight-worker.js"] {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
                    .await
                    .expect("page and Worker bootstrap requests stay bounded")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/xhr-preflight-worker.js" {
                assert!(
                    header_value(&request, "cookie")
                        .unwrap_or_default()
                        .contains("root=seed")
                );
            }
            let (content_type, headers, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "<script>globalThis.xhrPreflightMessages = []; globalThis.xhrPreflightWorker = new Worker('/xhr-preflight-worker.js'); xhrPreflightWorker.onmessage = event => xhrPreflightMessages.push(event.data);</script>".to_owned(),
                )
            } else {
                ("text/javascript", "", worker_script.clone())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let api_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(60), async move {
            type CorsExpectation<'a> = (
                &'a str,
                &'a str,
                bool,
                &'a [&'a str],
                Option<(&'a str, &'a str)>,
                Option<&'a str>,
                Option<&'a str>,
            );
            let expectations: [CorsExpectation<'_>; 8] = [
                (
                    "/page-default",
                    "OPTIONS",
                    false,
                    &[],
                    Some(("x-glass-default", "default-token")),
                    None,
                    None,
                ),
                (
                    "/page-default",
                    "POST",
                    false,
                    &[],
                    Some(("x-glass-default", "default-token")),
                    Some("{\"source\":\"page-default\"}"),
                    Some("default_cross=blocked"),
                ),
                (
                    "/page-sync-include",
                    "OPTIONS",
                    true,
                    &[],
                    Some(("x-glass-sync", "sync-token")),
                    None,
                    None,
                ),
                (
                    "/page-sync-include",
                    "POST",
                    true,
                    &["root=seed"],
                    Some(("x-glass-sync", "sync-token")),
                    Some("{\"source\":\"page-sync\"}"),
                    Some("page_sync=accepted"),
                ),
                (
                    "/page-sync-include-after",
                    "GET",
                    true,
                    &["page_sync=accepted"],
                    None,
                    None,
                    None,
                ),
                (
                    "/worker-include",
                    "OPTIONS",
                    true,
                    &[],
                    Some(("x-glass-worker", "worker-token")),
                    None,
                    None,
                ),
                (
                    "/worker-include",
                    "POST",
                    true,
                    &["root=seed", "page_sync=accepted"],
                    Some(("x-glass-worker", "worker-token")),
                    Some("{\"source\":\"worker\"}"),
                    Some("worker_include=accepted"),
                ),
                (
                    "/worker-include-after",
                    "GET",
                    true,
                    &["page_sync=accepted", "worker_include=accepted"],
                    None,
                    None,
                    None,
                ),
            ];
            let mut requests = Vec::new();
            for (path, method, include, expected_cookies, custom_header, body, set_cookie) in
                expectations
            {
                let (mut stream, _) = api_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().next(), Some(method));
                assert_eq!(request.split_whitespace().nth(1), Some(path));
                assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                let cookie = header_value(&request, "cookie")
                    .unwrap_or_default()
                    .to_owned();
                if method == "OPTIONS" {
                    assert!(
                        cookie.is_empty(),
                        "preflight for {path} must never carry cookies; got {cookie:?}"
                    );
                    assert_eq!(header_value(&request, "access-control-request-method"), Some("POST"));
                    let (header_name, _) = custom_header.expect("preflight case has a custom header");
                    let expected_headers = format!("content-type, {header_name}");
                    assert_eq!(
                        header_value(&request, "access-control-request-headers")
                            .map(str::to_ascii_lowercase),
                        Some(expected_headers)
                    );
                } else {
                    for expected_cookie in expected_cookies {
                        assert!(
                            cookie.contains(expected_cookie),
                            "{method} {path} must carry {expected_cookie}; got {cookie:?}"
                        );
                    }
                    if include {
                        assert!(!cookie.is_empty(), "credentialed {method} {path} needs cookies");
                    } else {
                        assert!(cookie.is_empty(), "default XHR {path} must omit cross-origin cookies");
                    }
                    if let Some((header_name, header_value_expected)) = custom_header {
                        assert_eq!(header_value(&request, header_name), Some(header_value_expected));
                        assert_eq!(header_value(&request, "content-type"), Some("application/json"));
                    }
                    if let Some(expected_body) = body {
                        assert_eq!(
                            request.split_once("\r\n\r\n").map(|(_, value)| value),
                            Some(expected_body)
                        );
                    }
                }

                let allow_origin = if include { expected_origin.as_str() } else { "*" };
                let allow_credentials = if include {
                    "Access-Control-Allow-Credentials: true\r\n"
                } else {
                    ""
                };
                let preflight_headers = if method == "OPTIONS" {
                    let (header_name, _) = custom_header.unwrap();
                    format!(
                        "Access-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type, {header_name}\r\n"
                    )
                } else {
                    String::new()
                };
                let cookie_header = set_cookie
                    .map(|value| {
                        format!("Set-Cookie: {value}; HttpOnly; Path=/; SameSite=Lax\r\n")
                    })
                    .unwrap_or_default();
                let response_body = if method == "OPTIONS" { "" } else { path };
                let status = if method == "OPTIONS" {
                    "204 No Content"
                } else {
                    "200 OK"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nAccess-Control-Allow-Origin: {allow_origin}\r\n{allow_credentials}{preflight_headers}{cookie_header}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                    response_body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((method.to_owned(), path.to_owned(), cookie));
            }
            requests
        })
        .await
        .expect("XHR preflight and actual requests stay bounded")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine.evaluate_async("xhrPreflightMessages").await.unwrap();
            if messages
                .as_array()
                .is_some_and(|messages| messages.iter().any(|message| message["kind"] == "ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker preflight fixture is ready");

    let page_default = tokio::time::timeout(
        Duration::from_secs(30),
        engine.evaluate_async(format!(
            r#"await new Promise(resolve => {{
              const xhr = new XMLHttpRequest();
              xhr.withCredentials = false;
              xhr.onload = () => resolve([xhr.status, xhr.responseText, xhr.responseURL]);
              xhr.onerror = () => resolve([0, "error", xhr.responseURL]);
              xhr.open("POST", "{api_origin}/page-default");
              xhr.setRequestHeader("Content-Type", "application/json");
              xhr.setRequestHeader("X-Glass-Default", "default-token");
              xhr.send(JSON.stringify({{source: "page-default"}}));
            }})"#
        )),
    )
    .await
    .expect("default page XHR preflight stays bounded")
    .unwrap();
    assert_eq!(
        page_default,
        serde_json::json!([200, "/page-default", format!("{api_origin}/page-default")])
    );

    let page_sync = engine
        .evaluate_async(format!(
            r#"(() => {{
              const send = (path, method, headerName, headerValue, body) => {{
                const xhr = new XMLHttpRequest();
                xhr.withCredentials = true;
                xhr.open(method, "{api_origin}" + path, false);
                if (headerName) {{
                  xhr.setRequestHeader("Content-Type", "application/json");
                  xhr.setRequestHeader(headerName, headerValue);
                }}
                xhr.send(body);
                return [xhr.status, xhr.responseText, xhr.responseURL];
              }};
              return [
                send("/page-sync-include", "POST", "X-Glass-Sync", "sync-token", JSON.stringify({{source: "page-sync"}})),
                send("/page-sync-include-after", "GET", null, null, null),
              ];
            }})()"#
        ))
        .await
        .unwrap();
    assert_eq!(
        page_sync,
        serde_json::json!([
            [
                200,
                "/page-sync-include",
                format!("{api_origin}/page-sync-include")
            ],
            [
                200,
                "/page-sync-include-after",
                format!("{api_origin}/page-sync-include-after")
            ],
        ])
    );

    engine
        .evaluate_async("xhrPreflightWorker.postMessage('run'); true")
        .await
        .unwrap();
    let worker_messages = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let messages = engine.evaluate_async("xhrPreflightMessages").await.unwrap();
            if messages.as_array().is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message["kind"] == "complete" || message["kind"] == "error")
            }) {
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker XHR preflight requests settle");
    assert_eq!(
        worker_messages,
        serde_json::json!([{"kind": "ready"}, {
            "kind": "complete",
            "results": [
                [200, "/worker-include", format!("{api_origin}/worker-include")],
                [200, "/worker-include-after", format!("{api_origin}/worker-include-after")],
            ]
        }])
    );

    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    let requests = api_server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|(method, path, _)| (method.as_str(), path.as_str()))
            .collect::<Vec<_>>(),
        [
            ("OPTIONS", "/page-default"),
            ("POST", "/page-default"),
            ("OPTIONS", "/page-sync-include"),
            ("POST", "/page-sync-include"),
            ("GET", "/page-sync-include-after"),
            ("OPTIONS", "/worker-include"),
            ("POST", "/worker-include"),
            ("GET", "/worker-include-after"),
        ]
    );
    for (name, value) in [
        ("root", "seed"),
        ("page_sync", "accepted"),
        ("worker_include", "accepted"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == name && cookie.value == value && cookie.http_only })
        );
    }
    assert!(
        !cookies.iter().any(|cookie| cookie.name == "default_cross"),
        "default cross-origin XHR must reject its response cookie"
    );
}
#[tokio::test]
async fn native_content_process_xhr_denied_credentialed_preflight_sends_no_post() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let api_origin = format!("http://{api_address}");
    let worker_api_origin = api_origin.clone();
    let expected_origin = page_origin.clone();
    let worker_script = format!(
        r#"
const apiOrigin = "{worker_api_origin}";
self.onmessage = async event => {{
  if (event.data !== "run") return;
  try {{
    const result = await new Promise(resolve => {{
      const xhr = new XMLHttpRequest();
      xhr.withCredentials = true;
      xhr.onload = () => resolve(["load", xhr.status]);
      xhr.onerror = () => resolve(["error", xhr.status]);
      xhr.open("POST", apiOrigin + "/worker-denied");
      xhr.setRequestHeader("Content-Type", "application/json");
      xhr.setRequestHeader("X-Glass-Denied", "worker-token");
      xhr.send(JSON.stringify({{source: "worker"}}));
    }});
    self.postMessage({{ kind: "complete", result }});
  }} catch (error) {{
    self.postMessage({{ kind: "error", message: String(error) }});
  }}
}};
self.postMessage({{ kind: "ready" }});
"#
    );

    let page_server = tokio::spawn(async move {
        for expected_path in ["/page", "/xhr-denied-preflight-worker.js"] {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
                    .await
                    .expect("page and Worker bootstrap requests stay bounded")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/xhr-denied-preflight-worker.js" {
                assert!(
                    header_value(&request, "cookie")
                        .unwrap_or_default()
                        .contains("root=seed")
                );
            }
            let (content_type, headers, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "<script>globalThis.deniedPreflightMessages = []; globalThis.deniedPreflightWorker = new Worker('/xhr-denied-preflight-worker.js'); deniedPreflightWorker.onmessage = event => deniedPreflightMessages.push(event.data);</script>".to_owned(),
                )
            } else {
                ("text/javascript", "", worker_script.clone())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let api_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            for path in ["/page-denied", "/worker-denied"] {
                let (mut stream, _) = api_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
                assert_eq!(request.split_whitespace().nth(1), Some(path));
                assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                assert_eq!(header_value(&request, "access-control-request-method"), Some("POST"));
                assert_eq!(
                    header_value(&request, "access-control-request-headers")
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("content-type, x-glass-denied")
                );
                assert!(
                    header_value(&request, "cookie").is_none(),
                    "credentialed preflight for {path} must not carry Cookie"
                );
                let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type, x-glass-denied\r\nSet-Cookie: denied_preflight=must-not-stick; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(response.as_bytes()).await.unwrap();
            }

            match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                Err(_) => {}
                Ok(Ok((mut stream, _))) => {
                    let request = read_http_request(&mut stream).await;
                    panic!("denied credentialed preflight dispatched an actual request: {request}");
                }
                Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
            }
        })
        .await
        .expect("denied preflights do not dispatch actual requests")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine
                .evaluate_async("deniedPreflightMessages")
                .await
                .unwrap();
            if messages
                .as_array()
                .is_some_and(|messages| messages.iter().any(|message| message["kind"] == "ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker denial fixture is ready");

    let page_result = tokio::time::timeout(
        Duration::from_secs(20),
        engine.evaluate_async(format!(
            r#"await new Promise(resolve => {{
              const xhr = new XMLHttpRequest();
              xhr.withCredentials = true;
              xhr.onload = () => resolve(["load", xhr.status]);
              xhr.onerror = () => resolve(["error", xhr.status]);
              xhr.open("POST", "{api_origin}/page-denied");
              xhr.setRequestHeader("Content-Type", "application/json");
              xhr.setRequestHeader("X-Glass-Denied", "page-token");
              xhr.send(JSON.stringify({{source: "page"}}));
            }})"#
        )),
    )
    .await
    .expect("page XHR rejects denied credentialed preflight")
    .unwrap();
    assert_eq!(page_result, serde_json::json!(["error", 0]));

    engine
        .evaluate_async("deniedPreflightWorker.postMessage('run'); true")
        .await
        .unwrap();
    let worker_messages = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine
                .evaluate_async("deniedPreflightMessages")
                .await
                .unwrap();
            if messages.as_array().is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message["kind"] == "complete" || message["kind"] == "error")
            }) {
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Worker XHR rejects denied credentialed preflight");
    assert_eq!(
        worker_messages,
        serde_json::json!([{"kind": "ready"}, {
            "kind": "complete",
            "result": ["error", 0]
        }])
    );

    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    api_server.await.unwrap();
    assert!(
        cookies
            .iter()
            .any(|cookie| { cookie.name == "root" && cookie.value == "seed" && cookie.http_only })
    );
    assert!(
        !cookies
            .iter()
            .any(|cookie| cookie.name == "denied_preflight")
    );
}
#[tokio::test]
async fn native_content_process_xhr_method_and_header_denied_preflights_send_no_post() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let api_origin = format!("http://{api_address}");
    let worker_api_origin = api_origin.clone();
    let expected_origin = page_origin.clone();
    let response_origin = page_origin.clone();
    let worker_script = format!(
        r#"
const apiOrigin = "{worker_api_origin}";
self.onmessage = async event => {{
  if (event.data !== "run") return;
  try {{
    const result = await new Promise(resolve => {{
      const xhr = new XMLHttpRequest();
      xhr.withCredentials = true;
      xhr.onload = () => resolve(["load", xhr.status]);
      xhr.onerror = () => resolve(["error", xhr.status]);
      xhr.open("POST", apiOrigin + "/worker-denied-header");
      xhr.setRequestHeader("Content-Type", "application/json");
      xhr.setRequestHeader("X-Glass-Denied", "worker-token");
      xhr.send(JSON.stringify({{source: "worker"}}));
    }});
    self.postMessage({{ kind: "complete", result }});
  }} catch (error) {{
    self.postMessage({{ kind: "error", message: String(error) }});
  }}
}};
self.postMessage({{ kind: "ready" }});
"#
    );

    let page_server = tokio::spawn(async move {
        for expected_path in ["/page", "/xhr-preflight-denials-worker.js"] {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
                    .await
                    .expect("page and Worker bootstrap requests stay bounded")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/xhr-preflight-denials-worker.js" {
                assert!(
                    header_value(&request, "cookie")
                        .unwrap_or_default()
                        .contains("root=seed")
                );
            }
            let (content_type, headers, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "<script>globalThis.deniedPreflightMessages = []; globalThis.deniedPreflightWorker = new Worker('/xhr-preflight-denials-worker.js'); deniedPreflightWorker.onmessage = event => deniedPreflightMessages.push(event.data);</script>".to_owned(),
                )
            } else {
                ("text/javascript", "", worker_script.clone())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let api_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            for (path, allowed_methods, allowed_headers, rejected_cookie) in [
                (
                    "/page-denied-method",
                    "GET",
                    "content-type, x-glass-denied",
                    "denied_method",
                ),
                (
                    "/worker-denied-header",
                    "POST",
                    "content-type",
                    "denied_header",
                ),
            ] {
                let (mut stream, _) = api_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
                assert_eq!(request.split_whitespace().nth(1), Some(path));
                assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                assert_eq!(header_value(&request, "access-control-request-method"), Some("POST"));
                assert_eq!(
                    header_value(&request, "access-control-request-headers")
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("content-type, x-glass-denied")
                );
                assert!(
                    header_value(&request, "cookie").is_none(),
                    "credentialed preflight for {path} must not carry Cookie"
                );
                let response = format!(
                    "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Methods: {allowed_methods}\r\nAccess-Control-Allow-Headers: {allowed_headers}\r\nSet-Cookie: {rejected_cookie}=must-not-stick; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }

            match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                Err(_) => {}
                Ok(Ok((mut stream, _))) => {
                    let request = read_http_request(&mut stream).await;
                    panic!("denied preflight dispatched an actual request: {request}");
                }
                Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
            }
        })
        .await
        .expect("denied method/header preflights do not dispatch actual requests")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine
                .evaluate_async("deniedPreflightMessages")
                .await
                .unwrap();
            if messages
                .as_array()
                .is_some_and(|messages| messages.iter().any(|message| message["kind"] == "ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker denial fixture is ready");

    let page_result = tokio::time::timeout(
        Duration::from_secs(20),
        engine.evaluate_async(format!(
            r#"await new Promise(resolve => {{
              const xhr = new XMLHttpRequest();
              xhr.withCredentials = true;
              xhr.onload = () => resolve(["load", xhr.status]);
              xhr.onerror = () => resolve(["error", xhr.status]);
              xhr.open("POST", "{api_origin}/page-denied-method");
              xhr.setRequestHeader("Content-Type", "application/json");
              xhr.setRequestHeader("X-Glass-Denied", "page-token");
              xhr.send(JSON.stringify({{source: "page"}}));
            }})"#
        )),
    )
    .await
    .expect("page XHR rejects method-denied credentialed preflight")
    .unwrap();
    assert_eq!(page_result, serde_json::json!(["error", 0]));

    engine
        .evaluate_async("deniedPreflightWorker.postMessage('run'); true")
        .await
        .unwrap();
    let worker_messages = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine
                .evaluate_async("deniedPreflightMessages")
                .await
                .unwrap();
            if messages.as_array().is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message["kind"] == "complete" || message["kind"] == "error")
            }) {
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Worker XHR rejects header-denied credentialed preflight");
    assert_eq!(
        worker_messages,
        serde_json::json!([{"kind": "ready"}, {
            "kind": "complete",
            "result": ["error", 0]
        }])
    );

    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    api_server.await.unwrap();
    assert!(
        cookies
            .iter()
            .any(|cookie| { cookie.name == "root" && cookie.value == "seed" && cookie.http_only })
    );
    assert!(
        !cookies
            .iter()
            .any(|cookie| { cookie.name == "denied_method" || cookie.name == "denied_header" })
    );
}
#[tokio::test]
async fn native_content_process_xhr_preflight_cache_separates_omit_and_include() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let api_origin = format!("http://{api_address}");
    let expected_origin = page_origin.clone();
    let response_origin = page_origin.clone();

    let page_server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
            .await
            .expect("page request stays bounded")
            .unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<title>Preflight cache owner</title>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nSet-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let api_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
            assert_eq!(request.split_whitespace().nth(1), Some("/cache"));
            assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
            assert_eq!(header_value(&request, "access-control-request-method"), Some("POST"));
            assert_eq!(
                header_value(&request, "access-control-request-headers")
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("content-type, x-glass-cache")
            );
            assert!(header_value(&request, "cookie").is_none());
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type, x-glass-cache\r\nAccess-Control-Max-Age: 600\r\nSet-Cookie: omit_preflight=must-not-stick; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();

            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("POST"));
            assert_eq!(request.split_whitespace().nth(1), Some("/cache"));
            assert!(header_value(&request, "cookie").is_none());
            assert_eq!(header_value(&request, "x-glass-cache"), Some("cache-token"));
            assert_eq!(
                request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default(),
                r#"{"source":"default"}"#
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\nSet-Cookie: omit_actual=must-not-stick; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();

            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
            assert_eq!(request.split_whitespace().nth(1), Some("/cache"));
            assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
            assert_eq!(header_value(&request, "access-control-request-method"), Some("POST"));
            assert_eq!(
                header_value(&request, "access-control-request-headers")
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("content-type, x-glass-cache")
            );
            assert!(
                header_value(&request, "cookie").is_none(),
                "credentialed preflight must not carry Cookie"
            );
            let preflight_response = format!(
                "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type, x-glass-cache\r\nAccess-Control-Max-Age: 600\r\nSet-Cookie: include_preflight=must-not-stick; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(preflight_response.as_bytes())
                .await
                .unwrap();

            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("POST"));
            assert_eq!(request.split_whitespace().nth(1), Some("/cache"));
            let cookie = header_value(&request, "cookie").unwrap_or_default();
            assert!(cookie.contains("root=seed"), "parent cookie missing: {cookie}");
            assert!(!cookie.contains("omit_preflight"));
            assert!(!cookie.contains("omit_actual"));
            assert!(!cookie.contains("include_preflight"));
            assert_eq!(header_value(&request, "x-glass-cache"), Some("cache-token"));
            assert_eq!(
                request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default(),
                r#"{"source":"include"}"#
            );
            let include_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nSet-Cookie: include_actual=accepted; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            );
            stream
                .write_all(include_response.as_bytes())
                .await
                .unwrap();

            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(
                request.split_whitespace().next(),
                Some("POST"),
                "a cached credentialed entry should avoid another OPTIONS request"
            );
            assert_eq!(request.split_whitespace().nth(1), Some("/cache"));
            let cookie = header_value(&request, "cookie").unwrap_or_default();
            assert!(cookie.contains("root=seed"), "parent cookie missing: {cookie}");
            assert!(
                cookie.contains("include_actual=accepted"),
                "parent response cookie missing: {cookie}"
            );
            assert!(!cookie.contains("omit_preflight"));
            assert!(!cookie.contains("omit_actual"));
            assert!(!cookie.contains("include_preflight"));
            assert_eq!(header_value(&request, "x-glass-cache"), Some("cache-token"));
            assert_eq!(
                request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default(),
                r#"{"source":"include-again"}"#
            );
            let final_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            );
            stream
                .write_all(final_response.as_bytes())
                .await
                .unwrap();

            match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                Err(_) => {}
                Ok(Ok((mut stream, _))) => {
                    let request = read_http_request(&mut stream).await;
                    panic!("unexpected extra preflight-cache request: {request}");
                }
                Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
            }
        })
        .await
        .expect("preflight cache request sequence stays bounded")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let results = tokio::time::timeout(
        Duration::from_secs(30),
        engine.evaluate_async(format!(
            r#"await (async () => {{
              const send = (include, source) => new Promise(resolve => {{
                const xhr = new XMLHttpRequest();
                if (include) xhr.withCredentials = true;
                xhr.onload = () => resolve(["load", xhr.status]);
                xhr.onerror = () => resolve(["error", xhr.status]);
                xhr.open("POST", "{api_origin}/cache");
                xhr.setRequestHeader("Content-Type", "application/json");
                xhr.setRequestHeader("X-Glass-Cache", "cache-token");
                xhr.send(JSON.stringify({{source}}));
              }});
              return [await send(false, "default"), await send(true, "include"), await send(true, "include-again")];
            }})()"#
        )),
    )
    .await
    .expect("all XHRs complete through the parent broker")
    .unwrap();
    assert_eq!(
        results,
        serde_json::json!([["load", 200], ["load", 200], ["load", 200]])
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    api_server.await.unwrap();
    for (name, value) in [("root", "seed"), ("include_actual", "accepted")] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == name && cookie.value == value && cookie.http_only })
        );
    }
    for name in ["omit_preflight", "omit_actual", "include_preflight"] {
        assert!(
            !cookies.iter().any(|cookie| cookie.name == name),
            "cookie from an unauthenticated or preflight response was accepted: {name}"
        );
    }
}
#[tokio::test]
async fn native_content_process_xhr_keeps_parent_cookie_on_cors_error() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let api_origin = format!("http://{api_address}");
    let expected_origin = page_origin.clone();
    let response_origin = page_origin.clone();

    let page_server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
            .await
            .expect("page request stays bounded")
            .unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<title>CORS response cookie owner</title>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nSet-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let api_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
            assert_eq!(request.split_whitespace().nth(1), Some("/cors-cookie"));
            assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
            assert_eq!(header_value(&request, "access-control-request-method"), Some("POST"));
            assert_eq!(
                header_value(&request, "access-control-request-headers")
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("content-type, x-glass-cookie")
            );
            assert!(header_value(&request, "cookie").is_none());
            let preflight_response = format!(
                "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type, x-glass-cookie\r\nAccess-Control-Max-Age: 600\r\nSet-Cookie: preflight_cookie=must-not-stick; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(preflight_response.as_bytes())
                .await
                .unwrap();

            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("POST"));
            assert_eq!(request.split_whitespace().nth(1), Some("/cors-cookie"));
            let cookie = header_value(&request, "cookie").unwrap_or_default();
            assert!(cookie.contains("root=seed"), "parent seed cookie missing: {cookie}");
            assert!(!cookie.contains("preflight_cookie"));
            assert!(!cookie.contains("cors_failed"));
            assert_eq!(header_value(&request, "x-glass-cookie"), Some("parent-owned"));
            assert_eq!(
                request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default(),
                r#"{"attempt":"blocked"}"#
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\nSet-Cookie: cors_failed=accepted; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 7\r\nConnection: close\r\n\r\nblocked")
                .await
                .unwrap();

            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(
                request.split_whitespace().next(),
                Some("POST"),
                "the successful credentialed preflight should be cached"
            );
            assert_eq!(request.split_whitespace().nth(1), Some("/cors-cookie"));
            let cookie = header_value(&request, "cookie").unwrap_or_default();
            assert!(cookie.contains("root=seed"), "parent seed cookie missing: {cookie}");
            assert!(
                cookie.contains("cors_failed=accepted"),
                "actual response cookie was not retained by parent: {cookie}"
            );
            assert!(!cookie.contains("preflight_cookie"));
            assert_eq!(header_value(&request, "x-glass-cookie"), Some("parent-owned"));
            assert_eq!(
                request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default(),
                r#"{"attempt":"visible"}"#
            );
            let final_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            );
            stream.write_all(final_response.as_bytes()).await.unwrap();

            match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                Err(_) => {}
                Ok(Ok((mut stream, _))) => {
                    let request = read_http_request(&mut stream).await;
                    panic!("unexpected extra CORS-cookie request: {request}");
                }
                Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
            }
        })
        .await
        .expect("CORS-cookie request sequence stays bounded")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let results = tokio::time::timeout(
        Duration::from_secs(30),
        engine.evaluate_async(format!(
            r#"await (async () => {{
              const send = attempt => new Promise(resolve => {{
                const xhr = new XMLHttpRequest();
                xhr.withCredentials = true;
                xhr.onload = () => resolve(["load", xhr.status, xhr.responseText]);
                xhr.onerror = () => resolve(["error", xhr.status, xhr.responseText]);
                xhr.open("POST", "{api_origin}/cors-cookie");
                xhr.setRequestHeader("Content-Type", "application/json");
                xhr.setRequestHeader("X-Glass-Cookie", "parent-owned");
                xhr.send(JSON.stringify({{attempt}}));
              }});
              return [await send("blocked"), await send("visible"), document.cookie];
            }})()"#
        )),
    )
    .await
    .expect("credentialed XHRs complete through the parent broker")
    .unwrap();
    assert_eq!(
        results,
        serde_json::json!([["error", 0, ""], ["load", 200, "ok"], ""])
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    api_server.await.unwrap();
    for (name, value) in [("root", "seed"), ("cors_failed", "accepted")] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == name && cookie.value == value && cookie.http_only }),
            "parent cookie jar is missing {name}={value}"
        );
    }
    assert!(
        !cookies
            .iter()
            .any(|cookie| cookie.name == "preflight_cookie"),
        "Set-Cookie from the credential-free preflight must be ignored"
    );
}
#[tokio::test]
async fn native_content_process_xhr_credentialed_preflight_wildcards_send_no_post() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let page_origin = format!("http://{page_address}");
    let api_origin = format!("http://{api_address}");
    let worker_api_origin = api_origin.clone();
    let expected_origin = page_origin.clone();
    let response_origin = page_origin.clone();
    let worker_script = format!(
        r#"
const apiOrigin = "{worker_api_origin}";
self.onmessage = async event => {{
  if (event.data !== "run") return;
  try {{
    const result = await new Promise(resolve => {{
      const xhr = new XMLHttpRequest();
      xhr.withCredentials = true;
      xhr.onload = () => resolve(["load", xhr.status]);
      xhr.onerror = () => resolve(["error", xhr.status]);
      xhr.open("POST", apiOrigin + "/worker-wildcard-header");
      xhr.setRequestHeader("Content-Type", "application/json");
      xhr.setRequestHeader("X-Glass-Wildcard", "worker-token");
      xhr.send(JSON.stringify({{source: "worker"}}));
    }});
    self.postMessage({{ kind: "complete", result }});
  }} catch (error) {{
    self.postMessage({{ kind: "error", message: String(error) }});
  }}
}};
self.postMessage({{ kind: "ready" }});
"#
    );

    let page_server = tokio::spawn(async move {
        for expected_path in ["/page", "/xhr-wildcard-preflight-worker.js"] {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(45), page_listener.accept())
                    .await
                    .expect("page and Worker bootstrap requests stay bounded")
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/xhr-wildcard-preflight-worker.js" {
                assert!(
                    header_value(&request, "cookie")
                        .unwrap_or_default()
                        .contains("root=seed")
                );
            }
            let (content_type, headers, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "Set-Cookie: root=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "<script>globalThis.wildcardPreflightMessages = []; globalThis.wildcardPreflightWorker = new Worker('/xhr-wildcard-preflight-worker.js'); wildcardPreflightWorker.onmessage = event => wildcardPreflightMessages.push(event.data);</script>".to_owned(),
                )
            } else {
                ("text/javascript", "", worker_script.clone())
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let api_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            for (path, allowed_methods, allowed_headers, rejected_cookie) in [
                (
                    "/page-wildcard-method",
                    "*",
                    "content-type, x-glass-wildcard",
                    "wildcard_method",
                ),
                (
                    "/worker-wildcard-header",
                    "POST",
                    "*",
                    "wildcard_header",
                ),
            ] {
                let (mut stream, _) = api_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
                assert_eq!(request.split_whitespace().nth(1), Some(path));
                assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                assert_eq!(header_value(&request, "access-control-request-method"), Some("POST"));
                assert_eq!(
                    header_value(&request, "access-control-request-headers")
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("content-type, x-glass-wildcard")
                );
                assert!(
                    header_value(&request, "cookie").is_none(),
                    "credentialed preflight for {path} must not carry Cookie"
                );
                let response = format!(
                    "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\nAccess-Control-Allow-Methods: {allowed_methods}\r\nAccess-Control-Allow-Headers: {allowed_headers}\r\nSet-Cookie: {rejected_cookie}=must-not-stick; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }

            match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                Err(_) => {}
                Ok(Ok((mut stream, _))) => {
                    let request = read_http_request(&mut stream).await;
                    panic!("credentialed wildcard preflight dispatched an actual request: {request}");
                }
                Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
            }
        })
        .await
        .expect("credentialed wildcard preflights do not dispatch actual requests")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{page_origin}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine
                .evaluate_async("wildcardPreflightMessages")
                .await
                .unwrap();
            if messages
                .as_array()
                .is_some_and(|messages| messages.iter().any(|message| message["kind"] == "ready"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("DedicatedWorker wildcard fixture is ready");

    let page_result = tokio::time::timeout(
        Duration::from_secs(20),
        engine.evaluate_async(format!(
            r#"await new Promise(resolve => {{
              const xhr = new XMLHttpRequest();
              xhr.withCredentials = true;
              xhr.onload = () => resolve(["load", xhr.status]);
              xhr.onerror = () => resolve(["error", xhr.status]);
              xhr.open("POST", "{api_origin}/page-wildcard-method");
              xhr.setRequestHeader("Content-Type", "application/json");
              xhr.setRequestHeader("X-Glass-Wildcard", "page-token");
              xhr.send(JSON.stringify({{source: "page"}}));
            }})"#
        )),
    )
    .await
    .expect("page XHR rejects wildcard credentialed method preflight")
    .unwrap();
    assert_eq!(page_result, serde_json::json!(["error", 0]));

    engine
        .evaluate_async("wildcardPreflightWorker.postMessage('run'); true")
        .await
        .unwrap();
    let worker_messages = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = engine
                .evaluate_async("wildcardPreflightMessages")
                .await
                .unwrap();
            if messages.as_array().is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message["kind"] == "complete" || message["kind"] == "error")
            }) {
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Worker XHR rejects wildcard credentialed header preflight");
    assert_eq!(
        worker_messages,
        serde_json::json!([{"kind": "ready"}, {
            "kind": "complete",
            "result": ["error", 0]
        }])
    );

    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    api_server.await.unwrap();
    assert!(
        cookies
            .iter()
            .any(|cookie| { cookie.name == "root" && cookie.value == "seed" && cookie.http_only })
    );
    assert!(
        !cookies
            .iter()
            .any(|cookie| { cookie.name == "wildcard_method" || cookie.name == "wildcard_header" })
    );
}
#[tokio::test]
async fn native_content_process_exposes_bounded_xhr_fetch_bridge() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/xhr", "/binary", "/xhr-binary", "/xhr-blob"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request_bytes = read_http_request_bytes(&mut stream).await;
            let request = String::from_utf8_lossy(&request_bytes);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/xhr" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            && value.trim() == "application/json"
                    })
                }));
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or_default();
                assert_eq!(body, r#"{"name":"glass"}"#);
            }
            if expected_path == "/xhr-binary" {
                assert_eq!(request.split_whitespace().next(), Some("POST"));
                assert!(request.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("content-type")
                            && value.trim() == "application/octet-stream"
                    })
                }));
                let header_end = request_bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                assert_eq!(&request_bytes[header_end..], [0_u8, 0xff, 0x80, b'A']);
            }
            if matches!(expected_path, "/binary" | "/xhr-binary" | "/xhr-blob") {
                let body = [0_u8, 0xff, 0x80, b'A'];
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
                continue;
            }
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<p>XHR owner</p>")
            } else {
                ("text/plain", "xhr-response")
            };
            let extra_headers = if expected_path == "/xhr" {
                "X-Glass-Response: one\r\nX-Glass-Response: two\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                const xhr = new XMLHttpRequest();
                xhr.open('POST', '/xhr');
                xhr.setRequestHeader('Content-Type', 'application/json');
                xhr.onload = async () => {
                    globalThis.xhrResult = [xhr.status, xhr.responseText, xhr.responseURL, xhr.getResponseHeader('content-type'), xhr.getResponseHeader('x-glass-response'), xhr.getAllResponseHeaders()];
                    const binaryResponse = await fetch('/binary');
                    const binaryBlob = await binaryResponse.blob();
                    const binaryXhr = new XMLHttpRequest();
                    binaryXhr.open('POST', '/xhr-binary');
                    binaryXhr.responseType = 'arraybuffer';
                    binaryXhr.setRequestHeader('Content-Type', 'application/octet-stream');
                    binaryXhr.onload = async () => {
                        let responseTextError = '';
                        try { binaryXhr.responseText; } catch (error) { responseTextError = error.name; }
                        globalThis.xhrBinaryResult = [binaryXhr.status, binaryXhr.response instanceof ArrayBuffer, Array.from(new Uint8Array(binaryXhr.response)), responseTextError, binaryXhr.responseURL];
                        const blobXhr = new XMLHttpRequest();
                        blobXhr.open('GET', '/xhr-blob');
                        blobXhr.responseType = 'blob';
                        blobXhr.onload = async () => {
                            let responseTextError = '';
                            try { blobXhr.responseText; } catch (error) { responseTextError = error.name; }
                            globalThis.xhrBlobResult = [blobXhr.status, blobXhr.response instanceof Blob, blobXhr.response.size, Array.from(await blobXhr.response.bytes()), responseTextError, blobXhr.responseURL];
                        };
                        blobXhr.send();
                    };
                    binaryXhr.send(binaryBlob);
                };
                xhr.send(new Blob(['{"name":"glass"}'], { type: 'application/json' }));
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.xhrResult").await.unwrap(),
        serde_json::json!([
            200,
            "xhr-response",
            format!("http://{address}/xhr"),
            "text/plain",
            "one, two",
            "connection: close\r\ncontent-length: 12\r\ncontent-type: text/plain\r\nx-glass-response: one, two\r\n"
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.xhrBinaryResult")
            .await
            .unwrap(),
        serde_json::json!([
            200,
            true,
            [0, 255, 128, 65],
            "InvalidStateError",
            format!("http://{address}/xhr-binary")
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.xhrBlobResult")
            .await
            .unwrap(),
        serde_json::json!([
            200,
            true,
            4,
            [0, 255, 128, 65],
            "InvalidStateError",
            format!("http://{address}/xhr-blob")
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_publishes_response_state_lifecycle() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/state"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<p>XHR states</p>")
            } else {
                ("text/plain", "state-response")
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nX-State: ready\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                globalThis.xhrStatePromise = new Promise(resolve => {
                    const xhr = new XMLHttpRequest();
                    const states = [];
                    const record = () => states.push({
                        readyState: xhr.readyState,
                        status: xhr.status,
                        statusText: xhr.statusText,
                        url: xhr.responseURL,
                        header: xhr.getResponseHeader('x-state'),
                        responseText: xhr.responseText,
                    });
                    xhr.onreadystatechange = record;
                    xhr.onload = () => resolve({ states, response: xhr.responseText });
                    xhr.open('GET', '/state');
                    xhr.send();
                });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await xhrStatePromise")
            .await
            .unwrap(),
        serde_json::json!({
            "states": [
                {
                    "readyState": 1,
                    "status": 0,
                    "statusText": "",
                    "url": "",
                    "header": null,
                    "responseText": ""
                },
                {
                    "readyState": 2,
                    "status": 200,
                    "statusText": "OK",
                    "url": format!("http://{address}/state"),
                    "header": "ready",
                    "responseText": ""
                },
                {
                    "readyState": 3,
                    "status": 200,
                    "statusText": "OK",
                    "url": format!("http://{address}/state"),
                    "header": "ready",
                    "responseText": "state-response"
                },
                {
                    "readyState": 4,
                    "status": 200,
                    "statusText": "OK",
                    "url": format!("http://{address}/state"),
                    "header": "ready",
                    "responseText": "state-response"
                }
            ],
            "response": "state-response"
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_override_mime_type_controls_response_projection() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..6 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("timed out waiting for the next override MIME type request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let (content_type, body) = match path {
                "/page" => (
                    "text/html",
                    r#"<script>
globalThis.overrideMimeResultPromise = new Promise(resolve => {
  const fallback = new XMLHttpRequest();
  fallback.responseType = 'blob';
  fallback.overrideMimeType('not-a-mime');
  fallback.open('GET', '/invalid');
  fallback.onload = () => {
    const xhr = new XMLHttpRequest();
    let done = '';
    xhr.overrideMimeType('text/xml');
    xhr.open('GET', '/override');
    xhr.onload = () => {
      try { xhr.overrideMimeType('text/plain'); } catch (error) { done = error.name; }
      const document = xhr.responseXML;
      resolve({
        invalidMimeType: fallback.response.type,
        invalidWireContentType: fallback.getResponseHeader('content-type'),
        done,
        document: document instanceof Document,
        identity: document === xhr.responseXML,
        root: document && document.documentElement.nodeName,
        text: document && document.documentElement.textContent,
        wireContentType: xhr.getResponseHeader('content-type'),
      });
    };
    xhr.onerror = () => resolve({ error: 'page-xhr' });
    xhr.send();
  };
  fallback.onerror = () => resolve({ error: 'page-fallback-xhr' });
  fallback.send();
});
globalThis.overrideMimeWorkerPromise = new Promise(resolve => {
  const worker = new Worker('/worker.js');
  worker.onmessage = event => resolve(event.data);
});
</script><main>XHR MIME override</main>"#,
                ),
                "/override" => (
                    "text/plain",
                    r#"<?xml version="1.0"?><root>override</root>"#,
                ),
                "/worker.js" => (
                    "text/javascript",
                    r#"const fallback = new XMLHttpRequest();
fallback.responseType = 'blob';
fallback.overrideMimeType('not-a-mime');
fallback.open('GET', '/worker-invalid');
fallback.onload = () => {
  const xhr = new XMLHttpRequest();
  xhr.responseType = 'blob';
  xhr.overrideMimeType('application/x-worker-override');
  xhr.open('GET', '/worker-blob');
  xhr.onload = () => postMessage({
    invalidMimeType: fallback.response.type,
    invalidWireContentType: fallback.getResponseHeader('content-type'),
    type: xhr.response.type,
    size: xhr.response.size,
    wireContentType: xhr.getResponseHeader('content-type'),
  });
  xhr.onerror = () => postMessage({ error: 'worker-xhr' });
  xhr.send();
};
fallback.onerror = () => postMessage({ error: 'worker-fallback-xhr' });
fallback.send();"#,
                ),
                "/invalid" => ("text/plain", "invalid"),
                "/worker-invalid" => ("text/plain", "invalid"),
                "/worker-blob" => ("text/plain", "worker"),
                other => panic!("unexpected XHR MIME override request path: {other}"),
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
    assert_eq!(
        engine
            .evaluate_async(
                "await Promise.all([overrideMimeResultPromise, overrideMimeWorkerPromise])"
            )
            .await
            .unwrap(),
        serde_json::json!([
            {
                "invalidMimeType": "application/octet-stream",
                "invalidWireContentType": "text/plain",
                "done": "InvalidStateError",
                "document": true,
                "identity": true,
                "root": "root",
                "text": "override",
                "wireContentType": "text/plain",
            },
            {
                "invalidMimeType": "application/octet-stream",
                "invalidWireContentType": "text/plain",
                "type": "application/x-worker-override",
                "size": 6,
                "wireContentType": "text/plain",
            },
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_guards_response_text_by_type_and_resets_it() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..5 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("timed out waiting for the responseText request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let (content_type, body) = match path {
                "/page" => (
                    "text/html",
                    r#"<script>
globalThis.responseTextResultPromise = new Promise(resolve => {
  const result = {};
  const json = new XMLHttpRequest();
  json.responseType = 'JSON';
  json.open('GET', '/json');
  json.onload = () => {
    let jsonGuard = '';
    try { json.responseText; } catch (error) { jsonGuard = error.name; }
    result.json = { guard: jsonGuard, response: json.response };

    const text = new XMLHttpRequest();
    text.open('GET', '/text');
    let openedText = text.responseText;
    let loadingText = '';
    text.onreadystatechange = () => {
      if (text.readyState === 3) loadingText = text.responseText;
    };
    text.onload = () => {
      result.text = {
        opened: openedText,
        loading: loadingText,
        done: text.responseText,
      };
      text.open('GET', '/text');
      result.reopened = [text.readyState, text.responseText, text.response];

      const abort = new XMLHttpRequest();
      abort.open('GET', 'data:text/plain,stale');
      abort.send();
      abort.abort();
      result.abort = [abort.readyState, abort.responseText, abort.response];

      const worker = new Worker('/worker-response-text.js');
      worker.onmessage = event => {
        result.worker = event.data;
        resolve(result);
      };
      worker.onerror = () => resolve({ error: 'worker-script' });
    };
    text.onerror = () => resolve({ error: 'text-xhr' });
    text.send();
  };
  json.onerror = () => resolve({ error: 'json-xhr' });
  json.send();
});
</script><main>XHR responseText</main>"#,
                ),
                "/worker-response-text.js" => (
                    "text/javascript",
                    r#"const xhr = new XMLHttpRequest();
xhr.responseType = 'ARRAYBUFFER';
xhr.open('GET', '/worker-binary');
xhr.onload = () => {
  let guard = '';
  try { xhr.responseText; } catch (error) { guard = error.name; }
  postMessage({ guard, binary: xhr.response instanceof ArrayBuffer, bytes: xhr.response.byteLength });
};
xhr.onerror = () => postMessage({ error: 'worker-xhr' });
xhr.send();"#,
                ),
                "/json" => ("application/json", r#"{"page":true}"#),
                "/text" => ("text/plain", "text-response"),
                "/worker-binary" => ("application/octet-stream", "0123456789"),
                other => panic!("unexpected responseText request path: {other}"),
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
    assert_eq!(
        engine
            .evaluate_async("await responseTextResultPromise")
            .await
            .unwrap(),
        serde_json::json!({
            "json": {
                "guard": "InvalidStateError",
                "response": {"page": true},
            },
            "text": {
                "opened": "",
                "loading": "text-response",
                "done": "text-response",
            },
            "reopened": [1, "", ""],
            "abort": [0, "", ""],
            "worker": {
                "guard": "InvalidStateError",
                "binary": true,
                "bytes": 10,
            },
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_reports_buffered_download_progress() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/download-progress",
            "/worker-download-progress.js",
            "/worker-download-progress",
        ] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("timed out waiting for the download progress request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body, content_length) = match expected_path {
                "/page" => ("text/html", "<p>XHR progress</p>", Some(19_usize)),
                "/download-progress" => ("text/plain", "page-body", Some(9_usize)),
                "/worker-download-progress.js" => (
                    "text/javascript",
                    r#"const xhr = new XMLHttpRequest();
const events = [];
xhr.onprogress = event => events.push([
  event.type,
  event instanceof ProgressEvent,
  event.target === xhr,
  event.currentTarget === xhr,
  xhr.readyState,
  event.lengthComputable,
  event.loaded,
  event.total,
]);
xhr.onload = () => {
  events.push('load');
  postMessage(events);
};
xhr.onerror = () => postMessage(['error']);
xhr.open('GET', '/worker-download-progress');
xhr.send();"#,
                    None,
                ),
                "/worker-download-progress" => ("text/plain", "worker-body", None),
                other => panic!("unexpected download progress request path: {other}"),
            };
            let length_header = content_length
                .map(|length| format!("Content-Length: {length}\r\n"))
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{length_header}Connection: close\r\n\r\n{body}"
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
                globalThis.downloadProgressPromise = new Promise(resolve => {
                    const xhr = new XMLHttpRequest();
                    const events = [];
                    xhr.onprogress = event => events.push([
                        event.type,
                        event instanceof ProgressEvent,
                        event.target === xhr,
                        event.currentTarget === xhr,
                        xhr.readyState,
                        event.lengthComputable,
                        event.loaded,
                        event.total,
                    ]);
                    xhr.onload = () => {
                        events.push('load');
                        const worker = new Worker('/worker-download-progress.js');
                        worker.onmessage = event => resolve({ page: events, worker: event.data });
                        worker.onerror = () => resolve({ error: 'worker-script' });
                    };
                    xhr.onerror = () => resolve({ error: 'page-xhr' });
                    xhr.open('GET', '/download-progress');
                    xhr.send();
                });
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("await downloadProgressPromise")
            .await
            .unwrap(),
        serde_json::json!({
            "page": [
                ["progress", true, true, true, 3, true, 9, 9],
                "load",
            ],
            "worker": [
                ["progress", true, true, true, 3, false, 11, 0],
                "load",
            ],
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_xhr_reports_brokered_response_progress_once() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/stream"] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("timed out waiting for the streaming XHR request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/page" {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 13\r\nConnection: close\r\n\r\n<p>stream</p>",
                    )
                    .await
                    .unwrap();
                continue;
            }
            // The parent network broker buffers this bounded response before
            // transferring it to the content process; socket writes do not
            // become separate page-side XHR progress records.
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 17\r\nConnection: close\r\n\r\nfirst-\xF0",
                )
                .await
                .unwrap();
            stream.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            stream.write_all(b"\x9F\x92\xA9-second").await.unwrap();
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
                globalThis.streamingXhrPromise = new Promise(resolve => {
                    const xhr = new XMLHttpRequest();
                    const states = [];
                    const progress = [];
                    xhr.onreadystatechange = () => {
                        if (xhr.readyState === 3)
                            states.push([xhr.readyState, xhr.responseText]);
                    };
                    xhr.onprogress = event => progress.push([
                        xhr.readyState,
                        event.loaded,
                        event.total,
                        event.lengthComputable,
                    ]);
                    xhr.onload = () => resolve({
                        states,
                        progress,
                        text: xhr.responseText,
                    });
                    xhr.onerror = () => resolve({ error: 'streaming-xhr' });
                    xhr.open('GET', '/stream');
                    xhr.send();
                });
            })()"#,
        )
        .await
        .unwrap();
    let result = engine
        .evaluate_async("await streamingXhrPromise")
        .await
        .unwrap();
    let progress = result
        .get("progress")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert_eq!(progress, &[serde_json::json!([3, 17, 17, true])]);
    assert_eq!(
        result,
        serde_json::json!({
            "states": [[3, "first-💩-second"]],
            "progress": [[3, 17, 17, true]],
            "text": "first-💩-second",
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

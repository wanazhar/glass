use super::*;

#[tokio::test]
async fn native_content_process_delivers_report_only_connect_events_for_websocket() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = format!(
            "<script>globalThis.websocketResult = null; globalThis.websocketReports = []; addEventListener('securitypolicyviolation', event => websocketReports.push([event.effectiveDirective, event.blockedURI, event.disposition])); globalThis.socket = new WebSocket('ws://{address}/socket'); globalThis.socket.onmessage = event => {{ websocketResult = [event.data, websocketReports]; globalThis.socket.close(); }}; globalThis.socket.onerror = () => {{ websocketResult = ['error', websocketReports]; }};</script>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy-Report-Only: connect-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        websocket
            .send(Message::Text("websocket-report-ok".into()))
            .await
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), websocket.next()).await;
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await new Promise((resolve, reject) => { const check = () => { if (globalThis.websocketResult) { resolve(globalThis.websocketResult); return; } setTimeout(check, 1); }; check(); })",
            )
            .await
            .unwrap(),
        serde_json::json!([
            "websocket-report-ok",
            [[
                "connect-src",
                format!("ws://{address}/socket"),
                "report"
            ]]
        ])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
// tungstenite requires the full HTTP response as the handshake error type.
#[allow(clippy::result_large_err)]
async fn native_content_process_drives_websocket_text_binary_and_close_events() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let websocket_cookie_seen = Arc::new(AtomicBool::new(false));
    let websocket_cookie_seen_by_server = Arc::clone(&websocket_cookie_seen);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = format!(
            "<input id='result' value='pending'><script>globalThis.pageScriptStatus = 'before'; globalThis.pageMessages = []; try {{ globalThis.pageSocket = new WebSocket('ws://{address}/socket'); globalThis.pageSocket.binaryType = 'arraybuffer'; globalThis.pageSocket.onopen = () => {{ globalThis.pageSocket.send('client-text'); globalThis.pageSocket.send(new Uint8Array([1, 2, 3])); }}; globalThis.pageSocket.onmessage = event => {{ if (typeof event.data === 'string') globalThis.pageMessages.push(event.data); else globalThis.pageMessages.push(Array.from(new Uint8Array(event.data)).join(',')); if (globalThis.pageMessages.length === 2) {{ document.getElementById('result').value = globalThis.pageMessages.join('|'); globalThis.pageSocket.close(1000, 'done'); }} }}; globalThis.pageSocket.onerror = () => {{ globalThis.pageSocketError = true; }}; globalThis.pageScriptStatus = 'after'; }} catch (error) {{ globalThis.pageScriptStatus = String(error); }}</script>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: page-cookie=parent-secret; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let websocket_cookie_seen = Arc::clone(&websocket_cookie_seen_by_server);
        let mut websocket = accept_hdr_async(
            stream,
            move |request: &WebSocketRequest, mut response: WebSocketResponse| {
                let cookie = request
                    .headers()
                    .get(COOKIE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default();
                websocket_cookie_seen.store(
                    cookie.contains("page-cookie=parent-secret"),
                    Ordering::SeqCst,
                );
                response.headers_mut().insert(
                    SET_COOKIE,
                    HeaderValue::from_static("ws-cookie=handshake-secret; HttpOnly; Path=/"),
                );
                Ok(response)
            },
        )
        .await
        .unwrap();
        websocket
            .send(Message::Ping(b"keepalive".to_vec().into()))
            .await
            .unwrap();
        websocket
            .send(Message::Text("server-text".into()))
            .await
            .unwrap();
        websocket
            .send(Message::Binary(vec![7_u8, 8, 255].into()))
            .await
            .unwrap();
        let mut received = Vec::new();
        let mut pong_received = false;
        while received.len() < 2 || !pong_received {
            match tokio::time::timeout(Duration::from_secs(2), websocket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Message::Text(value) => received.push(("text", value.to_string().into_bytes())),
                Message::Binary(value) => received.push(("binary", value.to_vec())),
                Message::Close(_) => break,
                Message::Pong(value) => {
                    assert_eq!(value.as_ref(), b"keepalive");
                    pong_received = true;
                }
                Message::Ping(_) => {}
                _ => {}
            }
        }
        assert_eq!(
            received,
            vec![("text", b"client-text".to_vec()), ("binary", vec![1, 2, 3])]
        );
        assert!(pong_received);
        let _ = tokio::time::timeout(Duration::from_secs(2), websocket.next()).await;
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/cookie-check"));
        let cookie = request
            .lines()
            .find_map(|line| {
                line.strip_prefix("Cookie: ")
                    .or_else(|| line.strip_prefix("cookie: "))
            })
            .unwrap_or_default();
        let body = cookie.to_owned();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("[globalThis.pageScriptStatus, typeof globalThis.pageSocket]")
            .await
            .unwrap(),
        serde_json::json!(["after", "object"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await new Promise((resolve, reject) => { const check = () => { if (globalThis.pageSocketError) { reject(new Error('websocket failed')); return; } if (globalThis.pageMessages.length === 2) { resolve(globalThis.pageMessages.join('|')); return; } if (globalThis.pageSocket.readyState === 3) { reject(new Error('websocket closed before messages')); return; } setTimeout(check, 1); }; check(); })",
            )
            .await
            .unwrap(),
        serde_json::json!("server-text|7,8,255")
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('result').value")
            .await
            .unwrap(),
        serde_json::json!("server-text|7,8,255")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/cookie-check').then(response => response.text()).then(value => value.includes('ws-cookie=handshake-secret'))",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
    assert!(websocket_cookie_seen.load(Ordering::SeqCst));
}
#[tokio::test]
async fn native_content_process_dispatches_large_websocket_event_as_data() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = format!(
            "<script>globalThis.largeLength = 0; globalThis.socket = new WebSocket('ws://{address}/socket'); socket.onmessage = event => {{ largeLength = typeof event.data === 'string' ? event.data.length : -1; socket.close(); }}; socket.onerror = () => {{ largeLength = -1; }};</script>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        websocket
            .send(Message::Text("x".repeat(20_000).into()))
            .await
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), websocket.next()).await;
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await new Promise((resolve, reject) => { const check = () => { if (largeLength === 20000) { resolve(largeLength); return; } if (largeLength < 0) { reject(new Error('large websocket event failed')); return; } setTimeout(check, 1); }; check(); })",
            )
            .await
            .unwrap(),
        serde_json::json!(20_000)
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_delivers_queued_websocket_events_on_ordinary_turns() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = format!(
            "<body></body><script>globalThis.events = []; globalThis.socket = new WebSocket('ws://{address}/socket'); globalThis.socket.onmessage = event => events.push(event.data);</script>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        websocket
            .send(Message::Text("queued-background".into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        engine.evaluate_async("events").await.unwrap(),
        serde_json::json!([])
    );
    assert_eq!(
        engine.evaluate_async("events").await.unwrap(),
        serde_json::json!(["queued-background"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

use super::*;

#[tokio::test]
async fn native_content_process_delivers_report_only_report_to_network_reports() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let report_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let report_address = report_listener.local_addr().unwrap();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Report To</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: session=secret; Path=/\r\nContent-Security-Policy: connect-src 'self'\r\nContent-Security-Policy-Report-Only: connect-src 'none'; report-to csp\r\nReporting-Endpoints: csp=\"http://{report_address}/csp\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/blocked"));
        let body = "allowed";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let report_server = tokio::spawn(async move {
        // Native engine startup can spend several seconds initializing its
        // JavaScript runtime before the report-only Fetch violation is sent.
        let (mut stream, _) =
            tokio::time::timeout(Duration::from_secs(30), report_listener.accept())
                .await
                .unwrap()
                .unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("POST"));
        assert_eq!(request.split_whitespace().nth(1), Some("/csp"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("content-type: application/reports+json")
        );
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
        assert!(request.contains("\"type\":\"csp-violation\""));
        assert!(request.contains("\"body\":{"));
        assert!(request.contains(&format!(
            "\"blocked-uri\":\"http://{page_address}/blocked\""
        )));
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{page_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("await fetch('/blocked').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("allowed")
    );
    page_server.await.unwrap();
    report_server.await.unwrap();
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_content_process_resolves_network_css_urls_against_stylesheet() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/styles/theme/main.css", "/styles/assets/dot.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            match expected_path {
                "/page" => {
                    let body = "<style>body{margin:0px}</style><link rel='stylesheet' href='/styles/theme/main.css'><div id='surface'></div>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/styles/theme/main.css" => {
                    let body = "#surface { width: 8px; height: 8px; background: url('../assets/dot.png') no-repeat 0 0 / 100% 100%; }";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/styles/assets/dot.png" => {
                    let headers = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    stream.write_all(headers.as_bytes()).await.unwrap();
                    stream.write_all(&png).await.unwrap();
                }
                _ => unreachable!(),
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 8,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let list = engine.display_list().unwrap();
    assert_eq!(
        list.commands
            .iter()
            .filter(|command| matches!(command, NativeDisplayCommand::Image { .. }))
            .count(),
        1
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_network_css_import_graph() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/styles/main.css",
            "/styles/theme/base.css",
            "/styles/theme/colors.css",
            "/styles/assets/dot.png",
        ] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            match expected_path {
                "/page" => {
                    let body = "<style>body{margin:0px}</style><link rel='stylesheet' href='/styles/main.css'><div id='surface'>Imported</div>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/styles/main.css" => {
                    let body = "@import '/styles/theme/base.css' layer(theme) screen; @import '/styles/print.css' print;";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/styles/theme/base.css" => {
                    let body = "@import './colors.css'; #surface { width: 8px; height: 8px; background: url('../assets/dot.png') no-repeat 0 0 / 100% 100%; }";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/styles/theme/colors.css" => {
                    let body = "#surface { color: red; }";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/styles/assets/dot.png" => {
                    let headers = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    stream.write_all(headers.as_bytes()).await.unwrap();
                    stream.write_all(&png).await.unwrap();
                }
                _ => unreachable!(),
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 8,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("getComputedStyle(document.getElementById('surface')).color")
            .await
            .unwrap(),
        serde_json::json!("rgb(255, 0, 0)")
    );
    assert_eq!(
        engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .filter(|command| matches!(command, NativeDisplayCommand::Image { .. }))
            .count(),
        1
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_preserves_fifo_network_task_order() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/first", "/second"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = if expected_path == "/page" {
                ("text/html", "<main>fifo network task</main>")
            } else {
                ("text/plain", expected_path.trim_start_matches('/'))
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
                "globalThis.networkOrder = []; await Promise.all([fetch('/first').then(() => globalThis.networkOrder.push('first')), fetch('/second').then(() => globalThis.networkOrder.push('second'))]); globalThis.networkOrder",
            )
            .await
            .unwrap(),
        serde_json::json!(["first", "second"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_round_robins_due_timer_between_network_tasks() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/first", "/second"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/page" {
                "<main>round robin task sources</main>"
            } else {
                expected_path.trim_start_matches('/')
            };
            let content_type = if expected_path == "/page" {
                "text/html"
            } else {
                "text/plain"
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
                "globalThis.taskSourceOrder = []; await new Promise(resolve => { let remaining = 3; const complete = label => { globalThis.taskSourceOrder.push(label); if (--remaining === 0) resolve(); }; fetch('/first').then(() => complete('first')); fetch('/second').then(() => complete('second')); setTimeout(() => complete('timer'), 0); }); globalThis.taskSourceOrder",
            )
            .await
            .unwrap(),
        serde_json::json!(["first", "timer", "second"])
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

use super::*;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const NATIVE_LOAD_TRACE_CHILD: &str = "GLASS_NATIVE_LOAD_TRACE_CHILD";
const NATIVE_LOAD_TRACE_URL: &str = "GLASS_NATIVE_LOAD_TRACE_URL";
const NATIVE_LOAD_TRACE_STORAGE: &str = "GLASS_NATIVE_LOAD_TRACE_STORAGE";

/// Isolated subprocess entrypoint for the trace regression. Only this child
/// receives `GLASS_RUNTIME_DIAGNOSTIC_TRACE`, so unrelated tests cannot add
/// entries to or mutate the assertions for its trace file.
#[tokio::test]
async fn native_load_trace_child_helper() {
    if std::env::var_os(NATIVE_LOAD_TRACE_CHILD).is_none() {
        return;
    }
    let _guard = native_content_process_test_lock().lock().await;
    let url = std::env::var(NATIVE_LOAD_TRACE_URL).expect("fixture URL from parent test");
    let storage = std::env::var_os(NATIVE_LOAD_TRACE_STORAGE)
        .expect("trace storage directory from parent test");
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(url)
            .with_storage_path(storage),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn native_load_trace_times_bounded_resource_stages_without_page_data() {
    const RESOURCE_DELAY: Duration = Duration::from_millis(80);
    // The 16-entry top-level stylesheet cap with the per-request 30s timeout
    // permits 480s of serial wait by itself, while one content-process load
    // expires after 60s. The fixture below proves the trace exposes cumulative
    // serial waits without spending time near either production bound.
    assert!(Duration::from_secs(16 * 30) > Duration::from_secs(60));
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page", "/a.css", "/b.css", "/a.png", "/b.png", "/a.js", "/b.js",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path != "/page" {
                tokio::time::sleep(RESOURCE_DELAY).await;
            }
            let (content_type, body): (&str, &[u8]) = match expected_path {
                "/page" => (
                    "text/html",
                    b"<html><head><link rel='stylesheet' href='/a.css'><link rel='stylesheet' href='/b.css'><script src='/a.js'></script><script src='/b.js'></script></head><body>NativeLoadBodyCanary<img src='/a.png'><img src='/b.png'></body></html>",
                ),
                "/a.css" => ("text/css", b"body { color: #111; }"),
                "/b.css" => ("text/css", b"body { background: #fff; }"),
                "/a.png" | "/b.png" => ("image/png", &png),
                "/a.js" | "/b.js" => ("text/javascript", b"window.fixtureScriptLoaded = true;"),
                _ => unreachable!("server only accepts the fixed fixture routes"),
            };
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
        }
    });

    // Keep the trace inside the workspace target tree rather than /tmp: the
    // native worker has a private /tmp mount, while its explicit storage path
    // is mounted read/write into its sandbox.
    let trace_root = std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!("glass-native-load-trace-{}", std::process::id()));
    std::fs::create_dir_all(&trace_root).unwrap();
    let trace_storage = trace_root.join("profile");
    std::fs::create_dir_all(&trace_storage).unwrap();
    let storage_path = trace_storage.join("profile.json");
    let trace_base = trace_storage.join("diagnostic.jsonl");
    let trace_path = trace_storage.join("diagnostic.native_load.jsonl");
    let url = format!("http://{address}/page");
    let executable = std::env::current_exe().unwrap();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(executable)
            .args([
                "--exact",
                "content_process::network_streaming::network_resources::native_load_trace_child_helper",
            ])
            .env(NATIVE_LOAD_TRACE_CHILD, "1")
            .env(NATIVE_LOAD_TRACE_URL, url)
            .env(NATIVE_LOAD_TRACE_STORAGE, storage_path)
            .env("GLASS_RUNTIME_DIAGNOSTIC_TRACE", trace_base)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "trace child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    server.await.unwrap();

    let trace = std::fs::read_to_string(&trace_path).unwrap();
    assert!(!trace.contains("NativeLoadBodyCanary"), "{trace}");
    assert!(!trace.contains("127.0.0.1"), "{trace}");
    for path in ["/page", "/a.css", "/a.png", "/a.js"] {
        assert!(
            !trace.contains(path),
            "trace leaked resource path {path}: {trace}"
        );
    }
    let records = trace
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let stage_complete = |name: &str| {
        records
            .iter()
            .find(|record| record["stage"] == name && record["phase"] == "complete")
            .unwrap_or_else(|| panic!("missing completed {name} stage: {records:?}"))
    };
    let stage_start_index = |name: &str| {
        records
            .iter()
            .position(|record| record["stage"] == name && record["phase"] == "start")
            .unwrap_or_else(|| panic!("missing start for {name}: {records:?}"))
    };
    let stage_end_index = |name: &str| {
        records
            .iter()
            .position(|record| record["stage"] == name && record["phase"] == "complete")
            .unwrap_or_else(|| panic!("missing completion for {name}: {records:?}"))
    };
    let ordered_stages = [
        "navigation_response",
        "document_discovery_parse",
        "stylesheet_fetches",
        "document_stylesheet_parse",
        "font_faces",
        "image_preloads",
        "images",
        "video_posters",
        "media",
        "script_sources",
        "document_wire_encode",
    ];
    for pair in ordered_stages.windows(2) {
        assert!(
            stage_end_index(pair[0]) < stage_start_index(pair[1]),
            "stage order mismatch: {} before {} in {records:?}",
            pair[0],
            pair[1]
        );
    }
    let ordered_wire_stages = [
        "document_wire_computed_styles",
        "document_wire_nodes",
        "document_wire_resources",
    ];
    for stage in ordered_wire_stages {
        stage_complete(stage);
    }
    for pair in ordered_wire_stages.windows(2) {
        assert!(
            stage_end_index(pair[0]) < stage_start_index(pair[1]),
            "wire stage order mismatch: {} before {} in {records:?}",
            pair[0],
            pair[1]
        );
    }
    for stage in [
        "javascript_host_view_eval",
        "page_script_execution",
        "content_process_mutation",
        "lifecycle_listener_lookup",
    ] {
        assert!(
            records
                .iter()
                .any(|record| record["stage"] == stage && record["phase"] == "complete"),
            "missing completed post-load stage {stage}: {records:?}"
        );
    }
    assert!(
        !records
            .iter()
            .any(|record| record["stage"] == "lifecycle_event_evaluation"),
        "a page with no lifecycle listeners should not rebuild the full JS bridge: {records:?}"
    );
    for stage in ["dom_content_loaded_dispatch", "page_load_event_dispatch"] {
        let elapsed = stage_complete(stage)["elapsed_us"].as_u64().unwrap();
        assert!(
            elapsed < 2_000_000,
            "no-listener {stage} should skip repeated bridge setup, took {elapsed}µs: {records:?}"
        );
    }
    let slow_threshold_us = RESOURCE_DELAY.as_micros() / 2;
    for stage in ["stylesheet_fetches", "images", "script_sources"] {
        assert!(
            stage_complete(stage)["elapsed_us"].as_u64().unwrap() as u128 >= slow_threshold_us * 2,
            "{stage} should include both controlled delayed responses: {records:?}"
        );
    }
    for (resource_type, count) in [
        ("stylesheet", 2_usize),
        ("image", 2_usize),
        ("external_script", 2_usize),
    ] {
        let spans = records
            .iter()
            .enumerate()
            .filter(|(_, record)| record["stage"] == "resource_fetch")
            .filter(|(_, record)| record["resource_type"] == resource_type)
            .collect::<Vec<_>>();
        assert_eq!(spans.len(), count * 2, "{resource_type}: {spans:?}");
        let mut ordinal_phases = BTreeMap::<u64, (Option<usize>, Option<usize>)>::new();
        for (position, record) in spans {
            assert!(record.get("url").is_none());
            assert!(record.get("body").is_none());
            let ordinal = record["ordinal"].as_u64().unwrap();
            let phases = ordinal_phases.entry(ordinal).or_default();
            if record["phase"] == "start" {
                phases.0 = Some(position);
            } else {
                assert_eq!(record["phase"], "complete");
                assert!(record["elapsed_us"].as_u64().unwrap() as u128 >= slow_threshold_us);
                phases.1 = Some(position);
            }
        }
        assert_eq!(ordinal_phases.len(), count);
        for (ordinal, (start, end)) in ordinal_phases {
            assert!(
                start.unwrap() < end.unwrap(),
                "{resource_type} #{ordinal} phase order"
            );
        }
    }
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&trace_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(trace_root).unwrap();
}

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

use super::*;

#[tokio::test]
async fn native_content_process_revalidates_stylesheet_and_script_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut stylesheet_requests = 0;
        let mut script_requests = 0;
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            match path {
                "/page" => {
                    let body = "<link rel='stylesheet' href='/style.css'><link rel='stylesheet' href='/style.css'><script src='/app.js'></script><script src='/app.js'></script><div id='target'>Cached assets</div>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/style.css" => {
                    stylesheet_requests += 1;
                    if stylesheet_requests == 1 {
                        let body = "#target { color: red; }";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nCache-Control: no-cache\r\nETag: \"style-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    } else {
                        assert_eq!(stylesheet_requests, 2);
                        assert!(
                            request.lines().any(
                                |line| line.eq_ignore_ascii_case("if-none-match: \"style-v1\"")
                            )
                        );
                        stream
                            .write_all(
                                b"HTTP/1.1 304 Not Modified\r\nCache-Control: no-cache\r\nETag: \"style-v1\"\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .unwrap();
                    }
                }
                "/app.js" => {
                    script_requests += 1;
                    if script_requests == 1 {
                        let body = "globalThis.externalRuns = (globalThis.externalRuns || 0) + 1;";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nCache-Control: no-cache\r\nETag: \"script-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    } else {
                        assert_eq!(script_requests, 2);
                        assert!(
                            request
                                .lines()
                                .any(|line| line
                                    .eq_ignore_ascii_case("if-none-match: \"script-v1\""))
                        );
                        stream
                            .write_all(
                                b"HTTP/1.1 304 Not Modified\r\nCache-Control: no-cache\r\nETag: \"script-v1\"\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .unwrap();
                    }
                }
                other => panic!("unexpected subresource request: {other}"),
            }
        }
        assert_eq!(stylesheet_requests, 2);
        assert_eq!(script_requests, 2);
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({ runs: globalThis.externalRuns, color: getComputedStyle(document.getElementById('target')).color })",
            )
            .await
            .unwrap(),
        serde_json::json!({ "runs": 2, "color": "rgb(255, 0, 0)" })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_applies_stylesheet_link_referrer_policy() {
    fn request_header(request: &str, name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
        })
    }

    fn css_response(body: &str, cache_headers: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\n{cache_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let stylesheet_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stylesheet_address = stylesheet_listener.local_addr().unwrap();
    let document_url = format!("http://{page_address}/page?secret=token#fragment");
    let document_referer = format!("http://{page_address}/page?secret=token");
    let document_origin = format!("http://{page_address}/");

    let page_server_referer = document_referer.clone();
    let page_server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/same.css",
            "/import.css",
            "/cache.css",
            "/cache.css",
            "/dynamic.css",
            "/dynamic-import.css",
            "/dynamic-cookie-check",
        ] {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(20), page_listener.accept())
                    .await
                    .unwrap_or_else(|_| {
                        panic!("timed out waiting for page-origin request {expected_path}")
                    })
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            let expected_request_target = if expected_path == "/page" {
                "/page?secret=token"
            } else {
                expected_path
            };
            assert_eq!(
                request.split_whitespace().nth(1),
                Some(expected_request_target)
            );
            match expected_path {
                "/page" => {
                    let body = format!(
                        "<link id='same' rel='stylesheet' href='/same.css' referrerpolicy='no-referrer'>\
                         <link id='default' rel='stylesheet' href='http://{stylesheet_address}/default.css'>\
                         <link id='empty' rel='stylesheet' href='http://{stylesheet_address}/empty.css' referrerpolicy=''>\
                         <link id='origin' rel='stylesheet' href='http://{stylesheet_address}/origin.css' referrerpolicy='ORIGIN'>\
                         <link id='invalid' rel='stylesheet' href='http://{stylesheet_address}/invalid.css' referrerpolicy='not-a-policy'>\
                         <link id='none' rel='stylesheet' href='http://{stylesheet_address}/none.css' referrerpolicy='no-referrer'>\
                         <link id='redirect' rel='stylesheet' href='http://{stylesheet_address}/redirect.css' referrerpolicy='no-referrer'>\
                         <link id='cache' rel='stylesheet' href='/cache.css'>\
                         <div id='target'>Stylesheet link policy</div>"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: page_visible=present; Path=/; SameSite=Lax\r\nSet-Cookie: page_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nReferrer-Policy: unsafe-url\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/same.css" => {
                    assert_eq!(request_header(&request, "referer"), None);
                    let body = "@import url('/import.css'); #target { color: red; }";
                    stream
                        .write_all(css_response(body, "Cache-Control: no-store\r\n").as_bytes())
                        .await
                        .unwrap();
                }
                "/import.css" => {
                    // @import is a distinct request path and does not inherit the link's
                    // no-referrer policy in this bounded slice.
                    assert_eq!(
                        request_header(&request, "referer").as_deref(),
                        Some(page_server_referer.as_str())
                    );
                    let body = "#target { font-style: italic; }";
                    stream
                        .write_all(css_response(body, "Cache-Control: no-store\r\n").as_bytes())
                        .await
                        .unwrap();
                }
                "/cache.css" if request_header(&request, "if-none-match").is_none() => {
                    assert_eq!(
                        request_header(&request, "referer").as_deref(),
                        Some(page_server_referer.as_str())
                    );
                    let body = "#target { text-decoration: underline; }";
                    stream
                        .write_all(
                            css_response(body, "Cache-Control: no-cache\r\nETag: \"style-v1\"\r\n")
                                .as_bytes(),
                        )
                        .await
                        .unwrap();
                }
                "/cache.css" => {
                    assert_eq!(request_header(&request, "referer"), None);
                    assert_eq!(
                        request_header(&request, "if-none-match").as_deref(),
                        Some("\"style-v1\"")
                    );
                    stream
                        .write_all(
                            b"HTTP/1.1 304 Not Modified\r\nCache-Control: no-cache\r\nETag: \"style-v1\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .await
                        .unwrap();
                }
                "/dynamic.css" => {
                    assert!(request.contains("page_visible=present"));
                    assert!(request.contains("page_secret=hidden"));
                    let body = "@import url('/dynamic-import.css'); #target { color: blue; }";
                    stream
                        .write_all(
                            css_response(
                                body,
                                concat!(
                                    "Set-Cookie: dynamic_style_visible=present; Path=/; SameSite=Lax\r\n",
                                    "Set-Cookie: dynamic_style_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                                    "Cache-Control: no-store\r\n"
                                ),
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                }
                "/dynamic-import.css" => {
                    assert!(request.contains("dynamic_style_visible=present"));
                    assert!(request.contains("dynamic_style_secret=hidden"));
                    let body = "#target { font-weight: bold; }";
                    stream
                        .write_all(css_response(body, "Cache-Control: no-store\r\n").as_bytes())
                        .await
                        .unwrap();
                }
                "/dynamic-cookie-check" => {
                    assert!(request.contains("dynamic_style_visible=present"));
                    assert!(request.contains("dynamic_style_secret=hidden"));
                    stream
                        .write_all(
                            b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .await
                        .unwrap();
                }
                _ => unreachable!(),
            }
        }
    });

    let stylesheet_origin = document_origin.clone();
    let stylesheet_full_referrer = document_referer.clone();
    let stylesheet_server = tokio::spawn(async move {
        let cases = [
            ("/default.css", Some(stylesheet_full_referrer.clone())),
            ("/empty.css", Some(stylesheet_full_referrer.clone())),
            ("/origin.css", Some(stylesheet_origin.clone())),
            ("/invalid.css", Some(stylesheet_full_referrer.clone())),
            ("/none.css", None),
            ("/redirect.css", None),
            ("/redirect-target.css", Some(stylesheet_origin.clone())),
            ("/dynamic-origin.css", Some(stylesheet_origin)),
            ("/dynamic-default.css", None),
        ];
        for (expected_path, expected_referer) in cases {
            let (mut stream, _) =
                tokio::time::timeout(Duration::from_secs(20), stylesheet_listener.accept())
                    .await
                    .unwrap_or_else(|_| {
                        panic!("timed out waiting for stylesheet-origin request {expected_path}")
                    })
                    .unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            assert_eq!(
                request_header(&request, "referer"),
                expected_referer,
                "{expected_path}"
            );
            if expected_path == "/redirect.css" {
                stream
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /redirect-target.css\r\nReferrer-Policy: origin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            } else {
                let body = "#target { }";
                stream
                    .write_all(css_response(body, "Cache-Control: no-store\r\n").as_bytes())
                    .await
                    .unwrap();
            }
        }
    });

    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(document_url)).unwrap();
    tokio::time::timeout(Duration::from_secs(45), engine.initialize_async())
        .await
        .expect("native engine initialization timed out")
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const origin = document.getElementById('origin'); const invalid = document.getElementById('invalid'); return [origin.referrerPolicy, invalid.referrerPolicy, invalid.getAttribute('referrerpolicy')]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(["ORIGIN", "", "not-a-policy"])
    );
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(45),
            engine.evaluate_async(&format!(
                "(() => {{ const meta = document.createElement('meta'); meta.name = 'referrer'; meta.content = 'no-referrer'; document.head.appendChild(meta); const cached = document.createElement('link'); cached.rel = 'stylesheet'; cached.href = '/cache.css'; cached.referrerPolicy = 'no-referrer'; document.head.appendChild(cached); const origin = document.createElement('link'); origin.rel = 'stylesheet'; origin.href = 'http://{stylesheet_address}/dynamic-origin.css'; origin.referrerPolicy = 'ORIGIN'; document.head.appendChild(origin); const fallback = document.createElement('link'); fallback.rel = 'stylesheet'; fallback.href = 'http://{stylesheet_address}/dynamic-default.css'; document.head.appendChild(fallback); const dynamic = document.createElement('link'); dynamic.rel = 'stylesheet'; dynamic.href = '/dynamic.css'; document.head.appendChild(dynamic); return [meta.content, meta.getAttribute('content'), cached.referrerPolicy, origin.referrerPolicy, origin.getAttribute('referrerpolicy'), fallback.referrerPolicy]; }})()"
            ))
        )
        .await
        .expect("dynamic stylesheet link processing timed out")
        .unwrap(),
        serde_json::json!([
            "no-referrer",
            "no-referrer",
            "no-referrer",
            "ORIGIN",
            "ORIGIN",
            ""
        ])
    );
    let visible_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    let visible_cookies = visible_cookies.as_str().unwrap_or_default();
    assert!(visible_cookies.contains("page_visible=present"));
    assert!(visible_cookies.contains("dynamic_style_visible=present"));
    assert!(!visible_cookies.contains("page_secret"));
    assert!(!visible_cookies.contains("dynamic_style_secret"));
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/dynamic-cookie-check').then(response => response.status)",
            )
            .await
            .unwrap(),
        serde_json::json!(204)
    );
    let parent_cookies = engine.cookies_async().await.unwrap();
    for (name, value, http_only) in [
        ("page_visible", "present", false),
        ("page_secret", "hidden", true),
        ("dynamic_style_visible", "present", false),
        ("dynamic_style_secret", "hidden", true),
    ] {
        assert!(parent_cookies.iter().any(|cookie| {
            cookie.name == name && cookie.value == value && cookie.http_only == http_only
        }));
    }
    assert_eq!(
        engine
            .evaluate_async(
                "({ color: getComputedStyle(document.getElementById('target')).color, fontStyle: getComputedStyle(document.getElementById('target')).fontStyle, fontWeight: getComputedStyle(document.getElementById('target')).fontWeight })",
            )
            .await
            .unwrap(),
        serde_json::json!({ "color": "rgb(0, 0, 255)", "fontStyle": "italic", "fontWeight": "bold" })
    );

    engine.close_async().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), page_server)
        .await
        .expect("page-origin stylesheet request fixture did not finish")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), stylesheet_server)
        .await
        .expect("stylesheet-origin request fixture did not finish")
        .unwrap();
}
#[tokio::test]
async fn native_content_process_enforces_subresource_integrity_for_scripts_and_styles() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let good_style = "#good { display: block; width: 80px; height: 24px; }";
    let bad_style = "#bad { display: block; width: 17px; height: 19px; }";
    let good_script = "globalThis.sriGood = 'executed';";
    let bad_script = "globalThis.sriBad = 'executed';";
    let digest = |body: &str| {
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body.as_bytes()))
    };
    let good_style_hash = digest(good_style);
    let good_script_hash = digest(good_script);
    let wrong_hash = base64::engine::general_purpose::STANDARD.encode([0_u8; 32]);
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/good.css", "/bad.css", "/good.js", "/bad.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let (content_type, body) = match expected_path {
                "/page" => (
                    "text/html",
                    format!(
                        "<link rel='stylesheet' href='/good.css' integrity='sha256-{good_style_hash}'><link rel='stylesheet' href='/bad.css' integrity='sha256-{wrong_hash}'><script src='/good.js' integrity='sha256-{good_script_hash}'></script><script src='/bad.js' integrity='sha256-{wrong_hash}'></script><button id='good'>Good</button><button id='bad'>Bad</button>"
                    ),
                ),
                "/good.css" => ("text/css", good_style.to_owned()),
                "/bad.css" => ("text/css", bad_style.to_owned()),
                "/good.js" => ("application/javascript", good_script.to_owned()),
                "/bad.js" => ("application/javascript", bad_script.to_owned()),
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
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let good = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Good")
        .unwrap();
    let bad = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Bad")
        .unwrap();
    assert_eq!(
        engine
            .layout()
            .unwrap()
            .box_for(good.node_id)
            .unwrap()
            .width,
        80
    );
    assert_ne!(
        engine.layout().unwrap().box_for(bad.node_id).unwrap().width,
        17
    );
    assert_eq!(
        engine
            .evaluate_async("[globalThis.sriGood, typeof globalThis.sriBad]")
            .await
            .unwrap(),
        serde_json::json!(["executed", "undefined"])
    );
    assert_eq!(
        engine
            .effects_since(0)
            .unwrap()
            .effects
            .iter()
            .filter(|effect| effect.kind == NativeEventKind::Error)
            .count(),
        2
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_same_origin_stylesheet_under_csp() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/style.css"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/style.css" {
                let referrer = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("referer")
                            .then_some(value.trim().to_owned())
                    })
                });
                assert_eq!(referrer, Some(format!("http://{address}/page")));
            }
            let (content_type, extra_headers, body) = if expected_path == "/page" {
                (
                    "text/html",
                    "Content-Security-Policy: default-src 'none'; style-src 'self'\r\n",
                    "<link rel='stylesheet' href='/style.css'><button id='target'>Styled</button>",
                )
            } else {
                ("text/css", "", "#target { width: 80px; height: 24px; }")
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
    let target = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Styled")
        .unwrap();
    assert_eq!(
        engine.layout().unwrap().box_for(target.node_id),
        Some(NativeRect {
            x: 8,
            y: 8,
            width: 80,
            height: 24,
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_blocks_csp_disallowed_stylesheet_before_request() {
    let _guard = native_content_process_test_lock().lock().await;
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let (target_request, target_request_rx) = tokio::sync::oneshot::channel();
    let target_server = tokio::spawn(async move {
        if let Ok(Ok((mut stream, _))) =
            tokio::time::timeout(Duration::from_secs(1), target_listener.accept()).await
        {
            let mut request = [0_u8; 4096];
            if let Ok(Ok(read)) =
                tokio::time::timeout(Duration::from_millis(250), stream.read(&mut request)).await
                && request[..read].starts_with(b"GET /style.css ")
            {
                let _ = target_request.send(());
            }
        }
    });

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = format!(
            "<link rel='stylesheet' href='http://{target_address}/style.css'><p id='target'>Unstyled</p>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; style-src 'self'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert!(
        !matches!(
            tokio::time::timeout(Duration::from_millis(250), target_request_rx).await,
            Ok(Ok(()))
        ),
        "CSP-disallowed stylesheet was requested"
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
    target_server.abort();
    let _ = target_server.await;
}
#[tokio::test]
async fn native_content_process_transfers_computed_style_for_layout() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "<style>body{margin:0px} #target { width: 80px; height: 24px; }</style><button id='target'>Target</button>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/styled")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let target = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Target")
        .unwrap();
    let layout = engine.layout().unwrap();
    assert_eq!(
        layout.box_for(target.node_id),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        })
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_paints_animated_gif_frames() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let gif = native_test_animated_gif_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/photo.gif"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/photo.gif" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/gif\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    gif.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&gif).await.unwrap();
            } else {
                let body =
                    "<img id='image' src='/photo.gif' style='width:1px;height:1px' alt='animated'>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let image_pixels = |engine: &NativeEngine| {
        engine
            .display_list()
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
                _ => None,
            })
            .unwrap()
    };
    let initial = image_pixels(&engine);
    let mut changed = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        if image_pixels(&engine) != initial {
            changed = true;
            break;
        }
    }
    assert!(
        changed,
        "content-process GIF did not advance to another frame"
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_paints_animated_webp_frames() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let webp = native_test_animated_webp_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/photo.webp"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/photo.webp" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/webp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    webp.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&webp).await.unwrap();
            } else {
                let body = "<img id='image' src='/photo.webp' style='width:1px;height:1px' alt='animated'>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let image_pixels = |engine: &NativeEngine| {
        engine
            .display_list()
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
                _ => None,
            })
            .unwrap()
    };
    let initial = image_pixels(&engine);
    let mut changed = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        if image_pixels(&engine) != initial {
            changed = true;
            break;
        }
    }
    assert!(
        changed,
        "content-process WebP did not advance to another frame"
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_paints_animated_apng_frames() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let apng = native_test_animated_apng_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/photo.apng"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/photo.apng" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/apng\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    apng.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&apng).await.unwrap();
            } else {
                let body = "<img id='image' src='/photo.apng' style='width:1px;height:1px' alt='animated'>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let image_pixels = |engine: &NativeEngine| {
        engine
            .display_list()
            .unwrap()
            .commands
            .into_iter()
            .find_map(|command| match command {
                NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
                _ => None,
            })
            .unwrap()
    };
    let initial = image_pixels(&engine);
    let mut changed = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        if image_pixels(&engine) != initial {
            changed = true;
            break;
        }
    }
    assert!(
        changed,
        "content-process APNG did not advance to another frame"
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_dispatches_external_image_error_event() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/missing.png", "/missing-again.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path != "/page" {
                stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            } else {
                let body = "<img id='image' src='/missing.png'><script>globalThis.imageEvents = []; const image = document.getElementById('image'); image.addEventListener('load', () => imageEvents.push('load')); image.addEventListener('error', (event) => imageEvents.push(event.type)); image.onerror = (event) => imageEvents.push('property-' + event.type);</script>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
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
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc, imageEvents]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 0, 0, format!("http://{address}/missing.png"), ["error", "property-error"]])
    );
    engine
        .evaluate_async(
            "(() => { const image = document.getElementById('image'); image.onerror = () => imageEvents.push('replacement-error'); image.src = '/missing-again.png'; return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc, imageEvents]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 0, 0, format!("http://{address}/missing-again.png"), ["error", "property-error", "error", "replacement-error"]])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_background_png_after_style_mutation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/image.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/image.png" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&png).await.unwrap();
            } else {
                let body = "<style>body{margin:0px}</style><div id='surface' style='width:8px;height:8px'></div>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
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
            .evaluate_async(
                "(() => { const surface = document.getElementById('surface'); surface.style.background = \"url('/image.png') no-repeat 0 0 / 100% 100%\"; return true; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(true)
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
    assert_eq!(
        engine
            .display_list()
            .unwrap()
            .rasterize()
            .unwrap()
            .pixel(1, 1),
        Some([255, 0, 0, 255])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_image_after_script_source_mutation() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/image.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/image.png" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&png).await.unwrap();
            } else {
                let body = "<style>body{margin:0px}</style><img id='image' width='8'><script>globalThis.imageLoaded = 0; document.getElementById('image').addEventListener('load', () => { globalThis.imageLoaded = 1; });</script>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 16,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { document.getElementById('image').src = '/image.png'; return true; })()"
            )
            .await
            .unwrap(),
        serde_json::json!(true)
    );

    let list = engine.display_list().unwrap();
    let image_id = list
        .commands
        .iter()
        .find_map(|command| match command {
            NativeDisplayCommand::Image { node_id, .. } => Some(*node_id),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        engine.layout().unwrap().box_for(image_id),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        })
    );
    assert_eq!(
        list.rasterize().unwrap().pixel(1, 1),
        Some([255, 0, 0, 255])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.imageLoaded")
            .await
            .unwrap(),
        serde_json::json!(1)
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_blob_object_url_image_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<img id='image' width='2' height='2'><div id='surface' style='width:2px;height:2px'></div><script>globalThis.imageLoaded = 0; globalThis.imageFailed = 0; const image = document.getElementById('image'); image.addEventListener('load', () => { globalThis.imageLoaded = 1; }); image.addEventListener('error', () => { globalThis.imageFailed = 1; });</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'; img-src blob:\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); const surface = document.getElementById('surface'); const url = URL.createObjectURL(new Blob(['<svg xmlns=\\\"http://www.w3.org/2000/svg\\\" width=\\\"2\\\" height=\\\"2\\\"><rect width=\\\"2\\\" height=\\\"2\\\" fill=\\\"red\\\"/></svg>'], { type: 'image/svg+xml' })); image.src = url; surface.style.backgroundImage = `url(\\\"${url}\\\")`; return url; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(format!("blob:http://{address}/glass-native-1"))
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, image.currentSrc, imageLoaded, imageFailed]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            true,
            2,
            2,
            format!("blob:http://{address}/glass-native-1"),
            1,
            0,
        ])
    );
    assert_eq!(
        engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .filter(|command| matches!(command, NativeDisplayCommand::Image { .. }))
            .count(),
        2
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_blocks_csp_disallowed_blob_image_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<img id='image' width='2' height='2'><script>globalThis.imageError = 0; document.getElementById('image').addEventListener('error', () => { globalThis.imageError += 1; });</script>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'; img-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const image = document.getElementById('image'); const url = URL.createObjectURL(new Blob(['<svg xmlns=\\\"http://www.w3.org/2000/svg\\\" width=\\\"2\\\" height=\\\"2\\\"></svg>'], { type: 'image/svg+xml' })); image.src = url; return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('image'); return [image.complete, image.naturalWidth, image.naturalHeight, imageError]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 0, 0, 1])
    );
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| matches!(command, NativeDisplayCommand::Image { .. }))
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_blob_object_url_stylesheet_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<head></head><body><div id='target'>Blob stylesheet</div></body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: style-src blob:\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
            .evaluate_async(
                "(() => { globalThis.blobStyleLoaded = false; const link = document.createElement('link'); link.setAttribute('rel', 'stylesheet'); const url = URL.createObjectURL(new Blob(['#target { color: rgb(1, 2, 3); }'], { type: 'text/css' })); link.href = url; link.addEventListener('load', () => { globalThis.blobStyleLoaded = true; }); document.head.appendChild(link); globalThis.blobStyleUrl = url; return url; })()",
            )
            .await
            .unwrap(),
        serde_json::json!(format!("blob:http://{address}/glass-native-1"))
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const target = document.getElementById('target'); const link = document.querySelector('link'); return [blobStyleLoaded, getComputedStyle(target).color, link.href]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([
            true,
            "rgb(1, 2, 3)",
            format!("blob:http://{address}/glass-native-1")
        ])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_blocks_csp_disallowed_blob_stylesheet_subresources() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<head></head><body><div id='target'>Blocked stylesheet</div></body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: style-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { globalThis.blobStyleError = 0; const link = document.createElement('link'); link.setAttribute('rel', 'stylesheet'); const url = URL.createObjectURL(new Blob(['#target { color: rgb(1, 2, 3); }'], { type: 'text/css' })); link.href = url; link.addEventListener('error', () => { globalThis.blobStyleError += 1; }); document.head.appendChild(link); return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const target = document.getElementById('target'); return [blobStyleError, getComputedStyle(target).color]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([1, "rgb(0, 0, 0)"])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_reuses_cacheable_external_png_for_duplicate_images() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/image.png"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            if expected_path == "/image.png" {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=60\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&png).await.unwrap();
            } else {
                let body = "<style>body{margin:0px}</style><div style='width:16px'><img id='first' src='/image.png' width='8'><img id='second' src='/image.png' width='8'></div>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
        match tokio::time::timeout(Duration::from_millis(250), listener.accept()).await {
            Ok(Ok((mut stream, _))) => {
                let _ = read_http_request(&mut stream).await;
                let response =
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(response).await.unwrap();
                true
            }
            _ => false,
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 16,
                height: 8,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let image_count = engine
        .display_list()
        .unwrap()
        .commands
        .iter()
        .filter(|command| matches!(command, NativeDisplayCommand::Image { .. }))
        .count();
    assert_eq!(image_count, 2);

    engine.close_async().await.unwrap();
    assert!(!server.await.unwrap(), "duplicate image was requested");
}
#[tokio::test]
async fn native_content_process_parent_owns_html_image_preloads_and_reuses_them() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_solid_png_bytes([40, 120, 220, 255]);
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/reuse.png",
            "/cookie.png",
            "/cookie-check",
            "/dynamic-a.png",
            "/dynamic-b.png",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let cookie_header = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            });
            let has_cookie = |name: &str, value: &str| {
                cookie_header.is_some_and(|cookies| {
                    cookies
                        .split(';')
                        .any(|cookie| cookie.trim().split_once('=') == Some((name, value)))
                })
            };
            match expected_path {
                "/page" => {
                    let body = "<!doctype html><head>\
                        <link rel='preload' as='image' href='/reuse.png'>\
                        <link rel='preload' as='image' href='/reuse.png'>\
                        <link rel='preload' as='image' href='/cookie.png'>\
                        <link rel='preload' as='image' crossorigin='anonymous' href='/unsupported.png'>\
                        </head><body><img id='preloaded' src='/reuse.png'></body>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: seed=parent; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/reuse.png" => {
                    assert!(has_cookie("seed", "parent"));
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.write_all(&png).await.unwrap();
                }
                "/cookie.png" => {
                    assert!(has_cookie("seed", "parent"));
                    assert!(!has_cookie("preload_secret", "hidden"));
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: no-store\r\nSet-Cookie: preload_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.write_all(&png).await.unwrap();
                }
                "/cookie-check" => {
                    assert!(has_cookie("seed", "parent"));
                    assert!(has_cookie("preload_secret", "hidden"));
                    let body = "parent-cookie-ok";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/dynamic-a.png" => {
                    assert!(has_cookie("seed", "parent"));
                    assert!(has_cookie("preload_secret", "hidden"));
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        png.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.write_all(&png).await.unwrap();
                }
                "/dynamic-b.png" => {
                    assert!(has_cookie("seed", "parent"));
                    assert!(has_cookie("preload_secret", "hidden"));
                    stream
                        .write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
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
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('preloaded'); \
                 return [image.complete, image.naturalWidth, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, format!("http://{address}/reuse.png")])
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/cookie-check').then(response => response.text())")
            .await
            .unwrap(),
        serde_json::json!("parent-cookie-ok")
    );
    let cookies = engine.cookies_async().await.unwrap();
    for name in ["seed", "preload_secret"] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only),
            "parent cookie jar is missing HttpOnly cookie {name}"
        );
    }

    engine
        .evaluate_async(
            "globalThis.preloadEvents = []; \
             const link = document.createElement('link'); \
             link.setAttribute('rel', 'preload'); link.setAttribute('as', 'image'); \
             link.addEventListener('load', () => { \
               preloadEvents.push('load'); \
               const image = document.createElement('img'); \
               image.setAttribute('id', 'dynamic-preloaded'); \
               image.setAttribute('src', '/dynamic-a.png'); \
               document.body.appendChild(image); \
             }); \
             link.addEventListener('error', () => preloadEvents.push('error')); \
             globalThis.imagePreloadLink = link; \
             link.setAttribute('href', '/dynamic-a.png'); \
             document.head.appendChild(link); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("preloadEvents").await.unwrap(),
        serde_json::json!(["load"])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.getElementById('dynamic-preloaded'); \
                 return [image.complete, image.naturalWidth, image.currentSrc]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, format!("http://{address}/dynamic-a.png")])
    );
    engine
        .evaluate_async("imagePreloadLink.setAttribute('href', '/dynamic-b.png'); true")
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("preloadEvents").await.unwrap(),
        serde_json::json!(["load", "error"])
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_video_poster_uses_document_referrer_policy() {
    let _guard = native_content_process_test_lock().lock().await;
    let poster_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let poster_address = poster_listener.local_addr().unwrap();
    let (request_tx, request_rx) = tokio::sync::oneshot::channel();
    let poster_server = tokio::spawn(async move {
        let (mut stream, _) = poster_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        request_tx.send(request).unwrap();
        let image = native_test_solid_png_bytes([70, 110, 210, 255]);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            image.len()
        )
        .into_bytes();
        response.extend_from_slice(&image);
        stream.write_all(&response).await.unwrap();
    });

    let owner_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let owner_address = owner_listener.local_addr().unwrap();
    assert_ne!(owner_address.port(), poster_address.port());
    let page = format!(
        "<!doctype html><video poster='http://{poster_address}/poster.png' referrerpolicy='unsafe-url'></video>"
    );
    let owner_server = tokio::spawn(async move {
        let (mut stream, _) = owner_listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nReferrer-Policy: no-referrer\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{owner_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    owner_server.await.unwrap();
    let request = tokio::time::timeout(Duration::from_secs(5), request_rx)
        .await
        .expect("poster request should reach its loopback server")
        .unwrap();
    assert!(
        !request.lines().any(|line| {
            line.split_once(':')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case("referer"))
        }),
        "the video attribute must not override the owner's no-referrer policy: {request}"
    );
    engine.close_async().await.unwrap();
    poster_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_video_poster_resolves_against_document_base_url() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let page = "<!doctype html><base id='poster-base' href='/assets/'><video id='poster' poster='cover.png'></video>";
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/assets/cover.png"));
        let image = native_test_solid_png_bytes([90, 190, 60, 255]);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            image.len()
        )
        .into_bytes();
        response.extend_from_slice(&image);
        stream.write_all(&response).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(
            request.split_whitespace().nth(1),
            Some("/changed/cover.png")
        );
        let image = native_test_solid_png_bytes([180, 100, 40, 255]);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            image.len()
        )
        .into_bytes();
        response.extend_from_slice(&image);
        stream.write_all(&response).await.unwrap();

        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/cover.png"));
        let image = native_test_solid_png_bytes([60, 160, 200, 255]);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            image.len()
        )
        .into_bytes();
        response.extend_from_slice(&image);
        stream.write_all(&response).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('poster').poster")
            .await
            .unwrap(),
        serde_json::json!(format!("http://{address}/assets/cover.png"))
    );
    let changed_base_poster = engine
        .evaluate_async(
            "document.getElementById('poster-base').setAttribute('href', '/changed/'); document.getElementById('poster').poster",
        )
        .await
        .unwrap();
    assert_eq!(
        changed_base_poster,
        serde_json::json!(format!("http://{address}/changed/cover.png"))
    );
    let removed_base_poster = engine
        .evaluate_async(
            "document.getElementById('poster-base').removeAttribute('href'); document.getElementById('poster').poster",
        )
        .await
        .unwrap();
    assert_eq!(
        removed_base_poster,
        serde_json::json!(format!("http://{address}/cover.png"))
    );
    server.await.unwrap();
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_content_process_blocks_csp_disallowed_video_poster_before_request() {
    let _guard = native_content_process_test_lock().lock().await;
    let poster_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let poster_address = poster_listener.local_addr().unwrap();
    let owner_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let owner_address = owner_listener.local_addr().unwrap();
    let owner_server = tokio::spawn(async move {
        let (mut stream, _) = owner_listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let page = format!(
            "<!doctype html><video poster='http://{poster_address}/blocked.png' style='width:80px;height:40px'></video>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Security-Policy: default-src 'none'; img-src 'self'\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{owner_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    owner_server.await.unwrap();
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| matches!(command, NativeDisplayCommand::Image { .. }))
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), poster_listener.accept())
            .await
            .is_err(),
        "CSP-disallowed video poster reached the network"
    );
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_content_process_blocks_csp_disallowed_image_before_request() {
    let _guard = native_content_process_test_lock().lock().await;
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target_listener.local_addr().unwrap();
    let (target_request, target_request_rx) = tokio::sync::oneshot::channel();
    let target_server = tokio::spawn(async move {
        if let Ok(Ok((mut stream, _))) =
            tokio::time::timeout(Duration::from_secs(1), target_listener.accept()).await
        {
            let mut request = [0_u8; 4096];
            if let Ok(Ok(read)) =
                tokio::time::timeout(Duration::from_millis(250), stream.read(&mut request)).await
                && request[..read].starts_with(b"GET /image.png ")
            {
                let _ = target_request.send(());
            }
        }
    });

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let body = format!(
            "<img id='blocked' src='http://{target_address}/image.png' width='8'><svg xmlns='http://www.w3.org/2000/svg'><image href='http://{target_address}/image.png' width='8' height='8'/></svg>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; img-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert!(
        !matches!(
            tokio::time::timeout(Duration::from_millis(250), target_request_rx).await,
            Ok(Ok(()))
        ),
        "CSP-disallowed image was requested"
    );
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| {
                matches!(
                    command,
                    NativeDisplayCommand::Image { .. } | NativeDisplayCommand::SvgImage { .. }
                )
            })
    );

    engine.close_async().await.unwrap();
    server.await.unwrap();
    target_server.abort();
    let _ = target_server.await;
}
#[test]
fn native_content_process_image_maps_work_in_top_level_and_same_origin_frames() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..5 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native image-map request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let (content_type, extra_headers, body) = match request.split_whitespace().nth(1) {
                Some("/parent") => (
                    "text/html",
                    "",
                    "<title>HTTP image map</title><img id='top-image' usemap='#top-map' style='display:block;width:100px;height:100px'><map name='top-map'><area id='top-download' shape='rect' coords='0,0,20,20' href='/download' download='map-area.txt' alt='Download area'><area id='top-link' shape='default' href='/stale' alt='Top destination'></map><iframe id='child' src='/child' style='width:120px;height:120px'></iframe><script>document.getElementById('top-link').addEventListener('click', event => { event.currentTarget.setAttribute('href', '/top-dest'); event.currentTarget.setAttribute('target', '_blank'); });</script>",
                ),
                Some("/child") => (
                    "text/html",
                    "",
                    "<title>Frame image map</title><img id='frame-image' usemap='#frame-map' style='display:block;width:100px;height:100px'><map name='frame-map'><area id='frame-link' shape='default' href='/frame-dest' alt='Frame destination'></map>",
                ),
                Some("/top-dest") => ("text/html", "", "<title>Top area popup</title>"),
                Some("/frame-dest") => ("text/html", "", "<title>Frame area destination</title>"),
                Some("/download") => (
                    "application/octet-stream",
                    "Content-Disposition: attachment; filename=server-name.txt\r\n",
                    "native image-map download bytes",
                ),
                other => panic!("unexpected native image-map request: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    session
        .action(SemanticAction::Click {
            target: "point=50,50".into(),
        })
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .any(|target| { !target.active && target.url == format!("http://{address}/top-dest") })
    );

    let frame_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("same-origin image-map frame")
        .id;
    session.select_frame(&frame_id).await.unwrap();
    let frame_observation = session
        .semantic_observe(SemanticObservationLevel::Interactive)
        .await
        .unwrap();
    assert!(
        frame_observation
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .any(|target| target.role == "link" && target.name == "Frame destination")
    );
    session
        .action(SemanticAction::Click {
            target: "id=frame-link".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/frame-dest")
    );

    session.select_frame("native-context:main").await.unwrap();
    assert_eq!(
        session.script("location.pathname").await.unwrap().value,
        serde_json::json!("/parent")
    );

    session
        .action(SemanticAction::Click {
            target: "point=10,10".into(),
        })
        .await
        .unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 2);
    let download_directory = std::env::temp_dir().join(format!(
        "glass-native-image-map-download-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&download_directory).unwrap();
    let outcome = session
        .native_wait_for_download(&download_directory, Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(outcome.suggested_filename, "map-area.txt");
    assert_eq!(
        fs::read(download_directory.join("map-area.txt")).unwrap(),
        b"native image-map download bytes"
    );
    fs::remove_dir_all(&download_directory).unwrap();
    session.close().await.unwrap();
    server.await.unwrap();
        });
    });
}

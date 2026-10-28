use super::*;

#[test]
fn native_content_process_idle_page_timer_brokers_dynamic_stylesheet_cookies() {
    std::thread::Builder::new()
        .name("native-idle-page-timer-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(4 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("idle page timer test runtime should build");
            runtime.block_on(
                native_content_process_idle_page_timer_brokers_dynamic_stylesheet_cookies_inner(),
            );
        })
        .expect("idle page timer test thread should start")
        .join()
        .expect("idle page timer test thread should complete");
}
#[tokio::test]
async fn native_content_process_http_navigation_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let font_bytes = native_test_color_font_bytes();
    let media_bytes = native_test_wav_bytes();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered document navigation should reach the local server")
            .unwrap();
        let request = read_http_request(&mut stream).await;
        let body = format!(
            "<!doctype html><html><head><meta http-equiv='Content-Security-Policy' content=\"img-src 'self'\"><link rel='stylesheet' href='/initial.css'></head><body><img src='/image.png'><img src='http://localhost:{}/blocked.png'><audio src='/media.wav'></audio><script>window.mediaSettled = new Promise(resolve => {{ const media = document.querySelector('audio'); media.addEventListener('load', resolve); media.addEventListener('error', resolve); }});</script><script src='/initial.js'></script><script>window.initialDocumentCookie = document.cookie; const dynamic = document.createElement('script'); dynamic.src = '/initial-dynamic.js'; document.head.appendChild(dynamic);</script></body></html>",
            address.port()
        );
        let response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: navigation_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: navigation_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: text/html; charset=utf-8\r\n",
                "Content-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'\r\n",
                "Referrer-Policy: strict-origin\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered parser stylesheet should reach the local server")
            .unwrap();
        let stylesheet_request = read_http_request(&mut stream).await;
        let stylesheet = "@import '/nested.css';";
        let stylesheet_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: stylesheet_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: stylesheet_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: text/css\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            stylesheet.len(),
            stylesheet
        );
        stream
            .write_all(stylesheet_response.as_bytes())
            .await
            .unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered CSS import should reach the local server")
            .unwrap();
        let dependency_request = read_http_request(&mut stream).await;
        let dependency = "@font-face { font-family: 'Parent Font'; src: url('/font.ttf'); } body { color: black; }";
        let dependency_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: dependency_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: dependency_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: text/css\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            dependency.len(),
            dependency
        );
        stream
            .write_all(dependency_response.as_bytes())
            .await
            .unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered CSS font should reach the local server")
            .unwrap();
        let font_request = read_http_request(&mut stream).await;
        let font_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: font_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: font_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: font/ttf\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n"
            ),
            font_bytes.len()
        );
        stream.write_all(font_response.as_bytes()).await.unwrap();
        stream.write_all(&font_bytes).await.unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered page image should reach the local server")
            .unwrap();
        let image_request = read_http_request(&mut stream).await;
        let image = native_test_png_bytes();
        let image_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: image_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: image_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: image/png\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n"
            ),
            image.len()
        );
        stream.write_all(image_response.as_bytes()).await.unwrap();
        stream.write_all(&image).await.unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered page media should reach the local server")
            .unwrap();
        let media_request = read_http_request(&mut stream).await;
        let media_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: media_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: media_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: audio/wav\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n"
            ),
            media_bytes.len()
        );
        stream.write_all(media_response.as_bytes()).await.unwrap();
        stream.write_all(&media_bytes).await.unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered parser script should reach the local server")
            .unwrap();
        let script_request = read_http_request(&mut stream).await;
        assert!(script_request.starts_with("GET /initial.js HTTP/1.1\r\n"));
        let script = "window.parserScriptCookie = document.cookie;";
        let script_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: parser_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: parser_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: application/javascript\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            script.len(),
            script
        );
        stream.write_all(script_response.as_bytes()).await.unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered initial dynamic script should reach the local server")
            .unwrap();
        let dynamic_script_request = read_http_request(&mut stream).await;
        let dynamic_script = "window.initialDynamicScriptCookie = document.cookie;";
        let dynamic_script_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: initial_dynamic_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: initial_dynamic_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: application/javascript\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            dynamic_script.len(),
            dynamic_script
        );
        stream
            .write_all(dynamic_script_response.as_bytes())
            .await
            .unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered FontFace load should reach the local server")
            .unwrap();
        let runtime_font_request = read_http_request(&mut stream).await;
        let runtime_font_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: runtime_font_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: runtime_font_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: font/ttf\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n"
            ),
            font_bytes.len()
        );
        stream
            .write_all(runtime_font_response.as_bytes())
            .await
            .unwrap();
        stream.write_all(&font_bytes).await.unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("font cookie follow-up should reach the local server")
            .unwrap();
        let font_cookie_followup_request = read_http_request(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        (
            request,
            stylesheet_request,
            dependency_request,
            font_request,
            image_request,
            media_request,
            script_request,
            dynamic_script_request,
            runtime_font_request,
            font_cookie_followup_request,
        )
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let runtime_font = engine
        .evaluate_async(
            r#"await (async () => {
                  const face = new FontFace("Runtime Parent Font", "url(/runtime-font.ttf)");
                  document.fonts.add(face);
                  await face.load();
                  return [face.status, document.fonts.check('16px "Runtime Parent Font"'), document.cookie];
                })()"#,
        )
        .await
        .unwrap();
    assert_eq!(runtime_font.get(0), Some(&serde_json::json!("loaded")));
    assert_eq!(runtime_font.get(1), Some(&serde_json::json!(true)));
    let runtime_cookie_projection = runtime_font
        .get(2)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(runtime_cookie_projection.contains("runtime_font_visible=present"));
    assert!(!runtime_cookie_projection.contains("runtime_font_secret"));
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/font-cookie-followup').then(response => response.status)"
            )
            .await
            .unwrap(),
        serde_json::json!(204)
    );
    for expression in ["window.parserScriptCookie", "window.initialDocumentCookie"] {
        let cookies = engine.evaluate_async(expression).await.unwrap();
        let cookies = cookies.as_str().unwrap_or_default();
        assert!(
            cookies.contains("navigation_visible=present"),
            "{expression}: {cookies}"
        );
        assert!(
            cookies.contains("parser_visible=present"),
            "{expression}: {cookies}"
        );
        assert!(
            cookies.contains("stylesheet_visible=present"),
            "{expression}: {cookies}"
        );
        assert!(
            cookies.contains("dependency_visible=present"),
            "{expression}: {cookies}"
        );
        assert!(
            cookies.contains("image_visible=present"),
            "{expression}: {cookies}"
        );
        assert!(
            cookies.contains("font_visible=present"),
            "{expression}: {cookies}"
        );
        assert!(
            !cookies.contains("navigation_secret"),
            "{expression}: {cookies}"
        );
        assert!(
            !cookies.contains("parser_secret"),
            "{expression}: {cookies}"
        );
        assert!(
            !cookies.contains("stylesheet_secret"),
            "{expression}: {cookies}"
        );
        assert!(
            !cookies.contains("dependency_secret"),
            "{expression}: {cookies}"
        );
        assert!(!cookies.contains("image_secret"), "{expression}: {cookies}");
        assert!(!cookies.contains("media_secret"), "{expression}: {cookies}");
        assert!(!cookies.contains("font_secret"), "{expression}: {cookies}");
    }
    // Audio is a non-blocking parser subresource. Its response cookie is not
    // guaranteed to precede parser-blocking scripts; it must be visible once
    // its own load/error turn has completed.
    engine
        .evaluate_async("await window.mediaSettled")
        .await
        .unwrap();
    let cookies = engine.evaluate_async("document.cookie").await.unwrap();
    let cookies = cookies.as_str().unwrap_or_default();
    assert!(
        cookies.contains("media_visible=present"),
        "document.cookie: {cookies}"
    );
    assert!(
        !cookies.contains("media_secret"),
        "document.cookie: {cookies}"
    );
    let dynamic_script_cookie = engine
        .evaluate_async("window.initialDynamicScriptCookie")
        .await
        .unwrap();
    let dynamic_script_cookie = dynamic_script_cookie.as_str().unwrap_or_default();
    assert!(dynamic_script_cookie.contains("initial_dynamic_visible=present"));
    assert!(!dynamic_script_cookie.contains("initial_dynamic_secret"));
    let cookies = engine.cookies_async().await.unwrap();
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "navigation_visible" && cookie.value == "present" && !cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "navigation_secret" && cookie.value == "hidden" && cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "parser_visible" && cookie.value == "present" && !cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "parser_secret" && cookie.value == "hidden" && cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "initial_dynamic_visible" && cookie.value == "present" && !cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "initial_dynamic_secret" && cookie.value == "hidden" && cookie.http_only
    }));
    for (name, value) in [
        ("stylesheet_visible", "present"),
        ("stylesheet_secret", "hidden"),
        ("dependency_visible", "present"),
        ("dependency_secret", "hidden"),
    ] {
        assert!(cookies.iter().any(|cookie| {
            cookie.name == name
                && cookie.value == value
                && cookie.http_only == name.ends_with("secret")
        }));
    }
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "image_visible" && cookie.value == "present" && !cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "image_secret" && cookie.value == "hidden" && cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "media_visible" && cookie.value == "present" && !cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "media_secret" && cookie.value == "hidden" && cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "font_visible" && cookie.value == "present" && !cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "font_secret" && cookie.value == "hidden" && cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "runtime_font_visible" && cookie.value == "present" && !cookie.http_only
    }));
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "runtime_font_secret" && cookie.value == "hidden" && cookie.http_only
    }));
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const image = document.querySelector('img'); return [image.complete, image.naturalWidth, image.naturalHeight]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, 2, 2])
    );
    engine.close_async().await.unwrap();
    let (
        request,
        stylesheet_request,
        dependency_request,
        font_request,
        image_request,
        media_request,
        script_request,
        dynamic_script_request,
        runtime_font_request,
        font_cookie_followup_request,
    ) = server.await.unwrap();
    assert!(request.starts_with("GET /page HTTP/1.1\r\n"));
    assert!(stylesheet_request.starts_with("GET /initial.css HTTP/1.1\r\n"));
    assert!(stylesheet_request.contains("navigation_visible=present"));
    assert!(stylesheet_request.contains("navigation_secret=hidden"));
    assert!(dependency_request.starts_with("GET /nested.css HTTP/1.1\r\n"));
    assert!(dependency_request.contains("stylesheet_visible=present"));
    assert!(dependency_request.contains("stylesheet_secret=hidden"));
    assert!(font_request.starts_with("GET /font.ttf HTTP/1.1\r\n"));
    assert!(font_request.contains("dependency_visible=present"));
    assert!(font_request.contains("dependency_secret=hidden"));
    assert!(image_request.starts_with("GET /image.png HTTP/1.1\r\n"));
    assert!(image_request.contains("font_visible=present"));
    assert!(image_request.contains("font_secret=hidden"));
    assert!(media_request.starts_with("GET /media.wav HTTP/1.1\r\n"));
    assert!(media_request.contains("image_visible=present"));
    assert!(media_request.contains("image_secret=hidden"));
    assert!(script_request.starts_with("GET /initial.js HTTP/1.1\r\n"));
    assert!(script_request.contains("navigation_visible=present"));
    assert!(script_request.contains("navigation_secret=hidden"));
    assert!(script_request.contains("dependency_visible=present"));
    assert!(script_request.contains("dependency_secret=hidden"));
    assert!(script_request.contains("image_visible=present"));
    assert!(script_request.contains("image_secret=hidden"));
    assert!(script_request.contains("media_visible=present"));
    assert!(script_request.contains("media_secret=hidden"));
    assert!(script_request.contains("font_visible=present"));
    assert!(script_request.contains("font_secret=hidden"));
    assert!(dynamic_script_request.starts_with("GET /initial-dynamic.js HTTP/1.1\r\n"));
    assert!(dynamic_script_request.contains("navigation_secret=hidden"));
    assert!(dynamic_script_request.contains("parser_secret=hidden"));
    assert!(runtime_font_request.starts_with("GET /runtime-font.ttf HTTP/1.1\r\n"));
    assert!(runtime_font_request.contains("initial_dynamic_visible=present"));
    assert!(runtime_font_request.contains("initial_dynamic_secret=hidden"));
    assert!(runtime_font_request.contains("font_secret=hidden"));
    assert!(font_cookie_followup_request.starts_with("GET /font-cookie-followup HTTP/1.1\r\n"));
    assert!(font_cookie_followup_request.contains("runtime_font_visible=present"));
    assert!(font_cookie_followup_request.contains("runtime_font_secret=hidden"));
}
#[tokio::test]
async fn native_content_process_parser_module_graph_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests: Vec<(String, Option<String>)> = Vec::new();
        while requests.len() < 7 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "parser module graph request idle timeout after paths {:?}",
                        requests.iter().map(|(path, _)| path).collect::<Vec<_>>()
                    )
                })
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("parser module graph request includes a URL")
                .to_owned();
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (headers, content_type, body) = match path.as_str() {
                "/page" => (
                    "Set-Cookie: parser_module_page=seed; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/html",
                    "<!doctype html><script type='module' src='/parser-module-entry.mjs'></script><script>globalThis.inlineModuleScript = document.createElement('script'); inlineModuleScript.type = 'module'; inlineModuleScript.src = '/inline-module-entry.mjs'; document.head.appendChild(inlineModuleScript);</script>",
                ),
                "/parser-module-entry.mjs" => (
                    "Set-Cookie: parser_module_entry=entry; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "import { value } from './parser-module-dependency.mjs'; document.documentElement.setAttribute('data-parser-module', value); fetch('/parser-module-data').then(async response => document.documentElement.setAttribute('data-parser-module-data', await response.text()));",
                ),
                "/parser-module-dependency.mjs" => (
                    "Set-Cookie: parser_module_dependency=dependency; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "export const value = 'parser module loaded';",
                ),
                "/parser-module-data" => (
                    "Set-Cookie: parser_module_data=data; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "parser module fetch complete",
                ),
                "/inline-module-entry.mjs" => (
                    "Set-Cookie: inline_module_entry=entry; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "import { value } from './inline-module-dependency.mjs'; document.documentElement.setAttribute('data-inline-module', value);",
                ),
                "/inline-module-dependency.mjs" => (
                    "Set-Cookie: inline_module_dependency=dependency; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/javascript",
                    "export const value = 'inline module loaded';",
                ),
                "/parser-module-after" => ("", "text/plain", "parent cookies retained"),
                other => panic!("unexpected parser module graph request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie));
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut state = serde_json::Value::Null;
    for _ in 0..16 {
        state = engine
            .evaluate_async(
                "({ module: document.documentElement.getAttribute('data-parser-module'), data: document.documentElement.getAttribute('data-parser-module-data'), inlineModule: document.documentElement.getAttribute('data-inline-module') })",
            )
            .await
            .unwrap();
        if state["module"] == "parser module loaded"
            && state["data"] == "parser module fetch complete"
            && state["inlineModule"] == "inline module loaded"
        {
            break;
        }
    }
    assert_eq!(
        state,
        serde_json::json!({
            "module": "parser module loaded",
            "data": "parser module fetch complete",
            "inlineModule": "inline module loaded",
        })
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await fetch('/parser-module-after').then(response => response.text())",
            )
            .await
            .unwrap(),
        serde_json::json!("parent cookies retained")
    );
    let cookies = engine.cookies_async().await.unwrap();
    for (name, value) in [
        ("parser_module_page", "seed"),
        ("parser_module_entry", "entry"),
        ("parser_module_dependency", "dependency"),
        ("parser_module_data", "data"),
        ("inline_module_entry", "entry"),
        ("inline_module_dependency", "dependency"),
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.value == value && cookie.http_only),
            "parent cookie jar is missing HttpOnly {name}"
        );
    }
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();
    let cookie_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .unwrap_or_else(|| panic!("missing parser module graph request {path}"))
            .1
            .as_deref()
            .unwrap_or("")
            .to_owned()
    };
    assert!(cookie_for("/parser-module-entry.mjs").contains("parser_module_page=seed"));
    for name in ["parser_module_page=seed", "parser_module_entry=entry"] {
        assert!(
            cookie_for("/parser-module-dependency.mjs").contains(name),
            "parent did not apply {name} before loading the parser module dependency"
        );
    }
    assert!(cookie_for("/inline-module-entry.mjs").contains("parser_module_page=seed"));
    assert!(cookie_for("/inline-module-dependency.mjs").contains("inline_module_entry=entry"));
    for name in [
        "parser_module_page=seed",
        "parser_module_entry=entry",
        "parser_module_dependency=dependency",
    ] {
        assert!(
            cookie_for("/parser-module-data").contains(name),
            "parent did not select {name} for the parser module Fetch"
        );
    }
    for name in [
        "parser_module_page=seed",
        "parser_module_entry=entry",
        "parser_module_dependency=dependency",
        "parser_module_data=data",
        "inline_module_entry=entry",
        "inline_module_dependency=dependency",
    ] {
        assert!(
            cookie_for("/parser-module-after").contains(name),
            "parent did not select {name} for the following page Fetch"
        );
    }
}
#[tokio::test]
async fn native_content_process_mutation_stylesheets_use_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered mutation test navigation should reach the server")
            .unwrap();
        let navigation_request = read_http_request(&mut stream).await;
        let body = "<!doctype html><button id='load-style'>Load style</button><script>document.getElementById('load-style').addEventListener('click', () => { const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = '/mutation.css'; document.head.appendChild(link); });</script>";
        let navigation_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: mutation_page_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: mutation_page_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: text/html; charset=utf-8\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            body.len(),
            body
        );
        stream
            .write_all(navigation_response.as_bytes())
            .await
            .unwrap();

        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered mutation stylesheet should reach the server")
            .unwrap();
        let stylesheet_request = read_http_request(&mut stream).await;
        let stylesheet = "@import '/mutation-import.css';";
        let stylesheet_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: mutation_style_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: mutation_style_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: text/css\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            stylesheet.len(),
            stylesheet
        );
        stream
            .write_all(stylesheet_response.as_bytes())
            .await
            .unwrap();

        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-brokered mutation stylesheet import should reach the server")
            .unwrap();
        let import_request = read_http_request(&mut stream).await;
        let import = "body { color: black; }";
        let import_response = format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Set-Cookie: mutation_import_visible=present; Path=/; SameSite=Lax\r\n",
                "Set-Cookie: mutation_import_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\n",
                "Content-Type: text/css\r\n",
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}"
            ),
            import.len(),
            import
        );
        stream.write_all(import_response.as_bytes()).await.unwrap();

        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
            .await
            .expect("parent-owned mutation cookies should reach the follow-up request")
            .unwrap();
        let followup_request = read_http_request(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        (
            navigation_request,
            stylesheet_request,
            import_request,
            followup_request,
        )
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .action_async(NativeAction::Click {
            target: "id=load-style".into(),
        })
        .await
        .unwrap();

    let visible_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    let visible_cookies = visible_cookies.as_str().unwrap_or_default();
    assert!(visible_cookies.contains("mutation_page_visible=present"));
    assert!(visible_cookies.contains("mutation_style_visible=present"));
    assert!(visible_cookies.contains("mutation_import_visible=present"));
    assert!(!visible_cookies.contains("mutation_page_secret"));
    assert!(!visible_cookies.contains("mutation_style_secret"));
    assert!(!visible_cookies.contains("mutation_import_secret"));
    assert_eq!(
        engine
            .evaluate_async("await fetch('/mutation-followup').then(response => response.status)")
            .await
            .unwrap(),
        serde_json::json!(204)
    );
    let cookies = engine.cookies_async().await.unwrap();
    for name in [
        "mutation_page_secret",
        "mutation_style_secret",
        "mutation_import_secret",
    ] {
        assert!(
            cookies.iter().any(|cookie| {
                cookie.name == name && cookie.value == "hidden" && cookie.http_only
            })
        );
    }
    engine.close_async().await.unwrap();

    let (navigation_request, stylesheet_request, import_request, followup_request) =
        server.await.unwrap();
    assert!(navigation_request.starts_with("GET /page HTTP/1.1\r\n"));
    assert!(stylesheet_request.starts_with("GET /mutation.css HTTP/1.1\r\n"));
    assert!(stylesheet_request.contains("mutation_page_secret=hidden"));
    assert!(import_request.starts_with("GET /mutation-import.css HTTP/1.1\r\n"));
    assert!(import_request.contains("mutation_style_secret=hidden"));
    assert!(followup_request.starts_with("GET /mutation-followup HTTP/1.1\r\n"));
    for name in [
        "mutation_page_secret=hidden",
        "mutation_style_secret=hidden",
        "mutation_import_secret=hidden",
    ] {
        assert!(
            followup_request.contains(name),
            "{name}: {followup_request}"
        );
    }
}
#[tokio::test]
async fn native_content_process_dynamic_page_module_uses_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..6 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("parent-brokered dynamic module request should reach the local server")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request
                .split_whitespace()
                .nth(1)
                .expect("request includes a path")
                .to_owned();
            let cookie = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (headers, content_type, body) = match path.as_str() {
                "/page" => (
                    concat!(
                        "Set-Cookie: page_session=initial; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: page_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<html><body><p>Native content</p></body></html>",
                ),
                "/parent.mjs" => (
                    "Set-Cookie: parent_script_secret=module-root; HttpOnly; Path=/\r\n",
                    "application/javascript",
                    "import { value } from './dependency.mjs'; document.body.setAttribute('data-parent-module', value); globalThis.runtimeImportPromise = import('./runtime-parent.mjs').then(runtime => { document.body.setAttribute('data-runtime-module', runtime.value); return runtime.value; });",
                ),
                "/dependency.mjs" => (
                    "",
                    "application/javascript",
                    "export const value = 'module dependency loaded';",
                ),
                "/runtime-parent.mjs" => (
                    "Set-Cookie: runtime_script_secret=runtime-root; HttpOnly; Path=/\r\n",
                    "application/javascript",
                    "import { value as dependencyValue } from './runtime-dependency.mjs'; export const value = dependencyValue;",
                ),
                "/runtime-dependency.mjs" => (
                    "",
                    "application/javascript",
                    "export const value = 'runtime import dependency loaded';",
                ),
                "/runtime-followup" => ("", "text/plain", "ok"),
                other => panic!("unexpected parent-brokered dynamic module path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((path, cookie));
        }
        requests
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "document.cookie = 'same_turn=present; Path=/'; const script = document.createElement('script'); script.type = 'module'; script.src = '/parent.mjs'; document.body.appendChild(script); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.body.getAttribute('data-parent-module')")
            .await
            .unwrap(),
        serde_json::json!("module dependency loaded")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await globalThis.runtimeImportPromise.then(value => value, error => String(error))",
            )
            .await
            .unwrap(),
        serde_json::json!("runtime import dependency loaded")
    );
    assert_eq!(
        engine
            .evaluate_async("await fetch('/runtime-followup').then(response => response.status)")
            .await
            .unwrap(),
        serde_json::json!(200)
    );
    let public_cookies = engine.evaluate_async("document.cookie").await.unwrap();
    assert!(
        public_cookies
            .as_str()
            .is_some_and(|cookies| cookies.contains("page_session=initial"))
    );
    assert!(
        !public_cookies
            .as_str()
            .is_some_and(|cookies| cookies.contains("page_secret"))
    );
    assert!(
        !public_cookies
            .as_str()
            .is_some_and(|cookies| cookies.contains("parent_script_secret"))
    );
    assert!(
        !public_cookies
            .as_str()
            .is_some_and(|cookies| cookies.contains("runtime_script_secret"))
    );
    let stored_cookies = engine.cookies_async().await.unwrap();
    assert!(stored_cookies.iter().any(|cookie| {
        cookie.name == "parent_script_secret" && cookie.value == "module-root" && cookie.http_only
    }));
    assert!(stored_cookies.iter().any(|cookie| {
        cookie.name == "runtime_script_secret" && cookie.value == "runtime-root" && cookie.http_only
    }));
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        [
            "/page",
            "/parent.mjs",
            "/dependency.mjs",
            "/runtime-parent.mjs",
            "/runtime-dependency.mjs",
            "/runtime-followup",
        ]
    );
    let root_cookie = requests[1].1.as_deref().unwrap_or_default();
    assert!(root_cookie.contains("page_session=initial"));
    assert!(root_cookie.contains("page_secret=initial-secret"));
    assert!(root_cookie.contains("same_turn=present"));
    let dependency_cookie = requests[2].1.as_deref().unwrap_or_default();
    assert!(dependency_cookie.contains("page_session=initial"));
    assert!(dependency_cookie.contains("page_secret=initial-secret"));
    assert!(dependency_cookie.contains("parent_script_secret=module-root"));
    assert!(dependency_cookie.contains("same_turn=present"));
    let runtime_root_cookie = requests[3].1.as_deref().unwrap_or_default();
    assert!(runtime_root_cookie.contains("page_secret=initial-secret"));
    assert!(runtime_root_cookie.contains("parent_script_secret=module-root"));
    assert!(runtime_root_cookie.contains("same_turn=present"));
    let runtime_dependency_cookie = requests[4].1.as_deref().unwrap_or_default();
    assert!(runtime_dependency_cookie.contains("runtime_script_secret=runtime-root"));
    let followup_cookie = requests[5].1.as_deref().unwrap_or_default();
    assert!(followup_cookie.contains("runtime_script_secret=runtime-root"));
}
#[test]
fn native_content_process_form_post_uses_parent_cookie_authority() {
    std::thread::Builder::new()
        .name("native-form-post-cookie-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("form POST cookie test runtime should build");
            runtime.block_on(native_content_process_form_post_uses_parent_cookie_authority_inner());
        })
        .expect("form POST cookie test thread should start")
        .join()
        .expect("form POST cookie test thread should finish");
}
#[tokio::test]
async fn native_content_process_ignores_csp_path_on_script_redirect() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/start.js", "/other.js"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = match expected_path {
                "/page" => {
                    let body = "<script nonce='bootstrap'>globalThis.redirectReports = []; addEventListener('securitypolicyviolation', event => redirectReports.push([event.effectiveDirective, event.blockedURI]));</script><script src='/start.js'></script>";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: default-src 'none'; script-src 'nonce-bootstrap' http://{address}/start.js\r\nContent-Security-Policy-Report-Only: default-src 'none'; script-src 'nonce-bootstrap' http://{address}/start.js\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/start.js" => "HTTP/1.1 302 Found\r\nLocation: /other.js\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
                "/other.js" => {
                    let body = "globalThis.redirectedScript = true;";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                _ => unreachable!(),
            };
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
                "({ ran: globalThis.redirectedScript === true, reports: redirectReports })"
            )
            .await
            .unwrap(),
        serde_json::json!({"ran": true, "reports": []})
    );
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_delivers_report_only_csp_report_uri_network_reports() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let report_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let report_address = report_listener.local_addr().unwrap();
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = "<body>Report URI</body>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: session=secret; Path=/\r\nContent-Security-Policy: connect-src 'self'\r\nContent-Security-Policy-Report-Only: connect-src 'none'; report-uri http://{report_address}/csp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
                .contains("content-type: application/csp-report")
        );
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
        assert!(request.contains("\"csp-report\""));
        assert!(request.contains(&format!(
            "\"blocked-uri\":\"http://{page_address}/blocked\""
        )));
        assert!(request.contains("\"effective-directive\":\"connect-src\""));
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
async fn native_content_process_uses_redirected_network_stylesheet_url_as_css_base() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let png = native_test_png_bytes();
    let server = tokio::spawn(async move {
        for expected_path in [
            "/page",
            "/styles/entry.css",
            "/styles/theme/main.css",
            "/styles/assets/dot.png",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            match expected_path {
                "/page" => {
                    let body = "<style>body{margin:0px}</style><link rel='stylesheet' href='/styles/entry.css'><div id='surface'></div>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
                "/styles/entry.css" => {
                    stream
                        .write_all(
                            b"HTTP/1.1 302 Found\r\nLocation: /styles/theme/main.css\r\nConnection: close\r\n\r\n",
                        )
                        .await
                        .unwrap();
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
async fn native_content_process_requires_cors_for_cross_origin_integrity_resources() {
    let _guard = native_content_process_test_lock().lock().await;
    let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let page_address = page_listener.local_addr().unwrap();
    let resource_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let resource_address = resource_listener.local_addr().unwrap();
    let style = "#cross-good { display: block; width: 80px; height: 24px; }";
    let good_script = "globalThis.crossGood = 'executed';";
    let blocked_script = "globalThis.crossBlocked = 'executed';";
    let digest = |body: &str| {
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body.as_bytes()))
    };
    let style_hash = digest(style);
    let good_script_hash = digest(good_script);
    let blocked_script_hash = digest(blocked_script);
    let page_server = tokio::spawn(async move {
        let (mut stream, _) = page_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let body = format!(
            "<link rel='stylesheet' href='http://{resource_address}/cross.css' crossorigin='anonymous' integrity='sha256-{style_hash}'><script src='http://{resource_address}/cross.js' crossorigin='anonymous' integrity='sha256-{good_script_hash}'></script><script src='http://{resource_address}/blocked.js' integrity='sha256-{blocked_script_hash}'></script><button id='cross-good'>Cross origin</button>"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let resource_server = tokio::spawn(async move {
        for expected_path in ["/cross.css", "/cross.js", "/blocked.js"] {
            let (mut stream, _) = resource_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let origin = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("origin")
                        .then_some(value.trim().to_owned())
                })
            });
            if expected_path == "/blocked.js" {
                assert_eq!(origin, None);
            } else {
                assert_eq!(origin, Some(format!("http://{page_address}")));
            }
            let (content_type, body) = match expected_path {
                "/cross.css" => ("text/css", style),
                "/cross.js" => ("application/javascript", good_script),
                "/blocked.js" => ("application/javascript", blocked_script),
                _ => unreachable!(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nAccess-Control-Allow-Origin: http://{page_address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{page_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let cross_good = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Cross origin")
        .unwrap();
    assert_eq!(
        engine
            .layout()
            .unwrap()
            .box_for(cross_good.node_id)
            .unwrap()
            .width,
        80
    );
    assert_eq!(
        engine
            .evaluate_async("[globalThis.crossGood, typeof globalThis.crossBlocked]")
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
        1
    );
    engine.close_async().await.unwrap();
    page_server.await.unwrap();
    resource_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_ignores_csp_path_on_frame_redirect() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/frame-start", "/frame-final"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let response = match expected_path {
                "/page" => {
                    let body = "<title>Parent</title><iframe src='/frame-start'>fallback</iframe>";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: frame-src http://{address}/frame-start\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                "/frame-start" => "HTTP/1.1 302 Found\r\nLocation: /frame-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
                "/frame-final" => {
                    let body = "<title>Redirected frame</title><p>Frame content</p>";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
                _ => unreachable!(),
            };
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child = frames
        .iter()
        .find(|frame| frame.id.ends_with(":frame-1"))
        .unwrap();
    assert_eq!(child.url, format!("http://{address}/frame-final"));
    session.close().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_preserves_bounded_cookie_state_between_navigations() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for expected_path in ["/set", "/check"] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(
                request
                    .split_whitespace()
                    .nth(1)
                    .is_some_and(|path| path == expected_path),
                "unexpected request: {request}"
            );
            if expected_path == "/set" {
                assert!(
                    !request.lines().any(|line| {
                        line.split_once(':')
                            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("cookie"))
                    }),
                    "session cookie leaked into its first request: {request}"
                );
            } else {
                assert!(
                    request.lines().any(|line| {
                        line.split_once(':').is_some_and(|(name, value)| {
                            name.eq_ignore_ascii_case("cookie") && value.trim() == "session=alpha"
                        })
                    }),
                    "session cookie was not sent on the next navigation: {request}"
                );
            }
            let (extra_headers, body) = if expected_path == "/set" {
                (
                    "Set-Cookie: session=alpha; Path=/\r\n",
                    "<title>Cookie set</title><p>First navigation</p>",
                )
            } else {
                ("", "<title>Cookie sent</title><p>Second navigation</p>")
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let base_url = format!("http://{address}");
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{base_url}/set")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(engine.snapshot().unwrap().title, "Cookie set");
    engine
        .navigate_async(format!("{base_url}/check"))
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().title, "Cookie sent");
    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_loads_live_svg_images_through_parent_cookie_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let first_png = native_test_solid_png_bytes([255, 0, 0, 255]);
    let second_png = native_test_solid_png_bytes([0, 255, 0, 255]);
    let replacement_png = native_test_solid_png_bytes([0, 0, 255, 255]);
    let local_data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD
            .encode(native_test_solid_png_bytes([255, 255, 0, 255]))
    );
    let server = tokio::spawn(async move {
        for expected_path in ["/page", "/first.png", "/second.png", "/replacement.png"] {
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

            let (png, set_cookie) = match expected_path {
                "/page" => {
                    let body = format!(
                        "<style>body{{margin:0px}}</style><svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='8px' height='8px' viewBox='0 0 8 8'><image id='first' href='/first.png' x='0' y='0' width='4' height='4'/><image id='second' xlink:href='/second.png' x='4' y='0' width='4' height='2'/><image id='local' href='{local_data_url}' x='0' y='4' width='4' height='4' transform='translate(1 0)'/></svg>"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: seed=parent; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    continue;
                }
                "/first.png" => {
                    assert!(has_cookie("seed", "parent"));
                    (
                        &first_png,
                        Some("Set-Cookie: from_first=parent; HttpOnly; Path=/; SameSite=Lax\r\n"),
                    )
                }
                "/second.png" => {
                    assert!(has_cookie("seed", "parent"));
                    assert!(has_cookie("from_first", "parent"));
                    (
                        &second_png,
                        Some("Set-Cookie: from_second=parent; HttpOnly; Path=/; SameSite=Lax\r\n"),
                    )
                }
                "/replacement.png" => {
                    assert!(has_cookie("seed", "parent"));
                    assert!(has_cookie("from_first", "parent"));
                    assert!(has_cookie("from_second", "parent"));
                    (&replacement_png, None)
                }
                _ => unreachable!(),
            };
            let set_cookie = set_cookie.unwrap_or_default();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n",
                png.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(png).await.unwrap();
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
            .filter(|command| matches!(command, NativeDisplayCommand::SvgImage { .. }))
            .count(),
        3
    );
    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(6, 1), Some([0, 255, 0, 255]));
    assert_eq!(surface.pixel(4, 1), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(2, 6), Some([255, 255, 0, 255]));
    assert_eq!(surface.pixel(0, 6), Some([255, 255, 255, 255]));
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    engine
        .evaluate_async(
            "document.getElementById('first').setAttribute('href', '/replacement.png'); true",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .display_list()
            .unwrap()
            .rasterize()
            .unwrap()
            .pixel(1, 1),
        Some([0, 0, 255, 255])
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("")
    );
    let cookies = engine.cookies_async().await.unwrap();
    for expected_name in ["seed", "from_first", "from_second"] {
        assert!(
            cookies
                .iter()
                .any(|cookie| { cookie.name == expected_name && cookie.http_only })
        );
    }

    engine.close_async().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_video_posters_use_parent_cookie_authority_and_paint() {
    let _guard = native_content_process_test_lock().lock().await;
    let owner_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let owner_address = owner_listener.local_addr().unwrap();
    let poster_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let poster_address = poster_listener.local_addr().unwrap();
    assert_ne!(owner_address.port(), poster_address.port());
    let page = "<!doctype html><style>body{margin:0px} video{position:absolute;left:0;top:0;width:80px;height:40px}</style><video id='poster' poster='/initial.png'></video>";
    let owner_server = tokio::spawn(async move {
        let (mut stream, _) = owner_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/page"));
        let response = format!(
            "HTTP/1.1 200 OK\r\nSet-Cookie: owner_visible=visible; Path=/; SameSite=Lax\r\nSet-Cookie: owner_secret=hidden; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        let (mut stream, _) = owner_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/initial.png"));
        assert!(
            request.contains("owner_visible=visible"),
            "missing owner cookie: {request}"
        );
        assert!(
            request.contains("owner_secret=hidden"),
            "missing HttpOnly owner cookie: {request}"
        );
        let image = native_test_solid_png_bytes([220, 20, 20, 255]);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nSet-Cookie: poster_initial=stored; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            image.len()
        )
        .into_bytes();
        response.extend_from_slice(&image);
        stream.write_all(&response).await.unwrap();
    });

    let observed_poster_requests = Arc::new(Mutex::new(Vec::new()));
    let server_observed_poster_requests = Arc::clone(&observed_poster_requests);
    let (poster_server_shutdown_tx, mut poster_server_shutdown_rx) = oneshot::channel();
    let poster_server = tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                _ = &mut poster_server_shutdown_rx => break,
                accepted = poster_listener.accept() => accepted,
            };
            let Ok((mut stream, _)) = accepted else {
                break;
            };
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            server_observed_poster_requests
                .lock()
                .await
                .push(request.clone());
            let cookie_header = request.lines().find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("cookie").then_some(value.trim())
                })
            });
            let has_cookie = |expected: &str| {
                cookie_header.is_some_and(|cookies| {
                    cookies.split(';').any(|cookie| cookie.trim() == expected)
                })
            };
            let response = match path {
                "/first.png" => {
                    assert!(
                        request.lines().any(|line| {
                            line.eq_ignore_ascii_case("Authorization: Basic dXNlcjpwYXNz")
                        }),
                        "poster URL credentials were not applied to the request: {request}"
                    );
                    for cookie in [
                        "owner_visible=visible",
                        "owner_secret=hidden",
                        "poster_initial=stored",
                    ] {
                        assert!(has_cookie(cookie), "missing {cookie} in {request}");
                    }
                    let image = native_test_solid_png_bytes([20, 180, 40, 255]);
                    let mut response = format!(
                        "HTTP/1.1 200 OK\r\nSet-Cookie: poster_secret=accepted; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        image.len()
                    )
                    .into_bytes();
                    response.extend_from_slice(&image);
                    response
                }
                "/second.png" => {
                    for cookie in [
                        "owner_visible=visible",
                        "owner_secret=hidden",
                        "poster_initial=stored",
                        "poster_secret=accepted",
                    ] {
                        assert!(has_cookie(cookie), "missing {cookie} in {request}");
                    }
                    let image = native_test_solid_png_bytes([20, 40, 220, 255]);
                    let mut response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        image.len()
                    )
                    .into_bytes();
                    response.extend_from_slice(&image);
                    response
                }
                "/missing.png" => {
                    for cookie in [
                        "owner_secret=hidden",
                        "poster_initial=stored",
                        "poster_secret=accepted",
                    ] {
                        assert!(has_cookie(cookie), "missing {cookie} in {request}");
                    }
                    b"HTTP/1.1 404 Not Found\r\nSet-Cookie: failed_poster=stored; HttpOnly; Path=/; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                }
                "/undecodable.png" => {
                    assert!(
                        has_cookie("failed_poster=stored"),
                        "missing failed_poster=stored in {request}"
                    );
                    b"HTTP/1.1 200 OK\r\nSet-Cookie: undecodable_poster=stored; HttpOnly; Path=/; SameSite=Lax\r\nContent-Type: image/png\r\nContent-Length: 12\r\nConnection: close\r\n\r\nnot an image".to_vec()
                }
                "/redirect-exit.png" => {
                    for cookie in ["failed_poster=stored", "undecodable_poster=stored"] {
                        assert!(has_cookie(cookie), "missing {cookie} in {request}");
                    }
                    b"HTTP/1.1 302 Found\r\nSet-Cookie: redirected_poster=stored; HttpOnly; Path=/; SameSite=Lax\r\nLocation: file:///blocked.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                }
                "/after-failure.png" => {
                    for cookie in [
                        "owner_secret=hidden",
                        "poster_initial=stored",
                        "poster_secret=accepted",
                        "failed_poster=stored",
                        "undecodable_poster=stored",
                        "redirected_poster=stored",
                    ] {
                        assert!(has_cookie(cookie), "missing {cookie} in {request}");
                    }
                    let image = native_test_solid_png_bytes([200, 40, 180, 255]);
                    let mut response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        image.len()
                    )
                    .into_bytes();
                    response.extend_from_slice(&image);
                    response
                }
                other => panic!("unexpected video poster request {other}: {request}"),
            };
            stream.write_all(&response).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_viewport(Viewport {
                width: 100,
                height: 60,
                device_scale_factor_milli: 1000,
            })
            .with_initial_url(format!("http://{owner_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    owner_server.await.unwrap();

    let initial_poster = engine
        .evaluate_async("document.getElementById('poster').poster")
        .await
        .unwrap();
    assert_eq!(
        initial_poster,
        serde_json::json!(format!("http://{owner_address}/initial.png"))
    );
    let initial_images = engine
        .display_list()
        .unwrap()
        .commands
        .into_iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::Image {
                rect,
                source_width,
                source_height,
                pixels,
                ..
            } => Some((rect, source_width, source_height, pixels)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(initial_images.len(), 1);
    assert_eq!(
        initial_images[0].0,
        NativeRect {
            x: 20,
            y: 0,
            width: 40,
            height: 40,
        }
    );
    assert_eq!((initial_images[0].1, initial_images[0].2), (2, 2));
    assert!(
        initial_images[0]
            .3
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [220, 20, 20, 255])
    );

    let setup_state = engine
        .evaluate_async(
            "(() => { const video = document.getElementById('poster'); globalThis.posterEvents = []; video.addEventListener('load', () => posterEvents.push('load')); video.addEventListener('error', () => posterEvents.push('error')); return [video.currentSrc, video.networkState, video.readyState, video.error && video.error.code]; })()",
        )
        .await
        .unwrap();
    assert_eq!(setup_state, serde_json::json!(["", 0, 0, null]));
    engine
        .evaluate_async(
            "(() => { const video = document.getElementById('poster'); video.poster = video.poster; return video.poster; })()",
        )
        .await
        .unwrap();

    let first_poster_url = format!("http://user:pass@{poster_address}/first.png");
    assert_eq!(
        engine
            .evaluate_async(&format!(
                "(() => {{ const video = document.getElementById('poster'); video.poster = {first_poster_url:?}; return video.poster; }})()"
            ))
            .await
            .unwrap(),
        serde_json::json!(first_poster_url)
    );
    let first_poster_pixels = engine
        .display_list()
        .unwrap()
        .commands
        .into_iter()
        .find_map(|command| match command {
            NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
            _ => None,
        })
        .unwrap();
    assert!(
        first_poster_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [20, 180, 40, 255])
    );

    let second_poster_url = format!("http://{poster_address}/second.png");
    engine
        .evaluate_async(&format!(
            "document.getElementById('poster').poster = {second_poster_url:?}; true"
        ))
        .await
        .unwrap();
    let second_poster_pixels = engine
        .display_list()
        .unwrap()
        .commands
        .into_iter()
        .find_map(|command| match command {
            NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
            _ => None,
        })
        .unwrap();
    assert!(
        second_poster_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [20, 40, 220, 255])
    );

    engine
        .evaluate_async("document.getElementById('poster').removeAttribute('poster'); true")
        .await
        .unwrap();
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| matches!(command, NativeDisplayCommand::Image { .. })),
        "clearing poster should remove its painted image"
    );

    let missing_poster_url = format!("http://{poster_address}/missing.png");
    engine
        .evaluate_async(&format!(
            "document.getElementById('poster').poster = {missing_poster_url:?}; true"
        ))
        .await
        .unwrap();
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| matches!(command, NativeDisplayCommand::Image { .. })),
        "failed poster must not retain or paint the previous image"
    );
    let undecodable_poster_url = format!("http://{poster_address}/undecodable.png");
    engine
        .evaluate_async(&format!(
            "document.getElementById('poster').poster = {undecodable_poster_url:?}; true"
        ))
        .await
        .unwrap();
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| matches!(command, NativeDisplayCommand::Image { .. })),
        "undecodable poster must not be painted"
    );
    let redirect_exit_url = format!("http://{poster_address}/redirect-exit.png");
    engine
        .evaluate_async(&format!(
            "document.getElementById('poster').poster = {redirect_exit_url:?}; true"
        ))
        .await
        .unwrap();
    assert!(
        !engine
            .display_list()
            .unwrap()
            .commands
            .iter()
            .any(|command| matches!(command, NativeDisplayCommand::Image { .. })),
        "poster redirect to an unsupported scheme must fail without painting"
    );
    let after_failure_url = format!("http://{poster_address}/after-failure.png");
    engine
        .evaluate_async(&format!(
            "document.getElementById('poster').poster = {after_failure_url:?}; true"
        ))
        .await
        .unwrap();
    let after_failure_pixels = engine
        .display_list()
        .unwrap()
        .commands
        .into_iter()
        .find_map(|command| match command {
            NativeDisplayCommand::Image { pixels, .. } => Some(pixels),
            _ => None,
        })
        .unwrap();
    assert!(
        after_failure_pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [200, 40, 180, 255])
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const video = document.getElementById('poster'); return [posterEvents, video.currentSrc, video.networkState, video.readyState, video.error && video.error.code, document.cookie]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([[], "", 0, 0, null, "owner_visible=visible"])
    );
    let cookies = engine.cookies_async().await.unwrap();
    for name in [
        "owner_secret",
        "poster_initial",
        "poster_secret",
        "failed_poster",
        "undecodable_poster",
        "redirected_poster",
    ] {
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == name && cookie.http_only),
            "parent cookie jar did not persist HttpOnly cookie {name}"
        );
    }

    let requests = observed_poster_requests.lock().await.clone();
    let paths = requests
        .iter()
        .map(|request| request.split_whitespace().nth(1).unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "/first.png",
            "/second.png",
            "/missing.png",
            "/undecodable.png",
            "/redirect-exit.png",
            "/after-failure.png"
        ]
    );
    engine.close_async().await.unwrap();
    poster_server_shutdown_tx.send(()).unwrap();
    poster_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_video_poster_url_credentials_stay_stripped_after_cross_origin_redirect()
 {
    let _guard = native_content_process_test_lock().lock().await;
    let origin_a_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_a = origin_a_listener.local_addr().unwrap();
    let origin_b_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_b = origin_b_listener.local_addr().unwrap();
    assert_ne!(origin_a.port(), origin_b.port());
    let origin_a_server = tokio::spawn(async move {
        let (mut stream, _) = origin_a_listener.accept().await.unwrap();
        let first_request = read_http_request(&mut stream).await;
        assert_eq!(first_request.split_whitespace().nth(1), Some("/start.png"));
        assert!(
            first_request
                .lines()
                .any(|line| { line.eq_ignore_ascii_case("Authorization: Basic dXNlcjpwYXNz") }),
            "the initial URL credentials should authorize origin A: {first_request}"
        );
        assert!(!first_request.contains("user:pass@"));
        let redirect = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{origin_b}/bridge\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(redirect.as_bytes()).await.unwrap();

        let (mut stream, _) = origin_a_listener.accept().await.unwrap();
        let final_request = read_http_request(&mut stream).await;
        assert_eq!(final_request.split_whitespace().nth(1), Some("/final.png"));
        assert!(
            !final_request.lines().any(|line| {
                line.split_once(':')
                    .is_some_and(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            }),
            "Authorization must stay stripped after the chain leaves origin A: {final_request}"
        );
        assert!(!final_request.contains("user:pass@"));
        let image = native_test_solid_png_bytes([40, 130, 200, 255]);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            image.len()
        )
        .into_bytes();
        response.extend_from_slice(&image);
        stream.write_all(&response).await.unwrap();
    });
    let origin_b_server = tokio::spawn(async move {
        let (mut stream, _) = origin_b_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/bridge"));
        assert!(
            !request.lines().any(|line| {
                line.split_once(':')
                    .is_some_and(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            }),
            "URL credentials must not be sent to origin B: {request}"
        );
        assert!(!request.contains("user:pass@"));
        let redirect = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{origin_a}/final.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(redirect.as_bytes()).await.unwrap();
    });

    let owner_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let owner_address = owner_listener.local_addr().unwrap();
    let page =
        format!("<!doctype html><video poster='http://user:pass@{origin_a}/start.png'></video>");
    let owner_server = tokio::spawn(async move {
        let (mut stream, _) = owner_listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
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
    origin_a_server.await.unwrap();
    origin_b_server.await.unwrap();
    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_content_process_reuses_successful_cors_preflight_cache() {
    let _guard = native_content_process_test_lock().lock().await;
    let document_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document_address = document_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let document_origin = format!("http://{document_address}");
    let expected_origin = document_origin.clone();
    let response_origin = document_origin.clone();

    let document_server = tokio::spawn(async move {
        let (mut stream, _) = document_listener.accept().await.unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<title>Fetch owner</title><p>Native fetch</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let api_server = tokio::spawn(async move {
        let (mut stream, _) = api_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("OPTIONS"));
        assert_eq!(request.split_whitespace().nth(1), Some("/submit"));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("origin") && value.trim() == expected_origin
            })
        }));
        let response = format!(
            "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type, x-glass-token\r\nAccess-Control-Max-Age: 600\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).await.unwrap();

        for value in ["first", "second"] {
            let (mut stream, _) = api_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().next(), Some("POST"));
            assert_eq!(request.split_whitespace().nth(1), Some("/submit"));
            assert!(request.lines().any(|line| {
                line.split_once(':').is_some_and(|(name, header_value)| {
                    name.eq_ignore_ascii_case("origin") && header_value.trim() == expected_origin
                })
            }));
            let body = format!("{{\"value\":\"{value}\"}}");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: {response_origin}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{document_origin}/index.html")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(&format!(
            "(() => {{ const options = {{ method: 'POST', credentials: 'omit', body: '{{\"name\":\"glass\"}}', headers: {{ 'Content-Type': 'application/json', 'X-Glass-Token': 'alpha' }} }}; fetch('http://{api_address}/submit', options).then(response => response.json()).then(first => fetch('http://{api_address}/submit', options).then(response => response.json()).then(second => {{ globalThis.corsPreflightCacheValues = [first.value, second.value]; }})); }})()"
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.corsPreflightCacheValues")
            .await
            .unwrap(),
        serde_json::json!(["first", "second"])
    );
    engine.close_async().await.unwrap();
    document_server.await.unwrap();
    api_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_exposes_opaque_no_cors_response() {
    let _guard = native_content_process_test_lock().lock().await;
    let document_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let document_address = document_listener.local_addr().unwrap();
    let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_address = api_listener.local_addr().unwrap();
    let document_origin = format!("http://{document_address}");

    let document_server = tokio::spawn(async move {
        let (mut stream, _) = document_listener.accept().await.unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = "<title>Fetch owner</title><p>Native fetch</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: connect-src *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let api_server = tokio::spawn(async move {
        let (mut stream, _) = api_listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().next(), Some("GET"));
        assert_eq!(request.split_whitespace().nth(1), Some("/opaque"));
        assert!(!request.lines().any(|line| {
            line.split_once(':')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case("origin"))
        }));
        let body = "secret cross-origin body";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-Private: hidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("{document_origin}/index.html")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(&format!(
            "fetch('http://{api_address}/opaque', {{ mode: 'same-origin' }}).then(() => {{ globalThis.sameOriginModeValue = ['fulfilled', false]; }}, error => {{ globalThis.sameOriginModeValue = [error.name, error.message.includes('different origin')]; }});"
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.sameOriginModeValue")
            .await
            .unwrap(),
        serde_json::json!(["TypeError", true])
    );
    engine
        .evaluate_async(&format!(
            "fetch('http://{api_address}/opaque', {{ mode: 'no-cors', headers: {{ 'X-Not-Safelisted': 'blocked' }} }}).then(() => {{ globalThis.noCorsHeaderValue = ['fulfilled', false]; }}, error => {{ globalThis.noCorsHeaderValue = [error.name, error.message.includes('non-safelisted')]; }});"
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.noCorsHeaderValue")
            .await
            .unwrap(),
        serde_json::json!(["TypeError", true])
    );
    engine
        .evaluate_async(&format!(
            "fetch('http://{api_address}/opaque', {{ mode: 'no-cors', credentials: 'omit' }}).then(async response => {{ const bodyRead = await response.text().then(() => 'readable', error => error.name); globalThis.opaqueFetchValue = [response.type, response.status, response.ok, response.url, Array.from(response.headers.entries()), bodyRead, response.body === null]; }});"
        ))
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.opaqueFetchValue")
            .await
            .unwrap(),
        serde_json::json!(["opaque", 0, false, "", [], "TypeError", true])
    );
    engine.close_async().await.unwrap();
    document_server.await.unwrap();
    api_server.await.unwrap();
}
#[tokio::test]
async fn native_content_process_synchronizes_document_cookie_with_http_session() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut page_stream, _) = listener.accept().await.unwrap();
        let page_request = read_http_request(&mut page_stream).await;
        assert_eq!(page_request.split_whitespace().nth(1), Some("/page"));
        let page_body = "<p>Cookie owner</p>";
        let page_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: theme=dark; Path=/\r\nSet-Cookie: secret=hidden; HttpOnly; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page_body}",
            page_body.len()
        );
        page_stream
            .write_all(page_response.as_bytes())
            .await
            .unwrap();

        let (mut echo_stream, _) = listener.accept().await.unwrap();
        let echo_request = read_http_request(&mut echo_stream).await;
        assert_eq!(echo_request.split_whitespace().nth(1), Some("/echo"));
        let cookie_header = echo_request.lines().find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("cookie").then_some(value.trim())
            })
        });
        assert_eq!(
            cookie_header,
            Some("secret=hidden; session=one; theme=dark")
        );
        let echo_body = "cookie-ok";
        let echo_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{echo_body}",
            echo_body.len()
        );
        echo_stream
            .write_all(echo_response.as_bytes())
            .await
            .unwrap();
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("theme=dark")
    );
    assert_eq!(
        engine
            .evaluate_async("document.cookie = 'session=one; Path=/'; document.cookie")
            .await
            .unwrap(),
        serde_json::json!("theme=dark; session=one")
    );
    engine
        .evaluate_async("document.cookie = 'page-secret=hidden; HttpOnly; Path=/'; true")
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("session=one; theme=dark")
    );
    engine
        .evaluate_async(
            "fetch('/echo').then(response => response.text()).then(value => { globalThis.cookieFetch = value; });",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.cookieFetch")
            .await
            .unwrap(),
        serde_json::json!("cookie-ok")
    );
    let cookies = engine.cookies_async().await.unwrap();
    assert!(cookies.iter().any(|cookie| {
        cookie.name == "secret" && cookie.http_only && cookie.domain == address.ip().to_string()
    }));
    engine
        .set_cookies_async(&[Cookie {
            name: "imported".into(),
            value: "yes".into(),
            domain: address.ip().to_string(),
            path: "/".into(),
            expires: 0.0,
            http_only: true,
            secure: false,
            same_site: None,
            is_session: true,
            size: None,
            priority: None,
        }])
        .await
        .unwrap();
    assert!(
        engine
            .cookies_async()
            .await
            .unwrap()
            .iter()
            .any(|cookie| cookie.name == "imported" && cookie.http_only)
    );
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        serde_json::json!("session=one; theme=dark")
    );
    engine.clear_cookies_async().await.unwrap();
    assert!(engine.cookies_async().await.unwrap().is_empty());
    engine.close_async().await.unwrap();
    server.await.unwrap();
}

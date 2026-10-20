use super::*;

#[tokio::test]
async fn native_content_process_page_fetch_credentials_survive_service_worker_handoff() {
    let _guard = native_content_process_test_lock().lock().await;
    let primary_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let primary_address = primary_listener.local_addr().unwrap();
    let cross_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cross_address = cross_listener.local_addr().unwrap();
    let page_origin = format!("http://{primary_address}");
    let cross_origin = format!("http://{cross_address}");
    let worker_cross_origin = cross_origin.clone();

    let primary_server = tokio::spawn(async move {
        for (expected_path, content_type, headers, body) in [
            (
                "/page",
                "text/html",
                "Set-Cookie: root=seed; Path=/; SameSite=Lax\r\n",
                "<!doctype html><script>globalThis.registrationPromise = null;</script>",
            ),
            (
                "/sw.js",
                "application/javascript",
                "",
                "service worker body is generated below",
            ),
        ] {
            let (mut stream, _) = primary_listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
            let body = if expected_path == "/sw.js" {
                format!(
                    r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {{
  const requestUrl = new URL(event.request.url);
  if (requestUrl.pathname === '/controlled') {{
    event.respondWith(new Response('<!doctype html><main>controlled</main>', {{
      headers: {{ 'Content-Type': 'text/html' }}
    }}));
  }} else if (requestUrl.pathname === '/probe') {{
    const credentialsMode = event.request.credentials;
    const caseName = requestUrl.searchParams.get('case') || '';
    const upstreamUrl = '{worker_cross_origin}/probe?case=' + encodeURIComponent(caseName);
    event.respondWith(fetch(upstreamUrl, {{ credentials: credentialsMode }}).then(response => response.text()).then(body =>
      new Response(credentialsMode + ':' + body, {{
        headers: {{ 'Content-Type': 'text/plain' }}
      }})));
  }}
}});"#
                )
            } else {
                body.to_owned()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let cross_server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async move {
            let mut requests = Vec::new();
            for _ in 0..7 {
                let (mut stream, _) = cross_listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let target = request
                    .split_whitespace()
                    .nth(1)
                    .expect("page Fetch includes a request target")
                    .to_owned();
                let (path, query) = target
                    .split_once('?')
                    .expect("credential request carries a query");
                assert!(path == "/probe" || path == "/direct");
                let case = query
                    .strip_prefix("case=")
                    .expect("credential request carries a case")
                    .to_owned();
                let cookie = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim().to_owned());
                let cors_headers = if case.contains("include") {
                    format!(
                        "Access-Control-Allow-Origin: {page_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                    )
                } else {
                    "Access-Control-Allow-Origin: *\r\n".to_owned()
                };
                let cookie_header = match case.as_str() {
                    "direct-same-origin" => {
                        "Set-Cookie: denied-direct-same=1; Path=/; SameSite=Lax\r\n"
                    }
                    "direct-omit" => {
                        "Set-Cookie: denied-direct-omit=1; Path=/; SameSite=Lax\r\n"
                    }
                    "direct-include" => {
                        "Set-Cookie: accepted-page=1; Path=/; SameSite=Lax\r\n"
                    }
                    "sw-same-origin" => {
                        "Set-Cookie: denied-worker-same=1; Path=/; SameSite=Lax\r\n"
                    }
                    "sw-omit" => {
                        "Set-Cookie: denied-worker-omit=1; Path=/; SameSite=Lax\r\n"
                    }
                    "sw-include" => {
                        "Set-Cookie: accepted-worker=1; Path=/; SameSite=Lax\r\n"
                    }
                    "sw-include-after" => "",
                    other => panic!("unexpected credentials case: {other}"),
                };
                let body = cookie.as_deref().unwrap_or("none");
                let response = format!(
                    "HTTP/1.1 200 OK\r\n{cors_headers}{cookie_header}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((case, cookie));
            }
            requests
        })
        .await
        .expect("Service Worker Fetch credential cases complete within the time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{primary_address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let direct_same_origin = engine
        .evaluate_async(&format!(
            "await fetch('{cross_origin}/direct?case=direct-same-origin', {{ credentials: 'same-origin' }}).then(response => response.text())"
        ))
        .await
        .unwrap();
    let direct_omit = engine
        .evaluate_async(&format!(
            "await fetch('{cross_origin}/direct?case=direct-omit', {{ credentials: 'omit' }}).then(response => response.text())"
        ))
        .await
        .unwrap();
    let direct_include = engine
        .evaluate_async(&format!(
            "await fetch('{cross_origin}/direct?case=direct-include', {{ credentials: 'include' }}).then(response => response.text())"
        ))
        .await
        .unwrap();
    engine
        .evaluate_async(
            "globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' }); null",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async registration => [registration.active.state, navigator.serviceWorker.controller !== null])",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );
    engine
        .navigate_async(format!("http://{primary_address}/controlled"))
        .await
        .unwrap();
    let worker_same_origin = engine
        .evaluate_async(
            "await fetch('/probe?case=sw-same-origin', { credentials: 'same-origin' }).then(response => response.text())",
        )
        .await
        .unwrap();
    let worker_omit = engine
        .evaluate_async(
            "await fetch('/probe?case=sw-omit', { credentials: 'omit' }).then(response => response.text())",
        )
        .await
        .unwrap();
    let worker_include = engine
        .evaluate_async(
            "await fetch('/probe?case=sw-include', { credentials: 'include' }).then(response => response.text())",
        )
        .await
        .unwrap();
    let worker_include_after = engine
        .evaluate_async(
            "await fetch('/probe?case=sw-include-after', { credentials: 'include' }).then(response => response.text())",
        )
        .await
        .unwrap();
    let request_credentials = engine
        .evaluate_async(&format!("new Request('{cross_origin}/probe').credentials"))
        .await
        .unwrap();
    let invalid_request = engine
        .evaluate_async(&format!(
            "(() => {{ try {{ new Request('{cross_origin}/probe', {{ credentials: 'invalid' }}); return false; }} catch (error) {{ return error instanceof TypeError; }} }})()"
        ))
        .await
        .unwrap();
    let invalid_fetch = engine
        .evaluate_async(&format!(
            "await fetch('{cross_origin}/probe?case=invalid', {{ credentials: 'invalid' }}).then(() => false, error => error instanceof TypeError)"
        ))
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    primary_server.await.unwrap();
    let cross_requests = cross_server.await.unwrap();

    assert_eq!(direct_same_origin, "none");
    assert_eq!(direct_omit, "none");
    assert!(direct_include.as_str().unwrap().contains("root=seed"));
    assert_eq!(worker_same_origin, "same-origin:none");
    assert_eq!(worker_omit, "omit:none");
    assert!(worker_include.as_str().unwrap().starts_with("include:"));
    assert!(
        worker_include_after
            .as_str()
            .unwrap()
            .starts_with("include:")
    );
    assert_eq!(request_credentials, "same-origin");
    assert_eq!(invalid_request, true);
    assert_eq!(invalid_fetch, true);

    let cookie_for = |case: &str| {
        cross_requests
            .iter()
            .find(|(request_case, _)| request_case == case)
            .unwrap_or_else(|| panic!("missing Service Worker Fetch request {case}"))
            .1
            .as_deref()
            .unwrap_or("")
            .to_owned()
    };
    assert!(cookie_for("direct-same-origin").is_empty());
    assert!(cookie_for("direct-omit").is_empty());
    assert!(cookie_for("direct-include").contains("root=seed"));
    assert!(cookie_for("sw-same-origin").is_empty());
    assert!(cookie_for("sw-omit").is_empty());
    assert!(cookie_for("sw-include").contains("root=seed"));
    let worker_include_cookie = cookie_for("sw-include");
    assert!(worker_include_cookie.contains("accepted-page=1"));
    assert!(!worker_include_cookie.contains("denied-direct-same="));
    assert!(!worker_include_cookie.contains("denied-direct-omit="));
    let include_after_cookie = cookie_for("sw-include-after");
    assert!(include_after_cookie.contains("root=seed"));
    assert!(include_after_cookie.contains("accepted-page=1"));
    assert!(include_after_cookie.contains("accepted-worker=1"));
    assert!(!include_after_cookie.contains("denied-worker-same="));
    assert!(!include_after_cookie.contains("denied-worker-omit="));
}
#[tokio::test]
async fn native_content_process_fetch_referrer_policy_survives_service_worker_handoff() {
    let _guard = native_content_process_test_lock().lock().await;
    let primary_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let primary_address = primary_listener.local_addr().unwrap();
    let cross_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cross_address = cross_listener.local_addr().unwrap();
    let page_origin = format!("http://{primary_address}");
    let cross_origin = format!("http://{cross_address}");
    let page_server_origin = page_origin.clone();
    let (stop_page_server, mut stop_page_server_rx) = tokio::sync::oneshot::channel();
    let page_server = tokio::spawn(async move {
        let mut seen = Vec::new();
        loop {
            let (mut stream, _) = tokio::select! {
                _ = &mut stop_page_server_rx => break,
                accepted = primary_listener.accept() => accepted.unwrap(),
            };
            let request = read_http_request(&mut stream).await;
            let target = request.split_whitespace().nth(1).unwrap_or_default();
            let path = target.split('?').next().unwrap_or_default();
            let referer = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let (content_type, body) = match path {
                    "/page" => (
                        "text/html",
                        "<!doctype html><main>referrer policy</main>".to_owned(),
                    ),
                    "/sw.js" => (
                        "application/javascript",
                        "self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  const path = new URL(event.request.url).pathname;
  if (path === '/controlled') {
    event.respondWith(new Response('<!doctype html><main>controlled</main>', {
      headers: { 'Content-Type': 'text/html' }
    }));
  } else if (path === '/sw-referrer') {
    event.respondWith(fetch(event.request));
  } else if (path === '/sw-inspect') {
    const reset = new Request(event.request, { referrerPolicy: 'same-origin' });
    event.respondWith(new Response(JSON.stringify([
      event.request.referrer,
      event.request.referrerPolicy,
      reset.referrer,
      reset.referrerPolicy,
    ]), {
      headers: { 'Content-Type': 'application/json' }
    }));
  }
 });"
                        .to_owned(),
                    ),
                    _ => (
                        "text/plain",
                        referer.clone().unwrap_or_else(|| "none".into()),
                    ),
                };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            seen.push((path.to_owned(), target.to_owned(), referer));
        }
        seen
    });
    let (stop_cross_server, mut stop_cross_server_rx) = tokio::sync::oneshot::channel();
    let cross_server = tokio::spawn(async move {
        let mut seen = Vec::new();
        loop {
            let (mut stream, _) = tokio::select! {
                _ = &mut stop_cross_server_rx => break,
                accepted = cross_listener.accept() => accepted.unwrap(),
            };
            let request = read_http_request(&mut stream).await;
            let target = request
                .split_whitespace()
                .nth(1)
                .expect("Fetch request includes a target")
                .to_owned();
            let path = target.split('?').next().unwrap_or_default();
            let referer = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_owned());
            let case = target
                .split_once("case=")
                .map(|(_, value)| value)
                .unwrap_or_default();
            let cors = format!("Access-Control-Allow-Origin: {page_server_origin}\r\n");
            let response = match path {
                "/redirect-original" => format!(
                    "HTTP/1.1 302 Found\r\n{cors}Location: {page_server_origin}/redirect-original-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                "/redirect-policy" => format!(
                    "HTTP/1.1 302 Found\r\n{cors}Referrer-Policy: origin\r\nLocation: {page_server_origin}/redirect-policy-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                _ => {
                    let body = case.to_owned();
                    format!(
                        "HTTP/1.1 200 OK\r\n{cors}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
            };
            stream.write_all(response.as_bytes()).await.unwrap();
            seen.push((target, referer));
        }
        seen
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_initial_url(format!("http://{primary_address}/page?private=1")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(&format!(
            r#"(() => {{
              globalThis.__nativeReferrerCustomRequest = new Request("{cross_origin}/record?case=custom", {{
                referrer: "{page_origin}/custom?key=1#fragment",
                referrerPolicy: "unsafe-url",
              }});
              globalThis.__nativeReferrerPolicyResetRequest = new Request("{cross_origin}/record?case=custom-policy-reset", {{
                referrer: "{page_origin}/custom?key=1#fragment",
                referrerPolicy: "unsafe-url",
              }});
              globalThis.__nativeReferrerResetRequest = new Request("{cross_origin}/record?case=cross-referrer-reset", {{
                referrer: "{cross_origin}/not-the-client",
              }});
              return null;
            }})()"#
        ))
        .await
        .unwrap();
    let mut direct_cases = Vec::new();
    for (case, request) in [
        (
            "default-cross",
            format!(
                "await fetch('{cross_origin}/record?case=default-cross').then(response => response.text())"
            ),
        ),
        (
            "custom",
            "await fetch(new Request(__nativeReferrerCustomRequest)).then(response => response.text())"
                .to_owned(),
        ),
        (
            "custom-policy-reset",
            "await fetch(__nativeReferrerPolicyResetRequest, { referrerPolicy: 'same-origin' }).then(response => response.text())"
                .to_owned(),
        ),
        (
            "cross-referrer-reset",
            "await fetch(__nativeReferrerResetRequest).then(response => response.text())".to_owned(),
        ),
    ] {
        direct_cases.push((case, engine.evaluate_async(&request).await.unwrap()));
    }
    let original_redirect = engine
        .evaluate_async(&format!(
            "await fetch('{cross_origin}/redirect-original?case=redirect-original').then(response => response.text())"
        ))
        .await
        .unwrap();
    let policy_redirect = engine
        .evaluate_async(&format!(
            "await fetch('{cross_origin}/redirect-policy?case=redirect-policy').then(response => response.text())"
        ))
        .await
        .unwrap();
    let same_origin = engine
        .evaluate_async("await fetch('/same?case=same').then(response => response.text())")
        .await
        .unwrap();
    let direct_results = engine
        .evaluate_async(
            r#"await (() => {
              const customRequest = __nativeReferrerCustomRequest;
              const customClone = new Request(customRequest);
              const resetRequest = __nativeReferrerResetRequest;
              const defaultRequest = new Request('/same');
              const clonedRequest = new Request(defaultRequest);
              const policyOverride = new Request(defaultRequest, { referrerPolicy: 'origin' });
              const customInitReset = new Request(customRequest, { method: 'GET' });
              const customPolicyOverride = new Request(customRequest, { referrerPolicy: 'origin' });
              const invalidRequest = (() => {
                try { new Request('/same', { referrerPolicy: 'invalid' }); return false; }
                catch (error) { return error instanceof TypeError; }
              })();
              return fetch('/same?case=invalid', { referrerPolicy: 'invalid' })
                .then(() => false, error => error instanceof TypeError)
                .then(invalidFetch => ({
                  referrer: defaultRequest.referrer,
                  referrerPolicy: defaultRequest.referrerPolicy,
                  cloneReferrer: clonedRequest.referrer,
                  overridePolicy: policyOverride.referrerPolicy,
                  customReferrer: customRequest.referrer,
                  customCloneReferrer: customClone.referrer,
                  customPolicy: customClone.referrerPolicy,
                  customInitReferrer: customInitReset.referrer,
                  customInitPolicy: customInitReset.referrerPolicy,
                  customOverrideReferrer: customPolicyOverride.referrer,
                  customOverridePolicy: customPolicyOverride.referrerPolicy,
                  resetReferrer: resetRequest.referrer,
                  invalidRequest,
                  invalidFetch,
                }));
            })()"#,
        )
        .await
        .unwrap();

    engine
        .evaluate_async(
            "globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' }); null",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(async registration => [registration.active.state, navigator.serviceWorker.controller !== null])",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );
    engine
        .navigate_async(format!("http://{primary_address}/controlled"))
        .await
        .unwrap();
    let sw_referrers = engine
        .evaluate_async(
            "await (async () => [
              await fetch('/sw-referrer?case=sw-none', { referrerPolicy: 'no-referrer' }).then(response => response.text()),
              await fetch('/sw-referrer?case=sw-unsafe', { referrerPolicy: 'unsafe-url' }).then(response => response.text()),
            ])()",
        )
        .await
        .unwrap();
    let sw_request_properties = engine
        .evaluate_async(
            "await fetch('/sw-inspect', { referrerPolicy: 'unsafe-url' }).then(response => response.json())",
        )
        .await
        .unwrap();
    engine.close_async().await.unwrap();
    let _ = stop_page_server.send(());
    let _ = stop_cross_server.send(());
    let primary_requests = page_server.await.unwrap();
    let cross_requests = cross_server.await.unwrap();

    let direct_results = direct_results.as_object().unwrap();
    let page_referrer = format!("{page_origin}/page?private=1");
    let controlled_referrer = format!("{page_origin}/controlled");
    let origin_referrer = format!("{page_origin}/");
    let custom_referrer = format!("{page_origin}/custom?key=1");
    let direct_referrer = |case: &str| {
        cross_requests
            .iter()
            .find(|(target, _)| {
                target
                    .split_once("case=")
                    .is_some_and(|(_, request_case)| request_case == case)
            })
            .unwrap_or_else(|| panic!("missing referrer request {case}"))
            .1
            .as_deref()
            .unwrap_or("")
            .to_owned()
    };
    for (case, expected) in [
        ("default-cross", origin_referrer.as_str()),
        ("custom", custom_referrer.as_str()),
        ("custom-policy-reset", ""),
        ("cross-referrer-reset", origin_referrer.as_str()),
    ] {
        assert_eq!(
            direct_referrer(case),
            expected,
            "referrer policy case {case}"
        );
    }
    assert_eq!(direct_cases.len(), 4);
    for (case, result) in &direct_cases {
        assert_eq!(result.as_str(), Some(*case), "Fetch response for {case}");
    }
    assert_eq!(original_redirect, page_referrer);
    assert_eq!(policy_redirect, origin_referrer);
    assert_eq!(same_origin, page_referrer);
    assert_eq!(direct_results["referrer"], "about:client");
    assert_eq!(direct_results["referrerPolicy"], "");
    assert_eq!(direct_results["cloneReferrer"], "about:client");
    assert_eq!(direct_results["overridePolicy"], "origin");
    assert_eq!(
        direct_results["customReferrer"],
        format!("{page_origin}/custom?key=1")
    );
    assert_eq!(
        direct_results["customCloneReferrer"],
        format!("{page_origin}/custom?key=1")
    );
    assert_eq!(direct_results["customPolicy"], "unsafe-url");
    assert_eq!(direct_results["customInitReferrer"], "about:client");
    assert_eq!(direct_results["customInitPolicy"], "");
    assert_eq!(direct_results["customOverrideReferrer"], "about:client");
    assert_eq!(direct_results["customOverridePolicy"], "origin");
    assert_eq!(direct_results["resetReferrer"], "about:client");
    assert_eq!(direct_results["invalidRequest"], true);
    assert_eq!(direct_results["invalidFetch"], true);
    assert_eq!(
        sw_referrers,
        serde_json::json!(["none", controlled_referrer])
    );
    assert_eq!(
        sw_request_properties,
        serde_json::json!(["about:client", "unsafe-url", "about:client", "same-origin"])
    );
    assert_eq!(
        primary_requests.len(),
        7,
        "primary request targets: {:?}",
        primary_requests
            .iter()
            .map(|(_, target, _)| target)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        cross_requests.len(),
        6,
        "cross-origin request targets: {:?}",
        cross_requests
            .iter()
            .map(|(target, _)| target)
            .collect::<Vec<_>>()
    );
}
#[test]
fn native_content_process_service_worker_fetch_cors_errors_keep_parent_cookies() {
    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(header, value)| {
                header.eq_ignore_ascii_case(name).then_some(value.trim())
            })
        })
    }

    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let _guard = native_content_process_test_lock().lock().await;
            let profile_path = std::env::temp_dir().join(format!(
                "glass-native-service-worker-fetch-cors-{}-profile.json",
                std::process::id()
            ));
            let cookie_profile_path =
                std::path::PathBuf::from(format!("{}.cookies", profile_path.display()));
            let profile_lock_path = profile_path.with_extension("lock");
            let cookie_profile_lock_path = cookie_profile_path.with_extension("lock");
            let worker_clients_path = profile_path.with_extension("clients");
            let worker_events_path = profile_path.with_extension("events");
            let worker_readers_path = profile_path.with_extension("readers");
            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &worker_clients_path,
                &worker_events_path,
                &worker_readers_path,
            ] {
                let _ = fs::remove_file(path);
            }

            let page_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let page_address = page_listener.local_addr().unwrap();
            let api_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let api_address = api_listener.local_addr().unwrap();
            let page_origin = format!("http://{page_address}");
            let api_origin = format!("http://{api_address}");
            let service_worker_api_origin = api_origin.clone();
            let expected_origin = page_origin.clone();
            let response_origin = page_origin.clone();

            let page_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for expected_path in ["/page", "/sw.js"] {
                        let (mut stream, _) = page_listener.accept().await.unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(expected_path));
                        let cookie = header_value(&request, "cookie").unwrap_or_default();
                        let (content_type, set_cookie, body) = if expected_path == "/page" {
                            (
                                "text/html",
                                "Set-Cookie: sw_cors_seed=initial; HttpOnly; Path=/; SameSite=Lax\r\n",
                                r#"<script>
globalThis.serviceWorkerRegistrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });
</script>"#
                                    .to_owned(),
                            )
                        } else {
                            assert!(
                                cookie.contains("sw_cors_seed=initial"),
                                "ServiceWorker script missed parent seed cookie: {cookie}"
                            );
                            (
                                "text/javascript",
                                "Set-Cookie: sw_cors_entry=worker; HttpOnly; Path=/; SameSite=Lax\r\n",
                                format!(
                                    r#"const apiOrigin = "{service_worker_api_origin}";
self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {{
  const path = new URL(event.request.url).pathname;
  if (path === '/cors-failed-navigation') {{
    event.respondWith((async () => {{
      try {{
        const response = await fetch(apiOrigin + '/cors-failed', {{ credentials: 'include' }});
        return new Response('unexpected:' + response.status, {{
          headers: {{ 'Content-Type': 'text/html' }}
        }});
      }} catch (error) {{
        return new Response('<!doctype html><body>handled:' + error.name + ':' + (error instanceof TypeError) + '</body>', {{
          headers: {{ 'Content-Type': 'text/html' }}
        }});
      }}
    }})());
  }} else if (path === '/worker-probe') {{
    event.respondWith((async () => {{
      try {{
        const response = await fetch(apiOrigin + '/cors-failed-explicit', {{ credentials: 'include' }});
        return new Response('unexpected:' + response.status, {{
          headers: {{ 'Content-Type': 'text/plain' }}
        }});
      }} catch (error) {{
        return new Response('handled-worker:' + error.name + ':' + (error instanceof TypeError), {{
          headers: {{ 'Content-Type': 'text/plain' }}
        }});
      }}
    }})());
  }} else if (path === '/worker-verify-probe') {{
    event.respondWith((async () => {{
      const response = await fetch(apiOrigin + '/worker-verify', {{ credentials: 'include' }});
      return new Response(await response.text(), {{
        headers: {{ 'Content-Type': 'text/plain' }}
      }});
    }})());
  }}
}});"#
                                ),
                            )
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{set_cookie}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    }
                })
                .await
                .expect("ServiceWorker page and script requests stay bounded")
            });

            let api_server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(45), async move {
                    for (path, required_cookies) in [
                        (
                            "/cors-failed",
                            vec!["sw_cors_seed=initial", "sw_cors_entry=worker"],
                        ),
                        (
                            "/cors-failed-explicit",
                            vec![
                                "sw_cors_seed=initial",
                                "sw_cors_entry=worker",
                                "sw_cors_rejected_navigation=accepted",
                            ],
                        ),
                        (
                            "/worker-verify",
                            vec![
                                "sw_cors_seed=initial",
                                "sw_cors_entry=worker",
                                "sw_cors_rejected_navigation=accepted",
                                "sw_cors_rejected_explicit=accepted",
                            ],
                        ),
                        (
                            "/page-verify",
                            vec![
                                "sw_cors_seed=initial",
                                "sw_cors_entry=worker",
                                "sw_cors_rejected_navigation=accepted",
                                "sw_cors_rejected_explicit=accepted",
                            ],
                        ),
                    ] {
                        let (mut stream, _) = tokio::time::timeout(
                            Duration::from_secs(30),
                            api_listener.accept(),
                        )
                        .await
                        .expect("authorized ServiceWorker cookie request reaches the parent")
                        .unwrap();
                        let request = read_http_request(&mut stream).await;
                        assert_eq!(request.split_whitespace().next(), Some("GET"));
                        assert_eq!(request.split_whitespace().nth(1), Some(path));
                        assert_eq!(header_value(&request, "origin"), Some(expected_origin.as_str()));
                        assert!(header_value(&request, "access-control-request-method").is_none());
                        assert!(header_value(&request, "access-control-request-headers").is_none());
                        let cookie = header_value(&request, "cookie").unwrap_or_default();
                        for expected in required_cookies {
                            assert!(cookie.contains(expected), "{path} missed {expected}: {cookie}");
                        }

                        let (cors_headers, set_cookie, body) = match path {
                            "/cors-failed" => (
                                "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\n".to_owned(),
                                "Set-Cookie: sw_cors_rejected_navigation=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "hidden",
                            ),
                            "/cors-failed-explicit" => (
                                "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Credentials: true\r\n".to_owned(),
                                "Set-Cookie: sw_cors_rejected_explicit=accepted; HttpOnly; Path=/; SameSite=Lax\r\n",
                                "hidden",
                            ),
                            "/worker-verify" => (
                                format!(
                                    "Access-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                                ),
                                "",
                                "worker-ok",
                            ),
                            "/page-verify" => (
                                format!(
                                    "Access-Control-Allow-Origin: {response_origin}\r\nAccess-Control-Allow-Credentials: true\r\n"
                                ),
                                "",
                                "page-ok",
                            ),
                            _ => unreachable!("only the declared ServiceWorker cookie paths are served"),
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n{cors_headers}{set_cookie}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes()).await.unwrap();
                    }

                    match tokio::time::timeout(Duration::from_secs(1), api_listener.accept()).await {
                        Err(_) => {}
                        Ok(Ok((mut stream, _))) => {
                            let request = read_http_request(&mut stream).await;
                            panic!("unexpected ServiceWorker cookie request: {request}");
                        }
                        Ok(Err(error)) => panic!("unexpected API listener error: {error}"),
                    }
                })
                .await
                .expect("ServiceWorker CORS-cookie requests stay bounded")
            });

            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default()
                    .with_storage_path(profile_path.clone())
                    .with_initial_url(format!("{page_origin}/page")),
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let state = session
                        .script(
                            "await serviceWorkerRegistrationPromise.then(registration => [registration.active && registration.active.state, navigator.serviceWorker.controller !== null])",
                        )
                        .await
                        .unwrap()
                        .value;
                    if state == serde_json::json!(["activated", true]) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("ServiceWorker activates and controls the page");

            session
                .navigate(format!("{page_origin}/cors-failed-navigation"))
                .await
                .unwrap();
            assert_eq!(
                session.script("document.body.textContent").await.unwrap().value,
                serde_json::json!("handled:TypeError:true")
            );
            assert_eq!(
                session
                    .script("await fetch('/worker-probe').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("handled-worker:TypeError:true")
            );
            assert_eq!(
                session
                    .script("await fetch('/worker-verify-probe').then(response => response.text())")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("worker-ok")
            );
            assert_eq!(
                session
                    .script(&format!(
                        "await fetch('{api_origin}/page-verify', {{ credentials: 'include' }}).then(response => response.text())"
                    ))
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("page-ok")
            );
            assert_eq!(
                session.script("document.cookie").await.unwrap().value,
                serde_json::json!("")
            );
            let cookies = session.native_cookies().await.unwrap();
            session.close().await.unwrap();
            page_server.await.unwrap();
            api_server.await.unwrap();

            for (name, value) in [
                ("sw_cors_seed", "initial"),
                ("sw_cors_entry", "worker"),
                ("sw_cors_rejected_navigation", "accepted"),
                ("sw_cors_rejected_explicit", "accepted"),
            ] {
                assert!(
                    cookies.iter().any(|cookie| {
                        cookie.name == name && cookie.value == value && cookie.http_only
                    }),
                    "parent cookie jar is missing HttpOnly {name}={value}"
                );
            }

            for path in [
                &profile_path,
                &cookie_profile_path,
                &profile_lock_path,
                &cookie_profile_lock_path,
                &worker_clients_path,
                &worker_events_path,
                &worker_readers_path,
            ] {
                let _ = fs::remove_file(path);
            }
        });
    });
}
#[tokio::test]
async fn native_content_process_service_worker_fetches_use_script_policy_container() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut requests = Vec::new();
            for _ in 0..13 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_http_request(&mut stream).await;
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .expect("ServiceWorker policy request includes a URL")
                    .to_owned();
                let referer = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("referer")
                        .then(|| value.trim().to_owned())
                });
                let (extra_headers, content_type, body) = match path.as_str() {
                    "/page" => (
                        "",
                        "text/html",
                        "<script>globalThis.registrationStates = ['pending', 'pending', 'pending']; const track = (index, script, options) => navigator.serviceWorker.register(script, options).then(() => registrationStates[index] = 'ready', error => registrationStates[index] = String(error)); globalThis.registrationPromise = Promise.all([track(0, '/scopes/no-ref/sw.js'), track(1, '/scopes/origin/sw.js'), track(2, '/scopes/module/sw.js', { type: 'module' })]);</script>",
                    ),
                    "/scopes/no-ref/sw.js" => (
                        "Referrer-Policy: no-referrer\r\n",
                        "application/javascript",
                        "self.addEventListener('install', event => event.waitUntil((async () => { await fetch('/probe/no-ref/fetch'); await new Promise((resolve, reject) => { const xhr = new XMLHttpRequest(); xhr.onload = resolve; xhr.onerror = () => reject(new Error('XHR failed')); xhr.open('GET', '/probe/no-ref/xhr'); xhr.send(); }); await self.skipWaiting(); })()));",
                    ),
                    "/scopes/origin/sw.js" => (
                        "Referrer-Policy: origin\r\n",
                        "application/javascript",
                        "self.addEventListener('install', event => event.waitUntil((async () => { await fetch('/probe/origin/fetch'); await fetch('/probe/origin/override', { referrerPolicy: 'no-referrer' }); await new Promise((resolve, reject) => { const xhr = new XMLHttpRequest(); xhr.onload = resolve; xhr.onerror = () => reject(new Error('XHR failed')); xhr.open('GET', '/probe/origin/xhr'); xhr.send(); }); await self.skipWaiting(); })()));",
                    ),
                    "/scopes/module/sw.js" => (
                        "",
                        "application/javascript",
                        "import { bridgeValue } from './bridge.js'; self.addEventListener('install', event => event.waitUntil((async () => { await fetch('/probe/module/fetch'); await new Promise((resolve, reject) => { const xhr = new XMLHttpRequest(); xhr.onload = resolve; xhr.onerror = () => reject(new Error('XHR failed')); xhr.open('GET', '/probe/module/xhr'); xhr.send(); }); await self.skipWaiting(); })()));",
                    ),
                    "/scopes/module/bridge.js" => (
                        "Referrer-Policy: origin\r\n",
                        "application/javascript",
                        "import { leafValue } from './leaf.js'; export const bridgeValue = leafValue;",
                    ),
                    "/scopes/module/leaf.js" => (
                        "",
                        "application/javascript",
                        "export const leafValue = 'loaded';",
                    ),
                    path if path.starts_with("/probe/") => (
                        "",
                        "text/plain",
                        "ok",
                    ),
                    other => panic!("unexpected ServiceWorker policy request: {other}"),
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                requests.push((path, referer));
            }
            requests.sort();
            requests
        })
        .await
        .expect("ServiceWorker policy requests stay within their time bound")
    });

    let mut engine = NativeEngine::new(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/page")),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    let mut states = serde_json::Value::Null;
    for _ in 0..16 {
        states = engine.evaluate_async("registrationStates").await.unwrap();
        if states
            .as_array()
            .is_some_and(|states| states.iter().all(|state| state == "ready"))
        {
            break;
        }
    }
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(states, serde_json::json!(["ready", "ready", "ready"]));
    let expected_paths = [
        "/page",
        "/scopes/no-ref/sw.js",
        "/probe/no-ref/fetch",
        "/probe/no-ref/xhr",
        "/scopes/origin/sw.js",
        "/probe/origin/fetch",
        "/probe/origin/override",
        "/probe/origin/xhr",
        "/scopes/module/sw.js",
        "/scopes/module/bridge.js",
        "/scopes/module/leaf.js",
        "/probe/module/fetch",
        "/probe/module/xhr",
    ];
    let mut actual_paths = requests
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    actual_paths.sort_unstable();
    let mut expected_paths = expected_paths.to_vec();
    expected_paths.sort_unstable();
    assert_eq!(actual_paths, expected_paths);

    let origin = format!("http://{address}");
    let root_referrer = |path: &str| format!("{origin}{path}");
    let referer_for = |path: &str| {
        requests
            .iter()
            .find(|(request_path, _)| request_path == path)
            .unwrap_or_else(|| panic!("missing request {path}"))
            .1
            .as_deref()
            .map(str::to_owned)
    };
    for path in ["/probe/no-ref/fetch", "/probe/no-ref/xhr"] {
        assert_eq!(
            referer_for(path),
            None,
            "ServiceWorker no-referrer for {path}"
        );
    }
    assert_eq!(
        referer_for("/probe/origin/fetch"),
        Some(format!("{origin}/"))
    );
    assert_eq!(referer_for("/probe/origin/xhr"), Some(format!("{origin}/")));
    assert_eq!(referer_for("/probe/origin/override"), None);
    for path in ["/probe/module/fetch", "/probe/module/xhr"] {
        assert_eq!(
            referer_for(path),
            Some(root_referrer("/scopes/module/sw.js")),
            "missing root response policy defaults to strict-origin-when-cross-origin for {path}"
        );
    }
    assert_eq!(
        referer_for("/scopes/module/bridge.js"),
        Some(root_referrer("/scopes/module/sw.js"))
    );
    assert_eq!(
        referer_for("/scopes/module/leaf.js"),
        Some(format!("{origin}/")),
        "a module dependency response policy governs its descendants"
    );
}
#[tokio::test]
async fn native_content_process_service_worker_registration_uses_live_document_referrer_policy() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let target = request
                .split_whitespace()
                .nth(1)
                .expect("ServiceWorker registration request includes a target")
                .to_owned();
            let path = target.split('?').next().unwrap_or_default();
            let referer = request.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("referer")
                    .then(|| value.trim().to_owned())
            });
            let (extra_headers, content_type, body) = match path {
                "/page" => (
                    "Referrer-Policy: no-referrer\r\n",
                    "text/html",
                    "<!doctype html><meta name='referrer' content='no-referrer'><main>register</main>",
                ),
                "/scopes/no-ref/sw.js" => (
                    "Referrer-Policy: unsafe-url\r\nCache-Control: no-store\r\n",
                    "application/javascript",
                    "self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));",
                ),
                "/scopes/origin/sw.js" => (
                    "Referrer-Policy: no-referrer\r\nCache-Control: no-store\r\n",
                    "application/javascript",
                    "self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));",
                ),
                "/scopes/module/sw.js" => (
                    "Referrer-Policy: no-referrer\r\nCache-Control: no-store\r\n",
                    "application/javascript",
                    "self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));",
                ),
                other => panic!("unexpected ServiceWorker registration request: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\n{extra_headers}Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            requests.push((target, referer));
        }
        requests
    });

    let page_url = format!("http://{address}/page?private=1");
    let mut engine =
        NativeEngine::new(NativeEngineConfig::default().with_initial_url(page_url.clone()))
            .unwrap();
    engine.initialize_async().await.unwrap();
    let pushed_page_url = format!("http://{address}/pushed?state=secret");
    assert_eq!(
        engine
            .evaluate_async("history.pushState({}, '', '/pushed?state=secret'); location.href",)
            .await
            .unwrap(),
        serde_json::json!(pushed_page_url),
        "same-document history changes the active URL without changing the creation URL"
    );

    assert_eq!(
        engine
            .evaluate_async(
                "await navigator.serviceWorker.register('/scopes/no-ref/sw.js').then(() => 'registered')",
            )
            .await
            .unwrap(),
        serde_json::json!("registered")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.querySelector('meta[name=referrer]').content = 'origin'; 'origin'",
            )
            .await
            .unwrap(),
        serde_json::json!("origin")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await navigator.serviceWorker.register('/scopes/origin/sw.js').then(() => 'registered')",
            )
            .await
            .unwrap(),
        serde_json::json!("registered")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.querySelector('meta[name=referrer]').content = 'unsafe-url'; 'unsafe-url'",
            )
            .await
            .unwrap(),
        serde_json::json!("unsafe-url")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "await navigator.serviceWorker.register('/scopes/module/sw.js', { type: 'module' }).then(reg => { globalThis.__slice837Registration = reg; return 'registered'; })",
            )
            .await
            .unwrap(),
        serde_json::json!("registered")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "document.querySelector('meta[name=referrer]').content = 'no-referrer'; 'no-referrer'",
            )
            .await
            .unwrap(),
        serde_json::json!("no-referrer")
    );
    assert_eq!(
        engine
            .evaluate_async("await globalThis.__slice837Registration.update(); 'updated'",)
            .await
            .unwrap(),
        serde_json::json!("updated")
    );
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(target, _)| target.as_str())
            .collect::<Vec<_>>(),
        [
            "/page?private=1",
            "/scopes/no-ref/sw.js",
            "/scopes/origin/sw.js",
            "/scopes/module/sw.js",
            "/scopes/module/sw.js",
        ]
    );
    let origin = format!("http://{address}/");
    assert_eq!(requests[1].1, None, "response policy is not retroactive");
    assert_eq!(requests[2].1.as_deref(), Some(origin.as_str()));
    assert_eq!(
        requests[3].1.as_deref(),
        Some(page_url.as_str()),
        "unsafe-url uses the active Document URL including its query"
    );
    assert_eq!(
        requests[4].1, None,
        "update captures the live Document policy"
    );
}

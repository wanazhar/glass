use super::super::config::NativeEngineLimits;
use super::{
    BTreeMap, MAX_NATIVE_MODULE_IMPORTS, Module, NativeDocument, NativeFetchReferrerPolicy,
    NativeJavaScriptRuntime, NativeModuleImportMap, NativeModuleResourceType, NativeOrigin,
    NativePageEventBatch, NativePageScript, NativeScriptCommand,
    NativeServiceWorkerRegistrationState, NativeServiceWorkerWorkerState, Viewport,
    effective_worker_global_referrer_policy, effective_worker_module_referrer_policy,
    module_source_maps, native_module_loader_name, rewrite_dynamic_imports_as_rejected,
    rewrite_dynamic_imports_as_rejected_with_count, rewrite_runtime_dynamic_module_imports,
    rewrite_runtime_dynamic_module_imports_with_policy, rewrite_static_json_module_specifiers,
    static_module_requests,
};
use crate::native_engine::javascript::NativePageEventContext;
use base64::Engine as _;

#[test]
fn module_source_maps_preserve_referrer_policy_per_module_identity() {
    let sources = [
        NativePageScript::Module {
            name: "https://document.test/#inline-1".into(),
            source: "import('./one.js')".into(),
            base_url: "https://document.test/".into(),
            referrer_policy: Some(NativeFetchReferrerPolicy::NoReferrer),
            node_index: Some(1),
        },
        NativePageScript::Module {
            name: "https://document.test/#inline-2".into(),
            source: "import('./two.js')".into(),
            base_url: "https://document.test/".into(),
            referrer_policy: Some(NativeFetchReferrerPolicy::Origin),
            node_index: Some(2),
        },
        NativePageScript::ModuleDependency {
            name: "https://document.test/static.js".into(),
            source: "import('./nested.js')".into(),
            base_url: "https://cdn.test/static.js".into(),
            referrer_policy: Some(NativeFetchReferrerPolicy::StrictOrigin),
        },
    ];

    let (module_sources, module_base_urls, referrer_policies) = module_source_maps(&sources);
    assert_eq!(module_sources.len(), 3);
    assert_eq!(
        module_base_urls["https://document.test/#inline-1"],
        "https://document.test/"
    );
    assert_eq!(
        referrer_policies["https://document.test/#inline-1"],
        NativeFetchReferrerPolicy::NoReferrer
    );
    assert_eq!(
        referrer_policies["https://document.test/#inline-2"],
        NativeFetchReferrerPolicy::Origin
    );
    assert_eq!(
        referrer_policies["https://document.test/static.js"],
        NativeFetchReferrerPolicy::StrictOrigin
    );
}

#[test]
fn page_module_dynamic_imports_settle_independently_after_host_responses() {
    fn dynamic_request(
        evaluation: &super::NativeScriptEvaluation,
    ) -> (u32, String, Option<String>, Option<String>) {
        evaluation
            .commands
            .iter()
            .find_map(|command| match command {
                NativeScriptCommand::Fetch {
                    request_id,
                    href,
                    destination,
                    module_referrer,
                    referrer_policy,
                    ..
                } if destination.as_deref() == Some("module") => Some((
                    *request_id,
                    href.clone(),
                    module_referrer.clone(),
                    referrer_policy.clone(),
                )),
                _ => None,
            })
            .expect("dynamic import must cross the host fetch boundary")
    }

    fn resolve_import(
        runtime: &NativeJavaScriptRuntime,
        request_id: u32,
        target: &str,
        source: &str,
        document: &NativeDocument,
        document_url: &url::Url,
        origin: &NativeOrigin,
    ) {
        let module_name = native_module_loader_name(target, NativeModuleResourceType::JavaScript);
        runtime
            .extend_module_sources(
                BTreeMap::from([(module_name.clone(), source.into())]),
                BTreeMap::from([(module_name.clone(), target.into())]),
            )
            .expect("host-loaded module source must be retained");
        runtime
            .extend_module_referrer_policies(BTreeMap::from([(
                module_name,
                NativeFetchReferrerPolicy::Origin,
            )]))
            .expect("host-loaded module policy must be retained");
        let alias = runtime
            .register_dynamic_module_alias(request_id, target)
            .expect("host response must register its module alias");
        runtime
            .resolve_fetch(
                request_id,
                &serde_json::json!({"dynamicModuleImport": {"moduleKey": alias}}),
                NativePageEventContext {
                    document,
                    document_url: document_url.as_str(),
                    origin,
                    viewport: Viewport::default(),
                    csp_violations: &[],
                },
                &NativePageEventBatch::default(),
            )
            .expect("host response must resume the pending import");
        runtime.discard_dynamic_module_alias(&alias);
    }

    let document_url = url::Url::parse("https://dynamic-import.test/page?secret=token")
        .expect("document URL is valid");
    let document =
        NativeDocument::parse("<html><body></body></html>", &NativeEngineLimits::default())
            .expect("fixture document must parse");
    let origin = NativeOrigin::from_url(&document_url).expect("document origin is valid");
    let external_url =
        url::Url::parse("https://module.test/entry.js").expect("external module URL is valid");
    let external_name = external_url.to_string();
    let external_source = "globalThis.externalImportSettled = false; globalThis.externalImportError = null; import('./external-child.js').then(() => { globalThis.externalImportSettled = true; }, error => { globalThis.externalImportError = String(error); globalThis.externalImportSettled = true; });";
    let inline_name = format!("{}#glass-inline-module-0", document_url);
    let inline_source = "globalThis.inlineImportSettled = false; globalThis.inlineImportError = null; import('./inline-child.js').then(() => { globalThis.inlineImportSettled = true; }, error => { globalThis.inlineImportError = String(error); globalThis.inlineImportSettled = true; });";
    let runtime = NativeJavaScriptRuntime::new_with_context_id("inline-dynamic-import-settlement")
        .expect("native JavaScript runtime must construct");
    runtime.set_module_sources(BTreeMap::from([
        (external_name.clone(), external_source.into()),
        (inline_name.clone(), inline_source.into()),
    ]));
    runtime.set_module_base_urls(BTreeMap::from([
        (external_name.clone(), external_url.to_string()),
        (inline_name.clone(), document_url.to_string()),
    ]));
    runtime.set_module_referrer_policies(BTreeMap::from([
        (external_name.clone(), NativeFetchReferrerPolicy::Origin),
        (inline_name.clone(), NativeFetchReferrerPolicy::Origin),
    ]));

    let external_evaluation = runtime
        .evaluate_module(
            &external_name,
            external_source,
            &document,
            document_url.as_str(),
            &origin,
            Viewport::default(),
        )
        .expect("external module must emit its dynamic import request");
    let inline_evaluation = runtime
        .evaluate_module(
            &inline_name,
            inline_source,
            &document,
            document_url.as_str(),
            &origin,
            Viewport::default(),
        )
        .expect("inline module must emit its dynamic import request");
    let (external_request_id, external_specifier, external_referrer, external_policy) =
        dynamic_request(&external_evaluation);
    let (inline_request_id, inline_specifier, inline_referrer, inline_policy) =
        dynamic_request(&inline_evaluation);
    assert_ne!(external_request_id, inline_request_id);
    assert_eq!(external_referrer.as_deref(), Some(external_url.as_str()));
    assert_eq!(external_policy.as_deref(), Some("origin"));
    assert_eq!(inline_referrer.as_deref(), Some(document_url.as_str()));
    assert_eq!(inline_policy.as_deref(), Some("origin"));

    let external_target = external_url
        .join(&external_specifier)
        .expect("external dynamic module URL resolves against its referrer")
        .to_string();
    let inline_target = document_url
        .join(&inline_specifier)
        .expect("dynamic module URL resolves against its referrer")
        .to_string();
    resolve_import(
        &runtime,
        external_request_id,
        &external_target,
        "globalThis.externalChildEvaluated = true;",
        &document,
        &document_url,
        &origin,
    );
    resolve_import(
        &runtime,
        inline_request_id,
        &inline_target,
        "globalThis.inlineChildEvaluated = true;",
        &document,
        &document_url,
        &origin,
    );

    let state = runtime
            .context
            .with(|ctx| {
                ctx.eval::<String, _>(
                    "JSON.stringify([globalThis.externalChildEvaluated === true, globalThis.externalImportSettled, globalThis.externalImportError, globalThis.inlineChildEvaluated === true, globalThis.inlineImportSettled, globalThis.inlineImportError])",
                )
            })
            .expect("settled import state must be readable");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&state).unwrap(),
        serde_json::json!([true, true, null, true, true, null]),
        "overlapping page-module dynamic imports must settle independently: {state}"
    );
}

#[test]
fn worker_module_response_policy_overrides_inherited_policy_only_when_recognized() {
    assert_eq!(
        effective_worker_module_referrer_policy(
            Some(NativeFetchReferrerPolicy::Origin),
            Some(NativeFetchReferrerPolicy::NoReferrer),
        ),
        Some(NativeFetchReferrerPolicy::NoReferrer)
    );
    assert_eq!(
        effective_worker_module_referrer_policy(Some(NativeFetchReferrerPolicy::Origin), None,),
        Some(NativeFetchReferrerPolicy::Origin)
    );
    assert_eq!(
        effective_worker_module_referrer_policy(None, Some(NativeFetchReferrerPolicy::Origin)),
        None
    );
}

#[test]
fn worker_global_policy_uses_network_response_and_local_creator_container() {
    assert_eq!(
        effective_worker_global_referrer_policy(
            "https://worker.test/worker.js",
            Some(NativeFetchReferrerPolicy::NoReferrer),
            Some(NativeFetchReferrerPolicy::UnsafeUrl),
        ),
        NativeFetchReferrerPolicy::UnsafeUrl
    );
    assert_eq!(
        effective_worker_global_referrer_policy(
            "https://worker.test/worker.js",
            Some(NativeFetchReferrerPolicy::NoReferrer),
            None,
        ),
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin
    );
    assert_eq!(
        effective_worker_global_referrer_policy(
            "file:///worker.js",
            Some(NativeFetchReferrerPolicy::Origin),
            None,
        ),
        NativeFetchReferrerPolicy::Origin
    );
    assert_eq!(
        effective_worker_global_referrer_policy("file:///worker.js", None, None),
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin
    );
}

#[test]
fn service_worker_entry_commands_capture_live_document_referrer_policy() {
    let document_url = "https://document.test/page?private=1";
    let document = NativeDocument::parse(
        "<html><head><meta name='referrer' content='no-referrer'></head><body></body></html>",
        &NativeEngineLimits::default(),
    )
    .expect("ServiceWorker policy fixture parses");
    let origin =
        NativeOrigin::from_url(&url::Url::parse(document_url).expect("Document URL parses"))
            .expect("Document origin is valid");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("sw-entry-policy")
        .expect("native JavaScript runtime must construct");
    runtime.set_service_worker_registrations(vec![NativeServiceWorkerRegistrationState {
        script_url: "https://document.test/existing/sw.js".into(),
        scope: "https://document.test/existing/".into(),
        state: "activated".into(),
        controlled: false,
        lifecycle: Vec::new(),
        active: Some(NativeServiceWorkerWorkerState {
            script_url: "https://document.test/existing/sw.js".into(),
            state: "activated".into(),
        }),
        waiting: None,
    }]);

    let initial_evaluation = runtime
        .evaluate(
            r##"(() => {
                  history.pushState({}, '', '/pushed?state=1');
                  navigator.serviceWorker.register('/scopes/no-ref/sw.js');
                  return true;
                })()"##,
            &document,
            document_url,
            &origin,
            Viewport::default(),
        )
        .expect("initial ServiceWorker command evaluates without network access");
    assert_eq!(initial_evaluation.value, serde_json::json!(true));

    let current_url = "https://document.test/pushed?state=1";
    let live_policy_evaluation = runtime
        .evaluate(
            r##"(() => {
                  const meta = document.querySelector("meta[name=referrer]");
                  meta.content = 'origin';
                  navigator.serviceWorker.register('/scopes/origin/sw.js');
                  return true;
                })()"##,
            &document,
            current_url,
            &origin,
            Viewport::default(),
        )
        .expect("live policy command evaluates without network access");
    assert_eq!(live_policy_evaluation.value, serde_json::json!(true));

    let update_evaluation = runtime
        .evaluate(
            r##"(() => {
                  const meta = document.querySelector("meta[name=referrer]");
                  meta.content = 'unsafe-url';
                  navigator.serviceWorker.register('/scopes/module/sw.js', { type: 'module' });
                  globalThis.__glassServiceWorkerRegistrations
                    .get('https://document.test/existing/')
                    .update();
                  return true;
                })()"##,
            &document,
            current_url,
            &origin,
            Viewport::default(),
        )
        .expect("ServiceWorker update evaluates without network access");
    assert_eq!(update_evaluation.value, serde_json::json!(true));

    let command_policies = runtime
        .take_service_worker_commands()
        .into_iter()
        .map(|command| match command {
            NativeScriptCommand::ServiceWorkerRegister {
                script_url,
                worker_type,
                referrer_url,
                referrer_policy,
                ..
            } => (
                "register",
                script_url,
                worker_type,
                referrer_url,
                referrer_policy,
            ),
            NativeScriptCommand::ServiceWorkerUpdate {
                scope,
                referrer_url,
                referrer_policy,
                ..
            } => (
                "update",
                scope,
                String::new(),
                referrer_url,
                referrer_policy,
            ),
            command => panic!("unexpected ServiceWorker command: {command:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        command_policies,
        [
            (
                "register",
                "https://document.test/scopes/no-ref/sw.js".into(),
                "classic".into(),
                "https://document.test/page?private=1".into(),
                "no-referrer".into(),
            ),
            (
                "register",
                "https://document.test/scopes/origin/sw.js".into(),
                "classic".into(),
                "https://document.test/page?private=1".into(),
                "origin".into(),
            ),
            (
                "register",
                "https://document.test/scopes/module/sw.js".into(),
                "module".into(),
                "https://document.test/page?private=1".into(),
                "unsafe-url".into(),
            ),
            (
                "update",
                "https://document.test/existing/".into(),
                String::new(),
                "https://document.test/page?private=1".into(),
                "unsafe-url".into(),
            ),
        ]
    );

    let invalid_policy = NativeScriptCommand::ServiceWorkerRegister {
        request_id: 42,
        script_url: "https://document.test/scopes/invalid/sw.js".into(),
        scope: "https://document.test/scopes/invalid/".into(),
        worker_type: "classic".into(),
        referrer_url: "https://document.test/page".into(),
        referrer_policy: "not-a-policy".into(),
    };
    assert!(
        runtime
            .apply_service_worker_command(&invalid_policy)
            .is_err()
    );
    assert!(runtime.take_service_worker_commands().is_empty());
}

#[test]
fn service_worker_navigation_preload_manager_is_stable_and_validates_header_values() {
    let document_url = "https://document.test/page";
    let document =
        NativeDocument::parse("<html><body></body></html>", &NativeEngineLimits::default())
            .expect("Navigation Preload fixture parses");
    let origin =
        NativeOrigin::from_url(&url::Url::parse(document_url).expect("Document URL parses"))
            .expect("Document origin is valid");
    let runtime = NativeJavaScriptRuntime::new_with_context_id("sw-navigation-preload")
        .expect("native JavaScript runtime must construct");
    runtime.set_service_worker_registrations(vec![NativeServiceWorkerRegistrationState {
        script_url: "https://document.test/sw.js".into(),
        scope: "https://document.test/".into(),
        state: "activated".into(),
        controlled: true,
        lifecycle: Vec::new(),
        active: Some(NativeServiceWorkerWorkerState {
            script_url: "https://document.test/sw.js".into(),
            state: "activated".into(),
        }),
        waiting: None,
    }]);

    let evaluation = runtime
        .evaluate(
            r##"(() => {
                  const registration = globalThis.__glassServiceWorkerRegistrations
                    .get('https://document.test/');
                  const manager = registration.navigationPreload;
                  const sameObject = manager === registration.navigationPreload
                    && manager instanceof NavigationPreloadManager;
                  globalThis.__navigationPreloadState = manager.getState();
                  manager.enable();
                  manager.setHeaderValue('\tglass-preview  \t');
                  manager.disable();
                  manager.getState();
                  return sameObject;
                })()"##,
            &document,
            document_url,
            &origin,
            Viewport::default(),
        )
        .expect("NavigationPreloadManager methods queue host commands");
    assert_eq!(evaluation.value, serde_json::json!(true));

    let operations = runtime
        .take_service_worker_commands()
        .into_iter()
        .map(|command| match command {
            NativeScriptCommand::ServiceWorkerNavigationPreload {
                operation,
                header_value,
                ..
            } => (operation, header_value),
            command => panic!("unexpected Navigation Preload command: {command:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        operations,
        [
            ("getState".into(), None),
            ("enable".into(), None),
            ("setHeaderValue".into(), Some("glass-preview".into())),
            ("disable".into(), None),
            ("getState".into(), None),
        ]
    );

    for valid in ["", "true", "header\tvalue", "latin-1-\u{00ff}"] {
        super::validate_navigation_preload_header_value(valid)
            .expect("normalized ByteString header value is valid");
    }
    for invalid in [
        " leading",
        "trailing ",
        "nul\0value",
        "line\nvalue",
        "wide-\u{0100}",
    ] {
        assert!(
            super::validate_navigation_preload_header_value(invalid).is_err(),
            "header value {invalid:?} is rejected at the native boundary"
        );
    }

    let invalid_command = NativeScriptCommand::ServiceWorkerNavigationPreload {
        request_id: 91,
        scope: "https://document.test/".into(),
        operation: "setHeaderValue".into(),
        header_value: Some("not-normalized ".into()),
    };
    assert!(
        runtime
            .apply_service_worker_command(&invalid_command)
            .is_err()
    );
    assert!(runtime.take_service_worker_commands().is_empty());
}

#[test]
fn service_worker_navigation_preload_response_resolves_to_a_readable_response() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("sw-navigation-preload-response")
        .expect("ServiceWorker runtime is valid");
    let worker_id = 42;
    let worker_url = "https://preload.test/sw.js";
    runtime
            .evaluate_service_worker_source(
                worker_id,
                worker_url,
                None,
                r#"self.addEventListener('fetch', event => {
  const navigationRequest = event.request;
  const clonedNavigationMode = navigationRequest.clone().mode;
  let constructorRejectsNavigationMode = false;
  try { new Request(navigationRequest.url, { mode: 'navigate' }); }
  catch (error) { constructorRejectsNavigationMode = error instanceof TypeError; }
  let clonedRequestInitMode = 'failed';
  try { clonedRequestInitMode = new Request(navigationRequest, { headers: { 'x-forwarded': 'yes' } }).mode; }
  catch (_error) {}
  let clonedExplicitNavigationModeRejects = false;
  try { new Request(navigationRequest, { mode: 'navigate' }); }
  catch (error) { clonedExplicitNavigationModeRejects = error instanceof TypeError; }
  event.respondWith(event.preloadResponse.then(async response => {
    if (!(response instanceof Response)) return new Response('missing');
    const clonedResponse = response.clone();
    let immutableHeaders = false;
    try { response.headers.set('x-preload-marker', 'mutated'); }
    catch (error) { immutableHeaders = error instanceof TypeError; }
    return new Response(JSON.stringify({
      status: response.status,
      url: response.url,
      redirected: response.redirected,
      type: response.type,
      mode: navigationRequest.mode,
      clonedMode: clonedNavigationMode,
      referrer: navigationRequest.referrer,
      referrerPolicy: navigationRequest.referrerPolicy,
      constructorRejectsNavigationMode,
      clonedRequestInitMode,
      clonedExplicitNavigationModeRejects,
      marker: response.headers.get('x-preload-marker'),
      immutableHeaders,
      body: await response.text(),
      clonedBody: await clonedResponse.text(),
    }), { headers: { 'Content-Type': 'application/json' } });
  }));
});"#,
                &BTreeMap::new(),
            )
            .expect("fetch listener installs");
    assert!(
        runtime
            .has_service_worker_fetch_listener()
            .expect("listener presence is observable by the host")
    );

    let request_id = 73;
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
        "referrer": "https://source.test/",
        "referrerUrl": "https://source.test/path/page?token=secret#fragment",
        "referrerPolicy": "strict-origin-when-cross-origin",
        "navigationPreloadRequestId": request_id,
    });
    let dispatched = runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("fetch event starts while preload promise is pending");
    assert!(dispatched.top_level_await_pending);

    let response = serde_json::json!({
        "url": "https://preload.test/final-page",
        "status": 200,
        "statusText": "OK",
        "headers": [["x-preload-marker", "preserved"]],
        "contentType": "text/plain",
        "bodyBase64": base64::engine::general_purpose::STANDARD.encode("preloaded body"),
        "bodyNull": false,
        "redirected": true,
        "opaque": false,
        "opaqueRedirect": false,
    });
    let resolved = runtime
        .resolve_service_worker_navigation_preload(
            worker_id, worker_url, request_id, &response, false,
        )
        .expect("host response resolves FetchEvent.preloadResponse");
    assert!(resolved.commands.is_empty());
    assert_eq!(
        runtime
            .take_top_level_await_result()
            .expect("ServiceWorker response promise settles"),
        Some(serde_json::json!({
            "handled": true,
            "response": {
                "url": "",
                "status": 200,
                "statusText": "",
                "headers": [["content-type", "application/json"]],
                "contentType": "application/json",
                "bodyBase64": base64::engine::general_purpose::STANDARD.encode(r#"{"status":200,"url":"https://preload.test/final-page","redirected":true,"type":"basic","mode":"navigate","clonedMode":"navigate","referrer":"https://source.test/","referrerPolicy":"strict-origin-when-cross-origin","constructorRejectsNavigationMode":true,"clonedRequestInitMode":"same-origin","clonedExplicitNavigationModeRejects":true,"marker":"preserved","immutableHeaders":true,"body":"preloaded body","clonedBody":"preloaded body"}"#),
                "bodyNull": false,
                "redirected": false,
            }
        }))
    );
    assert!(
        runtime
            .resolve_service_worker_navigation_preload(
                worker_id, worker_url, request_id, &response, false,
            )
            .is_err(),
        "the host rejects stale navigation-preload request ids"
    );
}

#[test]
fn service_worker_navigation_preload_does_not_delay_independent_response() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("sw-navigation-preload-independent-response")
            .expect("ServiceWorker runtime is valid");
    let worker_id = 45;
    let worker_url = "https://preload.test/sw.js";
    runtime
            .evaluate_service_worker_source(
                worker_id,
                worker_url,
                None,
                "self.addEventListener('fetch', event => event.respondWith(new Response('independent response')));",
                &BTreeMap::new(),
            )
            .expect("fetch listener installs");
    let request_id = 75;
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
        "navigationPreloadRequestId": request_id,
    });

    let evaluation = runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("FetchEvent dispatch returns the independent response");

    assert!(
        !evaluation.top_level_await_pending,
        "an independent response must not await the pending preload promise"
    );
    assert_eq!(evaluation.value["handled"], true);
    let encoded = evaluation.value["response"]["bodyBase64"]
        .as_str()
        .expect("independent response body is base64 encoded");
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("independent response body uses valid base64");
    assert_eq!(body, b"independent response");
}

#[test]
fn service_worker_fetch_response_does_not_wait_for_wait_until_lifetime() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("sw-fetch-response-independent-of-wait-until")
            .expect("ServiceWorker runtime is valid");
    let worker_id = 46;
    let worker_url = "https://preload.test/sw.js";
    runtime
        .evaluate_service_worker_source(
            worker_id,
            worker_url,
            None,
            r#"self.addEventListener('fetch', event => {
  event.waitUntil(Promise.resolve().then(() => {
    event.waitUntil(new Promise(resolve => {
      globalThis.__resolveFetchWaitUntil = resolve;
    }));
  }));
  event.respondWith(new Response('response is ready'));
});"#,
            &BTreeMap::new(),
        )
        .expect("fetch listener installs");
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
    });

    let evaluation = runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("FetchEvent response settles independently of waitUntil");

    assert!(
        !evaluation.top_level_await_pending,
        "a pending waitUntil promise must not hold the navigation response"
    );
    assert_eq!(evaluation.value["handled"], true);
    let encoded = evaluation.value["response"]["bodyBase64"]
        .as_str()
        .expect("response body is base64 encoded");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("response body uses valid base64"),
        b"response is ready"
    );

    let pending_lifetimes = runtime.context.with(|ctx| {
        ctx.eval::<usize, _>("globalThis.__glassServiceWorkerFetchLifetimes.size")
            .expect("pending lifetime promise remains retained by the worker")
    });
    assert_eq!(pending_lifetimes, 1);

    let remaining_lifetimes = runtime.context.with(|ctx| {
        ctx.eval::<(), _>("globalThis.__resolveFetchWaitUntil();")
            .expect("test can settle the independent lifetime promise");
        for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
            if !ctx.execute_pending_job() {
                break;
            }
        }
        ctx.eval::<usize, _>("globalThis.__glassServiceWorkerFetchLifetimes.size")
            .expect("settled lifetime promise is retired")
    });
    assert_eq!(remaining_lifetimes, 0);
}

#[test]
fn service_worker_fetch_respond_with_stops_later_listeners() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("sw-fetch-respond-with-stops-listeners")
            .expect("ServiceWorker runtime is valid");
    let worker_id = 47;
    let worker_url = "https://preload.test/sw.js";
    runtime
        .evaluate_service_worker_source(
            worker_id,
            worker_url,
            None,
            r#"self.addEventListener('fetch', event => {
  event.respondWith(new Response('first response'));
});
self.addEventListener('fetch', () => {
  globalThis.__laterFetchListenerRan = true;
});"#,
            &BTreeMap::new(),
        )
        .expect("fetch listeners install");
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
    });

    let evaluation = runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("FetchEvent settles after its first respondWith");
    assert_eq!(evaluation.value["handled"], true);
    let encoded = evaluation.value["response"]["bodyBase64"]
        .as_str()
        .expect("response body is base64 encoded");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("response body uses valid base64"),
        b"first response"
    );
    let later_listener_ran = runtime.context.with(|ctx| {
        ctx.eval::<bool, _>("globalThis.__laterFetchListenerRan === true")
            .expect("listener state can be inspected")
    });
    assert!(
        !later_listener_ran,
        "respondWith stops invocation of later fetch listeners"
    );
}

#[test]
fn service_worker_fetch_onfetch_replacement_preserves_listener_order() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id(
        "sw-fetch-onfetch-replacement-preserves-order",
    )
    .expect("ServiceWorker runtime is valid");
    let worker_id = 48;
    let worker_url = "https://preload.test/sw.js";
    runtime
        .evaluate_service_worker_source(
            worker_id,
            worker_url,
            None,
            r#"globalThis.__fetchOrder = [];
self.addEventListener('fetch', () => globalThis.__fetchOrder.push('early'));
self.onfetch = () => globalThis.__fetchOrder.push('replaced');
self.addEventListener('fetch', () => globalThis.__fetchOrder.push('middle'));
self.onfetch = () => globalThis.__fetchOrder.push('onfetch');
self.addEventListener('fetch', event => {
  globalThis.__fetchOrder.push('respond');
  event.respondWith(new Response('ordered response'));
});
self.addEventListener('fetch', () => globalThis.__fetchOrder.push('late'));"#,
            &BTreeMap::new(),
        )
        .expect("fetch listeners install");
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
    });

    let evaluation = runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("FetchEvent handlers dispatch in their registration order");

    assert_eq!(evaluation.value["handled"], true);
    let body = base64::engine::general_purpose::STANDARD
        .decode(
            evaluation.value["response"]["bodyBase64"]
                .as_str()
                .expect("response body is base64 encoded"),
        )
        .expect("response body uses valid base64");
    assert_eq!(body, b"ordered response");
    let order = runtime.context.with(|ctx| {
        ctx.eval::<String, _>("globalThis.__fetchOrder.join(',')")
            .expect("FetchEvent registration order can be inspected")
    });
    assert_eq!(order, "early,onfetch,middle,respond");
}

#[test]
fn service_worker_fetch_onfetch_reactivation_appends_and_respond_with_stops_later() {
    let runtime =
        NativeJavaScriptRuntime::new_with_context_id("sw-fetch-onfetch-reactivation-appends")
            .expect("ServiceWorker runtime is valid");
    let worker_id = 49;
    let worker_url = "https://preload.test/sw.js";
    runtime
        .evaluate_service_worker_source(
            worker_id,
            worker_url,
            None,
            r#"globalThis.__fetchOrder = [];
self.addEventListener('fetch', () => globalThis.__fetchOrder.push('early'));
self.onfetch = () => globalThis.__fetchOrder.push('deactivated handler');
self.addEventListener('fetch', () => globalThis.__fetchOrder.push('before deactivation'));
self.onfetch = null;
self.addEventListener('fetch', () => globalThis.__fetchOrder.push('after deactivation'));
self.onfetch = event => {
  globalThis.__fetchOrder.push('reactivated handler');
  event.respondWith(new Response('reactivated response'));
};
self.addEventListener('fetch', () => globalThis.__fetchOrder.push('later listener'));"#,
            &BTreeMap::new(),
        )
        .expect("fetch listeners install");
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
    });

    let evaluation = runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("reactivated onfetch handler dispatches in its new position");

    assert_eq!(evaluation.value["handled"], true);
    let body = base64::engine::general_purpose::STANDARD
        .decode(
            evaluation.value["response"]["bodyBase64"]
                .as_str()
                .expect("response body is base64 encoded"),
        )
        .expect("response body uses valid base64");
    assert_eq!(body, b"reactivated response");
    let order = runtime.context.with(|ctx| {
        ctx.eval::<String, _>("globalThis.__fetchOrder.join(',')")
            .expect("FetchEvent registration order can be inspected")
    });
    assert_eq!(
        order,
        "early,before deactivation,after deactivation,reactivated handler"
    );
}

#[test]
fn service_worker_preload_response_is_undefined_when_not_started() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("sw-preload-not-started")
        .expect("ServiceWorker runtime is valid");
    let worker_id = 44;
    let worker_url = "https://preload.test/sw.js";
    runtime
            .evaluate_service_worker_source(
                worker_id,
                worker_url,
                None,
                "self.addEventListener('fetch', event => event.respondWith(event.preloadResponse.then(response => new Response(response === undefined ? 'undefined' : 'present'))));",
                &BTreeMap::new(),
            )
            .expect("fetch listener installs");
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
    });
    runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("fetch event dispatches without a preload request");
    let settled = runtime
        .take_top_level_await_result()
        .expect("ServiceWorker response promise settles")
        .expect("fetch handler responds without a preload");
    assert_eq!(settled["handled"], true);
    let encoded = settled["response"]["bodyBase64"]
        .as_str()
        .expect("response body is base64 encoded");
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("response body uses valid base64");
    assert_eq!(body, b"undefined");
}

#[test]
fn service_worker_navigation_preload_network_error_rejects_type_error() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("sw-navigation-preload-error")
        .expect("ServiceWorker runtime is valid");
    let worker_id = 43;
    let worker_url = "https://preload.test/sw.js";
    runtime
            .evaluate_service_worker_source(
                worker_id,
                worker_url,
                None,
                "self.addEventListener('fetch', event => event.respondWith(event.preloadResponse.then(() => new Response('unexpected'), error => new Response(error.name))));",
                &BTreeMap::new(),
            )
            .expect("fetch listener installs");
    let request_id = 74;
    let request = serde_json::json!({
        "url": "https://preload.test/page",
        "method": "GET",
        "headers": [],
        "bodyNull": true,
        "mode": "navigate",
        "redirect": "follow",
        "credentialsMode": "include",
        "destination": "document",
        "navigationPreloadRequestId": request_id,
    });
    let dispatched = runtime
        .evaluate_service_worker_fetch(worker_id, worker_url, &request, false)
        .expect("fetch event starts with a pending preload response");
    assert!(dispatched.top_level_await_pending);

    let rejected = runtime
        .resolve_service_worker_navigation_preload(
            worker_id,
            worker_url,
            request_id,
            &serde_json::json!({"error": "connection refused"}),
            false,
        )
        .expect("network failure rejects the pending preload promise");
    assert!(rejected.commands.is_empty());
    let settled = runtime
        .take_top_level_await_result()
        .expect("network-error FetchEvent response settles")
        .expect("fetch handler handles the network error");
    assert_eq!(settled["handled"], true);
    let encoded = settled["response"]["bodyBase64"]
        .as_str()
        .expect("typed network error response body is encoded");
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("network error response body uses valid base64");
    assert_eq!(body, b"TypeError");
}

#[test]
fn worker_fetch_uses_global_policy_without_changing_request_policy() {
    let mut runtime = NativeJavaScriptRuntime::new_with_context_id("worker-fetch-policy")
        .expect("Worker runtime is valid");
    runtime.worker_global_referrer_policy = Some(NativeFetchReferrerPolicy::Origin);
    let evaluation = runtime
            .evaluate_worker_prepared(
                1,
                "https://worker.test/worker.js",
                "const inherited = new Request('https://target.test/inherited'); const override = new Request('https://target.test/override'); globalThis.__observedWorkerRequestPolicies = JSON.stringify({ inheritedPolicy: inherited.referrerPolicy, overridePolicy: override.referrerPolicy }); fetch(inherited); fetch(override, { referrerPolicy: 'no-referrer' });",
                &BTreeMap::new(),
            )
            .expect("Worker fetch commands are prepared");

    let public_policies = runtime
        .context
        .with(|ctx| ctx.eval::<String, _>("globalThis.__observedWorkerRequestPolicies"))
        .expect("public Worker request policies remain inspectable")
        .parse::<serde_json::Value>()
        .expect("public Worker request policies are JSON");
    assert_eq!(
        public_policies,
        serde_json::json!({
            "inheritedPolicy": "",
            "overridePolicy": "",
        })
    );
    let policies = evaluation
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeScriptCommand::Fetch {
                referrer_policy, ..
            } => referrer_policy.as_deref(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(policies, ["origin", "no-referrer"]);
}

#[test]
fn service_worker_fetch_and_async_xhr_use_global_policy_without_changing_request_policy() {
    let mut runtime = NativeJavaScriptRuntime::new_with_context_id("service-worker-policy")
        .expect("ServiceWorker runtime is valid");
    runtime.set_worker_global_referrer_policy(NativeFetchReferrerPolicy::Origin);
    let evaluation = runtime
            .evaluate_service_worker_source(
                7,
                "https://worker.test/service-worker.js",
                None,
                "const inherited = new Request('/inherited'); const explicit = new Request('/explicit', { referrerPolicy: 'same-origin' }); globalThis.__observedServiceWorkerRequestPolicies = JSON.stringify({ inherited: inherited.referrerPolicy, explicit: explicit.referrerPolicy }); fetch(inherited); fetch('/init-override', { referrerPolicy: 'no-referrer' }); fetch(explicit); const xhr = new XMLHttpRequest(); xhr.open('GET', '/xhr'); xhr.send();",
                &BTreeMap::new(),
            )
            .expect("ServiceWorker Fetch and asynchronous XHR commands are prepared");

    let public_policies = runtime
        .context
        .with(|ctx| ctx.eval::<String, _>("globalThis.__observedServiceWorkerRequestPolicies"))
        .expect("public ServiceWorker Request policies remain inspectable")
        .parse::<serde_json::Value>()
        .expect("public ServiceWorker Request policies are JSON");
    assert_eq!(
        public_policies,
        serde_json::json!({"inherited": "", "explicit": "same-origin"})
    );
    let policies = evaluation
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeScriptCommand::Fetch {
                referrer_policy, ..
            } => referrer_policy.as_deref(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(policies, ["origin", "no-referrer", "same-origin", "origin"]);
}

#[test]
fn static_module_prefetch_preserves_json_import_attributes() {
    let requests = static_module_requests(
        r#"
                import "./side-effect.json" with { type: "json" };
                import data from "./data.json" /* trivia */ with { "type": 'json' };
                export { default as config } from "./config.json" with { type: "json" };
                import { value } from "./code.js";
                import("./dynamic.json", { with: { type: "json" } });
            "#,
    )
    .unwrap();

    assert_eq!(
        requests
            .into_iter()
            .map(|request| (request.specifier, request.module_type))
            .collect::<Vec<_>>(),
        [
            ("./side-effect.json".into(), NativeModuleResourceType::Json),
            ("./data.json".into(), NativeModuleResourceType::Json),
            ("./config.json".into(), NativeModuleResourceType::Json),
            ("./code.js".into(), NativeModuleResourceType::JavaScript),
        ]
    );
}

#[test]
fn static_module_prefetch_fails_closed_for_unsupported_import_attributes() {
    let requests = static_module_requests(
        r#"
                import sheet from "./sheet.css" with { type: "css" };
                export { default as payload } from "./payload.json" with { encoding: "utf-8" };
            "#,
    )
    .unwrap();

    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| request.module_type == NativeModuleResourceType::Unsupported)
    );
}

#[test]
fn json_module_loader_captures_the_json_parser_and_exposes_only_default() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("json-module-test")
        .expect("native JavaScript runtime must construct");
    let module_url = "https://json-module.test/data.json";
    let entry_url = "https://json-module.test/app.js";
    runtime.set_module_sources(BTreeMap::from([(
        native_module_loader_name(module_url, NativeModuleResourceType::Json),
        r#"{"value":"first","value":"json","__proto__":{"polluted":true}}"#.to_owned(),
    )]));
    runtime.set_module_base_urls(BTreeMap::from([(
        entry_url.to_owned(),
        entry_url.to_owned(),
    )]));

    runtime.context.with(|ctx| {
            ctx.eval::<(), _>("JSON.parse = () => { throw new Error('mutable parser used'); };")
                .expect("the page may mutate its public JSON parser");
            Module::evaluate(
                ctx.clone(),
                entry_url,
                r#"
                    import data from "./data.json" with { type: "json" };
                    import * as dataModule from "./data.json" with { type: "json" };
                    globalThis.jsonModuleResult = [
                      Object.keys(dataModule).join(","),
                      data.value,
                      Object.prototype.hasOwnProperty.call(data, "__proto__"),
                      Object.getPrototypeOf(data) === Object.prototype,
                      data.__proto__.polluted,
                    ];
                "#,
            )
            .expect("JSON module must parse and link")
            .finish::<()>()
            .expect("JSON module evaluation must complete");
            assert_eq!(
                ctx.eval::<String, _>("JSON.stringify(globalThis.jsonModuleResult)")
                    .expect("JSON module result must serialize"),
                r#"["default","json",true,true,true]"#
            );

            let invalid_url = "https://json-module.test/invalid.json";
            let invalid_entry = "https://json-module.test/invalid-app.js";
            runtime.set_module_sources(BTreeMap::from([(
                native_module_loader_name(invalid_url, NativeModuleResourceType::Json),
                "not valid JSON".to_owned(),
            )]));
            runtime.set_module_base_urls(BTreeMap::from([(
                invalid_entry.to_owned(),
                invalid_entry.to_owned(),
            )]));
            ctx.eval::<(), _>("globalThis.invalidJsonEvaluated = false;")
                .expect("invalid JSON module result flag must initialize");
            let invalid_json = Module::evaluate(
                ctx.clone(),
                invalid_entry,
                r#"import value from "./invalid.json" with { type: "json" }; globalThis.invalidJsonEvaluated = true;"#,
            )
            .and_then(|module| module.finish::<()>());
            assert!(invalid_json.is_err(), "invalid JSON must reject module evaluation");
            assert!(!ctx
                .eval::<bool, _>("globalThis.invalidJsonEvaluated")
                .expect("invalid JSON execution state must be readable"));
        });
}

#[test]
fn same_static_specifier_can_resolve_as_json_and_javascript() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("json-module-identity-test")
        .expect("native JavaScript runtime must construct");
    let module_url = "https://json-module.test/mapped/shared.js";
    let marker_prefixed_js_url = "https://json-module.test/ordinary.js";
    let dependency_url = "https://json-module.test/dependency.js";
    let nested_json_url = "https://json-module.test/nested.json";
    let entry_url = "https://json-module.test/identity-app.js";
    runtime.set_module_sources(BTreeMap::from([
            (
                native_module_loader_name(module_url, NativeModuleResourceType::Json),
                r#"{"kind":"json"}"#.to_owned(),
            ),
            (
                module_url.to_owned(),
                "export const kind = 'javascript';".to_owned(),
            ),
            (
                marker_prefixed_js_url.to_owned(),
                "export const kind = 'marker-prefixed-javascript';".to_owned(),
            ),
            (
                dependency_url.to_owned(),
                "import nestedValue from './nested.json' with { type: 'json' }; export const nestedKind = nestedValue.kind;".to_owned(),
            ),
            (
                native_module_loader_name(nested_json_url, NativeModuleResourceType::Json),
                r#"{"kind":"nested-json"}"#.to_owned(),
            ),
        ]));
    runtime.set_module_base_urls(BTreeMap::from([
        (entry_url.to_owned(), entry_url.to_owned()),
        (dependency_url.to_owned(), dependency_url.to_owned()),
    ]));
    runtime.set_module_import_map(
            NativeModuleImportMap::parse(
                r#"{"imports":{"./shared.js":"/mapped/shared.js","glass-internal-json-request:./ordinary.js":"/ordinary.js","./dependency.js":"/dependency.js","./nested.json":"/nested.json"}}"#,
                entry_url,
            )
            .expect("the test import map must parse"),
        );

    let source = rewrite_static_json_module_specifiers(
            r#"
                import jsonValue from "./shared.js" with { type: "json" };
                import { kind as javascriptKind } from "./shared.js";
                import { kind as markerPrefixedJavascriptKind } from "glass-internal-json-request:./ordinary.js";
                import { nestedKind } from "./dependency.js";
                globalThis.sameSpecifierModuleTypes = [jsonValue.kind, javascriptKind, markerPrefixedJavascriptKind, nestedKind];
            "#,
        )
        .expect("the static JSON request must be disambiguated");
    assert!(source.contains("glass-internal-json-request:./shared.js"));

    runtime.context.with(|ctx| {
        Module::evaluate(ctx.clone(), entry_url, source)
            .expect("JSON and JavaScript requests must link independently")
            .finish::<()>()
            .expect("both module types must evaluate");
        assert_eq!(
            ctx.eval::<String, _>("JSON.stringify(globalThis.sameSpecifierModuleTypes)")
                .expect("module-type result must serialize"),
            r#"["json","javascript","marker-prefixed-javascript","nested-json"]"#
        );
    });
}

#[test]
fn service_worker_dynamic_import_rewrite_rejects_literal_and_computed_calls() {
    let source = r#"
            import('./literal.js', { with: { type: 'json' } });
            import(runtimeSpecifier, options);
            import.meta.url;
            worker.import(runtimeSpecifier);
            const text = "import('./string.js')";
        "#;

    let (rewritten, calls) = rewrite_dynamic_imports_as_rejected_with_count(source).unwrap();

    assert_eq!(calls, 2);
    assert!(rewritten.contains(
        "globalThis.__glassRejectDynamicImport('./literal.js', { with: { type: 'json' } })"
    ));
    assert!(rewritten.contains("globalThis.__glassRejectDynamicImport(runtimeSpecifier, options)"));
    assert!(rewritten.contains("import.meta.url"));
    assert!(rewritten.contains("worker.import(runtimeSpecifier)"));
    assert!(rewritten.contains("import('./string.js')"));
}

#[test]
fn service_worker_dynamic_import_rewrite_enforces_script_call_limit() {
    let source = "import(runtimeSpecifier);".repeat(MAX_NATIVE_MODULE_IMPORTS + 1);

    assert!(rewrite_dynamic_imports_as_rejected(&source).is_err());
}

#[test]
fn runtime_dynamic_import_rewrite_preserves_literal_and_computed_import_calls() {
    let source = r#"
            import './static-declaration.js';
            import('./literal.json', { with: { type: 'json' } });
            const text = "import(getName())";
            const pattern = /import\\(getName\\)/;
            module.import(getName());
            module?.import(getOptionalName());
            module /* member trivia */ . /* token trivia */ import(getCommentedName());
            const computed = import(/* first arg */ getName(), options);
            const template = `${import(getNestedName())}`;
            const methods = { import(value) {} };
        "#;

    let rewritten = rewrite_runtime_dynamic_module_imports(source, "import.meta.url")
        .expect("bounded import-call rewriting must fit the script limit");
    assert_eq!(
        rewritten
            .matches("globalThis.__glassDynamicImport(")
            .count(),
        3,
        "all three actual ImportCalls should be rewritten: {rewritten}"
    );
    assert!(rewritten.contains(
            "globalThis.__glassDynamicImport(import.meta.url,null,'./literal.json', { with: { type: 'json' } })"
        ));
    assert!(rewritten.contains(
        "globalThis.__glassDynamicImport(import.meta.url,null,/* first arg */ getName(), options)"
    ));
    assert!(rewritten.contains("__glassDynamicImport(import.meta.url,null,getNestedName())"));

    let inherited_policy = rewrite_runtime_dynamic_module_imports_with_policy(
        "import('./child.js')",
        "import.meta.url",
        "'origin'",
    )
    .expect("bounded dynamic imports must carry their fetch policy");
    assert_eq!(
        inherited_policy,
        "globalThis.__glassDynamicImport(import.meta.url,'origin','./child.js')"
    );
    assert!(rewritten.contains("import './static-declaration.js'"));
    assert!(rewritten.contains("module.import(getName())"));
    assert!(rewritten.contains("module?.import(getOptionalName())"));
    assert!(rewritten.contains("import(value) {}"));
}

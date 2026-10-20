use super::*;

#[test]
fn service_worker_clients_accept_maximum_url_without_bootstrap_source_coupling() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("service-worker-client-test")
        .expect("native JavaScript runtime must construct");
    let worker_url = "https://example.test/sw.js";
    let prefix = "https://example.test/";
    let client_url = format!(
        "{prefix}{}",
        "a".repeat(MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES - prefix.len())
    );
    assert_eq!(client_url.len(), MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES);
    runtime.set_service_worker_clients(vec![serde_json::json!({
        "clientId": "client-one",
        "clientUrl": client_url,
        "clientType": "window",
        "frameType": "top-level",
        "visibilityState": "visible",
        "focused": true,
        "controlled": true,
    })]);

    runtime
            .evaluate_service_worker_source(
                1,
                worker_url,
                None,
                "globalThis.__seenClientUrl = null; globalThis.clients.matchAll({ includeUncontrolled: true }).then(clients => { globalThis.__seenClientUrl = clients[0].url; }); true",
                &BTreeMap::new(),
            )
            .expect("service worker client projection must dispatch");
    let evaluation = runtime
        .evaluate_service_worker_source(
            1,
            worker_url,
            None,
            "globalThis.__seenClientUrl",
            &BTreeMap::new(),
        )
        .expect("service worker client projection must remain observable");
    assert_eq!(evaluation.value, serde_json::json!(client_url));
}

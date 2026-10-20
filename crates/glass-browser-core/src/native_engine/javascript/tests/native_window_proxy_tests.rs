use super::*;

#[test]
fn window_proxy_updates_accept_maximum_url_without_script_source_coupling() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("window-proxy-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let origin = NativeOrigin::Opaque;
    let viewport = Viewport::default();

    runtime
        .evaluate(
            "globalThis.popup = window.open('about:blank', '_blank'); true",
            &document,
            "about:blank",
            &origin,
            viewport,
        )
        .expect("WindowProxy must be created");

    let prefix = "fixture://large.test/#";
    let href = format!(
        "{prefix}{}",
        "a".repeat(MAX_NATIVE_SCRIPT_BYTES - prefix.len())
    );
    assert_eq!(href.len(), MAX_NATIVE_SCRIPT_BYTES);
    runtime
        .sync_window_proxies(&[NativeWindowProxyUpdate {
            cache_key: "\\u0000glass-window-1".into(),
            target_context_id: "native-target-1".into(),
            href,
            name: "_blank".into(),
            closed: false,
        }])
        .expect("maximum-size WindowProxy metadata must be queued");

    let evaluation = runtime
        .evaluate(
            "[popup.location.href.length, popup.location.href.endsWith('a'), popup.closed]",
            &document,
            "about:blank",
            &origin,
            viewport,
        )
        .expect("queued WindowProxy metadata must dispatch");
    assert_eq!(
        evaluation.value,
        serde_json::json!([MAX_NATIVE_SCRIPT_BYTES, true, false])
    );
}

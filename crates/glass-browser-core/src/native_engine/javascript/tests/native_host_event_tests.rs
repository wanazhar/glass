use super::*;

#[test]
fn host_key_event_batch_dispatches_quoted_metadata_without_source_interpolation() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("host-event-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let origin = NativeOrigin::Opaque;
    let viewport = Viewport::default();
    let key = "A\"'\\;";

    runtime
            .evaluate(
                r#"globalThis.addEventListener('keydown', (event) => {
                  globalThis.__seenHostKey = [event.key, event.code, event.altKey, event.ctrlKey, event.metaKey, event.shiftKey];
                  event.preventDefault();
                }); true"#,
                &document,
                "about:blank",
                &origin,
                viewport,
            )
            .expect("host key listener must install");

    let events = host_key_event_batch_with_modifiers(u32::MAX, NativeEventKind::KeyDown, key, 7)
        .expect("host key metadata must validate")
        .expect("host key metadata must not be empty");
    let evaluation = runtime
        .evaluate_with_host_events(&events, &document, "about:blank", &origin, viewport)
        .expect("structured host key event must dispatch");

    assert_eq!(evaluation.value, serde_json::json!([false]));
    let seen = runtime
        .evaluate(
            "globalThis.__seenHostKey",
            &document,
            "about:blank",
            &origin,
            viewport,
        )
        .expect("host key metadata must remain observable");
    assert_eq!(
        seen.value,
        serde_json::json!([key, key, true, true, true, false])
    );
}

#[test]
fn host_click_event_batch_exposes_all_modifier_flags() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("host-click-event-test")
        .expect("native JavaScript runtime must construct");
    let document = NativeDocument::empty();
    let origin = NativeOrigin::Opaque;
    let viewport = Viewport::default();
    runtime
            .evaluate(
                "globalThis.addEventListener('click', event => { globalThis.__seenClickModifiers = [event.altKey, event.ctrlKey, event.metaKey, event.shiftKey]; event.preventDefault(); }); true",
                &document,
                "about:blank",
                &origin,
                viewport,
            )
            .expect("host click listener must install");

    let events = host_click_event_batch_with_modifiers(u32::MAX, 15)
        .expect("host click modifier metadata must validate");
    let evaluation = runtime
        .evaluate_with_host_events(&events, &document, "about:blank", &origin, viewport)
        .expect("structured modifier click must dispatch");
    assert_eq!(evaluation.value, serde_json::json!([false]));
    let seen = runtime
        .evaluate(
            "globalThis.__seenClickModifiers",
            &document,
            "about:blank",
            &origin,
            viewport,
        )
        .expect("modifier flags must remain observable");
    assert_eq!(seen.value, serde_json::json!([true, true, true, true]));
}

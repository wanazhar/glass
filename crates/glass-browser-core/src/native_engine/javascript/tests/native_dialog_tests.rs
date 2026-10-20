use super::super::config::NativeEngineLimits;
use super::*;

#[test]
fn modal_dialog_host_preserves_argument_conversion_and_injected_results() {
    let runtime = NativeJavaScriptRuntime::new_with_context_id("dialog-host-test")
        .expect("native JavaScript runtime must construct");
    let opened = Arc::new(Mutex::new(Vec::new()));
    let opened_by_handler = Arc::clone(&opened);
    runtime
        .set_dialog_handler(Arc::new(move |dialog, url| {
            opened_by_handler
                .lock()
                .map_err(|_| "dialog test state was poisoned".to_owned())?
                .push((
                    dialog.dialog_type.clone(),
                    dialog.message.clone(),
                    dialog.default_value.clone(),
                    url,
                ));
            Ok(Some(NativeDialogResolution {
                accepted: true,
                prompt_value: (dialog.dialog_type == "prompt")
                    .then(|| format!("reply:{}", dialog.default_value.unwrap_or_default())),
            }))
        }))
        .expect("native dialog handler must install");
    let document = NativeDocument::parse("<body></body>", &NativeEngineLimits::default())
        .expect("dialog test document must parse");
    let evaluation = runtime
        .evaluate(
            r#"(() => {
                    alert();
                    alert(undefined);
                    alert(null);
                    return [
                        typeof globalThis.__glassNativeDialogCall,
                        confirm(undefined),
                        confirm(null),
                        prompt(undefined, undefined),
                        prompt(null, null),
                    ];
                })()"#,
            &document,
            "https://example.test/dialog",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("native modal calls must resume with injected decisions");

    assert_eq!(
        evaluation.value,
        serde_json::json!(["undefined", true, true, "reply:", "reply:null"])
    );
    assert_eq!(
        *opened.lock().unwrap(),
        vec![
            (
                "alert".into(),
                "".into(),
                None,
                "https://example.test/dialog".into()
            ),
            (
                "alert".into(),
                "undefined".into(),
                None,
                "https://example.test/dialog".into()
            ),
            (
                "alert".into(),
                "null".into(),
                None,
                "https://example.test/dialog".into()
            ),
            (
                "confirm".into(),
                "".into(),
                None,
                "https://example.test/dialog".into()
            ),
            (
                "confirm".into(),
                "null".into(),
                None,
                "https://example.test/dialog".into()
            ),
            (
                "prompt".into(),
                "".into(),
                Some("".into()),
                "https://example.test/dialog".into()
            ),
            (
                "prompt".into(),
                "null".into(),
                Some("null".into()),
                "https://example.test/dialog".into()
            ),
        ]
    );

    let repeated_evaluation = runtime
        .evaluate(
            "confirm('second evaluation')",
            &document,
            "https://example.test/dialog",
            &NativeOrigin::Opaque,
            Viewport::default(),
        )
        .expect("the native dialog bridge must be reinjected for later evaluations");
    assert_eq!(repeated_evaluation.value, serde_json::json!(true));
    assert_eq!(
        opened.lock().unwrap().last().map(|entry| entry.1.as_str()),
        Some("second evaluation")
    );
}

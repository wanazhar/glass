use super::*;

#[test]
fn native_runtime_session_uses_explicit_local_constructor() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let session =
        BrowserRuntimeSession::connect_native(NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3ERuntime%3C%2Ftitle%3E%3Cbutton%3ENative%20session%3C%2Fbutton%3E",
        ))
        .await
        .unwrap();
    assert_eq!(session.runtime(), BrowserRuntime::Native);
    assert_eq!(
        session.profile().identity.backend_id,
        NATIVE_ENGINE_BACKEND_ID
    );

    let evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(
        evidence.url,
        "data:text/html,%3Ctitle%3ERuntime%3C%2Ftitle%3E%3Cbutton%3ENative%20session%3C%2Fbutton%3E"
    );
    assert_eq!(evidence.title, "Runtime");
    assert_eq!(evidence.visible_text, "Native session");

    let nodes = session.native_semantic_nodes().unwrap();
    let button = nodes.iter().find(|node| node.tag_name == "button").unwrap();
    let serialized = serde_json::to_value(button).unwrap();
    assert_eq!(serialized["tagName"], "button");
    assert!(serialized.get("nodeId").is_some());
    let png = session.native_capture_png().unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    let script = session.script("1 + 1").await.unwrap();
    assert_eq!(script.value, serde_json::json!(2));
    let promise = session.script("await Promise.resolve(42)").await.unwrap();
    assert_eq!(promise.value, serde_json::json!(42));
    let host = session
        .script(
            "({ title: document.title, text: document.querySelector('button').innerText, tag: document.querySelector('button').tagName, width: window.innerWidth, origin: location.origin })",
        )
        .await
        .unwrap();
    assert_eq!(
        host.value,
        serde_json::json!({
            "title": "Runtime",
            "text": "Native session",
            "tag": "BUTTON",
            "width": 1280,
            "origin": "null",
        })
    );
    let persisted = session
        .script("globalThis.answer = (globalThis.answer || 0) + 1; globalThis.answer")
        .await
        .unwrap();
    assert_eq!(persisted.value, serde_json::json!(1));
    let persisted = session.script("globalThis.answer").await.unwrap();
    assert_eq!(persisted.value, serde_json::json!(1));
    let storage = session
        .script(
            "localStorage.setItem('theme', 'dark'); sessionStorage.setItem('tab', 'one'); ({ local: localStorage.getItem('theme'), session: sessionStorage.getItem('tab'), localLength: localStorage.length, sessionLength: sessionStorage.length })",
        )
        .await
        .unwrap();
    assert_eq!(
        storage.value,
        serde_json::json!({
            "local": "dark",
            "session": "one",
            "localLength": 1,
            "sessionLength": 1,
        })
    );
    let storage = session
        .script("({ local: localStorage.getItem('theme'), session: sessionStorage.getItem('tab'), localKey: localStorage.key(0), sessionKey: sessionStorage.key(0) })")
        .await
        .unwrap();
    assert_eq!(
        storage.value,
        serde_json::json!({
            "local": "dark",
            "session": "one",
            "localKey": "theme",
            "sessionKey": "tab",
        })
    );
    session
        .script("localStorage.clear(); sessionStorage.removeItem('tab'); true")
        .await
        .unwrap();
    let storage = session
        .script("[localStorage.length, sessionStorage.length]")
        .await
        .unwrap();
    assert_eq!(storage.value, serde_json::json!([0, 0]));
    let stored = session
        .storage(StorageRequest {
            context_id: "native-context".into(),
            scope: StorageScope::Session,
            operation: StorageOperation::Write {
                key: "answer".into(),
                value: "one".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(stored.entries.get("answer"), Some(&"one".to_owned()));
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_runtime_exposes_shared_semantic_session_contracts() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3EContracts%3C%2Ftitle%3E%3Cmain%3E%3Clabel%20for%3D%22name%22%3EName%3C%2Flabel%3E%3Cinput%20id%3D%22name%22%3E%3Cbutton%3ESave%3C%2Fbutton%3E%3C%2Fmain%3E",
        ),
    )
    .await
    .unwrap();

    let bootstrap = session.observe_bootstrap().await.unwrap();
    assert!(bootstrap.ready);
    assert!(bootstrap.complete);
    assert_eq!(bootstrap.context_id, 0);
    assert_eq!(bootstrap.page.title, "Contracts");

    let inspected = session.inspect_page().await.unwrap();
    assert_eq!(inspected.page.title, "Contracts");
    assert!(inspected.revision > 0);

    let observation = session
        .native_semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let policy = glass_browser::browser::policy::BrowserPolicy::development(
        std::env::current_dir().unwrap(),
    )
    .unwrap();
    let extracted = session
        .native_extract_structured(
            &StructuredExtractionRequest {
                fields: vec![ExtractionField {
                    name: "title".into(),
                    path: "$.page.title".into(),
                    kind: ExtractionKind::String,
                }],
                region_id: None,
                start_index: 0,
                continuation: None,
                max_items: 16,
                max_bytes: 16 * 1024,
            },
            &policy,
        )
        .await
        .unwrap();
    assert_eq!(extracted.records[0]["title"], "Contracts");

    let intent = SemanticIntentRequest {
        schema_version: 1,
        intent: "save".into(),
        action: SemanticIntentAction::Click,
        scope: Default::default(),
        constraints: IntentConstraints {
            role: Some("button".into()),
            name: Some("Save".into()),
            ..Default::default()
        },
        resolution_policy: SemanticResolutionPolicy::RequireExact,
        expected_revision: Some(observation.revision),
    };
    let resolved = session.native_resolve_intent(&intent).await.unwrap();
    assert_eq!(resolved.candidates.len(), 1);
    let execution = session
        .native_execute_intent(&SemanticIntentExecutionRequest {
            request: intent,
            candidate_id: resolved.candidates[0].id.clone(),
            value: None,
        })
        .await
        .unwrap();
    assert_eq!(
        execution.status,
        glass_browser::browser::session::SemanticIntentExecutionStatus::Executed
    );

    let delta = session.native_observe_delta().await.unwrap();
    assert!(delta.to_revision >= delta.from_revision);
    assert!(!delta.mutation_summary.url_changed);

    let checkpoint = session
        .native_export_checkpoint("contracts", &policy)
        .await
        .unwrap();
    assert!(!checkpoint.attach_mode);
    assert_eq!(
        checkpoint.topology.target_id.as_deref(),
        Some("native-context")
    );
    session.native_import_checkpoint(&checkpoint).await.unwrap();

    let diagnostics = session
        .native_diagnostics(Duration::from_millis(1))
        .await
        .unwrap();
    assert_eq!(diagnostics.target_id, "native-context");
    assert!(diagnostics.lifecycle.browser_ready);
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_runtime_reconciles_references_and_applies_environment_overrides() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let session =
        BrowserRuntimeSession::connect_native(NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3EParity%3C%2Ftitle%3E%3Cbutton%3ESave%3C%2Fbutton%3E",
        ))
        .await
        .unwrap();
    let observation = session
        .native_semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let button = observation
        .regions
        .iter()
        .flat_map(|region| region.targets.iter())
        .find(|target| target.role == "button")
        .unwrap();
    let reconciled = session
        .native_reconcile_references(
            observation.revision,
            std::slice::from_ref(&button.reference),
            &Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(reconciled.preserved, 1);
    assert_eq!(reconciled.relocated, 0);
    assert!(matches!(
        reconciled.mappings.as_slice(),
        [glass_browser::browser::session::ReferenceMapping::Preserved { .. }]
    ));

    session
        .native_set_user_agent(
            Some("GlassTest/1.0"),
            Some("fr-FR,fr;q=0.8"),
            Some("GlassTestOS"),
        )
        .await
        .unwrap();
    let identity = session
        .script("({ ua: navigator.userAgent, language: navigator.language, platform: navigator.platform })")
        .await
        .unwrap();
    assert_eq!(
        identity.value,
        serde_json::json!({
            "ua": "GlassTest/1.0",
            "language": "fr-FR",
            "platform": "GlassTestOS",
        })
    );

    session
        .native_set_geolocation(Some(&glass_browser::browser::session::GeoLocation {
            latitude: 51.5,
            longitude: -0.1,
            accuracy: Some(5.0),
        }))
        .await
        .unwrap();
    let location = session
        .script(
            "(() => { let value = null; navigator.geolocation.getCurrentPosition(position => value = position.coords); return value; })()",
        )
        .await
        .unwrap();
    assert_eq!(location.value["latitude"], 51.5);
    assert_eq!(location.value["longitude"], -0.1);

    session
        .native_set_timezone(Some("America/New_York"))
        .await
        .unwrap();
    assert_eq!(
        session
            .script("globalThis.__glassTimezoneId")
            .await
            .unwrap()
            .value,
        "America/New_York"
    );
    session.native_set_cpu_throttling(Some(2.0)).await.unwrap();
    session.native_set_cpu_throttling(None).await.unwrap();
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_runtime_supports_form_pdf_clipboard_and_consent_surfaces() {
    std::thread::Builder::new()
        .name("native-form-pdf-recovery-test".to_owned())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("native test runtime should build");
            runtime
                .block_on(native_runtime_supports_form_pdf_clipboard_and_consent_surfaces_inner())
        })
        .expect("native form/PDF test thread should start")
        .join()
        .expect("native form/PDF test thread should finish");
}

#[test]
fn native_runtime_extracts_validated_live_web_ir() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3ENative%20IR%3C%2Ftitle%3E%3Cmain%3E%3Ch1%3EWelcome%3C%2Fh1%3E%3Clabel%20for%3D%22name%22%3EName%3C%2Flabel%3E%3Cinput%20id%3D%22name%22%20type%3D%22text%22%20required%3E%3Cbutton%3ESave%3C%2Fbutton%3E%3C%2Fmain%3E",
        ),
    )
    .await
    .unwrap();

    let ir = session
        .native_extract_web_ir(&ExtractionRequest::default_document())
        .await
        .unwrap();
    ir.validate().unwrap();
    assert_eq!(ir.document.title.as_deref(), Some("Native IR"));
    assert!(
        ir.entities
            .iter()
            .any(|entity| entity.kind == WebIrEntityKind::Page)
    );
    assert!(
        ir.entities
            .iter()
            .any(|entity| entity.kind == WebIrEntityKind::Region)
    );
    assert!(
        ir.entities
            .iter()
            .any(|entity| entity.kind == WebIrEntityKind::Field)
    );
    assert!(
        ir.entities
            .iter()
            .any(|entity| entity.kind == WebIrEntityKind::Action)
    );
    assert!(ir.limits.missing_sources.contains(&EvidenceSource::Dom));
    assert!(
        ir.entity_details
            .values()
            .any(|details| { details.supported_actions.contains(&WebIrAction::Type) })
    );
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_runtime_executes_revision_guarded_form_tasks() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Cform%3E%3Clabel%20for%3D%22email%22%3EEmail%3C%2Flabel%3E%3Cinput%20id%3D%22email%22%20type%3D%22email%22%3E%3Clabel%3E%3Cinput%20id%3D%22remember%22%20type%3D%22checkbox%22%3ERemember%3C%2Flabel%3E%3C%2Fform%3E",
        ),
    )
    .await
    .unwrap();
    let observation = session.native_observe().await.unwrap();
    let mut inputs = BTreeMap::new();
    inputs.insert("Email".into(), "native@example.com".into());
    inputs.insert("Remember".into(), "true".into());
    let task = GlassTask {
        schema_version: 1,
        task: TaskKind::FormFill,
        scope: TaskScope {
            region_name: Some("Main content".into()),
            ..Default::default()
        },
        inputs,
        limits: TaskLimits::default(),
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: TaskRevisionPolicy::Exact,
        postconditions: Vec::new(),
    };
    let result = session
        .native_execute_task(&task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(result.status, "succeeded");
    let form = result
        .form
        .expect("native task should return form evidence");
    assert_eq!(form.filled, 2);
    assert_eq!(form.total, 2);
    assert!(form.fields.iter().all(|field| field.success));
    let values = session
        .script(
            "({ email: document.getElementById('email').value, remember: document.getElementById('remember').checked })",
        )
        .await
        .unwrap();
    assert_eq!(
        values.value,
        serde_json::json!({"email": "native@example.com", "remember": true})
    );

    let field_observation = session.native_observe().await.unwrap();
    let mut field_inputs = BTreeMap::new();
    field_inputs.insert("field".into(), "Email".into());
    let field_task = GlassTask {
        schema_version: 1,
        task: TaskKind::FieldRead,
        scope: TaskScope {
            region_name: Some("Main content".into()),
            ..Default::default()
        },
        inputs: field_inputs,
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: TaskRevisionPolicy::Exact,
        postconditions: Vec::new(),
    };
    let field_result = session
        .native_execute_task(&field_task, field_observation.revision, false)
        .await
        .unwrap();
    assert_eq!(field_result.status, "succeeded");
    assert_eq!(
        field_result.extraction.unwrap().records[0]["value"],
        "native@example.com"
    );

    let extract_observation = session.native_observe().await.unwrap();
    let extract_task = GlassTask {
        schema_version: 1,
        task: TaskKind::RegionExtract,
        scope: TaskScope {
            region_name: Some("Main content".into()),
            ..Default::default()
        },
        inputs: BTreeMap::new(),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: TaskRevisionPolicy::Exact,
        postconditions: Vec::new(),
    };
    let extract_result = session
        .native_execute_task(&extract_task, extract_observation.revision, false)
        .await
        .unwrap();
    assert_eq!(extract_result.status, "succeeded");
    assert_eq!(extract_result.extraction.unwrap().records.len(), 1);
    session.close().await.unwrap();
        });
    });
}

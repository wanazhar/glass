#![cfg(feature = "native-engine")]

use glass_browser::browser::native_backend::NATIVE_ENGINE_BACKEND_ID;
use glass_browser::browser::native_engine::{
    NativeAction, NativeColor, NativeDisplayCommand, NativeDocument, NativeEngine,
    NativeEngineConfig, NativeEngineError, NativeEngineLimits, NativeEventKind,
    NativeLifecycleState, NativeRect, NativeSurface, Viewport,
};
use glass_browser::browser_backend::{
    ActionRequest, BROWSER_BACKEND_SCHEMA_VERSION, BackendSelectionRequest,
    BrowserBackendDispatcher, BrowserCapability, CertificationLevel, EffectsRequest, EvidenceLevel,
    EvidenceRequest, NavigationRequest, ScriptRequest, SemanticAction, SupportLevel,
};
use glass_browser::{BackendFactory, BrowserRuntime, BrowserRuntimeSession, NativeEngineBackend};

#[tokio::test]
async fn native_runtime_session_uses_explicit_local_constructor() {
    let session =
        BrowserRuntimeSession::connect_native(NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3ERuntime%3C%2Ftitle%3E%3Cp%3ENative%20session%3C%2Fp%3E",
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
        "data:text/html,%3Ctitle%3ERuntime%3C%2Ftitle%3E%3Cp%3ENative%20session%3C%2Fp%3E"
    );
    assert_eq!(evidence.title, "Runtime");
    assert_eq!(evidence.visible_text, "Native session");

    let script_error = session.script("1 + 1").await.unwrap_err().to_string();
    assert!(script_error.contains("capability"));
    session.close().await.unwrap();
}

#[test]
fn native_layout_is_deterministic_and_uses_bounded_pixel_dimensions() {
    let document = NativeDocument::parse(
        "<style>button { width: 120px; height: 32px; }</style><main><button id='first'>First</button><button id='second' style='display:block;width:140px'>Second</button><p id='below'>Below</p></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let first = document.resolve_target("id=first").unwrap();
    let second = document.resolve_target("id=second").unwrap();
    let below = document.resolve_target("id=below").unwrap();
    let expected_first = NativeRect {
        x: 0,
        y: 0,
        width: 120,
        height: 32,
    };
    let expected_second = NativeRect {
        x: 0,
        y: 32,
        width: 140,
        height: 32,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.revision, 1);
    assert_eq!(layout.box_for(first), Some(expected_first));
    assert_eq!(layout.box_for(second), Some(expected_second));
    assert_eq!(
        layout.box_for(below),
        Some(NativeRect {
            x: 0,
            y: 64,
            width: 320,
            height: 20,
        })
    );
    assert_eq!(layout, document.layout(viewport).unwrap());
}

#[test]
fn native_layout_excludes_hidden_boxes_and_hit_testing_is_viewport_bound() {
    let document = NativeDocument::parse(
        "<style>.gone { display:none } .also-gone { visibility:hidden }</style><main><button class='gone' id='hidden'>Hidden</button><button class='also-gone' id='also-hidden'>Also hidden</button><button id='shown'>Shown</button></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let layout = document.layout(viewport).unwrap();
    assert_eq!(
        layout.box_for(document.resolve_target("id=hidden").unwrap()),
        None
    );
    assert_eq!(
        layout.box_for(document.resolve_target("id=also-hidden").unwrap()),
        None
    );
    let shown = document.resolve_target("id=shown").unwrap();
    let shown_rect = layout.box_for(shown).unwrap();
    assert_eq!(
        layout
            .hit_test(i64::from(shown_rect.x), i64::from(shown_rect.y))
            .unwrap(),
        Some(shown)
    );
    assert!(layout.hit_test(-1, 0).is_err());
    assert!(layout.hit_test(320, 0).is_err());
}

#[test]
fn native_display_list_is_revisioned_deterministic_and_visibility_aware() {
    let document = NativeDocument::parse(
        "<style>#card { background-color: #102030; color: rgb(1, 2, 3); }</style><main><div id='card'>Hello <span>child</span></div><div id='hidden' style='display:none;background-color:red'>Hidden</div></main>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let card = document.resolve_target("id=card").unwrap();
    let hidden = document.resolve_target("id=hidden").unwrap();

    let list = document.display_list(viewport).unwrap();
    assert_eq!(list.revision, document.revision());
    assert_eq!(list.viewport, viewport);
    assert!(matches!(
        list.commands.first(),
        Some(NativeDisplayCommand::Clear { color }) if *color == NativeColor::WHITE
    ));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::FillRect { node_id, color, .. }
                if *node_id == card
                    && *color == NativeColor { red: 16, green: 32, blue: 48, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { text, color, .. }
                if text == "Hello"
                    && *color == NativeColor { red: 1, green: 2, blue: 3, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { text, color, .. }
                if text == "child"
                    && *color == NativeColor { red: 1, green: 2, blue: 3, alpha: 255 }
        )
    }));
    assert!(!list.commands.iter().any(|command| match command {
        NativeDisplayCommand::FillRect { node_id, .. }
        | NativeDisplayCommand::TextRun { node_id, .. } => *node_id == hidden,
        NativeDisplayCommand::Clear { .. } => false,
    }));
    assert_eq!(list, document.display_list(viewport).unwrap());
}

#[test]
fn native_display_list_inherits_text_color_and_preserves_transparent_override() {
    let document = NativeDocument::parse(
        "<style>#parent { color: blue; }</style><div id='parent'><span id='child'>Child</span><span id='transparent' style='color:transparent'>Clear</span></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 320,
        height: 200,
        device_scale_factor_milli: 1000,
    };
    let child = document.resolve_target("id=child").unwrap();
    let transparent = document.resolve_target("id=transparent").unwrap();
    let list = document.display_list(viewport).unwrap();

    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == child
                    && *color == NativeColor { red: 0, green: 0, blue: 255, alpha: 255 }
        )
    }));
    assert!(list.commands.iter().any(|command| {
        matches!(
            command,
            NativeDisplayCommand::TextRun { node_id, color, .. }
                if *node_id == transparent
                    && *color == NativeColor { red: 0, green: 0, blue: 0, alpha: 0 }
        )
    }));
}

#[test]
fn native_engine_display_list_revision_tracks_accepted_actions() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://paint", "<button id='save'>Save</button>")
        .unwrap()
        .with_initial_url("fixture://paint");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial = engine.display_list().unwrap();
    assert_eq!(initial.revision, 1);
    engine
        .action(NativeAction::Click {
            target: "id=save".into(),
        })
        .unwrap();
    let after_action = engine.display_list().unwrap();
    assert_eq!(after_action.revision, 2);
    assert_ne!(initial, after_action);
}

#[test]
fn native_engine_raster_surface_is_bounded_and_does_not_mutate_revision() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 32,
            height: 24,
            device_scale_factor_milli: 2000,
        })
        .with_fixture(
            "fixture://surface",
            "<style>#card { background-color: red; color: blue; }</style><div id='card'>A</div>",
        )
        .unwrap()
        .with_initial_url("fixture://surface");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let before = engine.revision();
    let surface: NativeSurface = engine.rasterize().unwrap();
    assert_eq!(engine.revision(), before);
    assert_eq!(surface.width(), 32);
    assert_eq!(surface.height(), 24);
    assert_eq!(surface.rgba().len(), 32 * 24 * 4);
    assert_eq!(surface.pixel(31, 19), Some([255, 0, 0, 255]));
    assert_eq!(surface.pixel(1, 0), Some([0, 0, 255, 255]));
}

#[test]
fn native_point_click_hits_nested_content_and_preserves_state_on_rejection() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://layout",
            "<main><button id='save'><span>Save</span></button><p>Other</p></main>",
        )
        .unwrap()
        .with_initial_url("fixture://layout");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let button = engine
        .semantic_nodes()
        .unwrap()
        .into_iter()
        .find(|node| node.name == "Save")
        .unwrap();
    let rect = engine.layout().unwrap().box_for(button.node_id).unwrap();
    let point_target = format!("point={},{}", rect.x + 1, rect.y + 1);
    let clicked = engine
        .action(NativeAction::Click {
            target: point_target,
        })
        .unwrap();
    assert_eq!(clicked.revision, 2);
    assert!(
        engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .find(|node| node.node_id == button.node_id)
            .unwrap()
            .focused
    );

    let rejected = engine
        .action(NativeAction::Click {
            target: "point=319,199".into(),
        })
        .unwrap_err();
    assert!(matches!(
        rejected,
        NativeEngineError::TargetNotActionable { .. }
    ));
    assert_eq!(engine.revision(), 2);
}

#[tokio::test]
async fn native_point_click_uses_the_real_backend_dispatcher() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://point-dispatch",
            "<button id='save'>Save</button>",
        )
        .unwrap()
        .with_initial_url("fixture://point-dispatch");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let clicked = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "point=1,1".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(clicked.revision, 2);
    assert!(clicked.accepted);

    let outside = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "point=319,199".into(),
            },
        })
        .await
        .unwrap_err();
    assert!(matches!(
        outside,
        glass_browser::browser_backend::BrowserBackendError::UnsupportedOperation {
            operation, ..
        } if operation == "action"
    ));
    let evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(evidence.revision, 2);
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn fixture_navigation_projects_through_the_real_backend_dispatcher() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://welcome",
            "<!doctype html><html><head><title>Welcome</title><script>ignored()</script></head><body><h1>Hello &amp; Glass</h1><p>Native fixture</p></body></html>",
        )
        .unwrap();
    let backend = NativeEngineBackend::new(config).unwrap();
    assert_eq!(
        backend.profile().identity.backend_id,
        NATIVE_ENGINE_BACKEND_ID
    );
    assert_eq!(
        backend.profile().identity.certification.level,
        CertificationLevel::Experimental
    );
    assert_eq!(
        backend
            .profile()
            .capability(BrowserCapability::Action)
            .level,
        SupportLevel::Available
    );
    assert_eq!(
        backend
            .profile()
            .capability(BrowserCapability::Effects)
            .level,
        SupportLevel::Available
    );

    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let initial = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(initial.url, "about:blank");
    assert_eq!(initial.revision, 1);

    let navigation = dispatcher
        .navigate(NavigationRequest {
            url: "fixture://welcome".into(),
        })
        .await
        .unwrap();
    assert_eq!(navigation.url, "fixture://welcome");
    assert_eq!(navigation.revision, 2);

    let contexts = dispatcher
        .contexts(glass_browser::browser_backend::ContextRequest {
            include_background: false,
        })
        .await
        .unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].context_id, "native-context");
    assert!(contexts[0].active);

    let evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(evidence.title, "Welcome");
    assert_eq!(evidence.visible_text, "Hello & Glass Native fixture");
    assert!(evidence.complete);
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn unsupported_resources_and_capabilities_fail_without_state_mutation() {
    let backend = NativeEngineBackend::new(NativeEngineConfig::default()).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let before = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();

    let network = dispatcher
        .navigate(NavigationRequest {
            url: "https://example.com".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        network,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "navigation URL"
    ));
    let after = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(after, before);

    let script = dispatcher
        .script(ScriptRequest {
            context_id: "native-context".into(),
            source: "1 + 1".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        script,
        glass_browser::browser_backend::BrowserBackendError::CapabilityUnavailable {
            capability: BrowserCapability::Script,
            actual: SupportLevel::Unavailable,
            declared: false,
            ..
        }
    ));
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn parse_failure_preserves_the_current_document_and_revision() {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://broken", "<p title='unfinished>")
        .unwrap();
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let before = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();

    let error = dispatcher
        .navigate(NavigationRequest {
            url: "fixture://broken".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "native HTML document"
    ));
    let after = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(after, before);
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn data_url_and_limits_are_bounded() {
    let limits = NativeEngineLimits {
        max_document_bytes: 128,
        ..NativeEngineLimits::default()
    };
    let config = NativeEngineConfig::default().with_limits(limits);
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();
    let navigation = dispatcher
        .navigate(NavigationRequest {
            url: "data:text/html,%3Ctitle%3EData%3C%2Ftitle%3E%3Cp%3Eloaded%3C%2Fp%3E".into(),
        })
        .await
        .unwrap();
    assert_eq!(navigation.revision, 2);
    let evidence = dispatcher
        .evidence(EvidenceRequest {
            context_id: "native-context".into(),
            level: EvidenceLevel::Compact,
        })
        .await
        .unwrap();
    assert_eq!(evidence.title, "Data");
    assert_eq!(evidence.visible_text, "loaded");
    dispatcher.close().await.unwrap();
}

#[tokio::test]
async fn semantic_actions_and_effects_use_the_backend_contract() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://controls",
            "<label for='name'>Name</label><input id='name' type='text'><button id='save'>Save</button><input id='remember' type='checkbox'>",
        )
        .unwrap()
        .with_initial_url("fixture://controls");
    let backend = NativeEngineBackend::new(config).unwrap();
    let dispatcher = BrowserBackendDispatcher::new(&backend);
    dispatcher.initialize().await.unwrap();

    let clicked = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Click {
                target: "id=remember".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(clicked.revision, 2);
    assert!(clicked.accepted);

    let click_effects = dispatcher
        .effects(EffectsRequest {
            context_id: "native-context".into(),
            since_revision: 1,
        })
        .await
        .unwrap();
    assert_eq!(click_effects.revision, 2);
    assert!(click_effects.changed);

    let typed = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::Type {
                target: "id=name".into(),
                text: "Glass".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(typed.revision, 3);

    let unchanged = dispatcher
        .effects(EffectsRequest {
            context_id: "native-context".into(),
            since_revision: 3,
        })
        .await
        .unwrap();
    assert_eq!(unchanged.revision, 3);
    assert!(!unchanged.changed);

    let keypress = dispatcher
        .action(ActionRequest {
            context_id: "native-context".into(),
            action: SemanticAction::KeyPress {
                key: "Enter".into(),
            },
        })
        .await
        .unwrap_err();
    assert!(matches!(
        keypress,
        glass_browser::browser_backend::BrowserBackendError::UnsupportedOperation {
            operation, ..
        } if operation == "action"
    ));

    let future = dispatcher
        .effects(EffectsRequest {
            context_id: "native-context".into(),
            since_revision: 4,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        future,
        glass_browser::browser_backend::BrowserBackendError::InvalidConfiguration { field, .. }
            if field == "since revision"
    ));
    dispatcher.close().await.unwrap();
}

#[test]
fn native_backend_requires_explicit_factory_selection() {
    let automatic = BackendSelectionRequest {
        schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
        glass_version: env!("CARGO_PKG_VERSION").into(),
        preferred_backend_id: None,
        browser_family: None,
        browser_version: None,
        required_capabilities: vec![],
        minimum_certification: CertificationLevel::Experimental,
    };
    let native = BackendFactory::native(NativeEngineConfig::default()).unwrap();
    assert!(BackendFactory::start(&automatic, vec![native]).is_err());

    let explicit = BackendSelectionRequest {
        preferred_backend_id: Some(NATIVE_ENGINE_BACKEND_ID.into()),
        ..automatic
    };
    let native = BackendFactory::native(NativeEngineConfig::default()).unwrap();
    let started = BackendFactory::start(&explicit, vec![native]).unwrap();
    assert_eq!(
        started.profile().identity.backend_id,
        NATIVE_ENGINE_BACKEND_ID
    );
}

#[test]
fn lifecycle_state_is_terminal_after_close() {
    let mut engine = glass_browser::NativeEngine::new(NativeEngineConfig::default()).unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::New);
    engine.initialize().unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::Running);
    engine.close().unwrap();
    assert_eq!(engine.lifecycle(), NativeLifecycleState::Closed);
    assert!(engine.initialize().is_err());
    assert!(engine.close().is_err());
}

#[test]
fn semantic_dom_references_and_locators_are_revision_bound() {
    let document = NativeDocument::parse(
        "<label for='query'>Query</label><input id='query' type='search'><button id='go'>Go</button>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let nodes = document.semantic_nodes();
    let query = nodes.iter().find(|node| node.role == "textbox").unwrap();
    assert_eq!(query.name, "Query");
    assert_eq!(document.resolve_target("id=query"), Ok(query.node_id));
    assert_eq!(
        document.resolve_target("role=textbox[name=Query]"),
        Ok(query.node_id)
    );
    assert_eq!(
        query.reference,
        format!("ref=r1:n{}", query.node_id.index())
    );
    assert!(document.resolve_target(&query.reference).is_ok());
}

#[test]
fn native_actions_update_state_and_reject_unsafe_targets_before_mutation() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://controls",
            "<label for='name'>Name</label><input id='name' type='text'><input id='remember' type='checkbox'><input id='first' type='radio' name='choice'><input id='second' type='radio' name='choice' checked><input id='disabled' type='text' disabled><input id='readonly' type='text' readonly><button id='save'>Save</button><button id='other'>Other</button><p id='plain'>Plain</p><select id='country'><option id='one' value='one'>One</option><option id='two' value='two'>Two</option></select><select id='many' multiple><option id='many-one'>Many One</option></select>",
        )
        .unwrap()
        .with_initial_url("fixture://controls");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let initial_nodes = engine.semantic_nodes().unwrap();
    let remember_reference = initial_nodes
        .iter()
        .find(|node| node.role == "checkbox")
        .map(|node| node.reference.clone())
        .unwrap();
    assert_eq!(engine.revision(), 1);

    let click = engine
        .action(NativeAction::Click {
            target: "id=remember".into(),
        })
        .unwrap();
    assert_eq!(click.revision, 2);
    let after_click = engine.semantic_nodes().unwrap();
    let remember = after_click
        .iter()
        .find(|node| node.role == "checkbox")
        .unwrap();
    assert_eq!(remember.checked, Some(true));
    assert!(remember.focused);
    let click_effects = engine.effects_since(1).unwrap();
    assert_eq!(
        click_effects
            .effects
            .iter()
            .map(|effect| effect.kind)
            .collect::<Vec<_>>(),
        vec![
            NativeEventKind::Focus,
            NativeEventKind::Click,
            NativeEventKind::Change
        ]
    );

    let typed = engine
        .action(NativeAction::Type {
            target: "id=name".into(),
            text: "Glass".into(),
        })
        .unwrap();
    assert_eq!(typed.revision, 3);
    let after_type = engine.semantic_nodes().unwrap();
    let name = after_type.iter().find(|node| node.name == "Name").unwrap();
    let remember = after_type
        .iter()
        .find(|node| node.role == "checkbox")
        .unwrap();
    assert_eq!(name.empty, Some(false));
    assert!(name.focused);
    assert!(!remember.focused);

    engine
        .action(NativeAction::Click {
            target: "id=first".into(),
        })
        .unwrap();
    let radios = engine.semantic_nodes().unwrap();
    assert_eq!(
        radios
            .iter()
            .find(|node| node.role == "radio")
            .and_then(|node| node.checked),
        Some(true)
    );
    assert_eq!(
        radios
            .iter()
            .filter(|node| node.role == "radio")
            .nth(1)
            .and_then(|node| node.checked),
        Some(false)
    );

    let options = engine.semantic_nodes().unwrap();
    assert_eq!(
        options
            .iter()
            .find(|node| node.name == "One")
            .and_then(|node| node.selected),
        Some(true)
    );
    assert_eq!(
        options
            .iter()
            .find(|node| node.name == "Two")
            .and_then(|node| node.selected),
        Some(false)
    );
    assert_eq!(
        options
            .iter()
            .find(|node| node.role == "combobox" && node.tag_name == "select")
            .and_then(|node| node.empty),
        Some(false)
    );
    let selected = engine
        .action(NativeAction::Click {
            target: "id=two".into(),
        })
        .unwrap();
    assert_eq!(selected.revision, 5);
    let selected_options = engine.semantic_nodes().unwrap();
    assert_eq!(
        selected_options
            .iter()
            .find(|node| node.name == "One")
            .and_then(|node| node.selected),
        Some(false)
    );
    assert_eq!(
        selected_options
            .iter()
            .find(|node| node.name == "Two")
            .and_then(|node| node.selected),
        Some(true)
    );

    let revision_before_rejections = engine.revision();
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "role=button".into(),
        }),
        Err(glass_browser::NativeEngineError::AmbiguousTarget { matches: 2 })
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=plain".into(),
        }),
        Err(glass_browser::NativeEngineError::TargetNotActionable { .. })
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=many-one".into(),
        }),
        Err(glass_browser::NativeEngineError::TargetNotActionable { .. })
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: remember_reference,
        }),
        Err(glass_browser::NativeEngineError::DetachedTarget)
    ));
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=disabled".into(),
        }),
        Err(glass_browser::NativeEngineError::DisabledTarget)
    ));
    assert!(matches!(
        engine.action(NativeAction::Type {
            target: "id=readonly".into(),
            text: "nope".into(),
        }),
        Err(glass_browser::NativeEngineError::ReadOnlyTarget)
    ));
    assert_eq!(engine.revision(), revision_before_rejections);
}

#[test]
fn visibility_projection_and_actionability_are_consistent() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://visibility",
            "<button id='visible'>Visible</button><button id='hidden' hidden>Hidden</button><button id='aria' aria-hidden='true'>Aria</button><button id='display' style='display: none'>Display</button><button id='visibility' style='VISIBILITY : HIDDEN'>Visibility</button><div hidden><button id='nested'>Nested</button></div><input id='hidden-input' type='text' style='display:none'><button id='opaque' style='opacity:0'>Opaque</button>",
        )
        .unwrap()
        .with_initial_url("fixture://visibility");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.visible_text, "Visible Opaque");
    assert!(!snapshot.text_truncated);

    let nodes = engine.semantic_nodes().unwrap();
    let visible = nodes.iter().find(|node| node.name == "Visible").unwrap();
    assert!(!visible.hidden);
    let opaque = nodes.iter().find(|node| node.name == "Opaque").unwrap();
    assert!(!opaque.hidden);
    for name in ["Hidden", "Aria", "Display", "Visibility", "Nested"] {
        assert!(
            nodes
                .iter()
                .find(|node| node.name == name)
                .is_some_and(|node| node.hidden),
            "expected {name} to be hidden"
        );
    }
    assert!(
        nodes
            .iter()
            .find(|node| node.role == "textbox" && node.name.is_empty())
            .is_some_and(|node| node.hidden)
    );

    let revision_before_rejections = engine.revision();
    for target in ["hidden", "aria", "display", "visibility", "nested"] {
        let result = engine.action(NativeAction::Click {
            target: format!("id={target}"),
        });
        assert!(matches!(
            result,
            Err(NativeEngineError::TargetNotActionable { reason })
                if reason == "hidden targets are not actionable"
        ));
    }
    let result = engine.action(NativeAction::Type {
        target: "id=hidden-input".into(),
        text: "secret".into(),
    });
    assert!(matches!(
        result,
        Err(NativeEngineError::TargetNotActionable { reason })
            if reason == "hidden targets are not actionable"
    ));
    assert_eq!(engine.revision(), revision_before_rejections);
    assert!(
        engine
            .effects_since(revision_before_rejections)
            .unwrap()
            .effects
            .is_empty()
    );
    assert!(
        !engine
            .semantic_nodes()
            .unwrap()
            .into_iter()
            .any(|node| node.focused)
    );
}

#[test]
fn stylesheet_presentation_state_feeds_text_and_actionability() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://styles",
            "<style>/* bounded rules */ .gone, [data-mode=off] { display: none; } #covered { visibility: hidden; } button { display: none; }</style><button id='gone' class='gone'>Gone</button><button id='covered'>Covered</button><button id='attribute' data-mode='off'>Attribute</button><button id='override' style='display: block'>Override</button>",
        )
        .unwrap()
        .with_initial_url("fixture://styles");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.visible_text, "Override");

    let nodes = engine.semantic_nodes().unwrap();
    for name in ["Gone", "Covered", "Attribute"] {
        assert!(
            nodes
                .iter()
                .find(|node| node.name == name)
                .is_some_and(|node| node.hidden),
            "expected {name} to be hidden by stylesheet"
        );
    }
    assert!(
        nodes
            .iter()
            .find(|node| node.name == "Override")
            .is_some_and(|node| !node.hidden)
    );

    let revision_before_rejections = engine.revision();
    assert!(matches!(
        engine.action(NativeAction::Click {
            target: "id=gone".into(),
        }),
        Err(NativeEngineError::TargetNotActionable { .. })
    ));
    assert_eq!(engine.revision(), revision_before_rejections);
    let click = engine
        .action(NativeAction::Click {
            target: "id=override".into(),
        })
        .unwrap();
    assert_eq!(click.revision, revision_before_rejections + 1);
}

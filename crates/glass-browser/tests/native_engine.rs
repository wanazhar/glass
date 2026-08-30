#![cfg(feature = "native-engine")]

use glass_browser::browser::native_backend::NATIVE_ENGINE_BACKEND_ID;
use glass_browser::browser::native_engine::{
    NativeDocument, NativeEngineConfig, NativeEngineLimits, NativeLifecycleState,
};
use glass_browser::browser_backend::{
    BROWSER_BACKEND_SCHEMA_VERSION, BackendSelectionRequest, BrowserBackendDispatcher,
    BrowserCapability, CertificationLevel, EvidenceLevel, EvidenceRequest, NavigationRequest,
    ScriptRequest, SupportLevel,
};
use glass_browser::{BackendFactory, NativeEngineBackend};

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

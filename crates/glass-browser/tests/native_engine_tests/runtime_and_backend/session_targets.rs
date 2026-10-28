use super::*;

#[test]
fn native_runtime_dynamic_iframe_srcdoc_property_updates_the_attribute() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_runtime_dynamic_iframe_srcdoc_property_updates_the_attribute_inner());
    });
}

async fn native_runtime_dynamic_iframe_srcdoc_property_updates_the_attribute_inner() {
    let session =
        BrowserRuntimeSession::connect_native(NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3EParent%3C%2Ftitle%3E%3Cbody%3Eparent%3C%2Fbody%3E",
        ))
        .await
        .unwrap();
    let child_markup = "<p id='sample'>video sample</p>";
    let result = session
        .script(&format!(
            "(() => {{ const frame = document.createElement('iframe'); frame.id = 'output-iframe'; document.body.appendChild(frame); frame.srcdoc = {child_markup:?}; return [frame.srcdoc, frame.getAttribute('srcdoc')]; }})()"
        ))
        .await
        .unwrap();
    assert_eq!(
        result.value,
        serde_json::json!([child_markup, child_markup])
    );
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("dynamically authored srcdoc should create a child browsing context")
        .id
        .clone();
    session.native_select_frame(&child_id).await.unwrap();
    assert_eq!(
        session
            .script("document.body.textContent")
            .await
            .unwrap()
            .value,
        serde_json::json!("video sample")
    );
    session.close().await.unwrap();
}

#[test]
fn native_runtime_template_composed_iframe_srcdoc_bootstraps_embedded_media_document() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_runtime_template_composed_iframe_srcdoc_bootstraps_embedded_media_document_inner(
            ),
        );
    });
}

async fn native_runtime_template_composed_iframe_srcdoc_bootstraps_embedded_media_document_inner() {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://mdn-srcdoc.test/demo",
                "<!doctype html><template id='head'><title>%title%</title></template><template id='body'><main id='output'>%html%</main></template><iframe id='output-iframe' src='about:blank'></iframe>",
            )
            .unwrap()
            .with_initial_url("fixture://mdn-srcdoc.test/demo"),
    )
    .await
    .unwrap();
    let initial_frames = session.native_list_frames().await.unwrap();
    let initial_child_id = initial_frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("the static iframe should begin with its initial about:blank child")
        .id
        .clone();

    // This follows the MDN tabbed example: read separate template.innerHTML
    // fragments, interpolate a document, and assign it to an about:blank iframe.
    let result = session
        .script(
            "(() => { const head = document.querySelector('#head').innerHTML.replace('%title%', 'Embedded video'); const body = document.querySelector('#body').innerHTML.replace('%html%', '<video controls><source src=\"/media/flower.webm\" type=\"video/webm\"></video>'); const frame = document.querySelector('#output-iframe'); frame.srcdoc = '<!doctype html><html><head>' + head + '</head><body>' + body + '</body></html>'; return [frame.srcdoc.includes('<video'), frame.getAttribute('srcdoc').includes('/media/flower.webm')]; })()",
        )
        .await
        .unwrap();
    assert_eq!(result.value, serde_json::json!([true, true]));

    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(
        frames.len(),
        2,
        "srcdoc should replace the initial about:blank child"
    );
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("the template-composed iframe should have a child context")
        .id
        .clone();
    assert_ne!(
        child_id, initial_child_id,
        "changing srcdoc must replace the child document"
    );
    let stable_frames = session.native_list_frames().await.unwrap();
    assert_eq!(
        stable_frames
            .iter()
            .map(|frame| &frame.id)
            .collect::<Vec<_>>(),
        frames.iter().map(|frame| &frame.id).collect::<Vec<_>>(),
        "unchanged srcdoc must keep a stable child frame"
    );
    session.native_select_frame(&child_id).await.unwrap();
    let child = session
        .script("[document.title, document.querySelector('video') !== null, document.querySelector('source').getAttribute('src')]")
        .await
        .unwrap();
    assert_eq!(
        child.value,
        serde_json::json!(["Embedded video", true, "/media/flower.webm"])
    );
    session.close().await.unwrap();
}

#[test]
fn native_runtime_srcdoc_inherits_parent_base_for_relative_media_loading() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_runtime_srcdoc_inherits_parent_base_for_relative_media_loading_inner(),
        );
    });
}

async fn native_runtime_srcdoc_inherits_parent_base_for_relative_media_loading_inner() {
    let root =
        std::env::temp_dir().join(format!("glass-native-srcdoc-base-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let media_path = root.join("flower.webm");
    let page_path = root.join("index.html");
    fs::write(
        &media_path,
        include_bytes!(
            "../../../../glass-browser-core/tests/fixtures/native-media/two-color-frames-vp8.webm"
        ),
    )
    .unwrap();
    fs::write(
        &page_path,
        "<!doctype html><template id='head'><title>%title%</title></template><template id='body'>%html%</template><iframe id='output-iframe' src='about:blank'></iframe>",
    )
    .unwrap();
    let page_url = native_test_file_url(&page_path);
    let media_url = native_test_file_url(&media_path);
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url(page_url.clone())
            .with_allowed_file_root(root.clone()),
    )
    .await
    .unwrap();

    let initial_child_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("static src should create the initial about:blank frame")
        .id;
    session
        .script(
            "(() => { const head = document.querySelector('#head').innerHTML.replace('%title%', 'Inherited media base'); const body = document.querySelector('#body').innerHTML.replace('%html%', '<video id=\"sample\"><source src=\"./flower.webm\" type=\"video/webm; codecs=vp8\"></video>'); const frame = document.querySelector('#output-iframe'); frame.srcdoc = '<!doctype html><html><head>' + head + '</head><body>' + body + '</body></html>'; return true; })()",
        )
        .await
        .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .expect("srcdoc should have a child frame")
        .id
        .clone();
    assert_ne!(child_id, initial_child_id);
    assert_eq!(
        session
            .native_list_frames()
            .await
            .unwrap()
            .into_iter()
            .map(|frame| frame.id)
            .collect::<Vec<_>>(),
        frames.into_iter().map(|frame| frame.id).collect::<Vec<_>>(),
        "unchanged srcdoc should preserve the child frame"
    );
    session.native_select_frame(&child_id).await.unwrap();
    let media = session
        .script("[document.baseURI, document.querySelector('video').readyState, document.querySelector('video').currentSrc, document.querySelector('video').duration]")
        .await
        .unwrap();
    assert_eq!(media.value, serde_json::json!([page_url, 2, media_url, 1]));
    session.close().await.unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_runtime_session_exposes_agent_inspection_and_target_discovery() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_runtime_session_exposes_agent_inspection_and_target_discovery_inner());
    });
}

async fn native_runtime_session_exposes_agent_inspection_and_target_discovery_inner() {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3EInspect%3C%2Ftitle%3E%3Cmain%3E%3Cbutton%20id%3D%27save%27%3ESave%3C%2Fbutton%3E%3Cinput%20id%3D%27name%27%3E%3C%2Fmain%3E",
        ),
    )
    .await
    .unwrap();

    let inspection = session.native_inspect_page().await.unwrap();
    assert_eq!(inspection.page.title, "Inspect");
    assert_eq!(inspection.page.target_id, "native-context");
    assert_eq!(inspection.regions.len(), 1);
    assert_eq!(inspection.regions[0].targets.len(), 2);
    assert_eq!(inspection.limits.viewport.unwrap().width, 1280.0);

    let request = SemanticIntentRequest {
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
        expected_revision: Some(inspection.revision),
    };
    let result = session.native_find_target(&request).await.unwrap();
    assert_eq!(result.ambiguity, "none");
    assert_eq!(result.revision, Some(inspection.revision));
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].role, "button");
    assert_eq!(result.candidates[0].name, "Save");
    assert_eq!(result.candidates[0].confidence, IntentConfidence::Exact);

    let preflight = session
        .native_preflight_target(
            &result.candidates[0].reference,
            NativePreflightAction::Click,
        )
        .await
        .unwrap();
    assert!(preflight.unique);
    assert_eq!(preflight.revision, inspection.revision);

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id, "native-context");
    assert_eq!(targets[0].title, "Inspect");
    assert!(targets[0].active);
    assert_eq!(
        session
            .native_select_target(&targets[0].id)
            .await
            .unwrap()
            .id,
        "native-context"
    );

    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].id, "native-context:main");
    assert!(frames[0].active);
    assert!(!frames[0].out_of_process);
    assert_eq!(
        session.native_select_frame(&frames[0].id).await.unwrap().id,
        "native-context:main"
    );
    assert!(
        session
            .native_select_target("missing-target")
            .await
            .is_err()
    );
    assert!(session.native_select_frame("missing-frame").await.is_err());
    session.close().await.unwrap();
}

#[test]
fn native_runtime_semantic_intents_discover_and_route_nested_frame_targets() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_runtime_semantic_intents_discover_and_route_nested_frame_targets_inner(),
        );
    });
}

async fn native_runtime_semantic_intents_discover_and_route_nested_frame_targets_inner() {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://semantic-frame-parent",
                "<iframe style='display:block;width:48px;height:40px' src='fixture://semantic-frame-child'></iframe>",
            )
            .unwrap()
            .with_fixture(
                "fixture://semantic-frame-child",
                "<input id='inside' aria-label='Inside' value='before'>",
            )
            .unwrap()
            .with_initial_url("fixture://semantic-frame-parent"),
    )
    .await
    .unwrap();

    let inspection = session.native_inspect_page().await.unwrap();
    assert_eq!(inspection.regions.len(), 2);
    let child_target = inspection
        .regions
        .iter()
        .flat_map(|region| region.targets.iter())
        .find(|target| target.name == "Inside")
        .expect("nested input must be discoverable");
    let child_frame_id = child_target
        .frame_id
        .clone()
        .expect("nested semantic target must identify its frame");
    assert_ne!(child_frame_id, inspection.page.frame_id);

    let request = SemanticIntentRequest {
        schema_version: 1,
        intent: "inside".into(),
        action: SemanticIntentAction::Type,
        scope: Default::default(),
        constraints: IntentConstraints {
            role: Some("textbox".into()),
            name: Some("Inside".into()),
            ..Default::default()
        },
        resolution_policy: SemanticResolutionPolicy::RequireExact,
        expected_revision: Some(inspection.revision),
    };
    let resolved = session.native_find_target(&request).await.unwrap();
    assert_eq!(resolved.ambiguity, "none");
    assert_eq!(resolved.candidates.len(), 1);
    assert_eq!(
        resolved.candidates[0].frame_id.as_deref(),
        Some(child_frame_id.as_str())
    );

    let preflight = session
        .native_preflight_target(
            &resolved.candidates[0].reference,
            NativePreflightAction::Type,
        )
        .await
        .unwrap();
    assert!(preflight.unique);
    assert_eq!(preflight.frame_id.as_deref(), Some(child_frame_id.as_str()));

    let execution = session
        .native_act_and_verify(
            &SemanticIntentExecutionRequest {
                request,
                candidate_id: resolved.candidates[0].id.clone(),
                value: Some("after".into()),
            },
            None,
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    assert_eq!(execution.status, "dispatched_unverified");
    assert_eq!(
        execution.execution.action.as_ref().unwrap().frame_id,
        child_frame_id
    );

    session.native_select_frame(&child_frame_id).await.unwrap();
    let value = session
        .script("document.getElementById('inside').value")
        .await
        .unwrap();
    assert_eq!(value.value, serde_json::json!("after"));
    session.close().await.unwrap();
}

#[test]
fn native_runtime_session_preserves_independent_target_state_and_lifecycle() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(
            native_runtime_session_preserves_independent_target_state_and_lifecycle_inner(),
        );
    });
}

async fn native_runtime_session_preserves_independent_target_state_and_lifecycle_inner() {
    let first_url = "data:text/html,%3Ctitle%3EFirst%3C%2Ftitle%3E%3Cp%3Efirst%20target%3C%2Fp%3E";
    let second_url =
        "data:text/html,%3Ctitle%3ESecond%3C%2Ftitle%3E%3Cp%3Esecond%20target%3C%2Fp%3E";
    let third_url = "data:text/html,%3Ctitle%3EThird%3C%2Ftitle%3E%3Cp%3Ethird%20target%3C%2Fp%3E";
    let session = BrowserSession::start(NativeEngineConfig::default().with_initial_url(first_url))
        .await
        .unwrap();

    let initial_targets = session.list_targets().await.unwrap();
    assert_eq!(initial_targets.len(), 1);
    assert_eq!(initial_targets[0].id, "native-context");
    assert!(initial_targets[0].active);

    let second = session.create_target(second_url).await.unwrap();
    assert_eq!(second.id, "native-target-1");
    assert!(!second.active);
    assert_eq!(second.opener_id.as_deref(), Some("native-context"));
    assert_eq!(second.title, "Second");

    let targets = session.list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].id, "native-context");
    assert!(targets[0].active);
    assert_eq!(targets[1].id, "native-target-1");
    assert!(!targets[1].active);

    let selected = session.select_target(&second.id).await.unwrap();
    assert_eq!(selected.id, second.id);
    assert!(selected.active);
    assert_eq!(
        session.contexts().await.unwrap()[0].context_id,
        "native-target-1"
    );
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "Second"
    );

    session.select_target("native-context").await.unwrap();
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "First"
    );

    let third = session.create_target(third_url).await.unwrap();
    session.close_target(&third.id).await.unwrap();
    assert_eq!(session.list_targets().await.unwrap().len(), 2);

    session.close_target(&second.id).await.unwrap();
    let remaining = session.list_targets().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "native-context");
    assert!(remaining[0].active);

    session.close_target("native-context").await.unwrap();
    assert!(session.list_targets().await.unwrap().is_empty());
    assert!(session.contexts().await.is_err());
    assert!(session.list_frames().await.is_err());

    session.close().await.unwrap();
}

#[test]
fn native_runtime_session_owns_and_routes_child_frames() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_runtime_session_owns_and_routes_child_frames_inner());
    });
}

async fn native_runtime_session_owns_and_routes_child_frames_inner() {
    let parent_url = "data:text/html,%3Ctitle%3EParent%3C%2Ftitle%3E%3Ciframe%20src%3D%22about%3Ablank%22%3Efallback%3C%2Fiframe%3E%3Cp%3Eparent%3C%2Fp%3E";
    let child_url = "data:text/html,%3Ctitle%3EChild%3C%2Ftitle%3E%3Ciframe%20srcdoc%3D%22%3Cp%3Egrandchild%3C%2Fp%3E%22%3E%3C%2Fiframe%3E%3Cbutton%20id%3D%22child-button%22%3EChild%20button%3C%2Fbutton%3E%3Cp%3Echild%20content%3C%2Fp%3E";
    let session = BrowserSession::start(NativeEngineConfig::default().with_initial_url(parent_url))
        .await
        .unwrap();

    let frames = session.list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].id, "native-context:main");
    assert!(frames[0].active);
    assert_eq!(frames[1].id, "native-context:frame-1");
    assert_eq!(frames[1].parent_id.as_deref(), Some("native-context:main"));
    assert!(!frames[1].active);
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .visible_text,
        "parent"
    );

    session.select_frame(&frames[1].id).await.unwrap();
    session.navigate(child_url).await.unwrap();
    let child_evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(child_evidence.title, "Child");
    assert_eq!(child_evidence.visible_text, "Child button child content");
    assert_eq!(
        session.inspect_page().await.unwrap().page.frame_id,
        "native-context:frame-1"
    );
    assert_eq!(
        session.script("document.title").await.unwrap().value,
        serde_json::json!("Child")
    );
    let action = session
        .action(SemanticAction::Click {
            target: "id=child-button".into(),
        })
        .await
        .unwrap();
    assert!(action.accepted);
    let child_viewport = session
        .inspect_page()
        .await
        .unwrap()
        .limits
        .viewport
        .unwrap();
    assert!(child_viewport.scroll_y > 0.0);
    let child_frames = session.list_frames().await.unwrap();
    assert_eq!(child_frames.len(), 3);
    assert!(child_frames.iter().any(|frame| {
        frame.id == "native-context:frame-1" && frame.active && frame.url == child_url
    }));
    assert!(child_frames.iter().any(|frame| {
        frame.id == "native-context:frame-2"
            && !frame.active
            && frame.parent_id.as_deref() == Some("native-context:frame-1")
            && frame.url == "data:text/html,%3Cp%3Egrandchild%3C%2Fp%3E"
    }));
    assert!(
        child_frames
            .iter()
            .any(|frame| frame.id == "native-context:main" && !frame.active)
    );

    session
        .select_frame("native-context:frame-2")
        .await
        .unwrap();
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        "data:text/html,%3Cp%3Egrandchild%3C%2Fp%3E"
    );
    session
        .select_frame("native-context:frame-1")
        .await
        .unwrap();
    assert_eq!(
        session.script("document.title").await.unwrap().value,
        serde_json::json!("Child")
    );

    session.select_frame("native-context:main").await.unwrap();
    let parent_evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(parent_evidence.title, "Parent");
    assert_eq!(parent_evidence.visible_text, "parent");
    session.close().await.unwrap();
}

#[test]
fn native_runtime_opener_links_create_routable_popup_targets() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_runtime_opener_links_create_routable_popup_targets_inner());
    });
}

async fn native_runtime_opener_links_create_routable_popup_targets_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://popup-parent",
            "<script>globalThis.ready = true;</script><title>Opener</title><a id='open' target='_blank' href='fixture://popup-child'>Open popup</a><p>opener stays here</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://popup-child",
            "<title>Popup</title><p>popup content</p>",
        )
        .unwrap()
        .with_initial_url("fixture://popup-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let action = session
        .action(SemanticAction::Click {
            target: "id=open".into(),
        })
        .await
        .unwrap();
    assert!(action.accepted);
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        "fixture://popup-parent"
    );
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let popup = targets
        .iter()
        .find(|target| target.id == "native-target-1")
        .unwrap();
    assert!(!popup.active);
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    assert_eq!(popup.url, "fixture://popup-child");
    assert_eq!(popup.title, "Popup");

    session.native_select_target(&popup.id).await.unwrap();
    let popup_evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(popup_evidence.title, "Popup");
    assert_eq!(popup_evidence.visible_text, "popup content");

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    let opener_evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(opener_evidence.title, "Opener");
    assert_eq!(opener_evidence.visible_text, "Open popup opener stays here");
    session.close().await.unwrap();
}

#[test]
fn native_runtime_session_exposes_bounded_wait_and_verification() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(
            "data:text/html,%3Ctitle%3EVerify%3C%2Ftitle%3E%3Cmain%3E%3Cbutton%20id%3D%27save%27%3ESave%3C%2Fbutton%3E%3C/main%3E",
        ),
    )
    .await
    .unwrap();

    let lifecycle = session
        .native_wait(
            WaitCondition::Lifecycle("complete".into()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(lifecycle.condition, "lifecycle");
    assert_eq!(lifecycle.target_id, "native-context");

    let title = session
        .native_wait(
            WaitCondition::TitleContains("veriFY".into()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(title.condition, "title_contains");
    assert_eq!(title.target_id, "native-context");
    assert_eq!(title.last_state, "title=Verify");

    let url = session
        .native_wait(
            WaitCondition::UrlPrefix("data:text/html".into()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert!(url.last_state.starts_with("url=data:text/html"));

    let text = session
        .native_wait(
            WaitCondition::Text("Save".into()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(text.last_state, "present=true");

    let visible = session
        .native_wait(
            WaitCondition::TargetVisible("id=save".into()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(visible.condition, "target_visible");

    let stable = session
        .native_wait(
            WaitCondition::TargetStable("id=save".into()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(stable.condition, "target_stable");

    let javascript = session
        .native_wait(
            WaitCondition::JavaScript("document.title === 'Verify'".into()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(javascript.last_state, "true");

    let verified = session
        .native_verify(
            VerificationPredicate::All {
                all: vec![
                    VerificationPredicate::TitleContains {
                        value: "Verify".into(),
                    },
                    VerificationPredicate::TextContains {
                        value: "Save".into(),
                    },
                ],
            },
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(verified.status, "satisfied");
    assert!(verified.state.starts_with("all=["));

    let acted = session
        .native_act_and_verify(
            &SemanticIntentExecutionRequest {
                request: SemanticIntentRequest {
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
                    expected_revision: None,
                },
                candidate_id: "candidate_1".into(),
                value: None,
            },
            Some(VerificationPredicate::TextContains {
                value: "Save".into(),
            }),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(acted.status, "verified");
    assert!(!acted.mutation_possible);
    assert!(acted.execution.action.is_some());
    assert!(acted.execution.execution_id.is_some());

    let timeout = session
        .native_wait(
            WaitCondition::Text("missing".into()),
            Duration::from_millis(1),
        )
        .await
        .unwrap_err();
    assert!(timeout.to_string().contains("wait timed out"));
    session.close().await.unwrap();
        });
    });
}

#[test]
fn native_runtime_session_guards_mutations_and_serializes_races() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_runtime_session_guards_mutations_and_serializes_races_inner());
    });
}

async fn native_runtime_session_guards_mutations_and_serializes_races_inner() {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_initial_url("data:text/html,%3Cbutton%20id%3D%27save%27%3ESave%3C%2Fbutton%3E"),
    )
    .await
    .unwrap();

    let revision = session
        .evidence(EvidenceLevel::Compact)
        .await
        .unwrap()
        .revision;
    let stale = session
        .action_with_revision(
            SemanticAction::Click {
                target: "id=save".into(),
            },
            revision.saturating_sub(1),
        )
        .await
        .unwrap_err();
    assert!(stale.to_string().contains("stale page revision"));

    let (first, second) = tokio::join!(
        session.action_with_revision(
            SemanticAction::Click {
                target: "id=save".into(),
            },
            revision,
        ),
        session.action_with_revision(
            SemanticAction::Click {
                target: "id=save".into(),
            },
            revision,
        ),
    );
    assert_eq!(
        first.as_ref().ok().map(|result| result.revision),
        Some(revision + 1)
    );
    assert!(
        second
            .as_ref()
            .err()
            .is_some_and(|error| error.to_string().contains("stale page revision"))
    );

    let next_revision = first.unwrap().revision;
    let navigation = session
        .navigate_with_revision("data:text/html,%3Cp%3ANext%3C%2Fp%3E", next_revision)
        .await
        .unwrap();
    assert_eq!(navigation.revision, next_revision + 1);
    session.close().await.unwrap();
}

#[test]
fn native_runtime_session_loads_bounded_external_http_html_without_cdp() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_runtime_session_loads_bounded_external_http_html_without_cdp_inner());
    });
}

async fn native_runtime_session_loads_bounded_external_http_html_without_cdp_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request.split_whitespace().nth(1).unwrap_or("/");
            let (status, content_type, location, body) = if path.starts_with("/redirect") {
                ("302 Found", "text/plain", Some("/page"), "")
            } else if path.starts_with("/page") {
                (
                    "200 OK",
                    "text/html; charset=utf-8",
                    None,
                    concat!(
                        "<title>Native HTTP</title><p>External native page</p>",
                        "<script>const fragment=document.createElement('div');",
                        "fragment.innerHTML='<table><tr><td>parsed</td></tr></table>';",
                        "const table=fragment.firstElementChild;",
                        "document.body.appendChild(fragment);",
                        "document.title=table && table.firstElementChild.localName==='tbody'",
                        "?'Native HTTP':'native fragment parse failed';</script>",
                    ),
                )
            } else {
                ("200 OK", "application/json", None, "{\"not\":\"html\"}")
            };
            let location_header =
                location.map_or_else(String::new, |value| format!("Location: {value}\r\n"));
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{location_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let initial_url = format!("http://{address}/redirect#section");
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(initial_url),
    )
    .await
    .unwrap();
    let evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(evidence.url, format!("http://{address}/page#section"));
    assert_eq!(evidence.title, "Native HTTP");
    assert_eq!(evidence.visible_text, "External native page parsed");

    let error = session
        .navigate(format!("http://{address}/not-html"))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported content type"), "{error}");
    session.close().await.unwrap();
    server.await.unwrap();
}

#[test]
fn native_engine_parent_fetch_owns_cookie_matching_and_response_updates() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_engine_parent_fetch_owns_cookie_matching_and_response_updates_inner());
    });
}

async fn native_engine_parent_fetch_owns_cookie_matching_and_response_updates_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..3 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("parent Fetch request should reach the local server")
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
                        "Set-Cookie: parent_session=initial; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: parent_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<p>page</p>",
                ),
                "/set" => (
                    concat!(
                        "Set-Cookie: parent_session=latest; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: parent_secret=latest-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/plain",
                    "updated",
                ),
                "/inspect" => ("", "text/plain", "inspected"),
                other => panic!("unexpected parent Fetch path: {other}"),
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
    let update = engine
        .fetch_async(format!("http://{address}/set"), true)
        .await
        .unwrap();
    assert_eq!(update.body, b"updated");
    let document_cookie = engine.evaluate_async("document.cookie").await.unwrap();
    assert_eq!(document_cookie, "parent_session=latest");
    let inspected = engine
        .fetch_async(format!("http://{address}/inspect"), true)
        .await
        .unwrap();
    assert_eq!(inspected.body, b"inspected");
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();

    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/page", "/set", "/inspect"]
    );
    let update_cookie = requests[1].1.as_deref().unwrap_or_default();
    assert!(update_cookie.contains("parent_session=initial"));
    assert!(update_cookie.contains("parent_secret=initial-secret"));
    let inspect_cookie = requests[2].1.as_deref().unwrap_or_default();
    assert!(inspect_cookie.contains("parent_session=latest"));
    assert!(inspect_cookie.contains("parent_secret=latest-secret"));
}

use super::*;

#[test]
fn native_dialogs_are_owned_by_the_page_realm_and_prompt_backend() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let session = BrowserRuntimeSession::connect_native(
                NativeEngineConfig::default().with_initial_url(
                    "data:text/html,%3Ctitle%3EDialogs%3C%2Ftitle%3E%3Cp%3ENative%3C%2Fp%3E",
                ),
            )
            .await
            .unwrap();

            assert!(session.native_dialog_controller().is_err());
            assert!(session.native_pending_dialog().await.unwrap().is_none());
            assert_eq!(
                session
                    .script("alert('hello'); 'after-alert'")
                    .await
                    .unwrap()
                    .value,
                serde_json::json!("after-alert")
            );
            let pending = session.native_pending_dialog().await.unwrap().unwrap();
            assert_eq!(pending.dialog_type, "alert");
            assert_eq!(pending.message, "hello");
            assert_eq!(
                pending.url,
                "data:text/html,%3Ctitle%3EDialogs%3C%2Ftitle%3E%3Cp%3ENative%3C%2Fp%3E"
            );
            assert!(
                session
                    .native_resolve_dialog(PromptDecision::Accept)
                    .await
                    .unwrap()
                    .handled
            );
            assert!(session.native_pending_dialog().await.unwrap().is_none());

            assert_eq!(
                session.script("confirm('continue?')").await.unwrap().value,
                serde_json::Value::Bool(false)
            );
            assert_eq!(
                session
                    .native_pending_dialog()
                    .await
                    .unwrap()
                    .unwrap()
                    .dialog_type,
                "confirm"
            );
            assert!(
                session
                    .native_resolve_dialog(PromptDecision::Dismiss)
                    .await
                    .unwrap()
                    .handled
            );

            assert_eq!(
                session
                    .script("prompt('name', 'Glass')")
                    .await
                    .unwrap()
                    .value,
                serde_json::Value::Null
            );
            let pending = session.native_pending_dialog().await.unwrap().unwrap();
            assert_eq!(pending.dialog_type, "prompt");
            assert_eq!(pending.default_value.as_deref(), Some("Glass"));
            assert!(
                session
                    .native_resolve_dialog(PromptDecision::Dismiss)
                    .await
                    .unwrap()
                    .handled
            );
            session.close().await.unwrap();
        });
    });
}

#[test]
fn native_modifier_link_clicks_open_one_background_target_and_honor_cancellation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://modifier-parent",
            "<title>Modifier opener</title><a id='control' href='fixture://modifier-child'>Control link</a><a id='blank' target='_blank' href='fixture://modifier-blank'>Blank link</a><a id='cancel' href='fixture://modifier-canceled'>Canceled link</a><script>globalThis.__modifierTrace = []; for (const id of ['control', 'blank', 'cancel']) document.getElementById(id).addEventListener('click', event => { globalThis.__modifierTrace.push([id, event.altKey, event.ctrlKey, event.metaKey, event.shiftKey]); if (id === 'cancel') event.preventDefault(); });</script>",
        )
        .unwrap()
        .with_fixture("fixture://modifier-child", "<title>Modifier child</title>")
        .unwrap()
        .with_fixture("fixture://modifier-blank", "<title>Blank child</title>")
        .unwrap()
        .with_fixture("fixture://modifier-canceled", "<title>Canceled child</title>")
        .unwrap()
        .with_initial_url("fixture://modifier-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    session
        .action(SemanticAction::ClickWithModifiers {
            target: "id=control".into(),
            modifiers: ClickModifiers {
                alt: true,
                control: true,
                ..ClickModifiers::default()
            },
        })
        .await
        .unwrap();
    let first_targets = session.native_list_targets().await.unwrap();
    assert_eq!(first_targets.len(), 2);
    assert!(
        first_targets
            .iter()
            .any(|target| target.active && target.id == "native-context")
    );
    assert!(first_targets.iter().any(|target| {
        !target.active
            && target.url == "fixture://modifier-child"
            && target.opener_id.as_deref() == Some("native-context")
    }));

    session
        .action(SemanticAction::ClickWithModifiers {
            target: "id=blank".into(),
            modifiers: ClickModifiers {
                meta: true,
                shift: true,
                ..ClickModifiers::default()
            },
        })
        .await
        .unwrap();
    let second_targets = session.native_list_targets().await.unwrap();
    assert_eq!(second_targets.len(), 3);
    assert_eq!(
        second_targets
            .iter()
            .filter(|target| target.url == "fixture://modifier-blank")
            .count(),
        1,
        "modifier activation and target=_blank must create exactly one target"
    );

    session
        .action(SemanticAction::ClickWithModifiers {
            target: "id=cancel".into(),
            modifiers: ClickModifiers {
                control: true,
                ..ClickModifiers::default()
            },
        })
        .await
        .unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 3);
    assert_eq!(
        session
            .script("({url: location.href, trace: globalThis.__modifierTrace})")
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "url": "fixture://modifier-parent",
            "trace": [
                ["control", true, true, false, false],
                ["blank", false, false, true, true],
                ["cancel", false, true, false, false],
            ],
        })
    );
    session.close().await.unwrap();
        });
    });
}

#[test]
fn native_click_expect_popup_returns_causal_target_evidence() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://popup-contract",
            "<title>Opener</title><a id='open' target='_blank' href='fixture://popup-contract-child'>Open popup</a>",
        )
        .unwrap()
        .with_fixture(
            "fixture://popup-contract-child",
            "<title>Popup contract</title><p>ready</p>",
        )
        .unwrap()
        .with_initial_url("fixture://popup-contract");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let before = session.evidence(EvidenceLevel::Compact).await.unwrap();
    let outcome = session
        .native_click_expect_popup("id=open", Some(before.revision))
        .await
        .unwrap();
    assert_eq!(
        outcome.action,
        glass_browser::browser::session::ActionKind::ClickExpectPopup
    );
    assert_eq!(outcome.target_id, "native-context");
    assert_eq!(outcome.frame_id, "native-context:main");
    assert_eq!(outcome.opener_id, "native-context");
    assert_eq!(outcome.popup_id, "native-target-1");
    assert!(outcome.causally_verified_popup);
    assert!(outcome.evidence.trusted_click_witness);
    assert!(outcome.evidence.attached);
    assert_eq!(outcome.evidence.ready_state, "complete");
    assert!(!session.native_list_targets().await.unwrap()[1].active);
    session.close().await.unwrap();
        });
    });
}

#[test]
fn native_window_open_creates_and_reuses_named_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://window-open-parent",
            "<title>Window opener</title><script>globalThis.popup = window.open('fixture://window-open-child', 'report');</script><p>parent content</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-open-child",
            "<title>First report</title><p>first report</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-open-child-next",
            "<title>Second report</title><p>second report</p>",
        )
        .unwrap()
        .with_initial_url("fixture://window-open-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(targets[0].active);
    let popup = &targets[1];
    assert_eq!(popup.id, "native-target-1");
    assert!(!popup.active);
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    assert_eq!(popup.title, "First report");

    session
        .script("window.open('fixture://window-open-child-next', 'report'); 'reused'")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].id, "native-target-1");
    assert_eq!(targets[1].title, "Second report");
    assert!(targets[0].active);

    session
        .script("window.open('fixture://window-open-child', '_self');")
        .await
        .unwrap();
    assert_eq!(
        session
            .evidence(EvidenceLevel::Compact)
            .await
            .unwrap()
            .title,
        "First report"
    );
    assert_eq!(session.native_list_targets().await.unwrap().len(), 2);
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_open_loads_blob_document_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-popup-parent",
            "<title>Blob opener</title><script>globalThis.popup = window.open(URL.createObjectURL(new Blob(['<title>Blob popup</title><p>popup body</p>'], { type: 'text/html' })), 'blob-report');</script><p>parent</p>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-popup-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(targets[0].active);
    assert_eq!(targets[1].opener_id.as_deref(), Some("native-context"));
    assert!(targets[1].url.starts_with("blob:null/glass-native-1"));
    assert_eq!(targets[1].title, "Blob popup");

    session
        .script("popup.location.assign(URL.createObjectURL(new Blob(['<title>Blob popup next</title><p>next body</p>'], { type: 'text/html' }))); 'navigated'")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets[1].title, "Blob popup next");
    assert!(targets[1].url.starts_with("blob:null/glass-native-2"));

    session.native_select_target(&targets[1].id).await.unwrap();
    let evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(evidence.title, "Blob popup next");
    assert_eq!(evidence.visible_text, "next body");
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_identity_exposes_opener_and_persists_name() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://identity-parent",
            "<title>Identity parent</title><script>globalThis.received = []; addEventListener('message', event => { globalThis.received.push([event.data.kind, Boolean(event.source), event.origin]); }); window.name = 'parent'; globalThis.child = window.open('fixture://identity-child', 'report');</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://identity-child",
            "<title>Identity child</title><script>globalThis.observed = [window.name, window.opener !== null, window.opener.name]; window.opener.postMessage({ kind: 'child-ready' }, '*');</script><p>child</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://identity-child-next",
            "<title>Identity child next</title><p>next child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://identity-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child_id = targets
        .iter()
        .find(|target| !target.active)
        .unwrap()
        .id
        .clone();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session.script("globalThis.observed").await.unwrap().value,
        serde_json::json!(["report", true, "parent"])
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([["child-ready", true, "null"]])
    );

    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session
            .script("window.name = 'renamed'; [window.name, window.opener !== null]")
            .await
            .unwrap()
            .value,
        serde_json::json!(["renamed", true])
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    session
        .script("window.open('fixture://identity-child-next', 'renamed'); 'reused'")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].id, child_id);
    assert_eq!(targets[1].title, "Identity child next");
    session.close().await.unwrap();
        });
    });
}

#[test]
fn native_window_proxy_post_message_transfers_message_ports() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://window-message-transfer-parent",
            "<title>Message transfer parent</title><script>globalThis.senderState = []; const legacy = new MessageChannel(); const options = new MessageChannel(); globalThis.child = window.open('fixture://window-message-transfer-child', 'message-transfer-child'); child.postMessage({ kind: 'legacy', port: legacy.port1 }, '*', [legacy.port1]); child.postMessage({ kind: 'options', port: options.port1 }, { targetOrigin: '*', transfer: [options.port1] }); try { legacy.port1.postMessage({ value: 1 }); } catch (error) { senderState.push(error.name); } try { options.port1.postMessage({ value: 2 }); } catch (error) { senderState.push(error.name); }</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-message-transfer-child",
            "<title>Message transfer child</title><script>globalThis.received = []; addEventListener('message', event => { received.push([event.data.kind, event.ports.length, event.data.port === event.ports[0], event.ports[0] instanceof MessagePort, event.origin]); });</script><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://window-message-transfer-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child_id = targets
        .iter()
        .find(|target| !target.active)
        .unwrap()
        .id
        .clone();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([
            ["legacy", 1, true, true, "null"],
            ["options", 1, true, true, "null"],
        ])
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session
            .script("globalThis.senderState")
            .await
            .unwrap()
            .value,
        serde_json::json!(["InvalidStateError", "InvalidStateError"])
    );
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_proxy_array_buffer_transfer_detaches_sender() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://window-array-buffer-transfer-parent",
            "<title>ArrayBuffer transfer parent</title><script>globalThis.senderBufferState = []; const child = window.open('fixture://window-array-buffer-transfer-child', 'array-buffer-transfer-child'); const buffer = new ArrayBuffer(4); new Uint8Array(buffer).set([1, 2, 3, 4]); const view = new Uint8Array(buffer, 1, 2); child.postMessage({ buffer, view }, '*', [buffer]); senderBufferState = [buffer.byteLength, view.byteLength];</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-array-buffer-transfer-child",
            "<title>ArrayBuffer transfer child</title><script>globalThis.receivedBuffers = []; addEventListener('message', event => { const buffer = event.data.buffer; const view = event.data.view; receivedBuffers.push([buffer instanceof ArrayBuffer, buffer.byteLength, Array.from(new Uint8Array(buffer)), view instanceof Uint8Array, view.byteLength, Array.from(view), view.buffer === buffer, event.origin]); });</script><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://window-array-buffer-transfer-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child_id = targets
        .iter()
        .find(|target| !target.active)
        .unwrap()
        .id
        .clone();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session
            .script("globalThis.receivedBuffers")
            .await
            .unwrap()
            .value,
        serde_json::json!([[true, 4, [1, 2, 3, 4], true, 2, [2, 3], true, "null"]])
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session
            .script("globalThis.senderBufferState")
            .await
            .unwrap()
            .value,
        serde_json::json!([0, 0])
    );
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_proxy_message_port_round_trip_reaches_original_peer() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://window-message-port-round-trip-parent",
            "<title>Message port round trip parent</title><script>globalThis.replies = []; const channel = new MessageChannel(); channel.port2.addEventListener('message', event => { replies.push([event.data.kind, event.data.value, event.ports.length, event.data.port === event.ports[0]]); event.ports[0].postMessage({ kind: 'ack', value: 8 }); }); channel.port2.start(); globalThis.child = window.open('fixture://window-message-port-round-trip-child', 'message-port-round-trip-child'); child.postMessage({ kind: 'connect', port: channel.port1 }, '*', [channel.port1]);</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-message-port-round-trip-child",
            "<title>Message port round trip child</title><script>globalThis.received = []; globalThis.responseReply = null; addEventListener('message', event => { const port = event.ports[0]; received = [event.data.kind, event.ports.length, event.data.port === port, port instanceof MessagePort, event.origin]; const response = new MessageChannel(); response.port2.addEventListener('message', event => { responseReply = [event.data.kind, event.data.value]; }); response.port2.start(); port.postMessage({ kind: 'reply', value: 7, port: response.port1 }, [response.port1]); });</script><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://window-message-port-round-trip-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child_id = targets
        .iter()
        .find(|target| !target.active)
        .unwrap()
        .id
        .clone();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!(["connect", 1, true, true, "null"])
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.replies").await.unwrap().value,
        serde_json::json!([["reply", 7, 1, true]])
    );
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session
            .script("globalThis.responseReply")
            .await
            .unwrap()
            .value,
        serde_json::json!(["ack", 8])
    );
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_proxy_message_port_transfers_blob_urls_between_page_realms() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://window-port-blob-parent",
            "<title>Window port Blob parent</title><script>globalThis.replies = []; const channel = new MessageChannel(); channel.port2.onmessage = event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); replies.push([event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]); } catch (error) { replies.push(['error', String(error)]); } }; channel.port2.start(); globalThis.child = window.open('fixture://window-port-blob-child', 'window-port-blob-child'); const blob = new Blob(['parent-window-port-body'], { type: 'text/window-parent' }); child.postMessage({ kind: 'parent-to-child', url: URL.createObjectURL(blob) }, '*', [channel.port1]);</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-port-blob-child",
            "<title>Window port Blob child</title><script>globalThis.received = []; addEventListener('message', event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); received.push([event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]); const blob = new Blob(['child-window-port-body'], { type: 'text/window-child' }); event.ports[0].postMessage({ kind: 'child-to-parent', url: URL.createObjectURL(blob) }); } catch (error) { received.push(['error', String(error)]); } });</script><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://window-port-blob-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let child = session
        .native_list_targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| !target.active)
        .unwrap();
    session.native_select_target(&child.id).await.unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([[
            "parent-to-child",
            "parent-window-port-body",
            "text/window-parent",
            "null",
        ]])
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.replies").await.unwrap().value,
        serde_json::json!([[
            "child-to-parent",
            "child-window-port-body",
            "text/window-child",
            "",
        ]])
    );
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_proxy_message_port_routes_reset_with_navigation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://window-message-port-route-reuse-parent",
            "<title>Message port route reuse parent</title><script>const child = window.open('fixture://window-message-port-route-reuse-child', 'message-port-route-reuse-child'); const channel = new MessageChannel(); child.postMessage({ port: channel.port1 }, '*', [channel.port1]);</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-message-port-route-reuse-child",
            "<title>Message port route reuse child</title><script>addEventListener('message', event => { globalThis.connected = event.ports.length; });</script><p>child</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-message-port-route-reuse-parent-next",
            "<title>Message port route reuse parent next</title><script>const child = window.open('fixture://window-message-port-route-reuse-child-next', 'message-port-route-reuse-child-next'); const channel = new MessageChannel(); child.postMessage({ port: channel.port1 }, '*', [channel.port1]);</script><p>parent next</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://window-message-port-route-reuse-child-next",
            "<title>Message port route reuse child next</title><script>addEventListener('message', event => { globalThis.connected = event.ports.length; });</script><p>child next</p>",
        )
        .unwrap()
        .with_initial_url("fixture://window-message-port-route-reuse-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();
    assert_eq!(session.native_list_targets().await.unwrap().len(), 2);

    session
        .navigate("fixture://window-message-port-route-reuse-parent-next")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 3);
    let next_child_id = targets
        .iter()
        .find(|target| target.title == "Message port route reuse child next")
        .unwrap()
        .id
        .clone();
    session.native_select_target(&next_child_id).await.unwrap();
    assert_eq!(
        session.script("globalThis.connected").await.unwrap().value,
        serde_json::json!(1)
    );
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_proxy_close_removes_popup_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://close-parent",
            "<title>Close parent</title><script>globalThis.popup = window.open('fixture://close-child', 'close-target');</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://close-child",
            "<title>Close child</title><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://close-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    assert_eq!(session.native_list_targets().await.unwrap().len(), 2);
    let result = session
        .script("[popup.closed, (popup.close(), popup.closed)]")
        .await
        .unwrap();
    assert_eq!(result.value, serde_json::json!([false, true]));
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id, "native-context");
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_proxy_location_navigates_parked_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://location-parent",
            "<title>Location parent</title><script>globalThis.popup = window.open('fixture://location-child', 'location-target');</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://location-child",
            "<title>Location child</title><script>globalThis.observedOpener = opener.location.href;</script><p>child</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://location-child-next",
            "<title>Location child next</title><p>next</p>",
        )
        .unwrap()
        .with_initial_url("fixture://location-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let child = session
        .native_list_targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| !target.active)
        .unwrap();
    session.native_select_target(&child.id).await.unwrap();
    assert_eq!(
        session
            .script("globalThis.observedOpener")
            .await
            .unwrap()
            .value,
        serde_json::json!("fixture://location-parent")
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();

    assert_eq!(
        session
            .script("[popup.location.href, popup.location.toString()]")
            .await
            .unwrap()
            .value,
        serde_json::json!(["fixture://location-child", "fixture://location-child"])
    );
    session
        .script("popup.location.assign('fixture://location-child-next'); true")
        .await
        .unwrap();
    let targets = session.native_list_targets().await.unwrap();
    let child = targets.iter().find(|target| !target.active).unwrap();
    assert_eq!(child.url, "fixture://location-child-next");
    assert_eq!(child.title, "Location child next");
    session.close().await.unwrap();

        });
    });
}

#[test]
fn native_window_proxy_identity_tracks_navigation_and_close() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://proxy-identity-parent",
            "<title>Proxy identity parent</title><script>globalThis.popup = window.open('fixture://proxy-identity-child', 'proxy-identity-target');</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://proxy-identity-parent-next",
            "<title>Proxy identity parent next</title><p>parent next</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://proxy-identity-child",
            "<title>Proxy identity child</title><p>child</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://proxy-identity-child-next",
            "<title>Proxy identity child next</title><p>child next</p>",
        )
        .unwrap()
        .with_initial_url("fixture://proxy-identity-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let child_id = session
        .native_list_targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| !target.active)
        .unwrap()
        .id;
    session.native_select_target(&child_id).await.unwrap();
    session
        .navigate("fixture://proxy-identity-child-next")
        .await
        .unwrap();
    assert_eq!(
        session
            .native_list_targets()
            .await
            .unwrap()
            .into_iter()
            .find(|target| target.id == child_id)
            .unwrap()
            .url,
        "fixture://proxy-identity-child-next"
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session
            .script("[popup.location.href, popup.closed]")
            .await
            .unwrap()
            .value,
        serde_json::json!(["fixture://proxy-identity-child-next", false])
    );

    session
        .navigate("fixture://proxy-identity-parent-next")
        .await
        .unwrap();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session.script("opener.location.href").await.unwrap().value,
        serde_json::json!("fixture://proxy-identity-parent-next")
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    session.native_close_target("native-context").await.unwrap();
    session.native_select_target(&child_id).await.unwrap();
    assert_eq!(
        session.script("opener.closed").await.unwrap().value,
        serde_json::json!(true)
    );
    session.close().await.unwrap();

        });
    });
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
            NativeEventKind::FocusIn,
            NativeEventKind::Click,
            NativeEventKind::Input,
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
    let multi_selected = engine
        .action(NativeAction::Click {
            target: "id=many-one".into(),
        })
        .unwrap();
    assert_eq!(multi_selected.revision, 6);
    assert_eq!(
        engine
            .semantic_nodes()
            .unwrap()
            .iter()
            .filter(|node| node.role == "option")
            .nth(2)
            .and_then(|node| node.selected),
        Some(true)
    );
    let revision_before_rejections = engine.revision();
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

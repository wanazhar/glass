use super::*;

#[test]
fn native_local_tab_index_property_reflects_defaults_and_focus_order() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://tabindex-idl",
            "<div id='generic'></div><a id='anchor'>No href</a><button id='button' type='button'>Button</button><details><summary id='summary-first'>First</summary><summary id='summary-second'>Second</summary></details><input id='hidden-input' type='hidden'><div id='invalid' tabindex='bad'></div><div id='overflow'></div>",
        )
        .unwrap()
        .with_initial_url("fixture://tabindex-idl");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const generic = document.getElementById('generic'); const anchor = document.getElementById('anchor'); const button = document.getElementById('button'); const firstSummary = document.getElementById('summary-first'); const secondSummary = document.getElementById('summary-second'); const hiddenInput = document.getElementById('hidden-input'); const invalid = document.getElementById('invalid'); const overflow = document.getElementById('overflow'); const defaults = [generic.tabIndex, anchor.tabIndex, button.tabIndex, firstSummary.tabIndex, secondSummary.tabIndex, hiddenInput.tabIndex, invalid.tabIndex]; generic.tabIndex = 2.9; const truncation = [generic.tabIndex, generic.getAttribute('tabindex')]; generic.tabIndex = 4294967297; const wrapping = [generic.tabIndex, generic.getAttribute('tabindex')]; generic.tabIndex = -2.9; const negative = [generic.tabIndex, generic.getAttribute('tabindex')]; anchor.setAttribute('tabindex', 'invalid'); overflow.setAttribute('tabindex', '2147483648'); const invalidFallbacks = [anchor.tabIndex, overflow.tabIndex]; anchor.removeAttribute('tabindex'); generic.tabIndex = 2; return { defaults, truncation, wrapping, negative, invalidFallbacks, assigned: [generic.tabIndex, generic.getAttribute('tabindex')] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "defaults": [-1, 0, 0, 0, -1, 0, -1],
            "truncation": [2, "2"],
            "wrapping": [1, "1"],
            "negative": [-2, "-2"],
            "invalidFallbacks": [0, -1],
            "assigned": [2, "2"],
        })
    );
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("generic")
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_link_default_actions_open_blank_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://keyboard-popup-parent",
            "<title>Keyboard opener</title><a id='open' target='_blank' href='fixture://keyboard-popup-child'>Open popup</a>",
        )
        .unwrap()
        .with_fixture(
            "fixture://keyboard-popup-child",
            "<title>Keyboard popup</title><p>keyboard popup</p>",
        )
        .unwrap()
        .with_initial_url("fixture://keyboard-popup-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();
    session
        .script("document.getElementById('open').focus(); true")
        .await
        .unwrap();

    let outcome = session
        .action(SemanticAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert!(outcome.accepted);
    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(targets[0].active);
    assert_eq!(targets[0].url, "fixture://keyboard-popup-parent");
    let popup = &targets[1];
    assert!(!popup.active);
    assert_eq!(popup.url, "fixture://keyboard-popup-child");
    assert_eq!(popup.title, "Keyboard popup");
    assert_eq!(popup.opener_id.as_deref(), Some("native-context"));
    session.close().await.unwrap();

        });
    });
}
#[test]
fn native_local_blank_link_honors_click_cancellation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://cancel-popup-parent",
            "<script>document.addEventListener('click', event => event.preventDefault());</script><title>Opener</title><a id='open' target='_blank' href='fixture://cancel-popup-child'>Open popup</a>",
        )
        .unwrap()
        .with_fixture(
            "fixture://cancel-popup-child",
            "<title>Should not open</title><p>blocked</p>",
        )
        .unwrap()
        .with_initial_url("fixture://cancel-popup-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let action = session
        .action(SemanticAction::Click {
            target: "id=open".into(),
        })
        .await
        .unwrap();
    assert!(action.accepted);
    assert_eq!(session.native_list_targets().await.unwrap().len(), 1);
    assert_eq!(
        session.evidence(EvidenceLevel::Compact).await.unwrap().url,
        "fixture://cancel-popup-parent"
    );
    session.close().await.unwrap();
        });
    });
}
#[test]
fn native_local_form_data_iterators_are_live_and_self_iterating() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://form-data-live", "<p>FormData</p>")
        .unwrap()
        .with_initial_url("fixture://form-data-live");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(() => {
                const form = new FormData();
                form.append('a', 'one');
                form.append('b', 'two');
                const entries = form.entries();
                const first = entries.next();
                form.set('b', 'updated');
                form.append('c', 'three');
                const second = entries.next();
                const third = entries.next();
                const complete = entries.next();
                const keys = form.keys();
                const firstKey = keys.next();
                form.append('d', 'four');
                const remainingKeys = [keys.next().value, keys.next().value, keys.next().value, keys.next().done];
                globalThis.liveFormData = [
                    form[Symbol.iterator] === form.entries,
                    entries[Symbol.iterator]() === entries,
                    first.value,
                    first.done,
                    second.value,
                    third.value,
                    complete.done,
                    firstKey.value,
                    remainingKeys,
                ];
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.liveFormData")
            .await
            .unwrap(),
        serde_json::json!([
            true,
            true,
            ["a", "one"],
            false,
            ["b", "updated"],
            ["c", "three"],
            true,
            "a",
            ["b", "c", "d", true],
        ])
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_transform_streams_process_pipe_through_values() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://transform-streams", "<p>Transform streams</p>")
        .unwrap()
        .with_initial_url("fixture://transform-streams");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            r#"(async () => {
                const events = [];
                const transform = new TransformStream({
                    start() { events.push('start'); },
                    transform(value, controller) {
                        events.push('transform:' + value);
                        controller.enqueue(value.toUpperCase());
                    },
                    flush(controller) {
                        events.push('flush');
                        controller.enqueue('tail');
                    },
                });
                const source = new ReadableStream({
                    start(controller) {
                        controller.enqueue('one');
                        controller.enqueue('two');
                        controller.close();
                    },
                });
                const output = source.pipeThrough(transform);
                const reader = output.getReader();
                const first = await reader.read();
                const second = await reader.read();
                const tail = await reader.read();
                const end = await reader.read();
                reader.releaseLock();
                globalThis.transformStreamResult = {
                    events,
                    values: [first.value, second.value, tail.value, end.done],
                    identities: [output === transform.readable, transform.writable instanceof WritableStream, output instanceof ReadableStream],
                    sourceLocked: source.locked,
                    outputLocked: output.locked,
                };
            })()"#,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.transformStreamResult")
            .await
            .unwrap(),
        serde_json::json!({
            "events": ["start", "transform:one", "transform:two", "flush"],
            "values": ["ONE", "TWO", "tail", true],
            "identities": [true, true, true],
            "sourceLocked": false,
            "outputLocked": false
        })
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_script_owns_event_listeners_and_focus_order() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://script-events",
            "<div id='outer'><section id='middle'><input id='toggle' type='checkbox'></section></div><input id='name' type='text'>",
        )
        .unwrap()
        .with_initial_url("fixture://script-events");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    let initial_revision = engine.revision();

    let registered = engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); const outer = document.getElementById('outer'); const middle = document.getElementById('middle'); globalThis.events = []; globalThis.onOuterCapture = (event) => { globalThis.events.push('capture:' + event.currentTarget.id + ':' + event.eventPhase); }; globalThis.onMiddleCapture = (event) => { globalThis.events.push('capture:' + event.currentTarget.id + ':' + event.eventPhase); }; globalThis.onOuterBubble = (event) => { globalThis.events.push('bubble:' + event.currentTarget.id + ':' + event.eventPhase); }; globalThis.onToggleClick = (event) => { globalThis.events.push(event.type + ':' + event.target.id + ':' + event.currentTarget.id); document.getElementById('name').value = 'from-listener'; event.preventDefault(); }; globalThis.onProbe = (event) => { globalThis.events.push(event.type + ':' + event.detail); }; outer.addEventListener('click', globalThis.onOuterCapture, true); middle.addEventListener('click', globalThis.onMiddleCapture, { capture: true }); outer.addEventListener('click', globalThis.onOuterBubble); toggle.addEventListener('click', globalThis.onToggleClick); toggle.addEventListener('probe', globalThis.onProbe); toggle.focus(); const accepted = toggle.dispatchEvent(new CustomEvent('probe', { cancelable: true, detail: 'detail' })); return { active: document.activeElement.id, accepted, eventType: new Event('probe').type, parent: toggle.parentElement.id, events: globalThis.events }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        registered,
        serde_json::json!({
            "active": "toggle",
            "accepted": true,
            "eventType": "probe",
            "parent": "middle",
            "events": ["probe:detail"],
        })
    );
    assert_eq!(engine.revision(), initial_revision + 1);

    let prevented = engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); return { checked: (toggle.click(), toggle.checked), value: document.getElementById('name').value, events: globalThis.events }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        prevented,
        serde_json::json!({
            "checked": false,
            "value": "from-listener",
            "events": [
                "probe:detail",
                "capture:outer:1",
                "capture:middle:1",
                "click:toggle:toggle",
                "bubble:outer:3",
            ],
        })
    );
    assert_eq!(engine.revision(), initial_revision + 2);

    let accepted = engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); toggle.removeEventListener('click', globalThis.onToggleClick); toggle.click(); return toggle.checked; })()",
        )
        .await
        .unwrap();
    assert_eq!(accepted, serde_json::json!(true));
    assert_eq!(engine.revision(), initial_revision + 3);

    let focus = engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); field.focus(); field.blur(); return { active: document.activeElement, focused: field.focused }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        focus,
        serde_json::json!({
            "active": null,
            "focused": false,
        })
    );
    assert_eq!(engine.revision(), initial_revision + 4);
    let effects = engine.effects_since(initial_revision).unwrap();
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::Focus)
    );
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::Blur)
    );
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::Click)
    );
    let before_action = engine.revision();
    engine
        .evaluate_async(
            "(() => { globalThis.onActionClick = (event) => { globalThis.events.push('action:' + event.target.id); document.getElementById('name').value = 'from-action'; }; document.getElementById('toggle').addEventListener('click', globalThis.onActionClick); })()",
        )
        .await
        .unwrap();
    let action = engine
        .action(NativeAction::Click {
            target: "id=toggle".into(),
        })
        .unwrap();
    assert!(action.accepted);
    assert_eq!(action.revision, before_action + 1);
    assert_eq!(engine.revision(), before_action + 1);
    assert_eq!(
        engine
            .evaluate_async(
                "({ checked: document.getElementById('toggle').checked, value: document.getElementById('name').value, action: globalThis.events.includes('action:toggle') })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "checked": false,
            "value": "from-action",
            "action": true,
        })
    );
    let before_cancel = engine.revision();
    engine
        .evaluate_async(
            "(() => { const toggle = document.getElementById('toggle'); toggle.removeEventListener('click', globalThis.onActionClick); globalThis.onCancelClick = (event) => { globalThis.events.push('cancel:' + event.target.id); event.preventDefault(); }; toggle.addEventListener('click', globalThis.onCancelClick); })()",
        )
        .await
        .unwrap();
    let canceled = engine
        .action(NativeAction::Click {
            target: "id=toggle".into(),
        })
        .unwrap();
    assert!(canceled.accepted);
    assert_eq!(canceled.revision, before_cancel + 1);
    assert_eq!(engine.revision(), before_cancel + 1);
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('toggle').checked")
            .await
            .unwrap(),
        serde_json::json!(false)
    );
    let before_type = engine.revision();
    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); globalThis.typeEvents = []; field.addEventListener('focus', (event) => globalThis.typeEvents.push(event.type)); field.addEventListener('input', (event) => { globalThis.typeEvents.push(event.type); field.setAttribute('data-input', 'seen'); }); field.addEventListener('change', (event) => globalThis.typeEvents.push(event.type)); })()",
        )
        .await
        .unwrap();
    let typed = engine
        .action(NativeAction::Type {
            target: "id=name".into(),
            text: "typed-by-action".into(),
        })
        .unwrap();
    assert!(typed.accepted);
    assert_eq!(typed.revision, before_type + 1);
    assert_eq!(engine.revision(), before_type + 1);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, marker: document.getElementById('name').getAttribute('data-input'), events: globalThis.typeEvents })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "value": "typed-by-action",
            "marker": "seen",
            "events": ["focus", "input", "change"],
        })
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_pointer_actions_dispatch_browser_event_order() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://pointer-actions",
            "<button id='button' style='width:100px;height:24px'>Button</button><div id='source' draggable='true' style='width:100px;height:24px'>Source</div><div id='destination' style='width:100px;height:24px'>Destination</div>",
        )
        .unwrap()
        .with_initial_url("fixture://pointer-actions");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(
            "(() => { const button = document.getElementById('button'); const source = document.getElementById('source'); const destination = document.getElementById('destination'); globalThis.pointerEvents = []; globalThis.doubleClicks = 0; button.addEventListener('click', () => globalThis.doubleClicks += 1); button.addEventListener('mouseover', event => pointerEvents.push(event.type + ':' + event.target.id + ':' + event.bubbles + ':' + event.cancelable)); button.addEventListener('mouseenter', event => pointerEvents.push(event.type + ':' + event.target.id + ':' + event.bubbles + ':' + event.cancelable)); source.addEventListener('dragstart', event => pointerEvents.push(event.type + ':' + event.target.id + ':' + event.bubbles + ':' + event.cancelable)); destination.addEventListener('dragenter', event => pointerEvents.push(event.type + ':' + event.target.id + ':' + event.bubbles + ':' + event.cancelable)); destination.addEventListener('dragover', event => pointerEvents.push(event.type + ':' + event.target.id + ':' + event.bubbles + ':' + event.cancelable)); destination.addEventListener('drop', event => pointerEvents.push(event.type + ':' + event.target.id + ':' + event.bubbles + ':' + event.cancelable)); source.addEventListener('dragend', event => pointerEvents.push(event.type + ':' + event.target.id + ':' + event.bubbles + ':' + event.cancelable)); })()",
        )
        .await
        .unwrap();

    let double_clicked = engine
        .action(NativeAction::DoubleClick {
            target: "id=button".into(),
        })
        .unwrap();
    assert!(double_clicked.accepted);
    assert_eq!(
        engine
            .evaluate_async("globalThis.doubleClicks")
            .await
            .unwrap(),
        serde_json::json!(2)
    );

    let hovered = engine
        .action(NativeAction::Hover {
            target: "id=button".into(),
        })
        .unwrap();
    assert!(hovered.accepted);

    let dragged = engine
        .action(NativeAction::Drag {
            source: "id=source".into(),
            destination: "id=destination".into(),
        })
        .unwrap();
    assert!(dragged.accepted);
    assert_eq!(
        engine
            .evaluate_async("globalThis.pointerEvents")
            .await
            .unwrap(),
        serde_json::json!([
            "mouseover:button:true:true",
            "mouseenter:button:false:false",
            "dragstart:source:true:true",
            "dragenter:destination:true:true",
            "dragover:destination:true:true",
            "drop:destination:true:true",
            "dragend:source:true:false",
        ])
    );
    let effects = engine.effects_since(0).unwrap();
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::MouseOver)
    );
    assert!(
        effects
            .effects
            .iter()
            .any(|effect| effect.kind == NativeEventKind::DragEnd)
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_upload_populates_file_list_and_dispatches_input_change() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://upload",
            "<form id='form'><input id='upload' type='file' name='asset' multiple></form><script>globalThis.uploadEvents = []; const input = document.getElementById('upload'); input.addEventListener('input', event => uploadEvents.push(event.type + ':' + event.target.id)); input.addEventListener('change', event => uploadEvents.push(event.type + ':' + event.target.id));</script>",
        )
        .unwrap()
        .with_initial_url("fixture://upload");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let uploaded = engine
        .action(NativeAction::Upload {
            target: "id=upload".into(),
            files: vec![NativeFile {
                name: "payload.bin".into(),
                media_type: "application/octet-stream".into(),
                last_modified: 1234,
                bytes: vec![0, 255, 128, b'G'],
            }],
        })
        .unwrap();
    assert!(uploaded.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "await (async () => { const input = document.getElementById('upload'); const file = input.files.item(0); const formFile = new FormData(document.getElementById('form')).get('asset'); return { list: input.files instanceof FileList, length: input.files.length, itemMissing: input.files.item(1), name: file.name, type: file.type, size: file.size, modified: file.lastModified, bytes: Array.from(await file.bytes()), value: input.value, formFile: [formFile instanceof File, formFile.name, formFile.size, Array.from(await formFile.bytes())], events: globalThis.uploadEvents }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "list": true,
            "length": 1,
            "itemMissing": null,
            "name": "payload.bin",
            "type": "application/octet-stream",
            "size": 4,
            "modified": 1234,
            "bytes": [0, 255, 128, 71],
            "value": r"C:\fakepath\payload.bin",
            "formFile": [true, "payload.bin", 4, [0, 255, 128, 71]],
            "events": ["input:upload", "change:upload"],
        })
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keypress_edits_focused_text_and_honors_keydown_cancel() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://keyboard",
            "<input id='name' type='text' value='ab' style='width:120px;height:20px'>",
        )
        .unwrap()
        .with_initial_url("fixture://keyboard");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); globalThis.keyEvents = []; field.addEventListener('keydown', event => globalThis.keyEvents.push(event.type + ':' + event.key + ':' + event.code)); field.addEventListener('input', event => globalThis.keyEvents.push(event.type + ':' + field.value)); field.addEventListener('keyup', event => globalThis.keyEvents.push(event.type + ':' + event.key)); })()",
        )
        .await
        .unwrap();

    engine
        .action(NativeAction::Click {
            target: "id=name".into(),
        })
        .unwrap();
    let typed = engine
        .action(NativeAction::KeyPress { key: "!".into() })
        .unwrap();
    assert!(typed.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, events: globalThis.keyEvents })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "value": "ab!",
            "events": ["keydown:!:!", "input:ab!", "keyup:!"],
        })
    );

    let backspaced = engine
        .action(NativeAction::KeyPress {
            key: "Backspace".into(),
        })
        .unwrap();
    assert!(backspaced.accepted);
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('name').value")
            .await
            .unwrap(),
        serde_json::json!("ab")
    );

    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); field.addEventListener('keydown', event => { if (event.key === 'x') event.preventDefault(); }); })()",
        )
        .await
        .unwrap();
    let canceled = engine
        .action(NativeAction::KeyPress { key: "x".into() })
        .unwrap();
    assert!(canceled.accepted);
    assert_eq!(
        engine
            .evaluate_async("({ value: document.getElementById('name').value, last: globalThis.keyEvents.slice(-2) })")
            .await
            .unwrap(),
        serde_json::json!({
            "value": "ab",
            "last": ["keydown:x:KeyX", "keyup:x"],
        })
    );

    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); field.setSelectionRange(0, 1, 'forward'); return [field.selectionStart, field.selectionEnd, field.selectionDirection]; })()",
        )
        .await
        .unwrap();
    let replaced = engine
        .action(NativeAction::Shortcut {
            shortcut: "!".into(),
        })
        .unwrap();
    assert!(replaced.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, selectionStart: document.getElementById('name').selectionStart, selectionEnd: document.getElementById('name').selectionEnd })",
            )
            .await
            .unwrap(),
        serde_json::json!({"value": "!b", "selectionStart": 1, "selectionEnd": 1})
    );
    engine
        .evaluate_async(
            "(() => { const field = document.getElementById('name'); globalThis.shortcutEvents = []; field.addEventListener('keydown', event => globalThis.shortcutEvents.push([event.key, event.code, event.altKey, event.ctrlKey, event.metaKey, event.shiftKey])); field.addEventListener('keyup', event => globalThis.shortcutEvents.push([event.key, event.code, event.altKey, event.ctrlKey, event.metaKey, event.shiftKey])); globalThis.rawKeyEvents = []; field.addEventListener('keydown', event => globalThis.rawKeyEvents.push('down:' + event.key)); field.addEventListener('keyup', event => globalThis.rawKeyEvents.push('up:' + event.key)); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('name').setSelectionRange(0, 2, 'forward')")
        .await
        .unwrap();
    let extended = engine
        .action(NativeAction::Shortcut {
            shortcut: "Shift+ArrowLeft".into(),
        })
        .unwrap();
    assert!(extended.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "({ selectionStart: document.getElementById('name').selectionStart, selectionEnd: document.getElementById('name').selectionEnd, selectionDirection: document.getElementById('name').selectionDirection })",
            )
            .await
            .unwrap(),
        serde_json::json!({"selectionStart": 0, "selectionEnd": 1, "selectionDirection": "forward"})
    );
    engine
        .evaluate_async("globalThis.shortcutEvents = []")
        .await
        .unwrap();
    let before_shortcut = engine.revision();
    let shortcut = engine
        .action(NativeAction::Shortcut {
            shortcut: "Ctrl+a".into(),
        })
        .unwrap();
    assert!(shortcut.accepted);
    assert_eq!(shortcut.revision, before_shortcut + 1);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, selectionStart: document.getElementById('name').selectionStart, selectionEnd: document.getElementById('name').selectionEnd, selectionDirection: document.getElementById('name').selectionDirection, events: globalThis.shortcutEvents })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "value": "!b",
            "selectionStart": 0,
            "selectionEnd": 2,
            "selectionDirection": "forward",
            "events": [
                ["a", "KeyA", false, true, false, false],
                ["a", "KeyA", false, true, false, false],
            ],
        })
    );

    let deleted = engine
        .action(NativeAction::Shortcut {
            shortcut: "Backspace".into(),
        })
        .unwrap();
    assert!(deleted.accepted);
    assert_eq!(
        engine
            .evaluate_async(
                "({ value: document.getElementById('name').value, selectionStart: document.getElementById('name').selectionStart, selectionEnd: document.getElementById('name').selectionEnd })",
            )
            .await
            .unwrap(),
        serde_json::json!({"value": "", "selectionStart": 0, "selectionEnd": 0})
    );

    let before_keydown = engine.revision();
    let keydown = engine
        .action(NativeAction::KeyDown {
            key: "Enter".into(),
        })
        .unwrap();
    assert!(keydown.accepted);
    assert_eq!(keydown.revision, before_keydown + 1);
    let keyup = engine
        .action(NativeAction::KeyUp {
            key: "Enter".into(),
        })
        .unwrap();
    assert!(keyup.accepted);
    assert_eq!(keyup.revision, before_keydown + 2);
    assert_eq!(
        engine
            .evaluate_async("globalThis.rawKeyEvents.slice(-2)")
            .await
            .unwrap(),
        serde_json::json!(["down:Enter", "up:Enter"])
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_button_activation_preserves_key_phases_and_shortcut_revision() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-keyboard-button",
            "<button id='button' type='button'>Activate</button><button id='other' type='button'>Other</button><script>globalThis.events = []; globalThis.cancelEnter = false; globalThis.cancelClick = false; const button = document.getElementById('button'); button.addEventListener('keydown', event => { events.push('keydown:' + event.key); if (cancelEnter && event.key === 'Enter') event.preventDefault(); }); button.addEventListener('keyup', event => events.push('keyup:' + event.key)); button.addEventListener('click', event => { events.push('click'); if (cancelClick) event.preventDefault(); });</script>",
        )
        .unwrap()
        .with_initial_url("fixture://local-keyboard-button");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('button').focus()")
        .await
        .unwrap();

    let before_enter = engine.revision();
    let enter = engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();
    assert!(enter.accepted);
    assert_eq!(enter.revision, before_enter + 1);
    assert_eq!(engine.revision(), before_enter + 1);
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["keydown:Enter", "click", "keyup:Enter"])
    );

    engine
        .evaluate_async("globalThis.events = []")
        .await
        .unwrap();
    let before_space = engine.revision();
    let space = engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert!(space.accepted);
    assert_eq!(space.revision, before_space + 1);
    assert_eq!(engine.revision(), before_space + 1);
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["keydown: ", "keyup: ", "click"])
    );

    engine
        .evaluate_async("globalThis.events = []; globalThis.cancelEnter = true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["keydown:Enter", "keyup:Enter"])
    );

    engine
        .evaluate_async(
            "globalThis.events = []; globalThis.cancelEnter = false; globalThis.cancelClick = true",
        )
        .await
        .unwrap();
    let before_canceled_click = engine.revision();
    let canceled_click = engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();
    assert_eq!(canceled_click.revision, before_canceled_click + 1);
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["keydown:Enter", "click", "keyup:Enter"])
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_link_activation_obeys_cancelation_and_live_href() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-keyboard-link",
            "<a id='link' role='button' href='fixture://wrong-destination'>Open</a><script>globalThis.events = []; globalThis.cancelKeydown = false; globalThis.cancelClick = false; globalThis.mutateHref = false; const link = document.getElementById('link'); link.addEventListener('keydown', event => { events.push('keydown:' + event.key); if (cancelKeydown && event.key === 'Enter') event.preventDefault(); }); link.addEventListener('click', event => { events.push('click'); if (cancelClick) event.preventDefault(); if (mutateHref) link.setAttribute('href', 'fixture://keyboard-link-destination'); }); link.addEventListener('keyup', event => events.push('keyup:' + event.key));</script>",
        )
        .unwrap()
        .with_fixture(
            "fixture://keyboard-link-destination",
            "<p id='destination'>Keyboard destination</p>",
        )
        .unwrap()
        .with_fixture("fixture://wrong-destination", "<p>Wrong destination</p>")
        .unwrap()
        .with_initial_url("fixture://local-keyboard-link");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('link').focus()")
        .await
        .unwrap();

    engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["keydown: ", "keyup: "])
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://local-keyboard-link"
    );

    engine
        .evaluate_async("globalThis.events = []; globalThis.cancelKeydown = true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["keydown:Enter", "keyup:Enter"])
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://local-keyboard-link"
    );

    engine
        .evaluate_async("globalThis.events = []; globalThis.cancelKeydown = false; globalThis.cancelClick = true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["keydown:Enter", "click", "keyup:Enter"])
    );
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://local-keyboard-link"
    );

    engine
        .evaluate_async(
            "globalThis.events = []; globalThis.cancelClick = false; globalThis.mutateHref = true",
        )
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://keyboard-link-destination"
    );
    assert_eq!(
        engine.snapshot().unwrap().visible_text,
        "Keyboard destination"
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_button_activation_raw_space_is_focus_bound() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-raw-space",
            "<button id='first' type='button'>First</button><button id='second' type='button'>Second</button><script>globalThis.events = []; for (const button of document.querySelectorAll('button')) { button.addEventListener('keydown', event => events.push(button.id + ':down:' + event.key)); button.addEventListener('keyup', event => events.push(button.id + ':up:' + event.key)); button.addEventListener('click', () => events.push(button.id + ':click')); }</script>",
        )
        .unwrap()
        .with_fixture(
            "fixture://raw-space-replaced",
            "<button id='new' type='button'>New</button><script>globalThis.clicks = 0; document.getElementById('new').addEventListener('click', () => clicks++);</script>",
        )
        .unwrap()
        .with_initial_url("fixture://local-raw-space");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('first').focus()")
        .await
        .unwrap();

    let before_down = engine.revision();
    let down = engine
        .action(NativeAction::KeyDown {
            key: "Space".into(),
        })
        .unwrap();
    assert_eq!(down.revision, before_down + 1);
    let up = engine
        .action(NativeAction::KeyUp {
            key: "Space".into(),
        })
        .unwrap();
    assert_eq!(up.revision, before_down + 2);
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["first:down: ", "first:up: ", "first:click"])
    );

    engine
        .evaluate_async("globalThis.events = []; document.getElementById('first').focus()")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyDown {
            key: "Space".into(),
        })
        .unwrap();
    engine
        .evaluate_async("document.getElementById('second').focus()")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyUp {
            key: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["first:down: ", "second:up: "])
    );

    engine
        .evaluate_async("document.getElementById('first').focus()")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyDown {
            key: "Space".into(),
        })
        .unwrap();
    engine
        .evaluate_async("document.getElementById('first').disabled = true")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyUp {
            key: "Space".into(),
        })
        .unwrap();
    assert!(
        !engine
            .evaluate_async("globalThis.events.includes('first:click')")
            .await
            .unwrap()
            .as_bool()
            .unwrap()
    );

    engine
        .evaluate_async("document.getElementById('first').disabled = false; document.getElementById('first').focus(); globalThis.events = []")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyDown {
            key: "Space".into(),
        })
        .unwrap();
    engine
        .evaluate_async("document.getElementById('first').remove()")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyUp {
            key: "Space".into(),
        })
        .unwrap();
    assert!(
        !engine
            .evaluate_async("globalThis.events.includes('first:click')")
            .await
            .unwrap()
            .as_bool()
            .unwrap()
    );

    engine
        .navigate_async("fixture://local-raw-space")
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('first').focus()")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyDown {
            key: "Space".into(),
        })
        .unwrap();
    engine
        .navigate_async("fixture://raw-space-replaced")
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('new').focus()")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyUp {
            key: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.clicks").await.unwrap(),
        serde_json::json!(0)
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_checkable_activation_orders_events_and_restores_canceled_state() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-keyboard-checkable",
            "<form id='form'><input id='required' required><input id='checkbox' type='checkbox'><input id='radio-a' type='radio' name='group' checked><input id='radio-b' type='radio' name='group'><input id='radio-c' type='radio' name='other'></form><input id='radio-outside' type='radio' name='group' checked><input id='radio-unnamed' type='radio'>",
        )
        .unwrap()
        .with_initial_url("fixture://local-keyboard-checkable");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.trace = []; globalThis.cancelTarget = ''; globalThis.removeRadioNameOnClick = false; globalThis.joinRadioOnClick = false; document.addEventListener('keydown', event => trace.push('keydown:' + event.target.id + ':' + event.key)); document.addEventListener('keyup', event => trace.push('keyup:' + event.target.id + ':' + event.key)); document.addEventListener('click', event => { trace.push('click:' + event.target.id + ':' + event.target.checked + ':' + event.target.indeterminate + ':' + event.cancelable); if (removeRadioNameOnClick && event.target.id === 'radio-b') { event.target.removeAttribute('name'); event.preventDefault(); } else if (joinRadioOnClick && event.target.id === 'radio-b') { const radioC = document.getElementById('radio-c'); radioC.setAttribute('name', 'group'); radioC.checked = true; event.preventDefault(); } else if (cancelTarget === event.target.id) event.preventDefault(); }); document.addEventListener('input', event => trace.push('input:' + event.target.id + ':' + event.bubbles)); document.addEventListener('change', event => trace.push('change:' + event.target.id + ':' + event.bubbles)); document.addEventListener('submit', () => trace.push('submit')); document.addEventListener('invalid', () => trace.push('invalid')); true",
        )
        .await
        .unwrap();

    engine
        .evaluate_async("const checkbox = document.getElementById('checkbox'); checkbox.indeterminate = true; checkbox.focus(); true")
        .await
        .unwrap();
    let before_shortcut = engine.revision();
    let result = engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(result.revision, before_shortcut + 1);
    assert_eq!(engine.revision(), before_shortcut + 1);
    assert_eq!(
        engine.evaluate_async("globalThis.trace").await.unwrap(),
        serde_json::json!([
            "keydown:checkbox: ",
            "keyup:checkbox: ",
            "click:checkbox:true:false:true",
            "input:checkbox:true",
            "change:checkbox:true",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("({checked: checkbox.checked, indeterminate: checkbox.indeterminate})")
            .await
            .unwrap(),
        serde_json::json!({"checked": true, "indeterminate": false})
    );

    engine
        .evaluate_async("globalThis.trace = []; globalThis.cancelTarget = 'checkbox'; checkbox.checked = false; checkbox.indeterminate = true; checkbox.focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.trace").await.unwrap(),
        serde_json::json!([
            "keydown:checkbox: ",
            "keyup:checkbox: ",
            "click:checkbox:true:false:true",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("({checked: checkbox.checked, indeterminate: checkbox.indeterminate})")
            .await
            .unwrap(),
        serde_json::json!({"checked": false, "indeterminate": true})
    );

    engine
        .evaluate_async(
            "globalThis.trace = []; globalThis.cancelTarget = ''; checkbox.focus(); true",
        )
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.trace").await.unwrap(),
        serde_json::json!(["keydown:checkbox:Enter", "keyup:checkbox:Enter"])
    );
    assert_eq!(
        engine
            .evaluate_async("({checked: checkbox.checked, indeterminate: checkbox.indeterminate})")
            .await
            .unwrap(),
        serde_json::json!({"checked": false, "indeterminate": true})
    );

    engine
        .evaluate_async("const radioA = document.getElementById('radio-a'); const radioB = document.getElementById('radio-b'); const radioOutside = document.getElementById('radio-outside'); globalThis.trace = []; globalThis.cancelTarget = 'radio-b'; radioB.focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "({a: radioA.checked, b: radioB.checked, outside: radioOutside.checked})"
            )
            .await
            .unwrap(),
        serde_json::json!({"a": true, "b": false, "outside": true})
    );
    assert_eq!(
        engine.evaluate_async("globalThis.trace").await.unwrap(),
        serde_json::json!([
            "keydown:radio-b: ",
            "keyup:radio-b: ",
            "click:radio-b:true:false:true",
        ])
    );

    engine
        .evaluate_async("globalThis.trace = []; globalThis.cancelTarget = ''; radioB.focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({a: radioA.checked, b: radioB.checked, outside: radioOutside.checked, trace: globalThis.trace})")
            .await
            .unwrap(),
        serde_json::json!({
            "a": false,
            "b": true,
            "outside": true,
            "trace": [
                "keydown:radio-b: ",
                "keyup:radio-b: ",
                "click:radio-b:true:false:true",
                "input:radio-b:true",
                "change:radio-b:true",
            ],
        })
    );

    engine
        .evaluate_async("globalThis.trace = []; radioB.focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({a: radioA.checked, b: radioB.checked, outside: radioOutside.checked, trace: globalThis.trace})")
            .await
            .unwrap(),
        serde_json::json!({
            "a": false,
            "b": true,
            "outside": true,
            "trace": [
                "keydown:radio-b: ",
                "keyup:radio-b: ",
                "click:radio-b:true:false:true",
                "input:radio-b:true",
                "change:radio-b:true",
            ],
        })
    );
    assert!(
        engine
            .evaluate_async(
                "!globalThis.trace.includes('submit') && !globalThis.trace.includes('invalid')"
            )
            .await
            .unwrap()
            .as_bool()
            .unwrap()
    );

    engine
        .evaluate_async("globalThis.trace = []; globalThis.cancelTarget = ''; globalThis.removeRadioNameOnClick = true; radioA.checked = true; radioB.checked = false; radioB.focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({a: radioA.checked, b: radioB.checked, trace: globalThis.trace})")
            .await
            .unwrap(),
        serde_json::json!({
            "a": false,
            "b": false,
            "trace": [
                "keydown:radio-b: ",
                "keyup:radio-b: ",
                "click:radio-b:true:false:true",
            ],
        })
    );

    engine
        .evaluate_async("globalThis.trace = []; globalThis.removeRadioNameOnClick = false; globalThis.joinRadioOnClick = true; radioB.setAttribute('name', 'group'); radioA.checked = true; radioB.checked = false; document.getElementById('radio-c').setAttribute('name', 'other'); document.getElementById('radio-c').checked = false; radioB.focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({a: radioA.checked, b: radioB.checked, c: document.getElementById('radio-c').checked, trace: globalThis.trace})")
            .await
            .unwrap(),
        serde_json::json!({
            "a": true,
            "b": false,
            "c": false,
            "trace": [
                "keydown:radio-b: ",
                "keyup:radio-b: ",
                "click:radio-b:true:false:true",
            ],
        })
    );

    engine
        .evaluate_async("globalThis.trace = []; globalThis.joinRadioOnClick = false; document.getElementById('radio-unnamed').checked = false; true")
        .await
        .unwrap();
    engine
        .action(NativeAction::Click {
            target: "id=radio-unnamed".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({checked: document.getElementById('radio-unnamed').checked, trace: globalThis.trace})")
            .await
            .unwrap(),
        serde_json::json!({
            "checked": true,
            "trace": [
                "click:radio-unnamed:true:false:true",
                "input:radio-unnamed:true",
                "change:radio-unnamed:true",
            ],
        })
    );

    engine
        .evaluate_async(
            "globalThis.trace = []; checkbox.checked = false; checkbox.indeterminate = true; true",
        )
        .await
        .unwrap();
    engine
        .action(NativeAction::Click {
            target: "id=checkbox".into(),
        })
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("({checked: checkbox.checked, indeterminate: checkbox.indeterminate, trace: globalThis.trace})")
            .await
            .unwrap(),
        serde_json::json!({
            "checked": true,
            "indeterminate": false,
            "trace": [
                "click:checkbox:true:false:true",
                "input:checkbox:true",
                "change:checkbox:true",
            ],
        })
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_raw_space_checkable_activation_is_cancel_and_focus_bound() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-raw-space-checkable",
            "<input id='first' type='checkbox'><input id='second' type='checkbox'>",
        )
        .unwrap()
        .with_initial_url("fixture://local-raw-space-checkable");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "globalThis.trace = []; globalThis.cancelKeydown = false; document.addEventListener('keydown', event => { trace.push('down:' + event.target.id); if (cancelKeydown) event.preventDefault(); }); document.addEventListener('keyup', event => trace.push('up:' + event.target.id)); document.addEventListener('click', event => trace.push('click:' + event.target.id)); document.getElementById('first').focus(); true",
        )
        .await
        .unwrap();

    engine
        .evaluate_async("globalThis.cancelKeydown = true; globalThis.trace = []; true")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyDown { key: " ".into() })
        .unwrap();
    engine
        .action(NativeAction::KeyUp { key: " ".into() })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.trace").await.unwrap(),
        serde_json::json!(["down:first", "up:first"])
    );

    engine
        .evaluate_async("globalThis.cancelKeydown = false; globalThis.trace = []; document.getElementById('first').focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyDown { key: " ".into() })
        .unwrap();
    engine
        .evaluate_async("document.getElementById('second').focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyUp { key: " ".into() })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.trace").await.unwrap(),
        serde_json::json!(["down:first", "up:second"])
    );

    engine
        .evaluate_async("globalThis.trace = []; document.getElementById('first').focus(); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyDown { key: " ".into() })
        .unwrap();
    engine
        .evaluate_async("document.getElementById('first').setAttribute('type', 'text'); true")
        .await
        .unwrap();
    engine
        .action(NativeAction::KeyUp { key: " ".into() })
        .unwrap();
    assert_eq!(
        engine.evaluate_async("globalThis.trace").await.unwrap(),
        serde_json::json!(["down:first", "up:first"])
    );
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('first').checked")
            .await
            .unwrap(),
        serde_json::json!(false)
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_button_activation_runs_reset_validation_and_submit_defaults() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-keyboard-form",
            "<form id='form' action='fixture://local-keyboard-result'><input id='query' name='query' required value='initial'><button id='reset' type='reset'>Reset</button><button id='submit' name='action' value='go' type='submit'>Go</button></form><script>globalThis.events = []; globalThis.cancelReset = false; globalThis.cancelClick = false; globalThis.cancelSubmit = false; globalThis.changeResetType = false; const form = document.getElementById('form'); const reset = document.getElementById('reset'); reset.addEventListener('click', event => { events.push('click:reset'); if (cancelClick) event.preventDefault(); if (changeResetType) reset.type = 'button'; }); form.addEventListener('reset', event => { events.push('reset'); if (cancelReset) event.preventDefault(); }); form.addEventListener('submit', event => { events.push('submit:' + event.submitter.id); if (cancelSubmit) event.preventDefault(); }); document.getElementById('query').addEventListener('invalid', () => events.push('invalid:query'));</script>",
        )
        .unwrap()
        .with_fixture(
            "fixture://local-keyboard-result?query=ready&action=go",
            "<title>Keyboard submitted</title><p>Done</p>",
        )
        .unwrap()
        .with_initial_url("fixture://local-keyboard-form");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    engine
        .evaluate_async(
            "document.getElementById('query').value = 'changed'; document.getElementById('reset').focus(); globalThis.events = []; globalThis.cancelClick = true",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('query').value")
            .await
            .unwrap(),
        serde_json::json!("changed")
    );
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["click:reset"])
    );

    engine
        .evaluate_async(
            "globalThis.events = []; globalThis.cancelClick = false; globalThis.cancelReset = true",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('query').value")
            .await
            .unwrap(),
        serde_json::json!("changed")
    );
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["click:reset", "reset"])
    );

    engine
        .evaluate_async(
            "globalThis.events = []; globalThis.cancelReset = false; globalThis.changeResetType = true",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('query').value")
            .await
            .unwrap(),
        serde_json::json!("changed")
    );
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["click:reset"])
    );

    engine
        .evaluate_async(
            "globalThis.events = []; globalThis.changeResetType = false; document.getElementById('reset').type = 'reset'",
        )
        .await
        .unwrap();
    let before_reset = engine.revision();
    let reset = engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(reset.revision, before_reset + 1);
    assert_eq!(engine.revision(), before_reset + 1);
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('query').value")
            .await
            .unwrap(),
        serde_json::json!("initial")
    );

    engine
        .evaluate_async(
            "document.getElementById('query').value = ''; globalThis.events = []; document.getElementById('submit').focus()",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://local-keyboard-form"
    );
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["invalid:query"])
    );

    engine
        .evaluate_async(
            "document.getElementById('query').value = 'ready'; globalThis.events = []; globalThis.cancelSubmit = true",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Space".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://local-keyboard-form"
    );
    assert_eq!(
        engine.evaluate_async("globalThis.events").await.unwrap(),
        serde_json::json!(["submit:submit"])
    );

    engine
        .evaluate_async("globalThis.events = []; globalThis.cancelSubmit = false")
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://local-keyboard-result?query=ready&action=go"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Keyboard submitted");
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_button_activation_uses_live_form_owner() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://local-dynamic-form-owner",
            "<form id='original' action='fixture://wrong-form-owner'><button id='submit' form='original' name='source' value='button' type='submit'>Go</button></form><form id='destination' action='fixture://dynamic-form-owner-result'></form><script>globalThis.events = []; const submitter = document.getElementById('submit'); submitter.addEventListener('click', () => { events.push('click'); submitter.setAttribute('form', 'destination'); }); document.getElementById('original').addEventListener('submit', () => events.push('original-submit')); document.getElementById('destination').addEventListener('submit', event => events.push('destination-submit:' + event.submitter.id));</script>",
        )
        .unwrap()
        .with_fixture(
            "fixture://dynamic-form-owner-result?source=button",
            "<title>Dynamic owner</title><p>Submitted</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://wrong-form-owner?source=button",
            "<title>Wrong owner</title>",
        )
        .unwrap()
        .with_initial_url("fixture://local-dynamic-form-owner");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('submit').focus()")
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();

    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://dynamic-form-owner-result?source=button"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Dynamic owner");
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_script_click_owns_fixture_navigation() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let config = NativeEngineConfig::default()
                .with_fixture(
                    "fixture://script-nav",
                    "<title>Start</title><a id='next' href='fixture://script-destination'>Next</a>",
                )
                .unwrap()
                .with_fixture(
                    "fixture://script-destination",
                    "<title>Destination</title><p>Arrived</p>",
                )
                .unwrap()
                .with_initial_url("fixture://script-nav");
            let mut engine = NativeEngine::new(config).unwrap();
            engine.initialize_async().await.unwrap();
            let initial_revision = engine.revision();

            engine
                .evaluate_async("document.getElementById('next').click()")
                .await
                .unwrap();
            assert_eq!(engine.revision(), initial_revision + 2);
            assert_eq!(
                engine.snapshot().unwrap().url,
                "fixture://script-destination"
            );
            assert_eq!(engine.snapshot().unwrap().title, "Destination");
            assert_eq!(engine.snapshot().unwrap().visible_text, "Arrived");
            engine.close_async().await.unwrap();
        });
    });
}
#[test]
fn native_local_script_click_owns_same_document_fragment() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://script-fragment",
            "<title>Fragment</title><a id='jump' href='#part'>Jump</a><p id='part'>Part</p>",
        )
        .unwrap()
        .with_initial_url("fixture://script-fragment");
            let mut engine = NativeEngine::new(config).unwrap();
            engine.initialize_async().await.unwrap();
            let initial_revision = engine.revision();
            engine
                .evaluate_async("document.getElementById('jump').click()")
                .await
                .unwrap();
            assert_eq!(engine.revision(), initial_revision + 2);
            assert_eq!(
                engine.snapshot().unwrap().url,
                "fixture://script-fragment#part"
            );
            assert_eq!(engine.snapshot().unwrap().title, "Fragment");
            engine.close_async().await.unwrap();
        });
    });
}
#[test]
fn native_local_script_form_submit_navigates_with_get_controls() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://form-start",
            "<form id='search' action='fixture://form-result'><input name='query' value='hello'><input name='checked' type='checkbox' checked value='yes'></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://form-result?query=hello&checked=yes",
            "<title>Result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://form-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('search').submit()")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://form-result?query=hello&checked=yes"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Result");
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_script_form_submit_navigates_with_post_controls() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://post-form-start",
            "<form id='search' method='post' action='fixture://post-form-result'><input name='query' value='hello'></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://post-form-result",
            "<title>Post result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://post-form-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async("document.getElementById('search').submit()")
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, "fixture://post-form-result");
    assert_eq!(engine.snapshot().unwrap().title, "Post result");
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_rejects_unsupported_post_encoding() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://unsupported-post-form",
            "<form id='search' method='post' enctype='application/x-glass-unsupported' action='fixture://post-result'><input name='query' value='hello'></form>",
        )
        .unwrap()
        .with_fixture("fixture://post-result", "<title>Result</title>")
        .unwrap()
        .with_initial_url("fixture://unsupported-post-form");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    let error = engine
        .evaluate_async("document.getElementById('search').submit()")
        .await
        .unwrap_err();
    assert!(matches!(error, NativeEngineError::UnsupportedUrl { .. }));
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_submit_event_can_cancel_request_submit() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://form-events",
            "<form id='search' action='fixture://form-result'><input name='query' value='hello'></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://form-result?query=hello",
            "<title>Result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://form-events");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const form = document.getElementById('search'); globalThis.submitEvents = []; globalThis.onSubmit = event => { submitEvents.push(event.type); event.preventDefault(); }; form.addEventListener('submit', globalThis.onSubmit); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('search').requestSubmit()")
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, "fixture://form-events");
    assert_eq!(
        engine
            .evaluate_async("globalThis.submitEvents")
            .await
            .unwrap(),
        serde_json::json!(["submit"])
    );

    engine
        .evaluate_async(
            "(() => { const form = document.getElementById('search'); form.removeEventListener('submit', globalThis.onSubmit); form.submit(); })()",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://form-result?query=hello"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Result");
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_validation_and_submitter_metadata_are_bounded() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://validated-form",
            "<form id='search' action='fixture://validated-result'><input id='query' name='query' required><button id='go' name='action' value='search' type='submit'>Go</button></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://validated-result",
            "<title>Validated result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://validated-result?query=ready",
            "<title>Validated result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://validated-form");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { const form = document.getElementById('search'); const input = document.getElementById('query'); globalThis.invalidEvents = []; globalThis.submitEvents = []; input.addEventListener('invalid', event => invalidEvents.push(event.target.id)); form.addEventListener('submit', event => { submitEvents.push(event.submitter && event.submitter.id); event.preventDefault(); }); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, "fixture://validated-form");
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["query"])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.submitEvents")
            .await
            .unwrap(),
        serde_json::json!([])
    );

    engine
        .evaluate_async(
            "(() => { const input = document.getElementById('query'); input.value = 'ready'; })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().url, "fixture://validated-form");
    assert_eq!(
        engine
            .evaluate_async("globalThis.submitEvents")
            .await
            .unwrap(),
        serde_json::json!(["go"])
    );

    engine
        .evaluate_async("document.getElementById('search').submit()")
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://validated-result?query=ready"
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_validation_covers_common_constraints() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://common-validation",
            "<form id='search' action='fixture://common-validation-result'><input id='email' name='email' type='email' value='invalid'><input id='age' name='age' type='number' min='18' max='65' step='2' value='19'><textarea id='code' name='code' minlength='3' maxlength='5'>ab</textarea><button id='go' type='submit'>Go</button></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://common-validation-result?email=user%40example.com&age=20&code=abc",
            "<title>Validated result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://common-validation");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { globalThis.invalidEvents = []; ['email', 'age', 'code'].forEach(id => document.getElementById(id).addEventListener('invalid', event => invalidEvents.push(event.target.id))); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://common-validation"
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["email", "age", "code"])
    );
    engine
        .evaluate_async(
            "(() => { document.getElementById('email').value = 'user@example.com'; document.getElementById('age').value = '20'; document.getElementById('code').value = 'abc'; })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://common-validation-result?email=user%40example.com&age=20&code=abc"
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_validation_covers_pattern_constraints() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://pattern-validation",
            "<form id='search'><input id='code' name='code' pattern='[A-Z]{3}' value='AB1'><input id='ignored' name='ignored' pattern='[' value='accepted'><button id='go' type='submit'>Go</button></form>",
        )
        .unwrap()
        .with_initial_url("fixture://pattern-validation");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const code = document.getElementById('code'); const ignored = document.getElementById('ignored'); const form = document.getElementById('search'); return { mismatch: code.validity.patternMismatch, message: code.validationMessage, ignored: ignored.validity.valid, form: form.checkValidity() }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "mismatch": true,
            "message": "Please match the requested format.",
            "ignored": true,
            "form": false,
        })
    );
    engine
        .evaluate_async("document.getElementById('code').value = 'ABC'")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.getElementById('code').checkValidity()")
            .await
            .unwrap(),
        serde_json::json!(true)
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_validation_covers_temporal_constraints() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://temporal-validation",
            "<form id='search'><input id='date' name='date' type='date' value='2024-02-30'><input id='month' name='month' type='month' value='2024-13'><input id='time' name='time' type='time' value='24:00'><input id='datetime' name='datetime' type='datetime-local' value='2024-02-29T25:00'><input id='step-date' name='step-date' type='date' min='2024-01-01' step='2' value='2024-01-02'><button id='go' type='submit'>Go</button></form>",
        )
        .unwrap()
        .with_initial_url("fixture://temporal-validation");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { globalThis.invalidEvents = []; globalThis.submitCount = 0; ['date', 'month', 'time', 'datetime', 'step-date'].forEach(id => document.getElementById(id).addEventListener('invalid', event => invalidEvents.push(event.target.id))); document.getElementById('search').addEventListener('submit', event => { submitCount += 1; event.preventDefault(); }); })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["date", "month", "time", "datetime", "step-date"])
    );
    engine
        .evaluate_async(
            "(() => { document.getElementById('date').value = '2024-02-29'; document.getElementById('month').value = '2024-12'; document.getElementById('time').value = '23:59'; document.getElementById('datetime').value = '2024-02-29T23:59'; document.getElementById('step-date').value = '2024-01-03'; globalThis.invalidEvents = []; })()",
        )
        .await
        .unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!([])
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.submitCount")
            .await
            .unwrap(),
        serde_json::json!(1)
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_validation_api_exposes_validity_and_custom_errors() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://validation-api",
            "<form id='search' novalidate><input id='email' name='email' type='email' value='invalid'><input id='custom' name='custom' value='ready'><button id='go' type='submit'>Go</button></form>",
        )
        .unwrap()
        .with_initial_url("fixture://validation-api");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "(() => { globalThis.invalidEvents = []; ['email', 'custom'].forEach(id => document.getElementById(id).addEventListener('invalid', event => invalidEvents.push(event.target.id))); })()",
        )
        .await
        .unwrap();

    let validity_observed = engine
        .evaluate_async(
            "(() => { const email = document.getElementById('email'); const form = document.getElementById('search'); return { tag: form.tagName, snapshot: form.validity.valid, form: form.checkValidity(), valid: email.validity.valid, mismatch: email.validity.typeMismatch, message: email.validationMessage, willValidate: email.willValidate }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        validity_observed,
        serde_json::json!({
            "tag": "FORM",
            "snapshot": false,
            "form": false,
            "valid": false,
            "mismatch": true,
            "message": "Please enter a valid value.",
            "willValidate": true,
        })
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["email"])
    );

    engine
        .evaluate_async("document.getElementById('email').value = 'user@example.com'")
        .await
        .unwrap();
    engine
        .evaluate_async("document.getElementById('custom').setCustomValidity('blocked by policy')")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.invalidEvents = []; const custom = document.getElementById('custom'); return { result: custom.checkValidity(), custom: custom.validity.customError, message: custom.validationMessage }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "result": false,
            "custom": true,
            "message": "blocked by policy",
        })
    );
    assert_eq!(
        engine
            .evaluate_async("globalThis.invalidEvents")
            .await
            .unwrap(),
        serde_json::json!(["custom"])
    );

    engine
        .evaluate_async("document.getElementById('custom').setCustomValidity('')")
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.invalidEvents = []; const form = document.getElementById('search'); return { check: form.checkValidity(), report: form.reportValidity(), invalid: invalidEvents }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({ "check": true, "report": true, "invalid": [] })
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_data_constructor_collects_form_controls() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://form-data-constructor",
            "<form id='search'><input name='first' value='one'><textarea name='notes'>hello</textarea><select name='single'><option value='a'>A</option><option value='b' selected>B</option></select><select name='many' multiple><option value='x' selected>X</option><option selected>Y</option><option value='z'>Z</option></select><input name='unchecked' type='checkbox' value='no'><input name='checked' type='checkbox' value='yes' checked><button type='submit'>Go</button></form><input form='search' name='outside' value='two'><form id='files'><input name='upload' type='file'></form>",
        )
        .unwrap()
        .with_initial_url("fixture://form-data-constructor");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const data = new FormData(document.getElementById('search')); data.append('extra', 'three'); const file = new FormData(document.getElementById('files')).get('upload'); return { entries: Array.from(data.entries()), first: data.get('first'), checked: data.get('checked'), unchecked: data.has('unchecked'), outside: data.get('outside'), extra: data.get('extra'), emptyFile: [file instanceof File, file.name, file.size, file.type] }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "entries": [["first", "one"], ["notes", "hello"], ["single", "b"], ["many", "x"], ["many", "Y"], ["checked", "yes"], ["outside", "two"], ["extra", "three"]],
            "first": "one",
            "checked": "yes",
            "unchecked": false,
            "outside": "two",
            "extra": "three",
            "emptyFile": [true, "", 0, ""],
        })
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_file_and_blob_form_data_are_bounded_and_text_backed() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture("fixture://file-blob-form-data", "<p>File blob owner</p>")
        .unwrap()
        .with_initial_url("fixture://file-blob-form-data");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "const blob = new Blob(['hello', new Blob(['!'])], { type: 'TEXT/PLAIN' }); const file = new File(['world'], 'note.txt', { type: 'text/plain', lastModified: 7 }); const data = new FormData(); data.append('blob', blob); data.append('file', file); data.append('text', 'value'); const sliced = blob.slice(1, 4, 'text/custom'); const blobText = await blob.text(); const blobArrayBuffer = Array.from(new Uint8Array(await blob.arrayBuffer())); const blobBytes = typeof blob.bytes === 'function' ? Array.from(await blob.bytes()) : null; const unicodeBytes = Array.from(new Uint8Array(await new Blob(['hé😀']).arrayBuffer())); const sliceText = await sliced.text(); const fileText = await file.text(); const fileBytes = Array.from(new Uint8Array(await file.arrayBuffer())); const limitRejected = (() => { try { new Blob(['x'.repeat(20000)]); return false; } catch (error) { return error instanceof RangeError; } })(); ({ blob: [blob.size, blob.type, blobText], blobArrayBuffer, blobBytes, unicodeBytes, slice: [sliced.size, sliced.type, sliceText], file: [file.name, file.size, file.type, file.lastModified, fileText, fileBytes], limitRejected, entries: Array.from(data.entries()).map(([name, value]) => [name, value.__glassNativeBlob ? { name: value.name || null, type: value.type, text: value._text } : value]) })",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "blob": [6, "text/plain", "hello!"],
            "blobArrayBuffer": [104, 101, 108, 108, 111, 33],
            "blobBytes": [104, 101, 108, 108, 111, 33],
            "unicodeBytes": [104, 195, 169, 240, 159, 152, 128],
            "slice": [3, "text/custom", "ell"],
            "file": ["note.txt", 5, "text/plain", 7, "world", [119, 111, 114, 108, 100]],
            "limitRejected": true,
            "entries": [
                ["blob", {"name": null, "type": "text/plain", "text": "hello!"}],
                ["file", {"name": "note.txt", "type": "text/plain", "text": "world"}],
                ["text", "value"],
            ],
        })
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_keyboard_button_activation_submits_without_script_realm() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://keyboard-no-script-form",
            "<form action='fixture://keyboard-no-script-result'><input name='query' value='hello'><button id='go' name='action' value='search' type='submit'>Go</button></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://keyboard-no-script-result?query=hello&action=search",
            "<title>Keyboard result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://keyboard-no-script-form");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize().unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .unwrap();
    engine
        .action(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .unwrap();
    let result = engine
        .action(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .unwrap();

    assert!(result.accepted);
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://keyboard-no-script-result?query=hello&action=search"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Keyboard result");

        });
    });
}
#[test]
fn native_local_formnovalidate_serializes_submitter_value() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://formnovalidate-start",
            "<form action='fixture://formnovalidate-result'><input name='query' required><button id='skip' name='action' value='skip' type='submit' formnovalidate>Skip validation</button></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://formnovalidate-result?query=&action=skip",
            "<title>Submitter result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://formnovalidate-start");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "document.querySelector('form').requestSubmit(document.getElementById('skip'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://formnovalidate-result?query=&action=skip"
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_submitter_overrides_form_action_and_method() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://submitter-overrides",
            "<form id='search' method='post' action='fixture://wrong-result'><input name='query' value='hello'><button id='go' type='submit' formmethod='get' formaction='fixture://override-result' name='action' value='search'>Go</button></form>",
        )
        .unwrap()
        .with_fixture(
            "fixture://override-result?query=hello&action=search",
            "<title>Override result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://submitter-overrides");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://override-result?query=hello&action=search"
    );
    assert_eq!(engine.snapshot().unwrap().title, "Override result");
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_attribute_associates_external_controls_in_document_order() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://external-form-owner",
            "<input form='search' name='before' value='one'><form id='search' action='fixture://external-form-result'><input name='inside' value='two'></form><input form='search' name='after' value='three'><button id='go' form='search' name='action' value='submit' type='submit'>Submit</button>",
        )
        .unwrap()
        .with_fixture(
            "fixture://external-form-result?before=one&inside=two&after=three&action=submit",
            "<title>External form result</title><p>Submitted</p>",
        )
        .unwrap()
        .with_initial_url("fixture://external-form-owner");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(
            "document.getElementById('search').requestSubmit(document.getElementById('go'))",
        )
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://external-form-result?before=one&inside=two&after=three&action=submit"
    );
    assert_eq!(engine.snapshot().unwrap().title, "External form result");
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_submission_supports_multipart_and_text_plain() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let multipart_config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://multipart-form",
            "<form id='form' method='post' enctype='multipart/form-data' action='fixture://multipart-result'><input name='alpha' value='one'></form>",
        )
        .unwrap()
        .with_fixture("fixture://multipart-result", "<title>Multipart</title>")
        .unwrap()
        .with_initial_url("fixture://multipart-form");
    let mut multipart = NativeEngine::new(multipart_config).unwrap();
    multipart.initialize_async().await.unwrap();
    multipart
        .evaluate_async("document.getElementById('form').submit()")
        .await
        .unwrap();
    assert_eq!(
        multipart.snapshot().unwrap().url,
        "fixture://multipart-result"
    );
    multipart.close_async().await.unwrap();

    let text_plain_config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://text-plain-form",
            "<form id='form' method='post' enctype='text/plain' action='fixture://text-plain-result'><input name='alpha' value='one'></form>",
        )
        .unwrap()
        .with_fixture("fixture://text-plain-result", "<title>Text plain</title>")
        .unwrap()
        .with_initial_url("fixture://text-plain-form");
    let mut text_plain = NativeEngine::new(text_plain_config).unwrap();
    text_plain.initialize_async().await.unwrap();
    text_plain
        .evaluate_async("document.getElementById('form').submit()")
        .await
        .unwrap();
    assert_eq!(
        text_plain.snapshot().unwrap().url,
        "fixture://text-plain-result"
    );
    text_plain.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_runtime_collects_form_associated_custom_element_values() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://inline-form-associated-elements",
                "<!doctype html><html><body><form id='main-form'><x-face-lifecycle id='face-owner'></x-face-lifecycle><x-scalar-field id='scalar' name='scalar'></x-scalar-field><input name='builtin' value='native'><x-pair-field name='ignored'></x-pair-field><x-empty-field name='empty'></x-empty-field><x-disabled-field name='disabled' disabled></x-disabled-field><x-unnamed-field></x-unnamed-field><fieldset id='face-fieldset' disabled><legend><x-legend-field name='legend'></x-legend-field><x-face-lifecycle id='face-legend'></x-face-lifecycle></legend><x-face-lifecycle id='face-disabled'></x-face-lifecycle><x-fieldset-disabled name='fieldset-disabled'></x-fieldset-disabled></fieldset></form><form id='external-form'></form><x-external-field id='external-field' form='external-form' name='outside'></x-external-field><x-face-lifecycle id='face-explicit' form='external-form'></x-face-lifecycle></body></html>",
            )
            .unwrap()
            .with_initial_url("fixture://inline-form-associated-elements"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();

    let result = engine
        .evaluate_async(native_form_associated_elements_script())
        .await
        .unwrap();
    assert_native_form_associated_elements_result(&result);

    let result = engine
        .evaluate_async(native_form_associated_entries_after_script_refresh())
        .await
        .unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "entries": [
                ["scalar", "one"],
                ["builtin", "native"],
                ["first", "alpha"],
                ["second", "beta"],
                ["proof", ["file", "proof.txt", "text/plain"]],
                ["legend", "legend"],
            ],
            "internalsBrand": true,
            "formOwner": true,
            "lifecycleEventCount": 19,
            "explicitLifecycleOwner": "main-form",
        })
    );

    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_runtime_resets_forms_and_runs_face_reset_reactions() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let mut engine = NativeEngine::new(
                NativeEngineConfig::default()
                    .with_fixture("fixture://native-form-reset", native_form_reset_fixture())
                    .unwrap()
                    .with_initial_url("fixture://native-form-reset"),
            )
            .unwrap();
            engine.initialize_async().await.unwrap();
            engine
                .action(NativeAction::Upload {
                    target: "id=reset-file".into(),
                    files: vec![NativeFile {
                        name: "reset.bin".into(),
                        media_type: "application/octet-stream".into(),
                        last_modified: 17,
                        bytes: vec![0, 127, 255],
                    }],
                })
                .unwrap();

            let result = engine
                .evaluate_async(native_form_reset_script())
                .await
                .unwrap();
            assert_native_form_reset_result(&result);
            assert_eq!(
                engine
                    .evaluate_async(native_form_reset_after_script_refresh())
                    .await
                    .unwrap(),
                serde_json::json!({
                    "text": "cancelled-value",
                    "checked": true,
                    "textarea": "default-notes",
                    "select": "default-choice",
                    "options": [false, true],
                    "fallbackSelect": "fallback-enabled",
                    "fallbackOptions": [false, true],
                    "fileCount": 0,
                    "fileValue": "",
                    "customError": true,
                    "unnamedRadios": [true, true],
                    "namedRadios": [false, true],
                    "external": "external-default",
                    "faceValue": "after",
                    "callbackCount": 6,
                })
            );

            engine.close_async().await.unwrap();
        });
    });
}
#[test]
fn native_local_generic_explicit_tabindex_focus_order_and_enter_target() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://generic-tabindex-focus",
            "<div id='positive-first' tabindex='1'></div><div id='positive-first-tie' tabindex='1'></div><div id='positive-second' tabindex='+2'></div><div id='zero' tabindex='0'></div><h2 id='heading' tabindex='0'>Heading</h2><div id='generic-disabled-attribute' disabled tabindex='0'></div><div id='negative' tabindex='-1'></div><div id='invalid' tabindex='not-an-integer'></div><input id='hidden-input' type='hidden' tabindex='0'><button id='disabled-button' type='button' disabled tabindex='0'>Disabled</button><div id='hidden' hidden tabindex='0'></div>",
        )
        .unwrap()
        .with_initial_url("fixture://generic-tabindex-focus");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let mut active_ids = Vec::new();
    for _ in 0..6 {
        engine
            .action_async(NativeAction::Shortcut {
                shortcut: "Tab".into(),
            })
            .await
            .unwrap();
        active_ids.push(
            engine
                .evaluate_async("document.activeElement.id")
                .await
                .unwrap(),
        );
    }
    assert_eq!(
        serde_json::Value::Array(active_ids),
        serde_json::json!([
            "positive-first",
            "positive-first-tie",
            "positive-second",
            "zero",
            "heading",
            "generic-disabled-attribute",
        ])
    );
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Shift+Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("heading")
    );
    engine
        .evaluate_async(
            "document.getElementById('generic-disabled-attribute').setAttribute('tabindex', '3'); document.getElementById('positive-second').focus(); true",
        )
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("generic-disabled-attribute")
    );

    assert_eq!(
        engine
            .evaluate_async(
                "(() => { globalThis.__genericFocusTrace = []; const target = document.getElementById('negative'); for (const type of ['focus', 'keydown', 'keyup', 'click']) target.addEventListener(type, event => __genericFocusTrace.push(type + ':' + event.target.id + ':' + (event.key || ''))); target.focus(); return document.activeElement.id; })()",
            )
            .await
            .unwrap(),
        serde_json::json!("negative")
    );
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Enter".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("globalThis.__genericFocusTrace")
            .await
            .unwrap(),
        serde_json::json!([
            "focus:negative:",
            "keydown:negative:Enter",
            "keyup:negative:Enter",
        ])
    );
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("negative")
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_form_associated_custom_element_disabled_focus_state() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://form-associated-focus",
                native_form_associated_focus_markup(),
            )
            .unwrap()
            .with_initial_url("fixture://form-associated-focus"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    engine
        .evaluate_async(native_form_associated_focus_definition())
        .await
        .unwrap();

    assert_eq!(
        engine
            .evaluate_async(
                "document.getElementById('zero-first').focus(); document.getElementById('own-disabled').focus(); document.getElementById('fieldset-disabled').focus(); document.activeElement.id",
            )
            .await
            .unwrap(),
        serde_json::json!("zero-first")
    );

    for expected in ["enabled"] {
        engine
            .action_async(NativeAction::Shortcut {
                shortcut: "Tab".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            engine
                .evaluate_async("document.activeElement.id")
                .await
                .unwrap(),
            serde_json::json!(expected)
        );
    }
    engine
        .evaluate_async("document.getElementById('enabled').setAttribute('disabled', ''); true")
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("legend-enabled")
    );
    engine
        .evaluate_async("document.getElementById('enabled').removeAttribute('disabled'); true")
        .await
        .unwrap();
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Shift+Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("enabled")
    );
    for expected in ["legend-enabled", "ordinary-disabled"] {
        engine
            .action_async(NativeAction::Shortcut {
                shortcut: "Tab".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            engine
                .evaluate_async("document.activeElement.id")
                .await
                .unwrap(),
            serde_json::json!(expected)
        );
    }
    engine.close_async().await.unwrap();
        });
    });
}
#[test]
fn native_local_inert_focus_state_blocks_programmatic_and_sequential_focus() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
    let mut engine = NativeEngine::new(
        NativeEngineConfig::default()
            .with_fixture("fixture://inert-focus", native_inert_focus_markup())
            .unwrap()
            .with_initial_url("fixture://inert-focus"),
    )
    .unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const panel = document.getElementById('panel'); const explicit = document.getElementById('explicit-blocked'); const implicit = document.getElementById('implicit-blocked'); document.getElementById('first').focus(); explicit.focus(); implicit.focus(); const initial = [panel.inert, panel.hasAttribute('inert'), explicit.inert, document.activeElement.id]; panel.inert = false; const removed = [panel.inert, panel.hasAttribute('inert')]; document.getElementById('first').focus(); return { initial, removed }; })()",
            )
            .await
            .unwrap(),
        serde_json::json!({
            "initial": [true, true, false, "first"],
            "removed": [false, false],
        })
    );
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("explicit-blocked")
    );
    assert_eq!(
        engine
            .evaluate_async(
                "(() => { const panel = document.getElementById('panel'); panel.inert = true; document.getElementById('explicit-blocked').focus(); document.getElementById('implicit-blocked').focus(); return [panel.inert, panel.hasAttribute('inert'), document.activeElement.id]; })()",
            )
            .await
            .unwrap(),
        serde_json::json!([true, true, "explicit-blocked"])
    );
    engine
        .action_async(NativeAction::Shortcut {
            shortcut: "Tab".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.activeElement.id")
            .await
            .unwrap(),
        serde_json::json!("first")
    );
    engine.close_async().await.unwrap();

        });
    });
}
#[test]
fn native_local_focusin_focusout_bubble_with_related_targets() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(async {
            let mut engine = NativeEngine::new(
                NativeEngineConfig::default()
                    .with_fixture("fixture://focus-events", native_focus_event_markup())
                    .unwrap()
                    .with_initial_url("fixture://focus-events"),
            )
            .unwrap();
            engine.initialize_async().await.unwrap();
            assert_eq!(
                engine
                    .evaluate_async(&native_window_document_event_handler_surface_script())
                    .await
                    .unwrap(),
                serde_json::json!([[], [], [], [], [], [], [], [], [], [true, true]])
            );
            assert_eq!(
                engine
                    .evaluate_async(native_window_document_event_handler_dispatch_script())
                    .await
                    .unwrap(),
                serde_json::json!([
                    [
                        ["window-message", true, true, true],
                        ["document-ready", true, true, true],
                        ["document-visibility", true, true, true],
                        ["body-load", true, true, true],
                        ["frameset-afterprint", true, true, true],
                        ["body-click", true, true, true],
                    ],
                    true,
                    true,
                    true,
                    true,
                ])
            );
            assert_eq!(
                engine
                    .evaluate_async(native_window_routed_content_attribute_script())
                    .await
                    .unwrap(),
                serde_json::json!([
                    true,
                    true,
                    ["idl", "tail"],
                    true,
                    ["idl", "tail", "body-replacement", "tail"],
                    true,
                    ["idl", "tail", "body-replacement", "tail", "tail"],
                    true,
                    [
                        "idl",
                        "tail",
                        "body-replacement",
                        "tail",
                        "tail",
                        "tail",
                        "frameset"
                    ],
                    true,
                    [[true, true]],
                ])
            );
            assert_eq!(
                engine
                    .evaluate_async(&native_html_global_event_handler_surface_script())
                    .await
                    .unwrap(),
                serde_json::json!([[], [], []])
            );
            assert_eq!(
                engine
                    .evaluate_async(native_global_event_handler_dispatch_script())
                    .await
                    .unwrap(),
                serde_json::json!([[["click", true, true], ["input", true, true]], null, null,])
            );
            assert_eq!(
                engine
                    .evaluate_async(native_focus_event_listener_script())
                    .await
                    .unwrap(),
                native_focus_transition_events()
            );
            assert_eq!(
                engine.evaluate_async("idlFocusEvents").await.unwrap(),
                serde_json::json!([
                    ["focus-first", true, true],
                    ["blur-first", true, true],
                    ["focus-second", true, true],
                ])
            );
            assert_eq!(
                engine.evaluate_async("clearedFocusHandlers").await.unwrap(),
                serde_json::json!([null, null])
            );
            assert_eq!(
                engine.evaluate_async("idlFocusOrder").await.unwrap(),
                serde_json::json!(["before", "handler", "after"])
            );

            engine
                .action_async(NativeAction::Shortcut {
                    shortcut: "Tab".into(),
                })
                .await
                .unwrap();
            assert_eq!(
                engine.evaluate_async("focusEvents").await.unwrap(),
                serde_json::json!([
                    ["focus", "first", null, true],
                    ["focusin", "first", null, true],
                    ["blur", "first", "second", true],
                    ["focusout", "first", "second", true],
                    ["focus", "second", "first", true],
                    ["focusin", "second", "first", true],
                    ["blur", "second", "first", true],
                    ["focusout", "second", "first", true],
                    ["focus", "first", "second", true],
                    ["focusin", "first", "second", true],
                ])
            );
            assert_eq!(
                engine.evaluate_async("idlFocusEvents").await.unwrap(),
                serde_json::json!([
                    ["focus-first", true, true],
                    ["blur-first", true, true],
                    ["focus-second", true, true],
                ])
            );
            assert_eq!(
                engine.evaluate_async("idlFocusOrder").await.unwrap(),
                serde_json::json!(["before", "handler", "after", "before", "after"])
            );
            engine.close_async().await.unwrap();
        });
    });
}

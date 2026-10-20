use super::*;

#[test]
fn standalone_native_dialog_controller_resumes_the_original_page_script() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(tokio::task::LocalSet::new().run_until(
            standalone_native_dialog_controller_resumes_the_original_page_script_inner(),
        ));
    });
}

async fn standalone_native_dialog_controller_resumes_the_original_page_script_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        assert_eq!(request.split_whitespace().nth(1), Some("/modal-controller"));
        let body = "<script>window.dialogTrace=['before'];window.dialogTrace.push(confirm('continue?')?'accepted':'dismissed');window.dialogTrace.push(prompt('name?','seed'));alert('finished');window.dialogTrace.push('after');</script><p>ready</p>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let session = Arc::new(
        BrowserRuntimeSession::connect_native_with_modal_dialogs(NativeEngineConfig::default())
            .await
            .unwrap(),
    );
    let controller = session.native_dialog_controller().unwrap();
    let navigation_session = Arc::clone(&session);
    let url = format!("http://{address}/modal-controller");
    let navigation_url = url.clone();
    let navigation =
        tokio::task::spawn_local(async move { navigation_session.navigate(navigation_url).await });

    let confirm = wait_for_modal_dialog(&controller, "confirm").await;
    assert!(!confirm.context_id.is_empty());
    assert!(!confirm.frame_id.is_empty());
    assert_eq!(confirm.dialog.message, "continue?");
    assert!(!navigation.is_finished());
    assert!(
        controller
            .resolve_dialog(
                "native-dialog-stale",
                NativeDialogResolution {
                    accepted: true,
                    prompt_value: None,
                },
            )
            .is_err()
    );
    assert_eq!(
        controller.pending_dialog().unwrap().as_ref(),
        Some(&confirm)
    );
    controller
        .resolve_dialog(
            &confirm.id,
            NativeDialogResolution {
                accepted: true,
                prompt_value: None,
            },
        )
        .unwrap();

    let prompt = wait_for_modal_dialog(&controller, "prompt").await;
    assert_eq!(prompt.dialog.default_value.as_deref(), Some("seed"));
    assert!(!navigation.is_finished());
    controller
        .resolve_dialog(
            &prompt.id,
            NativeDialogResolution {
                accepted: true,
                prompt_value: Some("Ada".into()),
            },
        )
        .unwrap();

    let alert = wait_for_modal_dialog(&controller, "alert").await;
    assert_eq!(alert.dialog.message, "finished");
    assert!(!navigation.is_finished());
    controller
        .resolve_dialog(
            &alert.id,
            NativeDialogResolution {
                accepted: true,
                prompt_value: None,
            },
        )
        .unwrap();

    let outcome = tokio::time::timeout(Duration::from_secs(15), navigation)
        .await
        .expect("navigation remained suspended after the dialog decisions")
        .expect("native navigation task panicked")
        .unwrap();
    assert_eq!(outcome.url, url);
    assert!(controller.pending_dialog().unwrap().is_none());
    assert_eq!(
        session
            .script("window.dialogTrace.join('|')")
            .await
            .unwrap()
            .value,
        serde_json::json!("before|accepted|Ada|after")
    );

    let session = Arc::try_unwrap(session)
        .unwrap_or_else(|_| panic!("standalone modal session still has unexpected owners"));
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_frame_sandbox_modals_refresh_for_replacement_document() {
    run_native_browser_worker_test(|runtime| {
        let local_set = tokio::task::LocalSet::new();
        let test = local_set.run_until(async {
            let child = native_html_without_body_margin("<title>Child</title><script>globalThis.beforeCount = 0; addEventListener('beforeunload', event => { globalThis.beforeCount++; event.preventDefault(); event.returnValue = 'secret'; });</script><iframe sandbox='allow-scripts allow-modals' src='/grand'></iframe><button id='activate-child'>Activate child</button>");
            let grandchild = native_html_without_body_margin("<title>Grandchild</title><script>addEventListener('beforeunload', event => { event.preventDefault(); event.returnValue = 'secret'; });</script><button id='activate-grandchild'>Activate grandchild</button>");
            let config = NativeEngineConfig::default()
                .with_fixture(
                    "fixture://sandbox-refresh.test/parent",
                    "<title>Parent</title><iframe id='child' sandbox='allow-scripts' src='/child'></iframe>",
                )
                .unwrap()
                .with_fixture("fixture://sandbox-refresh.test/child", child.clone())
                .unwrap()
                .with_fixture("fixture://sandbox-refresh.test/grand", grandchild)
                .unwrap()
                .with_fixture("fixture://sandbox-refresh.test/grand-next", "<title>Grandchild next</title>")
                .unwrap()
                .with_fixture("fixture://sandbox-refresh.test/next", child)
                .unwrap()
                .with_fixture("fixture://sandbox-refresh.test/third", "<title>Third</title>")
                .unwrap()
                .with_initial_url("fixture://sandbox-refresh.test/parent");
            let session = Arc::new(
                BrowserRuntimeSession::connect_native_with_modal_dialogs(config)
                    .await
                    .unwrap(),
            );
            let controller = session.native_dialog_controller().unwrap();
            let frames = session.list_frames().await.unwrap();
            let root_id = frames[0].id.clone();
            let child_id = frames
                .iter()
                .find(|frame| frame.parent_id.as_deref() == Some(root_id.as_str()))
                .unwrap()
                .id
                .clone();
            let grandchild_id = frames
                .iter()
                .find(|frame| frame.parent_id.as_deref() == Some(child_id.as_str()))
                .unwrap()
                .id
                .clone();
            session.select_frame(&child_id).await.unwrap();
            session
                .action(SemanticAction::Click {
                    target: "id=activate-child".into(),
                })
                .await
                .unwrap();
            session.select_frame(&grandchild_id).await.unwrap();
            tokio::time::timeout(
                Duration::from_secs(15),
                session.action(SemanticAction::Click {
                    target: "id=activate-grandchild".into(),
                }),
            )
            .await
            .expect("grandchild activation action timed out")
            .unwrap();

            let inherited_navigation_session = Arc::clone(&session);
            let mut inherited_navigation = tokio::task::spawn_local(async move {
                inherited_navigation_session
                    .navigate("fixture://sandbox-refresh.test/grand-next")
                    .await
            });
            let inherited_result = tokio::select! {
                result = &mut inherited_navigation => result.unwrap().unwrap(),
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => panic!("nested allow-modals token cleared inherited restriction: {prompt:?}"),
            };
            assert_eq!(inherited_result.url, "fixture://sandbox-refresh.test/grand-next");
            assert!(controller.pending_dialog().unwrap().is_none());

            session.select_frame(&root_id).await.unwrap();
            session
                .script("document.getElementById('child').setAttribute('sandbox', 'allow-scripts allow-modals'); true")
                .await
                .unwrap();
            session.select_frame(&child_id).await.unwrap();
            session
                .navigate("fixture://sandbox-refresh.test/next")
                .await
                .unwrap();
            assert!(controller.pending_dialog().unwrap().is_none());

            session
                .action(SemanticAction::Click {
                    target: "id=activate-child".into(),
                })
                .await
                .unwrap();
            let next_session = Arc::clone(&session);
            let mut next_navigation = tokio::task::spawn_local(async move {
                next_session
                    .navigate("fixture://sandbox-refresh.test/third")
                    .await
            });
            let prompt = tokio::select! {
                prompt = wait_for_modal_dialog(&controller, "beforeunload") => prompt,
                result = &mut next_navigation => panic!("replacement document retained stale sandboxed-modals state: {result:?}"),
            };
            assert_eq!(prompt.frame_id, child_id);
            controller
                .resolve_dialog(
                    &prompt.id,
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: None,
                    },
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), next_navigation)
                .await
                .expect("replacement navigation did not complete")
                .unwrap()
                .unwrap();
            assert_eq!(session.evidence(EvidenceLevel::Compact).await.unwrap().url, "fixture://sandbox-refresh.test/third");
            Arc::try_unwrap(session)
                .unwrap_or_else(|_| panic!("native session still has unexpected owners"))
                .close()
                .await
                .unwrap();
        });
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(120), test)
                .await
                .expect("native frame sandbox modal test exceeded 120 seconds");
        });
    });
}
#[test]
fn native_same_origin_frame_script_projection_matches_window_contract() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_same_origin_frame_script_projection_matches_window_contract_inner());
    });
}

async fn native_same_origin_frame_script_projection_matches_window_contract_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/parent" => {
                    "<html><body><iframe id='child' src='/child'></iframe><p>parent</p></body></html>"
                }
                "/child" => {
                    "<!doctype html><!--frame-root--><html><head><script>addEventListener('message', event => { document.getElementById('inside').setAttribute('data-message', event.data.ok ? 'received' : 'bad'); document.getElementById('field').focus(); }); addEventListener('beforeunload', () => document.getElementById('trigger').click());</script></head><body><!--frame-body--><p id='inside' data-message='none'>same-origin <span id='old-child'>child</span></p><input id='field' value='before'><button id='trigger'></button></body></html>"
                }
                "/child-next" => {
                    "<html><body><p id='next'>navigated frame</p><p id='inside'></p></body></html>"
                }
                other => panic!("unexpected frame projection request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 2);
    let identity = session
        .script(
            "(() => { const frame = document.getElementById('child'); const childWindow = frame.contentWindow; const childDocument = frame.contentDocument; const inside = childDocument.querySelector('#inside'); const first = inside.firstChild; const bodyComment = childDocument.body.firstChild; const parsedDoctype = childDocument.doctype; parsedDoctype.remove(); const createdDoctype = childDocument.implementation.createDocumentType('frame-created'); childDocument.appendChild(createdDoctype); const createdComment = childDocument.createComment('frame-created-comment'); childDocument.body.appendChild(createdComment); globalThis.__createdFrameDoctype = createdDoctype; return { window: childWindow instanceof Window, document: childDocument instanceof Document && childWindow.document === childDocument && childDocument.defaultView === childWindow, parent: childWindow.parent === window && childWindow.top === window, frameElement: childWindow.frameElement === frame, content: childDocument.body.textContent, query: inside.textContent, textNodes: [first.nodeType, first.nodeValue, first instanceof Node, first instanceof CharacterData, first instanceof Text, first.data, first.length, first.parentNode === inside, first.nextSibling === childDocument.getElementById('old-child'), inside.childNodes.length, inside.children.length], domNodes: [childDocument.doctype instanceof DocumentType, childDocument.doctype.name, childDocument.doctype === createdDoctype, createdDoctype.parentNode === childDocument, bodyComment instanceof Comment, bodyComment.data, bodyComment.parentNode === childDocument.body, createdComment instanceof Comment, createdComment.data, createdComment.parentNode === childDocument.body, childDocument.body.lastChild === createdComment], collections: [childDocument.querySelectorAll('p') instanceof NodeList, childDocument.getElementsByTagName('p') instanceof HTMLCollection], surfaces: [childDocument.head === childDocument.querySelector('head'), childDocument.scripts instanceof HTMLCollection, childDocument.scripts.length, childDocument.scrollingElement === childDocument.documentElement], frames: window.length === 1 && window.frames[0] === childWindow }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        identity.value,
        serde_json::json!({
            "window": true,
            "document": true,
            "parent": true,
            "frameElement": true,
            "content": "same-origin child",
            "query": "same-origin child",
            "textNodes": [3, "same-origin ", true, true, true, "same-origin ", 12, true, true, 2, 1],
            "domNodes": [true, "frame-created", true, true, true, "frame-body", true, true, "frame-created-comment", true, true],
            "collections": [true, true],
            "surfaces": [true, true, 1, true],
            "frames": true,
        })
    );
    let frame_namespace = session
        .script(
            "(() => { const child = document.getElementById('child').contentDocument; const svg = child.createElementNS('http://www.w3.org/2000/svg', 'svg'); const circle = child.createElementNS('http://www.w3.org/2000/svg', 'circle'); svg.appendChild(circle); child.body.appendChild(svg); const math = child.createElementNS('http://www.w3.org/1998/Math/MathML', 'mi'); child.body.appendChild(math); return [svg.namespaceURI, circle.namespaceURI, math.namespaceURI, child.getElementsByTagName('svg')[0] === svg]; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        frame_namespace.value,
        serde_json::json!([
            "http://www.w3.org/2000/svg",
            "http://www.w3.org/2000/svg",
            "http://www.w3.org/1998/Math/MathML",
            true,
        ])
    );
    let frame_namespaced_attributes = session
        .script(
            "(() => { const xlink = 'http://www.w3.org/1999/xlink'; const child = document.getElementById('child').contentDocument; const svg = child.createElementNS('http://www.w3.org/2000/svg', 'svg'); svg.setAttributeNS(xlink, 'xlink:href', '#frame'); const attr = child.createAttributeNS(xlink, 'xlink:title'); attr.value = 'frame-title'; svg.attributes.setNamedItemNS(xlink, attr); child.body.appendChild(svg); const href = svg.getAttributeNodeNS(xlink, 'href'); const clone = svg.cloneNode(false); const cloneHref = clone.getAttributeNodeNS(xlink, 'href'); const attrOwner = attr.ownerElement === svg; const removed = svg.attributes.removeNamedItemNS(xlink, 'title'); return [svg.getAttributeNS(xlink, 'href'), href && [href.namespaceURI, href.localName, href.value], attrOwner, clone.namespaceURI, cloneHref && cloneHref.value, removed === attr, attr.ownerElement === null]; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        frame_namespaced_attributes.value,
        serde_json::json!([
            "#frame",
            ["http://www.w3.org/1999/xlink", "href", "#frame",],
            true,
            "http://www.w3.org/2000/svg",
            "#frame",
            true,
            true,
        ])
    );
    let frame_attributes = session
        .script(
            "(() => { const child = document.getElementById('child').contentDocument; const host = child.createElement('section'); const attr = child.createAttribute('data-frame'); attr.value = 'one'; const old = host.setAttributeNode(attr); const collection = host.attributes; const before = collection[0] === attr; attr.value = 'two'; const replacement = child.createAttribute('data-frame'); replacement.value = 'three'; const replaced = collection.setNamedItem(replacement); const removed = collection.removeNamedItem('data-frame'); return [old, collection instanceof NamedNodeMap, before, attr.value, replaced === attr, attr.ownerElement === null, removed === replacement, replacement.ownerElement === null, host.getAttribute('data-frame') === null, (() => { try { host.setAttributeNode(child.createAttribute('bad name')); return null; } catch (error) { return error.name; } })()]; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        frame_attributes.value,
        serde_json::json!([
            null,
            true,
            true,
            "two",
            true,
            true,
            true,
            true,
            true,
            "InvalidCharacterError"
        ])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; child.title = 'Frame title'; return [child.title, child.head !== null, child.querySelector('title').textContent]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["Frame title", true, "Frame title"])
    );
    assert_eq!(
        session
            .script("[document.getElementById('child').contentDocument.title, document.getElementById('child').contentDocument.head.tagName]")
            .await
            .unwrap()
            .value,
        serde_json::json!(["Frame title", "HEAD"])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const paragraph = child.getElementById('inside'); const root = paragraph.getRootNode(); paragraph.outerHTML = \"<article id='frame-replacement'>replaced</article>\"; const replacement = child.getElementById('frame-replacement'); const articles = child.getElementsByTagName('article'); const result = [root === child, replacement !== null, articles.length, articles[0] ? articles[0].id : null, articles[0] ? articles[0].__glassAttached : null, articles[0] ? articles[0].parentElement === child.body : null, child.getElementById('inside')]; replacement.outerHTML = \"<p id='inside' data-message='none'>same-origin <span id='old-child'>child</span></p>\"; return result; })()",
            )
            .await
            .unwrap()
            .value,
    serde_json::json!([true, true, 1, "frame-replacement", true, true, null])
);
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const inside = child.getElementById('inside'); const clone = inside.cloneNode(true); return [clone !== inside, clone.isEqualNode(inside), clone.firstElementChild !== inside.firstElementChild, clone.firstElementChild.tagName, clone.parentNode === null, clone.getRootNode() === clone, child.body.compareDocumentPosition(inside), inside.compareDocumentPosition(child.body)]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([true, true, true, "SPAN", true, true, 20, 10])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const inside = child.querySelector('p#inside'); const old = inside.querySelector('span#old-child'); const classes = inside.classList; classes.add('from-parent'); return [inside.matches('p#inside.from-parent'), inside.closest('body') === child.body, old === child.getElementById('old-child'), inside.getElementsByTagName('span').length, classes.length, classes.contains('from-parent')]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([true, true, true, 1, 1, true])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const host = child.createElement('article'); const item = child.createElement('span'); item.id = 'persistent-frame'; const text = child.createTextNode('persistent frame text'); host.append(item, text); child.body.appendChild(host); globalThis.__persistentFrameHost = host; globalThis.__persistentFrameItem = item; globalThis.__persistentFrameText = text; return [item.parentNode === host, text.parentNode === host, item.getRootNode() === child]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([true, true, true])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const host = globalThis.__persistentFrameHost; const item = globalThis.__persistentFrameItem; const text = globalThis.__persistentFrameText; const current = child.getElementById('persistent-frame'); const currentText = host.lastChild; const result = [current === item, host.contains(item), item.parentElement === host, item.getRootNode() === child, currentText === text, host.contains(text), text.parentNode === host, text.getRootNode() === child, text.data]; text.data = 'updated'; host.remove(); return result; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([true, true, true, true, true, true, true, true, "persistent frame text"])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const inside = child.getElementById('inside'); const style = inside.style; style.backgroundColor = 'blue'; style.setProperty('margin-top', '4px', 'important'); const dataset = inside.dataset; dataset.fromParent = 'yes'; const names = Object.keys(dataset).sort(); delete dataset.fromParent; return [style.backgroundColor, style.getPropertyPriority('margin-top'), style.cssText, dataset.message, dataset.fromParent, names, inside.style === style, inside.getAttribute('style')]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([
            "blue",
            "important",
            "background-color: blue; margin-top: 4px !important;",
            "none",
            null,
            ["fromParent", "message"],
            true,
            "background-color: blue; margin-top: 4px !important;",
        ])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const host = child.createElement('section'); const log = []; const observer = new MutationObserver(records => log.push(records.map(record => ({ type: record.type, target: record.target.id || record.target.nodeName, attributeName: record.attributeName, oldValue: record.oldValue, added: record.addedNodes.length, removed: record.removedNodes.length })))); observer.observe(child, { subtree: true, childList: true }); observer.observe(host, { attributes: true, attributeOldValue: true }); host.setAttribute('data-frame', 'yes'); child.body.appendChild(host); host.remove(); globalThis.frameMutationLog = log; return true; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(true)
    );
    assert_eq!(
        session
            .script("globalThis.frameMutationLog")
            .await
            .unwrap()
            .value,
        serde_json::json!([[
            {
                "type": "attributes",
                "target": "SECTION",
                "attributeName": "data-frame",
                "oldValue": null,
                "added": 0,
                "removed": 0,
            },
            {
                "type": "childList",
                "target": "BODY",
                "attributeName": null,
                "oldValue": null,
                "added": 1,
                "removed": 0,
            },
            {
                "type": "childList",
                "target": "BODY",
                "attributeName": null,
                "oldValue": null,
                "added": 0,
                "removed": 1,
            },
        ]])
    );
    assert_eq!(
        session
            .script(
                "(() => { const frame = document.getElementById('child'); const child = frame.contentDocument; const inside = child.getElementById('inside'); const calls = []; const topCalls = []; const onInside = event => calls.push(['inside', event.target === inside, event.currentTarget === inside, event.eventPhase]); const onBodyCapture = event => calls.push(['body-capture', event.currentTarget === child.body, event.eventPhase]); const onBody = event => calls.push(['body', event.currentTarget === child.body, event.eventPhase]); const onDocument = event => calls.push(['document', event.currentTarget === child, event.eventPhase]); const onWindow = event => calls.push(['window', event.currentTarget === child.defaultView, event.eventPhase]); inside.addEventListener('probe', onInside); child.body.addEventListener('probe', onBodyCapture, true); child.body.addEventListener('probe', onBody); child.addEventListener('probe', onDocument); child.defaultView.addEventListener('probe', onWindow); document.addEventListener('probe', () => topCalls.push('document')); window.addEventListener('probe', () => topCalls.push('window')); const accepted = inside.dispatchEvent(new Event('probe', { bubbles: true, cancelable: true })); inside.removeEventListener('probe', onInside); const second = inside.dispatchEvent(new Event('probe', { bubbles: true, cancelable: true })); return { accepted, second, calls, topCalls, owners: [inside.ownerDocument === child, child.defaultView === child.defaultView, typeof child.addEventListener === 'function', typeof child.defaultView.dispatchEvent === 'function'] }; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "accepted": true,
            "second": true,
            "calls": [
                ["body-capture", true, 1],
                ["inside", true, true, 2],
                ["body", true, 3],
                ["document", true, 3],
                ["window", true, 3],
                ["body-capture", true, 1],
                ["body", true, 3],
                ["document", true, 3],
                ["window", true, 3],
            ],
            "topCalls": [],
            "owners": [true, true, true, true],
        })
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const inside = child.getElementById('inside'); inside.setAttribute('data-message', 'parent-write'); inside.innerHTML = '<em id=\"new-child\">updated &amp; literal</em>'; child.getElementById('field').value = 'written-by-parent'; return [inside.getAttribute('data-message'), inside.innerHTML, inside.textContent, inside.innerText, child.getElementById('field').value]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["parent-write", "<em id=\"new-child\">updated &amp; literal</em>", "updated & literal", "updated & literal", "written-by-parent"])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const inside = child.getElementById('inside'); return [inside.getAttribute('data-message'), inside.innerHTML, inside.textContent, inside.innerText, child.getElementById('old-child'), child.getElementById('new-child').textContent, child.getElementById('field').value]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["parent-write", "<em id=\"new-child\">updated &amp; literal</em>", "updated & literal", "updated & literal", null, "updated & literal", "written-by-parent"])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const host = child.createElement('section'); host.setAttribute('id', 'created-in-frame'); const text = child.createTextNode('created'); text.textContent = 'created in frame'; host.appendChild(text); child.body.appendChild(host); const children = host.children; const nodes = host.childNodes; const extra = child.createElement('em'); host.insertBefore(extra, text); const inserted = [children instanceof HTMLCollection, nodes instanceof NodeList, children.length, nodes.length, host.firstChild === extra, host.lastChild === text, extra.nextSibling === text, host.contains(text), child.contains(host)]; host.replaceChild(text, extra); return [host.tagName, host.parentElement === child.body, text.parentNode === host, host.textContent, host.innerHTML, inserted, children.length, nodes.length, host.firstChild === text, host.lastChild === text]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["SECTION", true, true, "created in frame", "created in frame", [true, true, 1, 2, true, true, true, true, true], 0, 1, true, true])
    );
    assert_eq!(
        session
            .script(
                "(() => { const host = document.getElementById('child').contentDocument.getElementById('created-in-frame'); return [host.innerHTML, host.textContent, host.parentElement.tagName]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["created in frame", "created in frame", "BODY"])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const host = child.createElement('section'); host.setAttribute('id', 'fragment-frame'); const fragment = child.createDocumentFragment(); const text = child.createTextNode('native'); const strong = child.createElement('strong'); strong.textContent = 'frame'; fragment.append(text, strong); const scratch = child.createDocumentFragment(); const disposable = child.createElement('aside'); scratch.append(disposable); disposable.remove(); host.appendChild(fragment); child.body.appendChild(host); const after = child.createTextNode('-after'); strong.after(after); return [fragment instanceof DocumentFragment, fragment.ownerDocument === child, fragment.childNodes.length, scratch.childNodes.length, disposable.parentNode === null, host.innerHTML, host.textContent, host.childNodes.length, text.parentNode === host, strong.parentElement === host, after.parentNode === host]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([
            true,
            true,
            0,
            0,
            true,
            "native<strong>frame</strong>-after",
            "nativeframe-after",
            3,
            true,
            true,
            true,
        ])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const fragment = child.createDocumentFragment(); fragment.innerHTML = '<section data-source=\"frame\"><em>Projected &amp; ready</em> tail</section><hr>'; const section = fragment.firstElementChild; const em = fragment.querySelector('em'); const before = [fragment.innerHTML, fragment.childNodes.length, section.getAttribute('data-source'), em.textContent, section.parentNode === fragment]; const mount = child.createElement('main'); mount.appendChild(fragment); child.body.appendChild(mount); return [before, fragment.innerHTML, mount.querySelector('em') === em, mount.innerHTML]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([
            [
                "<section data-source=\"frame\"><em>Projected &amp; ready</em> tail</section><hr>",
                2,
                "frame",
                "Projected & ready",
                true,
            ],
            "",
            true,
            "<section data-source=\"frame\"><em>Projected &amp; ready</em> tail</section><hr>",
        ])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const fragment = child.createDocumentFragment(); fragment.innerHTML = '<!doctype html><!--ignored--><div title=\"a > b\">before<script>if (a < b) x = \"</div>\";</script><textarea>&lt;em&gt;safe&lt;/em&gt;</textarea></div>'; const div = fragment.firstElementChild; const script = div.querySelector('script'); const textarea = div.querySelector('textarea'); return [fragment.childNodes.length, div.getAttribute('title'), script.textContent, textarea.textContent, div.querySelector('em') === null]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([2, "a > b", "if (a < b) x = \"</div>\";", "<em>safe</em>", true])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const source = child.createElement('section'); const moved = child.createElement('b'); source.appendChild(moved); child.body.appendChild(source); const fragment = child.createDocumentFragment(); fragment.appendChild(moved); const stable = child.createElement('i'); fragment.appendChild(stable); const records = []; const observer = new MutationObserver(entries => records.push(entries.map(record => ({ type: record.type, target: record.target === fragment, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName), previous: record.previousSibling && (record.previousSibling.localName || record.previousSibling.nodeName), next: record.nextSibling && (record.nextSibling.localName || record.nextSibling.nodeName) })))); observer.observe(fragment, { childList: true, subtree: true }); const inserted = child.createElement('em'); fragment.insertBefore(inserted, stable); stable.remove(); globalThis.fragmentMutationLog = records; return [moved.parentNode === fragment, source.childNodes.length, fragment.childNodes.length]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!([true, 0, 2])
    );
    assert_eq!(
        session
            .script("globalThis.fragmentMutationLog")
            .await
            .unwrap()
            .value,
        serde_json::json!([[
            {
                "type": "childList",
                "target": true,
                "added": ["em"],
                "removed": [],
                "previous": "b",
                "next": "i",
            },
            {
                "type": "childList",
                "target": true,
                "added": [],
                "removed": ["i"],
                "previous": "em",
                "next": null,
            },
        ]])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const reflected = child.createElement('label'); reflected.id = 'frame-property-id'; reflected.className = 'frame alpha'; child.body.appendChild(reflected); const assigned = [reflected.id, reflected.className, reflected.getAttribute('id'), reflected.getAttribute('class'), child.getElementById('frame-property-id') === reflected, child.querySelector('.alpha') === reflected]; reflected.setAttribute('id', 'frame-attribute-id'); reflected.removeAttribute('class'); reflected.title = 'Frame title'; reflected.name = 'frame-field'; reflected.required = true; reflected.readOnly = true; reflected.hidden = true; reflected.disabled = true; reflected.multiple = true; reflected.type = 'BUTTON'; const form = [reflected.title, reflected.name, reflected.required, reflected.readOnly, reflected.hidden, reflected.disabled, reflected.multiple, reflected.type, reflected.getAttribute('title'), reflected.getAttribute('name'), reflected.getAttribute('required'), reflected.getAttribute('readonly'), reflected.getAttribute('hidden'), reflected.getAttribute('disabled'), reflected.getAttribute('multiple'), reflected.getAttribute('type')]; reflected.required = false; reflected.readOnly = false; reflected.hidden = false; reflected.disabled = false; reflected.multiple = false; return { assigned, method: [reflected.id, reflected.className, reflected.getAttribute('id'), reflected.getAttribute('class'), reflected.hasAttribute('class'), child.getElementById('frame-attribute-id') === reflected], form, cleared: [reflected.required, reflected.readOnly, reflected.hidden, reflected.disabled, reflected.multiple, reflected.hasAttribute('required'), reflected.hasAttribute('readonly'), reflected.hasAttribute('hidden'), reflected.hasAttribute('disabled'), reflected.hasAttribute('multiple')] }; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!({
            "assigned": ["frame-property-id", "frame alpha", "frame-property-id", "frame alpha", true, true],
            "method": ["frame-attribute-id", "", "frame-attribute-id", null, false, true],
            "form": ["Frame title", "frame-field", true, true, true, true, true, "button", "Frame title", "frame-field", "", "", "", "", "", "BUTTON"],
            "cleared": [false, false, false, false, false, false, false, false, false, false],
        })
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const reflected = child.getElementById('frame-attribute-id'); return [reflected.id, reflected.className, reflected.getAttribute('id'), reflected.getAttribute('class'), child.querySelector('#frame-attribute-id') === reflected, reflected.title, reflected.name, reflected.required, reflected.readOnly, reflected.hidden, reflected.disabled, reflected.multiple, reflected.type]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["frame-attribute-id", "", "frame-attribute-id", null, true, "Frame title", "frame-field", false, false, false, false, false, "button"])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const target = child.createElement('article'); child.body.appendChild(target); const log = []; const observer = new MutationObserver(records => log.push(records.map(record => ({ type: record.type, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName) })))); observer.observe(target, { childList: true }); target.innerHTML = '<div id=inner><span>Hi &amp; there</span></div>tail'; globalThis.frameElementInnerHtmlMutationLog = log; return [target.innerHTML, target.textContent, target.querySelector('#inner') !== null, target.querySelector('span').textContent, child.getElementById('inner') !== null, target.childNodes.length]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["<div id=\"inner\"><span>Hi &amp; there</span></div>tail", "Hi & theretail", true, "Hi & there", true, 2])
    );
    assert_eq!(
        session
            .script("globalThis.frameElementInnerHtmlMutationLog")
            .await
            .unwrap()
            .value,
        serde_json::json!([[
            {
                "type": "childList",
                "added": ["div", "#text"],
                "removed": [],
            }
        ]])
    );
    assert_eq!(
        session
            .script(
                "(() => { const child = document.getElementById('child').contentDocument; const target = child.createElement('p'); child.body.appendChild(target); const log = []; const observer = new MutationObserver(records => log.push(records.map(record => ({ type: record.type, added: record.addedNodes.map(node => node.localName || node.nodeName), removed: record.removedNodes.map(node => node.localName || node.nodeName) })))); observer.observe(target, { childList: true }); target.textContent = 'frame text'; globalThis.frameElementTextContentMutationLog = log; return [target.textContent, target.firstChild.nodeType, target.firstChild.nodeValue, target.childNodes.length, target.firstChild.parentNode === target]; })()",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(["frame text", 3, "frame text", 1, true])
    );
    assert_eq!(
        session
            .script("globalThis.frameElementTextContentMutationLog")
            .await
            .unwrap()
            .value,
        serde_json::json!([[
            {
                "type": "childList",
                "added": ["#text"],
                "removed": [],
            }
        ]])
    );
    let child_id = session
        .native_list_frames()
        .await
        .unwrap()
        .into_iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .unwrap()
        .id;
    session
        .script(
            "(() => { const frame = document.getElementById('child'); const child = frame.contentDocument; const inside = child.getElementById('trigger'); const field = child.getElementById('field'); globalThis.frameEventLog = []; globalThis.frameLifecycleLog = []; const record = label => event => globalThis.frameEventLog.push([label, event.type, event.target && event.target.nodeType || 0, event.currentTarget && event.currentTarget.nodeType || 0, event.eventPhase]); const recordLifecycle = event => globalThis.frameLifecycleLog.push([event.type, event.target === child.defaultView, event.currentTarget === child.defaultView, event.eventPhase]); inside.addEventListener('focus', record('inside-focus')); inside.addEventListener('click', record('inside-click')); field.addEventListener('focus', record('field-focus')); child.body.addEventListener('click', record('body-capture'), true); child.body.addEventListener('click', record('body')); child.addEventListener('click', record('document')); child.defaultView.addEventListener('click', record('window')); child.defaultView.addEventListener('pagehide', recordLifecycle); child.defaultView.addEventListener('unload', recordLifecycle); child.defaultView.addEventListener('load', recordLifecycle); child.defaultView.addEventListener('pageshow', recordLifecycle); document.addEventListener('click', () => globalThis.frameEventLog.push(['top-document'])); window.addEventListener('click', () => globalThis.frameEventLog.push(['top-window'])); return true; })()",
        )
        .await
        .unwrap();
    session.native_select_frame(&child_id).await.unwrap();
    session
        .script(
            "document.getElementById('trigger').click(); window.parent.document.getElementById('child').setAttribute('data-from-child', 'yes'); true",
        )
        .await
        .unwrap();
    session
        .native_select_frame("native-context:main")
        .await
        .unwrap();
    assert_eq!(
        session
            .script("globalThis.frameEventLog")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            ["inside-focus", "focus", 1, 1, 2],
            ["body-capture", "click", 1, 1, 1],
            ["inside-click", "click", 1, 1, 2],
            ["body", "click", 1, 1, 3],
            ["document", "click", 1, 9, 3],
            ["window", "click", 1, 0, 3],
        ])
    );
    assert_eq!(
        session
            .script("document.getElementById('child').getAttribute('data-from-child')")
            .await
            .unwrap()
            .value,
        serde_json::json!("yes")
    );
    session
        .script(
            "(() => { const frame = document.getElementById('child'); frame.contentWindow.postMessage({ ok: true }, frame.contentWindow.location.origin); return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.getElementById('child').contentDocument.getElementById('inside').getAttribute('data-message')")
            .await
            .unwrap()
            .value,
        serde_json::json!("received")
    );
    assert_eq!(
        session
            .script("globalThis.frameEventLog")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            ["inside-focus", "focus", 1, 1, 2],
            ["body-capture", "click", 1, 1, 1],
            ["inside-click", "click", 1, 1, 2],
            ["body", "click", 1, 1, 3],
            ["document", "click", 1, 9, 3],
            ["window", "click", 1, 0, 3],
            ["field-focus", "focus", 1, 1, 2],
        ])
    );
    session
        .script(
            "document.getElementById('child').contentWindow.location.assign('/child-next'); true",
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script("(() => { const frame = document.getElementById('child'); return [frame.contentWindow.location.pathname, frame.contentDocument.body.textContent]; })()")
            .await
            .unwrap()
            .value,
        serde_json::json!(["/child-next", "navigated frame"])
    );
    assert_eq!(
        session
            .script("globalThis.frameLifecycleLog")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            ["pagehide", true, true, 2],
            ["unload", true, true, 2],
            ["load", true, true, 2],
            ["pageshow", true, true, 2],
        ])
    );
    session
        .script(
            "(() => { const child = document.getElementById('child').contentDocument; child.getElementById('inside').innerHTML = 'x'.repeat(16300); return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script(
                "document.getElementById('child').contentDocument.getElementById('inside').innerHTML.length",
            )
            .await
            .unwrap()
            .value,
        serde_json::json!(16300)
    );
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_nested_frame_script_projection_preserves_window_chain() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_nested_frame_script_projection_preserves_window_chain_inner());
    });
}

async fn native_nested_frame_script_projection_preserves_window_chain_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let body = match path {
                "/parent" => "<html><body><iframe id='child' src='/child'></iframe></body></html>",
                "/child" => {
                    "<html><body><iframe id='grand' src='/grand'></iframe><p id='child-label'>child</p></body></html>"
                }
                "/grand" => {
                    "<html><head><script>addEventListener('message', event => document.getElementById('grand-label').setAttribute('data-message', event.data.ok ? 'received' : 'bad'));</script></head><body><p id='grand-label' data-message='none'>grand</p></body></html>"
                }
                "/grand-next" => "<html><body><p id='next'>next grandchild</p></body></html>",
                other => panic!("unexpected nested frame request path: {other}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let frames = session.native_list_frames().await.unwrap();
    assert_eq!(frames.len(), 3);
    let child_id = frames
        .iter()
        .find(|frame| frame.parent_id.as_deref() == Some("native-context:main"))
        .unwrap()
        .id
        .clone();
    let identity = session
        .script(
            "(() => { const child = document.getElementById('child'); const childWindow = child.contentWindow; const childDocument = child.contentDocument; const grand = childDocument.getElementById('grand'); const grandWindow = grand.contentWindow; const grandDocument = grand.contentDocument; return { childWindow: childWindow instanceof Window, childFrames: childWindow.frames === childWindow && childWindow.length === 1 && childWindow.frames[0] === grandWindow, chain: grandWindow.parent === childWindow && grandWindow.top === window, document: grandWindow.document === grandDocument && grandDocument.defaultView === grandWindow, frameElement: grandWindow.frameElement === grand, content: grandDocument.body.textContent }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        identity.value,
        serde_json::json!({
            "childWindow": true,
            "childFrames": true,
            "chain": true,
            "document": true,
            "frameElement": true,
            "content": "grand",
        })
    );
    session
        .script(
            "(() => { const grand = document.getElementById('child').contentDocument.getElementById('grand'); globalThis.__grandWindow = grand.contentWindow; globalThis.grandLifecycle = []; const record = event => grandLifecycle.push([event.type, event.target === grand.contentWindow, event.currentTarget === grand.contentWindow]); grand.contentWindow.addEventListener('pagehide', record); grand.contentWindow.addEventListener('unload', record); grand.contentWindow.addEventListener('pageshow', record); grand.contentWindow.postMessage({ ok: true }, grand.contentWindow.location.origin); return true; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        session
            .script("document.getElementById('child').contentDocument.getElementById('grand').contentDocument.getElementById('grand-label').getAttribute('data-message')")
            .await
            .unwrap()
            .value,
        serde_json::json!("received")
    );
    session
        .script(
            "document.getElementById('child').contentDocument.getElementById('grand').contentWindow.location.replace('/grand-next'); true",
        )
        .await
        .unwrap();
    let replacement = session
        .script("(() => { const grand = document.getElementById('child').contentDocument.getElementById('grand'); return [globalThis.__grandWindow === grand.contentWindow, grand.contentWindow.location.pathname, grand.contentDocument.body.textContent]; })()")
        .await
        .unwrap();
    assert_eq!(
        replacement.value,
        serde_json::json!([true, "/grand-next", "next grandchild"])
    );
    assert_eq!(
        session
            .script("globalThis.grandLifecycle")
            .await
            .unwrap()
            .value,
        serde_json::json!([
            ["pagehide", true, true],
            ["unload", true, true],
            ["pageshow", true, true],
        ])
    );
    session.native_select_frame(&child_id).await.unwrap();
    let selected_child = session
        .script("(() => ({ parent: window.parent !== window, top: window.top === window.parent && window.top !== window, frameElement: window.frameElement && window.frameElement.id === 'child' && window.frameElement.tagName === 'IFRAME', parentDocument: window.parent.document instanceof Document && window.parent.document.defaultView === window.parent, parentFrame: window.parent.frames[0] === window && window.parent.document.getElementById('child').contentWindow === window, nested: window.frames.length === 1 && window.frames[0].parent === window && window.frames[0].top === window.top }))()")
        .await
        .unwrap();
    assert_eq!(
        selected_child.value,
        serde_json::json!({
            "parent": true,
            "top": true,
            "frameElement": true,
            "parentDocument": true,
            "parentFrame": true,
            "nested": true,
        })
    );
    session.close().await.unwrap();
    server.await.unwrap();
}
#[test]
fn native_cross_origin_parent_security_and_cookie_authority() {
    std::thread::Builder::new()
        .name("native-cross-origin-frame-cookie-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cross-origin frame cookie test runtime should build");
            runtime.block_on(native_cross_origin_parent_security_and_cookie_authority_inner());
        })
        .expect("cross-origin frame cookie test thread should start")
        .join()
        .expect("cross-origin frame cookie test thread should finish");
}
#[test]
fn native_nested_form_top_target_navigates_parent_context() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_nested_form_top_target_navigates_parent_context_inner());
    });
}

async fn native_nested_form_top_target_navigates_parent_context_inner() {
    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default()
            .with_fixture(
                "fixture://form-top-parent",
                "<title>Parent</title><iframe src='fixture://form-top-child'></iframe>",
            )
            .unwrap()
            .with_fixture(
                "fixture://form-top-child",
                native_html_without_body_margin("<title>Child</title><form action='fixture://form-top-result' method='post' target='_top'><input name='query' value='hello'><button id='go' type='submit'>Navigate top</button></form>"),
            )
            .unwrap()
            .with_fixture(
                "fixture://form-top-result",
                "<title>Top result</title><p>parent navigated</p>",
            )
            .unwrap()
            .with_initial_url("fixture://form-top-parent"),
    )
    .await
    .unwrap();
    assert_eq!(session.native_list_frames().await.unwrap().len(), 2);
    session
        .action(SemanticAction::Click {
            target: "id=go".into(),
        })
        .await
        .unwrap();
    let evidence = session.evidence(EvidenceLevel::Compact).await.unwrap();
    assert_eq!(evidence.url, "fixture://form-top-result");
    assert_eq!(evidence.title, "Top result");
    assert_eq!(session.native_list_frames().await.unwrap().len(), 1);
    session.close().await.unwrap();
}
#[test]
fn native_post_message_round_trip_uses_window_proxy_and_origin_filter() {
    run_native_browser_worker_test(|runtime| {
        runtime
            .block_on(native_post_message_round_trip_uses_window_proxy_and_origin_filter_inner());
    });
}

async fn native_post_message_round_trip_uses_window_proxy_and_origin_filter_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://message-parent",
            "<title>Message parent</title><script>addEventListener('message', event => { globalThis.reply = event.data.reply; }); globalThis.popup = window.open('fixture://message-child', 'message-child'); globalThis.popup.postMessage({ kind: 'greeting', count: 2 }, '*');</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://message-child",
            "<script>addEventListener('message', event => { globalThis.received = { kind: event.data.kind, count: event.data.count, origin: event.origin, hasSource: Boolean(event.source) }; event.source.postMessage({ reply: event.data.kind }, '*'); });</script><title>Message child</title><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://message-parent");
    let session = BrowserRuntimeSession::connect_native(config).await.unwrap();

    let targets = session.native_list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    let child = targets.iter().find(|target| !target.active).unwrap();
    session.native_select_target(&child.id).await.unwrap();
    let child_message = session.script("globalThis.received").await.unwrap();
    assert_eq!(
        child_message.value,
        serde_json::json!({
            "kind": "greeting",
            "count": 2,
            "origin": "null",
            "hasSource": true,
        })
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    session
        .script("popup.postMessage({ kind: 'blocked' }, 'https://blocked.test'); popup.postMessage({ kind: 'allowed' }, '*'); true")
        .await
        .unwrap();
    session.native_select_target(&child.id).await.unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!({
            "kind": "allowed",
            "origin": "null",
            "hasSource": true,
        })
    );
    session
        .native_select_target("native-context")
        .await
        .unwrap();
    let parent_reply = session.script("globalThis.reply || null").await.unwrap();
    assert_eq!(parent_reply.value, serde_json::json!("allowed"));
    session.close().await.unwrap();
}
#[test]
fn native_post_message_transfers_blob_urls_between_window_realms() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_post_message_transfers_blob_urls_between_window_realms_inner());
    });
}

async fn native_post_message_transfers_blob_urls_between_window_realms_inner() {
    let config = NativeEngineConfig::default()
        .with_fixture(
            "fixture://blob-message-parent",
            "<title>Blob message parent</title><script>globalThis.received = []; addEventListener('message', event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); received.push([event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]); } catch (error) { received.push(['error', String(error)]); } }); globalThis.popup = window.open('fixture://blob-message-child', 'blob-message-child'); const blob = new Blob(['parent to child'], { type: 'text/parent' }); popup.postMessage({ kind: 'parent', url: URL.createObjectURL(blob) }, '*');</script><p>parent</p>",
        )
        .unwrap()
        .with_fixture(
            "fixture://blob-message-child",
            "<title>Blob message child</title><script>globalThis.received = null; addEventListener('message', event => { try { const xhr = new XMLHttpRequest(); xhr.open('GET', event.data.url, false); xhr.send(); globalThis.received = [event.data.kind, xhr.responseText, xhr.getResponseHeader('content-type'), event.origin]; const blob = new Blob(['child to parent'], { type: 'text/child' }); event.source.postMessage({ kind: 'child', url: URL.createObjectURL(blob) }, '*'); } catch (error) { globalThis.received = ['error', String(error)]; } });</script><p>child</p>",
        )
        .unwrap()
        .with_initial_url("fixture://blob-message-parent");
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
        serde_json::json!(["parent", "parent to child", "text/parent", "null"])
    );

    session
        .native_select_target("native-context")
        .await
        .unwrap();
    assert_eq!(
        session.script("globalThis.received").await.unwrap().value,
        serde_json::json!([["child", "child to parent", "text/child", "null"]])
    );
    session.close().await.unwrap();
}
#[tokio::test]
async fn native_initial_page_script_scrolls_and_delivers_window_event() {
    let config = NativeEngineConfig::default()
        .with_viewport(Viewport {
            width: 80,
            height: 24,
            device_scale_factor_milli: 1000,
        })
        .with_fixture(
            "fixture://initial-scroll",
            "<html><body><script>globalThis.initialScrollEvents = []; window.addEventListener('scroll', event => globalThis.initialScrollEvents.push([event.target === window, event.bubbles])); window.scrollTo({ top: 40 });</script><div style='height:220px'>Content</div></body></html>",
        )
        .unwrap()
        .with_initial_url("fixture://initial-scroll");
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let state = engine
        .evaluate_async(
            "[window.scrollY, document.documentElement.scrollTop, globalThis.initialScrollEvents]",
        )
        .await
        .unwrap();
    assert_eq!(state[0], serde_json::json!(40));
    assert_eq!(state[1], serde_json::json!(40));
    assert_eq!(state[2], serde_json::json!([[true, false]]));

    engine.close_async().await.unwrap();
}
#[tokio::test]
async fn native_host_fetch_service_worker_nested_cookie_uses_parent_authority() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for _ in 0..4 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("host Fetch and nested ServiceWorker request should reach the server")
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
                        "Set-Cookie: host_session=initial; Path=/; SameSite=Lax\r\n",
                        "Set-Cookie: host_secret=initial-secret; HttpOnly; Path=/; SameSite=Lax\r\n",
                    ),
                    "text/html",
                    "<!doctype html><script>globalThis.registrationPromise = navigator.serviceWorker.register('/sw.js', { scope: '/' });</script><main>host Fetch</main>",
                ),
                "/sw.js" => (
                    "",
                    "application/javascript",
                    r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => {
  if (new URL(event.request.url).pathname === '/api') {
    event.respondWith((async () => {
      const response = await fetch('/nested', { credentials: 'include' });
      return new Response(await response.text());
    })());
  }
});"#,
                ),
                "/nested" => (
                    "Set-Cookie: nested_worker=parent-owned; HttpOnly; Path=/; SameSite=Lax\r\n",
                    "text/plain",
                    "nested response",
                ),
                "/after" => ("", "text/plain", "after"),
                "/api" => ("", "text/plain", "host Fetch bypassed ServiceWorker"),
                other => panic!("unexpected host ServiceWorker Fetch path: {other}"),
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
    assert_eq!(
        engine
            .evaluate_async(
                "await registrationPromise.then(reg => [reg.active.state, navigator.serviceWorker.controller !== null])",
            )
            .await
            .unwrap(),
        serde_json::json!(["activated", true])
    );
    let response = engine
        .fetch_async(format!("http://{address}/api"), true)
        .await
        .unwrap();
    assert_eq!(response.body, b"nested response");
    assert_eq!(
        engine.evaluate_async("document.cookie").await.unwrap(),
        "host_session=initial"
    );
    assert!(engine.cookies_async().await.unwrap().iter().any(|cookie| {
        cookie.name == "nested_worker" && cookie.value == "parent-owned" && cookie.http_only
    }));
    assert_eq!(
        engine
            .fetch_async(format!("http://{address}/after"), true)
            .await
            .unwrap()
            .body,
        b"after"
    );
    engine.close_async().await.unwrap();
    let requests = server.await.unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/page", "/sw.js", "/nested", "/after"]
    );
    let nested_cookie = requests[2].1.as_deref().unwrap_or_default();
    assert!(nested_cookie.contains("host_session=initial"));
    assert!(nested_cookie.contains("host_secret=initial-secret"));
    let after_cookie = requests[3].1.as_deref().unwrap_or_default();
    assert!(after_cookie.contains("nested_worker=parent-owned"));
}
#[test]
fn native_nested_opacity_and_zero_opacity_keep_geometry_but_change_pixels() {
    let document = NativeDocument::parse(
        "<div id='zero' style='display:block;width:12px;height:12px;background-color:red;opacity:0'></div><div id='contents' style='display:contents;opacity:50%'><div id='child' style='display:block;width:12px;height:12px;background-color:blue;opacity:50%'></div></div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 24,
        height: 32,
        device_scale_factor_milli: 1000,
    };
    let zero = document.resolve_target("id=zero").unwrap();
    let contents = document.resolve_target("id=contents").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let layout = document.layout(viewport).unwrap();
    assert_eq!(layout.box_for(zero).unwrap().y, 0);
    assert_eq!(layout.box_for(child).unwrap().y, 12);
    assert_eq!(layout.hit_test(1, 1).unwrap(), Some(zero));
    assert_eq!(layout.hit_test(1, 13).unwrap(), Some(child));

    let list = document.display_list(viewport).unwrap();
    let groups = list
        .commands
        .iter()
        .filter_map(|command| match command {
            NativeDisplayCommand::BeginOpacityGroup { node_id, opacity } => {
                Some((*node_id, *opacity))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(groups, vec![(zero, 0), (contents, 128), (child, 128)]);

    let surface = list.rasterize().unwrap();
    assert_eq!(surface.pixel(1, 1), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(20, 20), Some([255, 255, 255, 255]));
    assert_eq!(surface.pixel(5, 13), Some([191, 191, 255, 255]));
}
#[test]
fn native_physical_box_edges_feed_content_origins_and_flow_margins() {
    let document = NativeDocument::parse(
        "<div id='outer' style='width:20px;height:10px;padding:1px 2px 3px 4px'><div id='child' style='display:block;width:4px;height:4px;margin:1px 2px 3px 4px'>A</div></div><div id='next' style='display:block;width:8px;height:4px;margin:2px 3px 4px 5px'>B</div>",
        &NativeEngineLimits::default(),
    )
    .unwrap();
    let viewport = Viewport {
        width: 64,
        height: 64,
        device_scale_factor_milli: 1000,
    };
    let outer = document.resolve_target("id=outer").unwrap();
    let child = document.resolve_target("id=child").unwrap();
    let next = document.resolve_target("id=next").unwrap();
    let layout = document.layout(viewport).unwrap();

    assert_eq!(
        layout.box_for(outer),
        Some(NativeRect {
            x: 0,
            y: 0,
            width: 26,
            height: 14,
        })
    );
    assert_eq!(
        layout
            .boxes
            .iter()
            .find(|layout_box| layout_box.node_id == outer)
            .unwrap()
            .content_rect,
        NativeRect {
            x: 4,
            y: 1,
            width: 20,
            height: 10,
        }
    );
    assert_eq!(
        layout.box_for(child),
        Some(NativeRect {
            x: 8,
            y: 2,
            width: 4,
            height: 4,
        })
    );
    assert_eq!(
        layout.box_for(next),
        Some(NativeRect {
            x: 5,
            y: 16,
            width: 8,
            height: 4,
        })
    );
    assert_eq!(
        layout
            .text_runs
            .iter()
            .find(|run| run.node_id == child)
            .map(|run| (run.origin, run.text.as_str())),
        Some((NativePoint { x: 8, y: 2 }, "A"))
    );
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
#[test]
fn native_same_origin_frame_template_content_uses_native_fragments() {
    run_native_browser_worker_test(|runtime| {
        runtime.block_on(native_same_origin_frame_template_content_uses_native_fragments_inner());
    });
}

async fn native_same_origin_frame_template_content_uses_native_fragments_inner() {
    let _guard = native_content_process_test_lock().lock().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
                .await
                .expect("native frame template request")
                .unwrap();
            let request = read_http_request(&mut stream).await;
            let body = match request.split_whitespace().nth(1) {
                Some("/parent") => {
                    "<html><body><iframe id='child' src='/child'></iframe></body></html>"
                }
                Some("/child") => {
                    "<!doctype html><template id='template'><span id='inside'>inert</span><template id='nested'><b>nested</b></template></template><p id='outside'>visible</p>"
                }
                other => panic!("unexpected same-origin frame request path: {other:?}"),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let session = BrowserRuntimeSession::connect_native(
        NativeEngineConfig::default().with_initial_url(format!("http://{address}/parent")),
    )
    .await
    .unwrap();
    let initial = session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; const template = childDocument.getElementById('template'); const content = template.content; const inside = content.querySelector('#inside'); const nested = content.querySelector('#nested'); globalThis.__frameTemplate = template; globalThis.__frameTemplateContent = content; globalThis.__frameTemplateOwner = content.ownerDocument; return { document: childDocument instanceof Document, template: template instanceof HTMLTemplateElement, fragment: content instanceof DocumentFragment && content.nodeType === 11, stable: content === template.content, detached: content.parentNode === null, owner: content.ownerDocument instanceof Document && content.ownerDocument !== childDocument && content.ownerDocument.documentElement === null, childOwner: inside.ownerDocument === content.ownerDocument, nestedOwner: nested.content.ownerDocument === content.ownerDocument, root: inside.getRootNode() === content && inside.parentNode === content, templateBoundary: template.childNodes.length === 0 && template.querySelector('#inside') === null, documentBoundary: childDocument.querySelector('#inside') === null, fragmentQuery: content.querySelector('#inside') === inside, templateHTML: template.innerHTML.includes('id=\"inside\"') }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        initial.value,
        serde_json::json!({
            "document": true,
            "template": true,
            "fragment": true,
            "stable": true,
            "detached": true,
            "owner": true,
            "childOwner": true,
            "nestedOwner": true,
            "root": true,
            "templateBoundary": true,
            "documentBoundary": true,
            "fragmentQuery": true,
            "templateHTML": true,
        })
    );

    let clones = session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; const source = childDocument.getElementById('template'); const sourceContent = source.content; const original = sourceContent.querySelector('#inside'); const nested = sourceContent.querySelector('#nested'); const shallow = source.cloneNode(false); const deep = source.cloneNode(true); const shallowContent = shallow.content; const deepContent = deep.content; const copiedInside = deepContent.querySelector('#inside'); const copiedNested = deepContent.querySelector('#nested'); const copiedNestedChild = copiedNested.content.querySelector('b'); const result = { shallowDistinct: shallowContent !== sourceContent, shallowStable: shallowContent === shallow.content, shallowEmpty: shallowContent.childNodes.length === 0, shallowOwner: shallowContent.ownerDocument === sourceContent.ownerDocument, deepDistinct: deepContent !== sourceContent, deepFragmentOwner: deepContent.ownerDocument === sourceContent.ownerDocument, deepChildDistinct: copiedInside !== original, deepChildParent: copiedInside.parentNode === deepContent, deepOwner: copiedInside.ownerDocument === sourceContent.ownerDocument, nestedDistinct: copiedNested !== nested && copiedNested.content !== nested.content, nestedChildDistinct: copiedNestedChild !== nested.content.querySelector('b'), nestedChildOwner: copiedNestedChild.ownerDocument === sourceContent.ownerDocument, templateBoundary: deep.childNodes.length === 0 && deep.querySelector('#inside') === null, nestedBoundary: deepContent.querySelector('b') === null, sourceUnchanged: original.textContent === 'inert' }; shallow.id = 'frameShallowClone'; deep.id = 'frameDeepClone'; childDocument.body.appendChild(deep); childDocument.body.appendChild(shallow); return result; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        clones.value,
        serde_json::json!({
            "shallowDistinct": true,
            "shallowStable": true,
            "shallowEmpty": true,
            "shallowOwner": true,
            "deepDistinct": true,
            "deepFragmentOwner": true,
            "deepChildDistinct": true,
            "deepChildParent": true,
            "deepOwner": true,
            "nestedDistinct": true,
            "nestedChildDistinct": true,
            "nestedChildOwner": true,
            "templateBoundary": true,
            "nestedBoundary": true,
            "sourceUnchanged": true,
        })
    );

    let persisted_clones = session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; const source = childDocument.getElementById('template'); const deep = childDocument.getElementById('frameDeepClone'); const shallow = childDocument.getElementById('frameShallowClone'); return { deepContent: deep.content.querySelector('#inside')?.textContent, deepNested: deep.content.querySelector('#nested')?.content.querySelector('b')?.textContent, shallowEmpty: shallow.content.childNodes.length === 0, sourceUnchanged: source.content.querySelector('#inside')?.textContent === 'inert', distinctFragment: deep.content !== source.content, inertOwner: deep.content.ownerDocument === source.content.ownerDocument }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        persisted_clones.value,
        serde_json::json!({
            "deepContent": "inert",
            "deepNested": "nested",
            "shallowEmpty": true,
            "sourceUnchanged": true,
            "distinctFragment": true,
            "inertOwner": true,
        })
    );

    let mutations = session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; const template = childDocument.getElementById('template'); const content = template.content; const inside = content.querySelector('#inside'); const added = childDocument.createElement('b'); added.id = 'added'; added.textContent = 'added'; content.appendChild(added); childDocument.body.appendChild(inside); globalThis.__frameTemplateMoved = inside; return { added: content.querySelector('#added') === added, moved: inside.parentNode === childDocument.body, ownerPreserved: inside.ownerDocument === globalThis.__frameTemplateOwner, fragmentStable: template.content === globalThis.__frameTemplateContent }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        mutations.value,
        serde_json::json!({
            "added": true,
            "moved": true,
            "ownerPreserved": true,
            "fragmentStable": true,
        })
    );

    let persisted = session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; const template = childDocument.getElementById('template'); return { sameDocumentOwner: template.content.ownerDocument === globalThis.__frameTemplateOwner, sameFragment: template.content === globalThis.__frameTemplateContent, added: template.content.querySelector('#added')?.textContent, moved: childDocument.querySelector('#inside')?.parentNode === childDocument.body }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        persisted.value,
        serde_json::json!({
            "sameDocumentOwner": true,
            "sameFragment": true,
            "added": "added",
            "moved": true,
        })
    );

    session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; childDocument.getElementById('template').innerHTML = '<em id=\"replacement\">new</em>'; return true; })()",
        )
        .await
        .unwrap();
    let replaced = session
        .script(
            "(() => { const childDocument = document.getElementById('child').contentDocument; const template = childDocument.getElementById('template'); return { sameFragment: template.content === globalThis.__frameTemplateContent, replacement: template.content.querySelector('#replacement')?.textContent, templateBoundary: template.querySelector('#replacement') === null, documentBoundary: childDocument.querySelector('#replacement') === null, moved: childDocument.querySelector('#inside')?.parentNode === childDocument.body }; })()",
        )
        .await
        .unwrap();
    assert_eq!(
        replaced.value,
        serde_json::json!({
            "sameFragment": true,
            "replacement": "new",
            "templateBoundary": true,
            "documentBoundary": true,
            "moved": true,
        })
    );

    session.close().await.unwrap();
    server.await.unwrap();
}
